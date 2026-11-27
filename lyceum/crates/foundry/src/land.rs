//! A world's land grid, and the land of a world that sets a finer grid than its files have.
//!
//! **The grid.** A world's land is cut into vertices on one grid, its [`LandGrid`], which its
//! region record declares: a landblock is always 192 meters of eight 24-meter land cells, and each
//! cell side is cut into equal steps: 1, 2 or 3 of them (24, 12 or 8 meters apart) for a world that sets
//! its spacing, and up to 4 for one whose imports lay its land. Every place Lyceum reads
//! or writes a vertex (the packs' `[[vertex]]` rows, the brushes, a building's level ground, the
//! ground a static stands on) takes its shape from that grid: a world vertex is
//! `landblock * 8 * steps + local` both ways, and a landblock's vertices run from 0 to `8 * steps`.
//!
//! **A finer world.** A world that sets its land's spacing (`[land]` in `world.toml`) finer than
//! its files' is built over a refined copy of them ([`refine`]): every landblock carries the full
//! grid, its heights read off the files' own ground, its cell corners exactly the files', and the
//! region record says how many steps a cell has ([`refined_region`]). Refining is the first layer of
//! the world, laid under every pack as an import is, so a pack's edits land on the fine grid and
//! the overlay holds every landblock.
//!
//! - **Heights**: a vertex inside a land cell takes the height-table entry nearest the files'
//!   ground under it, so the ground keeps its shape to within half a table step.
//! - **Cuts**: each square is cut as the land cell holding it is, so a cell's two triangles keep
//!   their edge.
//! - **Terrain and scene**: each vertex takes the nearest corner's of its cell (south and west on a
//!   tie), so the cells' corners, which the scenery and the maps read, are unchanged.
//! - **Roads**: a vertex is a road where the files' road covers it, with the nearest road corner's
//!   value, and the region's road is [`road_width`] wide.

use dereth_assets::world::{CellLandblock, DenseLand, MAX_DENSE_STEPS};
use dereth_landscape::WorldGrid;
use dereth_primitives::land_grid::{on_road, LandGrid};

use crate::compile::Field;

/// The road half-width of a world with `steps` vertex steps a land cell: the files' 5 meters, but
/// never more than half a step, so a road vertex's road stays within the squares around it.
#[must_use]
#[allow(clippy::cast_precision_loss)] // 1 to 4 steps
pub fn road_width(steps: usize) -> f32 {
    let spacing = LandGrid::DEFAULT.cell_length / steps.max(1) as f32;
    LandGrid::DEFAULT.road_width.min(spacing / 2.0)
}

/// The grid of a world of `steps` vertex steps a land cell.
#[must_use]
pub fn grid_of_steps(steps: usize) -> LandGrid {
    LandGrid::with_steps(steps, road_width(steps))
}

/// The world grid of land grid `land`: 255 landblocks a side, as every world has.
#[must_use]
pub const fn world_grid(land: LandGrid) -> WorldGrid {
    WorldGrid {
        land,
        ..WorldGrid::DEFAULT
    }
}

/// Vertex `(x, y)` of landblock `lb`'s grid: its height index and its terrain word, or `None` off
/// the block.
#[must_use]
pub fn vertex(lb: &CellLandblock, x: usize, y: usize) -> Option<(u8, u16)> {
    let side = 8 * lb.vertex_steps() + 1;
    if x >= side || y >= side {
        return None;
    }
    let i = x * side + y;
    Some((*lb.grid_height().get(i)?, *lb.grid_terrain().get(i)?))
}

/// A terrain word's ground: its type (bits 2-6) and its variant (bits 7-10, which a retail land
/// leaves clear). A terrain value is the two together, the type plus 32 times the variant, so
/// setting a type alone clears the variant.
pub const GROUND_BITS: u16 = 0x07FC;

/// The highest terrain value: type 31, variant 15.
pub const GROUND_MAX: u16 = 0x1FF;

/// A terrain value's type, without its variant.
#[must_use]
pub fn ground_type(terrain: u16) -> u16 {
    terrain & 0x1F
}

