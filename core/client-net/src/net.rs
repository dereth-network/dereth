//! `Net` — the crate's front door and the `Transport` implementation.
//!
//! `Transport` is the whole of what `dereth-protocol` and the client session (`client_session`) are
//! allowed to know about this module: two methods, no async, no runtime. A message handler is therefore a pure function of its
//! input and can be tested against a recorded capture with no I/O at all.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use dereth_primitives::LocalTime;
use dereth_primitives::NetBlobId as SeamNetBlobId;
use dereth_primitives::{IncomingMessage, NetQueue, Transport};

use crate::queues::{NetQueues, Queue, SEND_PRIORITY};
use crate::socket::{PortMode, DEFAULT_SERVER_PORT};
use dereth_transport::blob::{NetBlob, NetBlobId};
use dereth_transport::conn::ConnectionState;
use dereth_transport::flow::FlowQueue;
use dereth_transport::indicator::Indicator;
use dereth_transport::session::{ReceiverData, RejectReason};
use dereth_transport::wire::ParsedPacket;

/// One datagram the transport wants sent, with the address it must go to.
///
/// `None` means the connection has no recorded address — a replay — and the caller decides.
pub type Datagram = (Vec<u8>, Option<SocketAddr>);

/// The recipient id: an index into the 256-entry receiver table. The same value the transport
/// interface stamps on each incoming message as its sender.
pub use dereth_primitives::RecipientId;

/// Configuration, from the command line and the `Net.*` preferences.
#[derive(Debug, Clone)]
pub struct NetConfig {
    /// `-h host[:port]`, with the client's `,` and `:` handling.
    pub host: String,
    /// Default `0x1C88` = **7304**, not 9000.
    pub port: u16,
    /// `-o`/`-outport`, or `Net.UserSpecifiedPort`, or 0 to let the OS pick.
    pub client_port: u16,
    /// `Net.BindInterface`: `"A.B.C.D/M"`, a dotted-quad mask or a CIDR length.
    pub bind_interface: Option<String>,
    /// `Net.ComputeUniquePort`.
    pub compute_unique_port: bool,
    /// The logon version string: `"1802"` unless the world's server wants another. ACE compares
    /// it exactly.
    pub client_version: String,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_SERVER_PORT,
            client_port: 0,
            bind_interface: None,
            compute_unique_port: false,
            client_version: dereth_transport::conn::CLIENT_VERSION.to_owned(),
        }
    }
}

impl NetConfig {
    /// How the local socket should choose its port.
    #[must_use]
    pub fn port_mode(&self) -> PortMode {
        if self.compute_unique_port {
            PortMode::ComputeUnique
        } else {
            PortMode::Fixed(self.client_port)
        }
    }
}

/// One peer: its receive state, its send queue, its reassembler and where it lives.
#[derive(Debug)]
pub struct Connection {
    pub receiver: ReceiverData,
    pub flow: FlowQueue,
    pub indicator: Indicator,
    pub state: ConnectionState,
    pub addr: Option<SocketAddr>,
    /// Local time at the last heartbeat, used by the two-second connection-processing gate.
    pub local_time_last_heartbeat: f64,
    /// The highest sequence the peer has told us it holds, waiting to
    /// be applied to the retransmit cache.
    ///
    /// It is a *pending* value, not a running high-water mark
    /// applies it and zeroes it, and `0` means "nothing to do" rather than "sequence zero", which
    /// is why sequence numbers skip 0 on wrap. Two things write it, both in [`Net::feed`]:
    /// From an `AckSequence`, and from a
    /// `RequestRetransmit`'s **first** id — a NAK for 40 says everything below 40 arrived.
    pub flush_num: u32,
    /// Time of the last connection-state change, in the network-local clock domain.
    pub local_time_last_connection_state_changed: LocalTime,
    /// The `NetErrorDisconnect(None)` queued after draining older work.
    disconnect_response_queued: bool,
}

/// The transport.
///
/// The two recipient ids matter and are not interchangeable: queues 4, 5 and 8 go to the **login**
/// server and everything else to the world server. Against a single-process server such as ACE the
/// two ids are identical, so a rebuild that hard-codes one passes every local test and breaks on a
/// retail-style split deployment.
#[derive(Debug)]
pub struct Net {
    pub config: NetConfig,
    connections: BTreeMap<u16, Connection>,
    queues: NetQueues,
    /// Current world-server recipient id.
    world_recipient: RecipientId,
    /// Current logon-server recipient id.
    logon_recipient: RecipientId,
    /// A plain 16-bit counter incremented per blob.
    ///
    /// Starts at [`FIRST_BLOB_COUNTER`], not at 0. See that constant.
    unordered_stamp: u16,
    /// Non-ephemeral blob id, initialized to `make_initial_sequence_id(0)` = 0 and
    /// then advanced once before the first blob that reaches the wire. See
    /// [`FIRST_BLOB_COUNTER`].
    cur_non_ephemeral_id: NetBlobId,
    /// Datagrams built and waiting for the socket, each with where it must go. Exposed so a
    /// replay can drain them.
    pending_out: Vec<Datagram>,
    /// The last TimeSync value the server sent. The caller **hard-sets** its game clock to this.
    last_time_sync: Option<f64>,
    /// Current game time, read **before** the client applies the server's update.
    ///
    /// The clock itself lives in `dereth_client_runtime::platform::clock::Timer`; this crate cannot see it, so the
    /// frame's published value is pushed in with [`Net::set_cur_time`] the way retail publishes
    /// the global once per frame. Left at `0.0` by a caller that never
    /// pushes (every replay and every in-crate fixture), which is the "never synchronised" reading
    /// and can only ever *clear* the speed-hack latch below.
    cur_time: f64,
    /// Client speed-check time — a `double`, `0.0` for
    /// "not currently suspected".
    ///
    /// See [`Net::speed_hack_detection_time`] and the transcription in [`Net::feed`].
    time_client_speed_hack_detection: f64,
    /// Whether log-off has been sent.
    ///
    /// After the goodbye, the transport send returns 0 without calling `sendto` -- the client goes
    /// **silent**, which is why
    /// the Disconnect is the last datagram in every recorded session and nothing follows it.
    log_off_sent: bool,
    /// Whether the client is currently in game.
    ///
    /// Set by [`Net::enter_world`], whose whole body assigns true, and cleared by
    /// [`Net::exit_world_disconnect`]. The client reads
    /// it in exactly one place, the server-switch handler's logon-switch arm,
    /// which moves `world_recipient` only when the client is **not** in game.
    in_game: bool,
    /// Counters accumulating since the last heartbeat. Packet processing
    /// bumps them, and the connection walk zeroes them after each snapshot.
    current_link_status: crate::linkstatus::Snapshot,
    /// Link-status averages — the forty-sample rings the packet-loss
    /// figure is divided out of.
    link_status: crate::linkstatus::LinkStatusAverages,
    /// Whether a heartbeat notification fired and no one has asked yet — see
    /// [`Net::take_heartbeat`].
    heartbeat_pending: bool,
    /// Last shared-network time update, in the game/client clock domain.
    last_did_use_time: f64,
    /// Removing the connection raised the server-died login notice.
    server_died_pending: bool,
    /// Referral queue: a growable array of pending world logins.
    referral_queue: Vec<ReferralQueueEntry>,
    /// World-server switch history.
    world_switch_history: dereth_transport::conn::SwitchHistory,
    /// Logon-server switch history.
    logon_switch_history: dereth_transport::conn::SwitchHistory,
    /// The referral-queue walk gave up on a referral and notified the plugins of
    /// the world-connection-error status (5). See [`Net::take_world_connection_error`].
    world_connection_error_pending: bool,
}

/// One pending world login, 40 bytes in the original.
///
/// The layout is read straight off the referral handler's stores (the struct
/// is built on the stack and appended to the queue), and the 40-byte stride is confirmed twice
/// over: the handler's own duplicate scan advances by 0x28, and the queue walk multiplies by it.
///
/// ```text
/// u32          auths sent
/// u16          server id
/// double       next world-auth send time  ; both halves zeroed
/// sockaddr_in  server address             ; four dwords
/// u64          cookie
/// ```
///
/// The next send time starting at **0.0** is why the first request goes out
/// immediately: the gate is `entry.t < local_time`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferralQueueEntry {
    /// How many `WorldLoginRequest`s have gone to this endpoint.
    pub n_auths_sent: u32,
    /// The server id, copied from the `Referral` section.
    pub id_server: u16,
    /// When the next world-auth request goes out, in the network-local clock domain.
    pub localtime_to_send_next_world_auth: LocalTime,
    /// Where the `WorldLoginRequest` goes, verbatim and **not** port + 1.
    pub server_addr: SocketAddr,
    /// The cookie -- the eight bytes of the `WorldLoginRequest` body.
    pub cookie: u64,
}

