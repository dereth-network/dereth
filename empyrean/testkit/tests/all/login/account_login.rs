//! ACE: Source/ACE.Server/Network/Handlers/AuthenticationHandler.cs::HandleLoginRequest
//! Good login reaches character list; bad password booted; unknown account auto-created or
//! refused; second login boots the first; disconnect releases; delete/restore and timer; log off
//! returns to select after six seconds.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::admin::{AdminSendAdminRestoreCharacter, DddInterrogation};
use dereth_protocol::login::{
    CharGenVerificationResponse, CharacterDeleteAck, CharacterDeleteRequest, CharacterError,
    LoginAccountBooted, LoginCharacterScreenMessage, LoginCharacterSet, LoginEnterGameServerReady,
    LoginExecuteLogOff, LoginExecuteLogOffRequest, LoginSendEnterWorld, LoginSendEnterWorldRequest,
    LoginWorldInfo,
};
use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::Clock;
use empyrean_entity::enums::{AccessLevel, PropertyString, WeenieType};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState, SessionTerminationReason};
use empyrean_store::models::shard::Character;
use empyrean_store::MemAuth;
use empyrean_testkit::{decode, ClientId, ClientStatus, TestServer};
use empyrean_world::managers::{player_manager, world_manager};
use empyrean_world::sessions;
use empyrean_world::world::AuthHandle;

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;

/// ACE `CharacterError` values the client sees.
const LOGON: u32 = 0x01;
const DELETE: u32 = 0x06;
const ACCOUNT_DOESNT_EXIST: u32 = 0x0A;

/// A server whose world is open, as `Program.Main` leaves it when `world_closed` is off.
fn server() -> TestServer {
    let mut ts = TestServer::new();
    world_manager::open(&mut ts.world, None);
    ts
}

