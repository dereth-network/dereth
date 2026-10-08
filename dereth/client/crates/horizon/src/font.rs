//! The font tables the interface's text is drawn from: which glyph is where on the glyph pages,
//! how far the pen moves after each, and the kerning between pairs.

use std::collections::HashMap;

/// One glyph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    /// The glyph page it is on.
    pub page: u16,
    pub x: u16,
    pub y: u16,
    pub w: u8,
    pub h: u8,
    /// Added to the glyph's width to give the pen's advance.
    pub advance_adjust: i8,
    /// Added to the pen's top to place the glyph.
    pub y_offset: i8,
}

impl Glyph {
    /// How far the pen moves after this glyph.
    #[must_use]
    pub fn advance(&self) -> i32 {
        i32::from(self.w) + i32::from(self.advance_adjust)
    }
}

/// A font at one size.
#[derive(Debug, Clone, Default)]
pub struct Font {
    pub point_size: f32,
    pub line_height: i32,
    pub ascent: i32,
    pub glyphs: HashMap<char, Glyph>,
    pub kerning: HashMap<(char, char), i32>,
}

impl Font {
    /// The glyph for `ch`, or `None` when the font has none.
    #[must_use]
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.get(&ch)
    }

    /// The kerning between `left` and `right`.
    #[must_use]
    pub fn kern(&self, left: char, right: char) -> i32 {
        self.kerning.get(&(left, right)).copied().unwrap_or(0)
    }

    /// The width of `text` in this font's pixels.
    #[must_use]
    pub fn measure(&self, text: &str) -> i32 {
        let mut w = 0;
        let mut prev = None;
        for ch in text.chars() {
            if let Some(g) = self.glyph(ch).or_else(|| self.glyph('?')) {
                if let Some(p) = prev {
                    w += self.kern(p, ch);
                }
                w += g.advance();
            }
            prev = Some(ch);
        }
        w
    }
}
