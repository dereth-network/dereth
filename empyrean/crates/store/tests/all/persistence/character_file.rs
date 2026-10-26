//! Divergence: V459
//! A character file: its manifest written into a shard of one character and read back, the file
//! closed as one file, and a character read from one given this world's object ids.
//! Fixture: synthetic characters and objects in scratch SQLite files.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{PropertyDataId, PropertyInstanceId, PropertyInt, PropertyString};
use empyrean_entity::models::{PropertiesAllegiance, PropertiesEnchantmentRegistry};
use empyrean_entity::Biota;
use empyrean_store::character_file::{
    as_one_file, read_manifest, renumber, write_manifest, Class, Manifest, FORMAT,
};
use empyrean_store::models::shard::{
    Character, CharacterPropertiesFriendList, CharacterPropertiesQuestRegistry,
    CharacterPropertiesShortcutBar,
};
use empyrean_store::SqliteShard;
use std::path::{Path, PathBuf};

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("store-character-file")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn manifest() -> Manifest {
    Manifest {
        format: FORMAT,
        world: "sample-world".into(),
        era: "eor".into(),
        name: "Exporter".into(),
        id: 0x5000_0001,
        recipe: "version = 2\nlevel = 80\n".into(),
        hash: "0123456789abcdef".into(),
        checks: String::new(),
        made_by: "Empyrean 0.0.0".into(),
        classes: vec![
            Class {
                class: 1,
                name: "human".into(),
                symbol: None,
            },
            Class {
                class: 100_001,
                name: "coolsword".into(),
                symbol: Some("mypack:coolsword".into()),
            },
        ],
    }
}

