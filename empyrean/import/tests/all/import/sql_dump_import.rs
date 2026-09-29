//! Vectors: support/world_dump_fixture.rs, synthetic rows for every ACE world table
//! The dump importer reads every world table with full columns, decodes escapes/NULL/bits as
//! MySQL, maps every emote/spell column, links landblock instances and recipe children, errors on
//! missing column/NULL/bad value, is byte-deterministic, reports counts and hash.
//! Fixture: synthetic world records, SQL dumps and JSON documents.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_content::import::world_rows::WORLD_TABLES;
use empyrean_content::import::{import, mysqldump};
use empyrean_content::models::world::*;
use empyrean_content::pack::{Pack, TableId};
use empyrean_content::ImportError;

use crate::fixture;

fn d() -> DotNetDateTime {
    DotNetDateTime::new_hms(2021, 11, 1, 0, 0, 0)
}

#[test]
fn every_world_table_is_declared_and_has_rows() {
    let (_, imp) = fixture::imported();
    assert_eq!(
        WORLD_TABLES.len(),
        54,
        "ACE's World model has 54 table classes"
    );
    assert!(imp.missing_tables.is_empty(), "{:?}", imp.missing_tables);
    assert!(imp.unknown_tables.is_empty(), "{:?}", imp.unknown_tables);
    assert!(imp.unread_columns.is_empty(), "{:?}", imp.unread_columns);
    assert!(imp.orphans.is_empty(), "{:?}", imp.orphans);
    for (table, rows) in &imp.rows {
        assert!(*rows > 0, "fixture has no rows for {table}");
    }
    let rows: std::collections::BTreeMap<_, _> = imp.rows.iter().copied().collect();
    assert_eq!(
        rows["weenie"], 5,
        "two INSERT statements for one table both count"
    );
    assert_eq!(rows["weenie_properties_emote_action"], 2);
    assert_eq!(rows["landblock_instance"], 6);
}

#[test]
fn escapes_null_and_bits_decode_as_mysql_stores_them() {
    let db = fixture::db();
    let w = db
        .base()
        .get_all_weenies()
        .into_iter()
        .find(|w| w.class_id == 100)
        .unwrap();
    let names: Vec<&str> = w
        .weenie_properties_string
        .iter()
        .map(|s| s.value.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Drudge 'Tester' \"(x),(y)\"",
            "line1\nline2\r\t\\ \u{1a}\u{0}end"
        ]
    );
    let bools: Vec<bool> = w.weenie_properties_bool.iter().map(|b| b.value).collect();
    assert_eq!(bools, [true, false]);
    assert_eq!(w.weenie_properties_int64[0].value, -9_000_000_000);
    assert_eq!(w.weenie_properties_iid[0].value, 2_147_483_649);
    assert_eq!(
        w.weenie_properties_emote[0].quest.as_deref(),
        Some("quest's")
    );
    assert_eq!(w.weenie_properties_emote[0].weenie_class_id, None);
    assert_eq!(w.weenie_properties_emote[1].style, Some(2_147_483_709));
    assert_eq!(w.weenie_properties_emote[1].vendor_type, Some(-2));
    assert_eq!(w.weenie_properties_create_list[1].palette, -3);
    assert!(w.weenie_properties_create_list[1].try_to_bond);
    assert_eq!(w.last_modified, d());
}

#[test]
#[allow(clippy::cast_possible_truncation)]
fn a_float_column_is_parsed_through_double_as_mysql_does() {
    let db = fixture::db();
    let p = db.get_cached_house_portals(78);
    assert_eq!(
        p[0].origin_x.to_bits(),
        (1.0f32 + f32::EPSILON * 2.0).to_bits(),
        "1 + 2^-22, not 1 + 2^-23"
    );
    let i = db.get_landblock_instance_by_guid(2_056_994_817).unwrap();
    let expected = 73.2987f64 as f32;
    assert_eq!(i.origin_x.to_bits(), expected.to_bits());
    assert_eq!(i.angles_w.to_bits(), (0.174196f64 as f32).to_bits());
}

