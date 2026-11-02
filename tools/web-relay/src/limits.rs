//! What a relay beside a server allows one player, and all of them: sessions at once, counted by
//! client address and in total; handshakes waiting at once, and how long one may take; how many
//! datagrams and bytes a session may send a second; and how long a session may say nothing.
//!
//! Sessions and handshakes are counted, never timed. The rest reads a [`Clock`] the relay is
//! given, so that tests turn it by hand.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Time since some fixed start, never going back.
pub type Clock = Arc<dyn Fn() -> Duration + Send + Sync>;

/// The machine's monotonic clock.
#[must_use]
pub fn system_clock() -> Clock {
    let start = Instant::now();
    Arc::new(move || start.elapsed())
}

/// A rate and the burst above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate {
    pub per_second: u32,
    pub burst: u32,
}

/// What one session may send the server: datagrams and their bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendRate {
    pub datagrams: Rate,
    pub bytes: Rate,
}

/// The limits a relay keeps. `None` is unlimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Sessions open at once from one client address (an IPv6 one by its /64).
    pub per_ip: Option<u32>,
    /// Sessions open at once in all.
    pub sessions: Option<u32>,
    /// Connections in their TLS and WebSocket handshakes at once from one peer that is not a
    /// trusted proxy.
    pub handshakes_per_peer: Option<u32>,
    /// Connections in their handshakes at once in all.
    pub handshakes: Option<u32>,
    /// How long the TLS and WebSocket handshakes may take, together.
    pub handshake_time: Duration,
    /// How long a session may send nothing before it is closed (its will sent).
    pub idle: Option<Duration>,
    /// What one session may send the server.
    pub send: Option<SendRate>,
}

impl Limits {
    /// A relay on the player's machine, for that player: no limit but a handshake's time.
    pub const LOCAL: Self = Self {
        per_ip: None,
        sessions: None,
        handshakes_per_peer: None,
        handshakes: None,
        handshake_time: Duration::from_secs(10),
        idle: None,
        send: None,
    };

    /// A relay beside a server, before the operator changes the two counts it may.
    pub const PUBLIC: Self = Self {
        per_ip: Some(8),
        sessions: Some(100),
        handshakes_per_peer: Some(8),
        handshakes: Some(64),
        handshake_time: Duration::from_secs(10),
        // A game client sends at least every few seconds while it is connected; the servers
        // themselves drop a session silent for a minute.
        idle: Some(Duration::from_secs(60)),
        // A game client sends a few dozen small datagrams a second at most; this leaves it room,
        // and keeps a page from flooding the server from the relay's address.
        send: Some(SendRate {
            datagrams: Rate {
                per_second: 200,
                burst: 400,
            },
            bytes: Rate {
                per_second: 128 * 1024,
                burst: 256 * 1024,
            },
        }),
    };
}

/// Why a session or a handshake was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The address already has this many.
    PerAddress(u32),
    /// The relay already has this many.
    Total(u32),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PerAddress(n) => write!(f, "{n} already open from that address"),
            Self::Total(n) => write!(f, "{n} already open in all"),
        }
    }
}

/// A count of things open at once, by address and in total.
#[derive(Debug)]
pub struct Admission {
    per_address: Option<u32>,
    total: Option<u32>,
    open: Mutex<(HashMap<IpAddr, u32>, u32)>,
}

impl Admission {
    #[must_use]
    pub fn new(per_address: Option<u32>, total: Option<u32>) -> Arc<Self> {
        Arc::new(Self {
            per_address,
            total,
            open: Mutex::new((HashMap::new(), 0)),
        })
    }

    /// One more open, counted under `address` (`None`: in the total alone), or why not. It stays
    /// counted until the ticket is dropped.
    ///
    /// # Errors
    /// The address, or the relay, is at its limit.
    pub fn admit(self: &Arc<Self>, address: Option<IpAddr>) -> Result<Ticket, Refusal> {
        let mut guard = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let (by_address, total) = &mut *guard;
        if let Some(max) = self.total.filter(|max| *total >= *max) {
            return Err(Refusal::Total(max));
        }
        if let Some(a) = address {
            let n = by_address.get(&a).copied().unwrap_or(0);
            if let Some(max) = self.per_address.filter(|max| n >= *max) {
                return Err(Refusal::PerAddress(max));
            }
            by_address.insert(a, n + 1);
        }
        *total += 1;
        Ok(Ticket {
            admission: Arc::clone(self),
            address,
        })
    }

