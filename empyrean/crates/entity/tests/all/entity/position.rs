//! ACE: Source/ACE.Entity/Position.cs::Position
//! Position cell accessors, block crossings and rehoming, clamping, map coords, distances, in-
//! front-of, rotate, normalize, equality with NaN, strings, flagged serialize, starting
//! positions.
//! Fixture: enum values and synthetic entity records.

#![allow(clippy::disallowed_methods)]
// `(float)Math.PI` and friends are C# casts.
#![allow(clippy::cast_possible_truncation)]

use empyrean_entity::character_position_extensions::{invalid_position, starting_position};
use empyrean_entity::enums::PositionFlags;
use empyrean_entity::{BinaryReader, LandblockId, Position, Quaternion, Vector2, Vector3};

fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::from_components(cell, x, y, z, 0.0, 0.0, 0.0, 1.0, false)
}

fn xyz(p: &Position) -> (f32, f32, f32) {
    (p.position_x, p.position_y, p.position_z)
}

#[test]
fn new_is_cell_zero_with_identity_rotation() {
    let p = Position::new();
    assert_eq!(p.cell(), 0);
    assert_eq!(xyz(&p), (0.0, 0.0, 0.0));
    assert_eq!(p.rotation(), Quaternion::IDENTITY);
    let inv = invalid_position(0x5000_0001);
    assert_eq!(inv.cell(), 0);
    assert_eq!(inv.rotation(), Quaternion::IDENTITY);
}

#[test]
fn cell_accessors() {
    let p = at(0xA9B4_0123, 0.0, 0.0, 0.0); // indoors: stored as given
    assert_eq!(p.landblock(), 0xA9B4);
    assert_eq!(p.cell(), 0xA9B4_0123);
    assert_eq!((p.cell_x(), p.cell_y()), (0x01, 0x23));
    assert_eq!((p.landblock_x(), p.landblock_y()), (0xA9, 0xB4));
    assert_eq!(p.global_cell_x(), 0xA9 * 8 + 0x01);
    assert_eq!(p.global_cell_y(), 0xB4 * 8 + 0x23);
    assert!(p.indoors());
}

#[test]
fn crossing_east_moves_one_block_and_rehomes_the_cell() {
    // x 200 >= 192: offset (int)200 / 192 = 1, block A9 -> AA, x = 200 - 192 = 8.
    // cell: (uint)8 / 24 = 0, (uint)50 / 24 = 2 -> 0 * 8 + 2 + 1 = 3.
    let mut p = at(0xA9B4_0001, 0.0, 0.0, 0.0);
    let r = p.set_position(Vector3::new(200.0, 50.0, 5.0));
    assert_eq!(r, (true, true));
    assert_eq!(p.cell(), 0xAAB4_0003);
    assert_eq!(xyz(&p), (8.0, 50.0, 5.0));
    // No change the second time.
    assert_eq!(p.set_position(p.pos()), (false, false));
}

#[test]
fn crossing_west_uses_the_minus_one_offset() {
    // x -10: offset (int)-10 / 192 - 1 = -1, block A9 -> A8, x = -10 - (192 * -1) = 182.
    // cell: 182 / 24 = 7, 10 / 24 = 0 -> 7 * 8 + 0 + 1 = 57 = 0x39.
    let p = at(0xA9B4_0001, -10.0, 10.0, 0.0);
    assert_eq!(p.cell(), 0xA8B4_0039);
    assert_eq!(xyz(&p), (182.0, 10.0, 0.0));
}

#[test]
fn an_exact_negative_block_overshoots_and_comes_back() {
    // x -192: offset -192 / 192 - 1 = -2 (A9 -> A7), x = -192 + 384 = 192; then x >= 192:
    // offset 1 (A7 -> A8), x = 0.
    let p = at(0xA9B4_0001, -192.0, 0.0, 0.0);
    assert_eq!(p.landblock(), 0xA8B4);
    assert_eq!(p.position_x, 0.0);
    assert_eq!(p.cell(), 0xA8B4_0001);
}

#[test]
fn crossing_north_and_south() {
    let n = at(0xA9B4_0001, 0.0, 400.0, 0.0);
    assert_eq!(n.landblock(), 0xA9B6);
    assert_eq!(n.position_y, 16.0);
    let s = at(0xA9B4_0001, 0.0, -1.0, 0.0);
    assert_eq!(s.landblock(), 0xA9B3);
    assert_eq!(s.position_y, 191.0);
    // 191 / 24 = 7 -> cell 0 * 8 + 7 + 1 = 8.
    assert_eq!(s.cell(), 0xA9B3_0008);
}

