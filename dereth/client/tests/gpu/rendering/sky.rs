//! The sky: it is drawn from the region's sky objects, it is not black, and the day cycle moves
//! it. Every gfx id the region's day groups can name resolves to triangles; two times of day, and
//! a running clock, give different frames with the sun vector, ambient level and objects present
//! changing with the time; the sunlight vector's length is the direction brightness (normalising
//! it flattens the cycle); the shipped data uses the sky-object property bits; the far plane is
//! multiplied for the sky pass; and the UV scroll follows elapsed time, not frame count.
//! Fixture: the retail region record `0x13000000` and the meshes it names, drawn on a software
//! device.

#![cfg(gpu)]

use dereth_dat::RetailDatStore;
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The retail store, or **fail**: a missing oracle must not read as a pass.
fn store() -> RetailDatStore {
    crate::common::dat_store()
}

/// The smallest scene that still has a sky: one landblock, no scenery, no body.
fn scene_at(store: &std::sync::Arc<RetailDatStore>, gpu: &mut Gpu, t: f32) -> WorldScene {
    let cfg = SceneConfig {
        land_radius: 0,
        scenery_radius: 0,
        character: false,
        time_of_day: Some(t),
        ..SceneConfig::default()
    };
    WorldScene::load(store, gpu, cfg).expect("the landscape loads")
}

fn draw_and_count(scene: &WorldScene, gpu: &mut Gpu) -> (usize, Vec<u8>) {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let rgba = image.to_rgba();
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count();
    (lit, rgba)
}

// ---------------------------------------------------------------------------------------------
// 1. It is objects.
// ---------------------------------------------------------------------------------------------

/// **The sky is a set of ordinary objects.** Every gfx id any of the region's day groups can name
/// — 21 of them, setups and bare graphics objects alike — resolves to triangles through the same path
/// scenery uses. A shortfall here means an id the data can ask for has no geometry, which is the
/// `missing_geometry` counter's whole reason for existing.
///
/// Oracle: the retail region `0x13000000` and the retail portal dat.
#[test]
fn every_sky_object_the_region_names_resolves_to_triangles() {
    let store = store();
    let mut gpu = crate::common::test_gpu(400, 300);
    let store = std::sync::Arc::new(store);
    let scene = scene_at(&store, &mut gpu, 0.5);
    let s = scene.draw.stats.sky_stats;

    assert!(s.gfx_ids > 0, "the region names no sky objects at all");
    assert_eq!(
        s.gfx_ids,
        s.gfx_ids_drawable,
        "{} of {} sky gfx ids produced no triangles",
        s.gfx_ids - s.gfx_ids_drawable,
        s.gfx_ids
    );
    assert!(s.triangles > 0);
    assert_eq!(
        s.missing_geometry, 0,
        "the sky lookup asked for geometry the cache did not have"
    );
    assert!(s.live_objects > 0, "no sky object is present at noon");
    assert_eq!(
        s.live_objects,
        s.pass0_objects + s.pass1_objects,
        "an object is in neither pass: {} live, {} + {}",
        s.live_objects,
        s.pass0_objects,
        s.pass1_objects
    );
    eprintln!(
        "{} gfx ids, {} batches, {} triangles; {} live at noon ({} pass 0, {} pass 1)",
        s.gfx_ids, s.batches, s.triangles, s.live_objects, s.pass0_objects, s.pass1_objects
    );
}

