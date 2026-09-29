//! ACE: Source/ACE.Database/ShardDatabase.cs::ShardDatabase
//! Character save/list, names/stubs, rename/add, spell bar shifts like ACE, other
//! CharacterExtensions, spellbook filter column default on insert.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use empyrean_store::models::shard::*;

use crate::support::backends;

fn character(id: u32, account_id: u32, name: &str) -> Character {
    Character {
        id,
        account_id,
        name: name.into(),
        spellbook_filters: 16383,
        ..Default::default()
    }
}

#[test]
fn save_and_list_characters() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        let mut a = character(0x5000_0002, 7, "Beta");
        a.add_friend(0x5000_0009);
        a.get_or_create_quest("ZQuest").0.num_times_completed = 2;
        a.get_or_create_quest("aquest").0.last_time_completed = 5;
        a.add_spell_to_bar(0, 0, 1636);
        a.add_title_to_registry(3);
        a.add_title_to_registry(1);
        a.gameplay_options = Some(vec![1, 2, 3]);
        a.delete_time = u64::MAX; // bigint unsigned, stored in 64 bits
        assert!(db.save_character(&a), "{name}");
        assert!(
            db.save_character(&character(0x5000_0001, 7, "Alpha")),
            "{name}"
        );
        let mut deleted = character(0x5000_0003, 7, "Gamma");
        deleted.is_deleted = true;
        assert!(db.save_character(&deleted), "{name}");
        assert!(
            db.save_character(&character(0x5000_0004, 8, "Delta")),
            "{name}"
        );

        let list = db.get_characters(7, false);
        assert_eq!(
            list.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![0x5000_0001, 0x5000_0002],
            "{name}: by id"
        );
        assert_eq!(db.get_characters(7, true).len(), 3, "{name}");

        let beta = db.get_character(0x5000_0002).unwrap();
        // Property rows come back in primary-key order with the owner's id filled in (quest names
        // compare case-insensitively).
        let quests: Vec<&str> = beta
            .character_properties_quest_registry
            .iter()
            .map(|q| q.quest_name.as_str())
            .collect();
        assert_eq!(quests, vec!["aquest", "ZQuest"], "{name}");
        assert!(
            beta.character_properties_quest_registry
                .iter()
                .all(|q| q.character_id == 0x5000_0002),
            "{name}"
        );
        assert_eq!(
            beta.character_properties_title_book
                .iter()
                .map(|t| t.title_id)
                .collect::<Vec<_>>(),
            vec![1, 3],
            "{name}"
        );
        assert_eq!(beta.gameplay_options, Some(vec![1, 2, 3]), "{name}");
        assert_eq!(beta.delete_time, u64::MAX, "{name}");
        assert!(beta.has_as_friend(0x5000_0009), "{name}");
        assert!(
            db.get_character(0x5000_0003).is_some(),
            "{name}: GetCharacter includes deleted"
        );
        assert!(db.get_character(0x5000_0099).is_none(), "{name}");
    }
}

#[test]
fn names_and_stubs() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        let mut pending = character(0x5000_0005, 1, "Pending");
        pending.delete_time = 1_700_000_000;
        assert!(db.save_character(&pending));
        let mut gone = character(0x5000_0006, 1, "Gone");
        gone.is_deleted = true;
        assert!(db.save_character(&gone));
        let mut live = character(0x5000_0007, 1, "Live");
        live.add_friend(1);
        assert!(db.save_character(&live));

        // MySQL's collation compares names case-insensitively.
        assert!(!db.is_character_name_available("LIVE"), "{name}");
        // A character awaiting deletion (delete_Time > 0) is filtered out, so its name counts as free.
        assert!(db.is_character_name_available("pending"), "{name}");
        assert!(
            db.is_character_name_available("Gone"),
            "{name}: deleted names are free"
        );
        assert!(db.is_character_name_available("Nobody"), "{name}");

        // Stubs: no property lists.
        let stub = db.get_character_stub_by_name("live").unwrap();
        assert_eq!(stub.id, 0x5000_0007, "{name}");
        assert!(stub.character_properties_friend_list.is_empty(), "{name}");
        assert!(
            db.get_character_stub_by_name("Gone").is_none(),
            "{name}: only non-deleted by name"
        );
        assert_eq!(
            db.get_character_stub_by_guid(0x5000_0006).map(|c| c.name),
            Some("Gone".into()),
            "{name}"
        );
    }
}

