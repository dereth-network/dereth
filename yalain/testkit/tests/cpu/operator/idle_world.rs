//! An idle world's step: camps looked at only when they have something to do, creatures at rest
//! asleep where no player is near, woken by a player coming near or by anything changing them.
//! Fixture: the harness's world with walls known but empty (as a running server knows them),
//! camps of one beetle each on open ground far from the start, and the test camp beside the start.
//! The work is counted (camps looked at, creatures acted), never timed.

use std::sync::Arc;

use auberean_physics::land::Land;
use auberean_physics::world::World;
use yalain_content::Camp;
use yalain_testkit::{test_camp, Harness};
use yalain_world::things::Mode;
use yalain_world::walls::{Blocks, Walls};

/// Walls that hold no block: known, so creatures no player could see rest, but nothing stands in
/// anyone's way.
struct NoBlocks;

impl Blocks for NoBlocks {
    fn load(&mut self, _: &Land, _: u8, _: u8) -> bool {
        false
    }
}

/// A landblock on open ground far from the start.
const FAR: u32 = 0xA9B4;

/// A camp of one beetle on open ground in landblock `FAR + k`, far from the start and from the
/// test camp.
fn far_camp(k: u32) -> Camp {
    let mut c = test_camp(1);
    c.id = format!("far{k}");
    c.cell = ((FAR + k) << 16) | 0x17;
    c.origin = [80.0, 80.0, 10.0];
    c.spots = Vec::new();
    c
}

/// A world with walls known and `n` far camps, and the test camp beside the start when `beside`.
fn world(n: u32, beside: bool) -> Harness {
    let mut h = Harness::new();
    h.add_creatures(1);
    if !beside {
        h.server.spawns.camps.clear();
        h.server.camps.clear();
    }
    for k in 0..n {
        h.server.spawns.camps.push(far_camp(k));
        h.server
            .camps
            .push(yalain_world::spawn::CampState::default());
    }
    h.server.walls = Some(Walls::new(
        Arc::new(Land::new()),
        World::new(),
        Box::new(NoBlocks),
    ));
    h
}

/// One world step on.
fn world_step(h: &mut Harness) {
    h.step_by(yalain_world::spawn::STEP_SECONDS);
}

/// The camps looked at and the creatures acted at the last world step.
fn work(h: &Harness) -> (usize, usize) {
    (
        h.server.camp_watch.looked_at,
        h.server.step_timer.figures().creatures,
    )
}

/// The beetles of the world, by id.
fn beetles(h: &Harness) -> Vec<u64> {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.creature.is_some())
        .map(|t| t.id)
        .collect()
}

/// A world with no player in it does the same work at each step however many camps it has: after
/// the first step, which fills every camp, none is looked at and no creature acts, and every
/// creature sleeps.
/// Behaviour: server.camps.looked-at-only-with-something-to-do
#[test]
fn an_idle_worlds_step_does_no_more_work_with_forty_camps_than_with_one() {
    let mut idle = Vec::new();
    for n in [1, 40] {
        let mut h = world(n, false);
        world_step(&mut h);
        assert_eq!(
            work(&h),
            (n as usize, n as usize),
            "the first step fills them"
        );
        let mut most = (0, 0);
        for _ in 0..400 {
            world_step(&mut h);
            let (c, a) = work(&h);
            most = (most.0.max(c), most.1.max(a));
        }
        assert_eq!(beetles(&h).len(), n as usize, "{n} camps keep theirs");
        assert_eq!(h.server.things.asleep_count(), n as usize, "all asleep");
        idle.push(most);
    }
    assert_eq!(idle, vec![(0, 0), (0, 0)], "the most work of any idle step");
}

/// A camp far from every player replaces what it lost as one looked at every step would: a beetle
/// gone is noticed at the next step, and its replacement made at the step its period comes to,
/// with nothing looked at between.
/// Behaviour: server.camps.looked-at-only-with-something-to-do
#[test]
fn a_camp_far_from_every_player_notices_a_loss_at_once_and_replaces_it_when_due() {
    let mut h = world(3, false);
    for _ in 0..4 {
        world_step(&mut h);
    }
    let gone = h.server.camps[1].alive[0];
    yalain_world::death::vanish(&mut h.server, &mut h.net, gone);
    world_step(&mut h);
    assert_eq!(work(&h).0, 1, "the camp that lost it");
    let due = h.server.camps[1].due.clone();
    assert_eq!(due.len(), 1);
    let between = (0..)
        .map_while(|_| {
            world_step(&mut h);
            h.server.camps[1].alive.is_empty().then(|| work(&h).0)
        })
        .take(1000)
        .collect::<Vec<_>>();
    assert!(
        between.iter().all(|c| *c == 0),
        "nothing looked at: {between:?}"
    );
    assert_eq!(work(&h).0, 1, "looked at as it came due");
    assert_eq!(h.server.camps[1].alive.len(), 1, "replaced");
    let made_at = h.server.last_step.map(|t| t.0).unwrap_or_default();
    assert!(
        made_at >= due[0] && made_at - due[0] < yalain_world::spawn::STEP_SECONDS + 1e-9,
        "made at {made_at}, due {}",
        due[0]
    );
}

