//! ac1999 dat downloads: the server brings the client's `portal.dat` and `cell.dat` to its own
//! iterations before the character list, on the dat queue.
//!
//! A download opens with [`DatBegin`] (sent more than once), then one [`DatRecord`] per file. The
//! begin names the iteration each file reaches and how many records will come; the client adopts
//! the iterations once the last record is in. A [`DatTouch`] counts as one record. Nothing is
//! answered: the character list simply follows. Outside a download the client asks for a cell
//! record it lacks with [`CellRequest`], which [`CellRecord`] (or [`CellPair`]) answers.
//!
//! Records travel zlib-compressed behind their compressed and inflated lengths; inflating is the
//! caller's.

use dereth_protocol::{Message, MessageError, Opcode, Reader, Writer};

use super::invalid;

/// A record inflates to fewer bytes than this, or the client refuses it.
pub const MAX_RECORD: u32 = 0x40_0000;

/// One record's compressed bytes and the length they inflate to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packed {
    pub inflated_len: u32,
    pub bytes: Vec<u8>,
}

impl Packed {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let packed_len = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 packed length"))?;
        let inflated_len = r.u32()?;
        if inflated_len == 0 || inflated_len >= MAX_RECORD {
            return Err(invalid("ac1999 inflated length"));
        }
        let bytes = r.bytes(packed_len)?.to_vec();
        Ok(Self {
            inflated_len,
            bytes,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.inflated_len == 0 || self.inflated_len >= MAX_RECORD {
            return Err(invalid("ac1999 inflated length"));
        }
        w.u32(u32::try_from(self.bytes.len()).map_err(|_| invalid("ac1999 packed length"))?);
        w.u32(self.inflated_len);
        w.bytes(&self.bytes);
        Ok(())
    }
}

/// The alignment a server writes after a record's bytes, and nothing more.
fn expect_padding(r: &mut Reader<'_>) -> Result<(), MessageError> {
    if r.remaining() > 3 {
        return Err(invalid("ac1999 record trailing bytes"));
    }
    r.rest();
    Ok(())
}

/// `0xF7B8`: a download begins. The client adopts each iteration once `portal_records +
/// cell_records` records have arrived; the byte totals only size its progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DatBegin {
    pub portal_iteration: u32,
    pub portal_records: u32,
    pub cell_iteration: u32,
    pub cell_records: u32,
    pub portal_bytes: u32,
    pub cell_bytes: u32,
}

impl Message for DatBegin {
    const OPCODE: Opcode = Opcode(0xf7b8);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let value = Self {
            portal_iteration: r.u32()?,
            portal_records: r.u32()?,
            cell_iteration: r.u32()?,
            cell_records: r.u32()?,
            portal_bytes: r.u32()?,
            cell_bytes: r.u32()?,
        };
        r.expect_exhausted()?;
        Ok(value)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        for v in [
            self.portal_iteration,
            self.portal_records,
            self.cell_iteration,
            self.cell_records,
            self.portal_bytes,
            self.cell_bytes,
        ] {
            w.u32(v);
        }
        Ok(())
    }
}

/// Which file a downloaded record belongs in: the client's caches 0 to 2 (landblocks, their
/// information and dungeon cells) are `cell.dat`'s, every other cache `portal.dat`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DatFileKind {
    Portal,
    Cell,
}

impl DatFileKind {
    #[must_use]
    pub fn of_cache(cache: u32) -> Self {
        if cache <= 2 {
            Self::Cell
        } else {
            Self::Portal
        }
    }
}

/// `0xF7B7`: one downloaded record, for the cache that keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatRecord {
    pub id: u32,
    pub cache: u32,
    pub data: Packed,
}

impl DatRecord {
    #[must_use]
    pub fn file(&self) -> DatFileKind {
        DatFileKind::of_cache(self.cache)
    }
}

impl Message for DatRecord {
    const OPCODE: Opcode = Opcode(0xf7b7);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = r.u32()?;
        let cache = r.u32()?;
        let data = Packed::read(r)?;
        expect_padding(r)?;
        Ok(Self { id, cache, data })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id);
        w.u32(self.cache);
        self.data.write(w)?;
        w.align4();
        Ok(())
    }
}

/// `0xF7AC`: a `cell.dat` record the client asked for. It does not count toward a download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRecord {
    pub id: u32,
    pub data: Packed,
}

impl Message for CellRecord {
    const OPCODE: Opcode = Opcode(0xf7ac);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = r.u32()?;
        let data = Packed::read(r)?;
        expect_padding(r)?;
        Ok(Self { id, data })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id);
        self.data.write(w)?;
        w.align4();
        Ok(())
    }
}

/// `0xF7AB`: two `cell.dat` records in one message, the second's id straight after the first's
/// bytes; the first is absent when its id is zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellPair {
    pub first: Option<CellRecord>,
    pub second: CellRecord,
}

impl Message for CellPair {
    const OPCODE: Opcode = Opcode(0xf7ab);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let id = r.u32()?;
        let first = if id == 0 {
            None
        } else {
            Some(CellRecord {
                id,
                data: Packed::read(r)?,
            })
        };
        let id = r.u32()?;
        let second = CellRecord {
            id,
            data: Packed::read(r)?,
        };
        expect_padding(r)?;
        Ok(Self { first, second })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        match &self.first {
            None => w.u32(0),
            Some(first) if first.id != 0 => {
                w.u32(first.id);
                first.data.write(w)?;
            }
            Some(_) => return Err(invalid("ac1999 cell pair identity")),
        }
        w.u32(self.second.id);
        self.second.data.write(w)?;
        w.align4();
        Ok(())
    }
}

/// Policy: the ids one [`DatTouch`] may list.
const MAX_TOUCHED: usize = 0x1_0000;

/// `0xF7BB`: a list of record ids the client goes over in its caches. It counts as one record of
/// a download; nothing is downloaded with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatTouch {
    pub ids: Vec<u32>,
}

impl Message for DatTouch {
    const OPCODE: Opcode = Opcode(0xf7bb);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let count = usize::try_from(r.u32()?).map_err(|_| invalid("ac1999 id count"))?;
        if count > MAX_TOUCHED || count > r.remaining() / 4 {
            return Err(invalid("ac1999 id count"));
        }
        let ids = (0..count).map(|_| r.u32()).collect::<Result<_, _>>()?;
        r.expect_exhausted()?;
        Ok(Self { ids })
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if self.ids.len() > MAX_TOUCHED {
            return Err(invalid("ac1999 id count"));
        }
        w.u32(u32::try_from(self.ids.len()).map_err(|_| invalid("ac1999 id count"))?);
        for &id in &self.ids {
            w.u32(id);
        }
        Ok(())
    }
}

/// `0xF7A9`: the client asks for a `cell.dat` record it does not have, naming its cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRequest {
    pub id: u32,
    pub cache: u32,
}

impl Message for CellRequest {
    const OPCODE: Opcode = Opcode(0xf7a9);
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let value = Self {
            id: r.u32()?,
            cache: r.u32()?,
        };
        r.expect_exhausted()?;
        Ok(value)
    }
    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id);
        w.u32(self.cache);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
