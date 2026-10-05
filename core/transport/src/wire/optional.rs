//! The nineteen optional headers: their masks, their behaviour-flag bits, their wire lengths,
//! and the ascending-order invariant.
//!
//! See `docs/networking/01-packet-format.md` §3.
//!
//! Sections appear on the wire in **strictly ascending flag-mask order**, back to back, with no
//! padding, no length prefix and no type tag — the flag bits are the only framing. Two independent
//! mechanisms guarantee it in the original: an insertion sort on
//! optional-header sections keyed by their mask bit and a mask-ordered factory table walked from
//! index 0 upward on receipt.
//!
//! This is not cosmetic. The client hashes the optional block **per section**; ACE hashes it as one
//! buffer. The two agree only because `n << 16` is additive over a partition and every section
//! except `Flow` is a multiple of 4 bytes long — and `Flow`, the only 6-byte section, has the
//! highest mask and is therefore always last.

use super::{PacketFlags, WireError};

/// How long a section is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionLen {
    /// A fixed number of bytes, memcpy'd from the header object's data.
    Fixed(usize),
    /// `u32 count` followed by `4 * count` sequence ids. `count` must be `< 0x73`, i.e. at most
    /// **114** entries (the sequence-id list reader's `< 0x73` test). ACE's
    /// `MaxNumNakSeqIds` is 115, one larger; the client accepts 114 and rejects a header claiming
    /// 115 or more.
    SeqIdList,
    /// `PString ClientVersion`, the `u32` authenticator length, then that many bytes.
    LoginRequest,
}

/// The `count` value at which the sequence-id list reader rejects the section.
pub const SEQ_ID_LIST_REJECT_AT: u32 = 0x73;

/// One row of the factory table.
#[derive(Debug, Clone, Copy)]
pub struct OptionalHeaderSpec {
    /// The packet-header flag bit that selects this section.
    pub mask: u32,
    /// The header's protocol name, so the knowledge base and the code grep together.
    pub name: &'static str,
    /// The header's behaviour flags. **Not on the wire**; it drives the local packet machinery.
    /// See [`self::flags`].
    pub flags: u32,
    /// The section's length on the wire.
    pub len: SectionLen,
}

/// The per-header behaviour bits.
///
/// Not on the wire. Meanings are established by their use sites; see
/// `docs/networking/01-packet-format.md` §3.0.
pub mod flags {
    /// Stripped when the packet is stored for retransmission, and does **not** mark the packet as
    /// needing encryption or sequencing.
    pub const DISPOSABLE: u32 = 0x01;
    /// Must be the only thing in the packet; the packet is rejected if it also carries fragments or
    /// a second optional header.
    pub const EXCLUSIVE: u32 = 0x02;
    /// Allowed on a packet before its recipient connection is established.
    pub const PRE_CONNECTION: u32 = 0x04;
    /// The time-sensitive payload is refreshed before every (re)send, invalidating the checksum.
    pub const TIME_SENSITIVE: u32 = 0x08;
    /// Not eligible for the standalone path.
    pub const NO_STANDALONE: u32 = 0x10;
    /// Lets `transmit_new_packets` send the packet even when `wire_room_left` says no. Inert in the
    /// retail client because its wire-room test is folded onto `return 1`, but the bit
    /// is carried faithfully so a future pacing implementation keeps the bypass.
    pub const PRIORITY: u32 = 0x20;
    /// Enqueuing this header refreshes the connection's activity timestamp.
    pub const TOUCH_CONNECTION: u32 = 0x40;
    /// Present on ConnectRequest and ConnectResponse only. **No read site anywhere in the client.**
    /// Compatibility note #35: carry the value, never branch on it.
    pub const UNREAD_0X20000000: u32 = 0x2000_0000;
    /// Present on Referral only. **No read site anywhere in the client.**
    /// Compatibility note #35: carry the value, never branch on it.
    pub const UNREAD_0X40000000: u32 = 0x4000_0000;
}

