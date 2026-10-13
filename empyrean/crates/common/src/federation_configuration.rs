//! Not ACE: `[federation]`, the Dereth Network: this world's link to the hub that connects it to
//! other worlds, for cross-world tells, friends and rooms.
//!
//! Off unless `enabled` is set; off, the world behaves as ACE does. The world's registry key is
//! never in this file: `key_file` names the file that holds it, which only the server should read.

use serde::{Deserialize, Serialize};

/// `empyrean.toml` → `[federation]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FederationConfiguration {
    /// Link to the hub and turn the network on for this world's players.
    #[serde(rename = "Enabled")]
    pub enabled: bool,
    /// The hub's address: `wss://host[:port]`, or `ws://` on loopback only.
    #[serde(rename = "HubUrl")]
    pub hub_url: String,
    /// The world's slug in the hub's registry: the world half of its characters' addresses.
    #[serde(rename = "Slug")]
    pub slug: String,
    /// The file holding the world's registry key (`drth_...`), relative to this file's folder.
    #[serde(rename = "KeyFile")]
    pub key_file: String,
    /// The TurbineChat room that carries the network's Global room (`general`, `trade`, `lfg`,
    /// `roleplay`, or empty for none) until the hub keeps one for the world: a hub that holds none
    /// is told this one, once, and keeps it; after that the hub's holds, set on its operator page
    /// or with `/dn admin global`. Not set: `roleplay`. To go in a later release.
    #[serde(rename = "GlobalRoom")]
    pub global_room: Option<String>,
    /// Where the network's records are kept (stand-in ids, characters' settings, remote friends
    /// and squelches, blocks), relative to this file's folder.
    #[serde(rename = "StorePath")]
    pub store_path: String,
    /// Seconds a character must have played before it may send to the network.
    #[serde(rename = "MinCharacterAgeSeconds")]
    pub min_character_age_seconds: u32,
}

impl Default for FederationConfiguration {
    fn default() -> Self {
        Self {
            enabled: false,
            hub_url: "wss://hub.dereth.network".to_owned(),
            slug: String::new(),
            key_file: "network.key".to_owned(),
            global_room: None,
            store_path: "network.json".to_owned(),
            min_character_age_seconds: 0,
        }
    }
}
