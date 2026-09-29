//! The pack writer: deterministic ordering, content-addressed dedupe, index accumulation. Carried
//! from the v1 server.
//!
//! The output depends only on the *set* of records, never on the order they were added:
//! records are sorted by `(table_id, key)` before layout, a duplicate key is an error, the dedupe
//! map is a `BTreeMap`, and the header has no timestamp or host name.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::ImportError;
use crate::pack::cursor::Codec;
use crate::pack::format::{
    align8, PackHeader, TableDirEntry, FLAG_INDEX_SORTED, FORMAT_VERSION, HASH_COVERAGE_START,
    HEADER_LEN, INDEX_ENTRY_LEN, MAX_RECORD_LEN, SCHEMA_VERSION, TABLE_ENTRY_LEN,
};
use crate::pack::{IndexEntry, TableId};

#[derive(Debug)]
struct Pending {
    table: u16,
    key: u64,
    bytes: Vec<u8>,
}

/// What a build produced.
#[derive(Debug, Clone)]
pub struct BuildStats {
    pub file_len: u64,
    pub index_count: u64,
    pub blob_len: u64,
    pub deduplicated_records: u64,
    pub content_hash: [u8; 32],
    pub dataset_id: [u8; 16],
    /// `(name, record_count, total_record_bytes)` per table, ascending by table id.
    pub tables: Vec<(String, u32, u64)>,
}

/// Accumulates records, then lays out exactly one pack.
#[derive(Debug)]
pub struct PackWriter {
    records: Vec<Pending>,
    /// `table_id -> name`, ascending.
    tables: BTreeMap<u16, &'static str>,
    dataset_id: [u8; 16],
    importer_version: u32,
}

impl PackWriter {
    #[must_use]
    pub fn new(dataset_id: [u8; 16], importer_version: u32) -> Self {
        Self {
            records: Vec::new(),
            tables: BTreeMap::new(),
            dataset_id,
            importer_version,
        }
    }

    /// Declare a table even if it ends up with no records, so the directory documents the shape.
    pub fn declare(&mut self, table: TableId) {
        self.tables.entry(table.0).or_insert(table.name());
    }

    /// Encode and queue one record.
    pub fn add<T: Codec>(
        &mut self,
        table: TableId,
        key: u64,
        record: &T,
    ) -> Result<(), ImportError> {
        let mut bytes = Vec::new();
        record.put(&mut bytes);
        if bytes.len() > MAX_RECORD_LEN {
            return Err(ImportError::RecordTooLarge {
                table: table.0,
                key,
                len: bytes.len(),
            });
        }
        self.declare(table);
        self.records.push(Pending {
            table: table.0,
            key,
            bytes,
        });
        Ok(())
    }

    /// Lay the pack out in memory.
    pub fn finish(mut self) -> Result<(Vec<u8>, BuildStats), ImportError> {
        self.records.sort_by_key(|r| (r.table, r.key));
        if let Some(w) = self
            .records
            .windows(2)
            .find(|w| (w[0].table, w[0].key) == (w[1].table, w[1].key))
        {
            return Err(ImportError::DuplicateKey {
                table: w[0].table,
                key: w[0].key,
            });
        }

        // Blob layout, deduplicating byte-identical records (compared in full, not just by hash).
        let mut seen: BTreeMap<[u8; 32], usize> = BTreeMap::new();
        let mut offsets = Vec::with_capacity(self.records.len());
        let mut blob: Vec<u8> = Vec::new();
        let mut deduplicated_records = 0u64;
        for r in &self.records {
            let hash = *blake3::hash(&r.bytes).as_bytes();
            let digest = u64::from_le_bytes(hash[..8].try_into().expect("8 bytes"));
            let existing = seen
                .get(&hash)
                .copied()
                .filter(|&off| blob.get(off..off + r.bytes.len()) == Some(&r.bytes[..]));
            let off = if let Some(off) = existing {
                deduplicated_records += 1;
                off
            } else {
                blob.resize(blob.len().next_multiple_of(8), 0);
                let off = blob.len();
                blob.extend_from_slice(&r.bytes);
                seen.insert(hash, off);
                off
            };
            offsets.push((off, digest));
        }

        let mut counts: BTreeMap<u16, (u64, u32, u64)> = BTreeMap::new();
        for (i, r) in self.records.iter().enumerate() {
            let e = counts.entry(r.table).or_insert((i as u64, 0, 0));
            e.1 += 1;
            e.2 += r.bytes.len() as u64;
        }

        let table_count = u32::try_from(self.tables.len()).expect("fewer than 2^32 tables");
        let index_count = self.records.len() as u64;
        let dir_bytes = (TABLE_ENTRY_LEN as u64) * u64::from(table_count);
        let index_off = align8(u64::from(HEADER_LEN) + dir_bytes);
        let blob_off = align8(index_off + u64::from(INDEX_ENTRY_LEN) * index_count);
        let blob_len = blob.len() as u64;
        let header = PackHeader {
            format_version: FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            flags: FLAG_INDEX_SORTED,
            header_len: HEADER_LEN,
            table_count,
            index_entry_len: INDEX_ENTRY_LEN,
            index_off,
            index_count,
            blob_off,
            blob_len,
            content_hash: [0u8; 32],
            dataset_id: self.dataset_id,
            importer_version: self.importer_version,
        };

        let mut out = Vec::with_capacity(usize_of(blob_off + blob_len));
        header.write(&mut out);
        let mut stats_tables = Vec::with_capacity(self.tables.len());
        for (&id, &name) in &self.tables {
            let (first_index, record_count, bytes) = counts.get(&id).copied().unwrap_or((0, 0, 0));
            TableDirEntry {
                table_id: id,
                record_schema_version: 1,
                record_count,
                first_index,
                name: TableDirEntry::name_bytes(name),
            }
            .write(&mut out);
            stats_tables.push((name.to_owned(), record_count, bytes));
        }
        out.resize(usize_of(index_off), 0);
        for (r, &(off, digest)) in self.records.iter().zip(&offsets) {
            IndexEntry {
                table_id: r.table,
                len: u32::try_from(r.bytes.len()).expect("record length checked"),
                key: r.key,
                blob_off: off as u64,
                digest,
            }
            .write(&mut out);
        }
        out.resize(usize_of(blob_off), 0);
        out.extend_from_slice(&blob);

        let content_hash = *blake3::hash(&out[HASH_COVERAGE_START..]).as_bytes();
        out[0x40..0x60].copy_from_slice(&content_hash);
        let stats = BuildStats {
            file_len: out.len() as u64,
            index_count,
            blob_len,
            deduplicated_records,
            content_hash,
            dataset_id: self.dataset_id,
            tables: stats_tables,
        };
        Ok((out, stats))
    }
}

/// A file offset as a `usize`; the pack is built in memory, so every offset already fits.
fn usize_of(v: u64) -> usize {
    usize::try_from(v).expect("a pack held in memory fits in usize")
}

/// Write `bytes` to `<path>.partial`, then rename it over `path`, so a failed write never leaves a
/// half pack under the real name.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = std::path::PathBuf::from(partial);
    std::fs::write(&partial, bytes).map_err(|e| ImportError::io(&partial, e))?;
    std::fs::rename(&partial, path).map_err(|e| ImportError::io(path, e))
}
