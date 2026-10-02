//! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
//! Enter world sends ACE's messages in order; possessions in inventory order; blocked saved
//! position -> lifestone; no-log landblock and no-location fallbacks; age every 4-6 s; first-
//! login gift once.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use empyrean_common::era::EraExt as _;
use std::sync::Arc;

use dereth_primitives::ObjectId;
use dereth_primitives::{IncomingMessage, NetQueue};
use dereth_protocol::login::{
    CharGenVerificationResponse, LoginEnterGameServerReady, LoginSendEnterWorld,
    LoginSendEnterWorldRequest,
};
use dereth_protocol::{self as proto, Message};
use empyrean_content::models::world::{LandblockInstance, Weenie};
use empyrean_content::MemContent;
use empyrean_entity::enums::{
    AccessLevel, PropertyBool, PropertyDataId, PropertyInt, PropertyString, WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState};
use empyrean_testkit::land;
use empyrean_testkit::{ClientId, TestServer};
use empyrean_world::managers::guid_manager;
use empyrean_world::managers::world_manager::WorldStatusState;
use empyrean_world::physics::phys_ext;

const ACCOUNT: &str = "acct";
/// The character the bot creates: the first player guid.
const PLAYER: u32 = 0x5000_0001;
/// The sample CharGen's (heritage 1, sex 1) body.
const PLAYER_SETUP: u32 = 0x0200_0001;
/// The start landblock (the sample CharGen's Holtburg cell `0xA9B40019` at z = 94.005).
const START: u16 = 0xA9B4;
/// Flat land at height-table index 47: 94 m.
const HEIGHT: u8 = 47;

/// Nearby synthetic objects: two signs and a server-only marker in the start landblock.
const SIGN_1: u32 = 0x7A9B_4001;
const SIGN_2: u32 = 0x7A9B_4002;
const HIDDEN: u32 = 0x7A9B_4003;

// The sample CharGen (no clothing, so the character has no possessions) with the tables chargen
// reads.

fn dats() -> Arc<empyrean_dat::DatManager> {
    crate::support::login_fixture::dats(&[], false)
}

