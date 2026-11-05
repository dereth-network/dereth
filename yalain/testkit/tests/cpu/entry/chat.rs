//! Chat: sayings, tells, emotes, typed emotes and the chat service's room lines.

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

/// A tell that reaches no player in the world is kept for the friends service, as told, with its
/// teller; one that reached a player, or from a gagged teller, is not.
/// Behaviour: server.network.tell-away-linked
#[test]
fn a_tell_that_reaches_no_player_in_the_world_is_kept_for_the_friends_service() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (_, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let tell = |to: &str| {
        server_call(
            function::TELL,
            &[Arg::Int(0), literal("Psst."), literal(to)],
        )
    };
    h.bots[a].send(true, queue::WEENIE, &tell("Babyfare"));
    h.run(30);
    assert!(h.server.unheard_tells.is_empty());
    h.bots[a].send(true, queue::WEENIE, &tell("Nobody [F]"));
    h.run(30);
    assert_eq!(
        std::mem::take(&mut h.server.unheard_tells),
        vec![chat::UnheardTell {
            character: ida,
            name: "Elara".into(),
            tellee: "nobody".into(),
            text: "Psst.".into(),
        }]
    );
    let gag = yalain_world::admin::parse("gag elara")
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    yalain_world::admin::run(
        &mut h.server,
        &mut h.net,
        clock,
        &yalain_world::admin::Issuer::Console,
        &gag,
    );
    h.bots[a].send(true, queue::WEENIE, &tell("Nobody"));
    h.run(30);
    assert!(h.server.unheard_tells.is_empty());
}

/// A tell finds a character by its name as a client shows it, suffix and grammar mark and all, or by
/// its name alone; where one character's name is another's name and suffix, it goes to the first.
/// Behaviour: server.chat.tell-finds-a-suffixed-name
#[test]
fn a_tell_finds_a_character_by_its_name_with_or_without_its_suffix() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    let (c, _) = h.player_in_world("three", "Babyfare Bold");
    h.run(10);
    let suffix = |h: &mut Harness, id: u64, words: &str| {
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == id)
            .expect("in the world");
        words.clone_into(&mut p.play.suffix);
    };
    let told = |h: &mut Harness, name: &str, who: usize| {
        let from = h.bots[who].inbox.len();
        let tell = server_call(
            function::TELL,
            &[Arg::Int(0), literal("Psst."), literal(name)],
        );
        h.bots[a].send(true, queue::WEENIE, &tell);
        h.run(30);
        calls_of(h, who, from, chat::function::HEAR_TELL).len()
    };
    suffix(&mut h, idb, "the Grand");
    assert_eq!(told(&mut h, "Babyfare the Grand [F]", b), 1, "as shown");
    assert_eq!(told(&mut h, "babyfare the grand", b), 1, "without its mark");
    assert_eq!(told(&mut h, "Babyfare", b), 1, "its name alone");
    assert_eq!(
        told(&mut h, "Babyfare the", b),
        0,
        "part of a suffix is no name"
    );
    h.server
        .players
        .values_mut()
        .find(|p| p.character == idb)
        .expect("in the world")
        .admin
        .cloaked = true;
    assert_eq!(told(&mut h, "Babyfare the Grand", b), 0, "one it can't see");
    h.server
        .players
        .values_mut()
        .find(|p| p.character == idb)
        .expect("in the world")
        .admin
        .cloaked = false;
    suffix(&mut h, idb, "Bold");
    assert_eq!(told(&mut h, "Babyfare Bold", c), 1, "the name itself first");
    assert_eq!(told(&mut h, "Babyfare Bold", b), 0, "not the suffixed one");
    suffix(&mut h, idb, "");
    assert_eq!(told(&mut h, "Babyfare Bold", c), 1, "a suffix gone");
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

/// A string as the client packs one.
fn string(text: &str) -> Arg {
    let mut b = Builder::new();
    b.native(0x76, NativeObject::Text(text.encode_utf16().collect()));
    Arg::Object(b.graph())
}

