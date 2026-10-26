//! Routes to someone a body cannot stand beside: Shopkeeper Renald the Elder in Holtburg, who
//! keeps a booth in the corner of his shop (cell `0x0141` of `0xA9B4`). The booth opens onto the
//! street through a window whose sill is 0.85 m up and onto the shop's two rooms across counters a
//! metre high: it has no door a body walks through.

use dereth_navigation::{NavPath, PhysicsWalker, SegmentCheck, WalkerConfig};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;

/// Where the world places Shopkeeper Renald the Elder.
fn shopkeeper() -> Position {
    Position::new(
        CellId(0xA9B4_0141),
        Frame::new(Vec3::new(139.234, 18.329, 94.005), Quat::IDENTITY),
    )
}

/// A place in the shop's front room, by the counter into the booth.
fn front_room() -> Position {
    Position::new(
        CellId(0xA9B4_013F),
        Frame::new(Vec3::new(134.5, 14.9, 94.0), Quat::IDENTITY),
    )
}

/// Walk a body from `from` straight at `to`, and say where it stopped.
fn walk_at(d: &mut Dungeon, from: &Position, to: &Position) -> (bool, Position) {
    let mut w = PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        WalkerConfig::default(),
    );
    assert!(w.place(from), "the body stands at {from:?}");
    let out = w.walk(to);
    (out.reached, out.at)
}

/// Walk a body along `route`, corner by corner; whether it got to the end.
fn walk_along(d: &mut Dungeon, route: &NavPath) -> bool {
    let mut w = PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        WalkerConfig::default(),
    );
    let first = route.waypoints[0].position;
    w.place(&first)
        && route.waypoints[1..]
            .iter()
            .all(|p| w.walk(&p.position).reached)
}

/// Behaviour: world.interiors.holtburgs-elder-shopkeeper-is-reached-only-across-his-window-or-counter
#[test]
fn a_body_walking_at_holtburgs_elder_shopkeeper_stops_at_his_window_or_his_counter() {
    let mut d = outdoors(0xA9B4, 1);
    // From the street in front of his window.
    let street = ground(&d, 0xA9B4, 139.25, 21.5);
    let (reached, at) = walk_at(&mut d, &street, &shopkeeper());
    assert!(!reached, "the window lets no body in: {at:?}");
    assert!(at.cell.0 & 0xFFFF < 0x0100, "stopped outdoors: {at:?}");
    assert!(
        at.frame.origin.y > 19.5 && at.frame.origin.y < 20.5,
        "against the window's outer face: {at:?}"
    );
    // From the shop's front room, at the counter between it and the booth.
    let (reached, at) = walk_at(&mut d, &front_room(), &shopkeeper());
    assert!(!reached, "the counter lets no body through: {at:?}");
    assert_ne!(
        at.cell,
        shopkeeper().cell,
        "stopped outside the booth: {at:?}"
    );
    assert!(
        at.frame.origin.x < 136.3,
        "on the shop's side of the counter: {at:?}"
    );
}

/// Behaviour: plugins.nav.a-route-to-someone-walled-off-ends-as-near-them-as-a-body-walks
#[test]
fn a_route_to_holtburgs_elder_shopkeeper_ends_outside_his_window_from_the_street_or_the_shop() {
    let reach = dereth_client_runtime::travel::OBJECT_REACH;
    let mut d = outdoors(0xA9B4, 1);
    let street = ground(&d, 0xA9B4, 139.2, 26.0);
    for (from, name) in [(street, "the street"), (front_room(), "his shop")] {
        let route = d
            .nav
            .plan_near(&from, &shopkeeper(), reach)
            .unwrap_or_else(|e| panic!("from {name}: {e}"));
        let end = route.waypoints.last().expect("an end").position;
        assert!(
            end.cell.0 & 0xFFFF < 0x0100 && end.frame.origin.y > 19.5,
            "from {name} the route ends in the street at his window: {end:?}"
        );
        assert!(route.goal_offset <= reach, "{}", route.goal_offset);
        assert!(
            walk_along(&mut d, &route),
            "from {name} the route is walked"
        );
    }
}
