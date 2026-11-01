//! The stream's timings and bounds, and how a stream ends: what both sessions share.

use core::time::Duration;

use crate::element::ByeReason;

/// The keep-alive interval a world names by default: each side sends `Ping` after this long of
/// sending nothing.
pub const PING: Duration = Duration::from_secs(20);

/// Each side closes a stream it has heard nothing on for this long.
pub const SILENCE: Duration = Duration::from_secs(70);

/// A world closes a stream that has not said `Hello` this long after opening, and a client gives up
/// one the world has not welcomed this long after opening.
pub const HELLO_DEADLINE: Duration = Duration::from_secs(10);

/// A world closes a stream that has not presented a ticket this long after `Welcome`.
pub const TICKET_DEADLINE: Duration = Duration::from_secs(30);

/// The client's waits between attempts to connect: 1, 2, 5 and 10 seconds, then every 30.
pub const RECONNECT_WAITS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(30),
];

/// A stream that stood this long starts the client's waits again from the first.
pub const STOOD: Duration = Duration::from_secs(60);

/// The elements a world takes from a stream at once before it holds the stream to [`RATE`].
pub const BURST: u32 = 20;

/// The elements a second a world takes from a stream after the [`BURST`].
pub const RATE: u32 = 5;

/// How a stream ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The client's attempt to connect failed.
    DialFailed,
    /// The first bytes were not the preamble, or did not come in time (`websocket`: they began a
    /// WebSocket upgrade, which this version does not serve).
    NotAStream { websocket: bool },
    /// The connection ended without a farewell.
    Lost,
    /// The other side said goodbye.
    PeerBye(ByeReason),
    /// This side said goodbye: a deadline, a bound, the other side's broken protocol, or its
    /// host's wish.
    OwnBye(ByeReason),
}

/// Seconds as a duration.
pub(crate) fn secs(n: u32) -> Duration {
    Duration::from_secs(u64::from(n))
}
