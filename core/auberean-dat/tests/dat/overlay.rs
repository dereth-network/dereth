//! A world's overlay over the end-of-retail cell, portal and English files: what a world changes
//! reads through the client's two ways of reading a record, and what it does not change is the
//! file's.

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

/// **A world's overlay over the real portal and English files: a saved state and a string table
/// replaced, a saved state added, read by id and through the stores; every other record and each
/// file's data versions as the file holds them.**
///
/// Behaviour: data.overlay.over-the-retail-files
#[test]
fn a_world_replaces_and_adds_records_of_the_retail_portal_and_english_files() {
    let dir = require_install(TARGET);
    let portal_path = DatName::Portal.in_dir(&dir);
    let english_path = DatName::Local("English".to_owned()).in_dir(&dir);
    let portal = DatFile::open(&portal_path).expect("the portal file");
    let english = DatFile::open(&english_path).expect("the English file");
    let in_range = |f: &DatFile, r: std::ops::Range<u32>| -> Vec<u32> {
        f.entries()
            .map(|e| e.id)
            .filter(|id| r.contains(id))
            .collect()
    };
    let states = in_range(&portal, 0x6F00_0000..0x7000_0000);
    let (state, kept_state) = (states[0], states[1]);
    let added = states.iter().max().unwrap() + 1;
    assert!(portal.entry(added).is_none());
    let tables = in_range(&english, 0x2500_0000..0x2700_0000);
    let (table, kept_table) = (tables[0], tables[1]);

    let folder = Arc::new(MemoryFolder::new(std::path::Path::new("a-world")));
    let overlay_dir = OverlayDir::in_folder(folder.clone()).unwrap();
    let put = |role: ModernDat, base: &DatFile, records: &[(u32, Vec<u8>)]| {
        let name = FILE_SET.name(role).unwrap();
        let mut w = overlay_dir
            .writer(role, base.shared(), name, "a world", 0)
            .unwrap();
        for (id, bytes) in records {
            w.put(DataId(*id), bytes, 1, 0, 0).unwrap();
        }
        w.flush(0).unwrap();
    };
    let mut changed_state = portal.read(state).unwrap();
    changed_state.extend_from_slice(b"and more");
    let mut changed_table = english.read(table).unwrap();
    changed_table.extend_from_slice(b"and more");
    put(
        ModernDat::Portal,
        &portal,
        &[
            (state, changed_state.clone()),
            (added, b"a new state".to_vec()),
        ],
    );
    put(
        ModernDat::Local,
        &english,
        &[(table, changed_table.clone())],
    );

    let overlay = WorldOverlay::in_folder(folder).unwrap();
    let p = overlay.over(DatFile::open(&portal_path).unwrap()).unwrap();
    let e = overlay.over(DatFile::open(&english_path).unwrap()).unwrap();
    let clock: Arc<dyn Fn() -> u32 + Send + Sync> = Arc::new(|| 0x5000_0000);
    let s = StoreSet::over_files(&[Arc::new(p.clone()), Arc::new(e.clone())], &clock);
    let q = |id, kind: RecordType| QualifiedId { id, kind: kind.0 };

    assert_eq!(p.read(state).unwrap(), changed_state);
    assert_eq!(
        s.load_data(q(state, RecordType::SCRIPT_STATE)).unwrap(),
        changed_state
    );
    assert_eq!(p.read(added).unwrap(), b"a new state");
    assert_eq!(
        s.load_data(q(added, RecordType::SCRIPT_STATE)).unwrap(),
        b"a new state"
    );
    assert_eq!(e.read(table).unwrap(), changed_table);
    assert_eq!(
        s.load_data(q(table, RecordType::STRING_TABLE)).unwrap(),
        changed_table
    );
    assert_eq!(
        p.read(kept_state).unwrap(),
        portal.read(kept_state).unwrap()
    );
    assert_eq!(
        s.load_data(q(kept_state, RecordType::SCRIPT_STATE))
            .unwrap(),
        portal.read(kept_state).unwrap()
    );
    assert_eq!(
        e.read(kept_table).unwrap(),
        english.read(kept_table).unwrap()
    );
    assert_eq!(
        s.load_data(q(kept_table, RecordType::STRING_TABLE))
            .unwrap(),
        english.read(kept_table).unwrap()
    );
    assert_eq!(p.iterations().unwrap(), portal.iterations().unwrap());
    assert_eq!(e.iterations().unwrap(), english.iterations().unwrap());
    assert_eq!(p.len(), portal.len() + 1, "one record more");
    assert_eq!(e.len(), english.len());
}
