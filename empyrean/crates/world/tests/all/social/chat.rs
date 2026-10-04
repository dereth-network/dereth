//! Vectors: fixtures/vectors/chat/
//! Chat, channels, TurbineChat and squelch masks replay ACE squelch vectors and hand-read
//! TurbineChatHandler/SquelchManager behaviour.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use crate::social::fellowship;

use std::sync::Arc;
use std::time::Duration;

use dereth_primitives::ObjectId;
use dereth_protocol::actions::pack_action;
use dereth_protocol::comms::{
    CommunicationAddToChannel, CommunicationChannelBroadcast, CommunicationChannelBroadcastRecv,
    CommunicationChannelIndexRequest, CommunicationChannelListRequest, CommunicationEmote,
    CommunicationHearDirectSpeech, CommunicationModifyAccountSquelch,
    CommunicationModifyCharacterSquelch, CommunicationModifyGlobalSquelch,
    CommunicationRemoveFromChannel, CommunicationSetSquelchDb, CommunicationTalk,
    CommunicationTalkDirect, CommunicationTalkDirectByName, CommunicationTextboxString,
    CommunicationTransientString, CommunicationWeenieError, CommunicationWeenieErrorWithString,
};
use dereth_protocol::events::split_ui_blob;
use dereth_protocol::turbine::{decode_incoming, IncomingPayload, SendToRoomById};
use dereth_protocol::{write_blob, Message};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{CsCast, DotNetDict};
use empyrean_common::vectors::{self, u64_of};
use empyrean_content::models::world::Weenie as ContentWeenie;
use empyrean_content::MemContent;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, Channel, CharacterOption, ChatMessageType, PropertyBool, PropertyInt,
    PropertyString, SquelchMask, WeenieError, WeenieErrorWithString, WeenieType,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState};
use empyrean_store::models::auth::Account;
use empyrean_store::models::shard::Character;
use empyrean_store::MemShard;
use empyrean_world::dispatch::Class;
use empyrean_world::entity::i_player::IPlayer;
use empyrean_world::entity::offline_player::OfflinePlayer;
use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
use empyrean_world::managers::player_manager::{OnlinePlayer, OrdinalIgnoreCase};
use empyrean_world::managers::property_manager as pm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::managers::inbound_message_manager::{
    handle_client_message, run_inbound_message_queue,
};
use empyrean_world::network::structure::squelch_db::SquelchDB;
use empyrean_world::network::structure::squelch_info::SquelchInfo;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::managers::squelch_manager;
use empyrean_world::world_objects::player;
use empyrean_world::world_objects::world_object::CtorEnv;
use empyrean_world::World;

use empyrean_net::ClientMessage;

const A: u32 = 0x5000_0001;
const B: u32 = 0x5000_0002;
const C: u32 = 0x5000_0003;
const D: u32 = 0x5000_0004;
const SA: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
const SB: SessionId = SessionId {
    client_id: 2,
    generation: 1,
};
const SC: SessionId = SessionId {
    client_id: 3,
    generation: 1,
};

const PLAYER_WCID: u32 = 1;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
const GAME_EVENT: u32 = 0xF7B0;
const CHAT: u32 = 0xF7E0;
const TELL: u32 = 0x02BD;
const HEAR_SPEECH: u32 = 0x02BB;
const WEENIE_ERROR: u32 = 0x028A;
const WEENIE_ERROR_STR: u32 = 0x028B;
const TRANSIENT: u32 = 0x02EB;
const SET_SQUELCH_DB: u32 = 0x01F4;
const CHANNEL_BROADCAST: u32 = 0x0147;
const CHANNEL_LIST: u32 = 0x0148;
const CHANNEL_INDEX: u32 = 0x0149;
const TURBINE_CHAT: u32 = 0xF7DE;

struct EmptyShard;

impl ShardGuidQueries for EmptyShard {
    fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
        u32::MAX
    }
    fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
        Vec::new()
    }
}

fn account(account_id: u32, name: &str) -> Account {
    Account {
        account_id,
        account_name: name.to_owned(),
        ..Account::default()
    }
}

/// An online player: built from the weenie, with its Character, Account, session and
/// `PlayerManager` entries (as `PlayerEnterWorld` leaves them).
fn add_online(
    w: &mut World,
    guid: u32,
    name: &str,
    account_id: u32,
    account_name: &str,
    session: SessionId,
) {
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        player::player_from_weenie(
            env,
            Class::Player,
            weenie,
            ObjectGuid::new(guid),
            account_id,
        )
    });
    o.set_property(PropertyString::Name, name.to_owned());
    o.set_property(PropertyInt::Level, 1);
    let p = o.player.as_mut().expect("a player");
    p.player.character = Some(Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Default::default()
    });
    p.player.account = Some(account(account_id, account_name));
    w.objects.insert(o).expect("fresh");

    let mut s = SessionData {
        state: SessionState::WorldConnected,
        ..Default::default()
    };
    s.set_account(account_id, account_name.to_owned(), AccessLevel::Player);
    s.set_player(Some(ObjectGuid::new(guid)));
    w.sessions.insert(session, s);

    let g = ObjectGuid::new(guid);
    let pmgr = &mut w.player_manager;
    assert!(pmgr.online_players.try_add(
        guid,
        OnlinePlayer {
            guid: g,
            account: Some(account(account_id, account_name))
        }
    ));
    pmgr.player_names
        .insert(OrdinalIgnoreCase(name.to_owned()), IPlayer::Online(g));
    pmgr.player_accounts
        .get_or_insert_with(account_id, DotNetDict::new)
        .insert(guid, IPlayer::Online(g));
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    let mut w = World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    );
    w.content = Arc::new(MemContent::new().weenie(
        ContentWeenie::new(PLAYER_WCID, "human", WeenieType::Creature).with_did(
            empyrean_entity::enums::PropertyDataId::CombatTable,
            0x3000_0000,
        ),
    ));
    guid_manager::initialize(&mut w, &mut EmptyShard);
    pm::install_shard_config(&mut w, pm::shard_config_handle(Box::new(MemShard::new())));
    pm::initialize(&mut w, true);

    add_online(&mut w, A, "Alpha", 1, "alphaacct", SA);
    add_online(&mut w, B, "Bravo", 2, "bravoacct", SB);
    add_online(&mut w, C, "Charlie", 3, "charlieacct", SC);

    let mut biota = empyrean_entity::Biota {
        id: D,
        weenie_class_id: PLAYER_WCID,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, "Delta".to_owned());
    let g = ObjectGuid::new(D);
    let offline = OfflinePlayer {
        biota,
        guid: g,
        account: Some(account(4, "deltaacct")),
        last_requested_database_save: DotNetDateTime::MIN_VALUE,
        changes_detected: false,
        allegiance: None,
        allegiance_node: None,
    };
    let pmgr = &mut w.player_manager;
    pmgr.offline_players.insert(D, offline);
    pmgr.player_names
        .insert(OrdinalIgnoreCase("Delta".to_owned()), IPlayer::Offline(g));
    pmgr.player_accounts
        .get_or_insert_with(4, DotNetDict::new)
        .insert(D, IPlayer::Offline(g));
    w
}

fn guid(g: u32) -> ObjectGuid {
    ObjectGuid::new(g)
}

type Sent = Vec<(SessionId, empyrean_net::GameMessageGroup, Vec<u8>)>;

fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap())
}

