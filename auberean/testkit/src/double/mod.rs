//! A server double: the servers' side of one recorded game login, played to a client over an
//! in-memory network, with no game logic.
//!
//! The double runs the transport live, as the servers did: the connection request (fresh cookie
//! and key-stream seeds; the script's server id, iteration, client id and server clock), sequence
//! numbers, checksums, acknowledgements, retransmission, the server's clock and echo answers, the
//! referral (a fresh cookie, the world server's address in the double) and the server switch. It
//! replays the recorded server messages byte for byte, on their recorded queues with their recorded
//! blob ids, each when the client event that preceded it in the recording has happened ([`plan`]).
//! A data request is answered with the recorded answer, or refused as the 2017 servers refused
//! most.
//!
//! Sans-IO: the caller moves datagrams between the double and the client, and passes the time.
//! The servers live at the recorded ports of one address; nothing binds a socket.
//!
//! - [`script`]: the recorded game login.
//! - [`plan`]: what each side waits for.
//! - `recorded` (feature `captures`): scripts built from the capture index.

pub mod plan;
#[cfg(feature = "captures")]
pub mod recorded;
pub mod script;

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use dereth_primitives::LocalTime;
use dereth_transport::conn::{ConnectRequest, ConnectSeeds, NetErrorCode, Referral};
use dereth_transport::flow::HANDSHAKE_RESEND;
use dereth_transport::indicator::CompletedBlob;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use yalain_net::endpoint::{standalone, Endpoint, Role, Stats};

use plan::{data_refusal, data_request, Gate, ItemKind, ServerPlan, GATING};
use script::{message_type, Msg, Script};

/// One datagram on the in-memory network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    pub from: SocketAddr,
    pub to: SocketAddr,
    pub bytes: Vec<u8>,
}

/// One message as the double sent or received it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub queue: u16,
    pub blob_id: u64,
    pub raw: Vec<u8>,
}

/// What the double did.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The client's game login, as it arrived.
    pub login: Option<Vec<u8>>,
    /// Per recorded connection: server messages in the order the double queued them.
    pub sent: Vec<Vec<Sent>>,
    /// Per recorded connection: client messages in the order they completed.
    pub received: Vec<Vec<Sent>>,
    /// The gates that fired, with the double's time.
    pub gates_fired: Vec<(Gate, f64)>,
    /// Gates the double fired itself because the client did not ([`ServerDouble::force_next_gate`]).
    pub gates_forced: Vec<Gate>,
    /// Client messages of a gating type beyond what the recording has (type, occurrence).
    pub unmatched_gates: Vec<(u32, usize)>,
    /// Connections that came up (recorded index), in order.
    pub connected: Vec<usize>,
    /// World logins that named a cookie the double never sent.
    pub unknown_world_logins: u32,
    /// Referrals, world switches and goodbyes sent.
    pub referrals: u32,
    pub switches: u32,
    pub goodbyes: u32,
    pub silences: u32,
    /// Per recorded connection: the transport counters.
    pub stats: Vec<Stats>,
    /// Packets refused by a receive window or a parse.
    pub rejected: u64,
    /// Per recorded connection: the client said goodbye.
    pub client_goodbyes: Vec<bool>,
    /// What happened to the connections, with the double's time.
    pub log: Vec<(f64, usize, &'static str)>,
}

struct Live {
    ep: Endpoint,
    cookie: u64,
    request: Vec<u8>,
    request_at: f64,
    requests: u32,
    connected: bool,
    closed: bool,
    /// When the double said goodbye on it: it closes a little later.
    goodbye_at: Option<f64>,
    cursor: usize,
}

/// The double.
pub struct ServerDouble {
    script: Script,
    plan: ServerPlan,
    gate_index: HashMap<Gate, usize>,
    ip: Ipv4Addr,
    scale: f64,
    conns: Vec<Option<Live>>,
    client: Option<SocketAddr>,
    gate_fired: Vec<Option<f64>>,
    client_seen: HashMap<u32, usize>,
    referrals: Vec<(u64, usize)>,
    data_used: HashMap<(u32, u32), usize>,
    made: u32,
    switch_stamp: u32,
    rng: u64,
    out: Vec<Datagram>,
    report: Report,
}

impl core::fmt::Debug for ServerDouble {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ServerDouble")
            .field("conns", &self.script.conns.len())
            .finish_non_exhaustive()
    }
}

