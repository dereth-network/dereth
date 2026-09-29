//! One session's datagrams through the transport: `dereth-transport` takes each datagram apart
//! (`ParsedPacket`), puts fragments back together into blobs (`NetBlob`), and the checksums are
//! verified where the capture carries the key-stream seeds.
//!
//! Three rules sit on top of the transport's own:
//!
//! * a fragment-carrying datagram whose (direction, sequence number) was already seen is a
//!   duplicate delivery and its fragments are skipped (the rule the client's sequence gate
//!   applies, and the one `dereth-corpus` applies to the owned recordings);
//! * a blob still incomplete when the session ends is an **orphan**: at the head (its fragments
//!   arrived in the direction's first datagrams: the capture started mid-blob), at the tail (in
//!   the last datagrams: it stopped mid-blob), or in the middle, which means loss inside the
//!   capture and is counted separately;
//! * checksums are **reported, never required**. A plaintext datagram's checksum is always
//!   checkable; an encrypted one only once the session's `ConnectRequest` has given the seeds, and
//!   only until the first mismatch in that direction, after which the key walk has lost its place
//!   (`desynced_after` records how many had matched).
//!
//! Every message is decoded as its session's game ([`crate::game`]). The game is decided on the
//! session's first [`LEAD_IN`] messages, which are held back undecoded until then (or until the
//! session ends, if it has fewer); every later message is decoded as it completes. The summary
//! carries both the game the whole session's messages vote for ([`Summary::game`]) and the one its
//! messages were decoded as ([`Summary::decoded_as`]); when their decoders differ the writer
//! decodes the session again ([`crate::index::redecode_session`]).

use std::collections::{BTreeMap, BTreeSet};

use dereth_transport::blob::NetBlob;
use dereth_transport::wire::optional::pstring_unpack;
use dereth_transport::{CryptoSystem, PacketFlags, ParsedPacket};

use crate::decode::{decode_as, Decoded};
use crate::flows::Dir;
use crate::game::{form, Evidence, Game};

/// Messages a session's game is decided on before any is decoded.
pub const LEAD_IN: usize = 64;

/// One datagram of a session.
#[derive(Debug, Clone, PartialEq)]
pub struct PacketRow {
    /// Position in the session, both directions, from 0.
    pub idx: u32,
    pub dir: Dir,
    /// Seconds since the session's first datagram.
    pub t: f64,
    pub seq: u32,
    pub flags: u32,
    /// The datagram's length (the UDP payload).
    pub len: u32,
    /// Fragments it carried.
    pub frags: u16,
    /// `Some(true)` when the checksum was verified, `Some(false)` when it did not match, `None`
    /// when it could not be checked.
    pub checksum: Option<bool>,
    /// A duplicate delivery whose fragments were skipped.
    pub duplicate: bool,
    /// Why `dereth-transport` refused to parse it.
    pub parse_error: Option<String>,
}

/// One reassembled message.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageRow {
    /// Position in the session, both directions, in the order each one's last fragment arrived.
    pub idx: u32,
    pub dir: Dir,
    /// Seconds since the session's first datagram, at the datagram that completed it.
    pub t: f64,
    /// The datagram that completed it.
    pub packet_idx: u32,
    pub queue: u16,
    pub blob_id: u64,
    pub frags: u16,
    pub raw: Vec<u8>,
    pub decoded: Decoded,
}

/// Where a session's rows go.
pub trait Sink {
    fn packet(&mut self, p: PacketRow);
    fn message(&mut self, m: MessageRow);
}

/// Rows collected in memory.
#[derive(Debug, Default)]
pub struct VecSink {
    pub packets: Vec<PacketRow>,
    pub messages: Vec<MessageRow>,
}

impl Sink for VecSink {
    fn packet(&mut self, p: PacketRow) {
        self.packets.push(p);
    }
    fn message(&mut self, m: MessageRow) {
        self.messages.push(m);
    }
}

/// Checksum results for one direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChecksumStats {
    /// Datagrams whose checksum was computed.
    pub checked: u64,
    /// Of those, the ones that matched.
    pub ok: u64,
    /// Encrypted datagrams that could not be checked (no seeds, or after a desync).
    pub unchecked: u64,
    /// Encrypted datagrams that matched before the first mismatch, when there was one.
    pub desynced_after: Option<u64>,
}

