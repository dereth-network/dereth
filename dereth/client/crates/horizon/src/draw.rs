//! The interface's draw list, and the overlay it becomes over the finished world.
//!
//! The interface builds a list of [`Quad`]s each frame: a picture (or a flat colour), where it
//! goes on screen in pixels, which part of the picture, and a colour it is multiplied by. The
//! pictures are the [`crate::art::Art`] store's; [`Overlay`] uploads each through the
//! presentation the first time a list names it, keeps it for the life of the presentation, and
//! turns the list into the shared overlay's triangle batches.

use std::collections::BTreeMap;

use dereth_client_contract::overlay::{
    OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture, OverlayVertex,
    PreviewSpace,
};
use dereth_client_runtime::present::{PresentError, Presentation};
use dereth_primitives::{TextureData, TextureFormat, Viewport};

use crate::art::{Art, TexId};

/// An axis-aligned rectangle in pixels: left, top, width, height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// This rectangle moved by `(dx, dy)`.
    #[must_use]
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }

    /// This rectangle shrunk by `d` on every side.
    #[must_use]
    pub fn inset(&self, d: f32) -> Self {
        Self::new(
            self.x + d,
            self.y + d,
            (self.w - 2.0 * d).max(0.0),
            (self.h - 2.0 * d).max(0.0),
        )
    }

    /// The overlap of the two, or `None` when they do not meet.
    #[must_use]
    pub fn intersect(&self, o: &Self) -> Option<Self> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = self.right().min(o.right());
        let y1 = self.bottom().min(o.bottom());
        (x1 > x0 && y1 > y0).then(|| Self::new(x0, y0, x1 - x0, y1 - y0))
    }
}

std::thread_local! {
    /// The clip the draw list being built on this thread puts on what is drawn now.
    static CLIP: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// The clip what is drawn now is cut to, on the draw list being built: what the pad's focus
/// leaves out of reach, a list's rows scrolled out of sight.
#[must_use]
pub fn current_clip() -> Option<Rect> {
    CLIP.with(std::cell::Cell::get)
}

/// A colour, `0xAARRGGBB`.
pub type Argb = u32;

/// Opaque white: a picture drawn as it is.
pub const WHITE: Argb = 0xFFFF_FFFF;

/// `colour` with its alpha multiplied by `a` (0..1).
#[must_use]
pub fn with_alpha(colour: Argb, a: f32) -> Argb {
    let base = f32::from(u8::try_from(colour >> 24).unwrap_or(255));
    let alpha = dereth_primitives::num::to_i32((base * a.clamp(0.0, 1.0)).round()).clamp(0, 255);
    (colour & 0x00FF_FFFF) | (u32::try_from(alpha).unwrap_or(0) << 24)
}

/// One quad of the interface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quad {
    /// The picture, or `None` for a flat colour.
    pub tex: Option<TexId>,
    /// Where on screen, in pixels.
    pub dst: Rect,
    /// Which part of the picture, in its pixels. Ignored for a flat colour.
    pub src: Rect,
    /// The colour the picture's texels are multiplied by.
    pub colour: Argb,
    /// The part of the screen the quad may touch.
    pub clip: Option<Rect>,
    /// Turned this many radians clockwise about its centre (a turned quad is not clipped).
    pub turn: f32,
}

/// A frame's quads, in drawing order.
#[derive(Debug, Default, Clone)]
pub struct DrawList {
    pub quads: Vec<Quad>,
    /// The clip every quad pushed from now on takes, innermost last.
    clips: Vec<Rect>,
    /// The quads that stand over something in the world, and where it stood when they were
    /// drawn: [`Self::follow`] moves them to where it stands when the world is drawn.
    pub anchored: Vec<Anchored>,
}

/// Which point of an object in the world some quads stand on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The top of its body as drawn: a name over it.
    Top,
    /// Its origin, raised to the middle of its selection sphere: a ring round it.
    Origin,
}

/// Quads drawn over an object in the world.
#[derive(Debug, Clone, PartialEq)]
pub struct Anchored {
    pub id: dereth_primitives::ObjectId,
    pub anchor: Anchor,
    /// The quads, in the list.
    pub quads: std::ops::Range<usize>,
    /// Where the point they stand on was on screen when they were drawn.
    pub at: (f32, f32),
    /// Where they were drawn from before being put on whole pixels (a line of text's top left):
    /// moved, they land on the whole pixels they would have been drawn on there.
    pub snap: (f32, f32),
}

impl DrawList {
    /// Forget last frame's quads.
    pub fn clear(&mut self) {
        self.quads.clear();
        self.clips.clear();
        self.anchored.clear();
        CLIP.with(|c| c.set(None));
    }

