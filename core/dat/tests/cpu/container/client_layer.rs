//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The client's own records laid over a store: they answer what the base and the world's overlay
//! do not, the world's overlay wins over them whichever is laid first, and they are in no
//! iteration and in none of the base files the data-patch path writes against. A client layer is
//! written the same for the same records.
//! Fixture: small containers made in a temp folder with the dat writer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_dat::client_layer::{self, ClientLayer};
use dereth_dat::container::{CELL_DATFILE, LOCAL_DATFILE, PORTAL_DATFILE};
use dereth_dat::divine::DbType;
use dereth_dat::overlay::{self, OverlayDir, OverlayWriter};
use dereth_dat::write::DatWriter;
use dereth_dat::{ContainerEra, DatFile, ModernDat, RetailDatStore, ITERATION_LIST};
use dereth_primitives::{AssetSource, DataId};

const KEPT: DataId = DataId(0x0600_0001);
const REPLACED: DataId = DataId(0x0600_0002);
const DELETED: DataId = DataId(0x0600_0003);
const ADDED: DataId = DataId(0x0600_0004);
const CLIENT_ONLY: DataId = DataId(0x0600_0005);

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dereth-client-layer-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("base")).unwrap();
    d
}

/// A later dat set of three files, the portal holding three records, each file ten iterations.
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

/// The world's overlay: revision 11 replaces one portal record, adds one and deletes one.
fn write_overlay(store: &RetailDatStore, dir: &OverlayDir) {
    let portal = store.portal();
    let mut w = OverlayWriter::open_or_create(
        &dir.container(ModernDat::Portal),
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

/// The client's records: one the base lacks, one it holds, one the world replaces and one the
/// world deletes.
fn client(root: &Path) -> ClientLayer {
    let records: BTreeMap<DataId, Vec<u8>> = [
        (CLIENT_ONLY, b"client art".to_vec()),
        (KEPT, b"client kept".to_vec()),
        (REPLACED, b"client replaced".to_vec()),
        (DELETED, b"client deleted".to_vec()),
    ]
    .into_iter()
    .collect();
    let path = root.join("client-layer.dat");
    client_layer::write(&path, ContainerEra::Modern, &records).unwrap();
    let again = root.join("client-layer-again.dat");
    client_layer::write(&again, ContainerEra::Modern, &records).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        bytes,
        std::fs::read(&again).unwrap(),
        "the same records, the same bytes"
    );
    assert!(bytes.len() < 32 * 1024, "{} bytes", bytes.len());
    let built_in: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let layer = ClientLayer::from_static("client-layer", built_in).unwrap();
    assert_eq!(layer.era(), ContainerEra::Modern);
    assert_eq!(layer.base_name(), "client_portal.dat");
    assert_eq!(layer.ids(), vec![KEPT, REPLACED, DELETED, CLIENT_ONLY]);
    layer
}

#[test]
fn the_clients_records_lie_beneath_the_worlds_overlay_and_over_the_base() {
    let root = scratch("order");
    let base = root.join("base");
    base_set(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    write_overlay(&store, &dir);
    let layer = client(&root);

    // Over the base alone the client's records answer, and the base's others are its own.
    let plain = store.clone().with_client_layer(&layer);
    assert_eq!(plain.read_portal(CLIENT_ONLY).unwrap(), b"client art");
    assert_eq!(plain.read_portal(KEPT).unwrap(), b"client kept");
    assert!(plain.exists(CLIENT_ONLY));
    assert_eq!(plain.portal().len(), store.portal().len() + 1);

    // With the world's overlay laid after or before, the world's record wins and its deletion
    // holds; the client's answer what the world leaves alone.
    let after = store
        .clone()
        .with_client_layer(&layer)
        .with_overlay(&dir, Some("world one"))
        .unwrap();
    let before = store
        .clone()
        .with_overlay(&dir, Some("world one"))
        .unwrap()
        .with_client_layer(&layer);
    for s in [&after, &before] {
        assert_eq!(
            s.read_portal(REPLACED).unwrap(),
            b"new",
            "the world's record"
        );
        assert!(s.read_portal(DELETED).is_err(), "the world's deletion");
        assert_eq!(s.read_portal(ADDED).unwrap(), b"added");
        assert_eq!(s.read_portal(CLIENT_ONLY).unwrap(), b"client art");
        assert_eq!(s.read_portal(KEPT).unwrap(), b"client kept");
        assert_eq!(
            s.ids_of(DbType::RenderSurface),
            vec![KEPT, REPLACED, ADDED, CLIENT_ONLY]
        );
        assert_eq!(
            s.portal().len(),
            3 + 1 + 1,
            "the world's three, the iteration list and the client's one"
        );
    }
    // A reload keeps both.
    let mut again = after.clone();
    again.reload().unwrap();
    assert_eq!(again.read_portal(CLIENT_ONLY).unwrap(), b"client art");
    assert_eq!(again.read_portal(REPLACED).unwrap(), b"new");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_clients_records_are_in_no_iteration_and_in_none_of_the_base_files() {
    let root = scratch("iterations");
    let base = root.join("base");
    base_set(&base);
    let store = RetailDatStore::open_dir(&base).unwrap();
    let dir = OverlayDir::new(&root.join("overlay")).unwrap();
    write_overlay(&store, &dir);
    let layer = client(&root);
    let world = store.clone().with_overlay(&dir, Some("world one")).unwrap();
    for (without, with) in [
        (store.clone(), store.clone().with_client_layer(&layer)),
        (world.clone(), world.clone().with_client_layer(&layer)),
    ] {
        for t in ModernDat::REQUIRED {
            let (a, b) = (
                without.target_file(t).unwrap(),
                with.target_file(t).unwrap(),
            );
            assert_eq!(
                a.iteration_list().unwrap(),
                b.iteration_list().unwrap(),
                "{t:?}"
            );
            assert_eq!(a.header_iteration(), b.header_iteration());
            assert_eq!(overlay::fingerprint(a), overlay::fingerprint(b));
            // The data-patch path writes against the base, which has none of the client's.
            assert!(!b.base().contains(CLIENT_ONLY));
            assert_eq!(b.base().len(), a.base().len());
        }
        assert!(!with.portal().world_contains(CLIENT_ONLY));
        assert!(with.portal().contains(CLIENT_ONLY));
    }
    // A world's overlay is not a client layer.
    let world_container = DatFile::open(&dir.container(ModernDat::Portal)).unwrap();
    assert!(ClientLayer::from_file(world_container).is_err());
    let _ = std::fs::remove_dir_all(&root);
}
