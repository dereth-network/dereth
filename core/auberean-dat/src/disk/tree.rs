//! The directory: an order-62 B-tree of entries keyed by record id, with the root held apart and a
//! hundred other nodes kept in memory.
//!
//! **Kept nodes.** A node is found among the kept ones by its first block; one not kept replaces
//! the kept node used longest ago (written first if it is to be written). A node handle
//! ([`At`]) names a slot, not a node: a slot that another node has since replaced is read as it now
//! is, exactly as the client's own pointer into its slot is.
//!
//! **Inserting** splits, on the way down, every full node it is about to enter (the root first,
//! by moving it under a new empty root), so the leaf it reaches has room. A split leaves 30
//! entries each side and moves the middle one up.
//!
//! **Removing** makes sure, on the way down, that every node it enters other than the root holds
//! more than the fewest entries: it moves an entry through the parent from a neighbor that can
//! spare one (the left first), or else merges the node with a neighbor around their separator.
//! An entry in an inner node is replaced by its neighbor from a leaf (the largest of the left
//! subtree when that child can spare one, else the smallest of the right). A merge that empties
//! the root makes the merged node the root; the removal that caused it then finds nothing and
//! fails, leaving the record (it goes on the next attempt).
//!
//! Every change is a [`Transaction`]: written to the slot, made, and cleared.

use super::alloc::Blocks;
use super::node::{DirEntry, Kept, MAX_ENTRIES, MIN_ENTRIES};
use super::{DiskError, Result, Transaction};

/// How many nodes are kept in memory besides the root.
pub const KEPT: usize = 100;

/// Where a node is held: the root, or a kept slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Root,
    Slot(usize),
}

/// The directory of one file.
#[derive(Debug)]
pub struct Directory {
    pub root: Kept,
    slots: Vec<Kept>,
    /// The use counter: each node found takes the next value as its stamp.
    clock: i32,
}

impl Default for Directory {
    fn default() -> Self {
        Self::new()
    }
}

/// `v` as an index below `n`, or a failure (a transaction or a node holding nonsense).
fn index(v: u32, n: usize) -> Result<usize> {
    let i = v as usize;
    if i < n {
        Ok(i)
    } else {
        Err(DiskError::Failed)
    }
}

/// A count as the client's signed comparisons see it, as an index.
fn count_index(n: i32) -> usize {
    usize::try_from(n.clamp(0, MAX_ENTRIES)).unwrap_or(0)
}