    /// Note that the quads from `from` to the end of the list stand on `anchor` of `id`, which
    /// was at `at` on screen, drawn from `snap`.
    pub fn anchor(
        &mut self,
        id: dereth_primitives::ObjectId,
        anchor: Anchor,
        from: usize,
        at: (f32, f32),
        snap: (f32, f32),
    ) {
        if from < self.quads.len() {
            self.anchored.push(Anchored {
                id,
                anchor,
                quads: from..self.quads.len(),
                at,
                snap,
            });
        }
    }

    /// Move every anchored quad to where the point it stands on is now, as `now` says (`None`:
    /// nowhere on screen, and it is not drawn). The list is drawn up before the world moves for
    /// the frame; this is called once it has, so what stands over the world stands over it as it
    /// is drawn.
    pub fn follow(
        &mut self,
        now: impl Fn(dereth_primitives::ObjectId, Anchor) -> Option<(f32, f32)>,
    ) {
        let Self {
            quads: list,
            anchored,
            ..
        } = self;
        for a in anchored {
            let Some(quads) = list.get_mut(a.quads.clone()) else {
                continue;
            };
            match now(a.id, a.anchor) {
                Some((x, y)) => {
                    let snap = (a.snap.0 + x - a.at.0, a.snap.1 + y - a.at.1);
                    let dx = snap.0.round() - a.snap.0.round();
                    let dy = snap.1.round() - a.snap.1.round();
                    for q in quads {
                        q.dst = q.dst.offset(dx, dy);
                    }
                    a.at = (x, y);
                    a.snap = snap;
                }
                None => {
                    for q in quads {
                        q.dst = Rect::new(q.dst.x, q.dst.y, 0.0, 0.0);
                    }
                }
            }
        }
    }

    /// Clip everything pushed until the matching [`Self::pop_clip`] to `r` (and any outer clip).
    pub fn push_clip(&mut self, r: Rect) {
        let r = match self.clips.last() {
            Some(outer) => outer.intersect(&r).unwrap_or(Rect::new(r.x, r.y, 0.0, 0.0)),
            None => r,
        };
        self.clips.push(r);
        CLIP.with(|c| c.set(Some(r)));
    }

    pub fn pop_clip(&mut self) {
        self.clips.pop();
        CLIP.with(|c| c.set(self.clips.last().copied()));
    }

    /// How many quads are in the list so far: where something drawn between them goes.
    #[must_use]
    pub fn mark(&self) -> usize {
        self.quads.len()
    }

    /// Draw part `src` of picture `tex` at `dst`, multiplied by `colour`.
    pub fn image(&mut self, tex: TexId, src: Rect, dst: Rect, colour: Argb) {
        if dst.w <= 0.0 || dst.h <= 0.0 || colour >> 24 == 0 {
            return;
        }
        self.quads.push(Quad {
            tex: Some(tex),
            dst,
            src,
            colour,
            clip: self.clips.last().copied(),
            turn: 0.0,
        });
    }

    /// As [`Self::image`], turned `turn` radians clockwise about the centre of `dst`.
    pub fn image_turned(&mut self, tex: TexId, src: Rect, dst: Rect, colour: Argb, turn: f32) {
        if dst.w <= 0.0 || dst.h <= 0.0 || colour >> 24 == 0 {
            return;
        }
        self.quads.push(Quad {
            tex: Some(tex),
            dst,
            src,
            colour,
            clip: None,
            turn,
        });
    }

    /// A flat rectangle of `colour`.
    pub fn fill(&mut self, dst: Rect, colour: Argb) {
        if dst.w <= 0.0 || dst.h <= 0.0 || colour >> 24 == 0 {
            return;
        }
        self.quads.push(Quad {
            tex: None,
            dst,
            src: Rect::default(),
            colour,
            clip: self.clips.last().copied(),
            turn: 0.0,
        });
    }
}

/// What the overlay has drawn so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OverlayStats {
    pub textures_uploaded: u64,
    pub upload_failures: u64,
    pub quads_drawn: u64,
    pub quads_clipped: u64,
}

/// The keys this interface's pictures are uploaded under: their art ids in a range of their own,
/// so they never meet another interface's.
const KEY_BASE: u64 = 0x5849_5600_0000_0000;

/// The key of the one white texel the flat fills sample, in the low half's last slot.
const WHITE_SLOT: u64 = 0xFFFF_FFFF;

/// Picture `slot`'s key in `generation`: each set of art (each reading of the files) uploads under
/// keys of its own, so nothing let go of with the old set can take a picture of the new one with
/// it.
fn key(generation: u64, slot: u64) -> u64 {
    KEY_BASE | ((generation & 0xFF_FFFF) << 32) | slot
}

