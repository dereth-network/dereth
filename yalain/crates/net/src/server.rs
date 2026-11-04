//! The server's connections: the login service and the world service, each on an even/odd port
//! pair, every connection's handshake, transport and teardown.
//!
//! **Ports.** A client sends its login to the login service's even port and gets the handshake
//! from it; it answers to the odd port, and from then on the server sends from the odd port and
//! reads both. A world connection is the same on the world service's pair, reached by a referral
//! the login connection sends after the client asks to enter the world.
//!
//! **Sans-IO.** The host hands in datagrams with the port they arrived on ([`ServerNet::on_datagram`])
//! and the time ([`ServerNet::tick`]), and takes out datagrams to send ([`ServerNet::take_datagrams`])
//! and events ([`ServerNet::poll_event`]). Randomness (cookies and checksum seeds) comes from a
//! source the host gives ([`ServerNet::new`]): the executable's is the operating system's, and a
//! test's a fixed stream ([`fixed_stream`]).
//!
//! **A source that fails** is never made up for. A login or world login it fails for gets no
//! handshake and makes no connection (the client's own resend tries again, and a world login's
//! cookie is not spent); a referral it fails for closes the login, telling the client, so that it
//! does not wait for a world that will not come. Each is logged as an error.
//!
//! **A referral's cookie** lets one world connection in, once, within [`WORLD_LOGIN_TIMEOUT`]. It
//! travels in the clear, so its being random stops guessing but not reading: a world login must
//! also come from its login's address (or, sent to a network close by, from that kind of network).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};

use dereth_network_protocol::challenge::{self, Challenge};
use dereth_primitives::LocalTime;
use dereth_transport::conn::{ConnectRequest, ConnectSeeds, Referral};
use dereth_transport::flow::{CONNECTION_TIMEOUT, HANDSHAKE_RESEND, WORLD_LOGIN_TIMEOUT};
use dereth_transport::wire::{PacketFlags, ParsedPacket};

use crate::blob::{blob_id, BlobKind};
use crate::endpoint::{standalone, Endpoint, Role, Stats};
use crate::login::{parse_login, GameLogin};
use crate::pace::{Pace, DEFAULT_BYTES_PER_SECOND};

/// How many times a handshake is sent before the server gives up on the client.
pub const HANDSHAKE_TRIES: u32 = 20;

/// The first ephemeral stream a connection hands out. Any number serves: a stream is told
/// apart by its number alone, and the January 2017 world servers' were in this range.
pub const FIRST_STREAM: u32 = 0x0008_0000;

/// The two services.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Service {
    Login,
    World,
}

/// One of the four local sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Port {
    pub service: Service,
    /// The odd port of the pair: the one data is sent from.
    pub odd: bool,
}

impl Port {
    pub const LOGIN_EVEN: Self = Self {
        service: Service::Login,
        odd: false,
    };
    pub const LOGIN_ODD: Self = Self {
        service: Service::Login,
        odd: true,
    };
    pub const WORLD_EVEN: Self = Self {
        service: Service::World,
        odd: false,
    };
    pub const WORLD_ODD: Self = Self {
        service: Service::World,
        odd: true,
    };
    pub const ALL: [Self; 4] = [
        Self::LOGIN_EVEN,
        Self::LOGIN_ODD,
        Self::WORLD_EVEN,
        Self::WORLD_ODD,
    ];
}

/// What a service puts in its packet headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceId {
    /// The server's id in its headers and in a referral to it (below 0x100).
    pub server_id: u16,
    pub iteration: u16,
}

/// How the services identify themselves and where the world is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetConfig {
    pub login: ServiceId,
    pub world: ServiceId,
    /// The world service's even port as clients reach it: what a referral names (for a client
    /// close by, see [`ServerNet::refer_nearby`]).
    pub world_addr: SocketAddrV4,
    /// The rate each connection sends its large messages at ([`crate::pace`]).
    pub large_message_bytes_per_second: u32,
}

