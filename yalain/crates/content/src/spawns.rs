//! Where creatures spawn: camps, each standing for a generator the world keeps alive. A camp has a
//! place, a spread (outdoors a creature appears anywhere within its radius; indoors at one of its
//! spots), the templates it makes and how often each, the number it keeps alive and how long it
//! waits before making a new one. Every row says where it came from: measured from the recorded
//! sessions, or authored.
//!
//! A camp may also set how its creatures move over their templates' settings, and name a route
//! they walk at rest ([`crate::movement`]).
//!
//! The text form is tab-separated, one camp per line, so the table can be read and edited by hand.

use std::fmt::Write as _;

/// One camp.
#[derive(Debug, Clone, PartialEq)]
pub struct Camp {
    pub id: String,
    /// The cell of its center (an interior cell indoors).
    pub cell: u32,
    /// Its center in the cell's landblock.
    pub origin: [f32; 3],
    /// How far from the center a creature may appear (outdoors), in meters.
    pub radius: f32,
    /// How many creatures it keeps alive.
    pub population: u32,
    /// Seconds between a death and the creature that replaces it.
    pub regen: f64,
    /// The templates it makes, each with its weight.
    pub templates: Vec<(u32, u32)>,
    /// Fixed spots creatures appear at (indoors): a cell and a point.
    pub spots: Vec<(u32, [f32; 3])>,
    /// Where the row came from.
    pub placed: String,
    /// How sure the row is: `high`, `medium`, `low`, or `authored`.
    pub confidence: String,
    /// How its creatures move: settings over their templates', and a route.
    pub moves: crate::movement::CampMoves,
}

/// Every camp of the world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpawnTable {
    pub camps: Vec<Camp>,
}

/// The table's columns, in order. A table written before the last two (a camp's movement
/// settings and route) still reads, its camps setting neither.
pub const COLUMNS: [&str; 14] = [
    "camp",
    "cell",
    "x",
    "y",
    "z",
    "radius",
    "population",
    "regen",
    "templates",
    "spots",
    "placed",
    "confidence",
    "movement",
    "route",
];

/// The columns of a table written before camps set movement.
const COLUMNS_BEFORE_MOVES: usize = 12;

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    s.parse::<T>().map_err(|e| format!("{s}: {e}"))
}

impl SpawnTable {
    /// The table as text: a header line, then one tab-separated line per camp.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = COLUMNS.join("\t");
        out.push('\n');
        for c in &self.camps {
            let templates = c
                .templates
                .iter()
                .map(|(t, w)| format!("{t:08X}*{w}"))
                .collect::<Vec<_>>()
                .join(",");
            let spots = c
                .spots
                .iter()
                .map(|(cell, [x, y, z])| format!("{cell:08X}:{x}:{y}:{z}"))
                .collect::<Vec<_>>()
                .join("|");
            let _ = writeln!(
                out,
                "{}\t{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{templates}\t{spots}\t{}\t{}\t{}\t{}",
                c.id,
                c.cell,
                c.origin[0],
                c.origin[1],
                c.origin[2],
                c.radius,
                c.population,
                c.regen,
                c.placed.replace(['\t', '\n'], " "),
                c.confidence,
                c.moves.settings.to_text(),
                c.moves.route_text(),
            );
        }
        out
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A line without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut camps = Vec::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() != COLUMNS.len() && c.len() != COLUMNS_BEFORE_MOVES {
                return Err(at(format!("{} columns, not {}", c.len(), COLUMNS.len())));
            }
            let moves = if c.len() == COLUMNS.len() {
                crate::movement::CampMoves::from_text(c[12], c[13]).map_err(at)?
            } else {
                crate::movement::CampMoves::default()
            };
            let templates = c[8]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (t, w) = s.split_once('*').unwrap_or((s, "1"));
                    Ok((hex(t)?, num(w)?))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            let spots = c[9]
                .split('|')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let p: Vec<&str> = s.split(':').collect();
                    if p.len() != 4 {
                        return Err(format!("a spot is cell:x:y:z, not {s}"));
                    }
                    Ok((hex(p[0])?, [num(p[1])?, num(p[2])?, num(p[3])?]))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            camps.push(Camp {
                id: c[0].to_string(),
                cell: hex(c[1]).map_err(at)?,
                origin: [
                    num(c[2]).map_err(at)?,
                    num(c[3]).map_err(at)?,
                    num(c[4]).map_err(at)?,
                ],
                radius: num(c[5]).map_err(at)?,
                population: num(c[6]).map_err(at)?,
                regen: num(c[7]).map_err(at)?,
                templates,
                spots,
                placed: c[10].to_string(),
                confidence: c[11].to_string(),
                moves,
            });
        }
        Ok(Self { camps })
    }
}