/// **The shipped region uses the sky-object property bits, including the second pass.** It carries
/// 232 sky objects across 20 day groups with the values 0, 2, 4, 5 and **13**, so all three
/// inferred bits are exercised and a fourth, bit 3, is used and undocumented.
///
/// Oracle: `client_portal.dat`'s region record, read directly.
#[test]
fn the_shipped_region_does_use_the_sky_object_property_bits() {
    let store = store();
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let sky = region.sky_info.as_ref().expect("the region has sky info");
    let mut seen = std::collections::BTreeMap::<u32, usize>::new();
    for g in &sky.day_groups {
        for o in &g.sky_objects {
            *seen.entry(o.properties).or_default() += 1;
        }
    }
    eprintln!("SkyObject::properties over the shipped region: {seen:?}");
    assert!(
        seen.keys().any(|p| p & 1 != 0),
        "bit 0 (pass 1 / after_sky_cell) never appears: {seen:?}"
    );
    assert!(
        seen.keys().any(|p| p & 2 != 0),
        "bit 1 (hide under a fog override) never appears"
    );
    assert!(
        seen.keys().any(|p| p & 4 != 0),
        "bit 2 (weather / viewer-locked) never appears"
    );
    assert!(
        seen.keys().any(|p| p & 8 != 0),
        "bit 3 appears in the shipped data (properties 13) and has no documented meaning"
    );
}

// ---------------------------------------------------------------------------------------------
// 2 and 3. Not black, and the day cycle moves it.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.sky.the-sky-is-drawn-and-the-day-cycle-moves-it
/// **The acceptance line.** The sky is not black, and moving the time of day moves it.
///
/// The camera is left where `WorldScene::load` puts it — high over Holtburg, looking north and
/// slightly down — so the top of the frame is sky and the bottom is ground. Two times of day are
/// drawn and compared pixel for pixel.
///
/// Oracle: the retail data through the sky and lighting lookups, and a software GPU device.
#[test]
fn the_sky_is_not_black_and_the_day_cycle_moves_it() {
    let store = store();
    let mut gpu = crate::common::test_gpu(400, 300);
    let store = std::sync::Arc::new(store);

    let noon = scene_at(&store, &mut gpu, 0.5);
    let (lit_noon, frame_noon) = draw_and_count(&noon, &mut gpu);
    let total = 400 * 300;
    assert!(
        lit_noon > total / 4,
        "only {lit_noon} of {total} pixels are lit at noon: the frame is mostly black"
    );

    // The top eighth of the frame is above the horizon from this camera, and is sky alone.
    let sky_band = &frame_noon[..(400 * 300 / 8) * 4];
    let lit_sky = sky_band
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 8 || p[1] > 8 || p[2] > 8)
        .count();
    assert!(
        lit_sky > (400 * 300 / 8) * 9 / 10,
        "{lit_sky} of {} pixels above the horizon are lit: the sky is black",
        400 * 300 / 8
    );

    let night = scene_at(&store, &mut gpu, 0.02);
    let (lit_night, frame_night) = draw_and_count(&night, &mut gpu);
    assert_ne!(
        frame_noon, frame_night,
        "the two times of day produce identical frames"
    );
    let differing = frame_noon
        .as_chunks::<4>()
        .0
        .iter()
        .zip(frame_night.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > total / 10,
        "only {differing} of {total} pixels differ between noon and midnight"
    );
    eprintln!("noon {lit_noon} lit, midnight {lit_night} lit, {differing} pixels differ");

    // And the *reasons* moved, not just the pixels: the sun swung and the ambient changed.
    let (a, b) = (noon.landscape_lighting(), night.landscape_lighting());
    assert_ne!(a.sunlight, b.sunlight, "the sun did not move");
    assert!(
        (a.ambient_level - b.ambient_level).abs() > 1e-4
            || a.ambient_color != b.ambient_color
            || a.sunlight_color != b.sunlight_color,
        "the ambient did not change between noon and midnight"
    );
}

