//! Vectors: fixtures/vectors/crafting/
//! RecipeManager(_New), Player_Crafting, Tailoring and TinkerLog follow ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use dereth_assets::tables::{SkillBase, SkillFormula};
use dereth_primitives::DataId;
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f64_of, u64_of, Case};
use empyrean_content::models::world::{
    Recipe, RecipeMod, RecipeModsBool, RecipeModsDID, RecipeModsFloat, RecipeModsIID,
    RecipeModsInt, RecipeModsString, RecipeRequirementsBool, RecipeRequirementsDID,
    RecipeRequirementsFloat, RecipeRequirementsIID, RecipeRequirementsInt,
    RecipeRequirementsString, Weenie as ContentWeenie,
};
use empyrean_content::MemContent;
use empyrean_dat::dat_manager::file_id;
use empyrean_dat::file_types::{SecondaryAttributeTable, SkillTable, XpTable};
use empyrean_dat::{DatManager, FakeDats};
use empyrean_entity::enums::{
    CompareType, ConfirmationType, ItemType, MaterialType, PropertyAttribute, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyString,
    RequirementType, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::models::properties_attribute::PropertiesAttribute;
use empyrean_entity::models::properties_skill::PropertiesSkill;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::salvage_results::SalvageResults;
use empyrean_world::entity::tinker_log::TinkerLog;
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::property_manager as pm;
use empyrean_world::managers::{recipe_manager as rm, recipe_manager_new as rmn};
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_skill::CreatureSkill;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::player_crafting as pc;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;
use serde_json::{json, Value};

// ------------------------------------------------------------------ the stats area's synthetic dats

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
    let out = vectors::load_named("stats", "tables")
        .cases
        .into_iter()
        .next()
        .expect("stats/tables: one case")
        .output;
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

use empyrean_testkit::EmptyShard;

/// A world over the synthetic dats and `content`, with ACE's default settings and a dynamic guid
/// allocator.
fn world_on(content: MemContent) -> World {
    world_on_arc(Arc::new(content))
}

fn world_on_arc(content: Arc<MemContent>) -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(now, dats());
    w.content = content;
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::initialize(&mut w, true);
    w
}

fn world() -> World {
    world_on(MemContent::new())
}

// ------------------------------------------------------------------ values

fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).unwrap_or_else(|| panic!("not a uint: {v}"))).expect("u32")
}

fn i(v: &Value) -> i32 {
    i32::try_from(v.as_i64().unwrap_or_else(|| panic!("not an int: {v}"))).expect("i32")
}

fn p16(v: &Value) -> u16 {
    u16::try_from(i(v)).expect("u16 property")
}

