//! The interface colours: a small table of rows the interface draws with, and the few colours it
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
        0xFFEE_E1C5
    }

    /// Dimmed text.
    #[must_use]
    pub fn dim(&self) -> Argb {
        self.ui(row::GREY)
    }
}

/// The log window's colour for one of the game's chat types: speech, tells, the fellowship, the
/// allegiance, the chat channels and the game's own messages each in a colour of their own.
#[must_use]
pub fn chat_colour(chat_type: u8) -> Argb {
    match chat_type {
        // Speech: say.
        2 => 0xFFF7_F7F7,
        // Tells, both ways.
        3 | 4 | 31 => 0xFFFF_B8DE,
        // System messages.
        5 => 0xFFCC_CCCC,
        // Combat: the battle log's colours.
        6 | 21 => 0xFFFF_7B7B,
        22 => 0xFFFF_A0A0,
        // Magic and spellcasting.
        7 | 17 => 0xFFA0_C8FF,
        // The chat channels: green.
        0 | 1 | 8 | 9 | 20 => 0xFFD4_FF7D,
        // Social: shout.
        10 | 11 => 0xFFFF_A666,
        // Emotes.
        12 => 0xFFBA_FFF0,
        // Advancement: the level-up gold.
        13 => 0xFFFF_DE73,
        // Allegiance.
        18 | 33 => 0xFFAB_DBE5,
        // Fellowship: party.
        19 => 0xFF66_E5FF,
        // Client feedback and errors.
        15 | 26 => 0xFFFF_4A4A,
        _ => 0xFFCC_CCCC,
    }
}
