//! Terrain types a region draws no picture of its own for.
//!
//! The cells number their terrain types the same way in every era, but a region need not have a
//! picture of its own for every number a world's cells use. Its land surface lists a row (or a
//! palette) for every type from 0 to 31, but a type past the end of the region's own terrain list
//! is a filler there: the region before Throne of Destiny names 27 types in 1999 and 31 from Dark
//! Majesty on, and its land surface draws each type it does not name with a stand-in (the
//! hardware rows with Argila's tile, the software textures with a stand-in palette). The
//! end-of-retail region names 32, the last being Desolate Lands. A world of 1999 uses type 31 under
//! a grid of roads over the sea, which its region does not name, and the end-of-retail world uses
//! it at three vertices.
//!
//! Drawn with a region that does not name it, such a vertex takes the terrain type most common
//! among its neighbours that the region does draw: the ground around it continues across it
//! rather than showing the filler.
//!
//! Only the ground's look changes. The heights, the water, the scenery and everything else that
//! reads a terrain word keep the cell's own word.

use dereth_assets::region::LandSurf;
use dereth_assets::world::CellLandblock;

use crate::consts::SIDE_VERTEX_COUNT;

/// The terrain-type bits of a terrain word (bits 2-6).
const TYPE_BITS: u16 = 0x007C;

/// The terrain types (`0..32`) a region draws with a picture of its own, one bit each: every type
/// below `named` (the length of the region's terrain list) that `surf` has a texture-merge row or
/// a palette-shift palette for.
#[must_use]
pub fn drawn_terrain_types(surf: &LandSurf, named: usize) -> u32 {
    let mut mask = 0u32;
    let mut add = |t: u32| {
        if t < 32 && usize::try_from(t).is_ok_and(|t| t < named) {
            mask |= 1 << t;
        }
    };
    if let Some(tm) = &surf.tex_merge {
        for d in &tm.terrain_desc {
            add(d.terrain_type);
        }
    }
    if let Some(ps) = &surf.pal_shift {
        for t in &ps.textures {
            for (terrain, _) in &t.terrain_palettes {
                add(*terrain);
            }
        }
    }
    mask
}

