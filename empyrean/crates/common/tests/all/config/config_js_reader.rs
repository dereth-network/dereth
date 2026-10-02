//! Vectors: tests/fixtures/Config.js.example and local configuration parsing cases
//! ACE's Config.js.example loads with ACE key names and class defaults; comments/trailing commas,
//! string numbers, case-sensitive names, section replacement, removed settings ignored, round-
//! trip, path resolution, per-thread override, source URL.
//! Fixture: tests/fixtures/Config.js.example and locally constructed edge cases.

use std::path::{Path, PathBuf};

use empyrean_common::config_manager::{ConfigError, ConfigManager};
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_common::preloaded_landblock::PreloadedLandblocks;

const EXAMPLE: &str = include_str!("../../fixtures/Config.js.example");

fn parse(text: &str) -> MasterConfiguration {
    ConfigManager::deserialize(text).expect("valid configuration")
}

#[test]
fn ace_example_config_loads_with_ace_key_names() {
    let c = parse(EXAMPLE);
    let s = &c.server;
    assert_eq!(s.world_name, "ACEmulator");
    assert_eq!((s.network.host.as_str(), s.network.port), ("0.0.0.0", 9000));
    assert_eq!(
        (
            s.network.maximum_allowed_sessions,
            s.network.default_session_timeout
        ),
        (128, 60)
    );
    assert_eq!(s.network.maximum_allowed_sessions_per_ip_address, -1);
    assert!(s
        .network
        .allow_unlimited_sessions_from_ip_addresses
        .is_empty());
    assert!(
        s.accounts.override_character_permissions
            && s.accounts.allow_auto_account_creation
            && s.accounts.force_work_factor_migration
    );
    assert_eq!(
        (
            s.accounts.default_access_level,
            s.accounts.password_hash_work_factor
        ),
        (0, 8)
    );
    assert_eq!(s.dat_files_directory, "c:\\ACE\\Dats\\");
    // "ShutdownInterval": "60" and the cache times are strings in the example.
    assert_eq!(
        (
            s.shutdown_interval,
            s.shard_player_biota_cache_time,
            s.shard_non_player_biota_cache_time
        ),
        (60, 31, 11)
    );
    assert!(!s.server_performance_monitor_auto_start && s.landblock_preloading);
    assert_eq!(s.preloaded_landblocks.len(), 6);
    assert_eq!(
        s.preloaded_landblocks[0],
        PreloadedLandblocks {
            id: Some("E74EFFFF".into()),
            description: Some("Hebian-To (Global Events)".into()),
            permaload: true,
            include_adjacents: false,
            enabled: true
        }
    );
    assert_eq!(s.preloaded_landblocks[1].id.as_deref(), Some("A9B4FFFF"));
    // ACE's MySQL connections are not part of the configuration: the database files keep their
    // defaults.
    assert_eq!(
        (
            c.database.shard_db_path.as_str(),
            c.database.auth_db_path.as_str()
        ),
        ("./shard.db", "./auth.db")
    );
    let o = &c.offline;
    assert!(
        !o.purge_deleted_characters
            && o.purge_deleted_characters_days == 30
            && !o.purge_orphaned_biotas
    );
    assert!(
        o.prune_deleted_characters_from_friend_lists && !o.prune_deleted_objects_from_shortcut_bars
    );
    assert!(!c.ddd.enable_dat_patching && !c.ddd.precache_compressed_dat_files);
}

#[test]
fn class_defaults_match_ace() {
    let c = parse("{}");
    assert_eq!(c, MasterConfiguration::default());
    assert_eq!(
        c.server.world_name, "Empyrean",
        "ours (brand); ACE's class default is ACEmulator"
    );
    assert_eq!(
        c.server
            .preloaded_landblocks
            .iter()
            .filter(|p| p.enabled)
            .count(),
        1
    );
    assert!(c.offline.prune_deleted_characters_from_friend_lists);
    // Empyrean's own keys.
    assert_eq!(
        (
            c.server.log_level.as_str(),
            c.server.status_address.as_str()
        ),
        ("info", "")
    );
    assert!(c.server.interactive_console);
}

