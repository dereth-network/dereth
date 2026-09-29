//! ACE: Source/ACE.Server/WorldObjects/Player.cs::HandleActionTalk
//! Local speech heard in range only; tells reach targets, offline names answered, squelched
//! senders dropped; TalkDirect in landblock; General turbine chat reaches subscribers only.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_assets::tables::SkillFormula;
use dereth_primitives::NetQueue;
use dereth_primitives::ObjectId;
use dereth_protocol::comms::{
    CommunicationEmote, CommunicationHearDirectSpeech, CommunicationHearEmote,
    CommunicationHearSpeech, CommunicationModifyCharacterSquelch, CommunicationTalk,
    CommunicationTalkDirect, CommunicationTalkDirectByName, CommunicationTextboxString,
    CommunicationWeenieError, CommunicationWeenieErrorWithString,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::login::{LoginCharacterSet, LoginSendEnterWorld, LoginSendEnterWorldRequest};
use dereth_protocol::turbine::{decode_incoming, IncomingPayload, SendToRoomById};
use dereth_protocol::Message;
use empyrean_common::dotnet::DotNetDict;
use empyrean_content::models::world::Weenie;
use empyrean_content::MemContent;
use empyrean_dat::file_types::SecondaryAttributeTable;
use empyrean_dat::{file_id, FakeDats};
use empyrean_entity::enums::{
    AccessLevel, CharacterOption, ChatMessageType, PositionType, PropertyAttribute,
    PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyString, SquelchMask, WeenieError,
    WeenieErrorWithString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, ClientId, ClientStatus, TestServer};
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager;
use empyrean_world::world_objects::player_location;

const HOME: u16 = 0xA9B4;
const FAR: u16 = 0x2B2C;
const BLOCKS: [u16; 10] = [
    0xA8B3, 0xA8B4, 0xA8B5, 0xA9B3, 0xA9B4, 0xA9B5, 0xAAB3, 0xAAB4, 0xAAB5, FAR,
];

const ALPHA: u32 = 0x5000_0001;
const BRAVO: u32 = 0x5000_0002;
const CHARLIE: u32 = 0x5000_0003;
const DELTA: u32 = 0x5000_0004;

const GAME_EVENT: u32 = 0xF7B0;
const CHAT: u32 = 0xF7E0;
const HEAR_SPEECH: u32 = 0x02BB;
const HEAR_EMOTE: u32 = 0x01E0;
const TELL: u32 = 0x02BD;
const WEENIE_ERROR: u32 = 0x028A;
const WEENIE_ERROR_STR: u32 = 0x028B;
const SET_SQUELCH_DB: u32 = 0x01F4;
const TURBINE_CHAT: u32 = 0xF7DE;

fn outdoor(lb: u16, x: f32, y: f32, z: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = (x / 24.0) as u32 * 8 + (y / 24.0) as u32 + 1;
    Position::from_components(
        u32::from(lb) << 16 | cell,
        x,
        y,
        z,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )
}

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

/// `CharacterOptions2` bits for the given options.
fn options2(options: &[CharacterOption]) -> i32 {
    options
        .iter()
        .map(|o| {
            o.character_options2()
                .expect("a CharacterOptions2 option")
                .0
        })
        .fold(0u32, |a, b| a | b)
        .cast_signed()
}

/// An account with one character standing at `at`, with the given `CharacterOptions2`.
fn seed(
    ts: &TestServer,
    account: &str,
    guid: u32,
    name: &str,
    at: Position,
    character_options_2: i32,
) {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
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
        id: guid,
        account_id,
        name: name.to_owned(),
        character_options_2,
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
}

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

/// Alpha (listens to no global room) and Bravo (listens to General) 4 m apart at `HOME`; Charlie
/// (listens to General and Trade) in the far landblock; Delta (offline) anywhere.
fn seeded() -> TestServer {
    let mut ts = TestServer::with_dats(dats());
    land::use_flat_land_with_test_setup(&mut ts.world, &BLOCKS, 10);
    ts.world.content = Arc::new(MemContent::new().weenie(
        Weenie::new(1, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ),
    ));
    guid_manager::initialize(&mut ts.world, &mut EmptyShard);
    seed(
        &ts,
        "alpha",
        ALPHA,
        "Alpha",
        outdoor(HOME, 100.0, 100.0, 20.0),
        0,
    );
    seed(
        &ts,
        "bravo",
        BRAVO,
        "Bravo",
        outdoor(HOME, 104.0, 100.0, 20.0),
        options2(&[CharacterOption::ListenToGeneralChat]),
    );
    seed(
        &ts,
        "charlie",
        CHARLIE,
        "Charlie",
        outdoor(FAR, 100.0, 100.0, 20.0),
        options2(&[
            CharacterOption::ListenToGeneralChat,
            CharacterOption::ListenToTradeChat,
        ]),
    );
    seed(
        &ts,
        "delta",
        DELTA,
        "Delta",
        outdoor(HOME, 50.0, 50.0, 20.0),
        0,
    );
    player_manager::initialize(&mut ts.world);
    ts
}

fn enter(ts: &mut TestServer, account: &str, guid: u32) -> ClientId {
    let id = ts.connect(account, "pw");
    assert_eq!(
        ts.client(id).status(),
        ClientStatus::Connected,
        "{account} logs in"
    );
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
            character: ObjectId(guid),
            account: account.to_owned(),
        },
    );
    let g = ObjectGuid::new(guid);
    assert!(
        ts.run_until(1.0, |ts| ts
            .world
            .objects
            .get(g)
            .is_some_and(|o| o.current_landblock.is_some())),
        "{account} enters the world"
    );
    player_location::on_teleport_complete(&mut ts.world, g);
    ts.advance(0.5);
    let _ = TestServer::take_not_ported();
    id
}

