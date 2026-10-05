//! The classic interface's text: fonts rasterised by the host into a coverage atlas.
//!
//! The classic interface names each of its fonts the way a Windows program does: a height and an
//! average character width in pixels, a weight, italic or not, and a face name. A
//! [`FontSource`] asks the system for that font and draws each printable character of the Western code page
//! (Windows-1252) once, white on black, into a 16 by 14 grid of 64-pixel cells, with a 12-pixel
//! margin inside each cell so no glyph can touch its neighbour. What it returns is the atlas as
//! white RGBA whose alpha is the coverage, and for every character its box in the atlas, its
//! bearings from the pen position on the baseline (y grows downward) and its advance.

use std::collections::BTreeMap;

/// A font as the classic interface asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSpec {
    /// Cell height in pixels.
    pub height: i32,
    /// Average character width in pixels.
    pub width: i32,
    /// Weight: 400 is regular, 700 bold.
    pub weight: i32,
    /// Italic.
    pub italic: bool,
    /// The face name.
    pub face: String,
}

/// One character's place in the atlas and how it sits on the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Glyph {
    /// Left edge of the inked box in the atlas.
    pub x: i32,
    /// Top edge of the inked box in the atlas.
    pub y: i32,
    /// Width of the inked box.
    pub width: i32,
    /// Height of the inked box.
    pub height: i32,
    /// From the pen position to the box's left edge.
    pub bearing_x: i32,
    /// From the baseline to the box's top edge (negative above the baseline).
    pub bearing_y: i32,
    /// How far the pen moves after this character.
    pub advance: i32,
}

/// A rasterised font.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct FontAtlas {
    /// Atlas width in pixels.
    pub width: u32,
    /// Atlas height in pixels.
    pub height: u32,
    /// White pixels whose alpha is the glyph coverage, rows top first.
    pub rgba: Vec<u8>,
    /// Every drawn character, by Unicode scalar value.
    pub glyphs: BTreeMap<u32, Glyph>,
    /// The distance from one line's top to the next's.
    pub line_height: i32,
    /// The distance from a line's top to its baseline.
    pub baseline: i32,
    /// The face the system actually chose.
    pub face: String,
}

impl std::fmt::Debug for FontAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontAtlas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("glyphs", &self.glyphs.len())
            .field("line_height", &self.line_height)
            .field("baseline", &self.baseline)
            .field("face", &self.face)
            .finish_non_exhaustive()
    }
}

/// The fonts the classic interface asks for, by the name its screens use:
/// `(name, height, average width, weight, face)`.
pub const REQUESTS: [(&str, i32, i32, i32, &str); 17] = [
    ("10-4", 10, 4, 500, "Times New Roman"),
    ("14-5", 14, 5, 500, "Times New Roman"),
    ("14-6", 14, 6, 500, "Times New Roman"),
    ("15-5", 15, 5, 500, "Times New Roman"),
    ("15-6", 15, 6, 500, "Times New Roman"),
    ("16-6", 16, 6, 500, "Times New Roman"),
    ("16-7", 16, 7, 500, "Times New Roman"),
    ("20-8", 20, 8, 500, "Times New Roman"),
    ("25-10", 25, 10, 500, "Times New Roman"),
    ("35-16", 35, 16, 500, "Times New Roman"),
    ("courier-14-7", 14, 7, 700, "Courier New"),
    ("arial-14-6", 14, 6, 700, "Arial"),
    ("times-18-7-bold", 18, 7, 700, "Times New Roman"),
    ("times-35-16-heavy", 35, 16, 900, "Times New Roman"),
    ("times-35-13-bold", 35, 13, 700, "Times New Roman"),
    ("times-25-11", 25, 11, 500, "Times New Roman"),
    ("italic-15-6", 15, 6, 500, "Times New Roman Italic"),
];

/// Cell size, margin inside a cell, and the grid.
pub const CELL: i32 = 64;
pub const PAD: i32 = 12;
pub const COLUMNS: i32 = 16;
pub const ROWS: i32 = 14;

/// The Unicode scalar value of a Windows-1252 byte, or `None` for the five bytes the code page
/// leaves undefined.
#[must_use]
pub fn cp1252(byte: u8) -> Option<char> {
    const HIGH: [u16; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    match byte {
        0x80..=0x9F => match HIGH[usize::from(byte - 0x80)] {
            0 => None,
            c => char::from_u32(u32::from(c)),
        },
        _ => Some(char::from(byte)),
    }
}

/// The Windows-1252 byte for a character, if the code page has one.
#[must_use]
pub fn to_cp1252(c: char) -> Option<u8> {
    (0u8..=255).find(|b| cp1252(*b) == Some(c))
}

/// Find each glyph's inked box inside its cell and turn it into atlas coordinates and bearings.
///
/// `cells` holds each character's cell origin and advance; `ascent` is where the baseline sits
/// below the cell's margin. A glyph with no ink gets an empty box at the pen position.
///
/// # Errors
/// A glyph whose ink reaches its cell's edge: the margin was too small for this font.
pub fn measure_cells(
    alpha: &[u8],
    atlas_width: i32,
    cells: &[(u32, i32, i32, i32)],
    ascent: i32,
) -> Result<BTreeMap<u32, Glyph>, String> {
    let mut glyphs = BTreeMap::new();
    for &(codepoint, cx, cy, advance) in cells {
        let (mut left, mut top, mut right, mut bottom) = (CELL, CELL, 0, 0);
        for y in 0..CELL {
            for x in 0..CELL {
                let at = usize::try_from((cy + y) * atlas_width + cx + x).unwrap_or(usize::MAX);
                if alpha.get(at).copied().unwrap_or(0) != 0 {
                    left = left.min(x);
                    top = top.min(y);
                    right = right.max(x + 1);
                    bottom = bottom.max(y + 1);
                }
            }
        }
        if right == 0 {
            (left, right, top, bottom) = (PAD, PAD, PAD + ascent, PAD + ascent);
        } else if left == 0 || top == 0 || right == CELL || bottom == CELL {
            return Err(format!(
                "the glyph for U+{codepoint:04X} touches its cell's edge"
            ));
        }
        glyphs.insert(
            codepoint,
            Glyph {
                x: cx + left,
                y: cy + top,
                width: right - left,
                height: bottom - top,
                bearing_x: left - PAD,
                bearing_y: top - PAD - ascent,
                advance,
            },
        );
    }
    Ok(glyphs)
}

/// A pixel's coverage from the number of its sample points inside a glyph, 0 to 64 (the levels
/// of the Windows font system's grey glyph bitmaps), spread over a byte the way the game's own
/// glyph sheets spread theirs: level `n` is `4n - 1`, so full coverage is 255 and none is 0.
#[must_use]
pub fn coverage(level: u8) -> u8 {
    match level {
        0 => 0,
        64.. => 255,
        n => n * 4 - 1,
    }
}

/// What draws the classic interface's fonts: the host's font system. A host without one gives
/// the interface none, and the interface is not offered.
pub trait FontSource: Send + Sync {
    /// Rasterise `spec`.
    ///
    /// # Errors
    /// When the system cannot make the font.
    fn rasterize(&self, spec: &FontSpec) -> Result<FontAtlas, String>;
}
