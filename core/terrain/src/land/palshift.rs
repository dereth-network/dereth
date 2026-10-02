//! Palette-shift land texturing: the land surface of the software region a dat set from before
//! Throne of Destiny carries (its hardware region texture-merges).
//!
//! The region names a few 256-colour textures. Each texture's palette is cut into ranges, and a
//! cell is drawn with one of those textures whose ranges are refilled from the palettes of the
//! terrain types at the cell's four corners (and the road's): the picture's regions take the
//! corners' colours, so one image blends any four terrains. Which range takes which corner is the
//! texture's entry for the cell's road pattern, and the cell may be drawn rotated so that a
//! pattern the texture lists is found.
//!
//! The record layout is in `docs/formats/40-before-throne-of-destiny.md`.

use dereth_assets::region::{PalShift, PalShiftTexture};
use dereth_primitives::DataId;

use crate::consts::ROAD_TERRAIN_TYPE;
use crate::land::merge::MergeKey;
use crate::land::mesh::Rotation;

/// Where each corner's terrain type sits in a key: SW, SE, NE, NW.
const TERRAIN_SHIFT: [u32; 4] = [15, 10, 5, 0];

/// The palette-shift choice for one cell: the texture, the rotation it is drawn at, the texture's
/// road-pattern entry, the cell's key at that rotation (the surface's cache key) and the palettes
/// of the four unrotated corners (SW, SE, NE, NW) and of the road.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PalShiftChoice {
    pub key: MergeKey,
    pub rotation: Rotation,
    pub texture: usize,
    pub road: usize,
    pub palettes: [DataId; 5],
}

/// The cell's pseudo-random first texture: the landscape's position hash scaled to the texture
/// count.
#[must_use]
pub fn first_texture(gx: i32, gy: i32, count: usize) -> usize {
    let (x, y) = (gx.cast_unsigned(), gy.cast_unsigned());
    let v = y
        .wrapping_mul(0x6C1A_C587)
        .wrapping_sub(x.wrapping_mul(y.wrapping_mul(0x622D_BEDF).wrapping_add(0x421B_E3BD)))
        .wrapping_sub(0x791C_2B27);
    let count = u64::try_from(count).unwrap_or(0);
    usize::try_from((u64::from(v) * count) >> 32).unwrap_or(0)
}

/// The rotation to try first: the one whose key is smallest (the first of equals).
#[must_use]
pub fn first_rotation(keys: &[MergeKey; 4]) -> usize {
    let mut best = 0;
    for (i, k) in keys.iter().enumerate().skip(1) {
        if k.0 < keys[best].0 {
            best = i;
        }
    }
    best
}

/// The four-bit road pattern of a key: a bit per corner that has a road, SW the highest.
#[must_use]
pub fn road_pattern(key: MergeKey) -> u32 {
    let roads = (key.0 >> 20) & 0xFF;
    ((roads >> 6 & 3 != 0) as u32) << 3
        | ((roads >> 4 & 3 != 0) as u32) << 2
        | ((roads >> 2 & 3 != 0) as u32) << 1
        | (roads & 3 != 0) as u32
}

/// The palettes of `key`'s four corners (SW, SE, NE, NW) and, when it has a road, of the road, as
/// `texture` lists them; `None` when the texture has no palette for one of them.
fn corner_palettes(texture: &PalShiftTexture, key: MergeKey) -> Option<[DataId; 5]> {
    let palette_of = |t: u32| {
        texture
            .terrain_palettes
            .iter()
            .find(|(terrain, _)| *terrain == t)
            .map(|&(_, p)| p)
    };
    let mut out = [DataId(0); 5];
    for (k, shift) in TERRAIN_SHIFT.iter().enumerate() {
        out[k] = palette_of((key.0 >> shift) & 0x1F)?;
    }
    if road_pattern(key) != 0 {
        out[4] = palette_of(ROAD_TERRAIN_TYPE)?;
    }
    Some(out)
}

