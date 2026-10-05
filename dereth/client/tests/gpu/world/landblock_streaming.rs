//! The landblock window follows the viewer. Crossing a block boundary re-centres the window and
//! the render-space origin; the blocks that come into view are meshed at their LOD ring and
//! stitched towards the viewer; the blocks that leave are released, and a shift of `mid_width` or
//! more reloads the whole window; scenery, buildings and statics are baked per block, and a block
//! that scrolls back into full detail re-bakes its scenery; the merged-surface cache is hit rather
//! than compositing a surface per cell, so a long walk neither grows without bound nor re-uploads
//! everything. The window arithmetic itself is `dereth_landscape::window`'s, tested there. Fixture:
//! the retail dats on a WARP device at 800x600, a flycam or a running body out of Holtburg. Fails
//! when the retail dats are absent; a missing device fails too.

#![cfg(gpu)]

use std::collections::BTreeSet;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::Gpu;
use dereth_world_render::consts::BLOCK_LENGTH;
use {
    dereth_client_runtime::landblock::block_xy,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

/// The retail store, or **fail**. The return type offers no skip, so no caller can turn a missing
/// oracle into a green line.
fn store() -> RetailDatStore {
    crate::common::dat_store()
}

/// The free-camera scene, which is what `--no-character` produces.
fn flycam() -> SceneConfig {
    SceneConfig {
        character: false,
        ..SceneConfig::default()
    }
}

/// Load the scene and stand the free camera in the **middle** of the block it was built around.
///
/// The load parks the flycam at `y = -0.35 * 192`, south of the block and looking north at it, a
/// framing shot. That is genuinely a different landblock, so the first viewpoint update correctly
/// re-centres one block south. Every test below is about crossing boundaries deliberately, so
/// they start from the middle.
fn settled(store: &RetailDatStore, gpu: &mut Gpu) -> WorldScene {
    let mut scene = WorldScene::load(store, gpu, flycam()).expect("the scene loads");
    scene.camera.position.x = BLOCK_LENGTH * 0.5;
    scene.camera.position.y = BLOCK_LENGTH * 0.5;
    step(&mut scene, store, gpu, 0.0, 0.0);
    assert_eq!(
        scene.viewer_block(),
        Some(block_xy(DEFAULT_LANDBLOCK)),
        "the camera is in the block the scene was built around"
    );
    scene
}

/// One frame of walking: move the camera by `(dx, dy)` metres in the current render space, run the
/// simulation step (which re-centres the window) and then the fetch.
fn step(scene: &mut WorldScene, store: &RetailDatStore, gpu: &mut Gpu, dx: f32, dy: f32) {
    scene.camera.position.x += dx;
    scene.camera.position.y += dy;
    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        LocalTime(0.0),
        0.0,
    );
    scene.stream(store, gpu).expect("the streamed blocks build");
}

/// Oracle: retail's viewpoint update followed by landscape-frame calculation. Crossing one
/// block boundary re-centres the window on the new block and moves the render-space origin, so
/// the viewer's own coordinates land back inside `0..192`.
#[test]
fn crossing_a_block_boundary_recentres_the_window_and_the_render_space() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);
    let start = block_xy(DEFAULT_LANDBLOCK);
    let resident = scene.resident_blocks();
    assert_eq!(resident, 49, "mid_radius 3 is a 7x7 window");

    // The camera starts at x = 96 (the middle of the block). One block east.
    step(&mut scene, &store, &mut gpu, BLOCK_LENGTH, 0.0);
    assert_eq!(scene.viewer_block(), Some((start.0 + 1, start.1)));
    assert!(
        (0.0..BLOCK_LENGTH).contains(&scene.camera.position.x),
        "the origin did not follow: camera x is {}",
        scene.camera.position.x
    );
    assert_eq!(
        scene.resident_blocks(),
        resident,
        "the window neither grew nor shrank"
    );

    // North as well, and back again, so the negative shift is exercised too.
    step(&mut scene, &store, &mut gpu, 0.0, BLOCK_LENGTH);
    assert_eq!(scene.viewer_block(), Some((start.0 + 1, start.1 + 1)));
    step(&mut scene, &store, &mut gpu, -BLOCK_LENGTH, -BLOCK_LENGTH);
    assert_eq!(scene.viewer_block(), Some(start));
    assert_eq!(scene.resident_blocks(), resident);
}

