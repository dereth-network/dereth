//! ACE: Source/ACE.Server/Managers/PlayerManager.cs::PlayerManager
//! PlayerManager, OfflinePlayer, deletion check and login helpers on a bare World.
//! Fixture: synthetic dats, isolated world state.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::not_ported;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{AccessLevel, PropertyFloat, PropertyInt, PropertyString, WeenieType};
use empyrean_entity::ObjectGuid;
use empyrean_net::{NetAuthType, PacketInboundLoginRequest, SessionId};
use empyrean_store::models::shard::Character;
use empyrean_store::MemAuth;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::i_player::{self, IPlayer};
use empyrean_world::managers::player_manager as pm;
use empyrean_world::managers::world_manager as wm;
use empyrean_world::network::handlers::authentication_handler;
use empyrean_world::sessions::{self, CharacterSummary, SessionData};
use empyrean_world::world::AuthHandle;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const BOSS: u32 = 0x5000_0003;
const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

/// A world at 2026-01-01 00:00 UTC (Unix 1767225600), with its stores on the same clock.
fn world() -> (World, Arc<VirtualClock>) {
    let clock = Arc::new(VirtualClock::default());
    let now = ClockSnapshot::take(&*clock, 0.0);
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("empty fake dats"),
    );
    let shared: Arc<dyn Clock> = Arc::<VirtualClock>::clone(&clock);
    w.auth = AuthHandle::new(Box::new(MemAuth::new(AccountDefaults::default(), shared)));
    (w, clock)
}

/// Moves the world's clock reading on by `secs`.
fn pass(w: &mut World, clock: &VirtualClock, secs: f64) {
    clock.advance(Duration::from_secs_f64(secs));
    w.now = ClockSnapshot::take(clock, w.now.portal_year_ticks + secs);
}

