//! `NetBlob`, the 64-bit blob id, fragmentation and reassembly.
//!
//! A blob is one complete application message: a flat byte buffer whose first dword is the opcode.
//! On the way out it is split into 448-byte fragments and scattered across packets; on the way in
//! the fragments are collected by the connection's [`crate::indicator::Indicator`]. `dereth-transport` never
//! looks past that first byte — message bodies belong to `dereth-protocol`.
//!
//! See `docs/networking/04-netblobs-and-queues.md` §§1-3.

use crate::wire::{Fragment, FragmentHeader, MAX_FRAG_DATA};

/// The 64-bit NetBlobID, split across the fragment header's `Sequence` (low dword) and `Id` (high
/// dword).
///
/// **Never compare two of these numerically**: the ordering stamp in bits 32-47 wraps, so
/// arithmetic ordering is meaningless. Use [`NetBlobId::lhs_newer_ordering_stamp`].
///
/// Bit layout:
///
/// | bits | width | meaning |
/// |---|---:|---|
/// | 0-31 | 32 | sequence counter, the low dword |
/// | 32-47 | 16 | ordering stamp, a wrapping version number within one stream |
/// | 48-55 | 8 | sequence-id byte, part of the stream identity; always 0 in client-made blobs |
/// | 56-60 | 5 | ordering type |
/// | 61-62 | 2 | 0x20 is set on the client's login-bound blobs; no read site is known |
/// | 63 | 1 | ephemeral flag |
///
/// See `docs/networking/04-netblobs-and-queues.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct NetBlobId(pub u64);

impl NetBlobId {
    /// The ordering type used for world-bound queues, positioned in
    /// the high dword.
    pub const ORDERING_TYPE_WORLD: u32 = 0x0300_0000;
    /// The ordering type for the login-bound queues 4, 5 and 8. The extra `0x20` is bit 61, which
    /// nothing in the client reads back — but it is on the wire, so it must be right
    /// (the resolved compatibility notes #38).
    pub const ORDERING_TYPE_LOGIN: u32 = 0x2300_0000;

