//! Not ACE: `[status]`, how the world answers questions about itself without a login.
//!
//! The status ping is answered on the game port: a launcher that knows only a world's host and
//! port learns whether it is open, how many are on, the era and systems it plays, the server
//! software and the world's name, in two small datagrams and with no session. It is on unless
//! turned off; off, the ping is dropped as a server without it drops it.

use serde::{Deserialize, Serialize};

/// `empyrean.toml` → `[status]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusConfiguration {
    /// Answer the status ping on the game port.
    #[serde(rename = "UdpPing")]
    pub udp_ping: bool,
    /// Hellos (the step that hands out a token) a minute from one address; the excess is dropped.
    #[serde(rename = "HellosPerMinute")]
    pub hellos_per_minute: u32,
    /// Asks (the step that is answered with the status) a minute from one address.
    #[serde(rename = "AsksPerMinute")]
    pub asks_per_minute: u32,
    /// Status-ping replies a second, to everyone together.
    #[serde(rename = "RepliesPerSecond")]
    pub replies_per_second: u32,
}

impl Default for StatusConfiguration {
    fn default() -> Self {
        Self {
            udp_ping: true,
            hellos_per_minute: 4,
            asks_per_minute: 4,
            replies_per_second: 50,
        }
    }
}
