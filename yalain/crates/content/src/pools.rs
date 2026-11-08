//! Treasure pools: named draws a creature's loot takes its treasure from, nested as a tree. A pool
//! holds other pools or item templates, each with a weight and the window of item levels it is drawn
//! at; a draw descends from a pool to one of its entries whose window holds the item level, by
//! weight, until it reaches a template. A pool may name the race it is for and the share of a
//! killer of that race's draws from its parent it takes outright; the rest are drawn by weight.
//!
//! A death-loot profile may draw its items from pools: how many by its counts, each from one of its
//! draws picked by weight, and once per corpse a rare from a pool by a chance.
//!
//! The text form is tab-separated, one pool or one profile's draws per line.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The deepest a draw descends through pools; a deeper one gives nothing.
pub const DEEPEST: usize = 8;

/// The highest item level an entry's window can name (an entry without a top is open above).
pub const OPEN_ABOVE: i32 = i32::MAX;

/// What an entry of a pool leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Another pool, by its name.
    Pool(String),
    /// An item template.
    Template(u32),
}

/// One entry of a pool, or one draw of a profile: what it leads to, its weight, and the item levels
/// it is drawn at, inclusive.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub to: Target,
    pub weight: f32,
    pub least: i32,
    pub most: i32,
}

impl Entry {
    /// Whether it is drawn at an item level.
    #[must_use]
    pub const fn holds(&self, level: i32) -> bool {
        self.least <= level && level <= self.most
    }
}

/// A named pool.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pool {
    pub name: String,
    /// The race it is for (the race's bit: 1 human, 2 Lugian, 4 Tumerok, 8 Drudge, 16 Empyrean),
    /// 0 for none.
    pub race: u32,
    /// The share of a draw from its parent by a killer of its race that it takes outright.
    pub own: f32,
    pub entries: Vec<Entry>,
}

/// A death-loot profile's draws from pools: what its items are drawn from, and its rare.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Draws {
    pub draws: Vec<Entry>,
    /// The pool its rare is drawn from and the chance, rolled once a corpse.
    pub rare: Option<(String, f32)>,
}

/// Every pool, and the profiles that draw from them, by name.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pools {
    pub pools: BTreeMap<String, Pool>,
    pub profiles: BTreeMap<String, Draws>,
}

/// The dice a draw rolls: a pick among weights (none when every weight is 0), and a chance.
pub trait Dice {
    fn pick(&mut self, weights: &[f32]) -> Option<usize>;
    fn chance(&mut self, p: f32) -> bool;
}

impl Pools {
    /// One template drawn from a pool at an item level for a killer of a race (its bit; 0 for none):
    /// none when no entry holds the level, the pools named are missing, or the draw goes deeper than
    /// [`DEEPEST`].
    pub fn draw(&self, pool: &str, level: i32, race: u32, dice: &mut impl Dice) -> Option<u32> {
        let mut at = self.pools.get(pool)?;
        for _ in 0..DEEPEST {
            let own = (race != 0)
                .then(|| {
                    at.entries.iter().find_map(|e| match &e.to {
                        Target::Pool(n) => self
                            .pools
                            .get(n)
                            .filter(|p| p.race == race && p.own > 0.0 && e.holds(level)),
                        Target::Template(_) => None,
                    })
                })
                .flatten();
            let next = match own {
                Some(p) if dice.chance(p.own) => Target::Pool(p.name.clone()),
                _ => Self::pick(&at.entries, level, dice)?,
            };
            match next {
                Target::Template(t) => return Some(t),
                Target::Pool(n) => at = self.pools.get(&n)?,
            }
        }
        None
    }

    /// One of some entries whose window holds a level, by weight.
    fn pick(entries: &[Entry], level: i32, dice: &mut impl Dice) -> Option<Target> {
        let weights: Vec<f32> = entries
            .iter()
            .map(|e| if e.holds(level) { e.weight } else { 0.0 })
            .collect();
        dice.pick(&weights).map(|i| entries[i].to.clone())
    }

    /// The templates a profile's draws give for `count` items at an item level, for a killer of a
    /// race: one draw each, and nothing for a draw that comes to nothing.
    pub fn items(
        &self,
        profile: &str,
        count: usize,
        level: i32,
        race: u32,
        dice: &mut impl Dice,
    ) -> Vec<u32> {
        let Some(d) = self.profiles.get(profile) else {
            return Vec::new();
        };
        (0..count)
            .filter_map(|_| match Self::pick(&d.draws, level, dice)? {
                Target::Pool(p) => self.draw(&p, level, race, dice),
                Target::Template(t) => Some(t),
            })
            .collect()
    }

