//! Forces: the heroes' toggled skills held at a price (Taunt, Armor and Reflection Force). Each
//! attack that reaches the holder costs focus, and short of it the force turns off; a holder keeps
//! one force at a time; Taunt Force's reply answers each attack. Fixture: a player with a sword in
//! hand and two force skills made up after the data's, in the data's shared class.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, LUMBERING_MIGHT, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::effect_defs::{EffectDef, Force, Kind, Scale, HARMFUL, WATCHES_VITALS};
use yalain_world::effects::{self, function as effect_fn, Cast, On, TOGGLED};
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skill_use::function;
use yalain_world::state::SkillState;
use yalain_world::world::Player;

const SWORD: u32 = 0x4700_051C;
const SHOW_TEXT: u32 = 0x8205_0011;
/// The forces' class in the data.
const FORCES: u32 = 1129;
/// Armor Force and Taunt Force (the Defender's): their skills, saved states and effects.
const ARMOR_FORCE: u32 = 1638;
const ARMOR_FORCE_STATE: u32 = 0x7000_341E;
const ARMOR_FORCE_EFFECT: u32 = 0x7000_1EC6;
const TAUNT_FORCE: u32 = 1646;
const TAUNT_FORCE_STATE: u32 = 0x7000_3426;
const TAUNT_FORCE_EFFECT: u32 = 0x7000_1EDA;
/// What Taunt Force answers with (here a lasting record, to be seen), and a reply that watches
/// effects alone.
const ANSWER: u32 = 0x6F00_7430;
const PLAIN_REPLY: u32 = 0x6F00_7431;

fn def(state: u32, kind: Kind, class: u32, internal_flags: u32) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags,
        enum_value: 0,
        eq_class: class,
        name: None,
        ends_at_logout: false,
        ends_at_teleport: false,
    }
}

/// A player in the world with the scene drawn, vigor to spare, `focus` of 100, a sword in hand,
/// Armor Force and Taunt Force trained, and a test creature beside it; each force costs `cost`
/// focus an attack. Returns the client, the player and the creature.
fn hero(focus: i32, cost: f32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    let creature = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("told of it");
    // It doesn't attack of itself: the tests' attacks are told.
    if let Some(c) = h
        .server
        .things
        .get_mut(creature)
        .and_then(|t| t.creature.as_mut())
    {
        c.next_attack = f64::INFINITY;
    }
    let base = h.server.skill_table.skills[&LUMBERING_MIGHT].clone();
    for (id, state, effect) in [
        (ARMOR_FORCE, ARMOR_FORCE_STATE, ARMOR_FORCE_EFFECT),
        (TAUNT_FORCE, TAUNT_FORCE_STATE, TAUNT_FORCE_EFFECT),
    ] {
        let mut d = base.clone();
        d.id = id;
        d.state = state;
        d.name = (0xA860_7CEC, 0x2500_0000 | id);
        if let Some(u) = d.usage.as_mut() {
            u.user_effects = vec![effect];
        }
        h.server.skill_table.skills.insert(id, d);
    }
    let t = &mut h.server.effect_table;
    t.effects.insert(
        ARMOR_FORCE_EFFECT,
        def(
            ARMOR_FORCE_EFFECT,
            Kind::Numeric {
                flags: 0x4,
                stats: vec![effects::ARMOR_RATING],
                magnitude: Scale(vec![(1.0, 10.0), (10.0, 100.0)]),
            },
            FORCES,
            0x0101_0783 | WATCHES_VITALS,
        ),
    );
    t.effects.insert(
        TAUNT_FORCE_EFFECT,
        def(
            TAUNT_FORCE_EFFECT,
            Kind::Reply {
                effect: ANSWER,
                chance: Scale(vec![(0.0, 1.0)]),
                classes: Vec::new(),
            },
            FORCES,
            0x0901_0783 | WATCHES_VITALS,
        ),
    );
    t.effects.insert(
        PLAIN_REPLY,
        EffectDef {
            duration: Scale(vec![(0.0, 600.0)]),
            ..def(
                PLAIN_REPLY,
                Kind::Reply {
                    effect: ANSWER,
                    chance: Scale(vec![(0.0, 1.0)]),
                    classes: Vec::new(),
                },
                60,
                0x0901_0783,
            )
        },
    );
    t.effects.insert(
        ANSWER,
        EffectDef {
            duration: Scale(vec![(0.0, 60.0)]),
            ..def(ANSWER, Kind::Other, 61, 0x1100_0003 | HARMFUL)
        },
    );
    for effect in [ARMOR_FORCE_EFFECT, TAUNT_FORCE_EFFECT] {
        t.forces.insert(
            effect,
            Force {
                change: Scale(vec![(0.0, -cost)]),
                vital: quality::FOCUS,
            },
        );
    }
    let p = player(&mut h, me);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    quals::set_int(&mut p.qualities, quality::FOCUS_MAX, 100);
    quals::set_int(&mut p.qualities, quality::FOCUS, focus);
    for skill in [ARMOR_FORCE, TAUNT_FORCE, LUMBERING_MIGHT] {
        p.play.skills.push(SkillState {
            skill,
            granted: 0,
            mask: 0x1,
            cost: 5,
            children: 0,
            dependents: 0,
            spent: 0,
        });
    }
    (h, b, me, creature)
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

/// Use a skill and let the use end; the messages since the request begin at the index returned.
fn use_skill(h: &mut Harness, b: usize, id: u32, skill: u32) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(skill), Arg::LongInt(0)],
        ),
    );
    h.run(80);
    from
}

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

