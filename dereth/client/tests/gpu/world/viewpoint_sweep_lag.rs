//! The cost of the frame loop reading a viewpoint the *previous* frame's camera sweep left. Retail
//! sweeps the camera and then takes the block and cell decision from that frame's viewer cell;
//! `App::frame` runs `WorldScene::update` (with `recenter` and `update_viewer_cell`) before
//! `crate::camera::update_viewer`, so the decision on frame n reads `viewer[n - 1]`. The lag is one
//! frame of camera travel, below one 24 m cell at every station and never more than one cell
//! behind; `update` itself never moves the viewpoint, and `SceneStats::updates_without_a_sweep`
//! counts a harness that steps a body without the sweep. Stations: five runs on Holtburg's terrain
//! (`0xA9B4`), on and four metres off a 24 m line, walking and running, off-axis, and one long
//! enough to leave the block, all read in global metres. Fixture: the retail dats on a software
//! device; fails when the dats or a device are absent.

#![cfg(gpu)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_runtime::camera::CameraInput;
use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
    dereth_world_data::landblock::DEFAULT_LANDBLOCK,
};

/// `BLOCK_LENGTH`.
const BLOCK: f64 = 192.0;
/// `CELL_SIZE` — the side of one of a landblock's 8x8 cells.
const CELL: f64 = 24.0;
/// `MIN_QUANTUM`, the client's own step. The lag is one of these.
const STEP: f64 = dereth_physics::globals::MIN_QUANTUM;

/// The retail dats, or a failed test: a skipped test would read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn embodied(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> WorldScene {
    let mut scene = WorldScene::load(store, gpu, SceneConfig::default()).expect("the scene loads");
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    scene
}

/// Stand the body — and, through teleport's viewer initialization, the viewpoint — on Holtburg's
/// terrain at `(x, y)`, facing `heading` radians counter-clockwise from north.
///
/// The block's south-west quarter, for `world::landblock_streaming`'s reason: `attach_character`
/// spawns in the middle of a village whose outer walls a straight run north meets.
fn stand(scene: &mut WorldScene, x: f32, y: f32, heading: f32) {
    let block = LandblockId(DEFAULT_LANDBLOCK);
    let c = scene.character.as_mut().expect("a body");
    let ground = c
        .land()
        .ground_height(block, x, y)
        .expect("Holtburg's terrain");
    let h = heading * 0.5;
    c.teleport(Position::new(
        block.cell(1),
        Frame::new(
            Vec3::new(x, y, ground),
            Quat::new(math::cosf(h), 0.0, 0.0, math::sinf(h)),
        ),
    ));
}

// ---------------------------------------------------------------------------------------------
// The two derivations, rebuilt here rather than read out of the code under test
// ---------------------------------------------------------------------------------------------

/// The viewer origin in **global** metres: the landblock's south-west corner plus the block-relative
/// origin the `Position` carries.
///
/// Block-independent by construction, so a frame that re-centres the window cannot fake a jump.
/// `None` before viewer initialization has run: camera construction leaves the cell at `CellId(0)`,
/// which is not a position.
fn viewer_global(scene: &WorldScene) -> Option<(f64, f64, f64)> {
    let c = scene.character.as_ref().filter(|c| c.camera.attached())?;
    let p = c.camera.viewer;
    let b = p.cell.landblock();
    Some((
        f64::from(b.x()) * BLOCK + f64::from(p.frame.origin.x),
        f64::from(b.y()) * BLOCK + f64::from(p.frame.origin.y),
        f64::from(p.frame.origin.z),
    ))
}

