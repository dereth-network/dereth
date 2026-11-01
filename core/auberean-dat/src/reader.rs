//! A little-endian reader over one record's bytes: the primitives every record is built from.
//!
//! Every read is bounds-checked and fails with the offset it started at, so a decoder that runs
//! off the end, or a check that fails, says where. Records are read the way they were written:
//! values packed with no implicit padding, and explicit alignment ([`Reader::align`]) to four
//! bytes **counted from the start of the record**, where the writer asked for it.
//!
//! Decoders end with [`Reader::finish`]: a record decodes only when every byte was consumed.

use auberean_primitives::{scramble, BBox, DataId, Frame, Quat, Rgba, Sphere, Vec3};

use crate::error::DecodeError;

/// What every read returns.
pub type Result<T> = std::result::Result<T, DecodeError>;

/// A cursor over one record.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The offset of the next read, from the start of the record.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The record's length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Bytes not read yet.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    #[must_use]
    pub fn at_end(&self) -> bool {
        self.pos == self.buf.len()
    }

    /// The bytes not read yet, without consuming them.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// A failure at the current offset.
    pub fn fail<T>(&self, what: impl Into<String>) -> Result<T> {
        Err(DecodeError {
            at: self.pos,
            what: what.into(),
        })
    }

    /// Fail with `what()` at the current offset unless `cond` holds.
    pub fn ensure(&self, cond: bool, what: impl FnOnce() -> String) -> Result<()> {
        if cond {
            Ok(())
        } else {
            self.fail(what())
        }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return self.fail(format!("wants {n} bytes, {} left", self.remaining()));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.bytes(N)?);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    // D2-07315
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    /// A 32-bit truth value: any non-zero word is true, as the client reads it.
    pub fn bool32(&mut self) -> Result<bool> {
        Ok(self.u32()? != 0)
    }

    /// A record id stored as a 32-bit word.
    pub fn data_id(&mut self) -> Result<DataId> {
        Ok(DataId(self.u32()?))
    }

    /// A count that must fit in what is left at `min_each` bytes an item, so a bad count fails
    /// here instead of allocating.
    pub fn count(&mut self, min_each: usize) -> Result<usize> {
        let at = self.pos;
        let n = self.u32()? as usize;
        self.check_count(at, n, min_each)
    }

    /// A count stored in 16 bits, bounded like [`Reader::count`].
    pub fn count16(&mut self, min_each: usize) -> Result<usize> {
        let at = self.pos;
        let n = usize::from(self.u16()?);
        self.check_count(at, n, min_each)
    }

    fn check_count(&self, at: usize, n: usize, min_each: usize) -> Result<usize> {
        if n.saturating_mul(min_each) > self.remaining() {
            return Err(DecodeError {
                at,
                what: format!("count {n} x {min_each} exceeds {} left", self.remaining()),
            });
        }
        Ok(n)
    }

    /// A counted list: a 32-bit count, then the items, each read by `item`.
    pub fn list<T>(
        &mut self,
        min_each: usize,
        mut item: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let n = self.count(min_each)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(item(self)?);
        }
        Ok(v)
    }

    /// A list of 32-bit words with a 32-bit count.
    pub fn u32_list(&mut self) -> Result<Vec<u32>> {
        self.list(4, Self::u32)
    }

    /// A hash table: a header word whose low 24 bits are the entry count (the top byte sizes the
    /// table's buckets, which a reader does not need), then the entries, each read by `entry`.
    /// Entries come back in stored order.
    pub fn hash_table<T>(
        &mut self,
        min_each: usize,
        mut entry: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Vec<T>> {
        let at = self.pos;
        let n = (self.u32()? & 0x00FF_FFFF) as usize;
        let n = self.check_count(at, n, min_each)?;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(entry(self)?);
        }
        Ok(v)
    }

    /// A hash table and the bucket-size index it was written with (its header's top byte), which
    /// writing it back exactly needs.
    pub fn hash_table_with_buckets<T>(
        &mut self,
        min_each: usize,
        entry: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<(u8, Vec<T>)> {
        let buckets = self.rest().get(3).copied().unwrap_or(0);
        let v = self.hash_table(min_each, entry)?;
        Ok((buckets, v))
    }

    /// The archive's compressed 32-bit value: one byte below 0x80, two bytes below 0x4000 (the top
    /// bit of the first byte set), else four bytes (the top two bits set; the low 30 bits are the
    /// value, the first two bytes its high half).
    pub fn compressed_u32(&mut self) -> Result<u32> {
        let b0 = u32::from(self.u8()?);
        if b0 & 0x80 == 0 {
            return Ok(b0);
        }
        let b1 = u32::from(self.u8()?);
        if b0 & 0x40 == 0 {
            return Ok(((b0 & 0x7F) << 8) | b1);
        }
        let lo = u32::from(self.u16()?);
        Ok(((((b0 & 0x3F) << 8) | b1) << 16) | lo)
    }

    /// Skip to the next multiple of four bytes from the start of the record.
    // D2-03470
    pub fn align(&mut self) -> Result<()> {
        let pad = (4 - self.pos % 4) % 4;
        self.skip(pad)
    }

    /// The length prefix strings use: 16 bits, or `0xFFFF` and then 32 bits.
    fn string_length(&mut self) -> Result<usize> {
        let n = self.u16()?;
        Ok(if n == 0xFFFF {
            self.u32()? as usize
        } else {
            usize::from(n)
        })
    }

    /// A narrow string: its length, its scrambled bytes, then alignment to four.
    // D2-00700
    pub fn narrow_string(&mut self) -> Result<String> {
        let n = self.string_length()?;
        let stored = self.bytes(n)?;
        let s = scramble::reveal_narrow(stored);
        self.align()?;
        Ok(s)
    }

    /// A wide string: its length in UTF-16 units, its scrambled units, then alignment to four.
    // D2-00380
    pub fn wide_string(&mut self) -> Result<String> {
        let n = self.string_length()?;
        let stored = self.bytes(n.saturating_mul(2))?;
        let s = scramble::reveal_wide(stored);
        self.align()?;
        Ok(s)
    }

    // D2-06731
    pub fn vec3(&mut self) -> Result<Vec3> {
        Ok(Vec3 {
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    /// A rotation, scalar first.
    // D2-13258
    pub fn quat(&mut self) -> Result<Quat> {
        Ok(Quat {
            w: self.f32()?,
            x: self.f32()?,
            y: self.f32()?,
            z: self.f32()?,
        })
    }

    /// A frame: an origin, then a rotation.
    // D2-13158
    pub fn frame(&mut self) -> Result<Frame> {
        Ok(Frame {
            origin: self.vec3()?,
            rotation: self.quat()?,
        })
    }

    /// A box: least corner, then greatest.
    pub fn bbox(&mut self) -> Result<BBox> {
        Ok(BBox {
            min: self.vec3()?,
            max: self.vec3()?,
        })
    }

    /// A sphere: center, then radius.
    pub fn sphere(&mut self) -> Result<Sphere> {
        Ok(Sphere {
            center: self.vec3()?,
            radius: self.f32()?,
        })
    }

    /// A color as four floats, red first.
    pub fn rgba(&mut self) -> Result<Rgba> {
        Ok(Rgba {
            r: self.f32()?,
            g: self.f32()?,
            b: self.f32()?,
            a: self.f32()?,
        })
    }

    /// A record's leading id, which must be the id the record is filed under.
    pub fn expect_id(&mut self, id: u32) -> Result<()> {
        let at = self.pos;
        let got = self.u32()?;
        if got != id {
            return Err(DecodeError {
                at,
                what: format!("id {got:08X}, filed as {id:08X}"),
            });
        }
        Ok(())
    }

    /// The end check: every byte consumed.
    pub fn finish(&self) -> Result<()> {
        if self.at_end() {
            Ok(())
        } else {
            self.fail(format!("{} bytes left over", self.remaining()))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the reader's primitives)
    use super::*;

    #[test]
    fn a_compressed_value_takes_one_two_or_four_bytes() {
        let mut r = Reader::new(&[0x7F, 0x81, 0x02, 0xC1, 0x02, 0x04, 0x03]);
        assert_eq!(r.compressed_u32().unwrap(), 0x7F);
        assert_eq!(r.compressed_u32().unwrap(), 0x0102);
        assert_eq!(r.compressed_u32().unwrap(), 0x0102_0304);
        assert!(r.at_end());
    }

    #[test]
    fn alignment_counts_from_the_start_of_the_record() {
        let mut r = Reader::new(&[1, 2, 3, 4, 5, 6, 7, 8]);
        r.u8().unwrap();
        r.align().unwrap();
        assert_eq!(r.pos(), 4);
        r.align().unwrap();
        assert_eq!(r.pos(), 4);
    }

    #[test]
    fn a_narrow_string_is_length_scrambled_bytes_and_padding() {
        let stored = scramble::conceal(b"abcde");
        let mut b = vec![5, 0];
        b.extend_from_slice(&stored);
        b.push(0);
        let mut r = Reader::new(&b);
        assert_eq!(r.narrow_string().unwrap(), "abcde");
        r.finish().unwrap();
    }

    #[test]
    fn a_long_string_takes_a_32_bit_length_after_ffff() {
        let text = vec![b'x'; 70_000];
        let mut b = vec![0xFF, 0xFF];
        b.extend_from_slice(&70_000u32.to_le_bytes());
        b.extend_from_slice(&scramble::conceal(&text));
        b.extend_from_slice(&[0, 0]);
        let mut r = Reader::new(&b);
        assert_eq!(r.narrow_string().unwrap().len(), 70_000);
        r.finish().unwrap();
    }

    #[test]
    fn a_count_larger_than_what_is_left_fails_where_it_was_read() {
        let mut r = Reader::new(&[0, 0, 0, 0, 0xFF, 0xFF, 0, 0]);
        r.u32().unwrap();
        let e = r.count(4).unwrap_err();
        assert_eq!(e.at, 4);
    }

    #[test]
    fn a_hash_table_header_counts_in_its_low_24_bits() {
        let mut b = 0x0300_0002u32.to_le_bytes().to_vec();
        for v in [1u32, 2] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let mut r = Reader::new(&b);
        assert_eq!(r.hash_table(4, Reader::u32).unwrap(), vec![1, 2]);
        r.finish().unwrap();
    }

    #[test]
    fn finishing_with_bytes_left_fails() {
        let mut r = Reader::new(&[1, 2, 3, 4, 5]);
        r.u32().unwrap();
        assert_eq!(r.finish().unwrap_err().what, "1 bytes left over");
    }

    #[test]
    fn a_record_filed_under_another_id_fails() {
        let b = 0x0600_0001u32.to_le_bytes();
        let mut r = Reader::new(&b);
        assert!(r.expect_id(0x0600_0002).is_err());
    }
}