fn content() -> MemContent {
    let sign = |wcid: u32, name: &str| {
        Weenie::new(wcid, name, WeenieType::Generic)
            .with_string(PropertyString::Name, name)
            .with_did(PropertyDataId::Setup, land::TEST_SETUP)
            .with_int(PropertyInt::ItemType, 0x80)
    };
    MemContent::new()
        .weenie(
            Weenie::new(1, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(sign(50, "Sign"))
        .weenie(sign(51, "Marker").with_bool(PropertyBool::Visibility, true))
        .landblock_instance(LandblockInstance::new(
            SIGN_1,
            50,
            u32::from(START) << 16 | 0x0019,
            [90.0, 10.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            SIGN_2,
            50,
            u32::from(START) << 16 | 0x0021,
            [120.0, 20.0, 94.0],
        ))
        .landblock_instance(LandblockInstance::new(
            HIDDEN,
            51,
            u32::from(START) << 16 | 0x0019,
            [80.0, 12.0, 94.0],
        ))
}

/// The start landblock and its eight neighbours (`AddObject(player, true)` loads the adjacents).
fn blocks() -> Vec<u16> {
    let mut v = Vec::new();
    for dx in [-1i32, 0, 1] {
        for dy in [-1i32, 0, 1] {
            let x = u16::try_from(i32::from(START >> 8) + dx).expect("on the map");
            let y = u16::try_from(i32::from(START & 0xFF) + dy).expect("on the map");
            v.push(x << 8 | y);
        }
    }
    v
}

/// A server with flat land, the synthetic content and an account; a bot logged in at character
/// select.
fn server() -> (TestServer, ClientId, SessionId) {
    let mut ts = TestServer::with_dats(crate::support::login_fixture::dats(&[], false));
    ts.world.content = Arc::new(content());
    ts.world.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    land::use_flat_land_with_setup(
        &mut ts.world,
        &blocks(),
        HEIGHT,
        PLAYER_SETUP,
        land::test_setup_geometry(),
    );
    phys_ext::register_setup(&mut ts.world, land::TEST_SETUP, land::test_setup_geometry());

    crate::support::login_fixture::connect_account(ts, ACCOUNT)
}

fn create(ts: &mut TestServer, id: ClientId, name: &str) {
    ts.send_message(
        id,
        NetQueue::Logon,
        &crate::support::login_fixture::request(ACCOUNT, name),
    );
    ts.advance(0.5);
    let r = ts.received::<CharGenVerificationResponse>(id);
    assert_eq!(
        r.iter().map(|r| r.response_type).collect::<Vec<_>>(),
        [1],
        "created"
    );
    assert_eq!(r[0].identity.gid.0, PLAYER);
}

/// Creates the character, then enters the world with it; returns where the messages that answer
/// `Login_SendEnterWorld` start in the client's inbox.
fn enter_world(ts: &mut TestServer, id: ClientId) -> usize {
    create(ts, id, "Aldric");
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    assert_eq!(ts.received::<LoginEnterGameServerReady>(id).len(), 1);
    let from = ts.received_raw(id).len();
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(PLAYER),
            account: ACCOUNT.to_owned(),
        },
    );
    ts.advance(0.5);
    from
}

/// A message's label as the capture comparison writes it: the opcode, or `ev XXXX` for a game
/// event (the event type follows the player guid and the sequence).
fn label(m: &IncomingMessage) -> String {
    if m.opcode == 0xF7B0 {
        let event = u32::from_le_bytes(m.body[8..12].try_into().expect("event type"));
        format!("ev {event:04X}")
    } else {
        format!("{:04X}", m.opcode)
    }
}

/// The messages from `from` on, in the server's send order (the blob sequence).
fn sent_order(ts: &TestServer, id: ClientId, from: usize) -> Vec<&IncomingMessage> {
    let mut v: Vec<&IncomingMessage> = ts.received_raw(id)[from..].iter().collect();
    v.sort_by_key(|m| m.blob_id.low32());
    v
}

fn guid_of(m: &IncomingMessage) -> u32 {
    u32::from_le_bytes(m.body[0..4].try_into().expect("guid"))
}

/// Decodes a message with dereth-protocol (its type chosen by opcode, or by event type after the
/// ordered-event header), failing on any it does not know or that does not decode.
fn decode(m: &IncomingMessage) {
    use dereth_protocol::{admin, comms, login, objects, qualities, social};
    fn dec<M: Message + std::fmt::Debug>(m: &IncomingMessage, body: &[u8]) {
        proto::read_body_padded::<M>(body)
            .unwrap_or_else(|e| panic!("{} does not decode: {e:?}", label(m)));
    }
    let event = &m.body[m.body.len().min(12)..];
    match label(m).as_str() {
        "F746" => dec::<objects::LoginCreatePlayer>(m, &m.body),
        "F745" => dec::<objects::ItemCreateObject>(m, &m.body),
        "F755" => dec::<objects::EffectsPlayScriptType>(m, &m.body),
        "F7E0" => dec::<comms::CommunicationTextboxString>(m, &m.body),
        "02CD" => dec::<qualities::QualitiesPrivateUpdateInt>(m, &m.body),
        "EA60" => dec::<admin::AdminEnvirons>(m, &m.body),
        "ev 0013" => dec::<login::LoginPlayerDescription>(m, event),
        "ev 0029" => dec::<social::CharacterTitlesMessage>(m, event),
        "ev 0021" => dec::<social::SocialFriendsUpdate>(m, event),
        "ev 0196" => dec::<objects::ItemOnViewContents>(m, event),
        "ev 028A" => dec::<comms::CommunicationWeenieError>(m, event),
        "ev 028B" => dec::<comms::CommunicationWeenieErrorWithString>(m, event),
        "ev 0295" => dec::<comms::ChatRoomMembership>(m, event),
        "ev 0004" => dec::<comms::CommunicationPopUpString>(m, event),
        other => panic!("no decoder for {other}"),
    }
}

#[test]
fn a_created_character_enters_the_world_with_aces_messages_in_aces_order() {
    let (mut ts, id, session) = server();
    let _ = TestServer::take_not_ported();
    let from = enter_world(&mut ts, id);

    let got = sent_order(&ts, id, from);
    let labels: Vec<String> = got.iter().map(|m| label(m)).collect();
    // UIQueue first (DoSessionWork flushes it before the Smartbox queue), each queue in ACE's
    // enqueue order:
    // SendSelf: PlayerDescription, CharacterTitle, FriendsListUpdate; TurbineChatIsEnabled and the
    // General, Trade and LFG channels (a new character's default options); DoPlayerEnterWorld: the
    // first-login popup and the welcome text (which names the server's source, V376). Then
    // PlayerCreate and the player's CreateObject (no possessions), then TrackObject's creates of the
    // two nearby signs (the server-only marker is not sent).
    // ACE's first-tick Age update is not here; it rides the player's first heartbeat, 0 to 5 s in.
    let expected = [
        "ev 0013", "ev 0029", "ev 0021", "ev 028A", "ev 028B", "ev 0295", "ev 028B", "ev 0295",
        "ev 028B", "ev 0295", "ev 0004", "F7E0", "F746", "F745", "F745", "F745",
    ];
    assert_eq!(labels, expected, "{labels:?}");
    for m in &got {
        decode(m);
    }
    let creates: Vec<u32> = got
        .iter()
        .filter(|m| m.opcode == 0xF745)
        .map(|m| guid_of(m))
        .collect();
    assert_eq!(creates[0], PLAYER);
    let mut nearby = creates[1..].to_vec();
    nearby.sort_unstable();
    assert_eq!(nearby, [SIGN_1, SIGN_2]);

    // The session is in the world with its player; the player is online, in its landblock and
    // its body is in a physics cell at the start position.
    let s = ts.world.sessions.get(session).expect("session");
    assert_eq!(
        (s.state, s.player),
        (SessionState::WorldConnected, Some(ObjectGuid::new(PLAYER)))
    );
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("the player is in the world");
    assert_eq!(
        p.current_landblock.map(|l| l.raw() >> 16),
        Some(u32::from(START))
    );
    let h = p.phys.expect("a body");
    let cell = phys_ext::cur_cell(&ts.world, h).expect("placed in a cell");
    assert_eq!(cell.0, 0xA9B4_0019);
    assert_eq!(
        empyrean_world::managers::player_manager::get_online_player(&ts.world, PLAYER),
        Some(ObjectGuid::new(PLAYER)),
        "online"
    );
}

/// V288 (the retail pcaps): the client's ask to describe an object (`0xF6EA`, Control queue)
/// comes back as a fresh CreateObject byte-identical to the one the login sent, for an object the
/// player knows; the server-only marker in the same landblock (known, never sent) and an id the
/// player never saw get nothing.
#[test]
fn asking_for_a_known_object_brings_its_create_back() {
    let (mut ts, id, _) = server();
    let from = enter_world(&mut ts, id);
    let first: Vec<u8> = sent_order(&ts, id, from)
        .into_iter()
        .find(|m| m.opcode == 0xF745 && guid_of(m) == SIGN_1)
        .expect("the sign was created")
        .body
        .clone();
    ts.advance(1.0);

    for (ask, answered) in [(SIGN_1, true), (HIDDEN, false), (0x7A9B_40FF, false)] {
        let before = ts.received_raw(id).len();
        ts.send_message(
            id,
            NetQueue::Control,
            &dereth_protocol::objects::ObjectSendForceObjdesc { id: ObjectId(ask) },
        );
        ts.advance(0.5);
        let answers: Vec<&IncomingMessage> = ts.received_raw(id)[before..]
            .iter()
            .filter(|m| matches!(m.opcode, 0xF745 | 0xF625) && guid_of(m) == ask)
            .collect();
        if answered {
            assert_eq!(answers.len(), 1, "{ask:08X}: one answer");
            assert_eq!(
                answers[0].opcode, 0xF745,
                "a CreateObject, not an appearance update"
            );
            assert_eq!(
                answers[0].body, first,
                "byte-identical to the login's create"
            );
        } else {
            assert!(answers.is_empty(), "{ask:08X}: no answer");
        }
    }
}

// ---- seeded characters -----------------------------------------------------------------------

/// A position row at `cell` (x, y, z), facing north.
fn pos(cell: u32, x: f32, y: f32, z: f32) -> empyrean_entity::models::PropertiesPosition {
    empyrean_entity::models::PropertiesPosition {
        obj_cell_id: cell,
        position_x: x,
        position_y: y,
        position_z: z,
        rotation_w: 1.0,
        ..Default::default()
    }
}

/// An item biota in `container` (or wielded by `wielder`).
fn item(
    guid: u32,
    weenie_type: WeenieType,
    container: Option<u32>,
    wielder: Option<u32>,
) -> empyrean_entity::Biota {
    let mut b = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 50,
        weenie_type,
        ..Default::default()
    };
    b.set_property(PropertyString::Name, format!("Item {guid:08X}"));
    b.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    b.set_property(PropertyInt::ItemType, 0x80);
    b.set_property(PropertyInt::EncumbranceVal, 10);
    if let Some(c) = container {
        b.set_property(empyrean_entity::enums::PropertyInstanceId::Container, c);
    }
    if let Some(wi) = wielder {
        b.set_property(empyrean_entity::enums::PropertyInstanceId::Wielder, wi);
        b.set_property(PropertyInt::CurrentWieldedLocation, 0x0000_0002); // ChestWear
        b.set_property(PropertyInt::ValidLocations, 0x0000_0002);
    }
    b
}

