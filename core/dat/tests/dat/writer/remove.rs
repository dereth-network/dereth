//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Removal: leaf/absent/internal-entry deletes, borrow left/right, merge and root shrink, 1,000 and
//! 5,000-entry empty-out stays sound, interrupted remove readable, mask delete takes one landblock
//! family, iteration limit respected, purge from a retail cell dat copy.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::path::{Path, PathBuf};

use dereth_dat::write::{DatWriter, Fault};
use dereth_dat::{DatError, DatFile};
use dereth_primitives::DataId;

// ---------------------------------------------------------------------------------------------
// Disposable copies, and the proof that the originals were not touched.
// ---------------------------------------------------------------------------------------------

fn digest(path: &Path) -> (u64, u64) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    (bytes.len() as u64, h)
}

struct Scratch {
    dir: PathBuf,
    pristine: Vec<(PathBuf, (u64, u64))>,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("p4_1b2_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        Self {
            dir,
            pristine: Vec::new(),
        }
    }

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

fn payload(seed: u8, n: usize) -> Vec<u8> {
    #[allow(clippy::cast_possible_truncation)]
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// A fresh container with enough room that never expands mid-test.
fn container(s: &Scratch, name: &str, blocks: u32) -> DatWriter {
    DatWriter::create(&s.path(name), 0x400, 1, 0, 0x400 + blocks * 0x400).expect("create")
}

/// Every id the directory holds, ascending, read through the writer's own tree walk.
fn ids(w: &mut DatWriter) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![w.root()];
    while let Some(off) = stack.pop() {
        let n = w.node(off).expect("node");
        out.extend(n.ids.iter().copied());
        stack.extend(n.children.iter().copied());
    }
    out.sort_unstable();
    out
}

/// How many entries each child of the root holds, for the borrow/merge assertions.
fn child_counts(w: &mut DatWriter) -> Vec<usize> {
    let root = w.node(w.root()).expect("root");
    root.children
        .iter()
        .map(|c| w.node(*c).expect("child").ids.len())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// 1. A leaf entry.
// ---------------------------------------------------------------------------------------------

/// The whole point of the card, at its smallest: an id goes away, its blocks come back, and the
/// ordinary read-only reader agrees.
#[test]
fn removing_a_leaf_entry_frees_its_chain_and_the_reader_stops_finding_it() {
    let s = Scratch::new("leaf");
    let dat = s.path("remove_leaf.dat");
    {
        let mut w = container(&s, "remove_leaf.dat", 200);
        for i in 1..=5u32 {
            #[allow(clippy::cast_possible_truncation)]
            w.save(
                DataId(0x5000_0000 + i),
                &payload(i as u8, 1020 * 2 + 5),
                2,
                1,
                1_700_000_000,
            )
            .expect("save");
        }
        let free_before = w.header().free_count;
        assert!(
            w.remove(DataId(0x5000_0003)).expect("remove"),
            "the id was there"
        );
        // 2045 bytes over 1020-byte blocks is three blocks, back on the free list.
        assert_eq!(
            w.header().free_count,
            free_before + 3,
            "the record's chain came back"
        );
        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(
            audit.orphaned.is_empty(),
            "a remove must not leak: {audit:?}"
        );
        assert!(
            !w.remove(DataId(0x5000_0003)).expect("second remove"),
            "gone is gone"
        );
    }

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), 4, "one of five ids is gone");
    assert!(matches!(
        r.read(DataId(0x5000_0003)),
        Err(DatError::NotFound(_))
    ));
    for i in [1u32, 2, 4, 5] {
        #[allow(clippy::cast_possible_truncation)]
        let want = payload(i as u8, 1020 * 2 + 5);
        assert_eq!(r.read(DataId(0x5000_0000 + i)).expect("survivor"), want);
    }
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// Removing an id the file does not hold must not touch one byte.
#[test]
fn removing_an_absent_id_changes_nothing() {
    let s = Scratch::new("absent");
    let dat = s.path("remove_absent.dat");
    {
        let mut w = container(&s, "remove_absent.dat", 100);
        w.save(DataId(0x5100_0001), &payload(7, 300), 2, 1, 1_700_000_000)
            .expect("save");
    }
    let before = digest(&dat);
    {
        let mut w = DatWriter::open(&dat).expect("reopen for write");
        assert!(
            !w.remove(DataId(0x5100_0099)).expect("remove"),
            "it was never there"
        );
    }
    assert_eq!(digest(&dat), before, "a no-op remove rewrote the file");
}

