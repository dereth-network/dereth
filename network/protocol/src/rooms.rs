//! A world's rooms: which of the world's own rooms carries the Global room, and which of its own
//! rooms the hub mirrors into Discord for it (`docs/network/02-link-protocol.md` section 10,
//! `05-hub.md` section 5.10).
//!
//! The hub keeps them for each world and tells the world (`rooms`) at each `welcome` and whenever
//! either changes; the world applies them at once, and keeps the last it was told, so that it
//! starts right while the hub is away. The world's operator sets the room that carries the Global
//! room on the hub's site, or in game, where the world asks the hub for it (`rooms.global`). A world
//! the hub holds no such room for sends the room it carried it in before (`seed`), once; the hub
//! keeps it only while it holds none.
//!
//! The rooms messages are a family of their own: [`RoomsMessage`] reads them, and
//! [`crate::Message`] skips them as types it does not know, as every reader of a link skips a type
//! it does not know. A world that knows nothing of them keeps its own settings, and a hub that knows
//! nothing of them never tells a world its rooms.

use serde::{Deserialize, Serialize};

use crate::wire::{WireError, MAX_ID_CHARS, MAX_MESSAGE_BYTES};

/// A world's own rooms, by their names on the link: the TurbineChat rooms a world may carry the
/// Global room in, and those the hub may mirror into Discord for it alone.
pub const OWN_ROOMS: &[&str] = &["general", "trade", "lfg", "roleplay"];

/// The word that says a world carries the Global room in none of its rooms.
pub const OFF: &str = "off";

/// Whether `s` names where a world carries the Global room: one of [`OWN_ROOMS`], or [`OFF`].
#[must_use]
pub fn is_global_room(s: &str) -> bool {
    s == OFF || OWN_ROOMS.contains(&s)
}

/// One of the rooms messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RoomsMessage {
    /// Hub to world: the world's rooms. `global` is the own room that carries the Global room, or
    /// [`OFF`]; it is left out while the hub holds none for the world. `mirrored` are the world's
    /// own rooms the hub mirrors into Discord for it now, never the one carrying the Global room.
    /// `id` names the world's `rooms.global` this answers, when it answers one.
    #[serde(rename = "rooms")]
    Setup {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        global: Option<String>,
        mirrored: Vec<String>,
    },
    /// World to hub: carry the Global room in `room` (one of [`OWN_ROOMS`], or [`OFF`]) from now
    /// on. With `seed`, only while the hub holds no room for the world: the room the world carried
    /// it in before the hub kept it.
    #[serde(rename = "rooms.global")]
    Global {
        id: String,
        room: String,
        #[serde(default, skip_serializing_if = "is_false")]
        seed: bool,
    },
}

/// Every `type` of the rooms messages.
pub const TYPES: &[&str] = &["rooms", "rooms.global"];

/// A `seed` that is false is left out of the frame, as a reader takes it when it is.
fn is_false(b: &bool) -> bool {
    !*b
}

