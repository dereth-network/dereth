//! The world-object manager's nine tables, null-object placeholders, lost-cell table and 25 s
//! deferred-destruction queue.
//!
//! Three constants are observable and must be exact:
//! **25.0 s** destruction delay, **20.0 s** force-objdesc retry, **1.0 s** visible-list rebuild.
//! An item dropped and re-picked-up inside 25 s never re-downloads, and players notice.

use crate::objmap::ObjMap;
use dereth_primitives::{CellId, ObjectId, ServerTime};
use std::cmp::Reverse;
use std::collections::BTreeSet;

/// The destruction delay — hard-coded.
pub const DESTRUCTION_DELAY: f64 = 25.0;
/// The object-maintenance sweep's re-check tolerance, `0.00019999999494757503`.
///
/// The literal is an `f32` 2e-4 widened to `double`, which is why it is not exactly `2e-4`.
pub const DESTRUCTION_RECHECK_EPSILON: f64 = 0.000_199_999_994_947_575_03;
/// The null-table sweep interval: every 20 s an unresolved placeholder is re-requested.
pub const FORCE_OBJDESC_INTERVAL: f64 = 20.0;
/// `World::update_visible_object_list` runs at most once per second.
pub const VISIBLE_REBUILD_INTERVAL: f64 = 1.0;

/// The object inventory — the **reverse** index, and only for containers the client is
/// actively viewing.
///
/// Keeping this optional and scoped to "currently viewing" is load-bearing: a permanent
/// parent→children index changes when objects get destroyed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectInventory {
    pub container: ObjectId,
    /// Non-container contents, **ordered**; the index is the "place in list" the server and
    /// client agree on.
    pub items: Vec<ObjectId>,
    /// Side packs, ordered.
    pub containers: Vec<ObjectId>,
    /// For a creature, the ordered wielded/worn set.
    pub placements: Vec<InventoryPlacement>,
}

/// One inventory placement — an object id, its location and its priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryPlacement {
    pub iid: ObjectId,
    pub loc: u32,
    pub priority: u32,
}

/// Behavior: a **free function**, not a
/// method, and it returns whichever of the two wins.
///
/// ```text
/// pb = b.priority; ma = a.loc & mask; mb = b.loc & mask; pa = a.priority;
/// if (ma && !pa && ma > 0x1FF && ma < 0x4001) pa = 0x7F;
/// if (mb && !pb) {
///     if (mb < 0x200)   return a;
///     if (mb > 0x4000)  return a;
///     pb = 0x7F;
/// }
/// return (pa < pb) ? b : a;
/// ```
///
/// The `0x7F` promotion is the layering rule and it applies only inside the **armour band**,
/// `0x200`..`0x4000` — the nine armour-coverage bits. A placement the server left at priority 0
/// that covers an armour bit is treated as `0x7F`, i.e. above every ordinary clothing priority, so
/// a breastplate wins the chest colour over a shirt. Outside that band a zero priority stays zero.
/// Ties go to `a`, which is the list entry, so the **first** placement in list order wins among
/// equals — which is what makes the accumulator's all-zero start work in
/// [`ObjectInventory::upper_inv_obj`].
#[must_use]
pub fn determine_higher_priority(
    a: InventoryPlacement,
    b: InventoryPlacement,
    mask: u32,
) -> InventoryPlacement {
    let mut pb = b.priority;
    let ma = a.loc & mask;
    let mb = b.loc & mask;
    let mut pa = a.priority;
    if ma != 0 && pa == 0 && ma > 0x1FF && ma < 0x4001 {
        pa = 0x7F;
    }
    if mb != 0 && pb == 0 {
        if !(0x200..=0x4000).contains(&mb) {
            return a;
        }
        pb = 0x7F;
    }
    if pa < pb {
        b
    } else {
        a
    }
}

impl ObjectInventory {
    #[must_use]
    pub fn new(container: ObjectId) -> Self {
        Self {
            container,
            ..Self::default()
        }
    }

