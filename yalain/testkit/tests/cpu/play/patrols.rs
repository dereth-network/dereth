//! Creatures that walk a route at rest, and the movement settings a camp sets over its creatures'
//! templates. Fixture: the test world's beetle camp, its creature made after the camp is given a
//! route or settings, with a player in the world beside it.

use yalain_content::movement::{Gait, MovementOverrides, MovementType, Shape, Waypoint};
use yalain_content::{CampMoves, Prop, Route};
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::ai;
use yalain_world::things::Mode;
use yalain_world::world::IDLE_MOTION;

/// An outdoor place far from the training hall.
const OUT_CELL: u32 = 0xA9B4_0017;
const OUT: [f32; 3] = [80.0, 80.0, 10.0];

/// The test camp's own cell and point (indoors).
const IN_CELL: u32 = 0x0299_0109;
const IN: [f32; 3] = [65.0, 240.0, -44.6];

fn waypoint(cell: u32, at: [f32; 3], rest: f64) -> Waypoint {
    Waypoint { cell, at, rest }
}

fn route(shape: Shape, gait: Gait, waypoints: Vec<Waypoint>) -> Route {
    Route {
        key: "rounds".into(),
        shape,
        gait,
        speed: None,
        waypoints,
    }
}

/// A world whose camp makes one beetle at `start` (a cell and a point) moving as `moves` says; the
/// beetle never attacks first. Template properties are set before the camp makes it. Returns the
/// harness, the client and the beetle.
fn camp_with(
    start: (u32, [f32; 3]),
    moves: CampMoves,
    props: &[(&str, Prop)],
) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let mut t = h
        .server
        .catalog
        .get(TEST_CREATURE)
        .cloned()
        .expect("beetle");
    for (name, value) in props {
        t.props.set(name, value.clone());
    }
    h.server.catalog.insert(t);
    let camp = &mut h.server.spawns.camps[0];
    camp.cell = start.0;
    camp.origin = start.1;
    camp.spots = vec![start];
    camp.moves = moves;
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

fn patrolling(start: (u32, [f32; 3]), r: Route, props: &[(&str, Prop)]) -> (Harness, usize, u64) {
    camp_with(
        start,
        CampMoves {
            route: Some(r),
            ..CampMoves::default()
        },
        props,
    )
}

fn place_of(h: &Harness, id: u64) -> StoredPosition {
    h.server.things.get(id).and_then(|t| t.at).expect("placed")
}

fn flat(a: &StoredPosition, b: [f32; 3]) -> f32 {
    ((a.origin[0] - b[0]).powi(2) + (a.origin[1] - b[1]).powi(2)).sqrt()
}

fn motion(h: &Harness, id: u64) -> u32 {
    h.server.things.get(id).expect("placed").heading_motion & 0x00FF_FFFF
}

/// The waypoint the beetle is going to.
fn going_to(h: &Harness, id: u64) -> usize {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.patrol.as_ref())
        .map(|p| p.to)
        .expect("on a route")
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).round() as usize;
    n
}

/// A creature whose camp names a route walks to each waypoint in turn, stands at it as long as
/// the waypoint's rest, and turns back at the end of a back-and-forth route.
/// Behaviour: server.creatures.patrol-walks-its-route
#[test]
fn a_creature_on_a_route_walks_to_each_waypoint_rests_there_and_turns_back_at_the_end() {
    let a = OUT;
    let b = [OUT[0], OUT[1] + 12.0, OUT[2]];
    let (mut h, _, beetle) = patrolling(
        (OUT_CELL, a),
        route(
            Shape::BackAndForth,
            Gait::Walk,
            vec![waypoint(OUT_CELL, a, 6.0), waypoint(OUT_CELL, b, 4.0)],
        ),
        &[],
    );
    // It walks to the second, stands there and rests 4 s, walks back to the first and rests 6 s
    // there, and sets off for the second again.
    assert!(
        h.run_until(steps(20.0), |h| going_to(h, beetle) == 1
            && flat(&place_of(h, beetle), a) > 0.1),
        "it sets off for the second"
    );
    assert_eq!(motion(&h, beetle), ai::PATROL_WALKING, "at a walk");
    assert!(
        h.run_until(steps(15.0), |h| going_to(h, beetle) == 0),
        "it reaches the second"
    );
    assert!(flat(&place_of(&h, beetle), b) <= ai::ARRIVED);
    assert_eq!(motion(&h, beetle), IDLE_MOTION, "it stands there");
    let there = place_of(&h, beetle);
    h.run(steps(3.5));
    assert_eq!(place_of(&h, beetle).origin, there.origin, "it rests 4 s");
    assert!(
        h.run_until(steps(1.0), |h| place_of(h, beetle).origin != there.origin),
        "then sets off"
    );
    assert!(
        h.run_until(steps(15.0), |h| going_to(h, beetle) == 1),
        "and walks back to the first"
    );
    assert!(flat(&place_of(&h, beetle), a) <= ai::ARRIVED);
    let there = place_of(&h, beetle);
    h.run(steps(5.5));
    assert_eq!(place_of(&h, beetle).origin, there.origin, "it rests 6 s");
    assert!(
        h.run_until(steps(1.0), |h| place_of(h, beetle).origin != there.origin),
        "then sets off again"
    );
}

