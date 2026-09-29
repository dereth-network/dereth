//! ACE: Source/ACE.Database/ShardDatabase.cs::ShardDatabase
//! ShardDatabase queries on both backends: max guid/counts, sequence gaps,
//! inventory/wielded/possessions, by landblock, by wcid/type/house/allegiance, remove, live
//! player biotas, allegiance rows.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::*;
use empyrean_entity::models::*;
use empyrean_entity::Biota;
use empyrean_store::models::shard::Character;
use empyrean_store::ShardDatabase;

use crate::support::backends;

fn item(id: u32, wcid: u32, weenie_type: WeenieType, iids: &[(u16, u32)]) -> Biota {
    let mut b = Biota {
        id,
        weenie_class_id: wcid,
        weenie_type,
        ..Default::default()
    };
    if !iids.is_empty() {
        let mut d = DotNetDict::new();
        for &(t, v) in iids {
            d.insert(PropertyInstanceId(t), v);
        }
        b.properties_iid = Some(d);
    }
    b
}

fn at(mut b: Biota, cell: u32) -> Biota {
    let mut d = DotNetDict::new();
    d.insert(
        PositionType::Location,
        PropertiesPosition {
            obj_cell_id: cell,
            rotation_w: 1.0,
            ..Default::default()
        },
    );
    b.properties_position = Some(d);
    b
}

fn save(db: &mut dyn ShardDatabase, b: Biota) {
    let mut b = b;
    assert!(db.save_biota(&mut b, false));
}

fn ids(v: &[empyrean_store::models::shard::Biota]) -> Vec<u32> {
    v.iter().map(|b| b.id).collect()
}

#[test]
fn max_guid_and_counts() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        assert_eq!(
            db.get_max_guid_found_in_range(0x8000_0000, 0x8000_FFFF),
            u32::MAX,
            "{name}: nothing found"
        );
        for id in [0x8000_0005, 0x8000_0002, 0x8001_0000, 0x5000_0001] {
            save(db, item(id, 1, WeenieType::Generic, &[]));
        }
        assert_eq!(
            db.get_max_guid_found_in_range(0x8000_0000, 0x8000_FFFF),
            0x8000_0005,
            "{name}"
        );
        assert_eq!(
            db.get_max_guid_found_in_range(0x5000_0001, 0x5000_0001),
            0x5000_0001,
            "{name}"
        );
        assert_eq!(db.get_biota_count(), 4, "{name}");
        assert_eq!(db.get_estimated_biota_count("ace_shard"), 4, "{name}");
    }
}

#[test]
fn sequence_gaps_follow_aces_user_variable_query() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        // ids above min: 103, 104, 108, 109, 115, 116, 200
        for id in [103, 104, 108, 109, 115, 116, 200, 50] {
            save(db, item(id, 1, WeenieType::Generic, &[]));
        }
        // The walk starts at the first id above min (103), so 101..=102 is not reported; each jump
        // prev -> id is the gap (prev + 1, id - 1); nothing after the last id.
        let all = vec![(105, 107), (110, 114), (117, 199)];
        assert_eq!(db.get_sequence_gaps(100, u32::MAX), all, "{name}");
        // The limit stops once the ids already returned reach it; the last gap can overshoot.
        assert_eq!(db.get_sequence_gaps(100, 3), vec![(105, 107)], "{name}");
        assert_eq!(
            db.get_sequence_gaps(100, 4),
            vec![(105, 107), (110, 114)],
            "{name}"
        );
        assert_eq!(db.get_sequence_gaps(100, 0), vec![], "{name}");
        assert_eq!(
            db.get_sequence_gaps(200, u32::MAX),
            vec![],
            "{name}: no ids above min"
        );
        assert_eq!(db.get_sequence_gaps(u32::MAX, u32::MAX), vec![], "{name}");
    }
}

