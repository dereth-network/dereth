//! The session: the login connection and the world connections, over the shared connection
//! manager.
//!
//! **The login.** The client sends the game login to the login server's even port, again every two
//! seconds until the server answers with its connection request, at most twenty times. The
//! answer carries the cookie, the id the client puts in its headers and the two key-stream seeds;
//! the client registers the connection and acknowledges it to the server's odd port, again every
//! third of a second until the server's first ordinary packet arrives. The login connection is
//! then up, and stays up for the whole session: the character list, entering and leaving the
//! world, the data download and chat travel on it.
//!
//! **The world.** A referral (on the login connection, or on a world connection when the player
//! crosses into another world server's part of the world) names a world server and a cookie: the
//! connection manager sends the world login there until that server's connection request arrives,
//! which the client answers as it answered the login server's. The world server's first packet
//! switches the client's current world to it. A referral to a server the client already has a
//! connection to only refreshes that connection's cookie, and the switch follows on it.
//!
//! **Messages.** Reassembled messages are read queue by queue in the client's frame order
//! ([`crate::queues`]), and each is decoded whole. A message the client sends goes out on its
//! type's queue: the login-bound queues to the login server, the rest to the current world server.
//! Its blob id carries the ordering type and stamp of the counter it is stamped from
//! ([`crate::queues::counter_of`]), and the client's blob sequence starts at 0.
//!
//! **Leaving.** Leaving the world drops every connection but the login connection; logging off
//! says goodbye on every connection.
//!
//! **Losing the server.** The login server or the current world server going away (its goodbye
//! answered, or 140 seconds of silence) logs the client off, as does a net error once the login
//! is over: the session is over, and [`Event::ServerDied`] says so. A server letting a connection
//! go with an error says why first ([`Event::ConnectionError`]).

use std::collections::{BTreeMap, VecDeque};

use core::net::SocketAddr;

use auberean_protocol::{ClientMessage, DecodeError, ServerMessage};
use dereth_client_net::linkstatus::LinkStatusAverages;
use dereth_client_net::net::{from_seam_queue, to_seam_queue, BlobStamp};
use dereth_client_net::queues::Queue;
use dereth_client_net::{Net, NetConfig, RecipientId};
use dereth_primitives::{LocalTime, Transport};
use dereth_transport::conn::{
    handshake_port, iteration_verdict, ConnectRequest, ConnectionState, IterationVerdict,
    LoginState, NetErrorCode,
};
use dereth_transport::flow::HANDSHAKE_RESEND;
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

use crate::logon::Logon;
use crate::queues::{counter_of, is_kept, queue_of, receive_queues, Counter};

/// What a session needs to start.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The login server's even port.
    pub login_server: SocketAddr,
    pub logon: Logon,
}

/// A message as it arrived.
#[derive(Clone, Debug, PartialEq)]
pub struct Received {
    /// The queue it arrived on.
    pub queue: u16,
    /// Its 64-bit blob id: ordering type, server, ordering stamp, sequence.
    pub blob_id: u64,
    /// The connection it arrived on (the server's slot).
    pub server: u16,
    pub incoming: Incoming,
}

/// A received message, decoded or not.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Message(Box<ServerMessage>),
    /// A message of a type this client does not know, or one that did not read exactly.
    Undecodable {
        message_type: u32,
        bytes: Vec<u8>,
        error: DecodeError,
    },
}

/// What happened.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The login connection is up.
    LoginConnected,
    /// The login server refused the login, or never answered it.
    LoginFailed(NetErrorCode),
    /// A world server's connection came up: a referral was followed.
    WorldConnected {
        server: u16,
    },
    /// The current world server changed.
    WorldSwitched {
        server: u16,
    },
    Message(Received),
    /// A connection went away (the server said goodbye, or fell silent).
    ConnectionLost {
        server: u16,
        login: bool,
    },
    /// The login server (`login`) or the current world server is letting its connection go with
    /// an error; `error` is `None` for an error this client has no name for.
    ConnectionError {
        server: u16,
        login: bool,
        error: Option<NetErrorCode>,
    },
    /// The login server or the current world server is gone, or a net error came once the login
    /// was over: the client has logged off, and the session is over.
    ServerDied,
}

