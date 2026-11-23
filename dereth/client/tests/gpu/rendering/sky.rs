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
/// Landscape-lighting updates re-run `calc_lighting` on every loaded block, in place.
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
        // `App::frame` streams here, outside the frame bracket, because `Gpu::upload_texture`
        // runs a command list of its own.
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

/// The scene the light tick is watched in: the landscape two blocks round the viewer, at two
/// levels of detail, with no scenery and no body.
fn land_at(store: &std::sync::Arc<RetailDatStore>, gpu: &mut Gpu, t: f32) -> WorldScene {
    let cfg = SceneConfig {
        land_radius: 2,
        scenery_radius: 0,
        character: false,
        time_of_day: Some(t),
        ..SceneConfig::default()
    };
    WorldScene::load(store, gpu, cfg).expect("the landscape loads")
}

/// Step `scene`'s clock to `now` and stream, as a frame does, and say whether the step left any
/// block queued to be built between its update and its stream.
fn tick_to(
    scene: &mut WorldScene,
    store: &std::sync::Arc<RetailDatStore>,
    gpu: &mut Gpu,
    now: f64,
    dt: f64,
) -> usize {
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        dereth_primitives::LocalTime(now),
        #[allow(clippy::cast_possible_truncation)]
        {
            dt as f32
        },
    );
    let queued = scene.pending_slot_count();
    scene.stream(store, gpu).expect("stream");
    queued
}

/// Every resident block's baked vertex colours, by its block.
fn block_colours(scene: &WorldScene) -> std::collections::BTreeMap<(i32, i32), Vec<[u8; 3]>> {
    let window = &scene.streamer.window;
    let w = window.mid_width();
    (0..w)
        .flat_map(|xi| (0..w).map(move |yi| (xi, yi)))
        .filter_map(|(xi, yi)| {
            let slot = window.slot(xi, yi)?;
            let mesh = slot
                .mesh
                .as_ref()?
                .get::<dereth_terrain::land::mesh::LandblockMesh>()?;
            Some(((slot.block_x, slot.block_y), mesh.colours.clone()))
        })
        .collect()
}

/// What `calc_lighting` gives each resident block's mesh under the landscape's light as it
/// stands, by its block.
fn lit_colours(scene: &WorldScene) -> std::collections::BTreeMap<(i32, i32), Vec<[u8; 3]>> {
    let lighting = scene.landscape_lighting();
    let window = &scene.streamer.window;
    let w = window.mid_width();
    (0..w)
        .flat_map(|xi| (0..w).map(move |yi| (xi, yi)))
        .filter_map(|(xi, yi)| {
            let slot = window.slot(xi, yi)?;
            let mesh = slot
                .mesh
                .as_ref()?
                .get::<dereth_terrain::land::mesh::LandblockMesh>()?;
            Some((
                (slot.block_x, slot.block_y),
                dereth_terrain::land::lighting::calc_lighting(mesh, &lighting),
            ))
        })
        .collect()
}

/// One frame's clock step, the headless frame's.
const FRAME: f64 = 1.0 / 30.0;

