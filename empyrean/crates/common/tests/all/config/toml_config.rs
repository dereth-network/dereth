//! Divergence: V373
//! Empyrean.toml reader/writer/converter: defaults round-trip, every kept key parses and
//! survives, schema names each once, unknown/removed keys warned, renamed keys read under their
//! old names for one release with a warning, sections replace defaults, bad files error, writer
//! comments defaults, converter lists drops.
//! Fixture: tests/fixtures/Config.js.example and locally constructed edge cases.

use empyrean_common::config_manager::{ConfigError, ConfigManager};
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::preloaded_landblock::PreloadedLandblocks;
use empyrean_common::toml_config::{
    self, Deprecated, Ignored, Renamed, REMOVED, RENAMED, SECTIONS,
};

const ACE_EXAMPLE: &str = include_str!("../../fixtures/Config.js.example");

fn json(text: &str) -> MasterConfiguration {
    ConfigManager::deserialize(text).expect("valid Config.js")
}

fn toml(text: &str) -> toml_config::Parsed {
    toml_config::from_toml_str(text).expect("valid empyrean.toml")
}

/// What a configuration becomes after a trip through empyrean.toml.
fn via_toml(config: &MasterConfiguration) -> MasterConfiguration {
    let parsed = toml(&toml_config::to_toml_string(config));
    assert_eq!(parsed.unknown_keys, Vec::<String>::new());
    assert_eq!(parsed.ignored, Vec::<Ignored>::new());
    parsed.config
}

fn keys_of(parsed: &toml_config::Parsed) -> Vec<&str> {
    parsed.ignored.iter().map(|i| i.key.as_str()).collect()
}

#[test]
fn defaults_round_trip_and_an_empty_file_is_the_defaults() {
    let defaults = MasterConfiguration::default();
    assert_eq!(via_toml(&defaults), defaults);
    assert_eq!(toml("").config, defaults);
    assert_eq!(toml("").config, json("{}"));
}

