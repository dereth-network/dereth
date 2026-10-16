//! The forest cover: how much of each landscape vertex's cell the crowns of the trees an art
//! pack has far pictures for cover, how high they stand, and their colour, for the far ground
//! past the far trees.
//!
//! It is worked out from the land alone, as retail's scenery is placed: each vertex's terrain
//! type and scene type choose its scene by retail's hash, and each object of the scene that the
//! pack draws far off adds its crown's area times the share of vertices the scene places it at
//! (its frequency), where the ground is no road and its slope is one the object grows on. That is
//! what the placement grows on average, which is all the far ground can show; the hashes that
//! decide each tree, the block's edges and the cells buildings own are left out.

use std::collections::HashMap;

use dereth_assets::region::Region;
use dereth_assets::world::Scene;
use dereth_primitives::DataId;
use dereth_terrain::scenery::{check_slope, get_scene, num_scene_type, scene_count, scene_index};

use super::drawn_ground::{World, WORLD_SIDE};

/// The ground a landscape vertex's scene places its objects over, square meters: one cell.
const CELL_AREA: f32 = 24.0 * 24.0;
/// The tallest crowns' height the cover holds, meters: its height is kept in quarter meters.
const HEIGHT_STEP: f32 = 0.25;

/// What the forest cover takes of one model the pack draws far off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CoverModel {
    /// How far its crown reaches from its axis, meters.
    pub(crate) reach: f32,
    /// How tall it stands, meters.
    pub(crate) height: f32,
    /// Its far pictures' mean colour, linear.
    pub(crate) colour: [f32; 3],
}

/// The forest cover, a texel a landscape vertex, rows from the south.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cover {
    /// Each vertex's crown colour in the display's encoding and the share covered, four bytes.
    pub(crate) colour: Vec<u8>,
    /// Each vertex's canopy: the share covered times the crowns' height, quarter meters.
    pub(crate) canopy: Vec<u8>,
    /// How long it took, milliseconds.
    pub(crate) made_ms: f64,
}

fn to_srgb(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let e = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * dereth_primitives::num::math::powf(c, 1.0 / 2.4) - 0.055
    };
    // LINT-OK: a 0..1 value scaled to 0..255 and rounded.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (e * 255.0 + 0.5) as u8;
    b
}

