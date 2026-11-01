//! Chat: saying aloud, telling, emotes; the chat channels of the chat service.
//!
//! **Say.** What a player says reaches it and every client that knows it (the players whose view
//! holds it), as the speaker's saying: the call carries the speaker's object and the words exactly
//! as the client sent them. The words carry no name: each client names the speaker from its own
//! copy of the object.
//!
//! **Tell.** A tell reaches the player of that name (with or without its grammar mark, in any
//! case) as the teller's tell, with the teller's world name; the teller is shown "You tell ...";
//! a name no player in the world has is answered "... is unavailable".
//!
//! **Emotes.** An emote plays its behavior on the emoter for every client that knows it, with
//! the emote's id, from which each client writes the emote's text itself.
//!
//! **Channels.** The chat service runs on the login connection: a line a client sends to a room
//! comes back to it as the room's line, then as the answer to its request, and goes to every
//! other player in the world. The rooms a client may use are the channels the world sends it as
//! it enters.

use auberean_protocol::login::ChatPacket;
use auberean_protocol::physics::ObjectEvent;
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ServerCall};
use auberean_protocol::types::{BehaviorParams, ObjectStamp};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject, TextInfo, Variable, VariableValue};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{hash, Builder, Slot};
use crate::{player, session, Server, Stage};

/// The client functions chat answers with.
pub mod function {
    /// A saying, on the speaker's object.
    pub const SAYING: u32 = 0x8205_0016;
    /// A tell to the player.
    pub const HEAR_TELL: u32 = 0x8205_0017;
    /// The chat channels the player may use.
    pub const CHANNELS: u32 = 0x8205_001E;
    /// The player's allegiance.
    pub const ALLEGIANCE_PROFILE: u32 = 0x8203_002E;
}

/// Kinds of text in the chat window.
pub mod text_type {
    pub const TELL: u32 = 0x20;
    pub const TELL_ERROR: u32 = 0x21;
}

/// The strings of tells, in the client's tables: "You tell %1, \q%2\q" and "%1 is unavailable."
pub mod strings {
    pub const TABLE: u32 = 0x2500_0002;
    pub const YOU_TELL: u32 = 0x47B8_69AF;
    pub const UNAVAILABLE: u32 = 0xC311_A7F0;
    /// The variables those strings take: the one told, the words.
    pub const TELLEE: u32 = 0x1C06_9478;
    pub const WORDS: u32 = 0x9B4E_3800;
    pub const NAME: u32 = 0x15B2_0D98;
}

/// One prefab emote: its id, the behavior it plays, the effect it shows (0 for none), and
/// whether the behavior cancels a move and fades the children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Emote {
    pub id: u32,
    pub behavior: u32,
    pub fx: u32,
    pub cancels_move: bool,
    pub fades_children: bool,
}

const fn prefab(id: u32, behavior: u32, fx: u32, bits: u32) -> Emote {
    Emote {
        id,
        behavior,
        fx,
        cancels_move: bits & 0x20 != 0,
        fades_children: bits & 0x100 != 0,
    }
}

