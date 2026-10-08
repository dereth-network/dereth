//! The landblock's on-road test.
//!
//! Scenery suppression near roads follows the complete 16-case table below.
//!
//! ACE's older scenery placement, which only its landblock-mesh loader used and which is no longer
//! called, stubs this test as `(terrain & 3) != 0`, marked TODO in its own source. The scenery ACE's
//! server collides with has the full table. Every case below decides which trees exist.

use crate::consts::{CELL_SIZE, ROAD_WIDTH, ROAD_WIDTH_FAR, SIDE_VERTEX_COUNT, VERTEX_COUNT};

/// The landblock's on-road test.
///
/// `(x, y)` are block-local metres in `[0, 192)`. The four corner road bits of the containing cell
/// are `A = SW`, `B = NW`, `C = SE`, `D = NE`; the road half-width is 5 and its mirror is 19.
///
/// Out-of-range points return `false` rather than indexing past the grid: the client is called only
/// after the strict `0 <= x, y < 192` bounds filter, so this branch is unreachable in the scenery
/// path and exists so a stray caller cannot panic.
#[must_use]
pub fn on_road(terrain: &[u16; VERTEX_COUNT], x: f32, y: f32) -> bool {
    let cx = dereth_primitives::num::floor_to_i32(x / CELL_SIZE);
    let cy = dereth_primitives::num::floor_to_i32(y / CELL_SIZE);
    if !(0..8).contains(&cx) || !(0..8).contains(&cy) {
        return false;
    }
    #[allow(clippy::cast_precision_loss)] // cx, cy are 0..=7
    let (dx, dy) = (x - CELL_SIZE * cx as f32, y - CELL_SIZE * cy as f32);
    let base = (cx as usize) * SIDE_VERTEX_COUNT + cy as usize;
    let a = terrain[base] & 3 != 0; // SW
    let b = terrain[base + 1] & 3 != 0; // NW  (+y)
    let c = terrain[base + SIDE_VERTEX_COUNT] & 3 != 0; // SE  (+x)
    let d = terrain[base + SIDE_VERTEX_COUNT + 1] & 3 != 0; // NE
    on_road_cell(a, b, c, d, dx, dy)
}

