//! A position on the wire: one whose frame is not a frame does not read.

use auberean_primitives::{Frame, Quat, Vec3};
use auberean_protocol::quality::{PrivateQuality, VisualQuality};
use auberean_protocol::types::{ObjectStamp, Placement};
use auberean_protocol::ServerMessage;

fn at(origin: Vec3, rotation: Quat) -> Placement {
    Placement {
        cell: 0x7A69_0012,
        frame: Frame { origin, rotation },
    }
}

fn turned(w: f32, z: f32) -> Quat {
    Quat {
        w,
        x: 0.0,
        y: 0.0,
        z,
    }
}

fn private(p: Placement) -> Vec<u8> {
    ServerMessage::UpdatePlacement(PrivateQuality {
        quality: 0x5000_0001,
        value: p,
    })
    .encode()
}

fn visual(p: Placement) -> Vec<u8> {
    ServerMessage::UpdatePositionVisual(VisualQuality {
        object: ObjectStamp {
            id: 0x2000_0000_0000_0001,
            instance_stamp: 1,
            other_stamp: 2,
        },
        quality: 0x5000_0001,
        value: p,
    })
    .encode()
}

/// A position whose rotation is a unit to within 0.0002 of its square length reads; one further
/// off, one with no rotation at all, and one with a part that is not a number make the message
/// holding it not read, the player's own position and another object's alike.
///
/// Behaviour: protocol.position.a-frame-that-is-not-one-does-not-read
#[test]
fn a_position_whose_frame_is_not_a_frame_leaves_its_message_unread() {
    let origin = Vec3::new(10.0, 20.0, 5.0);
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let good = [
        Quat::IDENTITY,
        turned(half, half),
        // Square length 1.00019, inside the margin.
        turned(1.000_095, 0.0),
    ];
    let bad = [
        // Square length 1.00021, outside it.
        turned(1.000_105, 0.0),
        turned(0.0, 0.0),
        turned(0.707, 0.707),
        turned(f32::NAN, 0.0),
    ];
    for q in good {
        for bytes in [private(at(origin, q)), visual(at(origin, q))] {
            assert!(
                ServerMessage::decode_fields(&bytes).is_ok(),
                "a unit rotation {q:?} reads"
            );
        }
    }
    for q in bad {
        for bytes in [private(at(origin, q)), visual(at(origin, q))] {
            assert!(
                ServerMessage::decode_fields(&bytes).is_err(),
                "the rotation {q:?} does not read"
            );
        }
    }
    let lost = Vec3::new(10.0, f32::NAN, 5.0);
    assert!(ServerMessage::decode_fields(&private(at(lost, Quat::IDENTITY))).is_err());
}