/// The forest cover of `world`, its scenes chosen from `region` and read through `scenes`, the
/// models the pack draws far off by their retail object.
pub(crate) fn forest_cover(
    world: &World,
    region: &Region,
    scenes: &mut dyn FnMut(DataId) -> Option<Scene>,
    models: &HashMap<u32, CoverModel>,
) -> Cover {
    let started = web_time::Instant::now();
    let side = WORLD_SIDE;
    let mut colour = vec![0u8; side * side * 4];
    let mut canopy = vec![0u8; side * side];
    // ORDER-OK: a read memo, only looked up.
    let mut read: HashMap<DataId, Option<Scene>> = HashMap::new();
    let height = |x: usize, y: usize| world.heights[y.min(side - 1) * side + x.min(side - 1)];
    for gy in 0..side {
        for gx in 0..side {
            let w = world.words[gy * side + gx];
            if w & 3 != 0 {
                // A road grows no scenery.
                continue;
            }
            let terrain = usize::from((w >> 2) & 0x1F);
            let scene_type = usize::from(w >> 11);
            if scene_type >= num_scene_type(region, terrain) {
                continue;
            }
            let count = scene_count(region, terrain, scene_type);
            if count == 0 {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            // LINT-OK: vertex coordinates, at most 2040.
            let (ix, iy) = (gx as i32, gy as i32);
            let Some(id) = get_scene(region, terrain, scene_type, scene_index(ix, iy, count))
            else {
                continue;
            };
            let Some(scene) = read.entry(id).or_insert_with(|| scenes(id)).as_ref() else {
                continue;
            };
            // The slope of the cell the vertex places its objects over.
            let h = height(gx, gy);
            let (sx, sy) = (
                (height(gx + 1, gy) - h) / 24.0,
                (height(gx, gy + 1) - h) / 24.0,
            );
            let up = 1.0 / (1.0 + sx * sx + sy * sy).sqrt();
            let (mut area, mut tall) = (0.0f32, 0.0f32);
            let mut tint = [0.0f32; 3];
            for obj in &scene.objects {
                if obj.weenie_obj != 0 || !check_slope(obj, up) {
                    continue;
                }
                let Some(m) = models.get(&obj.obj_id.0) else {
                    continue;
                };
                let scale = if obj.min_scale == obj.max_scale {
                    obj.max_scale
                } else {
                    (obj.min_scale * obj.max_scale).max(0.0).sqrt()
                };
                let crown =
                    std::f32::consts::PI * (m.reach * scale).powi(2) * obj.freq.clamp(0.0, 1.0);
                area += crown;
                tall += crown * m.height * scale;
                for (t, c) in tint.iter_mut().zip(m.colour) {
                    *t += crown * c;
                }
            }
            if area <= 0.0 {
                continue;
            }
            let share = (area / CELL_AREA).min(1.0);
            let k = gy * side + gx;
            for (c, t) in tint.iter().enumerate() {
                colour[k * 4 + c] = to_srgb(t / area);
            }
            // LINT-OK: a share 0..1 scaled to 0..255 and rounded.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                colour[k * 4 + 3] = (share * 255.0 + 0.5) as u8;
                canopy[k] = (share * tall / area / HEIGHT_STEP + 0.5).min(255.0) as u8;
            }
        }
    }
    Cover {
        colour,
        canopy,
        made_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

/// What the forest cover takes of each model of `pack` that has far pictures, by its retail
/// object: its crown's reach and height from its pictures' quad, and its pictures' mean colour,
/// coverage weighted, toned as the pack tones it.
pub(crate) fn cover_models(pack: &crate::art_pack::ArtPack) -> HashMap<u32, CoverModel> {
    const FLOATS: usize = 16;
    let mut out = HashMap::new();
    for m in &pack.models {
        let Some(lod) = m.lods.iter().rev().find(|l| l.views.is_some()) else {
            continue;
        };
        let Some(p) = lod.primitives.first() else {
            continue;
        };
        let (mut half, mut top) = (0.0f32, 0.0f32);
        for v in p.vertices.as_chunks::<FLOATS>().0 {
            half = half.max(v[0].abs());
            top = top.max(v[2]);
        }
        let Some(image) = p
            .material
            .base_color_image
            .and_then(|i| pack.images.get(i as usize))
        else {
            continue;
        };
        // A level a few hundred texels across holds the mean as well as the largest.
        let k = (0..image.levels.len())
            .find(|k| (image.width >> k) <= 256)
            .unwrap_or(image.levels.len() - 1);
        let (w, h) = ((image.width >> k).max(1), (image.height >> k).max(1));
        let texels = match image.format {
            Some(f) => dereth_render::block_codec::decode(f, w, h, &image.levels[k]),
            None => image.levels[k].to_vec(),
        };
        let (mut sum, mut cover) = ([0.0f64; 3], 0.0f64);
        for t in texels.as_chunks::<4>().0 {
            let a = f64::from(t[3]) / 255.0;
            for (s, v) in sum.iter_mut().zip(&t[..3]) {
                let c = f32::from(*v) / 255.0;
                let lin = f64::from(if c <= 0.04045 {
                    c / 12.92
                } else {
                    dereth_primitives::num::math::powf((c + 0.055) / 1.055, 2.4)
                });
                *s += lin * a;
            }
            cover += a;
        }
        if cover <= 0.0 {
            continue;
        }
        let f = p.material.base_color;
        // LINT-OK: a colour's mean, 0..1, kept to a float's precision.
        #[allow(clippy::cast_possible_truncation)]
        let colour = [0, 1, 2].map(|c| (sum[c] / cover) as f32 * f[c]);
        // The crown's reach: most of the quad's half width (its widest leaves, not its mean).
        out.insert(
            m.id.0,
            CoverModel {
                reach: half * 0.8,
                height: top,
                colour,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    #[test]
    fn a_linear_colour_is_written_in_the_displays_encoding() {
        assert_eq!(to_srgb(0.0), 0);
        assert_eq!(to_srgb(1.0), 255);
        // Mid grey in light is 188 in the display's encoding.
        assert!((187..=189).contains(&to_srgb(0.5)));
        assert_eq!(to_srgb(2.0), 255);
    }
}
