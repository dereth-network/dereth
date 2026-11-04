//! Fights: a player's attack run on a creature, the creature's death, the experience and level it
//! pays, its corpse and what the corpse holds; taking from it and turning it into money.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE, TEST_ITEM};
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::levels::{Level, LevelTable};

const START_ATTACK: u32 = 0x8204_0008;
const ATTACK_STATE: u32 = 0x8204_001B;
const USE: u32 = 0x8216_0016;
const OPEN_DONE: u32 = 0x820E_005D;
const CLOSE_DONE: u32 = 0x820E_0060;
const MOVE_DONE: u32 = 0x820E_005E;
const CLOSE: u32 = 0x820E_000C;
const USE_DONE: u32 = 0x8216_0021;
const TRANSMUTE_ALL: u32 = 0x820E_0065;
const TRANSMUTE_ALL_DONE: u32 = 0x820E_006A;
const CORPSE: u32 = 0x4700_0539;
/// The human starting sword (a player attacks with a weapon in hand).
const STARTER_SWORD: u32 = 0x4700_051C;

/// Made-up levels for the tests: level 2 at 10 experience.
fn levels() -> LevelTable {
    LevelTable {
        levels: vec![
            Level {
                xp: 0,
                health: 100,
                vigor: 100,
                skill_credits: 0,
                hero_credits: 0,
            },
            Level {
                xp: 10,
                health: 130,
                vigor: 130,
                skill_credits: 1,
                hero_credits: 0,
            },
            Level {
                xp: 1000,
                health: 160,
                vigor: 160,
                skill_credits: 1,
                hero_credits: 0,
            },
        ],
    }
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}

fn creature_of(h: &Harness, b: usize) -> u64 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature is made")
}

/// The calls a client received since `from`: their functions and values.
fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// A use of an object as the client asks for it.
fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

/// A world with one beetle beside a new player, which attacks it until it dies; returns the
/// harness, the client, the player and the beetle.
fn fight() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    // The run is the claim, not who wins it: the beetle hits too softly to kill the player first.
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let dead = h.run_until(4000, |h| {
        h.world_messages(b, 0).iter().any(|m| {
            matches!(m, ServerMessage::UpdateBoolVisual(q) if q.object.id == beetle && q.quality == 279 && q.value == 1)
        })
    });
    assert!(dead, "the beetle dies");
    (h, b, me, beetle)
}

/// Asked once to attack, the server answers that the player attacks and swings for it, each swing
/// a story from the player with a hit or an evade on the target, until the target dies; its
/// killer is paid its death experience (11 for a level 2 beetle at level 1) and reaches level 2,
/// and everyone who sees it sees it dead.
/// Behaviour: server.combat.attack-run-to-the-death
#[test]
fn an_attack_run_swings_until_the_creature_dies_and_pays_its_experience() {
    let (h, b, me, beetle) = fight();
    let calls = calls_of(&h, b, 0);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == ATTACK_STATE && v == &vec![Value::Int(1)]),
        "the player is attacking"
    );
    let swings = h
        .world_messages(b, 0)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me))
        .count();
    assert!(swings >= 2, "it swings again and again: {swings}");
    let xp: Vec<u64> = h
        .world_messages(b, 0)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateLongInt(q) if q.quality == 300 => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(xp, vec![11], "the beetle's death experience");
    let level = h.world_messages(b, 0).iter().find_map(|m| match m {
        ServerMessage::UpdateInt(q) if q.quality == 302 => Some(q.value),
        _ => None,
    });
    assert_eq!(level, Some(2), "past 10 experience: level 2");
    let _ = beetle;
}