/// The **global** cell the viewer stands in: the global-landscape-coordinate answer before the
/// local eight-by-eight mask, which makes it comparable across a block crossing. Retail uses the
/// viewer's cell outdoors and derives its outside cell indoors.
fn viewer_cell_global(scene: &WorldScene) -> Option<(i32, i32)> {
    use dereth_physics::landdefs as ld;
    let c = scene.character.as_ref().filter(|c| c.camera.attached())?;
    let p = c.camera.viewer;
    let id = if ld::is_outdoors(p.cell) {
        p.cell
    } else {
        ld::get_outside_cell_id(p.cell, p.frame.origin)
    };
    ld::gid_to_lcoord(id)
}

/// The body's own position in global metres, for the premise assertion.
fn body_global(scene: &WorldScene) -> (f64, f64) {
    let p = scene.character.as_ref().expect("a body").position();
    let b = p.cell.landblock();
    (
        f64::from(b.x()) * BLOCK + f64::from(p.frame.origin.x),
        f64::from(b.y()) * BLOCK + f64::from(p.frame.origin.y),
    )
}

// ---------------------------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------------------------

struct Station {
    name: &'static str,
    x: f32,
    y: f32,
    /// Radians counter-clockwise from north.
    heading: f32,
    run: bool,
    frames: u32,
}

#[derive(Debug, Default)]
struct Reading {
    frames: u32,
    body_travel: f64,
    viewer_travel: f64,
    /// The lag itself: the largest single-frame step of the viewpoint, in metres.
    max_step: f64,
    /// Frames on which `viewer[n - 1]` and `viewer[n]` name a different cell — i.e. frames on which
    /// the split order takes a **different** decision from the client's.
    cell_disagreements: u32,
    /// The same for the 192 m block half.
    block_disagreements: u32,
    /// The largest change in cell index across one frame, in cells. 1 means the stale answer is
    /// never more than one cell behind; 2 would mean a frame can skip a cell entirely.
    max_cell_jump: i32,
    sweeps: u64,
    /// Sweeps the geometry pulled in — `CameraStats::sweeps_blocked`. The mechanism behind the
    /// outlier: a camera released from a wall travels further in one frame than a camera that is
    /// simply following a body.
    sweeps_blocked: u64,
    /// The largest single-frame step among frames where **neither** end was a blocked sweep — the
    /// lag of a camera that is only following. Reported beside [`Reading::max_step`] so the worst
    /// case has a mechanism and not only a magnitude.
    max_step_unblocked: f64,
    /// `SceneStats::updates` and `SceneStats::updates_without_a_sweep` at the end of the run.
    updates: u64,
    updates_without_a_sweep: u64,
}

impl Reading {
    fn mean_step(&self) -> f64 {
        if self.frames == 0 {
            0.0
        } else {
            self.viewer_travel / f64::from(self.frames)
        }
    }
}