fn kind(b: &[u8]) -> u32 {
    if u32_at(b, 0) == GAME_EVENT {
        u32_at(b, 12)
    } else {
        u32_at(b, 0)
    }
}

/// The kinds each session got, in send order.
fn kinds_to(sent: &Sent, session: SessionId) -> Vec<u32> {
    sent.iter()
        .filter(|(s, _, _)| *s == session)
        .map(|(_, _, b)| kind(b))
        .collect()
}

/// The messages `session` got.
fn to(sent: &Sent, session: SessionId) -> Vec<Vec<u8>> {
    sent.iter()
        .filter(|(s, _, _)| *s == session)
        .map(|(_, _, b)| b.clone())
        .collect()
}

fn decode<M: Message>(blob: &[u8]) -> M {
    let split = split_ui_blob(blob).expect("a blob");
    let mut body = split.body;
    M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", kind(blob)))
}

fn chat_text(blob: &[u8]) -> (String, u32) {
    let m: CommunicationTextboxString = decode(blob);
    (m.text, m.text_type)
}

/// Runs a client game action from `session` and returns everything sent meanwhile.
fn act<M: Message>(w: &mut World, session: SessionId, m: &M) -> Sent {
    start_capture();
    let data = pack_action(0x10, m).expect("encode");
    handle_client_message(w, ClientMessage::new(data).expect("opcode"), session);
    run_inbound_message_queue(w);
    take_sent()
}

/// Runs a client message (not a game action) from `session`.
fn message(w: &mut World, session: SessionId, data: Vec<u8>) -> Sent {
    start_capture();
    handle_client_message(w, ClientMessage::new(data).expect("opcode"), session);
    run_inbound_message_queue(w);
    take_sent()
}

fn character(w: &World, g: u32) -> &Character {
    w.objects
        .get(guid(g))
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .character
        .as_ref()
        .unwrap()
}

fn changes_detected(w: &World, g: u32) -> bool {
    w.objects
        .get(guid(g))
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player_database
        .character_changes_detected
}

fn clear_changes(w: &mut World, g: u32) {
    w.objects
        .get_mut(guid(g))
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_database
        .character_changes_detected = false;
}

fn squelch_rows(w: &World, g: u32) -> Vec<(u32, u32, u32)> {
    character(w, g)
        .character_properties_squelch
        .iter()
        .map(|r| (r.squelch_character_id, r.squelch_account_id, r.r#type))
        .collect()
}

fn we(e: WeenieError) -> u32 {
    e.0.cs_cast()
}

fn wes(e: WeenieErrorWithString) -> u32 {
    e.0.cs_cast()
}

fn set_option(w: &mut World, g: u32, option: CharacterOption) {
    empyrean_world::world_objects::player_character::set_character_option(w, guid(g), option, true);
}

// ------------------------------------------------------------------ vectors (ACE's compiled code)

#[test]
fn to_mask_add_remove_and_is_legal_channel_match_ace() {
    let file = vectors::load_named("chat", "chat_message_type_to_mask");
    assert!(file.cases.len() > 70);
    for c in &file.cases {
        let t = u32::try_from(u64_of(&c.input["type"]).unwrap()).unwrap();
        assert_eq!(
            u64::from(ChatMessageType(t).to_mask().0),
            u64_of(&c.output).unwrap(),
            "ToMask({t})"
        );
    }

    for (name, add) in [("squelch_mask_add", true), ("squelch_mask_remove", false)] {
        let file = vectors::load_named("chat", name);
        assert!(file.cases.len() > 800);
        for c in &file.cases {
            let a = SquelchMask(u32::try_from(u64_of(&c.input["a"]).unwrap()).unwrap());
            let b = SquelchMask(u32::try_from(u64_of(&c.input["b"]).unwrap()).unwrap());
            let r = if add { a.add(b) } else { a.remove(b) };
            assert_eq!(
                u64::from(r.0),
                u64_of(&c.output).unwrap(),
                "{name}({:X}, {:X})",
                a.0,
                b.0
            );
        }
    }

    let file = vectors::load_named("chat", "squelch_manager_is_legal_channel");
    for c in &file.cases {
        let t = u32::try_from(u64_of(&c.input["type"]).unwrap()).unwrap();
        assert_eq!(
            squelch_manager::is_legal_channel(ChatMessageType(t)),
            c.output.as_bool().unwrap(),
            "IsLegalChannel({t})"
        );
    }
}

/// `SquelchDB.Contains` over account, character and global squelches, against a null source and
/// Bravo (0x50000002 on account "bravoacct", as the harness's Player).
#[test]
fn squelch_db_contains_matches_ace() {
    let w = world();
    let file = vectors::load_named("chat", "squelch_db_contains");
    assert!(file.cases.len() > 1000);
    for c in &file.cases {
        let i = &c.input;
        let mut db = SquelchDB::default();
        for a in i["accounts"].as_array().unwrap() {
            db.accounts.add(a.as_str().unwrap().to_owned(), 0x5000_0009);
        }
        if let (Some(g), Some(m)) = (u64_of(&i["character_guid"]), u64_of(&i["character_mask"])) {
            db.characters.add(
                u32::try_from(g).unwrap(),
                SquelchInfo::from_filter(SquelchMask(u32::try_from(m).unwrap()), "Charlie", false),
            );
        }
        db.globals = SquelchInfo::new();
        if let Some(g) = u64_of(&i["global"]) {
            db.globals
                .filters
                .push(SquelchMask(u32::try_from(g).unwrap()));
        }
        let source = if i["player_source"].as_bool().unwrap() {
            guid(B)
        } else {
            guid(0)
        };
        let t = ChatMessageType(u32::try_from(u64_of(&i["type"]).unwrap()).unwrap());
        assert_eq!(
            db.contains(&w, source, t),
            c.output.as_bool().unwrap(),
            "{i}"
        );
    }
}

// ------------------------------------------------------------------ character squelches

#[test]
fn a_character_squelch_is_added_widened_narrowed_and_removed_with_aces_messages() {
    let mut w = world();
    let squelch =
        |add: bool, id: u32, name: &str, t: ChatMessageType| CommunicationModifyCharacterSquelch {
            add: i32::from(add),
            character_id: ObjectId(id),
            character_name: name.to_owned(),
            msg_type: t.0,
        };

    // by name, on the Tell channel
    let sent = act(
        &mut w,
        SA,
        &squelch(true, 0, "Bravo", ChatMessageType::Tell),
    );
    assert_eq!(kinds_to(&sent, SA), [CHAT, SET_SQUELCH_DB]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]),
        (
            "Bravo has been squelched on the Tell channel.".to_owned(),
            ChatMessageType::Broadcast.0
        )
    );
    let db: CommunicationSetSquelchDb = decode(&to(&sent, SA)[1]);
    let _ = db;
    assert_eq!(squelch_rows(&w, A), [(B, 0, SquelchMask::Tell.0)]);
    assert!(changes_detected(&w, A));
    assert_eq!(
        squelch_manager::squelches(&w, guid(A))
            .characters
            .get(&B)
            .map(|i| i.filters[0]),
        Some(SquelchMask::Tell)
    );
    assert!(squelch_manager::has_squelches(&w, guid(A)));

    // again: already squelched, and the SquelchDB is still sent
    let sent = act(
        &mut w,
        SA,
        &squelch(true, 0, "bravo", ChatMessageType::Tell),
    );
    assert_eq!(kinds_to(&sent, SA), [CHAT, SET_SQUELCH_DB]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Bravo is already squelched on the Tell channel."
    );

    // Bravo's tells are now blocked; Bravo gets the normal echo and no squelch notice (V264/V265/V266/V284)
    let sent = act(
        &mut w,
        SB,
        &CommunicationTalkDirectByName {
            message: "psst".to_owned(),
            target_name: "Alpha".to_owned(),
        },
    );
    assert_eq!(kinds_to(&sent, SB), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SB)[0]),
        (
            "You tell Alpha, \"psst\"".to_owned(),
            ChatMessageType::OutgoingTell.0
        )
    );
    assert!(kinds_to(&sent, SA).is_empty());

    // a second channel widens the mask (SquelchMask.Add)
    let sent = act(&mut w, SA, &squelch(true, B, "", ChatMessageType::Speech));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Bravo has been squelched on the Speech channel."
    );
    assert_eq!(
        squelch_rows(&w, A),
        [(B, 0, SquelchMask::Tell.0 | SquelchMask::Speech.0)]
    );

    // by guid, all channels: Add of AllChannels is AllChannels
    let sent = act(
        &mut w,
        SA,
        &squelch(true, B, "", ChatMessageType::AllChannels),
    );
    assert_eq!(chat_text(&to(&sent, SA)[0]).0, "Bravo has been squelched.");
    assert_eq!(squelch_rows(&w, A), [(B, 0, SquelchMask::AllChannels.0)]);

    // one channel off AllChannels leaves Combined without it. ACE-BUG: CharacterChangesDetected is
    // not set on this path.
    clear_changes(&mut w, A);
    let sent = act(&mut w, SA, &squelch(false, B, "", ChatMessageType::Tell));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Bravo has been unsquelched on the Tell channel."
    );
    assert_eq!(
        squelch_rows(&w, A),
        [(B, 0, SquelchMask::Combined.0 & !SquelchMask::Tell.0)]
    );
    assert!(
        changes_detected(&w, A),
        "V320: the partial unsquelch is flagged for saving (ACE: not)"
    );

    let sent = act(&mut w, SA, &squelch(false, B, "", ChatMessageType::Tell));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Bravo is not squelched on the Tell channel."
    );

    let sent = act(
        &mut w,
        SA,
        &squelch(false, 0, "Bravo", ChatMessageType::AllChannels),
    );
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Bravo has been unsquelched."
    );
    assert!(squelch_rows(&w, A).is_empty());
    assert!(changes_detected(&w, A));
    assert!(!squelch_manager::has_squelches(&w, guid(A)));

    let sent = act(
        &mut w,
        SA,
        &squelch(false, 0, "Bravo", ChatMessageType::AllChannels),
    );
    assert_eq!(chat_text(&to(&sent, SA)[0]).0, "Bravo is not squelched.");
    assert_eq!(kinds_to(&sent, SA), [CHAT, SET_SQUELCH_DB]);
}

