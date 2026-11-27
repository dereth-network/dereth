//! Effects a creature's skills put on the players they land on: a strike's and a missile's target
//! effects go on as records cast by the creature at its rating in the skill, on each player an area
//! special reaches, and a harming missile's effect is its drain alone. Fixture: a made-up creature in a camp beside where a character starts,
//! whose one basic attack is a made-up skill, and made-up effects.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::Prop;
use yalain_store::StoredPosition;
use yalain_testkit::{test_template, Harness, TEST_PROFILE};
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::skills::{Hook, SkillDef, Usage};
use yalain_world::state::EffectState;

/// The made-up creature.
const FOE: u32 = 0x4700_7C01;
/// Its made-up skills: a strike and a missile, each putting a debuff on what it lands on.
const WEAKENING_STRIKE: u32 = 1990;
const WEAKENING_SHOT: u32 = 1991;
/// The harming missile, whose hit drains.
const HARMING_SHOT: u32 = 1860;
/// A made-up debuff: the damage the player adds, lowered for 10 s.
const WEAKNESS: u32 = 0x6F00_7C02;
/// A made-up instant harm with its own appearance, the harming missile's target effect here.
const SAP: u32 = 0x6F00_7C03;
const SAP_FX: u32 = 0x4100_7C04;
/// The creature's rating in its attack.
const RATING: i64 = 7;

fn weakness() -> EffectDef {
    EffectDef {
        kind: EffectKind::Numeric {
            flags: yalain_world::effect_defs::numeric::ADD,
            stats: vec![yalain_world::effects::DAMAGE_ADD],
            magnitude: Scale(vec![(1.0, -1.0), (50.0, -150.0)]),
        },
        duration: Scale(vec![(0.0, 10.0)]),
        internal_flags: yalain_world::effect_defs::HARMFUL
            | yalain_world::effect_defs::CLIENT_VISIBLE,
        ..yalain_testkit::plain_effect(WEAKNESS)
    }
}

fn sap() -> EffectDef {
    EffectDef {
        kind: EffectKind::InstantVital {
            flags: 0x1,
            change: Scale(vec![(1.0, -2.0), (50.0, -150.0)]),
        },
        fx: SAP_FX,
        ..yalain_testkit::plain_effect(SAP)
    }
}

/// An attack skill as the install defines one: a damaging hook, throwing a missile when `missile`,
/// and these target effects.
fn attack(id: u32, missile: bool, effects: Vec<u32>) -> SkillDef {
    SkillDef {
        id,
        active: true,
        kind: Some(if missile {
            yalain_world::rules::Kind::Missile
        } else {
            yalain_world::rules::Kind::Melee
        }),
        usage: Some(Usage {
            min_range: -1.0,
            max_range: if missile { 40.0 } else { 2.0 },
            behavior_other: [0x4100_0076; 3],
            hooks: vec![Hook {
                damages: true,
                flags: 0x2804,
                missile: if missile { 0x4700_00BE } else { 0 },
                missile_speed: 35.0,
                mult: 1.0,
                chance: 1.0,
                anim_hook: 1,
                ..Hook::default()
            }],
            target_effects: effects,
            ..Usage::default()
        }),
        ..SkillDef::default()
    }
}

/// A camp of one creature whose one basic attack is `skill` at [`RATING`], `distance` meters east
/// of where a character starts, and a player in the world with health to spare: the harness, the
/// client, the player and the creature.
fn foe_at(skill: SkillDef, distance: f32) -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    let id = skill.id;
    h.server.skill_table.skills.insert(id, skill);
    h.server.effect_table.effects.insert(WEAKNESS, weakness());
    h.server.effect_table.effects.insert(SAP, sap());
    let mut p = yalain_content::Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(3))
        .set("GameplayStatistics_MaxHealth", Prop::Int(60))
        .set("AICombat_NaturalDamage", Prop::Float(10.0))
        .set("AICombat_NaturalVariance", Prop::Float(0.5))
        .set("AI_PerceptionRadius", Prop::Float(30.0))
        .set("AI_HomesickRadius", Prop::Float(40.0))
        .set("NPC_DeathLootProfile", Prop::Word(TEST_PROFILE))
        .set("NPC_StartingSkill1", Prop::Word(id))
        .set("NPC_StartingSkill1Level", Prop::Int(RATING))
        .set("NPC_StartingSkill1_IsBase", Prop::Bool(true));
    let mut catalog = yalain_testkit::test_catalog();
    catalog.insert(test_template(FOE, 0x1169, p));
    h.server.catalog = catalog;
    h.server.loot = yalain_testkit::test_loot();
    let spot = [59.06 + distance, 240.2, -44.6];
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "foes".into(),
        cell: 0x0299_0109,
        origin: spot,
        radius: 0.0,
        population: 1,
        regen: 60.0,
        templates: vec![(FOE, 1)],
        spots: vec![(0x0299_0109, spot)],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let (b, me) = h.player_in_world("one", "Elara");
    for p in h.server.players.values_mut() {
        use yalain_world::player::quality;
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 1_000_000);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 1_000_000);
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let foe = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == FOE => Some(o.id),
            _ => None,
        })
        .expect("the creature is made");
    (h, b, me, foe)
}

