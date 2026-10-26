//! Routes into the rooms of buildings: Yaohan Tong the Healer in Shoushi (`0xDA55`), whose room
//! (cell `0x014B`) is sunk 1.2 m below the ground the building stands on, inside the footprint of
//! the building's shell and under the floor that shell has at ground level.

use dereth_navigation::{PhysicsWalker, SegmentCheck, WalkerConfig};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;

/// Behaviour: plugins.nav.a-route-into-a-room-sunk-below-the-ground-is-found-and-walked
#[test]
fn a_route_from_shoushis_street_down_into_its_healers_sunken_room_is_found_and_walked() {
    let mut d = outdoors(0xDA55, 1);
    let street = ground(&d, 0xDA55, 100.75, 115.0);
    let healer = Position::new(
        CellId(0xDA55_014B),
        Frame::new(Vec3::new(129.12, 29.76, 18.805), Quat::IDENTITY),
    );
    let route = d.nav.plan(&street, &healer).expect("a route into the room");
    let end = route.waypoints.last().expect("an end").position;
    assert_eq!(end.cell, healer.cell, "the route ends in the healer's room");
    assert!(
        route.goal_offset < 0.5,
        "at the healer: {}",
        route.goal_offset
    );
    let mut w = PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        WalkerConfig::default(),
    );
    assert!(
        w.place(&route.waypoints[0].position),
        "the body stands in the street"
    );
    for p in &route.waypoints[1..] {
        let out = w.walk(&p.position);
        assert!(
            out.reached,
            "the body walks the route: stopped at {:?} short of {:?}",
            out.at, p.position
        );
    }
}
