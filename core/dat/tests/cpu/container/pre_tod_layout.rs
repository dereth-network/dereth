//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A container in the layout from before Throne of Destiny (44-byte header at 0x12C, 12-byte
//! directory entries, the iteration in the header) opens, walks and reads, and a dat set of two
//! such files opens as a store.
//! Fixture: containers built byte by byte in the test.

use dereth_dat::btree::PRE_TOD_NODE_SIZE;
use dereth_dat::container::{DatFile, CELL_DATFILE, PORTAL_DATFILE};
use dereth_dat::{ContainerEra, DatError, DatKind, PreTodDat, RetailDat, RetailDatStore};
use dereth_primitives::DataId;

/// Builds a container block by block: the 0x400-byte prologue, then chains laid end to end.
struct Builder {
    bytes: Vec<u8>,
    block: usize,
}

impl Builder {
    fn new(block: usize) -> Self {
        Self {
            bytes: vec![0; 0x400],
            block,
        }
    }

    /// Append `payload` as a chain of consecutive blocks and return its first block's offset.
    fn chain(&mut self, payload: &[u8]) -> u32 {
        let first = self.bytes.len();
        let per = self.block - 4;
        let blocks = payload.len().div_ceil(per).max(1);
        for i in 0..blocks {
            let next = if i + 1 == blocks {
                0
            } else {
                u32::try_from(self.bytes.len() + self.block).unwrap()
            };
            self.bytes.extend_from_slice(&next.to_le_bytes());
            let part = &payload[(i * per).min(payload.len())..((i + 1) * per).min(payload.len())];
            self.bytes.extend_from_slice(part);
            self.bytes.resize(first + (i + 1) * self.block, 0);
        }
        u32::try_from(first).unwrap()
    }

    /// A free block, its link carrying bit 31.
    fn free_block(&mut self) -> u32 {
        let at = u32::try_from(self.bytes.len()).unwrap();
        self.bytes.extend_from_slice(&0x8000_0000u32.to_le_bytes());
        self.bytes.resize(self.bytes.len() + self.block - 4, 0);
        at
    }

    /// A directory node: children (fill past the count), count, then 12-byte entries.
    fn node(children: &[u32], entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut n = vec![0xCD; PRE_TOD_NODE_SIZE];
        for i in 0..62 {
            let c = if children.is_empty() {
                if i == 0 {
                    0
                } else {
                    0xCDCD_CDCD
                }
            } else {
                children.get(i).copied().unwrap_or(0xCDCD_CDCD)
            };
            n[i * 4..i * 4 + 4].copy_from_slice(&c.to_le_bytes());
        }
        n[248..252].copy_from_slice(&u32::try_from(entries.len()).unwrap().to_le_bytes());
        for (i, (id, first, size)) in entries.iter().enumerate() {
            let o = 252 + i * 12;
            n[o..o + 4].copy_from_slice(&id.to_le_bytes());
            n[o + 4..o + 8].copy_from_slice(&first.to_le_bytes());
            n[o + 8..o + 12].copy_from_slice(&size.to_le_bytes());
        }
        n
    }

    /// Write the 44-byte header at 0x12C and return the file's bytes.
    fn finish(mut self, iteration: u32, root: u32, free: u32) -> Vec<u8> {
        let size = u32::try_from(self.bytes.len()).unwrap();
        let words = [
            0x5442,
            u32::try_from(self.block).unwrap(),
            size,
            iteration,
            free,
            free,
            1,
            root,
            0,
            0,
            0,
        ];
        for (i, w) in words.iter().enumerate() {
            self.bytes[0x12C + i * 4..0x12C + i * 4 + 4].copy_from_slice(&w.to_le_bytes());
        }
        self.bytes
    }
}

fn record(id: u32, len: usize) -> Vec<u8> {
    let mut v: Vec<u8> = id.to_le_bytes().to_vec();
    v.extend((0..len).map(|k| u8::try_from(k % 251).unwrap()));
    v
}

/// A two-level tree (a root with one entry over two leaves) in 0x100-byte blocks, so each 984-byte
/// node spans four blocks; records of one block and of many.
fn two_level(block: usize, iteration: u32) -> (Vec<u8>, Vec<(u32, Vec<u8>)>) {
    let mut b = Builder::new(block);
    let records: Vec<(u32, Vec<u8>)> = [
        (0x0100_0001, 10),
        (0x0100_0002, 700),
        (0x0600_0010, 3000),
        (0x0E00_0004, 40),
        (0x0E00_0018, 1),
    ]
    .into_iter()
    .map(|(id, len)| (id, record(id, len)))
    .collect();
    let at: Vec<u32> = records.iter().map(|(_, p)| b.chain(p)).collect();
    let entry = |i: usize| {
        (
            records[i].0,
            at[i],
            u32::try_from(records[i].1.len()).unwrap(),
        )
    };
    let left = b.chain(&Builder::node(&[], &[entry(0), entry(1)]));
    let right = b.chain(&Builder::node(&[], &[entry(3), entry(4)]));
    let root = b.chain(&Builder::node(&[left, right], &[entry(2)]));
    let free = b.free_block();
    (b.finish(iteration, root, free), records)
}

