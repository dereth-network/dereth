//! Per-connection receive state, sequence numbers, the NAK set and the retransmit cache.
//!
//! The thing to get right here is that **the ISAAC key stream is positional**. One value is consumed
//! per encrypted packet, in sequence order, on both sides. So:
//!
//! - when the receiver notices a gap it must *draw and park* a key for every skipped sequence
//!   number, and decrypt the eventual retransmission with the stored
//!   key rather than a fresh draw;
//! - a duplicate encrypted packet must be dropped **without** drawing
//!   (the new-sequence processor);
//! - the sender keeps the key it used for each packet and reuses it verbatim on
//!   retransmit.
//!
//! ACE's `CryptoSystem.Search`, which walks up to 256 values forward and keeps a `HashSet` of
//! skipped keys, is an *approximation* of this and desynchronises on a lost retransmission. It is
//! not used here, and the server checks its clients' packets with [`SequenceWindow`] as well (the
//! retail client and server shared this receive code). See
//! `docs/CORRECTIONS.md`.
//!
//! See `docs/networking/02-reliability-and-flow.md` §§1-4.

use std::collections::BTreeMap;
use std::net::IpAddr;

use dereth_primitives::LocalTime;

use crate::isaac::CryptoSystem;
use crate::wire::{OutPacket, PacketFlags, ProtoHeader, WireError};

/// The 32-bit wrapping comparison.
///
/// ```text
/// if (a == b) return false;
/// diff = a - b; sign = 1;
/// if (a < b) { diff = b - a; sign = -1; }
/// if (diff > 0x7FFFFFFF) sign = -sign;
/// return sign > 0;
/// ```
///
/// Transcribed rather than paraphrased, for the same reason as
/// [`crate::blob::NetBlobId::lhs_newer_ordering_stamp`]: at a difference of exactly `0x8000_0000`
/// the relation is not antisymmetric, because the sign is chosen by the numeric comparison before
/// the `> 0x7FFFFFFF` test flips it.
#[must_use]
pub fn lhs_newer(a: u32, b: u32) -> bool {
    if a == b {
        return false;
    }
    let (diff, mut sign) = if a < b { (b - a, -1i32) } else { (a - b, 1i32) };
    if diff > 0x7FFF_FFFF {
        sign = -sign;
    }
    sign > 0
}

/// The 16-bit form, used for `interval`.
#[must_use]
pub fn lhs_newer_u16(a: u16, b: u16) -> bool {
    if a == b {
        return false;
    }
    let (diff, mut sign) = if a < b { (b - a, -1i32) } else { (a - b, 1i32) };
    if diff > 0x7FFF {
        sign = -sign;
    }
    sign > 0
}

/// A three-way 16-bit wrapping compare, used for
/// `iteration`. Returns -1, 0 or 1.
#[must_use]
pub fn overflow_compare(a: u16, b: u16) -> i32 {
    if a == b {
        return 0;
    }
    let (diff, mut sign) = if a < b { (b - a, -1i32) } else { (a - b, 1i32) };
    if diff > 0x7FFF {
        sign = -sign;
    }
    sign
}

/// The sequence-id sanity check refuses a NAK set larger than this.
pub const NAK_SET_CAP: usize = 40_000;

/// `seq_id_sanity_check` refuses a sequence number newer than `highest_id_received + 0x7FFF`.
pub const SEQ_JUMP_CAP: u32 = 0x7FFF;

/// The NAK builder caps a burst at 114 ids (`0x72`).
///
/// ACE's `MaxNumNakSeqIds` is 115, one larger. A retail client accepts 114 and rejects a header
/// claiming 115 or more, so an ACE NAK of maximum size would be rejected.
pub const MAX_NAK_IDS: usize = 114;

/// `ReceiverState`.
///
/// `NO_STATE` is never assigned anywhere in the client; it is carried so the enum matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReceiverState {
    #[default]
    Undef = 0,
    Nak = 1,
    NoNak = 2,
    /// Never assigned by the client.
    NoState = 3,
}

/// Why a packet was refused at the sequence layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RejectReason {
    #[error("rec_id does not fit the 256-entry receiver table")]
    RecIdOutOfRange,
    #[error("no established connection for this rec_id and the packet is sequenced")]
    NoConnection,
    #[error("source address differs from the connection's stored address")]
    WrongSourceAddress,
    #[error("iteration_ is older than the stored one")]
    StaleIteration,
    #[error("iteration_ is newer than the stored one but carries no ConnectRequest")]
    NewIterationWithoutConnectRequest,
    #[error("datalen_ is at or above the 0xFFE1 rejection threshold")]
    DatalenTooLarge,
    #[error("seq_id jumps more than 32767 forward")]
    SequenceJumpTooLarge,
    #[error("the NAK set already holds more than 40000 entries")]
    NakSetOverflow,
    #[error("duplicate: sequence is not newer and was never NAKed")]
    DuplicateSequence,
    #[error("unsequenced packet carries EncryptedChecksum")]
    UnsequencedButEncrypted,
    #[error("the checksum does not match the key for its sequence")]
    BadChecksum,
}

/// The receive side's sequence window: the highest sequence accepted, the sequences skipped over
/// (each with the checksum key drawn for it), and the peer's key stream.
///
/// This is the one implementation of the positional key check, and it is role-neutral: the
/// retail client and the retail server ran the same receive code, so the client's
/// [`ReceiverData`] holds one per connection and the server holds one per session for its
/// client's packets. [`SequenceWindow::accept`] is the whole check for one packet.
#[derive(Debug, Clone)]
pub struct SequenceWindow {
    /// Highest sequence number accepted so far.
    pub highest_id_received: u32,
    /// The ordered set of sequence ids we have asked for, each with **the ISAAC key
    /// that was drawn for it**. A `BTreeMap` because `get_naks` walks it in order.
    pub seq_ids_we_naked: BTreeMap<u32, u32>,
    /// The peer's key stream: the keys its encrypted packets carry, one per sequence.
    pub crypto_incoming: CryptoSystem,
}

