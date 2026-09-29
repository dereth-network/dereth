//! Test support (feature `testing`): a client built from the shared client-side transport, a
//! stand-in world that answers logins, and a harness that runs them over [`MemoryNet`].
//!
//! [`TestClient`] is `dereth-client-net`'s own client transport (`Net`: sequencing, NAKs, retransmits,
//! reassembly, ACKs, TimeSync and EchoRequest cadences) plus the handshake the client's session
//! layer performs around it, transcribed from `dereth-client-runtime`'s `NetLink` (which this workspace
//! cannot depend on): `LoginRequest` every 2 s, `ConnectResponse` to port + 1 every 1/3 s until
//! the first server packet after the `ConnectRequest`. It is the retail client's behaviour, not
//! ACE's, so the server is tested against its real peer.
//!
//! Nothing here is a port; there are no ACE anchors.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use dereth_client_net::{Net, NetConfig as ClientNetConfig, RecipientId};
use dereth_primitives::LocalTime;
use dereth_primitives::{IncomingMessage, NetQueue, Transport};
use dereth_transport::conn::{
    build_login_request, handshake_port, ConnectRequest, ConnectionAuthenticator, LoginState,
};
use dereth_transport::flow::HANDSHAKE_RESEND;
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

use crate::driver::memory::MemoryNet;
use crate::{AccountSelect, CharacterError, Event, SessionId, SessionTerminationReason};

/// Where the client is in its login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Idle,
    /// Sending `LoginRequest`s, waiting for the `ConnectRequest`.
    LoginAuthenticating,
    /// Has the `ConnectRequest`; sending `ConnectResponse`s to port + 1.
    LoginConnecting,
    /// The first server packet after the `ConnectRequest` arrived.
    Connected,
    /// Gave up (20 unanswered login requests) or logged off.
    Done,
}

/// A client over the shared client transport. Times are seconds on the harness clock.
#[derive(Debug)]
pub struct TestClient {
    pub addr: SocketAddr,
    server: SocketAddr,
    net: Net,
    auth: ConnectionAuthenticator,
    login: LoginState,
    status: ClientStatus,
    cookie: u64,
    rec_id: u16,
    net_id: u16,
    iteration: u16,
    last_connect_ack: Option<LocalTime>,
    pending: Vec<(SocketAddr, Vec<u8>)>,
    /// When the first `ConnectRequest` arrived.
    pub connect_request_at: Option<f64>,
    /// When the first `ConnectResponse` went out.
    pub connect_response_sent_at: Option<f64>,
    /// When the link reached `Connected`.
    pub connected_at: Option<f64>,
    /// How many `LoginRequest`s and `ConnectResponse`s were sent.
    pub login_requests_sent: u32,
    pub connect_responses_sent: u32,
    /// Datagrams the client transport refused.
    pub rejected: u64,
    /// The client's clock rate against the harness clock (1.0: in step). A client whose clock runs
    /// fast is what ACE's echo check calls a speed hack.
    pub clock_scale: f64,
}

impl TestClient {
    /// A client at `addr` that will log in to the server at `server` (its port `P`).
    #[must_use]
    pub fn new(addr: SocketAddr, server: SocketAddr, account: &str, password: &str) -> Self {
        // The password goes as an archive string (compressed length, no padding), as the retail
        // client writes it.
        let auth = ConnectionAuthenticator::account_password(account, password);
        Self {
            addr,
            server,
            net: Net::new(ClientNetConfig::default()),
            auth,
            login: LoginState::new(),
            status: ClientStatus::Idle,
            cookie: 0,
            rec_id: 0,
            net_id: 0,
            iteration: 0,
            last_connect_ack: None,
            pending: Vec::new(),
            connect_request_at: None,
            connect_response_sent_at: None,
            connected_at: None,
            login_requests_sent: 0,
            connect_responses_sent: 0,
            rejected: 0,
            clock_scale: 1.0,
        }
    }

    #[must_use]
    pub const fn status(&self) -> ClientStatus {
        self.status
    }

    /// The client id the server assigned (the `ConnectRequest`'s `NetID`).
    #[must_use]
    pub const fn net_id(&self) -> u16 {
        self.net_id
    }

    /// Sets the client's `ConnectionAuthenticator` field; for tests of ACE's login checks.
    pub fn authenticator_mut(&mut self) -> &mut ConnectionAuthenticator {
        &mut self.auth
    }

    /// One datagram from the server.
    pub fn handle_datagram(&mut self, from: SocketAddr, bytes: &[u8], now: f64) {
        let Ok(parsed) = ParsedPacket::parse(bytes) else {
            self.rejected += 1;
            return;
        };
        if parsed.header.header.contains(PacketFlags::CONNECT_REQUEST) {
            if let Some(body) = parsed.optional.get(&PacketFlags::CONNECT_REQUEST) {
                if let Ok(cr) = ConnectRequest::from_bytes(body) {
                    self.handle_connect_request(&parsed.header, cr, from, now);
                }
            }
            return;
        }
        if self
            .net
            .feed(bytes, Some(from), LocalTime(now * self.clock_scale))
            .is_err()
        {
            self.rejected += 1;
        }
        if self.status == ClientStatus::LoginConnecting
            && self.net.connection_state(RecipientId(self.rec_id))
                == dereth_transport::conn::ConnectionState::Connected
        {
            self.status = ClientStatus::Connected;
            self.connected_at = Some(now);
        }
    }