/// Walk the station in **`App::frame`'s own order** — `WorldScene::update`, then
/// `crate::camera::update_viewer` — and record the per-frame viewpoint.
fn measure(store: &Arc<RetailDatStore>, st: &Station) -> Reading {
    // A device per station: five scenes on one software device exhaust its 2048 descriptor
    // slots, and separate devices keep the stations independent, so they do not share one
    // subject or its resource history.
    let mut gpu = crate::common::test_gpu(800, 600);
    let gpu = &mut gpu;
    let mut scene = embodied(store, gpu);
    stand(&mut scene, st.x, st.y, st.heading);

    let input = CharacterInput {
        forward: true,
        run: st.run,
        ..CharacterInput::default()
    };
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step in seconds, narrowed for the debug camera's own f32 delta.
    let dt = STEP as f32;
    let mut now = 0.0f64;

    // Settle: ten frames standing, so the camera's smoother has reached the body and the first
    // measured step is walking rather than the camera catching up from the teleport.
    for _ in 0..10 {
        now += STEP;
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dt,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            STEP,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");
    }

    let start_body = body_global(&scene);
    let sweeps_before = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps;
    let mut prev_p = viewer_global(&scene).expect("the teleport placed the viewpoint");
    let mut prev_c = viewer_cell_global(&scene).expect("and gave it a usable cell id");
    let blocked_before = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps_blocked;
    let mut prev_blocked = blocked_before;
    let mut was_blocked = false;
    let mut r = Reading::default();

    for _ in 0..st.frames {
        now += STEP;
        scene.update(CameraInput::default(), input, LocalTime(now), dt);
        // `App::frame`'s next line, and this loop needs it: the camera sweep is the only thing that
        // moves `Character::camera.viewer` during ordinary frame progression.
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            STEP,
        );
        scene.stream(store, gpu).expect("the streamed blocks build");

        let p = viewer_global(&scene).expect("the viewpoint stays placed");
        let c = viewer_cell_global(&scene).expect("and keeps a usable cell id");
        let step =
            ((p.0 - prev_p.0).powi(2) + (p.1 - prev_p.1).powi(2) + (p.2 - prev_p.2).powi(2)).sqrt();
        let blocked_now = scene
            .character
            .as_ref()
            .expect("a body")
            .camera
            .stats
            .sweeps_blocked;
        let this_blocked = blocked_now != prev_blocked;
        prev_blocked = blocked_now;
        r.frames += 1;
        r.viewer_travel += step;
        r.max_step = r.max_step.max(step);
        if !this_blocked && !was_blocked {
            r.max_step_unblocked = r.max_step_unblocked.max(step);
        }
        was_blocked = this_blocked;
        if c != prev_c {
            r.cell_disagreements += 1;
            r.max_cell_jump = r
                .max_cell_jump
                .max((c.0 - prev_c.0).abs().max((c.1 - prev_c.1).abs()));
        }
        if (c.0 >> 3, c.1 >> 3) != (prev_c.0 >> 3, prev_c.1 >> 3) {
            r.block_disagreements += 1;
        }
        prev_p = p;
        prev_c = c;
    }

    let end_body = body_global(&scene);
    r.body_travel =
        ((end_body.0 - start_body.0).powi(2) + (end_body.1 - start_body.1).powi(2)).sqrt();
    r.sweeps = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps
        - sweeps_before;
    r.sweeps_blocked = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps_blocked
        - blocked_before;
    // The unswept-update counter, in the order that must hold it at zero.
    r.updates = scene.draw.stats.updates;
    r.updates_without_a_sweep = scene.draw.stats.updates_without_a_sweep;
    r
}

const STATIONS: [Station; 5] = [
    // Exactly on the x = 24 m line and due north: cell boundaries met head-on, the degenerate
    // axis-aligned case.
    Station {
        name: "aligned-run-north",
        x: 24.0,
        y: 24.0,
        heading: 0.0,
        run: true,
        frames: 600,
    },
    // Four metres inside the cell, the same walk: the bracket that says the aligned answer is not
    // an artefact of the alignment.
    Station {
        name: "offset-run-north",
        x: 28.0,
        y: 24.0,
        heading: 0.0,
        run: true,
        frames: 600,
    },
    // The same again at walking speed. The lag is speed/30 Hz, so this station is the one that
    // says so: 2.606 against 4.000 m/s.
    Station {
        name: "offset-walk-north",
        x: 28.0,
        y: 24.0,
        heading: 0.0,
        run: false,
        frames: 600,
    },
    // Off-axis, so the two axes cross their boundaries at different frames and nothing cancels.
    Station {
        name: "diagonal-run",
        x: 28.0,
        y: 24.0,
        heading: 0.646,
        run: true,
        frames: 600,
    },
    // Long enough to leave the landblock: the 192 m half.
    Station {
        name: "block-crossing",
        x: 24.0,
        y: 24.0,
        heading: 0.0,
        run: true,
        frames: 3_000,
    },
];

// ---------------------------------------------------------------------------------------------
// 1. The measurement
// ---------------------------------------------------------------------------------------------