#[test]
fn comments_and_trailing_commas_are_accepted_outside_strings() {
    let c = parse(
        r#"/* header */ {
            "Server": {
                // a line comment
                "WorldName": "a // not a comment /* nor this */ \" still text",
                "Network": { "Port": 9100, /* inline */ },
                "PreloadedLandblocks": [ { "Id": "0001FFFF", }, ],
            },
        }"#,
    );
    assert_eq!(
        c.server.world_name,
        "a // not a comment /* nor this */ \" still text"
    );
    assert_eq!(c.server.network.port, 9100);
    assert_eq!(
        c.server.preloaded_landblocks.len(),
        1,
        "a list in the file replaces the default list"
    );
    assert_eq!(c.server.preloaded_landblocks[0].description, None);
    assert!(
        ConfigManager::deserialize(r#"{"Server": {"WorldName": "x",,}}"#).is_err(),
        "only one trailing comma"
    );
    assert_eq!(
        parse("\u{feff}{}"),
        MasterConfiguration::default(),
        "a byte order mark is skipped"
    );
}

#[test]
fn numbers_may_be_strings_but_must_fit() {
    let c = parse(
        r#"{"Server": {"Network": {"Port": "9005", "MaximumAllowedSessionsPerIPAddress": "-3"}}}"#,
    );
    assert_eq!(
        (
            c.server.network.port,
            c.server.network.maximum_allowed_sessions_per_ip_address
        ),
        (9005, -3)
    );
    for bad in [
        r#"{"Server": {"Network": {"Port": -1}}}"#,
        r#"{"Server": {"Network": {"Port": 1.5}}}"#,
        r#"{"Server": {"Network": {"Port": "nine"}}}"#,
        r#"{"Server": {"Network": {"Port": 4294967296}}}"#,
        r#"{"Server": {"LandblockPreloading": "true"}}"#,
        r#"{"Server": {"WorldName": 5}}"#,
    ] {
        match ConfigManager::deserialize(bad) {
            Err(ConfigError::Json(_)) => {}
            other => panic!("{bad} should fail to parse, got {other:?}"),
        }
    }
}

#[test]
fn property_names_are_case_sensitive_and_unknown_ones_ignored() {
    let c = parse(
        r#"{"Server": {"worldname": "lower", "Extra": [1, {"a": 2}], "WorldName": "Upper"}, "Unknown": null}"#,
    );
    assert_eq!(c.server.world_name, "Upper");
}

#[test]
fn a_present_section_replaces_the_default_object() {
    // System.Text.Json builds a fresh object for a section it finds, so keys the section omits take
    // the class defaults, not the initialisers; an absent section keeps its initialisers.
    let c = parse(r#"{"Offline": {"PurgeOrphanedBiotas": true}}"#);
    assert!(c.offline.purge_orphaned_biotas);
    assert_eq!(
        c.offline.purge_deleted_characters_days, 30,
        "the class default"
    );
}

#[test]
fn aces_removed_settings_are_ignored() {
    let c = parse(
        r#"{"Server": {"ModsDirectory": "m", "WorldDatabasePrecaching": true, "Threading": {"WorldThreadCountMultiplier": "NaN"}},
            "MySql": {"Shard": {"Database": "s"}, "ShardDbPath": "kept.db"},
            "Offline": {"AutoServerUpdateCheck": true, "AutoApplyDatabaseUpdates": 5}}"#,
    );
    let mut expected = MasterConfiguration::default();
    expected.database.shard_db_path = "kept.db".into();
    assert_eq!(c, expected);
}

#[test]
fn serialisation_round_trips_with_ace_key_names() {
    let c = parse(EXAMPLE);
    let text = ConfigManager::serialize(&c);
    assert!(
        text.contains("\"MaximumAllowedSessionsPerIPAddress\": -1"),
        "{text}"
    );
    assert!(text.contains("\"EnableDATPatching\": false"));
    assert!(text.contains("\"ShutdownInterval\": 60"));
    for gone in [
        "Threading",
        "ModsDirectory",
        "WorldDatabasePrecaching",
        "Authentication",
        "AutoServerUpdateCheck",
    ] {
        assert!(!text.contains(gone), "{gone} is not a setting");
    }
    assert!(
        text.starts_with("{\n  \"Server\": {\n    \"WorldName\": \"ACEmulator\","),
        "indented, declaration order"
    );
    assert_eq!(parse(&text), c);
}

#[test]
fn config_path_resolution_follows_ace() {
    let cwd = Path::new("/srv/cwd");
    let exe = Path::new("/opt/ace");
    let in_cwd = |p: &Path| p == Path::new("/srv/cwd/Config.js");
    let nowhere = |_: &Path| false;
    assert_eq!(
        ConfigManager::resolve_path("Config.js", cwd, Some(exe), &in_cwd),
        PathBuf::from("/srv/cwd/Config.js")
    );
    assert_eq!(
        ConfigManager::resolve_path("Config.js", cwd, Some(exe), &nowhere),
        PathBuf::from("/opt/ace/Config.js")
    );
    assert_eq!(
        ConfigManager::resolve_path("Config.js", cwd, None, &nowhere),
        PathBuf::from("/srv/cwd/Config.js")
    );
    assert_eq!(
        ConfigManager::resolve_path("etc/My.js", cwd, Some(exe), &nowhere),
        PathBuf::from("etc/My.js")
    );
}