/// A picture uploaded: its key and its size in texels.
type Uploaded = (OverlayTexture, (u32, u32));

/// The overlay: every picture uploaded so far, and this frame's batches.
#[derive(Debug, Default)]
pub struct Overlay {
    textures: BTreeMap<TexId, Option<Uploaded>>,
    white: Option<OverlayTexture>,
    items: Vec<OverlayItem>,
    /// Which set of art the uploads belong to.
    generation: u64,
    pub stats: OverlayStats,
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
        let size = present.size();
        self.items.clear();
        if portal {
            self.items.push(OverlayItem::Preview {
                space: PreviewSpace::Portal,
                rect: Viewport {
                    x: 0,
                    y: 0,
                    width: size.0,
                    height: size.1,
                },
            });
        }
        let mut spaces: Vec<(PreviewSpace, Rect, usize)> = spaces.to_vec();
        spaces.sort_by_key(|(_, _, at)| *at);
        let mut spaces = spaces.into_iter().peekable();
        if self.white.is_none() {
            let key = OverlayTexture {
                space: OverlaySpace::Local,
                key: key(self.generation, WHITE_SLOT),
            };
            let white = TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![255, 255, 255, 255]],
            };
            self.white = present.overlay_upload(key, &white).ok().map(|()| key);
        }
        for (i, q) in list.quads.iter().enumerate() {
            while let Some((space, r, _)) = spaces.next_if(|(_, _, at)| *at <= i) {
                self.items.push(preview(space, r));
            }
            let (texture, tex_size) = match q.tex {
                Some(id) => match self.upload(present, art, id) {
                    Some(u) => u,
                    None => continue,
                },
                None => match self.white {
                    Some(w) => (w, (1, 1)),
                    None => continue,
                },
            };
            let Some(vertices) = quad_vertices(q, tex_size, size) else {
                self.stats.quads_clipped += 1;
                continue;
            };
            // Linear filtering unless the picture is drawn texel for texel.
            let scaled = q.tex.is_some()
                && ((q.dst.w - q.src.w).abs() > 0.01 || (q.dst.h - q.src.h).abs() > 0.01);
            let sampler = OverlaySampler {
                point: !scaled,
                wrap_u: false,
                wrap_v: false,
            };
            self.stats.quads_drawn += 1;
            if let Some(OverlayItem::Triangles {
                texture: last,
                sampler: last_sampler,
                vertices: batch,
                ..
            }) = self.items.last_mut()
            {
                if *last == texture && *last_sampler == sampler {
                    batch.extend_from_slice(&vertices);
                    continue;
                }
            }
            self.items.push(OverlayItem::Triangles {
                material: OverlayMaterial::Image,
                texture,
                sampler,
                vertices: vertices.to_vec(),
            });
        }
        for (space, r, _) in spaces {
            self.items.push(preview(space, r));
        }
    }

    /// Picture `id`, uploaded the first time it is asked for.
    fn upload<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        art: &Art,
        id: TexId,
    ) -> Option<Uploaded> {
        if let Some(hit) = self.textures.get(&id) {
            return *hit;
        }
        let key = OverlayTexture {
            space: OverlaySpace::Local,
            key: key(self.generation, u64::from(id)),
        };
        let uploaded = art.image(id).and_then(|image| {
            let data = TextureData {
                width: image.width,
                height: image.height,
                format: TextureFormat::Bgra8,
                levels: vec![image.bgra.clone()],
            };
            present
                .overlay_upload(key, &data)
                .ok()
                .map(|()| (key, (image.width, image.height)))
        });
        if uploaded.is_some() {
            self.stats.textures_uploaded += 1;
        } else {
            self.stats.upload_failures += 1;
        }
        self.textures.insert(id, uploaded);
        uploaded
    }

    /// Let go of every picture uploaded, as a new set of art (the files read again) replaces
    /// them.
    pub fn release<P: Presentation + ?Sized>(&mut self, present: &mut P) {
        for (key, _) in std::mem::take(&mut self.textures).into_values().flatten() {
            let _ = present.overlay_release(key);
        }
        if let Some(white) = self.white.take() {
            let _ = present.overlay_release(white);
        }
        self.generation += 1;
        self.items.clear();
    }

    /// Draw this frame's batches over the frame, in order. Inside the frame bracket.
    ///
    /// # Errors
    /// Whatever the device answers.
    pub fn draw<P: Presentation + ?Sized>(&self, present: &mut P) -> Result<(), PresentError> {
        if self.items.is_empty() {
            return Ok(());
        }
        present.draw_overlay(&self.items)
    }

    /// This frame's batches, as [`Self::compose`] built them.
    #[must_use]
    pub fn items(&self) -> &[OverlayItem] {
        &self.items
    }
}

