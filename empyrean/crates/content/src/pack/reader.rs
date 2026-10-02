//! `Pack`: one `world.pack`, memory-mapped read-only for the process lifetime, or held in memory
//! (tests and [`crate::MemContent`]). Carried from the v1 server.
//!
//! A lookup is a binary search and a slice into the mapping; records decode into owned values at
//! the boundary, so nothing borrows from the mapping for longer than a decode. Replacing
//! `world.pack` requires a restart: `open` checks every region against the file length up front.

use std::path::Path;

use empyrean_common::era::EraId;

use crate::error::PackError;
use crate::pack::cursor::{decode, Codec, Cursor};
use crate::pack::format::{
    self, PackHeader, TableDirEntry, FLAGS_KNOWN, FLAG_INDEX_SORTED, HASH_COVERAGE_START,
    HEADER_LEN, INDEX_ENTRY_LEN, TABLE_ENTRY_LEN,
};
use crate::pack::index::Index;
use crate::pack::TableId;

enum Backing {
    Map(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Backing {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Map(m) => m,
            Self::Owned(v) => v,
        }
    }
}

impl std::fmt::Debug for Backing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Map(m) => write!(f, "Map({} bytes)", m.len()),
            Self::Owned(v) => write!(f, "Owned({} bytes)", v.len()),
        }
    }
}

/// One validated pack.
#[derive(Debug)]
pub struct Pack {
    data: Backing,
    header: PackHeader,
    /// Ascending by `table_id`.
    tables: Vec<TableDirEntry>,
    index_span: core::ops::Range<usize>,
    blob_span: core::ops::Range<usize>,
}

fn span(off: u64, len: u64) -> Result<core::ops::Range<usize>, PackError> {
    let start = usize::try_from(off).map_err(|_| PackError::LengthOverflow(off))?;
    let n = usize::try_from(len).map_err(|_| PackError::LengthOverflow(len))?;
    let end = start.checked_add(n).ok_or(PackError::LengthOverflow(off))?;
    Ok(start..end)
}

