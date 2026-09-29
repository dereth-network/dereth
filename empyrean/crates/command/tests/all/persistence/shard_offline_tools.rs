//! ACE: Source/ACE.Database/ShardDatabaseOfflineTools.cs::ShardDatabaseOfflineTools
//! ShardDatabaseOfflineTools on MemShard and SqliteShard: purge a character and its possessions,
//! purge old deletions in parallel, purge orphaned biotas, prune deleted
//! friends/squelches/shortcuts, remove duplicate anim parts/texture maps.
//! Fixture: synthetic command arguments, handler tables and isolated server state.

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};
use empyrean_entity::enums::{PositionType, PropertyInstanceId, PropertyString, WeenieType};
use empyrean_entity::models::PropertiesPosition;
use empyrean_store::models::shard::{
    BiotaPropertiesAnimPart, BiotaPropertiesTextureMap, Character, CharacterPropertiesFriendList,
    CharacterPropertiesShortcutBar, CharacterPropertiesSpellBar, CharacterPropertiesSquelch,
};
use empyrean_store::shard_database_offline_tools as tools;
use empyrean_store::{MemShard, ShardDatabase, SqliteShard};

const A: u32 = 0x5000_0001;
const B: u32 = 0x5000_0002;
const MISSING_CHARACTER: u32 = 0x5000_0003;
const D: u32 = 0x5000_0004;
const E: u32 = 0x5000_0005;
const M: u32 = 0x5000_0009;

const PACK: u32 = 0x8000_0001;
const IN_PACK: u32 = 0x8000_0002;
const WIELDED: u32 = 0x8000_0003;
const B_ITEM: u32 = 0x8000_0004;
const LOST_CONTAINER: u32 = 0x8000_0005;
const NOWHERE: u32 = 0x8000_0006;
const ALLEGIANCE_1: u32 = 0x8000_0007;
const ALLEGIANCE_2: u32 = 0x8000_0008;
const ORPHAN_ALLEGIANCE: u32 = 0x8000_0009;
const GONE_MONARCH: u32 = 0x5000_0999;
const GONE: u32 = 0x8000_0999;

/// Both backends, fresh.
fn backends() -> Vec<Box<dyn ShardDatabase>> {
    vec![
        Box::new(MemShard::new()),
        Box::new(SqliteShard::open_in_memory().expect("an in-memory SQLite shard")),
    ]
}

fn located() -> PropertiesPosition {
    PropertiesPosition {
        obj_cell_id: 0xA9B4_0019,
        position_x: 10.0,
        position_y: 10.0,
        rotation_w: 1.0,
        ..PropertiesPosition::default()
    }
}

fn biota<R>(
    db: &mut dyn ShardDatabase,
    id: u32,
    weenie_type: WeenieType,
    edit: impl FnOnce(&mut empyrean_entity::Biota) -> R,
) {
    let mut b = empyrean_entity::Biota {
        id,
        weenie_class_id: 1,
        weenie_type,
        ..Default::default()
    };
    b.set_property(PropertyString::Name, format!("Object {id:08X}"));
    let _ = edit(&mut b);
    assert!(db.save_biota(&mut b, true));
}

fn player(db: &mut dyn ShardDatabase, id: u32) {
    biota(db, id, WeenieType::Creature, |b| {
        b.properties_position
            .get_or_insert_with(DotNetDict::new)
            .insert(PositionType::Location, located());
    });
}

fn character(db: &mut dyn ShardDatabase, id: u32, edit: impl FnOnce(&mut Character)) {
    let mut c = Character {
        id,
        account_id: 1,
        name: format!("Character {id:08X}"),
        ..Character::default()
    };
    edit(&mut c);
    assert!(db.save_character(&c));
}

fn exists(db: &mut dyn ShardDatabase, id: u32) -> bool {
    tools::load_biota(db, id).is_some()
}

fn has_character(db: &mut dyn ShardDatabase, id: u32) -> bool {
    tools::all_characters(db).iter().any(|c| c.id == id)
}

/// A (deleted long ago) with a pack holding an item, and a wielded item; B (current) with an item.
fn seed_a_and_b(db: &mut dyn ShardDatabase) {
    player(db, A);
    character(db, A, |c| {
        c.is_deleted = true;
        c.delete_time = 1;
    });
    biota(db, PACK, WeenieType::Container, |b| {
        b.set_property(PropertyInstanceId::Container, A)
    });
    biota(db, IN_PACK, WeenieType::Generic, |b| {
        b.set_property(PropertyInstanceId::Container, PACK)
    });
    biota(db, WIELDED, WeenieType::MeleeWeapon, |b| {
        b.set_property(PropertyInstanceId::Wielder, A)
    });
    player(db, B);
    character(db, B, |_| {});
    biota(db, B_ITEM, WeenieType::Generic, |b| {
        b.set_property(PropertyInstanceId::Container, B)
    });
}