/// On a loop the creature goes on from the last waypoint to the first; a route walked at a run
/// is run; a camp may start its creatures toward any waypoint, either way round.
/// Behaviour: server.creatures.patrol-walks-its-route
#[test]
fn a_loop_goes_on_from_the_last_waypoint_to_the_first_and_may_start_anywhere_either_way() {
    let corners = [
        OUT,
        [OUT[0], OUT[1] + 10.0, OUT[2]],
        [OUT[0] + 10.0, OUT[1] + 10.0, OUT[2]],
        [OUT[0] + 10.0, OUT[1], OUT[2]],
    ];
    let r = route(
        Shape::Loop,
        Gait::Run,
        corners
            .iter()
            .map(|c| waypoint(OUT_CELL, *c, 0.0))
            .collect(),
    );
    let (mut h, _, beetle) = camp_with(
        (OUT_CELL, corners[0]),
        CampMoves {
            route: Some(r),
            start: 2,
            backward: true,
            ..CampMoves::default()
        },
        &[],
    );
    let mut order = vec![going_to(&h, beetle)];
    let mut ran = false;
    for _ in 0..steps(30.0) {
        h.step();
        ran |= motion(&h, beetle) == ai::RUNNING;
        let to = going_to(&h, beetle);
        if order.last() != Some(&to) {
            order.push(to);
        }
    }
    assert!(ran, "it runs");
    assert!(order.len() >= 6, "{order:?}");
    assert_eq!(order[..6], [2, 1, 0, 3, 2, 1], "the way back round");
}

/// A route is walked indoors, where a creature at rest otherwise stands, and by a creature whose
/// template says it does not wander.
/// Behaviour: server.creatures.patrol-walks-indoors
#[test]
fn a_route_is_walked_indoors_and_by_a_creature_that_does_not_wander() {
    let far = [IN[0], IN[1] + 8.0, IN[2]];
    let (mut h, _, beetle) = patrolling(
        (IN_CELL, IN),
        route(
            Shape::BackAndForth,
            Gait::Walk,
            vec![waypoint(IN_CELL, IN, 0.0), waypoint(IN_CELL, far, 0.0)],
        ),
        &[("AI_Wandering", Prop::Bool(false))],
    );
    assert!(
        h.run_until(steps(10.0), |h| flat(&place_of(h, beetle), far)
            <= ai::ARRIVED),
        "it walks its route indoors"
    );
}

/// A creature that never moves, or only turns, stays where it is whatever route its camp names.
/// Behaviour: server.creatures.camp-movement-settings
#[test]
fn a_creature_that_never_moves_or_only_turns_keeps_to_its_place_on_a_route() {
    for kind in [MovementType::Stationary, MovementType::TurnOnly] {
        let far = [OUT[0] + 8.0, OUT[1], OUT[2]];
        let (mut h, _, beetle) = camp_with(
            (OUT_CELL, OUT),
            CampMoves {
                settings: MovementOverrides {
                    kind: Some(kind),
                    ..MovementOverrides::default()
                },
                route: Some(route(
                    Shape::BackAndForth,
                    Gait::Walk,
                    vec![waypoint(OUT_CELL, far, 0.0), waypoint(OUT_CELL, OUT, 0.0)],
                )),
                ..CampMoves::default()
            },
            &[],
        );
        h.run(steps(10.0));
        assert!(flat(&place_of(&h, beetle), OUT) < 0.01, "{kind:?} stays");
    }
}

/// A camp's settings stand over its creatures' template's: a camp may make them stationary (they
/// then neither move nor fight), stop or start their wandering, and give them another perception.
/// Behaviour: server.creatures.camp-movement-settings
#[test]
fn a_camps_movement_settings_stand_over_its_creatures_templates() {
    let settings = |o: MovementOverrides| CampMoves {
        settings: o,
        ..CampMoves::default()
    };
    let (h, _, beetle) = camp_with(
        (OUT_CELL, OUT),
        settings(MovementOverrides {
            kind: Some(MovementType::Stationary),
            perception: Some(7.5),
            ..MovementOverrides::default()
        }),
        &[],
    );
    let stats = &h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .expect("beetle")
        .stats;
    assert!(!stats.fights, "a stationary creature never fights");
    assert!((stats.movement.perception - 7.5).abs() < 1e-6);
    assert_eq!(stats.movement.wander_range, 25.0, "the scripts' default");

    // A template that does not wander, made to by its camp, wanders.
    let (mut h, _, beetle) = camp_with(
        (OUT_CELL, OUT),
        settings(MovementOverrides {
            wanders: Some(true),
            wander_chance: Some(1.0),
            ..MovementOverrides::default()
        }),
        &[("AI_Wandering", Prop::Bool(false))],
    );
    let now = h.now.0;
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.next_wander = now;
    }
    assert!(
        h.run_until(steps(4.0), |h| flat(&place_of(h, beetle), OUT) > 0.1),
        "it wanders"
    );

    // A wandering template stopped by its camp never does.
    let (mut h, _, beetle) = camp_with(
        (OUT_CELL, OUT),
        settings(MovementOverrides {
            wanders: Some(false),
            ..MovementOverrides::default()
        }),
        &[],
    );
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
    assert!(flat(&place_of(&h, beetle), OUT) < 0.01, "never moved");
}

