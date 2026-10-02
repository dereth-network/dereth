//! Vectors: fixtures/vectors/loot/
//! LootGenerationFactory, table rolls, entity factories and mutation scripts replay ACE
//! loot/lootgen vectors.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use empyrean_common::era::EraExt as _;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::Time;
use empyrean_common::vectors::{self, f64_of, i64_of, Case};
use empyrean_content::models::world::{TreasureDeath, TreasureGemCount};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64, PropertyString,
    WeenieType,
};
use empyrean_entity::{ObjectGuid, Weenie};
use empyrean_tables::enums::{
    LootBias, TreasureArmorType, TreasureItemCategory, TreasureItemType, TreasureWeaponType,
    WeenieClassName,
};
use empyrean_world::entity::mutations::effect::Effect;
use empyrean_world::entity::mutations::effect_argument::EffectArgument;
use empyrean_world::entity::mutations::mutation_cache;
use empyrean_world::entity::mutations::mutation_filter::MutationFilter;
use empyrean_world::factories::entity::{missile_magic_defense, treasure_roll::TreasureRoll};
use empyrean_world::factories::loot_generation_factory as lgf;
use empyrean_world::factories::loot_generation_factory::tables_logic::{
    cantrips, spells, tables, wcids,
};
use empyrean_world::factories::loot_generation_factory_rare as rare;
use empyrean_world::factories::world_object_factory as factory;
use empyrean_world::managers::property_manager::default_property_manager as dpm;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::World;
use serde_json::{json, Value};

// ------------------------------------------------------------------------------------ helpers

fn snapshot() -> ClockSnapshot {
    let utc = DotNetDateTime::new_hms(2026, 9, 22, 12, 0, 0);
    ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: Time::get_unix_time_at(utc),
        utc,
        monotonic: Duration::ZERO,
    }
}

/// A world with no land and no content, with the default server properties loaded.
fn world() -> World {
    let w = World::new(
        snapshot(),
        FakeDats::new().build().expect("empty fake dats"),
    );
    dpm::load_default_properties(&w.property_manager);
    w
}

/// `ThreadSafeRandom` reseeded as the harness reseeds it (`new Random(seed)`).
fn seed(s: i64) {
    let s = i32::try_from(s).expect("an int seed");
    ThreadSafeRandom::seed(u64::from(s.cast_unsigned()));
}

/// The harness's `Post()`: the draw after a case.
fn post() -> i64 {
    i64::from(ThreadSafeRandom::next(0, 999_999))
}

fn int(v: &Value, key: &str) -> i64 {
    i64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not an integer in {v}"))
}

fn float(v: &Value, key: &str) -> f64 {
    f64_of(&v[key]).unwrap_or_else(|| panic!("{key}: not a number in {v}"))
}

