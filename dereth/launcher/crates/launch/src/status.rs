//! A world's live status, from Empyrean's public status document (`GET /v1/world`).
//!
//! The document is small and public: whether the world is open, how many are on, the era it plays,
//! and what its dats are. The dats matter most. They are what the server will compare, so when the document says them
//! they win over the registry's published numbers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::datset::Iterations;
use crate::world::{parse_iterations, WorldState};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LiveStatus {
    pub state: WorldState,
    pub players: Option<u32>,
    pub dats: Option<Iterations>,
    pub patching: Option<bool>,
    pub client_versions: Vec<String>,
    pub auto_create_accounts: Option<bool>,
    pub version: Option<String>,
    /// The era the world plays (`"eor"`, `"infiltration"`, ...).
    #[serde(default)]
    pub era: Option<String>,
}

/// Read the document. `None` if it is not one.
pub fn parse_world_document(body: &[u8]) -> Option<LiveStatus> {
    let v: Value = serde_json::from_slice(body).ok()?;
    let open = v.get("world_open")?.as_bool()?;
    let shutting = v
        .get("shutting_down")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Neither open nor going down: still starting.
    let state = match (open, shutting) {
        (_, true) => WorldState::Offline,
        (true, false) => WorldState::Online,
        (false, false) => WorldState::Starting,
    };
    let dats = v.get("dats");
    Some(LiveStatus {
        state,
        players: v
            .get("players_online")
            .and_then(Value::as_u64)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        dats: dats.and_then(parse_iterations),
        patching: dats
            .and_then(|d| d.get("patching"))
            .and_then(Value::as_bool),
        client_versions: v
            .get("client_versions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        auto_create_accounts: v
            .get("accounts")
            .and_then(|a| a.get("auto_create"))
            .and_then(Value::as_bool),
        version: v.get("version").and_then(Value::as_str).map(str::to_owned),
        era: v
            .get("era")
            .and_then(Value::as_str)
            .filter(|e| !e.is_empty())
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_example_reads() {
        let s = parse_world_document(
            br#"{"schema":1,"world_name":"Eulmore","version":"0.9.0",
            "world_open":true,"shutting_down":false,"players_online":12,"uptime_seconds":86400,
            "dats":{"portal":2072,"cell":982,"local":994,"highres":497,"patching":false},
            "client_versions":["1802"],"accounts":{"auto_create":true},"content_hash":"x"}"#,
        )
        .unwrap();
        assert_eq!(s.state, WorldState::Online);
        assert_eq!(s.players, Some(12));
        assert_eq!(s.dats, Some(Iterations::END_OF_RETAIL));
        assert_eq!(s.patching, Some(false));
        assert_eq!(s.client_versions, ["1802"]);
        assert_eq!(s.auto_create_accounts, Some(true));
    }

    #[test]
    fn empyrean_names_the_era_its_world_plays() {
        let s = parse_world_document(
            br#"{"world_name":"Test","world_open":true,"shutting_down":false,"players_online":0,
            "content_hash":"x","corrections_digest":"v1:0","era":"infiltration",
            "dats":{"portal":2072,"cell":982,"local":994,"highres":497,"patching":false},
            "client_versions":["1802"],"websocket_url":null,"not_ported":{}}"#,
        )
        .unwrap();
        assert_eq!(s.era.as_deref(), Some("infiltration"));
        assert_eq!(s.dats, Some(Iterations::END_OF_RETAIL));
        assert_eq!(s.client_versions, ["1802"]);
        let s = parse_world_document(br#"{"world_open":true}"#).unwrap();
        assert_eq!(s.era, None, "a document without an era says nothing");
    }

    #[test]
    fn starting_and_stopping_are_told_apart() {
        let s = parse_world_document(br#"{"world_open":false,"shutting_down":false}"#).unwrap();
        assert_eq!(s.state, WorldState::Starting);
        let s = parse_world_document(br#"{"world_open":true,"shutting_down":true}"#).unwrap();
        assert_eq!(s.state, WorldState::Offline);
    }

    #[test]
    fn a_null_highres_is_unknown_and_junk_is_nothing() {
        let s = parse_world_document(br#"{"world_open":true,"dats":{"portal":1,"highres":null}}"#)
            .unwrap();
        assert_eq!(s.dats.unwrap().highres, None);
        assert_eq!(parse_world_document(b"<html>"), None);
        assert_eq!(parse_world_document(br#"{"status":"ok"}"#), None);
    }
}
