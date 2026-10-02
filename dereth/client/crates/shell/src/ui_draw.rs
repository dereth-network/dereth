//! The UI render pass: one [`dereth_ui::UiDrawCmd`] becomes one textured quad.
//!
//! The UI pass preserves the blit modes, alpha-blend modifier, and the frame tail that draws the
//! 2D overlay, ends the scene, and presents. It also uses
//! `dereth_render::ui`, which owns every one of the four 2D pixel rules.
//!
//! **The half-pixel rule is the renderer's and is applied exactly once.** `ui::update_transform` emits
//! the client's own clip coordinates, including the `-1/W` term
//! (the pixel-to-clip transform's subtracted half pixel), and `ui::compensate_for_d3d12` undoes it
//! at the API boundary because a D3D12 rasteriser samples at the pixel centre where D3D9 sampled
//! at the corner. **Do not re-apply that compensation**. The single call is in
//! [`quad`] and there is no other.
//!
//! # Where the two halves run
//!
//! Uploading a texture runs a command list of its own (`Gpu::upload_texture` resets and closes the
//! frame list), so a UI texture created between `begin_frame` and `end_frame` would discard the
//! frame's recorded commands — the same constraint the world load, the object sync and the
//! landblock stream obey. So [`crate::gpu::Renderer::prepare_ui`] runs at step 7 of the frame,
//! and [`quad`] runs inside the frame's present step.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::DataId;
use dereth_render::font::{FontAtlas, GlyphSheet, TextBatch};
use dereth_render::ui::{self, ClipRect};
use dereth_ui::region::{IconRecipe, SurfaceOp};
use dereth_ui::text::PlacedGlyph;
use dereth_ui::{ImageSource, UiDrawCmd};

use dereth_client_contract::overlay::{OverlaySpace, OverlayTexture, OverlayVertex};

/// `D3DFVF_XYZ | D3DFVF_DIFFUSE | D3DFVF_TEX1` (0x142): float3 position, `D3DCOLOR` diffuse,
/// float2 uv — 24 bytes, the format emits its two triangles in.
pub const UI_VERTEX_BYTES: usize = 24;

/// The six vertices of one blit, in clip space, ready for `Gpu::draw_dynamic`.
///
/// `screen` and `clip` are inclusive UI-region boxes, so a box from `x0` to `x1`
/// covers `x1 - x0 + 1` pixels — `region.rs`'s "**Inclusive**, parent-relative. `w = x1 - x0 + 1`".
/// The visible rectangle is the intersection of the two, which the screen clip box produced and the
/// client set as the scissor before the blit.
///
/// Returns `None` when the intersection is empty, matching the `!clip.is_valid()` bail-out that
/// `UiSystem::draw` has already applied — this is
/// the second, arithmetic, form of the same test.
///
/// `image` is the **picture's own** pixel size — the uploaded texture's, which is what
/// the picture width/height getters return. The UVs are in the picture's texel units, not the
/// element's, because tiling never scales; see
/// [`dereth_render::ui::pixel_rules::graphic_draw_tiles`]. It is required rather than optional so
/// that a caller which does not know the picture cannot silently fall back to the element's own
/// extent.
/// One blit's vertices together with **the address mode its own UVs need**.
///
/// The two are returned together because they are computed from the same three numbers — the
/// clipped destination rectangle, the picture's extent and the tiling offset — and a caller that
/// recomputed the address mode from the *unclipped* element box would be answering a different
/// question from the one the vertices ask. The client's tiling blit has the same pairing: the
/// modulo that picks the start texel and the run length that decides whether a seam is crossed are
/// the same two lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiQuad {
    /// Six vertices, `UI_VERTEX_BYTES` each, ready for `Gpu::draw_dynamic`.
    pub vertices: Vec<u8>,
    /// Whether the `u` and `v` runs cross a seam of the picture, per
    /// [`dereth_render::ui::pixel_rules::graphic_draw_axis`]. `true` needs `TEXADDRESS_WRAP` on that
    /// axis; `false` is `TEXADDRESS_CLAMP`, and on that axis the UVs are inside `[0, 1]` by
    /// construction.
    pub wrap: (bool, bool),
}

#[must_use]
pub fn quad(cmd: &UiDrawCmd, fb: (u32, u32), image: (u32, u32)) -> Option<UiQuad> {
    let x0 = cmd.screen.x0.max(cmd.clip.x0);
    let y0 = cmd.screen.y0.max(cmd.clip.y0);
    let x1 = cmd.screen.x1.min(cmd.clip.x1);
    let y1 = cmd.screen.y1.min(cmd.clip.y1);
    if x1 < x0 || y1 < y0 {
        return None;
    }
    // Wholly off the back buffer: nothing to rasterise, and a negative origin would make
    // `update_transform`'s unsigned arguments meaningless.
    let fbw = i32::try_from(fb.0).unwrap_or(i32::MAX);
    let fbh = i32::try_from(fb.1).unwrap_or(i32::MAX);
    if x1 < 0 || y1 < 0 || x0 >= fbw || y0 >= fbh {
        return None;
    }
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (x1.min(fbw - 1), y1.min(fbh - 1));
    let (w, h) = (
        u32::try_from(x1 - x0 + 1).unwrap_or(0),
        u32::try_from(y1 - y0 + 1).unwrap_or(0),
    );
    if w == 0 || h == 0 {
        return None;
    }
    let (px, py) = (
        u32::try_from(x0).unwrap_or(0),
        u32::try_from(y0).unwrap_or(0),
    );

    // The renderer's four rules, then its D3D12 pixel-centre compensation, once.
    let rect = ui::update_transform(px, py, w, h, fb, fb);
    let rect = ui::compensate_for_d3d12(rect, fb);

    // The source rectangle inside the element's own picture, so a clipped element shows the part
    // of its picture that is still on screen rather than a squashed whole. The tiling offset
    // shifts the same way, which is why a tiled background scrolls under a fixed frame.
    //
    // **The divisor is the picture, not the element.** The picture is either
    // blitted once 1:1 into the element's own surface or repeated on a modulo grid. Region
    // blitting takes `min(src, dst)` on both extents, so the UI
    // region blit **never scales**. Retail's source texel for destination pixel `X` is
    // `(X - box.x0 + tiling_offset.x) mod graphic_width`; this is that expression with the
    // `mod` left to the sampler's `WRAP` address mode. Dividing by the element's width instead
    // would stretch a 10 x 5 border strip across a 792-pixel element where retail lays it down
    // 79.2 times.
    let (iw, ih) = image;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a texture extent, at most a few thousand, exact in f32.
    let (sw, sh) = (iw.max(1) as f32, ih.max(1) as f32);
    // **The start texel is reduced modulo the picture.** Each axis applies `%` and adds the extent
    // back when the remainder is negative. Under a `WRAP` sampler the reduction is a
    // no-op — `frac(u)` is unchanged by an integer number of copies; under `CLAMP` it is
    // load-bearing, because an unreduced `u0` of 1.0 would
    // put the *whole* run past the seam and smear the picture's last column across the element.
    // So the reduction and the per-axis address mode below have to land together.
    let (su, wrap_u) =
        ui::pixel_rules::graphic_draw_axis(iw, x0 - cmd.screen.x0 + cmd.tiling_offset.0, w);
    let (sv, wrap_v) =
        ui::pixel_rules::graphic_draw_axis(ih, y0 - cmd.screen.y0 + cmd.tiling_offset.1, h);
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a pixel offset inside a UI element, at most a few thousand, exact in f32. This is
    // 2D blit arithmetic, not engine arithmetic.
    let u0 = (su as f32) / sw;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let v0 = (sv as f32) / sh;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let u1 = u0 + (w as f32) / sw;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let v1 = v0 + (h as f32) / sh;

    Some(UiQuad {
        vertices: vertices(
            &rect,
            (u0, v0, u1, v1),
            cmd.alpha_blend_mod,
            cmd.rotation_z_degrees,
        ),
        wrap: (wrap_u, wrap_v),
    })
}