/// The records of `state` the player holds.
fn held(h: &Harness, me: u64, state: u32) -> Vec<EffectState> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| {
            p.play
                .effects
                .iter()
                .filter(|e| e.state == state)
                .copied()
                .collect()
        })
        .unwrap_or_default()
}

/// How many attacks of the creature's landed on the player since `from`: stories with a hit on it
/// (a strike's from the creature, a missile's arrival told by the player).
fn landed(h: &Harness, b: usize, from: usize, me: u64, skill: u32) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::DoStory(e) if e.action.skill == skill
                && e.action.hooks.iter().any(|k| k.target == me && k.numbers[3] < 0))
        })
        .count()
}

/// A creature's strike that lands puts its skill's target effect on the player: a record cast by the
/// creature at its rating in the skill, its add told to the player.
/// Behaviour: server.creatures.skills-put-their-effects-on-players
#[test]
fn a_creatures_strike_puts_its_skills_effect_on_the_player_at_its_rating() {
    let (mut h, b, me, foe) = foe_at(attack(WEAKENING_STRIKE, false, vec![WEAKNESS]), 0.5);
    let from = h.bots[b].inbox.len();
    h.run_until(20_000, |h| landed(h, b, from, me, WEAKENING_STRIKE) >= 1);
    assert!(
        landed(&h, b, from, me, WEAKENING_STRIKE) >= 1,
        "a strike lands"
    );
    // The record goes on as the strike reaches the player, after its story.
    h.run_until(200, |h| !held(h, me, WEAKNESS).is_empty());
    h.run(5);
    let e = held(&h, me, WEAKNESS);
    let e = e.first().expect("the skill's effect on the player");
    #[allow(clippy::cast_precision_loss)]
    let rating = RATING as f32;
    assert_eq!((e.caster, e.skill, e.item), (foe, 0, 0));
    assert!(
        (e.spellcraft - rating).abs() < f32::EPSILON,
        "{}",
        e.spellcraft
    );
    let told = h
        .calls(b, from)
        .iter()
        .any(|(f, _)| *f == yalain_world::effects::function::ADD);
    assert!(told, "the add is told to the player");
}

/// A creature's lasting effect on a player shows its appearance just before the record's add.
/// Behaviour: server.effects.appearance-before-its-record
#[test]
fn a_creatures_lasting_effect_shows_its_appearance_on_the_player_just_before_its_record() {
    const WEAKNESS_FX: u32 = 0x4100_7C05;
    let (mut h, b, me, _) = foe_at(attack(WEAKENING_STRIKE, false, vec![WEAKNESS]), 0.5);
    h.server.effect_table.effects.insert(
        WEAKNESS,
        EffectDef {
            fx: WEAKNESS_FX,
            ..weakness()
        },
    );
    let from = h.bots[b].inbox.len();
    h.run_until(20_000, |h| !held(h, me, WEAKNESS).is_empty());
    h.run(5);
    assert!(!held(&h, me, WEAKNESS).is_empty(), "the effect goes on");
    let msgs = h.sent_messages(b, from);
    let on_me = |m: &ServerMessage| match m {
        ServerMessage::DoFx(e) => e.sender.id == me && e.action.fx == WEAKNESS_FX,
        _ => false,
    };
    let shown = msgs
        .iter()
        .position(on_me)
        .expect("the appearance on the player");
    let add = yalain_world::effects::function::ADD;
    let next = msgs.get(shown + 1);
    assert!(
        matches!(next, Some(ServerMessage::PlayerCall(c)) if c.function.0 == add),
        "the appearance just before the add"
    );
}

/// A creature's missile that lands puts its skill's target effect on the player as it arrives, cast
/// by the creature at its rating in the skill.
/// Behaviour: server.creatures.skills-put-their-effects-on-players
#[test]
fn a_creatures_missile_puts_its_skills_effect_on_the_player_it_lands_on() {
    let (mut h, b, me, foe) = foe_at(attack(WEAKENING_SHOT, true, vec![WEAKNESS]), 22.0);
    let from = h.bots[b].inbox.len();
    h.run_until(20_000, |h| landed(h, b, from, me, WEAKENING_SHOT) >= 1);
    assert!(
        landed(&h, b, from, me, WEAKENING_SHOT) >= 1,
        "a missile lands"
    );
    let e = held(&h, me, WEAKNESS);
    let e = e
        .first()
        .expect("the skill's effect on the player as the missile arrives");
    #[allow(clippy::cast_precision_loss)]
    let rating = RATING as f32;
    assert_eq!(e.caster, foe);
    assert!(
        (e.spellcraft - rating).abs() < f32::EPSILON,
        "{}",
        e.spellcraft
    );
}

