//! The link messages: one JSON object per WebSocket text frame, between a world and the hub.
//!
//! Every message is an object with a `type`. The same message types flow both ways where both
//! ways make sense (a tell is sent by the origin world and delivered to the target world in the
//! same shape); the hub fills or checks the fields that name the sending world, so a world can only
//! ever speak for its own characters. An object with a `type` this version does not know is
//! [`WireError::Unknown`]: a reader skips it, so a later version can add messages.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::limits::Limits;
use crate::person::{
    is_link_code, is_page_url, is_within_view_limits, within_view_limits, EarlierAlts,
    PersonFriend, PersonOp, PersonOutcome, MAX_CARRIED, MAX_CODE_SECS, MAX_PERSON_CHARACTERS,
};

/// The link protocol's version.
pub const PROTOCOL: u32 = 1;

/// The longest message, in bytes of JSON, either side accepts. Longer frames close the link.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// The longest id a sender may put on a message.
pub const MAX_ID_CHARS: usize = 64;

/// A world as the hub describes it to the others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldInfo {
    /// Its network slug.
    pub slug: String,
    /// Its display name.
    pub name: String,
    /// Whether it has a link up now.
    pub online: bool,
}

/// How a tell, a room line or a hub request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Shown to the recipient (for a channel, to every member online).
    Delivered,
    /// Shown to the recipient, who is away; `message` is the away text.
    Afk,
    /// The recipient squelches the sender. The tell is not shown; what the sender sees is the
    /// sending world's choice, as it is for a local squelch.
    Squelched,
    /// Not shown: the recipient is not online, not on the network, does not allow the sender,
    /// or the receiving world refuses the sender. One answer for all of these, so nobody can learn
    /// which.
    Offline,
    /// Not shown: the recipient's world is on the network but its link is down, or the hub
    /// itself could not be reached.
    Unavailable,
    /// Not shown: the recipient's world is not on the network (unknown, opted out, or never
    /// opted in).
    NotOnNetwork,
    /// Not sent: the sender is over a rate limit.
    RateLimited,
    /// Not sent: the sender is not a member of the channel it wrote to.
    NotMember,
    /// Accepted (a room line handed on, a channel change made).
    Ok,
}

/// What a channel request asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelOp {
    /// Make a channel; the asker is its owner and first member.
    Create,
    /// Invite `who` (owner or member may invite).
    Invite,
    /// Accept an invitation.
    Join,
    /// Leave; the last member leaving ends the channel.
    Leave,
    /// The channels the asker belongs to.
    List,
    /// A channel's members (members only).
    Members,
}

/// How a channel request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelOutcome {
    Ok,
    /// `create`: the name is taken.
    Exists,
    /// No channel has that name.
    NoChannel,
    /// The asker is not a member.
    NotMember,
    /// `join` without an invitation.
    NotInvited,
    /// `invite`/`join`: already a member.
    AlreadyMember,
    /// Refused for another reason (a limit, a bad name, a blocked world).
    Refused,
}

