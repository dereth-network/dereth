//! Vectors: fixtures/vectors/weapons_player/
//! Player melee/missile, creature missile, ratings and body parts follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::numerics::Vector3;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::vectors::{self, f64_of, u64_of, Case};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, XpTable};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{
    AttackHeight, AttackType, CombatMode, DamageType, EnchantmentTypeFlags, PowerAccuracy,
    PropertyAttribute, PropertyAttribute2nd, PropertyFloat, PropertyInt, ResistanceType, Skill,
    SkillAdvancementClass, SpellCategory, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::attack_queue::AttackQueue;
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::world_objects::creature_combat::{self, CombatType};
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_skill::CreatureSkill;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::{
    creature_equipment, creature_missile, creature_rating, player_combat, player_melee,
    player_missile,
};
use empyrean_world::World;
use serde_json::Value;

// ------------------------------------------------------------------ the stats area's synthetic dats

fn table_case() -> Case {
    let file = vectors::load_named("stats", "tables");
    file.cases
        .into_iter()
        .next()
        .expect("stats/tables: one case")
}

fn u32s(v: &Value) -> Vec<u32> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|x| u32::try_from(x.as_u64().expect("uint")).expect("u32"))
        .collect()
}

fn formula(v: &Value) -> SkillFormula {
    let g = |k: &str| u32::try_from(v[k].as_u64().expect(k)).expect("u32");
    SkillFormula {
        w: g("w"),
        x: g("x"),
        y: g("y"),
        z: g("z"),
        attr1: g("attr1"),
        attr2: g("attr2"),
    }
}

/// The dats of `stats/tables.json`, as the stats and combat tests build them.
fn dats() -> Arc<DatManager> {
    let out = table_case().output;
    let xp = &out["xp"];
    let xp_table = XpTable {
        id: DataId(file_id::XP_TABLE),
        attribute_xp: u32s(&xp["attribute"]),
        vital_xp: u32s(&xp["vital"]),
        trained_xp: u32s(&xp["trained"]),
        specialized_xp: u32s(&xp["specialized"]),
        level_xp: vec![0, 0, 1000, 2500],
        level_credits: vec![0, 0, 1, 1],
    };
    let retired: Vec<u32> = u32s(&out["retired_added"]);
    let mut skills = BTreeMap::new();
    for (k, v) in out["skills"].as_object().expect("skills") {
        let id: u32 = k.parse().expect("skill id");
        if retired.contains(&id) {
            continue;
        }
        let int = |k: &str| i32::try_from(v[k].as_i64().expect(k)).expect("i32");
        skills.insert(
            id,
            SkillBase {
                description: String::new(),
                name: String::new(),
                icon: 0,
                trained_cost: int("trained_cost"),
                specialized_cost: int("specialized_cost"),
                category: 0,
                chargen_use: 0,
                min_level: u32::try_from(v["min_level"].as_u64().expect("min")).expect("u32"),
                formula: formula(&v["formula"]),
                upper_bound: 0.0,
                lower_bound: 0.0,
                learn_mod: 0.0,
            },
        );
    }
    let skill_table = SkillTable {
        id: DataId(file_id::SKILL_TABLE),
        buckets: 64,
        skills,
    };
    let vitals = &out["vitals"];
    let secondary = SecondaryAttributeTable {
        id: DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: formula(&vitals["health"]),
        stamina: formula(&vitals["stamina"]),
        mana: formula(&vitals["mana"]),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_xp_table(xp_table)
        .with_skill_table(skill_table)
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, secondary)
        .build()
        .expect("fake dats")
}

/// A world over the synthetic dats, with ACE's default settings loaded.
fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats());
    w.content = Arc::new(MemContent::new());
    empyrean_world::managers::property_manager::initialize(&mut w, true);
    w
}

// ------------------------------------------------------------------ objects from vector inputs

fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("not a uint: {v}"))).expect("u32")
}

fn i(v: &Value) -> i32 {
    i32::try_from(v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))).expect("i32")
}

/// A float written as its exact double.
#[allow(clippy::cast_possible_truncation)] // exact: the value is a float widened by the harness
fn f(v: &Value) -> f32 {
    f64_of(v).unwrap_or_else(|| panic!("not a number: {v}")) as f32
}

fn prop_u16(v: &Value) -> u16 {
    u16::try_from(i(v)).expect("u16 property")
}

fn class_of(name: &str) -> Class {
    match name {
        "Player" => Class::Player,
        "Creature" => Class::Creature,
        "MeleeWeapon" => Class::MeleeWeapon,
        "MissileLauncher" => Class::MissileLauncher,
        "Missile" => Class::Missile,
        "Clothing" => Class::Clothing,
        "GenericObject" => Class::GenericObject,
        other => panic!("unknown class {other}"),
    }
}

/// The int and float properties and the registry every spec carries.
fn apply_properties(o: &mut WorldObject, spec: &Value) {
    for p in spec["ints"].as_array().expect("ints") {
        o.set_property(PropertyInt(prop_u16(&p[0])), i(&p[1]));
    }
    for p in spec["floats"].as_array().expect("floats") {
        o.set_property(
            PropertyFloat(prop_u16(&p[0])),
            f64_of(&p[1]).expect("double"),
        );
    }
    let reg = o
        .biota
        .properties_enchantment_registry
        .get_or_insert_with(Vec::new);
    for e in spec
        .get("enchantments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        reg.push(PropertiesEnchantmentRegistry {
            spell_id: i(&e[0]),
            layer_id: 1,
            spell_category: SpellCategory(u(&e[1])),
            power_level: u(&e[2]),
            stat_mod_type: EnchantmentTypeFlags(u(&e[3]).cast_signed()),
            stat_mod_key: u(&e[4]),
            stat_mod_value: f(&e[5]),
            ..PropertiesEnchantmentRegistry::default()
        });
    }
}

const GEAR: [PropertyInt; 10] = [
    PropertyInt::GearDamage,
    PropertyInt::GearDamageResist,
    PropertyInt::GearCrit,
    PropertyInt::GearCritResist,
    PropertyInt::GearCritDamage,
    PropertyInt::GearCritDamageResist,
    PropertyInt::GearHealingBoost,
    PropertyInt::GearMaxHealth,
    PropertyInt::GearPKDamageRating,
    PropertyInt::GearPKDamageResistRating,
];