/// What a session was, once it has ended.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    /// Absolute time (seconds since the epoch) of the first and last datagram.
    pub t_first: f64,
    pub t_last: f64,
    /// No `ConnectRequest`: the capture began after the handshake.
    pub partial: bool,
    /// The client's `LoginRequest` was captured.
    pub login: bool,
    /// The client's `WorldLoginRequest` (the reconnect after a referral) was captured.
    pub world_login: bool,
    /// The server sent a `Referral`, moving the client to another server.
    pub referral: bool,
    /// The client version string from the `LoginRequest`.
    pub version: Option<String>,
    /// The key-stream seeds (server to client, client to server).
    pub seeds: Option<(u32, u32)>,
    pub disconnect: bool,
    /// Per direction (`Dir::index`).
    pub packets: [u64; 2],
    pub bytes: [u64; 2],
    pub messages: [u64; 2],
    /// Messages by [`crate::decode::Status`] per direction: ok, error, unknown, short, ac2.
    pub status: [[u64; 5]; 2],
    /// The session's messages counted by the form of their first dword.
    pub evidence: Evidence,
    /// The game the whole session's messages vote for ([`Evidence::verdict`]).
    pub game: Game,
    /// The game its messages were decoded as: the verdict on the first [`LEAD_IN`] messages.
    pub decoded_as: Game,
    pub parse_errors: u64,
    pub duplicates: u64,
    pub refused_fragments: u64,
    pub orphans_head: u64,
    pub orphans_tail: u64,
    pub orphans_mid: u64,
    pub checksum: [ChecksumStats; 2],
}

/// The sequence number the first key of a connection's stream belongs to: after the handshake
/// (which is unsequenced) both sides number from 2, and a datagram 2 the capture missed still drew
/// the first key.
const FIRST_ENCRYPTED_SEQ: u32 = 2;

/// Which key belongs to which sequence number, one direction. The stream advances once per
/// encrypted datagram *sent*: a sequence number the capture never shows still drew a key, and a
/// retransmission or a duplicate reuses the key its number drew first.
#[derive(Debug)]
struct Ledger {
    stream: CryptoSystem,
    drawn: BTreeMap<u32, u32>,
    next: Option<u32>,
}

impl Ledger {
    fn key(&mut self, seq: u32) -> Option<u32> {
        if let Some(k) = self.drawn.get(&seq) {
            return Some(*k);
        }
        if let Some(next) = self.next {
            if seq < next {
                return None;
            }
            // A gap this large is not loss; the walk has nothing to line up with.
            if seq - next > 100_000 {
                return None;
            }
            for missing in next..seq {
                let k = self.stream.next();
                self.drawn.insert(missing, k);
            }
        }
        let k = self.stream.next();
        self.drawn.insert(seq, k);
        self.next = Some(seq + 1);
        // Keys far behind the walk are never asked for again.
        if self.drawn.len() > 8192 {
            let cut = seq.saturating_sub(4096);
            self.drawn = self.drawn.split_off(&cut);
        }
        Some(k)
    }
}

/// A completed message not yet decoded, while the session's game is undecided.
#[derive(Debug)]
struct Held {
    idx: u32,
    dir: Dir,
    t: f64,
    packet_idx: u32,
    queue: u16,
    blob_id: u64,
    frags: u16,
    raw: Vec<u8>,
}

#[derive(Debug)]
struct Pending {
    blob: NetBlob,
    first_dir_packet: u64,
    last_dir_packet: u64,
    frags: u16,
}

/// Datagrams at either end of a direction within which an incomplete blob is a capture-edge
/// orphan rather than loss in the middle.
const EDGE: u64 = 64;