#[test]
fn every_emote_action_column_maps() {
    let db = fixture::db();
    let w = db
        .base()
        .get_all_weenies()
        .into_iter()
        .find(|w| w.class_id == 100)
        .unwrap();
    let a = &w.weenie_properties_emote[0].weenie_properties_emote_action[1];
    let expected = WeeniePropertiesEmoteAction {
        id: 2,
        emote_id: 1,
        order: 0,
        r#type: 1,
        delay: 0.5,
        extent: 2.0,
        motion: Some(318_767_235),
        message: Some("first".into()),
        test_string: Some("test".into()),
        min: Some(1),
        max: Some(2),
        min_64: Some(-5_000_000_000),
        max_64: Some(5_000_000_000),
        min_dbl: Some(0.5),
        max_dbl: Some(1.5),
        stat: Some(3),
        display: Some(true),
        amount: Some(4),
        amount_64: Some(6_000_000_000),
        hero_xp_64: Some(7_000_000_000),
        percent: Some(0.25),
        spell_id: Some(1234),
        wealth_rating: Some(5),
        treasure_class: Some(6),
        treasure_type: Some(7),
        p_script: Some(-1),
        sound: Some(-2),
        destination_type: Some(-3),
        weenie_class_id: Some(300),
        stack_size: Some(9),
        palette: Some(10),
        shade: Some(0.5),
        try_to_bond: Some(false),
        obj_cell_id: Some(2_847_146_009),
        origin_x: Some(1.0),
        origin_y: Some(2.0),
        origin_z: Some(3.0),
        angles_w: Some(1.0),
        angles_x: Some(0.0),
        angles_y: Some(0.0),
        angles_z: Some(0.0),
    };
    assert_eq!(*a, expected);
    let empty = &w.weenie_properties_emote[0].weenie_properties_emote_action[0];
    assert_eq!(empty.message.as_deref(), Some("second"));
    assert_eq!(
        (empty.motion, empty.min, empty.display, empty.origin_x),
        (None, None, None, None)
    );
}

#[test]
fn every_spell_column_maps() {
    let db = fixture::db();
    let s = db.get_cached_spell(1235).unwrap();
    assert_eq!(s.name, "Full Spell");
    assert_eq!(
        (s.stat_mod_type, s.stat_mod_key, s.stat_mod_val),
        (Some(1), Some(2), Some(3.5))
    );
    assert_eq!(s.non_tracking, Some(true));
    assert_eq!(
        (s.create_offset_origin_x, s.peturbation_origin_z),
        (Some(1.0), Some(12.0))
    );
    assert_eq!(
        (
            s.imbued_effect,
            s.slayer_creature_type,
            s.slayer_damage_bonus
        ),
        (Some(13), Some(14), Some(15.5))
    );
    assert_eq!(
        (s.crit_freq, s.crit_multiplier, s.ignore_magic_resist),
        (Some(16.5), Some(17.5), Some(18))
    );
    assert_eq!(
        (
            s.elemental_modifier,
            s.drain_percentage,
            s.damage_ratio,
            s.damage_type
        ),
        (Some(19.5), Some(20.5), Some(21.5), Some(22))
    );
    assert_eq!(
        (s.boost, s.boost_variance, s.source, s.destination),
        (Some(23), Some(24), Some(25), Some(26))
    );
    assert_eq!(
        (s.proportion, s.loss_percent, s.source_loss, s.transfer_cap),
        (Some(27.5), Some(28.5), Some(29), Some(30))
    );
    assert_eq!(
        (s.max_boost_allowed, s.transfer_bitfield, s.index, s.link),
        (Some(31), Some(32), Some(33), Some(34))
    );
    assert_eq!(
        (
            s.position_obj_cell_id,
            s.position_origin_z,
            s.position_angles_w
        ),
        (Some(2_847_146_009), Some(3.0), Some(1.0))
    );
    assert_eq!(
        (s.min_power, s.max_power, s.power_variance, s.dispel_school),
        (Some(35), Some(36), Some(37.5), Some(38))
    );
    assert_eq!(
        (s.align, s.number, s.number_variance, s.dot_duration),
        (Some(39), Some(40), Some(41.5), Some(42.5))
    );
    let empty = db.get_cached_spell(1234).unwrap();
    assert_eq!(empty.name, "Test Spell");
    assert_eq!(
        (empty.stat_mod_type, empty.non_tracking, empty.dot_duration),
        (None, None, None)
    );
    assert_eq!(empty.last_modified, d());
}

#[test]
fn landblock_instances_get_the_generated_landblock_and_their_links() {
    let db = fixture::db();
    let lb = db.get_cached_instances_by_landblock(0xA9B4);
    assert_eq!(
        lb.iter().map(|i| i.guid).collect::<Vec<_>>(),
        [2_056_994_817, 2_056_994_818, 2_056_994_819, 2_056_994_820]
    );
    assert_eq!(
        lb[0].weenie_class_id, 400,
        "the explicit column list, not the table order"
    );
    assert!(lb.iter().all(|i| i.landblock == Some(0xA9B4)));
    assert_eq!(lb[0].landblock_instance_link.len(), 1);
    assert_eq!(lb[0].landblock_instance_link[0].child_guid, 2_056_994_818);
    assert!(!lb[0].is_link_child && lb[1].is_link_child);
    let other = db.get_cached_instances_by_landblock(0x0001);
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].landblock, Some(1));
}

