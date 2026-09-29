//! The packet checksum and the header/payload split.
//!
//! The protocol has no confidentiality. This 32-bit additive hash, with the payload half XORed by
//! one ISAAC draw on encrypted packets, is the whole of its integrity story. Three things are
//! load-bearing and each is a documented way to fail every packet:
//!
//! 1. the `n << 16` length term, which wraps like everything else;
//! 2. the [`CHECKSUM_PLACEHOLDER`] substitution in the header hash — an implementation that hashes
//!    the header with a zero checksum field never agrees with anybody;
//! 3. the fact that the payload hash is a sum of *per-section* hashes, not one hash of the whole
//!    payload. That only coincides with ACE's whole-block hash because `n << 16` is additive over a
//!    partition and `Flow`, the one section whose length is not a multiple of 4, always sorts last.
//!
//! See `docs/networking/01-packet-format.md` §5.1-5.2.

/// The literal the client substitutes for the header's checksum field before hashing the header.
///
/// ACE's `PacketHeader.CalculateHash32` uses the same
/// constant. See `docs/networking/01-packet-format.md` §5.2.
pub const CHECKSUM_PLACEHOLDER: u32 = 0xBADD_70DD;

/// Byte offset of the checksum within the 20-byte packet header.
pub(crate) const CHECKSUM_OFFSET: usize = 8;

/// The client's 32-bit checksum.
///
/// Identical to ACE's `Hash32.Calculate`. Every arithmetic step is wrapping `u32`, including the
/// length term. The tail loop shifts the 0-3 leftover bytes by 24, 16 and 8 — note that the *first*
/// leftover byte gets the *largest* shift, which is the opposite of a little-endian read and is a
/// classic place to differ by accident.
///
/// See `docs/networking/01-packet-format.md` §5.1.
#[must_use]
pub fn hash32(buf: &[u8]) -> u32 {
    // The original's signature is `(const uint8* p, uint32 n)`, so the length term is a u32 by
    // construction. Nothing in the protocol exceeds the 65,504-byte receive buffer; the saturation
    // here is unreachable and exists only so the cast is not a truncation.
    let n = u32::try_from(buf.len()).unwrap_or(u32::MAX);
    let mut checksum = n.wrapping_shl(16);

    let (words, tail) = buf.as_chunks::<4>();
    for word in words {
        checksum = checksum.wrapping_add(u32::from_le_bytes(*word));
    }

    let mut shift = 24u32;
    for &b in tail {
        checksum = checksum.wrapping_add(u32::from(b).wrapping_shl(shift));
        shift = shift.wrapping_sub(8);
    }
    checksum
}

/// Hash of the 20-byte transport header with `checksum` replaced by [`CHECKSUM_PLACEHOLDER`].
///
/// The header checksum. The original hashes a copy of the header: it stores
/// the placeholder over the checksum field and hashes the copy; the wire bytes are untouched, which
/// is why the receiver can run the identical computation over the packet it just received.
///
/// See `docs/networking/01-packet-format.md` §5.2.
#[must_use]
pub fn header_hash_bytes(header: &[u8; 20]) -> u32 {
    let mut tmp = *header;
    tmp[CHECKSUM_OFFSET..CHECKSUM_OFFSET + 4].copy_from_slice(&CHECKSUM_PLACEHOLDER.to_le_bytes());
    hash32(&tmp)
}

/// One whole fragment's contribution to the payload hash.
///
/// The 16-byte fragment header and the payload are hashed **separately** and summed; hashing the
/// fragment as one buffer gives a different answer, because the `n << 16` term would then be
/// `(16 + payload) << 16` once instead of `16 << 16` plus `payload << 16`.
///
/// Returns `None` if `frag` is shorter than the 16-byte header, which the wire parser rejects
/// before it ever gets here.
///
/// See `docs/networking/01-packet-format.md` §5.2.
#[must_use]
pub fn fragment_hash(frag: &[u8]) -> Option<u32> {
    let (hdr, payload) = frag.split_at_checked(crate::wire::FRAG_HEADER_SIZE)?;
    Some(hash32(hdr).wrapping_add(hash32(payload)))
}

/// The payload half of the packet checksum: the sum of every optional header's hash and every
/// fragment's hash.
///
/// `optional` must already be in ascending flag-mask order. The sum itself does not care — addition
/// commutes — but the *wire* does, and computing the hash from a differently ordered list than the
/// one you serialise is how you get a packet that checksums correctly against nothing.
///
/// The packet checksum. See
/// `docs/networking/01-packet-format.md` §5.2.
#[must_use]
pub fn payload_hash<'a>(
    optional: impl IntoIterator<Item = &'a [u8]>,
    fragments: impl IntoIterator<Item = &'a [u8]>,
) -> u32 {
    let mut total = 0u32;
    for block in optional {
        total = total.wrapping_add(hash32(block));
    }
    for frag in fragments {
        total = total.wrapping_add(fragment_hash(frag).unwrap_or(0));
    }
    total
}

/// The value the sender writes into the header's checksum field.
///
/// ```text
/// checksum = headerHash + payloadHash                  // plaintext packet
/// checksum = headerHash + (payloadHash ^ isaacKey)     // EncryptedChecksum packet
/// ```
///
/// The checksum encryption. See
/// `docs/networking/01-packet-format.md` §5.2-5.3.
#[must_use]
pub fn wire_checksum(header_hash: u32, payload_hash: u32, isaac_key: Option<u32>) -> u32 {
    let pay = match isaac_key {
        Some(k) => payload_hash ^ k,
        None => payload_hash,
    };
    header_hash.wrapping_add(pay)
}