/// Choose the texture and rotation for a cell whose four rotated keys are `keys` (in rotation
/// order, as the cell rotation computes them), at global cell `(gx, gy)`.
///
/// From the cell's first texture, each texture in turn that has a palette for all four corners
/// (and for the road, when there is one) is tried at each rotation from the first, and the first
/// rotation whose road pattern the texture lists is taken. When none is, the cell is drawn with the
/// first texture unrotated.
#[must_use]
pub fn select(ps: &PalShift, keys: &[MergeKey; 4], gx: i32, gy: i32) -> PalShiftChoice {
    let count = ps.textures.len();
    let start = if count == 0 {
        0
    } else {
        first_texture(gx, gy, count)
    };
    let begin = first_rotation(keys);
    for step in 0..count {
        let t = (start + step) % count;
        let texture = &ps.textures[t];
        let Some(palettes) = corner_palettes(texture, keys[0]) else {
            continue;
        };
        for r in 0..4 {
            let rot = (begin + r) % 4;
            let pattern = road_pattern(keys[rot]);
            if let Some(road) = texture
                .road_maps
                .iter()
                .position(|m| m.road_code == pattern)
            {
                return PalShiftChoice {
                    key: keys[rot],
                    rotation: Rotation::ALL[rot],
                    texture: t,
                    road,
                    palettes,
                };
            }
        }
    }
    PalShiftChoice {
        key: keys[0],
        rotation: Rotation::Rot0,
        texture: 0,
        road: 0,
        palettes: ps
            .textures
            .first()
            .and_then(|t| corner_palettes(t, keys[0]))
            .unwrap_or([DataId(0); 5]),
    }
}

/// The sub-palettes that make a choice's palette: for each of the texture's ranges, `(palette,
/// first index, length)`, the palette being the corner (counted from the rotation) or the road
/// that the texture's road-pattern entry names for the range.
#[must_use]
pub fn sub_palettes(ps: &PalShift, choice: &PalShiftChoice) -> Vec<(DataId, u32, u32)> {
    let Some(texture) = ps.textures.get(choice.texture) else {
        return Vec::new();
    };
    let r = choice.rotation.index();
    let by_type = [
        choice.palettes[r % 4],
        choice.palettes[(r + 1) % 4],
        choice.palettes[(r + 2) % 4],
        choice.palettes[(r + 3) % 4],
        choice.palettes[4],
    ];
    let Some(road) = texture.road_maps.get(choice.road) else {
        return Vec::new();
    };
    texture
        .sub_palettes
        .iter()
        .zip(&road.sub_palette_types)
        .map(|(&(index, length), kind)| {
            let palette = usize::try_from(*kind)
                .ok()
                .and_then(|k| by_type.get(k))
                .copied()
                .unwrap_or(DataId(0));
            (palette, index, length)
        })
        .collect()
}

/// The palette a choice draws with: `base` (the texture's own palette, 32-bit ARGB) with each
/// sub-palette's range refilled from the same indices of its palette. `palette` answers a
/// palette's colours, or `None` for one that is missing, whose range then keeps the base colours.
#[must_use]
pub fn compose_palette(
    base: &[u32],
    subs: &[(DataId, u32, u32)],
    palette: &dyn Fn(DataId) -> Option<Vec<u32>>,
) -> Vec<u32> {
    let mut out = base.to_vec();
    for &(id, index, length) in subs {
        let Some(colours) = palette(id) else {
            continue;
        };
        let start = index as usize;
        let end = ((index + length) as usize)
            .min(out.len())
            .min(colours.len());
        if start < end {
            out[start..end].copy_from_slice(&colours[start..end]);
        }
    }
    out
}

