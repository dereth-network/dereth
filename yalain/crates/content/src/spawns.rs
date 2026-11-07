//! Where creatures spawn: camps, each standing for a generator the world keeps alive. A camp has a
//! place, a spread (outdoors a creature appears anywhere within its radius; indoors at one of its
//! spots), the templates it makes and how often each, the number it keeps alive and how long it
//! waits before making a new one. Every row says where it came from: measured from the recorded
//! sessions, or authored.
//!
//! A camp may also set how its creatures move over their templates' settings, and name a route
//! they walk at rest ([`crate::movement`]). A camp may give its creatures a name of their own in
//! place of their template's, a string of the install (2017: creatures of two ordinary kinds were
//! told by a name from their kind's own string table, and their corpses were named after it).
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
    /// The name its creatures are told by in place of their template's, as a text reference
    /// (string id, table id).
    pub name: Option<(u32, u32)>,
}

/// Every camp of the world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpawnTable {
    pub camps: Vec<Camp>,
}

/// The table's columns, in order. A table written before the last (a camp's name) still reads, its
/// camps naming none; so does one written before the two before it (a camp's movement settings and
/// route), its camps setting neither.
pub const COLUMNS: [&str; 15] = [
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
    "name",
];

/// The columns of a table written before camps set movement.
const COLUMNS_BEFORE_MOVES: usize = 12;

/// The columns of a table written before camps named their creatures.
const COLUMNS_BEFORE_NAMES: usize = 14;

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
                "{}\t{:08X}\t{}\t{}\t{}\t{}\t{}\t{}\t{templates}\t{spots}\t{}\t{}\t{}\t{}\t{}",
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
                c.name
                    .map(|(sid, table)| format!("{table:08X}:{sid:08X}"))
                    .unwrap_or_default(),
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
            if ![COLUMNS.len(), COLUMNS_BEFORE_NAMES, COLUMNS_BEFORE_MOVES].contains(&c.len()) {
                return Err(at(format!("{} columns, not {}", c.len(), COLUMNS.len())));
            }
            let moves = if c.len() >= COLUMNS_BEFORE_NAMES {
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
            let name = match c.get(14).filter(|n| !n.is_empty()) {
                None => None,
                Some(n) => {
                    let (table, sid) = n
                        .split_once(':')
                        .ok_or_else(|| at(format!("a name is table:string, not {n}")))?;
                    Some((hex(sid).map_err(at)?, hex(table).map_err(at)?))
                }
            };
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
                name,
            });
        }
        Ok(Self { camps })
    }
}
