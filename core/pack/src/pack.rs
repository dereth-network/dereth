//! A pack's header, `pack.toml`: what the pack is called, its version and who made it.
//!
//! ```toml
//! name = "holtburg"          # the pack's name, which its symbols are qualified with
//! version = "0.1.0"
//! game = "ac1"               # the game it is for; left out, the first game
//! authors = ["A. Builder"]
//! license = "MIT"
//! description = "A guide at Holtburg's gate."
//! ```
//!
//! Only `pack.toml` is required of a pack; the rest of its folder is the game's ([`crate::Dialect`]),
//! but for one row the grammar has of its own: `[[remove]]` ([`Remove`]), which takes a row of the
//! world's base database out by its item.

use std::path::Path;

use serde::Deserialize;

use crate::conflict::Scope;
use crate::dialect::{check_game, Dialect};
use crate::error::{Error, Loc};
use crate::provenance::Provenance;
use crate::symbol;

/// A pack's header, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackHeader {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    #[serde(default, rename = "format_version")]
    _format_version: u32,
    name: String,
    version: String,
    game: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    license: Option<String>,
    description: Option<String>,
}

impl PackHeader {
    /// A pack header's text, read; `path` names it in errors.
    ///
    /// # Errors
    /// Text that is not a pack header, a newer format than this program reads, a pack of another
    /// game, or a name that is not one.
    pub fn parse<D: Dialect>(path: &Path, text: &str) -> Result<Self, Error> {
        crate::format::check::<D>(path, text)?;
        let meta: PackFile = toml::from_str(text).map_err(|e| Error::toml(path, text, &e))?;
        check_game::<D>(meta.game.as_deref(), "pack")
            .map_err(|why| Error::source(&Loc::new(path, 0), why))?;
        if !symbol::is_name(&meta.name) {
            return Err(Error::source(
                &Loc::new(path, 0),
                format!(
                    "{:?} is not a pack name: lowercase letters, digits and hyphens",
                    meta.name
                ),
            ));
        }
        Ok(Self {
            name: meta.name,
            version: meta.version,
            authors: meta.authors,
            license: meta.license,
            description: meta.description,
        })
    }
}

/// A row that takes a row of the world's base database out, by the item the build's report names
/// it with:
///
/// ```toml
/// [[remove]]
/// item = "camp c-4586@77B8"
/// prov = { from = "authored", note = "no capture saw it after the update" }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remove {
    pub item: String,
    #[serde(default)]
    pub prov: Option<Provenance>,
}

impl Remove {
    /// The item taken out, in the one form the build's report names it; `at` is where the row is
    /// written.
    ///
    /// # Errors
    /// Text that names no item of the game's, or a whole landblock.
    pub fn item_name<D: Dialect>(&self, at: &Loc) -> Result<String, Error> {
        match Scope::parse::<D>(&self.item).map_err(|why| Error::source(at, why))? {
            Scope::Item(name) => Ok(name),
            Scope::Landblock(_) => Err(Error::source(
                at,
                "a [[remove]] takes out one item, not a landblock",
            )),
        }
    }
}
