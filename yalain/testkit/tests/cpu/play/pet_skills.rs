//! What a pet's skills do beyond its blows: the aura a spirit holds, what a strike leaves on the
//! creature it reaches, and a hive firing what makes another pet. Fixture: a summoning skill, a pet
//! effect and a template whose skills name an aura, target effects or a summoning, shaped as the
//! data defines them, the numbers made up; a player who trained it, and the test world's beetles.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{PetKind, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::Value;
use yalain_world::effect_defs::{Aura, EffectDef, Kind, Scale, HARMFUL};
use yalain_world::effects::{self, function as effect_fn, On};
use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
use yalain_world::pets::function;
use yalain_world::player::quality;
use yalain_world::quals;
use yalain_world::skills::{Hook, SkillDef, Usage};
use yalain_world::state::SkillState;
use yalain_world::world::Player;

const SUMMON: u32 = 3001;
const PET_SKILL: u32 = 3002;
const PET_EFFECT: u32 = 0x7000_1FD4;
const PET_TEMPLATE: u32 = 0x4700_0857;
const AURA: u32 = 0x7000_1FB5;
const PULSED: u32 = 0x7000_1FB6;
const SLOW: u32 = 0x7000_1FBB;
const BURN: u32 = 0x7000_06C4;
const MADE_EFFECT: u32 = 0x7000_1FAE;
const MADE_TEMPLATE: u32 = 0x4700_0856;
const MELEE: u32 = 1845;
const SWORD: u32 = 0x4700_051C;
const USE_SKILL: u32 = 0x8204_000B;
const BEETLE: u32 = yalain_testkit::TEST_CREATURE;

fn def(state: u32, kind: Kind) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale::default(),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x140_0413,
        enum_value: 1,
        eq_class: 0,
        name: None,
    }
}

/// The summoning skill and pet effect, and the pet's template naming its skill.
fn with_pet(h: &mut Harness, beetles: u32, flags: u32, skill: Option<Usage>) {
    h.add_creatures(beetles);
    let usage = Usage {
        min_range: -1.0,
        max_range: 1.0,
        recovery: 1.0,
        min_use: 0.5,
        powerup: 1.0,
        powerup_behavior: 0x4100_000C,
        vigor_add: -1.0,
        min_vigor: 1.0,
        implements_right: -2,
        implements_left: -1,
        valid_targets: 1,
        behavior_other: [0x4100_00ED; 3],
        behavior_self: [0x4100_00ED; 2],
        user_effects: vec![PET_EFFECT],
        ui_flags: 0x32C0,
        ..Usage::default()
    };
    let skill_def = |id: u32, usage: Usage| SkillDef {
        id,
        icon: 0x4100_47E6,
        cap: 150,
        table: 0x6F00_000B,
        flags: 0x41,
        active: true,
        usage: Some(usage),
        ..SkillDef::default()
    };
    h.server
        .skill_table
        .skills
        .insert(SUMMON, skill_def(SUMMON, usage));
    let pet = EffectDef {
        external_flags: 177,
        internal_flags: 0x4140_8513,
        ..def(
            PET_EFFECT,
            Kind::Pet {
                class: 0x400,
                lifetime: Scale::default(),
            },
        )
    };
    h.server.effect_table.effects.insert(PET_EFFECT, pet);
    h.server.pets.read_rows(
        [PetKind::new(PET_EFFECT, PET_TEMPLATE, flags, &[])],
        std::iter::empty(),
    );
    let the_skill = if let Some(u) = skill {
        h.server
            .skill_table
            .skills
            .insert(PET_SKILL, skill_def(PET_SKILL, u));
        PET_SKILL
    } else {
        MELEE
    };
    let mut props = Props::default();
    props
        .set("GameplayStatistics_MaxHealth", Prop::Int(560))
        .set("AICombat_NaturalDamage", Prop::Float(45.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(40.0))
        .set("NPC_StartingSkill1", Prop::Word(the_skill))
        .set("NPC_StartingSkill1Level", Prop::Int(0))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true));
    h.server
        .catalog
        .insert(test_template(PET_TEMPLATE, 0x1169, props));
}