#[test]
fn a_pre_tod_container_walks_its_twelve_byte_directory_and_reads_every_record() {
    let (bytes, records) = two_level(0x100, 1593);
    let file = DatFile::from_storage("cell.dat".into(), Box::new(bytes)).expect("opens");
    assert_eq!(file.era(), ContainerEra::PreTod);
    assert_eq!(file.header_iteration(), Some(1593));
    assert_eq!(file.header().block_size, 0x100);
    assert_eq!(
        file.header().data_set,
        CELL_DATFILE,
        "a 0x100-byte block is the cell file"
    );
    assert_eq!(file.len(), records.len());
    for (id, payload) in &records {
        assert_eq!(&file.read(DataId(*id)).expect("reads"), payload);
        assert!(file.lookup_via_tree(DataId(*id)).expect("search").is_some());
        let e = file.entry(DataId(*id)).expect("entry");
        assert_eq!((e.date, e.iteration, e.version()), (0, 0, 0));
    }
    assert!(file
        .lookup_via_tree(DataId(0x0100_0003))
        .expect("search")
        .is_none());
    let report = file.verify_structure().expect("walks");
    assert!(report.is_sound(), "{report:?}");
    assert_eq!(report.nodes, 3);
    assert_eq!(report.free_chain_len, 1);
    // No iteration list in this layout.
    assert!(matches!(file.iteration_list(), Err(DatError::NotFound(_))));
}

