//! Creatures at rest: where they wander and where they never do, how a walk or a run home ends,
//! and the idle behaviors they play now and then.

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_content::Prop;
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, Slope, TEST_CREATURE};
use yalain_world::ai::{self, IDLE_BEHAVIORS};
use yalain_world::things::Mode;
use yalain_world::world::IDLE_MOTION;

/// The test creature's template gets a property before its camp makes it.
fn with_prop(h: &mut Harness, name: &str, value: Prop) {
    let mut t = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("beetle");
    t.props.set(name, value);
    h.server.catalog.insert(t);
}

/// A world with one beetle and a player beside it; the beetle never attacks first. Returns the
/// harness, the client and the beetle.
fn resting(props: &[(&str, Prop)]) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    for (name, value) in props {
        with_prop(&mut h, name, value.clone());
    }
    let (b, _) = h.player_in_world("one", "Elara");
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
    (h, b, beetle)
}

/// An outdoor place far from the training hall.
fn outdoors() -> StoredPosition {
    StoredPosition {
        cell: 0xA9B4_0017,
        origin: [80.0, 80.0, 10.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// Make a creature's home a place, put it there, and have it think of wandering now, sure to go.
fn home_at(h: &mut Harness, id: u64, at: StoredPosition, range: f32) {
    let now = h.now.0;
    if let Some(t) = h.server.things.get_mut(id) {
        t.at = Some(at);
        if let Some(c) = t.creature.as_mut() {
            c.home = at;
            c.next_wander = now;
            c.stats.movement.wander_range = range;
            c.stats.movement.wander_chance = 0.0;
        }
    }
}

fn place_of(h: &Harness, id: u64) -> StoredPosition {
    h.server.things.get(id).and_then(|t| t.at).expect("placed")
}

fn flat(a: &StoredPosition, b: &StoredPosition) -> f32 {
    let (dx, dy, _) = ai::offset(a, b);
    (dx * dx + dy * dy).sqrt()
}

/// Outdoors, a creature at rest walks to a point within its wandering range of home, and when it
/// gets there it stands.
/// Behaviour: server.creatures.wander-and-stop-standing
#[test]
fn a_creature_at_rest_outdoors_walks_within_its_range_and_stands_where_it_ends() {
    let (mut h, _, beetle) = resting(&[]);
    let home = outdoors();
    home_at(&mut h, beetle, home, 10.0);
    assert!(
        h.run_until(40, |h| flat(&place_of(h, beetle), &home) > 0.1),
        "it walks"
    );
    let walking = h.server.things.get(beetle).expect("beetle").heading_motion & 0x00FF_FFFF;
    assert_eq!(walking, ai::WALKING, "walking");
    assert!(
        h.run_until(200, |h| h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.wander_to.is_none())),
        "the walk ends"
    );
    let t = h.server.things.get(beetle).expect("beetle");
    assert_eq!(t.heading_motion & 0x00FF_FFFF, IDLE_MOTION, "it stands");
    assert!(
        flat(&place_of(&h, beetle), &home) <= 10.01,
        "within its range"
    );
}

/// A walk at rest is a short one from where the creature stands, kept within its wandering range of
/// home: walk after walk, each from where the last ended, the median walk is a third to a half of
/// the range (not most of it, as a goal anywhere about home would make it), and every walk ends
/// within the range, a walk from its very edge too.
/// Behaviour: server.creatures.wander-short-walks
#[test]
fn a_walk_at_rest_is_drawn_from_where_the_creature_stands_and_ends_within_its_range() {
    let mut rng = yalain_world::rng::Rng::new(7);
    let range = 20.0_f32;
    let far = |p: [f32; 2]| (p[0] * p[0] + p[1] * p[1]).sqrt();
    let mut at = [0.0_f32, 0.0];
    let mut walks = Vec::new();
    for _ in 0..2000 {
        let goal = ai::wander_goal(&mut rng, at, range);
        assert!(far(goal) <= range + 1e-3, "{goal:?}");
        walks.push(far([goal[0] - at[0], goal[1] - at[1]]));
        at = goal;
    }
    walks.sort_by(f32::total_cmp);
    let median = walks[walks.len() / 2];
    assert!(
        (0.3 * range..0.5 * range).contains(&median),
        "median walk {median}"
    );
    for _ in 0..200 {
        let goal = ai::wander_goal(&mut rng, [range, 0.0], range);
        assert!(far(goal) <= range + 1e-3, "from the edge: {goal:?}");
    }
}

/// On a slope a walk still ends where it should, on the ground at its goal (the goal's height is
/// home's, the ground's is not), and the creature stands there and wanders again later.
/// Behaviour: server.creatures.wander-and-stop-standing
#[test]
fn a_walk_on_a_slope_ends_at_its_goal_and_the_creature_wanders_again() {
    let (mut h, _, beetle) = resting(&[]);
    h.server.ground = Some(Box::new(Slope));
    let home = outdoors();
    home_at(&mut h, beetle, home, 10.0);
    let ended = |h: &Harness| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.wander_to.is_none())
    };
    assert!(h.run_until(20, |h| !ended(h)), "it sets out");
    let goal = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.wander_to)
        .expect("a goal");
    assert!(h.run_until(200, ended), "the walk ends");
    assert!(flat(&place_of(&h, beetle), &goal) <= ai::ARRIVED);
    let t = h.server.things.get(beetle).expect("beetle");
    assert_eq!(t.heading_motion & 0x00FF_FFFF, IDLE_MOTION, "it stands");
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.next_wander = now;
    }
    assert!(h.run_until(20, |h| !ended(h)), "it wanders again");
}