fn seed_account(ts: &TestServer, name: &str, password: &str) -> u32 {
    ts.auth()
        .create_account(
            name,
            password,
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id
}

fn seed_character(ts: &TestServer, account_id: u32, guid: u32, name: &str, last_login: f64) {
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
        last_login_timestamp: last_login,
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

/// An account with Alpha (played at t=100) and Bravo (played at t=200), and `PlayerManager` loaded
/// from the shard, as `Program.Main` does before the world starts.
fn seeded() -> TestServer {
    let ts = server();
    let account_id = seed_account(&ts, "acct", "secret");
    seed_character(&ts, account_id, ALPHA, "Alpha", 100.0);
    seed_character(&ts, account_id, BRAVO, "Bravo", 200.0);
    let mut ts = ts;
    player_manager::initialize(&mut ts.world);
    ts
}

fn session_of(ts: &TestServer, account: &str) -> SessionId {
    ts.world
        .net
        .find_by_account(account)
        .expect("a session for the account")
}

fn names(set: &LoginCharacterSet) -> Vec<(u32, String, u32)> {
    set.characters
        .iter()
        .map(|c| (c.gid.0, c.name.clone(), c.seconds_greyed_out))
        .collect()
}

fn errors(ts: &TestServer, id: ClientId) -> Vec<u32> {
    ts.received::<CharacterError>(id)
        .iter()
        .map(|e| e.char_error)
        .collect()
}

/// Logs in and runs until the character list has arrived.
fn login(ts: &mut TestServer, account: &str, password: &str) -> ClientId {
    let id = ts.connect(account, password);
    assert_eq!(
        ts.client(id).status(),
        ClientStatus::Connected,
        "{account} logs in"
    );
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    id
}

/// What a dropped client looks like to the server: the transport terminates the session (here as
/// its network timeout would) and drops it 2 s later.
fn drop_session(ts: &mut TestServer, session: SessionId) {
    let now = ts.world.now;
    ts.world.net.terminate(
        session,
        SessionTerminationReason::NetworkTimeout,
        None,
        String::new(),
        now,
    );
    assert!(
        ts.run_until(5.0, |ts| ts.world.net.session(session).is_none()),
        "the session drops"
    );
    ts.step();
}

#[test]
fn a_good_login_reaches_the_character_list_with_the_accounts_characters() {
    let mut ts = seeded();
    assert_eq!(player_manager::get_offline_count(&ts.world), 2);
    let id = login(&mut ts, "acct", "secret");

    // SendConnectResponse: the character list, last played first, then the server name and the
    // DDD interrogation.
    let set = ts.received::<LoginCharacterSet>(id);
    assert_eq!(set.len(), 1);
    assert_eq!(
        names(&set[0]),
        vec![
            (BRAVO, "Bravo".to_owned(), 0),
            (ALPHA, "Alpha".to_owned(), 0)
        ]
    );
    assert!(set[0].deleted.is_empty());
    assert_eq!(set[0].account, "acct");
    assert_eq!(set[0].has_throne_of_destiny, 1);
    assert_eq!(
        ts.received::<LoginWorldInfo>(id),
        vec![LoginWorldInfo {
            connections: 0,
            max_connections: 128,
            world_name: "Empyrean".to_owned()
        }]
    );
    let ddd = ts.received::<DddInterrogation>(id);
    assert_eq!(ddd.len(), 1);
    assert_eq!(
        (
            ddd[0].servers_region,
            ddd[0].name_rule_language,
            ddd[0].product_id
        ),
        (1, 1, 1)
    );
    assert_eq!(ddd[0].supported_languages, vec![0, 1]);
    let opcodes: Vec<u32> = ts
        .received_raw(id)
        .iter()
        .map(|m| m.opcode)
        .filter(|&o| o == 0xF658 || o == 0xF7E1)
        .collect();
    assert_eq!(
        opcodes,
        vec![0xF658, 0xF7E1],
        "the list is queued before the server name"
    );

    // Both halves of the session agree; the account's last login is recorded.
    let session = session_of(&ts, "acct");
    assert_eq!(
        ts.world.net.session(session).map(|s| s.core.state),
        Some(SessionState::AuthConnected)
    );
    let s = ts.world.sessions.get(session).expect("game half");
    assert_eq!(
        (s.state, s.account.as_deref(), s.access_level),
        (
            SessionState::AuthConnected,
            Some("acct"),
            AccessLevel::Player
        )
    );
    assert_eq!(
        s.characters.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![BRAVO, ALPHA]
    );
    let account = ts.auth().get_account_by_name("acct").expect("account");
    assert_eq!(account.total_times_logged_in, 1);
    assert!(account.last_login_time.is_some());
}

/// The character screen's welcome, which the server sends with no message configured, is one of
/// the messages dereth-protocol reads, and so is everything else the login sends.
#[test]
fn every_message_up_to_the_character_list_decodes_the_welcome_included() {
    let mut ts = seeded();
    let id = login(&mut ts, "acct", "secret");
    assert_eq!(
        ts.received::<LoginCharacterScreenMessage>(id)
            .iter()
            .map(LoginCharacterScreenMessage::shown)
            .collect::<Vec<_>>(),
        vec!["Welcome to Empyrean!".to_owned()]
    );
    let bad: Vec<_> = decode::undecoded(ts.received_raw(id))
        .into_iter()
        .map(|d| (format!("0x{:04X} {}", d.kind, d.name()), d.result))
        .collect();
    assert!(bad.is_empty(), "messages that do not decode: {bad:#?}");
}

#[test]
fn a_bad_password_is_booted_with_aces_message() {
    let mut ts = seeded();
    let id = ts.add_client("acct", "wrong");
    assert!(ts.run_until(1.0, |ts| !ts.received::<LoginAccountBooted>(id).is_empty()));
    assert_eq!(
        ts.received::<LoginAccountBooted>(id),
        vec![LoginAccountBooted {
            reason: Some(
                " because the password entered for this account was not correct".to_owned()
            )
        }]
    );
    assert!(ts.received::<LoginCharacterSet>(id).is_empty());
    // NotAuthorizedPasswordMismatch: flushed for 2 s, then dropped, and the account is free.
    assert!(ts.run_until(3.0, |ts| ts.world.net.get_session_count() == 0));
    assert!(ts.run_until(1.0, |ts| ts.world.sessions.is_empty()));
    assert_eq!(
        ts.auth()
            .get_account_by_name("acct")
            .map(|a| a.total_times_logged_in),
        Some(0)
    );
    login(&mut ts, "acct", "secret");
}

#[test]
fn an_unknown_account_is_created_when_the_config_allows_it() {
    let mut ts = server();
    let id = login(&mut ts, "NewAcct", "pw");
    // `CreateAccount(loginRequest.Account.ToLower(), ...)` at `DefaultAccessLevel`.
    let account = ts
        .auth()
        .get_account_by_name("newacct")
        .expect("auto-created");
    assert_eq!(
        (account.account_name.as_str(), account.access_level),
        ("newacct", 0)
    );
    let set = ts.received::<LoginCharacterSet>(id);
    assert!(set[0].characters.is_empty());
    assert_eq!(set[0].account, "newacct");
}

#[test]
fn an_unknown_account_is_refused_when_auto_create_is_off_or_the_password_is_empty() {
    let mut ts = server();
    let clock: Arc<dyn Clock> = Arc::<empyrean_common::clock::VirtualClock>::clone(&ts.clock);
    let off = AccountDefaults {
        allow_auto_account_creation: false,
        ..AccountDefaults::default()
    };
    ts.world.auth = AuthHandle::new(Box::new(MemAuth::new(off, clock)));

    let id = ts.add_client("stranger", "pw");
    assert!(ts.run_until(1.0, |ts| !errors(ts, id).is_empty()));
    assert_eq!(errors(&ts, id), vec![ACCOUNT_DOESNT_EXIST]);
    assert!(ts.auth().get_account_by_name("stranger").is_none());

    // With auto-creation on, an empty password still creates nothing.
    let mut ts = server();
    let id = ts.add_client("stranger", "");
    assert!(ts.run_until(1.0, |ts| !errors(ts, id).is_empty()));
    assert_eq!(errors(&ts, id), vec![ACCOUNT_DOESNT_EXIST]);
    assert!(ts.auth().get_account_by_name("stranger").is_none());
}

#[test]
fn a_second_login_boots_the_first_and_is_refused_then_the_account_is_free() {
    let mut ts = seeded();
    let first = login(&mut ts, "acct", "secret");
    let first_session = session_of(&ts, "acct");

    // `account_login_boots_in_use` (true): the old session is booted (AccountLoggedIn) and the new
    // one refused (AccountInUse), both with CharacterError Logon.
    let second = ts.add_client("acct", "secret");
    assert!(ts.run_until(1.0, |ts| !errors(ts, second).is_empty()));
    assert_eq!(errors(&ts, second), vec![LOGON]);
    assert!(ts.run_until(1.0, |ts| !errors(ts, first).is_empty()));
    assert_eq!(errors(&ts, first), vec![LOGON]);
    assert_eq!(
        ts.world.net.session(first_session).and_then(|s| s
            .core
            .pending_termination
            .as_ref()
            .map(|p| p.reason)),
        Some(SessionTerminationReason::AccountLoggedIn)
    );

    // Both drop after their 2 s flush; a retry then gets in.
    assert!(ts.run_until(3.0, |ts| ts.world.net.get_session_count() == 0));
    let third = login(&mut ts, "acct", "secret");
    assert_eq!(names(&ts.received::<LoginCharacterSet>(third)[0]).len(), 2);
}

#[test]
fn disconnecting_at_any_stage_releases_the_account() {
    for stage in ["logging in", "character select", "in world"] {
        let mut ts = seeded();
        let id = ts.add_client("acct", "secret");
        match stage {
            "logging in" => {
                // DoLogin has accepted the account; the ConnectResponse has not arrived yet.
                assert!(ts.run_until(1.0, |ts| ts
                    .world
                    .net
                    .sessions()
                    .any(|s| s.core.state == SessionState::AuthConnectResponse)));
            }
            "character select" => {
                assert!(ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()));
            }
            _ => {
                assert!(ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()));
                let session = session_of(&ts, "acct");
                sessions::set_state(&mut ts.world, session, SessionState::WorldConnected);
                ts.world
                    .sessions
                    .get_mut(session)
                    .unwrap()
                    .set_player(Some(ObjectGuid::new(ALPHA)));
            }
        }
        let session = session_of(&ts, "acct");
        drop_session(&mut ts, session);

        assert!(
            ts.world.net.find_by_account("acct").is_none(),
            "{stage}: the account is released"
        );
        ts.step();
        assert!(
            ts.world.sessions.get(session).is_none(),
            "{stage}: the game half is gone"
        );
        let again = login(&mut ts, "acct", "secret");
        assert_eq!(
            names(&ts.received::<LoginCharacterSet>(again)[0]).len(),
            2,
            "{stage}: logs in again"
        );
    }
}

