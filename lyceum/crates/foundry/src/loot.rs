//! The world's loot tables: every death treasure and wielded treasure the packs and the server
//! hold, and every one something names that nothing holds; where each is named and how; and the
//! edits that copy a server's table into the pack being edited (its copy then stands for it), put
//! the server's back, make a new table, take a pack's own away, and change one.
//!
//! - A **death treasure** (`treasure_death`) is what a creature drops when it dies, drawn by the
//!   server's loot rules: a tier and a quality, which heritages its clothing, armor and weapons are
//!   for, and for items, magic items and mundane items each a chance, a least and a most, and the
//!   mix of things they are drawn from.
//! - A **wielded treasure** (`treasure_wielded`) is a list of weenies drawn in sets, what a
//!   creature is made wearing and wielding ([`crate::wielded`]).
//!
//! A weenie names a death treasure by its data id 35 and a wielded treasure by its data id 32; a
//! generator's entry that makes treasure (`where_Create` with the treasure bit) names either by its
//! class id, the death treasure when there are both. A table is known by its kind and id. The
//! packs' tables are as their files have them (a later pack's over an earlier's, and in a pack,
//! its first file's); the server's as its database has them, but for one a pack writes again.
//!
//! A pack's table is written in a file of its own, as the server's own export writes one: its
//! rows deleted, then written again. A copy of the server's keeps its id, in
//! `server/sql/<table>/<id>.sql` (the id in five figures at least). A pack's own table is named by
//! a symbol, `{{treasure:drudge-loot}}`, in `server/sql/<table>/drudge-loot.sql`, and wherever a
//! weenie names it; a build numbers it from 100000 up, as the lockfile keeps it, so packs made
//! apart never take one id for two tables. Death and wielded treasures share their ids, and so
//! their symbols.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::content::{self, SqlDoc};
use crate::edit::{token_symbol, Op, Session, Weenie};
use crate::error::Error;
use crate::lock::Lock;
pub use crate::quests::Whose;
use crate::source::Pack;
use crate::sql_ids::{self, LootUse, Treasure, WieldedItem};
use crate::symbol::Kind;

/// The first id a pack's own table is numbered from.
pub const NEW_ID: u32 = 100_000;

/// Which of the two kinds of table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LootKind {
    Death,
    Wielded,
}

impl LootKind {
    pub const ALL: [Self; 2] = [Self::Death, Self::Wielded];

    /// The server's table of it, which is also the folder of a pack's files of it.
    #[must_use]
    pub const fn table(self) -> &'static str {
        match self {
            Self::Death => "treasure_death",
            Self::Wielded => "treasure_wielded",
        }
    }

    /// The kind a folder of a pack's `server/sql` holds, when it holds tables.
    #[must_use]
    pub fn of_folder(folder: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.table() == folder)
    }

    /// Its word, as the server's console takes it: `death`, `wielded`.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Death => "death",
            Self::Wielded => "wielded",
        }
    }
}

/// What a table holds.
#[derive(Clone, Debug, PartialEq)]
pub enum LootRows {
    Death(Treasure),
    Wielded(Vec<WieldedItem>),
}

/// A pack file that holds a table, the pack's place in the load order, and the table's type as
/// the file writes it: its id, or its symbol's token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackFile {
    pub file: PathBuf,
    pub pack: usize,
    pub written: String,
}

/// `text`, the file `path` of pack `pack`, with each loot table's token (`{{treasure:...}}`)
/// filled in with the id `lock` gives its symbol (0 for one it gives none), and every other token
/// left as it is.
#[must_use]
pub fn with_ids<'a>(pack: &str, path: &Path, text: &'a str, lock: &Lock) -> Cow<'a, str> {
    if !text.contains("{{treasure:") {
        return Cow::Borrowed(text);
    }
    let Ok(tokens) = crate::sql::tokens(path, pack, text) else {
        return Cow::Borrowed(text);
    };
    let tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| t.kind == Kind::Treasure)
        .collect();
    Cow::Owned(crate::sql::fill(text, &tokens, |t| {
        lock.get(Kind::Treasure, &t.symbol).unwrap_or(0)
    }))
}

/// The token of loot table `symbol` (in full) as a file of pack `pack` writes it: a symbol of its
/// own by its name alone.
#[must_use]
pub fn token_of(pack: &str, symbol: &str) -> String {
    let name = symbol
        .strip_prefix(pack)
        .and_then(|s| s.strip_prefix(':'))
        .unwrap_or(symbol);
    format!("{{{{treasure:{name}}}}}")
}

/// The packs' loot tables: each table's rows and the file that writes them, a later pack's over
/// an earlier's and in a pack its first file's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackLoot {
    pub death: BTreeMap<u32, (Treasure, PackFile)>,
    pub wielded: BTreeMap<u32, (Vec<WieldedItem>, PackFile)>,
}

impl PackLoot {
    /// The tables of `packs`, as their files have them, their symbols numbered as `lock` numbers
    /// them.
    #[must_use]
    pub fn read(packs: &[Pack], lock: &Lock) -> Self {
        let mut out = Self::default();
        for (pi, p) in packs.iter().enumerate().rev() {
            let mut death: BTreeMap<u32, (Treasure, PackFile)> = BTreeMap::new();
            let mut wielded: BTreeMap<u32, (Vec<WieldedItem>, PackFile)> = BTreeMap::new();
            for f in &p.sql {
                if !f.text.contains("treasure_") {
                    continue;
                }
                // Each table's type as written: a number, or a token.
                let mut written: BTreeMap<u32, String> = BTreeMap::new();
                if let Ok(tokens) = crate::sql::tokens(&f.path, &p.name, &f.text) {
                    for t in tokens.iter().filter(|t| t.kind == Kind::Treasure) {
                        if let Some(id) = lock.get(Kind::Treasure, &t.symbol) {
                            written.insert(id, token_of(&p.name, &t.symbol));
                        }
                    }
                }
                let text = with_ids(&p.name, &f.path, &f.text, lock);
                let at = |id: u32| PackFile {
                    file: f.path.clone(),
                    pack: pi,
                    written: written.get(&id).cloned().unwrap_or_else(|| id.to_string()),
                };
                for (id, t) in sql_ids::death_treasures_in(&text) {
                    if id != 0 {
                        death.entry(id).or_insert_with(|| (t, at(id)));
                    }
                }
                for (id, rows) in sql_ids::wielded_in(&text) {
                    if id != 0 {
                        wielded.entry(id).or_insert_with(|| (rows, at(id)));
                    }
                }
            }
            for (id, v) in death {
                out.death.entry(id).or_insert(v);
            }
            for (id, v) in wielded {
                out.wielded.entry(id).or_insert(v);
            }
        }
        out
    }
}

