//! Chat: saying aloud, telling, emotes; the chat channels of the chat service.
//!
//! **Say.** What a player says reaches it and every client that knows it (the players whose view
//! holds it), as the speaker's saying: the call carries the speaker's object and the words exactly
//! as the client sent them. The words carry no name: each client names the speaker from its own
//! copy of the object.
//!
//! **Tell.** A tell reaches the player of that name (with or without its grammar mark, in any
//! case) as the teller's tell, with the teller's world name; the teller is shown "You tell ...";
//! a name no player in the world has is answered "... is unavailable", and kept for the world's
//! friends service ([`Server::unheard_tells`]), which sends it on when the name is an away, linked
//! character's. A player who ignores the teller on its friends service ([`Server::ignoring`]) never
//! hears it.
//!
//! **The Dereth Network's command.** A line said as `@dn ...`, aloud or into a room, is heard by
//! nobody: it is kept for the world's friends service ([`Server::network_commands`]), which
//! answers it. Its one word for the operator, `@dn verify <token>`, is an admin's: kept marked as
//! an admin's from an admin's account, refused from staff below it as any command beyond their
//! level is, and from a player's account kept as any other of its words.
//!
//! **Gags and admins.** A gagged player ([`crate::admin`]) says, tells, emotes and speaks into a
//! room to nobody, and is told it is gagged. A line an admin says as one of the admin commands is
//! heard by nobody, and answered.
//!
//! **Emotes.** An emote plays its behavior on the emoter for every client that knows it, with
//! the emote's id, from which each client writes the emote's text itself. A typed emote reaches
//! the emoter and every client that knows it as the emoter's, with its id, its name (the name
//! alone) and the words as typed; each client writes "name words" itself. The emoter's client
//! shows its own only as it hears it back.
//!
//! **Channels.** The chat service runs on the login connection: a line a client sends to a room
//! comes back to it as the room's line, then as the answer to its request, and goes to every
//! other player in the world (a kingdom's, an allegiance's or a fellowship's room: to its members).
//! The rooms a client may use are the channels the world sends it as it enters, and the rooms of
//! its kingdom, allegiance and fellowship. A line to any other room, or from a client not in the
//! world, is answered with a refusal and reaches nobody.
//!
//! **Rooms admins made.** A line to one reaches its members ([`crate::admin_calls`]) and no one else.
//!
//! **The Dereth Network's rooms.** A line in a room every player has (General, Trade, PvP) is also
//! kept for the world's link ([`Server::room_lines`]), which passes it on when the room carries the
//! network's lines. A line from another world reaches the players in the room as the room's own line
//! ([`hear_from_afar`]), and `@dng` says a line as the client would ([`say_in_room`]).

use auberean_protocol::login::ChatPacket;
use auberean_protocol::physics::ObjectEvent;
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ServerCall};
use auberean_protocol::types::{BehaviorParams, ObjectStamp};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject, TextInfo, Variable, VariableValue};
use auberean_script::wire::Out;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{hash, Builder, Slot};
use crate::{player, session, Clock, Server, Stage};

/// The client functions chat answers with.
pub mod function {
    /// A saying, on the speaker's object.
    pub const SAYING: u32 = 0x8205_0016;
    /// A tell to the player.
    pub const HEAR_TELL: u32 = 0x8205_0017;
    /// The chat channels the player may use.
    pub const CHANNELS: u32 = 0x8205_001E;
    /// A typed emote, on the emoter's object: its id, its name, the words.
    pub const HEAR_TYPED_EMOTE: u32 = 0x8205_0013;
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

/// The Dereth Network's command, said in chat (the client handles every `/` command itself):
/// a line that is `@dn` or `@derethnetwork` (the same command spelled out), or begins with one and
/// a space (in any case). Its words after that.
#[must_use]
pub fn network_words(text: &str) -> Option<&str> {
    command_words(text, "@dn").or_else(|| command_words(text, "@derethnetwork"))
}

/// The words after `command` in a line that is it or begins with it and a space, in any case.
fn command_words<'a>(text: &'a str, command: &str) -> Option<&'a str> {
    let t = text.trim_start();
    let head = t.get(..command.len())?;
    if !head.eq_ignore_ascii_case(command) {
        return None;
    }
    let rest = &t[command.len()..];
    if rest.is_empty() {
        return Some("");
    }
    rest.starts_with(char::is_whitespace).then(|| rest.trim())
}

