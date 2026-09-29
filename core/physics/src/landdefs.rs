//! `LandDefs` — cell-id encoding, global land-cell coordinates and the landblock offset.
//!
//! The cell id encoding, reproduced function by function against retail behaviour.
//!
//! A cell id is `block_x << 24 | block_y << 16 | cell_index`. Global land-cell coordinates
//! are in units of one 24 m cell and run `0 .. 0x7F8` (2040 = 255 blocks of 8).

use dereth_primitives::num::{floor_to_i32, to_i32_f64};
use dereth_primitives::{CellId, DataId, Vec3};

use crate::globals::{
    BLOCK_LENGTH, CELL_SIZE, LAND_HEIGHT_MAX, LAND_HEIGHT_TABLE_LEN, LCOORD_LIMIT,
};
use crate::math::V3;

/// A landblock's direction relative to the viewer block. [`heading`] returns the corresponding
/// angle in **radians**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Direction {
    InViewerBlock = 0,
    NorthOfViewer = 1,
    SouthOfViewer = 2,
    EastOfViewer = 3,
    WestOfViewer = 4,
    NorthwestOfViewer = 5,
    SouthwestOfViewer = 6,
    NortheastOfViewer = 7,
    SoutheastOfViewer = 8,
    Unknown = 9,
}

/// Heading for a relative block direction, in **radians**, from the client's switch table.
///
/// The literals are the floats the client's switch holds, not derived from `f32::consts`:
/// they are data, and substituting a "more accurate" constant would be substituting a value the
/// client does not have. Hence the `approx_constant` exemption.
#[allow(clippy::approx_constant)]
#[must_use]
pub fn heading(d: Direction) -> f32 {
    match d {
        Direction::SouthOfViewer => 3.141_592_7,
        Direction::EastOfViewer => 1.570_796_4,
        Direction::WestOfViewer => 4.712_389,
        Direction::NorthwestOfViewer => 5.497_787,
        Direction::SouthwestOfViewer => 3.926_990_7,
        Direction::NortheastOfViewer => 0.785_398_2,
        Direction::SoutheastOfViewer => 2.356_194_5,
        _ => 0.0,
    }
}

/// Convert a signed block delta to a [`Direction`].
#[must_use]
pub fn get_dir(dx: i32, dy: i32) -> Direction {
    if dx < 0 {
        if dy < 0 {
            return Direction::SouthwestOfViewer;
        }
        // WestOfViewer + (dy > 0) == west or northwest
        return if dy > 0 {
            Direction::NorthwestOfViewer
        } else {
            Direction::WestOfViewer
        };
    }
    if dx < 1 {
        if dy < 0 {
            return Direction::SouthOfViewer;
        }
        // 0 or 1 == InViewerBlock or NorthOfViewer
        return if dy > 0 {
            Direction::NorthOfViewer
        } else {
            Direction::InViewerBlock
        };
    }
    if dy < 0 {
        return Direction::SoutheastOfViewer;
    }
    // (dy > 0) * 4 + EastOfViewer == east or northeast
    if dy > 0 {
        Direction::NortheastOfViewer
    } else {
        Direction::EastOfViewer
    }
}

/// The cell-index half of `inbound_valid_cellid`: `1..=0x40` (land), `0x100..=0xFFFD` (interior)
/// and `0xFFFF` (the whole-block pseudo cell). This is the index test the re-basing paths
/// apply — **without** the landblock bounds check.
#[must_use]
pub const fn valid_cell_index(id: CellId) -> bool {
    let c = id.0 & 0xFFFF;
    (c != 0 && c < 0x41) || (c > 0xFF && c < 0xFFFE) || c == 0xFFFF
}

/// A valid index **and** both landblock
/// coordinates inside `0x7F8`.
#[must_use]
pub const fn inbound_valid_cellid(id: CellId) -> bool {
    valid_cell_index(id) && ((id.0 >> 21) & 0x7F8) < 0x7F8 && ((id.0 >> 13) & 0x7F8) < 0x7F8
}

