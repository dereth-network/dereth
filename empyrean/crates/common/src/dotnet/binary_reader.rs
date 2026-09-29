//! `System.IO.BinaryReader` over an in-memory payload (`new BinaryReader(new MemoryStream(bytes))`),
//! with the reader's default UTF-8 encoding: the one implementation of the primitive reads that
//! every server crate uses (empyrean-entity's `Unpack` members, empyrean-net's login request, empyrean-world's
//! `ClientMessage.Payload`).
//!
//! Not ported from ACE: a clean-room model of dotnet/runtime
//! `src/libraries/System.Private.CoreLib/src/System/IO/BinaryReader.cs` (net10) and its UTF-8
//! decoder, checked against net10 by the `entity/binary_reader_read_string16l` vectors.
//!
//! * Reads are little-endian. A fixed-size read past the end is .NET's `EndOfStreamException`
//!   ([`ReadError::EndOfStream`]) and leaves the position where it was.
//! * Moving `BaseStream.Position` past the end is allowed; the next read fails.
//! * `ReadBytes(count)` and `ReadChars(count)` never fail at the end of the stream: they return
//!   what is there.
//! * `ReadChars` decodes UTF-8 statefully, as the reader's `Decoder` does: an incomplete sequence
//!   at the end of what was read stays pending in the reader (and is dropped at the end of the
//!   stream); an ill-formed sequence decodes to one U+FFFD per maximal subpart. Each round reads as
//!   many bytes as characters are still wanted (one fewer while a sequence is pending), and when
//!   the decoder yields more UTF-16 units than are left to fill, .NET throws `ArgumentException`
//!   ([`ReadError::OutputBufferTooSmall`]).

use std::fmt;

/// A failed read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// `EndOfStreamException`: a fixed-size read at `at` needed `needed` bytes and `available`
    /// were left.
    EndOfStream {
        at: usize,
        needed: usize,
        available: usize,
    },
    /// `ArgumentException` from `ReadChars`: the UTF-8 decoder produced more UTF-16 units than
    /// were still unfilled (a pending lead byte replaced before, or completed by, the next byte
    /// read). `at` is the stream position after the chunk that overflowed.
    OutputBufferTooSmall { at: usize },
    /// `ArgumentOutOfRangeException` from `ReadChars`: a negative count (a `(int)` cast of a
    /// `uint` above `int.MaxValue`), thrown before anything is read.
    NegativeCount { at: usize },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ReadError::EndOfStream {
                at,
                needed,
                available,
            } => {
                write!(f, "EndOfStreamException: needed {needed} bytes at offset {at}, {available} available")
            }
            ReadError::OutputBufferTooSmall { at } => {
                write!(
                    f,
                    "ArgumentException: ReadChars output buffer too small at offset {at}"
                )
            }
            ReadError::NegativeCount { at } => {
                write!(
                    f,
                    "ArgumentOutOfRangeException: ReadChars count is negative at offset {at}"
                )
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// A `BinaryReader` over a byte slice.
#[derive(Debug, Clone)]
pub struct BinaryReader<'a> {
    buf: &'a [u8],
    pos: usize,
    /// The UTF-8 decoder's pending bytes (an incomplete sequence), kept across `ReadChars` calls.
    pending: Vec<u8>,
}

/// What one byte fed to the decoder produced.
enum Decoded {
    /// Nothing yet: the byte starts or extends a pending sequence.
    Pending,
    /// A scalar value (1 or 2 UTF-16 units); U+FFFD for an invalid lead byte.
    Char(char),
    /// U+FFFD for the pending ill-formed prefix; the byte must be fed again on its own.
    ReplaceAndRetry,
}

