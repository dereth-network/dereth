//! The Asheron's Call 2 dialect of the shared pack format: its game and era, the items an
//! override or a removal names, and what its symbols number.
//!
//! **Depends on** the shared pack format (`dereth-pack`), whose grammar it fills: worlds and
//! their packs, overrides, removals, provenance and the lockfile. **Used by** the world database
//! (`yalain-world-db`), which reads the database's packs and compiles them for the server, and by
//! the world builder when it builds worlds of this game.
//!
//! A world of this game says `game = "ac2"` in its world file and in every pack's header, and is
//! built on one era of the game's data files: today only the end of retail (`era = "eor"`), the
//! world as the final retail client's files and the January 2017 servers left it. Its packs hold
//! server rows (where people and objects stand, camps, loot, stores, quests, places, what people
//! say), each of which sets one **item** ([`Item`]), named in one form the build's report, an
//! override's `items` and a `[[remove]]` all use:
//!
//! | item | names |
//! |---|---|
//! | `object KEY@XXYY`, `camp KEY@XXYY` | a placed row by its key, in its landblock |
//! | `place KEY` | a destination |
//! | `loot 0xVALUE` | a death-loot profile |
//! | `keeper 0xTEMPLATE` | a storekeeper |
//! | `sale 0xSTORE ORDINAL` | a store's listing |
//! | `quest 0xID` | a quest's server values |
//! | `step 0xQUEST KEY` | a step of a quest |
//! | `talk KEY` | a line a person says |
//!
//! `landblock XXYY`, everything of a landblock, is the grammar's own. Numbers are read in
//! hexadecimal after `0x`, else in decimal, and written back one way, so `keeper 0x470033D0` and
//! `keeper 0x470033d0` are one item.
//!
//! **Must never** know the rows themselves: their fields and what they compile to are the world
//! database's.

use std::fmt;

use dereth_pack::{Dialect, Numbering, SymbolKind};

/// The game's key, as its world and pack files write it.
pub const GAME: &str = "ac2";

/// The eras a world of the game may be built on, by name.
pub const ERAS: [&str; 1] = ["eor"];

/// The game's words for the shared pack format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ac2;

/// An era of the game's data files, which a world is built on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Era {
    /// The end of retail: the final retail client's files and the world the January 2017 servers
    /// ran.
    Eor,
}

impl Era {
    /// How a world file names it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Eor => "eor",
        }
    }
}

/// What the game's symbols name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A static object a pack places: its world id is numbered within its landblock, above the
    /// highest the base uses there.
    Static,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind_name())
    }
}

impl SymbolKind for Kind {
    const KINDS: &'static [Self] = &[Self::Static];

    fn kind_name(self) -> &'static str {
        match self {
            Self::Static => "static",
        }
    }

    fn numbering(self) -> Numbering {
        match self {
            Self::Static => Numbering::InLandblock,
        }
    }

    fn written_in_hex(self) -> bool {
        true
    }
}

impl Dialect for Ac2 {
    const GAME: &'static str = GAME;
    const PRODUCT: &'static str = "Yalain";
    const BUILD_COMMAND: &'static str = "yalain world compile";
    const ITEM_FORMS: &'static str = "object <key>@<XXYY>, camp <key>@<XXYY>, place <key>, \
         loot <value>, keeper <template>, sale <store> <ordinal>, quest <id>, \
         step <quest> <key>, talk <key>, weapon <name>, route <key>, area <XXYY>, \
         clearing <key>@<XXYY>, or landblock <XXYY>";
    const NAMES_BASE: bool = true;
    type Kind = Kind;
    type Era = Era;

    fn era(name: Option<&str>) -> Result<Era, String> {
        match name {
            Some("eor") => Ok(Era::Eor),
            Some(other) => Err(format!(
                "era {other:?} is not one of {ERAS:?}: a world of this game is built on the end of retail"
            )),
            None => Err(format!(
                "a world of this game names its era, one of {ERAS:?}"
            )),
        }
    }

    fn item(_text: &str, words: &[&str]) -> Option<String> {
        Item::parse(words).map(|i| i.to_string())
    }
}

/// One thing a pack's row sets, as the build's report, an override and a removal name it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Item {
    /// A person or an object placed in a landblock, by its key.
    Object { key: String, landblock: u16 },
    /// A spawn camp in a landblock, by its key.
    Camp { key: String, landblock: u16 },
    /// A destination, by its key.
    Place { key: String },
    /// A death-loot profile, by its value.
    Loot { profile: u32 },
    /// A storekeeper, by its template.
    Keeper { template: u32 },
    /// A store's listing, by its store template and its ordinal.
    Sale { store: u32, ordinal: u32 },
    /// A quest's server values, by its id.
    Quest { id: u32 },
    /// A step of a quest, by the quest and the step's key.
    Step { quest: u32, key: String },
    /// A give that moves no quest, by its key.
    Give { key: String },
    /// A line a person says, by its key.
    Talk { key: String },
    /// A craft skill or a recipe a new character starts with, by its data id.
    Craft { id: u32 },
    /// What a butchery profile yields, by the profile's data id.
    Butchery { profile: u32 },
    /// A scene of the story, by its id.
    Scene { id: u32 },
    /// The scene a shard bestows, by the shard's world id.
    Shard { object: u64 },
    /// The template of a weapon creatures wield, by the name their templates give it.
    Weapon { name: String },
    /// What a creature's death effect makes around its killer, by the effect's name.
    DeathSpawn { effect: String },
    /// A kind of pet, by the effect or contract that names it.
    Pet { effect: u32 },
    /// What a summoning skill allows its user, by the skill.
    PetLimit { skill: u32 },
    /// A route creatures walk at rest, by its key.
    Route { key: String },
    /// The kind of player conflict a landblock allows, by the landblock.
    Area { landblock: u16 },
    /// Ground kept free of what the land grows, by its key in its landblock.
    Clearing { key: String, landblock: u16 },
}