    /// The place-in-items-list / place-in-containers-list queries — index, or `None`.
    ///
    /// The client asks **one** list; this asks both. Use [`Self::place_in_list`] where the choice of
    /// list is itself part of the behaviour being reproduced.
    #[must_use]
    pub fn place_of(&self, id: ObjectId) -> Option<usize> {
        self.items
            .iter()
            .position(|x| *x == id)
            .or_else(|| self.containers.iter().position(|x| *x == id))
    }

    /// Return the index in either the items list **or** the containers list — whichever the caller
    /// names — and `None` for the client's `-1`.
    ///
    /// The caller picks between the two with the child's own
    /// [`crate::weenie::Weenie::goes_in_containers_list`] predicate, and the answer becomes the
    /// `place` argument of the server's move-item notice. Asking the wrong list returns an index
    /// into the other one, which is a silently wrong slot.
    #[must_use]
    pub fn place_in_list(&self, id: ObjectId, containers_list: bool) -> Option<usize> {
        let list = if containers_list {
            &self.containers
        } else {
            &self.items
        };
        list.iter().position(|x| *x == id)
    }

    /// The location of `id` on this object — scans [`Self::placements`].
    #[must_use]
    pub fn location_on_object(&self, id: ObjectId) -> u32 {
        self.placements
            .iter()
            .find(|p| p.iid == id)
            .map_or(0, |p| p.loc)
    }

    /// The object-at-location query `(location mask, priority)`.
    #[must_use]
    pub fn object_at_location(&self, loc_mask: u32, priority: u32) -> Option<ObjectId> {
        self.placements
            .iter()
            .find(|p| p.loc & loc_mask != 0 && p.priority == priority)
            .map(|p| p.iid)
    }

    /// The paper doll's upper-inventory-object walk — the **topmost** placed item covering
    /// `loc_mask`, or `None` when nothing covers it.
    ///
    /// ```text
    /// loc == 0                         -> 0
    /// best = { iid: 0, loc: 0, priority: 0 }
    /// for each placement p covering loc: best = determine_higher_priority(p, best, loc)
    /// best.iid non-zero                -> best.iid
    /// otherwise                        -> the current player's id   // the fallback
    /// ```
    ///
    /// **The player fallback is the caller's**, because this type is one object's inventory and
    /// knows nothing about the smart box. `None` here is the client's "nothing over that colour",
    /// which the doll answers with the player's own id — clicking a bare shoulder selects you.
    ///
    /// The accumulator starts all-zero, so the first matching placement always wins against it:
    /// `determine_higher_priority` returns `a` on a tie and `best.priority` is 0.
    #[must_use]
    pub fn upper_inv_obj(&self, loc_mask: u32) -> Option<ObjectId> {
        if loc_mask == 0 {
            return None;
        }
        let mut best = InventoryPlacement {
            iid: ObjectId(0),
            loc: 0,
            priority: 0,
        };
        for p in &self.placements {
            if loc_mask & p.loc != 0 {
                best = determine_higher_priority(*p, best, loc_mask);
            }
        }
        (best.iid.0 != 0).then_some(best.iid)
    }

    /// Every id in both content lists, items first.
    #[must_use]
    pub fn all_contents(&self) -> Vec<ObjectId> {
        let mut v = self.items.clone();
        v.extend(self.containers.iter().copied());
        v
    }

    /// Insert `child` into the right list at `place`, **unless the
    /// child is already in that list**. Returns whether it inserted.
    ///
    /// The clamp is the client's: `place = min(place, id count)` before inserting into the list.
    ///
    /// **What the already-in-list guard actually does, measured rather than assumed.** It is not
    /// what makes the server-says-contain pre-placement survive the item's
    /// `0xF745`: removing it as a mutation changes **nothing** across all 29 recorded cases, and the
    /// trace says why: the description update assigns the new `pwd` *before* it calls the move
    /// handler, so the move handler reads the **new** container out of `pwd` and
    /// [`Self::remove_content`] takes the pre-placed id out again — and then this puts it back at
    /// the index the description update had already read off the list. The remove/re-add is a round trip and
    /// the guard never sees a duplicate to reject.
    ///
    /// It is kept because it is the client's first act here and because it is not dead in the
    /// client: the remove picks its list from the item's own predicate while
    /// the server-says-contain path picks from the server's `containerProperties`, and when those two
    /// disagree the remove misses and this guard is the only thing standing between the id and
    /// membership of both lists. They agree on all 29 corpus cases, so the branch is faithful and
    /// currently unfalsifiable here.
    pub fn add_content(&mut self, child: ObjectId, is_container: bool, place: usize) -> bool {
        let list = if is_container {
            &mut self.containers
        } else {
            &mut self.items
        };
        if list.contains(&child) {
            return false;
        }
        let at = place.min(list.len());
        list.insert(at, child);
        true
    }

