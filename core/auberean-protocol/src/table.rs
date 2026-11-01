//! Keyed tables as messages carry them.
//!
//! A table is one header word, the entry count in its low 24 bits and the sender's bucket-size
//! class in its top byte, then the entries in the order the sender's table held them: bucket by
//! bucket (a key's bucket is the key modulo the bucket count), each bucket from its newest entry
//! to its oldest. The order is part of the bytes, so a decoded table keeps it, and a table built
//! here ([`Table::from_inserted`]) produces it the same way.

use alloc::vec::Vec;

use crate::wire::{Field, Reader, Result, Writer};

/// The bucket counts the top byte of a table's header selects, smallest first.
pub const BUCKET_COUNTS: [u32; 23] = [
    11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196_597, 393_209,
    786_431, 1_572_853, 3_145_721, 6_291_449, 12_582_893, 25_165_813, 50_331_599,
];

/// The size class every small table in the recorded sessions was sent with (47 buckets).
pub const SMALL: u8 = 2;

/// The most entries any table in a message holds; a larger count is a corrupt message.
pub const MAX_ENTRIES: usize = 0x1_0000;

/// One keyed table.
#[derive(Clone, Debug, PartialEq)]
pub struct Table<V> {
    /// The header's top byte: which of [`BUCKET_COUNTS`] the sender's table used.
    pub size_class: u8,
    /// The entries, in the order sent.
    pub entries: Vec<(u32, V)>,
}

impl<V> Default for Table<V> {
    fn default() -> Self {
        Self {
            size_class: SMALL,
            entries: Vec::new(),
        }
    }
}

impl<V> Table<V> {
    /// A table whose entries were added in this order, laid out as the sender lays it out:
    /// bucket by bucket, the newest entry of each bucket first.
    #[must_use]
    pub fn from_inserted(size_class: u8, inserted: impl IntoIterator<Item = (u32, V)>) -> Self {
        let buckets = BUCKET_COUNTS
            .get(usize::from(size_class))
            .copied()
            .unwrap_or(BUCKET_COUNTS[0]);
        let mut entries: Vec<(usize, u32, V)> = inserted
            .into_iter()
            .enumerate()
            .map(|(i, (k, v))| (i, k, v))
            .collect();
        // Bucket ascending; within a bucket, the latest added first.
        entries.sort_by(|a, b| (a.1 % buckets).cmp(&(b.1 % buckets)).then(b.0.cmp(&a.0)));
        Self {
            size_class,
            entries: entries.into_iter().map(|(_, k, v)| (k, v)).collect(),
        }
    }

    /// The value under a key.
    #[must_use]
    pub fn get(&self, key: u32) -> Option<&V> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<V: Field> Field for Table<V> {
    // D2-01473, D2-00124, D2-07314, D2-07320, D2-07329, D2-07342, D2-07347
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let header = r.u32()?;
        let n = (header & 0x00FF_FFFF) as usize;
        if n > MAX_ENTRIES {
            return Err(r.invalid("table count"));
        }
        let mut entries = Vec::with_capacity(n.min(256));
        for _ in 0..n {
            let k = r.u32()?;
            entries.push((k, V::read(r)?));
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(Self {
            size_class: (header >> 24) as u8,
            entries,
        })
    }

    fn write(&self, w: &mut Writer) {
        let n = u32::try_from(self.entries.len())
            .unwrap_or(0x00FF_FFFF)
            .min(0x00FF_FFFF);
        w.u32(u32::from(self.size_class) << 24 | n);
        for (k, v) in &self.entries {
            w.u32(*k);
            v.write(w);
        }
    }
}