    /// How many are open in all.
    #[must_use]
    pub fn open(&self) -> u32 {
        self.open.lock().unwrap_or_else(PoisonError::into_inner).1
    }
}

/// One admitted; counted until dropped.
#[derive(Debug)]
pub struct Ticket {
    admission: Arc<Admission>,
    address: Option<IpAddr>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let mut guard = self
            .admission
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (by_address, total) = &mut *guard;
        *total = total.saturating_sub(1);
        if let Some(a) = self.address {
            if let Some(n) = by_address.get_mut(&a) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    by_address.remove(&a);
                }
            }
        }
    }
}

/// A token bucket on the relay's clock: full to its burst at the start, refilled at its rate.
#[derive(Debug, Clone)]
pub struct Bucket {
    rate: Rate,
    tokens: f64,
    at: Duration,
}

impl Bucket {
    #[must_use]
    pub fn new(rate: Rate, now: Duration) -> Self {
        Self {
            rate,
            tokens: f64::from(rate.burst),
            at: now,
        }
    }

    /// Takes `n` tokens at `now`, if there are that many.
    pub fn take(&mut self, n: u32, now: Duration) -> bool {
        let passed = now.saturating_sub(self.at).as_secs_f64();
        self.at = now.max(self.at);
        self.tokens = (self.tokens + passed * f64::from(self.rate.per_second))
            .min(f64::from(self.rate.burst));
        if self.tokens >= f64::from(n) {
            self.tokens -= f64::from(n);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> Option<IpAddr> {
        Some(s.parse().expect("an address"))
    }

    /// The per-address and total counts refuse at their limit, and give a place back when a
    /// ticket is dropped.
    #[test]
    fn sessions_are_refused_at_the_per_address_and_total_limits() {
        let a = Admission::new(Some(2), Some(3));
        let one = a.admit(ip("192.0.2.1")).expect("first");
        let _two = a.admit(ip("192.0.2.1")).expect("second");
        assert_eq!(
            a.admit(ip("192.0.2.1")).map(|_| ()),
            Err(Refusal::PerAddress(2))
        );
        let _three = a.admit(ip("192.0.2.2")).expect("another address");
        assert_eq!(a.admit(ip("192.0.2.3")).map(|_| ()), Err(Refusal::Total(3)));
        assert_eq!(a.open(), 3);
        drop(one);
        assert_eq!(a.open(), 2);
        assert!(a.admit(ip("192.0.2.1")).is_ok(), "a place given back");
        let unlimited = Admission::new(None, None);
        let held: Vec<_> = (0..1000)
            .map(|_| unlimited.admit(ip("192.0.2.1")).expect("unlimited"))
            .collect();
        assert_eq!(unlimited.open(), 1000);
        drop(held);
        assert_eq!(unlimited.open(), 0);
    }

    /// The bucket gives its burst at once, then its rate, by the clock it is handed.
    #[test]
    fn a_bucket_gives_its_burst_then_its_rate() {
        let rate = Rate {
            per_second: 10,
            burst: 20,
        };
        let mut b = Bucket::new(rate, Duration::ZERO);
        let at_once = (0..100).take_while(|_| b.take(1, Duration::ZERO)).count();
        assert_eq!(at_once, 20);
        assert!(!b.take(1, Duration::from_millis(50)), "half a token");
        assert!(b.take(1, Duration::from_millis(100)));
        let later = (0..100)
            .take_while(|_| b.take(1, Duration::from_secs(60)))
            .count();
        assert_eq!(later, 20, "never above the burst");
        assert!(!b.take(21, Duration::from_secs(120)), "more than the burst");
    }
}
