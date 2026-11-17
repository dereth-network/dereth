//! A polygon's plane is the client's to the last bit: the normal and the distance a terrain
//! triangle gets, which decide the height a body stands at on it, and an interior floor's plane
//! carried into the world.
//! Fixture: terrain triangles of a 48 degree, an 11 degree and a 9 degree slope, and a dungeon
//! ramp's two triangles in its room's frame, with the planes the client gives them (as bit
//! patterns).

use dereth_physics::geom::{PlaneExt, Polygon};
use dereth_physics::Plane;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

/// A plane as `[normal.x, normal.y, normal.z, d]` bit patterns.
fn bits(p: &Polygon) -> [u32; 4] {
    let (n, d) = (p.plane.normal, p.plane.d);
    [n.x.to_bits(), n.y.to_bits(), n.z.to_bits(), d.to_bits()]
}

fn triangle(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Polygon {
    let v = |p: [f32; 3]| Vec3::new(p[0], p[1], p[2]);
    Polygon::new(vec![v(a), v(b), v(c)])
}

/// The plane through a terrain triangle has the client's normal and distance exactly. On the
/// steep slope both of a cell's triangles get the same distance, -49.486305; working every step
/// of it in single precision puts them one bit either side of it. On the gentle slopes the
/// normal itself differs in its last bit when worked that way.
///
/// Behaviour: physics.polygons.a-terrain-triangles-plane-is-the-clients-to-the-last-bit
#[test]
fn a_terrain_triangles_plane_is_the_clients_to_the_last_bit() {
    let steep = [0xbf39_7652, 0xbe2b_3224, 0x3f2b_3224, 0xc245_f1fa];
    let eleven = [0xbe27_c759, 0x3da7_c758, 0x3f7b_ab05];
    let nine = [0x0000_0000, 0x3e28_5835, 0x3f7c_8450, 0xc2bd_633c];
    let cases = [
        (
            "48 degrees, first triangle",
            triangle(
                [0.0, 168.0, 116.0],
                [24.0, 168.0, 142.0],
                [24.0, 192.0, 148.0],
            ),
            steep,
        ),
        (
            "48 degrees, second triangle",
            triangle(
                [0.0, 168.0, 116.0],
                [24.0, 192.0, 148.0],
                [0.0, 192.0, 122.0],
            ),
            steep,
        ),
        (
            "11 degrees",
            triangle([72.0, 0.0, 58.0], [96.0, 0.0, 62.0], [96.0, 24.0, 60.0]),
            [eleven[0], eleven[1], eleven[2], 0xc234_e2eb],
        ),
        (
            "11 degrees, lower down",
            triangle([24.0, 24.0, 48.0], [48.0, 24.0, 52.0], [48.0, 48.0, 50.0]),
            [eleven[0], eleven[1], eleven[2], 0xc234_e2ec],
        ),
        (
            "9 degrees",
            triangle([72.0, 0.0, 96.0], [96.0, 0.0, 96.0], [72.0, 24.0, 92.0]),
            nine,
        ),
        (
            "9 degrees, the other triangle",
            triangle([96.0, 24.0, 92.0], [72.0, 24.0, 92.0], [96.0, 0.0, 96.0]),
            nine,
        ),
    ];
    for (what, p, want) in cases {
        assert_eq!(bits(&p), want, "{what}: {:?}", p.plane);
    }
}

/// A plane as `[normal.x, normal.y, normal.z, d]` from bit patterns.
fn plane(b: [u32; 4]) -> Plane {
    Plane {
        normal: Vec3::new(
            f32::from_bits(b[0]),
            f32::from_bits(b[1]),
            f32::from_bits(b[2]),
        ),
        d: f32::from_bits(b[3]),
    }
}

/// The plane of a dungeon ramp, kept in its room's own frame, is the client's in the world to the
/// last bit: turned by the room's rotation as the client keeps it and moved to the room's place.
/// Worked in single precision, the normal's smallest part and the distance each land an ulp or
/// more away.
///
/// Behaviour: physics.polygons.an-interior-floors-plane-in-the-world-is-the-clients
#[test]
fn an_interior_floors_plane_in_the_world_is_the_clients_to_the_last_bit() {
    let cell = CellId(0xD756_0100);
    let turn = f32::from_bits(0xbf35_04f7);
    let room = Position::new(
        cell,
        Frame::new(
            Vec3::new(108.0, 60.0, 40.0),
            Quat::new(turn, 0.0, 0.0, turn),
        ),
    );
    let here = Position::new(cell, Frame::default());
    let cases = [
        (
            "the ramp's first triangle",
            [0x0000_0000, 0x3ee1_d6db, 0x3f65_c00d, 0x3fd6_8c1b],
            [0xbee1_d6e4, 0xb497_963c, 0x3f65_c00d, 0x4156_a6b4],
        ),
        (
            "the ramp's second triangle",
            [0xb2c4_61c9, 0x3ee1_d6db, 0x3f65_c00e, 0x3fd6_8c1d],
            [0xbee1_d6e4, 0xb4a3_dc59, 0x3f65_c00e, 0x4156_a6b3],
        ),
    ];
    for (what, local, world) in cases {
        let got = plane(local).localtoglobal(&here, &room);
        assert_eq!(
            [
                got.normal.x.to_bits(),
                got.normal.y.to_bits(),
                got.normal.z.to_bits(),
                got.d.to_bits()
            ],
            world,
            "{what}: {got:?}"
        );
    }
}
