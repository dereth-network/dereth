//! Packet assembly on send, and splitting and parsing on receive.
//!
//! See `docs/networking/01-packet-format.md` §§1, 4.3, 5.
//!
//! A packet is modelled as `header | BTreeMap<mask, bytes> | Vec<Fragment>`: the map makes the
//! ascending-mask wire order fall out for free and makes it impossible to emit two sections with
//! the same mask.

use std::collections::BTreeMap;

use super::optional::{flags as mflags, section_len, spec_for};
use super::{
    Fragment, PacketFlags, ProtoHeader, WireError, FRAG_HEADER_SIZE, HEADER_SIZE,
    MAX_FRAGS_PER_PACKET, MAX_OPTIONAL_HEADERS,
};

/// A purely local field, never on the wire.
///
/// See `docs/networking/01-packet-format.md` §5.3.
pub mod netpacket_flags {
    /// The checksum has been computed and, if needed, encrypted. Prevents double-encryption.
    pub const CHECKSUM_DONE: u8 = 0x1;
    /// Some optional header is time-sensitive (flag `0x08`), so the payload must be refreshed
    /// and the checksum recomputed before each send.
    pub const TIME_SENSITIVE: u8 = 0x2;
    /// A fragment was added, **or** an optional header with flag `0x01` clear was added: the
    /// packet must be sequenced, encrypted and cached for retransmit. This bit *is* the
    /// encrypted-iff rule.
    pub const NEEDS_ENCRYPTION: u8 = 0x4;
    /// Some optional header has flag `0x20`, so the packet bypasses the wire-room test. Inert in
    /// the retail client, but carried faithfully.
    pub const PRIORITY: u8 = 0x8;
}

/// A packet being built for transmission.
#[derive(Debug, Clone, Default)]
pub struct OutPacket {
    /// `seq_id`, `rec_id`, `interval` and `iteration` are the caller's; `header`, `datalen` and
    /// `checksum` are filled in by [`OutPacket::serialize`].
    pub header: ProtoHeader,
    /// Optional-header sections keyed by mask. A `BTreeMap` so wire order is the map's order.
    pub optional: BTreeMap<u32, Vec<u8>>,
    pub fragments: Vec<Fragment>,
    /// Packet flag mask.
    pub flags: u8,
    /// The ISAAC value this packet's checksum was XORed with.
    ///
    /// Kept so a retransmit reuses it **verbatim** (verified against retail).
    /// Drawing a fresh key on retransmit desynchronises the stream permanently.
    pub crypto_key: Option<u32>,
}

impl OutPacket {
    #[must_use]
    pub fn new(header: ProtoHeader) -> Self {
        Self {
            header,
            ..Default::default()
        }
    }

    ///
    /// The original inserts into its optional-header list with an insertion sort on the mask; the
    /// `BTreeMap` does the same job. Sets the local packet flag bits the header's flags imply —
    /// in particular `NEEDS_ENCRYPTION` when the header is **not** disposable, which is one half of
    /// the encrypted-iff rule.
    ///
    /// # Errors
    /// [`WireError::TooManySections`] past the 32-section limit
    /// (the 32-entry optional-header list).
    pub fn add_optional_header(&mut self, mask: u32, data: Vec<u8>) -> Result<(), WireError> {
        if self.optional.len() >= MAX_OPTIONAL_HEADERS && !self.optional.contains_key(&mask) {
            return Err(WireError::TooManySections);
        }
        let flags = spec_for(mask).map_or(0, |s| s.flags);
        if flags & mflags::DISPOSABLE == 0 {
            self.flags |= netpacket_flags::NEEDS_ENCRYPTION;
        }
        if flags & mflags::TIME_SENSITIVE != 0 {
            self.flags |= netpacket_flags::TIME_SENSITIVE;
        }
        if flags & mflags::PRIORITY != 0 {
            self.flags |= netpacket_flags::PRIORITY;
        }
        self.flags &= !netpacket_flags::CHECKSUM_DONE;
        self.optional.insert(mask, data);
        Ok(())
    }

    /// Add a fragment. A fragment always makes the packet need encryption.
    ///
    /// # Errors
    /// a too-many-sections error past the 29-fragment limit.
    pub fn add_fragment(&mut self, frag: Fragment) -> Result<(), WireError> {
        if self.fragments.len() >= MAX_FRAGS_PER_PACKET {
            return Err(WireError::TooManySections);
        }
        self.flags |= netpacket_flags::NEEDS_ENCRYPTION;
        self.flags &= !netpacket_flags::CHECKSUM_DONE;
        self.fragments.push(frag);
        Ok(())
    }

