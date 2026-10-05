//! The player's own animation never flaps between two frames: over a held run and a held
//! sidestep at 30, 60 and 144 Hz the contact bit never changes, the animation ladder stays put, the
//! fractional frame never steps against its node's framerate, and no pose returns to the one before
//! it. The drawn body's own travel is forward on every frame; the sawtooth in its distance from the
//! camera comes from the 30 Hz physics gate against a camera smoothed every frame.
//! Fixture: `Character` on Holtburg's landblock from the retail dats, driven through the movement
//! actions; the second test runs `WorldScene::update` then `camera::update_viewer` (the frame
//! loop's order) on a headless software device. A missing dat is a failure.

#![cfg(gpu)]
#![allow(clippy::pedantic)]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_client_runtime::actions::movement::{
    action, command, on_action, CmdStruct, MovementAction,
};
use dereth_dat::RetailDatStore;
use dereth_input::ActionId;
use dereth_primitives::{DataId, LocalTime, Vec3};
use {
    dereth_client_runtime::character::Character, dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::MovementCommands,
    dereth_client_runtime::character::ALUVIAN_MALE_SCALE,
};
use {
    dereth_client_runtime::landblock::load_region,
    dereth_client_runtime::landblock::DEFAULT_LANDBLOCK, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};

/// The middle of Holtburg's own landblock, where the other embodied-body tests spawn.
const SPAWN: (f32, f32) = (96.0, 96.0);

/// The three cadences. 30 and 60 straddle `MIN_QUANTUM` (`0.03333333333333333`),
/// and 144 clears it every fifth frame, so the gate opens on a different rhythm in each.
const RATES: [f64; 3] = [30.0, 60.0, 144.0];

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn ev(a: ActionId, start: bool) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: a,
        phase: if start {
            dereth_client_runtime::actions::ActionPhase::Begin
        } else {
            dereth_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 0,
    }
}

/// One sampled display frame of the player's own body.
struct Sample {
    on_ground: bool,
    /// The current integer pose, obtained by flooring the fractional animation frame.
    pose: i32,
    /// The fractional frame advanced by the animation sequence.
    raw: f64,
    /// The playing node's framerate, whose **sign** is the direction `frame_number` may move in.
    framerate: f32,
    nodes: Vec<DataId>,
    command: dereth_animation::MotionCommand,
}

/// A settled body holding one movement key, and the display frames it produced.
fn walk(store: &Arc<RetailDatStore>, hz: f64, seconds: f64, cmd: u32) -> Vec<Sample> {
    let region = load_region(store).expect("the region decodes");
    let mut c = Character::new(store, &region, DEFAULT_LANDBLOCK, SPAWN).expect("the body");
    for i in 1..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the body must settle on the real floor before anything is measured"
    );
    assert!(
        (c.world.get(c.handle).expect("the body").scale - ALUVIAN_MALE_SCALE).abs() < 1e-6,
        "the player's part array is at the identity scale"
    );

    // *Run as Default Movement* is on, as it is on every shipped character, so the key-up state is
    // the running one.
    let mut mc = MovementCommands::default();
    let mut input = CharacterInput::default();
    mc.ui_toggles_run = true;
    assert!(mc.on_action(
        on_action(&ev(action::TOGGLE_RUN_WALK, false), |_| None),
        &mut input
    ));
    let _ = mc.take_control_retake_pending();
    c.input = input;
    for i in 1..=4 {
        c.update(LocalTime(2.0 + f64::from(i) / 30.0));
    }
    assert!(mc.on_action(
        MovementAction::SetMotion(CmdStruct {
            command: cmd,
            extent: None,
            start: Some(true)
        }),
        &mut input
    ));
    if mc.take_control_retake_pending() {
        c.take_control_from_server();
    }
    c.input = input;

    let t0 = 3.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (hz * seconds) as u32;
    let mut out = Vec::with_capacity(n as usize);
    for i in 1..=n {
        c.update(LocalTime(t0 + f64::from(i) / hz));
        let g = c.on_ground();
        let d = c.driver();
        let seq = &d.sequence;
        out.push(Sample {
            on_ground: g,
            pose: seq.curr_frame_number(),
            raw: seq.frame_number(),
            framerate: seq.curr().map_or(0.0, |i| seq.nodes()[i].framerate),
            nodes: seq.nodes().iter().map(|n| n.anim_id).collect(),
            command: d.movement.interp.interpreted_state.forward_command,
        });
    }
    out
}