    #[must_use]
    pub const fn high32(self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.0 >> 32) as u32
        }
    }

    #[must_use]
    pub const fn low32(self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        {
            self.0 as u32
        }
    }

    /// True when the ephemeral bit (bit 63) is set.
    #[must_use]
    pub const fn is_ephemeral(self) -> bool {
        self.0 & 0x8000_0000_0000_0000 != 0
    }

    /// The ordering type: `(u64)(high32 & 0x1F000000) << 32`.
    ///
    /// Note the five-bit mask: it excludes bit 61 (`0x20`), the login/world marker, which is why
    /// that bit has no reader.
    #[must_use]
    pub const fn ordering_type(self) -> u64 {
        ((self.high32() & 0x1F00_0000) as u64) << 32
    }

    /// The sequence id: `id & 0x00FF0000_FFFFFFFF`.
    ///
    /// **The stream identity**: the low dword plus bits 48-55, i.e. everything except the ordering
    /// stamp, the ordering type and the ephemeral bit. Two blobs with the same sequence id are two
    /// versions of the same thing, and that is what supersession keys on.
    #[must_use]
    pub const fn sequence_id(self) -> u64 {
        self.0 & 0x00FF_0000_FFFF_FFFF
    }

    /// Bits 32-47.
    #[must_use]
    pub const fn ordering_stamp(self) -> u16 {
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.0 >> 32) as u16
        }
    }

    /// The initial sequence id for stream `n`: `(u64)((n << 16) & 0xFF0000) << 32`.
    #[must_use]
    pub const fn make_initial_sequence_id(n: u32) -> Self {
        Self((((n << 16) & 0x00FF_0000) as u64) << 32)
    }

    /// The next non-ephemeral sequence id: `id + 1`, then mask the high dword
    /// back to `0xFF0000`.
    ///
    /// So the counter is effectively 32-bit and the sequence byte is preserved; a carry out of the
    /// low dword is discarded rather than incrementing the sequence byte.
    #[must_use]
    pub const fn next_non_ephemeral_sequence_id(self) -> Self {
        let next = self.0.wrapping_add(1);
        Self((next & 0x0000_0000_FFFF_FFFF) | (next & 0x00FF_0000_0000_0000))
    }

    /// The newer-ordering-stamp test — a wrapping 16-bit compare of bits 32-47.
    ///
    /// ```text
    /// if (a == b) return false;
    /// diff = a - b; sign = 1;
    /// if (a < b) { diff = b - a; sign = -1; }
    /// if (diff > 0x7FFF) sign = -sign;
    /// return sign > 0;
    /// ```
    ///
    /// Transcribed literally, because the obvious one-line paraphrase
    /// `a != b && a.wrapping_sub(b) < 0x8000` is **wrong at exactly half a period**. When the two
    /// stamps differ by 0x8000 the original's answer depends on which is numerically larger:
    /// "0 is newer than 0x8000" is true and "0x8000 is newer than 0" is false, so the relation is
    /// not antisymmetric there. Both paraphrase and original agree everywhere else.
    #[must_use]
    pub fn lhs_newer_ordering_stamp(self, other: Self) -> bool {
        let a = self.ordering_stamp();
        let b = other.ordering_stamp();
        if a == b {
            return false;
        }
        let (diff, mut sign) = if a < b { (b - a, -1i32) } else { (a - b, 1i32) };
        if diff > 0x7FFF {
            sign = -sign;
        }
        sign > 0
    }

    /// Build a blob id out of its parts.
    ///
    /// ```text
    /// if (lo32(ordering_type) != 0 || (hi32(ordering_type) & 0xFF000000) != hi32(ordering_type))
    ///     ordering_type = (u64)(hi32(ordering_type) & 0xFF000000) << 32
    /// if ((hi32(sequence_id) & 0x00FF0000) != hi32(sequence_id))
    ///     hi32(sequence_id) &= 0x00FF0000
    /// return ((u64)(hi32(ordering_type) | (u32)stamp | hi32(sequence_id)) << 32) | lo32(sequence_id)
    /// ```
    ///
    /// Note the ordering-type mask here is `0xFF000000`, eight bits, not the five
    /// [`NetBlobId::ordering_type`] reads back: the ephemeral bit and bit 61 travel inside the
    /// ordering-type argument.
    #[must_use]
    pub fn make(ordering_type: u64, stamp: u16, sequence_id: Self) -> Self {
        let mut ot = ordering_type;
        #[allow(clippy::cast_possible_truncation)]
        let ot_hi = (ot >> 32) as u32;
        #[allow(clippy::cast_possible_truncation)]
        let ot_lo = ot as u32;
        if ot_lo != 0 || (ot_hi & 0xFF00_0000) != ot_hi {
            ot = u64::from(ot_hi & 0xFF00_0000) << 32;
        }
        #[allow(clippy::cast_possible_truncation)]
        let ot_hi = (ot >> 32) as u32;

        let mut seq_hi = sequence_id.high32();
        if (seq_hi & 0x00FF_0000) != seq_hi {
            seq_hi &= 0x00FF_0000;
        }

        Self((u64::from(ot_hi | u32::from(stamp) | seq_hi) << 32) | u64::from(sequence_id.low32()))
    }
}

/// Reassembly state for one network blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobState {
    Frozen,
    Sending,
    Fragmented,
    Receiving,
    Received,
}

/// A bound this crate imposes and the original does not.
///
/// The fragment count is a `u16` from the wire, so count × 448 is up to **29 MB from a single
/// packet**. The original allocates that eagerly on the first fragment stored. This crate refuses a
/// blob whose declared size exceeds this and grows its buffer lazily besides, so a hostile
/// fragment count costs only what it actually sends. The bound is a deliberate deviation.
pub const MAX_REASSEMBLY_BYTES: usize = 8 * 1024 * 1024;

/// `NetBlob`.
#[derive(Debug, Clone)]
pub struct NetBlob {
    /// The hash key while reassembling: the stream key for an ephemeral blob, the whole id
    /// otherwise. The send-to-queue step restores it to `saved_net_blob_id` on
    /// completion.
    pub id: NetBlobId,
    /// The *original* id, kept while `id` is temporarily the ephemeral stream key.
    pub saved_net_blob_id: NetBlobId,
    pub state: BlobState,
    /// The whole message, opcode first.
    pub buf: Vec<u8>,
    /// The message length. On receive this is the value the fragment headers imply,
    /// which over-states the truth by up to 447 bytes until the final fragment arrives.
    pub buf_size: usize,
    /// The number of fragments the blob is made of.
    pub c_max_fragments: u32,
    /// Fragments stored so far.
    pub num_fragments: u32,
    /// Recipient id the blob arrived from.
    pub sender: u16,
    /// Ordering queue, 1-11.
    pub queue_id: u16,
    /// Send priority. **Lower is sent first.**
    pub priority: u32,
    /// Which fragment indices have been stored.
    ///
    /// The original keeps no such record, which is why a retransmitted fragment increments its
    /// received-fragment count again and can complete a blob early **with a hole**. This is the
    /// de-duplication, and it is a deliberate deviation rather than an accident.
    seen: Vec<bool>,
}