/// The typed emotes a client heard about an emoter: (id, name, words) each.
fn typed_emotes(h: &Harness, b: usize, from: usize, emoter: u64) -> Vec<(u64, String, String)> {
    let text = |v: Option<&Value>| match v.and_then(Value::root) {
        Some(Node::Text(t)) => t.clone(),
        _ => String::new(),
    };
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CellObjectCall(c)
                if c.object.id == emoter && c.call.function.0 == said::HEAR_TYPED_EMOTE =>
            {
                let v = calls::values(&c.call.args?.0).ok()?;
                let id = match v.get(2) {
                    Some(Value::Long(l)) => *l,
                    _ => 0,
                };
                Some((id, text(v.get(1)), text(v.first())))
            }
            _ => None,
        })
        .collect()
}

/// A typed emote reaches the emoter and every player whose view holds it, on the emoter's object,
/// with its id, its name alone and the words as typed; a player in another place does not hear
/// it, and a gagged player's reaches nobody.
/// Behaviour: server.chat.typed-emote-heard-nearby
#[test]
fn a_typed_emote_reaches_the_emoter_and_the_players_who_see_it_with_its_name_and_words() {
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
    let typed = server_call(function::TYPED_EMOTE, &[string("waves farewell")]);
    h.bots[a].send(true, queue::WEENIE, &typed);
    h.run(30);
    let want = vec![(ida, "Elara".to_string(), "waves farewell".to_string())];
    assert_eq!(
        typed_emotes(&h, a, fa, ida),
        want,
        "the emoter hears its own"
    );
    assert_eq!(
        typed_emotes(&h, b, fb, ida),
        want,
        "a player in its view hears it"
    );
    assert!(
        typed_emotes(&h, c, fc, ida).is_empty(),
        "a player in another copy of the hall does not"
    );
    let gag = yalain_world::admin::parse("gag elara")
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    yalain_world::admin::run(
        &mut h.server,
        &mut h.net,
        clock,
        &yalain_world::admin::Issuer::Console,
        &gag,
    );
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(true, queue::WEENIE, &typed);
    h.run(30);
    assert!(typed_emotes(&h, a, fa, ida).is_empty() && typed_emotes(&h, b, fb, ida).is_empty());
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

/// A line to a room the speaker may not use (one that is no room of the world's, or a kingdom's
/// room the speaker is not of) is answered with a refusal, and nobody hears it: not the speaker,
/// not the other players.
/// Behaviour: server.chat.room-line-to-a-room-not-the-speakers-is-refused
#[test]
fn a_line_to_a_room_the_speaker_may_not_use_is_refused_and_reaches_nobody() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    // A room no channel names, and the first kingdom's room (the speaker is of none).
    for (context, room) in [(3, 999), (4, 3)] {
        let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
        let packet = SendToRoomById {
            context,
            room,
            text: "hello?".into(),
            extra: ExtraBlob::Other(vec![1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0xD4, 0]),
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
        assert!(
            matches!(&to_a[..], [IncomingPayload::RoomResponse(r)]
                if r.context == context && r.result == chat::REFUSED),
            "room {room}: refused, and no echo: {to_a:?}"
        );
        assert_eq!(
            service(b, fb),
            Vec::new(),
            "room {room}: nobody else hears it"
        );
    }
}

/// The Dereth Network's command said aloud (`@dn`, or `@dn` and words, in any case) is heard by
/// nobody, the speaker included, and is kept for the friends service with the speaker and its
/// words; a line that only begins with the letters is said as any other.
/// Behaviour: server.friends.dn-said-in-chat
#[test]
fn the_networks_command_said_aloud_is_heard_by_nobody_and_kept_for_the_friends_service() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    for line in ["@dn on", "@DN", "  @dn help me"] {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::SAY, &[Arg::Int(0), literal(line)]),
        );
    }
    h.run(30);
    assert_eq!(heard(&h, a, fa, ida), 0, "not even the speaker");
    assert_eq!(heard(&h, b, fb, ida), 0);
    let kept: Vec<(u64, String, String)> = h
        .server
        .network_commands
        .drain(..)
        .map(|c| (c.character, c.name, c.words))
        .collect();
    assert_eq!(
        kept,
        vec![
            (ida, "Elara".into(), "on".into()),
            (ida, "Elara".into(), String::new()),
            (ida, "Elara".into(), "help me".into()),
        ]
    );
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::SAY, &[Arg::Int(0), literal("@dnd is a word")]),
    );
    h.run(30);
    assert_eq!(heard(&h, b, fb, ida), 1, "said as any other line");
    assert!(h.server.network_commands.is_empty());
    assert_eq!(chat::network_words("@dn  off "), Some("off"));
    assert_eq!(chat::network_words("hello @dn"), None);
}