    /// A profile's rare, rolled once: its pool's template at the item level, or none.
    pub fn rare(&self, profile: &str, level: i32, race: u32, dice: &mut impl Dice) -> Option<u32> {
        let (pool, chance) = self.profiles.get(profile)?.rare.as_ref()?;
        if !dice.chance(*chance) {
            return None;
        }
        self.draw(pool, level, race, dice)
    }

    /// The pools as text: a header, then a line per pool (`pool`, name, race, own share, entries)
    /// and a line per profile (`draws`, name, rare pool, rare chance, draws). An entry is
    /// `P:name` or `T:XXXXXXXX`, then `*weight@least-most`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from("kind\tname\trace\town\tentries\n");
        for p in self.pools.values() {
            let _ = writeln!(
                out,
                "pool\t{}\t{}\t{}\t{}",
                p.name,
                p.race,
                p.own,
                entries_text(&p.entries)
            );
        }
        for (name, d) in &self.profiles {
            let (pool, chance) = d.rare.clone().unwrap_or_default();
            let _ = writeln!(
                out,
                "draws\t{name}\t{pool}\t{chance}\t{}",
                entries_text(&d.draws)
            );
        }
        out
    }

    /// Read the pools' text.
    ///
    /// # Errors
    /// A line without its columns, or a value that does not read.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut t = Self::default();
        for (n, line) in text.lines().enumerate().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let c: Vec<&str> = line.split('\t').collect();
            let [kind, name, a, b, entries] = c[..] else {
                return Err(at(format!("{} columns, not 5", c.len())));
            };
            let entries = entries_of(entries).map_err(at)?;
            match kind {
                "pool" => {
                    let race = a.parse().map_err(|e| at(format!("{a}: {e}")))?;
                    let own = b.parse().map_err(|e| at(format!("{b}: {e}")))?;
                    t.pools.insert(
                        name.to_string(),
                        Pool {
                            name: name.to_string(),
                            race,
                            own,
                            entries,
                        },
                    );
                }
                "draws" => {
                    let chance: f32 = b.parse().map_err(|e| at(format!("{b}: {e}")))?;
                    let rare = (!a.is_empty()).then(|| (a.to_string(), chance));
                    t.profiles.insert(
                        name.to_string(),
                        Draws {
                            draws: entries,
                            rare,
                        },
                    );
                }
                other => return Err(at(format!("{other} is no kind of line"))),
            }
        }
        Ok(t)
    }
}

fn entries_text(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|e| {
            let to = match &e.to {
                Target::Pool(n) => format!("P:{n}"),
                Target::Template(t) => format!("T:{t:08X}"),
            };
            format!("{to}*{}@{}-{}", e.weight, e.least, e.most)
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn entries_of(s: &str) -> Result<Vec<Entry>, String> {
    s.split(',')
        .filter(|x| !x.is_empty())
        .map(|x| {
            let (to, rest) = x
                .split_once('*')
                .ok_or_else(|| format!("an entry is TO*WEIGHT@LEAST-MOST, not {x}"))?;
            let (weight, window) = rest
                .split_once('@')
                .ok_or_else(|| format!("an entry is TO*WEIGHT@LEAST-MOST, not {x}"))?;
            let (least, most) = window
                .split_once('-')
                .ok_or_else(|| format!("an entry is TO*WEIGHT@LEAST-MOST, not {x}"))?;
            let to = match to.split_once(':') {
                Some(("P", n)) => Target::Pool(n.to_string()),
                Some(("T", t)) => {
                    Target::Template(u32::from_str_radix(t, 16).map_err(|e| format!("{t}: {e}"))?)
                }
                _ => return Err(format!("{to} is neither P:name nor T:template")),
            };
            let bad = |e: &dyn std::fmt::Display| format!("{x}: {e}");
            Ok(Entry {
                to,
                weight: weight.parse().map_err(|e| bad(&e))?,
                least: least.parse().map_err(|e| bad(&e))?,
                most: most.parse().map_err(|e| bad(&e))?,
            })
        })
        .collect()
}
