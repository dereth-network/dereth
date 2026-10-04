//! The jump key: pressing Space starts a charge (the jump power bar rises, the body stays on the
//! ground), releasing it sends `Movement_Jump 0xF61B` with the charged extent and launches the
//! body, and Escape, a server control loss or a refusing condition (no player description, too
//! much burden, crouching, airborne release) cancels the charge without a jump. Also the order of
//! the jump's wire actions and the position reports around it. Fixture: a real `App` with the
//! shipped key map and gameplay UI from the retail dats, the player's identity, description and
//! qualities from early-inventory-and-casting and first-login-walk-jump, placed on real terrain,
//! driven by physical key messages. No connection, settings or file writes.

use crate::common::sim_app::{body, frames, key, movement_key, player_description};

use dereth_client::{
    app::App,
    config::Config,
    pump::Pump,
    world::{SceneConfig, DEFAULT_LANDBLOCK},
};
use dereth_client_model::combat::PowerBarMode;
use dereth_client_net::client_session::{
    testing::{Corpus, Direction},
    SessionEvent,
};
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{physics_state, ItemSetState};
use dereth_protocol::{Message, Opcode};

fn setup() -> App {
    let mut app = crate::common::sim_app::new(Config {
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
    assert_ne!(
        create.0.physicsdesc.state & physics_state::HIDDEN_PS,
        0,
        "the recorded login create precedes its authoritative unhide"
    );
    let unhide = corpus
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == ItemSetState::OPCODE.0)
        .filter_map(|r| dereth_protocol::read_body_padded::<ItemSetState>(&r.payload[4..]).ok())
        .find(|m| m.id == id && m.state & physics_state::HIDDEN_PS == 0)
        .expect("early-inventory-and-casting's later authoritative login unhide");
    let p = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
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
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemSetState::OPCODE,
            body: dereth_protocol::write_body(&unhide).expect("recorded F74B unhide encodes"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 60);
    let character = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(character.on_ground());
    assert!(!character
        .world
        .get(character.handle)
        .unwrap()
        .state
        .is_hidden());
    assert!(!app.objects().world.combat.jump_pending);
    app
}

fn space(app: &mut App, down: bool, time: u32) {
    key(app, winit::keyboard::KeyCode::Space, down, time);
}

fn next_manual_move_and_release(app: &mut App, time: u32) {
    use dereth_client_runtime::actions::movement::action::MOVE_FORWARD;
    let before = body(app).position().frame.origin;
    movement_key(app, MOVE_FORWARD, true, time);
    frames(app, 10);
    // After Crouch -> Ready the run is accepted and physics advances, but a transition
    // animation from the dat is still pending, so ten frames can show only a short displacement
    // without any loss of control. Wait for that transition instead: the budget is every
    // current node's full frame range (one cycle included) at its own rate, doubled for the
    // 30 Hz physics gate, plus ten settled frames.
    let (budget, station) = {
        let d = body(app).driver();
        let seconds: f64 = d
            .sequence
            .nodes()
            .iter()
            .map(|n| {
                assert!(n.framerate.is_finite() && n.framerate != 0.0);
                f64::from((n.high_frame - n.low_frame).abs() + 1) / f64::from(n.framerate.abs())
            })
            .sum();
        let station: Vec<_> = d
            .sequence
            .nodes()
            .iter()
            .map(|n| (n.anim_id, n.low_frame, n.high_frame, n.framerate))
            .collect();
        ((seconds * 60.0).ceil() as usize + 10, station)
    };
    for _ in 0..budget {
        if !body(app).driver().movement.motions_pending() {
            break;
        }
        frames(app, 1);
    }
    assert!(
        !body(app).driver().movement.motions_pending(),
        "authored transition must actually finish at station{time}: {station:?}"
    );
    frames(app, 10);
    let moved = body(app).position().frame.origin;
    assert!(math::hypotf(moved.x - before.x, moved.y - before.y) > 0.25,
        "fresh physical movement station{time}: before={before:?} after={moved:?} input={:?} physics={:?} grounded={} state={:?} pending={} longjump={} ticks={}",
        app.char_input(), body(app).world.get(body(app).handle).unwrap().velocity_vector,
        body(app).on_ground(), body(app).driver().movement.interp.interpreted_state,
        body(app).driver().movement.motions_pending(), body(app).driver().movement.interp.standing_longjump,
        body(app).stats.physics_ticks);
    movement_key(app, MOVE_FORWARD, false, time + 400);
    frames(app, 12);
    let stopped = body(app).position().frame.origin;
    frames(app, 10);
    let later = body(app).position().frame.origin;
    assert!(
        math::hypotf(later.x - stopped.x, later.y - stopped.y) < 0.01,
        "physical key-up stops translation"
    );
}

fn jump_bar_draws(app: &App) -> usize {
    let ui = &app.ui().unwrap().ui;
    app.ui_draw_list()
        .iter()
        .filter(|cmd| {
            app.hud()
                .panels
                .power_bar
                .bars
                .iter()
                .filter_map(|bar| bar.element)
                .any(|root| root == cmd.who || ui.is_ancestor_of(root, cmd.who))
        })
        .count()
}

/// The phase lane supplies the real NetLink with no OS socket. This observer consumes only
/// built datagrams; it never flushes transport, advances a separate clock, or sends an answer.
mod bound_wire {
    use super::*;
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::movement::{
        MovementAutonomousPosition, MovementJump, MovementMoveToState,
    };
    use std::collections::BTreeMap;

    #[derive(Debug)]
    struct Action {
        stamp: u32,
        opcode: u32,
        payload: Vec<u8>,
    }

    #[derive(Default)]
    struct Wire {
        reassembly: dereth_transport::indicator::Indicator,
        // Keep first-observed wire order separately: sorting by stamp would conceal inversion.
        actions: Vec<Action>,
        seen: BTreeMap<u32, (u64, Vec<u8>)>,
        retransmitted_actions: usize,
    }

    impl Wire {
        fn observe(&mut self, app: &mut App) {
            let observed_time =
                LocalTime(app.frames_drawn() as f64 * dereth_client::app::HEADLESS_STEP);
            let datagrams = app
                .replay_network_mut()
                .expect("explicit replay endpoint")
                .take_outgoing();
            for (bytes, _) in datagrams {
                let packet =
                    dereth_transport::ParsedPacket::parse(&bytes).expect("real output packet");
                for blob in self.reassembly.check_in_packet(
                    &packet.fragments,
                    packet.header.rec_id,
                    observed_time,
                ) {
                    if blob.payload.get(..4) != Some(0xF7B1_u32.to_le_bytes().as_slice()) {
                        continue; // Login/cache/ACK housekeeping is not a jump action.
                    }
                    assert_eq!(blob.queue_id, 3, "actual F7B1 uses the Weenie queue");
                    let action = unpack_action(&blob.payload).expect("actual action envelope");
                    let (stamp, opcode) = (action.stamp, action.sub_type.0);
                    if let Some((first_id, first_payload)) = self.seen.get(&stamp) {
                        assert_eq!(
                            *first_id, blob.id.0,
                            "a new blob must not reuse an action stamp"
                        );
                        assert_eq!(
                            first_payload, &blob.payload,
                            "a repeated stamp cannot change its body"
                        );
                        self.retransmitted_actions += 1;
                        continue;
                    }
                    self.seen.insert(stamp, (blob.id.0, blob.payload.clone()));
                    self.actions.push(Action {
                        stamp,
                        opcode,
                        payload: blob.payload,
                    });
                }
            }
        }

        fn frames(&mut self, app: &mut App, count: usize) {
            for _ in 0..count {
                assert!(app.frame());
                self.observe(app);
            }
        }

        fn since(&self, first: u32) -> Vec<(u32, u32)> {
            self.actions
                .iter()
                .filter(|a| a.stamp >= first)
                .map(|a| (a.stamp, a.opcode))
                .collect()
        }

        fn decode<M: Message>(&self, stamp: u32) -> M {
            let row = self
                .actions
                .iter()
                .find(|a| a.stamp == stamp)
                .expect("actual wire action arrived");
            assert_eq!(row.opcode, M::OPCODE.0);
            let mut action = unpack_action(&row.payload).unwrap();
            let body = M::read(&mut action.body).expect("wire body with original alignment");
            action
                .body
                .expect_exhausted()
                .expect("whole wire body consumed");
            body
        }

        fn non_positions_since(&self, first: u32) -> Vec<u32> {
            self.since(first)
                .into_iter()
                .map(|(_, op)| op)
                .filter(|op| *op != 0xF753)
                .collect()
        }
    }

    fn next_stamp(app: &mut App) -> u32 {
        app.replay_network_mut()
            .unwrap()
            .session
            .next_action_stamp()
    }

    #[test]
    fn actual_replay_endpoint_preserves_jump_action_order_and_next_ground_reporting() {
        let mut app = setup();
        player_description(&mut app, "early-inventory-and-casting");
        // An explicit option setting, not an injected successful action: starting a jump
        // consults this bit even when no attack is active.
        app.objects_mut().world.player_system.options.set(
            dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK,
            true,
        );
        frames(&mut app, 4); // Complete the recorded description's existing UI consumers.

        let mut net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "jump-wire-station",
            "unused",
            0,
        )
        .unwrap();
        // Same explicit connection facts as objects::frame_phase_ordering::Peer, no socket/handshake.
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEADBEEF,
            0x12345678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        app.attach_replay_network(net)
            .expect("phase-owned socket-free endpoint");
        let mut wire = Wire::default();
        wire.frames(&mut app, 4); // Bootstrap F61C/F753 are real, but not caused by Space.
        let before_quiet = next_stamp(&mut app);
        wire.frames(&mut app, 2);
        assert_eq!(
            next_stamp(&mut app),
            before_quiet,
            "grounded input-free fixture is settled"
        );
        assert!(
            app.position_reporter_stats().position_events > 0,
            "real initial F753 owner ran"
        );
        assert!(body(&app).on_ground());
        assert!(app
            .objects()
            .world
            .player_system
            .options
            .auto_repeat_attack());
        let baseline = app.position_reporter_stats();
        let first = next_stamp(&mut app);
        assert!(
            first < u32::MAX - 100,
            "this bounded station does not wrap action stamps"
        );

        space(&mut app, true, 800_000);
        wire.frames(&mut app, 1);
        assert!(app.objects().world.combat.jump_pending);
        assert_eq!(
            next_stamp(&mut app),
            first + 2,
            "attack cancellation then F61C enqueue once"
        );
        assert_eq!(
            app.position_reporter_stats().movement_events,
            baseline.movement_events + 1
        );
        assert_eq!(
            app.position_reporter_stats().jump_events,
            baseline.jump_events
        );
        assert!(
            wire.since(first).is_empty(),
            "packet processing preceded this input; no test flush"
        );

        let mut pump = Pump::new();
        let repeated = pump
            .key_message_for(winit::keyboard::KeyCode::Space, true, 800_100)
            .unwrap();
        app.input_manager_mut()
            .unwrap()
            .on_message(dereth_client::pump::Win32Message {
                lparam: repeated.lparam | (1 << 30),
                ..repeated
            });
        wire.frames(&mut app, 1);
        assert_eq!(wire.since(first), [(first, 0x1B7), (first + 1, 0xF61C)]);
        // The outgoing queue has wire and fragment capacity: queued short actions drain
        // at the next scheduled transport send phase, not an arbitrary retry-until-success loop.
        wire.frames(&mut app, 7);
        assert_eq!(
            next_stamp(&mut app),
            first + 2,
            "repeat/hold and later raw-state poll add no action"
        );
        assert_eq!(wire.since(first), [(first, 0x1B7), (first + 1, 0xF61C)]);
        let _: dereth_protocol::combat::CombatCancelAttack = wire.decode(first);
        let movement: MovementMoveToState = wire.decode(first + 1);
        assert!(movement.0.contact);
        assert!(
            movement.0.longjump_mode,
            "F61C reads charge flag before charge completion"
        );
        let pre_release = body(&app).position();

        space(&mut app, false, 800_300);
        wire.frames(&mut app, 1);
        assert_eq!(
            next_stamp(&mut app),
            first + 3,
            "release enqueues only F61B, not F61C/F753"
        );
        assert!(!body(&app).on_ground());
        assert_eq!(
            app.position_reporter_stats().position_events,
            baseline.position_events
        );
        assert_eq!(wire.since(first), [(first, 0x1B7), (first + 1, 0xF61C)]);
        space(&mut app, false, 800_301); // Duplicate up is input, not a fabricated answer.
        wire.frames(&mut app, 1);
        assert_eq!(
            wire.since(first),
            [(first, 0x1B7), (first + 1, 0xF61C), (first + 2, 0xF61B)]
        );
        assert_eq!(next_stamp(&mut app), first + 3);
        let jump: MovementJump = wire.decode(first + 2);
        assert_eq!(
            jump.0.extent.to_bits(),
            0.3_f32.to_bits(),
            "nine actual fixed Timer intervals"
        );
        assert_eq!(
            jump.0.velocity,
            dereth_protocol::types::Vec3 {
                x: 0.0,
                y: 0.0,
                z: 2.619_160_2
            }
        );
        assert_eq!(jump.0.position.objcell_id, pre_release.cell.0);
        assert_eq!(
            jump.0.position.frame.origin,
            dereth_protocol::types::Vec3 {
                x: pre_release.frame.origin.x,
                y: pre_release.frame.origin.y,
                z: pre_release.frame.origin.z,
            }
        );
        assert_eq!(
            jump.0.timestamps, movement.0.timestamps,
            "same unmodified source timestamp quartet"
        );
        wire.frames(&mut app, 2); // Observe a due physics step separately from immediate wire enqueue.
        assert!(body(&app).position().frame.origin.z > pre_release.frame.origin.z + 0.05);
        wire.frames(&mut app, 60);
        assert!(body(&app).on_ground());
        assert_eq!(wire.non_positions_since(first), [0x1B7, 0xF61C, 0xF61B]);
        assert_eq!(
            app.position_reporter_stats().jump_events,
            baseline.jump_events + 1
        );

        // A fresh ordinary tap (Space down and up with no movement key between them).
        app.objects_mut().world.player_system.options.set(
            dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK,
            false,
        );
        let second = next_stamp(&mut app);
        space(&mut app, true, 801_000);
        space(&mut app, false, 801_001);
        wire.frames(&mut app, 1);
        assert_eq!(
            next_stamp(&mut app),
            second + 2,
            "option-off tap is F61C then F61B only"
        );
        assert!(wire.since(second).is_empty());
        space(&mut app, false, 801_002);
        wire.frames(&mut app, 1);
        assert_eq!(wire.since(second), [(second, 0xF61C), (second + 1, 0xF61B)]);
        let tap_movement: MovementMoveToState = wire.decode(second);
        let tap: MovementJump = wire.decode(second + 1);
        assert!(tap_movement.0.longjump_mode && tap_movement.0.contact);
        assert_eq!(tap.0.extent.to_bits(), 0.001_f32.to_bits());
        assert_eq!(tap.0.velocity.z.to_bits(), 2.619_160_2_f32.to_bits());
        wire.frames(&mut app, 2);
        wire.frames(&mut app, 60);
        assert!(body(&app).on_ground());
        assert_eq!(wire.non_positions_since(second), [0xF61C, 0xF61B]);
        assert_eq!(
            app.position_reporter_stats().jump_events,
            baseline.jump_events + 2
        );

        // F753 is conditional, not a fourth jump message: sustained ground travel produces it
        // on its own schedule, and the physical key-up stops the travel. Whether F61B resets
        // the position-report deadline is `jump_report`'s claim, not this one.
        let walking = next_stamp(&mut app);
        let reports = app.position_reporter_stats().position_events;
        let before_walk = body(&app).position().frame.origin;
        movement_key(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
            true,
            802_000,
        );
        wire.frames(&mut app, 45); // Fixed scenario budget >1s, not an exact network cadence.
        let after_walk = body(&app).position().frame.origin;
        assert!(math::hypotf(after_walk.x - before_walk.x, after_walk.y - before_walk.y) > 0.25);
        assert!(
            app.position_reporter_stats().position_events > reports,
            "grounded F753 producer ran"
        );
        let positions: Vec<_> = wire
            .since(walking)
            .into_iter()
            .filter(|(_, op)| *op == 0xF753)
            .map(|(stamp, _)| stamp)
            .collect();
        assert!(
            !positions.is_empty(),
            "actual F753 reached the replay endpoint"
        );
        for stamp in positions {
            let position: MovementAutonomousPosition = wire.decode(stamp);
            assert_eq!(position.0.contact, 1);
            assert_ne!(position.0.position.objcell_id, 0);
        }
        movement_key(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
            false,
            803_000,
        );
        wire.frames(&mut app, 12);
        let stopped = body(&app).position().frame.origin;
        wire.frames(&mut app, 10);
        let later = body(&app).position().frame.origin;
        assert!(math::hypotf(later.x - stopped.x, later.y - stopped.y) < 0.01);
        assert_eq!(wire.non_positions_since(walking), [0xF61C, 0xF61C]);
        assert_eq!(
            app.position_reporter_stats().jump_events,
            baseline.jump_events + 2
        );
        eprintln!(
            "wire actions={} identical retransmitted actions={}",
            wire.actions.len(),
            wire.retransmitted_actions
        );
        app.shutdown();
    }
}

