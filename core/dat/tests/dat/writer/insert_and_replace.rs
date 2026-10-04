//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The DAT writer: records reopen byte-exact, owner dats are refused, created containers open,
//! replace/free/grow follow the retail allocator, the header changes only native dwords, the root
//! splits 30/1/30, 1,000 entries stay sound, iteration handling, interrupted writes stay readable,
//! a pending transaction stops the writer, cell-sized nodes split.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::path::{Path, PathBuf};

use dereth_dat::write::{DatWriter, Fault, SaveOutcome};
use dereth_dat::DatFile;
use dereth_primitives::DataId;

// ---------------------------------------------------------------------------------------------
// Disposable copies, and the proof that the originals were not touched.
// ---------------------------------------------------------------------------------------------

/// FNV-1a over the whole file, with the length mixed in. Not a cryptographic hash and does not
/// need to be: it is a before/after equality check on a file this suite never opens for writing.
fn digest(path: &Path) -> (u64, u64) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    (bytes.len() as u64, h)
}

/// A scratch directory named for the test, removed when the guard drops.
struct Scratch {
    dir: PathBuf,
    /// `(path, digest)` for every retail file this test read.
    pristine: Vec<(PathBuf, (u64, u64))>,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("dereth_dat_insert_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        Self {
            dir,
            pristine: Vec::new(),
        }
    }

    /// Copy one retail dat in, remembering the original's digest.
    fn copy_retail(&mut self, file: &str) -> PathBuf {
        let src = dereth_dat::testing::dat_dir().join(file);
        assert!(
            src.is_file(),
            "PREFLIGHT: {} is not a file; set DERETH_TEST_DAT_DIR",
            src.display()
        );
        let before = digest(&src);
        let dst = self.dir.join(file);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
        self.pristine.push((src, before));
        dst
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// The owner check this suite can make for itself.
    fn assert_pristine(&self) {
        for (src, before) in &self.pristine {
            let after = digest(src);
            assert_eq!(
                *before,
                after,
                "the retail dat {} changed during the test: {before:?} -> {after:?}",
                src.display()
            );
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.assert_pristine();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A deterministic payload of `n` bytes, distinguishable from any other `seed`.
fn payload(seed: u8, n: usize) -> Vec<u8> {
    #[allow(clippy::cast_possible_truncation)]
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 1. The block-chain writer: a record goes in, the ordinary reader reads it back byte for byte.
// ---------------------------------------------------------------------------------------------

/// A brand new id, one block long, exactly one block long and then several blocks long, reopened
/// with the read-only `DatFile` the rest of the client uses. This is the point of the card: bytes
/// in, the same bytes out, through a reader that knows nothing about the writer.
#[test]
fn a_new_record_reopens_byte_exact_through_the_ordinary_reader() {
    let mut s = Scratch::new("new_record");
    let dat = s.copy_retail("client_local_English.dat");

    // `client_local_English.dat` has 0x400-byte blocks, so 1020 payload bytes per block.
    let small = payload(0x11, 40);
    let exact = payload(0x22, 1020);
    let long = payload(0x33, 1020 * 3 + 7);

    let (free_before, count_before) = {
        let r = DatFile::open(&dat).expect("open the copy");
        (r.header().free_count, r.len())
    };

    {
        let mut w = DatWriter::open(&dat).expect("open the copy for writing");
        w.save(DataId(0x4100_0001), &small, 3, 1000, 1_700_000_000)
            .expect("save small");
        w.save(DataId(0x4100_0002), &exact, 3, 1000, 1_700_000_000)
            .expect("save exact");
        w.save(DataId(0x4100_0003), &long, 3, 1000, 1_700_000_000)
            .expect("save long");
    }

    let r = DatFile::open(&dat).expect("reopen the copy");
    assert_eq!(r.read(DataId(0x4100_0001)).expect("small"), small);
    assert_eq!(r.read(DataId(0x4100_0002)).expect("exact"), exact);
    assert_eq!(r.read(DataId(0x4100_0003)).expect("long"), long);
    assert_eq!(r.len(), count_before + 3, "three ids added");

    let e = r.entry(DataId(0x4100_0003)).expect("the long entry");
    assert_eq!(e.version(), 3);
    assert!(!e.compressed());
    assert_eq!(e.reserved(), 0);
    assert_eq!(e.size as usize, long.len());
    assert_eq!(e.date, 1_700_000_000);
    assert_eq!(e.iteration, 1000);

    // The free list paid for exactly the blocks the chains use: 1 + 1 + 4.
    assert_eq!(
        r.header().free_count,
        free_before - 6,
        "chains cost 1 + 1 + 4 blocks"
    );

    // And the container is still structurally sound by the reader's own checker.
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.free_chain_len, report.header_free_count as usize);
}

/// The guard that does not depend on a test remembering to copy: the writer refuses a path inside
/// `DERETH_TEST_DAT_DIR` outright.
#[test]
fn the_writer_refuses_to_open_an_owner_dat() {
    let owner = dereth_dat::testing::dat_file(dereth_dat::RetailDat::Local);
    let before = digest(&owner);
    let err = DatWriter::open(&owner).expect_err("opening an owner dat for write must be refused");
    assert!(
        err.to_string().contains("retail"),
        "the refusal must say why: {err}"
    );
    assert_eq!(
        digest(&owner),
        before,
        "the refused open must not have touched the file"
    );
}

/// A minimal container built from nothing -- file creation plus an empty tree -- opened by the
/// ordinary reader. Without this the B-tree split
/// test below would have to fill a retail leaf, and the tests that need a tiny free list would
/// have to shrink a retail file.
#[test]
fn a_created_container_opens_with_the_ordinary_reader() {
    let s = Scratch::new("created");
    let dat = s.path("dereth_dat_insert_minimal.dat");

    {
        let mut w = DatWriter::create(&dat, 0x400, 1, 0, 64 * 1024).expect("create");
        // File creation lays 0x400-byte blocks from 0x400 to the file size, then tree creation
        // takes the first of them for the root node (2 blocks at this block size).
        assert_eq!(w.header().block_size, 0x400);
        assert_eq!(w.header().file_size, 64 * 1024);
        assert_eq!(w.header().btree_root, 0x400);
        assert_eq!(w.header().free_count, (64 * 1024 - 0x400) / 0x400 - 2);
        assert_eq!(w.header().free_head, 0x400 + 2 * 0x400);
        w.save(
            DataId(0x7000_0001),
            &payload(0x55, 100),
            2,
            1,
            1_700_000_001,
        )
        .expect("save");
    }

    let r = DatFile::open(&dat).expect("the reader opens what the writer created");
    assert_eq!(r.len(), 1);
    assert_eq!(
        r.read(DataId(0x7000_0001)).expect("read"),
        payload(0x55, 100)
    );
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
}

// ---------------------------------------------------------------------------------------------
// 2. Replacing an existing record: new chain, repointed entry, old chain to the free list's tail.
// ---------------------------------------------------------------------------------------------

/// Read the free chain of a file that is not open for writing, straight out of its bytes. The
/// tests use it to predict what native would have put in the header.
fn free_chain(raw: &[u8], head: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut cur = head;
    while cur != 0 {
        out.push(cur);
        let o = cur as usize;
        cur = u32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]) & 0x7FFF_FFFF;
    }
    out
}

fn header_bytes(raw: &[u8]) -> [u8; 0x50] {
    let mut h = [0u8; 0x50];
    h.copy_from_slice(&raw[0x140..0x190]);
    h
}

/// Longer, then shorter -- each one read back byte for byte, with the free-list accounting checked
/// against the blocks the chains actually use.
#[test]
fn replacing_a_record_with_a_longer_and_a_shorter_one_round_trips() {
    let mut s = Scratch::new("replace");
    let dat = s.copy_retail("client_local_English.dat");
    let id = DataId(0x4200_0001);

    let three = payload(0x01, 1020 * 3);
    let five = payload(0x02, 1020 * 4 + 1);
    let one = payload(0x03, 12);

    let free;
    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        free = w.header().free_count;
        w.save(id, &three, 2, 10, 1_700_000_000).expect("add");
        assert_eq!(w.header().free_count, free - 3, "three blocks off the head");
        assert!(w.audit().expect("audit").is_sound());
    }
    assert_eq!(
        DatFile::open(&dat).expect("open").read(id).expect("read"),
        three
    );

    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        assert_eq!(
            w.save(id, &five, 3, 11, 1_700_000_100)
                .expect("replace longer"),
            SaveOutcome::Replaced
        );
        // Five blocks off the head, three back onto the tail.
        assert_eq!(w.header().free_count, free - 3 - 5 + 3);
        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(
            audit.orphaned.is_empty(),
            "a completed replace leaks nothing: {audit:?}"
        );
    }
    let r = DatFile::open(&dat).expect("open");
    assert_eq!(r.read(id).expect("read"), five);
    assert_eq!(r.entry(id).expect("entry").version(), 3);
    assert_eq!(r.entry(id).expect("entry").iteration, 11);
    assert_eq!(r.entry(id).expect("entry").date, 1_700_000_100);
    drop(r);

    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        w.save(id, &one, 3, 12, 1_700_000_200)
            .expect("replace shorter");
        assert_eq!(w.header().free_count, free - 3 - 5 + 3 - 1 + 5);
        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(audit.orphaned.is_empty(), "{audit:?}");
    }
    let r = DatFile::open(&dat).expect("open");
    assert_eq!(r.read(id).expect("read"), one);
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// A freed chain goes to the tail and is reused last.
#[test]
fn a_freed_chain_goes_to_the_tail_and_is_reused_last() {
    let s = Scratch::new("free_list_order");
    let dat = s.path("dereth_dat_insert_reuse.dat");
    let id = DataId(0x4300_0001);

    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 100 * 0x400).expect("create");
    // Blocks 0x400 and 0x800 are the root node; the free list starts at 0xC00.
    let head_before = w.header().free_head;
    let tail_before = w.header().free_tail;
    assert_eq!(head_before, 0xC00);
    assert_eq!(tail_before, 0x400 + 100 * 0x400 - 0x400);

    // One block, taken off the head.
    w.save(id, &payload(0x10, 100), 2, 1, 1_700_000_000)
        .expect("add");
    assert_eq!(
        w.header().free_head,
        head_before + 0x400,
        "the head advanced by one block"
    );
    assert_eq!(w.header().free_tail, tail_before, "the tail did not move");

    // Replace it: a new block off the head, the old one onto the tail.
    w.save(id, &payload(0x11, 100), 2, 2, 1_700_000_001)
        .expect("replace");
    assert_eq!(w.header().free_head, head_before + 2 * 0x400);
    assert_eq!(
        w.header().free_tail,
        head_before,
        "the freed block is now the tail"
    );

    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.orphaned.is_empty(), "{audit:?}");

    // The next allocation comes off the head, not off the block just freed.
    w.save(
        DataId(0x4300_0002),
        &payload(0x12, 100),
        2,
        1,
        1_700_000_002,
    )
    .expect("another");
    let taken = w
        .entry(DataId(0x4300_0002))
        .expect("lookup")
        .expect("the new entry")
        .offset;
    assert_eq!(
        taken,
        head_before + 2 * 0x400,
        "the head block, not the recycled one"
    );
    assert_ne!(taken, head_before);
    drop(w);

    // And the recycled block really is last in the chain, exactly once.
    let raw = std::fs::read(&dat).expect("read the file back");
    let header = header_bytes(&raw);
    let head = u32::from_le_bytes([header[0x14], header[0x15], header[0x16], header[0x17]]);
    let chain = free_chain(&raw, head);
    assert_eq!(*chain.last().expect("a non-empty free chain"), head_before);
    assert_eq!(
        chain.iter().filter(|b| **b == head_before).count(),
        1,
        "once, not twice"
    );
    let count = u32::from_le_bytes([header[0x1C], header[0x1D], header[0x1E], header[0x1F]]);
    assert_eq!(
        chain.len(),
        count as usize,
        "the free-block count still counts the chain"
    );
}

