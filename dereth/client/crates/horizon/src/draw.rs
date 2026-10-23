//! The interface's draw list, and the overlay it becomes over the finished world.
//!
//! The interface builds a list of [`Quad`]s each frame: a picture (or a flat colour), where it
//! goes on screen in pixels, which part of the picture, and a colour it is multiplied by. The
//! pictures are the [`crate::art::Art`] store's; [`Overlay`] uploads each through the
//! presentation the first time a list names it, keeps it for the life of the presentation, and
//! turns the list into the shared overlay's triangle batches.
//!
//! What stands in the world, a name over a body, is hidden behind the world where it stands in
//! front: its quads carry the depth of the point they stand on, and the presentation draws them
//! only where the world drawn this frame is farther away.
//!
//! Every quad is drawn on whole pixels. A picture drawn larger or smaller than its own texels is
//! drawn from a copy made at the size it is drawn ([`crate::resample`]), texel for texel, rather
//! than stretched on the device; the copies are made a few at a time and let go of when they go
//! unused.

use std::collections::{BTreeMap, HashMap};

use dereth_client_contract::overlay::{
    OverlayDepth, OverlayItem, OverlayMaterial, OverlaySampler, OverlaySpace, OverlayTexture,
    OverlayVertex, PreviewSpace,
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
    /// How deep in the world it stands, as the world's projection gives it (0 at the near plane,
    /// 1 at the far): it is hidden wherever the world drawn this frame is nearer. `None`: drawn
    /// over everything.
    pub depth: Option<f32>,
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
    /// The point over its body a name stands on.
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
    /// They stand in the world, and are hidden behind it at the depth of the point they stand
    /// on; otherwise they are drawn over it.
    pub in_world: bool,
}

/// Where the point some quads stand on is, as the world is drawn this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// On screen, in pixels.
    pub x: f32,
    pub y: f32,
    /// Its depth in the world as drawn, as the world's projection gives it (0 at the near plane,
    /// 1 at the far); `None` where that is not known.
    pub depth: Option<f32>,
}

impl Spot {
    /// A point on screen whose depth is not known.
    #[must_use]
    pub const fn at(x: f32, y: f32) -> Self {
        Self { x, y, depth: None }
    }
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
    /// was at `at` on screen, drawn from `snap`, and are drawn over the world.
    pub fn anchor(
        &mut self,
        id: dereth_primitives::ObjectId,
        anchor: Anchor,
        from: usize,
        at: (f32, f32),
        snap: (f32, f32),
    ) {
        self.anchor_as(id, anchor, from, (at, snap), false);
    }

    /// As [`Self::anchor`], for quads that stand in the world: [`Self::follow`] gives them the
    /// depth of the point they stand on, and they are hidden wherever the world is nearer.
    pub fn anchor_in_world(
        &mut self,
        id: dereth_primitives::ObjectId,
        anchor: Anchor,
        from: usize,
        at: (f32, f32),
        snap: (f32, f32),
    ) {
        self.anchor_as(id, anchor, from, (at, snap), true);
    }

    fn anchor_as(
        &mut self,
        id: dereth_primitives::ObjectId,
        anchor: Anchor,
        from: usize,
        (at, snap): ((f32, f32), (f32, f32)),
        in_world: bool,
    ) {
        if from < self.quads.len() {
            self.anchored.push(Anchored {
                id,
                anchor,
                quads: from..self.quads.len(),
                at,
                snap,
                in_world,
            });
        }
    }

