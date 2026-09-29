//! Vectors: local ACE biota and weenie property-bag cases in this module
//! Biota/Weenie property bags enumerate in .NET order, round-trip, spell book, skills, house
//! permissions, list extensions, book pages, allegiance records as ACE's extensions.
//! Fixture: enum values and synthetic entity records.

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    ItemType, PositionType, PropertyBool, PropertyFloat, PropertyInt, PropertyInt64,
    PropertyString, Skill, SkillAdvancementClass, WeenieType,
};
use empyrean_entity::models::properties_allegiance_extensions as allegiance;
use empyrean_entity::models::properties_anim_part_extensions as anim_parts;
use empyrean_entity::models::properties_book_page_data_extensions as pages;
use empyrean_entity::models::{
    Biota, PropertiesAllegiance, PropertiesAnimPart, PropertiesBookPageData, PropertiesPosition,
    Weenie,
};
use empyrean_entity::Position;

fn keys(b: &Biota) -> Vec<PropertyInt> {
    b.properties_int
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default()
}

#[test]
fn int_bag_enumerates_in_dotnet_order_after_sets_and_removes() {
    let mut b = Biota::default();
    assert_eq!(b.get_property(PropertyInt::Level), None);
    assert!(b.properties_int.is_none());

    assert!(b.set_property(PropertyInt::Level, 5));
    assert!(b.set_property(PropertyInt::Value, 100));
    assert!(b.set_property(PropertyInt::ItemType, 1));
    assert_eq!(
        keys(&b),
        [
            PropertyInt::Level,
            PropertyInt::Value,
            PropertyInt::ItemType
        ]
    );

    // Overwrite keeps the slot; an unchanged set reports no change.
    assert!(b.set_property(PropertyInt::Level, 6));
    assert!(!b.set_property(PropertyInt::Level, 6));
    assert_eq!(
        keys(&b),
        [
            PropertyInt::Level,
            PropertyInt::Value,
            PropertyInt::ItemType
        ]
    );

    // Remove frees slot 1; the next insert takes it, the one after appends.
    assert!(b.try_remove_property(PropertyInt::Value));
    assert!(!b.try_remove_property(PropertyInt::Value));
    assert!(b.set_property(PropertyInt::StackSize, 3));
    assert!(b.set_property(PropertyInt::MaxStackSize, 9));
    assert_eq!(
        keys(&b),
        [
            PropertyInt::Level,
            PropertyInt::StackSize,
            PropertyInt::ItemType,
            PropertyInt::MaxStackSize
        ]
    );

    // Two removes: most recently freed first.
    assert!(b.try_remove_property(PropertyInt::Level));
    assert!(b.try_remove_property(PropertyInt::ItemType));
    assert!(b.set_property(PropertyInt::Value, 1));
    assert!(b.set_property(PropertyInt::Mass, 2));
    assert_eq!(
        keys(&b),
        [
            PropertyInt::Mass,
            PropertyInt::StackSize,
            PropertyInt::Value,
            PropertyInt::MaxStackSize
        ]
    );
    assert_eq!(b.get_property(PropertyInt::Value), Some(1));
}

#[test]
fn every_bag_type_round_trips() {
    let mut b = Biota::default();
    assert!(b.set_property(PropertyBool::Stuck, true));
    assert!(b.set_property(PropertyFloat::HeartbeatInterval, 5.0));
    assert!(b.set_property(PropertyInt64::TotalExperience, 1 << 40));
    assert!(b.set_property(PropertyString::Name, "Drudge".to_owned()));
    assert!(!b.set_property(PropertyString::Name, "Drudge".to_owned()));
    assert_eq!(b.get_property(PropertyBool::Stuck), Some(true));
    assert_eq!(b.get_property(PropertyFloat::HeartbeatInterval), Some(5.0));
    assert_eq!(
        b.get_property(PropertyInt64::TotalExperience),
        Some(1 << 40)
    );
    assert_eq!(b.get_name().as_deref(), Some("Drudge"));
    // A NaN float is never equal to the stored value, so it always counts as changed.
    assert!(b.set_property(PropertyFloat::HeartbeatInterval, f64::NAN));
    assert!(b.set_property(PropertyFloat::HeartbeatInterval, f64::NAN));
    // Removing from a null bag.
    assert!(!b.try_remove_property(PropertyInt::Level));
}

#[test]
#[should_panic(expected = "NullReferenceException")]
fn get_name_without_strings_throws_like_ace() {
    let _ = Biota::default().get_name();
}

