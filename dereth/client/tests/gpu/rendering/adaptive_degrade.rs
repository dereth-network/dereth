//! **Adaptive degrade**: the frame-rate governor drives detail selection, with load response
//! and recovery measured against a pinned control.
//!
//! # Oracle
//!
//! Two, and neither is a hand-written expectation about what "less detail" ought to look like.
//!
//! 1. **Retail's curve**: five response weights with `0.10 / 0.01 / 0.00 / -0.15 / -0.02`
//!    coefficients, a 30-entry bias history and the targets `min_framerate = 8`,
//!    `ideal_framerate = 10`, `max_framerate = 20`; `dereth_world_render::degrade_loop`'s unit
//!    tests assert the arithmetic. This file checks that the client runs the governor once per
//!    frame using retail's 20-entry frame-duration meter.
//! 2. **The retail dats**, for the detail. The LOD bands are the shipped `GfxObjDegradeInfo`
//!    records of the buildings, scenery and emitter meshes around Holtburg, read back from the dat
//!    by the id each placement names. No LOD threshold is invented here.
//!
//! # What is asserted, and why it is a differential
//!
//! "The loop runs" is not the gate. The gate is that **detail falls back when frames get
//! expensive and recovers when they get cheap**, which is only visible against a control. Every
//! detail differential runs the *same* scene through the *same* synthetic frame-time series
//! twice: with the governor pinned to `PINNED_DEG_MUL` (automatic degrades off, a test scene's default) and
//! with the loop live. The changing bias is the only difference between the runs. The frame-time
//! series is injected, because a test that waits for a real machine to get slow is a test whose
//! result is a property of that machine.
//!
//! The harness calls the production scene-update and draw methods used by
//! `dereth_client::app::App::frame`; it observes the governor's outputs rather than input alone.
//!
//! # Reproducibility
//!
//! The governor responds to frame duration, so uncontrolled wall time would make it machine
//! dependent. Its timing input is the simulation-clock delta; `--headless` advances that clock
//! by [`dereth_client_runtime::platform::clock::HEADLESS_STEP`], so a headless frame measures a number
//! that depends on the frame *count* and not on elapsed time. The reproducibility test asserts
//! exactly that: three runs of the identical series produce identical captures and identical bias
//! trajectories, **with the loop on**.
//!
//! # Where the detail is measured, and what is only reported
//!
//! The bias slides the switch-over distances of every record the client selects from each frame:
//! baked statics, the parts of moving objects and particles alike. The differentials measure the
//! **static world**, placement by placement: the selected levels, the placements whose level is
//! the all-`FLT_MAX` terminator (a `gfxobj_id` of 0, drawn as nothing), the object triangles that
//! reached the device, and the captures. With the bias's route to the lookup cut, all four read
//! identical on both arms, pixels included, so the picture differs because detail was selected
//! differently and for no other reason.
//!
//! The **particle** counters (`ParticleStats::degraded_out`, `drawn`, `batches`) are reported and
//! not asserted. A particle's level moves with the bias only where `viewer distance /
//! gfxobj_scale.z` lies past the 50-unit degradation distance and inside a band whose `min`,
//! `ideal` and `max` differ; most shipped particle bands are degenerate, and the emitters the
//! client builds around these stations put no particle in a movable band, so the two arms agree
//! on the particles exactly while the static world differs.

#![cfg(gpu)]

use dereth_assets::motion::GfxObjDegradeInfo;
use dereth_assets::Decode;
use dereth_client_runtime::camera::FrameRate;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::Gpu;
use dereth_world_render::degrade_loop::{DegradeGovernor, DegradeLevel, FramerateTargets};
use std::collections::HashMap;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The retail store, or **fail**: absent dats are a missing oracle, not a reason to pass, so the
/// type offers no way to skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Holtburg, no body, a pinned time of day so the sky and the landscape lighting cannot move
/// between the two halves of a differential. `auto` is the one thing under test.
fn cfg(auto: bool) -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles: true,
        auto_degrades: auto,
        ..SceneConfig::default()
    }
}

/// One frame of the app's own order, with an injected `dt`.
///
/// This drives the production scene/device sequence used by [`dereth_client::app::App::frame`],
/// like `tests/gpu/rendering/particles.rs` and `tests/gpu/rendering/interiors_through_outdoor_portals.rs`. The injected input is `dt`:
/// the application passes a simulation-clock delta to `WorldScene::update`, and here it is
/// supplied rather than measured. That is what makes a slow frame slow without loading the GPU,
/// and it is the same substitution `--headless`'s `Clock::fixed_step` already makes.
struct Harness {
    stream: ObjectStream,
    now: f64,
}

impl Harness {
    fn new() -> Self {
        Self {
            stream: ObjectStream::new(),
            now: 0.0,
        }
    }

    fn frame(
        &mut self,
        store: &Arc<RetailDatStore>,
        gpu: &mut Gpu,
        scene: &mut WorldScene,
        dt: f64,
    ) -> Vec<u8> {
        self.now += dt;
        scene
            .sync_objects(store, gpu, &mut self.stream)
            .expect("sync_objects");
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the frame delta the binary itself narrows on the line above `world.update`.
        let dt32 = dt as f32;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(self.now),
            dt32,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }
}

/// The synthetic frame-time series both halves of every differential are driven with.
///
/// Three phases of 60 frames, chosen against the retail targets rather than against taste:
///
/// * **cheap** — `1/30 s`, which is [`dereth_client_runtime::platform::clock::HEADLESS_STEP`] itself. The window then
///   reads 30 fps, past `g = max_framerate * 1.25 = 25` and therefore in `wg`'s saturated arm,
///   where the step is exactly `+0.10`. This illustrates the timing hazard: even 30 fps is
///   already above the fast-response threshold of retail's curve, whose ideal is 10.
/// * **expensive** — `1/8 s`, so the window reports 8 fps. `wb` is exactly zero there (its support
///   is `(8, 9.5)`, open at both ends) and `wa` is `1 - |8 - 6| / 3 = 1/3`, the only non-zero
///   weight, so the step is exactly `-0.15`.
/// * **cheap again** — the recovery half, which is the half a one-directional test would miss.
///
/// The two values are deliberately the *mildest* that saturate each arm — `1/8 s` rather than
/// `1/5 s` — because `dt` is also the simulation's step, so an implausibly slow frame would make
/// the differential a comparison of two differently-simulated worlds rather than of two biases.
const PHASE: usize = 60;
const CHEAP_DT: f64 = dereth_client_runtime::platform::clock::HEADLESS_STEP;
const EXPENSIVE_DT: f64 = 1.0 / 8.0;

