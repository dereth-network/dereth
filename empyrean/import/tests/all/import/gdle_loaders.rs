//! Vectors: empyrean/fixtures/vectors/content_gdle
//! GDLELoader, LifestonedLoader, their converters and model binders (getters/setters, property
//! visibility, spell descriptions, vital convert) equal ACE vectors via field dumps.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::vectors::{self, VectorFile};
use empyrean_content::gdle::binders::Ability;
use empyrean_content::gdle::lifestoned_enums as names;
use empyrean_content::gdle::models as g;
use empyrean_content::gdle::{lifestoned_loader, loader};
use empyrean_content::import::json::models as m;
use empyrean_content::import::json::world as iw;
use empyrean_content::models::world as w;
use serde_json::{json, Map, Value};

fn load(name: &str) -> VectorFile {
    vectors::load_named("content_gdle", name)
}

// ---- dumps ---------------------------------------------------------------------------------

/// A value as the harness dumps it.
trait Dump {
    fn dump(&self) -> Value;
}

macro_rules! dump_num {
    ($($t:ty),*) => {$(
        impl Dump for $t {
            fn dump(&self) -> Value {
                json!(*self)
            }
        }
    )*};
}
dump_num!(u8, u16, u32, u64, i8, i16, i32, i64, bool);

fn float(d: f64) -> Value {
    if d.is_nan() {
        json!("NaN")
    } else if d == f64::INFINITY {
        json!("Infinity")
    } else if d == f64::NEG_INFINITY {
        json!("-Infinity")
    } else {
        json!(d)
    }
}

impl Dump for f32 {
    fn dump(&self) -> Value {
        float(f64::from(*self))
    }
}

impl Dump for f64 {
    fn dump(&self) -> Value {
        float(*self)
    }
}

impl Dump for String {
    fn dump(&self) -> Value {
        json!(self)
    }
}

impl Dump for &str {
    fn dump(&self) -> Value {
        json!(self)
    }
}

impl Dump for DotNetDateTime {
    fn dump(&self) -> Value {
        json!(self.ticks())
    }
}

impl<T: Dump> Dump for Option<T> {
    fn dump(&self) -> Value {
        self.as_ref().map_or(Value::Null, Dump::dump)
    }
}

impl<T: Dump> Dump for Vec<T> {
    fn dump(&self) -> Value {
        Value::Array(self.iter().map(Dump::dump).collect())
    }
}

impl<T: Dump> Dump for Box<T> {
    fn dump(&self) -> Value {
        (**self).dump()
    }
}

/// A `Result` the way a throwing getter dumps: the value, or `{throws}`.
impl<T: Dump> Dump for Result<T, String> {
    fn dump(&self) -> Value {
        match self {
            Ok(v) => v.dump(),
            Err(e) => json!({ "throws": e }),
        }
    }
}

macro_rules! obj {
    ($($k:literal => $v:expr),* $(,)?) => {{
        let mut o = Map::new();
        $( o.insert($k.to_owned(), Dump::dump(&$v)); )*
        Value::Object(o)
    }};
}

macro_rules! dump_struct {
    ($t:ty, |$s:ident| { $($k:literal => $v:expr),* $(,)? }) => {
        impl Dump for $t {
            fn dump(&self) -> Value {
                let $s = self;
                obj!($($k => $v),*)
            }
        }
    };
}

// GDLE models import-json reads.
dump_struct!(m::Landblock, |s| { "key" => s.key, "value" => s.value, "desc" => s.desc });
dump_struct!(m::LandblockValue, |s| { "links" => s.links, "weenies" => s.weenies });
dump_struct!(m::LandblockWeenie, |s| { "id" => s.id, "wcid" => s.wcid, "desc" => s.desc, "pos" => s.pos });
dump_struct!(m::LandblockLink, |s| { "target" => s.target, "source" => s.source, "desc" => s.desc });
dump_struct!(m::Position, |s| { "frame" => s.frame, "land_cell_id" => s.land_cell_id });
dump_struct!(m::Frame, |s| { "position" => s.position, "rotations" => s.rotations });
dump_struct!(m::Xyz, |s| { "x" => s.x, "y" => s.y, "z" => s.z, "display" => s.display() });
dump_struct!(m::Quaternion, |s| { "w" => s.w, "x" => s.x, "y" => s.y, "z" => s.z, "display" => s.display() });
dump_struct!(m::Quest, |s| { "key" => s.key, "value" => s.value });
dump_struct!(m::QuestValue, |s| { "fullname" => s.fullname, "maxsolves" => s.maxsolves, "mindelta" => s.mindelta });
dump_struct!(m::RecipeCombined, |s| { "key" => s.key, "desc" => s.desc, "recipe" => s.recipe, "precursors" => s.precursors });
dump_struct!(m::RecipePrecursor, |s| { "tool" => s.tool, "target" => s.target, "recipe_id" => s.recipe_id });
dump_struct!(m::Recipe, |s| {
    "recipe_id" => s.recipe_id, "skill" => s.skill, "skill_check_formula_type" => s.skill_check_formula_type,
    "data_id" => s.data_id, "difficulty" => s.difficulty, "success_wcid" => s.success_wcid,
    "success_amount" => s.success_amount, "success_message" => s.success_message,
    "success_consume_target_amount" => s.success_consume_target_amount,
    "success_consume_target_chance" => s.success_consume_target_chance,
    "success_consume_target_message" => s.success_consume_target_message,
    "success_consume_tool_amount" => s.success_consume_tool_amount,
    "success_consume_tool_chance" => s.success_consume_tool_chance,
    "success_consume_tool_message" => s.success_consume_tool_message, "fail_wcid" => s.fail_wcid,
    "fail_amount" => s.fail_amount, "fail_message" => s.fail_message,
    "failure_consume_target_amount" => s.failure_consume_target_amount,
    "failure_consume_target_chance" => s.failure_consume_target_chance,
    "failure_consume_target_message" => s.failure_consume_target_message,
    "failure_consume_tool_amount" => s.failure_consume_tool_amount,
    "failure_consume_tool_chance" => s.failure_consume_tool_chance,
    "failure_consume_tool_message" => s.failure_consume_tool_message, "unknown" => s.unknown,
    "mods" => s.mods, "requirements" => s.requirements,
});
dump_struct!(m::Mod, |s| {
    "int_requirements" => s.lists.int_requirements, "did_requirements" => s.lists.did_requirements,
    "iid_requirements" => s.lists.iid_requirements, "float_requirements" => s.lists.float_requirements,
    "string_requirements" => s.lists.string_requirements, "bool_requirements" => s.lists.bool_requirements,
    "modify_health" => s.modify_health, "modify_stamina" => s.modify_stamina, "modify_mana" => s.modify_mana,
    "requires_health" => s.requires_health, "requires_stamina" => s.requires_stamina,
    "requires_mana" => s.requires_mana, "unknown_7" => s.unknown7,
    "modification_script_id" => s.modification_script_id, "unknown_9" => s.unknown9, "unknown_10" => s.unknown10,
});
dump_struct!(m::RecipeRequirements, |s| {
    "int_requirements" => s.lists.int_requirements, "did_requirements" => s.lists.did_requirements,
    "iid_requirements" => s.lists.iid_requirements, "float_requirements" => s.lists.float_requirements,
    "string_requirements" => s.lists.string_requirements, "bool_requirements" => s.lists.bool_requirements,
});
impl<V: Dump> Dump for m::Requirement<V> {
    fn dump(&self) -> Value {
        obj!("unknown" => self.unknown, "operation_type" => self.operation_type, "message" => self.message,
            "stat" => self.stat, "value" => self.value)
    }
}
dump_struct!(m::StringRequirement, |s| {
    "unknown" => s.unknown, "operation_type" => s.operation_type, "stat" => s.stat, "value" => s.value,
});

