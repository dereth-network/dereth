//! `FlowQueue` — the send path: coalescing, sequence numbers, the 0.5 s interval and the cadences.
//!
//! **There is no outbound flow control.** Capacity checks for both wire bytes and queued fragments
//! always succeed. Queuing and dequeuing bytes do not update any accounting state.
//! Sending bytes accumulates a counter that nothing reads, and the parsed `-noflowqueue` switch
//! is also unused. These observations mean that neither capacity checks nor byte accounting
//! throttle outgoing packets: the client sends as fast as the queue fills.
//! The `Flow` header and the interval machinery remain **pure telemetry for the server**;
//! they do not impose an outbound rate limit.
//!
//! Making [`wire_room_left`] return a real value is a behaviour *change* and does not belong in
//! the default build.
//!
//! See `docs/networking/02-reliability-and-flow.md` §§6, 9.

use std::collections::VecDeque;

use dereth_primitives::LocalTime;

use crate::blob::NetBlob;
use crate::isaac::CryptoSystem;
use crate::session::SentPacketHistory;
use crate::wire::packet::netpacket_flags;
use crate::wire::{Fragment, OutPacket, ProtoHeader, WireError, MAX_BUILD_PAYLOAD};

// ---------------------------------------------------------------------------------------------
// The cadence table. A server expecting retail cadence otherwise sees the client as idle or as
// flooding. Every one of these is a wire-visible constant, not a tuning parameter.
// ---------------------------------------------------------------------------------------------

/// The flow queue advances the interval counter in steps of this.
pub const INTERVAL_SECONDS: f64 = 0.5;

/// The ACK cadence: one cumulative ACK every 2 seconds while nothing is missing.
/// The test is `>=`.
pub const ACK_INTERVAL: f64 = 2.0;

/// The NAK cadence: a NAK burst at most every 0.6 seconds. The test is `>`.
pub const NAK_INTERVAL: f64 = 0.6;

/// TimeSync + EchoRequest every 6 intervals — 3 seconds.
pub const TIMESYNC_ECHO_INTERVALS: u32 = 6;

/// A no-op command packet ([`crate::conn::IcmdCommand::Nop`]) to port + 1 every 220 (`0xDC`)
/// intervals — 110 seconds.
pub const ICMD_INTERVALS: u32 = 220;

/// 140 seconds of silence ends the connection. Note the
/// asymmetry — ACE's default session timeout is 60 seconds.
pub const CONNECTION_TIMEOUT: f64 = 140.0;

/// Connect-ack resend cadence. The retail constant, a double.
pub const HANDSHAKE_RESEND: f64 = 0.333_333_333;

/// The logon retry: LoginRequest every 2 s, 20 tries (40 s).
pub const LOGIN_RESEND: f64 = 2.0;
/// See [`LOGIN_RESEND`].
pub const LOGIN_RESEND_TRIES: u32 = 20;

/// The referral-queue walk: WorldLoginRequest every 0.333333333 s.
/// The same constant the connect-ack resend uses.
///
/// The resend time is this constant added to local time and stored on the entry.
pub const WORLD_LOGIN_RESEND: f64 = 0.333_333_333;

/// The world-login flow's give-up window, a retail double constant.
pub const WORLD_LOGIN_TIMEOUT: f64 = 280.0;

/// The cap the referral-queue walk compares its sent-auth count against, **as the client computes
/// it** -- a run-time division of two double constants, not a written-out integer:
///
/// The client divides the 280.0 constant by the 0.333333333 one at run time and compares
/// the quotient against the sent-auth count, giving up when it is less than or equal.
///
/// The quotient is **840.00000084**, not 840, because 0.333333333 is a hair below a third. The
/// check runs *before* the counter is incremented, so the entry is still eligible at
/// a count of 840 and the client therefore sends **841** `WorldLoginRequest`s over 840
/// intervals -- 280.0 s exactly, which is what the pair of constants was chosen to mean. Kept as
/// the division rather than as `841` so the two constants stay the only authority.
#[must_use]
pub fn world_login_cap() -> f64 {
    WORLD_LOGIN_TIMEOUT / WORLD_LOGIN_RESEND
}

/// The number of `WorldLoginRequest` **intervals** in the give-up window: `280.0 / 0.333333333`
/// truncated. The number of datagrams is one more; see [`world_login_cap`].
pub const WORLD_LOGIN_TRIES: u32 = 840;

/// Link-status snapshot cadence, current world server only.
pub const HEARTBEAT_INTERVAL: f64 = 2.0;

/// Disconnect step 4: in the disconnect-received state, send `NetErrorDisconnect` once the send
/// queue is empty **or** this long has passed.
pub const DISCONNECT_GRACE: f64 = 10.0;

/// Wire-room query. In retail it always answers yes (`return 1;`).
///
/// It is `const fn` returning `true` on purpose. Do not "fix" it: see the module docs.
#[must_use]
pub const fn wire_room_left() -> bool {
    true
}

/// Fragment-queue-room query. It also always answers yes.
#[must_use]
pub const fn frag_queue_room_left() -> bool {
    true
}

/// What crossing an interval boundary fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalEvent {
    /// Enqueue a `TimeSync` header with computed game time **and** an `EchoRequest` header with
    /// local time. Every 6 intervals, i.e. every 3 seconds.
    TimeSyncAndEcho,
    /// Send a standalone no-op command packet to the server address **port + 1**.
    /// Every 220 intervals, i.e. every 110 seconds.
    IcmdKeepAlive,
}