fn rows(spec: &Value, key: &str) -> Vec<Value> {
    spec.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn class_of(name: &str) -> Class {
    match name {
        "Player" => Class::Player,
        "GenericObject" => Class::GenericObject,
        "Clothing" => Class::Clothing,
        "MeleeWeapon" => Class::MeleeWeapon,
        "MissileLauncher" => Class::MissileLauncher,
        "Caster" => Class::Caster,
        "Stackable" => Class::Stackable,
        "Gem" => Class::Gem,
        "CraftTool" => Class::CraftTool,
        other => panic!("unknown class {other}"),
    }
}

/// The uninitialised object a harness spec describes (as `CraftingVectors.Build`), in the world.
fn build(w: &mut World, spec: &Value) -> ObjectGuid {
    let mut o = WorldObject::allocate(class_of(spec["class"].as_str().expect("class")));
    let id = u(&spec["id"]);
    o.guid = ObjectGuid::new(id);
    o.biota.id = id;
    o.biota.weenie_class_id = u(&spec["wcid"]);
    o.biota.weenie_type = WeenieType(u(&spec["wt"]));
    o.biota.properties_int = Some(DotNetDict::new());
    o.biota.properties_float = Some(DotNetDict::new());
    o.biota.properties_bool = Some(DotNetDict::new());
    o.biota.properties_string = Some(DotNetDict::new());
    o.biota.properties_did = Some(DotNetDict::new());
    o.biota.properties_iid = Some(DotNetDict::new());
    for r in rows(spec, "i") {
        o.set_property(PropertyInt(p16(&r[0])), i(&r[1]));
    }
    for r in rows(spec, "f") {
        o.set_property(PropertyFloat(p16(&r[0])), f64_of(&r[1]).expect("double"));
    }
    for r in rows(spec, "b") {
        o.set_property(PropertyBool(p16(&r[0])), r[1].as_bool().expect("bool"));
    }
    for r in rows(spec, "s") {
        o.set_property(
            PropertyString(p16(&r[0])),
            r[1].as_str().expect("str").to_owned(),
        );
    }
    for r in rows(spec, "d") {
        o.set_property(PropertyDataId(p16(&r[0])), u(&r[1]));
    }
    for r in rows(spec, "iid") {
        o.set_property(PropertyInstanceId(p16(&r[0])), u(&r[1]));
    }

    if o.is_creature() {
        let mut attributes = DotNetDict::new();
        for a in 1..=6u16 {
            attributes.insert(
                PropertyAttribute(a),
                PropertiesAttribute {
                    init_level: 100,
                    ..PropertiesAttribute::default()
                },
            );
        }
        o.biota.properties_attribute = Some(attributes);
        o.biota.properties_attribute_2nd = Some(DotNetDict::new());
        let mut skills = DotNetDict::new();
        for s in rows(spec, "sk") {
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
        o.biota.properties_enchantment_registry = Some(Vec::new());
        o.biota.properties_spell_book = Some(DotNetDict::new());

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
        let ids: Vec<Skill> = o
            .biota
            .properties_skill
            .as_ref()
            .map(|d| d.keys().copied().collect())
            .unwrap_or_default();
        for s in ids {
            o.skills_mut().insert(s, CreatureSkill::new(s));
        }
    }

    let g = o.guid;
    let is_player = o.is_player();
    w.objects.remove(g);
    w.objects.insert(o).expect("fresh guid");
    if is_player {
        // as the harness's uninitialised Session: the player and a game-event sequence
        let mut s = SessionData::default();
        s.set_account(
            7,
            "acct".to_owned(),
            empyrean_entity::enums::AccessLevel::Player,
        );
        s.set_player(Some(g));
        w.sessions.insert(SESSION, s);
    }
    g
}

const SESSION: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

fn rows_of<K: Copy + Ord, V>(
    keys: impl IntoIterator<Item = K>,
    get: impl Fn(K) -> Option<V>,
    key: impl Fn(K) -> i64,
    val: impl Fn(V) -> Value,
) -> Value {
    let mut ks: Vec<K> = keys.into_iter().collect();
    ks.sort();
    ks.dedup();
    Value::Array(
        ks.into_iter()
            .filter_map(|k| get(k).map(|v| json!([key(k), val(v)])))
            .collect(),
    )
}

fn f_json(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or_else(|| json!(x.to_string()), Value::Number)
}

/// An object's state as the harness's `State` writes it: biota and ephemeral keys through
/// `GetProperty`, nulls left out, plus the spell book.
fn state(w: &World, g: ObjectGuid) -> Value {
    let o = w.objects.get(g).expect("live object");
    let b = &o.biota;
    let eph_ints: Vec<PropertyInt> =
        o.wo.world_object_properties
            .ephemeral_property_ints
            .as_ref()
            .map(|d| d.keys().copied().collect())
            .unwrap_or_default();
    let ints = b
        .properties_int
        .iter()
        .flat_map(|d| d.keys().copied())
        .chain(PropertyInt::EPHEMERAL.iter().copied())
        .chain(eph_ints);
    let floats = b
        .properties_float
        .iter()
        .flat_map(|d| d.keys().copied())
        .chain(PropertyFloat::EPHEMERAL.iter().copied());
    let bools = b
        .properties_bool
        .iter()
        .flat_map(|d| d.keys().copied())
        .chain(PropertyBool::EPHEMERAL.iter().copied());
    let strings = b
        .properties_string
        .iter()
        .flat_map(|d| d.keys().copied())
        .chain(PropertyString::EPHEMERAL.iter().copied());
    let dids = b.properties_did.iter().flat_map(|d| d.keys().copied());
    let iids = b
        .properties_iid
        .iter()
        .flat_map(|d| d.keys().copied())
        .chain(PropertyInstanceId::EPHEMERAL.iter().copied());
    let mut spells: Vec<i32> = b
        .properties_spell_book
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    spells.sort_unstable();
    json!({
        "i": rows_of(ints, |k| o.get_property(k), |k| i64::from(k.0), |v| json!(v)),
        "f": rows_of(floats, |k| o.get_property(k), |k| i64::from(k.0), f_json),
        "b": rows_of(bools, |k| o.get_property(k), |k| i64::from(k.0), |v| json!(v)),
        "s": rows_of(strings, |k| o.get_property(k), |k| i64::from(k.0), |v| json!(v)),
        "d": rows_of(dids, |k| o.get_property(k), |k| i64::from(k.0), |v| json!(v)),
        "iid": rows_of(iids, |k| o.get_property(k), |k| i64::from(k.0), |v| json!(v)),
        "spells": spells,
    })
}

/// Numbers compare by value (the harness writes every double with a `.`).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            match (x.as_i64(), y.as_i64(), x.as_u64(), y.as_u64()) {
                (Some(p), Some(q), _, _) => p == q,
                (_, _, Some(p), Some(q)) => p == q,
                _ => x.as_f64().map(f64::to_bits) == y.as_f64().map(f64::to_bits),
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

/// Replays `crafting/<name>.json`: `each` answers the output in the harness's shape (a panic is
/// ACE's throw).
fn replay(name: &str, mut each: impl FnMut(&Case) -> Value) -> usize {
    let file = vectors::load_named("crafting", name);
    assert!(!file.cases.is_empty(), "crafting/{name}: no cases");
    let mut failures = Vec::new();
    for (n, case) in file.cases.iter().enumerate() {
        let got = each(case);
        if !same(&got, &case.output) {
            failures.push(format!(
                "case {n}: in {}\n    want {}\n    got  {got}",
                case.input, case.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "crafting/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures[..failures.len().min(5)].join("\n  ")
    );
    file.cases.len()
}

/// Runs `f`, answering ACE's `{"throws": ...}` shape for a panic (the type is not compared: a
/// Rust panic stands for any exception, the case must throw in ACE).
fn try_json(want: &Value, f: impl FnOnce() -> Value) -> Value {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) if want.get("throws").is_some() => want.clone(),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            json!({ "throws": format!("rust panic: {msg}") })
        }
    }
}

fn opt<T: Into<Value>>(v: Option<T>) -> Value {
    v.map_or(Value::Null, Into::into)
}

// ------------------------------------------------------------------ ACE vectors: chances

/// `RecipeManager.GetMaterialMod` over every MaterialType.
#[test]
fn material_mod_matches_ace() {
    replay("material_mod", |c| {
        f_json(f64::from(rm::get_material_mod(MaterialType(u(
            &c.input["material"]
        )))))
    });
}

/// `RecipeManager.GetTinkerChance` (the difficulty formula, the attempt table, imbues and their
/// augmentation, the foolproof wcids with the ushort cast, the Workmanship write-back, the
/// untrained refusal and the 10th-attempt throw).
#[test]
fn tinker_chance_matches_ace() {
    replay("tinker_chance", |c| {
        let mut w = world();
        let p = build(&mut w, &c.input["player"]);
        let t = build(&mut w, &c.input["tool"]);
        let g = build(&mut w, &c.input["target"]);
        let recipe = Recipe {
            id: 1,
            skill: u(&c.input["skill"]),
            salvage_type: u(&c.input["salvage_type"]),
            ..Recipe::default()
        };
        let want = &c.output["chance"];
        let chance = try_json(want, || {
            opt(rm::get_tinker_chance(&mut w, p, t, g, &recipe).map(f_json))
        });
        json!({
            "chance": chance,
            "target_iw": opt(w.objects.get(g).and_then(WorldObject::item_workmanship)),
            "tool_iw": opt(w.objects.get(t).and_then(WorldObject::item_workmanship)),
        })
    });
}

/// `RecipeManager.GetRecipeChance` for crafting recipes (HasDifficulty, ConvertToMoASkill, the
/// untrained refusal, the uint skill + LumAugSkilledCraft sum).
#[test]
fn recipe_chance_matches_ace() {
    replay("recipe_chance", |c| {
        let mut w = world();
        let p = build(&mut w, &c.input["player"]);
        let s = build(
            &mut w,
            &json!({"class": "GenericObject", "id": 0x8000_0001u32, "wcid": 3002, "wt": 1}),
        );
        let t = build(
            &mut w,
            &json!({"class": "GenericObject", "id": 0x8000_0002u32, "wcid": 3003, "wt": 1}),
        );
        let recipe = Recipe {
            id: 2,
            skill: u(&c.input["skill"]),
            difficulty: u(&c.input["difficulty"]),
            ..Recipe::default()
        };
        opt(rm::get_recipe_chance(&mut w, p, s, t, &recipe).map(f_json))
    });
}

// ------------------------------------------------------------------ ACE vectors: requirements

fn player_only(w: &mut World) -> ObjectGuid {
    build(
        w,
        &json!({"class": "Player", "id": 0x5000_0001u32, "wcid": 1, "wt": 10}),
    )
}

/// `RecipeManager.VerifyRequirement(Player, CompareType, double?, double, string)`.
#[test]
fn requirement_matches_ace() {
    let mut w = world();
    let p = player_only(&mut w);
    replay("requirement", |c| {
        let i = &c.input;
        json!(rm::verify_requirement(
            &mut w,
            p,
            CompareType(self::i(&i["compare"])),
            f64_of(&i["prop"]),
            f64_of(&i["val"]).expect("val"),
            Some("no")
        ))
    });
}

/// `RecipeManager.VerifyRequirement(Player, CompareType, string, string, string)`.
#[test]
fn requirement_string_matches_ace() {
    let mut w = world();
    let p = player_only(&mut w);
    replay("requirement_string", |c| {
        let i = &c.input;
        json!(rm::verify_requirement_string(
            &mut w,
            p,
            CompareType(self::i(&i["compare"])),
            i["prop"].as_str(),
            i["val"].as_str(),
            Some("no")
        ))
    });
}

/// `RecipeManager.VerifyRequirements(Recipe, Player, WorldObject, RequirementType)` over mixed
/// requirement lists (the index filter, the six kinds in order, the first failure).
#[test]
fn requirements_match_ace() {
    replay("requirements", |c| {
        let mut w = world();
        let p = player_only(&mut w);
        let o = build(&mut w, &c.input["obj"]);
        let mut recipe = Recipe {
            id: 3,
            ..Recipe::default()
        };
        for r in c.input["reqs"].as_array().expect("reqs") {
            let (index, stat, e) = (i8::try_from(i(&r[1])).expect("index"), i(&r[2]), i(&r[3]));
            match r[0].as_str().expect("kind") {
                "b" => recipe
                    .recipe_requirements_bool
                    .push(RecipeRequirementsBool {
                        index,
                        stat,
                        r#enum: e,
                        value: r[4].as_bool().expect("bool"),
                        message: Some("b".into()),
                        ..Default::default()
                    }),
                "i" => recipe.recipe_requirements_int.push(RecipeRequirementsInt {
                    index,
                    stat,
                    r#enum: e,
                    value: i(&r[4]),
                    message: Some("i".into()),
                    ..Default::default()
                }),
                "f" => recipe
                    .recipe_requirements_float
                    .push(RecipeRequirementsFloat {
                        index,
                        stat,
                        r#enum: e,
                        value: f64_of(&r[4]).expect("f"),
                        message: Some("f".into()),
                        ..Default::default()
                    }),
                "s" => recipe
                    .recipe_requirements_string
                    .push(RecipeRequirementsString {
                        index,
                        stat,
                        r#enum: e,
                        value: r[4].as_str().map(str::to_owned),
                        message: Some("s".into()),
                        ..Default::default()
                    }),
                "d" => recipe.recipe_requirements_did.push(RecipeRequirementsDID {
                    index,
                    stat,
                    r#enum: e,
                    value: u(&r[4]),
                    message: Some("d".into()),
                    ..Default::default()
                }),
                _ => recipe.recipe_requirements_iid.push(RecipeRequirementsIID {
                    index,
                    stat,
                    r#enum: e,
                    value: u(&r[4]),
                    message: Some("iid".into()),
                    ..Default::default()
                }),
            }
        }
        json!(rm::verify_requirements_of(
            &mut w,
            &recipe,
            p,
            o,
            RequirementType(i(&c.input["req_type"]))
        ))
    });
}

/// `new TinkerLog(csv)` (`Enum.TryParse` ignoring case) and `NumTinkers`.
#[test]
fn tinker_log_matches_ace() {
    replay("tinker_log", |c| {
        let log = TinkerLog::new(c.input["csv"].as_str());
        json!({"num": log.num_tinkers(MaterialType::Steel), "tinkers": log.tinkers.iter().map(|m| m.0).collect::<Vec<_>>()})
    });
}

// ------------------------------------------------------------------ ACE vectors: recipes applied

fn seed(s: &Value) {
    let s = i32::try_from(s.as_i64().expect("seed")).expect("an int seed");
    ThreadSafeRandom::seed(u64::from(s.cast_unsigned()));
}

fn post() -> Value {
    json!(ThreadSafeRandom::next(0, 999_999))
}

fn sorted(set: &DotNetHashSet<u32>) -> Value {
    let mut v: Vec<u32> = set.iter().copied().collect();
    v.sort_unstable();
    json!(v)
}

/// `RecipeManager.ModifyItem` over seeded synthetic recipes: every mod kind, operation, target
/// (`GetTargetMod`) and source (`GetSourceMod`, with ACE's NullReferenceException for a missing
/// result or an unknown source), AddSpell, and the mutation scripts.
#[test]
fn modify_item_matches_ace() {
    replay("modify_item", |c| {
        let i = &c.input;
        let mut w = world();
        let p = build(&mut w, &i["player"]);
        let s = build(&mut w, &i["source"]);
        let t = build(&mut w, &i["target"]);
        let r = (!i["result"].is_null()).then(|| build(&mut w, &i["result"]));
        let mut recipe = Recipe {
            id: 4,
            ..Recipe::default()
        };
        for m in i["mods"].as_array().expect("mods") {
            let mut rm_ = RecipeMod {
                executes_on_success: m["success"].as_bool().expect("bool"),
                data_id: self::i(&m["data_id"]),
                ..RecipeMod::default()
            };
            for row in m["rows"].as_array().expect("rows") {
                let (index, stat, e, source) = (
                    i8::try_from(self::i(&row[1])).expect("index"),
                    self::i(&row[2]),
                    self::i(&row[3]),
                    self::i(&row[4]),
                );
                match row[0].as_str().expect("kind") {
                    "b" => rm_.recipe_mods_bool.push(RecipeModsBool {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: row[5].as_bool().expect("b"),
                        ..Default::default()
                    }),
                    "i" => rm_.recipe_mods_int.push(RecipeModsInt {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: self::i(&row[5]),
                        ..Default::default()
                    }),
                    "f" => rm_.recipe_mods_float.push(RecipeModsFloat {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: f64_of(&row[5]).expect("f"),
                        ..Default::default()
                    }),
                    "s" => rm_.recipe_mods_string.push(RecipeModsString {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: row[5].as_str().map(str::to_owned),
                        ..Default::default()
                    }),
                    "d" => rm_.recipe_mods_did.push(RecipeModsDID {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: u(&row[5]),
                        ..Default::default()
                    }),
                    _ => rm_.recipe_mods_iid.push(RecipeModsIID {
                        index,
                        stat,
                        r#enum: e,
                        source,
                        value: u(&row[5]),
                        ..Default::default()
                    }),
                }
            }
            recipe.recipe_mod.push(rm_);
        }
        seed(&i["seed"]);
        let success = i["success"].as_bool().expect("success");
        try_json(&c.output, || {
            let modified = rm::modify_item(&mut w, p, &recipe, s, t, r, success);
            json!({
                "modified": sorted(&modified),
                "player": state(&w, p),
                "post": post(),
                "result": r.map_or(Value::Null, |r| state(&w, r)),
                "source": state(&w, s),
                "target": state(&w, t),
            })
        })
    });
}

/// `RecipeManager.TryMutate` with every tinkering mutation script (MutationCache), one
/// draw each, and `HandleTinkerLog`.
#[test]
fn try_mutate_matches_ace() {
    replay("try_mutate", |c| {
        let i = &c.input;
        let mut w = world();
        let p = player_only(&mut w);
        let s = build(&mut w, &i["source"]);
        let t = build(&mut w, &i["target"]);
        seed(&i["seed"]);
        try_json(&c.output, || {
            let mut modified = DotNetHashSet::new();
            let result = rm::try_mutate(
                &mut w,
                p,
                s,
                t,
                &Recipe {
                    id: 5,
                    ..Recipe::default()
                },
                u(&i["id"]),
                &mut modified,
            );
            json!({"modified": sorted(&modified), "post": post(), "result": result, "target": state(&w, t)})
        })
    });
}

// ------------------------------------------------------------------ ACE vectors: salvage

/// The salvage-bag weenies of `salvage_bags`' first case (and the new_recipe targets and recipes).
fn crafting_content() -> MemContent {
    let bags = vectors::load_named("crafting", "salvage_bags")
        .cases
        .into_iter()
        .next()
        .expect("a case")
        .input["bag_weenies"]
        .clone();
    let mut content = MemContent::new();
    for b in bags.as_array().expect("bag weenies") {
        let wcid = u(&b["wcid"]);
        let mut wn = ContentWeenie::new(wcid, &format!("u59bag{wcid}"), WeenieType::CraftTool)
            .with_string(PropertyString::Name, "Salvage")
            .with_int(
                PropertyInt::ItemType,
                i32::try_from(ItemType::TinkeringMaterial.0).expect("item type"),
            )
            .with_int(PropertyInt::MaxStructure, 100)
            .with_int(PropertyInt::MaterialType, i(&b["material"]));
        for r in b["ints"].as_array().expect("ints") {
            wn = wn.with_int(PropertyInt(p16(&r[0])), i(&r[1]));
        }
        content = content.weenie(wn);
    }
    let nr = vectors::load_named("crafting", "new_recipe")
        .cases
        .into_iter()
        .next()
        .expect("a case")
        .input["targets"]
        .clone();
    for t in nr.as_array().expect("targets") {
        let wcid = u(&t["wcid"]);
        let mut wn = ContentWeenie::new(wcid, &format!("u59target{wcid}"), WeenieType(u(&t["wt"])));
        let inscription = rows(t, "s")
            .into_iter()
            .find(|r| i(&r[0]) == i32::from(PropertyString::Inscription.0))
            .map(|r| r[1].as_str().expect("s").to_owned());
        if inscription.is_some() || !wcid.is_multiple_of(3) {
            wn = wn.with_string(PropertyString::Name, "t");
        }
        if let Some(s) = inscription {
            wn = wn.with_string(PropertyString::Inscription, &s);
        }
        content = content.weenie(wn);
    }
    for id in recipe_ids() {
        content = content.recipe(Recipe {
            id,
            ..Recipe::default()
        });
    }
    content
}

/// Every recipe id GetNewRecipe names (as the harness installs them).
fn recipe_ids() -> Vec<u32> {
    let mut ids: Vec<u32> = rmn::SOURCE_TO_RECIPE.values().copied().collect();
    ids.extend([
        3844, 9068, 9069, 3977, 9070, 4426, 8003, 3851, 3854, 3978, 3858, 3855, 3857, 3979, 5202,
        3848, 8700, 8701, 8699, 9133,
    ]);
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn messages(results: &SalvageResults) -> Value {
    Value::Array(
        results
            .get_messages()
            .iter()
            .map(|(skill, ms)| {
                json!([
                    skill.0,
                    ms.iter()
                        .map(|m| json!([
                            m.amount,
                            m.material_type.0,
                            f_json(f64::from(m.workmanship)),
                            m.num_items_in_material,
                            m.skill.0
                        ]))
                        .collect::<Vec<_>>()
                ])
            })
            .collect(),
    )
}

/// `Player.GetStructure`: the salvaging vs. highest trained tinkering yield, the workmanship cap,
/// the stack, a bag's own structure, and the message it is counted in.
#[test]
fn salvage_structure_matches_ace() {
    replay("salvage_structure", |c| {
        let mut w = world();
        let p = build(&mut w, &c.input["player"]);
        let it = build(&mut w, &c.input["item"]);
        let mut results = SalvageResults::new();
        let mut message = None;
        let structure = pc::get_structure(&mut w, p, it, &mut results, &mut message);
        json!({"item_iw": opt(w.objects.get(it).and_then(WorldObject::item_workmanship)), "messages": messages(&results), "structure": structure})
    });
}

/// `Player.AddSalvage` over item sequences: `GetSalvageBag` (reuse or a new bag from the factory,
/// its TOD fields cleared), `TryAddSalvage` (space, workmanship, bag-combining overflows, the
/// `salvage_handle_overages` switch), the bag value (capped at 75000) and the messages.
#[test]
fn salvage_bags_match_ace() {
    let content = Arc::new(crafting_content());
    replay("salvage_bags", |c| {
        let mut w = world_on_arc(Arc::clone(&content));
        pm::modify_bool(
            &w,
            "salvage_handle_overages",
            c.input["overages"].as_bool().expect("bool"),
        );
        let p = build(&mut w, &c.input["player"]);
        // (the items' ids moved out of the dynamic range, where the new bags' guids come from; ids
        // are not compared)
        let items: Vec<ObjectGuid> = c.input["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s["id"] = json!(u(&s["id"]) - 0x1000_0000);
                build(&mut w, &s)
            })
            .collect();
        let mut bags = Vec::new();
        let mut results = SalvageResults::new();
        try_json(&c.output, || {
            for &it in &items {
                pc::add_salvage(&mut w, p, &mut bags, it, &mut results);
            }
            let bag_rows: Vec<Value> = bags
                .iter()
                .map(|&b| {
                    let o = w.objects.get(b).expect("bag");
                    json!([
                        o.biota.weenie_class_id,
                        opt(o.structure()),
                        opt(o.item_workmanship()),
                        opt(o.num_items_in_material()),
                        opt(o.value()),
                        opt(empyrean_world::dispatch::name::name(&w, b)),
                        opt(o.material_type().map(|m| m.0))
                    ])
                })
                .collect();
            let item_rows: Vec<Value> = items
                .iter()
                .map(|&it| {
                    let o = w.objects.get(it).expect("item");
                    json!([opt(o.num_items_in_material()), opt(o.item_workmanship())])
                })
                .collect();
            json!({"bags": bag_rows, "items": item_rows, "messages": messages(&results)})
        })
    });
}

// ------------------------------------------------------------------ ACE vectors: GetNewRecipe

/// `RecipeManager.GetNewRecipe` for every source class it names (and one past the ushort cast)
/// against a spread of targets: the dye, ivory, leather, sandstone, workmanship, weapon, caster,
/// jewelry, armor, imbue, shield-cover, slayer, Paragon and uninscription rules.
#[test]
fn new_recipe_matches_ace() {
    let file = vectors::load_named("crafting", "new_recipe");
    let targets = file.cases[0].input["targets"]
        .as_array()
        .expect("targets")
        .clone();
    let mut w = world_on(crafting_content());
    let p = player_only(&mut w);
    replay("new_recipe", |c| {
        let i = &c.input;
        let mut src = json!({"class": "GenericObject", "id": 0x8000_0001u32, "wcid": u(&i["source_wcid"]), "wt": 1});
        if let Some(m) = i["source_material"].as_i64() {
            src["i"] = json!([[i32::from(PropertyInt::MaterialType.0), m]]);
        }
        let s = build(&mut w, &src);
        let t = build(
            &mut w,
            &targets[usize::try_from(u(&i["target"])).expect("index")],
        );
        opt(rmn::get_new_recipe(&w, p, s, t).map(|r| r.id))
    });
}

// ------------------------------------------------------------------ flows (hand-derived)

use empyrean_world::entity::confirmation::Confirmation;
use empyrean_world::entity::tailoring;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::world_objects::container;
use empyrean_world::world_objects::managers::confirmation_manager;
use empyrean_world::world_objects::world_object::CtorEnv;

const FLOW_PLAYER: u32 = 0x5000_0002;
const HUMAN: u32 = 1;
const BREASTPLATE: u32 = 11; // chest armor, the look donor
const COAT: u32 = 12; // chest armor, the target of the look
const SHIRT: u32 = 13; // chest and upper arms, for the reduction tool

fn flow_content() -> MemContent {
    use empyrean_entity::enums::{CoverageMask, EquipMask};
    let armor = |wcid: u32, name: &str, locations: u32, priority: u32, setup: u32| {
        ContentWeenie::new(wcid, name, WeenieType::Clothing)
            .with_string(PropertyString::Name, name)
            .with_int(
                PropertyInt::ItemType,
                i32::try_from(ItemType::Armor.0).unwrap(),
            )
            .with_int(PropertyInt::ValidLocations, locations.cast_signed())
            .with_int(PropertyInt::ClothingPriority, priority.cast_signed())
            .with_int(PropertyInt::ArmorLevel, 100)
            .with_int(PropertyInt::ItemWorkmanship, 5)
            .with_int(
                PropertyInt::PaletteTemplate,
                i32::try_from(setup % 256).unwrap(),
            )
            .with_did(PropertyDataId::Setup, setup)
            .with_did(PropertyDataId::Icon, setup + 1)
    };
    let kit = |wcid: u32, name: &str| {
        ContentWeenie::new(wcid, name, WeenieType::Generic)
            .with_string(PropertyString::Name, name)
            .with_int(PropertyInt::ItemType, 0x10)
    };
    let chest = EquipMask::ChestArmor.0;
    let outer_chest = CoverageMask::OuterwearChest.0;
    MemContent::new()
        .weenie(
            ContentWeenie::new(HUMAN, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_string(PropertyString::Name, "Tailor")
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(armor(
            BREASTPLATE,
            "Breastplate",
            chest,
            outer_chest,
            0x0200_0101,
        ))
        .weenie(armor(COAT, "Coat", chest, outer_chest, 0x0200_0202))
        .weenie(armor(
            SHIRT,
            "Shirt",
            chest | EquipMask::UpperArmArmor.0,
            outer_chest | CoverageMask::OuterwearUpperArms.0,
            0x0200_0303,
        ))
        .weenie(kit(tailoring::ARMOR_TAILORING_KIT, "Armor Tailoring Kit"))
        .weenie(kit(
            tailoring::ARMOR_MAIN_REDUCTION_TOOL,
            "Armor Main Reduction Tool",
        ))
        .weenie(kit(
            tailoring::ARMOR_LAYERING_TOOL_TOP,
            "Armor Layering Tool (Top)",
        ))
        .weenie(kit(tailoring::LEATHER_VEST, "Leather Vest"))
}

/// A world on `flow_content` with a player (Strength 100) whose session is SESSION.
fn flow_world() -> (World, ObjectGuid) {
    let mut w = world_on(flow_content());
    let weenie = w.content.get_cached_weenie(HUMAN).expect("player weenie");
    let mut o = CtorEnv::with_world(&w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            ObjectGuid::new(FLOW_PLAYER),
            1,
        )
    });
    let rec = o
        .biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(PropertyAttribute::Strength, Default::default);
    rec.init_level = 100;
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character::default());
    w.objects.insert(o).expect("fresh");
    let mut s = SessionData::default();
    s.set_account(
        7,
        "acct".to_owned(),
        empyrean_entity::enums::AccessLevel::Player,
    );
    s.set_player(Some(ObjectGuid::new(FLOW_PLAYER)));
    w.sessions.insert(SESSION, s);
    // `PlayerManager.AddPlayerToOnlinePlayers`: the Confirmation targets find the player there
    w.player_manager.online_players.insert(
        FLOW_PLAYER,
        empyrean_world::managers::player_manager::OnlinePlayer {
            guid: ObjectGuid::new(FLOW_PLAYER),
            account: None,
        },
    );
    (w, ObjectGuid::new(FLOW_PLAYER))
}

fn give(w: &mut World, player: ObjectGuid, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(weenie), guid)
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    assert!(container::try_add_to_inventory(
        w, player, guid, 0, false, true
    ));
    guid
}