    /// Run when the packet is stored for
    /// possible retransmission.
    ///
    /// Without this a resent packet carries a stale ACK or NAK.
    pub fn remove_disposable_optional_headers(&mut self) {
        self.optional
            .retain(|mask, _| !super::optional::is_disposable(*mask));
        self.flags &= !netpacket_flags::CHECKSUM_DONE;
    }

    /// Does this packet need to be sequenced, encrypted and cached? `flags & 0x4`.
    #[must_use]
    pub const fn needs_encryption(&self) -> bool {
        self.flags & netpacket_flags::NEEDS_ENCRYPTION != 0
    }

    /// The `header` value this packet will carry.
    ///
    /// `(num_frags ? 0x4 : 0) | OR of every optional mask | (encrypted ? 0x2 : 0)`, plus whatever
    /// the caller has already put in `header.header` — which is how `transmit_acks` sets
    /// `Retransmission`.
    #[must_use]
    pub fn computed_flags(&self) -> PacketFlags {
        let mut bits = self.header.header.0 & PacketFlags::RETRANSMISSION;
        if !self.fragments.is_empty() {
            bits |= PacketFlags::BLOB_FRAGMENTS;
        }
        for mask in self.optional.keys() {
            bits |= mask;
        }
        if self.needs_encryption() {
            bits |= PacketFlags::ENCRYPTED_CHECKSUM;
        }
        PacketFlags(bits)
    }

    /// Bytes after the 20-byte header.
    #[must_use]
    pub fn payload_len(&self) -> usize {
        self.optional.values().map(Vec::len).sum::<usize>()
            + self.fragments.iter().map(Fragment::wire_len).sum::<usize>()
    }

    /// The payload half of the checksum, summed per section in wire order.
    #[must_use]
    pub fn payload_hash(&self) -> u32 {
        let mut total = 0u32;
        for block in self.optional.values() {
            total = total.wrapping_add(crate::crc::hash32(block));
        }
        for frag in &self.fragments {
            total = total.wrapping_add(frag.hash());
        }
        total
    }

    /// Serialise to the exact bytes `sendto` writes.
    ///
    /// `key` is the ISAAC draw for this packet. It **must** be `Some` when
    /// [`OutPacket::needs_encryption`] and `None` otherwise: the receiver checks that equivalence
    /// bit for bit and drops the packet with no error on either side.
    ///
    /// # Errors
    /// [`WireError::EncryptionEquivalence`] when `key` disagrees with the packet's own flags,
    /// [`WireError::EncryptedWithZeroSequence`] for an encrypted packet with `seq_id = 0`.
    pub fn serialize(&mut self, key: Option<u32>) -> Result<Vec<u8>, WireError> {
        if key.is_some() != self.needs_encryption() {
            return Err(WireError::EncryptionEquivalence {
                encrypted: key.is_some(),
                needs_encryption: self.needs_encryption(),
            });
        }
        if key.is_some() && self.header.seq_id == 0 {
            return Err(WireError::EncryptedWithZeroSequence);
        }

        self.header.header = self.computed_flags();
        self.header.datalen = u16::try_from(self.payload_len()).unwrap_or(u16::MAX);
        self.header.checksum = 0;
        let header_hash = self.header.header_hash();
        self.header.checksum = crate::crc::wire_checksum(header_hash, self.payload_hash(), key);
        self.crypto_key = key;
        self.flags |= netpacket_flags::CHECKSUM_DONE;

        let mut out = Vec::with_capacity(HEADER_SIZE + self.payload_len());
        out.extend_from_slice(&self.header.to_bytes());
        for block in self.optional.values() {
            out.extend_from_slice(block);
        }
        for frag in &self.fragments {
            out.extend_from_slice(&frag.header.to_bytes());
            out.extend_from_slice(&frag.payload);
        }
        Ok(out)
    }
}

/// A datagram taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPacket {
    pub header: ProtoHeader,
    /// Sections keyed by mask, so they are in wire order by construction.
    pub optional: BTreeMap<u32, Vec<u8>>,
    pub fragments: Vec<Fragment>,
    /// The hash of everything after the 20-byte header, summed per section. Compare it against
    /// the checksum minus the header hash (XORed with the ISAAC key if encrypted) to verify the
    /// packet.
    pub payload_hash: u32,
    /// The header hash, kept so the session layer can recover the ISAAC key without re-hashing.
    pub header_hash: u32,
    /// True when the packet carries a section with behavior flag `0x04`, so it is legal before
    /// its recipient connection has been established.
    pub pre_connection_ok: bool,
}