/// An older world with the later interface beside it: the older files answer every record they
/// hold (in their layout), and the later portal and language files every other one.
#[test]
fn the_later_files_answer_what_an_older_world_lacks() {
    use dereth_dat::write::DatWriter;
    let dir = std::env::temp_dir().join(format!("dereth-pre-tod-later-{}", std::process::id()));
    let later_dir = dir.join("later");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&later_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(PreTodDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(PreTodDat::Cell.in_dir(&dir), &cell).expect("write");
    let shared = DataId(records[2].0);
    let only_later = DataId(0x3900_0001);
    let string_table = DataId(0x2300_0001);
    {
        let mut w = DatWriter::create(
            &RetailDat::Portal.in_dir(&later_dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        w.save(only_later, b"later only", 1, 1, 1).expect("save");
        w.save(shared, b"later copy", 1, 1, 1).expect("save");
        let mut l = DatWriter::create(
            &RetailDat::Local.in_dir(&later_dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
        l.save(string_table, b"strings", 1, 1, 1).expect("save");
    }
    let s = RetailDatStore::open_pre_tod_with_later(&dir, &later_dir).expect("opens");
    assert!(s.has_later_interface());
    assert_eq!(s.era(), ContainerEra::PreTod);
    // A record both portal files hold is the older one's.
    assert_eq!(s.read_portal(shared).expect("reads"), records[2].1);
    assert_eq!(s.era_of(shared), ContainerEra::PreTod);
    // One only the later portal file holds is the later one's, in its layout.
    assert_eq!(s.read_portal(only_later).expect("reads"), b"later only");
    assert_eq!(s.era_of(only_later), ContainerEra::Tod);
    assert!(s.resolve(only_later).is_some());
    // The language reads are the later language file's.
    assert_eq!(s.local().read(string_table).expect("reads"), b"strings");
    assert_eq!(s.era_of(string_table), ContainerEra::Tod);
    // The later files on their own read the shared record as the later portal file has it.
    let later = s.later_files().expect("the later files");
    assert_eq!(later.era(), ContainerEra::Tod);
    assert_eq!(later.read_portal(shared).expect("reads"), b"later copy");
    assert_eq!(later.read_portal(only_later).expect("reads"), b"later only");
    assert_eq!(later.local().read(string_table).expect("reads"), b"strings");
    assert!(!later.has_later_interface());
    drop(later);
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A later world with an older portal file beside it for presentation: the world's reads never
/// reach the older file, and the older file answers only as the legacy files, in its own layout.
#[test]
fn an_older_portal_beside_a_later_world_is_read_only_as_the_legacy_files() {
    use dereth_dat::write::DatWriter;
    let dir = std::env::temp_dir().join(format!("dereth-legacy-beside-{}", std::process::id()));
    let legacy_dir = dir.join("legacy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&legacy_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    std::fs::write(PreTodDat::Portal.in_dir(&legacy_dir), &portal).expect("write");
    let shared = DataId(records[2].0);
    let only_older = DataId(records[3].0);
    {
        let mut p = DatWriter::create(
            &RetailDat::Portal.in_dir(&dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        DatWriter::create(
            &RetailDat::Cell.in_dir(&dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        DatWriter::create(
            &RetailDat::Local.in_dir(&dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
    }
    let world = RetailDatStore::open_dir(&dir).expect("opens");
    assert!(world.legacy_files().is_none(), "nothing beside it yet");
    assert_eq!(
        world.modern_files().map(|m| m.era()),
        Some(ContainerEra::Tod),
        "a later world is its own later files"
    );
    let world = world.with_legacy_portal(&legacy_dir).expect("attaches");
    // The world's own reads are unchanged: the shared id is the later copy, and the record only
    // the older file holds is not there.
    assert_eq!(world.read_portal(shared).expect("reads"), b"later copy");
    assert!(world.read_portal(only_older).is_err());
    assert_eq!(world.era_of(shared), ContainerEra::Tod);
    // The legacy files read the older file, in its layout.
    let legacy = world.legacy_files().expect("the legacy files");
    assert_eq!(legacy.era(), ContainerEra::PreTod);
    assert_eq!(legacy.read_portal(shared).expect("reads"), records[2].1);
    assert_eq!(legacy.read_portal(only_older).expect("reads"), records[3].1);
    assert_eq!(legacy.era_of(shared), ContainerEra::PreTod);
    drop(legacy);
    // A later-layout file under the older name is refused as presentation files.
    std::fs::write(
        PreTodDat::Portal.in_dir(&legacy_dir),
        std::fs::read(RetailDat::Portal.in_dir(&dir)).expect("read"),
    )
    .expect("write");
    let err = RetailDatStore::open_dir(&dir)
        .expect("opens")
        .with_legacy_portal(&legacy_dir)
        .expect_err("the later layout under the older name");
    assert!(matches!(
        err,
        DatError::UnexpectedContainerEra {
            found: ContainerEra::Tod,
            expected: ContainerEra::PreTod,
            ..
        }
    ));
    drop(world);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An older world is its own legacy files, and its later files are the ones beside it.
#[test]
fn an_older_world_is_its_own_legacy_files() {
    let dir = std::env::temp_dir().join(format!("dereth-pre-tod-own-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(PreTodDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(PreTodDat::Cell.in_dir(&dir), &cell).expect("write");
    let s = RetailDatStore::open_pre_tod_dir(&dir).expect("opens");
    let legacy = s.legacy_files().expect("its own files");
    assert_eq!(legacy.era(), ContainerEra::PreTod);
    let (id, payload) = &records[1];
    assert_eq!(&legacy.read_portal(DataId(*id)).expect("reads"), payload);
    assert!(s.modern_files().is_none(), "no later files beside it");
    drop((s, legacy));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A later world's objects drawn with the older portal beside it: the older file answers every
/// record it holds, the world's own portal every record it lacks, and an image level is always
/// the later files'. The world's own reads are untouched, and its own era has no such store.
#[test]
fn the_object_files_of_a_later_world_read_the_older_portal_first_and_the_world_for_the_rest() {
    use dereth_dat::write::DatWriter;
    use dereth_dat::DbType;
    let dir = std::env::temp_dir().join(format!("dereth-object-files-{}", std::process::id()));
    let legacy_dir = dir.join("legacy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&legacy_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    std::fs::write(PreTodDat::Portal.in_dir(&legacy_dir), &portal).expect("write");
    let shared = DataId(records[1].0);
    let image = DataId(records[2].0);
    let world_only = DataId(0x0100_0099);
    {
        let mut p = DatWriter::create(
            &RetailDat::Portal.in_dir(&dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        p.save(world_only, b"world only", 1, 1, 1).expect("save");
        p.save(image, b"later image", 1, 1, 1).expect("save");
        DatWriter::create(
            &RetailDat::Cell.in_dir(&dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        DatWriter::create(
            &RetailDat::Local.in_dir(&dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
    }
    let world = RetailDatStore::open_dir(&dir).expect("opens");
    assert!(
        world.object_files(ContainerEra::PreTod).is_none(),
        "no older files beside it yet"
    );
    assert!(
        world.object_files(ContainerEra::Tod).is_none(),
        "the world's own era is the world's files"
    );
    let world = world.with_legacy_portal(&legacy_dir).expect("attaches");
    let objects = world
        .object_files(ContainerEra::PreTod)
        .expect("the older look");
    // A record both hold is the older file's, in its layout.
    assert_eq!(objects.read_portal(shared).expect("reads"), records[1].1);
    assert_eq!(objects.era_of(shared), ContainerEra::PreTod);
    // A record the older file lacks is the world's.
    assert_eq!(
        objects.read_portal(world_only).expect("reads"),
        b"world only"
    );
    assert_eq!(objects.era_of(world_only), ContainerEra::Tod);
    // An image level is the later files' even though the older file holds the same id.
    assert_eq!(
        objects
            .read_typed(DbType::RenderSurface, image)
            .expect("reads"),
        b"later image"
    );
    assert_eq!(objects.era_of(image), ContainerEra::Tod);
    // The world itself still reads its own.
    assert_eq!(world.read_portal(shared).expect("reads"), b"later copy");
    drop((objects, world));
    let _ = std::fs::remove_dir_all(&dir);
}

/// An older world's objects drawn with the later files beside it: the later portal answers what
/// it holds, the older world's portal what it lacks, and image levels are the later files'.
#[test]
fn the_object_files_of_an_older_world_read_the_later_portal_first_and_the_world_for_the_rest() {
    use dereth_dat::write::DatWriter;
    use dereth_dat::DbType;
    let dir = std::env::temp_dir().join(format!("dereth-object-files-old-{}", std::process::id()));
    let later_dir = dir.join("later");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&later_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(PreTodDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(PreTodDat::Cell.in_dir(&dir), &cell).expect("write");
    let shared = DataId(records[1].0);
    let older_only = DataId(records[0].0);
    let image = DataId(records[2].0);
    {
        let mut p = DatWriter::create(
            &RetailDat::Portal.in_dir(&later_dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        p.save(image, b"later image", 1, 1, 1).expect("save");
        DatWriter::create(
            &RetailDat::Local.in_dir(&later_dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
    }
    let world = RetailDatStore::open_pre_tod_with_later(&dir, &later_dir).expect("opens");
    assert!(world.object_files(ContainerEra::PreTod).is_none());
    let objects = world
        .object_files(ContainerEra::Tod)
        .expect("the later look");
    assert_eq!(objects.era(), ContainerEra::Tod);
    assert_eq!(objects.read_portal(shared).expect("reads"), b"later copy");
    assert_eq!(objects.era_of(shared), ContainerEra::Tod);
    assert_eq!(
        objects.read_portal(older_only).expect("reads"),
        records[0].1
    );
    assert_eq!(objects.era_of(older_only), ContainerEra::PreTod);
    assert_eq!(
        objects
            .read_typed(DbType::RenderSurface, image)
            .expect("reads"),
        b"later image"
    );
    // The world reads its own older record for the shared id.
    assert_eq!(world.read_portal(shared).expect("reads"), records[1].1);
    drop((objects, world));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_with_the_magic_at_neither_offset_is_refused() {
    let mut bytes = vec![0u8; 0x800];
    bytes[0x12C..0x130].copy_from_slice(&0x5443u32.to_le_bytes());
    assert!(matches!(
        DatFile::from_storage("x.dat".into(), Box::new(bytes)),
        Err(DatError::BadMagic(0))
    ));
}

#[test]
fn a_pre_tod_dat_set_opens_as_a_store_whose_portal_file_answers_language_reads() {
    let dir = std::env::temp_dir().join(format!("dereth-pre-tod-store-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(PreTodDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(PreTodDat::Cell.in_dir(&dir), &cell).expect("write");
    assert!(dereth_dat::holds_pre_tod_dats(&dir));

    let s = RetailDatStore::open_pre_tod_dir(&dir).expect("opens");
    assert_eq!(s.era(), ContainerEra::PreTod);
    assert_eq!(s.portal().header().data_set, PORTAL_DATFILE);
    assert_eq!(s.portal().header_iteration(), Some(2112));
    assert_eq!(s.cell().header_iteration(), Some(1593));
    assert_eq!(
        s.file(DatKind::Local).map(DatFile::path),
        Some(s.portal().path())
    );
    let (id, payload) = &records[3];
    assert_eq!(&s.local().read(DataId(*id)).expect("reads"), payload);
    assert!(!s.grant_highres().expect("no high-resolution file"));
    assert!(s.later_files().is_none(), "no later files beside it");

    // The same files under the later names are refused as the later dat set.
    for (dat, bytes) in [
        (RetailDat::Portal, &portal),
        (RetailDat::Cell, &cell),
        (RetailDat::Local, &portal),
    ] {
        std::fs::write(dat.in_dir(&dir), bytes).expect("write");
    }
    let err =
        RetailDatStore::open_dir(&dir).expect_err("the later dat set's names, the older layout");
    assert!(
        matches!(
            err,
            DatError::UnexpectedContainerEra {
                found: ContainerEra::PreTod,
                expected: ContainerEra::Tod,
                ..
            }
        ),
        "{err}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}
