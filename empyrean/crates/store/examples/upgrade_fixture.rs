//! Writes this release's upgrade fixture: a small synthetic shard and authentication database made
//! by the current code, and `expected.json`, what they hold as the store reads it back.
//!
//! ```text
//! cargo run -p empyrean-store --example upgrade_fixture -- empyrean/crates/store/tests/fixtures/upgrade/<version>
//! ```
//!
//! The folder must not exist yet (a committed fixture is never rewritten). The databases are
//! `shard.sqlite` and `auth.sqlite`, each one self-contained file (no write-ahead log). See
//! `UPGRADES.md` beside `Cargo.toml`.

#[path = "../tests/all/support/upgrade_snapshot.rs"]
mod upgrade_snapshot;

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::VirtualClock;
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::*;
use empyrean_entity::models::*;
use empyrean_entity::Biota;
use empyrean_store::models::shard::{
    Character, CharacterPropertiesFriendList, CharacterPropertiesQuestRegistry,
    CharacterPropertiesShortcutBar, CharacterPropertiesSpellBar,
};
use empyrean_store::{AuthDatabase, ShardDatabase, SqliteAuth, SqliteShard};

fn dict<K: std::hash::Hash + Eq + Clone, V>(
    items: impl IntoIterator<Item = (K, V)>,
) -> DotNetDict<K, V> {
    let mut d = DotNetDict::new();
    for (k, v) in items {
        d.insert(k, v);
    }
    d
}

/// A player: the properties a character is made of.
fn player(id: u32, name: &str, level: i32) -> Biota {
    let mut b = Biota {
        id,
        weenie_class_id: 1,
        weenie_type: WeenieType(10),
        ..Default::default()
    };
    b.properties_string = Some(dict([(PropertyString(1), name.to_owned())]));
    b.properties_int = Some(dict([
        (PropertyInt(25), level),
        (PropertyInt(24), 0x8000),
        (PropertyInt(113), 1),
    ]));
    b.properties_int64 = Some(dict([
        (PropertyInt64(1), 1_234_567_890_123),
        (PropertyInt64(2), 9_876_543_210),
    ]));
    b.properties_bool = Some(dict([(PropertyBool(1), true), (PropertyBool(52), false)]));
    b.properties_float = Some(dict([(PropertyFloat(21), 0.125)]));
    b.properties_did = Some(dict([
        (PropertyDataId(1), 0x0200_0001),
        (PropertyDataId(3), 0x2000_0001),
    ]));
    b.properties_position = Some(dict([(
        PositionType(1),
        PropertiesPosition {
            obj_cell_id: 0xA9B4_0019,
            position_x: 84.0,
            position_y: 7.1,
            position_z: 94.005,
            rotation_w: 0.707_106_77,
            rotation_x: 0.0,
            rotation_y: 0.0,
            rotation_z: -0.707_106_77,
        },
    )]));
    b.properties_attribute = Some(dict((1..=6).map(|a| {
        (
            PropertyAttribute(a),
            PropertiesAttribute {
                init_level: 10 + u32::from(a) * 10,
                level_from_cp: u32::from(a),
                cp_spent: 1000 * u32::from(a),
            },
        )
    })));
    b.properties_skill = Some(dict([6, 14, 31].map(|s| {
        (
            Skill(s),
            PropertiesSkill {
                level_from_pp: 5,
                sac: SkillAdvancementClass(2),
                pp: 1200,
                init_level: 10,
                ..Default::default()
            },
        )
    })));
    b.properties_spell_book = Some(dict([(1, 2.0f32), (157, 2.0)]));
    b
}

/// An item `holder` carries (`slot` 2, `Container`) or wields (`slot` 3, `Wielder`).
fn item(id: u32, wcid: u32, name: &str, holder: u32, slot: u16, stack: i32) -> Biota {
    let mut b = Biota {
        id,
        weenie_class_id: wcid,
        weenie_type: WeenieType(1),
        ..Default::default()
    };
    b.properties_string = Some(dict([(PropertyString(1), name.to_owned())]));
    b.properties_int = Some(dict([(PropertyInt(12), stack), (PropertyInt(19), 25)]));
    b.properties_iid = Some(dict([(PropertyInstanceId(slot), holder)]));
    b
}