/// A harming missile's target effect is the drain its hit is told with, put on once: the effect
/// itself, with its own appearance, never goes on beside it.
/// Behaviour: server.creatures.skills-put-their-effects-on-players
#[test]
fn a_harming_missiles_effect_is_its_drain_and_not_put_on_again() {
    let (mut h, b, me, _) = foe_at(attack(HARMING_SHOT, true, vec![SAP]), 22.0);
    let from = h.bots[b].inbox.len();
    h.run_until(20_000, |h| landed(h, b, from, me, HARMING_SHOT) >= 3);
    assert!(landed(&h, b, from, me, HARMING_SHOT) >= 3, "missiles land");
    let drains = h
        .world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == yalain_world::missiles::DRAIN_FX))
        .count();
    assert!(drains >= 3, "each hit drains: {drains}");
    let sapped = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.action.fx == SAP_FX));
    assert!(!sapped, "its effect is not put on beside the drain");
}

/// A creature's missile special with an area puts its skill's target effect on every player it lands
/// on, the player beside its target as well as the target.
/// Behaviour: server.creatures.skills-put-their-effects-on-players
#[test]
fn an_area_missile_puts_its_skills_effect_on_each_player_it_lands_on() {
    use yalain_world::missiles::{AttackSkill, Flight, CREATURE_ATTACKS};
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server
        .skill_table
        .skills
        .insert(WEAKENING_SHOT, attack(WEAKENING_SHOT, true, vec![WEAKNESS]));
    h.server.effect_table.effects.insert(WEAKNESS, weakness());
    let (b, me) = h.player_in_world("one", "Elara");
    let (_, near) = h.player_in_world("two", "Babyfare");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let creature = h
        .server
        .things
        .placed()
        .find(|t| t.template == yalain_testkit::TEST_CREATURE)
        .map(|t| t.id)
        .expect("the creature");
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    for p in h.server.players.values_mut() {
        use yalain_world::player::quality;
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, 1_000_000);
        yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, 1_000_000);
        if p.character == near {
            p.position.origin[0] = at.origin[0] + 5.0;
        }
    }
    if let Some(t) = h.server.things.get_mut(creature) {
        t.at = Some(StoredPosition {
            origin: [at.origin[0] + 2.0, at.origin[1], at.origin[2]],
            ..at
        });
        if let Some(c) = t.creature.as_mut() {
            c.stats.fights = false;
        }
    }
    // Loosed until it has landed on each, whatever the rolls, by which it lands only some of the
    // time.
    let mut landed_on = [false; 2];
    for _ in 0..100 {
        // Each arrival starts with no record of the effect on anyone.
        let worn: Vec<_> = h
            .server
            .players
            .iter()
            .flat_map(|(c, p)| {
                p.play
                    .effects
                    .iter()
                    .filter(|e| e.state == WEAKNESS)
                    .map(|e| (*c, e.eid))
            })
            .collect();
        for (world, eid) in worn {
            yalain_world::effects::remove(&mut h.server, &mut h.net, world, eid);
        }
        let from = h.bots[b].inbox.len();
        let clock = h.clock();
        yalain_world::missiles::arrives(
            &mut h.server,
            &mut h.net,
            Flight {
                creature,
                target: me,
                skill: AttackSkill {
                    skill: WEAKENING_SHOT,
                    radius: 12.5,
                    ..CREATURE_ATTACKS[7]
                },
                special: true,
                from: at,
                loosed_at: None,
            },
            clock,
        );
        h.run(10);
        for (i, who) in [me, near].into_iter().enumerate() {
            if area_hits(&h, b, from, who) == 0 {
                continue;
            }
            landed_on[i] = true;
            assert!(
                held(&h, who, WEAKNESS).iter().any(|e| e.caster == creature),
                "{who:#x} holds the skill's effect as it lands"
            );
        }
        if landed_on == [true; 2] {
            break;
        }
    }
    for (who, landed) in [me, near].into_iter().zip(landed_on) {
        assert!(landed, "a missile lands on {who:#x}");
    }
}

/// How many of the hooks of the stories of the area missile told since `from` landed on `who`.
fn area_hits(h: &Harness, b: usize, from: usize, who: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::DoStory(e) if e.action.skill == WEAKENING_SHOT => {
                Some(e.action.hooks.clone())
            }
            _ => None,
        })
        .flatten()
        .filter(|k| k.target == who && [0x4000_0008, 0x4000_0009].contains(&k.numbers[0]))
        .count()
}
