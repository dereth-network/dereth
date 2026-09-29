// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/PreloadedLandblock.cs
//! `PreloadedLandblocks` (`Config.js` → `Server.PreloadedLandblocks[]`). ACE's file is
//! `PreloadedLandblock.cs`; the class is plural.

use serde::{Deserialize, Serialize};

// ACE: PreloadedLandblocks
/// One landblock to load at startup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreloadedLandblocks {
    // ACE: PreloadedLandblocks.Id
    /// The landblock id as hex (`"A9B4FFFF"`); `null` by default.
    #[serde(rename = "Id")]
    pub id: Option<String>,

    // ACE: PreloadedLandblocks.Description
    /// Free text; `null` by default.
    #[serde(rename = "Description")]
    pub description: Option<String>,

    // ACE: PreloadedLandblocks.Permaload
    /// Never unload.
    #[serde(rename = "Permaload")]
    pub permaload: bool,

    // ACE: PreloadedLandblocks.IncludeAdjacents
    /// Also load (and, with `Permaload`, permaload) the adjacent landblocks.
    #[serde(rename = "IncludeAdjacents")]
    pub include_adjacents: bool,

    // ACE: PreloadedLandblocks.Enabled
    /// Whether this entry is used.
    #[serde(rename = "Enabled")]
    pub enabled: bool,
}