/// The value both send-side blob counters hold when the **first blob to reach the wire** is
/// stamped: sequence 1 and ordering stamp 1, never 0.
///
/// # Why this is 1 and not 0
///
/// The packet controller starts its non-ephemeral id at 0, the unordered stamp starts at zero, and
/// the getter **post**-increments — so the code alone predicts a first blob with sequence 0 and
/// stamp 0. The wire says otherwise. In all
/// six recorded retail sessions (five of which reach a blob) the
/// client's first fragment is
///
/// ```text
/// blob id low = 1, blob id high = 0x23000001   (ordering type 0x23, stamp 1, sequence 1)
/// ```
///
/// and the second is `2 / 0x23000002`. Both counters are advanced by exactly one before the first
/// blob is transmitted, which is what a single earlier failed send would do: the id and the stamp
/// are consumed at the top of the blob send,
/// before the packet send is called, and a refused blob takes them with it.
///
/// This is not cosmetic and it is not an optimisation. The server's ordered-fragment handler
/// starts `lastReceivedFragmentSequence` at 0 and processes a fragment only
/// when its sequence equals `lastReceivedFragmentSequence + 1`; anything else goes into
/// `outOfOrderFragments`, which is only ever drained by looking for that same `+ 1`. A first
/// fragment numbered 0 is therefore parked forever, silently: the packet is accepted, the cumulative
/// ACK advances, and the message is never dispatched. A live `0xF7C8` sent that way is
/// acknowledged and never answered with `0xF7DF Login_EnterGame_ServerReady`, so the client sits
/// in `EnteringWorld` for ever.
///
// *which* client blob consumes id 0 and stamp 0. It is deduced from the captures,
// not observed; no protocol send caller is obviously the place, and settling it needs a
// breakpoint in the retail client. The observable behaviour — the counters' value at the first
// transmitted blob — is verified six times over and is what this models.
const FIRST_BLOB_COUNTER: u32 = 1;

/// **Sixty seconds.** The speed-hack test uses the same constant for both halves of its window;
/// retail uses the same constant twice.
///
/// It is a *window*, not a tolerance on a running skew — see the transcription in [`Net::feed`].
const SPEED_HACK_WINDOW: f64 = 60.0;

impl Net {
    #[must_use]
    pub fn new(config: NetConfig) -> Self {
        Self {
            config,
            connections: BTreeMap::new(),
            queues: NetQueues::with_retail_registrations(),
            world_recipient: RecipientId(0),
            logon_recipient: RecipientId(0),
            unordered_stamp: u16::try_from(FIRST_BLOB_COUNTER).unwrap_or(1),
            cur_non_ephemeral_id: NetBlobId(
                NetBlobId::make_initial_sequence_id(0).0 | u64::from(FIRST_BLOB_COUNTER),
            ),
            pending_out: Vec::new(),
            log_off_sent: false,
            last_time_sync: None,
            cur_time: 0.0,
            time_client_speed_hack_detection: 0.0,
            in_game: false,
            current_link_status: crate::linkstatus::Snapshot::default(),
            link_status: crate::linkstatus::LinkStatusAverages::default(),
            heartbeat_pending: false,
            last_did_use_time: 0.0,
            server_died_pending: false,
            referral_queue: Vec::new(),
            world_switch_history: dereth_transport::conn::SwitchHistory::default(),
            logon_switch_history: dereth_transport::conn::SwitchHistory::default(),
            world_connection_error_pending: false,
        }
    }

    /// Register a peer, as step 3 does from the
    /// `ConnectRequest`'s two clear-text seeds.
    ///
    /// The **first** connection is the login server: step 6 sets both `logon_recipient` and
    /// `world_recipient` to it.
    pub fn add_connection(
        &mut self,
        rec_id: u16,
        net_id: u16,
        iteration: u16,
        outgoing_seed: u32,
        incoming_seed: u32,
        addr: Option<SocketAddr>,
    ) {
        self.add_connection_at(
            rec_id,
            net_id,
            iteration,
            outgoing_seed,
            incoming_seed,
            addr,
            LocalTime::default(),
        );
    }

    /// [`Self::add_connection`] at the actual network time.
    #[allow(clippy::too_many_arguments)] // the connection's identity, its two seeds and the clock
    pub fn add_connection_at(
        &mut self,
        rec_id: u16,
        net_id: u16,
        iteration: u16,
        outgoing_seed: u32,
        incoming_seed: u32,
        addr: Option<SocketAddr>,
        now: LocalTime,
    ) {
        let mut receiver = ReceiverData::new(rec_id, outgoing_seed, incoming_seed);
        receiver.net_id = net_id;
        receiver.iteration = iteration;
        receiver.addr = addr.map(|a| a.ip());
        // Receiver initialization sets `highest_id_received = 1`.
        receiver.window.highest_id_received = 1;
        self.connections.insert(
            rec_id,
            Connection {
                receiver,
                flow: FlowQueue::new(net_id, iteration),
                indicator: Indicator::new(),
                state: ConnectionState::ConnectionRequestAcked,
                addr,
                local_time_last_heartbeat: now.0,
                flush_num: 0,
                local_time_last_connection_state_changed: now,
                disconnect_response_queued: false,
            },
        );
        if self.logon_recipient.0 == 0 {
            self.logon_recipient = RecipientId(rec_id);
            self.world_recipient = RecipientId(rec_id);
        }

        // the connection-request handler's step between the logon recipient id and the cookie
        // store: walk the referral queue and drop the **first** entry whose server id equals this
        // packet's recipient id, then `break`. That is what stops the 0.333 s `WorldLoginRequest`
        // cadence -- the referred server has answered, so the referral is spent. Without it the
        // client would keep hammering the world server it is already connected to for 280
        // seconds.
        if let Some(i) = self
            .referral_queue
            .iter()
            .position(|e| e.id_server == rec_id)
        {
            self.referral_queue.swap_remove(i);
        }
    }

    /// The pending world-login referral queue.
    #[must_use]
    pub fn referral_queue(&self) -> &[ReferralQueueEntry] {
        &self.referral_queue
    }

    /// Referral cookie -- the cookie this connection
    /// would re-present if it had to be re-established.
    ///
    /// Two writers in the client, and **this crate owns neither of them outright**:
    ///
    /// * the connection-request handler's tail, which copies
    ///   the `ConnectRequest`'s own cookie into the receiver. The request is
    ///   parsed and answered in `dereth_client_runtime::net::NetLink::handle_connection_request`, so that
    ///   caller must make this call; see [`Net::set_referral_cookie`].
    /// * [`Net::handle_referral`]'s "the slot is already live" arm, which is in this crate.
    ///
    /// The one reader is [`Net::process_connections`]'s 140-second arm.
    #[must_use]
    pub fn referral_cookie(&self, r: RecipientId) -> u64 {
        self.connections
            .get(&r.0)
            .map_or(0, |c| c.receiver.referral_cookie)
    }

    /// See [`Net::referral_cookie`]. The handler's store, as a call the
    /// `ConnectRequest`'s handler makes.
    pub fn set_referral_cookie(&mut self, r: RecipientId, cookie: u64) {
        if let Some(c) = self.connections.get_mut(&r.0) {
            c.receiver.referral_cookie = cookie;
        }
    }

    /// One world-connection-error plugin-status edge with two zero arguments, raised by
    /// [`Net::process_referral_queue`] giving up on a referral.
    ///
    /// The give-up arm pushes the world-connection-error status **5** with two zero arguments. It
    /// is a plugin notification and nothing else -- retail raises
    /// no `NetError`, shows no dialog and leaves every other connection alone, so a client that
    /// ignores this edge behaves exactly as retail does. Exposed so a future UI notice has
    /// something to hang on.
    pub fn take_world_connection_error(&mut self) -> bool {
        std::mem::take(&mut self.world_connection_error_pending)
    }

    /// The referral handler -- "go and log in to *that* world server".
    ///
    /// ```text
    /// server = referral.server_id
    /// if (server >= 0x100) return; // the receiver table is 256 wide
    /// receiver = receivers[server]; // a flat array; never null
    /// if (receiver.state > Connected) // > 5
    ///               remove receiver's connection;
    /// if (receiver.recipient_id != 0) { // the slot is live
    ///               receiver.referral_cookie = referral.cookie; // refresh the cookie
    ///               return;
    ///           }
    /// for each queued referral
    ///               if its server id equals server, return; // already queued
    /// entry = { auths_sent = 0, server_id = server, next_auth_time = 0.0,
    ///                     address = referral.address, cookie = referral.cookie };
    ///           append entry to the referral queue;
    /// process the referral queue; // the first request goes now
    /// ```
    ///
    /// Two things are worth naming because they look like bugs and are not.
    ///
    /// **A zero recipient id means "slot free", so id 0 can never be the *live* arm.** The
    /// client uses the same sentinel in header verification: when the receiver id equals
    /// zero, only an unsequenced packet is legal. [`dereth_transport::session::ReceiverData`] already
    /// models it. A referral naming server 0 therefore always takes the queue arm.
    ///
    /// **The `> Connected` test is a *disposal*, not a refusal.** A referral for a connection
    /// that is already tearing down (`DisconnectReceived` or `DisconnectSent`) removes it
    /// first, which frees the slot and lets the same call queue the new login -- that is the whole
    /// re-connect path, and it is how [`Net::process_connections`]'s 140-second arm works.
    pub fn handle_referral(&mut self, referral: &dereth_transport::conn::Referral, now: LocalTime) {
        let id = referral.id_server;
        if id >= 0x100 {
            return;
        }
        if self.connection_state(RecipientId(id)) > ConnectionState::Connected
            && self.remove_connection(RecipientId(id))
        {
            // Removing a connection raises the server-died login notice
            // unconditionally on its server log-off branch, and [`Net::remove_connection`]'s
            // contract is that a `true` return means *the caller owes that notice*. Ignoring the
            // return here would let a self-referral that removed the login or current server log
            // the client off and tell nobody -- the client would sit on a live screen with a
            // dead, permanently silent transport.
            //
            // This is the ordinary path, not an edge: the connection walk sets the
            // state to `DisconnectReceived` **before** it tests the cookie, so the referral
            // this arm hands to the referral handler always finds state 6 and always removes.
            self.server_died_pending = true;
        }
        if self
            .connections
            .get(&id)
            .is_some_and(|c| c.receiver.rec_id != 0)
        {
            self.set_referral_cookie(RecipientId(id), referral.cookie);
            return;
        }
        if self.referral_queue.iter().any(|e| e.id_server == id) {
            return;
        }
        self.referral_queue.push(ReferralQueueEntry {
            n_auths_sent: 0,
            id_server: id,
            localtime_to_send_next_world_auth: LocalTime(0.0),
            server_addr: SocketAddr::V4(referral.addr),
            cookie: referral.cookie,
        });
        self.process_referral_queue(now);
    }

