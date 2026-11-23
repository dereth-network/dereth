//! Whom a skill may be aimed at: a use at a target of a kind the skill's definition does not take
//! is refused with the client's own text for that kind (the user itself, a monster, a monster's
//! corpse, a player's corpse, a monster's item, the user's own pet, a fellow), and no skill is used
//! while swimming. A basic attack at a monster's corpse is refused as on monster corpses.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TestGround, INNER_CALM, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
use yalain_world::skill_use::{function, refusal, target_bit};

const BOW: u32 = 0x4700_051B;
const SWORD: u32 = 0x4700_051C;
const DEADEYE: u32 = 207;

/// A new player with the scene drawn beside the test camp's beetle, made a practice target that
/// never fights or moves; returns the client, the player and the beetle.
fn beside_a_beetle() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.fights = false;
    }
    (h, b, me, beetle)
}

fn conn(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// Use a skill on a target, as the client packs it; the refusal's code, if one came.
fn refusal_of(h: &mut Harness, b: usize, id: u32, skill: u32, target: u64) -> Option<u32> {
    h.all_hold(skill);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)],
        ),
    );
    h.run(10);
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == function::REFUSED => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .and_then(|v| match v.last() {
                Some(Value::Int(code)) => Some(*code),
                _ => None,
            }),
        _ => None,
    })
}

/// Deadeye's kinds of target set to `kinds`.
fn deadeye_takes(h: &mut Harness, kinds: u32) {
    h.server
        .skill_table
        .skills
        .get_mut(&DEADEYE)
        .and_then(|d| d.usage.as_mut())
        .expect("Deadeye's use")
        .valid_targets = kinds;
}

fn pet(id: u64, owner: u64) -> yalain_world::pets::Pet {
    yalain_world::pets::Pet {
        id,
        owner,
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
    }
}

/// A hostile skill (Deadeye, taking monsters, players and pets as retail's hostile skills do) is
/// refused at its user (on yourself), at the user's own pet (on your pet) and at a monster's corpse
/// (on monster corpses); another player's pet is no pet of its own. A skill taking players alone,
/// or the user's pets alone, is refused at a monster (on monsters).
/// Behaviour: server.skills.refused-at-a-kind-it-does-not-take
#[test]
fn a_skill_is_refused_at_a_target_of_a_kind_it_does_not_take() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    h.arm(b, me, BOW);
    deadeye_takes(&mut h, 0x0030_0980);
    assert_eq!(
        refusal_of(&mut h, b, 1, DEADEYE, me),
        Some(refusal::ON_YOURSELF)
    );
    h.server.pets.by_id.insert(beetle, pet(beetle, me));
    assert_eq!(
        refusal_of(&mut h, b, 2, DEADEYE, beetle),
        Some(refusal::ON_YOUR_PET)
    );
    h.server.pets.by_id.remove(&beetle);
    for (id, kinds) in [(3, 0x100), (4, target_bit::PET_ONLY)] {
        deadeye_takes(&mut h, kinds);
        assert_eq!(
            refusal_of(&mut h, b, id, DEADEYE, beetle),
            Some(refusal::ON_MONSTERS),
            "taking {kinds:#x}"
        );
    }
    // Another's pet: no pet of the user's (its owner is not in the world, so it may be fought).
    deadeye_takes(&mut h, 0x0030_0980);
    h.server.pets.by_id.insert(beetle, pet(beetle, me + 1));
    assert_eq!(refusal_of(&mut h, b, 5, DEADEYE, beetle), None);
    h.server.pets.by_id.remove(&beetle);
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, me, clock);
    h.run(100);
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.corpse
                .as_ref()
                .is_some_and(|c| c.creature == TEST_CREATURE)
        })
        .map(|t| t.id)
        .expect("the beetle's corpse");
    assert_eq!(
        refusal_of(&mut h, b, 6, DEADEYE, corpse),
        Some(refusal::ON_MONSTER_CORPSES)
    );
    // A skill that takes monster corpses goes on to what a corpse is: dead.
    deadeye_takes(&mut h, 0x0030_09C0);
    assert_eq!(
        refusal_of(&mut h, b, 7, DEADEYE, corpse),
        Some(refusal::TARGET_DEAD)
    );
}

