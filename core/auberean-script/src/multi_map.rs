//! The multi-map: entries ordered by key, several under one key, as the engine keeps it (a C++
//! standard library ordered tree of the era).
//!
//! - **Adding** puts an entry after every entry whose key is not greater than its own, so entries
//!   of one key keep the order they were added in. Keys compare as unsigned numbers.
//! - **Iterating** walks the entries in key order; packing writes the entry count and then every
//!   entry in that order. **Reading a packed map** adds its entries one by one in packed order.
//! - **Looking up**: `find` is the first entry with a key, else past the last; `lower_bound` the
//!   first entry whose key is not less than it; `upper_bound` the first whose key is greater;
//!   `find_last` is the engine's `upper_bound` too, the place after a key's last entry, whether
//!   the map holds the key or not.
//! - **Removing** one entry unlinks it; removing a key removes every entry with it; clearing
//!   removes every entry.
//!
//! Every entry carries an identity, so an iterator names the entry it stands at, as the engine's
//! iterators hold a pointer to its tree node, and stays on it while others come and go.

use crate::library::Elem;
use crate::multi_hash::Node;
use crate::native_object::Item;

/// A key's place in the order: words, longs and references as unsigned numbers.
fn rank(key: &Item) -> u64 {
    match *key {
        Item::Word(v) | Item::Ref(v) => u64::from(v),
        Item::Long(v) => v,
        Item::HashedRef { id, .. } => u64::from(id),
    }
}

/// A multi-map as the engine keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiMap {
    pub key: Elem,
    pub value: Elem,
    /// The entries in key order.
    nodes: Vec<Node>,
    /// The identity the next entry gets.
    next_id: u32,
}

impl MultiMap {
    /// A new, empty map.
    // D2-04538, D2-04540, D2-04541, D2-04542
    #[must_use]
    pub fn new(key: Elem, value: Elem) -> Self {
        Self {
            key,
            value,
            nodes: Vec::new(),
            next_id: 0,
        }
    }

    /// A map read from its packed entries, as the engine reads one: each added in packed order.
    // D2-04549, D2-04551, D2-04554, D2-04557
    #[must_use]
    pub fn unpacked(key: Elem, value: Elem, entries: &[(Item, Item)]) -> Self {
        let mut m = Self::new(key, value);
        for (k, v) in entries {
            m.insert(*k, *v);
        }
        m
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every entry in key (and packing) order.
    #[must_use]
    pub fn entries(&self) -> &[Node] {
        &self.nodes
    }

    /// Every entry in packing order, as key and value.
    #[must_use]
    pub fn packed(&self) -> Vec<(Item, Item)> {
        self.nodes.iter().map(|n| (n.key, n.value)).collect()
    }

    /// The place of the first entry whose key is not less than `key`.
    fn lower_place(&self, key: &Item) -> usize {
        self.nodes.partition_point(|n| rank(&n.key) < rank(key))
    }

    /// The place of the first entry whose key is greater than `key`.
    fn upper_place(&self, key: &Item) -> usize {
        self.nodes.partition_point(|n| rank(&n.key) <= rank(key))
    }

    /// The identity of the entry at a place; none past the last.
    fn at(&self, place: usize) -> Option<u32> {
        self.nodes.get(place).map(|n| n.id)
    }

    /// Add an entry after every entry whose key is not greater than its own. Returns the new
    /// entry's identity.
    // D2-09099, D2-09170
    pub fn insert(&mut self, key: Item, value: Item) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let at = self.upper_place(&key);
        self.nodes.insert(at, Node { id, key, value });
        id
    }

    /// How many entries have this key.
    #[must_use]
    pub fn count(&self, key: &Item) -> usize {
        self.upper_place(key) - self.lower_place(key)
    }

    /// The first entry with this key; none (past the last) when there is none.
    // D2-09085, D2-09156, D2-09309, D2-09366
    #[must_use]
    pub fn find(&self, key: &Item) -> Option<u32> {
        let at = self.lower_place(key);
        self.nodes
            .get(at)
            .filter(|n| rank(&n.key) == rank(key))
            .map(|n| n.id)
    }

    /// The place after this key's last entry (none: past the last), as the engine's `find_last`
    /// answers.
    // D2-09086, D2-09157, D2-09310, D2-09367
    #[must_use]
    pub fn find_last(&self, key: &Item) -> Option<u32> {
        self.upper_bound(key)
    }

    /// The first entry whose key is not less than this one (none: past the last).
    // D2-09089, D2-09160, D2-09313, D2-09370
    #[must_use]
    pub fn lower_bound(&self, key: &Item) -> Option<u32> {
        self.at(self.lower_place(key))
    }

    /// The first entry whose key is greater than this one (none: past the last).
    // D2-09090, D2-09161, D2-09314, D2-09371
    #[must_use]
    pub fn upper_bound(&self, key: &Item) -> Option<u32> {
        self.at(self.upper_place(key))
    }

    /// The first entry (none when the map is empty).
    // D2-09087, D2-09158, D2-09311, D2-09368
    #[must_use]
    pub fn first(&self) -> Option<u32> {
        self.at(0)
    }

    /// The entry with this identity, if the map holds it.
    #[must_use]
    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn place_of(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// The entry after this one; `Some(None)` past the last. `None` when the map does not hold
    /// the entry.
    #[must_use]
    pub fn after(&self, id: u32) -> Option<Option<u32>> {
        Some(self.at(self.place_of(id)? + 1))
    }

    /// The entry before this one, or before the place past the last (`None`): `Some(None)` when
    /// there is none before it. `None` when the map does not hold the entry.
    #[must_use]
    pub fn before(&self, id: Option<u32>) -> Option<Option<u32>> {
        let place = match id {
            Some(id) => self.place_of(id)?,
            None => self.nodes.len(),
        };
        Some(place.checked_sub(1).and_then(|p| self.at(p)))
    }

    /// Remove the entry with this identity: whether the map held it.
    pub fn remove_node(&mut self, id: u32) -> bool {
        match self.place_of(id) {
            Some(p) => {
                self.nodes.remove(p);
                true
            }
            None => false,
        }
    }

    /// Remove every entry with this key: how many there were.
    pub fn remove_key(&mut self, key: &Item) -> usize {
        let (lo, hi) = (self.lower_place(key), self.upper_place(key));
        self.nodes.drain(lo..hi);
        hi - lo
    }

    /// Remove every entry.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// The references the entries hold: every key's, then every value's, in order.
    // D2-04543
    #[must_use]
    pub fn references(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .nodes
            .iter()
            .filter_map(|n| n.key.reference())
            .collect();
        v.extend(self.nodes.iter().filter_map(|n| n.value.reference()));
        v
    }

    pub(crate) fn remap(&mut self, f: &mut dyn FnMut(u32) -> u32) {
        for n in &mut self.nodes {
            n.key.remap(f);
            n.value.remap(f);
        }
    }
}