    /// The referral-queue walk -- the `WorldLoginRequest` resend cadence.
    ///
    /// The whole walk: an empty queue returns at once. Local time `t` is read once, before the
    /// walk, which runs from the last entry down to the first. An entry whose next-send time is
    /// not before `t` is skipped. An entry whose send count has reached `280.0 / 0.333333333` is
    /// swap-removed (the last entry moves into its place) and the plugins are notified of
    /// the world-connection-error status (5). Otherwise the count is incremented, the next-send
    /// time set to `t + 0.333333333`, and an optional header built with mask `0x20000`, flags 7,
    /// length 8 and the entry's cookie as data. The inlined optional-header send gate requires
    /// DISPOSABLE|EXCLUSIVE|PRE_CONNECTION and neither NO_STANDALONE nor TIME_SENSITIVE; 7 passes.
    /// A send packet is created with that one header, a zeroed header is given the header's mask
    /// as flags, the packet's size and the packet's checksum, and the packet is sent to the
    /// entry's server address.
    ///
    /// Four details that a paraphrase loses:
    ///
    /// * the walk is **downwards**, and a removal swaps the last entry into the slot just vacated,
    ///   so the swapped-in entry is *not* re-examined this pass -- `Vec::swap_remove` is the same
    ///   operation;
    /// * local time is sampled **once**, before the loop, so every entry
    ///   that becomes due in one pass is timed from the same instant;
    /// * the packet goes to the entry's server address **verbatim**. This is not the `ConnectResponse`,
    ///   so [`dereth_transport::conn::handshake_port`]'s port + 1 rule does **not** apply here;
    /// * `rec_id`, `iteration`, `interval` and `seq_id` are all hard zeroes, because the
    ///   `ProtoHeader` starts zeroed and only `header`, `checksum` and `datalen` are
    ///   written into it. There is no connection yet to take an id from.
    ///
    /// The client's own connection walk calls it before per-connection processing.
    pub fn process_referral_queue(&mut self, now: LocalTime) {
        let mut i = self.referral_queue.len();
        while i > 0 {
            i -= 1;
            if self.referral_queue[i].localtime_to_send_next_world_auth.0 >= now.0 {
                continue;
            }
            if dereth_transport::flow::world_login_cap()
                <= f64::from(self.referral_queue[i].n_auths_sent)
            {
                self.referral_queue.swap_remove(i);
                self.world_connection_error_pending = true;
                continue;
            }
            let entry = &mut self.referral_queue[i];
            entry.n_auths_sent += 1;
            entry.localtime_to_send_next_world_auth =
                LocalTime(now.0 + dereth_transport::flow::WORLD_LOGIN_RESEND);
            let (cookie, to) = (entry.cookie, entry.server_addr);
            // Sending reaches the wire through the virtual buffer send, and
            // the send returns 0 once log-off has been sent. The counter and
            // the timer have already advanced -- retail drops the datagram, it does not skip the
            // attempt.
            if self.log_off_sent {
                continue;
            }
            if let Some(bytes) = build_world_login_request(cookie) {
                self.pending_out.push((bytes, Some(to)));
            }
        }
    }

    /// The server-switch handler -- move `world_recipient` / `logon_recipient` to
    /// the connection this packet arrived on.
    ///
    /// The switch type picks the world or the logon history. A history that has switched before
    /// returns unless the stamp is `lhs_newer` than its last stamp; otherwise the history is marked
    /// switched and takes the stamp. A world switch sets `world_recipient` to the packet's
    /// recipient id and takes the receiver's `net_id`; a logon switch sets `logon_recipient` to
    /// it, and `world_recipient` too when `in_game` is clear.
    ///
    /// The pictured source is a single connection-wide id this build does not have -- every
    /// [`Connection`] carries its own network id and the packet writer reads that one,
    /// which is the same value the assignment would copy. Named rather than invented, as
    /// [`Net::exit_world_disconnect`] already names it.
    pub fn handle_server_switch(
        &mut self,
        switch: &dereth_transport::conn::ServerSwitch,
        rec_id: u16,
    ) {
        use dereth_transport::conn::ServerSwitchType;
        let history = match switch.switch_type {
            ServerSwitchType::WorldSwitch => &mut self.world_switch_history,
            ServerSwitchType::LogonSwitch => &mut self.logon_switch_history,
        };
        if !history.accept(switch.seq_no) {
            return;
        }
        match switch.switch_type {
            ServerSwitchType::WorldSwitch => self.world_recipient = RecipientId(rec_id),
            ServerSwitchType::LogonSwitch => {
                if !self.in_game {
                    self.world_recipient = RecipientId(rec_id);
                }
                self.logon_recipient = RecipientId(rec_id);
            }
        }
    }

    /// World-server switch history.
    #[must_use]
    pub fn world_switch_history(&self) -> dereth_transport::conn::SwitchHistory {
        self.world_switch_history
    }

    /// Logon-server switch history.
    #[must_use]
    pub fn logon_switch_history(&self) -> dereth_transport::conn::SwitchHistory {
        self.logon_switch_history
    }

    /// Current world-server recipient id.
    #[must_use]
    pub fn world_recipient(&self) -> RecipientId {
        self.world_recipient
    }

    /// Current logon-server recipient id.
    #[must_use]
    pub fn logon_recipient(&self) -> RecipientId {
        self.logon_recipient
    }

    /// One peer's `ReceiverData`, or `None` when nothing is registered on that recipient id.
    ///
    /// Receiver state indexed by recipient id. A test of the exit-world teardown needs to read the
    /// NAK state across the teardown edge; a reading taken only after it cannot tell a reset from a
    /// value that was never set.
    #[must_use]
    pub fn receiver(&self, r: RecipientId) -> Option<&ReceiverData> {
        self.connections.get(&r.0).map(|c| &c.receiver)
    }

    /// [`Net::receiver`], mutably.
    pub fn receiver_mut(&mut self, r: RecipientId) -> Option<&mut ReceiverData> {
        self.connections.get_mut(&r.0).map(|c| &mut c.receiver)
    }

    #[must_use]
    pub fn connection_state(&self, r: RecipientId) -> ConnectionState {
        self.connections
            .get(&r.0)
            .map_or(ConnectionState::Disconnected, |c| c.state)
    }

    /// The server's game time from the last TimeSync.
    ///
    /// The time-sync handler hands this straight to the timer's clock set, and the
    /// clock it sets drives physics, animation, spell durations, the map panel's date and the
    /// day/night cycle. TimeSync is the master clock, not a hint.
    ///
    /// **It is not an unconditional hard-set.** `set_time` is gated `if (external_time < S)`, so it
    /// only ever moves the absolute time *forward*; a stale or duplicate sync is accepted off the
    /// wire and then discarded by the clock. Past the gate, the correction is `external_offset =
    /// S - elapsed_time` if the server is ahead, and a rewind of `elapsed_time` if the local
    /// timer overshot, with a `1e-09` dead band on each. The consumer is
    /// `dereth_client_runtime::platform::clock::Timer::set_time`, which transcribes all of it.
    pub fn take_time_sync(&mut self) -> Option<f64> {
        self.last_time_sync.take()
    }

    /// Publish current game time for this frame, so the TimeSync speed-check tail can read the same
    /// global retail reads.
    ///
    /// Call it **before** the frame's datagrams are fed and **before** the TimeSync is applied
    /// from [`Net::take_time_sync`] — which is where retail's snapshot sits, because
    /// the timer publishes the global at the top of the client frame and TimeSync handling runs
    /// inside the optional-header walk that follows.
    pub fn set_cur_time(&mut self, cur_time: f64) {
        self.cur_time = cur_time;
    }

