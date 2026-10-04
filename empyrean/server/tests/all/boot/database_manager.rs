//! ACE: Source/ACE.Database/DatabaseManager.cs::DatabaseManager
//! DatabaseManager start-up checks: good pack installs the shard, out-of-range instance guid and
//! missing human weenie abort, empty content skips checks, no admin turns on auto-promotion,
//! database paths, files created/reopened.
//! Fixture: isolated configuration paths and synthetic server state.

use empyrean_common::era::EraExt as _;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_common::config_manager::ConfigManager;
use empyrean_common::config_paths::PathBase;
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{AccessLevel, WeenieType};
use empyrean_server::database_manager::{self, InitializeOptions};
use empyrean_store::{AuthDatabase, MemAuth, MemShard, ShardDatabase};
use empyrean_world::World;

fn clock() -> Arc<dyn Clock> {
    Arc::new(VirtualClock::default())
}

fn world_with(content: MemContent) -> World {
    let clock = VirtualClock::default();
    let now = ClockSnapshot::take(&clock, 0.0);
    let mut w = World::new(now, FakeDats::new().build().expect("empty fake dats"));
    w.content = Arc::new(content);
    w
}

fn options(world_content_loaded: bool) -> InitializeOptions {
    InitializeOptions {
        world_content_loaded,
        shard_player_biota_cache_time: 31,
        shard_non_player_biota_cache_time: 11,
        clock: clock(),
        threaded: false,
    }
}

fn instance(guid: u32) -> LandblockInstance {
    LandblockInstance {
        guid,
        weenie_class_id: 1,
        obj_cell_id: 0xA9B4_0001,
        ..LandblockInstance::default()
    }
}

/// A world database that passes ACE's checks: the `human` weenie, and static instance GUIDs.
fn good_content() -> MemContent {
    MemContent::new()
        .weenie(Weenie::new(1, "human", WeenieType::Creature))
        .landblock_instance(instance(0x7A9B_4000))
        .landblock_instance(instance(0x7FFF_FFFF))
}

/// A shard holding one biota, so that tests can tell whether it was installed.
fn shard_with(id: u32) -> MemShard {
    let mut shard = MemShard::new();
    let mut biota = empyrean_entity::Biota {
        id,
        weenie_class_id: 1,
        ..Default::default()
    };
    assert!(shard.save_biota(&mut biota, false));
    shard
}

fn auth() -> Box<dyn AuthDatabase> {
    Box::new(MemAuth::new(AccountDefaults::default(), clock()))
}

fn installed_shard_has(w: &World, id: u32) -> bool {
    w.shard.base_database().get_biota(id, false).is_some()
}

#[test]
fn a_good_pack_passes_the_start_up_checks_and_installs_the_shard() {
    let mut w = world_with(good_content());
    let failed =
        database_manager::initialize(&mut w, auth(), shard_with(0x5000_0001), &options(true));
    assert!(!failed, "InitializationFailure");
    assert!(
        installed_shard_has(&w, 0x5000_0001),
        "DatabaseManager.Shard is the given shard"
    );
}

#[test]
fn an_instance_guid_outside_the_static_range_aborts() {
    // IsWorldDatabaseGuidRangeValid: no landblock instance may have Guid >= 0x80000000.
    let mut w = world_with(good_content().landblock_instance(instance(0x8000_0000)));
    let failed =
        database_manager::initialize(&mut w, auth(), shard_with(0x5000_0001), &options(true));
    assert!(failed, "InitializationFailure");
    assert!(
        !installed_shard_has(&w, 0x5000_0001),
        "ACE returns before building the shard"
    );
}

#[test]
fn a_pack_without_the_human_weenie_aborts() {
    let content = MemContent::new()
        .weenie(Weenie::new(2, "humanoid", WeenieType::Creature))
        .landblock_instance(instance(0x7A9B_4000));
    let mut w = world_with(content);
    let failed =
        database_manager::initialize(&mut w, auth(), shard_with(0x5000_0001), &options(true));
    assert!(failed, "InitializationFailure");
    assert!(!installed_shard_has(&w, 0x5000_0001));
}

/// Divergence: V388
#[test]
fn a_pack_built_for_another_era_aborts_and_one_built_for_the_configured_era_passes() {
    use empyrean_common::era::EraId;
    for (pack, server, fails) in [
        (EraId::Infiltration, EraId::Eor, true),
        (EraId::Eor, EraId::Infiltration, true),
        (EraId::Infiltration, EraId::Infiltration, false),
        (EraId::Eor, EraId::Eor, false),
    ] {
        let mut w = world_with(good_content().era(pack));
        w.era = server.rules();
        let failed =
            database_manager::initialize(&mut w, auth(), shard_with(0x5000_0001), &options(true));
        assert_eq!(failed, fails, "a {pack} pack on an {server} server");
        assert_eq!(installed_shard_has(&w, 0x5000_0001), !fails);
    }
}

#[test]
fn empty_content_skips_the_checks() {
    let mut w = world_with(MemContent::new());
    let failed =
        database_manager::initialize(&mut w, auth(), shard_with(0x5000_0001), &options(false));
    assert!(!failed);
    assert!(installed_shard_has(&w, 0x5000_0001));
}