/// The file grows by a megabyte when check room says so.
#[test]
fn the_file_grows_by_a_megabyte_when_check_room_says_so() {
    let s = Scratch::new("expand");
    let dat = s.path("dereth_dat_insert_expand.dat");
    // 40 blocks is below the room check's 0x33 reserve, so the very first save has to expand.
    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 40 * 0x400).expect("create");
    let size_before = w.header().file_size;
    let free_before = w.header().free_count;
    assert_eq!(free_before, 38, "40 blocks less the two the root node took");

    w.save(
        DataId(0x4600_0001),
        &payload(0x44, 100),
        2,
        1,
        1_700_000_000,
    )
    .expect("save");

    assert_eq!(
        w.header().file_size,
        size_before + 0x0010_0000,
        "one megabyte appended"
    );
    // 1024 new blocks, less the one the record took.
    assert_eq!(w.header().free_count, free_before + 1024 - 1);
    assert_eq!(
        w.header().free_tail,
        w.header().file_size - 0x400,
        "the last new block"
    );
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.orphaned.is_empty(), "{audit:?}");
    drop(w);

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(
        r.read(DataId(0x4600_0001)).expect("read"),
        payload(0x44, 100)
    );
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.free_chain_len, report.header_free_count as usize);
    assert_eq!(
        std::fs::metadata(&dat).expect("metadata").len(),
        u64::from(size_before) + 0x0010_0000,
        "the header's file size is the real size on disk, as it is in all four retail files"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The header, byte for byte.
// ---------------------------------------------------------------------------------------------

/// The five dwords can change, and the 60 other bytes it must not.
///
/// The padding after `use_lru_fm` is 0xCD in every retail file -- uninitialised stack the original
/// tool wrote out -- and the 16-byte version GUID is load-bearing for the version stamp. A writer
/// that rebuilt the header from parsed fields would quietly zero both.
#[test]
fn the_header_changes_only_in_the_dwords_native_changes() {
    let mut s = Scratch::new("header_bytes");
    let dat = s.copy_retail("client_local_English.dat");

    let before_raw = std::fs::read(&dat).expect("read before");
    let before = header_bytes(&before_raw);
    let free_head = u32::from_le_bytes([before[0x14], before[0x15], before[0x16], before[0x17]]);
    let free_count = u32::from_le_bytes([before[0x1C], before[0x1D], before[0x1E], before[0x1F]]);
    let chain = free_chain(&before_raw, free_head);
    assert_eq!(
        chain.len(),
        free_count as usize,
        "the copy's free chain matches its header"
    );

    // Four blocks of payload, so the data store takes chain[0..4] and leaves
    // `free_head = chain[4]`.
    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        w.save(
            DataId(0x4500_0001),
            &payload(0x77, 1020 * 3 + 1),
            2,
            5,
            1_700_000_000,
        )
        .expect("save");
    }

    let after = header_bytes(&std::fs::read(&dat).expect("read after"));
    let mut expected = before;
    expected[0x14..0x18].copy_from_slice(&chain[4].to_le_bytes());
    expected[0x1C..0x20].copy_from_slice(&(free_count - 4).to_le_bytes());
    assert_eq!(
        after.as_slice(),
        expected.as_slice(),
        "the header differs from what retail's header write produces\n after    {after:02X?}\n \
         expected {expected:02X?}"
    );
    // Spelled out, so a reader of the failure knows which byte mattered.
    assert_eq!(
        &after[0x2D..0x30],
        &[0xCD, 0xCD, 0xCD],
        "the use_lru_fm padding survived"
    );
    assert_eq!(
        &after[0x3C..0x50],
        &before[0x3C..0x50],
        "the version stamp survived"
    );
    assert_eq!(
        &after[0x08..0x0C],
        &before[0x08..0x0C],
        "the file size did not need to grow"
    );
    assert_eq!(
        &after[0x18..0x1C],
        &before[0x18..0x1C],
        "the free-chain tail did not move"
    );
    assert_eq!(
        &after[0x20..0x24],
        &before[0x20..0x24],
        "the B-tree root did not move"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The B-tree, split at the native fan-out.
// ---------------------------------------------------------------------------------------------

/// The insert walk splits the root when it already holds 61 entries, and the split is
/// 30 / 1 / 30. So the 62nd insert -- not the 61st -- turns a
/// one-node tree into a root with one entry and two leaves.
#[test]
fn the_root_splits_on_the_sixty_second_entry_thirty_one_thirty() {
    let s = Scratch::new("btree_split");
    let dat = s.path("dereth_dat_insert_split.dat");
    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 4096 * 0x400).expect("create");

    let ids: Vec<DataId> = (0..61).map(|k| DataId(0x5000_0000 + k * 0x10)).collect();
    for (k, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        w.save(*id, &payload(k as u8, 64), 2, 1, 1_700_000_000)
            .expect("fill the root leaf");
    }
    let root = w.root();
    let view = w.node(root).expect("root");
    assert_eq!(
        view.ids.len(),
        61,
        "the root leaf holds 61 entries and has not split"
    );
    assert!(view.children.is_empty(), "still a leaf");

    // The 62nd.
    let sixty_second = DataId(0x5000_0000 + 61 * 0x10);
    w.save(sixty_second, &payload(0xEE, 64), 2, 1, 1_700_000_000)
        .expect("the entry that splits");

    let new_root = w.root();
    assert_ne!(
        new_root, root,
        "the root moved to a freshly allocated block"
    );
    let top = w.node(new_root).expect("new root");
    assert_eq!(top.ids.len(), 1, "one entry was promoted");
    assert_eq!(
        top.ids[0],
        ids[30].raw(),
        "entry 0x1e is the one that goes up"
    );
    assert_eq!(top.children.len(), 2);
    assert_eq!(
        top.children[0], root,
        "the old root block became the left leaf"
    );

    let left = w.node(top.children[0]).expect("left");
    let right = w.node(top.children[1]).expect("right");
    assert_eq!(left.ids.len(), 30, "0x1e entries stay on the left");
    assert_eq!(right.ids.len(), 31, "0x1e moved across plus the new one");
    assert_eq!(
        left.ids,
        ids[..30].iter().map(|d| d.raw()).collect::<Vec<_>>()
    );
    assert_eq!(
        right.ids[..30],
        ids[31..].iter().map(|d| d.raw()).collect::<Vec<_>>()[..]
    );
    assert_eq!(right.ids[30], sixty_second.raw());

    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.double_allocated.is_empty(), "{audit:?}");
    drop(w);

    // Every one of the 62 is still readable through the ordinary reader, and the tree the reader
    // walks has two leaves at one depth.
    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), 62);
    for (k, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let want = payload(k as u8, 64);
        assert_eq!(r.read(*id).expect("read"), want, "id {id:?}");
    }
    assert_eq!(r.read(sixty_second).expect("read"), payload(0xEE, 64));
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.nodes, 3);
    assert_eq!(
        report.leaf_depths.len(),
        1,
        "both leaves are at the same depth: {report:?}"
    );
    assert_eq!(report.out_of_order, 0);
}

