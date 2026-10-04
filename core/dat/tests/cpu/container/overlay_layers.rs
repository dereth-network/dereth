//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! A world's overlay laid over a store: a record it adds or replaces is read from it, a record a
//! tombstone covers is not there (a whole landblock under a family tombstone), every other record
//! is the base's, the iterations are the base's and the overlay's together, and the base files
//! are never written. An overlay is refused over another base and for another world.
//! Fixture: small containers made in a temp folder with the dat writer.

use std::path::{Path, PathBuf};

use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::divine::DbType;
use dereth_dat::overlay::{self, OverlayDir, OverlayError, OverlayWriter};
use dereth_dat::write::DatWriter;
use dereth_dat::{ContainerEra, DatFile, RetailDat, RetailDatStore, ITERATION_LIST};
use dereth_primitives::{AssetSource, DataId};

const KEPT: DataId = DataId(0x0600_0001);
const REPLACED: DataId = DataId(0x0600_0002);
const DELETED: DataId = DataId(0x0600_0003);
const ADDED: DataId = DataId(0x0600_0004);
const LANDBLOCK: DataId = DataId(0xA9B4_FFFF);
const ITS_CELL: DataId = DataId(0xA9B4_0100);
const OTHER_LANDBLOCK: DataId = DataId(0xA9B5_FFFF);

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
        RetailDat::Portal.file_name(),
        0x400,
        PORTAL_DATFILE,
        0,
        &[(KEPT, b"kept"), (REPLACED, b"old"), (DELETED, b"doomed")],
    );
    make(
        RetailDat::Cell.file_name(),
        0x100,
        CELL_DATFILE,
        1,
        &[
            (LANDBLOCK, b"land"),
            (ITS_CELL, b"room"),
            (OTHER_LANDBLOCK, b"elsewhere"),
        ],
    );
    make(RetailDat::Local.file_name(), 0x400, LOCAL_DATFILE, 1, &[]);
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dereth-overlay-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("base")).unwrap();
    d
}

fn sha(path: &Path) -> Vec<u8> {
    overlay::record_hash(&std::fs::read(path).unwrap()).to_vec()
}

/// The overlay this world was given: one revision (11) adding, replacing and deleting a portal
/// record, and one (12) deleting a whole landblock.
fn write_overlay(store: &RetailDatStore, dir: &OverlayDir, world: &str) {
    let portal = store.portal();
    let mut w = OverlayWriter::open_or_create(
        &dir.container(RetailDat::Portal),
        portal,
        "client_portal.dat",
        world,
        1,
    )
    .unwrap();
    w.save(portal, REPLACED, b"new", 1, 11, 1).unwrap();
    w.save(portal, ADDED, b"added", 1, 11, 1).unwrap();
    w.tombstone(DELETED, 0, 11).unwrap();
    w.add_iteration(11, 1).unwrap();
    w.flush(1).unwrap();
    let cell = store.cell();
    let mut c = OverlayWriter::open_or_create(
        &dir.container(RetailDat::Cell),
        cell,
        "client_cell_1.dat",
        world,
        1,
    )
    .unwrap();
    c.tombstone(LANDBLOCK, 0xFFFF_0000, 12).unwrap();
    c.add_iteration(11, 1).unwrap();
    c.add_iteration(12, 1).unwrap();
    c.flush(1).unwrap();
}

