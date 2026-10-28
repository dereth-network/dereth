//! Conflicts between a world's packs: two sources that set one thing differently, and the world's
//! word on which of two packs wins.
//!
//! An **item** is one thing packs can both set, named as the build's report and an override name
//! it, with the landblocks it stands in. What items there are is the game's ([`Dialect::item`]);
//! two forms are the grammar's own: `record <id>`, a record of the data files a pack replaces, and
//! a new record by its kind and symbol (`surface sample:stripes`).
//!
//! Sources that set an item alike agree. Set differently, the item is a conflict, and a build
//! stops with one report of every conflict: the item, and each pack, file and line with what it
//! sets. A world settles a conflict between two packs by saying which wins:
//!
//! ```toml
//! [[override]]
//! pack = "holtburg-plus"                    # the pack that wins
//! over = "holtburg"                         # the pack it wins over
//! items = ["weenie 1234", "landblock A9B4"] # what it wins; without a list, all both set
//! ```
//!
//! `landblock A9B4` names everything of the landblock. Where overrides of both packs cover an
//! item, the one that names it most closely wins: the item itself, then its landblock, then a
//! list-less override. The build takes the winner's and lists what it overrode. Two files of one
//! pack that disagree are no conflict a world settles: the pack is put right.

use std::collections::BTreeSet;
use std::fmt;

use crate::dialect::Dialect;
use crate::error::Loc;
use crate::symbol::SymbolKind;

/// One thing packs set: its name, as the report and an override name it, the landblocks it
/// stands in, and where it is when it is a place: a landblock and its meters there.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    pub landblocks: BTreeSet<u16>,
    pub place: Option<(u16, [f32; 2])>,
}

impl Item {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            landblocks: BTreeSet::new(),
            place: None,
        }
    }

    /// The item, standing in landblock `lb` too.
    #[must_use]
    pub fn in_landblock(mut self, lb: u16) -> Self {
        self.landblocks.insert(lb);
        self
    }

    /// The item, at `(x, y)` of landblock `lb`.
    #[must_use]
    pub fn at(mut self, lb: u16, x: f32, y: f32) -> Self {
        self.landblocks.insert(lb);
        self.place = Some((lb, [x, y]));
        self
    }
}

/// A replaced record's item.
#[must_use]
pub fn record_item(id: u32) -> String {
    format!("record {id:#010X}")
}

/// One source that sets an item: its pack, where, and what it sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub pack: String,
    pub at: Loc,
    /// What it sets, as the report says it: `sets it to 50`, `removes it`.
    pub sets: String,
    /// What it sets, compared: two claims that agree have the same.
    value: String,
}

impl Claim {
    #[must_use]
    pub fn new(pack: &str, at: &Loc, sets: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            pack: pack.to_owned(),
            at: at.clone(),
            sets: sets.into(),
            value: value.into(),
        }
    }

    /// What it sets, compared: two claims that agree have the same.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// An item sources set differently, with no word from the world on which wins.
#[derive(Clone, Debug, PartialEq)]
pub struct Conflict {
    /// What is set: one item, or a run of terrain vertices two files both set.
    pub what: String,
    pub landblocks: BTreeSet<u16>,
    /// Where it is, when it is a place: a landblock and its meters there.
    pub place: Option<(u16, [f32; 2])>,
    /// Every source that sets it, in load order.
    pub claims: Vec<Claim>,
}

impl Conflict {
    /// Whether `other` is the same conflict, wherever in their files its sources now stand.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        self.what == other.what
            && self.claims.len() == other.claims.len()
            && self
                .claims
                .iter()
                .zip(&other.claims)
                .all(|(a, b)| a.pack == b.pack && a.at.file == b.at.file && a.value == b.value)
    }

    /// The conflict on one line, starting where its first source is: as an editor lists it.
    #[must_use]
    pub fn line(&self) -> String {
        let mut out = String::new();
        for (i, c) in self.claims.iter().enumerate() {
            if i == 0 {
                out.push_str(&format!("{}: {}: {} {}", c.at, self.what, c.pack, c.sets));
            } else {
                out.push_str(&format!("; {} {} ({})", c.pack, c.sets, c.at));
            }
        }
        out
    }
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.what)?;
        for c in &self.claims {
            write!(f, "\n  {}, {}: {}", c.pack, c.at, c.sets)?;
        }
        Ok(())
    }
}

/// An item sources set differently, settled by the world: the claim that stands, and those it
/// stands over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overridden {
    pub what: String,
    pub landblocks: BTreeSet<u16>,
    pub winner: Claim,
    pub over: Vec<Claim>,
}

impl fmt::Display for Overridden {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut losers: Vec<&str> = Vec::new();
        for c in &self.over {
            if !losers.contains(&c.pack.as_str()) {
                losers.push(&c.pack);
            }
        }
        write!(
            f,
            "{}: {}'s ({}) over {}'s",
            self.what,
            self.winner.pack,
            self.winner.at,
            losers.join("'s and ")
        )
    }
}