/// A creature whose homesick radius is under 5 m never wanders, outdoors or in.
/// Behaviour: server.creatures.settled-never-wander
#[test]
fn a_creature_with_a_homesick_radius_under_five_meters_never_wanders() {
    let (mut h, _, beetle) = resting(&[("AI_HomesickRadius", Prop::Float(4.0))]);
    let home = outdoors();
    home_at(&mut h, beetle, home, 10.0);
    for _ in 0..20 {
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_wander = now;
        }
        h.run(5);
    }
    assert_eq!(place_of(&h, beetle), home, "never moved");
}

/// A creature whose template says it does not wander never does.
/// Behaviour: server.creatures.not-wandering-never-wander
#[test]
fn a_creature_whose_template_says_it_does_not_wander_never_wanders() {
    let (mut h, _, beetle) = resting(&[("AI_Wandering", Prop::Bool(false))]);
    let home = outdoors();
    home_at(&mut h, beetle, home, 10.0);
    for _ in 0..20 {
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_wander = now;
        }
        h.run(5);
    }
    assert_eq!(place_of(&h, beetle), home, "never moved");
}

/// Indoors a creature at rest stays where it is, however often a walk falls due; the same
/// creature outdoors sets out.
/// Behaviour: server.creatures.indoor-rest-in-place
#[test]
fn indoors_a_creature_at_rest_stays_where_it_is() {
    let (mut h, _, beetle) = resting(&[]);
    let home = place_of(&h, beetle);
    assert!(home.cell & 0xFFFF >= 0x100, "the test camp is indoors");
    home_at(&mut h, beetle, home, 10.0);
    for _ in 0..20 {
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_wander = now;
        }
        h.run(5);
    }
    assert_eq!(place_of(&h, beetle), home, "never moved indoors");
    home_at(&mut h, beetle, outdoors(), 10.0);
    assert!(
        h.run_until(40, |h| flat(&place_of(h, beetle), &outdoors()) > 0.1),
        "it walks outdoors"
    );
}

/// A creature running home that gets there is told standing at home: its last place carries no
/// running.
/// Behaviour: server.creatures.wander-and-stop-standing
#[test]
fn a_creature_that_runs_home_is_told_standing_when_it_gets_there() {
    let (mut h, b, beetle) = resting(&[]);
    let home = place_of(&h, beetle);
    let away = StoredPosition {
        origin: [home.origin[0] + 4.0, home.origin[1], home.origin[2]],
        ..home
    };
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(away);
        if let Some(c) = t.creature.as_mut() {
            c.mode = Mode::Returning;
        }
    }
    let from = h.bots[b].inbox.len();
    assert!(
        h.run_until(40, |h| h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.mode == Mode::Idle)),
        "it gets home"
    );
    h.run(4);
    let motions: Vec<u32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p)
                if p.sender.id == beetle =>
            {
                Some(p.heading_motion & 0x00FF_FFFF)
            }
            _ => None,
        })
        .collect();
    assert!(motions.contains(&ai::RUNNING), "it ran: {motions:x?}");
    assert_eq!(
        motions.last(),
        Some(&IDLE_MOTION),
        "told standing: {motions:x?}"
    );
}