/// An aura's skill as the spirits' are: nothing to strike with, the aura on its user.
fn aura_skill() -> Usage {
    Usage {
        min_range: -1.0,
        max_range: 20.0,
        valid_targets: 0x130_0101,
        behavior_other: [0x4100_0072; 3],
        behavior_self: [0x4100_0074; 2],
        user_effects: vec![AURA],
        ui_flags: 704,
        hooks: vec![Hook {
            damages: true,
            flags: 8200,
            ..Hook::default()
        }],
        ..Usage::default()
    }
}

/// The aura, reaching players within 20 m, and what it pulses (made harmful or not).
fn with_aura(h: &mut Harness, harmful: bool) {
    let aura = def(
        AURA,
        Kind::Aura(Aura {
            pulsed: vec![PULSED],
            targets: 0x1,
            range: 20.0,
        }),
    );
    h.server.effect_table.effects.insert(AURA, aura);
    let flags = 0x140_0713 | if harmful { HARMFUL } else { 0 };
    let pulsed = EffectDef {
        duration: Scale(vec![(0.0, 180.0)]),
        external_flags: 0x6,
        internal_flags: flags,
        eq_class: 21,
        ..def(
            PULSED,
            Kind::Numeric {
                flags: 0x6,
                stats: vec![0x4_0125],
                magnitude: Scale(vec![(1.0, 5.0), (50.0, 55.0)]),
            },
        )
    };
    h.server.effect_table.effects.insert(PULSED, pulsed);
}

fn in_world(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn player(h: &mut Harness, me: u64) -> &mut Player {
    h.server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world")
}

/// A player in the world with vigor to spare and the summoning skill trained.
fn summoner(h: &mut Harness) -> (usize, u64) {
    let (b, me) = in_world(h, "one", "Kestrel");
    h.arm(b, me, SWORD);
    let p = player(h, me);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    p.play.skills.push(SkillState {
        skill: SUMMON,
        granted: 0,
        mask: 0x1,
        cost: 2,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    // The world's beetles go for no one unbidden.
    for t in h.server.things.by_id.values_mut() {
        if let Some(c) = t.creature.as_mut() {
            c.stats.attacks_first = false;
        }
    }
    (b, me)
}

/// The skill used and the use over: the pet it made.
fn summon(h: &mut Harness, b: usize) -> u64 {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE_SKILL, &[Arg::Int(1), Arg::Int(SUMMON), Arg::LongInt(0)]),
    );
    h.run(120);
    h.calls(b, from)
        .iter()
        .find(|(f, _)| *f == function::ADD)
        .and_then(|(_, v)| {
            v.iter().find_map(|x| match x {
                Value::Long(l) => Some(*l),
                _ => None,
            })
        })
        .expect("told of its pet")
}

fn fellows(h: &mut Harness, ids: &[u64]) {
    h.server.fellowships.list.push(Fellowship {
        name: "The Band".into(),
        leader: ids[0],
        flags: 0x1,
        room: 1,
        fellows: ids
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
}

/// The records of what the aura pulses a player holds.
fn pulsed_on(h: &mut Harness, me: u64) -> Vec<(u64, f32, u32)> {
    player(h, me)
        .play
        .effects
        .iter()
        .filter(|e| e.state == PULSED)
        .map(|e| (e.caster, e.spellcraft, e.flags))
        .collect()
}

/// A spirit holds its template's aura from as it appears: its pulses reach its owner and the owner's
/// fellows within the aura's range of the spirit, each a pulse's record from the spirit at its
/// level; a fellow beyond that range, a stranger and another player's character are not reached.
/// Behaviour: server.pets.aura-reaches-owner-and-fellows
#[test]
fn a_spirits_aura_reaches_its_owner_and_fellows_in_range_and_no_stranger() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(aura_skill()));
    with_aura(&mut h, false);
    let (b, me) = summoner(&mut h);
    let (_, near) = in_world(&mut h, "two", "Near");
    let (_, far) = in_world(&mut h, "three", "Far");
    let (_, stranger) = in_world(&mut h, "four", "Stranger");
    fellows(&mut h, &[me, near, far]);
    player(&mut h, far).position.origin[0] += 60.0;
    let pet = summon(&mut h, b);
    h.step_by(1.0);
    let level = h
        .server
        .things
        .get(pet)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level)
        .expect("a pet");
    #[allow(clippy::cast_precision_loss)]
    let expect = (pet, level as f32, 0x452);
    assert_eq!(pulsed_on(&mut h, me), vec![expect], "its owner");
    assert_eq!(pulsed_on(&mut h, near), vec![expect], "a fellow in range");
    assert!(pulsed_on(&mut h, far).is_empty(), "a fellow out of range");
    assert!(pulsed_on(&mut h, stranger).is_empty(), "a stranger");
    // Again soon after, and then a couple of minutes apart.
    let mut pulses = Vec::new();
    let mut from = h.bots[b].inbox.len();
    for _ in 0..300 {
        h.step_by(0.5);
        let adds = h
            .calls(b, from)
            .iter()
            .filter(|(f, _)| *f == effect_fn::ADD)
            .count();
        if adds > 0 {
            pulses.push(h.now.0);
        }
        from = h.bots[b].inbox.len();
    }
    assert!(pulses.len() >= 2, "{pulses:?}");
    let first = pulses[1] - pulses[0];
    assert!((121.0..=131.0).contains(&first), "{pulses:?}");
}

