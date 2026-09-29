// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Cryptography/Hash32.cs
//! `ACE.Common.Cryptography.Hash32`: the packet checksum.
//!
//! ACE's hash is the client's (`dereth_transport::hash32`), step for step: `(length << 16)`
//! plus every whole little-endian dword, plus the 1–3 trailing bytes placed high to low
//! (`byte << 24`, `<< 16`, `<< 8`), all wrapping. The `hash32` rows of
//! `tests/fixtures/ace_common.tsv` are ACE's own output. What stays here is ACE's two signatures.

// ACE: Hash32.Calculate(Span<byte>, int)
/// The checksum of `data[..length]`.
///
/// # Panics
/// When `length` exceeds `data.len()`.
#[must_use]
pub fn calculate(data: &[u8], length: usize) -> u32 {
    calculate_at(data, 0, length)
}

// ACE: Hash32.Calculate(byte[], int, int)
/// [`calculate`] over `data[offset..offset + length]`.
///
/// # Panics
/// When the range exceeds `data.len()`.
#[must_use]
pub fn calculate_at(data: &[u8], offset: usize, length: usize) -> u32 {
    dereth_transport::hash32(&data[offset..offset + length])
}
