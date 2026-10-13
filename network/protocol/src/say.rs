//! Say: the hub asks a world to say a line as one of the world's own characters in a room, a line
//! the character's person said elsewhere (in a Discord channel that mirrors the room), and the
//! world's answer (`docs/network/02-link-protocol.md` section 9, `05-hub.md` section 5.9).
//!
//! The world says it under its own rules (the character is its own, on the network and linked by
//! itself, neither banned nor gagged, old enough, within the room's pace, and the room carried and
//! not turned off), shows it to its own players with the mark of where it was said, and sends it
//! on as any line of the room: a `room.say` carrying the same mark ([`crate::Message::RoomSay`]'s
//! `via`), which the hub hands every other world and every mirror of the room.
//!
//! The say messages are a family of their own: [`SayMessage`] reads them, and [`crate::Message`]
//! skips them as types it does not know, as every reader of a link skips a type it does not know.
//! A world that knows nothing of them never answers, and the hub tells the person so.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::wire::{is_room_name, WireError, MAX_ID_CHARS, MAX_MESSAGE_BYTES};

/// Where a line was said, as a line's mark names it: in Discord.
pub const VIA_DISCORD: &str = "discord";

/// The longest reason a world may give for a refusal, in characters.
pub const MAX_REASON_CHARS: usize = 32;

/// The reasons a world gives for saying no line, as this version names them. A world may give
/// another word; the hub takes one it does not know as a refusal without a reason it can name.
pub mod reason {
    /// The world has no such character (none of that name, or one deleted).
    pub const NO_CHARACTER: &str = "no_character";
    /// The character is not on the network (its player turned it off, or never on).
    pub const NOT_ON_NETWORK: &str = "not_on_network";
    /// The character never asked for its link: the person linked an earlier character of its name.
    pub const NOT_LINKED: &str = "not_linked";
    /// The character's account is banned.
    pub const BANNED: &str = "banned";
    /// The character is gagged.
    pub const GAGGED: &str = "gagged";
    /// The character is younger than the operator's minimum age for the network.
    pub const TOO_NEW: &str = "too_new";
    /// Over the character's pace of room lines.
    pub const RATE_LIMITED: &str = "rate_limited";
    /// The world does not carry the room now, or its operator turned it off.
    pub const ROOM_OFF: &str = "room_off";
    /// The world's own rules do not let the character speak in the room (an Olthoi player in a
    /// room it may not use, say).
    pub const NOT_ALLOWED: &str = "not_allowed";
}

/// Whether `s` may be a line's mark: 2 to 20 of `a-z`, `0-9` and inner hyphens, as a room's name.
#[must_use]
pub fn is_via(s: &str) -> bool {
    is_room_name(s)
}

/// A name as a player gives it, without the mark a world shows beside the speaker of a line said
/// from Discord: `Ann (Discord)` is `Ann`, and `Ann@coldeve (discord)` is `Ann@coldeve`. No
/// character's name holds a parenthesis ([`crate::address`]), so a trailing `(Discord)`, in any
/// case, is never part of one; a player who clicks a marked speaker's name, or types it as shown,
/// names the character. A name that is nothing but the mark is left as it is.
#[must_use]
pub fn without_mark(name: &str) -> &str {
    let t = name.trim_end();
    let Some(inner) = t.strip_suffix(')') else {
        return name;
    };
    let Some(open) = inner.rfind('(') else {
        return name;
    };
    if !inner[open + 1..].eq_ignore_ascii_case(VIA_DISCORD) {
        return name;
    }
    let rest = inner[..open].trim_end();
    if rest.is_empty() {
        name
    } else {
        rest
    }
}

/// Whether `s` may be a refusal's reason: 1 to [`MAX_REASON_CHARS`] of `a-z` and `_`.
#[must_use]
pub fn is_reason(s: &str) -> bool {
    (1..=MAX_REASON_CHARS).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

/// How a world took a line the hub asked it to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SayOutcome {
    /// Said: shown to the world's own players in the room, and sent on as a `room.say`.
    Said,
    /// Not said, for the `reason` given.
    Refused,
}