/// Every kept key, set in a file, reaches its setting.
#[test]
fn every_kept_key_parses() {
    let parsed = toml(
        r#"
        [server]
        world_name = "Kept World"
        dat_files_directory = 'D:\ac\'
        world_pack_path = "/srv/world.pack"
        world_overlay_path = "overlay.sqlite"
        world_base_sql = "dump.sql"
        world_base_patches = ["p1", "json:p2"]
        shutdown_interval = 5
        server_performance_monitor_auto_start = true
        shard_player_biota_cache_time = 1
        shard_non_player_biota_cache_time = 2
        landblock_preloading = false
        log_level = "debug"
        status_address = "127.0.0.1:9100"
        interactive_console = false
        [server.network]
        host = "127.0.0.1"
        port = 9100
        maximum_allowed_sessions = 3
        default_session_timeout = 7
        maximum_allowed_sessions_per_ip_address = 2
        allow_unlimited_sessions_from_ip_addresses = ["10.0.0.1"]
        [server.accounts]
        override_character_permissions = false
        default_access_level = 5
        allow_auto_account_creation = false
        password_hash_work_factor = 10
        force_work_factor_migration = false
        [[server.preloaded_landblocks]]
        id = "01020304"
        description = "one"
        permaload = false
        include_adjacents = true
        enabled = true
        [database]
        shard_db_path = "s.db"
        auth_db_path = "a.db"
        [offline]
        purge_deleted_characters = true
        purge_deleted_characters_days = 4
        purge_orphaned_biotas = true
        prune_deleted_characters_from_friend_lists = false
        prune_deleted_objects_from_shortcut_bars = true
        prune_deleted_characters_from_squelch_lists = true
        [ddd]
        enable_dat_patching = true
        precache_compressed_dat_files = true
        "#,
    );
    assert!(
        parsed.unknown_keys.is_empty() && parsed.ignored.is_empty(),
        "{parsed:?}"
    );
    let c = parsed.config;
    let s = &c.server;
    assert_eq!(s.world_name, "Kept World");
    assert_eq!(s.dat_files_directory, "D:\\ac\\");
    assert_eq!(s.world_pack_path, "/srv/world.pack");
    assert_eq!(
        (s.world_overlay_path.as_str(), s.world_base_sql.as_str()),
        ("overlay.sqlite", "dump.sql")
    );
    assert_eq!(s.world_base_patches, ["p1", "json:p2"]);
    assert_eq!(
        (
            s.shutdown_interval,
            s.shard_player_biota_cache_time,
            s.shard_non_player_biota_cache_time
        ),
        (5, 1, 2)
    );
    assert!(s.server_performance_monitor_auto_start && !s.landblock_preloading);
    assert_eq!(
        (s.log_level.as_str(), s.status_address.as_str()),
        ("debug", "127.0.0.1:9100")
    );
    assert!(!s.interactive_console);
    let n = &s.network;
    assert_eq!(
        (
            n.host.as_str(),
            n.port,
            n.maximum_allowed_sessions,
            n.default_session_timeout
        ),
        ("127.0.0.1", 9100, 3, 7)
    );
    assert_eq!(n.maximum_allowed_sessions_per_ip_address, 2);
    assert_eq!(n.allow_unlimited_sessions_from_ip_addresses, ["10.0.0.1"]);
    let a = &s.accounts;
    assert!(
        !a.override_character_permissions
            && !a.allow_auto_account_creation
            && !a.force_work_factor_migration
    );
    assert_eq!(
        (a.default_access_level, a.password_hash_work_factor),
        (5, 10)
    );
    assert_eq!(
        s.preloaded_landblocks,
        [PreloadedLandblocks {
            id: Some("01020304".into()),
            description: Some("one".into()),
            permaload: false,
            include_adjacents: true,
            enabled: true
        }]
    );
    assert_eq!(
        (
            c.database.shard_db_path.as_str(),
            c.database.auth_db_path.as_str()
        ),
        ("s.db", "a.db")
    );
    let o = &c.offline;
    assert!(
        o.purge_deleted_characters
            && o.purge_orphaned_biotas
            && !o.prune_deleted_characters_from_friend_lists
    );
    assert!(
        o.prune_deleted_objects_from_shortcut_bars && o.prune_deleted_characters_from_squelch_lists
    );
    assert_eq!(o.purge_deleted_characters_days, 4);
    assert!(c.ddd.enable_dat_patching && c.ddd.precache_compressed_dat_files);
    // And the same configuration survives the writer.
    assert_eq!(via_toml(&c), c);
}

#[test]
fn every_setting_changed_survives_the_trip() {
    let mut c = MasterConfiguration::default();
    let s = &mut c.server;
    s.world_name = "Test \"World\" \\ é".to_owned();
    s.network.host = "127.0.0.1".to_owned();
    s.network.allow_unlimited_sessions_from_ip_addresses =
        vec!["10.0.0.1".into(), "10.0.0.2".into()];
    s.dat_files_directory = "/home/user/ac/".to_owned();
    s.preloaded_landblocks = vec![
        PreloadedLandblocks {
            id: Some("01020304".into()),
            description: None,
            permaload: false,
            include_adjacents: true,
            enabled: true,
        },
        PreloadedLandblocks {
            id: None,
            description: Some("no id".into()),
            ..Default::default()
        },
    ];
    s.log_level = "trace".into();
    s.status_address = "0.0.0.0:1".into();
    s.interactive_console = false;
    c.database.shard_db_path = "one.db".into();
    c.database.auth_db_path = "one.db".into();
    c.offline.purge_deleted_characters_days = -4;
    assert_eq!(via_toml(&c), c);
    assert_eq!(json(&ConfigManager::serialize(&c)), c);

    // An empty landblock list is written in [server] and stays empty (not the default six).
    c.server.preloaded_landblocks.clear();
    let text = toml_config::to_toml_string(&c);
    assert!(text.contains("\npreloaded_landblocks = []\n"));
    assert_eq!(via_toml(&c), c);
}