/// Behaviour: movement.jump-charge.a-cancelled-charge-sends-no-jump
#[test]
fn escape_then_release_then_fresh_press_in_one_batch_cancels_only_the_old_charge() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    space(&mut app, true, 300_000);
    frames(&mut app, 5);
    assert!(app.objects().world.combat.jump_pending);
    assert!(body(&app).driver().movement.interp.standing_longjump);
    let old_start = app.objects().world.combat.build_start_time;
    key(&mut app, winit::keyboard::KeyCode::Escape, true, 300_100);
    space(&mut app, false, 300_101);
    space(&mut app, true, 300_102);
    frames(&mut app, 1);
    assert_eq!(
        app.jump_counts(),
        (0, 0),
        "Escape's synchronous charge cancellation precedes old key-up"
    );
    assert!(
        app.last_jump_request().is_none(),
        "cancelled old release must not produce F61B"
    );
    assert!(
        app.objects().world.combat.jump_pending,
        "fresh press after cancel starts next charge"
    );
    assert!(app.objects().world.combat.build_start_time > old_start);
    assert!(body(&app).on_ground());
    assert_eq!(app.interaction().stats.escape_finish_jumps, 1);
    // Opposite order: cancel this freshly started charge, then press before Escape.
    key(&mut app, winit::keyboard::KeyCode::Escape, false, 300_200);
    key(&mut app, winit::keyboard::KeyCode::Escape, true, 300_201);
    space(&mut app, false, 300_202);
    frames(&mut app, 1);
    assert!(!app.objects().world.combat.jump_pending);
    key(&mut app, winit::keyboard::KeyCode::Escape, false, 300_203);
    space(&mut app, true, 300_204);
    key(&mut app, winit::keyboard::KeyCode::Escape, true, 300_205);
    space(&mut app, false, 300_206);
    frames(&mut app, 1);
    assert!(!app.objects().world.combat.jump_pending);
    assert!(!body(&app).driver().movement.interp.standing_longjump);
    assert_eq!(app.jump_counts(), (0, 0));
    assert_eq!(
        app.interaction().stats.escape_finish_jumps,
        3,
        "each shared first leg runs exactly once"
    );
    next_manual_move_and_release(&mut app, 301_000);
    app.shutdown();
}