impl SequenceWindow {
    /// A window at sequence 0 over the key stream seeded with `incoming_seed`.
    #[must_use]
    pub fn new(incoming_seed: u32) -> Self {
        Self {
            highest_id_received: 0,
            seq_ids_we_naked: BTreeMap::new(),
            crypto_incoming: CryptoSystem::new(incoming_seed),
        }
    }

    /// The sequence and checksum check for one received packet, in the receiver's order.
    ///
    /// An unsequenced packet (sequence 0) is never encrypted and draws no key. A sequenced one
    /// goes through [`Self::process_new_seq_num`], which yields the key its checksum must carry;
    /// `checksum_ok` then checks the checksum under that key (`None`: in clear). A packet that
    /// fails there and was sequenced *and* encrypted is treated as lost and re-requested, which
    /// is what keeps the key stream aligned. The key parked for the re-request is the one this
    /// packet was just checked against -- the sender's key for this sequence -- and never a fresh
    /// draw: a draw would shift every later packet's key by one.
    ///
    /// Returns the key the packet was accepted under (`None` for a plaintext packet).
    ///
    /// # Errors
    /// A [`RejectReason`]; the packet is dropped.
    pub fn accept(
        &mut self,
        header: &ProtoHeader,
        checksum_ok: impl FnOnce(Option<u32>) -> bool,
    ) -> Result<Option<u32>, RejectReason> {
        let encrypted = header.header.is_encrypted();
        let key = if header.seq_id == 0 {
            if encrypted {
                return Err(RejectReason::UnsequencedButEncrypted);
            }
            None
        } else {
            self.process_new_seq_num(header)?
        };
        if !checksum_ok(key) {
            if header.seq_id != 0 && encrypted {
                self.add_nakked(header.seq_id, key);
            }
            return Err(RejectReason::BadChecksum);
        }
        Ok(key)
    }

    /// The sequence-id sanity check.
    ///
    /// ```text
    /// if (lhs_newer(seq_id, highest_id_received + 0x7FFF)) return false;
    /// return seq_ids_we_naked.len() <= 40000;
    /// ```
    ///
    /// # Errors
    /// [`RejectReason::SequenceJumpTooLarge`] or [`RejectReason::NakSetOverflow`].
    pub fn seq_id_sanity_check(&self, seq_id: u32) -> Result<(), RejectReason> {
        if lhs_newer(seq_id, self.highest_id_received.wrapping_add(SEQ_JUMP_CAP)) {
            return Err(RejectReason::SequenceJumpTooLarge);
        }
        if self.seq_ids_we_naked.len() > NAK_SET_CAP {
            return Err(RejectReason::NakSetOverflow);
        }
        Ok(())
    }

    /// The new-sequence-number walk, after the connection check.
    ///
    /// Returns the ISAAC key this packet's checksum must be XORed with, or `None` for a plaintext
    /// packet. The **only** places a key is produced are here and in explicit NAK recovery, and
    /// between them they consume exactly one stream value per encrypted sequence number.
    ///
    /// The duplicate-suppression branch is load-bearing: an encrypted packet whose sequence is not
    /// newer and is not in the NAK set is rejected **before** anything is drawn. Drawing there
    /// desynchronises the stream permanently, which is the failure ACE's forward-scanning
    /// `CryptoSystem.Search` exists to paper over.
    ///
    /// # Errors
    /// A [`RejectReason`].
    pub fn process_new_seq_num(
        &mut self,
        header: &ProtoHeader,
    ) -> Result<Option<u32>, RejectReason> {
        let seq_id = header.seq_id;
        self.seq_id_sanity_check(seq_id)?;

        let encrypted = header.header.is_encrypted();
        let mut parked: Option<u32> = None;

        if encrypted && !lhs_newer(seq_id, self.highest_id_received) {
            // The retransmission we asked for -- or a duplicate, which is dropped here without
            // touching the stream.
            parked = self.seq_ids_we_naked.remove(&seq_id);
            if parked.is_none() {
                return Err(RejectReason::DuplicateSequence);
            }
        }

        if lhs_newer(seq_id, self.highest_id_received) {
            self.process_newest_seq_num(header);
        }

        if encrypted {
            // In the receiver's decrypt: use the parked key if there is one, otherwise
            // draw the next value. Exactly one value per encrypted sequence, in order.
            let key = parked.unwrap_or_else(|| self.crypto_incoming.next());
            return Ok(Some(key));
        }
        Ok(None)
    }

    /// NAK the gap, then advance.
    ///
    /// The target is the header's `seq_id`, plus 1 when the checksum is not encrypted. Every id
    /// from `highest_id_received + 1` up to (not including) the target, skipping 0, is added to the
    /// NAK set with no key, and `highest_id_received` becomes the header's `seq_id`.
    ///
    /// The `+1` on the non-encrypted branch NAKs the packet's own sequence number, and it is
    /// reachable on a lossy link. Both ends stamp their unencrypted ACK/NAK packets with the
    /// *current*, already-used sequence value, so when the encrypted packet that used that value
    /// is lost, the ACK that follows it arrives as the newest sequence. Its number is then the
    /// lost packet's, and the `+1` is what asks for it (with a parked key) rather than skipping
    /// it. The compatibility notes #36 had called the branch unreachable and guarded it with a
    /// `debug_assert!`; a server run with loss fired it.
    pub fn process_newest_seq_num(&mut self, header: &ProtoHeader) {
        let encrypted = header.header.is_encrypted();
        let target = if encrypted {
            header.seq_id
        } else {
            header.seq_id.wrapping_add(1)
        };
        let mut i = self.highest_id_received.wrapping_add(1);
        while i != target {
            if i != 0 {
                self.add_nakked(i, None);
            }
            i = i.wrapping_add(1);
        }
        self.highest_id_received = header.seq_id;
    }

    /// Record a NAKed sequence.
    ///
    /// Inserts `seq` into the NAKed-sequence set with the next incoming crypto seed
    /// as the value — that is, it **draws and parks the ISAAC key** for the sequence number it just
    /// skipped, so the eventual retransmission decrypts under the key the sender used. Inserting an
    /// id that is already present with a non-null value is a no-op, so the key is drawn once.
    /// Returns whether `seq` was inserted.
    pub fn add_nakked(&mut self, seq: u32, key: Option<u32>) -> bool {
        if self.seq_ids_we_naked.contains_key(&seq) {
            return false;
        }
        let key = key.unwrap_or_else(|| self.crypto_incoming.next());
        self.seq_ids_we_naked.insert(seq, key);
        true
    }

