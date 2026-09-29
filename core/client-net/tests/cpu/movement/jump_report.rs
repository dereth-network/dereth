//! Contracts for jump report.
//! Fixture: shared recorded messages and synthetic state.

use super::common::*;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::{
    ContactPlane, PlayerMotion, PositionReporter, MIN_JUMP_EXTENT,
};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{MovementJump, RawMotionState};
use dereth_protocol::types::Vec3;
use dereth_protocol::Message;

const JUMP: u32 = 0xF61B;
const GAME_ACTION: u32 = 0xF7B1;

struct Recorded {
    scenario: &'static str,
    payload: Vec<u8>,
}

fn recorded_jumps() -> Vec<Recorded> {
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
            if action.sub_type.0 == JUMP {
                out.push(Recorded {
                    scenario: name,
                    payload: b.payload.clone(),
                });
            }
        }
    }
    out
}

fn decode(payload: &[u8]) -> MovementJump {
    let mut action = unpack_action(payload).expect("a captured game action must unpack");
    let m = MovementJump::read(&mut action.body).expect("a captured body must decode");
    action
        .body
        .expect_exhausted()
        .expect("a captured body must be fully consumed");
    m
}

fn motion_from_jump(m: &MovementJump) -> PlayerMotion {
    PlayerMotion {
        position: m.0.position,
        position_valid: true,
        timestamps: m.0.timestamps,
        contact: false,
        longjump_mode: false,
        raw_motion_state: RawMotionState::default(),
        contact_plane: ContactPlane::default(),
    }
}

/// Behaviour: movement.jump.a-jump-puts-the-bodys-own-position-and-speed-on-the-wire
#[test]
fn every_recorded_jump_body_re_encodes_byte_for_byte() {
    let recorded = recorded_jumps();
    let mut exact = 0usize;

    for r in &recorded {
        let m = decode(&r.payload);
        let motion = motion_from_jump(&m);
        let (mut rep, mut session) = reporter();
        rep.send_jump(true, m.0.extent, m.0.velocity, &motion, &mut session)
            .expect("the producer must encode every recorded jump");
        let produced = &session.transport.sent;
        assert_eq!(produced.len(), 1, "one blob per jump, in {}", r.scenario);
        assert_eq!(
            &produced[0].payload[8..],
            &r.payload[8..],
            "{}: the producer's 0xF61B body differs from the recording\n  produced {:02X?}\n  \
             recorded {:02X?}",
            r.scenario,
            &produced[0].payload[8..],
            &r.payload[8..]
        );
        assert_eq!(rep.stats.jump_events, 1);
        exact += 1;
    }

    assert!(!recorded.is_empty());
    assert_eq!(exact, recorded.len(), "every recorded jump is reproduced");
}

#[test]
fn standing_and_running_reference_jumps_decode_their_literal_fields() {
    let all = recorded_jumps();
    let recorded = ["first-login-walk-jump", "long-solo-play"].map(|name| {
        all.iter()
            .find(|r| r.scenario == name)
            .expect("reference jump")
    });
    let bodies: Vec<MovementJump> = recorded.iter().map(|r| decode(&r.payload)).collect();

    let extents: Vec<f32> = bodies.iter().map(|b| b.0.extent).collect();
    assert_ne!(
        extents[0], extents[1],
        "the two extents differ, so the field is exercised"
    );
    for e in &extents {
        assert!(
            *e > MIN_JUMP_EXTENT * 100.0,
            "both recorded extents are far above MIN_JUMP_EXTENT ({e}), so the floor is NOT \
             exercised by the corpus and rests on retail's constant"
        );
    }

    let vels: Vec<Vec3> = bodies.iter().map(|b| b.0.velocity).collect();
    assert_eq!(
        (vels[0].x, vels[0].y),
        (0.0, 0.0),
        "first-login-walk-jump's is a standing jump: no horizontal velocity"
    );
    assert!(
        vels[1].y.abs() > 1.0,
        "long-solo-play's is a running jump, so the horizontal half of `velocity` is exercised: {:?}",
        vels[1]
    );
    for v in &vels {
        assert!(v.z > 1.0, "both carry a real upward velocity: {v:?}");
    }

    assert_eq!(bodies[0].0.position.objcell_id, 0xA9B4_002A);
    assert_eq!(bodies[1].0.position.objcell_id, 0x7F03_02C3);
    assert_ne!(
        bodies[0].0.position.frame.orientation.z, bodies[1].0.position.frame.orientation.z,
        "two distinct headings"
    );

    for b in &bodies {
        let q = b.0.position.frame.orientation;
        assert_eq!(
            (q.x, q.y),
            (0.0, 0.0),
            "the recording pins neither quaternion x nor y: a player's frame is a rotation about \
             z alone, so those two floats rest on `Frame`'s layout, which the 1,827 recorded \
             position bodies share"
        );
    }

    let ts: Vec<_> = bodies.iter().map(|b| b.0.timestamps).collect();
    assert_eq!(
        (ts[0].instance, ts[1].instance),
        (90, 2),
        "instance is exercised and differs"
    );
    assert_eq!(
        (ts[0].server_control, ts[1].server_control),
        (0, 323),
        "server_control is exercised: 0 in one and non-zero in the other, which is what makes a \
         slot swapped with `instance` fail"
    );
    assert_eq!(
        (ts[0].teleport, ts[1].teleport),
        (0, 2),
        "teleport is exercised"
    );

    for t in &ts {
        assert_eq!(
            t.force_position, 0,
            "force_position is 0 in both recorded jumps, as it is in all 1,827 recorded position \
             bodies, because nothing in the corpus's 8,617 server blobs ever advances \
             the force-position timestamp. That slot's correctness rests on 's retail station, not \
             on this oracle."
        );
    }

    for r in recorded {
        assert_eq!(
            r.payload.len(),
            12 + 56,
            "a 0xF61B blob is the 12-byte header plus a 56-byte pack, so `JumpPack`'s align4 is \
             structurally a no-op and no recording can exercise it"
        );
    }
}

