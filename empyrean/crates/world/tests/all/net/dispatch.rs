//! ACE: Source/ACE.Server/Network/Managers/InboundMessageManager.cs::HandleClientMessage
//! Generated handler tables match ACE attributes; InboundMessageManager guards; catch-log-
//! continue; representative handlers driven by protocol-encoded payloads.
//! Fixture: synthetic dats, isolated world state.

use std::collections::BTreeMap;
use std::time::Duration;

use dereth_primitives::ObjectId;
use dereth_protocol::{self as proto, actions::pack_action, write_blob, Message};
use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::not_ported;
use empyrean_dat::{DatDatabaseType, FakeDats};
use empyrean_entity::ObjectGuid;
use empyrean_net::{ClientMessage, SessionId, SessionState};
use empyrean_world::network::game_action::actions::game_action_query_age::calculate_age_message;
use empyrean_world::network::managers::inbound_message_manager::{
    self as imm, dispatch_table, handle_client_message, run_inbound_message_queue, start_trace,
    take_trace, Payload, Read,
};
use empyrean_world::sessions::{self, CharacterSummary, SessionData};
use empyrean_world::World;

#[path = "../../../../entity/tests/all/support/ace_server_opcodes.rs"]
#[allow(dead_code)]
mod ace_server_opcodes;

pub(crate) const S: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};
pub(crate) const PLAYER: u32 = 0x5000_0001;

pub(crate) fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, FakeDats::new().build().expect("empty fake dats"))
}

/// A world with one session in `state`, with a player when `player` is set.
pub(crate) fn world_with(state: SessionState, player: bool) -> World {
    let mut w = world();
    let mut s = SessionData {
        state,
        ..Default::default()
    };
    s.set_account(
        7,
        "acct".to_owned(),
        empyrean_entity::enums::AccessLevel::Player,
    );
    s.set_player(player.then(|| ObjectGuid::new(PLAYER)));
    w.sessions.insert(S, s);
    w
}

/// Hands one message to the manager and runs the inbound queue, as the world loop would.
/// Returns the `not_ported!` sites hit and the values the handler read.
pub(crate) fn dispatch(w: &mut World, data: Vec<u8>) -> (BTreeMap<&'static str, u64>, Vec<Read>) {
    not_ported::take_local();
    start_trace();
    handle_client_message(w, ClientMessage::new(data).expect("opcode"), S);
    run_inbound_message_queue(w);
    (not_ported::take_local(), take_trace())
}

pub(crate) fn action<M: Message>(m: &M) -> Vec<u8> {
    pack_action(0x10, m).expect("encode")
}

