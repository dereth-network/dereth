//! Descriptor-slot lifetime for the shader-visible heap, and the reference-counted texture table
//! that drives it.
//!
//! # Why this module exists
//!
//! Taking each texture's SRV pair from a monotonic counter that nothing ever reclaimed would make
//! a session's descriptor use the *sum* of every texture it had ever seen rather than the number
//! it was holding. Three ordinary sessions reach that ceiling independently: Holtburg terrain
//! at `--land-radius 3` costs 504 pairs, a session that visits the datapatch, intro, character
//! management and gameplay screens uploads ~129 UI images, and per-object appearance costs ~10.6
//! pairs per distinct outfit. Landblock streaming made it a leak rather than a budget: blocks left
//! the window and their surfaces' descriptors stayed taken.
//!
//! # The model
//!
//! The original combined-texture cache uses reference counting. Its verified lookup and
//! insertion sequence is:
//!
//! ```text
//! key = (palette DID as u64) << 32 | indexed texture DID     // both zero if not a dat object
//! if key != 0 and shared_table[key] exists: add a reference and return it
//! t = a new texture
//! if key == 0: uncached_table.add(t)                        // uncached, freed by refcount
//! else: t.texture_code = key; shared_table.add(key, t)
//! ```
//!
//! and release removes the object from
//! the shared table by its texture code when that code is non-zero and from the uncached table
//! otherwise. The count itself is the object's link count, decremented on release,
//! which destroys the object when it reaches zero. [`TextureTable`] is that pair of tables and that
//! count; [`DescriptorAllocator`] is the D3D12-only half — the original had no descriptor heap, so
//! the free list has no counterpart to mirror.
//!
//! # The fence
//!
//! A descriptor a command list has already referenced must not be rewritten while the GPU may still
//! read it. That is the same rule `Gpu::upload_bytes` obeys when it retires an outgrown upload arena
//! rather than dropping it — a use-after-free there showed up as a device removal on WARP. So
//! [`DescriptorAllocator::release`] does not free a slot; it parks it with the fence value that must
//! complete first, and [`DescriptorAllocator::retire`] moves parked slots onto the free list once
//! the fence has passed. Nothing else may put a slot back into circulation.
//!
//! Everything here is pure: no COM, no `unsafe`, and no device. That is deliberate — the properties
//! that matter (reuse, boundedness, the fence gate, countable exhaustion) are then provable on a
//! machine with no GPU at all, which is where the tests at the bottom of this file run.

use std::collections::{HashMap, VecDeque};

/// Descriptors per texture. `t0` and `t1` are a contiguous pair per draw, because the two-stage
/// pixel shaders index a descriptor *table*; a single-texture draw sets both to the same view.
pub const DESCRIPTORS_PER_TEXTURE: u32 = 2;

/// The key of an uncached texture. A zero key goes
/// into the uncached table and is "uncached, freed by refcount". A palette produced by
/// modifying another has no DataID, so every dyed or recoloured item lands here.
pub const UNCACHED: u64 = 0;

/// `key = (palette DID as u64) << 32 | indexed texture DID`
/// (the combined-texture cache).
///
/// The key is **only** the two DataIDs — notably not `bClipMap` and not the texture scale. Adding
/// either would use more memory than the original and render identically; dropping the key thrashes.
#[must_use]
pub const fn combined_texture_key(palette_did: u32, texture_did: u32) -> u64 {
    // `u64::from` is not const-callable on this toolchain; the widening cast is the same value.
    ((palette_did as u64) << 32) | texture_did as u64
}

/// Which producer a [`TextureKey`] came from.
///
/// # Why the space is part of the key
///
/// Four different pieces of this client build a `u64` out of two `u32` halves and hand it to
/// [`TextureTable`]:
///
/// | producer | high half | low half |
/// |---|---|---|
/// | `world::combined_key` | palette DataID, or 0 | `RenderSurface` DataID |
/// | `world::resolve_surface`'s untextured arm | the packed ARGB colour word | 0 |
/// | `ui_draw::image_key` | an **operation hash**, or 0 | image DataID |
/// | `Gpu::prepare_ui_fonts` | 0 (glyphs) or 1 (outline) | font DataID |
///
/// Two of those halves are unrestricted 32-bit values — the colour word and the operation hash —
/// so **no bit of the 64 is free to tag with**, and the disjointness of the four could only ever
/// have been an argument about the shipped data. It was not even a true one. Two collisions are
/// reachable today and both are demonstrated in this file's tests:
///
/// * `image_key(id, Some(Multiply(c)))` produces the high half `c.rotate_left(3) | 1`, which is a
///   surjection onto the odd 32-bit words. Palette DataIDs are `0x04xxxxxx` and half of them are
///   odd, so a `Multiply` whose colour is one particular value gives a key **equal** to a world
///   `(palette, texture)` key whenever the UI image id equals the `RenderSurface` id.
/// * A UI image drawn **plain** keys on `(0, image DID)`; a world texture whose `RenderSurface`
///   carries no default palette keys on `(0, RenderSurface DID)`. Both ids are `0x06` surface ids
///   drawn from the same dat, so this one needs no arithmetic coincidence at all — only the same
///   surface appearing in both places.
///
/// # Why it is a defect here and not in retail
///
/// Retail shares one table, and legitimately: everything in it is an image texture built by
/// the client from a `RenderSurface` and a palette, so two owners that agree on
/// the key agree on the pixels. A derived surface -- a recoloured local
/// copy — has no maintainer and therefore **no DataID**, so it lands in the uncached table
/// with key 0 and is shared with nobody. The original has no operation hash because it never needs
/// one.
///
/// This build's UI decode path is a different function from its world decode path
/// (`TextureStore::texture_data` plus `ui_draw::derive`/`composite`, against
/// `texture_data_shifted` with `bClipMap` and a shift palette), so two owners agreeing on a key
/// here do **not** agree on the pixels. A hit would show as one surface wearing another's pixels,
/// and — worse and more silently — as a use-after-free: `BakeCache::release_group_texture` would
/// take the slot back while `Gpu::ui_textures` still named it.
///
/// So the space is carried in the key rather than argued about. The four constructors below are
/// **total and disjoint by construction**: two keys from different spaces are unequal whatever
/// their `u64` payloads are, because `PartialEq` compares the discriminant first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextureSpace {
    /// The combined-texture cache's own key: `(palette DID, RenderSurface DID)`.
    World,
    /// The untextured arm of the client's surface binding, keyed on the packed
    /// ARGB colour word. A separate cache space because a
    /// colour word is not a DataID and shares no arithmetic with one.
    SolidColor,
    /// A UI image and the `SurfaceOp` it is shown through.
    Ui,
    /// A font's baked glyph sheet or its outline sheet.
    Font,
}

