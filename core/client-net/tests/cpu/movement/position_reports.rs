//! Contracts for position reports.
//! Fixture: shared recorded messages and synthetic state.

use super::common::*;
use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use dereth_client_net::client_session::{ContactPlane, PlayerMotion, Session};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{MovementAutonomousPosition, MovementMoveToState};
use dereth_protocol::types::{Frame, PositionWire, Quat, Vec3};
use dereth_protocol::{Message, Opcode};

const MOVE_TO_STATE: u32 = 0xF61C;
const AUTONOMOUS_POSITION: u32 = 0xF753;
const GAME_ACTION: u32 = 0xF7B1;

struct Recorded {
    scenario: &'static str,
    sub_type: u32,
    payload: Vec<u8>,
}

fn recorded_position_actions() -> Vec<Recorded> {
    let mut out = Vec::new();
    for corpus in Corpus::shared_all() {
        let name = corpus.name.as_str();
        let mut blobs: Vec<_> = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
            .collect();
        blobs.sort_by_key(|b| b.blob_id);
        for b in blobs {
            let action = unpack_action(&b.payload).expect("a captured game action must unpack");
            let sub = action.sub_type.0;
            if sub == MOVE_TO_STATE || sub == AUTONOMOUS_POSITION {
                out.push(Recorded {
                    scenario: name,
                    sub_type: sub,
                    payload: b.payload.clone(),
                });
            }
        }
    }
    out
}

fn decode<M: Message>(payload: &[u8]) -> M {
    let mut action = unpack_action(payload).expect("a captured game action must unpack");
    let m = M::read(&mut action.body).expect("a captured body must decode");
    action
        .body
        .expect_exhausted()
        .expect("a captured body must be fully consumed");
    m
}

fn motion_from_autonomous(m: &MovementAutonomousPosition) -> PlayerMotion {
    PlayerMotion {
        position: m.0.position,
        position_valid: true,
        timestamps: m.0.timestamps,
        contact: m.0.contact != 0,
        longjump_mode: false,
        raw_motion_state: dereth_protocol::movement::RawMotionState::default(),
        contact_plane: ContactPlane::default(),
    }
}

fn motion_from_move_to_state(m: &MovementMoveToState) -> PlayerMotion {
    PlayerMotion {
        position: m.0.position,
        position_valid: true,
        timestamps: m.0.timestamps,
        contact: m.0.contact,
        longjump_mode: m.0.longjump_mode,
        raw_motion_state: m.0.raw_motion_state.clone(),
        contact_plane: ContactPlane::default(),
    }
}

/// Behaviour: movement.position-report.the-state-the-client-reports-is-the-bodys-own
#[test]
fn every_recorded_position_body_re_encodes_byte_for_byte() {
    let recorded = recorded_position_actions();
    let mut move_to_state = 0usize;
    let mut autonomous = 0usize;
    let mut exact = 0usize;
    let mut contact_set = 0usize;

    for r in &recorded {
        let (mut rep, mut session) = reporter();
        let produced = match r.sub_type {
            MOVE_TO_STATE => {
                let m: MovementMoveToState = decode(&r.payload);
                let motion = motion_from_move_to_state(&m);
                move_to_state += 1;
                rep.send_movement_event(0.0, &motion, &mut session)
                    .expect("the producer must encode every recorded MoveToState");
                session.transport.sent.clone()
            }
            AUTONOMOUS_POSITION => {
                let m: MovementAutonomousPosition = decode(&r.payload);
                if m.0.contact != 0 {
                    contact_set += 1;
                }
                let motion = motion_from_autonomous(&m);
                autonomous += 1;
                rep.send_position_event(0.0, &motion, &mut session)
                    .expect("the producer must encode every recorded AutonomousPosition");
                session.transport.sent.clone()
            }
            other => panic!("unexpected sub-type {other:#06X}"),
        };
        assert_eq!(produced.len(), 1, "one blob per send, in {}", r.scenario);
        let got = &produced[0].payload;
        assert_eq!(
            &got[8..],
            &r.payload[8..],
            "{}: the producer's {:#06X} body differs from the recording\n  produced {:02X?}\n  \
             recorded {:02X?}",
            r.scenario,
            r.sub_type,
            &got[8..],
            &r.payload[8..]
        );
        exact += 1;
    }

    assert!(move_to_state > 0 && autonomous > 0);
    assert_eq!(
        move_to_state,
        recorded
            .iter()
            .filter(|r| r.sub_type == MOVE_TO_STATE)
            .count()
    );
    assert_eq!(
        autonomous,
        recorded
            .iter()
            .filter(|r| r.sub_type == AUTONOMOUS_POSITION)
            .count()
    );
    assert_eq!(exact, recorded.len());
    assert_eq!(
        contact_set, autonomous,
        "every autonomous report is in contact"
    );
}

