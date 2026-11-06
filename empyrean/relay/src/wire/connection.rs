//! The ac1999 data connection: sequencing, keys, reassembly, flow reports and resends, as the
//! November 1999 client runs them. No application decoding.
//!
//! All clocks are supplied by the caller. A rejected datagram commits nothing: no key, sequence,
//! partial message, flow count or liveness stamp. Bounds marked as policy are this client's own;
//! the rest are the original client's.
//!
//! **Sequences and keys.** The server's first accepted data sequence is two. Each sequence draws
//! the next key from the inbound stream in order. A packet that skips ahead draws keys for the
//! sequences it skipped and holds them until those packets arrive, are given up by the server, or
//! are asked for again. A sequenced resend request or flow packet announces the sender's latest
//! sequence, which the receiver then waits for. Messages reach the caller in sequence order.
//!
//! **Flow.** Each data and control packet the client sends carries, after its header and outside
//! the header's length, the number of bytes received since the server's time byte last advanced.
//! When it advances, the client reports the finished period's count in a packet of its own.

use std::collections::{BTreeMap, VecDeque};
use std::net::SocketAddr;

use dereth_primitives::{LocalTime, NetQueue};

use super::{
    body_hash, encode_generic, encode_with_flow, flags, incoming_queue, Error as CodecError,
    Header, KeyStream, Packet, OUTGOING_QUEUES,
};
use dereth_transport::{Fragment, FragmentHeader};

/// Policy: how far a packet may skip ahead, and how many sent packets are kept for resends.
pub const WINDOW: u32 = 256;
/// The largest dat file the client takes in a download is under four megabytes.
const MAX_FILE: usize = 0x40_0000;
/// Policy: the fragments one received message may have: enough for the largest file the client
/// takes, compressed (which can grow it a little) and with its message header.
pub const MAX_PARTS: usize = (MAX_FILE + MAX_FILE / 64 + 64).div_ceil(PAYLOAD);
/// Policy: the fragments one sent message may have.
pub const SEND_PARTS: usize = 64;
/// Policy: events held for the caller. Received messages past this wait, in order, until polled.
pub const MAX_EVENTS: usize = 64;
const MAX_ASSEMBLIES: usize = 16;
/// Policy: the received bytes held at once, partial and complete: a few of the largest messages.
const MAX_BYTES: usize = 3 * MAX_PARTS * PAYLOAD;
/// The longest list the client writes.
const LIST_LIMIT: usize = 0x71;
/// The longest list the client reads.
const READ_LIMIT: u32 = 0x72;
/// The client asks for missing packets this often while any are missing.
const RESEND_INTERVAL: f64 = 0.25;
/// Without missing packets, the client sends its heartbeat this often.
const HEARTBEAT: f64 = 2.0;
/// Once the server has synchronised the clock, the client asks again after this long.
const TIME_SYNC_INTERVAL: f64 = 60.0;
/// A network error within this long of received data is ignored, as the client ignores it.
const ERROR_GRACE: f64 = 120.0;
/// Policy: silence, or a message that gains no fragment, for this long ends the connection.
const TIMEOUT: f64 = 30.0;
/// Policy: the same sequence is resent at most this often, however often it is asked for.
const RESEND_GUARD: f64 = 0.25;
/// A fragment's payload, so a packet stays inside the datagram budget.
const PAYLOAD: usize = 444;
/// The client's first outbound blob id; each blob takes the next.
const FIRST_BLOB: u64 = 0x8000_0000_0000_0000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("ac1999 packet: {0}")]
    Codec(#[from] CodecError),
    #[error("unexpected endpoint, recipient or table")]
    Peer,
    #[error("invalid local clock")]
    Clock,
    #[error("connection is closed")]
    Closed,
    #[error("connection resource limit exceeded")]
    Capacity,
    #[error("sequence outside the receive window")]
    Sequence,
    #[error("invalid transport control")]
    Control,
    #[error("inconsistent fragment identity or content")]
    Fragment,
}

