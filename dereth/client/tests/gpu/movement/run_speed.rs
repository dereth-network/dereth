//! The player's own body covers ground at the rate its Run skill gives, which the client computes
//! for its own body (everyone else runs at the server-supplied rate). Fixture: the same body run
//! twice over the retail `DEFAULT_LANDBLOCK` holding the forward key through the shipped key map,
//! once with no player description (rate 1.0) and once with `first-login-walk-jump`'s recorded
//! qualities; the body is built from `early-inventory-and-casting`'s recorded player. The
//! observable is path length per simulated second, against the dat's own 4.0 m/s baseline and the
//! retail run-rate formula retyped here.
//!
//! # The run rate
//!
//! `S` is the Run skill converted to float; the special case is equality with `800.0`, not a
//! greater-than clamp:
//!
//! ```text
//! load_mod = load < 1 ? 1.0 : load < 2 ? 2.0 - load : 0.0
//! S == 800.0: 18.0 / 4.0 = 4.5
//! otherwise: ((S / (S + 200.0)) * 11.0 * load_mod + 4.0) / scale / 4.0
//! ```
//!
//! The unnamed divisor is **4.0**, and the middle load branch is `2.0 - load`.
//!
//! The quality inquiry supplies Run skill `0x18` with its full skill-bonus stack, forced to **0**
//! when *current* stamina is 0, and the current load. The enclosing object inquiry gates this
//! calculation on player identity: **the client owns this number for its own body** and falls
//! back to the server-supplied run rate for everyone else.
//!
//! # Where it lands, and which of the two multiplicands carries it
//!
//! Applying the run rate multiplies the motion's *speed*. Adding the motion then uses that
//! speed twice: `sequence velocity = motion-data velocity * speed`, and every animation's
//! framerate is multiplied by the same `speed`. These are two roads to ground displacement,
//! but the shipped cycle used here has only the second:
//!
//! * The **constant-velocity** road integrates `velocity × 1/framerate` once per frame *crossed*.
//!   Crossings per second cancel the reciprocal framerate, leaving `velocity × dt` for a fixed
//!   sequence velocity. Scaling the velocity itself still scales displacement.
//! * The **root-motion** road composes `pos_frames[i]` once per frame
//!   crossed, undivided, so distance per second is `authored translation per frame × framerate` —
//!   directly proportional to `speed`.
//!
//! And the shipped `client_portal.dat` puts the whole run on the second road: for the Aluvian male
//! (`0x09000001`), `RunForward`'s `MotionData` velocity is **zero** and the cycle is animation
//! `0x03000002`, 24 frames at 30 fps carrying **3.19999 m** of authored root motion — exactly
//! **4.0 m/s** at rate 1.0, which is also the state-velocity calculation's `RunForward`
//! multiplier `4.0`. That state-velocity calculation is not the ground-speed calculation: it is
//! used only for the velocity carried into a jump, and agrees at rate 1.0 because the cycle was
//! authored to the same 4.0.
//!
//! The observable is never a scale factor read back out of the field that set it, and never a
//! per-tick figure: the physics gate opens on every *other* headless frame, so "metres on the
//! frames that moved" is twice the speed and reads plausibly. A machine without the retail dats
//! fails here.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#![allow(clippy::pedantic)]

use dereth_client::{
    app::{App, HEADLESS_STEP},
    config::Config,
    pump::Pump,
    world::{SceneConfig, DEFAULT_LANDBLOCK},
};
use dereth_client_net::client_session::{testing::Corpus, SessionEvent};
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::{Message, Opcode};

fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

fn key(app: &mut App, code: winit::keyboard::KeyCode, down: bool, time: u32) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.key_message_for(code, down, time).unwrap();
    app.input_manager_mut().unwrap().on_message(message);
}

fn body(app: &App) -> &dereth_client::character::Character {
    app.world_state().unwrap().character.as_ref().unwrap()
}