impl ParsedPacket {
    /// Parse a whole datagram.
    ///
    /// Rejection order follows the original: header sanity
    /// ([`ProtoHeader::verify`]) first, then the declared length against the real one, then the
    /// sections in ascending mask order, then the fragments, then the exact-consumption rule, then
    /// the structural invariants (exclusivity, the encrypted-iff equivalence, `seq_id != 0` on an
    /// encrypted packet).
    ///
    /// What is deliberately **not** checked here: the source address (the session layer owns the
    /// peer), the connection state, and the checksum itself (which needs the ISAAC key, and
    /// therefore the sequence bookkeeping, both of which live in `session.rs`).
    ///
    /// # Errors
    /// Any [`WireError`]. The client logs none of these; a rejected packet vanishes silently.
    pub fn parse(buf: &[u8]) -> Result<Self, WireError> {
        Self::parse_with(buf, ProtoHeader::verify)
    }

    /// [`Self::parse`] as a server receives a client's datagram: the header is checked with
    /// [`ProtoHeader::verify_from_client`], so a `rec_id` of 256 or more (the connection id the
    /// server assigned) is accepted. Every other rule is the same.
    ///
    /// # Errors
    /// Any [`WireError`] but [`WireError::RecIdOutOfRange`].
    pub fn parse_from_client(buf: &[u8]) -> Result<Self, WireError> {
        Self::parse_with(buf, ProtoHeader::verify_from_client)
    }

    fn parse_with(
        buf: &[u8],
        verify: fn(ProtoHeader) -> Result<(), WireError>,
    ) -> Result<Self, WireError> {
        let header = ProtoHeader::from_bytes(buf)?;
        verify(header)?;

        let body = &buf[HEADER_SIZE..];
        if usize::from(header.datalen) != body.len() {
            return Err(WireError::DatalenMismatch {
                declared: header.datalen,
                actual: body.len(),
            });
        }

        let mut optional = BTreeMap::new();
        let mut payload_hash = 0u32;
        let mut needs_encryption = false;
        let mut pre_connection_ok = false;
        let mut exclusive_mask = None;
        let mut offset = 0usize;

        // The parser walks the mask-sorted factory table from index 0
        // upward, so sections are consumed in ascending mask order. `optional_bits` yields the same
        // order, which is why a section written out of order fails: its bytes are read as some
        // other section's.
        for mask in header.header.optional_bits() {
            let rest = &body[offset..];
            let len = section_len(mask, rest)?;
            let block = &rest[..len];
            payload_hash = payload_hash.wrapping_add(crate::crc::hash32(block));
            offset += len;

            let flags = spec_for(mask).map_or(0, |s| s.flags);
            if flags & mflags::DISPOSABLE == 0 {
                needs_encryption = true;
            }
            if flags & mflags::PRE_CONNECTION != 0 {
                pre_connection_ok = true;
            }
            if flags & mflags::EXCLUSIVE != 0 {
                exclusive_mask = Some(mask);
            }
            optional.insert(mask, block.to_vec());
        }

        let mut fragments = Vec::new();
        if header.header.has_fragments() {
            // "Loop while at least 17 bytes remain (buffer size - offset >= 0x11)". Transcribed
            // literally: with exactly 16 bytes left the loop does not run, and the exact-consumption
            // check below then rejects the packet. So a trailing zero-payload fragment is not
            // representable on the wire, which is faithful and slightly surprising.
            while body.len().saturating_sub(offset) > FRAG_HEADER_SIZE {
                let (frag, used) = Fragment::from_bytes(&body[offset..])?;
                payload_hash = payload_hash.wrapping_add(frag.hash());
                offset += used;
                fragments.push(frag);
            }
            needs_encryption = true;
        }

        // "After the loop the iterator must be exactly at the end of the payload."
        if offset != body.len() {
            return Err(WireError::TrailingBytes(body.len() - offset));
        }

        // Flag `0x02`: an exclusive header must be the only thing in the packet.
        if let Some(mask) = exclusive_mask {
            if optional.len() > 1 || !fragments.is_empty() {
                return Err(WireError::ExclusiveHeaderNotAlone(mask));
            }
        }

        // `((header >> 1) & 1) == ((flags >> 2) & 1)` -- the encrypted-iff rule. A peer that gets
        // this wrong is dropped, with no error message on either side.
        let encrypted = header.header.is_encrypted();
        if encrypted != needs_encryption {
            return Err(WireError::EncryptionEquivalence {
                encrypted,
                needs_encryption,
            });
        }
        if encrypted && header.seq_id == 0 {
            return Err(WireError::EncryptedWithZeroSequence);
        }

        Ok(Self {
            header_hash: header.header_hash(),
            header,
            optional,
            fragments,
            payload_hash,
            pre_connection_ok,
        })
    }

