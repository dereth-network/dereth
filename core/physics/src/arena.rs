//! The physics-object arena and its handles.
//!
//! Track spec section 7.4 is the reason this exists. `check_walkable` and `transitional_insert`
//! recurse into each other, an object's collision handler can start another transition that moves
//! a *different* object, and the transition pool is global shared state ten levels deep.
//! Consequently **no `&mut PhysicsObj` may be held across anything that can re-enter**: every
//! object is addressed by [`PhysHandle`] and re-borrowed at each use.
//!
//! A generational index rather than a bare index, because objects are created and destroyed
//! *inside* `update_object` (collision reports call into the weenie layer) and a stale index would
//! silently name a different object. Hand-rolled rather than pulled from `slotmap`, because it is
//! sixty lines and the crate's dependency set is deliberately small.

/// A generational index into the world's object arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PhysHandle {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

impl PhysHandle {
    /// The raw index, for diagnostics only. Two handles with the same index and different
    /// generations name different objects.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.index
    }
}

#[derive(Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// A generational arena.
#[derive(Debug)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, value: T) -> PhysHandle {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            self.len += 1;
            return PhysHandle {
                index,
                generation: slot.generation,
            };
        }
        let index = u32::try_from(self.slots.len()).expect("arena index fits in u32");
        self.slots.push(Slot {
            generation: 0,
            value: Some(value),
        });
        self.len += 1;
        PhysHandle {
            index,
            generation: 0,
        }
    }

    pub fn remove(&mut self, h: PhysHandle) -> Option<T> {
        let slot = self.slots.get_mut(h.index as usize)?;
        if slot.generation != h.generation {
            return None;
        }
        let v = slot.value.take()?;
        // Bumping on removal is what makes a stale handle detectable rather than aliasing.
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(h.index);
        self.len -= 1;
        Some(v)
    }

    #[must_use]
    pub fn get(&self, h: PhysHandle) -> Option<&T> {
        let slot = self.slots.get(h.index as usize)?;
        if slot.generation != h.generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, h: PhysHandle) -> Option<&mut T> {
        let slot = self.slots.get_mut(h.index as usize)?;
        if slot.generation != h.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Two distinct handles at once. Returns `None` if they are the same slot or either is stale.
    pub fn get_two_mut(&mut self, a: PhysHandle, b: PhysHandle) -> Option<(&mut T, &mut T)> {
        if a.index == b.index {
            return None;
        }
        let (lo, hi) = if a.index < b.index { (a, b) } else { (b, a) };
        let (head, tail) = self.slots.split_at_mut(hi.index as usize);
        let l = head.get_mut(lo.index as usize)?;
        let r = tail.first_mut()?;
        if l.generation != lo.generation || r.generation != hi.generation {
            return None;
        }
        let (l, r) = (l.value.as_mut()?, r.value.as_mut()?);
        if a.index < b.index {
            Some((l, r))
        } else {
            Some((r, l))
        }
    }

    #[must_use]
    pub fn contains(&self, h: PhysHandle) -> bool {
        self.get(h).is_some()
    }

    /// Every live handle, in slot order. Not an iteration order anything observable depends on —
    /// the physics sweep uses [`crate::longhash::LongHash`] for that.
    pub fn handles(&self) -> impl Iterator<Item = PhysHandle> + '_ {
        self.slots.iter().enumerate().filter_map(|(i, s)| {
            s.value.as_ref().map(|_| PhysHandle {
                index: u32::try_from(i).expect("arena index fits"),
                generation: s.generation,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The requirement this type exists to satisfy: a handle to a removed object never names the
    // object that later takes its slot.

    #[test]
    fn a_removed_handle_does_not_name_the_slot_that_replaces_it() {
        let mut a: Arena<u32> = Arena::new();
        let h1 = a.insert(1);
        assert_eq!(a.remove(h1), Some(1));
        let h2 = a.insert(2);
        assert_eq!(h2.index, h1.index, "the slot is reused");
        assert_ne!(h2.generation, h1.generation, "but the generation moved on");
        assert_eq!(a.get(h1), None, "the stale handle must not resolve");
        assert_eq!(a.get(h2), Some(&2));
    }

    #[test]
    fn removal_during_iteration_is_safe_because_iteration_yields_handles() {
        let mut a: Arena<u32> = Arena::new();
        let hs: Vec<PhysHandle> = (0..10).map(|i| a.insert(i)).collect();
        let snapshot: Vec<PhysHandle> = a.handles().collect();
        assert_eq!(snapshot.len(), 10);
        for h in snapshot {
            if a.get(h).copied().is_some_and(|v| v % 2 == 0) {
                a.remove(h);
            }
        }
        assert_eq!(a.len(), 5);
        assert!(hs.iter().filter(|h| a.contains(**h)).count() == 5);
    }

    #[test]
    fn get_two_mut_refuses_the_same_slot_and_works_either_way_round() {
        let mut a: Arena<u32> = Arena::new();
        let h1 = a.insert(1);
        let h2 = a.insert(2);
        assert!(a.get_two_mut(h1, h1).is_none());
        {
            let (x, y) = a.get_two_mut(h1, h2).expect("two live handles");
            assert_eq!((*x, *y), (1, 2));
            *x = 10;
        }
        {
            let (y, x) = a.get_two_mut(h2, h1).expect("order does not matter");
            assert_eq!((*x, *y), (10, 2));
        }
        let stale = h1;
        a.remove(h1);
        assert!(a.get_two_mut(stale, h2).is_none());
    }

    #[test]
    fn double_removal_is_a_no_op() {
        let mut a: Arena<u32> = Arena::new();
        let h = a.insert(5);
        assert_eq!(a.remove(h), Some(5));
        assert_eq!(a.remove(h), None);
        assert!(a.is_empty());
    }
}