/// Counts for logs and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// World connections that came up.
    pub world_logins: u32,
    /// Messages read and dropped by their queue's rule.
    pub dropped: u32,
    /// Messages that did not decode.
    pub undecodable: u32,
    /// Datagrams the transport refused.
    pub rejected: u32,
}

/// One server the client has a connection to, or is connecting to.
#[derive(Clone, Copy, Debug)]
struct Link {
    login: bool,
    /// The server's even port, where the connection request came from.
    server: SocketAddr,
    cookie: u64,
    net_id: u16,
    iteration: u16,
    up: bool,
    acked_at: LocalTime,
}

/// The session.
#[derive(Debug)]
pub struct Session {
    net: Net,
    settings: Settings,
    login: LoginState,
    login_up: bool,
    login_failed: bool,
    /// Logged off: no login goes out again.
    logged_off: bool,
    links: BTreeMap<u16, Link>,
    world: u16,
    events: VecDeque<Event>,
    out: VecDeque<(SocketAddr, Vec<u8>)>,
    stats: Stats,
    /// The three ordering counters: ordered, unordered, data and chat.
    ordered: u16,
    unordered: u16,
    data: u16,
}

impl Session {
    /// A session about to log in: no login sent yet and nothing measured; the first login goes
    /// out at the first [`Self::tick`].
    // D2-05288, D2-05281, D2-05285, D2-05213
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        let config = NetConfig {
            receive_queues: receive_queues(),
            first_blob_sequence: 0,
            ..NetConfig::default()
        };
        Self {
            net: Net::new(config),
            settings,
            login: LoginState::new(),
            login_up: false,
            login_failed: false,
            logged_off: false,
            links: BTreeMap::new(),
            world: 0,
            events: VecDeque::new(),
            out: VecDeque::new(),
            stats: Stats::default(),
            ordered: 0,
            unordered: 0,
            data: 0,
        }
    }

    /// Whether the connection was measured again since last asked (every two seconds or more,
    /// on the current world server's traffic).
    pub fn take_heartbeat(&mut self) -> bool {
        self.net.take_heartbeat().is_some()
    }

    /// The connection's measures: the last snapshot (bytes each way, the time it spans, the time
    /// since the server was last heard) and the averages (round trips, packets lost).
    #[must_use]
    pub fn link_status(&self) -> &dereth_client_net::linkstatus::LinkStatusAverages {
        self.net.link_status()
    }

    /// The bytes received a second over the last two measures (see [`received_rate`]).
    #[must_use]
    pub fn average_bytes_per_second_received(&self) -> f64 {
        received_rate(self.net.link_status())
    }

    /// Counts so far.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The server's clock its last time-sync header carried, once.
    pub fn take_time_sync(&mut self) -> Option<f64> {
        self.net.take_time_sync()
    }

    /// Whether the login connection is up.
    #[must_use]
    pub fn login_connected(&self) -> bool {
        self.login_up
            && self.links.iter().any(|(id, l)| {
                l.login && self.net.connection_state(RecipientId(*id)) == ConnectionState::Connected
            })
    }

    /// The servers the client has a connection to, with whether each is the login server.
    pub fn connections(&self) -> impl Iterator<Item = (u16, bool)> + '_ {
        self.links
            .iter()
            .filter(|(_, l)| l.up)
            .map(|(id, l)| (*id, l.login))
    }

    /// The current world server's slot (0 before the first world connection).
    #[must_use]
    pub fn world_server(&self) -> u16 {
        self.world
    }

    /// A datagram from the network.
    pub fn handle_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        let Ok(parsed) = ParsedPacket::parse(bytes) else {
            self.stats.rejected += 1;
            return;
        };
        if let Some(body) = parsed.optional.get(&PacketFlags::CONNECT_REQUEST) {
            if let Ok(cr) = ConnectRequest::from_bytes(body) {
                self.connect_request(&parsed.header, &cr, from, now);
            }
            return;
        }
        if self.net.feed(bytes, Some(from), now).is_err() {
            self.stats.rejected += 1;
            return;
        }
        self.optional_headers(&parsed);
        self.notice_links();
        self.read_messages();
    }

    /// Whether the login is still going: neither connected nor failed, and not logged off.
    fn logging_in(&self) -> bool {
        !self.login_up && !self.login_failed && !self.logged_off
    }

    /// The login is over: connected (`None`), or failed and why. Told once.
    // D2-05243, D2-05258
    fn end_login(&mut self, failure: Option<NetErrorCode>) {
        if !self.logging_in() {
            return;
        }
        match failure {
            None => {
                self.login_up = true;
                self.events.push_back(Event::LoginConnected);
            }
            Some(code) => {
                self.login_failed = true;
                self.events.push_back(Event::LoginFailed(code));
            }
        }
    }

    /// The client's own reading of a packet's sections, once the shared transport has read
    /// them: the login server's new address, which the login is sent to from then on; a net
    /// error, which ends a login still going with that error (or well, for the no-error value),
    /// and otherwise logs the client off; and a server's goodbye with an error, told when it is
    /// the login server's or the current world server's.
    // D2-05273
    fn optional_headers(&mut self, parsed: &ParsedPacket) {
        if let Some(addr) = parsed
            .optional
            .get(&PacketFlags::LOGON_SERVER_ADDR)
            .and_then(|b| socket_address(b))
        {
            self.settings.login_server = addr;
        }
        if let Some(body) = parsed.optional.get(&PacketFlags::NET_ERROR) {
            let error = net_error(body);
            if self.logging_in() {
                // An error with no name here ends the login as the generic connection error.
                self.end_login(error.map(|e| e.unwrap_or(NetErrorCode::Generic)));
            } else {
                self.log_off();
                self.events.push_back(Event::ServerDied);
            }
        }
        if let Some(error) = parsed
            .optional
            .get(&PacketFlags::NET_ERROR_DISCONNECT)
            .and_then(|b| net_error(b))
        {
            let server = parsed.header.rec_id;
            let login = server == self.net.logon_recipient().0;
            if login || server == self.net.world_recipient().0 {
                self.events.push_back(Event::ConnectionError {
                    server,
                    login,
                    error,
                });
            }
        }
    }

    /// The server's connection request: register the connection and acknowledge it.
    // D2-05282
    fn connect_request(
        &mut self,
        header: &ProtoHeader,
        cr: &ConnectRequest,
        from: SocketAddr,
        now: LocalTime,
    ) {
        let rec = header.rec_id;
        let existing = self.links.get(&rec).map(|l| l.iteration);
        if iteration_verdict(existing, header.iteration) == IterationVerdict::Ignore {
            return;
        }
        let login = self.links.is_empty() || self.links.get(&rec).is_some_and(|l| l.login);
        let net_id = u16::try_from(cr.net_id & 0xFFFF).unwrap_or(0);
        self.net.add_connection_at(
            rec,
            net_id,
            header.iteration,
            cr.outgoing_seed,
            cr.incoming_seed,
            Some(from),
            now,
        );
        self.net.set_referral_cookie(RecipientId(rec), cr.cookie);
        if let Some(r) = self.net.receiver_mut(RecipientId(rec)) {
            // The connection request is the first datagram the connection counts.
            r.local_time_last_got_data = now;
            let _ = r.account_datagram(
                header.interval,
                u32::from(header.datalen)
                    + u32::try_from(dereth_transport::wire::HEADER_SIZE).unwrap_or(0),
            );
        }
        let link = Link {
            login,
            server: from,
            cookie: cr.cookie,
            net_id,
            iteration: header.iteration,
            up: false,
            acked_at: now,
        };
        self.links.insert(rec, link);
        self.send_connect_ack(rec, now);
    }

    /// The acknowledgement of a connection request: the cookie, to the server's odd port.
    // D2-05262
    fn send_connect_ack(&mut self, rec: u16, now: LocalTime) {
        let Some(link) = self.links.get_mut(&rec) else {
            return;
        };
        let mut p = OutPacket::new(ProtoHeader {
            rec_id: link.net_id,
            iteration: link.iteration,
            ..ProtoHeader::default()
        });
        if p.add_optional_header(
            PacketFlags::CONNECT_RESPONSE,
            link.cookie.to_le_bytes().to_vec(),
        )
        .is_err()
        {
            return;
        }
        if let Ok(bytes) = p.serialize(None) {
            let to = SocketAddr::new(link.server.ip(), handshake_port(link.server.port()));
            self.out.push_back((to, bytes));
            link.acked_at = now;
        }
    }

    /// The game login, to the login server's even port.
    fn send_login(&mut self) {
        let mut p = OutPacket::new(ProtoHeader::default());
        if p.add_optional_header(PacketFlags::LOGIN_REQUEST, self.settings.logon.body())
            .is_err()
        {
            return;
        }
        if let Ok(bytes) = p.serialize(None) {
            self.out.push_back((self.settings.login_server, bytes));
        }
    }

    /// Connections that came up or went away since the last look, and a switch of the current
    /// world.
    fn notice_links(&mut self) {
        let mut gone = Vec::new();
        let mut login_up = false;
        for (id, link) in &mut self.links {
            let state = self.net.connection_state(RecipientId(*id));
            if !link.up && state == ConnectionState::Connected {
                link.up = true;
                if link.login {
                    login_up = true;
                } else {
                    self.stats.world_logins += 1;
                    self.events.push_back(Event::WorldConnected { server: *id });
                }
            } else if link.up && state == ConnectionState::Disconnected {
                gone.push((*id, link.login));
            }
        }
        if login_up {
            self.end_login(None);
        }
        for (id, login) in gone {
            self.links.remove(&id);
            self.events
                .push_back(Event::ConnectionLost { server: id, login });
        }
        if self.net.take_server_died() {
            self.logged_off = true;
            self.events.push_back(Event::ServerDied);
        }
        let world = self.net.world_recipient().0;
        let logon = self.net.logon_recipient().0;
        if world != self.world && world != logon && world != 0 {
            self.world = world;
            self.events
                .push_back(Event::WorldSwitched { server: world });
        } else if world == logon {
            self.world = 0;
        }
    }

    /// Every reassembled message, in the client's queue order.
    fn read_messages(&mut self) {
        while let Some(m) = self.net.poll() {
            let queue = from_seam_queue(m.queue).map_or(0, Queue::id);
            if !is_kept(queue, m.opcode) {
                self.stats.dropped += 1;
                continue;
            }
            let mut bytes = m.opcode.to_le_bytes().to_vec();
            bytes.extend_from_slice(&m.body);
            let incoming = match ServerMessage::decode(&bytes) {
                Ok(msg) => Incoming::Message(Box::new(msg)),
                Err(error) => {
                    self.stats.undecodable += 1;
                    Incoming::Undecodable {
                        message_type: m.opcode,
                        bytes,
                        error,
                    }
                }
            };
            self.events.push_back(Event::Message(Received {
                queue,
                blob_id: m.blob_id.0,
                server: m.sender.0,
                incoming,
            }));
        }
    }

    /// Run the clocks: the login and acknowledgement resends, the referral walk, every
    /// connection's periodic sections, and the send queue.
    // D2-05212, D2-05279, D2-05277
    pub fn tick(&mut self, now: LocalTime) {
        if self.logging_in() && self.links.is_empty() {
            match self.login.should_send(now) {
                Ok(true) => self.send_login(),
                Ok(false) => {}
                Err(code) => self.end_login(Some(code)),
            }
        }
        let due: Vec<u16> = self
            .links
            .iter()
            .filter(|(_, l)| !l.up && now.seconds_since(l.acked_at) >= HANDSHAKE_RESEND)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.send_connect_ack(id, now);
        }
        self.net.process_connections(now);
        self.net.tick(now, core::time::Duration::ZERO);
        for (bytes, to) in self.net.take_outgoing() {
            if let Some(to) = to {
                self.out.push_back((to, bytes));
            }
        }
        self.notice_links();
        self.read_messages();
    }

    /// The next datagram to send, with where it goes.
    pub fn poll_transmit(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.out.pop_front()
    }

    /// The next thing that happened.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    /// Send a message on its queue, stamped from its counter. It goes out at the next
    /// [`Self::tick`]. Returns whether it was queued: it is not when there is no connection to
    /// send it on.
    // D2-05338, D2-05336
    pub fn send(&mut self, m: &ClientMessage) -> bool {
        let q = queue_of(m);
        let Some(queue) = Queue::from_wire(q) else {
            return false;
        };
        let counter = counter_of(m);
        let stamp = match counter {
            Counter::Ordered => {
                self.ordered = self.ordered.wrapping_add(1);
                self.ordered
            }
            Counter::Unordered => {
                self.unordered = self.unordered.wrapping_add(1);
                self.unordered
            }
            Counter::Data => {
                let s = self.data;
                self.data = s.wrapping_add(1);
                s
            }
        };
        let stamp = BlobStamp {
            ordering_type: counter.ordering_type(),
            stamp,
        };
        let queued = self
            .net
            .send_stamped(to_seam_queue(queue), stamp, &m.encode());
        if !queued && counter == Counter::Ordered {
            self.ordered = self.ordered.wrapping_sub(1);
        }
        queued
    }

    /// The client is entering the world: from now a switch of the logon server leaves the world
    /// server where it is, and the ordered counter starts again.
    // D2-05214, D2-05337
    pub fn enter_world(&mut self) {
        self.net.enter_world();
        self.ordered = 0;
    }

    /// The client left the world: every connection but the login connection goes.
    // D2-05215
    pub fn exit_world(&mut self) {
        self.net.exit_world_disconnect();
        self.links.retain(|_, l| l.login);
        self.world = 0;
    }

    /// What [`Self::log_off`] would send, made now and sending nothing: for a host that keeps a
    /// copy with the far end of its connection, in case it goes away before it can log off.
    #[must_use]
    pub fn goodbye(&self) -> Vec<(SocketAddr, Vec<u8>)> {
        self.net
            .goodbye()
            .into_iter()
            .filter_map(|(bytes, to)| Some((to?, bytes)))
            .collect()
    }

    /// Log off: goodbye on every connection, and nothing more is sent.
    // D2-05261
    pub fn log_off(&mut self) {
        self.net.log_off_server();
        for (bytes, to) in self.net.take_outgoing() {
            if let Some(to) = to {
                self.out.push_back((to, bytes));
            }
        }
        self.links.clear();
        self.login_up = false;
        self.logged_off = true;
    }
}

