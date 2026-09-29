//! The bounds-checked little-endian cursor every pack read goes through, its write side, and the
//! [`Codec`] every record type implements.
//!
//! Carried from the v1 server. There are no `repr(C)` casts and no `bytemuck`: a malformed pack can
//! only ever produce an error, never a misread struct.

use std::sync::Arc;

use empyrean_common::dotnet::DotNetDateTime;

use crate::error::PackError;

/// A byte cursor over one record's bytes. Every read is unaligned little-endian and bounds-checked.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

macro_rules! read_le {
    ($name:ident, $ty:ty, $n:literal) => {
        #[doc = concat!("One little-endian `", stringify!($ty), "`.")]
        #[inline]
        pub fn $name(&mut self) -> Result<$ty, PackError> {
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

    read_le!(u8, u8, 1);
    read_le!(i8, i8, 1);
    read_le!(u16, u16, 2);
    read_le!(i16, i16, 2);
    read_le!(u32, u32, 4);
    read_le!(i32, i32, 4);
    read_le!(u64, u64, 8);
    read_le!(i64, i64, 8);
    read_le!(f32, f32, 4);
    read_le!(f64, f64, 8);

    /// `n` raw bytes.
    #[inline]
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], PackError> {
        let end = self.pos.checked_add(n).ok_or(PackError::Overrun(n))?;
        if end > self.buf.len() {
            return Err(PackError::Overrun(end - self.buf.len()));
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    /// The invariant every record read ends with: the cursor landed exactly on the record end.
    pub fn expect_end(&self) -> Result<(), PackError> {
        match self.buf.len().cmp(&self.pos) {
            core::cmp::Ordering::Equal => Ok(()),
            core::cmp::Ordering::Greater => Err(PackError::Shortfall(self.buf.len() - self.pos)),
            core::cmp::Ordering::Less => Err(PackError::Overrun(self.pos - self.buf.len())),
        }
    }

    #[inline]
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }
}

/// The write half, as an extension trait on `Vec<u8>`.
pub trait PackWrite {
    fn put_u8(&mut self, v: u8);
    fn put_u16(&mut self, v: u16);
    fn put_u32(&mut self, v: u32);
    fn put_u64(&mut self, v: u64);
}

impl PackWrite for Vec<u8> {
    fn put_u8(&mut self, v: u8) {
        self.push(v);
    }
    fn put_u16(&mut self, v: u16) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u32(&mut self, v: u32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn put_u64(&mut self, v: u64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
}

/// A value with a fixed binary layout in a pack record. `put` and `get` are exact inverses; field
/// order is the format.
pub trait Codec: Sized {
    fn put(&self, w: &mut Vec<u8>);
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError>;
}

macro_rules! codec_le {
    ($($ty:ty => $read:ident),* $(,)?) => {$(
        impl Codec for $ty {
            #[inline]
            fn put(&self, w: &mut Vec<u8>) {
                w.extend_from_slice(&self.to_le_bytes());
            }
            #[inline]
            fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
                c.$read()
            }
        }
    )*};
}

codec_le!(u8 => u8, i8 => i8, u16 => u16, i16 => i16, u32 => u32, i32 => i32, u64 => u64, i64 => i64, f32 => f32, f64 => f64);

impl Codec for bool {
    fn put(&self, w: &mut Vec<u8>) {
        w.push(u8::from(*self));
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        Ok(c.u8()? != 0)
    }
}

/// `u32` byte length, then UTF-8 bytes.
impl Codec for String {
    fn put(&self, w: &mut Vec<u8>) {
        put_len(w, self.len());
        w.extend_from_slice(self.as_bytes());
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        let n = c.u32()? as usize;
        let b = c.bytes(n)?;
        core::str::from_utf8(b)
            .map(str::to_owned)
            .map_err(|_| PackError::BadUtf8(n))
    }
}

/// A presence byte, then the value.
impl<T: Codec> Codec for Option<T> {
    fn put(&self, w: &mut Vec<u8>) {
        match self {
            None => w.push(0),
            Some(v) => {
                w.push(1);
                v.put(w);
            }
        }
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        match c.u8()? {
            0 => Ok(None),
            _ => T::get(c).map(Some),
        }
    }
}

/// `u32` element count, then the elements.
impl<T: Codec> Codec for Vec<T> {
    fn put(&self, w: &mut Vec<u8>) {
        put_len(w, self.len());
        for v in self {
            v.put(w);
        }
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        let n = c.u32()? as usize;
        // Never trust a count for a pre-allocation beyond what the remaining bytes could hold.
        let mut out = Vec::with_capacity(n.min(c.buf.len().saturating_sub(c.pos)));
        for _ in 0..n {
            out.push(T::get(c)?);
        }
        Ok(out)
    }
}

impl<T: Codec> Codec for Arc<T> {
    fn put(&self, w: &mut Vec<u8>) {
        (**self).put(w);
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        T::get(c).map(Arc::new)
    }
}

/// .NET ticks.
impl Codec for DotNetDateTime {
    fn put(&self, w: &mut Vec<u8>) {
        self.ticks().put(w);
    }
    fn get(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        let t = c.i64()?;
        if !(0..=DotNetDateTime::MAX_VALUE.ticks()).contains(&t) {
            return Err(PackError::HeaderField {
                field: "DateTime ticks",
                expected: 0,
                found: t as u64,
            });
        }
        Ok(DotNetDateTime::from_ticks(t))
    }
}

fn put_len(w: &mut Vec<u8>, n: usize) {
    // A record is capped at MAX_RECORD_LEN (64 MiB) by the writer, so any length fits.
    w.put_u32(u32::try_from(n).unwrap_or(u32::MAX));
}

/// Decode a whole record: the value, then [`Cursor::expect_end`].
pub fn decode<T: Codec>(bytes: &[u8]) -> Result<T, PackError> {
    let mut c = Cursor::new(bytes);
    let v = T::get(&mut c)?;
    c.expect_end()?;
    Ok(v)
}

/// Encode a whole record.
#[must_use]
pub fn encode<T: Codec>(v: &T) -> Vec<u8> {
    let mut w = Vec::new();
    v.put(&mut w);
    w
}

/// Implements [`Codec`] for a struct by listing its fields in format order.
#[macro_export]
#[doc(hidden)]
macro_rules! impl_codec {
    ($ty:ty { $($field:ident),* $(,)? }) => {
        impl $crate::pack::Codec for $ty {
            fn put(&self, w: &mut Vec<u8>) {
                $( $crate::pack::Codec::put(&self.$field, w); )*
            }
            fn get(c: &mut $crate::pack::Cursor<'_>) -> Result<Self, $crate::error::PackError> {
                Ok(Self { $( $field: $crate::pack::Codec::get(c)?, )* })
            }
        }
    };
}