/// One link message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    /// World to hub, first: the protocol version it speaks and its software.
    #[serde(rename = "hello")]
    Hello { proto: u32, software: String },
    /// Hub to world, the answer to `hello`: the world's own slug, every federated world, the
    /// rooms and the limits.
    #[serde(rename = "welcome")]
    Welcome {
        proto: u32,
        world: String,
        hub: String,
        worlds: Vec<WorldInfo>,
        rooms: Vec<String>,
        limits: Limits,
    },
    /// Hub to world: a federated world's link came up or went down.
    #[serde(rename = "world")]
    World(WorldInfo),
    /// World to hub, about once a minute: the registry heartbeat over the link.
    #[serde(rename = "status")]
    Status {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        players_online: Option<u32>,
    },
    /// A watcher asks for a character's online status (`on`), or stops (`!on`). The watcher's
    /// world sends it for each remote friend of each of its characters online; the hub hands it
    /// to the target's home world, which alone decides what to show.
    #[serde(rename = "watch")]
    Watch {
        owner: Address,
        target: Address,
        on: bool,
    },
    /// The home world tells one watcher a character's status; the hub hands it on.
    #[serde(rename = "presence")]
    Presence {
        to: Address,
        of: Address,
        online: bool,
    },
    /// A tell, from the origin world to the hub and from the hub to the target world. `friend`:
    /// the sender lists the recipient as a friend (the sender's world vouches for it).
    #[serde(rename = "tell")]
    Tell {
        id: String,
        from: Address,
        to: Address,
        text: String,
        #[serde(default)]
        friend: bool,
    },
    /// How a tell ended, from the target world (or the hub) back to the origin world.
    #[serde(rename = "tell.result")]
    TellResult {
        id: String,
        outcome: Outcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// World to hub: the world carries a room (`on`) or no longer does.
    #[serde(rename = "room.join")]
    RoomJoin { room: String, on: bool },
    /// World to hub: a line one of its characters said in a room. `via` marks a line its person
    /// said elsewhere, which the hub asked the world to say ([`crate::say`]): `discord`.
    #[serde(rename = "room.say")]
    RoomSay {
        id: String,
        room: String,
        from: Address,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        via: Option<String>,
    },
    /// Hub to world: a line from another world's character, for every world carrying the room,
    /// with the mark its world gave it.
    #[serde(rename = "room.line")]
    RoomLine {
        room: String,
        from: Address,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        via: Option<String>,
    },
    /// Hub to world: how a `room.say` ended.
    #[serde(rename = "room.result")]
    RoomResult { id: String, outcome: Outcome },
    /// World to hub: a channel request by one of its characters.
    #[serde(rename = "channel")]
    Channel {
        id: String,
        op: ChannelOp,
        channel: String,
        by: Address,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        who: Option<Address>,
    },
    /// Hub to world: how a channel request ended.
    #[serde(rename = "channel.result")]
    ChannelResult {
        id: String,
        outcome: ChannelOutcome,
        channel: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        members: Vec<Address>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        channels: Vec<String>,
    },
    /// Hub to world: one of its characters is invited to a channel.
    #[serde(rename = "channel.invite")]
    ChannelInvite {
        channel: String,
        by: Address,
        to: Address,
    },
    /// Hub to world: a channel line for one member online on the world.
    #[serde(rename = "channel.line")]
    ChannelLine {
        channel: String,
        from: Address,
        to: Address,
        text: String,
    },
    /// World to hub: a request about the person layer, for one of the world's characters.
    /// `shown` goes only with [`PersonOp::Alts`], which worlds of the first version alone send.
    #[serde(rename = "person")]
    Person {
        id: String,
        op: PersonOp,
        who: Address,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<Address>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        targets: Vec<Address>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shown: Option<bool>,
    },
    /// Hub to world: how a person request ended; a code, how long it lasts and the page that
    /// links the character with it, or the characters of a person.
    #[serde(rename = "person.result")]
    PersonResult {
        id: String,
        outcome: PersonOutcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_secs: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        characters: Vec<Address>,
    },
    /// World to hub: `who` is online and on the network (`on`), so the hub sends its person view
    /// now and whenever it changes; or it stops.
    #[serde(rename = "person.watch")]
    PersonWatch { who: Address, on: bool },
    /// Hub to world: `to`'s person view: whether it is linked, and the person friends that hold
    /// for it, each with the characters it lists. `alts` is for worlds of the first version
    /// ([`EarlierAlts`]).
    #[serde(rename = "person.friends")]
    PersonFriends {
        to: Address,
        linked: bool,
        #[serde(default)]
        alts: EarlierAlts,
        #[serde(default)]
        persons: Vec<PersonFriend>,
    },
    /// Either way: the other side broke the protocol. `code` is one of [`ErrorCode`]'s words.
    #[serde(rename = "error")]
    Error { code: String, message: String },
}

/// The `code` of an `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// The `hello` named a version the hub does not speak (the hub closes the link).
    Protocol,
    /// A message names a character of another world as its own.
    Identity,
    /// A message did not parse, or a field is out of range.
    Malformed,
    /// A message arrived before `hello`.
    Order,
}