/// Enough entries to split internal nodes too, inserted out of order so that both the "descend
/// left of the promoted entry" and "descend right" branches of the insert walk (`descend_to_add`) run.
#[test]
fn a_thousand_entries_keep_the_tree_sound_at_every_depth() {
    let s = Scratch::new("btree_deep");
    let dat = s.path("dereth_dat_insert_deep.dat");
    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 8192 * 0x400).expect("create");

    // A shuffle with no dependencies: a multiplicative step coprime with the modulus.
    let ids: Vec<DataId> = (0..1000u32)
        .map(|k| DataId(0x6000_0000 + (k * 7919) % 100_003))
        .collect();
    for (k, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        w.save(*id, &payload(k as u8, 200), 2, 1, 1_700_000_000)
            .expect("insert");
    }
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.double_allocated.is_empty(), "{audit:?}");
    drop(w);

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), 1000);
    for (k, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let want = payload(k as u8, 200);
        assert_eq!(r.read(*id).expect("read"), want, "id {id:?}");
        assert_eq!(
            r.lookup_via_tree(*id).expect("lookup").as_ref(),
            r.entry(*id)
        );
    }
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.out_of_order, 0);
    assert!(report.max_entries_per_node <= 61, "{report:?}");
    assert_eq!(
        report.leaf_depths.len(),
        1,
        "all leaves at one depth: {report:?}"
    );
}