fn character(id: u32, account_id: u32, name: &str) -> Character {
    Character {
        id,
        account_id,
        name: name.to_owned(),
        total_logins: 7,
        character_options_1: 0x5000_0000,
        character_options_2: 0x0094_8700,
        last_login_timestamp: 1_000_000.5,
        character_properties_friend_list: vec![CharacterPropertiesFriendList {
            character_id: id,
            friend_id: 0x5000_0002,
        }],
        character_properties_quest_registry: vec![CharacterPropertiesQuestRegistry {
            character_id: id,
            quest_name: "PathwardenComplete".to_owned(),
            last_time_completed: 1_700_000_000,
            num_times_completed: 1,
        }],
        character_properties_shortcut_bar: vec![CharacterPropertiesShortcutBar {
            character_id: id,
            shortcut_bar_index: 1,
            shortcut_object_id: 0x8000_0001,
        }],
        character_properties_spell_bar: vec![CharacterPropertiesSpellBar {
            character_id: id,
            spell_bar_number: 1,
            spell_bar_index: 1,
            spell_id: 157,
        }],
        ..Default::default()
    }
}

fn write(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        return Err(format!(
            "{} exists: a committed fixture is never rewritten",
            dir.display()
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let shard_path = dir.join("shard.sqlite");
    let auth_path = dir.join("auth.sqlite");
    let clock = Arc::new(VirtualClock::default());
    let e = |e: empyrean_store::StoreError| e.to_string();

    let mut auth = SqliteAuth::open(&auth_path, AccountDefaults::default(), clock).map_err(e)?;
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let admin = auth
        .create_account(
            "fixtureadmin",
            "fixture-password-1",
            AccessLevel::Admin,
            local,
        )
        .map_err(e)?;
    let player_account = auth
        .create_account(
            "fixtureplayer",
            "fixture-password-2",
            AccessLevel::Player,
            local,
        )
        .map_err(e)?;

    let mut shard = SqliteShard::open(&shard_path).map_err(e)?;
    let characters = [
        (0x5000_0001, admin.account_id, "Fixture Admin", 126),
        (0x5000_0002, player_account.account_id, "Fixture Player", 40),
        (0x5000_0003, player_account.account_id, "Fixture Alt", 12),
    ];
    let mut next_item = 0x8000_0001;
    for (id, account, name, level) in characters {
        let mut body = player(id, name, level);
        let mut possessions = vec![
            item(next_item, 273, "Pyreal", id, 2, 25_000),
            item(next_item + 1, 7_378, "Healing Kit", id, 2, 1),
            item(next_item + 2, 22_059, "Sword", id, 3, 1),
        ];
        next_item += 3;
        let c = character(id, account, name);
        if !shard.add_character_in_parallel(&mut body, &mut possessions, &c) {
            return Err(format!("could not save {name}"));
        }
    }
    // One deleted character, kept by the store until purged.
    let mut deleted = character(0x5000_0004, player_account.account_id, "Fixture Gone");
    deleted.is_deleted = true;
    deleted.delete_time = 1_700_000_000;
    let mut body = player(0x5000_0004, "Fixture Gone", 1);
    if !shard.add_character_in_parallel(&mut body, &mut [], &deleted) {
        return Err("could not save the deleted character".to_owned());
    }

    let expected = upgrade_snapshot::snapshot(&mut auth, &mut shard);

    drop(auth);
    drop(shard);

    // One self-contained file each: the write-ahead log folded in, then a rollback journal.
    for path in [&shard_path, &auth_path] {
        let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
        empyrean_store::collation::register(&conn).map_err(|e| e.to_string())?;
        conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode = DELETE; VACUUM;",
        )
        .map_err(|e| e.to_string())?;
    }

    let text = serde_json::to_string_pretty(&expected).map_err(|e| e.to_string())? + "\n";
    std::fs::write(dir.join("expected.json"), text).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() -> ExitCode {
    let Some(dir) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: upgrade_fixture <folder>  (tests/fixtures/upgrade/<version>)");
        return ExitCode::from(2);
    };
    match write(&dir) {
        Ok(()) => {
            println!(
                "wrote the {} upgrade fixture to {}",
                env!("CARGO_PKG_VERSION"),
                dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("upgrade_fixture: {e}");
            ExitCode::FAILURE
        }
    }
}
