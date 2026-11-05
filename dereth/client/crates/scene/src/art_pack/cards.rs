//! Cards worked out as a pack opens: each model with no pictures of its own is drawn from round
//! it into a level of pictures ([`dereth_assets::art_pack`] says how one is laid out), which
//! takes over from its shape past [`cards_from`].

use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;
use std::sync::Arc;

use dereth_assets::art_pack::Views;
use dereth_primitives::num::math;
use dereth_primitives::DataId;

use super::{images, Image, PackLod, PackMaterial, PackModel, PackPrimitive, VERTEX_FLOATS};

/// The views round a card's model, and the tiles to a row.
pub const VIEWS: Views = Views {
    count: 16,
    columns: 4,
};

/// How far a card takes over from its model's shape, metres a metre of its scale: never, so the
/// loaded blocks draw shapes throughout and the far pass alone draws cards.
pub const CARDS_FROM: f32 = f32::INFINITY;

/// A tile's texels a metre of its model's height, and the least and most of its height.
const TEXELS_A_METRE: f32 = 8.0;
const LEAST_TILE: u32 = 32;
const MOST_TILE: u32 = 128;

/// Texels drawn each way for each texel kept, before the pictures are shrunk to their size.
const SUPERSAMPLE: u32 = 2;

/// How many texels the colour round a card's edges is carried into the empty texels beyond.
const BLEED: u32 = 6;

/// The cut a material with no cut of its own is drawn through.
const OPAQUE_CUT: f32 = 0.5;

/// The share of its colour a texel no other view sees keeps: shaded by the rest of the model.
const HIDDEN_KEPT: f32 = 0.2;

/// The share of the views a texel is seen from where it keeps all its colour, matched to the
/// models drawn whole: a leaf's, and a solid surface's, an open face of which is seen from seven
/// views of the sixteen.
const LEAF_OPEN_SHARE: f32 = 2.0 / 3.0;
const SOLID_OPEN_SHARE: f32 = 0.4;

/// [`CARDS_FROM`], or `DERETH_CARDS_FROM` where it is set.
#[must_use]
pub fn cards_from() -> f32 {
    std::env::var("DERETH_CARDS_FROM")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|v: &f32| *v > 0.0)
        .unwrap_or(CARDS_FROM)
}

/// A picture's largest level, four bytes a texel.
pub(crate) struct Texels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Texels {
    /// `rgba`, `width` by `height`; `None` when it is not that many texels.
    pub(crate) fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        (width > 0 && height > 0 && rgba.len() == (width * height * 4) as usize).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    fn of(image: &Image) -> Option<Self> {
        let level = image.levels.first()?;
        let rgba = match image.format {
            Some(f) => dereth_render::block_codec::decode(f, image.width, image.height, level),
            None => level.to_vec(),
        };
        (rgba.len() == (image.width * image.height * 4) as usize).then_some(Self {
            width: image.width,
            height: image.height,
            rgba,
        })
    }

    /// The texel at `uv`, the picture repeated.
    fn at(&self, uv: [f32; 2]) -> [u8; 4] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // wrapped into range
        let (x, y) = (
            ((uv[0].rem_euclid(1.0) * self.width as f32) as u32).min(self.width - 1),
            ((uv[1].rem_euclid(1.0) * self.height as f32) as u32).min(self.height - 1),
        );
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }
}

/// A card worked out for a model: its colour and normal pictures, and its level, whose quad names
/// no pictures yet.
#[derive(Debug)]
pub(crate) struct Card {
    pub(crate) colour: Image,
    pub(crate) normal: Image,
    pub(crate) lod: PackLod,
}

/// One part a card is drawn from: triangles of one surface, in the model's own space, height up.
pub(crate) struct Part<'a> {
    /// [`VERTEX_FLOATS`] floats a vertex.
    pub(crate) vertices: &'a [f32],
    /// Three a triangle.
    pub(crate) indices: &'a [u32],
    /// Its base colour picture.
    pub(crate) picture: Option<&'a Texels>,
    /// The base colour, linear, and its alpha.
    pub(crate) base_color: [f32; 4],
    /// The alpha it is cut below.
    pub(crate) cut: Option<f32>,
    /// Whether both faces are drawn.
    pub(crate) double_sided: bool,
    /// Its roughness.
    pub(crate) roughness: f32,
}