/// The iteration-list save rewrites `0xFFFF0001` after a patch, and the cache's
/// DDD-request completion is what calls it once a `PatchRevision` is
/// satisfied. `client_local_English.dat` ships with iterations 1..=994, so adding 995 must extend
/// the single run rather than append a second item.
#[test]
fn adding_an_iteration_rewrites_0xffff0001_as_one_run() {
    let mut s = Scratch::new("iteration");
    let dat = s.copy_retail("client_local_English.dat");

    let before = DatFile::open(&dat)
        .expect("open")
        .iteration_list()
        .expect("iteration list");
    assert!(!before.is_empty(), "the input contains an iteration run");
    assert_eq!(before[0], 1);
    let last = *before.last().unwrap();
    assert_eq!(before, (1..=last).collect::<Vec<_>>());
    let next = last + 1;

    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        assert_eq!(w.iteration_list().expect("read"), before);
        w.add_iteration(next, 1_700_000_000)
            .expect("extend the run");
        // Idempotent: adding it twice does not lengthen the set.
        w.add_iteration(next, 1_700_000_001)
            .expect("repeat the same iteration");
        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(audit.orphaned.is_empty(), "{audit:?}");
    }

    let r = DatFile::open(&dat).expect("reopen");
    let after = r.iteration_list().expect("iteration list");
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after.last(), Some(&next));
    assert_eq!(&after[..before.len()], before.as_slice());
    // Still `count`, `-count`, `first` -- twelve bytes, one run.
    let raw = r.read(DataId(0xFFFF_0001)).expect("raw payload");
    assert_eq!(
        raw.len(),
        12,
        "a single consecutive run, as in every retail dat"
    );
    assert_eq!(&raw[0..4], &next.to_le_bytes());
    assert_eq!(&raw[4..8], &(-(next as i32)).to_le_bytes());
    assert_eq!(&raw[8..12], &1u32.to_le_bytes());
    assert_eq!(r.entry(DataId(0xFFFF_0001)).expect("entry").version(), 1);
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// On save, an incoming entry with a non-zero `iter_` older than the stored one is
/// refused, and native reports that as success -- a patch must never walk a file backwards.
#[test]
fn an_older_iteration_is_refused_and_the_stored_record_is_untouched() {
    let s = Scratch::new("older_iteration");
    let dat = s.path("dereth_dat_insert_iter.dat");
    let id = DataId(0x4700_0001);
    let new = payload(0x60, 300);
    let older = payload(0x61, 300);

    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 100 * 0x400).expect("create");
    w.save(id, &new, 2, 100, 1_700_000_000)
        .expect("save at iteration 100");
    let free = w.header().free_count;

    assert_eq!(
        w.save(id, &older, 2, 50, 1_700_000_100)
            .expect("an older patch is not an error"),
        SaveOutcome::RefusedOlderIteration
    );
    assert_eq!(w.read(id).expect("read"), new, "the newer payload survived");
    assert_eq!(
        w.header().free_count,
        free,
        "nothing was allocated or freed"
    );

    // `iter_ == 0` means "no opinion" and still replaces.
    assert_eq!(
        w.save(id, &older, 2, 0, 1_700_000_200).expect("save"),
        SaveOutcome::Replaced
    );
    assert_eq!(w.read(id).expect("read"), older);

    // And so does an equal or newer one.
    assert_eq!(
        w.save(id, &new, 2, 100, 1_700_000_300).expect("save"),
        SaveOutcome::Replaced
    );
    assert_eq!(w.read(id).expect("read"), new);
}

