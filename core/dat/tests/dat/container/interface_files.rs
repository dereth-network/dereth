//! Beside the February 2005 world, the later interface's own files answer every record both sets
//! carry with the later set's record, while the world's store keeps answering with the older one.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`) beside the
//! end-of-retail files (`DERETH_TEST_DAT_DIR`).

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{AssetSource, DataId};

/// The images both portals hold under one id with different pictures: the ones an interface's
/// own art and a world's pictures can disagree on.
fn colliding_images(world: &RetailDatStore, later: &RetailDatStore) -> Vec<DataId> {
    let old = world.portal();
    later
        .portal()
        .iter_entries()
        .map(|(id, _)| id)
        .filter(|id| id.0 >> 24 == 0x06 && old.contains(*id))
        .filter(|id| old.read(*id).ok() != later.portal().read(*id).ok())
        .collect()
}

/// Behaviour: presentation.era.the-retail-interface-draws-its-own-art-over-an-older-world
#[test]
fn the_interface_files_beside_an_older_world_answer_with_the_later_pictures() {
    let old = dereth_dat::testing::classic_dat_dir().unwrap_or_else(|| {
        panic!(
            "{}",
            dereth_dat::testing::classic_shortfall().unwrap_or_default()
        )
    });
    let later_dir = dereth_dat::testing::dat_dir();
    let world = Arc::new(
        RetailDatStore::open_classic_with_modern(&old, &later_dir)
            .expect("the February 2005 dats beside the end-of-retail ones"),
    );
    let later = RetailDatStore::open_dir(&later_dir).expect("the end-of-retail dats");
    let ids = colliding_images(&world, &later);
    // Calibration: the two sets do disagree, so the test can tell them apart.
    assert!(ids.len() > 100, "only {} images differ", ids.len());

    let interface = world.interface_files();
    for id in ids {
        let mine = interface.read(id).expect("the interface reads it");
        assert_eq!(Some(&mine), later.read(id).ok().as_ref(), "{id:?}");
        assert_ne!(
            Some(mine),
            world.read(id).ok(),
            "{id:?}: the world's is the older one"
        );
    }

    // A later world's interface is the world's own files.
    let only_later = Arc::new(later);
    assert!(Arc::ptr_eq(&only_later.interface_files(), &only_later));
}