/// The six vertices themselves. Split out so the geometry can be asserted without a device.
///
/// `z_degrees` is, and is `0` for every element the shipped
/// client draws — [`dereth_render::ui::rotate_clip_quad`] documents why nothing can produce one,
/// and reproduces the unrotated corners bit for bit at `0`.
fn vertices(rect: &ClipRect, uv: (f32, f32, f32, f32), alpha: f32, z_degrees: i32) -> Vec<u8> {
    let [(lx_t, ly_t), (lx_b, ly_b), (rx_b, ry_b), (rx_t, ry_t)] =
        dereth_render::ui::rotate_clip_quad(*rect, z_degrees);
    let (u0, v0, u1, v1) = uv;
    // The alpha-blend modifier rides in the diffuse alpha, which is what the texture stage's
    // `D3DTA_DIFFUSE` alpha argument reads.
    let a = dereth_primitives::num::to_i32(alpha.clamp(0.0, 1.0) * 255.0).clamp(0, 255);
    #[allow(clippy::cast_sign_loss)]
    // LINT-OK: the byte field store that follows the clamped conversion above.
    let diffuse: u32 = 0x00FF_FFFF | ((a as u32) << 24);

    let mut out: Vec<u8> = Vec::with_capacity(6 * UI_VERTEX_BYTES);
    for (x, y, u, v) in [
        (lx_t, ly_t, u0, v0),
        (lx_b, ly_b, u0, v1),
        (rx_b, ry_b, u1, v1),
        (rx_b, ry_b, u1, v1),
        (rx_t, ry_t, u1, v0),
        (lx_t, ly_t, u0, v0),
    ] {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        // The UI is drawn with the depth test off; 0.5 is the same constant the first-pixel quad uses.
        out.extend_from_slice(&0.5f32.to_le_bytes());
        out.extend_from_slice(&diffuse.to_le_bytes());
        out.extend_from_slice(&u.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// The six vertices of one flat-colour fill, clipped to the command's visible rectangle.
///
/// A fill writes into the element's own surface, so it is bounded by
/// the same rectangle the element's quad would be — `screen ∩ clip` — and by the back buffer.
/// Returns `None` when the fill is entirely outside that, or when it cannot change a pixel
/// ([`dereth_ui::UiFill::is_invisible`]).
///
/// **The null colour is the whole reason for the second test.**
/// Every element-owned surface carries alpha, so the generated draw material modulates by the
/// surface's own alpha. Filling with the null colour clears the page to
/// fully transparent and, blended `SRCALPHA`/`INVSRCALPHA` over the frame, changes nothing. This
/// rebuild has no CPU page to clear, so the step is *modelled and then measured to be free*
/// rather than dropped.
#[must_use]
pub fn fill_quad(cmd: &UiDrawCmd, f: &dereth_ui::UiFill, fb: (u32, u32)) -> Option<Vec<u8>> {
    if f.is_invisible() {
        return None;
    }
    // The element's own visible rectangle, then the fill inside it.
    let vx0 = cmd.screen.x0.max(cmd.clip.x0).max(0);
    let vy0 = cmd.screen.y0.max(cmd.clip.y0).max(0);
    let fbw = i32::try_from(fb.0).unwrap_or(i32::MAX);
    let fbh = i32::try_from(fb.1).unwrap_or(i32::MAX);
    let vx1 = cmd.screen.x1.min(cmd.clip.x1).min(fbw - 1);
    let vy1 = cmd.screen.y1.min(cmd.clip.y1).min(fbh - 1);
    let x0 = f.x.max(vx0);
    let y0 = f.y.max(vy0);
    let x1 = (f.x + f.w - 1).min(vx1);
    let y1 = (f.y + f.h - 1).min(vy1);
    if x1 < x0 || y1 < y0 {
        return None;
    }
    let (w, h) = (
        u32::try_from(x1 - x0 + 1).unwrap_or(0),
        u32::try_from(y1 - y0 + 1).unwrap_or(0),
    );
    if w == 0 || h == 0 {
        return None;
    }
    // **A fill does not go through the four blit rules, and must not.** Those rules land one
    // surface texel on one pixel: the half-pixel offset and quarter-pixel size inset are calibrated
    // against a texture the same size as the element. A fill writes pixels into a CPU surface and
    // has no texture at all, so the geometry here is the pixel rectangle itself, edge to edge. On a 1x1
    // blip the difference is the whole thing: a `2 * (1 - 0.25) / width` quad is 0.75 px wide and
    // rasterises to nothing.
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: back-buffer pixel edges, at most a few thousand, exact in f32.
    let (fw, fh) = (fb.0 as f32, fb.1 as f32);
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let (l, r) = (x0 as f32, (x1 + 1) as f32);
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let (tp, bt) = (y0 as f32, (y1 + 1) as f32);
    let rect = ClipRect {
        x: 2.0 * l / fw - 1.0,
        y: 1.0 - 2.0 * bt / fh,
        sx: 2.0 * (r - l) / fw,
        sy: 2.0 * (bt - tp) / fh,
    };
    // UVs are all zero: the fill samples one texel of the 1x1 white texture the renderer binds, so
    // `TEXOP_MODULATE(TEXTURE, DIFFUSE)` hands back the diffuse colour untouched.
    Some(flat_vertices(&rect, f.color))
}

/// The six vertices of one **selection** rectangle, the second and last of the client's
/// window-surface pixel primitives.
///
/// Geometrically this is [`fill_quad`] and nothing else: a pixel rectangle, edge to edge, clipped
/// to `screen ∩ clip` and to the back buffer, with no texture of its own. It differs only in the
/// **blend** the caller binds — `INVDESTCOLOR` / `ZERO` over a white source, which is
/// `dest' = 1 - dest` and therefore `~*p & (r|g|b)` per channel.
///
/// The colour is white because the source term must be 1 in every channel for the blend to be a
/// pure inversion; it carries no information from the element.
///
/// The client's inversion leaves the destination **alpha** untouched (it ORs the destination's
/// alpha bits back in) and this does not, because the pair maps to `INV_DEST_ALPHA`/`ZERO` for the alpha channel. It cannot matter
/// here: every UI pipeline is created with `COLOR_WRITE_ENABLE_RGB`, so the back buffer's alpha is
/// never written by any of them, and `CapturedImage::to_rgba` forces it opaque for the same
/// reason.
#[must_use]
pub fn invert_quad(
    cmd: &UiDrawCmd,
    r: dereth_ui::region::Box2D,
    fb: (u32, u32),
) -> Option<Vec<u8>> {
    if !r.is_valid() {
        return None;
    }
    let f = dereth_ui::UiFill {
        x: r.x0,
        y: r.y0,
        w: r.x1 - r.x0 + 1,
        h: r.y1 - r.y0 + 1,
        color: 0xFFFF_FFFF,
    };
    fill_quad(cmd, &f, fb)
}

/// [`fill_quad`]'s geometry, with an explicit `D3DCOLOR`. Split out so it can be asserted with no
/// device, exactly as [`vertices`] is.
fn flat_vertices(rect: &ClipRect, color: u32) -> Vec<u8> {
    let (l, r, b, t) = (rect.x, rect.right(), rect.y, rect.top());
    let mut out: Vec<u8> = Vec::with_capacity(6 * UI_VERTEX_BYTES);
    for (x, y) in [(l, t), (l, b), (r, b), (r, b), (r, t), (l, t)] {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        out.extend_from_slice(&0.5f32.to_le_bytes());
        out.extend_from_slice(&color.to_le_bytes());
        out.extend_from_slice(&0.0f32.to_le_bytes());
        out.extend_from_slice(&0.0f32.to_le_bytes());
    }
    out
}

/// One UI image as the renderer caches it: the id, the operation it is shown through, and which
/// files it is read from.
pub type ImageKey = (DataId, Option<SurfaceOp>, ImageSource);

/// Which images the current draw list needs, once each and in first-use order.
///
/// An image shown through a [`SurfaceOp`] is a **different** texture from the same image shown
/// plain -- the colour-spot generator makes a local surface per spot and recolours it -- so the
/// pair is the key. So is the same id read from the world's files rather than the interface's:
/// beside an older world the two answer one id with different pictures ([`ImageSource`]).
#[must_use]
pub fn images(cmds: &[UiDrawCmd]) -> Vec<ImageKey> {
    let mut out: Vec<ImageKey> = Vec::new();
    for c in cmds {
        if let Some(id) = c.image {
            let k = (id, c.image_op, c.image_source);
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}

/// The descriptor-cache key for an image shown through an operation.
///
/// The client keys a derived texture on the pair that produced it;
/// a UI image has no palette, so the palette half of the key is free and the operation goes there.
/// **Zero** -- no operation -- keeps a plain image on the plain image's key, so a plain image is
/// never re-uploaded because an operation exists.
///
/// **The result is in [`dereth_render::TextureSpace::Ui`], and that is load-bearing.**
/// The operation hash is a full 32-bit word occupying the half the world path puts a *palette
/// DataID* in, and `Multiply`'s hash (`c.rotate_left(3) | 1`) is a surjection onto the odd words,
/// so it can reproduce any odd palette DID exactly. A plain image is worse still: it needs no
/// arithmetic coincidence at all, because `(0, image id)` is the same shape as an unpalettised
/// world texture's `(0, RenderSurface id)` and both ids are `0x06` surface ids out of the same dat.
/// The original client can share one table because every cache entry identifies a surface and a
/// palette; this build's UI decode ([`derive`](fn@derive) / [`composite`] over
/// `TextureStore::texture_data`) is a **different function** from its world decode
/// (`texture_data_shifted`, with a clip-map flag and a shift palette), so agreement on a key here is
/// not agreement on the pixels. See `dereth_render::descriptor::TextureSpace`.
#[must_use]
pub fn image_key(id: DataId, op: Option<SurfaceOp>) -> dereth_render::TextureKey {
    let half = match op {
        None => 0,
        // The low bit is forced so that no operation can collide with "no operation".
        Some(SurfaceOp::ReplaceColor { from, to }) => (from ^ to.rotate_left(1)) | 1,
        Some(SurfaceOp::Multiply(c)) => c.rotate_left(3) | 1,
        // Full-strength Colorize ignores the input alpha; all 24 hue/saturation RGB
        // bits remain in this key. Target sprites are distinct DAT images from icons.
        Some(SurfaceOp::Colorize(c)) => 0x8000_0001 | ((c & 0x00FF_FFFF) << 1),
        // **Every** surface of the recipe goes into the key, because every one of them changes the
        // pixels. The client's per-object icon recipe is keyed by object id, and its icon update
        // rebuilds the recipe when any of the five fields moves; a key that left
        // one out would hand a stale composite to the object that changed it.
        Some(SurfaceOp::Icon(r)) => {
            let (tag, a, b, c, d, e) = match r {
                IconRecipe::Object {
                    background,
                    effects,
                    icon,
                    overlay,
                    underlay,
                } => (
                    0x1CD0_0000_u32,
                    background,
                    effects,
                    icon,
                    overlay,
                    underlay,
                ),
                IconRecipe::Spell {
                    background,
                    icon,
                    tint,
                    overlay,
                } => (0x5BE1_0000_u32, background, icon, tint, overlay, None),
            };
            let raw = |v: Option<DataId>| v.map_or(0, |x| x.0);
            let mut h = u64::from(tag);
            for v in [a, b, c, d, e] {
                h = h.rotate_left(11) ^ u64::from(raw(v));
            }
            #[allow(clippy::cast_possible_truncation)]
            {
                ((h ^ (h >> 32)) as u32) | 1
            }
        }
    };
    dereth_render::TextureKey::ui(dereth_render::combined_texture_key(half, id.0))
}

/// [`image_key`] for an image read from `source`'s files. An interface image keeps its key. A world
/// image's key sets bit 1 of the operation half: a plain world image has half `2`, which no
/// operation produces (every operation's half is odd), so it never shares a texture with the
/// interface's picture under the same id. A composed icon is always read from the world's files,
/// so its key is its recipe's alone.
#[must_use]
pub fn image_key_from(
    id: DataId,
    op: Option<SurfaceOp>,
    source: ImageSource,
) -> dereth_render::TextureKey {
    let key = image_key(id, op);
    if source == ImageSource::Interface || matches!(op, Some(SurfaceOp::Icon(_))) {
        return key;
    }
    dereth_render::TextureKey::ui(key.raw() ^ (2u64 << 32))
}

/// A UI image, its operation and its files as an overlay texture: [`image_key_from`]'s payload,
/// in the overlay's shared image space.
#[must_use]
pub fn image_texture(id: DataId, op: Option<SurfaceOp>, source: ImageSource) -> OverlayTexture {
    OverlayTexture {
        space: OverlaySpace::Image,
        key: image_key_from(id, op, source).raw(),
    }
}

/// A font's glyph sheet (`pass` 0) or outline sheet (`pass` 1) as an overlay texture: the same
/// payload `dereth_render::TextureKey::font` carries.
#[must_use]
pub fn glyph_texture(pass: u32, font: DataId) -> OverlayTexture {
    OverlayTexture {
        space: OverlaySpace::Glyphs,
        key: dereth_render::TextureKey::font(pass, font.0).raw(),
    }
}

/// The one white texel the flat fills and the selection inverts sample: the UI's own, shared with
/// nothing.
pub const WHITE_TEXEL: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: u64::MAX,
};

/// The intro movie's current frame for image `id`: the UI's own, replaced every frame.
#[must_use]
pub fn movie_texture(id: DataId) -> OverlayTexture {
    OverlayTexture {
        space: OverlaySpace::Local,
        key: u64::from(id.0),
    }
}

/// [`UI_VERTEX_BYTES`]-byte vertex records as the overlay's vertices: the same position, colour and
/// texture coordinate, read back bit for bit.
#[must_use]
pub fn overlay_vertices(bytes: &[u8]) -> Vec<OverlayVertex> {
    let f = |b: &[u8], at: usize| f32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    bytes
        .as_chunks::<UI_VERTEX_BYTES>()
        .0
        .iter()
        .map(|v| OverlayVertex {
            position: [f(v, 0), f(v, 4), f(v, 8)],
            color: u32::from_le_bytes([v[12], v[13], v[14], v[15]]),
            uv: [f(v, 16), f(v, 20)],
        })
        .collect()
}

/// Compose the drag and base icon surfaces against decoded dat surfaces rather than a locked
/// render surface.
///
/// The client makes a 32 × 32 `A8R8G8B8` local surface, blits into it, and hands the result to its
/// texture cache. This does the same on the CPU and hands the result to the texture
/// uploader, which is where every other runtime-generated UI surface in this build already goes
/// ([`derive`](fn@derive)).
///
/// # What is faithful here and what is not
///
/// * The **order** and the **modes** are the client's, step for step, and they are
///   the part that a reader can get wrong in a way that still looks plausible: an overlay under the
///   base icon, or the background tile above it, draws a picture that is merely different rather
///   than obviously broken.
/// * The **blend arithmetic** is the client's three-channel and four-channel alpha blits — see
///   [`SurfaceOp::blit_3alpha`] and [`SurfaceOp::blit_4alpha`].
/// * The **extent** is the client's 32 × 32 surface. A source larger than that is
///   clipped, a smaller one covers its own corner and leaves the rest, which is
///   the colouring blit taking the smaller of the two extents on each axis.
/// * **Not faithful:** a source that does not decode is skipped rather than aborting the whole
///   composite. The client's icon renderer guards every blit with a null asset check and does the same.
///
/// Returns `None` when nothing in the recipe decoded, which the caller counts as a decode failure
/// exactly as it counts a missing plain image.
#[must_use]
pub fn composite(
    recipe: IconRecipe,
    fetch: &dyn Fn(DataId) -> Option<dereth_primitives::TextureData>,
) -> Option<dereth_primitives::TextureData> {
    let n = SurfaceOp::ICON_EXTENT;
    let mut got_any = false;
    // A freshly created local surface is not cleared by the client either; every recipe whose first
    // blit is present overwrites all of it with a plain blit, and one whose first blit is absent
    // is a picture the client would leave uninitialised. Transparent black is the honest stand-in
    // and it is what the alpha-blended layers above expect.
    let mut main = vec![0u32; (n * n) as usize];

    let load = |id: Option<DataId>| -> Option<Vec<u32>> {
        let d = fetch(id?)?;
        if d.format != dereth_primitives::TextureFormat::Bgra8 {
            return None;
        }
        let src = d.levels.first()?;
        let mut out = vec![0u32; (n * n) as usize];
        for y in 0..n.min(d.height) {
            for x in 0..n.min(d.width) {
                let at = ((y * d.width + x) * 4) as usize;
                let px = src.get(at..at + 4)?;
                out[(y * n + x) as usize] = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
            }
        }
        Some(out)
    };

    // `(dst, src, mode, 1.0)` over the whole 32x32.
    fn blit(dst: &mut [u32], src: &[u32], f: fn(u32, u32) -> u32) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            *d = f(*d, *s);
        }
    }
    // `(dst, &white, src)` — the *same texel* of the source,
    // not a constant colour, wherever the destination is exactly opaque white.
    fn replace_from(dst: &mut [u32], src: &[u32]) {
        for (d, s) in dst.iter_mut().zip(src.iter()) {
            if *d == SurfaceOp::OPAQUE_WHITE {
                *d = *s;
            }
        }
    }

    match recipe {
        IconRecipe::Object {
            background,
            effects,
            icon,
            overlay,
            underlay,
        } => {
            // The drag-icon pass uses `UIEffectIcons`, not the custom underlay.
            // ---- drag icon: icon, custom overlay, effects by colour replacement --------------
            let mut drag = vec![0u32; (n * n) as usize];
            if let Some(p) = load(icon) {
                got_any = true;
                blit(&mut drag, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(overlay) {
                got_any = true;
                blit(&mut drag, &p, SurfaceOp::blit_4alpha);
            }
            if let Some(p) = load(effects) {
                got_any = true;
                replace_from(&mut drag, &p);
            }
            // The based-icon pass uses the custom underlay, not `UIEffectIcons`.
            // ---- icon: type tile, custom underlay, then the drag surface --------------------
            let mut based = false;
            if let Some(p) = load(background) {
                got_any = true;
                based = true;
                blit(&mut main, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(underlay) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_3alpha);
            }
            // **One deliberate divergence, and it is in a case the client cannot reach.**
            // The three-channel alpha blit preserves the *destination's* alpha byte, so blitting the drag surface
            // onto a surface nothing has written leaves alpha 0 — an invisible icon. The client
            // blits a `0x10000004` row into the icon surface first every single time (all 34 rows of the
            // mapper resolve, which the backpack grid tests assert), so it never meets this; a
            // rebuild whose mapper lookup failed for one type would silently blank that cell
            // instead of drawing the icon. A plain blit for the un-based case keeps the failure visible as "no tile" rather than "no icon".
            blit(
                &mut main,
                &drag,
                if based {
                    SurfaceOp::blit_3alpha
                } else {
                    SurfaceOp::blit_normal
                },
            );
        }
        IconRecipe::Spell {
            background,
            icon,
            tint,
            overlay,
        } => {
            if let Some(p) = load(background) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_normal);
            }
            if let Some(p) = load(icon) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_4alpha);
            }
            if let Some(p) = load(tint) {
                got_any = true;
                replace_from(&mut main, &p);
            }
            if let Some(p) = load(overlay) {
                got_any = true;
                blit(&mut main, &p, SurfaceOp::blit_4alpha);
            }
        }
    }
    if !got_any {
        return None;
    }
    let mut bytes = Vec::with_capacity(main.len() * 4);
    for t in &main {
        bytes.extend_from_slice(&t.to_le_bytes());
    }
    Some(dereth_primitives::TextureData {
        width: n,
        height: n,
        format: dereth_primitives::TextureFormat::Bgra8,
        levels: vec![bytes],
    })
}