#[test]
fn the_schema_names_every_setting_once() {
    // Every key the configuration serialises is in the schema, and nothing else is.
    fn keys(value: &serde_json::Value, path: &mut Vec<String>, out: &mut Vec<String>) {
        if let Some(map) = value.as_object() {
            for (k, v) in map {
                path.push(k.clone());
                if v.is_object()
                    || v.as_array()
                        .is_some_and(|a| a.first().is_some_and(|e| e.is_object()))
                {
                    let inner = if v.is_object() { v } else { &v[0] };
                    keys(inner, path, out);
                } else {
                    out.push(path.join("."));
                }
                path.pop();
            }
        }
    }
    let value = serde_json::to_value(MasterConfiguration::default()).unwrap();
    let mut from_serde = Vec::new();
    keys(&value, &mut Vec::new(), &mut from_serde);
    let mut from_schema: Vec<String> = SECTIONS
        .iter()
        .flat_map(|s| {
            s.keys
                .iter()
                .map(move |k| format!("{}.{}", s.ace.join("."), k.ace))
        })
        .collect();
    from_serde.sort();
    from_schema.sort();
    assert_eq!(from_schema, from_serde);
    let mut toml_keys: Vec<String> = SECTIONS
        .iter()
        .flat_map(|s| {
            s.keys
                .iter()
                .map(move |k| format!("{}.{}", s.toml.join("."), k.toml))
        })
        .collect();
    let n = toml_keys.len();
    toml_keys.sort();
    toml_keys.dedup();
    assert_eq!(toml_keys.len(), n);
    // No removed setting is still in the schema.
    for r in REMOVED {
        assert!(
            !toml_keys
                .iter()
                .any(|k| k == r.toml || k.starts_with(&format!("{}.", r.toml))),
            "{}",
            r.toml
        );
    }
}

#[test]
fn unknown_keys_are_ignored_and_reported_in_file_order() {
    let parsed = toml(
        r#"
        colour = "blue"
        [server]
        world_name = "Kept"
        WorldName = "PascalCase is not a TOML key"
        wrold_name = "typo"
        [server.network]
        port = 9010
        extra = { a = 1 }
        [server.nope]
        x = 1
        [[server.preloaded_landblocks]]
        id = "A9B4FFFF"
        colour = 1
        [database.replica]
        host = "h"
        "#,
    );
    assert_eq!(parsed.config.server.world_name, "Kept");
    assert_eq!(parsed.config.server.network.port, 9010);
    assert_eq!(parsed.config.server.preloaded_landblocks.len(), 1);
    assert_eq!(
        parsed.unknown_keys,
        [
            "colour",
            "server.WorldName",
            "server.wrold_name",
            "server.network.extra",
            "server.nope",
            "server.preloaded_landblocks.colour",
            "database.replica",
        ]
    );
    assert!(parsed.ignored.is_empty());
}

/// A file written before the removal (every removed key and section, and `[mysql]`) loads: each
/// is reported once with its reason, and nothing it says is read.
#[test]
fn removed_keys_and_sections_warn_once_and_load() {
    let parsed = toml(
        r#"
        [server]
        world_name = "Old File"
        mods_directory = 'C:\Mods\'
        world_database_precaching = true
        [server.threading]
        world_thread_count_multiplier = 0.5
        database_thread_count_multiplier = 0.25
        multi_threaded_landblock_group_physics_ticking = true
        multi_threaded_landblock_group_ticking = true
        [mysql]
        shard_db_path = "old-shard.db"
        auth_db_path = "old-auth.db"
        [mysql.authentication]
        host = "db"
        [mysql.shard]
        database = "s"
        [mysql.world]
        port = 1
        [offline]
        purge_orphaned_biotas = true
        auto_update_world_database = true
        auto_server_update_check = true
        auto_apply_world_customizations = true
        world_customization_added_paths = ["x"]
        recurse_world_customization_paths = true
        auto_apply_database_updates = true
        "#,
    );
    assert!(parsed.unknown_keys.is_empty(), "{:?}", parsed.unknown_keys);
    assert_eq!(
        keys_of(&parsed),
        [
            "server.mods_directory",
            "server.world_database_precaching",
            "server.threading",
            "mysql.shard_db_path",
            "mysql.auth_db_path",
            "mysql.authentication",
            "mysql.shard",
            "mysql.world",
            "offline.auto_update_world_database",
            "offline.auto_server_update_check",
            "offline.auto_apply_world_customizations",
            "offline.world_customization_added_paths",
            "offline.recurse_world_customization_paths",
            "offline.auto_apply_database_updates",
        ]
    );
    // Every removed setting is covered, each with its reason.
    for r in REMOVED {
        let hit = parsed
            .ignored
            .iter()
            .find(|i| i.key == r.toml)
            .unwrap_or_else(|| panic!("{} not reported", r.toml));
        assert_eq!(hit.why, r.why);
    }
    for key in ["mysql.shard_db_path", "mysql.auth_db_path"] {
        let hit = parsed.ignored.iter().find(|i| i.key == key).unwrap();
        assert!(hit.why.contains("[database]"), "{}", hit.why);
    }
    // What is kept is read; what is removed or renamed is not.
    let mut expected = MasterConfiguration::default();
    expected.server.world_name = "Old File".into();
    expected.offline.purge_orphaned_biotas = true;
    assert_eq!(parsed.config, expected);
    // An empty [mysql] is reported too.
    assert_eq!(keys_of(&toml("[mysql]\n")), ["mysql"]);
}

