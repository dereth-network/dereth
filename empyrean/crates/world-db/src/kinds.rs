//! Each world table as rows: a model to a [`Row`] and back, in ACE's names. Ids a dump numbers
//! freely (a child row's own id and its parent's) are not written; [`crate::canon`] numbers them.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::world_rules::WorldRules;
use empyrean_content::loot::{GroupedTreasureDeath, SourceLootPolicy};
use empyrean_content::models::world::{
    CookBook, Encounter, Event, HousePortal, LandblockInstance, LandblockInstanceLink,
    PointsOfInterest, Quest, Recipe, RecipeMod, RecipeModsBool, RecipeModsDID, RecipeModsFloat,
    RecipeModsIID, RecipeModsInt, RecipeModsString, RecipeRequirementsBool, RecipeRequirementsDID,
    RecipeRequirementsFloat, RecipeRequirementsIID, RecipeRequirementsInt,
    RecipeRequirementsString, RecipeVitalRequirement, Spell, TreasureDeath, TreasureGemCount,
    TreasureMaterialBase, TreasureMaterialColor, TreasureMaterialGroups, TreasureWielded, Version,
    Weenie, WeeniePropertiesAnimPart, WeeniePropertiesAttribute, WeeniePropertiesAttribute2nd,
    WeeniePropertiesBodyPart, WeeniePropertiesBook, WeeniePropertiesBookPageData,
    WeeniePropertiesBool, WeeniePropertiesCreateList, WeeniePropertiesDID, WeeniePropertiesEmote,
    WeeniePropertiesEmoteAction, WeeniePropertiesEventFilter, WeeniePropertiesFloat,
    WeeniePropertiesGenerator, WeeniePropertiesIID, WeeniePropertiesInt, WeeniePropertiesInt64,
    WeeniePropertiesPalette, WeeniePropertiesPosition, WeeniePropertiesSkill,
    WeeniePropertiesSpellBook, WeeniePropertiesString, WeeniePropertiesTextureMap,
};
use toml::{Table, Value};

use crate::names::{self, Kind};
use crate::row::{rows_of, sub, Row};
use crate::value::{hex, id_of, stamp_put, stamp_take, time_of, time_text, Field, R, V};

impl Field for DotNetDateTime {
    const TYPE: crate::value::Ty = crate::value::Ty::Time;
    fn spell(&self) -> V {
        time_text(*self)
    }
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
    fn take(v: Option<&Value>) -> R<Self> {
        v.map_or(Ok(Self::default()), time_of)
    }
}

/// A model written as a row's fields, and read back from them.
pub trait Fields: Sized {
    fn put(&self, row: &mut Row, stamp: DotNetDateTime);
    fn take(t: &Table, stamp: DotNetDateTime) -> R<Self>;
    /// Each field a row of it writes: its name, what it is, and whether an integer is written in
    /// hexadecimal ([`crate::schema`]).
    fn spec() -> Vec<(&'static str, crate::value::Ty, bool)>;
    /// The fields that may be unset rather than at their default ([`Field::OPTIONAL`]).
    fn optional() -> Vec<&'static str>;
}

/// The type of the field `f` reads, for [`Fields::spec`].
fn ty_of<S, T: Field>(_: fn(&S) -> &T) -> crate::value::Ty {
    T::TYPE
}

/// Whether the field `f` reads may be unset, for [`Fields::optional`].
fn optional_of<S, T: Field>(_: fn(&S) -> &T) -> bool {
    T::OPTIONAL
}

/// Whether a field written `how` is an integer in hexadecimal.
macro_rules! is_hex {
    () => {
        false
    };
    (hex) => {
        true
    };
    (hexo) => {
        true
    };
    (stamp) => {
        false
    };
}

/// A field's name in a row: a raw identifier's without its `r#`.
macro_rules! name {
    ($f:ident) => {
        stringify!($f).trim_start_matches("r#")
    };
}

macro_rules! put_one {
    ($row:ident, $st:ident, $v:expr, $f:ident) => {
        $row.set(name!($f), Field::put(&$v))
    };
    ($row:ident, $st:ident, $v:expr, $f:ident, hex) => {
        $row.set(name!($f), ($v != 0).then(|| hex($v)))
    };
    ($row:ident, $st:ident, $v:expr, $f:ident, hexo) => {
        $row.set(name!($f), $v.map(hex))
    };
    ($row:ident, $st:ident, $v:expr, $f:ident, stamp) => {
        $row.set(name!($f), stamp_put($v, $st))
    };
}

macro_rules! take_one {
    ($t:ident, $st:ident, $f:ident) => {
        Field::take($t.get(name!($f))).map_err(|e| format!("{}: {e}", name!($f)))?
    };
    ($t:ident, $st:ident, $f:ident, hex) => {
        $t.get(name!($f)).map(id_of).transpose()?.unwrap_or(0)
    };
    ($t:ident, $st:ident, $f:ident, hexo) => {
        $t.get(name!($f)).map(id_of).transpose()?
    };
    ($t:ident, $st:ident, $f:ident, stamp) => {
        stamp_take($t.get(name!($f)), $st)?
    };
}

/// The fields of a model, each `name` or `name: how` (`hex`, `hexo` for an optional one, `stamp` for
/// `last_modified`); a field left out is never written and reads back as its default.
macro_rules! fields {
    ($ty:ident { $($f:ident $(: $how:ident)?),* $(,)? }) => {
        impl Fields for $ty {
            fn put(&self, row: &mut Row, stamp: DotNetDateTime) {
                let _ = stamp;
                $( put_one!(row, stamp, self.$f, $f $(, $how)?); )*
            }
            #[allow(clippy::needless_update)]
            fn take(t: &Table, stamp: DotNetDateTime) -> R<Self> {
                let _ = stamp;
                Ok(Self { $( $f: take_one!(t, stamp, $f $(, $how)?), )* ..Default::default() })
            }
            fn spec() -> Vec<(&'static str, crate::value::Ty, bool)> {
                vec![$( (name!($f), ty_of(|s: &$ty| &s.$f), is_hex!($($how)?)), )*]
            }
            fn optional() -> Vec<&'static str> {
                let mut v = Vec::new();
                $( if optional_of(|s: &$ty| &s.$f) { v.push(name!($f)); } )*
                v
            }
        }
    };
}