/// The conflicts found and the overrides taken, of some part of a world.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settled {
    pub conflicts: Vec<Conflict>,
    pub overridden: Vec<Overridden>,
}

impl Settled {
    pub fn extend(&mut self, other: Self) {
        self.conflicts.extend(other.conflicts);
        self.overridden.extend(other.overridden);
    }
}

/// What an override names: one item, or everything of a landblock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    Item(String),
    Landblock(u16),
}

impl Scope {
    /// An item as a world file writes it, read: `landblock <XXYY>`, an item of the game's
    /// ([`Dialect::item`]), or a new record's kind and symbol.
    ///
    /// # Errors
    /// Text that names no item, with the forms there are.
    pub fn parse<D: Dialect>(text: &str) -> Result<Self, String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let bad = || format!("{text:?} is not an item: {}", D::ITEM_FORMS);
        if let ["landblock", l] = words.as_slice() {
            return landblock(l).map(Self::Landblock).ok_or_else(bad);
        }
        let item = D::item(text, &words).or_else(|| match words.as_slice() {
            ["record", id] => {
                let h = id.strip_prefix("0x").or_else(|| id.strip_prefix("0X"))?;
                u32::from_str_radix(h, 16).ok().map(record_item)
            }
            [kind, symbol] => D::Kind::from_name(kind)
                .filter(|k| k.names_record())
                .map(|k| format!("{k} {symbol}")),
            _ => None,
        });
        item.map(Self::Item).ok_or_else(bad)
    }

    /// How closely the scope names `item`: 3 for the item itself, 2 for its landblock.
    fn covers(&self, item: &Item) -> Option<u8> {
        match self {
            Self::Item(name) => (*name == item.name).then_some(3),
            Self::Landblock(lb) => item.landblocks.contains(lb).then_some(2),
        }
    }
}

/// A landblock as an item writes it: four hexadecimal digits.
#[must_use]
pub fn landblock(word: &str) -> Option<u16> {
    (word.len() == 4)
        .then(|| u16::from_str_radix(word, 16).ok())
        .flatten()
}

/// A world's word that one pack wins over another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Override {
    /// The pack that wins, by name.
    pub pack: String,
    /// The pack it wins over.
    pub over: String,
    /// What it wins; `None` for everything both set.
    pub items: Option<Vec<Scope>>,
    /// Where the world file says it.
    pub at: Loc,
}

/// Whether `overrides` say pack `winner` wins `item` over pack `loser`, and how closely the
/// closest of them names it: 3 for the item itself, 2 for its landblock, 1 for an override
/// without a list.
#[must_use]
pub fn covers(overrides: &[Override], winner: &str, loser: &str, item: &Item) -> Option<u8> {
    overrides
        .iter()
        .filter(|o| o.pack == winner && o.over == loser)
        .filter_map(|o| match &o.items {
            None => Some(1),
            Some(scopes) => scopes.iter().filter_map(|s| s.covers(item)).max(),
        })
        .max()
}

/// Settle `item`, set by `claims` in load order: the place of the claim that stands. Claims that
/// agree stand together, the last of them returned. Otherwise a claim stands when the world says
/// its pack wins over every pack that sets the item otherwise (of two such, the one whose
/// overrides name the item more closely; of those, the later), recorded in `out` as an override;
/// with none, the conflict is recorded and the last claim stands, so an editor still shows a
/// world.
pub fn settle(overrides: &[Override], item: &Item, claims: &[Claim], out: &mut Settled) -> usize {
    let last = claims.len().saturating_sub(1);
    if claims.iter().all(|c| c.value == claims[0].value) {
        return last;
    }
    // How closely the world says a claim wins over every claim that differs, if it does.
    let wins = |w: &Claim| -> Option<u8> {
        claims
            .iter()
            .filter(|d| d.value != w.value)
            .map(|d| {
                if d.pack == w.pack {
                    None
                } else {
                    covers(overrides, &w.pack, &d.pack, item)
                }
            })
            .try_fold(u8::MAX, |least, r| r.map(|r| least.min(r)))
    };
    let best = (0..claims.len())
        .filter_map(|i| wins(&claims[i]).map(|r| (r, i)))
        .max();
    if let Some((_, w)) = best {
        out.overridden.push(Overridden {
            what: item.name.clone(),
            landblocks: item.landblocks.clone(),
            winner: claims[w].clone(),
            over: claims
                .iter()
                .filter(|d| d.value != claims[w].value)
                .cloned()
                .collect(),
        });
        return w;
    }
    out.conflicts.push(Conflict {
        what: item.name.clone(),
        landblocks: item.landblocks.clone(),
        place: item.place,
        claims: claims.to_vec(),
    });
    last
}
