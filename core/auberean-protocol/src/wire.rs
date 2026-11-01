//! The byte level every message is read and written at: little-endian fields, padding to four
//! bytes counted from the start of the message, and the error a read ends in.
//!
//! A [`Reader`] is made over the whole message, its type word included, so that its position is
//! the offset from the message start and [`Reader::align`] pads exactly as the sender did. A
//! [`Writer`] does the same in reverse and pads with zeros.

use alloc::vec::Vec;
use core::fmt;

use auberean_primitives::Vec3;

/// Why a message did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The message ended inside a field.
    Truncated { offset: usize },
    /// Bytes were left after the last field.
    Trailing { offset: usize, left: usize },
    /// A field held a value no sender writes (a count past any real one, an unknown flag).
    Invalid { offset: usize, what: &'static str },
    /// No message of this type travels in this direction.
    UnknownType(u32),
}

impl DecodeError {
    /// Where in the message the read failed (0 for an unknown type).
    #[must_use]
    pub const fn offset(&self) -> usize {
        match self {
            Self::Truncated { offset }
            | Self::Trailing { offset, .. }
            | Self::Invalid { offset, .. } => *offset,
            Self::UnknownType(_) => 0,
        }
    }

    /// A short name for the kind of failure, for counting failures by kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Truncated { .. } => "truncated",
            Self::Trailing { .. } => "trailing",
            Self::Invalid { what, .. } => what,
            Self::UnknownType(_) => "unknown type",
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { offset } => write!(f, "truncated at {offset}"),
            Self::Trailing { offset, left } => write!(f, "{left} bytes left at {offset}"),
            Self::Invalid { offset, what } => write!(f, "invalid {what} at {offset}"),
            Self::UnknownType(t) => write!(f, "no message of type {t:#x} in this direction"),
        }
    }
}

/// The result of a read.
pub type Result<T> = core::result::Result<T, DecodeError>;

/// The most entries any counted list in a message holds; a larger count is a corrupt message.
pub const MAX_COUNT: usize = 0x0010_0000;

/// A bounds-checked cursor over one message.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start of a whole message.
    #[must_use]
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// The offset from the message start.
    #[must_use]
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Bytes not read yet.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// A failure at the current position.
    #[must_use]
    pub const fn invalid(&self, what: &'static str) -> DecodeError {
        DecodeError::Invalid {
            offset: self.pos,
            what,
        }
    }

    /// The next `n` bytes.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.buf.len());
        let Some(end) = end else {
            return Err(DecodeError::Truncated { offset: self.pos });
        };
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self.bytes(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(b);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    pub fn vec3(&mut self) -> Result<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }

    /// A `u32` count of what follows, refused past [`MAX_COUNT`].
    pub fn count(&mut self) -> Result<usize> {
        let n = self.u32()? as usize;
        if n > MAX_COUNT {
            return Err(DecodeError::Invalid {
                offset: self.pos - 4,
                what: "count",
            });
        }
        Ok(n)
    }

    /// Skip to the next multiple of four from the message start. The skipped bytes are not
    /// checked: a sender may leave anything in them.
    pub fn align(&mut self) -> Result<()> {
        let pad = (4 - self.pos % 4) % 4;
        self.bytes(pad).map(|_| ())
    }

    /// Everything left.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    /// The exact-consumption check: nothing may be left.
    pub fn finish(&self) -> Result<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(DecodeError::Trailing {
                offset: self.pos,
                left: self.buf.len() - self.pos,
            })
        }
    }
}

/// Builds one message.
#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// A message starting with its type word.
    #[must_use]
    pub fn message(message_type: u32) -> Self {
        let mut w = Self {
            buf: Vec::with_capacity(64),
        };
        w.u32(message_type);
        w
    }

    /// The offset from the message start.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.buf.len()
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

    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn vec3(&mut self, v: Vec3) {
        self.f32(v.x);
        self.f32(v.y);
        self.f32(v.z);
    }

    /// A count as a `u32`.
    pub fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    /// Pad with zeros to the next multiple of four from the message start.
    pub fn align(&mut self) {
        while !self.buf.len().is_multiple_of(4) {
            self.buf.push(0);
        }
    }

    /// The message.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// A value read and written the same way wherever it appears in a message.
pub trait Field: Sized {
    fn read(r: &mut Reader<'_>) -> Result<Self>;
    fn write(&self, w: &mut Writer);
}

impl Field for u32 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.u32()
    }
    fn write(&self, w: &mut Writer) {
        w.u32(*self);
    }
}

impl Field for i32 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.i32()
    }
    fn write(&self, w: &mut Writer) {
        w.i32(*self);
    }
}

impl Field for u64 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.u64()
    }
    fn write(&self, w: &mut Writer) {
        w.u64(*self);
    }
}

impl Field for f32 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.f32()
    }
    fn write(&self, w: &mut Writer) {
        w.f32(*self);
    }
}

impl Field for f64 {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.f64()
    }
    fn write(&self, w: &mut Writer) {
        w.f64(*self);
    }
}

/// A `u32` count, then that many values.
pub fn read_list<T: Field>(r: &mut Reader<'_>) -> Result<Vec<T>> {
    let n = r.count()?;
    let mut v = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        v.push(T::read(r)?);
    }
    Ok(v)
}

/// The inverse of [`read_list`].
pub fn write_list<T: Field>(w: &mut Writer, items: &[T]) {
    w.count(items.len());
    for i in items {
        i.write(w);
    }
}