fn series() -> Vec<f64> {
    let mut v = Vec::with_capacity(PHASE * 3);
    v.extend(std::iter::repeat_n(CHEAP_DT, PHASE));
    v.extend(std::iter::repeat_n(EXPENSIVE_DT, PHASE));
    v.extend(std::iter::repeat_n(CHEAP_DT, PHASE));
    v
}

/// The stations, chosen from the **retail data**. The emitters name the camera path; the static
/// world around it is what the differentials measure.
///
/// The LOD distance adjustment is `max(abs(distance) - 50, 0)`, flooring distances below the
/// 50-unit degradation distance to zero, and its particle input is `viewer distance / gfxobj_scale.z` -- the
/// *particle's own* scale. So the bias can only change a particle's level where `viewer distance / scale`
/// lands inside that mesh's own band **and** above 50. Around Holtburg the band that satisfies
/// both is `0x010016FD`'s `(min 64, ideal 92, max 128)` -- thresholds `64 / 92 / 128` at bias
/// `-1 / 0 / +1`.
///
/// **Most shipped particle bands are degenerate** -- `min == ideal == max`, e.g. `0x01000FBF` and
/// `0x0100173E` at `(64, 64, 64)` -- and a degenerate band is one the bias provably cannot move.
/// That is a property of the data and is reported rather than worked around.
///
/// The two emitters this file measures are **named by their render-space origin, not by their
/// position in [`WorldScene::emitter_host_origins`]**: that list's membership and order are a
/// property of the client (which landblock statics become emitter hosts), so an index renumbers
/// whenever the client changes, while a coordinate is a fact about the dat.
///
/// Whether these stations still reach bias-sensitive particles depends on which emitter hosts the
/// client builds around Holtburg: when the scene builds fewer hosts (55 rather than 61), the
/// most distant ones, whose `0x0100160D` particles at scale 2.3 were the only bias-sensitive
/// population on this ladder, are absent, and both differentials measure zero with the governor
/// correctly at `-1`.
const STATION_EMITTER: Vec3 = Vec3::new(-154.986_33, -110.976_56, 35.3387);
/// The second emitter, used by the close-range station at the end of [`STATIONS`].
const NEAR_EMITTER: Vec3 = Vec3::new(86.662_11, -121.230_47, 94.005);
const HOSTS: [Vec3; 2] = [STATION_EMITTER, NEAR_EMITTER];
const STATION_HOST: usize = 0;
const STATION_RANGE: f32 = 46.0;

/// An emitter host pinned by **identity**, taken once, and carried across window re-centres.
///
/// [`WorldScene::emitter_host_origins`] is `blocks.values().flat_map(|b| b.hosts)`, so an *index*
/// into it names whichever host that slot currently holds — and the resident block set changes
/// every time the window scrolls. Re-reading `origins[index]` on every frame would chase a moving
/// target: the camera jumps to a different emitter, the jump re-centres the window, the resident
/// set changes again, and the station never settles.
///
/// The host is therefore read **once**, after [`warm_up`], and translated on every later frame:
/// the render space's origin is the viewer block's south-west corner, so a shift of `n` blocks
/// moves every point expressed in it by `-n * BLOCK_LENGTH`.
#[derive(Debug, Clone, Copy)]
struct PinnedHost {
    origin: Vec3,
    block: (i32, i32),
}

impl PinnedHost {
    /// Take the host that is **at** `want` now, together with the render space it was read in.
    ///
    /// Matched on the origin rather than taken by index -- see [`STATION_EMITTER`]. The match is
    /// asserted unique, so a client change that duplicates or moves an emitter says so here
    /// instead of silently re-aiming every station in this file.
    fn at(scene: &WorldScene, want: Vec3) -> Self {
        let origins = scene.emitter_host_origins();
        let found: Vec<Vec3> = origins
            .iter()
            .copied()
            .filter(|o| {
                let (dx, dy, dz) = (o.x - want.x, o.y - want.y, o.z - want.z);
                dx * dx + dy * dy + dz * dz < 0.01
            })
            .collect();
        assert_eq!(
            found.len(),
            1,
            "expected exactly one emitter host at {want:?}; found {found:?} among {} hosts \
             around Holtburg. This station names an emitter by where it is: if the client no \
             longer builds one there, re-measure the ladder, do not re-index it.",
            origins.len()
        );
        Self {
            origin: found[0],
            block: scene
                .viewer_block()
                .expect("a viewer block once the window has meshed"),
        }
    }

    /// The same host, in the render space of the frame about to be drawn.
    fn here(&self, scene: &WorldScene) -> Vec3 {
        let b = scene
            .viewer_block()
            .expect("a viewer block once the window has meshed");
        #[allow(clippy::cast_precision_loss)] // a landblock index difference, at most 255
        let (dx, dy) = ((b.0 - self.block.0) as f32, (b.1 - self.block.1) as f32);
        Vec3::new(
            self.origin.x - dx * dereth_terrain::consts::BLOCK_LENGTH,
            self.origin.y - dy * dereth_terrain::consts::BLOCK_LENGTH,
            self.origin.z,
        )
    }
}

/// Park the camera `range` south of the pinned emitter `host`, `height` above it, looking north.
///
/// The elevation is not decoration. The particles whose level the bias moves are the ones whose
/// `viewer distance / scale` lands inside a band, and for the shipped emitter meshes that is a *long* way off
/// — a `(64, 92, 128)` band on a particle at `gfxobj_scale.z = 3` is straddled at 192–276 m. From
/// eye height those sit behind Holtburg's own hillside, so the LOD decision changes and no pixel
/// does. Elevated stations seek a visible differential, but a pixel change is not required even
/// there; the draw-decision counters remain the asserted result.
fn park_at_an_emitter(
    scene: &mut WorldScene,
    host: &PinnedHost,
    range: f32,
    height: f32,
    yaw: f32,
) -> Vec3 {
    let o = host.here(scene);
    scene.camera.position = Vec3::new(o.x, o.y - range, o.z + height);
    scene.camera.yaw = yaw;
    scene.camera.pitch = if height > 4.0 { -0.15 } else { 0.0 };
    o
}

