//! ACE: Source/ACE.Server/WorldObjects/Entity/CreatureVital.cs::CreatureVital
//! Factory creature has stats/vitals/motion; buffed player's stats change with private updates;
//! vitae scales max vitals; InqBurden/InqJumpVelocity/InqRunRate read stats; motion-state
//! members; movement statics and MoveTo heading match ACE vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula, SpellBase};
use dereth_assets::{SkillTable, SpellTable};
use dereth_primitives::DataId;
use dereth_protocol::qualities::{
    QualitiesPrivateUpdateAttribute, QualitiesPrivateUpdateAttribute2nd,
    QualitiesPrivateUpdateAttribute2ndLevel, QualitiesPrivateUpdateSkill,
};
use dereth_protocol::{self as dp, Message as ProtoMessage};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, same_f32, same_f64, u64_of};
use empyrean_content::models::world::Spell as DbSpell;
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::properties::{PropertyAttribute, PropertyAttribute2nd};
use empyrean_entity::enums::{
    EnchantmentTypeFlags as F, MotionCommand, MotionStance, PropertyDataId, Skill,
    SkillAdvancementClass, Vital, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::{ObjectGuid, Weenie};
use empyrean_net::SessionId;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::spell::Spell;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent, GameMessage};
use empyrean_world::network::game_messages::messages::{
    game_message_private_update_attribute, game_message_private_update_skill,
    game_message_private_update_vital,
};
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::physics::weenie_object::{self as wo, WeenieObject};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::entity::creature_attribute::StatCtx;
use empyrean_world::world_objects::managers::enchantment_manager_with_caching as emc;
use empyrean_world::world_objects::world_object::{self as world_object, CtorEnv, WorldObject};
use empyrean_world::world_objects::{creature_vitals, player_vitals};
use empyrean_world::World;

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const PLAYER: ObjectGuid = ObjectGuid::new(0x5000_0001);

// ------------------------------------------------------------------ fixture

/// Retail-shaped vital formulas: max health End/2, stamina End, mana Self.
fn vital_table() -> SecondaryAttributeTable {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    }
}

/// Run: Quickness (retail's formula), usable untrained.
fn skill_table() -> SkillTable {
    let run = SkillBase {
        description: "Run".into(),
        name: "Run".into(),
        icon: 0,
        trained_cost: 4,
        specialized_cost: 4,
        category: 3,
        chargen_use: 1,
        min_level: 1,
        formula: SkillFormula {
            w: 0,
            x: 1,
            y: 0,
            z: 1,
            attr1: 3,
            attr2: 0,
        },
        upper_bound: 0.0,
        lower_bound: 0.0,
        learn_mod: 0.0,
    };
    SkillTable {
        id: DataId(file_id::SKILL_TABLE),
        buckets: 64,
        skills: BTreeMap::from([(Skill::Run.0.cast_unsigned(), run)]),
    }
}

/// A buff: `(id, category, stat_mod_type, key, value)`.
type Buff = (u32, u32, F, u32, f32);

const ADD_ATTR: F = F(F::Attribute.0 | F::SingleStat.0 | F::Additive.0);
const ADD_VITAL: F = F(F::SecondAtt.0 | F::SingleStat.0 | F::Additive.0);
const ADD_SKILL: F = F(F::Skill.0 | F::SingleStat.0 | F::Additive.0);

/// Endurance +10, Quickness +50, max health +25, Run +20 (made-up ids and categories).
fn buffs() -> Vec<Buff> {
    vec![
        (
            1,
            1,
            ADD_ATTR,
            u32::from(PropertyAttribute::Endurance.0),
            10.0,
        ),
        (
            2,
            2,
            ADD_ATTR,
            u32::from(PropertyAttribute::Quickness.0),
            50.0,
        ),
        (
            3,
            3,
            ADD_VITAL,
            u32::from(PropertyAttribute2nd::MaxHealth.0),
            25.0,
        ),
        (4, 4, ADD_SKILL, Skill::Run.0.cast_unsigned(), 20.0),
    ]
}