/// When its pet is gone the records of its aura's pulses come off whoever holds them, at once, and no
/// more come.
/// Behaviour: server.pets.aura-ends-with-the-pet
#[test]
fn a_spirits_aura_comes_off_with_the_spirit() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(aura_skill()));
    with_aura(&mut h, false);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    h.step_by(1.0);
    assert_eq!(pulsed_on(&mut h, me).len(), 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::DIE, &[Arg::LongInt(pet)]),
    );
    h.run(5);
    assert!(pulsed_on(&mut h, me).is_empty(), "off at once");
    assert!(
        h.calls(b, from)
            .iter()
            .any(|(f, _)| *f == effect_fn::REMOVE),
        "told"
    );
    for _ in 0..200 {
        h.step_by(1.0);
    }
    assert!(
        !h.calls(b, from).iter().any(|(f, _)| *f == effect_fn::ADD),
        "and no pulse came after"
    );
}

/// A pet's aura never puts on anything that harms: all it reaches is its owner and its friends.
/// Behaviour: server.pets.aura-never-harms
#[test]
fn a_pets_aura_puts_nothing_harmful_on_its_owner() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(aura_skill()));
    with_aura(&mut h, true);
    let (b, me) = summoner(&mut h);
    summon(&mut h, b);
    h.step_by(1.0);
    assert!(pulsed_on(&mut h, me).is_empty());
}

/// A striking skill that leaves effects, at a reach of its own.
fn strike_skill(effects: Vec<u32>) -> Usage {
    Usage {
        min_range: -1.0,
        max_range: 2.0,
        valid_targets: 0x30_0980,
        behavior_other: [0x4100_0077; 3],
        target_effects: effects,
        hooks: vec![Hook {
            flags: 8216,
            chance: 1.0,
            radius: 2.0,
            ..Hook::default()
        }],
        ..Usage::default()
    }
}

/// What a strike leaves in the tests: a slowing that lasts, and a burning that takes health.
fn with_left_effects(h: &mut Harness) {
    let slow = EffectDef {
        duration: Scale(vec![(0.0, 30.0)]),
        external_flags: 0xA,
        internal_flags: 0x340_0743,
        eq_class: 53,
        ..def(
            SLOW,
            Kind::Numeric {
                flags: 0xA,
                stats: vec![0x3_0129],
                magnitude: Scale(vec![(1.0, 0.9), (50.0, 0.6)]),
            },
        )
    };
    h.server.effect_table.effects.insert(SLOW, slow);
    let burn = EffectDef {
        duration: Scale(vec![(0.0, 10.0)]),
        external_flags: 0x11,
        internal_flags: 0x2340_0713,
        eq_class: 48,
        ..def(
            BURN,
            Kind::VitalOverTime {
                flags: 0x11,
                per_second: Scale(vec![(1.0, -1.0), (150.0, -75.0)]),
            },
        )
    };
    h.server.effect_table.effects.insert(BURN, burn);
}

fn beetle(h: &Harness) -> u64 {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == BEETLE && t.alive())
        .map(|t| t.id)
        .expect("a beetle")
}

/// Put a creature beside a player, slow to die and not fighting.
fn beside(h: &mut Harness, id: u64, me: u64) {
    let at = player(h, me).position;
    if let Some(t) = h.server.things.get_mut(id) {
        let mut to = at;
        to.origin[0] += 2.0;
        t.at = Some(to);
        t.max_health = 100_000;
        t.health = 100_000;
        if let Some(c) = t.creature.as_mut() {
            c.home = to;
            c.stats.attacks_first = false;
            c.stats.max_health = 100_000;
            c.wander_to = None;
            c.stats.movement.wander_chance = 0.0;
        }
    }
}