/// Two warm-up frames: emitter hosts are baked by the first `stream`, so the host list does not
/// exist until a frame has run.
fn warm_up(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scene: &mut WorldScene, h: &mut Harness) {
    for _ in 0..2 {
        h.frame(store, gpu, scene, CHEAP_DT);
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The loop is called, and it is called from the frame.
// ---------------------------------------------------------------------------------------------

/// **The adaptive governor runs once per `WorldScene::update`, and not otherwise.**
///
/// The counter is on the scene, so this asserts the call site rather than the callee: a complete
/// `dereth_world_render::degrade_loop` that nothing calls fails here.
#[test]
fn the_loop_runs_exactly_once_per_frame_of_the_apps_own_update_path() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    assert_eq!(
        scene.draw.degrade.frames, 0,
        "loading a scene is not a frame"
    );
    let mut h = Harness::new();
    for i in 1..=12u64 {
        h.frame(&store, &mut gpu, &mut scene, CHEAP_DT);
        assert_eq!(
            scene.draw.degrade.frames, i,
            "one degrade-level calculation per frame"
        );
    }
}

/// **The governor uses retail's frame-rate meter arithmetic over the same clock deltas
/// that drive simulation.**
///
/// The 20-entry ring starts zeroed and the divisor is the whole sum, so the first twenty frames
/// over-report — that is the client's own arithmetic and it is reproduced, not primed. Once the
/// window has filled, an injected `1/30 s` frame must read as 30 fps and a `1/8 s` frame as 8.
#[test]
fn the_measured_frame_rate_is_the_clients_twenty_frame_window() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let mut h = Harness::new();
    for _ in 0..FrameRate::WINDOW {
        h.frame(&store, &mut gpu, &mut scene, CHEAP_DT);
    }
    let fast = scene.draw.degrade.frame_rate.fps();
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the reciprocal of a frame duration, narrowed to compare against the client's own
    // single-precision frame-rate measurement.
    let want_fast = (1.0 / CHEAP_DT) as f32;
    assert!(
        (fast - want_fast).abs() < 0.1,
        "20 cheap frames must read {want_fast} fps: {fast}"
    );
    for _ in 0..FrameRate::WINDOW {
        h.frame(&store, &mut gpu, &mut scene, EXPENSIVE_DT);
    }
    let slow = scene.draw.degrade.frame_rate.fps();
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: as above.
    let want_slow = (1.0 / EXPENSIVE_DT) as f32;
    assert!(
        (slow - want_slow).abs() < 0.05,
        "20 slow frames must read {want_slow} fps: {slow}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The feedback feeds back.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.degrade.detail-falls-under-load-and-recovers
/// **The bias falls under load and recovers, on the documented curve, driven through the app.**
///
/// Oracle: retail's response curve. At 8 fps `wa = 1/3` is the only non-zero weight, so
/// the accepted step is exactly `-0.15`; at 30 fps `wg = 1` is the only one, so the step is exactly
/// `+0.10`. That 1.5:1 asymmetry is the shipped one — quality is shed faster than it is restored —
/// and it is measured here off the *scene's* governor, frame by frame, rather than off a unit
/// test's. Each phase is long enough to contain the 20-frame window the meter needs plus the
/// fourteen `-0.15` (or twenty `+0.10`) steps that cross the whole range.
#[test]
fn detail_falls_back_under_load_and_recovers_when_frames_get_cheap() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let mut h = Harness::new();
    warm_up(&store, &mut gpu, &mut scene, &mut h);
    let host = PinnedHost::at(&scene, HOSTS[STATION_HOST]);
    park_at_an_emitter(&mut scene, &host, STATION_RANGE, 1.5, 0.0);

    let mut trace = Vec::new();
    for dt in series() {
        h.frame(&store, &mut gpu, &mut scene, dt);
        trace.push(scene.draw.degrade.governor.deg_mul);
    }
    let cheap_end = trace[PHASE - 1];
    let loaded_end = trace[PHASE * 2 - 1];
    let recovered = trace[PHASE * 3 - 1];

    assert!(
        (cheap_end - 1.0).abs() < 1e-6,
        "cheap frames raise the bias to +1: {cheap_end}"
    );
    assert!(
        (loaded_end + 1.0).abs() < 1e-6,
        "expensive frames drive it to -1: {loaded_end}"
    );
    assert!(
        (recovered - 1.0).abs() < 1e-6,
        "and cheap frames bring it back: {recovered}"
    );

    // The curve, not merely the direction: the steps the loop actually took, once the 20-frame
    // window holds nothing but the phase it is in. The one step that may be short is the one that
    // lands on the clamp, `cand = clamp(deg_mul + delta, -1, +1)`, and it is allowed to be short
    // only because it lands exactly on the boundary.
    let steps = |from: usize, to: usize| -> Vec<(f32, f32)> {
        (from + 1..to)
            .map(|i| (trace[i] - trace[i - 1], trace[i]))
            .filter(|(d, _)| d.abs() > 1e-6)
            .collect()
    };
    // The `+0.10` steps are measured on the **recovery** phase, not the first one: the meter needs
    // twenty frames to fill and the bias has already saturated at `+1` inside them, so the only
    // place a `wg`-saturated step is visible with a full window is on the way back up from `-1`.
    let up = steps(PHASE * 2 + FrameRate::WINDOW, PHASE * 3);
    let down = steps(PHASE + FrameRate::WINDOW, PHASE * 2);
    assert!(!up.is_empty() && !down.is_empty(), "the bias never moved");
    for (d, at) in &up {
        assert!(
            (d - 0.10).abs() < 1e-5 || (at - 1.0).abs() < 1e-6,
            "a saturated `wg` step is +0.10 unless it clamps: saw {d} landing on {at}"
        );
    }
    for (d, at) in &down {
        assert!(
            (d + 0.15).abs() < 1e-5 || (at + 1.0).abs() < 1e-6,
            "a saturated `wa` step is -0.15 unless it clamps: saw {d} landing on {at}"
        );
    }
    eprintln!(
        "deg_mul {:.3} -> {:.3} -> {:.3}; {} steps up of +0.10, {} steps down of -0.15",
        cheap_end,
        loaded_end,
        recovered,
        up.len(),
        down.len()
    );
}

/// **A frame rate inside the curve's rest band moves nothing.**
///
/// `wc`'s coefficient is `0.00` and with the retail targets `wc` is the only weight with support
/// over `[9.5, 12.5]` fps, so the delta there is exactly zero. A loop that hunted around its
/// target would show up here as a non-zero trajectory; the client's does not hunt.
#[test]
fn a_frame_rate_in_the_rest_band_leaves_the_bias_alone() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let mut h = Harness::new();
    // 11 fps: above `wb`'s support and below `we`'s, where only `wc` is non-zero.
    //
    // The first twenty frames are **not** at 11 fps and are not expected to be: the client's
    // 20-entry ring starts zeroed and its divisor is the whole sum, so a fresh meter over-reports
    // and the loop reacts to that. What the rest band claims is that once the window has filled,
    // nothing moves any more.
    for _ in 0..FrameRate::WINDOW {
        h.frame(&store, &mut gpu, &mut scene, 1.0 / 11.0);
    }
    let fps = scene.draw.degrade.frame_rate.fps();
    assert!((fps - 11.0).abs() < 0.05, "the series reads {fps} fps");
    let settled = scene.draw.degrade.governor.deg_mul;
    for _ in 0..PHASE {
        h.frame(&store, &mut gpu, &mut scene, 1.0 / 11.0);
        assert_eq!(
            scene.draw.degrade.governor.deg_mul, settled,
            "the rest band is exact, not approximate: the loop does not hunt around it"
        );
    }
    let g = DegradeGovernor::automatic(FramerateTargets::default());
    assert_eq!(
        g.candidate(11.0),
        0.0,
        "and the candidate itself is exactly zero there"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The differential: the bias changes the detail drawn, and the captures show it.
// ---------------------------------------------------------------------------------------------

/// The static world's detail on one frame, read off the scene's own per-placement selection.
///
/// Every baked building and scenery placement around Holtburg has its level chosen each frame by
/// the same lookup the bias slides, so this is where the bias is visible on this landblock: the
/// shipped scenery records (a typical tree is `10/25/50, 25/50/100, 50/100/200` then the
/// terminator) straddle the distances the resident rings cover at every bias.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StaticDetail {
    /// Selected level indices, summed: a coarser model is a higher index.
    levels: u64,
    /// Placements whose selected level names no geometry, so nothing of them is drawn.
    culled: usize,
    /// Placements read.
    placements: usize,
}

impl std::ops::AddAssign for StaticDetail {
    fn add_assign(&mut self, o: Self) {
        self.levels += o.levels;
        self.culled += o.culled;
        self.placements += o.placements;
    }
}

/// The degrade records the scene's placements name, read back from the dat once each.
#[derive(Default)]
struct Records(HashMap<u32, GfxObjDegradeInfo>);

impl Records {
    /// Read [`StaticDetail`] off the scene's placements this frame. The terminator test is made
    /// against the record read back from the dat, not against anything the scene reports.
    fn static_detail(&mut self, store: &RetailDatStore, scene: &WorldScene) -> StaticDetail {
        let mut out = StaticDetail::default();
        for p in scene.degrade_probe() {
            let info = self.0.entry(p.record.0).or_insert_with(|| {
                let bytes = store
                    .read_typed(DbType::DegradeInfo, p.record)
                    .expect("the record the scene is holding is in the dat");
                GfxObjDegradeInfo::decode_payload(p.record, &bytes).expect("it decodes")
            });
            out.levels += u64::from(p.level);
            out.placements += 1;
            let level = usize::try_from(p.level).expect("a level index");
            if info.degrades.get(level).is_some_and(|d| d.gfxobj_id.0 == 0) {
                out.culled += 1;
            }
        }
        out
    }
}

/// A survey of one bias over the measured station and its neighbourhood.
///
/// `settle_dt` is driven for long enough to saturate the loop, then the camera is walked through
/// [`STATIONS`] and the detail counters are **accumulated over every frame** rather than sampled
/// on one. A single frame is a poor sample for the particles: whether any particle is inside the
/// narrow window where the bias can move its level depends on where that particle is in its scale
/// ramp this instant.
struct Survey {
    /// The static world's selected levels and terminator selections, summed over every frame.
    statics: StaticDetail,
    /// Object triangles drawn, summed over every frame.
    triangles: usize,
    /// Selected levels with no geometry, summed: retail's particle-draw rejection rule.
    degraded_out: usize,
    /// Particles that survived to a draw call, summed.
    drawn: usize,
    /// Draw calls the particles actually cost -- one per surface batch per drawn particle. This is
    /// what reached `Gpu::draw_dynamic`, so a change in it is a change in what the renderer did.
    batches: usize,
    /// Every capture, station by station and frame by frame — the pixel differential compares
    /// them all, because which frame a particle crosses its own LOD threshold on is a property of
    /// where it is in its scale ramp, not of the station.
    frames: Vec<Vec<u8>>,
    /// Final degradation bias.
    bias: f32,
}

/// `(host, range, height, yaw)` — the stations the survey walks.
///
/// A ladder of ranges, not a taste. `get_degrade`'s threshold for the `(min 64, ideal 92,
/// max 128)` band of `0x010016FD` is `64 / 92 / 128` at bias `-1 / 0 / +1`, and the distance it is
/// fed is `viewer distance / gfxobj_scale.z`. So the two directions of the feedback bite in **adjacent**
/// windows of that one ratio: `[64, 92)` is where a `-1` bias culls what the pinned bias draws,
/// and `[92, 128)` is where a `+1` bias draws what the pinned bias culls. Walking the camera out
/// from 44 m to 72 m walks the same particles through both, which is why one station cannot show
/// both halves and a ladder can.
///
/// The elevated and reversed stations seek pixel evidence: the LOD/draw decision is evaluated
/// for every particle regardless of the frustum, so a level that changes off-screen changes a
/// counter and no pixel.
const STATIONS: [(usize, f32, f32, f32); 11] = [
    (0, 44.0, 1.5, 0.0),
    (0, 48.0, 1.5, 0.0),
    (0, 52.0, 1.5, 0.0),
    (0, 56.0, 1.5, 0.0),
    (0, 60.0, 1.5, 0.0),
    (0, 64.0, 1.5, 0.0),
    (0, 68.0, 1.5, 0.0),
    (0, 72.0, 1.5, 0.0),
    (0, 56.0, 30.0, 0.0),
    (0, 64.0, 30.0, std::f32::consts::PI),
    (1, 10.0, 1.5, 0.0),
];
const SETTLE: usize = 60;
const AT_STATION: usize = 16;

fn survey(store: &Arc<RetailDatStore>, gpu: &mut Gpu, auto: bool, settle_dt: f64) -> Survey {
    let mut scene = WorldScene::load(store, gpu, cfg(auto)).expect("the landscape loads");
    let mut h = Harness::new();
    warm_up(store, gpu, &mut scene, &mut h);
    let pinned: Vec<PinnedHost> = HOSTS.iter().map(|o| PinnedHost::at(&scene, *o)).collect();
    park_at_an_emitter(&mut scene, &pinned[STATION_HOST], STATION_RANGE, 1.5, 0.0);
    for _ in 0..SETTLE {
        h.frame(store, gpu, &mut scene, settle_dt);
    }
    let mut records = Records::default();
    let mut out = Survey {
        statics: StaticDetail::default(),
        triangles: 0,
        degraded_out: 0,
        drawn: 0,
        batches: 0,
        frames: Vec::new(),
        bias: 0.0,
    };
    for (host, range, height, yaw) in STATIONS {
        park_at_an_emitter(&mut scene, &pinned[host], range, height, yaw);
        for _ in 0..AT_STATION {
            out.frames.push(h.frame(store, gpu, &mut scene, settle_dt));
            out.statics += records.static_detail(store, &scene);
            out.triangles += scene.draw.stats.object_triangles;
            let d = scene.drawn_particles();
            out.degraded_out += d.degraded_out;
            out.drawn += d.drawn;
            out.batches += d.batches;
        }
    }
    out.bias = scene.draw.degrade.governor.deg_mul;
    out
}

/// Pixels that differ between two surveys, frame by frame.
fn pixels_changed(a: &Survey, b: &Survey) -> usize {
    assert_eq!(a.frames.len(), b.frames.len());
    assert!(!a.frames.is_empty());
    // A zero here is only meaningful if the capture path is alive at all: two consecutive frames
    // of the *same* survey must differ, because the particles are moving.
    assert!(
        a.frames.windows(2).any(|w| w[0] != w[1]),
        "the capture path produced {} identical frames -- the differential would be vacuous",
        a.frames.len()
    );
    a.frames
        .iter()
        .zip(&b.frames)
        .map(|(x, y)| {
            let (x, y) = (x.as_chunks::<4>().0, y.as_chunks::<4>().0);
            x.iter().zip(y.iter()).filter(|(p, q)| p != q).count()
        })
        .sum()
}

/// **Detail falls back for identical scene/time inputs with a pinned versus live governor.**
///
/// The bias slides the switch-over distances of every level-of-detail record the client selects
/// from each frame: every baked building and scenery placement, every part of every moving
/// object and every particle. A negative bias pulls each threshold in toward `min_dist`, so a
/// placement at a given distance selects the same or a coarser level, and one near the end of
/// its record reaches the all-`FLT_MAX` terminator, whose `gfxobj_id` of 0 draws nothing.
///
/// The measured detail is the static world's: its selected levels and its terminator
/// selections, read placement by placement off the scene and checked against the records read
/// back from the dat. That is where the bias is visible on this landblock, and the drawn triangle
/// count and the pixels are the same change seen on the device. The particle counters are
/// reported beside them and not asserted: the particles around these stations sit either within
/// the 50-unit degradation distance, where every bias selects level 0, or in shipped bands with
/// `min == ideal == max`, which no bias can move, so on this landblock the two arms can agree on
/// them exactly.
///
/// The control is the identical scene with `--auto-degrades` off, driven with the identical
/// series through the identical stations, so the simulation state at every frame is the same in
/// both halves and the only difference is the degradation bias.
#[test]
fn a_live_governor_draws_less_detail_under_load_than_a_pinned_one() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let control = survey(&store, &mut gpu, false, EXPENSIVE_DT);
    let live = survey(&store, &mut gpu, true, EXPENSIVE_DT);

    assert_eq!(control.bias, 0.0, "the control is pinned to PINNED_DEG_MUL");
    assert!(
        (live.bias + 1.0).abs() < 1e-6,
        "the live loop is at -1 under load: {}",
        live.bias
    );

    let changed = pixels_changed(&control, &live);
    eprintln!(
        "pinned: statics {:?}, {} triangles, particles {} drawn / {} degraded out; live(-1): \
         statics {:?}, {} triangles, particles {} drawn / {} degraded out; {changed} pixels \
         differ over {} stations",
        control.statics,
        control.triangles,
        control.drawn,
        control.degraded_out,
        live.statics,
        live.triangles,
        live.drawn,
        live.degraded_out,
        STATIONS.len()
    );

    assert_eq!(
        control.statics.placements, live.statics.placements,
        "both arms must hold the same placements, or the differential compares two worlds"
    );
    assert!(
        live.statics.levels > control.statics.levels,
        "a negative bias pulls every switch-over distance in toward min_dist, so the static world \
         must select coarser levels: pinned {} vs live {}",
        control.statics.levels,
        live.statics.levels
    );
    assert!(
        live.statics.culled > control.statics.culled,
        "and more placements must reach their terminator and draw nothing: pinned {} vs live {}",
        control.statics.culled,
        live.statics.culled
    );
    assert!(
        live.triangles < control.triangles,
        "fewer object triangles must reach the device: pinned {} vs live {}",
        control.triangles,
        live.triangles
    );
    assert!(
        changed > 0,
        "and the coarser world must be visible in the captures"
    );
}

