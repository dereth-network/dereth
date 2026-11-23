//! The in-game map window's picture: the retail picture's parchment frame kept, the converted
//! land laid into its marker box so that a map note or the player's marker lands on the land it
//! names.
//!
//! The window places a marker at map coordinates `(ew, ns)` (tenths of a coordinate unit biased by
//! 1024: `b = c * 10 + 1024`) at `x0 + trunc(b_ew * w / 2048)` across and
//! `y0 + trunc((2047 - b_ns) * h / 2048)` down, `w` and `h` the box's sides. A player in global
//! land cell `l` (24 m cells from the world's south-west corner) is shown at coordinate
//! `(l - 1024) / 10 + 0.5`, so `b = l + 5`; a point a fraction into its cell sits on average at
//! `b = cells + 4.5`.

/// The map picture's marker box in its own pixels, inclusive: what the window's note binding
/// says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkerBox {
    pub x0: usize,
    pub x1: usize,
    pub y0: usize,
    pub y1: usize,
}

/// The shipped window's box.
pub const SHIPPED: MarkerBox = MarkerBox {
    x0: 6,
    x1: 247,
    y0: 8,
    y1: 258,
};

/// Where a cell's biased map value sits relative to the cell's own continuous coordinate.
const BIAS_FROM_CELLS: f64 = 4.5;

impl MarkerBox {
    fn w(&self) -> f64 {
        (self.x1 - self.x0 + 1) as f64
    }
    fn h(&self) -> f64 {
        (self.y1 - self.y0 + 1) as f64
    }

    /// The land a box pixel shows, as continuous global land-cell coordinates east and north:
    /// the span `[lo, hi)` its markers come from, each way.
    #[must_use]
    pub fn cells_of(&self, px: usize, py: usize) -> ((f64, f64), (f64, f64)) {
        let (dx, dy) = ((px - self.x0) as f64, (py - self.y0) as f64);
        let ew = (dx * 2048.0 / self.w(), (dx + 1.0) * 2048.0 / self.w());
        // Down the box is north to south: the row's top edge is its northern one.
        let ns = (
            2047.0 - (dy + 1.0) * 2048.0 / self.h(),
            2047.0 - dy * 2048.0 / self.h(),
        );
        (
            (ew.0 - BIAS_FROM_CELLS, ew.1 - BIAS_FROM_CELLS),
            (ns.0 - BIAS_FROM_CELLS, ns.1 - BIAS_FROM_CELLS),
        )
    }

    /// Whether picture pixel `(px, py)` is in the box.
    #[cfg(test)]
    #[must_use]
    pub fn holds(&self, px: usize, py: usize) -> bool {
        (self.x0..=self.x1).contains(&px) && (self.y0..=self.y1).contains(&py)
    }

    /// The box on the picture made `k` times as large each way: each pixel `k` by `k`.
    #[must_use]
    pub const fn scaled(self, k: usize) -> Self {
        Self {
            x0: self.x0 * k,
            x1: (self.x1 + 1) * k - 1,
            y0: self.y0 * k,
            y1: (self.y1 + 1) * k - 1,
        }
    }
}

/// How many times as large each way the map picture is made: [`INTERFACE_SCALE`] times as large
/// as the map pages show it, where they stretch the part of the world the land takes (`side` of
/// the world's `whole`, one way) over the whole marker box. At least [`INTERFACE_SCALE`], and at
/// most [`MOST_ENLARGED`].
#[must_use]
pub fn enlargement(side: f64, whole: f64) -> usize {
    let shown = if side <= 0.0 {
        1
    } else {
        // LINT-OK: a small whole number, held to the range below.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (whole / side).ceil() as usize;
        k.clamp(1, MOST_ENLARGED)
    };
    (shown * INTERFACE_SCALE).min(MOST_ENLARGED)
}