/// The kind of each message sent: the game-event type inside a `0xF7B0`, else the opcode.
fn sent_kinds() -> Vec<u32> {
    take_sent()
        .into_iter()
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == 0xF7B0 {
                word(12)
            } else {
                word(0)
            }
        })
        .collect()
}

/// `Tailoring.TailorArmor` then `ArmorApply`: the kit takes the breastplate's look into a Leather
/// Vest intermediate (`GetArmorWCID(ChestArmor)`, `SetArmorProperties`), consuming kit and
/// breastplate; the vest then gives the coat that look (`UpdateArmorProps`) and is consumed.
/// `VerifyUseRequirements` refuses the same item twice and a retained target.
#[test]
fn tailoring_moves_a_look_from_one_armor_to_another() {
    use empyrean_entity::enums::WeenieError;
    let (mut w, p) = flow_world();
    let kit = give(&mut w, p, tailoring::ARMOR_TAILORING_KIT);
    let donor = give(&mut w, p, BREASTPLATE);
    let coat = give(&mut w, p, COAT);

    w.objects.get_mut(donor).unwrap().set_retained(true);
    assert_eq!(
        tailoring::verify_use_requirements(&mut w, p, kit, donor),
        WeenieError::YouDoNotPassCraftingRequirements
    );
    w.objects.get_mut(donor).unwrap().set_retained(false);
    assert_eq!(
        tailoring::verify_use_requirements(&mut w, p, kit, kit),
        WeenieError::YouDoNotPassCraftingRequirements
    );
    assert_eq!(
        tailoring::verify_use_requirements(&mut w, p, kit, donor),
        WeenieError::None
    );

    tailoring::do_tailoring(&mut w, p, kit, donor);
    assert!(
        w.objects.get(kit).is_none() && w.objects.get(donor).is_none(),
        "kit and donor are consumed"
    );
    let vest = container::inventory_values(&w, p)
        .into_iter()
        .find(|&g| w.objects.get(g).unwrap().biota.weenie_class_id == tailoring::LEATHER_VEST)
        .expect("the intermediate");
    let v = w.objects.get(vest).unwrap();
    assert_eq!(
        (v.setup_table_id(), v.icon_id(), v.palette_template()),
        (0x0200_0101, 0x0200_0102, Some(1))
    );
    assert_eq!(
        v.get_property(PropertyString::Name).as_deref(),
        Some("Breastplate")
    );
    assert_eq!(
        v.target_type(),
        Some(ItemType(ItemType::Armor.0 | ItemType::Clothing.0))
    );

    start_capture();
    let ((), errors) =
        crate::log_capture::capture(|| tailoring::do_tailoring(&mut w, p, vest, coat));
    let sent = take_sent();
    let kinds: Vec<u32> = sent
        .iter()
        .map(|(_, _, b)| {
            let word = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
            if word(0) == 0xF7B0 {
                word(12)
            } else {
                word(0)
            }
        })
        .collect();
    // V236, V237.
    let update = &sent
        .iter()
        .find(|(_, _, b)| b[..4] == 0xF7DBu32.to_le_bytes())
        .expect("the UpdateObject")
        .2;
    let desc = dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemUpdateObject>(
        &update[4..],
    )
    .expect("decodes")
    .0;
    assert_eq!(
        desc.wdesc.icon_id, 0x0600_0000,
        "the icon base plus 0: none"
    );
    assert!(
        errors.iter().any(|l| l.starts_with("content error")
            && l.contains(&format!("{coat}"))
            && l.contains("0x02000102")),
        "{errors:?}"
    );
    let c = w.objects.get(coat).unwrap();
    assert_eq!(
        (c.setup_table_id(), c.icon_id(), c.palette_template()),
        (0x0200_0101, 0x0200_0102, Some(1))
    );
    assert_eq!(
        c.get_property(PropertyString::Name).as_deref(),
        Some("Breastplate")
    );
    assert!(w.objects.get(vest).is_none(), "the vest is consumed");
    // the chat first; then the property updates; the UpdateObject; the vest's removal and burden;
    // UseDone
    assert_eq!(kinds.first(), Some(&0xF7E0), "{kinds:04X?}");
    assert_eq!(
        &kinds[kinds.len() - 4..],
        [0xF7DB, 0x0024, 0x02CD, 0x01C7],
        "{kinds:04X?}"
    );
}