/// **The whole feedback in one differential: three phases, three biases, one control.**
///
/// A one-directional test would be satisfied by a governor that only ever subtracted, so the
/// recovery is measured too, and both halves walk the identical station ladder on the identical
/// frame-time series — so at every frame the two runs are simulating the same world and the only
/// difference between them is the degradation bias.
///
/// What is counted is retail's draw-rejection predicate over the static world: the selected
/// level's `gfxobj_id` is 0, so nothing of that placement is drawn. Against the pinned control the
/// live loop must cull **fewer** placements while it sits at `+1` (the thresholds have been pushed
/// out toward `max_dist`, so geometry survives further away), **more** while it sits at `-1`
/// (they have been pulled in toward `min_dist`), and fewer again once it has climbed back. The
/// particle terminator selections are reported beside them, for the reason given on
/// [`a_live_governor_draws_less_detail_under_load_than_a_pinned_one`].
#[test]
fn the_extra_culling_appears_under_load_and_goes_away_on_recovery() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);

    /// The tail of each phase, over which the bias has saturated and the meter's window holds
    /// nothing but that phase.
    const TAIL: usize = 20;
    /// Frames spent at one station before the ladder advances.
    const DWELL: usize = 4;
    const RANGES: [f32; 8] = [44.0, 48.0, 52.0, 56.0, 60.0, 64.0, 68.0, 72.0];

    // (static placements culled, particles degraded out, object triangles) over the tail of each
    // of the three phases.
    let run = |gpu: &mut Gpu, auto: bool| -> ([(usize, usize, usize); 3], Vec<f32>) {
        let mut scene = WorldScene::load(&store, gpu, cfg(auto)).expect("the landscape loads");
        let mut h = Harness::new();
        warm_up(&store, gpu, &mut scene, &mut h);
        let host = PinnedHost::at(&scene, HOSTS[STATION_HOST]);
        let mut records = Records::default();
        let mut buckets = [(0usize, 0usize, 0usize); 3];
        let mut trace = Vec::new();
        for (i, dt) in series().into_iter().enumerate() {
            park_at_an_emitter(
                &mut scene,
                &host,
                RANGES[(i / DWELL) % RANGES.len()],
                1.5,
                0.0,
            );
            h.frame(&store, gpu, &mut scene, dt);
            trace.push(scene.draw.degrade.governor.deg_mul);
            let phase = i / PHASE;
            if i % PHASE >= PHASE - TAIL {
                let s = records.static_detail(&store, &scene);
                let b = &mut buckets[phase];
                b.0 += s.culled;
                b.1 += scene.drawn_particles().degraded_out;
                b.2 += scene.draw.stats.object_triangles;
            }
        }
        (buckets, trace)
    };

    let (control, control_trace) = run(&mut gpu, false);
    let (live, live_trace) = run(&mut gpu, true);

    assert!(
        control_trace.iter().all(|b| *b == 0.0),
        "the control never moves"
    );
    assert!((live_trace[PHASE - 1] - 1.0).abs() < 1e-6, "cheap: +1");
    assert!((live_trace[PHASE * 2 - 1] + 1.0).abs() < 1e-6, "loaded: -1");
    assert!(
        (live_trace[PHASE * 3 - 1] - 1.0).abs() < 1e-6,
        "recovered: +1"
    );

    let excess = |p: usize| live[p].0 as i64 - control[p].0 as i64;
    let missing_triangles = |p: usize| control[p].2 as i64 - live[p].2 as i64;
    eprintln!(
        "phase tails, pinned -> live: static placements culled cheap {} -> {}, loaded {} -> {}, \
         recovered {} -> {}; particles degraded out {} -> {}, {} -> {}, {} -> {}; object \
         triangles {} -> {}, {} -> {}, {} -> {}",
        control[0].0,
        live[0].0,
        control[1].0,
        live[1].0,
        control[2].0,
        live[2].0,
        control[0].1,
        live[0].1,
        control[1].1,
        live[1].1,
        control[2].1,
        live[2].1,
        control[0].2,
        live[0].2,
        control[1].2,
        live[1].2,
        control[2].2,
        live[2].2
    );

    assert!(
        excess(1) > 0,
        "under load the live loop must cull placements the pinned control draws: {} vs {}",
        control[1].0,
        live[1].0
    );
    assert!(
        missing_triangles(1) > 0,
        "and draw fewer triangles for them"
    );
    assert!(
        excess(0) < 0,
        "at +1 the thresholds are pushed out to max_dist, so it must cull fewer than the pinned \
         control, not more: {} vs {}",
        control[0].0,
        live[0].0
    );
    assert!(
        excess(2) < 0,
        "and after the load it must be back on that side: {} vs {}",
        control[2].0,
        live[2].0
    );
    assert!(
        missing_triangles(0) < 0 && missing_triangles(2) < 0,
        "and draw more triangles, not fewer"
    );
    eprintln!(
        "excess terminator selections against the pinned control: cheap {}, loaded {}, recovered \
         {}",
        excess(0),
        excess(1),
        excess(2)
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Reproducibility.
// ---------------------------------------------------------------------------------------------

/// **With the loop live, a headless run is still byte-identical across runs.**
///
/// The loop's timing input is the simulation-clock delta, and `--headless` advances that clock by
/// a fixed quantum, so what the meter reports on frame *k* is `20 / Σ` over a window of fixed
/// quanta — a function of the frame **count**, identical on any machine. This is the assertion
/// that the acceptance gate (`--headless --frames 1 --capture out.png`, byte-identical across
/// three runs) survives a subsystem that is a function of frame rate.
#[test]
fn the_headless_path_is_reproducible_with_the_loop_live() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);

    let run = |gpu: &mut Gpu| -> (Vec<u8>, Vec<f32>) {
        let mut scene = WorldScene::load(&store, gpu, cfg(true)).expect("the landscape loads");
        let mut h = Harness::new();
        warm_up(&store, gpu, &mut scene, &mut h);
        let host = PinnedHost::at(&scene, HOSTS[STATION_HOST]);
        park_at_an_emitter(&mut scene, &host, STATION_RANGE, 1.5, 0.0);
        let mut trace = Vec::new();
        let mut rgba = Vec::new();
        for _ in 0..30 {
            // The real headless quantum, not the synthetic series: this is what
            // `--headless --frames n` actually pushes.
            rgba = h.frame(
                &store,
                gpu,
                &mut scene,
                dereth_client_runtime::platform::clock::HEADLESS_STEP,
            );
            trace.push(scene.draw.degrade.governor.deg_mul);
        }
        // Hand the scene's texture descriptors back before dropping it: three scenes with their
        // interior furniture on one device would exceed the 2,048-pair shader-visible heap, and
        // explicit release does not rely on drop-time reclamation. The scene is not used again.
        scene.release_textures(gpu);
        (rgba, trace)
    };

    let (a_px, a_trace) = run(&mut gpu);
    let (b_px, b_trace) = run(&mut gpu);
    let (c_px, c_trace) = run(&mut gpu);

    assert_eq!(
        a_trace, b_trace,
        "the bias trajectory is a function of the frame count"
    );
    assert_eq!(a_trace, c_trace);
    assert!(
        a_px == b_px && a_px == c_px,
        "three headless runs must be byte-identical"
    );
    assert!(!a_px.is_empty());
    // And it is not a trivially constant trajectory: a fixed 1/30 s quantum reads far above
    // `max_framerate * 1.25`, so the loop is genuinely running and genuinely moving.
    assert!(
        a_trace.last().copied().unwrap_or(0.0) > 0.0,
        "the loop moved: {a_trace:?}"
    );
}