/// A player's swing at a creature is told first; the creature loses the health as the strike
/// lands, its skill's time over its time scale later, and only then does it fall.
/// Behaviour: server.combat.strike-lands
#[test]
fn a_players_swing_hurts_a_creature_as_it_lands_by_its_skill() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    let thing = h.server.things.get_mut(beetle).expect("the beetle");
    thing.health = 1;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let hit = |m: &ServerMessage| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me
            && e.action.hooks.iter().any(|k| k.target == beetle && k.bits & 0x800 != 0))
    };
    assert!(
        h.run_until(4000, |h| h.world_messages(b, from).iter().any(hit)),
        "a hit is told"
    );
    let told = h.now.0;
    let (skill, scale) = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::DoStory(e) if hit(m) => {
                Some((e.action.skill, e.action.attacker_behavior.time_scale))
            }
            _ => None,
        })
        .expect("its story");
    let sword = Some(yalain_world::skill_use::implement::SWORD);
    let lands = yalain_world::combat::hit_seconds(skill, sword) / f64::from(scale);
    let alive = |h: &Harness| h.server.things.get(beetle).is_some_and(|t| t.health > 0);
    assert!(alive(&h), "not hurt as it is told");
    let dead = |m: &ServerMessage| {
        matches!(m, ServerMessage::UpdateBoolVisual(q)
            if q.object.id == beetle && q.quality == 279 && q.value == 1)
    };
    assert!(
        h.run_until(100, |h| h.world_messages(b, from).iter().any(dead)),
        "it lands"
    );
    let gap = h.now.0 - told;
    assert!(
        (gap - lands).abs() < 0.12,
        "it landed {gap:.3} s after its story, not {lands:.3} s"
    );
}

/// A strike lands at its own skill's time; a skill seen too little by what is held.
/// Behaviour: server.combat.strike-lands
#[test]
fn a_strike_lands_at_its_skills_own_time() {
    use yalain_world::combat::hit_seconds;
    use yalain_world::skill_use::implement::{BOW, STAVE, SWORD};
    assert_eq!(hit_seconds(690, Some(SWORD)), 0.552);
    assert_eq!(hit_seconds(691, Some(SWORD)), 0.785);
    assert_eq!(hit_seconds(617, Some(STAVE)), 1.056);
    assert_eq!(hit_seconds(1, Some(BOW)), 0.572);
    assert_eq!(hit_seconds(1, Some(STAVE)), 0.817);
    assert_eq!(hit_seconds(1, Some(SWORD)), 0.713);
}

/// A practice target (a creature that never fights back, as the halls' Straw Drudges) is attacked
/// like any other: the run is answered and swings at it, and it does not strike back.
/// Behaviour: server.combat.practice-targets-are-struck
#[test]
fn a_practice_target_is_struck_and_never_strikes_back() {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let dummy = creature_of(&h, b);
    let mut beside = h
        .server
        .players
        .values()
        .next()
        .expect("the player")
        .position;
    beside.origin[0] += 1.5;
    if let Some(t) = h.server.things.get_mut(dummy) {
        t.at = Some(beside);
        t.max_health = 25_000;
        t.health = 25_000;
        if let Some(c) = t.creature.as_mut() {
            c.stats.fights = false;
        }
    }
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(dummy)]),
    );
    h.run(200);
    let calls = calls_of(&h, b, from);
    assert!(
        calls
            .iter()
            .any(|(f, v)| *f == ATTACK_STATE && v == &vec![Value::Int(1)]),
        "the player is attacking: {calls:x?}"
    );
    let msgs = h.world_messages(b, from);
    let swings = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == me))
        .count();
    assert!(swings >= 3, "it swings again and again: {swings}");
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == dummy)),
        "the target never strikes back"
    );
}

/// Two seconds after its death the creature's corpse appears where it fell, holding what its loot
/// profile leaves; three seconds later the creature leaves the world.
/// Behaviour: server.death.corpse-and-leaving
#[test]
fn a_dead_creature_leaves_a_corpse_with_its_loot_and_then_leaves() {
    let (mut h, b, _, beetle) = fight();
    let corpse = h.run_until(200, |h| {
        h.world_messages(b, 0)
            .iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE))
    });
    assert!(corpse, "a corpse appears");
    let left = h.run_until(200, |h| {
        h.world_messages(b, 0)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == beetle))
    });
    assert!(left, "the creature leaves");
}

