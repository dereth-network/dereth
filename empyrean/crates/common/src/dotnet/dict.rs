//! `System.Collections.Generic.Dictionary<TKey, TValue>` and `HashSet<T>` with .NET's observable
//! enumeration order.
//!
//! ACE iterates dictionaries and hash sets in many places where the order leaks into behaviour
//! (which monster is picked, the order of messages, and so on). .NET's order is not hash order:
//! both types keep a dense `_entries` array, enumerate it from index 0 to `_count`, and skip the
//! free slots. A removed slot is pushed onto a free list (`_freeList`, encoded as
//! `StartOfFreeList - next`), and the next insert takes the most recently freed slot. When the free
//! list is empty, an insert appends at `_count`. `Clear` resets `_count`, the free list and the free
//! count, so the next insert starts again at slot 0. Resizing copies the entries in index order, so
//! it never changes the enumeration order and is not modelled.
//!
//! Source: dotnet/runtime `src/libraries/System.Private.CoreLib/src/System/Collections/Generic/
//! Dictionary.cs` (`TryInsert`, `Remove`, `Clear`, `Enumerator.MoveNext`) and `HashSet.cs`, which
//! has used the same entries-and-free-list layout since .NET 5.
//!
//! Lookup uses a Rust `HashMap` from key to slot index; only the slot layout is observable.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
enum Slot<K, V> {
    Occupied(K, V),
    /// A freed slot; `next` is the next free slot, most recently freed first.
    Free {
        next: Option<usize>,
    },
}

/// A `Dictionary<TKey, TValue>` that enumerates in .NET's order. See the module documentation.
#[derive(Debug, Clone)]
pub struct DotNetDict<K, V> {
    entries: Vec<Slot<K, V>>,
    index: HashMap<K, usize>,
    free_list: Option<usize>,
    free_count: usize,
}

impl<K, V> Default for DotNetDict<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
            free_list: None,
            free_count: 0,
        }
    }
}

impl<K: Hash + Eq + Clone, V> DotNetDict<K, V> {
    /// `new Dictionary<TKey, TValue>()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `Count`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() - self.free_count
    }

    /// `Count == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The slot a new entry takes: the head of the free list, else the end of the entries array.
    fn take_slot(&mut self, key: K, value: V) -> usize {
        if let Some(slot) = self.free_list {
            let Slot::Free { next } = self.entries[slot] else {
                unreachable!("free list points at an occupied slot")
            };
            self.free_list = next;
            self.free_count -= 1;
            self.entries[slot] = Slot::Occupied(key, value);
            slot
        } else {
            self.entries.push(Slot::Occupied(key, value));
            self.entries.len() - 1
        }
    }

    /// The indexer setter `dict[key] = value`: overwrites in place (the slot and so the enumeration
    /// position are kept), or inserts. Returns the previous value.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(&slot) = self.index.get(&key) {
            if let Slot::Occupied(_, v) = &mut self.entries[slot] {
                return Some(std::mem::replace(v, value));
            }
        }
        let slot = self.take_slot(key.clone(), value);
        self.index.insert(key, slot);
        None
    }

    /// `TryAdd(key, value)`: inserts only if the key is absent; returns whether it inserted.
    pub fn try_add(&mut self, key: K, value: V) -> bool {
        if self.index.contains_key(&key) {
            return false;
        }
        let slot = self.take_slot(key.clone(), value);
        self.index.insert(key, slot);
        true
    }

    /// `Add(key, value)`.
    ///
    /// # Panics
    /// When the key is already present, as .NET throws `ArgumentException`.
    pub fn add(&mut self, key: K, value: V) {
        assert!(
            self.try_add(key, value),
            "ArgumentException: An item with the same key has already been added."
        );
    }

    /// `TryGetValue` / the indexer getter.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let slot = *self.index.get(key)?;
        match &self.entries[slot] {
            Slot::Occupied(_, v) => Some(v),
            Slot::Free { .. } => None,
        }
    }

    /// `CollectionsMarshal.GetValueRefOrNullRef`.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let slot = *self.index.get(key)?;
        match &mut self.entries[slot] {
            Slot::Occupied(_, v) => Some(v),
            Slot::Free { .. } => None,
        }
    }

    /// `CollectionsMarshal.GetValueRefOrAddDefault`, generalised: the value for `key`, inserting
    /// `make()` first (at the slot an insert would take) if the key is absent.
    pub fn get_or_insert_with(&mut self, key: K, make: impl FnOnce() -> V) -> &mut V {
        let slot = match self.index.get(&key) {
            Some(&slot) => slot,
            None => {
                let slot = self.take_slot(key.clone(), make());
                self.index.insert(key, slot);
                slot
            }
        };
        match &mut self.entries[slot] {
            Slot::Occupied(_, v) => v,
            Slot::Free { .. } => unreachable!("index points at a free slot"),
        }
    }

    /// `ContainsKey`.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.index.contains_key(key)
    }

    /// `Remove(key, out value)`: frees the slot and pushes it onto the free list.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let slot = self.index.remove(key)?;
        let old = std::mem::replace(
            &mut self.entries[slot],
            Slot::Free {
                next: self.free_list,
            },
        );
        self.free_list = Some(slot);
        self.free_count += 1;
        match old {
            Slot::Occupied(_, v) => Some(v),
            Slot::Free { .. } => unreachable!("index pointed at a free slot"),
        }
    }

    /// `Clear()`: the next insert starts at slot 0 again.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
        self.free_list = None;
        self.free_count = 0;
    }

    /// Enumeration (`foreach (var kvp in dict)`), in .NET order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().filter_map(|s| match s {
            Slot::Occupied(k, v) => Some((k, v)),
            Slot::Free { .. } => None,
        })
    }

    /// Enumeration with mutable values, in .NET order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.entries.iter_mut().filter_map(|s| match s {
            Slot::Occupied(k, v) => Some((&*k, v)),
            Slot::Free { .. } => None,
        })
    }

    /// `Keys`, in .NET order.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }

    /// `Values`, in .NET order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.iter().map(|(_, v)| v)
    }

    /// Mutable `Values`, in .NET order.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.iter_mut().map(|(_, v)| v)
    }
}

/// A `HashSet<T>` that enumerates in .NET's order (the same slot and free-list rules as
/// [`DotNetDict`]).
#[derive(Debug, Clone)]
pub struct DotNetHashSet<T> {
    inner: DotNetDict<T, ()>,
}

impl<T> Default for DotNetHashSet<T> {
    fn default() -> Self {
        Self {
            inner: DotNetDict::default(),
        }
    }
}

impl<T: Hash + Eq + Clone> DotNetHashSet<T> {
    /// `new HashSet<T>()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `Add(item)`: returns whether the item was added.
    pub fn insert(&mut self, item: T) -> bool {
        self.inner.try_add(item, ())
    }

    /// `Remove(item)`: returns whether the item was present.
    pub fn remove<Q>(&mut self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.remove(item).is_some()
    }

    /// `Contains(item)`.
    pub fn contains<Q>(&self, item: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.inner.contains_key(item)
    }

    /// `Count`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `Count == 0`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// `Clear()`.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Enumeration, in .NET order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.keys()
    }
}

impl<K: Hash + Eq + Clone, V> FromIterator<(K, V)> for DotNetDict<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut d = Self::new();
        for (k, v) in iter {
            d.insert(k, v);
        }
        d
    }
}

impl<T: Hash + Eq + Clone> FromIterator<T> for DotNetHashSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = Self::new();
        for t in iter {
            s.insert(t);
        }
        s
    }
}