/// The records the player holds of this effect, toggled.
fn holds(h: &mut Harness, me: u64, effect: u32) -> usize {
    player(h, me)
        .play
        .effects
        .iter()
        .filter(|e| e.state == effect && e.flags == TOGGLED)
        .count()
}

fn focus(h: &mut Harness, me: u64) -> Option<i32> {
    quals::int(&player(h, me).qualities, quality::FOCUS)
}

/// An attack by the creature reaches the player.
fn attacked(h: &mut Harness, me: u64, creature: u64) -> usize {
    let world = yalain_world::social::world_of(&h.server, me).expect("in the world");
    let from = h.bots[0].inbox.len();
    let clock = h.clock();
    effects::answer_attack(&mut h.server, &mut h.net, world, creature, clock);
    h.run(5);
    from
}

/// Each attack that reaches the holder of a force costs the focus its data names, the focus sent;
/// when the holder has less than that, the force turns off instead, as its skill turned off would:
/// its toggle off, its record taken off, "You stop …", and nothing paid. The install's
/// descriptions and the guides: "Whenever the hero is hit, however, they lose focus, and the skill
/// turns off if there is insufficient focus to maintain it."
/// Behaviour: server.forces.focus-paid-on-each-attack
#[test]
fn each_attack_on_a_forces_holder_costs_its_focus_and_short_of_it_the_force_turns_off() {
    let (mut h, b, me, creature) = hero(7, 3.0);
    use_skill(&mut h, b, 1, ARMOR_FORCE);
    assert_eq!(holds(&mut h, me, ARMOR_FORCE_EFFECT), 1, "on");
    assert_eq!(
        focus(&mut h, me),
        Some(7),
        "turning it on costs no focus here"
    );
    let from = attacked(&mut h, me, creature);
    assert_eq!(focus(&mut h, me), Some(4));
    assert!(h.world_messages(b, from).iter().any(|m| matches!(
        m,
        ServerMessage::UpdateInt(q) if q.quality == quality::FOCUS && q.value == 4
    )));
    attacked(&mut h, me, creature);
    assert_eq!(focus(&mut h, me), Some(1));
    assert_eq!(holds(&mut h, me, ARMOR_FORCE_EFFECT), 1, "still on");
    let from = attacked(&mut h, me, creature);
    assert_eq!(focus(&mut h, me), Some(1), "nothing paid");
    assert_eq!(holds(&mut h, me, ARMOR_FORCE_EFFECT), 0, "off");
    let names: Vec<u32> = calls_of(&h, b, from).iter().map(|(f, _)| *f).collect();
    assert_eq!(names, [effect_fn::TOGGLE, effect_fn::REMOVE, SHOW_TEXT]);
    assert_eq!(
        calls_of(&h, b, from)[0].1,
        [Value::Int(0), Value::Int(ARMOR_FORCE)]
    );
    // Off, an attack costs nothing.
    attacked(&mut h, me, creature);
    assert_eq!(focus(&mut h, me), Some(1));
}

