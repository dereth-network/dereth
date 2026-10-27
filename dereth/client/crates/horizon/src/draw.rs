//! The interface's draw list, and the overlay it becomes over the finished world: the kit's
//! ([`dereth_ui_kit::draw`]), uploading through the presentation under this interface's own key
//! range. A name standing over a body is hidden behind the world where it stands in front, every
//! quad is drawn on whole pixels, and a picture drawn at another size than its own is drawn from
//! a copy made at that size.

use dereth_client_contract::overlay::{OverlayItem, OverlayTexture, PreviewSpace};
use dereth_client_runtime::present::{PresentError, Presentation};
use dereth_primitives::TextureData;

pub use dereth_ui_kit::draw::{
    current_clip, on_pixels, quad_vertices, whole, with_alpha, Anchor, Anchored, Argb, DrawList,
    OverlayStats, Quad, Rect, Spot, TextureSink, MADE_KEPT, MADE_PER_FRAME, WHITE,
};

use crate::art::Art;

/// The keys this interface's pictures are uploaded under: their art ids in a range of their own,
/// so they never meet another interface's.
const KEY_BASE: u64 = 0x5849_5600_0000_0000;

/// A presentation as the kit's texture sink.
struct Sink<'a, P: ?Sized>(&'a mut P);

impl<P: Presentation + ?Sized> TextureSink for Sink<'_, P> {
    fn size(&self) -> (u32, u32) {
        self.0.size()
    }
    fn upload(&mut self, key: OverlayTexture, data: &TextureData) -> bool {
        self.0.overlay_upload(key, data).is_ok()
    }
    fn release(&mut self, key: OverlayTexture) {
        let _ = self.0.overlay_release(key);
    }
}

/// The overlay: every picture uploaded so far, and this frame's batches.
#[derive(Debug)]
pub struct Overlay(dereth_ui_kit::draw::Overlay);

impl Default for Overlay {
    fn default() -> Self {
        Self(dereth_ui_kit::draw::Overlay::new(KEY_BASE))
    }
}

impl std::ops::Deref for Overlay {
    type Target = dereth_ui_kit::draw::Overlay;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Overlay {
    /// Turn `list` into this frame's batches, uploading every picture it names that is not
    /// uploaded yet. `portal` puts the portal space under everything, over the whole screen;
    /// `spaces` are other preview spaces, each drawn into its rectangle over the first `n` quads
    /// of the list and under the rest (`0` puts it under the whole interface). Outside the frame
    /// bracket.
    pub fn compose<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        art: &Art,
        list: &DrawList,
        portal: bool,
        spaces: &[(PreviewSpace, Rect, usize)],
    ) {
        let under = portal.then_some(PreviewSpace::Portal);
        self.0.compose(&mut Sink(present), art, list, under, spaces);
    }

    /// Let go of every picture uploaded, as a new set of art (the files read again) replaces
    /// them.
    pub fn release<P: Presentation + ?Sized>(&mut self, present: &mut P) {
        self.0.release(&mut Sink(present));
    }

    /// Draw this frame's batches over the frame, in order. Inside the frame bracket.
    ///
    /// # Errors
    /// Whatever the device answers.
    pub fn draw<P: Presentation + ?Sized>(&self, present: &mut P) -> Result<(), PresentError> {
        if self.0.items().is_empty() {
            return Ok(());
        }
        present.draw_overlay(self.0.items())
    }