fn order_attack(h: &mut Harness, b: usize, target: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::ATTACK, &[Arg::LongInt(target)]),
    );
    h.run(5);
}

/// A pet's strike that reaches a creature leaves its skill's target effects on it, cast by the pet
/// at its level.
/// Behaviour: server.pets.strikes-leave-their-effects
#[test]
fn a_pets_strike_leaves_its_skills_target_effects_on_the_creature() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(strike_skill(vec![SLOW])));
    with_left_effects(&mut h);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    let target = beetle(&h);
    beside(&mut h, target, me);
    order_attack(&mut h, b, target);
    let mut held = Vec::new();
    for _ in 0..40 {
        h.step_by(0.25);
        held = effects::records_of(&h.server, On::Creature(target)).to_vec();
        if held.iter().any(|e| e.state == SLOW) {
            break;
        }
    }
    let e = held.iter().find(|e| e.state == SLOW).expect("left on it");
    assert_eq!(e.caster, pet, "cast by the pet");
    let level = h
        .server
        .things
        .get(pet)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level)
        .expect("a pet");
    #[allow(clippy::cast_precision_loss)]
    let at = level as f32;
    assert!((e.spellcraft - at).abs() < f32::EPSILON, "at its level");
}

/// What a pet's strike leaves that takes health counts as its owner's, as its blows do: the creature
/// remembers the owner's harm, and the health taken is the pet's part of the kill. Its strikes keep
/// one running on the creature rather than adding one each.
/// Behaviour: server.pets.left-harm-is-its-owners
#[test]
fn harm_a_pets_strike_leaves_counts_as_its_owners() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(strike_skill(vec![BURN])));
    with_left_effects(&mut h);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    let target = beetle(&h);
    beside(&mut h, target, me);
    order_attack(&mut h, b, target);
    for _ in 0..30 {
        h.step_by(1.0);
    }
    let held = effects::records_of(&h.server, On::Creature(target)).to_vec();
    let burning = held.iter().filter(|e| e.state == BURN && e.caster == pet);
    assert_eq!(burning.count(), 1, "its strikes keep one running");
    let c = h
        .server
        .things
        .get(target)
        .and_then(|t| t.creature.as_ref())
        .expect("a creature");
    assert!(c.damage_by.iter().any(|(by, n)| *by == me && *n > 0));
    assert!(
        c.damage_by.iter().all(|(by, _)| *by != pet),
        "its harm is its owner's: {:?}",
        c.damage_by
    );
    assert!(c.taken_by.iter().any(|(by, n)| *by == pet && *n > 0));
}

/// A pet's skill that makes another pet: with a kind of it known it fires at what it takes on, the
/// pet made its owner's, beside the creature and going after it; with none it does nothing.
/// Behaviour: server.pets.fires-what-makes-a-pet
#[test]
fn a_hive_fires_wasps_at_a_creature_and_with_no_row_stands_idle() {
    let hive = Usage {
        min_range: -1.0,
        max_range: 30.0,
        recovery: 8.0,
        valid_targets: 0x30_0980,
        target_effects: vec![MADE_EFFECT],
        hooks: vec![Hook {
            flags: 8217,
            chance: 1.0,
            radius: 30.0,
            ..Hook::default()
        }],
        ..Usage::default()
    };
    let mut h = Harness::new();
    with_pet(&mut h, 1, 32, Some(hive));
    let made = def(
        MADE_EFFECT,
        Kind::Pet {
            class: 0x2000,
            lifetime: Scale(vec![(0.0, 30.0)]),
        },
    );
    h.server.effect_table.effects.insert(MADE_EFFECT, made);
    let mut props = Props::default();
    props
        .set("GameplayStatistics_MaxHealth", Prop::Int(100))
        .set("NPC_StartingSkill1", Prop::Word(MELEE))
        .set("NPC_StartingSkill1Level", Prop::Int(0))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true));
    h.server
        .catalog
        .insert(test_template(MADE_TEMPLATE, 0x1169, props));
    let (b, me) = summoner(&mut h);
    let hive_pet = summon(&mut h, b);
    let target = beetle(&h);
    beside(&mut h, target, me);
    // No row yet: it stands idle, and says nothing.
    let from = h.bots[b].inbox.len();
    order_attack(&mut h, b, target);
    for _ in 0..10 {
        h.step_by(1.0);
    }
    assert_eq!(h.server.pets.of(me), vec![hive_pet]);
    assert!(
        h.world_messages(b, from)
            .iter()
            .all(|m| !matches!(m, ServerMessage::CreateObject(_))),
        "nothing made"
    );
    // With a row, it fires.
    h.server.pets.read_rows(
        [PetKind::new(MADE_EFFECT, MADE_TEMPLATE, 0, &[])],
        std::iter::empty(),
    );
    order_attack(&mut h, b, target);
    for _ in 0..3 {
        h.step_by(1.0);
    }
    let made: Vec<u64> = h
        .server
        .pets
        .of(me)
        .into_iter()
        .filter(|p| *p != hive_pet)
        .collect();
    assert_eq!(made.len(), 1, "a wasp");
    let c = h
        .server
        .things
        .get(made[0])
        .and_then(|t| t.creature.as_ref())
        .expect("a pet");
    assert_eq!(c.target, Some(target), "going after what it was fired at");
    // Not again until its skill's own wait is up.
    h.step_by(1.0);
    assert_eq!(h.server.pets.of(me).len(), 2, "one per its wait");
    for _ in 0..9 {
        h.step_by(1.0);
    }
    assert!(h.server.pets.of(me).len() >= 3, "and again after it");
}