    /// `0.0` while nothing is suspected, otherwise the current game time at which the first
    /// out-of-tolerance TimeSync was seen.
    #[must_use]
    pub fn speed_hack_detection_time(&self) -> f64 {
        self.time_client_speed_hack_detection
    }

    /// Datagrams the transport wants sent, oldest first, **each with its destination**. Drained by
    /// the socket loop, or by a replay.
    ///
    /// The address is per datagram and not per connection because the NAT keep-alive
    /// (the ICMD command section, `docs/networking/01-packet-format.md` §3.15) goes to the server's
    /// **port + 1** while everything else on the same connection goes to the port the server
    /// answered from. It is `None` only when the connection was registered without an address,
    /// which is what a capture replay does; the caller then supplies its own.
    pub fn take_outgoing(&mut self) -> Vec<Datagram> {
        let out = std::mem::take(&mut self.pending_out);
        // Increment packets-sent by 1 and bytes-sent by `n`.
        // Counted here rather than at each `pending_out.push` because this is the send, and the
        // send counts it; a packet built and never drained never reached the wire.
        self.current_link_status.pkts_sent = self
            .current_link_status
            .pkts_sent
            .wrapping_add(u16::try_from(out.len()).unwrap_or(0));
        for (bytes, _) in &out {
            self.current_link_status.bytes_sent = self
                .current_link_status
                .bytes_sent
                .wrapping_add(u32::try_from(bytes.len()).unwrap_or(0));
        }
        out
    }

    /// The packet-loss accessor -- the cached average packet loss,
    /// as a **ratio**.
    ///
    /// See [`crate::linkstatus`] for why it is a ratio despite the accessor's name, and why the
    /// value before the first heartbeat is [`crate::linkstatus::INITIAL_PACKET_LOSS`] rather than
    /// this function's answer.
    #[must_use]
    pub fn average_packet_loss(&self) -> f64 {
        self.link_status.average_packet_loss()
    }

    /// The heartbeat notification reaches the link-status holder's own heartbeat,
    /// as a pull.
    ///
    /// `Some(loss)` on the first ask after a snapshot was added; `None` otherwise. The client
    /// pushes this into every plugin, and the link-status holder performs two stores -- the
    /// last-heard timestamp and the current average packet loss. This crate has no
    /// plugin list, and the holder lives in `dereth-client`, so the edge is pulled by the frame
    /// loop that owns both clocks.
    pub fn take_heartbeat(&mut self) -> Option<f64> {
        std::mem::take(&mut self.heartbeat_pending).then(|| self.link_status.average_packet_loss())
    }

    /// One edge raised by transport connection removal.
    pub fn take_server_died(&mut self) -> bool {
        std::mem::take(&mut self.server_died_pending)
    }

    /// Link-status averages for a test that wants to read the rings rather than the
    /// one number they divide into.
    #[must_use]
    pub fn link_status(&self) -> &crate::linkstatus::LinkStatusAverages {
        &self.link_status
    }

    /// Counters accumulated since the last heartbeat.
    #[must_use]
    pub fn current_link_status(&self) -> &crate::linkstatus::Snapshot {
        &self.current_link_status
    }

    /// The log-off send -- the client's transport-level goodbye.
    ///
    /// ```c
    /// header = { mask = 0x8000, flags = 3, payload length = 0 };
    /// for each connection
    ///     send the optional header to this receiver;
    /// mark logoff sent;
    /// clear the login recipient id;
    /// run exit-world teardown;
    /// ```
    ///
    /// The optional-header send builds it with sequence and interval zero, mask `0x8000`,
    /// and data length zero;
    /// the recipient and iteration come from the receiver. Flags `3` mean
    /// `EXCLUSIVE | DISPOSABLE` with `NO_STANDALONE` clear, so it passes
    /// the optional-header sender's five-way gate and it takes **no ISAAC key** -- a disposable-only
    /// packet is not encrypted (`docs/networking/01-packet-format.md` §2), which is why the checksum of this
    /// packet is a constant.
    ///
    /// This is the only sender of `PacketFlags::DISCONNECT`. `Session::log_off` is not a
    /// substitute: it sends `0xF653`, a different message on a different layer, and one the
    /// client does not even send from character select because no character is logged on.
    /// Without this datagram ACE sees no disconnect header and falls back on its 60-second
    /// timeout.
    ///
    /// The oracle is the corpus: **all seven** recorded retail sessions end with exactly one
    /// client-sent mask-`0x00008000` datagram of 20 bytes and nothing after it, and
    /// `login-account-booted` is the clearest case -- LoginRequest, ConnectResponse, one
    /// server blob, two ACK packets, then the Disconnect at t = 5.726 s with **no `0xF653`, no
    /// `0xF657` and no prelude of any kind**. The 20 bytes are byte-identical across
    /// `short-second-connection`, `login-account-booted` and `ddd-interrogation-only`.
    ///
    /// This function's last statement is the exit-world teardown, [`Net::exit_world_disconnect`].
    /// This caller drops the whole transport immediately afterwards, but no other does: the
    /// teardown has three more callers in the player system, and two of them are on the
    /// **enter-world** path. See that method.
    ///
    /// Note the order, which is the client's and is load-bearing: clearing the login recipient comes
    /// **before** the teardown, so reached from here the teardown removes *every* connection
    /// rather than keeping the login server.
    pub fn log_off_server(&mut self) -> usize {
        let goodbye = self.goodbye();
        let sent = goodbye.len();
        self.pending_out.extend(goodbye);
        self.log_off_sent = true;
        self.logon_recipient = RecipientId(0);
        self.exit_world_disconnect();
        sent
    }

    /// The datagrams [`Net::log_off_server`] would send, one per connection, without sending them
    /// or changing anything.
    ///
    /// They depend on nothing that changes while a connection lasts -- the packet is its
    /// receiver's id and iteration and a disconnect section, with no sequence and no key -- so a
    /// host that may lose the chance to send them (a web page being closed) can hand them to
    /// whatever outlives it ahead of time.
    #[must_use]
    pub fn goodbye(&self) -> Vec<(Vec<u8>, Option<SocketAddr>)> {
        let mut out = Vec::new();
        for conn in self.connections.values() {
            let mut p = dereth_transport::OutPacket::new(dereth_transport::wire::ProtoHeader {
                rec_id: conn.receiver.net_id,
                iteration: conn.receiver.iteration,
                ..Default::default()
            });
            if p.add_optional_header(dereth_transport::wire::PacketFlags::DISCONNECT, Vec::new())
                .is_err()
            {
                continue;
            }
            // No key: the section is disposable, so `header_ & ENCRYPTED_CHECKSUM` stays clear.
            debug_assert!(!p.needs_encryption());
            if let Ok(bytes) = p.serialize(None) {
                out.push((bytes, conn.addr));
            }
        }
        out
    }

    /// Whether [`Net::log_off_server`] has run.
    #[must_use]
    pub fn log_off_sent(&self) -> bool {
        self.log_off_sent
    }

    /// Entering the world -- the whole function sets the in-game flag.
    ///
    /// The character log-on's second step calls it immediately after
    /// the enter-world send.
    pub fn enter_world(&mut self) {
        self.in_game = true;
    }

    /// Whether the client is currently in game.
    #[must_use]
    pub fn in_game(&self) -> bool {
        self.in_game
    }

    /// Removing a connection -- forget one peer.
    ///
    /// ```c
    /// if a packet controller exists, delete recipient `rec_id`;
    /// remove the receiver from the connection queue;
    /// clear the receiver; // both crypto systems, key exchange and NAK sequence map;
    ///                     // state becomes Disconnected
    /// if the id is the login or current server and logoff was not sent {
    ///     send the server logoff;
    ///     send the server-died login notice;
    /// }
    /// ```
    ///
    /// Dropping the [`Connection`] out of the map performs the client's first three cleanup steps:
    /// the flow queue, the indicator, the receiver, its two `CryptoSystem`s and its NAK map all go
    /// with it, and a recipient that is not in the map is `Disconnected` to
    /// [`Net::connection_state`].
    ///
    /// **The return value is the re-entry.** `true` means this removal took the client's
    /// server-logoff plus server-died-notice branch -- the connection that went was the one the
    /// session is talking to -- and the caller owes the notice. It is a return rather than a call
    /// because connection failure is a *session*-layer notice and this crate has no
    /// way to raise one.
    pub fn remove_connection(&mut self, r: RecipientId) -> bool {
        if self.connections.remove(&r.0).is_none() {
            return false;
        }
        if (r == self.logon_recipient || r == self.world_recipient) && !self.log_off_sent {
            self.log_off_server();
            return true;
        }
        false
    }