/// The same claim over a **running clock** rather than two separate loads: game time and the
/// landscape light tick move the sky and relight the terrain from inside the frame loop.
///
/// This is the path a session actually takes, and the one the two-scene test above cannot reach:
/// Landscape-lighting updates re-run `calc_lighting` on every loaded block, which
/// this crate does by asking the window to regenerate each slot's mesh.
///
/// Oracle: the retail region's own `SkyTimeOfDay` ramp, and a WARP device.
#[test]
fn advancing_the_clock_relights_the_world_from_inside_the_frame_loop() {
    let store = store();
    let mut gpu = crate::common::test_gpu(400, 300);
    let store = std::sync::Arc::new(store);
    let mut scene = scene_at(&store, &mut gpu, 0.25);
    let before_light = scene.landscape_lighting();
    let (_, before) = draw_and_count(&scene, &mut gpu);

    // A quarter of an in-game day. One day is 7620 real seconds, so this is 1,900 s of clock and
    // 127 of the region's 15 s light ticks; the step is large because 57,000 frames at the headless
    // 1/30 s quantum is not a test. Nothing here depends on the step size: the clock reads `now`
    // and the tick compares against it.
    let mut now = 0.0f64;
    let step = 38.0;
    for _ in 0..50 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            dereth_primitives::LocalTime(now),
            #[allow(clippy::cast_possible_truncation)]
            {
                step as f32
            },
        );
        // The light tick queues a mesh rebuild; `App::frame` flushes it here, outside the frame
        // bracket, because `Gpu::upload_texture` runs a command list of its own.
        scene.stream(&store, &mut gpu).expect("stream");
    }

    let (_, t) = (scene.game_time().0, scene.game_time().2);
    assert!(
        (t - 0.25).abs() > 0.2,
        "the clock did not advance: still at {t} after 2000 frames"
    );
    let after_light = scene.landscape_lighting();
    assert_ne!(
        before_light.sunlight, after_light.sunlight,
        "the sun did not move with the clock"
    );

    let (_, after) = draw_and_count(&scene, &mut gpu);
    let differing = before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > (400 * 300) / 10,
        "only {differing} pixels changed over a quarter of a day"
    );
    eprintln!(
        "t {:.3} -> {t:.3}; sunlight {:?} -> {:?}; {differing} pixels changed",
        0.25, before_light.sunlight, after_light.sunlight
    );
}

/// **The sun vector is not normalised.** Sky lighting returns a sun vector whose *length is
/// `dir_bright`*, and `calc_lighting`'s `n . sunlight` already carries the brightness. This is the
/// test that fails if someone normalises it on the way through.
///
/// Oracle: the retail region's own `dir_bright` values, and `dereth_world_render::sky::get_lighting`.
#[test]
fn the_landscape_sun_vector_keeps_its_brightness() {
    let store = store();
    let mut gpu = crate::common::test_gpu(400, 300);
    let store = std::sync::Arc::new(store);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");

    for t in [0.02f32, 0.25, 0.5, 0.75] {
        let scene = scene_at(&store, &mut gpu, t);
        let (year, day, got_t) = scene.game_time();
        assert!(
            (got_t - t).abs() < 1e-3,
            "asked for time {t}, the clock says {got_t}"
        );
        let group = dereth_world_render::sky::present_day_group(&region, year, day)
            .expect("the region has day groups");
        let want = dereth_world_render::sky::get_lighting(group, got_t);
        let have = scene.landscape_lighting();
        assert_eq!(
            have.sunlight, want.sun_vec,
            "at t = {t} the landscape's sun vector is not the lighting lookup's"
        );
        assert!(
            (have.sunlight.magnitude() - want.sun_vec.magnitude()).abs() < 1e-6,
            "the vector was rescaled on the way through"
        );
        // `min_ambient = 0.2` is the floor applied by the landscape light tick.
        assert!(
            have.ambient_level >= 0.2 - 1e-6,
            "the min_ambient floor was not applied"
        );
        assert!(
            have.ambient_level >= want.ambient_level - 1e-6,
            "the floor lowered the ambient instead of raising it"
        );
    }
}

