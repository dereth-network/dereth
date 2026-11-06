//! A world's overlay over this game's files: a record it replaces, adds or deletes reads through
//! both of the client's ways of reading one, the file underneath is never changed, and a folder that
//! is not for these files is refused.

use std::path::Path;
use std::sync::Arc;

use auberean_dat::disk::controller::OPEN_CREATE;
use auberean_dat::disk::{Opened, SharedStore, StoreSet};
use auberean_dat::overlay::{role_of, WorldOverlay, WORLD_FOLDER};
use auberean_dat::{DatFile, QualifiedId, RecordType};
use dereth_dat::overlay::{OverlayDir, OverlayError};
use dereth_dat::{ClassicDat, DatFolder, MemoryFolder, ModernDat};
use dereth_primitives::DataId;

use crate::disk::support::{spec, OneFile};

/// A record in the cell file's landscape range: the file underneath holds these.
const KEPT: u32 = 0x0101_FFFE;
const REPLACED: u32 = 0x0102_FFFE;
const DELETED: u32 = 0x0103_FFFE;
const BLOCK_CELLS: [u32; 2] = [0x0104_0100, 0x0104_FFFE];
const ADDED: u32 = 0x0105_FFFE;

/// A cell file of this game's layout with those records, data versions 1 and 2.
fn cell_file(records: &[(u32, &[u8])]) -> DatFile {
    let mut files = OneFile::new(&[]);
    let s = SharedStore::new(Box::new(|| 0x5000_0000));
    let mut sp = spec(OPEN_CREATE, 0x400 + 256 * 0x100, 0x100);
    sp.data_set = 2;
    sp.data_subset = 1;
    assert_eq!(s.init_file(&mut files, &mut sp), Opened::Ready);
    let mut v = auberean_dat::disk::VersionSet::new();
    v.add(1);
    v.add(2);
    assert!(s.lock().save_iteration_list(&mut v));
    for (id, b) in records {
        assert!(s.save_data(*id, b));
    }
    s.lock().close();
    DatFile::from_bytes(files.bytes()).expect("the file reads")
}

fn base() -> DatFile {
    cell_file(&[
        (KEPT, b"kept"),
        (REPLACED, b"as retail"),
        (DELETED, b"deleted"),
        (BLOCK_CELLS[0], b"a room"),
        (BLOCK_CELLS[1], b"a room's block"),
    ])
}

/// A folder in memory holding an overlay of `world` over `base`.
fn overlay_over(base: &DatFile, world: &str, iteration: Option<u32>) -> Arc<MemoryFolder> {
    let folder = Arc::new(MemoryFolder::new(Path::new("world-overlay")));
    let dir = OverlayDir::in_folder(folder.clone()).unwrap();
    let mut w = dir
        .writer(ModernDat::Cell, base.shared(), "the cell file", world, 0)
        .unwrap();
    w.put(DataId(REPLACED), b"the world's own", 1, 0, 0)
        .unwrap();
    w.put(DataId(ADDED), b"added", 1, 0, 0).unwrap();
    w.tombstone(DataId(DELETED), 0, 3).unwrap();
    w.tombstone(DataId(BLOCK_CELLS[0]), 0xFFFF_0000, 3).unwrap();
    if let Some(i) = iteration {
        w.add_iteration(i, 0).unwrap();
    }
    w.flush(0).unwrap();
    folder
}

fn q(id: u32) -> QualifiedId {
    QualifiedId {
        id,
        kind: RecordType::LANDBLOCK_INFO.0,
    }
}

fn stores(f: &DatFile) -> StoreSet {
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 0x5000_0000);
    StoreSet::over_files(&[Arc::new(f.clone())], &clock)
}

/// **A replaced, an added and a deleted record read the same by id and through the stores the
/// data cache reads through; every other record is the file's.**
///
/// Behaviour: data.overlay.reads-agree
#[test]
fn a_record_the_overlay_changes_reads_the_same_by_both_ways_of_reading() {
    let base = base();
    let overlay = WorldOverlay::in_folder(overlay_over(&base, "a world", None)).unwrap();
    assert_eq!(overlay.world(), "a world");
    let f = overlay.over(base.clone()).unwrap();
    let s = stores(&f);
    let read = |id: u32| (f.read(id).ok(), s.load_data(q(id)));
    let both = |b: &[u8]| (Some(b.to_vec()), Some(b.to_vec()));
    assert_eq!(read(KEPT), both(b"kept"));
    assert_eq!(read(REPLACED), both(b"the world's own"));
    assert_eq!(read(ADDED), both(b"added"));
    assert_eq!(read(DELETED), (None, None));
    assert_eq!(read(BLOCK_CELLS[0]), (None, None), "by its block's mask");
    assert_eq!(read(BLOCK_CELLS[1]), (None, None), "by its block's mask");
    assert!(f.entry(ADDED).is_some() && f.entry(DELETED).is_none());
    assert_eq!(
        f.entries_in(0x0101_0000, 0x0105_FFFF)
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        [KEPT, REPLACED, ADDED]
    );
    assert!(s.is_on_disk(q(ADDED)) && !s.is_on_disk(q(DELETED)));
    // A record's head reads from where the record is, the overlay's or the file's.
    assert!(f.read_start(REPLACED).unwrap().starts_with(b"the world's"));
    assert!(f.read_start(KEPT).unwrap().starts_with(b"kept"));
    assert!(matches!(
        f.read_start(DELETED),
        Err(dereth_dat::DatError::NotFound(_))
    ));
    // The file under it holds what it held.
    assert_eq!(base.read(REPLACED).unwrap(), b"as retail");
    assert_eq!(f.base().read(REPLACED).unwrap(), b"as retail");
    assert_eq!(base.read(DELETED).unwrap(), b"deleted");
}

