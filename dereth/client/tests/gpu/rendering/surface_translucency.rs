//! Authored surface translucency reaches the frame: a surface with the translucent flag set is
//! drawn with vertex alpha truncate((1 - translucency) * 255), blended source-alpha over
//! inverse-source-alpha, and every other surface keeps alpha 0xFF.
//!
//! Frames are compared in pairs that differ only in `SceneConfig::surface_translucency` (the
//! computed byte against an opaque 0xFF): at the horizon only the sky band changes, and looking
//! straight down, with nothing translucent in view, nothing changes. The exact alpha arithmetic
//! and both mesh-building paths are unit tests in `src/world_scene.rs`.
//! Fixture: Holtburg (landblock 0xA9B4) from the retail dats on a software device; every fixture
//! path is an `expect`, never a skip.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::world::{SceneConfig, SceneStats, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::Gpu;

/// Missing DAT inputs fail; they do not turn this test into a passing skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(640, 640)
}

type Shot = (Vec<u8>, u32, u32);

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Shot {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let (w, h) = (image.width, image.height);
    (image.to_rgba(), w, h)
}

/// Changed pixels and the rectangle that bounds them.
fn diff(a: &Shot, b: &Shot) -> (usize, (u32, u32, u32, u32)) {
    assert_eq!((a.1, a.2), (b.1, b.2));
    let (w, h) = (a.1, a.2);
    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.0[i..i + 4] != b.0[i..i + 4] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (n, (x0, y0, x1, y1))
}

/// Build one scene, settle it and capture a frame. `honour` is the only difference between the
/// two arms of every pair below.
fn render(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    honour: bool,
    pitch_dy: f32,
) -> (SceneStats, Shot) {
    let cfg = SceneConfig {
        surface_translucency: honour,
        ..populated()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the landscape loads");
    scene.set_weather_enabled(false);
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    for i in 0..8u32 {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();
    if pitch_dy != 0.0 {
        scene.look(0.0, pitch_dy);
    }
    let stats = scene.draw.stats;
    let frame = shot(&mut scene, gpu);
    // Each arm releases its scene's textures after capture: scenes dropped without releasing
    // them accumulate descriptors until the 2048-slot heap is exhausted. Releasing after the
    // capture keeps this bookkeeping out of the pixels just measured.
    scene.release_textures(gpu);
    (stats, frame)
}

/// Holtburg terrain, scenery, buildings, interior statics and a body over landblock 0xA9B4.
/// Automatic degrade adjustment is disabled by the current default; weather and particles are off.
/// Each pair shares that configuration and the same settling ticks.
fn populated() -> SceneConfig {
    SceneConfig {
        land_radius: 1,
        scenery_radius: 1,
        particles: false,
        ..SceneConfig::default()
    }
}

/// Clamp at the current camera::PITCH_LIMIT (88.97 degrees), almost vertically downward.
/// This test station's resulting frame excludes the sky.
const LOOK_STRAIGHT_DOWN: f32 = 1000.0;

/// Declared change band y<300: its cutoff lies below the horizon near y=283. The lower
/// region must remain byte-identical. That geometric bound is a frame-level control; it does
/// not independently classify the material of every surface outside the band.
const SKY_BAND_BOTTOM: u32 = 300;

/// Changed pixels above and below [`SKY_BAND_BOTTOM`].
fn split_by_band(a: &Shot, b: &Shot) -> (usize, usize) {
    let (w, h) = (a.1, a.2);
    let (mut above, mut below) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a.0[i..i + 4] != b.0[i..i + 4] {
                if y < SKY_BAND_BOTTOM {
                    above += 1;
                } else {
                    below += 1;
                }
            }
        }
    }
    (above, below)
}

/// Behaviour: rendering.translucency.authored-surface-translucency-changes-only-translucent-pixels
/// Four runs share the block, body, eight settling ticks and inactive automatic degrade governor.
/// Each pair differs only in SceneConfig::surface_translucency; the second pair also uses a
/// downward camera orientation. The switch changes submitted alpha, not surface resolution.
///
/// At the horizon, some pixels must differ and all differences must lie above y=300. Looking
/// down with sky absent, the pair must be identical. The positive case demonstrates that this
/// same pixel comparison can see a change, making the zero-difference control meaningful.
#[test]
fn the_pair_differs_only_where_a_translucent_surface_is_drawn() {
    let store = store();
    let mut gpu = warp();

    let (opaque_stats, opaque_frame) = render(&store, &mut gpu, false, 0.0);
    let (alpha_stats, alpha_frame) = render(&store, &mut gpu, true, 0.0);

    eprintln!(
        "pair: {} distinct surface groups resolved, {} of them translucent; \
         {} object batches over {} triangles; {} sky object(s)",
        alpha_stats.surfaces_resolved,
        alpha_stats.surfaces_translucent,
        alpha_stats.object_batches,
        alpha_stats.object_triangles,
        alpha_stats.sky_objects
    );

    // The two arms must have built the *same* scene -- only the alpha byte may differ.
    assert_eq!(
        (
            opaque_stats.surfaces_resolved,
            opaque_stats.object_triangles
        ),
        (alpha_stats.surfaces_resolved, alpha_stats.object_triangles),
        "the two arms built different scenes, so the differential means nothing"
    );
    assert!(
        alpha_stats.surfaces_resolved > 0,
        "the scene resolved no surface at all; a zero differential below would be meaningless"
    );
    assert!(
        alpha_stats.surfaces_translucent > 0,
        "{} surface groups resolved and not one is translucent -- this frame cannot see the change",
        alpha_stats.surfaces_resolved
    );
    // The switch changes the written alpha byte, not the resolved surface statistics.
    assert_eq!(
        opaque_stats.surfaces_translucent, alpha_stats.surfaces_translucent,
        "the switch changed the surface resolution, not just the vertex byte"
    );

    let (changed, rect) = diff(&opaque_frame, &alpha_frame);
    let (above, below) = split_by_band(&opaque_frame, &alpha_frame);
    eprintln!(
        "differential: {changed} of {} pixels changed, bounded by {rect:?}; \
         {above} above y={SKY_BAND_BOTTOM} and {below} below it",
        (alpha_frame.1 * alpha_frame.2) as usize
    );
    assert!(
        changed > 0,
        "{} translucent surface groups resolved and not one pixel moved",
        alpha_stats.surfaces_translucent
    );
    assert_eq!(
        below, 0,
        "{below} pixels moved below the declared sky band -- a surface with TRANSLUCENT clear \
         did not hold still"
    );
    assert_eq!(above, changed, "the split lost pixels");

    // 2. The negative, from the same instrument. Straight down: no sky, and every surface the
    //    camera can see has the flag clear.
    let (_, down_opaque) = render(&store, &mut gpu, false, LOOK_STRAIGHT_DOWN);
    let (_, down_alpha) = render(&store, &mut gpu, true, LOOK_STRAIGHT_DOWN);
    let (down_changed, down_rect) = diff(&down_opaque, &down_alpha);
    eprintln!("control: looking straight down, {down_changed} pixels changed");
    assert_eq!(
        down_changed, 0,
        "{down_changed} pixels moved with nothing translucent in frame, bounded by {down_rect:?}"
    );
}
