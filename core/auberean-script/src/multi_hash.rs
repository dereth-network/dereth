//! The multi-hash: a hash table that keeps every entry added, several under one key, as the
//! engine keeps it (a C++ standard library hash table of the era, chained).
//!
//! The table is an array of buckets, each a chain of entries from its head. Its bucket count is
//! one of a fixed list of primes ([`BUCKET_SIZES`]); a new table has the first that is at least
//! 100 ([`START_BUCKETS`], 193). A key's bucket is its hash modulo the bucket count; a 32-bit key
//! is its own hash, a 64-bit key hashes to its low word.
//!
//! - **Adding** first grows the table when it would then hold more entries than it has buckets:
//!   to the first prime at least that count, moving each old bucket's chain, from its head, onto
//!   the heads of the new chains (so a chain that lands whole in one bucket comes out reversed).
//!   The entry then goes **right after the first entry with the same key** in its chain, or at
//!   the chain's head when there is none. Entries of one key therefore stay together, in the
//!   order first, newest, ..., second.
//! - **Iterating** walks the buckets in order and each chain from its head; packing writes the
//!   entry count and then every entry in that order.
//! - **Reading a packed table** adds its entries one by one in packed order, as adding does. A
//!   table read and packed again therefore lists each key's entries after its first in reverse
//!   order, and different keys sharing a bucket in reverse order of their first entries: the
//!   engine's packing is not its reading's inverse. Reading what it packed once more restores the
//!   original order.
//! - **Looking up** a key finds the first entry with it in its bucket; the end of its entries is
//!   the next entry of another key in the chain, else the head of the next bucket that has one,
//!   else past the last entry.
//! - **Removing** one entry unlinks it from its chain; removing a key unlinks every entry with
//!   it; clearing removes every entry. The table never shrinks.
//!
//! Every entry carries an identity, so an iterator names the entry it stands at, as the engine's
//! iterators hold a pointer to it, and stays on it while others come and go.

use crate::library::Elem;
use crate::native_object::Item;

/// The bucket counts a multi-hash may have, smallest first.
pub const BUCKET_SIZES: [u32; 28] = [
    53,
    97,
    193,
    389,
    769,
    1543,
    3079,
    6151,
    12289,
    24593,
    49157,
    98317,
    196_613,
    393_241,
    786_433,
    1_572_869,
    3_145_739,
    6_291_469,
    12_582_917,
    25_165_843,
    50_331_653,
    100_663_319,
    201_326_611,
    402_653_189,
    805_306_457,
    1_610_612_741,
    3_221_225_473,
    4_294_967_291,
];

/// The bucket count of a new multi-hash: the first of [`BUCKET_SIZES`] at least 100.
pub const START_BUCKETS: usize = 193;

/// The bucket count a table grows to for at least `n` entries: the first prime at least `n`,
/// else the largest.
// D2-04548
#[must_use]
pub fn bucket_count_for(n: u32) -> u32 {
    BUCKET_SIZES
        .iter()
        .copied()
        .find(|p| *p >= n)
        .unwrap_or(BUCKET_SIZES[BUCKET_SIZES.len() - 1])
}

/// The hash of a key: a 32-bit key is its own, a 64-bit key its low word, a hashed reference its
/// cached hash.
#[must_use]
pub fn hash_of(key: &Item) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    match *key {
        Item::Word(v) | Item::Ref(v) => v,
        Item::Long(v) => v as u32,
        Item::HashedRef { hash, .. } => hash,
    }
}

/// Same key: words, longs and references by value, hashed references by their reference.
fn same_key(a: &Item, b: &Item) -> bool {
    match (a, b) {
        (Item::HashedRef { id: x, .. }, Item::HashedRef { id: y, .. }) => x == y,
        _ => a == b,
    }
}

/// One entry: its identity, key and value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: u32,
    pub key: Item,
    pub value: Item,
}