impl NetBlob {
    /// The send-side constructor: the buffer, its size and the queue id.
    #[must_use]
    pub fn for_send(buf: Vec<u8>, queue_id: u16) -> Self {
        let buf_size = buf.len();
        let c_max_fragments = u32::try_from(buf_size.div_ceil(MAX_FRAG_DATA)).unwrap_or(u32::MAX);
        Self {
            id: NetBlobId::default(),
            saved_net_blob_id: NetBlobId::default(),
            state: BlobState::Frozen,
            buf,
            buf_size,
            c_max_fragments,
            num_fragments: 0,
            sender: 0,
            queue_id,
            priority: 0,
            seen: Vec::new(),
        }
    }

    /// The receive-side constructor. Empty.
    #[must_use]
    pub fn for_recv(sender: u16) -> Self {
        Self {
            id: NetBlobId::default(),
            saved_net_blob_id: NetBlobId::default(),
            state: BlobState::Receiving,
            buf: Vec::new(),
            buf_size: 0,
            c_max_fragments: 0,
            num_fragments: 0,
            sender,
            queue_id: 0,
            priority: 0,
            seen: Vec::new(),
        }
    }

    /// Split a blob into fragments.
    ///
    /// `num_frags = ceil(buf_size / 448)`, fragments in index order, each `min(448, remaining)`
    /// bytes. A zero-length blob produces no fragments, exactly as `ceil(0 / 448) = 0` says.
    #[must_use]
    pub fn fragmentize(&self) -> Vec<Fragment> {
        let num_frags = u16::try_from(self.c_max_fragments).unwrap_or(u16::MAX);
        self.buf
            .chunks(MAX_FRAG_DATA)
            .enumerate()
            .map(|(i, chunk)| {
                Fragment::new(
                    FragmentHeader {
                        blob_id_low: self.id.low32(),
                        blob_id_high: self.id.high32(),
                        num_frags,
                        blob_frag_size: 0, // filled in by Fragment::new
                        blob_num: u16::try_from(i).unwrap_or(u16::MAX),
                        queue_id: self.queue_id,
                    },
                    chunk.to_vec(),
                )
            })
            .collect()
    }

    /// Store one arriving fragment.
    ///
    /// Returns `true` when the fragment was stored. A fragment failing any test is dropped
    /// **silently and without incrementing `num_fragments`**, which is why a corrupted blob simply
    /// never completes rather than erroring.
    ///
    /// The two buffer-sizing rules:
    ///
    /// - On the **first** fragment stored,
    ///   `buf_size = (blob_num + 1 == num_frags) ? (num_frags - 1) * 448 - 16 + blob_frag_size
    ///                                         : num_frags * 448`.
    ///   Arriving out of order — a non-final fragment first — over-allocates by up to 447 bytes.
    /// - When the **final** fragment arrives and `c_max_fragments > 1`, `buf_size` is corrected to
    ///   `(c_max_fragments - 1) * 448 + payload_size`.
    pub fn receive_add_fragment(&mut self, frag: &Fragment) -> bool {
        let h = frag.header;
        let payload_size = frag.payload.len();
        if payload_size > MAX_FRAG_DATA {
            return false;
        }

        if self.c_max_fragments == 0 {
            // First fragment stored for this blob.
            if h.num_frags == 0 {
                return false;
            }
            let num_frags = usize::from(h.num_frags);
            let buf_size = if usize::from(h.blob_num) + 1 == num_frags {
                (num_frags - 1) * MAX_FRAG_DATA + payload_size
            } else {
                num_frags * MAX_FRAG_DATA
            };
            // The bound the original lacks. See MAX_REASSEMBLY_BYTES.
            if buf_size > MAX_REASSEMBLY_BYTES {
                return false;
            }
            self.buf_size = buf_size;
            self.c_max_fragments = u32::from(h.num_frags);
            self.queue_id = h.queue_id;
            self.seen = vec![false; num_frags];
        } else {
            if u32::from(h.num_frags) != self.c_max_fragments || h.queue_id != self.queue_id {
                return false;
            }
            if u32::from(h.blob_num) >= self.c_max_fragments {
                return false;
            }
        }

        if u32::from(h.blob_num) >= self.c_max_fragments {
            return false;
        }
        let offset = usize::from(h.blob_num) * MAX_FRAG_DATA;
        let end = offset + payload_size;
        if end > self.buf_size {
            return false;
        }

        // The deliberate deviation: the original would store this again and count it again.
        if self
            .seen
            .get(usize::from(h.blob_num))
            .copied()
            .unwrap_or(false)
        {
            return false;
        }

        // The buffer is grown lazily rather than allocated at `num_frags * 448`, so a hostile
        // `num_frags` costs only what it actually sends. `buf_size` still carries the documented
        // value, so every bounds test above behaves as the original's does.
        if self.buf.len() < end {
            self.buf.resize(end, 0);
        }
        self.buf[offset..end].copy_from_slice(&frag.payload);
        self.seen[usize::from(h.blob_num)] = true;
        self.num_fragments += 1;

        // The final fragment corrects the over-allocation.
        if u32::from(h.blob_num) + 1 == self.c_max_fragments && self.c_max_fragments > 1 {
            self.buf_size = (self.c_max_fragments as usize - 1) * MAX_FRAG_DATA + payload_size;
        }
        true
    }

