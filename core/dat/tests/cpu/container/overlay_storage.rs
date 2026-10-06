//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The writer and the overlay folder through the storage interface: a container written into
//! memory is byte for byte the one written to disk, an overlay kept in a folder in memory is the
//! one kept on disk and reads the same over a store, and it is reopened, found, named and removed
//! through its folder.
//! Fixture: small containers made in a temp folder and in memory with the dat writer.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::folder::{DatFolder, MemoryFile, MemoryFolder};
use dereth_dat::overlay::{container_name, OverlayDir, OverlayError};
use dereth_dat::write::DatWriter;
use dereth_dat::{ContainerEra, DatFile, ModernDat, RetailDatStore, ITERATION_LIST};
use dereth_primitives::DataId;

const KEPT: DataId = DataId(0x0600_0001);
const REPLACED: DataId = DataId(0x0600_0002);
const DELETED: DataId = DataId(0x0600_0003);
const ADDED: DataId = DataId(0x0600_0004);
const LATER: DataId = DataId(0x0600_0005);

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "dereth-overlay-storage-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("base")).unwrap();
    d
}

fn base_set(dir: &Path) {
    let make = |name: &str, block: u32, set: u32, subset: u32, records: &[(DataId, &[u8])]| {
        let mut w =
            DatWriter::create(&dir.join(name), block, set, subset, 0x400 + block * 64).unwrap();
        let all: Vec<u32> = (1..=10).collect();
        w.save(
            ITERATION_LIST,
            &dereth_dat::iteration::encode(&all),
            1,
            0,
            1,
        )
        .unwrap();
        for (id, bytes) in records {
            w.save(*id, bytes, 1, 5, 1).unwrap();
        }
    };
    make(
        ModernDat::Portal.file_name(),
        0x400,
        PORTAL_DATFILE,
        0,
        &[(KEPT, b"kept"), (REPLACED, b"old"), (DELETED, b"doomed")],
    );
    make(ModernDat::Cell.file_name(), 0x100, CELL_DATFILE, 1, &[]);
    make(ModernDat::Local.file_name(), 0x400, LOCAL_DATFILE, 1, &[]);
}

/// The same edits, in the same order, on any writer: records added (enough to grow the file and
/// split the directory), replaced, deleted, and an iteration recorded.
fn edit(w: &mut DatWriter) {
    w.save(
        ITERATION_LIST,
        &dereth_dat::iteration::encode(&[1, 2]),
        1,
        0,
        7,
    )
    .unwrap();
    for i in 0..150u32 {
        let bytes = vec![u8::try_from(i % 251).unwrap(); 40 + (i as usize * 37) % 900];
        w.save(DataId(0x0600_1000 + i), &bytes, 1, 3, 7).unwrap();
    }
    w.save(DataId(0x0600_1005), b"replaced", 1, 4, 8).unwrap();
    assert!(w.delete_data(DataId(0x0600_1006), 0).unwrap());
    w.add_iteration(11, 9).unwrap();
}

#[test]
fn a_container_written_through_memory_storage_is_byte_for_byte_the_one_written_to_disk() {
    let root = scratch("bytes");
    let path = root.join("a.dat");
    let mut disk = DatWriter::create(&path, 0x400, PORTAL_DATFILE, 0, 0x400 * 17).unwrap();
    edit(&mut disk);
    drop(disk);

    let memory = MemoryFile::default();
    let mut mem = DatWriter::create_in(
        PathBuf::from("a.dat"),
        Box::new(memory.clone()),
        0x400,
        PORTAL_DATFILE,
        0,
        0x400 * 17,
    )
    .unwrap();
    edit(&mut mem);
    drop(mem);
    let on_disk = std::fs::read(&path).unwrap();
    assert!(on_disk.len() > 0x400 * 17, "the file grew");
    assert_eq!(memory.bytes(), on_disk);

    // Opened again through its storage, the container is written the same way as one on disk.
    let mut disk = DatWriter::open(&path).unwrap();
    disk.save(DataId(0x0600_2000), b"again", 1, 12, 10).unwrap();
    drop(disk);
    let mut mem = DatWriter::open_in(PathBuf::from("a.dat"), Box::new(memory.clone())).unwrap();
    mem.save(DataId(0x0600_2000), b"again", 1, 12, 10).unwrap();
    drop(mem);
    assert_eq!(memory.bytes(), std::fs::read(&path).unwrap());

    // And it reads as any container does.
    let f = DatFile::from_storage("a.dat".into(), Box::new(memory)).unwrap();
    assert_eq!(f.read(DataId(0x0600_1005)).unwrap(), b"replaced");
    assert!(f.read(DataId(0x0600_1006)).is_err());
    assert_eq!(f.iteration_list().unwrap(), vec![1, 2, 11]);
    let _ = std::fs::remove_dir_all(&root);
}

