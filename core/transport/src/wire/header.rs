//! `ProtoHeader` — the fixed 20-byte transport header — and the 22 defined `header` flag bits.
//!
//! See `docs/networking/01-packet-format.md` §1 and §2.

use super::{WireError, DATALEN_REJECT_AT, HEADER_SIZE};

/// The `header` flag bits.
///
/// A newtype rather than an enum: the field is a bitfield and the client's own tests are bit tests.
/// Exactly 22 bits are defined; five more (3-7) are tolerated on receipt and never set on send, and
/// the remaining five (23, 28-31) cause the packet to be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PacketFlags(pub u32);

impl PacketFlags {
    /// This packet is a resend of a sequence number already sent. Set only by
    /// The receiver does not treat it specially beyond NAK
    /// bookkeeping.
    pub const RETRANSMISSION: u32 = 0x0000_0001;
    /// The payload half of `checksum` is XORed with an ISAAC value. Set **iff** the packet carries
    /// a fragment or a non-disposable optional header; implies `seq_id != 0`.
    pub const ENCRYPTED_CHECKSUM: u32 = 0x0000_0002;
    /// One or more blob fragments follow the optional headers.
    pub const BLOB_FRAGMENTS: u32 = 0x0000_0004;

    pub const SERVER_SWITCH: u32 = 0x0000_0100;
    pub const LOGON_SERVER_ADDR: u32 = 0x0000_0200;
    pub const EMPTY_HEADER1: u32 = 0x0000_0400;
    pub const REFERRAL: u32 = 0x0000_0800;
    pub const REQUEST_RETRANSMIT: u32 = 0x0000_1000;
    pub const REJECT_RETRANSMIT: u32 = 0x0000_2000;
    pub const ACK_SEQUENCE: u32 = 0x0000_4000;
    pub const DISCONNECT: u32 = 0x0000_8000;
    pub const LOGIN_REQUEST: u32 = 0x0001_0000;
    pub const WORLD_LOGIN_REQUEST: u32 = 0x0002_0000;
    pub const CONNECT_REQUEST: u32 = 0x0004_0000;
    pub const CONNECT_RESPONSE: u32 = 0x0008_0000;
    pub const NET_ERROR: u32 = 0x0010_0000;
    pub const NET_ERROR_DISCONNECT: u32 = 0x0020_0000;
    pub const CICMD_COMMAND: u32 = 0x0040_0000;
    pub const TIME_SYNC: u32 = 0x0100_0000;
    pub const ECHO_REQUEST: u32 = 0x0200_0000;
    pub const ECHO_RESPONSE: u32 = 0x0400_0000;
    pub const FLOW: u32 = 0x0800_0000;

    /// Every bit that selects an optional header section: bits 8-22 and 24-27, nineteen in all.
    pub const OPTIONAL_MASK: u32 = 0x0F7F_FF00;

    /// The bits the optional-header walk inspects.
    ///
    /// It returns false — and packet splitting rejects the packet — when any bit in this mask is
    /// still unconsumed after the factory table has walked. That is bit 23 and bits 28-31.
    pub const REJECT_TEST_MASK: u32 = 0xFFFF_FF00;

    /// Bits 3-7. Neither consumed nor rejected: the rejection test only looks at `& 0xFFFFFF00`.
    ///
    /// Whether the retail server ever set them is unknown —
    /// the resolved compatibility notes #40. Ignore on receipt, never set on send.
    pub const TOLERATED_UNDEFINED: u32 = 0x0000_00F8;

    #[must_use]
    pub const fn contains(self, mask: u32) -> bool {
        self.0 & mask != 0
    }

    #[must_use]
    pub const fn is_encrypted(self) -> bool {
        self.contains(Self::ENCRYPTED_CHECKSUM)
    }

    #[must_use]
    pub const fn has_fragments(self) -> bool {
        self.contains(Self::BLOB_FRAGMENTS)
    }

    /// The optional-header bits present, in ascending order.
    pub fn optional_bits(self) -> impl Iterator<Item = u32> {
        let present = self.0 & Self::OPTIONAL_MASK;
        (0..32).map(|i| 1u32 << i).filter(move |m| present & m != 0)
    }

    /// The undefined bits that cause rejection, or 0 if there are none.
    ///
    /// Rejected via the factory's unconsumed-bit test.
    #[must_use]
    pub const fn undefined_reject_bits(self) -> u32 {
        self.0 & Self::REJECT_TEST_MASK & !Self::OPTIONAL_MASK
    }
}