#[test]
fn a_manifest_reads_back_as_written_and_a_plain_shard_has_none() {
    let dir = scratch("manifest");
    let path = dir.join("character.empyrean-character");
    let shard = SqliteShard::open(&path).unwrap();
    assert_eq!(
        read_manifest(&shard).unwrap(),
        None,
        "a shard is no character file"
    );
    write_manifest(&shard, &manifest()).unwrap();
    assert_eq!(read_manifest(&shard).unwrap(), Some(manifest()));
    // Written again, it is the new one alone.
    let mut other = manifest();
    other.classes.truncate(1);
    other.checks = "It should have 80 total skill credits, but has 81.".into();
    write_manifest(&shard, &other).unwrap();
    assert_eq!(read_manifest(&shard).unwrap(), Some(other));
    as_one_file(&shard).unwrap();
    drop(shard);
    let beside: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        beside,
        ["character.empyrean-character"],
        "no journal beside it"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

fn with_iid(mut b: Biota, key: PropertyInstanceId, id: u32) -> Biota {
    b.set_property(key, id);
    b
}

const PLAYER: u32 = 0x5000_0007;
const BAG: u32 = 0x8000_0010;
const GEM: u32 = 0x8000_0011;
const SWORD: u32 = 0x8000_0012;
/// Another world's object: a patron, a house, a caster.
const ELSEWHERE: u32 = 0x5000_0099;

#[test]
fn renumbering_gives_each_object_a_new_id_that_its_links_follow_and_drops_another_worlds() {
    let mut player = Biota {
        id: PLAYER,
        ..Default::default()
    };
    player.set_property(PropertyString::Name, "Exporter".to_owned());
    player.set_property(PropertyInstanceId::Patron, ELSEWHERE);
    player.set_property(PropertyInstanceId::House, ELSEWHERE);
    player.set_property(PropertyInt::AllegianceOfficerRank, 2);
    player.set_property(PropertyDataId::HouseId, 77);
    let mut allegiance = DotNetDict::new();
    allegiance.insert(ELSEWHERE, PropertiesAllegiance::default());
    player.properties_allegiance = Some(allegiance);
    let mut houses = DotNetDict::new();
    houses.insert(ELSEWHERE, true);
    player.house_permissions = Some(houses);
    player.properties_enchantment_registry = Some(vec![
        PropertiesEnchantmentRegistry {
            spell_id: 1,
            caster_object_id: SWORD,
            ..Default::default()
        },
        PropertiesEnchantmentRegistry {
            spell_id: 2,
            caster_object_id: ELSEWHERE,
            ..Default::default()
        },
    ]);
    let bag = with_iid(
        Biota {
            id: BAG,
            ..Default::default()
        },
        PropertyInstanceId::Container,
        PLAYER,
    );
    let gem = with_iid(
        Biota {
            id: GEM,
            ..Default::default()
        },
        PropertyInstanceId::Container,
        BAG,
    );
    let mut sword = with_iid(
        Biota {
            id: SWORD,
            ..Default::default()
        },
        PropertyInstanceId::Wielder,
        PLAYER,
    );
    sword.set_property(PropertyInstanceId::AllowedWielder, PLAYER);
    sword.set_property(PropertyInstanceId::Creator, ELSEWHERE);
    sword.set_property(PropertyString::CraftsmanName, "Exporter".to_owned());
    let mut items = vec![bag, gem, sword];
    let mut record = Character {
        id: PLAYER,
        account_id: 3,
        name: "Exporter".into(),
        is_plussed: true,
        total_logins: 40,
        character_properties_friend_list: vec![CharacterPropertiesFriendList {
            character_id: PLAYER,
            friend_id: ELSEWHERE,
        }],
        character_properties_shortcut_bar: vec![
            CharacterPropertiesShortcutBar {
                character_id: PLAYER,
                shortcut_bar_index: 1,
                shortcut_object_id: SWORD,
            },
            CharacterPropertiesShortcutBar {
                character_id: PLAYER,
                shortcut_bar_index: 2,
                shortcut_object_id: ELSEWHERE,
            },
        ],
        character_properties_quest_registry: vec![CharacterPropertiesQuestRegistry {
            character_id: PLAYER,
            quest_name: "PlaytestDoorFlag".into(),
            last_time_completed: 0,
            num_times_completed: 1,
        }],
        ..Character::default()
    };

    let mut next = 0x8000_0100;
    let ids = renumber(
        &mut player,
        &mut items,
        &mut record,
        (0x5000_0002, 9, "Imported Hero"),
        || {
            next += 1;
            next
        },
    );
    let [bag, gem, sword] = [&items[0], &items[1], &items[2]];
    assert_eq!(player.id, 0x5000_0002);
    assert_eq!(
        (bag.id, gem.id, sword.id),
        (0x8000_0101, 0x8000_0102, 0x8000_0103)
    );
    assert_eq!(ids.len(), 4);
    assert_eq!(ids[&PLAYER], 0x5000_0002);
    // What it holds and wears follows it.
    assert_eq!(
        bag.get_property(PropertyInstanceId::Container),
        Some(player.id)
    );
    assert_eq!(
        gem.get_property(PropertyInstanceId::Container),
        Some(bag.id)
    );
    assert_eq!(
        sword.get_property(PropertyInstanceId::Wielder),
        Some(player.id)
    );
    assert_eq!(
        sword.get_property(PropertyInstanceId::AllowedWielder),
        Some(player.id),
        "bonded to it still"
    );
    assert_eq!(
        sword.get_property(PropertyString::CraftsmanName).as_deref(),
        Some("Imported Hero")
    );
    assert_eq!(sword.get_property(PropertyInstanceId::Creator), None);
    // What named another world's is gone.
    for key in [PropertyInstanceId::Patron, PropertyInstanceId::House] {
        assert_eq!(player.get_property(key), None, "{key:?}");
    }
    assert_eq!(
        player.get_property(PropertyInt::AllegianceOfficerRank),
        None
    );
    assert_eq!(player.get_property(PropertyDataId::HouseId), None);
    assert!(player.properties_allegiance.as_ref().unwrap().is_empty());
    assert!(player.house_permissions.as_ref().unwrap().is_empty());
    let casters: Vec<u32> = player
        .properties_enchantment_registry
        .as_ref()
        .unwrap()
        .iter()
        .map(|e| e.caster_object_id)
        .collect();
    assert_eq!(casters, [sword.id, 0], "its sword's spell, and another's");
    assert_eq!(
        player.get_property(PropertyString::Name).as_deref(),
        Some("Imported Hero")
    );
    // The record: its new id, account and name, its quests kept, another world's friends gone.
    assert_eq!(
        (record.id, record.account_id, record.name.as_str()),
        (player.id, 9, "Imported Hero")
    );
    assert_eq!(record.total_logins, 1);
    assert!(record.character_properties_friend_list.is_empty());
    assert_eq!(
        record
            .character_properties_shortcut_bar
            .iter()
            .map(|s| (s.character_id, s.shortcut_object_id))
            .collect::<Vec<_>>(),
        [(player.id, sword.id)]
    );
    assert_eq!(
        record.character_properties_quest_registry[0].character_id,
        player.id
    );
}