#[test]
fn sections_replace_their_defaults() {
    // One landblock entry replaces the default list.
    let t = toml("[[server.preloaded_landblocks]]\nid = \"A9B4FFFF\"\n").config;
    assert_eq!(
        t,
        json(r#"{"Server": {"PreloadedLandblocks": [{"Id": "A9B4FFFF"}]}}"#)
    );
    assert_eq!(t.server.preloaded_landblocks.len(), 1);
    // A section present takes the class defaults for what it leaves out.
    let t = toml("[database]\nshard_db_path = \"x.db\"\n").config;
    assert_eq!(
        (
            t.database.shard_db_path.as_str(),
            t.database.auth_db_path.as_str()
        ),
        ("x.db", "./auth.db")
    );
    // Numbers may be strings, as in Config.js ("ShutdownInterval": "60").
    let t = toml("[server]\nshutdown_interval = \"45\"\n").config;
    assert_eq!(t.server.shutdown_interval, 45);
}

#[test]
fn bad_files_are_errors() {
    for bad in [
        "[server",
        "[server]\nworld_name = 5\n",
        "[server.network]\nport = -1\n",
        "[server.network]\nport = 1.5\n",
        "[server]\nlandblock_preloading = \"true\"\n",
        "[server]\ninteractive_console = \"no\"\n",
        "[server]\nlog_level = 3\n",
        "server = 3\n",
        "[server]\nnetwork = 3\n",
    ] {
        match toml_config::from_toml_str(bad) {
            Err(e @ ConfigError::Toml(_)) => assert!(!e.to_string().is_empty()),
            other => panic!("{bad:?} should fail, got {other:?}"),
        }
    }
}

#[test]
fn the_writer_comments_every_key_with_its_default_and_origin() {
    let text = toml_config::to_toml_string(&MasterConfiguration::default());
    assert!(text.starts_with(toml_config::HEADER));
    for s in SECTIONS.iter().filter(|s| !s.array) {
        for k in s.keys {
            let origin = if k.in_ace {
                format!("(ACE Config.js: {}.{})", s.ace.join("."), k.ace)
            } else {
                "(Empyrean only)".to_owned()
            };
            assert!(
                text.contains("# default: ") && text.contains(&origin),
                "{origin}"
            );
        }
    }
    assert!(text.contains("# default: \"info\" (Empyrean only)\n"));
    assert!(text.contains("# default: \"Empyrean\" (ACE Config.js: Server.WorldName)\n"));
    assert!(text.contains("\n[database]\n") && !text.contains("[mysql"));
    assert!(text.contains("[[server.preloaded_landblocks]], default: 6 entries"));
    for r in REMOVED {
        let key = r.toml.rsplit('.').next().unwrap();
        assert!(
            !text.contains(&format!("\n{key} =")) && !text.contains(&format!("[{}]", r.toml)),
            "{}",
            r.toml
        );
    }
    // A UTF-8 byte order mark is dropped, as for Config.js.
    assert_eq!(toml(&format!("\u{feff}{text}")).config, toml(&text).config);
}

/// The converter: ACE's own `Config.js.example` becomes a configuration whose written
/// `empyrean.toml` loads back to it, and every key Empyrean does not have is listed as dropped.
#[test]
fn the_converter_reads_aces_example_and_lists_what_it_drops() {
    let converted = toml_config::from_config_js_str(ACE_EXAMPLE).expect("ACE's example converts");
    assert_eq!(converted.config, json(ACE_EXAMPLE));
    assert_eq!(converted.config.server.world_name, "ACEmulator");
    assert_eq!(converted.config.server.network.port, 9000);
    let dropped: Vec<&str> = converted.dropped.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(
        dropped,
        // (sorted by key: a JSON object's keys are not kept in file order)
        [
            "MySql.Authentication",
            "MySql.Shard",
            "MySql.World",
            "Offline.AutoApplyDatabaseUpdates",
            "Offline.AutoApplyWorldCustomizations",
            "Offline.AutoServerUpdateCheck",
            "Offline.AutoUpdateWorldDatabase",
            "Offline.RecurseWorldCustomizationPaths",
            "Offline.WorldCustomizationAddedPaths",
            "Server.ModsDirectory",
            "Server.Threading",
            "Server.WorldDatabasePrecaching",
        ]
    );
    for d in &converted.dropped {
        let r = REMOVED
            .iter()
            .find(|r| r.ace == d.key)
            .expect("a removed setting");
        assert_eq!(d.why, r.why);
    }

    let note = toml_config::conversion_note("Config.js", &converted.dropped);
    let text = toml_config::to_toml_string_with_note(&converted.config, Some(&note));
    let back = toml(&text);
    assert!(back.unknown_keys.is_empty() && back.ignored.is_empty());
    assert_eq!(back.config, converted.config);
    assert!(text.contains("#   MySql.Authentication: "));

    // Keys neither ACE nor Empyrean has are dropped too, named as such; nothing is dropped twice.
    let c = toml_config::from_config_js_str(
        r#"{"Server": {"Colour": 1, "PreloadedLandblocks": [{"Id": "1", "X": 1}, {"X": 2}]}, "Extra": {}}"#,
    )
    .unwrap();
    let keys: Vec<&str> = c.dropped.iter().map(|d| d.key.as_str()).collect();
    assert_eq!(
        keys,
        ["Extra", "Server.Colour", "Server.PreloadedLandblocks.X"]
    );
    assert!(c.dropped.iter().all(|d| d.why.contains("not a setting")));
    let clean = toml_config::conversion_note("x.js", &[]);
    assert!(clean.contains("Every key it had is kept"));
    // Not JSON, or a value that does not fit: errors.
    assert!(matches!(
        toml_config::from_config_js_str("{"),
        Err(ConfigError::Json(_))
    ));
    assert!(matches!(
        toml_config::from_config_js_str(r#"{"Server": {"Network": {"Port": "nine"}}}"#),
        Err(ConfigError::Json(_))
    ));
}

/// A renamed key keeps working under its old name for the release that renamed it: its value is
/// read as the new key's (across sections too), with a warning naming both; when the file also has
/// the new key, the new one wins and the old one is reported as not read.
#[test]
fn a_renamed_key_is_read_under_its_old_name_with_a_warning() {
    const RENAMES: &[Renamed] = &[
        Renamed {
            old: "server.realm_name",
            new: "server.world_name",
            since: "0.1.0",
        },
        Renamed {
            old: "server.shard_file",
            new: "database.shard_db_path",
            since: "0.1.0",
        },
        Renamed {
            old: "server.gone_nowhere",
            new: "server.not_a_setting",
            since: "0.1.0",
        },
    ];
    let parsed = toml_config::from_toml_str_with_renames(
        "[server]\nrealm_name = \"Renamed\"\nshard_file = \"old.db\"\ngone_nowhere = 1\n\
         [database]\nauth_db_path = \"a.db\"\n",
        RENAMES,
    )
    .unwrap();
    assert_eq!(parsed.config.server.world_name, "Renamed");
    assert_eq!(parsed.config.database.shard_db_path, "old.db");
    assert_eq!(parsed.config.database.auth_db_path, "a.db");
    assert_eq!(
        parsed.deprecated,
        [
            Deprecated {
                renamed: RENAMES[0],
                read: true
            },
            Deprecated {
                renamed: RENAMES[1],
                read: true
            },
        ]
    );
    // A rename to no setting leaves the old key unknown.
    assert_eq!(parsed.unknown_keys, ["server.gone_nowhere"]);
    let lines = toml_config::warnings(&parsed, std::path::Path::new("empyrean.toml"));
    assert!(
        lines.iter().any(|l| l.contains("`server.realm_name`")
            && l.contains("is now `server.world_name`")
            && l.contains("renamed in 0.1.0")
            && l.contains("this release only")),
        "{lines:?}"
    );

    // Both names: the new key wins, the old one is not read.
    let both = toml_config::from_toml_str_with_renames(
        "[server]\nrealm_name = \"Old\"\nworld_name = \"New\"\n",
        RENAMES,
    )
    .unwrap();
    assert_eq!(both.config.server.world_name, "New");
    assert_eq!(
        both.deprecated,
        [Deprecated {
            renamed: RENAMES[0],
            read: false
        }]
    );
    let lines = toml_config::warnings(&both, std::path::Path::new("empyrean.toml"));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("`server.realm_name`") && l.contains("remove the old key")),
        "{lines:?}"
    );

    // Without the rename, the old name is an unknown key.
    let plain = toml("[server]\nrealm_name = \"Renamed\"\n");
    assert_eq!(plain.unknown_keys, ["server.realm_name"]);
    assert!(plain.deprecated.is_empty());
}