    /// A `RejectRetransmit` arrived: the peer cannot resend these.
    ///
    /// Removes each id from the NAK set, ending the retry loop for it. It deliberately does **not**
    /// advance `highest_id_received`: the parked keys for those sequences are simply forgotten, and
    /// the stream stays aligned because the keys were already drawn.
    pub fn handle_empty_ack(&mut self, ids: &[u32]) {
        for id in ids {
            self.seq_ids_we_naked.remove(id);
        }
    }

    /// An in-order walk, capped at 114 ids.
    #[must_use]
    pub fn get_naks(&self) -> Vec<u32> {
        self.seq_ids_we_naked
            .keys()
            .copied()
            .take(MAX_NAK_IDS)
            .collect()
    }
}

/// Receiver data from the flat 256-entry shared-network array.
///
/// See `docs/networking/02-reliability-and-flow.md` §1.
#[derive(Debug)]
pub struct ReceiverData {
    /// Non-zero means this slot is in use; equals the index.
    pub rec_id: u16,
    /// The id to put in the outgoing header's `rec_id`.
    pub net_id: u16,
    /// Connection generation, from the ConnectRequest.
    pub iteration: u16,
    /// The highest sequence accepted, the NAK set with its parked keys, and the server -> client
    /// key stream (seeded from `ConnectRequest.OutgoingSeed`).
    pub window: SequenceWindow,
    pub nak_state: ReceiverState,
    /// Last time a PAK **or** a NAK header was enqueued. The two share this field, which is why the
    /// 2.0 s ACK cadence and the 0.6 s NAK cadence interfere with each other.
    pub time_stamp: LocalTime,
    /// Last time any valid packet arrived — drives the 140 s timeout.
    pub local_time_last_got_data: LocalTime,
    /// The peer's interval id we are currently accumulating bytes for. `0` means none yet: the
    /// first packet to arrive sets it without a report.
    pub current_remote_interval: u16,
    /// Bytes received in that interval, for the `Flow` header. Includes the 20-byte headers.
    pub bytes_received: u32,
    /// The client -> server stream, seeded from `ConnectRequest.IncomingSeed`.
    pub crypto_outgoing: CryptoSystem,
    /// Last echo RTT in seconds.
    pub round_trip_latency: f32,
    /// Referral cookie -- the cookie this connection
    /// would re-present if it had to be re-established.
    ///
    /// Written by the connection-request handler's tail (the `ConnectRequest`'s own cookie)
    /// and by the referral handler's live arm. Read in exactly one place,
    /// the 140-second timeout, which turns it into a
    /// self-referral. See `dereth_client_net::net::Net::referral_cookie`.
    pub referral_cookie: u64,
    /// The server's address. `verify_header` compares the address and **not** the port, which is what
    /// makes the port + 1 rule legal.
    pub addr: Option<IpAddr>,
}

impl ReceiverData {
    /// The receiver's crypto init.
    ///
    /// The two seeds arrive **in clear** in the `ConnectRequest`, and the field names are the
    /// *server's* point of view: `OutgoingSeed` is server -> client and becomes the client's
    /// incoming stream (`window.crypto_incoming`).
    #[must_use]
    pub fn new(rec_id: u16, outgoing_seed: u32, incoming_seed: u32) -> Self {
        Self {
            rec_id,
            net_id: 0,
            iteration: 0,
            window: SequenceWindow::new(outgoing_seed),
            nak_state: ReceiverState::Undef,
            time_stamp: LocalTime::default(),
            local_time_last_got_data: LocalTime::default(),
            current_remote_interval: 0,
            bytes_received: 0,
            crypto_outgoing: CryptoSystem::new(incoming_seed),
            round_trip_latency: 0.0,
            referral_cookie: 0,
            addr: None,
        }
    }

    /// Account one accepted datagram against the peer's interval, and return the `Flow` report
    /// that closes the interval it ends, if it ends one.
    ///
    /// The peer stamps every packet with its own half-second interval counter. When a packet
    /// arrives stamped with an interval newer than the one being counted (a 16-bit wrapping
    /// compare), or when none is being counted yet, the interval being counted is over: its report
    /// is the 6-byte `Flow` section -- the bytes received in it (`u32`) and its id (`u16`) -- and
    /// counting starts afresh on the new interval. No report is made for the "none yet" case,
    /// and an older or equal stamp changes nothing. The datagram itself, header included, is then
    /// counted into the interval now current, so the report for an interval covers every datagram
    /// stamped with it, the one that opened it among them.
    ///
    /// The client sends each report to the peer the datagram came from; the server uses it, if at
    /// all, as telemetry.
    #[must_use]
    pub fn account_datagram(&mut self, interval: u16, datagram_len: u32) -> Option<[u8; 6]> {
        let mut report = None;
        if self.current_remote_interval == 0
            || lhs_newer_u16(interval, self.current_remote_interval)
        {
            if self.current_remote_interval != 0 {
                let mut flow = [0u8; 6];
                flow[..4].copy_from_slice(&self.bytes_received.to_le_bytes());
                flow[4..].copy_from_slice(&self.current_remote_interval.to_le_bytes());
                report = Some(flow);
            }
            self.current_remote_interval = interval;
            self.bytes_received = 0;
        }
        self.bytes_received = self.bytes_received.wrapping_add(datagram_len);
        report
    }

