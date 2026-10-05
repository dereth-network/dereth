//! Not ACE: the status ping, answered on the game port before any session exists.
//!
//! DIVERGE: ACE answers nothing about a world without a login (V442). A launcher that knows only
//! a world's host and port asks its live status in two steps (`dereth_transport::status_ping`,
//! specified in `docs/networking/06-status-ping.md`):
//!
//! - a **hello** is answered with a token only, in a datagram smaller than the hello, so a forged
//!   source address gains nothing;
//! - an **ask** carrying a token this server issued to the same address and port within the last
//!   two windows is answered with the status; anything else is dropped without a word.
//!
//! The token is stateless: `HMAC-SHA256(secret, address ‖ port ‖ window)` cut to 16 bytes, with
//! the window number beside it. A window is [`WINDOW_SECONDS`]; a token is good in its own window
//! and the next. The secret is drawn at start and again every [`SECRET_WINDOWS`] windows, on a
//! window boundary; the one before is kept through the first window under the new one, so a
//! rotation never breaks a handshake in flight, and then forgotten. Secrets are never written or
//! logged.
//!
//! Each source address may send [`StatusPingConfig::hellos_per_minute`] hellos and as many asks a
//! minute, and the server sends at most [`StatusPingConfig::replies_per_second`] replies a second
//! in all; the excess is dropped silently and counted, and the count is logged once a minute.
//! Nothing here touches the session table.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

use dereth_primitives::era::{EraFeatureBits, EraFeatures};
pub use dereth_transport::status_ping::WorldState;
use dereth_transport::status_ping::{self, Request, StatusReply, Token};
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// How long a window is, in seconds.
pub const WINDOW_SECONDS: u64 = 30;

/// How many windows one secret is used for: ten minutes.
pub const SECRET_WINDOWS: u64 = 20;

/// The most source addresses counted in one minute. A request from one more is dropped.
pub const MAX_TRACKED_ADDRESSES: usize = 4096;

/// `empyrean.toml` → `[status]`, as the transport uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusPingConfig {
    /// Answer the status ping. Off, it is dropped as a server without it drops it.
    pub enabled: bool,
    /// Hellos a minute from one source address.
    pub hellos_per_minute: u32,
    /// Asks a minute from one source address.
    pub asks_per_minute: u32,
    /// Replies a second, token and status together, to everyone.
    pub replies_per_second: u32,
}

impl Default for StatusPingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hellos_per_minute: 4,
            asks_per_minute: 4,
            replies_per_second: 50,
        }
    }
}

/// What the status reply says. The world sets the lasting facts once and the live ones
/// ([`StatusFacts::state`], [`StatusFacts::players`]) every pass of its loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusFacts {
    pub state: WorldState,
    pub players: u16,
    /// The era's name (`eor`, `infiltration`).
    pub era: String,
    /// The world's full set of systems.
    pub features: EraFeatures,
    pub software: String,
    pub software_version: String,
    pub world_name: String,
}

impl Default for StatusFacts {
    fn default() -> Self {
        Self {
            state: WorldState::Starting,
            players: 0,
            era: String::new(),
            features: EraFeatures::default(),
            software: String::new(),
            software_version: String::new(),
            world_name: String::new(),
        }
    }
}

/// Where fresh secrets come from: the operating system's random source in the server, a fixed
/// sequence in tests.
pub type SecretSource = Box<dyn FnMut() -> [u8; 32] + Send>;

#[derive(Default)]
struct Secrets {
    /// The secret epoch `current` belongs to; `None` before the first is drawn.
    epoch: Option<u64>,
    current: [u8; 32],
    /// The epoch before's secret, during the first window of this one.
    previous: Option<[u8; 32]>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    hellos: u32,
    asks: u32,
}

/// The status ping's state: the secrets, the rate counts and the facts it answers with.
pub struct StatusPing {
    pub config: StatusPingConfig,
    pub facts: StatusFacts,
    secrets: Secrets,
    fresh_secret: SecretSource,
    minute: u64,
    per_address: HashMap<IpAddr, Counts>,
    second: u64,
    replies_this_second: u32,
    /// Requests dropped by the limits since the last log line.
    dropped: u64,
}

impl std::fmt::Debug for StatusPing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secrets.
        f.debug_struct("StatusPing")
            .field("config", &self.config)
            .field("facts", &self.facts)
            .field("addresses", &self.per_address.len())
            .field("dropped", &self.dropped)
            .finish_non_exhaustive()
    }
}

/// What became of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The datagram to send back to the asker.
    Reply(Vec<u8>),
    /// The ping is off, or the request is malformed, or its token is not good: nothing is said.
    Ignored,
    /// Over a rate limit: nothing is said, and it is counted.
    Limited,
}