/// A record native would refuse or mis store is refused here too.
#[test]
fn a_record_native_would_refuse_or_mis_store_is_refused_here_too() {
    let s = Scratch::new("refusals");
    let dat = s.path("dereth_dat_insert_refuse.dat");
    let mut w = DatWriter::create(&dat, 0x400, 1, 0, 0x400 + 100 * 0x400).expect("create");

    let err = w
        .save(DataId(0x4800_0001), &payload(1, 10), 0, 1, 1)
        .expect_err("version 0");
    assert!(err.to_string().contains("version 0"), "{err}");
    let err = w
        .save(DataId(0x4800_0002), &[], 2, 1, 1)
        .expect_err("empty payload");
    assert!(err.to_string().contains("empty payload"), "{err}");
    let err = w
        .save(DataId(0), &payload(1, 10), 2, 1, 1)
        .expect_err("INVALID_DID");
    assert!(err.to_string().contains("not found"), "{err}");

    // None of them left anything behind.
    assert_eq!(w.header().free_count, 98);
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.orphaned.is_empty(), "{audit:?}");
}

// ---------------------------------------------------------------------------------------------
// 6. Recoverability: what an interrupted write leaves on disk.
// ---------------------------------------------------------------------------------------------

/// An interrupt before the entry update leaves the old record readable.
#[test]
fn an_interrupt_before_the_entry_update_leaves_the_old_record_readable() {
    let mut s = Scratch::new("interrupt_chain");
    let dat = s.copy_retail("client_local_English.dat");
    let id = DataId(0x4900_0001);
    let old = payload(0x70, 1020 * 2);
    let new = payload(0x71, 1020 * 3);

    let free_after_add;
    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        w.save(id, &old, 2, 10, 1_700_000_000)
            .expect("the record that must survive");
        free_after_add = w.header().free_count;
    }

    {
        let mut w = DatWriter::open(&dat).expect("reopen for writing");
        w.inject_fault(Fault::AfterChainWrite);
        let err = w
            .save(id, &new, 3, 11, 1_700_000_100)
            .expect_err("the injected interrupt");
        assert!(
            matches!(
                err,
                dereth_dat::DatError::Interrupted(Fault::AfterChainWrite)
            ),
            "{err}"
        );
    }

    // The ordinary reader opens the file and still sees the old record, byte for byte.
    let r = DatFile::open(&dat).expect("the reader still opens the interrupted file");
    assert_eq!(
        r.read(id).expect("read"),
        old,
        "the old payload is what the directory names"
    );
    let e = r.entry(id).expect("entry");
    assert_eq!(e.version(), 2, "the entry was never rewritten");
    assert_eq!(e.iteration, 10);
    let report = r.verify_structure().expect("verify");
    assert!(
        report.is_sound(),
        "the free list is still consistent: {report:?}"
    );
    assert_eq!(report.free_chain_len, report.header_free_count as usize);
    drop(r);

    // Three blocks leaked; none of them is reachable twice.
    let mut w = DatWriter::open(&dat).expect("reopen");
    assert_eq!(
        w.header().free_count,
        free_after_add - 3,
        "the chain's blocks left the free list"
    );
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(
        audit.double_allocated.is_empty(),
        "no block is reachable twice: {audit:?}"
    );
    assert_eq!(
        audit.orphaned.len(),
        3,
        "exactly the half-written chain: {audit:?}"
    );
    let orphans = audit.orphaned.clone();

    // Writing again works, and never hands an orphan out: they are not on the free list.
    w.save(id, &new, 3, 11, 1_700_000_200)
        .expect("the retry succeeds");
    assert_eq!(w.read(id).expect("read"), new);
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.double_allocated.is_empty(), "{audit:?}");
    for o in &orphans {
        assert!(
            audit.orphaned.contains(o),
            "orphan {o:#010X} was quietly reused: {audit:?}"
        );
    }
}

