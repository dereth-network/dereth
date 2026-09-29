//! ACE: Source/ACE.Database/ShardDatabase.cs::ShardDatabase
//! Save/load biota identical including enumeration order, unsorted dictionaries in key order, EF-
//! like update path, backends store identical rows, constraint violation writes nothing, empty
//! collections, record ids, weenie-to-database biota.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use std::sync::Arc;

use empyrean_entity::enums::*;
use empyrean_entity::models::*;
use empyrean_store::adapter::BiotaConverter;
use empyrean_store::shard_database::PopulatedCollectionFlags;
use empyrean_store::ShardDatabase;

use crate::support::{assert_same, backends, dump, rich_biota};

fn reload(db: &mut dyn ShardDatabase, id: u32) -> empyrean_entity::Biota {
    let row = db.get_biota(id, false).expect("biota was saved");
    BiotaConverter::convert_to_entity_biota(&row, false)
}

#[test]
fn save_then_load_is_identical_in_database_order() {
    for (name, mut db) in backends() {
        let original = rich_biota(0x8000_0001, true);
        let mut snapshot = original.clone();
        assert!(db.save_biota(&mut snapshot, false), "{name}: save");

        let loaded = reload(db.as_mut(), 0x8000_0001);
        assert_same(name, &dump(&loaded, false), &dump(&original, false));

        // The surrogate-id records came back with their row ids, in insertion order.
        let cl = loaded.properties_create_list.as_ref().unwrap();
        assert!(
            cl[0].database_record_id > 0 && cl[1].database_record_id > cl[0].database_record_id,
            "{name}"
        );
        let em = loaded.properties_emote.as_ref().unwrap();
        assert!(
            em[0]
                .properties_emote_action
                .iter()
                .all(|a| a.database_record_id > 0),
            "{name}"
        );
    }
}

#[test]
fn unsorted_dictionaries_come_back_in_key_order() {
    for (name, mut db) in backends() {
        let mut unsorted = rich_biota(0x8000_0002, false);
        assert!(db.save_biota(&mut unsorted, false), "{name}");
        let loaded = reload(db.as_mut(), 0x8000_0002);

        // InnoDB returns child rows in primary-key order, so ACE's dictionaries are key-ordered
        // after a reload whatever order they were built in.
        let ints: Vec<u16> = loaded
            .properties_int
            .as_ref()
            .unwrap()
            .keys()
            .map(|k| k.0)
            .collect();
        assert_eq!(ints, vec![1, 5, 25, 93], "{name}");
        let spells: Vec<i32> = loaded
            .properties_spell_book
            .as_ref()
            .unwrap()
            .keys()
            .copied()
            .collect();
        assert_eq!(spells, vec![3, 1636, 2000], "{name}");
        let parts: Vec<i32> = loaded
            .properties_body_part
            .as_ref()
            .unwrap()
            .keys()
            .map(|k| k.0)
            .collect();
        assert_eq!(parts, vec![0, 1, 2, 8], "{name}");
        let events: Vec<i32> = loaded
            .properties_event_filter
            .as_ref()
            .unwrap()
            .iter()
            .copied()
            .collect();
        assert_eq!(events, vec![0x0002, 0x0100, 0x0400], "{name}");
        let ench: Vec<i32> = loaded
            .properties_enchantment_registry
            .as_ref()
            .unwrap()
            .iter()
            .map(|e| e.spell_id)
            .collect();
        assert_eq!(ench, vec![2, 5], "{name}");
        let perms: Vec<u32> = loaded
            .house_permissions
            .as_ref()
            .unwrap()
            .keys()
            .copied()
            .collect();
        assert_eq!(perms, vec![0x5000_0001, 0x5000_0009], "{name}");

        // Everything else is equal to the sorted build.
        assert_same(
            name,
            &dump(&loaded, false),
            &dump(&rich_biota(0x8000_0002, true), false),
        );
    }
}