fn seed_account(w: &World, name: &str) -> u32 {
    w.auth
        .lock()
        .create_account(
            name,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id
}

fn seed_character(w: &World, account_id: u32, guid: u32, name: &str) {
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    let character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    assert!(w
        .shard
        .base_database()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

/// Alpha and Bravo on account "acct", "+Boss" on account "boss", loaded into `PlayerManager`.
fn seeded() -> (World, Arc<VirtualClock>, u32) {
    let (mut w, clock) = world();
    let acct = seed_account(&w, "acct");
    seed_character(&w, acct, ALPHA, "Alpha");
    seed_character(&w, acct, BRAVO, "Bravo");
    let boss = seed_account(&w, "boss");
    seed_character(&w, boss, BOSS, "+Boss");
    pm::initialize(&mut w);
    (w, clock, acct)
}

fn player_object(w: &mut World, guid: u32) {
    let mut o = WorldObject::allocate(Class::Player);
    o.guid = ObjectGuid::new(guid);
    o.biota = pm::get_offline_player(w, guid)
        .expect("offline")
        .biota
        .clone();
    o.player.as_mut().expect("a player").player.character =
        Some(empyrean_store::models::shard::Character {
            id: guid,
            ..Default::default()
        });
    w.objects.insert(o).expect("fresh guid");
}

#[test]
fn player_manager_indexes_players_by_guid_name_and_account() {
    let (w, _, acct) = seeded();
    assert_eq!(pm::get_offline_count(&w), 3);
    assert_eq!(
        pm::get_all_offline(&w),
        vec![
            ObjectGuid::new(ALPHA),
            ObjectGuid::new(BRAVO),
            ObjectGuid::new(BOSS)
        ]
    );
    assert_eq!(
        pm::get_offline_player(&w, ALPHA).and_then(|o| o.account.as_ref().map(|a| a.account_id)),
        Some(acct)
    );

    // `playerNames` is OrdinalIgnoreCase, and FindByName trims leading '+'.
    assert_eq!(
        pm::find_by_name(&w, "ALPHA"),
        (Some(IPlayer::Offline(ObjectGuid::new(ALPHA))), false)
    );
    assert_eq!(
        pm::find_by_name(&w, "+bravo").0,
        Some(IPlayer::Offline(ObjectGuid::new(BRAVO)))
    );
    // The index holds "+Boss" itself, so the trimmed name misses it...
    assert_eq!(pm::find_by_name(&w, "+Boss").0, None);
    // ...while GetOfflinePlayer(name) also tries "+" + name.
    assert_eq!(
        pm::get_offline_player_by_name(&w, "boss").map(|o| o.guid.full()),
        Some(BOSS)
    );
    assert_eq!(
        pm::find_by_guid(&w, BRAVO),
        (Some(IPlayer::Offline(ObjectGuid::new(BRAVO))), false)
    );
    assert_eq!(pm::find_by_guid(&w, 0x5000_0099), (None, false));

    let players = pm::get_account_players(&w, acct).expect("account entry");
    assert_eq!(
        players.keys().copied().collect::<Vec<_>>(),
        vec![ALPHA, BRAVO]
    );
    assert_eq!(
        i_player::name(&w, IPlayer::Offline(ObjectGuid::new(ALPHA))).as_deref(),
        Some("Alpha")
    );
    assert!(!i_player::is_deleted(
        &w,
        IPlayer::Offline(ObjectGuid::new(ALPHA))
    ));
    assert!(!i_player::is_pending_deletion(
        &w,
        IPlayer::Offline(ObjectGuid::new(ALPHA))
    ));
}

#[test]
fn switching_a_player_online_and_back_moves_it_between_the_indexes() {
    let (mut w, _, acct) = seeded();
    let alpha = ObjectGuid::new(ALPHA);
    w.player_manager
        .offline_players
        .get_mut(&ALPHA)
        .unwrap()
        .set_property(PropertyInt::Level, 5);
    player_object(&mut w, ALPHA);

    assert!(pm::switch_player_from_offline_to_online(&mut w, alpha));
    assert_eq!(pm::get_online_player(&w, ALPHA), Some(alpha));
    assert_eq!(pm::get_online_count(&w), 1);
    assert_eq!(pm::get_offline_count(&w), 2);
    assert_eq!(
        pm::find_by_name(&w, "alpha"),
        (Some(IPlayer::Online(alpha)), true)
    );
    assert_eq!(
        pm::find_by_guid(&w, ALPHA),
        (Some(IPlayer::Online(alpha)), true)
    );
    assert_eq!(
        pm::get_account_players(&w, acct).unwrap().get(&ALPHA),
        Some(&IPlayer::Online(alpha))
    );
    assert_eq!(pm::get_online_player_by_name(&w, "ALPHA"), Some(alpha));
    assert!(
        w.objects
            .get(alpha)
            .unwrap()
            .wo
            .world_object_database
            .changes_detected,
        "the offline changes carry over"
    );
    assert_eq!(
        i_player::account(&w, IPlayer::Online(alpha)).map(|a| a.account_id),
        Some(acct)
    );
    // A second switch finds no offline player.
    assert!(!pm::switch_player_from_offline_to_online(&mut w, alpha));

    assert!(pm::switch_player_from_online_to_offline(&mut w, alpha));
    assert_eq!(pm::get_online_count(&w), 0);
    assert_eq!(
        pm::find_by_name(&w, "Alpha"),
        (Some(IPlayer::Offline(alpha)), false)
    );
    assert_eq!(
        pm::get_account_players(&w, acct).unwrap().get(&ALPHA),
        Some(&IPlayer::Offline(alpha))
    );
    assert_eq!(
        pm::get_offline_player(&w, ALPHA).and_then(|o| o.level()),
        Some(5)
    );
    assert!(!pm::switch_player_from_online_to_offline(&mut w, alpha));
}

#[test]
fn process_deleted_player_drops_it_from_every_index() {
    let (mut w, _, acct) = seeded();
    assert!(pm::process_deleted_player(&mut w, BRAVO));
    assert!(pm::get_offline_player(&w, BRAVO).is_none());
    assert_eq!(pm::find_by_name(&w, "Bravo").0, None);
    assert_eq!(
        pm::get_account_players(&w, acct)
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![ALPHA]
    );
    assert!(
        !pm::process_deleted_player(&mut w, BRAVO),
        "should never happen: already gone"
    );
}

#[test]
fn tick_saves_changed_offline_players_every_hour() {
    let (mut w, clock, _) = seeded();
    let saved_level = |w: &World| {
        w.shard
            .base_database()
            .get_all_player_biotas_in_parallel()
            .into_iter()
            .find(|b| b.id == ALPHA)
            .and_then(|b| b.get_property(PropertyInt::Level))
    };

    // `lastDatabaseSave` starts at DateTime.MinValue, so the first tick saves.
    w.player_manager
        .offline_players
        .get_mut(&ALPHA)
        .unwrap()
        .set_property(PropertyInt::Level, 7);
    pm::tick(&mut w);
    assert_eq!(saved_level(&w), Some(7));
    assert_eq!(w.player_manager.last_database_save, w.now.utc);
    assert!(
        !w.player_manager
            .offline_players
            .get(&ALPHA)
            .unwrap()
            .changes_detected
    );

    w.player_manager
        .offline_players
        .get_mut(&ALPHA)
        .unwrap()
        .set_property(PropertyInt::Level, 8);
    pass(&mut w, &clock, 3599.0);
    pm::tick(&mut w);
    assert_eq!(saved_level(&w), Some(7), "not before the hour");
    pass(&mut w, &clock, 1.0);
    pm::tick(&mut w);
    assert_eq!(
        saved_level(&w),
        Some(8),
        "`lastDatabaseSave + 1h <= UtcNow`"
    );
}

#[test]
fn tick_runs_the_delayed_and_the_forced_logoff_queues_in_seconds() {
    let (mut w, clock, _) = seeded();
    let alpha = ObjectGuid::new(ALPHA);
    player_object(&mut w, ALPHA);
    w.objects
        .get_mut(alpha)
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player
        .character = Some(empyrean_store::models::shard::Character {
        id: ALPHA,
        ..Default::default()
    });
    w.objects
        .get_mut(alpha)
        .unwrap()
        .biota
        .properties_enchantment_registry = Some(Default::default());
    w.sessions.insert(
        S,
        SessionData {
            player: Some(alpha),
            ..SessionData::default()
        },
    );

    // The PK logout delay: LogoffTimestamp is a Unix time.
    let due = w.now.unix_time + 20.0;
    w.objects
        .get_mut(alpha)
        .unwrap()
        .set_property(PropertyFloat::LogoffTimestamp, due);
    pm::add_player_to_logoff_queue(&mut w, alpha);
    pm::add_player_to_logoff_queue(&mut w, alpha);
    assert_eq!(
        w.player_manager.players_pending_logoff.len(),
        1,
        "added once"
    );
    pass(&mut w, &clock, 19.5);
    not_ported::take_local();
    pm::tick(&mut w);
    assert_eq!(w.player_manager.players_pending_logoff.len(), 1);
    pass(&mut w, &clock, 0.5);
    pm::tick(&mut w);
    assert!(w.player_manager.players_pending_logoff.is_empty());
    let p = w.objects.get(alpha).unwrap();
    assert!(p.player.as_ref().unwrap().player.is_logging_out && p.wo.world_object.is_busy);
    assert!(w.player_manager.players_pending_final_logoff.is_empty());
    assert_eq!(
        p.get_property(PropertyFloat::LogoffTimestamp),
        Some(w.now.unix_time),
        "SetPropertiesAtLogOut"
    );
    assert_eq!(
        w.sessions.get(S).unwrap().log_off_request_time,
        w.now.utc,
        "`Session.logOffRequestTime = DateTime.UtcNow`"
    );

    // The final log-off: forced 15 minutes after it was queued.
    pm::add_player_to_final_logoff_queue(&mut w, alpha);
    let finalized = w
        .player_manager
        .players_pending_final_logoff
        .front()
        .unwrap()
        .log_off_finalized_time;
    assert!((finalized - (w.now.unix_time + 900.0)).abs() < 1e-6);
    pass(&mut w, &clock, 899.0);
    pm::tick(&mut w);
    assert_eq!(w.player_manager.players_pending_final_logoff.len(), 1);
    pass(&mut w, &clock, 1.0);
    not_ported::take_local();
    pm::tick(&mut w);
    assert!(w.player_manager.players_pending_final_logoff.is_empty());
    assert!(
        !w.objects
            .get(alpha)
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .player
            .forced_log_off_requested
    );
    assert!(w
        .shard
        .base_database()
        .get_biota(ALPHA, false)
        .is_some_and(|b| b
            .biota_properties_float
            .iter()
            .any(|f| f.r#type == PropertyFloat::LogoffTimestamp.0)));

    pm::add_player_to_final_logoff_queue(&mut w, alpha);
    pm::remove_player_from_final_logoff_queue(&mut w, alpha);
    assert!(w.player_manager.players_pending_final_logoff.is_empty());
}

#[test]
fn an_account_is_at_its_character_slots_at_elevens() {
    let (w, _, acct) = seeded();
    assert!(!pm::is_account_at_max_character_slots(&w, "ACCT"));
    let mut w = w;
    for i in 0..9 {
        seed_character(&w, acct, 0x5000_0010 + i, &format!("Extra{i}"));
    }
    w.player_manager = Default::default();
    pm::initialize(&mut w);
    // 11 characters: `max_chars_per_account` (11) reached, ignoring the name's case.
    assert!(pm::is_account_at_max_character_slots(&w, "Acct"));
    assert!(!pm::is_account_at_max_character_slots(&w, "boss"));
}

#[test]
fn check_characters_for_deletion_deletes_once_the_time_has_strictly_passed() {
    let (mut w, clock, _) = seeded();
    // `Time.GetUnixTime() > DeleteTime`: equal is not yet.
    let unix = w.now.unix_time;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let delete_time = unix as u64;
    assert!(
        (unix - unix.trunc()).abs() < 1e-9,
        "the test clock is on a whole second"
    );
    let chars = vec![
        CharacterSummary {
            id: ALPHA,
            name: "Alpha".into(),
            delete_time,
            ..Default::default()
        },
        CharacterSummary {
            id: BRAVO,
            name: "Bravo".into(),
            ..Default::default()
        },
    ];
    w.sessions.insert(S, SessionData::default());
    sessions::update_characters(&mut w, S, chars);
    assert_eq!(w.sessions.get(S).unwrap().characters.len(), 2);

    pass(&mut w, &clock, 0.5);
    sessions::check_characters_for_deletion(&mut w, S);
    let left: Vec<u32> = w
        .sessions
        .get(S)
        .unwrap()
        .characters
        .iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(left, vec![BRAVO]);
    let stub = w
        .shard
        .base_database()
        .get_character_stub_by_guid(ALPHA)
        .expect("row kept");
    assert!(stub.is_deleted);
    assert!(
        pm::get_offline_player(&w, ALPHA).is_none(),
        "ProcessDeletedPlayer"
    );
}

#[test]
fn log_off_player_starts_the_log_off_clock_once() {
    let (mut w, clock, _) = world_and_clock();
    sessions::log_off_player(&mut w, S, false);
    assert_eq!(
        w.sessions.get(S).unwrap().log_off_request_time,
        DotNetDateTime::MIN_VALUE,
        "no player: nothing"
    );

    w.sessions
        .get_mut(S)
        .unwrap()
        .set_player(Some(ObjectGuid::new(ALPHA)));
    sessions::log_off_player(&mut w, S, false);
    let first = w.now.utc;
    assert_eq!(w.sessions.get(S).unwrap().log_off_request_time, first);
    pass(&mut w, &clock, 1.0);
    sessions::log_off_player(&mut w, S, false);
    assert_eq!(
        w.sessions.get(S).unwrap().log_off_request_time,
        first,
        "only while logOffRequestTime is MinValue"
    );
}

fn world_and_clock() -> (World, Arc<VirtualClock>, ()) {
    let (mut w, clock) = world();
    w.sessions.insert(S, SessionData::default());
    (w, clock, ())
}

#[test]
fn send_connect_response_lists_the_last_played_first() {
    // SendConnectResponse reads ConfigManager.Config.Server.WorldName.
    empyrean_common::config_manager::ConfigManager::initialize(
        empyrean_common::master_configuration::MasterConfiguration::default(),
    );
    let (mut w, _) = world();
    w.sessions.insert(
        S,
        SessionData {
            account: Some("acct".into()),
            ..SessionData::default()
        },
    );
    let c = |id: u32, t: f64| CharacterSummary {
        id,
        name: format!("C{id}"),
        last_login_timestamp: t,
        ..Default::default()
    };
    // OrderByDescending is stable, and .NET orders NaN below every number.
    let chars = vec![c(1, 1.0), c(2, f64::NAN), c(3, 3.0), c(4, 3.0), c(5, 2.0)];
    authentication_handler::send_connect_response(&mut w, S, chars);
    let ids: Vec<u32> = w
        .sessions
        .get(S)
        .unwrap()
        .characters
        .iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec![3, 4, 5, 1, 2]);
}

fn login_request(account: &str, password: &str) -> PacketInboundLoginRequest {
    PacketInboundLoginRequest {
        net_auth_type: NetAuthType::AccountPassword,
        timestamp: 0,
        account: account.to_owned(),
        password: Some(password.to_owned()),
        gls_ticket: None,
        client_version: "1802".to_owned(),
    }
}

#[test]
fn do_login_auto_creates_at_the_configured_access_level_or_as_the_first_admin() {
    let from = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 50_000);
    for (default_access_level, promote, want) in [
        (0, false, AccessLevel::Player),
        (2, false, AccessLevel::Sentinel),
        (9, false, AccessLevel::Player),
        (0, true, AccessLevel::Admin),
    ] {
        let (mut w, clock) = world();
        let shared: Arc<dyn Clock> = Arc::<VirtualClock>::clone(&clock);
        let config = AccountDefaults {
            default_access_level,
            ..AccountDefaults::default()
        };
        w.auth = AuthHandle::new(Box::new(MemAuth::new(config, shared)));
        w.auth.set_auto_promote_next_account_to_admin(promote);
        w.sessions.insert(S, SessionData::default());

        // No transport session: the account is created, then empyrean-net's half ends the login.
        authentication_handler::do_login(&mut w, S, from, &login_request("MixedCase", "pw"));
        let account = w
            .auth
            .lock()
            .get_account_by_name("mixedcase")
            .expect("created, lower-cased");
        assert_eq!(
            account.account_name, "mixedcase",
            "`loginRequest.Account.ToLower()`"
        );
        assert_eq!(
            account.access_level,
            u32::try_from(want.0).unwrap(),
            "DefaultAccessLevel {default_access_level}"
        );
        assert!(
            !w.auth.auto_promote_next_account_to_admin(),
            "the promotion is used up"
        );
    }

    // An empty password creates nothing.
    let (mut w, _) = world();
    authentication_handler::do_login(&mut w, S, from, &login_request("nobody", ""));
    assert!(w.auth.lock().get_account_by_name("nobody").is_none());
}

#[test]
fn player_enter_world_loads_the_possessions_then_enqueues_do_player_enter_world() {
    let (mut w, _, _) = seeded();
    w.sessions.insert(S, SessionData::default());
    let character = w
        .shard
        .base_database()
        .get_character_stub_by_guid(ALPHA)
        .expect("stub");
    not_ported::take_local();
    wm::player_enter_world(&mut w, S, character);
    assert!(
        !not_ported::take_local().contains_key("ACE: WorldManager.DoPlayerEnterWorld"),
        "waits for the shard"
    );
    wm::run_shard_callbacks(&mut w);
    empyrean_world::entity::actions::action_queue::run_actions(
        &mut w,
        empyrean_world::entity::actions::i_actor::Actor::World,
    );
    assert_eq!(
        w.sessions.get(S).and_then(|s| s.player),
        Some(empyrean_entity::ObjectGuid::new(ALPHA))
    );

    // An unknown character is only logged.
    let unknown = Character {
        id: 0x5000_0099,
        ..Character::default()
    };
    wm::player_enter_world(&mut w, S, unknown);
    wm::run_shard_callbacks(&mut w);
    assert_eq!(w.world_manager.action_queue.len(), 0);
}