/// One `FlowQueue` per recipient.
#[derive(Debug)]
pub struct FlowQueue {
    /// `highest_id_sent`, initialised to **1** by flow-queue initialization. It skips 0 on
    /// wrap and is **not advanced** by an unsequenced control packet.
    pub highest_id_sent: u32,
    /// our peer's id for us, written into every outgoing `rec_id`.
    pub net_id: u16,
    /// Our own receiver record's `iteration`.
    pub iteration: u16,
    /// The current local interval's id, a `u16` written into every outgoing `interval`.
    pub cur_local_interval: u16,
    /// `interval_time` — the next boundary.
    interval_time: LocalTime,
    /// Whether `interval_time` has been primed.
    interval_started: bool,
    /// Packets built and not yet sent.
    waiting: VecDeque<OutPacket>,
    /// The blob queue, a min-heap keyed by blob priority. **Lower priority values are sent
    /// first.** Kept as a sorted `Vec` because the queue is short and the order must be stable.
    blob_queue: Vec<NetBlob>,
    /// The ack list — sequence ids the peer NAKed and we hold cached.
    ack_list: Vec<u32>,
    /// The empty-ack list — sequence ids the peer NAKed that we cannot recover.
    empty_ack_list: Vec<u32>,
    /// `SentPacketHistory`.
    pub sent: SentPacketHistory,
    /// Bytes sent in the current local interval, accumulated by packet transmission. Nothing reads
    /// it.
    bytes_sent_this_interval: u32,
    /// What the time-sensitive sections say when a packet carrying them is sent.
    time_sensitive: TimeSensitive,
    /// Sections queued since the last send, placed on packets when the next one is built.
    pending_sections: Vec<(u32, Vec<u8>)>,
}

/// The current values of the two time-sensitive sections the client sends.
///
/// A packet carrying `TimeSync` or `EchoRequest` has those sections rewritten from these values
/// immediately before every send and resend, so a section that waited on a held packet still
/// carries the time it is sent at rather than the time it was queued.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TimeSensitive {
    /// `TimeSync`'s value: the game time, in seconds.
    pub game_time: f64,
    /// `EchoRequest`'s value: the local time, in seconds.
    pub local_time: f32,
}

impl FlowQueue {
    #[must_use]
    pub fn new(net_id: u16, iteration: u16) -> Self {
        Self {
            highest_id_sent: 1,
            net_id,
            iteration,
            cur_local_interval: 0,
            interval_time: LocalTime::default(),
            interval_started: false,
            waiting: VecDeque::new(),
            blob_queue: Vec::new(),
            ack_list: Vec::new(),
            empty_ack_list: Vec::new(),
            sent: SentPacketHistory::new(),
            bytes_sent_this_interval: 0,
            time_sensitive: TimeSensitive::default(),
            pending_sections: Vec::new(),
        }
    }

    /// Set the values the time-sensitive sections are rewritten from at the next send.
    pub fn set_time_sensitive(&mut self, values: TimeSensitive) {
        self.time_sensitive = values;
    }

    /// Rewrite a packet's time-sensitive sections from the current values.
    fn refresh_time_sensitive(&self, packet: &mut OutPacket) {
        if packet.flags & netpacket_flags::TIME_SENSITIVE == 0 {
            return;
        }
        if let Some(b) = packet
            .optional
            .get_mut(&crate::wire::PacketFlags::TIME_SYNC)
        {
            *b = self.time_sensitive.game_time.to_le_bytes().to_vec();
        }
        if let Some(b) = packet
            .optional
            .get_mut(&crate::wire::PacketFlags::ECHO_REQUEST)
        {
            *b = self.time_sensitive.local_time.to_le_bytes().to_vec();
        }
    }

    /// a blob for transmission. Lower `priority` is sent first;
    /// The UI blob send uses priority 5 for all UI-originated traffic.
    pub fn enqueue_blob(&mut self, blob: NetBlob) {
        let at = self
            .blob_queue
            .partition_point(|b| b.priority <= blob.priority);
        self.blob_queue.insert(at, blob);
    }

    /// Enqueue an optional header.
    ///
    /// Optional headers wait apart from the packets, and are placed on packets in mask order when
    /// the next packets are built ([`Self::place_sections`]); within a packet `OutPacket`'s
    /// `BTreeMap` keeps them in wire order. A packet that starts with optional headers goes to the
    /// **head** of the waiting list and a pure-fragment packet to the tail, so control traffic
    /// overtakes bulk data.
    ///
    /// # Errors
    /// None at present; see [`Self::enqueue_optional_headers`].
    pub fn enqueue_optional_header(&mut self, mask: u32, data: Vec<u8>) -> Result<(), WireError> {
        self.enqueue_optional_headers(std::slice::from_ref(&(mask, data)))
    }

    /// Several sections at once.
    ///
    /// They wait, with every other section queued since the last send, until the next
    /// [`Self::transmit_new_packets`] places them; see [`Self::place_sections`].
    ///
    /// # Errors
    /// None at present: the sections are only queued here, and the section limit is enforced
    /// when they are placed.
    pub fn enqueue_optional_headers(
        &mut self,
        sections: &[(u32, Vec<u8>)],
    ) -> Result<(), WireError> {
        self.pending_sections.extend(sections.iter().cloned());
        Ok(())
    }

