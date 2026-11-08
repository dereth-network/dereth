//! What a dead creature leaves in its corpse, by its death-loot profile: how many items (a chance
//! for each count), the items they are drawn from (each with a weight), and items with a chance of
//! their own on top of the draw. Fixed drops with their rates are the creature template's own and
//! are not in this table. Every row says where it came from.
//!
//! Beside the profiles, the death treasures: a fixed item that every creature or chest of a
//! template, or every creature a camp keeps, leaves at a chance of its own.
//!
//! The text form is tab-separated, one profile per line; the death treasures follow under a header
//! of their own, one a line.

use std::fmt::Write as _;

/// One death-loot profile.
#[derive(Debug, Clone, PartialEq)]
pub struct LootProfile {
    /// The profile's name (a generator-profile name).
    pub name: String,
    /// The chance of 0, 1, 2, ... items.
    pub counts: Vec<f32>,
    /// The item templates a draw picks from, with their weights.
    pub items: Vec<(u32, u32)>,
    /// Item templates with a chance of their own, on top of the draw.
    pub extras: Vec<(u32, f32)>,
    pub placed: String,
    pub confidence: String,
    /// What a chest's draw is: the level it is treasure of, and the worth each piece draws from.
    pub chest: Option<ChestTreasure>,
}

/// What a chest of a profile holds as treasure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChestTreasure {
    pub level: i32,
    pub value: (u32, u32),
}

/// A chest's column: `LEVEL:LOW:HIGH`, or empty.
fn chest(s: &str) -> Result<Option<ChestTreasure>, String> {
    if s.is_empty() {
        return Ok(None);
    }
    let f: Vec<&str> = s.split(':').collect();
    let [level, low, high] = f[..] else {
        return Err(format!("a chest is LEVEL:LOW:HIGH, not {s}"));
    };
    let bad = |e: std::num::ParseIntError| format!("{s}: {e}");
    Ok(Some(ChestTreasure {
        level: level.parse().map_err(bad)?,
        value: (low.parse().map_err(bad)?, high.parse().map_err(bad)?),
    }))
}

/// Who leaves a death treasure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dropper {
    /// Every creature or chest of a template.
    Template(u32),
    /// Every creature a camp keeps, by the camp's id in the spawn table.
    Camp(String),
}

/// A fixed item a creature leaves at its death, or a chest at an opening, at a chance of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathTreasure {
    pub from: Dropper,
    pub item: u32,
    /// The chance it is left, from above 0 to 1.
    pub chance: f32,
    pub placed: String,
    pub confidence: String,
}

/// Every profile, by name, and the death treasures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LootTable {
    pub profiles: Vec<LootProfile>,
    pub treasures: Vec<DeathTreasure>,
}

/// The table's columns, in order.
pub const COLUMNS: [&str; 7] = [
    "profile",
    "counts",
    "items",
    "extras",
    "placed",
    "confidence",
    "chest",
];

/// The death treasures' columns, in order; their header line starts their part of the text.
pub const TREASURE_COLUMNS: [&str; 5] = ["treasure from", "item", "chance", "placed", "confidence"];

fn hex(s: &str) -> Result<u32, String> {
    u32::from_str_radix(s, 16).map_err(|e| format!("{s}: {e}"))
}

impl LootTable {
    /// The profile of a name.
    #[must_use]
    pub fn profile(&self, name: &str) -> Option<&LootProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// The death treasures of a creature or chest of a template, kept by a camp of an id or none.
    pub fn treasures_of<'a>(
        &'a self,
        template: u32,
        camp: Option<&'a str>,
    ) -> impl Iterator<Item = &'a DeathTreasure> + 'a {
        self.treasures.iter().filter(move |t| match &t.from {
            Dropper::Template(x) => *x == template,
            Dropper::Camp(c) => Some(c.as_str()) == camp,
        })
    }

    /// The table as text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = COLUMNS.join("\t");
        out.push('\n');
        for p in &self.profiles {
            let counts = p
                .counts
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let items = p
                .items
                .iter()
                .map(|(t, w)| format!("{t:08X}*{w}"))
                .collect::<Vec<_>>()
                .join(",");
            let extras = p
                .extras
                .iter()
                .map(|(t, r)| format!("{t:08X}:{r}"))
                .collect::<Vec<_>>()
                .join(",");
            let _ = writeln!(
                out,
                "{}\t{counts}\t{items}\t{extras}\t{}\t{}\t{}",
                p.name,
                p.placed.replace(['\t', '\n'], " "),
                p.confidence,
                p.chest.map_or_else(String::new, |c| format!(
                    "{}:{}:{}",
                    c.level, c.value.0, c.value.1
                ))
            );
        }
        if !self.treasures.is_empty() {
            out.push_str(&TREASURE_COLUMNS.join("\t"));
            out.push('\n');
            for t in &self.treasures {
                let from = match &t.from {
                    Dropper::Template(x) => format!("template {x:08X}"),
                    Dropper::Camp(c) => format!("camp {c}"),
                };
                let _ = writeln!(
                    out,
                    "{from}\t{:08X}\t{}\t{}\t{}",
                    t.item,
                    t.chance,
                    t.placed.replace(['\t', '\n'], " "),
                    t.confidence
                );
            }
        }
        out
    }

    /// A death treasure's line.
    fn treasure(c: &[&str]) -> Result<DeathTreasure, String> {
        if c.len() != TREASURE_COLUMNS.len() {
            return Err(format!(
                "{} columns, not {}",
                c.len(),
                TREASURE_COLUMNS.len()
            ));
        }
        let from = match c[0].split_once(' ') {
            Some(("template", t)) => Dropper::Template(hex(t)?),
            Some(("camp", id)) if !id.is_empty() => Dropper::Camp(id.to_string()),
            _ => {
                return Err(format!(
                    "a treasure is a template's or a camp's, not {}",
                    c[0]
                ))
            }
        };
        Ok(DeathTreasure {
            from,
            item: hex(c[1])?,
            chance: c[2].parse::<f32>().map_err(|e| format!("{}: {e}", c[2]))?,
            placed: c[3].to_string(),
            confidence: c[4].to_string(),
        })
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A line without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut profiles = Vec::new();
        let mut treasures = Vec::new();
        let header = TREASURE_COLUMNS.join("\t");
        let mut in_treasures = false;
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            if line == header {
                in_treasures = true;
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            if in_treasures {
                treasures.push(Self::treasure(&c).map_err(at)?);
                continue;
            }
            if c.len() != COLUMNS.len() {
                return Err(at(format!("{} columns, not {}", c.len(), COLUMNS.len())));
            }
            let counts = c[1]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<f32>().map_err(|e| format!("{s}: {e}")))
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            let items = c[2]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (t, w) = s.split_once('*').unwrap_or((s, "1"));
                    Ok((hex(t)?, w.parse::<u32>().map_err(|e| format!("{w}: {e}"))?))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            let extras = c[3]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    let (t, r) = s
                        .split_once(':')
                        .ok_or_else(|| format!("an extra is template:chance, not {s}"))?;
                    Ok((hex(t)?, r.parse::<f32>().map_err(|e| format!("{r}: {e}"))?))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            profiles.push(LootProfile {
                name: c[0].to_string(),
                counts,
                items,
                extras,
                placed: c[4].to_string(),
                confidence: c[5].to_string(),
                chest: chest(c[6]).map_err(at)?,
            });
        }
        Ok(Self {
            profiles,
            treasures,
        })
    }
}
