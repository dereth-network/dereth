//! [`ObjMap`] preserves the object tables' observable iteration order.
//!
//! Object-maintenance tables use intrusive chains, a **fixed** bucket count, front insertion,
//! bucket index `((id >> 8) ^ id) & mask`. The client walks them in bucket order and, inside a
//! bucket, along the chain — i.e. most-recently-inserted first. That order reaches the player
//! through the visible-object sweep, the destruction sweep and the radar's object list, so a
//! `BTreeMap` here would be a silent behaviour change.

use dereth_primitives::num::hash::long_hash;
use dereth_primitives::{CellId, ObjectId};
use std::fmt;

/// A key an [`ObjMap`] can hash: the client's tables are all keyed by a 32-bit id.
pub trait LongKey: Copy + Eq + fmt::Debug {
    fn long_key(self) -> u32;
}

impl LongKey for ObjectId {
    #[inline]
    fn long_key(self) -> u32 {
        self.0
    }
}

impl LongKey for CellId {
    #[inline]
    fn long_key(self) -> u32 {
        self.0
    }
}

/// One entry, as it sits in a bucket chain.
#[derive(Debug, Clone)]
struct Entry<K, V> {
    key: K,
    value: V,
}

/// The client's long-keyed hash table, with the retail bucket count and insertion order.
///
/// The buckets never grow: `object_table` and `weenie_object_table` are created with 0x80 and stay
/// there for the life of the process, which is what makes the enumeration order stable enough to be
/// observable in the first place.
#[derive(Clone)]
pub struct ObjMap<K, V> {
    /// One `Vec` per bucket; index 0 of a bucket is the **head** of the chain, i.e. the newest.
    buckets: Vec<Vec<Entry<K, V>>>,
    mask: u32,
    len: usize,
}

impl<K: LongKey, V> ObjMap<K, V> {
    /// `num_buckets` must be a power of two; the client only ever uses 0x10, 0x20, 0x40 and 0x80.
    #[must_use]
    pub fn with_buckets(num_buckets: usize) -> Self {
        assert!(
            num_buckets.is_power_of_two() && num_buckets > 0,
            "LongHash bucket counts are powers of two"
        );
        let mut buckets = Vec::with_capacity(num_buckets);
        buckets.resize_with(num_buckets, Vec::new);
        Self {
            buckets,
            // The count is a power of two and the client's masks are `size - 1`.
            #[allow(clippy::cast_possible_truncation)]
            mask: (num_buckets - 1) as u32,
            len: 0,
        }
    }

    #[inline]
    fn bucket_of(&self, key: K) -> usize {
        long_hash(key.long_key(), self.mask) as usize
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn num_buckets(&self) -> usize {
        self.buckets.len()
    }

    #[must_use]
    pub fn contains_key(&self, key: K) -> bool {
        self.get(key).is_some()
    }

    #[must_use]
    pub fn get(&self, key: K) -> Option<&V> {
        let b = self.bucket_of(key);
        self.buckets[b]
            .iter()
            .find(|e| e.key == key)
            .map(|e| &e.value)
    }

    pub fn get_mut(&mut self, key: K) -> Option<&mut V> {
        let b = self.bucket_of(key);
        self.buckets[b]
            .iter_mut()
            .find(|e| e.key == key)
            .map(|e| &mut e.value)
    }

    /// The table's add — **front insertion**. Replacing an existing key keeps its chain position,
    /// which is what setting a weenie description relies on when it copies a new descriptor over
    /// an old one.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let b = self.bucket_of(key);
        if let Some(e) = self.buckets[b].iter_mut().find(|e| e.key == key) {
            return Some(std::mem::replace(&mut e.value, value));
        }
        self.buckets[b].insert(0, Entry { key, value });
        self.len += 1;
        None
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        let b = self.bucket_of(key);
        let pos = self.buckets[b].iter().position(|e| e.key == key)?;
        self.len -= 1;
        Some(self.buckets[b].remove(pos).value)
    }

    pub fn clear(&mut self) {
        for b in &mut self.buckets {
            b.clear();
        }
        self.len = 0;
    }

    /// Bucket order, then chain order. **This is the observable order.**
    pub fn iter(&self) -> impl Iterator<Item = (K, &V)> {
        self.buckets
            .iter()
            .flat_map(|b| b.iter().map(|e| (e.key, &e.value)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (K, &mut V)> {
        self.buckets
            .iter_mut()
            .flat_map(|b| b.iter_mut().map(|e| (e.key, &mut e.value)))
    }

    /// The keys, in table order. Handy when a sweep needs to mutate the table it is walking.
    pub fn keys(&self) -> Vec<K> {
        self.iter().map(|(k, _)| k).collect()
    }
}

impl<K: LongKey, V> Default for ObjMap<K, V> {
    /// The default is the 0x80 buckets `object_table` and `weenie_object_table` use.
    fn default() -> Self {
        Self::with_buckets(0x80)
    }
}

impl<K: LongKey, V: fmt::Debug> fmt::Debug for ObjMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `dereth_primitives::num::hash::long_hash`, itself transcribed from
    /// the client's own object-table lookup. The bucket arithmetic is restated here so the
    /// table cannot silently start using a different one.
    #[test]
    fn bucket_index_is_the_clients_long_hash() {
        let m: ObjMap<ObjectId, u32> = ObjMap::with_buckets(0x80);
        for id in [0x5000_0001u32, 0x8000_0234, 0xDEAD_BEEF, 0] {
            assert_eq!(
                m.bucket_of(ObjectId(id)),
                (((id >> 8) ^ id) & 0x7F) as usize,
                "bucket for 0x{id:08X}"
            );
        }
    }

    #[test]
    fn insertion_is_at_the_front_of_the_chain() {
        // Two ids that collide in a 0x80-bucket table: ((id>>8)^id)&0x7F must match.
        // 0x001: (0 ^ 1) & 0x7F = 1.  0x100: (1 ^ 0) & 0x7F = 1.
        let a = ObjectId(0x0000_0001);
        let b = ObjectId(0x0000_0100);
        assert_eq!(((a.0 >> 8) ^ a.0) & 0x7F, ((b.0 >> 8) ^ b.0) & 0x7F);

        let mut m: ObjMap<ObjectId, u32> = ObjMap::with_buckets(0x80);
        m.insert(a, 1);
        m.insert(b, 2);
        let order: Vec<_> = m.iter().map(|(k, _)| k).collect();
        assert_eq!(order, vec![b, a], "the newest entry heads the chain");
    }

    #[test]
    fn replacing_a_key_keeps_its_chain_position() {
        let a = ObjectId(0x0000_0001);
        let b = ObjectId(0x0000_0100);
        let mut m: ObjMap<ObjectId, u32> = ObjMap::with_buckets(0x80);
        m.insert(a, 1);
        m.insert(b, 2);
        assert_eq!(m.insert(a, 9), Some(1));
        assert_eq!(m.iter().map(|(k, _)| k).collect::<Vec<_>>(), vec![b, a]);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn remove_and_lookup() {
        let mut m: ObjMap<ObjectId, u32> = ObjMap::with_buckets(0x10);
        for i in 0..64u32 {
            m.insert(ObjectId(0x5000_0000 + i), i);
        }
        assert_eq!(m.len(), 64);
        assert_eq!(m.get(ObjectId(0x5000_0007)), Some(&7));
        assert_eq!(m.remove(ObjectId(0x5000_0007)), Some(7));
        assert_eq!(m.get(ObjectId(0x5000_0007)), None);
        assert_eq!(m.len(), 63);
    }
}