/// One of the say messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SayMessage {
    /// Hub to world: say `text` as `who`, one of this world's characters, in `room` (`global`, or
    /// one of the world's own rooms), marked as said `via` (`discord`).
    #[serde(rename = "say")]
    Ask {
        id: String,
        room: String,
        who: Address,
        text: String,
        via: String,
    },
    /// World to hub: how it took the line; `reason` with `refused` alone.
    #[serde(rename = "say.result")]
    Result {
        id: String,
        outcome: SayOutcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// Every `type` of the say messages.
pub const TYPES: &[&str] = &["say", "say.result"];

impl SayMessage {
    /// Reads one frame.
    ///
    /// # Errors
    /// [`WireError::Unknown`] for a type that is not one of the say messages,
    /// [`WireError::Malformed`] for anything else that is not one of them.
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
        let m: Self = serde_json::from_value(value)
            .map_err(|e| WireError::Malformed(format!("{kind}: {e}")))?;
        m.check()
            .map_err(|why| WireError::Malformed(format!("{kind}: {why}")))?;
        Ok(m)
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
            Self::Ask { .. } => "say",
            Self::Result { .. } => "say.result",
        }
    }

    /// The checks serde does not make: the id, the room, a character and never a channel, the
    /// text's length, the mark, and a reason with a refusal alone.
    fn check(&self) -> Result<(), &'static str> {
        let id_ok = |id: &str| !id.is_empty() && id.chars().count() <= MAX_ID_CHARS;
        match self {
            Self::Ask {
                id,
                room,
                who,
                text,
                via,
            } => {
                if !id_ok(id) {
                    return Err("id");
                }
                if !is_room_name(room) {
                    return Err("room");
                }
                if who.is_channel() {
                    return Err("a character, never a channel");
                }
                if text.chars().count() > crate::text::MAX_TEXT_CHARS {
                    return Err("text too long");
                }
                if !is_via(via) {
                    return Err("via");
                }
                Ok(())
            }
            Self::Result {
                id,
                outcome,
                reason,
            } => {
                if !id_ok(id) {
                    return Err("id");
                }
                match (outcome, reason.as_deref()) {
                    (_, None) => Ok(()),
                    (SayOutcome::Refused, Some(r)) if is_reason(r) => Ok(()),
                    (SayOutcome::Refused, Some(_)) => Err("reason"),
                    (SayOutcome::Said, Some(_)) => Err("a reason without a refusal"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryMessage, Message, ReachMessage};

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    fn malformed(text: &str) -> bool {
        matches!(SayMessage::from_json(text), Err(WireError::Malformed(_)))
    }

    /// Spec: link messages: every say message round-trips through its JSON form.
    #[test]
    fn every_say_message_round_trips_through_its_json() {
        let all = [
            SayMessage::Ask {
                id: "h7".into(),
                room: "global".into(),
                who: a("Ann@coldeve"),
                text: "evening all".into(),
                via: VIA_DISCORD.into(),
            },
            SayMessage::Ask {
                id: "h8".into(),
                room: "roleplay".into(),
                who: a("Sir Bob@coldeve"),
                text: String::new(),
                via: VIA_DISCORD.into(),
            },
            SayMessage::Result {
                id: "h7".into(),
                outcome: SayOutcome::Said,
                reason: None,
            },
            SayMessage::Result {
                id: "h8".into(),
                outcome: SayOutcome::Refused,
                reason: Some(reason::GAGGED.into()),
            },
            SayMessage::Result {
                id: "h9".into(),
                outcome: SayOutcome::Refused,
                reason: None,
            },
        ];
        for m in all {
            let json = m.to_json();
            assert!(
                json.contains(&format!("\"type\":\"{}\"", m.kind())),
                "{json}"
            );
            assert_eq!(SayMessage::from_json(&json), Ok(m), "{json}");
        }
        assert_eq!(
            SayMessage::Ask {
                id: "h1".into(),
                room: "global".into(),
                who: a("Ann@coldeve"),
                text: "hi".into(),
                via: VIA_DISCORD.into(),
            }
            .to_json(),
            r#"{"type":"say","id":"h1","room":"global","who":"Ann@coldeve","text":"hi","via":"discord"}"#
        );
    }

    /// Spec: link messages: a name given with the mark a line said from Discord shows beside its
    /// speaker names the character without it, in any case; any other name is left as it is.
    #[test]
    fn a_name_given_with_the_discord_mark_names_the_character() {
        assert_eq!(without_mark("Ann (Discord)"), "Ann");
        assert_eq!(without_mark("Sir Bob (DISCORD) "), "Sir Bob");
        assert_eq!(without_mark("Ann@coldeve (discord)"), "Ann@coldeve");
        assert_eq!(without_mark("Ann(Discord)"), "Ann");
        assert_eq!(without_mark("Ann"), "Ann");
        assert_eq!(without_mark("Ann (Discordia)"), "Ann (Discordia)");
        assert_eq!(without_mark("Ann (Site)"), "Ann (Site)");
        assert_eq!(without_mark("(Discord)"), "(Discord)");
        assert_eq!(without_mark("Discord"), "Discord");
        assert_eq!(without_mark("Ann (Discord) (Discord)"), "Ann (Discord)");
        assert_eq!(
            Address::parse(without_mark("Bob@coldeve (Discord)")),
            Ok(a("Bob@coldeve"))
        );
    }

    /// Spec: link messages: a reader that knows only the other link messages skips a say message
    /// as a type it does not know, and the say messages' reader skips theirs.
    #[test]
    fn a_reader_without_say_skips_its_messages() {
        let frame = SayMessage::Result {
            id: "h1".into(),
            outcome: SayOutcome::Said,
            reason: None,
        }
        .to_json();
        let unknown = WireError::Unknown("say.result".into());
        assert_eq!(Message::from_json(&frame), Err(unknown.clone()));
        assert_eq!(DirectoryMessage::from_json(&frame), Err(unknown.clone()));
        assert_eq!(ReachMessage::from_json(&frame), Err(unknown));
        assert_eq!(
            SayMessage::from_json(
                r#"{"type":"room.say","id":"r1","room":"global","from":"Ann@frost","text":"x"}"#
            ),
            Err(WireError::Unknown("room.say".into()))
        );
    }

    /// Spec: link messages: a say names a room and one character, never a channel, its text
    /// within the wire's limit, and its mark; its answer is said or refused, a reason with a
    /// refusal alone.
    #[test]
    fn a_say_names_a_room_and_a_character_and_its_answer_is_said_or_refused() {
        let ask = |room: &str, who: &str, text: &str, via: &str| {
            format!(
                r#"{{"type":"say","id":"h1","room":"{room}","who":"{who}","text":"{text}","via":"{via}"}}"#
            )
        };
        assert!(!malformed(&ask("global", "Ann@coldeve", "hi", "discord")));
        assert!(malformed(&ask("Global", "Ann@coldeve", "hi", "discord")));
        assert!(malformed(&ask(
            "global",
            "oldguard@channel",
            "hi",
            "discord"
        )));
        assert!(malformed(&ask("global", "Ann", "hi", "discord")));
        assert!(malformed(&ask("global", "Ann@coldeve", "hi", "")));
        assert!(malformed(&ask("global", "Ann@coldeve", "hi", "Discord")));
        assert!(malformed(&ask(
            "global",
            "Ann@coldeve",
            &"x".repeat(crate::text::MAX_TEXT_CHARS + 1),
            "discord"
        )));
        assert!(malformed(
            r#"{"type":"say","id":"h1","room":"global","who":"Ann@coldeve","text":"hi"}"#
        ));
        assert!(malformed(
            r#"{"type":"say","id":"","room":"global","who":"Ann@coldeve","text":"hi","via":"discord"}"#
        ));
        assert!(malformed(
            r#"{"type":"say.result","id":"h1","outcome":"said","reason":"gagged"}"#
        ));
        assert!(malformed(
            r#"{"type":"say.result","id":"h1","outcome":"refused","reason":"Gagged!"}"#
        ));
        assert!(malformed(
            r#"{"type":"say.result","id":"h1","outcome":"ok"}"#
        ));
        assert!(malformed(r#"{"type":"say.result","id":"h1"}"#));
        assert_eq!(
            SayMessage::from_json(
                r#"{"type":"say.result","id":"h1","outcome":"refused","reason":"a_later_reason"}"#
            ),
            Ok(SayMessage::Result {
                id: "h1".into(),
                outcome: SayOutcome::Refused,
                reason: Some("a_later_reason".into()),
            }),
            "a reason this version does not name still reads"
        );
    }
}