/// A hostile skill that takes players outside the fellowship alone is refused at a fellow (within
/// your fellowship), before who may harm whom; at a player outside the fellowship, or with a skill
/// that does not say so, it goes on to who may harm whom.
/// Behaviour: server.skills.refused-within-the-fellowship
#[test]
fn a_hostile_skill_is_refused_at_a_fellow() {
    let (mut h, b, me, _) = beside_a_beetle();
    let (_, other) = h.player_in_world("two", "Bram");
    h.run(5);
    let at = h.server.players[&conn(&h, me)].position;
    if let Some(p) = h.server.players.get_mut(&conn(&h, other)) {
        p.position.origin = [at.origin[0] + 3.0, at.origin[1], at.origin[2]];
    }
    h.arm(b, me, BOW);
    deadeye_takes(&mut h, 0x0030_0980);
    let outside = refusal_of(&mut h, b, 1, DEADEYE, other);
    assert_ne!(outside, Some(refusal::WITHIN_FELLOWSHIP));
    h.server.fellowships.list.push(Fellowship {
        name: "The Band".into(),
        leader: me,
        flags: 0x1,
        room: 1,
        fellows: [me, other]
            .iter()
            .map(|id| Fellow {
                id: *id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::MAX,
            })
            .collect(),
        last_claimant: 0,
    });
    assert_eq!(
        refusal_of(&mut h, b, 2, DEADEYE, other),
        Some(refusal::WITHIN_FELLOWSHIP)
    );
    deadeye_takes(&mut h, 0x0030_0180);
    assert_eq!(refusal_of(&mut h, b, 3, DEADEYE, other), outside);
}

/// A player whose feet are 1.25 m or more under its block's water surface uses no skill (while
/// swimming); one wading less deep uses it.
/// Behaviour: server.skills.not-while-swimming
#[test]
fn no_skill_is_used_while_swimming() {
    const SURFACE: f32 = 70.0;
    let (mut h, b, me, _) = beside_a_beetle();
    h.server.ground = Some(Box::new(TestGround {
        land: Some(SURFACE - 30.0),
        water: Some(SURFACE),
        ..TestGround::default()
    }));
    h.arm(b, me, SWORD);
    if let Some(p) = h.server.players.get_mut(&conn(&h, me)) {
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::player::quality::VIGOR_MAX,
            400,
        );
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::player::quality::VIGOR, 400);
    }
    let (cell, x, y) = yalain_world::spawn::outdoor(0x0299, 80.0, 80.0);
    for (id, depth, want) in [(1, 1.5, Some(refusal::SWIMMING)), (2, 1.0, None)] {
        if let Some(p) = h.server.players.get_mut(&conn(&h, me)) {
            p.position = StoredPosition {
                cell,
                origin: [x, y, SURFACE - depth],
                rotation: [1.0, 0.0, 0.0, 0.0],
            };
        }
        assert_eq!(
            refusal_of(&mut h, b, id, INNER_CALM, me),
            want,
            "{depth} m under"
        );
        h.run(100);
    }
}