#[test]
fn rename_and_add_character() {
    for (name, mut db) in backends() {
        let db = db.as_mut();
        let mut c = character(0x5000_0010, 3, "Old");
        let mut biota = empyrean_entity::Biota {
            id: 0x5000_0010,
            weenie_class_id: 1,
            ..Default::default()
        };
        let mut possessions = vec![empyrean_entity::Biota {
            id: 0x8000_0100,
            weenie_class_id: 2,
            ..Default::default()
        }];
        assert!(
            db.add_character_in_parallel(&mut biota, &mut possessions, &c),
            "{name}"
        );
        assert!(db.get_biota(0x8000_0100, false).is_some(), "{name}");

        assert!(db.rename_character(&mut c, "New"), "{name}");
        assert_eq!(c.name, "New", "{name}: the snapshot is renamed");
        assert_eq!(db.get_character(0x5000_0010).unwrap().name, "New", "{name}");
        assert!(db.is_character_name_available("Old"), "{name}");
    }
}

#[test]
fn spell_bar_shifts_like_ace() {
    let mut c = character(1, 1, "S");
    assert!(c.add_spell_to_bar(0, 0, 10));
    assert!(c.add_spell_to_bar(0, 1, 20));
    assert!(c.add_spell_to_bar(0, 0, 30)); // before 10: 10 and 20 move up
    assert!(c.add_spell_to_bar(0, 99, 40)); // past the end: clamped to the count
    assert!(!c.add_spell_to_bar(0, 0, 20), "already in this bar");
    assert!(c.add_spell_to_bar(1, 0, 20), "another bar");
    let bar0: Vec<(u32, u32)> = c
        .get_spells_in_bar(0)
        .iter()
        .map(|s| (s.spell_bar_index, s.spell_id))
        .collect();
    assert_eq!(bar0, vec![(1, 30), (2, 10), (3, 20), (4, 40)]);

    let removed = c.try_remove_spell_from_bar(0, 10).unwrap();
    assert_eq!(removed.spell_bar_index, 2);
    let bar0: Vec<(u32, u32)> = c
        .get_spells_in_bar(0)
        .iter()
        .map(|s| (s.spell_bar_index, s.spell_id))
        .collect();
    assert_eq!(bar0, vec![(1, 30), (2, 20), (3, 40)]);
    assert!(c.try_remove_spell_from_bar(0, 10).is_none());
    assert_eq!(c.get_spells_in_bar(1).len(), 1);
}