#[test]
fn movement_prefix_standing_charge_and_batched_tap_use_the_real_command_owner() {
    use dereth_client_runtime::actions::movement::action::{MOVE_FORWARD, TURN_LEFT};
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    // Forward before press is already interpreted when charge_jump observes standing state.
    movement_key(&mut app, MOVE_FORWARD, true, 400_000);
    space(&mut app, true, 400_001);
    frames(&mut app, 1);
    assert!(app.objects().world.combat.jump_pending);
    assert!(!body(&app).driver().movement.interp.standing_longjump);
    frames(&mut app, 4);
    space(&mut app, false, 400_200);
    frames(&mut app, 1);
    let v = app.last_jump_request().unwrap().0.velocity;
    assert!(
        math::hypotf(v.x, v.y) > 0.1,
        "moving jump carries the source horizontal impulse"
    );
    movement_key(&mut app, MOVE_FORWARD, false, 400_201);
    frames(&mut app, 65);
    assert!(body(&app).on_ground());

    // Standing press followed by forward updates the interpreted state, but the retail charge
    // gate holds the actual translation until release. Do not impose a false Ready-state oracle.
    space(&mut app, true, 401_000);
    movement_key(&mut app, MOVE_FORWARD, true, 401_001);
    frames(&mut app, 1);
    assert!(body(&app).driver().movement.interp.standing_longjump);
    let before = body(&app).position().frame.origin;
    frames(&mut app, 6);
    let charged = body(&app).position().frame.origin;
    assert!(math::hypotf(charged.x - before.x, charged.y - before.y) < 0.01);
    space(&mut app, false, 401_300);
    frames(&mut app, 1);
    assert!(!body(&app).driver().movement.interp.standing_longjump);
    movement_key(&mut app, MOVE_FORWARD, false, 401_301);
    frames(&mut app, 65);
    assert!(body(&app).on_ground());

    movement_key(&mut app, TURN_LEFT, true, 402_000);
    space(&mut app, true, 402_001);
    frames(&mut app, 1);
    assert!(
        !body(&app).driver().movement.interp.standing_longjump,
        "turn-before-press is not standing"
    );
    key(&mut app, winit::keyboard::KeyCode::Escape, true, 402_100);
    space(&mut app, false, 402_101);
    movement_key(&mut app, TURN_LEFT, false, 402_102);
    frames(&mut app, 10);
    assert_eq!(app.jump_counts(), (2, 0));
    // A complete physical tap in one InputManager batch is still a pending release, extent .001.
    space(&mut app, true, 403_000);
    space(&mut app, false, 403_001);
    frames(&mut app, 1);
    assert_eq!(app.jump_counts(), (3, 0));
    assert_eq!(app.last_jump_request().unwrap().0.extent, 0.001);
    frames(&mut app, 65);
    next_manual_move_and_release(&mut app, 404_000);
    assert_eq!(
        app.jump_counts(),
        (3, 0),
        "later physics/manual input does not repeat a jump"
    );
    app.shutdown();
}