/// A [`TextureTable`] key: a producer and the 64 bits it computed.
///
/// [`TextureSpace`] says why this is not a bare `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureKey {
    space: TextureSpace,
    key: u64,
}

impl TextureKey {
    /// A zero key is uncached, never shared, and freed by
    /// refcount.
    ///
    /// **Uncached-ness is a property of the payload, not of the space**, and
    /// [`Self::is_uncached`] is what [`TextureTable`] tests: a zero payload in *any* space never
    /// enters `by_key` and therefore cannot hit or collide with anything, whatever its
    /// discriminant says. This constant is simply the name to write when a caller has no key at
    /// all; `TextureKey::ui(0)` is a different `TextureKey` and behaves identically, which is
    /// correct and is why no space needs its own sentinel.
    pub const UNCACHED: Self = Self {
        space: TextureSpace::World,
        key: UNCACHED,
    };

    /// `(palette DID, RenderSurface DID)`, from `world::combined_key`.
    #[must_use]
    pub const fn world(key: u64) -> Self {
        Self {
            space: TextureSpace::World,
            key,
        }
    }

    /// The surface setup's solid-colour texel, keyed on the packed ARGB word. Zero — a fully
    /// transparent black texel — is [`Self::UNCACHED`], exactly as a zero pair is.
    #[must_use]
    pub const fn solid_color(argb: u32) -> Self {
        if argb == 0 {
            return Self::UNCACHED;
        }
        Self {
            space: TextureSpace::SolidColor,
            key: argb as u64,
        }
    }

    /// A UI image and its operation, from `ui_draw::image_key`.
    #[must_use]
    pub const fn ui(key: u64) -> Self {
        Self {
            space: TextureSpace::Ui,
            key,
        }
    }

    /// A font sheet. `pass` is 0 for the glyphs and 1 for the outline.
    #[must_use]
    pub const fn font(pass: u32, font_did: u32) -> Self {
        Self {
            space: TextureSpace::Font,
            key: combined_texture_key(pass, font_did),
        }
    }

    /// A font sheet by its already-combined payload, `combined_texture_key(pass, font DID)`: the
    /// form an overlay's glyph-sheet key carries. Equal to [`Self::font`] of the same pair.
    #[must_use]
    pub const fn font_sheet(key: u64) -> Self {
        Self {
            space: TextureSpace::Font,
            key,
        }
    }

    /// Which producer built it.
    #[must_use]
    pub const fn space(self) -> TextureSpace {
        self.space
    }

    /// The 64 bits, for a counter or a message. Never for a comparison across spaces.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.key
    }

    /// `key == 0`: the entry goes in the uncached table and is shared with nobody.
    #[must_use]
    pub const fn is_uncached(self) -> bool {
        self.key == UNCACHED
    }
}

/// Counters for [`DescriptorAllocator`]. Every tolerated failure has one, because a failure recorded
/// into a number nothing compares is a failure nobody sees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DescriptorStats {
    /// Slots handed out, including reuses.
    pub allocations: u64,
    /// Of those, the ones served from the free list rather than from fresh heap space. This is the
    /// number that separates "bounded" from "monotonic".
    pub reuses: u64,
    /// Slots parked for the fence.
    pub releases: u64,
    /// Slots the fence released back onto the free list.
    pub retired: u64,
    /// Allocation attempts that found the heap full. Non-zero means a caller got `None`.
    pub exhaustions: u64,
    /// Releases of a slot that was not live: a double release, a misaligned base, or an index past
    /// the heap. Tolerated (the caller gets `false`), counted, and asserted on in the tests.
    pub invalid_releases: u64,
}

/// What one slot of the heap is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    /// Never handed out.
    Unused,
    /// Handed out and not released.
    Live,
    /// Released, waiting for its fence value.
    Pending,
    /// On the free list.
    Free,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    /// The fence value that must complete before the slot may be handed out again.
    fence: u64,
    slot: u32,
}

/// A free-list allocator over the shader-visible CBV/SRV/UAV heap, in units of one texture's
/// descriptor pair.
///
/// Handles are **descriptor indices** (`slot * stride`), which is what `TextureSlot` has always
/// held and what `bind_texture` offsets the heap start by.
#[derive(Debug)]
pub struct DescriptorAllocator {
    /// Descriptors per slot.
    stride: u32,
    /// The budget, in slots.
    slots: u32,
    /// Slots ever taken from fresh heap space. This is the high-water mark of *heap consumption*:
    /// without reuse, this is the number that would grow without bound.
    frontier: u32,
    /// Slots available now. Popped from the back, so the order is deterministic (LIFO, the
    /// most-recently-retired first) rather than a hash order.
    free: Vec<u32>,
    /// Released slots waiting on the fence, in non-decreasing fence order.
    pending: VecDeque<Pending>,
    /// The largest fence value pushed so far, so the queue's ordering holds even if a caller
    /// releases with a stale value.
    last_pending_fence: u64,
    state: Vec<SlotState>,
    /// Slots currently handed out.
    live: u32,
    /// The largest `live` ever reached.
    high_water: u32,
    stats: DescriptorStats,
}

