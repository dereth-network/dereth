//! ACE: Source/ACE.Server/Network/GameAction/Actions/GameActionPingRequest.cs::Handle
//! FriendsOld and GetServerVersion answer with a line; a speed-hack verdict logs the player off;
//! a data request with patching off warns then boots.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::admin::{DddError, DddRequestData};
use dereth_protocol::comms::CommunicationTextboxString;
use dereth_protocol::login::{
    LoginAccountBooted, LoginCharacterSet, LoginSendEnterWorld, LoginSendEnterWorldRequest,
};
use dereth_protocol::{self as proto, Message};
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, ClientStatus, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::{player_manager, property_manager, world_manager};
use empyrean_world::network::handlers::{
    ddd_handler, friends_old_handler, get_server_version_handler,
};
use empyrean_world::network::managers::inbound_message_manager::Payload;
use empyrean_world::world_objects::player_location;

const HOME: u16 = 0xA9B4;
const BLOCKS: [u16; 9] = [
    0xA8B3, 0xA8B4, 0xA8B5, 0xA9B3, 0xA9B4, 0xA9B5, 0xAAB3, 0xAAB4, 0xAAB5,
];
const ALPHA: u32 = 0x5000_0001;
const GAME_EVENT: u32 = 0xF7B0;
const POPUP_STRING: u32 = 0x0004;
const TRANSIENT_STRING: u32 = 0x02EB;
/// `ChatMessageType.Broadcast` and `WorldBroadcast`.
const BROADCAST: u32 = 0;
const WORLD_BROADCAST: u32 = 20;

fn dats() -> Arc<empyrean_dat::DatManager> {
    let f = |attr1: u32, z: u32| SkillFormula {
        w: 0,
        x: 1,
        y: 0,
        z,
        attr1,
        attr2: 0,
    };
    let table = SecondaryAttributeTable {
        id: dereth_primitives::DataId(file_id::SECONDARY_ATTRIBUTE_TABLE),
        health: f(2, 2),
        stamina: f(2, 1),
        mana: f(6, 1),
    };
    empyrean_testkit::dats::with_stat_tables(FakeDats::new())
        .with_portal(file_id::SECONDARY_ATTRIBUTE_TABLE, table)
        .build()
        .expect("fake dats")
}

use empyrean_testkit::EmptyShard;

/// Alpha, standing in the middle of `HOME`.
fn seeded() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    world_manager::open(&mut ts.world, None);
    land::use_flat_land_with_test_setup(&mut ts.world, &BLOCKS, 10);
    ts.world.content = Arc::new(MemContent::new().weenie(
        Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ),
    ));
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);

    let account_id = ts
        .auth()
        .create_account(
            "alpha",
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: ALPHA,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, "Alpha".to_owned());
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    let at = Position::from_components(
        u32::from(HOME) << 16 | 0x21,
        100.0,
        100.0,
        20.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    );
    let position = PropertiesPosition {
        obj_cell_id: at.cell(),
        position_x: at.position_x,
        position_y: at.position_y,
        position_z: at.position_z,
        rotation_w: at.rotation_w,
        rotation_x: at.rotation_x,
        rotation_y: at.rotation_y,
        rotation_z: at.rotation_z,
    };
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(PositionType::Location, position);
    let character = Character {
        id: ALPHA,
        account_id,
        name: "Alpha".to_owned(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
    player_manager::initialize(&mut ts.world);
    ts
}

/// Logs Alpha in and into the world; the client has not sent `LoginComplete`, so
/// `FirstEnterWorldDone` is still false.
fn enter(ts: &mut TestServer) -> (ClientId, SessionId) {
    let id = ts.connect("alpha", "pw");
    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list arrives"
    );
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(ALPHA),
            account: "alpha".to_owned(),
        },
    );
    let g = ObjectGuid::new(ALPHA);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "Alpha enters the world"
    );
    player_location::on_teleport_complete(&mut ts.world, g);
    ts.advance(0.5);
    let _ = TestServer::take_not_ported();
    (
        id,
        ts.world.net.find_by_account("alpha").expect("a session"),
    )
}