/// `@derethnetwork` is `@dn` spelled out, and `@dng <text>` is `@dn global <text>` made short: each
/// is heard by nobody and kept with its words. A word that only begins with the letters is said.
/// Behaviour: server.friends.dn-spelled-out-and-dng
#[test]
fn derethnetwork_and_dng_are_the_networks_command_under_other_names() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let fb = h.bots[b].inbox.len();
    for line in [
        "@DerethNetwork on",
        "@derethnetwork",
        "@dng hello all",
        "@DNG",
    ] {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::SAY, &[Arg::Int(0), literal(line)]),
        );
    }
    h.run(30);
    assert_eq!(heard(&h, b, fb, ida), 0);
    let kept: Vec<String> = h
        .server
        .network_commands
        .drain(..)
        .map(|c| c.words)
        .collect();
    assert_eq!(kept, ["on", "", "global hello all", "global"]);
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::SAY, &[Arg::Int(0), literal("@dngs is a word")]),
    );
    h.run(30);
    assert_eq!(heard(&h, b, fb, ida), 1, "said as any other line");
    assert!(h.server.network_commands.is_empty());
}

/// The Dereth Network's command said into a chat room is answered as the room's request, and
/// reaches nobody, not even as the speaker's own echo.
/// Behaviour: server.friends.dn-said-in-chat
#[test]
fn the_networks_command_said_into_a_room_reaches_nobody() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let packet = SendToRoomById {
        context: 9,
        room: 1,
        text: "@dn off".into(),
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
    assert!(
        matches!(&to_a[..], [IncomingPayload::RoomResponse(r)] if r.context == 9 && r.result == 0),
        "the answer alone: {to_a:?}"
    );
    assert!(service(b, fb).is_empty(), "nobody hears it");
    assert_eq!(
        h.server
            .network_commands
            .iter()
            .map(|c| (c.character, c.words.as_str()))
            .collect::<Vec<_>>(),
        vec![(ida, "off")]
    );
}

/// A player who ignores another on the friends service never hears its tells by the game's own
/// tell either; the teller is shown its tell as usual.
/// Behaviour: server.friends.ignored-tells-not-shown
#[test]
fn a_game_tell_from_a_player_the_hearer_ignores_is_not_heard() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    h.server
        .ignoring
        .insert(idb, std::iter::once("elara".to_owned()).collect());
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(
            function::TELL,
            &[Arg::Int(0), literal("Psst."), literal("Babyfare")],
        ),
    );
    h.run(30);
    assert!(calls_of(&h, b, fb, chat::function::HEAR_TELL).is_empty());
    assert!(
        calls_of(&h, a, fa, SHOW_TEXT)
            .iter()
            .any(|v| v.get(1) == Some(&Value::Int(chat::text_type::TELL))),
        "the teller is shown its tell"
    );
    h.server.ignoring.clear();
    let fb = h.bots[b].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(
            function::TELL,
            &[Arg::Int(0), literal("Psst."), literal("Babyfare")],
        ),
    );
    h.run(30);
    assert_eq!(calls_of(&h, b, fb, chat::function::HEAR_TELL).len(), 1);
}

/// A system line the world writes itself (the network's answers, the friends service's lines)
/// ends its line, as the client's own strings do, so that lines in a row do not run together;
/// one that already ends its line is not given a second.
/// Behaviour: server.chat.system-line-ends-its-line
#[test]
fn a_system_line_the_world_writes_ends_its_line_as_the_clients_strings_do() {
    let mut h = Harness::new();
    let (b, id) = h.player_in_world("one", "Elara");
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(c, _)| *c)
        .expect("in the world");
    let from = h.bots[b].inbox.len();
    yalain_world::login::notice(&mut h.net, world, "You are not on the Dereth Network.");
    yalain_world::login::notice(&mut h.net, world, "Say @dn help for the commands.\n");
    h.run(10);
    let lines: Vec<String> = h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::Notice(n)) => n.text.literal.map(|l| l.text()),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines,
        [
            "You are not on the Dereth Network.\n",
            "Say @dn help for the commands.\n"
        ]
    );
}