/// Only a complete, ordered message reaches this boundary. Its body is still dialect-specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: u64,
    pub queue: NetQueue,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A server switch, which advances the connect progress. Its time is the server's clock.
    ServerSwitch {
        server_time: f64,
    },
    /// The first ordinary packet after the grant: the connection is up.
    Connected,
    Message(Message),
    Closed,
}

#[derive(Debug, Clone)]
struct Assembly {
    queue: u16,
    parts: Vec<Option<Vec<u8>>>,
    /// How many of the parts are here, and their bytes.
    have: usize,
    bytes: usize,
    /// When the message last gained a fragment.
    touched: LocalTime,
}

/// A received sequence waiting for its turn: fragments, or nothing to deliver.
#[derive(Debug, Clone)]
enum Frame {
    Data(Vec<Fragment>),
    Empty,
}

#[derive(Debug, Clone)]
struct Receive {
    keys: KeyStream,
    /// The highest sequence a key has been drawn for.
    last: u32,
    /// The highest sequence seen on any packet, which the heartbeat reports.
    highest: u32,
    /// Keys of sequences drawn for but not yet received: the missing packets.
    held: BTreeMap<u32, u32>,
    /// The next sequence to deliver.
    next: u32,
    parked: BTreeMap<u32, Frame>,
    assemblies: BTreeMap<u64, Assembly>,
    completed: VecDeque<u64>,
    events: VecDeque<Event>,
}