impl Pack {
    /// Map the file and validate it. Does **not** hash the file; see [`Pack::verify_hash`].
    pub fn open(path: &Path) -> Result<Self, PackError> {
        let file = std::fs::File::open(path).map_err(|e| PackError::io(path, e))?;
        // `Mmap::map` is unsafe because another process could truncate or rewrite the file while
        // it is mapped. The server's contract is that `world.pack` is replaced only across a
        // restart. This is the one `unsafe` in the crate, which is why it `deny`s rather than
        // `forbid`s unsafe code.
        #[allow(unsafe_code)]
        let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| PackError::io(path, e))?;
        Self::validate(Backing::Map(map))
    }

    /// A pack held in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PackError> {
        Self::validate(Backing::Owned(bytes))
    }

    fn validate(data: Backing) -> Result<Self, PackError> {
        let buf = data.bytes();
        let file_len = buf.len() as u64;
        let header = PackHeader::read(buf)?;
        if header.format_version != format::FORMAT_VERSION {
            return Err(PackError::FormatVersion {
                expected: format::FORMAT_VERSION,
                found: header.format_version,
            });
        }
        if header.schema_version != format::SCHEMA_VERSION {
            return Err(PackError::SchemaVersion {
                expected: format::SCHEMA_VERSION,
                found: header.schema_version,
            });
        }
        let fields = [
            (
                "header_len",
                u64::from(HEADER_LEN),
                u64::from(header.header_len),
            ),
            (
                "index_entry_len",
                u64::from(INDEX_ENTRY_LEN),
                u64::from(header.index_entry_len),
            ),
            (
                "flags INDEX_SORTED",
                1,
                u64::from(header.flags & FLAG_INDEX_SORTED),
            ),
        ];
        for (field, expected, found) in fields {
            if expected != found {
                return Err(PackError::HeaderField {
                    field,
                    expected,
                    found,
                });
            }
        }
        if header.flags & !FLAGS_KNOWN != 0 {
            return Err(PackError::UnknownFlags(header.flags));
        }
        if EraId::from_pack_code(header.era).is_none() {
            return Err(PackError::UnknownEra(header.era));
        }
        for (region, off) in [("index", header.index_off), ("blobs", header.blob_off)] {
            if off % format::REGION_ALIGN != 0 {
                return Err(PackError::Misaligned { region, off });
            }
        }
        let index_bytes = u64::from(INDEX_ENTRY_LEN)
            .checked_mul(header.index_count)
            .ok_or(PackError::LengthOverflow(header.index_count))?;
        let index_end = header
            .index_off
            .checked_add(index_bytes)
            .ok_or(PackError::LengthOverflow(header.index_off))?;
        if index_end > file_len {
            return Err(PackError::Truncated {
                region: "index",
                len: file_len,
                need: index_end,
            });
        }
        let blob_end = header
            .blob_off
            .checked_add(header.blob_len)
            .ok_or(PackError::LengthOverflow(header.blob_off))?;
        if blob_end > file_len {
            return Err(PackError::Truncated {
                region: "blobs",
                len: file_len,
                need: blob_end,
            });
        }
        let dir_bytes = (TABLE_ENTRY_LEN as u64) * u64::from(header.table_count);
        let dir_end = u64::from(HEADER_LEN) + dir_bytes;
        if dir_end > file_len || dir_end > header.index_off {
            return Err(PackError::Truncated {
                region: "table directory",
                len: file_len.min(header.index_off),
                need: dir_end,
            });
        }

        let mut c = Cursor::new(&buf[span(u64::from(HEADER_LEN), dir_bytes)?]);
        let mut tables = Vec::with_capacity(header.table_count as usize);
        let mut total: u64 = 0;
        for i in 0..header.table_count as usize {
            let e = TableDirEntry::read(&mut c)?;
            if tables
                .last()
                .is_some_and(|p: &TableDirEntry| e.table_id <= p.table_id)
            {
                return Err(PackError::DirectoryUnsorted(i));
            }
            let last = e.first_index + u64::from(e.record_count);
            if last > header.index_count {
                return Err(PackError::TableSpan {
                    table: TableId(e.table_id),
                    first: e.first_index,
                    last,
                    count: header.index_count,
                });
            }
            total += u64::from(e.record_count);
            tables.push(e);
        }
        c.expect_end()?;
        if total != header.index_count {
            return Err(PackError::HeaderField {
                field: "index_count vs sum of record_count",
                expected: total,
                found: header.index_count,
            });
        }
        let index_span = span(header.index_off, index_bytes)?;
        let blob_span = span(header.blob_off, header.blob_len)?;
        Ok(Self {
            data,
            header,
            tables,
            index_span,
            blob_span,
        })
    }

    #[must_use]
    pub fn header(&self) -> &PackHeader {
        &self.header
    }

    /// The era the content was built for (`empyrean-import --era`).
    #[must_use]
    pub fn era(&self) -> EraId {
        EraId::from_pack_code(self.header.era).expect("checked at open")
    }

    #[must_use]
    pub fn tables(&self) -> &[TableDirEntry] {
        &self.tables
    }

    /// The whole file.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.data.bytes()
    }

    /// Recompute the BLAKE3 content hash and compare it with the header's.
    pub fn verify_hash(&self) -> Result<(), PackError> {
        let found = *blake3::hash(&self.bytes()[HASH_COVERAGE_START..]).as_bytes();
        if found != self.header.content_hash {
            return Err(PackError::HashMismatch {
                expected: hex(&self.header.content_hash),
                found: hex(&found),
            });
        }
        self.index().check_sorted()
    }

    pub(crate) fn index(&self) -> Index<'_> {
        // Every bound was checked in `validate`.
        let count = usize::try_from(self.header.index_count).unwrap_or(0);
        Index::new(
            self.bytes().get(self.index_span.clone()).unwrap_or(&[]),
            count,
        )
        .unwrap_or_else(|_| Index::new(&[], 0).expect("empty index"))
    }

    fn blob(&self, table: TableId, key: u64, off: u64, len: u32) -> Result<&[u8], PackError> {
        let start = usize::try_from(off).map_err(|_| PackError::LengthOverflow(off))?;
        let end = start
            .checked_add(len as usize)
            .ok_or(PackError::BlobOutOfRange { table, key })?;
        self.bytes()
            .get(self.blob_span.clone())
            .and_then(|b| b.get(start..end))
            .ok_or(PackError::BlobOutOfRange { table, key })
    }

    /// The raw record bytes for one key. `None` when absent; `Err` only on a malformed pack.
    pub fn raw(&self, table: TableId, key: u64) -> Result<Option<&[u8]>, PackError> {
        let Some(e) = self.index().find(table.0, key) else {
            return Ok(None);
        };
        self.blob(table, key, e.blob_off, e.len).map(Some)
    }

    /// Decode one record. `None` when the key is absent.
    pub fn get<T: Codec>(&self, table: TableId, key: u64) -> Result<Option<T>, PackError> {
        self.raw(table, key)?.map(decode).transpose()
    }

    /// Keys of one table, ascending.
    #[must_use]
    pub fn keys(&self, table: TableId) -> Vec<u64> {
        let ix = self.index();
        ix.table_range(table.0)
            .filter_map(|i| ix.get(i).map(|e| e.key))
            .collect()
    }

    /// Every record of one table, ascending by key.
    pub fn all<T: Codec>(&self, table: TableId) -> Result<Vec<(u64, T)>, PackError> {
        let ix = self.index();
        ix.table_range(table.0)
            .map(|i| {
                let e = ix.get(i).ok_or(PackError::IndexUnsorted(i))?;
                Ok((e.key, decode(self.blob(table, e.key, e.blob_off, e.len)?)?))
            })
            .collect()
    }

    /// The number of records in one table, from the directory.
    #[must_use]
    pub fn count(&self, table: TableId) -> u32 {
        self.tables
            .iter()
            .find(|t| t.table_id == table.0)
            .map_or(0, |t| t.record_count)
    }
}

/// Lower-case hex of a digest.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
