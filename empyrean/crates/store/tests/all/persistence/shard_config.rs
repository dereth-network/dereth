//! ACE: Source/ACE.Database/ShardConfigDatabase.cs::ShardConfigDatabase
//! ShardConfigDatabase add/get/save/list on both backends; existing-key add and missing-key save
//! throw.
//! Fixture: synthetic account and shard records on the memory and SQLite backends.

use empyrean_store::models::shard::ConfigPropertiesLong;
use empyrean_store::{MemShard, ShardConfigDatabase, SqliteShard};

fn backends() -> Vec<(&'static str, Box<dyn ShardConfigDatabase>)> {
    vec![
        ("mem", Box::new(MemShard::new())),
        ("sqlite", Box::new(SqliteShard::open_in_memory().unwrap())),
    ]
}

#[test]
fn add_get_save_and_list() {
    for (name, mut db) in backends() {
        assert!(!db.bool_exists("pk_server"), "{name}");
        db.add_bool("pk_server", true, Some("pk"));
        db.add_long("max_chars", 11, None);
        db.add_long("Alpha", 1, None);
        db.add_double("xp_modifier", 1.5, None);
        db.add_string("motd", "hello", Some("m"));

        // Keys compare case-insensitively, as MySQL's collation does.
        assert!(db.bool_exists("PK_SERVER"), "{name}");
        assert!(!db.double_exists("pk_server"), "{name}");
        assert_eq!(
            db.get_bool("pk_server").map(|r| r.value),
            Some(true),
            "{name}"
        );
        assert_eq!(
            db.get_string("MOTD")
                .map(|r| (r.key, r.value, r.description)),
            Some(("motd".into(), "hello".into(), Some("m".into()))),
            "{name}"
        );
        assert!(db.get_long("nope").is_none(), "{name}");

        db.save_long(&ConfigPropertiesLong {
            key: "max_chars".into(),
            value: 12,
            description: Some("d".into()),
        });
        assert_eq!(
            db.get_long("max_chars").map(|r| (r.value, r.description)),
            Some((12, Some("d".into()))),
            "{name}"
        );

        // Key order.
        assert_eq!(
            db.get_all_longs()
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "max_chars"],
            "{name}"
        );
        assert_eq!(db.get_all_doubles().len(), 1, "{name}");
        assert_eq!(db.get_all_bools().len(), 1, "{name}");
        assert_eq!(db.get_all_strings().len(), 1, "{name}");
        assert!(
            db.long_exists("ALPHA") && db.string_exists("motd"),
            "{name}"
        );
    }
}

#[test]
fn adding_an_existing_key_or_saving_a_missing_one_throws() {
    for (name, mut db) in backends() {
        db.add_long("k", 1, None);
        let dup =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| db.add_long("K", 2, None)));
        assert!(dup.is_err(), "{name}");
        let missing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            db.save_long(&ConfigPropertiesLong {
                key: "missing".into(),
                value: 1,
                description: None,
            });
        }));
        assert!(missing.is_err(), "{name}");
    }
}