impl ServerDouble {
    /// A double at `ip` that plays `script` at `scale` times the recorded pace (0 sends each item
    /// as soon as its gate fires). `seed` makes its cookies and seeds.
    #[must_use]
    pub fn new(script: Script, ip: Ipv4Addr, scale: f64, seed: u64) -> Self {
        let plan = ServerPlan::new(&script);
        let gate_index = plan
            .gates
            .iter()
            .enumerate()
            .map(|(i, g)| (g.gate, i))
            .collect();
        let n = script.conns.len();
        Self {
            gate_fired: vec![None; plan.gates.len()],
            gate_index,
            plan,
            ip,
            scale,
            conns: (0..n).map(|_| None).collect(),
            client: None,
            client_seen: HashMap::new(),
            referrals: Vec::new(),
            data_used: HashMap::new(),
            made: 0,
            switch_stamp: 0x100,
            rng: seed ^ 0x9E37_79B9_7F4A_7C15,
            out: Vec::new(),
            report: Report {
                sent: vec![Vec::new(); n],
                received: vec![Vec::new(); n],
                client_goodbyes: vec![false; n],
                ..Report::default()
            },
            script,
        }
    }

    /// Where a client sends its game login.
    #[must_use]
    pub fn login_addr(&self) -> SocketAddr {
        self.addr(self.script.conns[0].server_port & !1)
    }