/// An item drawn from a creature's loot profile carries what its dropper's level gives it of its
/// own: a piece of equipment dropped by the level 2 beetle (no loot quality of its own) a least
/// level of 2 to 5 and no Arcane Lore.
/// Behaviour: server.loot.treasure-own-properties
#[test]
fn equipment_a_creature_drops_carries_a_least_level_of_its_own_by_the_creatures_level() {
    let (mut h, b, _, _) = fight();
    h.server.treasure.equipment.insert(0x200);
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let own: Vec<_> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_ITEM)
        .map(|t| t.own)
        .collect();
    assert!(!own.is_empty(), "the corpse holds the item");
    for o in own {
        let least = o.min_level.expect("its own least level");
        assert!((2..=5).contains(&least), "{least}");
        assert_eq!(o.arcane_lore, None, "no Arcane Lore below a base of 52");
        assert_eq!(o.effect, None);
    }
}

/// Using the corpse opens it: the opener is told what it holds (the beetle's profile always
/// leaves one item) and of the item; using the item takes it, and once the taking is through the
/// move is answered and the item is the player's.
/// Behaviour: server.loot.open-and-take
#[test]
fn a_corpse_opens_to_its_killer_and_an_item_taken_from_it_is_the_players() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let opened = calls_of(&h, b, from)
        .into_iter()
        .find(|(f, _)| *f == OPEN_DONE)
        .expect("the corpse opens");
    assert_eq!(opened.1.last(), Some(&Value::Long(corpse)));
    let item = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_ITEM => Some(o.id),
            _ => None,
        })
        .expect("the item it holds is made");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    assert!(
        h.run_until(100, |h| calls_of(h, b, from)
            .iter()
            .any(|(f, _)| *f == MOVE_DONE)),
        "the take is answered"
    );
    let held = h
        .server
        .players
        .values()
        .any(|p| p.play.items.iter().any(|x| x.id == item));
    assert!(held, "the item is in the player's pack");
}

/// A corpse is open to one player at a time: another player's use of it while it is open is
/// refused (status 10001, flags 956) and opens nothing; once the first closes it, the other's use
/// opens it.
/// Behaviour: server.loot.one-viewer-at-a-time
#[test]
fn a_corpse_another_player_has_open_refuses_the_use_until_it_is_closed() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    let (other, oid) = h.player_in_world("two", "Fellowa");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let from = h.bots[other].inbox.len();
    h.bots[other].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, oid)]),
    );
    h.run(20);
    let got = calls_of(&h, other, from);
    assert!(
        !got.iter().any(|(f, _)| *f == OPEN_DONE),
        "nothing opens for the second"
    );
    let refused = got.iter().any(|(f, v)| {
        *f == USE_DONE
            && v.iter().any(|x| match x {
                Value::Graph(g) => g.iter().any(|(_, n)| match n {
                    calls::Node::Script { slots, .. } => {
                        slots.iter().any(|s| s.1 == 10001) && slots.iter().any(|s| s.1 == 956)
                    }
                    _ => false,
                }),
                _ => false,
            })
    });
    assert!(refused, "the use is refused: {got:?}");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(CLOSE, &[Arg::LongInt(corpse)]),
    );
    h.run(20);
    let from = h.bots[other].inbox.len();
    h.bots[other].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, oid)]),
    );
    h.run(20);
    assert!(
        calls_of(&h, other, from)
            .iter()
            .any(|(f, _)| *f == OPEN_DONE),
        "closed, it opens for the other"
    );
}