/// The words the network's command is given by a line said: `@dng <text>` is `@dn global <text>`
/// made short.
fn said_network_words(text: &str) -> Option<String> {
    network_words(text).map(str::to_owned).or_else(|| {
        command_words(text, "@dng").map(|rest| format!("global {rest}").trim_end().to_owned())
    })
}

/// The Dereth Network's command, said by a player in the world: heard by nobody, and answered by
/// the world's friends service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkCommand {
    /// The player's world connection.
    pub world: ConnId,
    pub character: u64,
    pub name: String,
    /// The words after `@dn`.
    pub words: String,
    /// The speaker's account is an admin's: the operator's words are its to give.
    pub admin: bool,
}

/// The network's words for the operator, `@dn verify <token>` and `@dn admin ...`: an admin's.
pub const VERIFY: &str = "verify";
/// See [`VERIFY`].
pub const ADMIN: &str = "admin";

/// Whether the words after `@dn` begin with `verify` (in any case).
#[must_use]
pub fn is_verify(words: &str) -> bool {
    words
        .split_whitespace()
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case(VERIFY))
}

/// Whether the words after `@dn` begin with `admin` (in any case).
#[must_use]
pub fn is_admin_word(words: &str) -> bool {
    words
        .split_whitespace()
        .next()
        .is_some_and(|w| w.eq_ignore_ascii_case(ADMIN))
}

/// Keeps a line said as the network's command for the friends service: true when it was one. The
/// operator's words from a player's account are logged and go on as the friends service's, which
/// reads them as no word it knows.
fn network_command<S: Store>(s: &mut Server<S>, world: ConnId, text: &str) -> bool {
    let Some(words) = said_network_words(text) else {
        return false;
    };
    let Some(p) = s.players.get(&world) else {
        return true;
    };
    let admin = s.sessions.get(&p.login).is_some_and(|x| x.account.admin);
    if !admin && (is_verify(&words) || is_admin_word(&words)) {
        tracing::warn!(character = %p.name, "refused: the network's operator words need an admin's account");
    }
    let command = NetworkCommand {
        world,
        character: p.character,
        name: p.name.clone(),
        words,
        admin,
    };
    s.network_commands.push(command);
    true
}