#[test]
fn initialize_sets_the_process_configuration() {
    let mut c = MasterConfiguration::default();
    c.server.world_name = "Test World".into();
    ConfigManager::initialize(c);
    assert_eq!(ConfigManager::config().server.world_name, "Test World");
    match ConfigManager::initialize_from_path("__no_such_dir__/Config.js") {
        Err(e @ ConfigError::Missing(_)) => assert_eq!(e.to_string(), "missing configuration file"),
        other => panic!("expected Missing, got {other:?}"),
    }
    assert_eq!(
        ConfigManager::config().server.world_name,
        "Test World",
        "a failed load keeps the old configuration"
    );
}

/// A thread override is seen only on its thread.
#[test]
fn a_thread_override_is_seen_only_on_its_thread() {
    // (the process configuration is left alone: another test here changes it)
    let world_name =
        || std::panic::catch_unwind(|| ConfigManager::config().server.world_name.clone()).ok();
    let mut c = MasterConfiguration::default();
    c.server.world_name = "Overridden".into();
    let guard = ConfigManager::override_for_thread(c);
    assert_eq!(ConfigManager::config().server.world_name, "Overridden");
    let elsewhere = std::thread::spawn(world_name).join().unwrap();
    assert_ne!(
        elsewhere.as_deref(),
        Some("Overridden"),
        "another thread reads the process configuration"
    );
    {
        let mut inner = MasterConfiguration::default();
        inner.server.world_name = "Inner".into();
        let _inner = ConfigManager::override_for_thread(inner);
        assert_eq!(ConfigManager::config().server.world_name, "Inner");
    }
    assert_eq!(
        ConfigManager::config().server.world_name,
        "Overridden",
        "the outer override is back"
    );
    drop(guard);
    assert_ne!(
        world_name().as_deref(),
        Some("Overridden"),
        "the process configuration again"
    );
}

/// Not ACE: `server.character_screen_message` (empty by default) is read from the TOML file,
/// with its escapes.
#[test]
fn the_character_screen_message_is_read_from_the_toml_file() {
    use empyrean_common::toml_config;
    assert_eq!(
        MasterConfiguration::default()
            .server
            .character_screen_message,
        ""
    );
    let parsed = toml_config::from_toml_str(
        "[server]\ncharacter_screen_message = \"Welcome to Dereth.\\nPatch notes follow.\"\n",
    )
    .expect("valid");
    assert_eq!(parsed.unknown_keys, Vec::<String>::new());
    assert_eq!(
        parsed.config.server.character_screen_message,
        "Welcome to Dereth.\nPatch notes follow."
    );
}

/// Not ACE: `server.source_url` (empty by default) replaces the build's source address wherever
/// the server offers its source; blank means the build's.
#[test]
fn source_url_overrides_the_compiled_default() {
    use empyrean_common::{brand, toml_config};
    assert_eq!(MasterConfiguration::default().server.source_url, "");
    assert_eq!(
        brand::SOURCE_URL,
        option_env!("EMPYREAN_BUILD_SOURCE_URL")
            .unwrap_or("https://github.com/dereth-network/dereth")
    );
    let parsed =
        toml_config::from_toml_str("[server]\nsource_url = \"https://example.org/fork\"\n")
            .expect("valid");
    assert_eq!(parsed.unknown_keys, Vec::<String>::new());
    assert_eq!(parsed.config.server.source_url, "https://example.org/fork");
    {
        let _fork = ConfigManager::override_for_thread(parsed.config);
        assert_eq!(brand::source_url(), "https://example.org/fork");
        assert_eq!(
            brand::welcome(&brand::source_url()),
            "Welcome to Dereth\n powered by Empyrean (a fork of ACEmulator)\n(https://example.org/fork)\n\nFor more information on commands supported by this server, type @emphelp\n"
        );
    }
    let mut blank = MasterConfiguration::default();
    blank.server.source_url = "  ".into();
    let _blank = ConfigManager::override_for_thread(blank);
    assert_eq!(brand::source_url(), brand::SOURCE_URL);
    assert!(ConfigManager::try_config().is_some());
}
