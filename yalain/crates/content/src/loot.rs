//! What a dead creature leaves in its corpse, by its death-loot profile: how many items (a chance
//! for each count), the items they are drawn from (each with a weight), and items with a chance of
//! their own on top of the draw. Fixed drops with their rates are the creature template's own and
//! are not in this table. Every row says where it came from.
//!
//! Beside the profiles, the death treasures: a fixed item that every creature or chest of a
//! template, or every creature a camp keeps, leaves at a chance of its own (a chest's, for its
//! opener alone while a quest is where it needs it); the values a treasure item a creature
//! leaves draws its own value from, by the creature's level; the finds any creature's corpse
//! may hold beside its profile's draw; and the units a treasure weapon, shield, piece of armor or
//! jewel is worth by its least level, times one more than its enchantments.
//!
//! The text form is tab-separated, one profile per line; the death treasures follow under a header
//! of their own, one a line, then the pools, the values, the finds and the units, each under a
//! line of its own.

use std::collections::BTreeMap;
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
    /// The quest a chest's opener must hold, and where, for the chest to hold it at that opening
    /// (whether or not a key paid for it); a creature's corpse holds one for each of its claimants
    /// (its killer and the killer's fellows) holding it there.
    pub requires: Option<crate::QuestGate>,
    pub placed: String,
    pub confidence: String,
}

/// A find any creature's corpse may hold beside its profile's draw: one roll at each kill, at a
/// chance of its own, for a killer and a creature its gates let through; the item is drawn evenly
/// from the band of the killer's level.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpseFind {
    pub key: String,
    /// The chance a corpse its gates let through holds it, from above 0 to 1.
    pub chance: f32,
    /// The template property that, true, keeps a creature from ever leaving it.
    pub never: Option<String>,
    /// The least level of a creature that leaves it.
    pub least_level: Option<i32>,
    /// How many levels below its killer a creature that leaves it may be, at most.
    pub below_killer: Option<i32>,
    /// Whether it is left only for a killer whose hero skill credits, held and spent, are under
    /// its level's cap.
    pub hero_cap: bool,
    /// The items by the killer's least level, the levels rising from 0.
    pub bands: Vec<(i32, Vec<u32>)>,
    pub placed: String,
    pub confidence: String,
}

impl CorpseFind {
    /// The items a killer of a level draws from: those of the last band whose least level it
    /// reaches, else the first band's.
    #[must_use]
    pub fn band(&self, level: i32) -> &[u32] {
        self.bands
            .iter()
            .rev()
            .find(|(from, _)| level >= *from)
            .or(self.bands.first())
            .map_or(&[], |(_, items)| items.as_slice())
    }

    /// Every item it may be.
    pub fn items(&self) -> impl Iterator<Item = u32> + '_ {
        self.bands
            .iter()
            .flat_map(|(_, items)| items.iter().copied())
    }
}

/// Every profile, by name, and the death treasures.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LootTable {
    pub profiles: Vec<LootProfile>,
    pub treasures: Vec<DeathTreasure>,
    pub pools: crate::pools::Pools,
    /// The own values a treasure item a creature of a level leaves is drawn from, by the level:
    /// each value as often as it was given.
    pub values: BTreeMap<i32, Vec<u32>>,
    /// The finds any creature's corpse may hold, in the rows' order.
    pub finds: Vec<CorpseFind>,
    /// The unit a treasure weapon, shield, piece of armor or jewel is worth, from each least level
    /// on: drawn evenly from the least to the most.
    pub units: BTreeMap<i32, (u32, u32)>,
}

/// The line that starts the pools' part of a table's text.
const POOLS_PART: &str = "[pools]";

/// The line that starts the values' part of a table's text, after the pools'.
const VALUES_PART: &str = "[values]";

/// The line that starts the finds' part of a table's text, after the values'.
const FINDS_PART: &str = "[finds]";

/// The line that starts the units' part of a table's text, after the finds'.
const UNITS_PART: &str = "[units]";

