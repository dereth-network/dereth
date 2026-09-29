//! Vectors: local entity/protocol conversion and position serialization cases in this module
//! Conversions to shared types copy bits; a position written by dereth-protocol is
//! Position.Serialize's bytes; ids/frames convert both ways.
//! Fixture: enum values and synthetic entity records.

use dereth_protocol::types::space::{Origin, PositionWire};
use dereth_protocol::Writer;
use empyrean_entity::{Frame, LandblockId, ObjectGuid, Position, Quaternion, Vector3};

fn position() -> Position {
    // Awkward floats (a negative zero, a subnormal, a NaN payload) survive a copy, not arithmetic.
    let mut p = Position::from_components(
        0xA9B4_0019,
        84.0,
        -0.0,
        94.005,
        0.0,
        0.0,
        -0.0795,
        0.9968,
        false,
    );
    p.position_y = f32::from_bits(0x0000_0001);
    p.rotation_x = f32::from_bits(0x7FC0_1234);
    p
}

fn bits(p: &Position) -> [u32; 8] {
    [
        p.landblock_id().raw(),
        p.position_x.to_bits(),
        p.position_y.to_bits(),
        p.position_z.to_bits(),
        p.rotation_w.to_bits(),
        p.rotation_x.to_bits(),
        p.rotation_y.to_bits(),
        p.rotation_z.to_bits(),
    ]
}

#[test]
fn a_position_goes_to_the_wire_and_back_bit_for_bit() {
    let p = position();
    let wire = PositionWire::from(&p);
    assert_eq!(bits(&Position::from(wire)), bits(&p));
    let data = dereth_primitives::Position::from(&p);
    assert_eq!(bits(&Position::from(data)), bits(&p));
}

#[test]
fn a_position_written_by_dere_proto_is_what_serialize_writes() {
    let p = position();
    let mut ace = Vec::new();
    p.serialize(&mut ace, true, true);
    let mut w = Writer::body();
    PositionWire::from(&p).write(&mut w);
    assert_eq!(w.as_slice(), ace.as_slice());

    let mut ace = Vec::new();
    p.serialize(&mut ace, false, true);
    let mut w = Writer::body();
    Origin::from(&p).write(&mut w);
    assert_eq!(w.as_slice(), ace.as_slice());
}

#[test]
fn ids_and_frames_convert_both_ways() {
    let g = ObjectGuid::new(0x8000_1234);
    assert_eq!(dereth_primitives::ObjectId::from(g).0, 0x8000_1234);
    assert_eq!(
        ObjectGuid::from(dereth_primitives::ObjectId(0x5000_0001)),
        ObjectGuid::new(0x5000_0001)
    );
    assert_eq!(
        dereth_primitives::CellId::from(LandblockId::new(0x0101_0100)).0,
        0x0101_0100
    );
    assert_eq!(
        LandblockId::from(dereth_primitives::CellId(0xA9B4_0019)).raw(),
        0xA9B4_0019
    );
    let f = Frame {
        origin: Vector3::new(1.0, 2.0, 3.0),
        orientation: Quaternion::new(0.1, 0.2, 0.3, 0.9),
    };
    let w = dereth_protocol::types::space::Frame::from(f);
    assert_eq!(
        (w.orientation.w, w.orientation.x),
        (0.9, 0.1),
        "the wire orders the quaternion w first"
    );
    assert_eq!(Frame::from(w), f);
    assert_eq!(Frame::from(dereth_primitives::Frame::from(f)), f);
}
