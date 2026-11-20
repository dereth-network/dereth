//! A creature's special whose skill has an area: every player and pet within its radius of the
//! target is named in the special's one story with its own outcome, and each it lands on loses the
//! health its hook shows; a player beyond is left alone. Fixture: the test camp's beetles, one the
//! creature and one a pet, and three players placed around the target.

use auberean_protocol::types::StoryHook;
use auberean_protocol::{queue, ServerMessage};
use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::missiles::{AttackSkill, Flight, CREATURE_ATTACKS};
use yalain_world::player::quality;
use yalain_world::quals;

const HIT: u32 = 0x4000_0008;
const CRITICAL: u32 = 0x4000_0009;

/// The players, the creature and its target's neighbors as a test sets them out.
struct Field {
    h: Harness,
    /// The target's client, for what it was told.
    b: usize,
    creature: u64,
    pet: u64,
    target: (ConnId, u64),
    near: (ConnId, u64),
    far: (ConnId, u64),
}

/// The target with the scene drawn, another player 5 m from it and a third 20 m off, a pet of the
/// second player 3 m from the target, and the creature 2 m away; none fights on its own, and all
/// have health to spare.
fn field() -> Field {
    let mut h = Harness::new();
    h.add_creatures(2);
    let (b, me) = h.player_in_world("one", "Elara");
    let (_, near) = h.player_in_world("two", "Babyfare");
    let (_, far) = h.player_in_world("three", "Orrin");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let beetles: Vec<u64> = h
        .server
        .things
        .placed()
        .filter(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .collect();
    let (creature, pet) = (beetles[0], beetles[1]);
    for id in &beetles {
        if let Some(t) = h.server.things.get_mut(*id) {
            t.max_health = 100_000;
            t.health = 100_000;
            if let Some(c) = t.creature.as_mut() {
                c.stats.fights = false;
            }
        }
    }
    h.server.pets.by_id.insert(
        pet,
        yalain_world::pets::Pet {
            id: pet,
            owner: near,
            template: TEST_CREATURE,
            class: 0,
            flags: 0,
            mode: 0,
            name: None,
            order: yalain_world::pets::Order::Follow,
            next_plan: f64::MAX,
            goal: None,
            effect: 0,
            assist: None,
            next_tend: f64::MAX,
            asked_tend: false,
            asked_buff: false,
            missed: false,
            closing: None,
            vigor: None,
            turn_to: None,
        },
    );
    let conn = |h: &Harness, id: u64| {
        *h.server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .expect("in the world")
            .0
    };
    let at = h.server.players[&conn(&h, me)].position;
    let moved = |dx: f32| StoredPosition {
        origin: [at.origin[0] + dx, at.origin[1], at.origin[2]],
        ..at
    };
    for (id, dx) in [(near, 5.0), (far, 20.0)] {
        let c = conn(&h, id);
        if let Some(p) = h.server.players.get_mut(&c) {
            p.position = moved(dx);
        }
    }
    for (id, dx) in [(pet, -3.0), (creature, 2.0)] {
        if let Some(t) = h.server.things.get_mut(id) {
            t.at = Some(moved(dx));
        }
    }
    for p in h.server.players.values_mut() {
        quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 100_000);
        quals::set_int(&mut p.qualities, quality::HEALTH, 100_000);
    }
    Field {
        target: (conn(&h, me), me),
        near: (conn(&h, near), near),
        far: (conn(&h, far), far),
        h,
        b,
        creature,
        pet,
    }
}

fn health(f: &Field, who: (ConnId, u64)) -> i32 {
    quals::int(&f.h.server.players[&who.0].qualities, quality::HEALTH).unwrap_or(0)
}

/// The strike hooks of the creature's story of `skill` its target was told since `from`, whoever sent
/// it.
fn hooks(f: &Field, from: usize, skill: u32) -> Vec<StoryHook> {
    f.h.world_messages(f.b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.skill == skill => Some(e.action.hooks.clone()),
            _ => None,
        })
        .flatten()
        .filter(|k| k.target != 0)
        .collect()
}