const BUFF: u32 = 0x7000_0697;

/// A skill as the guardians' and the sand fiends' are: aimed at friends of these kinds, putting the
/// buff on its target.
fn buff_skill(valid_targets: u32, max_range: f32) -> Usage {
    Usage {
        min_range: -1.0,
        max_range,
        valid_targets,
        behavior_other: [0x4100_0075; 3],
        behavior_self: [0x4100_0074; 2],
        target_effects: vec![BUFF],
        ui_flags: 0x12C0,
        hooks: vec![Hook {
            damages: true,
            flags: 8200,
            ..Hook::default()
        }],
        ..Usage::default()
    }
}

/// The buff: a numeric change lasting a minute (made harmful or not).
fn with_buff(h: &mut Harness, harmful: bool) {
    let buff = EffectDef {
        duration: Scale(vec![(0.0, 60.0)]),
        external_flags: 0x2_000A,
        internal_flags: 0x140_0743 | if harmful { HARMFUL } else { 0 },
        eq_class: 27,
        ..def(
            BUFF,
            Kind::Numeric {
                flags: 0x2_000A,
                stats: vec![0x3_0129],
                magnitude: Scale(vec![(1.0, 1.05), (1000.0, 8.05)]),
            },
        )
    };
    h.server.effect_table.effects.insert(BUFF, buff);
}

/// The records of the buff a player holds: caster and spellcraft.
fn buffed(h: &mut Harness, me: u64) -> Vec<(u64, f32)> {
    player(h, me)
        .play
        .effects
        .iter()
        .filter(|e| e.state == BUFF)
        .map(|e| (e.caster, e.spellcraft))
        .collect()
}

/// The owner's rating in the summoning skill, which the pet's level follows.
fn rated(h: &mut Harness, me: u64, rating: u32) {
    if let Some(k) = player(h, me)
        .play
        .skills
        .iter_mut()
        .find(|k| k.skill == SUMMON)
    {
        k.granted = rating;
    }
}

/// The pet's stories that name a target, after a point in what a player was told.
fn pets_stories(h: &Harness, b: usize, from: usize, pet: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet))
        .count()
}

fn level_of(h: &Harness, pet: u64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    h.server
        .things
        .get(pet)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level as f32)
        .expect("a pet")
}