/// **The lag, per station, in metres and in decisions.**
///
/// Two things are asserted of every station:
///
/// 1. **The lag is below one cell boundary.** The largest single-frame viewpoint step must be far
///    under `CELL` (24 m) and under `BLOCK` (192 m) — so the stale decision can never be more than
///    one cell or one block behind, and it can never skip one.
/// 2. **It is never more than one cell behind**, which is `max_cell_jump == 1` on every station
///    that crosses a boundary at all. A 2 would mean a frame in which the split order misses a
///    cell outright rather than reaching it late.
///
/// And the premise, because a test whose success condition is "nothing happened" scores a broken
/// setup as a pass: the body must actually have walked, the viewpoint must actually have
/// followed it, and the run must actually have crossed boundaries. A frozen viewpoint agrees with
/// itself on every frame and would satisfy both assertions above.
#[test]
fn the_one_frame_lag_is_bounded_by_one_frame_of_camera_travel_at_every_station() {
    let store = store();

    let mut total_cells = 0u32;
    let mut total_blocks = 0u32;
    for st in &STATIONS {
        let r = measure(&store, st);

        // The premise, asserted rather than assumed.
        assert_eq!(
            r.sweeps,
            u64::from(r.frames),
            "{}: one sweep per frame",
            st.name
        );
        assert!(
            r.body_travel > 2.0 * CELL,
            "{}: the body must cross at least two cells, got {:.3} m",
            st.name,
            r.body_travel
        );
        assert!(
            r.viewer_travel > 2.0 * CELL,
            "{}: the viewpoint must follow it, got {:.3} m",
            st.name,
            r.viewer_travel
        );
        assert!(
            r.cell_disagreements > 0,
            "{}: the run must cross cell boundaries",
            st.name
        );
        // And `App::frame`'s order holds the unswept-update counter at zero, which is what makes
        // the frozen-viewpoint test below a measurement of the *other* order rather than of a
        // scene that never sweeps at all.
        assert_eq!(
            r.updates,
            u64::from(r.frames) + 10,
            "{}: every step is one update",
            st.name
        );
        assert_eq!(
            r.updates_without_a_sweep, 0,
            "{}: `update` then the sweep must never leave an update unswept",
            st.name
        );

        // 1. The lag is below one cell boundary.
        assert!(
            r.max_step < CELL,
            "{}: one frame of camera travel is {:.4} m, which is not below one 24 m cell",
            st.name,
            r.max_step
        );
        // 2. And it is never more than one cell behind.
        assert_eq!(
            r.max_cell_jump, 1,
            "{}: the stale answer must be at most one cell behind, never two",
            st.name
        );

        total_cells += r.cell_disagreements;
        total_blocks += r.block_disagreements;
        eprintln!(
            "lag {:>18}: {} frames, body {:7.3} m ({:.3} m/s), viewer {:7.3} m | lag max {:.6} m \
             (unblocked {:.6} m) mean {:.6} m ({:.4}% of a 24 m cell, {:.5}% of a 192 m block) \
             | {} blocked sweeps | decisions differing: {} cell, {} block",
            st.name,
            r.frames,
            r.body_travel,
            r.body_travel / (f64::from(r.frames) * STEP),
            r.viewer_travel,
            r.max_step,
            r.max_step_unblocked,
            r.mean_step(),
            100.0 * r.max_step / CELL,
            100.0 * r.max_step / BLOCK,
            r.sweeps_blocked,
            r.cell_disagreements,
            r.block_disagreements,
        );
    }
    eprintln!(
        "sweep lag: across five stations, {total_cells} frames take a different cell decision and \
         {total_blocks} a different block decision — each of them a boundary the split order \
         reaches one frame (1/30 s) late, never one it gets wrong."
    );
    assert!(
        total_cells > 0 && total_blocks > 0,
        "the sweep measured nothing at all"
    );
}