/// One weenie's naming of a table: the weenie, how, and the pack file that names it (`None` for
/// the server's database).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LootUser {
    pub by: Weenie,
    pub how: LootUse,
    pub file: Option<PathBuf>,
}

/// A loot table of the world.
#[derive(Clone, Debug, PartialEq)]
pub struct LootTable {
    pub kind: LootKind,
    pub id: u32,
    /// Its rows as they stand (a pack's over the server's), and the server's own.
    pub rows: Option<LootRows>,
    pub server: Option<LootRows>,
    /// The pack file that holds it, and that pack's name; for a pack's own table named by a
    /// symbol, its token as that file writes it.
    pub file: Option<PathBuf>,
    pub pack: Option<String>,
    pub symbol: Option<String>,
    pub whose: Whose,
    pub uses: Vec<LootUser>,
}

impl LootTable {
    /// The name of the symbol a pack names it by, as its own file writes it (`drudge-hoard`).
    #[must_use]
    pub fn symbol_name(&self) -> Option<&str> {
        self.symbol
            .as_deref()?
            .strip_prefix("{{treasure:")?
            .strip_suffix("}}")
            .map(str::trim)
    }

    /// How many weenies name it.
    #[must_use]
    pub fn places(&self) -> usize {
        let mut seen: HashSet<&Weenie> = HashSet::new();
        self.uses.iter().filter(|u| seen.insert(&u.by)).count()
    }

    /// Its death treasure, for one of that kind.
    #[must_use]
    pub fn death(&self) -> Option<&Treasure> {
        match &self.rows {
            Some(LootRows::Death(t)) => Some(t),
            _ => None,
        }
    }

    /// Its wielded treasure's rows, for one of that kind.
    #[must_use]
    pub fn wielded(&self) -> Option<&[WieldedItem]> {
        match &self.rows {
            Some(LootRows::Wielded(w)) => Some(w),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------------------------
// what the server draws a death treasure's items from
// -----------------------------------------------------------------------------------------------

/// What a death treasure's items can be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemKind {
    Weapon,
    Armor,
    Scroll,
    Clothing,
    Jewelry,
    Gem,
    Art,
    Pet,
    Cloak,
    Spirit,
    SocietyBreastplate,
    SocietyGauntlets,
    SocietyGirth,
    SocietyGreaves,
    SocietyHelm,
    SocietyPauldrons,
    SocietyTassets,
    SocietyVambraces,
    SocietySollerets,
    Consumable,
    HealingKit,
    Lockpick,
    Component,
    ManaStone,
    Pyreals,
}

impl ItemKind {
    /// Its name's key in the editor's text, after `loot.kind_`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Armor => "armor",
            Self::Scroll => "scroll",
            Self::Clothing => "clothing",
            Self::Jewelry => "jewelry",
            Self::Gem => "gem",
            Self::Art => "art",
            Self::Pet => "pet",
            Self::Cloak => "cloak",
            Self::Spirit => "spirit",
            Self::SocietyBreastplate => "society_breastplate",
            Self::SocietyGauntlets => "society_gauntlets",
            Self::SocietyGirth => "society_girth",
            Self::SocietyGreaves => "society_greaves",
            Self::SocietyHelm => "society_helm",
            Self::SocietyPauldrons => "society_pauldrons",
            Self::SocietyTassets => "society_tassets",
            Self::SocietyVambraces => "society_vambraces",
            Self::SocietySollerets => "society_sollerets",
            Self::Consumable => "consumable",
            Self::HealingKit => "healing_kit",
            Self::Lockpick => "lockpick",
            Self::Component => "component",
            Self::ManaStone => "mana_stone",
            Self::Pyreals => "pyreals",
        }
    }
}

/// The three groups of a death treasure's items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Items,
    Magic,
    Mundane,
}

impl Group {
    pub const ALL: [Self; 3] = [Self::Items, Self::Magic, Self::Mundane];

    /// Its columns: its chance, least, most and mix.
    #[must_use]
    pub const fn columns(self) -> [&'static str; 4] {
        match self {
            Self::Items => [
                "item_Chance",
                "item_Min_Amount",
                "item_Max_Amount",
                "item_Treasure_Type_Selection_Chances",
            ],
            Self::Magic => [
                "magic_Item_Chance",
                "magic_Item_Min_Amount",
                "magic_Item_Max_Amount",
                "magic_Item_Treasure_Type_Selection_Chances",
            ],
            Self::Mundane => [
                "mundane_Item_Chance",
                "mundane_Item_Min_Amount",
                "mundane_Item_Max_Amount",
                "mundane_Item_Type_Selection_Chances",
            ],
        }
    }

    /// Its key in the editor's text, after `loot.`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Items => "items",
            Self::Magic => "magic",
            Self::Mundane => "mundane",
        }
    }

    /// Its chance, least, most and mix in `t`.
    #[must_use]
    pub const fn of(self, t: &Treasure) -> (i32, i32, i32, i32) {
        let ((c, lo, hi), mix) = match self {
            Self::Items => (t.items, t.kinds.0),
            Self::Magic => (t.magic, t.kinds.1),
            Self::Mundane => (t.mundane, t.kinds.2),
        };
        (c, lo, hi, mix)
    }

    /// The mixes its items are drawn from, by their number from 1: each what it can be and how
    /// likely, as the server draws them.
    #[must_use]
    pub const fn mixes(self) -> &'static [&'static [(ItemKind, f32)]] {
        match self {
            Self::Items => ITEM_MIXES,
            Self::Magic => MAGIC_MIXES,
            Self::Mundane => MUNDANE_MIXES,
        }
    }
}

use ItemKind as K;

/// What an item is drawn from, by the mix a death treasure names.
const ITEM_MIXES: &[&[(ItemKind, f32)]] = &[
    &[(K::Weapon, 1.0)],
    &[(K::Armor, 1.0)],
    &[(K::Scroll, 1.0)],
    &[(K::Clothing, 1.0)],
    &[(K::Jewelry, 1.0)],
    &[(K::Gem, 1.0)],
    &[(K::Art, 1.0)],
    &[
        (K::Weapon, 0.125),
        (K::Armor, 0.125),
        (K::Scroll, 0.125),
        (K::Clothing, 0.125),
        (K::Jewelry, 0.125),
        (K::Gem, 0.125),
        (K::Art, 0.125),
        (K::Pet, 0.125),
    ],
    &[
        (K::Weapon, 0.2),
        (K::Armor, 0.2),
        (K::Scroll, 0.2),
        (K::Clothing, 0.05),
        (K::Jewelry, 0.05),
        (K::Gem, 0.05),
        (K::Art, 0.05),
        (K::Pet, 0.2),
    ],
    &[
        (K::Weapon, 0.3),
        (K::Armor, 0.3),
        (K::Scroll, 0.2),
        (K::Pet, 0.2),
    ],
    &[
        (K::Clothing, 0.25),
        (K::Jewelry, 0.25),
        (K::Gem, 0.25),
        (K::Art, 0.25),
    ],
];