    /// The header verification, in the documented **order**.
    ///
    /// `established` says whether this slot has an active connection with a nonzero recipient id; `source`
    /// is the datagram's source address. Note again what is *not* compared: the source **port**.
    /// The server's `ConnectResponse` and keep-alive go to port + 1, so it answers from a different
    /// port than the one the client sends most traffic to, and comparing the port would reject the
    /// first post-handshake packet.
    ///
    /// # Errors
    /// A [`RejectReason`]. The client logs none of them; it bumps a bad-packets-received counter.
    pub fn verify_header(
        header: &ProtoHeader,
        established: Option<&Self>,
        source: Option<IpAddr>,
    ) -> Result<(), RejectReason> {
        if header.rec_id >= 0x100 {
            return Err(RejectReason::RecIdOutOfRange);
        }
        let Some(recv) = established.filter(|r| r.rec_id != 0) else {
            // An unused slot accepts only an unsequenced packet: that is the handshake.
            return if header.seq_id == 0 {
                Ok(())
            } else {
                Err(RejectReason::NoConnection)
            };
        };
        if let (Some(stored), Some(actual)) = (recv.addr, source) {
            if stored != actual {
                return Err(RejectReason::WrongSourceAddress);
            }
        }
        match overflow_compare(header.iteration, recv.iteration) {
            i if i < 0 => return Err(RejectReason::StaleIteration),
            // A newer iteration is accepted only when it carries ConnectRequest -- that is how a
            // re-handshake replaces a stale connection.
            i if i > 0 && !header.header.contains(PacketFlags::CONNECT_REQUEST) => {
                return Err(RejectReason::NewIterationWithoutConnectRequest)
            }
            _ => {}
        }
        if header.datalen >= crate::wire::DATALEN_REJECT_AT {
            return Err(RejectReason::DatalenTooLarge);
        }
        Ok(())
    }

    /// [`SequenceWindow::accept`] for this connection: refused when the slot has no connection
    /// and the packet is sequenced.
    ///
    /// # Errors
    /// A [`RejectReason`].
    pub fn accept(
        &mut self,
        header: &ProtoHeader,
        checksum_ok: impl FnOnce(Option<u32>) -> bool,
    ) -> Result<Option<u32>, RejectReason> {
        if header.seq_id != 0 && self.rec_id == 0 {
            return Err(RejectReason::NoConnection);
        }
        let out = self.tracking_nak_state(|w| w.accept(header, checksum_ok));
        if out == Err(RejectReason::BadChecksum)
            && header.seq_id != 0
            && header.header.is_encrypted()
        {
            // The failed packet's sequence went back into the NAK set (possibly after its parked
            // key came out of it, leaving the size unchanged).
            self.nak_state = ReceiverState::Nak;
        }
        out
    }

    /// Runs `f` on the window, then settles `nak_state` if the NAK set changed size.
    ///
    /// Equivalent to setting it at every insert and removal: an insert makes the set non-empty
    /// ([`ReceiverState::Nak`]), and a removal leaves `Nak` or `NoNak` by what remains.
    fn tracking_nak_state<T>(&mut self, f: impl FnOnce(&mut SequenceWindow) -> T) -> T {
        let before = self.window.seq_ids_we_naked.len();
        let out = f(&mut self.window);
        if self.window.seq_ids_we_naked.len() != before {
            self.set_nak_state();
        }
        out
    }

    /// See [`SequenceWindow::seq_id_sanity_check`].
    ///
    /// # Errors
    /// [`RejectReason::SequenceJumpTooLarge`] or [`RejectReason::NakSetOverflow`].
    pub fn seq_id_sanity_check(&self, seq_id: u32) -> Result<(), RejectReason> {
        self.window.seq_id_sanity_check(seq_id)
    }

    /// The new-sequence-number walk: the connection check, then
    /// [`SequenceWindow::process_new_seq_num`].
    ///
    /// # Errors
    /// A [`RejectReason`].
    pub fn process_new_seq_num(
        &mut self,
        header: &ProtoHeader,
    ) -> Result<Option<u32>, RejectReason> {
        if header.seq_id != 0 && self.rec_id == 0 {
            return Err(RejectReason::NoConnection);
        }
        self.tracking_nak_state(|w| w.process_new_seq_num(header))
    }

    /// See [`SequenceWindow::process_newest_seq_num`].
    pub fn process_newest_seq_num(&mut self, header: &ProtoHeader) {
        self.tracking_nak_state(|w| w.process_newest_seq_num(header));
    }

    /// See [`SequenceWindow::add_nakked`]; an insert flips `nak_state` to `NAK_STATE`.
    pub fn add_nakked(&mut self, seq: u32, key: Option<u32>) {
        if self.window.add_nakked(seq, key) {
            self.nak_state = ReceiverState::Nak;
        }
    }

    /// A `RejectRetransmit` arrived. See [`SequenceWindow::handle_empty_ack`].
    pub fn handle_empty_ack(&mut self, ids: &[u32]) {
        self.window.handle_empty_ack(ids);
        self.set_nak_state();
    }

    /// An in-order walk, capped at 114 ids.
    #[must_use]
    pub fn get_naks(&self) -> Vec<u32> {
        self.window.get_naks()
    }

    /// An empty set means [`ReceiverState::NoNak`], non-empty [`ReceiverState::Nak`].
    pub fn set_nak_state(&mut self) {
        self.nak_state = if self.window.seq_ids_we_naked.is_empty() {
            ReceiverState::NoNak
        } else {
            ReceiverState::Nak
        };
    }
}

/// A bound this crate imposes and the original does not.
///
/// `SentPacketHistory` in the retail client is evicted **only** by cumulative ACK / NAK flush or
/// connection teardown; there is no size or age cap. This adds one while keeping the ACK/NAK
/// eviction semantics.
pub const SENT_CACHE_MAX_PACKETS: usize = 4096;

/// And by age. ACE uses ~120 seconds for the same purpose
/// (`cachedPacketRetentionTime`), which is a reasonable precedent.
pub const SENT_CACHE_MAX_AGE: f64 = 120.0;

/// `SentPacketHistory` — the retransmit cache.
///
/// See `docs/networking/02-reliability-and-flow.md` §4.1.
#[derive(Debug, Default)]
pub struct SentPacketHistory {
    /// The sent-packet list, a plain FIFO searched linearly in the original.
    packets: Vec<(u32, LocalTime, OutPacket)>,
    /// For every cached ephemeral fragment, sequence id -> latest blob id,
    /// keeping only the newest stamp. The client never *sends* ephemeral blobs, so this path is
    /// dead in the retail client and matters only to the shared server build.
    sent_net_blob_ids: BTreeMap<u64, u64>,
}