impl RoomsMessage {
    /// Reads one frame.
    ///
    /// # Errors
    /// [`WireError::Unknown`] for a type that is not one of the rooms messages,
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
            Self::Setup { .. } => "rooms",
            Self::Global { .. } => "rooms.global",
        }
    }

    /// The checks serde does not make: the ids, the rooms named, and each mirrored room once and
    /// never the one carrying the Global room.
    fn check(&self) -> Result<(), &'static str> {
        let id_ok = |id: &str| !id.is_empty() && id.chars().count() <= MAX_ID_CHARS;
        match self {
            Self::Setup {
                id,
                global,
                mirrored,
            } => {
                if id.as_deref().is_some_and(|id| !id_ok(id)) {
                    return Err("id");
                }
                if global.as_deref().is_some_and(|g| !is_global_room(g)) {
                    return Err("global");
                }
                for (i, room) in mirrored.iter().enumerate() {
                    if !OWN_ROOMS.contains(&room.as_str()) {
                        return Err("mirrored: one of the world's own rooms");
                    }
                    if mirrored[..i].contains(room) {
                        return Err("mirrored: each room once");
                    }
                    if global.as_deref() == Some(room.as_str()) {
                        return Err("mirrored: never the room carrying the Global room");
                    }
                }
                Ok(())
            }
            Self::Global { id, room, .. } => {
                if !id_ok(id) {
                    return Err("id");
                }
                if !is_global_room(room) {
                    return Err("room");
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryMessage, Message, ReachMessage, SayMessage};

    fn malformed(text: &str) -> bool {
        matches!(RoomsMessage::from_json(text), Err(WireError::Malformed(_)))
    }

    /// Spec: link messages: every rooms message round-trips through its JSON form.
    #[test]
    fn every_rooms_message_round_trips_through_its_json() {
        let all = [
            RoomsMessage::Setup {
                id: None,
                global: Some("roleplay".into()),
                mirrored: vec!["general".into(), "trade".into()],
            },
            RoomsMessage::Setup {
                id: Some("g1790000000-3".into()),
                global: Some(OFF.into()),
                mirrored: vec!["roleplay".into()],
            },
            RoomsMessage::Setup {
                id: None,
                global: None,
                mirrored: Vec::new(),
            },
            RoomsMessage::Global {
                id: "g1".into(),
                room: "trade".into(),
                seed: false,
            },
            RoomsMessage::Global {
                id: "g2".into(),
                room: OFF.into(),
                seed: true,
            },
        ];
        for m in all {
            let json = m.to_json();
            assert!(
                json.contains(&format!("\"type\":\"{}\"", m.kind())),
                "{json}"
            );
            assert_eq!(RoomsMessage::from_json(&json), Ok(m), "{json}");
        }
        assert_eq!(
            RoomsMessage::Setup {
                id: None,
                global: Some("roleplay".into()),
                mirrored: vec!["general".into()],
            }
            .to_json(),
            r#"{"type":"rooms","global":"roleplay","mirrored":["general"]}"#
        );
        assert_eq!(
            RoomsMessage::Global {
                id: "g1".into(),
                room: "roleplay".into(),
                seed: true,
            }
            .to_json(),
            r#"{"type":"rooms.global","id":"g1","room":"roleplay","seed":true}"#
        );
    }

    /// Spec: link messages: a reader that knows only the other link messages skips a rooms
    /// message as a type it does not know, and the rooms messages' reader skips theirs.
    #[test]
    fn a_reader_without_rooms_skips_its_messages() {
        let frame = RoomsMessage::Setup {
            id: None,
            global: Some("roleplay".into()),
            mirrored: Vec::new(),
        }
        .to_json();
        let unknown = WireError::Unknown("rooms".into());
        assert_eq!(Message::from_json(&frame), Err(unknown.clone()));
        assert_eq!(DirectoryMessage::from_json(&frame), Err(unknown.clone()));
        assert_eq!(ReachMessage::from_json(&frame), Err(unknown.clone()));
        assert_eq!(SayMessage::from_json(&frame), Err(unknown));
        assert_eq!(
            RoomsMessage::from_json(r#"{"type":"room.join","room":"global","on":true}"#),
            Err(WireError::Unknown("room.join".into()))
        );
    }

    /// Spec: link messages: a world's rooms name one of its own four rooms or `off` for the Global
    /// room, and each mirrored own room once, never the one carrying the Global room; a request
    /// names one of the four or `off`.
    #[test]
    fn a_worlds_rooms_name_its_own_four_rooms() {
        let setup = |global: &str, mirrored: &str| {
            format!(r#"{{"type":"rooms","global":"{global}","mirrored":[{mirrored}]}}"#)
        };
        assert!(!malformed(&setup("roleplay", r#""general","trade","lfg""#)));
        assert!(!malformed(&setup("off", r#""general","roleplay""#)));
        assert!(!malformed(r#"{"type":"rooms","mirrored":["roleplay"]}"#));
        assert!(malformed(&setup("global", "")));
        assert!(malformed(&setup("Roleplay", "")));
        assert!(malformed(&setup("society", "")));
        assert!(malformed(&setup("roleplay", r#""roleplay""#)));
        assert!(malformed(&setup("trade", r#""general","general""#)));
        assert!(malformed(&setup("trade", r#""global""#)));
        assert!(malformed(r#"{"type":"rooms","global":"roleplay"}"#));
        assert!(malformed(
            r#"{"type":"rooms","id":"","global":"roleplay","mirrored":[]}"#
        ));
        let ask = |room: &str| format!(r#"{{"type":"rooms.global","id":"g1","room":"{room}"}}"#);
        assert!(!malformed(&ask("general")));
        assert!(!malformed(&ask("off")));
        assert!(malformed(&ask("global")));
        assert!(malformed(&ask("LFG")));
        assert!(malformed(&ask("")));
        assert!(malformed(r#"{"type":"rooms.global","id":"g1"}"#));
        assert!(malformed(
            r#"{"type":"rooms.global","room":"trade","seed":true}"#
        ));
        assert!(malformed(
            r#"{"type":"rooms.global","id":"g1","room":"trade","seed":"yes"}"#
        ));
        assert_eq!(
            RoomsMessage::from_json(
                r#"{"type":"rooms.global","id":"g1","room":"trade","by":"Ann"}"#
            ),
            Ok(RoomsMessage::Global {
                id: "g1".into(),
                room: "trade".into(),
                seed: false,
            }),
            "a field this version does not know is ignored"
        );
    }
}
