//! `ClientNetwork`: the socket, the login FSM and the three-way handshake.
//!
//! # What this module is, and what it deliberately is not
//!
//! This module wires; it does not implement. Everything on the wire here comes from the transport
//! crates (`dereth_client_net`: `build_login_request`, `ConnectRequest`, `handshake_port`, `LoginState`,
//! `OutPacket`, `Net`) and everything above a blob comes from the session (`dereth_client_net::client_session::Session`,
//! which reaches the network only through [`dereth_primitives::Transport`] — an isolation this module does
//! not breach: it hands `Session` a real `Net` and nothing else changes).
//!
//! What lives here is the *driver*: the object the client calls
//! `ClientNetwork` — the thing that owns the socket, sends the `LoginRequest`, answers a
//! `ConnectRequest` with a `ConnectResponse` **on port + 1**, runs the two resend cadences and
//! pumps the transport once per frame. The transport supplies the handshake's *parts* and leaves
//! "the frame loop that calls `tick`" to the application, so the driver is here.
//! It is kept socket-free in `ClientNetwork` so it can be driven from a capture with no I/O at all,
//! with `NetLink` adding the real `NetSocket` on top.
//!
//! # The two-socket rule, which is the easiest thing here to get wrong
//!
//! The server binds `P` and `P + 1`. The `LoginRequest` goes to `P`; the server answers from `P`,
//! so the receiver's stored endpoint records `P`. The `ConnectResponse` is sent to **`P + 1`**, which tells
//! the server which source port the client transmits from; from then on the server sends from
//! `P + 1` while the client keeps sending to `P`. Header verification compares the source **address**
//! and never the source port, which is what makes that asymmetry legal.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use dereth_client_net::client_session::{Session, SessionEvent, SessionState};
use dereth_client_net::socket::{
    parse_host_spec, resolve_sin_addr, NetSocket, PortMode, RECV_BUDGET_MS,
};
use dereth_client_net::{Net, NetConfig};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_transport::conn::{
    build_login_request, handshake_port, iteration_verdict, ConnectRequest,
    ConnectionAuthenticator, IterationVerdict, LoginState, NetErrorCode,
};
use dereth_transport::flow::HANDSHAKE_RESEND;
use dereth_transport::wire::{PacketFlags, ParsedPacket, ProtoHeader, RECV_BUFFER_SIZE};
use dereth_transport::OutPacket;

/// Anything that stops the network layer coming up.
#[derive(Debug, thiserror::Error)]
pub enum ClientNetworkError {
    /// The bad-server-address network error from client address initialization.
    #[error("Invalid server address: {0}")]
    BadServerAddress(String),
    /// `ID_NetError_CantSocket` / `ID_NetError_CantBind`.
    #[error("Couldn't create local socket.  Make sure you have a working network.: {0}")]
    Socket(#[from] dereth_client_net::NetError),
    /// The credential has no windows-1252 form, so cannot be
    /// packed. The original client's narrow byte string cannot carry it either.
    #[error("Invalid or corrupted authentication")]
    BadCredential,
}

/// The link state the client's connection spin loop waits on.
///
/// The names are the client's `Net_*` plugin notifications
/// (the client network notifies plugins of status changes), not invented ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// Nothing sent yet.
    Idle,
    /// The login is authenticating — `LoginRequest`s are going out, the logon recipient id is 0.
    LoginAuthenticating,
    /// The login is connecting — a `ConnectRequest` arrived and `ConnectResponse`s are going out
    /// every 0.333333333 s. `ConnectionState::ConnectionRequestAcked`.
    LoginConnecting,
    /// The login is connected — the first packet that was not a `ConnectRequest` arrived.
    /// `ConnectionState::Connected`. **This is not "in world"**; that is `0x0013`, and it is
    /// [`SessionState::Playable`].
    Connected,
    /// The transport completed its `NetErrorDisconnect` handshake and removed the connection.
    Disconnected,
    /// The FSM ended with a `NetError`.
    Failed(NetErrorCode),
}

/// One datagram waiting for the socket, with the address it must go to.
///
/// The address is carried per datagram because there are two: the handshake's `ConnectResponse`
/// goes to `P + 1` and everything else to `P`.
pub type Datagram = (Vec<u8>, SocketAddr);

/// The process-wide connection status that the connection lamp reads.
///
/// Its only reader is the connection lamp, while producers live in network phases. A thread-local
/// mirrors that process-wide lifetime without `unsafe` and
/// without threading a field through the frame loop; the client is single-threaded.
///
/// It has two producers. A successful connection stamps the
/// login-connected transition. Thereafter the status is refreshed
/// on the current connected receiver's two-second network
/// update cadence, whether or not that invocation received another datagram.
pub mod link_status_holder {
    use std::cell::Cell;

    thread_local! {
        /// `(connected, last-heard-from-current-server)`.
        static STATE: Cell<(bool, f64)> = const { Cell::new((false, 0.0)) };
        /// Packet loss, returned directly as a float.
        ///
        /// **Its compiled-in initial value is `1.0f`, not zero**:
        /// initialization stores the `1.0f` bit pattern in the packet-loss field. A client that
        /// has heard nothing from a server reports total
        /// loss, and that is the value the panel shows until the first heartbeat lands.
        static PACKET_LOSS: Cell<f32> =
            const { Cell::new(dereth_client_net::linkstatus::INITIAL_PACKET_LOSS) };
    }