#[test]
fn positions_relative_destination_is_raw() {
    let mut b = Biota::default();
    assert!(b.get_position(PositionType::Location).is_none());
    let rec = PropertiesPosition {
        obj_cell_id: 0xA9B4_0001,
        position_x: 300.0,
        rotation_w: 1.0,
        ..Default::default()
    };
    b.set_property_position(PositionType::Location, rec.clone());
    b.set_property_position(PositionType::RelativeDestination, rec.clone());
    // Location goes through the normal constructor and is re-homed; RelativeDestination is not.
    assert_eq!(
        b.get_position(PositionType::Location).unwrap().cell(),
        0xAAB4_0021
    );
    let rel = b.get_position(PositionType::RelativeDestination).unwrap();
    assert_eq!((rel.cell(), rel.position_x), (0xA9B4_0001, 300.0));
    assert_eq!(b.get_property_position(PositionType::Location), Some(&rec));

    let p = Position::from_components(0xA9B4_0100, 1.0, 2.0, 3.0, 0.1, 0.2, 0.3, 0.4, false);
    b.set_position(PositionType::Home, &p);
    let stored = b.get_property_position(PositionType::Home).unwrap();
    assert_eq!(
        (
            stored.obj_cell_id,
            stored.position_z,
            stored.rotation_w,
            stored.rotation_x
        ),
        (0xA9B4_0100, 3.0, 0.4, 0.1)
    );
    assert!(b.try_remove_property_position(PositionType::Home));
    assert!(!b.try_remove_property_position(PositionType::Home));

    // The weenie version never builds a relative position.
    let w = Weenie {
        properties_position: b.properties_position.clone(),
        ..Default::default()
    };
    assert_eq!(
        w.get_position(PositionType::RelativeDestination)
            .unwrap()
            .cell(),
        0xAAB4_0021
    );
}

#[test]
fn spell_book() {
    let mut b = Biota::default();
    assert!(!b.has_known_spell());
    assert!(b.get_known_spells_ids().is_empty());
    assert_eq!(b.get_or_add_known_spell(10, 2.0), (2.0, true));
    assert_eq!(b.get_or_add_known_spell(10, 0.5), (2.0, false));
    assert_eq!(b.get_or_add_known_spell(20, 0.5), (0.5, true));
    assert_eq!(b.get_or_add_known_spell(30, 0.25), (0.25, true));
    assert!(b.try_remove_known_spell(20));
    assert_eq!(b.get_or_add_known_spell(40, 1.0), (1.0, true));
    assert_eq!(b.get_known_spells_ids(), [10, 40, 30]);
    assert_eq!(b.get_known_spells_probabilities(), [2.0, 1.0, 0.25]);
    assert_eq!(b.get_known_spells_ids_where(|s| s > 15), [40, 30]);
    assert!(b.spell_is_known(40) && !b.spell_is_known(20));
    let clone = b.clone_spells();
    assert_eq!(clone.keys().copied().collect::<Vec<_>>(), [10, 40, 30]);
    let m: DotNetHashSet<i32> = [30, 10, 99].into_iter().collect();
    assert_eq!(
        b.get_matching_spells(&m)
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        [10, 30]
    );
    b.clear_spells();
    assert!(!b.has_known_spell());
    assert!(b.properties_spell_book.is_some());
}

#[test]
fn skills_are_added_once_and_returned_by_reference() {
    let mut b = Biota::default();
    assert!(b.get_skill(Skill::Axe).is_none());
    let (s, added) = b.get_or_add_skill(Skill::Axe);
    assert!(added);
    s.sac = SkillAdvancementClass::Trained;
    let (s, added) = b.get_or_add_skill(Skill::Axe);
    assert!(!added);
    assert_eq!(s.sac, SkillAdvancementClass::Trained);
    assert_eq!(
        b.get_skill(Skill::Axe).unwrap().sac,
        SkillAdvancementClass::Trained
    );
}

#[test]
fn house_permissions() {
    let mut b = Biota::default();
    assert!(!b.has_house_guest(1));
    assert_eq!(b.get_house_guest_storage_permission(1), None);
    assert!(!b.remove_house_guest(1));
    b.add_or_update_house_guest(1, false);
    b.add_or_update_house_guest(2, true);
    b.add_or_update_house_guest(1, true);
    assert_eq!(b.get_house_guest_storage_permission(1), Some(true));
    assert!(b.remove_house_guest(1));
    b.add_or_update_house_guest(3, false);
    let perms = b.clone_house_permissions();
    assert_eq!(
        perms.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
        [(3, false), (2, true)]
    );
}