/// Behaviour: rendering.degrade.the-shipped-default-is-automatic
/// **The client starts with automatic degrades on, and the player's preference turns them off.**
///
/// Oracle: retail's automatic-degradation flag starts set and `Render.AutomaticDegrades` is
/// registered true. The shipped configuration is the one the client builds from an empty profile;
/// its scene moves the bias on the injected series. The same profile with
/// `AutomaticDegrades=False` gives a pinned scene no series can move, and turning the preference
/// off live pins a running scene from its next frame. A scene a test builds directly stays pinned,
/// which is what every golden image here relies on.
#[test]
fn the_shipped_client_runs_automatic_degrades_and_the_preference_turns_them_off() {
    use dereth_client_runtime::config::{Config, Preferences};
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let shipped = Config::from_args_and_prefs_with(&[], &Preferences::parse(""))
        .expect("an empty profile parses")
        .scene_config();
    assert!(shipped.auto_degrades, "the shipped default is automatic");
    assert!(
        shipped.render.automatic_degrades,
        "and the options page shows it on"
    );
    assert!(
        !SceneConfig::default().auto_degrades,
        "a directly built scene stays pinned"
    );
    let off = Config::from_args_and_prefs_with(
        &[],
        &Preferences::parse("[Render]\r\nAutomaticDegrades=False\r\n"),
    )
    .expect("the profile parses")
    .scene_config();
    assert!(!off.auto_degrades, "the preference turns them off");

    // The shipped switch, on the test scene: the bias moves.
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            auto_degrades: shipped.auto_degrades,
            render: shipped.render,
            ..cfg(false)
        },
    )
    .expect("the landscape loads");
    let mut h = Harness::new();
    let mut moved = false;
    for dt in series() {
        h.frame(&store, &mut gpu, &mut scene, dt);
        moved |= scene.draw.degrade.governor.deg_mul != dereth_terrain::consts::PINNED_DEG_MUL;
    }
    assert!(moved, "the shipped governor never moved the bias");
    assert!(scene.degrade_globals().auto_update_deg_mul);

    // The player turns the preference off on the running scene: pinned from the next frame.
    scene.draw.cfg.render.automatic_degrades = false;
    scene
        .update_from_preferences(&store, &mut gpu)
        .expect("the preference applies");
    for dt in series() {
        h.frame(&store, &mut gpu, &mut scene, dt);
        assert_eq!(
            scene.draw.degrade.governor.deg_mul,
            dereth_terrain::consts::PINNED_DEG_MUL,
            "turned off live, the governor never moves"
        );
    }

    // Off in the profile: pinned from the start, with the manual bias.
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            auto_degrades: off.auto_degrades,
            render: off.render,
            ..cfg(false)
        },
    )
    .expect("the landscape loads");
    let mut h = Harness::new();
    for dt in series() {
        h.frame(&store, &mut gpu, &mut scene, dt);
        assert_eq!(
            scene.draw.degrade.governor.deg_mul,
            dereth_terrain::consts::PINNED_DEG_MUL,
            "a pinned governor never moves"
        );
    }
    let g = scene.degrade_globals();
    assert!(
        !g.auto_update_deg_mul,
        "pinned is automatic degrades off..."
    );
    assert_eq!(
        g.user_bias,
        dereth_terrain::consts::PINNED_DEG_MUL,
        "...with the manual bias"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. The other degradation-level outputs and the camera override.
// ---------------------------------------------------------------------------------------------

/// **All degradation-level outputs move with the loop, not only the LOD bands.**
///
/// `deg_mul` slides four things at once: the two light-pool caps, `object_distance_2dsq` and
/// `particle_distance_2dsq`. This test checks the level's outputs directly against retail's table
/// through the scene's governor, independently of their rendering consumers.
#[test]
fn the_bias_the_loop_reaches_sets_the_whole_degrade_level() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let mut h = Harness::new();
    // Before the loop first accepts a bias the outputs are the client's startup ones, whose share
    // distances the setter's zero-bias answer does not reproduce.
    assert_eq!(scene.draw.degrade.governor.level(), DegradeLevel::startup());

    for _ in 0..PHASE * 2 {
        h.frame(&store, &mut gpu, &mut scene, EXPENSIVE_DT);
    }
    let loaded = scene.draw.degrade.governor.level();
    assert_eq!(loaded, DegradeLevel::new(-1.0));
    assert_eq!(
        (loaded.max_static_lights, loaded.max_dynamic_lights),
        (20, 4)
    );
    assert!(
        (loaded.object_distance_2dsq - 0.0).abs() < 1e-3,
        "25 + 25*(-1) = 0 m"
    );
    assert!(
        (loaded.particle_distance_2dsq - 49.0).abs() < 1e-3,
        "16 + 9*(-1) = 7 m"
    );

    for _ in 0..PHASE * 2 {
        h.frame(&store, &mut gpu, &mut scene, CHEAP_DT);
    }
    let cheap = scene.draw.degrade.governor.level();
    assert_eq!(cheap, DegradeLevel::new(1.0));
    assert_eq!((cheap.max_static_lights, cheap.max_dynamic_lights), (60, 9));
    assert!(
        (cheap.object_distance_2dsq - 42.0 * 42.0).abs() < 1e-2,
        "25 + 17 = 42 m"
    );
}