    /// On login connection, mark the link connected and record the supplied time.
    pub fn on_connected(now: f64) {
        STATE.with(|s| s.set((true, now)));
    }

    /// Stamp the current connection's two-second
    /// snapshot without changing the link state.
    ///
    /// **`now` is the frame timer's `cur_time`, never its `local_time`.**
    /// [`connection_status`] subtracts from that same clock. Supplying network-local time
    /// instead makes the difference include the external clock offset instead of just elapsed time. The one
    /// caller is `App::frame`, which owns both clocks; the network layer publishes only the
    /// successful-connect and heartbeat facts.
    pub fn on_heartbeat(now: f64) {
        STATE.with(|s| {
            let (connected, _) = s.get();
            s.set((connected, now));
        });
    }

    /// The heartbeat handler's second store sets packet loss to the averaged packet loss.
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the client stores the double-precision average in a float field.
    pub fn on_packet_loss(loss: f64) {
        PACKET_LOSS.with(|s| s.set(loss as f32));
    }

    /// Packet-loss percentage returns the stored floating-point field directly.
    ///
    /// A **ratio** in spite of the name; see [`dereth_client_net::linkstatus`].
    #[must_use]
    pub fn packet_loss() -> f32 {
        PACKET_LOSS.with(Cell::get)
    }

    /// For a test that wants a fresh holder. Never called in production.
    pub fn reset_packet_loss() {
        PACKET_LOSS.with(|s| s.set(dereth_client_net::linkstatus::INITIAL_PACKET_LOSS));
    }

    /// A login connection error — the link state becomes disconnected.
    pub fn disconnected() {
        STATE.with(|s| {
            let (_, t) = s.get();
            s.set((false, t));
        });
    }

    /// Connection status as an `Option`: `None` is the
    /// not-connected case, where the client returns 0.0 **and** clears the caller's flag, which
    /// every arm of the link-state update then re-tests.
    #[must_use]
    pub fn connection_status(now: f64) -> Option<f64> {
        STATE.with(|s| {
            let (connected, t) = s.get();
            connected.then(|| (now - t).max(0.0))
        })
    }
}

/// `0x01EA Character_ReturnPing`'s arrival counter — the receive half of the link-status panel's round
/// trip.
///
/// It is a thread-local for the reason [`link_status_holder`] is one: the arrival is decoded in
/// `crate::interaction::apply_events`, which holds neither the `Hud` nor the screen, and the only
/// reader is `HudView::ping_returns`. A **count** rather than a timestamp because
/// the link-status panel's ping handler subtracts at notice-delivery time and this build
/// has no notice bus; the panel does the subtraction on the frame it sees the count move.
pub mod ping_holder {
    use std::cell::Cell;

    thread_local! {
        static RETURNS: Cell<u64> = const { Cell::new(0) };
    }

    /// One `0x01EA` decoded.
    pub fn returned() {
        RETURNS.with(|s| s.set(s.get().wrapping_add(1)));
    }

    /// How many have arrived since the process started.
    #[must_use]
    pub fn returns() -> u64 {
        RETURNS.with(Cell::get)
    }

    /// For a test that wants a fresh count. Never called in production.
    pub fn reset() {
        RETURNS.with(|s| s.set(0));
    }
}

/// The login FSM and the handshake, over the transport and the session — with no
/// socket, so a capture can drive it.
#[derive(Debug)]
pub struct ClientNetwork {
    /// The session, holding the transport. Public because the frame loop reads its
    /// state and a test reads what it sent, matching the client's process-wide protocol facade.
    ///
    pub session: Session<Net>,
    /// Client network authentication, filled during authentication initialization.
    auth: ConnectionAuthenticator,
    /// The client network's logon data.
    login: LoginState,
    status: LinkStatus,
    /// The logon server address — where the `LoginRequest` goes. Port `P`.
    logon_addr: SocketAddr,
    /// the address *and port* the `ConnectRequest` arrived from, which is
    /// `P`. Everything after the handshake goes here.
    peer: Option<SocketAddr>,
    /// The logon address with port `P + 1`. Only the `ConnectResponse` goes here.
    handshake_addr: Option<SocketAddr>,
    /// echoed in the `ConnectResponse`.
    cookie: u64,
    /// The receiver id, network id and iteration of the login connection.
    rec_id: u16,
    net_id: u16,
    iteration: u16,
    /// The last handshake sent by the client network.
    last_connect_ack: Option<LocalTime>,
    pending: Vec<Datagram>,
    /// Set once, and only reported: the client shows the string and gives up.
    error: Option<NetErrorCode>,
    /// Datagrams the transport rejected. Packet processing logs none of them and bumps
    /// its bad-packets-received counter; keeping the count lets a replay assert that a capture was
    /// consumed whole, which is the only way to notice a silent drop.
    rejected: u32,
    /// The `dereth::trace::net` target is on ([`crate::trace::NET`]): log every received
    /// datagram's header, at `debug`.
    ///
    /// The retail equivalent is `acpl.dll`'s received-wire-data logger, which the shipped build wires
    /// into a slot nothing calls, so there is no faithful spelling to
    /// copy. Read once here rather than per datagram.
    trace: bool,
    /// The exact login-connected transition, waiting for App to stamp.
    connected_pending: bool,
    /// Whether the login ever completed. A login error before this is the initial connection
    /// failing, which the player is shown in a box before the process exits; one after it is a
    /// link that was up and went down.
    login_completed: bool,
}