/// Oracle: the retail dats and the landscape's block-orientation ring table
/// (`window::ring_assignment_matches_the_table_for_all_five_presets` fixes the table itself). After
/// a scroll, **every** slot must hold the block its window position names, meshed at that
/// position's ring and stitched towards the viewer.
///
/// This is the "no seam or pop at the boundary beyond what the rings themselves produce" clause: a
/// block that kept the LOD of the ring it used to be in is exactly a seam.
#[test]
fn after_a_scroll_every_slot_is_the_right_block_at_the_right_ring() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);

    for _ in 0..3 {
        step(&mut scene, &store, &mut gpu, BLOCK_LENGTH, BLOCK_LENGTH);
    }
    let viewer = scene.viewer_block().expect("a viewer block");
    assert_eq!(
        viewer,
        (
            block_xy(DEFAULT_LANDBLOCK).0 + 3,
            block_xy(DEFAULT_LANDBLOCK).1 + 3
        )
    );

    let rings = scene.window_rings();
    assert_eq!(rings.len(), 49);
    let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
    for r in rings {
        let want = (viewer.0 + r.xi - 3, viewer.1 + r.yi - 3);
        assert_eq!(
            r.block, want,
            "slot ({},{}) holds {:?}, wants {want:?}",
            r.xi, r.yi, r.block
        );
        assert!(seen.insert(r.block), "{:?} is in two slots", r.block);
        assert_eq!(
            r.side_cell_count, r.expected_side_cell_count,
            "slot ({},{}) is meshed at {} cells a side, its ring wants {}",
            r.xi, r.yi, r.side_cell_count, r.expected_side_cell_count
        );
    }
}

/// Oracle: the retail dats. Walk out of the starting block and the world is still drawn ahead of
/// you, with the objects that belong to the blocks you walked into.
///
/// The counters are the evidence rather than a screenshot: `scenery_objects`, `buildings` and
/// `static_objects` are a sum over the resident blocks, so a window that scrolled without re-baking
/// reports the objects of blocks that are no longer under it.
#[test]
fn walking_out_of_the_starting_block_keeps_the_world_and_its_objects_drawn() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);
    let at_home = scene.draw.stats;
    assert!(at_home.buildings > 0, "Holtburg has buildings");
    // `object_triangles` is what the frame draws and `object_triangles_resident` what the bake
    // holds. The bake is what this test is about.
    assert!(
        at_home.object_triangles_resident > 10_000,
        "only {} triangles baked",
        at_home.object_triangles_resident
    );
    assert!(
        at_home.object_triangles > 0,
        "the frame drew no object triangles"
    );

    // Four blocks east: far enough that not one of the original 3x3 object blocks is still in the
    // scenery radius, so every batch on screen was baked after the walk began.
    for _ in 0..4 {
        step(&mut scene, &store, &mut gpu, BLOCK_LENGTH, 0.0);
    }
    let away = scene.draw.stats;
    assert_eq!(away.blocks_meshed, 49, "the window is still full");
    assert_eq!(
        away.object_batches_untextured, 0,
        "every object surface still resolved"
    );
    assert!(
        away.object_triangles > 0,
        "the blocks we walked into grew no objects at all"
    );
    assert_ne!(
        away.object_triangles, at_home.object_triangles,
        "the object batches did not change, so they were not re-baked per block"
    );

    // And the frame still has a world in it, after four blocks of walking.
    scene.camera.position.z += 60.0;
    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let rgba = image.to_rgba();
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) > 24)
        .count();
    let total = (image.width * image.height) as usize;
    assert!(
        lit * 3 > total,
        "only {lit} of {total} pixels are lit four blocks from where the scene was built"
    );
}

/// Oracle: retail's block-window update's full-reload arm: if the shift is >= `mid_width` in
/// either axis, everything is released and the whole window is re-fetched. A teleport is what a
/// portal or a recall does, and it must not leave the previous window's blocks resident.
#[test]
fn a_teleport_beyond_the_window_reloads_it_without_leaking_the_old_blocks() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);
    let start = block_xy(DEFAULT_LANDBLOCK);

    // Ten blocks in one step: mid_width is 7, so this is the full-reload path.
    step(&mut scene, &store, &mut gpu, 10.0 * BLOCK_LENGTH, 0.0);
    assert_eq!(scene.viewer_block(), Some((start.0 + 10, start.1)));
    assert_eq!(
        scene.resident_blocks(),
        49,
        "the whole window was re-fetched"
    );
    for r in scene.window_rings() {
        assert!(
            (r.block.0 - (start.0 + 10)).abs() <= 3,
            "{:?} is a block from the window we left",
            r.block
        );
    }
}