/// A constructed private quality update, delivered the way the player description is.
fn ui_event<M: Message>(app: &mut App, message: M) {
    let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
    blob.extend(dereth_protocol::write_body(&message).unwrap());
    let event = SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob,
    };
    app.objects_mut().apply_event(&event, LocalTime(8.0));
    app.apply_hud_events(std::slice::from_ref(&event));
    app.apply_interaction_events(std::slice::from_ref(&event));
}

fn server_state(
    app: &mut App,
    command: dereth_animation::MotionCommand,
    flags: u8,
    autonomous: bool,
) {
    use dereth_protocol::movement::{
        InterpretedMotionState, MovementBody, MovementBuffer, MovementSetObjectMovement,
    };
    let player = app.objects().player().unwrap();
    let presence = app.objects().presence(player).unwrap();
    let message = MovementSetObjectMovement {
        id: player,
        instance_sequence: presence.instance,
        movement: MovementSetObjectMovement::encode_movement(&MovementBuffer {
            movement_timestamp: presence.movement_ts.wrapping_add(1),
            server_control_timestamp: presence.server_control_ts.wrapping_add(1),
            autonomous,
            body: MovementBody {
                current_style: 61,
                motion_flags: flags,
                interpreted: Some(InterpretedMotionState {
                    forward_command: Some(command.to_index().expect("retail command table index")),
                    ..Default::default()
                }),
                ..Default::default()
            },
        })
        .unwrap(),
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: M::OPCODE,
            body: dereth_protocol::write_body(&message).unwrap(),
        },
        LocalTime(9.0),
    );
    type M = MovementSetObjectMovement;
    frames(app, 1);
}

