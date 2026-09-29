//! The 16-byte fragment header and one whole fragment.
//!
//! See `docs/networking/01-packet-format.md` §4.

use super::{WireError, FRAG_HEADER_SIZE, MAX_FRAG_SIZE};

/// The 16-byte fragment header, little-endian.
///
/// Field names are the client's; ACE's names for the same fields are `Sequence`, `Id`, `Count`,
/// `Size`, `Index`, `Queue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FragmentHeader {
    /// Low 32 bits of the 64-bit NetBlobID.
    pub blob_id_low: u32,
    /// High 32 bits: ordering stamp, sequence byte, ordering type, ephemeral bit.
    pub blob_id_high: u32,
    /// Total number of fragments in this blob.
    pub num_frags: u16,
    /// **Whole fragment** size, including this 16-byte header.
    pub blob_frag_size: u16,
    /// 0-based index of this fragment.
    pub blob_num: u16,
    /// Ordering queue, 1-11.
    pub queue_id: u16,
}

impl FragmentHeader {
    /// The 64-bit NetBlobID this fragment belongs to.
    #[must_use]
    pub const fn blob_id(self) -> u64 {
        ((self.blob_id_high as u64) << 32) | self.blob_id_low as u64
    }

    /// Payload bytes carried by this fragment: `blob_frag_size - 16`.
    #[must_use]
    pub const fn payload_len(self) -> usize {
        (self.blob_frag_size as usize).saturating_sub(FRAG_HEADER_SIZE)
    }

    /// Decode 16 bytes.
    ///
    /// # Errors
    /// [`WireError::FragmentTruncated`] if fewer than 16 bytes are supplied.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, WireError> {
        let b: &[u8; FRAG_HEADER_SIZE] = buf
            .get(..FRAG_HEADER_SIZE)
            .and_then(|s| s.try_into().ok())
            .ok_or(WireError::FragmentTruncated {
                need: FRAG_HEADER_SIZE,
                have: buf.len(),
            })?;
        Ok(Self {
            blob_id_low: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            blob_id_high: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            num_frags: u16::from_le_bytes([b[8], b[9]]),
            blob_frag_size: u16::from_le_bytes([b[10], b[11]]),
            blob_num: u16::from_le_bytes([b[12], b[13]]),
            queue_id: u16::from_le_bytes([b[14], b[15]]),
        })
    }

    #[must_use]
    pub fn to_bytes(self) -> [u8; FRAG_HEADER_SIZE] {
        let mut b = [0u8; FRAG_HEADER_SIZE];
        b[0..4].copy_from_slice(&self.blob_id_low.to_le_bytes());
        b[4..8].copy_from_slice(&self.blob_id_high.to_le_bytes());
        b[8..10].copy_from_slice(&self.num_frags.to_le_bytes());
        b[10..12].copy_from_slice(&self.blob_frag_size.to_le_bytes());
        b[12..14].copy_from_slice(&self.blob_num.to_le_bytes());
        b[14..16].copy_from_slice(&self.queue_id.to_le_bytes());
        b
    }
}

/// One whole fragment: header plus payload, stored as the contiguous bytes that go on the wire.
///
/// The original does not copy at parse time — fragment data points into the receive buffer, and
/// reassembly must copy before the buffer is reused. Owning the bytes here removes that lifetime
/// hazard without changing any observable behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub header: FragmentHeader,
    pub payload: Vec<u8>,
}

impl Fragment {
    ///
    /// Accepts only `0x10 <= blob_frag_size <= 0x1D0` **and** `blob_frag_size <= bytes remaining`;
    /// anything else fails the whole packet, not just the fragment. Returns the fragment and the
    /// number of bytes consumed, which is `blob_frag_size` exactly.
    ///
    /// # Errors
    /// [`WireError::FragmentSizeOutOfRange`] or [`WireError::FragmentTruncated`].
    pub fn from_bytes(buf: &[u8]) -> Result<(Self, usize), WireError> {
        let header = FragmentHeader::from_bytes(buf)?;
        let size = header.blob_frag_size as usize;
        if !(FRAG_HEADER_SIZE..=MAX_FRAG_SIZE).contains(&size) {
            return Err(WireError::FragmentSizeOutOfRange(header.blob_frag_size));
        }
        if buf.len() < size {
            return Err(WireError::FragmentTruncated {
                need: size,
                have: buf.len(),
            });
        }
        Ok((
            Self {
                header,
                payload: buf[FRAG_HEADER_SIZE..size].to_vec(),
            },
            size,
        ))
    }