    /// Place the queued sections on packets, as the original's header heap is drained when the
    /// packets are built: one at a time, in ascending mask order (the order is stable for equal
    /// masks).
    ///
    /// Each section joins the first waiting packet of sections alone that can take it: one not
    /// yet checksummed, not a lone exclusive section, not already carrying that mask, and with
    /// room for it. Only when there is none does it start a packet of its own, at the head of the
    /// waiting list, so control traffic still overtakes queued data; the sections after it then
    /// join that packet. An exclusive section always starts its own. (Data fragments join a
    /// waiting packet of sections the other way round, as they are coalesced.) So sections queued
    /// together normally leave together: the periodic packet carries `AckSequence`, `TimeSync`,
    /// `EchoRequest` and `Flow` at once, `header = 0x0B004002`.
    ///
    /// Together with the rule that a packet whose only sections are `TimeSync`, `EchoRequest` and
    /// `Flow` is not sent while it is the last one waiting (see [`Self::transmit_new_packets`]),
    /// this is what the recorded sessions show. Those three wait on a held packet until the
    /// two-second `AckSequence` or a data fragment joins them, or a newer packet queues ahead of
    /// them. A second `Flow` report arriving while one is held takes a packet of its own at the
    /// head, and so goes out first, alone and sequenced (`header = 0x08000002`), while the
    /// acknowledgement joins the older one.
    fn place_sections(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_sections);
        pending.sort_by_key(|(mask, _)| *mask);
        for (mask, data) in pending {
            let exclusive = crate::wire::optional::spec_for(mask)
                .is_some_and(|s| s.flags & crate::wire::optional::flags::EXCLUSIVE != 0);
            let host = if exclusive {
                None
            } else {
                self.waiting.iter().position(|p| {
                    p.flags & netpacket_flags::CHECKSUM_DONE == 0
                        && p.fragments.is_empty()
                        && !is_lone_exclusive(p)
                        && !p.optional.contains_key(&mask)
                        && p.optional.len() < crate::wire::MAX_OPTIONAL_HEADERS
                        && p.payload_len() + data.len() <= MAX_BUILD_PAYLOAD
                })
            };
            match host {
                Some(i) => {
                    let _ = self.waiting[i].add_optional_header(mask, data);
                }
                None => {
                    let mut packet = OutPacket::new(self.header_template(0));
                    let _ = packet.add_optional_header(mask, data);
                    self.enqueue_packet_at_head(packet);
                }
            }
        }
    }

    fn header_template(&self, seq_id: u32) -> ProtoHeader {
        ProtoHeader {
            seq_id,
            rec_id: self.net_id,
            interval: self.cur_local_interval,
            iteration: self.iteration,
            ..Default::default()
        }
    }

    /// Insert at the head, but skip over an already-checksummed
    /// head, because that packet's bytes are settled and reordering it would change what has
    /// effectively been committed.
    fn enqueue_packet_at_head(&mut self, packet: OutPacket) {
        let skip = usize::from(
            self.waiting
                .front()
                .is_some_and(|p| p.flags & netpacket_flags::CHECKSUM_DONE != 0),
        );
        self.waiting.insert(skip, packet);
    }

    /// Coalesce queued blobs into packets.
    ///
    /// Each queued blob is dequeued in turn and fragmented. Each fragment goes into the first
    /// waiting packet that has `flags` bit 0 clear, whose `size` plus the fragment's size is
    /// below `0x1D1`, and that is not a single exclusive optional header; when there is none, a new
    /// send packet is created, the fragment added, and the packet enqueued at the tail.
    ///
    /// The `< 0x1D1` bound is on the **payload**, so a full datagram is 20 + 464 = 484 bytes.
    pub fn coalesce_data(&mut self) {
        let blobs = std::mem::take(&mut self.blob_queue);
        for blob in blobs {
            for frag in blob.fragmentize() {
                self.append_fragment(frag);
            }
        }
    }

    fn append_fragment(&mut self, frag: Fragment) {
        let need = frag.wire_len();
        let slot = self.waiting.iter().position(|p| {
            p.flags & netpacket_flags::CHECKSUM_DONE == 0
                && p.payload_len() + need < MAX_BUILD_PAYLOAD + 1
                && !is_lone_exclusive(p)
                && p.fragments.len() < crate::wire::MAX_FRAGS_PER_PACKET
        });
        match slot {
            Some(i) => {
                // The bounds were just checked, so this cannot fail.
                let _ = self.waiting[i].add_fragment(frag);
            }
            None => {
                let mut packet = OutPacket::new(self.header_template(0));
                let _ = packet.add_fragment(frag);
                // A pure-fragment packet goes to the tail.
                self.waiting.push_back(packet);
            }
        }
    }

    /// Transmit the new packets.
    ///
    /// Returns the datagrams to hand to `sendto`, in order. `crypto` is `crypto_outgoing`.
    ///
    /// The sequence-number rules, which a server relies on and which ACE special-cases on exactly
    /// this basis:
    ///
    /// - a packet with `flags & 4` clear — no fragments, only disposable headers, i.e. a pure
    ///   ACK / NAK / control packet — **reuses** `highest_id_sent` and is sent in the clear;
    /// - otherwise `highest_id_sent += 1`, forced to 1 if it wrapped to 0, and the packet is
    ///   encrypted and cached for retransmit.
    pub fn transmit_new_packets(
        &mut self,
        crypto: &mut CryptoSystem,
        now: LocalTime,
    ) -> Vec<Vec<u8>> {
        self.place_sections();
        self.coalesce_data();
        let mut out = Vec::new();
        while let Some(mut packet) = self.waiting.pop_front() {
            // Stop when there is no wire room and the packet lacks flag 8 -- never taken.
            if !wire_room_left() && packet.flags & netpacket_flags::PRIORITY == 0 {
                self.waiting.push_front(packet);
                break;
            }

            // Stop when `header & 0xFFFFFF` is 0 and this is the last waiting packet -- do not
            // send an empty tail packet. The test is made on the fragment bit and the section
            // masks alone, before the encrypted-checksum bit is added, so a packet holding only
            // `TimeSync`, `EchoRequest` and `Flow` (all above `0xFFFFFF`) is held although it
            // will be encrypted when it goes.
            let flags = packet.computed_flags().0 & !crate::wire::PacketFlags::ENCRYPTED_CHECKSUM;
            if flags & 0x00FF_FFFF == 0 && self.waiting.is_empty() {
                self.waiting.push_front(packet);
                break;
            }
            self.refresh_time_sensitive(&mut packet);

            packet.header.rec_id = self.net_id;
            packet.header.interval = self.cur_local_interval;
            packet.header.iteration = self.iteration;

            let key = if packet.needs_encryption() {
                // Reuse a stored key only for a packet that already has a sequence number and had
                // been checksummed before -- that is the retransmit path.
                let reuse = packet.crypto_key.filter(|_| {
                    packet.header.seq_id != 0 && packet.flags & netpacket_flags::CHECKSUM_DONE != 0
                });
                match reuse {
                    Some(k) => Some(k),
                    None => {
                        self.highest_id_sent = self.highest_id_sent.wrapping_add(1);
                        if self.highest_id_sent == 0 {
                            self.highest_id_sent = 1;
                        }
                        packet.header.seq_id = self.highest_id_sent;
                        Some(crypto.next())
                    }
                }
            } else {
                // Reuse, do not advance.
                packet.header.seq_id = self.highest_id_sent;
                None
            };

            let Ok(bytes) = packet.serialize(key) else {
                continue;
            };
            self.bytes_sent_this_interval = self
                .bytes_sent_this_interval
                .wrapping_add(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
            out.push(bytes);

            if packet.needs_encryption() {
                self.sent.add_sent_packet(packet, now);
            }
        }
        out
    }

    /// Enqueue an empty ack — "stop asking for this one, it is gone".
    ///
    /// The client keeps the empty-ack list (a FIFO) and a hash table of the same ids side by side,
    /// and this function is a no-op when the hash table already holds the id.
    /// The dequeue removes from **both**, so a single `Vec` with a membership test
    /// is an exact model of the pair: an id can be rejected again once its first rejection has gone
    /// out on the wire, but never queued twice while one is pending.
    ///
    /// Private because both call sites are in this file, exactly as in the original: `enqueue_acks`
    /// (the id was never cached, or has been flushed) and `transmit_acks` (the cached packet has
    /// nothing left to resend).
    fn enqueue_empty_ack(&mut self, id: u32) {
        if !self.empty_ack_list.contains(&id) {
            self.empty_ack_list.push(id);
        }
    }

    /// A NAK arrived from the peer.
    ///
    /// For each requested id: if the retransmit cache still has it, put it on the ack list; if not,
    /// it is unrecoverable, so put it on the empty-ack list and tell the peer to stop asking.
    ///
    /// # This is a merge of two ascending lists, not a filter and an append
    ///
    /// The original walks the ack list with a list cursor and `requested` with an index at the same
    /// time, and does one of three things at each step:
    ///
    /// | comparison | original | here |
    /// |---|---|---|
    /// | `want == entry` | advance both (already queued) | same |
    /// | `want` older than `entry` | insert `want` before the cursor, keep `entry` | push `want`, keep `entry` |
    /// | `want` newer than `entry` | **remove at the cursor** — drop `entry` | drop `entry` |
    ///
    /// and stops the moment `requested` runs out, leaving the rest of the ack list alone.
    ///
    /// **The third row is not `retain(|id| !lhs_newer(first, *id))`** — drop everything older than
    /// the *first* requested id. That gives the same answer whenever the ack list holds nothing
    /// between the first and last requested id, and a different one otherwise: for an ack list of
    /// `[5, 10]` and `requested = [3, 12]` the client ends with `[3, 12]` and the filter ends with
    /// `[3, 5, 10, 12]`, resending two packets the peer had implicitly acknowledged.
    ///
    /// A NAK also implicitly acknowledges everything below its first id. That is the NAK
    /// processor's doing, not this function's, and it lands in
    /// `dereth_client_net::net::Connection::flush_num`.
    ///
    /// Returns the first requested id, which is what the caller feeds to the implicit-ACK rule.
    pub fn enqueue_acks(&mut self, requested: &[u32]) -> Option<u32> {
        let first = *requested.first()?;
        let existing = std::mem::take(&mut self.ack_list);
        let mut merged: Vec<u32> = Vec::with_capacity(existing.len() + requested.len());
        let mut cur = 0usize;
        let mut i = 0usize;
        while i < requested.len() && cur < existing.len() {
            let entry = existing[cur];
            let want = requested[i];
            if want == entry {
                merged.push(entry);
                cur += 1;
                i += 1;
            } else if crate::session::lhs_newer(entry, want) {
                // `want` sorts before `entry`, so it is inserted here.
                if self.sent.contains(want) {
                    merged.push(want);
                } else {
                    self.enqueue_empty_ack(want);
                }
                i += 1;
            } else {
                // Remove the current entry: `entry` is older than an id the peer is still asking for, so the
                // peer has implicitly acknowledged it.
                cur += 1;
            }
        }
        // Whichever list is left over. Only one of these two can be non-empty.
        merged.extend_from_slice(&existing[cur..]);
        while i < requested.len() {
            let want = requested[i];
            if self.sent.contains(want) {
                merged.push(want);
            } else {
                self.enqueue_empty_ack(want);
            }
            i += 1;
        }
        self.ack_list = merged;
        Some(first)
    }

    /// A cumulative ACK retires what it covers.
    ///
    /// Two steps, in this order: drop the **prefix** of the ack list that is older than the flush number
    /// (the peer has acknowledged those, so resending them would be pure waste), then
    /// flush the sent-packet cache itself. Returns how many cached packets
    /// went, so a caller can assert the retirement happened rather than assume it.
    ///
    /// The recipient's per-frame step calls this **after emptying the flow queue**, which is what
    /// makes a datagram carrying both `AckSequence(N)` and `RequestRetransmit(<N)` still resend:
    /// the flush is deferred by a whole tick, on purpose.
    ///
    /// The observed flush also has an unconditional drop-all mode with one caller.
    /// Dropping the whole `FlowQueue` has the same effect here, so the separate mode is
    /// not modelled.
    pub fn flush_sent_packets(&mut self, flush_num: u32) -> usize {
        // Pop the head until it equals `flush_num` or `flush_num` is not newer than it -- the list
        // is ascending, so this is a prefix and never a scan.
        while let Some(&head) = self.ack_list.first() {
            if head == flush_num || !crate::session::lhs_newer(flush_num, head) {
                break;
            }
            self.ack_list.remove(0);
        }
        self.sent.flush(flush_num)
    }

    /// Resend everything the peer NAKed.
    ///
    /// Each resend reuses the cached packet's **stored** `crypto_key`; see
    /// [`crate::session::reserialize_for_retransmit`]. A cached packet that became empty (all its
    /// optional headers were disposable and were stripped when it was cached) is not resent —
    /// `enqueue_empty_ack` is called instead.
    ///
    /// **There is no retry bound and no retry counter anywhere on this path.** `transmit_acks` loops
    /// on until the ack list is empty and nothing counts how often a given
    /// sequence has been resent; the only back-pressure in the original is `wire_room_left`, which
    /// leaves the id on the list for the next tick and which this build folds to `return 1` (see
    /// the module docs). A resend loop is bounded only by the peer eventually stopping
    /// asking, or by the packet ageing out of [`crate::session::SentPacketHistory`] and being answered
    /// with `RejectRetransmit` instead.
    pub fn transmit_acks(&mut self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let ids = std::mem::take(&mut self.ack_list);
        for id in ids {
            let Some(cached) = self.sent.get(id) else {
                self.enqueue_empty_ack(id);
                continue;
            };
            let mut resend = cached.clone();
            if resend.payload_len() == 0 {
                self.enqueue_empty_ack(id);
                continue;
            }
            resend.header.interval = self.cur_local_interval;
            resend.header.iteration = self.iteration;
            self.refresh_time_sensitive(&mut resend);
            if let Ok(bytes) = crate::session::reserialize_for_retransmit(&mut resend) {
                out.push(bytes);
            }
        }
        out
    }

    /// Up to 114 ids into a `RejectRetransmit` section.
    ///
    /// ( is the caller; the name is misleading — it sends
    /// `RejectRetransmit`, not `RequestRetransmit`.)
    ///
    /// # A retail quirk that is deliberately *not* reproduced
    ///
    // UNVERIFIED: how often the retail client actually emits `RejectRetransmit`.
    /// The original compiles into a 116-entry buffer of ids whose first entry is the count. It
    /// opens with
    ///
    /// ```text
    /// if (empty_num == 0 || buffer[0] != 0) { buffer[0] = 0; }   // and compile nothing
    /// else { buffer[0] = min(empty_num, 0x72); ...drain... }
    /// ```
    ///
    /// Its **only** consumer reads `buffer[0]`, copies the ids
    /// into a sequence-id list header and never clears the count. The count is written in exactly
    /// two places, both inside this function. So a client with a standing
    /// backlog compiles a section only on every *other* call, and
    /// does not initialise the buffer at all, so whether the very first call compiles depends on
    /// heap contents.
    ///
    /// **An uninitialised read has no behaviour to transcribe**, and the observable consequence is a
    /// half-second delay rather than a lost id — the ids stay on the empty-ack list either way. So
    /// this drains on every call, and the divergence is recorded here rather than reproduced.
    /// Settling it needs a breakpoint on the empty-ack compiler in the retail client with a NAK backlog
    /// standing; the corpus cannot answer it, because it carries **zero** `RejectRetransmit`
    /// headers in either direction (0 of 11,330 datagrams).
    #[must_use]
    pub fn compile_empty_acks(&mut self) -> Option<Vec<u8>> {
        if self.empty_ack_list.is_empty() {
            return None;
        }
        let n = self.empty_ack_list.len().min(crate::session::MAX_NAK_IDS);
        let ids: Vec<u32> = self.empty_ack_list.drain(..n).collect();
        let mut block = Vec::with_capacity(4 + 4 * ids.len());
        block.extend_from_slice(&u32::try_from(ids.len()).unwrap_or(0).to_le_bytes());
        for id in ids {
            block.extend_from_slice(&id.to_le_bytes());
        }
        Some(block)
    }

    /// The flow queue's interval advance, plus the client queue's own local-interval bump.
    ///
    /// ```text
    /// n = 0
    /// while (interval_time < local_time) { interval_time += 0.5; ++n; }
    /// if (n) advance the local interval by n
    /// ```
    ///
    /// Returns whatever crossing those boundaries fires.
    pub fn advance_interval(&mut self, now: LocalTime) -> Vec<IntervalEvent> {
        if !self.interval_started {
            self.interval_started = true;
            self.interval_time = now;
        }
        let before = u32::from(self.cur_local_interval);
        let mut n = 0u32;
        while self.interval_time.0 < now.0 {
            self.interval_time = LocalTime(self.interval_time.0 + INTERVAL_SECONDS);
            n += 1;
        }
        if n == 0 {
            return Vec::new();
        }
        self.cur_local_interval = self
            .cur_local_interval
            .wrapping_add(u16::try_from(n).unwrap_or(u16::MAX));
        self.bytes_sent_this_interval = 0;

        let after = before + n;
        let mut events = Vec::new();
        if crossed_multiple(before, after, TIMESYNC_ECHO_INTERVALS) {
            events.push(IntervalEvent::TimeSyncAndEcho);
        }
        if crossed_multiple(before, after, ICMD_INTERVALS) {
            events.push(IntervalEvent::IcmdKeepAlive);
        }
        events
    }

    /// How many packets are built and waiting. Tests and diagnostics.
    #[must_use]
    pub fn waiting_len(&self) -> usize {
        self.waiting.len()
    }

    /// The waiting list's payload sizes, head first. Tests and diagnostics.
    #[must_use]
    pub fn waiting_payload_sizes(&self) -> Vec<usize> {
        self.waiting.iter().map(OutPacket::payload_len).collect()
    }
}