/// A received message: its kind (the game-event type inside a `0xF7B0`, else the opcode) and blob.
#[derive(Debug, Clone)]
struct Got {
    kind: u32,
    blob: Vec<u8>,
}

impl Got {
    fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
}

/// What `id` received from index `from` on.
fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == GAME_EVENT {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .collect()
}

/// The chat kinds only (the players' own heartbeat, motion and position updates are left out).
const CHAT_KINDS: [u32; 8] = [
    CHAT,
    HEAR_SPEECH,
    HEAR_EMOTE,
    TELL,
    WEENIE_ERROR,
    WEENIE_ERROR_STR,
    SET_SQUELCH_DB,
    TURBINE_CHAT,
];

fn chat_since(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    got(ts, id, from)
        .into_iter()
        .filter(|g| CHAT_KINDS.contains(&g.kind))
        .collect()
}

fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

struct Three {
    ts: TestServer,
    a: ClientId,
    b: ClientId,
    c: ClientId,
}

fn three() -> Three {
    let mut ts = seeded();
    let a = enter(&mut ts, "alpha", ALPHA);
    let b = enter(&mut ts, "bravo", BRAVO);
    let c = enter(&mut ts, "charlie", CHARLIE);
    ts.advance(1.0);
    Three { ts, a, b, c }
}

fn marks(t: &Three) -> [usize; 3] {
    [
        t.ts.received_raw(t.a).len(),
        t.ts.received_raw(t.b).len(),
        t.ts.received_raw(t.c).len(),
    ]
}

// ------------------------------------------------------------------ scenarios

/// Local speech (`HandleActionTalk`): a HearSpeech to the speaker and to every player in
/// `LocalBroadcastRange` who knows it; Charlie, a landblock away, hears nothing. An emote goes the
/// same way as a HearEmote.
#[test]
fn local_speech_is_heard_in_range_and_not_out_of_it() {
    let mut t = three();
    let [ma, mb, mc] = marks(&t);
    t.ts.send_game_action(
        t.a,
        &CommunicationTalk {
            message: "hello, Dereth".to_owned(),
        },
    );
    t.ts.advance(0.3);

    for (id, mark) in [(t.a, ma), (t.b, mb)] {
        let g = chat_since(&t.ts, id, mark);
        assert_eq!(kinds(&g), [HEAR_SPEECH]);
        let s: CommunicationHearSpeech = g[0].decode();
        assert_eq!(
            (
                s.message.as_str(),
                s.sender_name.as_str(),
                s.sender_id.0,
                s.text_type
            ),
            ("hello, Dereth", "Alpha", ALPHA, ChatMessageType::Speech.0)
        );
    }
    assert!(chat_since(&t.ts, t.c, mc).is_empty(), "out of range");

    let [ma, mb, mc] = marks(&t);
    t.ts.send_game_action(
        t.b,
        &CommunicationEmote {
            message: "waves.".to_owned(),
        },
    );
    t.ts.advance(0.3);
    for (id, mark) in [(t.a, ma), (t.b, mb)] {
        let g = chat_since(&t.ts, id, mark);
        assert_eq!(kinds(&g), [HEAR_EMOTE]);
        let e: CommunicationHearEmote = g[0].decode();
        assert_eq!(
            (e.sender.0, e.sender_name.as_str(), e.text.as_str()),
            (BRAVO, "Bravo", "waves.")
        );
    }
    assert!(chat_since(&t.ts, t.c, mc).is_empty());
}