impl NetConfig {
    /// The ids the January 2017 login and world servers used, with the world at `world_addr`.
    #[must_use]
    pub const fn retail_ids(world_addr: SocketAddrV4) -> Self {
        Self {
            login: ServiceId {
                server_id: 0xF1,
                iteration: 5,
            },
            world: ServiceId {
                server_id: 0x0B,
                iteration: 2,
            },
            world_addr,
            large_message_bytes_per_second: DEFAULT_BYTES_PER_SECOND,
        }
    }

    const fn service(&self, s: Service) -> ServiceId {
        match s {
            Service::Login => self.login,
            Service::World => self.world,
        }
    }
}

/// A connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnId(pub u32);

/// Why a connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The client said goodbye.
    ClientLeft,
    /// Nothing from the client for too long, or no answer to the handshake.
    TimedOut,
    /// The server closed it.
    Closed,
    /// A new login from the same client replaced it.
    Replaced,
}

/// What happened, for the layer above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    /// A client finished the login handshake.
    LoggedIn {
        conn: ConnId,
        login: GameLogin,
    },
    /// A client reached the world by the referral `cookie`, which `login` sent.
    WorldJoined {
        conn: ConnId,
        login: ConnId,
        cookie: u64,
    },
    /// A message arrived.
    Message {
        conn: ConnId,
        queue: u16,
        blob_id: u64,
        bytes: Vec<u8>,
    },
    Closed {
        conn: ConnId,
        why: CloseReason,
    },
}

/// A datagram to send, from one of the local sockets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    pub from: Port,
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}

/// A fixed stream of numbers for [`ServerNet::new`], the same for the same `seed`: for tests alone,
/// since one number it gives shows every later one (splitmix64).
pub fn fixed_stream(seed: u64) -> impl FnMut() -> Result<u64, String> + Send {
    let mut state = seed;
    move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Ok(z ^ (z >> 31))
    }
}

/// Where cookies and checksum seeds come from: the host's source of 64 random bits at a time.
struct Random(Box<dyn FnMut() -> Result<u64, String> + Send>);

impl std::fmt::Debug for Random {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Random")
    }
}

/// What one handshake draws from the source.
#[derive(Debug, Clone, Copy)]
struct HandshakeNumbers {
    seeds: ConnectSeeds,
    cookie: u64,
}

#[derive(Debug)]
struct Handshake {
    cookie: u64,
    packet: Vec<u8>,
    sent_at: LocalTime,
    tries: u32,
}

#[derive(Debug)]
struct Conn {
    service: Service,
    peer: SocketAddr,
    client_id: u16,
    ep: Endpoint,
    handshake: Option<Handshake>,
    login: Option<GameLogin>,
    /// World connections: the login connection whose referral led here, and its cookie.
    via: Option<(ConnId, u64)>,
    /// The next sequence number of a message blob.
    sequence: u32,
    /// The next ordering stamp per queue and ordering kind.
    stamps: BTreeMap<(u16, u8), u16>,
    /// Per ephemeral stream, the stamp of the last message sent on it.
    streams: BTreeMap<u32, u16>,
    /// The next ephemeral stream to hand out.
    next_stream: u32,
}

/// This machine's own address on the way to a client, if it has one.
type Toward = Box<dyn FnMut(Ipv4Addr) -> Option<Ipv4Addr> + Send>;

/// Referrals of clients close by to an address on their own side ([`crate::referral`]).
struct Nearby {
    listening: IpAddr,
    toward: Toward,
}

impl std::fmt::Debug for Nearby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Nearby")
            .field("listening", &self.listening)
            .finish_non_exhaustive()
    }
}

/// The port a carried client numbers the world service as ([`ServerNet::refer_carried`]).
struct Carried(Box<dyn FnMut(SocketAddr) -> Option<u16> + Send>);