    /// The exit-world disconnect -- leave the world, keep the login server.
    ///
    /// ```c
    /// clear the currently-in-game flag if set;
    /// current server recipient = login recipient;
    /// for each connection, remove it unless its id is the login recipient;
    /// if a connection remains,
    ///                         copy its network id;
    ///                         clear its NAK state;
    ///                         clear the world-switch-history flag;
    ///                         reset the event counter to 0;
    /// otherwise clear the login recipient id;
    /// ```
    ///
    /// # It is not only the shutdown path
    ///
    /// The log-off send drops the whole transport a step later, so from there the teardown looks
    /// incidental. It has **four** call sites, three of them in the player system:
    ///
    /// | caller | when |
    /// |---|---|
    /// | character log-on | first step, immediately **before** the enter-world request |
    /// | character log-on | on `0xF7DF`, before the ready-to-enter flag becomes true |
    /// | character log-off | on the server's `0xF653` |
    /// | server log-off | its last act, at shutdown |
    ///
    /// So this is the client's **re-entry gate**: every entry into the world runs it twice before
    /// the request leaves, which is what makes a second entry from the same client session start
    /// from the same state as the first.
    ///
    /// # Why connection-removal re-entry cannot fire from here
    ///
    /// Removing a connection calls the server-logoff path when the removed recipient is
    /// the login or current server and logoff has not been sent. Reached from here it never
    /// can: the current server was set equal to the login recipient above, and the loop skips
    /// the login recipient, so every id it passes differs from both. The ordering of those statements
    /// is what guarantees it, and reversing them would recurse. Asserted by the
    /// `the_teardown_can_never_re_enter_log_off_server` test.
    ///
    /// # What this build has no field for, named rather than invented
    ///
    /// A single connection-wide id; here every [`Connection`] carries its own network id and the
    /// packet writer reads that one, which is the same value
    /// the assignment would copy. Dead against a single-process shard, and not guessed at here.
    ///
    /// The world-switch history flag does have a field: [`Net::world_switch_history`] is cleared
    /// while leaving the world but retaining the login server, which is exactly what this teardown
    /// does.
    ///
    /// Resetting the event counter to 0 is the session's job, not the transport's:
    /// `crate::client_session::Session` performs it on the same three edges.
    ///
    /// Returns how many connections were removed.
    pub fn exit_world_disconnect(&mut self) -> usize {
        self.in_game = false;
        self.world_recipient = self.logon_recipient;
        let doomed: Vec<u16> = self
            .connections
            .keys()
            .copied()
            .filter(|id| *id != self.logon_recipient.0)
            .collect();
        for id in &doomed {
            let re_entered = self.remove_connection(RecipientId(*id));
            debug_assert!(
                !re_entered,
                "exit_world_disconnect skips logon_recipient and world_recipient equals it, so \
                 remove_connection cannot re-enter log_off_server from here"
            );
        }
        match self.connections.values_mut().next() {
            // The head connection's NAK state is reset to `NoNak`. The head after the loop is the login
            // server, the one connection the teardown keeps.
            Some(head) => {
                head.receiver.nak_state = dereth_transport::session::ReceiverState::NoNak;
                // The world switch history's been-switched flag is cleared -- the next world switch is
                // accepted whatever stamp it carries, because the next world is a fresh
                // conversation. The *logon* history is deliberately untouched: the client
                // resets only the world half here.
                self.world_switch_history.been_switched_before = false;
            }
            // Nothing survived: `logon_recipient = 0`. Reached from the server log-off, which
            // zeroes `logon_recipient` before calling here, so the loop removes *every*
            // connection.
            //
            // **Kept and unfalsifiable.** Code and tests can agree here while the explanation is
            // wrong. Deleting this line reddens nothing, and not because the tests are weak:
            // the arm can never be observed *changing* anything. The loop keeps
            // `logon_recipient`'s connection whenever it is in the table, so reaching `None` needs
            // `logon_recipient` to name a connection that is not there -- and the only writer of
            // `logon_recipient`
            // (connection-request step 6) sets it in the same statement that
            // inserts. So the one caller that gets here, the server log-off, has already made it 0.
            // The client carries the line anyway and so does this; it is a claim about a state
            // neither build can construct, not dead code to throw away.
            None => self.logon_recipient = RecipientId(0),
        }
        doomed.len()
    }

    /// Process one received datagram in its documented order.
    ///
    /// # Errors
    /// A [`RejectReason`]. The client logs none of them and bumps the bad-packets-received
    /// counter; a rejected packet simply vanishes.
    pub fn feed(
        &mut self,
        raw: &[u8],
        from: Option<SocketAddr>,
        now: LocalTime,
    ) -> Result<(), RejectReason> {
        let parsed = ParsedPacket::parse(raw).map_err(|_| RejectReason::NoConnection)?;
        let rec_id = parsed.header.rec_id;

        // 1. Verify the header.
        ReceiverData::verify_header(
            &parsed.header,
            self.connections.get(&rec_id).map(|c| &c.receiver),
            from.map(|a| a.ip()),
        )?;

        let Some(conn) = self.connections.get_mut(&rec_id) else {
            // An unsequenced pre-connection packet (the handshake) is legal here, but it is the
            // session layer that acts on it; the transport has nowhere to put it yet.
            return Ok(());
        };

        // 3. `process_new_seq_num`, which is where the ISAAC stream advances -- exactly once per
        //    encrypted sequence number, and never for a duplicate.
        // 4. The checksum. A packet that fails here and was sequenced *and* encrypted is treated as
        //    lost and re-requested under the key it was just checked against. Both steps are the
        //    shared receive check, which the server runs on its clients' packets too.
        conn.receiver
            .accept(&parsed.header, |key| parsed.checksum_ok(key))?;

        // 5-6. The peer's interval and the byte count for the `Flow` report. A datagram stamped
        //      with a newer interval closes the one being counted, and its report is queued for
        //      the peer; it waits on a held packet like the other periodic sections (see the flow
        //      queue's enqueue) and rides the next acknowledgement or data packet.
        if let Some(flow) = conn.receiver.account_datagram(
            parsed.header.interval,
            u32::try_from(raw.len()).unwrap_or(u32::MAX),
        ) {
            let _ = conn
                .flow
                .enqueue_optional_header(dereth_transport::wire::PacketFlags::FLOW, flow.to_vec());
        }

        // 8. The 140-second timeout's clock.
        conn.receiver.local_time_last_got_data = now;

        // The packet processor: the first packet that is *not* a ConnectRequest
        // completes the handshake.
        if conn.state == ConnectionState::ConnectionRequestAcked
            && !parsed
                .header
                .header
                .contains(dereth_transport::wire::PacketFlags::CONNECT_REQUEST)
        {
            conn.state = ConnectionState::Connected;
            conn.local_time_last_connection_state_changed = now;
        }

        // In the optional-header processor the header owns the state edge. The
        // derived client handler inspects the `NetError` body separately, so the no-error
        // sentinel still enters DisconnectReceived without becoming a connection-error notice.
        if parsed
            .header
            .header
            .contains(dereth_transport::wire::PacketFlags::NET_ERROR_DISCONNECT)
        {
            conn.state = ConnectionState::DisconnectReceived;
            conn.local_time_last_connection_state_changed = now;
        }

        // TimeSync is the master clock.
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::TIME_SYNC)
            .filter(|b| b.len() >= 8)
        {
            let bits = u64::from_le_bytes([
                body[0], body[1], body[2], body[3], body[4], body[5], body[6], body[7],
            ]);
            let server_time = f64::from_bits(bits);
            self.last_time_sync = Some(server_time);

            // ------------------------------------------------------------------------------
            // the time-sync handler's tail — the speed-hack detector.
            // ------------------------------------------------------------------------------
            //
            // The handler's byte-level instruction sequence and an independent rendering
            // agree instruction for instruction.
            // `cur` is the current-time snapshot, taken **before** applying the server time. Here it
            // is the value the frame published through
            // [`Net::set_cur_time`], and applying the sync is the caller's later step, so the
            // ordering is retail's.
            //
            // The three arms, in order: if `cur <= S + 60` the latch is reset to zero; else if the
            // latch is still zero it is set to `cur` and the handler returns; else if
            // `cur > latch + 60` and there is a recipient, the error header goes out.
            //
            // So it is **not** a running skew and **not** a single delta: it is a single delta
            // (`cur > S + 60`) that has to hold *continuously for another 60 seconds of client
            // time* before anything is sent. Any sync inside tolerance clears the latch outright.
            //
            // What it does locally is **nothing**. There is no store to the latch on the sending
            // arm, no disconnect, no state change, no UI: the client keeps playing and keeps
            // re-sending the accusation on every subsequent out-of-tolerance TimeSync. The client
            // reports itself and leaves the verdict to the server.
            //
            // Blast radius against the shard we run: the `NetError` section's flags `7` mean
            // disposable | exclusive | pre-connection (with mask `0x100000`
            // and flags of 7), so the section
            // travels alone in an unsequenced, unencrypted packet that is never cached for
            // retransmit. ACE's optional-header decoder does not consume section `0x00100000`
            // at all, so the datagram fails its CRC and is dropped — one discarded datagram, with
            // nothing sequenced behind it.
            if self.cur_time <= server_time + SPEED_HACK_WINDOW {
                self.time_client_speed_hack_detection = 0.0;
            } else if self.time_client_speed_hack_detection == 0.0 {
                self.time_client_speed_hack_detection = self.cur_time;
            } else if self.time_client_speed_hack_detection + SPEED_HACK_WINDOW < self.cur_time {
                // The controller's optional-header enqueue resolves `rec_id` back to a
                // flow queue — the queue of the connection the TimeSync arrived on, which is this
                // one. The non-null recipient guard is the `let Some(conn)` above.
                let _ = conn.flow.enqueue_optional_header(
                    dereth_transport::wire::PacketFlags::NET_ERROR,
                    dereth_transport::conn::NetErrorCode::RunningSpeedhack
                        .pack()
                        .to_vec(),
                );
            }
        }