/// A creature free of its home goes after a player however far from home the fight takes it; one
/// kept near home gives up past its distance from home.
/// Behaviour: server.creatures.free-range-no-home-leash
#[test]
fn a_free_range_creature_is_never_called_home_by_its_distance_from_it() {
    for (kind, gives_up) in [
        (MovementType::FreeRange, false),
        (MovementType::ConstrainedToHome, true),
    ] {
        let (mut h, _, beetle) = camp_with(
            (OUT_CELL, OUT),
            CampMoves {
                settings: MovementOverrides {
                    kind: Some(kind),
                    ..MovementOverrides::default()
                },
                ..CampMoves::default()
            },
            &[],
        );
        let player = h
            .server
            .players
            .values()
            .next()
            .map(|p| p.character)
            .expect("a player");
        let far = StoredPosition {
            cell: OUT_CELL,
            origin: [OUT[0] + ai::MAX_FROM_HOME + 10.0, OUT[1], OUT[2]],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        if let Some(t) = h.server.things.get_mut(beetle) {
            t.at = Some(far);
            if let Some(c) = t.creature.as_mut() {
                c.mode = Mode::Chasing;
                c.target = Some(player);
            }
        }
        for p in h.server.players.values_mut() {
            p.position = StoredPosition {
                origin: [far.origin[0] + 5.0, far.origin[1], far.origin[2]],
                ..far
            };
        }
        h.run(10);
        let mode = h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.mode)
            .expect("beetle");
        assert_eq!(mode == Mode::Returning, gives_up, "{kind:?}: {mode:?}");
    }
}

/// A creature on a route that gives up a fight runs back to the route's nearest waypoint, not to
/// where its camp made it, and carries on along the route from there.
/// Behaviour: server.creatures.patrol-returns-to-nearest-waypoint
#[test]
fn a_creature_on_a_route_that_gives_up_a_fight_goes_back_to_the_nearest_waypoint() {
    let ends = [
        OUT,
        [OUT[0], OUT[1] + 30.0, OUT[2]],
        [OUT[0], OUT[1] + 60.0, OUT[2]],
    ];
    let (mut h, _, beetle) = patrolling(
        (OUT_CELL, ends[0]),
        route(
            Shape::BackAndForth,
            Gait::Walk,
            ends.iter().map(|e| waypoint(OUT_CELL, *e, 2.0)).collect(),
        ),
        &[],
    );
    let player = h
        .server
        .players
        .values()
        .next()
        .map(|p| p.character)
        .expect("a player");
    // Fighting 6 m off the route beside its last waypoint, the player gone far beyond it.
    let fought = StoredPosition {
        cell: OUT_CELL,
        origin: [OUT[0] + 6.0, OUT[1] + 55.0, OUT[2]],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(fought);
        if let Some(c) = t.creature.as_mut() {
            c.mode = Mode::Chasing;
            c.target = Some(player);
        }
    }
    for p in h.server.players.values_mut() {
        p.position = StoredPosition {
            origin: [OUT[0] + 70.0, OUT[1] + 55.0, OUT[2]],
            ..fought
        };
    }
    let mode = |h: &Harness| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.mode)
            .expect("beetle")
    };
    assert!(
        h.run_until(steps(3.0), |h| mode(h) == Mode::Returning),
        "it gives up"
    );
    assert_eq!(going_to(&h, beetle), 2, "toward the nearest waypoint");
    assert!(
        h.run_until(steps(1.0), |h| motion(h, beetle) == ai::RUNNING),
        "running back"
    );
    assert!(
        h.run_until(steps(10.0), |h| mode(h) == Mode::Idle),
        "it gets back"
    );
    assert!(
        flat(&place_of(&h, beetle), ends[2]) <= ai::ARRIVED,
        "at the nearest waypoint"
    );
    assert!(
        h.run_until(steps(30.0), |h| flat(&place_of(h, beetle), ends[1])
            <= ai::ARRIVED),
        "and walks on along its route"
    );
}
