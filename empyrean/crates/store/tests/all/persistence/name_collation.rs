//! Vectors: tests/fixtures/collation_uca1400.json and uca1400_all_weights.json
//! Utf8mb4_uca1400_ai_ci weights, pairs and orders equal MariaDB in Rust and SQLite;
//! character/quest/config/account names match like ACE.
//! Fixture: tests/fixtures/collation_uca1400.json and locally constructed edge cases.

use std::cmp::Ordering;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::VirtualClock;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_entity::enums::AccessLevel;
use empyrean_store::collation::{self, CollationKey};
use empyrean_store::models::shard::Character;
use empyrean_store::{AuthDatabase, MemAuth, ShardConfigDatabase, SqliteAuth, SqliteShard};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::support::backends;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../fixtures/collation_uca1400.json")).unwrap()
}

fn hex_weights(w: &[u16]) -> String {
    w.iter().map(|x| format!("{x:04X}")).collect()
}

fn sign(o: Ordering) -> i64 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

#[test]
fn measured_columns_use_this_collation() {
    let f = fixture();
    assert_eq!(f["collation"], collation::NAME);
    assert_eq!(f["pad_attribute"], "PAD SPACE");
    let keyed: Vec<&Value> = f["columns"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["keyed_by_ace"] == true)
        .collect();
    assert_eq!(
        keyed.len(),
        7,
        "character.name, quest_Name, four config keys, accountName"
    );
    assert!(keyed.iter().all(|c| c["collation"] == collation::NAME));
}

#[test]
fn every_code_point_weighs_as_in_mariadb() {
    let f: Value =
        serde_json::from_str(include_str!("../../fixtures/uca1400_all_weights.json")).unwrap();
    let mut h = Sha256::new();
    let mut n = 0u64;
    let mut buf = [0u8; 4];
    for c in (0..=0x10_FFFFu32).filter_map(char::from_u32) {
        let w = collation::weights(c.encode_utf8(&mut buf));
        h.update(u32::from(c).to_le_bytes());
        h.update([u8::try_from(w.len()).unwrap()]);
        for x in w {
            h.update(x.to_le_bytes());
        }
        n += 1;
    }
    assert_eq!(n, f["code_points"].as_u64().unwrap());
    let digest: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(digest, f["sha256"].as_str().unwrap());
}

#[test]
fn corpus_weight_strings_match_mariadb() {
    let f = fixture();
    let strings = f["strings"].as_array().unwrap();
    let mut bad = Vec::new();
    for (s, w) in strings.iter().zip(f["weights"].as_array().unwrap()) {
        let s = s.as_str().unwrap();
        if hex_weights(&collation::weights(s)) != w.as_str().unwrap() {
            bad.push(s.to_owned());
        }
    }
    assert!(
        bad.is_empty(),
        "{} of {} weight strings differ: {:?}",
        bad.len(),
        strings.len(),
        &bad[..bad.len().min(10)]
    );
}