    fn handle_connect_request(
        &mut self,
        header: &ProtoHeader,
        cr: ConnectRequest,
        from: SocketAddr,
        now: f64,
    ) {
        if self.status != ClientStatus::LoginAuthenticating {
            return;
        }
        self.rec_id = header.rec_id;
        self.net_id = u16::try_from(cr.net_id & 0xFFFF).unwrap_or(0);
        self.iteration = header.iteration;
        self.cookie = cr.cookie;
        self.net.add_connection_at(
            self.rec_id,
            self.net_id,
            self.iteration,
            cr.outgoing_seed,
            cr.incoming_seed,
            Some(from),
            LocalTime(now),
        );
        self.net
            .set_referral_cookie(RecipientId(self.rec_id), cr.cookie);
        self.status = ClientStatus::LoginConnecting;
        self.connect_request_at.get_or_insert(now);
        self.send_connect_ack(now);
    }

    fn send_connect_ack(&mut self, now: f64) {
        let mut p = OutPacket::new(ProtoHeader {
            rec_id: self.net_id,
            iteration: self.iteration,
            ..ProtoHeader::default()
        });
        if p.add_optional_header(
            PacketFlags::CONNECT_RESPONSE,
            self.cookie.to_le_bytes().to_vec(),
        )
        .is_err()
        {
            return;
        }
        if let Ok(bytes) = p.serialize(None) {
            let to = SocketAddr::new(self.server.ip(), handshake_port(self.server.port()));
            self.pending.push((to, bytes));
            self.last_connect_ack = Some(LocalTime(now));
            self.connect_responses_sent += 1;
            self.connect_response_sent_at.get_or_insert(now);
        }
    }

    fn send_login_request(&mut self) {
        let mut p = OutPacket::new(ProtoHeader::default());
        if p.add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&self.auth))
            .is_err()
        {
            return;
        }
        if let Ok(bytes) = p.serialize(None) {
            self.pending.push((self.server, bytes));
            self.login_requests_sent += 1;
        }
    }

    /// The client's frame: the login FSM, then the transport's connection walk and send half.
    pub fn tick(&mut self, now: f64) {
        let t = LocalTime(now * self.clock_scale);
        match self.status {
            ClientStatus::Idle | ClientStatus::LoginAuthenticating => {
                match self.login.should_send(t) {
                    Ok(true) => {
                        self.status = ClientStatus::LoginAuthenticating;
                        self.send_login_request();
                    }
                    Ok(false) => {}
                    Err(_) => self.status = ClientStatus::Done,
                }
            }
            ClientStatus::LoginConnecting => {
                if self
                    .last_connect_ack
                    .is_none_or(|l| t.seconds_since(l) >= HANDSHAKE_RESEND)
                {
                    self.send_connect_ack(now);
                }
            }
            ClientStatus::Connected | ClientStatus::Done => {}
        }
        self.net.process_connections(t);
        self.net.tick(t, Duration::ZERO);
        let server = self.server;
        self.pending.extend(
            self.net
                .take_outgoing()
                .into_iter()
                .map(|(bytes, to)| (to.unwrap_or(server), bytes)),
        );
    }

    /// Datagrams to send, with their destinations.
    pub fn take_outgoing(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        std::mem::take(&mut self.pending)
    }

    /// Queues one message (opcode first) on a client queue.
    pub fn send(&mut self, queue: NetQueue, message: &[u8]) {
        Transport::send(&mut self.net, queue, false, message);
    }

    /// The next message received, in the client's queue-drain order.
    pub fn poll(&mut self) -> Option<IncomingMessage> {
        Transport::poll(&mut self.net)
    }

    /// The client's goodbye: a `Disconnect` to every connection, then silence.
    pub fn log_off(&mut self) {
        self.net.log_off_server();
        let server = self.server;
        self.pending.extend(
            self.net
                .take_outgoing()
                .into_iter()
                .map(|(bytes, to)| (to.unwrap_or(server), bytes)),
        );
        self.status = ClientStatus::Done;
    }
}

/// How the stand-in world answers a login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginPolicy {
    /// Answer every `LoginRequest` at once, auto-creating accounts (ACE's
    /// `AllowAutoAccountCreation`), with ACE's password and in-use checks.
    Immediate,
    /// Leave `LoginRequest`s unanswered (they collect in [`Harness::events`]).
    Never,
}

/// A stand-in for the world's half of ACE's login (`DoLogin` and the account checks of
/// `AccountSelectCallback`), enough to drive the transport. Accounts are created on first use.
#[derive(Debug, Default)]
pub struct TestAccounts {
    accounts: BTreeMap<String, (u32, String)>,
}

