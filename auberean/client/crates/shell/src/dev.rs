//! Developer affordances, never a player's: a login driven from the command line, and a status
//! overlay drawn in a small built-in pixel font. Both are off unless asked for.
//!
//! **The command-line login** ([`DevLogin`]) is an [`Interface`] with no screen: it connects to the
//! server with the account given, enters the world as the character named (making it first, a
//! human, when the account has none of that name), or as the first character when none is named.
//! It is how the world is tried before the game's own interface exists.
//!
//! **The overlay** ([`overlay`]) writes a few lines over the world's top left: the session's phase,
//! the player's cell, place, heading and speed, the bodies drawn, the frame rate, the selection,
//! the player's health and vigor, and the last lines heard.

use core::net::SocketAddr;

use auberean_client_sdk::protocol::table::SMALL;
use auberean_client_sdk::{Action, NewCharacter};

use crate::interface::{Game, Interface, Overlay, Phase, Request};

/// A human's choices, as the creation screen sends them for a human with its defaults: the
/// entity, the start area, the race (1), male (`0x1000`) or female (`0x2000`), the class and the
/// implement, and a body of build and height 1.
#[must_use]
pub fn human(name: &str, female: bool) -> NewCharacter {
    NewCharacter {
        name: name.encode_utf16().collect(),
        entity: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: if female { 0x2000 } else { 0x1000 },
        class: 2,
        implement: 1,
        physique: vec![(1, 1.0), (2, 1.0)],
        physique_size_class: SMALL,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Start,
    Connecting,
    Creating,
    Entering,
    Playing,
    Done,
}

/// The command-line login.
#[derive(Clone, Debug)]
pub struct DevLogin {
    pub server: SocketAddr,
    pub account: String,
    pub password: String,
    /// The character to play; the first listed when `None`.
    pub character: Option<String>,
    stage: Stage,
    /// The refusals of an entry seen, and when to ask again after the last.
    refusals: u32,
    retry_at: f64,
}

/// How long the command-line login waits after an entry is refused (the character still in the
/// world) before it asks again.
const RETRY_AFTER: f64 = 2.0;

impl DevLogin {
    #[must_use]
    pub fn new(
        server: SocketAddr,
        account: String,
        password: String,
        character: Option<String>,
    ) -> Self {
        Self {
            server,
            account,
            password,
            character,
            stage: Stage::Start,
            refusals: 0,
            retry_at: 0.0,
        }
    }
}

impl Interface for DevLogin {
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        // An entry refused: back at the list, asked again a little later.
        if let Some((reason, n)) = game.client.and_then(|c| c.model.refused) {
            if n != self.refusals {
                self.refusals = n;
                if self.stage == Stage::Entering {
                    tracing::info!(reason, "the entry was refused; asking again");
                    self.stage = Stage::Creating;
                    self.retry_at = game.now + RETRY_AFTER;
                }
            }
        }
        if game.now < self.retry_at {
            return;
        }
        match (self.stage, game.phase) {
            (Stage::Start, Phase::Disconnected) => {
                tracing::info!(server = %self.server, account = %self.account, "logging in");
                requests.push(Request::Connect {
                    server: self.server,
                    account: self.account.clone(),
                    password: self.password.clone(),
                });
                self.stage = Stage::Connecting;
            }
            (Stage::Connecting | Stage::Creating, Phase::Characters) => {
                let Some(list) = game.client.and_then(|c| c.model.characters.as_ref()) else {
                    return;
                };
                let live = list.characters.iter().filter(|c| c.seconds_greyed_out == 0);
                let pick = match &self.character {
                    Some(name) => live
                        .clone()
                        .find(|c| c.name.text().eq_ignore_ascii_case(name)),
                    None => live.clone().next(),
                };
                if let Some(c) = pick {
                    tracing::info!(character = %c.name.text(), id = format_args!("{:#x}", c.id), "entering the world");
                    requests.push(Request::Act(Action::PlayCharacter { character: c.id }));
                    self.stage = Stage::Entering;
                } else if self.stage == Stage::Connecting {
                    let name = self
                        .character
                        .clone()
                        .unwrap_or_else(|| "Wanderer".to_owned());
                    tracing::info!(character = %name, "making a human");
                    requests.push(Request::Act(Action::CreateCharacter(human(&name, false))));
                    self.stage = Stage::Creating;
                }
            }
            (Stage::Entering, Phase::World) => self.stage = Stage::Playing,
            (Stage::Playing, Phase::Characters) => self.stage = Stage::Done,
            _ => {}
        }
    }
}