/// Give each of `models` with no pictures of its own, but those `skip` names, a card, its
/// pictures added to `images`.
pub fn add_cards(models: &mut [PackModel], images: &mut Vec<Image>, skip: &HashSet<DataId>) {
    let wanted: Vec<usize> = (0..models.len())
        .filter(|k| !skip.contains(&models[*k].id) && source(&models[*k]).is_some())
        .collect();
    let mut used: Vec<u32> = wanted
        .iter()
        .filter_map(|k| source(&models[*k]))
        .flat_map(|l| &l.primitives)
        .filter_map(|p| p.material.base_color_image)
        .collect();
    used.sort_unstable();
    used.dedup();
    let shared: &[Image] = images;
    let decoded = images::par_map(used.clone(), &|i| {
        shared.get(i as usize).and_then(Texels::of)
    });
    let pictures: HashMap<u32, Texels> = used
        .into_iter()
        .zip(decoded)
        .filter_map(|(i, t)| t.map(|t| (i, t)))
        .collect();
    let shapes: &[PackModel] = models;
    let base = u32::try_from(images.len()).unwrap_or(u32::MAX);
    let cards = images::par_map(wanted.clone(), &|k| bake(&shapes[k], &pictures));
    let mut next = base;
    for (k, card) in wanted.into_iter().zip(cards) {
        let Some(mut card) = card else {
            continue;
        };
        for p in &mut card.lod.primitives {
            p.material.base_color_image = Some(next);
            p.material.normal_image = Some(next + 1);
        }
        images.push(card.colour);
        images.push(card.normal);
        next += 2;
        hand_over(&mut models[k], card.lod, cards_from());
    }
}

/// The level a model's card is drawn from: its first level of shape the view draws, for a model
/// with no level of pictures.
fn source(m: &PackModel) -> Option<&PackLod> {
    if m.lods.iter().any(|l| l.views.is_some()) {
        return None;
    }
    m.lods
        .iter()
        .find(|l| !l.shadows_only && !l.primitives.is_empty())
}

/// `card` added to `m` as its last level, drawn past `from`: the first level of shape reaching
/// it stops there, and those past it are drawn into the shadows alone.
fn hand_over(m: &mut PackModel, card: PackLod, from: f32) {
    let mut reached = false;
    for lod in m.lods.iter_mut().filter(|l| !l.shadows_only) {
        if reached {
            lod.shadows_only = true;
        } else if lod.distance >= from {
            lod.distance = from;
            reached = true;
        }
    }
    if reached {
        m.lods.push(card);
    }
}

/// Where a model's card stands round its upright axis: half its width, its foot and its top.
fn extent(parts: &[Part<'_>]) -> Option<(f32, f32, f32)> {
    let (mut half, mut foot, mut top) = (0.0f32, f32::MAX, f32::MIN);
    for p in parts {
        for v in p.vertices.as_chunks::<VERTEX_FLOATS>().0 {
            half = half.max(math::hypotf(v[0], v[1]));
            foot = foot.min(v[2]);
            top = top.max(v[2]);
        }
    }
    (half > 0.05 && top - foot > 0.05).then_some((half, foot, top))
}

/// `v` rounded up to a whole block of four.
fn blocks(v: f32) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small positive size
    let n = v.ceil().max(4.0) as u32;
    n.next_multiple_of(4)
}

/// The texels of one view as drawn: the nearest surface's depth, colour, normal and coverage,
/// and whether it is a leaf (a cut-out surface, or any of a plant's).
struct Drawn {
    depth: Vec<f32>,
    colour: Vec<[f32; 3]>,
    normal: Vec<[f32; 3]>,
    leaf: Vec<bool>,
}

