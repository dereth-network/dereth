//! A bounded little-endian cursor over one record, shared by the table and graphics decoders.

pub(crate) struct Cursor<'a> {
    inner: dereth_dat::Cursor<'a>,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            inner: dereth_dat::Cursor::new(data),
        }
    }

    pub(crate) fn pos(&self) -> usize {
        self.inner.position()
    }

    /// Move back to an earlier position, to read the same bytes as another shape.
    pub(crate) fn rewind(&mut self, pos: usize) {
        self.inner.rewind(pos);
    }

    /// The bytes from `start` to the current position.
    pub(crate) fn since(&self, start: usize) -> &'a [u8] {
        self.inner.since(start)
    }

    pub(crate) fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    fn read<T>(
        &mut self,
        size: usize,
        read: impl FnOnce(&mut dereth_dat::Cursor<'a>) -> Result<T, dereth_dat::DatError>,
    ) -> Result<T, String> {
        let pos = self.pos();
        let remaining = self.remaining();
        read(&mut self.inner)
            .map_err(|_| format!("offset {pos}: need {size} bytes, have {remaining}"))
    }

    /// `size` bytes of field data.
    pub(crate) fn take(&mut self, size: usize) -> Result<&'a [u8], String> {
        self.read(size, |c| c.bytes(size))
    }

    /// `count * each` bytes of field data, refusing a product that overflows.
    pub(crate) fn take_n(&mut self, count: u32, each: usize) -> Result<&'a [u8], String> {
        let size = (count as usize)
            .checked_mul(each)
            .ok_or_else(|| format!("offset {}: {count} items overflow", self.pos()))?;
        self.take(size)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, String> {
        self.read(2, dereth_dat::Cursor::u16)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, String> {
        self.read(4, dereth_dat::Cursor::u32)
    }

    pub(crate) fn f64(&mut self) -> Result<f64, String> {
        self.read(8, dereth_dat::Cursor::f64)
    }

    /// The padding up to the next multiple of four.
    pub(crate) fn align(&mut self) -> Result<(), String> {
        let pad = (4 - self.pos() % 4) % 4;
        self.take(pad).map(|_| ())
    }

    /// Every byte consumed.
    pub(crate) fn end(&self) -> Result<(), String> {
        if self.remaining() != 0 {
            return Err(format!(
                "{} unconsumed bytes at offset {}",
                self.remaining(),
                self.inner.position()
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
                self.pos() - 4
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (cursor adapter admission, errors and offsets)
    use super::Cursor;

    #[test]
    fn failed_fields_and_padding_preserve_their_start_offset() {
        let mut c = Cursor::new(&[1, 2, 3]);
        assert_eq!(c.take(1).unwrap(), &[1]);
        assert_eq!(c.u32().unwrap_err(), "offset 1: need 4 bytes, have 2");
        assert_eq!(c.f64().unwrap_err(), "offset 1: need 8 bytes, have 2");
        assert_eq!(c.align().unwrap_err(), "offset 1: need 3 bytes, have 2");
        assert_eq!(
            c.take(usize::MAX).unwrap_err(),
            format!("offset 1: need {} bytes, have 2", usize::MAX)
        );
        assert_eq!(
            c.take_n(2, usize::MAX).unwrap_err(),
            "offset 1: 2 items overflow"
        );
        assert_eq!(c.pos(), 1);
        assert_eq!(c.end().unwrap_err(), "2 unconsumed bytes at offset 1");
        assert_eq!(c.u16().unwrap(), 0x0302);
        c.end().unwrap();
    }

    #[test]
    fn rewind_and_since_clamp_to_consumed_bytes() {
        let data = [1, 2, 3, 4];
        let mut c = Cursor::new(&data);
        c.take(3).unwrap();
        c.rewind(usize::MAX);
        assert_eq!(c.pos(), 3);
        assert_eq!(c.since(1), &[2, 3]);
        assert_eq!(c.since(usize::MAX), &[]);
        c.rewind(1);
        assert_eq!(c.since(0), &[1]);
        c.align().unwrap();
        c.end().unwrap();
    }

    #[test]
    fn packed_ids_keep_fifteen_high_bits_and_consume_a_complete_first_half() {
        let mut c = Cursor::new(&[0xff, 0xff, 0x34, 0x12]);
        assert_eq!(c.packed_id().unwrap(), 0x7fff_1234);
        c.end().unwrap();
        let mut c = Cursor::new(&[0xff, 0xff, 0x34]);
        assert_eq!(c.packed_id().unwrap_err(), "offset 2: need 2 bytes, have 1");
        assert_eq!(c.pos(), 2);
    }
}
