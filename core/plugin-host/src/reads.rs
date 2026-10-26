//! Reads of one object for plugins: its class, and its properties by the keys loot profiles use.
//!
//! An object's properties come from two places: what every client is told about it when it comes
//! into view (its public description: name, type, value, stack, container, icon), and what an
//! appraisal adds (the property tables, spells, weapon and armour figures). The keys are the game's
//! own property numbers, plus a block of keys above `0x0C000000` (integers) and `0x0A000000`
//! (floats) that name figures the game keeps outside its tables, as loot profiles written for the
//! retail client's plugins spell them.

use dereth_client_model::{StatKey, StatType, StatValue, Weenie, World};
use dereth_plugin_api::Value;
use dereth_primitives::ObjectId;
use dereth_protocol::types::AppraisalProfile;
use dereth_rules::weenie::{bitfield, item_type};

/// An object's class, as a number and a name. The numbers are the ones loot profiles store.
#[must_use]
pub fn class_of(w: &Weenie) -> (u32, &'static str) {
    let t = w.pwd.obj_type;
    let bits = w.pwd.bitfield;
    let name = w.pwd.name.as_str();
    /// The public description's door flag.
    const DOOR: u32 = 0x0000_1000;
    /// The public description's lifestone flag.
    const LIFESTONE: u32 = 0x0000_4000;
    /// The public description's portal flag.
    const PORTAL: u32 = 0x0004_0000;
    let c = |n: u32, s: &'static str| (n, s);
    if bits & bitfield::PLAYER != 0 {
        return c(24, "Player");
    }
    if bits & bitfield::CORPSE != 0 {
        return c(27, "Corpse");
    }
    if bits & bitfield::VENDOR != 0 {
        return c(25, "Vendor");
    }
    if bits & DOOR != 0 {
        return c(26, "Door");
    }
    if bits & LIFESTONE != 0 || t & item_type::LIFESTONE != 0 {
        return c(28, "Lifestone");
    }
    if bits & PORTAL != 0 || t & item_type::PORTAL != 0 {
        return c(14, "Portal");
    }
    if t & item_type::CREATURE != 0 {
        return if bits & bitfield::ATTACKABLE != 0 {
            c(5, "Monster")
        } else {
            c(37, "Npc")
        };
    }
    if bits & bitfield::HEALER != 0 {
        return c(29, "HealingKit");
    }
    if bits & bitfield::LOCKPICK != 0 {
        return c(30, "Lockpick");
    }
    let has = |b: u32| t & b != 0;
    if has(item_type::MELEE_WEAPON) {
        c(1, "MeleeWeapon")
    } else if has(item_type::MISSILE_WEAPON) {
        c(9, "MissileWeapon")
    } else if has(item_type::CASTER) {
        c(31, "WandStaffOrb")
    } else if has(item_type::ARMOR) {
        c(2, "Armor")
    } else if has(item_type::CLOTHING) {
        c(3, "Clothing")
    } else if has(item_type::JEWELRY) {
        c(4, "Jewelry")
    } else if has(item_type::FOOD) {
        c(6, "Food")
    } else if has(item_type::MONEY) {
        c(7, "Money")
    } else if has(item_type::CONTAINER) {
        c(10, "Container")
    } else if has(item_type::GEM) {
        c(11, "Gem")
    } else if has(item_type::SPELL_COMPONENTS) {
        if name.contains("Foci") {
            c(38, "Foci")
        } else {
            c(12, "SpellComponent")
        }
    } else if has(item_type::KEY) {
        c(13, "Key")
    } else if has(item_type::PROMISSORY_NOTE) {
        c(15, "TradeNote")
    } else if has(item_type::MANASTONE) {
        c(16, "ManaStone")
    } else if has(item_type::CRAFT_COOKING_BASE) {
        c(18, "BaseCooking")
    } else if has(item_type::CRAFT_ALCHEMY_BASE) {
        c(19, "BaseAlchemy")
    } else if has(item_type::CRAFT_FLETCHING_BASE) {
        c(20, "BaseFletching")
    } else if has(item_type::CRAFT_ALCHEMY_INTERMEDIATE) {
        c(22, "CraftedAlchemy")
    } else if has(item_type::CRAFT_FLETCHING_INTERMEDIATE) {
        c(23, "CraftedFletching")
    } else if has(item_type::TINKERING_MATERIAL) {
        c(39, "Salvage")
    } else if has(item_type::TINKERING_TOOL) {
        c(40, "Ust")
    } else if has(item_type::SERVICE) {
        c(41, "Services")
    } else if has(item_type::WRITABLE) {
        if name.contains("Scroll") {
            c(42, "Scroll")
        } else if name.contains("Journal") {
            c(34, "Journal")
        } else {
            c(33, "Book")
        }
    } else if has(item_type::MISC | item_type::USELESS) {
        c(8, "Misc")
    } else {
        c(0, "Unknown")
    }
}

