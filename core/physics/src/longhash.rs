//! `LongHash<T>` and its by-value twin — the fixed-size intrusive table whose iteration order is
//! observable.
//!
//! Hash `((k >> 8) ^ k) & mask`, a **fixed** per-instance table size, insertion
//! at the **front** of the bucket chain, iteration ascending by bucket then along the chain. A
//! `HashMap` that rehashes changes the order the moment it grows, and the order decides which of
//! two simultaneous collisions is reported first.
//!
//! The object-maintenance code builds `object_table` with `table_size = 0x80`,
//! `key_shift = 8` and a `table_mask` of `0x7F` (derived by the constructor's shift loop). That is
//! the table swept during updates, so [`OBJECT_TABLE_SIZE`] is 128 and physics
//! update order is bucket order over it.

use dereth_primitives::num::hash::long_hash;

/// Fixed bucket count of the object-maintenance table.
pub const OBJECT_TABLE_SIZE: u32 = 0x80;
/// The matching mask, `table_size - 1`, which the client's constructor derives with a shift loop.
pub const OBJECT_TABLE_MASK: u32 = OBJECT_TABLE_SIZE - 1;

/// An order-preserving `LongHash`. Values are stored in a flat arena so that the chain can be
/// walked without any borrow that would outlive a re-entrant call.
#[derive(Debug, Clone)]
pub struct LongHash<V> {
    mask: u32,
    /// Head index per bucket, or `u32::MAX` for empty.
    heads: Vec<u32>,
    entries: Vec<Entry<V>>,
    /// Freed slots, reused newest-first, which is what the original's allocator does in practice.
    free: Vec<u32>,
    len: usize,
}

#[derive(Debug, Clone)]
struct Entry<V> {
    key: u32,
    next: u32,
    value: Option<V>,
}

const NIL: u32 = u32::MAX;

