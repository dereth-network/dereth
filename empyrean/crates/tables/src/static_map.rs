//! The Rust shape of a C# `Dictionary` literal.
//!
//! Hand-written, not ported. Every generated `Dictionary<K, V>` literal is a
//! `StaticMap`: the entries in **declaration order**, which is the order a freshly built .NET
//! `Dictionary` enumerates in, plus (when that order is not already sorted) a key-sorted index for
//! lookups. Nothing is built at startup and nothing is hashed.

/// A read-only map over a `static` slice of `(key, value)` pairs.
#[derive(Debug)]
pub struct StaticMap<K: 'static, V: 'static> {
    entries: &'static [(K, V)],
    /// Indices into `entries` sorted by key; `None` when `entries` is itself sorted by key.
    by_key: Option<&'static [u32]>,
}

impl<K: Ord + 'static, V: 'static> StaticMap<K, V> {
    /// A map over `entries` (declaration order). `by_key` is `None` if they are already sorted.
    pub const fn new(entries: &'static [(K, V)], by_key: Option<&'static [u32]>) -> Self {
        Self { entries, by_key }
    }

    fn index_of(&self, key: &K) -> Option<usize> {
        match self.by_key {
            None => self.entries.binary_search_by(|(k, _)| k.cmp(key)).ok(),
            Some(idx) => idx
                .binary_search_by(|&i| self.entries[i as usize].0.cmp(key))
                .ok()
                .map(|j| idx[j] as usize),
        }
    }

    /// `Dictionary.TryGetValue`.
    pub fn get(&self, key: &K) -> Option<&'static V> {
        let entries = self.entries;
        self.index_of(key).map(|i| &entries[i].1)
    }

    /// `Dictionary.ContainsKey`.
    pub fn contains_key(&self, key: &K) -> bool {
        self.index_of(key).is_some()
    }

    /// The entries in declaration order (.NET enumeration order).
    pub fn entries(&self) -> &'static [(K, V)] {
        self.entries
    }

    /// `Dictionary.Count`.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