/// One session's transport state.
#[derive(Debug)]
pub struct Session {
    t0: Option<f64>,
    next_packet: u32,
    next_message: u32,
    seen: [BTreeSet<u32>; 2],
    pending: BTreeMap<(usize, u64), Pending>,
    ledgers: [Option<Ledger>; 2],
    desynced: [bool; 2],
    /// Encrypted datagrams whose checksum matched, per direction.
    encrypted_ok: [u64; 2],
    /// The game messages are decoded as, once decided.
    decode_as: Option<Game>,
    held: Vec<Held>,
    summary: Summary,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            t0: None,
            next_packet: 0,
            next_message: 0,
            seen: [BTreeSet::new(), BTreeSet::new()],
            pending: BTreeMap::new(),
            ledgers: [None, None],
            desynced: [false; 2],
            encrypted_ok: [0; 2],
            decode_as: None,
            held: Vec::new(),
            summary: Summary {
                partial: true,
                ..Summary::default()
            },
        }
    }

    /// Take one datagram.
    pub fn datagram(&mut self, dir: Dir, ts: f64, bytes: &[u8], sink: &mut dyn Sink) {
        let t0 = *self.t0.get_or_insert(ts);
        if self.next_packet == 0 {
            self.summary.t_first = ts;
        }
        self.summary.t_last = ts;
        let d = dir.index();
        let idx = self.next_packet;
        self.next_packet += 1;
        let dir_packet = self.summary.packets[d];
        self.summary.packets[d] += 1;
        self.summary.bytes[d] += bytes.len() as u64;
        let mut row = PacketRow {
            idx,
            dir,
            t: ts - t0,
            seq: 0,
            flags: 0,
            len: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            frags: 0,
            checksum: None,
            duplicate: false,
            parse_error: None,
        };
        if bytes.len() >= 8 {
            row.seq = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            row.flags = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        }
        let p = match ParsedPacket::parse(bytes) {
            Ok(p) => p,
            Err(e) => {
                self.summary.parse_errors += 1;
                row.parse_error = Some(e.to_string());
                sink.packet(row);
                return;
            }
        };
        let flags = p.header.header;
        row.frags = u16::try_from(p.fragments.len()).unwrap_or(u16::MAX);
        if flags.contains(PacketFlags::DISCONNECT) {
            self.summary.disconnect = true;
        }
        if dir == Dir::C2s && flags.contains(PacketFlags::WORLD_LOGIN_REQUEST) {
            self.summary.world_login = true;
        }
        if dir == Dir::S2c && flags.contains(PacketFlags::REFERRAL) {
            self.summary.referral = true;
        }
        if dir == Dir::C2s && flags.contains(PacketFlags::LOGIN_REQUEST) {
            self.summary.login = true;
            if self.summary.version.is_none() {
                if let Some((v, _)) = p
                    .optional
                    .get(&PacketFlags::LOGIN_REQUEST)
                    .and_then(|b| pstring_unpack(b))
                {
                    self.summary.version = Some(String::from_utf8_lossy(v).into_owned());
                }
            }
        }
        if dir == Dir::S2c
            && flags.contains(PacketFlags::CONNECT_REQUEST)
            && self.summary.seeds.is_none()
        {
            if let Some(b) = p
                .optional
                .get(&PacketFlags::CONNECT_REQUEST)
                .filter(|b| b.len() >= 0x1C)
            {
                let at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
                let (s2c, c2s) = (at(0x14), at(0x18));
                self.summary.seeds = Some((s2c, c2s));
                self.summary.partial = false;
                self.ledgers = [
                    Some(Ledger {
                        stream: CryptoSystem::new(c2s),
                        drawn: BTreeMap::new(),
                        next: Some(FIRST_ENCRYPTED_SEQ),
                    }),
                    Some(Ledger {
                        stream: CryptoSystem::new(s2c),
                        drawn: BTreeMap::new(),
                        next: Some(FIRST_ENCRYPTED_SEQ),
                    }),
                ];
            }
        }
        row.checksum = self.check(d, &p);

        if flags.has_fragments() {
            if !self.seen[d].insert(p.header.seq_id) {
                self.summary.duplicates += 1;
                row.duplicate = true;
                sink.packet(row);
                return;
            }
            // Old sequence numbers are never repeated this far back.
            if self.seen[d].len() > 16_384 {
                let cut = p.header.seq_id.saturating_sub(8192);
                self.seen[d] = self.seen[d].split_off(&cut);
            }
        }
        let t = row.t;
        sink.packet(row);
        for f in &p.fragments {
            let id = f.header.blob_id();
            let entry = self.pending.entry((d, id)).or_insert_with(|| Pending {
                blob: NetBlob::for_recv(0),
                first_dir_packet: dir_packet,
                last_dir_packet: dir_packet,
                frags: 0,
            });
            entry.last_dir_packet = dir_packet;
            if !entry.blob.receive_add_fragment(f) {
                self.summary.refused_fragments += 1;
                continue;
            }
            entry.frags += 1;
            if !entry.blob.is_complete() {
                continue;
            }
            let Some(mut done) = self.pending.remove(&(d, id)) else {
                continue;
            };
            let raw = done.blob.take_payload();
            self.summary.evidence.add(form(&raw));
            let h = Held {
                idx: self.next_message,
                dir,
                t,
                packet_idx: idx,
                queue: done.blob.queue_id,
                blob_id: id,
                frags: done.frags,
                raw,
            };
            self.next_message += 1;
            if let Some(game) = self.decode_as {
                self.emit(game, h, sink);
            } else {
                self.held.push(h);
                if self.held.len() >= LEAD_IN {
                    self.decide(self.summary.evidence.verdict(), sink);
                }
            }
        }
    }

    /// Fix the game messages are decoded as, and decode the ones held back.
    fn decide(&mut self, game: Game, sink: &mut dyn Sink) {
        self.decode_as = Some(game);
        for h in std::mem::take(&mut self.held) {
            self.emit(game, h, sink);
        }
    }

    fn emit(&mut self, game: Game, h: Held, sink: &mut dyn Sink) {
        let decoded = decode_as(game, h.dir, &h.raw);
        let d = h.dir.index();
        self.summary.status[d][decoded.status.index()] += 1;
        self.summary.messages[d] += 1;
        sink.message(MessageRow {
            idx: h.idx,
            dir: h.dir,
            t: h.t,
            packet_idx: h.packet_idx,
            queue: h.queue,
            blob_id: h.blob_id,
            frags: h.frags,
            raw: h.raw,
            decoded,
        });
    }

    fn check(&mut self, d: usize, p: &ParsedPacket) -> Option<bool> {
        let cs = &mut self.summary.checksum[d];
        if !p.header.header.is_encrypted() {
            let ok = p.checksum_ok(None);
            cs.checked += 1;
            cs.ok += u64::from(ok);
            return Some(ok);
        }
        if self.desynced[d] {
            cs.unchecked += 1;
            return None;
        }
        let Some(ledger) = self.ledgers[d].as_mut() else {
            cs.unchecked += 1;
            return None;
        };
        let Some(key) = ledger.key(p.header.seq_id) else {
            cs.unchecked += 1;
            return None;
        };
        let ok = p.checksum_ok(Some(key));
        cs.checked += 1;
        if ok {
            cs.ok += 1;
            self.encrypted_ok[d] += 1;
        } else {
            cs.desynced_after = Some(self.encrypted_ok[d]);
            self.desynced[d] = true;
        }
        Some(ok)
    }

    /// End the session: decode what was held back, settle its game, and classify what is still
    /// incomplete.
    #[must_use]
    pub fn finish(mut self, sink: &mut dyn Sink) -> Summary {
        let game = self.summary.evidence.verdict();
        if self.decode_as.is_none() {
            self.decide(game, sink);
        }
        self.summary.game = game;
        self.summary.decoded_as = self.decode_as.unwrap_or(game);
        for ((d, _), p) in std::mem::take(&mut self.pending) {
            let n = self.summary.packets[d];
            if p.first_dir_packet < EDGE {
                self.summary.orphans_head += 1;
            } else if p.last_dir_packet + EDGE >= n {
                self.summary.orphans_tail += 1;
            } else {
                self.summary.orphans_mid += 1;
            }
        }
        self.summary
    }
}