    /// Insert `id` at ordinal `num`, appending when that node does not exist, which is the whole
    /// body of the weenie's server-says-contain.
    ///
    /// Deliberately **without** [`Self::add_content`]'s already-in-list guard and without its
    /// clamp, because the server-says-contain path has neither. The client walks to node `num`; when there is no
    /// such node and `num` is not the id count, the append-at-end flag sends it to the append path.
    /// Inserting at `min(num, len)` produces the same list in both branches.
    pub fn add_at_num(&mut self, child: ObjectId, containers_list: bool, num: usize) {
        let list = if containers_list {
            &mut self.containers
        } else {
            &mut self.items
        };
        let at = num.min(list.len());
        list.insert(at, child);
    }

    /// Remove `child` from whichever list holds it.
    pub fn remove_content(&mut self, child: ObjectId) -> bool {
        if let Some(i) = self.items.iter().position(|x| *x == child) {
            self.items.remove(i);
            return true;
        }
        if let Some(i) = self.containers.iter().position(|x| *x == child) {
            self.containers.remove(i);
            return true;
        }
        false
    }

    /// The wield-location setter's list maintenance: remove the old entry, then insert the
    /// new one **at the head**, keeping the list ordered by clothing priority
    /// (the placement priority rule).
    pub fn set_placement(&mut self, iid: ObjectId, loc: u32, priority: u32) {
        self.placements.retain(|p| p.iid != iid);
        if loc != 0 {
            self.placements
                .insert(0, InventoryPlacement { iid, loc, priority });
            self.placements
                .sort_by_key(|p| std::cmp::Reverse(p.priority));
        }
    }
}

/// The lost-cell table — objects parked because their cell is not loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LostCell {
    pub cell: CellId,
    pub objects: Vec<ObjectId>,
}

/// The nine world-object tables, with the client bucket counts.
///
/// `object_table` and `null_object_table` hold *physics* objects, which the physics crate owns.
/// This crate keeps only the membership — the ids — because the null-object machinery and the
/// destruction
/// cascade are decided on membership and on "does the physics object have a cell", nothing more.
#[derive(Debug)]
pub struct ObjectTables {
    /// `weenie_object_table` — every game object that exists. 0x80 buckets.
    pub weenies: ObjMap<ObjectId, crate::weenie::Weenie>,
    /// `null_weenie_object_table` — placeholders. 0x10 buckets.
    pub null_weenies: ObjMap<ObjectId, NullPlaceholder>,
    /// `object_table` — physics objects. 0x80 buckets.
    pub physics: ObjMap<ObjectId, PhysicsPresence>,
    /// `null_object_table`. 0x10 buckets.
    pub null_physics: ObjMap<ObjectId, NullPlaceholder>,
    /// `object_inventory_table` — only while viewing. 0x20 buckets.
    pub inventories: ObjMap<ObjectId, ObjectInventory>,
    /// `lost_cell_table`. 0x80 buckets.
    pub lost_cells: ObjMap<CellId, LostCell>,
    /// `visible_object_table` — rebuilt at 1 Hz. A `BTreeSet` is correct here: the client
    /// clears and refills it wholesale and the only reader asks "is this id in it".
    pub visible: BTreeSet<ObjectId>,
    /// `destruction_object_table` — id → scheduled time. 0x40 buckets.
    pub doomed: ObjMap<ObjectId, ServerTime>,
    /// `object_destruction_queue` — the min-heap of `(time, id)`.
    pub doom_queue: DoomQueue,
}