/// `Tailoring.TailorReduceArmor` (main tool: a chest-and-arms piece becomes chest only,
/// OuterwearChest) and `TailorLayerArmor` (top: TopLayerPriority true); each consumes its tool.
#[test]
fn reduction_and_layering_tools_change_the_armor() {
    use empyrean_entity::enums::{CoverageMask, EquipMask};
    let (mut w, p) = flow_world();
    let tool = give(&mut w, p, tailoring::ARMOR_MAIN_REDUCTION_TOOL);
    let shirt = give(&mut w, p, SHIRT);
    tailoring::do_tailoring(&mut w, p, tool, shirt);
    let s = w.objects.get(shirt).unwrap();
    assert_eq!(
        (s.valid_locations(), s.clothing_priority()),
        (
            Some(EquipMask::ChestArmor),
            Some(CoverageMask::OuterwearChest)
        )
    );
    assert!(w.objects.get(tool).is_none());

    let layer = give(&mut w, p, tailoring::ARMOR_LAYERING_TOOL_TOP);
    tailoring::do_tailoring(&mut w, p, layer, shirt);
    assert_eq!(
        w.objects
            .get(shirt)
            .unwrap()
            .get_property(PropertyBool::TopLayerPriority),
        Some(true)
    );
    assert!(w.objects.get(layer).is_none());
}

