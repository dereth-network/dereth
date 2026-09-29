//! The overlay's records as the world database reads them: for each pack record the overlay has
//! rewritten, its new bytes, or `None` when the overlay deleted it (which hides the base record).
//!
//! Not ACE-derived.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::pack::{decode, Codec, TableId};
use crate::world_database::q;

/// One record the overlay sets: `(table, key, bytes)`, `None` bytes for a deletion.
pub type Record = (TableId, u64, Option<Arc<[u8]>>);

/// Record overrides, keyed `(table, key)`.
#[derive(Debug, Default, Clone)]
pub struct Layer {
    records: BTreeMap<(u16, u64), Option<Arc<[u8]>>>,
}

impl Layer {
    /// Set (or, with `None`, delete) one record.
    pub fn put(&mut self, table: TableId, key: u64, data: Option<Arc<[u8]>>) {
        self.records.insert((table.0, key), data);
    }

    /// Whether the overlay has anything for this record: `Some(None)` is a deletion.
    #[must_use]
    pub fn get(&self, table: TableId, key: u64) -> Option<Option<Arc<[u8]>>> {
        self.records.get(&(table.0, key)).cloned()
    }

    /// Records overridden, deletions included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Every override, ascending by `(table, key)`.
    pub fn iter(&self) -> impl Iterator<Item = (TableId, u64, Option<&Arc<[u8]>>)> {
        self.records
            .iter()
            .map(|(&(t, k), v)| (TableId(t), k, v.as_ref()))
    }

    /// Decode one overridden record: `None` when the overlay does not touch it, `Some(None)` when
    /// it deleted it.
    #[must_use]
    pub fn decoded<T: Codec>(&self, table: TableId, key: u64) -> Option<Option<T>> {
        self.records
            .get(&(table.0, key))
            .map(|v| v.as_deref().map(|b| q(decode::<T>(b))))
    }

    /// `base` (one table, ascending by key) with this layer applied: replaced records swapped in,
    /// deleted ones dropped, added ones inserted, still ascending by key.
    #[must_use]
    pub fn merge<T: Codec>(&self, table: TableId, base: Vec<(u64, T)>) -> Vec<(u64, T)> {
        let over: Vec<(u64, Option<&Arc<[u8]>>)> = self
            .records
            .range((table.0, 0)..=(table.0, u64::MAX))
            .map(|(&(_, k), v)| (k, v.as_ref()))
            .collect();
        if over.is_empty() {
            return base;
        }
        let mut out: Vec<(u64, T)> = base
            .into_iter()
            .filter(|(k, _)| over.binary_search_by_key(k, |(o, _)| *o).is_err())
            .collect();
        out.extend(
            over.into_iter()
                .filter_map(|(k, v)| v.map(|b| (k, q(decode::<T>(b))))),
        );
        out.sort_by_key(|(k, _)| *k);
        out
    }
}