#[test]
fn character_squelch_refusals_and_an_offline_target() {
    let mut w = world();
    let squelch = |id: u32, name: &str, t: u32| CommunicationModifyCharacterSquelch {
        add: 1,
        character_id: ObjectId(id),
        character_name: name.to_owned(),
        msg_type: t,
    };

    // not a legal channel: the chat line only
    let sent = act(
        &mut w,
        SA,
        &squelch(0, "Bravo", ChatMessageType::OutgoingTell.0),
    );
    assert_eq!(kinds_to(&sent, SA), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "OutgoingTell is not a legal squelch channel"
    );

    let sent = act(&mut w, SA, &squelch(0x5000_0099, "", 3));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Couldn't find player to squelch."
    );
    let sent = act(&mut w, SA, &squelch(0, "Nobody", 3));
    assert_eq!(chat_text(&to(&sent, SA)[0]).0, "Nobody not found.");
    let sent = act(&mut w, SA, &squelch(0, "   ", 3));
    assert!(sent.is_empty(), "a blank name is ignored");
    let sent = act(&mut w, SA, &squelch(A, "", 3));
    assert_eq!(kinds_to(&sent, SA), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "You can't squelch yourself!"
    );

    // an offline player, by guid: PlayerManager.FindByGuid finds it, and the SquelchDB names it
    let sent = act(&mut w, SA, &squelch(D, "", ChatMessageType::Tell.0));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Delta has been squelched on the Tell channel."
    );
    let db = squelch_manager::squelches(&w, guid(A));
    assert_eq!(
        db.characters
            .get(&D)
            .map(|i| (i.filters[0], i.player_name.clone())),
        Some((SquelchMask::Tell, Some("Delta".to_owned())))
    );
}

// ------------------------------------------------------------------ account and global squelches

#[test]
fn an_account_squelch_blocks_every_character_of_the_account() {
    let mut w = world();
    let account_squelch = |add: bool, name: &str| CommunicationModifyAccountSquelch {
        add: i32::from(add),
        character_name: name.to_owned(),
    };

    let sent = act(&mut w, SA, &account_squelch(true, "Alpha"));
    assert_eq!(kinds_to(&sent, SA), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "You can't squelch yourself!"
    );
    let sent = act(&mut w, SA, &account_squelch(true, "Nobody"));
    assert_eq!(chat_text(&to(&sent, SA)[0]).0, "Nobody not found.");

    let sent = act(&mut w, SA, &account_squelch(true, "Charlie"));
    assert_eq!(kinds_to(&sent, SA), [CHAT, SET_SQUELCH_DB]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Charlie's account has been squelched."
    );
    assert_eq!(squelch_rows(&w, A), [(C, 3, SquelchMask::AllChannels.0)]);
    let db = squelch_manager::squelches(&w, guid(A));
    assert_eq!(db.accounts.get("charlieacct"), Some(&C));
    assert!(db.characters.is_empty());
    assert!(
        db.contains(&w, guid(C), ChatMessageType::Tell),
        "the account squelch covers every channel"
    );

    let sent = act(
        &mut w,
        SC,
        &CommunicationTalkDirectByName {
            message: "hey".to_owned(),
            target_name: "alpha".to_owned(),
        },
    );
    assert_eq!(kinds_to(&sent, SC), [CHAT]);
    assert!(kinds_to(&sent, SA).is_empty());

    let sent = act(&mut w, SA, &account_squelch(true, "Charlie"));
    assert_eq!(
        kinds_to(&sent, SA),
        [CHAT],
        "already squelched: no SquelchDB"
    );
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Charlie's account is already squelched."
    );

    // V320: a character squelch of the character the account squelch
    // was made through leaves the account squelch as it is (ACE overwrote it with a Speech-only
    // character squelch)
    let character_squelch = CommunicationModifyCharacterSquelch {
        add: 1,
        character_id: ObjectId(0),
        character_name: "Charlie".to_owned(),
        msg_type: ChatMessageType::Speech.0,
    };
    let sent = act(&mut w, SA, &character_squelch);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Charlie is already squelched on the Speech channel."
    );
    assert_eq!(squelch_rows(&w, A), [(C, 3, SquelchMask::AllChannels.0)]);
    assert!(
        squelch_manager::squelches(&w, guid(A)).contains(&w, guid(C), ChatMessageType::Tell),
        "still every channel"
    );

    let sent = act(&mut w, SA, &account_squelch(false, "Charlie"));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Charlie's account has been unsquelched."
    );
    assert!(squelch_rows(&w, A).is_empty());
    let sent = act(&mut w, SA, &account_squelch(false, "Charlie"));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "Charlie's account is not squelched."
    );
}