/// Oracle: retail's merged-surface and decoded-object caches. Walking back over ground already
/// seen re-merges from the cache, not from scratch.
///
/// The count is asserted, not merely recorded: a scroll that re-uploads every surface passes
/// every other test in this module.
///
/// Retail's block destruction removes surfaces once per cell, and a reference count reaching zero
/// **destroys the merged texture**, so a block that leaves and comes back does re-merge; walking
/// back over seen ground does build something. What is asserted is that the cache is **hit**
/// (`terrain_surfaces_built` far below `terrain_surface_requests`) rather than compositing a
/// texture per cell per step, and that residency is a function of where the window is: a round
/// trip that ends where it started ends holding what it started with.
#[test]
fn walking_back_over_seen_ground_re_merges_from_the_cache_and_not_from_scratch() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);
    let first = scene.draw.stats.terrain_surfaces;
    let built_at_load = scene.draw.stats.terrain_surfaces_built;
    assert!(first > 50, "only {first} surfaces at load");

    let round_trip = |scene: &mut WorldScene, gpu: &mut Gpu| {
        for _ in 0..2 {
            step(scene, &store, gpu, BLOCK_LENGTH, 0.0);
        }
        for _ in 0..2 {
            step(scene, &store, gpu, -BLOCK_LENGTH, 0.0);
        }
        (
            scene.draw.stats.terrain_surfaces,
            scene.draw.stats.terrain_surfaces_built,
        )
    };
    let (resident_one, built_one) = round_trip(&mut scene, &mut gpu);
    let (resident_two, built_two) = round_trip(&mut scene, &mut gpu);
    eprintln!(
        "streaming round trips: {first} surfaces at load, {resident_one} then {resident_two} \
         resident after each; {built_one} then {built_two} composited cumulatively over \
         {} cell requests",
        scene.draw.stats.terrain_surface_requests
    );

    // The premise: each round trip actually composited something, or every equality below is about
    // a window that never moved. Both round trips do composite, and that is not a defect: a block
    // that left the window had its surfaces destroyed by the release edge, exactly as retail's
    // block teardown destroys them.
    assert!(
        built_one > built_at_load,
        "the first round trip composited nothing over the {built_at_load} the load did"
    );
    assert!(
        built_two > built_one,
        "the second round trip composited nothing at all"
    );

    // Residency is a function of the window's position, and both round trips end at the same
    // place, so both must end holding the same thing -- and the same thing the load held.
    assert_eq!(scene.viewer_block(), Some(block_xy(DEFAULT_LANDBLOCK)));
    assert_eq!(
        resident_two, resident_one,
        "two identical round trips left different residencies ({resident_two} against \
         {resident_one})"
    );
    assert_eq!(
        resident_one, first,
        "a round trip that ended where it began holds {resident_one} merged surfaces against the \
         {first} it started with"
    );

    // **The cache is hit.** The two round trips cover exactly the same ground at exactly the same
    // detail levels, so the second must ask for the same cells and composite far fewer than one
    // surface per cell. Bounded by the population rather than pinned to today's figure: a cache
    // that missed every time would have `built == requests`.
    let requests = scene.draw.stats.terrain_surface_requests;
    assert!(
        u64::from(scene.draw.stats.terrain_surfaces_built) * 4 < requests,
        "{} surfaces composited over {requests} cell requests: fewer than four cells share a \
         merged surface, so the terrain-surface cache is saving next to nothing",
        scene.draw.stats.terrain_surfaces_built
    );
}