/// The first integer key of the figures kept outside the property tables.
pub const INT_EXTRA: u32 = 0x0D00_0000;
/// The first float key of the figures kept outside the property tables.
pub const FLOAT_EXTRA: u32 = 0x0A00_0000;

/// A property of `id`'s own qualities, which the client keeps for the player's character (and
/// keeps current from the server's updates).
fn own(world: &World, id: ObjectId, ty: StatType, key: u32) -> Option<StatValue> {
    world
        .weenie(id)?
        .qualities
        .as_ref()?
        .get(StatKey::new(ty, key))
}

/// An integer property of `id`: its own qualities (the player's), then the appraisal's table, then
/// the public description.
#[must_use]
pub fn int(world: &World, id: ObjectId, key: u32) -> Option<i64> {
    if key < INT_EXTRA {
        if let Some(StatValue::Int(v)) = own(world, id, StatType::Int, key) {
            return Some(i64::from(v));
        }
    }
    let w = world.weenie(id)?;
    let d = &w.pwd;
    let a = world.appraisal.get(id);
    let from_appraisal =
        |k: u32| a.and_then(|p| dereth_client_model::appraisal_model::inq::int(p, k));
    let opt = |v: Option<u32>| v.map(i64::from);
    match key {
        // Burden, value: the public description carries them before any appraisal.
        5 => from_appraisal(5).map(i64::from).or(d.burden.map(i64::from)),
        19 => from_appraisal(19).map(i64::from).or(opt(d.value)),
        131 => from_appraisal(131).map(i64::from).or(opt(d.material_type)),
        k if k >= INT_EXTRA => extra_int(w, a, k - INT_EXTRA),
        k => from_appraisal(k).map(i64::from),
    }
}

fn extra_int(w: &Weenie, a: Option<&AppraisalProfile>, n: u32) -> Option<i64> {
    let d = &w.pwd;
    let opt = |v: Option<u32>| v.map(i64::from);
    let weapon = a.and_then(|p| p.weapon_profile.as_ref());
    match n {
        0 => Some(i64::from(d.obj_type)),
        1 => Some(i64::from(d.icon_id)),
        2 => d.container_id.map(|c| i64::from(c.0)),
        4 => d.items_capacity.map(i64::from),
        5 => d.containers_capacity.map(i64::from),
        6 => d.stack_size.map(i64::from),
        7 => d.max_stack_size.map(i64::from),
        8 => d.spell_id.map(i64::from),
        10 => d.wielder_id.map(|c| i64::from(c.0)),
        11 => opt(d.location),
        13 => opt(d.priority),
        14 => opt(d.valid_locations),
        15 => d.combat_use.map(i64::from),
        16 => opt(d.effects),
        17 => d.ammo_type.map(i64::from),
        18 => opt(d.useability),
        20 => opt(d.hook_item_types),
        21 => d.hook_type.map(i64::from),
        23 | 26 | 27 => Some(i64::from(d.bitfield)),
        30 => a
            .and_then(|p| p.spell_book.as_ref())
            .map(|s| i64::try_from(s.len()).unwrap_or(i64::MAX)),
        31 => weapon.map(|x| i64::from(x.weapon_time)),
        32 => weapon.map(|x| i64::from(x.weapon_skill)),
        33 => weapon.map(|x| i64::from(x.damage_type)),
        34 => weapon.map(|x| i64::from(x.weapon_damage)),
        41 => opt(d.icon_overlay_id),
        42 => opt(d.icon_underlay_id),
        _ => None,
    }
}