impl DescriptorAllocator {
    /// A budget of `descriptors` descriptors, handed out `stride` at a time.
    ///
    /// # Panics
    /// If `stride` is zero.
    #[must_use]
    pub fn new(descriptors: u32, stride: u32) -> Self {
        assert!(
            stride > 0,
            "a descriptor slot needs at least one descriptor"
        );
        let slots = descriptors / stride;
        Self {
            stride,
            slots,
            frontier: 0,
            free: Vec::new(),
            pending: VecDeque::new(),
            last_pending_fence: 0,
            state: vec![SlotState::Unused; slots as usize],
            live: 0,
            high_water: 0,
            stats: DescriptorStats::default(),
        }
    }

    /// Take a slot, preferring one the fence has already released.
    ///
    /// Returns the base descriptor index, or `None` when the heap is full — which bumps
    /// [`DescriptorStats::exhaustions`] and leaves every other field untouched.
    pub fn alloc(&mut self) -> Option<u32> {
        let slot = if let Some(reused) = self.free.pop() {
            self.stats.reuses += 1;
            reused
        } else if self.frontier < self.slots {
            let fresh = self.frontier;
            self.frontier += 1;
            fresh
        } else {
            self.stats.exhaustions += 1;
            return None;
        };
        self.state[slot as usize] = SlotState::Live;
        self.live += 1;
        self.high_water = self.high_water.max(self.live);
        self.stats.allocations += 1;
        Some(slot * self.stride)
    }

    /// Park a slot until `fence` completes.
    ///
    /// `fence` must be a value the queue has not yet signalled — in `Gpu` that is
    /// `next_fence_value`, the value the next `end_frame` or `wait_idle` will signal, so every
    /// command list that could already have referenced the descriptor precedes it.
    ///
    /// Returns `false` for a base that is not a live slot, bumping
    /// [`DescriptorStats::invalid_releases`]. A double release must not put the slot on the free
    /// list twice; that would hand the same descriptors to two owners.
    pub fn release(&mut self, base: u32, fence: u64) -> bool {
        if !base.is_multiple_of(self.stride) {
            self.stats.invalid_releases += 1;
            return false;
        }
        let slot = base / self.stride;
        if slot >= self.slots || self.state[slot as usize] != SlotState::Live {
            self.stats.invalid_releases += 1;
            return false;
        }
        self.state[slot as usize] = SlotState::Pending;
        self.live -= 1;
        self.stats.releases += 1;
        // Clamp forward so the queue stays sorted whatever the caller passes. Later is always safe;
        // earlier is the use-after-free.
        let fence = fence.max(self.last_pending_fence);
        self.last_pending_fence = fence;
        self.pending.push_back(Pending { fence, slot });
        true
    }

    /// Move every slot whose fence has completed onto the free list. Returns how many moved.
    pub fn retire(&mut self, completed_fence: u64) -> u32 {
        let mut moved = 0;
        while let Some(front) = self.pending.front().copied() {
            if front.fence > completed_fence {
                break;
            }
            self.pending.pop_front();
            self.state[front.slot as usize] = SlotState::Free;
            self.free.push(front.slot);
            moved += 1;
        }
        self.stats.retired += u64::from(moved);
        moved
    }

    /// Slots ever taken from fresh heap space — the ceiling this module exists to bound.
    #[must_use]
    pub fn frontier(&self) -> u32 {
        self.frontier
    }

    /// Slots currently handed out.
    #[must_use]
    pub fn live(&self) -> u32 {
        self.live
    }

    /// The largest number of slots ever live at once.
    #[must_use]
    pub fn high_water(&self) -> u32 {
        self.high_water
    }

    /// Slots on the free list, reusable now.
    #[must_use]
    pub fn free_slots(&self) -> u32 {
        // LINT-OK: the free list can never hold more entries than `slots`, which is a u32.
        #[allow(clippy::cast_possible_truncation)]
        {
            self.free.len() as u32
        }
    }

    /// Slots released but still behind the fence.
    #[must_use]
    pub fn pending_slots(&self) -> u32 {
        // LINT-OK: bounded by `slots`, which is a u32.
        #[allow(clippy::cast_possible_truncation)]
        {
            self.pending.len() as u32
        }
    }

    /// The budget, in slots.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.slots
    }

    /// The counters. Assert on these, not on the absence of a log line.
    #[must_use]
    pub fn stats(&self) -> DescriptorStats {
        self.stats
    }

    /// Narrow the budget so a test can reach exhaustion without filling the real heap. Test-only, and
    /// only valid before anything has been allocated.
    ///
    /// `#[doc(hidden)] pub` rather than `#[cfg(test)] pub(crate)` because this module lives outside
    /// `dereth-render`: the two backends' `narrow_descriptor_budget` helpers are in another
    /// crate, and a `cfg(test)` item is not compiled for it. The precondition assert below is what
    /// keeps it test-only in effect.
    #[doc(hidden)]
    pub fn narrow_for_test(&mut self, slots: u32) {
        assert_eq!(
            self.frontier, 0,
            "narrow the budget before allocating from it"
        );
        self.slots = slots;
        self.state = vec![SlotState::Unused; slots as usize];
    }
}

