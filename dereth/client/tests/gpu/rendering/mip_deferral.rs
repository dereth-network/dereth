//! A landscape whose compressed textures' mip chains are built off the main thread ends up exactly
//! the landscape that builds them in the upload.
//!
//! With a streaming budget the scene asks `dereth_client::mip_worker` for each chain; a block whose
//! bake wanted one that was not ready is handed back, kept as terrain alone, and baked again once
//! the chain is done. Streamed until nothing is left queued, it must hold the same blocks,
//! scenery, buildings, statics and triangles as the same window baked synchronously -- and the
//! deferral must actually have happened, or this proves nothing.
//! Fixture: the retail dats' Holtburg window (landblock 0xA9B4) on a software device.
//!
//! Behaviour: none (an equivalence of two bake paths inside the client, not a retail claim)

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use std::time::{Duration, Instant};

use dereth_client::present::Scene as _;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_render::device::Gpu;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn device() -> Gpu {
    crate::common::test_gpu(320, 240)
}

fn config(budget: Option<Duration>) -> SceneConfig {
    // Holtburg and the blocks around it: a town's worth of buildings and statics.
    SceneConfig {
        landblock: 0xA9B4,
        land_radius: 2,
        scenery_radius: 1,
        character: false,
        stream_budget: budget,
        // Every object at full detail: which level a part draws at follows the camera, and the
        // two scenes pick their levels at different moments of the load.
        degrade_levels: false,
        part_degrades: false,
        ..SceneConfig::default()
    }
}

#[test]
fn a_landscape_baked_behind_the_mip_worker_matches_one_baked_in_place() {
    let store = store();
    // The deferred scene first, on a fresh device, so every chain it needs is genuinely cold.
    let mut gpu = device();
    let mut deferred = WorldScene::load(&store, &mut gpu, config(Some(Duration::from_millis(4))))
        .expect("the deferred scene loads");
    let deadline = Instant::now() + Duration::from_secs(120);
    while deferred.pending_slot_count() > 0 {
        assert!(
            Instant::now() < deadline,
            "the deferred scene never finished streaming"
        );
        deferred.stream(&store, &mut gpu).expect("streams");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        deferred.deferred_surfaces() > 0,
        "no bake waited on the worker, so this compared the synchronous path with itself"
    );
    let d = deferred.census();
    deferred.release_textures(&mut gpu);
    drop(deferred);

    let mut direct =
        WorldScene::load(&store, &mut gpu, config(None)).expect("the direct scene loads");
    assert_eq!(
        direct.pending_slot_count(),
        0,
        "an unbudgeted load builds everything at once"
    );
    assert_eq!(direct.deferred_surfaces(), 0, "and never defers");
    let s = direct.census();
    direct.release_textures(&mut gpu);

    assert_eq!(
        (
            d.blocks_meshed,
            d.scenery_objects,
            d.buildings,
            d.static_objects,
            d.object_triangles
        ),
        (
            s.blocks_meshed,
            s.scenery_objects,
            s.buildings,
            s.static_objects,
            s.object_triangles
        ),
        "deferred (blocks, scenery, buildings, statics, triangles) against direct"
    );
    assert!(
        s.scenery_objects + s.buildings + s.static_objects > 0,
        "the window has objects to compare"
    );
}