/// A player says something: it and every client that knows it hear it. The network's command and
/// an admin's command ([`crate::admin`]) are heard by nobody; a gagged player is told it may not
/// talk, and nobody hears it.
pub fn say<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    call: &ServerCall,
    clock: Clock,
) {
    let said = crate::calls::values(&call.args.0)
        .ok()
        .and_then(|v| v.get(1).and_then(text_of))
        .and_then(|t| t.literal)
        .map(|l| String::from_utf16_lossy(&l));
    if let Some(t) = said {
        if crate::admin::said(s, net, world, &t, clock) || network_command(s, world, &t) {
            return;
        }
    }
    if crate::admin::refuse_gagged(s, net, world) {
        return;
    }
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

/// A name as a player types it, without the grammar mark, in lower case: a character's name, or its
/// name and suffix as a client shows it ([`crate::presence::player_named_for`] reads either).
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

/// A tell that reached no player in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnheardTell {
    /// The teller's id and name.
    pub character: u64,
    pub name: String,
    /// The name it was told to, in small letters, without its grammar mark.
    pub tellee: String,
    pub text: String,
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
    if crate::admin::refuse_gagged(s, net, world) {
        return;
    }
    let Some(teller) = s.players.get(&world) else {
        return;
    };
    let wanted = bare_name(&String::from_utf16_lossy(
        tellee.literal.as_deref().unwrap_or_default(),
    ));
    let Some(target) = crate::presence::player_named_for(s, world, &wanted) else {
        let unheard = UnheardTell {
            character: teller.character,
            name: teller.name.clone(),
            tellee: wanted,
            text: String::from_utf16_lossy(words.literal.as_deref().unwrap_or_default()),
        };
        s.unheard_tells.push(unheard);
        unavailable(net, world, tellee);
        return;
    };
    // A player who ignores the teller never hears it; the teller is shown its tell as usual.
    let ignored = s.players.get(&target).is_some_and(|q| {
        s.ignoring
            .get(&q.character)
            .is_some_and(|names| names.contains(&teller.name.to_lowercase()))
    });
    if !ignored {
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
    }
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

/// "... is unavailable", for a tell to a name no player in the world has.
fn unavailable(net: &mut ServerNet, world: ConnId, tellee: TextInfo) {
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
    if crate::admin::refuse_gagged(s, net, world) {
        return;
    }
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

/// The script type of a string.
const STRING: u16 = 0x76;

pub(crate) fn string_arg(text: &str) -> Arg {
    let mut b = Builder::new();
    b.native(STRING, NativeObject::Text(text.encode_utf16().collect()));
    Arg::Object(b.graph())
}

/// A typed emote as everyone hears it, on the emoter's object (its stamp): its id, its name, the
/// words.
#[must_use]
pub fn heard_typed_emote(emoter: ObjectStamp, name: &str, words: &str) -> ServerMessage {
    ServerMessage::CellObjectCall(ObjectCall {
        object: emoter,
        call: ClientCall {
            function: FunctionId(function::HEAR_TYPED_EMOTE),
            args: Some(
                auberean_protocol::script::ScriptArgs::from_values(&[
                    string_arg(words),
                    string_arg(name),
                    Arg::LongInt(emoter.id),
                ])
                .expect("a typed emote packs"),
            ),
        },
    })
}

/// A player types an emote: it and every client that knows it hear it. A gagged player is told it
/// may not talk, and nobody hears it.
pub fn typed_emote<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(words) = v.first().and_then(|x| match x.root() {
        Some(Node::Text(t)) => Some(t.clone()),
        _ => None,
    }) else {
        s.unhandled.note("a typed emote that does not read");
        return;
    };
    if crate::admin::refuse_gagged(s, net, world) {
        return;
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let m = heard_typed_emote(p.stamp(), &p.name, &words).encode();
    let id = p.character;
    send_to_knowers(s, net, world, id, BlobKind::UNORDERED, &m);
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

/// The room of the PvP channel, which the network's `lfg` and `roleplay` mean on this game: it
/// carried no line in any recorded session.
pub const PVP_ROOM: u32 = NEW_CHANNELS[0].room;
/// The room of the General channel.
pub const GENERAL_ROOM: u32 = NEW_CHANNELS[1].room;
/// The room of the first Trade channel.
pub const TRADE_ROOM: u32 = NEW_CHANNELS[2].room;

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

/// The faction channel of each kingdom: its room, its faction and its name after the world's.
pub const KINGDOM_CHANNELS: [(u32, u32, &str); 4] = [
    (crate::kingdoms::kingdom::SHADOW, 3, "Faction Faction1 "),
    (crate::kingdoms::kingdom::ORDER, 4, "Faction Faction2 "),
    (crate::kingdoms::kingdom::DOMINION, 5, "Faction Faction3 "),
    (crate::kingdoms::kingdom::NEUTRAL, 6, "Faction Neutral "),
];

/// The channels of a player of a kingdom (none: the neutral kingdom's): a new character's, with
/// its kingdom's faction channel.
#[must_use]
pub fn channels_of(kingdom: Option<u32>) -> [Channel; 4] {
    let mut list = NEW_CHANNELS;
    let k = crate::kingdoms::counted(kingdom);
    if let Some((faction, room, name)) = KINGDOM_CHANNELS.iter().find(|c| c.0 == k) {
        list[3] = Channel {
            room: *room,
            faction: *faction,
            name,
            ..list[3].clone()
        };
    }
    list
}

/// Send a player its channels, `times` times over.
pub fn send_channels<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, times: usize) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let ch = channels(
        &s.cfg.world_name,
        &channels_of(crate::kingdoms::of(&p.qualities)),
    )
    .encode();
    for _ in 0..times {
        net.send(world, queue::EVENT, BlobKind::PRIVATE, ch.clone());
    }
}

/// What a character entering the world is told of its allegiance and its channels, after the
/// objects around it: its allegiance's profile, then its channels, twice (as 2017 sent them).
pub fn tell_allegiance_and_channels<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
) {
    crate::allegiance::enter(s, net, world);
    send_channels(s, net, world, 2);
    crate::allegiance::after_entry(s, net, world);
}