fn hits(names: &[&'static str]) -> BTreeMap<&'static str, u64> {
    names.iter().map(|n| (*n, 1)).collect()
}

pub(crate) fn decoded<M: std::fmt::Debug>(m: &M) -> Read {
    Read::Decoded(format!("{m:?}"))
}

/// The two leading reads `GameActionPacket.HandleGameAction` makes.
pub(crate) fn header(opcode: u32) -> Vec<Read> {
    vec![Read::U32(0x10), Read::U32(opcode)]
}

pub(crate) fn with_header(opcode: u32, rest: Vec<Read>) -> Vec<Read> {
    let mut v = header(opcode);
    v.extend(rest);
    v
}

// ---- the tables --------------------------------------------------------------------------------

#[test]
fn tables_hold_exactly_aces_14_messages_and_149_actions() {
    let msgs = dispatch_table::MESSAGE_HANDLERS;
    let acts = dispatch_table::ACTION_HANDLERS;
    assert_eq!(msgs.len(), 14);
    assert_eq!(acts.len(), 149);
    assert!(
        msgs.windows(2)
            .all(|p| p[0].attribute.opcode < p[1].attribute.opcode),
        "sorted, unique"
    );
    assert!(
        acts.windows(2)
            .all(|p| p[0].attribute.opcode < p[1].attribute.opcode),
        "sorted, unique"
    );

    for h in msgs {
        assert!(
            ace_server_opcodes::GAME_MESSAGE_OPCODE.contains(&(h.name, h.attribute.opcode)),
            "GameMessageOpcode.{} = 0x{:04X}",
            h.name,
            h.attribute.opcode
        );
    }
    for h in acts {
        assert!(
            ace_server_opcodes::GAME_ACTION_TYPE.contains(&(h.name, h.attribute.opcode)),
            "GameActionType.{} = 0x{:04X}",
            h.name,
            h.attribute.opcode
        );
    }

    // The registered states, from the attributes.
    let state = |op| {
        imm::message_handler(op)
            .expect("registered")
            .attribute
            .state
    };
    assert_eq!(state(0xF7B1), SessionState::WorldConnected); // GameAction
    assert_eq!(state(0xF656), SessionState::AuthConnected); // CharacterCreate
    assert_eq!(state(0xF653), SessionState::WorldConnected); // CharacterLogOff
    assert_eq!(state(0xF7E6), SessionState::AuthConnected); // DDD_InterrogationResponse
    assert_eq!(state(0xF7E3), SessionState::WorldConnected); // DDD_RequestDataMessage
    assert_eq!(imm::action_handler(0x0036).expect("Use").name, "Use");
    assert!(
        imm::action_handler(0xF61E).is_none(),
        "ACE declares DoMovementCommand but registers no handler"
    );
}

// ---- the guards -------------------------------------------------------------------------------

#[test]
fn a_state_mismatch_drops_the_message_before_it_is_queued() {
    let mut w = world_with(SessionState::AuthConnected, true);
    let use_item = action(&proto::items::InventoryUseEvent {
        object: ObjectId(0x8000_0001),
    });
    handle_client_message(&mut w, ClientMessage::new(use_item).unwrap(), S);
    assert!(
        w.sessions.inbound.inbound_message_queue.is_empty(),
        "GameAction is WorldConnected only"
    );

    let (hit, _) = dispatch(
        &mut w,
        action(&proto::items::InventoryUseEvent {
            object: ObjectId(1),
        }),
    );
    assert!(hit.is_empty());

    // An unknown session has no state and is dropped the same way.
    let mut w = world();
    handle_client_message(
        &mut w,
        ClientMessage::new(write_blob(&proto::login::LoginSendEnterWorldRequest).unwrap()).unwrap(),
        S,
    );
    assert!(w.sessions.inbound.inbound_message_queue.is_empty());
}

#[test]
fn a_null_player_drops_a_game_action() {
    // Player cleared between enqueue and run: the queued WorldConnected handler returns early.
    let mut w = world_with(SessionState::WorldConnected, true);
    not_ported::take_local();
    let data = action(&proto::items::InventoryUseEvent {
        object: ObjectId(1),
    });
    handle_client_message(&mut w, ClientMessage::new(data.clone()).unwrap(), S);
    assert_eq!(w.sessions.inbound.inbound_message_queue.len(), 1);
    w.sessions.get_mut(S).unwrap().set_player(None);
    run_inbound_message_queue(&mut w);
    assert!(not_ported::take_local().is_empty());
    assert!(w.sessions.inbound.inbound_message_queue.is_empty());

    // HandleGameAction's own guard.
    start_trace();
    let mut p = Payload::new(&data);
    p.read_u32().unwrap();
    p.read_u32().unwrap();
    imm::handle_game_action(&mut w, 0x0036, &mut p, S);
    assert!(not_ported::take_local().is_empty());
    assert_eq!(p.remaining(), 4, "the action's payload is not read");
}

#[test]
fn a_failing_handler_is_caught_and_logged_and_the_session_survives() {
    let mut w = world_with(SessionState::WorldConnected, true);

    // A short payload: ACE's EndOfStreamException, caught by the catch-log-continue.
    let mut short = action(&proto::items::InventoryUseEvent {
        object: ObjectId(1),
    });
    short.truncate(14);
    let (hit, _) = dispatch(&mut w, short);
    assert!(hit.is_empty());
    assert_eq!(w.sessions.inbound.handler_exceptions, 1);

    // A panic inside a handler is caught the same way.
    imm::invoke(&mut w, "GameAction", 0x0036, "Use", S, |_| {
        panic!("handler bug")
    });
    assert_eq!(w.sessions.inbound.handler_exceptions, 2);

    // The session keeps working: the next action reaches its handler (QueryHealth, ported by unit
    // 5.20: the session's player is not in `World.objects` here, so it throws at its first read of
    // the player, ACE's NullReferenceException, caught once more).
    let (hit, _) = dispatch(
        &mut w,
        action(&proto::combat::CombatQueryHealth {
            target: ObjectId(2),
        }),
    );
    assert!(hit.is_empty());
    assert_eq!(w.sessions.inbound.handler_exceptions, 3);
}

#[test]
fn unknown_opcodes_are_counted() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let (hit, _) = dispatch(&mut w, vec![0x34, 0x12, 0, 0, 1, 2, 3, 4]);
    assert!(hit.is_empty());
    assert_eq!(w.sessions.inbound.unhandled_messages, 1);

    // A game action with an unregistered sub-type (0xF61E is declared but has no handler).
    let data = proto::actions::pack_action_raw(0x10, proto::Opcode(0xF61E), &[0; 8]);
    let (hit, _) = dispatch(&mut w, data);
    assert!(hit.is_empty());
    assert_eq!(w.sessions.inbound.unhandled_actions, 1);
    assert_eq!(w.sessions.inbound.handler_exceptions, 0);
}

#[test]
fn the_inbound_queue_runs_what_was_queued_at_entry() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let query = action(&proto::combat::CombatQueryHealth {
        target: ObjectId(1),
    });
    handle_client_message(&mut w, ClientMessage::new(query.clone()).unwrap(), S);
    handle_client_message(&mut w, ClientMessage::new(query).unwrap(), S);
    not_ported::take_local();
    run_inbound_message_queue(&mut w);
    assert_eq!(w.sessions.inbound.handler_exceptions, 2);
    assert!(w.sessions.inbound.inbound_message_queue.is_empty());
}