#[test]
fn a_global_squelch_sets_squelch_global_and_filters_system_messages() {
    let mut w = world();
    let global = |add: bool, t: ChatMessageType| CommunicationModifyGlobalSquelch {
        add: i32::from(add),
        msg_type: t.0,
    };

    let sent = act(&mut w, SA, &global(true, ChatMessageType::Magic));
    assert_eq!(kinds_to(&sent, SA), [CHAT, SET_SQUELCH_DB]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "The Magic channel has been squelched."
    );
    assert_eq!(
        w.objects
            .get(guid(A))
            .unwrap()
            .get_property(PropertyInt::SquelchGlobal),
        Some(SquelchMask::Magic.0.cast_signed())
    );

    let sent = act(&mut w, SA, &global(true, ChatMessageType::Magic));
    assert_eq!(kinds_to(&sent, SA), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "The Magic channel is already squelched."
    );

    // Player.SendMessage: a squelched legal channel is dropped, any other type is sent
    start_capture();
    player::send_message(&mut w, guid(A), "a spell", ChatMessageType::Magic);
    assert!(take_sent().is_empty());
    player::send_message(&mut w, guid(A), "news", ChatMessageType::Broadcast);
    player::send_message(&mut w, guid(A), "a hit", ChatMessageType::Combat);
    assert_eq!(kinds_to(&take_sent(), SA), [CHAT, CHAT]);

    let sent = act(&mut w, SA, &global(false, ChatMessageType::Magic));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "The Magic channel has been unsquelched."
    );
    assert_eq!(
        w.objects
            .get(guid(A))
            .unwrap()
            .get_property(PropertyInt::SquelchGlobal),
        None
    );
    let sent = act(&mut w, SA, &global(false, ChatMessageType::Magic));
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "The Magic channel is already unsquelched."
    );
}

// ------------------------------------------------------------------ tells

#[test]
fn a_tell_by_name_reaches_its_target_and_offline_names_are_not_available() {
    let mut w = world();
    let tell = |message: &str, target: &str| CommunicationTalkDirectByName {
        message: message.to_owned(),
        target_name: target.to_owned(),
    };

    // the name is trimmed and matched ignoring case
    let sent = act(&mut w, SB, &tell("hello there", "  alpha "));
    assert_eq!(kinds_to(&sent, SB), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SB)[0]),
        (
            "You tell Alpha, \"hello there\"".to_owned(),
            ChatMessageType::OutgoingTell.0
        )
    );
    assert_eq!(kinds_to(&sent, SA), [TELL]);
    let t: CommunicationHearDirectSpeech = decode(&to(&sent, SA)[0]);
    assert_eq!(
        (
            t.message.as_str(),
            t.sender_name.as_str(),
            t.sender_id.0,
            t.target_id.0,
            t.text_type,
            t.secret_flags
        ),
        ("hello there", "Bravo", B, A, ChatMessageType::Tell.0, 0)
    );
    assert!(kinds_to(&sent, SC).is_empty());

    for target in ["Delta", "Nobody"] {
        let sent = act(&mut w, SB, &tell("anyone?", target));
        assert_eq!(kinds_to(&sent, SB), [WEENIE_ERROR], "{target}");
        let e: CommunicationWeenieError = decode(&to(&sent, SB)[0]);
        assert_eq!(e.error_type, we(WeenieError::CharacterNotAvailable));
        assert_eq!(sent.len(), 1);
    }

    // a tell to oneself: no echo, the tell arrives
    let sent = act(&mut w, SB, &tell("note", "Bravo"));
    assert_eq!(kinds_to(&sent, SB), [TELL]);

    // an away target: the AFK reply (default text), and the tell still arrives
    w.objects
        .get_mut(guid(A))
        .unwrap()
        .set_property(PropertyBool::Afk, true);
    let sent = act(&mut w, SB, &tell("you there?", "Alpha"));
    assert_eq!(kinds_to(&sent, SB), [CHAT, WEENIE_ERROR_STR]);
    let e: CommunicationWeenieErrorWithString = decode(&to(&sent, SB)[1]);
    assert_eq!(
        (e.error_type, e.text.as_str()),
        (
            wes(WeenieErrorWithString::AFK),
            "Alpha is away: I am currently away from the keyboard."
        )
    );
    assert_eq!(kinds_to(&sent, SA), [TELL]);
    w.objects
        .get_mut(guid(A))
        .unwrap()
        .set_property(PropertyString::Afk, "brb".to_owned());
    let sent = act(&mut w, SB, &tell("ok", "Alpha"));
    let e: CommunicationWeenieErrorWithString = decode(&to(&sent, SB)[1]);
    assert_eq!(e.text, "Alpha is away: brb");
}

/// `GetNameWithSuffix`: an Olthoi player's tells are signed `&`, or `^` with `NoOlthoiTalk`.
#[test]
fn an_olthoi_players_name_carries_its_suffix() {
    let mut w = world();
    w.objects
        .get_mut(guid(B))
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_properties
        .is_olthoi_player = true;
    let tell = CommunicationTalkDirectByName {
        message: "hiss".to_owned(),
        target_name: "Alpha".to_owned(),
    };
    let sent = act(&mut w, SB, &tell);
    let t: CommunicationHearDirectSpeech = decode(&to(&sent, SA)[0]);
    assert_eq!(t.sender_name, "Bravo&");
    w.objects
        .get_mut(guid(B))
        .unwrap()
        .set_property(PropertyBool::NoOlthoiTalk, true);
    let sent = act(&mut w, SB, &tell);
    let t: CommunicationHearDirectSpeech = decode(&to(&sent, SA)[0]);
    assert_eq!(t.sender_name, "Bravo^");
}

#[test]
fn a_gagged_player_cannot_tell_talk_or_emote() {
    let mut w = world();
    w.objects
        .get_mut(guid(B))
        .unwrap()
        .set_property(PropertyBool::IsGagged, true);
    let gag =
        "You are unable to talk locally, globally, or send tells because you have been gagged.";

    let tell = CommunicationTalkDirectByName {
        message: "hi".to_owned(),
        target_name: "Alpha".to_owned(),
    };
    for sent in [
        act(&mut w, SB, &tell),
        act(
            &mut w,
            SB,
            &CommunicationTalk {
                message: "hi".to_owned(),
            },
        ),
        act(
            &mut w,
            SB,
            &CommunicationEmote {
                message: "waves".to_owned(),
            },
        ),
    ] {
        assert_eq!(kinds_to(&sent, SB), [TRANSIENT, CHAT]);
        let t: CommunicationTransientString = decode(&to(&sent, SB)[0]);
        assert_eq!(t.text, gag);
        assert_eq!(
            chat_text(&to(&sent, SB)[1]),
            (gag.to_owned(), ChatMessageType::WorldBroadcast.0)
        );
        assert!(kinds_to(&sent, SA).is_empty());
    }
}

#[test]
fn talk_direct_to_an_object_not_in_the_landblock_is_not_available() {
    let mut w = world();
    let sent = act(
        &mut w,
        SB,
        &CommunicationTalkDirect {
            message: "hi".to_owned(),
            target: ObjectId(A),
        },
    );
    assert_eq!(kinds_to(&sent, SB), [WEENIE_ERROR]);
    let e: CommunicationWeenieError = decode(&to(&sent, SB)[0]);
    assert_eq!(e.error_type, we(WeenieError::CharacterNotAvailable));
}

