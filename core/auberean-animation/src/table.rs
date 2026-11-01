//! A keyed table walked in the client's order.
//!
//! The animation state the client keeps (the behaviors a body plays, which behavior each layer
//! holds, the sliders and their crossfades, the effects started and stopped) lives in hash tables
//! whose order matters: the trackers walk them each frame, calling back and adding into poses as
//! they go. A table has a fixed number of buckets (the first of the client's prime sizes at least
//! the size it was made for); a key lands in bucket `key % buckets`, a new entry goes first in its
//! bucket, and a walk goes bucket by bucket, each from its newest entry.

/// The bucket counts the client's hash tables choose from.
pub const BUCKET_SIZES: [u32; 23] = [
    11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196597, 393209,
    786431, 1572853, 3145721, 6291449, 12582893, 25165813, 50331599,
];

/// The size the animation tables are made for.
pub const DEFAULT_SIZE: u32 = 0x20;

/// The bucket count of a table made for `size` entries: the first of [`BUCKET_SIZES`] at least
/// `size`, the largest when none is.
#[must_use]
pub fn buckets_for(size: u32) -> u32 {
    BUCKET_SIZES
        .iter()
        .copied()
        .find(|&b| b >= size)
        .unwrap_or(BUCKET_SIZES[BUCKET_SIZES.len() - 1])
}

/// A table of `u32` keys in the client's walking order.
#[derive(Clone, Debug, PartialEq)]
pub struct Table<V> {
    buckets: u32,
    /// The entries in walking order.
    entries: Vec<(u32, V)>,
}

impl<V> Default for Table<V> {
    fn default() -> Self {
        Self::new(DEFAULT_SIZE)
    }
}

impl<V> Table<V> {
    /// An empty table made for `size` entries.
    #[must_use]
    pub fn new(size: u32) -> Self {
        Self {
            buckets: buckets_for(size),
            entries: Vec::new(),
        }
    }

    fn bucket(&self, key: u32) -> u32 {
        key % self.buckets
    }

    #[must_use]
    pub fn get(&self, key: u32) -> Option<&V> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: u32) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    #[must_use]
    pub fn contains(&self, key: u32) -> bool {
        self.entries.iter().any(|(k, _)| *k == key)
    }

    /// Add an entry the table lacks, first in its bucket; `false` (and the table unchanged) when
    /// the key is there already.
    pub fn add(&mut self, key: u32, value: V) -> bool {
        if self.contains(key) {
            return false;
        }
        let b = self.bucket(key);
        let at = self
            .entries
            .iter()
            .position(|(k, _)| self.bucket(*k) >= b)
            .unwrap_or(self.entries.len());
        self.entries.insert(at, (key, value));
        true
    }

    /// Set a key's value: in place when the table has it, else added first in its bucket.
    pub fn set(&mut self, key: u32, value: V) {
        if let Some(v) = self.get_mut(key) {
            *v = value;
        } else {
            self.add(key, value);
        }
    }

    /// Take an entry out.
    pub fn remove(&mut self, key: u32) -> Option<V> {
        let at = self.entries.iter().position(|(k, _)| *k == key)?;
        Some(self.entries.remove(at).1)
    }

    /// The entry the table's value for `key` is, or the table's `value` added for it.
    pub fn entry_or(&mut self, key: u32, value: V) -> &mut V {
        self.add(key, value);
        self.get_mut(key).expect("present after adding")
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The keys in walking order.
    pub fn keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|(k, _)| *k)
    }

    /// The entries in walking order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &V)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, v))
    }

    /// The entries in walking order, the values mutable.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u32, &mut V)> + '_ {
        self.entries.iter_mut().map(|(k, v)| (*k, v))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a container: checked against the order it is defined by)
    use super::*;

    #[test]
    fn a_table_walks_bucket_by_bucket_newest_first() {
        let mut t = Table::new(DEFAULT_SIZE);
        assert_eq!(buckets_for(DEFAULT_SIZE), 47);
        // 3 and 50 share bucket 3; 1 is bucket 1; 49 is bucket 2.
        for k in [3, 1, 50, 49] {
            t.add(k, k * 10);
        }
        assert_eq!(t.keys().collect::<Vec<_>>(), [1, 49, 50, 3]);
        assert!(!t.add(3, 0), "a key is added once");
        t.set(3, 7);
        assert_eq!(t.get(3), Some(&7));
        assert_eq!(t.remove(50), Some(500));
        assert_eq!(t.keys().collect::<Vec<_>>(), [1, 49, 3]);
    }
}
