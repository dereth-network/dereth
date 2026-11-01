//! The cell the camera stands in, which chooses what a frame of the world draws. Fixture: hall
//! copy 0x0298 of the September 2013 cell file (rooms 40 m across, a room open to the sky above
//! each), and block 0x6967, whose cave's first room (0x0100) has a doorway out to the land.

use auberean_physics::camera::{camera_cell, touches_outside};
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use dereth_primitives::{CellId, Vec3};

use crate::common::need_world;

/// From a room the camera stands in the room, or a room it sees, that holds its point; past every
/// room it sees, on the land outside under the point where one of them has a doorway out, and in
/// the room it started from where none has (the camera keeps to the rooms). From the land it
/// stands on the land, unless a room of the block with a doorway out holds the point.
///
/// Behaviour: draw.inside.the-frame-is-chosen-by-the-cell-the-camera-stands-in
#[test]
fn the_camera_stands_in_the_room_that_holds_it_or_on_the_land() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    add_block_cells(&land, &w.records, &mut trees, 0x02, 0x98);
    let hall = CellId(0x0298_0109);
    // in the room itself, and in the room beside it (0x0100, 40 m west)
    let here = camera_cell(&land, hall, Vec3::new(59.0, 240.0, -44.0));
    assert_eq!(here.map(|(c, _)| c), Some(hall));
    let (beside, at) = camera_cell(&land, hall, Vec3::new(5.0, 262.0, -44.0)).expect("a cell");
    assert_eq!(beside, CellId(0x0298_0100));
    assert_eq!((at.x, at.y), (5.0, 262.0), "the same block");
    // far above every room: the hall has no doorway out, so the room it started from
    let (kept, _) = camera_cell(&land, hall, Vec3::new(100.0, 100.0, 900.0)).expect("a cell");
    assert_eq!(kept, hall);

    // The cave mouth: its first room leads out.
    land.add_block(&w.records, &w.region, &mut trees, 0x69, 0x67);
    add_block_cells(&land, &w.records, &mut trees, 0x69, 0x67);
    let mouth = CellId(0x6967_0100);
    let geom = land.cell(mouth).expect("the cave's first room");
    assert!(touches_outside(&geom));
    let o = geom.frame.origin;
    let inside = Vec3::new(o.x, o.y, o.z + 1.0);
    let ground = camera_cell(&land, CellId(0x6967_0001), Vec3::new(5.0, 5.0, 300.0))
        .expect("a cell")
        .0;
    assert_eq!(ground.0 >> 16, 0x6967);
    assert!((1..=64).contains(&(ground.0 & 0xFFFF)));
    let walked_in = camera_cell(&land, ground, inside).expect("a cell").0;
    assert_eq!(
        walked_in, mouth,
        "the room holding the point, from the land"
    );
    assert_eq!(camera_cell(&land, mouth, inside).map(|c| c.0), Some(mouth));
    // out through the doorway, high above the room: the land outside
    let (out, _) = camera_cell(&land, mouth, Vec3::new(o.x, o.y, o.z + 400.0)).expect("a cell");
    assert_eq!(out.0 >> 16, 0x6967);
    assert!((1..=64).contains(&(out.0 & 0xFFFF)), "{:08X}", out.0);
}