impl<'a> BinaryReader<'a> {
    /// A reader at position 0.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        BinaryReader {
            buf,
            pos: 0,
            pending: Vec::new(),
        }
    }

    /// `BaseStream.Position`.
    #[must_use]
    pub fn position(&self) -> usize {
        self.pos
    }

    /// `BaseStream.Position = pos` (may be past the end).
    pub fn set_position(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// `BaseStream.Length`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// `BaseStream.Length == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// `BaseStream.Length - BaseStream.Position`, or 0 past the end.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// The bytes from the position to the end (empty past the end).
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.buf.get(self.pos..).unwrap_or(&[])
    }

    /// `BaseStream.Position += n`.
    pub fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n);
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], ReadError> {
        let available = self.remaining();
        if N > available {
            return Err(ReadError::EndOfStream {
                at: self.pos,
                needed: N,
                available,
            });
        }
        let mut a = [0u8; N];
        a.copy_from_slice(&self.buf[self.pos..self.pos + N]);
        self.pos += N;
        Ok(a)
    }

    /// `ReadByte()`.
    pub fn read_byte(&mut self) -> Result<u8, ReadError> {
        self.take::<1>().map(|b| b[0])
    }

    /// `ReadUInt16()`.
    pub fn read_u16(&mut self) -> Result<u16, ReadError> {
        self.take().map(u16::from_le_bytes)
    }

    /// `ReadUInt32()`.
    pub fn read_u32(&mut self) -> Result<u32, ReadError> {
        self.take().map(u32::from_le_bytes)
    }

    /// `ReadInt32()`.
    pub fn read_i32(&mut self) -> Result<i32, ReadError> {
        self.take().map(i32::from_le_bytes)
    }

    /// `ReadSingle()`.
    pub fn read_f32(&mut self) -> Result<f32, ReadError> {
        self.take().map(f32::from_le_bytes)
    }

    /// `ReadDouble()`.
    pub fn read_f64(&mut self) -> Result<f64, ReadError> {
        self.take().map(f64::from_le_bytes)
    }

    /// `ReadBytes(count)`: what is left when fewer than `count` remain.
    pub fn read_bytes(&mut self, count: usize) -> &'a [u8] {
        let n = count.min(self.remaining());
        let s = if n == 0 {
            &[][..]
        } else {
            &self.buf[self.pos..self.pos + n]
        };
        self.pos += n;
        s
    }

    /// `new string(ReadChars(count))`: up to `count` UTF-16 units, fewer at the end of the stream.
    ///
    /// `BinaryReader.InternalReadChars`: each round reads (MemoryStream `InternalEmulateRead`) as
    /// many bytes as units are still wanted, one fewer when the decoder holds a pending sequence
    /// and more than one is wanted, and decodes them with `flush: false`.
    ///
    /// # Errors
    /// [`ReadError::OutputBufferTooSmall`] where .NET throws `ArgumentException`.
    pub fn read_chars(&mut self, count: usize) -> Result<String, ReadError> {
        let mut out = String::new();
        let mut left = count;
        while left > 0 {
            let mut num_bytes = left;
            if num_bytes > 1 && !self.pending.is_empty() {
                num_bytes -= 1;
            }
            let bytes = self.read_bytes(num_bytes);
            if bytes.is_empty() {
                break;
            }
            let mut produced = 0usize;
            for &b in bytes {
                let mut decoded = self.decode(b);
                loop {
                    let (c, retry) = match decoded {
                        Decoded::Pending => break,
                        Decoded::Char(c) => (c, false),
                        Decoded::ReplaceAndRetry => ('\u{FFFD}', true),
                    };
                    produced += c.len_utf16();
                    if produced > left {
                        return Err(ReadError::OutputBufferTooSmall { at: self.pos });
                    }
                    out.push(c);
                    if !retry {
                        break;
                    }
                    decoded = self.decode(b);
                }
            }
            left -= produced;
        }
        Ok(out)
    }

    /// One byte into the stateful UTF-8 decoder (Unicode "maximal subpart" replacement).
    fn decode(&mut self, b: u8) -> Decoded {
        let Some(&lead) = self.pending.first() else {
            return match b {
                0x00..=0x7F => Decoded::Char(char::from(b)),
                0xC2..=0xF4 => {
                    self.pending.push(b);
                    Decoded::Pending
                }
                _ => Decoded::Char('\u{FFFD}'),
            };
        };
        // The range the first continuation byte must fall in, and the sequence length.
        let (lo, hi, width) = match lead {
            0xC2..=0xDF => (0x80, 0xBF, 2),
            0xE0 => (0xA0, 0xBF, 3),
            0xED => (0x80, 0x9F, 3),
            0xE1..=0xEF => (0x80, 0xBF, 3),
            0xF0 => (0x90, 0xBF, 4),
            0xF4 => (0x80, 0x8F, 4),
            _ => (0x80, 0xBF, 4),
        };
        let (lo, hi) = if self.pending.len() == 1 {
            (lo, hi)
        } else {
            (0x80, 0xBF)
        };
        if !(lo..=hi).contains(&b) {
            self.pending.clear();
            return Decoded::ReplaceAndRetry;
        }
        self.pending.push(b);
        if self.pending.len() < width {
            return Decoded::Pending;
        }
        // The ranges above admit only well-formed sequences.
        let c = std::str::from_utf8(&self.pending)
            .ok()
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{FFFD}');
        self.pending.clear();
        Decoded::Char(c)
    }
}
