//! zlib / DEFLATE decompression, for the one place the client needs it: a `DDD_DataMessage` whose
//! compressed flag byte is set.
//!
//! The client's decompressor is one call to zlib's `uncompress`:
//!
//! ```text
//! size = the wire `data_size`, size dword included
//! if size <= 4 or size - 4 < 0x10: fail
//! want = the u32 at offset 4                       // the uncompressed length
//! allocate `want` bytes of output
//! uncompress(output + 4, &want, buffer + 8, size - 8)
//! ```
//!
//! So the compressed record on the wire is `[u32 uncompressed_length][zlib stream]`, which is
//! exactly what ACE's `DDDManager.PrependUncompressedFileSize` builds
//! (ACE's `Source/ACE.Server/Managers/DDDManager.cs`), and the stream is a **zlib**
//! wrapper (RFC 1950) around DEFLATE (RFC 1951) — `ZLibStream`, not `DeflateStream`.
//!
//! This is a decompressor only. Nothing in this rebuild compresses: the compressed flag is 0 for
//! all 887,455 retail entries and the client's DDD save path inflates before it stores, so a
//! patched dat keeps the flag at 0.
//!
//! The algorithm is the one in zlib's own `contrib/puff` — canonical Huffman decoded a bit at a
//! time — chosen because it is short enough to read and has no table-construction corner cases to
//! get wrong. Dat records are kilobytes, not megabytes; a table-driven decoder would buy nothing
//! measurable and cost the one thing that matters here, which is being obviously correct.

use crate::error::DatError;

/// RFC 1951 § 3.2.5, the length base for symbols 257..=285.
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
/// The extra bits that follow each of [`LENGTH_BASE`].
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// RFC 1951 § 3.2.5, the distance base for symbols 0..=29.
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
/// The extra bits that follow each of [`DIST_BASE`].
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// RFC 1951 § 3.2.7: the order the code-length code lengths arrive in.
const CLEN_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// A DEFLATE bit stream: least-significant bit first within each byte, and Huffman codes packed
/// most-significant bit of the code first (RFC 1951 § 3.1.1). Both are handled below; getting them
/// the wrong way round is the classic way to write an inflater that decodes garbage.
struct Bits<'a> {
    data: &'a [u8],
    next: usize,
    acc: u32,
    have: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            next: 0,
            acc: 0,
            have: 0,
        }
    }

    fn take(&mut self, n: u32) -> Result<u32, DatError> {
        debug_assert!(n <= 24);
        while self.have < n {
            let b = *self
                .data
                .get(self.next)
                .ok_or(DatError::Inflate("the stream ended mid-code"))?;
            self.next += 1;
            self.acc |= u32::from(b) << self.have;
            self.have += 8;
        }
        let v = self.acc & ((1u32 << n) - 1);
        self.acc >>= n;
        self.have -= n;
        Ok(v)
    }

    /// Drop the rest of the current byte, which a stored block starts after.
    fn align(&mut self) {
        let drop = self.have % 8;
        self.acc >>= drop;
        self.have -= drop;
    }

    /// The next whole bytes, for a stored block. The buffered bits are byte-aligned by then.
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], DatError> {
        // Anything still buffered is a whole number of bytes that have already been consumed from
        // `data`; rewind by that many so the slice starts where the bits do.
        let start = self.next - (self.have / 8) as usize;
        self.acc = 0;
        self.have = 0;
        let end = start
            .checked_add(n)
            .ok_or(DatError::Inflate("stored block length overflows"))?;
        let out = self.data.get(start..end).ok_or(DatError::Inflate(
            "a stored block runs past the end of the stream",
        ))?;
        self.next = end;
        Ok(out)
    }
}