/// V264/V265/V266/V284: a tell to a player in the landblock. A gagged sender gets the gag
/// message and no "You tell ..." echo (a fix: ACE echoes before the gag check). A squelched
/// sender gets the echo and nothing else: no "has you squelched" notice (the retail pcaps), and
/// nothing is delivered.
#[test]
fn talk_direct_echo_is_withheld_from_a_gagged_sender_but_kept_for_a_squelched_one() {
    use super::fellowship::{chats_to, events_to, sent, H};
    let (a, b) = (guid(A), guid(B));
    let mut h = H::small();
    let sa = h.player(a, "Alpha", 3);
    let sb = h.player(b, "Bravo", 3);
    for (i, s) in [sa, sb].into_iter().enumerate() {
        let d = h.w.sessions.get_mut(s).unwrap();
        d.state = SessionState::WorldConnected;
        d.set_account(
            u32::try_from(i + 1).unwrap(),
            format!("acct{i}"),
            AccessLevel::Player,
        );
    }
    let tell = CommunicationTalkDirect {
        message: "hi".to_owned(),
        target: ObjectId(A),
    };
    let talk = |h: &mut H| {
        start_capture();
        handle_client_message(
            &mut h.w,
            ClientMessage::new(pack_action(0x10, &tell).unwrap()).unwrap(),
            sb,
        );
        run_inbound_message_queue(&mut h.w);
        sent()
    };

    // delivered: the echo, then the tell
    let msgs = talk(&mut h);
    assert_eq!(chats_to(&msgs, sb), ["You tell Alpha, \"hi\""]);
    assert_eq!(events_to(&msgs, sa), [TELL]);

    // gagged: the gag message only
    h.w.objects
        .get_mut(b)
        .unwrap()
        .set_property(PropertyBool::IsGagged, true);
    let msgs = talk(&mut h);
    assert_eq!(
        chats_to(&msgs, sb),
        ["You are unable to talk locally, globally, or send tells because you have been gagged."]
    );
    assert!(msgs.iter().all(|m| m.0 != sa), "nothing delivered");
    h.w.objects
        .get_mut(b)
        .unwrap()
        .set_property(PropertyBool::IsGagged, false);

    // squelched: the echo only, no notice (the sender does not learn of the squelch)
    squelch_manager::handle_action_modify_character_squelch(
        &mut h.w,
        a,
        true,
        B,
        "",
        ChatMessageType::Tell,
    );
    let msgs = talk(&mut h);
    assert_eq!(chats_to(&msgs, sb), ["You tell Alpha, \"hi\""]);
    assert!(events_to(&msgs, sb).is_empty(), "no squelch notice");
    assert_eq!(
        msgs.iter().filter(|m| m.0 == sb).count(),
        1,
        "the echo and nothing else"
    );
    assert!(msgs.iter().all(|m| m.0 != sa), "nothing delivered");
}

// ------------------------------------------------------------------ legacy channels

#[test]
fn chat_channels_are_gated_by_access_level_fellowship_and_allegiance() {
    let mut w = world();
    let say = |channel: Channel| CommunicationChannelBroadcast {
        channel: channel.0.cast_unsigned(),
        message: "hello".to_owned(),
    };
    for (channel, error) in [
        (Channel::Abuse, WeenieError::YouCantUseThatChannel),
        (Channel::Admin, WeenieError::YouCantUseThatChannel),
        (Channel::Audit, WeenieError::YouCantUseThatChannel),
        (Channel::Advocate2, WeenieError::YouCantUseThatChannel),
        (Channel::Sentinel, WeenieError::YouCantUseThatChannel),
        (Channel::Fellow, WeenieError::YouDoNotBelongToAFellowship),
        (Channel::Vassals, WeenieError::YouAreNotInAllegiance),
        (Channel::Patron, WeenieError::YouAreNotInAllegiance),
        (Channel::Monarch, WeenieError::YouAreNotInAllegiance),
        (Channel::CoVassals, WeenieError::YouAreNotInAllegiance),
        (
            Channel::AllegianceBroadcast,
            WeenieError::YouAreNotInAllegiance,
        ),
    ] {
        let sent = act(&mut w, SA, &say(channel));
        assert_eq!(kinds_to(&sent, SA), [WEENIE_ERROR], "{channel}");
        let e: CommunicationWeenieError = decode(&to(&sent, SA)[0]);
        assert_eq!(e.error_type, we(error), "{channel}");
        assert_eq!(sent.len(), 1);
    }

    // an unknown channel does nothing
    assert!(act(&mut w, SA, &say(Channel(0x7000_0000))).is_empty());

    // an advocate talks on Advocate1 to everyone who has it active; the sender's own copy has no name
    w.sessions.get_mut(SC).unwrap().access_level = AccessLevel::Advocate;
    for g in [A, C] {
        w.objects
            .get_mut(guid(g))
            .unwrap()
            .set_property(PropertyInt::ChannelsActive, Channel::Advocate1.0);
    }
    let sent = act(&mut w, SC, &say(Channel::Advocate1));
    assert_eq!(kinds_to(&sent, SA), [CHANNEL_BROADCAST]);
    assert_eq!(kinds_to(&sent, SC), [CHANNEL_BROADCAST]);
    assert!(kinds_to(&sent, SB).is_empty());
    let got: CommunicationChannelBroadcastRecv = decode(&to(&sent, SA)[0]);
    assert_eq!(
        (got.channel, got.sender_name.as_str(), got.message.as_str()),
        (Channel::Advocate1.0.cast_unsigned(), "Charlie", "hello")
    );
    let own: CommunicationChannelBroadcastRecv = decode(&to(&sent, SC)[0]);
    assert_eq!(own.sender_name, "");

    // a squelch does not stop it (ignoreSquelch)
    squelch_manager::handle_action_modify_character_squelch(
        &mut w,
        guid(A),
        true,
        C,
        "",
        ChatMessageType::AllChannels,
    );
    let sent = act(&mut w, SC, &say(Channel::Advocate1));
    assert_eq!(kinds_to(&sent, SA), [CHANNEL_BROADCAST]);

    // Help: the server's note first, then the broadcast (only when the sender has Help active)
    let sent = act(&mut w, SB, &say(Channel::Help));
    assert_eq!(kinds_to(&sent, SB), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SB)[0]).0,
        "GameActionChatChannel TellHelp Needs work."
    );
}