impl ClientNetwork {
    /// Connection setup minus the socket: parse the host, build the authenticator and
    /// start the login FSM.
    ///
    /// `connection_sequence_number` is the real-time value captured when the request starts; ACE reads it
    /// as `Timestamp` and does nothing with it. It is a parameter rather than a wall-clock read so
    /// a replay is deterministic.
    ///
    /// # Errors
    /// [`ClientNetworkError::BadServerAddress`] when `-h` names no address this build can resolve.
    pub fn new(
        host: &str,
        default_port: u16,
        account: &str,
        password: &str,
        connection_sequence_number: u32,
    ) -> Result<Self, ClientNetworkError> {
        let spec = parse_host_spec(host, default_port);
        // `inet_addr()`, then `gethostbyname()`, so `-h` accepts a host name as well as a
        // dotted quad.
        let logon_addr = resolve_sin_addr(&spec.host, spec.port)
            .ok_or_else(|| ClientNetworkError::BadServerAddress(host.to_string()))?;

        let mut auth = ConnectionAuthenticator::account_password(account, password);
        auth.connection_sequence_number = connection_sequence_number;
        // **The password is an archive string, not a packed one**: compressed length, then the
        // bytes, with **no padding**, where the account name two fields earlier is a `u16`
        // length, the bytes and zero padding to 4. The authenticator already packs it that way;
        // it is written again here from the session's string encoder because that one encodes
        // the text as windows-1252, as the original client's narrow strings are, and refuses a
        // character that has no windows-1252 form rather than sending its UTF-8 bytes.
        let mut w = dereth_protocol::Writer::new();
        w.astring(password)
            .map_err(|_| ClientNetworkError::BadCredential)?;
        auth.extra_data = w.into_inner();

        let config = NetConfig {
            host: spec.host,
            port: spec.port,
            ..NetConfig::default()
        };
        Ok(Self {
            session: Session::new(Net::new(config)),
            auth,
            login: LoginState::new(),
            status: LinkStatus::Idle,
            logon_addr,
            peer: None,
            handshake_addr: None,
            cookie: 0,
            rec_id: 0,
            net_id: 0,
            iteration: 0,
            last_connect_ack: None,
            pending: Vec::new(),
            error: None,
            rejected: 0,
            trace: crate::trace::net(),
            connected_pending: false,
            login_completed: false,
        })
    }

    #[must_use]
    pub fn status(&self) -> LinkStatus {
        self.status
    }

    /// The error that ended the login **before the link was ever up**, if that is how it ended.
    ///
    /// This is the initial connection failing: a refusal the shard sent before the handshake (a
    /// wrong client version, a shutdown), or the login requests going unanswered. The client shows
    /// it in a modal box and exits ([`crate::connect_failure`]). A login error after the link has
    /// been up is not this, and answers `None`.
    #[must_use]
    pub fn login_refusal(&self) -> Option<NetErrorCode> {
        match self.status {
            LinkStatus::Failed(code) if !self.login_completed => Some(code),
            _ => None,
        }
    }

    /// Was the login connection successfully established since this was last asked?
    ///
    /// Packet processing advances from `ConnectionState::ConnectionRequestAcked` only after
    /// accepting a non-`ConnectRequest`; ending the login context then emits the login-connected
    /// status.
    /// App consumes this edge because it owns the current frame time.
    pub fn take_connected(&mut self) -> bool {
        std::mem::take(&mut self.connected_pending)
    }

    /// The link-heartbeat edge and the packet-loss average that
    /// goes with it.
    ///
    /// `Some(ratio)` on the first ask after the transport took a
    /// two-second snapshot; `None` otherwise. See [`dereth_client_net::linkstatus`] for the arithmetic and
    /// for why the number is a ratio rather than a percentage.
    pub fn take_link_heartbeat(&mut self) -> Option<f64> {
        self.session.transport.take_heartbeat()
    }

    /// The live packet-loss average, whether or not a
    /// heartbeat is pending. For a test that wants to read it without the edge.
    #[must_use]
    pub fn average_packet_loss(&self) -> f64 {
        self.session.transport.average_packet_loss()
    }

    /// The session's own state. `0x0013` is what makes this [`SessionState::Playable`].
    #[must_use]
    pub fn session_state(&self) -> SessionState {
        self.session.state()
    }

    /// Where the `LoginRequest` was sent.
    #[must_use]
    pub fn logon_addr(&self) -> SocketAddr {
        self.logon_addr
    }

    /// Where a `ConnectResponse` goes: the server's port **+ 1**.
    #[must_use]
    pub fn handshake_addr(&self) -> Option<SocketAddr> {
        self.handshake_addr
    }

    #[must_use]
    pub fn error(&self) -> Option<NetErrorCode> {
        self.error
    }

    /// How many received datagrams the transport rejected.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    /// The character list, once `0xF658` has arrived.
    #[must_use]
    pub fn characters(&self) -> &dereth_protocol::login::LoginCharacterSet {
        self.session.characters()
    }

    /// The player's own object id, once `0xF746` has arrived.
    #[must_use]
    pub fn player_id(&self) -> Option<ObjectId> {
        self.session.player_id()
    }