#[test]
fn recipes_carry_all_twelve_child_tables() {
    let db = fixture::db();
    let r = db.get_recipe(2).unwrap();
    assert_eq!(r.skill, 35);
    assert_eq!(r.fail_message.as_deref(), Some("no"));
    assert_eq!(r.success_message, None);
    assert_eq!(r.data_id, 7);
    let m = &r.recipe_mod[0];
    assert!(m.executes_on_success && !m.unknown_7);
    assert_eq!(
        (
            m.health,
            m.stamina,
            m.mana,
            m.data_id,
            m.unknown_9,
            m.instance_id
        ),
        (1, 2, 3, 4, 5, 6)
    );
    assert!(m.recipe_mods_bool[0].value);
    assert_eq!(m.recipe_mods_did[0].value, u32::MAX);
    assert_eq!(m.recipe_mods_float[0].value, 0.125);
    assert_eq!(m.recipe_mods_iid[0].value, 7);
    assert_eq!(
        (m.recipe_mods_int[0].index, m.recipe_mods_int[0].value),
        (-1, -8)
    );
    assert_eq!(m.recipe_mods_string[0].value, None);
    assert!(!r.recipe_requirements_bool[0].value);
    assert_eq!(
        r.recipe_requirements_bool[0].message.as_deref(),
        Some("must be false")
    );
    assert_eq!(r.recipe_requirements_did[0].value, 9);
    assert_eq!(r.recipe_requirements_float[0].value, 2.5);
    assert_eq!(r.recipe_requirements_iid[0].value, 8);
    assert_eq!(r.recipe_requirements_int[0].value, -4);
    assert_eq!(
        r.recipe_requirements_string[0].value.as_deref(),
        Some("abc")
    );
    assert!(db.get_recipe(1).unwrap().recipe_mod.is_empty());
}

#[test]
fn small_tables_round_trip_whole_rows() {
    let db = fixture::db();
    assert_eq!(
        *db.get_cached_event("eventone").unwrap(),
        Event {
            id: 1,
            name: "EventOne".into(),
            start_time: -1,
            end_time: -1,
            state: 1,
            last_modified: d()
        }
    );
    assert_eq!(
        *db.get_cached_quest("OtherQuest").unwrap(),
        Quest {
            id: 2,
            name: "OtherQuest".into(),
            min_delta: 0,
            max_solves: 5,
            message: Some("done".into()),
            last_modified: d()
        }
    );
    assert_eq!(
        *db.get_cached_point_of_interest("holtburg").unwrap(),
        PointsOfInterest {
            id: 1,
            name: "Holtburg".into(),
            weenie_class_id: 300,
            last_modified: d()
        }
    );
    assert_eq!(
        db.get_version().unwrap(),
        Version {
            id: 1,
            base_version: Some("v0.8.8".into()),
            patch_version: Some("v0.9.294".into()),
            last_modified: d()
        }
    );
    let td = db.get_cached_death_treasure(5).unwrap();
    assert_eq!(
        (
            td.tier,
            td.loot_quality_mod,
            td.item_chance,
            td.mundane_item_type_selection_chances
        ),
        (1, 0.25, 100, 10)
    );
    let tw = db.get_cached_wielded_treasure(9);
    assert_eq!(tw.len(), 2);
    assert!(
        tw[0].set_start && !tw[0].has_sub_set && tw[1].has_sub_set && tw[1].continues_previous_set
    );
    assert_eq!(
        (tw[1].shade, tw[1].stack_size, tw[1].stack_size_variance),
        (0.5, 2, 0.25)
    );
    let pack = Pack::from_bytes(fixture::imported().0.clone()).unwrap();
    let gems = pack
        .all::<TreasureGemCount>(TableId::TREASURE_GEM_COUNT)
        .unwrap();
    assert_eq!(
        gems[0].1,
        TreasureGemCount {
            id: 1,
            gem_code: 2,
            tier: 1,
            count: 3,
            chance: 0.5
        }
    );
}

#[test]
fn a_missing_column_is_an_error_not_a_default() {
    let sql = "CREATE TABLE `event` (\n  `id` int(10) unsigned NOT NULL,\n  `name` varchar(255) NOT NULL\n) ENGINE=InnoDB;\n";
    let err = import(sql.as_bytes()).unwrap_err();
    assert!(
        matches!(err, ImportError::SchemaColumn { ref table, ref column } if table == "event" && column == "start_Time"),
        "{err}"
    );
}