/// Expand 8-bit indices through a 32-bit ARGB palette into opaque B, G, R, A bytes.
#[must_use]
pub fn expand(indices: &[u8], palette: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(indices.len() * 4);
    for &i in indices {
        let argb = palette.get(usize::from(i)).copied().unwrap_or(0);
        let [b, g, r, _] = argb.to_le_bytes();
        out.extend_from_slice(&[b, g, r, 0xFF]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_assets::region::PalShiftRoad;

    fn texture(roads: &[u32], terrains: &[u32]) -> PalShiftTexture {
        PalShiftTexture {
            tex_gid: DataId(0x0500_0001),
            sub_palettes: vec![(0, 2), (2, 2), (4, 2), (6, 2), (8, 2)],
            road_maps: roads
                .iter()
                .map(|&road_code| PalShiftRoad {
                    road_code,
                    sub_palette_types: vec![0, 1, 2, 3, 4],
                })
                .collect(),
            terrain_palettes: terrains
                .iter()
                .map(|&t| (t, DataId(0x0400_0100 + t)))
                .collect(),
        }
    }

    fn keys(types: [u16; 4], roads: [u16; 4]) -> [MergeKey; 4] {
        let k = |r: usize| {
            MergeKey::new(
                [0, 1, 2, 3].map(|i| types[(r + i) % 4]),
                [0, 1, 2, 3].map(|i| roads[(r + i) % 4]),
                1,
            )
        };
        [k(0), k(1), k(2), k(3)]
    }

    /// The position hash matches the landscape's: the same `(x, y)` always picks the same first
    /// texture, and over a block of cells every texture is first somewhere.
    #[test]
    fn the_first_texture_is_a_position_hash_over_the_texture_count() {
        assert_eq!(first_texture(10, 20, 2), first_texture(10, 20, 2));
        let mut seen = [false; 3];
        for x in 0..16 {
            for y in 0..16 {
                seen[first_texture(x, y, 3)] = true;
            }
        }
        assert_eq!(seen, [true; 3]);
        assert_eq!(first_texture(5, 5, 1), 0);
    }

    /// A cell with a road on its SW corner only is drawn at the rotation that brings the road to
    /// the corner the texture's one-road pattern names, with that rotation's key.
    #[test]
    fn a_road_pattern_the_texture_lacks_is_found_by_rotating_the_cell() {
        let ps = PalShift {
            textures: vec![texture(&[0, 0b0001], &[1, 2, ROAD_TERRAIN_TYPE])],
        };
        // Road on SW only: pattern 0b1000, which the texture does not list; rotated so that SW is
        // last, it is 0b0001.
        let k = keys([1, 1, 2, 2], [1, 0, 0, 0]);
        let c = select(&ps, &k, 3, 4);
        assert_eq!(road_pattern(c.key), 0b0001);
        assert_eq!(c.rotation, Rotation::Rot90);
        assert_eq!(c.key, k[1]);
        assert_eq!(c.road, 1);
        assert_eq!(c.palettes[0], DataId(0x0400_0101), "SW is terrain 1");
        assert_eq!(c.palettes[2], DataId(0x0400_0102), "NE is terrain 2");
        assert_eq!(c.palettes[4], DataId(0x0400_0120), "the road's palette");
    }

    /// A texture without a palette for one of the corners is passed over.
    #[test]
    fn a_texture_missing_a_corners_palette_is_passed_over() {
        let ps = PalShift {
            textures: vec![texture(&[0], &[1]), texture(&[0], &[1, 7])],
        };
        let c = select(&ps, &keys([1, 7, 1, 7], [0; 4]), 0, 0);
        assert_eq!(c.texture, 1);
    }

    /// The composed palette takes each range from the palette of the corner the road entry names,
    /// counted from the rotation; indices outside every range keep the base colours.
    #[test]
    fn each_range_takes_its_corners_colours_from_the_same_indices() {
        let ps = PalShift {
            textures: vec![texture(&[0], &[1, 2, 3, 4])],
        };
        let choice = PalShiftChoice {
            key: MergeKey(0),
            rotation: Rotation::Rot90,
            texture: 0,
            road: 0,
            palettes: [
                DataId(0x0400_0101),
                DataId(0x0400_0102),
                DataId(0x0400_0103),
                DataId(0x0400_0104),
                DataId(0x0400_0120),
            ],
        };
        let subs = sub_palettes(&ps, &choice);
        // Range 0 takes type 0, which at ROT_90 is the SE corner's palette.
        assert_eq!(subs[0], (DataId(0x0400_0102), 0, 2));
        assert_eq!(subs[3], (DataId(0x0400_0101), 6, 2));
        let base = vec![0xFF00_0000; 12];
        let colour_of = |id: DataId| Some(vec![id.raw(); 12]);
        let pal = compose_palette(&base, &subs, &colour_of);
        assert_eq!(pal[0], 0x0400_0102);
        assert_eq!(pal[7], 0x0400_0101);
        assert_eq!(pal[10], 0xFF00_0000, "outside every range");
        assert_eq!(
            expand(&[0, 10], &[0x0011_2233; 11]),
            [0x33, 0x22, 0x11, 0xFF, 0x33, 0x22, 0x11, 0xFF]
        );
    }
}