    /// Datagrams the stack wants sent, oldest first, each with its destination.
    pub fn take_outgoing(&mut self) -> Vec<Datagram> {
        std::mem::take(&mut self.pending)
    }

    /// The datagrams [`Self::log_off_server`] would send, with where each goes, without sending
    /// them: see `dereth_client_net::net::Net::goodbye`.
    #[must_use]
    pub fn goodbye(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.session
            .transport
            .goodbye()
            .into_iter()
            .filter_map(|(bytes, to)| Some((bytes, to.or(self.peer)?)))
            .collect()
    }

    /// Queue the transport-level goodbye.
    ///
    /// Returns how many standalone `Disconnect` packets were queued, which is one
    /// per entry in the transport's connection list; a client that never connected queues none, and that is a
    /// number rather than a silence. `NetLink::log_off_server` is what puts them on the wire.
    pub fn log_off_server(&mut self) -> usize {
        let n = self.session.transport.log_off_server();
        for (bytes, to) in self.session.transport.take_outgoing() {
            if let Some(to) = to.or(self.peer) {
                self.pending.push((bytes, to));
            }
        }
        n
    }

    /// Shared packet processing precedes client optional-header processing.
    ///
    /// A `ConnectRequest` is answered here and **not** handed to the transport: the transport has
    /// no connection to put it on yet, and a duplicate must be "ignored entirely"
    /// (connect-request handling, step 1). Everything else goes to [`Net::feed`], whose rejections
    /// are silent — the client logs none of them and bumps its bad-packets-received counter.
    pub fn feed(&mut self, raw: &[u8], from: SocketAddr, now: LocalTime) {
        let Ok(parsed) = ParsedPacket::parse(raw) else {
            return;
        };

        if let Some(body) = parsed.optional.get(&PacketFlags::CONNECT_REQUEST) {
            if let Ok(cr) = ConnectRequest::from_bytes(body) {
                self.handle_connection_request(&parsed.header, cr, from, now);
            }
            return;
        }

        if self.trace {
            tracing::debug!(
                target: "dereth::trace::net",
                "recv seq={} flags={:#010X} len={} frags={} from={from}",
                parsed.header.seq_id,
                parsed.header.header.0,
                raw.len(),
                parsed.fragments.len()
            );
        }
        if self.session.transport.feed(raw, Some(from), now).is_err() {
            self.rejected = self.rejected.saturating_add(1);
            return;
        }

        // Read either error body only after transport validation accepts the packet. `NetError` (0x100000)
        // is legal before a connection; `NetErrorDisconnect` (0x200000) is sequenced/encrypted,
        // so a bad checksum or stale sequence must not mutate this wrapper first.
        if let Some(code) = parsed
            .optional
            .get(&PacketFlags::NET_ERROR)
            .and_then(|b| NetErrorCode::unpack(b))
            .filter(|code| *code != NetErrorCode::None)
        {
            // During optional-header processing, the live async login
            // context ends with that error. After it has ended, retail discards this body's
            // specific code, logs off the server, then raises `ServerDied`.
            if self.login_context_active() {
                self.fail_login(code);
            } else {
                self.log_off_server();
                self.raise_server_died();
            }
        }

        if let Some(code) = parsed
            .optional
            .get(&PacketFlags::NET_ERROR_DISCONNECT)
            .and_then(|b| NetErrorCode::unpack(b))
            .filter(|code| *code != NetErrorCode::None)
        {
            // The login recipient publishes status 4, whose link-status handler
            // keeps the final error and marks
            // the holder disconnected. Status 5 for a distinct current-world recipient has no
            // connection-status arm. The transport's DisconnectReceived state owns teardown in both
            // cases and must continue through its response/removal lifecycle.
            if parsed.header.rec_id == self.session.transport.logon_recipient().0 {
                self.fail_login(code);
            }
        }
        if self.status == LinkStatus::LoginConnecting
            && self
                .session
                .transport
                .connection_state(dereth_client_net::RecipientId(self.rec_id))
                == dereth_transport::conn::ConnectionState::Connected
        {
            // the first packet that is not a
            // `ConnectRequest` completes the login FSM — it ends with the no-error result =>
            // login connected.
            self.status = LinkStatus::Connected;
            self.cookie = 0;
            self.connected_pending = true;
            self.login_completed = true;
        }
    }