/// A float property of `id` (appraisal tables, then the figures outside them).
#[must_use]
pub fn float(world: &World, id: ObjectId, key: u32) -> Option<f64> {
    let w = world.weenie(id)?;
    let a = world.appraisal.get(id);
    if key < FLOAT_EXTRA {
        if let Some(StatValue::Float(v)) = own(world, id, StatType::Float, key) {
            return Some(v);
        }
    }
    if key >= FLOAT_EXTRA {
        let armor = a.and_then(|p| p.armor_profile.as_ref());
        let weapon = a.and_then(|p| p.weapon_profile.as_ref());
        return match key - FLOAT_EXTRA {
            0 => armor.map(|x| f64::from(x.mod_vs_slash)),
            1 => armor.map(|x| f64::from(x.mod_vs_pierce)),
            2 => armor.map(|x| f64::from(x.mod_vs_bludgeon)),
            3 => armor.map(|x| f64::from(x.mod_vs_acid)),
            4 => armor.map(|x| f64::from(x.mod_vs_electric)),
            5 => armor.map(|x| f64::from(x.mod_vs_fire)),
            6 => armor.map(|x| f64::from(x.mod_vs_cold)),
            8 => w.pwd.use_radius.map(f64::from),
            9 => w.pwd.workmanship.map(f64::from),
            11 => weapon.map(|x| x.damage_variance),
            12 => weapon.map(|x| x.weapon_offense),
            13 => weapon.map(|x| x.max_velocity),
            14 => weapon.map(|x| x.damage_mod),
            _ => None,
        };
    }
    a.and_then(|p| dereth_client_model::appraisal_model::inq::float(p, key))
}

/// A string property of `id`: its name (key 1) from the public description, the rest from the
/// appraisal.
#[must_use]
pub fn string(world: &World, id: ObjectId, key: u32) -> Option<String> {
    let w = world.weenie(id)?;
    if key == 1 {
        return Some(w.pwd.name.clone());
    }
    if let Some(StatValue::Str(v)) = own(world, id, StatType::String, key) {
        return Some(v);
    }
    let a = world.appraisal.get(id)?;
    dereth_client_model::appraisal_model::inq::string(a, key)
}

/// A boolean property of `id`: its own qualities, then the appraisal.
#[must_use]
pub fn boolean(world: &World, id: ObjectId, key: u32) -> Option<bool> {
    if let Some(StatValue::Bool(v)) = own(world, id, StatType::Bool, key) {
        return Some(v);
    }
    let a = world.appraisal.get(id)?;
    dereth_client_model::appraisal_model::inq::boolean(a, key)
}

