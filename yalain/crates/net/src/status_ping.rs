//! The status ping (`docs/networking/06-status-ping.md`): a world's live status over UDP on its
//! login port, asked in two steps with no session and no account, as Empyrean answers it on its
//! game port. A launcher, or a hub trying the world from outside, learns whether the world is open,
//! how many are in it, the server and its version, and the world's name.
//!
//! - A **hello** is answered with a token only, smaller than the hello, so a forged source address
//!   draws nothing larger than it sent.
//! - An **ask** carrying a token this server gave the same address and port, in its window or the
//!   one after, is answered with the status; anything else gets no answer.
//!
//! The datagrams are the shared transport's (`dereth_transport::status_ping`). The token is this
//! module's own, as the specification describes it: a window of [`WINDOW_SECONDS`], and a code
//! that is an HMAC-SHA256 of the asker's address, port and window under a secret drawn from the
//! system on the first request and again every [`SECRET_WINDOWS`] windows; the secret before is
//! kept through the first window under a new one, so no handshake in flight is broken, then
//! forgotten. Secrets are never written or logged.
//!
//! Each address may send [`Limits::hellos_per_minute`] hellos and [`Limits::asks_per_minute`]
//! asks a minute, at most [`MAX_ADDRESSES`] addresses are counted in a minute, and the server sends
//! at most [`Limits::replies_per_second`] replies a second in all: the rest are dropped without an
//! answer, and their count is logged once a minute. Asheron's Call 2 has no era, so the status
//! names none and no systems.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use dereth_transport::status_ping::{self, Request, StatusReply, Token, TOKEN_MAC_LEN};
pub use dereth_transport::status_ping::{is_status_ping, WorldState};
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// A window's length, in seconds.
pub const WINDOW_SECONDS: u64 = 30;

/// The windows one secret serves: ten minutes.
pub const SECRET_WINDOWS: u64 = 20;

/// The most source addresses counted in one minute.
pub const MAX_ADDRESSES: usize = 4096;

/// How many requests the server answers, as the specification's defaults have them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Hellos a minute from one address.
    pub hellos_per_minute: u32,
    /// Asks a minute from one address.
    pub asks_per_minute: u32,
    /// Replies a second, tokens and statuses together, to everyone.
    pub replies_per_second: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            hellos_per_minute: 4,
            asks_per_minute: 4,
            replies_per_second: 50,
        }
    }
}

/// What the status says: the server sets the lasting facts once, and the state and players as
/// the world runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub state: WorldState,
    pub players: u16,
    /// The server software (`Yalain`) and its version.
    pub software: String,
    pub version: String,
    pub world_name: String,
}

/// Where secrets come from: the system's random source in the server, a fixed sequence in tests.
pub type Secrets = Box<dyn FnMut() -> [u8; 32] + Send>;

/// One address's requests this minute.
#[derive(Debug, Default, Clone, Copy)]
struct Asked {
    hellos: u32,
    asks: u32,
}

/// The status ping's state: whether it is answered, its limits, what it says, the secrets and the
/// counts.
pub struct StatusPing {
    pub on: bool,
    pub limits: Limits,
    pub facts: Facts,
    draw: Secrets,
    /// The secret's period (window / [`SECRET_WINDOWS`]) and the secret; `None` before the first.
    secret: Option<(u64, [u8; 32])>,
    /// The period before's secret, while the first window of a new one runs.
    before: Option<(u64, [u8; 32])>,
    minute: u64,
    asked: HashMap<IpAddr, Asked>,
    second: u64,
    replies: u32,
    /// Requests over the limits since the count was last logged.
    dropped: u64,
}

impl std::fmt::Debug for StatusPing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secrets.
        f.debug_struct("StatusPing")
            .field("on", &self.on)
            .field("limits", &self.limits)
            .field("facts", &self.facts)
            .field("dropped", &self.dropped)
            .finish_non_exhaustive()
    }
}

impl StatusPing {
    #[must_use]
    pub fn new(on: bool, limits: Limits, facts: Facts, draw: Secrets) -> Self {
        Self {
            on,
            limits,
            facts,
            draw,
            secret: None,
            before: None,
            minute: 0,
            asked: HashMap::new(),
            second: 0,
            replies: 0,
            dropped: 0,
        }
    }