/// `m`'s card, drawn from `pictures` (its base colour pictures, by index); `None` for a model too
/// small or flat to draw.
fn bake(m: &PackModel, pictures: &HashMap<u32, Texels>) -> Option<Card> {
    let parts: Vec<Part<'_>> = source(m)?
        .primitives
        .iter()
        .map(|p| Part {
            vertices: &p.vertices,
            indices: &p.indices,
            picture: p.material.base_color_image.and_then(|i| pictures.get(&i)),
            base_color: p.material.base_color,
            cut: p.material.alpha_cutoff,
            double_sided: p.material.double_sided,
            roughness: p.material.roughness,
        })
        .collect();
    bake_parts(&parts, m.foliage)
}

/// The card of a model drawn from `parts` (cut-out growth if `foliage`); `None` for one too small
/// or flat to draw.
#[allow(clippy::too_many_lines)] // one drawing of every view
pub(crate) fn bake_parts(parts: &[Part<'_>], foliage: bool) -> Option<Card> {
    let (half, foot, top) = extent(parts)?;
    let height = top - foot;
    #[allow(clippy::cast_precision_loss)] // tile sizes
    let tile_h = blocks(height * TEXELS_A_METRE).clamp(LEAST_TILE, MOST_TILE);
    #[allow(clippy::cast_precision_loss)]
    let tile_w = blocks(tile_h as f32 * 2.0 * half / height).clamp(16, 2 * MOST_TILE);
    let (count, columns) = (VIEWS.count, VIEWS.columns);
    let rows = count.div_ceil(columns);
    let (sw, sh) = (tile_w * SUPERSAMPLE, tile_h * SUPERSAMPLE);
    let (width, height_px) = (sw * columns, sh * rows);
    let texels = (width * height_px) as usize;
    let mut drawn = Drawn {
        depth: vec![f32::INFINITY; texels],
        colour: vec![[0.0; 3]; texels],
        normal: vec![[0.0, 0.0, 1.0]; texels],
        leaf: vec![false; texels],
    };
    let mut roughness = (0.0f32, 0.0f32);
    for p in parts {
        roughness.0 += p.roughness;
        roughness.1 += 1.0;
    }
    for k in 0..count {
        let (toward, across) = bearing(k);
        let (ox, oy) = ((k % columns) * sw, (k / columns) * sh);
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (sw as f32, sh as f32);
        for p in parts {
            let picture = p.picture;
            let cut = p.cut;
            let verts = p.vertices.as_chunks::<VERTEX_FLOATS>().0;
            // Each vertex on the view's tile: across, down from the top, and how far along the
            // view from the plane through the axis.
            let placed: Vec<[f32; 3]> = verts
                .iter()
                .map(|v| {
                    [
                        (v[0] * across[0] + v[1] * across[1]) / half * 0.5 * fw + 0.5 * fw,
                        (top - v[2]) / height * fh,
                        -(v[0] * toward[0] + v[1] * toward[1]),
                    ]
                })
                .collect();
            for tri in p.indices.as_chunks::<3>().0 {
                let [a, b, c] = tri.map(|i| i as usize);
                let (Some(pa), Some(pb), Some(pc)) = (placed.get(a), placed.get(b), placed.get(c))
                else {
                    continue;
                };
                let area = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pc[0] - pa[0]) * (pb[1] - pa[1]);
                if area.abs() < 1e-9 {
                    continue;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let span = |lo: f32, hi: f32, most: u32| {
                    (
                        lo.floor().max(0.0) as u32,
                        (hi.ceil().max(0.0) as u32).min(most),
                    )
                };
                let (x0, x1) = span(pa[0].min(pb[0]).min(pc[0]), pa[0].max(pb[0]).max(pc[0]), sw);
                let (y0, y1) = span(pa[1].min(pb[1]).min(pc[1]), pa[1].max(pb[1]).max(pc[1]), sh);
                for y in y0..y1 {
                    for x in x0..x1 {
                        #[allow(clippy::cast_precision_loss)]
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let w0 = ((pb[0] - px) * (pc[1] - py) - (pc[0] - px) * (pb[1] - py)) / area;
                        let w1 = ((pc[0] - px) * (pa[1] - py) - (pa[0] - px) * (pc[1] - py)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let s = w0 * pa[2] + w1 * pb[2] + w2 * pc[2];
                        let i = ((oy + y) * width + ox + x) as usize;
                        if s >= drawn.depth[i] {
                            continue;
                        }
                        let mix = |o: usize, n: usize| -> [f32; 4] {
                            let mut out = [0.0; 4];
                            for (j, slot) in out.iter_mut().enumerate().take(n) {
                                *slot = w0 * verts[a][o + j]
                                    + w1 * verts[b][o + j]
                                    + w2 * verts[c][o + j];
                            }
                            out
                        };
                        let uv = mix(10, 2);
                        let tint = mix(12, 4);
                        let texel = picture.map_or([255; 4], |t| t.at([uv[0], uv[1]]));
                        let alpha = f32::from(texel[3]) / 255.0 * p.base_color[3] * tint[3];
                        if cut.is_some_and(|c| alpha < c) {
                            continue;
                        }
                        let n = mix(3, 3);
                        let mut n = normalised([n[0], n[1], n[2]]);
                        // A two-sided face seen from behind faces the eye; a leaf keeps its plant's.
                        if !foliage && p.double_sided && n[0] * toward[0] + n[1] * toward[1] < 0.0 {
                            n = n.map(|c| -c);
                        }
                        drawn.depth[i] = s;
                        drawn.colour[i] = [0, 1, 2]
                            .map(|c| linear(texel[c]) * p.base_color[c] * tint[c].clamp(0.0, 1.0));
                        drawn.normal[i] = n;
                        drawn.leaf[i] = foliage || cut.is_some();
                    }
                }
            }
        }
    }
    shade_hidden(&mut drawn, (sw, sh), width, half);
    let (colour, normal) = pictures_of(&drawn, width, height_px, half);
    let shrunk = |rgba: Vec<u8>, srgb: bool| {
        let mut made = images::with_view_levels(width, height_px, rgba, srgb);
        made.levels.remove(0);
        made.width = width / SUPERSAMPLE;
        made.height = height_px / SUPERSAMPLE;
        images::compressed(made, images::BlockFormat::Bc7)
    };
    let colour = shrunk(colour, true);
    let normal = shrunk(normal, false);
    let rough = if roughness.1 > 0.0 {
        roughness.0 / roughness.1
    } else {
        1.0
    };
    Some(Card {
        colour,
        normal,
        lod: PackLod {
            distance: f32::INFINITY,
            primitives: vec![quad(half, foot, top, rough)],
            views: Some(VIEWS),
            shadows_only: false,
        },
    })
}

/// Each view's bearing round the axis: the way it looks along, and the way across it.
fn bearing(k: u32) -> ([f32; 2], [f32; 2]) {
    #[allow(clippy::cast_precision_loss)] // a handful of views
    let theta = TAU * k as f32 / VIEWS.count as f32;
    let (sin, cos) = (math::sinf(theta), math::cosf(theta));
    ([cos, sin], [-sin, cos])
}

/// `drawn`'s colour darkened by how few of the views see each texel: a needle deep in its crown
/// is shaded by the rest of it, as the model's own shadows and occlusion shade it.
fn shade_hidden(drawn: &mut Drawn, tile: (u32, u32), width: u32, half: f32) {
    let (sw, sh) = tile;
    let (count, columns) = (VIEWS.count, VIEWS.columns);
    let ways: Vec<([f32; 2], [f32; 2])> = (0..count).map(bearing).collect();
    #[allow(clippy::cast_precision_loss)] // tile sizes
    let (fw, views) = (sw as f32, count as f32);
    // A texel nearer than another view's surface by less than this is the surface it sees.
    let slack = 3.0 * 2.0 * half / fw;
    let at = |k: u32, x: u32, y: u32| {
        (((k / columns) * sh + y) * width + (k % columns) * sw + x) as usize
    };
    let mut kept = vec![1.0f32; drawn.depth.len()];
    for (k, (toward, across)) in (0..count).zip(&ways) {
        for y in 0..sh {
            for x in 0..sw {
                let s = drawn.depth[at(k, x, y)];
                if !s.is_finite() {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let a = ((x as f32 + 0.5) / fw - 0.5) * 2.0 * half;
                let p = [across[0] * a - toward[0] * s, across[1] * a - toward[1] * s];
                let i = at(k, x, y);
                let (leaf, n) = (drawn.leaf[i], drawn.normal[i]);
                // A solid surface is seen only from in front of it; a leaf from either side.
                let seen = (0..count)
                    .zip(&ways)
                    .filter(|(j, (t, c))| {
                        if !leaf && n[0] * t[0] + n[1] * t[1] < -0.05 {
                            return false;
                        }
                        let xj = (p[0] * c[0] + p[1] * c[1]) / half * 0.5 * fw + 0.5 * fw;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let xj = (xj.max(0.0) as u32).min(sw - 1);
                        -(p[0] * t[0] + p[1] * t[1]) <= drawn.depth[at(*j, xj, y)] + slack
                    })
                    .count();
                let share = if leaf {
                    LEAF_OPEN_SHARE
                } else {
                    SOLID_OPEN_SHARE
                };
                #[allow(clippy::cast_precision_loss)]
                let open = (seen as f32 / views / share).min(1.0);
                kept[i] = HIDDEN_KEPT + (1.0 - HIDDEN_KEPT) * open;
            }
        }
    }
    for (c, k) in drawn.colour.iter_mut().zip(kept) {
        *c = c.map(|v| v * k);
    }
}

/// The colour picture (coverage in alpha) and normal picture (depth in alpha) of `drawn`, the
/// colour and normal carried a little way into the empty texels so their edges filter cleanly.
fn pictures_of(drawn: &Drawn, width: u32, height: u32, half: f32) -> (Vec<u8>, Vec<u8>) {
    let n = (width * height) as usize;
    let mut filled: Vec<bool> = drawn.depth.iter().map(|d| d.is_finite()).collect();
    let mut colour = drawn.colour.clone();
    let mut normal = drawn.normal.clone();
    for _ in 0..BLEED * SUPERSAMPLE {
        let was = filled.clone();
        for y in 0..height {
            for x in 0..width {
                let i = (y * width + x) as usize;
                if was[i] {
                    continue;
                }
                let (mut c, mut nn, mut k) = ([0.0f32; 3], [0.0f32; 3], 0.0f32);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let (qx, qy) = (x.cast_signed() + dx, y.cast_signed() + dy);
                    if qx < 0 || qy < 0 || qx >= width.cast_signed() || qy >= height.cast_signed() {
                        continue;
                    }
                    let j = (qy.cast_unsigned() * width + qx.cast_unsigned()) as usize;
                    if was[j] {
                        for e in 0..3 {
                            c[e] += colour[j][e];
                            nn[e] += normal[j][e];
                        }
                        k += 1.0;
                    }
                }
                if k > 0.0 {
                    colour[i] = c.map(|v| v / k);
                    normal[i] = normalised(nn);
                    filled[i] = true;
                }
            }
        }
    }
    let mut c_out = Vec::with_capacity(n * 4);
    let mut n_out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let covered = drawn.depth[i].is_finite();
        c_out.extend(colour[i].map(srgb));
        c_out.push(if covered { 255 } else { 0 });
        n_out.extend(normal[i].map(|v| unit_byte(v * 0.5 + 0.5)));
        n_out.push(if covered {
            unit_byte(0.5 + drawn.depth[i] / (2.0 * half))
        } else {
            255
        });
    }
    (c_out, n_out)
}

/// The card's upright quad across its model's axis, its first axis across and its height up,
/// the top of its pictures at its top.
fn quad(half: f32, foot: f32, top: f32, roughness: f32) -> PackPrimitive {
    let corner = |x: f32, z: f32, u: f32, v: f32| {
        [
            x, 0.0, z, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 1.0, u, v, 1.0, 1.0, 1.0, 1.0,
        ]
    };
    let vertices: Vec<f32> = [
        corner(-half, top, 0.0, 0.0),
        corner(half, top, 1.0, 0.0),
        corner(half, foot, 1.0, 1.0),
        corner(-half, foot, 0.0, 1.0),
    ]
    .concat();
    PackPrimitive {
        vertices: Arc::new(vertices),
        indices: Arc::new(vec![0, 2, 1, 0, 3, 2]),
        material: PackMaterial {
            base_color: [1.0; 4],
            base_color_image: None,
            normal_image: None,
            normal_scale: 1.0,
            metal_rough_image: None,
            metallic: 0.0,
            roughness,
            occlusion_image: None,
            occlusion_strength: 0.0,
            alpha_cutoff: Some(OPAQUE_CUT),
            double_sided: true,
        },
    }
}

fn normalised(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-6 {
        v.map(|c| c / l)
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        math::powf((c + 0.055) / 1.055, 2.4)
    }
}

fn srgb(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * math::powf(c, 1.0 / 2.4) - 0.055
    };
    unit_byte(s)
}