/// Builds a creature or player (and its equipment, joined to the rating cache as
/// `TryEquipObject` does) from a `weapons_player` spec into the world.
fn combatant_in(w: &mut World, spec: &Value) -> ObjectGuid {
    let mut o = WorldObject::allocate(class_of(spec["class"].as_str().expect("class")));
    let id = u(&spec["id"]);
    o.guid = ObjectGuid::new(id);
    o.biota.id = id;
    o.biota.weenie_class_id = u(&spec["wcid"]);
    o.biota.weenie_type = WeenieType(u(&spec["weenie_type"]));

    let mut attributes = DotNetDict::new();
    for (k, a) in spec["attributes"]
        .as_array()
        .expect("attributes")
        .iter()
        .enumerate()
    {
        attributes.insert(
            PropertyAttribute(u16::try_from(k + 1).expect("attr")),
            PropertiesAttribute {
                init_level: u(&a[0]),
                level_from_cp: u(&a[1]),
                cp_spent: 0,
            },
        );
    }
    o.biota.properties_attribute = Some(attributes);
    let mut vitals = DotNetDict::new();
    for (k, v) in spec["vitals"]
        .as_array()
        .expect("vitals")
        .iter()
        .enumerate()
    {
        vitals.insert(
            PropertyAttribute2nd(u16::try_from(2 * k + 1).expect("vital")),
            PropertiesAttribute2nd {
                init_level: u(&v[0]),
                level_from_cp: u(&v[1]),
                cp_spent: 0,
                current_level: u(&v[2]),
            },
        );
    }
    o.biota.properties_attribute_2nd = Some(vitals);
    let mut skills = DotNetDict::new();
    for s in spec["skills"].as_array().expect("skills") {
        skills.insert(
            Skill(i(&s[0])),
            PropertiesSkill {
                sac: SkillAdvancementClass(u(&s[1])),
                init_level: u(&s[2]),
                level_from_pp: u16::try_from(u(&s[3])).expect("ranks"),
                ..PropertiesSkill::default()
            },
        );
    }
    o.biota.properties_skill = Some(skills);
    apply_properties(&mut o, spec);

    // the stat part of Creature.SetEphemeralValues, as the harness builds it
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        let cv = CreatureVital::new(&mut o, v);
        o.vitals_mut().insert(v, cv);
    }
    for a in 1..=6u16 {
        let ca = CreatureAttribute::new(&mut o, PropertyAttribute(a));
        o.attributes_mut().insert(PropertyAttribute(a), ca);
    }
    let skill_ids: Vec<Skill> = o
        .biota
        .properties_skill
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    for s in skill_ids {
        o.skills_mut().insert(s, CreatureSkill::new(s));
    }

    if let Some(state) = spec.get("state") {
        let stance = empyrean_entity::enums::MotionStance(u(&state["stance"]));
        o.wo.world_object_properties.current_motion_state = Some(Motion::from_stance(stance));
        let fields = creature_combat::fields_mut(&mut o);
        fields.combat_mode = CombatMode(i(&state["combat_mode"]));
        fields.attack_type = AttackType(i(&state["attack_type"]));
        let player = o.player.as_mut().expect("a player");
        player.player_melee.power_level = f(&state["power_level"]);
        player.player_missile.accuracy_level = f(&state["accuracy_level"]);
        player.player.is_logging_out = state["is_logging_out"].as_bool().expect("bool");
    }

    // equipment, in EquippedObjects order
    for item in spec["equipped"].as_array().expect("equipped") {
        let mut io = WorldObject::allocate(class_of(item["class"].as_str().expect("class")));
        let iid = u(&item["id"]);
        io.guid = ObjectGuid::new(iid);
        io.biota.id = iid;
        io.biota.weenie_class_id = u(&item["wcid"]);
        io.biota.weenie_type = WeenieType(u(&item["weenie_type"]));
        apply_properties(&mut io, item);
        let gear: Vec<i32> = GEAR
            .iter()
            .map(|&g| io.get_property(g).unwrap_or(0))
            .collect();
        let equipment = &mut o.creature.as_mut().expect("creature").creature_equipment;
        equipment.equipped_objects.insert(io.guid, ());
        // Creature.AddItemToEquippedItemsRatingCache
        if gear.iter().any(|&g| g != 0) {
            let cache = equipment
                .equipped_items_rating_cache
                .get_or_insert_with(|| {
                    let mut d = DotNetDict::new();
                    for g in GEAR {
                        d.insert(g, 0);
                    }
                    d
                });
            for (k, g) in GEAR.iter().zip(gear) {
                *cache.get_mut(k).expect("key") += g;
            }
        }
        w.objects.insert(io).expect("fresh item guid");
    }

    let g = o.guid;
    let is_player = o.is_player();
    w.objects.insert(o).expect("fresh guid");
    if is_player {
        give_walkable_body(w, g);
    }
    g
}

/// As the ACE harness builds them: a player has a `PhysicsObj` that is `OnWalkable`
/// (`Player.IsJumping` reads it).
fn give_walkable_body(w: &mut World, g: ObjectGuid) {
    let h = empyrean_world::physics::phys_ext::make_object(w, 0, g.full(), true);
    w.physics
        .get_mut(h)
        .expect("the new body")
        .transient_state
        .set_on_walkable_bit(true);
    w.objects.get_mut(g).expect("the player").phys = Some(h);
}

// ------------------------------------------------------------------ comparisons

fn num(got: f64, want: &Value) -> bool {
    let w = f64_of(want).unwrap_or_else(|| panic!("not a number: {want}"));
    got.to_bits() == w.to_bits() || got.is_nan() && w.is_nan()
}

/// Replays `weapons_player/<name>.json`, each case in a fresh world; answers the case count.
fn replay(name: &str, each: impl FnMut(&mut World, &Case) -> Vec<Value>) -> usize {
    replay_where(name, |_| true, each)
}