/// One dat image with its operation applied, texel by texel.
///
/// The client blits the template into a locked `RenderSurface` and walks it; this walks the decoded
/// BGRA8 mip chain, which is the same four bytes per texel in the same order. A format this
/// rebuild did not decode to BGRA8 is returned untouched rather than corrupted.
#[must_use]
pub fn derive(
    mut data: dereth_primitives::TextureData,
    op: SurfaceOp,
) -> dereth_primitives::TextureData {
    if data.format != dereth_primitives::TextureFormat::Bgra8 {
        return data;
    }
    for level in &mut data.levels {
        for texel in level.as_chunks_mut::<4>().0 {
            *texel = op.apply(u32::from_le_bytes(*texel)).to_le_bytes();
        }
    }
    data
}

/// How the UI textures resolved.
///
/// Every field exists because the corresponding failure is **tolerated** — a UI element whose image
/// will not decode is drawn as nothing, exactly as the client treats a missing image texture — and
/// the project's standing rule is that a tolerated failure gets a counter
/// and the counter gets asserted on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiTextureStats {
    /// Distinct images uploaded to the GPU. Each consumes one SRV slot until its screen releases
    /// it.
    pub uploaded: u64,
    /// Images whose id chain did not resolve or whose payload did not decode.
    pub decode_failures: u64,
    /// Draw commands skipped because their image never resolved.
    pub skipped_draws: u64,
    /// Draw commands that produced no geometry (clipped entirely away).
    pub clipped_away: u64,
    /// Quads actually submitted.
    pub quads_drawn: u64,
    /// Image blits drawn at the source **surface's** own pixel size and unrotated, which the
    /// transform update binds **POINT** for.
    ///
    /// The source surface is the element's own box (the surface the element's UI object is created
    /// with, over its width and height), not the picture, so an element larger than its
    /// picture is **not** scaled — it is tiled, and it is counted here and in
    /// [`Self::blits_tiled`] at once.
    pub blits_unscaled: u64,
    /// Image blits whose destination rectangle differs from the source **surface's** pixel size,
    /// or which are rotated, which the transform update binds **LINEAR** for.
    ///
    /// **Nothing in this build or in retail produces one** at a fixed display size — see
    /// `ui_surface_sampler`'s caveat for the one uncertain route out. The surface is created and
    /// resized at the element's own size, and the stretch path has no caller in the reference
    /// client, so a non-zero reading here means a *new* producer appeared, which is exactly what
    /// a counter on a dead arm is for. A tile misread as a stretch would also land here.
    pub blits_scaled: u64,
    /// Image blits whose picture does not cover the element's box, or which carry a tiling
    /// offset — retail's modulo-grid arm, which this build draws
    /// with a **WRAP** sampler instead of a CPU repeat. Overlaps [`Self::blits_unscaled`]: the
    /// address mode and the filter are independent bits of the sampler index.
    pub blits_tiled: u64,
    /// Fonts whose glyph texture was uploaded. That texture is the font's own foreground sheet
    /// rather than a 256x256 bake, so its size varies with the font; it is one texture and one SRV slot for the life of the process.
    pub fonts_baked: u64,
    /// Fonts whose object or glyph sheet would not read or decode.
    pub font_failures: u64,
    /// Fonts whose **outline** (background) glyph texture was uploaded. 37 of the 49
    /// shipped fonts have a background sheet; the other 12 need no second texture because their
    /// outline arm draws the foreground sheet eight times instead.
    pub font_outlines_baked: u64,
    /// Outline draw calls that took the **`0x7000`** arm: the font has a background
    /// sheet, so one dilated quad per glyph out of that second texture.
    pub outline_draws_sheet: u64,
    /// Outline draw calls that took the **`0x9000`** arm: the font has no background
    /// sheet, so eight quads per glyph out of the *foreground* texture over
    /// [`dereth_render::font::OUTLINE_NEIGHBOURHOOD`].
    ///
    /// The two are counted apart because "the outline is drawn" and "the right arm was chosen"
    /// are different claims, and the shipped char-gen screen exercises **both**.
    pub outline_draws_neighbourhood: u64,
    /// Outline **draw calls** — one per font run of an element carrying attribute
    /// `0x21`, submitted *before* that run's foreground draw. Zero on a screen with no outlined
    /// text, which is most of them; the denominator for "the outline pass actually ran".
    pub outline_draws: u64,
    /// Outline quads submitted. For the `0x7000` arm this equals the run's glyph
    /// count; for the `0x9000` arm it is up to **eight times** it, which is what makes the two
    /// arms distinguishable from the counters alone.
    pub outline_glyphs_drawn: u64,
    /// Glyphs whose outline produced no geometry — clipped away, zero-width, or the
    /// element asked for an outline its font could not supply. Not a failure count on its own;
    /// [`Self::font_failures`] is.
    pub outline_glyphs_skipped: u64,
    /// Glyph quads actually submitted.
    pub glyphs_drawn: u64,
    /// Glyph **draw calls** -- one per font run, where [`Self::glyphs_drawn`] counts
    /// the quads inside them. It is the denominator for `Gpu::sampler_binds()[UI_GLYPH_SAMPLER]`:
    /// without it, "some draw bound POINT" and "every glyph draw bound POINT" are the same
    /// reading, and only the second is the claim.
    pub glyph_draws: u64,
    /// Glyphs that produced no geometry. Three causes, all of them ordinary: the font had no
    /// glyph texture, the glyph is zero-width (every space is), or it fell entirely outside its
    /// element's visible box (a scrolled-off list row). It is therefore **not** a failure count;
    /// [`Self::font_failures`] is. A font that will not rasterise shows up in both.
    pub glyphs_skipped: u64,
    /// Flat-colour rectangles submitted.
    pub fills_drawn: u64,
    /// Fills that could change no pixel and were therefore not submitted. Every null-color erase
    /// lands here: the null colour has zero alpha, so under
    /// `SRCALPHA`/`INVSRCALPHA` the destination is untouched. Counted rather than dropped
    /// silently, because "the erase is free" is a claim that has to be measurable.
    pub fills_invisible: u64,
    /// Fills clipped entirely outside their element's visible rectangle.
    pub fills_clipped: u64,
    /// Selection rectangles submitted, one per selected glyph. Zero on every screen with nothing selected, which is most frames, so it
    /// is the denominator for "the selection was actually drawn" rather than a load figure.
    pub inverts_drawn: u64,
    /// Selection rectangles clipped entirely outside their element's visible
    /// rectangle -- a selection scrolled out of a box. Counted apart from
    /// [`Self::inverts_drawn`] so "nothing was selected" and "the highlight fell off the element"
    /// are two readings and not one.
    pub inverts_clipped: u64,
    /// Preview passes actually run this frame: each is a creature-mode preview space drawn inside
    /// its viewport rectangle. Zero with a preview on screen is a black panel where a model
    /// should be, so it is counted rather than assumed.
    pub previews_drawn: u64,
    /// Preview passes declined because object index 0 was absent. A space whose setup would not
    /// load lands here,
    /// and the panel is left exactly as the UI blitted it.
    pub previews_empty: u64,
}