/// The prefab emotes, as the client's emote table sets them up.
pub const EMOTES: [Emote; 47] = [
    prefab(0xCD77_D18F, 0x4100_001B, 0, 0x101),
    prefab(0x59A2_E4D4, 0x4100_001B, 0, 0x101),
    prefab(0x6173_A576, 0x4100_001C, 0, 0x121),
    prefab(0xCD4D_7F99, 0x4100_010F, 0, 0x121),
    prefab(0x552F_2F9F, 0x4100_0110, 0, 0x121),
    prefab(0xC347_6D87, 0x4100_0111, 0, 0x101),
    prefab(0x7D9B_C50A, 0x4100_0112, 0, 0x21),
    prefab(0xD747_EC47, 0x4100_0113, 0, 0x1),
    prefab(0x97CC_A479, 0x4100_0114, 0, 0x1),
    prefab(0x3EBF_7C6A, 0x4100_0115, 0, 0x1),
    prefab(0x9754_A461, 0x4100_0116, 0, 0x121),
    prefab(0x8F4C_A443, 0x4100_0125, 0, 0x121),
    prefab(0xFD4B_B181, 0x4100_015B, 0, 0x121),
    prefab(0xCD77_D183, 0x4100_0125, 0, 0x121),
    prefab(0xF109_6C45, 0x4100_0126, 0, 0x21),
    prefab(0x6142_776C, 0x4100_0127, 0, 0x21),
    prefab(0xC27A_0DA0, 0x4100_0128, 0, 0x121),
    prefab(0x2E9A_1D7F, 0x4100_0118, 0, 0x121),
    prefab(0xDD5E_C189, 0x4100_0117, 0, 0x101),
    prefab(0x7B97_2F74, 0x4100_011A, 0, 0x121),
    prefab(0xE9C7_11AB, 0x4100_011B, 0, 0x121),
    prefab(0x91E7_0187, 0x4100_011C, 0, 0x121),
    prefab(0x0F3D_1CF2, 0x4100_011D, 0, 0x121),
    prefab(0xDD56_7195, 0x4100_011E, 0, 0x121),
    prefab(0x9E0A_5C09, 0x4100_011F, 0, 0x121),
    prefab(0xDA7A_CC23, 0x4100_0129, 0, 0x21),
    prefab(0x235A_504E, 0x4100_012A, 0, 0x21),
    prefab(0xCE73_6D78, 0x4100_012B, 0, 0x121),
    prefab(0x0E73_6D78, 0x4100_012C, 0, 0x121),
    prefab(0x4E73_6D78, 0x4100_012D, 0, 0x121),
    prefab(0x8E73_6D79, 0x4100_012E, 0, 0x121),
    prefab(0xB75B_4F8F, 0x4100_0125, 0, 0x101),
    prefab(0x51F8_C33D, 0x4100_00A6, 0x8100_02B3, 0x21),
    prefab(0x51F8_C33B, 0x4100_00A7, 0x8100_02B4, 0x21),
    prefab(0x51F8_C339, 0x4100_00A8, 0x8100_02B5, 0x21),
    prefab(0x51F8_C337, 0x4100_00A9, 0x8100_02B6, 0x21),
    prefab(0x51F8_C335, 0x4100_00AA, 0x8100_02B7, 0x21),
    prefab(0x51F8_C333, 0x4100_00AB, 0x8100_02B8, 0x21),
    prefab(0x51F8_C331, 0x4100_00AC, 0x8100_02B9, 0x21),
    prefab(0x51F8_C32F, 0x4100_00AD, 0x8100_02BA, 0x21),
    prefab(0x51F8_C32D, 0x4100_00AE, 0x8100_02BB, 0x21),
    prefab(0xED1A_2205, 0x4100_00AF, 0x8100_02BC, 0x21),
    prefab(0x32B4_A467, 0x4100_0125, 0x4100_04FC, 0x121),
    prefab(0xD980_234A, 0x4100_0158, 0x4100_04FB, 0x121),
    prefab(0xDE0B_9C75, 0x4100_0150, 0x8100_0325, 0x1),
    prefab(0x9E33_3C61, 0x4100_0151, 0x8100_0324, 0x1),
    prefab(0xE9C6_D181, 0x4100_012F, 0x4100_05EE, 0x101),
];

/// The behavior word bits a prefab emote's behavior carries beyond the table's: the emote's id
/// and the context.
pub const EMOTE_ID_BIT: u32 = 0x80_0000;

fn send_to_knowers<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    from: ConnId,
    id: u64,
    kind: BlobKind,
    bytes: &[u8],
) {
    for (conn, q) in &s.players {
        if *conn == from || q.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, kind, bytes.to_vec());
        }
    }
}

/// A player says something: it and every client that knows it hear it.
pub fn say<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, call: &ServerCall) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let m = ServerMessage::CellObjectCall(ObjectCall {
        object: p.stamp(),
        call: ClientCall {
            function: FunctionId(function::SAYING),
            args: Some(call.args.clone()),
        },
    })
    .encode();
    let id = p.character;
    send_to_knowers(s, net, world, id, BlobKind::UNORDERED, &m);
}

fn text_of(v: &Value) -> Option<TextInfo> {
    match v.root() {
        Some(Node::TextInfo(t)) => Some(t.clone()),
        _ => None,
    }
}

/// A string reference as a call value.
#[must_use]
pub fn text_arg(t: TextInfo) -> Arg {
    let mut b = Builder::new();
    b.native(session::native::TEXT, NativeObject::TextInfo(t));
    Arg::Object(b.graph())
}

/// A name as a player types it: the name alone, without the grammar mark, in lower case.
#[must_use]
pub fn bare_name(typed: &str) -> String {
    typed
        .split(" [")
        .next()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
}

