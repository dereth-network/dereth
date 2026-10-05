//! A placed static whose model has an animation of its own is drawn in its current pose. The
//! butterfly that scenery grows west of Shoushi (block `0xD655`) bobs and beats its wings; with
//! the camera parked in front of it, two frames half a second apart differ where it flies, and its
//! drawn parts are somewhere else. Fixture: the retail dats around Shoushi, drawn on the graphics
//! device at a pinned time of day with no body.

#![cfg(gpu)]

use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, Frame, LocalTime, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The butterfly's setup.
const BUTTERFLY: DataId = DataId(0x0200_0493);

/// Shoushi's block with no body, the time of day pinned so the sky and the land's lighting
/// cannot move between the two frames.
fn cfg() -> SceneConfig {
    SceneConfig {
        character: false,
        landblock: 0xD655,
        scenery_radius: 1,
        time_of_day: Some(0.5),
        // Nothing else in view moves: no emitters.
        particles: false,
        ..SceneConfig::default()
    }
}

/// The app's own per-frame order, `frames` times from `*t`, handing back the last capture.
fn run(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    t: &mut f64,
    frames: usize,
) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    let mut rgba = Vec::new();
    for _ in 0..frames {
        *t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(*t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    rgba
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// The butterfly's drawn parts this frame.
fn butterfly(scene: &WorldScene) -> Vec<Frame> {
    let found: Vec<Vec<Frame>> = scene
        .draw
        .animated_static_parts()
        .into_iter()
        .filter(|(setup, _)| *setup == BUTTERFLY)
        .map(|(_, parts)| parts)
        .collect();
    assert!(
        !found.is_empty(),
        "no butterfly is drawn posed around Shoushi; the animated statics are {:?}",
        scene
            .draw
            .animated_static_parts()
            .iter()
            .map(|(s, _)| *s)
            .collect::<Vec<_>>()
    );
    // The one west of the town, nearest block 0xD655's (89, 65), in the renderer's space: the
    // south-west corner of the viewer's block is its origin.
    let (vx, vy) = scene
        .world
        .streamer
        .window
        .viewer_block()
        .expect("the window has a viewer block");
    #[allow(clippy::cast_precision_loss)] // block indices
    let at = Vec3::new(
        89.2 + (0xD6 - vx) as f32 * 192.0,
        65.1 + (0x55 - vy) as f32 * 192.0,
        52.0,
    );
    found
        .into_iter()
        .min_by(|a, b| {
            let d = |p: &[Frame]| dist(p[0].origin, at);
            d(a).total_cmp(&d(b))
        })
        .expect("one butterfly")
}

/// Behaviour: rendering.scenery.an-animated-static-is-drawn-in-its-current-pose
#[test]
fn the_butterfly_west_of_shoushi_is_drawn_in_a_new_place_half_a_second_later() {
    let store = crate::common::dats();
    let (w, h) = (640u32, 480u32);
    let mut gpu = crate::common::test_gpu(w, h);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg()).expect("the landscape loads");
    // And no rain.
    scene.set_weather_enabled(false);
    let mut t = 0.0f64;
    // Two frames bring the block's live objects up and pose them.
    run(&store, &mut gpu, &mut scene, &mut t, 2);
    assert!(
        scene.draw.stats.animated_hosts >= 1,
        "no animated static is live around Shoushi"
    );
    let start = butterfly(&scene);
    #[allow(clippy::cast_precision_loss)] // two parts
    let n = start.len() as f32;
    let centre = Vec3::new(
        start.iter().map(|p| p.origin.x).sum::<f32>() / n,
        start.iter().map(|p| p.origin.y).sum::<f32>() / n,
        start.iter().map(|p| p.origin.z).sum::<f32>() / n,
    );
    // Two metres south of the butterfly and a metre and a half above it, looking down at it so
    // that its wings face the camera.
    scene.camera.position = Vec3::new(centre.x, centre.y - 2.0, centre.z + 1.6);
    scene.camera.yaw = 0.0;
    // atan(1.6 / 2.0).
    scene.camera.pitch = -0.6747;
    let first = run(&store, &mut gpu, &mut scene, &mut t, 1);
    let posed = butterfly(&scene);
    // Half a second on: fifteen frames at the headless step.
    let second = run(&store, &mut gpu, &mut scene, &mut t, 15);
    let later = butterfly(&scene);

    let moved = posed
        .iter()
        .zip(&later)
        .map(|(a, b)| dist(a.origin, b.origin))
        .fold(0.0f32, f32::max);
    assert!(
        moved > 0.05,
        "the butterfly's parts moved {moved} m in half a second"
    );
    // The two pictures differ where the butterfly is: the middle of the screen. Everything else
    // in the view is still, so a frozen butterfly leaves the two the same.
    let (cx, cy) = (w / 2, h / 2);
    let half = 120u32;
    let mut changed = 0usize;
    for y in cy - half..cy + half {
        for x in cx - half..cx + half {
            let i = ((y * w + x) * 4) as usize;
            let d = (0..3)
                .map(|c| first[i + c].abs_diff(second[i + c]))
                .max()
                .unwrap_or(0);
            if d > 24 {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 200,
        "only {changed} pixels around the butterfly changed in half a second"
    );
}