fn spell_base(id: u32, category: u32) -> SpellBase {
    SpellBase {
        name: format!("Buff {id}"),
        description: String::new(),
        school: 4,
        icon: 0,
        category,
        bitfield: 0x4, // beneficial
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 10,
        spell_economy_mod: 1.0,
        formula_version: 0,
        component_loss: 0.0,
        meta_spell_type: 1,
        meta_spell_id: id,
        duration: Some((1800.0, 0.0, 0.0)),
        portal_lifetime: None,
        raw_comps: [0; 8],
        comp_key: 0,
        comps: Vec::new(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: 0,
        mana_mod: 0,
    }
}

/// A world with the stat tables above, the buff spells, and one session S whose player is
/// PLAYER: every attribute 100, vitals empty, Run trained.
fn world() -> World {
    let table = SpellTable {
        id: DataId(file_id::SPELL_TABLE),
        spell_buckets: 64,
        spells: buffs()
            .iter()
            .map(|&(id, cat, ..)| (id, spell_base(id, cat)))
            .collect(),
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    };
    let dats = empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_spell_table(table)
        .with_skill_table(skill_table())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, vital_table())
        .build()
        .expect("fake dats");
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_790_000_000.0,
        utc: DotNetDateTime::new(2026, 9, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats);
    let mut content = MemContent::new();
    for (id, _, stat, key, val) in buffs() {
        content = content.spell(DbSpell {
            id,
            name: format!("Buff {id}"),
            stat_mod_type: Some(stat.0.cast_unsigned()),
            stat_mod_key: Some(key),
            stat_mod_val: Some(val),
            ..DbSpell::default()
        });
    }
    w.content = Arc::new(content);

    let mut o = WorldObject::allocate(Class::Player);
    o.guid = PLAYER;
    o.biota.id = PLAYER.full();
    o.biota.properties_enchantment_registry = Some(Vec::new());
    let mut attributes = DotNetDict::new();
    for a in 1..=6u16 {
        attributes.insert(
            PropertyAttribute(a),
            PropertiesAttribute {
                init_level: 100,
                ..Default::default()
            },
        );
    }
    o.biota.properties_attribute = Some(attributes);
    o.biota.properties_attribute_2nd = Some(DotNetDict::new());
    let mut skills = DotNetDict::new();
    skills.insert(
        Skill::Run,
        PropertiesSkill {
            sac: SkillAdvancementClass::Trained,
            ..Default::default()
        },
    );
    o.biota.properties_skill = Some(skills);
    o.set_level(Some(10));
    // `Creature.SetEphemeralValues`' stat block (the constructor's path: detached, then filled)
    creature_vitals::set_ephemeral_stat_values(&w, &mut o);
    w.objects.insert(o).expect("fresh guid");
    let mut s = SessionData::default();
    s.set_player(Some(PLAYER));
    w.sessions.insert(S, s);
    w
}

fn p(w: &World) -> &WorldObject {
    w.objects.get(PLAYER).expect("the player")
}

fn cast(w: &mut World, id: u32) {
    let spell = Spell::new(w, id, true);
    emc::add(w, PLAYER, &spell, Some(PLAYER), None, false, false);
}

/// `(Endurance.Current, Health.MaxValue, Stamina.MaxValue, Run.Current)` in the world.
fn stats(w: &mut World) -> (u32, u32, u32, u32) {
    let (endurance, health, stamina) = (p(w).endurance(), p(w).health(), p(w).stamina());
    let run = w
        .objects
        .get_mut(PLAYER)
        .unwrap()
        .get_creature_skill(Skill::Run, false)
        .expect("Run");
    let c = &mut StatCtx::in_world(w, PLAYER);
    let e = endurance.current(c);
    let h = health.max_value(c);
    let s = stamina.max_value(c);
    (e, h, s, run.current(w, PLAYER))
}

/// Decodes `m` with dereth-protocol as `M` (opcode, then the body; the re-encoding must match).
fn decode<M: ProtoMessage + std::fmt::Debug>(m: &[u8]) -> M {
    let (ty, body) = (u32::from_le_bytes(m[..4].try_into().unwrap()), &m[4..]);
    assert_eq!(ty, M::OPCODE.0, "opcode");
    let decoded = dp::read_body_padded::<M>(body).expect("dereth-protocol decodes it");
    let again = dp::write_body(&decoded).expect("re-encode");
    assert_eq!(
        &body[..again.len()],
        again.as_slice(),
        "the fields round-trip"
    );
    decoded
}

fn data(m: &GameMessage) -> Vec<u8> {
    m.data.clone()
}

/// A creature from the factory has its stats full vitals and a motion state.