/// A skill that takes neither is refused at an item in a monster's corpse (on monsters' items)
/// and at a player's corpse (on the corpses of other players); one taking them goes on past these.
/// Behaviour: server.skills.refused-at-a-kind-it-does-not-take
#[test]
fn a_skill_is_refused_at_a_monsters_item_and_at_a_players_corpse() {
    let (mut h, b, me, beetle) = beside_a_beetle();
    h.arm(b, me, BOW);
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, me, clock);
    h.run(100);
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.corpse
                .as_ref()
                .is_some_and(|c| c.creature == TEST_CREATURE)
        })
        .map(|t| t.id)
        .expect("the beetle's corpse");
    let item = yalain_world::inventory::make_item(&mut h.server, yalain_testkit::TEST_ITEM, 1, 0)
        .expect("made");
    h.server.things.get_mut(item).expect("the item").container = Some(corpse);
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.contents.push(item);
    }
    deadeye_takes(&mut h, 0x0030_0980);
    assert_eq!(
        refusal_of(&mut h, b, 1, DEADEYE, item),
        Some(refusal::ON_MONSTERS_ITEMS)
    );
    deadeye_takes(&mut h, 0x0030_0980 | target_bit::MONSTER_ITEM);
    assert_ne!(
        refusal_of(&mut h, b, 2, DEADEYE, item),
        Some(refusal::ON_MONSTERS_ITEMS)
    );
    // The same corpse as a player's: one that left no creature behind.
    if let Some(c) = h
        .server
        .things
        .get_mut(corpse)
        .and_then(|t| t.corpse.as_mut())
    {
        c.creature = 0;
    }
    deadeye_takes(&mut h, 0x0030_0980);
    assert_eq!(
        refusal_of(&mut h, b, 3, DEADEYE, corpse),
        Some(refusal::ON_PLAYERS_CORPSES)
    );
    deadeye_takes(&mut h, 0x0030_0980 | target_bit::PLAYER_CORPSE);
    assert_ne!(
        refusal_of(&mut h, b, 4, DEADEYE, corpse),
        Some(refusal::ON_PLAYERS_CORPSES)
    );
}

/// What a client was sent since `from` of an attack's start, in the order the server sent it: the
/// attack state, by its value, and a failed attack, by its code.
fn attack_told(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    use yalain_world::combat::function as f;
    h.sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == f::ATTACK_STATE || c.function.0 == f::BASIC_ATTACKS_FAILED =>
            {
                let v = calls::values(&c.args.as_ref()?.0).ok()?;
                match v.first() {
                    Some(Value::Int(n)) => Some((c.function.0, *n)),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// A basic attack started at a creature dead but not yet a corpse is refused as at a dead target;
/// at a monster's corpse, as on monster corpses. Either way, in the order sent, the player is told
/// attacking, then the refusal, then not attacking.
/// Behaviour: server.player-fights.a-refused-start-is-told-begun-first
/// Behaviour: server.player-fights.a-basic-attack-at-a-corpse-is-refused-as-on-monster-corpses
#[test]
fn a_basic_attack_at_a_monsters_corpse_is_refused_as_on_monster_corpses_after_the_attack_state() {
    use yalain_world::combat::{error, function as f};
    let (mut h, b, me, beetle) = beside_a_beetle();
    h.arm(b, me, SWORD);
    h.run(5);
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.health = 0;
    }
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, beetle, me, clock);
    let start = |h: &mut Harness, target: u64| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(f::START_ATTACK, &[Arg::LongInt(target)]),
        );
        h.run(4);
        attack_told(h, b, from)
    };
    assert_eq!(
        start(&mut h, beetle),
        [
            (f::ATTACK_STATE, 1),
            (f::BASIC_ATTACKS_FAILED, error::TARGET_DEAD),
            (f::ATTACK_STATE, 0)
        ],
        "a dead creature"
    );
    h.run(100);
    let corpse = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.corpse
                .as_ref()
                .is_some_and(|c| c.creature == TEST_CREATURE)
        })
        .map(|t| t.id)
        .expect("the beetle's corpse");
    assert_eq!(
        start(&mut h, corpse),
        [
            (f::ATTACK_STATE, 1),
            (f::BASIC_ATTACKS_FAILED, refusal::ON_MONSTER_CORPSES),
            (f::ATTACK_STATE, 0)
        ],
        "its corpse"
    );
}