    /// Build a fragment from a payload, filling in `blob_frag_size`.
    #[must_use]
    pub fn new(mut header: FragmentHeader, payload: Vec<u8>) -> Self {
        header.blob_frag_size = u16::try_from(FRAG_HEADER_SIZE + payload.len()).unwrap_or(u16::MAX);
        Self { header, payload }
    }

    /// The bytes as they go on the wire: 16-byte header then payload.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FRAG_HEADER_SIZE + self.payload.len());
        out.extend_from_slice(&self.header.to_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// `hash32(fragment header, 16) + hash32(payload, blob_frag_size - 16)`.
    ///
    #[must_use]
    pub fn hash(&self) -> u32 {
        crate::crc::hash32(&self.header.to_bytes()).wrapping_add(crate::crc::hash32(&self.payload))
    }

    /// Total wire size.
    #[must_use]
    pub fn wire_len(&self) -> usize {
        FRAG_HEADER_SIZE + self.payload.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the worked example's fragment in
    /// `docs/networking/01-packet-format.md` §6, produced by
    /// the independent packet-format calculation.
    #[test]
    fn worked_example_fragment() {
        let bytes: [u8; 28] = [
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x1C, 0x00, 0x00, 0x00,
            0x09, 0x00, 0xB0, 0xF7, 0x00, 0x00, 0x01, 0x00, 0x00, 0x50, 0x02, 0x00, 0x00, 0x00,
        ];
        let (frag, used) = Fragment::from_bytes(&bytes).expect("valid fragment");
        assert_eq!(used, 28);
        assert_eq!(frag.header.blob_id(), 0x8000_0000_0000_0001);
        assert_eq!(frag.header.num_frags, 1);
        assert_eq!(frag.header.blob_frag_size, 28);
        assert_eq!(frag.header.blob_num, 0);
        assert_eq!(frag.header.queue_id, 9);
        assert_eq!(frag.payload.len(), 12);
        assert_eq!(frag.to_bytes(), bytes);
        assert_eq!(frag.hash(), 0xD041_F7B5);
    }

    /// Fragment creation accepts `0x10 <= blob_frag_size <= 0x1D0` and nothing else.
    #[test]
    fn fragment_size_bounds() {
        let mk = |size: u16, avail: usize| {
            let mut buf = vec![0u8; avail];
            buf[10..12].copy_from_slice(&size.to_le_bytes());
            Fragment::from_bytes(&buf)
        };
        assert!(mk(16, 16).is_ok());
        assert!(mk(464, 464).is_ok());
        assert_eq!(mk(15, 64), Err(WireError::FragmentSizeOutOfRange(15)));
        assert_eq!(mk(465, 512), Err(WireError::FragmentSizeOutOfRange(465)));
        assert_eq!(
            mk(464, 100),
            Err(WireError::FragmentTruncated {
                need: 464,
                have: 100
            })
        );
    }

    #[test]
    fn header_round_trips() {
        let h = FragmentHeader {
            blob_id_low: 0xDEAD_BEEF,
            blob_id_high: 0x2300_0001,
            num_frags: 7,
            blob_frag_size: 464,
            blob_num: 3,
            queue_id: 9,
        };
        assert_eq!(FragmentHeader::from_bytes(&h.to_bytes()), Ok(h));
        assert_eq!(h.payload_len(), 448);
        assert_eq!(h.blob_id(), 0x2300_0001_DEAD_BEEF);
    }
}