/// Behaviour: world.viewpoint.the-cell-decision-reads-last-frames-sweep
///
/// **The mechanism of the lag, stated as a measurement rather than as a comment.**
///
/// `WorldScene::update` does not move `Character::camera.viewer`: ordinary frames move it in the
/// later camera sweep, while teleport initializes it directly. Thus `recenter` and
/// `update_viewer_cell` read a value **bit-identical** to the previous frame's swept result. That
/// makes the lag exactly one frame and not a smear, and it is why the
/// measurement above can be taken off a single run.
///
/// This is the assertion that fails if anything inside `update` ever starts writing the
/// viewpoint, which would make every number in this file mean something else.
///
/// The sweep moves the viewpoint on **exactly the frames the 30 Hz gate opened on** — 75 of these
/// 120, because a clock accumulated in `f64` steps of `1/30` lands a hair under `MIN_QUANTUM` on
/// two frames in five. The body-physics callback reaches the camera update only inside the physics
/// gate's taken arm, so on a frame that does not tick the sought position does not change and the
/// sweep re-lands on the same point. The guard is stated against the gate rather than the frame
/// count: not "it moved often" but "it moved on a tick and never off one".
#[test]
fn world_scene_update_does_not_move_the_viewpoint_which_is_why_the_lag_is_exactly_one_frame() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = embodied(&store, &mut gpu);
    stand(&mut scene, 24.0, 24.0, 0.0);

    let input = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step, narrowed for the debug camera's f32 delta.
    let dt = STEP as f32;
    let mut now = 0.0f64;
    let mut moved_by_the_sweep = 0u32;
    let mut ticked_frames = 0u32;
    let mut moved_without_a_tick = 0u32;
    let ticks = |s: &WorldScene| s.character.as_ref().expect("the body").stats.physics_ticks;
    let mut seen = ticks(&scene);

    for _ in 0..120 {
        now += STEP;
        let before = viewer_global(&scene).expect("placed");
        scene.update(CameraInput::default(), input, LocalTime(now), dt);
        let after_update = viewer_global(&scene).expect("still placed");
        assert_eq!(
            before, after_update,
            "WorldScene::update moved the viewpoint; the lag is no longer one frame"
        );
        let now_ticks = ticks(&scene);
        let ticked = now_ticks != seen;
        seen = now_ticks;
        if ticked {
            ticked_frames += 1;
        }
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            STEP,
        );
        let after_sweep = viewer_global(&scene).expect("still placed");
        if after_sweep != after_update {
            moved_by_the_sweep += 1;
            if !ticked {
                moved_without_a_tick += 1;
            }
        }
    }
    // The premise: the sweep is what moves it, and it did so on the frames that could move it.
    // Without this the equality above is satisfied by a viewpoint that never moves at all.
    assert!(
        ticked_frames > 50,
        "the 30 Hz gate opened on only {ticked_frames} of 120 frames of a 1/30 s clock"
    );
    assert!(
        moved_by_the_sweep > 50,
        "the sweep must be what moves the viewpoint, and it moved it on {moved_by_the_sweep} of \
         120 frames ({ticked_frames} of which opened the gate)"
    );
    // And it may not move on a frame the gate left shut, because the smoother that
    // produces the sought position runs only on the physics gate's taken arm.
    assert_eq!(
        moved_without_a_tick, 0,
        "the sweep moved the viewpoint on {moved_without_a_tick} frames on which the body never \
         ticked; the camera update is reachable only from the physics-updated callback and \
         so cannot have run on those"
    );
    eprintln!(
        "sweep lag: over 120 frames `update` moved the viewpoint 0 times, the gate opened \
         {ticked_frames} times and the sweep moved it {moved_by_the_sweep} times"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The harness failure mode: an update with no sweep
// ---------------------------------------------------------------------------------------------

/// **A harness that drives `WorldScene::update` without the sweep has a viewpoint that never
/// advances — and the null is structural, not incidental.**
///
/// In this harness's ordinary progression, `Character::camera.viewer` is written by the sweep or
/// directly by teleport initialization. With neither writer invoked here, travel is not "small";
/// it is exactly zero — a different
/// kind of number from a measurement that happened to come out low.
///
/// The client cannot reach this state: its sweep is unconditional and sits on the line before the
/// viewpoint is read. So this is a **harness hazard**, pinned here as a measurement so that it
/// stays one.
#[test]
fn driving_update_without_the_sweep_freezes_the_viewpoint_exactly() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene = embodied(&store, &mut gpu);
    stand(&mut scene, 24.0, 24.0, 0.0);

    let input = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a fixed simulation step, narrowed for the debug camera's f32 delta.
    let dt = STEP as f32;
    let mut now = 0.0f64;

    let start_body = body_global(&scene);
    let start_view = viewer_global(&scene).expect("the teleport placed it");
    let sweeps_before = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .stats
        .sweeps;

    for _ in 0..600 {
        now += STEP;
        scene.update(CameraInput::default(), input, LocalTime(now), dt);
        scene
            .stream(&store, &mut gpu)
            .expect("the streamed blocks build");
    }

    let end_body = body_global(&scene);
    let end_view = viewer_global(&scene).expect("still placed");
    let body_travel =
        ((end_body.0 - start_body.0).powi(2) + (end_body.1 - start_body.1).powi(2)).sqrt();

    // The premise: the body really did run, so "the viewpoint did not move" is a fact about the
    // viewpoint and not about a simulation that did nothing.
    assert!(
        body_travel > 3.0 * CELL,
        "the body must run, got {body_travel:.3} m"
    );
    assert_eq!(
        scene
            .character
            .as_ref()
            .expect("a body")
            .camera
            .stats
            .sweeps,
        sweeps_before,
        "no sweep may have run"
    );
    // **The unswept-update counter, doing the job the pin is for.** The hazard is a number the
    // scene reports on the frame it happens, rather than a failure twenty frames downstream in
    // whichever test happens to depend on the window moving.
    assert_eq!(
        scene.draw.stats.updates, 600,
        "every loop iteration is one update"
    );
    assert_eq!(
        scene.draw.stats.updates_without_a_sweep, 599,
        "every update after the first ran with no sweep since the previous one"
    );
    // The mechanism: exactly zero, because nothing else writes the field.
    assert_eq!(
        start_view, end_view,
        "the viewpoint must be bit-identical after a {body_travel:.1} m run"
    );
    eprintln!(
        "lag: 600 frames of `update` alone moved the body {body_travel:.2} m and the viewpoint \
         0.00000 m — the field has two writers and neither was called"
    );
}