fn nested(id: u32, t: TextInfo) -> Variable {
    Variable {
        id,
        kind: 9,
        value: VariableValue::Nested(Box::new(t)),
    }
}

/// Show a text in a player's chat window: a kind of text and a string reference.
#[must_use]
pub fn show_text(kind: u32, text: TextInfo) -> ServerMessage {
    session::call(
        session::function::SHOW_TEXT,
        &[text_arg(text), Arg::Int(kind)],
    )
    .expect("a string reference packs")
}

/// A player tells another something.
pub fn tell<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let (Some(Value::Int(flags)), Some(words), Some(tellee)) = (
        v.first(),
        v.get(1).and_then(text_of),
        v.get(2).and_then(text_of),
    ) else {
        s.unhandled.note("a tell that does not read");
        return;
    };
    let Some(teller) = s.players.get(&world) else {
        return;
    };
    let wanted = bare_name(&String::from_utf16_lossy(
        tellee.literal.as_deref().unwrap_or_default(),
    ));
    let target = s
        .players
        .iter()
        .find(|(_, q)| q.name.to_lowercase() == wanted)
        .map(|(c, _)| *c);
    let Some(target) = target else {
        let mut t = TextInfo {
            string_id: strings::UNAVAILABLE,
            table: strings::TABLE,
            ..TextInfo::default()
        };
        t.set_variable(nested(strings::NAME, tellee));
        net.send(
            world,
            queue::EVENT,
            BlobKind::PRIVATE,
            show_text(text_type::TELL_ERROR, t).encode(),
        );
        return;
    };
    let heard = session::call(
        function::HEAR_TELL,
        &[
            Arg::Int(*flags),
            text_arg(words.clone()),
            text_arg(TextInfo {
                has_literal: 1,
                literal: Some(
                    player::world_name(&teller.name, teller.sex)
                        .encode_utf16()
                        .collect(),
                ),
                ..TextInfo::default()
            }),
            Arg::LongInt(teller.character),
        ],
    )
    .expect("a tell packs");
    net.send(target, queue::EVENT, BlobKind::PRIVATE, heard.encode());
    let mut t = TextInfo {
        string_id: strings::YOU_TELL,
        table: strings::TABLE,
        ..TextInfo::default()
    };
    t.set_variable(nested(strings::TELLEE, tellee));
    t.set_variable(nested(strings::WORDS, words));
    net.send(
        world,
        queue::EVENT,
        BlobKind::PRIVATE,
        show_text(text_type::TELL, t).encode(),
    );
}

/// A player shows a prefab emote.
pub fn emote<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(Value::Int(id)) = v.first() else {
        s.unhandled.note("an emote that does not read");
        return;
    };
    let Some(e) = EMOTES.iter().find(|e| e.id == *id).copied() else {
        s.unhandled.note(format!("emote {id:#010x}"));
        return;
    };
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.visual = p.visual.wrapping_add(1);
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR | EMOTE_ID_BIT | BehaviorParams::CONTEXT,
        behavior: e.behavior,
        context,
        ..BehaviorParams::default()
    };
    b.words[12] = e.id;
    if e.cancels_move {
        b.bits |= 0x20;
        b.words[1] = 1;
    }
    if e.fades_children {
        b.bits |= 0x100;
        b.words[6] = 1;
    }
    if e.fx != 0 {
        b.bits |= BehaviorParams::FX;
        b.fx = e.fx;
    }
    let m = ServerMessage::PlayBehavior(ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action: b,
    })
    .encode();
    let id = p.character;
    send_to_knowers(s, net, world, id, BlobKind::VISUAL, &m);
}

// ---- the allegiance and the channels, sent as a character enters --------------------------------

/// The script package of an allegiance's profile, with its size in slots.
pub const ALLEGIANCE_PROFILE: (u16, usize) = (0x1B5, 13);
/// The script package of a channel's data, with its size in slots.
pub const CHANNEL_DATA: (u16, usize) = (0x1AF4, 7);

/// The profile of a character in no allegiance: no name, patron, members, monarch, message,
/// officers or vassals; nobody online; no faction; nobody in it.
#[must_use]
pub fn no_allegiance() -> ServerMessage {
    let none = Slot::Ref(None);
    let mut b = Builder::new();
    b.object(
        ALLEGIANCE_PROFILE.0,
        &[
            none,
            none,
            none,
            none,
            Slot::Int(0),
            Slot::Int(0),
            none,
            none,
            none,
            Slot::Int(0),
            Slot::Int(0),
            Slot::Int(0),
            none,
        ],
    );
    session::call(function::ALLEGIANCE_PROFILE, &[Arg::Object(b.graph())]).expect("a profile packs")
}