// ---------------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------------
//
// The text path preserves the font dat object, the texture-based font atlas, all eight
// pixel-positioning rules, and resolution of `StringInfo` through the string tables when set.
//
// `UiDrawCmd` carries no colour and no glyphs; this module joins `dereth_ui::text` and
// `dereth_render::font`. It owns no policy: layout is [`dereth_ui::text`], the atlas and the quad
// rules are [`dereth_render::font`], and the dat decoders are `dereth_assets`'.
//
// **The GPU path, not the CPU one.** Fonts can use a baked 256x256 atlas or blit the source sheet
// directly into an element-owned surface. The client uses source sheets for UI text and the baked
// atlas for the debug overlay. This rebuild keeps that distinction: [`FontAtlas::from_glyph_sheets`]
// retains the source-sheet coordinates used here, while [`FontAtlas::build`] remains the baked
// debug-font path. Both place each glyph at the pen plus its two bearings.

/// Anything that stops a font from loading. Every one is tolerated — text simply does not appear —
/// so every one is counted in [`UiTextureStats`].
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("font {0:?}: {1}")]
    Dat(DataId, String),
    #[error("font {0:?} glyph sheet {1:?}: {2}")]
    Sheet(DataId, DataId, String),
    #[error("font {0:?}: {1}")]
    Atlas(DataId, dereth_render::RenderError),
}