/// The `zfar` multiplier is a behaviour, not an optimisation: the shipped cloud decks are 20 km
/// across and the default far plane is 4 km, so without it they are clipped away entirely.
///
/// Oracle: the original sky draw multiplies the active far plane by 4.0, and
/// `dereth_world_render::sky::SKY_ZFAR_MULTIPLIER` carries that behavior here.
#[test]
fn the_sky_is_drawn_with_four_times_the_far_plane() {
    assert_eq!(dereth_world_render::sky::SKY_ZFAR_MULTIPLIER, 4.0);
    let world = dereth_render::ViewParams::default();
    let sky = dereth_scene::sky::SkyScene::view_params(&world);
    assert!((sky.zfar - world.zfar * 4.0).abs() < 1e-3);
    // And nothing else about the projection changed: znear in particular stays 0.1.
    assert_eq!(sky.znear, world.znear);
    assert_eq!(sky.fov_y_rad, world.fov_y_rad);
    assert_eq!(sky.aspect, world.aspect);
}

/// The sky's UV scroll is a **`dt`-scaled accumulator**, so the same elapsed time must produce the
/// same picture regardless of how many frames it took — and holding `dt` at zero must freeze it.
///
/// Oracle: texture-velocity update, once per frame with the
/// frame's elapsed time, over every id in `texture_velocity_gids`:
///
/// ```text
/// d = dt * offset;  if (total >= 1.0) d -= 1.0;  total += d
/// texture-velocity update(gfxobj, total)   // -> per-frame UV delta
/// ```
///
/// Accumulating one velocity per **frame** with no `dt` (the sky streams past at frame rate)
/// fails the half-`dt` leg. A **constant** offset (the sky stopped dead) fails the moving leg.
/// Only the `dt`-scaled accumulator passes both, which is why the claim is on the rate and not
/// merely on "it moves".
#[test]
fn the_sky_scrolls_by_elapsed_time_and_not_by_frame_count() {
    let store = store();
    let mut gpu = crate::common::test_gpu(400, 300);
    let store = std::sync::Arc::new(store);
    let cfg = || SceneConfig {
        land_radius: 0,
        scenery_radius: 0,
        character: false,
        particles: false,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    // The clock is held still so the day cycle contributes nothing and every difference is the
    // scroll; only `dt` varies.
    let run = |gpu: &mut Gpu, frames: usize, dt: f32| {
        let mut scene = WorldScene::load(&store, gpu, cfg()).expect("the landscape loads");
        for _ in 0..frames {
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                dereth_client_runtime::character::CharacterInput::default(),
                dereth_primitives::LocalTime(0.0),
                dt,
            );
        }
        draw_and_count(&scene, gpu).1
    };

    let still = run(&mut gpu, 60, 0.0);
    let moved = run(&mut gpu, 60, 1.0 / 60.0);
    assert_ne!(
        moved, still,
        "60 frames of real elapsed time did not move the sky at all"
    );

    // One second of elapsed time, reached two ways. The totals are sums of `dt * offset`, so the
    // coarse and fine runs land on the same place and the picture matches.
    let coarse = run(&mut gpu, 60, 1.0 / 60.0);
    let fine = run(&mut gpu, 120, 1.0 / 120.0);
    // Not bit-equality: 60 additions of `dt` and 120 of `dt/2` are the same sum in exact arithmetic
    // and differ in the last f32 bits, which can move a texel. The threshold-free way to say "the
    // scroll follows seconds, not frames" is a **ratio**: doubling the frame rate must matter far
    // less than the motion itself. A frame-counting implementation makes the two comparable — it
    // would scroll twice as far in the fine run — so this fails loudly for the bug it guards.
    let d_rate = coarse.iter().zip(&fine).filter(|(a, b)| a != b).count();
    let d_still = coarse.iter().zip(&still).filter(|(a, b)| a != b).count();
    assert!(
        d_rate * 20 < d_still,
        "halving the frame time changed {d_rate} bytes against {d_still} for the motion itself: \
         the scroll is counting frames, not seconds"
    );
}
