//! The static scene over Holtburg draws a picture of the world: every window slot meshes and
//! every cell has a texture, the frame is lit below the horizon and the sky above it, two frames
//! of an unmoved camera are identical, and scenery grows only on full-detail blocks.
//! "The frame is not black" is the claim that catches a scene whose meshes are built and never
//! handed to the landblock window.
//! Fixture: the four retail dats in `$DERETH_TEST_DAT_DIR` (their absence fails) and a software
//! device at 800x600 (its absence skips with a printed line).

#![cfg(gpu)]

use dereth_dat::RetailDatStore;
use dereth_scene::world_scene::SceneReads;
use {
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

/// The retail store, or **fail**: absent dats are a missing oracle, not a reason to pass, so the
/// type offers no way to skip.
fn store() -> RetailDatStore {
    crate::common::dat_store()
}

/// Oracle: the retail dats. `mid_radius` 3 is `mid_width` 7, and Holtburg is far enough inside the
/// 255x255 world that all 49 of those blocks exist, so the window fills completely. Every one of
/// them must mesh, and the merge cache must produce a texture for every distinct cell key —
/// a cell with no texture is a cell that draws whatever was last bound.
#[test]
fn the_holtburg_scene_meshes_the_whole_window_and_texture_maps_every_cell() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let cfg = SceneConfig::default();
    assert_eq!(
        cfg.landblock, DEFAULT_LANDBLOCK,
        "the default scene is Holtburg"
    );
    let scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    let s = scene.draw.stats;

    let width = 2 * cfg.land_radius + 1;
    assert_eq!(
        s.blocks_meshed,
        (width * width) as usize,
        "every window slot has geometry"
    );
    // A few hundred distinct merge keys over 49 blocks: far more than one (which would mean the key
    // is not varying) and far fewer than 49*64 (which would mean the cache is not caching).
    assert!(
        s.terrain_surfaces > 50,
        "only {} distinct terrain surfaces",
        s.terrain_surfaces
    );
    assert!(
        s.terrain_surfaces < 49 * 64,
        "{} surfaces is one per cell",
        s.terrain_surfaces
    );
    assert_eq!(
        s.object_batches_untextured, 0,
        "every object surface resolved to pixels"
    );

    // Holtburg is a town: the centre block and its eight neighbours carry buildings, the designers'
    // static objects, and generated scenery. All three paths must produce something.
    assert!(s.buildings > 0, "Holtburg has no buildings");
    assert!(
        s.static_objects > 0,
        "Holtburg has no landblock-info objects"
    );
    assert!(
        s.scenery_objects > 0,
        "no scenery grew on the 3x3 around Holtburg"
    );
    // `object_triangles` is what *this frame draws*, a function of where the camera stands;
    // `object_triangles_resident` is what the bake holds. Both are checked, and so is the
    // inequality between them: the default camera is 45 m above the block, so most of the window
    // is past its detail bands, and a scene that drew everything it held would mean the per-frame
    // level selection is not running.
    assert!(
        s.object_triangles_resident > 10_000,
        "only {} object triangles baked",
        s.object_triangles_resident
    );
    assert!(
        s.object_triangles > 0,
        "the frame drew no object triangles at all"
    );
    assert!(
        s.object_triangles < s.object_triangles_resident,
        "the frame drew every triangle it holds ({} of {}), so no level was selected",
        s.object_triangles,
        s.object_triangles_resident
    );
}

/// Behaviour: rendering.scene.the-static-holtburg-scene-draws-lit-and-stable
/// Oracle: the retail dats, rendered. The fixed default camera looks north and down over Holtburg
/// from 45 m above its highest vertex, so the terrain fills the lower two thirds of the frame and
/// the sky dome and its cloud decks the top.
///
/// This is the assertion that "it rendered Dereth" rather than "it rendered": a frame whose terrain
/// vanished still passes a byte-identical capture.
#[test]
fn a_frame_of_the_static_scene_is_mostly_lit_below_the_horizon() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");

    // Without this the very first frame executes a command list pointing into an upload buffer
    // that `Gpu::upload_bytes` released when it grew -- see `WorldScene::reserve_upload_arena`.
    scene.reserve_upload_arena(&mut gpu).expect("arena");
    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    assert_eq!((image.width, image.height), (800, 600));

    let px = image.bgra.as_chunks::<4>().0;
    let lit = |row_lo: u32, row_hi: u32| -> f32 {
        let (mut on, mut total) = (0usize, 0usize);
        for y in row_lo..row_hi {
            for x in 0..image.width {
                let p = px[(y * image.width + x) as usize];
                total += 1;
                if p[0] > 8 || p[1] > 8 || p[2] > 8 {
                    on += 1;
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            on as f32 / total as f32
        }
    };
    // The bottom half is ground, in front of the camera and inside the loaded window.
    let ground = lit(300, 600);
    assert!(
        ground > 0.9,
        "only {:.0}% of the lower half is lit",
        ground * 100.0
    );
    // The top eighth is above the horizon: the sky draw runs before the first block, so these
    // pixels are the sky dome and its cloud decks.
    let sky = lit(0, 75);
    assert!(
        sky > 0.9,
        "only {:.0}% of the top of the frame is lit; the sky pass drew nothing",
        sky * 100.0
    );
}

/// Oracle: the same scene, twice in one process. The scene is static and the camera has not moved,
/// so two frames must be pixel-identical — the property the dat tier's headless capture test then
/// measures across three processes.
#[test]
fn two_frames_of_an_unmoved_camera_are_pixel_identical() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
    scene.reserve_upload_arena(&mut gpu).expect("arena");
    let mut shots = Vec::new();
    for _ in 0..2 {
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        shots.push(gpu.capture().expect("capture"));
    }
    assert_eq!(shots[0], shots[1]);
}

/// Oracle: the landblock setup's full-detail guard — scenery only grows on a block meshed at
/// eight cells per side. The LOD rings reduce every block outside the innermost to 4, 2 or 1, so
/// a scenery radius wider than the full-detail ring must not add objects; generating no scenery
/// for them is the client's own behaviour.
#[test]
fn scenery_only_grows_on_full_detail_blocks() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let near = SceneConfig {
        scenery_radius: 1,
        ..SceneConfig::default()
    };
    let far = SceneConfig {
        scenery_radius: 3,
        ..SceneConfig::default()
    };
    let a = WorldScene::load(&store, &mut gpu, near)
        .expect("loads")
        .draw
        .stats;
    let b = WorldScene::load(&store, &mut gpu, far)
        .expect("loads")
        .draw
        .stats;
    // The block orientation keeps ring 0 and ring 1 at eight cells per side, so widening the radius
    // past ring 1 buys buildings and static objects but no generated scenery.
    assert_eq!(
        a.scenery_objects, b.scenery_objects,
        "scenery escaped the full-detail rings"
    );
    assert!(b.buildings >= a.buildings);
}