/// The dialog answered no: `Confirmation_CraftInteration.ProcessConfirmation` sends
/// `YouChickenOut` and nothing is crafted; a stale context id is refused and the confirmation
/// stays pending; `ConfirmationManager.EnqueueAbort` sends ConfirmationDone and the timeout
/// message.
#[test]
fn a_refused_or_expired_craft_confirmation_crafts_nothing() {
    let (mut w, p) = flow_world();
    let a = give(&mut w, p, COAT);
    let b = give(&mut w, p, SHIRT);

    let craft = ConfirmationType::CraftInteraction;
    let pending = |w: &World| {
        w.objects
            .get(p)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .confirmation_manager
            .get(craft)
            .map(|c| c.context_id)
    };
    assert!(confirmation_manager::enqueue_send(
        &mut w,
        p,
        Confirmation::craft_interation(p, a, b),
        "sure?"
    ));
    assert!(
        !confirmation_manager::enqueue_send(
            &mut w,
            p,
            Confirmation::craft_interation(p, a, b),
            "again?"
        ),
        "one craft confirmation at a time"
    );
    let ctx = pending(&w).unwrap();
    assert!(
        !confirmation_manager::handle_response(&mut w, p, craft, ctx + 7, true, false),
        "a stale context id"
    );
    assert_eq!(pending(&w), Some(ctx), "put back");

    start_capture();
    assert!(confirmation_manager::handle_response(
        &mut w, p, craft, ctx, false, false
    ));
    assert_eq!(sent_kinds(), [0x028A], "YouChickenOut");
    assert!(w.objects.get(a).is_some() && w.objects.get(b).is_some());

    // a second request, left to expire
    assert!(confirmation_manager::enqueue_send(
        &mut w,
        p,
        Confirmation::craft_interation(p, a, b),
        "sure?"
    ));
    let ctx = pending(&w).unwrap();
    start_capture();
    confirmation_manager::enqueue_abort(&mut w, p, craft, ctx);
    assert_eq!(
        sent_kinds(),
        [0x0276, 0xF7E0],
        "ConfirmationDone, then the message"
    );
}

