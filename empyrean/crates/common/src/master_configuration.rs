// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/MasterConfiguration.cs
//! `MasterConfiguration`: the root of the configuration (`empyrean.toml`, read by
//! [`crate::toml_config`]). The serde names are ACE's `Config.js` names, which the one-time
//! `Config.js` converter reads.

use serde::{Deserialize, Serialize};

use crate::database_configuration::DatabaseConfiguration;
use crate::ddd_configuration::DDDConfiguration;
use crate::era::EraConfiguration;
use crate::game_configuration::GameConfiguration;
use crate::offline_configuration::OfflineConfiguration;

// ACE: MasterConfiguration
/// The whole configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MasterConfiguration {
    // ACE: MasterConfiguration.Server
    /// `Server`.
    #[serde(rename = "Server")]
    pub server: GameConfiguration,

    // ACE: MasterConfiguration.MySql
    /// `MySql` in ACE; `[database]` in `empyrean.toml`.
    #[serde(rename = "MySql")]
    pub database: DatabaseConfiguration,

    // ACE: MasterConfiguration.Offline
    /// `Offline`.
    #[serde(rename = "Offline")]
    pub offline: OfflineConfiguration,

    // ACE: MasterConfiguration.DDD
    /// `DDD`.
    #[serde(rename = "DDD")]
    pub ddd: DDDConfiguration,

    /// Not ACE: the era the world plays (`[era]`).
    /// DIVERGE: an extra section; ACE has one set of rules.
    #[serde(rename = "Era")]
    pub era: EraConfiguration,
}