fn blob<M: Message>(m: &M) -> Vec<u8> {
    proto::write_blob(m).expect("encodable")
}

/// What the client received from index `from` on: the game-event type inside a `0xF7B0`, else the
/// opcode.
fn kinds(ts: &TestServer, id: ClientId, from: usize) -> Vec<u32> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            if m.opcode == GAME_EVENT {
                u32::from_le_bytes(m.body[8..12].try_into().expect("event"))
            } else {
                m.opcode
            }
        })
        .collect()
}

fn texts(ts: &TestServer, id: ClientId) -> Vec<(String, u32)> {
    ts.received::<CommunicationTextboxString>(id)
        .into_iter()
        .map(|t| (t.text, t.text_type))
        .collect()
}

/// `FriendsOld` answers with a broadcast line; `GetServerVersion` with the build information in a
/// world-broadcast line (`ServerBuildInfo.GetVersionInfo`, over the world database's version row).
#[test]
fn friends_old_and_server_version_answer_with_a_line() {
    let mut ts = seeded();
    let (id, session) = enter(&mut ts);
    // the world database's version row, which `ServerBuildInfo.GetVersionInfo` reports
    let version = empyrean_content::models::world::version::Version {
        id: 1,
        base_version: Some("v0.9.290".into()),
        patch_version: Some("v0.0.1".into()),
        last_modified: empyrean_common::dotnet::DotNetDateTime::new(2026, 9, 1),
    };
    ts.world.content = std::sync::Arc::new(empyrean_content::MemContent::new().version(version));
    friends_old_handler::friends_old(
        &mut ts.world,
        &mut Payload::new(&0xF7CDu32.to_le_bytes()),
        session,
    )
    .expect("handled");
    get_server_version_handler::get_server_version(
        &mut ts.world,
        &mut Payload::new(&0xF7CCu32.to_le_bytes()),
        session,
    )
    .expect("handled");
    ts.run_until(0.5, |_| false);
    let t = texts(&ts, id);
    assert!(
        t.contains(&(
            "That command is not used on this server.".to_owned(),
            BROADCAST
        )),
        "{t:?}"
    );
    // the login welcome names Empyrean, where its source is (AGPL; the build's repository when
    // `server.source_url` is unset) and our help command (brand); no separate source line follows
    let welcome = "Welcome to Dereth\n powered by Empyrean (a fork of ACEmulator)\n(https://github.com/dereth-network/dereth)\n\nFor more information on commands supported by this server, type @emphelp\n";
    assert!(t.contains(&(welcome.to_owned(), BROADCAST)), "{t:?}");
    assert!(
        !t.iter().any(|(l, _)| l.contains("its source:")),
        "the source is in the welcome, not a line of its own: {t:?}"
    );
    let expected = empyrean_common::server_build_info::get_version_info(
        Some("v0.9.290"),
        Some("v0.0.1"),
        empyrean_common::dotnet::DotNetDateTime::new(2026, 9, 1),
    );
    assert!(
        expected.starts_with("Server binaries version ") && expected.contains(" : Empyrean\n"),
        "{expected}"
    );
    assert!(expected.contains(
        "Server database version Base: v0.9.290 Patch: v0.0.1 - compiled Tue Sep 1 00:00:00 2026\n"
    ));
    assert!(
        expected.ends_with(" mode\nServer source: https://github.com/dereth-network/dereth\n"),
        "{expected}"
    );
    assert!(
        t.contains(&(expected, WORLD_BROADCAST)),
        "the version lines: {t:?}"
    );
}

/// The speed-hack verdict: the player is told "TimeSync: client speed error" and logged off.
#[test]
fn a_speed_hack_verdict_logs_the_player_off() {
    let mut ts = seeded();
    let (id, session) = enter(&mut ts);
    world_manager::network_session_verify_echo_speed_hack(&mut ts.world, session);
    ts.run_until(1.0, |_| false);
    assert!(texts(&ts, id).contains(&("TimeSync: client speed error".to_owned(), BROADCAST)));
    assert_ne!(
        ts.world
            .sessions
            .get(session)
            .expect("game half")
            .log_off_request_time,
        empyrean_common::dotnet::datetime::DotNetDateTime::MIN_VALUE,
        "LogOffPlayer ran"
    );
}