/// The one physical unmodified map-4 binding for a movement action, resolved the way
/// `movement::jump_charge_release` resolves it: never an injected action result.
fn movement_key(app: &mut App, action: dereth_input::ActionId, down: bool, time: u32) {
    use winit::keyboard::KeyCode::*;
    let binding = app
        .input_manager_mut()
        .unwrap()
        .keys_for_action(action, dereth_input::InputMapId(4))
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .expect("shipped unmodified movement key");
    let keycode = [
        KeyW, KeyE, ArrowUp, KeyA, KeyD, ArrowLeft, ArrowRight, KeyS, KeyQ, KeyZ, KeyX, KeyC,
    ]
    .into_iter()
    .find(|keycode| {
        dereth_client::pump::scan_code_from_key_code(*keycode)
            .is_some_and(|scan| (scan & 0xff) as u32 == (binding.control.offset() & 0x7f) as u32)
    })
    .expect("known physical movement key for this retail DAT");
    key(app, keycode, down, time);
}

/// **Landblock-global** metres. `Character::render_frame` is relative to `viewer_block`, which the
/// re-centre moves; a speed measured through it would step by 192 m when it does.
fn world_xy(app: &App) -> (f64, f64) {
    let p = body(app).position();
    let b = p.cell.landblock();
    let bl = f64::from(dereth_physics::globals::BLOCK_LENGTH);
    (
        f64::from(b.x()) * bl + f64::from(p.frame.origin.x),
        f64::from(b.y()) * bl + f64::from(p.frame.origin.y),
    )
}

/// **early-inventory-and-casting's recorded `0xF745` for the player is the *login-tunnel* create.**
///
/// Its physics-state word is `0x00404410` — `HIDDEN_PS | GRAVITY_PS | IGNORE_COLLISIONS_PS |
/// EDGE_SLIDE_PS` — and retail unhides the body 6.7 s later with the `0xF74B Item_SetState` at
/// `t_rel = 24.562`, `state = 0x00400408` (`EDGE_SLIDE | GRAVITY | REPORT_COLLISIONS`). Both
/// halves are in the capture; replaying only the create is a login that never finished.
///
/// The create's word reaches the player's physical body. The create handler ends by applying the
/// unpacked state and timestamp slot 2, with no player exemption. State application
/// then checks whether this object is the player and, when `HIDDEN_PS` has cleared, clears the
/// login-tunnel state. That tail shows that arriving hidden is an expected login phase.
///
/// Position integration checks `HIDDEN_PS` (`0x4000`) before obtaining the part array's
/// animation offset and skips that offset entirely while hidden. Thus none of `RunForward`'s
/// authored root motion reaches the ground until the recorded unhide is applied.
fn unhide_the_player(app: &mut App, corpus: &Corpus, id: ObjectId) {
    let row = corpus
        .blobs
        .iter()
        .find(|b| {
            b.opcode == 0xf74b
                && b.payload[4..8] == id.0.to_le_bytes()
                && u32::from_le_bytes(b.payload[8..12].try_into().unwrap())
                    & dereth_physics::PhysicsState::HIDDEN_PS
                    == 0
        })
        .expect("early-inventory-and-casting's recorded 0xF74B unhide for the player");
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: row.payload[4..].to_vec(),
        },
        LocalTime(1.0),
    );
}

fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 640,
        height: 480,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required DATs and headless device");
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and animated body");
    frames(&mut app, 60);
    let corpus = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .expect("captured player");
    let player = corpus.blobs.iter().find(|b| b.opcode == 0xf746).unwrap();
    let id = ObjectId(u32::from_le_bytes(player.payload[4..8].try_into().unwrap()));
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf745 && b.payload[4..8] == id.0.to_le_bytes())
        .unwrap();
    let mut create = dereth_protocol::objects::ItemCreateObject::read(
        &mut dereth_protocol::Reader::new(&row.payload[4..]),
    )
    .unwrap();
    let p = body(&app).position();
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: p.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: p.frame.origin.x,
                y: p.frame.origin.y,
                z: p.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: p.frame.rotation.w,
                x: p.frame.rotation.x,
                y: p.frame.rotation.y,
                z: p.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).unwrap(),
        },
        LocalTime(1.0),
    );
    unhide_the_player(&mut app, &corpus, id);
    frames(&mut app, 60);
    assert!(body(&app).on_ground());
    let c = body(&app);
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the login create arrives with HIDDEN_PS and a hidden body never \
         advances its animation offset, so the recorded unhide has to have reached it"
    );
    app
}