#[test]
fn purge_character_removes_the_possessions_the_biota_and_the_row() {
    for mut db in backends() {
        let db = &mut *db;
        seed_a_and_b(db);
        assert_eq!(
            tools::purge_character(db, A, Some("test")),
            (1, 1, 3),
            "characters, player biotas, possessions"
        );
        for id in [A, PACK, IN_PACK, WIELDED] {
            assert!(!exists(db, id), "{id:08X} purged");
        }
        assert!(!has_character(db, A));
        assert!(
            exists(db, B) && exists(db, B_ITEM) && has_character(db, B),
            "B untouched"
        );
        // nothing left to purge
        assert_eq!(tools::purge_character(db, A, None), (0, 0, 0));
    }
}

#[test]
fn purge_characters_in_parallel_purges_old_deletions_only() {
    let now = DotNetDateTime::UNIX_EPOCH.add_days(100.0);
    for mut db in backends() {
        let db = &mut *db;
        seed_a_and_b(db);
        // D: pending deletion yesterday (kept at 30 days); E: marked deleted with no time (purged)
        player(db, D);
        character(db, D, |c| c.delete_time = 99 * 86_400);
        player(db, E);
        character(db, E, |c| c.is_deleted = true);
        assert_eq!(tools::purge_characters_in_parallel(db, 30, now), (2, 2, 3));
        assert!(!has_character(db, A) && !has_character(db, E));
        assert!(has_character(db, D) && has_character(db, B));
    }
}

#[test]
fn purge_orphaned_biotas_purges_each_kind_of_orphan() {
    for mut db in backends() {
        let db = &mut *db;
        seed_a_and_b(db);
        // D: a character without a biota; E: a player biota without a character
        character(db, D, |_| {});
        player(db, E);
        // an item whose container is gone, and an item with no container, wielder or location
        biota(db, LOST_CONTAINER, WeenieType::Generic, |b| {
            b.set_property(PropertyInstanceId::Container, GONE)
        });
        biota(db, NOWHERE, WeenieType::Generic, |_| {});
        // two allegiances of one monarch: the second is an unused duplicate
        player(db, M);
        character(db, M, |_| {});
        for id in [ALLEGIANCE_1, ALLEGIANCE_2] {
            biota(db, id, WeenieType::Allegiance, |b| {
                b.set_property(PropertyInstanceId::Monarch, M)
            });
        }
        // V365 (a fix): an allegiance whose monarch is gone is purged (ACE's check never fired)
        biota(db, ORPHAN_ALLEGIANCE, WeenieType::Allegiance, |b| {
            b.set_property(PropertyInstanceId::Monarch, GONE_MONARCH)
        });

        // D's row (1), E's biota (1), the lost item (1), the item nowhere (1), the duplicate (1),
        // the orphaned allegiance (1)
        assert_eq!(tools::purge_orphaned_biotas_in_parallel(db), 6);
        assert!(!has_character(db, D) && !exists(db, E));
        assert!(!exists(db, LOST_CONTAINER) && !exists(db, NOWHERE));
        assert!(
            exists(db, ALLEGIANCE_1) && !exists(db, ALLEGIANCE_2) && !exists(db, ORPHAN_ALLEGIANCE)
        );
        for id in [A, PACK, IN_PACK, WIELDED, B, B_ITEM, M] {
            assert!(exists(db, id), "{id:08X} kept");
        }
        assert_eq!(
            tools::purge_orphaned_biotas_in_parallel(db),
            0,
            "nothing left"
        );
    }
}