// ---- the chat service -----------------------------------------------------------------------

/// The chat service's queue on the login connection.
pub const CHAT_QUEUE: u16 = 4;

/// The code a line is answered with when it may not go: from a client not in the world, to a room
/// that is not one of the world's, or to a room the speaker is not in. Its client shows that the
/// room is not available. (Every line the 2017 servers answered was answered 0.)
pub const REFUSED: u32 = 0xC112_0018;

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

/// What a speaker's client says about it with a line, as the service passes it on: the note's
/// version 1, a zero word, the chat flags (0 for everything a world says itself) and the speaker's
/// meta letters.
#[must_use]
pub fn speaker_note(meta_letters: &str) -> Vec<u8> {
    let letters: Vec<u16> = meta_letters.encode_utf16().collect();
    let mut w = Out::new();
    w.u16(1);
    w.u16(0);
    w.u32(0);
    w.wide(&letters);
    w.buf
}

/// The meta letters of a character of this name and sex: `M` or `F`, and `v` when the name starts
/// with a vowel.
#[must_use]
pub fn meta_letters(name: &str, sex: u32) -> String {
    let written = player::world_name(name, sex);
    written
        .rsplit_once('[')
        .map(|(_, letters)| letters.trim_end_matches(']').to_owned())
        .unwrap_or_default()
}

/// A line a character said in a room every player has, kept for the world's link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomSaid {
    pub character: u64,
    pub name: String,
    pub room: u32,
    pub text: String,
}

/// A line that may go: back to its sender as the room's line, then (for a line the client asked to
/// send) as the answer to its request, and to every other player the room reaches (`members`, or
/// everyone in the world). A line in a room everyone has is kept for the world's link.
fn relay<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    name: &str,
    line: &RoomLine,
    members: Option<&[u64]>,
    answered: bool,
) {
    let event = |l: ConnId| room_event(service_word(l), line.room, name, &line.text, &line.extra);
    send_chat(net, login, event(login));
    if answered {
        send_chat(
            net,
            login,
            room_response(service_word(login), line.context, 0),
        );
    }
    let others: Vec<ConnId> = s
        .sessions
        .iter()
        .filter(|(c, x)| match x.stage {
            Stage::InWorld { character, .. } => {
                **c != login && members.is_none_or(|m| m.contains(&character))
            }
            _ => false,
        })
        .map(|(c, _)| *c)
        .collect();
    for l in others {
        send_chat(net, l, event(l));
    }
    if members.is_none() {
        let character = s.sessions.get(&login).and_then(|x| match x.stage {
            Stage::InWorld { character, .. } => Some(character),
            _ => None,
        });
        if let Some(character) = character {
            s.room_lines.push(RoomSaid {
                character,
                name: name.to_owned(),
                room: line.room,
                text: String::from_utf16_lossy(&line.text),
            });
        }
    }
}