#[test]
fn a_creature_from_the_factory_has_its_stats_full_vitals_and_a_motion_state() {
    let w = world();
    let mut weenie = Weenie {
        weenie_class_id: 100,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    let mut attributes = DotNetDict::new();
    attributes.insert(
        PropertyAttribute::Endurance,
        PropertiesAttribute {
            init_level: 60,
            ..Default::default()
        },
    );
    attributes.insert(
        PropertyAttribute::Self_,
        PropertiesAttribute {
            init_level: 30,
            ..Default::default()
        },
    );
    weenie.properties_attribute = Some(attributes);
    let mut vitals = DotNetDict::new();
    // a stored current level above zero is still overwritten ("fix tod data")
    vitals.insert(
        PropertyAttribute2nd::MaxHealth,
        PropertiesAttribute2nd {
            init_level: 5,
            current_level: 3,
            ..Default::default()
        },
    );
    weenie.properties_attribute_2nd = Some(vitals);
    weenie.properties_skill = Some(DotNetDict::new());
    weenie.properties_did = Some(DotNetDict::new());
    weenie
        .properties_did
        .as_mut()
        .unwrap()
        .insert(PropertyDataId::MotionTable, 0x0900_0001);

    let o = CtorEnv::with_world(&w, |env| {
        factory::create_new_world_object(env, Arc::new(weenie), ObjectGuid::new(0x8000_0100))
    })
    .expect("a creature");

    // the stat block: 3 vitals, 6 attributes (missing records added), no skills
    assert_eq!(o.attributes().len(), 6);
    assert_eq!(o.vitals().len(), 3);
    assert_eq!(o.biota.properties_attribute.as_ref().unwrap().len(), 6);
    // Health: 5 + End/2 = 35; Stamina: End = 60; Mana: Self = 30 (non-players are filled)
    assert_eq!(
        (
            o.health().current(&o),
            o.stamina().current(&o),
            o.mana().current(&o)
        ),
        (35, 60, 30)
    );
    // `CurrentMotionState = new Motion(MotionStance.NonCombat, MotionCommand.Ready)` (after
    // WorldObject's `Invalid` for a motion table)
    let m =
        o.wo.world_object_properties
            .current_motion_state
            .as_ref()
            .expect("a motion state");
    assert_eq!(
        (m.stance, m.motion_state.forward_command),
        (MotionStance::NonCombat, MotionCommand::Ready)
    );
}

/// A buffed players stats change and the private updates carry them.

#[test]
fn a_buffed_players_stats_change_and_the_private_updates_carry_them() {
    let mut w = world();

    // Before: Endurance 100; max health End/2 = 50; stamina End = 100; Run = Quickness = 100.
    // The first read fills the manager's caches.
    assert_eq!(stats(&mut w), (100, 50, 100, 100));
    assert_eq!(player_vitals_rate(&mut w), run_rate_of(100));

    start_capture();
    for (id, ..) in buffs() {
        cast(&mut w, id);
    }
    let _ = take_sent();

    // After (the casts cleared the caches): Endurance 110; health 110/2 + 25 = 80; stamina 110;
    // Run = Quickness 150 + 20 = 170.
    assert_eq!(stats(&mut w), (110, 80, 110, 170));
    // read again: from the caches, the same
    assert_eq!(stats(&mut w), (110, 80, 110, 170));
    let endurance = p(&w).endurance();
    assert_eq!(
        endurance
            .modifier_type(&mut StatCtx::in_world(&mut w, PLAYER))
            .0,
        1,
        "Buffed"
    );
    assert_eq!(player_vitals_rate(&mut w), run_rate_of(170));

    // Player.SetMaxVitals: the vitals to their buffed maxima, then the three level updates
    start_capture();
    player_vitals::player_set_max_vitals(&mut w, PLAYER);
    let sent = take_sent();
    assert_eq!(sent.len(), 3);
    let levels: Vec<(u32, u32)> = sent
        .iter()
        .map(|(_, _, b)| {
            let d: QualitiesPrivateUpdateAttribute2ndLevel = decode(b);
            (d.0.property_id, d.0.value)
        })
        .collect();
    assert_eq!(
        levels,
        [
            (Vital::Health.0, 80),
            (Vital::Stamina.0, 110),
            (Vital::Mana.0, 100)
        ]
    );

    // GameMessagePrivateUpdateVital carries the vital's record: current 80
    let health = p(&w).health();
    let m = game_message_private_update_vital::game_message_private_update_vital(
        w.objects.get_mut(PLAYER).unwrap(),
        health,
    );
    let d: QualitiesPrivateUpdateAttribute2nd = decode(&data(&m));
    assert_eq!(
        (d.0.property_id, d.0.value.current_level),
        (u32::from(PropertyAttribute2nd::MaxHealth.0), 80)
    );

    // the attribute and skill updates carry the records (the base: the client applies the
    // enchantments it is told about itself)
    let m = game_message_private_update_attribute::game_message_private_update_attribute(
        w.objects.get_mut(PLAYER).unwrap(),
        endurance,
    );
    let d: QualitiesPrivateUpdateAttribute = decode(&data(&m));
    assert_eq!(
        (
            d.0.property_id,
            d.0.value.init_level,
            d.0.value.level_from_cp
        ),
        (2, 100, 0)
    );
    let run = w
        .objects
        .get_mut(PLAYER)
        .unwrap()
        .get_creature_skill(Skill::Run, false)
        .unwrap();
    let m = game_message_private_update_skill::game_message_private_update_skill(
        w.objects.get_mut(PLAYER).unwrap(),
        run,
    );
    let d: QualitiesPrivateUpdateSkill = decode(&data(&m));
    assert_eq!(
        (d.0.property_id, d.0.value.sac),
        (
            Skill::Run.0.cast_unsigned(),
            SkillAdvancementClass::Trained.0
        )
    );
}

/// `WeenieObject.InqRunRate` for the player.
fn player_vitals_rate(w: &mut World) -> f32 {
    let weenie = WeenieObject::new(w, PLAYER);
    let (answered, rate) = weenie.inq_run_rate(w);
    assert!(answered, "InqRunRate always answers");
    rate
}

#[test]
fn vitae_scales_the_max_vitals_detached_and_in_the_world() {
    use empyrean_entity::enums::SpellId;
    use empyrean_entity::models::PropertiesEnchantmentRegistry;
    use empyrean_world::world_objects::player_properties::player_vitae;

    let mut w = world();
    // a 10% vitae penalty (ACE's vitae entry: spell 666, StatModValue 0.9)
    let vitae = PropertiesEnchantmentRegistry {
        spell_id: SpellId::Vitae.0.cast_signed(),
        layer_id: 1,
        spell_category: empyrean_entity::enums::SpellCategory(204),
        stat_mod_type: F(F::MultipleStat.0 | F::Multiplicative.0 | F::Vitae.0),
        stat_mod_value: 0.9,
        ..PropertiesEnchantmentRegistry::default()
    };
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .biota
        .properties_enchantment_registry = Some(vec![vitae]);

    assert!(same_f32(player_vitae(&StatCtx::detached(&w, p(&w))), 0.9));
    assert!(same_f32(
        player_vitae(&StatCtx::in_world(&mut w, PLAYER)),
        0.9
    ));
    // max health 50 * 0.9 = 45; stamina 90
    let (health, stamina) = (p(&w).health(), p(&w).stamina());
    assert_eq!(
        (
            health.max_value(&mut StatCtx::detached(&w, p(&w))),
            stamina.max_value(&mut StatCtx::detached(&w, p(&w)))
        ),
        (45, 90)
    );
    assert_eq!(health.max_value(&mut StatCtx::in_world(&mut w, PLAYER)), 45);
}

/// Inq burden and inq jump velocity read the players stats.

#[test]
fn inq_burden_and_inq_jump_velocity_read_the_players_stats() {
    let mut w = world();
    let weenie = WeenieObject::new(&w, PLAYER);
    assert!(weenie.is_player);

    // Strength 100: capacity 150 * 100 = 15000; EncumbranceVal 3000: burden 0.2
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_encumbrance_val(Some(3000));
    assert_eq!(weenie.inq_burden(&mut w), Some(0.2));

    // no stamina: the jump skill counts as 0; GetJumpHeight(0.2, 0, 1, 1) = max(0.05, 0.35) = 0.35
    let stamina = p(&w).stamina();
    stamina.set_current(w.objects.get_mut(PLAYER).unwrap(), 0);
    assert!(p(&w).biota.get_skill(Skill::Jump).is_none());
    let (answered, v) = weenie.inq_jump_velocity(&mut w, 1.0);
    assert!(answered);
    let want = retail_jump_height(0.2, 0, 1.0, 1.0).velocity();
    assert!(same_f32(v, want), "{v} vs {want}");
    // `GetCreatureSkill(Skill.Jump)` added the record, as ACE's does
    assert_eq!(
        p(&w).biota.get_skill(Skill::Jump).map(|s| s.sac),
        Some(SkillAdvancementClass::Untrained)
    );

    // a non-player answers nothing
    let dummy = WeenieObject {
        is_creature: true,
        ..WeenieObject::default()
    };
    assert_eq!(dummy.inq_burden(&mut w), None);
    assert_eq!(dummy.inq_jump_velocity(&mut w, 1.0), (false, 0.0));
    // and a creature-less weenie object runs at skill 0
    assert_eq!(dummy.inq_run_rate(&mut w), (true, 1.0));
}

/// V332: the physics motion interpreter's run rate is the creature's own,
/// burden included, not the unencumbered rate.
#[test]
fn inq_run_rate_applies_the_players_burden_as_get_run_rate_does() {
    use empyrean_world::world_objects::monster_navigation::get_run_rate;

    let mut w = world();
    let weenie = WeenieObject::new(&w, PLAYER);
    let unencumbered = player_vitals_rate(&mut w);
    assert!(same_f32(unencumbered, get_run_rate(&mut w, PLAYER)));

    // Strength 100: capacity 15000; EncumbranceVal 25000: burden 5/3, which slows the run
    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .set_encumbrance_val(Some(25000));
    assert!(weenie.inq_burden(&mut w).is_some_and(|b| b > 1.0));
    let (answered, encumbered) = weenie.inq_run_rate(&mut w);
    assert!(answered);
    let reported = get_run_rate(&mut w, PLAYER);
    assert!(same_f32(encumbered, reported), "{encumbered} vs {reported}");
    assert!(encumbered < unencumbered, "{encumbered} vs {unencumbered}");
}

/// Execute motion set stance and get current motion state follow current motion state.

#[test]
fn execute_motion_set_stance_and_get_current_motion_state_follow_current_motion_state() {
    let mut w = world();
    // a player constructed by hand has no CurrentMotionState: (Invalid, Ready)
    assert_eq!(
        world_object::get_current_motion_state(&w, PLAYER),
        (MotionStance::Invalid, MotionCommand::Ready)
    );

    w.objects
        .get_mut(PLAYER)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state = Some(Motion::new(
        MotionStance::NonCombat,
        MotionCommand::Ready,
        1.0,
    ));
    // no motion table: an animation length of 0; the motion becomes the current one
    let len = world_object::execute_motion(
        &mut w,
        PLAYER,
        Motion::new(MotionStance::NonCombat, MotionCommand::Sitting, 1.0),
        false,
        None,
        false,
    );
    assert!(same_f32(len, 0.0));
    assert_eq!(
        world_object::get_current_motion_state(&w, PLAYER),
        (MotionStance::NonCombat, MotionCommand::Sitting)
    );

    world_object::set_stance(&mut w, PLAYER, MotionStance::HandCombat, false);
    assert_eq!(
        world_object::get_current_motion_state(&w, PLAYER),
        (MotionStance::HandCombat, MotionCommand::Ready)
    );
}

// ------------------------------------------------------------------ ACE vectors: movement/

fn replay(name: &str, mut each: impl FnMut(&serde_json::Value, &serde_json::Value) -> bool) {
    let file = vectors::load_named("movement", name);
    assert!(!file.cases.is_empty(), "movement/{name}: no cases");
    let bad: Vec<String> = file
        .cases
        .iter()
        .filter(|c| !each(&c.input, &c.output))
        .map(|c| format!("in {} expected {}", c.input, c.output))
        .collect();
    assert!(
        bad.is_empty(),
        "movement/{name}: {} of {} differ:\n  {}",
        bad.len(),
        file.cases.len(),
        bad[..bad.len().min(8)].join("\n  ")
    );
}

fn int(v: &serde_json::Value) -> i32 {
    i32::try_from(i64_of(v).expect("int")).expect("i32")
}

/// Retail's jump height, spelled out: single-precision constants, the product at double precision,
/// the 0.35 floor (also for NaN).
fn retail_jump_height(load: f32, skill: u32, power: f32, scaling: f32) -> RetailHeight {
    #[allow(clippy::cast_precision_loss)]
    let s = f64::from(skill as f32);
    let load_mod = if load < 1.0 {
        1.0
    } else if load < 2.0 {
        f64::from(2.0 - load)
    } else {
        0.0
    };
    let h = (s / (s + 1300.0) * f64::from(22.2f32) + f64::from(0.05f32))
        * load_mod
        * f64::from(power.clamp(0.0, 1.0))
        / f64::from(scaling);
    RetailHeight(if h >= f64::from(0.35f32) {
        h
    } else {
        f64::from(0.35f32)
    })
}

struct RetailHeight(f64);

impl RetailHeight {
    #[allow(clippy::cast_possible_truncation)]
    fn to_f32(&self) -> f32 {
        self.0 as f32
    }
    #[allow(clippy::cast_possible_truncation)]
    fn velocity(&self) -> f32 {
        (self.0 * 19.6).sqrt() as f32
    }
}

/// How many representable floats apart two finite values of the same sign are.
fn float_steps(a: f32, b: f32) -> u32 {
    if !a.is_finite() || !b.is_finite() || a.is_sign_negative() != b.is_sign_negative() {
        return u32::MAX;
    }
    a.to_bits().abs_diff(b.to_bits())
}

/// `EncumbranceSystem.GetBurden` and `GetBurdenMod` are the shared rules' `burden::load` and
/// `load_mod`: ACE's vectors are replayed against them directly. `EncumbranceCapacity` stays ACE's
/// (the rules saturate where ACE wraps; these vectors reach it with 2^31 - 1 augmentations).
#[test]
fn movement_system_and_encumbrance_statics_match_ace() {
    use dereth_rules::burden;
    replay("movement_system_get_run_rate", |i, o| {
        let got = wo::movement_system_get_run_rate(
            f32_of(&i["burden"]).unwrap(),
            int(&i["run_skill"]),
            f32_of(&i["scaling"]).unwrap(),
        );
        same_f64(got, f64_of(o).unwrap())
    });
    replay("movement_system_get_jump_height", |i, o| {
        let skill = u32::try_from(u64_of(&i["jump_skill"]).unwrap()).unwrap();
        let got = wo::movement_system_get_jump_height(
            f32_of(&i["burden"]).unwrap(),
            skill,
            f32_of(&i["power"]).unwrap(),
            f32_of(&i["scaling"]).unwrap(),
        );
        // V230: retail's product at double precision, rounded once. ACE's
        // recorded height is the record wherever it agrees; elsewhere it is one float step from
        // retail's (each of ACE's steps rounds), or NaN for a NaN power (retail's floor).
        let ace = f32_of(o).unwrap();
        let power = f32_of(&i["power"]).unwrap();
        let retail = retail_jump_height(
            f32_of(&i["burden"]).unwrap(),
            skill,
            power,
            f32_of(&i["scaling"]).unwrap(),
        );
        same_f32(got, retail.to_f32())
            && (same_f32(got, ace)
                || (power.is_nan() && ace.is_nan())
                || float_steps(got, ace) <= 1)
    });
    // V231: retail's saturating capacity. ACE's recorded value is the record and
    // must still match wherever ACE's `int` arithmetic did not overflow.
    replay("encumbrance_system_encumbrance_capacity", |i, o| {
        let (strength, augs) = (int(&i["strength"]), int(&i["num_augs"]));
        let got = wo::encumbrance_system_encumbrance_capacity(strength, augs);
        let exact = i64::from(strength.max(0)) * (150 + i64::from(augs.clamp(0, 5)) * 30);
        let ace_overflowed = exact > i64::from(i32::MAX) || augs.checked_mul(30).is_none();
        got == dereth_rules::burden::encumbrance_capacity(strength, augs)
            && (ace_overflowed || got == int(o))
    });
    replay("encumbrance_system_get_burden", |i, o| {
        same_f32(
            burden::load(int(&i["capacity"]), int(&i["encumbrance"])),
            f32_of(o).unwrap(),
        )
    });
    replay("encumbrance_system_get_burden_mod", |i, o| {
        same_f32(
            burden::load_mod(f32_of(&i["burden"]).unwrap()),
            f32_of(o).unwrap(),
        )
    });
}

#[test]
fn the_move_to_heading_matches_aces_a_frame() {
    use empyrean_world::world_objects::creature_navigation::a_frame_get_heading;
    replay("a_frame_get_heading", |i, o| {
        let q = empyrean_common::dotnet::Quaternion::new(
            f32_of(&i["x"]).unwrap(),
            f32_of(&i["y"]).unwrap(),
            f32_of(&i["z"]).unwrap(),
            f32_of(&i["w"]).unwrap(),
        );
        same_f32(a_frame_get_heading(q), f32_of(o).unwrap())
    });
}

/// `(float)MovementSystem.GetRunRate(0.0f, runSkill, 1.0f)`.
fn run_rate_of(run_skill: i32) -> f32 {
    wo::movement_system_get_run_rate(0.0, run_skill, 1.0).cs_cast()
}
