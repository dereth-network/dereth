//! ACE: Source/ACE.Database/Adapter/WeenieConverter.cs::ConvertToEntityWeenie
//! ConvertToEntityWeenie keeps row order, sorts emote actions and book pages, converts creature
//! tables by enum key, keeps empty collections null, first slot/last value on repeats.
//! Fixture: synthetic world records, SQL dumps and JSON documents.

use empyrean_content::adapter::convert_to_entity_weenie;
use empyrean_content::models::world::*;
use empyrean_entity::enums::{
    CombatBodyPart, DamageType, DestinationType, EmoteCategory, MotionCommand, MotionStance,
    PlayScript, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64, PropertyString, RegenLocationType,
    RegenerationType, Skill, SkillAdvancementClass, Sound, VendorType, WeenieType,
};

use crate::fixture;

fn creature_row() -> Weenie {
    fixture::db()
        .base()
        .get_all_weenies()
        .into_iter()
        .find(|w| w.class_id == 100)
        .unwrap()
}

#[test]
fn base_fields_and_scalar_bags_keep_row_order() {
    let e = convert_to_entity_weenie(&creature_row(), false);
    assert_eq!(e.weenie_class_id, 100);
    assert_eq!(e.class_name.as_deref(), Some("drudgetest"));
    assert_eq!(e.weenie_type, WeenieType::Creature);
    let ints: Vec<(PropertyInt, i32)> = e
        .properties_int
        .as_ref()
        .unwrap()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(ints, [(PropertyInt::Level, 5), (PropertyInt::ItemType, 16)]);
    assert_eq!(
        e.properties_int64.as_ref().unwrap().get(&PropertyInt64(1)),
        Some(&-9_000_000_000)
    );
    assert_eq!(
        e.properties_float.as_ref().unwrap().get(&PropertyFloat(39)),
        Some(&1.5)
    );
    assert_eq!(
        e.properties_bool.as_ref().unwrap().get(&PropertyBool(1)),
        Some(&true)
    );
    assert_eq!(
        e.properties_did
            .as_ref()
            .unwrap()
            .get(&PropertyDataId::Setup),
        Some(&33_554_433)
    );
    assert_eq!(
        e.properties_string
            .as_ref()
            .unwrap()
            .get(&PropertyString::Name)
            .map(String::as_str),
        Some("Drudge 'Tester' \"(x),(y)\"")
    );
    let pos = e
        .properties_position
        .as_ref()
        .unwrap()
        .get(&PositionType::Location)
        .unwrap();
    assert_eq!(
        (
            pos.obj_cell_id,
            pos.position_x,
            pos.rotation_w,
            pos.rotation_z
        ),
        (2_847_146_009, 84.0, 0.996, -0.08)
    );
    // Spell book: the dictionary keeps the rows' order (id order here, spell 2000 first).
    let spells: Vec<(i32, f32)> = e
        .properties_spell_book
        .as_ref()
        .unwrap()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(spells, [(2000, 2.5), (1000, 2.25)]);
    assert!(e.properties_iid.is_some());
    // Collections the weenie has no rows for stay null.
    assert!(e.properties_book.is_none() && e.properties_book_page_data.is_none());
}

#[test]
fn emotes_sort_actions_by_order_and_cast_like_csharp() {
    let e = convert_to_entity_weenie(&creature_row(), false);
    let emotes = e.properties_emote.as_ref().unwrap();
    assert_eq!(emotes.len(), 2);
    assert_eq!(emotes[0].category, EmoteCategory(1));
    let messages: Vec<Option<&str>> = emotes[0]
        .properties_emote_action
        .iter()
        .map(|a| a.message.as_deref())
        .collect();
    assert_eq!(
        messages,
        [Some("first"), Some("second")],
        "OrderBy(r => r.Order)"
    );
    let a = &emotes[0].properties_emote_action[0];
    assert_eq!(a.motion, Some(MotionCommand(318_767_235)));
    assert_eq!(
        a.p_script,
        Some(PlayScript(u32::MAX)),
        "(PlayScript?)(int?)-1"
    );
    assert_eq!(a.sound, Some(Sound(u32::MAX - 1)));
    assert_eq!(a.destination_type, Some(-3));
    assert_eq!(
        (a.min_64, a.hero_xp_64, a.display),
        (Some(-5_000_000_000), Some(7_000_000_000), Some(true))
    );
    let b = &emotes[1];
    assert_eq!(b.style, Some(MotionStance(2_147_483_709)));
    assert_eq!(b.substyle, Some(MotionCommand(1_090_519_043)));
    assert_eq!(b.vendor_type, Some(VendorType(-2)));
    assert_eq!(
        (b.probability, b.min_health, b.max_health),
        (0.5, Some(0.25), Some(0.75))
    );
    assert_eq!(
        b.database_record_id, 0,
        "the converter does not set DatabaseRecordId"
    );
}

