//! Verification in game, the hub's half: the challenges it sends a world's game port, and the
//! answers it waits for (`docs/network/05-hub.md` section 2.5).
//!
//! A verification call that has passed its key, token and attempts checks has the hub send each of
//! the endpoint's addresses (at most [`MAX_ADDRESSES`]) a challenge at the game port, each address
//! a fresh random nonce of its own, [`COPIES`] times, [`EVERY`] apart, since UDP may lose any one.
//! The call then waits up to [`WAIT`] from the first for the world to post a nonce back over HTTPS
//! with the same key, from any address. The address whose nonce comes back is the one the world
//! answered at.
//!
//! So one attempt sends at most [`COPIES`] × [`MAX_ADDRESSES`] datagrams, each smaller than the
//! request that caused it, and only after an authenticated call within the world's attempts; at
//! most one challenge waits for a world at a time, and at most [`MAX_WAITING`] across the hub.
//! Nothing here is written anywhere: a nonce lives only while its challenge waits.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use dereth_network_protocol::challenge::{Challenge, NONCE_LEN};

/// How many times each address is sent its challenge.
pub const COPIES: usize = 3;
/// The time between two copies.
pub const EVERY: Duration = Duration::from_secs(2);
/// How long a verification call waits for the world's answer, from the first challenge.
pub const WAIT: Duration = Duration::from_secs(15);
/// The most addresses one attempt challenges: the first of the endpoint's that the hub may reach.
pub const MAX_ADDRESSES: usize = 4;
/// How long a verification call waits for its endpoint's host name to be looked up: longer than
/// any other lookup, since verification is rare and a name server may be slow.
pub const LOOKUP_DEADLINE: Duration = Duration::from_secs(10);
/// The most challenges waiting at once across the hub.
pub const MAX_WAITING: usize = 256;

/// One address challenged, and its nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub to: SocketAddr,
    pub nonce: [u8; NONCE_LEN],
}

impl Target {
    /// A target at `ip` and `port`, with a fresh nonce.
    #[must_use]
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self {
            to: SocketAddr::new(ip, port),
            nonce: crate::registry::random_bytes::<NONCE_LEN>(),
        }
    }
}

#[derive(Debug)]
struct Pending {
    /// The digest of the key the verification call came with; an answer must carry the same key.
    key: [u8; 32],
    targets: Vec<Target>,
    /// The target whose nonce came back.
    answered: Option<SocketAddr>,
}

/// Why a challenge could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// One already waits for this world.
    Busy,
    /// [`MAX_WAITING`] already wait.
    Full,
}

/// The challenges waiting for an answer, by world.
#[derive(Debug, Default)]
pub struct Waiting {
    pending: Mutex<HashMap<String, Pending>>,
    answered: Condvar,
}

impl Waiting {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether a challenge waits for `slug`.
    #[must_use]
    pub fn waits_for(&self, slug: &str) -> bool {
        self.lock().contains_key(slug)
    }

    /// Opens `slug`'s challenge of `targets`, for a call that came with the key whose digest is
    /// `key`. It waits until the ticket is dropped.
    ///
    /// # Errors
    /// One already waits for the world, or too many wait across the hub.
    pub fn open(
        &self,
        slug: &str,
        key: [u8; 32],
        targets: Vec<Target>,
    ) -> Result<Ticket<'_>, Refused> {
        let mut p = self.lock();
        if p.contains_key(slug) {
            return Err(Refused::Busy);
        }
        if p.len() >= MAX_WAITING {
            return Err(Refused::Full);
        }
        p.insert(
            slug.to_owned(),
            Pending {
                key,
                targets,
                answered: None,
            },
        );
        Ok(Ticket {
            waiting: self,
            slug: slug.to_owned(),
        })
    }

    /// The world's answer: `nonce`, posted with the key whose digest is `key`. True when it is the
    /// nonce of one of the addresses `slug`'s waiting challenge was sent to, and the key is the one
    /// its verification call came with; that address is then the one the world answered at.
    #[must_use]
    pub fn answer(&self, slug: &str, key: &[u8; 32], nonce: &[u8; NONCE_LEN]) -> bool {
        let mut p = self.lock();
        let Some(pending) = p.get_mut(slug) else {
            return false;
        };
        let same_key = crate::registry::same_digest(&pending.key, key);
        let mut hit = None;
        for t in &pending.targets {
            let same = t
                .nonce
                .iter()
                .zip(nonce)
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0;
            if same {
                hit = Some(t.to);
            }
        }
        match hit {
            Some(to) if same_key => {
                pending.answered.get_or_insert(to);
                drop(p);
                self.answered.notify_all();
                true
            }
            _ => false,
        }
    }
}

/// A challenge that waits, until dropped.
#[derive(Debug)]
pub struct Ticket<'a> {
    waiting: &'a Waiting,
    slug: String,
}