/// A 64-bit integer property of `id`: its own qualities, then the appraisal.
#[must_use]
pub fn int64(world: &World, id: ObjectId, key: u32) -> Option<i64> {
    if let Some(StatValue::Int64(v)) = own(world, id, StatType::Int64, key) {
        return Some(v);
    }
    let a = world.appraisal.get(id)?;
    a.tables
        .int64s
        .as_ref()?
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// A data-id property of `id` (an icon, a setup, a spell set: a number naming game data): its
/// own qualities, then the appraisal.
#[must_use]
pub fn did(world: &World, id: ObjectId, key: u32) -> Option<u32> {
    if let Some(StatValue::Did(v)) = own(world, id, StatType::Did, key) {
        return Some(v.0);
    }
    let a = world.appraisal.get(id)?;
    a.tables
        .dids
        .as_ref()?
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// An instance-id property of `id` (another object: an owner, a wielder, a container): its own
/// qualities, then the appraisal.
#[must_use]
pub fn iid(world: &World, id: ObjectId, key: u32) -> Option<u32> {
    if let Some(StatValue::Iid(v)) = own(world, id, StatType::Iid, key) {
        return Some(v.0);
    }
    let a = world.appraisal.get(id)?;
    a.tables
        .iids
        .as_ref()?
        .entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.0)
}

/// What the client knows of `id`'s health: `{ current, max, fraction }` from the fellowship's
/// figures for a fellow, from the last appraisal for a creature, or `{ fraction }` from the
/// selected object's health bar when it is the one selected; nil when it knows nothing.
#[must_use]
pub fn health(world: &World, view: &dyn dereth_client_contract::GameView, id: ObjectId) -> Value {
    let whole = |cur: u32, max: u32| {
        Value::map().with("current", cur).with("max", max).with(
            "fraction",
            if max == 0 {
                Value::Nil
            } else {
                Value::from(f64::from(cur) / f64::from(max))
            },
        )
    };
    if view.player() == Some(id) {
        if let Some((cur, max)) = view.vital(id, dereth_client_contract::view::Vital::Health) {
            return whole(cur, max);
        }
    }
    if let Some(f) = view.fellowship() {
        if let Some(m) = f.members.iter().find(|m| m.id == id) {
            return whole(m.current_health, m.max_health);
        }
    }
    if world.selected == Some(id) {
        if let (Some(fraction), _) = view.selected_meters() {
            return Value::map().with("fraction", f64::from(fraction));
        }
    }
    if let Some(c) = world
        .appraisal
        .get(id)
        .and_then(|a| a.creature_profile.as_ref())
    {
        return whole(c.health, c.max_health);
    }
    Value::Nil
}

/// The spells an appraisal of `id` listed, by id.
#[must_use]
pub fn spells(world: &World, id: ObjectId) -> Vec<u32> {
    world
        .appraisal
        .get(id)
        .and_then(|p| p.spell_book.clone())
        .unwrap_or_default()
}

/// The last appraisal of `id` as a plugin sees it, or nil: `{ success, ints, floats, strings,
/// bools, spells }`, each table keyed by property number.
#[must_use]
pub fn appraisal(world: &World, id: ObjectId) -> Value {
    let Some(p) = world.appraisal.get(id) else {
        return Value::Nil;
    };
    fn table<T: Clone>(
        entries: Option<&dereth_protocol::archive::PackedHash<u32, T>>,
        f: impl Fn(&T) -> Value,
    ) -> Value {
        let mut m = std::collections::BTreeMap::new();
        for (k, v) in entries.map(|h| h.entries.as_slice()).unwrap_or_default() {
            m.insert(k.to_string(), f(v));
        }
        Value::Map(m)
    }
    let t = &p.tables;
    Value::map()
        .with("success", p.success_flag != 0)
        .with("ints", table(t.ints.as_ref(), |v| Value::from(*v)))
        .with("int64s", table(t.int64s.as_ref(), |v| Value::Int(*v)))
        .with("floats", table(t.floats.as_ref(), |v| Value::Num(*v)))
        .with(
            "strings",
            table(t.strings.as_ref(), |v| Value::from(v.as_str())),
        )
        .with("bools", table(t.bools.as_ref(), |v| Value::Bool(*v != 0)))
        .with(
            "spells",
            Value::List(
                p.spell_book
                    .iter()
                    .flatten()
                    .map(|s| Value::from(*s))
                    .collect(),
            ),
        )
}

/// A property's value of `id` by its type's name (`int`, `int64`, `float`, `string`, `bool`,
/// `did`, `iid`), as a plugin reads it; nil when the client does not know it.
#[must_use]
pub fn property(world: &World, id: ObjectId, ty: &str, key: u32) -> Value {
    match ty {
        "int" => Value::from(int(world, id, key)),
        "int64" => Value::from(int64(world, id, key)),
        "float" => Value::from(float(world, id, key)),
        "string" => Value::from(string(world, id, key)),
        "bool" => Value::from(boolean(world, id, key)),
        "did" => Value::from(did(world, id, key)),
        "iid" => Value::from(iid(world, id, key)),
        _ => Value::Nil,
    }
}

/// Every property the client knows of `id`, by type: `{ ints, int64s, floats, strings, bools,
/// dids, iids }`, each a map from the property's number to its value. What its public description
/// told every client comes first, under the game's own property numbers (its type, value, burden,
/// stack, icons, container and the rest), then what the last appraisal added, then the object's
/// own qualities (the client keeps them for the player's character); a later source's value takes
/// a number's place. With `only`, just that type's map (`"int"`, `"int64"`, `"float"`,
/// `"string"`, `"bool"`, `"did"` or `"iid"`).
#[must_use]
pub fn properties(world: &World, id: ObjectId, only: Option<&str>) -> Value {
    use std::collections::BTreeMap;
    let Some(w) = world.weenie(id) else {
        return Value::Nil;
    };
    let d = &w.pwd;
    let mut ints: BTreeMap<u32, Value> = BTreeMap::new();
    let mut int64s: BTreeMap<u32, Value> = BTreeMap::new();
    let mut floats: BTreeMap<u32, Value> = BTreeMap::new();
    let mut strings: BTreeMap<u32, Value> = BTreeMap::new();
    let mut bools: BTreeMap<u32, Value> = BTreeMap::new();
    let mut dids: BTreeMap<u32, Value> = BTreeMap::new();
    let mut iids: BTreeMap<u32, Value> = BTreeMap::new();
    // The public description, under the game's property numbers.
    for (k, v) in [
        (1, Some(d.obj_type)),
        (4, d.priority),
        (5, d.burden.map(u32::from)),
        (6, d.items_capacity.map(u32::from)),
        (7, d.containers_capacity.map(u32::from)),
        (9, d.valid_locations),
        (10, d.location),
        (11, d.max_stack_size.map(u32::from)),
        (12, d.stack_size.map(u32::from)),
        (16, d.useability),
        (18, d.effects),
        (19, d.value),
        (50, d.ammo_type.map(u32::from)),
        (51, d.combat_use.map(u32::from)),
        (91, d.max_structure.map(u32::from)),
        (92, d.structure.map(u32::from)),
        (94, d.target_type),
        (95, d.blip_color.map(u32::from)),
        (131, d.material_type),
        (133, d.radar_enum.map(u32::from)),
        (151, d.hook_type.map(u32::from)),
        (152, d.hook_item_types),
        (280, d.cooldown_id),
    ] {
        if let Some(v) = v {
            ints.insert(k, Value::from(v));
        }
    }
    if let Some(v) = d.workmanship {
        ints.insert(105, Value::from(f64::from(v).round()));
    }
    if let Some(r) = d.use_radius {
        floats.insert(54, Value::Num(f64::from(r)));
    }
    if let Some(c) = d.cooldown_duration {
        floats.insert(167, Value::Num(c));
    }
    strings.insert(1, Value::from(d.name.as_str()));
    if let Some(p) = &d.plural_name {
        strings.insert(20, Value::from(p.as_str()));
    }
    dids.insert(8, Value::from(d.icon_id));
    for (k, v) in [
        (28, d.spell_id.map(u32::from)),
        (50, d.icon_overlay_id),
        (52, d.icon_underlay_id),
    ] {
        if let Some(v) = v {
            dids.insert(k, Value::from(v));
        }
    }
    for (k, v) in [
        (2, d.container_id),
        (3, d.wielder_id),
        (26, d.monarch),
        (32, d.house_owner_iid),
        (44, d.pet_owner),
    ] {
        if let Some(v) = v {
            iids.insert(k, Value::from(v.0));
        }
    }
    // What the last appraisal added.
    if let Some(a) = world.appraisal.get(id) {
        let t = &a.tables;
        for (k, v) in t.ints.iter().flat_map(|h| h.entries.iter()) {
            ints.insert(*k, Value::from(*v));
        }
        for (k, v) in t.int64s.iter().flat_map(|h| h.entries.iter()) {
            int64s.insert(*k, Value::Int(*v));
        }
        for (k, v) in t.floats.iter().flat_map(|h| h.entries.iter()) {
            floats.insert(*k, Value::Num(*v));
        }
        for (k, v) in t.strings.iter().flat_map(|h| h.entries.iter()) {
            strings.insert(*k, Value::from(v.as_str()));
        }
        for (k, v) in t.bools.iter().flat_map(|h| h.entries.iter()) {
            bools.insert(*k, Value::Bool(*v != 0));
        }
        for (k, v) in t.dids.iter().flat_map(|h| h.entries.iter()) {
            dids.insert(*k, Value::from(*v));
        }
        for (k, v) in t.iids.iter().flat_map(|h| h.entries.iter()) {
            iids.insert(*k, Value::from(v.0));
        }
    }
    // The object's own qualities.
    if let Some(q) = w.qualities.as_ref() {
        for (k, v) in q.ints.iter().flatten() {
            ints.insert(*k, Value::from(*v));
        }
        for (k, v) in q.int64s.iter().flatten() {
            int64s.insert(*k, Value::Int(*v));
        }
        for (k, v) in q.floats.iter().flatten() {
            floats.insert(*k, Value::Num(*v));
        }
        for (k, v) in q.strings.iter().flatten() {
            strings.insert(*k, Value::from(v.as_str()));
        }
        for (k, v) in q.bools.iter().flatten() {
            bools.insert(*k, Value::Bool(*v));
        }
        for (k, v) in q.dids.iter().flatten() {
            dids.insert(*k, Value::from(v.0));
        }
        for (k, v) in q.iids.iter().flatten() {
            iids.insert(*k, Value::from(v.0));
        }
    }
    let map = |m: BTreeMap<u32, Value>| {
        Value::Map(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    };
    match only {
        Some("int") => map(ints),
        Some("int64") => map(int64s),
        Some("float") => map(floats),
        Some("string") => map(strings),
        Some("bool") => map(bools),
        Some("did") => map(dids),
        Some("iid") => map(iids),
        Some(_) => Value::Nil,
        None => Value::map()
            .with("ints", map(ints))
            .with("int64s", map(int64s))
            .with("floats", map(floats))
            .with("strings", map(strings))
            .with("bools", map(bools))
            .with("dids", map(dids))
            .with("iids", map(iids)),
    }
}

/// The property types a condition names.
pub const PROPERTY_TYPES: &[&str] = &["int", "int64", "float", "string", "bool", "did", "iid"];

/// One test of a property in a `world.find` filter's `where`: `{ type, key, op, value }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub ty: String,
    pub key: u32,
    pub op: String,
    pub value: Value,
}

/// The tests a condition may make.
pub const CONDITION_OPS: &[&str] = &[
    "==", "~=", "<", "<=", ">", ">=", "contains", "exists", "missing",
];

impl Condition {
    /// The conditions of a filter's `where` (absent: none).
    ///
    /// # Errors
    /// A condition that is not `{ type, key, op, value }` with a known type and test.
    pub fn list(v: &Value) -> Result<Vec<Self>, String> {
        if v.is_nil() {
            return Ok(Vec::new());
        }
        let items = v
            .as_list()
            .ok_or("where is a list of { type, key, op, value }")?;
        items
            .iter()
            .map(|c| {
                let ty = c.get("type").as_str().unwrap_or_default().to_owned();
                if !PROPERTY_TYPES.contains(&ty.as_str()) {
                    return Err(format!(
                        "a condition's type is one of {}",
                        PROPERTY_TYPES.join(", ")
                    ));
                }
                let key = c
                    .get("key")
                    .as_i64()
                    .and_then(|k| u32::try_from(k).ok())
                    .ok_or("a condition's key is a property number")?;
                let op = c.get("op").as_str().unwrap_or("==").to_owned();
                if !CONDITION_OPS.contains(&op.as_str()) {
                    return Err(format!(
                        "a condition's op is one of {}",
                        CONDITION_OPS.join(" ")
                    ));
                }
                Ok(Self {
                    ty,
                    key,
                    op,
                    value: c.get("value").clone(),
                })
            })
            .collect()
    }

    /// Whether `id` meets it.
    #[must_use]
    pub fn holds(&self, world: &World, id: ObjectId) -> bool {
        let have = property(world, id, &self.ty, self.key);
        match self.op.as_str() {
            "exists" => !have.is_nil(),
            "missing" => have.is_nil(),
            _ if have.is_nil() => false,
            "contains" => match (have.as_str(), self.value.as_str()) {
                (Some(h), Some(w)) => h.to_lowercase().contains(&w.to_lowercase()),
                _ => false,
            },
            op => {
                let ord = match (have.as_f64(), self.value.as_f64()) {
                    (Some(a), Some(b)) => a.partial_cmp(&b),
                    _ => match (&have, &self.value) {
                        (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
                        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
                        _ => None,
                    },
                };
                let Some(o) = ord else {
                    return op == "~=";
                };
                match op {
                    "==" => o.is_eq(),
                    "~=" => o.is_ne(),
                    "<" => o.is_lt(),
                    "<=" => o.is_le(),
                    ">" => o.is_gt(),
                    _ => o.is_ge(),
                }
            }
        }
    }
}

/// Every object class by its number, as loot profiles and plugins name them.
const CLASS_NAMES: [&str; 43] = [
    "Unknown",
    "MeleeWeapon",
    "Armor",
    "Clothing",
    "Jewelry",
    "Monster",
    "Food",
    "Money",
    "Misc",
    "MissileWeapon",
    "Container",
    "Gem",
    "SpellComponent",
    "Key",
    "Portal",
    "TradeNote",
    "ManaStone",
    "Plant",
    "BaseCooking",
    "BaseAlchemy",
    "BaseFletching",
    "CraftedCooking",
    "CraftedAlchemy",
    "CraftedFletching",
    "Player",
    "Vendor",
    "Door",
    "Corpse",
    "Lifestone",
    "HealingKit",
    "Lockpick",
    "WandStaffOrb",
    "Bundle",
    "Book",
    "Journal",
    "Sign",
    "Housing",
    "Npc",
    "Foci",
    "Salvage",
    "Ust",
    "Services",
    "Scroll",
];

/// The name of object class `n`, or `None` for a number no class has.
#[must_use]
pub fn class_name(n: u32) -> Option<&'static str> {
    CLASS_NAMES.get(usize::try_from(n).ok()?).copied()
}