// ---- representative handlers --------------------------------------------------------------------

#[test]
fn use_item_decodes_the_guid_and_calls_player_handle_action_use_item() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let m = proto::items::InventoryUseEvent {
        object: ObjectId(0x8000_1234),
    };
    let (_, reads) = dispatch(&mut w, action(&m));
    assert_eq!(reads, with_header(0x0036, vec![decoded(&m)]));
    assert_eq!(w.sessions.inbound.handler_exceptions, 1);
}

#[test]
fn melee_attack_and_targeted_cast() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let m = proto::combat::CombatTargetedMeleeAttack {
        target: ObjectId(0x8000_0002),
        attack_height: 2,
        power_level: 0.75,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0008, vec![decoded(&m)]));

    let m = proto::combat::MagicCastTargetedSpell {
        target: ObjectId(0x8000_0003),
        spell_id: 1782,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x004A, vec![decoded(&m)]));
}

#[test]
fn drop_pick_up_give_and_split() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let m = proto::items::InventoryDropItem {
        item: ObjectId(0x8000_0010),
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(
        hit,
        hits(&[]),
        "`Teleporting` is the real field now: the first read of the absent player"
    );
    assert_eq!(reads, with_header(0x001B, vec![decoded(&m)]));

    let m = proto::items::InventoryPutItemInContainer {
        item: ObjectId(0x8000_0010),
        container: ObjectId(PLAYER),
        slot: 3,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(
        hit,
        hits(&[]),
        "Reading suicide state requires a live player"
    );
    assert_eq!(reads, with_header(0x0019, vec![decoded(&m)]));

    let m = proto::items::InventoryGiveObjectRequest {
        target: ObjectId(0x8000_0020),
        item: ObjectId(0x8000_0010),
        amount: 5,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(
        hit,
        hits(&[]),
        "`Teleporting` is the real field now: the first read of the absent player"
    );
    assert_eq!(reads, with_header(0x00CD, vec![decoded(&m)]));

    let m = proto::items::InventoryStackableSplitToContainer {
        stack: ObjectId(0x8000_0010),
        container: ObjectId(PLAYER),
        slot: 0,
        amount: 10,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0055, vec![decoded(&m)]));
}

#[test]
fn talk_reads_the_clients_string_and_routes_speech_or_commands() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let mut p = empyrean_world::world_objects::world_object::WorldObject::allocate(
        empyrean_world::dispatch::Class::Player,
    );
    p.guid = ObjectGuid::new(PLAYER);
    w.objects.insert(p).expect("fresh");
    let m = proto::comms::CommunicationTalk {
        message: "hello".to_owned(),
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0015, vec![decoded(&m)]));

    let (hit, _) = dispatch(
        &mut w,
        action(&proto::comms::CommunicationTalk {
            message: "@help".to_owned(),
        }),
    );
    assert_eq!(
        hit,
        hits(&[]),
        "the command path is empyrean-command's (installed by CommandManager.Initialize)"
    );
}