/// A chat channel the world gives a player: its kind, room, region, faction, whether its room
/// is still to be made, and its name after the world's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub kind: u32,
    pub room: u32,
    pub region: u32,
    pub faction: u32,
    pub pending: bool,
    pub name: &'static str,
}

/// The channels the 2017 world gave a new character: player-versus-player, general, the first
/// trade channel, the neutral faction.
pub const NEW_CHANNELS: [Channel; 4] = [
    Channel {
        kind: 1 << 24,
        room: 2,
        region: 0,
        faction: 0,
        pending: true,
        name: "PvP",
    },
    Channel {
        kind: 1 << 19,
        room: 1,
        region: 0,
        faction: 0,
        pending: true,
        name: "General",
    },
    Channel {
        kind: 1 << 20,
        room: 8,
        region: 1,
        faction: 0,
        pending: false,
        name: "Trade 1",
    },
    Channel {
        kind: 1 << 22,
        room: 6,
        region: 0,
        faction: 0x8000_0000,
        pending: true,
        name: "Faction Neutral ",
    },
];

/// Tags the script machine gives a channel's slots: 2 for a whole number, 4 for one set from an
/// enumeration or a truth value (as the 2017 server's slots carried them).
const fn tag_of(set_as_enum: bool) -> u8 {
    if set_as_enum {
        4
    } else {
        2
    }
}

/// The channels a player may use, keyed by their kind.
#[must_use]
pub fn channels(world_name: &str, list: &[Channel]) -> ServerMessage {
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    let entries: Vec<(Item, Item)> = list
        .iter()
        .zip((first..).step_by(2))
        .map(|(c, id)| (Item::Word(c.kind), Item::Ref(id)))
        .collect();
    b.native(
        session::native::WORD_REF_HASH,
        hash(Elem::Word, Elem::Ref, session::TABLE_SIZE, &entries),
    );
    for c in list {
        let name = b.next_id() + 1;
        b.object(
            CHANNEL_DATA.0,
            &[
                Slot::Tagged(tag_of(c.pending), u32::from(c.pending)),
                Slot::Tagged(2, c.kind),
                Slot::Tagged(tag_of(c.region != 0), c.region),
                Slot::Tagged(tag_of(c.region == 0), c.room),
                Slot::Tagged(2, 1),
                Slot::Tagged(tag_of(c.faction != 0), c.faction),
                Slot::Ref(Some(name)),
            ],
        );
        let text = format!("{world_name} {}", c.name);
        b.native(118, NativeObject::Text(text.encode_utf16().collect()));
    }
    session::call(function::CHANNELS, &[Arg::Object(b.graph())]).expect("the channels pack")
}

/// What a character entering the world is told of its allegiance and its channels, after the
/// objects around it: its allegiance's profile, then its channels, twice (as 2017 sent them).
pub fn tell_allegiance_and_channels<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
) {
    crate::allegiance::enter(s, net, world);
    let ch = channels(&s.cfg.world_name, &NEW_CHANNELS).encode();
    for _ in 0..2 {
        net.send(world, queue::EVENT, BlobKind::PRIVATE, ch.clone());
    }
    crate::allegiance::after_entry(s, net, world);
}

// ---- the chat service -----------------------------------------------------------------------

/// The chat service's queue on the login connection.
pub const CHAT_QUEUE: u16 = 4;

/// A line a client sends to a room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomLine {
    pub context: u32,
    pub room: u32,
    pub text: Vec<u16>,
    /// What the client says about the speaker (its version, flags and grammar letters), kept as
    /// sent: the service sends it on with the line.
    pub extra: Vec<u8>,
}

fn wide(r: &mut dereth_protocol::Reader<'_>) -> Option<Vec<u16>> {
    let n = r.compressed_u32().ok()? as usize;
    let raw = r.bytes(n.checked_mul(2)?).ok()?;
    Some(
        raw.as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect(),
    )
}