/// [`replay`] over the cases `keep` accepts; answers how many ran.
fn replay_where(
    name: &str,
    keep: impl Fn(&Case) -> bool,
    mut each: impl FnMut(&mut World, &Case) -> Vec<Value>,
) -> usize {
    let file = vectors::load_named("weapons_player", name);
    let mut failures = Vec::new();
    let mut ran = 0;
    for (n, case) in file.cases.iter().enumerate() {
        if !keep(case) {
            continue;
        }
        ran += 1;
        let mut w = world();
        let got = each(&mut w, case);
        let want = case
            .output
            .as_array()
            .unwrap_or_else(|| panic!("{name}[{n}]: out is {}", case.output));
        assert_eq!(got.len(), want.len(), "{name}[{n}]: output width");
        for (k, (g, e)) in got.iter().zip(want).enumerate() {
            let same = match (g, e) {
                (Value::Number(a), b) if b.is_number() || b.is_string() => {
                    num(a.as_f64().expect("num"), b)
                }
                (a, b) => a == b,
            };
            if !same {
                failures.push(format!("{name}[{n}] out[{k}]: got {g}, ACE {e}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    ran
}

fn jf(x: f32) -> Value {
    Value::from(f64::from(x))
}

// ------------------------------------------------------------------ vector replays

#[test]
fn rating_getters_match_ace() {
    let n = replay("ratings", |w, case| {
        let g = combatant_in(w, &case.input);
        vec![
            creature_rating::get_damage_rating(w, g).into(),
            creature_rating::get_damage_resist_rating(w, g, None, true).into(),
            creature_rating::get_damage_resist_rating(w, g, Some(CombatType::Melee), true).into(),
            creature_rating::get_damage_resist_rating(w, g, Some(CombatType::Missile), true).into(),
            creature_rating::get_damage_resist_rating(w, g, Some(CombatType::Magic), true).into(),
            creature_rating::get_damage_resist_rating(w, g, Some(CombatType::Melee), false).into(),
            jf(creature_rating::get_damage_resist_rating_mod(
                w, g, None, true,
            )),
            jf(creature_rating::get_damage_resist_rating_mod(
                w,
                g,
                Some(CombatType::Melee),
                true,
            )),
            creature_rating::get_spec_defense_bonus(w, g, None).into(),
            creature_rating::get_spec_defense_bonus(w, g, Some(CombatType::Melee)).into(),
            creature_rating::get_spec_defense_bonus(w, g, Some(CombatType::Missile)).into(),
            creature_rating::get_spec_defense_bonus(w, g, Some(CombatType::Magic)).into(),
            creature_rating::get_crit_rating(w, g).into(),
            creature_rating::get_crit_damage_rating(w, g).into(),
            creature_rating::get_crit_resist_rating(w, g).into(),
            creature_rating::get_crit_damage_resist_rating(w, g).into(),
            creature_rating::get_healing_boost_rating(w, g).into(),
            creature_rating::get_healing_resist_rating(w, g).into(),
            jf(creature_rating::get_healing_rating_mod(w, g)),
            creature_rating::get_life_resist_rating(w, g).into(),
            jf(creature_rating::get_life_resist_rating_mod(w, g)),
            creature_rating::get_dot_resistance_rating(w, g).into(),
            creature_rating::get_nether_resist_rating(w, g).into(),
            creature_rating::get_gear_max_health(w, g).into(),
            creature_rating::get_pk_damage_rating(w, g).into(),
            creature_rating::get_pk_damage_resist_rating(w, g).into(),
            creature_rating::get_gear_pk_damage_rating(w, g).into(),
            creature_rating::get_gear_pk_damage_resist_rating(w, g).into(),
            creature_rating::get_item_mana_reduction_rating(w, g).into(),
            creature_rating::get_mana_charge_rating(w, g).into(),
        ]
    });
    assert_eq!(n, 240);
}

/// Divergence: V393
/// ACE's rating vectors, whose combatants carry ratings at the end of retail, have none in an era
/// without ratings: every resistance, critical, healing, life, damage-over-time and nether rating
/// is 0 and each modifier 1.
#[test]
fn an_era_without_ratings_reads_every_rating_as_none() {
    let file = vectors::load_named("weapons_player", "ratings");
    let mut had_ratings = 0;
    for case in &file.cases {
        let mut w = world();
        let g = combatant_in(&mut w, &case.input);
        let eor: i32 = creature_rating::get_crit_rating(&mut w, g)
            .abs()
            .max(creature_rating::get_damage_resist_rating(&mut w, g, None, true).abs())
            .max(creature_rating::get_healing_boost_rating(&mut w, g).abs());
        had_ratings += usize::from(eor != 0);
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        for c in [None, Some(CombatType::Melee), Some(CombatType::Magic)] {
            assert_eq!(
                creature_rating::get_damage_resist_rating(&mut w, g, c, true),
                0
            );
            assert_eq!(
                creature_rating::get_damage_resist_rating_mod(&mut w, g, c, true),
                1.0
            );
        }
        for r in [
            creature_rating::get_crit_rating(&mut w, g),
            creature_rating::get_crit_damage_rating(&mut w, g),
            creature_rating::get_crit_resist_rating(&mut w, g),
            creature_rating::get_crit_damage_resist_rating(&mut w, g),
            creature_rating::get_healing_boost_rating(&mut w, g),
            creature_rating::get_healing_resist_rating(&mut w, g),
            creature_rating::get_life_resist_rating(&mut w, g),
            creature_rating::get_dot_resistance_rating(&mut w, g),
            creature_rating::get_nether_resist_rating(&mut w, g),
        ] {
            assert_eq!(r, 0);
        }
        assert_eq!(creature_rating::get_healing_rating_mod(&mut w, g), 1.0);
    }
    assert!(had_ratings > 0, "the vectors exercise ratings");
}

/// Divergence: V394
/// Assessing an unattackable creature shows its armour levels at the end of retail and none (nor
/// ratings) in an era before assessment showed them.
#[test]
fn an_era_before_assessed_armor_levels_shows_none() {
    use empyrean_world::network::structure::appraise_info::appraise_info_new;
    let file = vectors::load_named("weapons_player", "ratings");
    let mut shown = 0;
    for case in file
        .cases
        .iter()
        .filter(|c| c.input["class"].as_str() == Some("Creature"))
    {
        let mut w = world();
        let g = combatant_in(&mut w, &case.input);
        w.objects
            .get_mut(g)
            .expect("built")
            .set_property(empyrean_entity::enums::PropertyBool::Attackable, false);
        let examiner = ObjectGuid::new(0x5000_0FFF);
        if appraise_info_new(&mut w, g, examiner, true)
            .armor_levels
            .is_none()
        {
            continue;
        }
        shown += 1;
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        let old = appraise_info_new(&mut w, g, examiner, true);
        assert!(old.armor_levels.is_none());
        assert!(old.creature_profile.is_some(), "the rest is still shown");
    }
    assert!(shown > 0, "some assessed creature shows armour levels");
}

/// Divergence: V395
/// A player's old weapon skill converts to a consolidated one at the end of retail (a bow to
/// Missile Weapons) and stays itself before the 2012 consolidation.
#[test]
fn a_players_old_weapon_skill_is_its_own_before_the_consolidation() {
    use empyrean_world::world_objects::world_object::convert_to_mo_a_skill;
    let file = vectors::load_named("weapons_player", "ratings");
    let case = file
        .cases
        .iter()
        .find(|c| c.input["class"].as_str() == Some("Player"))
        .expect("a player case");
    let mut w = world();
    let g = combatant_in(&mut w, &case.input);
    assert_eq!(
        convert_to_mo_a_skill(&mut w, g, Skill::Bow),
        Skill::MissileWeapons
    );
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    for s in [Skill::Axe, Skill::Sword, Skill::Bow, Skill::ThrownWeapon] {
        assert_eq!(convert_to_mo_a_skill(&mut w, g, s), s);
    }
}

/// A player of `player_mods`' first case (a melee weapon wielded) with Strength 100 and
/// Coordination 150, a trained Unarmed Combat of 200 ranks, and `edit` applied to its spec.
fn era_player(w: &mut World, edit: impl FnOnce(&mut Value)) -> ObjectGuid {
    let file = vectors::load_named("weapons_player", "player_mods");
    let mut spec = file.cases[0].input.clone();
    spec["attributes"][0] = serde_json::json!([100, 0]);
    spec["attributes"][3] = serde_json::json!([150, 0]);
    spec["skills"]
        .as_array_mut()
        .expect("skills")
        .push(serde_json::json!([13, 2, 200, 0]));
    edit(&mut spec);
    combatant_in(w, &spec)
}

/// The spec's wielded weapon.
fn wielded(spec: &Value) -> ObjectGuid {
    ObjectGuid::new(u(&spec["equipped"][0]["id"]))
}

/// Divergence: V395, V402
/// Before the weapon-skill consolidation a dagger's damage scales with Coordination (at the end
/// of retail with Strength, only Finesse Weapons taking Coordination); an empty-handed player
/// fights with Unarmed Combat, scales with Strength at 0.004 a point rather than 0.011, adds a
/// twentieth of the skill to its maximum damage, and a bare kick does 1 rather than 2.
#[test]
fn an_era_before_the_consolidation_uses_the_older_melee_damage() {
    use empyrean_world::world_objects::skill_formula;
    let dagger = |spec: &mut Value| {
        for p in spec["equipped"][0]["ints"].as_array_mut().expect("ints") {
            if p[0] == 48 {
                p[1] = serde_json::json!(4);
            }
        }
    };
    let mut w = world();
    let mut weapon = None;
    let g = era_player(&mut w, |s| {
        dagger(s);
        weapon = Some(wielded(s));
    });
    let weapon = weapon.expect("a weapon");
    assert_eq!(
        creature_combat::get_attribute_mod(&mut w, g, Some(weapon)),
        skill_formula::get_attribute_mod(100, false),
        "Strength at the end of retail"
    );
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(
        creature_combat::get_attribute_mod(&mut w, g, Some(weapon)),
        skill_formula::get_attribute_mod(150, false),
        "Coordination for a dagger"
    );
    assert_eq!(
        creature_combat::get_unarmed_skill_damage_bonus(&mut w, g),
        0
    );

    let mut w = world();
    let g = era_player(&mut w, |s| s["equipped"] = serde_json::json!([]));
    let eor_skill = player_combat::get_current_weapon_skill(&mut w, g);
    assert_ne!(eor_skill, Skill::UnarmedCombat, "the highest melee skill");
    let eor_mod = creature_combat::get_attribute_mod(&mut w, g, None);
    assert_eq!(eor_mod, skill_formula::get_attribute_mod(100, false));
    assert_eq!(
        creature_combat::get_unarmed_skill_damage_bonus(&mut w, g),
        0
    );
    let eor_kick = player_combat::get_base_damage_mod(&mut w, g, g);
    assert_eq!(eor_kick.base_damage.max_damage, 2);

    w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(
        player_combat::get_current_weapon_skill(&mut w, g),
        Skill::UnarmedCombat
    );
    let older = creature_combat::get_attribute_mod(&mut w, g, None);
    assert_eq!(older, 1.0 + 45.0 * 0.004);
    assert!(older < eor_mod);
    let unarmed = empyrean_world::world_objects::world_object_weapon::SkillOf::get(
        &mut w,
        g,
        Skill::UnarmedCombat,
    )
    .current(&mut w);
    assert!(unarmed >= 200);
    assert_eq!(
        creature_combat::get_unarmed_skill_damage_bonus(&mut w, g),
        i32::try_from(unarmed / 20).expect("small"),
        "a twentieth of the skill"
    );
    let kick = player_combat::get_base_damage_mod(&mut w, g, g);
    assert_eq!(kick.base_damage.max_damage, 1, "a bare kick");
}

/// Divergence: V401
/// A multiplicative weapon-speed enchantment (Rockslide, half the time) changes nothing at the
/// end of retail, which reads only the additive ones, and halves the weapon's speed value before
/// Throne of Destiny, both in the attack and in the profile an assessment shows.
#[test]
fn an_era_with_multiplicative_weapon_speed_scales_the_weapon_by_it() {
    use empyrean_world::network::structure::weapon_profile::weapon_profile_new;
    use empyrean_world::world_objects::world_object_weapon;
    let mut w = world();
    let mut weapon = None;
    let g = era_player(&mut w, |s| {
        let item = &mut s["equipped"][0];
        item["ints"]
            .as_array_mut()
            .expect("ints")
            .push(serde_json::json!([49, 40]));
        item["enchantments"]
            .as_array_mut()
            .expect("enchantments")
            .push(serde_json::json!([2439, 0, 1, 0x5004, 49, 0.5]));
        weapon = Some(wielded(s));
    });
    let weapon = weapon.expect("a weapon");
    assert_eq!(world_object_weapon::get_weapon_speed(&mut w, Some(g)), 40);
    assert_eq!(weapon_profile_new(&w, weapon).weapon_time, 40);

    w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(world_object_weapon::get_weapon_speed(&mut w, Some(g)), 20);
    assert_eq!(weapon_profile_new(&w, weapon).weapon_time, 20);
}

/// Divergence: V408
/// An era without the heritage masteries gives no mastery bonus, where the end of
/// retail (with universal masteries) gives it for any masterable weapon.
#[test]
fn an_era_without_masteries_gives_no_heritage_bonus() {
    use empyrean_world::world_objects::player_skills;
    let mut w = world();
    let mut weapon = None;
    let g = era_player(&mut w, |s| weapon = Some(wielded(s)));
    let weapon = weapon.expect("a weapon");
    let eor = player_skills::player_get_heritage_bonus(&w, g, weapon);
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert!(!player_skills::player_get_heritage_bonus(&w, g, weapon));
    assert!(eor, "the end of retail's universal masteries");
}

/// Divergence: V404
/// A player without the Shield skill, a 200-level shield (absorbing a quarter of magic) on its
/// arm and a creature in front: at the end of retail the skill caps the shield to nothing in
/// melee stance and a peaceful stance drops it altogether; before the Shield skill it counts in
/// full in any stance, and absorbs magic as a missile launcher does, by Magic Defense.
#[test]
fn an_era_before_the_shield_skill_counts_a_shield_in_full_in_any_stance() {
    use empyrean_entity::Position;
    use empyrean_world::world_objects::{skill_formula, spell_projectile};
    const SHIELD: u32 = 0x5000_1F00;
    let mut w = world();
    let g = era_player(&mut w, |s| {
        s["equipped"]
            .as_array_mut()
            .expect("equipped")
            .push(serde_json::json!({
                "class": "GenericObject", "id": SHIELD, "wcid": 44, "weenie_type": 1,
                "ints": [[10, 0x0020_0000], [51, 4], [28, 200]],
                "floats": [[13, 1.0], [159, 0.25]], "enchantments": [],
            }));
        s["skills"]
            .as_array_mut()
            .expect("skills")
            .push(serde_json::json!([15, 3, 400, 0]));
    });
    let file = vectors::load_named("weapons_player", "ratings");
    let foe = combatant_in(
        &mut w,
        &file
            .cases
            .iter()
            .find(|c| c.input["class"].as_str() == Some("Creature"))
            .expect("a creature")
            .input,
    );
    let at = Position::from_components(0xA9B4_0001, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0, 1.0, false);
    for o in [g, foe] {
        w.objects.get_mut(o).expect("placed").set_location(Some(at));
    }
    let shield = ObjectGuid::new(SHIELD);
    let set_mode = |w: &mut World, m: CombatMode| {
        creature_combat::fields_mut(w.objects.get_mut(g).expect("the player")).combat_mode = m;
    };
    let full = skill_formula::calc_armor_mod(200.0);

    set_mode(&mut w, CombatMode::Melee);
    assert_eq!(
        creature_combat::get_shield_mod(&mut w, g, foe, DamageType::Slash, None),
        1.0,
        "no Shield skill, no shield"
    );
    set_mode(&mut w, CombatMode::NonCombat);
    assert_eq!(
        creature_combat::get_shield_mod(&mut w, g, foe, DamageType::Slash, None),
        1.0
    );

    w.era = empyrean_common::era::EraId::Infiltration.rules();
    assert_eq!(
        creature_combat::get_shield_mod(&mut w, g, foe, DamageType::Slash, None),
        full,
        "in a peaceful stance too"
    );
    set_mode(&mut w, CombatMode::Melee);
    assert_eq!(
        creature_combat::get_shield_mod(&mut w, g, foe, DamageType::Slash, None),
        full
    );
    let absorb = spell_projectile::get_absorb_mod(&mut w, foe, g);
    assert_eq!(absorb, spell_projectile::absorb_magic(&mut w, g, shield));
    assert!(absorb < 1.0, "a quarter of the magic, by Magic Defense");
}

#[test]
fn rating_modifiers_match_ace() {
    let n = replay("rating_mods", |_, case| {
        let r = i(&case.input["rating"]);
        [
            creature_rating::get_damage_rating_int(r),
            creature_rating::get_critical_damage_rating(r),
            creature_rating::get_damage_resistance_rating(r),
            creature_rating::get_critical_damage_resistance_rating(r),
            creature_rating::get_damage_over_time_resistance_rating(r),
            creature_rating::get_health_drain_resistance_rating(r),
            creature_rating::get_healing_boost_rating_int(r),
            creature_rating::get_aetheria_surge_rating(r),
            creature_rating::get_mana_charge_rating_int(r),
            creature_rating::get_mana_reduction_rating(r),
            creature_rating::get_damage_reduction_rating(r),
            creature_rating::get_healing_reduction_rating(r),
            creature_rating::get_damage_resistance_reduction_rating(r),
            creature_rating::get_pk_damage_rating_int(r),
            creature_rating::get_pk_damage_resistance_rating(r),
        ]
        .into_iter()
        .map(jf)
        .collect()
    });
    assert_eq!(n, 247);
}

#[test]
fn player_modifiers_match_ace() {
    let n = replay("player_mods", player_modifiers);
    assert_eq!(n, 240);
}

fn player_modifiers(w: &mut World, case: &Case) -> Vec<Value> {
    {
        let p = combatant_in(w, &case.input);
        let weapon = creature_equipment::get_equipped_weapon(w, p, false);
        let mut out: Vec<Value> = vec![
            jf(player_melee::power_level(w, p)),
            jf(player_missile::accuracy_level(w, p)),
            jf(player_combat::get_power_mod(w, p, weapon)),
            jf(player_combat::get_accuracy_mod(w, p, weapon)),
            jf(player_combat::get_power_mod(w, p, None)),
            jf(player_combat::get_accuracy_mod(w, p, None)),
            player_melee::get_power_range(w, p).0.into(),
            player_missile::get_accuracy_range(w, p).0.into(),
            jf(player_combat::get_power_accuracy_bar(w, p)),
            jf(player_combat::get_recklessness_mod(w, p)),
            jf(player_combat::get_stamina_mod(w, p)),
            player_combat::get_held_item_burden(w, p).into(),
            player_combat::get_attack_stamina(w, p, PowerAccuracy::Low).into(),
            player_combat::get_attack_stamina(w, p, PowerAccuracy::Medium).into(),
            player_combat::get_attack_stamina(w, p, PowerAccuracy::High).into(),
        ];
        for t in case.input["natural_types"].as_array().expect("types") {
            out.push(jf(player_combat::get_natural_resistance(
                w,
                p,
                DamageType(i(t)),
            )));
        }
        out.push(player_combat::get_natural_resistance_string(w, p, ResistanceType::Slash).into());
        out.push(player_combat::get_regen_bonus_string(w, p).into());
        out.push(
            match player_combat::get_combat_type(w, p) {
                CombatType::Melee => 0,
                CombatType::Missile => 1,
                CombatType::Magic => 2,
            }
            .into(),
        );
        out.push(player_combat::get_current_weapon_skill(w, p).0.into());
        out.push(player_combat::get_highest_melee_skill(w, p).0.into());
        out.push(player_combat::get_current_attack_skill(w, p).0.into());
        out.push(player_combat::get_effective_attack_skill(w, p).into());
        out.push(jf(player_combat::get_defense_stance_mod(w, p)));
        out.push(player_combat::is_pk_death(w, p, None).into());
        out.push(player_combat::is_pk_death(w, p, Some(p.full())).into());
        out.push(player_combat::is_pk_death(w, p, Some(0x5000_0001)).into());
        out.push(player_combat::is_pk_death(w, p, Some(0x8000_0001)).into());
        out.push(player_combat::is_pk_lite_death(w, p, None).into());
        out.push(player_combat::is_pk_lite_death(w, p, Some(0x5000_0001)).into());
        out
    }
}

#[test]
fn aim_levels_and_2d_directions_match_ace() {
    let v3 = |v: &Value| Vector3::new(f(&v[0]), f(&v[1]), f(&v[2]));
    let n = replay("aim_level", |_, case| {
        let level = creature_missile::get_aim_level(v3(&case.input["v"]));
        let dir = creature_missile::get_dir_2d(v3(&case.input["a"]), v3(&case.input["b"]));
        vec![
            level.0.into(),
            Value::Array(vec![jf(dir.x), jf(dir.y), jf(dir.z)]),
        ]
    });
    assert_eq!(n, 381);
}

#[test]
fn missile_weapon_helpers_match_ace() {
    let n = replay("missile_weapons", |w, case| {
        let c = combatant_in(w, &case.input);
        let missile = creature_equipment::get_equipped_missile_weapon(w, c);

        // the player: only AttackHeight is read by GetAimHeight
        let mut p = WorldObject::allocate(Class::Player);
        p.guid = ObjectGuid::new(0x5000_0FF0);
        w.objects.insert(p).expect("fresh");
        let height = case.input["attack_height"]
            .as_i64()
            .map(|h| AttackHeight(i32::try_from(h).expect("height")));
        player_melee::set_attack_height(w, ObjectGuid::new(0x5000_0FF0), height);
        let aim_height = {
            let w: &World = w;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                player_missile::get_aim_height(w, ObjectGuid::new(0x5000_0FF0), c)
            }))
        };

        vec![
            jf(creature_missile::get_max_missile_range(w, c)),
            jf(creature_missile::get_projectile_speed(w, c)),
            missile.map_or(Value::Null, |m| {
                creature_missile::get_launch_missile_sound(w, m).0.into()
            }),
            aim_height.map_or_else(
                |_| serde_json::json!({"throws": "System.InvalidOperationException"}),
                jf,
            ),
        ]
    });
    assert_eq!(n, 160);
}

// ------------------------------------------------------------------ AttackQueue

/// `AttackQueue.Fetch`: the queued level repeats until another is queued behind it; an empty queue
/// answers 0.5 (read from `Entity/AttackQueue.cs`).
#[test]
fn attack_queue_fetch_keeps_the_last_level() {
    let mut q = AttackQueue::new(ObjectGuid::new(0x5000_0001));
    assert_eq!(q.fetch(), 0.5, "empty");
    q.add(0.25);
    assert_eq!(q.fetch(), 0.25);
    assert_eq!(q.fetch(), 0.25, "the last level repeats");
    q.add(0.75);
    q.add(1.0);
    assert_eq!(q.fetch(), 0.75, "drops one, answers the next");
    assert_eq!(q.fetch(), 1.0);
    assert_eq!(q.fetch(), 1.0);
    q.clear();
    assert_eq!(q.fetch(), 0.5);
}

// ------------------------------------------------------------------ scenarios (no physics)

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{PlayerKillerStatus, WeenieErrorWithString as E};
use empyrean_net::SessionId;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::sessions::SessionData;
use serde_json::json;

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const GAME_EVENT: u32 = 0xF7B0;

/// A combatant spec: every attribute 100 (Endurance `endurance`), full vitals, Melee Defense at
/// `sac`, the PK status, in melee mode or not.
fn spec(
    class: &str,
    id: u32,
    status: Option<PlayerKillerStatus>,
    endurance: u32,
    sac: u32,
    melee: bool,
) -> Value {
    let ints = status.map_or_else(Vec::new, |s| {
        vec![json!([PropertyInt::PlayerKillerStatus.0, s.0])]
    });
    let mut v = json!({
        "class": class, "id": id, "wcid": 1, "weenie_type": 10,
        "attributes": [[100, 0], [endurance, 0], [100, 0], [100, 0], [100, 0], [100, 0]],
        "vitals": [[100, 0, 100], [100, 0, 100], [50, 0, 50]],
        "skills": [[Skill::MeleeDefense.0, sac, 200, 0]],
        "ints": ints, "floats": [], "equipped": [],
    });
    if class == "Player" {
        v["state"] = json!({
            "stance": empyrean_entity::enums::MotionStance::NonCombat.0,
            "combat_mode": if melee { CombatMode::Melee.0 } else { CombatMode::NonCombat.0 },
            "attack_type": 0, "power_level": 0.5, "accuracy_level": 0.5, "is_logging_out": false,
        });
    }
    v
}

fn with_experience(w: &mut World, g: ObjectGuid) {
    let o = w.objects.get_mut(g).expect("live");
    o.set_property(PropertyInt::Level, 1);
    o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
    o.set_property(
        empyrean_entity::enums::PropertyInt64::AvailableExperience,
        0,
    );
}

/// Both at the same spot (the house restriction passes on the same cell).
fn place(w: &mut World, g: ObjectGuid) {
    let pos = empyrean_entity::Position::from_components(
        0xA9B4_0014,
        96.0,
        96.0,
        10.0,
        1.0,
        0.0,
        0.0,
        0.0,
        false,
    );
    w.objects.get_mut(g).expect("live").set_location(Some(pos));
}

fn with_session(w: &mut World, player: ObjectGuid) {
    let mut s = SessionData::default();
    s.set_player(Some(player));
    // an authenticated session (`Session.AccountId`: the squelch checks read it)
    s.set_account(
        1,
        "acct".to_owned(),
        empyrean_entity::enums::AccessLevel::Player,
    );
    w.sessions.insert(S, s);
}

fn opcodes(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<u32> {
    sent.iter()
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == GAME_EVENT {
                word(12)
            } else {
                word(0)
            }
        })
        .collect()
}

/// `Player.CheckPKStatusVsTarget(target, null)` against every pair of statuses (read from
/// `Player_Combat.cs`): Free on either side passes; an NPK attacker or target is refused with its
/// pair of errors; different PK types are refused; a monster with a non-default status must match.
#[test]
fn pk_status_checks_follow_aces_table() {
    use PlayerKillerStatus as K;
    let statuses = [K::NPK, K::PK, K::PKLite, K::Free];
    for own in statuses {
        for other in statuses {
            let mut w = world();
            let a = combatant_in(
                &mut w,
                &spec("Player", 0x5000_0001, Some(own), 100, 2, true),
            );
            let b = combatant_in(
                &mut w,
                &spec("Player", 0x5000_0002, Some(other), 100, 2, true),
            );
            place(&mut w, a);
            place(&mut w, b);
            let got = player_combat::check_pk_status_vs_target(&mut w, a, Some(b), None);
            let want = if own == K::Free || other == K::Free {
                None
            } else if own == K::NPK {
                Some(vec![
                    E::YouFailToAffect_YouAreNotPK,
                    E::_FailsToAffectYou_TheyAreNotPK,
                ])
            } else if other == K::NPK {
                Some(vec![
                    E::YouFailToAffect_TheyAreNotPK,
                    E::_FailsToAffectYou_YouAreNotPK,
                ])
            } else if own != other {
                Some(vec![
                    E::YouFailToAffect_NotSamePKType,
                    E::_FailsToAffectYou_NotSamePKType,
                ])
            } else {
                None
            };
            assert_eq!(got, want, "{own:?} vs player {other:?}");

            // a monster: the default (NPK) passes; any other status must match the player's
            let m = combatant_in(
                &mut w,
                &spec(
                    "Creature",
                    0x8000_0001,
                    (other != K::NPK).then_some(other),
                    100,
                    2,
                    false,
                ),
            );
            let got = player_combat::check_pk_status_vs_target(&mut w, a, Some(m), None);
            let want = if own == K::Free || other == K::Free || other == K::NPK || own == other {
                None
            } else {
                Some(vec![
                    E::YouFailToAffect_NotSamePKType,
                    E::_FailsToAffectYou_NotSamePKType,
                ])
            };
            assert_eq!(got, want, "{own:?} vs monster {other:?}");
        }
    }
    let mut w = world();
    let a = combatant_in(&mut w, &spec("Player", 0x5000_0001, None, 100, 2, true));
    assert_eq!(
        player_combat::check_pk_status_vs_target(&mut w, a, Some(a), None),
        None,
        "self"
    );
    assert_eq!(
        player_combat::check_pk_status_vs_target(&mut w, a, None, None),
        None,
        "no target"
    );
}

/// `Player.TakeDamageOverTime`: the rounded amount off Health, the periodic-damage chat line
/// (nether: its own text, in the Magic channel).
#[test]
fn damage_over_time_reports_the_periodic_damage() {
    use dereth_protocol::comms::CommunicationTextboxString;
    for (damage_type, text, chat) in [
        (
            DamageType::Fire,
            "You receive 10 points of periodic damage.",
            empyrean_entity::enums::ChatMessageType::Combat.0,
        ),
        (
            DamageType::Nether,
            "You receive 10 points of periodic nether damage.",
            empyrean_entity::enums::ChatMessageType::Magic.0,
        ),
    ] {
        let mut w = world();
        let p = combatant_in(&mut w, &spec("Player", 0x5000_0001, None, 100, 2, true));
        with_session(&mut w, p);
        start_capture();
        player_combat::take_damage_over_time(&mut w, p, 10.4, damage_type);
        let sent = take_sent();
        let o = w.objects.get(p).unwrap();
        assert_eq!(o.health().current(o), 90);
        let chat_msg = sent
            .iter()
            .find(|(_, _, b)| b.starts_with(&0xF7E0u32.to_le_bytes()))
            .expect("a chat line");
        let m: CommunicationTextboxString = dereth_protocol::read_body_padded(&chat_msg.2[4..])
            .expect("dereth-protocol decodes it");
        assert_eq!(
            (m.text.as_str(), m.text_type),
            (text, chat),
            "{damage_type:?}"
        );
    }
}

/// `Player.OnEvade`: the last attacker is set, the evasion message is sent; in combat mode an
/// untrained defense always costs a stamina point, a trained one only when the Endurance chance
/// (`0.000005 e^2 + 0.00124 e - 0.07`, clamped to 0.75) fails its roll; out of combat nothing.
#[test]
fn an_evasion_costs_stamina_unless_endurance_saves_it() {
    let stamina = |w: &World, g: ObjectGuid| {
        let o = w.objects.get(g).unwrap();
        o.stamina().current(o)
    };
    for (melee, sac, endurance) in [
        (false, 2, 100),
        (true, 1, 100),
        (true, 2, 100),
        (true, 3, 300),
    ] {
        for seed in 0..20u64 {
            let mut w = world();
            let p = combatant_in(
                &mut w,
                &spec("Player", 0x5000_0001, None, endurance, sac, melee),
            );
            with_experience(&mut w, p);
            let m = combatant_in(&mut w, &spec("Creature", 0x8000_0001, None, 100, 2, false));
            with_session(&mut w, p);
            ThreadSafeRandom::seed(seed);
            start_capture();
            player_combat::on_evade(&mut w, p, m, CombatType::Melee);
            let sent = take_sent();
            let spent = 100 - stamina(&w, p);

            #[allow(clippy::cast_precision_loss)]
            let e = endurance as f32;
            let chance = (e * e * 0.000_005 + e * 0.001_24 - 0.07).clamp(0.0, 0.75);
            ThreadSafeRandom::seed(seed);
            let want = if !melee {
                0
            } else if sac < 2 {
                1
            } else {
                u32::from(f64::from(chance) <= ThreadSafeRandom::next_float(0.0, 1.0))
            };
            assert_eq!(
                spent, want,
                "melee {melee}, sac {sac}, endurance {endurance}, seed {seed}"
            );
            // PrivateUpdateInstanceID(CurrentAttacker), [the stamina update], the evasion message
            let ops = opcodes(&sent);
            assert_eq!(ops.first(), Some(&0x02D9), "{ops:04X?}");
            assert_eq!(ops.last(), Some(&0x01B4), "{ops:04X?}");
            let current_attacker = w
                .objects
                .get(p)
                .unwrap()
                .get_property(empyrean_entity::enums::PropertyInstanceId::CurrentAttacker);
            assert_eq!(current_attacker, Some(m.full()));
        }
    }
}

/// `Player.GetHighestMeleeSkill`: light, then heavy, then finesse, each replacing the best only
/// when strictly higher, so a tie keeps the earlier skill.
#[test]
fn the_highest_melee_skill_keeps_the_first_of_a_tie() {
    for (light, heavy, finesse, want) in [
        (200, 200, 200, Skill::LightWeapons),
        (100, 200, 200, Skill::HeavyWeapons),
        (100, 150, 200, Skill::FinesseWeapons),
        (300, 200, 300, Skill::LightWeapons),
    ] {
        let mut w = world();
        let mut s = spec("Player", 0x5000_0001, None, 100, 2, true);
        s["skills"] = json!([
            [Skill::LightWeapons.0, 2, light, 0],
            [Skill::HeavyWeapons.0, 2, heavy, 0],
            [Skill::FinesseWeapons.0, 2, finesse, 0],
        ]);
        let p = combatant_in(&mut w, &s);
        assert_eq!(
            player_combat::get_highest_melee_skill(&mut w, p),
            want,
            "{light}/{heavy}/{finesse}"
        );
    }
}

/// `Player.DamageTarget` against a player: an evaded attack tells the defender through its
/// `OnEvade` (the last attacker and the evasion message on the defender's session).
#[test]
fn an_evaded_player_attack_reaches_the_defenders_on_evade() {
    const S2: SessionId = SessionId {
        client_id: 2,
        generation: 1,
    };
    let mut evaded = 0;
    for seed in 0..40u64 {
        let mut w = world();
        let a = combatant_in(
            &mut w,
            &spec(
                "Player",
                0x5000_0001,
                Some(PlayerKillerStatus::PK),
                100,
                2,
                true,
            ),
        );
        let d = combatant_in(
            &mut w,
            &spec(
                "Player",
                0x5000_0002,
                Some(PlayerKillerStatus::PK),
                100,
                3,
                true,
            ),
        );
        with_experience(&mut w, a);
        with_experience(&mut w, d);
        place(&mut w, a);
        place(&mut w, d);
        with_session(&mut w, a);
        let mut s = SessionData::default();
        s.set_player(Some(d));
        s.set_account(
            2,
            "acct2".to_owned(),
            empyrean_entity::enums::AccessLevel::Player,
        );
        w.sessions.insert(S2, s);
        ThreadSafeRandom::seed(seed);
        start_capture();
        let e = player_combat::damage_target(&mut w, a, d, Some(a)).expect("the defender is alive");
        let sent = take_sent();
        if e.has_damage() {
            continue;
        }
        evaded += 1;
        let to_defender: Vec<_> = sent.iter().filter(|(s, _, _)| *s == S2).cloned().collect();
        let ops = opcodes(&to_defender);
        assert!(
            ops.contains(&0x01B4),
            "the defender's EvasionDefenderNotification: {ops:04X?}"
        );
        let attacker = w
            .objects
            .get(d)
            .unwrap()
            .get_property(empyrean_entity::enums::PropertyInstanceId::CurrentAttacker);
        assert_eq!(attacker, Some(a.full()), "OnEvade sets the last attacker");
    }
    assert!(evaded > 0, "some seed evades");
}

/// Appraisal ratings read the rating getters.
#[test]
fn appraisal_ratings_read_the_rating_getters() {
    let mut w = world();
    let mut s = spec("Creature", 0x8000_0001, None, 100, 2, false);
    s["ints"] = json!([
        [PropertyInt::HealingBoostRating.0, 7],
        [PropertyInt::DotResistRating.0, 8],
        [PropertyInt::NetherResistRating.0, 9],
        [PropertyInt::LifeResistRating.0, 10],
    ]);
    s["equipped"] = json!([{"class": "Clothing", "id": 0x8000_0002u32, "wcid": 2, "weenie_type": 2,
        "ints": [[PropertyInt::GearMaxHealth.0, 11], [PropertyInt::GearHealingBoost.0, 3]], "floats": []}]);
    let c = combatant_in(&mut w, &s);
    let r =
        empyrean_world::world_objects::world_object_networking::shims::creature_ratings(&mut w, c);
    assert_eq!(
        (
            r.healing_boost_rating,
            r.dot_resist_rating,
            r.nether_resist_rating,
            r.life_resist_rating,
            r.gear_max_health
        ),
        (10, 8, 9, 10, 11)
    );
}

/// `CreatureVital.Base`: a player's max health adds the equipped gear's max health rating
/// (`GetGearMaxHealth`, from the equipped items' rating cache).
#[test]
fn gear_max_health_raises_a_players_max_health() {
    let max_health = |gear: i32| {
        let mut w = world();
        let mut s = spec("Player", 0x5000_0001, None, 100, 2, true);
        s["equipped"] = json!([{"class": "Clothing", "id": 0x8000_0002u32, "wcid": 2, "weenie_type": 2,
            "ints": [[PropertyInt::GearMaxHealth.0, gear]], "floats": []}]);
        let p = combatant_in(&mut w, &s);
        let o = w.objects.get(p).unwrap();
        o.health().base(&w, o)
    };
    assert_eq!(max_health(11) - max_health(0), 11);
}

mod weapon_and_rating_queries {
    use crate::support::navigation_world::*;

    /// `AppraiseInfo.AddRatings` reads the `Creature_Rating` getters: a monster's DamageRating and
    /// CritRating properties come back (no heritage bonus without a weapon).
    #[test]
    fn appraisal_ratings_read_the_creature_rating_getters() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        let o = h.w.objects.get_mut(MONSTER).unwrap();
        o.set_property(PropertyInt::DamageRating, 7);
        o.set_property(PropertyInt::CritRating, 3);
        let r = shims::creature_ratings(&mut h.w, MONSTER);
        assert_eq!(
            (r.damage_rating, r.crit_rating, r.damage_resist_rating),
            (7, 3, 0)
        );
    }

    /// `WorldObject.HasImbuedEffect` and `player.CombatMode == Missile` through the networking shims.
    #[test]
    fn imbued_effects_and_the_combat_mode_are_read_from_their_ports() {
        let mut h = H::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.monster(100.0, 100.0);
        h.w.objects
            .get_mut(MONSTER)
            .unwrap()
            .set_imbued_effect(ImbuedEffectType::CriticalStrike);
        assert!(shims::has_imbued_effect(
            &h.w,
            MONSTER,
            ImbuedEffectType::CriticalStrike
        ));
        assert!(!shims::has_imbued_effect(
            &h.w,
            MONSTER,
            ImbuedEffectType::CripplingBlow
        ));

        assert!(!shims::player_combat_mode_is_missile(&h.w, MONSTER));
        creature_combat::set_combat_mode_field(&mut h.w, MONSTER, CombatMode::Missile);
        assert!(shims::player_combat_mode_is_missile(&h.w, MONSTER));
    }

    /// `WorldObject.GetBaseDamage()`: `Damage ?? 0` and `(float)(DamageVariance ?? 0)`.
    #[test]
    fn base_damage_reads_damage_and_variance() {
        let mut w = bare_world();
        let s = ObjectGuid::new(0x7000_0003);
        let mut o = at(s, 0xA9B4_0001, 1.0, 1.0);
        o.set_property(PropertyInt::Damage, 12);
        o.set_property(PropertyFloat::DamageVariance, 0.25);
        w.objects.insert(o).unwrap();
        let d = dispatch::get_base_damage::get_base_damage(&w, s);
        assert_eq!((d.max_damage, d.variance), (12, 0.25));
        let bare = ObjectGuid::new(0x7000_0004);
        w.objects.insert(at(bare, 0xA9B4_0001, 1.0, 1.0)).unwrap();
        let d = dispatch::get_base_damage::get_base_damage(&w, bare);
        assert_eq!((d.max_damage, d.variance), (0, 0.0));
    }
}