#[test]
fn a_failed_transition_clamps_and_computes_cell_65() {
    // ACE-BUG: block FE cannot go east; x clamps to 192, and (uint)192 / 24 = 8 gives
    // cell 8 * 8 + 0 + 1 = 65 = 0x41.
    let p = at(0xFEB4_0001, 250.0, 0.0, 0.0);
    assert_eq!(p.position_x, 192.0);
    assert_eq!(p.cell(), 0xFEB4_0041);
    // At block row 0 going south: y clamps to 0.
    let q = at(0xA900_0001, 5.0, -5.0, 0.0);
    assert_eq!(q.landblock(), 0xA900);
    assert_eq!(q.position_y, 0.0);
}

#[test]
fn indoor_positions_are_never_rehomed() {
    let mut p = at(0xA9B4_0100, 500.0, -500.0, 0.0);
    assert_eq!(p.cell(), 0xA9B4_0100);
    assert_eq!(xyz(&p), (500.0, -500.0, 0.0));
    assert_eq!(
        p.set_position(Vector3::new(1000.0, 0.0, 0.0)),
        (false, false)
    );
}

#[test]
fn cell_zero_is_resolved_to_the_outdoor_cell() {
    // (30, 30): 30 / 24 = 1, 1 -> 1 * 8 + 1 + 1 = 10.
    let p = at(0xA9B4_0000, 30.0, 30.0, 1.0);
    assert_eq!(p.cell(), 0xA9B4_000A);
    let q = Position::from_vectors(
        0xA9B4_0000,
        Vector3::new(30.0, 30.0, 1.0),
        Quaternion::IDENTITY,
    );
    assert_eq!(q.cell(), 0xA9B4_000A);
}

#[test]
fn relative_positions_are_stored_raw_and_the_copy_constructor_rehomes_them() {
    let rel = Position::from_components(0xA9B4_0001, 300.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, true);
    assert_eq!(rel.cell(), 0xA9B4_0001);
    assert_eq!(rel.position_x, 300.0);
    // A Rust copy is a field copy.
    let field_copy = rel;
    assert_eq!(field_copy.position_x, 300.0);
    // new Position(p) goes through the Pos setter: 300 -> AA, 108; 108 / 24 = 4 -> 4 * 8 + 1 = 33.
    let copy = Position::from_position(&rel);
    assert_eq!(copy.cell(), 0xAAB4_0021);
    assert_eq!(copy.position_x, 108.0);
}

#[test]
fn map_coordinates() {
    // (0 - 0.5) * 10 = -5; (uint)(-5 + 1024) = 1019 = block 127, cell 3 -> offset 3 * 24 + 12 = 84.
    // Cell (3 << 3 | 3) + 1 = 28 = 0x1C.
    let p = Position::from_coordinates(0.0, 0.0).unwrap();
    assert_eq!(p.cell(), 0x7F7F_001C);
    assert_eq!(xyz(&p), (84.0, 84.0, 0.0));
    assert_eq!(p.rotation(), Quaternion::IDENTITY);
    // North-south drives Y, east-west X: ns 10 -> 95 + 1024 = 1119 = block 139 (0x8B), cell 7.
    let q = Position::from_coordinates(10.0, 0.0).unwrap();
    // cellX 3, cellY 7: (3 << 3 | 7) + 1 = 32 = 0x20.
    assert_eq!(q.cell(), 0x7F8B_0020);
    assert_eq!(q.position_y, 7.0 * 24.0 + 12.0);
    // East-west 1: 5 + 1024 = 1029 = block 128 (0x80), cell 5 -> x 5 * 24 + 12 = 132;
    // cell (5 << 3 | 3) + 1 = 44 = 0x2C.
    let r = Position::from_coordinates(0.0, 1.0).unwrap();
    assert_eq!(r.cell(), 0x807F_002C);
    assert_eq!(xyz(&r), (132.0, 84.0, 0.0));
    // Off the map: (200 - 0.5) * 10 + 1024 = 3019 >= 0x7F8.
    assert!(Position::from_coordinates(200.0, 0.0).is_err());
    // East-west: (103 - 0.5) * 10 + 1024 = 2049 >= 0x7F8.
    assert!(Position::from_coordinates(0.0, 103.0).is_err());
}