/// **A bodiless scene is not an unswept one, and the counter says so.**
///
/// `--no-character` has no body viewer at all — the flycam *is* the viewpoint on
/// that path, and `WorldScene::viewpoint` falls through to `self.camera.position`. So "no sweep"
/// is the correct state there rather than a missing frame step, and
/// `SceneStats::updates_without_a_sweep` must stay at zero however long the scene is stepped.
///
/// Without this station the counter's body-only guard is unfalsifiable: every other test in this
/// file drives a body, so a version that counted the bodiless path too would be green everywhere.
#[test]
fn a_bodiless_scene_never_counts_an_update_as_unswept() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    assert!(
        scene.character.is_none(),
        "the --no-character path, and no body was attached"
    );

    let mut now = 0.0f64;
    for _ in 0..60 {
        now += STEP;
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a fixed simulation step, narrowed for the flycam's own f32 delta.
        let dt = STEP as f32;
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dt,
        );
    }
    assert_eq!(
        scene.draw.stats.updates, 60,
        "the denominator: the scene really was stepped"
    );
    assert_eq!(
        scene.draw.stats.updates_without_a_sweep, 0,
        "a scene with no body has nothing to sweep and must not be counted as unswept"
    );
    eprintln!("sweep lag: 60 bodiless updates, 0 counted unswept");
}