/// "Am I outdoors?" — the test the whole engine uses: `(objcell_id & 0xFFFF) < 0x100`.
#[must_use]
pub const fn is_outdoors(id: CellId) -> bool {
    (id.0 & 0xFFFF) < 0x100
}

/// The landscape definitions' in-bounds test.
#[must_use]
pub const fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < LCOORD_LIMIT && y < LCOORD_LIMIT
}

/// The landblock's `0xXXYYFFFF` data id.
#[must_use]
pub const fn get_block_did(id: CellId) -> DataId {
    DataId((id.0 & 0xFFFF_0000) | 0xFFFF)
}

/// The global cell coordinates of a landblock's
/// south-west corner. Returns `None` for id `0` or an out-of-range block.
#[must_use]
pub fn blockid_to_lcoord(id: CellId) -> Option<(i32, i32)> {
    if id.0 == 0 {
        return None;
    }
    #[allow(clippy::cast_possible_wrap)]
    let x = ((id.0 >> 21) & 0x7F8) as i32;
    #[allow(clippy::cast_possible_wrap)]
    let y = (((id.0 >> 16) & 0xFF) * 8) as i32;
    if (0..LCOORD_LIMIT).contains(&x) && y < LCOORD_LIMIT {
        Some((x, y))
    } else {
        None
    }
}

/// The global cell coordinates of an **outdoor** cell id.
#[must_use]
pub fn gid_to_lcoord(id: CellId) -> Option<(i32, i32)> {
    if !inbound_valid_cellid(id) || (id.0 & 0xFFFF) >= 0x100 {
        return None;
    }
    #[allow(clippy::cast_possible_wrap)]
    let mut x = ((id.0 >> 21) & 0x7F8) as i32;
    #[allow(clippy::cast_possible_wrap)]
    let mut y = (((id.0 >> 16) & 0xFF) << 3) as i32;
    #[allow(clippy::cast_possible_wrap)]
    {
        x += (((id.0 & 0xFFFF) - 1) >> 3) as i32;
        // The original writes `(id - 1) & 7` over the whole 32-bit id; identical for any id whose
        // low word is at least 1, which the guard above has already established.
        y += (id.0.wrapping_sub(1) & 7) as i32;
    }
    if in_bounds(x, y) {
        Some((x, y))
    } else {
        None
    }
}

/// Returns `CellId(0)` when either coordinate is outside
/// `[0, 0x7F8)`, exactly as the original does.
#[must_use]
pub fn lcoord_to_gid(x: i32, y: i32) -> CellId {
    if !in_bounds(x, y) {
        return CellId(0);
    }
    #[allow(clippy::cast_sign_loss)]
    let (ux, uy) = (x as u32, y as u32);
    CellId(((uy & 7) + 1 + (ux & 7) * 8) | (((ux & 0xFFFF_FFF8) << 5) | (uy >> 3)) << 16)
}

/// The metre offset from one landblock to another.
/// Z is always `0`; landblocks are not stacked.
#[must_use]
pub fn get_block_offset(from: CellId, to: CellId) -> Vec3 {
    if from.0 >> 16 == to.0 >> 16 {
        return Vec3::ZERO;
    }
    // Note: the original does NOT bounds-check here; it only special-cases id 0.
    #[allow(clippy::cast_possible_wrap)]
    let (fx, fy) = if from.0 == 0 {
        (0_i32, 0_i32)
    } else {
        (
            (((from.0 >> 21) & 0x7F8) as i32),
            ((((from.0 >> 16) & 0xFF) << 3) as i32),
        )
    };
    #[allow(clippy::cast_possible_wrap)]
    let (tx, ty) = if to.0 == 0 {
        (0_i32, 0_i32)
    } else {
        (
            (((to.0 >> 21) & 0x7F8) as i32),
            ((((to.0 >> 16) & 0xFF) << 3) as i32),
        )
    };
    #[allow(clippy::cast_precision_loss)]
    Vec3::new(
        (tx - fx) as f32 * CELL_SIZE,
        (ty - fy) as f32 * CELL_SIZE,
        0.0,
    )
}

