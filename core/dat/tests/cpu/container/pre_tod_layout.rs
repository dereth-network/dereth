//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A container in the layout from before Throne of Destiny (44-byte header at 0x12C, 12-byte
//! directory entries, the iteration in the header) opens, walks and reads, and a dat set of two
//! such files opens as a store.
//! Fixture: containers built byte by byte in the test.

use dereth_dat::btree::CLASSIC_NODE_SIZE;
use dereth_dat::container::{DatFile, CELL_DATFILE, PORTAL_DATFILE};
use dereth_dat::{ClassicDat, ContainerEra, DatError, DatKind, ModernDat, RetailDatStore};
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
        let mut n = vec![0xCD; CLASSIC_NODE_SIZE];
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
    assert_eq!(file.era(), ContainerEra::Classic);
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

/// An older world with the modern interface beside it: the older files answer every record they
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
    std::fs::write(ClassicDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(ClassicDat::Cell.in_dir(&dir), &cell).expect("write");
    let shared = DataId(records[2].0);
    let only_later = DataId(0x3900_0001);
    let string_table = DataId(0x2300_0001);
    {
        let mut w = DatWriter::create(
            &ModernDat::Portal.in_dir(&later_dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        w.save(only_later, b"later only", 1, 1, 1).expect("save");
        w.save(shared, b"later copy", 1, 1, 1).expect("save");
        let mut l = DatWriter::create(
            &ModernDat::Local.in_dir(&later_dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
        l.save(string_table, b"strings", 1, 1, 1).expect("save");
    }
    let s = RetailDatStore::open_classic_with_modern(&dir, &later_dir).expect("opens");
    assert!(s.has_modern_interface());
    assert_eq!(s.era(), ContainerEra::Classic);
    // A record both portal files hold is the older one's.
    assert_eq!(s.read_portal(shared).expect("reads"), records[2].1);
    assert_eq!(s.era_of(shared), ContainerEra::Classic);
    // One only the later portal file holds is the later one's, in its layout.
    assert_eq!(s.read_portal(only_later).expect("reads"), b"later only");
    assert_eq!(s.era_of(only_later), ContainerEra::Modern);
    assert!(s.resolve(only_later).is_some());
    // The language reads are the later language file's.
    assert_eq!(s.local().read(string_table).expect("reads"), b"strings");
    assert_eq!(s.era_of(string_table), ContainerEra::Modern);
    // The later files on their own read the shared record as the later portal file has it.
    let later = s.modern_companion_files().expect("the later files");
    assert_eq!(later.era(), ContainerEra::Modern);
    assert_eq!(later.read_portal(shared).expect("reads"), b"later copy");
    assert_eq!(later.read_portal(only_later).expect("reads"), b"later only");
    assert_eq!(later.local().read(string_table).expect("reads"), b"strings");
    assert!(!later.has_modern_interface());
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
    std::fs::write(ClassicDat::Portal.in_dir(&legacy_dir), &portal).expect("write");
    let shared = DataId(records[2].0);
    let only_older = DataId(records[3].0);
    {
        let mut p = DatWriter::create(
            &ModernDat::Portal.in_dir(&dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        DatWriter::create(
            &ModernDat::Cell.in_dir(&dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        DatWriter::create(
            &ModernDat::Local.in_dir(&dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
    }
    let world = RetailDatStore::open_dir(&dir).expect("opens");
    assert!(world.classic_files().is_none(), "nothing beside it yet");
    assert_eq!(
        world.modern_files().map(|m| m.era()),
        Some(ContainerEra::Modern),
        "a later world is its own later files"
    );
    let world = world.with_classic_portal(&legacy_dir).expect("attaches");
    // The world's own reads are unchanged: the shared id is the later copy, and the record only
    // the older file holds is not there.
    assert_eq!(world.read_portal(shared).expect("reads"), b"later copy");
    assert!(world.read_portal(only_older).is_err());
    assert_eq!(world.era_of(shared), ContainerEra::Modern);
    // The legacy files read the older file, in its layout.
    let legacy = world.classic_files().expect("the legacy files");
    assert_eq!(legacy.era(), ContainerEra::Classic);
    assert_eq!(legacy.read_portal(shared).expect("reads"), records[2].1);
    assert_eq!(legacy.read_portal(only_older).expect("reads"), records[3].1);
    assert_eq!(legacy.era_of(shared), ContainerEra::Classic);
    drop(legacy);
    // A later-layout file under the older name is refused as presentation files.
    std::fs::write(
        ClassicDat::Portal.in_dir(&legacy_dir),
        std::fs::read(ModernDat::Portal.in_dir(&dir)).expect("read"),
    )
    .expect("write");
    let err = RetailDatStore::open_dir(&dir)
        .expect("opens")
        .with_classic_portal(&legacy_dir)
        .expect_err("the later layout under the older name");
    assert!(matches!(
        err,
        DatError::UnexpectedContainerEra {
            found: ContainerEra::Modern,
            expected: ContainerEra::Classic,
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
    std::fs::write(ClassicDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(ClassicDat::Cell.in_dir(&dir), &cell).expect("write");
    let s = RetailDatStore::open_classic_dir(&dir).expect("opens");
    let legacy = s.classic_files().expect("its own files");
    assert_eq!(legacy.era(), ContainerEra::Classic);
    let (id, payload) = &records[1];
    assert_eq!(&legacy.read_portal(DataId(*id)).expect("reads"), payload);
    assert!(s.modern_files().is_none(), "no later files beside it");
    drop((s, legacy));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A later world's objects drawn with the older portal beside it: the older file answers every
/// record, in its own layout, image ids included; a record it lacks is missing, never the
/// world's. The world's own reads are untouched, and its own era has no such store.
#[test]
fn the_object_files_of_a_later_world_are_the_older_portal_alone() {
    use dereth_dat::write::DatWriter;
    use dereth_dat::DbType;
    let dir = std::env::temp_dir().join(format!("dereth-object-files-{}", std::process::id()));
    let legacy_dir = dir.join("legacy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&legacy_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    std::fs::write(ClassicDat::Portal.in_dir(&legacy_dir), &portal).expect("write");
    let shared = DataId(records[1].0);
    let image = DataId(records[2].0);
    let world_only = DataId(0x0100_0099);
    {
        let mut p = DatWriter::create(
            &ModernDat::Portal.in_dir(&dir),
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
            &ModernDat::Cell.in_dir(&dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        DatWriter::create(
            &ModernDat::Local.in_dir(&dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
    }
    let world = RetailDatStore::open_dir(&dir).expect("opens");
    assert!(
        world.object_files(ContainerEra::Classic).is_none(),
        "no older files beside it yet"
    );
    assert!(
        world.object_files(ContainerEra::Modern).is_none(),
        "the world's own era is the world's files"
    );
    let world = world.with_classic_portal(&legacy_dir).expect("attaches");
    let objects = world
        .object_files(ContainerEra::Classic)
        .expect("the older look");
    // A record both hold is the older file's, in its layout.
    assert_eq!(objects.read_portal(shared).expect("reads"), records[1].1);
    assert_eq!(objects.era_of(shared), ContainerEra::Classic);
    // A record the older file lacks is not borrowed from the world.
    assert!(objects.read_portal(world_only).is_err());
    // An id the older file holds is its own record, whatever the later files hold under it.
    assert_eq!(
        objects
            .read_typed(DbType::RenderSurface, image)
            .expect("reads"),
        records[2].1
    );
    assert_eq!(objects.era_of(image), ContainerEra::Classic);
    // The world itself still reads its own.
    assert_eq!(world.read_portal(shared).expect("reads"), b"later copy");
    // With no older cell file in the folder there are no older interiors.
    assert!(world.interior_files(ContainerEra::Classic).is_none());
    drop((objects, world));
    let _ = std::fs::remove_dir_all(&dir);
}

/// An older world's objects drawn with the later files beside it: the later portal answers every
/// record it holds, and a record only the older world holds is missing, never the world's. The
/// later cell file beside it answers the interiors drawn with the later look, and nothing else.
#[test]
fn the_object_files_of_an_older_world_are_the_later_portal_alone() {
    use dereth_dat::write::DatWriter;
    use dereth_dat::DbType;
    let dir = std::env::temp_dir().join(format!("dereth-object-files-old-{}", std::process::id()));
    let later_dir = dir.join("later");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&later_dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(ClassicDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(ClassicDat::Cell.in_dir(&dir), &cell).expect("write");
    let shared = DataId(records[1].0);
    let older_only = DataId(records[0].0);
    let image = DataId(records[2].0);
    {
        let mut p = DatWriter::create(
            &ModernDat::Portal.in_dir(&later_dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        p.save(image, b"later image", 1, 1, 1).expect("save");
        DatWriter::create(
            &ModernDat::Local.in_dir(&later_dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
        let mut c = DatWriter::create(
            &ModernDat::Cell.in_dir(&later_dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        c.save(DataId(0x0101_0100), b"later room", 1, 1, 1)
            .expect("save");
    }
    let world = RetailDatStore::open_classic_with_modern(&dir, &later_dir).expect("opens");
    assert!(world.object_files(ContainerEra::Classic).is_none());
    // The later cell file answers the interiors' cell reads, beside the later portal; the
    // world's own cell reads are untouched.
    let interiors = world
        .interior_files(ContainerEra::Modern)
        .expect("the later interiors");
    assert_eq!(
        interiors.read_cell(DataId(0x0101_0100)).expect("reads"),
        b"later room"
    );
    assert_eq!(interiors.read_portal(shared).expect("reads"), b"later copy");
    assert!(world.read_cell(DataId(0x0101_0100)).is_err());
    assert!(world.interior_files(ContainerEra::Classic).is_none());
    let objects = world
        .object_files(ContainerEra::Modern)
        .expect("the later look");
    assert_eq!(objects.era(), ContainerEra::Modern);
    assert_eq!(objects.read_portal(shared).expect("reads"), b"later copy");
    assert_eq!(objects.era_of(shared), ContainerEra::Modern);
    assert!(objects.read_portal(older_only).is_err());
    assert_eq!(
        objects
            .read_typed(DbType::RenderSurface, image)
            .expect("reads"),
        b"later image"
    );
    // The world reads its own older record for the shared id.
    assert_eq!(world.read_portal(shared).expect("reads"), records[1].1);
    // Attaching a Classic presentation portal retains the Modern portal but replaces the
    // single companion cell. Interior views use that cell without another format check.
    let replaced = world.clone().with_classic_portal(&dir).expect("attaches");
    let interiors = replaced
        .interior_files(ContainerEra::Modern)
        .expect("the replaced companion cell");
    assert_eq!(interiors.cell().era(), ContainerEra::Classic);
    assert_eq!(interiors.cell().path(), ClassicDat::Cell.in_dir(&dir));
    assert_eq!(interiors.read_portal(shared).expect("reads"), b"later copy");
    assert_eq!(
        replaced.read_portal(shared).expect("world reads"),
        records[1].1
    );

    let portal_only = dir.join("portal-only");
    std::fs::create_dir_all(&portal_only).expect("temp dir");
    std::fs::write(ClassicDat::Portal.in_dir(&portal_only), &portal).expect("write");
    let no_cell = replaced
        .with_classic_portal(&portal_only)
        .expect("attaches");
    assert!(no_cell.interior_files(ContainerEra::Modern).is_none());
    assert_eq!(
        no_cell
            .object_files(ContainerEra::Modern)
            .expect("retained portal")
            .read_portal(shared)
            .expect("reads"),
        b"later copy"
    );
    // An existing cell in the wrong format is just as absent as a missing file.
    std::fs::copy(
        ModernDat::Cell.in_dir(&later_dir),
        ClassicDat::Cell.in_dir(&portal_only),
    )
    .expect("copy");
    let wrong_cell = no_cell.with_classic_portal(&portal_only).expect("attaches");
    assert!(wrong_cell.interior_files(ContainerEra::Modern).is_none());
    assert_eq!(
        wrong_cell
            .object_files(ContainerEra::Modern)
            .expect("retained portal")
            .read_portal(shared)
            .expect("reads"),
        b"later copy"
    );
    drop((interiors, wrong_cell));
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
    std::fs::write(ClassicDat::Portal.in_dir(&dir), &portal).expect("write");
    std::fs::write(ClassicDat::Cell.in_dir(&dir), &cell).expect("write");
    assert!(dereth_dat::holds_classic_dats(&dir));

    let s = RetailDatStore::open_classic_dir(&dir).expect("opens");
    assert_eq!(s.era(), ContainerEra::Classic);
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
    assert!(
        s.modern_companion_files().is_none(),
        "no later files beside it"
    );

    // The same files under the later names are refused as the later dat set.
    for (dat, bytes) in [
        (ModernDat::Portal, &portal),
        (ModernDat::Cell, &cell),
        (ModernDat::Local, &portal),
    ] {
        std::fs::write(dat.in_dir(&dir), bytes).expect("write");
    }
    let err =
        RetailDatStore::open_dir(&dir).expect_err("the later dat set's names, the older layout");
    assert!(
        matches!(
            err,
            DatError::UnexpectedContainerEra {
                found: ContainerEra::Classic,
                expected: ContainerEra::Modern,
                ..
            }
        ),
        "{err}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The cheap iteration read: a file from before Throne of Destiny answers from its header, a later
/// one from its `0xFFFF0001` list, each with the data set its header names; neither walks the
/// whole directory, and a file that is not a whole dat is an error.
#[test]
fn read_iterations_answers_both_layouts_and_refuses_what_is_not_a_dat() {
    use dereth_dat::container::LOCAL_DATFILE;
    use dereth_dat::write::DatWriter;
    let dir = std::env::temp_dir().join(format!("dereth-read-iterations-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    let (cell, _) = two_level(0x100, 1593);
    std::fs::write(ClassicDat::Cell.in_dir(&dir), &cell).expect("write");
    let it = DatFile::read_iterations(&ClassicDat::Cell.in_dir(&dir)).expect("reads");
    assert_eq!(it.era, ContainerEra::Classic);
    assert_eq!(
        (it.data_set, it.count, it.highest),
        (CELL_DATFILE, 1593, 1593)
    );

    let later = ModernDat::Local.in_dir(&dir);
    {
        let mut w =
            DatWriter::create(&later, 0x400, LOCAL_DATFILE, 1, 0x400 + 0x400 * 32).expect("create");
        // A list with a gap: 1..=990, then 994.
        let mut list: Vec<u32> = (1..=990).collect();
        list.push(994);
        w.save(
            dereth_dat::ITERATION_LIST,
            &dereth_dat::iteration::encode(&list),
            1,
            0,
            1,
        )
        .expect("save");
        for k in 0..200u32 {
            w.save(DataId(0x2300_0000 + k), b"filler", 1, 1, 1)
                .expect("save");
        }
    }
    let it = DatFile::read_iterations(&later).expect("reads");
    assert_eq!(it.era, ContainerEra::Modern);
    assert_eq!(
        (it.data_set, it.data_subset, it.count, it.highest),
        (LOCAL_DATFILE, 1, 991, 994)
    );
    assert_eq!(
        DatFile::open(&later)
            .expect("opens")
            .iteration_list()
            .expect("list")
            .len(),
        991,
        "the count the full reader decodes"
    );

    // A later file with no list, a truncated file and a file of something else are errors.
    let none = ModernDat::Portal.in_dir(&dir);
    {
        let mut w =
            DatWriter::create(&none, 0x400, PORTAL_DATFILE, 0, 0x400 + 0x400 * 8).expect("create");
        w.save(DataId(0x0600_0001), b"x", 1, 1, 1).expect("save");
    }
    assert!(matches!(
        DatFile::read_iterations(&none),
        Err(DatError::NotFound(_))
    ));
    let bytes = std::fs::read(&later).expect("read");
    let short = dir.join("short.dat");
    std::fs::write(&short, &bytes[..0x200]).expect("write");
    assert!(DatFile::read_iterations(&short).is_err());
    let junk = dir.join("junk.dat");
    std::fs::write(&junk, vec![0x5Au8; 0x2000]).expect("write");
    assert!(DatFile::read_iterations(&junk).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Files already open, wherever they were read from, make the stores a folder makes: an older
/// world alone, an older world with the later interface beside it, and a later world with the
/// older portal beside it. A file in the other layout is refused as it is from a folder.
#[test]
fn files_already_open_make_the_stores_a_folder_makes() {
    use dereth_dat::write::DatWriter;
    let dir = std::env::temp_dir().join(format!("dereth-pre-tod-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (portal, records) = two_level(0x400, 2112);
    let (cell, _) = two_level(0x100, 1593);
    let shared = DataId(records[2].0);
    let only_later = DataId(0x3900_0001);
    let string_table = DataId(0x2300_0001);
    {
        let mut p = DatWriter::create(
            &ModernDat::Portal.in_dir(&dir),
            0x400,
            1,
            0,
            0x400 + 0x400 * 32,
        )
        .expect("create");
        p.save(only_later, b"later only", 1, 1, 1).expect("save");
        p.save(shared, b"later copy", 1, 1, 1).expect("save");
        DatWriter::create(
            &ModernDat::Cell.in_dir(&dir),
            0x100,
            2,
            1,
            0x400 + 0x100 * 16,
        )
        .expect("create");
        let mut l = DatWriter::create(
            &ModernDat::Local.in_dir(&dir),
            0x400,
            3,
            1,
            0x400 + 0x400 * 16,
        )
        .expect("create");
        l.save(string_table, b"strings", 1, 1, 1).expect("save");
    }
    let held = |name: &str, bytes: Vec<u8>| {
        DatFile::from_storage(name.into(), Box::new(bytes)).expect("opens")
    };
    let later = |dat: ModernDat| {
        held(
            dat.file_name(),
            std::fs::read(dat.in_dir(&dir)).expect("read"),
        )
    };

    // An older world alone.
    let older = RetailDatStore::open_classic_with(
        held("portal.dat", portal.clone()),
        held("cell.dat", cell.clone()),
    )
    .expect("opens");
    assert_eq!(older.era(), ContainerEra::Classic);
    assert_eq!(older.read_portal(shared).expect("reads"), records[2].1);
    assert!(!older.has_modern_interface());

    // The later interface beside it.
    let both = older
        .clone()
        .with_modern_interface(
            later(ModernDat::Portal),
            later(ModernDat::Local),
            Some(later(ModernDat::Cell)),
        )
        .expect("attaches");
    assert!(both.has_modern_interface());
    assert_eq!(both.read_portal(shared).expect("reads"), records[2].1);
    assert_eq!(both.read_portal(only_later).expect("reads"), b"later only");
    assert_eq!(both.local().read(string_table).expect("reads"), b"strings");
    assert_eq!(both.era_of(only_later), ContainerEra::Modern);

    // A later world with the older portal beside it; a cell file in the later layout is left out.
    let world = RetailDatStore::open_with(
        later(ModernDat::Portal),
        later(ModernDat::Cell),
        later(ModernDat::Local),
        None,
    )
    .with_classic_files(
        held("portal.dat", portal.clone()),
        Some(later(ModernDat::Cell)),
    )
    .expect("attaches");
    assert_eq!(world.read_portal(shared).expect("reads"), b"later copy");
    let legacy = world.classic_files().expect("the legacy files");
    assert_eq!(legacy.read_portal(shared).expect("reads"), records[2].1);
    assert!(world.interior_files(ContainerEra::Classic).is_none());

    // The wrong layout is refused each way.
    let refused = |r: Result<RetailDatStore, DatError>, expected: ContainerEra| match r {
        Err(DatError::UnexpectedContainerEra { expected: e, .. }) => assert_eq!(e, expected),
        other => panic!("{:?}", other.map(|s| s.era())),
    };
    refused(
        RetailDatStore::open_classic_with(later(ModernDat::Portal), held("cell.dat", cell)),
        ContainerEra::Classic,
    );
    refused(
        older.with_modern_interface(
            held("portal.dat", portal.clone()),
            later(ModernDat::Local),
            None,
        ),
        ContainerEra::Modern,
    );
    refused(
        RetailDatStore::open_with(
            later(ModernDat::Portal),
            later(ModernDat::Cell),
            later(ModernDat::Local),
            None,
        )
        .with_classic_files(later(ModernDat::Portal), None),
        ContainerEra::Classic,
    );
    let _ = std::fs::remove_dir_all(&dir);
}