    /// This frame's batches, as [`Self::compose`] built them.
    #[must_use]
    pub fn items(&self) -> &[OverlayItem] {
        self.0.items()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use crate::art::TexId;
    use dereth_client_contract::overlay::OverlaySampler;
    use dereth_client_runtime::present::NullPresentation;

    fn textures(items: &[OverlayItem]) -> Vec<OverlayTexture> {
        items
            .iter()
            .filter_map(|i| match i {
                OverlayItem::Triangles { texture, .. } => Some(*texture),
                OverlayItem::Preview { .. } => None,
            })
            .collect()
    }

    #[test]
    fn a_picture_drawn_after_the_art_is_replaced_is_uploaded_under_a_key_of_its_own() {
        let mut present = NullPresentation::new(64, 64);
        let art = Art::empty();
        let id = art.add_image(crate::art::Image::solid(4, 4, [1, 2, 3, 255]));
        let mut list = DrawList::default();
        list.image(
            id,
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Rect::new(0.0, 0.0, 8.0, 8.0),
            WHITE,
        );
        list.fill(Rect::new(8.0, 8.0, 4.0, 4.0), WHITE);
        let mut overlay = Overlay::default();
        overlay.compose(&mut present, &art, &list, false, &[]);
        let before = textures(overlay.items());
        overlay.release(&mut present);
        let art = Art::empty();
        let again = art.add_image(crate::art::Image::solid(4, 4, [9, 9, 9, 255]));
        assert_eq!(again, id, "the new art hands out the same picture id");
        overlay.compose(&mut present, &art, &list, false, &[]);
        let after = textures(overlay.items());
        assert_eq!(before.len(), 2);
        assert_eq!(
            after.len(),
            2,
            "both the picture and the flat fill are drawn again"
        );
        assert!(before.iter().all(|k| !after.contains(k)));
    }

    /// A preview placed at a mark in the list is drawn over what was drawn before the mark and
    /// under what was drawn after it; one placed at the start is under everything.
    #[test]
    fn a_preview_is_drawn_between_the_quads_either_side_of_its_mark() {
        let mut present = NullPresentation::new(64, 64);
        let art = Art::empty();
        let mut list = DrawList::default();
        list.fill(Rect::new(0.0, 0.0, 8.0, 8.0), WHITE);
        let mark = list.mark();
        list.fill(Rect::new(8.0, 8.0, 8.0, 8.0), WHITE);
        let mut overlay = Overlay::default();
        let doll = Rect::new(0.0, 0.0, 16.0, 16.0);
        overlay.compose(
            &mut present,
            &art,
            &list,
            false,
            &[
                (PreviewSpace::PaperDoll, doll, mark),
                (PreviewSpace::CharGen, doll, 0),
            ],
        );
        let kinds: Vec<&str> = overlay
            .items()
            .iter()
            .map(|i| match i {
                OverlayItem::Triangles { .. } => "quads",
                OverlayItem::Preview {
                    space: PreviewSpace::PaperDoll,
                    ..
                } => "doll",
                OverlayItem::Preview { .. } => "other",
            })
            .collect();
        assert_eq!(kinds, ["other", "quads", "doll", "quads"]);
    }

    /// Each corner of every quad of `items`, in pixels on a back buffer `fb` pixels across and
    /// down, with the sampler it is drawn with.
    fn corners(items: &[OverlayItem], fb: (f32, f32)) -> Vec<((f32, f32), OverlaySampler)> {
        items
            .iter()
            .flat_map(|i| match i {
                OverlayItem::Triangles {
                    vertices, sampler, ..
                } => vertices
                    .iter()
                    .map(|v| {
                        (
                            (
                                (v.position[0] + 1.0) / 2.0 * fb.0,
                                (1.0 - v.position[1]) / 2.0 * fb.1,
                            ),
                            *sampler,
                        )
                    })
                    .collect(),
                OverlayItem::Preview { .. } => Vec::new(),
            })
            .collect()
    }

    /// A list of one picture, eight texels square, drawn `size` pixels square at `(x, y)`.
    fn one_picture(art: &Art, x: f32, y: f32, size: f32) -> (DrawList, TexId) {
        let id = art.add_image(crate::art::Image::solid(8, 8, [10, 20, 30, 255]));
        let mut list = DrawList::default();
        list.image(
            id,
            Rect::new(0.0, 0.0, 8.0, 8.0),
            Rect::new(x, y, size, size),
            WHITE,
        );
        (list, id)
    }

    /// Fills and pictures alike land on whole pixels, wherever they were asked to stand; one
    /// thinner than a pixel is a pixel wide.
    #[test]
    fn every_quad_is_drawn_on_whole_pixels_and_none_thinner_than_one() {
        let mut present = NullPresentation::new(200, 100);
        let art = Art::empty();
        let (mut list, _) = one_picture(&art, 10.4, 20.6, 8.0);
        list.fill(Rect::new(30.3, 40.7, 12.2, 0.4), WHITE);
        let mut overlay = Overlay::default();
        overlay.compose(&mut present, &art, &list, false, &[]);
        let corners = corners(overlay.items(), (200.0, 100.0));
        assert_eq!(corners.len(), 12, "two quads drawn");
        for ((x, y), _) in &corners {
            assert!(
                (x - x.round()).abs() < 1e-3 && (y - y.round()).abs() < 1e-3,
                "a corner at ({x}, {y})"
            );
        }
        let ys: Vec<f32> = corners[6..].iter().map(|((_, y), _)| *y).collect();
        let tall = ys.iter().copied().fold(f32::MIN, f32::max)
            - ys.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            (tall - 1.0).abs() < 1e-3,
            "the thin fill is {tall} pixels tall"
        );
        assert!(
            corners[..6].iter().all(|(_, s)| s.point),
            "the picture texel for texel"
        );
    }

