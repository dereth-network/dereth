//! The kit's draw list, and the overlay it becomes over the finished world.
//!
//! An interface builds a list of [`Quad`]s each frame: a picture (or a flat colour), where it
//! goes on screen in pixels, which part of the picture, and a colour it is multiplied by. The
//! pictures are the [`crate::art::Art`] store's; [`Overlay`] uploads each through the
//! interface's [`TextureSink`] the first time a list names it, keeps it until the art lets it go
//! or is replaced, and turns the list into the shared overlay's triangle batches.
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

    /// Whether `dst` lies wholly outside the clip in force, where nothing of it would show.
    fn clipped_away(&self, dst: &Rect) -> bool {
        self.clips
            .last()
            .is_some_and(|c| dst.intersect(c).is_none())
    }

    /// Draw part `src` of picture `tex` at `dst`, multiplied by `colour`.
    pub fn image(&mut self, tex: TexId, src: Rect, dst: Rect, colour: Argb) {
        if dst.w <= 0.0 || dst.h <= 0.0 || colour >> 24 == 0 || self.clipped_away(&dst) {
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
        if dst.w <= 0.0 || dst.h <= 0.0 || colour >> 24 == 0 || self.clipped_away(&dst) {
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

/// Where the overlay's pictures go: the device an interface draws on, which keeps each picture
/// under a key once it is uploaded. An interface implements it over its presentation.
pub trait TextureSink {
    /// The back buffer's size in pixels.
    fn size(&self) -> (u32, u32);
    /// Upload `data` under `key`; whether the device took it.
    fn upload(&mut self, key: OverlayTexture, data: &TextureData) -> bool;
    /// Let the picture under `key` go.
    fn release(&mut self, key: OverlayTexture);
}

/// The key of the one white texel the flat fills sample, in the low half's last slot.
const WHITE_SLOT: u64 = 0xFFFF_FFFF;

/// Picture `slot`'s key in `generation`, in the key range `base` opens: each set of art (each
/// reading of the files) uploads under keys of its own, so nothing let go of with the old set can
/// take a picture of the new one with it.
fn key(base: u64, generation: u64, slot: u64) -> u64 {
    base | ((generation & 0xFF_FFFF) << 32) | slot
}

/// The key of the `n`th picture made at its drawn size in `generation`: in the key range just
/// above `base`'s, beside the art's own pictures.
fn made_key(base: u64, generation: u64, n: u64) -> u64 {
    key(base.wrapping_add(1 << 40), generation, n & 0xFFFF_FFFF)
}

/// How many pixels of pictures made at their drawn size one frame makes, at most: a change of
/// scale spreads the work over a few frames, and until a picture is made it is stretched on the
/// device.
pub const MADE_PER_FRAME: u64 = 150_000;

/// How many frames a picture made at its drawn size is kept unused before it is let go.
pub const MADE_KEPT: u64 = 600;

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
    /// The top bytes of every key this overlay uploads under: a range of its own, so its pictures
    /// never meet another interface's or another overlay's.
    base: u64,
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
    /// An overlay uploading under keys in the range `base` opens: `base` keeps only its top
    /// three bytes, and the rest of each key is the overlay's own.
    #[must_use]
    pub fn new(base: u64) -> Self {
        Self {
            base: base & 0xFFFF_FF00_0000_0000,
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

    /// Turn `list` into this frame's batches, uploading every picture it names that is not
    /// uploaded yet, and letting go of every picture the art let go. `under` is a preview space
    /// drawn under everything, over the whole screen; `spaces` are other preview spaces, each
    /// drawn into its rectangle over the first `n` quads of the list and under the rest (`0`
    /// puts it under the whole interface). Outside the frame bracket.
    pub fn compose<S: TextureSink + ?Sized>(
        &mut self,
        sink: &mut S,
        art: &Art,
        list: &DrawList,
        under: Option<PreviewSpace>,
        spaces: &[(PreviewSpace, Rect, usize)],
    ) {
        for id in art.take_forgotten() {
            if let Some(Some((key, _))) = self.textures.remove(&id) {
                sink.release(key);
            }
            self.made.retain(|from, made| {
                let keep = from.tex != id;
                if let (false, Some(t)) = (keep, made.texture) {
                    sink.release(t);
                }
                keep
            });
        }
        let size = sink.size();
        self.items.clear();
        if let Some(space) = under {
            self.items.push(OverlayItem::Preview {
                space,
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
                key: key(self.base, self.generation, WHITE_SLOT),
            };
            let white = TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![255, 255, 255, 255]],
            };
            self.white = sink.upload(key, &white).then_some(key);
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
                        .then(|| self.made(sink, art, &q, &mut spent))
                        .flatten();
                    match made {
                        Some(made) => {
                            q.src = Rect::new(0.0, 0.0, q.dst.w, q.dst.h);
                            made
                        }
                        None => match self.upload(sink, art, id) {
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
        self.let_go_unused(sink);
    }

    /// The picture `q` draws, made at the size `q` draws it, once; `None` until it is made (this
    /// frame's share, `spent` pixels so far, is used up) or when it cannot be.
    fn made<S: TextureSink + ?Sized>(
        &mut self,
        sink: &mut S,
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
            key: made_key(self.base, self.generation, self.made_count),
        };
        self.made_count += 1;
        let data = TextureData {
            width: picture.width,
            height: picture.height,
            format: TextureFormat::Bgra8,
            levels: vec![picture.bgra],
        };
        let texture = sink.upload(key, &data).then_some(key);
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
    fn let_go_unused<S: TextureSink + ?Sized>(&mut self, sink: &mut S) {
        let frame = self.frame;
        self.asked.retain(|_, at| *at == frame);
        self.made.retain(|_, made| {
            let keep = frame.saturating_sub(made.used) <= MADE_KEPT;
            if !keep {
                if let Some(t) = made.texture {
                    sink.release(t);
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
    fn upload<S: TextureSink + ?Sized>(
        &mut self,
        sink: &mut S,
        art: &Art,
        id: TexId,
    ) -> Option<Uploaded> {
        if let Some(hit) = self.textures.get(&id) {
            return *hit;
        }
        let key = OverlayTexture {
            space: OverlaySpace::Local,
            key: key(self.base, self.generation, u64::from(id)),
        };
        let uploaded = art.image(id).and_then(|image| {
            let data = TextureData {
                width: image.width,
                height: image.height,
                format: TextureFormat::Bgra8,
                levels: vec![image.bgra.clone()],
            };
            sink.upload(key, &data)
                .then_some((key, (image.width, image.height)))
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
    pub fn release<S: TextureSink + ?Sized>(&mut self, sink: &mut S) {
        for (key, _) in std::mem::take(&mut self.textures).into_values().flatten() {
            sink.release(key);
        }
        if let Some(white) = self.white.take() {
            sink.release(white);
        }
        for made in std::mem::take(&mut self.made).into_values() {
            if let Some(t) = made.texture {
                sink.release(t);
            }
        }
        self.asked.clear();
        self.generation += 1;
        self.items.clear();
    }

    /// This frame's batches, as [`Self::compose`] built them, for the interface to draw over the
    /// frame, in order, inside the frame bracket.
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
    //! Behaviour: none (this client's own widget kit)
    use super::*;

    /// Inside a clip, what lies wholly outside it is not kept, a picture or a fill; what is partly
    /// inside it is, and with no clip everything is.
    #[test]
    fn a_quad_wholly_outside_its_clip_is_not_kept() {
        let mut l = DrawList::default();
        let tex: TexId = 1;
        let src = Rect::new(0.0, 0.0, 1.0, 1.0);
        l.fill(Rect::new(500.0, 500.0, 10.0, 10.0), WHITE);
        l.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        l.fill(Rect::new(200.0, 0.0, 10.0, 10.0), WHITE);
        l.image(tex, src, Rect::new(0.0, 300.0, 10.0, 10.0), WHITE);
        l.fill(Rect::new(95.0, 95.0, 10.0, 10.0), WHITE);
        l.image(tex, src, Rect::new(-5.0, 50.0, 10.0, 10.0), WHITE);
        l.pop_clip();
        assert_eq!(
            l.quads.len(),
            3,
            "the one with no clip and the two partly inside"
        );
    }

    /// A sink that keeps what it was given.
    #[derive(Default)]
    struct Sink {
        live: std::collections::BTreeSet<OverlayTexture>,
    }

    impl TextureSink for Sink {
        fn size(&self) -> (u32, u32) {
            (64, 64)
        }
        fn upload(&mut self, key: OverlayTexture, _data: &TextureData) -> bool {
            self.live.insert(key);
            true
        }
        fn release(&mut self, key: OverlayTexture) {
            self.live.remove(&key);
        }
    }

    /// Two overlays in ranges of their own never upload under one key, and a picture the art
    /// lets go is let go by the overlay that uploaded it.
    #[test]
    fn overlays_keep_to_their_ranges_and_a_picture_let_go_is_released() {
        let art = Art::empty();
        let id = art.add_image(crate::art::Image::solid(4, 4, [1, 2, 3, 255]));
        let mut list = DrawList::default();
        list.image(
            id,
            Rect::new(0.0, 0.0, 4.0, 4.0),
            Rect::new(0.0, 0.0, 8.0, 8.0),
            WHITE,
        );
        let (mut a, mut b) = (Sink::default(), Sink::default());
        let mut one = Overlay::new(0x1100_0000_0000_0000);
        let mut two = Overlay::new(0x2200_0000_0000_0000);
        one.compose(&mut a, &art, &list, None, &[]);
        two.compose(&mut b, &art, &list, None, &[]);
        assert_eq!(a.live.len(), 2, "the picture and the white texel");
        assert!(a.live.is_disjoint(&b.live));
        art.forget_image(id);
        one.compose(&mut a, &art, &DrawList::default(), None, &[]);
        assert_eq!(a.live.len(), 1, "only the white texel is left");
    }

    /// Behaviour: none (the kit's overlay takes the space drawn under it; an API contract).
    /// The preview space the caller names is drawn first, over the whole screen, under every quad
    /// and every other space; naming none draws none.
    #[test]
    fn the_space_named_to_go_under_everything_is_drawn_first_over_the_whole_screen() {
        let art = Art::empty();
        let mut list = DrawList::default();
        list.fill(Rect::new(1.0, 1.0, 4.0, 4.0), WHITE);
        let mut sink = Sink::default();
        let mut overlay = Overlay::new(0x1100_0000_0000_0000);
        let other = (PreviewSpace::Examine, Rect::new(2.0, 2.0, 10.0, 10.0), 0);
        overlay.compose(&mut sink, &art, &list, Some(PreviewSpace::Tool), &[other]);
        let spaces: Vec<(PreviewSpace, Viewport)> = overlay
            .items()
            .iter()
            .filter_map(|i| match i {
                OverlayItem::Preview { space, rect } => Some((*space, *rect)),
                OverlayItem::Triangles { .. } => None,
            })
            .collect();
        assert!(matches!(
            overlay.items().first(),
            Some(OverlayItem::Preview {
                space: PreviewSpace::Tool,
                ..
            })
        ));
        assert_eq!(
            spaces[0].1,
            Viewport {
                x: 0,
                y: 0,
                width: 64,
                height: 64
            }
        );
        assert_eq!(spaces[1].0, PreviewSpace::Examine);
        overlay.compose(&mut sink, &art, &list, None, &[]);
        assert!(overlay
            .items()
            .iter()
            .all(|i| matches!(i, OverlayItem::Triangles { .. })));
    }
}
