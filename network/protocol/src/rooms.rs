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

use crate::game::Game;
use serde::{Deserialize, Serialize};

use crate::wire::{WireError, GLOBAL_ROOM, MAX_ID_CHARS, MAX_MESSAGE_BYTES};

/// A world's own rooms, by their names on the link: the TurbineChat rooms a world may carry the
/// Global room in, and those the hub may mirror into Discord for it alone.
pub const OWN_ROOMS: &[&str] = &["general", "trade", "lfg", "roleplay"];

/// What each of [`OWN_ROOMS`] is called in a game, in their order: the rooms a player of that game
/// sees. Asheron's Call 2 has no LFG or Roleplay channel, and carries both names in PvP.
#[must_use]
pub const fn own_room_titles(game: Game) -> [&'static str; 4] {
    match game {
        Game::Dereth => ["General", "Trade", "LFG", "Roleplay"],
        Game::Auberean => ["General", "Trade", "PvP", "PvP"],
    }
}

/// The word that says a world carries the Global room in none of its rooms.
pub const OFF: &str = "off";

/// Whether `s` names where a world carries the Global room: one of [`OWN_ROOMS`], or [`OFF`].
#[must_use]
pub fn is_global_room(s: &str) -> bool {
    s == OFF || OWN_ROOMS.contains(&s)
}

/// What a world answers `@dn global` with when it carries the Global room in none of its rooms.
pub const NOT_CARRIED: &str = "This world does not carry the Global room.";

/// How long the lines of one of a world's own rooms stay on the world, unsent, after the hub says
/// it does not mirror the room into Discord, in seconds. The next line after it goes to the hub
/// again, so a room its operator has mirrored since is carried within the minute.
pub const UNCARRIED_SECS: f64 = 60.0;

/// A world's `federation.global_room`: which of its own rooms carries the Global room until its hub
/// keeps one. Left out, `unset`, the world's game's own choice; empty, none; otherwise one of
/// [`OWN_ROOMS`], in any case.
///
/// # Errors
/// A name that is not one of the four rooms.
pub fn read_setting(
    named: Option<&str>,
    unset: &'static str,
) -> Result<Option<&'static str>, String> {
    match named.map(str::trim) {
        None => Ok(Some(unset)),
        Some("") => Ok(None),
        Some(name) => {
            let word = name.to_ascii_lowercase();
            OWN_ROOMS
                .iter()
                .find(|r| **r == word)
                .map(|r| Some(*r))
                .ok_or_else(|| {
                    format!(
                        "federation.global_room {name:?}: general, trade, lfg, roleplay or empty"
                    )
                })
        }
    }
}

/// A world's rooms as its hub last told them (`rooms`), kept with the network's records so that it
/// carries them while the hub is away.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptRooms {
    /// The room that carries the Global room (one of the world's own rooms, or [`OFF`]); none while
    /// the hub held none for the world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global: Option<String>,
    /// The world's own rooms the hub mirrors into Discord for it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mirrored: Vec<String>,
}

impl KeptRooms {
    /// What a world keeps once its hub has told it `global` and `mirrored`: a hub that holds no
    /// room leaves the one it last told the world.
    #[must_use]
    pub fn told(before: Option<&Self>, global: Option<&str>, mirrored: &[String]) -> Self {
        Self {
            global: global
                .map(str::to_owned)
                .or_else(|| before.and_then(|r| r.global.clone())),
            mirrored: mirrored
                .iter()
                .filter(|r| OWN_ROOMS.contains(&r.as_str()))
                .cloned()
                .collect(),
        }
    }
}

/// Where a line said in `room` goes past its world, as its speaker is told.
fn reach(room: &str) -> &'static str {
    if room == GLOBAL_ROOM {
        "the other worlds"
    } else {
        "Discord"
    }
}

/// What a speaker is told when its line in `room` stayed on its world because it sent too quickly.
#[must_use]
pub fn too_quickly_line(room: &str) -> String {
    format!(
        "Your line did not reach {}: you are sending too quickly.",
        reach(room)
    )
}

/// What a speaker is told when its line in `room` stayed on its world because the hub refused it:
/// the speaker's own pace was kept there, so it is the world's share of the room that is spent.
#[must_use]
pub fn held_back_line(room: &str) -> String {
    format!(
        "Your line did not reach {}: the Dereth Network is holding back this world's lines for now.",
        reach(room)
    )
}

/// What a speaker is told when its line in `room` stayed on its world because the character is
/// younger than the operator's minimum age.
#[must_use]
pub fn too_new_line(room: &str) -> String {
    format!(
        "Your line did not reach {}: this character is too new to send to the Dereth Network yet.",
        reach(room)
    )
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

    /// Spec: hub: an operator is shown each room by what a player of the world's game calls it.
    #[test]
    fn each_of_a_worlds_rooms_has_the_name_its_game_gives_it() {
        assert_eq!(OWN_ROOMS.len(), own_room_titles(Game::Dereth).len());
        assert_eq!(
            own_room_titles(Game::Dereth),
            ["General", "Trade", "LFG", "Roleplay"]
        );
        assert_eq!(
            own_room_titles(Game::Auberean),
            ["General", "Trade", "PvP", "PvP"]
        );
    }

    /// Spec: world behaviour: `federation.global_room` left out is the game's own choice, empty is none, and
    /// anything but the four rooms is refused.
    #[test]
    fn the_global_room_setting_takes_the_four_rooms_or_empty() {
        assert_eq!(read_setting(None, "general"), Ok(Some("general")));
        assert_eq!(read_setting(None, "roleplay"), Ok(Some("roleplay")));
        assert_eq!(read_setting(Some(""), "general"), Ok(None));
        assert_eq!(read_setting(Some("  "), "general"), Ok(None));
        for room in OWN_ROOMS {
            assert_eq!(read_setting(Some(room), "general"), Ok(Some(*room)));
        }
        assert_eq!(read_setting(Some(" Trade "), "general"), Ok(Some("trade")));
        for bad in ["off", "global", "pvp", "society"] {
            let why = read_setting(Some(bad), "general").expect_err(bad);
            assert!(why.starts_with("federation.global_room"), "{why}");
        }
    }

    /// Spec: world behaviour: a hub that holds no room for the world leaves the one it last told it, and a
    /// mirrored room it does not know is left out.
    #[test]
    fn a_world_keeps_the_last_room_its_hub_held() {
        let before = KeptRooms::told(None, Some("trade"), &["general".to_owned()]);
        assert_eq!(before.global.as_deref(), Some("trade"));
        let after = KeptRooms::told(Some(&before), None, &["pvp".to_owned(), "lfg".to_owned()]);
        assert_eq!(after.global.as_deref(), Some("trade"));
        assert_eq!(after.mirrored, ["lfg"]);
        assert_eq!(KeptRooms::told(None, None, &[]), KeptRooms::default());
    }

    /// Spec: world behaviour: a speaker is told of the other worlds for a Global line, of Discord for one of
    /// the world's own rooms.
    #[test]
    fn a_speakers_line_names_where_the_line_did_not_go() {
        assert_eq!(
            too_quickly_line(GLOBAL_ROOM),
            "Your line did not reach the other worlds: you are sending too quickly."
        );
        assert_eq!(
            too_quickly_line("general"),
            "Your line did not reach Discord: you are sending too quickly."
        );
        assert!(held_back_line("trade").starts_with("Your line did not reach Discord: "));
        assert!(too_new_line(GLOBAL_ROOM).contains("the other worlds: this character is too new"));
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