impl std::fmt::Debug for Carried {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Carried")
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingReferral {
    login: ConnId,
    sent_at: LocalTime,
    /// It sent the client to an address on a network close by.
    nearby: bool,
}

/// The server's connections.
#[derive(Debug)]
pub struct ServerNet {
    cfg: NetConfig,
    random: Random,
    conns: BTreeMap<ConnId, Conn>,
    next_conn: u32,
    referrals: HashMap<u64, PendingReferral>,
    nearby: Option<Nearby>,
    carried: Option<Carried>,
    switch_stamp: u32,
    /// The game clock at `clock_base`.
    game_time: f64,
    clock_base: LocalTime,
    /// The game clock is held at `game_time`.
    held: bool,
    out: Vec<Datagram>,
    events: VecDeque<NetEvent>,
    /// The Dereth Network hub's challenges heard on the login service's even port (the world's
    /// game port), oldest first, for the host to hand the world's network half. `None` while the
    /// world reads none (the network off, or no verification of the world waiting): a challenge
    /// is then dropped, as the packet reader drops it for its impossible size.
    hub_challenges: Option<VecDeque<Challenge>>,
}

/// The most hub challenges kept for the host between two of its passes; the rest are dropped.
pub const HUB_CHALLENGES_KEPT: usize = 16;

impl ServerNet {
    /// The services, with `random` for cookies and checksum seeds and the game clock reading
    /// `game_time` at `now`. `random` gives 64 random bits, or says why it has none.
    #[must_use]
    pub fn new(
        cfg: NetConfig,
        random: impl FnMut() -> Result<u64, String> + Send + 'static,
        game_time: f64,
        now: LocalTime,
    ) -> Self {
        Self {
            cfg,
            random: Random(Box::new(random)),
            conns: BTreeMap::new(),
            next_conn: 1,
            referrals: HashMap::new(),
            nearby: None,
            carried: None,
            switch_stamp: 0,
            game_time,
            clock_base: now,
            held: false,
            out: Vec::new(),
            events: VecDeque::new(),
            hub_challenges: None,
        }
    }

    /// Whether the game port reads the Dereth Network hub's challenges now: only while the world
    /// waits for the hub to verify it. Turned off, those not yet taken are dropped.
    pub fn hear_hub_challenges(&mut self, on: bool) {
        match (on, &self.hub_challenges) {
            (true, None) => self.hub_challenges = Some(VecDeque::new()),
            (false, Some(_)) => self.hub_challenges = None,
            _ => {}
        }
    }

    /// The hub's challenges heard since the last call, oldest first.
    pub fn take_hub_challenges(&mut self) -> Vec<Challenge> {
        self.hub_challenges
            .as_mut()
            .map(|k| k.drain(..).collect())
            .unwrap_or_default()
    }

    /// The configuration.
    #[must_use]
    pub const fn config(&self) -> &NetConfig {
        &self.cfg
    }

    /// Refer a client close by (on this machine or a network of its own kind) to the address it
    /// reached the services at, rather than to the configured world address, which stays the one
    /// for everyone else ([`crate::referral`]). `listening` is the address the services listen on;
    /// `toward` is this machine's own address on the way to a client, for services listening on
    /// every address (the host's route lookup).
    pub fn refer_nearby(
        &mut self,
        listening: IpAddr,
        toward: impl FnMut(Ipv4Addr) -> Option<Ipv4Addr> + Send + 'static,
    ) {
        self.nearby = Some(Nearby {
            listening,
            toward: Box::new(toward),
        });
    }

    /// Refer a client the host carries over one connection of its own (a WebSocket) to the world
    /// service's port as that connection numbers it: `port_of` says it for such a client's address,
    /// and nothing for any other client, which keeps the service's own port. Such a client has no
    /// other way there: its connection is all it reaches the server by, and its frames name ports
    /// by its own numbers.
    pub fn refer_carried(
        &mut self,
        port_of: impl FnMut(SocketAddr) -> Option<u16> + Send + 'static,
    ) {
        self.carried = Some(Carried(Box::new(port_of)));
    }