fn teleport_here(app: &mut App) {
    use dereth_protocol::movement::{position_flags, MovementPositionEvent, PositionPack};
    let pos = body(app).position();
    let player = app.objects().player().unwrap();
    let presence = app.objects().presence(player).unwrap();
    let msg = MovementPositionEvent {
        id: player,
        position: PositionPack {
            flags: position_flags::IS_GROUNDED,
            origin: dereth_protocol::types::Origin {
                objcell_id: pos.cell.0,
                origin: dereth_protocol::types::Vec3 {
                    x: pos.frame.origin.x,
                    y: pos.frame.origin.y,
                    z: pos.frame.origin.z,
                },
            },
            orientation: dereth_protocol::types::Quat {
                w: pos.frame.rotation.w,
                x: pos.frame.rotation.x,
                y: pos.frame.rotation.y,
                z: pos.frame.rotation.z,
            },
            instance_timestamp: presence.instance,
            position_timestamp: presence.position_ts.wrapping_add(1),
            teleport_timestamp: presence.teleport_ts.wrapping_add(1),
            force_position_timestamp: presence.force_position_ts,
            ..Default::default()
        },
    };
    let before = app.player_teleports_applied();
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: MovementPositionEvent::OPCODE,
            body: dereth_protocol::write_body(&msg).unwrap(),
        },
        LocalTime(9.1),
    );
    frames(app, 1);
    assert_eq!(app.player_teleports_applied(), before + 1);
}