fn at(cell: u32, x: f32, y: f32, z: f32) -> PositionWire {
    PositionWire {
        objcell_id: cell,
        frame: Frame {
            origin: Vec3 { x, y, z },
            orientation: Quat::default(),
        },
    }
}

fn standing(pos: PositionWire) -> PlayerMotion {
    PlayerMotion {
        position: pos,
        position_valid: true,
        timestamps: dereth_protocol::movement::MoveTimestamps::default(),
        contact: true,
        longjump_mode: false,
        raw_motion_state: dereth_protocol::movement::RawMotionState::default(),
        contact_plane: ContactPlane {
            normal: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            d: 0.0,
        },
    }
}

fn sub_types(session: &Session<MockTransport>) -> Vec<u32> {
    session
        .transport
        .sent
        .iter()
        .map(|b| u32::from_le_bytes([b.payload[8], b.payload[9], b.payload[10], b.payload[11]]))
        .collect()
}

/// Behaviour: movement.position-report.a-walking-body-reports-where-it-is-and-a-still-one-falls-silent
#[test]
fn the_schedule_is_one_second_strictly_and_only_when_the_position_moved() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "the first frame reports where the player is"
    );

    for (i, t) in [0.1_f64, 0.5, 0.999, 1.0].into_iter().enumerate() {
        m.position.frame.origin.x = 10.0 + (i as f32 + 1.0) * 0.25;
        rep.use_time(t, &m, &mut session);
        assert_eq!(
            rep.stats.position_events, 1,
            "t={t}: 1.0 s has not strictly elapsed, so nothing may go out"
        );
    }
    m.position.frame.origin.x = 20.0;
    rep.use_time(1.0001, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 2,
        "the first sample past 1.0 s reports"
    );

    m.position.frame.origin.x = 30.0;
    rep.use_time(1.9, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 2,
        "the interval is measured from the last send"
    );
    rep.use_time(2.0002, &m, &mut session);
    assert_eq!(rep.stats.position_events, 3);

    let before = rep.stats.position_events;
    for t in [3.1_f64, 4.2, 10.0, 60.0, 600.0] {
        rep.use_time(t, &m, &mut session);
    }
    assert_eq!(
        rep.stats.position_events, before,
        "a still player reports nothing at all"
    );

    let subs = sub_types(&session);
    assert_eq!(subs.iter().filter(|s| **s == MOVE_TO_STATE).count(), 1);
    assert_eq!(
        subs.iter().filter(|s| **s == AUTONOMOUS_POSITION).count(),
        usize::try_from(rep.stats.position_events).expect("a frame count fits a usize")
    );
}

#[test]
fn a_cell_change_reports_immediately_without_waiting_for_the_interval() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(rep.stats.position_events, 1);

    m.position.frame.origin.x = 12.0;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "control: an in-cell move waits for the interval"
    );

    m.position.objcell_id = 0x00A9_0126;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(rep.stats.position_events, 2, "a cell change does not wait");
    assert_eq!(rep.last_sent_position().objcell_id, 0x00A9_0126);

    m.position.objcell_id = 0x00A9_0100;
    rep.use_time(0.21, &m, &mut session);
    assert_eq!(rep.stats.position_events, 3);
}

#[test]
fn a_contact_plane_change_reports_immediately_and_a_smaller_one_does_not() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(rep.stats.position_events, 1);

    rep.use_time(0.1, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "control: an unchanged plane sends nothing"
    );

    m.contact_plane.d = 0.0001;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "0.0001 is inside the 0.0002 epsilon"
    );

    m.contact_plane.normal = Vec3 {
        x: 0.0,
        y: 0.30,
        z: 0.95,
    };
    rep.use_time(0.3, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 2,
        "a new contact plane does not wait for the interval"
    );

    rep.use_time(0.4, &m, &mut session);
    assert_eq!(rep.stats.position_events, 2);

    m.contact_plane.d = 0.5;
    rep.use_time(0.5, &m, &mut session);
    assert_eq!(rep.stats.position_events, 3);
}

