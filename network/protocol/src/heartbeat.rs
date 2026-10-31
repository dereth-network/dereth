//! The heartbeat a world sends the hub over HTTP: `POST /v1/servers/<slug>/heartbeat` with the
//! world's key as `Authorization: Bearer drth_...`. It keeps the world's status fresh in the
//! registry, as `status` does over a link, for a world that holds no link; and it may name the
//! world's game ([`Game`]), which becomes the world's.

use serde::{Deserialize, Serialize};

use crate::Game;

/// The states a world reports, as a link's `status` names them too.
pub const STATES: [&str; 4] = ["online", "degraded", "maintenance", "offline"];

/// How a world counts the players it reports: the characters in the world.
pub const CONNECTED_CHARACTERS: &str = "connected-characters";

/// The kinds of software a heartbeat may say sent it: the mod for a stock ACE world, an agent
/// beside a world, an operator by hand, and the two servers with the network built in, Empyrean
/// (Asheron's Call) and Yalain (Asheron's Call 2).
pub const ADAPTER_KINDS: [&str; 5] = ["ace-mod", "agent", "manual", "empyrean", "yalain"];

/// What sent a heartbeat: one of [`ADAPTER_KINDS`], and its version (1 to 40 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adapter {
    pub kind: String,
    pub version: String,
}

/// A heartbeat, as a world writes it. Every field but `state` may be left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    /// One of [`STATES`].
    pub state: String,
    /// How many players are in the world: none while it is `offline`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players_online: Option<u32>,
    /// How they are counted ([`CONNECTED_CHARACTERS`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players_kind: Option<String>,
    /// The world's software's version, at most 60 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emulator_version: Option<String>,
    /// What sent the heartbeat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Adapter>,
    /// The game the world runs; a heartbeat that names none leaves the world's game as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<Game>,
}

/// The route a world's heartbeat is posted to, under the hub's address.
#[must_use]
pub fn path(slug: &str) -> String {
    format!("/v1/servers/{slug}/heartbeat")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: a heartbeat is written with the registry's own field names, leaving out what it
    /// does not say.
    #[test]
    fn a_heartbeat_writes_the_registrys_field_names_and_leaves_out_what_it_does_not_say() {
        let full = Heartbeat {
            state: "online".into(),
            players_online: Some(4),
            players_kind: Some(CONNECTED_CHARACTERS.into()),
            emulator_version: Some("0.1.0".into()),
            adapter: Some(Adapter {
                kind: "yalain".into(),
                version: "0.1.0".into(),
            }),
            game: Some(Game::Auberean),
        };
        assert_eq!(
            serde_json::to_value(&full).expect("written"),
            serde_json::json!({
                "state": "online", "playersOnline": 4, "playersKind": "connected-characters",
                "emulatorVersion": "0.1.0", "adapter": { "kind": "yalain", "version": "0.1.0" },
                "game": "auberean",
            })
        );
        let bare = Heartbeat {
            state: "offline".into(),
            players_online: None,
            players_kind: None,
            emulator_version: None,
            adapter: None,
            game: None,
        };
        assert_eq!(
            serde_json::to_string(&bare).expect("written"),
            r#"{"state":"offline"}"#
        );
        assert_eq!(path("shardfall"), "/v1/servers/shardfall/heartbeat");
        assert!(STATES.contains(&full.state.as_str()));
        assert!(ADAPTER_KINDS.contains(&"yalain"));
    }
}