/// What a client was sent by the chat service since `from`, read.
fn service_packets(h: &Harness, b: usize, from: usize) -> Vec<IncomingPayload> {
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
}

/// A note as the recorded clients attach it: its version, a zero word, its flags and its speaker's
/// meta letters. These are the four forms 19,839 recorded lines carry.
/// Behaviour: server.chat.speaker-note-as-recorded
#[test]
fn a_speakers_note_is_what_the_recorded_clients_attach() {
    let head = [1, 0, 0, 0, 0, 0, 0, 0];
    for (letters, rest) in [
        ("M", &[0x01, 0x00, 0xD4, 0x00][..]),
        ("F", &[0x01, 0x00, 0x64, 0x00][..]),
        ("Mv", &[0x02, 0x00, 0xEC, 0x92, 0xE3, 0xE5, 0x00, 0x00][..]),
        ("Fv", &[0x02, 0x00, 0x5C, 0x92, 0xE3, 0xE5, 0x00, 0x00][..]),
    ] {
        assert_eq!(chat::speaker_note(letters), [&head[..], rest].concat());
    }
    use yalain_world::chargen::FEMALE;
    assert_eq!(chat::meta_letters("Elara", FEMALE), "Fv");
    assert_eq!(chat::meta_letters("Babyfare", 0), "M");
    let none = chat::speaker_note("");
    assert_eq!(
        none,
        [&head[..], &[0, 0, 0, 0]].concat(),
        "no letters, as a note"
    );
}

/// A line from elsewhere reaches every player in the world as the room's own line under the name it
/// is given, with no meta letters, except a player the rule says does not hear it. It is kept for
/// nobody, and it answers no request.
/// Behaviour: server.chat.line-from-afar-reaches-the-room
#[test]
fn a_line_from_afar_reaches_the_players_in_the_room_and_only_those_it_lets_hear() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (b, babyfare) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    chat::hear_from_afar(
        &h.server,
        &mut h.net,
        chat::PVP_ROOM,
        "Bob@coldeve",
        "Ballroom at 8!",
        |id, name| id != babyfare && name != "Babyfare",
    );
    h.run(20);
    let to_a = service_packets(&h, a, fa);
    let [IncomingPayload::RoomEvent(e)] = &to_a[..] else {
        panic!("{to_a:?}")
    };
    assert_eq!(
        (e.room, e.name.as_str(), e.text.as_str()),
        (2, "Bob@coldeve", "Ballroom at 8!")
    );
    assert_eq!(e.extra, chat::speaker_note(""));
    assert_eq!(service_packets(&h, b, fb), []);
    assert_eq!(h.server.room_lines, [], "it goes no further");
}

/// `@dng` says a line as the client would: it comes back to its speaker as the room's own line
/// under its name and note, reaches the others, and is kept for the world's link; no request is
/// answered, since the client made none. A gagged character is told it may not talk, and nobody
/// hears it.
/// Behaviour: server.chat.said-from-anywhere-as-a-room-line
#[test]
fn a_line_said_from_anywhere_is_the_rooms_own_line_and_no_request_is_answered() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    chat::say_in_room(
        &mut h.server,
        &mut h.net,
        elara,
        chat::GENERAL_ROOM,
        "Anyone near Arwic?",
    );
    h.run(20);
    for (bot, from) in [(a, fa), (b, fb)] {
        let got = service_packets(&h, bot, from);
        let [IncomingPayload::RoomEvent(e)] = &got[..] else {
            panic!("the room's line and nothing else: {got:?}")
        };
        assert_eq!(
            (e.room, e.name.as_str(), e.text.as_str()),
            (1, "Elara", "Anyone near Arwic?")
        );
        assert_eq!(e.extra, chat::speaker_note("Mv"), "a new human man");
    }
    assert_eq!(
        h.server.room_lines,
        [chat::RoomSaid {
            character: elara,
            name: "Elara".into(),
            room: 1,
            text: "Anyone near Arwic?".into(),
        }]
    );
    h.server.room_lines.clear();

    let gag = yalain_world::admin::parse("gag elara")
        .expect("a moderation command")
        .expect("it reads");
    let clock = h.clock();
    yalain_world::admin::run(
        &mut h.server,
        &mut h.net,
        clock,
        &yalain_world::admin::Issuer::Console,
        &gag,
    );
    let from = h.bots[b].inbox.len();
    chat::say_in_room(&mut h.server, &mut h.net, elara, chat::GENERAL_ROOM, "hush");
    h.run(20);
    assert_eq!(service_packets(&h, b, from), []);
    assert_eq!(h.server.room_lines, []);
}