/// Read a client's request to the chat service: a line sent to a room.
#[must_use]
pub fn read_room_line(packet: &[u8]) -> Option<RoomLine> {
    let mut r = dereth_protocol::Reader::new(packet);
    let mut header = [0u32; 7];
    for w in &mut header {
        *w = r.u32().ok()?;
    }
    if header[..3] != [3, 2, 1] {
        return None;
    }
    let n = r.u32().ok()? as usize;
    let mut body = dereth_protocol::Reader::new(r.bytes(n).ok()?);
    let context = body.u32().ok()?;
    let (kind, method) = (body.u32().ok()?, body.u32().ok()?);
    if (kind, method) != (2, 2) {
        return None;
    }
    let room = body.u32().ok()?;
    let text = wide(&mut body)?;
    let m = body.u32().ok()? as usize;
    let extra = body.bytes(m).ok()?.to_vec();
    Some(RoomLine {
        context,
        room,
        text,
        extra,
    })
}

fn packet(header: [u32; 7], body: &[u8]) -> Vec<u8> {
    let mut w = dereth_protocol::Writer::new();
    for h in header {
        w.u32(h);
    }
    w.u32(u32::try_from(body.len()).unwrap_or(0));
    w.bytes(body);
    w.into_inner()
}

/// The word the service names a login connection with in its packets.
#[must_use]
pub fn service_word(login: ConnId) -> u32 {
    0x00F1_0000 | (login.0 & 0xFFFF)
}

/// A room's line as the service sends it: the room, the speaker's name, the words, and what the
/// speaker's client said about it.
#[must_use]
pub fn room_event(word: u32, room: u32, name: &str, text: &[u16], extra: &[u8]) -> Vec<u8> {
    let mut w = dereth_protocol::Writer::new();
    w.u32(room);
    let name: Vec<u16> = name.encode_utf16().collect();
    for t in [&name[..], text] {
        w.compressed_u32(u32::try_from(t.len()).unwrap_or(0));
        for u in t {
            w.u16(*u);
        }
    }
    w.u32(u32::try_from(extra.len()).unwrap_or(0));
    w.bytes(extra);
    packet([1, 1, 1, word, 1, word, 0], &w.into_inner())
}

/// The service's answer to a request: its context and the result (0 for done).
#[must_use]
pub fn room_response(word: u32, context: u32, result: u32) -> Vec<u8> {
    let body: Vec<u8> = [context, 2, 2, result]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    packet([5, 2, 1, word, 1, word, 0], &body)
}

fn send_chat(net: &mut ServerNet, login: ConnId, packet: Vec<u8>) {
    net.send(
        login,
        CHAT_QUEUE,
        BlobKind::LOGIN,
        ServerMessage::Chat(ChatPacket(packet)).encode(),
    );
}

/// A client's packet to the chat service, on its login connection.
pub fn service<S: Store>(s: &mut Server<S>, net: &mut ServerNet, login: ConnId, packet: &[u8]) {
    let Some(line) = read_room_line(packet) else {
        s.unhandled
            .note("a chat-service request other than a room line");
        return;
    };
    let speaker = s.sessions.get(&login).and_then(|x| match x.stage {
        Stage::InWorld { world, .. } => s.players.get(&world).map(|p| p.name.clone()),
        _ => None,
    });
    let Some(name) = speaker else {
        s.unhandled
            .note("a room line from a client not in the world");
        return;
    };
    // An allegiance's or a fellowship's room reaches its members alone.
    let members = crate::allegiance::room_members(s, line.room)
        .or_else(|| s.fellowships.room_members(line.room));
    if !NEW_CHANNELS.iter().any(|c| c.room == line.room) && members.is_none() {
        s.unhandled.note(format!("a line to room {}", line.room));
        return;
    }
    let speaker_id = s.sessions.get(&login).and_then(|x| match x.stage {
        Stage::InWorld { character, .. } => Some(character),
        _ => None,
    });
    if let Some(m) = &members {
        if !speaker_id.is_some_and(|id| m.contains(&id)) {
            s.unhandled
                .note(format!("a line to room {} from outside it", line.room));
            return;
        }
    }
    let event = |l: ConnId| room_event(service_word(l), line.room, &name, &line.text, &line.extra);
    send_chat(net, login, event(login));
    send_chat(
        net,
        login,
        room_response(service_word(login), line.context, 0),
    );
    let others: Vec<ConnId> = s
        .sessions
        .iter()
        .filter(|(c, x)| match x.stage {
            Stage::InWorld { character, .. } => {
                **c != login && members.as_ref().is_none_or(|m| m.contains(&character))
            }
            _ => false,
        })
        .map(|(c, _)| *c)
        .collect();
    for l in others {
        send_chat(net, l, event(l));
    }
}