#[test]
fn creature_tables_convert_with_their_enum_keys() {
    let e = convert_to_entity_weenie(&creature_row(), false);
    let attrs: Vec<PropertyAttribute> = e
        .properties_attribute
        .as_ref()
        .unwrap()
        .keys()
        .copied()
        .collect();
    assert_eq!(
        attrs,
        [PropertyAttribute(2), PropertyAttribute::Strength],
        "row order, not key order"
    );
    assert_eq!(
        e.properties_attribute_2nd
            .as_ref()
            .unwrap()
            .get(&PropertyAttribute2nd::MaxHealth)
            .unwrap()
            .current_level,
        15
    );
    let bp = e
        .properties_body_part
        .as_ref()
        .unwrap()
        .get(&CombatBodyPart::Head)
        .unwrap();
    assert_eq!(
        (
            bp.d_type,
            bp.d_val,
            bp.d_var,
            bp.base_armor,
            bp.armor_vs_nether
        ),
        (DamageType(4), 2, 0.75, 5, 13)
    );
    assert_eq!((bp.bh, bp.hlf, bp.lrb), (1, 0.1, 1.2));
    let sk = e.properties_skill.as_ref().unwrap().get(&Skill(6)).unwrap();
    assert_eq!((sk.sac, sk.init_level), (SkillAdvancementClass(2), 10));
    let cl = e.properties_create_list.as_ref().unwrap();
    assert_eq!(cl[1].destination_type, DestinationType(8));
    assert_eq!(
        (
            cl[1].weenie_class_id,
            cl[1].palette,
            cl[1].shade,
            cl[1].try_to_bond
        ),
        (628, -3, 0.5, true)
    );
    let g = e.properties_generator.as_ref().unwrap();
    assert_eq!(
        (
            g[0].when_create,
            g[0].where_create,
            g[0].delay,
            g[0].obj_cell_id
        ),
        (RegenerationType(2), RegenLocationType(4), None, None)
    );
    assert_eq!(
        (
            g[1].delay,
            g[1].stack_size,
            g[1].palette_id,
            g[1].obj_cell_id
        ),
        (Some(120.0), Some(5), Some(3), Some(2_847_146_009))
    );
    let ev: Vec<i32> = e
        .properties_event_filter
        .as_ref()
        .unwrap()
        .iter()
        .copied()
        .collect();
    assert_eq!(ev, [94, 414]);
    assert_eq!(
        e.properties_anim_part.as_ref().unwrap()[0].animation_id,
        16_777_217
    );
    assert_eq!(e.properties_palette.as_ref().unwrap()[0].length, 24);
    let tm: Vec<u8> = e
        .properties_texture_map
        .as_ref()
        .unwrap()
        .iter()
        .map(|t| t.part_index)
        .collect();
    assert_eq!(
        tm,
        [9, 1],
        "the converter keeps whatever order the rows came in"
    );
}

#[test]
fn book_pages_sort_by_page_id() {
    let row = fixture::db()
        .base()
        .get_all_weenies()
        .into_iter()
        .find(|w| w.class_id == 200)
        .unwrap();
    let e = convert_to_entity_weenie(&row, false);
    assert_eq!(
        e.properties_book
            .as_ref()
            .map(|b| (b.max_num_pages, b.max_num_chars_per_page)),
        Some((2, 1000))
    );
    let pages = e.properties_book_page_data.as_ref().unwrap();
    let texts: Vec<&str> = pages
        .iter()
        .map(|p| p.page_text.as_deref().unwrap())
        .collect();
    assert_eq!(texts, ["page one", "page two"]);
    assert!(pages[0].ignore_author);
    assert_eq!(pages[1].author_id, u32::MAX);
    assert_eq!(pages[1].author_account.as_deref(), Some("prewritten"));
}

#[test]
fn empty_collections_stay_null_unless_asked_for() {
    let bare = Weenie {
        class_id: 7,
        class_name: "bare".into(),
        r#type: 1,
        ..Default::default()
    };
    let e = convert_to_entity_weenie(&bare, false);
    assert!(
        e.properties_int.is_none()
            && e.properties_emote.is_none()
            && e.properties_event_filter.is_none()
    );
    assert!(e.properties_book.is_none());
    let e = convert_to_entity_weenie(&bare, true);
    assert!(e.properties_int.as_ref().is_some_and(|d| d.is_empty()));
    assert!(e.properties_emote.as_ref().is_some_and(|v| v.is_empty()));
    assert!(e
        .properties_event_filter
        .as_ref()
        .is_some_and(|s| s.is_empty()));
    assert!(
        e.properties_book.is_none(),
        "the book is a single row, never instantiated empty"
    );
}

#[test]
fn a_repeated_key_keeps_its_first_slot_and_last_value() {
    let w = Weenie::new(8, "dup", WeenieType::Generic)
        .with_int(PropertyInt::Level, 1)
        .with_int(PropertyInt::ItemType, 2)
        .with_int(PropertyInt::Level, 3);
    let e = convert_to_entity_weenie(&w, false);
    let ints: Vec<(PropertyInt, i32)> = e
        .properties_int
        .as_ref()
        .unwrap()
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(ints, [(PropertyInt::Level, 3), (PropertyInt::ItemType, 2)]);
}