/// A line a client sends to a room every player has is kept for the world's link with the words as
/// sent; one to a kingdom's room, which reaches its own members alone, or one refused, is not.
/// Behaviour: server.chat.room-line-kept-for-the-link
#[test]
fn a_line_in_a_room_everyone_has_is_kept_for_the_link_and_a_kingdoms_is_not() {
    let mut h = Harness::new();
    let (a, elara) = h.player_in_world("one", "Elara");
    h.run(10);
    let send = |h: &mut Harness, room: u32| {
        let packet = SendToRoomById {
            context: 7,
            room,
            text: "Anyone?".into(),
            extra: ExtraBlob::Speaker {
                sender: 1,
                chat_type: 100 << 16 | 1,
            },
        }
        .network_packet()
        .expect("packs");
        h.bots[a].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
        h.run(20);
    };
    for room in [chat::GENERAL_ROOM, chat::TRADE_ROOM, chat::PVP_ROOM] {
        send(&mut h, room);
        assert_eq!(
            std::mem::take(&mut h.server.room_lines),
            [chat::RoomSaid {
                character: elara,
                name: "Elara".into(),
                room,
                text: "Anyone?".into(),
            }]
        );
    }
    for room in [6, 77] {
        send(&mut h, room);
        assert_eq!(h.server.room_lines, [], "room {room}");
    }
    let names: Vec<&str> = [chat::GENERAL_ROOM, chat::TRADE_ROOM, chat::PVP_ROOM]
        .iter()
        .map(|r| {
            chat::NEW_CHANNELS
                .iter()
                .find(|c| c.room == *r)
                .expect("a new character's channel")
                .name
        })
        .collect();
    assert_eq!(names, ["General", "Trade 1", "PvP"]);
}

/// A saying over a client's world connection.
fn say(h: &mut Harness, b: usize, text: &str) {
    let m = server_call(function::SAY, &[Arg::Int(0), literal(text)]);
    h.bots[b].send(true, queue::WEENIE, &m);
}

/// How many times a client has been told it is talking too fast since `from`.
fn told_too_fast(h: &Harness, b: usize, from: usize) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::Notice(n)
            if n.text.literal.as_ref().is_some_and(|l| {
                String::from_utf16_lossy(&l.0).trim_end() == chat::TOO_FAST
            }))
        })
        .count()
}

/// A player saying far more than the limit at once is heard for at least the limit's lines and not
/// for all of them, and told once; a moment later it is heard again; kept just past the limit, it
/// is told only every few seconds.
/// Behaviour: server.chat.talk-has-a-limit
#[test]
fn a_player_past_the_talk_limit_is_told_once_and_heard_again_a_moment_later() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    let told_from = h.bots[a].inbox.len();
    const SAID: usize = 60;
    for i in 0..SAID {
        say(&mut h, a, &format!("line {i}"));
    }
    h.run(20);
    let heard_now = heard(&h, b, from, ida);
    assert!(heard_now >= 10, "at least the limit: {heard_now}");
    assert!(heard_now < SAID, "not all of them: {heard_now}");
    assert_eq!(told_too_fast(&h, a, told_from), 1, "told once");
    assert_eq!(told_too_fast(&h, b, from), 0, "told to it alone");

    h.advance(2.0);
    h.run(1);
    let from = h.bots[b].inbox.len();
    say(&mut h, a, "again");
    h.run(10);
    assert_eq!(heard(&h, b, from, ida), 1, "heard again");

    let told_from = h.bots[a].inbox.len();
    let mut lines = 0;
    // Three lines in four steps: fifteen a second, for 60 seconds.
    for step in 0..1_200 {
        if step % 4 != 3 {
            say(&mut h, a, "again and again");
            lines += 1;
        }
        h.run(1);
    }
    let told = told_too_fast(&h, a, told_from);
    assert!(told <= 13, "told {told} times over {lines} lines");
    assert!(told >= 5, "told {told} times");
}