/// **Map mode disables degradation; ordinary overhead look does not.**
///
/// Map mode sets the `CameraEffects::degrades_disabled` output; this test checks its propagation
/// to the scene's degradation globals. As in retail's selector, the override forces level 0 before
/// any distance calculation.
///
/// Retail's overhead look raises this flag only while map mode is active.
/// The ordinary overhead look keeps degradation enabled, an independent negative control here.
#[test]
fn the_overhead_camera_mode_disables_degrades() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let scene_cfg = SceneConfig {
        character: true,
        ..cfg(true)
    };
    let mut scene = WorldScene::load(&store, &mut gpu, scene_cfg).expect("the landscape loads");
    let region = dereth_world_data::landblock::load_region(&store).expect("the region loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the character attaches");
    let mut h = Harness::new();
    h.frame(&store, &mut gpu, &mut scene, CHEAP_DT);

    assert!(
        !scene.degrades_disabled(),
        "the default camera mode degrades normally"
    );
    assert!(!scene.degrade_globals().degrades_disabled);

    {
        let c = scene.character.as_mut().expect("--character built a body");
        let cam = &mut c.camera;
        cam.set.look_down(&mut cam.manager, true);
        assert!(
            !cam.set.effects.degrades_disabled,
            "the plain overhead look does not disable degrades"
        );
        cam.set.look_down(&mut cam.manager, false);
        cam.set.set_map_mode(&mut cam.manager, true);
        assert!(
            cam.set.effects.degrades_disabled,
            "map mode requests disabled degrades"
        );
    }
    h.frame(&store, &mut gpu, &mut scene, CHEAP_DT);
    assert!(scene.degrades_disabled(), "and the scene honours it");
    assert!(scene.degrade_globals().degrades_disabled);

    {
        let c = scene.character.as_mut().expect("a body");
        let cam = &mut c.camera;
        cam.set.set_map_mode(&mut cam.manager, false);
        cam.set.look_down(&mut cam.manager, false);
    }
    assert!(!scene.degrades_disabled(), "leaving the mode restores them");
}