/// A character saved at `location` with a lifestone at `sanctuary`, `possessions` and
/// `total_logins`, seeded into the shard before the server starts (PlayerManager loads it).
fn seeded_server(
    location: Option<empyrean_entity::models::PropertiesPosition>,
    sanctuary: Option<empyrean_entity::models::PropertiesPosition>,
    mut possessions: Vec<empyrean_entity::Biota>,
    total_logins: i32,
) -> (TestServer, ClientId, SessionId) {
    let setup = move |w: &mut empyrean_world::World| {
        let account_id = w
            .auth
            .lock()
            .create_account(
                ACCOUNT,
                "pw",
                AccessLevel::Player,
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("created")
            .account_id;
        let mut biota = empyrean_entity::Biota {
            id: PLAYER,
            weenie_class_id: 1,
            weenie_type: WeenieType::Creature,
            ..Default::default()
        };
        biota.set_property(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        );
        biota.set_property(PropertyString::Name, "Aldric".to_owned());
        biota.set_property(PropertyDataId::Setup, PLAYER_SETUP);
        biota.set_property(PropertyInt::HeritageGroup, 1);
        if let Some(l) = location {
            biota.set_property_position(empyrean_entity::enums::PositionType::Location, l);
        }
        if let Some(s) = sanctuary {
            biota.set_property_position(empyrean_entity::enums::PositionType::Sanctuary, s);
        }
        let character = empyrean_store::models::shard::Character {
            id: PLAYER,
            account_id,
            name: "Aldric".to_owned(),
            total_logins,
            ..Default::default()
        };
        assert!(w.shard.base_database().add_character_in_parallel(
            &mut biota,
            &mut possessions,
            &character
        ));
    };
    let mut ts = TestServer::with_setup(dats(), setup);
    ts.world.content = Arc::new(content());
    ts.world.world_manager.world_status = WorldStatusState::Open;
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    land::use_flat_land_with_setup(
        &mut ts.world,
        &blocks(),
        HEIGHT,
        PLAYER_SETUP,
        land::test_setup_geometry(),
    );
    phys_ext::register_setup(&mut ts.world, land::TEST_SETUP, land::test_setup_geometry());
    let id = ts.connect(ACCOUNT, "pw");
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(s, _)| s)
        .next()
        .expect("session");
    assert!(ts.run_until(1.0, |ts| ts
        .world
        .sessions
        .get(session)
        .is_some_and(|s| s.state == SessionState::AuthConnected)));
    (ts, id, session)
}