/// Kill the process after the entry has been repointed but before the old chain reaches the free
/// list. The new record is the one a reader sees; the *old* chain is what leaks.
#[test]
fn an_interrupt_before_the_free_list_update_leaves_the_new_record_readable() {
    let mut s = Scratch::new("interrupt_free");
    let dat = s.copy_retail("client_local_English.dat");
    let id = DataId(0x4A00_0001);
    let old = payload(0x80, 1020 * 2);
    let new = payload(0x81, 1020 * 3);

    {
        let mut w = DatWriter::open(&dat).expect("open for writing");
        w.save(id, &old, 2, 10, 1_700_000_000)
            .expect("the record being replaced");
    }

    {
        let mut w = DatWriter::open(&dat).expect("reopen for writing");
        w.inject_fault(Fault::AfterEntryUpdate);
        let err = w
            .save(id, &new, 3, 11, 1_700_000_100)
            .expect_err("the injected interrupt");
        assert!(
            matches!(
                err,
                dereth_dat::DatError::Interrupted(Fault::AfterEntryUpdate)
            ),
            "{err}"
        );
    }

    let r = DatFile::open(&dat).expect("the reader still opens the interrupted file");
    assert_eq!(
        r.read(id).expect("read"),
        new,
        "the new payload is the one that landed"
    );
    let e = r.entry(id).expect("entry");
    assert_eq!(e.version(), 3);
    assert_eq!(e.iteration, 11);
    assert_eq!(e.date, 1_700_000_100);
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.free_chain_len, report.header_free_count as usize);
    drop(r);

    let mut w = DatWriter::open(&dat).expect("reopen");
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(
        audit.double_allocated.is_empty(),
        "no block is reachable twice: {audit:?}"
    );
    assert_eq!(
        audit.orphaned.len(),
        2,
        "exactly the old two-block chain: {audit:?}"
    );
    let orphans = audit.orphaned.clone();

    w.save(
        DataId(0x4A00_0002),
        &payload(0x82, 4000),
        2,
        1,
        1_700_000_200,
    )
    .expect("write again");
    let audit = w.audit().expect("audit");
    assert!(audit.double_allocated.is_empty(), "{audit:?}");
    for o in &orphans {
        assert!(
            audit.orphaned.contains(o),
            "orphan {o:#010X} was quietly reused: {audit:?}"
        );
    }
}