impl StatusPing {
    #[must_use]
    pub fn new(config: StatusPingConfig, facts: StatusFacts, fresh_secret: SecretSource) -> Self {
        Self {
            config,
            facts,
            secrets: Secrets::default(),
            fresh_secret,
            minute: 0,
            per_address: HashMap::new(),
            second: 0,
            replies_this_second: 0,
            dropped: 0,
        }
    }

    /// One status-ping datagram from `from`, at `seconds` on the monotonic clock.
    pub fn on_request(&mut self, from: SocketAddr, datagram: &[u8], seconds: u64) -> Outcome {
        if !self.config.enabled {
            return Outcome::Ignored;
        }
        let Some(request) = status_ping::parse_request(datagram) else {
            return Outcome::Ignored;
        };
        self.roll_counts(seconds);
        if !self.admit(from.ip(), &request) {
            self.dropped += 1;
            return Outcome::Limited;
        }
        let window = seconds / WINDOW_SECONDS;
        self.rotate(window);
        let reply = match request {
            Request::Hello => status_ping::token_reply(self.issue(from, window)),
            Request::Ask(token) => {
                if !self.verify(from, token, window) {
                    return Outcome::Ignored;
                }
                self.reply()
            }
        };
        if self.replies_this_second >= self.config.replies_per_second {
            self.dropped += 1;
            return Outcome::Limited;
        }
        self.replies_this_second += 1;
        Outcome::Reply(reply)
    }

    /// The status reply for the facts as they stand.
    #[must_use]
    pub fn reply(&self) -> Vec<u8> {
        let bits = EraFeatureBits::of(self.facts.features);
        StatusReply {
            format_version: status_ping::REPLY_FORMAT_VERSION,
            state: self.facts.state,
            players: self.facts.players,
            era: self.facts.era.clone(),
            era_table_version: bits.table_version,
            era_features: bits.bytes,
            software: self.facts.software.clone(),
            software_version: self.facts.software_version.clone(),
            world_name: self.facts.world_name.clone(),
        }
        .encode()
    }

    /// A new minute clears the per-address counts and logs the last one's drops; a new second
    /// clears the reply count.
    fn roll_counts(&mut self, seconds: u64) {
        let minute = seconds / 60;
        if minute != self.minute {
            if self.dropped > 0 {
                log::info!(
                    "Status ping: {} request(s) over the rate limits dropped",
                    self.dropped
                );
                self.dropped = 0;
            }
            self.minute = minute;
            self.per_address.clear();
        }
        if seconds != self.second {
            self.second = seconds;
            self.replies_this_second = 0;
        }
    }

    fn admit(&mut self, ip: IpAddr, request: &Request) -> bool {
        if !self.per_address.contains_key(&ip) && self.per_address.len() >= MAX_TRACKED_ADDRESSES {
            return false;
        }
        let counts = self.per_address.entry(ip).or_default();
        let (n, limit) = match request {
            Request::Hello => (&mut counts.hellos, self.config.hellos_per_minute),
            Request::Ask(_) => (&mut counts.asks, self.config.asks_per_minute),
        };
        if *n >= limit {
            return false;
        }
        *n += 1;
        true
    }

    /// Draws a new secret when `window` starts a new epoch, and forgets the old one once the
    /// first window under the new one has passed.
    fn rotate(&mut self, window: u64) {
        let epoch = window / SECRET_WINDOWS;
        if self.secrets.epoch != Some(epoch) {
            let fresh = (self.fresh_secret)();
            let previous =
                (self.secrets.epoch.map(|e| e + 1) == Some(epoch)).then_some(self.secrets.current);
            self.secrets = Secrets {
                epoch: Some(epoch),
                current: fresh,
                previous,
            };
        }
        if window > epoch * SECRET_WINDOWS {
            self.secrets.previous = None;
        }
    }

    fn secret_for(&self, window: u64) -> Option<&[u8; 32]> {
        let epoch = self.secrets.epoch?;
        let of = window / SECRET_WINDOWS;
        if of == epoch {
            Some(&self.secrets.current)
        } else if of + 1 == epoch {
            self.secrets.previous.as_ref()
        } else {
            None
        }
    }

    fn mac(secret: &[u8; 32], from: SocketAddr, window: u32) -> [u8; status_ping::TOKEN_MAC_LEN] {
        let mut m = <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("any key length");
        match from.ip() {
            IpAddr::V4(a) => {
                m.update(&[4]);
                m.update(&a.octets());
            }
            IpAddr::V6(a) => {
                m.update(&[6]);
                m.update(&a.octets());
            }
        }
        m.update(&from.port().to_le_bytes());
        m.update(&window.to_le_bytes());
        let full = m.finalize().into_bytes();
        let mut out = [0u8; status_ping::TOKEN_MAC_LEN];
        out.copy_from_slice(&full[..status_ping::TOKEN_MAC_LEN]);
        out
    }