/// Tells (`GameActionTell`): the echo to the sender and the tell to the target, across
/// landblocks; an offline name answers `CharacterNotAvailable`; once Charlie squelches Alpha's
/// tells, Alpha gets the echo and `MessageBlocked_`, and Charlie nothing; Charlie's local speech
/// squelch then drops Alpha's speech too, while Bravo still hears it.
#[test]
fn tells_reach_their_target_offline_names_answer_and_squelched_senders_are_dropped() {
    let mut t = three();
    let [ma, _, mc] = marks(&t);
    t.ts.send_game_action(
        t.a,
        &CommunicationTalkDirectByName {
            message: "meet at the well".to_owned(),
            target_name: "charlie".to_owned(),
        },
    );
    t.ts.advance(0.3);
    let ga = chat_since(&t.ts, t.a, ma);
    assert_eq!(kinds(&ga), [CHAT]);
    let echo: CommunicationTextboxString = ga[0].decode();
    assert_eq!(
        (echo.text.as_str(), echo.text_type),
        (
            "You tell Charlie, \"meet at the well\"",
            ChatMessageType::OutgoingTell.0
        )
    );
    let gc = chat_since(&t.ts, t.c, mc);
    assert_eq!(kinds(&gc), [TELL]);
    let tell: CommunicationHearDirectSpeech = gc[0].decode();
    assert_eq!(
        (
            tell.message.as_str(),
            tell.sender_name.as_str(),
            tell.sender_id.0,
            tell.target_id.0,
            tell.text_type
        ),
        (
            "meet at the well",
            "Alpha",
            ALPHA,
            CHARLIE,
            ChatMessageType::Tell.0
        )
    );

    // Delta has a character but is offline
    let [ma, ..] = marks(&t);
    t.ts.send_game_action(
        t.a,
        &CommunicationTalkDirectByName {
            message: "there?".to_owned(),
            target_name: "Delta".to_owned(),
        },
    );
    t.ts.advance(0.3);
    let ga = chat_since(&t.ts, t.a, ma);
    assert_eq!(kinds(&ga), [WEENIE_ERROR]);
    let e: CommunicationWeenieError = ga[0].decode();
    assert_eq!(
        e.error_type,
        u32::try_from(WeenieError::CharacterNotAvailable.0).unwrap()
    );

    // Charlie squelches Alpha's tells
    let [_, _, mc] = marks(&t);
    t.ts.send_game_action(
        t.c,
        &CommunicationModifyCharacterSquelch {
            add: 1,
            character_id: ObjectId(0),
            character_name: "Alpha".to_owned(),
            msg_type: ChatMessageType::Tell.0,
        },
    );
    t.ts.advance(0.3);
    let gc = chat_since(&t.ts, t.c, mc);
    assert_eq!(kinds(&gc), [CHAT, SET_SQUELCH_DB]);
    let note: CommunicationTextboxString = gc[0].decode();
    assert_eq!(note.text, "Alpha has been squelched on the Tell channel.");

    let [ma, _, mc] = marks(&t);
    t.ts.send_game_action(
        t.a,
        &CommunicationTalkDirectByName {
            message: "hello?".to_owned(),
            target_name: "Charlie".to_owned(),
        },
    );
    t.ts.advance(0.3);
    let ga = chat_since(&t.ts, t.a, ma);
    assert_eq!(
        kinds(&ga),
        [CHAT],
        "the echo and no squelch notice (V264/V265/V266/V284)"
    );
    assert!(
        chat_since(&t.ts, t.c, mc).is_empty(),
        "the squelched tell is dropped"
    );

    // the squelch survives in the Character (saved with it)
    let o = t.ts.world.objects.get(ObjectGuid::new(CHARLIE)).unwrap();
    let rows = &o
        .player
        .as_ref()
        .unwrap()
        .player
        .character
        .as_ref()
        .unwrap()
        .character_properties_squelch;
    assert_eq!(
        rows.iter()
            .map(|r| (r.squelch_character_id, r.squelch_account_id, r.r#type))
            .collect::<Vec<_>>(),
        [(ALPHA, 0, SquelchMask::Tell.0)]
    );

    // Bravo squelches Alpha's speech: Alpha still hears itself, Bravo does not
    t.ts.send_game_action(
        t.b,
        &CommunicationModifyCharacterSquelch {
            add: 1,
            character_id: ObjectId(ALPHA),
            character_name: String::new(),
            msg_type: ChatMessageType::Speech.0,
        },
    );
    t.ts.advance(0.3);
    let [ma, mb, _] = marks(&t);
    t.ts.send_game_action(
        t.a,
        &CommunicationTalk {
            message: "anyone?".to_owned(),
        },
    );
    t.ts.advance(0.3);
    assert_eq!(kinds(&chat_since(&t.ts, t.a, ma)), [HEAR_SPEECH]);
    assert!(
        chat_since(&t.ts, t.b, mb).is_empty(),
        "Bravo squelches Alpha's speech"
    );
}

/// TalkDirect (`GameActionTalkDirect`) to a player in the landblock: the echo, then the tell.
#[test]
fn talk_direct_reaches_a_player_in_the_landblock() {
    let mut t = three();
    let [_, mb, _] = marks(&t);
    t.ts.send_game_action(
        t.b,
        &CommunicationTalkDirect {
            message: "psst".to_owned(),
            target: ObjectId(ALPHA),
        },
    );
    t.ts.advance(0.3);
    let gb = chat_since(&t.ts, t.b, mb);
    assert_eq!(kinds(&gb), [CHAT]);
    let echo: CommunicationTextboxString = gb[0].decode();
    assert_eq!(echo.text, "You tell Alpha, \"psst\"");
    let ga: Vec<Got> = chat_since(&t.ts, t.a, 0)
        .into_iter()
        .filter(|g| g.kind == TELL)
        .collect();
    let tell: CommunicationHearDirectSpeech = ga.last().expect("the tell").decode();
    assert_eq!(
        (
            tell.message.as_str(),
            tell.sender_name.as_str(),
            tell.sender_id.0
        ),
        ("psst", "Bravo", BRAVO)
    );
}

/// TurbineChat: at login each listener joins General ("You have entered the General channel.");
/// a General message reaches the subscribed players (Bravo, Charlie, in any landblock) and not
/// the others (Alpha, the sender, gets only its response).
#[test]
fn a_general_turbine_chat_message_reaches_the_subscribed_players_and_not_the_others() {
    let mut t = three();

    let entered = |ts: &TestServer, id: ClientId| {
        got(ts, id, 0)
            .iter()
            .filter(|g| g.kind == WEENIE_ERROR_STR)
            .map(|g| g.decode::<CommunicationWeenieErrorWithString>())
            .filter(|e| {
                e.error_type
                    == u32::try_from(WeenieErrorWithString::YouHaveEnteredThe_Channel.0).unwrap()
            })
            .map(|e| e.text)
            .collect::<Vec<_>>()
    };
    assert!(entered(&t.ts, t.a).is_empty());
    assert_eq!(entered(&t.ts, t.b), ["General"]);
    assert_eq!(entered(&t.ts, t.c), ["General", "Trade"]);

    let [ma, mb, mc] = marks(&t);
    let m = SendToRoomById {
        context: 5,
        room: 2,
        text: "LF a sparring partner".to_owned(),
        sender: ALPHA,
        chat_type: 2,
    };
    t.ts.send_message(t.a, NetQueue::Logon, &m);
    t.ts.advance(0.3);

    for (id, mark) in [(t.b, mb), (t.c, mc)] {
        let g = chat_since(&t.ts, id, mark);
        assert_eq!(kinds(&g), [TURBINE_CHAT]);
        let packet = decode_incoming(&g[0].blob[4..]).expect("a TurbineChat blob");
        assert!(
            !packet.ace_overstated_extent,
            "V239: both extents are exact, as retail's"
        );
        match packet.payload {
            IncomingPayload::RoomEvent(e) => assert_eq!(
                (e.room, e.name.as_str(), e.text.as_str()),
                (2, "Alpha", "LF a sparring partner")
            ),
            other => panic!("{other:?}"),
        }
    }
    let ga = chat_since(&t.ts, t.a, ma);
    assert_eq!(kinds(&ga), [TURBINE_CHAT]);
    match decode_incoming(&ga[0].blob[4..])
        .expect("a TurbineChat blob")
        .payload
    {
        IncomingPayload::RoomResponse(r) => assert_eq!(r.context, 5),
        other => panic!("{other:?}"),
    }

    // Trade: only Charlie listens
    let [_, mb, mc] = marks(&t);
    let m = SendToRoomById {
        context: 6,
        room: 3,
        text: "WTS a fine gem".to_owned(),
        sender: ALPHA,
        chat_type: 3,
    };
    t.ts.send_message(t.a, NetQueue::Logon, &m);
    t.ts.advance(0.3);
    assert!(chat_since(&t.ts, t.b, mb).is_empty());
    assert_eq!(kinds(&chat_since(&t.ts, t.c, mc)), [TURBINE_CHAT]);
}