/// Behaviour: world.streaming.the-landblock-window-follows-the-viewer
///
/// Oracle: the retail dats, driven through the physics and the motion table for long enough to
/// leave the block.
///
/// **Walk** out of the starting block and the world stays drawn ahead of you. The flycam tests
/// above teleport the viewpoint; this one runs a body over real terrain at the speed the motion
/// table gives it.
#[test]
fn a_walking_body_carries_the_window_with_it() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let store = std::sync::Arc::new(store);
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // `attach_character` stands the body in the middle of Holtburg, a village with solid outer
    // walls: `DatLandSource::building` registers every building shell in its sorted cell, so a
    // straight run north from the spawn meets a building and stops there. That is the walls
    // working, not the streaming failing, so the walk starts from the block's clear south-west
    // quarter instead, placed with `Character::teleport`'s position install.
    {
        let block = dereth_primitives::LandblockId(DEFAULT_LANDBLOCK);
        let c = scene.character.as_mut().expect("a body");
        let ground = c
            .land()
            .ground_height(block, 24.0, 24.0)
            .expect("Holtburg's terrain");
        c.teleport(dereth_primitives::Position::new(
            block.cell(1),
            dereth_primitives::Frame::new(
                dereth_primitives::Vec3::new(24.0, 24.0, ground),
                dereth_primitives::Quat::IDENTITY,
            ),
        ));
    }

    let start = scene.viewer_block().expect("a viewer block");
    let spawn = scene.character.as_ref().expect("a body").position();
    assert_eq!(spawn.cell.landblock().x(), 0xA9);

    // `MIN_QUANTUM` per frame, exactly as `--headless` steps the clock, and the run key held.
    let input = dereth_client_runtime::character::CharacterInput {
        forward: true,
        run: true,
        ..dereth_client_runtime::character::CharacterInput::default()
    };
    let step_s = dereth_physics::globals::MIN_QUANTUM;
    let mut now = 0.0f64;
    let mut crossings = 0usize;
    let mut block = start;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step in seconds, narrowed for the debug camera's own f32 delta.
    let dt = step_s as f32;
    let sweeps_before = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps;
    for _ in 0..3_000 {
        now += step_s;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            input,
            LocalTime(now),
            dt,
        );
        // **`App::frame`'s next line, and this loop needs it.** Retail's normal rendering updates
        // the swept camera's viewer state before using its cell id for the landscape viewpoint
        // update, so the swept camera is what carries the window. Without this call
        // `Character::camera.viewer` stays where the teleport's viewer initialization left it, the
        // viewpoint never advances, and the window never re-centres however far the body runs.
        // The other tests here step by teleporting, which re-attaches the camera.
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step_s,
        );
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
        let b = scene.viewer_block().expect("a viewer block");
        if b != block {
            block = b;
            crossings += 1;
            if crossings == 2 {
                break;
            }
        }
    }
    // The premise for the assertion below: the camera really was driven. A frozen viewer position
    // is a window that cannot move, and "the window did not follow the body" would then be
    // a statement about the harness rather than about the streaming.
    assert!(
        scene
            .character
            .as_ref()
            .expect("a body")
            .camera
            .stats
            .sweeps
            > sweeps_before,
        "no camera sweep ran, so the viewpoint could not have moved whatever the body did"
    );
    assert!(
        crossings >= 1,
        "the body never left its landblock in 100 s of running"
    );
    assert_ne!(block, start, "the window did not follow the body");

    // The body is still inside the block the render space is anchored to, and the window is full.
    let f = scene.character.as_ref().expect("a body").render_frame();
    assert!(
        (0.0..BLOCK_LENGTH).contains(&f.origin.x) && (0.0..BLOCK_LENGTH).contains(&f.origin.y),
        "the body is at {:?} in a space anchored on {block:?}",
        f.origin
    );
    assert_eq!(scene.resident_blocks(), 49);
    assert_eq!(scene.draw.stats.object_batches_untextured, 0);
    for r in scene.window_rings() {
        assert_eq!(r.side_cell_count, r.expected_side_cell_count);
    }
}