/// A canonical Huffman decoding table: how many codes there are of each length, and the symbols in
/// canonical order. `puff`'s representation.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    /// Build from a code-length vector, rejecting an over- or (for a multi-symbol alphabet)
    /// under-subscribed set the way `inflate` does.
    fn build(lengths: &[u8]) -> Result<Self, DatError> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            if l as usize > 15 {
                return Err(DatError::Inflate("a code length above 15"));
            }
            counts[l as usize] += 1;
        }
        let used = lengths.len() - counts[0] as usize;
        counts[0] = 0;
        if used == 0 {
            return Ok(Self {
                counts,
                symbols: Vec::new(),
            });
        }
        // Kraft: the code set must not be over-subscribed. One symbol of length 1 (`left == 1`
        // after the loop) is the "incomplete but legal" case zlib allows for a distance tree.
        let mut left: i32 = 1;
        for &count in &counts[1..16] {
            left <<= 1;
            left -= i32::from(count);
            if left < 0 {
                return Err(DatError::Inflate("an over-subscribed huffman code set"));
            }
        }
        let mut offsets = [0u16; 16];
        for l in 1..15 {
            offsets[l + 1] = offsets[l] + counts[l];
        }
        let mut symbols = vec![0u16; used];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                let slot = offsets[l as usize] as usize;
                symbols[slot] = u16::try_from(sym).unwrap_or(u16::MAX);
                offsets[l as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, b: &mut Bits<'_>) -> Result<u16, DatError> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= i32::try_from(b.take(1)?).unwrap_or(0);
            let count = i32::from(self.counts[len]);
            if code - count < first {
                let slot = usize::try_from(index + (code - first))
                    .map_err(|_| DatError::Inflate("a negative huffman symbol index"))?;
                return self
                    .symbols
                    .get(slot)
                    .copied()
                    .ok_or(DatError::Inflate("a huffman code with no symbol"));
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(DatError::Inflate("a huffman code longer than 15 bits"))
    }
}

/// The RFC 1951 fixed literal/length and distance trees.
fn fixed_trees() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    for (i, l) in lit.iter_mut().enumerate() {
        *l = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let dist = [5u8; 30];
    // Neither can fail: both are complete, valid code sets by construction.
    (
        Huffman::build(&lit).unwrap_or_else(|_| unreachable!("the fixed literal tree is valid")),
        Huffman::build(&dist).unwrap_or_else(|_| unreachable!("the fixed distance tree is valid")),
    )
}

/// Inflate a raw DEFLATE stream (no zlib wrapper) into at most `limit` bytes.
///
/// # Errors
///
/// [`DatError::Inflate`] for any malformed stream, and for one that would exceed `limit` — a
/// bounded output is what keeps a hostile `DDD_DataMessage` from allocating without end.
pub fn inflate_raw(src: &[u8], limit: usize) -> Result<Vec<u8>, DatError> {
    let mut b = Bits::new(src);
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(1 << 20));
    loop {
        let final_block = b.take(1)? == 1;
        match b.take(2)? {
            0 => {
                b.align();
                let header = b.bytes(4)?;
                let len = usize::from(u16::from_le_bytes([header[0], header[1]]));
                let nlen = usize::from(u16::from_le_bytes([header[2], header[3]]));
                if len ^ 0xFFFF != nlen {
                    return Err(DatError::Inflate(
                        "a stored block whose LEN and NLEN disagree",
                    ));
                }
                if out.len() + len > limit {
                    return Err(DatError::Inflate(
                        "the stream expands past the declared length",
                    ));
                }
                out.extend_from_slice(b.bytes(len)?);
            }
            1 => {
                let (lit, dist) = fixed_trees();
                inflate_block(&mut b, &lit, &dist, &mut out, limit)?;
            }
            2 => {
                let (lit, dist) = dynamic_trees(&mut b)?;
                inflate_block(&mut b, &lit, &dist, &mut out, limit)?;
            }
            _ => return Err(DatError::Inflate("block type 3 is reserved")),
        }
        if final_block {
            return Ok(out);
        }
    }
}

/// RFC 1951 § 3.2.7: read the two trees a dynamic block carries.
fn dynamic_trees(b: &mut Bits<'_>) -> Result<(Huffman, Huffman), DatError> {
    let nlen = b.take(5)? as usize + 257;
    let ndist = b.take(5)? as usize + 1;
    let ncode = b.take(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(DatError::Inflate("too many literal or distance codes"));
    }
    let mut clen = [0u8; 19];
    for i in 0..ncode {
        clen[CLEN_ORDER[i]] = u8::try_from(b.take(3)?).unwrap_or(0);
    }
    let code_tree = Huffman::build(&clen)?;

    let mut lengths = vec![0u8; nlen + ndist];
    let mut i = 0;
    while i < lengths.len() {
        let sym = code_tree.decode(b)?;
        match sym {
            0..=15 => {
                lengths[i] = u8::try_from(sym).unwrap_or(0);
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(DatError::Inflate("a repeat with no previous length"));
                }
                let prev = lengths[i - 1];
                let n = b.take(2)? as usize + 3;
                if i + n > lengths.len() {
                    return Err(DatError::Inflate(
                        "a repeat past the end of the length list",
                    ));
                }
                lengths[i..i + n].fill(prev);
                i += n;
            }
            17 | 18 => {
                let n = if sym == 17 {
                    b.take(3)? as usize + 3
                } else {
                    b.take(7)? as usize + 11
                };
                if i + n > lengths.len() {
                    return Err(DatError::Inflate(
                        "a zero run past the end of the length list",
                    ));
                }
                i += n;
            }
            _ => return Err(DatError::Inflate("an invalid code-length symbol")),
        }
    }
    if lengths[256] == 0 {
        return Err(DatError::Inflate("no end-of-block code"));
    }
    let lit = Huffman::build(&lengths[..nlen])?;
    let dist = Huffman::build(&lengths[nlen..])?;
    Ok((lit, dist))
}