#[test]
fn null_in_a_not_null_field_is_an_error() {
    let sql = "CREATE TABLE `version` (\n  `id` int(10) unsigned NOT NULL,\n  `base_Version` varchar(45),\n  `patch_Version` varchar(45),\n  `last_Modified` datetime NOT NULL\n) ENGINE=InnoDB;\n\
               INSERT INTO `version` VALUES (1,NULL,NULL,NULL);\n";
    let err = import(sql.as_bytes()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::UnexpectedNull {
                column: "last_Modified",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn a_bad_value_names_its_table_row_and_column() {
    let sql = "CREATE TABLE `version` (\n  `id` int(10) unsigned NOT NULL,\n  `base_Version` varchar(45),\n  `patch_Version` varchar(45),\n  `last_Modified` datetime NOT NULL\n) ENGINE=InnoDB;\n\
               INSERT INTO `version` VALUES (1,NULL,NULL,'2021-11-01 00:00:00'),(-2,NULL,NULL,'2021-11-01 00:00:00');\n";
    let err = import(sql.as_bytes()).unwrap_err();
    assert!(
        matches!(
            err,
            ImportError::BadValue {
                row: 2,
                column: "id",
                ..
            }
        ),
        "{err}"
    );
}

#[test]
fn the_same_dump_imports_to_identical_bytes() {
    let (a, ia) = import(fixture::dump().as_bytes()).unwrap();
    let (b, ib) = import(fixture::dump().as_bytes()).unwrap();
    assert_eq!(a, b);
    assert_eq!(ia.stats.content_hash, ib.stats.content_hash);
    // The dataset id is the dump's own hash, so any change to the dump changes it.
    let (_, ic) = import(fixture::dump().replace("EventOne", "EventUno").as_bytes()).unwrap();
    assert_ne!(ia.stats.dataset_id, ic.stats.dataset_id);
}

#[test]
fn the_report_counts_rows_and_names_the_hash() {
    let (bytes, imp) = fixture::imported();
    let r = imp.report_json();
    assert_eq!(r["dump_rows"]["weenie"], 5);
    assert_eq!(r["dump_rows"]["weenie_properties_d_i_d"], 3);
    assert_eq!(r["pack_bytes"], bytes.len() as u64);
    let pack = Pack::from_bytes(bytes.clone()).unwrap();
    assert_eq!(
        r["content_hash"],
        empyrean_content::pack::hex(&pack.header().content_hash)
    );
}

#[test]
fn mysqldump_rows_split_on_tuples_not_on_parentheses_in_strings() {
    #[derive(Default)]
    struct Collect(Vec<Vec<Option<Vec<u8>>>>);
    impl mysqldump::DumpSink for Collect {
        fn create_table(&mut self, _: &mysqldump::TableDef) -> Result<(), ImportError> {
            Ok(())
        }
        fn row(&mut self, row: &mysqldump::Row<'_>) -> Result<(), ImportError> {
            let cols = row.columns().to_vec();
            self.0.push(cols.iter().map(|_| None).collect());
            let last = self.0.last_mut().unwrap();
            for (i, c) in cols.iter().enumerate() {
                let c: &'static str = Box::leak(c.clone().into_boxed_str());
                last[i] = row.raw(c).unwrap().bytes().map(<[u8]>::to_vec);
            }
            Ok(())
        }
    }
    let sql = "CREATE TABLE `t` (\r\n  `a` text,\r\n  `b` int\r\n) ENGINE=InnoDB;\r\nINSERT INTO `t` VALUES ('x),(y',1),(NULL,2);\r\n";
    let mut c = Collect::default();
    mysqldump::scan(sql.as_bytes(), &mut c).unwrap();
    assert_eq!(c.0.len(), 2);
    assert_eq!(c.0[0][0].as_deref(), Some(&b"x),(y"[..]));
    assert_eq!(c.0[1][0], None);
    assert_eq!(c.0[1][1].as_deref(), Some(&b"2"[..]));

    let short = "CREATE TABLE `t` (\n  `a` int,\n  `b` int\n) ENGINE=InnoDB;\nINSERT INTO `t` VALUES (1);\n";
    assert!(matches!(
        mysqldump::scan(short.as_bytes(), &mut Collect::default()),
        Err(ImportError::Sql { line: 5, .. })
    ));
    let early = "INSERT INTO `t` VALUES (1);\n";
    assert!(matches!(
        mysqldump::scan(early.as_bytes(), &mut Collect::default()),
        Err(ImportError::Sql { .. })
    ));
}