// The weenie's child rows, without `id` and `object_id`.
fields!(WeeniePropertiesAnimPart {
    index,
    animation_id: hex
});
fields!(WeeniePropertiesAttribute {
    init_level,
    level_from_cp,
    cp_spent
});
fields!(WeeniePropertiesAttribute2nd {
    init_level,
    level_from_cp,
    cp_spent,
    current_level
});
fields!(WeeniePropertiesBodyPart {
    d_type,
    d_val,
    d_var,
    base_armor,
    armor_vs_slash,
    armor_vs_pierce,
    armor_vs_bludgeon,
    armor_vs_cold,
    armor_vs_fire,
    armor_vs_acid,
    armor_vs_electric,
    armor_vs_nether,
    bh,
    hlf,
    mlf,
    llf,
    hrf,
    mrf,
    lrf,
    hlb,
    mlb,
    llb,
    hrb,
    mrb,
    lrb,
});
fields!(WeeniePropertiesBook {
    max_num_pages,
    max_num_chars_per_page
});
fields!(WeeniePropertiesBookPageData {
    page_id,
    author_id: hex,
    author_name,
    author_account,
    ignore_author,
    page_text,
});
fields!(WeeniePropertiesCreateList {
    destination_type,
    weenie_class_id,
    stack_size,
    palette,
    shade,
    try_to_bond,
});
fields!(WeeniePropertiesEmoteAction {
    order,
    delay,
    extent,
    motion: hexo,
    message,
    test_string,
    min,
    max,
    min_64,
    max_64,
    min_dbl,
    max_dbl,
    stat,
    display,
    amount,
    amount_64,
    hero_xp_64,
    percent,
    spell_id,
    wealth_rating,
    treasure_class,
    treasure_type,
    p_script,
    sound,
    destination_type,
    weenie_class_id,
    stack_size,
    palette,
    shade,
    try_to_bond,
    obj_cell_id: hexo,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
});
fields!(WeeniePropertiesEventFilter { event });
fields!(WeeniePropertiesGenerator {
    probability,
    weenie_class_id,
    delay,
    init_create,
    max_create,
    when_create,
    where_create,
    stack_size,
    palette_id,
    shade,
    obj_cell_id: hexo,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
});
fields!(WeeniePropertiesPalette {
    sub_palette_id: hex,
    offset,
    length
});
fields!(WeeniePropertiesSkill {
    level_from_pp,
    sac,
    pp,
    init_level,
    resistance_at_last_check,
    last_used_time,
});
fields!(WeeniePropertiesTextureMap {
    index,
    old_id: hex,
    new_id: hex
});

// The other tables, without their dump ids (a spell's and a recipe's are their keys, written
// by the row).
fields!(Encounter {
    last_modified: stamp
});
fields!(Event {
    name,
    start_time,
    end_time,
    state,
    last_modified: stamp
});
fields!(HousePortal {
    house_id,
    obj_cell_id: hex,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
    last_modified: stamp,
});
fields!(PointsOfInterest {
    name,
    weenie_class_id,
    last_modified: stamp
});
fields!(Quest {
    name,
    min_delta,
    max_solves,
    message,
    last_modified: stamp
});
fields!(Spell {
    name,
    stat_mod_type,
    stat_mod_key,
    stat_mod_val,
    e_type,
    base_intensity,
    variance,
    wcid,
    num_projectiles,
    num_projectiles_variance,
    spread_angle,
    vertical_angle,
    default_launch_angle,
    non_tracking,
    create_offset_origin_x,
    create_offset_origin_y,
    create_offset_origin_z,
    padding_origin_x,
    padding_origin_y,
    padding_origin_z,
    dims_origin_x,
    dims_origin_y,
    dims_origin_z,
    peturbation_origin_x,
    peturbation_origin_y,
    peturbation_origin_z,
    imbued_effect,
    slayer_creature_type,
    slayer_damage_bonus,
    crit_freq,
    crit_multiplier,
    ignore_magic_resist,
    elemental_modifier,
    drain_percentage,
    damage_ratio,
    damage_type,
    boost,
    boost_variance,
    source,
    destination,
    proportion,
    loss_percent,
    source_loss,
    transfer_cap,
    max_boost_allowed,
    transfer_bitfield,
    index,
    link,
    position_obj_cell_id: hexo,
    position_origin_x,
    position_origin_y,
    position_origin_z,
    position_angles_w,
    position_angles_x,
    position_angles_y,
    position_angles_z,
    min_power,
    max_power,
    power_variance,
    dispel_school,
    align,
    number,
    number_variance,
    dot_duration,
    last_modified: stamp,
});
fields!(TreasureDeath {
    tier,
    loot_quality_mod,
    unknown_chances,
    item_chance,
    item_min_amount,
    item_max_amount,
    item_treasure_type_selection_chances,
    magic_item_chance,
    magic_item_min_amount,
    magic_item_max_amount,
    magic_item_treasure_type_selection_chances,
    mundane_item_chance,
    mundane_item_min_amount,
    mundane_item_max_amount,
    mundane_item_type_selection_chances,
    last_modified: stamp,
});
fields!(TreasureGemCount {
    gem_code,
    tier,
    count,
    chance
});
fields!(TreasureMaterialBase {
    material_code,
    tier,
    probability,
    material_id
});
fields!(TreasureMaterialColor {
    material_id,
    color_code,
    palette_template,
    probability
});
fields!(TreasureMaterialGroups {
    material_group,
    tier,
    probability,
    material_id
});
fields!(TreasureWielded {
    weenie_class_id,
    palette_id,
    unknown_1,
    shade,
    stack_size,
    stack_size_variance,
    probability,
    unknown_3,
    unknown_4,
    unknown_5,
    set_start,
    has_sub_set,
    continues_previous_set,
    unknown_9,
    unknown_10,
    unknown_11,
    unknown_12,
    last_modified: stamp,
});
fields!(Version {
    base_version,
    patch_version,
    last_modified: stamp
});
fields!(RecipeVitalRequirement {
    health,
    stamina,
    mana
});
fields!(Recipe {
    unknown_1,
    skill,
    difficulty,
    salvage_type,
    success_wcid,
    success_amount,
    success_message,
    fail_wcid,
    fail_amount,
    fail_message,
    success_destroy_source_chance,
    success_destroy_source_amount,
    success_destroy_source_message,
    success_destroy_target_chance,
    success_destroy_target_amount,
    success_destroy_target_message,
    fail_destroy_source_chance,
    fail_destroy_source_amount,
    fail_destroy_source_message,
    fail_destroy_target_chance,
    fail_destroy_target_amount,
    fail_destroy_target_message,
    data_id: hex,
    last_modified: stamp,
});
fields!(RecipeMod {
    executes_on_success,
    health,
    stamina,
    mana,
    unknown_7,
    data_id,
    unknown_9,
    instance_id,
});
fields!(RecipeModsBool {
    index,
    stat,
    value,
    r#enum,
    source
});
fields!(RecipeModsDID {
    index,
    stat,
    value: hex,
    r#enum,
    source
});
fields!(RecipeModsFloat {
    index,
    stat,
    value,
    r#enum,
    source
});
fields!(RecipeModsIID {
    index,
    stat,
    value: hex,
    r#enum,
    source
});
fields!(RecipeModsInt {
    index,
    stat,
    value,
    r#enum,
    source
});
fields!(RecipeModsString {
    index,
    stat,
    value,
    r#enum,
    source
});
fields!(RecipeRequirementsBool {
    index,
    stat,
    value,
    r#enum,
    message
});
fields!(RecipeRequirementsDID {
    index,
    stat,
    value: hex,
    r#enum,
    message
});
fields!(RecipeRequirementsFloat {
    index,
    stat,
    value,
    r#enum,
    message
});
fields!(RecipeRequirementsIID {
    index,
    stat,
    value: hex,
    r#enum,
    message
});
fields!(RecipeRequirementsInt {
    index,
    stat,
    value,
    r#enum,
    message
});
fields!(RecipeRequirementsString {
    index,
    stat,
    value,
    r#enum,
    message
});