/// Recover the ISAAC value an encrypted packet was XORed with, given the packet as received.
///
/// This is the receive-side inverse of [`wire_checksum`] and it is also the diagnostic that tells
/// you whether a CRC failure is a checksum bug or a key-stream desynchronisation: if the recovered
/// value appears anywhere in the peer's stream, the checksum is right and the *position* is wrong.
///
/// The read path does the equivalent in place. Note that the real client
/// never scans for the key — see `docs/CORRECTIONS.md`.
#[must_use]
pub fn recover_isaac_key(wire_checksum: u32, header_hash: u32, payload_hash: u32) -> u32 {
    wire_checksum.wrapping_sub(header_hash) ^ payload_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the worked example in `docs/networking/01-packet-format.md` §6, produced by
    /// the independent packet-format calculation, whose `calc_checksum32` is asserted equal
    /// to ACE's `Hash32.Calculate` for every length 0-599 by that same script.
    #[test]
    fn worked_example_header_hash() {
        // header with checksum_ = 0: seq 1, flags 0x06, recID 0x000B, interval 0x0100,
        // datalen 0x1C, iteration 1.
        let header: [u8; 20] = [
            0x01, 0x00, 0x00, 0x00, // seq_id
            0x06, 0x00, 0x00, 0x00, // header_
            0x00, 0x00, 0x00, 0x00, // checksum_
            0x0B, 0x00, // rec_id
            0x00, 0x01, // interval_
            0x1C, 0x00, // datalen_
            0x01, 0x00, // iteration_
        ];
        assert_eq!(header_hash_bytes(&header), 0xBBF2_710B);
    }

    /// Oracle: same worked example. `fragmentChecksum = 0xD041F7B5`.
    #[test]
    fn worked_example_fragment_and_wire() {
        let frag: [u8; 28] = [
            0x01, 0x00, 0x00, 0x00, // blob_id_low
            0x00, 0x00, 0x00, 0x80, // blob_id_high (ephemeral bit)
            0x01, 0x00, // num_frags
            0x1C, 0x00, // blob_frag_size
            0x00, 0x00, // blob_num
            0x09, 0x00, // queue_id = UI
            0xB0, 0xF7, 0x00, 0x00, // opcode 0xF7B0
            0x01, 0x00, 0x00, 0x50, //
            0x02, 0x00, 0x00, 0x00, //
        ];
        assert_eq!(fragment_hash(&frag), Some(0xD041_F7B5));

        let pay = payload_hash(std::iter::empty(), [frag.as_slice()]);
        assert_eq!(pay, 0xD041_F7B5);

        // The first draw of CryptoSystem(0xDEADBEEF); see isaac.rs for its own proof.
        let key = 0x5DA2_2D96u32;
        let wire = wire_checksum(0xBBF2_710B, pay, Some(key));
        assert_eq!(wire, 0x49D6_4B2E);
        assert_eq!(recover_isaac_key(wire, 0xBBF2_710B, pay), key);
    }

    /// The placeholder is not cosmetic: hashing with `checksum_ = 0` gives a different answer, and
    /// the difference is exactly the constant. This is the failure mode
    /// `docs/networking/01-packet-format.md` §5.2 warns about.
    #[test]
    fn placeholder_substitution_is_load_bearing() {
        let header = [0u8; 20];
        assert_eq!(
            header_hash_bytes(&header),
            hash32(&header).wrapping_add(CHECKSUM_PLACEHOLDER)
        );
        assert_ne!(header_hash_bytes(&header), hash32(&header));
    }

    /// The length term wraps. A 65,536-byte buffer of zeroes hashes to 0, not to 0x1_0000_0000.
    #[test]
    fn length_term_wraps() {
        assert_eq!(hash32(&vec![0u8; 0x1_0000]), 0);
        assert_eq!(hash32(&[]), 0);
    }

    /// The tail bytes shift down from 24, so a single trailing byte lands in the high byte. Guards
    /// against the natural but wrong "little-endian read of a short tail".
    #[test]
    fn tail_bytes_shift_down_from_24() {
        assert_eq!(hash32(&[0x23]), (1 << 16) + 0x2300_0000);
        assert_eq!(hash32(&[0x01, 0x02]), (2 << 16) + 0x0100_0000 + 0x0002_0000);
        assert_eq!(
            hash32(&[0x01, 0x02, 0x03]),
            (3u32 << 16)
                .wrapping_add(0x0100_0000)
                .wrapping_add(0x0002_0000)
                .wrapping_add(0x0000_0300)
        );
    }

    /// Per-section summing equals whole-block hashing only when every section but the last is a
    /// multiple of 4 bytes. This test states the invariant the section-order rule depends on
    /// (`docs/networking/01-packet-format.md` §5.2), in both directions.
    #[test]
    fn per_section_equals_whole_block_only_when_aligned() {
        let a = [1u8, 2, 3, 4, 5, 6, 7, 8]; // 8 bytes, aligned
        let b = [9u8, 10, 11, 12, 13, 14]; // 6 bytes -- the Flow section's length
        let mut joined = a.to_vec();
        joined.extend_from_slice(&b);
        // Aligned section first, ragged section last: the partition is additive.
        assert_eq!(hash32(&a).wrapping_add(hash32(&b)), hash32(&joined));

        // Ragged section first: it is not. This is why Flow must sort last.
        let mut swapped = b.to_vec();
        swapped.extend_from_slice(&a);
        assert_ne!(hash32(&b).wrapping_add(hash32(&a)), hash32(&swapped));
    }

    /// A fragment shorter than its own header is not hashable; the wire parser rejects it first.
    #[test]
    fn short_fragment_is_not_hashable() {
        assert_eq!(fragment_hash(&[0u8; 15]), None);
        assert_eq!(fragment_hash(&[0u8; 16]), Some(hash32(&[0u8; 16])));
    }
}