/// The static physics-state bit — bit **0**, value `1` (`PhysicsState`).
///
/// This is the bit both of this crate's `state` readers test, and it is **not** `HIDDEN_PS`.
/// The visible-object sweep tests `state & 1` on the physics object, and the next-selection
/// policy tests `state & 1` on the same word. `HIDDEN_PS` is
/// `0x4000` (16384), and neither site reads it.
///
/// The distinction is not cosmetic: it decides what the predicate *means*. A static object is one
/// the world owns and never moves — scenery, a wall lamp, a lifestone — and retail's visible sweep
/// excludes it because there is nothing about it to maintain, not because it is invisible. Naming
/// the reader "hidden" invites the reading that an object the server hid drops out of the sweep,
/// which it does not: it stays visible-listed and is refused later, by
/// [`crate::selection::CLOAKED_PS`] in the selection cycle and by the draw path's `NODRAW_PS`.
pub const STATIC_PS: u32 = 0x0000_0001;

/// What this crate keeps of a physics object: enough for the destruction cascade and the visible
/// sweep, and nothing that belongs to the physics crate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicsPresence {
    /// `None` when the object is in no loaded cell.
    pub cell: Option<CellId>,
    /// The **whole** physics-state word the descriptor carried, not one bit of it.
    ///
    /// Two readers in this crate ask the bit-0 question and both go through [`Self::is_static`];
    /// see [`STATIC_PS`] for which bit that is and why the name matters.
    ///
    /// **This is the single source of the state word.**
    ///
    /// In the client there is exactly one: the world controller's state update calls
    /// the physics body's state setter, and the visible-list rebuild reads the result on its
    /// next 1 Hz pass. A create-time-only word would miss `0xF74B Item_SetState`, so a `0xF74B`
    /// that set or cleared `STATIC_PS` would move retail's visible list and not this one — a
    /// visibility defect, not a cosmetic one, because both readers here test bit 0. It has
    /// exactly two producers: the physics descriptor at create and merge, and the physics-state
    /// update path, called from `dereth_client_runtime::objects::ObjectStream::set_state` past
    /// that handler's `update_times[STATE_TS]` gate.
    ///
    /// **Why the source is this table and not the runtime's.** Both of the retail readers of
    /// bit 0 are methods of this crate's [`crate::World`], and this crate cannot depend on the
    /// client runtime; a word stored only over there would be invisible to them.
    ///
    /// **There is no second copy.** The runtime reaches this word through
    /// `dereth_client_runtime::objects::ObjectStream::physics_state` or straight off
    /// `ObjectStream::world`, and `ObjectPhysics::sync` is handed this table by parameter rather
    /// than trusting a copy. A copy could only be proved faithful by sampling the places
    /// somebody thought of; not storing one is the exhaustive form of the same assertion.
    pub state: u32,
    /// The parent this object is attached to, if any.
    pub parent: Option<(ObjectId, u32)>,
    /// Selected part-array asset; application-resolved initialization facts live on World.
    pub setup_id: u32,
}

impl PhysicsPresence {
    /// Test `state & STATIC_PS`, using [`STATIC_PS`], the question both the visible-object sweep and next-selection
    /// policy ask, named for the bit it reads.
    #[must_use]
    pub fn is_static(&self) -> bool {
        self.state & STATIC_PS != 0
    }
}

/// A null-table placeholder: an id referenced before its description arrived.
#[derive(Debug, Clone, Default)]
pub struct NullPlaceholder {
    /// `update_time`, stamped at creation and again on every 20 s re-request.
    pub update_time: ServerTime,
    /// Blobs parked on this object, replayed in `SequenceGate` sequence order once it becomes real
    /// (the weenie object's queued-blob replay).
    pub queued_blobs: Vec<QueuedBlob>,
    /// Physical-null attachment survives the placeholder's initialisation. Unused for
    /// Weenie-only nulls.
    pub parent: Option<(ObjectId, u32)>,
}

/// Asset-independent result of the part-array initialisation. The application resolves DATs;
/// an absent cache entry means unresolved, not permission to accept an attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicsSetupFacts {
    Failed,
    Ready {
        holding_locations: std::collections::BTreeSet<u32>,
    },
}

