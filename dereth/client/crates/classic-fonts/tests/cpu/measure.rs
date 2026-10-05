//! How far one font atlas is from another, glyph by glyph: advances, pixels, line metrics.
//! Shared by the comparison tests and the comparison example.

use dereth_classic_dat::fonts::{FontAtlas, Glyph};
use std::collections::BTreeMap;

/// The differences between two atlases of the same request.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Characters both atlases draw.
    pub glyphs: usize,
    /// Advances equal, one pixel apart, further apart.
    pub exact: usize,
    pub one: usize,
    pub worse: usize,
    /// Glyphs whose pixels are all equal.
    pub identical: usize,
    /// Pixels inked in either atlas, placed from the pen.
    pub pixels: usize,
    /// Of those, pixels whose coverage differs at all, and pixels on (half coverage or more) in
    /// one and off in the other.
    pub differing: usize,
    pub on_off: usize,
    /// The coverage differences summed, and the reference atlas's coverage summed.
    pub error: u64,
    pub ink: u64,
}

impl Tally {
    pub fn add(&mut self, o: &Tally) {
        self.glyphs += o.glyphs;
        self.exact += o.exact;
        self.one += o.one;
        self.worse += o.worse;
        self.identical += o.identical;
        self.pixels += o.pixels;
        self.differing += o.differing;
        self.on_off += o.on_off;
        self.error += o.error;
        self.ink += o.ink;
    }

    /// The coverage differences as a share of the reference's ink, in percent.
    #[allow(clippy::cast_precision_loss)]
    pub fn ink_error(&self) -> f64 {
        100.0 * self.error as f64 / self.ink.max(1) as f64
    }

    #[allow(clippy::cast_precision_loss, dead_code)]
    pub fn percent(n: usize, of: usize) -> f64 {
        100.0 * n as f64 / of.max(1) as f64
    }
}

/// A glyph's inked pixels, by their place from the pen on the baseline.
pub fn ink(atlas: &FontAtlas, g: &Glyph) -> BTreeMap<(i32, i32), u8> {
    let width = i32::try_from(atlas.width).unwrap();
    let mut out = BTreeMap::new();
    for y in 0..g.height {
        for x in 0..g.width {
            let at = usize::try_from(((g.y + y) * width + g.x + x) * 4 + 3).unwrap();
            let a = atlas.rgba[at];
            if a != 0 {
                out.insert((g.bearing_x + x, g.bearing_y + y), a);
            }
        }
    }
    out
}

/// `ours` measured against `reference`.
pub fn compare(reference: &FontAtlas, ours: &FontAtlas) -> Tally {
    let mut t = Tally::default();
    for (code, g) in &reference.glyphs {
        let Some(o) = ours.glyphs.get(code) else {
            continue;
        };
        t.glyphs += 1;
        match (g.advance - o.advance).abs() {
            0 => t.exact += 1,
            1 => t.one += 1,
            _ => t.worse += 1,
        }
        let a = ink(reference, g);
        let b = ink(ours, o);
        let mut keys: Vec<_> = a.keys().chain(b.keys()).copied().collect();
        keys.sort_unstable();
        keys.dedup();
        let mut same = true;
        for k in keys {
            let (x, y) = (
                a.get(&k).copied().unwrap_or(0),
                b.get(&k).copied().unwrap_or(0),
            );
            t.pixels += 1;
            t.ink += u64::from(x);
            if x != y {
                t.differing += 1;
                t.error += u64::from(x.abs_diff(y));
                same = false;
            }
            if (x >= 128) != (y >= 128) {
                t.on_off += 1;
            }
        }
        if same {
            t.identical += 1;
        }
    }
    t
}