        // The optional-header processor's EchoResponse arm.
        // It widens the two wire floats, subtracts them from the double local clock
        // in order, and narrows only the final store. Retail applies no stale, sign or finiteness
        // guard to the resulting sample: every addressed receiver keeps it, while only
        // `world_recipient` enters the ping ring.
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::ECHO_RESPONSE)
            .filter(|b| b.len() >= 8)
        {
            let local_time = f32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            let holding_time = f32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: retail narrows the double subtraction result to f32 only at the final store.
            let sample = (now.0 - f64::from(local_time) - f64::from(holding_time)) as f32;
            conn.receiver.round_trip_latency = sample;
            if rec_id == self.world_recipient.0 {
                self.link_status.on_ping_response(sample);
            }
        }

        // The reliability triple, the shared optional-header processor's three remaining
        // arms. A client that parses all three and acts on none of them breaks like this:
        //
        //   * a `RequestRetransmit` naming packets the server never got produces **nothing** --
        //     ack enqueueing is the only filler of the ack list, and the `transmit_acks` at the
        //     bottom of `tick` would run every tick over a list that is structurally always
        //     empty;
        //   * a `RejectRetransmit` leaves our own NAK set standing, so the client asks for a
        //     sequence the server has already said is gone, for ever, every 0.6 s;
        //   * an `AckSequence` retires nothing, so `SentPacketHistory` is bounded only by this
        //     crate's own 4096-packet / 120-second cap and never by the protocol.
        //
        // The order is the original's: NAK before reject before ack. It matters, because
        // Ack enqueueing checks the sent-packet store, and the flush this ack schedules is
        // deliberately deferred to `tick` -- see `Connection::flush_num`.
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::REQUEST_RETRANSMIT)
        {
            // The NAK handler feeds the recipient's NAK processing.
            let ids = dereth_transport::wire::optional::seq_ids(body);
            // The acks-received perf counter is bumped by n, which
            // the perf counters route to the **NAKed-packets** count. Retail's
            // "acks received" is a `RequestRetransmit` arriving from the peer -- the name is the
            // old net layer's and the field it lands in is the honest one.
            self.current_link_status.pkts_naked = self
                .current_link_status
                .pkts_naked
                .wrapping_add(u16::try_from(ids.len()).unwrap_or(u16::MAX));
            if let Some(first) = conn.flow.enqueue_acks(&ids) {
                // The NAK processing's own tail: the NAK's first id is an implicit cumulative ACK of
                // everything below it, applied through the same flush number as `AckSequence`.
                if first != conn.flush_num
                    && dereth_transport::session::lhs_newer(first, conn.flush_num)
                {
                    conn.flush_num = first;
                }
            }
        }
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::REJECT_RETRANSMIT)
        {
            // The empty-ack handler drops those ids from the set of ids we NAKed. The parked
            // ISAAC keys are forgotten rather than consumed, and `highest_id_received` does
            // **not** advance: the keys were drawn when the gap was noticed, so the stream stays
            // aligned.
            //
            // Why the sequence cannot advance is not a choice made here. A `RejectRetransmit` is a
            // control datagram with `seq_id == 0`, and the shared packet processor splits on
            // exactly that before any optional header is read:
            // a non-zero `seq_id` goes to the sequence walk, and a zero one with `header & 2`
            // clear carries on with no sequence at all.
            //
            // The new-sequence path is therefore never reached and nothing is drawn. The whole
            // arm removes each listed id from the NAK set and returns: it writes no
            // counter, logs nothing, and tears nothing down -- the NAK state is recomputed from the
            // set by the receiver's periodic state update; its empty-set arm is what
            // turns the peer's NAK burst back into an `AckSequence`.
            conn.receiver
                .handle_empty_ack(&dereth_transport::wire::optional::seq_ids(body));
        }
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::ACK_SEQUENCE)
            .filter(|b| b.len() >= 4)
        {
            // The pak handler feeds the recipient's queue flush. A zero is ignored
            // by the original, and so is a value that is not newer.
            let flush = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            if flush != 0
                && flush != conn.flush_num
                && dereth_transport::session::lhs_newer(flush, conn.flush_num)
            {
                conn.flush_num = flush;
            }
        }

        // 9. Fragments -> blobs -> net queues.
        if !parsed.fragments.is_empty() {
            let done = conn
                .indicator
                .check_in_packet(&parsed.fragments, rec_id, now);
            for blob in done {
                self.queues.add_received_blob(blob);
            }
        }

        // the optional-header processor's two switch arms, last because both
        // handlers need `&mut self` and the `conn` borrow above has to end first. Neither can
        // share a datagram with anything else -- both sections carry the EXCLUSIVE flag, which
        // `ParsedPacket::parse` enforces -- so nothing above them can have depended on the order.
        //
        // * `0x100 ServerSwitch` dispatches to the server-switch handler
        // * `0x800 Referral` dispatches to the referral handler
        //
        // Decoding these sections without acting on them would let a Referral arrive, pass every
        // check, update the byte counters and vanish.
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::SERVER_SWITCH)
        {
            if let Ok(Some(switch)) = dereth_transport::conn::ServerSwitch::from_bytes(body) {
                self.handle_server_switch(&switch, rec_id);
            }
        }
        if let Some(body) = parsed
            .optional
            .get(&dereth_transport::wire::PacketFlags::REFERRAL)
        {
            if let Ok(referral) = dereth_transport::conn::Referral::from_bytes(body) {
                self.handle_referral(&referral, now);
            }
        }

        // The perf counters route the three received-packet counters
        // (optional headers only 3, headers and data 4, data only 5) into the **one**
        // received-packets field, so a packet that got this far is one received packet whatever it
        // carried; the bytes-received counter takes its length. `process_connections`, like the
        // client, owns the independent two-second snapshot edge.
        self.current_link_status.pkts_received =
            self.current_link_status.pkts_received.wrapping_add(1);
        self.current_link_status.bytes_received = self
            .current_link_status
            .bytes_received
            .wrapping_add(u32::try_from(raw.len()).unwrap_or(u32::MAX));
        Ok(())
    }

    /// The shared connection walk, then the client's own per-connection step.
    ///
    /// This is the network step's cadence, not a packet-arrival callback. The current world
    /// receiver therefore takes its two-second snapshot even when no new datagram arrived during
    /// this invocation; the time since data was last received is the elapsed time from the
    /// receiver's independent accepted-packet stamp.
    pub fn process_connections(&mut self, now: LocalTime) {
        // the client's connection walk, whose whole body is the referral-queue walk followed by
        // a return -- the referral cadence runs **before** the per-connection walk and is driven by this
        // call, not by `tick`.
        self.process_referral_queue(now);

        let cur_time = self.cur_time;
        let ordinary_use_time =
            cur_time - self.last_did_use_time < dereth_transport::flow::CONNECTION_TIMEOUT;
        let world_recipient = self.world_recipient.0;
        let log_off_sent = self.log_off_sent;
        let mut remove = Vec::new();
        let mut self_referrals: Vec<dereth_transport::conn::Referral> = Vec::new();

        for (&rec_id, conn) in &mut self.connections {
            match conn.state {
                ConnectionState::DisconnectSent => {
                    remove.push(rec_id);
                    continue;
                }
                // The client's DisconnectReceived arm falls through to the Connected work until the
                // matching response has actually gone out.
                ConnectionState::Connected | ConnectionState::DisconnectReceived
                    if rec_id == world_recipient
                        && now.0 - conn.local_time_last_heartbeat
                            >= crate::linkstatus::HEARTBEAT_INTERVAL =>
                {
                    conn.local_time_last_heartbeat = now.0;
                    self.current_link_status.round_trip_delay = conn.receiver.round_trip_latency;
                    #[allow(clippy::cast_possible_truncation)]
                    // LINT-OK: both fields are `float` in the client's link-status snapshot.
                    {
                        self.current_link_status.time_since_last_got_data =
                            (now.0 - conn.receiver.local_time_last_got_data.0) as f32;
                        self.current_link_status.snapshot_duration =
                            (now.0 - self.link_status.local_time_of_snapshot) as f32;
                    }
                    let snapshot = self.current_link_status;
                    self.link_status.add_snapshot(&snapshot, now.0);
                    self.heartbeat_pending = true;
                    self.current_link_status = crate::linkstatus::Snapshot::default();
                }
                _ => {}
            }

            // In the connection walk the stall guard reads current game time and the last shared
            // network update; the silence clock is independently local time.
            if ordinary_use_time
                && now.0 - conn.receiver.local_time_last_got_data.0
                    > dereth_transport::flow::CONNECTION_TIMEOUT
                && conn.state < ConnectionState::DisconnectReceived
            {
                let was = conn.state;
                conn.state = ConnectionState::DisconnectReceived;
                conn.local_time_last_connection_state_changed = now;
                conn.disconnect_response_queued = false;

                // In the connection walk, this is the **self-referral** that implements the
                // client's automatic world reconnect and is the one reader of the receiver's
                // referral cookie:
                //
                // it sets the connection state to 6, and only if it WAS `Connected`, the
                // referral cookie is non-zero and `log_off_sent` is clear does it build a referral
                // struct from that cookie (with the receiver's address and recipient id) and hand
                // it to the referral handler.
                //
                // The client hands a Referral it made itself to its own receive handler; because the state
                // was set to 6 on the line above, the referral handler's `> Connected` arm tears
                // the connection down and the queue arm then re-logs-in with the same cookie.
                // Nothing goes on the wire at this point -- the Referral never leaves the
                // process.
                //
                // Deferred out of the `&mut self.connections` walk the way the removals below
                // are; the client runs it inline, and nothing between here and the loop's end
                // reads what it changes.
                if was == ConnectionState::Connected
                    && conn.receiver.referral_cookie != 0
                    && !log_off_sent
                {
                    if let Some(SocketAddr::V4(v4)) = conn.addr {
                        self_referrals.push(dereth_transport::conn::Referral {
                            cookie: conn.receiver.referral_cookie,
                            addr: v4,
                            id_server: conn.receiver.rec_id,
                            family: 2,
                        });
                    }
                }
            }
        }

        // The shared per-frame step writes the entry snapshot only after the connection walk.
        self.last_did_use_time = cur_time;
        for rec_id in remove {
            if self.remove_connection(RecipientId(rec_id)) {
                self.server_died_pending = true;
            }
        }
        for referral in self_referrals {
            self.handle_referral(&referral, now);
        }
    }

    /// Run shared-network time processing and then packet-controller time processing.
    ///
    /// `budget` is the receive budget; the client uses **50 ms** and
    /// re-checks it on every loop iteration.
    ///
    /// The receive half needs a socket; with none bound this is the send half only, which is what a
    /// replay drives.
    pub fn tick(&mut self, now: LocalTime, _budget: Duration) {
        // The send returns 0 once log-off has been sent -- after the goodbye the
        // client transmits nothing at all, which is why the recorded Disconnect is the last
        // datagram in every session even though the flow queue's 2 s ACK cadence is still running.
        if self.log_off_sent {
            return;
        }
        for conn in self.connections.values_mut() {
            conn.indicator.flush_timed_out_eph_info(now);

            // The local-interval bump — the two periodic headers.
            // If the events were computed and dropped, the client would send nothing at all once
            // it had said its piece: no ACK, no TimeSync, no EchoRequest, no keep-alive. ACE's
            // `Session.TickOutbound` would then reach `Network.TimeoutTick` and terminate the
            // session, and behind a NAT the mapping would expire first.
            let mut icmd_keep_alive = false;
            let mut time_sync_due = false;
            for event in conn.flow.advance_interval(now) {
                match event {
                    dereth_transport::flow::IntervalEvent::TimeSyncAndEcho => time_sync_due = true,
                    dereth_transport::flow::IntervalEvent::IcmdKeepAlive => icmd_keep_alive = true,
                }
            }

            // In the connection walk, the cumulative-ack enqueue when
            // the NAK set is empty, when it is not. The two
            // share one timestamp, which is why the 2.0 s ACK cadence and the 0.6 s
            // NAK cadence interfere.
            let mut sections: Vec<(u32, Vec<u8>)> = Vec::new();
            if matches!(
                conn.state,
                ConnectionState::Connected | ConnectionState::DisconnectReceived
            ) {
                let since = now.seconds_since(conn.receiver.time_stamp);
                let naks = conn.receiver.get_naks();
                if naks.is_empty() {
                    if since >= dereth_transport::flow::ACK_INTERVAL {
                        sections.push((
                            dereth_transport::wire::PacketFlags::ACK_SEQUENCE,
                            conn.receiver
                                .window
                                .highest_id_received
                                .to_le_bytes()
                                .to_vec(),
                        ));
                        conn.receiver.time_stamp = now;
                    }
                } else if since > dereth_transport::flow::NAK_INTERVAL {
                    let mut block = Vec::with_capacity(4 + 4 * naks.len());
                    block.extend_from_slice(&u32::try_from(naks.len()).unwrap_or(0).to_le_bytes());
                    for id in naks {
                        block.extend_from_slice(&id.to_le_bytes());
                    }
                    sections.push((
                        dereth_transport::wire::PacketFlags::REQUEST_RETRANSMIT,
                        block,
                    ));
                    conn.receiver.time_stamp = now;
                }

                // The time-sensitive pair is queued at the interval boundary itself. Neither
                // `TimeSync` nor `EchoRequest` lies below the section masks a packet must carry
                // to be sent while it is the last one waiting, so the pair waits on a held packet
                // and rides the next thing the connection sends -- the two-second cumulative ACK,
                // a data fragment, or a packet queued ahead of it. That is why the recorded
                // sessions carry it on the ACK (`header_ = 0x0B004002`) or on a data packet
                // (`0x0B000006`), and never on a clock of its own.
                if time_sync_due {
                    sections.push((
                        dereth_transport::wire::PacketFlags::TIME_SYNC,
                        0f64.to_le_bytes().to_vec(),
                    ));
                    sections.push((
                        dereth_transport::wire::PacketFlags::ECHO_REQUEST,
                        0f32.to_le_bytes().to_vec(),
                    ));
                }
            }
            if !sections.is_empty() {
                let _ = conn.flow.enqueue_optional_headers(&sections);
            }
            // What the pair says is written as the packet goes, not as it is queued: `TimeSync`
            // carries computed time, the sender's *game* time -- this crate has no game clock, and
            // the client's is hard-set from the server's last TimeSync, so that is the value
            // available at this layer, and it is one ACE logs and ignores. `EchoRequest` carries
            // local time, which ACE *does* read: `NetworkSession.VerifyEcho` compares successive
            // values against its own wall clock, so it must be the real local time at the send.
            conn.flow
                .set_time_sensitive(dereth_transport::flow::TimeSensitive {
                    game_time: self.last_time_sync.unwrap_or(0.0),
                    local_time: local_time_f32(now),
                });

            let mut crypto = std::mem::replace(
                &mut conn.receiver.crypto_outgoing,
                dereth_transport::CryptoSystem::new(0),
            );
            // Emptying the flow queue transmits NAKs, ACKs and then new packets.
            if let Some(block) = conn.flow.compile_empty_acks() {
                let _ = conn.flow.enqueue_optional_header(
                    dereth_transport::wire::PacketFlags::REJECT_RETRANSMIT,
                    block,
                );
            }
            let to = conn.addr;
            let resends = conn.flow.transmit_acks();
            // The retransmits-sent perf counter is bumped by n -> the retransmitted-packets
            // count. These also go through `take_outgoing`, so they count as sent packets too,
            // exactly as retail's do: the send path bumps the packets-sent counter for
            // every datagram and the retransmit counter is additional.
            self.current_link_status.pkts_retransmitted = self
                .current_link_status
                .pkts_retransmitted
                .wrapping_add(u16::try_from(resends.len()).unwrap_or(u16::MAX));
            self.pending_out
                .extend(resends.into_iter().map(|b| (b, to)));
            let outgoing = conn.flow.transmit_new_packets(&mut crypto, now);
            if conn.disconnect_response_queued
                && outgoing.iter().any(|bytes| {
                    ParsedPacket::parse(bytes).is_ok_and(|packet| {
                        packet
                            .header
                            .header
                            .contains(dereth_transport::wire::PacketFlags::NET_ERROR_DISCONNECT)
                    })
                })
            {
                // The socket adapter cannot report a successful `sendto` back through `Datagram`;
                // successful serialization and handoff is this transport boundary's send edge.
                conn.state = ConnectionState::DisconnectSent;
                conn.local_time_last_connection_state_changed = now;
                conn.disconnect_response_queued = false;
            }
            self.pending_out
                .extend(outgoing.into_iter().map(|b| (b, to)));
            conn.receiver.crypto_outgoing = crypto;

            // the recipient's per-frame step, immediately after emptying the flow queue:
            //
            //     when the flush number is non-zero, flush the sent packets up to it and zero it.
            //
            // **After**, not before, and that is the whole reason `flush_num` is a pending value
            // rather than something `feed` applies on the spot: a datagram carrying `AckSequence(N)`
            // and `RequestRetransmit(M < N)` together must still resend M. Applying the flush
            // eagerly would drop M from the cache first and answer a recoverable NAK with
            // `RejectRetransmit`.
            if conn.flush_num != 0 {
                conn.flow.flush_sent_packets(conn.flush_num);
                conn.flush_num = 0;
            }

            // in the flow queue's empty step this is deliberately after all three transmit
            // passes. An empty queue therefore queues the response now and sends it next tick;
            // the ten-second arm prevents older unsent work from blocking teardown forever.
            if conn.state == ConnectionState::DisconnectReceived
                && !conn.disconnect_response_queued
                && (conn.flow.waiting_len() == 0
                    || now.0 - conn.local_time_last_connection_state_changed.0
                        > dereth_transport::flow::DISCONNECT_GRACE)
                && conn
                    .flow
                    .enqueue_optional_header(
                        dereth_transport::wire::PacketFlags::NET_ERROR_DISCONNECT,
                        dereth_transport::conn::NetErrorCode::None.pack().to_vec(),
                    )
                    .is_ok()
            {
                conn.disconnect_response_queued = true;
            }

            // The NAT keep-alive, last because it is not a flow-queue packet at all: `seq_id = 0`,
            // `iteration = 0`, unsequenced, and addressed to the server's **port + 1**
            // (`docs/networking/01-packet-format.md` §3.15). A server that looks the session up before dispatching
            // on the listener rejects it as stale, which is why it is built here rather than
            // enqueued.
            if icmd_keep_alive {
                if let Some(bytes) = build_icmd_nop() {
                    let dest = to.map(|a| {
                        SocketAddr::new(a.ip(), dereth_transport::conn::handshake_port(a.port()))
                    });
                    self.pending_out.push((bytes, dest));
                }
            }
        }
    }
}

