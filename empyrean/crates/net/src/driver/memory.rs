//! `MemoryNet`: the in-process driver.
//!
//! No sockets and no wall clock. Datagrams travel through two [`Link`]s (client to server and
//! server to client) that can lose, reorder and duplicate them, reproducibly from a seed. Time is
//! a [`ClockSnapshot`] the test advances; the links schedule on its `monotonic` time. The lossy link's shape is salvaged from v1's `LossyLink`
//! (`empyrean/crates/net/src/testing.rs` at `server-v1-final`): hold a datagram for a few steps to reorder
//! it, deliver it twice to duplicate it.

use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;

use crate::session_connection_data::SessionRandom;
use crate::{ClockSnapshotExt, PortKind, ServerNet};

/// How a [`Link`] mistreats traffic. Probabilities are per datagram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkModel {
    pub loss: f64,
    pub reorder: f64,
    pub duplicate: f64,
    pub seed: u64,
    /// One-way delay on the virtual clock.
    pub latency: Duration,
}

impl LinkModel {
    /// Delivers everything, once, in order.
    #[must_use]
    pub const fn perfect() -> Self {
        Self {
            loss: 0.0,
            reorder: 0.0,
            duplicate: 0.0,
            seed: 0,
            latency: Duration::ZERO,
        }
    }

    /// The same probability of loss, reordering and duplication.
    #[must_use]
    pub const fn lossy(p: f64, seed: u64) -> Self {
        Self {
            loss: p,
            reorder: p,
            duplicate: p,
            seed,
            latency: Duration::ZERO,
        }
    }
}

/// What a [`Link`] has done, for a test to assert on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LinkStats {
    pub offered: u64,
    pub dropped: u64,
    pub reordered: u64,
    pub duplicated: u64,
    pub delivered: u64,
}

/// One datagram in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    pub from: SocketAddr,
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}

/// A one-way link that loses, reorders and duplicates datagrams, reproducibly.
#[derive(Debug)]
pub struct Link {
    model: LinkModel,
    rng: SessionRandom,
    /// Each held datagram with the step and the time it is due at.
    held: VecDeque<(u64, Duration, Datagram)>,
    step: u64,
    pub stats: LinkStats,
}