/// **With no overlay the file's stores are made as they were, and no record of a folder with
/// nothing over this file is taken.**
///
/// Behaviour: data.overlay.none-reads-the-files
#[test]
fn a_file_with_no_overlay_over_it_reads_as_it_did() {
    let base = base();
    let plain = stores(&base);
    assert_eq!(plain.load_data(q(REPLACED)).unwrap(), b"as retail");
    assert!(base.shared().layer().is_none());
    // A folder holding an overlay of another file's role only.
    let folder = Arc::new(MemoryFolder::new(Path::new("world-overlay")));
    let dir = OverlayDir::in_folder(folder.clone()).unwrap();
    let portal = cell_file(&[]);
    dir.writer(
        ModernDat::Portal,
        portal.shared(),
        "the portal file",
        "a world",
        0,
    )
    .unwrap()
    .flush(0)
    .unwrap();
    let f = WorldOverlay::in_folder(folder).unwrap().over(base).unwrap();
    assert!(f.shared().layer().is_none());
    assert_eq!(f.read(REPLACED).unwrap(), b"as retail");
}

/// **An overlay made against other files is refused; so is another world's container, a folder with
/// no overlay in it and a folder holding the install's own files.**
///
/// Behaviour: data.overlay.refused-when-not-for-these-files
#[test]
fn a_folder_not_for_these_files_is_refused_and_never_read() {
    let made_over = base();
    let other = cell_file(&[(KEPT, b"another install")]);
    let folder = overlay_over(&made_over, "a world", None);
    let refused = WorldOverlay::in_folder(folder.clone()).unwrap().over(other);
    assert!(matches!(refused, Err(OverlayError::BaseMismatch { .. })));
    // The same records, written in another order, are the same files.
    let again = cell_file(&[
        (BLOCK_CELLS[1], b"a room's block"),
        (BLOCK_CELLS[0], b"a room"),
        (DELETED, b"deleted"),
        (REPLACED, b"as retail"),
        (KEPT, b"kept"),
    ]);
    let taken = WorldOverlay::in_folder(folder)
        .unwrap()
        .over(again)
        .unwrap();
    assert_eq!(taken.read(REPLACED).unwrap(), b"the world's own");

    let empty = Arc::new(MemoryFolder::new(Path::new("nothing")));
    assert!(matches!(
        WorldOverlay::in_folder(empty),
        Err(OverlayError::NotOverlay(_))
    ));
    let install = Arc::new(MemoryFolder::new(Path::new("install")));
    drop(install.create(ClassicDat::Portal.file_name()).unwrap());
    assert!(matches!(
        WorldOverlay::in_folder(install),
        Err(OverlayError::BaseFolder(_))
    ));
}

/// **The overlay's revisions are among the file's data versions, however they are read.**
///
/// Behaviour: data.overlay.revisions-are-data-versions
#[test]
fn a_revision_of_the_overlay_is_among_the_data_versions_the_client_reports() {
    let base = base();
    let without = WorldOverlay::in_folder(overlay_over(&base, "a world", None)).unwrap();
    let f = without.over(base.clone()).unwrap();
    assert_eq!(f.iterations().unwrap(), [1, 2]);
    let held = |f: &DatFile| {
        let s = stores(f);
        let v = s.by_slot.iter().flatten().next().unwrap();
        let on_file = v.lock().load_iteration_list().unwrap().values;
        assert_eq!(on_file, v.held.lock().unwrap().values, "kept as stored");
        on_file
    };
    assert_eq!(held(&f), [1, 2]);

    let with = WorldOverlay::in_folder(overlay_over(&base, "a world", Some(3))).unwrap();
    let f = with.over(base.clone()).unwrap();
    assert_eq!(f.iterations().unwrap(), [1, 2, 3]);
    assert_eq!(held(&f), [1, 2, 3]);
    assert_eq!(base.iterations().unwrap(), [1, 2]);
    assert_eq!(role_of(&f), Some(ModernDat::Cell));
    // Another region's file is no file an overlay lies over.
    let mut other = OneFile::new(&[]);
    let s = SharedStore::new(Box::new(|| 0));
    let mut sp = spec(OPEN_CREATE, 0x400 + 256 * 0x100, 0x100);
    sp.data_set = 2;
    sp.data_subset = 2;
    assert_eq!(s.init_file(&mut other, &mut sp), Opened::Ready);
    s.lock().close();
    let region_two = DatFile::from_bytes(other.bytes()).unwrap();
    assert_eq!(role_of(&region_two), None);
    assert!(with.over(region_two).unwrap().shared().layer().is_none());
}

/// **A world with no overlay folder, or an empty one, has no overlay and its records are the
/// install's; what isn't an overlay in that folder is refused.**
///
/// Behaviour: data.overlay.none-reads-the-files
#[test]
fn a_world_with_nothing_in_its_overlay_folder_has_no_overlay() {
    let world = std::env::temp_dir().join(format!("auberean-of-world-{}", std::process::id()));
    std::fs::remove_dir_all(&world).ok();
    std::fs::create_dir_all(&world).unwrap();
    assert!(
        WorldOverlay::of_world(&world).unwrap().is_none(),
        "no folder"
    );
    let folder = world.join(WORLD_FOLDER);
    std::fs::create_dir_all(&folder).unwrap();
    assert!(
        WorldOverlay::of_world(&world).unwrap().is_none(),
        "an empty folder"
    );
    std::fs::write(folder.join("overlay_cell.dat"), b"not a container").unwrap();
    assert!(
        WorldOverlay::of_world(&world).is_err(),
        "a container that doesn't read is never taken for no overlay"
    );
    std::fs::remove_dir_all(&world).ok();
}