/// `RecipeManager.UseObjectOnTarget`'s refusals before any recipe: busy (YoureTooBusy), an item on
/// itself (the chat line, the transient string and UseDone).
#[test]
fn use_object_on_target_refusals() {
    let (mut w, p) = flow_world();
    let a = give(&mut w, p, COAT);
    w.objects.get_mut(p).unwrap().wo.world_object.is_busy = true;
    start_capture();
    rm::use_object_on_target(&mut w, p, a, a, false);
    assert_eq!(sent_kinds(), [0x01C7]);
    w.objects.get_mut(p).unwrap().wo.world_object.is_busy = false;
    start_capture();
    rm::use_object_on_target(&mut w, p, a, a, false);
    assert_eq!(sent_kinds(), [0xF7E0, 0x02EB, 0x01C7]);
}

/// `RecipeManager.CreateDestroyItems`: two draws decide the target's and the source's
/// destruction against the recipe's chances (here 0 for the target and 1 for the source): the
/// source is consumed, the target kept, the destroy message and the recipe message are sent.
#[test]
fn create_destroy_items_follows_the_recipe_chances() {
    let (mut w, p) = flow_world();
    let a = give(&mut w, p, COAT);
    let b = give(&mut w, p, SHIRT);
    let recipe = Recipe {
        id: 9,
        success_destroy_source_chance: 1.0,
        success_destroy_source_amount: 1,
        success_destroy_source_message: Some("The coat is used up.".into()),
        success_destroy_target_chance: 0.0,
        success_message: Some("Done.".into()),
        ..Recipe::default()
    };
    start_capture();
    let modified =
        rm::create_destroy_items(&mut w, p, &recipe, a, b, 1.0, true).expect("no product to miss");
    assert!(modified.is_empty());
    assert!(
        w.objects.get(a).is_none() && w.objects.get(b).is_some(),
        "the source goes, the target stays"
    );
    // the source's removal and burden, its destroy message, then the recipe's message
    assert_eq!(sent_kinds(), [0x0024, 0x02CD, 0xF7E0, 0xF7E0]);
}