/// Behaviour: movement.animation.the-players-own-animation-never-flaps-between-two-frames
/// The player's own animation, at three frame rates, for the forward run and for the sidestep,
/// whose cycle plays backwards and would hide a sign error in the monotonicity test.
///
/// * the contact bit: the walkability update is an edge detector and the only source of the
///   ground callbacks, both of which reapply the interpreted movement; alternating contact is the
///   flap.
/// * the ladder: only the motion table's same-motion branch appends; every other branch removes
///   cyclic animations and rewrites `frame_number`, restarting the animation.
/// * the fractional frame: advancement adds `framerate * dt` and never moves against the
///   framerate's sign, so a step the other way is a re-queue.
/// * `floor(frame_number)` as an A, B, A pattern, stated as a count.
/// * airborne-while-walking frames.
#[test]
fn the_players_own_animation_does_not_oscillate_between_two_frames_at_any_frame_rate() {
    let store = store();
    for cmd in [command::WALK_FORWARD, command::SIDE_STEP_RIGHT] {
        for hz in RATES {
            let s = walk(&store, hz, 20.0, cmd);
            let label = format!("cmd {cmd:#010X} at {hz} Hz");

            // --- Non-vacuity first. "It never flapped" over a body that never animated is not a
            // measurement.
            let crossings = s.windows(2).filter(|w| w[0].pose != w[1].pose).count();
            assert!(
                crossings >= 60,
                "{label}: the body must actually cross animation frames, not {crossings} in {} \
                 display frames",
                s.len()
            );
            let distinct: std::collections::BTreeSet<i32> = s.iter().map(|x| x.pose).collect();
            assert!(
                distinct.len() >= 8,
                "{label}: only {} distinct poses were reached; the cycle must play",
                distinct.len()
            );

            // --- (1) The contact bit. Ground callbacks fire only on walkability edges.
            // Transitions of CONTACT && ON_WALKABLE between sampled updates expose that churn;
            // a settled walk on this terrain should not change ground contact.
            let flips: Vec<usize> = s
                .windows(2)
                .enumerate()
                .filter(|(_, w)| w[0].on_ground != w[1].on_ground)
                .map(|(i, _)| i + 1)
                .collect();
            assert!(
                flips.is_empty(),
                "{label}: the player left the ground {} times in {} frames (at {flips:?}). Each is \
                 a walkability edge and therefore a ground-entry/ground-exit callback, and \
                 movement interpretation swaps the forward command for \
                 `MotionCommand::FALLING` on every other one.",
                flips.len(),
                s.len()
            );

            // --- (2) The ladder. Held input issues one motion; the cycle is one node and the only
            // legitimate changes are the link animations the motion table interposes on the way in.
            let ladder = s.windows(2).filter(|w| w[0].nodes != w[1].nodes).count();
            assert!(
                ladder <= 4,
                "{label}: the animation ladder changed {ladder} times in {} frames. Every branch \
                 of the motion table's sequence selection but the same-motion one removes cyclic animations, \
                 which rewrites `frame_number`.",
                s.len()
            );

            // --- The **tick** stream, which is what (3) and (4) have to be measured on.
            //
            // The 30 Hz physics gate (`MIN_QUANTUM <= elapsed`) advances the sequence on only
            // some display frames. Poses can persist across several displayed frames, producing
            // `A A B B A A`, in which `A B A` never matches even for a real alternation.
            // Consecutive repeats are collapsed here so an oscillation is `A B A` however long
            // each pose is held.
            let ticks: Vec<&Sample> = s
                .iter()
                .enumerate()
                .filter(|(i, x)| *i == 0 || (x.raw - s[i - 1].raw).abs() > 1e-12)
                .map(|(_, x)| x)
                .collect();
            assert!(
                ticks.len() >= 60,
                "{label}: only {} of {} display frames advanced the sequence at all",
                ticks.len(),
                s.len()
            );

            // --- (3) `frame_number` never moves against its node's framerate. A cycle wrap is a
            // large jump; a re-queue is a small one, and that is the whole distinction.
            let against: Vec<f64> = ticks
                .windows(2)
                .map(|w| w[1].raw - w[0].raw)
                .filter(|step| step.abs() < 4.0)
                .zip(ticks.windows(2).map(|w| f64::from(w[1].framerate)))
                .filter(|(step, dir)| *dir != 0.0 && step * dir < -1e-9)
                .map(|(step, _)| step)
                .collect();
            assert!(
                against.is_empty(),
                "{label}: the animation sequence's `frame_number` stepped against its own framerate on {} of {} \
                 stepped frames (steps {against:?}) -- ordinary sequence advancement cannot do \
                 that, so each one is the sequence being re-queued underneath it",
                against.len(),
                ticks.len()
            );

            // --- (4) As a count: pose A, pose B, pose A.
            let aba = ticks
                .windows(3)
                .filter(|w| w[0].pose == w[2].pose && w[0].pose != w[1].pose)
                .count();
            assert_eq!(
                aba, 0,
                "{label}: the drawn pose returned to the frame before it {aba} times in {} stepped \
                 frames -- that is 'oscillating very fast between 2 animation frames'",
                ticks.len()
            );

            // --- (5) Both together, as `movement::remote_walk_animation` states it.
            let airborne = s.iter().filter(|x| !x.on_ground).count();
            assert_eq!(
                airborne,
                0,
                "{label}: {airborne} of {} frames of a driven walk report no walkable contact",
                s.len()
            );

            let cmds: std::collections::BTreeSet<_> = s.iter().map(|x| x.command).collect();
            assert_eq!(
                cmds.len(),
                1,
                "{label}: the forward command changed during a held key: {cmds:?}"
            );
        }
    }
}

