//! The particle update decides whether to run an emitter with one cutoff for the whole emitter
//! manager (the largest of its emitters' degrade distances, floored at 100 m), where retail asks
//! each emitter about its own distance. The module pins that known difference: a narrower emitter
//! stays enabled past its own cutoff inside a manager whose other emitters reach further. A
//! per-emitter fix must replace the pinned expectation with a per-emitter acceptance, not relax
//! its denominator. The probe recomputes the manager and per-emitter answers from the shared
//! degrade data; no frame is drawn. Fixture: Holtburg's scripted placements from the retail dats,
//! plus one synthetic degrade record for the worked numbers. Fails when the retail dats are
//! absent; skips only without a device.
//!
//! Behaviour: none (it pins a known difference from retail, which decides per emitter)

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::objects::ObjectStream;
use dereth_client::world::{EmitterDegrade, SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::Gpu;

fn warp() -> Gpu {
    crate::common::software_gpu(640, 480)
}

/// The retail dats, or **fail**: a missing oracle must not read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// A scene over Holtburg with its emitters running: the flycam, so the reading is a function of
/// the camera position alone and not of a body's animation.
fn lit(store: &Arc<RetailDatStore>, gpu: &mut Gpu) -> (WorldScene, ObjectStream) {
    // Use scenery radius 2 and fixed midday. Resident scripted placements
    // determine the emitter hosts; no local body is introduced.
    let cfg = SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    let mut stream = ObjectStream::new();
    // Two seconds of sync/update/stream ordering. Synchronization spawns scripted placement
    // hosts; without it the probe reads a vacuous zero. This loop does not draw.
    let mut now = 0.0f64;
    for _ in 0..120 {
        now += 1.0 / 60.0;
        step(store, gpu, &mut scene, &mut stream, now);
    }
    (scene, stream)
}

fn step(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    now: f64,
) {
    scene
        .sync_objects(store, gpu, stream)
        .expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        dereth_client::character::CharacterInput::default(),
        LocalTime(now),
        1.0 / 60.0,
    );
    scene.stream(store, gpu).expect("stream");
}

/// Step the scene a second at `pos`, so the emitters have been updated at that distance.
fn stand_at(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    stream: &mut ObjectStream,
    pos: Vec3,
    t0: f64,
) -> f64 {
    scene.camera.position = pos;
    let mut now = t0;
    for _ in 0..60 {
        now += 1.0 / 60.0;
        step(store, gpu, scene, stream, now);
    }
    now
}

fn spread(rows: &[EmitterDegrade]) -> Vec<f32> {
    let mut d: Vec<f32> = rows.iter().map(|r| r.own_distance).collect();
    d.sort_by(f32::total_cmp);
    d.dedup();
    d
}

// ---------------------------------------------------------------------------------------------
// 1. The shape: it is a granularity bug, not another local copy of a shared function
// ---------------------------------------------------------------------------------------------