/// Counters for [`TextureTable`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextureTableStats {
    /// Keyed lookups that found a live texture and took a reference instead of uploading.
    pub hits: u64,
    /// Keyed lookups that did not.
    pub misses: u64,
    /// Textures registered.
    pub inserts: u64,
    /// Explicit `AddRef`s (a second owner of a slot the caller already has in hand).
    pub add_refs: u64,
    /// `Release` calls that found a live entry.
    pub releases: u64,
    /// Of those, the ones that took the count to zero and freed the texture.
    pub frees: u64,
    /// Releases naming a slot with no entry: a double free, or a handle from a previous device.
    /// Tolerated, counted, asserted on.
    pub unknown_releases: u64,
    /// `AddRef`s naming a slot with no entry. Same treatment.
    pub unknown_add_refs: u64,
    /// **A census, not a failure.** Inserts whose sixty-four-bit payload was
    /// already live under a **different** [`TextureSpace`].
    ///
    /// Every one of these is a pair of textures that a single shared `u64` map would have merged
    /// into one, silently: one surface wearing another's pixels, and a use-after-free the moment
    /// either owner released. With the space in the key they are two entries and nothing happens,
    /// so this number is what tells the difference between *"the collision was arithmetically
    /// possible"* and *"the collision is live in the shipped data"* — a distinction no argument
    /// about DataID ranges can settle.
    ///
    /// It is not an error and nothing asserts it at zero. The key-space tests report it
    /// and `descriptor.rs`'s own tests calibrate that it can be non-zero at all.
    pub cross_space_payload_collisions: u64,
}

/// What a release did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Released {
    /// The count reached zero: the caller must now free the slot and the resource.
    Freed,
    /// Another owner still holds it; the remaining count.
    StillLinked(u32),
    /// No such entry. Counted in [`TextureTableStats::unknown_releases`].
    Unknown,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Texture code plus the space it was computed in. Zero means the
    /// texture lives in the uncached table: uncached, freed by refcount, never shared.
    key: TextureKey,
    /// Number of linked database objects.
    links: u32,
}

/// The shared and custom texture tables and the link count that
/// decides when an entry leaves them.
#[derive(Debug, Default)]
pub struct TextureTable {
    // ORDER-OK: keyed by texture code and only ever looked up, never iterated for anything a
    // rendered result depends on. The original is a hash table keyed on the 64-bit code too.
    by_key: HashMap<TextureKey, u32>,
    // ORDER-OK: keyed by descriptor slot and only ever looked up.
    entries: HashMap<u32, Entry>,
    /// **The census.** Which spaces currently hold each sixty-four-bit payload, so that
    /// [`TextureTableStats::cross_space_payload_collisions`] is a count of what actually happened
    /// rather than of what could. Bounded by the number of live keyed entries, and emptied with
    /// them — a cumulative "payloads ever seen" map would be exactly the unbounded growth the
    /// neighbouring row is about.
    // ORDER-OK: keyed by payload and only ever looked up; the Vec is a set of at most four.
    by_payload: HashMap<u64, Vec<TextureSpace>>,
    stats: TextureTableStats,
}