/// The finds' columns, in order.
pub const FIND_COLUMNS: [&str; 9] = [
    "find",
    "chance",
    "never",
    "least level",
    "below killer",
    "hero cap",
    "bands",
    "placed",
    "confidence",
];

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
pub const TREASURE_COLUMNS: [&str; 6] = [
    "treasure from",
    "item",
    "chance",
    "placed",
    "confidence",
    "requires",
];

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
                    "{from}\t{:08X}\t{}\t{}\t{}\t{}",
                    t.item,
                    t.chance,
                    t.placed.replace(['\t', '\n'], " "),
                    t.confidence,
                    t.requires.map_or_else(String::new, |g| g.to_text())
                );
            }
        }
        if self.pools != crate::pools::Pools::default() {
            out.push_str(POOLS_PART);
            out.push('\n');
            out.push_str(&self.pools.to_text());
        }
        if !self.values.is_empty() {
            out.push_str(VALUES_PART);
            out.push_str("\nlevel\tvalues\n");
            for (level, values) in &self.values {
                let values: Vec<String> = values.iter().map(ToString::to_string).collect();
                let _ = writeln!(out, "{level}\t{}", values.join(","));
            }
        }
        if !self.finds.is_empty() {
            out.push_str(FINDS_PART);
            out.push('\n');
            out.push_str(&FIND_COLUMNS.join("\t"));
            out.push('\n');
            for f in &self.finds {
                let bands = f
                    .bands
                    .iter()
                    .map(|(from, items)| {
                        let items: Vec<String> = items.iter().map(|t| format!("{t:08X}")).collect();
                        format!("{from}:{}", items.join("+"))
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let num = |v: Option<i32>| v.map_or_else(String::new, |v| v.to_string());
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{bands}\t{}\t{}",
                    f.key,
                    f.chance,
                    f.never.as_deref().unwrap_or_default(),
                    num(f.least_level),
                    num(f.below_killer),
                    if f.hero_cap { "yes" } else { "" },
                    f.placed.replace(['\t', '\n'], " "),
                    f.confidence
                );
            }
        }
        if !self.units.is_empty() {
            out.push_str(UNITS_PART);
            out.push_str("\nleast\tlow\thigh\n");
            for (least, (low, high)) in &self.units {
                let _ = writeln!(out, "{least}\t{low}\t{high}");
            }
        }
        out
    }

    /// The unit an item of a least level is worth: the last unit whose least level it reaches,
    /// else the first; none in a table with no units.
    #[must_use]
    pub fn unit_at(&self, least: i32) -> Option<(u32, u32)> {
        self.units
            .range(..=least)
            .next_back()
            .or_else(|| self.units.iter().next())
            .map(|(_, u)| *u)
    }

    /// The units' part of a table's text.
    fn units_of(text: &str) -> Result<BTreeMap<i32, (u32, u32)>, String> {
        let mut out = BTreeMap::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let at = |e: String| format!("{UNITS_PART} line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            let [least, low, high] = c[..] else {
                return Err(at(format!("a line is LEAST<tab>LOW<tab>HIGH, not {line}")));
            };
            let int = |s: &str| s.parse::<u32>().map_err(|e| at(format!("{s}: {e}")));
            let least: i32 = least.parse().map_err(|e| at(format!("{least}: {e}")))?;
            out.insert(least, (int(low)?, int(high)?));
        }
        Ok(out)
    }

    /// The values a treasure item a creature of a level leaves draws its own value from: its
    /// level's, else the nearest level's that has some (the lower of two as near); none in a table
    /// with no values.
    #[must_use]
    pub fn values_at(&self, level: i32) -> &[u32] {
        let below = self.values.range(..=level).next_back();
        let above = self.values.range(level..).next();
        match (below, above) {
            (Some((b, v)), Some((a, w))) => {
                if i64::from(*a) - i64::from(level) < i64::from(level) - i64::from(*b) {
                    w
                } else {
                    v
                }
            }
            (Some((_, v)), None) | (None, Some((_, v))) => v,
            (None, None) => &[],
        }
    }

    /// The values' part of a table's text.
    fn values_of(text: &str) -> Result<BTreeMap<i32, Vec<u32>>, String> {
        let mut out = BTreeMap::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let at = |e: String| format!("{VALUES_PART} line {}: {e}", n + 1);
            let (level, values) = line
                .split_once('\t')
                .ok_or_else(|| at(format!("a line is LEVEL<tab>VALUES, not {line}")))?;
            let level: i32 = level.parse().map_err(|e| at(format!("{level}: {e}")))?;
            let values = values
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<u32>().map_err(|e| format!("{s}: {e}")))
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            out.insert(level, values);
        }
        Ok(out)
    }

    /// The finds' part of a table's text.
    fn finds_of(text: &str) -> Result<Vec<CorpseFind>, String> {
        let mut out = Vec::new();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let at = |e: String| format!("{FINDS_PART} line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() != FIND_COLUMNS.len() {
                return Err(at(format!(
                    "{} columns, not {}",
                    c.len(),
                    FIND_COLUMNS.len()
                )));
            }
            let num = |s: &str| -> Result<Option<i32>, String> {
                if s.is_empty() {
                    Ok(None)
                } else {
                    s.parse().map(Some).map_err(|e| format!("{s}: {e}"))
                }
            };
            let bands = c[6]
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|b| {
                    let (from, items) = b
                        .split_once(':')
                        .ok_or_else(|| format!("a band is LEVEL:ITEMS, not {b}"))?;
                    let from = from.parse::<i32>().map_err(|e| format!("{from}: {e}"))?;
                    let items = items.split('+').map(hex).collect::<Result<Vec<_>, _>>()?;
                    Ok((from, items))
                })
                .collect::<Result<Vec<_>, String>>()
                .map_err(at)?;
            out.push(CorpseFind {
                key: c[0].to_string(),
                chance: c[1]
                    .parse::<f32>()
                    .map_err(|e| at(format!("{}: {e}", c[1])))?,
                never: (!c[2].is_empty()).then(|| c[2].to_string()),
                least_level: num(c[3]).map_err(at)?,
                below_killer: num(c[4]).map_err(at)?,
                hero_cap: c[5] == "yes",
                bands,
                placed: c[7].to_string(),
                confidence: c[8].to_string(),
            });
        }
        Ok(out)
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
            requires: if c[5].is_empty() {
                None
            } else {
                Some(crate::QuestGate::from_text(c[5])?)
            },
            placed: c[3].to_string(),
            confidence: c[4].to_string(),
        })
    }

    /// Read a table's text.
    ///
    /// # Errors
    /// A line without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let (text, units) = match text.split_once(&format!("\n{UNITS_PART}\n")) {
            Some((t, u)) => (t, Self::units_of(u)?),
            None => (text, BTreeMap::new()),
        };
        let (text, finds) = match text.split_once(&format!("\n{FINDS_PART}\n")) {
            Some((t, f)) => (t, Self::finds_of(f)?),
            None => (text, Vec::new()),
        };
        let (text, values) = match text.split_once(&format!("\n{VALUES_PART}\n")) {
            Some((t, v)) => (t, Self::values_of(v)?),
            None => (text, BTreeMap::new()),
        };
        let (text, pools) = match text.split_once(&format!("\n{POOLS_PART}\n")) {
            Some((t, p)) => (t, crate::pools::Pools::from_text(p)?),
            None => (text, crate::pools::Pools::default()),
        };
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
            pools,
            values,
            finds,
            units,
        })
    }
}