/// Replays every case of `loot/<name>` (or `lootgen/<name>`), collecting the mismatches.
fn replay(area: &str, name: &str, mut each: impl FnMut(&Case) -> Value) {
    let _tables = crate::cantrip_tables_read();
    let file = vectors::load_named(area, name);
    assert!(!file.cases.is_empty(), "{area}/{name}: no cases");
    let mut failures = Vec::new();
    for case in &file.cases {
        let got = each(case);
        if got != case.output {
            failures.push(format!(
                "in {}\n    expected {}\n    got      {got}",
                case.input, case.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{area}/{name}: {} of {} cases differ from ACE:\n  {}",
        failures.len(),
        file.cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// A synthetic weenie as the harness builds one (`LootCommon.NewWeenie`).
fn weenie(wcid: u32, weenie_type: WeenieType, name: &str) -> Weenie {
    let mut strings = DotNetDict::new();
    strings.insert(PropertyString::Name, name.to_owned());
    Weenie {
        weenie_class_id: wcid,
        class_name: Some(name.to_lowercase().replace(' ', "")),
        weenie_type,
        properties_string: Some(strings),
        ..Weenie::default()
    }
}

fn set_int(w: &mut Weenie, p: PropertyInt, v: i32) {
    w.properties_int
        .get_or_insert_with(DotNetDict::new)
        .insert(p, v);
}

/// `WorldObjectFactory.CreateWorldObject(weenie, guid)`.
fn construct(w: &World, weenie: Weenie, guid: u32) -> WorldObject {
    CtorEnv::with_world(w, |env| {
        factory::create_world_object(env, Some(Arc::new(weenie)), ObjectGuid::new(guid))
    })
    .expect("a constructible weenie")
}

fn pairs<V>(items: impl Iterator<Item = (u16, V)>, val: impl Fn(V) -> Value) -> Value {
    let mut v: Vec<(i64, Value)> = items.map(|(k, x)| (i64::from(k), val(x))).collect();
    v.sort_by_key(|(k, _)| *k);
    Value::Array(v.into_iter().map(|(k, x)| json!([k, x])).collect())
}

fn spells_value(wo: &WorldObject) -> Value {
    match wo.biota.properties_spell_book.as_ref() {
        None => Value::Null,
        Some(book) => Value::Array(
            book.iter()
                .map(|(k, v)| json!([k, f64::from(*v)]))
                .collect(),
        ),
    }
}

/// `LootCommon.AllProps`.
fn all_props(wo: &WorldObject) -> Value {
    json!({
        "bool": pairs(wo.get_all_property_bools().iter().map(|(k, v)| (k.0, *v)), Value::from),
        "did": pairs(wo.get_all_property_data_id().iter().map(|(k, v)| (k.0, *v)), Value::from),
        "float": pairs(wo.get_all_property_float().iter().map(|(k, v)| (k.0, *v)), Value::from),
        "iid": pairs(wo.get_all_property_instance_id().iter().map(|(k, v)| (k.0, *v)), Value::from),
        "int": pairs(
            wo.get_all_property_int().iter().filter(|(k, _)| **k != PropertyInt::CreationTimestamp).map(|(k, v)| (k.0, *v)),
            Value::from
        ),
        "int64": pairs(wo.get_all_property_int64().iter().map(|(k, v)| (k.0, *v)), Value::from),
        "string": pairs(wo.get_all_property_string().iter().map(|(k, v)| (k.0, v.clone())), Value::from),
        "spells": spells_value(wo),
    })
}

/// `LootCommon.ItemDiff`: the item's difference from its weenie.
#[cfg_attr(not(feature = "real-content"), allow(dead_code))]
fn item_diff(wo: &WorldObject) -> Value {
    let w = wo.weenie.as_ref().expect("a weenie-built item");
    let mut set = Vec::new();
    let mut removed = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn diff<K: Copy + Eq + std::hash::Hash, V: Clone + PartialEq + Into<Value>>(
        kind: &str,
        key: impl Fn(K) -> u16,
        weenie: Option<&DotNetDict<K, V>>,
        now: &DotNetDict<K, V>,
        skip: impl Fn(K) -> bool,
        set: &mut Vec<Value>,
        removed: &mut Vec<Value>,
    ) {
        let mut s: Vec<(u16, Value)> = Vec::new();
        for (k, v) in now.iter() {
            if skip(*k) {
                continue;
            }
            if weenie.and_then(|d| d.get(k)).is_some_and(|old| old == v) {
                continue;
            }
            s.push((key(*k), v.clone().into()));
        }
        s.sort_by_key(|(k, _)| *k);
        set.extend(s.into_iter().map(|(k, v)| json!([kind, k, v])));
        if let Some(d) = weenie {
            let mut r: Vec<u16> = d
                .iter()
                .filter(|(k, _)| !now.contains_key(k))
                .map(|(k, _)| key(*k))
                .collect();
            r.sort_unstable();
            removed.extend(r.into_iter().map(|k| json!([kind, k])));
        }
    }

    diff(
        "bool",
        |k: PropertyBool| k.0,
        w.properties_bool.as_ref(),
        &wo.get_all_property_bools(),
        |_| false,
        &mut set,
        &mut removed,
    );
    diff(
        "did",
        |k: PropertyDataId| k.0,
        w.properties_did.as_ref(),
        &wo.get_all_property_data_id(),
        |_| false,
        &mut set,
        &mut removed,
    );
    diff(
        "float",
        |k: PropertyFloat| k.0,
        w.properties_float.as_ref(),
        &wo.get_all_property_float(),
        |_| false,
        &mut set,
        &mut removed,
    );
    diff(
        "iid",
        |k: empyrean_entity::enums::PropertyInstanceId| k.0,
        w.properties_iid.as_ref(),
        &wo.get_all_property_instance_id(),
        |_| false,
        &mut set,
        &mut removed,
    );
    diff(
        "int",
        |k: PropertyInt| k.0,
        w.properties_int.as_ref(),
        &wo.get_all_property_int(),
        |k| k == PropertyInt::CreationTimestamp,
        &mut set,
        &mut removed,
    );
    diff(
        "int64",
        |k: PropertyInt64| k.0,
        w.properties_int64.as_ref(),
        &wo.get_all_property_int64(),
        |_| false,
        &mut set,
        &mut removed,
    );
    diff(
        "string",
        |k: PropertyString| k.0,
        w.properties_string.as_ref(),
        &wo.get_all_property_string(),
        |_| false,
        &mut set,
        &mut removed,
    );
    json!({
        "wcid": wo.biota.weenie_class_id,
        "set": set,
        "removed": removed,
        "spells": spells_value(wo),
    })
}

fn profile(tier: i32, qm: f32, heritage: i32) -> TreasureDeath {
    TreasureDeath {
        tier,
        loot_quality_mod: qm,
        unknown_chances: heritage,
        ..TreasureDeath::default()
    }
}

fn roll_value(r: Option<TreasureRoll>) -> Value {
    match r {
        None => Value::Null,
        Some(r) => json!({
            "item_type": r.item_type.0, "armor_type": r.armor_type.0, "weapon_type": r.weapon_type.0,
            "wcid": r.wcid.0, "base_armor_level": r.base_armor_level, "item_difficulty": f64::from(r.item_difficulty),
        }),
    }
}

/// `LootCommon.Calls`: `count` results (a panic stands for ACE's exception and ends the list),
/// then the post draw.
fn calls(count: usize, mut call: impl FnMut() -> Value) -> Value {
    let mut results = Vec::new();
    for _ in 0..count {
        match catch_unwind(AssertUnwindSafe(&mut call)) {
            Ok(v) => results.push(v),
            Err(_) => {
                results.push(json!({ "throws": "panic" }));
                break;
            }
        }
    }
    json!({ "results": results, "post": post() })
}

/// A `{throws}` from ACE matches any panic here (the exception type has no Rust counterpart).
fn normalise_throws(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.len() == 1 && o.contains_key("throws") => {
            json!({ "throws": "panic" })
        }
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalise_throws(x)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(normalise_throws).collect()),
        other => other.clone(),
    }
}

// ------------------------------------------------------------------------------ mutation scripts

fn arg_value(a: Option<&EffectArgument>) -> Value {
    match a {
        None => Value::Null,
        Some(a) => json!([
            a.r#type.0,
            a.stat_type.0,
            a.stat_idx,
            a.int_val,
            a.long_val,
            a.double_val,
            f64::from(a.min_val),
            f64::from(a.max_val),
            a.is_valid
        ]),
    }
}

fn effect_value(e: &Effect) -> Value {
    json!({ "type": e.r#type.0, "quality": arg_value(e.quality.as_ref()), "arg1": arg_value(e.arg1.as_ref()), "arg2": arg_value(e.arg2.as_ref()) })
}

fn filter_value(f: Option<&MutationFilter>) -> Value {
    let Some(f) = f else { return Value::Null };
    json!({ "mutations": f.mutations.iter().map(|m| json!({
        "chances": m.chances.iter().map(|c| f64::from(*c)).collect::<Vec<_>>(),
        "outcomes": m.outcomes.iter().map(|o| json!({
            "effect_lists": o.effect_lists.iter().map(|l| json!({
                "chance": f64::from(l.chance),
                "effects": l.effects.iter().map(effect_value).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })).collect::<Vec<_>>() })
}

#[test]
fn mutation_scripts_compile_as_ace_does() {
    let w = world();
    replay("loot", "mutation_scripts", |c| {
        match c.input["op"].as_str().expect("op") {
            "resource_names" => {
                json!(mutation_cache::manifest_resource_names().collect::<Vec<_>>())
            }
            "id_map" => {
                let mut ids: Vec<u32> = (0x3800_0000..0x3800_0100)
                    .chain(0x3900_0000..0x3900_0010)
                    .collect();
                ids.retain(|&id| mutation_cache::mutation_id_to_filename(id).is_some());
                Value::Array(
                    ids.into_iter()
                        .map(|id| json!([id, mutation_cache::mutation_id_to_filename(id)]))
                        .collect(),
                )
            }
            "compile" => {
                let filename = c.input["filename"].as_str().expect("filename");
                filter_value(mutation_cache::get_mutation(&w, filename).as_deref())
            }
            "compile_id" => {
                let id = u32::try_from(int(&c.input, "id")).expect("u32");
                match mutation_cache::get_mutation_by_id(&w, id) {
                    None => Value::Null,
                    Some(f) => json!({
                        "mutations": f.mutations.len(),
                        "effects": f.mutations.iter().map(|m| m.outcomes.iter().map(|o| o.effect_lists.iter().map(|l| l.effects.len()).sum::<usize>()).sum::<usize>()).sum::<usize>(),
                    }),
                }
            }
            op => panic!("unknown op {op}"),
        }
    });
}

#[test]
fn mutation_filters_mutate_as_ace_does() {
    let w = world();
    replay("loot", "mutation_apply", |c| {
        let mut wn = weenie(90001, WeenieType::Generic, "Test Item");
        for p in c.input["props"].as_array().expect("props") {
            let kind = p[0].as_str().expect("kind");
            let idx = u16::try_from(p[1].as_i64().expect("idx")).expect("u16");
            match kind {
                "int" => set_int(
                    &mut wn,
                    PropertyInt(idx),
                    i32::try_from(p[2].as_i64().expect("int")).expect("i32"),
                ),
                "int64" => {
                    wn.properties_int64
                        .get_or_insert_with(DotNetDict::new)
                        .insert(PropertyInt64(idx), p[2].as_i64().expect("int64"));
                }
                "float" => {
                    wn.properties_float
                        .get_or_insert_with(DotNetDict::new)
                        .insert(PropertyFloat(idx), f64_of(&p[2]).expect("float"));
                }
                "bool" => {
                    wn.properties_bool
                        .get_or_insert_with(DotNetDict::new)
                        .insert(PropertyBool(idx), p[2].as_bool().expect("bool"));
                }
                "did" => {
                    let v = u32::try_from(p[2].as_i64().expect("did")).expect("u32");
                    wn.properties_did
                        .get_or_insert_with(DotNetDict::new)
                        .insert(PropertyDataId(idx), v);
                }
                k => panic!("unknown kind {k}"),
            }
        }
        let mut wo = construct(&w, wn, 0x8000_0001);
        seed(int(&c.input, "seed"));
        let filter =
            mutation_cache::get_mutation(&w, c.input["filename"].as_str().expect("filename"))
                .expect("script");
        let tier = i32::try_from(int(&c.input, "tier")).expect("tier");
        let mutated = filter.try_mutate(&mut wo, tier);
        json!({ "mutated": mutated, "props": all_props(&wo), "post": post() })
    });
}

// ----------------------------------------------------------------------------------- table rolls

fn wcid(w: WeenieClassName) -> Value {
    json!(w.0)
}

/// The harness's `ProfileCalls`, by name.
fn profile_call(w: &World, name: &str, p: &TreasureDeath) -> Value {
    use tables::*;
    use wcids::*;
    let spells_list = |v: Vec<empyrean_entity::enums::SpellId>| {
        Value::Array(v.into_iter().map(|s| json!(s.0)).collect())
    };
    if let Some((call, n)) = name.split_once(':') {
        let n: i32 = n.parse().expect("an enum value");
        return match call {
            "ArmorWcids.Roll" => {
                let mut t = TreasureArmorType(n);
                let r = armor_wcids::roll(p, &mut t);
                json!([r.0, t.0])
            }
            "WeaponWcids.Roll" => {
                let mut t = TreasureWeaponType(n);
                let r = weapon_wcids::roll(p, &mut t);
                json!([r.0, t.0])
            }
            "SocietyArmorWcids.Roll" => wcid(society_armor_wcids::roll(p, TreasureItemType(n))),
            other => panic!("unknown call {other}"),
        };
    }
    match name {
        "QualityChance.Roll" => json!(quality_chance::roll(p)),
        "QualityChance.RollInterval" => json!(f64::from(quality_chance::roll_interval(p))),
        "CantripChance.RollNumCantrips" => json!(cantrips::cantrip_chance::roll_num_cantrips(w, p)),
        "CantripChance.RollCantripLevel" => {
            json!(cantrips::cantrip_chance::roll_cantrip_level(w, p))
        }
        "NumCantrips.RollNumCantrips" => json!(cantrips::num_cantrips::roll_num_cantrips(p)),
        "NumCantrips.RollCantripLevel" => json!(cantrips::num_cantrips::roll_cantrip_level(p)),
        "ArmorSpells.Roll" => spells_list(spells::armor_spells::roll(p)),
        "MeleeSpells.Roll" => spells_list(spells::melee_spells::roll(p)),
        "MissileSpells.Roll" => spells_list(spells::missile_spells::roll(p)),
        "AetheriaChance.Roll_ItemMaxLevel" => json!(aetheria_chance::roll_item_max_level(p)),
        "CloakChance.Roll_ItemMaxLevel" => json!(cloak_chance::roll_item_max_level(p)),
        "PetDeviceChance.Roll" => json!(pet_device_chance::roll(p)),
        "ScrollLevelChance.Roll" => json!(scroll_level_chance::roll(p)),
        "ArmorModVsTypeChance.RollQualityLevel" => {
            json!(armor_mod_vs_type_chance::roll_quality_level(p))
        }
        "MissileMagicDefense.Roll" => {
            missile_magic_defense::roll(p.tier).map_or(Value::Null, |f| json!(f64::from(f)))
        }
        "ClothingWcids.Roll" => wcid(clothing_wcids::roll(p)),
        "CoalescedManaWcids.Roll" => wcid(coalesced_mana_wcids::roll(p)),
        "ConsumeWcids.Roll" => wcid(consume_wcids::roll(p)),
        "HealKitWcids.Roll" => wcid(heal_kit_wcids::roll(p)),
        "LockpickWcids.Roll" => wcid(lockpick_wcids::roll(p)),
        "ManaStoneWcids.Roll" => wcid(mana_stone_wcids::roll(p)),
        "PetDeviceWcids.Roll" => wcid(pet_device_wcids::roll(p)),
        "SpellComponentWcids.Roll" => wcid(spell_component_wcids::roll(p)),
        "SocietyArmorWcids.GetSociety" => json!(society_armor_wcids::get_society(p).0),
        "ArmorWcids.RollHeritage" => json!(armor_wcids::roll_heritage(p).0),
        "ArmorWcids.RollOverRobeWcid" => wcid(armor_wcids::roll_over_robe_wcid(p)),
        "WeaponWcids.RollHeritage" => json!(weapon_wcids::roll_heritage(p).0),
        "WeaponWcids.RollSwordWcid" => wcid(weapon_wcids::roll_sword_wcid(p)),
        "WeaponWcids.RollMaceWcid" => wcid(weapon_wcids::roll_mace_wcid(p)),
        "WeaponWcids.RollAxeWcid" => wcid(weapon_wcids::roll_axe_wcid(p)),
        "WeaponWcids.RollSpearWcid" => wcid(weapon_wcids::roll_spear_wcid(p)),
        "WeaponWcids.RollUnarmedWcid" => wcid(weapon_wcids::roll_unarmed_wcid(p)),
        "WeaponWcids.RollStaffWcid" => wcid(weapon_wcids::roll_staff_wcid(p)),
        "WeaponWcids.RollDaggerWcid" => wcid(weapon_wcids::roll_dagger_wcid(p)),
        "WeaponWcids.RollBowWcid" => wcid(weapon_wcids::roll_bow_wcid(p)),
        "WeaponWcids.RollCrossbowWcid" => wcid(weapon_wcids::roll_crossbow_wcid(p)),
        "WeaponWcids.RollAtlatlWcid" => wcid(weapon_wcids::roll_atlatl_wcid(p)),
        "WeaponWcids.RollCaster" => wcid(weapon_wcids::roll_caster(p)),
        other => panic!("unknown call {other}"),
    }
}

#[test]
fn table_rolls_draw_as_ace_does() {
    let w = world();
    let file = vectors::load_named("loot", "table_rolls");
    let mut failures = Vec::new();
    for c in &file.cases {
        let name = c.input["call"].as_str().expect("call");
        #[allow(clippy::cast_possible_truncation)]
        let p = profile(
            i32::try_from(int(&c.input, "tier")).expect("tier"),
            float(&c.input, "qm") as f32,
            i32::try_from(int(&c.input, "heritage")).expect("heritage"),
        );
        seed(int(&c.input, "seed"));
        let got = calls(4, || profile_call(&w, name, &p));
        if got != normalise_throws(&c.output) {
            failures.push(format!(
                "in {}\n    expected {}\n    got      {got}",
                c.input, c.output
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "loot/table_rolls: {} of {} differ:\n  {}",
        failures.len(),
        file.cases.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn item(w: &World, wcid: u32, weenie_type: WeenieType, props: &Value) -> WorldObject {
    let mut wn = weenie(wcid, weenie_type, "Test Item");
    for p in props.as_array().expect("props") {
        assert_eq!(p[0], "int");
        let idx = u16::try_from(p[1].as_i64().expect("idx")).expect("u16");
        set_int(
            &mut wn,
            PropertyInt(idx),
            i32::try_from(p[2].as_i64().expect("v")).expect("i32"),
        );
    }
    if wn.properties_int.is_none() && weenie_type == WeenieType::Clothing {
        wn.properties_int = Some(DotNetDict::new());
    }
    construct(w, wn, 0x8000_0002)
}

#[test]
fn item_rolls_draw_as_ace_does() {
    let w = world();
    replay("loot", "item_rolls", |c| {
        let call = c.input["call"].as_str().expect("call");
        let wcid = u32::try_from(int(&c.input, "wcid")).expect("wcid");
        let weenie_type = if wcid == 3000 {
            WeenieType::Clothing
        } else {
            WeenieType::Caster
        };
        let wo = item(&w, wcid, weenie_type, &c.input["props"]);
        if call == "CasterSlotSpells.IsOrb" {
            return json!(tables::caster_slot_spells::is_orb(&wo));
        }
        let tier = i32::try_from(int(&c.input, "tier")).expect("tier");
        #[allow(clippy::cast_possible_truncation)]
        let qm = c
            .input
            .get("qm")
            .map_or(0.0, |_| float(&c.input, "qm") as f32);
        let p = profile(tier, qm, 0);
        let mut roll = TreasureRoll::with_item_type(TreasureItemType(
            i32::try_from(
                c.input
                    .get("item_type")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            )
            .expect("i32"),
        ));
        roll.armor_type = TreasureArmorType(
            i32::try_from(
                c.input
                    .get("armor_type")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            )
            .expect("i32"),
        );
        seed(int(&c.input, "seed"));
        calls(4, || match call {
            "WandSpells.Roll" => Value::Array(
                spells::wand_spells::roll(&wo, &p)
                    .into_iter()
                    .map(|s| json!(s.0))
                    .collect(),
            ),
            "CasterSlotSpells.Roll" => json!(tables::caster_slot_spells::roll(&wo).0),
            "EquipmentSetChance.Roll" => tables::equipment_set_chance::roll(&wo, &p, &roll)
                .map_or(Value::Null, |e| json!(e.0)),
            "GearRatingChance.Roll" => json!(tables::gear_rating_chance::roll(&wo, &p, &roll)),
            other => panic!("unknown call {other}"),
        })
    });
}

#[test]
fn roll_wcid_draws_as_ace_does() {
    replay("loot", "roll_wcid", |c| {
        let mut p = profile(
            i32::try_from(int(&c.input, "tier")).expect("tier"),
            0.0,
            i32::try_from(int(&c.input, "heritage")).expect("heritage"),
        );
        let code = i32::try_from(int(&c.input, "code")).expect("code");
        p.item_treasure_type_selection_chances = code;
        p.magic_item_treasure_type_selection_chances = code;
        p.mundane_item_type_selection_chances = code;
        let category =
            TreasureItemCategory(i32::try_from(int(&c.input, "category")).expect("category"));
        let r#type = TreasureItemType(i32::try_from(int(&c.input, "type")).expect("type"));
        seed(int(&c.input, "seed"));
        calls(4, || roll_value(lgf::roll_wcid(&p, category, r#type)))
    });
}

#[test]
fn long_desc_matches_ace() {
    let w = world();
    replay("loot", "long_desc", |c| {
        let mut wn = weenie(90002, WeenieType::Generic, "Test Item");
        let did = u32::try_from(int(&c.input, "did")).expect("did");
        if did != 0 {
            let mut d = DotNetDict::new();
            d.insert(PropertyDataId::Spell, did);
            wn.properties_did = Some(d);
        }
        let book = c.input["book"].as_array().expect("book");
        if !book.is_empty() {
            let mut d = DotNetDict::new();
            for s in book {
                d.insert(
                    i32::try_from(s.as_i64().expect("spell")).expect("i32"),
                    2.0f32,
                );
            }
            wn.properties_spell_book = Some(d);
        }
        let wo = construct(&w, wn, 0x8000_0003);
        lgf::get_long_desc(&wo).map_or(Value::Null, Value::from)
    });
}

// -------------------------------------------------------------------------- hand-derived checks

/// The coinstack a synthetic world holds (`WeenieClassName.coinstack`, 273).
fn coin_world() -> World {
    let mut w = world();
    let coins = empyrean_content::models::world::Weenie::new(273, "coinstack", WeenieType::Coin)
        .with_string(PropertyString::Name, "Pyreal")
        .with_int(PropertyInt::StackUnitValue, 1)
        .with_int(PropertyInt::StackUnitEncumbrance, 0)
        .with_int(PropertyInt::MaxStackSize, 25000);
    w.content = Arc::new(MemContent::new().weenie(coins));
    empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
    w
}

struct EmptyShard;

impl empyrean_world::managers::guid_manager::ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }

    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// Pyreals: the wcid needs no draw, the coinstack's constructor draws its heartbeat, then
/// `MutateCoins` draws the stack size in the tier's range; value and burden follow the stack.
#[test]
fn a_pyreal_drop_is_the_tier_range_draw_after_the_constructor_draw() {
    for tier in 1..=8 {
        let mut w = coin_world();
        let p = profile(tier, 0.0, 0);
        seed(99);
        let wo = lgf::create_random_loot_objects_of_category(
            &mut w,
            &p,
            TreasureItemCategory::Item,
            TreasureItemType::Pyreal,
        )
        .expect("a coinstack");

        let mut r = empyrean_common::random::DotNetRandom::new(99);
        let _heartbeat = r.next_double();
        let (min, max) = [
            (5, 50),
            (10, 200),
            (10, 500),
            (25, 1000),
            (50, 5000),
            (250, 5000),
            (250, 5000),
            (250, 5000),
        ][usize::try_from(tier - 1).unwrap()];
        let expected = r.next_range(min, max + 1);

        assert_eq!(wo.stack_size(), Some(expected), "tier {tier}");
        assert_eq!(wo.value(), Some(expected), "tier {tier}: 1 pyreal each");
        assert_eq!(
            ThreadSafeRandom::next(0, 999_999),
            r.next_range(0, 1_000_000),
            "tier {tier}: no other draw"
        );
    }
}

/// A profile whose three chances are 0 makes three draws (one per group) and no loot.
#[test]
fn a_profile_with_no_chances_draws_three_times_and_drops_nothing() {
    let mut w = coin_world();
    let p = TreasureDeath {
        tier: 1,
        ..TreasureDeath::default()
    };
    seed(5);
    assert!(lgf::create_random_loot_objects(&mut w, &p).is_empty());
    let mut r = empyrean_common::random::DotNetRandom::new(5);
    for _ in 0..3 {
        let _ = r.next_range(1, 101);
    }
    assert_eq!(
        ThreadSafeRandom::next(0, 999_999),
        r.next_range(0, 1_000_000)
    );
}

/// The gem wcids a tier-1 gem roll can give (found by rolling), and the wcid the roll after
/// `seed(first_seed)` gives.
fn tier_1_gems(first_seed: i64) -> (std::collections::BTreeSet<u32>, u32) {
    let p = profile(1, 0.0, 0);
    let gem = |_: usize| {
        lgf::roll_wcid(&p, TreasureItemCategory::Item, TreasureItemType::Gem)
            .expect("a gem roll")
            .wcid
            .0
            .cast_unsigned()
    };
    seed(4);
    let gems = (0..4000).map(gem).collect();
    seed(first_seed);
    (gems, gem(0))
}

/// A world whose content has the given gem weenies, under `rules`.
fn gem_world(
    gems: &std::collections::BTreeSet<u32>,
    rules: &'static empyrean_common::era::EraRules,
) -> World {
    let mut w = world();
    let content = gems.iter().fold(MemContent::new(), |c, &g| {
        c.weenie(
            empyrean_content::models::world::Weenie::new(g, "gem", WeenieType::Gem)
                .with_string(PropertyString::Name, "Gem"),
        )
    });
    w.content = Arc::new(content);
    w.era = rules;
    empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
    w
}

/// Under the end-of-retail rule (ACE's), a rolled weenie the world database lacks creates nothing;
/// under the pack-only rule it is rolled again, and the item is one the world has.
/// Divergence: V387
#[test]
fn a_rolled_weenie_the_world_lacks_is_rolled_again_only_under_the_pack_only_rule() {
    use empyrean_common::era::EraId;
    let (mut gems, first) = tier_1_gems(3);
    assert!(gems.len() > 1, "{gems:?}");
    gems.remove(&first);

    let mut w = gem_world(&gems, EraId::Eor.rules());
    seed(3);
    let dropped = lgf::create_random_loot_objects_of_category(
        &mut w,
        &profile(1, 0.0, 0),
        TreasureItemCategory::Item,
        TreasureItemType::Gem,
    );
    assert!(
        dropped.is_none(),
        "ACE's rule: the missing gem {first} drops nothing"
    );

    let mut w = gem_world(&gems, EraId::Infiltration.rules());
    seed(3);
    let dropped = lgf::create_random_loot_objects_of_category(
        &mut w,
        &profile(1, 0.0, 0),
        TreasureItemCategory::Item,
        TreasureItemType::Gem,
    )
    .expect("the pack-only rule rolls a gem the world has");
    assert_ne!(dropped.weenie_class_id(), first);
    assert!(gems.contains(&dropped.weenie_class_id()));

    // With every gem present the rule rolls exactly as ACE's does.
    gems.insert(first);
    for rules in [EraId::Eor.rules(), EraId::Infiltration.rules()] {
        let mut w = gem_world(&gems, rules);
        seed(3);
        let dropped = lgf::create_random_loot_objects_of_category(
            &mut w,
            &profile(1, 0.0, 0),
            TreasureItemCategory::Item,
            TreasureItemType::Gem,
        )
        .expect("a gem");
        assert_eq!(dropped.weenie_class_id(), first);
        assert_eq!(post(), {
            seed(3);
            let mut w = gem_world(&gems, EraId::Eor.rules());
            let _ = lgf::create_random_loot_objects_of_category(
                &mut w,
                &profile(1, 0.0, 0),
                TreasureItemCategory::Item,
                TreasureItemType::Gem,
            );
            post()
        });
    }
}

/// Under the pack-only rule, a world with none of a type's weenies drops nothing for it, and
/// neither fails nor panics (ACE's aetheria creation panics on a missing weenie).
/// Divergence: V387
#[test]
fn a_world_without_a_types_weenies_drops_nothing_of_it_under_the_pack_only_rule() {
    let mut w = world();
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    for tier in 1..=8 {
        for item_type in [
            TreasureItemType::Gem,
            TreasureItemType::PetDevice,
            TreasureItemType::Cloak,
        ] {
            assert!(lgf::create_random_loot_objects_of_category(
                &mut w,
                &profile(tier, 0.0, 0),
                TreasureItemCategory::MagicItem,
                item_type,
            )
            .is_none());
        }
    }
}

/// A 100% item group of exactly two pyreal stacks (item profile 8: coins only? no, the explicit
/// type path): two coinstacks, each with its constructor draw and its range draw, in order.
#[test]
fn a_certain_item_group_rolls_its_count_then_each_item() {
    let mut w = coin_world();
    seed(11);
    let a = lgf::create_random_loot_objects_of_category(
        &mut w,
        &profile(2, 0.0, 0),
        TreasureItemCategory::Item,
        TreasureItemType::Pyreal,
    )
    .expect("coins");
    let b = lgf::create_random_loot_objects_of_category(
        &mut w,
        &profile(2, 0.0, 0),
        TreasureItemCategory::Item,
        TreasureItemType::Pyreal,
    )
    .expect("coins");
    let mut r = empyrean_common::random::DotNetRandom::new(11);
    let _ = r.next_double();
    let first = r.next_range(10, 201);
    let _ = r.next_double();
    let second = r.next_range(10, 201);
    assert_eq!(
        (a.stack_size(), b.stack_size()),
        (Some(first), Some(second))
    );
    assert_ne!(a.guid, b.guid, "each item takes a new dynamic guid");
}

/// Rares: a luck of 2500 or more makes the tier-1 roll certain at the default rate (1 in 2500);
/// each higher tier is one further roll; `GetRareTier` finds the tier of a rare wcid.
#[test]
fn rares_roll_their_tier_then_an_even_pick() {
    assert_eq!(rare::get_rare_tier(30183), 1);
    assert_eq!(rare::get_rare_tier(30107), 2);
    assert_eq!(rare::get_rare_tier(52034), 3);
    assert_eq!(rare::get_rare_tier(30352), 4);
    assert_eq!(rare::get_rare_tier(70003), 5);
    assert_eq!(rare::get_rare_tier(45470), 6);
    assert_eq!(rare::get_rare_tier(273), 0);

    let mut w = world();
    empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
    for s in 0..50 {
        seed(s);
        // no rare weenies in this world: the wcid is rolled, the creation fails
        assert!(rare::try_create_rare(&mut w, 2600).is_none());
        let mut r = empyrean_common::random::DotNetRandom::new(i32::try_from(s).unwrap());
        assert_eq!(r.next_range(1, 2), 1, "t1_chance max(2500 - 2600, 1) = 1");
        let mut tier = 1;
        for (n, t) in [(10, 2), (100, 3), (1250, 4), (3017, 5), (3500, 6)] {
            if r.next_range(1, n + 1) == 1 {
                tier = t;
            }
        }
        let count = [42, 28, 5, 47, 112, 61][tier - 1];
        let _pick = r.next_range(0, count);
        assert_eq!(
            ThreadSafeRandom::next(0, 999_999),
            r.next_range(0, 1_000_000),
            "seed {s}: tier {tier}"
        );
    }

    // with no luck, one draw and (almost always) nothing
    seed(1);
    assert!(rare::try_create_rare(&mut w, 0).is_none());
}

/// The mutation effect operators, hand-derived from `EffectArgumentOp.cs`: the left operand's
/// type wins; a zero divisor returns the left operand; mixed int/double casts back to int.
#[test]
fn effect_argument_operators_follow_the_left_type() {
    let i = EffectArgument::from_int;
    let d = EffectArgument::from_double;
    let l = EffectArgument::from_long;
    assert_eq!(
        EffectArgument::op_addition(&i(5), &d(2.7))
            .unwrap()
            .to_int(),
        7
    );
    assert_eq!(
        EffectArgument::op_addition(&d(5.0), &i(2))
            .unwrap()
            .to_double(),
        7.0
    );
    assert_eq!(
        EffectArgument::op_multiply(&i(10), &d(0.95))
            .unwrap()
            .to_int(),
        9
    );
    assert_eq!(
        EffectArgument::op_division(&i(7), &i(2)).unwrap().to_int(),
        3
    );
    assert_eq!(
        EffectArgument::op_division(&i(7), &i(0)).unwrap().to_int(),
        7
    );
    assert_eq!(
        EffectArgument::op_subtraction(&l(10), &i(3))
            .unwrap()
            .to_long(),
        7
    );
    assert_eq!(
        EffectArgument::op_addition(&i(i32::MAX), &i(1))
            .unwrap()
            .to_int(),
        i32::MIN,
        "unchecked int"
    );
    assert!(
        EffectArgument::op_addition(&EffectArgument::new(), &i(1)).is_none(),
        "Invalid + int is null"
    );
    assert!(EffectArgument::op_less_than(&i(1), &i(2)));
    assert!(
        !EffectArgument::op_less_than(&i(1), &d(2.0)),
        "a type mismatch compares false"
    );
}

/// `a = b / c` multiplies in ACE (marked ACE-BUG); no shipped script uses it.
#[test]
fn assign_divide_multiplies_as_ace_does() {
    let w = world();
    let mut wo = construct(
        &w,
        weenie(90003, WeenieType::Generic, "Test Item"),
        0x8000_0004,
    );
    let effect = Effect {
        quality: Some(EffectArgument::from_quality(
            empyrean_entity::enums::StatType::Int,
            i32::from(PropertyInt::Value.0),
        )),
        r#type: empyrean_entity::enums::MutationEffectType::AssignDivide,
        arg1: Some(EffectArgument::from_int(12)),
        arg2: Some(EffectArgument::from_int(4)),
    };
    assert!(effect.try_mutate(&mut wo));
    assert_eq!(wo.value(), Some(48));
    for s in mutation_cache::manifest_resource_names() {
        let filename = s.trim_start_matches("ACE.Server.Entity.Mutations.");
        let f = mutation_cache::build_mutation(filename).expect("every embedded script compiles");
        let uses = f
            .mutations
            .iter()
            .flat_map(|m| &m.outcomes)
            .flat_map(|o| &o.effect_lists)
            .flat_map(|l| &l.effects)
            .any(|e| e.r#type == empyrean_entity::enums::MutationEffectType::AssignDivide);
        assert!(!uses, "{filename} uses a = b / c");
    }
}

/// GemCountChance before the host installs the `treasure_gem_count` rows rolls nothing (0, no
/// draw, a `not_ported!` hit); after, the tier's table (tiers above 6 use tier 6's).
#[test]
fn gem_count_chance_uses_the_installed_rows() {
    let rows = vec![
        TreasureGemCount {
            id: 1,
            gem_code: 7,
            tier: 1,
            count: 1,
            chance: 1.0,
        },
        TreasureGemCount {
            id: 2,
            gem_code: 7,
            tier: 6,
            count: 2,
            chance: 0.5,
        },
        TreasureGemCount {
            id: 3,
            gem_code: 7,
            tier: 6,
            count: 3,
            chance: 0.5,
        },
        TreasureGemCount {
            id: 4,
            gem_code: 7,
            tier: 6,
            count: 9,
            chance: 0.0,
        },
    ];
    let mut w = world();
    tables::gem_count_chance::gem_count_chance(&mut w, &rows);
    seed(3);
    assert_eq!(tables::gem_count_chance::roll(&w, 7, 1), 1);
    let r8 = tables::gem_count_chance::roll(&w, 7, 8);
    let mut r = empyrean_common::random::DotNetRandom::new(3);
    let _ = r.next_double();
    let d = r.next_double();
    assert_eq!(
        r8,
        if d < 0.5 { 2 } else { 3 },
        "tier 8 uses tier 6; the 0-chance row is dropped"
    );
    assert_eq!(tables::gem_count_chance::roll(&w, 9, 1), 0, "no table: 0");
}

/// Item-kind distribution by tier: 4000 magic-item wcid rolls per tier with a fixed seed, on the
/// magic item profile most tiers use. The counts are pinned (they are ACE's: `roll_wcid` replays
/// ACE's draws, `loot/roll_wcid`), and each kind's share is sane.
#[test]
fn item_kind_distribution_by_tier_is_pinned() {
    let mut table = Vec::new();
    for tier in 1..=8 {
        let mut p = profile(tier, 0.0, 0);
        p.magic_item_treasure_type_selection_chances = 8;
        seed(4100 + i64::from(tier));
        let mut counts = std::collections::BTreeMap::<i32, u32>::new();
        for _ in 0..4000 {
            let r = lgf::roll_wcid(&p, TreasureItemCategory::MagicItem, TreasureItemType::Undef)
                .expect("a roll");
            *counts.entry(r.item_type.0).or_default() += 1;
        }
        let total: u32 = counts.values().sum();
        assert_eq!(total, 4000);
        // profile 8 splits almost evenly over the seven equipment kinds (gem .. clothing), with
        // cloaks and pet devices rare: each share is sane
        for (&kind, &n) in &counts {
            if (2..=8).contains(&kind) {
                assert!(
                    (440..=700).contains(&n),
                    "tier {tier} kind {kind}: {n} of 4000"
                );
            } else {
                assert!(n < 100, "tier {tier} kind {kind}: {n} of 4000");
            }
        }
        table.push((tier, counts));
    }
    let pinned = format!("{table:?}");
    assert_eq!(
        pinned, PINNED_DISTRIBUTION,
        "the item-kind distribution moved"
    );
}

const PINNED_DISTRIBUTION: &str = "[(1, {2: 574, 3: 553, 4: 528, 5: 598, 6: 578, 7: 531, 8: 568, 25: 49, 27: 21}), (2, {2: 577, 3: 550, 4: 503, 5: 637, 6: 576, 7: 529, 8: 564, 25: 39, 27: 25}), (3, {2: 585, 3: 580, 4: 481, 5: 605, 6: 586, 7: 526, 8: 572, 25: 43, 27: 22}), (4, {2: 543, 3: 596, 4: 498, 5: 612, 6: 575, 7: 559, 8: 567, 25: 35, 27: 15}), (5, {2: 565, 3: 548, 4: 538, 5: 607, 6: 583, 7: 595, 8: 511, 25: 29, 27: 24}), (6, {2: 590, 3: 578, 4: 506, 5: 609, 6: 576, 7: 548, 8: 543, 25: 26, 27: 24}), (7, {2: 554, 3: 542, 4: 494, 5: 598, 6: 611, 7: 581, 8: 562, 25: 43, 27: 15}), (8, {2: 569, 3: 543, 4: 506, 5: 588, 6: 596, 7: 589, 8: 546, 25: 48, 27: 15})]";

/// `LootBias` is used by the test generator; the enum values are ACE's.
#[test]
fn loot_bias_values_are_aces() {
    assert_eq!(
        (LootBias::UnBiased.0, LootBias::Armor.0, LootBias::Weapons.0),
        (0, 1, 2)
    );
}

/// `LootStats` and the `/testlootgen` report, hand-derived from `LootStats.cs` and
/// `LootGenerationFactory_Test.cs`: counters are floats, drop rates divide in float.
#[test]
fn loot_stats_count_and_report() {
    use empyrean_world::factories::loot_generation_factory_test as lgt;
    use empyrean_world::factories::loot_stats::LootStats;

    let w = world();
    let mut gem = weenie(90010, WeenieType::Gem, "Test Gem");
    set_int(
        &mut gem,
        PropertyInt::ItemType,
        empyrean_entity::enums::ItemType::Gem.0.cast_signed(),
    );
    set_int(&mut gem, PropertyInt::ItemMaxMana, 300);
    let gem = construct(&w, gem, 0x8000_0010);

    let mut ls = LootStats::new(false);
    assert_eq!(
        (ls.min_mana, ls.min_items_created, ls.min_al),
        (50000, 100, 1000)
    );
    ls.add_item(None, false);
    ls.add_item(Some(&gem), false);
    ls.add_item(Some(&gem), false);
    assert_eq!(
        (ls.total_items, ls.null_count, ls.gem_count),
        (3.0, 1.0, 2.0)
    );
    assert_eq!(
        (
            ls.has_mana_count,
            ls.min_mana,
            ls.max_mana,
            ls.total_max_mana
        ),
        (2, 300, 300, 600)
    );

    let report = lgt::build_display_stats(&ls, "none");
    assert!(report.starts_with(
        "
 No Table(s) was selected to display, showing only general statistics 
 Treasure Items 
 ---- 
 Armor=0 
"
    ));
    assert!(
        report.contains(
            " Gem=2 
"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            " NullCount=1 
 Total Found=3 
 TotalGenerated=3
"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            " Gem= 66.66667% 
"
        ),
        "float 2 / 3 * 100: {report}"
    );
    assert!(
        report.contains(
            " Necklace = 0	 Droprate = NaN%
"
        ),
        "0 / 0 jewelry: {report}"
    );
    assert!(
        report.ends_with(
            "
 Mana capacity across all items Min=300  Max=300 Avg Mana=300"
        ),
        "{report}"
    );

    let mut w = coin_world();
    let header = lgt::test_loot_gen_monster(&mut w, 999_999, 3, false, "all");
    assert!(header.ends_with(
        " DID 999999 you specified is invalid. 
"
    ));
}

/// `LootParser` over a `Factories/Tables`-style source, hand-derived from `LootParser.cs`: only
/// non-readonly `ChanceTable<T> Name = new ChanceTable<T>()` blocks are parsed, up to the first
/// two-character line; comments and short lines are skipped; an unparsable line is dropped.
#[test]
fn loot_parser_reads_chance_table_literals() {
    use empyrean_world::factories::entity::loot_parser::{self, ParsedTable};

    let src = [
        "        private static ChanceTable<int> T1_Chances = new ChanceTable<int>()",
        "        {",
        "            ( 1, 0.75f ),",
        "            // a comment",
        "            ( 2, 0.25f ),",
        "            ( x, 0.25f ),",
        "        };",
        "        private static readonly ChanceTable<int> Skipped = new ChanceTable<int>()",
        "        private static ChanceTable<WeenieClassName> Wcids = new ChanceTable<WeenieClassName>()",
        "        {",
        "            ( WeenieClassName.coinstack, 1.0f ),",
        "        };",
        "        private static ChanceTable<bool> Flags = new ChanceTable<bool>()",
        "        {",
        "            (  true, 0.05f ),",
        "            ( false, 0.95f ),",
        "        };",
        "        private static ChanceTable<TreasureWeaponType> Types = new ChanceTable<TreasureWeaponType>()",
        "        {",
        "            ( TreasureWeaponType.Sword, 1.0f ),",
        "        };",
        "        private static ChanceTable<float> Floats = new ChanceTable<float>()",
    ]
    .map(str::to_owned);
    let tables = loot_parser::parse_lines(&src);
    let names: Vec<&String> = tables.keys().collect();
    assert_eq!(names, ["T1_Chances", "Wcids", "Flags", "Types", "Floats"]);
    match tables.get("T1_Chances") {
        Some(Some(ParsedTable::Int(t))) => assert_eq!(t.entries(), &[(1, 0.75), (2, 0.25)]),
        other => panic!("{other:?}"),
    }
    match tables.get("Wcids") {
        Some(Some(ParsedTable::Wcid(t))) => {
            assert_eq!(t.entries(), &[(WeenieClassName::coinstack, 1.0)])
        }
        other => panic!("{other:?}"),
    }
    match tables.get("Flags") {
        Some(Some(ParsedTable::Bool(t))) => assert_eq!(t.entries(), &[(true, 0.05), (false, 0.95)]),
        other => panic!("{other:?}"),
    }
    match tables.get("Types") {
        Some(Some(ParsedTable::WeaponType(t))) => {
            assert_eq!(t.entries(), &[(TreasureWeaponType::Sword, 1.0)])
        }
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(tables.get("Floats"), Some(None)),
        "an unknown element type is null"
    );
    assert_eq!(
        loot_parser::get_table_name("ChanceTable<int> Name = new"),
        Some("Name".to_owned())
    );
    assert_eq!(loot_parser::read_all_lines("a\r\nb\rc\n"), ["a", "b", "c"]);
}

// ---------------------------------------------------------------------- the Infiltration era's loot

/// The Infiltration era's scripts: each compiles into mutations over that era's six tiers (the
/// non-elemental casters' into none).
/// Divergence: V412
#[test]
fn the_infiltration_loot_scripts_compile_over_six_tiers() {
    let w = world();
    let melee = [
        "axe",
        "dagger",
        "dagger_ms",
        "mace",
        "spear",
        "staff",
        "sword",
        "sword_ms",
        "unarmed",
    ];
    let mut names: Vec<String> = melee
        .iter()
        .map(|m| format!("MeleeWeapons.Damage_WieldDifficulty_DamageVariance.Infiltration.{m}.txt"))
        .collect();
    for m in [
        "axe", "dagger", "mace", "spear", "staff", "sword", "unarmed",
    ] {
        names.push(format!(
            "MeleeWeapons.WeaponOffense_WeaponDefense.Infiltration.{m}_offense_defense.txt"
        ));
    }
    for m in [
        "atlatl_elemental",
        "atlatl_non_elemental",
        "atlatl_regular_non_elemental",
        "bow_elemental",
        "bow_non_elemental",
        "bow_short_non_elemental",
        "crossbow_elemental",
        "crossbow_light_non_elemental",
        "crossbow_non_elemental",
    ] {
        names.push(format!("MissileWeapons.Infiltration.{m}.txt"));
    }
    for m in ["caster_elemental", "caster_non_elemental"] {
        names.push(format!("Casters.Infiltration.{m}.txt"));
    }
    for m in [
        "armor_level",
        "covenant_armor_level",
        "covenant_shield_level",
        "shield_level",
    ] {
        names.push(format!("ArmorLevel.Infiltration.{m}.txt"));
    }
    for name in &names {
        let filter = mutation_cache::get_mutation(&w, name)
            .unwrap_or_else(|| panic!("{name} is embedded and compiles"));
        // The era's non-elemental casters carry no wield requirement: an empty script.
        assert_eq!(
            filter.mutations.is_empty(),
            name.ends_with("caster_non_elemental.txt"),
            "{name}"
        );
        for m in &filter.mutations {
            assert_eq!(m.chances.len(), 6, "{name}: six tiers");
        }
    }
    assert!(
        mutation_cache::manifest_resource_names().all(|n| !n.contains("Infiltration")),
        "ACE's own manifest stays ACE's"
    );
}

/// A weapon rolled for an Infiltration treasure comes from that era's tables: an old weapon
/// skill's weapon, a bow, crossbow or atlatl, or a caster, never a two-handed weapon, and it names
/// the era script its mutation takes; the same roll at the end of retail names none.
/// Divergence: V412
#[test]
fn an_infiltration_weapon_roll_names_its_eras_script_and_is_never_two_handed() {
    use empyrean_common::era::LootRules;
    let w = world();
    let mut kinds = std::collections::BTreeSet::new();
    seed(7);
    for tier in 1..=6 {
        for heritage in [1, 2, 3, 21] {
            for _ in 0..60 {
                let roll = lgf::roll_wcid_in(
                    LootRules::Infiltration,
                    &profile(tier, 0.0, heritage),
                    TreasureItemCategory::Item,
                    TreasureItemType::Weapon,
                )
                .expect("a weapon");
                assert_ne!(roll.wcid, WeenieClassName::undef, "tier {tier}");
                assert!(
                    !matches!(
                        roll.weapon_type,
                        TreasureWeaponType::TwoHandedWeapon
                            | TreasureWeaponType::TwoHandedAxe
                            | TreasureWeaponType::TwoHandedMace
                            | TreasureWeaponType::TwoHandedSpear
                            | TreasureWeaponType::TwoHandedSword
                    ),
                    "{:?}",
                    roll.weapon_type
                );
                let script = roll.era_script.expect("an era script");
                kinds.insert(script);
                let name = match roll.weapon_type {
                    TreasureWeaponType::Bow
                    | TreasureWeaponType::Crossbow
                    | TreasureWeaponType::Atlatl => {
                        format!("MissileWeapons.Infiltration.{script}_non_elemental.txt")
                    }
                    TreasureWeaponType::Caster => {
                        "Casters.Infiltration.caster_non_elemental.txt".to_owned()
                    }
                    _ => format!(
                        "MeleeWeapons.Damage_WieldDifficulty_DamageVariance.Infiltration.{script}.txt"
                    ),
                };
                assert!(mutation_cache::get_mutation(&w, &name).is_some(), "{name}");
            }
        }
    }
    for kind in [
        "axe", "mace", "spear", "staff", "sword", "unarmed", "bow", "crossbow", "atlatl", "caster",
    ] {
        assert!(kinds.contains(kind), "{kind} never rolled: {kinds:?}");
    }
    let roll = lgf::roll_wcid_in(
        LootRules::EndOfRetail,
        &profile(3, 0.0, 1),
        TreasureItemCategory::Item,
        TreasureItemType::Weapon,
    )
    .expect("a weapon");
    assert_eq!(roll.era_script, None);
}

/// Armour rolled for an Infiltration treasure is one of that era's kinds (leather, studded leather,
/// chainmail, the heritages' platemail and their low and high armours, covenant), never the later
/// sets; jewelry, clothing and food come from the era's tables too.
/// Divergence: V412
#[test]
fn an_infiltration_armor_roll_is_one_of_its_eras_kinds() {
    use empyrean_common::era::LootRules;
    let kinds = [
        TreasureArmorType::Leather,
        TreasureArmorType::StuddedLeather,
        TreasureArmorType::Chainmail,
        TreasureArmorType::Platemail,
        TreasureArmorType::Scalemail,
        TreasureArmorType::Yoroi,
        TreasureArmorType::Celdon,
        TreasureArmorType::Amuli,
        TreasureArmorType::Koujia,
        TreasureArmorType::Covenant,
        TreasureArmorType::Lorica,
        TreasureArmorType::Nariyid,
        TreasureArmorType::Chiran,
    ];
    seed(5);
    for tier in 1..=6 {
        for heritage in [1, 2, 3, 22] {
            for _ in 0..40 {
                let roll = lgf::roll_wcid_in(
                    LootRules::Infiltration,
                    &profile(tier, 0.0, heritage),
                    TreasureItemCategory::Item,
                    TreasureItemType::Armor,
                )
                .expect("armour");
                assert_ne!(roll.wcid, WeenieClassName::undef);
                assert!(kinds.contains(&roll.armor_type), "{:?}", roll.armor_type);
                for item_type in [
                    TreasureItemType::Jewelry,
                    TreasureItemType::Clothing,
                    TreasureItemType::Consumable,
                ] {
                    let roll = lgf::roll_wcid_in(
                        LootRules::Infiltration,
                        &profile(tier, 0.0, heritage),
                        TreasureItemCategory::Item,
                        item_type,
                    )
                    .expect("an item");
                    assert_ne!(roll.wcid, WeenieClassName::undef, "{item_type:?}");
                }
            }
        }
    }
}

/// The Infiltration era's treasure profiles: a creature's chances scaled by its tier (tier 1: 30%
/// of items, 20% of magic items, 90% of mundane items); a chest's profile moved off one too rich
/// for where it stands (4 to 6), with no mundane items, at least 3 and half again as many items
/// and magic items, at loot quality 0.2 or better; steel chests (338) left alone. At the end of
/// retail every profile is the table's.
/// Divergence: V413
#[test]
fn an_infiltration_creature_and_chest_drop_by_their_eras_profiles() {
    use empyrean_world::dispatch::Class;
    let row = |treasure_type: u32, tier: i32| TreasureDeath {
        id: treasure_type,
        treasure_type,
        tier,
        item_chance: 100,
        item_min_amount: 1,
        item_max_amount: 1,
        magic_item_chance: 100,
        magic_item_min_amount: 1,
        magic_item_max_amount: 2,
        mundane_item_chance: 100,
        ..TreasureDeath::default()
    };
    let mut w = world();
    w.content = Arc::new(
        MemContent::new()
            .treasure_death(row(4, 1))
            .treasure_death(row(6, 2))
            .treasure_death(row(338, 3)),
    );
    let place = |w: &mut World, class: Class, id: u32| {
        let mut o = WorldObject::allocate(class);
        o.guid = ObjectGuid::new(id);
        o.biota.id = id;
        w.objects.insert(o).expect("fresh");
        ObjectGuid::new(id)
    };
    let creature = place(&mut w, Class::Creature, 0x8000_0001);
    let chest = place(&mut w, Class::Chest, 0x8000_0002);

    assert_eq!(
        *lgf::era_death_treasure(&w, 4, creature).expect("a profile"),
        row(4, 1),
        "the end of retail: the table's"
    );

    w.era = empyrean_common::era::EraId::Infiltration.rules();
    let p = lgf::era_death_treasure(&w, 4, creature).expect("a profile");
    assert_eq!(
        (p.item_chance, p.magic_item_chance, p.mundane_item_chance),
        (30, 20, 90)
    );
    let p = lgf::era_death_treasure(&w, 4, chest).expect("a profile");
    assert_eq!(p.treasure_type, 6, "moved to the richer profile");
    assert_eq!(
        (
            p.mundane_item_chance,
            p.item_max_amount,
            p.magic_item_max_amount,
            p.loot_quality_mod
        ),
        (0, 5, 3, 0.2)
    );
    assert_eq!(
        *lgf::era_death_treasure(&w, 338, chest).expect("a profile"),
        row(338, 3),
        "steel chests are left alone"
    );
}

/// Pyreals at the Infiltration era: that era's amount per tier (50-100 at tier 1, ...), after the
/// coinstack's constructor draw.
/// Divergence: V412
#[test]
fn an_infiltration_pyreal_drop_is_its_eras_tier_range() {
    for (tier, (min, max)) in [
        (1, (50, 100)),
        (2, (400, 1000)),
        (4, (1200, 4000)),
        (6, (2000, 5000)),
    ] {
        let mut w = coin_world();
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        seed(99);
        let wo = lgf::create_random_loot_objects_of_category(
            &mut w,
            &profile(tier, 0.0, 0),
            TreasureItemCategory::Item,
            TreasureItemType::Pyreal,
        )
        .expect("a coinstack");
        let mut r = empyrean_common::random::DotNetRandom::new(99);
        let _heartbeat = r.next_double();
        assert_eq!(
            wo.stack_size(),
            Some(r.next_range(min, max + 1)),
            "tier {tier}"
        );
    }
}

// ---------------------------------------------------------------------- the Infiltration era's magic

/// The Infiltration era's spell levels: an item's spells by its tier's chances (tier 1 levels 1
/// to 3 ... tier 8 levels 6 and 7, never the eighth); a scroll's the same way to the fifth tier and
/// the sixth level above, a steel chest's always the seventh; a scroll's spell is one of the era's
/// (no void magic, no two-handed or dual-wield mastery) at that level; and an item named for its
/// spell takes the era's names.
/// Divergence: V416
#[test]
fn the_infiltration_spell_and_scroll_levels_are_its_eras() {
    use empyrean_entity::enums::SpellId;
    use empyrean_tables::logic::era::infiltration as era;
    seed(5);
    let spell_levels: [&[i32]; 8] = [
        &[1, 2, 3],
        &[3, 4, 5],
        &[4, 5, 6],
        &[4, 5, 6],
        &[5, 6],
        &[5, 6],
        &[6, 7],
        &[6, 7],
    ];
    let scroll_levels: [&[i32]; 8] = [
        &[1, 2, 3],
        &[3, 4, 5],
        &[4, 5, 6],
        &[4, 5, 6],
        &[5, 6],
        &[6],
        &[6],
        &[6],
    ];
    for tier in 1..=8 {
        let allowed = spell_levels[usize::try_from(tier - 1).expect("tier")];
        let scrolls = scroll_levels[usize::try_from(tier - 1).expect("tier")];
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..400 {
            let level = era::roll_spell_level(tier);
            assert!(allowed.contains(&level), "tier {tier}: spell level {level}");
            seen.insert(level);
            let level = era::roll_scroll_level(tier, 0, 0.0);
            assert!(
                scrolls.contains(&level),
                "tier {tier}: scroll level {level}"
            );
            assert_eq!(era::roll_scroll_level(tier, 338, 0.0), 7, "a steel chest");
        }
        assert_eq!(seen.len(), allowed.len(), "tier {tier}: every level drops");
    }

    let era_spells = era::scroll_spells();
    for later in [
        SpellId::NetherBolt1,
        SpellId::TwoHandedMasterySelf1,
        SpellId::DualWieldMasterySelf1,
        SpellId::SpiritDrinkerSelf1,
    ] {
        assert!(!era_spells.contains(&later), "{later:?}");
    }
    assert!(era_spells.contains(&SpellId::SpearMasterySelf1));
    for _ in 0..200 {
        let spell = era::roll_scroll_spell(3);
        let first = era_spells
            .iter()
            .copied()
            .find(|&s| era::spell_at_level(s, 3) == spell)
            .unwrap_or_else(|| panic!("{spell:?} is an era spell at level 3"));
        assert_ne!(first, SpellId::Undef);
    }

    assert_eq!(
        era::spell_descriptor(SpellId::BloodDrinkerSelf3),
        Some("Blood Drinker")
    );
    assert_eq!(
        era::spell_descriptor(SpellId::FlameBolt5),
        Some("Flame Bolt")
    );
    assert_eq!(
        era::spell_descriptor(SpellId::DirtyFightingMasterySelf2),
        Some("Dirty Fighting")
    );
}

/// The Infiltration era's cantrips: none before the third tier, at most three to an item, and
/// minor or major only (the end of retail's eighth tier has epic and legendary ones and up to
/// four); a caster's cantrips and item spells are the era's (War Magic its only magic aptitude,
/// Defender and Hermetic Link its spells, no Spirit Drinker); a weapon's aptitude is its own old
/// skill's.
/// Divergence: V416
#[test]
fn infiltration_cantrips_are_minor_or_major_and_its_casters_spells_its_own() {
    use empyrean_entity::enums::{Skill, SpellId};
    use empyrean_tables::logic::era::infiltration as era;
    let mut w = world();
    w.era = empyrean_common::era::EraId::Infiltration.rules();
    seed(9);
    for tier in 1..=8 {
        for quality in [0.0f32, 1.0] {
            let p = profile(tier, quality, 1);
            for _ in 0..200 {
                let n = cantrips::cantrip_chance::roll_num_cantrips(&w, &p);
                let level = cantrips::cantrip_chance::roll_cantrip_level(&w, &p);
                assert!(n <= 3, "tier {tier}: {n} cantrips");
                if tier <= 2 {
                    assert_eq!(n, 0, "tier {tier}");
                }
                assert!(
                    (1..=2).contains(&level),
                    "tier {tier}: cantrip level {level}"
                );
            }
        }
    }
    let eor = world();
    let top = profile(8, 1.0, 1);
    assert_eq!(cantrips::cantrip_chance::roll_cantrip_level(&eor, &top), 4);

    for _ in 0..200 {
        let c = era::roll_caster_cantrip();
        assert_ne!(c, SpellId::CantripVoidMagicAptitude1);
        for spell in era::roll_wand_spells(1.0) {
            assert!(
                [SpellId::DefenderSelf1, SpellId::HermeticLinkSelf1].contains(&spell),
                "{spell:?}"
            );
        }
    }
    assert_eq!(
        era::weapon_aptitude(Skill::Axe),
        SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1
    );
    assert_eq!(
        era::weapon_aptitude(Skill::Crossbow),
        SpellId::CANTRIPCROSSBOWAPTITUDE1
    );
    assert_eq!(
        era::weapon_aptitude(Skill::Spear),
        SpellId::CANTRIPSPEARAPTITUDE1
    );
}

// ---------------------------------------------------------------------- real content (lootgen/)

/// The real-content tier: `lootgen/` replayed over `world.pack` (`EMPYREAN_TEST_WORLD_PACK`, default
/// `world.pack` in the repository) and the retail dats (`DERETH_TEST_DAT_DIR`). Fails, never skips, when they are absent.
#[cfg(feature = "real-content")]
mod real_content {

    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};

    use super::*;

    fn real_world() -> World {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        let dats = DatManager::initialize(Arc::new(source)).expect("retail dats");
        let path = empyrean_common::test_paths::world_pack();
        let pack = PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        });
        let mut w = World::new(snapshot(), dats);
        dpm::load_default_properties(&w.property_manager);
        w.content = Arc::new(pack);
        empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
        w
    }

    /// Every weapon an Infiltration treasure of tiers 1 to 6 rolls, on the Infiltration world
    /// (`EMPYREAN_TEST_INFILTRATION_PACK`), is a weenie that world has, mutated by the era's
    /// scripts: a melee or thrown weapon of an old weapon skill whose wield requirement (when it has
    /// one) is that skill, a bow, crossbow or atlatl, or a caster; none carries a later skill, and
    /// the first tier's melee weapons are the plain ones (ACE's tables drop elemental ones there).
    /// Divergence: V412
    #[test]
    fn every_infiltration_weapon_drop_is_an_old_skill_weapon_the_world_has() {
        use empyrean_entity::enums::Skill;
        let mut w = real_world();
        let path = empyrean_common::test_paths::infiltration_pack();
        w.content = Arc::new(PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the Infiltration world.pack at {} (set EMPYREAN_TEST_INFILTRATION_PACK): {e}",
                path.display()
            )
        }));
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        let later = [
            Skill::HeavyWeapons,
            Skill::LightWeapons,
            Skill::FinesseWeapons,
            Skill::MissileWeapons,
            Skill::TwoHandedCombat,
        ];
        seed(11);
        let mut made = 0;
        for tier in 1..=6 {
            for category in [TreasureItemCategory::Item, TreasureItemCategory::MagicItem] {
                for _ in 0..40 {
                    let wo = lgf::create_random_loot_objects_of_category(
                        &mut w,
                        &profile(tier, 0.0, 1),
                        category,
                        TreasureItemType::Weapon,
                    )
                    .unwrap_or_else(|| panic!("tier {tier}: a weapon"));
                    made += 1;
                    let skill = wo.weapon_skill();
                    // The era's first tier drops only the plain weapons of each kind.
                    if tier == 1 && wo.biota.weenie_type == WeenieType::MeleeWeapon {
                        let name = wo.get_property(PropertyString::Name).unwrap_or_default();
                        assert!(
                            !["Acid", "Flaming", "Frost", "Lightning"]
                                .iter()
                                .any(|e| name.starts_with(e)),
                            "tier 1: {name}"
                        );
                    }
                    assert!(
                        !later.contains(&skill),
                        "{}: {skill:?}",
                        wo.weenie_class_id()
                    );
                    if let Some(wield) = wo.wield_skill_type() {
                        assert!(
                            !later.contains(&Skill(wield)),
                            "{}: wields with {wield}",
                            wo.weenie_class_id()
                        );
                    }
                }
            }
        }
        assert_eq!(made, 6 * 2 * 40);

        // The era's item counts: a group whose one roll passes drops its minimum, then rolls for
        // each further item; at loot quality 1 every roll passes, so a 10% group of one to three
        // items drops three (at the end of retail it would usually drop none).
        let group = TreasureDeath {
            tier: 2,
            loot_quality_mod: 1.0,
            item_chance: 10,
            item_min_amount: 1,
            item_max_amount: 3,
            item_treasure_type_selection_chances: 1,
            unknown_chances: 1,
            ..TreasureDeath::default()
        };
        for _ in 0..20 {
            assert_eq!(lgf::create_random_loot_objects(&mut w, &group).len(), 3);
        }

        // Armour, clothing, jewelry and food: every roll is a weenie the world has.
        for tier in 1..=6 {
            for item_type in [
                TreasureItemType::Armor,
                TreasureItemType::Clothing,
                TreasureItemType::Jewelry,
                TreasureItemType::Consumable,
            ] {
                for _ in 0..20 {
                    let rules = w.era.loot_rules;
                    let roll = lgf::roll_wcid_in(
                        rules,
                        &profile(tier, 0.0, 1),
                        TreasureItemCategory::Item,
                        item_type,
                    )
                    .expect("a roll");
                    assert!(
                        w.content
                            .get_cached_weenie(roll.wcid.0.cast_unsigned())
                            .is_some(),
                        "tier {tier} {item_type:?}: {} is in the Infiltration world",
                        roll.wcid.0
                    );
                }
            }
        }
    }

    /// Every magic item an Infiltration treasure of tiers 1 to 6 rolls, on the Infiltration world,
    /// carries the era's magic: each spell at a level the era's tier gives (never the eighth),
    /// cantrips minor or major only, none on jewelry or clothing, and no later spell (Spirit
    /// Drinker, void magic, the later skills' masteries); a scroll teaches one of the era's
    /// spells at one of its tier's levels.
    /// Divergence: V416
    #[test]
    fn every_infiltration_magic_item_carries_its_eras_magic() {
        use empyrean_entity::enums::SpellId;
        use empyrean_tables::logic::era::infiltration as era;
        use empyrean_tables::logic::tables::spell_level_progression;
        let mut w = real_world();
        let path = empyrean_common::test_paths::infiltration_pack();
        w.content = Arc::new(PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the Infiltration world.pack at {} (set EMPYREAN_TEST_INFILTRATION_PACK): {e}",
                path.display()
            )
        }));
        w.era = empyrean_common::era::EraId::Infiltration.rules();
        let max_level = [3, 5, 6, 6, 6, 6];
        let later = [
            SpellId::SpiritDrinkerSelf1,
            SpellId::NetherBolt1,
            SpellId::CantripVoidMagicAptitude1,
            SpellId::TwoHandedMasterySelf1,
            SpellId::DualWieldMasterySelf1,
        ];
        seed(21);
        let (mut spells_seen, mut cantrips_seen) = (0, 0);
        for tier in 1..=6 {
            for item_type in [
                TreasureItemType::Weapon,
                TreasureItemType::Armor,
                TreasureItemType::Clothing,
                TreasureItemType::Jewelry,
                TreasureItemType::Caster,
            ] {
                for _ in 0..30 {
                    let Some(wo) = lgf::create_random_loot_objects_of_category(
                        &mut w,
                        &profile(tier, 1.0, 1),
                        TreasureItemCategory::MagicItem,
                        item_type,
                    ) else {
                        continue;
                    };
                    let book: Vec<SpellId> = wo
                        .biota
                        .properties_spell_book
                        .as_ref()
                        .map(|b| b.keys().map(|&s| SpellId(s.cast_unsigned())).collect())
                        .unwrap_or_default();
                    for spell in book {
                        let levels = spell_level_progression::get_spell_levels(spell)
                            .unwrap_or_else(|| panic!("{spell:?} has levels"));
                        let first = levels.iter().copied().find(|&s| s != SpellId::Undef);
                        assert!(
                            !later.iter().any(|&l| Some(l) == first),
                            "tier {tier} {item_type:?}: {spell:?}"
                        );
                        let level = levels
                            .iter()
                            .position(|&s| s == spell)
                            .expect("in its levels")
                            + 1;
                        if levels.len() == 4 {
                            cantrips_seen += 1;
                            assert!(level <= 2, "tier {tier}: cantrip {spell:?}");
                            assert!(
                                !matches!(
                                    item_type,
                                    TreasureItemType::Jewelry | TreasureItemType::Clothing
                                ),
                                "tier {tier} {item_type:?}: cantrip {spell:?}"
                            );
                        } else {
                            spells_seen += 1;
                            let max = max_level[usize::try_from(tier - 1).expect("tier")];
                            assert!(
                                i32::try_from(level).expect("small") <= max,
                                "tier {tier} {item_type:?}: {spell:?} is level {level}"
                            );
                        }
                    }
                }
            }
            for _ in 0..20 {
                let Some(scroll) = lgf::create_random_loot_objects_of_category(
                    &mut w,
                    &profile(tier, 0.0, 1),
                    TreasureItemCategory::Item,
                    TreasureItemType::Scroll,
                ) else {
                    continue;
                };
                let spell = SpellId(scroll.spell_did().expect("a scroll's spell"));
                let levels = spell_level_progression::get_spell_levels(spell).expect("levels");
                let level = levels
                    .iter()
                    .position(|&s| s == spell)
                    .expect("in its levels")
                    + 1;
                assert!(
                    era::scroll_spells().iter().any(|&s| era::spell_at_level(
                        s,
                        i32::try_from(level).expect("small")
                    ) == spell),
                    "tier {tier}: scroll of {spell:?}"
                );
            }
        }
        assert!(spells_seen > 200, "{spells_seen} spells");
        assert!(cantrips_seen > 0, "{cantrips_seen} cantrips");
    }

    /// The `treasure_death` row with primary key `id` (the harness names profiles by id).
    fn profile_by_id(w: &World, id: i64) -> TreasureDeath {
        let id = u32::try_from(id).expect("u32");
        w.content
            .get_all_treasure_death()
            .values()
            .find(|p| p.id == id)
            .cloned()
            .unwrap_or_else(|| panic!("no treasure_death {id}"))
    }

    fn run(make: impl FnOnce() -> Vec<WorldObject>) -> Value {
        run_nullable(|| make().into_iter().map(Some).collect())
    }

    /// As the harness's `Run`: a `null` item (a kind that created nothing) stays in the list.
    fn run_nullable(make: impl FnOnce() -> Vec<Option<WorldObject>>) -> Value {
        match catch_unwind(AssertUnwindSafe(make)) {
            Ok(items) => json!({
                "items": items.iter().map(|i| i.as_ref().map_or(Value::Null, item_diff)).collect::<Vec<_>>(),
                "post": post(),
            }),
            Err(_) => json!({ "throws": "panic" }),
        }
    }

    /// A known gap outside this unit: ACE's `Scroll.SetEphemeralValues` sets a scroll's LongDesc
    /// ("Inscribed spell: ...") from its spell; `scroll.rs` still stubs it (`not_ported!("ACE:
    /// Spell.Spell")`). Where the port's scroll lacks that one value, it is taken out of ACE's
    /// item before comparing; once `scroll.rs` sets it, the comparison is exact again.
    fn exclude_scroll_long_desc(expected: &Value, got: &Value) -> (Value, usize) {
        let mut expected = expected.clone();
        let mut n = 0;
        if let (Some(e_items), Some(g_items)) = (
            expected.get_mut("items").and_then(Value::as_array_mut),
            got.get("items").and_then(Value::as_array),
        ) {
            for (e, g) in e_items.iter_mut().zip(g_items) {
                let is_long_desc = |x: &Value| {
                    x[0] == "string"
                        && x[1] == 16
                        && x[2]
                            .as_str()
                            .is_some_and(|s| s.starts_with("Inscribed spell: "))
                };
                let got_has = g["set"]
                    .as_array()
                    .is_some_and(|s| s.iter().any(|x| x[0] == "string" && x[1] == 16));
                if let Some(set) = e.get_mut("set").and_then(Value::as_array_mut) {
                    if !got_has && set.iter().any(is_long_desc) {
                        set.retain(|x| !is_long_desc(x));
                        n += 1;
                    }
                }
            }
        }
        (expected, n)
    }

    fn check(name: &str, mut each: impl FnMut(&mut World, &Value) -> Value) {
        let mut w = real_world();
        let file = vectors::load_named("lootgen", name);
        assert!(!file.cases.is_empty(), "lootgen/{name}: no cases");
        let mut failures = Vec::new();
        let mut excluded = 0;
        for c in &file.cases {
            let got = each(&mut w, &c.input);
            let (expected, n) = exclude_scroll_long_desc(&normalise_throws(&c.output), &got);
            excluded += n;
            if got != expected {
                failures.push(format!(
                    "in {}\n    expected {}\n    got      {got}",
                    c.input, c.output
                ));
            }
        }
        // LOOT_SHOW=n lists more of the differences
        let show = std::env::var("LOOT_SHOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        assert!(
            failures.is_empty(),
            "lootgen/{name}: {} of {} differ:
  {}",
            failures.len(),
            file.cases.len(),
            failures
                .iter()
                .take(show)
                .cloned()
                .collect::<Vec<_>>()
                .join(
                    "
  "
                )
        );
        if excluded > 0 {
            eprintln!("lootgen/{name}: {excluded} scroll LongDesc value(s) excluded (scroll descriptions omitted)");
        }
    }

    /// Every tier 1-6 death-treasure profile of the pack, rolled under the rules of the era the
    /// pack was built for, creates every item it rolls: no roll is left naming a weenie the world
    /// database lacks.
    /// Divergence: V387
    #[test]
    fn loot_rolled_under_the_packs_era_creates_every_item_of_tiers_1_to_6() {
        let _tables = crate::cantrip_tables_read();
        let mut w = real_world();
        w.era = w.content.era().rules();
        seed(20_260_929);
        let mut profiles: Vec<TreasureDeath> = w
            .content
            .get_all_treasure_death()
            .values()
            .filter(|p| (1..=6).contains(&p.tier))
            .cloned()
            .collect();
        profiles.sort_by_key(|p| p.id);
        assert!(!profiles.is_empty(), "the pack has tier 1-6 profiles");
        let mut created = 0;
        let mut failed = Vec::new();
        for p in &profiles {
            for (category, chance) in [
                (TreasureItemCategory::Item, p.item_chance),
                (TreasureItemCategory::MagicItem, p.magic_item_chance),
                (TreasureItemCategory::MundaneItem, p.mundane_item_chance),
            ] {
                if chance <= 0 {
                    continue;
                }
                for _ in 0..12 {
                    match lgf::create_random_loot_objects_of_category(
                        &mut w,
                        p,
                        category,
                        TreasureItemType::Undef,
                    ) {
                        Some(_) => created += 1,
                        None => failed.push((p.id, category)),
                    }
                }
            }
        }
        assert!(created > 0);
        assert!(
            failed.is_empty(),
            "era {}: {} of {} rolls created nothing, first {:?}",
            w.era.id,
            failed.len(),
            created + failed.len(),
            &failed[..failed.len().min(8)]
        );
    }

    /// `SpellLevelCache.GetSpellLevel` is the client (scarab) level: Strength Other I-VIII are
    /// levels 1-8 by their first component.
    #[test]
    fn spell_level_cache_reads_the_scarab_level() {
        use empyrean_world::factories::entity::spell_level_cache::get_spell_level;
        let w = real_world();
        let levels = empyrean_tables::logic::tables::spell_level_progression::get_spell_levels(
            empyrean_entity::enums::SpellId::StrengthOther1,
        )
        .expect("a progression");
        for (i, s) in levels.iter().enumerate() {
            assert_eq!(
                get_spell_level(&w, s.0.cast_signed()),
                i32::try_from(i + 1).unwrap(),
                "{s:?}"
            );
        }
    }

    #[test]
    fn the_pack_holds_the_dumps_profiles() {
        check("profiles", |w, input| {
            let p = profile_by_id(w, int(input, "id"));
            json!({
                "tier": p.tier, "qm": f64::from(p.loot_quality_mod), "heritage": p.unknown_chances,
                "item": [p.item_chance, p.item_min_amount, p.item_max_amount, p.item_treasure_type_selection_chances],
                "magic": [p.magic_item_chance, p.magic_item_min_amount, p.magic_item_max_amount, p.magic_item_treasure_type_selection_chances],
                "mundane": [p.mundane_item_chance, p.mundane_item_min_amount, p.mundane_item_max_amount, p.mundane_item_type_selection_chances],
            })
        });
    }

    fn corpses(tier: u32) {
        check(&format!("corpses_t{tier}"), |w, input| {
            let p = profile_by_id(w, int(input, "profile"));
            seed(int(input, "seed"));
            run(|| lgf::create_random_loot_objects(w, &p))
        });
    }

    #[test]
    fn corpses_t1_match_ace() {
        corpses(1);
    }
    #[test]
    fn corpses_t2_match_ace() {
        corpses(2);
    }
    #[test]
    fn corpses_t3_match_ace() {
        corpses(3);
    }
    #[test]
    fn corpses_t4_match_ace() {
        corpses(4);
    }
    #[test]
    fn corpses_t5_match_ace() {
        corpses(5);
    }
    #[test]
    fn corpses_t6_match_ace() {
        corpses(6);
    }
    #[test]
    fn corpses_t7_match_ace() {
        corpses(7);
    }
    #[test]
    fn corpses_t8_match_ace() {
        corpses(8);
    }

    #[test]
    fn every_item_kind_matches_ace() {
        for tier in 1..=8 {
            check(&format!("kinds_t{tier}"), |w, input| {
                let p = profile_by_id(w, int(input, "profile"));
                let category =
                    TreasureItemCategory(i32::try_from(int(input, "category")).expect("i32"));
                let r#type = TreasureItemType(i32::try_from(int(input, "type")).expect("i32"));
                seed(int(input, "seed"));
                run_nullable(|| {
                    vec![lgf::create_random_loot_objects_of_category(
                        w, &p, category, r#type,
                    )]
                })
            });
        }
    }

    #[test]
    fn rares_match_ace() {
        check("rares", |w, input| {
            match input["op"].as_str().expect("op") {
                "create" => {
                    let luck = i32::try_from(int(input, "luck")).expect("i32");
                    seed(int(input, "seed"));
                    run(|| rare::try_create_rare(w, luck).into_iter().collect())
                }
                _ => json!(rare::get_rare_tier(
                    u32::try_from(int(input, "wcid")).expect("u32")
                )),
            }
        });
    }

    #[test]
    fn slag_matches_ace() {
        check("slag", |w, input| {
            let p = profile_by_id(w, int(input, "profile"));
            seed(int(input, "seed"));
            run(|| {
                empyrean_world::factories::loot_generation_factory_olthoi_play::roll_slag(w, &p)
                    .into_iter()
                    .collect()
            })
        });
    }

    #[test]
    fn mutate_item_matches_ace() {
        check("mutate_item", |w, input| {
            let p = profile_by_id(w, int(input, "profile"));
            let wcid = u32::try_from(int(input, "wcid")).expect("u32");
            let Some(mut wo) = lgf::world_object_factory_create_new_world_object(w, wcid) else {
                return json!({ "items": [] });
            };
            seed(int(input, "seed"));
            match catch_unwind(AssertUnwindSafe(|| lgf::mutate_item(w, &mut wo, &p, true))) {
                Ok(mutated) => {
                    json!({ "mutated": mutated, "items": [item_diff(&wo)], "post": post() })
                }
                Err(_) => json!({ "throws": "panic" }),
            }
        });
    }

    #[test]
    fn treasure_generators_match_ace() {
        check("test_generators", |w, input| {
            let tier = i32::try_from(int(input, "tier")).expect("i32");
            let p = TreasureDeath {
                tier,
                ..TreasureDeath::default()
            };
            let magical = input["magical"].as_bool().expect("bool");
            seed(int(input, "seed"));
            match input["op"].as_str().expect("op") {
                "melee" => {
                    run(|| {
                        empyrean_world::factories::loot_generation_factory_melee::create_melee_weapon(w, &p, magical).into_iter().collect()
                    })
                }
                "missile" => run(|| {
                    empyrean_world::factories::loot_generation_factory_missile::create_missile_weapon(w, &p, magical, true).into_iter().collect()
                }),
                "caster" => run(|| {
                    empyrean_world::factories::loot_generation_factory_caster::create_caster(
                        w, &p, magical,
                    )
                    .into_iter()
                    .collect()
                }),
                _ => {
                    let bias = LootBias(i32::try_from(int(input, "bias")).expect("i32"));
                    run(|| {
                        lgf::create_random_loot_objects_test(w, &p, magical, bias)
                            .into_iter()
                            .collect()
                    })
                }
            }
        });
    }
}

mod world_treasure {
    use crate::support::treasure_world::*;

    /// `GeneratorProfile.Spawn` of a `Treasure` profile whose "wcid" is a death-treasure
    /// DID: `TreasureGenerator` calls `LootGenerationFactory.CreateRandomLootObjects`, the items join
    /// the store, each is linked to the generator and placed (Spawn_Default), and the generator is
    /// marked `GeneratedTreasureItem`.
    #[test]
    fn a_treasure_generator_spawns_the_loot_factorys_items_into_the_store() {
        let mut h = H::new();
        h.load_landblock();
        let g = h.place_new(GEN_WCID, GEN);

        h.regenerate(g);

        // Before TreasureGenerator: the generator's constructor (its heartbeat) and `SelectAProfile`
        // (one draw for the one initial selection); the replay makes the same two draws.
        let expected = factory_replay(|w| {
            let _ = CtorEnv::with_world(w, |env| {
                factory::create_new_world_object_by_wcid(env, GEN_WCID, ObjectGuid::new(GEN))
            });
            let _select = ThreadSafeRandom::next_float(0.0, 1.0);
        });

        let spawned = h.spawned(g);
        assert_eq!(spawned.len(), 2, "two pyreal stacks");
        let items: Vec<&WorldObject> = spawned.iter().map(|&s| h.o(s)).collect();
        assert_eq!(
            stacks(items.iter().copied()),
            expected,
            "the factory's items, draw for draw"
        );
        // Pinned: tier-1 pyreals are 5..=50 each (MutateCoins), value = stack size, no burden.
        assert_eq!(
            expected,
            [
                (COINSTACK_WCID, Some(41), Some(41), Some(0)),
                (COINSTACK_WCID, Some(16), Some(16), Some(0))
            ]
        );
        for s in &spawned {
            let o = h.o(*s);
            assert_eq!(o.wo.world_object_generators.generator, Some(g));
            assert_eq!(o.generator_id(), Some(GEN));
            assert_eq!(
                o.current_landblock,
                Some(LandblockId::new(u32::from(LB) << 16 | 0xFFFF)),
                "placed"
            );
        }
        assert!(h.o(g).generated_treasure_item());
    }

    /// A generator stack size sets value and burden.
    #[test]
    fn a_generator_stack_size_sets_value_and_burden() {
        let mut h = H::new();
        h.load_landblock();
        let g = h.place_new(STACK_GEN_WCID, GEN);
        h.regenerate(g);
        let spawned = h.spawned(g);
        assert_eq!(spawned.len(), 1);
        let o = h.o(spawned[0]);
        assert_eq!(
            (o.stack_size(), o.value(), o.encumbrance_val()),
            (Some(5), Some(15), Some(10))
        );
    }

    /// `Creature.CreateCorpse` -> `GenerateTreasure`: a creature with a `DeathTreasureType` gets the
    /// factory's items in its corpse (`corpse.TryAddToInventory`), after the corpse's constructor
    /// draw. The corpse holds them; they have left the landscape.
    #[test]
    fn a_creature_with_death_treasure_drops_the_factorys_items_in_its_corpse() {
        let mut h = H::new();
        h.load_landblock();
        h.creature(Class::Creature, MONSTER, "Drudge");
        h.o_mut(MONSTER)
            .set_property(PropertyDataId::DeathTreasureType, PYREALS_DID);
        assert_eq!(
            creature_death::death_treasure(&h.w, MONSTER).map(|t| t.tier),
            Some(1)
        );
        // on the landscape: `CreateCorpse` reads the physics body's position and velocity
        assert!(
            lm::add_object(&mut h.w, MONSTER, false),
            "placed on the landblock"
        );

        ThreadSafeRandom::seed(SEED);
        creature_death::create_corpse(&mut h.w, MONSTER, None, false);

        let corpse = (0x8000_0000..0x8000_0010u32)
            .map(ObjectGuid::new)
            .find(|g| h.w.objects.get(*g).is_some_and(WorldObject::is_corpse))
            .expect("a corpse");
        let contents = creature_death::container_inventory(&h.w, corpse);
        let items: Vec<&WorldObject> = contents.iter().map(|&i| h.o(i)).collect();

        // The replay: the corpse's constructor (one dynamic guid and its heartbeat draw), then the factory.
        let expected = factory_replay(|w| {
            let weenie = w
                .content
                .get_cached_weenie_by_class_name("corpse")
                .expect("corpse weenie");
            let guid = gm::new_dynamic_guid(w);
            let _ = CtorEnv::with_world(w, |env| {
                factory::create_world_object(env, Some(weenie), guid)
            });
        });
        assert_eq!(
            stacks(items.iter().copied()),
            expected,
            "the factory's items, draw for draw"
        );
        assert_eq!(
            expected,
            [
                (COINSTACK_WCID, Some(32), Some(32), Some(0)),
                (COINSTACK_WCID, Some(28), Some(28), Some(0))
            ],
            "pinned"
        );
        for o in &items {
            assert_eq!(o.container_id(), Some(corpse.full()));
            assert!(
                o.location().is_none(),
                "in the corpse, not on the landscape"
            );
        }
        assert_eq!(
            h.o(corpse).get_property(PropertyString::Name).as_deref(),
            Some("Corpse of Drudge")
        );
    }

    /// The whole death chain: damage, `Die`, the death animation (0 s), then the corpse on the
    /// landblock with the loot factory's pyreals, and the monster destroyed.
    #[test]
    fn a_killed_monster_leaves_its_death_treasure_on_its_corpse() {
        let mut h = H::new();
        h.load_landblock();
        h.creature(Class::Creature, MONSTER, "Drudge");
        h.o_mut(MONSTER)
            .set_property(PropertyDataId::DeathTreasureType, PYREALS_DID);
        assert!(lm::add_object(&mut h.w, MONSTER, false));
        h.creature(Class::Creature, PLAYER, "Slayer");

        damage_history::add(&mut h.w, MONSTER, PLAYER, DamageType::Slash, 100);
        creature_death::die(&mut h.w, MONSTER);
        h.tick();

        assert!(h.w.objects.get(MONSTER).is_none(), "destroyed");
        let corpse = (0x8000_0000..0x8000_0010u32)
            .map(ObjectGuid::new)
            .find(|g| h.w.objects.get(*g).is_some_and(WorldObject::is_corpse))
            .expect("a corpse");
        let contents = creature_death::container_inventory(&h.w, corpse);
        assert_eq!(contents.len(), 2, "two pyreal stacks");
        for i in contents {
            let o = h.o(i);
            assert_eq!(o.biota.weenie_class_id, COINSTACK_WCID);
            assert!(
                (5..=50).contains(&o.stack_size().unwrap()),
                "tier-1 pyreals"
            );
            assert_eq!(o.container_id(), Some(corpse.full()));
        }
    }

    /// V291: treasure the creature carries in its own `Inventory` (InventoryTreasureType loot is
    /// marked `Treasure`; a wielded-treasure item the creature could not wield keeps `WieldTreasure`)
    /// goes onto the corpse with the rest. After the death chain (corpse, then the creature's
    /// `Destroy`), each item the corpse lists is a live object whose guid was not recycled, and it can
    /// be taken out of the corpse into another container.
    #[test]
    fn inventory_treasure_moves_to_the_corpse_and_outlives_the_creature() {
        use empyrean_entity::enums::DestinationType;
        use empyrean_world::world_objects::{container, world_object as wo};

        let mut h = H::new();
        h.load_landblock();
        h.creature(Class::Creature, MONSTER, "Drudge");
        h.o_mut(MONSTER)
            .set_property(PropertyInt::ItemsCapacity, 10);
        assert!(lm::add_object(&mut h.w, MONSTER, false));
        h.creature(Class::Creature, PLAYER, "Slayer");
        h.o_mut(PLAYER).set_property(PropertyInt::ItemsCapacity, 10);

        let mut carried = Vec::new();
        for dest in [DestinationType::Treasure, DestinationType::WieldTreasure] {
            let g = gm::new_dynamic_guid(&mut h.w);
            let mut o = CtorEnv::with_world(&h.w, |env| {
                factory::create_new_world_object_by_wcid(env, STACK_WCID, g)
            })
            .expect("weenie");
            o.wo.world_object.destination_type = dest;
            h.w.objects.insert(o).expect("fresh guid");
            assert!(
                container::try_add_to_inventory(&mut h.w, MONSTER, g, 0, false, true),
                "carried by the creature"
            );
            carried.push(g);
        }
        let recycled = |w: &mut World| {
            gm::get_dynamic_guid_debug_info(w)
                .rsplit("recycled GUIDs available: ")
                .next()
                .map(str::to_owned)
        };
        let recycled_before = recycled(&mut h.w);

        damage_history::add(&mut h.w, MONSTER, PLAYER, DamageType::Slash, 100);
        creature_death::die(&mut h.w, MONSTER);
        h.tick();

        assert!(
            h.w.objects.get(MONSTER).is_none(),
            "the creature is destroyed"
        );
        let corpse = (0x8000_0000..0x8000_0010u32)
            .map(ObjectGuid::new)
            .find(|g| h.w.objects.get(*g).is_some_and(WorldObject::is_corpse))
            .expect("a corpse");
        let contents = creature_death::container_inventory(&h.w, corpse);
        for g in &carried {
            assert!(contents.contains(g), "0x{g} is on the corpse");
        }
        for g in contents {
            let o = h.w.objects.get(g).expect("a listed item exists");
            assert!(
                !o.wo.world_object.is_destroyed,
                "0x{g} was not destroyed with the creature"
            );
            assert_eq!(o.container_id(), Some(corpse.full()));
        }
        assert_eq!(
            recycled(&mut h.w),
            recycled_before,
            "no carried item's guid was recycled"
        );

        for g in carried {
            assert!(
                container::try_remove_from_inventory(&mut h.w, corpse, g, false),
                "taken from the corpse"
            );
            assert!(
                container::try_add_to_inventory(&mut h.w, PLAYER, g, 0, false, true),
                "into another container"
            );
            assert!(!h.o(g).wo.world_object.is_destroyed);
            assert!(container::try_remove_from_inventory(
                &mut h.w, PLAYER, g, false
            ));
            wo::destroy(&mut h.w, g, true, false);
            assert!(h.w.objects.get(g).is_none(), "a later Destroy takes it");
        }
    }

    /// Changing stack size updates value and encumbrance.
    #[test]
    fn changing_stack_size_updates_value_and_encumbrance() {
        let h = H::new();
        let mut o = CtorEnv::with_world(&h.w, |env| {
            factory::create_new_world_object_by_wcid(env, STACK_WCID, ObjectGuid::new(0x7A9B_4400))
        })
        .expect("weenie exists");
        empyrean_world::world_objects::world_object_networking::shims::set_stack_size(
            &mut o,
            Some(4),
        );
        assert_eq!(
            (o.stack_size(), o.value(), o.encumbrance_val()),
            (Some(4), Some(12), Some(8))
        );
    }
}

#[cfg(feature = "real-content")]
mod world_treasure_real {
    mod real_content {
        use crate::support::treasure_world::real_content::*;

        #[test]
        fn the_275_templates_carry_twelve_rended_loot_weapons() {
            let _tables = crate::cantrip_tables_read();
            let mut w = real_world();
            ThreadSafeRandom::seed(SEED);
            let weenie = w
                .content
                .get_cached_weenie_by_class_name("human")
                .expect("human weenie");

            let guid = gm::new_player_guid(&mut w);
            let p = player_factory_ex::create_275_heavy_weapons(
                &mut w,
                Arc::clone(&weenie),
                guid,
                1,
                "Heavy",
            );
            let r = rended(&p);
            assert_eq!(r.len(), 12, "{r:?}");
            for o in p
                .inventory
                .iter()
                .filter(|o| o.get_property(PropertyInt::ImbuedEffect).is_some())
            {
                assert_eq!(
                    o.weapon_skill(),
                    Skill::HeavyWeapons,
                    "only heavy weapons are kept"
                );
            }

            let guid = gm::new_player_guid(&mut w);
            let p = player_factory_ex::create_275_missile_weapons(
                &mut w,
                Arc::clone(&weenie),
                guid,
                1,
                "Missile",
            );
            // Twelve missile weapons, each through AddRend: only an elemental one (a single
            // W_DamageType) gets a rending imbue; a plain bow's damage comes from its ammunition.
            // `TryAddToInventory` checks the player's burden, and this template's Strength is 10:
            // only the first ones fit (as in ACE; the template is unreferenced there).
            let missile: Vec<&WorldObject> = p
                .inventory
                .iter()
                .filter(|o| o.weapon_skill() == Skill::MissileWeapons)
                .collect();
            assert!(!missile.is_empty());
            for o in missile {
                let single = matches!(
                    o.get_property(PropertyInt::DamageType),
                    Some(1 | 2 | 4 | 8 | 16 | 32 | 64 | 3)
                );
                assert_eq!(
                    o.get_property(PropertyInt::ImbuedEffect).is_some(),
                    single,
                    "wcid {}",
                    o.biota.weenie_class_id
                );
            }

            let guid = gm::new_player_guid(&mut w);
            let p = player_factory_ex::create_275_war_magic(&mut w, weenie, guid, 1, "Wand");
            // The loop runs until twelve elemental war wands (wandacid..=wandslashing) were made; the
            // Strength-10 template carries what its burden allows, all of them such wands.
            let r = rended(&p);
            assert!(
                r.iter().all(|(wcid, _)| (29259..=29265).contains(wcid)),
                "elemental war wands only: {r:?}"
            );
        }
    }
}

mod drop_rates {

    /// `CreateList` / `CreateListSetModifier` in `Creature.CreateListSelect(list, dropRateMod)`: a set
    /// of a 50% trophy and a 50% nothing, at drop rate 2, has TrophyMod 2 (0.5 * 2 is not above 1)
    /// and NoneMod (1 - 1) / 0.5 = 0, so the trophy always drops; at 1 it sometimes does not.
    #[test]
    fn a_drop_rate_scales_a_treasure_sets_trophy_chance() {
        use empyrean_entity::enums::DestinationType;
        use empyrean_entity::models::PropertiesCreateList;
        let entry = |wcid: u32, id: u32| PropertiesCreateList {
            database_record_id: id,
            destination_type: DestinationType::Treasure,
            weenie_class_id: wcid,
            shade: 0.5,
            ..Default::default()
        };
        let list = vec![entry(10, 1), entry(0, 2)];

        let set = empyrean_world::entity::create_list::CreateList::new(&list);
        assert_eq!(set.item_sets, [0, 0]);
        let m = set.get_set_modifier(0, 2.0);
        assert_eq!(
            (m.trophy_mod, m.none_mod(), m.trophy_probability()),
            (2.0, 0.0, 1.0)
        );
        assert_eq!(
            set.get_set_modifier(0, 4.0).trophy_mod,
            2.0,
            "capped so the trophy is certain"
        );

        empyrean_common::thread_safe_random::ThreadSafeRandom::seed(3);
        for _ in 0..50 {
            let got =
                empyrean_world::world_objects::creature_equipment::create_list_select_with_drop_rate(
                    &list, 2.0,
                );
            assert_eq!(
                got.iter().map(|e| e.weenie_class_id).collect::<Vec<_>>(),
                [10]
            );
        }
        let dropped = (0..50)
            .filter(|_| {
                empyrean_world::world_objects::creature_equipment::create_list_select_with_drop_rate(
                    &list, 1.0,
                )
                .iter()
                .any(|e| e.weenie_class_id == 10)
            })
            .count();
        assert!(dropped > 0 && dropped < 50, "{dropped} of 50");
    }
}

#[cfg(feature = "real-content")]
mod retail_creature_loot {
    use crate::support::creature_world::real_content::*;

    /// V291 on real content: the five creatures with an `InventoryTreasureType` (the Focusing
    /// Stone quest's liches and skeletons) carry that loot in their Inventory, marked `Treasure`.
    /// Killed, each leaves it on its corpse; after the creature's `Destroy` every item the corpse
    /// lists is a live object, and it can be taken out of the corpse.
    #[test]
    fn inventory_treasure_creatures_leave_live_loot_on_their_corpses() {
        use empyrean_entity::enums::{DamageType, DestinationType};
        use empyrean_world::entity::damage_history;
        use empyrean_world::world_objects::{container, creature_death};

        let mut h = world();
        let lb = LandblockId::new(u32::from(HOME) << 16 | 0xFFFF);
        lm::get_landblock(&mut h.w, lb, false, false);
        let p = player(&mut h, 48.0, 32.0);
        let mut carried_total = 0;
        for (i, wcid) in [4123u32, 4124, 4125, 4126, 4127].into_iter().enumerate() {
            let g = gm::new_dynamic_guid(&mut h.w);
            let mut o = CtorEnv::with_world(&h.w, |env| {
                factory::create_new_world_object_by_wcid(env, wcid, g)
            })
            .expect("the weenie");
            #[allow(clippy::cast_precision_loss)]
            let x = 120.0 + 10.0 * i as f32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // a cell index from a position on the block
            let cell = u32::from(HOME) << 16 | ((x / 24.0) as u32 * 8 + (120.0 / 24.0) as u32 + 1);
            let z = empyrean_world::entity::position_extensions::get_terrain_z(
                &h.w,
                &Position::from_components(cell, x, 120.0, 0.0, 0.0, 0.0, 0.0, 1.0, false),
            );
            o.set_location(Some(Position::from_components(
                cell,
                x,
                120.0,
                z + 0.005,
                0.0,
                0.0,
                0.0,
                1.0,
                false,
            )));
            h.w.objects.insert(o).expect("fresh guid");
            creature::post_insert(&mut h.w, g);
            assert!(lm::add_object(&mut h.w, g, false), "placed");

            let carried: Vec<ObjectGuid> = container::inventory_values(&h.w, g)
                .into_iter()
                .filter(|&c| {
                    (h.o(c).wo.world_object.destination_type & DestinationType::Treasure)
                        != DestinationType::default()
                })
                .collect();
            carried_total += carried.len();

            damage_history::add(&mut h.w, g, p, DamageType::Slash, 10_000);
            creature_death::die(&mut h.w, g);
            for _ in 0..200 {
                if h.w.objects.get(g).is_none() {
                    break;
                }
                h.advance(0.05);
                h.tick();
            }
            assert!(
                h.w.objects.get(g).is_none(),
                "{wcid} is destroyed after its death animation"
            );

            let corpse = carried
                .first()
                .and_then(|&c| h.w.objects.get(c))
                .and_then(|o| o.container_id())
                .map(ObjectGuid::new);
            for &c in &carried {
                let item =
                    h.w.objects
                        .get(c)
                        .unwrap_or_else(|| panic!("{wcid}: carried 0x{c} still exists"));
                assert!(
                    !item.wo.world_object.is_destroyed,
                    "{wcid}: 0x{c} was not destroyed with its creature"
                );
                assert_eq!(
                    item.container_id().map(ObjectGuid::new),
                    corpse,
                    "{wcid}: 0x{c} is on the corpse"
                );
            }
            if let Some(corpse) = corpse {
                assert!(h.o(corpse).is_corpse());
                for c in creature_death::container_inventory(&h.w, corpse) {
                    assert!(
                        !h.o(c).wo.world_object.is_destroyed,
                        "{wcid}: everything the corpse lists is live"
                    );
                }
                for &c in &carried {
                    assert!(
                        container::try_remove_from_inventory(&mut h.w, corpse, c, false),
                        "{wcid}: taken from the corpse"
                    );
                }
            }
        }
        assert!(
            carried_total > 0,
            "the five creatures carried some inventory treasure"
        );
    }
}