/// A packet holding exactly one section, and that section exclusive (flag `0x02`). Nothing may
/// be added to it.
fn is_lone_exclusive(p: &OutPacket) -> bool {
    p.fragments.is_empty()
        && p.optional.len() == 1
        && p.optional.keys().any(|m| {
            crate::wire::optional::spec_for(*m)
                .is_some_and(|s| s.flags & crate::wire::optional::flags::EXCLUSIVE != 0)
        })
}

/// Did the interval counter cross a multiple of `period` in going from `before` to `after`?
///
/// Retail's interval advance writes this as `(id + n) % period == 1 || n > period - 1 || it wrapped
/// past a boundary`; the observable result is "fires once per `period` intervals", which is what
/// this computes.
fn crossed_multiple(before: u32, after: u32, period: u32) -> bool {
    before / period != after / period
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{PacketFlags, HEADER_SIZE};

    fn blob(size: usize, priority: u32) -> NetBlob {
        let mut b = NetBlob::for_send(vec![0xAB; size], 9);
        b.priority = priority;
        b
    }

    /// Oracle: the sent counter starts at 1, and the two sequence rules
    /// (`docs/networking/02-reliability-and-flow.md` §2).
    #[test]
    fn highest_id_sent_starts_at_one_and_is_not_advanced_by_a_control_packet() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0x1234_5678);
        assert_eq!(fq.highest_id_sent, 1);

        // A pure ACK is disposable, so it reuses the current value and goes out in the clear.
        fq.enqueue_optional_header(PacketFlags::ACK_SEQUENCE, 7u32.to_le_bytes().to_vec())
            .expect("ack");
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        assert_eq!(out.len(), 1);
        let p = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(p.header.seq_id, 1, "reused, not advanced");
        assert!(!p.header.header.is_encrypted());
        assert_eq!(fq.highest_id_sent, 1);

        // A packet with a fragment advances it.
        fq.enqueue_blob(blob(16, 5));
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        assert_eq!(out.len(), 1);
        let p = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(p.header.seq_id, 2);
        assert!(p.header.header.is_encrypted());
        assert_eq!(fq.highest_id_sent, 2);
    }

    /// The counter skips 0 on wrap: `if (++highest_id_sent == 0) highest_id_sent = 1;`.
    #[test]
    fn highest_id_sent_never_becomes_zero() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0);
        fq.highest_id_sent = 0xFFFF_FFFF;
        fq.enqueue_blob(blob(16, 5));
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        assert_eq!(fq.highest_id_sent, 1);
        let p = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(p.header.seq_id, 1);
    }

    /// `coalesce_data` packs fragments up to 464 payload bytes and starts a new packet otherwise.
    ///
    /// Oracle: the coalescer's bound (packet `size_` plus fragment size below `0x1D1`), and the
    /// size table in `docs/networking/01-packet-format.md` §4.2.
    #[test]
    fn coalesce_packs_up_to_464_payload_bytes_then_starts_a_new_packet() {
        let mut fq = FlowQueue::new(0x0B, 1);
        // Four 100-byte blobs: each fragment is 16 + 100 = 116 bytes. Four fit in 464 exactly.
        for _ in 0..4 {
            fq.enqueue_blob(blob(100, 5));
        }
        fq.coalesce_data();
        assert_eq!(fq.waiting_payload_sizes(), vec![464]);

        // A fifth would make 580, so it starts a new packet.
        fq.enqueue_blob(blob(100, 5));
        fq.coalesce_data();
        assert_eq!(fq.waiting_payload_sizes(), vec![464, 116]);

        // And no datagram exceeds 20 + 464.
        let mut crypto = CryptoSystem::new(0);
        for dg in fq.transmit_new_packets(&mut crypto, LocalTime(0.0)) {
            assert!(dg.len() <= HEADER_SIZE + MAX_BUILD_PAYLOAD, "{}", dg.len());
        }
    }

    /// A blob larger than 448 bytes fragments, and the fragments pack across packets in index
    /// order.
    #[test]
    fn a_large_blob_fragments_across_packets_in_index_order() {
        let mut fq = FlowQueue::new(0x0B, 1);
        fq.enqueue_blob(blob(448 * 3, 5));
        fq.coalesce_data();
        // Each fragment is 464 bytes, so one per packet.
        assert_eq!(fq.waiting_payload_sizes(), vec![464, 464, 464]);

        let mut crypto = CryptoSystem::new(0);
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        let indices: Vec<u16> = out
            .iter()
            .map(|dg| {
                crate::ParsedPacket::parse(dg).expect("parse").fragments[0]
                    .header
                    .blob_num
            })
            .collect();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    /// Control packets go to the **head** of the waiting list, pure-fragment packets to the tail.
    ///
    /// Oracle: the packet enqueue and its add-to-head.
    #[test]
    fn control_packets_overtake_bulk_data() {
        let mut fq = FlowQueue::new(0x0B, 1);
        fq.enqueue_blob(blob(400, 5));
        fq.coalesce_data();
        assert_eq!(fq.waiting_len(), 1);

        fq.enqueue_optional_header(PacketFlags::REQUEST_RETRANSMIT, {
            let mut v = 1u32.to_le_bytes().to_vec();
            v.extend_from_slice(&9u32.to_le_bytes());
            v
        })
        .expect("nak");

        let mut crypto = CryptoSystem::new(0);
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        let first = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert!(
            first
                .optional
                .contains_key(&PacketFlags::REQUEST_RETRANSMIT),
            "the NAK must overtake the queued fragment"
        );
        let second = crate::ParsedPacket::parse(&out[1]).expect("parse");
        assert_eq!(second.fragments.len(), 1);
    }

    /// Lower `priority_` is sent first. The UI blob sender uses 5 for everything, so this only
    /// bites a rebuild that starts prioritising.
    #[test]
    fn lower_priority_blobs_are_sent_first() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut hi = blob(16, 9);
        hi.buf = vec![0x99; 16];
        let mut lo = blob(16, 1);
        lo.buf = vec![0x11; 16];
        fq.enqueue_blob(hi);
        fq.enqueue_blob(lo);
        fq.coalesce_data();
        let mut crypto = CryptoSystem::new(0);
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        let p = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(p.fragments[0].payload[0], 0x11);
        assert_eq!(p.fragments[1].payload[0], 0x99);
    }

    /// The cadence table, on a simulated clock: `docs/networking/02-reliability-and-flow.md` §6.1.
    ///
    /// TimeSync + Echo every 6 intervals (3 s), ICMD every 220 intervals (110 s).
    #[test]
    fn the_interval_cadence_fires_timesync_every_3s_and_icmd_every_110s() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut timesyncs = 0;
        let mut icmds = 0;
        // 300 seconds at 100 ms per tick.
        let mut t = 0.0f64;
        while t < 300.0 {
            t += 0.1;
            for e in fq.advance_interval(LocalTime(t)) {
                match e {
                    IntervalEvent::TimeSyncAndEcho => timesyncs += 1,
                    IntervalEvent::IcmdKeepAlive => icmds += 1,
                }
            }
        }
        assert_eq!(fq.cur_local_interval, 600, "0.5 s intervals over 300 s");
        assert_eq!(timesyncs, 100, "600 / 6 = one every 3 seconds");
        assert_eq!(icmds, 2, "600 / 220 = one every 110 seconds");
    }

    /// A stalled client that resumes crosses many intervals in one tick, and the events still fire
    /// once each rather than once per interval crossed.
    #[test]
    fn a_large_time_jump_fires_each_event_once() {
        let mut fq = FlowQueue::new(0x0B, 1);
        fq.advance_interval(LocalTime(0.0));
        let events = fq.advance_interval(LocalTime(120.0));
        assert_eq!(fq.cur_local_interval, 240);
        assert_eq!(
            events,
            vec![IntervalEvent::TimeSyncAndEcho, IntervalEvent::IcmdKeepAlive]
        );
    }

    /// A NAK for a sequence still in the cache goes on the resend list; one that is gone goes on
    /// the `RejectRetransmit` list instead.
    ///
    /// Oracle: the ack enqueue and the ack transmit.
    #[test]
    fn a_nak_resends_what_is_cached_and_rejects_what_is_not() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0xDEAD_BEEF);
        fq.enqueue_blob(blob(16, 5));
        let sent = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        assert_eq!(sent.len(), 1);
        let seq = crate::ParsedPacket::parse(&sent[0])
            .expect("parse")
            .header
            .seq_id;

        fq.enqueue_acks(&[seq, seq + 50]);
        let resends = fq.transmit_acks();
        assert_eq!(resends.len(), 1);
        let p = crate::ParsedPacket::parse(&resends[0]).expect("parse");
        assert_eq!(p.header.seq_id, seq);
        assert!(p.header.header.contains(PacketFlags::RETRANSMISSION));
        // The retransmission decrypts under the key the original used, not a new draw.
        assert_eq!(p.recovered_key(), 0x5DA2_2D96);

        // The unrecoverable one becomes a RejectRetransmit section.
        let block = fq.compile_empty_acks().expect("reject list");
        assert_eq!(&block[0..4], &1u32.to_le_bytes());
        assert_eq!(&block[4..8], &(seq + 50).to_le_bytes());
    }

    /// A resend carries the **current** `interval_`, not the one the packet was first sent under.
    ///
    /// Oracle: the ack transmit builds a fresh `ProtoHeader` and fills
    /// `interval_` from the current local interval's id and `iteration_` from
    /// our own receiver record's `iteration_`; only `seq_id`, `checksum_` and `datalen_` come off
    /// the cached packet. Without this test a rebuild that resent the stale interval would look
    /// identical on every other assertion.
    #[test]
    fn a_resend_carries_the_current_interval_not_the_cached_one() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0xDEAD_BEEF);
        fq.advance_interval(LocalTime(0.0));
        fq.enqueue_blob(blob(16, 5));
        let sent = fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        assert_eq!(sent.len(), 1);
        let orig = crate::ParsedPacket::parse(&sent[0]).expect("parse").header;

        // Six 0.5 s boundaries later the interval has moved on.
        fq.advance_interval(LocalTime(3.0));
        assert_eq!(fq.cur_local_interval, 6);
        assert_ne!(orig.interval, fq.cur_local_interval);

        fq.enqueue_acks(&[orig.seq_id]);
        let resends = fq.transmit_acks();
        assert_eq!(resends.len(), 1);
        let p = crate::ParsedPacket::parse(&resends[0])
            .expect("parse")
            .header;
        assert_eq!(p.seq_id, orig.seq_id, "the sequence is the cached one");
        assert_eq!(p.interval, fq.cur_local_interval, "the interval is not");
        assert_eq!(p.iteration, fq.iteration);
    }

    /// `RejectRetransmit` is capped at 114 ids per section, like the NAK list.
    #[test]
    fn reject_retransmit_caps_at_114() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let ids: Vec<u32> = (1..=200).collect();
        fq.enqueue_acks(&ids);
        let block = fq.compile_empty_acks().expect("first burst");
        assert_eq!(&block[0..4], &114u32.to_le_bytes());
        assert_eq!(block.len(), 4 + 4 * 114);
        let block = fq.compile_empty_acks().expect("second burst");
        assert_eq!(&block[0..4], &86u32.to_le_bytes());
        assert!(fq.compile_empty_acks().is_none());
    }

    /// `enqueue_acks` is a **merge**, and the difference from a filter is not hypothetical.
    ///
    /// Oracle: the ack list holds 5 and 10, and the peer NAKs
    /// 3 and 12. The client's remove-at-cursor arm drops 5 and 10 — a NAK for 12 says everything below
    /// 12 arrived — and the list ends as `[3, 12]`. A filter such as
    /// `retain(|id| !lhs_newer(first, *id))` drops only what is older than the *first* id, 3, and
    /// leaves `[3, 5, 10, 12]`: two packets resent that the peer had already acknowledged.
    ///
    /// The list is asserted through `transmit_acks`, which reports one datagram per surviving id,
    /// because `ack_list` is private and the wire is what matters.
    #[test]
    fn a_nak_implicitly_acknowledges_everything_below_its_last_id() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0xDEAD_BEEF);
        // Four cached packets, sequences 2..=5, so `contains` is true for every id used below.
        for _ in 0..4 {
            fq.enqueue_blob(blob(16, 5));
            fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        }
        assert_eq!(fq.sent.len(), 4);

        fq.enqueue_acks(&[3, 5]);
        fq.enqueue_acks(&[2, 4]);
        // 3 and 5 were on the list; the second NAK's 4 is newer than 3, so 3 goes.
        let resends = fq.transmit_acks();
        let ids: Vec<u32> = resends
            .iter()
            .map(|d| crate::ParsedPacket::parse(d).expect("parse").header.seq_id)
            .collect();
        assert_eq!(
            ids,
            vec![2, 4, 5],
            "3 was implicitly acknowledged by the NAK for 4; a filter on the first id would have \
             kept it and resent four packets"
        );
    }

    /// The empty-ack enqueue is idempotent, and `transmit_acks` must go through it.
    ///
    /// The empty-ack hash table means an id already awaiting a `RejectRetransmit` is not queued
    /// again. A `transmit_acks` that pushed straight onto the empty-ack list would send an id that
    /// had been rejected once and NAKed again twice in the same section — a sequence-id list
    /// header naming the same sequence twice, and one of the 114 slots wasted.
    #[test]
    fn an_id_already_awaiting_rejection_is_not_queued_twice() {
        let mut fq = FlowQueue::new(0x0B, 1);
        // Nothing is cached, so both routes into the empty-ack list fire for the same id.
        fq.enqueue_acks(&[77]);
        fq.enqueue_acks(&[77]);
        let block = fq.compile_empty_acks().expect("one rejection");
        assert_eq!(&block[0..4], &1u32.to_le_bytes(), "one id, not two");
        assert_eq!(&block[4..8], &77u32.to_le_bytes());
        assert!(fq.compile_empty_acks().is_none(), "and nothing left over");

        // Once it has gone out, the id may be rejected again
        // removes it from `empties_` as well as from the list.
        fq.enqueue_acks(&[77]);
        let block = fq
            .compile_empty_acks()
            .expect("a second rejection is legal");
        assert_eq!(&block[4..8], &77u32.to_le_bytes());
    }

    /// A cumulative ACK retires the cache and the resend list together.
    ///
    /// Oracle: the pending-ACK prefix older than the flush number is removed first, then the
    /// sent-packet cache is flushed to the same boundary.
    #[test]
    fn a_cumulative_ack_retires_the_cache_and_the_pending_resends_below_it() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0xDEAD_BEEF);
        for _ in 0..5 {
            fq.enqueue_blob(blob(16, 5));
            fq.transmit_new_packets(&mut crypto, LocalTime(0.0));
        }
        assert_eq!(fq.sent.len(), 5, "sequences 2..=6 are cached");
        fq.enqueue_acks(&[2, 3, 4, 5, 6]);

        // "I have everything below 5."
        assert_eq!(fq.flush_sent_packets(5), 3, "2, 3 and 4 leave the cache");
        assert_eq!(fq.sent.len(), 2);

        let ids: Vec<u32> = fq
            .transmit_acks()
            .iter()
            .map(|d| crate::ParsedPacket::parse(d).expect("parse").header.seq_id)
            .collect();
        assert_eq!(ids, vec![5, 6], "and the resend list lost the same prefix");
        assert!(
            fq.compile_empty_acks().is_none(),
            "a flushed id is not rejected -- it was never asked for again"
        );
    }

    /// The flow-control hooks are constants and must stay constants: the client never holds a
    /// packet back for lack of room.
    #[test]
    fn there_is_no_outbound_flow_control() {
        assert!(wire_room_left());
        assert!(frag_queue_room_left());
    }

    fn flow(bytes: u32, interval: u16) -> Vec<u8> {
        let mut v = bytes.to_le_bytes().to_vec();
        v.extend_from_slice(&interval.to_le_bytes());
        v
    }

    fn masks(datagram: &[u8]) -> u32 {
        crate::ParsedPacket::parse(datagram)
            .expect("parse")
            .header
            .header
            .0
    }

    /// Behaviour: link.time-sync.the-periodic-sections-wait-for-the-next-packet-the-client-sends
    ///
    /// A packet holding only `TimeSync`, `EchoRequest` and `Flow` is not sent while it is the
    /// last one waiting, although it will be encrypted when it goes; the next acknowledgement
    /// joins it, and the four leave together as the recorded `0x0B004002` packet. The two time
    /// sections then say the time they were sent at, not the time they were queued at.
    #[test]
    fn the_periodic_sections_wait_for_an_acknowledgement_and_carry_the_send_time() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0x1234_5678);
        fq.set_time_sensitive(TimeSensitive {
            game_time: 1.0,
            local_time: 1.0,
        });
        fq.enqueue_optional_headers(&[
            (PacketFlags::TIME_SYNC, 0f64.to_le_bytes().to_vec()),
            (PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec()),
            (PacketFlags::FLOW, flow(52, 1092)),
        ])
        .expect("queued");
        assert!(
            fq.transmit_new_packets(&mut crypto, LocalTime(3.0))
                .is_empty(),
            "held: nothing below 0x1000000 to send"
        );
        assert_eq!(fq.waiting_len(), 1);

        fq.set_time_sensitive(TimeSensitive {
            game_time: 250.5,
            local_time: 4.25,
        });
        fq.enqueue_optional_header(PacketFlags::ACK_SEQUENCE, 7u32.to_le_bytes().to_vec())
            .expect("ack");
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(4.0));
        assert_eq!(out.len(), 1, "one packet carries all four");
        assert_eq!(masks(&out[0]), 0x0B00_4002);
        let p = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(p.optional[&PacketFlags::TIME_SYNC], 250.5f64.to_le_bytes());
        assert_eq!(
            p.optional[&PacketFlags::ECHO_REQUEST],
            4.25f32.to_le_bytes()
        );
        assert_eq!(p.optional[&PacketFlags::FLOW], flow(52, 1092));
        assert_eq!(fq.waiting_len(), 0);
    }

    /// Behaviour: link.flow.each-closed-server-interval-is-reported-with-its-byte-count
    ///
    /// A second report queued while one is held starts a packet of its own at the head and goes
    /// out alone and sequenced (`0x08000002`); the acknowledgement that follows joins the older,
    /// held one. That is the order a recorded session shows: the newer report first, the older
    /// one with the next acknowledgement.
    #[test]
    fn a_second_report_goes_first_and_the_acknowledgement_takes_the_held_one() {
        let mut fq = FlowQueue::new(0x0B, 1);
        let mut crypto = CryptoSystem::new(0x1234_5678);
        fq.enqueue_optional_header(PacketFlags::FLOW, flow(24, 1110))
            .expect("queued");
        assert!(fq
            .transmit_new_packets(&mut crypto, LocalTime(0.0))
            .is_empty());

        fq.enqueue_optional_header(PacketFlags::FLOW, flow(28, 1111))
            .expect("queued");
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.1));
        assert_eq!(out.len(), 1);
        assert_eq!(masks(&out[0]), 0x0800_0002, "alone, and sequenced");
        let first = crate::ParsedPacket::parse(&out[0]).expect("parse");
        assert_eq!(first.optional[&PacketFlags::FLOW], flow(28, 1111));
        assert_eq!(
            first.header.seq_id, 2,
            "a Flow report takes a sequence number"
        );

        // The acknowledgement and a third report queued together: the acknowledgement is placed
        // first, on the held packet, and the report starts a packet ahead of it.
        fq.enqueue_optional_header(PacketFlags::FLOW, flow(24, 1112))
            .expect("queued");
        fq.enqueue_optional_header(PacketFlags::ACK_SEQUENCE, 9u32.to_le_bytes().to_vec())
            .expect("ack");
        let out = fq.transmit_new_packets(&mut crypto, LocalTime(0.4));
        let sent: Vec<(u32, Vec<u8>)> = out
            .iter()
            .map(|d| {
                let p = crate::ParsedPacket::parse(d).expect("parse");
                (p.header.header.0, p.optional[&PacketFlags::FLOW].clone())
            })
            .collect();
        assert_eq!(
            sent,
            vec![(0x0800_0002, flow(24, 1112)), (0x0800_4002, flow(24, 1110)),]
        );
    }
}