/// A multi-hash as the engine keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiHash {
    pub key: Elem,
    pub value: Elem,
    /// The chains, one per bucket, each from its head.
    pub buckets: Vec<Vec<Node>>,
    /// The identity the next entry gets.
    next_id: u32,
}

/// Where an entry is: its bucket and its place in the chain.
type Place = (usize, usize);

impl MultiHash {
    /// A new, empty table of [`START_BUCKETS`] buckets.
    // D2-04627, D2-04642
    #[must_use]
    pub fn new(key: Elem, value: Elem) -> Self {
        Self {
            key,
            value,
            buckets: vec![Vec::new(); START_BUCKETS],
            next_id: 0,
        }
    }

    /// A table read from its packed entries, as the engine reads one: each added in packed order.
    // D2-04628, D2-04616, D2-04609, D2-04626, D2-04623, D2-04631, D2-04637, D2-04641
    #[must_use]
    pub fn unpacked(key: Elem, value: Elem, entries: &[(Item, Item)]) -> Self {
        let mut t = Self::new(key, value);
        for (k, v) in entries {
            t.add(*k, *v);
        }
        t
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buckets.iter().all(Vec::is_empty)
    }

    fn bucket_of(&self, key: &Item) -> usize {
        hash_of(key) as usize % self.buckets.len()
    }