/// The nineteen sections, in ascending mask order — the same order used by the factory table and
/// on the wire.
///
/// Every row cites its section in
/// `docs/networking/01-packet-format.md` §§3.1-3.19.
pub static OPTIONAL_HEADERS: [OptionalHeaderSpec; 19] = [
    // §3.1  ServerSwitch: section mask 256, behavior flags 96, one server-switch record.
    OptionalHeaderSpec {
        mask: PacketFlags::SERVER_SWITCH,
        name: "ServerSwitch",
        flags: flags::PRIORITY | flags::TOUCH_CONNECTION,
        len: SectionLen::Fixed(8),
    },
    // §3.2  LogonServerAddr: section mask 512, behavior flags 7, one `sockaddr_in`.
    OptionalHeaderSpec {
        mask: PacketFlags::LOGON_SERVER_ADDR,
        name: "LogonServerAddr",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(16),
    },
    // §3.3  EmptyHeader1: section mask 1024, behavior flags 7, no body.
    OptionalHeaderSpec {
        mask: PacketFlags::EMPTY_HEADER1,
        name: "EmptyHeader1",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(0),
    },
    // §3.4  Referral: section mask 2048, behavior flags 1073741922, one referral record.
    OptionalHeaderSpec {
        mask: PacketFlags::REFERRAL,
        name: "Referral",
        flags: flags::UNREAD_0X40000000
            | flags::TOUCH_CONNECTION
            | flags::PRIORITY
            | flags::EXCLUSIVE,
        len: SectionLen::Fixed(32),
    },
    // §3.5  RequestRetransmit: section mask 4096, behavior flags 33, a sequence-id list.
    OptionalHeaderSpec {
        mask: PacketFlags::REQUEST_RETRANSMIT,
        name: "RequestRetransmit",
        flags: flags::DISPOSABLE | flags::PRIORITY,
        len: SectionLen::SeqIdList,
    },
    // §3.6  RejectRetransmit: section mask 8192, behavior flags 33, a sequence-id list.
    OptionalHeaderSpec {
        mask: PacketFlags::REJECT_RETRANSMIT,
        name: "RejectRetransmit",
        flags: flags::DISPOSABLE | flags::PRIORITY,
        len: SectionLen::SeqIdList,
    },
    // §3.7  AckSequence: section mask 16384, behavior flags 1, one u32.
    OptionalHeaderSpec {
        mask: PacketFlags::ACK_SEQUENCE,
        name: "AckSequence",
        flags: flags::DISPOSABLE,
        len: SectionLen::Fixed(4),
    },
    // §3.8  Disconnect: section mask 32768, behavior flags 3, no body.
    OptionalHeaderSpec {
        mask: PacketFlags::DISCONNECT,
        name: "Disconnect",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE,
        len: SectionLen::Fixed(0),
    },
    // §3.9
    OptionalHeaderSpec {
        mask: PacketFlags::LOGIN_REQUEST,
        name: "LoginRequest",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::LoginRequest,
    },
    // §3.10 WorldLoginRequest: section mask 131072, behavior flags 7, one u64.
    OptionalHeaderSpec {
        mask: PacketFlags::WORLD_LOGIN_REQUEST,
        name: "WorldLoginRequest",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(8),
    },
    // §3.11
    OptionalHeaderSpec {
        mask: PacketFlags::CONNECT_REQUEST,
        name: "ConnectRequest",
        flags: flags::UNREAD_0X20000000
            | flags::DISPOSABLE
            | flags::EXCLUSIVE
            | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(32),
    },
    // §3.12 ConnectResponse: section mask 524288, behavior flags 536870919, one u64.
    OptionalHeaderSpec {
        mask: PacketFlags::CONNECT_RESPONSE,
        name: "ConnectResponse",
        flags: flags::UNREAD_0X20000000
            | flags::DISPOSABLE
            | flags::EXCLUSIVE
            | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(8),
    },
    // §3.13 NetError: section mask 1048576, behavior flags 7.
    OptionalHeaderSpec {
        mask: PacketFlags::NET_ERROR,
        name: "NetError",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(8),
    },
    // §3.14 NetErrorDisconnect: section mask 2097152, behavior flags 2.
    // Flags 2 mean exclusive and *not* disposable, so this section IS sequenced and encrypted. Easy to
    // get wrong because it looks like its plaintext sibling NetError.
    OptionalHeaderSpec {
        mask: PacketFlags::NET_ERROR_DISCONNECT,
        name: "NetErrorDisconnect",
        flags: flags::EXCLUSIVE,
        len: SectionLen::Fixed(8),
    },
    // §3.15 CICMDCommand: section mask 4194304, behavior flags 7, one command record.
    OptionalHeaderSpec {
        mask: PacketFlags::CICMD_COMMAND,
        name: "CICMDCommand",
        flags: flags::DISPOSABLE | flags::EXCLUSIVE | flags::PRE_CONNECTION,
        len: SectionLen::Fixed(8),
    },
    // §3.16
    OptionalHeaderSpec {
        mask: PacketFlags::TIME_SYNC,
        name: "TimeSync",
        flags: flags::TIME_SENSITIVE | flags::NO_STANDALONE,
        len: SectionLen::Fixed(8),
    },
    // §3.17
    OptionalHeaderSpec {
        mask: PacketFlags::ECHO_REQUEST,
        name: "EchoRequest",
        flags: flags::TIME_SENSITIVE | flags::NO_STANDALONE,
        len: SectionLen::Fixed(4),
    },
    // §3.18
    OptionalHeaderSpec {
        mask: PacketFlags::ECHO_RESPONSE,
        name: "EchoResponse",
        flags: flags::TIME_SENSITIVE | flags::NO_STANDALONE,
        len: SectionLen::Fixed(8),
    },
    // §3.19 Flow: section mask 134217728, behavior flags 16, one flow record.
    // The only section whose length is not a multiple of 4, and the highest mask, so it is always
    // last. That coincidence is what makes per-section and whole-block hashing agree.
    OptionalHeaderSpec {
        mask: PacketFlags::FLOW,
        name: "Flow",
        flags: flags::NO_STANDALONE,
        len: SectionLen::Fixed(6),
    },
];