impl Item {
    /// An item as an override or a removal writes it, split into words; `None` when it names
    /// none of the game's.
    #[must_use]
    pub fn parse(words: &[&str]) -> Option<Self> {
        Some(match words {
            ["object", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Object { key, landblock }
            }
            ["camp", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Camp { key, landblock }
            }
            ["clearing", placed] => {
                let (key, landblock) = placed_key(placed)?;
                Self::Clearing { key, landblock }
            }
            ["place", key] => Self::Place { key: symbol(key)? },
            ["loot", v] => Self::Loot {
                profile: number(v)?,
            },
            ["keeper", t] => Self::Keeper {
                template: number(t)?,
            },
            ["sale", s, o] => Self::Sale {
                store: number(s)?,
                ordinal: number(o)?,
            },
            ["quest", q] => Self::Quest { id: number(q)? },
            ["step", q, key] => Self::Step {
                quest: number(q)?,
                key: symbol(key)?,
            },
            ["give", key] => Self::Give { key: symbol(key)? },
            ["talk", key] => Self::Talk { key: symbol(key)? },
            ["craft", id] => Self::Craft { id: number(id)? },
            ["butchery", p] => Self::Butchery {
                profile: number(p)?,
            },
            ["scene", id] => Self::Scene { id: number(id)? },
            ["shard", o] => Self::Shard {
                object: number64(o)?,
            },
            ["weapon", name] => Self::Weapon {
                name: identifier(name)?,
            },
            ["death_spawn", effect] => Self::DeathSpawn {
                effect: identifier(effect)?,
            },
            ["pet", effect] => Self::Pet {
                effect: number(effect)?,
            },
            ["pet_limit", skill] => Self::PetLimit {
                skill: number(skill)?,
            },
            ["route", key] => Self::Route { key: symbol(key)? },
            ["area", lb] => Self::Area {
                landblock: u16::from_str_radix(lb, 16).ok().filter(|_| lb.len() == 4)?,
            },
            _ => return None,
        })
    }

    /// The landblock a placed item stands in.
    #[must_use]
    pub const fn landblock(&self) -> Option<u16> {
        match self {
            Self::Object { landblock, .. } | Self::Camp { landblock, .. } => Some(*landblock),
            _ => None,
        }
    }
}

impl fmt::Display for Item {
    /// The one form the build's report names it in.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Object { key, landblock } => write!(f, "object {key}@{landblock:04X}"),
            Self::Camp { key, landblock } => write!(f, "camp {key}@{landblock:04X}"),
            Self::Place { key } => write!(f, "place {key}"),
            Self::Loot { profile } => write!(f, "loot {profile:#x}"),
            Self::Keeper { template } => write!(f, "keeper {template:#010x}"),
            Self::Sale { store, ordinal } => write!(f, "sale {store:#010x} {ordinal}"),
            Self::Quest { id } => write!(f, "quest {id:#010x}"),
            Self::Step { quest, key } => write!(f, "step {quest:#010x} {key}"),
            Self::Give { key } => write!(f, "give {key}"),
            Self::Talk { key } => write!(f, "talk {key}"),
            Self::Craft { id } => write!(f, "craft {id:#010x}"),
            Self::Butchery { profile } => write!(f, "butchery {profile:#010x}"),
            Self::Scene { id } => write!(f, "scene {id:#010x}"),
            Self::Shard { object } => write!(f, "shard {object:#018x}"),
            Self::Weapon { name } => write!(f, "weapon {name}"),
            Self::DeathSpawn { effect } => write!(f, "death_spawn {effect}"),
            Self::Pet { effect } => write!(f, "pet {effect:#010x}"),
            Self::PetLimit { skill } => write!(f, "pet_limit {skill:#010x}"),
            Self::Route { key } => write!(f, "route {key}"),
            Self::Area { landblock } => write!(f, "area {landblock:04X}"),
            Self::Clearing { key, landblock } => write!(f, "clearing {key}@{landblock:04X}"),
        }
    }
}

/// Whether `s` is a key: lowercase letters, digits and hyphens, starting with a letter or a
/// digit, at most 64 long.
#[must_use]
pub fn is_key(s: &str) -> bool {
    s.len() <= 64 && dereth_pack::symbol::is_name(s)
}

/// A name of the install's own: letters, digits and `_`, at most 80 long.
fn identifier(s: &str) -> Option<String> {
    (!s.is_empty() && s.len() <= 80 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then(|| s.to_owned())
}

fn symbol(s: &str) -> Option<String> {
    is_key(s).then(|| s.to_owned())
}

/// A placed row's key with its landblock: `KEY@XXYY`.
fn placed_key(s: &str) -> Option<(String, u16)> {
    let (key, _) = s.split_once('@')?;
    let landblock = dereth_pack::symbol::instance_landblock(s).ok()??;
    Some((symbol(key)?, landblock))
}

/// A number written in hexadecimal after `0x`, else in decimal.
fn number64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn number(s: &str) -> Option<u32> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}
