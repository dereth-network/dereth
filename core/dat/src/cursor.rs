//! The byte cursor over one dat payload.
//!
//! Every read is **unaligned** little-endian. The dat unpack archive is not word-aligned
//! (the alignment call is inert unless flag bit `2` is set, and nothing the client does sets
//! it), so a decoder that "helpfully" aligns
//! corrupts every object. Serialisation primitives are described in
//! `docs/formats/03-serialisation-primitives.md`.

use crate::error::DatError;
use dereth_primitives::{DataId, Frame, Quat, Vec3};

/// A byte cursor over one dat payload. Every read is unaligned little-endian.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

macro_rules! read_le {
    ($name:ident, $ty:ty, $n:literal, $doc:literal) => {
        #[doc = $doc]
        #[inline]
        pub fn $name(&mut self) -> Result<$ty, DatError> {
            let b = self.bytes($n)?;
            let mut a = [0u8; $n];
            a.copy_from_slice(b);
            Ok(<$ty>::from_le_bytes(a))
        }
    };
}

impl<'a> Cursor<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The whole payload this cursor walks. `PackObj` alignment is on the absolute address and
    /// every blob base is 4-aligned, so an offset from here is the right thing to align.
    #[must_use]
    pub fn buffer(&self) -> &'a [u8] {
        self.buf
    }

    read_le!(u8, u8, 1, "One byte.");
    read_le!(i8, i8, 1, "One signed byte.");
    read_le!(u16, u16, 2, "`u16` LE, unaligned.");
    read_le!(i16, i16, 2, "`i16` LE, unaligned.");
    read_le!(u32, u32, 4, "`u32` LE, unaligned.");
    read_le!(i32, i32, 4, "`i32` LE, unaligned.");
    read_le!(u64, u64, 8, "`u64` LE, unaligned.");
    read_le!(f32, f32, 4, "IEEE-754 `f32` LE, unaligned.");
    read_le!(f64, f64, 8, "IEEE-754 `f64` LE, unaligned.");

    /// `n` raw bytes. The byte serialiser applies no alignment.
    #[inline]
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], DatError> {
        let end = self.pos.checked_add(n).ok_or(DatError::Overrun(n))?;
        if end > self.buf.len() {
            return Err(DatError::Overrun(end - self.buf.len()));
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// A bare `u32`; the typed-id wrapper adds nothing on the wire.
    #[inline]
    pub fn data_id(&mut self) -> Result<DataId, DatError> {
        Ok(DataId(self.u32()?))
    }

    /// Three floats.
    #[inline]
    pub fn vec3(&mut self) -> Result<Vec3, DatError> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    /// A frame: origin xyz then quaternion w,x,y,z. 28 bytes, unaligned.
    /// The field order matches the on-disk frame record.
    #[inline]
    pub fn frame(&mut self) -> Result<Frame, DatError> {
        let origin = self.vec3()?;
        let rotation = Quat::new(self.f32()?, self.f32()?, self.f32()?, self.f32()?);
        Ok(Frame::new(origin, rotation))
    }

    /// A placed frame, read as [`Self::frame`], for the records whose frame the client uses as it
    /// is: a light, a holding or connection point, a particle offset, a landblock object, a
    /// building, a cell. A frame whose values do not make a valid frame -- a NaN anywhere, or a
    /// rotation that is not a unit quaternion -- keeps its origin and **rotates by nothing**, which
    /// is what the original leaves such a frame doing: it keeps the values but never builds the
    /// rotation it draws and collides with. So a malformed record cannot hand a NaN rotation to
    /// the geometry downstream. (The animation and part-placement frames are read with
    /// [`Self::frame`]: those are normalised where they are used, as in the original.)
    #[inline]
    pub fn placed_frame(&mut self) -> Result<Frame, DatError> {
        let f = self.frame()?;
        if dereth_primitives::frame::frame_is_valid(&f) {
            Ok(f)
        } else {
            Ok(Frame::new(f.origin, Quat::IDENTITY))
        }
    }

    /// The compressed 32-bit integer form, read direction.
    ///
    /// The top two bytes are big-endian and the low 16 bits are a native little-endian `u16`; the
    /// hybrid is deliberate.
    pub fn compressed_u32(&mut self) -> Result<u32, DatError> {
        let b0 = self.u8()?;
        if b0 & 0x80 == 0 {
            return Ok(u32::from(b0));
        }
        let b1 = self.u8()?;
        if b0 & 0x40 == 0 {
            return Ok((u32::from(b0 & 0x7F) << 8) | u32::from(b1));
        }
        let s = self.u16()?;
        Ok((((u32::from(b0 & 0x3F) << 8) | u32::from(b1)) << 16) | u32::from(s))
    }

    /// A DataID relative to a known `0x0X000000` base.
    ///
    /// `u16`, or (bit 15 set) `u16` high plus `u16` low. A packed delta of 0 means the base itself,
    /// which is how the pack side encodes `INVALID_DID`.
    pub fn data_id_of_known_type(&mut self, base: u32) -> Result<DataId, DatError> {
        let v = self.u16()?;
        if v & 0x8000 != 0 {
            let lo = self.u16()?;
            Ok(DataId(base.wrapping_add(
                (u32::from(v & 0x3FFF) << 16) | u32::from(lo),
            )))
        } else {
            Ok(DataId(base.wrapping_add(u32::from(v))))
        }
    }

    /// Narrow archive strings: compressed length, then that many bytes,
    /// **no terminator and no padding**.
    ///
    /// Decoded as cp1252: the client renders these through the Windows ANSI code
    /// page. Use [`Cursor::archive_string_bytes`] when the raw bytes matter.
    pub fn archive_string(&mut self) -> Result<String, DatError> {
        Ok(dereth_primitives::text::cp1252::decode(
            self.archive_string_bytes()?,
        ))
    }

    /// The raw bytes of an `Archive` string, without any decoding.
    pub fn archive_string_bytes(&mut self) -> Result<&'a [u8], DatError> {
        let n = self.compressed_u32()? as usize;
        self.bytes(n)
    }

    /// Wide archive strings: compressed **character** count, then
    /// that many UTF-16LE code units. No BOM, no terminator, no padding.
    pub fn archive_wstring(&mut self) -> Result<String, DatError> {
        let n = self.compressed_u32()? as usize;
        let b = self.bytes(n.checked_mul(2).ok_or(DatError::Overrun(n))?)?;
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_le_bytes)
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    /// A packed string: `u16` length with the `0xFFFF` escape to
    /// a `u32`, then the payload, then zero-padding to a 4-byte boundary.
    ///
    /// The client's two quirks are reproduced: a payload of exactly one NUL byte is the empty
    /// string, and a trailing NUL inside a longer payload shortens the string by one.
    pub fn packobj_string(&mut self) -> Result<String, DatError> {
        Ok(dereth_primitives::text::cp1252::decode(
            self.packobj_string_bytes()?,
        ))
    }

    /// The raw bytes of a `PackObj` string, with the two client quirks applied and the padding
    /// consumed.
    pub fn packobj_string_bytes(&mut self) -> Result<&'a [u8], DatError> {
        let mut n = usize::from(self.u16()?);
        if n == 0xFFFF {
            n = self.u32()? as usize;
        }
        let raw = self.bytes(n)?;
        self.align_ptr();
        // `len == 1 && *p == 0` is the empty string; otherwise a trailing NUL in
        // the payload is dropped from the length.
        let out = if raw.len() == 1 && raw[0] == 0 {
            &raw[..0]
        } else if raw.last() == Some(&0) {
            &raw[..raw.len() - 1]
        } else {
            raw
        };
        Ok(out)
    }

    /// Pointer alignment: `pad = (-pos) & 3`, skipped.
    ///
    /// Only valid inside a `PackObj` blob, whose base is always 4-aligned so that the offset and
    /// the absolute address agree.
    #[inline]
    pub fn align_ptr(&mut self) {
        let pad = self.pos.wrapping_neg() & 3;
        self.pos = (self.pos + pad).min(self.buf.len());
    }

    /// `ALIGN_PTR`'s 2-byte sibling, for readers that round a `u16` list up to 2.
    #[inline]
    pub fn align2(&mut self) {
        self.pos = (self.pos + (self.pos & 1)).min(self.buf.len());
    }

    /// The invariant every decoder ends with: the cursor landed exactly on the payload end.
    pub fn expect_end(&self) -> Result<(), DatError> {
        match self.buf.len().cmp(&self.pos) {
            std::cmp::Ordering::Equal => Ok(()),
            std::cmp::Ordering::Greater => Err(DatError::Shortfall(self.buf.len() - self.pos)),
            std::cmp::Ordering::Less => Err(DatError::Overrun(self.pos - self.buf.len())),
        }
    }

    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    #[inline]
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Skip `n` bytes, bounds-checked.
    #[inline]
    pub fn skip(&mut self, n: usize) -> Result<(), DatError> {
        self.bytes(n).map(|_| ())
    }

    /// Move the cursor to an absolute offset. Used only by the version footer
    /// (the archive version footer), which genuinely seeks.
    pub fn seek(&mut self, pos: usize) -> Result<(), DatError> {
        if pos > self.buf.len() {
            return Err(DatError::Overrun(pos - self.buf.len()));
        }
        self.pos = pos;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_bytes(origin: [f32; 3], q: [f32; 4]) -> Vec<u8> {
        origin
            .iter()
            .chain(q.iter())
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    /// A placed frame with a NaN or non-unit rotation keeps its origin and rotates by nothing; a
    /// valid one, and any frame read without the check, is kept exactly as stored.
    #[test]
    fn a_placed_frame_whose_rotation_is_invalid_rotates_by_nothing() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let good = frame_bytes([1.0, 2.0, 3.0], [half, 0.0, 0.0, half]);
        let f = Cursor::new(&good).placed_frame().expect("28 bytes");
        assert_eq!(f.rotation, Quat::new(half, 0.0, 0.0, half));

        for q in [[f32::NAN, 0.0, 0.0, 1.0], [0.0; 4], [2.0, 0.0, 0.0, 0.0]] {
            let bad = frame_bytes([1.0, 2.0, 3.0], q);
            let f = Cursor::new(&bad).placed_frame().expect("28 bytes");
            assert_eq!(
                f.origin,
                Vec3::new(1.0, 2.0, 3.0),
                "{q:?}: the origin is kept"
            );
            assert_eq!(f.rotation, Quat::IDENTITY, "{q:?}: no rotation");
            let raw = Cursor::new(&bad).frame().expect("28 bytes");
            assert_eq!(
                raw.rotation.w.to_bits(),
                q[0].to_bits(),
                "the plain read is raw"
            );
        }
    }

    /// Oracle: the compressed 32-bit integer form and an independent reference reader.
    fn write_compressed(v: u32) -> Vec<u8> {
        if v < 0x80 {
            vec![u8::try_from(v).unwrap()]
        } else if v < 0x4000 {
            vec![
                u8::try_from((v >> 8) | 0x80).unwrap(),
                u8::try_from(v & 0xFF).unwrap(),
            ]
        } else {
            let mut o = vec![
                u8::try_from((v >> 24) | 0xC0).unwrap(),
                u8::try_from((v >> 16) & 0xFF).unwrap(),
            ];
            o.extend_from_slice(&u16::try_from(v & 0xFFFF).unwrap().to_le_bytes());
            o
        }
    }

    #[test]
    fn compressed_u32_round_trips_at_the_three_encoding_boundaries() {
        let mut cases: Vec<u32> = Vec::new();
        for b in [0u32, 0x7F, 0x80, 0x3FFF, 0x4000, 0x3FFF_FFFF] {
            for d in [0u32, 1] {
                cases.push(b.saturating_sub(d));
                cases.push(b.wrapping_add(d).min(0x3FFF_FFFF));
            }
        }
        // Plus a deterministic sweep across the whole representable space.
        let mut x: u32 = 0x1234_5678;
        for _ in 0..20_000 {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            cases.push(x & 0x3FFF_FFFF);
        }
        for v in cases {
            let enc = write_compressed(v);
            let mut c = Cursor::new(&enc);
            assert_eq!(
                c.compressed_u32().unwrap(),
                v,
                "value {v:#X} encoding {enc:02X?}"
            );
            assert_eq!(c.remaining(), 0, "value {v:#X} left bytes");
            let want = if v < 0x80 {
                1
            } else if v < 0x4000 {
                2
            } else {
                4
            };
            assert_eq!(enc.len(), want);
        }
    }

    /// Worked example: `83 69` decodes to 873 at offset 9 of local-data file `0x2300000E`.
    #[test]
    fn compressed_u32_matches_the_documented_worked_example() {
        let mut c = Cursor::new(&[0x83, 0x69]);
        assert_eq!(c.compressed_u32().unwrap(), 873);
    }

    /// Oracle: the packed-string pack and unpack pair.
    #[test]
    fn packobj_string_pads_to_four_and_escapes_at_ffff() {
        // "abc" -> u16 3, three bytes, one pad byte = 6 bytes.
        let buf = [3u8, 0, b'a', b'b', b'c', 0];
        let mut c = Cursor::new(&buf);
        assert_eq!(c.packobj_string().unwrap(), "abc");
        c.expect_end().unwrap();

        // The 0xFFFF escape: u16 0xFFFF, u32 real length, payload, pad.
        let mut buf = vec![0xFFu8, 0xFF];
        buf.extend_from_slice(&5u32.to_le_bytes());
        // 4 (long form) + 2 + 5 = 11, rounded up to 12 -> one pad byte.
        buf.extend_from_slice(b"hello");
        buf.push(0);
        let mut c = Cursor::new(&buf);
        assert_eq!(c.packobj_string().unwrap(), "hello");
        c.expect_end().unwrap();
    }

    /// The two documented quirks of.
    #[test]
    fn packobj_string_quirks_are_reproduced() {
        // A lone NUL is the empty string.
        let buf = [1u8, 0, 0, 0];
        let mut c = Cursor::new(&buf);
        assert_eq!(c.packobj_string().unwrap(), "");
        c.expect_end().unwrap();

        // A sender that included the terminator: "ab\0" -> "ab".
        let buf = [3u8, 0, b'a', b'b', 0, 0];
        let mut c = Cursor::new(&buf);
        assert_eq!(c.packobj_string().unwrap(), "ab");
        c.expect_end().unwrap();
    }

    /// The `Archive` string form has no padding at all.
    #[test]
    fn archive_string_has_no_padding() {
        let buf = [3u8, b'a', b'b', b'c', 0xEE];
        let mut c = Cursor::new(&buf);
        assert_eq!(c.archive_string().unwrap(), "abc");
        assert_eq!(c.position(), 4, "an Archive string must not pad to 4");
        assert_eq!(c.remaining(), 1);
    }

    /// `align_ptr` is `(-pos) & 3` and is the *only* padding a dat decoder applies.
    #[test]
    fn align_ptr_matches_the_documented_formula() {
        for pos in 0..64usize {
            let buf = vec![0u8; 128];
            let mut c = Cursor::new(&buf);
            c.skip(pos).unwrap();
            c.align_ptr();
            assert_eq!(c.position(), pos + (pos.wrapping_neg() & 3));
            assert_eq!(c.position() % 4, 0);
        }
    }

    /// No alignment happens unless a decoder asks for it. The string table's first hash
    /// key sits at offset 11 in the observed data, so implicit alignment would skip it.
    #[test]
    fn no_alignment_is_applied_by_ordinary_reads() {
        let buf = [0u8; 32];
        let mut c = Cursor::new(&buf);
        c.u8().unwrap();
        c.u32().unwrap();
        assert_eq!(
            c.position(),
            5,
            "a u32 read at offset 1 must not be padded to offset 4"
        );
        c.u16().unwrap();
        assert_eq!(c.position(), 7);
        c.f64().unwrap();
        assert_eq!(c.position(), 15);
    }

    #[test]
    fn expect_end_reports_the_shortfall() {
        let buf = [0u8; 8];
        let mut c = Cursor::new(&buf);
        c.u32().unwrap();
        match c.expect_end() {
            Err(DatError::Shortfall(4)) => {}
            other => panic!("expected Shortfall(4), got {other:?}"),
        }
    }

    #[test]
    fn reads_past_the_end_error_rather_than_panic() {
        let buf = [0u8; 3];
        let mut c = Cursor::new(&buf);
        match c.u32() {
            Err(DatError::Overrun(1)) => {}
            other => panic!("expected Overrun(1), got {other:?}"),
        }
    }

    /// Oracle: the known-type DataID encoding.
    #[test]
    fn data_id_of_known_type_has_a_two_and_a_four_byte_form() {
        let buf = 0x0123u16.to_le_bytes();
        let mut c = Cursor::new(&buf);
        assert_eq!(
            c.data_id_of_known_type(0x0100_0000).unwrap(),
            DataId(0x0100_0123)
        );

        let mut buf = Vec::new();
        buf.extend_from_slice(&0x8001u16.to_le_bytes()); // hi = 1, escape bit
        buf.extend_from_slice(&0x2345u16.to_le_bytes());
        let mut c = Cursor::new(&buf);
        assert_eq!(
            c.data_id_of_known_type(0x0100_0000).unwrap(),
            DataId(0x0101_2345)
        );
    }
}