// ---------------------------------------------------------------------------------------------
// 2. An internal entry: predecessor, successor, and the merge that neither can serve.
// ---------------------------------------------------------------------------------------------

/// Fill one leaf past 61 so the root splits, then bias one side so it can spare an entry.
fn split_tree(s: &Scratch, name: &str, left_extra: u32, right_extra: u32) -> DatWriter {
    let mut w = container(s, name, 900);
    // 61 ids at multiples of 0x100 fill the root leaf exactly; the 62nd forces the split.
    for i in 1..=62u32 {
        w.save(DataId(i * 0x100), &payload(1, 60), 2, 1, 1_700_000_000)
            .expect("save");
    }
    for k in 0..left_extra {
        w.save(DataId(1 + k), &payload(2, 60), 2, 1, 1_700_000_000)
            .expect("left filler");
    }
    for k in 0..right_extra {
        w.save(DataId(63 * 0x100 + k), &payload(3, 60), 2, 1, 1_700_000_000)
            .expect("right filler");
    }
    w
}

/// The delete's first arm: the left child can spare an entry
/// (it holds more than 0x1e entries), so the walk names the predecessor and the internal delete
/// copies it over the target and drops it from the donor leaf.
#[test]
fn removing_an_internal_entry_promotes_the_predecessor_when_the_left_child_can_spare_one() {
    let s = Scratch::new("internal_pred");
    let mut w = split_tree(&s, "internal_pred.dat", 1, 0);

    let root = w.node(w.root()).expect("root");
    assert_eq!(root.ids.len(), 1, "one separator in the root");
    assert_eq!(
        child_counts(&mut w),
        vec![31, 31],
        "left biased so it can spare one"
    );
    let separator = root.ids[0];
    let left = w.node(root.children[0]).expect("left");
    let predecessor = *left.ids.last().expect("the left child's largest id");

    let before = ids(&mut w);
    assert!(w.remove(DataId(separator)).expect("remove the separator"));

    let root = w.node(w.root()).expect("root after");
    assert_eq!(
        root.ids,
        vec![predecessor],
        "the predecessor moved up into the root"
    );
    assert_eq!(
        child_counts(&mut w),
        vec![30, 31],
        "the donor leaf lost one"
    );

    let after = ids(&mut w);
    assert_eq!(after.len(), before.len() - 1);
    assert!(
        !after.contains(&separator),
        "the removed id is nowhere in the tree"
    );
    assert_eq!(
        after.iter().filter(|i| **i == predecessor).count(),
        1,
        "no duplicate key"
    );
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
}

/// The second arm: only the right child can spare an entry, so names the
/// successor instead.
#[test]
fn removing_an_internal_entry_promotes_the_successor_when_only_the_right_child_can_spare_one() {
    let s = Scratch::new("internal_succ");
    let mut w = split_tree(&s, "internal_succ.dat", 0, 1);

    let root = w.node(w.root()).expect("root");
    assert_eq!(
        child_counts(&mut w),
        vec![30, 32],
        "only the right child is above the minimum"
    );
    let separator = root.ids[0];
    let right = w.node(root.children[1]).expect("right");
    let successor = right.ids[0];

    assert!(w.remove(DataId(separator)).expect("remove the separator"));

    let root = w.node(w.root()).expect("root after");
    assert_eq!(
        root.ids,
        vec![successor],
        "the successor moved up into the root"
    );
    assert_eq!(
        child_counts(&mut w),
        vec![30, 31],
        "the donor leaf lost one"
    );
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
}