    /// The world service's address as a referral names it to a client at `client`.
    fn world_addr_for(&mut self, client: SocketAddr) -> SocketAddrV4 {
        let named = self.cfg.world_addr;
        let ip = match self.nearby.as_mut() {
            Some(n) => {
                crate::referral::world_ip(n.listening, *named.ip(), client.ip(), &mut n.toward)
            }
            None => *named.ip(),
        };
        let port = self
            .carried
            .as_mut()
            .and_then(|c| (c.0)(client))
            .unwrap_or(named.port());
        SocketAddrV4::new(ip, port)
    }

    /// The game clock now.
    #[must_use]
    pub fn game_time(&self, now: LocalTime) -> f64 {
        if self.held {
            return self.game_time;
        }
        self.game_time + now.seconds_since(self.clock_base)
    }

    /// Hold the game clock where it stands at `now` (a developer's setting, for comparing clients
    /// at one moment of the game's day): from then on it reads the same, for the game and in what
    /// every connection sends.
    pub fn hold_clock(&mut self, now: LocalTime) {
        self.game_time = self.game_time(now);
        self.clock_base = now;
        self.held = true;
        for c in self.conns.values_mut() {
            c.ep.hold_clock(now);
        }
    }

    /// Is the game clock held?
    #[must_use]
    pub const fn clock_held(&self) -> bool {
        self.held
    }

    /// The next event, if any.
    pub fn poll_event(&mut self) -> Option<NetEvent> {
        self.events.pop_front()
    }

    /// The datagrams to send.
    pub fn take_datagrams(&mut self) -> Vec<Datagram> {
        std::mem::take(&mut self.out)
    }

    /// The client address of a connection.
    #[must_use]
    pub fn peer(&self, conn: ConnId) -> Option<SocketAddr> {
        self.conns.get(&conn).map(|c| c.peer)
    }

    /// The service a connection is on.
    #[must_use]
    pub fn service(&self, conn: ConnId) -> Option<Service> {
        self.conns.get(&conn).map(|c| c.service)
    }

    /// A connection's transport counters.
    #[must_use]
    pub fn stats(&self, conn: ConnId) -> Option<Stats> {
        self.conns.get(&conn).map(|c| c.ep.stats)
    }

    /// How many fragments of a connection's large messages are waiting to be sent.
    #[must_use]
    pub fn large_waiting(&self, conn: ConnId) -> usize {
        self.conns.get(&conn).map_or(0, |c| c.ep.pace.waiting())
    }

    /// How many connections are open (handshakes included).
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.conns.len()
    }

    /// Have the messages queued on a connection all been acknowledged?
    #[must_use]
    pub fn acknowledged(&self, conn: ConnId) -> bool {
        self.conns
            .get(&conn)
            .is_some_and(|c| c.ep.messages_acknowledged())
    }

    fn free_client_id(&self, service: Service) -> Option<u16> {
        (1..=255u16).find(|id| {
            !self
                .conns
                .values()
                .any(|c| c.service == service && c.client_id == *id)
        })
    }

    /// 64 bits from the source; none, logged, when it fails.
    fn draw(&mut self) -> Option<u64> {
        (self.random.0)()
            .map_err(|e| tracing::error!("the random source failed: {e}"))
            .ok()
    }

    /// The seeds and cookie of a handshake; none when the source fails.
    #[allow(clippy::cast_possible_truncation)]
    fn handshake_numbers(&mut self) -> Option<HandshakeNumbers> {
        let seeds = self.draw()?;
        Some(HandshakeNumbers {
            seeds: ConnectSeeds {
                server_to_client: (seeds >> 32) as u32,
                client_to_server: seeds as u32,
            },
            cookie: self.draw()?,
        })
    }