#[test]
fn accepted_server_loss_cancels_but_bare_teleport_does_not_invent_finish_jump() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    space(&mut app, true, 600_000);
    frames(&mut app, 4);
    assert!(app.objects().world.combat.jump_pending);
    // An accepted non-autonomous server movement that also carries the standing-long-jump flag.
    // Finishing the jump on control loss clears the body's flag after that movement is applied.
    server_state(
        &mut app,
        dereth_animation::MotionCommand::READY,
        dereth_protocol::movement::motion_flags::STANDING_LONG_JUMP,
        false,
    );
    assert!(!app.objects().world.combat.jump_pending);
    assert!(!body(&app).driver().movement.interp.standing_longjump);
    space(&mut app, false, 600_200);
    frames(&mut app, 1);
    assert_eq!(app.jump_counts(), (0, 0));
    assert!(app.last_jump_request().is_none());
    assert!(app.hud().panels.power_bar.bars.iter().all(|b| !b.visible));
    next_manual_move_and_release(&mut app, 601_000);

    space(&mut app, true, 602_000);
    frames(&mut app, 4);
    let started = app.objects().world.combat.build_start_time;
    teleport_here(&mut app);
    assert!(
        app.objects().world.combat.jump_pending,
        "teleport notification leaves the pending jump charge intact"
    );
    assert_eq!(app.objects().world.combat.build_start_time, started);
    // An autonomous echo of the player's own movement is refused, so it is not a control loss.
    server_state(&mut app, dereth_animation::MotionCommand::READY, 0, true);
    assert!(app.objects().world.combat.jump_pending);
    key(&mut app, winit::keyboard::KeyCode::Escape, true, 602_200);
    space(&mut app, false, 602_201);
    frames(&mut app, 1);
    assert_eq!(app.jump_counts(), (0, 0));
    next_manual_move_and_release(&mut app, 603_000);

    // A successful jump, then a press and release while airborne. Retail accepts a charge in
    // the air but refuses the release (reason 24), so no second jump packet is sent.
    space(&mut app, true, 604_000);
    space(&mut app, false, 604_001);
    frames(&mut app, 2);
    assert!(!body(&app).on_ground());
    let success = app.last_jump_request().unwrap().clone();
    space(&mut app, true, 604_050);
    frames(&mut app, 1);
    assert!(
        app.objects().world.combat.jump_pending,
        "charge has no airborne rejection"
    );
    space(&mut app, false, 604_060);
    frames(&mut app, 1);
    assert_eq!(app.jump_counts(), (2, 1));
    assert_eq!(app.last_jump_request(), Some(&success));
    assert!(!app.objects().world.combat.jump_pending);
    frames(&mut app, 65);
    assert!(body(&app).on_ground());
    next_manual_move_and_release(&mut app, 605_000);
    server_state(&mut app, dereth_animation::MotionCommand::CROUCH, 0, false);
    assert_eq!(
        body(&app)
            .driver()
            .movement
            .interp
            .interpreted_state
            .forward_command,
        dereth_animation::MotionCommand::CROUCH,
        "actual server motion installed the forbidden stance"
    );
    space(&mut app, true, 606_000);
    space(&mut app, false, 606_001);
    frames(&mut app, 1);
    assert!(!app.objects().world.combat.jump_pending);
    assert_eq!(
        app.jump_counts(),
        (2, 1),
        "crouch refuses charge, so release is not attempted"
    );
    assert_eq!(app.last_jump_request(), Some(&success));
    server_state(&mut app, dereth_animation::MotionCommand::READY, 0, false);
    frames(&mut app, 30);
    next_manual_move_and_release(&mut app, 607_000);
    app.shutdown();
}

fn gameplay(
    app: &mut App,
) -> (
    &mut dereth_ui::UiSystem,
    &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
) {
    let shell = app.ui_mut().unwrap();
    let screen = shell.flow.current_mut().unwrap();
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        &mut shell.ui,
        any.downcast_mut().expect("actual gameplay screen"),
    )
}