// ---------------------------------------------------------------------------------------------
// 6. The bias over the whole shipped degrade corpus.
// ---------------------------------------------------------------------------------------------

/// **What the loop's output means for detail, measured over the whole shipped corpus.**
///
/// The live LOD differential above covers particles; retail reselects every part of every
/// object each frame, so the broader *meaning* of a bias swing is measured here
/// directly against the oracle: all 4,131 `GfxObjDegradeInfo` records in `client_portal.dat`, at
/// a sweep of viewer distances through the reproduced LOD-selection arithmetic.
///
/// This is a census, not a spot check: every record, every distance, no sampling.
#[test]
fn over_the_whole_degrade_corpus_the_bias_moves_the_selected_level() {
    let store = store();
    let ids = store.ids_of(dereth_dat::DbType::DegradeInfo);
    assert!(
        ids.len() > 4_000,
        "client_portal.dat holds 4,131 degrade records, saw {}",
        ids.len()
    );

    let level = |info: &dereth_assets::motion::GfxObjDegradeInfo, d: f32, bias: f32| {
        dereth_world_render::objects::degrade::get_degrade(
            info,
            d,
            &dereth_world_render::objects::degrade::DegradeGlobals {
                deg_mul: bias,
                ..dereth_world_render::objects::degrade::DegradeGlobals::default()
            },
        )
        .0
    };

    // Every distance the shipped bands actually span, on a 5 m grid out to 400 m -- past the
    // longest `max_dist` in the file.
    let distances: Vec<f32> = (0..=80).map(|i| (i * 5) as f32).collect();

    let (mut records, mut degenerate, mut moved, mut culled_earlier, mut kept_longer) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    // Records with a level whose bands are out of order.
    let mut disordered = 0usize;
    let mut pairs = 0usize;
    for id in ids {
        let Ok(bytes) = store.read_typed(dereth_dat::DbType::DegradeInfo, id) else {
            continue;
        };
        let Ok(info) =
            <dereth_assets::motion::GfxObjDegradeInfo as dereth_assets::Decode>::decode_payload(
                id, &bytes,
            )
        else {
            continue;
        };
        records += 1;
        // A band with `min == ideal == max` on every level is one no bias can move: both threshold
        // formulas collapse to the same number. The particle meshes around Holtburg are mostly of
        // this kind, which is why the pixel differential above needed a measured station.
        if info
            .degrades
            .iter()
            .all(|g| g.min_dist == g.ideal_dist && g.ideal_dist == g.max_dist)
        {
            degenerate += 1;
        }
        // **This is a property of the *records*, not of `get_degrade`.**
        // The monotonicity below holds only where a level's bands are ordered
        // `min <= ideal <= max`; a level whose `max_dist` is *below* its `ideal_dist` moves its
        // threshold **inwards** at a positive bias, so a faster machine degrades it sooner.
        // 51 of the dat's 4,131 records carry such a level -- `0x11000114`'s level 1 is
        // `min 10, ideal 20, max 4` -- and the client honours them literally. The count is
        // asserted below rather than the exclusion being a loophole. A threshold clamp instead of
        // a subtracting one (`d` in `{0} u [50, inf)`, not `[0, inf)`) would make the offending
        // bands unreachable and hide them.
        let ordered = info
            .degrades
            .iter()
            .all(|g| g.min_dist <= g.ideal_dist && g.ideal_dist <= g.max_dist);
        if !ordered {
            disordered += 1;
        }
        let mut this_moved = false;
        for &d in &distances {
            pairs += 1;
            let (lo, mid, hi) = (
                level(&info, d, -1.0),
                level(&info, d, 0.0),
                level(&info, d, 1.0),
            );
            if lo != mid || mid != hi {
                this_moved = true;
            }
            // A negative bias never picks a *finer* level than the pinned one, and a positive bias
            // never picks a coarser one: the thresholds move monotonically with the bias.
            assert!(
                !ordered || (lo >= mid && mid >= hi),
                "d={d} bias monotonicity broken on an ordered record: {lo} {mid} {hi}"
            );
            let terminator = |l: usize| info.degrades.get(l).is_some_and(|g| g.gfxobj_id.0 == 0);
            if terminator(lo) && !terminator(mid) {
                culled_earlier += 1;
            }
            if !terminator(hi) && terminator(mid) {
                kept_longer += 1;
            }
        }
        if this_moved {
            moved += 1;
        }
    }

    eprintln!(
        "{records} degrade records ({degenerate} with a band no bias can move); {moved} change \
         level somewhere on the sweep; over {pairs} (record, distance) pairs a -1 bias culls \
         {culled_earlier} that the pinned bias draws and a +1 bias draws {kept_longer} that the \
         pinned bias culls"
    );
    assert!(
        moved * 2 > records,
        "the bias must move most records somewhere: {moved} of {records}"
    );
    assert!(
        culled_earlier > 0,
        "a negative bias must cull earlier somewhere"
    );
    assert!(
        kept_longer > 0,
        "a positive bias must keep geometry longer somewhere"
    );
    assert!(degenerate < records, "not every band is degenerate");
    // The exclusion above is a measurement, not a loophole. If this number moves, a
    // new dat has changed how many records the client's own literal reading of the bands makes
    // non-monotone in the bias, and the exemption has to be re-justified rather than widened.
    eprintln!("{disordered} of {records} records carry a level with min > ideal or ideal > max");
    assert_eq!(disordered, 51, "records whose bands are out of order");
}