#[test]
fn combat_mode_confirmation_shortcut_and_allegiance_ban() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let m = proto::combat::CombatChangeCombatMode { combat_mode: 2 };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0053, vec![decoded(&m)]));

    let m = proto::comms::CharacterConfirmationResponse {
        confirmation_type: 5,
        context_id: 9,
        accepted: 1,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0275, vec![decoded(&m)]));

    let mut p = empyrean_world::world_objects::world_object::WorldObject::allocate(
        empyrean_world::dispatch::Class::Player,
    );
    p.guid = ObjectGuid::new(PLAYER);
    p.player.as_mut().unwrap().player.character = Some(empyrean_store::models::shard::Character {
        id: PLAYER,
        ..Default::default()
    });
    w.objects.insert(p).expect("fresh");
    let m = proto::login::CharacterAddShortCut {
        shortcut: proto::login::ShortCutData {
            index: 2,
            object_id: ObjectId(0x8000_0030),
            spell_id: 0,
        },
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x019C, vec![decoded(&m)]));
    let character = w
        .objects
        .get(ObjectGuid::new(PLAYER))
        .unwrap()
        .player
        .as_ref()
        .unwrap()
        .player
        .character
        .clone()
        .unwrap();
    assert_eq!(
        character
            .character_properties_shortcut_bar
            .iter()
            .map(|s| (s.shortcut_bar_index, s.shortcut_object_id))
            .collect::<Vec<_>>(),
        [(3, 0x8000_0030)]
    );

    let m = proto::social::AllegianceAddAllegianceBan {
        name: "Someone".to_owned(),
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x02A1, vec![decoded(&m)]));
}

/// V351 (retail): Buy is read as the client writes it — 24-bit amounts and the trailing
/// alternate-currency dword — and Sell likewise, without the currency.
#[test]
fn buy_and_sell_read_the_clients_layout() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let item = |amount, iid| proto::trade::ItemProfile {
        amount,
        iid: ObjectId(iid),
        pwd: None,
    };
    let m = proto::trade::VendorBuy {
        vendor_id: ObjectId(0x8000_0040),
        items: vec![item(3, 0x8000_0041), item(1, 0x8000_0042)],
        alternate_currency_id: 0,
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x005F, vec![decoded(&m)]));

    let m = proto::trade::VendorSell {
        vendor_id: ObjectId(0x8000_0040),
        items: vec![item(2, 0x8000_0043)],
    };
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x0060, vec![decoded(&m)]));
}

/// AcceptTrade reads the whole packed trade the client sends, both item lists included, and still
/// acts on the server's own trade state only.
#[test]
fn accept_trade_reads_both_item_lists() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let profile = |iid| proto::types::ContentProfile {
        iid: ObjectId(iid),
        container_properties: 0,
    };
    let m = proto::trade::TradeAcceptTradeRequest(proto::trade::Trade {
        self_list: vec![profile(0x8000_0051), profile(0x8000_0052)],
        partner_list: vec![profile(0x8000_0061)],
        partner: ObjectId(0x5000_0002),
        stamp: 12.5,
        status: 3,
        initiator: 1,
        accepted: 1,
        partner_accepted: 0,
    });
    let (hit, reads) = dispatch(&mut w, action(&m));
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, with_header(0x01FA, vec![decoded(&m)]));
}

