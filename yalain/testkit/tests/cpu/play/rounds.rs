//! A world's rounds: one creature at a time going round places in order, each death bringing the
//! next stop's, the one a stop names for its killer's kingdom where it names one. Fixture: the test
//! world, one player, a round of the test creature and a copy of it.

use yalain_content::rounds::{Round, RoundTable, Stop};
use yalain_content::Place;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::combat;
use yalain_world::kingdoms::{kingdom, set_kingdom};
use yalain_world::things::Mode;

/// The copy of the test creature a stop names for the Order's killers.
const ORDERS: u32 = 0x4700_9224;

fn place(x: f32) -> Place {
    Place {
        cell: 0x0299_0109,
        origin: [x, 230.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

fn world(gap: Option<f64>) -> (Harness, u64) {
    let mut h = Harness::new();
    h.add_creatures(0);
    let mut orders = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("the test creature");
    orders.id = ORDERS;
    h.server.catalog.insert(orders);
    let stop = |x: f32| Stop {
        at: place(x),
        template: TEST_CREATURE,
        by_kingdom: vec![(kingdom::ORDER, ORDERS)],
    };
    h.server.content.rounds = RoundTable {
        rows: vec![Round {
            key: "guardian".into(),
            stops: vec![stop(70.0), stop(75.0), stop(80.0)],
            gap,
        }],
    };
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    (h, me)
}

/// The round's creatures alive: each one's template and where it stands across.
fn standing(h: &Harness) -> Vec<(u32, f32)> {
    let mut v: Vec<(u32, f32)> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .filter_map(|t| Some((t.template, t.at?.origin[0])))
        .collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1));
    v
}

/// The round's creature, hurt to death by this character.
fn kill(h: &mut Harness, by: u64) {
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
        .expect("the round's creature");
    let t = h.server.things.get_mut(id).expect("it");
    t.health = 10;
    t.max_health = 10;
    let clock = h.clock();
    combat::hurt_creature(&mut h.server, &mut h.net, id, by, 50, clock);
    h.run(2);
}

/// The first stop's creature stands as the world starts; each death brings the next stop's, one at
/// a time, round to the first again after the last; a stop stands the creature it names for its
/// killer's kingdom, and its own for a killer in no kingdom.
/// Behaviour: server.creatures.a-round-goes-round-its-places-one-at-a-time
#[test]
fn a_rounds_creature_goes_round_its_stops_one_at_a_time_as_its_killers_kingdom_calls_for() {
    let (mut h, me) = world(None);
    assert_eq!(standing(&h), [(TEST_CREATURE, 70.0)], "the first stop's");
    kill(&mut h, me);
    assert_eq!(
        standing(&h),
        [(TEST_CREATURE, 75.0)],
        "the next, a killer in no kingdom"
    );
    for p in h.server.players.values_mut() {
        set_kingdom(&mut p.qualities, kingdom::ORDER);
    }
    kill(&mut h, me);
    assert_eq!(
        standing(&h),
        [(ORDERS, 80.0)],
        "the Order's, after an Order killer"
    );
    kill(&mut h, me);
    assert_eq!(
        standing(&h),
        [(ORDERS, 70.0)],
        "round to the first after the last"
    );
}

/// Outdoors, a stop's creature stands on the ground under its place, whatever height the place
/// gives; indoors, at the place's own height.
/// Behaviour: server.creatures.a-round-goes-round-its-places-one-at-a-time
#[test]
fn a_rounds_creature_stands_on_the_ground_outdoors() {
    let (mut h, _) = world(None);
    h.server.ground = Some(Box::new(yalain_testkit::Slope));
    h.server.content.rounds.rows[0].stops[0].at = Place {
        cell: 0x6FA9_0012,
        origin: [40.0, 50.0, 999.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    yalain_world::rounds::forget_all(&mut h.server, &mut h.net);
    h.run(2);
    let z = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .find_map(|t| t.at.filter(|at| at.cell == 0x6FA9_0012))
        .map(|at| at.origin[2]);
    assert_eq!(z, Some(8.0), "on the slope, a fifth of 40 m up");
}

/// With a gap, nothing of the round stands until the gap has gone after a death.
/// Behaviour: server.creatures.a-round-goes-round-its-places-one-at-a-time
#[test]
fn a_rounds_next_creature_waits_out_its_gap() {
    let (mut h, me) = world(Some(30.0));
    kill(&mut h, me);
    assert_eq!(standing(&h), [], "within the gap");
    let t = h.now.0;
    while h.now.0 < t + 31.0 {
        h.step();
    }
    assert_eq!(standing(&h), [(TEST_CREATURE, 75.0)], "after it");
}