/// The interrupt cases above leave the journal alone. This writer refuses to build on top of a
/// journal record it cannot replay, rather than pretending the file is quiescent.
#[test]
fn a_pending_transaction_record_stops_the_writer_rather_than_being_ignored() {
    let mut s = Scratch::new("pending_transaction");
    let dat = s.copy_retail("client_local_English.dat");

    // A well-formed split-node transaction header: type byte 6, then the 0x4C50 magic.
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&dat)
            .expect("open the copy for a poke");
        f.seek(SeekFrom::Start(0x100)).expect("seek");
        f.write_all(&[6, 0x50, 0x4C, 0, 0])
            .expect("write a pending record");
    }

    let err = DatWriter::open(&dat).expect_err("a pending transaction must stop the writer");
    assert!(err.to_string().contains("pending"), "{err}");
    // The reader is not blocked by it: `DatFile` never looks at the journal.
    assert_eq!(
        DatFile::open(&dat)
            .expect("the reader still opens it")
            .transaction_slot()
            .expect("slot")[0],
        6
    );
}

/// The cell dat's geometry: 0x100-byte blocks, so a 0x6B4 `BTNode` is a **seven**-block chain and
/// `write_node` has to walk it rather than write two blocks and stop. Nothing else in this suite
/// exercises a node longer than two blocks.
#[test]
fn a_cell_sized_container_splits_correctly_with_seven_block_nodes() {
    let s = Scratch::new("cell_blocks");
    let dat = s.path("dereth_dat_insert_cell.dat");
    let mut w = DatWriter::create(&dat, 0x100, 2, 1, 0x400 + 4000 * 0x100).expect("create");
    assert_eq!(w.header().block_size, 0x100);
    // 0x6B4 bytes at 252 payload bytes per block is seven blocks, taken off the head at 0x400.
    assert_eq!(
        w.header().free_head,
        0x400 + 7 * 0x100,
        "the root node is a seven-block chain"
    );
    assert_eq!(w.header().free_count, 4000 - 7);

    let ids: Vec<DataId> = (0..62).map(|k| DataId(0x0100_0000 + k * 0x100)).collect();
    for (k, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        w.save(*id, &payload(k as u8, 600), 2, 1, 1_700_000_000)
            .expect("insert");
    }

    let top = w.node(w.root()).expect("root");
    assert_eq!(top.ids.len(), 1, "the 62nd entry split the root");
    assert_eq!(top.ids[0], ids[30].raw());
    assert_eq!(w.node(top.children[0]).expect("left").ids.len(), 30);
    assert_eq!(w.node(top.children[1]).expect("right").ids.len(), 31);
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound(), "{audit:?}");
    assert!(audit.double_allocated.is_empty(), "{audit:?}");
    assert_eq!(
        audit.in_nodes,
        3 * 7,
        "three nodes, seven blocks each: {audit:?}"
    );

    // A replace, so a seven-block node is rewritten in place rather than only written once.
    w.save(ids[0], &payload(0xAB, 1500), 3, 2, 1_700_000_500)
        .expect("replace");
    assert_eq!(w.read(ids[0]).expect("read"), payload(0xAB, 1500));
    drop(w);

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.header().data_set, 2, "a cell-shaped header");
    assert_eq!(r.header().data_subset, 1);
    assert_eq!(r.len(), 62);
    assert_eq!(r.read(ids[0]).expect("read"), payload(0xAB, 1500));
    for (k, id) in ids.iter().enumerate().skip(1) {
        #[allow(clippy::cast_possible_truncation)]
        let want = payload(k as u8, 600);
        assert_eq!(r.read(*id).expect("read"), want, "id {id:?}");
    }
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.nodes, 3);
    assert_eq!(report.leaf_depths.len(), 1, "{report:?}");
}