/// Behaviour: rendering.sky.the-light-tick-builds-no-block-again
/// **The light tick re-lights the land without building it again.** Between two light ticks the
/// landscape queues and builds nothing, and its colours hold; across the tick it still queues and
/// builds nothing, while every resident block is re-lit where it stands.
///
/// The clock is the scene's injected one, stepped a headless frame at a time from the first
/// light tick to the next.
///
/// Oracle: the retail region's light tick of 15 s, and a GPU device.
#[test]
fn the_light_tick_relights_the_land_without_building_a_block_again() {
    let store = std::sync::Arc::new(store());
    let mut gpu = crate::common::test_gpu(400, 300);
    let mut scene = land_at(&store, &mut gpu, 0.25);

    // Settle: the first update takes the first light tick; every block is built.
    let mut now = FRAME;
    tick_to(&mut scene, &store, &mut gpu, now, FRAME);
    while scene.pending_slot_count() > 0 {
        scene.stream(&store, &mut gpu).expect("stream");
    }
    let resident = block_colours(&scene).len();
    assert_eq!(resident, 25, "the window is not five blocks square");
    let deadline = scene.next_light_tick;
    let built = scene.draw.stats.blocks_built;
    let relit = scene.draw.stats.blocks_relit;
    let colours = block_colours(&scene);
    let light = scene.landscape_lighting();

    // The quiet period: every frame short of the next light tick.
    let mut frames = 0;
    while now + FRAME <= deadline {
        now += FRAME;
        frames += 1;
        let queued = tick_to(&mut scene, &store, &mut gpu, now, FRAME);
        assert_eq!(
            queued, 0,
            "{queued} blocks queued {now:.3} s into the quiet period"
        );
    }
    assert!(frames > 400, "only {frames} frames before the light tick");
    assert_eq!(
        scene.landscape_lighting(),
        light,
        "the light changed between ticks"
    );
    assert_eq!(
        scene.draw.stats.blocks_built, built,
        "a block was built between ticks"
    );
    assert_eq!(
        scene.draw.stats.blocks_relit, relit,
        "a block was re-lit between ticks"
    );
    assert!(
        block_colours(&scene) == colours,
        "a block's colours changed between ticks"
    );

    // Across the tick: the frames up to and including the one that takes it.
    while scene.draw.stats.blocks_relit == relit {
        assert!(
            now < deadline + 1.0,
            "no light tick by {now:.3} s (due after {deadline:.3})"
        );
        now += FRAME;
        let queued = tick_to(&mut scene, &store, &mut gpu, now, FRAME);
        assert_eq!(
            queued, 0,
            "the light tick at {now:.3} s queued {queued} blocks"
        );
    }
    assert_ne!(
        scene.landscape_lighting(),
        light,
        "the light tick changed no light"
    );
    assert_eq!(
        scene.draw.stats.blocks_relit - relit,
        resident as u64,
        "the light tick did not re-light every resident block once"
    );
    assert_eq!(
        scene.draw.stats.blocks_built,
        built,
        "the light tick built {} blocks again",
        scene.draw.stats.blocks_built - built
    );
    eprintln!(
        "{frames} quiet frames; the tick at {now:.3} s re-lit {resident} blocks and built none"
    );
}