/// Enters the world with the seeded character; the start of the answer in the inbox.
fn enter_seeded(ts: &mut TestServer, id: ClientId) -> usize {
    ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
    ts.advance(0.1);
    let from = ts.received_raw(id).len();
    ts.send_message(
        id,
        NetQueue::Logon,
        &LoginSendEnterWorld {
            character: ObjectId(PLAYER),
            account: ACCOUNT.to_owned(),
        },
    );
    ts.advance(0.5);
    from
}

fn chat_lines(ts: &TestServer, id: ClientId, from: usize) -> Vec<String> {
    ts.received_raw(id)[from..]
        .iter()
        .filter(|m| m.opcode == 0xF7E0)
        .map(|m| {
            proto::read_body_padded::<dereth_protocol::comms::CommunicationTextboxString>(&m.body)
                .expect("decodes")
                .text
        })
        .collect()
}

#[test]
fn possessions_are_created_in_aces_inventory_order_after_the_player() {
    // The shard returns the inventory by id, each container followed by its contents: A, P (a
    // pack) with C inside, D; and W wielded. SortWorldObjectsIntoInventory walks that list from the
    // end, so the player's Inventory is D, P, A; SendInventoryAndWieldedItems sends each, a pack
    // followed by its ViewContents (UI queue) and contents, then the wielded items.
    const A: u32 = 0x8000_0010;
    const P: u32 = 0x8000_0011;
    const C: u32 = 0x8000_0012;
    const D: u32 = 0x8000_0013;
    const W: u32 = 0x8000_0014;
    let possessions = vec![
        item(A, WeenieType::Generic, Some(PLAYER), None),
        item(P, WeenieType::Container, Some(PLAYER), None),
        item(C, WeenieType::Generic, Some(P), None),
        item(D, WeenieType::Generic, Some(PLAYER), None),
        item(W, WeenieType::Clothing, None, Some(PLAYER)),
    ];
    let (mut ts, id, _) = seeded_server(
        Some(pos(0xA9B4_0019, 84.0, 7.1, 94.005)),
        None,
        possessions,
        0,
    );
    let from = enter_seeded(&mut ts, id);
    let got = sent_order(&ts, id, from);
    for m in &got {
        decode(m);
    }
    let creates: Vec<u32> = got
        .iter()
        .filter(|m| m.opcode == 0xF745)
        .map(|m| guid_of(m))
        .take(6)
        .collect();
    assert_eq!(creates, [PLAYER, D, P, C, A, W]);
    let labels: Vec<String> = got.iter().map(|m| label(m)).collect();
    assert_eq!(
        &labels[..4],
        ["ev 0013", "ev 0029", "ev 0021", "ev 0196"],
        "{labels:?}"
    );

    // The player's burden is its items': A, D and the worn W at 10, plus the pack's total. The
    // pack's own burden is restored from its weenie by `Container(Biota)` (ACE's 2020-03-28 fix),
    // and this weenie has none: the pack weighs C's 10.
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("in the world");
    assert_eq!(p.encumbrance_val(), Some(40));
    assert_eq!(
        ts.world
            .objects
            .get(ObjectGuid::new(P))
            .and_then(|o| o.encumbrance_val()),
        Some(10)
    );
    // Placement positions renumbered: main pack items D, A (null placements, stable), side slot P.
    let placement = |g: u32| {
        ts.world
            .objects
            .get(ObjectGuid::new(g))
            .and_then(|o| o.placement_position())
    };
    assert_eq!(
        (placement(D), placement(A), placement(P), placement(C)),
        (Some(0), Some(1), Some(0), Some(0))
    );
}