/// The overlay a world was given, written through `dir`'s own writer.
fn write_overlay(store: &RetailDatStore, dir: &OverlayDir) {
    let portal = store.portal();
    let mut w = dir
        .writer(
            ModernDat::Portal,
            portal,
            "client_portal.dat",
            "world one",
            1,
        )
        .unwrap();
    w.save(portal, REPLACED, b"new", 1, 11, 1).unwrap();
    w.save(portal, ADDED, b"added", 1, 11, 1).unwrap();
    w.tombstone(DELETED, 0, 11).unwrap();
    w.add_iteration(11, 1).unwrap();
    w.flush(1).unwrap();
}

#[test]
fn an_overlay_kept_in_a_folder_in_memory_is_the_one_kept_on_disk_and_is_read_and_reopened_through_it(
) {
    let root = scratch("folder");
    let base = root.join("base");
    base_set(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();

    let on_disk = OverlayDir::new(&root.join("overlay")).unwrap();
    write_overlay(&store, &on_disk);
    let memory = MemoryFolder::new(Path::new("overlays/world-one"));
    let in_memory = OverlayDir::in_folder(Arc::new(memory.clone())).unwrap();
    assert!(!in_memory.holds(ModernDat::Portal) && in_memory.containers().is_empty());
    write_overlay(&store, &in_memory);
    assert_eq!(
        memory
            .file(container_name(ModernDat::Portal))
            .unwrap()
            .bytes(),
        std::fs::read(on_disk.container(ModernDat::Portal)).unwrap(),
        "the same container, byte for byte"
    );
    assert_eq!(memory.names(), ["overlay_portal.dat"]);
    assert_eq!(
        in_memory.containers(),
        [PathBuf::from("overlays/world-one/overlay_portal.dat")]
    );
    assert_eq!(in_memory.world_key().as_deref(), Some("world one"));
    assert_eq!(in_memory.base_era(), Some(ContainerEra::Modern));

    let s = store
        .clone()
        .with_overlay(&in_memory, Some("world one"))
        .unwrap();
    assert!(s.has_overlay());
    assert_eq!(s.read_portal(KEPT).unwrap(), b"kept");
    assert_eq!(s.read_portal(REPLACED).unwrap(), b"new");
    assert_eq!(s.read_portal(ADDED).unwrap(), b"added");
    assert!(s.read_portal(DELETED).is_err());
    assert_eq!(
        s.portal().iteration_list().unwrap(),
        (1..=11).collect::<Vec<_>>()
    );
    assert!(matches!(
        store.clone().with_overlay(&in_memory, Some("world two")),
        Err(OverlayError::OtherWorld { .. })
    ));

    // A later patch, written into the same folder, is read once the overlay is reopened from it.
    let mut w = in_memory
        .writer(
            ModernDat::Portal,
            store.portal(),
            "client_portal.dat",
            "world one",
            1,
        )
        .unwrap();
    w.save(store.portal(), LATER, b"later", 1, 12, 1).unwrap();
    w.add_iteration(12, 1).unwrap();
    w.flush(1).unwrap();
    drop(w);
    assert!(
        s.read_portal(LATER).is_err(),
        "the store read before it stands"
    );
    let mut again = s.clone();
    again.reload().unwrap();
    assert_eq!(again.read_portal(LATER).unwrap(), b"later");
    assert_eq!(again.read_portal(REPLACED).unwrap(), b"new");

    // Removed, the folder holds no overlay, and the base is read alone.
    in_memory.remove(ModernDat::Portal).unwrap();
    assert!(in_memory.containers().is_empty() && in_memory.world_key().is_none());
    let bare = store.clone().with_overlay(&in_memory, None).unwrap();
    assert_eq!(bare.read_portal(REPLACED).unwrap(), b"old");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_folder_holding_base_files_is_no_overlay_folder_wherever_it_is_kept() {
    let memory = MemoryFolder::new(Path::new("not-an-overlay"));
    let mut f = memory.create(ModernDat::Portal.file_name()).unwrap();
    f.write_all_at(0, b"x").unwrap();
    assert!(matches!(
        OverlayDir::in_folder(Arc::new(memory)),
        Err(OverlayError::BaseFolder(p)) if p == Path::new("not-an-overlay")
    ));
}