/// Read a `Font` (`0x40xxxxxx`) out of the dat as the renderer's metrics type.
///
/// **`dereth_assets::ui::FontChar` types the three bearings `u8`.** The reference layout
/// treats them as signed, and negative values do occur
/// in the shipped fonts; reading one unsigned would put its glyph 255 pixels to the right. The
/// reinterpretation is here, in the one line that joins the two crates, rather than in
/// `dereth_assets`.
///
/// # Errors
/// [`FontError::Dat`] when the object is absent or will not decode.
pub fn load_font(
    store: &dereth_dat::RetailDatStore,
    did: DataId,
) -> Result<dereth_render::font::Font, FontError> {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource;
    let bytes = store
        .read(did)
        .map_err(|e| FontError::Dat(did, e.to_string()))?;
    let f = dereth_assets::ui::Font::decode_payload(did, &bytes)
        .map_err(|e| FontError::Dat(did, e.to_string()))?;
    let mut char_descs: Vec<dereth_render::font::FontCharDesc> = f
        .chars
        .iter()
        .map(|c| dereth_render::font::FontCharDesc {
            unicode: c.unicode,
            offset_x: c.offset_x,
            offset_y: c.offset_y,
            width: c.width,
            height: c.height,
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_before: c.h_offset_before as i8,
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_after: c.h_offset_after as i8,
            #[allow(clippy::cast_possible_wrap)]
            vertical_offset_before: c.v_offset_before as i8,
        })
        .collect();
    // Glyph lookup is a dense map in the client and a binary search in the renderer's `Font`, so the
    // records must be sorted; they are in every retail font, and sorting
    // makes that true rather than assumed.
    char_descs.sort_unstable_by_key(|d| d.unicode);
    Ok(dereth_render::font::Font {
        max_char_height: f.max_char_height,
        max_char_width: f.max_char_width,
        char_descs,
        num_horizontal_border_pixels: f.num_horizontal_border_pixels,
        num_vertical_border_pixels: f.num_vertical_border_pixels,
        #[allow(clippy::cast_possible_wrap)]
        baseline_offset: f.baseline_offset as i32,
        foreground_surface_data_id: f.foreground_surface.0,
        background_surface_data_id: f.background_surface.0,
    })
}