impl<V> LongHash<V> {
    /// A table with a fixed size. `size` must be a power of two, as every client instance is.
    #[must_use]
    pub fn with_size(size: u32) -> Self {
        assert!(
            size.is_power_of_two(),
            "LongHash table sizes are powers of two"
        );
        Self {
            mask: size - 1,
            heads: vec![NIL; size as usize],
            entries: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// The 128-bucket table sweeps.
    #[must_use]
    pub fn object_table() -> Self {
        Self::with_size(OBJECT_TABLE_SIZE)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn bucket(&self, key: u32) -> usize {
        long_hash(key, self.mask) as usize
    }

    /// Insert at the **front** of the bucket chain. An existing key is replaced in place, which
    /// keeps its position — the original's `clobber` does the same.
    pub fn insert(&mut self, key: u32, value: V) -> Option<V> {
        let b = self.bucket(key);
        let mut i = self.heads[b];
        while i != NIL {
            if self.entries[i as usize].key == key {
                return self.entries[i as usize].value.replace(value);
            }
            i = self.entries[i as usize].next;
        }
        let slot = if let Some(s) = self.free.pop() {
            self.entries[s as usize] = Entry {
                key,
                next: self.heads[b],
                value: Some(value),
            };
            s
        } else {
            let s = u32::try_from(self.entries.len()).expect("table index fits");
            self.entries.push(Entry {
                key,
                next: self.heads[b],
                value: Some(value),
            });
            s
        };
        self.heads[b] = slot;
        self.len += 1;
        None
    }

    #[must_use]
    pub fn get(&self, key: u32) -> Option<&V> {
        let mut i = self.heads[self.bucket(key)];
        while i != NIL {
            let e = &self.entries[i as usize];
            if e.key == key {
                return e.value.as_ref();
            }
            i = e.next;
        }
        None
    }

    pub fn get_mut(&mut self, key: u32) -> Option<&mut V> {
        let b = self.bucket(key);
        let mut i = self.heads[b];
        while i != NIL {
            if self.entries[i as usize].key == key {
                return self.entries[i as usize].value.as_mut();
            }
            i = self.entries[i as usize].next;
        }
        None
    }

    pub fn remove(&mut self, key: u32) -> Option<V> {
        let b = self.bucket(key);
        let mut prev = NIL;
        let mut i = self.heads[b];
        while i != NIL {
            if self.entries[i as usize].key == key {
                let next = self.entries[i as usize].next;
                if prev == NIL {
                    self.heads[b] = next;
                } else {
                    self.entries[prev as usize].next = next;
                }
                self.free.push(i);
                self.len -= 1;
                return self.entries[i as usize].value.take();
            }
            prev = i;
            i = self.entries[i as usize].next;
        }
        None
    }

    /// Every key in iteration order: ascending bucket, then along the chain from its head.
    ///
    /// Returned as an owned `Vec` on purpose. re-seeds its iterator each tick
    /// and objects can be created *and destroyed* inside `update_object`, so the sweep must
    /// tolerate removal of the current element; snapshotting the key order and re-looking-up each
    /// key is the shape that does that without holding a borrow across a re-entrant call.
    #[must_use]
    pub fn keys_in_order(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.len);
        for &head in &self.heads {
            let mut i = head;
            while i != NIL {
                let e = &self.entries[i as usize];
                out.push(e.key);
                i = e.next;
            }
        }
        out
    }

    /// Iterate values in the same order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &V)> + '_ {
        self.keys_in_order()
            .into_iter()
            .filter_map(move |k| self.get(k).map(|v| (k, v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the retail table size (0x80), key shift (8) and mask derivation, and the front
    // insertion (`entry.hash_next = buckets[i]; buckets[i] = entry`).

    #[test]
    fn the_object_table_is_128_buckets_masked_with_0x7f() {
        assert_eq!(OBJECT_TABLE_SIZE, 0x80);
        assert_eq!(OBJECT_TABLE_MASK, 0x7F);
        // The client's constructor derives the mask with a shift loop; reproduce it and compare.
        let mut mask = 0_u32;
        let mut bit = 1_u32;
        loop {
            mask |= bit;
            bit <<= 1;
            if mask | bit >= OBJECT_TABLE_SIZE {
                break;
            }
        }
        assert_eq!(mask, OBJECT_TABLE_MASK);
    }

    #[test]
    fn insertion_is_at_the_front_of_the_bucket_chain() {
        let mut t: LongHash<u32> = LongHash::with_size(8);
        // Three keys that all hash to the same bucket: ((k >> 8) ^ k) & 7.
        let same: Vec<u32> = (0..10_000_u32)
            .filter(|k| long_hash(*k, 7) == 3)
            .take(3)
            .collect();
        assert_eq!(same.len(), 3);
        for (i, k) in same.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            t.insert(*k, i as u32);
        }
        let order = t.keys_in_order();
        // Reverse insertion order within the bucket.
        assert_eq!(order, vec![same[2], same[1], same[0]]);
    }

    #[test]
    fn iteration_is_bucket_ascending_then_chain() {
        let mut t: LongHash<u32> = LongHash::with_size(4);
        for k in 0..40_u32 {
            t.insert(k, k);
        }
        let order = t.keys_in_order();
        assert_eq!(order.len(), 40);
        let buckets: Vec<u32> = order.iter().map(|k| long_hash(*k, 3)).collect();
        assert!(
            buckets.windows(2).all(|w| w[0] <= w[1]),
            "buckets must ascend: {buckets:?}"
        );
        // and within one bucket the keys descend, because insertion is at the front
        for b in 0..4_u32 {
            let inb: Vec<u32> = order
                .iter()
                .copied()
                .filter(|k| long_hash(*k, 3) == b)
                .collect();
            assert!(inb.windows(2).all(|w| w[0] > w[1]), "bucket {b}: {inb:?}");
        }
    }

    #[test]
    fn the_order_is_not_sorted_and_not_insertion_order() {
        // The iteration order is observable: a HashMap or a Vec
        // would give a different answer, and the difference is observable.
        let mut t: LongHash<u32> = LongHash::object_table();
        let keys: Vec<u32> = (0..64_u32).map(|i| 0x8000_0000 + i * 0x101).collect();
        for k in &keys {
            t.insert(*k, *k);
        }
        let order = t.keys_in_order();
        assert_ne!(order, keys, "must not be insertion order");
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_ne!(order, sorted, "must not be sorted order");
        assert_eq!(order.len(), keys.len());
    }

    #[test]
    fn removal_during_a_sweep_is_tolerated() {
        // The object-table iterator must survive removal of the element it is standing on.
        let mut t: LongHash<u32> = LongHash::object_table();
        for k in 0..50_u32 {
            t.insert(k, k);
        }
        let order = t.keys_in_order();
        let mut seen = 0;
        for k in order {
            if t.get(k).is_some() {
                seen += 1;
                if k % 3 == 0 {
                    t.remove(k);
                }
            }
        }
        assert_eq!(seen, 50);
        assert_eq!(t.len(), 50 - (0..50_u32).filter(|k| k % 3 == 0).count());
    }

    #[test]
    fn get_and_remove_round_trip() {
        let mut t: LongHash<&'static str> = LongHash::with_size(16);
        assert!(t.is_empty());
        t.insert(7, "a");
        t.insert(0x0107, "b"); // same bucket as 7? ((0x107>>8)^0x107)&15 = (1 ^ 7) & 15 = 6
        assert_eq!(t.get(7), Some(&"a"));
        assert_eq!(t.get(0x0107), Some(&"b"));
        assert_eq!(t.len(), 2);
        assert_eq!(t.remove(7), Some("a"));
        assert_eq!(t.get(7), None);
        assert_eq!(t.len(), 1);
        // a freed slot is reused
        t.insert(99, "c");
        assert_eq!(t.get(99), Some(&"c"));
    }
}