/// What a magic item is drawn from, by the mix a death treasure names.
const MAGIC_MIXES: &[&[(ItemKind, f32)]] = &[
    &[(K::Weapon, 1.0)],
    &[(K::Armor, 1.0)],
    &[(K::Scroll, 1.0)],
    &[(K::Clothing, 1.0)],
    &[(K::Jewelry, 1.0)],
    &[(K::Gem, 1.0)],
    &[(K::Art, 1.0)],
    &[
        (K::Weapon, 0.15),
        (K::Armor, 0.15),
        (K::Scroll, 0.14),
        (K::Clothing, 0.14),
        (K::Jewelry, 0.14),
        (K::Gem, 0.14),
        (K::Art, 0.125),
        (K::Cloak, 0.01),
        (K::Spirit, 0.005),
    ],
    &[
        (K::Weapon, 0.3),
        (K::Armor, 0.3),
        (K::Scroll, 0.1),
        (K::Clothing, 0.1),
        (K::Jewelry, 0.1),
        (K::Gem, 0.05),
        (K::Art, 0.035),
        (K::Cloak, 0.01),
        (K::Spirit, 0.005),
    ],
    &[
        (K::Weapon, 0.3),
        (K::Armor, 0.3),
        (K::Scroll, 0.185),
        (K::Clothing, 0.1),
        (K::Jewelry, 0.1),
        (K::Cloak, 0.01),
        (K::Spirit, 0.005),
    ],
    &[
        (K::Jewelry, 0.33),
        (K::Gem, 0.34),
        (K::Art, 0.315),
        (K::Cloak, 0.01),
        (K::Spirit, 0.005),
    ],
    &[
        (K::Scroll, 0.4),
        (K::Jewelry, 0.2),
        (K::Gem, 0.2),
        (K::Art, 0.185),
        (K::Cloak, 0.01),
        (K::Spirit, 0.005),
    ],
    &[(K::SocietyBreastplate, 1.0)],
    &[(K::SocietyGauntlets, 1.0)],
    &[(K::SocietyGirth, 1.0)],
    &[(K::SocietyGreaves, 1.0)],
    &[(K::SocietyHelm, 1.0)],
    &[(K::SocietyPauldrons, 1.0)],
    &[(K::SocietyTassets, 1.0)],
    &[(K::SocietyVambraces, 1.0)],
    &[(K::SocietySollerets, 1.0)],
    &[
        (K::Weapon, 0.35),
        (K::Armor, 0.35),
        (K::Clothing, 0.1),
        (K::Jewelry, 0.1),
        (K::Cloak, 0.05),
        (K::Pet, 0.05),
    ],
    &[
        (K::Clothing, 0.3),
        (K::Jewelry, 0.2),
        (K::Cloak, 0.2),
        (K::Pet, 0.3),
    ],
    &[
        (K::Weapon, 0.2),
        (K::Armor, 0.2),
        (K::Clothing, 0.3),
        (K::Jewelry, 0.3),
    ],
];

/// What a mundane item is drawn from, by the mix a death treasure names.
const MUNDANE_MIXES: &[&[(ItemKind, f32)]] = &[
    &[(K::Consumable, 1.0)],
    &[(K::HealingKit, 1.0)],
    &[(K::Lockpick, 1.0)],
    &[(K::Component, 1.0)],
    &[(K::ManaStone, 1.0)],
    &[(K::Pyreals, 1.0)],
    &[
        (K::Pyreals, 0.17),
        (K::Consumable, 0.17),
        (K::HealingKit, 0.16),
        (K::Lockpick, 0.16),
        (K::Component, 0.17),
        (K::ManaStone, 0.17),
    ],
    &[
        (K::Pyreals, 0.34),
        (K::Component, 0.33),
        (K::ManaStone, 0.33),
    ],
];

/// Whom a death treasure's clothing, armor and weapons are made for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Heritage {
    Aluvian,
    Gharundim,
    Sho,
    Viamontian,
    CelestialHand,
    EldrytchWeb,
    RadiantBlood,
}

impl Heritage {
    /// Its name's key in the editor's text, after `loot.heritage_`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Aluvian => "aluvian",
            Self::Gharundim => "gharundim",
            Self::Sho => "sho",
            Self::Viamontian => "viamontian",
            Self::CelestialHand => "celestial_hand",
            Self::EldrytchWeb => "eldrytch_web",
            Self::RadiantBlood => "radiant_blood",
        }
    }
}

use Heritage as H;

/// The heritages a death treasure's things are made for, by the mix it names (`unknown_Chances`),
/// counted from 1. Any other number is drawn as Aluvian, Gharu'ndim or Sho alike.
pub const HERITAGE_MIXES: &[&[(Heritage, f32)]] = &[
    &[(H::Aluvian, 1.0)],
    &[(H::Gharundim, 1.0)],
    &[(H::Sho, 1.0)],
    &[(H::Aluvian, 0.75), (H::Gharundim, 0.25)],
    &[(H::Aluvian, 0.75), (H::Sho, 0.25)],
    &[(H::Aluvian, 0.25), (H::Gharundim, 0.75)],
    &[(H::Gharundim, 0.75), (H::Sho, 0.25)],
    &[(H::Aluvian, 0.25), (H::Sho, 0.75)],
    &[(H::Gharundim, 0.25), (H::Sho, 0.75)],
    &[(H::Aluvian, 0.5), (H::Gharundim, 0.5)],
    &[(H::Aluvian, 0.5), (H::Sho, 0.5)],
    &[(H::Gharundim, 0.5), (H::Sho, 0.5)],
    &[(H::Aluvian, 0.8), (H::Gharundim, 0.1), (H::Sho, 0.1)],
    &[(H::Aluvian, 0.1), (H::Gharundim, 0.8), (H::Sho, 0.1)],
    &[(H::Aluvian, 0.1), (H::Gharundim, 0.1), (H::Sho, 0.8)],
    &[(H::Aluvian, 0.5), (H::Gharundim, 0.25), (H::Sho, 0.25)],
    &[(H::Aluvian, 0.25), (H::Gharundim, 0.5), (H::Sho, 0.25)],
    &[(H::Aluvian, 0.25), (H::Gharundim, 0.25), (H::Sho, 0.5)],
    &[(H::Aluvian, 0.34), (H::Gharundim, 0.33), (H::Sho, 0.33)],
    &[(H::Viamontian, 1.0)],
    &[
        (H::Aluvian, 0.25),
        (H::Gharundim, 0.25),
        (H::Sho, 0.25),
        (H::Viamontian, 0.25),
    ],
    &[(H::CelestialHand, 1.0)],
    &[(H::EldrytchWeb, 1.0)],
    &[(H::RadiantBlood, 1.0)],
];