/// At rest and standing, a creature of a class of behavior plays one of its two idle behaviors
/// when its time comes, for the clients that see it: the behavior alone, with its context.
/// Behaviour: server.creatures.idle-behaviors
#[test]
fn a_creature_at_rest_plays_an_idle_behavior_now_and_then() {
    let (mut h, b, beetle) = resting(&[("AI_AISuperClass", Prop::Word(6))]);
    let next = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.next_idle)
        .expect("beetle");
    assert!(next.is_finite(), "it has idles");
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.next_idle = now;
    }
    let from = h.bots[b].inbox.len();
    h.run(4);
    let played: Vec<BehaviorParams> = h
        .world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::PlayBehavior(e) if e.sender.id == beetle => Some(e.action),
            _ => None,
        })
        .collect();
    assert_eq!(played.len(), 1, "one idle: {played:?}");
    assert!(IDLE_BEHAVIORS.contains(&played[0].behavior));
    assert_eq!(
        played[0].bits,
        BehaviorParams::BEHAVIOR | BehaviorParams::CONTEXT
    );
    let again = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.next_idle)
        .expect("beetle");
    assert!(again > h.now.0, "the next comes later");
}

/// A creature without a class of behavior never plays an idle, nor does a summoned one.
/// Behaviour: server.creatures.idle-behaviors
#[test]
fn a_creature_without_a_class_or_a_summoned_one_never_idles() {
    let (h, _, beetle) = resting(&[]);
    let next = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.next_idle)
        .expect("beetle");
    assert!(next.is_infinite(), "no idles without a class");
    let mut props = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .expect("beetle")
        .props
        .clone();
    props.set("AI_AISuperClass", Prop::Word(6));
    let idles = |p: &yalain_content::Props| {
        yalain_content::CreatureStats::from_props(p).is_some_and(|s| s.idles)
    };
    assert!(idles(&props), "a drudge's class idles");
    props.set(
        "AI_AISuperClass",
        Prop::Word(yalain_content::props::SUMMONED_CLASS),
    );
    assert!(!idles(&props), "a summoned creature's does not");
}

/// The mean of `n` gaps a creature draws to its next idle, each as it plays one.
fn mean_idle_gap(h: &mut Harness, beetle: u64, n: usize) -> f64 {
    let creature = |h: &mut Harness| {
        h.server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
            .expect("beetle")
            .next_idle
    };
    let (mut sum, mut got) = (0.0, 0);
    for _ in 0..n * 20 {
        let now = h.now.0;
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.next_idle = now;
            c.next_wander = f64::INFINITY;
        }
        h.run(1);
        let next = creature(h);
        if next > now {
            sum += next - h.now.0;
            got += 1;
            if got == n {
                break;
            }
        }
    }
    assert_eq!(got, n, "played its idles");
    #[allow(clippy::cast_precision_loss)]
    let mean = sum / n as f64;
    mean
}

/// A creature of a class the world gives an idle gap waits that long between its idles on average;
/// one of a class it gives none waits the server's own gap.
/// Behaviour: server.creatures.idle-rate-by-class
#[test]
fn a_creature_idles_at_its_classs_gap_from_the_world_and_otherwise_at_the_servers() {
    for (class, row, expected) in [(6, Some(60.0), 60.0_f32), (0x1D, None, ai::IDLE_EVERY)] {
        let (mut h, _, beetle) = resting(&[("AI_AISuperClass", Prop::Word(class))]);
        h.server.creature_rows.idle_every.clear();
        if let Some(gap) = row {
            h.server.creature_rows.idle_every.insert(class, gap);
        }
        h.server.creature_rows.idle_every.insert(0x7F, 9000.0);
        let mean = mean_idle_gap(&mut h, beetle, 400);
        let wanted = f64::from(expected);
        assert!(
            (mean - wanted).abs() < wanted * 0.2,
            "class {class:#x}: a mean gap of {mean:.1} s, not about {expected}"
        );
        assert!(
            (ai::idle_every(&h.server, TEST_CREATURE) - expected).abs() < 1e-3,
            "class {class:#x}"
        );
    }
}
