//! Routes in Holtburg's Training Academy where a new character's walks went wrong, planned over
//! the retail cells and driven by keys through the physics world at 60, 20, 10 and 5 frames a
//! second, as the client's travel drives the player's character.

use dereth_navigation::NavPath;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;
use crate::drive::drive;

fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

/// Plan from `from` to `to` and drive the route at each frame rate: it arrives, never stuck, in
/// little more than a body running it without a stop would take.
fn driven_at_every_frame_rate(d: &mut Dungeon, what: &str, from: &Position, to: &Position) {
    let path: NavPath = d
        .nav
        .plan_toward(from, to)
        .unwrap_or_else(|e| panic!("{what}: no route: {e}"))
        .0;
    for fps in [60.0, 20.0, 10.0, 5.0] {
        let player = std::sync::Arc::clone(&d.player);
        let Dungeon { world, nav, .. } = d;
        nav.unblock();
        let mut replan = |p: &Position, toward: &Position| {
            nav.block_ahead(p, toward);
            nav.plan_toward(p, to).ok().map(|(p, _)| p)
        };
        let r = drive(world, &player, path.clone(), fps, &mut replan);
        assert!(
            r.arrived,
            "{what} at {fps} frames a second: did not arrive ({r:?})"
        );
        assert_eq!(r.stuck, 0, "{what} at {fps} frames a second: stuck ({r:?})");
        assert!(
            r.seconds <= r.ideal * 1.5 + 3.0,
            "{what} at {fps} frames a second: {:.1} s against {:.1} s",
            r.seconds,
            r.ideal
        );
    }
}

/// Behaviour: plugins.nav.a-route-is-followed-at-any-frame-rate
#[test]
fn academy_routes_that_went_wrong_are_driven_to_their_end_at_60_20_10_and_5_frames_a_second() {
    let mut d = dungeon(0x8602);
    // Through the Outer Courtyard portal, to the Sentry: the first straight walk stopped against
    // the courtyard's floor.
    driven_at_every_frame_rate(
        &mut d,
        "outer courtyard",
        &at(0x8602_02C3, 119.0, -141.0, 0.0),
        &at(0x8602_02C2, 123.75, -133.0, 0.0),
    );
    // Through the Central Courtyard portal, to the Foreman.
    driven_at_every_frame_rate(
        &mut d,
        "central courtyard",
        &at(0x8602_021E, 50.0, -54.0, 0.0),
        &at(0x8602_01F4, 36.75, -73.75, 0.0),
    );
    // Up the ramp between the hive's levels, to a place seven metres off on the level above.
    driven_at_every_frame_rate(
        &mut d,
        "hive ramp",
        &at(0x8602_01A5, 188.75, -209.0, -8.04),
        &at(0x8602_01A7, 187.75, -216.25, -6.0),
    );
    // To a waypoint three metres off in the hive, which a body circled for fifteen seconds.
    driven_at_every_frame_rate(
        &mut d,
        "hive waypoint",
        &at(0x8602_010E, 178.25, -246.75, -12.0),
        &at(0x8602_0108, 179.5, -244.25, -12.0),
    );
    // Into the woodpile, where a wasp was given up as out of reach.
    driven_at_every_frame_rate(
        &mut d,
        "woodpile",
        &at(0x8602_01F4, 36.75, -73.75, 0.0),
        &at(0x8602_01AF, 9.0, -84.0, 0.0),
    );
}
