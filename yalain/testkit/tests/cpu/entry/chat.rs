//! Chat: sayings, tells, emotes and the chat service's room lines.

use auberean_primitives::Vec3;
use auberean_protocol::login::ChatPacket;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use dereth_protocol::turbine::{decode_incoming, ExtraBlob, IncomingPayload, SendToRoomById};
use yalain_testkit::play::{report, server_call};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::chat;
use yalain_world::chat::function as said;
use yalain_world::graph::Builder;
use yalain_world::requests::function;
use yalain_world::session::function::SHOW_TEXT;
use yalain_world::world::IDLE_MOTION;

/// A text as the client packs one: a string reference holding a literal.
fn literal(text: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        106,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(text.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    Arg::Object(b.graph())
}

fn heard(h: &Harness, b: usize, from: usize, speaker: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::CellObjectCall(c)
                if c.object.id == speaker && c.call.function.0 == said::SAYING)
        })
        .count()
}

/// What a player says reaches it and every player whose view holds it, as a saying on the
/// speaker's object carrying the words as sent; a player in another place (another copy of the
/// hall) does not hear it.
/// Behaviour: server.chat.say-heard-nearby
#[test]
fn a_saying_reaches_the_speaker_and_the_players_within_hearing() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    let (c, _) = h.player_in_world("three", "Farwalker");
    h.bots[c].send(
        true,
        queue::WEENIE,
        &report(
            0x0298_0109,
            Vec3::new(59.0, 240.0, -44.6),
            0,
            IDLE_MOTION,
            0,
        ),
    );
    h.run(20);
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    let call = server_call(function::SAY, &[Arg::Int(0), literal("Well met.")]);
    h.bots[a].send(true, queue::WEENIE, &call);
    h.run(30);
    assert_eq!(heard(&h, a, fa, ida), 1, "the speaker hears itself");
    assert_eq!(heard(&h, b, fb, ida), 1, "a player in its view hears it");
    assert_eq!(
        heard(&h, c, fc, ida),
        0,
        "a player in another copy of the hall does not"
    );
    let words = h
        .world_messages(b, fb)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CellObjectCall(c) => c.call.args,
            _ => None,
        })
        .expect("the saying");
    let auberean_protocol::ClientMessage::ServerCall(sent) = call else {
        panic!()
    };
    assert_eq!(words, sent.args, "the words as the client sent them");
}

fn calls_of(h: &Harness, b: usize, from: usize, f: u32) -> Vec<Vec<Value>> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == f => {
                c.args.and_then(|a| calls::values(&a.0).ok())
            }
            _ => None,
        })
        .collect()
}

fn text_in(v: &Value) -> Option<TextInfo> {
    match v.root() {
        Some(Node::TextInfo(t)) => Some(t.clone()),
        _ => None,
    }
}

/// A tell reaches the player of that name, typed with or without its grammar mark and in any
/// case, as the teller's tell with its id and world name; the teller is shown "You tell ..." with
/// the name as typed and the words; a name no player in the world has is answered "... is
/// unavailable".
/// Behaviour: server.chat.tell-delivered-or-refused
#[test]
fn a_tell_reaches_the_named_player_and_the_teller_is_shown_it_or_told_the_name_is_unavailable() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let tell = server_call(
        function::TELL,
        &[Arg::Int(0), literal("Psst."), literal("babyfare [F]")],
    );
    h.bots[a].send(true, queue::WEENIE, &tell);
    h.run(30);
    let heard = calls_of(&h, b, fb, chat::function::HEAR_TELL);
    assert_eq!(heard.len(), 1, "the named player hears it");
    let v = &heard[0];
    assert_eq!(v[3], Value::Long(ida), "from the teller");
    let teller = text_in(&v[2])
        .and_then(|t| t.literal)
        .map(|l| String::from_utf16_lossy(&l));
    assert_eq!(teller.as_deref(), Some("Elara [Mv]"));
    let words = text_in(&v[1])
        .and_then(|t| t.literal)
        .map(|l| String::from_utf16_lossy(&l));
    assert_eq!(words.as_deref(), Some("Psst."));
    let shown = calls_of(&h, a, fa, SHOW_TEXT);
    let you_tell = shown
        .iter()
        .find(|v| v.get(1) == Some(&Value::Int(chat::text_type::TELL)))
        .and_then(|v| text_in(&v[0]))
        .expect("the teller is shown its tell");
    assert_eq!(
        (you_tell.string_id, you_tell.table, you_tell.variables.len()),
        (chat::strings::YOU_TELL, chat::strings::TABLE, 2)
    );
    let fa = h.bots[a].inbox.len();
    let nobody = server_call(
        function::TELL,
        &[Arg::Int(0), literal("Hello?"), literal("Nobody")],
    );
    h.bots[a].send(true, queue::WEENIE, &nobody);
    h.run(30);
    let refused = calls_of(&h, a, fa, SHOW_TEXT)
        .into_iter()
        .find(|v| v.get(1) == Some(&Value::Int(chat::text_type::TELL_ERROR)))
        .and_then(|v| text_in(&v[0]))
        .expect("refused");
    assert_eq!(refused.string_id, chat::strings::UNAVAILABLE);
}

