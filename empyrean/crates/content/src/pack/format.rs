//! The `world.pack` container: header, table directory and index entry, read and write, paired.
//!
//! Carried from the v1 server (our own format, not ACE-derived). Little-endian, every region
//! 8-byte aligned:
//!
//! | region | size | contents |
//! |---|---|---|
//! | header | 128 | magic `"ERE_PACK"`, versions, flags, counts, region offsets, BLAKE3-256 content hash of bytes `0x60..EOF`, BLAKE3-128 dataset id of the input, importer version, era |
//! | table directory | 32 per table | id, record schema, record count, first index entry, 16-byte name |
//! | index | 32 per record | table id, length, `u64` key, blob offset, 64-bit digest; sorted by `(table, key)` |
//! | blobs | | records back to back, each starting 8-aligned |
//!
//! There is deliberately **no timestamp and no host name anywhere in the file**: two builds from
//! the same dump are byte-identical.

use crate::error::PackError;
use crate::pack::cursor::{Cursor, PackWrite};

/// `"ERE_PACK"` read little-endian.
pub const MAGIC: u64 = 0x4B43_4150_5F45_5245;

/// The container layout. A reader refuses any other value.
pub const FORMAT_VERSION: u32 = 1;

/// The record schemas (the ACE World-DB models, `crate::records`). A reader refuses any other
/// value: a record carries no framing that would make a misread fail reliably. v1's packs were
/// schema 1..=3; this crate's records start at 100.
pub const SCHEMA_VERSION: u32 = 100;

/// Fixed header size.
pub const HEADER_LEN: u32 = 128;

/// BLAKE3-256 covers the file from here to EOF: the tail of the header, the directory, the index
/// and the blob region. It excludes `content_hash` itself and every field above it.
pub const HASH_COVERAGE_START: usize = 0x60;

/// Size of one table-directory entry.
pub const TABLE_ENTRY_LEN: usize = 32;

/// Size of one index entry.
pub const INDEX_ENTRY_LEN: u32 = 32;

/// Every region begins on an 8-byte boundary; the writer pads and the reader asserts.
pub const REGION_ALIGN: u64 = 8;

/// Always set, and asserted on open.
pub const FLAG_INDEX_SORTED: u32 = 1 << 0;
/// Bits outside this mask must be zero.
pub const FLAGS_KNOWN: u32 = FLAG_INDEX_SORTED;

/// A single record longer than this is refused by the writer.
pub const MAX_RECORD_LEN: usize = 64 * 1024 * 1024;

/// Round `v` up to the next multiple of [`REGION_ALIGN`].
#[must_use]
pub const fn align8(v: u64) -> u64 {
    v.wrapping_add(REGION_ALIGN - 1) & !(REGION_ALIGN - 1)
}

/// The 128-byte header at offset 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackHeader {
    pub format_version: u32,
    pub schema_version: u32,
    pub flags: u32,
    pub header_len: u32,
    pub table_count: u32,
    pub index_entry_len: u32,
    pub index_off: u64,
    pub index_count: u64,
    pub blob_off: u64,
    pub blob_len: u64,
    /// BLAKE3-256 of the file from [`HASH_COVERAGE_START`] to EOF.
    pub content_hash: [u8; 32],
    /// BLAKE3-128 of the importer's input bytes.
    pub dataset_id: [u8; 16],
    /// Inside the hash coverage, so an importer bump changes the hash.
    pub importer_version: u32,
    /// The era the content was built for, as `empyrean_common::era::EraId::pack_code` numbers it.
    /// End of retail is 0, which is also what a pack written before the field existed holds.
    pub era: u32,
}

impl PackHeader {
    /// Decode the header from the first [`HEADER_LEN`] bytes. Validates only what is structural;
    /// version policy lives in `Pack::from_bytes`.
    pub fn read(buf: &[u8]) -> Result<Self, PackError> {
        if buf.len() < HEADER_LEN as usize {
            return Err(PackError::Truncated {
                region: "header",
                len: buf.len() as u64,
                need: u64::from(HEADER_LEN),
            });
        }
        let mut c = Cursor::new(&buf[..HEADER_LEN as usize]);
        let magic = c.u64()?;
        if magic != MAGIC {
            return Err(PackError::BadMagic(magic));
        }
        let format_version = c.u32()?;
        let schema_version = c.u32()?;
        let flags = c.u32()?;
        let header_len = c.u32()?;
        let table_count = c.u32()?;
        let index_entry_len = c.u32()?;
        let index_off = c.u64()?;
        let index_count = c.u64()?;
        let blob_off = c.u64()?;
        let blob_len = c.u64()?;
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(c.bytes(32)?);
        debug_assert_eq!(c.position(), HASH_COVERAGE_START);
        let mut dataset_id = [0u8; 16];
        dataset_id.copy_from_slice(c.bytes(16)?);
        let importer_version = c.u32()?;
        let era = c.u32()?;
        if c.bytes(8)?.iter().any(|&b| b != 0) {
            return Err(PackError::HeaderField {
                field: "reserved",
                expected: 0,
                found: 1,
            });
        }
        c.expect_end()?;
        Ok(Self {
            format_version,
            schema_version,
            flags,
            header_len,
            table_count,
            index_entry_len,
            index_off,
            index_count,
            blob_off,
            blob_len,
            content_hash,
            dataset_id,
            importer_version,
            era,
        })
    }