/// Behaviour: rendering.sky.the-light-tick-relights-each-block-as-a-rebuild-would
/// **The light tick's colours are the ones a rebuilt block bakes.** After a light tick every
/// resident block's vertex colours are `calc_lighting` of its own mesh under the new light; they
/// moved from the old hour's; and building every block again under the same light changes no
/// colour and no pixel of the frame.
///
/// One update a long way on in game time is one light tick: the tick fires once whatever the
/// step, and the long step moves the sun far enough that the colours plainly change.
///
/// Oracle: the landscape lighting function on the block's own mesh, and a GPU device.
#[test]
fn a_light_tick_relights_each_block_as_building_it_again_would() {
    let store = std::sync::Arc::new(store());
    let mut gpu = crate::common::test_gpu(400, 300);
    let mut scene = land_at(&store, &mut gpu, 0.25);

    let mut now = FRAME;
    tick_to(&mut scene, &store, &mut gpu, now, FRAME);
    while scene.pending_slot_count() > 0 {
        scene.stream(&store, &mut gpu).expect("stream");
    }
    let before = block_colours(&scene);
    let light = scene.landscape_lighting();
    let built = scene.draw.stats.blocks_built;

    // Ten minutes of clock in one step: one light tick, an hour and a half of game time.
    now += 600.0;
    let queued = tick_to(&mut scene, &store, &mut gpu, now, FRAME);
    assert_eq!(queued, 0, "the light tick queued {queued} blocks");
    assert_eq!(
        scene.draw.stats.blocks_built, built,
        "the light tick built blocks"
    );
    assert_ne!(
        scene.landscape_lighting().sunlight,
        light.sunlight,
        "the sun did not move"
    );

    let after = block_colours(&scene);
    assert_eq!(after.len(), before.len());
    let moved: usize = after
        .iter()
        .map(|(b, c)| c.iter().zip(&before[b]).filter(|(x, y)| x != y).count())
        .sum();
    let total: usize = after.values().map(Vec::len).sum();
    assert!(
        moved * 2 > total,
        "only {moved} of {total} vertex colours moved with the sun"
    );
    for (block, lit) in lit_colours(&scene) {
        assert!(
            after[&block] == lit,
            "block {block:?}'s colours are not its mesh's lighting under the new light"
        );
    }
    let (_, frame) = draw_and_count(&scene, &mut gpu);

    // Every block built again under the same light: the same colours, the same frame.
    scene.draw.rebuild_resident_blocks(&mut scene.world);
    assert!(
        scene.pending_slot_count() > 0,
        "no block was queued to be built again"
    );
    while scene.pending_slot_count() > 0 {
        scene.stream(&store, &mut gpu).expect("stream");
    }
    assert_eq!(
        scene.draw.stats.blocks_built - built,
        after.len() as u64,
        "not every block was built again"
    );
    assert!(
        block_colours(&scene) == after,
        "a block built again under the same light baked other colours"
    );
    let (_, rebuilt) = draw_and_count(&scene, &mut gpu);
    let differing = frame
        .as_chunks::<4>()
        .0
        .iter()
        .zip(rebuilt.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "{differing} pixels differ from the frame of the blocks built again"
    );
    eprintln!("{moved} of {total} vertex colours moved; the rebuilt frame is the same");
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

// ---------------------------------------------------------------------------------------------
// A world's sky tints.
// ---------------------------------------------------------------------------------------------

/// The world key the tinted test worlds are written for.
const TINTED_WORLD: &str = "a tinted sky";

/// Sky tints for every day group of the retail region: every object `tint` and the veil `veil`
/// at every time of day.
fn tints_for(store: &RetailDatStore, tint: u32, veil: f32) -> dereth_assets::sky_tints::SkyTints {
    use dereth_assets::sky_tints::{SkyTints, TintedDay, TintedTime};
    let id = dereth_primitives::DataId(0x1300_0000);
    let bytes = store
        .read_typed(dereth_dat::DbType::Region, id)
        .expect("the region record");
    let region = <dereth_assets::Region as dereth_assets::Decode>::decode_payload(id, &bytes)
        .expect("the region decodes");
    let sky = region.sky_info.expect("the region has a sky");
    SkyTints {
        days: sky
            .day_groups
            .iter()
            .map(|g| TintedDay {
                objects: g.sky_objects.len(),
                times: g
                    .sky_time
                    .iter()
                    .map(|_| TintedTime {
                        veil,
                        tints: vec![tint; g.sky_objects.len()],
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The retail world with an overlay holding `tints` as its sky tints, in `scratch`.
fn tinted_world(
    scratch: &dereth_dat::testing::ScratchDir,
    name: &str,
    tints: &dereth_assets::sky_tints::SkyTints,
) -> RetailDatStore {
    use dereth_dat::overlay::{OverlayDir, OverlayWriter};
    let dir = OverlayDir::new(&scratch.path().join(name)).expect("an overlay folder");
    let plain = store();
    let portal = plain.portal().base();
    let revision = dereth_dat::decompose::revision_over(&portal);
    let mut w = OverlayWriter::open_or_create(
        &dir.container(dereth_dat::ModernDat::Portal),
        &portal,
        "client_portal.dat",
        TINTED_WORLD,
        1,
    )
    .expect("the portal overlay");
    w.save(
        &portal,
        dereth_assets::sky_tints::SKY_TINTS,
        &tints.encode(),
        1,
        revision,
        1,
    )
    .expect("the tints saved");
    w.add_iteration(revision, 1).expect("the revision");
    w.flush(1).expect("flushed");
    drop(w);
    store()
        .with_overlay(&dir, Some(TINTED_WORLD))
        .expect("the overlay over the world")
}

/// The top eighth of a 400 x 300 frame, which from `scene_at`'s camera is sky alone.
fn sky_band(frame: &[u8]) -> &[[u8; 4]] {
    frame[..(400 * 300 / 8) * 4].as_chunks::<4>().0
}

/// Each channel's mean over `pixels`.
fn means(pixels: &[[u8; 4]]) -> [f64; 3] {
    let mut sum = [0.0f64; 3];
    for p in pixels {
        for k in 0..3 {
            sum[k] += f64::from(p[k]);
        }
    }
    #[allow(clippy::cast_precision_loss)] // a frame's pixels
    let n = pixels.len().max(1) as f64;
    sum.map(|s| s / n)
}

/// Behaviour: rendering.sky.a-worlds-sky-tints-color-its-sky-objects-by-the-hour
/// **A world's tints color its sky.** The retail world at noon is drawn as it is, then with an
/// overlay whose tints make every sky object red, then with one whose tints leave the objects as
/// they are and lay the whole of the fog's color over them. The red sky keeps its red and loses
/// nearly all its green and blue; the veiled one is the fog's color, one color across the band,
/// and the land under it is the land of the untinted frame.
///
/// Oracle: the retail region and sky objects, and tints written for them; a GPU device.
#[test]
fn a_worlds_sky_tints_color_its_sky_and_veil_it() {
    let scratch = dereth_dat::testing::ScratchDir::new("sky-tints").expect("scratch");
    let mut gpu = crate::common::test_gpu(400, 300);
    let plain = std::sync::Arc::new(store());
    let (_, untinted) = draw_and_count(&scene_at(&plain, &mut gpu, 0.5), &mut gpu);

    let red = std::sync::Arc::new(tinted_world(
        &scratch,
        "red",
        &tints_for(&plain, 0xFFFF_0000, 0.0),
    ));
    let scene = scene_at(&red, &mut gpu, 0.5);
    let sky = scene.sky().expect("a sky");
    assert!(sky.has_tints());
    assert_eq!(sky.tinted(), sky.live(), "every object present is tinted");
    assert_eq!(sky.veil(), Some(0.0));
    let (_, reddened) = draw_and_count(&scene, &mut gpu);
    let (was, now) = (means(sky_band(&untinted)), means(sky_band(&reddened)));
    eprintln!("sky band, untinted {was:?}, red {now:?}");
    assert!(
        was[1] + was[2] > 60.0,
        "the untinted sky has green and blue: {was:?}"
    );
    assert!(
        now[1] + now[2] < (was[1] + was[2]) * 0.1,
        "the red tint left green and blue: {was:?} against {now:?}"
    );
    assert!(
        now[0] > was[0] * 0.5,
        "the red tint lost the red: {was:?} against {now:?}"
    );

    let veiled_world = std::sync::Arc::new(tinted_world(
        &scratch,
        "veiled",
        &tints_for(&plain, 0xFFFF_FFFF, 1.0),
    ));
    let scene = scene_at(&veiled_world, &mut gpu, 0.5);
    assert_eq!(scene.sky().and_then(|s| s.veil()), Some(1.0));
    let fog = scene.world_fog().color;
    let (_, veiled) = draw_and_count(&scene, &mut gpu);
    let want = [(fog >> 16) & 0xFF, (fog >> 8) & 0xFF, fog & 0xFF].map(f64::from);
    for p in sky_band(&veiled) {
        for k in 0..3 {
            assert!(
                (f64::from(p[k]) - want[k]).abs() <= 3.0,
                "a veiled sky pixel {p:?} is not the fog's color {want:?}"
            );
        }
    }
    // The land: every drawn pixel the red tint left as it was, which is no sky object's.
    let (mut land, mut changed) = (0usize, 0usize);
    for ((u, r), v) in untinted
        .as_chunks::<4>()
        .0
        .iter()
        .zip(reddened.as_chunks::<4>().0)
        .zip(veiled.as_chunks::<4>().0)
    {
        if u == r && u[..3].iter().map(|&c| u32::from(c)).sum::<u32>() > 24 {
            land += 1;
            changed += usize::from(u != v);
        }
    }
    assert!(land > 400 * 300 / 8, "only {land} pixels of land");
    assert!(
        changed * 50 < land,
        "the veil reached the land: {changed} of {land} of its pixels changed"
    );
}

/// Behaviour: rendering.sky.a-world-without-sky-tints-draws-its-sky-as-the-region-says
/// **A world without tints draws its sky as the region says.** The retail world has none: its sky
/// has no tints, tints no object and lays no veil. And tints that change nothing (every object
/// white, no veil) draw the very same frame as none at all, at noon and at midnight: the tints'
/// path leaves an object's color as it is unless the world's tint changes it.
///
/// Oracle: the retail region and sky objects; a GPU device.
#[test]
fn a_world_without_sky_tints_draws_its_sky_as_the_region_says() {
    let scratch = dereth_dat::testing::ScratchDir::new("sky-no-tints").expect("scratch");
    let mut gpu = crate::common::test_gpu(400, 300);
    let plain = std::sync::Arc::new(store());
    let white = std::sync::Arc::new(tinted_world(
        &scratch,
        "white",
        &tints_for(&plain, 0xFFFF_FFFF, 0.0),
    ));
    for t in [0.5, 0.02] {
        let scene = scene_at(&plain, &mut gpu, t);
        let sky = scene.sky().expect("a sky");
        assert!(!sky.has_tints());
        assert_eq!(sky.tinted(), 0);
        assert_eq!(sky.veil(), None);
        let (_, untinted) = draw_and_count(&scene, &mut gpu);
        let scene = scene_at(&white, &mut gpu, t);
        assert_eq!(
            scene.sky().map(|s| s.tinted()),
            scene.sky().map(|s| s.live())
        );
        let (_, whitened) = draw_and_count(&scene, &mut gpu);
        let differing = untinted
            .iter()
            .zip(&whitened)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "at {t}, white tints and no veil changed {differing} bytes of the frame"
        );
    }
}
