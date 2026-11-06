//! A world's overlay over the end-of-retail cell file and portal file: what a world changes reads
//! through the client's two ways of reading a record, and what it does not change is the file's.

use std::sync::Arc;

use auberean_dat::disk::StoreSet;
use auberean_dat::locate::{DatName, FILE_SET};
use auberean_dat::overlay::{role_of, WorldOverlay};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::{DatFile, QualifiedId, RecordType};
use dereth_dat::overlay::OverlayDir;
use dereth_dat::{MemoryFolder, ModernDat};
use dereth_primitives::DataId;

fn q(id: u32) -> QualifiedId {
    QualifiedId {
        id,
        kind: RecordType::LANDBLOCK_INFO.0,
    }
}

/// **A world's overlay over the real cell file: a landblock's placed objects replaced, another's
/// deleted, read by id and through the stores; every other record as the file holds it, and the
/// file's own data versions.**
///
/// Behaviour: data.overlay.over-the-retail-files
#[test]
fn a_world_replaces_and_deletes_records_of_the_retail_cell_file() {
    let path = DatName::Cell(1).in_dir(&require_install(TARGET));
    let base = DatFile::open(&path).expect("the cell file");
    let infos: Vec<u32> = base
        .entries()
        .map(|e| e.id)
        .filter(|id| id & 0xFFFF == 0xFFFE)
        .take(3)
        .collect();
    let (replaced, deleted, kept) = (infos[0], infos[1], infos[2]);
    let added = (replaced & 0xFFFF_0000) | 0xFFF0;
    assert!(
        base.entry(added).is_none(),
        "a record the file does not hold"
    );

    let folder = Arc::new(MemoryFolder::new(std::path::Path::new("a-world")));
    let dir = OverlayDir::in_folder(folder.clone()).unwrap();
    let name = FILE_SET.name(ModernDat::Cell).unwrap();
    let mut w = dir
        .writer(ModernDat::Cell, base.shared(), name, "a world", 0)
        .unwrap();
    let mut changed = base.read(replaced).unwrap();
    changed.extend_from_slice(b"and more");
    w.put(DataId(replaced), &changed, 1, 0, 0).unwrap();
    w.put(DataId(added), b"a new record", 1, 0, 0).unwrap();
    w.tombstone(DataId(deleted), 0, 0).unwrap();
    w.flush(0).unwrap();

    let overlay = WorldOverlay::in_folder(folder).unwrap();
    let f = overlay.over(DatFile::open(&path).unwrap()).unwrap();
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 0x5000_0000);
    let s = StoreSet::over_files(&[Arc::new(f.clone())], &clock);

    assert_eq!(f.read(replaced).unwrap(), changed);
    assert_eq!(s.load_data(q(replaced)).unwrap(), changed);
    assert_eq!(f.read(added).unwrap(), b"a new record");
    assert_eq!(s.load_data(q(added)).unwrap(), b"a new record");
    assert!(f.read(deleted).is_err() && f.entry(deleted).is_none());
    assert!(s.load_data(q(deleted)).is_none());
    assert_eq!(f.read(kept).unwrap(), base.read(kept).unwrap());
    assert_eq!(s.load_data(q(kept)).unwrap(), base.read(kept).unwrap());

    // A world with no revision of its own reports the file's data versions, in the file's order.
    assert_eq!(f.iterations().unwrap(), base.iterations().unwrap());
    let held = s.by_slot.iter().flatten().next().unwrap();
    assert_eq!(
        held.lock().load_iteration_list().unwrap().values,
        base.iterations().unwrap()
    );
    // Every other record of the file reads as it did, and the directory has one more, one less.
    assert_eq!(f.len(), base.len());
    // Each of the set's files plays its role by its header.
    for (name, role) in [
        (DatName::Portal, ModernDat::Portal),
        (DatName::Cell(1), ModernDat::Cell),
        (DatName::HighRes, ModernDat::HighRes),
        (DatName::Local("English".to_owned()), ModernDat::Local),
    ] {
        let file = DatFile::open(&name.in_dir(&require_install(TARGET))).unwrap();
        assert_eq!(role_of(&file), Some(role), "{name:?}");
    }
    let other = DatFile::open(&DatName::Cell(2).in_dir(&require_install(TARGET))).unwrap();
    assert_eq!(role_of(&other), None, "the other region's");
    for e in base.entries().step_by(997) {
        if e.id != replaced && e.id != deleted {
            assert_eq!(
                f.read(e.id).unwrap(),
                base.read(e.id).unwrap(),
                "{:#x}",
                e.id
            );
        }
    }
    assert!(
        base.read(deleted).is_ok(),
        "the file under it still holds it"
    );
}