/// Turning a corpse's items into money moves each into the pack and destroys it there, the money
/// going up by its value; the answer says what was earned; each item is let go of (its holder
/// none) before it is gone.
/// Behaviour: server.loot.turned-into-money
#[test]
fn turning_a_corpses_items_into_money_pays_their_value() {
    let (mut h, b, me, _) = fight();
    assert!(h.run_until(200, |h| h.world_messages(b, 0).iter().any(
        |m| matches!(m, ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE)
    )));
    let corpse = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o.id),
            _ => None,
        })
        .expect("corpse");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    let from = h.bots[b].inbox.len();
    let mut g = Builder::new();
    g.object(
        0x1B59,
        &[
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(corpse),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(me),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(TRANSMUTE_ALL, &[Arg::Object(g.graph())]),
    );
    h.run(20);
    assert!(
        calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == TRANSMUTE_ALL_DONE),
        "answered"
    );
    let money: Vec<i32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == 3100 => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(money.len(), 1, "one item, one payment");
    assert!(money[0] >= 1, "worth something: {money:?}");
    let msgs = h.world_messages(b, from);
    let gone: Vec<u64> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::ForgetObject(o) => Some(o.0.id),
            _ => None,
        })
        .collect();
    assert_eq!(gone.len(), 1, "the item gone");
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::UpdateInstanceIdVisual(q)
            if q.object.id == gone[0] && q.quality == 258 && q.value == 0)),
        "let go of first"
    );
}

/// The corpse a client was told of, from its messages since `from`.
fn corpse_made(
    h: &Harness,
    b: usize,
    from: usize,
) -> Option<auberean_protocol::physics::CreateObject> {
    h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.weenie.entity_did == CORPSE => Some(o),
        _ => None,
    })
}

/// The death pose a client saw a creature fall in.
fn pose_of(h: &Harness, b: usize, creature: u64) -> f32 {
    h.world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::MoveSlider(e) if e.sender.id == creature && e.action.slider == POSE => {
                Some(e.action.value)
            }
            _ => None,
        })
        .expect("the death pose")
}

const POSE: u32 = 0x4000_001D;
const DEATH: u32 = 0x4100_001F;
const DEAD_MODE: u32 = 0x4100_0005;
const CORPSE_MODE: u32 = 0x4100_0003;

/// A creature that dies is told once more where it fell, standing still, after its death.
/// Behaviour: server.death.stops-where-it-falls
#[test]
fn a_dying_creature_is_told_standing_where_it_fell_after_its_death() {
    let (mut h, b, _, beetle) = fight();
    h.run(2);
    let msgs = h.world_messages(b, 0);
    let death = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == beetle && e.action.behavior == DEATH))
        .expect("its death");
    let fell = h
        .server
        .things
        .get(beetle)
        .and_then(|t| t.at)
        .expect("still there");
    let stop = msgs[death..]
        .iter()
        .find_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p)
                if p.sender.id == beetle =>
            {
                Some(*p)
            }
            _ => None,
        })
        .expect("its place after its death");
    assert_eq!(
        stop.heading_motion & 0x00FF_FFFF,
        yalain_world::world::IDLE_MOTION,
        "standing still"
    );
    assert_eq!(stop.origin.x, fell.origin[0]);
    assert_eq!(stop.origin.y, fell.origin[1]);
}