/// The 20-byte transport header, little-endian, no padding.
///
/// Field names are the client's, minus the trailing underscore Rust would not accept idiomatically:
/// `seq_id`, `header`, `checksum`, `rec_id`, `interval`, `datalen`, `iteration`.
///
/// See `docs/networking/01-packet-format.md` §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProtoHeader {
    /// Packet sequence number for this direction of this connection. 0 means "unsequenced"
    /// (handshake, pure ACK). Starts at 1 and never returns to 0.
    pub seq_id: u32,
    /// The flag bits.
    pub header: PacketFlags,
    /// The packet checksum. Already ISAAC-XORed when `header & 0x2`.
    pub checksum: u32,
    /// The **receiver's** id for this connection, so it can index `receivers[rec_id]`. Must be
    /// < 0x100 or drops the packet.
    pub rec_id: u16,
    /// The sender's current 0.5-second interval counter. Wrapping 16-bit; flow accounting only,
    /// not a clock.
    pub interval: u16,
    /// Bytes following this header.
    pub datalen: u16,
    /// Connection generation, echoed from the peer's ConnectRequest.
    pub iteration: u16,
}

impl ProtoHeader {
    /// Decode 20 bytes. Does not validate; see [`ProtoHeader::verify`].
    ///
    /// # Errors
    /// [`WireError::TooShortForHeader`] if fewer than 20 bytes are supplied.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, WireError> {
        let b: &[u8; HEADER_SIZE] = buf
            .get(..HEADER_SIZE)
            .and_then(|s| s.try_into().ok())
            .ok_or(WireError::TooShortForHeader(buf.len()))?;
        Ok(Self {
            seq_id: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            header: PacketFlags(u32::from_le_bytes([b[4], b[5], b[6], b[7]])),
            checksum: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            rec_id: u16::from_le_bytes([b[12], b[13]]),
            interval: u16::from_le_bytes([b[14], b[15]]),
            datalen: u16::from_le_bytes([b[16], b[17]]),
            iteration: u16::from_le_bytes([b[18], b[19]]),
        })
    }

    /// Encode to 20 bytes exactly as `sendto` sees them.
    #[must_use]
    pub fn to_bytes(self) -> [u8; HEADER_SIZE] {
        let mut b = [0u8; HEADER_SIZE];
        b[0..4].copy_from_slice(&self.seq_id.to_le_bytes());
        b[4..8].copy_from_slice(&self.header.0.to_le_bytes());
        b[8..12].copy_from_slice(&self.checksum.to_le_bytes());
        b[12..14].copy_from_slice(&self.rec_id.to_le_bytes());
        b[14..16].copy_from_slice(&self.interval.to_le_bytes());
        b[16..18].copy_from_slice(&self.datalen.to_le_bytes());
        b[18..20].copy_from_slice(&self.iteration.to_le_bytes());
        b
    }

    /// The header half of the packet checksum, with `checksum` replaced by `0xBADD70DD`.
    ///
    #[must_use]
    pub fn header_hash(self) -> u32 {
        crate::crc::header_hash_bytes(&self.to_bytes())
    }

    /// The header-only checks.
    ///
    /// The order matters and is preserved: `datalen` first, then `rec_id`, then the flag bits.
    /// Note what is **not** here — `verify_header` compares the source *address* but not the source
    /// *port*, which is what makes the port + 1 asymmetry legal. The address check
    /// belongs to the session layer, which knows the peer.
    ///
    /// # Errors
    /// One of [`WireError::DatalenTooLarge`], [`WireError::RecIdOutOfRange`] or
    /// [`WireError::UndefinedFlagBits`].
    pub fn verify(self) -> Result<(), WireError> {
        self.verify_checks(true)
    }

    /// The header-only checks as a server receives a client's packet: [`Self::verify`] without the
    /// `rec_id` limit. That limit is the client's own (its receiver table has 256 slots); on a
    /// packet from a client, `rec_id` is the connection id the server assigned, and retail's
    /// servers assigned ids of 256 and more (up to 397 in the captures).
    ///
    /// # Errors
    /// [`WireError::DatalenTooLarge`] or [`WireError::UndefinedFlagBits`].
    pub fn verify_from_client(self) -> Result<(), WireError> {
        self.verify_checks(false)
    }

    fn verify_checks(self, rec_id_limit: bool) -> Result<(), WireError> {
        if self.datalen >= DATALEN_REJECT_AT {
            return Err(WireError::DatalenTooLarge(self.datalen));
        }
        if rec_id_limit && self.rec_id >= 0x100 {
            return Err(WireError::RecIdOutOfRange(self.rec_id));
        }
        let bad = self.header.undefined_reject_bits();
        if bad != 0 {
            return Err(WireError::UndefinedFlagBits(bad));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the worked example's header bytes in
    /// `docs/networking/01-packet-format.md` §6, reproduced by
    /// the independent packet-format calculation.
    #[test]
    fn worked_example_header_round_trips() {
        let bytes: [u8; 20] = [
            0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x2E, 0x4B, 0xD6, 0x49, 0x0B, 0x00,
            0x00, 0x01, 0x1C, 0x00, 0x01, 0x00,
        ];
        let h = ProtoHeader::from_bytes(&bytes).expect("20 bytes");
        assert_eq!(h.seq_id, 1);
        assert_eq!(h.header.0, 0x0000_0006);
        assert_eq!(h.checksum, 0x49D6_4B2E);
        assert_eq!(h.rec_id, 0x000B);
        assert_eq!(h.interval, 0x0100);
        assert_eq!(h.datalen, 0x001C);
        assert_eq!(h.iteration, 1);
        assert_eq!(h.to_bytes(), bytes);
        assert_eq!(h.header_hash(), 0xBBF2_710B);
    }

    /// Exactly nineteen bits select an optional header, and they are the documented ones.
    #[test]
    fn nineteen_optional_bits() {
        assert_eq!(PacketFlags::OPTIONAL_MASK.count_ones(), 19);
        for m in [
            PacketFlags::SERVER_SWITCH,
            PacketFlags::LOGON_SERVER_ADDR,
            PacketFlags::EMPTY_HEADER1,
            PacketFlags::REFERRAL,
            PacketFlags::REQUEST_RETRANSMIT,
            PacketFlags::REJECT_RETRANSMIT,
            PacketFlags::ACK_SEQUENCE,
            PacketFlags::DISCONNECT,
            PacketFlags::LOGIN_REQUEST,
            PacketFlags::WORLD_LOGIN_REQUEST,
            PacketFlags::CONNECT_REQUEST,
            PacketFlags::CONNECT_RESPONSE,
            PacketFlags::NET_ERROR,
            PacketFlags::NET_ERROR_DISCONNECT,
            PacketFlags::CICMD_COMMAND,
            PacketFlags::TIME_SYNC,
            PacketFlags::ECHO_REQUEST,
            PacketFlags::ECHO_RESPONSE,
            PacketFlags::FLOW,
        ] {
            assert_ne!(PacketFlags::OPTIONAL_MASK & m, 0, "{m:#X}");
        }
    }

    /// Bit 23 and bits 28-31 are rejected; bits 0-7 are not, because the rejection test only looks
    /// at `& 0xFFFFFF00`. Compatibility note #40.
    #[test]
    fn undefined_bits_reject_only_at_or_above_0x100() {
        let ok = ProtoHeader {
            header: PacketFlags(PacketFlags::TOLERATED_UNDEFINED),
            ..Default::default()
        };
        assert!(ok.verify().is_ok());

        for bad in [0x0080_0000u32, 0x1000_0000, 0x8000_0000] {
            let h = ProtoHeader {
                header: PacketFlags(bad),
                ..Default::default()
            };
            assert_eq!(
                h.verify(),
                Err(WireError::UndefinedFlagBits(bad)),
                "{bad:#X}"
            );
        }
    }

    /// `datalen_ >= 0xFFE1` is rejected; 0xFFE0 is the largest accepted.
    #[test]
    fn datalen_rejection_threshold() {
        let mk = |d| ProtoHeader {
            datalen: d,
            ..Default::default()
        };
        assert!(mk(0xFFE0).verify().is_ok());
        assert_eq!(mk(0xFFE1).verify(), Err(WireError::DatalenTooLarge(0xFFE1)));
        assert_eq!(mk(0xFFFF).verify(), Err(WireError::DatalenTooLarge(0xFFFF)));
    }

    /// `rec_id` indexes a 256-entry table.
    #[test]
    fn rec_id_must_fit_the_receiver_table() {
        let mk = |r| ProtoHeader {
            rec_id: r,
            ..Default::default()
        };
        assert!(mk(0xFF).verify().is_ok());
        assert_eq!(mk(0x100).verify(), Err(WireError::RecIdOutOfRange(0x100)));
    }

    /// A server receiving a client's packet has no such table: `rec_id` is the id it assigned.
    #[test]
    fn a_server_accepts_any_client_rec_id() {
        let mk = |r| ProtoHeader {
            rec_id: r,
            ..Default::default()
        };
        assert!(mk(0x100).verify_from_client().is_ok());
        assert!(mk(397).verify_from_client().is_ok());
        assert!(mk(0xFFFF).verify_from_client().is_ok());
        let long = ProtoHeader {
            datalen: 0xFFFF,
            ..Default::default()
        };
        assert_eq!(
            long.verify_from_client(),
            Err(WireError::DatalenTooLarge(0xFFFF))
        );
    }

    #[test]
    fn optional_bits_are_yielded_ascending() {
        let f = PacketFlags(
            PacketFlags::FLOW
                | PacketFlags::ACK_SEQUENCE
                | PacketFlags::SERVER_SWITCH
                | PacketFlags::BLOB_FRAGMENTS,
        );
        let bits: Vec<u32> = f.optional_bits().collect();
        assert_eq!(
            bits,
            vec![
                PacketFlags::SERVER_SWITCH,
                PacketFlags::ACK_SEQUENCE,
                PacketFlags::FLOW
            ]
        );
    }
}