/// Oracle: the retail cell dat, twenty landblocks of it. A long walk is where a streaming bug that
/// looks fine over three blocks shows up: an unreleased block, a merge cache that never hits, a
/// descriptor heap that fills.
///
/// Two things are asserted rather than merely exercised. The window stays exactly full (a leak
/// would grow it and a release bug would shrink it), and the merge cache **composites** far fewer
/// surfaces than it is asked for (`terrain_surfaces_built` against `terrain_surface_requests`),
/// which is what says the cache is being hit at all. Twenty blocks of Dereth share most of their
/// terrain pairs; a cache that missed every time would composite one surface per cell per block.
///
/// Residency is a function of where the window is, and walking twenty blocks east onto simpler
/// terrain legitimately ends holding fewer surfaces than Holtburg did, so it is asserted as a
/// **bound**: a leaking cache walking twenty new columns would hold several windows' worth.
#[test]
fn a_twenty_block_walk_releases_what_it_leaves_and_reuses_what_it_has_seen() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = settled(&store, &mut gpu);
    let start = block_xy(DEFAULT_LANDBLOCK);
    let first = scene.draw.stats.terrain_surfaces;
    let built_at_load = scene.draw.stats.terrain_surfaces_built;
    let asked_at_load = scene.draw.stats.terrain_surface_requests;

    for n in 1..=20 {
        step(&mut scene, &store, &mut gpu, BLOCK_LENGTH, 0.0);
        assert_eq!(
            scene.viewer_block(),
            Some((start.0 + n, start.1)),
            "the window fell behind after {n} blocks"
        );
        assert_eq!(
            scene.resident_blocks(),
            49,
            "the window is not full after {n} blocks"
        );
    }
    let resident = scene.draw.stats.terrain_surfaces;
    let built = scene.draw.stats.terrain_surfaces_built - built_at_load;
    let asked = scene.draw.stats.terrain_surface_requests - asked_at_load;
    eprintln!(
        "streaming 20-block walk: {first} merged surfaces at load, {resident} at the end; \
         {built} composited over {asked} cell requests; {} surfaces freed, {} double releases",
        scene.draw.stats.terrain_surfaces_freed, scene.draw.stats.terrain_unowned_releases
    );

    // The premise: the walk really did meet terrain the load window did not hold.
    assert!(
        built > 0,
        "twenty new columns of Dereth composited no new merged surface"
    );
    // **The cache is hit**, on the pair that can say so. Bounded by the population -- a cache that
    // missed every time would have `built == asked`.
    assert!(
        u64::from(built) * 8 < asked,
        "{built} surfaces composited over {asked} cell requests: fewer than eight cells share a \
         merged surface, so the terrain-surface cache is not being hit"
    );
    // **Residency is bounded.** Twenty blocks of new terrain through a 49-block window: a cache
    // that kept everything it had ever seen would hold several windows' worth. The bound is
    // derived from the window's population rather than pinned to a count, so it keeps measuring
    // if the data changes.
    assert!(
        resident <= 2 * first,
        "{resident} merged surfaces resident after twenty blocks against the {first} a window of \
         the same size held at the load: the cache is accumulating departed blocks"
    );
    assert_eq!(
        scene.draw.stats.terrain_unowned_releases, 0,
        "a merged surface was released twice"
    );
    // Every slot still holds the block its position names, at its ring's detail.
    for r in scene.window_rings() {
        assert_eq!(r.block, (start.0 + 20 + r.xi - 3, start.1 + r.yi - 3));
        assert_eq!(r.side_cell_count, r.expected_side_cell_count);
    }
}

// ---------------------------------------------------------------------------------------------
// A block that scrolls back into full detail re-bakes its scenery
// ---------------------------------------------------------------------------------------------