/// The death pose is one of six whole poses, and the corpse lies in it: its object carries the
/// pose's slider unless it is the first pose. In the same tick come the corpse's scale, its two
/// effects, its pose, its mode, then nobody's claim, its killer, the creature it was and its name,
/// their visual stamps 2 to 10, the object's own.
/// Behaviour: server.death.corpse-settles
#[test]
fn a_corpse_lies_in_the_death_pose_and_is_followed_by_its_settling() {
    let (mut h, b, me, beetle) = fight();
    let pose = pose_of(&h, b, beetle);
    assert!((0..6).any(|p| pose == p as f32), "a whole pose: {pose}");
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let corpse = corpse_made(&h, b, from).expect("corpse");
    assert_eq!(corpse.physics.mode, CORPSE_MODE);
    assert_eq!(corpse.physics.visual_ordering_stamp, 10);
    let lies = corpse
        .physics
        .sliders
        .iter()
        .find(|s| s.slider == POSE)
        .map(|s| s.value);
    if pose == 0.0 {
        assert_eq!(lies, None, "the first pose is left out");
    } else {
        assert_eq!(lies, Some(pose));
    }
    // Whichever pose this fight drew, each is made so.
    let mut t = h.server.things.get(corpse.id).cloned().expect("the corpse");
    for p in 0..6u8 {
        t.pose = Some(f32::from(p));
        let made = yalain_world::death::corpse_create(&t);
        let slider = made
            .physics
            .sliders
            .iter()
            .find(|s| s.slider == POSE)
            .map(|s| s.value);
        assert_eq!(slider, (p > 0).then_some(f32::from(p)), "pose {p}");
    }
    let msgs = h.world_messages(b, from);
    let at = msgs
        .iter()
        .position(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse.id))
        .expect("made");
    let after: Vec<(String, u16)> = msgs[at + 1..]
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == corpse.id => {
                Some((format!("scale {}", q.value), q.object.other_stamp))
            }
            ServerMessage::DoFx(e) if e.sender.id == corpse.id => {
                Some((format!("fx {:x}", e.action.fx), e.sender.other_stamp))
            }
            ServerMessage::MoveSlider(e) if e.sender.id == corpse.id => {
                Some((format!("pose {}", e.action.value), e.sender.other_stamp))
            }
            ServerMessage::SetMode(e) if e.sender.id == corpse.id => {
                Some((format!("mode {:x}", e.action.0), e.sender.other_stamp))
            }
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == corpse.id => {
                Some((format!("{} {:x}", q.quality, q.value), q.object.other_stamp))
            }
            ServerMessage::UpdateStringInfoVisual(q) if q.object.id == corpse.id => Some((
                if q.value == corpse.weenie.name {
                    format!("{} its name", q.quality)
                } else {
                    format!("{} another name", q.quality)
                },
                q.object.other_stamp,
            )),
            _ => None,
        })
        .collect();
    let expected: Vec<(String, u16)> = vec![
        ("scale 1".into(), 2),
        ("fx 41000002".into(), 3),
        ("fx 41000004".into(), 4),
        (format!("pose {pose}"), 5),
        ("mode 41000003".into(), 6),
        ("322 0".into(), 7),
        (format!("320 {me:x}"), 8),
        (format!("321 {beetle:x}"), 9),
        ("256 its name".into(), 10),
    ];
    assert_eq!(after, expected);
}

/// A corpse made for a client whose view it comes into after it appeared is followed by the same
/// changes it was made up of, its name last at stamp 10 (2017: 369 of 442 corpses first seen
/// after their creature's death).
/// Behaviour: server.death.corpse-settles-for-newcomers
#[test]
fn a_corpse_seen_later_is_followed_by_its_settling_too() {
    let (mut h, b, _me, _beetle) = fight();
    let from = h.bots[b].inbox.len();
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let corpse = corpse_made(&h, b, from).expect("corpse");
    let (c, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let msgs = h.world_messages(c, 0);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == corpse.id)),
        "made for the newcomer"
    );
    // Sent after the object, on the visual channel: they may arrive before it.
    let mut stamps: Vec<u16> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateFloatVisual(q) if q.object.id == corpse.id => {
                Some(q.object.other_stamp)
            }
            ServerMessage::DoFx(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::MoveSlider(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::SetMode(e) if e.sender.id == corpse.id => Some(e.sender.other_stamp),
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == corpse.id => {
                Some(q.object.other_stamp)
            }
            _ => None,
        })
        .collect();
    stamps.sort_unstable();
    let named = msgs.iter().find_map(|m| match m {
        ServerMessage::UpdateStringInfoVisual(q) if q.object.id == corpse.id => {
            Some((q.value == corpse.weenie.name, q.object.other_stamp))
        }
        _ => None,
    });
    assert_eq!(
        stamps,
        [2, 3, 4, 5, 6, 7, 8, 9],
        "the changes it was made up of"
    );
    assert_eq!(named, Some((true, 10)), "its name last");
}