    /// Every entry in iteration (and packing) order.
    pub fn entries(&self) -> impl Iterator<Item = &Node> + '_ {
        self.buckets.iter().flatten()
    }

    /// Every entry in packing order, as key and value.
    #[must_use]
    pub fn packed(&self) -> Vec<(Item, Item)> {
        self.entries().map(|n| (n.key, n.value)).collect()
    }

    /// Grow so that `hint` entries fit: when `hint` is more than the bucket count, to the first
    /// prime at least `hint`, each old chain moved from its head onto the new chains' heads.
    // D2-04611, D2-04618, D2-04625
    pub fn reserve(&mut self, hint: usize) {
        let old = self.buckets.len();
        if hint <= old {
            return;
        }
        let n = bucket_count_for(u32::try_from(hint).unwrap_or(u32::MAX)) as usize;
        if n <= old {
            return;
        }
        let mut fresh: Vec<Vec<Node>> = vec![Vec::new(); n];
        for chain in &self.buckets {
            for node in chain {
                fresh[hash_of(&node.key) as usize % n].insert(0, *node);
            }
        }
        self.buckets = fresh;
    }

    /// Insert without growing: right after the first entry with the same key in its chain, else
    /// at the chain's head. Returns the new entry's identity.
    // D2-04610, D2-04617, D2-04620, D2-04624
    pub fn insert(&mut self, key: Item, value: Item) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let node = Node { id, key, value };
        let b = self.bucket_of(&key);
        let chain = &mut self.buckets[b];
        match chain.iter().position(|n| same_key(&n.key, &key)) {
            Some(i) => chain.insert(i + 1, node),
            None => chain.insert(0, node),
        }
        id
    }

    /// Add an entry as the engine's scripts do: grow for one more, then insert.
    // D2-09361
    pub fn add(&mut self, key: Item, value: Item) -> u32 {
        self.reserve(self.len() + 1);
        self.insert(key, value)
    }

    /// How many entries have this key.
    // D2-09351
    #[must_use]
    pub fn count(&self, key: &Item) -> usize {
        self.buckets[self.bucket_of(key)]
            .iter()
            .filter(|n| same_key(&n.key, key))
            .count()
    }

    fn place(&self, id: u32) -> Option<Place> {
        self.buckets
            .iter()
            .enumerate()
            .find_map(|(b, chain)| chain.iter().position(|n| n.id == id).map(|i| (b, i)))
    }

    /// The entry with this identity, if the table holds it.
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.place(id).map(|(b, i)| &self.buckets[b][i])
    }

    /// The first entry of the first bucket from `b` on that has one.
    fn head_from(&self, b: usize) -> Option<u32> {
        self.buckets
            .get(b..)?
            .iter()
            .find_map(|chain| chain.first().map(|n| n.id))
    }

    /// The first entry in iteration order (none when the table is empty).
    // D2-04353, D2-09073, D2-09144, D2-09225, D2-09294, D2-09356, D2-09437
    #[must_use]
    pub fn first(&self) -> Option<u32> {
        self.head_from(0)
    }

    /// The entry after this one in iteration order: the next in its chain, else the head of the
    /// next bucket that has one; `Some(None)` past the last. `None` when the table does not hold
    /// the entry.
    // D2-04337, D2-04356
    #[must_use]
    pub fn after(&self, id: u32) -> Option<Option<u32>> {
        let (b, i) = self.place(id)?;
        Some(match self.buckets[b].get(i + 1) {
            Some(n) => Some(n.id),
            None => self.head_from(b + 1),
        })
    }

    /// The first entry with this key (none when there is none).
    // D2-09071, D2-09142, D2-09223, D2-09292, D2-09354, D2-09435
    #[must_use]
    pub fn find(&self, key: &Item) -> Option<u32> {
        self.buckets[self.bucket_of(key)]
            .iter()
            .find(|n| same_key(&n.key, key))
            .map(|n| n.id)
    }

    /// Where this key's entries end: the next entry of another key in the chain, else the head
    /// of the next bucket that has one; none (past the last) when there is neither or the key has
    /// no entry.
    // D2-09067, D2-09139, D2-09352, D2-09072, D2-09143, D2-09224, D2-09293, D2-09355, D2-09436
    #[must_use]
    pub fn end_of(&self, key: &Item) -> Option<u32> {
        let b = self.bucket_of(key);
        let chain = &self.buckets[b];
        let first = chain.iter().position(|n| same_key(&n.key, key))?;
        match chain[first + 1..].iter().find(|n| !same_key(&n.key, key)) {
            Some(n) => Some(n.id),
            None => self.head_from(b + 1),
        }
    }

    /// Remove the entry with this identity, if the chain its key names holds it.
    // D2-09069, D2-09353
    pub fn remove_node(&mut self, id: u32) {
        if let Some((b, i)) = self.place(id) {
            self.buckets[b].remove(i);
        }
    }

    /// Remove every entry with this key: how many there were.
    // D2-09070, D2-09141, D2-09291
    pub fn remove_key(&mut self, key: &Item) -> usize {
        let b = self.bucket_of(key);
        let chain = &mut self.buckets[b];
        let before = chain.len();
        chain.retain(|n| !same_key(&n.key, key));
        before - chain.len()
    }

    /// Remove every entry; the bucket count stays.
    // D2-04467, D2-04468
    pub fn clear(&mut self) {
        self.buckets.iter_mut().for_each(Vec::clear);
    }

    /// The references the entries hold: every key's, then every value's, in iteration order.
    // D2-04633, D2-04639
    #[must_use]
    pub fn references(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.entries().filter_map(|n| n.key.reference()).collect();
        v.extend(self.entries().filter_map(|n| n.value.reference()));
        v
    }

    /// Replace every reference the entries hold, in place (no entry moves).
    // D2-04634, D2-04640
    pub fn remap(&mut self, f: &mut dyn FnMut(u32) -> u32) {
        for n in self.buckets.iter_mut().flatten() {
            n.key.remap(f);
            n.value.remap(f);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the table's rules on hand-built entries)
    use super::*;

    fn w(v: u32) -> Item {
        Item::Word(v)
    }

    fn values(t: &MultiHash) -> Vec<u32> {
        t.entries()
            .map(|n| match n.value {
                Item::Word(v) => v,
                _ => 0,
            })
            .collect()
    }

    #[test]
    fn a_new_table_has_193_buckets() {
        assert_eq!(MultiHash::new(Elem::Word, Elem::Word).buckets.len(), 193);
        assert_eq!(bucket_count_for(100), 193);
        assert_eq!(bucket_count_for(194), 389);
        assert_eq!(bucket_count_for(u32::MAX), 4_294_967_291);
    }

    #[test]
    fn entries_of_one_key_go_after_its_first() {
        let mut t = MultiHash::new(Elem::Word, Elem::Word);
        for v in [10, 20, 30, 40] {
            t.add(w(1), w(v));
        }
        assert_eq!(values(&t), [10, 40, 30, 20]);
        assert_eq!(t.count(&w(1)), 4);
    }

    #[test]
    fn a_new_key_goes_to_the_head_of_its_chain() {
        let mut t = MultiHash::new(Elem::Word, Elem::Word);
        t.add(w(1), w(10));
        t.add(w(194), w(20));
        t.add(w(1), w(30));
        assert_eq!(values(&t), [20, 10, 30]);
    }

    #[test]
    fn reading_a_packed_table_reverses_each_key_after_its_first_and_twice_restores_it() {
        let packed: Vec<(Item, Item)> = [(1, 1), (1, 2), (1, 3), (1, 4), (2, 5), (2, 6), (2, 7)]
            .iter()
            .map(|(k, v)| (w(*k), w(*v)))
            .collect();
        let once = MultiHash::unpacked(Elem::Word, Elem::Word, &packed);
        assert_eq!(values(&once), [1, 4, 3, 2, 5, 7, 6]);
        let twice = MultiHash::unpacked(Elem::Word, Elem::Word, &once.packed());
        assert_eq!(twice.packed(), packed);
    }

    #[test]
    fn growing_moves_each_chain_reversed_onto_the_new_buckets() {
        let mut t = MultiHash::new(Elem::Word, Elem::Word);
        for v in 0..193 {
            t.add(w(7), w(v));
        }
        assert_eq!(t.buckets.len(), 193);
        let before = values(&t);
        t.add(w(7), w(1000));
        assert_eq!(t.buckets.len(), 389);
        let mut after: Vec<u32> = before.into_iter().rev().collect();
        // The new entry follows the first of its key, now the old chain's last.
        after.insert(1, 1000);
        assert_eq!(values(&t), after);
    }

    #[test]
    fn a_key_ends_at_the_next_key_in_its_chain_or_the_next_bucket() {
        let mut t = MultiHash::new(Elem::Word, Elem::Word);
        t.add(w(1), w(10));
        t.add(w(1), w(11));
        t.add(w(194), w(20));
        t.add(w(5), w(50));
        // Bucket 1: 194, then 1's two; bucket 5: 5.
        let k194 = t.find(&w(194)).unwrap();
        let k1 = t.find(&w(1)).unwrap();
        assert_eq!(t.end_of(&w(194)), Some(k1));
        assert_eq!(t.end_of(&w(1)), t.find(&w(5)));
        assert_eq!(t.end_of(&w(5)), None);
        assert_eq!(t.end_of(&w(9)), None);
        assert_eq!(t.first(), Some(k194));
        assert_eq!(t.after(k194), Some(Some(k1)));
    }

    #[test]
    fn removing_an_entry_or_a_key_keeps_the_rest_in_order() {
        let mut t = MultiHash::new(Elem::Word, Elem::Word);
        for v in [10, 20, 30] {
            t.add(w(1), w(v));
        }
        t.add(w(2), w(40));
        let second = t.after(t.find(&w(1)).unwrap()).unwrap().unwrap();
        t.remove_node(second);
        assert_eq!(values(&t), [10, 20, 40]);
        assert_eq!(t.remove_key(&w(1)), 2);
        assert_eq!(values(&t), [40]);
        t.clear();
        assert!(t.is_empty());
        assert_eq!(t.buckets.len(), 193);
    }

    #[test]
    fn a_64_bit_key_hashes_to_its_low_word() {
        let mut t = MultiHash::new(Elem::Long, Elem::Word);
        t.add(Item::Long(0x1_0000_0003), w(1));
        t.add(Item::Long(3), w(2));
        assert_eq!(t.buckets[3].len(), 2);
        assert_eq!(t.count(&Item::Long(3)), 1);
    }
}