    /// Handle a connection request, following its ten steps in order.
    fn handle_connection_request(
        &mut self,
        header: &ProtoHeader,
        cr: ConnectRequest,
        from: SocketAddr,
        now: LocalTime,
    ) {
        // Step 1: an id already in use is replaced only by a strictly newer iteration.
        let existing = self.peer.is_some().then_some(self.iteration);
        if iteration_verdict(existing, header.iteration) == IterationVerdict::Ignore {
            return;
        }

        // Steps 3-6. `net_id` is what goes in the header's `rec_id` on everything we send; ACE
        // sets it to 0, which is why a rebuild that assumes it equals the received `rec_id` still
        // works locally and would not on retail.
        self.rec_id = header.rec_id;
        self.net_id = u16::try_from(cr.net_id & 0xFFFF).unwrap_or(0);
        self.iteration = header.iteration;
        self.session.transport.add_connection_at(
            self.rec_id,
            self.net_id,
            self.iteration,
            cr.outgoing_seed,
            cr.incoming_seed,
            Some(from),
            now,
        );

        // Step 7 -- the receiver's referral cookie is set from the request's cookie, the last store
        // connection-request handling makes before it changes the connection state.
        //
        // It has to go on the **receiver**, not in `self.cookie`: `feed` sets `self.cookie = 0`
        // the moment the link reaches `Connected`, and the one reader of the referral cookie is
        // connection processing's 140-second arm, which fires long after that.
        // Keeping it only in `self.cookie` forgets it exactly when the self-referral needs it.
        //
        // The reader and the whole referral machinery live in `dereth-client-net`; this line is
        // the other half. Without it the referral cookie is 0 for the life of every connection
        // and the automatic world re-connect never works.
        self.session
            .transport
            .set_referral_cookie(dereth_client_net::RecipientId(self.rec_id), cr.cookie);

        // Packet processing's final timestamp update, for the packet that just built this
        // receiver.
        //
        // ```text
        // process the optional headers                     <- we are here
        // if the packet has a receiver:
        //     record the current local time as the last-data time
        //     process the blob fragments
        // ```
        //
        // Receiver initialization does **not** initialize the last-data time; the
        // slot keeps whatever the zeroed receiver array held. Retail survives that only
        // because connection-request handling runs *inside* optional-header processing, so the
        // stamp three statements later lands on the receiver the ConnectRequest just created.
        //
        // This wrapper answers a `ConnectRequest` without handing it to [`dereth_client_net::Net::feed`],
        // which owns the rest of the stamp, so the stamp has to be made here. Without it the new
        // connection's silence clock reads 0.0 while the process clock is seconds since the
        // process started, and `Net::process_connections`'s
        // `140.0 < local_time - last-data time` arm fires on the very next pass for any login
        // made more than 140 s after start-up -- a patch screen, a retyped password, a second
        // character. The link goes disconnect-received -> disconnect-sent -> connection
        // removal before the first world datagram is read. On a five-login recording whose logins
        // begin at t = 0, 235, 1370, 1605 and 1995 s, the first survives and the other four die
        // three datagrams in, losing 883 of 885 server packets.
        if let Some(recv) = self
            .session
            .transport
            .receiver_mut(dereth_client_net::RecipientId(self.rec_id))
        {
            recv.local_time_last_got_data = now;
            // The `ConnectRequest` is the first datagram the new connection counts toward the
            // peer's interval: it starts the count, so the first `Flow` report covers it too.
            let _ = recv.account_datagram(
                header.interval,
                u32::from(header.datalen)
                    + u32::try_from(dereth_transport::wire::HEADER_SIZE).unwrap_or(0),
            );
        }

        // Step 8, and the port + 1 rule.
        self.cookie = cr.cookie;
        self.peer = Some(from);
        self.handshake_addr = Some(SocketAddr::new(from.ip(), handshake_port(from.port())));

        // Steps 9-10.
        self.status = LinkStatus::LoginConnecting;
        self.send_connect_ack(now);
    }