impl Directory {
    /// No tree yet.
    // D2-17672
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: Kept::new(),
            slots: Vec::new(),
            clock: 1,
        }
    }

    /// The kept nodes let go.
    // D2-17673
    pub fn destroy_tree(&mut self) {
        self.slots.clear();
    }

    fn fresh_slots(&mut self) {
        self.slots = vec![Kept::new(); KEPT];
    }

    /// The node at `at`.
    #[must_use]
    pub fn kept(&self, at: At) -> &Kept {
        match at {
            At::Root => &self.root,
            At::Slot(i) => &self.slots[i],
        }
    }

    fn kept_mut(&mut self, at: At) -> &mut Kept {
        match at {
            At::Root => &mut self.root,
            At::Slot(i) => &mut self.slots[i],
        }
    }

    /// A new tree: an empty root stored at the free list's head, the header pointed at it.
    ///
    /// # Errors
    /// The root's store or the header's write.
    // D2-17674
    pub fn create_tree(&mut self, b: &mut Blocks) -> Result<()> {
        self.fresh_slots();
        let at = b.info.first_free;
        self.root.offset = at;
        b.info.root = at;
        b.store_data(&self.root.node.0, at)?;
        b.save_file_info()
    }

    /// The tree a file holds: its root read from the header's root block.
    ///
    /// # Errors
    /// [`DiskError::Failed`] when the root does not load.
    // D2-17675
    pub fn load_tree(&mut self, b: &mut Blocks) -> Result<()> {
        self.fresh_slots();
        self.root.offset = b.info.root;
        let at = self.root.offset;
        if b.load_data(&mut self.root.node.0, at) {
            Ok(())
        } else {
            Err(DiskError::Failed)
        }
    }

    /// Write a node in place.
    // D2-17676
    pub fn update_node(&mut self, b: &mut Blocks, at: At) {
        let k = self.kept_mut(at);
        k.dirty = false;
        let off = k.offset;
        let _ = b.update(&k.node.0, off);
    }

    /// Write every kept node marked to be written, the root's place into the header, the header,
    /// and clear the transaction slot.
    ///
    /// # Errors
    /// The header's write.
    // D2-17677
    pub fn save_data_to_file(&mut self, b: &mut Blocks) -> Result<()> {
        for i in 0..self.slots.len() {
            if self.slots[i].offset != 0 && self.slots[i].dirty {
                self.update_node(b, At::Slot(i));
            }
        }
        if b.info.root != self.root.offset {
            b.info.root = self.root.offset;
        }
        let r = b.save_file_info();
        let _ = b.clear_transaction();
        r
    }

    /// The node whose first block is `offset`, and its slot (-1 for the root). One not kept is
    /// read into the slot used longest ago; `offset` 0 asks for an empty slot (a slot holding none
    /// is taken as it is).
    // D2-17678
    pub fn find_node(&mut self, b: &mut Blocks, offset: u32) -> (At, i32) {
        if offset == self.root.offset {
            return (At::Root, -1);
        }
        let mut oldest = 0;
        for i in 0..self.slots.len() {
            if self.slots[i].offset == offset {
                self.slots[i].stamp = self.clock;
                self.clock = self.clock.wrapping_add(1);
                return (At::Slot(i), i32::try_from(i).unwrap_or(-1));
            }
            if self.slots[i].stamp < self.slots[oldest].stamp {
                oldest = i;
            }
        }
        if self.slots[oldest].dirty {
            self.update_node(b, At::Slot(oldest));
        }
        let k = &mut self.slots[oldest];
        if offset == 0 {
            k.clear_head();
        } else {
            let _ = b.load_data(&mut k.node.0, offset);
            k.offset = offset;
        }
        k.stamp = self.clock;
        self.clock = self.clock.wrapping_add(1);
        (At::Slot(oldest), i32::try_from(oldest).unwrap_or(-1))
    }

    /// An empty node, stored at the free list's head.
    // D2-17679
    pub fn allocate_empty_node(&mut self, b: &mut Blocks) -> At {
        let (at, _) = self.find_node(b, 0);
        let first = b.info.first_free;
        let k = self.kept_mut(at);
        k.node.set_count(0);
        k.offset = first;
        let _ = b.store_data(&k.node.0, first);
        at
    }

    /// Whether the node at `at` holds `id`, by binary search; `idx` is where it is, or where it
    /// would go. A node holding more entries than a node can answers no and leaves `idx` alone.
    /// A node holding none compares the id with its first entry's place as it stands.
    // D2-17665
    pub fn has_entry(&self, id: u32, at: At, idx: &mut i32) -> bool {
        let n = &self.kept(at).node;
        if n.count() > MAX_ENTRIES {
            return false;
        }
        let mut hi = n.count() - 1;
        let mut lo = 0;
        let mut mid = 0;
        if hi >= 0 {
            let mut found = false;
            loop {
                mid = (hi + lo) / 2;
                let e = n.id(count_index(mid));
                if id == e {
                    found = true;
                    lo = hi + 1;
                } else if id < e {
                    hi = mid - 1;
                } else {
                    lo = mid + 1;
                }
                if lo > hi {
                    break;
                }
            }
            if found {
                *idx = mid;
                return true;
            }
        }
        if n.id(count_index(mid)) < id {
            mid += 1;
        }
        *idx = mid;
        false
    }

    /// Take entry `i` out of a node, the entries and child links after it moved down one, and mark
    /// the node to be written.
    // D2-17666
    pub fn extract_entry_shift(&mut self, at: At, i: usize) -> DirEntry {
        let k = self.kept_mut(at);
        let e = k.node.entry(i);
        let n = k.node.count() - 1;
        k.node.set_count(n);
        let left = count_index(n).saturating_sub(i);
        k.node.move_entries(i, i + 1, left);
        if !k.node.is_leaf() {
            k.node.move_links(i + 1, i + 2, left);
        }
        k.dirty = true;
        e
    }

    /// Take a node's first entry and its first child link, the rest moved down one.
    // D2-17667
    pub fn remove_least(&mut self, at: At) -> (DirEntry, u32) {
        let k = self.kept_mut(at);
        let e = k.node.entry(0);
        let link = k.node.next(0);
        let n = k.node.count() - 1;
        k.node.set_count(n);
        k.node.move_entries(0, 1, count_index(n));
        if !k.node.is_leaf() {
            k.node.move_links(0, 1, count_index(n) + 1);
        }
        (e, link)
    }

    /// Add an entry and a child link after a node's thirty entries.
    // D2-17668
    pub fn add_most(&mut self, at: At, e: DirEntry, link: u32) {
        let k = self.kept_mut(at);
        k.node.set_entry(MIN_ENTRIES as usize, e);
        k.node.set_count(k.node.count() + 1);
        k.node.set_next(MIN_ENTRIES as usize + 1, link);
    }

    /// Add an entry (and, in an inner node, a child link) before a node's thirty entries.
    // D2-17669
    pub fn add_least(&mut self, at: At, e: DirEntry, link: u32) {
        let k = self.kept_mut(at);
        k.node.move_entries(1, 0, MIN_ENTRIES as usize);
        k.node.set_entry(0, e);
        k.node.set_count(k.node.count() + 1);
        if !k.node.is_leaf() {
            k.node.move_links(1, 0, MIN_ENTRIES as usize + 1);
            k.node.set_next(0, link);
        }
    }

    /// A record used: kept for the last-used list, which is never in use.
    // D2-17670
    pub fn restamp_entry(&mut self, _id: u32) {}

    /// Split the full child `ul3` of node `ul1` in two: its upper thirty entries (and links) to the
    /// new node at `ul4` (allocated now unless the free list's head has already moved past it),
    /// its middle entry up into the parent, which must still hold `ul2` entries.
    ///
    /// # Errors
    /// A transaction holding nonsense.
    // D2-17680
    pub fn split_node_exec(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let idx = index(t.ul(3), MAX_ENTRIES as usize)?;
        let (p, _) = self.find_node(b, t.ul(1));
        let child_off = self.kept(p).node.next(idx);
        let (c, _) = self.find_node(b, child_off);
        let n = if b.info.first_free == t.ul(4) {
            self.allocate_empty_node(b)
        } else {
            self.find_node(b, t.ul(4)).0
        };
        if self.kept(n).node.count() != MIN_ENTRIES {
            let cn = self.kept(c).node.clone();
            let k = self.kept_mut(n);
            k.node.0[4 * 63..4 * 63 + 16 * 30]
                .copy_from_slice(&cn.0[4 * 63 + 16 * 31..4 * 63 + 16 * 61]);
            if !cn.is_leaf() {
                k.node.0[..4 * 31].copy_from_slice(&cn.0[4 * 31..4 * 62]);
            }
            k.node.set_count(MIN_ENTRIES);
            self.update_node(b, n);
        }
        let cnt = self.kept(p).node.count();
        if cnt.cast_unsigned() == t.ul(2) {
            let middle = self.kept(c).node.entry(MIN_ENTRIES as usize);
            let k = self.kept_mut(p);
            let cnt = count_index(cnt);
            if idx < cnt {
                for i in (idx + 1..=cnt).rev() {
                    k.node.set_next(i + 1, k.node.next(i));
                    k.node.set_entry(i, k.node.entry(i - 1));
                }
            }
            k.node.set_next(idx + 1, t.ul(4));
            k.node.set_entry(idx, middle);
            k.node.set_count(k.node.count() + 1);
            k.dirty = false;
            let off = k.offset;
            let _ = b.update(&k.node.0, off);
        }
        if self.kept(c).node.count() != MIN_ENTRIES {
            let k = self.kept_mut(c);
            k.node.set_count(MIN_ENTRIES);
            k.dirty = false;
            let off = k.offset;
            let _ = b.update(&k.node.0, off);
        }
        Ok(())
    }

    /// Add entry `ul1` to leaf `ul6` (which must still hold `ul7` entries) in id order, its data
    /// stored from `ul4` and dated `now`; with no data (recovery), give back the blocks stored
    /// from `ul4` instead.
    ///
    /// # Errors
    /// [`DiskError::Corrupt`] when the node is not a leaf; the store's.
    // D2-17681
    pub fn insert_entry_exec(
        &mut self,
        b: &mut Blocks,
        t: &Transaction,
        data: Option<&[u8]>,
        now: u32,
    ) -> Result<()> {
        let (leaf, _) = self.find_node(b, t.ul(6));
        if !self.kept(leaf).node.is_leaf() {
            return Err(DiskError::Corrupt);
        }
        let cnt = self.kept(leaf).node.count();
        if t.ul(7) != cnt.cast_unsigned() {
            return Ok(());
        }
        let Some(data) = data else {
            return b.store_data_rollback(t.ul(4));
        };
        let k = self.kept_mut(leaf);
        let mut i = cnt - 1;
        while i >= 0 {
            let at = count_index(i);
            if k.node.id(at) <= t.ul(1) {
                break;
            }
            k.node.set_entry(at + 1, k.node.entry(at));
            i -= 1;
        }
        let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        k.node.set_entry(
            usize::try_from(i + 1).unwrap_or(0),
            DirEntry {
                id: t.ul(1),
                offset: t.ul(4),
                size,
                date: now,
            },
        );
        k.node.set_count(cnt + 1);
        let r = b.store_data(data, t.ul(4));
        self.update_node(b, leaf);
        r
    }

    /// Replace entry `ul3` of node `ul2` (record `ul1`): the data stored from `ul7` and dated
    /// `now`, then the old chain (`ul6`) freed. With no data (recovery), whichever of the two
    /// chains the entry does not point at is freed.
    ///
    /// # Errors
    /// The free's.
    // D2-17682
    pub fn update_object_exec(
        &mut self,
        b: &mut Blocks,
        t: &Transaction,
        data: Option<&[u8]>,
        now: u32,
    ) -> Result<()> {
        let (n, _) = self.find_node(b, t.ul(2));
        let i = index(t.ul(3), MAX_ENTRIES as usize)?;
        let mut chain = t.ul(7);
        if let Some(data) = data {
            let k = self.kept_mut(n);
            let mut e = k.node.entry(i);
            e.offset = chain;
            e.size = u32::try_from(data.len()).unwrap_or(u32::MAX);
            e.date = now;
            k.node.set_entry(i, e);
            let _ = b.store_data(data, chain);
            self.update_node(b, n);
            self.restamp_entry(t.ul(1));
        }
        if chain == self.kept(n).node.entry(i).offset {
            chain = t.ul(6);
        }
        let (tail, count) = (b.info.final_free, b.info.free_count);
        b.delete_blocks(chain, tail, count, true)
    }

    /// Merge child `ul3` into its left neighbor `ul2` around their separator, entry `ul4` of
    /// parent `ul1` (which must still hold `ul5` entries); the right child's blocks freed. A root
    /// left with no entries is replaced by the merged node.
    ///
    /// # Errors
    /// A transaction holding nonsense.
    // D2-17683
    pub fn merge_nodes_exec(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let (p, _) = self.find_node(b, t.ul(1));
        let (l, ls) = self.find_node(b, t.ul(2));
        let (r, rs) = self.find_node(b, t.ul(3));
        if self.kept(p).node.count().cast_unsigned() != t.ul(5) {
            return Ok(());
        }
        let i = index(t.ul(4), MAX_ENTRIES as usize)?;
        let sep = self.extract_entry_shift(p, i);
        let rn = self.kept(r).clone();
        {
            let k = self.kept_mut(l);
            k.node.set_entry(MIN_ENTRIES as usize, sep);
            k.node.set_count(k.node.count() + 1);
            k.node.set_next(MIN_ENTRIES as usize + 1, 0);
            let lc = count_index(k.node.count());
            let rc = count_index(rn.node.count()).min(MAX_ENTRIES as usize - lc);
            for j in 0..rc {
                k.node.set_entry(lc + j, rn.node.entry(j));
            }
            for j in 0..=rc.min(61 - lc) {
                k.node.set_next(lc + j, rn.node.next(j));
            }
            k.node.set_count(k.node.count() + rn.node.count());
        }
        self.update_node(b, l);
        let (tail, count) = (b.info.final_free, b.info.free_count);
        let _ = b.delete_blocks(rn.offset, tail, count, false);
        if let Ok(rs) = usize::try_from(rs) {
            self.slots[rs].clear_head();
        }
        if p == At::Root && self.root.node.count() == 0 {
            let lk = self.kept(l).clone();
            b.info.root = lk.offset;
            let _ = b.save_file_info();
            let (tail, count) = (b.info.final_free, b.info.free_count);
            let _ = b.delete_blocks(self.root.offset, tail, count, false);
            self.root.node = lk.node;
            self.root.offset = lk.offset;
            if let Ok(ls) = usize::try_from(ls) {
                self.slots[ls].clear_head();
            }
            return Ok(());
        }
        self.update_node(b, p);
        Ok(())
    }

    /// [`Directory::merge_nodes_exec`] as a transaction: children `l` and `r` of `p` around entry
    /// `idx`.
    ///
    /// # Errors
    /// The slot's write or the merge's.
    // D2-17684
    pub fn merge_nodes_trans(
        &mut self,
        b: &mut Blocks,
        p: At,
        l: At,
        r: At,
        idx: i32,
    ) -> Result<()> {
        let t = Transaction::new(
            Transaction::MERGE,
            &[
                self.kept(p).offset,
                self.kept(l).offset,
                self.kept(r).offset,
                idx.cast_unsigned(),
                self.kept(p).node.count().cast_unsigned(),
            ],
        );
        b.save_transaction(&t)?;
        self.merge_nodes_exec(b, &t)?;
        b.clear_transaction()
    }

    /// The smallest id under a node.
    // D2-17685
    pub fn find_min(&mut self, b: &mut Blocks, mut at: At) -> u32 {
        while !self.kept(at).node.is_leaf() {
            let next = self.kept(at).node.next(0);
            at = self.find_node(b, next).0;
        }
        self.kept(at).node.id(0)
    }

    /// The largest id under a node.
    // D2-17686
    pub fn find_max(&mut self, b: &mut Blocks, mut at: At) -> u32 {
        while !self.kept(at).node.is_leaf() {
            let n = &self.kept(at).node;
            let next = n.next(count_index(n.count()));
            at = self.find_node(b, next).0;
        }
        let n = &self.kept(at).node;
        n.id(count_index(n.count() - 1))
    }

    /// Remove entry `ul3` (record `ul1`) of leaf `ul2`, its blocks freed.
    ///
    /// # Errors
    /// A transaction holding nonsense.
    // D2-17687
    pub fn delete_leaf_exec(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let (n, _) = self.find_node(b, t.ul(2));
        let i = index(t.ul(3), MAX_ENTRIES as usize)?;
        let e = self.kept(n).node.entry(i);
        if t.ul(1) == e.id {
            let (tail, count) = (b.info.final_free, b.info.free_count);
            let _ = b.delete_blocks(e.offset, tail, count, true);
            self.extract_entry_shift(n, i);
            self.update_node(b, n);
        }
        Ok(())
    }

    /// [`Directory::delete_leaf_exec`] as a transaction.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17688
    pub fn delete_leaf_trans(&mut self, b: &mut Blocks, leaf: At, idx: i32) -> Result<()> {
        let k = self.kept(leaf);
        let t = Transaction::new(
            Transaction::DELETE_LEAF,
            &[k.node.id(count_index(idx)), k.offset, idx.cast_unsigned()],
        );
        let _ = b.save_transaction(&t);
        let _ = self.delete_leaf_exec(b, &t);
        b.clear_transaction()
    }

    /// Remove entry `ul5` (record `ul1`) of inner node `ul3`, its blocks freed, by moving entry
    /// `ul6` (record `ul2`) of leaf `ul4` into its place.
    ///
    /// # Errors
    /// A transaction holding nonsense.
    // D2-17689
    pub fn delete_internal_exec(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let (n, _) = self.find_node(b, t.ul(3));
        let (leaf, _) = self.find_node(b, t.ul(4));
        let i = index(t.ul(5), MAX_ENTRIES as usize)?;
        let j = index(t.ul(6), MAX_ENTRIES as usize)?;
        let e = self.kept(n).node.entry(i);
        if t.ul(1) == e.id {
            let (tail, count) = (b.info.final_free, b.info.free_count);
            let _ = b.delete_blocks(e.offset, tail, count, true);
            let moved = self.kept(leaf).node.entry(j);
            self.kept_mut(n).node.set_entry(i, moved);
            self.update_node(b, n);
        }
        if t.ul(2) == self.kept(leaf).node.id(j) {
            self.extract_entry_shift(leaf, j);
            self.update_node(b, leaf);
        }
        Ok(())
    }

    /// Move one entry through parent `ul1` into its child `ul3` (which must still hold `ul4`
    /// entries) from the neighbor (`ul6` set: the left, else the right, which must still hold
    /// `ul5`): the neighbor's nearest entry up, the separator (record `ul2`) down. Always answers
    /// a failure, done or not.
    ///
    /// # Errors
    /// Always [`DiskError::Failed`].
    // D2-17690
    pub fn rotate_entry_exec(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let mut idx = index(t.ul(3), MAX_ENTRIES as usize)?;
        let from_left = t.ul(6) != 0;
        let (p, _) = self.find_node(b, t.ul(1));
        let child_off = self.kept(p).node.next(idx);
        let (c, _) = self.find_node(b, child_off);
        let sib_off = if from_left {
            self.kept(p).node.next(idx.wrapping_sub(1).min(61))
        } else {
            self.kept(p).node.next(idx + 1)
        };
        let (s, _) = self.find_node(b, sib_off);
        let n = self.kept(s).node.count();
        if t.ul(5) == n.cast_unsigned() {
            let (taken, link) = if from_left {
                let k = self.kept_mut(s);
                let last = count_index(n - 1);
                let link = k.node.next(count_index(n));
                k.node.set_count(n - 1);
                (k.node.entry(last), link)
            } else {
                self.remove_least(s)
            };
            if t.ul(2) == self.kept(p).node.id(idx) {
                if from_left {
                    idx = idx.wrapping_sub(1).min(60);
                }
                let k = self.kept_mut(p);
                let old = k.node.entry(idx);
                k.node.set_entry(idx, taken);
                if t.ul(4) == self.kept(c).node.count().cast_unsigned() {
                    if from_left {
                        self.add_least(c, old, link);
                    } else {
                        self.add_most(c, old, link);
                    }
                    self.update_node(b, c);
                }
                self.update_node(b, p);
            }
            self.update_node(b, s);
        }
        Err(DiskError::Failed)
    }

    /// Finish or undo the change a file's transaction slot holds, then clear the slot. An
    /// insertion's record (kind 4) is cleared as it is; a rotation answers a failure and leaves
    /// the slot.
    ///
    /// # Errors
    /// [`DiskError::UnknownTransaction`] for a kind not known; the change's.
    // D2-17691
    pub fn recover_transaction(&mut self, b: &mut Blocks, t: &Transaction) -> Result<()> {
        let r = match t.kind {
            0 | Transaction::INSERT | 12 => Ok(()),
            Transaction::INSERT_ROLLBACK => self.insert_entry_exec(b, t, None, 0),
            Transaction::DELETE_LEAF => self.delete_leaf_exec(b, t),
            Transaction::DELETE_INTERNAL => self.delete_internal_exec(b, t),
            Transaction::MERGE => self.merge_nodes_exec(b, t),
            Transaction::UPDATE => self.update_object_exec(b, t, None, 0),
            Transaction::SPLIT => self.split_node_exec(b, t),
            Transaction::ROTATE => self.rotate_entry_exec(b, t),
            _ => return Err(DiskError::UnknownTransaction),
        };
        if r.is_ok() {
            let _ = b.clear_transaction();
        }
        r
    }

    /// The entry for `id` from node `start` down, and the node it is in.
    // D2-17692
    pub fn search(&mut self, b: &mut Blocks, start: At, id: u32) -> Option<(At, usize)> {
        let mut at = start;
        let mut idx = 0;
        loop {
            if self.has_entry(id, at, &mut idx) {
                return Some((at, count_index(idx)));
            }
            if self.kept(at).node.is_leaf() {
                return None;
            }
            let next = self.kept(at).node.next(count_index(idx));
            at = self.find_node(b, next).0;
        }
    }

    /// [`Directory::split_node_exec`] as a transaction: child `idx` of `p`.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17693
    pub fn split_node_trans(&mut self, b: &mut Blocks, p: At, idx: i32) -> Result<()> {
        let k = self.kept(p);
        let t = Transaction::new(
            Transaction::SPLIT,
            &[
                k.offset,
                k.node.count().cast_unsigned(),
                idx.cast_unsigned(),
                b.info.first_free,
                b.info.free_count,
            ],
        );
        let _ = b.save_transaction(&t);
        let _ = self.split_node_exec(b, &t);
        b.clear_transaction()
    }

    /// [`Directory::insert_entry_exec`] as a transaction: `data` as record `id` in `leaf`.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17694
    pub fn insert_entry_trans(
        &mut self,
        b: &mut Blocks,
        leaf: At,
        id: u32,
        data: &[u8],
        now: u32,
    ) -> Result<()> {
        let k = self.kept(leaf);
        let t = Transaction::new(
            Transaction::INSERT,
            &[
                id,
                0,
                0,
                b.info.first_free,
                u32::try_from(data.len()).unwrap_or(u32::MAX),
                k.offset,
                k.node.count().cast_unsigned(),
            ],
        );
        let _ = b.save_transaction(&t);
        let _ = self.insert_entry_exec(b, &t, Some(data), now);
        b.clear_transaction()
    }

    /// [`Directory::update_object_exec`] as a transaction: record `id` in node `at` replaced by
    /// `data`.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17695
    pub fn update_data_trans(
        &mut self,
        b: &mut Blocks,
        at: At,
        id: u32,
        data: &[u8],
        now: u32,
    ) -> Result<()> {
        let mut idx = 0;
        self.has_entry(id, at, &mut idx);
        let k = self.kept(at);
        let t = Transaction::new(
            Transaction::UPDATE,
            &[
                id,
                k.offset,
                idx.cast_unsigned(),
                0,
                0,
                k.node.entry(count_index(idx)).offset,
                b.info.first_free,
            ],
        );
        let _ = b.save_transaction(&t);
        let _ = self.update_object_exec(b, &t, Some(data), now);
        b.clear_transaction()
    }

    /// Add record `id` (not in the tree): down from the root, splitting every full node before
    /// entering it, to the leaf it belongs in.
    ///
    /// # Errors
    /// The insertion's.
    // D2-17696
    pub fn descend_to_add(&mut self, b: &mut Blocks, id: u32, data: &[u8], now: u32) -> Result<()> {
        if self.root.node.count() == MAX_ENTRIES {
            let fresh = self.allocate_empty_node(b);
            let At::Slot(k) = fresh else {
                return Err(DiskError::Failed);
            };
            core::mem::swap(&mut self.root, &mut self.slots[k]);
            let old = self.slots[k].offset;
            self.root.node.set_next(0, old);
            b.info.root = self.root.offset;
            let _ = b.save_file_info();
            let _ = self.split_node_trans(b, At::Root, 0);
        }
        let mut cur = At::Root;
        let mut link = self.root.node.next(0);
        loop {
            if link == 0 {
                return self.insert_entry_trans(b, cur, id, data, now);
            }
            let n = &self.kept(cur).node;
            let mut i = n.count() - 1;
            while i >= 0 && n.id(count_index(i)) > id {
                i -= 1;
            }
            let next = n.next(count_index(i + 1));
            let mut child = self.find_node(b, next).0;
            if self.kept(child).node.count() == MAX_ENTRIES {
                let _ = self.split_node_trans(b, cur, i + 1);
                if self.kept(cur).node.id(count_index(i + 1)) < id {
                    let next = self.kept(cur).node.next(count_index(i + 2));
                    child = self.find_node(b, next).0;
                }
            }
            link = self.kept(child).node.next(0);
            cur = child;
        }
    }

    /// [`Directory::delete_internal_exec`] as a transaction.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17697
    #[allow(clippy::too_many_arguments)]
    pub fn delete_internal_trans(
        &mut self,
        b: &mut Blocks,
        id: u32,
        moved: u32,
        node: u32,
        leaf: u32,
        idx: i32,
        leaf_idx: i32,
    ) -> Result<()> {
        let t = Transaction::new(
            Transaction::DELETE_INTERNAL,
            &[
                id,
                moved,
                node,
                leaf,
                idx.cast_unsigned(),
                leaf_idx.cast_unsigned(),
            ],
        );
        let _ = b.save_transaction(&t);
        let _ = self.delete_internal_exec(b, &t);
        b.clear_transaction()
    }

    /// [`Directory::rotate_entry_exec`] as a transaction.
    ///
    /// # Errors
    /// The slot's clearing.
    // D2-17698
    pub fn rotate_entry_trans(
        &mut self,
        b: &mut Blocks,
        p: At,
        idx: i32,
        child_count: i32,
        sib_count: i32,
        from_left: bool,
    ) -> Result<()> {
        let k = self.kept(p);
        let t = Transaction::new(
            Transaction::ROTATE,
            &[
                k.offset,
                k.node.id(count_index(idx)),
                idx.cast_unsigned(),
                child_count.cast_unsigned(),
                sib_count.cast_unsigned(),
                u32::from(from_left),
            ],
        );
        let _ = b.save_transaction(&t);
        let _ = self.rotate_entry_exec(b, &t);
        b.clear_transaction()
    }

    /// The entry for `id`, and the node it is in; none for id 0 or an empty tree.
    // D2-17699
    pub fn lookup(&mut self, b: &mut Blocks, id: u32) -> Option<(At, usize)> {
        if id != 0 && self.root.node.count() != 0 {
            return self.search(b, At::Root, id);
        }
        None
    }

    /// Find `id` from `start` down for removing it, making each node entered (but `start`) hold
    /// more than the fewest entries on the way: an entry moved in from a neighbor that can spare
    /// one (the left first), else a merge with a neighbor.
    ///
    /// # Errors
    /// [`DiskError::Failed`] when `id` is not found.
    // D2-17700
    pub fn descend_to_delete(&mut self, b: &mut Blocks, start: At, id: u32) -> Result<(At, i32)> {
        let mut at = start;
        let mut left: Option<At> = None;
        let mut right: Option<At> = None;
        let mut idx: i32 = -1;
        loop {
            if self.has_entry(id, at, &mut idx) {
                return Ok((at, idx));
            }
            if self.kept(at).node.is_leaf() {
                return Err(DiskError::Failed);
            }
            let i = idx;
            let next = self.kept(at).node.next(count_index(i));
            let mut child = self.find_node(b, next).0;
            let cc = self.kept(child).node.count();
            if cc <= MIN_ENTRIES {
                let mut done = false;
                if i > 0 {
                    let off = self.kept(at).node.next(count_index(i - 1));
                    let l = self.find_node(b, off).0;
                    left = Some(l);
                    let lc = self.kept(l).node.count();
                    if lc > MIN_ENTRIES {
                        let _ = self.rotate_entry_trans(b, at, i, cc, lc, true);
                        done = true;
                    }
                }
                if !done && i < self.kept(at).node.count() {
                    let off = self.kept(at).node.next(count_index(i + 1));
                    let r = self.find_node(b, off).0;
                    right = Some(r);
                    let rc = self.kept(r).node.count();
                    if rc > MIN_ENTRIES {
                        let _ = self.rotate_entry_trans(b, at, i, cc, rc, false);
                        done = true;
                    }
                }
                if !done {
                    match right {
                        Some(r) if i < self.kept(at).node.count() => {
                            let _ = self.merge_nodes_trans(b, at, child, r, i);
                        }
                        _ => {
                            if let Some(l) = left {
                                if i > 0 {
                                    let _ = self.merge_nodes_trans(b, at, l, child, i - 1);
                                    child = l;
                                }
                            }
                        }
                    }
                }
            }
            at = child;
        }
    }

    /// The ids from `lo` to `hi` under a node, each node's entries visited before the child to
    /// their left; a node's entries are read from its slot as it stands after each child.
    // D2-17701
    pub fn build_id_list_range(
        &mut self,
        b: &mut Blocks,
        list: &mut Vec<u32>,
        at: At,
        lo: u32,
        hi: u32,
    ) {
        let cnt = self.kept(at).node.count();
        if lo > hi || cnt >= 62 {
            return;
        }
        let mut idx = 0;
        self.has_entry(lo, at, &mut idx);
        let mut cnt = cnt;
        if idx > cnt {
            return;
        }
        let mut i = idx;
        loop {
            let mut id = lo;
            if i < cnt {
                id = self.kept(at).node.id(count_index(i));
                if lo <= id && id <= hi {
                    list.push(id);
                }
            }
            if !self.kept(at).node.is_leaf() {
                let next = self
                    .kept(at)
                    .node
                    .next(usize::try_from(i).unwrap_or(0).min(61));
                let child = self.find_node(b, next).0;
                self.build_id_list_range(b, list, child, lo, hi);
            }
            if hi < id {
                return;
            }
            cnt = self.kept(at).node.count();
            i += 1;
            if i > cnt {
                return;
            }
        }
    }

    /// Every id under a node: the node's own, then each child's in turn (the node found again
    /// when its slot was taken meanwhile).
    // D2-17702
    pub fn build_id_list(&mut self, b: &mut Blocks, list: &mut Vec<u32>, mut at: At) {
        if self.kept(at).node.count() >= 62 {
            return;
        }
        let home = self.kept(at).offset;
        let mut i = 0;
        while i < self.kept(at).node.count() {
            list.push(self.kept(at).node.id(count_index(i)));
            i += 1;
        }
        if self.kept(at).node.is_leaf() || self.kept(at).node.count() < 0 {
            return;
        }
        let mut i = 0usize;
        loop {
            let next = self.kept(at).node.next(i.min(61));
            let child = self.find_node(b, next).0;
            self.build_id_list(b, list, child);
            if self.kept(at).offset != home {
                at = self.find_node(b, home).0;
            }
            i += 1;
            if i > count_index(self.kept(at).node.count()) {
                return;
            }
        }
    }

    /// Remove record `id` from the tree under `start`.
    ///
    /// # Errors
    /// [`DiskError::Failed`] when it is not found (or not found again after a merge that emptied
    /// the root); the change's.
    // D2-17703
    pub fn delete(&mut self, b: &mut Blocks, id: u32, start: At) -> Result<()> {
        let mut found = self.descend_to_delete(b, start, id);
        loop {
            let (node, idx) = found?;
            if self.kept(node).node.is_leaf() {
                return self.delete_leaf_trans(b, node, idx);
            }
            let i = count_index(idx);
            let off = self.kept(node).node.next(i);
            let left = self.find_node(b, off).0;
            if self.kept(left).node.count() > MIN_ENTRIES {
                let max = self.find_max(b, left);
                let (leaf, li) = self.descend_to_delete(b, left, max).unwrap_or((left, -1));
                let (n_off, l_off) = (self.kept(node).offset, self.kept(leaf).offset);
                return self.delete_internal_trans(b, id, max, n_off, l_off, idx, li);
            }
            let off = self.kept(node).node.next(i + 1);
            let right = self.find_node(b, off).0;
            if self.kept(right).node.count() > MIN_ENTRIES {
                let min = self.find_min(b, right);
                let (leaf, li) = self.descend_to_delete(b, right, min).unwrap_or((right, -1));
                let (n_off, l_off) = (self.kept(node).offset, self.kept(leaf).offset);
                return self.delete_internal_trans(b, id, min, n_off, l_off, idx, li);
            }
            self.merge_nodes_trans(b, node, left, right, idx)?;
            found = self.descend_to_delete(b, left, id);
        }
    }

    /// Remove record `id`; whether it went.
    // D2-17704
    pub fn remove(&mut self, b: &mut Blocks, id: u32) -> bool {
        self.delete(b, id, At::Root).is_ok()
    }

    /// The ids from `lo` to `hi` (see [`Directory::build_id_list_range`]); whether there are any.
    // D2-17706
    pub fn get_id_list_range(
        &mut self,
        b: &mut Blocks,
        list: &mut Vec<u32>,
        lo: u32,
        hi: u32,
    ) -> bool {
        list.clear();
        if self.root.node.count() != 0 {
            self.build_id_list_range(b, list, At::Root, lo, hi);
        }
        !list.is_empty()
    }

    /// Make room by removing the record used longest ago: with the last-used list never in use,
    /// nothing is removed.
    // D2-17707
    pub fn try_delete_oldest(&mut self, _b: &mut Blocks) -> bool {
        false
    }
}