impl TextureTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `key` names a live cached texture, without taking a link on it as [`Self::get`] does.
    #[must_use]
    pub fn contains(&self, key: TextureKey) -> bool {
        !key.is_uncached() && self.by_key.contains_key(&key)
    }

    /// `if (key != 0 and texture_table[key] exists) { AddRef; return it }`.
    ///
    /// [`UNCACHED`] never hits, which makes a dyed item's texture private to its owning surface.
    pub fn get(&mut self, key: TextureKey) -> Option<u32> {
        if key.is_uncached() {
            self.stats.misses += 1;
            return None;
        }
        match self.by_key.get(&key).copied() {
            Some(slot) => {
                if let Some(e) = self.entries.get_mut(&slot) {
                    e.links += 1;
                    self.stats.hits += 1;
                    return Some(slot);
                }
                // A key pointing at no entry would be the table disagreeing with itself; treat it
                // as a miss and drop the stale key rather than handing back a dead slot.
                self.by_key.remove(&key);
                self.stats.misses += 1;
                None
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Register a freshly uploaded texture at `slot` with one link.
    ///
    /// A non-zero `key` goes in the shared table; [`UNCACHED`] goes in the uncached table, which
    /// here is just the absence of a key.
    pub fn insert(&mut self, key: TextureKey, slot: u32) {
        if !key.is_uncached() {
            self.by_key.insert(key, slot);
            // The census. Counted before this space is recorded, so a second entry in the
            // *same* space (which cannot happen through `by_key`, but is what a caller who
            // inserted twice would produce) is not counted as a cross-space collision.
            let spaces = self.by_payload.entry(key.raw()).or_default();
            if spaces.iter().any(|s| *s != key.space()) {
                self.stats.cross_space_payload_collisions += 1;
            }
            spaces.push(key.space());
        }
        self.entries.insert(slot, Entry { key, links: 1 });
        self.stats.inserts += 1;
    }

    /// Take another link on a slot the caller already holds. Returns the new count.
    pub fn add_ref(&mut self, slot: u32) -> Option<u32> {
        match self.entries.get_mut(&slot) {
            Some(e) => {
                e.links += 1;
                self.stats.add_refs += 1;
                Some(e.links)
            }
            None => {
                self.stats.unknown_add_refs += 1;
                None
            }
        }
    }

    /// Release: decrement, and at zero remove from whichever table holds it --
    /// exactly what the client's destructor does.
    pub fn release(&mut self, slot: u32) -> Released {
        let Some(e) = self.entries.get_mut(&slot) else {
            self.stats.unknown_releases += 1;
            return Released::Unknown;
        };
        self.stats.releases += 1;
        e.links -= 1;
        if e.links > 0 {
            return Released::StillLinked(e.links);
        }
        let key = e.key;
        self.entries.remove(&slot);
        if !key.is_uncached() {
            self.by_key.remove(&key);
            // The census map goes with the entries it describes.
            if let Some(spaces) = self.by_payload.get_mut(&key.raw()) {
                if let Some(i) = spaces.iter().position(|s| *s == key.space()) {
                    spaces.swap_remove(i);
                }
                if spaces.is_empty() {
                    self.by_payload.remove(&key.raw());
                }
            }
        }
        self.stats.frees += 1;
        Released::Freed
    }

    /// The link count of `slot`, if it is live.
    #[must_use]
    pub fn links(&self, slot: u32) -> Option<u32> {
        self.entries.get(&slot).map(|e| e.links)
    }

    /// Live textures.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Live textures that are shareable, i.e. in the shared table rather than the uncached
    /// table.
    #[must_use]
    pub fn keyed_len(&self) -> usize {
        self.by_key.len()
    }

    /// Every live keyed entry, for a census.
    ///
    /// Sorted, so that a test comparing two devices' key sets reads the same on every run.
    // ORDER-OK: the result is sorted before it leaves.
    #[must_use]
    pub fn keys(&self) -> Vec<TextureKey> {
        let mut out: Vec<TextureKey> = self.by_key.keys().copied().collect();
        out.sort_by_key(|k| (k.space(), k.raw()));
        out
    }

    /// The counters.
    #[must_use]
    pub fn stats(&self) -> TextureTableStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocator(slots: u32) -> DescriptorAllocator {
        DescriptorAllocator::new(slots * DESCRIPTORS_PER_TEXTURE, DESCRIPTORS_PER_TEXTURE)
    }

    // Oracle: the stated requirement -- "Releasing and re-uploading must reuse slots, so a long walk
    // or a long session is bounded rather than monotonic." The counter that used to be monotonic is
    // `frontier`, so the assertion is on that and not merely on the returned handle.
    #[test]
    fn a_create_release_create_cycle_reuses_the_slot_instead_of_advancing_the_counter() {
        let mut a = allocator(16);
        let first = a.alloc().expect("a fresh heap has room");
        assert_eq!(first, 0);
        assert_eq!(a.frontier(), 1);
        assert!(a.release(first, 7));
        // Still not reusable: the fence has not passed.
        assert_eq!(a.free_slots(), 0);
        assert_eq!(a.retire(7), 1);
        let second = a.alloc().expect("the retired slot is available");
        assert_eq!(second, first, "the same descriptor pair, not the next one");
        assert_eq!(a.frontier(), 1, "the monotonic counter did not advance");
        assert_eq!(a.stats().reuses, 1);
        assert_eq!(a.stats().allocations, 2);
    }

    // Oracle: the stated requirement -- "N releases followed by N uploads leaves the high-water mark
    // at N, not 2N -- i.e. the ceiling is genuinely bounded."
    #[test]
    fn n_releases_then_n_uploads_leave_the_heap_high_water_at_n_not_2n() {
        const N: u32 = 64;
        let mut a = allocator(1024);
        let first: Vec<u32> = (0..N).map(|_| a.alloc().expect("room")).collect();
        assert_eq!(a.frontier(), N);
        for h in &first {
            assert!(a.release(*h, 1));
        }
        assert_eq!(a.retire(1), N);
        let second: Vec<u32> = (0..N).map(|_| a.alloc().expect("room")).collect();
        assert_eq!(
            a.frontier(),
            N,
            "2N would be the unbounded behaviour this replaces"
        );
        assert_eq!(a.high_water(), N, "N live at once, twice over, is still N");
        assert_eq!(a.stats().reuses, u64::from(N));
        // The same set of slots came back, in some order; no slot was handed out twice at once.
        let mut a_sorted = first.clone();
        let mut b_sorted = second.clone();
        a_sorted.sort_unstable();
        b_sorted.sort_unstable();
        assert_eq!(a_sorted, b_sorted);
    }

    // Oracle: the stated requirement -- "A slot must not be reused while the GPU may still be
    // reading it", enforced the way `upload_bytes` enforces it for outgrown upload arenas: the fence
    // value, not the release call, is what puts the resource back in circulation. Skipping this is a
    // use-after-free that showed up as a device removal on WARP.
    #[test]
    fn a_slot_released_this_frame_is_not_handed_out_until_the_fence_has_passed() {
        let mut a = allocator(2);
        let x = a.alloc().expect("room");
        let y = a.alloc().expect("room");
        // The frame that may still be reading x and y will signal fence value 9.
        assert!(a.release(x, 9));
        assert!(a.release(y, 9));
        assert_eq!(a.pending_slots(), 2);
        assert_eq!(a.free_slots(), 0);
        // The heap is nominally empty of live textures, and yet an allocation must fail rather than
        // reissue a descriptor the GPU may be reading.
        assert_eq!(a.live(), 0);
        assert_eq!(a.alloc(), None, "reissuing here is the use-after-free");
        assert_eq!(a.stats().exhaustions, 1, "and the refusal is counted");
        // The fence has reached 8: still not good enough.
        assert_eq!(a.retire(8), 0);
        assert_eq!(a.alloc(), None);
        assert_eq!(a.stats().exhaustions, 2);
        // 9 completes. Now, and only now.
        assert_eq!(a.retire(9), 2);
        assert!(a.alloc().is_some());
        assert!(a.alloc().is_some());
        assert_eq!(a.frontier(), 2);
    }

    // Oracle: the stated requirement -- "heap exhaustion still fails cleanly and countably rather
    // than corrupting." Cleanly means the allocator's own bookkeeping is untouched by the failure,
    // which is asserted by allocating successfully afterwards.
    #[test]
    fn heap_exhaustion_is_counted_and_leaves_the_allocator_usable() {
        let mut a = allocator(3);
        let held: Vec<u32> = (0..3).map(|_| a.alloc().expect("room")).collect();
        let before = a.stats();
        for _ in 0..5 {
            assert_eq!(a.alloc(), None);
        }
        let after = a.stats();
        assert_eq!(
            after.exhaustions,
            before.exhaustions + 5,
            "every refusal is counted"
        );
        assert_eq!(
            after.allocations, before.allocations,
            "a refusal is not an allocation"
        );
        assert_eq!(a.live(), 3);
        assert_eq!(a.frontier(), 3);
        // And the allocator still works once a slot comes back.
        assert!(a.release(held[1], 4));
        a.retire(4);
        assert_eq!(a.alloc(), Some(held[1]));
        assert_eq!(a.live(), 3);
    }

    // Oracle: the stated requirement -- "If you make something tolerant of failure, give it a
    // counter and assert on that counter." A double release is tolerated (the caller gets `false`)
    // because the alternative is a panic in a teardown path; it must not put the slot on the free
    // list twice, which would hand the same descriptors to two owners.
    #[test]
    fn a_double_release_is_counted_and_does_not_free_the_slot_twice() {
        let mut a = allocator(4);
        let x = a.alloc().expect("room");
        assert!(a.release(x, 1));
        assert!(
            !a.release(x, 1),
            "the second release names a slot that is no longer live"
        );
        assert_eq!(a.stats().invalid_releases, 1);
        assert_eq!(a.stats().releases, 1);
        a.retire(1);
        assert_eq!(a.free_slots(), 1, "one slot came back, not two");
        assert_eq!(a.pending_slots(), 0);
    }

    // Oracle: same rule. A handle that is not a slot base, or is past the heap, is a caller bug;
    // it is refused and counted rather than indexing something.
    #[test]
    fn a_misaligned_or_out_of_range_release_is_counted_and_refused() {
        let mut a = allocator(4);
        let x = a.alloc().expect("room");
        assert!(!a.release(x + 1, 1), "an odd base is not a pair base");
        assert!(
            !a.release(4 * DESCRIPTORS_PER_TEXTURE, 1),
            "past the budget"
        );
        assert!(
            !a.release(2 * DESCRIPTORS_PER_TEXTURE, 1),
            "in range but never allocated"
        );
        assert_eq!(a.stats().invalid_releases, 3);
        assert_eq!(a.live(), 1, "none of them disturbed the live count");
        assert!(a.release(x, 1), "the real handle still works");
    }

    // Oracle: the shape of the problem this module fixes -- landblock streaming adds terrain
    // surfaces as you walk and drops the blocks. The bound to prove is that the heap consumption of
    // a long walk is the size of the *window*, not the length of the walk.
    #[test]
    fn a_long_walk_streaming_blocks_in_and_out_stays_bounded_by_the_window() {
        // A 7x7 window of landblocks, 4 merged surfaces each, walked across 200 blocks.
        const WINDOW: u32 = 49 * 4;
        let mut a = allocator(2048);
        let mut resident: VecDeque<u32> = VecDeque::new();
        let mut fence = 1u64;
        for _ in 0..WINDOW {
            resident.push_back(a.alloc().expect("the window fits"));
        }
        for _ in 0..200 {
            // One block leaves the window and one enters, per frame.
            for _ in 0..4 {
                let gone = resident.pop_front().expect("resident");
                assert!(a.release(gone, fence));
            }
            fence += 1;
            a.retire(fence);
            for _ in 0..4 {
                resident.push_back(a.alloc().expect("a walk must not exhaust the heap"));
            }
        }
        assert_eq!(a.live(), WINDOW);
        assert_eq!(a.high_water(), WINDOW);
        // The monotonic version of this loop would have reached 196 + 800 = 996 slots. The bound is
        // the window plus at most one frame's worth still behind the fence.
        assert!(
            a.frontier() <= WINDOW + 4,
            "{} slots for a 200-block walk",
            a.frontier()
        );
        assert_eq!(a.stats().allocations, u64::from(WINDOW) + 800);
        assert!(a.stats().reuses >= 796);
        assert_eq!(a.stats().exhaustions, 0);
        assert_eq!(a.stats().invalid_releases, 0);
    }

    // The verified combined-texture cache uses `key = (palette->DID as uint64) << 32 |
    // indexedTex->DID", and "the cache key is (palette DataID, indexed-texture DataID) and nothing
    // else".
    #[test]
    fn the_combined_texture_key_is_the_palette_did_over_the_texture_did() {
        assert_eq!(
            combined_texture_key(0x0400_0123, 0x0600_4567),
            0x0400_0123_0600_4567
        );
        assert_eq!(combined_texture_key(0, 0), UNCACHED);
        // "both zero if the palette is not a dat object" -- so a *modified* palette gives key 0 only
        // when the texture DataID is zero too; a real indexed texture with a modified palette still
        // produces a distinct, non-zero key, which is why the client tracks the uncached case by the
        // whole key rather than by the palette alone.
        assert_ne!(combined_texture_key(0, 0x0600_4567), UNCACHED);
    }

    // ---------------------------------------------------------------------------------------
    // The four key spaces
    // ---------------------------------------------------------------------------------------

    /// **The collision, demonstrated rather than asserted to be possible.**
    ///
    /// `ui_draw::image_key(id, Some(Multiply(c)))` puts `c.rotate_left(3) | 1` in the palette half.
    /// That map is a surjection onto the **odd** 32-bit words, and half of every palette DataID
    /// range is odd, so for any odd palette DID there is a `Multiply` colour that reproduces it
    /// exactly. Here is one: the `Multiply` colour that yields palette DID `0x0400_0123`.
    ///
    /// This test carries `image_key`'s arithmetic rather than calling it, because `image_key` is
    /// in `dereth-client` and this file is the layer the two producers meet in. The two lines are
    /// checked against each other by `dereth-client`'s own
    /// `tests/gpu/rendering/texture_cache_key_spaces.rs::the_ui_and_world_key_spaces_can_produce_the_same_sixty_four_bits`,
    /// which drives the real `image_key` and gets the same number.
    #[test]
    fn the_two_producers_really_can_compute_the_same_sixty_four_bits() {
        let palette_did = 0x0400_0123u32;
        let texture_did = 0x0600_4567u32;
        // Invert `c.rotate_left(3) | 1 == palette_did` for an odd target: `c = palette_did >> 3`
        // rotated back.
        let c = palette_did.rotate_right(3);
        let op_half = c.rotate_left(3) | 1;
        assert_eq!(
            op_half, palette_did,
            "a Multiply colour reproduces the palette DID exactly"
        );
        let world_bits = combined_texture_key(palette_did, texture_did);
        let ui_bits = combined_texture_key(op_half, texture_did);
        assert_eq!(
            world_bits, ui_bits,
            "the raw sixty-four bits collide -- which is what one shared u64 map keyed on"
        );

        // And the second, arithmetic-free case: a plain UI image and a world texture whose
        // RenderSurface carries no default palette both key on `(0, that surface id)`.
        assert_eq!(
            combined_texture_key(0, texture_did),
            combined_texture_key(0, texture_did)
        );

        // With the space in the key, neither collides.
        assert_ne!(TextureKey::world(world_bits), TextureKey::ui(ui_bits));
        assert_ne!(
            TextureKey::world(combined_texture_key(0, texture_did)),
            TextureKey::ui(combined_texture_key(0, texture_did))
        );
    }

    /// **The disjointness itself, over the whole payload space rather than over examples.**
    ///
    /// Two keys from different spaces are unequal *whatever* their `u64` payloads are, because
    /// `PartialEq` on the struct compares the discriminant before the payload. That is the whole
    /// argument, and it is checked here on the hardest input available: the **same** payload in
    /// every space, including payloads that are reachable in more than one of them.
    ///
    /// The uncached sentinel is excluded deliberately and separately asserted: it is one value
    /// across every space by construction, and it can never hit, so it cannot collide.
    #[test]
    fn no_two_spaces_can_produce_equal_keys() {
        let payloads: [u64; 6] = [
            1,
            0x0400_0123_0600_4567,
            combined_texture_key(0, 0x0600_4567),
            combined_texture_key(1, 0x4000_0001),
            u64::from(u32::MAX),
            u64::MAX,
        ];
        for p in payloads {
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the low half is exactly what the solid-colour producer keys on.
            let low = p as u32;
            let spaces = [
                TextureKey::world(p),
                TextureKey::ui(p),
                TextureKey::font(0, low),
                TextureKey::font(1, low),
                TextureKey::solid_color(low),
            ];
            for (i, a) in spaces.iter().enumerate() {
                for (j, b) in spaces.iter().enumerate() {
                    if i == j || a.is_uncached() || b.is_uncached() {
                        continue;
                    }
                    assert_ne!(
                        a,
                        b,
                        "payload {p:#018x}: {:?} and {:?} are equal",
                        a.space(),
                        b.space()
                    );
                }
            }
        }
        assert!(
            TextureKey::solid_color(0).is_uncached(),
            "a zero colour word is uncached"
        );
        assert!(TextureKey::world(UNCACHED).is_uncached());
        // Uncached-ness is a property of the payload, not of the discriminant: a zero payload in
        // any space is uncached and therefore cannot collide, even though it is not `==` to
        // `UNCACHED`. `TextureTable` tests `is_uncached()` and never the constant, which is what
        // makes that true where it matters.
        assert!(TextureKey::ui(UNCACHED).is_uncached());
        assert!(TextureKey::font(0, 0).is_uncached());
        assert_ne!(TextureKey::ui(UNCACHED), TextureKey::UNCACHED);
        let mut t = TextureTable::new();
        t.insert(TextureKey::ui(UNCACHED), 0);
        assert_eq!(
            t.keyed_len(),
            0,
            "an uncached key must not enter `texture_table`"
        );
        assert_eq!(t.get(TextureKey::ui(UNCACHED)), None, "and must never hit");
    }

    /// The table itself, not just the key type: two spaces holding the same payload get two
    /// entries and neither serves the other's lookup.
    ///
    /// The premise is the first assertion: within **one** space that payload does hit, so a green
    /// run cannot mean the table simply stopped caching.
    #[test]
    fn one_payload_in_two_spaces_takes_two_slots_and_neither_hits_the_other() {
        let bits = combined_texture_key(0x0400_0123, 0x0600_4567);
        let mut t = TextureTable::new();
        t.insert(TextureKey::world(bits), 0);
        assert_eq!(
            t.get(TextureKey::world(bits)),
            Some(0),
            "the space caches at all"
        );
        assert_eq!(
            t.get(TextureKey::ui(bits)),
            None,
            "the UI space must not hit the world's"
        );
        t.insert(TextureKey::ui(bits), 2);
        assert_eq!(t.get(TextureKey::ui(bits)), Some(2));
        assert_eq!(t.keyed_len(), 2, "two spaces, two entries");
        // Releasing one to zero must not take the other's key out of the table with it -- the
        // half of the defect that would have shown as a use-after-free rather than as wrong
        // pixels.
        assert_eq!(t.release(0), Released::StillLinked(1));
        assert_eq!(t.release(0), Released::Freed);
        assert_eq!(
            t.get(TextureKey::ui(bits)),
            Some(2),
            "the UI entry survived"
        );
        assert_eq!(t.keyed_len(), 1);
    }

    /// The census's calibration.
    ///
    /// `cross_space_payload_collisions` is reported over real scenes and asserted at nothing, so
    /// it has to be shown here that it can be non-zero — and that it counts what it says it
    /// counts rather than every insert. Per §7.8: a zero from an instrument that has never
    /// produced a non-zero is not a measurement.
    #[test]
    fn the_cross_space_census_counts_a_shared_payload_and_only_a_shared_payload() {
        let bits = combined_texture_key(0x0400_0123, 0x0600_4567);
        let other = combined_texture_key(0x0400_0124, 0x0600_4567);
        let mut t = TextureTable::new();
        t.insert(TextureKey::world(bits), 0);
        assert_eq!(
            t.stats().cross_space_payload_collisions,
            0,
            "one space, no collision"
        );
        t.insert(TextureKey::world(other), 2);
        assert_eq!(
            t.stats().cross_space_payload_collisions,
            0,
            "different payloads"
        );
        t.insert(TextureKey::ui(bits), 4);
        assert_eq!(
            t.stats().cross_space_payload_collisions,
            1,
            "the same payload, two spaces"
        );
        t.insert(TextureKey::font(0, 0x4000_0001), 6);
        assert_eq!(t.stats().cross_space_payload_collisions, 1);
        // The census map is emptied with the entries it describes, so a released pair does not
        // keep counting -- and re-inserting after both are gone starts clean.
        assert_eq!(t.release(0), Released::Freed);
        assert_eq!(t.release(4), Released::Freed);
        t.insert(TextureKey::world(bits), 8);
        assert_eq!(
            t.stats().cross_space_payload_collisions,
            1,
            "the released UI entry must not still be counted against the payload"
        );
    }

    // Oracle: the same passage -- "if (key != 0 and texture_table[key] exists) { AddRef; return it }"
    // and the release path, which destroys at zero links.
    #[test]
    fn a_second_get_of_a_keyed_texture_addrefs_rather_than_uploading_again() {
        let mut t = TextureTable::new();
        let key = TextureKey::world(combined_texture_key(0x0400_0001, 0x0600_0002));
        assert_eq!(t.get(key), None, "nothing is cached yet");
        t.insert(key, 0);
        assert_eq!(t.links(0), Some(1));
        assert_eq!(t.get(key), Some(0), "the second owner shares the texture");
        assert_eq!(t.links(0), Some(2));
        assert_eq!(t.stats().hits, 1);
        assert_eq!(t.stats().misses, 1);
        // One owner going away does not free it.
        assert_eq!(t.release(0), Released::StillLinked(1));
        assert_eq!(t.len(), 1);
        // The second does, and the key leaves the table with it.
        assert_eq!(t.release(0), Released::Freed);
        assert!(t.is_empty());
        assert_eq!(t.keyed_len(), 0);
        assert_eq!(t.get(key), None);
        assert_eq!(t.stats().frees, 1);
    }

    // Oracle: the same passage -- "if (key == 0) custom_texture_table.add(t) // uncached, freed by
    // refcount", and "A palette produced by the palette-modification path has no DataID, so every
    // dyed/recoloured item gets an uncached texture that lives only as long as its surface."
    #[test]
    fn an_uncached_texture_is_never_shared_between_owners() {
        let mut t = TextureTable::new();
        t.insert(TextureKey::UNCACHED, 0);
        t.insert(TextureKey::UNCACHED, 2);
        assert_eq!(t.get(TextureKey::UNCACHED), None, "key 0 must never hit");
        assert_eq!(t.len(), 2, "two dyed items, two textures");
        assert_eq!(t.keyed_len(), 0, "and neither is in texture_table");
        assert_eq!(t.release(0), Released::Freed);
        assert_eq!(t.release(2), Released::Freed);
        assert!(t.is_empty());
    }

    // Oracle: "If you make something tolerant of failure, give it a counter and assert on that
    // counter." Releasing a handle twice, or one from a device that has gone away, must not panic
    // in an owner's teardown -- but it must be visible.
    #[test]
    fn an_unknown_release_or_addref_is_counted_rather_than_silently_ignored() {
        let mut t = TextureTable::new();
        t.insert(TextureKey::UNCACHED, 0);
        assert_eq!(t.release(0), Released::Freed);
        assert_eq!(
            t.release(0),
            Released::Unknown,
            "the second release has nothing to free"
        );
        assert_eq!(t.release(998), Released::Unknown);
        assert_eq!(t.stats().unknown_releases, 2);
        assert_eq!(t.add_ref(998), None);
        assert_eq!(t.stats().unknown_add_refs, 1);
        assert_eq!(t.stats().frees, 1, "and nothing was freed twice");
    }

    // Oracle: the two halves have to agree, which is the property the device wiring depends on -- a
    // slot is returned to the allocator exactly when the table's link count reaches zero.
    #[test]
    fn the_table_and_the_allocator_agree_over_a_session_of_shared_textures() {
        let mut a = allocator(8);
        let mut t = TextureTable::new();
        let mut fence = 1u64;
        // Five distinct keyed textures, acquired and dropped by overlapping generations of owners.
        let keys: Vec<TextureKey> = (1..=5)
            .map(|i| TextureKey::world(combined_texture_key(0x0400_0000 + i, i)))
            .collect();
        let mut held: Vec<u32> = Vec::new();
        for round in 0..10u32 {
            for k in &keys {
                let slot = match t.get(*k) {
                    Some(s) => s,
                    None => {
                        let s = a.alloc().expect("five textures fit in eight slots");
                        t.insert(*k, s);
                        s
                    }
                };
                held.push(slot);
            }
            // Drop the previous generation, so two generations overlap.
            if round >= 1 {
                for slot in held.drain(..keys.len()) {
                    if t.release(slot) == Released::Freed {
                        assert!(a.release(slot, fence));
                    }
                }
            }
            fence += 1;
            a.retire(fence);
        }
        for slot in held.drain(..) {
            if t.release(slot) == Released::Freed {
                assert!(a.release(slot, fence));
            }
        }
        fence += 1;
        a.retire(fence);
        assert!(t.is_empty(), "every owner released");
        assert_eq!(a.live(), 0, "and every slot came back");
        assert_eq!(a.free_slots(), a.frontier());
        assert!(
            a.frontier() <= 5,
            "{} slots for five distinct textures",
            a.frontier()
        );
        assert_eq!(a.stats().exhaustions, 0);
        assert_eq!(a.stats().invalid_releases, 0);
        assert_eq!(t.stats().unknown_releases, 0);
    }
}