/// The global cell coordinates a landblock-relative
/// origin lands in, which may be outside the block the id names.
///
/// The division is by the **double** literal `24.0`, so the float origin is promoted first; that
/// is reproduced here rather than done in `f32`.
#[must_use]
pub fn get_outside_lcoord(id: CellId, origin: Vec3) -> Option<(i32, i32)> {
    if !valid_cell_index(id) {
        return None;
    }
    let (bx, by) = blockid_to_lcoord(id).unwrap_or((0, 0));
    let x = bx + to_i32_f64((f64::from(origin.x) / 24.0).floor());
    let y = by + to_i32_f64((f64::from(origin.y) / 24.0).floor());
    if in_bounds(x, y) {
        Some((x, y))
    } else {
        None
    }
}

/// Re-base an origin that has walked past `0` or
/// `192` into the correct landblock. On failure the id is zeroed, which is what the original
/// does and what callers test for.
///
/// Note the guard is the *index* test, not [`inbound_valid_cellid`]: a block coordinate at the
/// very edge is not rejected here.
pub fn adjust_to_outside(id: &mut CellId, origin: &mut Vec3) -> bool {
    if !valid_cell_index(*id) {
        *id = CellId(0);
        return false;
    }
    if origin.x.abs() < crate::globals::EPSILON {
        origin.x = 0.0;
    }
    if origin.y.abs() < crate::globals::EPSILON {
        origin.y = 0.0;
    }
    let Some((x, y)) = get_outside_lcoord(*id, *origin) else {
        *id = CellId(0);
        return false;
    };
    *id = lcoord_to_gid(x, y);
    origin.x -= (origin.x / BLOCK_LENGTH).floor() * BLOCK_LENGTH;
    origin.y -= (origin.y / BLOCK_LENGTH).floor() * BLOCK_LENGTH;
    true
}

/// [`adjust_to_outside`] on a copy.
#[must_use]
pub fn get_outside_cell_id(id: CellId, origin: Vec3) -> CellId {
    let (mut id, mut o) = (id, origin);
    if adjust_to_outside(&mut id, &mut o) {
        id
    } else {
        CellId(0)
    }
}

/// The cell centre of an outdoor cell in landblock-relative metres:
/// `((2 * cx + 1) * 12, (2 * cy + 1) * 12, 0)`, from the landblock's polygon construction.
#[must_use]
pub fn land_cell_origin(cell_x: i32, cell_y: i32) -> Vec3 {
    #[allow(clippy::cast_precision_loss)]
    Vec3::new(
        (2 * cell_x + 1) as f32 * crate::globals::HALF_SQUARE_LENGTH,
        (2 * cell_y + 1) as f32 * crate::globals::HALF_SQUARE_LENGTH,
        0.0,
    )
}

/// The `(cell_x, cell_y)` inside the block for an outdoor cell index (`1 ..= 0x40`).
///
/// `index = (cell_x & 7) * 8 + (cell_y & 7) + 1`, so this is its inverse. The client
/// spells it `cy = (id - 1) & 7`, `cx = (index - 1) >> 3`.
#[must_use]
pub const fn cell_index_to_xy(index: u16) -> (u32, u32) {
    let i = index as u32;
    (((i - 1) >> 3) & 7, (i - 1) & 7)
}

/// Accept the region's 256-entry table only if every
/// entry is in `[0, 800]`. The original stops at the first bad entry and returns 0, leaving the
/// entries it already copied in place; this returns the validated table or `None`.
#[must_use]
pub fn validate_height_table(table: &[f32]) -> Option<[f32; LAND_HEIGHT_TABLE_LEN]> {
    if table.len() < LAND_HEIGHT_TABLE_LEN {
        return None;
    }
    let mut out = [0.0_f32; LAND_HEIGHT_TABLE_LEN];
    for (o, &v) in out.iter_mut().zip(table.iter()) {
        if !(0.0..=LAND_HEIGHT_MAX).contains(&v) {
            return None;
        }
        *o = v;
    }
    Some(out)
}

