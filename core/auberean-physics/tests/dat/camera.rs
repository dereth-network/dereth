//! The cell the camera stands in, which chooses what a frame of the world draws. Fixture: hall
//! copy 0x0298 of the September 2013 cell file (rooms 40 m across, a room open to the sky above
//! each), and block 0x6967's land (its cave's first room, 0x0100, has a doorway out to it).

use auberean_physics::camera::{touches_outside, Camera, CameraSettings};
use auberean_physics::interior::{add_block_cells, is_interior};
use auberean_physics::land::{cell_of, Land};
use auberean_physics::mesh::Trees;
use dereth_primitives::{CellId, Frame, Position, Quat};

use crate::common::need_world;
use crate::interior::{body, sighted, with_the_player};

/// The camera stands in the cell its last line of sight ended in: behind a body turned every eighth
/// of a turn in the hall's starting room, in that room or one beyond a doorway (a line that leaves
/// a room by its open top, through no doorway, ends in the room); behind one on the land, in the
/// land cell under it.
///
/// Behaviour: draw.inside.the-frame-is-chosen-by-the-cell-the-camera-stands-in
#[test]
fn the_camera_stands_in_the_cell_its_last_line_ended_in() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    add_block_cells(&land, &w.records, &mut trees, 0x02, 0x98);
    land.add_block(&w.records, &w.region, &mut trees, 0x69, 0x67);
    add_block_cells(&land, &w.records, &mut trees, 0x69, 0x67);
    let mouth = CellId(0x6967_0100);
    let geom = land.cell(mouth).expect("the cave's first room");
    assert!(touches_outside(&geom));
    let (ground, _) = land
        .ground(
            dereth_primitives::LandblockId::new(0x69, 0x67),
            dereth_primitives::Vec3::new(30.0, 30.0, 0.0),
        )
        .expect("the land under it");
    let camera = Camera::new(CameraSettings::default());
    let (mut rooms, mut outside) = (0, 0);
    for at in [
        Position::new(
            CellId(0x0298_0109),
            Frame::new(
                dereth_primitives::Vec3::new(59.0, 240.0, -44.0),
                Quat::IDENTITY,
            ),
        ),
        Position::new(
            CellId(0x6967_000C),
            Frame::new(
                dereth_primitives::Vec3::new(30.0, 30.0, ground),
                Quat::IDENTITY,
            ),
        ),
    ] {
        let world = with_the_player(w, &land, &mut body(w, at, 0.0));
        for k in 0..8u8 {
            let heading = f32::from(k) * 45.0;
            let (look, wanted) = camera.wanted(at.frame.origin, heading);
            let seen = sighted(&world, &land, &at, look, wanted);
            let eye = seen.eye;
            if is_interior(eye.cell) {
                rooms += 1;
                assert_eq!(eye.cell.0 >> 16, at.cell.0 >> 16);
                assert!(land.cell(eye.cell).is_some(), "{heading}: {seen:?}");
            } else {
                outside += 1;
                let (cx, cy) = cell_of(eye.frame.origin).expect("on the block");
                assert_eq!(
                    eye.cell.0 & 0xFFFF,
                    u32::try_from(cx * 8 + cy + 1).expect("a land cell"),
                    "{heading}: {seen:?}"
                );
            }
        }
    }
    assert!(rooms >= 8, "{rooms} in rooms");
    assert_eq!(outside, 8, "on the land, in the land cell under it");
}

/// A line of sight from the land through a cave's mouth ends in the cave: the land cell under the
/// mouth knows the room, which its block names among the cells it sees from outside. Told nothing,
/// the same line would run on under the land.
///
/// Behaviour: detection.sight.a-line-of-sight-from-the-land-goes-into-a-cave-by-its-mouth
#[test]
fn a_line_of_sight_from_the_land_goes_into_a_cave_by_its_mouth() {
    use auberean_physics::cells::{adjust_to_outside, OUTDOORS};
    use auberean_physics::ethereal::SightLine;
    use auberean_physics::geom::Ray;
    use auberean_physics::world::World;
    use dereth_physics::math::{globaltolocal, l2g, localtoglobalvec};
    use dereth_primitives::Vec3;

    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    land.add_block(&w.records, &w.region, &mut trees, 0x69, 0x67);
    add_block_cells(&land, &w.records, &mut trees, 0x69, 0x67);
    let mouth = CellId(0x6967_0100);
    let g = land.cell(mouth).expect("the cave's first room");
    let door = g
        .portals
        .iter()
        .find(|p| p.other_cell_id == OUTDOORS)
        .expect("a doorway out");
    let n = door.portal.vertices.len();
    #[allow(clippy::cast_precision_loss)]
    let k = 1.0 / n as f32;
    let mid = door.portal.vertices.iter().fold(Vec3::ZERO, |a, v| {
        Vec3::new(a.x + v.x * k, a.y + v.y * k, a.z + v.z * k)
    });
    let middle = dereth_physics::math::localtoglobal(&g.frame, mid);
    let facing = localtoglobalvec(l2g(g.frame.rotation), door.portal.plane.normal);
    let along = |d: f32| {
        Vec3::new(
            middle.x + facing.x * d,
            middle.y + facing.y * d,
            middle.z + facing.z * d,
        )
    };
    // The side of the doorway the room's own cell tree holds is in; the other is out.
    let held = |p: Vec3| {
        g.cell_bsp
            .as_ref()
            .is_some_and(|t| t.point_inside_cell_bsp(globaltolocal(&g.frame, p)))
    };
    let sign = if held(along(1.0)) { -1.0 } else { 1.0 };
    let (from, to) = (along(sign), along(-sign));
    assert!(held(to), "ends in the room: {to:?}");
    let (mut over, mut start) = (mouth, from);
    assert!(adjust_to_outside(&mut over, &mut start));
    assert!(
        !land.touching_cells(over).is_empty(),
        "the land under the mouth knows the room"
    );
    let world = World::new();
    let mut los = SightLine::default();
    los.set_ray(Ray::segment(from, to), over.0);
    assert!(world.los_check(&land, &mut los));
    assert!(
        is_interior(CellId(los.final_cell)),
        "ends in the cave: {los:?}"
    );
}