/// A pet whose skill is aimed at its summoner keeps its buff on its owner: put on at the pet's level,
/// told as the pet's use of it on its owner, not put on again while it runs and put on again once it
/// has run out; a fellow and a stranger beside it get nothing.
/// Behaviour: server.pets.keep-buffs-on-their-owner
#[test]
fn a_pet_keeps_its_buff_on_its_owner_and_on_no_one_else() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(buff_skill(0x130_0100, 10.0)));
    with_buff(&mut h, false);
    let (b, me) = summoner(&mut h);
    let (_, fellow) = in_world(&mut h, "two", "Fellow");
    let (_, stranger) = in_world(&mut h, "three", "Stranger");
    fellows(&mut h, &[me, fellow]);
    rated(&mut h, me, 30);
    let from = h.bots[b].inbox.len();
    let pet = summon(&mut h, b);
    h.step_by(1.0);
    let level = level_of(&h, pet);
    assert!(level > 1.0, "{level}");
    assert_eq!(buffed(&mut h, me), vec![(pet, level)], "its owner");
    assert!(buffed(&mut h, fellow).is_empty(), "a fellow");
    assert!(buffed(&mut h, stranger).is_empty(), "a stranger");
    let told = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::DoStory(e) if e.action.attacker == pet
            && e.action.skill == PET_SKILL
            && e.action.hooks.iter().any(|k| k.target == me))
    });
    assert!(told, "told as the pet's use on its owner");
    let first = player(&mut h, me)
        .play
        .effects
        .iter()
        .find(|e| e.state == BUFF)
        .map(|e| e.eid);
    let held_from = h.bots[b].inbox.len();
    for _ in 0..50 {
        h.step_by(1.0);
    }
    assert_eq!(
        buffed(&mut h, me).len(),
        1,
        "not put on again while it runs"
    );
    assert_eq!(pets_stories(&h, b, held_from, pet), 0, "nor used again");
    for _ in 0..30 {
        h.step_by(1.0);
    }
    let again = player(&mut h, me)
        .play
        .effects
        .iter()
        .find(|e| e.state == BUFF)
        .map(|e| e.eid);
    assert!(
        again.is_some() && again != first,
        "put on again once run out"
    );
    assert!(buffed(&mut h, fellow).is_empty() && buffed(&mut h, stranger).is_empty());
}

/// A pet whose skill is aimed at its user keeps the buff on itself, and on no player.
/// Behaviour: server.pets.keep-buffs-on-themselves
#[test]
fn a_pet_keeps_its_own_buff_on_itself() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(buff_skill(0x30_0001, 10.0)));
    with_buff(&mut h, false);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    h.step_by(1.0);
    let level = level_of(&h, pet);
    let held: Vec<(u64, f32)> = effects::records_of(&h.server, On::Creature(pet))
        .iter()
        .filter(|e| e.state == BUFF)
        .map(|e| (e.caster, e.spellcraft))
        .collect();
    assert_eq!(held, vec![(pet, level)]);
    assert!(buffed(&mut h, me).is_empty(), "its owner");
}

/// A pet's buff reaches no owner beyond its skill's reach, and nothing that harms is put on.
/// Behaviour: server.pets.keep-buffs-on-their-owner
#[test]
fn a_pets_buff_reaches_no_owner_out_of_reach_and_puts_on_nothing_harmful() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(buff_skill(0x130_0100, 1.0)));
    with_buff(&mut h, false);
    let (b, me) = summoner(&mut h);
    summon(&mut h, b);
    for _ in 0..10 {
        h.step_by(1.0);
    }
    assert!(buffed(&mut h, me).is_empty(), "out of reach");
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(buff_skill(0x130_0100, 10.0)));
    with_buff(&mut h, true);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    for _ in 0..10 {
        h.step_by(1.0);
    }
    assert!(buffed(&mut h, me).is_empty(), "harmful");
    assert!(effects::records_of(&h.server, On::Creature(pet)).is_empty());
}

const KIT: u32 = 0x4700_1C83;
const KIT_EFFECT: &str = "Eff_Test_Item_Summon_Kit";
const ITEM_USE: u32 = 0x8216_0016;

/// An item's use, as the client asks it: the item, used on its user.
fn use_of(item: u64, player: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
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

/// The pet a world's row says items naming the kit's effect make, one of its class at most as the
/// skill that summons it allows, and kits in the player's pack: what each names, used up when used.
fn with_kits(h: &mut Harness, world: yalain_net::ConnId, kits: u32) {
    let mut kind = PetKind::new(PET_EFFECT, PET_TEMPLATE, 1096, &[]);
    kind.item_effects = vec![KIT_EFFECT.to_string()];
    h.server
        .pets
        .read_rows([kind], [(SUMMON, vec![(1, 0x400)])]);
    let mut props = Props::default();
    props
        .set("Usage_Effect1", Prop::Text(KIT_EFFECT.to_string()))
        .set("Usage_Effect1_Spellcraft", Prop::Float(40.0))
        .set("Item_IsDestroyOnUse", Prop::Bool(true))
        .set("Item_IsStackable", Prop::Bool(false));
    h.server.catalog.insert(test_template(KIT, 0x1C7A, props));
    for _ in 0..kits {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, KIT, 1);
    }
}