    /// A status-ping datagram from `from`, `seconds` into the server's monotonic clock: the
    /// datagram to send back from the port it came to, or `None` for no answer.
    pub fn answer(&mut self, from: SocketAddr, datagram: &[u8], seconds: u64) -> Option<Vec<u8>> {
        if !self.on {
            return None;
        }
        let request = status_ping::parse_request(datagram)?;
        self.count_from(seconds);
        if !self.admitted(from.ip(), &request) {
            self.dropped += 1;
            return None;
        }
        let window = seconds / WINDOW_SECONDS;
        self.keep_secrets(window);
        let reply = match request {
            Request::Hello => status_ping::token_reply(self.token(from, window)?),
            Request::Ask(token) if self.good(from, token, window) => self.status(),
            Request::Ask(_) => return None,
        };
        if self.replies >= self.limits.replies_per_second {
            self.dropped += 1;
            return None;
        }
        self.replies += 1;
        Some(reply)
    }

    /// The status as it stands.
    #[must_use]
    pub fn status(&self) -> Vec<u8> {
        StatusReply {
            format_version: status_ping::REPLY_FORMAT_VERSION,
            state: self.facts.state,
            players: self.facts.players,
            era: String::new(),
            era_table_version: 0,
            era_features: Vec::new(),
            software: self.facts.software.clone(),
            software_version: self.facts.version.clone(),
            world_name: self.facts.world_name.clone(),
        }
        .encode()
    }

    /// A new minute clears the counts by address, logging the last one's drops; a new second
    /// clears the replies sent.
    fn count_from(&mut self, seconds: u64) {
        if seconds / 60 != self.minute {
            if self.dropped > 0 {
                tracing::info!(
                    "status ping: {} request(s) over the limits dropped",
                    self.dropped
                );
                self.dropped = 0;
            }
            self.minute = seconds / 60;
            self.asked.clear();
        }
        if seconds != self.second {
            self.second = seconds;
            self.replies = 0;
        }
    }

    /// Whether `ip` may send this request this minute; counted when it may.
    fn admitted(&mut self, ip: IpAddr, request: &Request) -> bool {
        if !self.asked.contains_key(&ip) && self.asked.len() >= MAX_ADDRESSES {
            return false;
        }
        let asked = self.asked.entry(ip).or_default();
        let (n, most) = match request {
            Request::Hello => (&mut asked.hellos, self.limits.hellos_per_minute),
            Request::Ask(_) => (&mut asked.asks, self.limits.asks_per_minute),
        };
        if *n >= most {
            return false;
        }
        *n += 1;
        true
    }

    /// The secrets for `window`: a new period draws a new secret, the one before kept through
    /// the new period's first window when the periods are neighbors.
    fn keep_secrets(&mut self, window: u64) {
        let period = window / SECRET_WINDOWS;
        if self.secret.is_none_or(|(p, _)| p != period) {
            self.before = self.secret.filter(|(p, _)| p + 1 == period);
            self.secret = Some((period, (self.draw)()));
        }
        if window != period * SECRET_WINDOWS {
            self.before = None;
        }
    }

    /// The secret held for `window`, if any.
    fn secret_of(&self, window: u64) -> Option<&[u8; 32]> {
        let period = window / SECRET_WINDOWS;
        [self.secret.as_ref(), self.before.as_ref()]
            .into_iter()
            .flatten()
            .find(|(p, _)| *p == period)
            .map(|(_, s)| s)
    }

    fn code(secret: &[u8; 32], from: SocketAddr, window: u32) -> [u8; TOKEN_MAC_LEN] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("any key length");
        match from.ip() {
            IpAddr::V4(a) => {
                mac.update(&[4]);
                mac.update(&a.octets());
            }
            IpAddr::V6(a) => {
                mac.update(&[6]);
                mac.update(&a.octets());
            }
        }
        mac.update(&from.port().to_le_bytes());
        mac.update(&window.to_le_bytes());
        let full = mac.finalize().into_bytes();
        let mut code = [0u8; TOKEN_MAC_LEN];
        code.copy_from_slice(&full[..TOKEN_MAC_LEN]);
        code
    }

    /// The token for `from` in `window`.
    fn token(&self, from: SocketAddr, window: u64) -> Option<Token> {
        let window = u32::try_from(window).ok()?;
        let secret = self.secret_of(u64::from(window))?;
        Some(Token {
            window,
            mac: Self::code(secret, from, window),
        })
    }

    /// Whether `token` was given to `from` in this window or the one before, under a secret still
    /// held. The codes are compared whole, never byte by byte.
    fn good(&self, from: SocketAddr, token: Token, window: u64) -> bool {
        let given = u64::from(token.window);
        if given != window && given + 1 != window {
            return false;
        }
        let Some(secret) = self.secret_of(given) else {
            return false;
        };
        Self::code(secret, from, token.window)
            .iter()
            .zip(token.mac)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}