    /// Append the 128 header bytes. Paired with [`PackHeader::read`]; field order is the format.
    pub fn write(&self, w: &mut Vec<u8>) {
        let start = w.len();
        w.put_u64(MAGIC);
        w.put_u32(self.format_version);
        w.put_u32(self.schema_version);
        w.put_u32(self.flags);
        w.put_u32(self.header_len);
        w.put_u32(self.table_count);
        w.put_u32(self.index_entry_len);
        w.put_u64(self.index_off);
        w.put_u64(self.index_count);
        w.put_u64(self.blob_off);
        w.put_u64(self.blob_len);
        w.extend_from_slice(&self.content_hash);
        debug_assert_eq!(w.len() - start, HASH_COVERAGE_START);
        w.extend_from_slice(&self.dataset_id);
        w.put_u32(self.importer_version);
        w.put_u32(self.era);
        w.extend_from_slice(&[0u8; 8]);
        debug_assert_eq!(w.len() - start, HEADER_LEN as usize);
    }
}

/// One 32-byte table-directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDirEntry {
    pub table_id: u16,
    pub record_schema_version: u16,
    pub record_count: u32,
    /// Index of this table's first entry. Entries are contiguous because the index is sorted by
    /// `(table_id, key)`.
    pub first_index: u64,
    /// ASCII, zero-padded. Diagnostic only.
    pub name: [u8; 16],
}

impl TableDirEntry {
    pub fn read(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        let table_id = c.u16()?;
        let record_schema_version = c.u16()?;
        let record_count = c.u32()?;
        let first_index = c.u64()?;
        let mut name = [0u8; 16];
        name.copy_from_slice(c.bytes(16)?);
        Ok(Self {
            table_id,
            record_schema_version,
            record_count,
            first_index,
            name,
        })
    }

    pub fn write(&self, w: &mut Vec<u8>) {
        w.put_u16(self.table_id);
        w.put_u16(self.record_schema_version);
        w.put_u32(self.record_count);
        w.put_u64(self.first_index);
        w.extend_from_slice(&self.name);
    }

    /// Pack an ASCII name into the fixed 16 bytes, truncating rather than failing.
    #[must_use]
    pub fn name_bytes(s: &str) -> [u8; 16] {
        let mut out = [0u8; 16];
        let src = s.as_bytes();
        let n = src.len().min(16);
        out[..n].copy_from_slice(&src[..n]);
        out
    }

    /// The name as a `&str`, up to the first NUL.
    #[must_use]
    pub fn name_str(&self) -> &str {
        let end = self
            .name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.name.len());
        core::str::from_utf8(&self.name[..end]).unwrap_or("<non-ascii>")
    }
}

/// One 32-byte index entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    pub table_id: u16,
    pub len: u32,
    pub key: u64,
    /// Offset from the start of the blob region, **not** from the start of the file.
    pub blob_off: u64,
    /// The low 64 bits of BLAKE3 of the record bytes. Identical records share one blob.
    pub digest: u64,
}

impl IndexEntry {
    pub fn read(c: &mut Cursor<'_>) -> Result<Self, PackError> {
        let table_id = c.u16()?;
        let reserved = c.u16()?;
        if reserved != 0 {
            return Err(PackError::HeaderField {
                field: "index entry reserved",
                expected: 0,
                found: u64::from(reserved),
            });
        }
        let len = c.u32()?;
        let key = c.u64()?;
        let blob_off = c.u64()?;
        let digest = c.u64()?;
        Ok(Self {
            table_id,
            len,
            key,
            blob_off,
            digest,
        })
    }

    pub fn write(&self, w: &mut Vec<u8>) {
        w.put_u16(self.table_id);
        w.put_u16(0);
        w.put_u32(self.len);
        w.put_u64(self.key);
        w.put_u64(self.blob_off);
        w.put_u64(self.digest);
    }

    /// The sort key of the whole index: `(table_id, key)`, strictly ascending.
    #[must_use]
    pub fn sort_key(&self) -> (u16, u64) {
        (self.table_id, self.key)
    }
}