#[test]
fn a_saved_position_no_body_can_occupy_is_moved_to_the_lifestone() {
    // The v1 lesson: a location the physics cannot place a body at (here a landblock with no land)
    // fails LandblockManager.AddObject; ACE logs, relocates to the lifestone, adds again, and
    // teleports there 5 s later.
    // The lifestone is not ACE's ultimate fallback cell, so the two cannot be confused.
    let lifestone = pos(0xA9B4_0021, 120.0, 20.0, 94.005);
    let (mut ts, id, _) = seeded_server(
        Some(pos(0x5368_0001, 10.0, 10.0, 0.0)),
        Some(lifestone),
        Vec::new(),
        1,
    );
    let _ = TestServer::take_not_ported();
    let from = enter_seeded(&mut ts, id);
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("in the world");
    let h = p.phys.expect("a body");
    assert_eq!(
        phys_ext::cur_cell(&ts.world, h).map(|c| c.0),
        Some(0xA9B4_0021),
        "placed at the lifestone"
    );
    assert_eq!(p.location().map(|l| l.cell()), Some(0xA9B4_0021));
    assert!(
        sent_order(&ts, id, from).iter().any(|m| m.opcode == 0xF746),
        "the login sequence was sent"
    );
    let before = ts.received_raw(id).len();
    ts.advance(5.0);
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("in the world");
    assert!(p.wo.world_object.teleporting, "the delayed teleport");
    assert!(
        ts.received_raw(id)[before..]
            .iter()
            .any(|m| m.opcode == 0xF751),
        "PlayerTeleport sent"
    );
}

#[test]
fn a_character_on_a_no_log_landblock_logs_in_at_its_lifestone() {
    let lifestone = pos(0xA9B4_0019, 84.0, 7.1, 94.005);
    // 0x0007: the Town Network
    let (mut ts, id, _) = seeded_server(
        Some(pos(0x0007_0120, 10.0, 10.0, 0.0)),
        Some(lifestone),
        Vec::new(),
        3,
    );
    let from = enter_seeded(&mut ts, id);
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("in the world");
    assert_eq!(p.location().map(|l| l.cell()), Some(0xA9B4_0019));
    let lines = chat_lines(&ts, id, from);
    assert_eq!(
        lines.last().map(String::as_str),
        Some("The currents of portal space cannot return you from whence you came. Your previous location forbids login.")
    );
    // Not a first login and no MOTD: no popup.
    assert!(!sent_order(&ts, id, from)
        .iter()
        .any(|m| label(m) == "ev 0004"));
}