fn player_description(app: &mut App, name: &str) {
    let corpus = Corpus::load(name)
        .unwrap()
        .expect("recorded player description");
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf7b0 && b.payload[12..16] == 0x13_u32.to_le_bytes())
        .expect("0013 player description");
    let desc = dereth_protocol::read_body::<dereth_protocol::login::LoginPlayerDescription>(
        &row.payload[16..],
    )
    .unwrap();
    let event = SessionEvent::PlayerDescription(Box::new(desc));
    app.objects_mut().apply_event(&event, LocalTime(1.0));
    app.apply_hud_events(std::slice::from_ref(&event));
    app.apply_interaction_events(std::slice::from_ref(&event));
}

/// Frames of run-up discarded before the window opens: the authored `Ready -> RunForward` link is
/// not the cycle whose speed this file is about.
const SETTLE: usize = 45;
/// Frames measured. Headless `App::frame` steps the simulated clock by exactly `MIN_QUANTUM`
/// (1/30 s), so this is `WINDOW / 30` simulated seconds. **Even**, and the window is opened
/// immediately after a physics tick, so it contains a whole number of gate openings: the gate opens
/// on alternate frames here and a ragged window would be one tick of travel out.
const WINDOW: usize = 120;

/// What one run of the body measured.
struct Run {
    /// Path length divided by simulated seconds, m/s. **This is the observable.**
    speed: f64,
    /// Straight-line displacement over the same window, to prove the path was not a detour around
    /// something the two runs met at different distances from the start.
    net: f64,
    ticks: u64,
}

/// Hold *forward* through the real keymap and measure the ground covered per simulated second.
///
/// The hold-run key is deliberately **not** pressed. Command dispatch stores
/// `hold_run = (param != 0)` and passes its inverse to motion interpretation, so
/// `effective_run = !hold_run`: running is the default state and
/// holding the key walks you.
fn measured_run(app: &mut App, t0: u32) -> Run {
    movement_key(
        app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        true,
        t0 + 1,
    );
    frames(app, SETTLE);
    assert_eq!(
        body(app)
            .driver()
            .movement
            .interp
            .interpreted_state
            .forward_command,
        dereth_animation::MotionCommand::RUN_FORWARD,
        "premise: the body must actually be running, or this measures a walk"
    );

    // Open the window on the frame after a gate opening, so `WINDOW` even frames hold exactly
    // `WINDOW / 2` of them.
    let seen = body(app).stats.physics_ticks;
    while body(app).stats.physics_ticks == seen {
        frames(app, 1);
    }

    let ticks0 = body(app).stats.physics_ticks;
    let start = world_xy(app);
    let mut prev = start;
    let mut path = 0.0f64;
    for _ in 0..WINDOW {
        frames(app, 1);
        let now = world_xy(app);
        path += math::hypot(now.0 - prev.0, now.1 - prev.1);
        prev = now;
    }
    let seconds = WINDOW as f64 * HEADLESS_STEP;
    Run {
        speed: path / seconds,
        net: math::hypot(prev.0 - start.0, prev.1 - start.1),
        ticks: body(app).stats.physics_ticks - ticks0,
    }
}

/// Retail's run-rate and load-modifier formulas, retyped from this file's header. Deliberately
/// **not** `dereth_animation::get_run_rate`: the production expression must not be its own
/// oracle.
fn retail_run_rate(load: f32, run_skill: i32, scale: f32) -> f32 {
    let load_mod = if load < 1.0 {
        1.0
    } else if load < 2.0 {
        2.0 - load
    } else {
        0.0
    };
    let s = run_skill as f32;
    if s == 800.0 {
        return 18.0 / 4.0;
    }
    ((s / (s + 200.0)) * 11.0 * load_mod + 4.0) / scale / 4.0
}