#[test]
fn no_admin_account_turns_on_auto_promotion() {
    let mut w = world_with(good_content());
    assert!(!database_manager::initialize(
        &mut w,
        auth(),
        MemShard::new(),
        &options(true)
    ));
    assert!(w.auth.auto_promote_next_account_to_admin());

    let local = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let mut with_admin = auth();
    with_admin
        .create_account("f22player", "pw", AccessLevel::Player, local)
        .expect("created");
    let mut w = world_with(good_content());
    assert!(!database_manager::initialize(
        &mut w,
        with_admin,
        MemShard::new(),
        &options(true)
    ));
    assert!(
        w.auth.auto_promote_next_account_to_admin(),
        "a Player account is not an admin"
    );

    let mut with_admin = auth();
    with_admin
        .create_account("f22admin", "pw", AccessLevel::Admin, local)
        .expect("created");
    let mut w = world_with(good_content());
    assert!(!database_manager::initialize(
        &mut w,
        with_admin,
        MemShard::new(),
        &options(true)
    ));
    assert!(!w.auth.auto_promote_next_account_to_admin());
    assert!(
        w.auth.lock().get_account_by_name("f22admin").is_some(),
        "DatabaseManager.Authentication"
    );
}

#[test]
fn database_paths_come_from_the_config_then_the_default() {
    let root = std::path::absolute("/").unwrap();
    let (cwd, conf) = (root.join("work"), root.join("etc").join("empyrean"));
    let file = conf.join("empyrean.toml");
    let base = PathBase::new(Some(&file), &cwd, Some(root.join("home")), None);
    let key = if cfg!(windows) {
        r"D:\data\shard.db"
    } else {
        "/data/shard.db"
    };
    assert_eq!(
        database_manager::database_path(key, "./d.db", &base),
        PathBuf::from(key)
    );
    // Relative (the default included): beside the configuration file, not in the working directory.
    assert_eq!(
        database_manager::database_path(" ", "./d.db", &base),
        conf.join("d.db")
    );
    assert_eq!(
        database_manager::database_path("", "./d.db", &base),
        conf.join("d.db")
    );
    assert_eq!(
        database_manager::database_path("data/s.db", "./d.db", &base),
        conf.join("data").join("s.db")
    );
    assert_eq!(
        database_manager::database_path("~/s.db", "./d.db", &base),
        root.join("home").join("s.db")
    );
    // No configuration file: the working directory.
    let defaults_base = PathBase::new(None, &cwd, None, None);
    assert_eq!(
        database_manager::database_path("", "./d.db", &defaults_base),
        cwd.join("d.db")
    );

    let text =
        "[database]\nshard_db_path = 'D:\\data\\shard.db'\nauth_db_path = 'D:\\data\\auth.db'\n";
    let config = empyrean_common::toml_config::from_toml_str(text)
        .expect("valid")
        .config;
    assert_eq!(config.database.shard_db_path, r"D:\data\shard.db");
    assert_eq!(config.database.auth_db_path, r"D:\data\auth.db");
    if cfg!(windows) {
        assert_eq!(
            database_manager::configured_shard_db_path(&config, &base),
            PathBuf::from(r"D:\data\shard.db")
        );
        assert_eq!(
            database_manager::configured_auth_db_path(&config, &base),
            PathBuf::from(r"D:\data\auth.db")
        );
    }
    let defaults = MasterConfiguration::default();
    assert_eq!(
        database_manager::configured_shard_db_path(&defaults, &base),
        conf.join("shard.db")
    );
    assert_eq!(
        database_manager::configured_auth_db_path(&defaults, &base),
        conf.join("auth.db")
    );

    let defaults = MasterConfiguration::default();
    assert_eq!(defaults.database.shard_db_path, "./shard.db");
    assert_eq!(defaults.database.auth_db_path, "./auth.db");
    let text = ConfigManager::serialize(&defaults);
    assert!(
        text.contains("\"ShardDbPath\": \"./shard.db\"")
            && text.contains("\"AuthDbPath\": \"./auth.db\""),
        "{text}"
    );
}

#[test]
fn database_files_are_created_and_reopened() {
    let dir = std::env::temp_dir().join(format!(
        "empyrean-server-database-manager-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (shard_path, auth_path) = (dir.join("shard.db"), dir.join("auth.db"));
    let _ = std::fs::remove_file(&shard_path);
    let _ = std::fs::remove_file(&auth_path);

    let (mut shard, mut auth) = database_manager::open_databases(
        &shard_path,
        &auth_path,
        AccountDefaults::default(),
        clock(),
    )
    .expect("opened");
    let mut biota = empyrean_entity::Biota {
        id: 0x5000_0009,
        weenie_class_id: 1,
        ..Default::default()
    };
    assert!(shard.save_biota(&mut biota, false));
    auth.create_account(
        "f22file",
        "pw",
        AccessLevel::Player,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
    )
    .expect("created");
    drop((shard, auth));

    let (mut shard, mut auth) = database_manager::open_databases(
        &shard_path,
        &auth_path,
        AccountDefaults::default(),
        clock(),
    )
    .expect("reopened");
    assert!(shard.get_biota(0x5000_0009, false).is_some());
    assert!(auth.get_account_by_name("f22file").is_some());
    drop((shard, auth));
    let _ = std::fs::remove_dir_all(&dir);
}