/// Look up a section by its mask.
#[must_use]
pub fn spec_for(mask: u32) -> Option<&'static OptionalHeaderSpec> {
    OPTIONAL_HEADERS.iter().find(|s| s.mask == mask)
}

/// Is this section disposable — stripped when the packet is cached for retransmit, and not a reason
/// to encrypt the packet? Flag `0x01`.
#[must_use]
pub fn is_disposable(mask: u32) -> bool {
    spec_for(mask).is_some_and(|s| s.flags & flags::DISPOSABLE != 0)
}

/// The ids inside a received `RequestRetransmit` or `RejectRetransmit` section.
///
/// Both sequence-id list variants store the count first and the ids after it; every reader walks
/// exactly `count` entries.
///
/// [`section_len`] has already rejected a count of `0x73` or more and a section that does not carry
/// its ids, so a body that reaches here is well formed. The `take` is belt and braces: a caller that
/// hands over a hand-built block must not be able to make this allocate on a hostile count.
#[must_use]
pub fn seq_ids(body: &[u8]) -> Vec<u32> {
    let Some(count) = read_u32(body, 0) else {
        return Vec::new();
    };
    let count = (count as usize).min(SEQ_ID_LIST_REJECT_AT as usize - 1);
    (0..count)
        .filter_map(|i| read_u32(body, 4 + 4 * i))
        .collect()
}