fn kits_held(h: &mut Harness, me: u64) -> Vec<u64> {
    player(h, me)
        .play
        .items
        .iter()
        .filter(|i| i.template == KIT)
        .map(|i| i.id)
        .collect()
}

/// An item naming an effect a world's pet row lists makes that pet for its user at the item's
/// spellcraft, and is used up; one more than the skill that summons it allows is refused, the item
/// kept and nothing made.
/// Behaviour: server.pets.made-by-an-items-use
#[test]
fn a_kits_use_makes_its_pet_at_the_kits_spellcraft_and_is_used_up_within_the_kinds_limit() {
    let mut h = Harness::new();
    with_pet(&mut h, 0, 1096, None);
    let (b, me) = summoner(&mut h);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world");
    with_kits(&mut h, world, 2);
    let kits = kits_held(&mut h, me);
    assert_eq!(kits.len(), 2);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(ITEM_USE, &[use_of(kits[0], me)]),
    );
    h.run(20);
    let pets = h.server.pets.of(me);
    assert_eq!(pets.len(), 1, "made");
    let spent = yalain_world::item_use::flags::SPENT;
    let answered = super::items::answers(&h, b, from);
    assert_eq!(answered.len(), 1, "one answer");
    assert_eq!(answered[0].flags & spent, spent, "told spent");
    let level = h
        .server
        .things
        .get(pets[0])
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level);
    assert_eq!(level, Some(40), "at the kit's spellcraft");
    assert_eq!(
        kits_held(&mut h, me),
        vec![kits[1]],
        "the kit used is used up"
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(ITEM_USE, &[use_of(kits[1], me)]),
    );
    h.run(20);
    assert_eq!(h.server.pets.of(me), pets, "no second of its class");
    assert_eq!(kits_held(&mut h, me), vec![kits[1]], "kept");
}

const DRAIN: u32 = 0x7000_08C5;

/// A pet's strike that leaves a vital transfer takes its number at the pet's level from the
/// creature's health, and gives the pet its share of what was taken.
/// Behaviour: server.effects.transfers-move-what-was-taken
#[test]
fn a_pets_drain_takes_health_from_the_creature_and_gives_the_pet_its_share() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 1096, Some(strike_skill(vec![DRAIN])));
    let drain = EffectDef {
        external_flags: 0x145,
        internal_flags: 0x4340_1013 | HARMFUL,
        ..def(
            DRAIN,
            Kind::VitalTransfer {
                flags: 0x145,
                target: Scale(vec![(1.0, -10.0), (50.0, -200.0)]),
                caster: Scale(vec![(0.0, 2.0)]),
            },
        )
    };
    h.server.effect_table.effects.insert(DRAIN, drain);
    let (b, me) = summoner(&mut h);
    let pet = summon(&mut h, b);
    if let Some(t) = h.server.things.get_mut(pet) {
        t.health = 100;
    }
    let target = beetle(&h);
    beside(&mut h, target, me);
    #[allow(clippy::cast_precision_loss)]
    let level = h
        .server
        .things
        .get(pet)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.level as f32)
        .expect("a pet");
    #[allow(clippy::cast_possible_truncation)]
    let took = (-10.0 - (level - 1.0) * 190.0 / 49.0).floor() as i32;
    let from = h.bots[b].inbox.len();
    order_attack(&mut h, b, target);
    let told = |h: &Harness, on: u64, n: i32| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.sender.id == on
                && e.action.skill == 0
                && e.action.hooks.iter().any(|k| k.target == on && k.numbers[3] == n))
        })
    };
    let drained = h.run_until(200, |h| told(h, target, took));
    assert!(drained, "the creature told the drain of {took}");
    assert!(told(&h, pet, -2 * took), "the pet told twice that back");
}

/// A trap's strike as the data has one: a blow within a few meters that leaves an effect.
fn trap_skill() -> Usage {
    Usage {
        min_range: -1.0,
        max_range: 3.0,
        valid_targets: 0x30_0980,
        behavior_other: [0x4100_0077; 3],
        target_effects: vec![SLOW],
        hooks: vec![Hook {
            flags: 0x2008,
            chance: 1.0,
            radius: 3.0,
            ..Hook::default()
        }],
        ..Usage::default()
    }
}

