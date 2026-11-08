//! The ground's surface: retail's per-vertex normals of a block's drawn grid, and each drawn cell's
//! picture.

use super::{Kind, TextureKey};
use crate::scene::Terrain;

/// How short a summed normal may be before it is taken as straight up.
const SHORT: f32 = 2.0e-4;

/// The most overlays one cell's splat blends.
pub const SPLAT_OVERLAYS: usize = 5;

/// One ground cell's picture as retail composites it, kept as its layers so they are blended per
/// pixel: the base tile, repeated `base_tiling` times across the cell, then each overlay in turn.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Splat {
    pub base: Option<TextureKey>,
    pub base_tiling: u32,
    /// The terrain type the base's picture is of, when it is of one.
    pub base_ground: Option<u32>,
    /// The base's normal and surface pictures, when a pack's ground brings them.
    pub base_maps: Option<GroundMaps>,
    /// The retail picture the pack's base replaces, for its broad patches.
    pub base_broad: Option<BroadPicture>,
    pub overlays: Vec<SplatOverlay>,
    /// What each layer lays on steep ground, the base's first, then each overlay's in turn; empty
    /// where nothing is laid by the slope.
    pub slopes: Vec<LayerSlopes>,
    /// What a pack's ground lays by the water; `None` where no pack's ground is drawn.
    pub bank: Option<SplatBank>,
}

/// A retail ground's picture, kept under the pack's material that replaces it: the material is laid
/// out in its broad patches of light, dark and color, so the ground keeps its character from afar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BroadPicture {
    pub picture: TextureKey,
    /// Its repeats across a cell.
    pub tiling: u32,
}

/// What a pack's ground lays by the water, while the water is drawn over its bed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SplatBank {
    /// The grass grown in over a muddy bank.
    pub grass: SteepTile,
    /// The land beside the water, laid in place of retail's water ground at its edge and above it,
    /// and whether that land is mud.
    pub land: SteepTile,
    pub land_is_mud: bool,
    /// The bed well inside the water.
    pub bed: SteepTile,
}

/// What a pack's ground lays in a layer's place on steep ground.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct LayerSlopes {
    /// Where it is steep, and where it is steeper; `None` where the layer is not the pack's.
    pub scree: Option<SteepTile>,
    pub rock: Option<SteepTile>,
    /// Whether the layer is snow or ice, which keep most of their own look.
    pub snow: bool,
}

/// A picture laid on steep ground, its maps, and whether it is the ground's own material for its
/// slopes rather than the generic scree or rock.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SteepTile {
    pub tile: TextureKey,
    pub tiling: u32,
    pub maps: Option<GroundMaps>,
    pub own: bool,
}

/// A ground layer's maps beyond its picture, laid as its picture is: a normal picture (its red and
/// green the normal's lean along the picture's first coordinate and up its rows) and a surface
/// picture (its red the roughness), each optional, and how strongly the normal is bent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroundMaps {
    pub normal: Option<TextureKey>,
    pub surface: Option<TextureKey>,
    /// The normal's strength, in thousandths: 1000 as it was made.
    pub normal_scale: u32,
}

/// One layer of a [`Splat`]: where its alpha map keeps what is beneath it (alpha 1) and where it
/// lays its tile (alpha 0). The alpha map spans the cell once, turned by `rotation` quarter turns;
/// the tile repeats `tiling` times across the cell. A missing tile lays retail's debug green.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SplatOverlay {
    pub alpha: TextureKey,
    pub rotation: u8,
    pub tile: Option<TextureKey>,
    pub tiling: u32,
    /// The terrain type the tile's picture is of, when it is of one (the road's for a road).
    pub ground: Option<u32>,
    /// The tile's normal and surface pictures, when a pack's ground brings them.
    pub maps: Option<GroundMaps>,
    /// The retail picture the pack's tile replaces, for its broad patches.
    pub broad: Option<BroadPicture>,
}

/// The splat a ground cell's merge key draws: the key without its detail level, which changes the
/// composite's size and nothing of its layers.
#[must_use]
pub const fn splat_key(merge_key: u32) -> u32 {
    merge_key & 0x0FFF_FFFF
}

/// One drawn ground cell: its picture, the quarter turns its corners' texture coordinates are
/// rotated by, and its kind (water when every corner is).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TerrainCell {
    pub texture: TextureKey,
    pub rotation: u8,
    pub kind: Kind,
}

/// The terrain words at the four corners of drawn cell `(i, j)` of a grid of `side` cells, SW,
/// SE, NE and NW, from the block's full 9 × 9 words.
#[must_use]
pub fn cell_corners(words: &[u16; 81], side: usize, i: usize, j: usize) -> [u16; 4] {
    let step = 8 / side.clamp(1, 8);
    let at = |x: usize, y: usize| words.get(x * step * 9 + y * step).copied().unwrap_or(0);
    [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)]
}

/// One block's ground surface.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainSurface {
    /// One per vertex of the block's [`Terrain`], in its order: the normal retail lights the
    /// vertex with.
    pub normals: Vec<[f32; 3]>,
    /// One per drawn cell, in the terrain's cell order.
    pub cells: Vec<TerrainCell>,
}