/// The update uses the shared should_draw_particles helper; the known issue is the manager-wide
/// cutoff supplied to it, not another local arithmetic copy. Here require a nonempty probe, manager distances
/// no smaller than each emitter's and more than one distinct individual cutoff across the window.
/// These predicates alone do not prove exact max equality or a mixed manager; the next test
/// explicitly searches for a mixed manager.
#[test]
fn should_draw_particles_is_called_and_the_defect_is_the_distance_it_is_handed() {
    let store = store();
    let mut gpu = warp();
    let (scene, _stream) = lit(&store, &mut gpu);
    let rows = scene.emitter_degrade_probe();

    // The denominator first: a probe over a window with no emitters would satisfy every
    // implication below vacuously.
    assert!(
        !rows.is_empty(),
        "the Holtburg window has no live emitter at all"
    );
    eprintln!(
        "{} emitter(s) over {} host(s); distinct own cut-offs in the shipped data: {:?}",
        rows.len(),
        scene.draw.stats.emitter_hosts,
        spread(&rows)
    );

    // A manager cutoff is at least each emitter cutoff. This inequality is necessary for the
    // current max-with-100m-floor implementation, but is not an independent exact-max check.
    for r in &rows {
        assert!(
            r.manager_distance >= r.own_distance,
            "emitter {} asks {} and its manager asks {}, which is below it — the fold is not a max",
            r.emitter,
            r.own_distance,
            r.manager_distance
        );
    }

    // And the shipped data really does carry more than one cut-off, or nothing about granularity
    // could ever be observed here.
    assert!(
        spread(&rows).len() > 1,
        "every emitter in this window has the same cut-off, so this window cannot show the defect"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The defect, measured: emitters running past their own cut-off
// ---------------------------------------------------------------------------------------------

/// PINNED DEFECT: require a positive cutoff-answer disagreement in a measured population.
/// Search for a manager with differing individual cutoffs, then move to their midpoint and
/// inspect the new probe. The predicate compares recomputed manager/individual distance tests;
/// a zero could reflect sampling as well as a corrected seam. A future per-emitter fix must
/// revise the probe contract and replace this deliberate defect expectation coherently.
#[test]
fn an_emitter_is_asked_about_its_manager_and_not_about_itself() {
    let store = store();
    let mut gpu = warp();
    let (mut scene, mut stream) = lit(&store, &mut gpu);

    // Require a mixed manager and choose an emitter at its smallest individual cutoff.
    // A manager whose emitters agree reads zero everywhere; the explicit search distinguishes
    // that uninformative population from this sample.
    let rows = scene.emitter_degrade_probe();
    assert!(!rows.is_empty(), "no live emitter in the window");
    let mut mixed: Option<(EmitterDegrade, f32, f32)> = None;
    for r in &rows {
        let mates: Vec<&EmitterDegrade> = rows.iter().filter(|q| q.owner == r.owner).collect();
        let lo = mates
            .iter()
            .map(|q| q.own_distance)
            .fold(f32::INFINITY, f32::min);
        let hi = mates.iter().map(|q| q.own_distance).fold(0.0f32, f32::max);
        if lo < hi && r.own_distance == lo {
            mixed = Some((*r, lo, hi));
            break;
        }
    }
    let mixed_managers = rows
        .iter()
        .map(|r| r.owner)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|o| {
            let mates: Vec<f32> = rows
                .iter()
                .filter(|q| q.owner == *o)
                .map(|q| q.own_distance)
                .collect();
            mates.iter().fold(f32::INFINITY, |a, b| a.min(*b))
                < mates.iter().fold(0.0f32, |a, b| a.max(*b))
        })
        .count();
    let managers = rows
        .iter()
        .map(|r| r.owner)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    eprintln!(
        "{} emitter(s) over {managers} manager(s); {mixed_managers} of those carry more than one cut-off, which are the only ones where the granularity is observable at all",
        rows.len()
    );
    let Some((narrow, lo, hi)) = mixed else {
        panic!(
            "no manager in the window mixes cut-offs ({} emitters, {managers} managers): the measurement is blind, not negative",
            rows.len()
        );
    };

    // Stand at the midpoint of the selected manager's individual cutoff range, relative to
    // that host. Its narrow cutoff rejects this distance while the larger manager cutoff accepts.
    let d = f32::midpoint(lo, hi);
    let o = narrow.origin;
    let t = stand_at(
        &store,
        &mut gpu,
        &mut scene,
        &mut stream,
        Vec3::new(o.x, o.y - d, o.z),
        10.0,
    );
    let at_mid = scene.emitter_degrade_probe();
    let over = at_mid
        .iter()
        .filter(|r| r.drawing_past_its_own_cutoff())
        .count();
    let live: usize = at_mid
        .iter()
        .filter(|r| r.drawing_past_its_own_cutoff())
        .map(|r| r.live)
        .sum();
    let example = at_mid
        .iter()
        .find(|r| r.drawing_past_its_own_cutoff())
        .copied();
    eprintln!(
        "standing {d:.1} m from a manager whose emitters cut off at {lo:.1} m and {hi:.1} m: {over} of {} emitter(s) in the window are past their own cut-off and inside their manager's, holding {live} live particle(s) between them",
        at_mid.len()
    );
    if let Some(r) = example {
        eprintln!(
            "worked example: emitter {} ({:#010X}) cuts off at {:.1} m by its own `GfxObjDegradeInfo`; its manager asks {:.1} m; its object's sort centre is {:.1} m -- so the per-emitter cutoff rejects this distance while the manager cutoff accepts it, and this build is holding {} live particle(s) in it",
            r.emitter, r.gfxobj.0, r.own_distance, r.manager_distance, r.cypt, r.live
        );
    }

    // Deliberately require the known cutoff disagreement. A future per-emitter correction must
    // replace this defect expectation together with the update/probe contract, not relax it.
    assert!(
        over > 0,
        "no emitter probe in the window has differing manager and individual cutoff answers"
    );
    assert!(
        example.is_some(),
        "a non-zero count with no example is an arithmetic error"
    );

    // Same-frame control: managers whose emitters share one cutoff. Require a nonempty sample
    // and no disagreement here. The manager's 100m floor means equal individual cutoffs do not
    // universally imply equal manager cutoff; zero is an observed requirement for this sample,
    // not a general arithmetic identity or an independent observation of emitter update flags.
    let one_cutoff: Vec<&EmitterDegrade> = at_mid
        .iter()
        .filter(|r| {
            let mates: Vec<f32> = at_mid
                .iter()
                .filter(|q| q.owner == r.owner)
                .map(|q| q.own_distance)
                .collect();
            mates.iter().fold(f32::INFINITY, |a, b| a.min(*b))
                == mates.iter().fold(0.0f32, |a, b| a.max(*b))
        })
        .collect();
    let control_disagree = one_cutoff
        .iter()
        .filter(|r| r.drawing_past_its_own_cutoff())
        .count();
    eprintln!(
        "control, same frame: {control_disagree} of {} emitter(s) whose manager carries a single cut-off are past their own -- against {over} of {} window-wide",
        one_cutoff.len(),
        at_mid.len()
    );
    assert!(
        !one_cutoff.is_empty(),
        "every manager in the window mixes cut-offs, so the control has nothing to read and the non-zero above is uncalibrated"
    );
    assert_eq!(
        control_disagree, 0,
        "a single-cutoff manager disagrees with its emitter cutoff in this control sample"
    );
    let _ = t;
}

// ---------------------------------------------------------------------------------------------
// 3. Why the draw level cannot stand in for the emitter cutoff
// ---------------------------------------------------------------------------------------------

/// Pin the numeric reason a draw-level terminator cannot generally compensate for the tighter
/// emitter cutoff. This test constructs a record and checks default globals; it does not read
/// an executable image. The literal 50.0 expectation is independent of the production constant.
///
/// The emitter cutoff uses the raw maximum band distance. Degradation selection subtracts the
/// global distance before comparing its bands. For the synthetic unscaled record below, ideal
/// and maximum are both 40 m, so selection retains real geometry well after the 40 m emitter
/// cutoff and reaches the terminator around 90 m. Real particle positions, scales and bands
/// can differ; this worked case does not establish one universal 50 m gap for every emitter.
#[test]
fn the_draw_terminator_sits_s_r_degrade_distance_beyond_the_emitter_cutoff() {
    use dereth_assets::motion::{GfxObjDegradeInfo, GfxObjInfo};
    use dereth_world_render::objects::degrade::{
        get_degrade, get_max_degrade_distance, DegradeGlobals,
    };

    // Explicit numeric expectations for the default globals, without inheriting another symbol
    // on the expected side. Automatic multiplier updates are enabled, with initial multiplier zero.
    let g = DegradeGlobals::default();
    assert_eq!(g.degrade_distance, 50.0, "initial degradation distance");
    assert!(
        g.auto_update_deg_mul,
        "automatic degradation multiplier updates start enabled"
    );
    assert_eq!(
        g.deg_mul, 0.0,
        "the pinned governor, so the thresholds are the record's ideal_dist"
    );

    // Synthetic two-band record: ideal and maximum both 40 m, then an all-FLT_MAX terminator
    // with graphics-object ID zero. This isolates the cutoff difference without a DAT census.
    let info = GfxObjDegradeInfo {
        id: dereth_primitives::DataId(0x0F00_0001),
        degrades: vec![
            GfxObjInfo {
                gfxobj_id: dereth_primitives::DataId(0x0100_0001),
                degrade_mode: 1,
                min_dist: 0.0,
                ideal_dist: 40.0,
                max_dist: 40.0,
            },
            GfxObjInfo {
                gfxobj_id: dereth_primitives::DataId(0),
                degrade_mode: 1,
                min_dist: f32::MAX,
                ideal_dist: f32::MAX,
                max_dist: f32::MAX,
            },
        ],
    };
    // The emitter's own cut-off is the raw band edge — `degrades[0].max_dist` at two levels.
    assert_eq!(
        get_max_degrade_distance(&info),
        40.0,
        "the emitter cutoff is the raw maximum distance"
    );

    // The real level remains selected at 89 m; the 90.1 m check selects the terminator.
    let level_at = |d: f32| get_degrade(&info, d, &g).0;
    assert_eq!(
        level_at(40.0),
        0,
        "at the emitter's own cut-off the draw is still on level 0"
    );
    assert_eq!(level_at(89.0), 0, "and it still is 49 m past it");
    assert_eq!(
        level_at(90.1),
        1,
        "the terminator is reached at 40 + 50, not at 40"
    );

    // At 60 m this worked record still selects real geometry even though its emitter cutoff
    // is 40 m. That selection cannot substitute for the missing per-emitter distance decision;
    // this numeric check does not itself render or inspect a running emitter.
    assert!(
        level_at(60.0) == 0,
        "the draw cannot stand in for the emitter test: it is still drawing at 60 m"
    );
}
