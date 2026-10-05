//! A landblock static's animation-event queue stays bounded over a long session: every event a
//! scripted static raises is drained in the step that raised it, while the shipped ambient scripts
//! keep calling each other round their loop and their sounds reach the mixer.
//!
//! Fixture: the retail dats' default landblock (Holtburg) loaded into a scene on a software
//! device with no recording; its scripted statics are run for sixteen seconds of simulated time
//! (two warm-up), and every `EmitterHost`'s pending-event count is sampled after each second and
//! compared with the baseline, in the shape of `presentation::long_session_growth`.
//!
//! The shipped ambient scripts call each other round a loop, so an undrained queue would grow for
//! as long as the client runs. The other direction is asserted too: the drain must not make the
//! scene quieter. The re-arming loop keeps firing after the drain (`pes_hooks` climbs) and the
//! routed sounds reach `WorldScene::take_sound_events`, which is what `App::frame` hands the
//! mixer.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_runtime::audio::SoundTrigger;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::LocalTime;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// One second of simulated time per cycle, so the series is readable in seconds.
const FRAMES_PER_CYCLE: u32 = 30;
const WARMUP_CYCLES: u32 = 2;
const CYCLES: u32 = 14;

/// Behaviour: presentation.long-session.a-static-emitters-event-queue-stays-bounded
///
/// Holtburg's scripted statics, run for a quarter of a minute of simulated time, sampled after
/// every second: the pending-event count never leaves its baseline of zero.
#[test]
fn a_landblock_statics_event_queue_stays_bounded_over_a_long_session() {
    let mut gpu = crate::common::test_gpu(640, 480);
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    // One `sync_objects` is what spawns the hosts; `update` is what runs their scripts.
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("sync_objects");
    let hosts = scene.draw.stats.emitter_hosts;
    assert!(
        hosts >= 20,
        "only {hosts} scripted statics in the window; the data has more"
    );

    let mut t = 0.0;
    let mut waves = 0u64;
    let cycle = |scene: &mut WorldScene, t: &mut f64, waves: &mut u64| {
        for _ in 0..FRAMES_PER_CYCLE {
            *t += 1.0 / 30.0;
            scene.update(
                dereth_client_runtime::camera::CameraInput::default(),
                dereth_client_runtime::character::CharacterInput::default(),
                LocalTime(*t),
                1.0 / 30.0,
            );
            // `App::frame` drains this every frame into the mixer (`audio.rs`'s step 3). A station
            // that did not would be measuring its own omission.
            for s in scene.take_sound_events() {
                if matches!(s, SoundTrigger::Wave { .. }) {
                    *waves += 1;
                }
            }
        }
    };

    for _ in 0..WARMUP_CYCLES {
        cycle(&mut scene, &mut t, &mut waves);
    }
    let baseline = scene.host_pending_events();
    let drained_at_baseline = scene.draw.stats.hosts.drained;
    let pes_at_baseline = scene.draw.stats.hosts.pes_hooks;
    let waves_at_baseline = waves;

    let mut series: Vec<usize> = Vec::new();
    let mut drained: Vec<u64> = Vec::new();
    for _ in 0..CYCLES {
        cycle(&mut scene, &mut t, &mut waves);
        series.push(scene.host_pending_events());
        drained.push(scene.draw.stats.hosts.drained);
    }

    let h = scene.draw.stats.hosts;
    eprintln!(
        "static event queue: {hosts} hosts, {t:.1}s simulated. pending after each cycle: {series:?} \
         (baseline {baseline}); cumulative drained: {drained:?}"
    );
    eprintln!(
        "static event tally: drained {} = sounds {} + sound-type {} + sound-type-no-table {} + \
         in-driver {} + no-body {} + scale {} (no body {}) + unreceivable {} + retail-noop {} + \
         motion-done {}; pes {} ({} delayed); {} wave trigger(s) reached the mixer",
        h.drained,
        h.sounds,
        h.sound_types,
        h.sound_types_no_table,
        h.applied_in_driver,
        h.no_body,
        h.scale_hooks,
        h.scale_hooks_no_body,
        h.unreceivable,
        h.retail_noop,
        h.motion_done,
        h.pes_hooks,
        h.pes_hooks_delayed,
        waves,
    );

    // 1. The leak. Every sample must equal the baseline, which is what "bounded" means when the
    //    drain runs in the same step that raised them.
    for (i, n) in series.iter().enumerate() {
        assert_eq!(
            *n, baseline,
            "after cycle {i} the statics are holding {n} undrained event(s) against a baseline of \
             {baseline}. The cumulative-drained series {drained:?} is what that queue grows by \
             when nothing takes it: linear, and for as long as the session runs."
        );
    }
    assert_eq!(
        baseline, 0,
        "the drain runs inside the step that raises them, so it is exhaustive"
    );

    // 2. The tally adds up — no event was counted twice and none escaped its arm.
    assert_eq!(
        h.drained,
        h.sounds
            + h.sound_types
            + h.sound_types_no_table
            + h.applied_in_driver
            + h.no_body
            // `SetScale` and the resolvable `SoundTable` hooks have arms of their own, apart from
            // `no_body` and `sound_types_no_table`.
            + h.scale_hooks
            + h.unreceivable
            + h.retail_noop
            + h.motion_done,
        "an event was drained and landed in no arm"
    );

    // 3. It is not bounded because it is empty: the statics kept raising events all session.
    assert!(
        h.drained > drained_at_baseline,
        "nothing was raised after the baseline; the station is asserting an empty queue"
    );

    // 4. The re-arming loop still fires after the drain. Holtburg's `CALL_PES` hooks are the
    //    immediate arm, so this is the loop and not the timer; the timer has its own dat-tier
    //    test.
    assert!(
        h.pes_hooks > pes_at_baseline,
        "the shipped ambient scripts stopped calling each other once the drain was added"
    );
    assert_eq!(
        h.pes_hooks_delayed, 0,
        "a Holtburg static now arms a delayed CALL_PES timer"
    );

    // 5. The routed arm. 302 of the 2,161 scripted setups put a sound in their default script;
    //    these are those sounds reaching the mixer's queue.
    assert!(h.sounds > 0, "no static's sound hook was routed at all");
    assert!(
        waves > waves_at_baseline,
        "the sounds were counted at the drain and none reached take_sound_events"
    );
    assert_eq!(
        h.sounds, waves,
        "every routed host sound must arrive as exactly one wave trigger"
    );
}
