//! Where the route check puts its body down: a place on a floor, which the planner gives at the
//! floor's own height, in Holtburg's Training Academy (`0x8602`).

use std::sync::Arc;

use dereth_navigation::SegmentCheck;
use dereth_primitives::{CellId, Frame, ObjectId, Position, Quat, Vec3};

use crate::common::*;

/// A place on the floor of the academy's hive (cell `0x010E`) where the world refuses a person
/// put with their feet on the floor: their lowest sphere is 5 mm in it.
fn hive_floor() -> Position {
    Position::new(
        CellId(0x8602_010E),
        Frame::new(Vec3::new(178.25, -246.75, -12.0), Quat::IDENTITY),
    )
}

/// Behaviour: plugins.nav.a-body-checking-a-route-is-put-down-resting-on-the-floor
#[test]
fn a_body_is_put_down_resting_on_the_floor_where_one_with_its_feet_in_it_is_refused() {
    let mut d = dungeon(0x8602);
    let at = hive_floor();
    let body = d
        .world
        .create(ObjectId(0x5000_0009), Arc::clone(&d.player), true);
    assert!(
        !d.world.set_position(body, &at),
        "the world takes no person sunk 5 mm into the hive's floor"
    );

    // Every clear place of the academy, the hive's among them, is stood on.
    let region = d.nav.region(d.first_cell()).expect("loaded");
    let places: Vec<Position> = region
        .places()
        .filter(|&i| region.place(i).clear)
        .map(|i| region.position(i))
        .collect();
    let refused = places
        .iter()
        .filter(|p| !dereth_navigation::stand(&mut d.world, body, &d.player, p))
        .count();
    assert_eq!(refused, 0, "of {} clear places", places.len());
    d.world.destroy(body);

    // The walker's body rests on the floor 5 mm above the place, where it stands.
    let mut w = d.walker();
    assert!(w.place(&at), "the walker's body stands in the hive");
    let p = w.position().expect("in the world");
    assert_eq!(p.cell, at.cell);
    let o = p.frame.origin;
    assert!(
        (o.x - 178.25).abs() < 1e-3 && (o.y + 246.75).abs() < 1e-3 && (o.z + 11.995).abs() < 1e-3,
        "{o:?}"
    );
}