#[test]
fn corpus_pairs_match_mariadb_in_rust_and_sqlite() {
    let f = fixture();
    let strings: Vec<&str> = f["strings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let shard = SqliteShard::open_in_memory().unwrap();
    let mut stmt = shard
        .connection()
        .prepare(&format!("SELECT ?1 = ?2 COLLATE {0}, CASE WHEN ?1 < ?2 COLLATE {0} THEN -1 WHEN ?1 > ?2 COLLATE {0} THEN 1 ELSE 0 END", collation::NAME))
        .unwrap();
    let pairs = f["pairs"].as_array().unwrap();
    let mut agree = 0;
    let mut bad = Vec::new();
    for p in pairs {
        let p: Vec<i64> = p
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        let (a, b) = (
            strings[usize::try_from(p[0]).unwrap()],
            strings[usize::try_from(p[1]).unwrap()],
        );
        let (eq, cmp) = (p[2] == 1, p[3]);
        let ours = (
            collation::eq(a, b),
            sign(collation::compare(a, b)),
            CollationKey::new(a) == CollationKey::new(b),
        );
        let sqlite: (bool, i64) = stmt
            .query_row([a, b], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        if ours == (eq, cmp, eq) && sqlite == (eq, cmp) {
            agree += 1;
        } else {
            bad.push(format!(
                "{a:?} vs {b:?}: mariadb ({eq}, {cmp}) ours {ours:?} sqlite {sqlite:?}"
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{agree}/{} agree; first: {:?}",
        pairs.len(),
        &bad[..bad.len().min(10)]
    );
    assert!(pairs.len() > 10_000);
}

#[test]
fn corpus_orders_match_mariadb_in_rust_and_sqlite() {
    let f = fixture();
    let strings: Vec<&str> = f["strings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let shard = SqliteShard::open_in_memory().unwrap();
    let mut stmt = shard
        .connection()
        .prepare(&format!("SELECT CAST(value AS INTEGER) FROM json_each(?1) ORDER BY ?2 ->> ('$[' || value || ']') COLLATE {}, CAST(value AS INTEGER)", collation::NAME))
        .unwrap();
    let all = serde_json::to_string(&strings).unwrap();
    for (name, expected) in f["orders"].as_object().unwrap() {
        let expected: Vec<usize> = expected
            .as_array()
            .unwrap()
            .iter()
            .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        let mut ours = expected.clone();
        ours.sort_unstable();
        ours.sort_by(|&a, &b| collation::compare(strings[a], strings[b]).then(a.cmp(&b)));
        assert_eq!(ours, expected, "{name}: Rust order");

        let mut ids = expected.clone();
        ids.sort_unstable();
        let sqlite: Vec<usize> = stmt
            .query_map([serde_json::to_string(&ids).unwrap(), all.clone()], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
            .map(|v| usize::try_from(v.unwrap()).unwrap())
            .collect();
        assert_eq!(sqlite, expected, "{name}: SQLite ORDER BY");
    }
}

fn character(id: u32, name: &str) -> Character {
    Character {
        id,
        account_id: 1,
        name: name.into(),
        spellbook_filters: 16383,
        ..Default::default()
    }
}

/// Variants MariaDB treats as the same name as "Aluvian" (case, accents, trailing spaces,
/// ignorable soft hyphen) and ones it does not.
const SAME: &[&str] = &[
    "Aluvian",
    "ALUVIAN",
    "aluvian",
    "Àlüvîàn",
    "ÁLÜVÍÀN",
    "Aluvian ",
    "Aluvian   ",
    "Alu\u{ad}vian",
];
const DIFFERENT: &[&str] = &[
    "Aluvia",
    "Aluviann",
    "Aluvian\t",
    "Aluvian.",
    " Aluvian",
    "Aluv ian",
    "Aluvián!",
];

#[test]
fn character_names_match_like_ace() {
    for (backend, mut db) in backends() {
        let db = db.as_mut();
        assert!(
            db.save_character(&character(0x5000_0001, "Aluvian")),
            "{backend}"
        );
        assert!(
            db.save_character(&character(0x5000_0002, "Straße")),
            "{backend}"
        );
        for &v in SAME {
            assert!(
                !db.is_character_name_available(v),
                "{backend}: {v:?} is taken"
            );
            assert_eq!(
                db.get_character_stub_by_name(v).map(|c| c.id),
                Some(0x5000_0001),
                "{backend}: {v:?}"
            );
        }
        for &v in DIFFERENT {
            assert!(
                db.is_character_name_available(v),
                "{backend}: {v:?} is free"
            );
            assert!(
                db.get_character_stub_by_name(v).is_none(),
                "{backend}: {v:?}"
            );
        }
        // ß weighs as "ss"; þ is its own letter.
        assert!(!db.is_character_name_available("STRASSE"), "{backend}");
        assert_eq!(
            db.get_character_stub_by_name("strasse").map(|c| c.id),
            Some(0x5000_0002),
            "{backend}"
        );
        assert!(
            db.save_character(&character(0x5000_0003, "Þorn")),
            "{backend}"
        );
        assert!(db.is_character_name_available("Thorn"), "{backend}");
        assert!(!db.is_character_name_available("þORN"), "{backend}");
    }
}

#[test]
fn quest_names_are_unique_and_ordered_by_the_collation() {
    for (backend, mut db) in backends() {
        let db = db.as_mut();
        let mut c = character(0x5000_0001, "Questor");
        for q in ["b", "Ämulet", "a", "Zed", "élan"] {
            c.get_or_create_quest(q);
        }
        assert!(db.save_character(&c), "{backend}");
        let got = db.get_character(0x5000_0001).unwrap();
        let names: Vec<&str> = got
            .character_properties_quest_registry
            .iter()
            .map(|q| q.quest_name.as_str())
            .collect();
        assert_eq!(
            names,
            ["a", "Ämulet", "b", "élan", "Zed"],
            "{backend}: primary-key order"
        );

        // ACE's in-memory lookups are ordinal ignore-case, so "Amulet" is a new quest there, but
        // the primary key (character_Id, quest_Name) rejects it as a duplicate of "Ämulet".
        let (_, created) = c.get_or_create_quest("Amulet");
        assert!(created, "{backend}");
        assert!(
            !db.save_character(&c),
            "{backend}: duplicate key under the collation"
        );

        // Trailing spaces are padding: "Zed " duplicates "Zed".
        let mut d = character(0x5000_0002, "Padded");
        d.get_or_create_quest("Zed");
        d.get_or_create_quest("zed ");
        assert!(!db.save_character(&d), "{backend}: PAD SPACE duplicate");
    }
}

#[test]
fn config_keys_match_like_ace() {
    let dbs: Vec<(&str, Box<dyn ShardConfigDatabase>)> = vec![
        ("mem", Box::new(empyrean_store::MemShard::new())),
        ("sqlite", Box::new(SqliteShard::open_in_memory().unwrap())),
    ];
    for (backend, mut db) in dbs {
        db.add_long("max_chars", 11, None);
        db.add_long("Zeta", 1, None);
        db.add_long("éclair", 2, None);
        assert!(db.long_exists("MAX_CHARS "), "{backend}");
        assert!(db.long_exists("máx_chárs"), "{backend}");
        assert!(!db.long_exists("max_chars\t"), "{backend}");
        assert_eq!(db.get_long("ECLAIR").map(|r| r.value), Some(2), "{backend}");
        let keys: Vec<String> = db.get_all_longs().into_iter().map(|r| r.key).collect();
        assert_eq!(
            keys,
            ["éclair", "max_chars", "Zeta"],
            "{backend}: key order"
        );
    }
}

#[test]
fn account_names_match_like_ace() {
    let clock = Arc::new(VirtualClock::new(DotNetDateTime::new_hms(
        2026, 3, 4, 5, 6, 7,
    )));
    let cfg = AccountDefaults {
        password_hash_work_factor: 4,
        ..AccountDefaults::default()
    };
    let dbs: Vec<(&str, Box<dyn AuthDatabase>)> = vec![
        ("mem", Box::new(MemAuth::new(cfg.clone(), clock.clone()))),
        (
            "sqlite",
            Box::new(SqliteAuth::open_in_memory(cfg, clock).unwrap()),
        ),
    ];
    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    for (backend, mut db) in dbs {
        let a = db
            .create_account("Aluvian", "pw", AccessLevel::Player, local)
            .unwrap();
        for &v in SAME {
            assert_eq!(
                db.get_account_by_name(v).map(|x| x.account_id),
                Some(a.account_id),
                "{backend}: {v:?}"
            );
            assert_eq!(
                db.get_account_id_by_name(v),
                a.account_id,
                "{backend}: {v:?}"
            );
            assert!(
                db.create_account(v, "pw", AccessLevel::Player, local)
                    .is_err(),
                "{backend}: {v:?} is a duplicate"
            );
        }
        for &v in DIFFERENT {
            assert!(db.get_account_by_name(v).is_none(), "{backend}: {v:?}");
        }
        assert!(
            db.create_account("Aluvian\t", "pw", AccessLevel::Player, local)
                .is_ok(),
            "{backend}"
        );
    }
}