/// The tiers a death treasure may be.
pub const TIERS: std::ops::RangeInclusive<i32> = 1..=8;

/// The columns of a death treasure's row as the server's export writes them, after its type.
pub const DEATH_COLUMNS: [&str; 15] = [
    "tier",
    "loot_Quality_Mod",
    "unknown_Chances",
    "item_Chance",
    "item_Min_Amount",
    "item_Max_Amount",
    "item_Treasure_Type_Selection_Chances",
    "magic_Item_Chance",
    "magic_Item_Min_Amount",
    "magic_Item_Max_Amount",
    "magic_Item_Treasure_Type_Selection_Chances",
    "mundane_Item_Chance",
    "mundane_Item_Min_Amount",
    "mundane_Item_Max_Amount",
    "mundane_Item_Type_Selection_Chances",
];

/// A new death treasure's start: tier 1, items of every kind made for any heritage, up to one
/// item and one mundane item.
#[must_use]
pub const fn plain_death() -> Treasure {
    Treasure {
        tier: 1,
        quality: 0.0,
        items: (100, 0, 1),
        magic: (0, 0, 0),
        mundane: (100, 0, 1),
        heritage: 19,
        kinds: (9, 8, 7),
    }
}

/// A decimal as a table's file writes it: as short as reads back the same.
fn decimal(v: f32) -> String {
    if v == 0.0 {
        return "0".into();
    }
    format!("{v}")
}

/// `t`'s value of death treasure column `column`, as written.
fn death_value(t: &Treasure, column: &str) -> String {
    let v = |n: i32| n.to_string();
    match column {
        "tier" => v(t.tier),
        "loot_Quality_Mod" => decimal(t.quality),
        "unknown_Chances" => v(t.heritage),
        "item_Chance" => v(t.items.0),
        "item_Min_Amount" => v(t.items.1),
        "item_Max_Amount" => v(t.items.2),
        "item_Treasure_Type_Selection_Chances" => v(t.kinds.0),
        "magic_Item_Chance" => v(t.magic.0),
        "magic_Item_Min_Amount" => v(t.magic.1),
        "magic_Item_Max_Amount" => v(t.magic.2),
        "magic_Item_Treasure_Type_Selection_Chances" => v(t.kinds.1),
        "mundane_Item_Chance" => v(t.mundane.0),
        "mundane_Item_Min_Amount" => v(t.mundane.1),
        "mundane_Item_Max_Amount" => v(t.mundane.2),
        _ => v(t.kinds.2),
    }
}

/// Death treasure `id` (its type as written: a number, or a symbol's token) as its own file
/// writes it: its row deleted, then written again, stamped `today`.
#[must_use]
pub fn death_text(id: &str, t: &Treasure, today: &str) -> String {
    let cols: Vec<String> = std::iter::once("treasure_Type")
        .chain(DEATH_COLUMNS)
        .chain(std::iter::once("last_Modified"))
        .map(|c| format!("`{c}`"))
        .collect();
    let values: Vec<String> = std::iter::once(id.to_owned())
        .chain(DEATH_COLUMNS.iter().map(|c| death_value(t, c)))
        .chain(std::iter::once(format!("'{today}'")))
        .collect();
    format!(
        "DELETE FROM `treasure_death` WHERE `treasure_Type` = {id};\n\n\
         INSERT INTO `treasure_death` ({})\nVALUES ({});\n",
        cols.join(", "),
        values.join(", ")
    )
}

/// The columns of a wielded treasure's row as the server's export writes them.
const WIELDED_COLUMNS: [&str; 19] = [
    "treasure_Type",
    "weenie_Class_Id",
    "palette_Id",
    "unknown_1",
    "shade",
    "stack_Size",
    "stack_Size_Variance",
    "probability",
    "unknown_3",
    "unknown_4",
    "unknown_5",
    "set_Start",
    "has_Sub_Set",
    "continues_Previous_Set",
    "unknown_9",
    "unknown_10",
    "unknown_11",
    "unknown_12",
    "last_Modified",
];

/// Wielded treasure `id` (its type as written: a number, or a symbol's token) as its own file
/// writes it: its rows deleted, then written again in order, each with the name of what it makes
/// after it when `name` knows it, stamped `today`.
#[must_use]
pub fn wielded_text(
    id: &str,
    rows: &[WieldedItem],
    today: &str,
    name: &dyn Fn(&str) -> Option<String>,
) -> String {
    let cols: Vec<String> = WIELDED_COLUMNS.iter().map(|c| format!("`{c}`")).collect();
    let flag = |b: bool| if b { "True" } else { "False" };
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let u = r.unknown;
            let comment = name(&r.weenie)
                .map(|n| format!(" /* {} */", n.replace("*/", "* /")))
                .unwrap_or_default();
            format!(
                "({id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, '{today}'){comment}",
                r.weenie.trim(),
                r.palette,
                u[0],
                decimal(r.shade),
                r.stack,
                decimal(r.variance),
                decimal(r.probability),
                u[1],
                u[2],
                u[3],
                flag(r.set_start),
                flag(r.has_sub_set),
                flag(r.continues),
                u[4],
                u[5],
                u[6],
                u[7],
            )
        })
        .collect();
    format!(
        "DELETE FROM `treasure_wielded` WHERE `treasure_Type` = {id};\n\n\
         INSERT INTO `treasure_wielded` ({})\nVALUES {};\n",
        cols.join(", "),
        lines.join("\n     , ")
    )
}

/// Whether `text` holds nothing but comments and blank lines.
fn holds_nothing(text: &str) -> bool {
    let mut rest = text;
    let mut kept = String::new();
    while let Some(i) = rest.find("/*") {
        kept.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |e| &rest[i + e + 2..]);
    }
    kept.push_str(rest);
    kept.lines()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with("--"))
}