/// `item`'s fields as a row of their own.
fn row_of<T: Fields>(item: &T, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    item.put(&mut r, stamp);
    r
}

/// The fields of a row as an inline table.
fn inline<T: Fields>(item: &T, stamp: DotNetDateTime) -> V {
    V::Inline(row_of(item, stamp).fields)
}

fn list_of<T: Fields>(items: &[T], stamp: DotNetDateTime) -> Vec<Row> {
    items.iter().map(|i| row_of(i, stamp)).collect()
}

fn take_list<T: Fields>(t: &Table, name: &str, stamp: DotNetDateTime) -> R<Vec<T>> {
    rows_of(t, name)?
        .into_iter()
        .map(|r| T::take(r, stamp).map_err(|e| format!("{name}: {e}")))
        .collect()
}

fn i64_of(n: i64, what: &str) -> R<u16> {
    u16::try_from(n).map_err(|_| format!("{what} {n} is out of range"))
}

/// A map of a weenie's typed property rows, by the type's name.
fn by_name<T>(
    kind: Kind,
    rows: &[T],
    ty: impl Fn(&T) -> i64,
    value: impl Fn(&T) -> V,
) -> Vec<(String, V)> {
    let names = names::of(kind);
    rows.iter().map(|r| (names.name(ty(r)), value(r))).collect()
}

/// A map's entries back as `(type, value)`.
fn entries<'a>(t: &'a Table, map: &str, kind: Kind) -> R<Vec<(i64, &'a Value)>> {
    let names = names::of(kind);
    let Some(m) = sub(t, map) else {
        return Ok(Vec::new());
    };
    m.iter()
        .map(|(k, v)| Ok((names.value(k).map_err(|e| format!("{map}: {e}"))?, v)))
        .collect()
}

fn position_v(p: &WeeniePropertiesPosition) -> V {
    V::Inline(vec![
        ("cell".into(), hex(p.obj_cell_id)),
        (
            "at".into(),
            V::Array(vec![
                p.origin_x.spell(),
                p.origin_y.spell(),
                p.origin_z.spell(),
            ]),
        ),
        (
            "rotation".into(),
            V::Array(vec![
                p.angles_w.spell(),
                p.angles_x.spell(),
                p.angles_y.spell(),
                p.angles_z.spell(),
            ]),
        ),
    ])
}

/// Three or four floats.
fn floats<const N: usize>(v: Option<&Value>, what: &str) -> R<[f32; N]> {
    let items = v
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{what} isn't a list of {N} numbers"))?;
    if items.len() != N {
        return Err(format!("{what} has {} numbers, not {N}", items.len()));
    }
    let mut out = [0.0; N];
    for (o, i) in out.iter_mut().zip(items) {
        *o = f32::take(Some(i))?;
    }
    Ok(out)
}