/// Datagram builders for tests, on `dereth-transport`'s own serialiser.
pub mod build {
    use dereth_transport::wire::optional::pstring_pack;
    use dereth_transport::{Fragment, FragmentHeader, OutPacket, PacketFlags, ProtoHeader};

    /// A client `LoginRequest` carrying `version`.
    #[must_use]
    pub fn login_request(version: &str) -> Vec<u8> {
        let mut body = pstring_pack(version.as_bytes());
        body.extend_from_slice(&4u32.to_le_bytes());
        body.extend_from_slice(&[1, 2, 3, 4]);
        let mut p = OutPacket::new(ProtoHeader::default());
        p.add_optional_header(PacketFlags::LOGIN_REQUEST, body)
            .expect("login");
        p.serialize(None).expect("serialises")
    }

    /// A server `ConnectRequest` carrying the two seeds (server to client, client to server).
    #[must_use]
    pub fn connect_request(s2c_seed: u32, c2s_seed: u32) -> Vec<u8> {
        let mut body = vec![0u8; 32];
        body[0x14..0x18].copy_from_slice(&s2c_seed.to_le_bytes());
        body[0x18..0x1C].copy_from_slice(&c2s_seed.to_le_bytes());
        let mut p = OutPacket::new(ProtoHeader::default());
        p.add_optional_header(PacketFlags::CONNECT_REQUEST, body)
            .expect("connect");
        p.serialize(None).expect("serialises")
    }