/// V304: a delete naming a slot outside the list (the
/// client's -1 when it cannot find the character) is refused with CharacterError Delete, as the
/// other refused deletes are, and nothing else is sent; ACE's handler threw and never answered.
#[test]
fn a_delete_of_a_slot_outside_the_list_is_refused() {
    let mut ts = seeded();
    let id = login(&mut ts, "acct", "secret");
    let _ = ts.take_received(id);
    ts.send_message(
        id,
        NetQueue::Logon,
        &CharacterDeleteRequest {
            account: "acct".to_owned(),
            slot_index: -1,
        },
    );
    ts.advance(0.1);
    let opcodes: Vec<u32> = ts.received_raw(id).iter().map(|m| m.opcode).collect();
    assert_eq!(
        opcodes,
        [<CharacterError as dereth_protocol::Message>::OPCODE.0],
        "only the refusal"
    );
    assert_eq!(errors(&ts, id), vec![DELETE]);
    assert!(ts
        .shard()
        .get_character_stub_by_guid(BRAVO)
        .is_some_and(|c| c.delete_time == 0));
    assert!(ts
        .shard()
        .get_character_stub_by_guid(ALPHA)
        .is_some_and(|c| c.delete_time == 0));
}

#[test]
fn delete_then_restore_then_the_deletion_timer() {
    let mut ts = seeded();
    let id = login(&mut ts, "acct", "secret");
    let delete_bravo = CharacterDeleteRequest {
        account: "acct".to_owned(),
        slot_index: 0,
    };

    // A delete naming another account is answered with CharacterError Delete.
    ts.send_message(
        id,
        NetQueue::Logon,
        &CharacterDeleteRequest {
            account: "other".to_owned(),
            slot_index: 0,
        },
    );
    ts.advance(0.1);
    assert_eq!(errors(&ts, id), vec![DELETE]);

    // Slot 0 is Bravo (last played first). The ack, then the list once the save is done: Bravo is
    // greyed out with the seconds left until it is deleted (V444), the hour of `char_delete_time`
    // less the moments since the delete; ACE sent 1.
    let before = ts.world.now.unix_time;
    ts.send_message(id, NetQueue::Logon, &delete_bravo);
    ts.advance(0.1);
    assert_eq!(ts.received::<CharacterDeleteAck>(id).len(), 1);
    let sets = ts.received::<LoginCharacterSet>(id);
    assert_eq!(sets.len(), 2);
    let listed = names(&sets[1]);
    assert_eq!(
        listed
            .iter()
            .map(|(gid, name, _)| (*gid, name.clone()))
            .collect::<Vec<_>>(),
        vec![(BRAVO, "Bravo".to_owned()), (ALPHA, "Alpha".to_owned())]
    );
    assert!(
        (3599..=3600).contains(&listed[0].2),
        "seconds left: {}",
        listed[0].2
    );
    assert_eq!(listed[1].2, 0, "Alpha is not pending deletion");
    let stub = ts.shard().get_character_stub_by_guid(BRAVO).expect("stub");
    #[allow(clippy::cast_precision_loss)]
    let delete_time = stub.delete_time as f64;
    // `(ulong)(Time.GetUnixTime() + 3600)`: truncated to the second.
    assert!(
        (before + 3599.0..before + 3601.0).contains(&delete_time) && delete_time.fract() == 0.0,
        "char_delete_time: {delete_time} vs {before}"
    );
    assert!(!stub.is_deleted);

    // Restore: the name is still free, so DeleteTime goes back to 0 and the client is told OK.
    ts.send_message(
        id,
        NetQueue::Control,
        &AdminSendAdminRestoreCharacter {
            iid: ObjectId(BRAVO),
            restored_char_name: String::new(),
            account_to_restore_to: String::new(),
        },
    );
    ts.advance(0.1);
    let restored = ts.received::<CharGenVerificationResponse>(id);
    assert_eq!(restored.len(), 1);
    assert_eq!(
        (restored[0].response_type, restored[0].identity.gid.0),
        (1, BRAVO)
    );
    assert_eq!(
        (
            restored[0].identity.name.as_str(),
            restored[0].identity.seconds_greyed_out
        ),
        ("Bravo", 0)
    );
    assert_eq!(
        ts.shard()
            .get_character_stub_by_guid(BRAVO)
            .map(|c| c.delete_time),
        Some(0)
    );

    // Delete again, and let the hour pass: the next login's UpdateCharacters deletes Bravo for good.
    // The hour is taken off the stored DeleteTime rather than run on the clock, because a
    // TestClient cannot join a server that has been up for more than ~130 s (see the receipt).
    ts.send_message(id, NetQueue::Logon, &delete_bravo);
    ts.advance(0.1);
    let session = session_of(&ts, "acct");
    drop_session(&mut ts, session);
    let mut stub = ts.shard().get_character_stub_by_guid(BRAVO).expect("stub");
    assert!(stub.delete_time > 0);
    stub.delete_time -= 3600;
    assert!(ts.shard().save_character(&stub));
    let id = login(&mut ts, "acct", "secret");
    assert_eq!(
        names(&ts.received::<LoginCharacterSet>(id)[0]),
        vec![(ALPHA, "Alpha".to_owned(), 0)]
    );
    let stub = ts
        .shard()
        .get_character_stub_by_guid(BRAVO)
        .expect("stub kept");
    assert!(
        stub.is_deleted,
        "CheckCharactersForDeletion saved IsDeleted"
    );
    assert!(
        player_manager::get_offline_player(&ts.world, BRAVO).is_none(),
        "ProcessDeletedPlayer"
    );
    assert_eq!(player_manager::find_by_name(&ts.world, "Bravo").0, None);
    assert!(
        player_manager::find_by_name(&ts.world, "alpha").0.is_some(),
        "names ignore case"
    );
}