impl ErrorCode {
    /// The code's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Identity => "identity",
            Self::Malformed => "malformed",
            Self::Order => "order",
        }
    }
}

/// Why a frame is not a message this version reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Not JSON, not an object, no `type`, too long, or a known type with bad fields.
    Malformed(String),
    /// A `type` this version does not know: skipped, not an error.
    Unknown(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(why) => write!(f, "malformed message: {why}"),
            Self::Unknown(t) => write!(f, "unknown message type {t:?}"),
        }
    }
}

impl std::error::Error for WireError {}

/// Every `type` this version knows.
pub const TYPES: &[&str] = &[
    "hello",
    "welcome",
    "world",
    "status",
    "watch",
    "presence",
    "tell",
    "tell.result",
    "room.join",
    "room.say",
    "room.line",
    "room.result",
    "channel",
    "channel.result",
    "channel.invite",
    "channel.line",
    "person",
    "person.result",
    "person.watch",
    "person.friends",
    "error",
];

impl Message {
    /// Reads one frame.
    ///
    /// # Errors
    /// [`WireError::Unknown`] for a type this version does not know, [`WireError::Malformed`]
    /// for anything else that is not a message.
    pub fn from_json(text: &str) -> Result<Self, WireError> {
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Malformed("longer than the limit".into()));
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| WireError::Malformed(e.to_string()))?;
        let Some(kind) = value.get("type").and_then(serde_json::Value::as_str) else {
            return Err(WireError::Malformed("no type".into()));
        };
        if !TYPES.contains(&kind) {
            return Err(WireError::Unknown(kind.to_owned()));
        }
        let kind = kind.to_owned();
        let m: Message = serde_json::from_value(value)
            .map_err(|e| WireError::Malformed(format!("{kind}: {e}")))?;
        m.check()?;
        Ok(m)
    }

    /// `to`'s person view, cut to a view's limits ([`within_view_limits`]) and then to one frame:
    /// while it is longer than [`MAX_MESSAGE_BYTES`], the last person friend that lists more than
    /// its handle loses its last character, and once none does, the last person friend goes.
    #[must_use]
    pub fn person_view(to: Address, linked: bool, persons: Vec<PersonFriend>) -> Self {
        let mut view = Self::PersonFriends {
            to,
            linked,
            alts: EarlierAlts,
            persons: within_view_limits(persons),
        };
        while view.to_json().len() > MAX_MESSAGE_BYTES {
            let Self::PersonFriends { persons, .. } = &mut view else {
                break;
            };
            if let Some(p) = persons.iter_mut().rev().find(|p| p.characters.len() > 1) {
                p.characters.pop();
            } else if persons.pop().is_none() {
                break;
            }
        }
        view
    }

    /// Writes one frame.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// The message's `type`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Hello { .. } => "hello",
            Self::Welcome { .. } => "welcome",
            Self::World(_) => "world",
            Self::Status { .. } => "status",
            Self::Watch { .. } => "watch",
            Self::Presence { .. } => "presence",
            Self::Tell { .. } => "tell",
            Self::TellResult { .. } => "tell.result",
            Self::RoomJoin { .. } => "room.join",
            Self::RoomSay { .. } => "room.say",
            Self::RoomLine { .. } => "room.line",
            Self::RoomResult { .. } => "room.result",
            Self::Channel { .. } => "channel",
            Self::ChannelResult { .. } => "channel.result",
            Self::ChannelInvite { .. } => "channel.invite",
            Self::ChannelLine { .. } => "channel.line",
            Self::Person { .. } => "person",
            Self::PersonResult { .. } => "person.result",
            Self::PersonWatch { .. } => "person.watch",
            Self::PersonFriends { .. } => "person.friends",
            Self::Error { .. } => "error",
        }
    }

    /// The range checks serde does not make: ids, text lengths (an away text's as a tell's), room
    /// and channel names, and the person layer's fields. Every message the hub hands on is one it
    /// read under these checks, with at most an id of its own in place of the sender's, so none it
    /// sends is over the frame limit.
    fn check(&self) -> Result<(), WireError> {
        let bad = |why: &str| Err(WireError::Malformed(why.to_owned()));
        let id_ok = |id: &str| !id.is_empty() && id.chars().count() <= MAX_ID_CHARS;
        // Text is cut by the receiving world's text rules; the wire refuses only what no rule
        // would ever keep.
        let text_ok = |t: &str| t.chars().count() <= crate::text::MAX_TEXT_CHARS;
        match self {
            Self::Tell { id, .. }
            | Self::RoomSay { id, .. }
            | Self::TellResult { id, .. }
            | Self::RoomResult { id, .. }
            | Self::Channel { id, .. }
            | Self::ChannelResult { id, .. }
            | Self::Person { id, .. }
            | Self::PersonResult { id, .. }
                if !id_ok(id) =>
            {
                return bad("id");
            }
            _ => {}
        }
        match self {
            Self::Tell { text, .. }
            | Self::RoomSay { text, .. }
            | Self::RoomLine { text, .. }
            | Self::ChannelLine { text, .. }
            | Self::TellResult {
                message: Some(text),
                ..
            } if !text_ok(text) => {
                return bad("text too long");
            }
            _ => {}
        }
        match self {
            Self::Status { state, .. } if !STATES.contains(&state.as_str()) => {
                return bad("state");
            }
            Self::RoomJoin { room, .. }
            | Self::RoomSay { room, .. }
            | Self::RoomLine { room, .. }
                if !is_room_name(room) =>
            {
                return bad("room");
            }
            Self::RoomSay { via: Some(via), .. } | Self::RoomLine { via: Some(via), .. }
                if !crate::say::is_via(via) =>
            {
                return bad("via");
            }
            Self::Channel { channel, .. }
            | Self::ChannelInvite { channel, .. }
            | Self::ChannelLine { channel, .. }
                if !crate::address::is_channel_name(channel) =>
            {
                return bad("channel");
            }
            // A result's names are shown to a player, so they are held to the channel-name rule
            // too.
            Self::ChannelResult {
                channel, channels, ..
            } if !crate::address::is_channel_name(channel)
                || !channels.iter().all(|c| crate::address::is_channel_name(c)) =>
            {
                return bad("channel");
            }
            _ => {}
        }
        self.check_person()
    }

    /// The person layer's fields ([`crate::person`]).
    fn check_person(&self) -> Result<(), WireError> {
        let bad = |why: &str| Err(WireError::Malformed(why.to_owned()));
        match self {
            Self::Person {
                op,
                who,
                target,
                targets,
                shown,
                ..
            } => {
                if who.is_channel()
                    || target.as_ref().is_some_and(Address::is_channel)
                    || targets.iter().any(Address::is_channel)
                {
                    return bad("person: a channel is not a character");
                }
                if op.needs_target() && target.is_none() {
                    return bad("person: no target");
                }
                if *op == PersonOp::Carry && !(1..=MAX_CARRIED).contains(&targets.len()) {
                    return bad("person: carry without targets, or too many");
                }
                if *op == PersonOp::Alts && shown.is_none() {
                    return bad("person: alts without shown");
                }
            }
            Self::PersonResult {
                code,
                expires_secs,
                url,
                characters,
                ..
            } => {
                if code.as_deref().is_some_and(|c| !is_link_code(c)) {
                    return bad("person.result: code");
                }
                if expires_secs.is_some_and(|s| s > MAX_CODE_SECS) {
                    return bad("person.result: expires_secs");
                }
                if url.as_deref().is_some_and(|u| !is_page_url(u)) {
                    return bad("person.result: url");
                }
                if characters.len() > MAX_PERSON_CHARACTERS
                    || characters.iter().any(Address::is_channel)
                {
                    return bad("person.result: characters");
                }
            }
            Self::PersonWatch { who, .. } if who.is_channel() => {
                return bad("person.watch: a channel is not a character");
            }
            Self::PersonFriends { to, persons, .. }
                if to.is_channel() || !is_within_view_limits(persons) =>
            {
                return bad("person.friends: persons");
            }
            _ => {}
        }
        Ok(())
    }
}