    /// Send the client network's connection acknowledgement.
    ///
    /// `seq_id = 0`, header flags `0x80000`, `rec_id = net_id`, `iteration = iteration`,
    /// `interval = 0`, an 8-byte body carrying the cookie, sent to the
    /// server's port **+ 1**. `ConnectResponse` is disposable, so the packet is not encrypted and
    /// [`OutPacket::serialize`] takes no key.
    fn send_connect_ack(&mut self, now: LocalTime) {
        let Some(to) = self.handshake_addr else {
            return;
        };
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 0,
            rec_id: self.net_id,
            interval: 0,
            iteration: self.iteration,
            ..Default::default()
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
            self.pending.push((bytes, to));
            self.last_connect_ack = Some(now);
        }
    }

    /// client version `"1802"` and the authenticator,
    /// with a hand-built header: everything zero but header flags `0x10000` and the data length.
    fn send_login_request(&mut self) {
        let mut p = OutPacket::new(ProtoHeader::default());
        if p.add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&self.auth))
            .is_err()
        {
            return;
        }
        if let Ok(bytes) = p.serialize(None) {
            self.pending.push((bytes, self.logon_addr));
        }
    }

    /// Run the login FSM and connect-ack resend, followed by the
    /// session's queue drain and the packet controller's send half.
    ///
    /// Compatibility coarse adapter. App calls the split receive and packet-controller phases
    /// with `local_time`, not simulated `cur_time`; semantic queue delivery is separate.
    pub fn tick(&mut self, now: LocalTime) {
        if !self.login_use_time(now) && !self.transport_disconnect_pending() {
            return;
        }
        self.session.transport.process_connections(now);
        self.consume_transport_server_died();
        self.session.tick(now);
        self.packet_controller_use_time(now);
    }

    /// Client-network receive/login phase without semantic queue delivery or packet sending.
    pub fn receive_use_time(&mut self, now: LocalTime) {
        if !self.login_use_time(now) && !self.transport_disconnect_pending() {
            return;
        }
        self.session.receive(now);
        self.session.network_use_time(now);
        self.session.transport.process_connections(now);
        self.consume_transport_server_died();
    }

    fn consume_transport_server_died(&mut self) {
        if self.session.transport.take_server_died() {
            self.raise_server_died();
        }
    }

    fn login_context_active(&self) -> bool {
        matches!(
            self.status,
            LinkStatus::Idle | LinkStatus::LoginAuthenticating | LinkStatus::LoginConnecting
        )
    }

    fn fail_login(&mut self, code: NetErrorCode) {
        self.error = Some(code);
        self.status = LinkStatus::Failed(code);
        link_status_holder::disconnected();
        // An error in the same receive phase wins over an unconsumed successful edge.
        self.connected_pending = false;
    }

    fn raise_server_died(&mut self) {
        self.status = LinkStatus::Disconnected;
        self.connected_pending = false;
        link_status_holder::disconnected();
        self.session.server_died();
    }

    fn transport_disconnect_pending(&self) -> bool {
        let transport = &self.session.transport;
        [transport.logon_recipient(), transport.world_recipient()]
            .into_iter()
            .any(|recipient| {
                matches!(
                    transport.connection_state(recipient),
                    dereth_transport::conn::ConnectionState::DisconnectReceived
                        | dereth_transport::conn::ConnectionState::DisconnectSent
                )
            })
    }

    fn login_use_time(&mut self, now: LocalTime) -> bool {
        match self.status {
            LinkStatus::Disconnected | LinkStatus::Failed(_) => return false,
            // The login tick runs while the logon recipient id is 0: immediately, then every
            // 2.0 s, for at most 20 attempts.
            LinkStatus::Idle | LinkStatus::LoginAuthenticating => match self.login.should_send(now)
            {
                Ok(true) => {
                    self.status = LinkStatus::LoginAuthenticating;
                    self.send_login_request();
                }
                Ok(false) => {}
                Err(code) => {
                    self.fail_login(code);
                    return false;
                }
            },
            // Connection processing re-sends the `ConnectResponse` every
            // 0.333333333 s while the state is `ConnectionState::ConnectionRequestAcked`, with no
            // attempt limit.
            LinkStatus::LoginConnecting => {
                let due = self
                    .last_connect_ack
                    .is_none_or(|t| now.seconds_since(t) >= HANDSHAKE_RESEND);
                if due {
                    self.send_connect_ack(now);
                }
            }
            LinkStatus::Connected => {}
        }

        true
    }

    /// Packet-controller phase. App calls this after queue4 and before database/UI-element
    /// work; actions queued by those later phases wait until the next invocation.
    pub fn packet_controller_use_time(&mut self, now: LocalTime) {
        // **The player system's two calls on the transport singleton.**
        //
        // World-exit disconnect and world entry are
        // made by the player system on the transport singleton directly; `dereth_client_net::client_session` reaches the
        // transport only through the two-method `Transport` seam, so it counts them and this owns
        // both objects and makes them.
        //
        // **This is the only site, and its position is the point.** Every datagram this build
        // sends is built by `Net::tick` on the line below, so a teardown applied here is applied
        // before any datagram that the blob it accompanies could ride on — which is the client's
        // ordering (exit-world disconnect, then enter-world request), even though
        // the blob was queued a few statements earlier. Moving this call after `Net::tick` would
        // reverse it on a split deployment, where the teardown drops the world connection and its
        // queued blobs.
        let (exits, enters) = self.session.take_transport_calls();
        for _ in 0..exits {
            self.session.transport.exit_world_disconnect();
        }
        for _ in 0..enters {
            self.session.transport.enter_world();
        }

        // build this frame's datagrams.
        self.session
            .transport
            .tick(now, Duration::from_millis(RECV_BUDGET_MS));
        // Each datagram now names its own destination, because the NAT keep-alive goes to the
        // server's port + 1 and everything else to the port the `ConnectRequest` came from. A
        // replay registers its connection without an address, so `None` falls back on the logon
        // address.
        for (bytes, to) in self.session.transport.take_outgoing() {
            if let Some(to) = to.or(self.peer) {
                self.pending.push((bytes, to));
            }
        }
    }

    /// Start the two-step enter-world exchange for the character
    /// the caller picked.
    pub fn enter_world(&mut self, character: ObjectId, account: &str) {
        self.session.enter_world(character, account);
    }

    /// Everything the session decoded since the last drain.
    ///
    /// The batch is handed straight over. Closing [`Session::object_arrived`] for each object
    /// (`0x0013` is parked on the player's) is [`crate::objects::ObjectStream::pump`]'s job,
    /// because that is where the object table is.
    pub fn drain_events(&mut self) -> Vec<SessionEvent> {
        self.session.drain_events().collect()
    }
}

/// `ClientNetwork` plus the real UDP socket.
#[derive(Debug)]
pub struct NetLink {
    pub net: ClientNetwork,
    socket: Option<NetSocket>,
}

impl NetLink {
    /// Bind the socket and start the login FSM.
    ///
    /// The interface is `0.0.0.0` unless `Net.BindInterface` says otherwise; that preference is
    /// [`dereth_client_net::socket::select_interface`] over an enumeration this build does not
    /// have, so the unfiltered default is used and the switch is not offered.
    ///
    /// # Errors
    /// [`ClientNetworkError`] for a bad `-h` or a socket that will not bind.
    pub fn connect(
        host: &str,
        default_port: u16,
        client_port: u16,
        account: &str,
        password: &str,
        connection_sequence_number: u32,
    ) -> Result<Self, ClientNetworkError> {
        let net = ClientNetwork::new(
            host,
            default_port,
            account,
            password,
            connection_sequence_number,
        )?;
        let socket = NetSocket::bind(Ipv4Addr::UNSPECIFIED, PortMode::Fixed(client_port))?;
        Ok(Self {
            net,
            socket: Some(socket),
        })
    }