/// A reader of a table's type as a file writes it: its id, for a number or a symbol's token.
pub type IdOf<'a> = &'a dyn Fn(&str) -> Option<u32>;

/// The statements ``DELETE FROM `<table>` WHERE `treasure_Type` = <id>;`` of `text` whose type
/// `id_of` reads as `id`, each as the byte range of the whole statement with the line break after
/// it.
fn table_deletes(
    text: &str,
    kind: LootKind,
    id_of: IdOf<'_>,
    id: u32,
) -> Vec<std::ops::Range<usize>> {
    let head = format!("DELETE FROM `{}` WHERE `treasure_Type` = ", kind.table());
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(&head) {
        let start = from + i;
        let value = start + head.len();
        let Some(end) = text[value..].find(';').map(|e| value + e) else {
            break;
        };
        // Its line break, and the blank line after it.
        let mut whole = end + 1;
        for _ in 0..2 {
            if text[whole..].starts_with("\r\n") {
                whole += 2;
            } else if text[whole..].starts_with('\n') {
                whole += 1;
            }
        }
        if id_of(text[value..end].trim()) == Some(id) {
            out.push(start..whole);
        }
        from = end;
    }
    out
}

/// `text` without the rows of table `id` of `kind` (its type read by `id_of`) and the `DELETE`s
/// before them: `Some(None)` when nothing else is left in it, `None` when it holds no such table.
#[must_use]
pub fn without_table(
    text: &str,
    kind: LootKind,
    id_of: IdOf<'_>,
    id: u32,
) -> Option<Option<String>> {
    let mut doc = SqlDoc::parse(text);
    let rows: Vec<_> = doc
        .rows(kind.table())
        .into_iter()
        .filter(|r| doc.cell(*r, "treasure_Type").and_then(|v| id_of(&v)) == Some(id))
        .collect();
    if rows.is_empty() {
        return None;
    }
    for r in rows.into_iter().rev() {
        doc.remove_row(r);
    }
    let mut out = doc.render();
    for range in table_deletes(&out, kind, id_of, id).into_iter().rev() {
        out.replace_range(range, "");
    }
    Some((!holds_nothing(&out)).then_some(out))
}

/// What a death treasure column takes, `value` as typed, written: `None` when it is no value
/// of the column.
fn death_typed(column: &str, value: &str) -> Option<String> {
    let v = content::bare(value);
    match column {
        "loot_Quality_Mod" => v.parse::<f32>().ok().filter(|f| f.is_finite()).map(decimal),
        "tier" => v
            .parse::<i32>()
            .ok()
            .filter(|t| TIERS.contains(t))
            .map(|n| n.to_string()),
        c if c.ends_with("_Chance") => v
            .parse::<i32>()
            .ok()
            .filter(|n| (0..=100).contains(n))
            .map(|n| n.to_string()),
        c if DEATH_COLUMNS.contains(&c) => v
            .parse::<i32>()
            .ok()
            .filter(|n| *n >= 0)
            .map(|n| n.to_string()),
        _ => None,
    }
}

impl Session {
    /// The loot table `id` a generator's treasure entry names: the death treasure when the world
    /// has one of that id, else the wielded treasure.
    #[must_use]
    pub fn loot_kind_of(&self, id: u32) -> LootKind {
        if self.treasure(id).is_some() || !self.wielded_ids().contains(&id) {
            LootKind::Death
        } else {
            LootKind::Wielded
        }
    }

    /// Every wielded treasure's id: the packs' and the server's.
    fn wielded_ids(&self) -> std::collections::BTreeSet<u32> {
        self.server_wielded()
            .keys()
            .chain(self.pack_loot().wielded.keys())
            .copied()
            .collect()
    }