/// One Huffman-coded block, literal by literal.
fn inflate_block(
    b: &mut Bits<'_>,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    limit: usize,
) -> Result<(), DatError> {
    loop {
        let sym = lit.decode(b)?;
        if sym < 256 {
            if out.len() >= limit {
                return Err(DatError::Inflate(
                    "the stream expands past the declared length",
                ));
            }
            out.push(u8::try_from(sym).unwrap_or(0));
            continue;
        }
        if sym == 256 {
            return Ok(());
        }
        let li = usize::from(sym) - 257;
        if li >= LENGTH_BASE.len() {
            return Err(DatError::Inflate("an invalid length symbol"));
        }
        let len = usize::from(LENGTH_BASE[li]) + b.take(u32::from(LENGTH_EXTRA[li]))? as usize;

        let dsym = usize::from(dist.decode(b)?);
        if dsym >= DIST_BASE.len() {
            return Err(DatError::Inflate("an invalid distance symbol"));
        }
        let d = usize::from(DIST_BASE[dsym]) + b.take(u32::from(DIST_EXTRA[dsym]))? as usize;
        if d > out.len() {
            return Err(DatError::Inflate(
                "a back-reference before the start of the output",
            ));
        }
        if out.len() + len > limit {
            return Err(DatError::Inflate(
                "the stream expands past the declared length",
            ));
        }
        // Byte at a time, on purpose: DEFLATE allows an overlapping copy (`d < len`), which is how
        // a run is encoded, and a slice copy would get it wrong.
        let start = out.len() - d;
        for k in 0..len {
            let byte = out[start + k];
            out.push(byte);
        }
    }
}

/// RFC 1950 Adler-32.
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Inflate a zlib stream (RFC 1950) of known uncompressed length.
///
/// `want` is the length the sender declared. The output must be exactly that long: a stream that
/// stops short or runs long is a stream the receiver mis-read, and storing the result would put a
/// truncated record in the dat.
///
/// # Errors
///
/// [`DatError::Inflate`] for a malformed header, body or checksum;
/// [`DatError::InflateLength`] when the result is not `want` bytes.
pub fn inflate_zlib(src: &[u8], want: usize) -> Result<Vec<u8>, DatError> {
    let (&cmf, rest) = src
        .split_first()
        .ok_or(DatError::Inflate("an empty zlib stream"))?;
    let (&flg, body) = rest
        .split_first()
        .ok_or(DatError::Inflate("a one-byte zlib stream"))?;
    if cmf & 0x0F != 8 {
        return Err(DatError::Inflate(
            "the zlib compression method is not deflate",
        ));
    }
    if cmf >> 4 > 7 {
        return Err(DatError::Inflate("a zlib window above 32 KiB"));
    }
    if (u32::from(cmf) * 256 + u32::from(flg)) % 31 != 0 {
        return Err(DatError::Inflate("the zlib header check fails"));
    }
    if flg & 0x20 != 0 {
        return Err(DatError::Inflate(
            "a zlib preset dictionary, which no sender uses",
        ));
    }
    if body.len() < 4 {
        return Err(DatError::Inflate(
            "a zlib stream with no room for its checksum",
        ));
    }
    let out = inflate_raw(body, want)?;
    if out.len() != want {
        return Err(DatError::InflateLength {
            got: out.len(),
            want,
        });
    }
    // The checksum is the last four bytes, big-endian. `inflate_raw` stops at the final block, so
    // finding the trailer means taking it from the end rather than from the bit reader's position.
    let tail = &body[body.len() - 4..];
    let declared = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
    if declared != adler32(&out) {
        return Err(DatError::Inflate("the zlib adler-32 does not match"));
    }
    Ok(out)
}