#[test]
fn a_jump_does_not_disturb_the_position_schedule() {
    let (mut rep, mut session) = reporter();
    let m = PlayerMotion {
        position: dereth_protocol::types::PositionWire {
            objcell_id: 0x00A9_0125,
            frame: dereth_protocol::types::Frame {
                origin: Vec3 {
                    x: 10.0,
                    y: 10.0,
                    z: 0.0,
                },
                orientation: dereth_protocol::types::Quat::default(),
            },
        },
        position_valid: true,
        contact: true,
        contact_plane: ContactPlane {
            normal: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            d: 0.0,
        },
        ..PlayerMotion::default()
    };
    rep.use_time(10.0, &m, &mut session);
    assert_eq!(rep.stats.position_events, 1);
    let sent_at_first = *rep.last_sent_position();

    let mut jumping = m.clone();
    jumping.position.frame.origin.x = 12.5;
    let stamp = rep
        .send_jump(
            true,
            1.0,
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 3.5,
            },
            &jumping,
            &mut session,
        )
        .expect("the jump encodes");
    assert!(
        stamp > 0,
        "the jump takes an action-order header stamp like every other game action"
    );
    assert_eq!(rep.stats.jump_events, 1);
    assert_eq!(
        *rep.last_sent_position(),
        sent_at_first,
        "a jump must not refresh `last_sent_position`: the schedule compares against the last \
         POSITION report, and folding the jump into it would silence the next one"
    );

    let mut moved = m;
    moved.position.frame.origin.x = 15.0;
    rep.use_time(10.6, &moved, &mut session);
    assert_eq!(
        rep.stats.position_events, 1,
        "a jump must not reset `last_sent_position_time` and so must not trigger a report"
    );

    moved.position.frame.origin.x = 20.0;
    rep.use_time(11.0001, &moved, &mut session);
    assert_eq!(
        rep.stats.position_events, 2,
        "the 1.0 s schedule still runs from the last position report, not from the jump"
    );
}

#[test]
fn a_refused_jump_sends_nothing() {
    let (mut rep, mut session) = reporter();
    let m = PlayerMotion {
        position_valid: true,
        ..PlayerMotion::default()
    };
    assert!(rep
        .send_jump(false, 1.0, Vec3::default(), &m, &mut session)
        .is_none());
    assert_eq!(rep.stats.jump_events, 0);
    assert!(
        session.transport.sent.is_empty(),
        "a refused jump puts nothing on the wire"
    );
}

#[test]
fn the_extent_is_floored_at_the_constant_the_binary_holds() {
    assert_eq!(
        MIN_JUMP_EXTENT.to_bits(),
        u32::from_le_bytes([0x6F, 0x12, 0x83, 0x3A]),
        "the client's own jump-velocity bytes"
    );
    let m = PlayerMotion::default();
    assert_eq!(
        PositionReporter::jump_pack(0.0, Vec3::default(), &m).extent,
        MIN_JUMP_EXTENT
    );
    assert_eq!(
        PositionReporter::jump_pack(0.000_5, Vec3::default(), &m).extent,
        MIN_JUMP_EXTENT
    );
    assert_eq!(
        PositionReporter::jump_pack(MIN_JUMP_EXTENT, Vec3::default(), &m).extent,
        MIN_JUMP_EXTENT
    );
    assert_eq!(
        PositionReporter::jump_pack(0.5, Vec3::default(), &m).extent,
        0.5
    );
    assert_eq!(
        PositionReporter::jump_pack(1.0, Vec3::default(), &m).extent,
        1.0
    );
}