/// The 16-case geometry, split out so each case has a unit test that names its corner set.
///
/// | corners with road | on-road test |
/// |---|---|
/// | none | false |
/// | all four | true |
/// | A only (SW) | `dx + dy < 5` |
/// | B only (NW) | `dy - dx > 19` |
/// | C only (SE) | `dy - dx < -19` |
/// | D only (NE) | `dx + dy > 43` |
/// | A, B (west edge) | `dx < 5` |
/// | A, C (south edge) | `dy < 5` |
/// | C, D (east edge) | `dx > 19` |
/// | B, D (north edge) | `dy > 19` |
/// | A, D (SW-NE diagonal) | `\|dx - dy\| < 5` |
/// | B, C (NW-SE diagonal) | `\|dx + dy - 24\| < 5` |
/// | A, B, C (no NE) | `dx < 5 or dy < 5` |
/// | A, B, D (no SE) | `dx < 5 or dy > 19` |
/// | A, C, D (no NW) | `dx > 19 or dy < 5` |
/// | B, C, D (no SW) | `dx > 19 or dy > 19` |
#[must_use]
pub fn on_road_cell(a: bool, b: bool, c: bool, d: bool, dx: f32, dy: f32) -> bool {
    const W: f32 = ROAD_WIDTH; // 5
    const F: f32 = ROAD_WIDTH_FAR; // 19
    match (a, b, c, d) {
        (false, false, false, false) => false,
        (true, true, true, true) => true,
        // one corner
        (true, false, false, false) => dx + dy < W,
        (false, true, false, false) => dy - dx > F,
        (false, false, true, false) => dy - dx < -F,
        (false, false, false, true) => dx + dy > CELL_SIZE + F,
        // two corners: the four edges
        (true, true, false, false) => dx < W,
        (true, false, true, false) => dy < W,
        (false, false, true, true) => dx > F,
        (false, true, false, true) => dy > F,
        // two corners: the two diagonals
        (true, false, false, true) => (dx - dy).abs() < W,
        (false, true, true, false) => (dx + dy - CELL_SIZE).abs() < W,
        // three corners
        (true, true, true, false) => dx < W || dy < W,
        (true, true, false, true) => dx < W || dy > F,
        (true, false, true, true) => dx > F || dy < W,
        (false, true, true, true) => dx > F || dy > F,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered 16-case road table. One case
    /// per row, each probed at a point the row's geometry says is on the road and at one it says is
    /// off it. The stub in ACE's older scenery path would answer "on road" for every row with any
    /// corner set, so every `false` assertion here is also a regression test against porting it.
    #[test]
    fn every_one_of_the_sixteen_cases_matches_the_table() {
        // (A, B, C, D, on-road point, off-road point)
        type Case = (bool, bool, bool, bool, (f32, f32), (f32, f32));
        const CASES: &[Case] = &[
            // no corners: nothing is on the road, anywhere.
            (false, false, false, false, (0.0, 0.0), (0.0, 0.0)),
            // A only (SW corner at (0,0)): dx + dy < 5.
            (true, false, false, false, (1.0, 1.0), (12.0, 12.0)),
            // B only (NW corner at (0,24)): dy - dx > 19.
            (false, true, false, false, (1.0, 23.0), (12.0, 12.0)),
            // C only (SE corner at (24,0)): dy - dx < -19.
            (false, false, true, false, (23.0, 1.0), (12.0, 12.0)),
            // D only (NE corner at (24,24)): dx + dy > 43.
            (false, false, false, true, (23.0, 23.0), (12.0, 12.0)),
            // A, B: the west edge, dx < 5.
            (true, true, false, false, (1.0, 12.0), (12.0, 12.0)),
            // A, C: the south edge, dy < 5.
            (true, false, true, false, (12.0, 1.0), (12.0, 12.0)),
            // C, D: the east edge, dx > 19.
            (false, false, true, true, (23.0, 12.0), (12.0, 12.0)),
            // B, D: the north edge, dy > 19.
            (false, true, false, true, (12.0, 23.0), (12.0, 12.0)),
            // A, D: the SW-NE diagonal, |dx - dy| < 5.
            (true, false, false, true, (12.0, 12.0), (2.0, 22.0)),
            // B, C: the NW-SE diagonal, |dx + dy - 24| < 5.
            (false, true, true, false, (12.0, 12.0), (2.0, 2.0)),
            // A, B, C: dx < 5 or dy < 5.
            (true, true, true, false, (1.0, 20.0), (12.0, 12.0)),
            // A, B, D: dx < 5 or dy > 19.
            (true, true, false, true, (12.0, 23.0), (12.0, 12.0)),
            // A, C, D: dx > 19 or dy < 5.
            (true, false, true, true, (12.0, 1.0), (12.0, 12.0)),
            // B, C, D: dx > 19 or dy > 19.
            (false, true, true, true, (23.0, 12.0), (12.0, 12.0)),
            // all four: everything is road.
            (true, true, true, true, (12.0, 12.0), (12.0, 12.0)),
        ];
        for &(a, b, c, d, on, off) in CASES {
            let all = a && b && c && d;
            let none = !(a || b || c || d);
            assert_eq!(
                on_road_cell(a, b, c, d, on.0, on.1),
                !none,
                "({a},{b},{c},{d}) at {on:?} should be on the road"
            );
            if !all && !none {
                assert!(
                    !on_road_cell(a, b, c, d, off.0, off.1),
                    "({a},{b},{c},{d}) at {off:?} should be off the road"
                );
            }
        }
    }

    /// Oracle: ACE's older scenery path answers `(terrain & 3) != 0`, i.e. "on road" whenever
    /// *any* corner has a road bit. The cell centre of a single-corner road cell is the cheapest
    /// place to see the two disagree, and this test pins the disagreement so nobody "simplifies"
    /// it away.
    #[test]
    fn the_client_disagrees_with_aces_stub_at_the_cell_centre() {
        for corners in [(true, false, false, false), (false, true, false, false)] {
            let (a, b, c, d) = corners;
            let ace_says = a || b || c || d;
            assert!(ace_says);
            assert!(
                !on_road_cell(a, b, c, d, 12.0, 12.0),
                "the client says off-road at the centre"
            );
        }
    }

    /// Oracle: the road calculation's `cx = floor(x/24)` / `dx = x - 24*cx` decomposition and
    /// the `x*9 + y` corner indexing, checked by putting the road on exactly one grid vertex and
    /// finding which cells see it.
    #[test]
    fn cell_selection_and_corner_indexing_use_the_x_major_grid() {
        let mut terrain = [0u16; VERTEX_COUNT];
        // Vertex (3, 4) is on a road. It is the NE corner of cell (2,3), the NW of (3,3),
        // the SE of (2,4) and the SW of (3,4).
        terrain[3 * SIDE_VERTEX_COUNT + 4] = 1;
        // Cell (3,4) has only its SW corner set: on-road iff dx + dy < 5.
        assert!(on_road(&terrain, 3.0 * 24.0 + 1.0, 4.0 * 24.0 + 1.0));
        assert!(!on_road(&terrain, 3.0 * 24.0 + 12.0, 4.0 * 24.0 + 12.0));
        // Cell (2,3) has only its NE corner set: on-road iff dx + dy > 43.
        assert!(on_road(&terrain, 2.0 * 24.0 + 23.0, 3.0 * 24.0 + 23.0));
        assert!(!on_road(&terrain, 2.0 * 24.0 + 1.0, 3.0 * 24.0 + 1.0));
        // A cell nowhere near it is clear.
        assert!(!on_road(&terrain, 100.0, 100.0));
    }

    /// Oracle: the road half-width is the eighth land-definition value, 5, and its mirror is
    /// `24 - 5 = 19` — the client writes both literals.
    #[test]
    fn the_road_half_width_is_five_and_its_mirror_is_nineteen() {
        assert_eq!(ROAD_WIDTH, 5.0);
        assert_eq!(ROAD_WIDTH_FAR, 19.0);
        // The west-edge case flips exactly at 5.
        assert!(on_road_cell(true, true, false, false, 4.999, 12.0));
        assert!(!on_road_cell(true, true, false, false, 5.0, 12.0));
        // The east-edge case flips exactly at 19.
        assert!(!on_road_cell(false, false, true, true, 19.0, 12.0));
        assert!(on_road_cell(false, false, true, true, 19.001, 12.0));
    }
}