/// Consume one section from `buf`, returning its length.
///
/// The client's readers take their bytes straight from the receive buffer; the caller slices.
///
/// # Errors
/// [`WireError::SectionTruncated`], [`WireError::SeqIdListTooLong`] or
/// [`WireError::MalformedPString`].
pub fn section_len(mask: u32, buf: &[u8]) -> Result<usize, WireError> {
    let spec = spec_for(mask).ok_or(WireError::UndefinedFlagBits(mask))?;
    match spec.len {
        SectionLen::Fixed(n) => {
            if buf.len() < n {
                return Err(WireError::SectionTruncated {
                    mask,
                    need: n,
                    have: buf.len(),
                });
            }
            Ok(n)
        }
        SectionLen::SeqIdList => {
            let count = read_u32(buf, 0).ok_or(WireError::SectionTruncated {
                mask,
                need: 4,
                have: buf.len(),
            })?;
            if count >= SEQ_ID_LIST_REJECT_AT {
                return Err(WireError::SeqIdListTooLong { mask, count });
            }
            // count < 0x73, so this cannot overflow.
            let n = 4 + 4 * count as usize;
            if buf.len() < n {
                return Err(WireError::SectionTruncated {
                    mask,
                    need: n,
                    have: buf.len(),
                });
            }
            Ok(n)
        }
        SectionLen::LoginRequest => {
            let after_version = pstring_len(buf).ok_or(WireError::MalformedPString)?;
            let cb_auth = read_u32(buf, after_version).ok_or(WireError::SectionTruncated {
                mask,
                need: after_version + 4,
                have: buf.len(),
            })?;
            // It must be < 0xFFE1 and must not run past the end of the packet.
            if cb_auth >= u32::from(super::DATALEN_REJECT_AT) {
                return Err(WireError::SectionTruncated {
                    mask,
                    need: cb_auth as usize,
                    have: buf.len(),
                });
            }
            let n = after_version + 4 + cb_auth as usize;
            if buf.len() < n {
                return Err(WireError::SectionTruncated {
                    mask,
                    need: n,
                    have: buf.len(),
                });
            }
            Ok(n)
        }
    }
}

use super::le::read_u32;

/// Length of a packed `PString`, including its length word and its 0-3 alignment bytes.
///
/// A `u16` length, or `u16 0xFFFF` followed by a
/// `u32` length when the string is 65,535 bytes or longer, then the bytes, then zero padding to a
/// multiple of 4. ACE calls this `ReadString16L`.
///
/// The padding is measured from the start of the string field, which is the same thing as from the
/// start of the packet's optional block: every section with a mask below `LoginRequest`'s is a
/// multiple of 4 bytes long, so the two never diverge.
#[must_use]
pub fn pstring_len(buf: &[u8]) -> Option<usize> {
    let short = u16::from_le_bytes([*buf.first()?, *buf.get(1)?]);
    let (prefix, len) = if short == 0xFFFF {
        (6usize, read_u32(buf, 2)? as usize)
    } else {
        (2usize, short as usize)
    };
    let raw = prefix.checked_add(len)?;
    let padded = raw.next_multiple_of(4);
    if buf.len() < padded {
        return None;
    }
    Some(padded)
}

/// Deserialise a packed `PString`, the inverse of [`pstring_pack`]: the string's bytes and the
/// padded length consumed. `None` when the buffer is shorter than the padded string.
#[must_use]
pub fn pstring_unpack(buf: &[u8]) -> Option<(&[u8], usize)> {
    let padded = pstring_len(buf)?;
    let short = u16::from_le_bytes([buf[0], buf[1]]);
    let (prefix, len) = if short == 0xFFFF {
        (6usize, read_u32(buf, 2)? as usize)
    } else {
        (2usize, usize::from(short))
    };
    Some((buf.get(prefix..prefix.checked_add(len)?)?, padded))
}

/// Serialise a packed `PString`.
#[must_use]
pub fn pstring_pack(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 8);
    if s.len() >= 0xFFFF {
        out.extend_from_slice(&0xFFFFu16.to_le_bytes());
        out.extend_from_slice(&u32::try_from(s.len()).unwrap_or(u32::MAX).to_le_bytes());
    } else {
        out.extend_from_slice(&u16::try_from(s.len()).unwrap_or(u16::MAX).to_le_bytes());
    }
    out.extend_from_slice(s);
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