/// How many times as large again as the pages show it the map picture is made, so it keeps its
/// own texels on an interface drawn up to this many times as large.
pub const INTERFACE_SCALE: usize = 2;

/// The most the map picture is enlarged: a picture past this is larger than any page shows.
pub const MOST_ENLARGED: usize = 8;

/// `rgb`, `w` by `h`, made `k` times as large each way, each pixel blended from the four about
/// its middle.
#[must_use]
pub fn enlarge(rgb: &[u8], (w, h): (usize, usize), k: usize) -> Vec<u8> {
    let (bw, bh) = (w * k, h * k);
    let mut out = vec![0u8; bw * bh * 3];
    let at = |x: usize, y: usize, c: usize| f64::from(rgb[(y * w + x) * 3 + c]);
    for y in 0..bh {
        // LINT-OK: pixels of a picture of a few hundred a side.
        #[allow(clippy::cast_precision_loss)]
        let v = ((y as f64 + 0.5) / k as f64 - 0.5).clamp(0.0, (h - 1) as f64);
        // LINT-OK: inside the picture, as clamped above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let y0 = v.floor() as usize;
        let (y1, fy) = ((y0 + 1).min(h - 1), v - v.floor());
        for x in 0..bw {
            #[allow(clippy::cast_precision_loss)]
            let u = ((x as f64 + 0.5) / k as f64 - 0.5).clamp(0.0, (w - 1) as f64);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let x0 = u.floor() as usize;
            let (x1, fx) = ((x0 + 1).min(w - 1), u - u.floor());
            for c in 0..3 {
                let top = at(x0, y0, c) * (1.0 - fx) + at(x1, y0, c) * fx;
                let bottom = at(x0, y1, c) * (1.0 - fx) + at(x1, y1, c) * fx;
                // LINT-OK: a blend of bytes.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let v = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
                out[(y * bw + x) * 3 + c] = v;
            }
        }
    }
    out
}

/// The overview's pixel (continuous, pixel centers at whole numbers) showing global land-cell
/// coordinates `(u, v)` east and north: the overview is the world picture of `3` pixels a land
/// cell and `vertices * 3` a side, its pixel `i` at cell `(i - 1) / 3` east and its row `j` at
/// cell `last - (j - 1) / 3` north, shrunk to `side` pixels.
#[must_use]
pub fn overview_at(u: f64, v: f64, vertices: usize, side: usize) -> (f64, f64) {
    let picture = (vertices * 3) as f64;
    let last = (vertices - 1) as f64;
    let scale = side as f64 / picture;
    let i = 3.0 * u + 1.0;
    let j = 3.0 * (last - v) + 1.0;
    ((i + 0.5) * scale - 0.5, (j + 0.5) * scale - 0.5)
}