    fn addr(&self, port: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(self.ip, port))
    }

    /// The script being played.
    #[must_use]
    pub fn script(&self) -> &Script {
        &self.script
    }

    /// The plan being played.
    #[must_use]
    pub fn plan(&self) -> &ServerPlan {
        &self.plan
    }

    /// What happened so far.
    #[must_use]
    pub fn report(&self) -> Report {
        let mut r = self.report.clone();
        r.stats = self
            .conns
            .iter()
            .map(|c| c.as_ref().map_or_else(Stats::default, |l| l.ep.stats))
            .collect();
        r
    }

    /// How many of a connection's items (messages, referrals, switches) have gone out.
    #[must_use]
    pub fn released(&self, conn: usize) -> usize {
        self.conns
            .get(conn)
            .and_then(Option::as_ref)
            .map_or(0, |l| l.cursor)
    }

    /// Whether a connection came up.
    #[must_use]
    pub fn is_connected(&self, conn: usize) -> bool {
        self.conns
            .get(conn)
            .and_then(Option::as_ref)
            .is_some_and(|l| l.connected)
    }

    /// Whether an item is due to go out without the client doing anything: its gate has fired and
    /// it waits only on its recorded delay.
    #[must_use]
    pub fn item_due(&self) -> bool {
        self.conns.iter().enumerate().any(|(i, c)| {
            c.as_ref().is_some_and(|l| {
                l.connected
                    && !l.closed
                    && self.plan.items[i]
                        .get(l.cursor)
                        .is_some_and(|it| self.gate_fired[it.gate].is_some())
            })
        })
    }

    /// What each connection waits on: its next item's recorded time, kind and gate, and whether
    /// the gate fired. For a test's message when a replay stops making progress.
    #[must_use]
    pub fn describe_waits(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.conns.iter().enumerate() {
            let state = match c {
                None => "not connected",
                Some(l) if l.closed => "closed",
                Some(l) if !l.connected => "connecting",
                Some(_) => "up",
            };
            let next = c.as_ref().and_then(|l| self.plan.items[i].get(l.cursor));
            if let Some(it) = next {
                let kind = match &it.kind {
                    ItemKind::Msg(m) => format!("msg {:x}", m.mtype),
                    other => format!("{other:?}"),
                };
                out.push_str(&format!(
                    "[{i} {state}: t {:.1} {kind} gate {:?} fired {}] ",
                    it.t,
                    self.plan.gates[it.gate].gate,
                    self.gate_fired[it.gate].is_some()
                ));
            } else if !self.plan.items[i].is_empty() {
                let left = self.plan.items[i].len() - c.as_ref().map_or(0, |l| l.cursor);
                if left > 0 {
                    out.push_str(&format!("[{i} {state}: {left} left] "));
                }
            }
        }
        out
    }

    /// Whether a goodbye went out on a connection.
    #[must_use]
    pub fn said_goodbye(&self, conn: usize) -> bool {
        self.conns
            .get(conn)
            .and_then(Option::as_ref)
            .is_some_and(|l| l.goodbye_at.is_some())
    }

    /// Whether every connection's items have all gone out.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.conns.iter().enumerate().all(|(i, c)| match c {
            Some(l) => l.closed || l.cursor >= self.plan.items[i].len(),
            None => self.plan.items[i].is_empty() || self.unreachable(i),
        })
    }

    /// A connection no referral the double will send leads to.
    fn unreachable(&self, conn: usize) -> bool {
        conn != 0
            && !self
                .plan
                .items
                .iter()
                .flatten()
                .any(|it| matches!(it.kind, ItemKind::Referral { to } if to == conn))
    }

    /// The gate the double waits on next, if it is the client's to fire: the earliest unfired gate
    /// that holds back an item on a live connection, once every item recorded before that gate has
    /// gone out (so the client has had everything the recorded client had before it sent the
    /// gate's message).
    #[must_use]
    pub fn waiting_on(&self) -> Option<Gate> {
        let mut best: Option<usize> = None;
        for (i, c) in self.conns.iter().enumerate() {
            let Some(l) = c else { continue };
            if !l.connected || l.closed {
                continue;
            }
            if let Some(it) = self.plan.items[i].get(l.cursor) {
                if self.gate_fired[it.gate].is_none() {
                    best = Some(best.map_or(it.gate, |b| b.min(it.gate)));
                }
            }
        }
        let g = best?;
        let t = self.plan.gates[g].t;
        for (i, c) in self.conns.iter().enumerate() {
            let Some(l) = c else { continue };
            if !l.connected || l.closed {
                continue;
            }
            if self.plan.items[i].get(l.cursor).is_some_and(|it| it.t < t) {
                return None;
            }
        }
        Some(self.plan.gates[g].gate)
    }

    /// How long the recorded client took to fire a gate after the last thing recorded before it
    /// (an item going out, or a connection coming up).
    #[must_use]
    pub fn recorded_wait(&self, gate: Gate) -> f64 {
        let Some(g) = self.gate_index.get(&gate) else {
            return 0.0;
        };
        let t = self.plan.gates[*g].t;
        let before = self
            .plan
            .items
            .iter()
            .flatten()
            .map(|it| it.t)
            .chain(self.script.conns.iter().map(|c| c.t_connect))
            .filter(|x| *x <= t)
            .fold(f64::NEG_INFINITY, f64::max);
        if before.is_finite() {
            t - before
        } else {
            0.0
        }
    }

    /// Fire the gate the double waits on, as if the client had sent its message: for a test whose
    /// client does not send what the recorded one did, so the replay can go on. Returns the gate.
    pub fn force_next_gate(&mut self, now: LocalTime) -> Option<Gate> {
        let gate = self.waiting_on()?;
        let g = *self.gate_index.get(&gate)?;
        self.gate_fired[g] = Some(now.0);
        self.report.gates_forced.push(gate);
        if let Gate::Client { mtype, nth } = gate {
            let seen = self.client_seen.entry(mtype).or_default();
            *seen = (*seen).max(nth + 1);
        }
        Some(gate)
    }

    fn random(&mut self) -> u64 {
        // A fixed-seed generator: the double's cookies and seeds repeat from run to run.
        self.rng = self.rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn conn_of_pair(&self, port: u16) -> impl Iterator<Item = usize> + '_ {
        self.script
            .conns
            .iter()
            .filter(move |c| c.server_port & !1 == port & !1)
            .map(|c| c.n)
    }

    /// A datagram from the client, sent to `to`.
    pub fn datagram(&mut self, to: SocketAddr, from: SocketAddr, bytes: &[u8], now: LocalTime) {
        let Ok(p) = ParsedPacket::parse_from_client(bytes) else {
            self.report.rejected += 1;
            return;
        };
        let h = p.header.header;
        if h.contains(PacketFlags::LOGIN_REQUEST) {
            self.login_request(to, from, &p, now);
        } else if h.contains(PacketFlags::WORLD_LOGIN_REQUEST) {
            self.world_login_request(to, from, &p, now);
        } else if h.contains(PacketFlags::CONNECT_RESPONSE) {
            self.connect_response(to, from, &p, now);
        } else if h.contains(PacketFlags::CICMD_COMMAND) {
            // The client's keep-alive to the odd port; nothing to answer.
        } else {
            self.data(to, from, &p, bytes.len(), now);
        }
    }

    fn connect_request(&mut self, conn: usize, client: SocketAddr, now: LocalTime) -> Live {
        let spec = self.script.conns[conn].clone();
        #[allow(clippy::cast_possible_truncation)]
        let seeds = ConnectSeeds {
            server_to_client: self.random() as u32,
            client_to_server: self.random() as u32,
        };
        let cookie = self.random();
        let cr = ConnectRequest::new(spec.server_time, cookie, spec.net_id, seeds);
        let request = standalone(
            spec.server_id,
            spec.iteration,
            PacketFlags::CONNECT_REQUEST,
            cr.to_bytes().to_vec(),
        );
        #[allow(clippy::cast_possible_truncation)]
        let mut ep = Endpoint::new(
            Role::Server,
            spec.server_id,
            spec.net_id as u16,
            spec.iteration,
            seeds,
            client,
            spec.server_time,
            now,
        );
        ep.recv.time_stamp = now;
        let from = self.addr(spec.server_port & !1);
        self.out.push(Datagram {
            from,
            to: client,
            bytes: request.clone(),
        });
        Live {
            ep,
            cookie,
            request,
            request_at: now.0,
            requests: 1,
            connected: false,
            closed: false,
            goodbye_at: None,
            cursor: 0,
        }
    }

    fn login_request(
        &mut self,
        to: SocketAddr,
        from: SocketAddr,
        p: &ParsedPacket,
        now: LocalTime,
    ) {
        if to.port() & !1 != self.script.conns[0].server_port & !1 {
            return;
        }
        if self.client == Some(from) && self.conns[0].is_some() {
            // A resend: the connection request is being resent on its own clock.
            return;
        }
        self.client = Some(from);
        self.report.login = p.optional.get(&PacketFlags::LOGIN_REQUEST).cloned();
        let live = self.connect_request(0, from, now);
        self.conns[0] = Some(live);
    }

    fn world_login_request(
        &mut self,
        to: SocketAddr,
        from: SocketAddr,
        p: &ParsedPacket,
        now: LocalTime,
    ) {
        let cookie = p
            .optional
            .get(&PacketFlags::WORLD_LOGIN_REQUEST)
            .and_then(|b| b.get(..8))
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])));
        let Some(&(_, conn)) = self.referrals.iter().find(|(c, _)| *c == cookie) else {
            self.report.unknown_world_logins += 1;
            return;
        };
        if self.script.conns[conn].server_port & !1 != to.port() & !1 || self.conns[conn].is_some()
        {
            return;
        }
        let live = self.connect_request(conn, from, now);
        self.conns[conn] = Some(live);
    }

    fn connect_response(
        &mut self,
        to: SocketAddr,
        from: SocketAddr,
        p: &ParsedPacket,
        now: LocalTime,
    ) {
        let cookie = p
            .optional
            .get(&PacketFlags::CONNECT_RESPONSE)
            .and_then(|b| b.get(..8))
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])));
        let candidates: Vec<usize> = self.conn_of_pair(to.port()).collect();
        let Some(conn) = candidates.into_iter().find(|i| {
            self.conns[*i]
                .as_ref()
                .is_some_and(|l| l.ep.peer == from && !l.connected && l.cookie == cookie)
        }) else {
            return;
        };
        let spec = self.script.conns[conn].clone();
        let stamp = self.next_switch_stamp();
        let Some(live) = self.conns[conn].as_mut() else {
            return;
        };
        live.connected = true;
        live.ep.recv.local_time_last_got_data = now;
        self.report.connected.push(conn);
        self.report.log.push((now.0, conn, "up"));
        let mut sends = Vec::new();
        if !spec.login && spec.switched {
            // A world server's first packet is the server switch, alone.
            live.ep
                .send_section(PacketFlags::SERVER_SWITCH, switch_body(stamp));
            sends = live.ep.flush_alone(now);
            self.report.switches += 1;
        }
        let client = live.ep.peer;
        let server_id = spec.server_id;
        for (j, other) in self.conns.iter_mut().enumerate() {
            if let Some(o) = other.as_mut() {
                let same = self.script.conns[j].server_port & !1 == spec.server_port & !1
                    && self.script.conns[j].server_id == server_id;
                if j != conn && same && !o.closed {
                    o.closed = true;
                }
            }
        }
        if let Some(&g) = self.gate_index.get(&Gate::Connect(conn)) {
            if self.gate_fired[g].is_none() {
                self.gate_fired[g] = Some(now.0);
                self.report.gates_fired.push((Gate::Connect(conn), now.0));
            }
        }
        let from = self.addr((spec.server_port & !1) + 1);
        for bytes in sends {
            self.out.push(Datagram {
                from,
                to: client,
                bytes,
            });
        }
    }

    fn next_switch_stamp(&mut self) -> u32 {
        self.switch_stamp += 1;
        self.switch_stamp
    }

    fn data(
        &mut self,
        to: SocketAddr,
        from: SocketAddr,
        p: &ParsedPacket,
        len: usize,
        now: LocalTime,
    ) {
        let candidates: Vec<usize> = self.conn_of_pair(to.port()).collect();
        let Some(conn) = candidates.into_iter().find(|i| {
            self.conns[*i]
                .as_ref()
                .is_some_and(|l| l.ep.peer == from && l.connected && !l.closed)
        }) else {
            return;
        };
        let Some(live) = self.conns[conn].as_mut() else {
            return;
        };
        let incoming = match live.ep.feed(p, len, now) {
            Ok(inc) => inc,
            Err(_) => {
                self.report.rejected += 1;
                return;
            }
        };
        if incoming.disconnect {
            live.closed = true;
            self.report.client_goodbyes[conn] = true;
            self.report
                .log
                .push((now.0, conn, "the client said goodbye"));
        }
        for blob in incoming.blobs {
            self.client_message(conn, blob, now);
        }
    }

    fn client_message(&mut self, conn: usize, blob: CompletedBlob, now: LocalTime) {
        let raw = blob.payload;
        let mtype = message_type(&raw);
        let nth = {
            let n = self.client_seen.entry(mtype).or_default();
            *n += 1;
            *n - 1
        };
        self.report.received[conn].push(Sent {
            queue: blob.queue_id,
            blob_id: blob.id.0,
            raw: raw.clone(),
        });
        if GATING.contains(&mtype) {
            match self.gate_index.get(&Gate::Client { mtype, nth }) {
                Some(&g) => {
                    if self.gate_fired[g].is_none() {
                        self.gate_fired[g] = Some(now.0);
                        self.report
                            .gates_fired
                            .push((Gate::Client { mtype, nth }, now.0));
                    }
                }
                None => self.report.unmatched_gates.push((mtype, nth)),
            }
        }
        if let Some(key) = data_request(&raw) {
            let recorded = self.plan.data_answers.get(&key).map(|v| {
                let used = self.data_used.entry(key).or_default();
                let m = v[(*used).min(v.len() - 1)].clone();
                *used += 1;
                m
            });
            let server_id = self.script.conns[conn].server_id;
            let answer = recorded.unwrap_or_else(|| {
                let blob_id = made_blob_id(server_id, self.made);
                self.made += 1;
                let raw = data_refusal(key.0, key.1);
                Msg {
                    conn,
                    c2s: false,
                    queue: 5,
                    blob_id,
                    mtype: message_type(&raw),
                    raw,
                }
            });
            self.report.sent[conn].push(Sent {
                queue: answer.queue,
                blob_id: answer.blob_id,
                raw: answer.raw.clone(),
            });
            if let Some(live) = self.conns[conn].as_mut() {
                live.ep.send_blob(answer.queue, answer.blob_id, answer.raw);
            }
        }
    }

    /// Release what is due, resend connection requests, run every connection. Returns the
    /// datagrams to deliver.
    ///
    /// Items go out in recorded order across the live connections: the earliest pending one goes
    /// first, when its gate has fired and its delay has passed, so a goodbye on one connection never
    /// overtakes the switch away from it on another.
    pub fn tick(&mut self, now: LocalTime) -> Vec<Datagram> {
        // Connection requests not yet answered go again.
        for conn in 0..self.conns.len() {
            let pair = self.script.conns[conn].server_port & !1;
            let even = self.addr(pair);
            let Some(live) = self.conns[conn].as_mut() else {
                continue;
            };
            if !live.closed
                && !live.connected
                && now.0 - live.request_at > HANDSHAKE_RESEND
                && live.requests < 20
            {
                live.request_at = now.0;
                live.requests += 1;
                self.out.push(Datagram {
                    from: even,
                    to: live.ep.peer,
                    bytes: live.request.clone(),
                });
            }
        }
        loop {
            let mut next: Option<(f64, usize)> = None;
            for (conn, c) in self.conns.iter().enumerate() {
                let cursor = match c {
                    Some(l) if l.closed => continue,
                    Some(l) => l.cursor,
                    // A connection the client was referred to but has not made yet holds back
                    // what was recorded after its first item.
                    None if self.referrals.iter().any(|(_, to)| *to == conn) => 0,
                    None => continue,
                };
                if let Some(it) = self.plan.items[conn].get(cursor) {
                    if next.is_none_or(|(t, _)| it.t < t) {
                        next = Some((it.t, conn));
                    }
                }
            }
            let Some((_, conn)) = next else { break };
            let Some(live) = self.conns[conn].as_ref().filter(|l| l.connected) else {
                break;
            };
            let it = &self.plan.items[conn][live.cursor];
            let Some(fired) = self.gate_fired[it.gate] else {
                break;
            };
            if now.0 < fired + it.delay * self.scale {
                break;
            }
            let kind = it.kind.clone();
            let section = !matches!(kind, ItemKind::Msg(_));
            self.release(conn, kind, now);
            if section {
                // A referral, switch or goodbye goes on the wire before anything released after
                // it on another connection, as the recorded server sent them.
                let odd = self.addr((self.script.conns[conn].server_port & !1) + 1);
                if let Some(live) = self.conns[conn].as_mut() {
                    for bytes in live.ep.tick(now) {
                        self.out.push(Datagram {
                            from: odd,
                            to: live.ep.peer,
                            bytes,
                        });
                    }
                }
            }
        }
        for conn in 0..self.conns.len() {
            let pair = self.script.conns[conn].server_port & !1;
            let odd = self.addr(pair + 1);
            let Some(live) = self.conns[conn].as_mut() else {
                continue;
            };
            if live.closed || !live.connected {
                continue;
            }
            for bytes in live.ep.tick(now) {
                self.out.push(Datagram {
                    from: odd,
                    to: live.ep.peer,
                    bytes,
                });
            }
            if live.goodbye_at.is_some_and(|t| now.0 - t > 5.0) {
                live.closed = true;
            }
        }
        std::mem::take(&mut self.out)
    }

    /// Send one item on its connection.
    fn release(&mut self, conn: usize, kind: ItemKind, now: LocalTime) {
        let cookie = match kind {
            ItemKind::Referral { .. } => self.random(),
            _ => 0,
        };
        let stamp = match kind {
            ItemKind::Switch => self.next_switch_stamp(),
            _ => 0,
        };
        let Some(live) = self.conns[conn].as_mut() else {
            return;
        };
        match kind {
            ItemKind::Msg(m) => {
                live.ep.send_blob(m.queue, m.blob_id, m.raw.clone());
                self.report.sent[conn].push(Sent {
                    queue: m.queue,
                    blob_id: m.blob_id,
                    raw: m.raw,
                });
            }
            ItemKind::Referral { to } => {
                let target = &self.script.conns[to];
                let r = Referral {
                    cookie,
                    addr: SocketAddrV4::new(self.ip, target.server_port & !1),
                    id_server: target.server_id,
                    family: 2,
                };
                live.ep
                    .send_section(PacketFlags::REFERRAL, r.to_bytes().to_vec());
                self.referrals.push((cookie, to));
                self.report.referrals += 1;
                self.report.log.push((now.0, conn, "referral"));
            }
            ItemKind::Switch => {
                live.ep
                    .send_section(PacketFlags::SERVER_SWITCH, switch_body(stamp));
                self.report.switches += 1;
                self.report.log.push((now.0, conn, "switch"));
            }
            ItemKind::Silence => {
                live.closed = true;
                self.report.silences += 1;
                self.report.log.push((now.0, conn, "silent"));
            }
            ItemKind::Goodbye => {
                live.ep.send_section(
                    PacketFlags::NET_ERROR_DISCONNECT,
                    NetErrorCode::None.pack().to_vec(),
                );
                live.goodbye_at = Some(now.0);
                self.report.goodbyes += 1;
                self.report.log.push((now.0, conn, "goodbye"));
            }
        }
        live.cursor += 1;
    }
}

/// A world switch's body: its stamp, then the world-switch type.
fn switch_body(stamp: u32) -> Vec<u8> {
    let mut body = stamp.to_le_bytes().to_vec();
    body.extend_from_slice(&0u32.to_le_bytes());
    body
}

/// A blob id for a message the double makes itself, outside the recorded ranges.
fn made_blob_id(server_id: u16, n: u32) -> u64 {
    let high = 0x2300_0000 | (u32::from(server_id & 0xFF) << 16) | ((0xE000 + n) & 0xFFFF);
    (u64::from(high) << 32) | u64::from(0xE000_0000u32.wrapping_add(n))
}