/// A corpse made again for a client (one asking for it again, one whose view it comes into) lies
/// as it was first made, and a dead creature made again before it leaves is in its dead mode and
/// pose.
/// Behaviour: server.death.made-again-lying
#[test]
fn a_corpse_or_dead_creature_made_again_lies_in_its_pose() {
    let (mut h, b, _, beetle) = fight();
    let pose = pose_of(&h, b, beetle);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::ForceCreate(auberean_protocol::physics::Target(beetle)),
    );
    h.run(4);
    let again = h
        .world_messages(b, from)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == beetle => Some(o),
            _ => None,
        })
        .expect("the dead creature made again");
    assert_eq!(again.physics.mode, DEAD_MODE);
    let lies = |o: &auberean_protocol::physics::CreateObject| {
        o.physics
            .sliders
            .iter()
            .find(|s| s.slider == POSE)
            .map_or(0.0, |s| s.value)
    };
    assert_eq!(lies(&again), pose);
    assert!(h.run_until(200, |h| corpse_made(h, b, from).is_some()));
    let first = corpse_made(&h, b, from).expect("corpse");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::ForceCreate(auberean_protocol::physics::Target(
            first.id,
        )),
    );
    h.run(4);
    let asked = corpse_made(&h, b, from).expect("the corpse made again");
    let (c, _) = h.player_in_world("two", "Babyfare");
    scene_drawn(&mut h, c);
    let seen = corpse_made(&h, c, 0).expect("the corpse made for one coming in");
    for o in [&asked, &seen] {
        assert_eq!(o.physics.mode, CORPSE_MODE);
        assert_eq!(lies(o), pose);
        assert_eq!(o.weenie.name, first.weenie.name, "Corpse of the creature");
        assert_eq!(
            o.weenie.objects, first.weenie.objects,
            "its killer and what it was"
        );
    }
}

/// Open the first corpse a fight leaves; returns it.
fn open_corpse(h: &mut Harness, b: usize, me: u64) -> u64 {
    assert!(h.run_until(200, |h| corpse_made(h, b, 0).is_some()));
    let corpse = corpse_made(h, b, 0).expect("corpse").id;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(corpse, me)]),
    );
    h.run(20);
    corpse
}

/// An open corpse whose opener goes more than six meters from it closes: the opener is told, as
/// when it asks to close it; nearer, it stays open.
/// Behaviour: server.loot.corpse-closes-when-its-opener-walks-away
#[test]
fn an_open_corpse_closes_when_its_opener_walks_away() {
    let (mut h, b, me, _) = fight();
    let corpse = open_corpse(&mut h, b, me);
    let world = *h.server.players.keys().next().expect("in");
    let at = h
        .server
        .things
        .get(corpse)
        .and_then(|t| t.at)
        .expect("the corpse stands in the world");
    let stamp = h.server.players[&world].teleport_stamp;
    let step = |h: &mut Harness, dx: f32| {
        let to = auberean_primitives::Vec3::new(at.origin[0] + dx, at.origin[1], at.origin[2]);
        let report =
            yalain_testkit::play::report(at.cell, to, 0, yalain_world::world::IDLE_MOTION, stamp);
        h.bots[b].send(true, queue::WEENIE, &report);
        h.run(20);
    };
    let from = h.bots[b].inbox.len();
    step(&mut h, 4.0);
    assert!(
        !calls_of(&h, b, from).iter().any(|(f, _)| *f == CLOSE_DONE),
        "still near"
    );
    assert_eq!(
        yalain_world::inventory::open_corpse_of(&h.server, world),
        Some(corpse)
    );
    step(&mut h, 8.0);
    let closed: Vec<_> = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == CLOSE_DONE)
        .collect();
    assert_eq!(closed.len(), 1, "told once");
    assert_eq!(closed[0].1.last(), Some(&Value::Long(corpse)));
    assert_eq!(
        yalain_world::inventory::open_corpse_of(&h.server, world),
        None
    );
}