#[test]
fn missing_description_and_private_load_enchantment_updates_reach_jump_and_burden_widget() {
    use dereth_client_model::enchant::ench_type;
    use dereth_protocol::qualities::{
        MagicUpdateEnchantment, PrivateUpdate, QualitiesPrivateUpdateAttribute,
        QualitiesPrivateUpdateInt,
    };
    use dereth_protocol::types::qualities::{Attribute, Enchantment, StatMod};
    let mut app = setup();
    // With no player description the jump is refused, rather than jumping with skill 0 and load 0.
    space(&mut app, true, 500_000);
    space(&mut app, false, 500_001);
    frames(&mut app, 1);
    assert!(!app.objects().world.combat.jump_pending);
    assert!(app.last_jump_request().is_none());
    assert_eq!(app.jump_counts(), (0, 0));
    player_description(&mut app, "early-inventory-and-casting");
    ui_event(
        &mut app,
        QualitiesPrivateUpdateAttribute(PrivateUpdate {
            sequence: 1,
            property_id: 1,
            value: Attribute {
                init_level: 100,
                ..Default::default()
            },
        }),
    );
    ui_event(
        &mut app,
        QualitiesPrivateUpdateInt(PrivateUpdate {
            sequence: 1,
            property_id: 5,
            value: 30_000,
        }),
    );
    assert_eq!(
        dereth_client_model::inventory::burden::inq_load(
            app.objects().world.player_qualities().unwrap()
        ),
        2.0
    );
    {
        let (ui, screen) = gameplay(&mut app);
        let panel = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == dereth_ui_screens::screens::gameplay::window::INVENTORY_PAGE)
            .expect("shipped inventory page")
            .panel_id;
        screen.recv_set_panel_visibility(ui, panel, true);
    }
    frames(&mut app, 2);
    assert_eq!(gameplay(&mut app).1.inventory.burden.unwrap().1, 200);
    space(&mut app, true, 500_100);
    space(&mut app, false, 500_101);
    frames(&mut app, 1);
    assert!(
        app.last_jump_request().is_none(),
        "a load of 2 or more refuses the charge before release"
    );
    assert!(!app.objects().world.combat.jump_pending);

    // An active +100 Strength enchantment changes the load that both the inventory burden meter
    // and the next jump read. The test does not copy qualities or set driver state directly.
    let player = app.objects().player().unwrap();
    ui_event(
        &mut app,
        MagicUpdateEnchantment(Enchantment {
            id: 1,
            category_word: 1,
            power_level: 100,
            duration: 60.0,
            caster: player,
            smod: StatMod {
                kind: ench_type::ATTRIBUTE | ench_type::SINGLE_STAT | ench_type::ADDITIVE,
                key: 1,
                value: 100.0,
            },
            ..Default::default()
        }),
    );
    frames(&mut app, 2);
    assert_eq!(
        dereth_client_model::inventory::burden::inq_load(
            app.objects().world.player_qualities().unwrap()
        ),
        1.0
    );
    assert_eq!(
        gameplay(&mut app).1.inventory.burden.unwrap().1,
        100,
        "the inventory load-level consumer must use enchanted Strength too"
    );
    space(&mut app, true, 500_200);
    space(&mut app, false, 500_201);
    frames(&mut app, 1);
    assert_eq!(app.jump_counts(), (1, 0));
    assert_eq!(app.last_jump_request().unwrap().0.velocity.z, 2.619_160_2);
    frames(&mut app, 65);
    assert!(body(&app).on_ground());
    app.shutdown();
}

#[test]
fn recorded_player_qualities_drive_the_actual_release_impulse() {
    let mut app = setup();
    player_description(&mut app, "first-login-walk-jump");
    let q = app
        .objects()
        .world
        .player_qualities()
        .expect("the actual player-description qualities");
    let table = app.hud().skill_table.as_ref().expect("actual SkillTable");
    assert_eq!(
        dereth_client_model::skills::inq_skill(q, table, 0x16, false),
        Some(75)
    );
    key(&mut app, winit::keyboard::KeyCode::ShiftLeft, true, 199_999);
    space(&mut app, true, 200_000);
    frames(&mut app, 1);
    assert!(app.objects().world.combat.jump_pending);
    frames(&mut app, 17);
    space(&mut app, false, 200_600);
    frames(&mut app, 1);
    let packet = app.last_jump_request().expect("actual release packet");
    assert_eq!(packet.0.extent.to_bits(), 0.6_f32.to_bits());
    // Retail's jump-height formula at Jump skill 75, load < 1, extent 0.6, computed here with no
    // driver values or packet fields fed in; the recorded 0.5086846 jump fixes the skill at 75.
    let height = (((75.0_f32 / 1375.0) * 22.2 + 0.05) * 0.6).max(0.35);
    let expected = (height * 19.6).sqrt();
    assert_eq!(
        packet.0.velocity.z.to_bits(),
        expected.to_bits(),
        "current player skill must reach the actual impulse; default MotionEnv skill0 is wrong"
    );
    app.shutdown();
}