#[test]
fn inventory_wielded_and_possessions() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        let player = 0x5000_0001;
        let pack = 0x8000_0010;
        save(db, item(player, 1, WeenieType::Creature, &[]));
        save(
            db,
            item(0x8000_0003, 2, WeenieType::Generic, &[(2, player)]),
        );
        save(db, item(pack, 3, WeenieType::Container, &[(2, player)]));
        save(
            db,
            item(0x8000_0001, 4, WeenieType::Generic, &[(2, player)]),
        );
        save(db, item(0x8000_0020, 5, WeenieType::Generic, &[(2, pack)]));
        save(db, item(0x8000_0011, 6, WeenieType::Generic, &[(2, pack)]));
        save(
            db,
            item(0x8000_0030, 7, WeenieType::MeleeWeapon, &[(3, player)]),
        );
        save(
            db,
            item(0x8000_0031, 8, WeenieType::Clothing, &[(3, 0x5000_0002)]),
        );

        // By object id (type_value_idx), each container followed by its contents (by id).
        let inv = db.get_inventory_in_parallel(player, true);
        assert_eq!(
            ids(&inv),
            vec![0x8000_0001, 0x8000_0003, pack, 0x8000_0011, 0x8000_0020],
            "{name}"
        );
        assert_eq!(
            ids(&db.get_inventory_in_parallel(player, false)),
            vec![0x8000_0001, 0x8000_0003, pack],
            "{name}"
        );
        assert_eq!(
            ids(&db.get_wielded_items_in_parallel(player)),
            vec![0x8000_0030],
            "{name}"
        );

        let p = db.get_possessed_biotas_in_parallel(player);
        assert_eq!(ids(&p.inventory).len(), 5, "{name}");
        assert_eq!(ids(&p.wielded_items), vec![0x8000_0030], "{name}");
        assert!(
            p.inventory
                .iter()
                .all(|b| !b.biota_properties_iid.is_empty()),
            "{name}: full biotas"
        );
    }
}

#[test]
fn objects_by_landblock() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        // Statics: ids 0x7LLLLxxx for landblock 0xA9B4.
        save(db, item(0x7A9B_4001, 1, WeenieType::Door, &[]));
        save(db, item(0x7A9B_4FFF, 1, WeenieType::Door, &[]));
        save(db, item(0x7A9B_5000, 1, WeenieType::Door, &[]));
        assert_eq!(
            ids(&db.get_static_objects_by_landblock(0xA9B4)),
            vec![0x7A9B_4001, 0x7A9B_4FFF],
            "{name}"
        );

        // Dynamics: a Location in the landblock's cells, not in a container, not wielded, id >= 0x80000000.
        save(
            db,
            at(item(0x8000_0009, 1, WeenieType::Generic, &[]), 0xA9B4_0031),
        );
        save(
            db,
            at(item(0x8000_0002, 1, WeenieType::Generic, &[]), 0xA9B4_0031),
        );
        save(
            db,
            at(item(0x8000_0001, 1, WeenieType::Generic, &[]), 0xA9B4_0100),
        );
        save(
            db,
            at(
                item(0x8000_0003, 1, WeenieType::Generic, &[(2, 0x5000_0001)]),
                0xA9B4_0001,
            ),
        );
        save(
            db,
            at(
                item(0x8000_0004, 1, WeenieType::Generic, &[(3, 0x5000_0001)]),
                0xA9B4_0001,
            ),
        );
        save(
            db,
            at(
                item(0x8000_0005, 1, WeenieType::Generic, &[(2, 0)]),
                0xA9B4_0002,
            ),
        );
        save(
            db,
            at(item(0x8000_0006, 1, WeenieType::Generic, &[]), 0xA9B5_0001),
        );
        save(
            db,
            at(item(0x5000_0001, 1, WeenieType::Creature, &[]), 0xA9B4_0001),
        );
        // type_cell_idx order: by cell, then object id.
        assert_eq!(
            ids(&db.get_dynamic_objects_by_landblock(0xA9B4)),
            vec![0x8000_0005, 0x8000_0002, 0x8000_0009, 0x8000_0001],
            "{name}"
        );
    }
}