/// One parked blob. The bytes belong to the protocol crate; the ordering is this crate's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedBlob {
    /// The `SequenceGate` sequence the ordered variant keys on; `None` for the unordered variant.
    pub sequence: Option<u32>,
    pub payload: Vec<u8>,
}

/// One destruction-heap entry. Retail compares only `when`, never the object id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoomEntry {
    pub when: f64,
    pub id: ObjectId,
}

/// The destruction priority queue's operations, not Rust's heap tie policy.
/// The insert leaves equal keys in place; the remove-min replaces the root with the
/// last node; the sift-down chooses left before right and swaps only on a strict decrease.
/// Equal deadlines can affect a container's subsequent child scheduling, so their order matters.
#[derive(Debug, Default)]
pub struct DoomQueue(Vec<Reverse<DoomEntry>>);

impl DoomQueue {
    pub fn push(&mut self, entry: Reverse<DoomEntry>) {
        self.0.push(entry);
        let mut i = self.0.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 2;
            if self.0[parent].0.when <= self.0[i].0.when {
                break;
            }
            self.0.swap(i, parent);
            i = parent;
        }
    }

    #[must_use]
    pub fn peek(&self) -> Option<&Reverse<DoomEntry>> {
        self.0.first()
    }
    /// Entries in the heap, stale rows included. Instrumentation.
    ///
    /// The doomed-object schedule is a lazy heap: [`crate::World`] can stop
    /// caring about an id (a cancelled deadline, a re-entered cell) without the heap's copy
    /// being removed, and the pop-time check against `tables.doomed` is what makes that safe.
    /// Safe is not the same as bounded, so a long-session station has to be able to read the
    /// heap's own length and not only `doomed.len()`: a cycle that queues one deadline and
    /// cancels it leaves `doomed` at zero and could leave this climbing for ever.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the heap holds nothing at all. Instrumentation.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn pop(&mut self) -> Option<Reverse<DoomEntry>> {
        if self.0.is_empty() {
            return None;
        }
        let first = self.0.swap_remove(0);
        let mut i = 0;
        loop {
            let left = 2 * i + 1;
            if left >= self.0.len() {
                break;
            }
            let mut smallest = i;
            if self.0[left].0.when < self.0[smallest].0.when {
                smallest = left;
            }
            let right = left + 1;
            if right < self.0.len() && self.0[right].0.when < self.0[smallest].0.when {
                smallest = right;
            }
            if smallest == i {
                break;
            }
            self.0.swap(i, smallest);
            i = smallest;
        }
        Some(first)
    }
}

