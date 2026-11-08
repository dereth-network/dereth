//! The Asheron's Call 2 world database: its packs of server rows read and checked, merged in load
//! order, compiled against an install into the server's tables, and packaged as a release.
//!
//! The database is the server's half of the world (where people and objects stand, what spawns
//! and drops, what stores sell, how quests move on, where portals lead) as packs of TOML rows that
//! reference the user's own install by data id.
//!
//! **Depends on** the shared pack format (`dereth-pack`) in this game's dialect
//! (`auberean-pack`), for the world file, the pack headers, the lockfile, provenance, overrides,
//! removals and the merge; the record decoders (`auberean-assets`) and the script library
//! (`auberean-script`), for reading the install's templates, stores and quest definitions; the
//! server's tables (`yalain-content`), which a compile fills; and `dereth-primitives` for its
//! math. **Used by** the `yalain`
//! program (`yalain world ...`, `--world-db`), the server's tests, and the world builder
//! (`athenaeum`), whose worlds lie over a released database ([`layer`]).
//!
//! A database is a folder:
//!
//! ```text
//! world.toml                      format_version, key, game "ac2", era, packs in load order, overrides
//! world.lock                      ids given to authored symbols, kept across builds
//! <pack>/pack.toml                name, version, game, authors, license, description
//! <pack>/server/landblocks/XXYY.toml   [[object]], [[camp]] standing in landblock XXYY
//! <pack>/server/places.toml       [[place]]
//! <pack>/server/loot.toml         [[loot]]
//! <pack>/server/stores.toml       [[keeper]], [[sale]]
//! <pack>/server/quests/QQQQQQQQ.toml   [[quest]], [[step]] of one quest
//! <pack>/server/gives/TTTTTTTT.toml    [[give]] of one template given to
//! <pack>/server/talk/TTTTTTTT.toml     [[talk]] of one template
//! <pack>/server/crafting.toml     [[craft]]
//! <pack>/server/story.toml        [[scene]], [[shard]]
//! <pack>/server/weapons.toml      [[weapon]]
//! <pack>/server/pets.toml         [[pet]], [[pet_limit]]
//! <pack>/server/routes.toml       [[route]]
//! <pack>/server/areas.toml        [[area]]
//! ```
//!
//! Every row carries `prov`, where it came from: `capture` (measured from recorded sessions, with an
//! evidence handle, an opaque reference and a confidence), `dat` (an install record), `derived` (a
//! listed rule) or `authored`. Text the install holds (names, lines, journals) is referenced by
//! string id, never copied; short identifiers of the install (enum names, destination keys) may sit
//! beside their numbers as text; new prose a pack writes goes in a `*_text` field, which a compile
//! uses only where no string reference is given, and which is said as literal text.
//!
//! **Must never** do I/O: the caller hands in the files ([`Files`]) and the install
//! ([`compile::Install`]); a release is bytes out.

use std::collections::BTreeMap;
use std::fmt;

pub mod check;
pub mod compile;
pub mod install;
pub mod layer;
pub mod merge;
pub mod release;
pub mod rows;

pub use rows::{Database, Pack, Row, World};

/// A database's text files, by path relative to its folder (`/` between parts).
pub type Files = BTreeMap<String, String>;

/// The game a database is for.
pub const GAME: &str = auberean_pack::GAME;

/// The eras a database may be of: the end of retail, the world as it stood in January 2017, whose
/// ids resolve in the final retail client's files.
pub const ERAS: [&str; 1] = auberean_pack::ERAS;

/// The pack format this crate reads and writes: the shared pack format's.
pub const FORMAT_VERSION: i64 = dereth_pack::format::CURRENT as i64;

/// Something wrong with a database, where it is: its file and line (0 when the whole file).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Problem {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl Problem {
    #[must_use]
    pub fn new(file: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.message)
        }
    }
}

/// The problems as lines, for a message.
#[must_use]
pub fn report(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