/// A camp far from every player makes the replacement for a creature that died when it is due,
/// though the body still lies where it fell.
/// Behaviour: server.camps.looked-at-only-with-something-to-do
#[test]
fn a_camp_far_from_every_player_replaces_its_dead_when_due_with_the_body_still_lying() {
    let mut h = world(2, false);
    h.server.spawns.camps[1].regen = 2.0;
    for _ in 0..4 {
        world_step(&mut h);
    }
    let dead = h.server.camps[1].alive[0];
    let now = h.now.0;
    yalain_world::spawn::died(&mut h.server, 1, dead, now);
    let replaced = h.run_until(400, |h| h.server.camps[1].alive.len() == 1);
    assert!(replaced, "a new beetle");
    assert!(h.server.things.get(dead).is_some(), "the body still lies");
    assert!(h.now.0 - now < 2.0 + 2.0 * yalain_world::spawn::STEP_SECONDS + 1e-9);
}

/// A camp's period runs from when its dead creature's body leaves the world, not from the death:
/// the replacement comes its period after the body has gone.
/// Behaviour: server.camps.period-from-the-body-leaving
#[test]
fn a_camps_replacement_comes_its_period_after_the_dead_body_leaves() {
    let mut h = yalain_testkit::Harness::new();
    h.add_creatures(1);
    h.server.spawns.camps[0].regen = 20.0;
    let (_, me) = h.player_in_world("one", "Elara");
    h.run(20);
    let dead = h.server.camps[0].alive[0];
    let died = h.now.0;
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, dead, me, clock);
    assert!(
        h.run_until(400, |h| h.server.things.get(dead).is_none()),
        "the body leaves"
    );
    let left = h.now.0;
    assert!(
        left - died > 4.0,
        "a few seconds after the death: {}",
        left - died
    );
    assert!(
        h.run_until(800, |h| h.server.camps[0].alive.len() == 1),
        "a new beetle"
    );
    let after = h.now.0 - left;
    assert!(
        (20.0..20.0 + yalain_world::spawn::STEP_SECONDS + 0.1).contains(&after),
        "replaced {after} s after the body left"
    );
}

/// The creatures near a player who comes into the world wake and act, and the one beside the start
/// goes after the player as it does; those far away sleep on.
/// Behaviour: server.creatures.asleep-where-no-player-is-near
#[test]
fn a_player_coming_near_wakes_the_creatures_there_and_only_those() {
    let mut h = world(5, true);
    for _ in 0..8 {
        world_step(&mut h);
    }
    assert_eq!(work(&h), (0, 0), "an idle world");
    assert_eq!(h.server.things.asleep_count(), 6);
    let beside = h.server.camps[0].alive[0];
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    world_step(&mut h);
    assert_eq!(work(&h).1, 1, "the beetle beside the start acts");
    assert_eq!(h.server.things.asleep_count(), 5, "the far ones sleep on");
    assert!(!h.server.things.is_asleep(beside));
    let goes_after = h.run_until(200, |h| {
        h.server
            .things
            .get(beside)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.mode == Mode::Chasing && c.target == Some(me))
    });
    assert!(goes_after, "it goes after the player");
}

/// A creature asleep that something changes wakes: a hurt one regains its health as any hurt
/// creature does, and sleeps again once whole.
/// Behaviour: server.creatures.asleep-where-no-player-is-near
#[test]
fn a_creature_asleep_that_is_hurt_wakes_and_regains_its_health() {
    let mut h = world(1, false);
    for _ in 0..4 {
        world_step(&mut h);
    }
    let beetle = beetles(&h)[0];
    assert!(h.server.things.is_asleep(beetle));
    let now = h.now.0;
    let full = {
        let t = h.server.things.get_mut(beetle).expect("the beetle");
        t.health = t.max_health / 2;
        t.last_hurt = now;
        t.max_health
    };
    assert!(!h.server.things.is_asleep(beetle), "woken by the hurt");
    let whole = h.run_until(4000, |h| {
        h.server
            .things
            .get(beetle)
            .is_some_and(|t| t.health == full)
    });
    assert!(whole, "it regains its health");
    // Whole within the step under way; its regaining is put to rest at the next, and it sleeps at
    // the one after.
    for _ in 0..3 {
        world_step(&mut h);
    }
    assert!(h.server.things.is_asleep(beetle), "and sleeps again");
    // Asleep, its regaining is put to rest as an awake creature's is: given more health to have
    // (as an effect would), it regains it a share at a time from then, not at once.
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.max_health += 20;
    }
    world_step(&mut h);
    world_step(&mut h);
    let t = h.server.things.get(beetle).expect("the beetle");
    assert!(
        t.health <= full + 1,
        "{} of {} after half a second",
        t.health,
        t.max_health
    );
}