/// The two triangles cell `(i, j)` of a grid of `side` cells is drawn as, as vertex indices: SW,
/// SE, NE and NW of a cell cut from its south-west corner to its north-east one, and SW, SE, NW,
/// NE, NW, SE of one cut the other way, each wound counter-clockwise seen from above.
#[must_use]
pub fn cell_triangles(side: usize, i: usize, j: usize, cut: bool) -> [[usize; 3]; 2] {
    let v = |a: usize, b: usize| a * (side + 1) + b;
    let (sw, se, ne, nw) = (v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1));
    if cut {
        [[sw, se, ne], [sw, ne, nw]]
    } else {
        [[sw, se, nw], [ne, nw, se]]
    }
}

/// A triangle's unit normal: `(b − a) × (c − a)`, divided by its length; straight up for a
/// triangle with no area.
fn plane_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        let inv = 1.0 / len;
        [n[0] * inv, n[1] * inv, n[2] * inv]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Retail's per-vertex normals of a block's drawn grid: at each vertex the sum of the unit normals
/// of the triangles that use it, within the block alone, divided by its length (straight up when
/// it is shorter than 0.0002). Every triangle weighs the same, whatever its area, and a vertex on
/// the block's edge sees only the block's own triangles, as retail lights it.
#[must_use]
pub fn normals(t: &Terrain) -> Vec<[f32; 3]> {
    let side = usize::from(t.side);
    let at = |i: usize| -> [f32; 3] { t.vertices.get(i).map_or([0.0; 3], |v| [v.x, v.y, v.z]) };
    let mut sum = vec![[0.0f32; 3]; t.vertices.len()];
    for i in 0..side {
        for j in 0..side {
            let cut = t.cuts.get(i * side + j).copied().unwrap_or(false);
            for tri in cell_triangles(side, i, j, cut) {
                let n = plane_normal(at(tri[0]), at(tri[1]), at(tri[2]));
                for &k in &tri {
                    if let Some(s) = sum.get_mut(k) {
                        s[0] += n[0];
                        s[1] += n[1];
                        s[2] += n[2];
                    }
                }
            }
        }
    }
    sum.into_iter()
        .map(|s| {
            let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
            if len < SHORT {
                [0.0, 0.0, 1.0]
            } else {
                let inv = 1.0 / len;
                [s[0] * inv, s[1] * inv, s[2] * inv]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use glam::Vec3;

    fn grid(side: u8, height: impl Fn(usize, usize) -> f32, cut: bool) -> Terrain {
        let n = usize::from(side);
        let step = 192.0 / f32::from(side);
        let mut vertices = Vec::new();
        for i in 0..=n {
            for j in 0..=n {
                #[allow(clippy::cast_precision_loss)] // grid indices, at most 8
                vertices.push(Vec3::new(i as f32 * step, j as f32 * step, height(i, j)));
            }
        }
        Terrain {
            side,
            vertices,
            cuts: vec![cut; n * n],
            words: [0; 81],
        }
    }

    /// **Flat ground's normals are straight up, and a uniform slope's are its plane's normal at
    /// every vertex**, inside the block and on its edges, whichever way the cells are cut.
    #[test]
    fn flat_and_evenly_sloped_ground_has_its_planes_normal_everywhere() {
        for cut in [false, true] {
            for n in normals(&grid(8, |_, _| 12.0, cut)) {
                assert_eq!(n, [0.0, 0.0, 1.0]);
            }
            // A rise of 6 m a 24 m cell eastward: the normal leans west.
            let slope = grid(
                8,
                |i, _| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = i as f32;
                    x * 6.0
                },
                cut,
            );
            let want = Vec3::new(-6.0, 0.0, 24.0).normalize();
            for n in normals(&slope) {
                let d = Vec3::from(n) - want;
                assert!(d.length() < 1e-6, "{n:?} against {want:?}");
            }
        }
    }

    /// **Each triangle weighs the same, whatever its size, and the block's edge sees only the
    /// block**: a single raised vertex in a corner leans the corner's normal by its one or two
    /// triangles alone.
    #[test]
    fn each_incident_triangle_counts_once_and_the_block_edge_sees_only_its_own() {
        let t = grid(1, |i, j| if (i, j) == (1, 1) { 24.0 } else { 0.0 }, true);
        let n = normals(&t);
        // Cut from SW to NE, the north-east corner is in both triangles; the south-east one is in
        // the first alone, whose plane rises toward the north-east corner.
        let tri0 = plane_normal([0.0, 0.0, 0.0], [192.0, 0.0, 0.0], [192.0, 192.0, 24.0]);
        assert_eq!(n[2], tri0, "the south-east corner takes its one triangle");
        let tri1 = plane_normal([0.0, 0.0, 0.0], [192.0, 192.0, 24.0], [0.0, 192.0, 0.0]);
        let sum = Vec3::from(tri0) + Vec3::from(tri1);
        let both = sum / sum.length();
        assert!((Vec3::from(n[3]) - both).length() < 1e-7);
        assert!((Vec3::from(n[0]) - both).length() < 1e-7);
    }
}
