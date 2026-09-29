//! The two ordered wrappers.
//!
//! Source: `docs/networking/messages/00-dispatch-and-queues.md` §3. These are *headers*, not message types: the real message type is the dword that
//! follows the header.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use dereth_primitives::ObjectId;

/// `OrderedEventHeader` — game events, server → client.
/// Pack size 12.
///
/// | offset | size | field |
/// |---:|---:|---|
/// | 0x0 | 4 | magic `0xF7B0` |
/// | 0x4 | 4 | `iid_` — the object the event is ordered against; **0 = global/session-ordered** |
/// | 0x8 | 4 | `stamp_` — per-object event sequence |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderedEventHeader {
    pub iid: ObjectId,
    pub stamp: u32,
}

impl OrderedEventHeader {
    /// The magic in dword 0.
    pub const MAGIC: u32 = 0xF7B0;
    /// Packed header size after `/OPT:ICF` folding.
    pub const PACK_SIZE: usize = 12;

    /// `UnPack` returns 0 and **rewinds the read pointer** when the first dword is not `0xF7B0`;
    /// the caller then treats the whole payload as an unordered event body.
    ///
    /// So this returns `None` without consuming anything.
    #[must_use]
    pub fn read(r: &mut Reader<'_>) -> Option<Self> {
        let probe = r.clone();
        let magic = r.u32().ok()?;
        if magic != Self::MAGIC {
            *r = probe;
            return None;
        }
        match (r.u32(), r.u32()) {
            (Ok(iid), Ok(stamp)) => Some(Self {
                iid: ObjectId(iid),
                stamp,
            }),
            _ => {
                // A truncated header is not an ordered blob either: rewind, exactly as the client's
                // UnPack does when it runs out of bytes.
                *r = probe;
                None
            }
        }
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(Self::MAGIC);
        w.u32(self.iid.0);
        w.u32(self.stamp);
    }
}

/// `OrderedActionHeader` — game actions, client → server.
/// Pack size 8.
///
/// | offset | size | field |
/// |---:|---:|---|
/// | 0x0 | 4 | magic `0xF7B1` |
/// | 0x4 | 4 | `stamp_` — the next UI counter |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderedActionHeader {
    pub stamp: u32,
}

impl OrderedActionHeader {
    pub const MAGIC: u32 = 0xF7B1;
    pub const PACK_SIZE: usize = 8;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let magic = r.u32()?;
        if magic != Self::MAGIC {
            return Err(MessageError::BadMagic {
                expected: Self::MAGIC,
                found: magic,
            });
        }
        Ok(Self { stamp: r.u32()? })
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(Self::MAGIC);
        w.u32(self.stamp);
    }
}

/// The newer-ordering-stamp test — the 16-bit half-range wrap comparison.
///
/// `lhs` is newer when `lhs - rhs` lands in the lower half of the range. The same rule governs
/// every per-object and per-property sequence in the protocol at its own width; see
/// [`crate::wrap::not_older_u8`] for the 8-bit form used by `PropertySequenceGate`.
#[must_use]
pub fn lhs_newer_ordering_stamp(lhs: u16, rhs: u16) -> bool {
    lhs.wrapping_sub(rhs) != 0 && lhs.wrapping_sub(rhs) < 0x8000
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/messages/00-dispatch-and-queues.md` §3, the two ordered wrappers.
    #[test]
    fn worder_hdr_round_trips() {
        let h = OrderedEventHeader {
            iid: ObjectId(0x5000_1234),
            stamp: 7,
        };
        let mut w = Writer::new();
        h.write(&mut w);
        assert_eq!(w.len(), OrderedEventHeader::PACK_SIZE);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(OrderedEventHeader::read(&mut r), Some(h));
        r.expect_exhausted().unwrap();
    }

    /// The H2 acceptance test: `read` returns `None` and does **not** consume when the magic is
    /// absent, because the caller then treats the whole payload as an unordered event body.
    #[test]
    fn worder_hdr_read_does_not_consume_when_the_magic_is_absent() {
        let bytes = [0x13, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
        let mut r = Reader::new(&bytes);
        assert_eq!(OrderedEventHeader::read(&mut r), None);
        assert_eq!(r.position(), 0, "the read pointer must be rewound");
        assert_eq!(
            r.u32().unwrap(),
            0x0013,
            "the body still starts with its own opcode"
        );
    }

    /// A truncated header is not an ordered blob either, and must not half-consume.
    #[test]
    fn worder_hdr_read_rewinds_on_a_short_buffer() {
        let bytes = [0xB0, 0xF7, 0x00, 0x00, 0x01];
        let mut r = Reader::new(&bytes);
        assert_eq!(OrderedEventHeader::read(&mut r), None);
        assert_eq!(r.position(), 0);
    }

    /// `iid_ == 0` is the "globally ordered" case, which ACE never produces and retail did; the
    /// header itself must still decode.
    #[test]
    fn worder_hdr_accepts_a_zero_iid() {
        let bytes = [0xB0, 0xF7, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0];
        let mut r = Reader::new(&bytes);
        assert_eq!(
            OrderedEventHeader::read(&mut r),
            Some(OrderedEventHeader {
                iid: ObjectId(0),
                stamp: 5
            })
        );
    }

    #[test]
    fn order_hdr_round_trips_and_rejects_a_wrong_magic() {
        let h = OrderedActionHeader { stamp: 0x1234 };
        let mut w = Writer::new();
        h.write(&mut w);
        assert_eq!(w.len(), OrderedActionHeader::PACK_SIZE);
        let bytes = w.into_inner();
        assert_eq!(
            OrderedActionHeader::read(&mut Reader::new(&bytes)).unwrap(),
            h
        );

        let bad = [0xB0, 0xF7, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            OrderedActionHeader::read(&mut Reader::new(&bad)),
            Err(MessageError::BadMagic {
                expected: 0xF7B1,
                found: 0xF7B0
            })
        );
    }

    /// The sixteen bit wrap window is half range.
    #[test]
    fn the_sixteen_bit_wrap_window_is_half_range() {
        assert!(lhs_newer_ordering_stamp(5, 4));
        assert!(!lhs_newer_ordering_stamp(4, 5));
        assert!(!lhs_newer_ordering_stamp(5, 5), "equal is not newer");
        // Across the wrap.
        assert!(lhs_newer_ordering_stamp(0x0001, 0xFFFF));
        assert!(!lhs_newer_ordering_stamp(0xFFFF, 0x0001));
        // Exactly half the period: 0x8000 ahead is NOT newer, 0x7FFF ahead is.
        assert!(lhs_newer_ordering_stamp(0x7FFF, 0x0000));
        assert!(!lhs_newer_ordering_stamp(0x8000, 0x0000));
    }
}