#[test]
fn an_overlay_adds_replaces_and_deletes_over_the_base_and_never_writes_it() {
    let root = scratch("layers");
    let base = root.join("base");
    base_set(&base);
    let before: Vec<Vec<u8>> = RetailDat::REQUIRED
        .iter()
        .map(|d| sha(&d.in_dir(&base)))
        .collect();
    dereth_dat::protect_install(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    write_overlay(&store, &dir, "world one");

    let s = store.clone().with_overlay(&dir, Some("world one")).unwrap();
    assert!(s.has_overlay());
    assert_eq!(s.read_portal(KEPT).unwrap(), b"kept");
    assert_eq!(s.read_portal(REPLACED).unwrap(), b"new");
    assert_eq!(s.read_portal(ADDED).unwrap(), b"added");
    assert!(s.read_portal(DELETED).is_err());
    assert!(!s.exists(DELETED) && s.exists(ADDED) && s.exists(KEPT));
    assert_eq!(s.resolve(ADDED).map(|r| r.0), Some(DbType::RenderSurface));
    assert_eq!(s.resolve(DELETED), None);
    assert_eq!(
        s.ids_of(DbType::RenderSurface),
        vec![KEPT, REPLACED, ADDED],
        "ascending, the deleted one gone"
    );
    assert_eq!(s.era_of(ADDED), ContainerEra::Tod);
    // The whole landblock family is gone; the other landblock stands.
    assert!(s.read_cell(LANDBLOCK).is_err() && s.read_cell(ITS_CELL).is_err());
    assert_eq!(s.read_cell(OTHER_LANDBLOCK).unwrap(), b"elsewhere");
    assert_eq!(
        s.cell().len(),
        1 + 1,
        "the other landblock and the iteration list"
    );
    assert_eq!(
        s.portal().len(),
        3 + 1,
        "three world records and the iteration list"
    );
    // The iterations are the base's run and the overlay's revisions together.
    assert_eq!(
        s.portal().iteration_list().unwrap(),
        (1..=11).collect::<Vec<_>>()
    );
    assert_eq!(
        s.cell().iteration_list().unwrap(),
        (1..=12).collect::<Vec<_>>()
    );
    // The overlay's own records are never world records.
    assert!(!s.exists(overlay::MANIFEST) && !s.exists(overlay::TOMBSTONES));
    // The store without the overlay is the base, and the base files were never written.
    assert_eq!(store.read_portal(REPLACED).unwrap(), b"old");
    let after: Vec<Vec<u8>> = RetailDat::REQUIRED
        .iter()
        .map(|d| sha(&d.in_dir(&base)))
        .collect();
    assert_eq!(before, after);
    // A reload reads the overlay again.
    let mut again = s.clone();
    again.reload().unwrap();
    assert_eq!(again.read_portal(REPLACED).unwrap(), b"new");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_overlay_is_refused_over_another_base_for_another_world_and_in_a_base_folder() {
    let root = scratch("refusals");
    let base = root.join("base");
    base_set(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    write_overlay(&store, &dir, "world one");
    assert_eq!(dir.world_key().as_deref(), Some("world one"));

    assert!(matches!(
        store.clone().with_overlay(&dir, Some("world two")),
        Err(OverlayError::OtherWorld { .. })
    ));
    assert!(matches!(
        OverlayDir::new(&base),
        Err(OverlayError::BaseFolder(_))
    ));
    // Another base: the same files, one record changed.
    {
        let mut w = DatWriter::open(&RetailDat::Portal.in_dir(&base)).unwrap();
        w.save(KEPT, b"changed", 1, 6, 2).unwrap();
    }
    let changed = RetailDatStore::open_dir(&base).unwrap();
    let refused = changed.with_overlay(&dir, None);
    assert!(
        matches!(refused, Err(OverlayError::BaseMismatch { .. })),
        "{refused:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_record_older_than_the_one_in_force_is_refused_and_the_overlay_copy_wins_over_a_tombstone() {
    let root = scratch("older");
    let base = root.join("base");
    base_set(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    let portal: DatFile = store.portal().clone();
    let mut w = OverlayWriter::open_or_create(
        &dir.container(RetailDat::Portal),
        &portal,
        "client_portal.dat",
        "w",
        1,
    )
    .unwrap();
    assert_eq!(
        w.save(&portal, KEPT, b"older", 1, 4, 1).unwrap(),
        dereth_dat::SaveOutcome::RefusedOlderIteration
    );
    w.tombstone(DELETED, 0, 11).unwrap();
    // Deleted, then sent again in a later revision: the overlay's copy is read.
    w.save(&portal, DELETED, b"back", 1, 12, 1).unwrap();
    w.flush(1).unwrap();
    drop(w);
    let s = store.with_overlay(&dir, Some("w")).unwrap();
    assert_eq!(s.read_portal(KEPT).unwrap(), b"kept");
    assert_eq!(s.read_portal(DELETED).unwrap(), b"back");
    let _ = std::fs::remove_dir_all(&root);
}