    /// Explicit socket-free endpoint for an offline replay. It runs the same ClientNetwork/Session
    /// code but never binds, polls or sends an OS socket. Built datagrams remain in `net` until
    /// the replay inspects them; they are not counted as successful physical sends.
    pub fn replay(net: ClientNetwork) -> Self {
        Self { net, socket: None }
    }

    /// The bound local address, for the packet controller's recipient-registration log line.
    #[must_use]
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.socket.as_ref().map(|socket| socket.local_addr)
    }

    /// One frame: drain the socket within the 50 ms budget, run the FSM, then send.
    ///
    /// The socket is polled from the main loop with a 50 ms budget, because packet processing
    /// mutates game state that the rest of the frame reads without locking.
    pub fn tick(&mut self, now: LocalTime) {
        self.receive_socket(now, web_time::Instant::now());
        self.net.tick(now);
        self.flush_socket();
    }

    /// Production receive/login phase. The same path is used by socket-free replay endpoints.
    pub fn receive_use_time(&mut self, now: LocalTime) {
        self.receive_socket(now, web_time::Instant::now());
        self.net.receive_use_time(now);
    }

    /// Publish this frame's current time to the transport. Time-sync handling reads it
    /// before setting network time.
    ///
    /// The transport is the one that needs it (the speed-hack tail is inside the optional-header
    /// walk) and this crate is the only one that has a clock, so the frame hands it over the way
    /// retail hands it over: by publishing the global once, at the top of the client tick,
    /// before any datagram is processed.
    pub fn set_cur_time(&mut self, cur_time: f64) {
        self.net.session.transport.set_cur_time(cur_time);
    }

    pub fn packet_controller_use_time(&mut self, now: LocalTime) {
        self.net.packet_controller_use_time(now);
        self.flush_socket();
    }

    /// The budget's start is an explicit parameter rather than a second
    /// `Instant::now()` inside the loop's own function: this is a sub-frame deadline, not the
    /// frame's `Timer`, so it is the one clock read in the client that `App`'s
    /// [`crate::platform::clock::Clock`] deliberately does not own.
    fn receive_socket(&mut self, now: LocalTime, budget_start: web_time::Instant) {
        let deadline = budget_start + Duration::from_millis(RECV_BUDGET_MS);
        let mut buf = vec![0u8; RECV_BUFFER_SIZE];
        // `WSAEWOULDBLOCK` is the normal drain-complete signal, and `WSAECONNRESET` -- which
        // Windows raises on an ICMP port-unreachable for a previous `sendto` -- ends the loop just
        // as quietly: the network update returns false and nothing reads that return value.
        while let Some(Ok(Some((len, from)))) = self
            .socket
            .as_ref()
            .map(|socket| socket.recv_from(&mut buf))
        {
            self.net.feed(&buf[..len], from, now);
            if web_time::Instant::now() >= deadline {
                break;
            }
        }
    }

    fn flush_socket(&mut self) {
        if let Some(socket) = self.socket.as_ref() {
            for (bytes, to) in self.net.take_outgoing() {
                let _ = socket.send_to(&bytes, to);
            }
        }
    }

    /// The logoff path's first line builds and flushes the logoff datagram; the socket send has to
    /// happen **before** the socket is closed.
    ///
    /// There is no frame after this: `App::shutdown` runs once, the loop has
    /// already ended, and `Step::CleanupNet` drops this object. So the datagram is written to the
    /// socket here rather than queued for a `tick` that will never come. It returns
    /// **(datagrams actually written, datagrams the logoff built)** so the caller can report a
    /// send that failed instead of reporting `Ran` on a queue nobody drained.
    /// Queue the transport-level goodbye.
    ///
    /// Returns how many standalone `Disconnect` packets were queued, which is one
    /// per entry in the transport's connection list; a client that never connected queues none, and that is a
    /// number rather than a silence. `NetLink::log_off_server` is what puts them on the wire.
    pub fn log_off_server(&mut self) -> (usize, usize) {
        let queued = self.net.log_off_server();
        let Some(socket) = self.socket.as_ref() else {
            return (0, queued);
        };
        let mut sent = 0usize;
        for (bytes, to) in self.net.take_outgoing() {
            // `send_to` answers `Ok(false)` for a short or would-block write, which is not a send;
            // only a whole datagram counts, so the pair below cannot read "1 of 1" on a goodbye
            // the OS refused.
            //
            // **Untested in this workspace, and kept.** Counting every attempt instead would pass
            // every suite: nothing here owns a socket that refuses a 20-byte datagram to a bound
            // loopback peer, and `tests/gpu/presentation/shutdown.rs` runs with no link at all. The line stays
            // because the point is that a step must not report `Ran` on something it has not
            // done.
            if matches!(socket.send_to(&bytes, to), Ok(true)) {
                sent += 1;
            }
        }
        (sent, queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `ConnectResponse` goes to the server's port **+ 1** while everything else goes to the
    /// port the server answered from.
    ///
    /// The destination is the receiver's stored address with its port incremented by one; the
    /// test also pins the `ConnectResponse` header fields.
    #[test]
    fn the_connect_response_goes_to_port_plus_one() {
        let mut c = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0).expect("host");
        assert_eq!(c.logon_addr(), "127.0.0.1:19000".parse().unwrap());

        // The first tick sends a LoginRequest to P, immediately.
        c.tick(LocalTime(0.0));
        let out = c.take_outgoing();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, "127.0.0.1:19000".parse::<SocketAddr>().unwrap());
        let p = ParsedPacket::parse(&out[0].0).expect("LoginRequest parses");
        assert!(p.header.header.contains(PacketFlags::LOGIN_REQUEST));
        assert_eq!(p.header.seq_id, 0);
        assert_eq!(p.header.rec_id, 0);
        assert_eq!(p.header.iteration, 0);

        // The server answers from P with a ConnectRequest; the response goes to P + 1.
        let cr = ConnectRequest {
            server_time: 1234.5,
            cookie: 0xCB85_1E1F_0244_2EED,
            net_id: 0,
            outgoing_seed: 0x6A98_D361,
            incoming_seed: 0x1C08_CCF5,
        };
        let mut req = OutPacket::new(ProtoHeader {
            seq_id: 0,
            rec_id: 0x0B,
            interval: 0x0440,
            iteration: 1,
            ..Default::default()
        });
        req.add_optional_header(PacketFlags::CONNECT_REQUEST, cr.to_bytes().to_vec())
            .expect("section");
        let bytes = req.serialize(None).expect("serialize");
        c.feed(&bytes, "127.0.0.1:19000".parse().unwrap(), LocalTime(0.1));

        assert_eq!(c.status(), LinkStatus::LoginConnecting);
        assert_eq!(c.handshake_addr(), Some("127.0.0.1:19001".parse().unwrap()));
        let out = c.take_outgoing();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, "127.0.0.1:19001".parse::<SocketAddr>().unwrap());
        let ack = ParsedPacket::parse(&out[0].0).expect("ConnectResponse parses");
        assert!(ack.header.header.contains(PacketFlags::CONNECT_RESPONSE));
        assert_eq!(ack.header.seq_id, 0);
        assert_eq!(
            ack.header.rec_id, 0,
            "the record identifier is the receiver's network identifier"
        );
        assert_eq!(ack.header.interval, 0);
        assert_eq!(ack.header.iteration, 1);
        assert_eq!(
            ack.optional
                .get(&PacketFlags::CONNECT_RESPONSE)
                .map(Vec::as_slice),
            Some(&cr.cookie.to_le_bytes()[..]),
            "the cookie is echoed verbatim"
        );
    }

    /// A duplicate `ConnectRequest` at the same iteration is ignored entirely; a newer one is not.
    ///
    /// Oracle: client connection-request handling step 1, via
    /// [`dereth_transport::conn::iteration_verdict`].
    #[test]
    fn a_stale_connect_request_is_ignored_and_a_newer_one_rebuilds() {
        let mut c = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0).expect("host");
        let cr = ConnectRequest {
            server_time: 0.0,
            cookie: 7,
            net_id: 0,
            outgoing_seed: 1,
            incoming_seed: 2,
        };
        let build = |iteration: u16| {
            let mut p = OutPacket::new(ProtoHeader {
                rec_id: 0x0B,
                iteration,
                ..Default::default()
            });
            p.add_optional_header(PacketFlags::CONNECT_REQUEST, cr.to_bytes().to_vec())
                .expect("section");
            p.serialize(None).expect("serialize")
        };
        let from: SocketAddr = "127.0.0.1:19000".parse().unwrap();

        c.feed(&build(1), from, LocalTime(0.0));
        assert_eq!(c.take_outgoing().len(), 1, "the first is answered");
        c.feed(&build(1), from, LocalTime(0.5));
        assert!(
            c.take_outgoing().is_empty(),
            "the duplicate is ignored entirely"
        );
        c.feed(&build(2), from, LocalTime(1.0));
        assert_eq!(c.take_outgoing().len(), 1, "a newer iteration rebuilds");
    }

    /// Twenty unanswered login requests time out.
    #[test]
    fn twenty_unanswered_login_requests_time_out() {
        let mut c = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0).expect("host");
        let mut sent = 0;
        // 0.0, 2.0, ... 40.0 is 21 ticks; the twenty-first attempt is the one that fails.
        for i in 0..=20 {
            c.tick(LocalTime(f64::from(i) * 2.0));
            sent += c.take_outgoing().len();
        }
        assert_eq!(sent, 20, "twenty attempts, then no more");
        assert_eq!(
            c.status(),
            LinkStatus::Failed(NetErrorCode::ClientTimedOutServer)
        );
    }

    /// The sessions transport calls reach the transport.
    #[test]
    fn the_sessions_transport_calls_reach_the_transport() {
        let mut c = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", 0).expect("host");

        // Station 1 — world entry, which phase 2 requests.
        c.session.transport.enter_world();
        assert!(
            c.session.transport.in_game(),
            "world entry sets the in-game flag"
        );

        // Character-logon phase 1 raises `ExitWorldDisconnect` before
        // its `0xF7C8`; the session counts it and the network tick makes the call.
        c.session
            .enter_world(dereth_primitives::ObjectId(0x5000_0001), "ac01");
        c.tick(LocalTime(1.0));

        // Station 2.
        assert!(
            !c.session.transport.in_game(),
            "the world-exit disconnect reached the transport"
        );
        assert_eq!(
            c.session.take_transport_calls(),
            (0, 0),
            "and the queue was drained rather than accumulating"
        );
    }
}