/// Ask to turn a corpse's items into money; returns the answer's values.
fn transmute(h: &mut Harness, b: usize, me: u64, corpse: u64) -> Vec<Value> {
    let from = h.bots[b].inbox.len();
    let mut g = Builder::new();
    g.object(
        0x1B59,
        &[
            Slot::Int(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(1),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(corpse),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(0),
            Slot::Long(me),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(TRANSMUTE_ALL, &[Arg::Object(g.graph())]),
    );
    h.run(20);
    calls_of(h, b, from)
        .into_iter()
        .find(|(f, _)| *f == TRANSMUTE_ALL_DONE)
        .expect("answered")
        .1
}

/// An answer's slot, and whether the list a slot of it refers to holds an id.
fn slot(v: &[Value], i: usize) -> u32 {
    match v.first().and_then(Value::root) {
        Some(calls::Node::Script { slots, .. }) => slots[i].1,
        _ => panic!("an answer object"),
    }
}
fn lists(v: &[Value], i: usize, id: u64) -> bool {
    let entry = slot(v, i);
    match v.first().and_then(|g| g.entry(entry)) {
        Some(calls::Node::Native { bytes, .. }) => bytes.windows(8).any(|w| w == id.to_le_bytes()),
        _ => false,
    }
}

/// The items a corpse holds now.
fn held_by(h: &Harness, corpse: u64) -> Vec<u64> {
    h.server
        .things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.contents.clone())
        .unwrap_or_default()
}

/// Turning a corpse's items into money keeps back an item worth nothing: it stays in the corpse,
/// listed as not turned into money with error 1105, the answer's status 1105 too when nothing was
/// turned into money, and nothing is paid.
/// Behaviour: server.loot.quest-items-kept-from-money
#[test]
fn turning_a_corpses_items_into_money_keeps_an_item_worth_nothing() {
    let (mut h, b, me, _) = fight();
    let corpse = open_corpse(&mut h, b, me);
    let item = held_by(&h, corpse)[0];
    if let Some(t) = h.server.things.get_mut(item) {
        t.value = 0;
    }
    let from = h.bots[b].inbox.len();
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 0), 1105, "the error");
    assert_eq!(slot(&v, 7), 1105, "the status: nothing turned into money");
    assert_eq!(slot(&v, 4), 0, "nothing earned");
    assert!(lists(&v, 10, item), "listed as not turned into money");
    assert!(!lists(&v, 2, item));
    assert_eq!(held_by(&h, corpse), vec![item], "still in the corpse");
    assert!(h.server.things.get(item).is_some(), "not destroyed");
    let paid = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::UpdateInt(q) if q.quality == 3100));
    assert!(!paid, "nothing paid");
}