/// The pixel font: 3 by 5 pixels a glyph, rows top first, the top row in the high bits.
const GLYPHS: &[(char, u16)] = &[
    ('0', 0b111_101_101_101_111),
    ('1', 0b010_110_010_010_111),
    ('2', 0b111_001_111_100_111),
    ('3', 0b111_001_111_001_111),
    ('4', 0b101_101_111_001_001),
    ('5', 0b111_100_111_001_111),
    ('6', 0b111_100_111_101_111),
    ('7', 0b111_001_001_010_010),
    ('8', 0b111_101_111_101_111),
    ('9', 0b111_101_111_001_111),
    ('A', 0b010_101_111_101_101),
    ('B', 0b110_101_110_101_110),
    ('C', 0b011_100_100_100_011),
    ('D', 0b110_101_101_101_110),
    ('E', 0b111_100_110_100_111),
    ('F', 0b111_100_110_100_100),
    ('G', 0b011_100_101_101_011),
    ('H', 0b101_101_111_101_101),
    ('I', 0b111_010_010_010_111),
    ('J', 0b001_001_001_101_010),
    ('K', 0b101_101_110_101_101),
    ('L', 0b100_100_100_100_111),
    ('M', 0b101_111_111_101_101),
    ('N', 0b110_101_101_101_101),
    ('O', 0b010_101_101_101_010),
    ('P', 0b110_101_110_100_100),
    ('Q', 0b010_101_101_110_011),
    ('R', 0b110_101_110_101_101),
    ('S', 0b011_100_010_001_110),
    ('T', 0b111_010_010_010_010),
    ('U', 0b101_101_101_101_111),
    ('V', 0b101_101_101_101_010),
    ('W', 0b101_101_111_111_101),
    ('X', 0b101_101_010_101_101),
    ('Y', 0b101_101_010_010_010),
    ('Z', 0b111_001_010_100_111),
    ('.', 0b000_000_000_000_010),
    (',', 0b000_000_000_010_100),
    (':', 0b000_010_000_010_000),
    ('-', 0b000_000_111_000_000),
    ('+', 0b000_010_111_010_000),
    ('/', 0b001_001_010_100_100),
    ('(', 0b010_100_100_100_010),
    (')', 0b010_001_001_001_010),
    ('[', 0b110_100_100_100_110),
    (']', 0b011_001_001_001_011),
    ('%', 0b101_001_010_100_101),
    ('=', 0b000_111_000_111_000),
    ('?', 0b110_001_010_000_010),
    ('!', 0b010_010_010_000_010),
    ('\'', 0b010_010_000_000_000),
    ('"', 0b101_101_000_000_000),
    ('#', 0b101_111_101_111_101),
    ('_', 0b000_000_000_000_111),
    ('<', 0b001_010_100_010_001),
    ('>', 0b100_010_001_010_100),
];

/// The pixels of a character's glyph (an unknown one is blank).
#[must_use]
pub fn glyph(c: char) -> u16 {
    let c = c.to_ascii_uppercase();
    GLYPHS
        .iter()
        .find(|(g, _)| *g == c)
        .map_or(0, |(_, bits)| *bits)
}

/// Write `text` at `(x, y)` (its top left, window pixels) with pixels `scale` wide.
pub fn text(o: &mut Overlay, x: f32, y: f32, scale: f32, color: [f32; 4], text: &str) {
    let mut cx = x;
    for ch in text.chars() {
        let bits = glyph(ch);
        for row in 0..5u16 {
            for col in 0..3u16 {
                if bits & (1 << (14 - (row * 3 + col))) != 0 {
                    let (px, py) = (cx + f32::from(col) * scale, y + f32::from(row) * scale);
                    o.fill([px, py, px + scale, py + scale], color);
                }
            }
        }
        cx += 4.0 * scale;
    }
}

/// The status overlay's lines, over a dark band.
pub fn overlay(o: &mut Overlay, lines: &[String]) {
    const SCALE: f32 = 2.0;
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (
        longest as f32 * 4.0 * SCALE + 12.0,
        lines.len() as f32 * 7.0 * SCALE + 10.0,
    );
    o.fill([0.0, 0.0, w, h], [0.0, 0.0, 0.0, 0.55]);
    for (i, l) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = 6.0 + i as f32 * 7.0 * SCALE;
        text(o, 6.0, y, SCALE, [0.95, 0.92, 0.75, 1.0], l);
    }
}
