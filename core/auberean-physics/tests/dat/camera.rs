//! The cell the camera stands in, which chooses what a frame of the world draws, and the cell a
//! frame is drawn from with the camera drawn between its places. Fixture: hall copy 0x0298 of the
//! September 2013 cell file (rooms 40 m across, a room open to the sky above each), and block
//! 0x6967's land (its cave's first room, 0x0100, has a doorway out to it).

use auberean_physics::camera::{drawn_from, touches_outside, Camera, CameraSettings};
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

/// A player put on the bed of deep water rises and floats with its water line at the surface, and
/// the camera looking up from under it stands where it wants, far above the land: no line of sight
/// reaches the bed. Fixture: block 0x0298's land, flat at height 0 under 70.95 m of water, and the
/// player's own motion values.
///
/// Behaviour: motion.element.a-player-on-a-deep-bed-floats-up-and-the-camera-with-it
#[test]
fn a_player_on_a_deep_bed_floats_up_and_the_camera_looks_up_from_under_it() {
    use auberean_assets::animation::{MotionDesc, MotionParams};
    use auberean_physics::body::{Body, BodyConfig};
    use auberean_physics::camera::{sight_lines, Lines};
    use auberean_physics::motion::{EARTH, PEACE, WATER};
    use dereth_physics::geom::Sphere;
    use dereth_primitives::Vec3;
    const SURFACE: f32 = 70.95;
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    land.add_block(&w.records, &w.region, &mut trees, 0x02, 0x98);
    let md: MotionDesc = w.records.get(0x3200_0000).expect("the player's motion");
    let values = |element: u32| -> MotionParams {
        let (_, set) = md
            .set_of
            .iter()
            .find(|(k, _)| k.element == element && k.mode == PEACE)
            .expect("in the everyday mode");
        md.sets[*set as usize].default
    };
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: values(EARTH),
        air: values(EARTH),
    };
    let cell = CellId(0x0298_0001);
    let at = Position::new(cell, Frame::new(Vec3::new(12.0, 12.0, 0.5), Quat::IDENTITY));
    let mut b = Body::new(config, at, 0.0);
    b.minterp.set_description(1, std::sync::Arc::new(md));
    let dt = auberean_physics::body::tick_seconds();
    b.tick(&land, dt);
    b.tick(&land, dt);
    assert_eq!(b.element(), WATER, "in the water");
    assert!(!b.contact, "off the bed");
    for _ in 0..40 * 30 {
        b.tick(&land, dt);
    }
    let o = b.position.frame.origin;
    assert!(
        o.z > SURFACE - 3.0 && o.z < SURFACE && b.depth().abs() < 0.1,
        "floats at the surface: {o:?}, depth {}",
        b.depth()
    );
    let wanted = Vec3::new(o.x, o.y - 0.9, o.z - 4.4);
    let world = auberean_physics::world::World::default();
    let seen = sight_lines(
        &world,
        &Lines {
            land: &land,
            cell,
            origin: o,
            target_offset: Vec3::new(0.0, 0.0, 1.2),
            wanted,
            env_only: false,
        },
    )
    .expect("the lines start in a loaded cell");
    assert!(
        (seen.eye.frame.origin.z - wanted.z).abs() < 0.01,
        "where it wants: {:?}",
        seen.eye
    );
}

/// The points `d` meters either side of doorway `door` of the room `room`, out from its middle:
/// the one its room's own cell tree does not hold (beyond), and the one it does (short of it).
fn either_side(
    room: &dereth_physics::source::EnvCellGeometry,
    door: &dereth_physics::source::CellPortal,
    d: f32,
) -> (dereth_primitives::Vec3, dereth_primitives::Vec3) {
    use dereth_physics::math::{globaltolocal, l2g, localtoglobal, localtoglobalvec};
    use dereth_primitives::Vec3;
    let corners = &door.portal.vertices;
    #[allow(clippy::cast_precision_loss)]
    let k = 1.0 / corners.len() as f32;
    let mid = corners.iter().fold(Vec3::ZERO, |a, v| {
        Vec3::new(a.x + v.x * k, a.y + v.y * k, a.z + v.z * k)
    });
    let middle = localtoglobal(&room.frame, mid);
    let facing = localtoglobalvec(l2g(room.frame.rotation), door.portal.plane.normal);
    let along = |d: f32| {
        Vec3::new(
            middle.x + facing.x * d,
            middle.y + facing.y * d,
            middle.z + facing.z * d,
        )
    };
    let held = |p: Vec3| {
        room.cell_bsp
            .as_ref()
            .is_some_and(|t| t.point_inside_cell_bsp(globaltolocal(&room.frame, p)))
    };
    let sign = if held(along(d)) { -1.0 } else { 1.0 };
    (along(sign * d), along(-sign * d))
}

/// A place in cell `cell` at `origin`, facing north.
fn place(cell: CellId, origin: dereth_primitives::Vec3) -> Position {
    Position::new(cell, Frame::new(origin, Quat::IDENTITY))
}