    /// `num_fragments == c_max_fragments`.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.c_max_fragments != 0 && self.num_fragments == self.c_max_fragments
    }

    /// The reassembled message, trimmed to `buf_size`.
    #[must_use]
    pub fn take_payload(&mut self) -> Vec<u8> {
        self.buf.resize(self.buf_size, 0);
        std::mem::take(&mut self.buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/04-netblobs-and-queues.md` §2, the helper table defining
    /// blob-id packing and sequence-arithmetic contracts.
    #[test]
    fn net_blob_id_accessors() {
        // ephemeral, ordering type 0, stamp 0, sequence 0x2A -- what ACE sends
        // (MessageFragment.CreateServerFragment: Id = 0x80000000, Sequence = a counter).
        let ace = NetBlobId(0x8000_0000_0000_002A);
        assert!(ace.is_ephemeral());
        assert_eq!(ace.ordering_type(), 0);
        assert_eq!(ace.ordering_stamp(), 0);
        assert_eq!(ace.sequence_id(), 0x2A);

        // A client-made world-bound blob: ordering type 0x03, stamp 7, sequence 5.
        let world = NetBlobId(0x0300_0007_0000_0005);
        assert!(!world.is_ephemeral());
        assert_eq!(world.ordering_type(), 0x0300_0000u64 << 32);
        assert_eq!(world.ordering_stamp(), 7);
        assert_eq!(world.sequence_id(), 5);

        // Login-bound: 0x23. `ordering_type` masks five bits, so it reads back as 0x03 -- that is
        // exactly why bit 61 has no reader (compatibility note #38).
        let login = NetBlobId(0x2300_0007_0000_0005);
        assert_eq!(login.ordering_type(), 0x0300_0000u64 << 32);
        assert_eq!(login.sequence_id(), 5);
    }

    /// The stamp comparison wraps: 0 is newer than 0xFFFF.
    #[test]
    fn ordering_stamp_comparison_wraps() {
        let at = |s: u16| NetBlobId(u64::from(s) << 32);
        assert!(at(1).lhs_newer_ordering_stamp(at(0)));
        assert!(!at(0).lhs_newer_ordering_stamp(at(1)));
        assert!(at(0).lhs_newer_ordering_stamp(at(0xFFFF)));
        assert!(!at(0xFFFF).lhs_newer_ordering_stamp(at(0)));
        // Equal stamps are not newer in either direction.
        assert!(!at(5).lhs_newer_ordering_stamp(at(5)));
        assert!(at(0x7FFF).lhs_newer_ordering_stamp(at(0)));
    }

    /// The half-period case, transcribed rather than
    /// paraphrased: at a difference of exactly 0x8000 the relation is **not** antisymmetric,
    /// because the sign is decided by the numeric comparison before the 0x7FFF test flips it.
    ///
    /// The natural paraphrase `a != b && a.wrapping_sub(b) < 0x8000` gets `LHSNewer(0, 0x8000)`
    /// wrong, which is why this test exists.
    #[test]
    fn ordering_stamp_comparison_is_asymmetric_at_exactly_half_a_period() {
        let at = |s: u16| NetBlobId(u64::from(s) << 32);
        assert!(at(0).lhs_newer_ordering_stamp(at(0x8000)));
        assert!(!at(0x8000).lhs_newer_ordering_stamp(at(0)));
        assert!(at(1).lhs_newer_ordering_stamp(at(0x8001)));
        assert!(!at(0x8001).lhs_newer_ordering_stamp(at(1)));
    }

    /// `make_initial_sequence_id(0) = 0`, and the counter runs 0, 1, 2, ... in the low dword with bits
    /// 48-55 permanently 0, matching the client's counter.
    #[test]
    fn non_ephemeral_sequence_counter() {
        let mut id = NetBlobId::make_initial_sequence_id(0);
        assert_eq!(id.0, 0);
        for expected in 1..=5u64 {
            id = id.next_non_ephemeral_sequence_id();
            assert_eq!(id.0, expected);
        }
        // The sequence byte survives, and a carry out of the low dword does not reach it.
        let with_byte = NetBlobId::make_initial_sequence_id(3);
        assert_eq!(with_byte.0, 0x0003_0000_0000_0000);
        let wrapped = NetBlobId(with_byte.0 | 0xFFFF_FFFF).next_non_ephemeral_sequence_id();
        assert_eq!(wrapped.0, 0x0003_0000_0000_0000);
    }

    /// Building a blob id normalises both of its inputs before combining them.
    #[test]
    fn make_net_blob_id_normalises() {
        // The blob-id constructor's two constants, in the form it passes them.
        let world = NetBlobId::make(
            u64::from(NetBlobId::ORDERING_TYPE_WORLD) << 32,
            0x1234,
            NetBlobId(7),
        );
        assert_eq!(world.0, 0x0300_1234_0000_0007);

        let login = NetBlobId::make(
            u64::from(NetBlobId::ORDERING_TYPE_LOGIN) << 32,
            0x1234,
            NetBlobId(7),
        );
        assert_eq!(login.0, 0x2300_1234_0000_0007);

        // Junk in the low dword of the ordering type is discarded.
        let dirty = NetBlobId::make(
            (u64::from(NetBlobId::ORDERING_TYPE_WORLD) << 32) | 0xDEAD,
            0,
            NetBlobId(1),
        );
        assert_eq!(dirty.0, 0x0300_0000_0000_0001);

        // Junk in the sequence id's high dword outside bits 48-55 is masked away.
        let dirty_seq = NetBlobId::make(0, 0, NetBlobId(0xFFFF_FFFF_0000_0009));
        assert_eq!(dirty_seq.0, 0x00FF_0000_0000_0009);
    }

    /// `Fragmentize` produces `ceil(size / 448)` fragments in index order.
    #[test]
    fn fragmentize_produces_ceil_size_over_448_fragments_in_order() {
        for (size, expected) in [
            (0usize, 0usize),
            (1, 1),
            (448, 1),
            (449, 2),
            (896, 2),
            (897, 3),
        ] {
            let blob = NetBlob::for_send(vec![0xAB; size], 9);
            let frags = blob.fragmentize();
            assert_eq!(frags.len(), expected, "size {size}");
            for (i, f) in frags.iter().enumerate() {
                assert_eq!(usize::from(f.header.blob_num), i);
                assert_eq!(usize::from(f.header.num_frags), expected);
                assert_eq!(f.header.queue_id, 9);
                assert!(f.payload.len() <= MAX_FRAG_DATA);
            }
            let total: usize = frags.iter().map(|f| f.payload.len()).sum();
            assert_eq!(total, size);
        }
    }

    fn frag(id: NetBlobId, num_frags: u16, blob_num: u16, payload: Vec<u8>) -> Fragment {
        Fragment::new(
            FragmentHeader {
                blob_id_low: id.low32(),
                blob_id_high: id.high32(),
                num_frags,
                blob_frag_size: 0,
                blob_num,
                queue_id: 9,
            },
            payload,
        )
    }

    /// The two buffer-sizing rules: an out-of-order first fragment over-allocates by up to 447
    /// bytes, and the final fragment corrects it. `docs/networking/01-packet-format.md` §4.4.
    #[test]
    fn buffer_sizing_over_allocates_then_corrects() {
        let id = NetBlobId(1);

        // In order: the first fragment is not final, so buf_size = 3 * 448.
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 3, 0, vec![1; 448])));
        assert_eq!(b.buf_size, 3 * 448);
        assert!(b.receive_add_fragment(&frag(id, 3, 1, vec![2; 448])));
        assert_eq!(b.buf_size, 3 * 448);
        assert!(b.receive_add_fragment(&frag(id, 3, 2, vec![3; 100])));
        assert_eq!(b.buf_size, 2 * 448 + 100, "the final fragment corrects it");
        assert!(b.is_complete());
        assert_eq!(b.take_payload().len(), 2 * 448 + 100);

        // Final fragment first: buf_size is right immediately, and stays right.
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 3, 2, vec![3; 100])));
        assert_eq!(b.buf_size, 2 * 448 + 100);
        assert!(b.receive_add_fragment(&frag(id, 3, 0, vec![1; 448])));
        assert_eq!(b.buf_size, 2 * 448 + 100);
        assert!(b.receive_add_fragment(&frag(id, 3, 1, vec![2; 448])));
        assert!(b.is_complete());

        // Single-fragment blob: `blob_num + 1 == num_frags` on the first fragment, and the
        // `c_max_fragments > 1` guard means the correction rule does not fire.
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 1, 0, vec![7; 12])));
        assert_eq!(b.buf_size, 12);
        assert!(b.is_complete());
    }

    /// A fragment that fails any test is dropped silently and does not increment `num_fragments`,
    /// so the blob simply never completes.
    #[test]
    fn mismatched_fragments_are_dropped_without_counting() {
        let id = NetBlobId(1);
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 3, 0, vec![1; 448])));

        // num_frags disagrees.
        assert!(!b.receive_add_fragment(&frag(id, 4, 1, vec![2; 448])));
        // queue_id disagrees.
        let mut wrong_queue = frag(id, 3, 1, vec![2; 448]);
        wrong_queue.header.queue_id = 5;
        assert!(!b.receive_add_fragment(&wrong_queue));
        // blob_num out of range.
        assert!(!b.receive_add_fragment(&frag(id, 3, 3, vec![2; 448])));
        // Payload over 448.
        assert!(!b.receive_add_fragment(&frag(id, 3, 1, vec![2; 449])));

        assert_eq!(b.num_fragments, 1);
        assert!(!b.is_complete());

        // The original's fifth test -- "the destination range fits in the buffer" -- turns out to
        // be unreachable once the four above have passed: `blob_num < c_max_fragments` and
        // `payload <= 448` together bound `blob_num * 448 + payload` by `num_frags * 448`, which is
        // the largest `buf_size` can ever be. It is implemented anyway, because it is what the
        // original does and because it is the guard that would catch a future sizing change.
        assert!(b.receive_add_fragment(&frag(id, 3, 2, vec![2; 448])));
        assert_eq!(b.num_fragments, 2);
    }

    /// The deliberate deviation from retail (`docs/networking/01-packet-format.md` §4.4): a
    /// duplicate fragment index is not counted twice, so a blob cannot complete early with a hole.
    /// The original does count it.
    #[test]
    fn duplicate_fragment_indices_are_de_duplicated() {
        let id = NetBlobId(1);
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 2, 0, vec![1; 448])));
        assert!(!b.receive_add_fragment(&frag(id, 2, 0, vec![1; 448])));
        assert_eq!(b.num_fragments, 1);
        assert!(
            !b.is_complete(),
            "the original would have completed here, with a hole"
        );
        assert!(b.receive_add_fragment(&frag(id, 2, 1, vec![2; 8])));
        assert!(b.is_complete());
    }

    /// `num_frags = 0xFFFF` is a 29 MB allocation from one packet in the original. Here it costs
    /// what it sends, and a declared size beyond the cap is refused outright.
    #[test]
    fn reassembly_allocation_is_bounded() {
        let id = NetBlobId(1);
        let mut b = NetBlob::for_recv(1);
        // 0xFFFF * 448 = 29,360,127 bytes declared -- above MAX_REASSEMBLY_BYTES, refused.
        assert!(!b.receive_add_fragment(&frag(id, 0xFFFF, 0, vec![0; 448])));
        assert_eq!(b.c_max_fragments, 0);
        assert!(b.buf.is_empty());

        // A large but permitted blob allocates only what has actually arrived.
        let mut b = NetBlob::for_recv(1);
        assert!(b.receive_add_fragment(&frag(id, 1000, 0, vec![0; 448])));
        assert_eq!(b.buf_size, 1000 * 448);
        assert_eq!(b.buf.len(), 448, "lazily grown, not eagerly allocated");
    }

    /// `num_frags = 0` cannot start a blob.
    #[test]
    fn zero_fragment_blob_is_refused() {
        let mut b = NetBlob::for_recv(1);
        assert!(!b.receive_add_fragment(&frag(NetBlobId(1), 0, 0, vec![0; 4])));
    }
}