#[test]
fn update_path_rewrites_rows_like_entity_framework() {
    for (name, mut db) in backends() {
        let id = 0x8000_0003;
        let mut first = rich_biota(id, true);
        assert!(db.save_biota(&mut first, false), "{name}");

        // The world's copy, as loaded: record ids known.
        let mut world = reload(db.as_mut(), id);
        let created_ids: Vec<u32> = world
            .properties_create_list
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.database_record_id)
            .collect();

        // Change, add, remove across the collections.
        world
            .properties_int
            .as_mut()
            .unwrap()
            .insert(PropertyInt(5), 55);
        world
            .properties_int
            .as_mut()
            .unwrap()
            .remove(&PropertyInt(25));
        world
            .properties_int
            .as_mut()
            .unwrap()
            .insert(PropertyInt(2), 2);
        world.properties_string = None;
        world.properties_anim_part.as_mut().unwrap().truncate(1);
        world
            .properties_palette
            .as_mut()
            .unwrap()
            .push(PropertiesPalette {
                sub_palette_id: 9,
                offset: 1,
                length: 2,
            });
        world.properties_create_list_mut().unwrap().remove(0);
        world.properties_emote_mut().unwrap()[0]
            .properties_emote_action
            .truncate(1);
        world.properties_emote_mut().unwrap().push(PropertiesEmote {
            category: EmoteCategory(3),
            ..Default::default()
        });
        world.properties_book = None;
        world.properties_book_page_data.as_mut().unwrap().pop();
        world
            .properties_enchantment_registry
            .as_mut()
            .unwrap()
            .retain(|e| e.spell_id != 2);
        world
            .house_permissions
            .as_mut()
            .unwrap()
            .insert(0x5000_0001, false);
        let nan = PropertiesPosition {
            rotation_x: f32::NAN,
            ..world
                .properties_position
                .as_ref()
                .unwrap()
                .get(&PositionType(1))
                .unwrap()
                .clone()
        };
        world
            .properties_position
            .as_mut()
            .unwrap()
            .insert(PositionType(1), nan);
        world.properties_event_filter_mut().unwrap().remove(&0x0100);
        world.properties_skill.as_mut().unwrap().insert(
            Skill(6),
            PropertiesSkill {
                pp: 1,
                ..Default::default()
            },
        );

        let mut snapshot = world.clone();
        assert!(db.save_biota(&mut snapshot, false), "{name}");
        let loaded = reload(db.as_mut(), id);

        // What the world would read back: key-ordered bags (PropertyInt 2 moves before 5), the kept
        // create-list row keeps its id, NaN rotations become identity.
        let mut expected = world.clone();
        let mut ints = empyrean_common::dotnet::DotNetDict::new();
        for (k, v) in [(1u16, 10), (2, 2), (5, 55), (93, 1_044_563)] {
            ints.insert(PropertyInt(k), v);
        }
        expected.properties_int = Some(ints);
        let p1 = expected
            .properties_position
            .as_mut()
            .unwrap()
            .get_mut(&PositionType(1))
            .unwrap();
        p1.rotation_w = 1.0;
        p1.rotation_x = 0.0;
        p1.rotation_y = 0.0;
        p1.rotation_z = 0.0;
        assert_same(name, &dump(&loaded, false), &dump(&expected, false));

        let kept = loaded.properties_create_list.as_ref().unwrap();
        assert_eq!(kept.len(), 1, "{name}");
        assert_eq!(
            kept[0].database_record_id, created_ids[1],
            "{name}: the matched row keeps its id"
        );

        // The stored flags now omit the emptied collections.
        let row = db.get_biota(id, false).unwrap();
        let flags = PopulatedCollectionFlags(row.populated_collection_flags);
        assert!(
            !flags.has_flag(PopulatedCollectionFlags::BIOTA_PROPERTIES_STRING),
            "{name}"
        );
        assert!(
            !flags.has_flag(PopulatedCollectionFlags::BIOTA_PROPERTIES_BOOK),
            "{name}"
        );
        assert!(
            flags.has_flag(PopulatedCollectionFlags::BIOTA_PROPERTIES_INT),
            "{name}"
        );
    }
}