/// Behaviour: movement.jump-charge.a-press-charges-and-only-the-release-jumps
/// Behaviour: combat.power-bar.a-jump-raises-the-standalone-bar-and-not-the-classic-one
#[test]
fn real_space_press_charges_without_launching_and_release_jumps() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");
    let before = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    let routed = app.actions_routed();
    space(&mut app, true, 100_000);
    frames(&mut app, 1);
    assert!(
        app.actions_routed() > routed,
        "actual shipped keymap must route Space"
    );
    assert!(
        app.objects().world.combat.jump_pending,
        "the jump action must start a charge, not fire the immediate full-extent debug jump"
    );
    assert_eq!(
        app.objects().world.combat.power_bar_mode,
        PowerBarMode::Jump
    );
    assert!(app.objects().world.combat.build_in_progress);
    assert!(
        jump_bar_draws(&app) > 0,
        "producing-frame Begin reaches actual retained draw list"
    );
    assert!(app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .on_ground());
    let build_start = app.objects().world.combat.build_start_time;
    let mut pump = Pump::new();
    let repeated = pump
        .key_message_for(winit::keyboard::KeyCode::Space, true, 100_100)
        .unwrap();
    app.input_manager_mut()
        .unwrap()
        .on_message(dereth_client::pump::Win32Message {
            lparam: repeated.lparam | (1 << 30),
            ..repeated
        });
    frames(&mut app, 8);
    assert_eq!(
        app.objects().world.combat.build_start_time,
        build_start,
        "actual raw keyboard repeat must not restart charge"
    );
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.on_ground());
    assert!((c.position().frame.origin.z - before.frame.origin.z).abs() < 0.01);
    assert!(
        app.hud().panels.power_bar.bars.iter().any(|b| b.visible
            && b.mode == dereth_ui_screens::hud::powerbar::PowerBarMode::Jump
            && b.level > 0.1),
        "actual DAT jump meter must charge"
    );
    space(&mut app, false, 100_300);
    frames(&mut app, 1);
    assert!(!app.objects().world.combat.jump_pending);
    assert!(!app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .on_ground());
    let request = app
        .last_jump_request()
        .expect("actual successful release packet")
        .clone();
    assert_eq!(
        request.0.extent.to_bits(),
        0.3_f32.to_bits(),
        "nine fixed Timer intervals /1s"
    );
    assert_eq!(
        request.0.velocity,
        dereth_protocol::types::Vec3 {
            x: 0.0,
            y: 0.0,
            z: 2.619_160_2
        },
        "the minimum height 0.35 gives sqrt(0.35*19.6), not the cached achieved velocity"
    );
    assert_eq!(
        request.0.position.frame.origin.z, before.frame.origin.z,
        "packet is made before physics translates the released body"
    );
    assert_eq!(app.jump_counts(), (1, 0));
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.world.get(c.handle).unwrap().velocity_vector.z > 0.0);
    let ticks = c.stats.physics_ticks;
    space(&mut app, false, 100_301); // repeated key-up cannot submit a second release
    frames(&mut app, 3);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(
        c.stats.physics_ticks > ticks,
        "observe a real 30 Hz physics step, not a skipped gate"
    );
    assert!(c.position().frame.origin.z > before.frame.origin.z + 0.05);
    assert!(
        c.velocity().z > 0.0,
        "achieved upward arc after physics advances"
    );
    frames(&mut app, 60);
    assert!(app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .on_ground());
    assert_eq!(
        app.last_jump_request(),
        Some(&request),
        "no repeated launch on later frames"
    );
    assert_eq!(app.jump_counts(), (1, 0));
    // A combat power bar is raised first, then Space sends the jump notices to the same bar
    // subscribers from the dats. This covers the bars' display lifecycle, not a live attack.
    use dereth_ui_screens::bind::{attr, attr_float};
    let classic = app.hud().panels.combat_window.actual_power.unwrap();
    for prior in [PowerBarMode::Combat, PowerBarMode::AdvancedCombat] {
        app.objects_mut()
            .world
            .combat
            .begin_power_bar(prior, false, 0);
        app.objects_mut().world.combat.set_power_bar_level(0.625);
        frames(&mut app, 1);
        space(&mut app, true, 101_000);
        frames(&mut app, 4);
        assert_eq!(
            attr_float(&app.ui().unwrap().ui, classic, attr::METER_LEVEL),
            Some(0.0)
        );
        // Only a power bar that subscribed during initialization receives the notices: the
        // classic bar registers no subscriptions, while the floating bar registers five. So the
        // jump raises the floating bar only, and the classic bar staying down is asserted below.
        assert!(app
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .filter(|b| b.registered)
            .all(|b| b.visible && b.mode == dereth_ui_screens::hud::powerbar::PowerBarMode::Jump));
        assert!(
            app.hud()
                .panels
                .power_bar
                .bars
                .iter()
                .filter(|b| !b.registered)
                .all(|b| !b.visible),
            "the classic power-bar object hears no notice and must stay down"
        );
        assert!(jump_bar_draws(&app) > 0);
        key(&mut app, winit::keyboard::KeyCode::Escape, true, 101_200);
        space(&mut app, false, 101_201);
        frames(&mut app, 1);
        assert!(
            app.hud()
                .panels
                .power_bar
                .bars
                .iter()
                .all(|b| !b.visible && b.level == 0.0),
            "prior={prior:?} pending={} mode={:?} escape={} bars={:?}",
            app.objects().world.combat.jump_pending,
            app.objects().world.combat.power_bar_mode,
            app.interaction().stats.escape_finish_jumps,
            app.hud().panels.power_bar.bars
        );
        assert_eq!(
            jump_bar_draws(&app),
            0,
            "producing-frame Finish must remove the actual commands drawn while finishing the frame"
        );
        key(&mut app, winit::keyboard::KeyCode::Escape, false, 101_202);
    }
    assert_eq!(app.jump_counts(), (1, 0));
    app.shutdown();
}