#[test]
fn a_character_with_no_location_starts_at_aces_fallback() {
    let (mut ts, id, _) = seeded_server(None, None, Vec::new(), 0);
    let _ = enter_seeded(&mut ts, id);
    let p = ts
        .world
        .objects
        .get(ObjectGuid::new(PLAYER))
        .expect("in the world");
    // No Instantiation either: `new Position(0xA9B40019, 84, 7.1f, 94, ...)`, on flat land at 94 m.
    assert_eq!(p.location().map(|l| l.cell()), Some(0xA9B4_0019));
    assert!(p
        .phys
        .and_then(|h| phys_ext::cur_cell(&ts.world, h))
        .is_some());
}

/// V255/V277/V280/V281: the Age update comes every 4 to 6 s (uniform, drawn per update), not
/// ACE's 7 s; its value keeps ACE's accumulation. HB-1: it rides the player's
/// heartbeat, so the first comes 0 to 5 s after entering (retail's first Age update was uniform
/// over 0-5 s after the player's create), not on the first tick.
#[test]
fn the_age_is_sent_every_four_to_six_seconds_from_the_first_heartbeat() {
    let (mut ts, id, _) = seeded_server(
        Some(pos(0xA9B4_0019, 84.0, 7.1, 94.005)),
        None,
        Vec::new(),
        0,
    );
    let entered = ts.seconds();
    let from = enter_seeded(&mut ts, id);
    let ages = |ts: &TestServer| {
        ts.received_raw(id)[from..]
            .iter()
            .filter(|m| m.opcode == 0x02CD)
            .map(|m| {
                proto::read_body_padded::<dereth_protocol::qualities::QualitiesPrivateUpdateInt>(
                    &m.body,
                )
                .expect("decodes")
                .0
                .value
            })
            .collect::<Vec<_>>()
    };
    assert!(
        ts.run_until(6.0, |ts| ages(ts).len() == 1),
        "the first heartbeat's Age update"
    );
    assert_eq!(ages(&ts), [1], "Age ?? 1 at the first update");
    let mut last = ts.seconds();
    assert!(
        last - entered < 5.6,
        "the first update {} s in",
        last - entered
    );
    for n in 2..6 {
        assert!(ts.run_until(7.0, |ts| ages(ts).len() == n));
        let took = ts.seconds() - last;
        // read up to half a second after each update went out
        assert!(
            (3.4..6.6).contains(&took),
            "update {n} {took} s after the last"
        );
        last = ts.seconds();
    }
    let second = ages(&ts)[1];
    assert!(
        second > 1,
        "initial age + whole seconds since the first update: {second}"
    );
}

/// `Player.HandlePreOrderItems` (`PlayerEnterWorld`): under the default subscription level a first
/// login gets `W_GEMACTDPURCHASEREWARDARMOR_CLASS` in its pack through `TryCreatePreOrderItem` and
/// `ActdReceivedItems` is set; the next login finds the property set and adds nothing.
#[test]
fn a_first_login_receives_the_throne_of_destiny_gift_once() {
    const GIFT: u32 = 31000; // W_GEMACTDPURCHASEREWARDARMOR_CLASS
    let (mut ts, id, _) = server();
    ts.world.content = Arc::new(
        content().weenie(
            Weenie::new(GIFT, "Gift", WeenieType::Generic)
                .with_string(PropertyString::Name, "Gift")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        ),
    );
    let from = enter_world(&mut ts, id);

    let player = ObjectGuid::new(PLAYER);
    let gifts = |ts: &TestServer| {
        empyrean_world::world_objects::container::get_inventory_items_of_wcid(
            &ts.world, player, GIFT,
        )
    };
    assert_eq!(gifts(&ts).len(), 1, "the gift is in the pack");
    let p = ts.world.objects.get(player).expect("in the world");
    assert_eq!(p.get_property(PropertyBool::ActdReceivedItems), Some(true));
    // it is created on the client with the possessions
    let gift = gifts(&ts)[0].full();
    assert!(
        sent_order(&ts, id, from)
            .iter()
            .any(|m| m.opcode == 0xF745 && guid_of(m) == gift),
        "its CreateObject"
    );

    // TryCreatePreOrderItem again (a later login): the property is set, nothing is added
    empyrean_world::world_objects::player_networking::handle_pre_order_items(&mut ts.world, player);
    assert_eq!(gifts(&ts).len(), 1);
}