/// Inflates a compressed `DDD_DataMessage` payload to the bytes
/// the dat will hold.
///
/// `data` is the message's payload **after** its size dword, which is what the client's cache
/// pack leaves after its first four bytes: the client's size is the wire
/// `data_size` and its buffer's first four bytes are the size slot, zeroed. So native's
/// u32 at offset 4 is `data[0..4]` here, and its `buffer + 8, size - 8` is `data[4..]`.
///
/// The length gate is native's, expressed in these coordinates: native needs `size > 4` and
/// `size - 4 >= 0x10`, and `size == data.len() + 4`, so `data.len() >= 0x10`.
///
/// # Errors
///
/// [`DatError::CompressedTooShort`] for a payload below the client's own floor, and whatever
/// [`inflate_zlib`] returns for a stream that does not inflate to its declared length.
pub fn decompress_ddd_record(data: &[u8]) -> Result<Vec<u8>, DatError> {
    if data.len() < 0x10 {
        return Err(DatError::CompressedTooShort(data.len()));
    }
    let want = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    inflate_zlib(&data[4..], want)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors produced with CPython's `zlib`, which is the same zlib ACE's `ZLibStream` is a
    /// reimplementation of:
    ///
    /// ```text
    /// python -c "import zlib;print(zlib.compress(b'...', LEVEL).hex())"
    /// ```
    ///
    /// Level 0 is a stored block, level 1 a fixed-Huffman block for short input, level 9 a dynamic
    /// one for input with enough structure — so the three vectors below cover all three block
    /// types, which is the whole of RFC 1951's outer layer.
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }

    const PLAIN_A: &[u8] = b"the quick brown fox";

    /// `zlib.compress(b"the quick brown fox", 0)` — BTYPE 0, a stored block.
    const STORED: &str = "7801011300ecff74686520717569636b2062726f776e20666f78478e0734";
    /// `zlib.compress(b"the quick brown fox", 1)` — BTYPE 1, fixed Huffman.
    const FIXED: &str = "78012bc94855282ccd4cce56482aca2fcf5348cbaf0000478e0734";
    /// `zlib.compress(dynamic_plain(), 9)` — BTYPE 2, a dynamic tree, 3000 bytes down to 58, so
    /// the back-reference path (including overlapping copies) is exercised too.
    const DYNAMIC: &str = "78daedcaa11100400803c1a66828138142311174ff65bcb9d5ab6ce5da23efa43bb3d6b82fb551f9ce250e87c3e170381c0e87f3f53c4417af82";

    /// The plaintext behind [`DYNAMIC`], written the same way in Python and here so the vector can
    /// be regenerated without a stored copy of 3000 bytes:
    /// `bytes(al[(i*i*7+i*13)%36] for i in range(3000))`.
    fn dynamic_plain() -> Vec<u8> {
        const AL: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        (0..3000u64)
            .map(|i| AL[((i * i * 7 + i * 13) % 36) as usize])
            .collect()
    }

    #[test]
    fn a_stored_block_round_trips() {
        assert_eq!(
            inflate_zlib(&hex(STORED), PLAIN_A.len()).expect("stored"),
            PLAIN_A
        );
    }

    #[test]
    fn a_fixed_huffman_block_round_trips() {
        assert_eq!(
            inflate_zlib(&hex(FIXED), PLAIN_A.len()).expect("fixed"),
            PLAIN_A
        );
    }

    #[test]
    fn a_dynamic_huffman_block_round_trips() {
        let plain = dynamic_plain();
        assert_eq!(
            inflate_zlib(&hex(DYNAMIC), plain.len()).expect("dynamic"),
            plain
        );
    }

    #[test]
    fn a_corrupt_checksum_is_refused() {
        let mut z = hex(FIXED);
        let n = z.len();
        z[n - 1] ^= 0xFF;
        assert!(matches!(
            inflate_zlib(&z, PLAIN_A.len()),
            Err(DatError::Inflate(_))
        ));
    }

    #[test]
    fn a_corrupt_body_is_refused() {
        let mut z = hex(DYNAMIC);
        z[20] ^= 0x55;
        assert!(inflate_zlib(&z, dynamic_plain().len()).is_err());
    }

    #[test]
    fn a_wrong_declared_length_is_refused() {
        assert!(matches!(
            inflate_zlib(&hex(FIXED), PLAIN_A.len() + 1),
            Err(DatError::Inflate(_) | DatError::InflateLength { .. })
        ));
        assert!(matches!(
            inflate_zlib(&hex(FIXED), PLAIN_A.len() - 1),
            Err(DatError::Inflate(_) | DatError::InflateLength { .. })
        ));
    }

    #[test]
    fn a_header_that_is_not_zlib_is_refused() {
        assert!(matches!(
            inflate_zlib(&[0x1F, 0x8B, 0x08, 0x00], 4),
            Err(DatError::Inflate(_))
        ));
    }

    #[test]
    fn adler32_matches_the_rfc_example() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }
}