impl TestAccounts {
    /// The world's reply to one `LoginRequest` event.
    pub fn answer(
        &mut self,
        net: &mut crate::ServerNet,
        session: SessionId,
        request: &crate::PacketInboundLoginRequest,
        now: crate::ClockSnapshot,
    ) {
        let password = request.password.clone().unwrap_or_default();
        if !self.accounts.contains_key(&request.account)
            && request.net_auth_type == crate::NetAuthType::AccountPassword
            && !password.is_empty()
        {
            let id = u32::try_from(self.accounts.len() + 1).unwrap_or(u32::MAX);
            self.accounts
                .insert(request.account.clone(), (id, password.clone()));
        }
        if net.account_select_callback(session, now) != AccountSelect::Continue {
            return;
        }
        let Some((id, stored)) = self.accounts.get(&request.account).cloned() else {
            net.reject_login(
                session,
                SessionTerminationReason::NotAuthorizedAccountNotFound,
                Some((net.messages.character_error)(
                    CharacterError::AccountDoesntExist,
                )),
                String::new(),
                now,
            );
            return;
        };
        if stored != password {
            net.reject_login(
                session,
                SessionTerminationReason::NotAuthorizedPasswordMismatch,
                Some((net.messages.boot_account)(Some(
                    " because the password entered for this account was not correct",
                ))),
                String::new(),
                now,
            );
            return;
        }
        // `account_login_boots_in_use` is true by default: boot the old session, refuse this one.
        if let Some(previous) = net.find_by_account(&request.account) {
            net.terminate(
                previous,
                SessionTerminationReason::AccountLoggedIn,
                Some((net.messages.character_error)(CharacterError::Logon)),
                String::new(),
                now,
            );
            net.reject_login(
                session,
                SessionTerminationReason::AccountInUse,
                Some((net.messages.character_error)(CharacterError::Logon)),
                String::new(),
                now,
            );
            return;
        }
        net.accept_login(session, id, request.account.clone(), 1);
    }
}

/// [`MemoryNet`] plus clients plus the stand-in world, stepped together on the virtual clock.
#[derive(Debug)]
pub struct Harness {
    pub net: MemoryNet,
    pub clients: Vec<TestClient>,
    /// Every server event except the `LoginRequest`s the policy answered.
    pub events: Vec<Event>,
    pub policy: LoginPolicy,
    pub accounts: TestAccounts,
}

impl Harness {
    #[must_use]
    pub fn new(net: MemoryNet) -> Self {
        Self {
            net,
            clients: Vec::new(),
            events: Vec::new(),
            policy: LoginPolicy::Immediate,
            accounts: TestAccounts::default(),
        }
    }

    /// Adds a client at `addr`; returns its index.
    pub fn add_client(&mut self, addr: SocketAddr, account: &str, password: &str) -> usize {
        let server = self.net.server_addr(crate::PortKind::C2S);
        self.clients
            .push(TestClient::new(addr, server, account, password));
        self.clients.len() - 1
    }

    /// Seconds on the virtual clock (its monotonic time).
    #[must_use]
    pub fn now_seconds(&self) -> f64 {
        self.net.now.monotonic.as_secs_f64()
    }

    /// One frame at the current time: every client ticks once, then datagrams and events are
    /// exchanged until nothing moves, all at the same instant. A reply therefore costs no virtual
    /// time unless the server itself defers it.
    pub fn step(&mut self) {
        let now = self.now_seconds();
        for c in &mut self.clients {
            c.tick(now);
        }
        for _ in 0..64 {
            let mut moved = 0usize;
            for c in &mut self.clients {
                for (to, bytes) in c.take_outgoing() {
                    self.net.client_send(c.addr, to, bytes);
                    moved += 1;
                }
            }
            self.net.pump();
            for c in &mut self.clients {
                while let Some(d) = self.net.client_recv(c.addr) {
                    c.handle_datagram(d.from, &d.bytes, now);
                    moved += 1;
                }
            }
            let events: Vec<Event> = self.net.server.events().collect();
            moved += events.len();
            for e in events {
                match (&e, self.policy) {
                    (
                        Event::LoginRequest {
                            session, request, ..
                        },
                        LoginPolicy::Immediate,
                    ) => {
                        let now = self.net.now;
                        self.accounts
                            .answer(&mut self.net.server, *session, request, now);
                    }
                    _ => self.events.push(e),
                }
            }
            if moved == 0 && self.net.links_idle() {
                break;
            }
        }
    }

    /// Steps, advancing the clock by `tick` after each step, until `until` holds or `max` of
    /// virtual time has passed. Returns whether `until` held.
    pub fn run_until(
        &mut self,
        max: Duration,
        tick: Duration,
        mut until: impl FnMut(&mut Self) -> bool,
    ) -> bool {
        let end = self.net.now.monotonic + max;
        loop {
            self.step();
            if until(self) {
                return true;
            }
            if self.net.now.monotonic >= end {
                return false;
            }
            self.net.advance(tick);
        }
    }
}