/// Local time as the `EchoRequest` section carries it.
///
/// The client's `local_time` is a `float` and the section is four bytes, so the narrowing is the
/// original's, not a shortcut. `dereth_primitives::num`'s rules govern *engine* arithmetic; this is a wire field.
#[allow(clippy::cast_possible_truncation)]
fn local_time_f32(now: LocalTime) -> f32 {
    now.0 as f32
}

/// The local-interval bump's no-op command keep-alive, built whole.
///
/// The no-op command, 1, parameter 0; sequence, iteration and interval are all zero, and the
/// section is disposable, so the packet is unencrypted and takes no ISAAC key.
///
/// **The recipient id is a hard zero and takes no argument, and that is retail's structure, not a
/// simplification**. The local-interval bump reaches the wire through
/// the optional-header send, and it is the *only* one of that function's three
/// call sites in the client that passes **no receiver** (a literal null argument before the
/// call). The optional-header sender branches on exactly that argument, and
/// its no-receiver arm writes a literal zero into both recipient and iteration;
/// **both remain zero**, whatever network id the server has assigned. The protocol header
/// carries recipient id at `+12` and iteration at `+18`. The other two
/// call sites — the log-off send and the connect ack are the other two —
/// take the non-null arm and stamp the receiver's network id and iteration;
/// [`Net::log_off_server`] and `dereth-client`'s `send_connect_ack` do the same.
///
/// Passing `conn.flow.net_id` here would be invisible in six of the seven locked
/// captures and wrong in the seventh: all **13** recorded command keep-alives carry
/// recipient and iteration zero, `short-play-with-training` included, and it is the only recorded
/// session whose assigned network id is not 0. Taking no parameter is deliberate — a `net_id`
/// argument would invite exactly that regression.
/// # `WorldLoginRequest` datagram, built whole
///
/// 28 bytes: the 20-byte protocol header and the 8-byte cookie. The header is a zeroed stack
/// local; only mask `0x20000`, checksum and data length are written afterwards.
/// Sequence, recipient, interval and iteration are all 0, exactly like [`build_icmd_nop`]'s
/// and for the same
/// structural reason: there is no receiver to take an id from. `WorldLoginRequest` uses
/// flags 7 (`DISPOSABLE | EXCLUSIVE | PRE_CONNECTION`), so the packet is unencrypted and
/// takes no ISAAC key.
fn build_world_login_request(cookie: u64) -> Option<Vec<u8>> {
    let mut p = dereth_transport::OutPacket::new(dereth_transport::wire::ProtoHeader {
        rec_id: 0,
        ..Default::default()
    });
    p.add_optional_header(
        dereth_transport::wire::PacketFlags::WORLD_LOGIN_REQUEST,
        cookie.to_le_bytes().to_vec(),
    )
    .ok()?;
    debug_assert!(!p.needs_encryption(), "flags 7 has the disposable bit");
    p.serialize(None).ok()
}