// GDLE models only the loaders read.
dump_struct!(g::Event, |s| { "key" => s.key, "value" => s.value });
dump_struct!(g::EventValue, |s| { "start_time" => s.start_time, "end_time" => s.end_time, "event_state" => s.event_state });
dump_struct!(g::Spells, |s| { "table" => s.table });
dump_struct!(g::SpellTable, |s| { "spell_base_hash" => s.spell_base_hash });
dump_struct!(g::SpellBaseHash, |s| { "key" => s.key, "value" => s.value });
dump_struct!(g::SpellValue, |s| {
    "base_mana" => s.base_mana, "base_range_constant" => s.base_range_constant, "base_range_mod" => s.base_range_mod,
    "bitfield" => s.bitfield, "caster_effect" => s.caster_effect, "category" => s.category,
    "component_loss" => s.component_loss, "desc" => s.desc, "display_order" => s.display_order,
    "fizzle_effect" => s.fizzle_effect, "formula" => s.formula, "formula_version" => s.formula_version,
    "icon_id" => s.icon_id, "mana_mod" => s.mana_mod, "meta_spell" => s.meta_spell, "name" => s.name,
    "non_component_target_type" => s.non_component_target_type, "power" => s.power,
    "recovery_amount" => s.recovery_amount, "recovery_interval" => s.recovery_interval, "school" => s.school,
    "spell_economy_mod" => s.spell_economy_mod, "target_effect" => s.target_effect,
});
dump_struct!(g::MetaSpell, |s| { "type" => s.r#type, "spell" => s.spell });
dump_struct!(g::Spell, |s| {
    "degrade_limit" => s.degrade_limit, "degrade_modifier" => s.degrade_modifier, "duration" => s.duration,
    "stat_mod" => s.stat_mod, "spell_category" => s.spell_category, "spell_id" => s.spell_id, "boost" => s.boost,
    "boost_variance" => s.boost_variance, "damage_type" => s.damage_type, "bitfield" => s.bitfield,
    "dest" => s.dest, "loss_percent" => s.loss_percent, "max_boost_allowed" => s.max_boost_allowed,
    "proportion" => s.proportion, "source_loss" => s.source_loss, "source" => s.source,
    "transfer_cap" => s.transfer_cap, "non_tracking" => s.non_tracking, "base_intensity" => s.base_intensity,
    "create_offset" => s.create_offset, "crit_freq" => s.crit_freq, "crit_multiplier" => s.crit_multiplier,
    "default_launch_angle" => s.default_launch_angle, "dims" => s.dims, "elemental_modifier" => s.elemental_modifier,
    "e_type" => s.e_type, "ignore_magic_resist" => s.ignore_magic_resist, "imbued_effect" => s.imbued_effect,
    "num_projectiles" => s.num_projectiles, "num_projectiles_variance" => s.num_projectiles_variance,
    "padding" => s.padding, "peturbation" => s.peturbation, "slayer_creature_type" => s.slayer_creature_type,
    "slayer_damage_bonus" => s.slayer_damage_bonus, "spread_angle" => s.spread_angle, "variance" => s.variance,
    "vertical_angle" => s.vertical_angle, "wcid" => s.wcid, "index" => s.index, "link" => s.link,
    "portal_lifetime" => s.portal_lifetime, "position" => s.position, "align" => s.align,
    "max_power" => s.max_power, "min_power" => s.min_power, "number" => s.number,
    "number_variance" => s.number_variance, "power_variance" => s.power_variance, "school" => s.school,
    "damage_ratio" => s.damage_ratio, "drain_percentage" => s.drain_percentage,
});
dump_struct!(g::StatMod, |s| { "key" => s.key, "type" => s.r#type, "val" => s.val });
dump_struct!(g::CreateOffset, |s| { "x" => s.x, "y" => s.y, "z" => s.z, "w" => s.w });
dump_struct!(g::Region, |s| {
    "encounter_map" => s.encounter_map, "encounters" => s.encounters, "table_count" => s.table_count,
    "table_size" => s.table_size,
});
dump_struct!(g::Encounter, |s| { "key" => s.key, "value" => s.value });
dump_struct!(g::TerrainData, |s| { "key" => s.key, "value" => s.value });
dump_struct!(g::WieldedTreasureTable, |s| { "key" => s.key, "value" => s.value });
dump_struct!(g::WieldedTreasure, |s| {
    "continues_previous_set" => s.continues_previous_set, "set_start" => s.set_start, "has_sub_set" => s.has_sub_set,
    "probability" => s.probability, "stack_size" => s.stack_size, "stack_size_variance" => s.stack_size_variance,
    "palette_id" => s.palette_id, "shade" => s.shade, "unknown_1" => s.unknown1, "unknown_3" => s.unknown3,
    "unknown_4" => s.unknown4, "unknown_5" => s.unknown5, "unknown_9" => s.unknown9, "unknown_10" => s.unknown10,
    "unknown_11" => s.unknown11, "unknown_12" => s.unknown12, "weenie_class_id" => s.weenie_class_id,
});
dump_struct!(g::WorldSpawns, |s| { "version" => s.version, "landblocks" => s.landblocks });

// World rows.
dump_struct!(w::LandblockInstance, |s| {
    "guid" => s.guid, "landblock" => s.landblock, "weenie_class_id" => s.weenie_class_id, "obj_cell_id" => s.obj_cell_id,
    "origin_x" => s.origin_x, "origin_y" => s.origin_y, "origin_z" => s.origin_z, "angles_w" => s.angles_w,
    "angles_x" => s.angles_x, "angles_y" => s.angles_y, "angles_z" => s.angles_z, "is_link_child" => s.is_link_child,
    "last_modified" => s.last_modified, "landblock_instance_link" => s.landblock_instance_link,
});
dump_struct!(w::LandblockInstanceLink, |s| {
    "id" => s.id, "parent_guid" => s.parent_guid, "child_guid" => s.child_guid, "last_modified" => s.last_modified,
});
dump_struct!(w::Event, |s| {
    "name" => s.name, "start_time" => s.start_time, "end_time" => s.end_time, "state" => s.state,
    "last_modified" => s.last_modified,
});
dump_struct!(w::Spell, |s| {
    "id" => s.id, "name" => s.name, "stat_mod_type" => s.stat_mod_type, "stat_mod_key" => s.stat_mod_key,
    "stat_mod_val" => s.stat_mod_val, "e_type" => s.e_type, "base_intensity" => s.base_intensity,
    "variance" => s.variance, "wcid" => s.wcid, "num_projectiles" => s.num_projectiles,
    "num_projectiles_variance" => s.num_projectiles_variance, "spread_angle" => s.spread_angle,
    "vertical_angle" => s.vertical_angle, "default_launch_angle" => s.default_launch_angle,
    "non_tracking" => s.non_tracking, "create_offset_origin_x" => s.create_offset_origin_x,
    "create_offset_origin_y" => s.create_offset_origin_y, "create_offset_origin_z" => s.create_offset_origin_z,
    "padding_origin_x" => s.padding_origin_x, "padding_origin_y" => s.padding_origin_y,
    "padding_origin_z" => s.padding_origin_z, "dims_origin_x" => s.dims_origin_x, "dims_origin_y" => s.dims_origin_y,
    "dims_origin_z" => s.dims_origin_z, "peturbation_origin_x" => s.peturbation_origin_x,
    "peturbation_origin_y" => s.peturbation_origin_y, "peturbation_origin_z" => s.peturbation_origin_z,
    "imbued_effect" => s.imbued_effect, "slayer_creature_type" => s.slayer_creature_type,
    "slayer_damage_bonus" => s.slayer_damage_bonus, "crit_freq" => s.crit_freq, "crit_multiplier" => s.crit_multiplier,
    "ignore_magic_resist" => s.ignore_magic_resist, "elemental_modifier" => s.elemental_modifier,
    "drain_percentage" => s.drain_percentage, "damage_ratio" => s.damage_ratio, "damage_type" => s.damage_type,
    "boost" => s.boost, "boost_variance" => s.boost_variance, "source" => s.source, "destination" => s.destination,
    "proportion" => s.proportion, "loss_percent" => s.loss_percent, "source_loss" => s.source_loss,
    "transfer_cap" => s.transfer_cap, "max_boost_allowed" => s.max_boost_allowed,
    "transfer_bitfield" => s.transfer_bitfield, "index" => s.index, "link" => s.link,
    "position_obj_cell_id" => s.position_obj_cell_id, "position_origin_x" => s.position_origin_x,
    "position_origin_y" => s.position_origin_y, "position_origin_z" => s.position_origin_z,
    "position_angles_w" => s.position_angles_w, "position_angles_x" => s.position_angles_x,
    "position_angles_y" => s.position_angles_y, "position_angles_z" => s.position_angles_z,
    "min_power" => s.min_power, "max_power" => s.max_power, "power_variance" => s.power_variance,
    "dispel_school" => s.dispel_school, "align" => s.align, "number" => s.number,
    "number_variance" => s.number_variance, "dot_duration" => s.dot_duration, "last_modified" => s.last_modified,
});
dump_struct!(w::TreasureWielded, |s| {
    "treasure_type" => s.treasure_type, "weenie_class_id" => s.weenie_class_id, "palette_id" => s.palette_id,
    "unknown_1" => s.unknown_1, "shade" => s.shade, "stack_size" => s.stack_size,
    "stack_size_variance" => s.stack_size_variance, "probability" => s.probability, "unknown_3" => s.unknown_3,
    "unknown_4" => s.unknown_4, "unknown_5" => s.unknown_5, "set_start" => s.set_start, "has_sub_set" => s.has_sub_set,
    "continues_previous_set" => s.continues_previous_set, "unknown_9" => s.unknown_9, "unknown_10" => s.unknown_10,
    "unknown_11" => s.unknown_11, "unknown_12" => s.unknown_12, "last_modified" => s.last_modified,
});

// World rows as the JSON converters fill them.
dump_struct!(iw::Quest, |s| {
    "name" => s.name, "min_delta" => s.min_delta, "max_solves" => s.max_solves, "message" => s.message,
    "last_modified" => s.last_modified,
});
dump_struct!(iw::CookBook, |s| {
    "recipe_id" => s.recipe_id, "source_wcid" => s.source_wcid, "target_wcid" => s.target_wcid,
    "last_modified" => s.last_modified,
});
/// A requirement (`message`) or mod (`source`) row; `last` names the table's last column.
fn recipe_row<V: Dump>(r: &iw::RecipeRow<V>, last: &str) -> Value {
    let mut o = obj!("index" => r.index, "stat" => r.stat, "value" => r.value, "enum" => r.r#enum);
    let last_value = if last == "message" {
        r.message.dump()
    } else {
        r.source.dump()
    };
    o.as_object_mut()
        .unwrap()
        .insert(last.to_owned(), last_value);
    o
}
fn rows<V: Dump>(v: &[iw::RecipeRow<V>], last: &str) -> Value {
    Value::Array(v.iter().map(|r| recipe_row(r, last)).collect())
}
impl Dump for iw::Recipe {
    fn dump(&self) -> Value {
        let s = self;
        let mut o = obj!(
            "id" => s.id, "unknown_1" => s.unknown_1, "skill" => s.skill, "difficulty" => s.difficulty,
            "salvage_type" => s.salvage_type, "success_wcid" => s.success_wcid, "success_amount" => s.success_amount,
            "success_message" => s.success_message, "fail_wcid" => s.fail_wcid, "fail_amount" => s.fail_amount,
            "fail_message" => s.fail_message, "success_destroy_source_chance" => s.success_destroy_source_chance,
            "success_destroy_source_amount" => s.success_destroy_source_amount,
            "success_destroy_source_message" => s.success_destroy_source_message,
            "success_destroy_target_chance" => s.success_destroy_target_chance,
            "success_destroy_target_amount" => s.success_destroy_target_amount,
            "success_destroy_target_message" => s.success_destroy_target_message,
            "fail_destroy_source_chance" => s.fail_destroy_source_chance,
            "fail_destroy_source_amount" => s.fail_destroy_source_amount,
            "fail_destroy_source_message" => s.fail_destroy_source_message,
            "fail_destroy_target_chance" => s.fail_destroy_target_chance,
            "fail_destroy_target_amount" => s.fail_destroy_target_amount,
            "fail_destroy_target_message" => s.fail_destroy_target_message, "data_id" => s.data_id,
            "last_modified" => s.last_modified,
        );
        let map = o.as_object_mut().unwrap();
        map.insert(
            "recipe_requirements_int".into(),
            rows(&s.requirements_int, "message"),
        );
        map.insert(
            "recipe_requirements_did".into(),
            rows(&s.requirements_did, "message"),
        );
        map.insert(
            "recipe_requirements_iid".into(),
            rows(&s.requirements_iid, "message"),
        );
        map.insert(
            "recipe_requirements_float".into(),
            rows(&s.requirements_float, "message"),
        );
        map.insert(
            "recipe_requirements_string".into(),
            rows(&s.requirements_string, "message"),
        );
        map.insert(
            "recipe_requirements_bool".into(),
            rows(&s.requirements_bool, "message"),
        );
        let mods = s
            .recipe_mod
            .iter()
            .map(|r| {
                let mut o = obj!(
                    "executes_on_success" => r.executes_on_success, "health" => r.health, "stamina" => r.stamina,
                    "mana" => r.mana, "unknown_7" => r.unknown_7, "data_id" => r.data_id, "unknown_9" => r.unknown_9,
                    "instance_id" => r.instance_id,
                );
                let map = o.as_object_mut().unwrap();
                map.insert("recipe_mods_int".into(), rows(&r.mods_int, "source"));
                map.insert("recipe_mods_did".into(), rows(&r.mods_did, "source"));
                map.insert("recipe_mods_iid".into(), rows(&r.mods_iid, "source"));
                map.insert("recipe_mods_float".into(), rows(&r.mods_float, "source"));
                map.insert("recipe_mods_string".into(), rows(&r.mods_string, "source"));
                map.insert("recipe_mods_bool".into(), rows(&r.mods_bool, "source"));
                o
            })
            .collect();
        map.insert("recipe_mod".into(), Value::Array(mods));
        o
    }
}
impl<T: Dump> Dump for iw::Prop<T> {
    fn dump(&self) -> Value {
        obj!("type" => self.r#type, "value" => self.value)
    }
}
dump_struct!(iw::EmoteAction, |s| {
    "order" => s.order, "type" => s.r#type, "delay" => s.delay, "extent" => s.extent, "motion" => s.motion,
    "message" => s.message, "p_script" => s.p_script, "sound" => s.sound,
});
dump_struct!(iw::Emote, |s| {
    "category" => s.category, "probability" => s.probability, "weenie_class_id" => s.weenie_class_id,
    "style" => s.style, "substyle" => s.substyle, "weenie_properties_emote_action" => s.actions,
});
dump_struct!(iw::Weenie, |s| {
    "class_id" => s.class_id, "class_name" => s.class_name, "type" => s.r#type, "last_modified" => s.last_modified,
    "weenie_properties_int" => s.int, "weenie_properties_string" => s.string, "weenie_properties_did" => s.did,
    "weenie_properties_emote" => s.emote,
});

// Lifestoned weenies (the members the loaders' cases set).
fn stats<T: Dump>(v: &Option<Vec<Option<T>>>) -> Value {
    v.dump()
}
dump_struct!(m::IntStat, |s| { "key" => s.key, "value" => s.value });
dump_struct!(m::DidStat, |s| { "key" => s.key, "value" => s.value });
dump_struct!(m::StringStat, |s| { "key" => s.key, "value" => s.value });
impl Dump for m::LsdWeenie {
    fn dump(&self) -> Value {
        obj!("weenie_id" => self.weenie_id, "weenie_type_id" => self.weenie_type_id, "name" => self.name(),
            "int_stats" => RawValue(stats(&self.int_stats)), "did_stats" => RawValue(stats(&self.did_stats)),
            "string_stats" => RawValue(stats(&self.string_stats)))
    }
}

/// An already dumped value.
struct RawValue(Value);
impl Dump for RawValue {
    fn dump(&self) -> Value {
        self.0.clone()
    }
}

// ---- comparison ----------------------------------------------------------------------------

/// Every value of `ours` must be in `ace` (objects by key, arrays element by element).
fn same(at: &str, ours: &Value, ace: &Value, failures: &mut Vec<String>) {
    match (ours, ace) {
        (Value::Object(o), Value::Object(a)) => {
            for (k, v) in o {
                match a.get(k) {
                    Some(av) => same(&format!("{at}.{k}"), v, av, failures),
                    None => failures.push(format!("{at}.{k}: not in ACE's dump")),
                }
            }
        }
        (Value::Array(o), Value::Array(a)) => {
            if o.len() == a.len() {
                for (i, (v, av)) in o.iter().zip(a).enumerate() {
                    same(&format!("{at}[{i}]"), v, av, failures);
                }
            } else {
                failures.push(format!("{at}: {} elements, ACE {}", o.len(), a.len()));
            }
        }
        (Value::Number(o), Value::Number(a)) => {
            let equal = match (o.as_i64(), a.as_i64(), o.as_u64(), a.as_u64()) {
                (Some(x), Some(y), _, _) => x == y,
                (_, _, Some(x), Some(y)) => x == y,
                _ => vectors::same_f64(o.as_f64().unwrap(), a.as_f64().unwrap()),
            };
            if !equal {
                failures.push(format!("{at}: ours {ours}, ACE {ace}"));
            }
        }
        (o, a) if o.is_number() || a.is_number() => {
            let (x, y) = (vectors::f64_of(o), vectors::f64_of(a));
            if !matches!((x, y), (Some(x), Some(y)) if vectors::same_f64(x, y)) {
                failures.push(format!("{at}: ours {ours}, ACE {ace}"));
            }
        }
        _ => {
            if ours != ace {
                failures.push(format!("{at}: ours {ours}, ACE {ace}"));
            }
        }
    }
}

fn finish(what: &str, failures: &[String], checked: usize) {
    assert!(
        failures.is_empty(),
        "{what}: {} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 0, "{what}: only {checked} cases ran");
}

// ---- files ---------------------------------------------------------------------------------

/// A fresh folder with the case's files: `[path, text]` or `[path, text, creation ticks]`.
fn case_folder(tag: &str, files: &Value) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("serv-content-8b3-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files.as_array().unwrap() {
        let path = dir.join(f[0].as_str().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // File.WriteAllText writes UTF-8 without a byte order mark (a U+FEFF in the text stays).
        std::fs::write(&path, f[1].as_str().unwrap()).unwrap();
        if let Some(ticks) = f.get(2).and_then(Value::as_i64) {
            set_created(&path, ticks);
        }
    }
    dir
}

/// .NET ticks (100 ns since 0001-01-01) as a `SystemTime`.
fn ticks_to_time(ticks: i64) -> SystemTime {
    const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    let since = u64::try_from(ticks - UNIX_EPOCH_TICKS).unwrap();
    SystemTime::UNIX_EPOCH + std::time::Duration::from_nanos(since * 100)
}

/// `File.SetCreationTimeUtc`; only Windows lets a program set it.
#[cfg(windows)]
fn set_created(path: &Path, ticks: i64) {
    use std::os::windows::fs::FileTimesExt;
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_times(std::fs::FileTimes::new().set_created(ticks_to_time(ticks)))
        .unwrap();
}

#[cfg(not(windows))]
fn set_created(_path: &Path, _ticks: i64) {}

/// The creation times the harness gave the case's files, by path under `dir`, for the loaders'
/// creation-time seam on hosts where [`set_created`] cannot set them.
fn case_times(dir: &Path, files: &Value) -> impl Fn(&Path) -> SystemTime {
    let times: Vec<(Vec<String>, SystemTime)> = files
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let rel = f[0]
                .as_str()
                .unwrap()
                .split('/')
                .map(str::to_owned)
                .collect();
            (
                rel,
                f.get(2)
                    .and_then(Value::as_i64)
                    .map_or(SystemTime::UNIX_EPOCH, ticks_to_time),
            )
        })
        .collect();
    let dir = dir.to_path_buf();
    move |path: &Path| {
        let rel: Vec<String> = path
            .strip_prefix(&dir)
            .unwrap_or_else(|_| panic!("{} is not under the case folder", path.display()))
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        times.iter().find(|(r, _)| *r == rel).map_or_else(
            || panic!("{}: not one of the case's files", path.display()),
            |(_, t)| *t,
        )
    }
}

fn out<T: Dump>(r: Result<T, String>) -> Value {
    match r {
        Ok(v) => json!({ "ok": true, "result": v.dump() }),
        Err(_) => json!({ "ok": false, "result": null }),
    }
}

fn sorted_by<T: Clone, K: Ord>(v: &[T], key: impl Fn(&T) -> K) -> Vec<T> {
    let mut v = v.to_vec();
    v.sort_by_key(key);
    v
}

fn pair<A: Dump, B: Dump>(a: &A, b: &B) -> Value {
    Value::Array(vec![a.dump(), b.dump()])
}

/// Run one `GDLELoader` case, sorted as the harness sorts it.
fn run_gdle(loader_name: &str, dir: &Path, arg: &Value) -> Value {
    let file = dir.join("input.json");
    let offset = || u16::try_from(arg.as_u64().unwrap()).unwrap();
    match loader_name {
        "TryLoadLandblock" => out(loader::try_load_landblock(&file)),
        "TryLoadLandblocksInParallel" => out(loader::try_load_landblocks_in_parallel(dir)
            .map(|v| sorted_by(&v, |l| l.as_ref().map(|l| l.key)))),
        "TryLoadLandblocksConverted" => out(loader::try_load_landblocks_converted(dir, offset())
            .map(|(r, l)| {
                RawValue(pair(
                    &sorted_by(&r, |i| i.guid),
                    &sorted_by(&l, |l| (l.parent_guid, l.child_guid)),
                ))
            })),
        "TryLoadWorldSpawns" => out(loader::try_load_world_spawns(&file)),
        "TryLoadWorldSpawnsConverted" => {
            out(loader::try_load_world_spawns_converted(&file, offset())
                .map(|(r, l)| RawValue(pair(&r, &l))))
        }
        "TryLoadEvents" => out(loader::try_load_events(&file)),
        "TryLoadEventsConverted" => out(loader::try_load_events_converted(&file)),
        "TryLoadQuest" => out(loader::try_load_quest(&file)),
        "TryLoadQuests" => out(loader::try_load_quests(&file)),
        "TryLoadQuestsConverted" => out(loader::try_load_quests_converted(&file)),
        "TryLoadSpells" => out(loader::try_load_spells(&file)),
        "TryLoadSpellsConverted" => out(loader::try_load_spells_converted(&file)),
        "TryLoadRecipe" => out(loader::try_load_recipe(&file)),
        "TryLoadRecipeCombined" => out(loader::try_load_recipe_combined(&file)),
        "TryLoadRecipeCombinedInParallel" => out(loader::try_load_recipe_combined_in_parallel(dir)
            .map(|v| sorted_by(&v, |c| c.as_ref().map(|c| c.key)))),
        "TryLoadRecipeCombinedConverted" => out(loader::try_load_recipe_combined_converted(dir)
            .map(|(r, c)| {
                RawValue(pair(
                    &sorted_by(&r, |r| r.id),
                    &sorted_by(&c, |c| (c.recipe_id, c.source_wcid)),
                ))
            })),
        "TryLoadRecipesConverted" => out(loader::try_load_recipes_converted(&file)),
        "TryLoadRecipePrecursors" => out(loader::try_load_recipe_precursors(&file)),
        "TryLoadRecipePrecursorsConverted" => {
            out(loader::try_load_recipe_precursors_converted(&file))
        }
        "TryLoadRegion" => out(loader::try_load_region(&file)),
        "TryLoadTerrainData" => out(loader::try_load_terrain_data(&file)),
        "TryLoadWieldedTreasureTable" => out(loader::try_load_wielded_treasure_table(&file)),
        "TryLoadWieldedTreasureTableConverted" => {
            out(loader::try_load_wielded_treasure_table_converted(&file))
        }
        other => panic!("no loader {other}"),
    }
}

#[test]
fn gdle_loader_matches_ace() {
    let file = load("gdle_loader");
    let mut failures = Vec::new();
    for (i, case) in file.cases.iter().enumerate() {
        let name = case.input["name"].as_str().unwrap();
        let loader_name = case.input["loader"].as_str().unwrap();
        let dir = case_folder(&format!("gdle{i}"), &case.input["files"]);
        let ours = match name {
            // ACE's call is on a path that does not exist.
            "missing_file" => out(loader::try_load_quest(&dir.join("input.json.missing"))),
            "landblocks_in_parallel_missing" => out(loader::try_load_landblocks_in_parallel(
                &dir.join("missing"),
            )),
            _ => run_gdle(
                loader_name,
                &dir,
                case.input.get("arg").unwrap_or(&Value::Null),
            ),
        };
        same(name, &ours, &case.output, &mut failures);
        let _ = std::fs::remove_dir_all(&dir);
    }
    finish("gdle_loader", &failures, file.cases.len());
}

#[test]
fn lifestoned_loader_matches_ace() {
    let file = load("lifestoned_loader");
    let mut failures = Vec::new();
    for (i, case) in file.cases.iter().enumerate() {
        let name = case.input["name"].as_str().unwrap();
        let loader_name = case.input["loader"].as_str().unwrap();
        let dir = case_folder(&format!("lsd{i}"), &case.input["files"]);
        // Newest-first ordering needs the creation times the harness set. Only Windows lets a
        // program set them (`case_folder` does, and the loaders read them there too); on every
        // host the loaders' seam is also given them.
        let times = case_times(&dir, &case.input["files"]);
        let file1 = dir.join("input.json");
        let shift = case
            .input
            .get("arg")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ours = match (name, loader_name) {
            ("weenies_missing_folder", _) => {
                out(lifestoned_loader::try_load_weenies(&dir.join("missing")))
            }
            (_, "TryLoadWeenie") => out(lifestoned_loader::try_load_weenie(&file1)),
            (_, "TryLoadWeenies") => {
                if cfg!(windows) {
                    let real = out(lifestoned_loader::try_load_weenies(&dir));
                    same(
                        &format!("{name} (file system times)"),
                        &real,
                        &case.output,
                        &mut failures,
                    );
                }
                out(lifestoned_loader::try_load_weenies_by(&dir, &times))
            }
            (_, "TryLoadWeeniesInParallel") => {
                out(lifestoned_loader::try_load_weenies_in_parallel(&dir)
                    .map(|v| sorted_by(&v, |w| w.as_ref().map(|w| w.weenie_id))))
            }
            (_, "TryLoadWeenieConverted") => {
                out(lifestoned_loader::try_load_weenie_converted(&file1, shift))
            }
            (_, "TryLoadWeeniesConverted") => {
                if cfg!(windows) {
                    let real = out(lifestoned_loader::try_load_weenies_converted(&dir, shift));
                    same(
                        &format!("{name} (file system times)"),
                        &real,
                        &case.output,
                        &mut failures,
                    );
                }
                out(lifestoned_loader::try_load_weenies_converted_by(
                    &dir, shift, &times,
                ))
            }
            (_, "TryLoadWeeniesConvertedInParallel") => out(
                lifestoned_loader::try_load_weenies_converted_in_parallel(&dir, shift)
                    .map(|v| sorted_by(&v, |w| w.class_id)),
            ),
            (_, other) => panic!("no loader {other}"),
        };
        same(name, &ours, &case.output, &mut failures);
        let _ = std::fs::remove_dir_all(&dir);
    }
    finish("lifestoned_loader", &failures, file.cases.len());
}

// ---- enum names ----------------------------------------------------------------------------

/// The name of `value` through the binder that names it, as a dump (a string or `{throws}`).
fn binder_name(e: &str, value: Option<i32>) -> Option<Value> {
    use empyrean_content::import::json::models::*;
    let v = value.unwrap_or(0);
    Some(match e {
        "BoolPropertyId" => BoolStat { key: v, value: 0 }.property_id_binder().dump(),
        "DidPropertyId" => DidStat { key: v, value: 0 }.property_id_binder().dump(),
        "DoublePropertyId" => FloatStat { key: v, value: 0.0 }.property_id_binder().dump(),
        "IidPropertyId" => IidStat { key: v, value: 0 }.property_id_binder().dump(),
        "Int64PropertyId" => Int64Stat { key: v, value: 0 }.property_id_binder().dump(),
        "StringPropertyId" => StringStat {
            key: v,
            value: None,
        }
        .property_id_binder()
        .dump(),
        "IntPropertyId" => IntStat {
            key: v,
            ..IntStat::default()
        }
        .property_id_binder()
        .dump(),
        "SkillId" => SkillListing {
            skill_id: value,
            skill: None,
        }
        .skill_name()
        .dump(),
        "PositionType" => PositionListing {
            position_type: v,
            position: None,
        }
        .position_type_name()
        .dump(),
        _ => return None,
    })
}

#[test]
fn lifestoned_enum_names_match_ace() {
    let file = load("lifestoned_enums");
    let mut failures = Vec::new();
    let mut checked = 0;
    for case in &file.cases {
        let e = case.input["enum"].as_str().unwrap();
        let v = case.input["value"]
            .as_i64()
            .map(|v| i32::try_from(v).unwrap());
        let want = &case.output;
        if let Some(ours) = binder_name(e, v) {
            checked += 1;
            if ours != want["get_name"] {
                failures.push(format!(
                    "{e} {v:?}: GetName ours {ours}, ACE {}",
                    want["get_name"]
                ));
            }
        } else if e == "SpellId" {
            checked += 1;
            let v = v.unwrap();
            let ours = names::SPELL_ID
                .iter()
                .find(|(k, _)| *k == v)
                .map_or_else(|| v.to_string(), |(_, n)| (*n).to_owned());
            if Some(ours.as_str()) != want["to_string"].as_str() {
                failures.push(format!(
                    "SpellId {v}: ours {ours}, ACE {}",
                    want["to_string"]
                ));
            }
        }
    }
    finish("lifestoned_enums", &failures, checked);
}

// ---- binders -------------------------------------------------------------------------------

fn parse<T>(
    json: &str,
    read: impl Fn(&empyrean_content::import::json::value::Json) -> Result<Option<T>, String>,
) -> T {
    let doc = empyrean_content::import::json::value::parse(json).unwrap();
    read(&doc).unwrap().unwrap()
}

/// The binder getters of one model, as the harness names them.
fn getters(type_name: &str, json: &str) -> Value {
    use empyrean_content::import::json::models::*;
    match type_name {
        "BodyPartListing" => {
            let x = parse(json, |d| BodyPartListing::read(d, "x"));
            obj!("BodyPartType" => x.body_part_type())
        }
        "BoolStat" => {
            let x = parse(json, |d| BoolStat::read(d, "x"));
            obj!("BoolValue" => x.bool_value(), "PropertyIdBinder" => x.property_id_binder())
        }
        "DidStat" => {
            obj!("PropertyIdBinder" => parse(json, |d| DidStat::read(d, "x")).property_id_binder())
        }
        "FloatStat" => {
            obj!("PropertyIdBinder" => parse(json, |d| FloatStat::read(d, "x")).property_id_binder())
        }
        "IidStat" => {
            obj!("PropertyIdBinder" => parse(json, |d| IidStat::read(d, "x")).property_id_binder())
        }
        "Int64Stat" => {
            obj!("PropertyIdBinder" => parse(json, |d| Int64Stat::read(d, "x")).property_id_binder())
        }
        "StringStat" => {
            obj!("PropertyIdBinder" => parse(json, |d| StringStat::read(d, "x")).property_id_binder())
        }
        "IntStat" => {
            let x = parse(json, |d| IntStat::read(d, "x"));
            obj!(
                "MultiSelect" => x.multi_select(), "PropertyIdBinder" => x.property_id_binder(),
                "ItemTypeBoundValue" => x.item_type_bound_value(), "WeenieTypeBoundValue" => x.weenie_type_bound_value(),
                "CreatureTypeBoundValue" => x.creature_type_bound_value(), "ArmorTypeBoundValue" => x.armor_type_bound_value(),
                "WieldRequirementsBoundValue" => x.wield_requirements_bound_value(),
                "PaletteTemplateBoundValue" => x.palette_template_bound_value(), "Material_Binder" => x.material_binder(),
                "HeritageBinder" => x.heritage_binder(), "WeaponTypeBoundValue" => x.weapon_type_bound_value(),
                "SkillIdBoundValue" => x.skill_id_bound_value(),
            )
        }
        "CreateItem" => {
            let x = parse(json, |d| CreateItem::read(d, "x"));
            obj!("Destination_Binder" => x.destination_binder(), "TryToBond_BooleanBinder" => x.try_to_bond_boolean_binder())
        }
        "GeneratorTable" => {
            let x = parse(json, |d| GeneratorTable::read(d, "x"));
            obj!("WhenCreateEnum" => x.when_create_enum(), "WhereCreateEnum" => x.where_create_enum())
        }
        "PositionListing" => {
            obj!("PositionTypeName" => parse(json, |d| PositionListing::read(d, "x")).position_type_name())
        }
        "Skill" => obj!("Status_Binder" => parse(json, |d| Skill::read(d, "x")).status_binder()),
        "SkillListing" => {
            obj!("SkillName" => parse(json, |d| SkillListing::read(d, "x")).skill_name())
        }
        "XYZ" => obj!("Display" => parse(json, |d| Xyz::read(d, "x")).display()),
        "Quaternion" => obj!("Display" => parse(json, |d| Quaternion::read(d, "x")).display()),
        "Emote" => obj!("EmoteCategory" => parse(json, |d| Emote::read(d, "x")).emote_category()),
        "EmoteAction" => {
            let x = parse(json, |d| EmoteAction::read(d, "x"));
            obj!(
                "EmoteActionType_Binder" => x.emote_action_type_binder(), "Motion_Binder" => x.motion_binder(),
                "PScript_Binder" => x.p_script_binder(), "WealthRating_Binder" => x.wealth_rating_binder(),
                "TreasureClass_Binder" => x.treasure_class_binder(),
            )
        }
        "LSDWeenie" => {
            let x = parse(json, LsdWeenie::read);
            obj!(
                "WeenieClassId" => x.weenie_class_id(), "WeenieType_Binder" => x.weenie_type_binder(),
                "ItemType" => x.item_type(), "HasAbilities" => x.has_abilities(),
                "HasGeneratorTable" => x.has_generator_table(), "HasBodyPartList" => x.has_body_part_list(),
                "UIEffects" => x.ui_effects(), "IconId" => x.icon_id(), "UnderlayId" => x.underlay_id(),
                "OverlayId" => x.overlay_id(), "OverlaySecondaryId" => x.overlay_secondary_id(),
            )
        }
        other => panic!("no getters for {other}"),
    }
}

#[test]
fn binder_getters_match_ace() {
    let file = load("getters");
    let mut failures = Vec::new();
    for case in &file.cases {
        let t = case.input["type"].as_str().unwrap();
        let json = case.input["json"].as_str().unwrap();
        let ours = getters(t, json);
        same(&format!("{t} {json}"), &ours, &case.output, &mut failures);
        // Every getter ACE recorded is checked.
        let n = case.output.as_object().unwrap().len();
        if ours.as_object().unwrap().len() != n {
            failures.push(format!("{t}: {n} getters in ACE's case"));
        }
    }
    finish("getters", &failures, file.cases.len());
}

fn i32_arg(v: &Value) -> Option<i32> {
    v.as_i64().map(|x| i32::try_from(x).unwrap())
}

fn u32_arg(v: &Value) -> Option<u32> {
    v.as_i64().map(|x| u32::try_from(x).unwrap())
}

/// The model after one setter, as `{model}` or `{throws, model}`.
#[allow(clippy::too_many_lines)]
fn setter(type_name: &str, json: &str, property: &str, value: &Value) -> Value {
    use empyrean_content::import::json::models::*;
    fn result<T>(r: Result<(), String>, model: Value) -> Value {
        let _ = std::marker::PhantomData::<T>;
        match r {
            Ok(()) => json!({ "model": model }),
            Err(e) => json!({ "throws": e, "model": model }),
        }
    }
    match type_name {
        "IntStat" => {
            let mut x = parse(json, |d| IntStat::read(d, "x"));
            let r = match property {
                "MultiSelect" => x.set_multi_select(
                    value
                        .as_array()
                        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect()),
                ),
                "ItemTypeBoundValue" => x.set_item_type_bound_value(u32_arg(value)),
                "WeenieTypeBoundValue" => x.set_weenie_type_bound_value(u32_arg(value)),
                "Material_Binder" => x.set_material_binder(u32_arg(value)),
                "CreatureTypeBoundValue" => x.set_creature_type_bound_value(i32_arg(value)),
                "ArmorTypeBoundValue" => x.set_armor_type_bound_value(i32_arg(value)),
                "WieldRequirementsBoundValue" => {
                    x.set_wield_requirements_bound_value(i32_arg(value))
                }
                "PaletteTemplateBoundValue" => x.set_palette_template_bound_value(i32_arg(value)),
                "HeritageBinder" => x.set_heritage_binder(i32_arg(value)),
                "WeaponTypeBoundValue" => x.set_weapon_type_bound_value(i32_arg(value)),
                "SkillIdBoundValue" => x.set_skill_id_bound_value(i32_arg(value)),
                other => panic!("IntStat.{other}"),
            };
            result::<()>(
                r,
                obj!("key" => x.key, "value" => x.value, "multi_select_raw" => x.multi_select_raw),
            )
        }
        "BoolStat" => {
            let mut x = parse(json, |d| BoolStat::read(d, "x"));
            x.set_bool_value(value.as_bool().unwrap());
            result::<()>(Ok(()), obj!("key" => x.key, "value" => x.value))
        }
        "BodyPartListing" => {
            let mut x = parse(json, |d| BodyPartListing::read(d, "x"));
            x.set_body_part_type(i32_arg(value).unwrap());
            result::<()>(Ok(()), obj!("key" => x.key))
        }
        "CreateItem" => {
            let mut x = parse(json, |d| CreateItem::read(d, "x"));
            match property {
                "Destination_Binder" => x.set_destination_binder(i32_arg(value)),
                "TryToBond_BooleanBinder" => {
                    x.set_try_to_bond_boolean_binder(value.as_bool().unwrap())
                }
                other => panic!("CreateItem.{other}"),
            }
            result::<()>(
                Ok(()),
                obj!("destination" => x.destination, "try_to_bond" => x.try_to_bond),
            )
        }
        "GeneratorTable" => {
            let mut x = parse(json, |d| GeneratorTable::read(d, "x"));
            match property {
                "WhenCreateEnum" => x.set_when_create_enum(i32_arg(value).unwrap()),
                "WhereCreateEnum" => x.set_where_create_enum(i32_arg(value).unwrap()),
                other => panic!("GeneratorTable.{other}"),
            }
            result::<()>(
                Ok(()),
                obj!("when_create" => x.when_create, "where_create" => x.where_create),
            )
        }
        "Skill" => {
            let mut x = parse(json, |d| Skill::read(d, "x"));
            x.set_status_binder(i32_arg(value));
            result::<()>(Ok(()), obj!("trained_level" => x.trained_level))
        }
        "Emote" => {
            let mut x = parse(json, |d| Emote::read(d, "x"));
            x.set_emote_category(i32_arg(value).unwrap());
            result::<()>(Ok(()), obj!("category" => x.category))
        }
        "EmoteAction" => {
            let mut x = parse(json, |d| EmoteAction::read(d, "x"));
            match property {
                "EmoteActionType_Binder" => x.set_emote_action_type_binder(i32_arg(value).unwrap()),
                "Motion_Binder" => x.set_motion_binder(u32_arg(value)),
                "PScript_Binder" => x.set_p_script_binder(i32_arg(value)),
                "WealthRating_Binder" => x.set_wealth_rating_binder(i32_arg(value)),
                "TreasureClass_Binder" => x.set_treasure_class_binder(i32_arg(value)),
                other => panic!("EmoteAction.{other}"),
            }
            result::<()>(
                Ok(()),
                obj!("emote_action_type" => x.emote_action_type, "motion" => x.motion, "p_script" => x.p_script,
                    "wealth_rating" => x.wealth_rating, "treasure_class" => x.treasure_class),
            )
        }
        "LSDWeenie" => {
            let mut x = parse(json, LsdWeenie::read);
            match property {
                "WeenieClassId" => x.set_weenie_class_id(u32_arg(value).unwrap()),
                "WeenieType_Binder" => x.set_weenie_type_binder(u32_arg(value).unwrap()),
                other => panic!("LSDWeenie.{other}"),
            }
            result::<()>(
                Ok(()),
                obj!("weenie_id" => x.weenie_id, "weenie_type_id" => x.weenie_type_id),
            )
        }
        "XYZ" => {
            let mut x = parse(json, |d| Xyz::read(d, "x"));
            let r = x.set_display(value.as_str());
            result::<()>(r, x.dump())
        }
        "Quaternion" => {
            let mut x = parse(json, |d| Quaternion::read(d, "x"));
            let r = x.set_display(value.as_str());
            result::<()>(r, x.dump())
        }
        other => panic!("no setters for {other}"),
    }
}

#[test]
fn binder_setters_match_ace() {
    let file = load("setters");
    let mut failures = Vec::new();
    for case in &file.cases {
        let t = case.input["type"].as_str().unwrap();
        let json = case.input["json"].as_str().unwrap();
        let p = case.input["property"].as_str().unwrap();
        let v = &case.input["value"];
        let ours = setter(t, json, p, v);
        let at = format!("{t}.{p} = {v} on {json}");
        same(&at, &ours, &case.output, &mut failures);
        if case.output.get("throws").is_some() != ours.get("throws").is_some() {
            failures.push(format!("{at}: ours {ours}, ACE {}", case.output));
        }
    }
    finish("setters", &failures, file.cases.len());
}

#[test]
fn is_property_visible_matches_ace() {
    use empyrean_content::import::json::models::{Emote, EmoteAction};
    let file = load("is_property_visible");
    let mut failures = Vec::new();
    for case in &file.cases {
        let t = case.input["type"].as_str().unwrap();
        let p = case.input["property"].as_str().unwrap();
        let mut visible = Vec::new();
        let mut thrown = None;
        for v in -1i64..=200 {
            let bits = u32::from_ne_bytes(i32::try_from(v).unwrap().to_ne_bytes());
            let r = if t == "Emote" {
                Emote::is_property_visible(
                    p,
                    &Emote {
                        category: bits,
                        ..Emote::default()
                    },
                )
            } else {
                EmoteAction::is_property_visible(
                    p,
                    &EmoteAction {
                        emote_action_type: bits,
                        ..EmoteAction::default()
                    },
                )
            };
            match r {
                Ok(true) => visible.push(v),
                Ok(false) => {}
                Err(e) => {
                    thrown = Some(e);
                    break;
                }
            }
        }
        let ours = thrown.map_or_else(
            || json!({ "visible_for": visible }),
            |e| json!({ "throws": e }),
        );
        if ours != case.output {
            failures.push(format!("{t}.{p}: ours {ours}, ACE {}", case.output));
        }
    }
    finish("is_property_visible", &failures, file.cases.len());
}

#[test]
fn spell_descriptions_match_ace() {
    let file = load("spell_description");
    let mut failures = Vec::new();
    for case in &file.cases {
        let id = i32::try_from(case.input["spell_id"].as_i64().unwrap()).unwrap();
        let ours = empyrean_content::import::json::models::SpellbookEntry {
            spell_id: id,
            ..Default::default()
        }
        .get_spell_description();
        if Some(ours.as_str()) != case.output.as_str() {
            failures.push(format!("{id}: ours {ours:?}, ACE {}", case.output));
        }
    }
    finish("spell_description", &failures, file.cases.len());
}

#[test]
fn vital_convert_matches_ace() {
    let file = load("vital_convert");
    let mut failures = Vec::new();
    for case in &file.cases {
        let a = Ability {
            base: u32_arg(&case.input["base"]),
            ranks: u32_arg(&case.input["ranks"]),
            experience_spent: u32_arg(&case.input["experience_spent"]),
        };
        let v = m::Vital::convert(&a);
        let ours = obj!("xp_spent" => v.xp_spent, "level_from_cp" => v.level_from_cp, "ranks" => v.ranks, "current" => v.current);
        same(
            &format!("{}", case.input),
            &ours,
            &case.output,
            &mut failures,
        );
    }
    finish("vital_convert", &failures, file.cases.len());
}
