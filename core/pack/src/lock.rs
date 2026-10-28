//! A world's lockfile: the ids its symbols were given, kept so a rebuild gives the same ones.
//!
//! ```toml
//! # This world's ids, numbered by lyceum build. Commit it with world.toml: a rebuild
//! # keeps every id here.
//!
//! [render-surface]
//! "sample:stripes" = 0x06007577
//!
//! [wcid]
//! "sample:guide" = 100000
//! ```
//!
//! One table per kind of symbol, in the order the game lists its kinds, each id written in
//! hexadecimal or decimal as the kind's ids are ([`crate::SymbolKind`]).
//!
//! Ids are per world: one world's lockfile says nothing about another's. A symbol keeps its id as
//! long as the lockfile holds it, and the lockfile keeps a symbol no source uses any more, so an id
//! is never handed to something else in the same world.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::path::Path;

use crate::dialect::Dialect;
use crate::error::{Error, Loc};
use crate::store::{DiskStore, SourceStore};
use crate::symbol::SymbolKind;

/// The ids of a world's symbols, by kind.
pub struct Lock<D: Dialect> {
    ids: BTreeMap<D::Kind, BTreeMap<String, u32>>,
}

impl<D: Dialect> Default for Lock<D> {
    fn default() -> Self {
        Self {
            ids: BTreeMap::new(),
        }
    }
}

impl<D: Dialect> Clone for Lock<D> {
    fn clone(&self) -> Self {
        Self {
            ids: self.ids.clone(),
        }
    }
}

impl<D: Dialect> PartialEq for Lock<D> {
    fn eq(&self, other: &Self) -> bool {
        self.ids == other.ids
    }
}

impl<D: Dialect> Eq for Lock<D> {}

impl<D: Dialect> fmt::Debug for Lock<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lock").field("ids", &self.ids).finish()
    }
}

impl<D: Dialect> Lock<D> {
    /// The lockfile at `path`; an empty one when there is none yet.
    ///
    /// # Errors
    /// A lockfile that does not read.
    pub fn read(path: &Path) -> Result<Self, Error> {
        Self::read_from(&DiskStore, path)
    }

    /// The lockfile at `path` in `store`; an empty one when there is none yet.
    ///
    /// # Errors
    /// A lockfile that does not read.
    pub fn read_from(store: &dyn SourceStore, path: &Path) -> Result<Self, Error> {
        match store.read_to_string(path) {
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
        let table: toml::Table = toml::from_str(text).map_err(|e| Error::toml(path, text, &e))?;
        let at = Loc::new(path, 0);
        let mut lock = Self::default();
        for (kind, symbols) in table {
            let k = D::Kind::from_name(&kind)
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
        let mut out = format!(
            "# This world's ids, numbered by {}. Commit it with world.toml: a rebuild\n\
             # keeps every id here.\n",
            D::BUILD_COMMAND
        );
        for (kind, symbols) in &self.ids {
            if symbols.is_empty() {
                continue;
            }
            let _ = write!(out, "\n[{kind}]\n");
            for (symbol, id) in symbols {
                if kind.written_in_hex() {
                    let _ = writeln!(out, "{symbol:?} = 0x{id:08X}");
                } else {
                    let _ = writeln!(out, "{symbol:?} = {id}");
                }
            }
        }
        out
    }

    /// Write the lockfile at `path` in `store` when its text differs from what is there, and
    /// there is a lockfile already or an id to keep. Answers whether it was written.
    ///
    /// # Errors
    /// A lockfile that will not write.
    pub fn save(&self, store: &dyn SourceStore, path: &Path) -> Result<bool, Error> {
        let text = self.render();
        if store.read_to_string(path).ok().as_deref() == Some(text.as_str())
            || !(store.is_file(path) || !self.is_empty())
        {
            return Ok(false);
        }
        store
            .write(path, text.as_bytes())
            .map_err(|e| Error::io(path, e))?;
        Ok(true)
    }

    /// The id `symbol` of `kind` was given.
    #[must_use]
    pub fn get(&self, kind: D::Kind, symbol: &str) -> Option<u32> {
        self.ids.get(&kind)?.get(symbol).copied()
    }

    /// Give `symbol` of `kind` the id `id`.
    pub fn insert(&mut self, kind: D::Kind, symbol: &str, id: u32) {
        self.ids
            .entry(kind)
            .or_default()
            .insert(symbol.to_owned(), id);
    }

    /// Every id of `kind`, with its symbol.
    pub fn of(&self, kind: D::Kind) -> impl Iterator<Item = (&str, u32)> + '_ {
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
        D::Kind::KINDS
            .iter()
            .filter(|k| k.names_record())
            .any(|k| self.of(*k).any(|(_, x)| x == id))
    }
}
