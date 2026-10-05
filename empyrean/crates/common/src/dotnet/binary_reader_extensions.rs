// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/BinaryReaderExtensions.cs
//! `ACE.Common.Extensions.BinaryReaderExtensions`, as inherent methods of the shared
//! [`BinaryReader`]. They sit next to the reader they extend rather than in `crate::extensions`,
//! where ACE's layout would put them.

use super::binary_reader::{BinaryReader, ReadError};

// ACE: BinaryReaderExtensions.CalculatePadMultiple
fn calculate_pad_multiple(length: u32, multiple: u32) -> u32 {
    multiple
        .wrapping_mul(length.wrapping_add(multiple).wrapping_sub(1) / multiple)
        .wrapping_sub(length)
}

impl BinaryReader<'_> {
    // ACE: BinaryReaderExtensions.Skip
    /// `BaseStream.Position += length`; never fails, may pass the end.
    pub fn skip_length(&mut self, length: u32) {
        self.skip(length as usize);
    }

    // ACE: BinaryReaderExtensions.ReadString32L
    /// The login header's string: a `u32` byte count, then the client's compressed length (one byte
    /// below 0x80, two below 0x4000, else four), then that many Windows-1252 bytes, then padding of
    /// the whole field to a multiple of 4.
    ///
    /// **Retail's reading, not ACE's.** ACE skipped one length byte, or
    /// two from 256 characters up, where the client's two-byte form starts at 128, so a string of
    /// 128–254 characters kept a stray leading character; and it decoded the text as UTF-8.
    ///
    /// # Errors
    /// [`ReadError::EndOfStream`] when the count, the length or the text runs past the end.
    pub fn read_string32l(&mut self) -> Result<String, ReadError> {
        let length = self.read_u32()?;
        if length == 0 {
            return Ok(String::new());
        }
        let available = self.remaining();
        if length as usize > available {
            return Err(ReadError::EndOfStream {
                at: self.position(),
                needed: length as usize,
                available,
            });
        }
        let start = self.position();
        let b0 = self.read_byte()?;
        let text_len = if b0 & 0x80 == 0 {
            u32::from(b0)
        } else {
            let b1 = self.read_byte()?;
            if b0 & 0x40 == 0 {
                (u32::from(b0 & 0x7F) << 8) | u32::from(b1)
            } else {
                let tail = self.read_u16()?;
                (((u32::from(b0 & 0x3F) << 8) | u32::from(b1)) << 16) | u32::from(tail)
            }
        };
        let text = self.read_cp1252(text_len as usize)?;
        // The field is `length` bytes from `start`; pad it to a multiple of 4, as ACE did.
        let used = u32::try_from(self.position() - start).unwrap_or(u32::MAX);
        self.skip_length(length.saturating_sub(used));
        self.skip_length(calculate_pad_multiple(4u32.wrapping_add(length), 4));
        Ok(text)
    }

    // ACE: BinaryReaderExtensions.ReadString16L
    /// The client's packed string: a `u16` **byte** count (0xFFFF then a `u32` for a long one), that
    /// many Windows-1252 bytes, then padding to the next multiple of 4 of the message position.
    ///
    /// **Retail's reading, not ACE's.** ACE read a *character* count and
    /// decoded the bytes as UTF-8, so every non-ASCII character a player typed (é, ñ, the curly
    /// quotes) came out as U+FFFD, a byte run that happened to be valid UTF-8 consumed more bytes
    /// than counted and misaligned what followed, and the long form was not recognised. The
    /// decoding is `dereth_protocol::cp1252`, the client's own table.
    ///
    /// # Errors
    /// [`ReadError::EndOfStream`] when the count or the text runs past the end.
    pub fn read_string16l(&mut self) -> Result<String, ReadError> {
        let short = self.read_u16()?;
        let length = if short == 0xFFFF {
            self.read_u32()? as usize
        } else {
            usize::from(short)
        };
        let mut text = self.read_cp1252(length)?;
        // A packed terminator is not text: the client drops a trailing NUL too.
        if text.ends_with('\0') {
            text.pop();
        }
        let pad = self.position().wrapping_neg() & 3;
        self.skip(pad);
        Ok(text)
    }

    /// `length` Windows-1252 bytes as text.
    fn read_cp1252(&mut self, length: usize) -> Result<String, ReadError> {
        let available = self.remaining();
        if length > available {
            return Err(ReadError::EndOfStream {
                at: self.position(),
                needed: length,
                available,
            });
        }
        Ok(dereth_primitives::text::cp1252::decode(
            self.read_bytes(length),
        ))
    }
}