#[test]
fn a_move_to_state_reads_nothing_during_a_pk_logout() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let o = empyrean_world::world_objects::world_object::WorldObject {
        guid: ObjectGuid::new(PLAYER),
        player: Some(Box::default()),
        ..Default::default()
    };
    assert!(w.objects.insert(o).is_ok());
    empyrean_world::world_objects::player::fields_mut(&mut w, ObjectGuid::new(PLAYER)).pk_logout =
        true;
    let data = proto::actions::pack_action_raw(0x10, proto::Opcode(0xF61C), &[0; 8]);
    let (hit, reads) = dispatch(&mut w, data);
    assert!(hit.is_empty(), "{hit:?}");
    assert_eq!(reads, header(0xF61C));
}

#[test]
fn log_off_and_character_messages() {
    let mut w = world_with(SessionState::WorldConnected, true);
    let m = proto::login::LoginExecuteLogOffRequest {
        character: ObjectId(PLAYER),
    };
    let (hit, reads) = dispatch(&mut w, write_blob(&m).unwrap());
    assert_eq!(hit, hits(&[]), "PK logout state is read from the player");
    assert!(reads.is_empty(), "ACE reads nothing from CharacterLogOff");

    let mut w = world_with(SessionState::AuthConnected, false);
    let m = proto::login::LoginSendEnterWorld {
        character: ObjectId(PLAYER),
        account: "acct".to_owned(),
    };
    let (hit, reads) = dispatch(&mut w, write_blob(&m).unwrap());
    assert_eq!(hit, hits(&[]));
    assert_eq!(reads, vec![Read::U32(PLAYER), Read::Str("acct".to_owned())]);
}

// ---- the payload reader ------------------------------------------------------------------------

/// V232: the client's packed string, read as the client writes it.
#[test]
fn read_string16l_reads_the_clients_packed_string() {
    // Length 3, "abc", then padding to the next multiple of 4 of the message position.
    let data = [
        0, 0, 0, 0, 3, 0, b'a', b'b', b'c', 0xAA, 0xAA, 0xAA, 7, 0, 0, 0,
    ];
    let mut p = Payload::new(&data);
    assert_eq!(p.read_string16l().unwrap(), "abc");
    assert_eq!(p.read_u32().unwrap(), 7);

    // CP-1252 bytes are CP-1252 text (ACE read U+FFFD here), and what follows still reads.
    let data = [
        0, 0, 0, 0, 3, 0, 0xE9, b'a', b'b', 0xAA, 0xAA, 0xAA, 9, 0, 0, 0,
    ];
    let mut p = Payload::new(&data);
    assert_eq!(p.read_string16l().unwrap(), "\u{e9}ab");
    assert_eq!(p.read_u32().unwrap(), 9);
    // Ending on such a byte is fine too (ACE threw).
    let data = [0, 0, 0, 0, 2, 0, b'a', 0xE9, 9, 0, 0, 0];
    let mut p = Payload::new(&data);
    assert_eq!(p.read_string16l().unwrap(), "a\u{e9}");
    assert_eq!(p.read_u32().unwrap(), 9);

    // A trailing NUL is a packed terminator, dropped as the client drops it.
    let data = [0, 0, 0, 0, 2, 0, b'a', 0];
    assert_eq!(Payload::new(&data).read_string16l().unwrap(), "a");
}

#[test]
fn payload_reads_past_the_end_fail_but_skips_do_not() {
    let data = [0, 0, 0, 0, 1, 0, 0, 0];
    let mut p = Payload::new(&data);
    assert_eq!(p.read_u32().unwrap(), 1);
    p.skip(8);
    p.align();
    assert_eq!(p.remaining(), 0);
    assert!(p.read_byte().is_err());
    assert!(p.read_bytes(4).is_empty(), "ReadBytes returns what is left");

    let mut p = Payload::new(&[0, 0, 0, 0, 1]);
    p.read_byte().unwrap();
    p.align();
    assert_eq!(p.position(), 8);
}

// ---- ported helpers and the session game half ----------------------------------------------------