    /// Move every anchored quad to where the point it stands on is now, as `now` says (`None`:
    /// nowhere on screen, and it is not drawn), and give those that stand in the world its depth.
    /// The list is drawn up before the world moves for the frame; this is called once it has, so
    /// what stands over the world stands over it as it is drawn.
    pub fn follow(&mut self, now: impl Fn(dereth_primitives::ObjectId, Anchor) -> Option<Spot>) {
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
                Some(Spot { x, y, depth }) => {
                    let snap = (a.snap.0 + x - a.at.0, a.snap.1 + y - a.at.1);
                    let dx = snap.0.round() - a.snap.0.round();
                    let dy = snap.1.round() - a.snap.1.round();
                    let depth = depth.filter(|_| a.in_world);
                    for q in quads {
                        q.dst = q.dst.offset(dx, dy);
                        q.depth = depth;
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
            depth: None,
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
            depth: None,
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
            depth: None,
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
    /// Pictures made at the size they are drawn.
    pub pictures_made: u64,
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

/// The keys of the pictures made at the size they are drawn: a range of their own beside the
/// art's.
const MADE_BASE: u64 = 0x5849_5700_0000_0000;

/// The key of the `n`th picture made at its drawn size in `generation`.
fn made_key(generation: u64, n: u64) -> u64 {
    MADE_BASE | ((generation & 0xFF_FFFF) << 32) | (n & 0xFFFF_FFFF)
}

/// How many pixels of pictures made at their drawn size one frame makes, at most: a change of
/// scale spreads the work over a few frames, and until a picture is made it is stretched on the
/// device.
pub const MADE_PER_FRAME: u64 = 150_000;

/// How many frames a picture made at its drawn size is kept unused before it is let go.
const MADE_KEPT: u64 = 600;

/// What a picture made at its drawn size was made from: the picture, the part of it (its edges'
/// bits) and the size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct MadeFrom {
    tex: TexId,
    src: [u32; 4],
    size: (u32, u32),
}

/// A picture made at its drawn size: where it was uploaded (`None` when the upload failed), and
/// the last frame that drew it.
#[derive(Debug, Clone, Copy)]
struct Made {
    texture: Option<OverlayTexture>,
    used: u64,
}

/// A picture uploaded: its key and its size in texels.
type Uploaded = (OverlayTexture, (u32, u32));

/// The overlay: every picture uploaded so far, and this frame's batches.
#[derive(Debug)]
pub struct Overlay {
    textures: BTreeMap<TexId, Option<Uploaded>>,
    white: Option<OverlayTexture>,
    items: Vec<OverlayItem>,
    /// Which set of art the uploads belong to.
    generation: u64,
    /// The pictures made at the size they are drawn, and how many have been made.
    made: HashMap<MadeFrom, Made>,
    made_count: u64,
    /// The pictures last frame drew at a size not yet made: one drawn at the same size again is
    /// made, and one whose size changes every frame (a window growing as it opens) is not.
    asked: HashMap<MadeFrom, u64>,
    /// How many frames have been composed.
    frame: u64,
    /// How many pixels of pictures one frame makes at most ([`MADE_PER_FRAME`]).
    pub made_per_frame: u64,
    pub stats: OverlayStats,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            textures: BTreeMap::new(),
            white: None,
            items: Vec::new(),
            generation: 0,
            made: HashMap::new(),
            made_count: 0,
            asked: HashMap::new(),
            frame: 0,
            made_per_frame: MADE_PER_FRAME,
            stats: OverlayStats::default(),
        }
    }
}

/// `v` on the nearest whole pixel, halves upward, so a run of whole steps from any start stays
/// whole steps apart.
#[must_use]
pub fn whole(v: f32) -> f32 {
    (v + 0.5).floor()
}

/// `r` with its edges on whole pixels; one of some size never narrower than a pixel.
fn snapped(r: Rect) -> Rect {
    let (x0, y0) = (whole(r.x), whole(r.y));
    let at_least = |size: f32| if size > 0.0 { 1.0 } else { 0.0 };
    let x1 = whole(r.right()).max(x0 + at_least(r.w));
    let y1 = whole(r.bottom()).max(y0 + at_least(r.h));
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// `q` on whole pixels: its edges and its clip's. A turned quad is left as it is.
#[must_use]
pub fn on_pixels(q: &Quad) -> Quad {
    if q.turn != 0.0 {
        return *q;
    }
    Quad {
        dst: snapped(q.dst),
        clip: q.clip.map(snapped),
        ..*q
    }
}

/// Whether `q` draws its picture texel for texel: as many pixels as the part of the picture it
/// shows has texels.
fn texel_for_texel(q: &Quad) -> bool {
    (q.dst.w - q.src.w).abs() <= 0.01 && (q.dst.h - q.src.h).abs() <= 0.01
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
        self.frame += 1;
        let mut spent = 0;
        for (i, q) in list.quads.iter().enumerate() {
            while let Some((space, r, _)) = spaces.next_if(|(_, _, at)| *at <= i) {
                self.items.push(preview(space, r));
            }
            let mut q = on_pixels(q);
            let (texture, tex_size) = match q.tex {
                Some(id) => {
                    let made = (q.turn == 0.0 && !texel_for_texel(&q))
                        .then(|| self.made(present, art, &q, &mut spent))
                        .flatten();
                    match made {
                        Some(made) => {
                            q.src = Rect::new(0.0, 0.0, q.dst.w, q.dst.h);
                            made
                        }
                        None => match self.upload(present, art, id) {
                            Some(u) => u,
                            None => continue,
                        },
                    }
                }
                None => match self.white {
                    Some(w) => (w, (1, 1)),
                    None => continue,
                },
            };
            let q = &q;
            let Some(vertices) = quad_vertices(q, tex_size, size) else {
                self.stats.quads_clipped += 1;
                continue;
            };
            // Linear filtering unless the picture is drawn texel for texel.
            let scaled = q.tex.is_some() && !texel_for_texel(q);
            let sampler = OverlaySampler {
                point: !scaled,
                wrap_u: false,
                wrap_v: false,
            };
            let depth = if q.depth.is_some() {
                OverlayDepth::World
            } else {
                OverlayDepth::Over
            };
            self.stats.quads_drawn += 1;
            if let Some(OverlayItem::Triangles {
                texture: last,
                sampler: last_sampler,
                depth: last_depth,
                vertices: batch,
                ..
            }) = self.items.last_mut()
            {
                if *last == texture && *last_sampler == sampler && *last_depth == depth {
                    batch.extend_from_slice(&vertices);
                    continue;
                }
            }
            self.items.push(OverlayItem::Triangles {
                material: OverlayMaterial::Image,
                texture,
                sampler,
                depth,
                vertices: vertices.to_vec(),
            });
        }
        for (space, r, _) in spaces {
            self.items.push(preview(space, r));
        }
        self.let_go_unused(present);
    }

    /// The picture `q` draws, made at the size `q` draws it, once; `None` until it is made (this
    /// frame's share, `spent` pixels so far, is used up) or when it cannot be.
    fn made<P: Presentation + ?Sized>(
        &mut self,
        present: &mut P,
        art: &Art,
        q: &Quad,
        spent: &mut u64,
    ) -> Option<Uploaded> {
        let id = q.tex?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a quad on whole pixels on the screen, a few thousand at most.
        let size = (q.dst.w as u32, q.dst.h as u32);
        if size.0 == 0 || size.1 == 0 || q.src.w <= 0.0 || q.src.h <= 0.0 {
            return None;
        }
        let from = MadeFrom {
            tex: id,
            src: [
                q.src.x.to_bits(),
                q.src.y.to_bits(),
                q.src.w.to_bits(),
                q.src.h.to_bits(),
            ],
            size,
        };
        let frame = self.frame;
        if let Some(made) = self.made.get_mut(&from) {
            made.used = frame;
            return made.texture.map(|t| (t, size));
        }
        let asked_before = self.asked.insert(from, frame) == Some(frame - 1);
        if !asked_before || *spent >= self.made_per_frame {
            return None;
        }
        self.asked.remove(&from);
        let image = art.image(id)?;
        let picture = crate::resample::resample(&image, q.src, size);
        *spent += u64::from(size.0) * u64::from(size.1);
        let key = OverlayTexture {
            space: OverlaySpace::Local,
            key: made_key(self.generation, self.made_count),
        };
        self.made_count += 1;
        let data = TextureData {
            width: picture.width,
            height: picture.height,
            format: TextureFormat::Bgra8,
            levels: vec![picture.bgra],
        };
        let texture = present.overlay_upload(key, &data).ok().map(|()| key);
        if texture.is_some() {
            self.stats.pictures_made += 1;
        } else {
            self.stats.upload_failures += 1;
        }
        self.made.insert(
            from,
            Made {
                texture,
                used: frame,
            },
        );
        texture.map(|t| (t, size))
    }

    /// Let go of the pictures made at their drawn size that no frame has drawn for a while.
    fn let_go_unused<P: Presentation + ?Sized>(&mut self, present: &mut P) {
        let frame = self.frame;
        self.asked.retain(|_, at| *at == frame);
        self.made.retain(|_, made| {
            let keep = frame.saturating_sub(made.used) <= MADE_KEPT;
            if !keep {
                if let Some(t) = made.texture {
                    let _ = present.overlay_release(t);
                }
            }
            keep
        });
    }

    /// How many pictures made at their drawn size are kept now.
    #[must_use]
    pub fn made_kept(&self) -> usize {
        self.made.values().filter(|m| m.texture.is_some()).count()
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
        for made in std::mem::take(&mut self.made).into_values() {
            if let Some(t) = made.texture {
                let _ = present.overlay_release(t);
            }
        }
        self.asked.clear();
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

/// The depth `q`'s vertices carry: its own, kept between the near and far planes so that one
/// standing nearer than the near plane is drawn over everything rather than cut away; the middle
/// of the range for one drawn over everything, which no depth is tested against.
fn vertex_depth(q: &Quad) -> f32 {
    q.depth.map_or(0.5, |d| d.clamp(0.0, 1.0))
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
    let z = vertex_depth(q);
    let v = |x: f32, y: f32, u: f32, v: f32| OverlayVertex {
        position: [x, y, z],
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
    let z = vertex_depth(q);
    let v = |p: (f32, f32), u: f32, v: f32| OverlayVertex {
        position: [p.0, p.1, z],
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
