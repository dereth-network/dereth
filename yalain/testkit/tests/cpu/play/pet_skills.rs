//! What a pet's skills do beyond its blows: the aura a spirit holds. Fixture: a summoning skill, a
//! pet effect and a template whose skills name an aura, shaped as the data defines them, the numbers
//! made up; a player who trained it, and the test world's beetles.

use auberean_protocol::queue;
use auberean_script::args::Arg;
use yalain_content::{PetKind, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_template, Harness};
use yalain_world::calls::Value;
use yalain_world::effect_defs::{Aura, EffectDef, Kind, Scale, HARMFUL};
use yalain_world::effects::function as effect_fn;
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
const MELEE: u32 = 1845;
const SWORD: u32 = 0x4700_051C;
const USE_SKILL: u32 = 0x8204_000B;

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
