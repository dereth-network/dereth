//! Ranges handed out of one pool: the vertex, index, instance and draw item pools each give a
//! user a run of consecutive elements and take it back when the user goes.

use std::collections::BTreeMap;
use std::ops::Range;

/// A pool of `capacity` elements, handed out in runs, first fit.
#[derive(Debug, Clone, Default)]
pub struct Pool {
    capacity: u32,
    /// The free runs, by their start, never touching one another.
    free: BTreeMap<u32, u32>,
    used: u32,
}

impl Pool {
    /// A pool of `capacity` elements, all free.
    #[must_use]
    pub fn new(capacity: u32) -> Self {
        let mut free = BTreeMap::new();
        if capacity > 0 {
            free.insert(0, capacity);
        }
        Self {
            capacity,
            free,
            used: 0,
        }
    }

    /// The elements it holds.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// The elements handed out.
    #[must_use]
    pub fn used(&self) -> u32 {
        self.used
    }

    /// A run of `len` elements, from the first free run long enough; `None` when none is.
    pub fn take(&mut self, len: u32) -> Option<Range<u32>> {
        if len == 0 {
            return Some(0..0);
        }
        let (&start, &run) = self.free.iter().find(|(_, run)| **run >= len)?;
        self.free.remove(&start);
        if run > len {
            self.free.insert(start + len, run - len);
        }
        self.used += len;
        Some(start..start + len)
    }

    /// Give back `r`, which [`Self::take`] handed out, joining it to the free runs beside it.
    pub fn give(&mut self, r: Range<u32>) {
        if r.is_empty() {
            return;
        }
        let (mut start, mut end) = (r.start, r.end);
        self.used -= end - start;
        if let Some((&before, &len)) = self.free.range(..start).next_back() {
            if before + len == start {
                self.free.remove(&before);
                start = before;
            }
        }
        if let Some(len) = self.free.remove(&end) {
            end += len;
        }
        self.free.insert(start, end - start);
    }

    /// The pool grown to `capacity` elements, the new ones free.
    pub fn grow(&mut self, capacity: u32) {
        if capacity <= self.capacity {
            return;
        }
        let extra = self.capacity..capacity;
        self.used += extra.end - extra.start;
        self.capacity = capacity;
        self.give(extra);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Runs come from the first free run that fits, and a run given back joins its neighbours, so
    /// the whole pool is one free run again once everything is given back.
    #[test]
    fn runs_are_taken_first_fit_and_given_back_whole() {
        let mut p = Pool::new(100);
        let a = p.take(10).expect("fits");
        let b = p.take(20).expect("fits");
        let c = p.take(30).expect("fits");
        assert_eq!((a.clone(), b.clone(), c.clone()), (0..10, 10..30, 30..60));
        assert_eq!(p.used(), 60);
        p.give(b);
        // A run of 25 does not fit the hole of 20, so it comes after `c`.
        assert_eq!(p.take(25), Some(60..85));
        // One of 15 fits the hole.
        assert_eq!(p.take(15), Some(10..25));
        assert_eq!(p.take(50), None, "nothing that long is free");
        p.give(a);
        p.give(c);
        p.give(10..25);
        p.give(60..85);
        assert_eq!(p.used(), 0);
        assert_eq!(p.take(100), Some(0..100), "one free run again");
    }

    /// A grown pool hands out its new elements, joined to a free run at its old end.
    #[test]
    fn a_grown_pool_joins_its_new_elements_to_its_free_end() {
        let mut p = Pool::new(10);
        let a = p.take(6).expect("fits");
        assert_eq!(p.take(8), None);
        p.grow(20);
        assert_eq!(p.capacity(), 20);
        assert_eq!(p.take(8), Some(6..14));
        p.give(a);
        assert_eq!(p.used(), 8);
    }
}