/// The terrain value of type `ty` with variant `variant`.
#[must_use]
pub fn ground(ty: u16, variant: u16) -> u16 {
    (ty & 0x1F) | ((variant & 0xF) << 5)
}

/// The table a converted land names its grounds in, beside its overlay: `type variant name`, a
/// line each after a header.
pub const GROUND_NAMES: &str = "ground-names.tsv";

/// The names in a [`GROUND_NAMES`] table, by terrain value ([`ground`]).
///
/// # Errors
/// A line that is not a type, a variant and a name.
pub fn parse_ground_names(text: &str) -> Result<std::collections::BTreeMap<u16, String>, String> {
    let mut out = std::collections::BTreeMap::new();
    for (k, line) in text.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (Some(ty), Some(variant), Some(name)) = (
            cols.first()
                .and_then(|c| c.parse::<u16>().ok())
                .filter(|t| *t < 32),
            cols.get(1)
                .and_then(|c| c.parse::<u16>().ok())
                .filter(|v| *v < 16),
            cols.get(2).map(|n| n.trim()).filter(|n| !n.is_empty()),
        ) else {
            return Err(format!("line {}: not a type, a variant and a name", k + 1));
        };
        out.insert(ground(ty, variant), name.to_owned());
    }
    Ok(out)
}