/// A quest item is kept back however much it is worth, while the corpse's other items are turned
/// into money: the answer's status 0, its error 1105.
/// Behaviour: server.loot.quest-items-kept-from-money
#[test]
fn turning_a_corpses_items_into_money_keeps_a_quest_item_and_pays_the_rest() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let quest_item = 0x4700_2109;
    let mut props = yalain_content::Props::default();
    props.set("Item_IsQuestItem", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(quest_item, 0x200, props));
    h.server.loot.profiles[0].extras = vec![(quest_item, 1.0)];
    h.server.loot.profiles[0].counts = vec![0.0, 0.0, 1.0];
    h.server.levels = levels();
    let (b, me) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    h.arm(b, me, STARTER_SWORD);
    let beetle = creature_of(&h, b);
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    assert!(
        h.run_until(4000, |h| corpse_made(h, b, 0).is_some()),
        "it dies"
    );
    let corpse = open_corpse(&mut h, b, me);
    let held = held_by(&h, corpse);
    let quest = *held
        .iter()
        .find(|i| {
            h.server
                .things
                .get(**i)
                .is_some_and(|t| t.template == quest_item)
        })
        .expect("the quest item");
    let other = *held.iter().find(|i| **i != quest).expect("the other item");
    if let Some(t) = h.server.things.get_mut(quest) {
        t.value = 50;
    }
    let v = transmute(&mut h, b, me, corpse);
    assert_eq!(slot(&v, 0), 1105, "the error");
    assert_eq!(
        slot(&v, 7),
        0,
        "the status: something was turned into money"
    );
    assert!(slot(&v, 4) >= 1, "the other item paid");
    assert!(lists(&v, 10, quest) && !lists(&v, 2, quest));
    assert!(lists(&v, 2, other) && !lists(&v, 10, other));
    assert_eq!(held_by(&h, corpse), vec![quest], "the quest item stays");
}

/// What a treasure item carries of its own follows its dropper: the loot base is the loot quality
/// in tens (else the level); equipment's least level the base plus 0 to 3, every one of those
/// drawn, at most 50; Arcane Lore only above a base of 51, from the game's table, below the base's
/// own; a potion's effect of its level's tier (up to 15, 16-29, 30-39, 40-44, 45 up) at a
/// spellcraft from 0 to 100.
/// Behaviour: server.loot.treasure-own-properties-by-level
#[test]
fn a_treasure_items_own_properties_follow_its_droppers_level() {
    use yalain_world::rng::Rng;
    use yalain_world::treasure::{
        arcane_lore, base_of, lore_at, min_level, potion_effect, potion_tier, POTION_EFFECTS,
    };
    assert_eq!(base_of(43, Some(440)), 44);
    assert_eq!(base_of(20, None), 20);
    let tiers: Vec<usize> = [1, 15, 16, 29, 30, 39, 40, 44, 45, 60]
        .iter()
        .map(|l| potion_tier(*l))
        .collect();
    assert_eq!(tiers, [0, 0, 1, 1, 2, 2, 3, 3, 4, 4]);
    assert_eq!(lore_at(50.0), None);
    assert_eq!(lore_at(50.5), Some(8));
    assert_eq!(lore_at(51.0), Some(17));
    assert_eq!(lore_at(150.0), Some(887));
    let mut rng = Rng::new(7);
    let mut seen = [false; 4];
    let (mut low, mut high) = (100.0f32, 0.0f32);
    for _ in 0..2000 {
        let l = min_level(&mut rng, 20);
        assert!((20..=23).contains(&l), "{l}");
        seen[usize::try_from(l - 20).unwrap()] = true;
        assert!(min_level(&mut rng, 49) <= 50);
        assert_eq!(min_level(&mut rng, 60), 50);
        assert_eq!(arcane_lore(&mut rng, 51), None);
        let lore = arcane_lore(&mut rng, 57).expect("a base of 57 always draws above 50");
        assert!((17..=lore_at(56.0).unwrap()).contains(&lore), "{lore}");
        let (state, spellcraft) = potion_effect(&mut rng, 10);
        assert!(POTION_EFFECTS.iter().any(|k| k[0] == state), "{state:08X}");
        assert!((0.0..=100.0).contains(&spellcraft));
        low = low.min(spellcraft);
        high = high.max(spellcraft);
    }
    assert_eq!(seen, [true; 4], "each of the base plus 0 to 3 drawn");
    assert!(
        low < 5.0 && high > 95.0,
        "spellcraft over 0 to 100: {low} to {high}"
    );
    let some = (0..200)
        .filter(|_| arcane_lore(&mut rng, 53).is_some())
        .count();
    assert!(
        (40..=120).contains(&some),
        "about two in five above 50: {some}"
    );
}