/// `character` says `text` in `room` as its client would, from wherever it was talking: the room's
/// line back to it and to the others, and on to the world's link. A gagged character is told it may
/// not talk, and nobody hears it. No request of the client's is answered, since it made none.
pub fn say_in_room<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    character: u64,
    room: u32,
    text: &str,
) {
    let Some((world, login, name, sex)) = s
        .players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(w, p)| (*w, p.login, p.name.clone(), p.sex))
    else {
        return;
    };
    if crate::admin::refuse_gagged(s, net, world) {
        return;
    }
    let line = RoomLine {
        context: 0,
        room,
        text: text.encode_utf16().collect(),
        extra: speaker_note(&meta_letters(&name, sex)),
    };
    relay(s, net, login, &name, &line, None, false);
}

/// A line from elsewhere reaches every player in the world `hears` lets hear it, by id and name, as
/// the room's own line under the name given. It has no meta letters, since the world knows none for
/// its speaker.
pub fn hear_from_afar<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    room: u32,
    name: &str,
    text: &str,
    hears: impl Fn(u64, &str) -> bool,
) {
    let text: Vec<u16> = text.encode_utf16().collect();
    let note = speaker_note("");
    let listeners: Vec<ConnId> = s
        .sessions
        .iter()
        .filter_map(|(login, x)| match x.stage {
            Stage::InWorld { character, world } => s
                .players
                .get(&world)
                .filter(|p| hears(character, &p.name))
                .map(|_| *login),
            _ => None,
        })
        .collect();
    for l in listeners {
        send_chat(
            net,
            l,
            room_event(service_word(l), room, name, &text, &note),
        );
    }
}

/// A client's packet to the chat service, on its login connection.
pub fn service<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    packet: &[u8],
    clock: Clock,
) {
    let Some(line) = read_room_line(packet) else {
        s.unhandled
            .note("a chat-service request other than a room line");
        return;
    };
    let speaker = s.sessions.get(&login).and_then(|x| match x.stage {
        Stage::InWorld { world, .. } => s.players.get(&world).map(|p| p.name.clone()),
        _ => None,
    });
    let refuse = |net: &mut ServerNet| {
        send_chat(
            net,
            login,
            room_response(service_word(login), line.context, REFUSED),
        );
    };
    let Some(name) = speaker else {
        refuse(net);
        return;
    };
    let world = s.sessions.get(&login).and_then(|x| match x.stage {
        Stage::InWorld { world, .. } => Some(world),
        _ => None,
    });
    // The network's command or an admin's, said into a room: answered, and heard by nobody. A
    // gagged player's line is answered, and goes nowhere.
    if let Some(world) = world {
        let text = String::from_utf16_lossy(&line.text);
        if crate::admin::said(s, net, world, &text, clock)
            || network_command(s, world, &text)
            || crate::admin::refuse_gagged(s, net, world)
        {
            send_chat(
                net,
                login,
                room_response(service_word(login), line.context, 0),
            );
            return;
        }
    }
    // A room admins made: to its members, as an admin's answer, or nowhere.
    if let Some(world) = world {
        let text = String::from_utf16_lossy(&line.text);
        if let Some(went) = crate::admin_calls::rooms::said(s, net, world, line.room, &name, &text)
        {
            if went {
                send_chat(
                    net,
                    login,
                    room_response(service_word(login), line.context, 0),
                );
            } else {
                refuse(net);
            }
            return;
        }
    }
    // A kingdom's, an allegiance's or a fellowship's room reaches its members alone.
    let members = crate::kingdoms::room_members(s, line.room)
        .or_else(|| crate::allegiance::room_members(s, line.room))
        .or_else(|| s.fellowships.room_members(line.room));
    if !NEW_CHANNELS.iter().any(|c| c.room == line.room) && members.is_none() {
        refuse(net);
        return;
    }
    let speaker_id = s.sessions.get(&login).and_then(|x| match x.stage {
        Stage::InWorld { character, .. } => Some(character),
        _ => None,
    });
    if let Some(m) = &members {
        if !speaker_id.is_some_and(|id| m.contains(&id)) {
            refuse(net);
            return;
        }
    }
    relay(s, net, login, &name, &line, members.as_deref(), true);
}