    fn issue(&self, from: SocketAddr, window: u64) -> Token {
        let window = u32::try_from(window).unwrap_or(u32::MAX);
        let secret = self.secret_for(u64::from(window)).expect("drawn by rotate");
        Token {
            window,
            mac: Self::mac(secret, from, window),
        }
    }

    /// Whether `token` was issued to `from` in this window or the one before, under a secret
    /// still held.
    fn verify(&self, from: SocketAddr, token: Token, window: u64) -> bool {
        let t = u64::from(token.window);
        if t != window && t + 1 != window {
            return false;
        }
        let Some(secret) = self.secret_for(t) else {
            return false;
        };
        // Constant time: the code is compared whole, never byte by byte.
        let expected = Self::mac(secret, from, token.window);
        expected
            .iter()
            .zip(token.mac)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counting_secrets() -> SecretSource {
        let mut n = 0u8;
        Box::new(move || {
            n += 1;
            [n; 32]
        })
    }

    fn ping(config: StatusPingConfig) -> StatusPing {
        StatusPing::new(
            config,
            StatusFacts {
                state: WorldState::Open,
                players: 3,
                era: "infiltration".into(),
                features: EraFeatures::INFILTRATION,
                software: "Empyrean".into(),
                software_version: "0.2.0".into(),
                world_name: "Test".into(),
            },
            counting_secrets(),
        )
    }

    fn unlimited() -> StatusPingConfig {
        StatusPingConfig {
            hellos_per_minute: u32::MAX,
            asks_per_minute: u32::MAX,
            replies_per_second: u32::MAX,
            ..StatusPingConfig::default()
        }
    }

    fn addr(last: u8, port: u16) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, last], port))
    }

    fn token(p: &mut StatusPing, from: SocketAddr, at: u64) -> Token {
        match p.on_request(from, &status_ping::hello(), at) {
            Outcome::Reply(r) => status_ping::parse_token_reply(&r).expect("a token"),
            other => panic!("no token: {other:?}"),
        }
    }

    fn asked(p: &mut StatusPing, from: SocketAddr, t: Token, at: u64) -> Option<StatusReply> {
        match p.on_request(from, &status_ping::ask(t), at) {
            Outcome::Reply(r) => Some(StatusReply::parse(&r).expect("a status reply")),
            _ => None,
        }
    }

    #[test]
    fn a_token_is_issued_and_answered_with_the_status() {
        let mut p = ping(unlimited());
        let a = addr(1, 5000);
        let t = token(&mut p, a, 1000);
        let s = asked(&mut p, a, t, 1001).expect("answered");
        assert_eq!(s.state, WorldState::Open);
        assert_eq!(s.players, 3);
        assert_eq!(s.era, "infiltration");
        assert_eq!(s.software, "Empyrean");
        assert_eq!(s.software_version, "0.2.0");
        assert_eq!(s.world_name, "Test");
        assert_eq!(s.era_table_version, EraFeatures::TABLE_VERSION);
        let bits = EraFeatureBits {
            table_version: s.era_table_version,
            bytes: s.era_features,
        };
        assert_eq!(
            bits.overrides().apply(EraFeatures::ALL),
            EraFeatures::INFILTRATION
        );
    }

    #[test]
    fn a_token_lasts_its_window_and_the_next_and_no_longer() {
        let mut p = ping(unlimited());
        let a = addr(1, 5000);
        // Issued at the start of window 40 (1200 s).
        let t = token(&mut p, a, 1200);
        assert!(asked(&mut p, a, t, 1229).is_some(), "its own window");
        assert!(asked(&mut p, a, t, 1259).is_some(), "the next");
        assert!(asked(&mut p, a, t, 1260).is_none(), "two windows on");
        // A token from the future is no better.
        let future = Token {
            window: t.window + 5,
            ..t
        };
        assert!(asked(&mut p, a, future, 1205).is_none());
    }

    #[test]
    fn a_token_from_the_secret_before_a_rotation_holds_for_one_window() {
        let mut p = ping(unlimited());
        let a = addr(1, 5000);
        // Window 39 is the last of epoch 1 (windows 20 to 39); window 40 starts epoch 2.
        let old = token(&mut p, a, 39 * WINDOW_SECONDS + 29);
        assert!(
            asked(&mut p, a, old, 40 * WINDOW_SECONDS).is_some(),
            "the first window under the new secret still knows the old one"
        );
        let new = token(&mut p, a, 40 * WINDOW_SECONDS + 1);
        assert_ne!(old.mac, new.mac);
        assert!(asked(&mut p, a, new, 41 * WINDOW_SECONDS).is_some());
        assert!(
            asked(&mut p, a, old, 41 * WINDOW_SECONDS).is_none(),
            "and then forgets it"
        );
        assert!(p.secrets.previous.is_none());
    }

    #[test]
    fn a_token_is_good_only_from_the_address_and_port_it_was_issued_to() {
        let mut p = ping(unlimited());
        let t = token(&mut p, addr(1, 5000), 100);
        assert!(
            asked(&mut p, addr(2, 5000), t, 101).is_none(),
            "another address"
        );
        assert!(
            asked(&mut p, addr(1, 5001), t, 101).is_none(),
            "another port"
        );
        let v6 = SocketAddr::from(([0, 0, 0, 0, 0, 0xffff, 0x0a00, 1], 5000));
        assert!(
            asked(&mut p, v6, t, 101).is_none(),
            "the same address mapped into IPv6"
        );
        assert!(asked(&mut p, addr(1, 5000), t, 101).is_some());
    }

    #[test]
    fn a_tampered_token_is_refused() {
        let mut p = ping(unlimited());
        let a = addr(1, 5000);
        let t = token(&mut p, a, 100);
        for i in 0..status_ping::TOKEN_MAC_LEN {
            let mut bad = t;
            bad.mac[i] ^= 1;
            assert!(asked(&mut p, a, bad, 101).is_none(), "byte {i}");
        }
        let moved = Token {
            window: t.window - 1,
            ..t
        };
        assert!(asked(&mut p, a, moved, 101).is_none(), "another window");
        assert!(asked(&mut p, a, Token::default(), 101).is_none(), "none");
    }

    #[test]
    fn the_answer_to_a_hello_is_no_larger_than_the_hello() {
        let mut p = ping(unlimited());
        let Outcome::Reply(r) = p.on_request(addr(1, 5000), &status_ping::hello(), 5) else {
            panic!("answered");
        };
        assert!(r.len() <= status_ping::hello().len());
        // A hello cut short is not answered at all.
        let short = &status_ping::hello()[..status_ping::TOKEN_REPLY_LEN];
        assert_eq!(p.on_request(addr(1, 5000), short, 5), Outcome::Ignored);
    }

    #[test]
    fn the_per_address_and_global_limits_drop_the_excess_and_count_it() {
        let mut p = ping(StatusPingConfig {
            hellos_per_minute: 4,
            asks_per_minute: 4,
            replies_per_second: 3,
            ..StatusPingConfig::default()
        });
        let a = addr(1, 5000);
        let mut answered = 0;
        for i in 0..6 {
            // A second apart, so only the per-address limit applies.
            if matches!(
                p.on_request(a, &status_ping::hello(), 60 + i),
                Outcome::Reply(_)
            ) {
                answered += 1;
            }
        }
        assert_eq!(answered, 4, "four hellos a minute");
        assert_eq!(p.dropped, 2);
        // The next minute starts afresh, and asks have their own count.
        let t = token(&mut p, a, 120);
        for i in 0..4 {
            assert!(asked(&mut p, a, t, 121 + i).is_some(), "ask {i}");
        }
        assert!(asked(&mut p, a, t, 125).is_none(), "the fifth ask");
        // Everyone together: three replies a second.
        let outcomes: Vec<Outcome> = (0..5)
            .map(|i| p.on_request(addr(100 + i, 5000), &status_ping::hello(), 130))
            .collect();
        assert_eq!(
            outcomes
                .iter()
                .filter(|o| matches!(o, Outcome::Reply(_)))
                .count(),
            3
        );
        assert_eq!(
            outcomes.iter().filter(|o| **o == Outcome::Limited).count(),
            2
        );
    }

    #[test]
    fn the_switch_off_answers_nothing() {
        let mut on = ping(unlimited());
        let a = addr(1, 5000);
        let t = token(&mut on, a, 10);
        let mut p = ping(StatusPingConfig {
            enabled: false,
            ..unlimited()
        });
        assert_eq!(p.on_request(a, &status_ping::hello(), 10), Outcome::Ignored);
        assert_eq!(p.on_request(a, &status_ping::ask(t), 10), Outcome::Ignored);
        assert_eq!(p.dropped, 0);
    }

    #[test]
    fn the_secrets_never_show_in_the_debug_form() {
        let mut p = ping(unlimited());
        let _ = token(&mut p, addr(1, 5000), 10);
        let text = format!("{p:?}");
        assert!(
            !text.contains("current") && !text.contains("secret"),
            "{text}"
        );
    }
}