    fn open(
        &mut self,
        service: Service,
        from: SocketAddr,
        login: Option<GameLogin>,
        via: Option<(ConnId, u64)>,
        numbers: HandshakeNumbers,
        now: LocalTime,
    ) {
        let Some(client_id) = self.free_client_id(service) else {
            tracing::warn!(?service, %from, "no free client id; login ignored");
            return;
        };
        let id = self.cfg.service(service);
        let HandshakeNumbers { seeds, cookie } = numbers;
        let game_time = self.game_time(now);
        let cr = ConnectRequest::new(game_time, cookie, u32::from(client_id), seeds);
        let packet = standalone(
            id.server_id,
            id.iteration,
            PacketFlags::CONNECT_REQUEST,
            cr.to_bytes().to_vec(),
        );
        let mut ep = Endpoint::new(
            Role::Server,
            id.server_id,
            client_id,
            id.iteration,
            seeds,
            from,
            game_time,
            now,
        );
        ep.pace = Pace::at_rate(self.cfg.large_message_bytes_per_second);
        if self.held {
            ep.hold_clock(now);
        }
        let conn = ConnId(self.next_conn);
        self.next_conn += 1;
        self.out.push(Datagram {
            from: Port {
                service,
                odd: false,
            },
            to: from,
            bytes: packet.clone(),
        });
        tracing::debug!(?conn, ?service, %from, client_id, "handshake sent");
        self.conns.insert(
            conn,
            Conn {
                service,
                peer: from,
                client_id,
                ep,
                handshake: Some(Handshake {
                    cookie,
                    packet,
                    sent_at: now,
                    tries: 1,
                }),
                login,
                via,
                sequence: 1,
                stamps: BTreeMap::new(),
                streams: BTreeMap::new(),
                next_stream: FIRST_STREAM,
            },
        );
    }

    fn close_with(&mut self, conn: ConnId, why: CloseReason, goodbye: bool) {
        if let Some(c) = self.conns.remove(&conn) {
            if goodbye && c.handshake.is_none() {
                self.out.push(Datagram {
                    from: Port {
                        service: c.service,
                        odd: true,
                    },
                    to: c.peer,
                    bytes: c.ep.disconnect_packet(),
                });
            }
            self.referrals.retain(|_, r| r.login != conn);
            self.events.push_back(NetEvent::Closed { conn, why });
        }
    }

    /// Close a connection, telling the client.
    pub fn close(&mut self, conn: ConnId) {
        self.close_with(conn, CloseReason::Closed, true);
    }

    /// Send what is queued on a connection now, then close it, telling the client: its last
    /// message and the goodbye leave together.
    pub fn end(&mut self, conn: ConnId, now: LocalTime) {
        if let Some(c) = self.conns.get_mut(&conn) {
            if c.handshake.is_none() {
                let from = Port {
                    service: c.service,
                    odd: true,
                };
                for bytes in c.ep.tick(now) {
                    self.out.push(Datagram {
                        from,
                        to: c.peer,
                        bytes,
                    });
                }
            }
        }
        self.close(conn);
    }