/// An emote plays its behavior on the emoter for it and every client that knows it, carrying the
/// emote's id and a context, with the table's extra settings.
/// Behaviour: server.chat.emote-plays-its-behavior
#[test]
fn an_emote_plays_its_behavior_on_the_emoter_for_everyone_who_sees_it() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let fb = h.bots[b].inbox.len();
    let wave = chat::EMOTES[2];
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EMOTE, &[Arg::Int(wave.id)]),
    );
    let played = h
        .wait_for(b, fb, |m| match m {
            ServerMessage::PlayBehavior(e) if e.sender.id == ida => Some(e.action.clone()),
            _ => None,
        })
        .expect("seen by the other player");
    assert_eq!(played.behavior, wave.behavior);
    assert_eq!(played.words[12], wave.id, "the emote's id");
    assert_eq!(played.bits & 0x20 != 0, wave.cancels_move);
    assert_eq!(played.bits & 0x100 != 0, wave.fades_children);
    assert!(played.bits & 0x40_0000 != 0, "a context");
}

/// A line sent to a chat room comes back to the sender as the room's line (its name, the words
/// and what its client said of it), then as the answer to its request, and reaches every other
/// player in the world; each line read back by the shared chat-packet reader.
/// Behaviour: server.chat.room-line-echoed-answered-and-relayed
#[test]
fn a_room_line_is_echoed_answered_and_relayed_to_the_other_players() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let packet = SendToRoomById {
        context: 7,
        room: 1,
        text: "Anyone near Arwic?".into(),
        extra: ExtraBlob::Speaker {
            sender: 1,
            chat_type: 100 << 16 | 1,
        },
    }
    .network_packet()
    .expect("packs");
    h.bots[a].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    h.run(30);
    let service = |b: usize, from: usize| -> Vec<IncomingPayload> {
        h.bots[b].inbox[from..]
            .iter()
            .filter(|r| !r.world)
            .filter_map(|r| match r.message() {
                Some(ServerMessage::Chat(ChatPacket(p))) => decode_incoming(
                    &[
                        &u32::try_from(p.len()).expect("small").to_le_bytes()[..],
                        &p,
                    ]
                    .concat(),
                )
                .ok()
                .map(|i| i.payload),
                _ => None,
            })
            .collect()
    };
    let to_a = service(a, fa);
    assert_eq!(to_a.len(), 2, "the echo, then the answer: {to_a:?}");
    let IncomingPayload::RoomEvent(e) = &to_a[0] else {
        panic!("{to_a:?}")
    };
    assert_eq!(
        (e.room, e.name.as_str(), e.text.as_str()),
        (1, "Elara", "Anyone near Arwic?")
    );
    let IncomingPayload::RoomResponse(r) = &to_a[1] else {
        panic!("{to_a:?}")
    };
    assert_eq!((r.context, r.result), (7, 0));
    let to_b = service(b, fb);
    assert!(
        matches!(&to_b[..], [IncomingPayload::RoomEvent(e)] if e.name == "Elara" && e.room == 1),
        "{to_b:?}"
    );
}