    /// The wielded loot tables that hold weenie `w`, by id: the packs' as their files have them (a
    /// later pack's over an earlier's), and the server's own but those a pack writes again.
    #[must_use]
    pub fn loot_holding(&self, w: &Weenie) -> Vec<u32> {
        let packs = self.packs();
        let loot = self.pack_loot();
        // Whether a row's weenie, as pack `pack` (none: the server) writes it, is `w`.
        let names = |pack: Option<&str>, v: &str| match (sql_ids::number(v), w) {
            (Some(c), Weenie::Class(x)) => c == *x,
            (None, Weenie::Symbol(s)) => {
                pack.and_then(|p| token_symbol(p, v)).as_deref() == Some(s.as_str())
            }
            _ => false,
        };
        let mut out: Vec<u32> = loot
            .wielded
            .iter()
            .filter(|(_, (rows, f))| {
                let pack = packs.get(f.pack).map(|p| p.name.as_str());
                rows.iter().any(|r| names(pack, &r.weenie))
            })
            .map(|(id, _)| *id)
            .collect();
        if matches!(w, Weenie::Class(_)) {
            for (id, rows) in self.server_wielded() {
                if !loot.wielded.contains_key(id) && rows.iter().any(|r| names(None, &r.weenie)) {
                    out.push(*id);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Wielded treasure `id`'s rows as they stand: a pack's, else the server's.
    #[must_use]
    pub fn wielded_rows(&self, id: u32) -> Option<Vec<WieldedItem>> {
        self.pack_loot()
            .wielded
            .get(&id)
            .map(|(r, _)| r.clone())
            .or_else(|| self.server_wielded().get(&id).cloned())
    }

    /// Every loot table of the world by kind and id: those the packs and the server hold, and
    /// those something names that nothing holds; each with where it is held and every weenie
    /// that names it.
    #[must_use]
    pub fn world_loot(&self) -> Vec<LootTable> {
        let mut all: BTreeMap<(LootKind, u32), LootTable> = BTreeMap::new();
        let blank = |kind: LootKind, id: u32| LootTable {
            kind,
            id,
            rows: None,
            server: None,
            file: None,
            pack: None,
            symbol: None,
            whose: Whose::Nobody,
            uses: Vec::new(),
        };
        for (id, t) in self.server_treasures() {
            let e = all
                .entry((LootKind::Death, *id))
                .or_insert_with(|| blank(LootKind::Death, *id));
            e.rows = Some(LootRows::Death(t.clone()));
            e.server.clone_from(&e.rows);
            e.whose = Whose::Server;
        }
        for (id, rows) in self.server_wielded() {
            let e = all
                .entry((LootKind::Wielded, *id))
                .or_insert_with(|| blank(LootKind::Wielded, *id));
            e.rows = Some(LootRows::Wielded(rows.clone()));
            e.server.clone_from(&e.rows);
            e.whose = Whose::Server;
        }
        let packs = self.packs();
        let target = self.target();
        let loot = self.pack_loot();
        let held =
            loot.death
                .iter()
                .map(|(id, (t, f))| ((LootKind::Death, *id), LootRows::Death(t.clone()), f))
                .chain(loot.wielded.iter().map(|(id, (r, f))| {
                    ((LootKind::Wielded, *id), LootRows::Wielded(r.clone()), f)
                }));
        for (key, rows, f) in held {
            let e = all.entry(key).or_insert_with(|| blank(key.0, key.1));
            let mine = f.pack == target;
            e.rows = Some(rows);
            e.file = Some(f.file.clone());
            e.pack = Some(packs[f.pack].name.clone());
            e.symbol = f.written.starts_with("{{").then(|| f.written.clone());
            e.whose = if e.server.is_some() {
                Whose::Changed { mine }
            } else {
                Whose::Pack { mine }
            };
        }
        // Who names each: a death treasure by data id 35, a wielded one by data id 32, and a
        // generator's treasure entry the death treasure when there is one of its id.
        let deaths: std::collections::BTreeSet<u32> = all
            .keys()
            .filter(|(k, _)| *k == LootKind::Death)
            .map(|(_, id)| *id)
            .collect();
        let kind_of = |table: u32, how: LootUse| match how {
            LootUse::Dies => LootKind::Death,
            LootUse::Wields => LootKind::Wielded,
            LootUse::Generator { .. } => {
                if deaths.contains(&table) || !all.contains_key(&(LootKind::Wielded, table)) {
                    LootKind::Death
                } else {
                    LootKind::Wielded
                }
            }
        };
        let mut found: Vec<((LootKind, u32), LootUser)> = Vec::new();
        // The packs' weenies, a later pack's file of one over an earlier's.
        let mut written: HashSet<Weenie> = HashSet::new();
        for p in packs.iter().rev() {
            let as_weenie = |v: &str| match sql_ids::number(v) {
                Some(c) => Some(Weenie::Class(c)),
                None => token_symbol(&p.name, v).map(Weenie::Symbol),
            };
            for f in &p.sql {
                let owners: Vec<Weenie> = sql_ids::weenie_facts(&f.text)
                    .keys()
                    .filter_map(|k| as_weenie(k))
                    .collect();
                if owners.iter().any(|o| written.contains(o)) {
                    continue;
                }
                let text = with_ids(&p.name, &f.path, &f.text, &self.compiled().lock);
                for u in sql_ids::loot_uses(&text) {
                    let Some(by) = as_weenie(&u.by) else { continue };
                    found.push((
                        (kind_of(u.table, u.how), u.table),
                        LootUser {
                            by,
                            how: u.how,
                            file: Some(f.path.clone()),
                        },
                    ));
                }
                written.extend(owners);
            }
        }
        // The server's own, but those a pack writes again.
        let classes = self.pack_classes();
        for (table, named) in self.server_loot_uses() {
            for (by, how) in named {
                if written.contains(&Weenie::Class(*by)) || classes.contains_key(by) {
                    continue;
                }
                found.push((
                    (kind_of(*table, *how), *table),
                    LootUser {
                        by: Weenie::Class(*by),
                        how: *how,
                        file: None,
                    },
                ));
            }
        }
        for (key, user) in found {
            all.entry(key)
                .or_insert_with(|| blank(key.0, key.1))
                .uses
                .push(user);
        }
        all.into_values().collect()
    }

    /// Table `id` of `kind` of the world, as [`Self::world_loot`] has it.
    #[must_use]
    pub fn loot_table(&self, kind: LootKind, id: u32) -> Option<LootTable> {
        self.world_loot()
            .into_iter()
            .find(|t| t.kind == kind && t.id == id)
    }

    /// The pack being edited's table `id` of `kind`, of `whose`, or the refusal.
    fn loot_of(&self, kind: LootKind, id: u32, whose: &[Whose]) -> Result<LootTable, Error> {
        self.loot_table(kind, id)
            .filter(|t| whose.contains(&t.whose))
            .ok_or_else(|| {
                Error::Refused(format!(
                    "{} {id} is not a table the pack being edited may change so",
                    kind.table()
                ))
            })
    }

    /// The file the pack being edited holds its copy of the server's table `id` of `kind` in.
    #[must_use]
    pub fn loot_file(&self, kind: LootKind, id: u32) -> PathBuf {
        self.loot_file_named(kind, &format!("{id:05}"))
    }

    /// The file the pack being edited holds a table of `kind` named `name` in.
    fn loot_file_named(&self, kind: LootKind, name: &str) -> PathBuf {
        self.packs()[self.target()]
            .dir
            .join("server")
            .join("sql")
            .join(kind.table())
            .join(format!("{name}.sql"))
    }

    /// The id of a table's type as a file of the pack being edited writes it: a number, or a
    /// symbol's token as the build numbers it.
    #[must_use]
    pub fn loot_id_of(&self, written: &str) -> Option<u32> {
        let w = content::bare(written);
        if let Some(n) = sql_ids::number(&w) {
            return Some(n);
        }
        let inner = w.strip_prefix("{{")?.strip_suffix("}}")?;
        let (kind, sym) = inner.split_once(':')?;
        if kind.trim() != Kind::Treasure.name() {
            return None;
        }
        let pack = &self.packs()[self.target()].name;
        let sym = crate::symbol::qualify(pack, sym.trim()).ok()?;
        self.treasure_id(&sym)
    }

    /// How a file of the pack being edited names table `t`: by its symbol's token when a pack
    /// names it by one, else by its id.
    #[must_use]
    pub fn loot_written(&self, t: &LootTable) -> String {
        let full = t
            .symbol_name()
            .zip(t.pack.as_deref())
            .and_then(|(name, pack)| crate::symbol::qualify(pack, name).ok());
        match full {
            Some(full) => token_of(&self.packs()[self.target()].name, &full),
            None => t.id.to_string(),
        }
    }

    /// The id the build gave loot table symbol `symbol` (in full).
    #[must_use]
    pub fn treasure_id(&self, symbol: &str) -> Option<u32> {
        self.compiled().lock.get(Kind::Treasure, symbol)
    }

    /// The name of the weenie a table's row names as written: a server's class or a pack's own.
    fn row_name(&self, written: &str) -> Option<String> {
        if let Some(c) = sql_ids::number(written) {
            return self.class_facts(c).and_then(|f| f.name);
        }
        let pack = &self.packs()[self.target()].name;
        let sym = token_symbol(pack, written)?;
        self.pack_weenies().remove(&sym).and_then(|f| f.name)
    }

    /// Table `id` (its type as written: a number, or a symbol's token) holding `rows`, as its own
    /// file writes it.
    #[must_use]
    pub fn loot_text(&self, id: &str, rows: &LootRows) -> String {
        match rows {
            LootRows::Death(t) => death_text(id, t, self.stamp()),
            LootRows::Wielded(r) => {
                let names: BTreeMap<String, Option<String>> = r
                    .iter()
                    .map(|x| (x.weenie.trim().to_owned(), self.row_name(&x.weenie)))
                    .collect();
                wielded_text(id, r, self.stamp(), &|w| {
                    names.get(w.trim()).cloned().flatten()
                })
            }
        }
    }

    /// The server's own table `id` of `kind`, as its own file writes it: what a server whose
    /// copy of it was changed takes to have it back.
    #[must_use]
    pub fn loot_server_text(&self, kind: LootKind, id: u32) -> Option<String> {
        let rows = match kind {
            LootKind::Death => LootRows::Death(self.server_treasures().get(&id)?.clone()),
            LootKind::Wielded => LootRows::Wielded(self.server_wielded().get(&id)?.clone()),
        };
        Some(self.loot_text(&id.to_string(), &rows))
    }

    /// Server table `id` of `kind` copied into the pack being edited as the server has it: the
    /// pack's copy then stands for it, to be changed.
    ///
    /// # Errors
    /// No such table of the server's, or one a pack holds already.
    pub fn loot_change_here(&self, kind: LootKind, id: u32) -> Result<Op, Error> {
        let t = self.loot_of(kind, id, &[Whose::Server])?;
        let rows = t
            .server
            .ok_or_else(|| Error::Refused(format!("the server has no {} {id}", kind.table())))?;
        Ok(Op::WriteSql {
            file: self.loot_file(kind, id),
            text: Some(self.loot_text(&id.to_string(), &rows)),
        })
    }

    /// The pack being edited's table `id` of `kind` taken out of its file: the server's own then
    /// stands again (`server`), or the pack's own goes, once nothing names it.
    ///
    /// # Errors
    /// Not the pack being edited's copy of a server's table (`server`), or a table of its own
    /// that nothing names.
    pub fn loot_take_away(&self, kind: LootKind, id: u32, server: bool) -> Result<Op, Error> {
        let whose = if server {
            Whose::Changed { mine: true }
        } else {
            Whose::Pack { mine: true }
        };
        let t = self.loot_of(kind, id, &[whose])?;
        if !server && !t.uses.is_empty() {
            return Err(Error::Refused(format!(
                "{} {id} is still used in {} place(s)",
                kind.table(),
                t.places()
            )));
        }
        let file = t
            .file
            .ok_or_else(|| Error::Refused(format!("{} {id} is in no file", kind.table())))?;
        let text = self
            .packs()
            .iter()
            .flat_map(|p| &p.sql)
            .find(|f| f.path == file)
            .map(|f| f.text.clone())
            .unwrap_or_default();
        let text = without_table(&text, kind, &|w| self.loot_id_of(w), id).ok_or_else(|| {
            Error::Refused(format!(
                "{} {id} is not in {}",
                kind.table(),
                file.display()
            ))
        })?;
        Ok(Op::WriteSql { file, text })
    }

    /// Whether `name` names a table of the pack being edited already: the token of one, or its
    /// file.
    fn loot_name_taken(&self, kind: LootKind, name: &str) -> bool {
        let pack = &self.packs()[self.target()].name;
        let token = token_of(pack, &format!("{pack}:{name}"));
        let loot = self.pack_loot();
        let held = loot
            .death
            .values()
            .map(|(_, f)| f)
            .chain(loot.wielded.values().map(|(_, f)| f))
            .any(|f| f.pack == self.target() && f.written == token);
        let file = self.loot_file_named(kind, name);
        held || self.packs()[self.target()]
            .sql
            .iter()
            .any(|f| f.path == file)
    }

    /// A new table of `kind` in the pack being edited, named `name` (else `loot-1`, `loot-2`,
    /// the first no table of the pack has): a copy of `from` (a table of the world of the same
    /// kind), else a plain start (a wielded treasure's, one set of `first` alone). Answers the
    /// change and the new table's symbol, in full, which the build numbers.
    ///
    /// # Errors
    /// No such table to copy, a wielded treasure with nothing to start from, or a name that is
    /// not a symbol's or that a table has.
    pub fn loot_new(
        &self,
        kind: LootKind,
        from: Option<u32>,
        first: Option<&str>,
        name: Option<&str>,
    ) -> Result<(Op, String), Error> {
        let pack = self.packs()[self.target()].name.clone();
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => {
                if !crate::symbol::is_name(n) {
                    return Err(Error::Refused(format!(
                        "{n:?} is not a table's name: lowercase letters, digits and hyphens"
                    )));
                }
                if self.loot_name_taken(LootKind::Death, n)
                    || self.loot_name_taken(LootKind::Wielded, n)
                {
                    return Err(Error::Refused(format!(
                        "a table named {n} is there already"
                    )));
                }
                n.to_owned()
            }
            None => (1..)
                .map(|i| format!("loot-{i}"))
                .find(|n| {
                    !self.loot_name_taken(LootKind::Death, n)
                        && !self.loot_name_taken(LootKind::Wielded, n)
                        && self.treasure_id(&format!("{pack}:{n}")).is_none()
                })
                .unwrap_or_default(),
        };
        let rows = match from {
            Some(f) => self
                .loot_table(kind, f)
                .and_then(|t| t.rows)
                .ok_or_else(|| Error::Refused(format!("there is no {} {f}", kind.table())))?,
            None => match kind {
                LootKind::Death => LootRows::Death(plain_death()),
                LootKind::Wielded => {
                    let first = first.ok_or_else(|| {
                        Error::Refused("a new wielded treasure starts with something".into())
                    })?;
                    LootRows::Wielded(vec![WieldedItem {
                        weenie: first.trim().to_owned(),
                        probability: 1.0,
                        set_start: true,
                        ..WieldedItem::default()
                    }])
                }
            },
        };
        let symbol = format!("{pack}:{name}");
        Ok((
            Op::WriteSql {
                file: self.loot_file_named(kind, &name),
                text: Some(self.loot_text(&token_of(&pack, &symbol), &rows)),
            },
            symbol,
        ))
    }

    /// The pack being edited's file of table `id` of `kind`, its text there, and the table's type
    /// as it writes it, for a change of it: its own, or its copy of the server's.
    fn editable_loot(&self, kind: LootKind, id: u32) -> Result<(PathBuf, String, String), Error> {
        let t = self.loot_of(
            kind,
            id,
            &[Whose::Pack { mine: true }, Whose::Changed { mine: true }],
        )?;
        let file = t
            .file
            .ok_or_else(|| Error::Refused(format!("{} {id} is in no file", kind.table())))?;
        let text = self
            .packs()
            .iter()
            .flat_map(|p| &p.sql)
            .find(|f| f.path == file)
            .map(|f| f.text.clone())
            .unwrap_or_default();
        let written = t.symbol.clone().unwrap_or_else(|| id.to_string());
        Ok((file, text, written))
    }

    /// Column `column` of the pack's death treasure `id` set to `value`, as typed: a tier from 1
    /// to 8, a quality, a chance from 0 to 100, or a count or a mix of 0 or more, a group's least
    /// no more than its most.
    ///
    /// # Errors
    /// Not the pack's own table or its copy of the server's, no such column, or no value of it.
    pub fn death_set(&self, id: u32, column: &str, value: &str) -> Result<Op, Error> {
        let (file, text, _) = self.editable_loot(LootKind::Death, id)?;
        let v = death_typed(column, value)
            .ok_or_else(|| Error::Refused(format!("{value:?} is no value of {column}")))?;
        let mut doc = SqlDoc::parse(&text);
        let row = doc
            .rows(LootKind::Death.table())
            .into_iter()
            .find(|r| {
                doc.cell(*r, "treasure_Type")
                    .and_then(|v| self.loot_id_of(&v))
                    == Some(id)
            })
            .ok_or_else(|| Error::Refused(format!("treasure_death {id} has no row")))?;
        // A group's least is no more than its most.
        for g in Group::ALL {
            let [_, lo, hi, _] = g.columns();
            let other = if column == lo {
                Some((hi, true))
            } else if column == hi {
                Some((lo, false))
            } else {
                None
            };
            if let Some((o, is_least)) = other {
                let n: i32 = v.parse().unwrap_or(0);
                let m: i32 = doc.cell(row, o).and_then(|x| x.parse().ok()).unwrap_or(0);
                if (is_least && n > m) || (!is_least && n < m) {
                    return Err(Error::Refused(format!(
                        "the least of a group is no more than its most ({lo} {}, {hi} {})",
                        if is_least { n } else { m },
                        if is_least { m } else { n }
                    )));
                }
            }
        }
        if !doc.set_cell(row, column, &v) {
            return Err(Error::Refused(format!("treasure_death has no {column}")));
        }
        Ok(Op::WriteSql {
            file,
            text: Some(doc.render()),
        })
    }

    /// The pack's wielded treasure `id` written as `rows`, in order: its own file written anew,
    /// or in another file of the pack, its rows there replaced.
    ///
    /// # Errors
    /// Not the pack's own table or its copy of the server's, or no rows.
    pub fn wielded_set(&self, id: u32, rows: &[WieldedItem]) -> Result<Op, Error> {
        if rows.is_empty() {
            return Err(Error::Refused(
                "a wielded treasure makes something; delete it instead".into(),
            ));
        }
        let (file, text, written) = self.editable_loot(LootKind::Wielded, id)?;
        let new = self.loot_text(&written, &LootRows::Wielded(rows.to_vec()));
        let id_of = |w: &str| self.loot_id_of(w);
        let text = match without_table(&text, LootKind::Wielded, &id_of, id).flatten() {
            Some(rest) => format!("{}\n\n{new}", rest.trim_end()),
            None => new,
        };
        Ok(Op::WriteSql {
            file,
            text: Some(text),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: Lyceum's loot tables written and taken out of a file)
    use super::*;

    #[test]
    fn a_tables_own_file_is_its_rows_deleted_then_written_and_without_them_is_nothing() {
        let t = plain_death();
        let text = death_text("37", &t, "2026-10-25 00:00:00");
        assert!(text.starts_with("DELETE FROM `treasure_death` WHERE `treasure_Type` = 37;\n\n"));
        let back = sql_ids::death_treasures_in(&text);
        assert_eq!(back.get(&37), Some(&t), "read back as written");
        let n = |v: &str| sql_ids::number(v);
        assert_eq!(without_table(&text, LootKind::Death, &n, 37), Some(None));
        assert_eq!(without_table(&text, LootKind::Death, &n, 38), None);
        assert_eq!(without_table(&text, LootKind::Wielded, &n, 37), None);
        // Beside another table, only its own rows and delete go.
        let rows = vec![WieldedItem {
            weenie: "273".into(),
            probability: 1.0,
            set_start: true,
            stack: 200,
            ..WieldedItem::default()
        }];
        let w = wielded_text("37", &rows, "2026-10-25 00:00:00", &|w| {
            (w == "273").then(|| "Pyreal".to_owned())
        });
        assert!(
            w.contains("True, False, False") && w.contains("/* Pyreal */"),
            "{w}"
        );
        assert_eq!(sql_ids::wielded_in(&w).get(&37), Some(&rows));
        let both = format!("{text}\n{w}");
        let left = without_table(&both, LootKind::Death, &n, 37)
            .unwrap()
            .unwrap();
        assert!(
            !left.contains("treasure_death") && left.contains("treasure_wielded"),
            "{left}"
        );
    }

    #[test]
    fn every_mix_adds_up_to_one_and_a_death_column_takes_only_its_values() {
        for g in Group::ALL {
            for (i, mix) in g.mixes().iter().enumerate() {
                let sum: f32 = mix.iter().map(|(_, p)| p).sum();
                assert!((sum - 1.0).abs() < 1e-3, "{g:?} {}: {sum}", i + 1);
            }
        }
        for mix in HERITAGE_MIXES {
            let sum: f32 = mix.iter().map(|(_, p)| p).sum();
            assert!((sum - 1.0).abs() < 1e-3);
        }
        assert_eq!(death_typed("tier", "9"), None);
        assert_eq!(death_typed("tier", " 4 "), Some("4".into()));
        assert_eq!(death_typed("item_Chance", "101"), None);
        assert_eq!(death_typed("magic_Item_Chance", "35"), Some("35".into()));
        assert_eq!(death_typed("loot_Quality_Mod", "0.25"), Some("0.25".into()));
        assert_eq!(death_typed("item_Min_Amount", "-1"), None);
        assert_eq!(death_typed("id", "1"), None);
    }
}
