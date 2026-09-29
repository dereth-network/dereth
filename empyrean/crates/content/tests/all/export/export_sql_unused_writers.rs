//! Vectors: empyrean/fixtures/vectors/content_export_sql
//! HousePortal, TreasureDeath, TreasureWielded, Biota and Character SQL writers match ACE vectors
//! rebuilt from reflection dumps; null fill outside strings/comments; gameplay options as binary
//! literal.
//! Fixture: checked-in ACE JSON vectors and the local case adapters.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::vectors;
use empyrean_content::export::sql::*;
use empyrean_content::models::world::{HousePortal, TreasureDeath, TreasureWielded};
use empyrean_store::models::shard::*;
use serde_json::Value;

use super::export_sql::{dicts, load, run, writer, Checker, MODES};

/// A value of the harness's model dump.
trait Dump: Sized {
    fn dump(v: &Value) -> Self;
}

macro_rules! dump_int {
    ($($t:ty),*) => {$(
        impl Dump for $t {
            fn dump(v: &Value) -> Self {
                if let Some(i) = v.as_i64() {
                    <$t>::try_from(i).unwrap_or_else(|_| panic!("{v} out of range"))
                } else {
                    <$t>::try_from(v.as_u64().unwrap_or_else(|| panic!("{v} is not an integer")))
                        .unwrap_or_else(|_| panic!("{v} out of range"))
                }
            }
        }
    )*};
}
dump_int!(u8, u16, u32, u64, i8, i16, i32, i64);

impl Dump for f32 {
    fn dump(v: &Value) -> Self {
        vectors::f32_of(v).unwrap_or_else(|| panic!("{v} is not a float"))
    }
}

impl Dump for f64 {
    fn dump(v: &Value) -> Self {
        vectors::f64_of(v).unwrap_or_else(|| panic!("{v} is not a double"))
    }
}

impl Dump for bool {
    fn dump(v: &Value) -> Self {
        v.as_bool().unwrap_or_else(|| panic!("{v} is not a bool"))
    }
}

impl Dump for String {
    fn dump(v: &Value) -> Self {
        v.as_str()
            .unwrap_or_else(|| panic!("{v} is not a string"))
            .to_owned()
    }
}

impl Dump for DotNetDateTime {
    fn dump(v: &Value) -> Self {
        DotNetDateTime::from_ticks(i64::dump(v))
    }
}

impl<T: Dump> Dump for Option<T> {
    fn dump(v: &Value) -> Self {
        if v.is_null() {
            None
        } else {
            Some(T::dump(v))
        }
    }
}

impl<T: Dump> Dump for Vec<T> {
    fn dump(v: &Value) -> Self {
        v.as_array()
            .unwrap_or_else(|| panic!("{v} is not an array"))
            .iter()
            .map(T::dump)
            .collect()
    }
}

/// `impl Dump` for a row type from the listed fields (the dump's snake_case keys).
macro_rules! dump_struct {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl Dump for $t {
            fn dump(v: &Value) -> Self {
                let mut x = <$t>::default();
                $(
                    let key = stringify!($f).trim_start_matches("r#");
                    // The dump splits a digit run off a word (Int64 -> int_64); the Rust field keeps
                    // int64 in the one table name that has it.
                    let field = v
                        .get(key)
                        .or_else(|| v.get(&key.replace("int64", "int_64")))
                        .unwrap_or_else(|| panic!("{} has no {key}", stringify!($t)));
                    x.$f = Dump::dump(field);
                )*
                x
            }
        }
    };
}

dump_struct!(HousePortal {
    id,
    house_id,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
    last_modified
});
dump_struct!(TreasureDeath {
    id,
    treasure_type,
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
    last_modified,
});
dump_struct!(TreasureWielded {
    id,
    treasure_type,
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
    last_modified,
});