impl Link {
    #[must_use]
    pub fn new(model: LinkModel) -> Self {
        Self {
            model,
            rng: SessionRandom::new(model.seed),
            held: VecDeque::new(),
            step: 0,
            stats: LinkStats::default(),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f64 {
        (self.rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Offers a datagram at time `now`; it is dropped, or held until a later [`Link::step`] (0 to 3
    /// steps later, and no earlier than `now + latency`), possibly twice.
    pub fn send(&mut self, datagram: Datagram, now: Duration) {
        self.stats.offered += 1;
        if self.unit() < self.model.loss {
            self.stats.dropped += 1;
            return;
        }
        let copies = if self.unit() < self.model.duplicate {
            self.stats.duplicated += 1;
            2
        } else {
            1
        };
        let delay = if self.unit() < self.model.reorder {
            self.stats.reordered += 1;
            1 + self.rng.next_u64() % 3
        } else {
            0
        };
        let due_time = now + self.model.latency;
        for _ in 0..copies {
            self.held
                .push_back((self.step + delay, due_time, datagram.clone()));
        }
    }

    /// Advances one step and releases what is due at `now`, in the order it was offered.
    pub fn step(&mut self, now: Duration) -> Vec<Datagram> {
        let mut out = Vec::new();
        let mut keep = VecDeque::new();
        while let Some((due, due_time, d)) = self.held.pop_front() {
            if due <= self.step && due_time <= now {
                out.push(d);
            } else {
                keep.push_back((due, due_time, d));
            }
        }
        self.held = keep;
        self.step += 1;
        self.stats.delivered += out.len() as u64;
        out
    }

    /// Whether anything is still in flight.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Whether something in flight is due by `now` (possibly after a few more steps).
    #[must_use]
    pub fn has_due(&self, now: Duration) -> bool {
        self.held.iter().any(|(_, t, _)| *t <= now)
    }
}

/// An in-process network with one [`ServerNet`] at `server_ip:port` (and `port + 1`) and any
/// number of client addresses.
#[derive(Debug)]
pub struct MemoryNet {
    pub server: ServerNet,
    pub now: ClockSnapshot,
    server_ip: IpAddr,
    port: u16,
    to_server: Link,
    to_clients: Link,
    inboxes: BTreeMap<SocketAddr, VecDeque<Datagram>>,
    /// Datagrams addressed to nobody (a closed port or an unknown host), counted and dropped.
    pub undeliverable: u64,
}

impl MemoryNet {
    /// A perfect network.
    #[must_use]
    pub fn new(server: ServerNet, server_ip: IpAddr) -> Self {
        Self::with_links(
            server,
            server_ip,
            LinkModel::perfect(),
            LinkModel::perfect(),
        )
    }

    /// A network whose two directions follow the given models.
    #[must_use]
    pub fn with_links(
        server: ServerNet,
        server_ip: IpAddr,
        to_server: LinkModel,
        to_clients: LinkModel,
    ) -> Self {
        let port = server.config.port;
        Self {
            server,
            now: ClockSnapshot::at_seconds(0.0),
            server_ip,
            port,
            to_server: Link::new(to_server),
            to_clients: Link::new(to_clients),
            inboxes: BTreeMap::new(),
            undeliverable: 0,
        }
    }

    /// The server's address on one of its two ports.
    #[must_use]
    pub fn server_addr(&self, kind: PortKind) -> SocketAddr {
        match kind {
            PortKind::C2S => SocketAddr::new(self.server_ip, self.port),
            PortKind::S2C => SocketAddr::new(self.server_ip, self.port.wrapping_add(1)),
        }
    }

    /// A client sends a datagram.
    pub fn client_send(&mut self, from: SocketAddr, to: SocketAddr, bytes: Vec<u8>) {
        self.to_server
            .send(Datagram { from, to, bytes }, self.now.monotonic);
    }

    /// One step of the network at the current time: the client-to-server link delivers what is
    /// due, the server polls, and what it sends enters the server-to-client link, whose due
    /// datagrams land in the clients' inboxes. Nothing waits for the clock.
    ///
    /// Returns the pass's session count (ACE `DoSessionWork`'s result, which counts a session
    /// dropped in this pass).
    pub fn pump(&mut self) -> usize {
        let now = self.now;
        for d in self.to_server.step(now.monotonic) {
            let kind = if d.to == self.server_addr(PortKind::C2S) {
                PortKind::C2S
            } else if d.to == self.server_addr(PortKind::S2C) {
                PortKind::S2C
            } else {
                self.undeliverable += 1;
                continue;
            };
            self.server.on_datagram(kind, d.from, &d.bytes, now);
        }
        let session_count = self.server.do_session_work(now);
        let outgoing: Vec<_> = self.server.drain_outgoing().collect();
        for out in outgoing {
            let from = self.server_addr(out.via_port_kind);
            self.to_clients.send(
                Datagram {
                    from,
                    to: out.to,
                    bytes: out.bytes,
                },
                now.monotonic,
            );
        }
        for d in self.to_clients.step(now.monotonic) {
            self.inboxes.entry(d.to).or_default().push_back(d);
        }
        session_count
    }

    /// The next datagram waiting for `client`.
    pub fn client_recv(&mut self, client: SocketAddr) -> Option<Datagram> {
        self.inboxes.get_mut(&client)?.pop_front()
    }

    /// Moves the clock.
    pub fn advance(&mut self, d: Duration) {
        self.now = self.now.advanced(d);
    }

    /// Replaces both links' models (what is already in flight keeps its schedule).
    pub fn set_link_models(&mut self, to_server: LinkModel, to_clients: LinkModel) {
        self.to_server.model = to_server;
        self.to_server.rng = SessionRandom::new(to_server.seed);
        self.to_clients.model = to_clients;
        self.to_clients.rng = SessionRandom::new(to_clients.seed);
    }

    /// Whether neither link holds anything due by now.
    #[must_use]
    pub fn links_idle(&self) -> bool {
        !self.to_server.has_due(self.now.monotonic) && !self.to_clients.has_due(self.now.monotonic)
    }

    /// The two links' counters: (client to server, server to client).
    #[must_use]
    pub const fn link_stats(&self) -> (LinkStats, LinkStats) {
        (self.to_server.stats, self.to_clients.stats)
    }
}