/// Load the font and take its **foreground** glyph sheet as the glyph texture, whole.
///
/// **This deliberately does not bake a texture-based font atlas.** Atlas construction bakes
/// printable ASCII into a 256x256 atlas, and the font-texture setup returns false when the
/// range will not fit. Three shipped fonts do not fit -- `0x40000013`, `0x40000014` and
/// `0x40000024`, whose maximum character heights are 46, 48 and 42 -- and the char-gen layouts
/// name two of them: `0x40000024` is the FINISH button's caption and `0x40000014` is the town
/// page's title. The 256x256 edge and that `return false` are both faithful, and
/// [`dereth_render::font::FontAtlas::build`] implements them for the debug overlay. They do not
/// apply here, because **the client's UI text does not use the atlas at all**:
/// it blits glyphs from the font's source surfaces straight into a UI surface. The baked atlas is
/// used for the small debug font. The source here is the sheet, addressed by the glyph's own source
/// rectangle, and no font can overflow anything.
///
/// **The background (outline) sheet is loaded too** — it is the second glyph texture the
/// glyph blit reads from when its flags carry `0x4000`, which is the `0x7000` outline pass of the
/// text element's two-pass glyph loop. The two sheets are not *composited* into one atlas at
/// all, they are two textures drawn in two passes, the outline first in the current outline colour and the foreground
/// second in the glyph's own colour.
///
/// 37 of the 49 shipped fonts have one; the other 12 take the eight-draw
/// [`dereth_render::font::OUTLINE_NEIGHBOURHOOD`] arm instead and need no second texture.
///
/// # Errors
/// [`FontError`] for a font that will not read or a glyph sheet that will not decode. A font whose
/// **outline** sheet will not decode is *not* an error: the outline is dropped and the foreground
/// still draws, which is the same tolerance every other UI texture failure gets.
pub fn build_font_atlas(
    store: &dereth_dat::RetailDatStore,
    did: DataId,
) -> Result<FontAtlas, FontError> {
    let font = load_font(store, did)?;
    let sheet_id = DataId(font.foreground_surface_data_id);
    let textures = crate::textures::TextureStore::new(store);
    let sheet = textures
        .bgra8(sheet_id)
        .map_err(|e| FontError::Sheet(did, sheet_id, e.to_string()))?;
    let flat: Vec<u8> = sheet.pixels.iter().flat_map(|p| *p).collect();
    // The outline sheet, when the font has one. `background_surface_data_id` is 0 for the 12
    // fonts that do not.
    let outline_flat: Option<Vec<u8>> = (font.background_surface_data_id != 0)
        .then_some(DataId(font.background_surface_data_id))
        .and_then(|id| match textures.bgra8(id) {
            Ok(s) if s.width == sheet.width && s.height == sheet.height => {
                Some(s.pixels.iter().flat_map(|p| *p).collect())
            }
            Ok(s) => {
                tracing::warn!(
                    "font {did:?} outline sheet {id:?} is {}x{} where the foreground \
                     sheet is {}x{}; the outline pass is dropped for this font",
                    s.width,
                    s.height,
                    sheet.width,
                    sheet.height
                );
                None
            }
            Err(e) => {
                tracing::warn!("font {did:?} outline sheet {id:?} would not decode: {e}");
                None
            }
        });
    FontAtlas::from_glyph_sheets(
        &font,
        GlyphSheet {
            width: sheet.width,
            height: sheet.height,
            bgra: &flat,
        },
        outline_flat.as_ref().map(|o| GlyphSheet {
            width: sheet.width,
            height: sheet.height,
            bgra: o,
        }),
    )
    .map_err(|e| FontError::Atlas(did, e))
}

/// Font metrics including the baseline and maximum character height, as `dereth_ui`'s layout seam.
#[derive(Debug)]
pub struct DatFontMetrics(dereth_render::font::Font);

impl dereth_ui::text::FontMetrics for DatFontMetrics {
    fn advance(&self, ch: u16) -> i32 {
        self.0.get_char_width_a(ch)
    }
    fn height(&self) -> i32 {
        i32::try_from(self.0.max_char_height).unwrap_or(0)
    }
}

/// The font mapper — against the retail dats, memoised.
///
/// A font that will not load is remembered as absent, so a broken id costs one failed read rather
/// than one per element.
#[derive(Debug)]
pub struct DatFontProvider {
    store: Arc<dereth_dat::RetailDatStore>,
    cache: RefCell<BTreeMap<DataId, Option<Arc<dyn dereth_ui::text::FontMetrics>>>>,
}

impl DatFontProvider {
    #[must_use]
    pub fn new(store: Arc<dereth_dat::RetailDatStore>) -> Self {
        Self {
            store,
            cache: RefCell::new(BTreeMap::new()),
        }
    }
}

impl dereth_ui::text::FontProvider for DatFontProvider {
    fn metrics(&self, did: DataId) -> Option<Arc<dyn dereth_ui::text::FontMetrics>> {
        if let Some(hit) = self.cache.borrow().get(&did) {
            return hit.clone();
        }
        let v: Option<Arc<dyn dereth_ui::text::FontMetrics>> = match load_font(&self.store, did) {
            Ok(f) => Some(Arc::new(DatFontMetrics(f))),
            Err(e) => {
                tracing::warn!("{e}");
                None
            }
        };
        self.cache.borrow_mut().insert(did, v.clone());
        v
    }
}

/// `StringTable` lookup in the dats: the UI crate's, shared with every interface.
pub use dereth_ui::text::DatStringResolver;

// String-table escape decoding is [`dereth_ui::text::unescape`], applied by the `StringResolver`
// contract itself, so every crate that resolves string-table rows runs the same pass -- see
// `dereth/client/crates/ui/src/text/escape.rs` for the byte-level reading of the unescaping step.
//
// The client runs it in the internal string lookup, straight after the row is read. The
// higher-level string getter -- what text append, attribute handling, and the margin-aware size
// query all call -- is one level above it. The text element never
// sees an escape, which is why its glyph query has no handling for one.

/// Split a command's glyphs into runs of consecutive glyphs sharing one font.
///
/// One run is one draw call, because one atlas is one texture. Every shipped screen puts a single
/// font on a label, so this is almost always one run — but per-glyph font *is* the model
/// ('s fallback picks a different font for a character the element's own
/// font does not contain), and splitting this way keeps submission order, which is the only
/// ordering the batch has.
#[must_use]
pub fn font_runs(glyphs: &[PlacedGlyph]) -> Vec<(DataId, &[PlacedGlyph])> {
    let mut out: Vec<(DataId, &[PlacedGlyph])> = Vec::new();
    let mut start = 0usize;
    while start < glyphs.len() {
        let font = glyphs[start].font;
        let mut end = start + 1;
        while end < glyphs.len() && glyphs[end].font == font {
            end += 1;
        }
        out.push((font, &glyphs[start..end]));
        start = end;
    }
    out
}