/// Every rename this build carries names a setting, and belongs to this release: the next release
/// removes it, so an old name works for one release only.
#[test]
fn every_rename_names_a_setting_and_lasts_one_release() {
    for r in RENAMED {
        assert_eq!(
            r.since,
            env!("CARGO_PKG_VERSION"),
            "`{}` was renamed in an earlier release: remove the entry",
            r.old
        );
        let (section, key) = r.new.rsplit_once('.').unwrap_or(("", r.new));
        let text = format!("[{section}]\n{key} = {}\n", "0");
        let parsed = toml_config::from_toml_str(&text);
        assert!(
            parsed.map_or(true, |p| p.unknown_keys.is_empty()),
            "`{}` names no setting",
            r.new
        );
    }
}

/// Unknown keys are warnings, one line each naming the key and the file.
#[test]
fn every_unknown_key_gets_a_warning_line_naming_it() {
    let parsed = toml("colour = \"blue\"\n[server]\nwrold_name = \"typo\"\n");
    let lines = toml_config::warnings(&parsed, std::path::Path::new("conf/empyrean.toml"));
    let file = std::path::Path::new("conf/empyrean.toml")
        .display()
        .to_string();
    assert_eq!(
        lines,
        [
            format!("Configuration: unknown key `colour` in {file} is ignored"),
            format!("Configuration: unknown key `server.wrold_name` in {file} is ignored"),
        ]
    );
}

/// Divergence: V418
/// `[era]` takes a key per system over the profile's table; a key left out is written commented
/// out, and the settings survive the trip.
#[test]
fn the_era_section_turns_systems_on_and_off() {
    let parsed = toml("[era]\nprofile = \"infiltration\"\naetheria = true\ntrade = false\n");
    assert_eq!(parsed.unknown_keys, Vec::<String>::new());
    let era = &parsed.config.era;
    assert_eq!(era.features.get("aetheria"), Some(true));
    assert_eq!(era.features.get("trade"), Some(false));
    assert_eq!(era.features.get("chess"), None);
    let rules = era.rules();
    assert!(rules.features.aetheria && !rules.features.trade && rules.features.chess);
    assert_eq!(via_toml(&parsed.config), parsed.config);
    let text = toml_config::to_toml_string(&MasterConfiguration::default());
    assert!(text.contains("\n# spell_research = false\n"), "{text}");
    assert!(text.contains("\n# trade = true\n"), "{text}");
    assert!(toml_config::from_toml_str("[era]\nchess = \"yes\"\n").is_err());
}