/// The scene the halves below need, and **the configuration is the whole reason this test exists
/// in this shape.**
///
/// `SceneConfig::default()` is `land_radius: 3, scenery_radius: 1`, and rings 0 and 1 are both
/// meshed at 8 cells a side in retail's block-orientation table. So under the default **no block
/// that bakes objects at all is ever on a coarse ring**: a block leaving ring 1 also leaves the
/// scenery radius, `queue`'s *"scrolled out of the scenery radius"* arm pushes `SlotWork::Full`,
/// and the bake is thrown away rather than kept. A stale coarse bake is therefore unreachable in
/// the default configuration, and a test against `settled()` passes even with
/// `generate_scenery`'s guard deleted.
///
/// `scenery_radius == land_radius` is the configuration in which a resident block can be both
/// **baking objects** and **on a coarse ring**, as in `world::streaming_slot_release`.
fn scenery_window(store: &RetailDatStore, gpu: &mut Gpu) -> WorldScene {
    let cfg = SceneConfig {
        character: false,
        land_radius: 2,
        scenery_radius: 2,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene.camera.position.x = BLOCK_LENGTH * 0.5;
    scene.camera.position.y = BLOCK_LENGTH * 0.5;
    step(&mut scene, store, gpu, 0.0, 0.0);
    assert_eq!(scene.viewer_block(), Some(block_xy(DEFAULT_LANDBLOCK)));
    assert_eq!(scene.resident_blocks(), 25, "land_radius 2 is a 5x5 window");
    scene
}

/// Oracle: retail's size-change notification, static-object initialization and visible-cell
/// traversal. A size change releases visible cells, physics objects, static objects and buildings
/// only on the **demotion out of full detail** (from 8 cells a side to fewer). On every
/// viewer-cell change retail traverses the whole `mid_width * mid_width` window to initialize
/// buildings, scenery, static objects and dynamic objects; static-object initialization returns
/// unless `side_cell_count == 8` and builds only when the block's static count is zero. So a
/// detail-level change discards a block's objects and full detail builds them again.
///
/// # Why this is three assertions and not one
///
/// The full-detail guard fires on the **arrival** and a stale bake would be kept by the **scroll
/// inward**, so a single station cannot see both halves; they are asserted separately below, on
/// the block itself rather than on a window total.
///
/// And they are asserted on scenery **alone**. This module's other post-walk assertion is
/// `object_triangles > 0`, which the block-info statics satisfy by themselves, so the statics
/// would mask the scenery's absence; `static_objects` recovers on every homecoming either way.
#[test]
fn a_block_that_scrolls_back_into_full_detail_re_bakes_its_scenery() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = scenery_window(&store, &mut gpu);
    let start = block_xy(DEFAULT_LANDBLOCK);
    let at_home = scene.draw.stats.scenery_objects;
    let statics_home = scene.draw.stats.static_objects;

    // ------------------------------------------------------------------------------------
    // Half 1: the ARRIVAL. A block on an outer LOD ring bakes no scenery, and that is
    // faithful: retail builds static objects only at full detail. Its statics are the control.
    // ------------------------------------------------------------------------------------
    let target = (start.0 + 2, start.1);
    let coarse = scene
        .block_bake(target)
        .expect("the target block is resident from the start");
    // The denominator first, and it is what makes the zero below a measurement rather than a
    // silence: this block IS inside the scenery radius and DID run the bake. Without it, "the
    // full-detail guard refused this block's scenery" and "this slot bakes nothing at all" read
    // identically, which is exactly how the default `scenery_radius: 1` hides the subject.
    assert!(
        coarse.baked,
        "{target:?} did not bake at all, so its zero scenery says nothing about the full-detail \
         guard"
    );
    assert_ne!(
        coarse.side_cell_count, 8,
        "{target:?} is meshed at full detail on ring 2, so this test's first half is asserting \
         nothing -- the landscape block-orientation ring table has changed"
    );
    assert_eq!(
        coarse.scenery, 0,
        "{target:?} arrived on an outer LOD ring meshed at {} cells a side and baked {} scenery \
         objects; retail's landblock static-object initialization returns immediately unless \
         side_cell_count == 8 and `generate_scenery` carries that guard as its first line, so \
         this must be zero",
        coarse.side_cell_count, coarse.scenery
    );
    assert!(
        at_home > 0,
        "the window over Holtburg baked no scenery at all"
    );

    // ------------------------------------------------------------------------------------
    // Half 2: the SCROLL INWARD. The same block, now at full detail, has scenery.
    // ------------------------------------------------------------------------------------
    for _ in 0..2 {
        step(&mut scene, &store, &mut gpu, BLOCK_LENGTH, 0.0);
    }
    assert_eq!(
        scene.viewer_block(),
        Some(target),
        "the walk did not land on the target block"
    );
    let fine = scene
        .block_bake(target)
        .expect("the target block is still resident");
    assert!(fine.baked, "the viewer's own block did not bake");
    assert_eq!(
        fine.side_cell_count, 8,
        "the viewer's own block is not meshed at full detail"
    );
    assert!(
        fine.scenery > 0,
        "{target:?} scrolled from ring 2 to the centre of the window and still holds {} scenery \
         objects. Landscape block update makes that a `Resized`, which is `SlotWork::Mesh`, and a \
         `SlotWork::Mesh` that reuses a bake taken at a different side_cell_count keeps the \
         scenery-free one for ever -- which is what \
         `BlockDraw::baked_side_cell_count` exists to prevent",
        fine.scenery
    );

    // ------------------------------------------------------------------------------------
    // Half 3: the HOME STATION of an out-and-back walk, on `scenery_objects` alone.
    // ------------------------------------------------------------------------------------
    for _ in 0..2 {
        step(&mut scene, &store, &mut gpu, -BLOCK_LENGTH, 0.0);
    }
    assert_eq!(
        scene.viewer_block(),
        Some(start),
        "the walk did not come home"
    );
    assert_eq!(
        scene.draw.stats.scenery_objects, at_home,
        "the window came home holding {} scenery objects where it held {at_home} before the walk: \
         walk away from a landblock and back and its trees are gone until you relog",
        scene.draw.stats.scenery_objects
    );
    // The second population, counted and asserted on its own. It recovers even when the scenery
    // does not, which is precisely why it could hide the scenery's absence from an aggregate.
    assert_eq!(
        scene.draw.stats.static_objects, statics_home,
        "the window came home holding {} statics where it held {statics_home}",
        scene.draw.stats.static_objects
    );
}