/// The third arm: both children sit at the 30-entry minimum, so
/// pushes the separator down between them -- 30 + 1 + 30 = 61 -- the root is left empty, and
/// the header's B-tree root becomes the merged node's block while the old root's blocks go to the
/// free list.
#[test]
fn two_minimal_children_merge_the_separator_down_and_the_root_shrinks() {
    let s = Scratch::new("root_shrink");
    let mut w = split_tree(&s, "root_shrink.dat", 0, 0);
    assert_eq!(
        child_counts(&mut w),
        vec![30, 31],
        "62 ids split 30 | 1 | 31"
    );

    // Take one off the right child so both sit at the minimum.
    let right_child = w.node(w.root()).expect("root").children[1];
    let right_first = w.node(right_child).expect("right").ids[0];
    assert!(w.remove(DataId(right_first)).expect("trim the right child"));
    assert_eq!(
        child_counts(&mut w),
        vec![30, 30],
        "both children at the minimum"
    );

    let old_root = w.root();
    let separator = w.node(old_root).expect("root").ids[0];
    let free_before = w.header().free_count;
    let before = ids(&mut w);

    assert!(w.remove(DataId(separator)).expect("remove the separator"));

    assert_ne!(
        w.root(),
        old_root,
        "the B-tree root moved to the merged node"
    );
    let root = w.node(w.root()).expect("root after");
    assert!(root.children.is_empty(), "the tree is one leaf deep again");
    assert_eq!(
        root.ids.len(),
        60,
        "30 + 1 + 30, minus the entry that was removed"
    );
    // Three chains came back: the right child's node (two blocks, 0x6B4 over 1020-byte blocks),
    // the old root's node (two more) and the removed record's single block.
    assert_eq!(
        w.header().free_count,
        free_before + 2 + 2 + 1,
        "the absorbed node, the old root and the record all came back"
    );

    let after = ids(&mut w);
    assert_eq!(after.len(), before.len() - 1);
    assert!(!after.contains(&separator));
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
}

/// The delete walk's borrow arm, on the way down rather than at the target: the leaf that holds
/// the target is at the minimum, its right sibling is not, so the sibling's least entry rotates
/// through the parent before the descent continues.
#[test]
fn an_underflowing_leaf_borrows_from_its_right_sibling_before_the_descent_continues() {
    let s = Scratch::new("borrow_right");
    let mut w = split_tree(&s, "borrow_right.dat", 0, 2);
    assert_eq!(
        child_counts(&mut w),
        vec![30, 33],
        "the left leaf is at the minimum"
    );

    let root = w.node(w.root()).expect("root");
    let separator_before = root.ids[0];
    let left = w.node(root.children[0]).expect("left");
    let victim = left.ids[0];
    let right_first = w.node(root.children[1]).expect("right").ids[0];

    assert!(w
        .remove(DataId(victim))
        .expect("remove from the minimal leaf"));

    let root = w.node(w.root()).expect("root after");
    assert_eq!(
        root.ids,
        vec![right_first],
        "the right child's least entry rotated into the root"
    );
    assert_ne!(root.ids[0], separator_before);
    assert_eq!(
        child_counts(&mut w),
        vec![30, 32],
        "left borrowed one and then lost one"
    );
    let after = ids(&mut w);
    assert!(!after.contains(&victim));
    assert!(
        after.contains(&separator_before),
        "the old separator moved down, it did not vanish"
    );
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
}

/// The mirror: the target is in the *right* leaf, which is at the minimum, and the left sibling is
/// the one that can spare an entry (the left-sibling flag set).
#[test]
fn an_underflowing_leaf_borrows_from_its_left_sibling_too() {
    let s = Scratch::new("borrow_left");
    let mut w = split_tree(&s, "borrow_left.dat", 2, 0);
    assert_eq!(
        child_counts(&mut w),
        vec![32, 31],
        "the right leaf can still be trimmed"
    );

    // Trim the right leaf to the minimum without touching the left one.
    let right_child = w.node(w.root()).expect("root").children[1];
    let right = w.node(right_child).expect("right");
    let trim = *right.ids.last().expect("last");
    assert!(w.remove(DataId(trim)).expect("trim"));
    assert_eq!(
        child_counts(&mut w),
        vec![32, 30],
        "now only the left sibling is above minimum"
    );

    let root = w.node(w.root()).expect("root");
    let separator_before = root.ids[0];
    let left_last = *w
        .node(root.children[0])
        .expect("left")
        .ids
        .last()
        .expect("last");
    let victim = *w
        .node(root.children[1])
        .expect("right")
        .ids
        .last()
        .expect("last");

    assert!(w
        .remove(DataId(victim))
        .expect("remove from the minimal right leaf"));

    let root = w.node(w.root()).expect("root after");
    assert_eq!(
        root.ids,
        vec![left_last],
        "the left child's largest entry rotated into the root"
    );
    assert_eq!(
        child_counts(&mut w),
        vec![31, 30],
        "left gave one up, right borrowed and lost one"
    );
    let after = ids(&mut w);
    assert!(!after.contains(&victim));
    assert!(after.contains(&separator_before));
    let audit = w.audit().expect("audit");
    assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
}