/// A force's price at a spellcraft is the line its data draws, to the nearest whole point: Taunt
/// Force's and Reflection Force's 1 at 1 to 5 at 10, Armor Force's 2 to 10 (the guides' figures).
/// Behaviour: server.forces.price-by-spellcraft
#[test]
fn a_forces_price_follows_its_datas_line_by_spellcraft() {
    let taunt = Force {
        change: Scale(vec![(1.0, -1.0), (10.0, -5.0)]),
        vital: quality::FOCUS,
    };
    let armor = Force {
        change: Scale(vec![(1.0, -2.0), (10.0, -10.0)]),
        vital: quality::FOCUS,
    };
    assert_eq!(
        [1.0, 2.0, 5.5, 10.0, 0.0, 20.0].map(|x| taunt.cost(x)),
        [1, 1, 3, 5, 1, 5]
    );
    assert_eq!([1.0, 10.0].map(|x| armor.cost(x)), [2, 10]);
}

/// A holder keeps one force at a time: turning one on turns off the one it held, as that skill
/// turned off would (its toggle off, its record taken off, "You stop …"), before the new one is on.
/// A toggled skill that is no force stays on.
/// Behaviour: server.forces.one-at-a-time
#[test]
fn turning_a_force_on_turns_off_the_force_its_holder_held() {
    let (mut h, b, me, _) = hero(100, 1.0);
    use_skill(&mut h, b, 1, LUMBERING_MIGHT);
    let lumbering = player(&mut h, me).play.effects.len();
    assert!(lumbering > 0);
    use_skill(&mut h, b, 2, ARMOR_FORCE);
    assert_eq!(holds(&mut h, me, ARMOR_FORCE_EFFECT), 1);
    h.run(200);
    let from = use_skill(&mut h, b, 3, TAUNT_FORCE);
    assert_eq!(holds(&mut h, me, ARMOR_FORCE_EFFECT), 0, "the first off");
    assert_eq!(holds(&mut h, me, TAUNT_FORCE_EFFECT), 1, "the second on");
    assert_eq!(
        player(&mut h, me).play.effects.len(),
        lumbering + 1,
        "Lumbering Might stays on"
    );
    let got = calls_of(&h, b, from);
    let off = got
        .iter()
        .position(|(f, v)| {
            *f == effect_fn::TOGGLE && *v == [Value::Int(0), Value::Int(ARMOR_FORCE)]
        })
        .expect("the first's toggle off");
    assert_eq!(got[off + 1].0, effect_fn::REMOVE);
    assert_eq!(got[off + 2].0, SHOW_TEXT);
    let on = got
        .iter()
        .position(|(f, v)| {
            *f == effect_fn::TOGGLE && *v == [Value::Int(1), Value::Int(TAUNT_FORCE)]
        })
        .expect("the second's toggle on");
    assert!(off < on);
}

/// A reply that watches its holder's vitals change (Taunt Force's) answers each attack that reaches
/// its holder with its effect on the attacker, at its spellcraft, as it answers a harmful effect;
/// a reply that watches effects alone doesn't. The install's description: "Taunts enemies every
/// time they attack you."
/// Behaviour: server.forces.taunt-answers-attacks
#[test]
fn taunt_forces_reply_answers_each_attack_on_its_holder_and_a_plain_reply_does_not() {
    let (mut h, b, me, creature) = hero(100, 1.0);
    let answers = |h: &Harness| {
        effects::records_of(&h.server, On::Creature(creature))
            .iter()
            .filter(|e| e.state == ANSWER && e.caster == me)
            .count()
    };
    // A plain reply held: an attack isn't answered.
    let world = yalain_world::social::world_of(&h.server, me).expect("in the world");
    let clock = h.clock();
    let cast = Cast {
        spellcraft: 30.0,
        ..Cast::default()
    };
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        On::Player(world),
        PLAIN_REPLY,
        cast,
        clock
    ));
    attacked(&mut h, me, creature);
    assert_eq!(answers(&h), 0, "a plain reply");
    use_skill(&mut h, b, 1, TAUNT_FORCE);
    assert_eq!(holds(&mut h, me, TAUNT_FORCE_EFFECT), 1);
    attacked(&mut h, me, creature);
    assert_eq!(answers(&h), 1, "answered");
    attacked(&mut h, me, creature);
    assert_eq!(answers(&h), 2, "each attack");
    assert_eq!(focus(&mut h, me), Some(98), "and paid for");
}