/// Something landed, and each player and the pet lost the health its landed hooks show, all told.
fn each_landed_hook_took_its_health(
    f: &Field,
    hooks: &[StoryHook],
    before: &[((ConnId, u64), i32)],
) {
    let landed = |k: &&StoryHook| [HIT, CRITICAL].contains(&k.numbers[0].cast_unsigned());
    assert!(
        hooks.iter().any(|k| landed(&k)),
        "something landed: {hooks:?}"
    );
    let shown = |id: u64| -> i32 {
        hooks
            .iter()
            .filter(landed)
            .filter(|k| k.target == id)
            .map(|k| -k.numbers[3])
            .sum()
    };
    for (who, was) in before {
        assert_eq!(was - health(f, *who), shown(who.1), "player {:#x}", who.1);
    }
    let pet_lost = 100_000 - f.h.server.things.get(f.pet).map_or(0, |t| t.health);
    assert_eq!(pet_lost, shown(f.pet), "the pet");
}

/// A creature's special strike with an area of 12.5 m on its target reaches the player 5 m from it
/// and that player's pet 3 m from it, each with its own outcome in the one story, and each it lands
/// on loses the health shown; the player 20 m off is not named and loses nothing. The same special
/// without an area names its target alone.
/// Behaviour: server.creatures.area-specials-reach-everyone-in-their-radius
#[test]
fn a_creatures_area_special_strikes_every_player_and_pet_within_its_radius() {
    let mut f = field();
    let strong = CREATURE_ATTACKS[1];
    let blast = AttackSkill {
        radius: 12.5,
        ..strong
    };
    let before = [
        (f.target, health(&f, f.target)),
        (f.near, health(&f, f.near)),
        (f.far, health(&f, f.far)),
    ];
    let from = f.h.bots[f.b].inbox.len();
    let clock = f.h.clock();
    yalain_world::combat::creature_strike_with(
        &mut f.h.server,
        &mut f.h.net,
        f.creature,
        f.target.0,
        blast,
        true,
        clock,
    );
    f.h.run(100);
    let told = hooks(&f, from, strong.skill);
    let mut named: Vec<u64> = told.iter().map(|k| k.target).collect();
    named.sort_unstable();
    let mut want = vec![f.target.1, f.near.1, f.pet];
    want.sort_unstable();
    assert_eq!(
        named, want,
        "the target, the player and the pet within the radius"
    );
    each_landed_hook_took_its_health(&f, &told, &before);
    assert_eq!(
        health(&f, f.far),
        before[2].1,
        "the far player is left alone"
    );

    let from = f.h.bots[f.b].inbox.len();
    let clock = f.h.clock();
    yalain_world::combat::creature_strike_with(
        &mut f.h.server,
        &mut f.h.net,
        f.creature,
        f.target.0,
        strong,
        true,
        clock,
    );
    f.h.run(100);
    let named: Vec<u64> = hooks(&f, from, strong.skill)
        .iter()
        .map(|k| k.target)
        .collect();
    assert_eq!(named, [f.target.1], "without an area, the target alone");
}

/// A creature's special missile with an area arrives at its target and reaches the player 5 m from
/// it and the pet 3 m from it: each arrival is the target's one story naming all three, and each it
/// lands on loses the health shown; the player 20 m off is never named and loses nothing.
/// Behaviour: server.creatures.area-specials-reach-everyone-in-their-radius
#[test]
fn a_creatures_area_missile_reaches_every_player_and_pet_within_its_radius_where_it_lands() {
    let mut f = field();
    let poison = CREATURE_ATTACKS[7];
    let before = [
        (f.target, health(&f, f.target)),
        (f.near, health(&f, f.near)),
        (f.far, health(&f, f.far)),
    ];
    let from = f.h.bots[f.b].inbox.len();
    let aimed = f.h.server.players[&f.target.0].position;
    // Five arrive, so some land whatever the rolls.
    for _ in 0..5 {
        let clock = f.h.clock();
        yalain_world::missiles::arrives(
            &mut f.h.server,
            &mut f.h.net,
            Flight {
                creature: f.creature,
                target: f.target.1,
                skill: AttackSkill {
                    radius: 12.5,
                    ..poison
                },
                special: true,
                from: aimed,
                loosed_at: None,
            },
            clock,
        );
        f.h.run(10);
    }
    let told = hooks(&f, from, poison.skill);
    let mut want = vec![f.target.1, f.near.1, f.pet];
    want.sort_unstable();
    for arrival in told.chunks(3) {
        let mut named: Vec<u64> = arrival.iter().map(|k| k.target).collect();
        named.sort_unstable();
        assert_eq!(
            named, want,
            "each arrival names the three within the radius"
        );
    }
    assert_eq!(told.len(), 15, "five arrivals");
    each_landed_hook_took_its_health(&f, &told, &before);
    assert_eq!(
        health(&f, f.far),
        before[2].1,
        "the far player is left alone"
    );
}