impl Default for ObjectTables {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectTables {
    #[must_use]
    pub fn new() -> Self {
        Self {
            weenies: ObjMap::with_buckets(0x80),
            null_weenies: ObjMap::with_buckets(0x10),
            physics: ObjMap::with_buckets(0x80),
            null_physics: ObjMap::with_buckets(0x10),
            inventories: ObjMap::with_buckets(0x20),
            lost_cells: ObjMap::with_buckets(0x80),
            visible: BTreeSet::new(),
            doomed: ObjMap::with_buckets(0x40),
            doom_queue: DoomQueue::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered world-object behavior §4's bucket-count table.
    #[test]
    fn the_tables_have_the_retail_bucket_counts() {
        let t = ObjectTables::new();
        assert_eq!(t.weenies.num_buckets(), 0x80);
        assert_eq!(t.null_weenies.num_buckets(), 0x10);
        assert_eq!(t.physics.num_buckets(), 0x80);
        assert_eq!(t.null_physics.num_buckets(), 0x10);
        assert_eq!(t.inventories.num_buckets(), 0x20);
        assert_eq!(t.lost_cells.num_buckets(), 0x80);
        assert_eq!(t.doomed.num_buckets(), 0x40);
    }

    /// Oracle: §13 — the three constants are observable and must be exact.
    #[test]
    fn the_three_observable_constants_are_exact() {
        assert_eq!(DESTRUCTION_DELAY, 25.0);
        assert_eq!(FORCE_OBJDESC_INTERVAL, 20.0);
        assert_eq!(VISIBLE_REBUILD_INTERVAL, 1.0);
        // The re-check tolerance is an f32 2e-4 widened to double, not the decimal 2e-4.
        assert_eq!(DESTRUCTION_RECHECK_EPSILON, f64::from(2e-4f32));
    }

    /// Oracle: the inventory placement list — a new placement is inserted at the head and the list
    /// kept in clothing-priority
    /// order, because the paperdoll draw order depends on it.
    #[test]
    fn placements_are_kept_in_priority_order() {
        let mut inv = ObjectInventory::new(ObjectId(1));
        inv.set_placement(ObjectId(10), 0x1, 5);
        inv.set_placement(ObjectId(11), 0x2, 9);
        inv.set_placement(ObjectId(12), 0x4, 1);
        assert_eq!(
            inv.placements.iter().map(|p| p.iid).collect::<Vec<_>>(),
            vec![ObjectId(11), ObjectId(10), ObjectId(12)]
        );
        assert_eq!(inv.location_on_object(ObjectId(11)), 0x2);
        // loc 0 removes.
        inv.set_placement(ObjectId(11), 0, 9);
        assert_eq!(inv.location_on_object(ObjectId(11)), 0);
        assert_eq!(inv.placements.len(), 2);
    }

    #[test]
    fn content_lists_are_ordered_and_split_by_kind() {
        let mut inv = ObjectInventory::new(ObjectId(1));
        inv.add_content(ObjectId(10), false, 0);
        inv.add_content(ObjectId(11), false, 0);
        inv.add_content(ObjectId(12), true, 0);
        assert_eq!(inv.items, vec![ObjectId(11), ObjectId(10)]);
        assert_eq!(inv.containers, vec![ObjectId(12)]);
        assert_eq!(inv.place_of(ObjectId(10)), Some(1));
        assert!(inv.remove_content(ObjectId(11)));
        assert_eq!(inv.items, vec![ObjectId(10)]);
    }

    /// Static ps is bit zero and is not hidden ps.
    #[test]
    fn static_ps_is_bit_zero_and_is_not_hidden_ps() {
        assert_eq!(STATIC_PS, 1, "PhysicsState::STATIC_PS");
        const HIDDEN_PS: u32 = 16384;
        assert_eq!(HIDDEN_PS, 0x4000);
        assert_ne!(
            STATIC_PS, HIDDEN_PS,
            "they are different bits, which is the whole finding"
        );

        // An object the server has *hidden* is not static, and `update_visible_object_list`
        // therefore keeps it in the visible list -- which is the behaviour the old name denied.
        let hidden = PhysicsPresence {
            state: HIDDEN_PS,
            ..PhysicsPresence::default()
        };
        assert!(!hidden.is_static());
        let stat = PhysicsPresence {
            state: STATIC_PS,
            ..PhysicsPresence::default()
        };
        assert!(stat.is_static());
        // ...and the mask is a mask: a word carrying both answers the STATIC question.
        let both = PhysicsPresence {
            state: STATIC_PS | HIDDEN_PS,
            ..PhysicsPresence::default()
        };
        assert!(both.is_static());
    }

    /// The state word carries the bits select next reads.
    #[test]
    fn the_state_word_carries_the_bits_select_next_reads() {
        const CLOAKED_PS: u32 = 0x0010_0000;
        const REPORT_COLLISIONS_AS_ENVIRONMENT_PS: u32 = 0x0020_0000;
        assert_eq!(CLOAKED_PS, 1_048_576);
        assert_eq!(REPORT_COLLISIONS_AS_ENVIRONMENT_PS, 2_097_152);
        let p = PhysicsPresence {
            state: CLOAKED_PS | REPORT_COLLISIONS_AS_ENVIRONMENT_PS,
            ..PhysicsPresence::default()
        };
        assert_ne!(p.state & CLOAKED_PS, 0);
        assert_ne!(p.state & REPORT_COLLISIONS_AS_ENVIRONMENT_PS, 0);
        assert!(!p.is_static(), "neither of them is bit 0");
    }
}
