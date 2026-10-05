//! A world's live status, from Empyrean's public status document (`GET /v1/world`) or from its
//! status ping on the game port (`dereth_transport::status_ping`).
//!
//! The document is small and public: whether the world is open, how many are on, the era it plays
//! and the systems it has, and what its dats are. The dats matter most. They are what the server will compare, so when the document says them
//! they win over the registry's published numbers.
//!
//! The status ping says less, and needs no web address: whether the world is open, how many are
//! on, its era and systems, the server software and its version, and the world's name.

use dereth_primitives::EraFeatureBits;
use dereth_transport::status_ping::{StatusReply, WorldState as PingState};
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
    /// The systems the world has, as the document's `features` object lists them, in the form the
    /// Dereth client's `--era-features` reads (`ratings=false,trade=true,...`).
    #[serde(default)]
    pub era_features: Option<String>,
    /// The server software, when the world says it (`Empyrean`).
    #[serde(default)]
    pub software: Option<String>,
    /// The world's own name for itself.
    #[serde(default)]
    pub world_name: Option<String>,
}

/// Read a status ping's reply ([`StatusReply`]). The systems are kept by name, as far as this
/// build's table and the world's both name them; the rest are left to the era's table.
#[must_use]
pub fn from_status_reply(r: &StatusReply) -> LiveStatus {
    let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_owned());
    let bits = EraFeatureBits {
        table_version: r.era_table_version,
        bytes: r.era_features.clone(),
    };
    let features = bits.overrides();
    LiveStatus {
        state: match r.state {
            PingState::Open => WorldState::Online,
            PingState::Starting => WorldState::Starting,
            PingState::ShuttingDown => WorldState::Offline,
            PingState::Unknown(_) => WorldState::Unknown,
        },
        players: Some(u32::from(r.players)),
        version: some(&r.software_version),
        era: some(&r.era),
        era_features: (!features.is_empty()).then(|| features.to_string()),
        software: some(&r.software),
        world_name: some(&r.world_name),
        ..LiveStatus::default()
    }
}

/// The document's `features` object (`{"ratings":false,"trade":true,...}`) in the `--era-features`
/// form, by name. Entries that are not booleans are skipped; `None` for no object
/// or an empty one.
fn era_features(v: &Value) -> Option<String> {
    let list: Vec<String> = v
        .as_object()?
        .iter()
        .filter_map(|(name, on)| Some(format!("{name}={}", on.as_bool()?)))
        .collect();
    (!list.is_empty()).then(|| list.join(","))
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
        era_features: v.get("features").and_then(era_features),
        software: None,
        world_name: v
            .get("world_name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
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
        assert_eq!(s.era_features, None);
    }

    #[test]
    fn empyrean_lists_the_systems_its_world_has() {
        let s = parse_world_document(
            br#"{"world_open":true,"era":"infiltration",
            "features":{"ratings":false,"aetheria":true,"trade":true,"chess":false,"later":7}}"#,
        )
        .unwrap();
        assert_eq!(
            s.era_features.as_deref(),
            Some("aetheria=true,chess=false,ratings=false,trade=true")
        );
        let s = parse_world_document(br#"{"world_open":true,"features":{}}"#).unwrap();
        assert_eq!(s.era_features, None);
    }

    #[test]
    fn a_status_ping_reply_reads_as_the_live_status() {
        use dereth_primitives::{EraFeatureBits, EraFeatures, EraId};
        let has = EraFeatures {
            aetheria: true,
            ..EraId::Infiltration.features()
        };
        let bits = EraFeatureBits::of(has);
        let mut r = StatusReply {
            format_version: 1,
            state: PingState::Open,
            players: 7,
            era: "infiltration".into(),
            era_table_version: bits.table_version,
            era_features: bits.bytes,
            software: "Empyrean".into(),
            software_version: "0.2.0".into(),
            world_name: "Loopback".into(),
        };
        let s = from_status_reply(&r);
        assert_eq!(s.state, WorldState::Online);
        assert_eq!(s.players, Some(7));
        assert_eq!(s.era.as_deref(), Some("infiltration"));
        assert_eq!(s.software.as_deref(), Some("Empyrean"));
        assert_eq!(s.version.as_deref(), Some("0.2.0"));
        assert_eq!(s.world_name.as_deref(), Some("Loopback"));
        let (o, unknown) =
            dereth_primitives::EraFeatureOverrides::parse(s.era_features.as_deref().unwrap())
                .unwrap();
        assert!(unknown.is_empty());
        assert_eq!(o.apply(EraFeatures::NONE), has);
        assert_eq!(s.dats, None, "the ping says nothing of the dats");

        r.state = PingState::ShuttingDown;
        assert_eq!(from_status_reply(&r).state, WorldState::Offline);
        r.state = PingState::Starting;
        assert_eq!(from_status_reply(&r).state, WorldState::Starting);
        // A table version 0 names no systems: they are unknown, not off.
        r.era_table_version = 0;
        r.world_name = "  ".into();
        let s = from_status_reply(&r);
        assert_eq!(s.era_features, None);
        assert_eq!(s.world_name, None);
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