/// The bytes received a second over the last two measures: the bytes they counted over the time
/// they spanned, and 0 while that time is not above zero.
// D2-05468
#[must_use]
pub fn received_rate(link: &LinkStatusAverages) -> f64 {
    let time = link.time_diffs.total();
    if time <= 0.0 {
        return 0.0;
    }
    link.bytes_received.total() / time
}

/// A net error section's error: `None` for the no-error value, else the error, `Some(None)` when
/// this client has no name for it.
fn net_error(body: &[u8]) -> Option<Option<NetErrorCode>> {
    if body.get(..8) == Some(&NetErrorCode::None.pack()[..]) {
        return None;
    }
    Some(NetErrorCode::unpack(body).filter(|e| *e != NetErrorCode::None))
}

/// An IPv4 socket address as the wire carries it: the family (2), the port in network order, the
/// address, eight bytes of padding.
fn socket_address(body: &[u8]) -> Option<SocketAddr> {
    let b = body.get(..8)?;
    if u16::from_le_bytes([b[0], b[1]]) != 2 {
        return None;
    }
    let port = u16::from_be_bytes([b[2], b[3]]);
    let ip = core::net::Ipv4Addr::new(b[4], b[5], b[6], b[7]);
    Some(SocketAddr::new(core::net::IpAddr::V4(ip), port))
}