fn unit_byte(v: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..255
    let b = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    b
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    /// A model of one upright square of `material`, `half` wide each way and two metres tall,
    /// facing the first axis, at one level drawn to `distance`.
    fn square(half: f32, distance: f32) -> PackModel {
        let corner = |y: f32, z: f32| {
            [
                0.0, y, z, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
            ]
        };
        let vertices = [
            corner(-half, 0.0),
            corner(half, 0.0),
            corner(half, 2.0),
            corner(-half, 2.0),
        ]
        .concat();
        PackModel {
            id: DataId(0x0200_0001),
            foliage: false,
            lods: vec![PackLod {
                distance,
                primitives: vec![PackPrimitive {
                    vertices: Arc::new(vertices),
                    indices: Arc::new(vec![0, 1, 2, 0, 2, 3]),
                    material: PackMaterial {
                        alpha_cutoff: None,
                        ..quad(1.0, 0.0, 1.0, 1.0).material
                    },
                }],
                views: None,
                shadows_only: false,
            }],
            bounds: ([0.0, 0.0, 1.0], 1.5),
            credit: dereth_assets::art_pack::Credit::default(),
        }
    }

    #[test]
    fn a_model_gets_a_card_level_past_the_distance_and_its_farther_shapes_shadow_alone() {
        let mut m = square(1.0, f32::INFINITY);
        let mut far = m.lods[0].clone();
        far.distance = f32::INFINITY;
        m.lods[0].distance = 100.0;
        m.lods.push(far.clone());
        m.lods.push(far);
        let card = bake(&m, &HashMap::new()).expect("a card");
        hand_over(&mut m, card.lod, 300.0);
        let shown: Vec<(f32, bool, bool)> = m
            .lods
            .iter()
            .map(|l| (l.distance, l.views.is_some(), l.shadows_only))
            .collect();
        assert_eq!(
            shown,
            [
                (100.0, false, false),
                (300.0, false, false),
                (f32::INFINITY, false, true),
                (f32::INFINITY, true, false),
            ]
        );
    }

    #[test]
    fn a_card_covers_its_model_from_the_front_and_shows_it_edge_on_from_the_side() {
        let m = square(1.0, f32::INFINITY);
        let card = bake(&m, &HashMap::new()).expect("a card");
        // The tiles: two metres tall at eight texels a metre (the least, 32), as wide as the
        // square's two metres.
        assert_eq!((card.colour.width, card.colour.height), (4 * 32, 4 * 32));
        let colour = Texels::of(&card.colour).expect("decodes");
        // View 0 looks along the first axis, at the square's face: its tile's middle is covered.
        assert!(colour.at([16.0 / 128.0, 16.0 / 128.0])[3] > 200);
        // View 4, a quarter turn round, sees the square edge on: its middle is all but empty.
        assert!(colour.at([16.0 / 128.0, 48.0 / 128.0])[3] < 64);
        assert_eq!(card.lod.views, Some(VIEWS));
        assert_eq!(card.lod.primitives[0].indices.len(), 6);
    }

    /// A white solid model of `quads`, each four corners round its face and the way it faces.
    fn solid(quads: &[([[f32; 3]; 4], [f32; 3])]) -> (Vec<f32>, Vec<u32>) {
        let (mut vertices, mut indices) = (Vec::new(), Vec::new());
        for (corners, n) in quads {
            let base = u32::try_from(vertices.len() / VERTEX_FLOATS).expect("a few");
            for c in corners {
                vertices.extend([
                    c[0], c[1], c[2], n[0], n[1], n[2], 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0,
                    1.0, 1.0,
                ]);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        (vertices, indices)
    }

    /// The card of `quads`, and its colour picture at the middle of view 0's tile.
    fn middle_of_view_0(quads: &[([[f32; 3]; 4], [f32; 3])]) -> [u8; 4] {
        let (vertices, indices) = solid(quads);
        let part = Part {
            vertices: &vertices,
            indices: &indices,
            picture: None,
            base_color: [1.0; 4],
            cut: None,
            double_sided: false,
            roughness: 1.0,
        };
        let card = bake_parts(&[part], false).expect("a card");
        let colour = Texels::of(&card.colour).expect("decodes");
        // Four tiles to a row and a column: view 0's middle.
        colour.at([0.125, 0.125])
    }

    /// The face across x = `x` from y = `y0` to `y1`, two metres tall, facing `n`.
    fn wall_x(x: f32, (y0, y1): (f32, f32), n: f32) -> ([[f32; 3]; 4], [f32; 3]) {
        (
            [[x, y0, 0.0], [x, y1, 0.0], [x, y1, 2.0], [x, y0, 2.0]],
            [n, 0.0, 0.0],
        )
    }

    /// The face across y = `y` from x = `x0` to `x1`, two metres tall, facing `n`.
    fn wall_y(y: f32, (x0, x1): (f32, f32), n: f32) -> ([[f32; 3]; 4], [f32; 3]) {
        (
            [[x0, y, 0.0], [x1, y, 0.0], [x1, y, 2.0], [x0, y, 2.0]],
            [0.0, n, 0.0],
        )
    }

    #[test]
    fn a_solid_models_open_face_keeps_its_colour_and_the_back_of_a_narrow_nook_darkens() {
        // A box: view 0 looks at its face across x = 1, which it shares with the views round it.
        let open = middle_of_view_0(&[
            wall_x(1.0, (-1.0, 1.0), 1.0),
            wall_x(-1.0, (-1.0, 1.0), -1.0),
            wall_y(1.0, (-1.0, 1.0), 1.0),
            wall_y(-1.0, (-1.0, 1.0), -1.0),
        ]);
        assert!(open[0] >= 250, "an open face keeps its colour: {open:?}");
        // A channel open toward view 0, two metres deep and half a metre wide: its back is seen
        // from straight in alone.
        let nook = middle_of_view_0(&[
            wall_x(-1.0, (-0.25, 0.25), 1.0),
            wall_y(0.25, (-1.0, 1.0), -1.0),
            wall_y(-0.25, (-1.0, 1.0), 1.0),
        ]);
        assert!(nook[3] > 200, "the back is drawn: {nook:?}");
        assert!(nook[0] < 200, "the back of a nook is shaded: {nook:?}");
    }

    #[test]
    fn a_model_with_pictures_of_its_own_or_skipped_gets_no_card() {
        let mut own = square(1.0, f32::INFINITY);
        own.lods[0].views = Some(VIEWS);
        let plain = square(1.0, f32::INFINITY);
        let skipped = PackModel {
            id: DataId(0x0200_0002),
            ..square(1.0, f32::INFINITY)
        };
        let mut models = vec![own, plain, skipped];
        let mut images = Vec::new();
        add_cards(&mut models, &mut images, &[DataId(0x0200_0002)].into());
        let levels: Vec<usize> = models.iter().map(|m| m.lods.len()).collect();
        assert_eq!(levels, [1, 2, 1]);
        assert_eq!(images.len(), 2);
        let card = &models[1].lods[1].primitives[0].material;
        assert_eq!(
            (card.base_color_image, card.normal_image),
            (Some(0), Some(1))
        );
    }
}