#[test]
fn log_off_returns_to_character_select_after_aces_six_seconds() {
    let mut ts = seeded();
    let id = login(&mut ts, "acct", "secret");
    let session = session_of(&ts, "acct");

    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    assert_eq!(ts.received::<LoginEnterGameServerReady>(id).len(), 1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(ALPHA),
            account: "acct".to_owned(),
        },
    );
    ts.advance(0.1);
    assert_eq!(
        ts.world.net.session(session).map(|s| s.core.state),
        Some(SessionState::WorldConnected)
    );
    let s = ts.world.sessions.get(session).unwrap();
    assert_eq!(
        (s.state, s.player),
        (SessionState::WorldConnected, Some(ObjectGuid::new(ALPHA)))
    );

    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginExecuteLogOffRequest {
            character: ObjectId(ALPHA),
        },
    );
    let sent = ts.seconds();
    assert!(ts.run_until(10.0, |ts| !ts.received::<LoginExecuteLogOff>(id).is_empty()));
    let took = ts.seconds() - sent;
    // `logOffRequestTime.AddSeconds(6) <= DateTime.UtcNow` in seconds of the clock, never in ticks:
    // the request is handled one tick after it is sent, the log-off fires in the first tick at or
    // after 6 s, and its messages reach the client with the next DoSessionWork.
    let tick = TestServer::TICK.as_secs_f64();
    assert!(
        (6.0..6.0 + 4.0 * tick).contains(&took),
        "log-off took {took} s"
    );
    assert!(ts.run_until(0.2, |ts| ts.received::<LoginCharacterSet>(id).len() == 2));
    assert_eq!(names(&ts.received::<LoginCharacterSet>(id)[1]).len(), 2);
    assert_eq!(ts.received::<LoginWorldInfo>(id).len(), 2);

    let s = ts.world.sessions.get(session).unwrap();
    assert_eq!((s.state, s.player), (SessionState::AuthConnected, None));
    assert_eq!(
        ts.world.net.session(session).map(|s| s.core.state),
        Some(SessionState::AuthConnected)
    );
}