// ---------------------------------------------------------------------------------------------
// 3. The whole tree, up and back down again.
// ---------------------------------------------------------------------------------------------

/// A thousand shuffled ids in, a thousand differently-shuffled ids out. At the end the directory
/// is one empty leaf, no data block is reachable twice, none is orphaned, and every block that is
/// not the root node is on the free list. This is the test that would catch a merge or a borrow
/// that loses a subtree.
#[test]
fn a_thousand_entries_inserted_and_removed_leave_an_empty_tree_with_every_block_free() {
    let s = Scratch::new("thousand");
    let dat = s.path("thousand.dat");
    let mut order: Vec<u32> = (1..=1000u32).map(|i| 0x6000_0000 + i * 7).collect();
    // A deterministic shuffle -- an LCG walk, so the insert and remove orders differ.
    let mut state = 0x1234_5678u32;
    let mut shuffle = |v: &mut Vec<u32>| {
        for i in (1..v.len()).rev() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            v.swap(i, (state >> 8) as usize % (i + 1));
        }
    };
    shuffle(&mut order);
    let insert = order.clone();
    shuffle(&mut order);
    let remove = order.clone();
    assert_ne!(
        insert, remove,
        "the two orders must differ or the test proves less"
    );

    {
        let mut w = container(&s, "thousand.dat", 4000);
        let empty_root_nodes = w.audit().expect("audit").in_nodes;
        for id in &insert {
            #[allow(clippy::cast_possible_truncation)]
            w.save(
                DataId(*id),
                &payload((*id & 0xFF) as u8, 200),
                2,
                1,
                1_700_000_000,
            )
            .expect("save");
        }
        for (k, id) in remove.iter().enumerate() {
            assert!(
                w.remove(DataId(*id)).expect("remove"),
                "{id:#010X} at step {k}"
            );
            if k % 97 == 0 {
                let audit = w.audit().expect("audit");
                assert!(audit.is_sound(), "step {k}: {audit:?}");
                assert!(audit.double_allocated.is_empty(), "step {k}: {audit:?}");
            }
        }
        assert!(ids(&mut w).is_empty(), "the directory is empty");
        assert!(
            w.node(w.root()).expect("root").children.is_empty(),
            "one leaf, no children"
        );

        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(audit.double_allocated.is_empty(), "{audit:?}");
        assert!(audit.orphaned.is_empty(), "nothing leaked: {audit:?}");
        assert_eq!(audit.in_records, 0, "no record blocks left");
        assert_eq!(
            audit.in_nodes, empty_root_nodes,
            "only the root node is still allocated"
        );
        assert_eq!(
            audit.free + audit.in_nodes,
            audit.total,
            "every other block is free"
        );
    }

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), 0);
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// How deep the tree is, counting the root as level 1.
fn depth(w: &mut DatWriter) -> usize {
    let mut at = w.root();
    let mut d = 1;
    loop {
        let n = w.node(at).expect("node");
        let Some(first) = n.children.first() else {
            return d;
        };
        at = *first;
        d += 1;
    }
}

