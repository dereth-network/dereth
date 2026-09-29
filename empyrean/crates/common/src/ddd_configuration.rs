// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/DDDConfiguration.cs
//! `DDDConfiguration` (`Config.js` → `DDD`): client dat patching from the server's dats.

use serde::{Deserialize, Serialize};

// ACE: DDDConfiguration
/// Client DAT patching.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DDDConfiguration {
    // ACE: DDDConfiguration.EnableDATPatching
    /// Allow the server to patch client DAT files via `DDDManager`.
    #[serde(rename = "EnableDATPatching")]
    pub enable_dat_patching: bool,

    // ACE: DDDConfiguration.PrecacheCompressedDATFiles
    /// Precache every DAT file that would be sent compressed, at startup.
    #[serde(rename = "PrecacheCompressedDATFiles")]
    pub precache_compressed_dat_files: bool,
}
