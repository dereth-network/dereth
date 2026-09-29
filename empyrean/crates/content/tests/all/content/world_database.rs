//! ACE: Source/ACE.Database/WorldDatabase.cs::WorldDatabase
//! WorldDatabase/WithEntityCache: GetWeenie row filters/order, cached conversion and miss
//! caching, random weenies, creature name check, cookbooks, encounters, events, house portals,
//! POI/quests/spells, treasure caches, MemContent parity.
//! Fixture: synthetic world records, SQL dumps and JSON documents.

use std::sync::Arc;

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::*;
use empyrean_content::{MemContent, WorldDatabase};
use empyrean_entity::enums::{HouseType, PropertyInt, PropertyString, WeenieType};

use crate::fixture;

#[test]
fn get_weenie_drops_creature_and_book_rows_by_type_and_orders_by_index() {
    let db = fixture::db();
    // 500 is a Generic with attribute, skill and book rows: GetWeenie leaves them empty.
    let item = db.get_weenie(500).unwrap();
    assert!(item.weenie_properties_attribute.is_empty() && item.weenie_properties_skill.is_empty());
    assert!(item.weenie_properties_book.is_none());
    assert_eq!(item.weenie_properties_int[0].value, -7);
    // GetAllWeenies loads every table with no filter.
    let all = db.get_all_weenies();
    let item_all = all.iter().find(|w| w.class_id == 500).unwrap();
    assert_eq!(item_all.weenie_properties_attribute.len(), 1);
    assert!(item_all.weenie_properties_book.is_some());

    let c = db.get_weenie(100).unwrap();
    assert_eq!(c.weenie_properties_attribute.len(), 2);
    // `WHERE object_Id = x` reads through UNIQUE (object_Id, type): type order, not id order.
    let types: Vec<u16> = c
        .weenie_properties_attribute
        .iter()
        .map(|a| a.r#type)
        .collect();
    assert_eq!(types, [1, 2]);
    let parts: Vec<u8> = c
        .weenie_properties_texture_map
        .iter()
        .map(|t| t.index)
        .collect();
    assert_eq!(parts, [1, 9]);
    let ints: Vec<u16> = c.weenie_properties_int.iter().map(|i| i.r#type).collect();
    assert_eq!(ints, [1, 25]);
    // The spell book is `OrderBy(Id)`.
    let spells: Vec<i32> = c
        .weenie_properties_spell_book
        .iter()
        .map(|s| s.spell)
        .collect();
    assert_eq!(spells, [2000, 1000]);
    assert!(
        db.get_weenie(200).unwrap().weenie_properties_book.is_some(),
        "a Book keeps its book"
    );
    assert!(db.get_weenie(12345).is_none());
}

#[test]
fn get_cached_weenie_converts_once_and_caches_misses() {
    let db = fixture::db();
    assert_eq!(db.get_weenie_cache_count(), 0);
    let a = db.get_cached_weenie(100).unwrap();
    let b = db.get_cached_weenie(100).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(
        a.properties_int.as_ref().unwrap().get(&PropertyInt::Level),
        Some(&5)
    );
    assert!(db.get_cached_weenie(999).is_none());
    assert_eq!(
        db.get_weenie_cache_count(),
        1,
        "a cached miss is a null entry, not counted"
    );
    assert!(db.clear_cached_weenie(100));
    assert!(!Arc::ptr_eq(&a, &db.get_cached_weenie(100).unwrap()));
    // By class name: MySQL compares case-insensitively; the cache key is the lower-cased name.
    let by_name = db.get_cached_weenie_by_class_name("DRUDGETEST").unwrap();
    assert_eq!(by_name.weenie_class_id, 100);
    assert!(db.get_cached_weenie_by_class_name("nosuch").is_none());
    assert_eq!(
        db.get_weenie_by_class_name("booktest").unwrap().class_id,
        200
    );
    db.clear_weenie_cache();
    assert_eq!(db.get_weenie_cache_count(), 0);
}

#[test]
fn cache_all_weenies_populates_the_type_and_scroll_caches() {
    let db = fixture::db();
    assert_eq!(db.get_scroll_weenie(1234).unwrap().weenie_class_id, 300);
    // Before the caches are populated the scroll comes from the bare `weenie` row.
    assert!(db.get_scroll_weenie(1234).unwrap().properties_did.is_none());
    assert!(db.get_scroll_weenie(4321).is_none());

    let db = fixture::db();
    db.cache_all_weenies();
    assert_eq!(db.get_weenie_cache_count(), 5);
    let scroll = db.get_scroll_weenie(1234).unwrap();
    assert!(
        scroll.properties_did.is_some(),
        "after CacheAllWeenies the full weenie is indexed"
    );
    assert!(db.get_scroll_weenie(4321).is_none());
    #[allow(clippy::cast_possible_wrap)]
    let books = db.get_random_weenies_of_type(WeenieType::Book.0 as i32, 3);
    assert_eq!(books.len(), 3);
    assert!(books
        .iter()
        .all(|w| w.as_ref().unwrap().weenie_class_id == 200));
    assert!(db.get_random_weenies_of_type(77, 3).is_empty());
}

#[test]
fn random_weenies_draw_next_zero_to_count_minus_one() {
    let db = MemContent::new()
        .weenie(Weenie::new(1, "a", WeenieType::Gem))
        .weenie(Weenie::new(2, "b", WeenieType::Gem))
        .weenie(Weenie::new(3, "c", WeenieType::Gem))
        .weenie(Weenie::new(4, "d", WeenieType::Generic));
    ThreadSafeRandom::seed(99);
    let expected: Vec<u32> = (0..5)
        .map(|_| [1, 2, 3][usize::try_from(ThreadSafeRandom::next(0, 2)).unwrap()])
        .collect();
    ThreadSafeRandom::seed(99);
    #[allow(clippy::cast_possible_wrap)]
    let got: Vec<u32> = db
        .get_random_weenies_of_type(WeenieType::Gem.0 as i32, 5)
        .into_iter()
        .map(|w| w.unwrap().weenie_class_id)
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn creature_name_check_is_case_insensitive_and_creature_only() {
    let db = fixture::db();
    assert!(db.is_creature_name_in_world_database("drudge 'tester' \"(x),(y)\""));
    assert!(
        !db.is_creature_name_in_world_database("Thing"),
        "500 is not a Creature"
    );
    assert!(!db.is_creature_name_in_world_database("Nobody"));
}

#[test]
fn names_and_class_names_are_keyed_by_class_id_in_order() {
    let db = fixture::db();
    let names: Vec<(u32, String)> = db
        .get_all_weenie_names()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    assert_eq!(names[0], (100, "Drudge 'Tester' \"(x),(y)\"".to_owned()));
    assert_eq!(names[1], (200, String::new()), "no Name row is \"\"");
    assert_eq!(names.len(), 5);
    let classes: Vec<u32> = db.get_all_weenie_class_names().keys().copied().collect();
    assert_eq!(classes, [100, 200, 300, 400, 500]);
}

#[test]
fn houses_join_slum_lords_to_their_instances() {
    let db = fixture::db();
    let houses = db.get_houses_all();
    assert_eq!(houses.len(), 1);
    assert_eq!(
        houses[0].house_type,
        HouseType::Cottage,
        "IndexOf(\"cottage\", OrdinalIgnoreCase)"
    );
    assert_eq!(houses[0].landblock_instance.guid, 2_056_994_817);
    assert!(
        houses[0].weenie.weenie_properties_string.is_empty(),
        "the join loads no child rows"
    );
    use empyrean_content::entity::HouseListResults;
    assert_eq!(
        HouseListResults::get_house_type("xApartMent"),
        HouseType::Apartment
    );
    assert_eq!(
        HouseListResults::get_house_type("villa_mansion"),
        HouseType::Villa
    );
    assert_eq!(HouseListResults::get_house_type("hut"), HouseType::Undef);
}

#[test]
fn cookbooks_take_the_lowest_id_and_share_the_recipe_cache() {
    let db = fixture::db();
    let cb = db.get_cached_cookbook(100, 200).unwrap();
    assert_eq!(
        (cb.id, cb.recipe_id),
        (5, 2),
        "two rows for one pair: the lowest id"
    );
    let recipe = cb.recipe.clone().unwrap();
    assert!(
        Arc::ptr_eq(&recipe, &db.get_cached_recipe(2).unwrap()),
        "RecipeManager_New's secondary index"
    );
    assert!(Arc::ptr_eq(&cb, &db.get_cached_cookbook(100, 200).unwrap()));
    assert!(db.get_cached_cookbook(1, 2).is_none());
    // GetRecipe (the override) caches what it loads.
    let r1 = db.get_cached_recipe(1).unwrap();
    assert!(Arc::ptr_eq(&r1, &db.get_cached_recipe(1).unwrap()));
    assert_eq!(
        db.get_cookbook_cache_count(),
        2,
        "one entry per source class id, a miss included"
    );
    let by_recipe: Vec<(u32, u32)> = db
        .get_cookbooks_by_recipe_id(2)
        .iter()
        .map(|c| c.as_ref().map(|c| (c.source_wcid, c.target_wcid)).unwrap())
        .collect();
    assert_eq!(by_recipe, [(100, 200), (300, 400)]);
    db.clear_cookbook_cache();
    assert_eq!(db.get_cookbook_cache_count(), 0);

    let all = db.get_all_cookbooks();
    assert_eq!(all.iter().map(|c| c.id).collect::<Vec<_>>(), [5, 10, 11]);
    assert!(
        Arc::ptr_eq(
            all[0].recipe.as_ref().unwrap(),
            all[2].recipe.as_ref().unwrap()
        ),
        "one tracked recipe"
    );
    // After GetAllCookbooks, (100, 200) is cached from the first row in id order.
    assert_eq!(db.get_cached_cookbook(100, 200).unwrap().id, 5);
}

#[test]
fn encounters_come_in_cell_order_and_are_cached_per_landblock() {
    let db = fixture::db();
    let e = db.get_cached_encounters_by_landblock(5);
    assert_eq!(
        e.iter().map(|e| (e.cell_x, e.cell_y)).collect::<Vec<_>>(),
        [(1, 2), (3, 1)]
    );
    assert!(Arc::ptr_eq(&e, &db.get_cached_encounters_by_landblock(5)));
    assert!(db.get_cached_encounters_by_landblock(7).is_empty());
    assert_eq!(db.get_encounter_cache_count(), 2);
    assert!(db.clear_cached_encounters_by_landblock(5));
    assert!(!db.clear_cached_encounters_by_landblock(5));
}

#[test]
fn events_are_cached_by_lower_case_name() {
    let db = fixture::db();
    assert_eq!(db.get_cached_event("EVENT TWO").unwrap().id, 2);
    assert!(db.get_cached_event("nope").is_none());
    assert_eq!(db.get_events_cache_count(), 1);
    assert!(db.clear_cached_event("Event Two"));
    assert_eq!(db.get_all_events().len(), 2);
    assert_eq!(db.get_events_cache_count(), 2);
}

#[test]
fn house_portals_by_house_use_the_house_cell_index() {
    let db = fixture::db();
    let p = db.get_cached_house_portals(77);
    assert_eq!(
        p.iter().map(|p| p.id).collect::<Vec<_>>(),
        [2, 1],
        "UNIQUE (house_Id, obj_Cell_Id)"
    );
    let lb = db.get_cached_house_portals_by_landblock(0xA9B4);
    assert_eq!(
        lb.iter().map(|p| p.id).collect::<Vec<_>>(),
        [1, 2],
        "a table scan: id order"
    );
    let db = fixture::db();
    db.cache_all_house_portals();
    assert_eq!(
        db.get_cached_house_portals(77)
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(db.get_cached_house_portals(78).len(), 1);
}

#[test]
fn landblock_queries() {
    let db = fixture::db();
    assert_eq!(db.get_landblock_instances_cache_count(), 0);
    let a = db.get_cached_instances_by_landblock(0xA9B4);
    assert!(Arc::ptr_eq(
        &a,
        &db.get_cached_instances_by_landblock(0xA9B4)
    ));
    assert_eq!(db.get_landblock_instances_cache_count(), 1);
    assert!(db.clear_cached_instances_by_landblock(0xA9B4));
    db.clear_cached_landblock_instances();
    assert_eq!(db.get_landblock_instances_cache_count(), 0);
    // The first instance that is not a house portal (11730), a door (278, 568) or a link child.
    assert_eq!(db.get_cached_basement_house_guid(0xA9B4), 2_056_994_817);
    assert_eq!(db.get_cached_basement_house_guid(0x1234), 0);
    assert_eq!(
        db.get_landblock_instance_by_guid(1_879_052_288)
            .unwrap()
            .weenie_class_id,
        100
    );
    assert_eq!(
        db.get_landblock_instance_by_guid(2_056_994_818)
            .unwrap()
            .weenie_class_id,
        11730
    );
    assert!(db.get_landblock_instance_by_guid(5).is_none());
    // A table scan: guid order, across landblocks (0x0002's guid is the highest).
    let by_wcid: Vec<u32> = db
        .get_landblock_instances_by_wcid(100)
        .iter()
        .map(|i| i.guid)
        .collect();
    assert_eq!(by_wcid, [1_879_052_288, 2_056_994_820, 2_147_418_113]);
    assert_eq!(
        db.get_landblock_instance_by_guid(2_147_418_113)
            .unwrap()
            .landblock,
        Some(2),
        "not in its guid's landblock"
    );
    assert!(db.is_world_database_guid_range_valid());
    let bad = MemContent::new().landblock_instance(LandblockInstance::new(
        0x8000_0001,
        1,
        0x0101_0001,
        [0.0; 3],
    ));
    assert!(!bad.is_world_database_guid_range_valid());
}

#[test]
fn points_of_interest_quests_and_spells() {
    let db = fixture::db();
    assert_eq!(db.get_points_of_interest_cache_count(), 0);
    db.cache_all_points_of_interest();
    assert_eq!(db.get_points_of_interest_cache_count(), 1);
    assert_eq!(db.get_points_of_interest_cache()[0].0, "holtburg");
    assert!(db.get_cached_point_of_interest("Nowhere").is_none());
    assert_eq!(
        db.get_points_of_interest_cache().len(),
        2,
        "the miss is cached as null"
    );

    // The quest cache key is the name as given; MySQL matches case-insensitively.
    let q = db.get_cached_quest("testquest").unwrap();
    assert_eq!(
        (q.id, q.min_delta, q.max_solves, q.message.clone()),
        (1, 3600, -1, None)
    );
    assert!(db.clear_cached_quest("testquest"));
    assert!(!db.clear_cached_quest("TestQuest"));

    assert_eq!(db.get_spell_cache_count(), 0);
    assert!(db.get_cached_spell(1).is_none());
    assert_eq!(db.get_spell_cache_count(), 0);
    db.cache_all_spells();
    assert_eq!(db.get_spell_cache_count(), 2);
    db.clear_spell_cache();
    assert_eq!(db.get_spell_cache_count(), 0);
    let names: Vec<(u32, String)> = db
        .get_all_spell_names()
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    assert_eq!(
        names,
        [
            (1234, "Test Spell".to_owned()),
            (1235, "Full Spell".to_owned())
        ]
    );
}

#[test]
fn treasure_caches() {
    let db = fixture::db();
    assert_eq!(db.get_cached_death_treasure(6).unwrap().tier, 2);
    assert!(db.get_cached_death_treasure(99).is_none());
    assert_eq!(db.get_death_treasure_cache_count(), 1);
    db.cache_all_treasures_death();
    assert_eq!(db.get_death_treasure_cache_count(), 2);
    let all = db.get_all_treasure_death();
    assert_eq!(all.keys().copied().collect::<Vec<_>>(), [5, 6]);

    // Base: probabilities 1 and 1 in (code 1, tier 1) scale to 0.5 each; the 0 row is filtered;
    // (1, 2) sums to 1 already and is left alone.
    let base = db.get_cached_treasure_material_base(1, 1).unwrap();
    assert_eq!(
        base.iter()
            .map(|b| (b.material_id, b.probability))
            .collect::<Vec<_>>(),
        [(11, 0.5), (12, 0.5)]
    );
    assert_eq!(
        db.get_cached_treasure_material_base(1, 2).unwrap()[0].probability,
        1.0
    );
    assert!(db.get_cached_treasure_material_base(2, 1).is_none());
    // Colours: 0.2 + 0.2 summed in double and narrowed, then each * (1.0f / 0.4f).
    let colors = db.get_cached_treasure_material_colors(11, 3).unwrap();
    #[allow(clippy::cast_possible_truncation)]
    let total = (f64::from(0.2f32) + f64::from(0.2f32)) as f32;
    let expected = 0.2f32 * (1.0f32 / total);
    assert_eq!(
        colors
            .iter()
            .map(|c| c.probability.to_bits())
            .collect::<Vec<_>>(),
        [expected.to_bits(); 2]
    );
    let sum = [0.01f32, 0.04, 0.02]
        .iter()
        .map(|&p| f64::from(p))
        .sum::<f64>();
    #[allow(clippy::cast_possible_truncation)]
    let factor = 1.0f32 / (sum as f32);
    let colors = db.get_cached_treasure_material_colors(12, 4).unwrap();
    let got: Vec<u32> = colors.iter().map(|c| c.probability.to_bits()).collect();
    assert_eq!(
        got,
        [0.01f32 * factor, 0.04 * factor, 0.02 * factor].map(f32::to_bits),
        "LINQ Sum of floats is a double"
    );
    // Groups: 0.5 + 0.5 is 1: untouched.
    let groups = db.get_cached_treasure_material_group(40, 1).unwrap();
    assert_eq!(
        groups.iter().map(|g| g.probability).collect::<Vec<_>>(),
        [0.5, 0.5]
    );
    assert!(db.get_cached_treasure_material_group(40, 2).is_none());

    let tw = db.get_cached_wielded_treasure(10);
    assert_eq!(tw.len(), 1);
    assert_eq!(db.get_wielded_treasure_cache_count(), 1);
    assert!(db.get_cached_wielded_treasure(11).is_empty());
    db.clear_wielded_treasure_cache();
    db.cache_all_treasure_wielded();
    assert_eq!(db.get_wielded_treasure_cache_count(), 2);
    let all = db.get_all_treasure_wielded();
    assert_eq!(
        all.iter().map(|(k, v)| (*k, v.len())).collect::<Vec<_>>(),
        [(9, 2), (10, 1)]
    );
}

#[test]
fn exists_and_version() {
    let db = fixture::db();
    assert!(db.exists(false));
    assert_eq!(
        db.get_version().unwrap().patch_version.as_deref(),
        Some("v0.9.294")
    );
    assert!(MemContent::new().get_version().is_none());
}

#[test]
fn mem_content_serves_the_same_api_from_built_rows() {
    let content = MemContent::new()
        .weenie(
            Weenie::new(10, "Rabbit", WeenieType::Creature)
                .with_string(PropertyString::Name, "Rabbit")
                .with_int(PropertyInt::Level, 2),
        )
        .landblock_instance(LandblockInstance::new(
            0x7123_4001,
            10,
            0x1234_0021,
            [12.0, 24.0, 0.0],
        ));
    let db: &dyn WorldDatabase = &content;
    let w = db.get_cached_weenie(10).unwrap();
    assert_eq!(
        w.properties_string
            .as_ref()
            .unwrap()
            .get(&PropertyString::Name)
            .map(String::as_str),
        Some("Rabbit")
    );
    let lb = db.get_cached_instances_by_landblock(0x1234);
    assert_eq!(lb.len(), 1);
    assert_eq!(lb[0].landblock, Some(0x1234));
    assert!(db.is_creature_name_in_world_database("RABBIT"));
    // Adding a row starts a fresh database.
    let content = content.weenie(Weenie::new(11, "Hare", WeenieType::Creature));
    assert!(content.get_cached_weenie(11).is_some());
}

#[test]
#[should_panic(expected = "duplicate key")]
fn mem_content_refuses_two_weenies_with_one_class_id() {
    let db = MemContent::new()
        .weenie(Weenie::new(1, "a", WeenieType::Generic))
        .weenie(Weenie::new(1, "b", WeenieType::Generic));
    let _ = db.get_cached_weenie(1);
}

/// GemCountChance's static-constructor query: every `treasure_gem_count` row, in id order
/// (MariaDB's primary-key order), the zero-chance ones included (the constructor filters them).
#[test]
fn all_treasure_gem_count_rows_in_id_order() {
    let db = fixture::db();
    let rows = db.get_all_treasure_gem_count();
    assert_eq!(
        rows,
        [TreasureGemCount {
            id: 1,
            gem_code: 2,
            tier: 1,
            count: 3,
            chance: 0.5
        }]
    );

    let row = |id, chance| TreasureGemCount {
        id,
        gem_code: 7,
        tier: 1,
        count: 1,
        chance,
    };
    let mem = MemContent::new()
        .treasure_gem_count(row(9, 0.0))
        .treasure_gem_count(row(3, 1.0));
    let ids: Vec<(u32, f32)> = mem
        .get_all_treasure_gem_count()
        .iter()
        .map(|r| (r.id, r.chance))
        .collect();
    assert_eq!(ids, [(3, 1.0), (9, 0.0)]);
}