/// Patching off and `show_dat_warning` on: before the first `LoginComplete` the client cannot be
/// booted cleanly, so each request is answered with the warning as a popup, a world-broadcast line
/// and a transient string, then a DDD error (type 1); afterwards the request boots it.
#[test]
fn a_data_request_with_patching_off_warns_then_boots() {
    let mut ts = seeded();
    let (id, session) = enter(&mut ts);
    assert!(property_manager::modify_bool(
        &ts.world,
        "show_dat_warning",
        true
    ));
    let msg = property_manager::get_string(&ts.world, "dat_older_warning_msg", "", true).item;

    let from = ts.received_raw(id).len();
    let bytes = blob(&DddRequestData {
        resource_type: 3,
        resource_id: 0xA9B4_0100,
    });
    ddd_handler::ddd_request_data_message_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        false,
    )
    .expect("handled");
    ts.run_until(1.0, |_| false);
    let got: Vec<u32> = kinds(&ts, id, from)
        .into_iter()
        .filter(|k| [POPUP_STRING, TRANSIENT_STRING, 0xF7E0, 0xF7E4].contains(k))
        .collect();
    assert_eq!(got.len(), 4, "{got:?}");
    assert!(got.contains(&POPUP_STRING) && got.contains(&TRANSIENT_STRING));
    assert_eq!(
        ts.received::<DddError>(id),
        vec![DddError {
            resource_type: 3,
            resource_id: 0xA9B4_0100,
            error: 1
        }]
    );
    assert!(texts(&ts, id).contains(&(msg.clone(), WORLD_BROADCAST)));
    assert!(ts.received::<LoginAccountBooted>(id).is_empty());

    let player = ObjectGuid::new(ALPHA);
    ts.world
        .objects
        .get_mut(player)
        .expect("Alpha")
        .set_first_enter_world_done(true);
    ddd_handler::ddd_request_data_message_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        false,
    )
    .expect("handled");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginAccountBooted>(id).is_empty()),
        "booted"
    );
    assert_eq!(
        ts.received::<LoginAccountBooted>(id)[0].reason.as_deref(),
        Some(format!(" because {}", msg.trim_end_matches('.')).as_str())
    );
}

#[cfg(feature = "real-content")]
mod client_actions_real {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
    use crate::support::real_content_bot::real::*;

    /// Retail client actions are answered.
    #[test]
    fn retail_client_actions_are_answered() {
        use dereth_protocol::admin::{
            CharacterQueryAge, CharacterQueryAgeResponse, CharacterQueryBirth,
            CharacterRequestPing, CharacterReturnPing,
        };
        use dereth_protocol::login::CharacterPlayerOptionChangedEvent;
        use empyrean_entity::enums::CharacterOption;
        let mut l = create_and_enter();
        let mark = l.mark();
        l.action(&CharacterRequestPing);
        l.action(&CharacterQueryAge {
            target: ObjectId(0),
        });
        l.action(&CharacterQueryBirth {
            target: ObjectId(0),
        });
        l.action(&CharacterPlayerOptionChangedEvent {
            option: CharacterOption::AutoRepeatAttacks.0.cast_unsigned(),
            value: 1,
        });
        l.advance(0.5);
        assert_eq!(
            l.since::<CharacterReturnPing>(mark).len(),
            1,
            "the ping answer"
        );
        let age = l.since::<CharacterQueryAgeResponse>(mark);
        assert_eq!(age.len(), 1, "the age answer");
        assert_eq!(age[0].target_name, "");
        assert!(
            l.since::<CommunicationTextboxString>(mark)
                .iter()
                .any(|t| t.text.starts_with("You were born on ")),
            "the date of birth"
        );
        assert!(
            empyrean_world::world_objects::player_character::get_character_option(
                &l.ts.world,
                l.g,
                CharacterOption::AutoRepeatAttacks
            )
        );
        l.assert_all_decode(mark, "Monster health actions");
    }
}