/// A weenie as one row: its class, name and type, its properties by name, and its lists.
#[must_use]
pub fn weenie_row(w: &Weenie, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set("class", Some(V::Int(i64::from(w.class_id))));
    r.set("class_name", Some(V::Str(w.class_name.clone())));
    r.set(
        "type",
        Some(V::Str(
            names::of(Kind::WeenieType).name(i64::from(w.r#type)),
        )),
    );
    r.set("last_modified", stamp_put(w.last_modified, stamp));
    r.map(
        "int",
        by_name(
            Kind::Int,
            &w.weenie_properties_int,
            |p| p.r#type.into(),
            |p| V::Int(p.value.into()),
        ),
    );
    r.map(
        "int64",
        by_name(
            Kind::Int64,
            &w.weenie_properties_int64,
            |p| p.r#type.into(),
            |p| V::Int(p.value),
        ),
    );
    r.map(
        "bool",
        by_name(
            Kind::Bool,
            &w.weenie_properties_bool,
            |p| p.r#type.into(),
            |p| V::Bool(p.value),
        ),
    );
    r.map(
        "float",
        by_name(
            Kind::Float,
            &w.weenie_properties_float,
            |p| p.r#type.into(),
            |p| p.value.spell(),
        ),
    );
    r.map(
        "string",
        by_name(
            Kind::Str,
            &w.weenie_properties_string,
            |p| p.r#type.into(),
            |p| V::Str(p.value.clone()),
        ),
    );
    r.map(
        "did",
        by_name(
            Kind::Did,
            &w.weenie_properties_did,
            |p| p.r#type.into(),
            |p| hex(p.value),
        ),
    );
    r.map(
        "iid",
        by_name(
            Kind::Iid,
            &w.weenie_properties_iid,
            |p| p.r#type.into(),
            |p| hex(p.value),
        ),
    );
    r.map(
        "position",
        by_name(
            Kind::Position,
            &w.weenie_properties_position,
            |p| p.position_type.into(),
            position_v,
        ),
    );
    r.map(
        "attribute",
        by_name(
            Kind::Attribute,
            &w.weenie_properties_attribute,
            |p| p.r#type.into(),
            |p| inline(p, stamp),
        ),
    );
    r.map(
        "vital",
        by_name(
            Kind::Vital,
            &w.weenie_properties_attribute_2nd,
            |p| p.r#type.into(),
            |p| inline(p, stamp),
        ),
    );
    r.map(
        "skill",
        by_name(
            Kind::Skill,
            &w.weenie_properties_skill,
            |p| p.r#type.into(),
            |p| inline(p, stamp),
        ),
    );
    r.map(
        "body_part",
        by_name(
            Kind::BodyPart,
            &w.weenie_properties_body_part,
            |p| p.key.into(),
            |p| inline(p, stamp),
        ),
    );
    r.map(
        "spell_book",
        w.weenie_properties_spell_book
            .iter()
            .map(|s| (s.spell.to_string(), s.probability.spell()))
            .collect(),
    );
    if let Some(b) = &w.weenie_properties_book {
        r.map("book", row_of(b, stamp).fields);
    }
    r.list("anim_part", list_of(&w.weenie_properties_anim_part, stamp));
    r.list(
        "book_page",
        list_of(&w.weenie_properties_book_page_data, stamp),
    );
    r.list(
        "create_list",
        list_of(&w.weenie_properties_create_list, stamp),
    );
    r.list(
        "emote",
        w.weenie_properties_emote
            .iter()
            .map(|e| {
                let mut er = Row::default();
                er.set(
                    "category",
                    Some(V::Str(
                        names::of(Kind::EmoteCategory).name(i64::from(e.category)),
                    )),
                );
                er.set("probability", Field::put(&e.probability));
                er.set("weenie_class_id", Field::put(&e.weenie_class_id));
                er.set("style", e.style.map(hex));
                er.set("substyle", e.substyle.map(hex));
                er.set("quest", Field::put(&e.quest));
                er.set("vendor_type", Field::put(&e.vendor_type));
                er.set("min_health", Field::put(&e.min_health));
                er.set("max_health", Field::put(&e.max_health));
                er.list(
                    "action",
                    e.weenie_properties_emote_action
                        .iter()
                        .map(|a| {
                            let mut ar = Row::default();
                            ar.set(
                                "type",
                                Some(V::Str(names::of(Kind::EmoteType).name(i64::from(a.r#type)))),
                            );
                            a.put(&mut ar, stamp);
                            ar
                        })
                        .collect(),
                );
                er
            })
            .collect(),
    );
    r.list(
        "event_filter",
        list_of(&w.weenie_properties_event_filter, stamp),
    );
    r.list("generator", list_of(&w.weenie_properties_generator, stamp));
    r.list("palette", list_of(&w.weenie_properties_palette, stamp));
    r.list(
        "texture_map",
        list_of(&w.weenie_properties_texture_map, stamp),
    );
    r
}

fn named_u32(t: &Table, field: &str, kind: Kind) -> R<u32> {
    let v = t.get(field).ok_or_else(|| format!("no {field}"))?;
    let n = match v {
        Value::String(s) => names::of(kind).value(s)?,
        other => other
            .as_integer()
            .ok_or_else(|| format!("{field} isn't a name"))?,
    };
    u32::try_from(n).map_err(|_| format!("{field} {n} is out of range"))
}

/// A weenie read back from its row.
pub fn weenie_of(t: &Table, stamp: DotNetDateTime) -> R<Weenie> {
    let class = t.get("class").map(id_of).transpose()?.ok_or("no class")?;
    let err = |e: String| format!("weenie {class}: {e}");
    let mut w = Weenie {
        class_id: class,
        class_name: String::take(t.get("class_name")).map_err(err)?,
        r#type: i32::try_from(named_u32(t, "type", Kind::WeenieType).map_err(err)?)
            .map_err(|e| err(e.to_string()))?,
        last_modified: stamp_take(t.get("last_modified"), stamp).map_err(err)?,
        ..Weenie::default()
    };
    for (ty, v) in entries(t, "int", Kind::Int).map_err(err)? {
        w.weenie_properties_int.push(WeeniePropertiesInt {
            r#type: i64_of(ty, "int").map_err(err)?,
            value: i32::take(Some(v)).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "int64", Kind::Int64).map_err(err)? {
        w.weenie_properties_int64.push(WeeniePropertiesInt64 {
            r#type: i64_of(ty, "int64").map_err(err)?,
            value: i64::take(Some(v)).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "bool", Kind::Bool).map_err(err)? {
        w.weenie_properties_bool.push(WeeniePropertiesBool {
            r#type: i64_of(ty, "bool").map_err(err)?,
            value: bool::take(Some(v)).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "float", Kind::Float).map_err(err)? {
        w.weenie_properties_float.push(WeeniePropertiesFloat {
            r#type: i64_of(ty, "float").map_err(err)?,
            value: f64::take(Some(v)).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "string", Kind::Str).map_err(err)? {
        w.weenie_properties_string.push(WeeniePropertiesString {
            r#type: i64_of(ty, "string").map_err(err)?,
            value: String::take(Some(v)).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "did", Kind::Did).map_err(err)? {
        w.weenie_properties_did.push(WeeniePropertiesDID {
            r#type: i64_of(ty, "did").map_err(err)?,
            value: id_of(v).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "iid", Kind::Iid).map_err(err)? {
        w.weenie_properties_iid.push(WeeniePropertiesIID {
            r#type: i64_of(ty, "iid").map_err(err)?,
            value: id_of(v).map_err(err)?,
            ..Default::default()
        });
    }
    for (ty, v) in entries(t, "position", Kind::Position).map_err(err)? {
        let p = v
            .as_table()
            .ok_or_else(|| err("a position isn't a table".into()))?;
        let at: [f32; 3] = floats(p.get("at"), "at").map_err(err)?;
        let rot: [f32; 4] = floats(p.get("rotation"), "rotation").map_err(err)?;
        w.weenie_properties_position.push(WeeniePropertiesPosition {
            position_type: i64_of(ty, "position").map_err(err)?,
            obj_cell_id: p
                .get("cell")
                .map(id_of)
                .transpose()
                .map_err(err)?
                .unwrap_or(0),
            origin_x: at[0],
            origin_y: at[1],
            origin_z: at[2],
            angles_w: rot[0],
            angles_x: rot[1],
            angles_y: rot[2],
            angles_z: rot[3],
            ..Default::default()
        });
    }
    let table_of = |v: &Value| {
        v.as_table()
            .cloned()
            .ok_or_else(|| err("not a table".into()))
    };
    for (ty, v) in entries(t, "attribute", Kind::Attribute).map_err(err)? {
        let mut a = WeeniePropertiesAttribute::take(&table_of(v)?, stamp).map_err(err)?;
        a.r#type = i64_of(ty, "attribute").map_err(err)?;
        w.weenie_properties_attribute.push(a);
    }
    for (ty, v) in entries(t, "vital", Kind::Vital).map_err(err)? {
        let mut a = WeeniePropertiesAttribute2nd::take(&table_of(v)?, stamp).map_err(err)?;
        a.r#type = i64_of(ty, "vital").map_err(err)?;
        w.weenie_properties_attribute_2nd.push(a);
    }
    for (ty, v) in entries(t, "skill", Kind::Skill).map_err(err)? {
        let mut a = WeeniePropertiesSkill::take(&table_of(v)?, stamp).map_err(err)?;
        a.r#type = i64_of(ty, "skill").map_err(err)?;
        w.weenie_properties_skill.push(a);
    }
    for (ty, v) in entries(t, "body_part", Kind::BodyPart).map_err(err)? {
        let mut a = WeeniePropertiesBodyPart::take(&table_of(v)?, stamp).map_err(err)?;
        a.key = i64_of(ty, "body_part").map_err(err)?;
        w.weenie_properties_body_part.push(a);
    }
    if let Some(m) = sub(t, "spell_book") {
        for (k, v) in m {
            w.weenie_properties_spell_book
                .push(WeeniePropertiesSpellBook {
                    spell: k
                        .parse::<i32>()
                        .map_err(|_| err(format!("spell {k:?} isn't a number")))?,
                    probability: f32::take(Some(v)).map_err(err)?,
                    ..Default::default()
                });
        }
    }
    if let Some(b) = sub(t, "book") {
        w.weenie_properties_book = Some(WeeniePropertiesBook::take(b, stamp).map_err(err)?);
    }
    w.weenie_properties_anim_part = take_list(t, "anim_part", stamp).map_err(err)?;
    w.weenie_properties_book_page_data = take_list(t, "book_page", stamp).map_err(err)?;
    w.weenie_properties_create_list = take_list(t, "create_list", stamp).map_err(err)?;
    for e in rows_of(t, "emote").map_err(err)? {
        let id = |f: &str| e.get(f).map(id_of).transpose().map_err(err);
        let mut emote = WeeniePropertiesEmote {
            category: named_u32(e, "category", Kind::EmoteCategory).map_err(err)?,
            probability: f32::take(e.get("probability")).map_err(err)?,
            weenie_class_id: <Option<u32> as Field>::take(e.get("weenie_class_id")).map_err(err)?,
            style: id("style")?,
            substyle: id("substyle")?,
            quest: <Option<String> as Field>::take(e.get("quest")).map_err(err)?,
            vendor_type: <Option<i32> as Field>::take(e.get("vendor_type")).map_err(err)?,
            min_health: <Option<f32> as Field>::take(e.get("min_health")).map_err(err)?,
            max_health: <Option<f32> as Field>::take(e.get("max_health")).map_err(err)?,
            ..Default::default()
        };
        for a in rows_of(e, "action").map_err(err)? {
            let mut action = WeeniePropertiesEmoteAction::take(a, stamp).map_err(err)?;
            action.r#type = named_u32(a, "type", Kind::EmoteType).map_err(err)?;
            emote.weenie_properties_emote_action.push(action);
        }
        w.weenie_properties_emote.push(emote);
    }
    w.weenie_properties_event_filter = take_list(t, "event_filter", stamp).map_err(err)?;
    w.weenie_properties_generator = take_list(t, "generator", stamp).map_err(err)?;
    w.weenie_properties_palette = take_list(t, "palette", stamp).map_err(err)?;
    w.weenie_properties_texture_map = take_list(t, "texture_map", stamp).map_err(err)?;
    Ok(w)
}

/// A placement as a row: its guid, class, cell, spot and turn, and the guids it links.
#[must_use]
pub fn placement_row(i: &LandblockInstance, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set("guid", Some(hex(i.guid)));
    r.set("class", Some(V::Int(i64::from(i.weenie_class_id))));
    r.set("cell", Some(hex(i.obj_cell_id)));
    r.set(
        "at",
        Some(V::Array(vec![
            i.origin_x.spell(),
            i.origin_y.spell(),
            i.origin_z.spell(),
        ])),
    );
    r.set(
        "rotation",
        Some(V::Array(vec![
            i.angles_w.spell(),
            i.angles_x.spell(),
            i.angles_y.spell(),
            i.angles_z.spell(),
        ])),
    );
    r.set("link_child", Field::put(&i.is_link_child));
    r.set("last_modified", stamp_put(i.last_modified, stamp));
    let links = &i.landblock_instance_link;
    if links.iter().all(|l| l.last_modified == stamp) {
        r.set(
            "links",
            (!links.is_empty())
                .then(|| V::Array(links.iter().map(|l| hex(l.child_guid)).collect())),
        );
    } else {
        r.set(
            "links",
            Some(V::Array(
                links
                    .iter()
                    .map(|l| {
                        let mut f = vec![("child".to_owned(), hex(l.child_guid))];
                        if let Some(t) = stamp_put(l.last_modified, stamp) {
                            f.push(("last_modified".into(), t));
                        }
                        V::Inline(f)
                    })
                    .collect(),
            )),
        );
    }
    r
}

/// A placement read back.
pub fn placement_of(t: &Table, stamp: DotNetDateTime) -> R<LandblockInstance> {
    let guid = t
        .get("guid")
        .map(id_of)
        .transpose()?
        .ok_or("a placement has no guid")?;
    let err = |e: String| format!("placement {guid:#010X}: {e}");
    let at: [f32; 3] = floats(t.get("at"), "at").map_err(err)?;
    let rot: [f32; 4] = floats(t.get("rotation"), "rotation").map_err(err)?;
    let mut links = Vec::new();
    if let Some(Value::Array(items)) = t.get("links") {
        for l in items {
            links.push(match l {
                Value::Table(lt) => LandblockInstanceLink {
                    child_guid: lt
                        .get("child")
                        .map(id_of)
                        .transpose()
                        .map_err(err)?
                        .unwrap_or(0),
                    last_modified: stamp_take(lt.get("last_modified"), stamp).map_err(err)?,
                    ..Default::default()
                },
                other => LandblockInstanceLink {
                    child_guid: id_of(other).map_err(err)?,
                    last_modified: stamp,
                    ..Default::default()
                },
            });
        }
    }
    Ok(LandblockInstance {
        guid,
        landblock: None,
        weenie_class_id: t
            .get("class")
            .map(id_of)
            .transpose()
            .map_err(err)?
            .unwrap_or(0),
        obj_cell_id: t
            .get("cell")
            .map(id_of)
            .transpose()
            .map_err(err)?
            .unwrap_or(0),
        origin_x: at[0],
        origin_y: at[1],
        origin_z: at[2],
        angles_w: rot[0],
        angles_x: rot[1],
        angles_y: rot[2],
        angles_z: rot[3],
        is_link_child: bool::take(t.get("link_child")).map_err(err)?,
        last_modified: stamp_take(t.get("last_modified"), stamp).map_err(err)?,
        landblock_instance_link: links,
    })
}

/// An encounter as a row: its landblock, cell and class.
#[must_use]
pub fn encounter_row(e: &Encounter, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    #[allow(clippy::cast_sign_loss)]
    r.set("landblock", Some(V::Block(e.landblock as u32)));
    r.set(
        "cell",
        Some(V::Array(vec![
            V::Int(e.cell_x.into()),
            V::Int(e.cell_y.into()),
        ])),
    );
    r.set("class", Some(V::Int(i64::from(e.weenie_class_id))));
    e.put(&mut r, stamp);
    r
}

/// An encounter read back.
pub fn encounter_of(t: &Table, stamp: DotNetDateTime) -> R<Encounter> {
    let mut e = Encounter::take(t, stamp)?;
    #[allow(clippy::cast_possible_wrap)]
    {
        e.landblock = t
            .get("landblock")
            .map(id_of)
            .transpose()?
            .ok_or("an encounter has no landblock")? as i32;
    }
    let cell = t
        .get("cell")
        .and_then(Value::as_array)
        .ok_or("an encounter has no cell")?;
    if cell.len() != 2 {
        return Err("an encounter's cell isn't [x, y]".into());
    }
    e.cell_x = i32::take(Some(&cell[0]))?;
    e.cell_y = i32::take(Some(&cell[1]))?;
    e.weenie_class_id = t
        .get("class")
        .map(id_of)
        .transpose()?
        .ok_or("an encounter has no class")?;
    Ok(e)
}

/// A cook book entry: a recipe, what it's used on and with.
#[must_use]
pub fn cook_book_row(c: &CookBook, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set("recipe", Some(V::Int(i64::from(c.recipe_id))));
    r.set("source", Some(V::Int(i64::from(c.source_wcid))));
    r.set("target", Some(V::Int(i64::from(c.target_wcid))));
    r.set("last_modified", stamp_put(c.last_modified, stamp));
    r
}

pub fn cook_book_of(t: &Table, stamp: DotNetDateTime) -> R<CookBook> {
    let n = |f: &str| {
        t.get(f)
            .map(id_of)
            .transpose()?
            .ok_or_else(|| format!("a cook book entry has no {f}"))
    };
    Ok(CookBook {
        recipe_id: n("recipe")?,
        source_wcid: n("source")?,
        target_wcid: n("target")?,
        last_modified: stamp_take(t.get("last_modified"), stamp)?,
        ..Default::default()
    })
}

/// A recipe, by its id: its fields, its mods (each with its six lists) and its six requirement
/// lists.
#[must_use]
pub fn recipe_row(x: &Recipe, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set("id", Some(V::Int(i64::from(x.id))));
    x.put(&mut r, stamp);
    r.list(
        "mod",
        x.recipe_mod
            .iter()
            .map(|m| {
                let mut mr = row_of(m, stamp);
                mr.list("bool", list_of(&m.recipe_mods_bool, stamp));
                mr.list("did", list_of(&m.recipe_mods_did, stamp));
                mr.list("float", list_of(&m.recipe_mods_float, stamp));
                mr.list("iid", list_of(&m.recipe_mods_iid, stamp));
                mr.list("int", list_of(&m.recipe_mods_int, stamp));
                mr.list("string", list_of(&m.recipe_mods_string, stamp));
                mr
            })
            .collect(),
    );
    r.list("require_bool", list_of(&x.recipe_requirements_bool, stamp));
    r.list("require_did", list_of(&x.recipe_requirements_did, stamp));
    r.list(
        "require_float",
        list_of(&x.recipe_requirements_float, stamp),
    );
    r.list("require_iid", list_of(&x.recipe_requirements_iid, stamp));
    r.list("require_int", list_of(&x.recipe_requirements_int, stamp));
    r.list(
        "require_string",
        list_of(&x.recipe_requirements_string, stamp),
    );
    r
}

pub fn recipe_of(t: &Table, stamp: DotNetDateTime) -> R<Recipe> {
    let id = t
        .get("id")
        .map(id_of)
        .transpose()?
        .ok_or("a recipe has no id")?;
    let err = |e: String| format!("recipe {id}: {e}");
    let mut x = Recipe::take(t, stamp).map_err(err)?;
    x.id = id;
    for m in rows_of(t, "mod").map_err(err)? {
        let mut rm = RecipeMod::take(m, stamp).map_err(err)?;
        rm.recipe_mods_bool = take_list(m, "bool", stamp).map_err(err)?;
        rm.recipe_mods_did = take_list(m, "did", stamp).map_err(err)?;
        rm.recipe_mods_float = take_list(m, "float", stamp).map_err(err)?;
        rm.recipe_mods_iid = take_list(m, "iid", stamp).map_err(err)?;
        rm.recipe_mods_int = take_list(m, "int", stamp).map_err(err)?;
        rm.recipe_mods_string = take_list(m, "string", stamp).map_err(err)?;
        x.recipe_mod.push(rm);
    }
    x.recipe_requirements_bool = take_list(t, "require_bool", stamp).map_err(err)?;
    x.recipe_requirements_did = take_list(t, "require_did", stamp).map_err(err)?;
    x.recipe_requirements_float = take_list(t, "require_float", stamp).map_err(err)?;
    x.recipe_requirements_iid = take_list(t, "require_iid", stamp).map_err(err)?;
    x.recipe_requirements_int = take_list(t, "require_int", stamp).map_err(err)?;
    x.recipe_requirements_string = take_list(t, "require_string", stamp).map_err(err)?;
    Ok(x)
}

/// A record keyed by a number of its own (a spell's id, a recipe's vitals, a treasure type), with
/// its other fields.
#[must_use]
pub fn keyed_row<T: Fields>(key: &str, value: u32, item: &T, stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set(key, Some(V::Int(i64::from(value))));
    item.put(&mut r, stamp);
    r
}

/// A keyed record read back, and its key.
pub fn keyed_of<T: Fields>(t: &Table, key: &str, stamp: DotNetDateTime) -> R<(u32, T)> {
    let k = t
        .get(key)
        .map(id_of)
        .transpose()?
        .ok_or_else(|| format!("no {key}"))?;
    Ok((k, T::take(t, stamp).map_err(|e| format!("{key} {k}: {e}"))?))
}

/// A wielded treasure type's rows, in their order, as one row: a set is its order.
#[must_use]
pub fn wielded_row(ty: u32, rows: &[TreasureWielded], stamp: DotNetDateTime) -> Row {
    let mut r = Row::default();
    r.set("type", Some(V::Int(i64::from(ty))));
    r.list("entry", list_of(rows, stamp));
    r
}

/// A wielded treasure type's rows read back, in their order.
pub fn wielded_of(t: &Table, stamp: DotNetDateTime) -> R<Vec<TreasureWielded>> {
    let ty = t.get("type").map(id_of).transpose()?.ok_or("no type")?;
    let mut rows: Vec<TreasureWielded> = take_list(t, "entry", stamp)?;
    for r in &mut rows {
        r.treasure_type = ty;
    }
    Ok(rows)
}

/// A treasure material group's materials at one tier, in their order.
#[must_use]
pub fn material_group_row(
    group: u32,
    tier: u32,
    rows: &[TreasureMaterialGroups],
    stamp: DotNetDateTime,
) -> Row {
    let mut r = Row::default();
    r.set("material_group", Some(V::Int(i64::from(group))));
    r.set("tier", Some(V::Int(i64::from(tier))));
    r.list("entry", list_of(rows, stamp));
    r
}

/// A treasure material group's materials at one tier read back, in their order.
pub fn material_groups_of(t: &Table, stamp: DotNetDateTime) -> R<Vec<TreasureMaterialGroups>> {
    let n = |f: &str| t.get(f).map(id_of).transpose()?.ok_or(format!("no {f}"));
    let (group, tier) = (n("material_group")?, n("tier")?);
    let mut rows: Vec<TreasureMaterialGroups> = take_list(t, "entry", stamp)?;
    for r in &mut rows {
        r.material_group = group;
        r.tier = tier;
    }
    Ok(rows)
}

/// A record with no key of its own beyond its fields.
#[must_use]
pub fn plain_row<T: Fields>(item: &T, stamp: DotNetDateTime) -> Row {
    row_of(item, stamp)
}

// ---- the source loot tables, which are their own serde models --------------------------------

fn json_v(v: &serde_json::Value) -> V {
    match v {
        serde_json::Value::Null => V::Array(Vec::new()),
        serde_json::Value::Bool(b) => V::Bool(*b),
        serde_json::Value::Number(n) => n.as_i64().map_or_else(
            || {
                V::Float(crate::value::float_text(format!(
                    "{}",
                    n.as_f64().unwrap_or(0.0)
                )))
            },
            V::Int,
        ),
        serde_json::Value::String(s) => V::Str(s.clone()),
        serde_json::Value::Array(items) => V::Array(items.iter().map(json_v).collect()),
        serde_json::Value::Object(m) => V::Inline(
            m.iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), json_v(v)))
                .collect(),
        ),
    }
}

fn is_object_list(v: &serde_json::Value) -> bool {
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.iter().all(serde_json::Value::is_object))
}

/// A JSON object as a row: scalars and lists of scalars as fields, an object as a table of its
/// own, a list of objects as a list of rows. A null is left out.
fn json_row(m: &serde_json::Map<String, serde_json::Value>) -> Row {
    let mut r = Row::default();
    for (k, v) in m {
        match v {
            serde_json::Value::Null => {}
            serde_json::Value::Object(o) => r.tables.push((k.clone(), json_row(o))),
            serde_json::Value::Array(items) if is_object_list(v) => r.lists.push((
                k.clone(),
                items
                    .iter()
                    .filter_map(|i| i.as_object().map(json_row))
                    .collect(),
            )),
            other => r.fields.push((k.clone(), json_v(other))),
        }
    }
    r
}

fn toml_json(v: &Value) -> serde_json::Value {
    match v {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Integer(i) => serde_json::Value::from(*i),
        Value::Float(f) => serde_json::Value::from(*f),
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        Value::Array(items) => serde_json::Value::Array(items.iter().map(toml_json).collect()),
        Value::Table(t) => {
            serde_json::Value::Object(t.iter().map(|(k, v)| (k.clone(), toml_json(v))).collect())
        }
    }
}

/// A serde model's row, without its dump id.
pub fn serde_row<T: serde::Serialize>(item: &T) -> R<Row> {
    let mut v = serde_json::to_value(item).map_err(|e| e.to_string())?;
    if let Some(m) = v.as_object_mut() {
        m.remove("id");
    }
    let serde_json::Value::Object(m) = v else {
        return Err("not an object".into());
    };
    Ok(json_row(&m))
}

/// A serde model read back from its row, its id 0.
pub fn serde_of<T: serde::de::DeserializeOwned>(t: &Table) -> R<T> {
    let mut v = toml_json(&Value::Table(t.clone()));
    if let Some(m) = v.as_object_mut() {
        m.insert("id".into(), serde_json::Value::from(0));
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// The grouped death treasure's row.
pub fn grouped_treasure_row(g: &GroupedTreasureDeath) -> R<Row> {
    serde_row(g)
}

pub fn grouped_treasure_of(t: &Table) -> R<GroupedTreasureDeath> {
    serde_of(t)
}

/// The loot policy's row.
pub fn loot_policy_row(p: &SourceLootPolicy) -> R<Row> {
    serde_row(p)
}

pub fn loot_policy_of(t: &Table) -> R<SourceLootPolicy> {
    serde_of(t)
}

/// A name as the rules row compares names: its letters and digits, lower case.
fn plain_name(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The skills or heritages a rules row names: their names, or their numbers back from names.
struct Names {
    what: &'static str,
    names: Vec<(u32, &'static str)>,
}

impl Names {
    fn skills() -> Self {
        use empyrean_entity::enums::Skill;
        Self {
            what: "skill",
            names: Skill::ALL
                .iter()
                .zip(Skill::NAMES)
                .map(|(s, n)| (u32::try_from(s.0).unwrap_or(0), *n))
                .collect(),
        }
    }
    fn heritages() -> Self {
        use empyrean_entity::enums::HeritageGroup;
        Self {
            what: "heritage",
            names: HeritageGroup::ALL
                .iter()
                .zip(HeritageGroup::NAMES)
                .map(|(h, n)| (u32::try_from(h.0).unwrap_or(0), *n))
                .collect(),
        }
    }
    fn name(&self, id: u64) -> serde_json::Value {
        self.names
            .iter()
            .find(|(i, _)| u64::from(*i) == id)
            .map_or_else(
                || serde_json::Value::from(id),
                |(_, n)| serde_json::Value::from(*n),
            )
    }
    fn id(&self, v: &serde_json::Value) -> R<u32> {
        let name = v
            .as_str()
            .ok_or_else(|| format!("a {} is named: {v}", self.what))?;
        self.names
            .iter()
            .find(|(_, n)| plain_name(n) == plain_name(name))
            .map(|(i, _)| *i)
            .ok_or_else(|| format!("{name:?} is not a {}", self.what))
    }
}

/// The world's rules as their row: skills and heritages by name, every property in one table.
pub fn rules_row(r: &WorldRules) -> R<Row> {
    Ok(json_row(&rules_json(r)?))
}

/// The world's rules as their row's fields, in JSON ([`rules_row`]).
pub fn rules_json(r: &WorldRules) -> R<serde_json::Map<String, serde_json::Value>> {
    let mut v = serde_json::to_value(r).map_err(|e| e.to_string())?;
    let m = v.as_object_mut().ok_or("the rules are not a table")?;
    for (key, names) in [
        ("creation_skills", Names::skills()),
        ("training_skills", Names::skills()),
        ("heritages", Names::heritages()),
    ] {
        if let Some(only) = m
            .get(key)
            .and_then(|a| a.get("only"))
            .and_then(|o| o.as_array())
        {
            let list = only
                .iter()
                .map(|i| names.name(i.as_u64().unwrap_or(0)))
                .collect();
            m.insert(key.into(), serde_json::Value::Array(list));
        }
    }
    if let Some(serde_json::Value::Object(p)) = m.remove("properties") {
        let flat: serde_json::Map<String, serde_json::Value> = p
            .into_iter()
            .filter_map(|(_, kind)| match kind {
                serde_json::Value::Object(o) => Some(o),
                _ => None,
            })
            .flatten()
            .collect();
        m.insert("properties".into(), serde_json::Value::Object(flat));
    }
    Ok(std::mem::take(m))
}

/// The world's rules read back from their row.
pub fn rules_of(t: &Table) -> R<WorldRules> {
    let mut v = toml_json(&Value::Table(t.clone()));
    let m = v.as_object_mut().ok_or("the rules are not a table")?;
    m.remove("prov");
    for (key, names) in [
        ("creation_skills", Names::skills()),
        ("training_skills", Names::skills()),
        ("heritages", Names::heritages()),
    ] {
        if let Some(serde_json::Value::Array(list)) = m.get(key) {
            let ids = list.iter().map(|n| names.id(n)).collect::<R<Vec<u32>>>()?;
            m.insert(key.into(), serde_json::json!({ "only": ids }));
        }
    }
    if let Some(serde_json::Value::Object(flat)) = m.remove("properties") {
        let mut p = serde_json::json!({ "bool": {}, "long": {}, "string": {} });
        for (k, x) in flat {
            let kind = match &x {
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Number(n) if n.is_i64() => "long",
                serde_json::Value::String(_) => "string",
                _ => {
                    return Err(format!(
                        "property {k} is a switch, a whole number or text, not {x}"
                    ))
                }
            };
            p[kind][k] = x;
        }
        m.insert("properties".into(), p);
    }
    let r: WorldRules = serde_json::from_value(v).map_err(|e| e.to_string())?;
    r.check()?;
    Ok(r)
}