dump_struct!(Biota {
    id,
    weenie_class_id,
    weenie_type,
    populated_collection_flags,
    biota_properties_anim_part,
    biota_properties_attribute,
    biota_properties_attribute_2nd,
    biota_properties_body_part,
    biota_properties_book,
    biota_properties_book_page_data,
    biota_properties_bool,
    biota_properties_create_list,
    biota_properties_did,
    biota_properties_emote,
    biota_properties_enchantment_registry,
    biota_properties_event_filter,
    biota_properties_float,
    biota_properties_generator,
    biota_properties_iid,
    biota_properties_int,
    biota_properties_int64,
    biota_properties_palette,
    biota_properties_position,
    biota_properties_skill,
    biota_properties_spell_book,
    biota_properties_string,
    biota_properties_texture_map,
});
dump_struct!(BiotaPropertiesAnimPart {
    id,
    object_id,
    index,
    animation_id,
    order
});
dump_struct!(BiotaPropertiesAttribute {
    object_id,
    r#type,
    init_level,
    level_from_cp,
    cp_spent
});
dump_struct!(BiotaPropertiesAttribute2nd {
    object_id,
    r#type,
    init_level,
    level_from_cp,
    cp_spent,
    current_level
});
dump_struct!(BiotaPropertiesBodyPart {
    id,
    object_id,
    key,
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
dump_struct!(BiotaPropertiesBook {
    object_id,
    max_num_pages,
    max_num_chars_per_page
});
dump_struct!(BiotaPropertiesBookPageData {
    id,
    object_id,
    page_id,
    author_id,
    author_name,
    author_account,
    ignore_author,
    page_text
});
dump_struct!(BiotaPropertiesBool {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesCreateList {
    id,
    object_id,
    destination_type,
    weenie_class_id,
    stack_size,
    palette,
    shade,
    try_to_bond
});
dump_struct!(BiotaPropertiesDID {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesEmote {
    id,
    object_id,
    category,
    probability,
    weenie_class_id,
    style,
    substyle,
    quest,
    vendor_type,
    min_health,
    max_health,
    biota_properties_emote_action,
});
dump_struct!(BiotaPropertiesEmoteAction {
    id,
    emote_id,
    order,
    r#type,
    delay,
    extent,
    motion,
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
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
});
dump_struct!(BiotaPropertiesEnchantmentRegistry {
    object_id,
    enchantment_category,
    spell_id,
    layer_id,
    has_spell_set_id,
    spell_category,
    power_level,
    start_time,
    duration,
    caster_object_id,
    degrade_modifier,
    degrade_limit,
    last_time_degraded,
    stat_mod_type,
    stat_mod_key,
    stat_mod_value,
    spell_set_id,
});
dump_struct!(BiotaPropertiesEventFilter { object_id, event });
dump_struct!(BiotaPropertiesFloat {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesGenerator {
    id,
    object_id,
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
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z,
});
dump_struct!(BiotaPropertiesIID {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesInt {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesInt64 {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesPalette {
    id,
    object_id,
    sub_palette_id,
    offset,
    length,
    order
});
dump_struct!(BiotaPropertiesPosition {
    object_id,
    position_type,
    obj_cell_id,
    origin_x,
    origin_y,
    origin_z,
    angles_w,
    angles_x,
    angles_y,
    angles_z
});
dump_struct!(BiotaPropertiesSkill {
    object_id,
    r#type,
    level_from_pp,
    sac,
    pp,
    init_level,
    resistance_at_last_check,
    last_used_time
});
dump_struct!(BiotaPropertiesSpellBook {
    object_id,
    spell,
    probability
});
dump_struct!(BiotaPropertiesString {
    object_id,
    r#type,
    value
});
dump_struct!(BiotaPropertiesTextureMap {
    id,
    object_id,
    index,
    old_id,
    new_id,
    order
});

dump_struct!(Character {
    id,
    account_id,
    name,
    is_plussed,
    is_deleted,
    delete_time,
    last_login_timestamp,
    total_logins,
    character_options_1,
    character_options_2,
    gameplay_options,
    spellbook_filters,
    hair_texture,
    default_hair_texture,
    character_properties_contract_registry,
    character_properties_fill_comp_book,
    character_properties_friend_list,
    character_properties_quest_registry,
    character_properties_shortcut_bar,
    character_properties_spell_bar,
    character_properties_title_book,
});
dump_struct!(CharacterPropertiesContractRegistry {
    character_id,
    contract_id,
    delete_contract,
    set_as_display_contract
});
dump_struct!(CharacterPropertiesFillCompBook {
    character_id,
    spell_component_id,
    quantity_to_rebuy
});
dump_struct!(CharacterPropertiesFriendList {
    character_id,
    friend_id
});
dump_struct!(CharacterPropertiesQuestRegistry {
    character_id,
    quest_name,
    last_time_completed,
    num_times_completed
});
dump_struct!(CharacterPropertiesShortcutBar {
    character_id,
    shortcut_bar_index,
    shortcut_object_id
});
dump_struct!(CharacterPropertiesSpellBar {
    character_id,
    spell_bar_number,
    spell_bar_index,
    spell_id
});
dump_struct!(CharacterPropertiesTitleBook {
    character_id,
    title_id
});

/// Check one writer's cases: `file_name` of the model (or of its first row), `delete`, `insert`.
fn check<M: Dump, W: Default + std::ops::DerefMut<Target = SQLWriter>>(
    name: &str,
    file_name: impl Fn(&W, &M) -> Option<String>,
    delete: impl Fn(&W, &M, &mut SqlOut) -> Result<(), SqlWriterError>,
    insert: impl Fn(&W, &M, &mut SqlOut) -> Result<(), SqlWriterError>,
) {
    let d = dicts();
    let mut c = Checker::new();
    for case in &load(name).cases {
        let case_name = case.input["name"].as_str().unwrap();
        let model = M::dump(&case.input["model"]);
        for (mode, named) in MODES {
            let w: W = writer(&d, named);
            let want = &case.output[mode];
            let at = |part: &str| format!("{case_name}/{mode}/{part}");
            match file_name(&w, &model) {
                Some(f) => c.file_name(&at("file_name"), &want["file_name"], f),
                None => {
                    assert!(
                        want["file_name"].is_null(),
                        "{}: ACE names a file",
                        at("file_name")
                    )
                }
            }
            let (out, err) = run(&d, |o| delete(&w, &model, o));
            c.text(&at("delete"), &want["delete"], &out, err);
            let (out, err) = run(&d, |o| insert(&w, &model, o));
            c.text(&at("insert"), &want["insert"], &out, err);
        }
    }
    c.finish(name);
}

#[test]
fn house_portal_writer_matches_ace() {
    check::<Vec<HousePortal>, HousePortalSQLWriter>(
        "house_portal",
        |w, m| m.first().map(|p| w.get_default_file_name(p)),
        |w, m, o| w.create_sql_delete_statement(m, o),
        |w, m, o| w.create_sql_insert_statement(m, o),
    );
}

#[test]
fn treasure_death_writer_matches_ace() {
    check::<TreasureDeath, TreasureDeathSQLWriter>(
        "treasure_death",
        |w, m| Some(w.get_default_file_name(m)),
        |w, m, o| {
            w.create_sql_delete_statement(m, o);
            Ok(())
        },
        |w, m, o| {
            w.create_sql_insert_statement(m, o);
            Ok(())
        },
    );
}

#[test]
fn treasure_wielded_writer_matches_ace() {
    check::<Vec<TreasureWielded>, TreasureWieldedSQLWriter>(
        "treasure_wielded",
        |w, m| m.first().map(|t| w.get_default_file_name(t)),
        |w, m, o| w.create_sql_delete_statement(m, o),
        |w, m, o| w.create_sql_insert_statement(m, o),
    );
}

#[test]
fn biota_writer_matches_ace() {
    check::<Biota, BiotaSQLWriter>(
        "biota",
        |w, m| Some(w.get_default_file_name(m)),
        |w, m, o| {
            w.create_sql_delete_statement(m, o);
            Ok(())
        },
        |w, m, o| w.create_sql_insert_statement(m, o),
    );
}

#[test]
fn character_writer_matches_ace() {
    check::<Character, CharacterSQLWriter>(
        "character",
        |w, m| Some(w.get_default_file_name(m)),
        |w, m, o| {
            w.create_sql_delete_statement(m, o);
            Ok(())
        },
        |w, m, o| {
            w.create_sql_insert_statement(m, o);
            Ok(())
        },
    );
}

#[test]
fn null_fields_are_filled_outside_strings_and_comments_only() {
    // V364 (a fix): ACE's replacements also ran inside string values and comments
    let f = sql_writer::SQLWriter::fix_null_fields;
    assert_eq!(
        f("VALUES (1, , 'a, , b', , )"),
        "VALUES (1, NULL, 'a, , b', NULL, NULL)"
    );
    assert_eq!(
        f("VALUES (1, 'it''s, ) x', 2) /* Drudge, , Slinker */"),
        "VALUES (1, 'it''s, ) x', 2) /* Drudge, , Slinker */"
    );
    assert_eq!(
        f("VALUES (1, 2) /*  */;"),
        "VALUES (1, 2);",
        "an empty comment still goes"
    );
    assert_eq!(
        f("VALUES (1, ' /*  */', , 3)"),
        "VALUES (1, ' /*  */', NULL, 3)"
    );
    assert_eq!(f("VALUES (1, 2 / 3, , 4)"), "VALUES (1, 2 / 3, NULL, 4)");
}

#[test]
fn a_characters_gameplay_options_are_written_as_a_binary_literal() {
    // V364 (a fix): ACE wrote the bare word System.Byte[]
    let character = Character {
        id: 2,
        account_id: 3,
        name: "Opts".to_owned(),
        gameplay_options: Some(vec![1, 2, 0xAB]),
        ..Character::default()
    };
    let mut out = SqlOut::new("\n");
    CharacterSQLWriter::default().create_sql_insert_statement(&character, &mut out);
    assert!(out.text.contains(", X'0102AB', "), "{}", out.text);
}
