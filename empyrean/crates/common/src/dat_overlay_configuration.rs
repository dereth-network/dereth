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
    /// How many patch records a minute a client that keeps overlays is sent; 0 keeps ACE's
    /// 1,000 a minute shared by every session. A retail client always has ACE's.
    #[serde(rename = "RecordsPerMinute")]
    pub records_per_minute: u32,
    /// The most patch bytes a second such a client is sent. The client's receive buffer is
    /// 128 KiB and it reads its socket once a frame, so a budget of about a frame's worth of that
    /// buffer a tenth of a second keeps the patch from overflowing it and losing datagrams.
    #[serde(rename = "BytesPerSecond")]
    pub bytes_per_second: u32,
}

/// The default [`DatOverlayConfiguration::bytes_per_second`]: 1 MiB.
pub const DEFAULT_BYTES_PER_SECOND: u32 = 1 << 20;

/// The default [`DatOverlayConfiguration::records_per_minute`]: a thousand a second.
pub const DEFAULT_RECORDS_PER_MINUTE: u32 = 60_000;

impl Default for DatOverlayConfiguration {
    fn default() -> Self {
        Self {
            path: String::new(),
            patching: true,
            records_per_minute: DEFAULT_RECORDS_PER_MINUTE,
            bytes_per_second: DEFAULT_BYTES_PER_SECOND,
        }
    }
}
