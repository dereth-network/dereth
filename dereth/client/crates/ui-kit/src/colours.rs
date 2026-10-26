//! The kit's colours: a small table of rows the widgets draw text with, and the few colours it
//! names itself.

use crate::draw::Argb;

/// The rows of the colour table the interface draws with.
pub mod row {
    /// White body text.
    pub const WHITE: u32 = 1;
    /// Pale grey text.
    pub const GREY: u32 = 2;
    /// Black edge.
    pub const BLACK: u32 = 7;
    /// Cream heading text.
    pub const CREAM: u32 = 8;
    /// Dark brown edge of cream headings.
    pub const HEADING_EDGE: u32 = 9;
    /// The blue glow edge of the title and lobby menus.
    pub const MENU_GLOW: u32 = 37;
}

/// The interface's colours.
#[derive(Debug, Clone)]
pub struct Colours {
    /// Each row's colour, `0xAARRGGBB`.
    rows: Vec<Argb>,
}

/// The rows the interface draws with: its own colours, set to sit with its bronze art (a dark
/// warm edge for the menus' glow).
fn own_rows() -> Vec<Argb> {
    let mut rows = vec![0xFFFF_FFFF; row::MENU_GLOW as usize + 1];
    rows[row::WHITE as usize] = 0xFFFF_FFFF;
    rows[row::GREY as usize] = 0xFFC8_C4BA;
    rows[row::BLACK as usize] = 0xFF00_0000;
    rows[row::CREAM as usize] = 0xFFEE_E1C5;
    rows[row::HEADING_EDGE as usize] = 0xFF2A_1C10;
    rows[row::MENU_GLOW as usize] = 0xFF2A_1C10;
    rows
}

impl Default for Colours {
    fn default() -> Self {
        Self { rows: own_rows() }
    }
}

impl Colours {
    /// The game's own text colours, for a skin drawn in the game's font: parchment text, a
    /// dimmer parchment, and no edge round the letters.
    #[must_use]
    pub fn retail_plain() -> Self {
        Self::plain_text(0xFFE8_D8A8, 0xFF8A_806A)
    }

    /// Text in `text`, dimmed text in `dim`, headings as the text, and no edge round the letters:
    /// the colours of a skin whose own font draws its text.
    #[must_use]
    pub fn plain_text(text: Argb, dim: Argb) -> Self {
        let mut rows = own_rows();
        rows[row::WHITE as usize] = text;
        rows[row::GREY as usize] = dim;
        rows[row::BLACK as usize] = 0;
        rows[row::CREAM as usize] = text;
        rows[row::HEADING_EDGE as usize] = 0;
        rows[row::MENU_GLOW as usize] = 0;
        Self { rows }
    }
}

impl Colours {
    /// Row `row`, or white when the table has no such row.
    #[must_use]
    pub fn ui(&self, row: u32) -> Argb {
        self.rows.get(row as usize).copied().unwrap_or(0xFFFF_FFFF)
    }

    /// Body text.
    #[must_use]
    pub fn text(&self) -> Argb {
        self.ui(row::WHITE)
    }

    /// Body text's edge.
    #[must_use]
    pub fn edge(&self) -> Argb {
        self.ui(row::BLACK)
    }

    /// Window titles and headings: cream.
    #[must_use]
    pub fn heading(&self) -> Argb {
        self.ui(row::CREAM)
    }

    /// Dimmed text.
    #[must_use]
    pub fn dim(&self) -> Argb {
        self.ui(row::GREY)
    }
}
