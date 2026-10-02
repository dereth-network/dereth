//! A bounded little-endian cursor over one record, shared by the table and graphics decoders.

pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Move back to an earlier position, to read the same bytes as another shape.
    pub(crate) fn rewind(&mut self, pos: usize) {
        self.pos = pos.min(self.pos);
    }

    /// The bytes from `start` to the current position.
    pub(crate) fn since(&self, start: usize) -> &'a [u8] {
        &self.data[start.min(self.pos)..self.pos]
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn consume(&mut self, size: usize) -> Result<&'a [u8], String> {
        if size > self.remaining() {
            return Err(format!(
                "offset {}: need {size} bytes, have {}",
                self.pos,
                self.remaining()
            ));
        }
        let out = &self.data[self.pos..self.pos + size];
        self.pos += size;
        Ok(out)
    }

    /// `size` bytes of field data.
    pub(crate) fn take(&mut self, size: usize) -> Result<&'a [u8], String> {
        self.consume(size)
    }

    /// `count * each` bytes of field data, refusing a product that overflows.
    pub(crate) fn take_n(&mut self, count: u32, each: usize) -> Result<&'a [u8], String> {
        let size = (count as usize)
            .checked_mul(each)
            .ok_or_else(|| format!("offset {}: {count} items overflow", self.pos))?;
        self.take(size)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    /// The padding up to the next multiple of four.
    pub(crate) fn align(&mut self) -> Result<(), String> {
        let pad = (4 - self.pos % 4) % 4;
        self.consume(pad).map(|_| ())
    }

    /// Every byte consumed.
    pub(crate) fn end(&self) -> Result<(), String> {
        if self.pos != self.data.len() {
            return Err(format!(
                "{} unconsumed bytes at offset {}",
                self.remaining(),
                self.pos
            ));
        }
        Ok(())
    }

    /// A 16-bit id, widened to 32 bits by a second half when the high bit is set.
    pub(crate) fn packed_id(&mut self) -> Result<u32, String> {
        let high = self.u16()?;
        if high & 0x8000 != 0 {
            let low = self.u16()?;
            return Ok((u32::from(high & 0x7FFF) << 16) | u32::from(low));
        }
        Ok(u32::from(high))
    }

    /// An object description (version `0x11`): aligned, then counts of palette swaps, texture
    /// swaps and part swaps, each list in turn, then aligned again.
    pub(crate) fn objdesc(&mut self) -> Result<(), String> {
        self.align()?;
        let head = self.take(4)?;
        let (version, palettes, textures, parts) = (head[0], head[1], head[2], head[3]);
        if version != 0x11 {
            return Err(format!(
                "object description version {version:#x} at offset {}",
                self.pos - 4
            ));
        }
        if palettes != 0 {
            self.packed_id()?;
        }
        for _ in 0..palettes {
            self.packed_id()?;
            self.take(2)?;
        }
        for _ in 0..textures {
            self.take(1)?;
            self.packed_id()?;
            self.packed_id()?;
        }
        for _ in 0..parts {
            self.take(1)?;
            self.packed_id()?;
        }
        self.align()
    }
}