/// The six-vertices-per-glyph geometry for one run, clipped to the element's visible rectangle.
///
/// `clip` is the intersection of the command's `screen` and `clip` boxes — the same rectangle
/// [`quad`] rasterises into — because the client's text is composed *into* the element's surface
/// and can therefore never escape it. There is no scissor test in this path; clipping happens in
/// software when the surface is composed.
///
/// Returns the raw vertex bytes and how many glyphs were dropped for want of an atlas entry.
#[must_use]
pub fn glyph_vertices(
    atlas: &FontAtlas,
    glyphs: &[PlacedGlyph],
    clip: (i32, i32, i32, i32),
    fb: (u32, u32),
) -> (Vec<u8>, u64) {
    let mut batch = TextBatch::new();
    batch.begin();
    let mut dropped = 0u64;
    for g in glyphs {
        if !batch.draw_ui_glyph(atlas, g.ch, g.x, g.y, g.color, clip, fb) {
            dropped += 1;
        }
    }
    let (verts, _) = batch.end();
    (pack_text_vertices(&verts), dropped)
}

/// `TextVertex` -> the raw little-endian bytes `draw_dynamic` takes.
fn pack_text_vertices(verts: &[dereth_render::font::TextVertex]) -> Vec<u8> {
    let mut out = Vec::with_capacity(verts.len() * UI_VERTEX_BYTES);
    for v in verts {
        out.extend_from_slice(&v.origin[0].to_le_bytes());
        out.extend_from_slice(&v.origin[1].to_le_bytes());
        out.extend_from_slice(&v.origin[2].to_le_bytes());
        out.extend_from_slice(&v.diffuse.to_le_bytes());
        out.extend_from_slice(&v.u.to_le_bytes());
        out.extend_from_slice(&v.v.to_le_bytes());
    }
    out
}

/// Which texture the outline pass of a run samples, and therefore which of the client's two glyph-
/// outline arms was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineArm {
    /// Flags `0x7000`: **one** draw per glyph, out of the font's own *background* sheet, through
    /// the glyph's source rectangle dilated by the two border-pixel counts.
    BackgroundSheet,
    /// Flags `0x9000`: **eight** draws per glyph out of the *foreground* sheet, over
    /// [`dereth_render::font::OUTLINE_NEIGHBOURHOOD`]. The font has no background sheet.
    Neighbourhood,
}

/// The outline pass's geometry for one run — the original text element's first glyph pass, drawn before
/// [`glyph_vertices`]'s foreground pass over the same pens.
///
/// `colour` is the element's stored outline colour; every glyph of the run uses it because the
/// client reads that per-element value inside the loop and never the glyph's own colour.
///
/// Returns the vertex bytes, which arm produced them — the caller must bind the **outline**
/// texture for [`OutlineArm::BackgroundSheet`] and the ordinary foreground one for
/// [`OutlineArm::Neighbourhood`] — and how many glyph draws were dropped.
#[must_use]
pub fn outline_vertices(
    atlas: &FontAtlas,
    glyphs: &[PlacedGlyph],
    colour: u32,
    clip: (i32, i32, i32, i32),
    fb: (u32, u32),
) -> (Vec<u8>, OutlineArm, u64) {
    let mut batch = TextBatch::new();
    batch.begin();
    let mut dropped = 0u64;
    let arm = if atlas.has_outline_sheet() {
        for g in glyphs {
            if !batch.draw_ui_outline_glyph(atlas, g.ch, g.x, g.y, colour, clip, fb) {
                dropped += 1;
            }
        }
        OutlineArm::BackgroundSheet
    } else {
        for g in glyphs {
            // Eight draws per glyph; a glyph at the very edge of the clip box may lose some of
            // them, and one wholly outside loses all eight, which is the one that counts as
            // dropped -- the same accounting the foreground pass uses.
            if batch.draw_ui_outline_neighbourhood(atlas, g.ch, g.x, g.y, colour, clip, fb) == 0 {
                dropped += 1;
            }
        }
        OutlineArm::Neighbourhood
    };
    let (verts, _) = batch.end();
    (pack_text_vertices(&verts), arm, dropped)
}

/// The visible rectangle of a draw command: `screen` intersected with `clip`, clamped to the back
/// buffer. `None` when nothing of it is on screen.
#[must_use]
pub fn visible_box(cmd: &UiDrawCmd, fb: (u32, u32)) -> Option<(i32, i32, i32, i32)> {
    let fbw = i32::try_from(fb.0).unwrap_or(i32::MAX);
    let fbh = i32::try_from(fb.1).unwrap_or(i32::MAX);
    let x0 = cmd.screen.x0.max(cmd.clip.x0).max(0);
    let y0 = cmd.screen.y0.max(cmd.clip.y0).max(0);
    let x1 = cmd.screen.x1.min(cmd.clip.x1).min(fbw - 1);
    let y1 = cmd.screen.y1.min(cmd.clip.y1).min(fbh - 1);
    if x1 < x0 || y1 < y0 {
        return None;
    }
    Some((x0, y0, x1, y1))
}

