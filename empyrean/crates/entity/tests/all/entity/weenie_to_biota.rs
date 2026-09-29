//! ACE: Source/ACE.Entity/Adapter/WeenieConverter.cs::ConvertToBiota
//! ConvertToBiota copies scalars and compacts dictionaries, empty collections per flag, records
//! go through ACE Clone, referenced collections shared until written.
//! Fixture: enum values and synthetic entity records.

use std::sync::Arc;

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::convert_to_biota;
use empyrean_entity::enums::{
    CombatBodyPart, DamageType, EmoteCategory, PositionType, PropertyInt, PropertyString,
    WeenieType,
};
use empyrean_entity::models::{
    PropertiesBodyPart, PropertiesBook, PropertiesCreateList, PropertiesEmote,
    PropertiesEmoteAction, PropertiesGenerator, PropertiesPosition, Weenie,
};

fn emote(category: EmoteCategory, record: u32) -> PropertiesEmote {
    PropertiesEmote {
        database_record_id: record,
        category,
        probability: 0.5,
        quest: Some("q".to_owned()),
        object: Some(Arc::new(Weenie::default())),
        properties_emote_action: vec![PropertiesEmoteAction {
            database_record_id: record + 1,
            message: Some("hello".to_owned()),
            amount: Some(3),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn weenie() -> Weenie {
    let mut ints = DotNetDict::new();
    ints.insert(PropertyInt::ItemType, 1);
    ints.insert(PropertyInt::Value, 2);
    ints.insert(PropertyInt::Level, 3);
    // Leaves a free slot in the weenie's dictionary.
    ints.remove(&PropertyInt::Value);

    let mut strings = DotNetDict::new();
    strings.insert(PropertyString::Name, "Drudge".to_owned());

    let mut positions = DotNetDict::new();
    positions.insert(
        PositionType::Location,
        PropertiesPosition {
            obj_cell_id: 0xA9B4_0001,
            position_x: 5.0,
            rotation_w: 1.0,
            ..Default::default()
        },
    );

    let mut body = DotNetDict::new();
    body.insert(
        CombatBodyPart::Head,
        PropertiesBodyPart {
            d_type: DamageType::Slash,
            d_val: 4,
            ..Default::default()
        },
    );

    let events: DotNetHashSet<i32> = [7, 8].into_iter().collect();

    Weenie {
        weenie_class_id: 1234,
        class_name: Some("drudge".to_owned()),
        weenie_type: WeenieType::Creature,
        properties_int: Some(ints),
        properties_string: Some(strings),
        properties_position: Some(positions),
        properties_bool: Some(DotNetDict::new()),
        properties_create_list: Some(Arc::new(vec![PropertiesCreateList {
            database_record_id: 55,
            weenie_class_id: 9,
            stack_size: 2,
            ..Default::default()
        }])),
        properties_emote: Some(Arc::new(vec![emote(EmoteCategory::Use, 60)])),
        properties_event_filter: Some(Arc::new(events)),
        properties_generator: Some(Arc::new(vec![PropertiesGenerator {
            database_record_id: 70,
            weenie_class_id: 11,
            ..Default::default()
        }])),
        properties_body_part: Some(Arc::new(body)),
        properties_book: Some(PropertiesBook {
            max_num_pages: 3,
            max_num_chars_per_page: 100,
        }),
        ..Default::default()
    }
}

#[test]
fn copies_scalars_and_compacts_dictionaries() {
    let w = weenie();
    let mut b = convert_to_biota(&w, 0x8000_0001, false, false);
    assert_eq!(
        (b.id, b.weenie_class_id, b.weenie_type),
        (0x8000_0001, 1234, WeenieType::Creature)
    );
    assert_eq!(b.get_property(PropertyInt::Level), Some(3));
    assert_eq!(b.get_name().as_deref(), Some("Drudge"));
    assert_eq!(
        b.get_position(PositionType::Location).unwrap().position_x,
        5.0
    );

    // `new Dictionary(source)` drops the free slot: a new key appends in the biota but reuses
    // the slot in the weenie.
    let mut wi = w.properties_int.clone().unwrap();
    wi.insert(PropertyInt::Mass, 4);
    assert_eq!(
        wi.keys().copied().collect::<Vec<_>>(),
        [PropertyInt::ItemType, PropertyInt::Mass, PropertyInt::Level]
    );
    b.set_property(PropertyInt::Mass, 4);
    assert_eq!(
        b.properties_int
            .as_ref()
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [PropertyInt::ItemType, PropertyInt::Level, PropertyInt::Mass]
    );

    // The book is cloned even though books have no "empty" state.
    assert_eq!(
        b.properties_book,
        Some(PropertiesBook {
            max_num_pages: 3,
            max_num_chars_per_page: 100
        })
    );
}

#[test]
fn empty_collections_follow_instantiate_empty_collections() {
    let w = weenie();
    assert!(convert_to_biota(&w, 1, false, false)
        .properties_bool
        .is_none());
    let b = convert_to_biota(&w, 1, true, false);
    assert!(b.properties_bool.as_ref().is_some_and(DotNetDict::is_empty));
    // A null weenie collection stays null either way.
    assert!(b.properties_float.is_none());
    assert!(b.properties_spell_book.is_none());
}

#[test]
fn copied_records_go_through_ace_clone() {
    let w = weenie();
    let b = convert_to_biota(&w, 1, false, false);

    // Not shared, and Clone() reset DatabaseRecordId (and the emote's Object).
    assert!(!Arc::ptr_eq(
        b.properties_emote.as_ref().unwrap(),
        w.properties_emote.as_ref().unwrap()
    ));
    let cl = &b.properties_create_list.as_ref().unwrap()[0];
    assert_eq!(
        (cl.database_record_id, cl.weenie_class_id, cl.stack_size),
        (0, 9, 2)
    );
    let e = &b.properties_emote.as_ref().unwrap()[0];
    assert_eq!(
        (e.database_record_id, e.category, e.quest.as_deref()),
        (0, EmoteCategory::Use, Some("q"))
    );
    assert!(e.object.is_none());
    assert_eq!(e.properties_emote_action[0].database_record_id, 0);
    assert_eq!(
        e.properties_emote_action[0].message.as_deref(),
        Some("hello")
    );
    assert_eq!(e.properties_emote_action[0].amount, Some(3));
    assert_eq!(
        b.properties_generator.as_ref().unwrap()[0].database_record_id,
        0
    );
    assert_eq!(
        b.properties_event_filter
            .as_ref()
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [7, 8]
    );
    assert_eq!(
        b.properties_body_part
            .as_ref()
            .unwrap()
            .get(&CombatBodyPart::Head)
            .unwrap()
            .d_val,
        4
    );
}

#[test]
fn referenced_collections_are_shared_until_written() {
    let w = Arc::new(weenie());
    let mut b1 = convert_to_biota(&w, 1, false, true);
    let b2 = convert_to_biota(&w, 2, false, true);

    // A new biota sees the weenie's own collections (records untouched, ids kept).
    assert!(Arc::ptr_eq(
        b1.properties_emote.as_ref().unwrap(),
        w.properties_emote.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        b1.properties_body_part.as_ref().unwrap(),
        w.properties_body_part.as_ref().unwrap()
    ));
    assert_eq!(
        b1.properties_create_list.as_ref().unwrap()[0].database_record_id,
        55
    );
    assert!(b1.properties_emote.as_ref().unwrap()[0].object.is_some());

    // Mutating the biota copies on write and never reaches the weenie or another biota.
    b1.properties_emote_mut().unwrap()[0].probability = 0.9;
    b1.properties_emote_mut()
        .unwrap()
        .push(emote(EmoteCategory::Death, 80));
    b1.properties_create_list_mut().unwrap().clear();
    b1.properties_event_filter_mut().unwrap().insert(9);
    b1.properties_generator_mut().unwrap()[0].weenie_class_id = 12;
    b1.properties_body_part_mut()
        .unwrap()
        .get_mut(&CombatBodyPart::Head)
        .unwrap()
        .d_val = 40;

    assert_eq!(w.properties_emote.as_ref().unwrap().len(), 1);
    assert_eq!(w.properties_emote.as_ref().unwrap()[0].probability, 0.5);
    assert_eq!(w.properties_create_list.as_ref().unwrap().len(), 1);
    assert_eq!(w.properties_event_filter.as_ref().unwrap().len(), 2);
    assert_eq!(
        w.properties_generator.as_ref().unwrap()[0].weenie_class_id,
        11
    );
    assert_eq!(
        w.properties_body_part
            .as_ref()
            .unwrap()
            .get(&CombatBodyPart::Head)
            .unwrap()
            .d_val,
        4
    );
    assert!(Arc::ptr_eq(
        b2.properties_emote.as_ref().unwrap(),
        w.properties_emote.as_ref().unwrap()
    ));

    // The biota sees its writes, and the copy kept the shared records' ids.
    let e = b1.properties_emote.as_ref().unwrap();
    assert_eq!(
        (e.len(), e[0].probability, e[0].database_record_id),
        (2, 0.9, 60)
    );
    assert_eq!(
        b1.properties_body_part
            .as_ref()
            .unwrap()
            .get(&CombatBodyPart::Head)
            .unwrap()
            .d_val,
        40
    );

    // Non-common collections are always copied, even when referencing.
    let mut b3 = convert_to_biota(&w, 3, false, true);
    b3.set_property(PropertyInt::Level, 99);
    assert_eq!(w.get_property(PropertyInt::Level), Some(3));
}

#[test]
fn referencing_passes_null_and_empty_through() {
    let w = Weenie {
        properties_emote: Some(Arc::new(Vec::new())),
        ..Default::default()
    };
    let b = convert_to_biota(&w, 1, false, true);
    assert!(b.properties_emote.as_ref().is_some_and(|e| e.is_empty()));
    assert!(b.properties_generator.is_none());
    // Without referencing, the empty list is dropped.
    assert!(convert_to_biota(&w, 1, false, false)
        .properties_emote
        .is_none());
}