impl SentPacketHistory {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cache one sent packet.
    ///
    /// Stripping the disposable headers here is not optional: without it a resent packet carries a
    /// stale `AckSequence` or `RequestRetransmit`, telling the peer something that was true when
    /// the packet was first built and is not true now.
    pub fn add_sent_packet(&mut self, mut packet: OutPacket, now: LocalTime) {
        packet.remove_disposable_optional_headers();
        for frag in &packet.fragments {
            let id = crate::blob::NetBlobId(frag.header.blob_id());
            if id.is_ephemeral() {
                let entry = self
                    .sent_net_blob_ids
                    .entry(id.sequence_id())
                    .or_insert(id.0);
                if crate::blob::NetBlobId(*entry).lhs_newer_ordering_stamp(id) {
                    // The stored one is newer; keep it.
                } else {
                    *entry = id.0;
                }
            }
        }
        self.packets.push((packet.header.seq_id, now, packet));
        self.evict(now);
    }

    /// The bound. Applied after every insert; never touches a packet the ACK/NAK path still needs
    /// in the ordinary case, because the retail cadences flush far faster than these limits.
    fn evict(&mut self, now: LocalTime) {
        self.packets
            .retain(|(_, sent, _)| now.seconds_since(*sent) <= SENT_CACHE_MAX_AGE);
        if self.packets.len() > SENT_CACHE_MAX_PACKETS {
            let excess = self.packets.len() - SENT_CACHE_MAX_PACKETS;
            self.packets.drain(..excess);
        }
    }

    /// Is this sequence still cached?
    #[must_use]
    pub fn contains(&self, seq: u32) -> bool {
        self.packets.iter().any(|(s, _, _)| *s == seq)
    }

    /// The cached packet for a NAKed sequence.
    #[must_use]
    pub fn get(&self, seq: u32) -> Option<&OutPacket> {
        self.packets
            .iter()
            .find(|(s, _, _)| *s == seq)
            .map(|(_, _, p)| p)
    }

    /// Drop the cached packets **strictly older** than
    /// `flush_num`, and erase the corresponding ephemeral blob ids.
    ///
    /// # The boundary is exclusive
    ///
    /// The original pops from the head and stops on the first packet it must keep:
    ///
    /// ```text
    /// if (seq_num == flush) return 1;                 // keep it, and everything after
    /// if (-1 < lhs_newer_sign(seq_num, flush)) return 1;
    /// pop;
    /// ```
    ///
    /// so the packet whose sequence *equals* `flush_num` **survives**. The tempting
    /// `retain(|seq| lhs_newer(seq, flush_num))` drops that packet as well.
    ///
    /// **The extra packet is not a rounding error, it is the one being NAKed.** `flush_num` is fed
    /// from two places (`Connection::flush_num`): `AckSequence`, which carries
    /// `highest_id_received` and is inclusive, and the retransmit-request path, which
    /// sets it to the **first id the peer is asking for** — a sequence the peer explicitly does
    /// *not* have. An inclusive flush deletes that packet from the cache in the same tick it was
    /// NAKed, so a second NAK for it (the resend was lost too, which is the whole point of a lossy
    /// link) is answered with `RejectRetransmit` and the blob is gone for good. Exclusive is what
    /// makes the shared field safe for both writers.
    ///
    /// Returns how many packets went.
    pub fn flush(&mut self, flush_num: u32) -> usize {
        let before = self.packets.len();
        let mut dropped_ids = Vec::new();
        // A prefix pop, not a filter: the list is in send order, and the original stops at the
        // first packet it keeps rather than scanning past it. The two differ across a sequence
        // wrap, where "older" is not the same relation as "earlier in the list".
        while let Some((seq, _, _)) = self.packets.first() {
            if *seq == flush_num || !lhs_newer(flush_num, *seq) {
                break;
            }
            let (_, _, packet) = self.packets.remove(0);
            for frag in &packet.fragments {
                dropped_ids.push(crate::blob::NetBlobId(frag.header.blob_id()));
            }
        }
        for id in dropped_ids {
            // The erase removes the entry only when the stored id still equals
            // this blob's id, so a newer update is not lost.
            if id.is_ephemeral() && self.sent_net_blob_ids.get(&id.sequence_id()) == Some(&id.0) {
                self.sent_net_blob_ids.remove(&id.sequence_id());
            }
        }
        before - self.packets.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.packets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }
}

