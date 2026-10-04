//! Not ACE: `[dat_overlay]`, the world's data overlay over the base data files.
//!
//! A world whose data differs from the base files keeps only the difference: the overlay folder
//! `empyrean-import dat-overlay` writes, one container per file it changes. The server reads its
//! world as the base files with the overlay over them, and patches clients that keep overlays of
//! their own (the Dereth client) to the same world.

use serde::{Deserialize, Serialize};

/// The world's data overlay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatOverlayConfiguration {
    /// The overlay folder; empty for a world that is its base files.
    #[serde(rename = "Path")]
    pub path: String,
    /// Patch a client that keeps overlays (one that says so in its interrogation response) to the
    /// world: send it the overlay's manifest, its revisions and the world's cell records in the
    /// patch. Off, such a client missing the overlay is refused as a client missing iterations
    /// is with `[ddd] enable_dat_patching` off.
    #[serde(rename = "Patching")]
    pub patching: bool,
}

impl Default for DatOverlayConfiguration {
    fn default() -> Self {
        Self {
            path: String::new(),
            patching: true,
        }
    }
}