/// The same walk one level deeper. Everything above keeps the root's children as leaves, so the
/// rotate and merge arms only ever move *leaf* nodes -- and an entry that rotates out of an
/// internal node has to carry a child link with it (the two `memmove`s that shift the child links
/// down and back up again). Sixty-two children of 61 entries is what it takes
/// to split the root a second time, so this fills past that and then empties it again.
#[test]
fn a_three_level_tree_survives_being_emptied_one_entry_at_a_time() {
    let s = Scratch::new("deep");
    let dat = s.path("deep.dat");
    let mut order: Vec<u32> = (1..=5000u32).map(|i| 0x7000_0000 + i * 3).collect();
    let mut state = 0x0BAD_C0DEu32;
    let mut shuffle = |v: &mut Vec<u32>| {
        for i in (1..v.len()).rev() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            v.swap(i, (state >> 8) as usize % (i + 1));
        }
    };
    shuffle(&mut order);
    let insert = order.clone();
    shuffle(&mut order);
    let remove = order.clone();

    {
        let mut w = container(&s, "deep.dat", 20_000);
        let empty_root_nodes = w.audit().expect("audit").in_nodes;
        for id in &insert {
            #[allow(clippy::cast_possible_truncation)]
            w.save(
                DataId(*id),
                &payload((*id & 0xFF) as u8, 120),
                2,
                1,
                1_700_000_000,
            )
            .expect("save");
        }
        assert_eq!(
            depth(&mut w),
            3,
            "5000 entries must have split the root twice"
        );

        // Removing the first half is what drives the internal rotates and merges: the middle
        // level thins out while the root still has children of its own.
        let mut saw_two_levels = false;
        for (k, id) in remove.iter().enumerate() {
            assert!(
                w.remove(DataId(*id)).expect("remove"),
                "{id:#010X} at step {k}"
            );
            if depth(&mut w) == 2 {
                saw_two_levels = true;
            }
            if k % 499 == 0 {
                let audit = w.audit().expect("audit");
                assert!(audit.is_sound(), "step {k}: {audit:?}");
                assert!(audit.double_allocated.is_empty(), "step {k}: {audit:?}");
            }
        }
        assert!(
            saw_two_levels,
            "the tree must have collapsed a level on the way down"
        );
        assert!(ids(&mut w).is_empty());
        assert_eq!(depth(&mut w), 1, "one empty leaf left");

        let audit = w.audit().expect("audit");
        assert!(audit.is_sound(), "{audit:?}");
        assert!(audit.orphaned.is_empty(), "nothing leaked: {audit:?}");
        assert_eq!(audit.in_records, 0);
        assert_eq!(
            audit.in_nodes, empty_root_nodes,
            "only the root node is still allocated"
        );
    }

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), 0);
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// An interrupt in the middle of a remove leaves a file the ordinary reader still opens, with the
/// free list consistent and no block reachable twice. Native's window is the same one:
/// the leaf delete frees the record's chain before it rewrites the node.
#[test]
fn an_interrupted_remove_leaves_a_readable_file() {
    let s = Scratch::new("interrupt");
    let dat = s.path("interrupt.dat");
    {
        let mut w = container(&s, "interrupt.dat", 200);
        for i in 1..=6u32 {
            #[allow(clippy::cast_possible_truncation)]
            w.save(
                DataId(0x6100_0000 + i),
                &payload(i as u8, 700),
                2,
                1,
                1_700_000_000,
            )
            .expect("save");
        }
        w.inject_fault(Fault::AfterRecordFreed);
        let err = w
            .remove(DataId(0x6100_0004))
            .expect_err("the injected interrupt");
        assert!(
            matches!(err, DatError::Interrupted(Fault::AfterRecordFreed)),
            "{err}"
        );
    }

    let r = DatFile::open(&dat).expect("the reader still opens the file");
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    for i in [1u32, 2, 3, 5, 6] {
        #[allow(clippy::cast_possible_truncation)]
        let want = payload(i as u8, 700);
        assert_eq!(r.read(DataId(0x6100_0000 + i)).expect("survivor"), want);
    }
    // The interrupted id's entry is still in the directory and its chain is on the free list, so
    // the read fails loudly -- `FreeBlockInChain`, the same refusal a stale reader gets after a
    // patch -- rather than answering with something that is not the record.
    let err = r
        .read(DataId(0x6100_0004))
        .expect_err("the half-removed record must not read back");
    assert!(matches!(err, DatError::FreeBlockInChain { .. }), "{err}");

    // `audit()` cannot run here, and that is the point: it walks every entry's chain and refuses
    // one that steps into a free block, which is exactly the state the interrupt left. The
    // failure is a *leaked* record chain plus a dangling entry, never a double allocation -- and
    // the free-chain length check above is what rules the latter out.
    let mut w = DatWriter::open(&dat).expect("reopen for write");
    let err = w.audit().expect_err("the audit sees the dangling entry");
    assert!(matches!(err, DatError::FreeBlockInChain { .. }), "{err}");
}

// ---------------------------------------------------------------------------------------------
// 4. `delete_data_by_mask` -- the shape the DDD purge path actually calls.
// ---------------------------------------------------------------------------------------------

