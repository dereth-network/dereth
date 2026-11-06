//! The inverse of [`Reader`](crate::Reader): writes a record the way the client packs one, so a
//! decoded record written back gives the bytes it was read from.
//!
//! Values are packed with no implicit padding; [`Writer::align`] pads with zeros to four bytes
//! **counted from the start of the record**, where the writer asks for it, exactly where the
//! reader skips. Strings are stored scrambled, as the reader expects them.

use auberean_primitives::{scramble, BBox, DataId, Frame, Quat, Rgba, Sphere, Vec3};

/// A growing record.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The offset of the next write, from the start of the record.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.buf.len()
    }

    /// The record's bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.buf
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.bytes(&v.to_le_bytes());
    }

    /// A truth value as a 32-bit word: 1 or 0.
    pub fn bool32(&mut self, v: bool) {
        self.u32(u32::from(v));
    }

    pub fn data_id(&mut self, v: DataId) {
        self.u32(v.0);
    }

    /// A 32-bit count.
    pub fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    /// A counted list: the count, then each item.
    pub fn list<T>(&mut self, items: &[T], mut item: impl FnMut(&mut Self, &T)) {
        self.count(items.len());
        for x in items {
            item(self, x);
        }
    }

    /// A hash table: its header word (the entry count in the low 24 bits, the bucket-size index
    /// the table was written with in the top byte), then each entry.
    pub fn hash_table<T>(
        &mut self,
        buckets: u8,
        items: &[T],
        mut entry: impl FnMut(&mut Self, &T),
    ) {
        let n = u32::try_from(items.len()).unwrap_or(0x00FF_FFFF) & 0x00FF_FFFF;
        self.u32(n | (u32::from(buckets) << 24));
        for x in items {
            entry(self, x);
        }
    }

    /// The archive's compressed 32-bit value (see [`Reader::compressed_u32`](crate::Reader::compressed_u32)).
    #[allow(clippy::cast_possible_truncation)]
    pub fn compressed_u32(&mut self, v: u32) {
        if v < 0x80 {
            self.u8(v as u8);
        } else if v < 0x4000 {
            self.u8(((v >> 8) as u8) | 0x80);
            self.u8(v as u8);
        } else {
            let hi = (v >> 16) & 0x3FFF;
            self.u8(((hi >> 8) as u8) | 0xC0);
            self.u8(hi as u8);
            self.u16(v as u16);
        }
    }

    /// Zeros to the next multiple of four bytes from the start of the record.
    // D2-03470, D2-00237
    pub fn align(&mut self) {
        while !self.buf.len().is_multiple_of(4) {
            self.buf.push(0);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn string_length(&mut self, n: usize) {
        if n < 0xFFFF {
            self.u16(n as u16);
        } else {
            self.u16(0xFFFF);
            self.u32(u32::try_from(n).unwrap_or(u32::MAX));
        }
    }

    /// A narrow string: its length, its scrambled bytes, then alignment. Characters above U+00FF
    /// cannot be stored narrow and are written as `?`.
    // D2-00700
    pub fn narrow_string(&mut self, s: &str) {
        let plain: Vec<u8> = s
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect();
        self.string_length(plain.len());
        self.bytes(&scramble::conceal(&plain));
        self.align();
    }

    /// A wide string: its length in UTF-16 units, its scrambled units, then alignment.
    // D2-00380
    pub fn wide_string(&mut self, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.wide_units(&units);
    }

    /// A wide string from its UTF-16 units as stored (unpaired surrogates included).
    pub fn wide_units(&mut self, units: &[u16]) {
        let plain: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        self.string_length(units.len());
        self.bytes(&scramble::conceal(&plain));
        self.align();
    }

    pub fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    /// A rotation, scalar first.
    pub fn quat(&mut self, q: Quat) {
        self.f32(q.w);
        self.f32(q.x);
        self.f32(q.y);
        self.f32(q.z);
    }

    /// A frame: an origin, then a rotation.
    pub fn frame(&mut self, f: Frame) {
        self.vec3(f.origin);
        self.quat(f.rotation);
    }

    pub fn bbox(&mut self, b: BBox) {
        self.vec3(b.min);
        self.vec3(b.max);
    }

    pub fn sphere(&mut self, s: Sphere) {
        self.vec3(s.center);
        self.f32(s.radius);
    }

    pub fn rgba(&mut self, c: Rgba) {
        self.f32(c.r);
        self.f32(c.g);
        self.f32(c.b);
        self.f32(c.a);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the writer's own round trip of primitives)
    use super::*;
    use crate::Reader;

    #[test]
    fn strings_and_alignment_read_back_as_written() {
        let mut w = Writer::new();
        w.u8(7);
        w.align();
        w.narrow_string("Arwic");
        w.wide_string("Ellaria");
        w.compressed_u32(0x7F);
        w.compressed_u32(0x3FFF);
        w.compressed_u32(0x1234_5678);
        let b = w.finish();
        let mut r = Reader::new(&b);
        assert_eq!(r.u8().unwrap(), 7);
        r.align().unwrap();
        assert_eq!(r.narrow_string().unwrap(), "Arwic");
        assert_eq!(r.wide_string().unwrap(), "Ellaria");
        assert_eq!(r.compressed_u32().unwrap(), 0x7F);
        assert_eq!(r.compressed_u32().unwrap(), 0x3FFF);
        assert_eq!(r.compressed_u32().unwrap(), 0x1234_5678);
        r.finish().unwrap();
    }
}