/// Sayings, tells, emotes, typed emotes and room lines spend one limit between them, and a line past
/// it goes nowhere: with the limit spent and half a second's refill, 50 lines of the five kinds bring
/// far less than one kind's limit.
/// Behaviour: server.chat.talk-has-a-limit
#[test]
fn every_kind_of_talk_spends_one_limit_between_them() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == ida)
        .map(|(c, _)| c)
        .expect("in the world");
    let mut spent = 0;
    while !chat::refuses_talk(&mut h.server, &mut h.net, world) {
        spent += 1;
        assert!(spent <= 10, "past the limit");
    }
    assert_eq!(spent, 10, "exactly the limit, the clock standing still");
    let from = h.bots[b].inbox.len();
    let wave = chat::EMOTES[2].id;
    for _ in 0..10 {
        say(&mut h, a, "hello");
        for m in [
            server_call(function::EMOTE, &[Arg::Int(wave)]),
            server_call(function::TYPED_EMOTE, &[string("waves")]),
            server_call(
                function::TELL,
                &[Arg::Int(0), literal("Psst."), literal("babyfare [F]")],
            ),
        ] {
            h.bots[a].send(true, queue::WEENIE, &m);
        }
        let packet = SendToRoomById {
            context: 7,
            room: chat::GENERAL_ROOM,
            text: "Anyone?".into(),
            extra: ExtraBlob::Speaker {
                sender: 1,
                chat_type: 100 << 16 | 1,
            },
        }
        .network_packet()
        .expect("packs");
        h.bots[a].send(false, 4, &ClientMessage::Chat(ChatPacket(packet)));
    }
    h.run(10);
    let went = heard(&h, b, from, ida)
        + h.server.room_lines.len()
        + calls_of(&h, b, from, chat::function::HEAR_TELL).len()
        + typed_emotes(&h, b, from, ida).len()
        + h.world_messages(b, from)
            .iter()
            .filter(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == ida))
            .count();
    assert!(went < 10, "{went} went on");
}

/// A notice comes to the client's reader as it was written: what it takes for a line's flags and for
/// an escape reach the screen as they are, a remote line's text among them.
/// Behaviour: server.chat.notice-as-written
#[test]
fn a_notice_with_brackets_and_backslashes_is_read_by_the_client_as_written() {
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let from = h.bots[b].inbox.len();
    for line in ["back [soon] now", r"a\b \[c] {d|e} #f ^g !h"] {
        yalain_world::login::notice(&mut h.net, world, line);
    }
    h.run(5);
    let read: Vec<String> = h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::Notice(n)) => n.text.literal.map(|l| l.text()),
            _ => None,
        })
        .map(|sent| {
            let wide: Vec<u16> = sent.trim_end_matches('\n').encode_utf16().collect();
            let mut info = auberean_text::info::StringInfo::default();
            info.set_literal(&wide, false);
            String::from_utf16_lossy(&info.literal_value(false))
        })
        .collect();
    assert_eq!(read, ["back [soon] now", r"a\b \[c] {d|e} #f ^g !h"]);
}

/// A line said as the network's command counts as talk: past how fast a player may talk it is not
/// kept, the player is told, and once the pace has filled it is kept again.
/// Behaviour: server.chat.talk-has-a-limit
#[test]
fn the_networks_command_said_past_how_fast_one_may_talk_goes_nowhere() {
    use yalain_world::chat::TOO_FAST;
    // Past the burst of ten, however little time the run takes.
    let lines = 40;
    let mut h = Harness::new();
    let (b, _) = h.player_in_world("one", "Elara");
    let from = h.bots[b].inbox.len();
    for _ in 0..lines {
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(function::SAY, &[Arg::Int(0), literal("@dn on")]),
        );
    }
    h.run(5);
    let kept = h.server.network_commands.len();
    assert!((10..lines).contains(&kept), "{kept} of {lines} kept");
    h.run(5);
    assert!(h.bots[b].notices(from).iter().any(|n| n.contains(TOO_FAST)));
    h.server.network_commands.clear();
    h.advance(10.0);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::SAY, &[Arg::Int(0), literal("@dn on")]),
    );
    h.run(5);
    assert_eq!(h.server.network_commands.len(), 1, "the pace has filled");
}