#[test]
fn add_remove_list_and_index_channels() {
    let mut w = world();
    let add = CommunicationAddToChannel {
        channel: Channel::Audit.0.cast_unsigned(),
    };
    let remove = CommunicationRemoveFromChannel {
        channel: Channel::Audit.0.cast_unsigned(),
    };
    let list = CommunicationChannelListRequest {
        channel: Channel::Audit.0.cast_unsigned(),
    };

    // a plain player is ignored
    w.objects
        .get_mut(guid(A))
        .unwrap()
        .set_property(PropertyInt::ChannelsAllowed, Channel::Audit.0);
    assert!(act(&mut w, SA, &add).is_empty());
    assert!(act(&mut w, SA, &list).is_empty());

    // a sentinel with Audit allowed
    w.sessions.get_mut(SC).unwrap().access_level = AccessLevel::Sentinel;
    w.objects.get_mut(guid(C)).unwrap().set_property(
        PropertyInt::ChannelsAllowed,
        (Channel::Audit | Channel::Sentinel).0,
    );
    let sent = act(&mut w, SC, &add);
    assert_eq!(kinds_to(&sent, SC), [WEENIE_ERROR_STR]);
    let e: CommunicationWeenieErrorWithString = decode(&to(&sent, SC)[0]);
    assert_eq!(
        (e.error_type, e.text.as_str()),
        (
            wes(WeenieErrorWithString::YouHaveEnteredThe_Channel),
            "Audit"
        )
    );
    assert_eq!(
        w.objects
            .get(guid(C))
            .unwrap()
            .get_property(PropertyInt::ChannelsActive),
        Some(Channel::Audit.0)
    );

    let sent = act(&mut w, SC, &list);
    assert_eq!(kinds_to(&sent, SC), [CHANNEL_LIST]);
    let body = &to(&sent, SC)[0][16..];
    assert_eq!(u32_at(body, 0), 1);
    assert_eq!(&body[4..6], &8u16.to_le_bytes());
    assert_eq!(&body[6..14], b"+Charlie");

    // not allowed: nothing
    let admin = CommunicationAddToChannel {
        channel: Channel::Admin.0.cast_unsigned(),
    };
    assert!(act(&mut w, SC, &admin).is_empty());

    let sent = act(&mut w, SC, &remove);
    let e: CommunicationWeenieErrorWithString = decode(&to(&sent, SC)[0]);
    assert_eq!(
        (e.error_type, e.text.as_str()),
        (wes(WeenieErrorWithString::YouHaveLeftThe_Channel), "Audit")
    );
    assert_eq!(
        w.objects
            .get(guid(C))
            .unwrap()
            .get_property(PropertyInt::ChannelsActive),
        Some(0)
    );
    assert!(act(&mut w, SC, &remove).is_empty(), "not active: nothing");

    // the channel index: admins only
    assert!(act(&mut w, SC, &CommunicationChannelIndexRequest).is_empty());
    w.objects
        .get_mut(guid(C))
        .unwrap()
        .set_property(PropertyBool::IsAdmin, true);
    let sent = act(&mut w, SC, &CommunicationChannelIndexRequest);
    assert_eq!(kinds_to(&sent, SC), [CHANNEL_INDEX]);
    assert_eq!(u32_at(&to(&sent, SC)[0], 16), 8);
}

// ------------------------------------------------------------------ TurbineChat

/// A client TurbineChat blob with any blob and dispatch type (ACE's reader layout; the
/// "bytes to follow" words are exact, as the client writes them).
fn turbine_blob(
    blob_type: u32,
    dispatch: u32,
    context: u32,
    room: u32,
    text: &str,
    sender: u32,
    chat_type: u32,
) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut body = Vec::new();
    for v in [context, 2, 2, room] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    let n = units.len();
    if n < 0x80 {
        body.push(u8::try_from(n).unwrap());
    } else {
        body.push(u8::try_from(n >> 8).unwrap() | 0x80);
        body.push(u8::try_from(n & 0xFF).unwrap());
    }
    for u in units {
        body.extend_from_slice(&u.to_le_bytes());
    }
    for v in [0x0C, sender, 0, chat_type] {
        body.extend_from_slice(&v.to_le_bytes());
    }
    let mut packet = Vec::new();
    for v in [blob_type, dispatch, 1, 0, 0, 0, 0] {
        packet.extend_from_slice(&v.to_le_bytes());
    }
    packet.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    packet.extend(body);
    let mut data = TURBINE_CHAT.to_le_bytes().to_vec();
    data.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
    data.extend(packet);
    data
}

/// The client's `SendToRoomById` request, as dereth-protocol encodes it.
fn general(context: u32, text: &str) -> Vec<u8> {
    write_blob(&SendToRoomById {
        context,
        room: 2,
        text: text.to_owned(),
        sender: A,
        chat_type: 2,
    })
    .expect("encode")
}

fn turbine(blob: &[u8]) -> IncomingPayload {
    assert_eq!(u32_at(blob, 0), TURBINE_CHAT);
    let packet = decode_incoming(&blob[4..]).expect("decodes");
    assert!(!packet.ace_overstated_extent, "V239: the extents are exact");
    packet.payload
}

fn room_event(blob: &[u8]) -> (u32, String, String) {
    match turbine(blob) {
        IncomingPayload::RoomEvent(e) => (e.room, e.name, e.text),
        other => panic!("not a room event: {other:?}"),
    }
}

/// The room event's sender id and chat type (the words after its text).
fn room_event_sender_and_type(blob: &[u8]) -> (u32, u32) {
    match turbine(blob) {
        IncomingPayload::RoomEvent(e) => (u32_at(&e.extra, 0), u32_at(&e.extra, 8)),
        other => panic!("not a room event: {other:?}"),
    }
}

fn room_response(blob: &[u8]) -> u32 {
    match turbine(blob) {
        IncomingPayload::RoomResponse(r) => r.context,
        other => panic!("not a response: {other:?}"),
    }
}

#[test]
fn turbine_blob_matches_dereth_protocols_request_encoding() {
    assert_eq!(general(7, "hello"), turbine_blob(3, 2, 7, 2, "hello", A, 2));
}

/// V239 (retail, owner 2026-09-24): the server's TurbineChat is retail's layout. Both "bytes to
/// follow" counts are exact (dereth-protocol reads it without its ACE-extent allowance), and each string
/// carries its own length in the client's packed form, however long the name or the message.
#[test]
fn the_servers_turbine_chat_is_retails_layout() {
    use empyrean_entity::enums::{ChatNetworkBlobDispatchType, ChatNetworkBlobType, ChatType};
    use empyrean_world::network::game_messages::messages::game_message_turbine_chat::game_message_turbine_chat;

    let blob = |kind, name: &str, text: &str| {
        game_message_turbine_chat(
            kind,
            ChatNetworkBlobDispatchType(1),
            2,
            name,
            text,
            A,
            ChatType(2),
        )
        .data
    };
    let long_name = "N".repeat(200);
    let long_text = "x".repeat(300);
    let longer_text = "y".repeat(0x4001);
    for (name, text) in [
        ("Alpha", "hi"),
        ("", ""),
        ("Alpha", long_text.as_str()),
        (long_name.as_str(), "hi"),
        (long_name.as_str(), longer_text.as_str()),
    ] {
        let b = blob(ChatNetworkBlobType::NETBLOB_EVENT_BINARY, name, text);
        assert_eq!(u32_at(&b, 4) as usize, b.len() - 8, "the outer count");
        assert_eq!(u32_at(&b, 36) as usize, b.len() - 40, "the payload count");
        assert_eq!(room_event(&b), (2, name.to_owned(), text.to_owned()));
    }
    let b = blob(ChatNetworkBlobType::NETBLOB_RESPONSE_BINARY, "Alpha", "hi");
    assert_eq!((u32_at(&b, 4), u32_at(&b, 36)), (48, 16));
    assert_eq!(room_response(&b), 2);
}