/// Stand a thing this far east of a spot.
fn put_by(h: &mut Harness, id: u64, by: yalain_store::StoredPosition, east: f32) {
    let mut to = by;
    to.origin[0] += east;
    if let Some(t) = h.server.things.get_mut(id) {
        t.at = Some(to);
        if let Some(c) = t.creature.as_mut() {
            c.home = to;
            c.wander_to = None;
            c.stats.movement.wander_chance = 0.0;
        }
    }
}

/// The stories a pet told the player since `from`.
fn stories_by(h: &Harness, b: usize, from: usize, pet: u64) -> Vec<Vec<u64>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.attacker == pet => Some(
                e.action
                    .hooks
                    .iter()
                    .map(|k| k.target)
                    .filter(|t| *t != 0)
                    .collect(),
            ),
            _ => None,
        })
        .collect()
}

/// A pet whose kind ends on its strike (a trap) strikes nothing but a creature: beside a player at
/// peace and another player's trap it stays, striking none. A creature in its reach takes one strike,
/// and what the strike carries lands on it, cast by the trap, before the trap ends: taken off its
/// owner's list and dead, with no second strike.
/// Behaviour: server.pets.ends-on-its-strike
#[test]
fn a_trap_strikes_a_creature_once_and_ends_and_never_strikes_a_player_or_anothers_pet() {
    let mut h = Harness::new();
    with_pet(&mut h, 1, 32, Some(trap_skill()));
    with_left_effects(&mut h);
    if let Some(k) = h.server.pets.kinds.get_mut(&PET_EFFECT) {
        k.stays = true;
        k.ends_on_strike = true;
        k.lands = 1.0;
    }
    let (b, me) = summoner(&mut h);
    let target = beetle(&h);
    let far = player(&mut h, me).position;
    put_by(&mut h, target, far, 60.0);
    let trap = summon(&mut h, b);
    let at = h
        .server
        .things
        .get(trap)
        .and_then(|t| t.at)
        .expect("set down");
    // Another player at peace beside it, with a trap of its own beside it.
    let (c, other) = in_world(&mut h, "two", "Wren");
    h.arm(c, other, SWORD);
    let p = player(&mut h, other);
    p.play.skills.push(SkillState {
        skill: SUMMON,
        granted: 0,
        mask: 0x1,
        cost: 2,
        children: 0,
        dependents: 0,
        spent: 0,
    });
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    let theirs = summon(&mut h, c);
    put_by(&mut h, theirs, at, 2.5);
    let mut beside = at;
    beside.origin[1] += 1.5;
    player(&mut h, other).position = beside;
    let from = h.bots[b].inbox.len();
    let from_c = h.bots[c].inbox.len();
    for _ in 0..20 {
        h.step_by(0.5);
    }
    assert!(stories_by(&h, b, from, trap).is_empty(), "none struck");
    assert!(
        stories_by(&h, c, from_c, theirs).is_empty(),
        "theirs struck none"
    );
    assert_eq!(h.server.pets.of(me), vec![trap], "it waits");
    assert_eq!(h.server.pets.of(other), vec![theirs]);
    // A creature comes within its reach.
    put_by(&mut h, target, at, -1.5);
    let from = h.bots[b].inbox.len();
    let from_c = h.bots[c].inbox.len();
    for _ in 0..40 {
        h.step_by(0.25);
    }
    let told = stories_by(&h, b, from, trap);
    assert_eq!(told, vec![vec![target]], "one strike, at the creature");
    let left = effects::records_of(&h.server, On::Creature(target))
        .iter()
        .find(|e| e.state == SLOW)
        .map(|e| e.caster);
    assert_eq!(left, Some(trap), "what it carries landed, cast by the trap");
    assert!(h.server.pets.of(me).is_empty(), "ended");
    assert!(
        h.calls(b, from).iter().any(|(f, _)| *f == function::REMOVE),
        "its owner told"
    );
    assert!(h.server.things.get(trap).is_none_or(|t| !t.alive()));
    let mut named: Vec<u64> = stories_by(&h, c, from_c, theirs).concat();
    named.extend(stories_by(&h, b, from, trap).concat());
    assert!(
        named.iter().all(|t| *t == target),
        "only ever the creature: {named:?}"
    );
}