/// Which fonts the current draw list needs, once each and in first-use order.
#[must_use]
pub fn fonts(cmds: &[UiDrawCmd]) -> Vec<DataId> {
    let mut out: Vec<DataId> = Vec::new();
    for c in cmds {
        for g in &c.glyphs {
            if g.font.0 != 0 && !out.contains(&g.font) {
                out.push(g.font);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_ui::region::Box2D;
    use dereth_ui::{BlitMode, ElemHandle};

    fn cmd(screen: Box2D, clip: Box2D) -> UiDrawCmd {
        UiDrawCmd {
            who: ElemHandle::for_test(0),
            screen,
            clip,
            image: Some(DataId(0x0600_0001)),
            image_op: None,
            image_source: dereth_ui::ImageSource::Interface,
            blit_mode: BlitMode::default(),
            alpha_blend_mod: 1.0,
            tiling_offset: (0, 0),
            rotation_z_degrees: 0,
            color: 0xFFFF_FFFF,
            glyphs: Vec::new(),
            text_outline: None,
            invert: Vec::new(),
            fills: Vec::new(),
        }
    }

    /// The overlay's vertices are the device's 24-byte records read back bit for bit, so a list
    /// lowered onto the overlay and packed again draws the bytes it was lowered from.
    #[test]
    fn overlay_vertices_are_the_device_records_bit_for_bit() {
        let c = cmd(Box2D::new(10, 20, 73, 51), Box2D::new(0, 0, 799, 599));
        let q = quad(&c, (800, 600), (16, 16)).expect("a visible quad");
        let v = overlay_vertices(&q.vertices);
        assert_eq!(v.len(), 6);
        let mut packed = Vec::new();
        for x in &v {
            for c in x.position {
                packed.extend_from_slice(&c.to_le_bytes());
            }
            packed.extend_from_slice(&x.color.to_le_bytes());
            packed.extend_from_slice(&x.uv[0].to_le_bytes());
            packed.extend_from_slice(&x.uv[1].to_le_bytes());
        }
        assert_eq!(packed, q.vertices);
    }

    /// A one pixel fill covers exactly one pixel of clip space.
    #[test]
    fn a_one_pixel_fill_covers_exactly_one_pixel_of_clip_space() {
        let c = cmd(Box2D::new(0, 0, 799, 599), Box2D::new(0, 0, 799, 599));
        let f = dereth_ui::UiFill::point(400, 300, 0xFFFF_0000);
        let v = fill_quad(&c, &f, (800, 600)).expect("a visible fill");
        let (l, r) = (f32_at(&v, 0), f32_at(&v, 2 * UI_VERTEX_BYTES));
        // Pixel 400 of 800 spans clip x from 2*400/800 - 1 = 0.0 to 2*401/800 - 1 = 0.0025.
        assert!((l - 0.0).abs() < 1e-6, "left edge {l}");
        assert!((r - 0.0025).abs() < 1e-6, "right edge {r}");
        // ...which is exactly one pixel wide.
        assert!(((r - l) * 800.0 / 2.0 - 1.0).abs() < 1e-4);
    }

    /// The null colour cannot change a pixel under `SRCALPHA`/`INVSRCALPHA`, so it never reaches
    /// the rasteriser -- and neither does a fill clipped entirely outside its element.
    #[test]
    fn an_invisible_or_clipped_fill_produces_no_geometry() {
        let c = cmd(Box2D::new(10, 10, 50, 50), Box2D::new(10, 10, 50, 50));
        assert!(fill_quad(
            &c,
            &dereth_ui::UiFill::point(20, 20, dereth_ui::UiFill::NULL),
            (800, 600)
        )
        .is_none());
        assert!(fill_quad(
            &c,
            &dereth_ui::UiFill::point(200, 20, 0xFFFF_FFFF),
            (800, 600)
        )
        .is_none());
        // ...and one that straddles the edge is clipped to the element rather than dropped.
        let big = dereth_ui::UiFill {
            x: 40,
            y: 40,
            w: 100,
            h: 100,
            color: 0xFFFF_FFFF,
        };
        assert!(fill_quad(&c, &big, (800, 600)).is_some());
    }

    fn f32_at(v: &[u8], i: usize) -> f32 {
        f32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]])
    }

    /// An unclipped element covers exactly its own pixel rectangle.
    #[test]
    fn an_unclipped_element_covers_exactly_its_own_pixel_rectangle() {
        let fb = (800u32, 600u32);
        for (x, y, w, h) in [
            (0i32, 0i32, 100i32, 40i32),
            (37, 91, 64, 64),
            (10, 10, 1, 1),
        ] {
            let b = Box2D {
                x0: x,
                y0: y,
                x1: x + w - 1,
                y1: y + h - 1,
            };
            let v = quad(
                &cmd(b, b),
                fb,
                (u32::try_from(w).unwrap(), u32::try_from(h).unwrap()),
            )
            .expect("visible")
            .vertices;
            assert_eq!(v.len(), 6 * UI_VERTEX_BYTES);

            let left = f32_at(&v, 0);
            let bottom = f32_at(&v, UI_VERTEX_BYTES + 4);
            let rect = dereth_render::ui::ClipRect {
                x: left,
                y: bottom,
                sx: f32_at(&v, 2 * UI_VERTEX_BYTES) - left,
                sy: f32_at(&v, 4) - bottom,
            };
            let surf = dereth_render::ui::UiSurface::create(
                u32::try_from(w).unwrap(),
                u32::try_from(h).unwrap(),
                true,
            )
            .unwrap();
            let (cols, rows) = dereth_render::ui::coverage(
                &rect,
                fb,
                surf.physical,
                surf.texture,
                dereth_render::ui::PixelCentre::D3D12,
            );
            let col_ids: Vec<i64> = cols.iter().map(|c| c.0).collect();
            let row_ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
            assert_eq!(
                col_ids,
                (i64::from(x)..i64::from(x + w)).collect::<Vec<_>>(),
                "columns for {w}x{h} at ({x},{y})"
            );
            assert_eq!(
                row_ids,
                (i64::from(y)..i64::from(y + h)).collect::<Vec<_>>(),
                "rows for {w}x{h} at ({x},{y})"
            );
        }
    }

    // Oracle: `dereth_ui::region` keeps the screen clip box valid whenever it emits a command, and
    // uses the inclusive box convention
    // `w = x1 - x0 + 1`. A clip that removes the left half must move the u range, not squash it.
    #[test]
    fn a_clipped_element_shows_the_part_of_its_picture_that_is_still_on_screen() {
        let fb = (800u32, 600u32);
        let screen = Box2D {
            x0: 100,
            y0: 100,
            x1: 199,
            y1: 199,
        };
        let clip = Box2D {
            x0: 150,
            y0: 100,
            x1: 199,
            y1: 199,
        };
        // The picture is exactly the element's 100 x 100, so this is the crop arm.
        let v = quad(&cmd(screen, clip), fb, (100, 100))
            .expect("visible")
            .vertices;
        // u at the first vertex is (150 - 100) / 100 = 0.5, and the right edge is still 1.0.
        assert!(
            (f32_at(&v, 16) - 0.5).abs() < 1e-6,
            "u0 = {}",
            f32_at(&v, 16)
        );
        assert!(
            (f32_at(&v, 2 * UI_VERTEX_BYTES + 16) - 1.0).abs() < 1e-6,
            "u1 = {}",
            f32_at(&v, 2 * UI_VERTEX_BYTES + 16)
        );
    }

    #[test]
    fn an_element_clipped_to_nothing_produces_no_geometry() {
        let fb = (800u32, 600u32);
        let screen = Box2D {
            x0: 100,
            y0: 100,
            x1: 199,
            y1: 199,
        };
        let clip = Box2D {
            x0: 300,
            y0: 300,
            x1: 399,
            y1: 399,
        };
        assert!(quad(&cmd(screen, clip), fb, (100, 100)).is_none());
        // ...and so does one entirely off the back buffer.
        let off = Box2D {
            x0: -400,
            y0: -400,
            x1: -1,
            y1: -1,
        };
        assert!(quad(&cmd(off, off), fb, (100, 100)).is_none());
    }

    // Oracle: the alpha-blend modifier is carried in the diffuse alpha.
    #[test]
    fn the_alpha_blend_modifier_rides_in_the_diffuse_alpha() {
        let fb = (64u32, 64u32);
        let b = Box2D {
            x0: 0,
            y0: 0,
            x1: 31,
            y1: 31,
        };
        let mut c = cmd(b, b);
        c.alpha_blend_mod = 0.5;
        let v = quad(&c, fb, (32, 32)).expect("visible").vertices;
        let diffuse = u32::from_le_bytes([v[12], v[13], v[14], v[15]]);
        assert_eq!(diffuse >> 24, 127, "0.5 * 255 truncates toward zero");
        c.alpha_blend_mod = 1.0;
        let v = quad(&c, fb, (32, 32)).expect("visible").vertices;
        let diffuse = u32::from_le_bytes([v[12], v[13], v[14], v[15]]);
        assert_eq!(diffuse, 0xFFFF_FFFF);
    }

    #[test]
    fn the_image_list_is_first_use_order_and_has_no_duplicates() {
        let b = Box2D {
            x0: 0,
            y0: 0,
            x1: 9,
            y1: 9,
        };
        let mut a = cmd(b, b);
        a.image = Some(DataId(2));
        let mut c = cmd(b, b);
        c.image = Some(DataId(1));
        let mut d = cmd(b, b);
        d.image = None;
        assert_eq!(
            images(&[a.clone(), c, a.clone(), d]),
            vec![
                (DataId(2), None, ImageSource::Interface),
                (DataId(1), None, ImageSource::Interface)
            ]
        );

        let mut e = cmd(b, b);
        e.image = Some(DataId(2));
        e.image_op = Some(dereth_ui::region::SurfaceOp::ReplaceColor {
            from: 0xFF00_0000,
            to: 0xFF12_3456,
        });
        assert_eq!(
            images(&[a, e.clone(), e.clone()]).len(),
            2,
            "one per distinct pair"
        );
        assert_ne!(
            image_key(DataId(2), None),
            image_key(DataId(2), e.image_op),
            "a derived surface must not collide with its own template in the descriptor cache"
        );

        // The same id read from the world's files is another picture, and another texture.
        let mut w = cmd(b, b);
        w.image = Some(DataId(2));
        w.image_source = ImageSource::World;
        assert_eq!(images(&[w.clone(), e.clone(), w]).len(), 2);
        assert_ne!(
            image_key_from(DataId(2), None, ImageSource::World),
            image_key_from(DataId(2), None, ImageSource::Interface),
        );
        assert_ne!(
            image_key_from(DataId(2), None, ImageSource::World),
            image_key_from(DataId(2), e.image_op, ImageSource::Interface),
        );
    }
}