    /// A picture drawn at another size than its own is stretched on the frame it first stands at
    /// that size, and from the next is drawn texel for texel from a copy made at that size, made
    /// once.
    #[test]
    fn a_picture_drawn_at_another_size_is_drawn_from_a_copy_made_at_that_size() {
        let mut present = NullPresentation::new(200, 100);
        let art = Art::empty();
        let (list, _) = one_picture(&art, 10.0, 10.0, 13.0);
        let mut overlay = Overlay::default();
        overlay.compose(&mut present, &art, &list, false, &[]);
        assert!(
            corners(overlay.items(), (200.0, 100.0))
                .iter()
                .all(|(_, s)| !s.point),
            "the first frame stretches it"
        );
        overlay.compose(&mut present, &art, &list, false, &[]);
        assert_eq!(overlay.stats.pictures_made, 1);
        assert!(
            corners(overlay.items(), (200.0, 100.0))
                .iter()
                .all(|(_, s)| s.point),
            "then it is drawn texel for texel"
        );
        let OverlayItem::Triangles {
            texture, vertices, ..
        } = &overlay.items()[0]
        else {
            panic!("the picture's triangles");
        };
        assert!(vertices.iter().all(|v| v.uv[0] == 0.0 || v.uv[0] == 1.0));
        let made = *texture;
        for _ in 0..3 {
            overlay.compose(&mut present, &art, &list, false, &[]);
        }
        assert_eq!(overlay.stats.pictures_made, 1, "made once");
        assert_eq!(textures(overlay.items()), [made]);
    }

    /// A picture whose size changes from frame to frame (a window growing as it opens) is
    /// stretched, not made at every size it passes through.
    #[test]
    fn a_picture_whose_size_changes_every_frame_is_not_made() {
        let mut present = NullPresentation::new(200, 100);
        let art = Art::empty();
        let mut overlay = Overlay::default();
        for n in 0..8 {
            #[allow(clippy::cast_precision_loss)]
            let (list, _) = one_picture(&art, 10.0, 10.0, 12.0 + n as f32);
            overlay.compose(&mut present, &art, &list, false, &[]);
        }
        assert_eq!(overlay.stats.pictures_made, 0);
    }

    /// A copy no frame has drawn for a while is let go of, and the overlay keeps no more.
    #[test]
    fn a_copy_left_undrawn_for_a_while_is_let_go_of() {
        let mut present = NullPresentation::new(200, 100);
        let art = Art::empty();
        let (list, _) = one_picture(&art, 10.0, 10.0, 13.0);
        let mut overlay = Overlay::default();
        overlay.compose(&mut present, &art, &list, false, &[]);
        overlay.compose(&mut present, &art, &list, false, &[]);
        assert_eq!(overlay.made_kept(), 1);
        let released = present.counts().overlay_releases;
        let empty = DrawList::default();
        for _ in 0..MADE_KEPT {
            overlay.compose(&mut present, &art, &empty, false, &[]);
        }
        assert_eq!(overlay.made_kept(), 1, "kept for a while");
        overlay.compose(&mut present, &art, &empty, false, &[]);
        assert_eq!(overlay.made_kept(), 0);
        assert_eq!(present.counts().overlay_releases, released + 1);
    }
}