#[test]
fn backends_store_identical_rows() {
    let mut rows = Vec::new();
    for (name, mut db) in backends() {
        let mut a = rich_biota(0x8000_0010, false);
        assert!(db.save_biota(&mut a, false));
        let mut w = reload(db.as_mut(), 0x8000_0010);
        w.properties_create_list_mut()
            .unwrap()
            .push(PropertiesCreateList {
                weenie_class_id: 99,
                ..Default::default()
            });
        w.properties_generator_mut().unwrap().clear();
        assert!(db.save_biota(&mut w, false));
        rows.push((name, db.get_biota(0x8000_0010, false).unwrap()));
    }
    assert_eq!(
        rows[0].1, rows[1].1,
        "mem and sqlite hold the same rows, ids included"
    );
}

#[test]
fn a_constraint_violation_fails_the_save_and_writes_nothing() {
    for (name, mut db) in backends() {
        let id = 0x8000_0020;
        let mut good = rich_biota(id, true);
        assert!(db.save_biota(&mut good, false));
        let before = db.get_biota(id, false).unwrap();

        // Two enchantments with the same spell and layer but different casters: ACE matches them as
        // two rows, and MySQL's unique (object_Id, spell_Id, layer_Id) key rejects the save.
        let mut bad = reload(db.as_mut(), id);
        bad.properties_int
            .as_mut()
            .unwrap()
            .insert(PropertyInt(1), -1);
        bad.properties_enchantment_registry
            .as_mut()
            .unwrap()
            .push(PropertiesEnchantmentRegistry {
                spell_id: 2,
                layer_id: 1,
                caster_object_id: 0x5000_0077,
                ..Default::default()
            });
        assert!(
            !db.save_biota(&mut bad, false),
            "{name}: DoSaveBiota returns false after its retry"
        );
        assert_eq!(
            db.get_biota(id, false).unwrap(),
            before,
            "{name}: nothing was written"
        );
    }
}

#[test]
fn a_book_page_without_text_is_rejected_like_a_not_null_column() {
    for (name, mut db) in backends() {
        let mut b = rich_biota(0x8000_0021, true);
        b.properties_book_page_data.as_mut().unwrap()[0].page_text = None;
        assert!(!db.save_biota(&mut b, false), "{name}");
        assert!(db.get_biota(0x8000_0021, false).is_none(), "{name}");
    }
}

#[test]
fn empty_collections_are_not_instantiated_unless_asked() {
    let row = empyrean_store::models::shard::Biota {
        id: 5,
        weenie_class_id: 3,
        weenie_type: 1,
        ..Default::default()
    };
    let b = BiotaConverter::convert_to_entity_biota(&row, false);
    assert!(
        b.properties_int.is_none() && b.properties_emote.is_none() && b.properties_book.is_none()
    );
    let b = BiotaConverter::convert_to_entity_biota(&row, true);
    assert!(b.properties_int.as_ref().is_some_and(|d| d.is_empty()));
    assert!(b.properties_emote.as_ref().is_some_and(|l| l.is_empty()));
    assert!(
        b.properties_book.is_none(),
        "a missing book row stays null either way"
    );
}