/// Divergence: V398
/// In an era before the Throne of Destiny gifts a first login receives none.
#[test]
fn a_first_login_in_an_era_without_the_gifts_receives_none() {
    const GIFT: u32 = 31000; // W_GEMACTDPURCHASEREWARDARMOR_CLASS
    let (mut ts, id, _) = server();
    ts.world.era = empyrean_common::era::EraId::Infiltration.rules();
    ts.world.content = Arc::new(
        content().weenie(
            Weenie::new(GIFT, "Gift", WeenieType::Generic)
                .with_string(PropertyString::Name, "Gift")
                .with_did(PropertyDataId::Setup, land::TEST_SETUP),
        ),
    );
    enter_world(&mut ts, id);
    let player = ObjectGuid::new(PLAYER);
    assert!(
        empyrean_world::world_objects::container::get_inventory_items_of_wcid(
            &ts.world, player, GIFT,
        )
        .is_empty()
    );
    let p = ts.world.objects.get(player).expect("in the world");
    assert_eq!(p.get_property(PropertyBool::ActdReceivedItems), None);
}

/// Divergence: V410
/// A first login opens the Welcome Letter in the pack (`Writing_BookOpen`, `0x00B4`) and shows no
/// training-hall popup (`Communication_PopUpString`, `0x0004`) in an era whose first login reads
/// the letter (the era's starter gear gives it); at the end of retail the popup, and no book.
#[test]
fn a_first_login_in_an_era_with_the_welcome_letter_opens_it() {
    const LETTER: u32 = 0x8000_0020;
    for (era, opened) in [
        (empyrean_common::era::EraId::Eor, false),
        (empyrean_common::era::EraId::Infiltration, true),
    ] {
        let mut letter = item(LETTER, WeenieType::Book, Some(PLAYER), None);
        letter.weenie_class_id = 1077;
        letter.properties_book = Some(empyrean_entity::models::PropertiesBook {
            max_num_pages: 1,
            max_num_chars_per_page: 1000,
        });
        let (mut ts, id, _) = seeded_server(
            Some(pos(0xA9B4_0019, 84.0, 7.1, 94.005)),
            None,
            vec![letter],
            0,
        );
        ts.world.era = era.rules();
        let from = enter_seeded(&mut ts, id);
        let labels: Vec<String> = sent_order(&ts, id, from).iter().map(|m| label(m)).collect();
        assert_eq!(
            labels.iter().any(|l| l == "ev 00B4"),
            opened,
            "{era}: {labels:?}"
        );
        assert_eq!(
            labels.iter().any(|l| l == "ev 0004"),
            !opened,
            "{era}: {labels:?}"
        );
    }
}

#[cfg(feature = "real-content")]
mod retail_gift_real {
    //! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionQueryItemMana
    use crate::support::inventory_action_world::real::*;

    /// `Player.HandlePreOrderItems` -> `TryCreatePreOrderItem` (Player_Networking.cs) at login, on
    /// ACE's default subscription level: the Throne of Destiny armor gem (wcid 31000) is placed in
    /// the pack once and `ActdReceivedItems` is set; the thank-you line waits for
    /// `show_first_login_gift`, false by default.
    #[test]
    fn a_new_character_receives_the_throne_of_destiny_gift() {
        let l = create_and_enter();
        let gems = l
            .inventory_wcids()
            .into_iter()
            .filter(|&w| w == 31000)
            .count();
        assert_eq!(gems, 1, "{:?}", l.inventory_wcids());
        let o = l.ts.world.objects.get(l.g).unwrap();
        assert_eq!(
            o.get_property(empyrean_entity::enums::PropertyBool::ActdReceivedItems),
            Some(true)
        );
        let chats: Vec<String> = l
            .since::<CommunicationTextboxString>(0)
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert!(
            !chats.iter().any(|c| c.contains("Throne of Destiny")),
            "{chats:?}"
        );
    }
}

pub(crate) use crate::support::empty_shard::EmptyShard;