#[test]
fn weenie_helpers() {
    let mut w = Weenie {
        weenie_type: WeenieType::Generic,
        ..Default::default()
    };
    assert_eq!(w.get_item_type(), ItemType(0));
    assert_eq!(w.get_max_stack_size(), 1);
    assert_eq!(w.get_stack_unit_encumbrance(), 0);
    assert!(!w.is_stuck() && !w.is_vendor_service() && !w.requires_backpack_slot_or_is_container());

    let mut ints = DotNetDict::new();
    ints.insert(PropertyInt::ItemType, 0x80);
    ints.insert(PropertyInt::MaxStackSize, 100);
    ints.insert(PropertyInt::StackUnitEncumbrance, 2);
    ints.insert(PropertyInt::EncumbranceVal, 50);
    ints.insert(PropertyInt::Value, 7);
    w.properties_int = Some(ints);
    // Not stackable: MaxStackSize and StackUnitEncumbrance are ignored.
    assert_eq!(w.get_max_stack_size(), 1);
    assert_eq!(w.get_stack_unit_encumbrance(), 50);
    assert_eq!(w.get_item_type(), ItemType(0x80));
    assert_eq!(w.get_value(), Some(7));
    assert_eq!(w.get_max_structure(), None);

    for t in [
        WeenieType::Stackable,
        WeenieType::Coin,
        WeenieType::SpellComponent,
        WeenieType::Missile,
    ] {
        w.weenie_type = t;
        assert!(w.is_stackable());
        assert_eq!(w.get_max_stack_size(), 100);
        assert_eq!(w.get_stack_unit_encumbrance(), 2);
    }
    w.weenie_type = WeenieType::Container;
    assert!(!w.is_stackable());
    assert!(w.requires_backpack_slot_or_is_container());

    let mut strings = DotNetDict::new();
    strings.insert(PropertyString::Name, "Sword".to_owned());
    w.properties_string = Some(strings);
    assert_eq!(w.get_plural_name(), "Swords");
    w.properties_string
        .as_mut()
        .unwrap()
        .insert(PropertyString::PluralName, "Many Swords".to_owned());
    assert_eq!(w.get_plural_name(), "Many Swords");
    assert_eq!(w.get_name().as_deref(), Some("Sword"));
}

#[test]
fn list_extensions() {
    assert_eq!(anim_parts::get_count(None), 0);
    assert!(anim_parts::clone(None).is_none());
    let list = vec![
        PropertiesAnimPart {
            index: 1,
            animation_id: 2,
        },
        PropertiesAnimPart {
            index: 3,
            animation_id: 4,
        },
    ];
    assert_eq!(anim_parts::get_count(Some(&list)), 2);
    assert_eq!(anim_parts::clone(Some(&list)).unwrap(), list);
    let mut dest = vec![PropertiesAnimPart {
        index: 9,
        animation_id: 9,
    }];
    anim_parts::copy_to(Some(&list), &mut dest);
    assert_eq!(dest.len(), 3);
    assert_eq!(dest[1], list[0]);
}

#[test]
fn book_pages() {
    let page = |t: &str| PropertiesBookPageData {
        page_text: Some(t.to_owned()),
        ..Default::default()
    };
    let mut v = Vec::new();
    assert_eq!(pages::add_page(&mut v, page("a")), 0);
    assert_eq!(pages::add_page(&mut v, page("b")), 1);
    assert_eq!(pages::get_page_count(Some(&v)), 2);
    assert_eq!(
        pages::get_page(Some(&v), 1).unwrap().page_text.as_deref(),
        Some("b")
    );
    assert!(pages::get_page(Some(&v), 2).is_none());
    assert!(pages::get_page(None, 0).is_none());
    assert!(!pages::remove_page(Some(&mut v), 2));
    assert!(pages::remove_page(Some(&mut v), 0));
    assert_eq!(
        pages::get_page(Some(&v), 0).unwrap().page_text.as_deref(),
        Some("b")
    );
    assert!(!pages::remove_page(None, 0));
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn book_page_negative_index_throws() {
    let v = vec![PropertiesBookPageData::default()];
    let _ = pages::get_page(Some(&v), -1);
}

#[test]
fn allegiance_records() {
    let mut d = DotNetDict::new();
    allegiance::add_or_update_allegiance(&mut d, 10, false, true);
    allegiance::add_or_update_allegiance(&mut d, 20, true, false);
    allegiance::add_or_update_allegiance(&mut d, 30, true, true);
    allegiance::add_or_update_allegiance(&mut d, 10, true, true);
    let approved: Vec<u32> = allegiance::get_approved_vassals(Some(&d))
        .keys()
        .copied()
        .collect();
    assert_eq!(approved, [10, 30]);
    let banned: Vec<u32> = allegiance::get_ban_list(Some(&d)).keys().copied().collect();
    assert_eq!(banned, [10, 20, 30]);
    assert_eq!(
        allegiance::get_first_or_default_by_character_id(Some(&d), 20),
        Some(&PropertiesAllegiance {
            banned: true,
            approved_vassal: false
        })
    );
    assert!(allegiance::try_remove_allegiance(Some(&mut d), 20));
    assert!(!allegiance::try_remove_allegiance(Some(&mut d), 20));
    assert!(!allegiance::try_remove_allegiance(None, 20));
    assert!(allegiance::get_approved_vassals(None).is_empty());
    assert!(allegiance::get_first_or_default_by_character_id(None, 1).is_none());
}
