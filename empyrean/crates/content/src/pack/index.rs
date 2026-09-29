//! Binary search over the sorted index, and iteration by table. Carried from the v1 server.
//!
//! Lookup is a `partition_point` over the index for `(table_id, key)`, then a slice of the blob
//! region: no allocation, no decode, no start-up cost.

use crate::error::PackError;
use crate::pack::cursor::Cursor;
use crate::pack::format::{IndexEntry, INDEX_ENTRY_LEN};

/// A borrowed view of the index region. Entries are decoded on demand.
#[derive(Debug, Clone, Copy)]
pub struct Index<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Index<'a> {
    /// `bytes` must be at least `count * INDEX_ENTRY_LEN` long.
    pub(crate) fn new(bytes: &'a [u8], count: usize) -> Result<Self, PackError> {
        let need = count
            .checked_mul(INDEX_ENTRY_LEN as usize)
            .ok_or(PackError::LengthOverflow(count as u64))?;
        if bytes.len() < need {
            return Err(PackError::Truncated {
                region: "index",
                len: bytes.len() as u64,
                need: need as u64,
            });
        }
        Ok(Self {
            bytes: &bytes[..need],
            count,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Entry `i`, or `None` when out of range.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<IndexEntry> {
        if i >= self.count {
            return None;
        }
        let off = i * INDEX_ENTRY_LEN as usize;
        IndexEntry::read(&mut Cursor::new(
            &self.bytes[off..off + INDEX_ENTRY_LEN as usize],
        ))
        .ok()
    }

    /// The number of entries strictly before the first whose `(table_id, key)` is `>= probe`.
    #[must_use]
    pub fn partition_point(&self, probe: (u16, u64)) -> usize {
        let (mut lo, mut hi) = (0usize, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let less = self.get(mid).is_some_and(|e| e.sort_key() < probe);
            if less {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// The entry for exactly `(table_id, key)`, if present.
    #[must_use]
    pub fn find(&self, table_id: u16, key: u64) -> Option<IndexEntry> {
        let e = self.get(self.partition_point((table_id, key)))?;
        (e.table_id == table_id && e.key == key).then_some(e)
    }

    /// The half-open index range holding `table_id`'s entries.
    #[must_use]
    pub fn table_range(&self, table_id: u16) -> core::ops::Range<usize> {
        let start = self.partition_point((table_id, 0));
        let end = match table_id.checked_add(1) {
            Some(next) => self.partition_point((next, 0)),
            None => self.count,
        };
        start..end.max(start)
    }

    /// Check the whole index is strictly ascending by `(table_id, key)`.
    pub fn check_sorted(&self) -> Result<(), PackError> {
        let mut prev: Option<(u16, u64)> = None;
        for i in 0..self.count {
            let k = self.get(i).ok_or(PackError::IndexUnsorted(i))?.sort_key();
            if prev.is_some_and(|p| k <= p) {
                return Err(PackError::IndexUnsorted(i));
            }
            prev = Some(k);
        }
        Ok(())
    }
}