/// Retail's /age text (V312/V333, V312): `[Nmo] [Nd] [Nh] [Nm] Ns`, 30-day months, no years or weeks,
/// zero units left out except the seconds. Every full answer quoted from the retail sessions, at the
/// age in seconds it spells out.
#[test]
fn calculate_age_message_is_retails() {
    const MO: i32 = 30 * 86_400;
    const D: i32 = 86_400;
    const H: i32 = 3_600;
    const M: i32 = 60;
    for (age, text) in [
        (5 * H + 44 * M, "5h 44m 0s"),
        (H + 14 * M, "1h 14m 0s"),
        (H + 1, "1h 1s"),
        (15 * M + 43, "15m 43s"),
        (10 * D + 5 * H + 58 * M + 46, "10d 5h 58m 46s"),
        (
            3 * MO + 16 * D + 21 * H + 12 * M + 40,
            "3mo 16d 21h 12m 40s",
        ),
        (4 * MO + 22 * D + 2 * H + 42, "4mo 22d 2h 42s"),
        (5 * MO + 7 * D + 49 * M + 21, "5mo 7d 49m 21s"),
        (7 * MO + 2 * D + 16 * H + 27 * M + 2, "7mo 2d 16h 27m 2s"),
        (
            26 * MO + 19 * D + 2 * H + 58 * M + 14,
            "26mo 19d 2h 58m 14s",
        ),
        (
            26 * MO + 19 * D + 4 * H + 39 * M + 46,
            "26mo 19d 4h 39m 46s",
        ),
    ] {
        assert_eq!(calculate_age_message(age), text, "{age} s");
    }
    // The same answers at the seconds they spell out (the sessions checked against Age).
    assert_eq!(calculate_age_message(3_601), "1h 1s");
    assert_eq!(calculate_age_message(943), "15m 43s");
    assert_eq!(calculate_age_message(885_526), "10d 5h 58m 46s");
    assert_eq!(calculate_age_message(9_234_760), "3mo 16d 21h 12m 40s");
    assert_eq!(calculate_age_message(18_376_022), "7mo 2d 16h 27m 2s");
    assert_eq!(calculate_age_message(13_567_761), "5mo 7d 49m 21s");
    // Days past a week stay days (`18d`, `22d`, `29d`, `1mo 29d` in the sessions); no years.
    assert_eq!(calculate_age_message(18 * D + 5), "18d 5s");
    assert_eq!(calculate_age_message(MO + 29 * D + 7), "1mo 29d 7s");
    assert_eq!(calculate_age_message(364 * D), "12mo 4d 0s");
    // Seconds are always printed; an age of nothing (or below it) is `0s`.
    assert_eq!(calculate_age_message(0), "0s");
    assert_eq!(calculate_age_message(59), "59s");
    assert_eq!(calculate_age_message(H), "1h 0s");
    assert_eq!(calculate_age_message(2 * MO + 5), "2mo 5s");
    assert_eq!(calculate_age_message(-5), "0s");
}

#[test]
fn session_game_half() {
    let mut s = SessionData::default();
    assert_eq!(s.state, SessionState::AuthLoginRequest);
    assert_eq!(s.log_off_request_time, DotNetDateTime::MIN_VALUE);
    s.init_session_for_world_login();
    assert_eq!(s.game_event_sequence, 1);
    assert!(s.add_to_ddd_queue(0x0E00_0001, DatDatabaseType::Portal));
    assert!(s.add_to_ddd_queue(0x1234_FFFE, DatDatabaseType::Cell));
    let q: Vec<_> = s.ddd_data_queue.as_ref().unwrap().iter().copied().collect();
    assert_eq!(
        q,
        vec![
            (0x0E00_0001, DatDatabaseType::Portal),
            (0x1234_FFFE, DatDatabaseType::Cell)
        ]
    );

    let mut w = world_with(SessionState::AuthConnected, false);
    w.sessions
        .get_mut(S)
        .unwrap()
        .characters
        .push(CharacterSummary {
            id: 1,
            ..Default::default()
        });
    not_ported::take_local();
    sessions::update_characters(
        &mut w,
        S,
        vec![CharacterSummary {
            id: 2,
            name: "b".to_owned(),
            ..Default::default()
        }],
    );
    assert_eq!(
        w.sessions
            .get(S)
            .unwrap()
            .characters
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(
        not_ported::take_local(),
        hits(&[]),
        "Expired characters are checked for deletion"
    );
}