/// `lb` with each vertex whose terrain type is not in `drawn` given the type most common among
/// its eight neighbours that is (the lower type on a tie), its road and scene bits kept. A vertex
/// none of whose neighbours is drawn takes the most common drawn type in the block, and keeps its
/// own type when the block has none. `None` when every vertex is already drawn, which is nearly
/// every block.
#[must_use]
pub fn fill_undrawn_terrain(lb: &CellLandblock, drawn: u32) -> Option<CellLandblock> {
    let type_at = |x: usize, y: usize| u32::from(lb.terrain_type(x, y));
    let is_drawn = |t: u32| t < 32 && drawn & (1 << t) != 0;
    let side = SIDE_VERTEX_COUNT;
    if (0..side).all(|x| (0..side).all(|y| is_drawn(type_at(x, y)))) {
        return None;
    }
    // The most common drawn type among `cells`, the lower type on a tie.
    let majority = |cells: &mut dyn Iterator<Item = (usize, usize)>| -> Option<u32> {
        let mut counts = [0u32; 32];
        for (x, y) in cells {
            let t = type_at(x, y);
            if is_drawn(t) {
                counts[t as usize] += 1;
            }
        }
        let best = (0..32u32).max_by_key(|t| (counts[*t as usize], std::cmp::Reverse(*t)))?;
        (counts[best as usize] > 0).then_some(best)
    };
    let block = majority(&mut (0..side).flat_map(|x| (0..side).map(move |y| (x, y))));
    let mut out = lb.clone();
    for x in 0..side {
        for y in 0..side {
            if is_drawn(type_at(x, y)) {
                continue;
            }
            let mut around = (x.saturating_sub(1)..=(x + 1).min(side - 1))
                .flat_map(|nx| {
                    (y.saturating_sub(1)..=(y + 1).min(side - 1)).map(move |ny| (nx, ny))
                })
                .filter(|&(nx, ny)| (nx, ny) != (x, y));
            let Some(t) = majority(&mut around).or(block) else {
                continue;
            };
            let w = &mut out.terrain[x * side + y];
            // `t` is below 32, so it fits the five type bits.
            *w = (*w & !TYPE_BITS) | (u16::try_from(t).unwrap_or(0) << 2);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    fn block(types: impl Fn(usize, usize) -> u16) -> CellLandblock {
        let mut terrain = [0u16; 81];
        for x in 0..9 {
            for y in 0..9 {
                terrain[x * 9 + y] = types(x, y) << 2;
            }
        }
        CellLandblock {
            id: DataId(0xF930_FFFF),
            lbi_exists: 0,
            terrain,
            height: [0; 81],
        }
    }

    /// A vertex of a type the land surface lacks takes its neighbours' most common drawn type;
    /// its road and scene bits stay, and every other vertex is untouched.
    #[test]
    fn an_undrawn_type_takes_the_most_common_drawn_type_around_it() {
        // Forest floor (4) to the west and north, barren rock (2) elsewhere, type 31 at (4, 4).
        let mut lb = block(|x, y| if x < 4 || y > 4 { 4 } else { 2 });
        lb.terrain[4 * 9 + 4] = (31 << 2) | 0x2 | (5 << 11);
        let drawn = !(1u32 << 31);
        let filled = fill_undrawn_terrain(&lb, drawn).expect("one vertex filled");
        // Neighbours of (4, 4): x in 3..=5, y in 3..=5. x = 3 or y = 5 is forest floor: five of
        // the eight; the other three are barren rock.
        assert_eq!(filled.terrain_type(4, 4), 4);
        assert_eq!(filled.road(4, 4), 2);
        assert_eq!(filled.scene_type(4, 4), 5);
        for i in (0..81).filter(|&i| i != 4 * 9 + 4) {
            assert_eq!(filled.terrain[i], lb.terrain[i]);
        }
        // A tie goes to the lower type, and a block with nothing to fill is left alone.
        let mut tie = block(|_, _| 3);
        tie.terrain[4 * 9 + 4] = 31 << 2;
        for (x, y) in [(3, 3), (3, 4), (3, 5), (4, 3)] {
            tie.terrain[x * 9 + y] = 6 << 2;
        }
        let t = fill_undrawn_terrain(&tie, drawn).expect("filled");
        let around: Vec<u16> = [
            (3, 3),
            (3, 4),
            (3, 5),
            (4, 3),
            (4, 5),
            (5, 3),
            (5, 4),
            (5, 5),
        ]
        .iter()
        .map(|&(x, y)| tie.terrain_type(x, y))
        .collect();
        assert_eq!(around.iter().filter(|&&t| t == 3).count(), 4);
        assert_eq!(around.iter().filter(|&&t| t == 6).count(), 4);
        assert_eq!(t.terrain_type(4, 4), 3);
        assert!(fill_undrawn_terrain(&block(|_, _| 20), drawn).is_none());
    }

    /// Undrawn vertices do not vote, and a vertex with no drawn neighbour takes the block's most
    /// common drawn type.
    #[test]
    fn undrawn_neighbours_do_not_vote_and_an_island_takes_the_block_majority() {
        // A 3x3 patch of type 31 in a sea of deep water (20), with one shallow (16) vertex.
        let mut lb = block(|x, y| {
            if (3..=5).contains(&x) && (3..=5).contains(&y) {
                31
            } else {
                20
            }
        });
        lb.terrain[0] = 16 << 2;
        let drawn = !(1u32 << 31);
        let filled = fill_undrawn_terrain(&lb, drawn).expect("filled");
        for x in 3..=5 {
            for y in 3..=5 {
                assert_eq!(filled.terrain_type(x, y), 20, "({x}, {y})");
            }
        }
        // Every vertex undrawn: nothing to take, so the words stay.
        let all = block(|_, _| 31);
        assert_eq!(fill_undrawn_terrain(&all, drawn), Some(all.clone()));
    }

    /// The drawn set is the texture-merge rows' types, or the palette-shift textures' palettes,
    /// that the region names.
    #[test]
    fn the_drawn_types_are_the_ones_the_land_surface_has_pictures_for() {
        use dereth_assets::region::{PalShift, PalShiftTexture, TerrainDesc, TexMerge};
        let row = |t: u32| TerrainDesc {
            terrain_type: t,
            tex_gid: DataId(0x0500_0001),
            tex_tiling: 1,
            max_vert_bright: 0,
            min_vert_bright: 0,
            max_vert_saturate: 0,
            min_vert_saturate: 0,
            max_vert_hue: 0,
            min_vert_hue: 0,
            detail_tex_tiling: 0,
            detail_tex_gid: DataId(0),
        };
        let merge = LandSurf {
            surf_type: 0,
            tex_merge: Some(TexMerge {
                base_tex_size: 512,
                corner_terrain_maps: Vec::new(),
                side_terrain_maps: Vec::new(),
                road_maps: Vec::new(),
                terrain_desc: vec![row(0), row(4), row(31), row(32)],
            }),
            pal_shift: None,
        };
        assert_eq!(drawn_terrain_types(&merge, 32), 1 | 1 << 4 | 1 << 31);
        // A row past the end of the region's own terrain list is a filler, not a picture.
        assert_eq!(drawn_terrain_types(&merge, 31), 1 | 1 << 4);
        let shift = LandSurf {
            surf_type: 1,
            tex_merge: None,
            pal_shift: Some(PalShift {
                textures: vec![PalShiftTexture {
                    tex_gid: DataId(0),
                    sub_palettes: Vec::new(),
                    road_maps: Vec::new(),
                    terrain_palettes: vec![(2, DataId(0x0400_0001)), (20, DataId(0x0400_0002))],
                }],
            }),
        };
        assert_eq!(drawn_terrain_types(&shift, 27), 1 << 2 | 1 << 20);
        assert_eq!(drawn_terrain_types(&shift, 20), 1 << 2);
    }
}