    /// Take in one datagram that arrived on `port` from `from`. A Dereth Network hub's challenge
    /// is taken before the packet reader: kept on the login service's even port while the world
    /// hears them ([`ServerNet::hear_hub_challenges`]), and otherwise dropped; it is never
    /// answered over UDP and makes no connection.
    pub fn on_datagram(&mut self, port: Port, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        if challenge::is_challenge(bytes) {
            if port == Port::LOGIN_EVEN {
                self.keep_hub_challenge(bytes);
            }
            return;
        }
        let p = match ParsedPacket::parse_from_client(bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(%from, ?port, "unparsable datagram: {e}");
                return;
            }
        };
        let h = p.header.header;
        if h.contains(PacketFlags::LOGIN_REQUEST) {
            if port.service != Service::Login {
                return;
            }
            self.login_request(from, &p, now);
        } else if h.contains(PacketFlags::WORLD_LOGIN_REQUEST) {
            if port.service != Service::World {
                return;
            }
            let cookie = p
                .optional
                .get(&PacketFlags::WORLD_LOGIN_REQUEST)
                .and_then(|b| b.get(..8))
                .map_or(0, |b| {
                    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
                });
            self.world_login_request(from, cookie, now);
        } else if h.contains(PacketFlags::CONNECT_RESPONSE) {
            let cookie = p
                .optional
                .get(&PacketFlags::CONNECT_RESPONSE)
                .and_then(|b| b.get(..8))
                .map_or(0, |b| {
                    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
                });
            self.connect_response(port.service, from, cookie, now);
        } else if h.contains(PacketFlags::CICMD_COMMAND) {
            // The client's keep-alive to the odd port: nothing to answer.
        } else {
            self.data(port.service, from, &p, bytes.len(), now);
        }
    }

    /// A datagram shaped as the hub's challenge: kept while the world hears them and it reads.
    fn keep_hub_challenge(&mut self, bytes: &[u8]) {
        let Some(kept) = self.hub_challenges.as_mut() else {
            return;
        };
        if kept.len() >= HUB_CHALLENGES_KEPT {
            return;
        }
        if let Some(c) = Challenge::parse(bytes) {
            kept.push_back(c);
        }
    }

    fn find(&self, service: Service, from: SocketAddr) -> Option<ConnId> {
        self.conns
            .iter()
            .find(|(_, c)| c.service == service && c.peer == from)
            .map(|(id, _)| *id)
    }

    fn login_request(&mut self, from: SocketAddr, p: &ParsedPacket, now: LocalTime) {
        if let Some(existing) = self.find(Service::Login, from) {
            if self
                .conns
                .get(&existing)
                .is_some_and(|c| c.handshake.is_some())
            {
                return; // a resend; the handshake is being resent on its own clock
            }
        }
        let Some(numbers) = self.handshake_numbers() else {
            return;
        };
        if let Some(existing) = self.find(Service::Login, from) {
            // A new login from a client already logged in: it started over.
            self.close_with(existing, CloseReason::Replaced, false);
        }
        let Some(login) = p
            .optional
            .get(&PacketFlags::LOGIN_REQUEST)
            .and_then(|b| parse_login(b))
        else {
            tracing::info!(%from, "a login that does not read; ignored");
            return;
        };
        self.open(Service::Login, from, Some(login), None, numbers, now);
    }

    fn world_login_request(&mut self, from: SocketAddr, cookie: u64, now: LocalTime) {
        if self
            .find(Service::World, from)
            .is_some_and(|c| self.conns.get(&c).is_some_and(|c| c.handshake.is_some()))
        {
            return;
        }
        let Some(r) = self.referrals.get(&cookie).copied() else {
            tracing::info!(%from, "a world login with an unknown cookie; ignored");
            return;
        };
        let Some(login) = self.conns.get(&r.login) else {
            return;
        };
        // The cookie travels in the clear: one read off the wire must not let another address in.
        // A client sent to the world on a network close by may come from another address there
        // than its login did: a login through the router's public address arrives from the
        // router's own address on the network, the world login straight from the client's.
        let across = r.nearby && crate::referral::is_nearby(from.ip());
        if login.peer.ip() != from.ip() && !across {
            tracing::info!(%from, "a world login from another address than its login; ignored");
            return;
        }
        // No numbers, no handshake: the cookie stays good for the client's next resend.
        let Some(numbers) = self.handshake_numbers() else {
            return;
        };
        if let Some(existing) = self.find(Service::World, from) {
            self.close_with(existing, CloseReason::Replaced, false);
        }
        // Used once: from here a second world login with this cookie is one with an unknown cookie.
        self.referrals.remove(&cookie);
        self.open(
            Service::World,
            from,
            None,
            Some((r.login, cookie)),
            numbers,
            now,
        );
    }

    fn connect_response(
        &mut self,
        service: Service,
        from: SocketAddr,
        cookie: u64,
        now: LocalTime,
    ) {
        let Some((&id, _)) = self.conns.iter().find(|(_, c)| {
            c.service == service
                && c.peer.ip() == from.ip()
                && c.handshake.as_ref().is_some_and(|h| h.cookie == cookie)
        }) else {
            return;
        };
        let Some(c) = self.conns.get_mut(&id) else {
            return;
        };
        c.handshake = None;
        c.ep.recv.local_time_last_got_data = now;
        c.ep.recv.time_stamp = now;
        match c.service {
            Service::Login => {
                let login = c.login.clone().unwrap_or_else(|| GameLogin {
                    version: String::new(),
                    auth_type: 0,
                    auth_flags: 0,
                    timestamp: 0,
                    account: String::new(),
                    session_key: Vec::new(),
                    ticket: Vec::new(),
                });
                self.events
                    .push_back(NetEvent::LoggedIn { conn: id, login });
            }
            Service::World => {
                // The world's first packet is the server switch, alone.
                self.switch_stamp += 1;
                let mut body = self.switch_stamp.to_le_bytes().to_vec();
                body.extend_from_slice(&0u32.to_le_bytes());
                c.ep.send_section(PacketFlags::SERVER_SWITCH, body);
                let peer = c.peer;
                for dg in c.ep.flush_alone(now) {
                    self.out.push(Datagram {
                        from: Port::WORLD_ODD,
                        to: peer,
                        bytes: dg,
                    });
                }
                let (login, cookie) = c.via.unwrap_or((ConnId(0), 0));
                self.events.push_back(NetEvent::WorldJoined {
                    conn: id,
                    login,
                    cookie,
                });
            }
        }
        tracing::debug!(conn = ?id, ?service, %from, "connected");
    }

    fn data(
        &mut self,
        service: Service,
        from: SocketAddr,
        p: &ParsedPacket,
        len: usize,
        now: LocalTime,
    ) {
        let Some(id) = self.find(service, from) else {
            return;
        };
        let Some(c) = self.conns.get_mut(&id) else {
            return;
        };
        if c.handshake.is_some() {
            return;
        }
        let inc = match c.ep.feed(p, len, now) {
            Ok(inc) => inc,
            Err(e) => {
                tracing::debug!(conn = ?id, seq = p.header.seq_id, "packet refused: {e}");
                return;
            }
        };
        for b in inc.blobs {
            self.events.push_back(NetEvent::Message {
                conn: id,
                queue: b.queue_id,
                blob_id: b.id.0,
                bytes: b.payload,
            });
        }
        if inc.disconnect {
            self.close_with(id, CloseReason::ClientLeft, false);
        }
    }

    /// Queue a message on a connection. Returns the blob id it travels under.
    ///
    /// The ordering stamp counts per queue and ordering kind from 1, so the messages of one kind
    /// on one queue carry consecutive stamps: the client holds a private message until the one
    /// stamped before it has arrived.
    pub fn send(
        &mut self,
        conn: ConnId,
        queue: u16,
        kind: BlobKind,
        bytes: Vec<u8>,
    ) -> Option<u64> {
        let c = self.conns.get_mut(&conn)?;
        let stamp = c.stamps.entry((queue, kind.0)).or_insert(1);
        let this = *stamp;
        *stamp = this.wrapping_add(1);
        let seq = c.sequence;
        c.sequence = seq.wrapping_add(1);
        #[allow(clippy::cast_possible_truncation)]
        let id = blob_id(kind, self.cfg.service(c.service).server_id as u8, this, seq);
        c.ep.send_blob(queue, id, bytes);
        Some(id)
    }

    /// Start a queue's stamps of one ordering kind again from 1: a character entering the world
    /// on a connection that has carried one before (the client keeps a private order per
    /// character, from 1).
    pub fn restart_stamps(&mut self, conn: ConnId, queue: u16, kind: BlobKind) {
        if let Some(c) = self.conns.get_mut(&conn) {
            c.stamps.remove(&(queue, kind.0));
        }
    }

    /// A new ephemeral stream on a connection: a sequence of messages each of which supersedes
    /// the ones before it (an object's positions, say).
    pub fn new_stream(&mut self, conn: ConnId) -> Option<u32> {
        let c = self.conns.get_mut(&conn)?;
        let s = c.next_stream;
        c.next_stream = s.wrapping_add(1);
        Some(s)
    }

    /// Queue a message on an ephemeral stream: it travels under the stream's number with the
    /// next stamp of the stream, and the client drops any older one of the stream it has not
    /// finished reading. Returns the blob id.
    pub fn send_ephemeral(
        &mut self,
        conn: ConnId,
        queue: u16,
        stream: u32,
        bytes: Vec<u8>,
    ) -> Option<u64> {
        let c = self.conns.get_mut(&conn)?;
        let stamp = c.streams.entry(stream).or_insert(0);
        *stamp = stamp.wrapping_add(1);
        #[allow(clippy::cast_possible_truncation)]
        let id = blob_id(
            BlobKind::EPHEMERAL,
            self.cfg.service(c.service).server_id as u8,
            *stamp,
            stream,
        );
        c.ep.send_blob(queue, id, bytes);
        Some(id)
    }

    /// Send a referral to the world service on a login connection. Returns the cookie the client
    /// will bring to the world. With no cookie to give (the random source failed) the login is
    /// closed, telling the client.
    pub fn refer(&mut self, login: ConnId, now: LocalTime) -> Option<u64> {
        let c = self.conns.get(&login)?;
        if c.service != Service::Login || c.handshake.is_some() {
            return None;
        }
        let Some(cookie) = self.draw() else {
            self.close(login);
            return None;
        };
        let peer = self.conns.get(&login)?.peer;
        let addr = self.world_addr_for(peer);
        let c = self.conns.get_mut(&login)?;
        let r = Referral {
            cookie,
            addr,
            id_server: self.cfg.world.server_id,
            family: 2,
        };
        c.ep.send_section(PacketFlags::REFERRAL, r.to_bytes().to_vec());
        self.referrals.insert(
            cookie,
            PendingReferral {
                login,
                sent_at: now,
                nearby: crate::referral::is_nearby(IpAddr::V4(*addr.ip())),
            },
        );
        Some(cookie)
    }

    /// Run every connection for this moment: resend handshakes, send what is queued, close the
    /// silent.
    pub fn tick(&mut self, now: LocalTime) {
        let mut closing = Vec::new();
        for (id, c) in &mut self.conns {
            if let Some(h) = c.handshake.as_mut() {
                if now.seconds_since(h.sent_at) >= HANDSHAKE_RESEND {
                    if h.tries >= HANDSHAKE_TRIES {
                        closing.push(*id);
                        continue;
                    }
                    h.tries += 1;
                    h.sent_at = now;
                    self.out.push(Datagram {
                        from: Port {
                            service: c.service,
                            odd: false,
                        },
                        to: c.peer,
                        bytes: h.packet.clone(),
                    });
                }
                continue;
            }
            for dg in c.ep.tick(now) {
                self.out.push(Datagram {
                    from: Port {
                        service: c.service,
                        odd: true,
                    },
                    to: c.peer,
                    bytes: dg,
                });
            }
            if now.seconds_since(c.ep.last_heard()) > CONNECTION_TIMEOUT {
                closing.push(*id);
            }
        }
        for id in closing {
            self.close_with(id, CloseReason::TimedOut, true);
        }
        self.referrals
            .retain(|_, r| now.seconds_since(r.sent_at) <= WORLD_LOGIN_TIMEOUT);
    }
}