/// Recompute the checksum for a retransmission, reusing the packet's **stored** key.
///
/// The ack dequeue, verified against retail, uses the packet's own stored crypto key. Drawing a fresh key here would decrypt under
/// a value the receiver never parked, and the packet would be dropped forever.
///
/// # Errors
/// [`WireError::EncryptionEquivalence`] if the cached packet has no stored key but needs one.
pub fn reserialize_for_retransmit(packet: &mut OutPacket) -> Result<Vec<u8>, WireError> {
    let key = packet.crypto_key;
    packet.header.header = PacketFlags(packet.header.header.0 | PacketFlags::RETRANSMISSION);
    packet.serialize(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{Fragment, FragmentHeader};

    fn recv() -> ReceiverData {
        let mut r = ReceiverData::new(1, 0xDEAD_BEEF, 0x1234_5678);
        r.net_id = 0x0B;
        r
    }

    fn hdr(seq: u32, flags: u32) -> ProtoHeader {
        ProtoHeader {
            seq_id: seq,
            header: PacketFlags(flags),
            ..Default::default()
        }
    }

    /// Oracle: the client's own 32-bit newer-than test, transcribed.
    #[test]
    fn lhs_newer_wraps_and_is_asymmetric_at_half_a_period() {
        assert!(lhs_newer(2, 1));
        assert!(!lhs_newer(1, 2));
        assert!(!lhs_newer(1, 1));
        assert!(lhs_newer(0, 0xFFFF_FFFF));
        assert!(!lhs_newer(0xFFFF_FFFF, 0));
        // Exactly half a period: the sign is decided before the 0x7FFFFFFF test flips it.
        assert!(lhs_newer(0, 0x8000_0000));
        assert!(!lhs_newer(0x8000_0000, 0));
    }

    /// Oracle: the order of the header-validation tests is the client's, and
    /// the source **port** is deliberately absent.
    #[test]
    fn verify_header_rejects_in_order_and_ignores_the_source_port() {
        use std::net::Ipv4Addr;
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let other = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let mut r = recv();
        r.addr = Some(peer);
        r.iteration = 5;

        // rec_id first, before anything looks at the connection.
        let mut h = hdr(0, 0);
        h.rec_id = 0x100;
        assert_eq!(
            ReceiverData::verify_header(&h, Some(&r), Some(peer)),
            Err(RejectReason::RecIdOutOfRange)
        );

        // An unused slot accepts seq_id == 0 and rejects anything else.
        assert!(ReceiverData::verify_header(&hdr(0, 0), None, Some(peer)).is_ok());
        assert_eq!(
            ReceiverData::verify_header(&hdr(1, 0), None, Some(peer)),
            Err(RejectReason::NoConnection)
        );

        // The source address is compared...
        let mut h = hdr(1, 0);
        h.iteration = 5;
        assert_eq!(
            ReceiverData::verify_header(&h, Some(&r), Some(other)),
            Err(RejectReason::WrongSourceAddress)
        );
        // ...and the port is not, which is what makes the port + 1 rule legal.
        assert!(ReceiverData::verify_header(&h, Some(&r), Some(peer)).is_ok());

        // An older iteration is rejected outright.
        let mut h = hdr(1, 0);
        h.iteration = 4;
        assert_eq!(
            ReceiverData::verify_header(&h, Some(&r), Some(peer)),
            Err(RejectReason::StaleIteration)
        );
        // A newer one only with ConnectRequest.
        let mut h = hdr(1, 0);
        h.iteration = 6;
        assert_eq!(
            ReceiverData::verify_header(&h, Some(&r), Some(peer)),
            Err(RejectReason::NewIterationWithoutConnectRequest)
        );
        h.header = PacketFlags(PacketFlags::CONNECT_REQUEST);
        assert!(ReceiverData::verify_header(&h, Some(&r), Some(peer)).is_ok());

        // datalen_ is checked last.
        let mut h = hdr(1, 0);
        h.iteration = 5;
        h.datalen = 0xFFE1;
        assert_eq!(
            ReceiverData::verify_header(&h, Some(&r), Some(peer)),
            Err(RejectReason::DatalenTooLarge)
        );
    }

    /// The heart of duplicate suppression (`docs/networking/02-reliability-and-flow.md` §2.2): a
    /// duplicate encrypted packet is dropped **without** consuming an ISAAC value.
    ///
    /// Oracle: duplicate removal returning null makes the
    /// function return before decryption is reached, and decryption is the only draw
    /// site on that path.
    #[test]
    fn a_duplicate_encrypted_packet_does_not_consume_an_isaac_value() {
        let mut r = recv();
        let enc = PacketFlags::ENCRYPTED_CHECKSUM;

        // Sequence 1: new highest, draws the first key.
        let k1 = r.process_new_seq_num(&hdr(1, enc)).expect("accept");
        assert_eq!(
            k1,
            Some(0x5DA2_2D96),
            "the first draw of CryptoSystem(0xDEADBEEF)"
        );
        assert_eq!(r.window.highest_id_received, 1);

        // Sequence 1 again: not newer, not NAKed -> rejected, no draw.
        assert_eq!(
            r.process_new_seq_num(&hdr(1, enc)),
            Err(RejectReason::DuplicateSequence)
        );

        // So sequence 2 gets the *second* value, not the third.
        let k2 = r.process_new_seq_num(&hdr(2, enc)).expect("accept");
        assert_eq!(k2, Some(0xDB3B_A3B6), "the second draw");
    }

    /// `process_newest_seq_num` parks a key for every skipped sequence, and the retransmission is
    /// decrypted with the **stored** key rather than a fresh draw.
    ///
    /// Oracle: the new-sequence path passes the parked key to its decrypt call. The eight expected
    /// values are `CryptoSystem(0xDEADBEEF)`'s first eight,
    /// printed by the independent packet-format calculation.
    #[test]
    fn skipped_sequences_park_their_keys_and_retransmits_reuse_them() {
        let stream = [
            0x5DA2_2D96u32,
            0xDB3B_A3B6,
            0x9FD9_67F9,
            0x0748_7047,
            0x0A8E_4664,
        ];
        let mut r = recv();
        let enc = PacketFlags::ENCRYPTED_CHECKSUM;

        // Sequence 4 arrives first: 1, 2 and 3 are skipped, so three keys are drawn and parked,
        // and only then is the fourth drawn for the packet itself.
        let k = r.process_new_seq_num(&hdr(4, enc)).expect("accept");
        assert_eq!(r.get_naks(), vec![1, 2, 3]);
        assert_eq!(r.window.seq_ids_we_naked[&1], stream[0]);
        assert_eq!(r.window.seq_ids_we_naked[&2], stream[1]);
        assert_eq!(r.window.seq_ids_we_naked[&3], stream[2]);
        assert_eq!(
            k,
            Some(stream[3]),
            "the packet's own key is the fourth draw"
        );
        assert_eq!(r.window.highest_id_received, 4);
        assert_eq!(r.nak_state, ReceiverState::Nak);

        // The retransmission of 2 arrives. It reuses the parked key -- no new draw.
        let k = r.process_new_seq_num(&hdr(2, enc)).expect("accept");
        assert_eq!(k, Some(stream[1]));
        assert_eq!(r.get_naks(), vec![1, 3]);

        // And sequence 5 gets the fifth value, proving nothing extra was consumed.
        let k = r.process_new_seq_num(&hdr(5, enc)).expect("accept");
        assert_eq!(k, Some(stream[4]));
    }

    /// An unencrypted packet carrying a new highest sequence takes the `+1` branch: it NAKs the
    /// gap **and its own sequence**, parking a key for each, and draws nothing for itself.
    ///
    /// The path is real: a peer's ACK-only packet carries the sequence of the last encrypted
    /// packet it sent, so when that packet is lost the ACK arrives as the newest. Its number is
    /// the lost packet's, the NAK asks for it, and the retransmission decrypts under the key
    /// parked here. This branch used to be a `debug_assert!` that panicked in a debug build.
    #[test]
    fn an_unencrypted_newest_sequence_naks_its_own_number_too() {
        let stream = [0x5DA2_2D96u32, 0xDB3B_A3B6, 0x9FD9_67F9, 0x0748_7047];
        let mut r = recv();
        let enc = PacketFlags::ENCRYPTED_CHECKSUM;
        assert_eq!(
            r.process_new_seq_num(&hdr(1, enc)).expect("accept"),
            Some(stream[0])
        );

        // Encrypted 2 and 3 are lost; the peer's ACK for them is stamped 3, in clear.
        assert_eq!(
            r.process_new_seq_num(&hdr(3, PacketFlags::ACK_SEQUENCE))
                .expect("accept"),
            None
        );
        assert_eq!(r.get_naks(), vec![2, 3], "the gap and the ACK's own number");
        assert_eq!(r.window.seq_ids_we_naked[&2], stream[1]);
        assert_eq!(r.window.seq_ids_we_naked[&3], stream[2]);
        assert_eq!(r.window.highest_id_received, 3);

        // The retransmissions decrypt under the parked keys, and 4 gets the fourth draw.
        assert_eq!(
            r.process_new_seq_num(&hdr(3, enc)).expect("accept"),
            Some(stream[2])
        );
        assert_eq!(
            r.process_new_seq_num(&hdr(2, enc)).expect("accept"),
            Some(stream[1])
        );
        assert!(r.get_naks().is_empty());
        assert_eq!(
            r.process_new_seq_num(&hdr(4, enc)).expect("accept"),
            Some(stream[3])
        );
    }

    /// `RejectRetransmit` forgets the parked keys without advancing `highest_id_received`. The
    /// stream stays aligned because the keys were already drawn.
    #[test]
    fn reject_retransmit_forgets_parked_keys_without_advancing() {
        let mut r = recv();
        let enc = PacketFlags::ENCRYPTED_CHECKSUM;
        r.process_new_seq_num(&hdr(4, enc)).expect("accept");
        assert_eq!(r.window.highest_id_received, 4);

        r.handle_empty_ack(&[1, 2, 3]);
        assert!(r.window.seq_ids_we_naked.is_empty());
        assert_eq!(r.window.highest_id_received, 4, "not advanced");
        assert_eq!(r.nak_state, ReceiverState::NoNak);

        // Sequence 5 still gets the fifth value: the stream never lost its place.
        assert_eq!(
            r.process_new_seq_num(&hdr(5, enc)).expect("accept"),
            Some(0x0A8E_4664)
        );
    }

    /// `seq_id_sanity_check` rejects a jump of more than 32767 and a NAK set over 40000.
    #[test]
    fn seq_id_sanity_check_bounds() {
        let mut r = recv();
        assert!(r.seq_id_sanity_check(0x7FFF).is_ok());
        assert_eq!(
            r.seq_id_sanity_check(0x8000),
            Err(RejectReason::SequenceJumpTooLarge)
        );

        // The cap is "more than 40000", so 40001 entries is the first rejection.
        let cap = u32::try_from(NAK_SET_CAP).expect("the cap fits a u32");
        for i in 1..=cap {
            r.window.seq_ids_we_naked.insert(i, 0);
        }
        assert!(r.seq_id_sanity_check(1).is_ok());
        r.window.seq_ids_we_naked.insert(cap + 1, 0);
        assert_eq!(r.seq_id_sanity_check(1), Err(RejectReason::NakSetOverflow));
    }

    /// A NAK burst is capped at 114 ids and is emitted in ascending order.
    #[test]
    fn get_naks_caps_at_114_in_order() {
        let mut r = recv();
        for i in (1..=300u32).rev() {
            r.window.seq_ids_we_naked.insert(i, 0);
        }
        let naks = r.get_naks();
        assert_eq!(naks.len(), MAX_NAK_IDS);
        assert_eq!(naks[0], 1);
        assert_eq!(u64::from(naks[MAX_NAK_IDS - 1]), MAX_NAK_IDS as u64);
        assert!(naks.windows(2).all(|w| w[0] < w[1]));
    }

    /// `add_nakked` on an id already present is a no-op, so the key is drawn exactly once.
    #[test]
    fn add_nakked_is_idempotent() {
        let mut r = recv();
        r.add_nakked(7, None);
        let first = r.window.seq_ids_we_naked[&7];
        r.add_nakked(7, None);
        assert_eq!(r.window.seq_ids_we_naked[&7], first);
        // The next fresh NAK gets the *second* stream value, not the third.
        r.add_nakked(8, None);
        assert_eq!(r.window.seq_ids_we_naked[&8], 0xDB3B_A3B6);
    }

    /// `accept` is the whole receive check: a packet whose checksum fails under its sequence's key
    /// is refused and its key parked again (never a fresh draw), so the resend is accepted; a
    /// resend under its parked key that fails parks the same key back and keeps `NAK_STATE`.
    #[test]
    fn accept_parks_the_key_of_a_failed_packet_for_its_resend() {
        let stream = [0x5DA2_2D96u32, 0xDB3B_A3B6, 0x9FD9_67F9];
        let enc = PacketFlags::ENCRYPTED_CHECKSUM;
        let mut r = recv();
        assert_eq!(
            r.accept(&hdr(1, enc), |k| k == Some(stream[0])),
            Ok(Some(stream[0]))
        );
        assert_eq!(
            r.accept(&hdr(2, enc), |_| false),
            Err(RejectReason::BadChecksum)
        );
        assert_eq!(
            r.window.seq_ids_we_naked.get(&2),
            Some(&stream[1]),
            "the key it was checked against"
        );
        assert_eq!(r.nak_state, ReceiverState::Nak);
        // A resend that fails again: the parked key goes back, the state stays NAK.
        r.nak_state = ReceiverState::NoNak;
        assert_eq!(
            r.accept(&hdr(2, enc), |_| false),
            Err(RejectReason::BadChecksum)
        );
        assert_eq!(r.window.seq_ids_we_naked.get(&2), Some(&stream[1]));
        assert_eq!(r.nak_state, ReceiverState::Nak);
        assert_eq!(
            r.accept(&hdr(2, enc), |k| k == Some(stream[1])),
            Ok(Some(stream[1]))
        );
        assert_eq!(r.nak_state, ReceiverState::NoNak);
        assert_eq!(
            r.accept(&hdr(3, enc), |k| k == Some(stream[2])),
            Ok(Some(stream[2])),
            "no extra draw"
        );
        // Unsequenced: never encrypted, no key.
        assert_eq!(
            r.accept(&hdr(0, enc), |_| true),
            Err(RejectReason::UnsequencedButEncrypted)
        );
        assert_eq!(r.accept(&hdr(0, 0), |k| k.is_none()), Ok(None));
        // A slot with no connection refuses a sequenced packet; a bare window has no such check.
        let mut idle = ReceiverData::new(0, 0xDEAD_BEEF, 0);
        assert_eq!(
            idle.accept(&hdr(1, enc), |_| true),
            Err(RejectReason::NoConnection)
        );
        let mut w = SequenceWindow::new(0xDEAD_BEEF);
        assert_eq!(w.accept(&hdr(1, enc), |_| true), Ok(Some(stream[0])));
    }

    /// The retransmit cache strips disposable headers when caching, and re-encrypts with the
    /// packet's stored key.
    ///
    /// Oracle: step 2, and the flow queue's ack dequeue.
    #[test]
    fn cache_strips_disposable_headers_and_retransmit_reuses_the_stored_key() {
        let mut store = SentPacketHistory::new();
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 9,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::ACK_SEQUENCE, vec![0; 4])
            .expect("ack");
        p.add_optional_header(PacketFlags::TIME_SYNC, vec![0; 8])
            .expect("timesync");
        p.add_fragment(Fragment::new(FragmentHeader::default(), vec![1, 2, 3, 4]))
            .expect("frag");
        let key = 0x5DA2_2D96;
        let first = p.serialize(Some(key)).expect("first send");

        store.add_sent_packet(p, LocalTime(0.0));
        let cached = store.get(9).expect("cached");
        assert_eq!(
            cached.optional.keys().copied().collect::<Vec<_>>(),
            vec![PacketFlags::TIME_SYNC],
            "the disposable ACK must not survive into a retransmission"
        );
        assert_eq!(cached.crypto_key, Some(key));

        let mut resend = cached.clone();
        let bytes = reserialize_for_retransmit(&mut resend).expect("resend");
        assert_ne!(bytes, first, "the ACK is gone and Retransmission is set");
        assert!(resend.header.header.contains(PacketFlags::RETRANSMISSION));
        assert!(resend.header.header.is_encrypted());

        // The receiver recovers the same key it parked.
        let parsed = crate::ParsedPacket::parse(&bytes).expect("parse");
        assert_eq!(parsed.recovered_key(), key);
    }

    /// The cumulative ACK evicts everything **strictly below** its sequence, and the named
    /// sequence itself survives.
    ///
    /// Oracle: the cache walk checks `if (seq_num == flush) return 1;` before any
    /// comparison. A boundary one packet off (`flush(3) == 3` on a cache of 1..=5) deletes the
    /// packet the NAK processor names, which is the one a second NAK would need.
    #[test]
    fn flush_evicts_strictly_below_the_ack_and_keeps_the_named_sequence() {
        let mut store = SentPacketHistory::new();
        for seq in 1..=5u32 {
            let mut p = OutPacket::new(ProtoHeader {
                seq_id: seq,
                ..Default::default()
            });
            p.add_fragment(Fragment::new(FragmentHeader::default(), vec![0; 4]))
                .expect("frag");
            store.add_sent_packet(p, LocalTime(0.0));
        }
        assert_eq!(store.len(), 5);
        assert_eq!(store.flush(3), 2, "1 and 2 go; 3 is kept");
        assert_eq!(store.len(), 3);
        assert!(!store.contains(2));
        assert!(
            store.contains(3),
            "the NAKed sequence must still be resendable"
        );
        assert!(store.contains(4));
        assert!(store.contains(5));
    }

    /// The bound this crate adds. Eviction by age never removes a packet the ACK cadence would
    /// still have flushed.
    #[test]
    fn the_cache_is_bounded_by_age() {
        let mut store = SentPacketHistory::new();
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 1,
            ..Default::default()
        });
        p.add_fragment(Fragment::new(FragmentHeader::default(), vec![0; 4]))
            .expect("frag");
        store.add_sent_packet(p, LocalTime(0.0));
        assert_eq!(store.len(), 1);

        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 2,
            ..Default::default()
        });
        p.add_fragment(Fragment::new(FragmentHeader::default(), vec![0; 4]))
            .expect("frag");
        store.add_sent_packet(p, LocalTime(SENT_CACHE_MAX_AGE + 1.0));
        assert_eq!(store.len(), 1, "the 120 s old packet aged out");
        assert!(store.contains(2));
    }

    /// Behaviour: link.flow.each-closed-server-interval-is-reported-with-its-byte-count
    ///
    /// The first datagram starts the count without a report; a datagram stamped with a newer
    /// interval closes the counted one, reporting its bytes (headers included) and its id, and
    /// is itself counted into the new one; an older or equal stamp closes nothing. The compare
    /// wraps at 16 bits.
    #[test]
    fn a_newer_interval_closes_the_counted_one_with_its_byte_count() {
        let mut r = recv();
        assert_eq!(
            r.account_datagram(1088, 52),
            None,
            "the first sets the interval"
        );
        assert_eq!(r.account_datagram(1088, 28), None);
        assert_eq!(
            r.account_datagram(1087, 40),
            None,
            "an older stamp is counted, not closed"
        );
        let report = r
            .account_datagram(1090, 24)
            .expect("a newer stamp closes 1088");
        assert_eq!(
            u32::from_le_bytes([report[0], report[1], report[2], report[3]]),
            120
        );
        assert_eq!(u16::from_le_bytes([report[4], report[5]]), 1088);
        assert_eq!((r.current_remote_interval, r.bytes_received), (1090, 24));

        // Across the wrap: 2 is newer than 0xFFFE.
        let mut r = recv();
        let _ = r.account_datagram(0xFFFE, 30);
        let report = r.account_datagram(2, 30).expect("2 is newer than 0xFFFE");
        assert_eq!(u16::from_le_bytes([report[4], report[5]]), 0xFFFE);
        assert_eq!(
            u32::from_le_bytes([report[0], report[1], report[2], report[3]]),
            30
        );
    }
}