/// The names the world's land gives its grounds ([`GROUND_NAMES`]): the last of `imports`
/// (overlay folders) that holds the table. Empty when none does, as for a land not converted from
/// AC2.
///
/// # Errors
/// A table that will not read or parse.
pub fn ground_names(
    imports: &[std::path::PathBuf],
) -> Result<std::collections::BTreeMap<u16, String>, String> {
    for dir in imports.iter().rev() {
        let path = dir.join(GROUND_NAMES);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                return parse_ground_names(&text).map_err(|e| format!("{}: {e}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Ok(std::collections::BTreeMap::new())
}

/// The grounds a land's region offers, in order, each its terrain value and its picture: each of
/// its `types` types, then the variants the region gives a picture of their own (its terrain
/// descriptions, each a type with its variant above bit 8, and a picture), under their type. A
/// description of no type (the road's) offers none.
#[must_use]
pub fn grounds<T: Copy>(
    types: usize,
    descs: impl IntoIterator<Item = (u32, T)>,
) -> Vec<(u16, Option<T>)> {
    let descs: Vec<(u32, T)> = descs.into_iter().collect();
    let mut out = Vec::new();
    for ty in 0..u32::try_from(types.min(32)).unwrap_or(32) {
        let picture = |variant: u32| {
            descs
                .iter()
                .find(|(t, _)| *t == ty | variant << 8)
                .map(|(_, p)| *p)
        };
        let ty16 = u16::try_from(ty).unwrap_or(0);
        out.push((ground(ty16, 0), picture(0)));
        for variant in 1..16u16 {
            if let Some(p) = picture(u32::from(variant)) {
                out.push((ground(ty16, variant), Some(p)));
            }
        }
    }
    out
}

/// Vertex `(x, y)`'s fine height byte on landblock `lb`'s finer grid; `None` on a 9 × 9 block, whose
/// vertices stand at their height-table entries.
#[must_use]
pub fn fine(lb: &CellLandblock, x: usize, y: usize) -> Option<u8> {
    let d = lb.dense.as_ref()?;
    d.fine.get(x * d.side() + y).copied()
}

/// Set one field of vertex `(x, y)` of landblock `lb`'s grid. A height set on a finer grid stands
/// at its table entry: the vertex's fine height is cleared. A vertex at a land cell corner is set
/// in the corner arrays too, which the readers that work per land cell read.
pub fn set(lb: &mut CellLandblock, x: usize, y: usize, field: Field, value: u16) {
    let steps = lb.vertex_steps();
    let side = 8 * steps + 1;
    if x >= side || y >= side {
        return;
    }
    let apply = |height: &mut u8, word: &mut u16| match field {
        Field::Height => *height = u8::try_from(value).unwrap_or(u8::MAX),
        Field::Road => *word = (*word & !0x0003) | (value & 0x3),
        Field::Terrain => *word = (*word & !GROUND_BITS) | ((value & GROUND_MAX) << 2),
        Field::Scene => *word = (*word & !0xF800) | ((value & 0x1F) << 11),
    };
    if let Some(d) = lb.dense.as_mut() {
        let i = x * side + y;
        apply(&mut d.height[i], &mut d.terrain[i]);
        if field == Field::Height {
            d.fine[i] = 0;
        }
    }
    if x.is_multiple_of(steps) && y.is_multiple_of(steps) {
        let i = (x / steps) * 9 + y / steps;
        apply(&mut lb.height[i], &mut lb.terrain[i]);
    }
}

/// Landblock `lb` (of the files, at one vertex a land cell corner) on a grid of `steps` vertex
/// steps a land cell, as the module says, its heights in the region's height `table`; the landblock itself when `steps` is 1 or it already
/// carries a grid. `landblock` is its id's high half, which places its cells' cuts.
#[must_use]
pub fn refine(lb: &CellLandblock, table: &[f32], landblock: u16, steps: usize) -> CellLandblock {
    if steps <= 1 || steps > MAX_DENSE_STEPS || lb.dense.is_some() {
        return lb.clone();
    }
    let (bx, by) = (i32::from(landblock >> 8), i32::from(landblock & 0xFF));
    let side = 8 * steps + 1;
    let squares = 8 * steps;
    #[allow(clippy::cast_precision_loss)] // 1 to 4 steps
    let step = 1.0 / steps as f32;
    let corner_z = |cx: usize, cy: usize| table[usize::from(lb.height[cx * 9 + cy])];
    let base = LandGrid::DEFAULT;
    let road_square = base.cell_length;
    let mut height = vec![0u8; side * side];
    let mut terrain = vec![0u16; side * side];
    for x in 0..side {
        for y in 0..side {
            let (cx, cy) = ((x / steps).min(7), (y / steps).min(7));
            // The point's place in its cell, 0 to 1 each way.
            #[allow(clippy::cast_precision_loss)] // within a cell
            let (u, v) = (
                (x - cx * steps) as f32 * step,
                (y - cy * steps) as f32 * step,
            );
            let i = x * side + y;
            let on_corner = x.is_multiple_of(steps) && y.is_multiple_of(steps);
            if on_corner {
                let c = (x / steps) * 9 + y / steps;
                height[i] = lb.height[c];
                terrain[i] = lb.terrain[c];
                continue;
            }
            // The cell's own triangle under the point.
            let (a, b, c, d) = (
                corner_z(cx, cy),
                corner_z(cx, cy + 1),
                corner_z(cx + 1, cy),
                corner_z(cx + 1, cy + 1),
            );
            let cut = dereth_terrain::land::mesh::sw_to_ne_cut(
                bx * 8 + i32::try_from(cx).unwrap_or(0),
                by * 8 + i32::try_from(cy).unwrap_or(0),
            );
            let z = if cut {
                // The edge runs south-west to north-east: a + u (c - a) + v (d - c) below it.
                if u >= v {
                    a + u * (c - a) + v * (d - c)
                } else {
                    a + v * (b - a) + u * (d - b)
                }
            } else if u + v <= 1.0 {
                a + u * (c - a) + v * (b - a)
            } else {
                d + (1.0 - u) * (b - d) + (1.0 - v) * (c - d)
            };
            height[i] = crate::brush::nearest_height(table, z);
            // The nearest corner's terrain and scene, south and west on a tie.
            let near = |t: f32| usize::from(t > 0.5);
            let (nx, ny) = (cx + near(u), cy + near(v));
            let mut word = lb.terrain[nx * 9 + ny] & !0x0003;
            // A road where the files' road covers the point, with the nearest road corner's value.
            #[allow(clippy::cast_precision_loss)] // within a block
            let (px, py) = ((cx as f32 + u) * road_square, (cy as f32 + v) * road_square);
            if on_road(&lb.terrain, 9, road_square, base.road_width, px, py) {
                let corners = [(cx, cy), (cx, cy + 1), (cx + 1, cy), (cx + 1, cy + 1)];
                let nearest = corners
                    .iter()
                    .filter(|(kx, ky)| lb.terrain[kx * 9 + ky] & 3 != 0)
                    .min_by(|p, q| {
                        let dist = |(kx, ky): &(usize, usize)| {
                            #[allow(clippy::cast_precision_loss)] // within a block
                            let (dx, dy) = (kx.abs_diff(cx) as f32 - u, ky.abs_diff(cy) as f32 - v);
                            dx * dx + dy * dy
                        };
                        dist(p).total_cmp(&dist(q))
                    });
                word |= nearest.map_or(1, |(kx, ky)| lb.terrain[kx * 9 + ky] & 3);
            }
            terrain[i] = word;
        }
    }
    let cuts: Vec<bool> = (0..squares * squares)
        .map(|k| {
            let (sx, sy) = (k / squares, k % squares);
            dereth_terrain::land::mesh::sw_to_ne_cut(
                bx * 8 + i32::try_from(sx / steps).unwrap_or(0),
                by * 8 + i32::try_from(sy / steps).unwrap_or(0),
            )
        })
        .collect();
    let mut split = vec![0u8; DenseLand::split_bytes(steps)];
    for (k, cut) in cuts.iter().enumerate() {
        if *cut {
            split[k / 8] |= 1 << (k % 8);
        }
    }
    CellLandblock::from_dense(
        lb.id,
        lb.lbi_exists,
        DenseLand {
            steps,
            terrain: terrain.into(),
            height: height.into(),
            split: split.into(),
            fine: DenseLand::no_fine(steps),
        },
    )
}

/// The region record `bytes` with its land cut into `steps` vertex steps a cell and its roads
/// [`road_width`] wide, every other byte as it was. `None` when the bytes do not read as a region
/// whose name is where a region's is.
#[must_use]
pub fn refined_region(bytes: &[u8], steps: usize) -> Option<Vec<u8>> {
    // The id, the region's number and version, then its name: a length and the text, padded to
    // four bytes; then the land definitions, whose fifth word is the steps and eighth the road
    // width.
    let len = usize::from(u16::from_le_bytes(bytes.get(12..14)?.try_into().ok()?));
    let land = (12 + 2 + len).next_multiple_of(4);
    let mut out = bytes.to_vec();
    let width = road_width(steps);
    out.get_mut(land + 16..land + 20)?
        .copy_from_slice(&u32::try_from(steps).ok()?.to_le_bytes());
    out.get_mut(land + 28..land + 32)?
        .copy_from_slice(&width.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a world's land refined onto a finer grid)
    use super::*;

    fn block() -> CellLandblock {
        let mut lb = CellLandblock {
            id: dereth_primitives::DataId(0xA9B4_FFFF),
            lbi_exists: 1,
            terrain: [0; 81],
            height: [0; 81],
            dense: None,
        };
        for x in 0..9 {
            for y in 0..9 {
                lb.height[x * 9 + y] = u8::try_from(10 * x + y).unwrap_or(0);
                lb.terrain[x * 9 + y] = u16::try_from((x + y) % 32).unwrap_or(0) << 2;
            }
        }
        lb
    }

    #[test]
    fn a_ground_sets_the_type_and_variant_bits_and_a_plain_type_clears_the_variant() {
        let mut lb = block();
        // Road 2 and scene 9 under every vertex, to show they stay.
        for w in &mut lb.terrain {
            *w |= 2 | (9 << 11);
        }
        set(&mut lb, 2, 3, Field::Terrain, ground(12, 5));
        assert_eq!(lb.terrain[2 * 9 + 3], 2 | (12 << 2) | (5 << 7) | (9 << 11));
        set(&mut lb, 2, 3, Field::Terrain, 4);
        assert_eq!(lb.terrain[2 * 9 + 3], 2 | (4 << 2) | (9 << 11));
        assert_eq!(
            (ground_type(ground(31, 15)), ground(31, 15)),
            (31, GROUND_MAX)
        );
    }

    #[test]
    fn a_lands_grounds_are_its_types_each_followed_by_the_variants_it_has_pictures_for() {
        // Three types; type 1 with variants 2 and 1 (listed out of order), and the road's
        // description, of no type.
        let descs = [
            (0u32, 'a'),
            (1, 'b'),
            (1 | 2 << 8, 'd'),
            (1 | 1 << 8, 'c'),
            (2, 'e'),
            (32, 'r'),
        ];
        assert_eq!(
            grounds(3, descs),
            [
                (0, Some('a')),
                (1, Some('b')),
                (ground(1, 1), Some('c')),
                (ground(1, 2), Some('d')),
                (2, Some('e')),
            ]
        );
        // A land without variants: its types alone, one without a picture still offered.
        assert_eq!(grounds(2, [(0u32, 'a')]), [(0, Some('a')), (1, None)]);
    }

    /// A converted land's ground names are read by terrain value; a line that is not a type, a
    /// variant and a name is refused, and a land without the table names none.
    #[test]
    fn a_lands_ground_names_are_read_by_terrain_value() {
        let names = parse_ground_names(
            "type\tvariant\tname\n1\t0\tGrass land\n3\t1\tTropic bamboo\n3\t14\tTropic bamboo, steeper\n",
        )
        .unwrap();
        assert_eq!(names.len(), 3);
        assert_eq!(names[&1], "Grass land");
        assert_eq!(names[&ground(3, 1)], "Tropic bamboo");
        assert_eq!(names[&ground(3, 14)], "Tropic bamboo, steeper");
        for bad in [
            "type\tvariant\tname\n40\t0\tX\n",
            "type\tvariant\tname\n1\t0\n",
        ] {
            assert!(parse_ground_names(bad).is_err(), "{bad:?}");
        }
        assert!(ground_names(&[std::path::PathBuf::from("no such land")])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_road_is_never_wider_than_half_a_step() {
        assert_eq!([1, 2, 3, 4].map(road_width), [5.0, 5.0, 4.0, 3.0]);
        assert!(grid_of_steps(1).is_default());
        assert_eq!(grid_of_steps(3).vertex_spacing(), 8.0);
    }

    /// A height set on a finer grid stands at its table entry: that vertex's fine byte is cleared,
    /// and no other's; setting its terrain leaves it.
    #[test]
    fn a_height_set_on_a_finer_grid_clears_that_vertexs_fine_height() {
        let mut lb = block();
        lb.dense = Some(Box::new(DenseLand {
            steps: 2,
            terrain: vec![0; 17 * 17].into(),
            height: vec![0; 17 * 17].into(),
            split: vec![0; DenseLand::split_bytes(2)].into(),
            fine: vec![40; 17 * 17].into(),
        }));
        set(&mut lb, 3, 5, Field::Terrain, 4);
        assert_eq!(fine(&lb, 3, 5), Some(40));
        set(&mut lb, 3, 5, Field::Height, 9);
        assert_eq!(fine(&lb, 3, 5), Some(0));
        assert_eq!(fine(&lb, 3, 6), Some(40));
        assert_eq!(fine(&block(), 3, 5), None, "a 9 x 9 block has none");
    }

    #[test]
    fn a_vertex_is_set_on_the_full_grid_and_at_a_corner_in_the_corner_arrays_too() {
        let mut lb = block();
        lb.dense = Some(Box::new(DenseLand {
            steps: 2,
            terrain: vec![0; 17 * 17].into(),
            height: vec![0; 17 * 17].into(),
            split: vec![0; DenseLand::split_bytes(2)].into(),
            fine: DenseLand::no_fine(2),
        }));
        set(&mut lb, 3, 5, Field::Height, 77);
        assert_eq!(vertex(&lb, 3, 5), Some((77, 0)));
        assert_eq!(lb.height, block().height, "no corner moved");
        set(&mut lb, 4, 6, Field::Terrain, 9);
        assert_eq!(vertex(&lb, 4, 6), Some((0, 9 << 2)));
        assert_eq!(lb.terrain[2 * 9 + 3], 9 << 2);
        assert_eq!(vertex(&lb, 17, 0), None);
    }
}