#[test]
fn other_character_extensions() {
    let mut c = character(9, 1, "E");
    // Shortcuts are stored at index + 1.
    c.add_or_update_shortcut(0, 0x8000_0001);
    c.add_or_update_shortcut(0, 0x8000_0002);
    c.add_or_update_shortcut(4, 0x8000_0003);
    assert_eq!(
        c.get_shortcuts()
            .iter()
            .map(|s| (s.shortcut_bar_index, s.shortcut_object_id))
            .collect::<Vec<_>>(),
        vec![(1, 0x8000_0002), (5, 0x8000_0003)]
    );
    assert!(c.try_remove_shortcut(4).is_some());
    assert!(c.try_remove_shortcut(4).is_none());

    // Quests match OrdinalIgnoreCase.
    let (q, created) = c.get_or_create_quest("KillTheThing");
    assert!(created);
    assert_eq!(q.character_id, 0, "left for the save to fill in, as in ACE");
    assert!(!c.get_or_create_quest("killthething").1);
    assert!(c.get_quest("KILLTHETHING").is_some());
    assert!(c.erase_quest("killTHEthing"));
    assert!(!c.erase_quest("killTHEthing"));
    c.get_or_create_quest("a");
    c.get_or_create_quest("b");
    assert_eq!(c.erase_all_quests(), vec!["a".to_string(), "b".to_string()]);

    // Contracts.
    let (k, created) = c.get_or_create_contract(12);
    k.set_as_display_contract = true;
    assert!(created);
    assert!(c.get_contract(12).unwrap().set_as_display_contract);
    assert_eq!(c.get_contracts_count(), 1);
    assert_eq!(c.get_contracts_ids(), vec![12]);
    assert!(c.erase_contract(12).is_some());
    c.get_or_create_contract(1);
    assert_eq!(c.erase_all_contracts().len(), 1);

    // Fill components: (int) casts of the wcid and amount.
    let (f, exists) = c.add_fill_component(0x8000_0001, 5);
    assert!(!exists);
    assert_eq!(f.spell_component_id, i32::MIN + 1);
    // The int SpellComponentId never equals a uint wcid >= 2^31 (both widen to long), so the lookup
    // misses and a second row is added.
    assert!(!c.add_fill_component(0x8000_0001, 9).1);
    assert_eq!(c.get_fill_components().len(), 2);
    assert!(c.get_fill_component(0x8000_0001).is_none());
    assert!(c.try_remove_fill_component(0x8000_0001).is_none());
    c.character_properties_fill_comp_book.clear();
    c.add_fill_component(691, 5);
    assert_eq!(c.get_fill_component(691).unwrap().quantity_to_rebuy, 5);
    assert!(c.try_remove_fill_component(691).is_some());

    // Friends, squelches, titles.
    assert!(!c.add_friend(5).1);
    assert!(c.add_friend(5).1);
    assert!(c.try_remove_friend(5).is_some());
    c.add_friend(6);
    assert!(c.clear_all_friends());
    assert!(c.get_friends().is_empty());
    c.add_or_update_squelch(7, 70, 1);
    c.add_or_update_squelch(7, 71, 2);
    assert_eq!(c.get_squelches().len(), 1);
    assert!(c.try_remove_squelch(7, 70).is_none(), "account must match");
    assert!(c.try_remove_squelch(7, 71).is_some());
    assert_eq!(c.add_title_to_registry(4), (false, 1));
    assert_eq!(c.add_title_to_registry(4), (true, 1));
    assert_eq!(c.get_titles().len(), 1);
}

/// Spellbook filters take the column default on insert only.
#[test]
fn spellbook_filters_take_the_column_default_on_insert_only() {
    assert_eq!(Character::default().spellbook_filters, 16383);
    assert_eq!(SPELLBOOK_FILTERS_DEFAULT, 16383);

    for (name, mut db) in backends() {
        let db = db.as_mut();
        let mut c = Character {
            spellbook_filters: 0,
            ..character(0x5000_0001, 7, "Alpha")
        };
        assert!(db.save_character(&c), "{name}");
        assert_eq!(
            db.get_character(0x5000_0001).unwrap().spellbook_filters,
            16383,
            "{name}: inserted 0 reads back the default"
        );

        assert!(db.save_character(&c), "{name}");
        assert_eq!(
            db.get_character(0x5000_0001).unwrap().spellbook_filters,
            0,
            "{name}: an update writes 0"
        );

        c.spellbook_filters = 5;
        assert!(db.save_character(&c), "{name}");
        assert_eq!(
            db.get_character(0x5000_0001).unwrap().spellbook_filters,
            5,
            "{name}"
        );

        let explicit = Character {
            spellbook_filters: 7,
            ..character(0x5000_0002, 7, "Beta")
        };
        assert!(db.save_character(&explicit), "{name}");
        assert_eq!(
            db.get_character(0x5000_0002).unwrap().spellbook_filters,
            7,
            "{name}: a set value is inserted as is"
        );
    }
}
