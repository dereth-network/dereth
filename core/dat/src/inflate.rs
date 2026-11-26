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
//! The DEFLATE body is decoded by `miniz_oxide`, a table-driven decoder in pure Rust: a patch of a
//! whole land is hundreds of megabytes of compressed records, which a decoder taking a bit at a
//! time made the slowest part of applying it. The zlib wrapper's checks (its header, the declared
//! length, the Adler-32) are this module's own, so a stream is refused for the same reasons as
//! before.

use crate::error::DatError;

/// Inflate a raw DEFLATE stream (no zlib wrapper) into at most `limit` bytes.
///
/// # Errors
///
/// [`DatError::Inflate`] for any malformed stream, and for one that would exceed `limit` — a
/// bounded output is what keeps a hostile `DDD_DataMessage` from allocating without end.
pub fn inflate_raw(src: &[u8], limit: usize) -> Result<Vec<u8>, DatError> {
    use miniz_oxide::inflate::TINFLStatus;
    miniz_oxide::inflate::decompress_to_vec_with_limit(src, limit).map_err(|e| {
        DatError::Inflate(match e.status {
            TINFLStatus::HasMoreOutput => "the stream expands past the declared length",
            TINFLStatus::FailedCannotMakeProgress => "the stream ends before its final block",
            _ => "a malformed deflate stream",
        })
    })
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
