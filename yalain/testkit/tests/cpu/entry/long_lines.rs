//! How long a line of talk may be: the longest a client can send is heard whole and goes out at
//! once, and one past it is heard by nobody.

use auberean_protocol::login::ChatPacket;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use dereth_protocol::turbine::{ExtraBlob, SendToRoomById};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::graph::Builder;
use yalain_world::requests::function;
use yalain_world::{chat, fellowship};

/// The most characters a line a client can send holds: its chat entry's 512, every one escaped,
/// through an alias of as many.
const LONGEST: usize = 2048;

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

fn string(text: &str) -> Arg {
    let mut b = Builder::new();
    #[allow(clippy::cast_possible_truncation)]
    b.native(
        native_type::WP_STRING as u16,
        NativeObject::Text(text.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

#[derive(Clone, Copy)]
enum Kind {
    Say,
    Tell,
    Emote,
    Room,
}

/// A line of talk of this kind, as the client sends it.
fn send(h: &mut Harness, b: usize, kind: Kind, words: &str) {
    let call = |f, args: &[Arg]| server_call(f, args);
    match kind {
        Kind::Say => {
            let m = call(function::SAY, &[Arg::Int(0), literal(words)]);
            h.bots[b].send(true, queue::WEENIE, &m);
        }
        Kind::Tell => {
            let m = call(
                function::TELL,
                &[Arg::Int(0), literal(words), literal("Babyfare")],
            );
            h.bots[b].send(true, queue::WEENIE, &m);
        }
        Kind::Emote => {
            let m = call(function::TYPED_EMOTE, &[string(words)]);
            h.bots[b].send(true, queue::WEENIE, &m);
        }
        Kind::Room => {
            let packet = SendToRoomById {
                context: 7,
                room: chat::GENERAL_ROOM,
                text: words.into(),
                extra: ExtraBlob::Speaker {
                    sender: 1,
                    chat_type: 100 << 16 | 1,
                },
            }
            .network_packet()
            .expect("packs");
            h.bots[b].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
        }
    }
}

/// The units of the words in anything a client took since `from`, by looking for the words.
fn holds(h: &Harness, b: usize, from: usize, words: &str) -> bool {
    let wanted: Vec<u8> = words.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let scrambled = auberean_primitives::scramble::conceal(&wanted);
    h.bots[b].inbox[from..].iter().any(|r| {
        [&wanted, &scrambled]
            .iter()
            .any(|w| r.bytes.windows(w.len()).any(|x| x == w.as_slice()))
    })
}

const KINDS: [Kind; 4] = [Kind::Say, Kind::Tell, Kind::Emote, Kind::Room];

/// The longest line a client can send is heard whole, of each kind, and goes out at once: nothing
/// a listener is told next waits behind it.
/// Behaviour: server.chat.longest-line-heard-at-once
#[test]
fn the_longest_line_a_client_can_send_is_heard_whole_and_holds_nothing_back() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == idb)
        .expect("in the world")
        .0;
    let login = h.server.players[&world].login;
    for kind in KINDS {
        let words = "x".repeat(LONGEST);
        let from = h.bots[b].inbox.len();
        send(&mut h, a, kind, &words);
        for _ in 0..4 {
            h.step();
            assert_eq!(
                h.net.large_waiting(world),
                0,
                "nothing waits on the world side"
            );
            assert_eq!(
                h.net.large_waiting(login),
                0,
                "nothing waits on the login side"
            );
        }
        h.run(20);
        assert!(holds(&h, b, from, &words), "heard whole");
    }
}

/// A line past the most a line may carry reaches nobody, whichever way it is said, and however it
/// carries its bytes; its speaker is told, and may go on talking.
/// Behaviour: server.chat.line-too-long-goes-nowhere
#[test]
fn a_line_past_the_longest_reaches_nobody_and_its_speaker_is_told() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    for kind in KINDS {
        let words = "y".repeat(chat::MOST_TALK_BYTES / 2 + 1);
        let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
        send(&mut h, a, kind, &words);
        h.run(20);
        assert!(
            !holds(&h, b, from_b, &words),
            "a listener hears nothing of it"
        );
        assert!(!holds(&h, a, from_a, &words), "nor its speaker");
        let told = h.world_messages(a, from_a).iter().any(|m| {
            matches!(m, ServerMessage::Notice(n)
            if n.text.literal.as_ref().is_some_and(|l| {
                String::from_utf16_lossy(&l.0).trim_end() == chat::TOO_LONG
            }))
        });
        let refused = h.bots[a].inbox[from_a..]
            .iter()
            .any(|r| !r.world && r.queue == chat::CHAT_QUEUE);
        match kind {
            Kind::Room => assert!(refused, "the room's answer refuses it"),
            _ => assert!(told, "told"),
        }
        let from_b = h.bots[b].inbox.len();
        send(&mut h, a, kind, "still here");
        h.run(20);
        assert!(holds(&h, b, from_b, "still here"), "may go on talking");
    }
    // Words in the part of the call a listener is passed unread count as much.
    let padding = literal(&"z".repeat(chat::MOST_TALK_BYTES / 2 + 1));
    let from = h.bots[b].inbox.len();
    let m = server_call(function::SAY, &[Arg::Int(0), literal("hello"), padding]);
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(20);
    assert!(!holds(&h, b, from, "hello"), "the whole call is counted");
}

/// A fellowship's name no chat entry could hold is refused, with the words for a name it can't take.
/// Behaviour: server.fellowship.name-past-the-longest-refused
#[test]
fn a_fellowship_name_past_the_chat_entrys_length_is_refused() {
    assert!(fellowship::name_ok(
        &"f".repeat(fellowship::MOST_NAME_CHARS)
    ));
    assert!(!fellowship::name_ok(
        &"f".repeat(fellowship::MOST_NAME_CHARS + 1)
    ));
}