/// The cache's DDD-request worker deletes by mask with `(id, 0xFFFF0000)` for
/// a type-1 (landblock) purge, so the whole `0xXXXX0000` family goes and no neighbour does.
#[test]
fn delete_data_by_mask_takes_one_whole_landblock_family_and_no_neighbour() {
    let s = Scratch::new("mask");
    let dat = s.path("mask.dat");
    let family: Vec<u32> = vec![
        0xAABB_0100,
        0xAABB_0101,
        0xAABB_01FF,
        0xAABB_FFFE,
        0xAABB_FFFF,
    ];
    let neighbours: Vec<u32> = vec![0xAABA_FFFF, 0xAABC_0100, 0xAABC_FFFF, 0x00BB_0100];
    {
        let mut w = container(&s, "mask.dat", 300);
        for id in family.iter().chain(neighbours.iter()) {
            #[allow(clippy::cast_possible_truncation)]
            w.save(
                DataId(*id),
                &payload((*id >> 8) as u8, 400),
                2,
                1,
                1_700_000_000,
            )
            .expect("save");
        }
        let removed = w
            .delete_data_by_mask(DataId(0xAABB_FFFF), 0xFFFF_0000)
            .expect("delete the landblock family");
        assert_eq!(
            removed,
            family.len(),
            "every id of the family, and only those"
        );
        let audit = w.audit().expect("audit");
        assert!(audit.is_sound() && audit.orphaned.is_empty(), "{audit:?}");
    }

    let r = DatFile::open(&dat).expect("reopen");
    for id in &family {
        assert!(
            matches!(r.read(DataId(*id)), Err(DatError::NotFound(_))),
            "{id:#010X} survived"
        );
    }
    for id in &neighbours {
        assert!(
            r.read(DataId(*id)).is_ok(),
            "{id:#010X} was collateral damage"
        );
    }
    assert!(r.verify_structure().expect("verify").is_sound());
}

/// The delete's iteration guard refuses a record whose stored `iter_` is newer than the
/// caller's limit. The purge path never uses it, but the delete is the public entry point and
/// carries it.
#[test]
fn delete_data_refuses_a_record_newer_than_the_iteration_limit() {
    let s = Scratch::new("iter_guard");
    let dat = s.path("iter_guard.dat");
    {
        let mut w = container(&s, "iter_guard.dat", 100);
        w.save(DataId(0x6200_0001), &payload(9, 300), 2, 500, 1_700_000_000)
            .expect("save");
        assert!(!w
            .delete_data(DataId(0x6200_0001), 400)
            .expect("below the stored iteration"));
        assert!(w
            .delete_data(DataId(0x6200_0001), 500)
            .expect("at the stored iteration"));
    }
    let r = DatFile::open(&dat).expect("reopen");
    assert!(matches!(
        r.read(DataId(0x6200_0001)),
        Err(DatError::NotFound(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// 5. The retail cell dat, which is the file a purge actually arrives for.
// ---------------------------------------------------------------------------------------------

/// A copy of the real cell dat -- 0x100-byte blocks, so a `BTNode` is seven blocks and the node
/// chain arithmetic is different from every other test above. One shipped landblock family is
/// purged and the rest of the file still verifies.
#[test]
fn a_landblock_family_is_purged_from_a_copy_of_the_retail_cell_dat() {
    let mut s = Scratch::new("retail_cell");
    let dat = s.copy_retail("client_cell_1.dat");

    let (block, present, entries_before) = {
        let r = DatFile::open(&dat).expect("open the copy");
        let block = 0xA9B4u32;
        let present: Vec<u32> = r
            .iter_ids()
            .map(DataId::raw)
            .filter(|id| id >> 16 == block)
            .collect();
        assert!(
            present.len() > 1,
            "the fixture landblock must have a family to purge"
        );
        (block, present, r.len())
    };

    {
        let mut w = DatWriter::open(&dat).expect("open the copy for writing");
        let removed = w
            .delete_data_by_mask(DataId((block << 16) | 0xFFFF), 0xFFFF_0000)
            .expect("purge the family");
        assert_eq!(removed, present.len());
    }

    let r = DatFile::open(&dat).expect("reopen");
    assert_eq!(r.len(), entries_before - present.len());
    for id in &present {
        assert!(
            matches!(r.read(DataId(*id)), Err(DatError::NotFound(_))),
            "{id:#010X} survived"
        );
    }
    let report = r.verify_structure().expect("verify");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.leaf_depths.len(), 1, "one leaf depth: {report:?}");
}