/// The location formatter's numeric half: the familiar `N/S, E/W` pair
/// in tenths of a cell, derived by subtracting `(0x400, 0x100)` from the global cell coordinates
/// and scaling by `0.1`.
#[must_use]
pub fn cellid_to_coordinates(id: CellId) -> Option<(f32, f32)> {
    let (x, y) = gid_to_lcoord(id)?;
    #[allow(clippy::cast_precision_loss)]
    Some(((y - 0x100) as f32 * 0.1, (x - 0x400) as f32 * 0.1))
}

/// The `floor(v / CELL_SIZE)` used by land-cell lookup and by the outdoor cell-list builder,
/// as a single named helper so the `as i32` lint never has to be silenced at a call site.
#[must_use]
pub fn cell_of(v: f32) -> i32 {
    floor_to_i32(v / CELL_SIZE)
}

/// The horizontal distance between two landblock-relative points that may be in different
/// blocks, used by the tests below and by `land.rs`.
#[must_use]
pub fn offset_between(from: CellId, from_p: Vec3, to: CellId, to_p: Vec3) -> Vec3 {
    get_block_offset(from, to).add(to_p).sub(from_p)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the client's own cell-id validation, the landblock-coordinate conversions,
    // the block offset, the adjust-to-outside step and the outside-coordinate lookup.

    #[test]
    fn cell_index_validity_covers_every_documented_range() {
        let lb = 0xA9B4_0000_u32;
        assert!(!valid_cell_index(CellId(lb)), "index 0 is invalid");
        for i in 1..=0x40_u32 {
            assert!(
                valid_cell_index(CellId(lb | i)),
                "{i:#x} is an outdoor land cell"
            );
        }
        for i in 0x41..=0xFF_u32 {
            assert!(
                !valid_cell_index(CellId(lb | i)),
                "{i:#x} is in the invalid gap"
            );
        }
        for i in [0x100_u32, 0x1234, 0xFFFD] {
            assert!(
                valid_cell_index(CellId(lb | i)),
                "{i:#x} is an interior cell"
            );
        }
        assert!(
            !valid_cell_index(CellId(lb | 0xFFFE)),
            "0xFFFE is the landblock-info pseudo id"
        );
        assert!(
            valid_cell_index(CellId(lb | 0xFFFF)),
            "0xFFFF is the valid whole-block id"
        );
    }

    #[test]
    fn inbound_valid_cellid_rejects_the_top_block_row_and_column() {
        // blockX = 0xFF gives lcoord 0x7F8, which is not < 0x7F8.
        assert!(!inbound_valid_cellid(CellId(0xFF00_0001)));
        assert!(!inbound_valid_cellid(CellId(0x00FF_0001)));
        assert!(inbound_valid_cellid(CellId(0xFEFE_0001)));
    }

    #[test]
    fn outdoor_cell_ids_round_trip_exhaustively_over_the_index_range() {
        // Every one of the 64 indices, over a spread of landblocks, through
        // gid_to_lcoord -> lcoord_to_gid.
        for &(bx, by) in &[
            (0_u32, 0_u32),
            (1, 0),
            (0, 1),
            (0x7F, 0x33),
            (0xFE, 0xFE),
            (0xA9, 0xB4),
        ] {
            for index in 1..=0x40_u32 {
                let id = CellId((bx << 24) | (by << 16) | index);
                let (x, y) = gid_to_lcoord(id).expect("valid outdoor id");
                assert_eq!(lcoord_to_gid(x, y), id, "{id:?} -> ({x},{y})");
                // and the coordinates decompose the way the encoding says they should
                #[allow(clippy::cast_possible_wrap)]
                {
                    assert_eq!(x >> 3, bx as i32);
                    assert_eq!(y >> 3, by as i32);
                }
                assert_eq!((x & 7) * 8 + (y & 7) + 1, i32::try_from(index).unwrap());
            }
        }
    }

    #[test]
    fn lcoord_to_gid_covers_the_whole_grid_and_rejects_the_edges() {
        assert_eq!(lcoord_to_gid(-1, 0), CellId(0));
        assert_eq!(lcoord_to_gid(0, -1), CellId(0));
        assert_eq!(lcoord_to_gid(LCOORD_LIMIT, 0), CellId(0));
        assert_eq!(lcoord_to_gid(0, LCOORD_LIMIT), CellId(0));
        // a full sweep of the last valid coordinate
        let id = lcoord_to_gid(LCOORD_LIMIT - 1, LCOORD_LIMIT - 1);
        assert_eq!(id, CellId(0xFEFE_0040));
        assert_eq!(
            gid_to_lcoord(id),
            Some((LCOORD_LIMIT - 1, LCOORD_LIMIT - 1))
        );
    }

    #[test]
    fn lcoord_round_trips_over_a_deterministic_sweep_of_the_grid() {
        // Every 37th coordinate in both axes: 56 x 56 = 3136 pairs, spanning every block row.
        let mut checked = 0;
        let mut x = 0;
        while x < LCOORD_LIMIT {
            let mut y = 0;
            while y < LCOORD_LIMIT {
                let id = lcoord_to_gid(x, y);
                assert_ne!(id, CellId(0));
                assert_eq!(gid_to_lcoord(id), Some((x, y)), "({x},{y})");
                checked += 1;
                y += 37;
            }
            x += 37;
        }
        assert_eq!(checked, 56 * 56);
    }

    #[test]
    fn get_block_offset_is_zero_within_a_block_and_192_between_neighbours() {
        let a = CellId(0xA9B4_0000 | 1);
        assert_eq!(get_block_offset(a, CellId(0xA9B4_0000 | 0x40)), Vec3::ZERO);
        // one block east is +192 in X
        let east = CellId(0xAAB4_0000 | 1);
        assert_eq!(get_block_offset(a, east), Vec3::new(192.0, 0.0, 0.0));
        // one block north is +192 in Y
        let north = CellId(0xA9B5_0000 | 1);
        assert_eq!(get_block_offset(a, north), Vec3::new(0.0, 192.0, 0.0));
        // and it is antisymmetric
        assert_eq!(get_block_offset(east, a), Vec3::new(-192.0, 0.0, 0.0));
        // Z is never anything but zero
        assert_eq!(get_block_offset(a, CellId(0x0000_0001)).z, 0.0);
    }

    #[test]
    fn get_block_offset_matches_a_random_sweep_of_landblock_pairs() {
        // Oracle: the formula in get_block_offset, evaluated independently here from
        // the block coordinates rather than restated from the implementation.
        let mut seed = 0x1234_5678_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 16) & 0xFF
        };
        for _ in 0..2000 {
            let (ax, ay, bx, by) = (next(), next(), next(), next());
            let a = CellId((ax << 24) | (ay << 16) | 1);
            let b = CellId((bx << 24) | (by << 16) | 1);
            let off = get_block_offset(a, b);
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
            let expect = if ax == bx && ay == by {
                Vec3::ZERO
            } else {
                Vec3::new(
                    (bx as i32 - ax as i32) as f32 * 192.0,
                    (by as i32 - ay as i32) as f32 * 192.0,
                    0.0,
                )
            };
            assert_eq!(off, expect, "{a:?} -> {b:?}");
        }
    }

    #[test]
    fn adjust_to_outside_rebases_across_a_block_boundary() {
        // Walk 1 m past the east edge of block (0xA9, 0xB4).
        let mut id = CellId(0xA9B4_0000 | 1);
        let mut o = Vec3::new(193.0, 10.0, 5.0);
        assert!(adjust_to_outside(&mut id, &mut o));
        assert_eq!(id.landblock().x(), 0xAA, "the block must advance east");
        assert_eq!(id.landblock().y(), 0xB4);
        assert!((o.x - 1.0).abs() < 1e-4, "{o:?}");
        assert_eq!(o.y, 10.0);
        assert_eq!(o.z, 5.0, "Z is never wrapped");
    }

    #[test]
    fn adjust_to_outside_rebases_backwards_too() {
        let mut id = CellId(0xA9B4_0000 | 1);
        let mut o = Vec3::new(-1.0, 10.0, 0.0);
        assert!(adjust_to_outside(&mut id, &mut o));
        assert_eq!(id.landblock().x(), 0xA8);
        assert!((o.x - 191.0).abs() < 1e-3, "{o:?}");
    }

    #[test]
    fn adjust_to_outside_snaps_tiny_coordinates_to_zero_first() {
        // Without the snap, -1e-5 would floor to cell -1 and move the object a block west.
        let mut id = CellId(0xA9B4_0000 | 1);
        let mut o = Vec3::new(-0.000_01, -0.000_01, 0.0);
        assert!(adjust_to_outside(&mut id, &mut o));
        assert_eq!(id.landblock().x(), 0xA9);
        assert_eq!(id.landblock().y(), 0xB4);
        assert_eq!(o.x, 0.0);
        assert_eq!(o.y, 0.0);
    }

    #[test]
    fn adjust_to_outside_zeroes_the_id_when_it_walks_off_the_world() {
        let mut id = CellId(0x0000_0001);
        let mut o = Vec3::new(-1000.0, 0.0, 0.0);
        assert!(!adjust_to_outside(&mut id, &mut o));
        assert_eq!(id, CellId(0));
        // and for an id whose index is in the invalid gap
        let mut bad = CellId(0xA9B4_0000 | 0x50);
        let mut p = Vec3::ZERO;
        assert!(!adjust_to_outside(&mut bad, &mut p));
        assert_eq!(bad, CellId(0));
    }

    #[test]
    fn adjust_to_outside_round_trips_over_a_sweep_of_offsets() {
        // For any point inside the block, adjust_to_outside must be idempotent and must pick the
        // cell the index encoding names.
        for i in 0..8 {
            for j in 0..8 {
                #[allow(clippy::cast_precision_loss)]
                let o0 = Vec3::new(i as f32 * 24.0 + 12.0, j as f32 * 24.0 + 12.0, 0.0);
                let mut id = CellId(0xA9B4_0000 | 1);
                let mut o = o0;
                assert!(adjust_to_outside(&mut id, &mut o));
                assert_eq!(o, o0, "an in-block origin must be untouched");
                assert_eq!(id.index(), u16::try_from(i * 8 + j + 1).unwrap());
                // idempotent
                let (before_id, before_o) = (id, o);
                assert!(adjust_to_outside(&mut id, &mut o));
                assert_eq!((id, o), (before_id, before_o));
            }
        }
    }

    #[test]
    fn cell_index_to_xy_inverts_the_index_encoding() {
        for x in 0..8_u32 {
            for y in 0..8_u32 {
                let index = u16::try_from(x * 8 + y + 1).unwrap();
                assert_eq!(cell_index_to_xy(index), (x, y));
            }
        }
    }

    #[test]
    fn height_table_validation_matches_the_client_bounds() {
        let good = [0.0_f32; LAND_HEIGHT_TABLE_LEN];
        assert!(validate_height_table(&good).is_some());
        let mut edge = good;
        edge[201] = 800.0;
        assert!(
            validate_height_table(&edge).is_some(),
            "800 exactly is accepted"
        );
        let mut over = good;
        over[3] = 800.000_1;
        assert!(validate_height_table(&over).is_none());
        let mut neg = good;
        neg[3] = -0.000_1;
        assert!(validate_height_table(&neg).is_none());
        assert!(
            validate_height_table(&good[..255]).is_none(),
            "a short table is rejected"
        );
    }

    #[test]
    fn direction_headings_are_radians() {
        assert_eq!(heading(Direction::InViewerBlock), 0.0);
        assert_eq!(heading(Direction::NorthOfViewer), 0.0);
        assert!((heading(Direction::EastOfViewer) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((heading(Direction::SouthOfViewer) - std::f32::consts::PI).abs() < 1e-6);
        assert_eq!(get_dir(-1, -1), Direction::SouthwestOfViewer);
        assert_eq!(get_dir(0, 0), Direction::InViewerBlock);
        assert_eq!(get_dir(1, 1), Direction::NortheastOfViewer);
        assert_eq!(get_dir(1, 0), Direction::EastOfViewer);
        assert_eq!(get_dir(0, 1), Direction::NorthOfViewer);
    }
}