    /// The ISAAC value this packet's checksum was XORed with, given that it is encrypted.
    ///
    /// The session layer compares this against the key it drew (or parked) for `seq_id`. It does
    /// **not** scan the stream for a match: see
    /// `docs/CORRECTIONS.md`.
    #[must_use]
    pub fn recovered_key(&self) -> u32 {
        crate::crc::recover_isaac_key(self.header.checksum, self.header_hash, self.payload_hash)
    }

    /// Is the packet's checksum right, given the ISAAC key (or `None` for a plaintext packet)?
    #[must_use]
    pub fn checksum_ok(&self, key: Option<u32>) -> bool {
        crate::crc::wire_checksum(self.header_hash, self.payload_hash, key) == self.header.checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::FragmentHeader;

    fn worked_datagram() -> Vec<u8> {
        // docs/networking/01-packet-format.md §6, byte for byte.
        let hex = "01000000060000002e4bd6490b0000011c000100\
                   01000000000000800100 1c00 00000900 b0f70000 01000050 02000000";
        hex.split_whitespace()
            .collect::<String>()
            .as_bytes()
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).expect("ascii"), 16).expect("hex"))
            .collect()
    }

    /// The client's parser rejects a recipient id (`rec_id`) of 256 or more; the server's, reading
    /// a client's datagram, accepts it and otherwise reads the same packet.
    #[test]
    fn only_the_client_parser_limits_rec_id() {
        let mut dg = worked_datagram();
        dg[12..14].copy_from_slice(&300u16.to_le_bytes());
        assert_eq!(
            ParsedPacket::parse(&dg),
            Err(WireError::RecIdOutOfRange(300))
        );
        let p = ParsedPacket::parse_from_client(&dg).expect("the server accepts id 300");
        assert_eq!(p.header.rec_id, 300);
        let mut original = ParsedPacket::parse(&worked_datagram()).expect("parses");
        original.header.rec_id = 300;
        assert_eq!(p.fragments, original.fragments);
        assert_eq!(p.payload_hash, original.payload_hash);
    }

    /// Oracle: the worked example in `docs/networking/01-packet-format.md` §6, printed by
    /// the independent packet-format calculation. This one assertion exercises the header
    /// layout, the fragment layout, both halves of the checksum and the first ISAAC draw at once.
    #[test]
    fn worked_example_parses_and_verifies() {
        let dg = worked_datagram();
        assert_eq!(dg.len(), 48);
        let p = ParsedPacket::parse(&dg).expect("the worked example must parse");
        assert_eq!(p.header.seq_id, 1);
        assert!(p.header.header.is_encrypted());
        assert!(p.header.header.has_fragments());
        assert!(p.optional.is_empty());
        assert_eq!(p.fragments.len(), 1);
        assert_eq!(p.header_hash, 0xBBF2_710B);
        assert_eq!(p.payload_hash, 0xD041_F7B5);
        assert_eq!(p.recovered_key(), 0x5DA2_2D96);
        assert!(p.checksum_ok(Some(0x5DA2_2D96)));
        assert!(!p.checksum_ok(Some(0x5DA2_2D97)));
    }

    /// The same bytes come back out. Serialising is the parse's inverse.
    #[test]
    fn worked_example_round_trips_through_outpacket() {
        let dg = worked_datagram();
        let p = ParsedPacket::parse(&dg).expect("parse");
        let mut out = OutPacket::new(ProtoHeader {
            seq_id: 1,
            rec_id: 0x000B,
            interval: 0x0100,
            iteration: 1,
            ..Default::default()
        });
        out.add_fragment(p.fragments[0].clone()).expect("one frag");
        assert!(out.needs_encryption());
        let bytes = out.serialize(Some(0x5DA2_2D96)).expect("serialize");
        assert_eq!(bytes, dg);
    }

    /// The encrypted-iff rule, both directions (`docs/networking/01-packet-format.md` §5.3). A
    /// packet that carries a fragment but clears `EncryptedChecksum` is rejected, and so is one
    /// that sets it while carrying only disposable sections.
    #[test]
    fn encrypted_iff_fragments_or_non_disposable_header() {
        // Fragment present, EncryptedChecksum cleared -> reject.
        let mut dg = worked_datagram();
        dg[4] = 0x04; // header_ = BlobFragments only
                      // Recompute the checksum so the *only* thing wrong is the flag.
        let h = ProtoHeader::from_bytes(&dg).expect("hdr");
        let mut fixed = h;
        fixed.checksum = 0;
        let hh = fixed.header_hash();
        let pay = 0xD041_F7B5u32;
        dg[8..12].copy_from_slice(&hh.wrapping_add(pay).to_le_bytes());
        assert_eq!(
            ParsedPacket::parse(&dg),
            Err(WireError::EncryptionEquivalence {
                encrypted: false,
                needs_encryption: true
            })
        );

        // A pure ACK is disposable, so it must NOT be encrypted.
        let mut ack = OutPacket::new(ProtoHeader {
            seq_id: 0,
            ..Default::default()
        });
        ack.add_optional_header(PacketFlags::ACK_SEQUENCE, 7u32.to_le_bytes().to_vec())
            .expect("add");
        assert!(!ack.needs_encryption());
        assert!(matches!(
            ack.serialize(Some(0x1234)),
            Err(WireError::EncryptionEquivalence { .. })
        ));
        let bytes = ack.serialize(None).expect("plaintext ack");
        let parsed = ParsedPacket::parse(&bytes).expect("ack parses");
        assert!(!parsed.header.header.is_encrypted());
        assert_eq!(parsed.header.seq_id, 0);

        // TimeSync is not disposable, so a packet carrying one IS encrypted.
        let mut ts = OutPacket::new(ProtoHeader {
            seq_id: 5,
            ..Default::default()
        });
        ts.add_optional_header(PacketFlags::TIME_SYNC, 0f64.to_le_bytes().to_vec())
            .expect("add");
        assert!(ts.needs_encryption());
        let bytes = ts.serialize(Some(0xABCD_1234)).expect("encrypted timesync");
        let parsed = ParsedPacket::parse(&bytes).expect("parses");
        assert!(parsed.header.header.is_encrypted());
        assert_eq!(parsed.recovered_key(), 0xABCD_1234);
    }

    /// An encrypted packet must carry a non-zero sequence number.
    #[test]
    fn encrypted_packet_needs_a_sequence_number() {
        let mut p = OutPacket::new(ProtoHeader::default());
        p.add_optional_header(PacketFlags::TIME_SYNC, vec![0u8; 8])
            .expect("add");
        assert_eq!(
            p.serialize(Some(1)),
            Err(WireError::EncryptedWithZeroSequence)
        );
    }

    /// Sections serialise in ascending mask order regardless of the order they were added, and the
    /// per-section hash follows that order. `docs/networking/01-packet-format.md` §3.0.
    #[test]
    fn sections_serialise_in_ascending_mask_order() {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 3,
            ..Default::default()
        });
        // Added highest-mask first, on purpose.
        p.add_optional_header(PacketFlags::FLOW, vec![0xAA; 6])
            .expect("flow");
        p.add_optional_header(PacketFlags::TIME_SYNC, vec![0xBB; 8])
            .expect("timesync");
        p.add_optional_header(PacketFlags::ACK_SEQUENCE, vec![0xCC; 4])
            .expect("ack");

        let bytes = p.serialize(Some(0x11)).expect("serialize");
        let body = &bytes[HEADER_SIZE..];
        // AckSequence (0x4000) then TimeSync (0x1000000) then Flow (0x8000000).
        assert_eq!(&body[0..4], &[0xCC; 4]);
        assert_eq!(&body[4..12], &[0xBB; 8]);
        assert_eq!(&body[12..18], &[0xAA; 6]);

        let parsed = ParsedPacket::parse(&bytes).expect("parse");
        assert_eq!(parsed.optional.len(), 3);
        assert!(parsed.checksum_ok(Some(0x11)));

        // The per-section sum equals the whole-block hash here, because Flow is last.
        assert_eq!(
            parsed.payload_hash,
            crate::crc::hash32(&body[0..18]),
            "per-section and whole-block hashing agree only while Flow sorts last"
        );
    }

    /// Every fixed-length section round-trips at its documented length.
    #[test]
    fn every_optional_header_round_trips_at_its_documented_length() {
        use crate::wire::optional::{SectionLen, OPTIONAL_HEADERS};
        for spec in &OPTIONAL_HEADERS {
            let block: Vec<u8> = match spec.len {
                SectionLen::Fixed(n) => (0..n).map(|i| u8::try_from(i).unwrap_or(0)).collect(),
                SectionLen::SeqIdList => {
                    let mut v = 3u32.to_le_bytes().to_vec();
                    v.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0]);
                    v
                }
                SectionLen::LoginRequest => {
                    let mut v = crate::wire::optional::pstring_pack(b"1802");
                    v.extend_from_slice(&4u32.to_le_bytes());
                    v.extend_from_slice(&[9, 9, 9, 9]);
                    v
                }
            };
            let mut p = OutPacket::new(ProtoHeader {
                seq_id: 1,
                ..Default::default()
            });
            p.add_optional_header(spec.mask, block.clone())
                .expect("add");
            let key = if p.needs_encryption() {
                Some(0x5DA2_2D96)
            } else {
                None
            };
            let bytes = p.serialize(key).expect(spec.name);
            let parsed = ParsedPacket::parse(&bytes).expect(spec.name);
            assert_eq!(
                parsed.optional.get(&spec.mask),
                Some(&block),
                "{}",
                spec.name
            );
            assert_eq!(
                usize::from(parsed.header.datalen),
                block.len(),
                "{}",
                spec.name
            );
            assert!(parsed.checksum_ok(key), "{}", spec.name);
        }
    }

    /// Fragment parsing consumes the payload exactly; one leftover byte fails the packet.
    #[test]
    fn a_single_leftover_byte_fails_the_packet() {
        let mut dg = worked_datagram();
        dg.push(0x00);
        dg[16..18].copy_from_slice(&29u16.to_le_bytes()); // datalen_ += 1
        assert_eq!(ParsedPacket::parse(&dg), Err(WireError::TrailingBytes(1)));
    }

    /// `datalen_` must match the bytes actually present.
    #[test]
    fn datalen_must_match_the_datagram() {
        let mut dg = worked_datagram();
        dg[16..18].copy_from_slice(&27u16.to_le_bytes());
        assert_eq!(
            ParsedPacket::parse(&dg),
            Err(WireError::DatalenMismatch {
                declared: 27,
                actual: 28
            })
        );
    }

    /// Flag `0x02`: an exclusive header must be alone.
    #[test]
    fn exclusive_headers_must_be_alone() {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 0,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::DISCONNECT, vec![])
            .expect("disconnect");
        p.add_optional_header(PacketFlags::ACK_SEQUENCE, vec![0; 4])
            .expect("ack");
        let bytes = p.serialize(None).expect("serialize");
        assert_eq!(
            ParsedPacket::parse(&bytes),
            Err(WireError::ExclusiveHeaderNotAlone(PacketFlags::DISCONNECT))
        );
    }

    /// Disposable sections are stripped when the packet is cached for retransmit, so a resend never
    /// carries a stale ACK. `docs/networking/02-reliability-and-flow.md` §4.1.
    #[test]
    fn caching_strips_disposable_headers() {
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
        p.remove_disposable_optional_headers();
        assert_eq!(
            p.optional.keys().copied().collect::<Vec<_>>(),
            vec![PacketFlags::TIME_SYNC]
        );
        assert!(p.needs_encryption());
        assert_eq!(p.fragments.len(), 1);
    }

    /// A packet with an undefined bit at or above 0x100 is rejected before anything is parsed.
    #[test]
    fn undefined_high_bit_rejects_the_packet() {
        let mut dg = worked_datagram();
        dg[4..8].copy_from_slice(&(0x0000_0006u32 | 0x0080_0000).to_le_bytes());
        assert_eq!(
            ParsedPacket::parse(&dg),
            Err(WireError::UndefinedFlagBits(0x0080_0000))
        );
    }

    /// The 32-section and 29-fragment array bounds.
    #[test]
    fn section_and_fragment_limits() {
        let mut p = OutPacket::new(ProtoHeader::default());
        for i in 0..MAX_FRAGS_PER_PACKET {
            p.add_fragment(Fragment::new(
                FragmentHeader::default(),
                vec![u8::try_from(i).unwrap_or(0)],
            ))
            .expect("within the 29-fragment limit");
        }
        assert_eq!(
            p.add_fragment(Fragment::new(FragmentHeader::default(), vec![0])),
            Err(WireError::TooManySections)
        );
        // Only 19 masks exist, so the 32-section cap is unreachable in practice; assert the guard
        // is where the array bound is anyway.
        assert_eq!(MAX_OPTIONAL_HEADERS, 32);
    }
}