/// Woken, a creature looks about at its own moment of the second as it did before it slept, its
/// next idle behavior and its next thought of wandering come after fresh waits from the moment it
/// woke (the thought within the longest rest between walks), and no player counts as within its
/// perception since its last step.
/// Behaviour: server.creatures.woken-take-up-their-clocks
#[test]
fn a_creature_woken_looks_about_at_its_own_moment_and_its_waits_start_afresh() {
    let mut h = world(1, false);
    for _ in 0..4 {
        world_step(&mut h);
    }
    let beetle = beetles(&h)[0];
    assert!(h.server.things.is_asleep(beetle));
    // Ten minutes asleep, every clock of its behind.
    let now = h.now.0 + 600.0;
    let phase = 0.375;
    let at = {
        // Read and set without waking it.
        let t = h.server.things.by_id.get_mut(&beetle).expect("the beetle");
        let c = t.creature.as_mut().expect("a creature");
        c.next_look = now - 3.0 - (1.0 - phase);
        c.next_idle = now - 5.0;
        c.next_wander = now - 7.0;
        c.in_sight = vec![7];
        t.at.expect("placed")
    };
    let near = std::collections::BTreeSet::from([yalain_world::rest::block(&at)]);
    yalain_world::rest::wake(&mut h.server, &near, now);
    assert!(!h.server.things.is_asleep(beetle));
    let c = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.creature.as_ref())
        .expect("the beetle");
    assert!(
        (c.next_look.rem_euclid(1.0) - phase).abs() < 1e-9,
        "its own moment: {}",
        c.next_look
    );
    assert!(
        c.next_look >= now && c.next_look < now + 1.0,
        "{}",
        c.next_look
    );
    assert!(c.next_idle >= now, "idle at {}", c.next_idle);
    let (_, longest) = yalain_world::ai::WANDER_REST;
    assert!(
        c.next_wander >= now && c.next_wander <= now + f64::from(longest),
        "wander at {}",
        c.next_wander
    );
    assert!(c.in_sight.is_empty());
}

/// Put the player with this character in landblock `block`, at the middle of its outdoor cell 0x17.
fn put_player(h: &mut Harness, me: u64, block: u32) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("the player");
    p.position.cell = (block << 16) | 0x17;
    p.position.origin = [80.0, 80.0, 10.0];
}

/// A player in the landblock beside a creature's place or its home wakes it, as one in its own
/// does; one two landblocks off does not.
/// Behaviour: server.creatures.asleep-where-no-player-is-near
#[test]
fn a_player_in_the_landblock_beside_a_creatures_place_or_home_keeps_it_awake() {
    let mut h = world(1, false);
    let (_b, me) = h.player_in_world("one", "Elara");
    for _ in 0..4 {
        world_step(&mut h);
    }
    let beetle = beetles(&h)[0];
    assert!(h.server.things.is_asleep(beetle), "the start is far off");
    put_player(&mut h, me, FAR + 2);
    world_step(&mut h);
    world_step(&mut h);
    assert!(h.server.things.is_asleep(beetle), "two landblocks off");
    put_player(&mut h, me, FAR + 0x200);
    world_step(&mut h);
    world_step(&mut h);
    assert!(
        h.server.things.is_asleep(beetle),
        "two landblocks off the other way"
    );
    put_player(&mut h, me, FAR + 0x100);
    world_step(&mut h);
    assert!(
        !h.server.things.is_asleep(beetle),
        "beside its place the other way"
    );
    put_player(&mut h, me, FAR + 1);
    world_step(&mut h);
    assert!(!h.server.things.is_asleep(beetle), "beside its place");
    assert_eq!(work(&h).1, 1, "it acts");
    // Its home two landblocks on from its place: a player beside the home, three from the place,
    // keeps it awake.
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.home.cell = ((FAR + 2) << 16) | 0x17;
    }
    put_player(&mut h, me, FAR + 3);
    for _ in 0..4 {
        world_step(&mut h);
    }
    assert!(!h.server.things.is_asleep(beetle), "beside its home");
    put_player(&mut h, me, FAR + 5);
    for _ in 0..4 {
        world_step(&mut h);
    }
    assert!(h.server.things.is_asleep(beetle), "far from both");
}