#[test]
fn map_coordinates_from_a_vector() {
    // (0, 0) + 101.95 = 101.95f; * 240 rounds to 24468.0; 24468 / 192 = 127, 24468 % 192 = 84;
    // 84 / 24 = 3 -> cell 3 * 8 + 3 + 1 = 28. The same cell as the float constructor.
    let p = Position::from_map_coordinates(Vector2::new(0.0, 0.0));
    assert_eq!(p.cell(), 0x7F7F_001C);
    assert_eq!(xyz(&p), (84.0, 84.0, 0.0));
}

#[test]
fn distances_within_and_across_landblocks() {
    let a = at(0xA9B4_0001, 10.0, 10.0, 0.0);
    let b = at(0xA9B4_0009, 13.0, 14.0, 12.0); // same landblock, different cell
    assert_eq!(a.distance_2d(&b), 5.0);
    assert_eq!(a.distance_2d_squared(&b), 25.0);
    assert_eq!(a.squared_distance_to(&b), 169.0);
    assert_eq!(a.distance_to(&b), 13.0);

    // Across: dx = (A9 - AA) * 192 + 10 - 5 = -187.
    let c = at(0xAAB4_0001, 5.0, 10.0, 0.0);
    assert_eq!(a.distance_to(&c), 187.0);
    assert_eq!(a.distance_2d_squared(&c), 187.0 * 187.0);
    assert_eq!(a.get_offset(&c), Vector3::new(187.0, 0.0, 0.0));
    assert_eq!(c.get_offset(&a), Vector3::new(-187.0, 0.0, 0.0));

    // Null.
    assert_eq!(a.distance_to(None), f32::MAX);
    assert_eq!(a.squared_distance_to(None), f32::MAX);
    assert_eq!(a.distance_2d(None), f32::MAX);
    assert_eq!(a.distance_2d_squared(None), f32::MAX);
    assert_eq!(
        a.get_offset(None),
        Vector3::new(f32::MAX, f32::MAX, f32::MAX)
    );
}

#[test]
fn in_front_of() {
    // Identity: heading atan2(0, 1) = 0 -> dx = -(sin 0 * 10) = -0, dy = 10; z + 0.05.
    let a = at(0xA9B4_0001, 10.0, 10.0, 0.0);
    let f = a.in_front_of(10.0, false);
    assert_eq!(xyz(&f), (10.0, 20.0, 0.05));
    assert_eq!(f.cell(), 0xA9B4_0001);
    assert_eq!(f.rotation(), Quaternion::IDENTITY);

    // Facing south (z 1, w 0): heading atan2(0, -1) = pi -> dy = -10, dx is -(1.2e-15) and
    // vanishes against 10.
    let mut s = a;
    s.set_rotation(Quaternion::new(0.0, 0.0, 1.0, 0.0));
    let g = s.in_front_of(10.0, false);
    assert_eq!(xyz(&g), (10.0, 0.0, 0.05));
    assert_eq!((g.rotation_z, g.rotation_w), (1.0, 0.0));

    // rotate180 of identity: identity * CreateFromYawPitchRoll(0, 0, (float)PI) = (0, 0, sin, cos)
    // of (float)PI * 0.5; x and y are forced to 0.
    let h = a.in_front_of(10.0, true);
    let half = (std::f64::consts::PI as f32) * 0.5;
    assert_eq!((h.rotation_x, h.rotation_y), (0.0, 0.0));
    assert_eq!(h.rotation_z, half.sin());
    assert_eq!(h.rotation_w, half.cos());
    assert_eq!(h.rotation_z, 1.0);
}

#[test]
fn rotate_faces_the_direction() {
    for dir in [
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, -1.0, 0.0),
        Vector3::new(-3.0, 4.0, 0.0),
    ] {
        let mut p = Position::new();
        p.rotate(dir);
        assert_eq!((p.rotation_x, p.rotation_y), (0.0, 0.0));
        let got = p.get_current_dir();
        let want = Vector3::normalize(dir);
        assert!((got - want).length() < 1e-6, "{dir:?}: {got:?}");
    }
    // Facing +X is roll -pi/2: z = sin(-pi/4), w = cos(-pi/4).
    let mut p = Position::new();
    p.rotate(Vector3::new(1.0, 0.0, 0.0));
    let half = (-std::f64::consts::FRAC_PI_2) as f32 * 0.5;
    assert_eq!(p.rotation_z, half.sin());
    assert_eq!(p.rotation_w, half.cos());
}