#[test]
fn prunes_remove_deleted_friends_squelches_and_shortcuts() {
    for mut db in backends() {
        let db = &mut *db;
        seed_a_and_b(db);
        character(db, B, |c| {
            c.character_properties_friend_list = [A, MISSING_CHARACTER]
                .into_iter()
                .map(|friend_id| CharacterPropertiesFriendList {
                    character_id: B,
                    friend_id,
                })
                .chain([CharacterPropertiesFriendList {
                    character_id: B,
                    friend_id: B,
                }])
                .collect();
            c.character_properties_shortcut_bar = vec![
                CharacterPropertiesShortcutBar {
                    character_id: B,
                    shortcut_bar_index: 0,
                    shortcut_object_id: GONE,
                },
                CharacterPropertiesShortcutBar {
                    character_id: B,
                    shortcut_bar_index: 1,
                    shortcut_object_id: B_ITEM,
                },
            ];
            c.character_properties_squelch = vec![
                CharacterPropertiesSquelch {
                    character_id: B,
                    squelch_character_id: A,
                    squelch_account_id: 0,
                    r#type: 1,
                },
                CharacterPropertiesSquelch {
                    character_id: B,
                    squelch_character_id: MISSING_CHARACTER,
                    squelch_account_id: 5,
                    r#type: 1,
                },
            ];
            c.character_properties_spell_bar = vec![CharacterPropertiesSpellBar {
                character_id: B,
                spell_bar_number: 1,
                spell_bar_index: 1,
                spell_id: 3,
            }];
        });

        assert_eq!(
            tools::prune_deleted_characters_from_friend_lists(db),
            2,
            "A (deleted) and the missing character"
        );
        assert_eq!(tools::prune_deleted_objects_from_shortcut_bars(db), 1);
        assert_eq!(
            tools::prune_deleted_characters_from_squelch_lists(db),
            1,
            "account squelches stay"
        );
        let b = db.get_character(B).expect("B");
        assert_eq!(
            b.character_properties_friend_list
                .iter()
                .map(|f| f.friend_id)
                .collect::<Vec<_>>(),
            [B]
        );
        assert_eq!(
            b.character_properties_shortcut_bar
                .iter()
                .map(|s| s.shortcut_object_id)
                .collect::<Vec<_>>(),
            [B_ITEM]
        );
        assert_eq!(
            b.character_properties_squelch
                .iter()
                .map(|s| s.squelch_account_id)
                .collect::<Vec<_>>(),
            [5]
        );
        assert_eq!(
            b.character_properties_spell_bar.len(),
            1,
            "other lists untouched"
        );
        assert_eq!(tools::prune_deleted_characters_from_friend_lists(db), 0);

        // the spell bar patch is applied (no bar numbered 0): the check returns
        tools::check_for_pr2918_script(db);
        tools::check_for_biota_properties_palette_order_column_in_shard(db);
    }
}

#[test]
fn duplicates_of_unordered_anim_parts_and_texture_maps_are_removed() {
    for mut db in backends() {
        let db = &mut *db;
        biota(db, PACK, WeenieType::Generic, |_| {});
        let mut b = tools::load_biota(db, PACK).expect("stored");
        let part = |index: u8, order: Option<u8>| BiotaPropertiesAnimPart {
            id: 0,
            object_id: PACK,
            index,
            animation_id: 0x0100_0001,
            order,
        };
        b.biota_properties_anim_part = vec![
            part(0, None),
            part(1, None),
            part(0, Some(0)),
            part(1, Some(1)),
        ];
        let tex = |index: u8, order: Option<u8>| BiotaPropertiesTextureMap {
            id: 0,
            object_id: PACK,
            index,
            old_id: 1,
            new_id: 2,
            order,
        };
        b.biota_properties_texture_map = vec![tex(0, None), tex(0, Some(0))];
        empyrean_store::shard_database::set_biota_populated_collections(&mut b);
        db.write_biota(&mut b).expect("written");
        // a biota with ordered rows only keeps them
        biota(db, B_ITEM, WeenieType::Generic, |_| {});
        let mut c = tools::load_biota(db, B_ITEM).expect("stored");
        c.biota_properties_anim_part = vec![BiotaPropertiesAnimPart {
            id: 0,
            object_id: B_ITEM,
            index: 0,
            animation_id: 5,
            order: Some(0),
        }];
        empyrean_store::shard_database::set_biota_populated_collections(&mut c);
        db.write_biota(&mut c).expect("written");

        assert_eq!(tools::fix_anim_part_and_texture_map_from_pr2731(db), 3);
        let b = tools::load_biota(db, PACK).expect("stored");
        assert!(
            b.biota_properties_anim_part
                .iter()
                .all(|p| p.order.is_none())
                && b.biota_properties_anim_part.len() == 2
        );
        assert!(
            b.biota_properties_texture_map
                .iter()
                .all(|p| p.order.is_none())
                && b.biota_properties_texture_map.len() == 1
        );
        assert_eq!(
            tools::load_biota(db, B_ITEM)
                .expect("stored")
                .biota_properties_anim_part
                .len(),
            1
        );
        assert_eq!(tools::fix_anim_part_and_texture_map_from_pr2731(db), 0);
    }
}