/// **The body's travel is forward on every frame; the sawtooth on screen is the camera's lag over
/// the 30 Hz gate, not a physics oscillation.**
///
/// Driven through `WorldScene::update` and then `camera::update_viewer`, in `App::frame`'s order.
///
/// The body's physics advances when `MIN_QUANTUM <= elapsed`; camera smoothing runs every frame.
/// The smoothing factor is `t_stiffness * dt * 10`, with initial stiffness 0.45 copied
/// into the active camera state. These different update cadences explain a sawtooth with the
/// gate's period and travel-quantum scale. This test requires forward horizontal body travel
/// and a camera-gap range greater than 0.01; it logs amplitude and reversals without prescribing
/// their exact values or ruling out every other possible oscillation.
#[test]
fn the_drawn_bodys_own_travel_is_monotone_and_the_screen_sawtooth_is_the_gate() {
    let store = store();
    for hz in RATES {
        let mut gpu = crate::common::test_gpu(320, 240);
        let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("scene");
        let region = load_region(&store).expect("the region decodes");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");

        let mut input = CharacterInput::default();
        let mut t = 0.0f64;
        #[allow(clippy::cast_possible_truncation)]
        let dtf = (1.0 / hz) as f32;
        for _ in 0..90 {
            t += 1.0 / hz;
            scene.update(Default::default(), input, LocalTime(t), dtf);
            dereth_client_runtime::camera::update_viewer(
                &mut scene,
                Default::default(),
                LocalTime(t),
                1.0 / hz,
            );
        }
        input.forward = true;
        input.run = true;

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (hz * 10.0) as u32;
        let mut body: Vec<Vec3> = Vec::new();
        let mut gap: Vec<f32> = Vec::new();
        for _ in 0..n {
            t += 1.0 / hz;
            scene.update(Default::default(), input, LocalTime(t), dtf);
            dereth_client_runtime::camera::update_viewer(
                &mut scene,
                Default::default(),
                LocalTime(t),
                1.0 / hz,
            );
            let c = scene.character.as_ref().expect("the body");
            let b = c.render_frame().origin;
            let cam = scene.camera.position;
            body.push(b);
            let (dx, dy, dz) = (b.x - cam.x, b.y - cam.y, b.z - cam.z);
            gap.push(dz.mul_add(dz, dx.mul_add(dx, dy * dy)).sqrt());
        }

        // Non-vacuity: the body has to have gone somewhere.
        let (a, z) = (body[0], body[body.len() - 1]);
        let (tx, ty) = (z.x - a.x, z.y - a.y);
        let travelled = tx.mul_add(tx, ty * ty).sqrt();
        assert!(
            travelled > 20.0,
            "{hz} Hz: the body must run, not {travelled:.2} m"
        );
        let heading = (tx / travelled, ty / travelled);

        // The body's own horizontal travel is forward on every frame -- it either stands (the gate
        // was shut) or moves forward, never back. A physics oscillation would show here.
        let mut backward = 0usize;
        let mut worst = 0.0f32;
        for w in body.windows(2) {
            let along = (w[1].x - w[0].x) * heading.0 + (w[1].y - w[0].y) * heading.1;
            if along < -1e-4 {
                backward += 1;
                worst = worst.max(-along);
            }
        }
        assert_eq!(
            backward, 0,
            "{hz} Hz: the body moved *backwards* along its own course on {backward} frames \
             (worst {worst:.4} m). The physics is oscillating, not only the picture."
        );

        // Record the camera-gap amplitude and reversals rather than imposing an exact waveform.
        // The lower bound below only requires visible variation; changes remain observable here.
        let tail = &gap[gap.len() / 2..];
        let (mn, mx) = tail
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let reversals = tail
            .windows(3)
            .filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.0)
            .count();
        eprintln!(
            "camera gap @ {hz} Hz: body->camera gap {mn:.3}..{mx:.3} m (peak-to-peak {:.3} m), \
             {reversals} reversals in {} steady frames",
            mx - mn,
            tail.len()
        );
        assert!(
            mx - mn > 0.01,
            "{hz} Hz: the sawtooth this test exists to record is not present ({:.4} m); either \
             the gate or the camera smoother changed and this measurement is now about something \
             else",
            mx - mn
        );
    }
}
