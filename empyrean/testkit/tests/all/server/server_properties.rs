//! ACE: Source/ACE.Server/Managers/PropertyManager.cs::PropertyManager
//! TestServer starts as Program.Main; world_closed refuses with logon server full; character list
//! and DDD interrogation follow properties; property worker every 300 s and on shutdown.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use dereth_protocol::admin::DddInterrogation;
use dereth_protocol::login::{CharacterError, LoginCharacterSet};
use empyrean_entity::enums::AccessLevel;
use empyrean_net::SessionState;
use empyrean_testkit::{ClientId, ClientStatus, TestServer};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::managers::server_manager;
use empyrean_world::managers::world_manager::WorldStatusState;
use empyrean_world::World;

/// ACE's `CharacterError.LogonServerFull`.
const LOGON_SERVER_FULL: u32 = 0x15;

fn fake_dats() -> std::sync::Arc<empyrean_dat::DatManager> {
    empyrean_dat::FakeDats::new()
        .build()
        .expect("empty fake dats")
}

fn seed_account(w: &World, name: &str) {
    w.auth
        .lock()
        .create_account(
            name,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created");
}

/// A server whose shard held `seed`'s properties (and an account `acct`/`pw`) before start-up.
fn server_with(seed: impl FnOnce(&mut dyn empyrean_store::ShardConfigDatabase)) -> TestServer {
    TestServer::with_setup(fake_dats(), |w| {
        seed_account(w, "acct");
        let config = w.property_manager.shard_config.clone();
        let mut db = config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        seed(&mut **db);
    })
}

fn errors(ts: &TestServer, id: ClientId) -> Vec<u32> {
    ts.received::<CharacterError>(id)
        .iter()
        .map(|e| e.char_error)
        .collect()
}

fn character_list(ts: &mut TestServer, id: ClientId) -> LoginCharacterSet {
    assert!(
        ts.run_until(6.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    ts.received::<LoginCharacterSet>(id).remove(0)
}

#[test]
fn the_server_starts_open_and_initialised_as_program_main_leaves_it() {
    let ts = TestServer::new();
    assert_eq!(ts.world.world_manager.world_status, WorldStatusState::Open);
    // PropertyManager.Initialize started the worker timer 300 s from start-up.
    assert_eq!(
        ts.world.property_manager.worker_timer_due(),
        Some(Duration::from_secs(300))
    );
    assert_eq!(
        empyrean_world::sessions::config_server_world_name(),
        "Empyrean"
    );
}

#[test]
fn a_world_closed_by_the_property_refuses_players_with_logon_server_full() {
    let mut ts = server_with(|db| db.add_bool("world_closed", true, None));
    assert_eq!(
        ts.world.world_manager.world_status,
        WorldStatusState::Closed
    );

    let id = ts.add_client("acct", "pw");
    assert!(
        ts.run_until(6.0, |ts| !errors(ts, id).is_empty()),
        "the login is answered"
    );
    assert_eq!(errors(&ts, id), vec![LOGON_SERVER_FULL]);
    assert!(ts.received::<LoginCharacterSet>(id).is_empty());
    // SessionTerminationReason.WorldClosed: the session is dropped.
    assert!(ts.run_until(5.0, |ts| ts.world.net.get_session_count() == 0));

    // The same shard without the property: the list arrives.
    let mut ts = server_with(|_| {});
    let id = ts.add_client("acct", "pw");
    let _ = character_list(&mut ts, id);
    assert!(errors(&ts, id).is_empty());
}

#[test]
fn the_character_list_and_ddd_interrogation_follow_the_properties() {
    // DefaultPropertyManager: max_chars_per_account 11, use_turbine_chat true, allow_highres_dat false.
    let mut ts = server_with(|_| {});
    let id = ts.add_client("acct", "pw");
    let set = character_list(&mut ts, id);
    assert_eq!((set.num_allowed_characters, set.use_turbine_chat), (11, 1));
    assert!(ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()));
    assert_eq!(ts.received::<DddInterrogation>(id)[0].product_id, 0x1);

    let mut ts = server_with(|db| {
        db.add_long("max_chars_per_account", 20, None);
        db.add_bool("use_turbine_chat", false, None);
        db.add_bool("allow_highres_dat", true, None);
    });
    let id = ts.add_client("acct", "pw");
    let set = character_list(&mut ts, id);
    assert_eq!((set.num_allowed_characters, set.use_turbine_chat), (20, 0));
    assert!(ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()));
    assert_eq!(ts.received::<DddInterrogation>(id)[0].product_id, 0x1 | 0x4);
}

/// A client added after 200 seconds of uptime connects.
#[test]
fn a_client_added_after_200_seconds_of_uptime_connects() {
    let mut ts = server_with(|_| {});
    ts.advance(200.0);
    assert!(ts.seconds() >= 200.0);
    let id = ts.connect("acct", "pw");
    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
    let _ = character_list(&mut ts, id);
    let session = ts.world.net.find_by_account("acct").expect("session");
    assert_eq!(
        ts.world.net.session(session).map(|s| s.core.state),
        Some(SessionState::AuthConnected)
    );

    // client_mut reaches the same client.
    ts.client_mut(id).clock_scale = 1.0;
    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
}

#[test]
fn the_world_loop_runs_the_property_worker_every_300_seconds() {
    let mut ts = TestServer::new();
    let config = ts.world.property_manager.shard_config.clone();
    let written = || {
        config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_bool("world_closed")
            .map(|r| r.value)
    };

    assert!(pm::modify_bool(&ts.world, "world_closed", true));
    ts.advance(299.9);
    assert_eq!(written(), None, "not written before 300 s");
    ts.advance(0.2);
    assert_eq!(written(), Some(true), "written at 300 s");

    assert!(pm::modify_bool(&ts.world, "world_closed", false));
    ts.advance(299.0);
    assert_eq!(written(), Some(true));
    ts.advance(1.1);
    assert_eq!(written(), Some(false), "and again 300 s later");
}

#[test]
fn shutdown_resyncs_and_stops_the_property_worker() {
    let mut ts = TestServer::new();
    let config = ts.world.property_manager.shard_config.clone();
    assert!(pm::modify_long(&ts.world, "max_chars_per_account", 15));
    server_manager::do_shutdown_now(&mut ts.world);
    assert!(ts.run_until(10.0, |ts| ts.exited()));
    let row = config
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_long("max_chars_per_account");
    assert_eq!(row.map(|r| r.value), Some(15), "ResyncVariables wrote it");
    assert_eq!(
        ts.world.property_manager.worker_timer_due(),
        None,
        "StopUpdating"
    );
}