#[test]
fn convert_from_entity_biota_keeps_record_ids_only_when_asked() {
    let mut b = rich_biota(9, true);
    Arc::make_mut(b.properties_create_list.as_mut().unwrap())[0].database_record_id = 77;
    let without = BiotaConverter::convert_from_entity_biota(&b, false);
    let with = BiotaConverter::convert_from_entity_biota(&b, true);
    assert_eq!(without.biota_properties_create_list[0].id, 0);
    assert_eq!(with.biota_properties_create_list[0].id, 77);
    // Emote actions carry ACE's uint.MaxValue foreign-key placeholder and their list position.
    let actions = &without.biota_properties_emote[0].biota_properties_emote_action;
    assert!(actions.iter().all(|a| a.emote_id == u32::MAX));
    assert_eq!(
        actions.iter().map(|a| a.order).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // Anim parts, palettes and texture maps record their position in `order`; pages in `page_id`.
    assert_eq!(
        without
            .biota_properties_anim_part
            .iter()
            .map(|a| a.order)
            .collect::<Vec<_>>(),
        vec![Some(0), Some(1)]
    );
    assert_eq!(
        without
            .biota_properties_book_page_data
            .iter()
            .map(|p| p.page_id)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
}

/// The entity weenie with the same collections as [`rich_biota`] (weenies have no allegiance,
/// enchantments or house permissions).
fn rich_weenie(sorted: bool) -> empyrean_entity::Weenie {
    let b = rich_biota(0, sorted);
    empyrean_entity::Weenie {
        weenie_class_id: 4242,
        class_name: Some("richweenie".into()),
        weenie_type: b.weenie_type,
        properties_bool: b.properties_bool,
        properties_did: b.properties_did,
        properties_float: b.properties_float,
        properties_iid: b.properties_iid,
        properties_int: b.properties_int,
        properties_int64: b.properties_int64,
        properties_string: b.properties_string,
        properties_position: b.properties_position,
        properties_spell_book: b.properties_spell_book,
        properties_anim_part: b.properties_anim_part,
        properties_palette: b.properties_palette,
        properties_texture_map: b.properties_texture_map,
        properties_create_list: b.properties_create_list,
        properties_emote: b.properties_emote,
        properties_event_filter: b.properties_event_filter,
        properties_generator: b.properties_generator,
        properties_attribute: b.properties_attribute,
        properties_attribute_2nd: b.properties_attribute_2nd,
        properties_body_part: b.properties_body_part,
        properties_skill: b.properties_skill,
        properties_book: b.properties_book,
        properties_book_page_data: b.properties_book_page_data,
    }
}

#[test]
fn weenie_converter_convert_to_database_biota_round_trips() {
    use empyrean_store::adapter::WeenieConverter;

    for (name, mut db) in backends() {
        let weenie = rich_weenie(true);
        let id = 0x8000_0050;
        let mut row = WeenieConverter::convert_to_database_biota(&weenie, id);
        assert_eq!(row.weenie_class_id, 4242);
        // As in ACE: no order on anim parts, palettes and texture maps; actions carry their position.
        assert!(row
            .biota_properties_anim_part
            .iter()
            .all(|a| a.order.is_none()));
        assert_eq!(
            row.biota_properties_emote[0]
                .biota_properties_emote_action
                .iter()
                .map(|a| a.order)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(db.do_save_biota(&mut row), "{name}");

        // The stored biota reads back as the entity-side WeenieConverter.ConvertToBiota would build it.
        let loaded = reload(db.as_mut(), id);
        let expected = empyrean_entity::convert_to_biota(&weenie, id, false, false);
        assert_same(name, &dump(&loaded, false), &dump(&expected, false));

        // The null orders mean the first update replaces those rows (ACE deletes rows whose Order is null).
        let before: Vec<u32> = db
            .get_biota(id, false)
            .unwrap()
            .biota_properties_anim_part
            .iter()
            .map(|a| a.id)
            .collect();
        let mut again = loaded.clone();
        assert!(db.save_biota(&mut again, false), "{name}");
        let after = db.get_biota(id, false).unwrap();
        assert!(
            after
                .biota_properties_anim_part
                .iter()
                .all(|a| a.order.is_some() && !before.contains(&a.id)),
            "{name}"
        );
        assert_same(
            name,
            &dump(&reload(db.as_mut(), id), false),
            &dump(&expected, false),
        );
    }
}
