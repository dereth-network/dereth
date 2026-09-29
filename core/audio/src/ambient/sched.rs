//! The ambient scheduler's min-heap, keyed by absolute time.
//!
//! A textbook **1-based binary min-heap** with the observed tie-breaking preserved:
//! equal-time insertion and removal order affects packet retransmits and the ambient schedule.
//! Equal deadlines are common: periodic ambient sounds sharing a `min_rate`
//! are scheduled for the same instant, so their relative order must be preserved.
//!
//! ```text
//! insert                   i = ++n
//!                          while (i > 1 && key < A[i/2].key) { A[i] = A[i/2]; i /= 2; }
//!                          A[i] = { key, data }
//!                          -- a strict `<`, so an equal key
//!                             stops immediately and stays *below* the earlier one.
//! heapify                  l = 2i, r = 2i+1, smallest = i
//!                          if (l <= n && A[l].key < A[i].key)        smallest = l
//!                          if (r <= n && A[r].key < A[smallest].key) smallest = r
//!                          if (smallest != i) { swap; heapify(smallest) }
//!                          -- both tests strict, so a tie keeps the parent.
//! remove-min               returns A[1]; A[1] = A[n]; --n; heapify(1)
//! ```
//!
//! The heap preserves the original node layout and ordering rules.

/// One heap node: `{ double key; void* data; }` plus padding, 16 bytes in the original.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Node<T: Copy> {
    key: f64,
    data: T,
}

/// The min-heap. `T` stands in for the `AmbientSound*` payload.
#[derive(Debug, Clone, Default)]
pub struct PQueueArray<T: Copy> {
    /// 1-based: index 0 is unused, exactly as the original's `A - 1` addressing arranges.
    nodes: Vec<Option<Node<T>>>,
}

impl<T: Copy> PQueueArray<T> {
    #[must_use]
    pub fn new() -> Self {
        Self { nodes: vec![None] }
    }

    /// The number of scheduled nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The smallest key without removing it — `A[0]->key`, which the frame update peeks at.
    #[must_use]
    pub fn peek_key(&self) -> Option<f64> {
        self.nodes.get(1).and_then(|n| n.as_ref()).map(|n| n.key)
    }

    /// Insert, sifting up while the new key is strictly smaller than its parent's.
    pub fn insert(&mut self, key: f64, data: T) {
        self.nodes.push(None);
        let mut i = self.len();
        while i > 1 {
            let parent = i / 2;
            let pk = self.key_at(parent);
            // Stop unless key < parent.key. Written as a
            // negated `<` rather than `>=` because retail treats *unordered* as "stop" too,
            // and `>=` on a NaN would keep sifting.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(key < pk) {
                break;
            }
            self.nodes[i] = self.nodes[parent];
            i = parent;
        }
        self.nodes[i] = Some(Node { key, data });
    }

    /// Remove the minimum: return the root, move the last node to it and sift down.
    pub fn remove_min(&mut self) -> Option<(f64, T)> {
        if self.is_empty() {
            return None;
        }
        let root = self.nodes[1].take().expect("non-empty heap has a root");
        let last = self.nodes.pop().flatten();
        if !self.is_empty() {
            self.nodes[1] = last;
            self.heapify(1);
        }
        Some((root.key, root.data))
    }

    fn key_at(&self, i: usize) -> f64 {
        self.nodes[i].as_ref().map_or(f64::INFINITY, |n| n.key)
    }

    /// Sift down, iterative rather than recursive -- same result.
    fn heapify(&mut self, start: usize) {
        let n = self.len();
        let mut i = start;
        loop {
            let l = 2 * i;
            let r = 2 * i + 1;
            let mut smallest = i;
            if l <= n && self.key_at(l) < self.key_at(i) {
                smallest = l;
            }
            if r <= n && self.key_at(r) < self.key_at(smallest) {
                smallest = r;
            }
            if smallest == i {
                return;
            }
            self.nodes.swap(i, smallest);
            i = smallest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered heap insert and sift-down rules transcribed in this module's docs.
    #[test]
    fn it_pops_in_ascending_key_order() {
        let mut q = PQueueArray::new();
        for (k, v) in [(5.0, 'e'), (1.0, 'a'), (4.0, 'd'), (2.0, 'b'), (3.0, 'c')] {
            q.insert(k, v);
        }
        assert_eq!(q.len(), 5);
        let mut got = Vec::new();
        while let Some((k, v)) = q.remove_min() {
            got.push((k, v));
        }
        assert_eq!(
            got,
            vec![(1.0, 'a'), (2.0, 'b'), (3.0, 'c'), (4.0, 'd'), (5.0, 'e')]
        );
    }

    /// The tie-break is not stable and is not arbitrary either: it is whatever the strict-`<` sift
    /// produces, and it is what the original produces. Contract 7.5 names it as observable, so this
    /// test pins the exact sequence rather than merely asserting the keys are non-decreasing.
    #[test]
    fn equal_keys_come_out_in_the_heaps_own_order() {
        let mut q = PQueueArray::new();
        for v in 0..8u32 {
            q.insert(1.0, v);
        }
        let mut got = Vec::new();
        while let Some((_, v)) = q.remove_min() {
            got.push(v);
        }
        // Derived by hand-simulating retail's insert and remove-min: with strict `<`,
        // insert never sifts an equal key up, so the array holds 0..7 in order; each remove-min then
        // returns the root and moves the *tail* into it, and heapify moves nothing because every
        // key is equal. So the first element comes out first and the rest come out backwards.
        assert_eq!(got, vec![0, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn peek_and_empty_behave() {
        let mut q: PQueueArray<u32> = PQueueArray::new();
        assert!(q.is_empty());
        assert_eq!(q.peek_key(), None);
        assert_eq!(q.remove_min(), None);
        q.insert(7.5, 1);
        assert_eq!(q.peek_key(), Some(7.5));
        q.insert(2.5, 2);
        assert_eq!(q.peek_key(), Some(2.5));
        assert_eq!(q.remove_min(), Some((2.5, 2)));
        assert_eq!(q.peek_key(), Some(7.5));
    }

    /// A thousand random inserts and removals still come out sorted.
    #[test]
    fn a_large_mixed_workload_stays_a_valid_heap() {
        let mut q = PQueueArray::new();
        let mut rng = dereth_primitives::num::rng::Ran2::new(31337);
        let mut expect: Vec<f64> = Vec::new();
        for i in 0..1000u32 {
            let k = rng.next_f64() * 100.0;
            q.insert(k, i);
            expect.push(k);
        }
        expect.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
        let mut got = Vec::new();
        while let Some((k, _)) = q.remove_min() {
            got.push(k);
        }
        assert_eq!(got, expect);
    }
}
