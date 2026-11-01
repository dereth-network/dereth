//! Reading and writing packed script data: little-endian words, scrambled strings, alignment to
//! four bytes from the start of the buffer.
//!
//! Strings are a 16-bit length (`0xFFFF` then a 32-bit length when longer), the units scrambled
//! as every stored string is, then padding to four bytes. Wide strings are kept as their UTF-16
//! units, so a string reads back to exactly the bytes it came from.

use auberean_primitives::scramble;

/// Why packed data could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodecError {
    /// The byte offset where the failing read started.
    pub at: usize,
    pub what: String,
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "@{:#x}: {}", self.at, self.what)
    }
}

impl std::error::Error for CodecError {}

pub type Result<T> = core::result::Result<T, CodecError>;

/// A cursor over packed bytes. Alignment counts from the start of the slice.
#[derive(Clone, Debug)]
pub struct In<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> In<'a> {
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    #[must_use]
    pub fn at_end(&self) -> bool {
        self.pos == self.buf.len()
    }

    pub fn fail<T>(&self, what: impl Into<String>) -> Result<T> {
        Err(CodecError {
            at: self.pos,
            what: what.into(),
        })
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return self.fail(format!("{n} bytes wanted, {} left", self.remaining()));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64(&mut self) -> Result<u64> {
        let lo = self.u32()?;
        let hi = self.u32()?;
        Ok(crate::value::join(lo, hi))
    }

    /// A count of items at least `min_each` bytes each, refused when it cannot fit.
    pub fn count(&mut self, n: usize, min_each: usize) -> Result<usize> {
        if n.saturating_mul(min_each) > self.remaining() {
            return self.fail(format!(
                "{n} items of {min_each} bytes exceed {} left",
                self.remaining()
            ));
        }
        Ok(n)
    }

    /// Skip to the next multiple of four from the start.
    pub fn align(&mut self) -> Result<()> {
        let pad = (4 - self.pos % 4) % 4;
        self.bytes(pad).map(|_| ())
    }

    /// A wide string: its units, unscrambled.
    pub fn wide(&mut self) -> Result<Vec<u16>> {
        let n = self.u16()?;
        let n = if n == 0xFFFF {
            self.u32()? as usize
        } else {
            usize::from(n)
        };
        let stored = self.bytes(n.saturating_mul(2))?;
        let plain = scramble::reveal(stored);
        let units = plain
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        self.align()?;
        Ok(units)
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

/// Packed bytes being written. Alignment counts from the start of the buffer.
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub buf: Vec<u8>,
}

impl Out {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        let (lo, hi) = crate::value::split(v);
        self.u32(lo);
        self.u32(hi);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// Zero padding to the next multiple of four.
    pub fn align(&mut self) {
        while !self.buf.len().is_multiple_of(4) {
            self.buf.push(0);
        }
    }

    /// A wide string, scrambled, with its length and padding.
    pub fn wide(&mut self, units: &[u16]) {
        match u16::try_from(units.len()) {
            Ok(n) if n != 0xFFFF => self.u16(n),
            _ => {
                self.u16(0xFFFF);
                #[allow(clippy::cast_possible_truncation)]
                self.u32(units.len() as u32);
            }
        }
        let plain: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        self.bytes(&scramble::conceal(&plain));
        self.align();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (byte helpers)
    use super::*;

    #[test]
    fn a_wide_string_reads_back_to_its_units_and_its_bytes() {
        let units: Vec<u16> = "Tumerok\u{e9}".encode_utf16().collect();
        let mut o = Out::new();
        o.u16(7);
        o.align();
        o.wide(&units);
        assert_eq!(o.buf.len() % 4, 0);
        let mut i = In::new(&o.buf);
        assert_eq!(i.u16().unwrap(), 7);
        i.align().unwrap();
        assert_eq!(i.wide().unwrap(), units);
        assert!(i.at_end());
    }
}