/// The states a world's `status` may report.
pub const STATES: &[&str] = &["online", "maintenance", "degraded", "offline"];

/// The one room of the first version: the cross-world Global room.
pub const GLOBAL_ROOM: &str = "global";

/// Whether `s` is a room name: 2 to 20 of `a-z`, `0-9` and inner hyphens.
#[must_use]
pub fn is_room_name(s: &str) -> bool {
    (2..=20).contains(&s.len())
        && s.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// `n` written in letters, for names, which hold no digits.
    fn letters(mut n: usize) -> String {
        let mut s = String::new();
        loop {
            s.push(char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
            n /= 26;
            if n == 0 {
                return s;
            }
        }
    }

    /// Spec: link messages: every message round-trips through its JSON form.
    #[test]
    fn every_message_round_trips_through_its_json() {
        let all = vec![
            Message::Hello {
                proto: PROTOCOL,
                software: "test 1".into(),
            },
            Message::Welcome {
                proto: PROTOCOL,
                world: "a".repeat(2),
                hub: "hub 1".into(),
                worlds: vec![WorldInfo {
                    slug: "coldeve".into(),
                    name: "Coldeve".into(),
                    online: true,
                }],
                rooms: vec![GLOBAL_ROOM.into()],
                limits: Limits::DEFAULT,
            },
            Message::World(WorldInfo {
                slug: "coldeve".into(),
                name: "Coldeve".into(),
                online: false,
            }),
            Message::Status {
                state: "online".into(),
                players_online: Some(3),
            },
            Message::Watch {
                owner: a("Ann@aa"),
                target: a("Bob@coldeve"),
                on: true,
            },
            Message::Presence {
                to: a("Ann@aa"),
                of: a("Bob@coldeve"),
                online: true,
            },
            Message::Tell {
                id: "t1".into(),
                from: a("Ann@aa"),
                to: a("Bob@coldeve"),
                text: "hi".into(),
                friend: true,
            },
            Message::TellResult {
                id: "t1".into(),
                outcome: Outcome::Afk,
                message: Some("Bob is away: brb".into()),
            },
            Message::RoomJoin {
                room: GLOBAL_ROOM.into(),
                on: true,
            },
            Message::RoomSay {
                id: "r1".into(),
                room: GLOBAL_ROOM.into(),
                from: a("Ann@aa"),
                text: "event at 8".into(),
                via: Some(crate::say::VIA_DISCORD.into()),
            },
            Message::RoomLine {
                room: GLOBAL_ROOM.into(),
                from: a("Ann@aa"),
                text: "event at 8".into(),
                via: None,
            },
            Message::RoomResult {
                id: "r1".into(),
                outcome: Outcome::Ok,
            },
            Message::Channel {
                id: "c1".into(),
                op: ChannelOp::Invite,
                channel: "oldguard".into(),
                by: a("Ann@aa"),
                who: Some(a("Bob@coldeve")),
            },
            Message::ChannelResult {
                id: "c1".into(),
                outcome: ChannelOutcome::Ok,
                channel: "oldguard".into(),
                members: vec![a("Ann@aa")],
                channels: vec![],
            },
            Message::ChannelInvite {
                channel: "oldguard".into(),
                by: a("Ann@aa"),
                to: a("Bob@coldeve"),
            },
            Message::ChannelLine {
                channel: "oldguard".into(),
                from: a("Ann@aa"),
                to: a("Bob@coldeve"),
                text: "tonight?".into(),
            },
            Message::Person {
                id: "p1".into(),
                op: PersonOp::Befriend,
                who: a("Ann@aa"),
                target: Some(a("Bob@coldeve")),
                targets: Vec::new(),
                shown: None,
            },
            Message::PersonResult {
                id: "p1".into(),
                outcome: PersonOutcome::Ok,
                code: Some("K7QD-2MXP".into()),
                expires_secs: Some(600),
                url: Some("https://dereth.network/characters/".into()),
                characters: vec![a("Bob@coldeve"), a("Bob@dreamweave")],
            },
            Message::PersonWatch {
                who: a("Ann@aa"),
                on: true,
            },
            Message::PersonFriends {
                to: a("Ann@aa"),
                linked: true,
                alts: EarlierAlts,
                persons: vec![PersonFriend {
                    handle: a("Bob@coldeve"),
                    characters: vec![a("Bob@coldeve"), a("Bob@dreamweave")],
                }],
            },
            Message::Error {
                code: ErrorCode::Identity.word().into(),
                message: "x".into(),
            },
        ];
        assert_eq!(all.len(), TYPES.len(), "one example of every type");
        for m in all {
            let json = m.to_json();
            assert!(
                json.contains(&format!("\"type\":\"{}\"", m.kind())),
                "{json}"
            );
            assert_eq!(Message::from_json(&json), Ok(m), "{json}");
        }
    }

    /// Spec: link messages: an unknown type is skipped, not an error; a bad field is malformed.
    #[test]
    fn an_unknown_type_is_told_apart_from_a_malformed_message() {
        assert_eq!(
            Message::from_json(r#"{"type":"mail.send","x":1}"#),
            Err(WireError::Unknown("mail.send".into()))
        );
        for bad in [
            "not json",
            "[]",
            r#"{"no":"type"}"#,
            r#"{"type":"tell","id":"t","from":"Ann@aa","to":"Bob","text":"x"}"#,
            r#"{"type":"tell","id":"","from":"Ann@aa","to":"Bob@bb","text":"x"}"#,
            r#"{"type":"room.say","id":"r","room":"Global!","from":"Ann@aa","text":"x"}"#,
        ] {
            assert!(
                matches!(Message::from_json(bad), Err(WireError::Malformed(_))),
                "{bad} is malformed"
            );
        }
        let long = format!(
            r#"{{"type":"status","state":"{}"}}"#,
            "x".repeat(MAX_MESSAGE_BYTES)
        );
        assert!(matches!(
            Message::from_json(&long),
            Err(WireError::Malformed(_))
        ));
    }

    /// Spec: link messages: an away text is held to the text limit, so a hub handing an answer on
    /// under a longer id never sends a frame over the frame limit.
    #[test]
    fn an_away_text_over_the_text_limit_is_malformed() {
        let result = |chars: usize| {
            format!(
                r#"{{"type":"tell.result","id":"h1","outcome":"afk","message":"{}"}}"#,
                "y".repeat(chars)
            )
        };
        let limit = crate::text::MAX_TEXT_CHARS;
        assert!(Message::from_json(&result(limit)).is_ok());
        assert!(matches!(
            Message::from_json(&result(limit + 1)),
            Err(WireError::Malformed(_))
        ));
        // The longest away text the wire takes, of the character JSON writes longest, answered
        // under the longest id a sender may give: within the frame.
        let longest = Message::TellResult {
            id: "i".repeat(MAX_ID_CHARS),
            outcome: Outcome::Afk,
            message: Some("\u{1}".repeat(limit)),
        };
        assert!(longest.to_json().len() <= MAX_MESSAGE_BYTES);
    }

    /// Spec: link messages: a channel result's channel and channels are channel names, since a
    /// world shows them to a player.
    #[test]
    fn a_channel_result_naming_no_channel_is_malformed() {
        for bad in [
            r#"{"type":"channel.result","id":"c","outcome":"ok","channel":"<Tell:x>"}"#,
            r#"{"type":"channel.result","id":"c","outcome":"ok","channel":"list","channels":["oldguard","a b"]}"#,
        ] {
            assert!(
                matches!(Message::from_json(bad), Err(WireError::Malformed(_))),
                "{bad} is malformed"
            );
        }
        assert!(Message::from_json(
            r#"{"type":"channel.result","id":"c","outcome":"ok","channel":"list","channels":["oldguard","new-guard"]}"#
        )
        .is_ok());
    }

    /// Spec: link messages: an optional field may be left out.
    #[test]
    fn optional_fields_may_be_left_out() {
        let m = Message::from_json(
            r#"{"type":"tell","id":"t","from":"Ann@aa","to":"Bob@bb","text":"x"}"#,
        )
        .expect("a tell");
        assert!(matches!(m, Message::Tell { friend: false, .. }));
        let r = Message::from_json(r#"{"type":"tell.result","id":"t","outcome":"offline"}"#)
            .expect("a result");
        assert!(matches!(
            r,
            Message::TellResult {
                outcome: Outcome::Offline,
                message: None,
                ..
            }
        ));
    }

    /// Spec: link messages: a room line marks where it was said only when it was said elsewhere,
    /// so every other line is the frame it was before the mark; a mark is a word, as a room's
    /// name is.
    #[test]
    fn a_room_line_carries_its_mark_only_when_it_was_said_elsewhere() {
        let line = |via: Option<&str>| Message::RoomLine {
            room: GLOBAL_ROOM.into(),
            from: a("Ann@aa"),
            text: "hi".into(),
            via: via.map(str::to_owned),
        };
        assert_eq!(
            line(None).to_json(),
            r#"{"type":"room.line","room":"global","from":"Ann@aa","text":"hi"}"#
        );
        assert_eq!(
            line(Some("discord")).to_json(),
            r#"{"type":"room.line","room":"global","from":"Ann@aa","text":"hi","via":"discord"}"#
        );
        for bad in [
            r#"{"type":"room.say","id":"r","room":"global","from":"Ann@aa","text":"x","via":"Discord"}"#,
            r#"{"type":"room.say","id":"r","room":"global","from":"Ann@aa","text":"x","via":""}"#,
            r#"{"type":"room.line","room":"global","from":"Ann@aa","text":"x","via":true}"#,
        ] {
            assert!(
                matches!(Message::from_json(bad), Err(WireError::Malformed(_))),
                "{bad} is malformed"
            );
        }
    }

    /// Spec: link messages: a person view carries `alts` `shown` for the worlds of the first
    /// version, which read it, and a world reads a view with any word there, or none.
    #[test]
    fn a_person_view_says_shown_to_the_worlds_that_read_it() {
        let view = Message::person_view(a("Ann@aa"), false, Vec::new());
        assert_eq!(
            view.to_json(),
            r#"{"type":"person.friends","to":"Ann@aa","linked":false,"alts":"shown","persons":[]}"#
        );
        for frame in [
            r#"{"type":"person.friends","to":"Ann@aa","linked":true,"alts":"hidden"}"#,
            r#"{"type":"person.friends","to":"Ann@aa","linked":true}"#,
        ] {
            assert!(
                matches!(
                    Message::from_json(frame),
                    Ok(Message::PersonFriends { linked: true, .. })
                ),
                "{frame}"
            );
        }
    }

    /// Spec: link messages: a person request names a character, never a channel; the requests
    /// about another character name it, and `alts`, which worlds of the first version send, says
    /// which way.
    #[test]
    fn a_person_request_missing_what_its_op_needs_is_malformed() {
        for bad in [
            r#"{"type":"person","id":"p","op":"befriend","who":"Ann@aa"}"#,
            r#"{"type":"person","id":"p","op":"characters","who":"Ann@aa"}"#,
            r#"{"type":"person","id":"p","op":"alts","who":"Ann@aa"}"#,
            r#"{"type":"person","id":"p","op":"carry","who":"Ann@aa"}"#,
            r#"{"type":"person","id":"p","op":"carry","who":"Ann@aa","targets":["oldguard@channel"]}"#,
            r#"{"type":"person","id":"p","op":"befriend","who":"Ann@aa","target":"oldguard@channel"}"#,
            r#"{"type":"person","id":"p","op":"code","who":"oldguard@channel"}"#,
            r#"{"type":"person","id":"p","op":"mail","who":"Ann@aa"}"#,
            r#"{"type":"person.watch","who":"oldguard@channel","on":true}"#,
        ] {
            assert!(
                matches!(Message::from_json(bad), Err(WireError::Malformed(_))),
                "{bad} is malformed"
            );
        }
        let carry = Message::Person {
            id: "p".into(),
            op: PersonOp::Carry,
            who: a("Ann@aa"),
            target: None,
            targets: vec![a("Bob@coldeve"), a("Cat@dreamweave")],
            shown: None,
        };
        assert_eq!(Message::from_json(&carry.to_json()), Ok(carry));
        let too_many = format!(
            r#"{{"type":"person","id":"p","op":"carry","who":"Ann@aa","targets":[{}]}}"#,
            (0..=MAX_CARRIED)
                .map(|i| format!("\"Char {}@bb\"", letters(i)))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(matches!(
            Message::from_json(&too_many),
            Err(WireError::Malformed(_))
        ));
        assert!(Message::from_json(
            r#"{"type":"person","id":"p","op":"alts","who":"Ann@aa","shown":false}"#
        )
        .is_ok());
        assert!(
            Message::from_json(r#"{"type":"person","id":"p","op":"code","who":"Ann@aa"}"#).is_ok()
        );
    }

    /// Spec: link messages: a person result's code, lapse, page and characters are held to their
    /// shapes, since a world shows them as they are.
    #[test]
    fn a_person_result_out_of_shape_is_malformed() {
        let names = |n: usize| {
            (0..n)
                .map(|i| format!("\"Char {}@cc\"", letters(i)))
                .collect::<Vec<_>>()
                .join(",")
        };
        let result = |n: usize| {
            format!(
                r#"{{"type":"person.result","id":"p","outcome":"ok","characters":[{}]}}"#,
                names(n)
            )
        };
        assert!(
            Message::from_json(&result(20)).is_ok(),
            "20 characters are within it"
        );
        let over = result(21);
        for bad in [
            r#"{"type":"person.result","id":"p","outcome":"ok","code":"<Tell:x>"}"#,
            r#"{"type":"person.result","id":"p","outcome":"ok","code":"K7QD-2MXP","expires_secs":3601}"#,
            r#"{"type":"person.result","id":"p","outcome":"ok","url":"https://x/ <b>"}"#,
            r#"{"type":"person.result","id":"p","outcome":"ok","characters":["oldguard@channel"]}"#,
            over.as_str(),
        ] {
            assert!(
                matches!(Message::from_json(bad), Err(WireError::Malformed(_))),
                "{bad} is malformed"
            );
        }
    }

    /// Spec: link messages: a person view is cut to one frame, however long its names: other
    /// characters go first, then the last person friends.
    #[test]
    fn the_longest_person_view_fits_one_frame() {
        use crate::person::MAX_PERSON_FRIENDS;
        // The longest name, in a letter JSON writes in two bytes, at the longest slug.
        let longest = |i: usize| {
            let tag = letters(i);
            let name = format!("{}{tag}", "\u{e9}".repeat(32 - tag.len()));
            a(&format!("{name}@abcdefghijkl"))
        };
        let persons: Vec<PersonFriend> = (0..MAX_PERSON_FRIENDS)
            .map(|i| PersonFriend {
                handle: longest(i),
                characters: vec![longest(i), longest(i + 1000)],
            })
            .collect();
        let view = Message::person_view(longest(5000), true, persons);
        let json = view.to_json();
        assert!(json.len() <= MAX_MESSAGE_BYTES, "{} bytes", json.len());
        assert_eq!(Message::from_json(&json), Ok(view.clone()));
        let Message::PersonFriends { persons, .. } = view else {
            panic!("a view");
        };
        assert!(
            persons.iter().all(|p| p.characters.len() == 1),
            "every other character goes before any person friend"
        );
        assert!(persons.len() < MAX_PERSON_FRIENDS);
        let short = Message::person_view(
            a("Ann@aa"),
            true,
            vec![PersonFriend {
                handle: a("Bob@bb"),
                characters: vec![a("Bob@bb"), a("Rob@cc")],
            }],
        );
        let Message::PersonFriends { persons, .. } = short else {
            panic!("a view");
        };
        assert_eq!(
            persons[0].characters.len(),
            2,
            "a view that fits is not cut"
        );
        let over = format!(
            r#"{{"type":"person.friends","to":"Ann@aa","linked":true,"alts":"shown","persons":[{{"handle":"Bob@bb","characters":[{}]}}]}}"#,
            (0..21)
                .map(|i| format!("\"Char {}@bb\"", letters(i)))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(matches!(
            Message::from_json(&over),
            Err(WireError::Malformed(_))
        ));
    }
}