/// A frame is drawn from the room the camera is drawn in. Drawn between its last two places, the
/// camera counts as in the room it stood in before; 10 cm past a doorway of that room the frame is
/// drawn from the room beyond, and 10 cm short of it from the room itself, at every doorway between
/// two rooms of the hall's copy.
///
/// Behaviour: draw.inside.a-frame-is-drawn-from-the-room-the-camera-is-drawn-in
#[test]
fn a_frame_is_drawn_from_the_room_the_camera_is_drawn_in() {
    use auberean_physics::cells::OUTDOORS;
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    add_block_cells(&land, &w.records, &mut trees, 0x02, 0x98);
    let mut doorways = 0;
    for id in land.interior_cells_of(0x0298_0000) {
        let room = land.cell(id).expect("an added room");
        for door in room.portals.iter().filter(|p| p.other_cell_id != OUTDOORS) {
            let (past, short) = either_side(&room, door, 0.1);
            let beyond = CellId(door.other_cell_id);
            assert_eq!(
                drawn_from(&land, &place(id, past)).cell,
                beyond,
                "{id:?} past its doorway into {beyond:?}, at {past:?}"
            );
            let here = drawn_from(&land, &place(id, short));
            assert_eq!(here.cell, id, "{id:?} short of its doorway, at {short:?}");
            assert_eq!(here.frame.origin, short);
            doorways += 1;
        }
    }
    assert!(doorways >= 150, "{doorways} doorways between rooms");
}

/// The cave's first room, its doorway out, and the point 10 cm out of that doorway, from a world
/// holding block 0x6967's cells and, when `with_land`, its land.
fn out_of_the_mouth(
    w: &crate::common::World,
    with_land: bool,
) -> (Land, CellId, dereth_primitives::Vec3) {
    use auberean_physics::cells::OUTDOORS;
    let mouth = CellId(0x6967_0100);
    let land = Land::new();
    let mut trees = Trees::default();
    if with_land {
        land.add_block(&w.records, &w.region, &mut trees, 0x69, 0x67);
    }
    add_block_cells(&land, &w.records, &mut trees, 0x69, 0x67);
    let room = land.cell(mouth).expect("the cave's first room");
    let door = room
        .portals
        .iter()
        .find(|p| p.other_cell_id == OUTDOORS)
        .expect("a doorway out");
    let (out, _) = either_side(&room, door, 0.1);
    (land, mouth, out)
}

/// Drawn 10 cm out of a cave's mouth while still counted in its first room, the camera has its
/// frame drawn from the land cell under it, the land being in.
///
/// Behaviour: draw.inside.a-frame-is-drawn-from-the-land-once-the-camera-is-out
#[test]
fn a_frame_is_drawn_from_the_land_once_the_camera_is_out() {
    use auberean_physics::cells::adjust_to_outside;
    let w = need_world!();
    let (land, mouth, out) = out_of_the_mouth(w, true);
    let (mut under, mut origin) = (mouth, out);
    assert!(adjust_to_outside(&mut under, &mut origin));
    let drawn = drawn_from(&land, &place(mouth, out));
    assert_eq!(drawn.cell, under, "the land cell under it");
    assert!(!is_interior(drawn.cell));
    assert_eq!(drawn.frame.origin, origin);
}

/// Drawn 10 cm out of a cave's mouth while still counted in its first room, with no land in, the
/// camera keeps that room to draw its frame from.
///
/// Behaviour: draw.inside.a-camera-out-of-the-rooms-with-no-land-keeps-its-room
#[test]
fn a_camera_out_of_the_rooms_with_no_land_keeps_its_room() {
    let w = need_world!();
    let (land, mouth, out) = out_of_the_mouth(w, false);
    let drawn = drawn_from(&land, &place(mouth, out));
    assert_eq!(drawn.cell, mouth);
    assert_eq!(drawn.frame.origin, out);
}

/// A camera drawn 10 cm in through a doorway out of a cave or a building, still counted in the land
/// cell outside it, has its frame drawn from the room inside; 10 cm short of the doorway, from the
/// land cell. Every doorway out, that the land cell under it knows, of the rooms of twelve blocks
/// with doorways out: caves, dungeon mouths and buildings.
///
/// Behaviour: draw.inside.a-frame-is-drawn-from-the-room-once-the-camera-is-in
#[test]
fn a_frame_is_drawn_from_the_room_once_the_camera_is_in() {
    use auberean_physics::cells::{adjust_to_outside, OUTDOORS};
    let w = need_world!();
    let outside = |cell: CellId, at: dereth_primitives::Vec3| {
        let (mut over, mut origin) = (cell, at);
        assert!(adjust_to_outside(&mut over, &mut origin));
        place(over, origin)
    };
    let (mut doorways, mut unknown) = (0, 0);
    for (x, y) in [
        (0x69u8, 0x67u8),
        (0xDA, 0xAC),
        (0x8B, 0x91),
        (0x68, 0x88),
        (0x58, 0x70),
        (0x3F, 0x9E),
        (0x32, 0x4E),
        (0x04, 0xB5),
        (0x04, 0xA8),
        (0xF1, 0x8F),
        (0xF0, 0x75),
        (0xE1, 0xA8),
    ] {
        let block = u32::from(x) << 24 | u32::from(y) << 16;
        let land = Land::new();
        let mut trees = Trees::default();
        land.add_block(&w.records, &w.region, &mut trees, x, y);
        add_block_cells(&land, &w.records, &mut trees, x, y);
        for id in land.interior_cells_of(block) {
            let room = land.cell(id).expect("an added room");
            for door in room.portals.iter().filter(|p| p.other_cell_id == OUTDOORS) {
                let (short, inside) = either_side(&room, door, 0.1);
                let from = outside(id, inside);
                if !land.touching_cells(from.cell).contains(&id) {
                    unknown += 1;
                    continue;
                }
                let drawn = drawn_from(&land, &from);
                assert_eq!(drawn.cell, id, "{id:?} 10 cm in, from {:?}", from.cell);
                assert_eq!(drawn.frame.origin, inside);
                let before = outside(id, short);
                assert_eq!(drawn_from(&land, &before), before, "{id:?} 10 cm short");
                doorways += 1;
            }
        }
    }
    println!("{doorways} doorways out, {unknown} whose land cell does not know the room");
    assert!(doorways >= 12, "{doorways} doorways out");
}