/// Not ACE's (V302, owner 2026-09-24, retail): the request's character count is read in the
/// client's packed form, as dereth-protocol writes it: one byte below 0x80, two up to 0x3FFF, and from
/// 0x4000 four (`0xC0` and up, then two more bytes). Each reads back exactly and consumes exactly
/// its own bytes.
#[test]
fn the_turbine_chat_length_reads_the_clients_packed_form() {
    use empyrean_world::network::handlers::turbine_chat_handler::read_packed_length;
    use empyrean_world::network::managers::inbound_message_manager::Payload;
    for (n, size) in [
        (0u32, 1),
        (0x7F, 1),
        (0x80, 2),
        (0x3FFF, 2),
        (0x4000, 4),
        (0x4001, 4),
        (0x1_2345, 4),
    ] {
        let mut w = dereth_protocol::Writer::new();
        w.compressed_u32(n);
        let packed = w.into_inner();
        assert_eq!(packed.len(), size, "{n:#X}");
        let mut data = vec![0; 4]; // the opcode the payload starts after
        data.extend(&packed);
        data.push(0xEE);
        let mut p = Payload::new(&data);
        assert_eq!(
            read_packed_length(&mut p).expect("reads"),
            n as usize,
            "{n:#X}"
        );
        assert_eq!(p.position(), 4 + size, "{n:#X}: exactly its bytes");
    }
}

/// V302: a General message of 0x4001 characters (a four-byte count) reaches the listener whole.
#[test]
fn a_general_message_of_0x4000_characters_or_more_reads_whole() {
    let mut w = world();
    set_option(&mut w, B, CharacterOption::ListenToGeneralChat);
    let text = "z".repeat(0x4001);
    let sent = message(&mut w, SA, general(7, &text));
    assert_eq!(kinds_to(&sent, SB), [TURBINE_CHAT]);
    assert_eq!(room_event(&to(&sent, SB)[0]), (2, "Alpha".to_owned(), text));
}

#[test]
fn a_general_message_reaches_the_listeners_and_the_sender_gets_its_response() {
    let mut w = world();
    set_option(&mut w, B, CharacterOption::ListenToGeneralChat);
    set_option(&mut w, C, CharacterOption::ListenToGeneralChat);

    let sent = message(&mut w, SA, general(7, "hello all"));
    assert_eq!(kinds_to(&sent, SB), [TURBINE_CHAT]);
    assert_eq!(kinds_to(&sent, SC), [TURBINE_CHAT]);
    assert_eq!(
        room_event(&to(&sent, SB)[0]),
        (2, "Alpha".to_owned(), "hello all".to_owned())
    );
    // Alpha does not listen to General: only the response for its context
    assert_eq!(kinds_to(&sent, SA), [TURBINE_CHAT]);
    assert_eq!(room_response(&to(&sent, SA)[0]), 7);

    // a squelch (any character squelch covering all channels) drops the recipient
    squelch_manager::handle_action_modify_character_squelch(
        &mut w,
        guid(C),
        true,
        A,
        "",
        ChatMessageType::AllChannels,
    );
    let sent = message(&mut w, SA, general(8, "again"));
    assert_eq!(kinds_to(&sent, SB), [TURBINE_CHAT]);
    assert!(kinds_to(&sent, SC).is_empty());

    // global chat off: nothing is read or sent
    assert!(pm::modify_bool(&w, "use_turbine_chat", false));
    assert!(message(&mut w, SA, general(9, "anyone")).is_empty());
}

#[test]
fn turbine_chat_rejects_echoes_and_room_rules() {
    let mut w = world();
    set_option(&mut w, B, CharacterOption::ListenToGeneralChat);

    // level requirement: the reason as a transient and a chat line, then the response
    assert!(pm::modify_long(&w, "chat_requires_player_level", 5));
    let sent = message(&mut w, SA, general(3, "hi"));
    assert_eq!(kinds_to(&sent, SA), [TRANSIENT, CHAT, TURBINE_CHAT]);
    let reason =
        "General is currently disabled for you because this character has not reached level 5.";
    let t: CommunicationTransientString = decode(&to(&sent, SA)[0]);
    assert_eq!(t.text, reason);
    assert_eq!(
        chat_text(&to(&sent, SA)[1]),
        (reason.to_owned(), ChatMessageType::Broadcast.0)
    );
    assert_eq!(room_response(&to(&sent, SA)[2]), 3);
    assert!(kinds_to(&sent, SB).is_empty());
    assert!(pm::modify_long(&w, "chat_requires_player_level", 0));

    // echo only: the sender gets its own message and the response
    assert!(pm::modify_bool(&w, "chat_echo_only", true));
    let sent = message(&mut w, SA, general(4, "echo"));
    assert_eq!(kinds_to(&sent, SA), [TURBINE_CHAT, TURBINE_CHAT]);
    assert_eq!(room_event(&to(&sent, SA)[0]).2, "echo");
    assert_eq!(room_response(&to(&sent, SA)[1]), 4);
    assert!(kinds_to(&sent, SB).is_empty());
    assert!(pm::modify_bool(&w, "chat_echo_only", false));

    // General disabled: at the first listener the sender gets the response and nothing goes out
    assert!(pm::modify_bool(&w, "chat_disable_general", true));
    let sent = message(&mut w, SA, general(5, "no"));
    assert_eq!(kinds_to(&sent, SA), [TURBINE_CHAT]);
    assert!(kinds_to(&sent, SB).is_empty());
    assert!(pm::modify_bool(&w, "chat_disable_general", false));

    // Society with no society: a chat line only
    let society = write_blob(&SendToRoomById {
        context: 6,
        room: 7,
        text: "hail".to_owned(),
        sender: A,
        chat_type: 6,
    })
    .unwrap();
    let sent = message(&mut w, SA, society);
    assert_eq!(kinds_to(&sent, SA), [CHAT]);
    assert_eq!(
        chat_text(&to(&sent, SA)[0]).0,
        "You do not belong to a society."
    );

    // Olthoi from a non-Olthoi player: nothing
    let olthoi = write_blob(&SendToRoomById {
        context: 6,
        room: 10,
        text: "hiss".to_owned(),
        sender: A,
        chat_type: 10,
    })
    .unwrap();
    assert!(message(&mut w, SA, olthoi).is_empty());

    // Allegiance: no allegiance (AllegianceManager is not ported): nothing
    let allegiance = write_blob(&SendToRoomById {
        context: 6,
        room: 0x0B00_0001,
        text: "hi".to_owned(),
        sender: A,
        chat_type: 1,
    })
    .unwrap();
    assert!(message(&mut w, SA, allegiance).is_empty());

    // an unknown blob type: nothing
    assert!(message(&mut w, SA, turbine_blob(9, 2, 1, 2, "x", A, 2)).is_empty());
}

/// By name (`ASYNCMETHOD_SENDTOROOMBYNAME`), the chat type picks the room. V319 (owner
/// 2026-09-24, a fix): the listen filters compare that room, so a listener without the Trade
/// option does not get it (ACE compared the raw room id and sent it).
#[test]
fn a_by_name_request_is_routed_by_its_chat_type() {
    let mut w = world();
    let sent = message(&mut w, SA, turbine_blob(3, 1, 10, 0x1234, "wts", A, 3));
    assert!(
        kinds_to(&sent, SB).is_empty(),
        "Bravo does not listen to Trade"
    );
    assert_eq!(
        kinds_to(&sent, SA),
        [TURBINE_CHAT],
        "nor does Alpha: its response only"
    );
    for g in [A, B] {
        set_option(&mut w, g, CharacterOption::ListenToTradeChat);
        set_option(&mut w, g, CharacterOption::ListenToGeneralChat);
    }
    let sent = message(&mut w, SA, turbine_blob(3, 1, 11, 0x1234, "wts", A, 3));
    assert_eq!(
        kinds_to(&sent, SB),
        [TURBINE_CHAT],
        "Bravo listens to Trade"
    );
    assert_eq!(
        room_event(&to(&sent, SB)[0]),
        (3, "Alpha".to_owned(), "wts".to_owned())
    );
    assert_eq!(
        room_event_sender_and_type(&to(&sent, SB)[0]),
        (A, 3),
        "the chat type follows the adjusted room"
    );
    assert_eq!(
        kinds_to(&sent, SA),
        [TURBINE_CHAT, TURBINE_CHAT],
        "Alpha is a recipient too, then its response"
    );
    assert_eq!(room_response(&to(&sent, SA)[1]), 11);

    // an unknown chat type goes to General, and the chat type becomes the room's
    let sent = message(&mut w, SA, turbine_blob(3, 1, 12, 0x1234, "hi", A, 0));
    assert_eq!(room_event(&to(&sent, SB)[0]).0, 2);
    assert_eq!(room_event_sender_and_type(&to(&sent, SB)[0]), (A, 2));
}