fn build_icmd_nop() -> Option<Vec<u8>> {
    const CMD_NOP: u32 = 1;
    let mut p = dereth_transport::OutPacket::new(dereth_transport::wire::ProtoHeader {
        rec_id: 0,
        ..Default::default()
    });
    let mut body = Vec::with_capacity(8);
    body.extend_from_slice(&CMD_NOP.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    p.add_optional_header(dereth_transport::wire::PacketFlags::CICMD_COMMAND, body)
        .ok()?;
    p.serialize(None).ok()
}

/// `Queue` -> the seam's [`NetQueue`].
///
/// The seam names six of the twelve and folds the rest into `Other`. `Other` carries the raw id, so
/// nothing is lost and a queue the session layer has not been taught to name still arrives.
#[must_use]
pub fn to_seam_queue(q: Queue) -> NetQueue {
    match q {
        Queue::Control => NetQueue::Control,
        Queue::Weenie => NetQueue::Weenie,
        Queue::Login => NetQueue::Logon,
        Queue::Database => NetQueue::ClientCache,
        Queue::Ui => NetQueue::UiQueue,
        Queue::WorldObjects => NetQueue::WorldObjects,
        other => NetQueue::Other(u8::try_from(other.id()).unwrap_or(0)),
    }
}

/// The reverse. `Other` carrying an out-of-range id yields `None`, which `send` treats as a drop —
/// the same as `add_received_blob` does on the receive side.
#[must_use]
pub fn from_seam_queue(q: NetQueue) -> Option<Queue> {
    match q {
        NetQueue::Control => Some(Queue::Control),
        NetQueue::Weenie => Some(Queue::Weenie),
        NetQueue::Logon => Some(Queue::Login),
        NetQueue::ClientCache => Some(Queue::Database),
        NetQueue::UiQueue => Some(Queue::Ui),
        NetQueue::WorldObjects => Some(Queue::WorldObjects),
        NetQueue::Other(id) => Queue::from_wire(u16::from(id)),
    }
}

impl Transport for Net {
    /// The UI blob send.
    ///
    /// Queues 4, 5 and 8 go to the login recipient and everything else to the world recipient;
    /// every blob gets ordering type `0x23000000` or `0x03000000`, the per-blob
    /// unordered stamp, and priority 5.
    ///
    /// `ordered` is the caller's request for the ordered wrapper. The retail UI protocol sender
    /// an ordering type other than those two constants, so it is honoured only through the queue
    /// mapping — but the parameter is kept, because a rebuild that later wants to prioritise
    /// movement over chat needs it and it costs nothing on the wire.
    fn send(&mut self, queue: NetQueue, ordered: bool, payload: &[u8]) {
        let _ = ordered;
        let Some(q) = from_seam_queue(queue) else {
            return; // an id outside 1..=11 is a drop, not an error
        };
        let recipient = if q.is_login_bound() {
            self.logon_recipient
        } else {
            self.world_recipient
        };
        let Some(conn) = self.connections.get_mut(&recipient.0) else {
            return;
        };
        if !conn.state.accepts_blobs() {
            // The client flow queue's enqueue refuses at or past `DisconnectReceived`.
            return;
        }

        let mut blob = NetBlob::for_send(payload.to_vec(), q.id());
        blob.id = NetBlobId::make(
            u64::from(q.ordering_type()) << 32,
            self.unordered_stamp,
            self.cur_non_ephemeral_id,
        );
        blob.saved_net_blob_id = blob.id;
        blob.priority = SEND_PRIORITY;
        self.unordered_stamp = self.unordered_stamp.wrapping_add(1);
        self.cur_non_ephemeral_id = self.cur_non_ephemeral_id.next_non_ephemeral_sequence_id();
        conn.flow.enqueue_blob(blob);
    }

    /// One whole reassembled blob, in the frame's queue-drain order, or `None`.
    ///
    /// Never a fragment: reassembly is complete before a blob reaches a queue. The opcode is lifted
    /// out of the first dword because every dispatcher needs it; a blob shorter than four bytes has
    /// no opcode and is dropped rather than misread.
    fn poll(&mut self) -> Option<IncomingMessage> {
        loop {
            let blob = self.queues.pop_next()?;
            if blob.payload.len() < 4 {
                continue;
            }
            let opcode = u32::from_le_bytes([
                blob.payload[0],
                blob.payload[1],
                blob.payload[2],
                blob.payload[3],
            ]);
            let queue = Queue::from_wire(blob.queue_id).map_or(NetQueue::Other(0), to_seam_queue);
            return Some(IncomingMessage {
                opcode,
                queue,
                // Both of these are carried rather than discarded: the sender distinguishes the
                // login server from the world server on a split deployment, and the blob id carries
                // the ordering type and stamp the dispatcher needs. `CompletedBlob::id` is already
                // the *original* id, restored by the queue hand-off, not the ephemeral stream key.
                sender: RecipientId(blob.sender),
                blob_id: SeamNetBlobId(blob.id.0),
                body: blob.payload[4..].to_vec(),
            });
        }
    }
}

#[cfg(test)]
mod tests;
