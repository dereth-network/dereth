//! Vectors: fixtures/vectors/weapons/
//! DamageEvent, imbue mods, attack types, strikes, stance helpers replay ACE; monster/player
//! swings land with ACE messages; combat-mode updates; self-squelch; nether armour default.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f64_of, u64_of, Case};
use empyrean_content::models::world::{Weenie as WeenieRow, WeeniePropertiesBodyPart};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, XpTable};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{
    AttackHeight, AttackType, CombatBodyPart, CombatMode, DamageType, EnchantmentTypeFlags,
    MotionCommand, MotionStance, PositionType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass,
    SpellCategory, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::models::{
    PropertiesBodyPart, PropertiesEnchantmentRegistry, PropertiesPosition,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::damage_event::DamageEvent;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::motion::movement_data::Motion;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::creature_combat::{self, CombatType};
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_skill::CreatureSkill;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::world_objects::world_object_weapon::{self as weapon, SkillOf};
use empyrean_world::world_objects::{creature_melee, world_object_weapon};
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

/// The dats of `stats/tables.json` (the skill table before `AddRetiredSkills`, which `DatManager`
/// runs itself), as the stats tests build them.
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

/// A world over the synthetic dats and `content`, with ACE's default settings loaded (the
/// `PropertyManager` values the harness caches).
fn world(content: MemContent) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats());
    w.content = Arc::new(content);
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

/// The property lists every spec carries.
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
    for p in spec["bools"].as_array().expect("bools") {
        o.set_property(PropertyBool(prop_u16(&p[0])), p[1].as_bool().expect("bool"));
    }
    for p in spec["strings"].as_array().expect("strings") {
        o.set_property(
            PropertyString(prop_u16(&p[0])),
            p[1].as_str().expect("string").to_owned(),
        );
    }
    let reg = o
        .biota
        .properties_enchantment_registry
        .get_or_insert_with(Vec::new);
    for e in spec["enchantments"].as_array().expect("enchantments") {
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
    if let Some(loc) = spec.get("location") {
        let l = loc.as_array().expect("location");
        let mut positions = DotNetDict::new();
        positions.insert(
            PositionType::Location,
            PropertiesPosition {
                obj_cell_id: u(&l[0]),
                position_x: f(&l[1]),
                position_y: f(&l[2]),
                position_z: f(&l[3]),
                rotation_w: f(&l[4]),
                rotation_x: f(&l[5]),
                rotation_y: f(&l[6]),
                rotation_z: f(&l[7]),
            },
        );
        o.biota.properties_position = Some(positions);
    }
}

fn class_of(name: &str) -> Class {
    match name {
        "Player" => Class::Player,
        "Creature" => Class::Creature,
        "MeleeWeapon" => Class::MeleeWeapon,
        "Clothing" => Class::Clothing,
        "GenericObject" => Class::GenericObject,
        other => panic!("unknown class {other}"),
    }
}

fn body_part(p: &Value) -> (CombatBodyPart, PropertiesBodyPart) {
    let q = p[6].as_array().expect("quadrants");
    let qf = |k: usize| f(&q[k]);
    (
        CombatBodyPart(i(&p[0])),
        PropertiesBodyPart {
            d_type: DamageType(i(&p[1])),
            d_val: i(&p[2]),
            d_var: f(&p[3]),
            base_armor: i(&p[4]),
            bh: i(&p[5]),
            hlf: qf(0),
            mlf: qf(1),
            llf: qf(2),
            hrf: qf(3),
            mrf: qf(4),
            lrf: qf(5),
            hlb: qf(6),
            mlb: qf(7),
            llb: qf(8),
            hrb: qf(9),
            mrb: qf(10),
            lrb: qf(11),
            ..PropertiesBodyPart::default()
        },
    )
}

/// The weenie row a defender creature's `GetBodyParts(wcid)` reads.
fn weenie_row(spec: &Value) -> WeenieRow {
    let wcid = u(&spec["wcid"]);
    let mut row = WeenieRow::new(wcid, &format!("combatvec{wcid}"), WeenieType::Creature);
    for (n, p) in spec["body_parts"]
        .as_array()
        .expect("body_parts")
        .iter()
        .enumerate()
    {
        let (key, bp) = body_part(p);
        row.weenie_properties_body_part
            .push(WeeniePropertiesBodyPart {
                id: u32::try_from(n + 1).expect("id"),
                object_id: wcid,
                key: u16::try_from(key.0).expect("key"),
                d_type: bp.d_type.0,
                d_val: bp.d_val,
                d_var: bp.d_var,
                base_armor: bp.base_armor,
                bh: bp.bh,
                hlf: bp.hlf,
                mlf: bp.mlf,
                llf: bp.llf,
                hrf: bp.hrf,
                mrf: bp.mrf,
                lrf: bp.lrf,
                hlb: bp.hlb,
                mlb: bp.mlb,
                llb: bp.llb,
                hrb: bp.hrb,
                mrb: bp.mrb,
                lrb: bp.lrb,
                ..WeeniePropertiesBodyPart::default()
            });
    }
    row
}

/// Builds a combatant (and its equipment) from a `damage_event` spec into the world.
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
    if let Some(parts) = spec.get("body_parts") {
        let mut d = DotNetDict::new();
        for p in parts.as_array().expect("body parts") {
            let (k, v) = body_part(p);
            d.insert(k, v);
        }
        o.biota.properties_body_part = Some(Arc::new(d));
    }
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

    // combat state
    let state = &spec["state"];
    let stance = MotionStance(u(&state["stance"]));
    let forward = MotionCommand(u(&state["forward"]));
    o.wo.world_object_properties.current_motion_state = Some(Motion::new(stance, forward, 1.0));
    o.creature
        .as_mut()
        .expect("a creature")
        .monster_combat
        .attack_height = state["attack_height"]
        .as_i64()
        .map(|h| AttackHeight(i32::try_from(h).expect("height")));
    {
        let fields = creature_combat::fields_mut(&mut o);
        fields.combat_mode = CombatMode(i(&state["combat_mode"]));
        fields.attack_type = AttackType(i(&state["attack_type"]));
    }
    if let Some(p) = state.get("power_level") {
        let player = o.player.as_mut().expect("a player");
        player.player_melee.power_level = f(p);
        player.player_missile.accuracy_level = f(&state["accuracy_level"]);
        player.player.is_logging_out = state["is_logging_out"].as_bool().expect("bool");
        player.player.pk_logout = state["pk_logout"].as_bool().expect("bool");
    }

    // equipment, in EquippedObjects order
    let mut items = Vec::new();
    for item in spec["equipped"].as_array().expect("equipped") {
        let mut io = WorldObject::allocate(class_of(item["class"].as_str().expect("class")));
        let iid = u(&item["id"]);
        io.guid = ObjectGuid::new(iid);
        io.biota.id = iid;
        io.biota.weenie_class_id = u(&item["wcid"]);
        io.biota.weenie_type = WeenieType(u(&item["weenie_type"]));
        apply_properties(&mut io, item);
        items.push(io.guid);
        o.creature
            .as_mut()
            .expect("creature")
            .creature_equipment
            .equipped_objects
            .insert(io.guid, ());
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

/// `same_f32` over a JSON number (floats are written as their exact double).
fn same(got: f32, want: &Value) -> bool {
    let w = f64_of(want).unwrap_or_else(|| panic!("not a number: {want}"));
    #[allow(clippy::float_cmp)]
    {
        (got.is_nan() && w.is_nan()) || f64::from(got) == w
    }
}

struct Errs(Vec<String>);

impl Errs {
    fn f32(&mut self, what: &str, got: f32, want: &Value) {
        if !same(got, want) {
            self.0.push(format!("{what}: got {got:?} want {want}"));
        }
    }

    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, got: T, want: T) {
        if got != want {
            self.0.push(format!("{what}: got {got:?} want {want:?}"));
        }
    }
}

fn check_event(e: &DamageEvent, want: &Value, rng_after: i32) -> Result<(), String> {
    let mut x = Errs(Vec::new());
    x.f32("accuracy_mod", e.accuracy_mod, &want["accuracy_mod"]);
    let armor: Option<Vec<u32>> = e
        .armor
        .as_ref()
        .map(|a| a.iter().map(|g| g.full()).collect());
    let want_armor: Option<Vec<u32>> = want["armor"].as_array().map(|a| a.iter().map(u).collect());
    x.eq("armor", armor, want_armor);
    x.f32("armor_mod", e.armor_mod, &want["armor_mod"]);
    x.eq(
        "attack_conditions",
        e.attack_conditions().0,
        i(&want["attack_conditions"]),
    );
    x.eq(
        "attack_part",
        e.attack_part.as_ref().map(|p| p.0 .0),
        want["attack_part"]
            .as_i64()
            .map(|v| i32::try_from(v).unwrap()),
    );
    x.f32("attribute_mod", e.attribute_mod, &want["attribute_mod"]);
    x.f32("base_damage", e.base_damage, &want["base_damage"]);
    match (&e.base_damage_mod, want["bdm"].as_array()) {
        (None, None) => {}
        (Some(m), Some(b)) => {
            x.eq("bdm.max", m.base_damage.max_damage, i(&b[0]));
            x.f32("bdm.variance", m.base_damage.variance, &b[1]);
            x.f32("bdm.bonus", m.damage_bonus, &b[2]);
            x.f32("bdm.mod", m.damage_mod, &b[3]);
            x.f32("bdm.variance_mod", m.variance_mod, &b[4]);
            x.eq("bdm.elemental", m.elemental_bonus, i(&b[5]));
            x.f32("bdm.MaxDamage", m.max_damage(), &b[6]);
            x.f32("bdm.MinDamage", m.min_damage(), &b[7]);
        }
        (g, w) => x.0.push(format!("bdm: got {g:?} want {w:?}")),
    }
    x.eq("body_part", e.body_part.0, i(&want["body_part"]));
    let ct = match e.combat_type {
        CombatType::Melee => 0,
        CombatType::Missile => 1,
        CombatType::Magic => 2,
    };
    x.eq("combat_type", ct, i(&want["combat_type"]));
    x.f32("crit_chance", e.critical_chance, &want["crit_chance"]);
    x.f32(
        "crit_damage_mod",
        e.critical_damage_mod,
        &want["crit_damage_mod"],
    );
    x.f32(
        "crit_damage_rating_mod",
        e.critical_damage_rating_mod,
        &want["crit_damage_rating_mod"],
    );
    x.f32(
        "crit_damage_resistance_rating_mod",
        e.critical_damage_resistance_rating_mod,
        &want["crit_damage_resistance_rating_mod"],
    );
    x.eq(
        "crit_defended",
        e.critical_defended,
        want["crit_defended"].as_bool().unwrap(),
    );
    x.f32("damage", e.damage, &want["damage"]);
    x.f32(
        "damage_before_mitigation",
        e.damage_before_mitigation,
        &want["damage_before_mitigation"],
    );
    x.f32(
        "damage_mitigated",
        e.damage_mitigated,
        &want["damage_mitigated"],
    );
    x.f32(
        "damage_rating_base_mod",
        e.damage_rating_base_mod,
        &want["damage_rating_base_mod"],
    );
    x.f32(
        "damage_rating_mod",
        e.damage_rating_mod,
        &want["damage_rating_mod"],
    );
    x.f32(
        "damage_resistance_rating_base_mod",
        e.damage_resistance_rating_base_mod,
        &want["damage_resistance_rating_base_mod"],
    );
    x.f32(
        "damage_resistance_rating_mod",
        e.damage_resistance_rating_mod,
        &want["damage_resistance_rating_mod"],
    );
    x.eq("damage_type", e.damage_type.0, i(&want["damage_type"]));
    x.eq(
        "effective_attack_skill",
        e.effective_attack_skill,
        u(&want["effective_attack_skill"]),
    );
    x.eq(
        "effective_defense_skill",
        e.effective_defense_skill,
        u(&want["effective_defense_skill"]),
    );
    x.eq("evaded", e.evaded, want["evaded"].as_bool().unwrap());
    x.f32("evasion_chance", e.evasion_chance, &want["evasion_chance"]);
    x.eq(
        "general_failure",
        e.general_failure,
        want["general_failure"].as_bool().unwrap(),
    );
    x.f32("heritage_mod", e.heritage_mod, &want["heritage_mod"]);
    x.eq(
        "is_critical",
        e.is_critical,
        want["is_critical"].as_bool().unwrap(),
    );
    x.eq(
        "overpower",
        e.overpower,
        want["overpower"].as_bool().unwrap(),
    );
    x.eq(
        "part_key",
        e.creature_part
            .as_ref()
            .map(|_| e.properties_body_part.as_ref().expect("key").0 .0),
        want["part_key"].as_i64().map(|v| i32::try_from(v).unwrap()),
    );
    x.f32("pk_damage_mod", e.pk_damage_mod, &want["pk_damage_mod"]);
    x.f32(
        "pk_damage_resistance_mod",
        e.pk_damage_resistance_mod,
        &want["pk_damage_resistance_mod"],
    );
    x.f32("power_mod", e.power_mod, &want["power_mod"]);
    x.eq("quadrant", e.quadrant.0, i(&want["quadrant"]));
    x.f32(
        "recklessness_mod",
        e.recklessness_mod,
        &want["recklessness_mod"],
    );
    x.f32("resistance_mod", e.resistance_mod, &want["resistance_mod"]);
    x.eq("rng_after", rng_after, i(&want["rng_after"]));
    x.f32("shield_mod", e.shield_mod, &want["shield_mod"]);
    x.f32("slayer_mod", e.slayer_mod, &want["slayer_mod"]);
    x.f32(
        "sneak_attack_mod",
        e.sneak_attack_mod,
        &want["sneak_attack_mod"],
    );
    x.eq(
        "weapon",
        e.weapon.map(|g| g.full()),
        want["weapon"].as_u64().map(|v| u32::try_from(v).unwrap()),
    );
    x.f32(
        "weapon_resistance_mod",
        e.weapon_resistance_mod,
        &want["weapon_resistance_mod"],
    );
    if x.0.is_empty() {
        Ok(())
    } else {
        Err(x.0.join("; "))
    }
}

fn replay(name: &str, mut each: impl FnMut(&Case) -> Result<(), String>) -> usize {
    let file = vectors::load_named("weapons", name);
    let mut failures = Vec::new();
    for (n, case) in file.cases.iter().enumerate() {
        if let Err(got) = each(case) {
            failures.push(format!("case {n}: {got}"));
        }
    }
    assert!(!file.cases.is_empty(), "weapons/{name}: no cases");
    assert!(
        failures.is_empty(),
        "weapons/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    file.cases.len()
}

// ------------------------------------------------------------------ vector replays

#[test]
fn damage_events_match_ace() {
    let file = vectors::load_named("weapons", "damage_event");
    let mut content = MemContent::new();
    for case in &file.cases {
        let d = &case.input["defender"];
        if d["class"] == "Creature" {
            content = content.weenie(weenie_row(d));
        }
    }
    let mut w = world(content);
    let n = replay("damage_event", |c| {
        let attacker = combatant_in(&mut w, &c.input["attacker"]);
        let defender = combatant_in(&mut w, &c.input["defender"]);
        // ACE read a missing ArmorModVsNether as 0 in damage; the server now reads it as 1.0
        // (V273, V273). These vectors pin ACE's arithmetic, so the defender's items carry ACE's
        // value explicitly.
        let equipped: Vec<ObjectGuid> = w
            .objects
            .get(defender)
            .and_then(|o| o.creature.as_ref())
            .map(|c| {
                c.creature_equipment
                    .equipped_objects
                    .keys()
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        for item in equipped {
            let io = w.objects.get_mut(item).expect("the item");
            if io.get_property(PropertyFloat::ArmorModVsNether).is_none() {
                io.set_property(PropertyFloat::ArmorModVsNether, 0.0);
            }
        }
        let motion = c.input["attack_motion"]
            .as_u64()
            .map(|m| MotionCommand(u32::try_from(m).unwrap()));
        ThreadSafeRandom::seed(u64::from(u32::from_ne_bytes(
            i(&c.input["seed"]).to_ne_bytes(),
        )));
        if let Some(t) = empyrean_common::vectors::throws(&c.output) {
            return Err(format!("ACE threw {t}; not expected in this grid"));
        }
        let e = DamageEvent::calculate_damage(&mut w, attacker, defender, None, motion, None);
        let rng_after = ThreadSafeRandom::next(0, 1_000_000);
        check_event(&e, &c.output, rng_after)
    });
    assert!(n >= 500, "{n} cases");
}

#[test]
fn imbue_mods_match_ace() {
    let mut w = world(MemContent::new());
    let mut next = 0x8000_1000u32;
    replay("imbue_mods", |c| {
        let skill = Skill(i(&c.input["skill"]));
        let mut o = WorldObject::allocate(Class::Creature);
        o.guid = ObjectGuid::new(next);
        o.biota.id = next;
        next += 1;
        o.biota.properties_attribute = Some(DotNetDict::new());
        o.biota.properties_attribute_2nd = Some(DotNetDict::new());
        let mut skills = DotNetDict::new();
        skills.insert(
            skill,
            PropertiesSkill {
                sac: SkillAdvancementClass::Trained,
                init_level: u(&c.input["init"]),
                ..PropertiesSkill::default()
            },
        );
        o.biota.properties_skill = Some(skills);
        o.biota.properties_enchantment_registry = Some(Vec::new());
        for a in 1..=6u16 {
            let ca = CreatureAttribute::new(&mut o, PropertyAttribute(a));
            o.attributes_mut().insert(PropertyAttribute(a), ca);
        }
        o.skills_mut().insert(skill, CreatureSkill::new(skill));
        let g = o.guid;
        w.objects.insert(o).expect("fresh");
        let cs = Some(SkillOf::get(&mut w, g, skill));
        let out = c.output.as_array().expect("array");
        let mut x = Errs(Vec::new());
        x.f32(
            "CS pve",
            weapon::get_critical_strike_mod(&w, cs, false),
            &out[0],
        );
        x.f32(
            "CS pvp",
            weapon::get_critical_strike_mod(&w, cs, true),
            &out[1],
        );
        x.f32("CB", weapon::get_crippling_blow_mod(&w, cs), &out[2]);
        x.f32("Rending", weapon::get_rending_mod(&w, cs), &out[3]);
        x.f32(
            "ArmorRending",
            weapon::get_armor_rending_mod(&w, cs),
            &out[4],
        );
        x.eq(
            "BaseSkillImbued",
            weapon::get_base_skill_imbued(&w, cs),
            i(&out[5]),
        );
        let t = match weapon::get_imbued_skill_type(cs) {
            world_object_weapon::ImbuedSkillType::Undef => 0,
            world_object_weapon::ImbuedSkillType::Melee => 1,
            world_object_weapon::ImbuedSkillType::Missile => 2,
            world_object_weapon::ImbuedSkillType::Magic => 3,
        };
        x.eq("ImbuedSkillType", t, i(&out[6]));
        x.f32(
            "Interval(true)",
            weapon::get_imbued_interval(&w, cs, true),
            &out[7],
        );
        x.f32(
            "Interval(false)",
            weapon::get_imbued_interval(&w, cs, false),
            &out[8],
        );
        if x.0.is_empty() {
            Ok(())
        } else {
            Err(x.0.join("; "))
        }
    });
}

#[test]
fn weapon_attack_types_match_ace() {
    let mut w = world(MemContent::new());
    let mut next = 0x8000_2000u32;
    let powers = [0.0f32, 0.32, 0.33, 0.5, 1.0];
    replay("attack_type", |c| {
        let mut o = WorldObject::allocate(Class::MeleeWeapon);
        o.guid = ObjectGuid::new(next);
        next += 1;
        let at = i(&c.input["attack_type"]);
        if at != 0 {
            o.set_property(PropertyInt::AttackType, at);
        }
        let g = o.guid;
        w.objects.insert(o).expect("fresh");
        let mut got = Vec::new();
        for st in c.input["stances"].as_array().expect("stances") {
            for &p in &powers {
                for off in [false, true] {
                    got.push(serde_json::json!(
                        weapon::get_attack_type(&w, g, MotionStance(u(st)), p, off).0
                    ));
                }
            }
        }
        got.push(serde_json::json!(weapon::is_thrust_slash(
            w.objects.get(g).unwrap()
        )));
        if Value::Array(got.clone()) == c.output {
            Ok(())
        } else {
            Err(format!("{got:?}"))
        }
    });
}

#[test]
fn strikes_match_ace() {
    let mut w = world(MemContent::new());
    let mut next = 0x8000_3000u32;
    replay("strikes", |c| {
        let mut o = WorldObject::allocate(Class::Creature);
        o.guid = ObjectGuid::new(next);
        next += 1;
        o.wo.world_object_properties.current_motion_state =
            Some(Motion::from_stance(MotionStance(u(&c.input["stance"]))));
        let g = o.guid;
        w.objects.insert(o).expect("fresh");
        let at = AttackType(i(&c.input["attack_type"]));
        let got = serde_json::json!([
            creature_melee::get_num_strikes_of(&w, g, at),
            creature_melee::multi_strike(at, "SlashHigh"),
            creature_melee::multi_strike(at, "ThrustMed"),
            creature_melee::two_handed_combat(&w, g),
            creature_melee::is_dual_wield_attack(&w, g),
        ]);
        if got == c.output {
            Ok(())
        } else {
            Err(got.to_string())
        }
    });
}

// ------------------------------------------------------------------ hand-derived

#[test]
fn damage_type_selection_skips_undef_and_can_pick_a_composite() {
    // GetFlags(Slash | Pierce | Bludgeon) = [Undef, Slash, Pierce, Bludgeon, Physical]; Next(1, 4)
    let mut seen = std::collections::BTreeSet::new();
    ThreadSafeRandom::seed(7);
    for _ in 0..200 {
        seen.insert(
            empyrean_entity::enums::DamageType::select_damage_type(DamageType::Physical, None).0,
        );
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        vec![1, 2, 4, 7],
        "Slash, Pierce, Bludgeon and (ACE-BUG) Physical"
    );
}

#[test]
fn stance_helpers_follow_ace() {
    assert_eq!(
        creature_combat::add_shield_stance(MotionStance::SwordCombat),
        MotionStance::SwordShieldCombat
    );
    assert_eq!(
        creature_combat::add_shield_stance(MotionStance::ThrownWeaponCombat),
        MotionStance::ThrownShieldCombat
    );
    assert_eq!(
        creature_combat::add_shield_stance(MotionStance::BowCombat),
        MotionStance::BowCombat
    );
    assert_eq!(creature_combat::get_exhausted_skill(40), 20);
    assert_eq!(creature_combat::get_exhausted_skill(300), 250);
    assert_eq!(creature_combat::get_exhausted_skill(101), 51);
    assert_eq!(
        creature_combat::get_defense_skill(CombatType::Missile),
        Skill::MissileDefense
    );
    assert_eq!(
        creature_combat::get_resistance_type(DamageType::Health),
        empyrean_entity::enums::ResistanceType::HealthDrain
    );
}

// ------------------------------------------------------------------ scenarios

const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const PLAYER: u32 = 0x5000_0001;
const MONSTER: u32 = 0x8000_0001;
const GAME_EVENT: u32 = 0xF7B0;
const UPDATE_INT: u32 = 0x02CD;
const UPDATE_VITAL_LEVEL: u32 = 0x02E9;
const UPDATE_IID: u32 = 0x02D9;
const UPDATE_MOTION: u32 = 0xF74C;
const SOUND: u32 = 0xF750;
const PLAY_EFFECT: u32 = 0xF755;

fn opcodes(sent: &[(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)]) -> Vec<u32> {
    sent.iter()
        .map(|(_, _, b)| u32::from_le_bytes(b[..4].try_into().unwrap()))
        .collect()
}

/// A game event's type and body (opcode, player guid, sequence, event type, then the body).
fn event(bytes: &[u8]) -> (u32, &[u8]) {
    (
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
        &bytes[16..],
    )
}

/// Decodes a message with dereth-protocol as `M` (opcode, then the body).
fn decode<M: dereth_protocol::Message + std::fmt::Debug>(m: &[u8]) -> M {
    assert_eq!(
        u32::from_le_bytes(m[..4].try_into().unwrap()),
        M::OPCODE.0,
        "opcode"
    );
    dereth_protocol::read_body_padded::<M>(&m[4..]).expect("dereth-protocol decodes it")
}

/// A combat notification's body. `AttackConditions` is one dword, as the client reads it (V235/V293);
/// ACE's trailing high dword is gone, so the body decodes as it stands.
fn notification(body: &[u8]) -> &[u8] {
    body
}

/// Two fighters 1 m apart facing each other: a player (session S) and a drudge-like monster whose
/// one body part hits for 20 +/- 0.5 and whose weenie has a chest in every quadrant.
fn arena() -> (World, ObjectGuid, ObjectGuid) {
    let parts = serde_json::json!([[
        1,
        4,
        20,
        0.5,
        10,
        2,
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
    ]]);
    let monster_spec = serde_json::json!({
        "class": "Creature", "id": MONSTER, "wcid": 7001, "weenie_type": 10,
        "attributes": [[100, 0], [100, 0], [100, 0], [100, 0], [100, 0], [100, 0]],
        "vitals": [[200, 0, 200], [100, 0, 100], [50, 0, 50]],
        "skills": [[6, 2, 380, 0], [45, 2, 200, 0]],
        "ints": [], "floats": [], "bools": [], "strings": [[1, "Drudge"]], "enchantments": [],
        "location": [0xA9B4_0014u32, 96.0, 96.0, 10.0, 1.0, 0.0, 0.0, 0.0],
        "body_parts": parts, "equipped": [],
        "state": {"stance": 0x8000_003C_u32, "forward": 0x4100_0003_u32, "combat_mode": 2, "attack_type": 0, "attack_height": 2},
    });
    let player_spec = serde_json::json!({
        "class": "Player", "id": PLAYER, "wcid": 1, "weenie_type": 10,
        "attributes": [[100, 0], [100, 0], [100, 0], [100, 0], [100, 0], [100, 0]],
        "vitals": [[100, 0, 100], [100, 0, 100], [50, 0, 50]],
        "skills": [[6, 2, 10, 0], [45, 2, 400, 0]],
        "ints": [], "floats": [], "bools": [], "strings": [[1, "Tester"]], "enchantments": [],
        "location": [0xA9B4_0014u32, 96.0, 97.0, 10.0, 0.0, 0.0, 0.0, 1.0],
        "equipped": [],
        "state": {"stance": 0x8000_003C_u32, "forward": 0x4100_0003_u32, "combat_mode": 2, "attack_type": 1, "attack_height": 2,
                  "power_level": 0.5, "accuracy_level": 0.0, "is_logging_out": false, "pk_logout": false},
    });
    let mut w = world(MemContent::new().weenie(weenie_row(&monster_spec)));
    let monster = combatant_in(&mut w, &monster_spec);
    let player = combatant_in(&mut w, &player_spec);
    let o = w.objects.get_mut(player).expect("live");
    o.set_property(PropertyInt::Level, 1);
    o.set_property(empyrean_entity::enums::PropertyInt64::TotalExperience, 0);
    o.set_property(
        empyrean_entity::enums::PropertyInt64::AvailableExperience,
        0,
    );
    empyrean_world::world_objects::monster::set_monster_state_value(
        &mut w,
        monster,
        empyrean_world::world_objects::monster::State::Awake,
    );
    let mut s = SessionData::default();
    s.set_player(Some(player));
    // an authenticated session (`Session.AccountId`: the squelch checks read it)
    s.set_account(
        1,
        "acct".to_owned(),
        empyrean_entity::enums::AccessLevel::Player,
    );
    w.sessions.insert(S, s);
    (w, player, monster)
}

#[test]
fn switching_to_melee_and_back_sends_the_combat_mode_updates() {
    let (mut w, player, _) = arena();
    creature_combat::set_combat_mode_field(&mut w, player, CombatMode::NonCombat);
    w.objects
        .get_mut(player)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state = Some(Motion::from_stance(MotionStance::NonCombat));

    start_capture();
    let t = creature_combat::set_combat_mode(&mut w, player, CombatMode::Melee);
    let sent = take_sent();
    assert_eq!(t, 0.0, "no motion table: no animation time");
    assert_eq!(creature_combat::combat_mode(&w, player), CombatMode::Melee);
    let stance = w
        .objects
        .get(player)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state
        .as_ref()
        .unwrap()
        .stance;
    assert_eq!(stance, MotionStance::HandCombat, "unarmed: hand combat");
    // the stance's UpdateMotion broadcast reaches the player itself (it has a PhysicsObj), then
    assert_eq!(
        opcodes(&sent),
        [UPDATE_MOTION, UPDATE_INT],
        "PrivateUpdatePropertyInt(CombatMode) to the player"
    );
    // sequence byte, property, value
    assert_eq!(&sent[1].2[5..], [40u8, 0, 0, 0, 2, 0, 0, 0].as_slice());

    // already in the right stance: nothing to do
    start_capture();
    assert_eq!(
        creature_combat::set_combat_mode(&mut w, player, CombatMode::Melee),
        0.0
    );
    assert!(take_sent().is_empty());

    start_capture();
    creature_combat::set_combat_mode(&mut w, player, CombatMode::NonCombat);
    let sent = take_sent();
    assert_eq!(opcodes(&sent), [UPDATE_MOTION, UPDATE_INT]);
    assert_eq!(&sent[1].2[5..], [40u8, 0, 0, 0, 1, 0, 0, 0].as_slice());
    assert_eq!(
        creature_combat::combat_mode(&w, player),
        CombatMode::NonCombat
    );
}

/// V330 (V330): with `persist_movement` on, a missile reload in an unchanged stance keeps the
/// sidestep and turn the player already had, both in its current motion state and in the
/// broadcast UpdateMotion.
#[test]
fn persisted_movement_keeps_the_sidestep_and_turn_when_the_stance_is_unchanged() {
    use empyrean_world::entity::actions::action_chain::ActionChain;
    use empyrean_world::entity::actions::action_queue::run_actions;
    use empyrean_world::entity::actions::i_actor::Actor;
    use empyrean_world::managers::property_manager as pm;
    use empyrean_world::world_objects::world_object_networking;

    let (mut w, player, _) = arena();
    pm::modify_bool(&w, "persist_movement", true);
    creature_combat::set_combat_mode_field(&mut w, player, CombatMode::Missile);
    let mut current = Motion::from_stance(MotionStance::BowCombat);
    current.set_sidestep_command(MotionCommand::SideStepRight, 1.25);
    current.set_turn_command(MotionCommand::TurnRight, 1.5);
    w.objects
        .get_mut(player)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state = Some(current);

    let mut chain = ActionChain::new();
    world_object_networking::enqueue_motion_persist_stance(
        &mut w,
        player,
        &mut chain,
        MotionStance::BowCombat,
        MotionCommand::Reload,
        1.0,
    );
    start_capture();
    chain.enqueue_chain(&mut w);
    run_actions(&mut w, Actor::Object(player));
    let sent = take_sent();

    let state = w
        .objects
        .get(player)
        .unwrap()
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
        .unwrap();
    assert_eq!(
        state.motion_state.forward_command,
        MotionCommand::Reload,
        "the new motion is stored"
    );
    assert_eq!(
        (
            state.motion_state.sidestep_command,
            state.motion_state.sidestep_speed
        ),
        (MotionCommand::SideStepRight, 1.25),
        "the stored state keeps the sidestep"
    );
    assert_eq!(
        (
            state.motion_state.turn_command,
            state.motion_state.turn_speed
        ),
        (MotionCommand::TurnRight, 1.5),
        "the stored state keeps the turn"
    );

    let motions: Vec<_> = sent
        .iter()
        .filter(|(_, _, b)| b[..4] == UPDATE_MOTION.to_le_bytes())
        .collect();
    assert_eq!(motions.len(), 1, "one UpdateMotion broadcast");
    let m =
        dereth_protocol::read_body_padded::<dereth_protocol::movement::MovementSetObjectMovement>(
            &motions[0].2[4..],
        )
        .expect("UpdateMotion decodes");
    let body = m.decoded_movement().unwrap().body;
    let s = body.interpreted.expect("an interpreted motion state");
    assert!(
        s.sidestep_command.is_some(),
        "the broadcast keeps the sidestep: {s:?}"
    );
    assert_eq!(
        s.sidestep_speed,
        Some(1.25),
        "the broadcast keeps the sidestep speed"
    );
    assert!(
        s.turn_command.is_some(),
        "the broadcast keeps the turn: {s:?}"
    );
    assert_eq!(
        s.turn_speed,
        Some(1.5),
        "the broadcast keeps the turn speed"
    );
}

#[test]
fn stance_queue_accumulates_weapon_swaps() {
    let (mut w, player, _) = arena();
    // now = 1000: the first swap starts now, the second queues behind it
    assert_eq!(
        creature_combat::handle_stance_queue(&mut w, player, 1.5),
        0.0
    );
    assert_eq!(
        creature_combat::handle_stance_queue(&mut w, player, 1.0),
        1.5
    );
    assert_eq!(
        creature_combat::fields(w.objects.get(player).unwrap()).last_weapon_swap,
        1002.5
    );
}

#[test]
fn a_monster_swing_lands_and_the_player_takes_it() {
    use dereth_protocol::combat::DefenderNotification;
    use dereth_protocol::qualities::QualitiesPrivateUpdateAttribute2ndLevel;
    use dereth_protocol::Message as _;
    let (mut w, player, monster) = arena();
    // find a seed where the monster hits
    let mut seed = 0u64;
    let e = loop {
        ThreadSafeRandom::seed(seed);
        let e = DamageEvent::calculate_damage(&mut w, monster, player, None, None, None);
        if e.has_damage() {
            break e;
        }
        seed += 1;
    };
    assert!(
        e.damage > 0.0 && e.damage_type == DamageType::Bludgeon,
        "{e:?}"
    );
    let before = w
        .objects
        .get(player)
        .unwrap()
        .health()
        .current(w.objects.get(player).unwrap());

    start_capture();
    let taken = empyrean_world::world_objects::player_combat::take_damage(
        &mut w,
        player,
        Some(monster),
        e.damage_type,
        e.damage,
        e.body_part,
        e.is_critical,
        e.attack_conditions(),
    );
    let sent = take_sent();
    let after = w
        .objects
        .get(player)
        .unwrap()
        .health()
        .current(w.objects.get(player).unwrap());
    let amount: i32 = empyrean_common::dotnet::CsCast::cs_cast(
        empyrean_common::dotnet::math::round(f64::from(e.damage)),
    );
    assert_eq!(taken, amount);
    assert_eq!(i64::from(before) - i64::from(after), i64::from(amount));

    // the last attacker, the health update, the stamina point (melee mode), the defender
    // notification, then the broadcasts, which reach the player itself (it has a PhysicsObj): the
    // hit sound and splatter, and the wound sound (20 of 100 health is at least 10%)
    assert_eq!(
        opcodes(&sent),
        [
            UPDATE_IID,
            UPDATE_VITAL_LEVEL,
            UPDATE_VITAL_LEVEL,
            GAME_EVENT,
            SOUND,
            PLAY_EFFECT,
            SOUND
        ]
    );
    let health: QualitiesPrivateUpdateAttribute2ndLevel = decode(&sent[1].2);
    assert_eq!(
        (health.0.property_id, health.0.value),
        (2, after),
        "Vital.Health, the new current"
    );
    let (ty, body) = event(&sent[3].2);
    assert_eq!(ty, DefenderNotification::OPCODE.0);
    let d: DefenderNotification =
        dereth_protocol::read_body_padded(notification(body)).expect("dereth-protocol decodes it");
    assert_eq!(d.attacker_name, "Drudge");
    assert_eq!(d.damage, u32::try_from(amount).unwrap());
    assert_eq!(d.damage_type, 4);
    assert_eq!(d.critical, u32::from(e.is_critical));
}

#[test]
fn a_player_swing_hits_or_is_evaded_with_aces_messages() {
    use dereth_protocol::combat::{AttackerNotification, EvasionAttackerNotification};
    use dereth_protocol::Message as _;
    let (mut w, player, monster) = arena();
    let health = |w: &World| {
        let o = w.objects.get(monster).unwrap();
        o.health().current(o)
    };
    let (mut hits, mut evades) = (0, 0);
    for seed in 0..40u64 {
        ThreadSafeRandom::seed(seed);
        let before = health(&w);
        start_capture();
        let e = empyrean_world::world_objects::player_combat::damage_target(
            &mut w,
            player,
            monster,
            Some(player),
        )
        .expect("the monster is alive");
        let sent = take_sent();
        let events: Vec<(u32, Vec<u8>)> = sent
            .iter()
            .filter(|(_, _, b)| u32::from_le_bytes(b[..4].try_into().unwrap()) == GAME_EVENT)
            .map(|(_, _, b)| {
                let (t, body) = event(b);
                (t, body.to_vec())
            })
            .collect();
        if e.has_damage() {
            hits += 1;
            let amount: u32 = empyrean_common::dotnet::CsCast::cs_cast(
                empyrean_common::dotnet::math::round(f64::from(e.damage)),
            );
            assert_eq!(before - health(&w), amount.min(before));
            if health(&w) > 0 {
                assert_eq!(events[0].0, AttackerNotification::OPCODE.0);
                let a: AttackerNotification =
                    dereth_protocol::read_body_padded(notification(&events[0].1)).expect("decodes");
                assert_eq!(a.defender_name, "Drudge");
                assert_eq!(a.damage, amount);
            }
        } else {
            evades += 1;
            assert_eq!(before, health(&w));
            assert_eq!(events[0].0, EvasionAttackerNotification::OPCODE.0);
        }
        if health(&w) == 0 {
            break;
        }
    }
    assert!(hits > 0 && evades > 0, "hits {hits}, evades {evades}");
}

// Cross-checks against the shared client rules.

/// The client's appraisal of a caster's elemental bonus "vs. Players", the shared function.
fn client_elemental_mod_pk_modifier(m: f64) -> f64 {
    dereth_rules::combat::elemental_mod_pk_modifier(m)
}

/// caster elemental pvp modifier agrees with the client.
/// V381.
#[test]
fn caster_elemental_pvp_modifier_agrees_with_the_client() {
    let mut w = world(MemContent::new());
    let mut wielder = WorldObject::allocate(Class::Creature);
    wielder.guid = ObjectGuid::new(0x8000_4000);
    wielder.biota.properties_enchantment_registry = Some(Vec::new());
    let mut target = WorldObject::allocate(Class::Player);
    target.guid = ObjectGuid::new(0x5000_4000);
    let (wielder, target) = (wielder.guid, {
        let g = target.guid;
        w.objects.insert(wielder).unwrap();
        w.objects.insert(target).unwrap();
        g
    });
    let mut disagreements = Vec::new();
    for k in 0..=50 {
        let m = 1.0 + f64::from(k) * 0.01;
        let mut caster = WorldObject::allocate(Class::Caster);
        caster.guid = ObjectGuid::new(0x8000_5000 + k);
        caster.biota.properties_enchantment_registry = Some(Vec::new());
        caster.set_property(PropertyInt::DamageType, DamageType::Fire.0);
        caster.set_property(PropertyFloat::ElementalDamageMod, m);
        let g = caster.guid;
        w.objects.insert(caster).unwrap();
        let ace = weapon::get_caster_elemental_damage_modifier(
            &mut w,
            Some(g),
            Some(wielder),
            Some(target),
            DamageType::Fire,
        );
        #[allow(clippy::cast_possible_truncation)]
        let client = client_elemental_mod_pk_modifier(m) as f32;
        if (ace - client).abs() > 1e-6 {
            disagreements.push((m, ace, client));
        }
    }
    assert!(
        disagreements.is_empty(),
        "{} of 51 differ, e.g. {:?}",
        disagreements.len(),
        disagreements.last()
    );
}

/// A global combat self squelch silences the attackers notifications.
#[test]
fn a_global_combat_self_squelch_silences_the_attackers_notifications() {
    use dereth_protocol::combat::{AttackerNotification, EvasionAttackerNotification};
    use dereth_protocol::Message as _;
    let (mut w, player, monster) = arena();
    w.objects
        .get_mut(player)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player
        .character = Some(empyrean_store::models::shard::Character::default());
    empyrean_world::world_objects::managers::squelch_manager::handle_action_modify_global_squelch(
        &mut w,
        player,
        true,
        empyrean_entity::enums::ChatMessageType::CombatSelf,
    );
    for seed in 0..10u64 {
        ThreadSafeRandom::seed(seed);
        start_capture();
        if empyrean_world::world_objects::player_combat::damage_target(
            &mut w,
            player,
            monster,
            Some(player),
        )
        .is_none()
        {
            break;
        }
        let events: Vec<u32> = take_sent()
            .iter()
            .filter(|(_, _, b)| u32::from_le_bytes(b[..4].try_into().unwrap()) == GAME_EVENT)
            .map(|(_, _, b)| event(b).0)
            .collect();
        assert!(
            !events.contains(&AttackerNotification::OPCODE.0)
                && !events.contains(&EvasionAttackerNotification::OPCODE.0),
            "{events:04X?}"
        );
    }
}

/// V273: armour whose weenie has no `ArmorModVsNether` counts as 1.0 against
/// nether in damage, the value the retail captures showed in every appraisal of such armour; the
/// other types keep ACE's 0 for a missing multiplier, and a stored nether value is used as is.
#[test]
fn armour_without_a_nether_multiplier_counts_as_one_against_nether() {
    use empyrean_world::world_objects::monster_melee::get_resistance;
    let mut armor = WorldObject::allocate(Class::Clothing);
    assert!(
        (get_resistance(&armor, DamageType::Nether) - 1.0).abs() < f64::EPSILON,
        "missing nether is 1.0"
    );
    assert!(
        get_resistance(&armor, DamageType::Slash).abs() < f64::EPSILON,
        "missing slash is still 0"
    );
    armor.set_property(PropertyFloat::ArmorModVsNether, 0.0);
    assert!(
        get_resistance(&armor, DamageType::Nether).abs() < f64::EPSILON,
        "a stored 0.0 stays 0.0"
    );
}

mod cloak_proc_floor {
    use crate::support::content_interactions::*;

    /// With the custom cloak scale on, `cloak_min_proc` (the name the registered default has) is the
    /// floor of the proc chance: at 1.0 a level-1 cloak always procs, even from a blow of no damage.
    /// ACE read `cloak_min_proc_base`, which nothing sets, so the floor stayed 0 and it never procced.
    #[test]
    fn the_cloak_min_proc_setting_is_the_floor_of_the_proc_chance() {
        let mut h = H::new();
        assert!(pm::modify_bool(&h.w, "use_cloak_proc_custom_scale", true));
        assert!(pm::modify_double(&h.w, "cloak_max_proc_base", 0.0, false));
        assert!(pm::modify_double(&h.w, "cloak_min_proc", 1.0, false));

        let cloak = object(&mut h.w, Class::Clothing, 0x8000_0100);
        o(&mut h.w, cloak).set_property(PropertyInt::ItemMaxLevel, 1);
        o(&mut h.w, cloak).set_property(PropertyInt::ItemXpStyle, 1);
        o(&mut h.w, cloak).set_property(PropertyInt64::ItemBaseXp, 1000);
        o(&mut h.w, cloak).set_property(PropertyInt64::ItemTotalXp, 1000);
        assert_eq!(h.w.objects.get(cloak).unwrap().item_level(), Some(1));

        assert!(
            cloak::roll_proc(&mut h.w, cloak, 0.0),
            "the 1.0 floor procs"
        );
    }
}

mod damage_messages {
    use crate::support::social_world::*;

    /// `Strings.GetAttackVerb` (Strings.cs): four tiers per damage type, split at more than half, a
    /// quarter and a tenth; any other type is "hit"; a negative fraction is refused.
    #[test]
    fn attack_verbs_follow_the_damage_fraction() {
        use empyrean_entity::enums::DamageType;
        use empyrean_world::entity::strings::get_attack_verb;
        let cases = [
            (DamageType::Slash, 0.51, "mangle", "mangles"),
            (DamageType::Slash, 0.5, "slash", "slashes"),
            (DamageType::Pierce, 0.26, "impale", "impales"),
            (DamageType::Bludgeon, 0.11, "bash", "bashes"),
            (DamageType::Fire, 0.1, "singe", "singes"),
            (DamageType::Cold, 0.9, "freeze", "freezes"),
            (DamageType::Acid, 0.0, "blister", "blisters"),
            (DamageType::Electric, 0.3, "jolt", "jolts"),
            (DamageType::Nether, 0.2, "twist", "twists"),
            (DamageType::Health, 0.05, "drain", "drains"),
            (DamageType::Stamina, 0.9, "hit", "hits"),
        ];
        for (t, p, s, pl) in cases {
            assert_eq!(get_attack_verb(t, p), Some((s, pl)), "{t:?} {p}");
        }
        assert_eq!(get_attack_verb(DamageType::Slash, -0.1), None);
    }

    /// `WorldObject.GetAttackMessage` (WorldObject.cs): the verb for `amount / Health.Base` and the
    /// lower-cased damage type name.
    #[test]
    fn an_attack_message_names_the_verb_and_the_damage_type() {
        use empyrean_entity::enums::DamageType;
        let mut h = H::small();
        h.player(A, "Alpha", 3);
        h.player(B, "Bravo", 3);
        let base = {
            let o = h.w.objects.get(B).unwrap();
            o.health().base(&h.w, o)
        };
        let amount = base; // the whole base: more than half
        let msg = empyrean_world::dispatch::get_attack_message::get_attack_message(
            &h.w,
            A,
            B,
            DamageType::Fire,
            amount,
        );
        assert_eq!(
            msg,
            format!("You incinerate Bravo for {amount} points of fire damage!")
        );
    }

    /// `Aetheria.CalcProcRate` (Aetheria.cs): 1% per item level plus 0.1% per luminance surge
    /// augmentation of a player wielder (no combat-mode multiplier at peace).
    #[test]
    fn aetheria_proc_rate_is_level_and_augmentation() {
        use empyrean_entity::enums::{ItemXpStyle, PropertyInt};
        let mut h = H::small();
        h.player(A, "Alpha", 3);
        h.player(B, "Bravo", 3);
        let o = h.w.objects.get_mut(B).unwrap();
        o.set_property(PropertyInt64::ItemBaseXp, 1000);
        o.set_property(PropertyInt::ItemMaxLevel, 5);
        o.set_property(PropertyInt::ItemXpStyle, ItemXpStyle::Fixed.0);
        o.set_property(PropertyInt64::ItemTotalXp, 3000);
        let level = empyrean_world::entity::experience_system::item_total_xp_to_level(
            3000,
            1000,
            5,
            ItemXpStyle::Fixed,
        );
        assert!(level > 0);
        h.w.objects
            .get_mut(A)
            .unwrap()
            .set_lum_aug_surge_chance_rating(5);
        let rate = empyrean_world::entity::aetheria::calc_proc_rate(&h.w, B, A);
        #[allow(clippy::cast_precision_loss)]
        let expected = level as f32 * 0.01 + 5.0 * 0.001;
        assert!((rate - expected).abs() < 1e-6, "{rate} {expected}");
    }
}