/// The map picture: `retail` (RGB, `w` x `h`) with every box pixel replaced by the mean of the
/// overview's pixels (BGRA, `side` a side, of a world of `vertices` a side) that show the land
/// its markers come from.
#[must_use]
pub fn compose(
    retail: &[u8],
    (w, h): (usize, usize),
    bx: MarkerBox,
    overview: &[u8],
    side: usize,
    vertices: usize,
) -> Vec<u8> {
    let mut out = retail.to_vec();
    for py in bx.y0..=bx.y1.min(h - 1) {
        for px in bx.x0..=bx.x1.min(w - 1) {
            let ((e0, e1), (n0, n1)) = bx.cells_of(px, py);
            // The overview pixels whose centers fall in the span; north is up, so the northern
            // edge is the smaller row.
            let (xa, ya) = overview_at(e0, n1, vertices, side);
            let (xb, yb) = overview_at(e1, n0, vertices, side);
            let span = |a: f64, b: f64| -> (usize, usize) {
                let lo = a.ceil().max(0.0);
                let hi = (b.ceil() - 1.0).min(side as f64 - 1.0);
                // LINT-OK: inside the overview.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (lo, hi) = (lo as usize, hi.max(lo) as usize);
                (lo.min(side - 1), hi.min(side - 1))
            };
            let ((x0, x1), (y0, y1)) = (span(xa, xb), span(ya, yb));
            let mut sum = [0u64; 3];
            let mut n = 0u64;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let k = (y * side + x) * 4;
                    sum[0] += u64::from(overview[k + 2]);
                    sum[1] += u64::from(overview[k + 1]);
                    sum[2] += u64::from(overview[k]);
                    n += 1;
                }
            }
            let k = (py * w + px) * 3;
            for c in 0..3 {
                // LINT-OK: a mean of bytes.
                #[allow(clippy::cast_possible_truncation)]
                let v = ((sum[c] + n / 2) / n.max(1)) as u8;
                out[k + c] = v;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's map picture)
    use super::*;

    /// The window's own placement of a marker of no size at map coordinates `(ew, ns)`.
    fn place(bx: MarkerBox, ew: f64, ns: f64) -> (i64, i64) {
        let biased = |c: f64| c * 10.0 + 1024.0;
        #[allow(clippy::cast_possible_truncation)]
        let x = bx.x0 as i64 + (biased(ew) * bx.w() / 2048.0) as i64;
        #[allow(clippy::cast_possible_truncation)]
        let y = bx.y0 as i64 + ((2047.0 - biased(ns)) * bx.h() / 2048.0) as i64;
        (x, y)
    }

    /// The map coordinates a player in global land cell `(lx, ly)` is shown at.
    fn coords(lx: i64, ly: i64) -> (f64, f64) {
        (
            (lx - 1024) as f64 * 0.1 + 0.5,
            (ly - 1024) as f64 * 0.1 + 0.5,
        )
    }

    #[test]
    fn a_player_marker_lands_on_the_box_pixel_that_shows_its_cell() {
        let bx = SHIPPED;
        for lx in (0..2040).step_by(7) {
            for ly in (0..2040).step_by(11) {
                let (ew, ns) = coords(lx, ly);
                let (x, y) = place(bx, ew, ns);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (x, y) = (x as usize, y as usize);
                if !bx.holds(x, y) {
                    continue;
                }
                let ((e0, e1), (n0, n1)) = bx.cells_of(x, y);
                // The cell's middle lies in the span the pixel shows, give or take the half
                // cell a marker can stand from its cell's middle.
                let (cx, cy) = (lx as f64 + 0.5, ly as f64 + 0.5);
                assert!(e0 - 0.5 <= cx && cx <= e1 + 0.5, "east {lx}: {e0}..{e1}");
                assert!(n0 - 0.5 <= cy && cy <= n1 + 0.5, "north {ly}: {n0}..{n1}");
            }
        }
    }

    /// Land taking a fifth of the world's width is shown by the pages stretched five times, and
    /// AC2's land about 1.4 times; the picture is made twice as large again as that, for an
    /// interface drawn twice as large: AC2's four times. Land filling the world is made twice its
    /// size; no more than the most however small the land.
    #[test]
    fn the_picture_is_made_twice_as_large_as_the_pages_stretch_the_land() {
        assert_eq!(enlargement(4374.0, 6123.0), 4, "AC2's land");
        assert_eq!(enlargement(3100.0, 6123.0), 4, "1.98 times, rounded up");
        assert_eq!(enlargement(3000.0, 6123.0), 6, "2.04 times, rounded up");
        assert_eq!(enlargement(6123.0, 6123.0), 2);
        assert_eq!(enlargement(1224.6, 6123.0), MOST_ENLARGED);
        assert_eq!(enlargement(10.0, 6123.0), MOST_ENLARGED);
        assert_eq!(enlargement(0.0, 6123.0), 2);
    }

    /// An enlarged picture's box is the same part of it: each of the box's pixels the block of
    /// pixels it became, so the land under each is the same.
    #[test]
    fn an_enlarged_box_covers_the_pixels_its_pixels_became() {
        let bx = SHIPPED.scaled(5);
        assert_eq!((bx.x0, bx.x1, bx.y0, bx.y1), (30, 1239, 40, 1294));
        let (a, b) = (SHIPPED.cells_of(100, 100), bx.cells_of(500, 500));
        assert!((a.0 .0 - b.0 .0).abs() < 1e-9 && (a.1 .1 - b.1 .1).abs() < 1e-9);
        assert_eq!(SHIPPED.scaled(1), SHIPPED);
    }

    /// Enlarging keeps a plain picture plain and its size times the enlargement, and blends
    /// across an edge rather than stepping.
    #[test]
    fn an_enlarged_picture_is_k_times_the_size_and_blended() {
        let (w, h) = (4, 2);
        // The left half black, the right half white.
        let rgb: Vec<u8> = (0..w * h)
            .flat_map(|i| [if i % w < 2 { 0 } else { 255 }; 3])
            .collect();
        let big = enlarge(&rgb, (w, h), 4);
        assert_eq!(big.len(), w * 4 * h * 4 * 3);
        let at = |x: usize, y: usize| big[(y * w * 4 + x) * 3];
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(15, 7), 255);
        let edge: Vec<u8> = (4..12).map(|x| at(x, 3)).collect();
        assert!(edge.windows(2).all(|p| p[0] <= p[1]), "{edge:?}");
        assert!(edge.iter().any(|&v| v > 0 && v < 255), "blended: {edge:?}");
    }

    #[test]
    fn the_box_runs_west_to_east_and_north_to_south() {
        let bx = SHIPPED;
        let ((w0, _), (_, n_top)) = bx.cells_of(bx.x0, bx.y0);
        let ((_, e1), (s0, _)) = bx.cells_of(bx.x1, bx.y1);
        assert!(w0 < 0.0 && e1 > 2040.0, "{w0} {e1}");
        assert!(n_top > 2040.0 && s0 < 0.0, "{n_top} {s0}");
        // Spans tile the box with no gap.
        assert_eq!(
            bx.cells_of(bx.x0, bx.y0).0 .1,
            bx.cells_of(bx.x0 + 1, bx.y0).0 .0
        );
        assert_eq!(
            bx.cells_of(bx.x0, bx.y0).1 .0,
            bx.cells_of(bx.x0, bx.y0 + 1).1 .1
        );
    }

    #[test]
    fn the_overview_shows_the_south_west_corner_at_its_bottom_left() {
        let (x, y) = overview_at(0.0, 0.0, 2041, 2048);
        assert!(x.abs() < 1.0 && (y - 2047.0).abs() < 1.0, "{x} {y}");
        let (x, y) = overview_at(2040.0, 2040.0, 2041, 2048);
        assert!((x - 2047.0).abs() < 1.0 && y.abs() < 1.0, "{x} {y}");
    }

    #[test]
    fn only_the_box_is_redrawn_and_it_shows_the_land_under_it() {
        // A 2048-pixel overview, the world's east half red, west half blue.
        let side = 2048;
        let mut ov = vec![0u8; side * side * 4];
        for y in 0..side {
            for x in 0..side {
                let k = (y * side + x) * 4;
                if x >= side / 2 {
                    ov[k + 2] = 255;
                } else {
                    ov[k] = 255;
                }
            }
        }
        let (w, h) = (257, 267);
        let retail = vec![7u8; w * h * 3];
        let out = compose(&retail, (w, h), SHIPPED, &ov, side, 2041);
        let at = |x: usize, y: usize| &out[(y * w + x) * 3..(y * w + x) * 3 + 3];
        assert_eq!(at(0, 0), [7, 7, 7]);
        assert_eq!(at(250, 100), [7, 7, 7]);
        assert_eq!(at(10, 100), [0, 0, 255]);
        assert_eq!(at(240, 100), [255, 0, 0]);
    }
}
