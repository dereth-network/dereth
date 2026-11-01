//! A creature walking a route through the September 2013 world's walls: a guard's corridor on the
//! upper floor of the Lugian fortress (0x048B), walked end to end and back with the test beetle's
//! body, every place it is told at inside the room it names.

use yalain_content::movement::{Gait, Shape, Waypoint};
use yalain_content::{CampMoves, Route};
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::ai;

use crate::walls::{inside, walls};

/// The corridor's two ends, as a guard walked it in 2017.
const EAST: (u32, [f32; 3]) = (0x048B_0162, [-285.6, 30.0, 148.0]);
const WEST: (u32, [f32; 3]) = (0x048B_0150, [-344.3, 30.0, 148.0]);

fn place(end: (u32, [f32; 3])) -> StoredPosition {
    StoredPosition {
        cell: end.0,
        origin: end.1,
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

fn flat(a: &StoredPosition, b: [f32; 3]) -> f32 {
    ((a.origin[0] - b[0]).powi(2) + (a.origin[1] - b[1]).powi(2)).sqrt()
}

/// A creature on a route walks it leg by leg through the walls: from its camp at the east end to
/// the west end and back, every place inside the cell it names.
/// Behaviour: server.creatures.patrol-walks-indoors
#[test]
fn a_creature_walks_a_fortress_corridor_route_end_to_end_through_the_walls() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let camp = &mut h.server.spawns.camps[0];
    camp.cell = EAST.0;
    camp.origin = EAST.1;
    camp.spots = vec![EAST];
    camp.moves = CampMoves {
        route: Some(Route {
            key: "corridor".into(),
            shape: Shape::BackAndForth,
            gait: Gait::Run,
            speed: None,
            waypoints: vec![
                Waypoint {
                    cell: EAST.0,
                    at: EAST.1,
                    rest: 1.0,
                },
                Waypoint {
                    cell: WEST.0,
                    at: WEST.1,
                    rest: 1.0,
                },
            ],
        }),
        ..CampMoves::default()
    };
    let mut w = walls();
    let _ = w.joined(&place(EAST), &place(WEST));
    h.server.walls = Some(w);
    let (_, _) = h.player_in_world("one", "Elara");
    // The player stands in the corridor (a creature no player could see rests).
    let middle = StoredPosition {
        origin: [-315.0, 30.0, 148.0],
        ..place(EAST)
    };
    for p in h.server.players.values_mut() {
        p.position = middle;
    }
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .expect("the camp's beetle");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.attacks_first = false;
    }
    let mut reached = Vec::new();
    for _ in 0..1200 {
        h.step();
        let at = h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.at)
            .expect("placed");
        let w = h.server.walls.as_ref().expect("walls");
        assert!(inside(w, &at), "inside the cell it names: {at:?}");
        for (name, end) in [("west", WEST), ("east", EAST)] {
            if flat(&at, end.1) <= ai::ARRIVED && reached.last() != Some(&name) {
                reached.push(name);
            }
        }
        if reached.len() >= 3 {
            break;
        }
    }
    assert_eq!(reached[..], ["east", "west", "east"], "end to end and back");
}
