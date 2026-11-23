//! AC1's procedural scenery kept clear of AC2's placed statics.
//!
//! AC1 grows a cell's scenery from the scene its south-west corner's terrain word names (bits
//! 11-15, an index into the terrain type's 32 scene types), and a scene type with no scenes grows
//! nothing, on the client and the server alike. Every retail terrain type fills all 32, most with
//! scenes, so the converted world's region gives every type's last scene type ([`NO_SCENE`]) a
//! scene description with no scenes; the land's own words, which carry AC2's scene field, are kept
//! off it (moved to the one before). A cell under a static, or within a few meters of one, then has
//! its corner's scene index set to [`NO_SCENE`]: the trees and rocks AC1 would grow through a wall
//! or a house are not grown, and nothing else about the ground changes.

/// The scene index that grows nothing in the converted world.
pub const NO_SCENE: u16 = 31;

/// `word` with its scene index set to [`NO_SCENE`], its road, type and variant kept.
#[must_use]
pub const fn without_scenery(word: u16) -> u16 {
    (word & 0x07FF) | (NO_SCENE << 11)
}

/// The cells (column east, row north, `cell` meters a side, `cells` a world side) that the box
/// `[x0, x1] x [y0, y1]` meters, grown by `margin`, overlaps.
#[must_use]
pub fn cells_under(
    (x0, y0, x1, y1): (f64, f64, f64, f64),
    margin: f64,
    cell: f64,
    cells: usize,
) -> Vec<(usize, usize)> {
    let span = |lo: f64, hi: f64| {
        let a = ((lo - margin) / cell).floor().max(0.0);
        let b = ((hi + margin) / cell).floor().min(cells as f64 - 1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        (a as usize, b as usize, a <= b)
    };
    let (ca, cb, okx) = span(x0, x1);
    let (ra, rb, oky) = span(y0, y1);
    if !okx || !oky {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in ca..=cb {
        for r in ra..=rb {
            out.push((c, r));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's scenery clearing)
    use super::*;

    #[test]
    fn a_cleared_word_names_no_scene_and_keeps_its_road_type_and_variant() {
        let w = 0x3 | (21 << 2) | (5 << 7) | (17 << 11);
        let c = without_scenery(w);
        assert_eq!(c >> 11, NO_SCENE);
        assert_eq!(c & 0x07FF, w & 0x07FF);
    }

    #[test]
    fn the_cells_under_a_static_and_its_margin_are_cleared_and_no_others() {
        // A 10 m house at 30..40 east, 50..60 north, cleared 4 m round: cells 1 (24..48) east,
        // 1..2 (24..72) north.
        assert_eq!(
            cells_under((30.0, 50.0, 40.0, 60.0), 4.0, 24.0, 100),
            [(1, 1), (1, 2)]
        );
        // The margin reaches the next cell west.
        assert_eq!(
            cells_under((25.0, 30.0, 26.0, 31.0), 4.0, 24.0, 100),
            [(0, 1), (1, 1)]
        );
        // Off the world's edges, clamped.
        assert_eq!(
            cells_under((-50.0, -50.0, -40.0, -40.0), 4.0, 24.0, 100),
            []
        );
        assert_eq!(
            cells_under((2390.0, 0.0, 2500.0, 1.0), 0.0, 24.0, 100),
            [(99, 0)]
        );
    }
}