#[test]
fn normalize_multiplies_by_the_reciprocal() {
    let p = Position::new();
    let v = Vector3::new(3.0, 0.0, 4.0);
    assert_eq!(p.normalize(v), Vector3::new(3.0 * 0.2, 0.0, 4.0 * 0.2));
    assert_eq!(
        Vector3::normalize(v),
        Vector3::new(3.0 / 5.0, 0.0, 4.0 / 5.0)
    );
}

#[test]
fn equals_compares_cell_origin_and_rotation_with_nan_equal() {
    let a = at(0xA9B4_0001, 10.0, 10.0, 0.0);
    let mut b = a;
    assert!(a.equals(&b));
    b.position_z = 0.5;
    assert!(!a.equals(&b));
    // Same landblock, other cell: not equal (Cell is the full id).
    let mut c = a;
    c.set_landblock_id(LandblockId::new(0xA9B4_0002));
    assert!(!a.equals(&c));
    // NaN equals NaN under Equals.
    let mut n1 = a;
    n1.position_x = f32::NAN;
    let n2 = n1;
    assert!(n1.equals(&n2));
    assert!(!a.equals(None));
}

#[test]
fn to_string_and_loc_string() {
    let mut p = at(0xA9B4_0001, 10.0, 10.5, 0.0);
    p.set_rotation(Quaternion::new(0.0, 0.0, 0.25, 0.75));
    assert_eq!(p.to_string(), "A9B40001 [10 10.5 0]");
    assert_eq!(
        p.to_loc_string(),
        "0xA9B40001 [10.000000 10.500000 0.000000] 0.750000 0.000000 0.000000 0.250000"
    );
}

#[test]
fn serialize_with_flags_skips_the_flagged_components() {
    let mut p = at(0xA9B4_0001, 1.0, 2.0, 3.0);
    p.set_rotation(Quaternion::new(0.1, 0.2, 0.3, 0.4));
    let flags = PositionFlags::HasPlacementID
        | PositionFlags::OrientationHasNoX
        | PositionFlags::OrientationHasNoY;
    let mut buf = Vec::new();
    p.serialize_with_flags(&mut buf, flags, 7, true);
    let mut want = Vec::new();
    for w in [0x32u32, 0xA9B4_0001] {
        want.extend_from_slice(&w.to_le_bytes());
    }
    for f in [1.0f32, 2.0, 3.0, 0.4, 0.3] {
        want.extend_from_slice(&f.to_le_bytes());
    }
    want.extend_from_slice(&7i32.to_le_bytes());
    assert_eq!(buf, want);

    // HasVelocity appends three zero floats; no landblock when asked not to.
    let mut v = Vec::new();
    p.serialize_with_flags(&mut v, PositionFlags::HasVelocity, 0, false);
    assert_eq!(v.len(), 4 + 12 + 16 + 12);
    assert_eq!(&v[4..8], &1.0f32.to_le_bytes());
}

#[test]
fn serialize_and_read_back() {
    let mut p = at(0xA9B4_0100, 1.5, 2.5, 3.5);
    p.set_rotation(Quaternion::new(0.1, 0.2, 0.3, 0.4));
    let mut buf = Vec::new();
    p.serialize(&mut buf, true, true);
    assert_eq!(buf.len(), 32);
    // Wire order is w, x, y, z.
    assert_eq!(&buf[16..20], &0.4f32.to_le_bytes());
    let q = Position::from_reader(&mut BinaryReader::new(&buf)).unwrap();
    assert!(q.equals(&p));
    assert!(Position::from_reader(&mut BinaryReader::new(&buf[..31])).is_none());

    let mut short = Vec::new();
    p.serialize(&mut short, false, false);
    assert_eq!(short.len(), 12);
}

#[test]
fn starting_positions() {
    // Every starting cell is an indoor cell (0x01AD), so the stored values are ACE's literals.
    let cases = [
        (0u32, 0x8603_01ADu32),
        (1, 0x7F03_01AD),
        (2, 0x8C04_01AD),
        (3, 0x7203_01AD),
        (99, 0x8603_01AD),
    ];
    for (area, cell) in cases {
        let p = starting_position(area);
        assert_eq!(p.cell(), cell, "area {area}");
        assert_eq!(xyz(&p), (12.3199, -28.482, 0.004_999_999_5));
        assert_eq!(
            p.rotation(),
            Quaternion::new(0.0, 0.0, -0.940_805_9, -0.338_945_9)
        );
    }
}
