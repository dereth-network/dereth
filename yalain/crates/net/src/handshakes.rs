//! The logins and world logins the server has answered and whose clients have not answered back.
//!
//! A login's source address means nothing until its client echoes the cookie it was sent there, so
//! one waiting holds no connection and takes no place from one: it is kept here, a bounded number of
//! them per service and per address, the oldest giving way. It is answered once, and again only
//! when the client asks again, at a pace: the client resends its login every two seconds until it is
//! answered, and its answer every third of a second until the server's first packet. Of 1,650
//! connections recorded in 2017, 1,648 were sent one handshake and none more than two.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, SocketAddr};

use dereth_primitives::LocalTime;
use dereth_transport::conn::ConnectSeeds;

use crate::login::GameLogin;
use crate::server::{ConnId, Service};

/// The most waiting on one service. Past it the oldest gives way, so a client that answers at once
/// still gets in however many are waiting.
pub const MOST_WAITING: usize = 1024;
/// The most waiting from one address (an IPv6 address's /64). Past it that address's oldest gives
/// way, so one address can't push out the others.
pub const MOST_WAITING_PER_ADDRESS: usize = 16;
/// How long one waits for its client to answer, in seconds. The longest recorded took under 2.
pub const WAITING_SECONDS: f64 = 10.0;
/// How soon one is answered again, in seconds: a client asks again each third of a second at most.
pub const ANSWERED_AGAIN_AFTER: f64 = 0.25;

/// A login answered, waiting for its client to answer back.
#[derive(Debug)]
pub struct Waiting {
    pub service: Service,
    pub peer: SocketAddr,
    pub client_id: u16,
    pub cookie: u64,
    pub seeds: ConnectSeeds,
    /// The handshake it was sent, sent again if it asks again.
    pub packet: Vec<u8>,
    pub login: Option<GameLogin>,
    /// World logins: the login connection whose referral led here, and its cookie.
    pub via: Option<(ConnId, u64)>,
    at: LocalTime,
    answered_at: LocalTime,
}

impl Waiting {
    /// One answered at `at`.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        service: Service,
        peer: SocketAddr,
        client_id: u16,
        cookie: u64,
        seeds: ConnectSeeds,
        packet: Vec<u8>,
        login: Option<GameLogin>,
        via: Option<(ConnId, u64)>,
        at: LocalTime,
    ) -> Self {
        Self {
            service,
            peer,
            client_id,
            cookie,
            seeds,
            packet,
            login,
            via,
            at,
            answered_at: at,
        }
    }
}

/// Whose an address's waiting count is: an IPv4 address (one mapped into IPv6 included), or an
/// IPv6 address's /64.
fn whose(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v) => {
            let s = v.segments();
            IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

/// Everything waiting.
#[derive(Debug, Default)]
pub struct Handshakes {
    next: u64,
    /// Oldest first.
    list: BTreeMap<u64, Waiting>,
    by_peer: HashMap<(Service, SocketAddr), u64>,
    by_cookie: HashMap<u64, u64>,
    /// By service, and by service and address.
    of_service: HashMap<Service, usize>,
    of_address: HashMap<(Service, IpAddr), usize>,
}

impl Handshakes {
    /// How many are waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Is nothing waiting?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Is this client waiting?
    #[must_use]
    pub fn is_waiting(&self, service: Service, peer: SocketAddr) -> bool {
        self.by_peer.contains_key(&(service, peer))
    }

    /// What to send a client that has asked again, if it is waiting and it is time.
    pub fn answer_again(
        &mut self,
        service: Service,
        peer: SocketAddr,
        now: LocalTime,
    ) -> Option<&[u8]> {
        let n = self.by_peer.get(&(service, peer))?;
        let w = self.list.get_mut(n)?;
        if now.seconds_since(w.answered_at) < ANSWERED_AGAIN_AFTER {
            return None;
        }
        w.answered_at = now;
        Some(w.packet.as_slice())
    }

    fn forget(&mut self, n: u64) -> Option<Waiting> {
        let w = self.list.remove(&n)?;
        self.by_peer.remove(&(w.service, w.peer));
        self.by_cookie.remove(&w.cookie);
        if let Some(c) = self.of_service.get_mut(&w.service) {
            *c = c.saturating_sub(1);
        }
        let key = (w.service, whose(w.peer.ip()));
        match self.of_address.get_mut(&key) {
            Some(c) if *c > 1 => *c -= 1,
            _ => {
                self.of_address.remove(&key);
            }
        }
        Some(w)
    }

    /// Make room for one more from this address, then keep it.
    pub fn insert(&mut self, w: Waiting) {
        let (service, address) = (w.service, whose(w.peer.ip()));
        let from_it = self
            .of_address
            .get(&(service, address))
            .copied()
            .unwrap_or(0);
        let oldest = if from_it >= MOST_WAITING_PER_ADDRESS {
            self.list
                .iter()
                .find(|(_, x)| x.service == service && whose(x.peer.ip()) == address)
                .map(|(n, _)| *n)
        } else if self.of_service.get(&service).copied().unwrap_or(0) >= MOST_WAITING {
            self.list
                .iter()
                .find(|(_, x)| x.service == service)
                .map(|(n, _)| *n)
        } else {
            None
        };
        if let Some(n) = oldest {
            self.forget(n);
        }
        let n = self.next;
        self.next += 1;
        self.by_peer.insert((service, w.peer), n);
        self.by_cookie.insert(w.cookie, n);
        *self.of_service.entry(service).or_default() += 1;
        *self.of_address.entry((service, address)).or_default() += 1;
        self.list.insert(n, w);
    }

    /// The one a client has answered with its cookie, from the address it was sent to.
    pub fn answered(&mut self, service: Service, from: IpAddr, cookie: u64) -> Option<Waiting> {
        let n = *self.by_cookie.get(&cookie)?;
        let w = self.list.get(&n)?;
        (w.service == service && w.peer.ip() == from)
            .then(|| self.forget(n))
            .flatten()
    }

    /// Forget those that waited too long.
    pub fn expire(&mut self, now: LocalTime) {
        while let Some((&n, w)) = self.list.first_key_value() {
            if now.seconds_since(w.at) < WAITING_SECONDS {
                break;
            }
            self.forget(n);
        }
    }
}
