//! The alpha lists.
//!
//! Adding a mesh to the alpha list, and flushing that list.
//!
//! Meshes are appended and flushed in the frame-composition order described below.
//!
//! **The alpha list is not sorted.** It is a `Vec` flushed in insertion order, with a
//! documented cap of 3 000 entries per list. Making it a sorted structure is the single most
//! tempting "improvement" in this crate and it changes visible output: translucent surfaces resolve
//! by insertion order, and insertion order is the cell and part draw order, which is already far to
//! near. A depth sort produces a *different* picture, not a better one.

use dereth_primitives::{DrawBatch, Frame, MeshHandle, RenderBackend, TextureHandle};

use crate::consts::ALPHA_LIST_CAP;

/// One deferred subset, as the client's alpha-list add records it: the mesh buffer, subset number,
/// surface, material, matrix-valid flag, multipass flag and world matrix, 0x54 bytes in the client.
#[derive(Debug, Clone, PartialEq)]
pub struct AlphaEntry {
    pub mesh: MeshHandle,
    /// Which subset of the mesh.
    pub surface_num: u32,
    pub texture: Option<TextureHandle>,
    /// The matrix-valid flag — set only for the **first** deferred subset of each list per mesh, which is
    /// also when the object-to-world matrix and the material are recorded. Later subsets of the
    /// same mesh inherit whatever matrix is already installed.
    pub first_of_kind: bool,
    pub world_matrix: Frame,
    pub multipass: bool,
    pub range: std::ops::Range<u32>,
}

/// Which of the two lists an entry goes on. The client's alpha-list add appends to the clip list
/// (clip-mapped) or the alpha list (blended), and the alpha-list flush drains the **clip list
/// first**, then the alpha list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaList {
    Clip,
    Blend,
}

/// The pair of deferred lists. Each is a `Vec`, capped at 3 000 entries; a full list **drops** the
/// subset ([`AlphaLists::push`] returns false) rather than growing.
#[derive(Debug, Default)]
pub struct AlphaLists {
    clip: Vec<AlphaEntry>,
    blend: Vec<AlphaEntry>,
    /// How many subsets were dropped because a list was full. Not a client field; it exists so a
    /// test can tell "dropped" from "never submitted".
    pub dropped: usize,
}

impl AlphaLists {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns false when the list was full and the
    /// subset was dropped.
    pub fn push(&mut self, list: AlphaList, e: AlphaEntry) -> bool {
        let v = match list {
            AlphaList::Clip => &mut self.clip,
            AlphaList::Blend => &mut self.blend,
        };
        if v.len() >= ALPHA_LIST_CAP {
            self.dropped += 1;
            return false;
        }
        v.push(e);
        true
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.clip.len() + self.blend.len()
    }

    /// The clip-map list's count on its own: a test about *which* list a subset landed on
    /// needs it, and `len()` cannot answer that.
    #[must_use]
    pub fn clip_len(&self) -> usize {
        self.clip.len()
    }

    /// The alpha list's count on its own.
    #[must_use]
    pub fn blend_len(&self) -> usize {
        self.blend.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clip.is_empty() && self.blend.is_empty()
    }

    /// The entries in the exact order [`Self::flush`] would draw them: the clip list first, then
    /// the blend list, each in **insertion order**.
    #[must_use]
    pub fn draw_order(&self) -> Vec<&AlphaEntry> {
        self.clip.iter().chain(self.blend.iter()).collect()
    }

    /// The alpha-list flush's **threshold test alone**: it returns immediately unless
    /// `count >= t * 3000` for **at least one** list, so `ready(0.0)` is always true.
    ///
    /// Split out of [`Self::flush`] so the test can run without the
    /// [`RenderBackend`] seam — see [`Self::take_draw_order`].
    #[must_use]
    pub fn ready(&self, min_z: f32) -> bool {
        #[allow(clippy::cast_precision_loss)] // the cap is 3000
        let threshold = min_z * ALPHA_LIST_CAP as f32;
        #[allow(clippy::cast_precision_loss)]
        let r = self.clip.len() as f32 >= threshold || self.blend.len() as f32 >= threshold;
        r
    }

    /// Drain both lists in [`Self::draw_order`] — clip list first, insertion order within each —
    /// leaving the pair empty, exactly as the client's alpha-list flush resets each count as it goes.
    ///
    /// Split out of [`Self::flush`]. `flush` submits through
    /// [`RenderBackend`], this crate's seam; the client's own object pass draws through a
    /// `PipelineKey` and a dynamic vertex buffer, which that seam cannot express, so
    /// `dereth_client::world`'s alpha pass takes the entries and submits them itself. Splitting is
    /// what keeps **one** statement of the threshold and **one** of the order, rather than a
    /// second copy on the caller's side.
    ///
    /// [`Self::dropped`] is **not** reset: it is a per-frame diagnostic, not a client field.
    pub fn take_draw_order(&mut self) -> Vec<AlphaEntry> {
        let mut out = std::mem::take(&mut self.clip);
        out.append(&mut self.blend);
        out
    }