impl Receive {
    /// A copy of the sequencing state alone, with nothing parked, assembled or queued: what a
    /// datagram changes before it is known to be accepted. Received messages can be megabytes,
    /// so they are not copied for every packet.
    fn staged(&self) -> Self {
        Self {
            keys: self.keys.clone(),
            last: self.last,
            highest: self.highest,
            held: self.held.clone(),
            next: self.next,
            parked: BTreeMap::new(),
            assemblies: BTreeMap::new(),
            completed: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    /// Take an accepted datagram's staged changes, then deliver what is now in order.
    fn commit(&mut self, staged: Self, now: LocalTime) {
        self.keys = staged.keys;
        self.last = staged.last;
        self.highest = staged.highest;
        self.held = staged.held;
        self.parked.extend(staged.parked);
        for event in staged.events {
            self.push(event);
        }
        self.drain(now);
    }

    /// Notifications beyond twice the message cap are dropped rather than grown without bound.
    fn push(&mut self, event: Event) {
        if self.events.len() < 2 * MAX_EVENTS {
            self.events.push_back(event);
        }
    }

    fn memory(&self) -> usize {
        let assembly: usize = self.assemblies.values().map(|a| a.bytes).sum();
        let parked: usize = self
            .parked
            .values()
            .map(|f| match f {
                Frame::Data(fs) => fs.iter().map(|f| f.payload.len()).sum(),
                Frame::Empty => 0,
            })
            .sum();
        let events: usize = self
            .events
            .iter()
            .map(|e| match e {
                Event::Message(m) => m.bytes.len(),
                _ => 0,
            })
            .sum();
        assembly + parked + events
    }

    /// Draw keys up to `seq`, holding those of the sequences before it. `seq` itself is held too
    /// when it is only announced.
    fn advance(&mut self, seq: u32, announce: bool) -> Result<Option<u32>, Error> {
        if seq <= self.last {
            return Ok(None);
        }
        if seq - self.last > WINDOW || seq == u32::MAX {
            return Err(Error::Sequence);
        }
        for missing in self.last + 1..seq {
            let key = self.keys.next();
            self.held.insert(missing, key);
        }
        let key = self.keys.next();
        self.last = seq;
        if announce {
            self.held.insert(seq, key);
            return Ok(None);
        }
        Ok(Some(key))
    }

    /// The server will not resend `seq`: stop waiting and deliver past it.
    fn skip(&mut self, seq: u32) {
        if self.held.remove(&seq).is_some() && seq >= self.next {
            self.parked.insert(seq, Frame::Empty);
        }
    }

    fn fragment(&mut self, fragment: Fragment, now: LocalTime) -> Result<(), Error> {
        let h = fragment.header;
        let id = h.blob_id();
        let count = usize::from(h.num_frags);
        let index = usize::from(h.blob_num);
        let queue = incoming_queue(h.queue_id)?;
        if count == 0 || count > MAX_PARTS || index >= count || self.completed.contains(&id) {
            return Err(Error::Fragment);
        }
        if !self.assemblies.contains_key(&id) && self.assemblies.len() >= MAX_ASSEMBLIES {
            return Err(Error::Capacity);
        }
        let assembly = self.assemblies.entry(id).or_insert_with(|| Assembly {
            queue: h.queue_id,
            parts: vec![None; count],
            have: 0,
            bytes: 0,
            touched: now,
        });
        if assembly.queue != h.queue_id || assembly.parts.len() != count {
            return Err(Error::Fragment);
        }
        match &assembly.parts[index] {
            Some(previous) if previous != &fragment.payload => return Err(Error::Fragment),
            Some(_) => return Ok(()),
            None => {
                assembly.have += 1;
                assembly.bytes += fragment.payload.len();
                assembly.parts[index] = Some(fragment.payload);
                assembly.touched = now;
            }
        }
        if assembly.have == count {
            let assembly = self.assemblies.remove(&id).ok_or(Error::Fragment)?;
            let bytes: Vec<u8> = assembly.parts.into_iter().flatten().flatten().collect();
            if bytes.len() < 4 {
                return Err(Error::Fragment);
            }
            self.push(Event::Message(Message { id, queue, bytes }));
            self.completed.push_back(id);
            if self.completed.len() > WINDOW as usize {
                self.completed.pop_front();
            }
        }
        Ok(())
    }

    /// Deliver parked sequences in order. A sequence whose fragments could complete more messages
    /// than the caller has room for stays parked, with everything after it, until it is polled.
    /// A fragment that conflicts with what was already assembled is dropped; its packet was the
    /// server's own, so its sequence still counts as received.
    fn drain(&mut self, now: LocalTime) {
        while let Some(frame) = self.parked.get(&self.next) {
            let completions = match frame {
                Frame::Data(fragments) => fragments.len(),
                Frame::Empty => 0,
            };
            if self.events.len() + completions > MAX_EVENTS {
                break;
            }
            if let Some(Frame::Data(fragments)) = self.parked.remove(&self.next) {
                for f in fragments {
                    let _ = self.fragment(f, now);
                }
            } else {
                self.parked.remove(&self.next);
            }
            self.next += 1;
        }
    }
}

/// A data packet kept for resends: its sequence's fragments and their keyed hash.
#[derive(Debug)]
struct Sent {
    fragments: Vec<u8>,
    sum: u32,
    last_sent: LocalTime,
}

/// Packets to send in reply to one received packet; committed with the rest of its effects.
#[derive(Default)]
struct Replies {
    packets: Vec<Vec<u8>>,
    resend: Vec<u32>,
    deny: Vec<u32>,
    close: bool,
}

/// One server connection: its endpoint, its id, the recipient id this client adopted and the
/// table byte the grant set. No socket and no end-of-retail receiver table.
#[derive(Debug)]
pub struct Connection {
    endpoint: SocketAddr,
    server: u32,
    recipient: u32,
    table: u8,
    receive: Receive,
    outgoing_keys: KeyStream,
    /// The last data sequence sent; control packets carry it.
    last_out: u32,
    next_blob: u64,
    sent: BTreeMap<u32, Sent>,
    outgoing: VecDeque<Vec<u8>>,
    /// The server's latest time byte, which outgoing headers repeat.
    time: u8,
    /// Bytes received since the time byte last advanced.
    flow: u32,
    connected: bool,
    last_now: LocalTime,
    last_heard: LocalTime,
    last_data: Option<LocalTime>,
    last_list: LocalTime,
    last_time_sync: Option<LocalTime>,
    time_sync_interval: f64,
    closed: bool,
}

/// Whether time byte `a` is newer than `b`, across the byte's wrap.
fn newer(a: u8, b: u8) -> bool {
    if a.abs_diff(b) < 0x80 {
        a > b
    } else {
        a < b
    }
}

fn word(bytes: &[u8], index: usize) -> Option<u32> {
    let w: [u8; 4] = bytes.get(index * 4..index * 4 + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(w))
}

fn list(sequences: &[u32]) -> Vec<u8> {
    let mut body = u32::try_from(sequences.len())
        .unwrap_or(0)
        .to_le_bytes()
        .to_vec();
    for seq in sequences {
        body.extend(seq.to_le_bytes());
    }
    body
}

impl Connection {
    /// Start after a verified grant. Both directions begin with sequence one accounted for.
    /// # Errors
    /// Rejects a nonfinite initial clock.
    pub fn new(
        endpoint: SocketAddr,
        grant: &super::Grant,
        incoming: KeyStream,
        outgoing: KeyStream,
        now: LocalTime,
    ) -> Result<Self, Error> {
        if !now.0.is_finite() {
            return Err(Error::Clock);
        }
        Ok(Self {
            endpoint,
            server: grant.server,
            recipient: grant.recipient,
            table: grant.table,
            receive: Receive {
                keys: incoming,
                last: 1,
                highest: 1,
                held: BTreeMap::new(),
                next: 2,
                parked: BTreeMap::new(),
                assemblies: BTreeMap::new(),
                completed: VecDeque::new(),
                events: VecDeque::new(),
            },
            outgoing_keys: outgoing,
            last_out: 1,
            next_blob: FIRST_BLOB + 1,
            sent: BTreeMap::new(),
            outgoing: VecDeque::new(),
            time: 1,
            flow: 0,
            connected: false,
            last_now: now,
            last_heard: now,
            last_data: None,
            last_list: now,
            last_time_sync: None,
            time_sync_interval: 0.0,
            closed: false,
        })
    }

    fn clock(&self, now: LocalTime) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if !now.0.is_finite() || now.0 < self.last_now.0 {
            return Err(Error::Clock);
        }
        Ok(())
    }

    fn header(&self, sequence: u32, flags: u32) -> Header {
        Header {
            sequence,
            flags,
            recipient: self.recipient,
            time: self.time,
            table: self.table,
            ..Header::default()
        }
    }

    /// A header-only notice: the acknowledgment, the switch reply, the log-off and the like.
    fn bare(&self, flags: u32) -> Vec<u8> {
        let header = Header {
            time: 0,
            ..self.header(0, flags)
        };
        encode_generic(header, &[]).unwrap_or_default()
    }

    fn time_sync_due(&self, now: LocalTime) -> bool {
        self.last_time_sync
            .is_none_or(|t| now.seconds_since(t) > self.time_sync_interval)
    }

    /// A control packet with the flow word, asking for a clock synchronisation when one is due.
    fn control(&mut self, sequence: u32, flags: u32, body: &[u8], now: LocalTime) -> Vec<u8> {
        let mut flags = flags;
        if self.time_sync_due(now) {
            flags |= flags::TIME_SYNC;
            self.last_time_sync = Some(now);
        }
        encode_with_flow(
            self.header(sequence, flags),
            self.flow,
            body,
            body_hash(body),
        )
        .unwrap_or_default()
    }

    fn data_packet(&self, seq: u32, sent: &Sent, resent: bool) -> Vec<u8> {
        let mut flags = flags::DATA | flags::CHECKSUM;
        if resent {
            flags |= flags::RESENT;
        }
        encode_with_flow(
            self.header(seq, flags),
            self.flow,
            &sent.fragments,
            sent.sum,
        )
        .unwrap_or_default()
    }

    /// Receive one datagram. Nothing is committed unless every check passes.
    /// # Errors
    /// Rejects wrong peers, unsupported flags, malformed framing, corruption or resource excess.
    pub fn feed(&mut self, bytes: &[u8], from: SocketAddr, now: LocalTime) -> Result<(), Error> {
        self.clock(now)?;
        let packet = Packet::decode(bytes)?;
        let h = packet.header;
        // Policy: the endpoint is pinned. The client itself matches only the recipient.
        if from != self.endpoint || h.recipient != self.server || !h.has(flags::CHECKSUM) {
            return Err(Error::Peer);
        }
        let table_ok = h.table == self.table
            || (h.table < self.table && h.has(flags::TIME_SYNC))
            || (h.table > self.table && h.has(flags::DENY));
        if !table_ok {
            return Err(Error::Peer);
        }
        let mut rx = self.receive.staged();
        let mut out = Replies::default();
        let mut time = self.time;
        let mut flow = self.flow;
        let mut time_sync_interval = self.time_sync_interval;
        let mut last_time_sync = self.last_time_sync;
        let mut connected = self.connected;
        let mut data = false;

        if h.has(flags::GRANT) {
            // A repeated grant, or the server's keepalive: acknowledge it again.
            out.packets.push(self.bare(flags::GRANT | flags::CHECKSUM));
        } else if h.has(flags::TIME_SYNC) {
            if !packet.generic_checksum_ok() {
                return Err(CodecError::Checksum.into());
            }
            time_sync_interval = TIME_SYNC_INTERVAL;
        } else if h.has(flags::DISCONNECT) {
            if !packet.bare_checksum_ok() {
                return Err(CodecError::Checksum.into());
            }
            out.packets
                .push(self.bare(flags::DISCONNECT | flags::CHECKSUM));
            out.close = true;
        } else if h.has(flags::NET_ERROR) {
            let recent = self
                .last_data
                .is_some_and(|t| now.seconds_since(t) < ERROR_GRACE);
            if packet.bare_checksum_ok() {
                out.close = !recent;
            } else if packet.generic_checksum_ok() && packet.body.len() == 8 {
                // ac1999's server sends its errors with a body and an ordinary checksum, which
                // the client would not accept; this client ends the session on them.
                out.close = true;
            } else {
                return Err(CodecError::Checksum.into());
            }
        } else {
            // Every other packet counts toward the flow report, and a newer time byte closes the
            // period: its count is reported unless the new byte is one.
            if newer(h.time, time) {
                if h.time != 1 {
                    let report = encode_generic(
                        Header {
                            time: h.time.wrapping_sub(1),
                            ..self.header(0, flags::FLOW_REPORT | flags::CHECKSUM)
                        },
                        &flow.to_le_bytes(),
                    )?;
                    out.packets.push(report);
                }
                time = h.time;
                flow = 0;
            }
            flow = flow.saturating_add(20 + u32::from(h.payload_len));

            if h.has(flags::SERVER_SWITCH) {
                if !packet.generic_checksum_ok() {
                    return Err(CodecError::Checksum.into());
                }
                let server_time = packet
                    .body
                    .get(..8)
                    .and_then(|b| b.try_into().ok())
                    .map_or(0.0, f64::from_le_bytes);
                if !server_time.is_finite() {
                    return Err(Error::Control);
                }
                time_sync_interval = TIME_SYNC_INTERVAL;
                last_time_sync = Some(now);
                out.packets
                    .push(self.bare(flags::SERVER_SWITCH | flags::CHECKSUM));
                rx.push(Event::ServerSwitch { server_time });
            } else if h.has(flags::DENY) {
                if !packet.generic_checksum_ok() {
                    return Err(CodecError::Checksum.into());
                }
                let count = word(packet.body, 0).ok_or(Error::Control)?;
                if count > READ_LIMIT {
                    return Err(Error::Control);
                }
                for i in 1..=count as usize {
                    match word(packet.body, i) {
                        Some(seq) if seq != 0 => rx.skip(seq),
                        _ => {}
                    }
                }
            } else {
                if !connected {
                    connected = true;
                    rx.push(Event::Connected);
                }
                if h.has(flags::SKIP) || h.has(flags::RESEND_REQUEST) || h.has(flags::FLOW) {
                    if !packet.generic_checksum_ok() {
                        return Err(CodecError::Checksum.into());
                    }
                    let count = word(packet.body, 0);
                    if h.has(flags::SKIP) {
                        let count = count.ok_or(Error::Control)?;
                        if count > READ_LIMIT {
                            return Err(Error::Control);
                        }
                        for i in 1..=count as usize {
                            if let Some(seq) = word(packet.body, i) {
                                rx.skip(seq);
                            }
                        }
                    } else {
                        if h.has(flags::RESEND_REQUEST) {
                            let count = count.ok_or(Error::Control)?;
                            if count == 0 || count > READ_LIMIT {
                                return Err(Error::Control);
                            }
                            // The client reads as many words as the count says and the packet
                            // holds; zeros are skipped.
                            for i in 1..=count as usize {
                                match word(packet.body, i) {
                                    Some(seq) if seq != 0 => {
                                        if self.sent.contains_key(&seq) {
                                            out.resend.push(seq);
                                        } else {
                                            out.deny.push(seq);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        // The packet's sequence is the sender's latest, which is now awaited.
                        rx.advance(h.sequence, true)?;
                    }
                } else if h.sequence != 0 {
                    let key = if h.sequence <= rx.last {
                        rx.held.remove(&h.sequence)
                    } else {
                        rx.advance(h.sequence, false)?
                    };
                    rx.highest = rx.highest.max(h.sequence);
                    if let Some(key) = key {
                        let frame = if h.has(flags::DATA) {
                            let fragments = packet.fragments(key)?;
                            for f in &fragments {
                                incoming_queue(f.header.queue_id)?;
                                if usize::from(f.header.num_frags) > MAX_PARTS {
                                    return Err(Error::Capacity);
                                }
                            }
                            data = true;
                            Frame::Data(fragments)
                        } else {
                            Frame::Empty
                        };
                        if h.sequence >= rx.next {
                            rx.parked.insert(h.sequence, frame);
                        }
                    }
                }
            }
        }
        if self.receive.memory() + rx.memory() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        if self.outgoing.len() + out.packets.len() + out.resend.len() + 2 > WINDOW as usize {
            return Err(Error::Capacity);
        }

        self.receive.commit(rx, now);
        self.time = time;
        self.flow = flow;
        self.time_sync_interval = time_sync_interval;
        self.last_time_sync = last_time_sync;
        self.connected = connected;
        self.last_heard = now;
        self.last_now = now;
        if data {
            self.last_data = Some(now);
        }
        self.outgoing.extend(out.packets);
        for seq in out.resend {
            let packet = self.sent.get(&seq).and_then(|sent| {
                (now.seconds_since(sent.last_sent) >= RESEND_GUARD)
                    .then(|| self.data_packet(seq, sent, true))
            });
            if let Some(packet) = packet {
                self.outgoing.push_back(packet);
                if let Some(sent) = self.sent.get_mut(&seq) {
                    sent.last_sent = now;
                }
            }
        }
        if !out.deny.is_empty() {
            out.deny.truncate(LIST_LIMIT);
            let body = list(&out.deny);
            let skip = self.control(0, flags::SKIP | flags::CHECKSUM, &body, now);
            self.outgoing.push_back(skip);
            // ac1999's server reads denials under its own flag, without the flow word.
            let compat: Vec<u32> = out.deny.iter().copied().filter(|&s| s >= 2).collect();
            if !compat.is_empty() {
                let deny = encode_generic(
                    self.header(0, flags::DENY | flags::CHECKSUM),
                    &list(&compat),
                )?;
                self.outgoing.push_back(deny);
            }
        }
        if out.close {
            self.close();
        }
        Ok(())
    }

    /// Queue a message as data packets, split at 444 bytes, each with the next sequence and key.
    /// Only dialect application codecs should call this.
    /// # Errors
    /// Refuses a queue the client never sends on, an empty or oversized message, sequence
    /// exhaustion, closed state or a full outgoing queue.
    pub fn send(&mut self, queue: u16, bytes: &[u8], now: LocalTime) -> Result<(), Error> {
        self.clock(now)?;
        if !OUTGOING_QUEUES.contains(&queue) {
            return Err(CodecError::Queue.into());
        }
        if bytes.len() < 4 || bytes.len() > SEND_PARTS * PAYLOAD {
            return Err(Error::Capacity);
        }
        let count = bytes.len().div_ceil(PAYLOAD);
        let parts = u32::try_from(count).map_err(|_| Error::Capacity)?;
        if self.outgoing.len() + count > WINDOW as usize
            || self.last_out.checked_add(parts + 1).is_none()
        {
            return Err(Error::Capacity);
        }
        let id = self.next_blob;
        self.next_blob = self.next_blob.wrapping_add(1);
        for (index, payload) in bytes.chunks(PAYLOAD).enumerate() {
            self.last_out += 1;
            let seq = self.last_out;
            let fragment = Fragment::new(
                FragmentHeader {
                    blob_id_low: (id & 0xffff_ffff) as u32,
                    blob_id_high: (id >> 32) as u32,
                    num_frags: u16::try_from(count).map_err(|_| Error::Capacity)?,
                    blob_num: u16::try_from(index).map_err(|_| Error::Capacity)?,
                    queue_id: queue,
                    ..Default::default()
                },
                payload.to_vec(),
            );
            let sent = Sent {
                sum: fragment.hash() ^ self.outgoing_keys.next(),
                fragments: fragment.to_bytes(),
                last_sent: now,
            };
            self.outgoing.push_back(self.data_packet(seq, &sent, false));
            self.sent.insert(seq, sent);
        }
        while self.sent.len() > WINDOW as usize {
            self.sent.pop_first();
        }
        self.last_now = now;
        Ok(())
    }

    /// Run the client's timers with a caller clock: a resend request every quarter second while
    /// packets are missing, otherwise a heartbeat every two seconds. No sleeps or socket calls.
    /// # Errors
    /// Refuses an invalid clock, closed state or a full output queue.
    pub fn tick(&mut self, now: LocalTime) -> Result<(), Error> {
        self.clock(now)?;
        if now.seconds_since(self.last_heard) >= TIMEOUT
            || self
                .receive
                .assemblies
                .values()
                .any(|a| now.seconds_since(a.touched) >= TIMEOUT)
        {
            self.close();
            return Ok(());
        }
        if self.outgoing.len() + 1 > WINDOW as usize {
            return Err(Error::Capacity);
        }
        let missing: Vec<u32> = self.receive.held.keys().copied().take(LIST_LIMIT).collect();
        let since = now.seconds_since(self.last_list);
        if !missing.is_empty() && since > RESEND_INTERVAL {
            let packet = self.control(
                self.last_out,
                flags::RESEND_REQUEST | flags::CHECKSUM,
                &list(&missing),
                now,
            );
            self.outgoing.push_back(packet);
            self.last_list = now;
        } else if missing.is_empty() && since >= HEARTBEAT {
            let body = self.receive.highest.to_le_bytes();
            let packet = self.control(self.last_out, flags::FLOW | flags::CHECKSUM, &body, now);
            self.outgoing.push_back(packet);
            self.last_list = now;
        }
        self.last_now = now;
        Ok(())
    }

    /// Local teardown: discard queued work and keep only a closed notification.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.sent.clear();
        self.outgoing.clear();
        self.receive.held.clear();
        self.receive.parked.clear();
        self.receive.assemblies.clear();
        self.receive.events.clear();
        self.receive.completed.clear();
        self.receive.events.push_back(Event::Closed);
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The next event. Taking one makes room for messages held back at the cap.
    pub fn poll(&mut self) -> Option<Event> {
        let event = self.receive.events.pop_front();
        if !self.closed {
            self.receive.drain(self.last_now);
        }
        event
    }

    /// The log-off, as the client sends it, followed by the reset that ac1999's server ends a
    /// session on. The caller owns transmission and teardown.
    #[must_use]
    pub fn goodbye(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        if self.closed {
            return Vec::new();
        }
        [
            self.bare(flags::LOGOFF | flags::CHECKSUM),
            self.bare(flags::RESET | flags::CHECKSUM),
        ]
        .into_iter()
        .map(|bytes| (bytes, self.endpoint))
        .collect()
    }

    pub fn take_outgoing(&mut self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.outgoing
            .drain(..)
            .map(|bytes| (bytes, self.endpoint))
            .collect()
    }

    /// Drain one packet so an enclosing transport can preserve its own output bound.
    pub fn pop_outgoing(&mut self) -> Option<(Vec<u8>, SocketAddr)> {
        self.outgoing
            .pop_front()
            .map(|bytes| (bytes, self.endpoint))
    }
}

#[cfg(test)]
mod tests;