impl Ticket<'_> {
    /// The address the world answered at, waiting for it until `until`; none if it has not
    /// answered by then.
    #[must_use]
    pub fn answered_by(&self, until: Instant) -> Option<SocketAddr> {
        let mut p = self.waiting.lock();
        loop {
            let answered = p.get(&self.slug).and_then(|x| x.answered);
            if answered.is_some() {
                return answered;
            }
            let now = Instant::now();
            if now >= until {
                return None;
            }
            p = self
                .waiting
                .answered
                .wait_timeout(p, until - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.waiting.lock().remove(&self.slug);
    }
}

/// The sockets one attempt sends from: one for each address family it challenges, bound to
/// loopback when every address of that family is a loopback one (a hub developed on one machine),
/// and otherwise to the family's unspecified address on a port the system picks.
#[derive(Debug)]
pub struct Sender {
    v4: Option<UdpSocket>,
    v6: Option<UdpSocket>,
}

impl Sender {
    /// The sockets for `targets`; a family whose socket cannot be opened sends nothing.
    #[must_use]
    pub fn for_targets(targets: &[Target]) -> Self {
        let bind = |v6: bool| {
            let mine: Vec<&Target> = targets.iter().filter(|t| t.to.is_ipv6() == v6).collect();
            if mine.is_empty() {
                return None;
            }
            let loopback = mine.iter().all(|t| t.to.ip().is_loopback());
            let ip = match (v6, loopback) {
                (false, true) => IpAddr::V4(Ipv4Addr::LOCALHOST),
                (false, false) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                (true, true) => IpAddr::V6(Ipv6Addr::LOCALHOST),
                (true, false) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            };
            UdpSocket::bind(SocketAddr::new(ip, 0)).ok()
        };
        Self {
            v4: bind(false),
            v6: bind(true),
        }
    }

    /// Sends each target its challenge for `slug` once; how many went. A send that fails is not
    /// retried here: the next copy goes anyway.
    pub fn send_round(&self, slug: &str, targets: &[Target]) -> usize {
        let mut sent = 0;
        for t in targets {
            let socket = if t.to.is_ipv6() { &self.v6 } else { &self.v4 };
            let datagram = Challenge {
                slug: slug.to_owned(),
                nonce: t.nonce,
            }
            .encode();
            if let (Some(s), Some(d)) = (socket, datagram) {
                if s.send_to(&d, t.to).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }
}

/// `ip` and `port` as a message names them: `[v6]:port` for IPv6.
#[must_use]
pub fn shown_at(ip: IpAddr, port: u16) -> String {
    SocketAddr::new(ip, port).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> [u8; 32] {
        [b; 32]
    }

    /// Spec: hub: a challenge is answered only by one of the nonces it sent, with the key its call
    /// came with; the answer names the address whose nonce came back.
    #[test]
    fn only_a_sent_nonce_with_the_calls_own_key_answers_a_challenge() {
        let w = Waiting::default();
        let a = Target::new("192.0.2.1".parse().expect("an address"), 9000);
        let b = Target::new("2001:db8::1".parse().expect("an address"), 9000);
        assert_ne!(a.nonce, b.nonce, "each address has its own nonce");
        let ticket = w
            .open("coldeve", key(1), vec![a.clone(), b.clone()])
            .expect("opened");
        assert_eq!(
            w.open("coldeve", key(1), Vec::new()).err(),
            Some(Refused::Busy),
            "one at a time for a world"
        );
        assert!(!w.answer("coldeve", &key(1), &[0; NONCE_LEN]), "a nonce never sent");
        assert!(!w.answer("coldeve", &key(2), &b.nonce), "another key");
        assert!(!w.answer("frost", &key(1), &b.nonce), "another world");
        assert_eq!(
            ticket.answered_by(Instant::now() + Duration::from_millis(20)),
            None
        );
        assert!(w.answer("coldeve", &key(1), &b.nonce));
        assert_eq!(ticket.answered_by(Instant::now()), Some(b.to));
        drop(ticket);
        assert!(!w.waits_for("coldeve"), "gone once its call is over");
        assert!(!w.answer("coldeve", &key(1), &a.nonce), "an old nonce");
    }

    /// Spec: hub: a waiting call wakes as soon as the world answers.
    #[test]
    fn a_waiting_call_wakes_when_the_answer_comes() {
        let w = std::sync::Arc::new(Waiting::default());
        let t = Target::new("192.0.2.7".parse().expect("an address"), 9000);
        let nonce = t.nonce;
        let w2 = std::sync::Arc::clone(&w);
        let started = Instant::now();
        let answered = std::thread::scope(|s| {
            let ticket = w.open("aa", key(3), vec![t.clone()]).expect("opened");
            s.spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                assert!(w2.answer("aa", &key(3), &nonce));
            });
            ticket.answered_by(Instant::now() + Duration::from_secs(5))
        });
        assert_eq!(answered, Some(t.to));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// Spec: hub: at most so many challenges wait across the hub.
    #[test]
    fn no_more_challenges_wait_than_the_hub_allows() {
        let w = Waiting::default();
        let tickets: Vec<Ticket<'_>> = (0..MAX_WAITING)
            .map(|i| w.open(&format!("w{i}"), key(0), Vec::new()).expect("opened"))
            .collect();
        assert_eq!(
            w.open("one-more", key(0), Vec::new()).err(),
            Some(Refused::Full)
        );
        drop(tickets);
        assert!(w.open("one-more", key(0), Vec::new()).is_ok());
    }
}