#[test]
fn the_sender_refuses_a_body_that_is_not_on_the_ground_or_has_no_valid_position() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    m.contact = false;
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(rep.stats.position_events, 0);
    assert_eq!(rep.stats.position_events_gated, 1);

    m.contact = true;
    m.position_valid = false;
    rep.use_time(0.1, &m, &mut session);
    assert_eq!(rep.stats.position_events, 0);
    assert_eq!(rep.stats.position_events_gated, 2);

    m.position_valid = true;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "with both satisfied the same frame reports"
    );
}

#[test]
fn a_body_the_server_is_driving_does_not_report_its_own_position() {
    let (mut rep, mut session) = reporter();
    let m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    assert_eq!(rep.autonomy_level, 2, "the constructor's value");

    rep.active = false;
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(rep.stats.position_events, 0, "IsActive() is the first gate");

    rep.active = true;
    for level in [0u32, 1] {
        rep.autonomy_level = level;
        rep.use_time(0.1, &m, &mut session);
        assert_eq!(
            rep.stats.position_events, 0,
            "autonomy level {level} is the server's"
        );
    }
    rep.autonomy_level = 2;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(rep.stats.position_events, 1);
}

#[test]
fn a_move_to_state_resets_the_schedule_but_not_the_position_it_compares_against() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    rep.use_time(0.0, &m, &mut session);
    assert_eq!(
        (rep.stats.movement_events, rep.stats.position_events),
        (1, 1)
    );
    assert_eq!(
        sub_types(&session),
        vec![MOVE_TO_STATE, AUTONOMOUS_POSITION]
    );
    let after_first = rep.last_sent_position().frame.origin.x;

    m.position.frame.origin.x = 12.0;
    m.raw_motion_state.forward_command = Some(0x4500_0005);
    rep.use_time(0.5, &m, &mut session);
    assert_eq!(
        rep.stats.movement_events, 2,
        "a changed motion state sends a MoveToState"
    );
    assert_eq!(
        rep.stats.position_events, 1,
        "and no autonomous position, the interval is unmet"
    );
    assert_eq!(
        rep.last_sent_position().frame.origin.x,
        after_first,
        "SendMovementEvent must not touch last_sent_position"
    );

    m.position.frame.origin.x = 14.0;
    rep.use_time(1.02, &m, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "the MoveToState restarted the interval"
    );

    rep.use_time(1.51, &m, &mut session);
    assert_eq!(rep.stats.position_events, 2);
    assert_eq!(rep.last_sent_position().frame.origin.x, 14.0);

    let before = rep.stats.movement_events;
    rep.use_time(1.6, &m, &mut session);
    rep.use_time(1.7, &m, &mut session);
    assert_eq!(rep.stats.movement_events, before);
}

#[test]
fn pressing_and_releasing_are_two_separate_move_to_states() {
    let (mut rep, mut session) = reporter();
    let mut m = standing(at(0x00A9_0125, 10.0, 10.0, 0.0));
    rep.use_time(0.0, &m, &mut session);
    let idle = rep.stats.movement_events;

    m.raw_motion_state.forward_command = Some(0x4500_0005); // WalkForward
    m.raw_motion_state.current_holdkey = Some(2); // the run hold key
    rep.use_time(0.1, &m, &mut session);
    assert_eq!(rep.stats.movement_events, idle + 1, "press");

    m.raw_motion_state.forward_command = None;
    m.raw_motion_state.current_holdkey = None;
    rep.use_time(0.2, &m, &mut session);
    assert_eq!(rep.stats.movement_events, idle + 2, "release");

    let subs = sub_types(&session);
    let idle = usize::try_from(idle).expect("a blob count fits a usize");
    assert_eq!(
        subs.iter().filter(|s| **s == MOVE_TO_STATE).count(),
        idle + 2
    );
}

#[test]
fn the_two_sub_opcodes_are_the_numbers_the_client_writes() {
    assert_eq!(<MovementMoveToState as Message>::OPCODE, Opcode(0xF61C));
    assert_eq!(
        <MovementAutonomousPosition as Message>::OPCODE,
        Opcode(0xF753)
    );

    let (mut rep, mut session) = reporter();
    let m = standing(at(0x00A9_0125, 1.0, 2.0, 3.0));
    rep.use_time(0.0, &m, &mut session);
    let sent = &session.transport.sent;
    assert_eq!(&sent[0].payload[8..12], &[0x1C, 0xF6, 0x00, 0x00]);
    assert_eq!(&sent[1].payload[8..12], &[0x53, 0xF7, 0x00, 0x00]);
    assert!(sent.iter().all(|b| b.ordered));
}