/// Behaviour: movement.run.the-body-covers-ground-at-the-run-skills-rate
#[test]
fn the_body_covers_ground_at_the_run_skills_rate() {
    // ---- A: no player description. The quality inquiry fails, so the motion
    // interpreter uses its initial cached run rate of 1.0.
    let mut app = setup();
    assert!(
        app.hud().player_desc(&app.objects().world).is_none(),
        "premise: run A has no weenie to inquire of"
    );
    let a = measured_run(&mut app, 100_000);
    app.shutdown();

    // ---- B: the same body, the same ground, with real recorded qualities.
    let mut app = setup();
    player_description(&mut app, "first-login-walk-jump");
    let (run_skill, load) = {
        let q = app
            .hud()
            .player_desc(&app.objects().world)
            .expect("recorded player-description qualities");
        let t = app.hud().skill_table.as_ref().expect("shipped SkillTable");
        (
            dereth_client_model::skills::inq_skill(q, t, 0x18, false).expect("a Run skill") as i32,
            dereth_client_model::inventory::burden::inq_load(q),
        )
    };
    let b = measured_run(&mut app, 200_000);
    app.shutdown();

    let expected = retail_run_rate(load, run_skill, 1.0);
    let seconds = WINDOW as f64 * HEADLESS_STEP;
    eprintln!(
        "run speed: Run skill {run_skill}, load {load:.4} -> run-rate formula = {expected:.6}. \
         Over {seconds:.3} simulated seconds: no weenie {:.4} m/s ({} ticks, net {:.3} m); \
         with qualities {:.4} m/s ({} ticks, net {:.3} m); ratio {:.6}",
        a.speed,
        a.ticks,
        a.net,
        b.speed,
        b.ticks,
        b.net,
        b.speed / a.speed
    );

    // --- premises, so a green result cannot be an accident of a body that never ran ------------
    for (name, r) in [("A", &a), ("B", &b)] {
        assert_eq!(
            r.ticks as usize,
            WINDOW / 2,
            "run {name}: the window must hold a whole number of gate openings, not {} over \
             {WINDOW} frames",
            r.ticks
        );
        assert!(
            (r.net - r.speed * seconds).abs() < r.net * 0.02,
            "run {name}: path {:.3} m but net displacement {:.3} m -- the body was deflected, so \
             the two runs did not cover comparable ground",
            r.speed * seconds,
            r.net
        );
    }
    assert!(
        expected > 1.5,
        "first-login-walk-jump's Run skill {run_skill} gives rate {expected}, too close to 1.0 for this \
         station to distinguish a wired run rate from an unwired one"
    );

    // --- the baseline is the dat's, absolutely -------------------------------------------------
    //
    // `0x09000001`'s `RunForward` cycle is animation `0x03000002`: 24 frames at 30 fps carrying
    // 3.1999915 m of authored `pos_frames` translation. At rate 1.0 that is 3.9999893 m/s, and it
    // is also the state-velocity calculation's multiplier 4.0. A ratio
    // alone would pass with both runs scaled by any constant; this pins the scale.
    assert!(
        (a.speed - 3.999_989_3).abs() < 0.02,
        "with no weenie the body must run at the shipped cycle's own authored speed, \
         3.9999893 m/s, not {:.4} m/s",
        a.speed
    );

    // --- and the Run skill has to reach the ground ---------------------------------------------
    //
    // The 2 % is whole-frame quantisation, not slack for a wrong formula. At rate 1.9758 the cycle
    // plays at 59.27 fps against a 1/15 s tick, so each tick crosses 3.95 frames and the loop can
    // only apply whole ones (`while i < frame_number.floor()`); the window ends mid-frame and the
    // cyclic wrap re-times its own leftover. Measured, that lands 0.9 % under the exact ratio.
    // Retail crosses whole frames the same way, so tightening this would be asserting a smoothness
    // the client does not have.
    let ratio = b.speed / a.speed;
    assert!(
        (ratio - f64::from(expected)).abs() < f64::from(expected) * 0.02,
        "the body covered {:.4} m/s with Run skill {run_skill} at load {load:.4}, and \
         {:.4} m/s with no weenie at all -- a ratio of {ratio:.4} where \
         the movement run-rate calculation says {expected:.4}, i.e. {:.4} m/s: the Run skill is \
         not reaching the body's run rate (`MotionEnv::run_rate`, fed by \
         `dereth_client_model::skills::inq_run_rate`).",
        b.speed,
        a.speed,
        a.speed * f64::from(expected),
    );
}