#[test]
fn biotas_by_wcid_type_houses_and_allegiance() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        save(
            db,
            item(0x8000_0003, 42, WeenieType::SlumLord, &[(32, 0x5000_0001)]),
        );
        save(db, item(0x8000_0001, 42, WeenieType::SlumLord, &[]));
        save(db, item(0x8000_0002, 7, WeenieType::SlumLord, &[(32, 0)]));
        save(
            db,
            item(0x8000_0004, 9, WeenieType::Allegiance, &[(26, 0x5000_0001)]),
        );

        assert_eq!(
            ids(&db.get_biotas_by_wcid(42)),
            vec![0x8000_0001, 0x8000_0003],
            "{name}"
        );
        assert_eq!(
            ids(&db.get_biotas_by_type(WeenieType::SlumLord)),
            vec![0x8000_0001, 0x8000_0002, 0x8000_0003],
            "{name}"
        );

        // Any HouseOwner row counts, whatever its value; only the biota rows are loaded.
        let houses = db.get_houses_owned();
        assert_eq!(ids(&houses), vec![0x8000_0002, 0x8000_0003], "{name}");
        assert!(
            houses.iter().all(|h| h.biota_properties_iid.is_empty()),
            "{name}"
        );

        assert_eq!(
            db.get_allegiance_id(0x5000_0001),
            Some(0x8000_0004),
            "{name}"
        );
        // ACE-BUG: 0, not null, when there is none.
        assert_eq!(db.get_allegiance_id(0x5000_0002), Some(0), "{name}");
        assert!(db.get_biota(0, false).is_none(), "{name}");
    }
}

#[test]
fn remove_biota() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        assert!(
            db.remove_biota(0x8000_0001),
            "{name}: removing a missing biota succeeds"
        );
        save(db, item(0x8000_0001, 1, WeenieType::Generic, &[(2, 5)]));
        save(db, item(0x8000_0002, 1, WeenieType::Generic, &[]));
        assert!(
            db.remove_biotas_in_parallel(&[0x8000_0001, 0x8000_0003]),
            "{name}"
        );
        assert!(db.get_biota(0x8000_0001, false).is_none(), "{name}");
        assert!(
            db.get_inventory_in_parallel(5, false).is_empty(),
            "{name}: child rows went with it"
        );
        assert_eq!(db.get_biota_count(), 1, "{name}");
    }
}

#[test]
fn all_player_biotas_are_those_of_live_characters() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        for (id, deleted) in [
            (0x5000_0002u32, false),
            (0x5000_0001, false),
            (0x5000_0003, true),
            (0x5000_0004, false),
        ] {
            let c = Character {
                id,
                account_id: 1,
                name: format!("C{id:x}"),
                is_deleted: deleted,
                ..Default::default()
            };
            assert!(db.save_character(&c), "{name}");
            if id != 0x5000_0004 {
                save(db, item(id, 1, WeenieType::Creature, &[]));
            }
        }
        // 0x50000004 has no biota: logged and skipped.
        let biotas = db.get_all_player_biotas_in_parallel();
        assert_eq!(
            biotas.iter().map(|b| b.id).collect::<Vec<_>>(),
            vec![0x5000_0001, 0x5000_0002],
            "{name}"
        );
    }
}

#[test]
fn allegiance_rows_need_their_character() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        let mut a = item(0x8000_0040, 1, WeenieType::Allegiance, &[]);
        let mut members = DotNetDict::new();
        members.insert(
            0x5000_0001u32,
            PropertiesAllegiance {
                banned: true,
                approved_vassal: false,
            },
        );
        a.properties_allegiance = Some(members);
        assert!(
            !db.save_biota(&mut a.clone(), false),
            "{name}: foreign key to character"
        );
        assert!(db.save_character(&Character {
            id: 0x5000_0001,
            name: "M".into(),
            ..Default::default()
        }));
        assert!(db.save_biota(&mut a, false), "{name}");
        let row = db.get_biota(0x8000_0040, false).unwrap();
        assert_eq!(row.biota_properties_allegiance.len(), 1, "{name}");
    }
}