/// Serialise an archive string: the length in the compressed form (one byte below `0x80`, two
/// below `0x4000` with the top bit set, otherwise four with the top two bits set, the low half
/// little-endian), then the bytes, with no padding. The login request's password travels in this
/// form, unlike the account name beside it, which is a packed string.
#[must_use]
pub fn astring_pack(s: &[u8]) -> Vec<u8> {
    let n = u32::try_from(s.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(s.len() + 4);
    // Each arm masks explicitly to the byte it writes.
    #[allow(clippy::cast_possible_truncation)]
    if n < 0x80 {
        out.push(n as u8);
    } else if n < 0x4000 {
        out.push(((n >> 8) as u8) | 0x80);
        out.push((n & 0xFF) as u8);
    } else {
        out.push(((n >> 24) as u8) | 0xC0);
        out.push(((n >> 16) & 0xFF) as u8);
        out.extend_from_slice(&((n & 0xFFFF) as u16).to_le_bytes());
    }
    out.extend_from_slice(s);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Oracle: `docs/networking/01-packet-format.md` §§3.1-3.19, the table of nineteen
    /// optional headers with their flags and lengths.
    #[test]
    fn table_is_ascending_and_complete() {
        assert_eq!(OPTIONAL_HEADERS.len(), 19);
        for w in OPTIONAL_HEADERS.windows(2) {
            assert!(w[0].mask < w[1].mask, "{} then {}", w[0].name, w[1].name);
        }
        let masks: BTreeSet<u32> = OPTIONAL_HEADERS.iter().map(|s| s.mask).collect();
        assert_eq!(masks.len(), 19, "no duplicate masks");
        let or: u32 = OPTIONAL_HEADERS.iter().fold(0, |a, s| a | s.mask);
        assert_eq!(or, PacketFlags::OPTIONAL_MASK);
    }

    /// Every section but `Flow` is a multiple of 4 bytes, and `Flow` is last. That is the entire
    /// reason the client's per-section hash and ACE's whole-block hash agree. See
    /// `docs/networking/01-packet-format.md` §5.2.
    #[test]
    fn only_flow_is_not_a_multiple_of_four_and_it_is_last() {
        for spec in &OPTIONAL_HEADERS {
            let fixed = match spec.len {
                SectionLen::Fixed(n) => n,
                // Both variable forms are 4-aligned by construction: the seq-id list is 4 + 4n, and
                // a packed PString is padded to a multiple of 4.
                SectionLen::SeqIdList | SectionLen::LoginRequest => 4,
            };
            if spec.name == "Flow" {
                assert_eq!(fixed % 4, 2);
            } else {
                assert_eq!(fixed % 4, 0, "{}", spec.name);
            }
        }
        assert_eq!(
            OPTIONAL_HEADERS[OPTIONAL_HEADERS.len() - 1].name,
            "Flow",
            "Flow must sort last or the checksum equivalence breaks"
        );
    }

    /// Oracle: the packet-format specification's list of the twelve disposable headers, by name.
    #[test]
    fn exactly_twelve_headers_are_disposable() {
        let disposable: Vec<&str> = OPTIONAL_HEADERS
            .iter()
            .filter(|s| s.flags & flags::DISPOSABLE != 0)
            .map(|s| s.name)
            .collect();
        assert_eq!(
            disposable,
            vec![
                "LogonServerAddr",
                "EmptyHeader1",
                "RequestRetransmit",
                "RejectRetransmit",
                "AckSequence",
                "Disconnect",
                "LoginRequest",
                "WorldLoginRequest",
                "ConnectRequest",
                "ConnectResponse",
                "NetError",
                "CICMDCommand",
            ]
        );
    }

    /// `NetErrorDisconnect` has behavior flags 2: exclusive, **not** disposable, so it is sequenced and
    /// encrypted, unlike its plaintext sibling `NetError` (behavior flags 7). See
    /// `docs/networking/01-packet-format.md` §5.3.
    #[test]
    fn net_error_disconnect_is_not_disposable() {
        assert!(is_disposable(PacketFlags::NET_ERROR));
        assert!(!is_disposable(PacketFlags::NET_ERROR_DISCONNECT));
        assert_eq!(
            spec_for(PacketFlags::NET_ERROR_DISCONNECT).map(|s| s.flags),
            Some(flags::EXCLUSIVE)
        );
    }

    /// Compatibility note #35: the 0x20000000 and 0x40000000 flag bits have no read
    /// site in the client. They are carried so the values stay faithful; this test pins where they
    /// live so a future reader is a deliberate act, not an accident.
    #[test]
    fn the_two_unread_m_flags_bits_appear_only_where_documented() {
        let with_20: Vec<&str> = OPTIONAL_HEADERS
            .iter()
            .filter(|s| s.flags & flags::UNREAD_0X20000000 != 0)
            .map(|s| s.name)
            .collect();
        assert_eq!(with_20, vec!["ConnectRequest", "ConnectResponse"]);
        let with_40: Vec<&str> = OPTIONAL_HEADERS
            .iter()
            .filter(|s| s.flags & flags::UNREAD_0X40000000 != 0)
            .map(|s| s.name)
            .collect();
        assert_eq!(with_40, vec!["Referral"]);
    }

    /// A NAK list caps at 114 entries; a header claiming 115 is rejected. ACE's cap is 115, one
    /// larger, so an ACE NAK of maximum size would be rejected by a retail client.
    /// `docs/networking/01-packet-format.md` §3.5.
    #[test]
    fn seq_id_list_caps_at_114() {
        let mut buf = vec![0u8; 4 + 4 * 114];
        buf[0..4].copy_from_slice(&114u32.to_le_bytes());
        assert_eq!(
            section_len(PacketFlags::REQUEST_RETRANSMIT, &buf),
            Ok(4 + 4 * 114)
        );

        let mut buf = vec![0u8; 4 + 4 * 115];
        buf[0..4].copy_from_slice(&115u32.to_le_bytes());
        assert_eq!(
            section_len(PacketFlags::REQUEST_RETRANSMIT, &buf),
            Err(WireError::SeqIdListTooLong {
                mask: PacketFlags::REQUEST_RETRANSMIT,
                count: 115
            })
        );
    }

    /// PString: `u16 len`, bytes, then zero padding to a multiple of 4. `"1802"` is 2 + 4 = 6,
    /// padded to 8.
    #[test]
    fn pstring_pads_to_four() {
        let packed = pstring_pack(b"1802");
        assert_eq!(packed, vec![0x04, 0x00, b'1', b'8', b'0', b'2', 0x00, 0x00]);
        assert_eq!(pstring_len(&packed), Some(8));

        assert_eq!(pstring_pack(b"").len(), 4);
        assert_eq!(pstring_pack(b"ab").len(), 4);
        assert_eq!(pstring_pack(b"abcd").len(), 8);
    }

    /// `pstring_unpack` is `pstring_pack`'s inverse, including the padding it consumes, and
    /// refuses a string the buffer cuts short.
    #[test]
    fn pstring_unpack_inverts_pack() {
        for s in [&b""[..], b"ab", b"1802", b"account"] {
            let packed = pstring_pack(s);
            assert_eq!(pstring_unpack(&packed), Some((s, packed.len())), "{s:?}");
        }
        let packed = pstring_pack(b"account");
        assert_eq!(pstring_unpack(&packed[..packed.len() - 1]), None);
    }

    /// A LoginRequest section is `PString version | u32 cbAuthData | auth bytes`.
    #[test]
    fn login_request_length_is_computed_from_its_own_fields() {
        let mut buf = pstring_pack(b"1802");
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 12]);
        assert_eq!(
            section_len(PacketFlags::LOGIN_REQUEST, &buf),
            Ok(8 + 4 + 12)
        );

        // Truncated: the declared authenticator runs past the end.
        let mut short = pstring_pack(b"1802");
        short.extend_from_slice(&12u32.to_le_bytes());
        short.extend_from_slice(&[0u8; 4]);
        assert!(section_len(PacketFlags::LOGIN_REQUEST, &short).is_err());
    }

    /// A truncated fixed section is rejected rather than read short.
    #[test]
    fn fixed_sections_reject_truncation() {
        assert_eq!(section_len(PacketFlags::REFERRAL, &[0u8; 32]), Ok(32));
        assert_eq!(
            section_len(PacketFlags::REFERRAL, &[0u8; 31]),
            Err(WireError::SectionTruncated {
                mask: PacketFlags::REFERRAL,
                need: 32,
                have: 31
            })
        );
        // EmptyHeader1 has no payload; its whole effect is that the flag bit is consumed.
        assert_eq!(section_len(PacketFlags::EMPTY_HEADER1, &[]), Ok(0));
    }
}