    /// Flush the alpha list at `t`.
    ///
    /// Returns immediately unless `count >= t * 3000` for **at least one** list, so
    /// `flush(0.0)` always flushes. It draws the clip list in insertion order, then the alpha list,
    /// resetting each count. **There is no depth sort**: the visible order is exactly the order in
    /// which cells and parts were drawn.
    pub fn flush(&mut self, min_z: f32, r: &mut dyn RenderBackend) -> usize {
        if !self.ready(min_z) {
            return 0;
        }
        let mut n = 0;
        for e in self.take_draw_order() {
            r.draw(&DrawBatch {
                mesh: e.mesh,
                texture: e.texture,
                transform: e.world_matrix,
                range: e.range,
            });
            n += 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use crate::testing::Recorder;

    fn entry(mesh: u32, order_key: f32) -> AlphaEntry {
        AlphaEntry {
            mesh: MeshHandle(mesh),
            surface_num: 0,
            texture: None,
            first_of_kind: true,
            // The transform's z stands in for depth, so a "helpful" sort would have something to
            // sort by. Nothing in this module may read it.
            world_matrix: Frame::new(
                dereth_primitives::Vec3::new(0.0, 0.0, order_key),
                dereth_primitives::Quat::IDENTITY,
            ),
            multipass: false,
            range: 0..3,
        }
    }

    /// Oracle: alpha-list flushing and contract 11.7 — "There is no depth sort: the visible
    /// order is exactly the order in which cells and parts were drawn."
    ///
    /// The scenario is deliberately reordering-detecting: entries are pushed in an order that is
    /// **not** sorted by depth, near-to-far or far-to-near, so any sorted structure produces a
    /// different sequence and this test fails. That is trap 11's whole point.
    #[test]
    fn the_alpha_list_flushes_in_insertion_order_and_a_sort_would_fail_this() {
        let mut lists = AlphaLists::new();
        // Depths 5, 1, 9, 3 -- neither ascending nor descending.
        for (mesh, z) in [(10u32, 5.0f32), (11, 1.0), (12, 9.0), (13, 3.0)] {
            assert!(lists.push(AlphaList::Blend, entry(mesh, z)));
        }
        let mut r = Recorder::new();
        assert_eq!(lists.flush(0.0, &mut r), 4);
        assert_eq!(
            r.draw_order(),
            vec![
                MeshHandle(10),
                MeshHandle(11),
                MeshHandle(12),
                MeshHandle(13)
            ],
            "insertion order, not depth order"
        );
        // Prove the scenario really is reordering-detecting: a depth sort gives a different answer.
        let mut sorted = [(10u32, 5.0f32), (11, 1.0), (12, 9.0), (13, 3.0)];
        sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_ne!(
            sorted.map(|(m, _)| MeshHandle(m)).to_vec(),
            r.draw_order(),
            "a near-to-far sort must differ from the recorded order"
        );
        sorted.reverse();
        assert_ne!(
            sorted.map(|(m, _)| MeshHandle(m)).to_vec(),
            r.draw_order(),
            "and so must a far-to-near sort"
        );
    }

    /// Oracle: `FlushAlphaList` — "draws the **clip list** in insertion order ... then the same for
    /// the **alpha list**". The two lists are separate and the clip list always goes first.
    #[test]
    fn the_clip_list_drains_before_the_blend_list() {
        let mut lists = AlphaLists::new();
        lists.push(AlphaList::Blend, entry(1, 0.0));
        lists.push(AlphaList::Clip, entry(2, 0.0));
        lists.push(AlphaList::Blend, entry(3, 0.0));
        lists.push(AlphaList::Clip, entry(4, 0.0));
        assert_eq!(
            lists
                .draw_order()
                .iter()
                .map(|e| e.mesh)
                .collect::<Vec<_>>(),
            vec![MeshHandle(2), MeshHandle(4), MeshHandle(1), MeshHandle(3)]
        );
        let mut r = Recorder::new();
        lists.flush(0.0, &mut r);
        assert_eq!(
            r.draw_order(),
            vec![MeshHandle(2), MeshHandle(4), MeshHandle(1), MeshHandle(3)]
        );
    }

    /// Oracle: `FlushAlphaList(t)` — "returns immediately unless `count >= t*3000` for at least one
    /// list, so `FlushAlphaList(0.0)` always flushes". The per-cell block draw calls it with
    /// `t = 0.75`, i.e. only once a list has 2 250 entries.
    #[test]
    fn the_threshold_defers_a_short_list_and_zero_always_flushes() {
        let mut lists = AlphaLists::new();
        for i in 0..10u32 {
            lists.push(AlphaList::Blend, entry(i, 0.0));
        }
        let mut r = Recorder::new();
        assert_eq!(
            lists.flush(0.75, &mut r),
            0,
            "ten entries is far below 0.75 * 3000"
        );
        assert!(r.draws.is_empty());
        assert_eq!(lists.len(), 10, "and nothing was lost");
        assert_eq!(lists.flush(0.0, &mut r), 10, "flush(0.0) always flushes");
        assert!(lists.is_empty());
    }

    /// The split flush agrees with flush itself.
    #[test]
    fn the_split_flush_agrees_with_flush_itself() {
        let build = || {
            let mut l = AlphaLists::new();
            l.push(AlphaList::Blend, entry(1, 0.0));
            l.push(AlphaList::Clip, entry(2, 0.0));
            l.push(AlphaList::Blend, entry(3, 0.0));
            l.push(AlphaList::Clip, entry(4, 0.0));
            l
        };
        // The two per-list counts, which `len()` cannot give.
        let l = build();
        assert_eq!((l.clip_len(), l.blend_len()), (2, 2));
        assert_eq!(l.len(), 4);

        // `ready(t)` is `FlushAlphaList`'s own early return, and `flush` must agree with it.
        let mut short = build();
        assert!(!short.ready(0.75), "four entries is far below 0.75 * 3000");
        let mut r = Recorder::new();
        assert_eq!(short.flush(0.75, &mut r), 0);
        assert_eq!(short.len(), 4, "a deferred flush loses nothing");
        assert!(short.ready(0.0), "flush(0.0) always flushes");

        // `take_draw_order` is `draw_order` and then the reset, in one.
        let mut a = build();
        let want: Vec<MeshHandle> = a.draw_order().iter().map(|e| e.mesh).collect();
        let taken: Vec<MeshHandle> = a.take_draw_order().iter().map(|e| e.mesh).collect();
        assert_eq!(taken, want, "clip list first, insertion order within each");
        assert_eq!(
            taken,
            vec![MeshHandle(2), MeshHandle(4), MeshHandle(1), MeshHandle(3)]
        );
        assert!(a.is_empty(), "and the lists are drained");
        assert_eq!((a.clip_len(), a.blend_len()), (0, 0));

        // And `flush` puts the same sequence on a backend.
        let mut b = build();
        let mut r = Recorder::new();
        assert_eq!(b.flush(0.0, &mut r), 4);
        assert_eq!(r.draw_order(), want);
        assert!(b.is_empty());
    }

    /// Oracle: each deferred list is capped at **3000** entries; insertion returns false
    /// and drops the subset when the list is full. The cap is a shipped behaviour: a 3 001st
    /// translucent subset is not drawn at all.
    #[test]
    fn a_full_list_drops_the_subset_rather_than_growing() {
        let mut lists = AlphaLists::new();
        for i in 0..ALPHA_LIST_CAP {
            // LINT-OK: index arithmetic over the 3000-entry cap.
            assert!(
                lists.push(AlphaList::Blend, entry(i as u32, 0.0)),
                "entry {i}"
            );
        }
        assert!(
            !lists.push(AlphaList::Blend, entry(9999, 0.0)),
            "the 3001st is dropped"
        );
        assert_eq!(lists.dropped, 1);
        assert_eq!(lists.len(), ALPHA_LIST_CAP);
        // The other list is independent and still has room.
        assert!(lists.push(AlphaList::Clip, entry(1, 0.0)));
    }

    /// Oracle: `firstOfKind` is true only for the **first** deferred
    /// subset of each list per mesh; when it is set the entry also records the current
    /// object-to-world matrix and the current material, and `FlushAlphaList` re-installs them
    /// before drawing that entry. Subsequent subsets of the same mesh inherit the matrix already
    /// set.
    #[test]
    fn only_the_first_subset_of_a_mesh_carries_its_matrix() {
        let mut lists = AlphaLists::new();
        let mut first = entry(7, 0.0);
        first.first_of_kind = true;
        let mut second = entry(7, 0.0);
        second.surface_num = 1;
        second.first_of_kind = false;
        lists.push(AlphaList::Blend, first);
        lists.push(AlphaList::Blend, second);
        let order = lists.draw_order();
        assert!(order[0].first_of_kind);
        assert!(!order[1].first_of_kind);
        assert_eq!(order[1].surface_num, 1);
        // Both still draw, in submission order.
        let mut r = Recorder::new();
        assert_eq!(lists.flush(0.0, &mut r), 2);
    }
}
