//! A world's lockfile: the ids its symbols were given, kept so a rebuild gives the same ones.
//!
//! ```toml
//! # This world's ids, numbered by lyceum build. Commit it with world.toml: a rebuild keeps
//! # every id here.
//!
//! [render-surface]
//! "sample:stripes" = 0x06007577
//!
//! [wcid]
//! "sample:guide" = 100000
//!
//! [guid]
//! "sample:guide@A9B4" = 0x7A9B4FFF
//!
//! [cell]
//! "sample:hall@A9B4" = 0xA9B4018A
//! ```
//!
//! Ids are per world: one world's lockfile says nothing about another's. A symbol keeps its id as
//! long as the lockfile holds it, and the lockfile keeps a symbol no source uses any more, so an id
//! is never handed to something else in the same world.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::error::{Error, Loc};
use crate::symbol::Kind;

/// The ids of a world's symbols, by kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    ids: BTreeMap<Kind, BTreeMap<String, u32>>,
}

impl Lock {
    /// The lockfile at `path`; an empty one when there is none yet.
    ///
    /// # Errors
    /// A lockfile that does not read.
    pub fn read(path: &Path) -> Result<Self, Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(path, &text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::io(path, e)),
        }
    }

    /// A lockfile's text, read; `path` names it in errors.
    ///
    /// # Errors
    /// Text that is not a lockfile.
    pub fn parse(path: &Path, text: &str) -> Result<Self, Error> {
        let table: toml::Table = toml::from_str(text).map_err(|e| {
            let at = e
                .span()
                .map_or_else(|| Loc::new(path, 0), |s| Loc::at(path, text, s.start));
            Error::source(&at, e.message().to_owned())
        })?;
        let at = Loc::new(path, 0);
        let mut lock = Self::default();
        for (kind, symbols) in table {
            let k = Kind::parse(&kind)
                .ok_or_else(|| Error::source(&at, format!("[{kind}] is not a kind of symbol")))?;
            let toml::Value::Table(symbols) = symbols else {
                return Err(Error::source(&at, format!("[{kind}] is a table")));
            };
            for (symbol, id) in symbols {
                let id = id
                    .as_integer()
                    .and_then(|i| u32::try_from(i).ok())
                    .ok_or_else(|| {
                        Error::source(&at, format!("{kind} {symbol:?} is not given an id"))
                    })?;
                lock.insert(k, &symbol, id);
            }
        }
        Ok(lock)
    }

    /// The lockfile's text.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# This world's ids, numbered by lyceum build. Commit it with world.toml: a rebuild\n\
             # keeps every id here.\n",
        );
        for (kind, symbols) in &self.ids {
            if symbols.is_empty() {
                continue;
            }
            let _ = write!(out, "\n[{kind}]\n");
            for (symbol, id) in symbols {
                if kind.hex() {
                    let _ = writeln!(out, "{symbol:?} = 0x{id:08X}");
                } else {
                    let _ = writeln!(out, "{symbol:?} = {id}");
                }
            }
        }
        out
    }

    /// The id `symbol` of `kind` was given.
    #[must_use]
    pub fn get(&self, kind: Kind, symbol: &str) -> Option<u32> {
        self.ids.get(&kind)?.get(symbol).copied()
    }

    /// Give `symbol` of `kind` the id `id`.
    pub fn insert(&mut self, kind: Kind, symbol: &str, id: u32) {
        self.ids
            .entry(kind)
            .or_default()
            .insert(symbol.to_owned(), id);
    }

    /// Every id of `kind`, with its symbol.
    pub fn of(&self, kind: Kind) -> impl Iterator<Item = (&str, u32)> + '_ {
        self.ids
            .get(&kind)
            .into_iter()
            .flat_map(|m| m.iter().map(|(s, id)| (s.as_str(), *id)))
    }

    /// Whether the lockfile holds no id at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.values().all(BTreeMap::is_empty)
    }

    /// Whether any symbol of any kind holds `id` among the data-file kinds.
    #[must_use]
    pub fn holds_record_id(&self, id: u32) -> bool {
        Kind::ALL
            .iter()
            .filter(|k| k.is_record())
            .any(|k| self.of(*k).any(|(_, x)| x == id))
    }
}