/// Preview space `space`, drawn into `r` on screen.
fn preview(space: PreviewSpace, r: Rect) -> OverlayItem {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a rectangle on screen, clamped to it, a few thousand pixels at most.
    let px = |v: f32| v.max(0.0).round() as u32;
    OverlayItem::Preview {
        space,
        rect: Viewport {
            x: px(r.x),
            y: px(r.y),
            width: px(r.w),
            height: px(r.h),
        },
    }
}

/// The six vertices of `q`, after its clip, on a back buffer of `fb` pixels; `None` when nothing
/// of it is left.
#[must_use]
pub fn quad_vertices(q: &Quad, tex_size: (u32, u32), fb: (u32, u32)) -> Option<[OverlayVertex; 6]> {
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a texture or back-buffer extent, at most a few thousand, exact in f32.
    let (tw, th, fw, fh) = (
        tex_size.0.max(1) as f32,
        tex_size.1.max(1) as f32,
        fb.0.max(1) as f32,
        fb.1.max(1) as f32,
    );
    let screen = Rect::new(0.0, 0.0, fw, fh);
    if q.turn != 0.0 {
        return turned_vertices(q, (tw, th), (fw, fh));
    }
    let visible = match q.clip {
        Some(c) => q.dst.intersect(&c)?.intersect(&screen)?,
        None => q.dst.intersect(&screen)?,
    };
    // The part of the source the visible part of the destination shows.
    let (sx, sy) = (q.src.w / q.dst.w, q.src.h / q.dst.h);
    let (u0, v0, u1, v1) = if q.tex.is_some() {
        (
            (q.src.x + (visible.x - q.dst.x) * sx) / tw,
            (q.src.y + (visible.y - q.dst.y) * sy) / th,
            (q.src.x + (visible.right() - q.dst.x) * sx) / tw,
            (q.src.y + (visible.bottom() - q.dst.y) * sy) / th,
        )
    } else {
        (0.0, 0.0, 1.0, 1.0)
    };
    let cx = |x: f32| x / fw * 2.0 - 1.0;
    let cy = |y: f32| 1.0 - y / fh * 2.0;
    let (l, t, r, b) = (
        cx(visible.x),
        cy(visible.y),
        cx(visible.right()),
        cy(visible.bottom()),
    );
    let v = |x: f32, y: f32, u: f32, v: f32| OverlayVertex {
        position: [x, y, 0.5],
        color: q.colour,
        uv: [u, v],
    };
    Some([
        v(l, t, u0, v0),
        v(l, b, u0, v1),
        v(r, b, u1, v1),
        v(r, b, u1, v1),
        v(r, t, u1, v0),
        v(l, t, u0, v0),
    ])
}

/// The six vertices of a turned quad: its corners turned about its centre, the whole of its
/// source; `None` when it lies wholly off the screen.
fn turned_vertices(q: &Quad, tex: (f32, f32), fb: (f32, f32)) -> Option<[OverlayVertex; 6]> {
    let (tw, th) = tex;
    let (fw, fh) = fb;
    let (cx, cy) = (q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h / 2.0);
    // At least half the diagonal: enough to tell a quad wholly off the screen.
    let reach = (q.dst.w + q.dst.h) / 2.0;
    if cx + reach < 0.0 || cy + reach < 0.0 || cx - reach > fw || cy - reach > fh {
        return None;
    }
    let (s, c) = (
        dereth_primitives::num::math::sinf(q.turn),
        dereth_primitives::num::math::cosf(q.turn),
    );
    let corner = |dx: f32, dy: f32| {
        let (x, y) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
        (x / fw * 2.0 - 1.0, 1.0 - y / fh * 2.0)
    };
    let (hw, hh) = (q.dst.w / 2.0, q.dst.h / 2.0);
    let (lt, lb, rb, rt) = (
        corner(-hw, -hh),
        corner(-hw, hh),
        corner(hw, hh),
        corner(hw, -hh),
    );
    let (u0, v0, u1, v1) = if q.tex.is_some() {
        (
            q.src.x / tw,
            q.src.y / th,
            q.src.right() / tw,
            q.src.bottom() / th,
        )
    } else {
        (0.0, 0.0, 1.0, 1.0)
    };
    let v = |p: (f32, f32), u: f32, v: f32| OverlayVertex {
        position: [p.0, p.1, 0.5],
        color: q.colour,
        uv: [u, v],
    };
    Some([
        v(lt, u0, v0),
        v(lb, u0, v1),
        v(rb, u1, v1),
        v(rb, u1, v1),
        v(rt, u1, v0),
        v(lt, u0, v0),
    ])
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
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
}