/// V319: an Olthoi player leaving the Olthoi channel (as at log-out) is
/// told so; ACE's `!IsOlthoiPlayer || !IsAdmin` told only an Olthoi admin. A player that is
/// neither still gets nothing.
#[test]
fn an_olthoi_player_is_told_it_left_the_olthoi_channel() {
    use empyrean_world::world_objects::player_networking::leave_turbine_chat_channel;
    let mut w = world();
    start_capture();
    leave_turbine_chat_channel(&mut w, guid(A), "Olthoi", false);
    assert!(
        to(&take_sent(), SA).is_empty(),
        "not an Olthoi player: nothing"
    );

    w.objects
        .get_mut(guid(A))
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_properties
        .is_olthoi_player = true;
    start_capture();
    leave_turbine_chat_channel(&mut w, guid(A), "Olthoi", false);
    let sent = take_sent();
    assert_eq!(kinds_to(&sent, SA).first(), Some(&WEENIE_ERROR_STR));
    let e: CommunicationWeenieErrorWithString = decode(&to(&sent, SA)[0]);
    assert_eq!(
        (e.error_type, e.text.as_str()),
        (wes(WeenieErrorWithString::YouHaveLeftThe_Channel), "Olthoi")
    );
}

#[test]
fn a_gagged_player_cannot_use_turbine_chat() {
    let mut w = world();
    set_option(&mut w, B, CharacterOption::ListenToGeneralChat);
    w.objects
        .get_mut(guid(A))
        .unwrap()
        .set_property(PropertyBool::IsGagged, true);
    let sent = message(&mut w, SA, general(1, "hi"));
    assert_eq!(kinds_to(&sent, SA), [TRANSIENT, CHAT]);
    assert!(kinds_to(&sent, SB).is_empty());
}

#[test]
fn local_speech_outside_the_world_goes_nowhere() {
    // No physics body: `EnqueueBroadcast` and `OnTalk` return at once (the in-world case is
    // empyrean-testkit's `chat.rs`).
    let mut w = world();
    let sent = act(
        &mut w,
        SA,
        &CommunicationTalk {
            message: "hello".to_owned(),
        },
    );
    assert!(!kinds_to(&sent, SA).contains(&HEAR_SPEECH));
    assert!(sent.is_empty());
}

/// Combat messages from a squelched source are dropped.
#[test]
fn combat_messages_from_a_squelched_source_are_dropped() {
    let mut w = world();
    squelch_manager::handle_action_modify_character_squelch(
        &mut w,
        guid(A),
        true,
        B,
        "",
        ChatMessageType::AllChannels,
    );
    start_capture();
    empyrean_world::world_objects::player::send_message_from(
        &mut w,
        guid(A),
        "Bravo hits you!",
        ChatMessageType::CombatEnemy,
        Some(guid(B)),
    );
    assert!(to(&take_sent(), SA).is_empty(), "Bravo is squelched");
    start_capture();
    empyrean_world::world_objects::player::send_message_from(
        &mut w,
        guid(A),
        "Charlie hits you!",
        ChatMessageType::CombatEnemy,
        Some(guid(C)),
    );
    let sent = take_sent();
    assert_eq!(to(&sent, SA).len(), 1);
    assert_eq!(chat_text(&to(&sent, SA)[0]).0, "Charlie hits you!");
}

mod world_notifications {
    use crate::support::social_world::*;

    /// `Player.SendFriendStatusUpdates` over `PlayerManager.GetOnlineInverseFriends` and
    /// `CharacterExtensions.HasAsFriend` (Player_Networking.cs): a player who lists Alpha as a friend
    /// gets the FriendStatusChanged update and "Alpha has come online."; one who does not, nothing.
    #[test]
    fn friends_hear_a_player_come_online() {
        let mut h = H::small();
        let _sa = h.player(A, "Alpha", 3);
        let sb = h.player(B, "Bravo", 3);
        let character =
            h.w.objects
                .get_mut(B)
                .unwrap()
                .player
                .as_mut()
                .unwrap()
                .player
                .character
                .as_mut()
                .unwrap();
        character.character_properties_friend_list.push(
            empyrean_store::models::shard::CharacterPropertiesFriendList {
                character_id: B.full(),
                friend_id: A.full(),
            },
        );

        start_capture();
        empyrean_world::world_objects::player_networking::send_friend_status_updates(
            &mut h.w, A, false, true,
        );
        let msgs = sent();
        assert_eq!(super::fellowship::events_to(&msgs, sb), [0x0021]);
        assert_eq!(chats_to(&msgs, sb), ["Alpha has come online."]);
        assert_eq!(
            msgs.iter().filter(|m| m.0 != sb).count(),
            0,
            "Alpha lists no one"
        );
    }

    /// `Creature.DoWorldBroadcast` (Creature_Networking.cs): `PlayerManager.BroadcastToAll` reaches
    /// every online player.
    #[test]
    fn a_world_broadcast_reaches_everyone_online() {
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        let sb = h.player(B, "Bravo", 3);
        start_capture();
        empyrean_world::world_objects::creature_networking::do_world_broadcast(
            &mut h.w,
            A,
            "Hear ye",
            empyrean_entity::enums::ChatMessageType::Broadcast,
        );
        let msgs = sent();
        assert_eq!(chats_to(&msgs, sa), ["Hear ye"]);
        assert_eq!(chats_to(&msgs, sb), ["Hear ye"]);
    }

    /// `Player.GagsTick` (Player_Tick.cs): the gag notice once; the gag lifts when its duration runs
    /// out, with the ungag notice.
    #[test]
    fn a_gag_counts_down_on_the_heartbeat() {
        let mut h = H::small();
        let sa = h.player(A, "Alpha", 3);
        {
            let o = h.w.objects.get_mut(A).unwrap();
            o.set_is_gagged(true);
            o.set_gag_duration(8.0);
            o.wo.world_object_tick.cached_heartbeat_interval = 5.0;
        }
        start_capture();
        empyrean_world::world_objects::player_tick::gags_tick(&mut h.w, A);
        assert!(h.w.objects.get(A).unwrap().is_gagged());
        assert_eq!(h.w.objects.get(A).unwrap().gag_duration(), 3.0);
        let first = sent().iter().filter(|m| m.0 == sa).count();
        assert!(first >= 1, "the gag notice");

        start_capture();
        empyrean_world::world_objects::player_tick::gags_tick(&mut h.w, A);
        let o = h.w.objects.get(A).unwrap();
        assert!(!o.is_gagged());
        assert_eq!((o.gag_duration(), o.gag_timestamp()), (0.0, 0.0));
        assert!(sent().iter().any(|m| m.0 == sa), "the ungag notice");
    }
}