    /// A datagram carrying fragments `(blob id, fragment index, fragment count, bytes)`, encrypted
    /// with `key`.
    #[must_use]
    pub fn fragments(seq: u32, key: u32, frags: &[(u64, u16, u16, &[u8])]) -> Vec<u8> {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: seq,
            ..ProtoHeader::default()
        });
        for (id, num, count, bytes) in frags {
            let h = FragmentHeader {
                blob_id_low: u32::try_from(id & 0xFFFF_FFFF).unwrap_or(0),
                blob_id_high: u32::try_from(id >> 32).unwrap_or(0),
                num_frags: *count,
                blob_frag_size: 0,
                blob_num: *num,
                queue_id: 5,
            };
            p.add_fragment(Fragment::new(h, bytes.to_vec()))
                .expect("fragment");
        }
        p.serialize(Some(key)).expect("serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;
    use crate::decode::Status;
    use dereth_transport::CryptoSystem;

    const MAX: usize = 448;

    #[test]
    fn a_handshake_gives_seeds_version_and_verified_checksums() {
        let (s2c_seed, c2s_seed) = (0x1234_5678, 0x9ABC_DEF0);
        let mut s2c = CryptoSystem::new(s2c_seed);
        let mut s = Session::new();
        let mut sink = VecSink::default();
        s.datagram(Dir::C2s, 100.0, &login_request("1802"), &mut sink);
        s.datagram(
            Dir::S2c,
            100.1,
            &connect_request(s2c_seed, c2s_seed),
            &mut sink,
        );
        // A two-fragment message, the fragments in separate datagrams.
        let blob: Vec<u8> = [0xE1u8, 0xF7, 0, 0]
            .iter()
            .copied()
            .chain((0..500).map(|i| (i % 251) as u8))
            .collect();
        let (a, b) = blob.split_at(MAX);
        s.datagram(
            Dir::S2c,
            100.2,
            &fragments(2, s2c.next(), &[(7, 0, 2, a)]),
            &mut sink,
        );
        let k2 = s2c.next();
        s.datagram(
            Dir::S2c,
            100.3,
            &fragments(3, k2, &[(7, 1, 2, b)]),
            &mut sink,
        );
        // The same datagram delivered twice: counted, fragments skipped.
        s.datagram(
            Dir::S2c,
            100.4,
            &fragments(3, k2, &[(7, 1, 2, b)]),
            &mut sink,
        );
        let sum = s.finish(&mut sink);
        assert!(!sum.partial);
        assert!(sum.login);
        assert_eq!(sum.version.as_deref(), Some("1802"));
        assert_eq!(sum.seeds, Some((s2c_seed, c2s_seed)));
        assert_eq!(sink.messages.len(), 1);
        let m = &sink.messages[0];
        assert_eq!(m.raw, blob);
        assert_eq!((m.blob_id, m.frags, m.queue, m.packet_idx), (7, 2, 5, 3));
        assert!((m.t - 0.3).abs() < 1e-9);
        assert_eq!(sum.duplicates, 1);
        assert_eq!(sum.checksum[Dir::S2c.index()].ok, 4, "{sum:?}");
        assert_eq!(sum.checksum[Dir::S2c.index()].desynced_after, None);
        assert_eq!(sum.checksum[Dir::C2s.index()].ok, 1);
        assert_eq!(
            sink.packets
                .iter()
                .filter(|p| p.checksum == Some(true))
                .count(),
            5
        );
    }

    #[test]
    fn a_wrong_key_desyncs_the_direction_and_stops_checking_it() {
        let mut s2c = CryptoSystem::new(1);
        let mut s = Session::new();
        let mut sink = VecSink::default();
        s.datagram(Dir::S2c, 0.0, &connect_request(1, 2), &mut sink);
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(2, s2c.next(), &[(1, 0, 1, &[1, 0, 0, 0])]),
            &mut sink,
        );
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(3, 0xDEAD, &[(2, 0, 1, &[2, 0, 0, 0])]),
            &mut sink,
        );
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(4, s2c.next(), &[(3, 0, 1, &[3, 0, 0, 0])]),
            &mut sink,
        );
        let sum = s.finish(&mut sink);
        let c = sum.checksum[Dir::S2c.index()];
        assert_eq!(c.desynced_after, Some(1));
        assert_eq!(c.unchecked, 1);
        assert_eq!(
            sum.messages[Dir::S2c.index()],
            3,
            "reassembly does not depend on checksums"
        );
    }

    /// The first datagram after the handshake is numbered 2 and draws the first key, so a capture
    /// that missed it must still park that key before checking datagram 3.
    #[test]
    fn a_missed_first_datagram_still_lets_the_key_walk_line_up() {
        let mut s2c = CryptoSystem::new(9);
        let mut s = Session::new();
        let mut sink = VecSink::default();
        s.datagram(Dir::S2c, 0.0, &connect_request(9, 10), &mut sink);
        let _missed = s2c.next();
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(3, s2c.next(), &[(1, 0, 1, &[1, 0, 0, 0])]),
            &mut sink,
        );
        let sum = s.finish(&mut sink);
        assert_eq!(sum.checksum[Dir::S2c.index()].desynced_after, None);
        assert_eq!(sum.checksum[Dir::S2c.index()].ok, 2);
    }

    #[test]
    fn a_session_without_a_handshake_is_partial_and_its_edge_orphans_are_classified() {
        let mut s = Session::new();
        let mut sink = VecSink::default();
        // Starts mid-blob: fragment 1 of 2 with no fragment 0.
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(10, 5, &[(1, 1, 2, &[9; 10])]),
            &mut sink,
        );
        // A lost fragment in the middle of the capture.
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(11, 5, &[(2, 0, 2, &[9; MAX])]),
            &mut sink,
        );
        for i in 0..200 {
            s.datagram(
                Dir::S2c,
                0.0,
                &fragments(12 + i, 5, &[(100 + u64::from(i), 0, 1, &[1, 0, 0, 0])]),
                &mut sink,
            );
        }
        // Ends mid-blob.
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(300, 5, &[(3, 0, 2, &[9; MAX])]),
            &mut sink,
        );
        let sum = s.finish(&mut sink);
        assert!(sum.partial);
        assert_eq!(sum.checksum[Dir::S2c.index()].unchecked, 203);
        assert_eq!(sum.messages[Dir::S2c.index()], 200);
        // Blob 2 began in the direction's first datagrams, so it counts as the head too.
        assert_eq!(
            (sum.orphans_head, sum.orphans_mid, sum.orphans_tail),
            (2, 0, 1)
        );
    }

    #[test]
    fn loss_in_the_middle_is_a_mid_orphan() {
        let mut s = Session::new();
        let mut sink = VecSink::default();
        for i in 0..100 {
            s.datagram(
                Dir::S2c,
                0.0,
                &fragments(1 + i, 5, &[(u64::from(i), 0, 1, &[1, 0, 0, 0])]),
                &mut sink,
            );
        }
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(200, 5, &[(999, 0, 2, &[9; MAX])]),
            &mut sink,
        );
        for i in 0..100 {
            s.datagram(
                Dir::S2c,
                0.0,
                &fragments(201 + i, 5, &[(1000 + u64::from(i), 0, 1, &[1, 0, 0, 0])]),
                &mut sink,
            );
        }
        assert_eq!(s.finish(&mut sink).orphans_mid, 1);
    }

    /// A session of `n` one-fragment messages, the `i`th built by `blob(i)`.
    fn session_of(n: u32, blob: impl Fn(u32) -> Vec<u8>) -> (Summary, VecSink) {
        let mut s = Session::new();
        let mut sink = VecSink::default();
        for i in 0..n {
            let dir = if i % 3 == 0 { Dir::C2s } else { Dir::S2c };
            s.datagram(
                dir,
                f64::from(i),
                &fragments(10 + i, 5, &[(u64::from(i), 0, 1, &blob(i))]),
                &mut sink,
            );
        }
        let sum = s.finish(&mut sink);
        (sum, sink)
    }

    fn ac2_blob(i: u32) -> Vec<u8> {
        let mut b = vec![0x86, 0x00, 0x01, 0x00];
        b.extend_from_slice(&i.to_le_bytes());
        b
    }

    #[test]
    fn an_ac2_session_is_reassembled_as_ever_and_its_messages_stored_whole_as_ac2() {
        let (sum, sink) = session_of(100, ac2_blob);
        assert_eq!((sum.game, sum.decoded_as), (Game::Ac2, Game::Ac2));
        assert_eq!(
            (sum.evidence.ac1, sum.evidence.ac2, sum.evidence.other),
            (0, 100, 0)
        );
        assert_eq!(sink.messages.len(), 100);
        assert_eq!(sum.status[Dir::S2c.index()], [0, 0, 0, 0, 66]);
        assert_eq!(sum.status[Dir::C2s.index()], [0, 0, 0, 0, 34]);
        for (i, m) in sink.messages.iter().enumerate() {
            assert_eq!(
                m.idx as usize, i,
                "message order is kept across the lead-in"
            );
            assert_eq!(m.raw, ac2_blob(m.idx));
            assert_eq!(
                (m.decoded.status, m.decoded.mtype, m.decoded.codec),
                (Status::Ac2, 0x0001_0086, None)
            );
        }
    }

    #[test]
    fn an_ac1_session_decodes_as_ac1_and_the_lead_in_is_held_until_decided() {
        let info = dereth_protocol::login::LoginWorldInfo {
            connections: 1,
            max_connections: 2,
            world_name: "W".into(),
        };
        let blob = dereth_protocol::write_blob(&info).unwrap();
        let mut s = Session::new();
        let mut sink = VecSink::default();
        let n = u32::try_from(LEAD_IN).unwrap();
        for i in 0..n - 1 {
            s.datagram(
                Dir::S2c,
                0.0,
                &fragments(10 + i, 5, &[(u64::from(i), 0, 1, &blob)]),
                &mut sink,
            );
        }
        assert!(sink.messages.is_empty(), "held until the game is decided");
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(10 + n, 5, &[(u64::from(n), 0, 1, &blob)]),
            &mut sink,
        );
        assert_eq!(sink.messages.len(), LEAD_IN);
        s.datagram(
            Dir::S2c,
            0.0,
            &fragments(11 + n, 5, &[(u64::from(n) + 1, 0, 1, &blob)]),
            &mut sink,
        );
        assert_eq!(
            sink.messages.len(),
            LEAD_IN + 1,
            "decoded as it completes once decided"
        );
        let sum = s.finish(&mut sink);
        assert_eq!((sum.game, sum.decoded_as), (Game::Ac1, Game::Ac1));
        assert_eq!(sum.status[Dir::S2c.index()], [65, 0, 0, 0, 0]);
        assert!(sink.messages.iter().all(|m| m.decoded.status == Status::Ok));
    }

    /// A lead-in of AC1 messages in a session that is AC2 overall: decoded as the lead-in said,
    /// and the summary shows the disagreement for the writer to decode it again.
    #[test]
    fn a_lead_in_that_disagrees_with_the_whole_session_is_reported() {
        let (sum, _) = session_of(400, |i| {
            if (i as usize) < LEAD_IN {
                vec![0xE1, 0xF7, 0, 0]
            } else {
                ac2_blob(i)
            }
        });
        assert_eq!((sum.game, sum.decoded_as), (Game::Ac2, Game::Ac1));
        let (sum, _) = session_of(3, ac2_blob);
        assert_eq!(
            (sum.game, sum.decoded_as),
            (Game::Ac2, Game::Ac2),
            "a short session is decided at its end"
        );
    }

    #[test]
    fn an_unparseable_datagram_is_recorded_with_its_reason() {
        let mut s = Session::new();
        let mut sink = VecSink::default();
        s.datagram(Dir::C2s, 0.0, &[1, 2, 3], &mut sink);
        let sum = s.finish(&mut sink);
        assert_eq!(sum.parse_errors, 1);
        assert!(sink.packets[0].parse_error.is_some());
    }
}
