//! A world built while its patch was still arriving is built again from the patched files once the
//! patch ends: what it read of the patched records before is let go. An update exchange that
//! changed nothing, as a retail world's does, leaves the world standing as it was.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_physics::source::LandSource as _;
use dereth_primitives::{DataId, LandblockId};
use dereth_protocol::admin::DddData;

use super::*;

/// The block the world is built at, and its land record.
const BLOCK: u16 = 0xA9B4;
const LAND: DataId = DataId(0xA9B4_FFFF);

fn retail_store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

/// The client over the retail files, its world's records kept in `overlay`, with the world built
/// at [`BLOCK`] with a body, as it is once the server has placed the player there.
fn in_the_world(overlay: &std::path::Path) -> App<NullShell> {
    let cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        overlay_dat_dir: Some(overlay.to_path_buf()),
        ..Config::default()
    };
    let mut app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(retail_store()),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::sim_present::SimPresentation::new(64, 64))),
    )
    .expect("bring-up");
    app.defer_static_scene(crate::scene::SceneConfig {
        landblock: BLOCK,
        character: true,
        ..crate::scene::SceneConfig::default()
    });
    build_world(&mut app);
    app
}

/// Build the world the scene names from the files as they are now, as world entry does.
fn build_world(app: &mut App<NullShell>) {
    let cfg = app.pending_scene.take().expect("a scene armed");
    let store = Arc::clone(&app.store);
    app.present
        .load_world(&store, cfg, &mut app.world)
        .expect("the world builds");
}

/// The land heights the world's collision reads for [`BLOCK`].
fn heights(app: &App<NullShell>) -> Vec<u8> {
    let c = app
        .world
        .as_ref()
        .and_then(|w| w.character.as_ref())
        .expect("a world with a body");
    c.land()
        .landblock(LandblockId(BLOCK))
        .expect("the block's land")
        .height
        .to_vec()
}

/// Patch [`LAND`] to `bytes` through the data-patch path, and end the patch as `DDD_EndDDD` does.
fn patch_land(app: &mut App<NullShell>, bytes: &[u8]) {
    let iteration = app.store.cell().entry(LAND).map_or(0, |e| e.iteration) + 1;
    let m = DddData {
        dat_file_type: 1,
        dat_file_id: 2,
        resource_type: 0,
        resource_id: LAND.raw(),
        iteration,
        compressed: 0,
        version: 1,
        data_size: u32::try_from(bytes.len()).expect("small") + 4,
        data: bytes.to_vec(),
    };
    let (outcome, _) = app.ddd.on_data(&m);
    assert!(outcome.wrote(), "the patch is written: {outcome:?}");
    app.ddd_invalidation = Some(app.ddd.on_end());
    app.invalidate_after_ddd();
}

/// Behaviour: net.dat-patch.every-view-reads-the-patched-records
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_world_built_before_its_patch_lands_reads_the_patched_land_afterward() {
    let overlay = dereth_dat::testing::ScratchDir::new("patch-rebuild").expect("a scratch folder");
    let mut app = in_the_world(overlay.path());
    let before = heights(&app);
    // The block's land, every height a step up, where the record holds them.
    let mut land = app
        .store
        .cell()
        .read(LAND)
        .expect("the block's land record");
    let at = land
        .windows(before.len())
        .position(|w| w == before.as_slice())
        .expect("the record holds the heights the world read");
    for h in &mut land[at..at + before.len()] {
        *h = h.wrapping_add(1);
    }
    let want: Vec<u8> = before.iter().map(|h| h.wrapping_add(1)).collect();
    assert_ne!(before, want);

    patch_land(&mut app, &land);
    assert!(
        app.world.is_none(),
        "the world read before the patch is let go, not kept with the land it read"
    );
    // World entry's own step builds it again, around where the player stands, from the files as
    // they are now.
    build_world(&mut app);
    assert_eq!(heights(&app), want, "the world reads the patched land");
}

/// Behaviour: net.dat-patch.every-view-reads-the-patched-records
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn an_update_exchange_that_changed_nothing_leaves_the_world_standing() {
    let overlay = dereth_dat::testing::ScratchDir::new("patch-none").expect("a scratch folder");
    let mut app = in_the_world(overlay.path());
    let before = heights(&app);
    let generation = app.store_generation();
    // The server says the files are up to date: an exchange with nothing in it.
    app.ddd_invalidation = Some(app.ddd.on_end());
    app.invalidate_after_ddd();
    assert_eq!(app.store_generation(), generation, "nothing reopened");
    assert!(app.world.is_some(), "the world stands");
    assert!(app.pending_scene.is_none(), "nothing is built again");
    assert_eq!(heights(&app), before);
}
