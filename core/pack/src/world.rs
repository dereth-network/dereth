//! A world: the packs it is built from, in order, and where its build goes.
//!
//! ```toml
//! # world.toml
//! key = "sample-world"      # the world's key, which every overlay container records
//! game = "ac1"              # the game it is for; left out, the first game
//! era = "eor"               # the era whose files it is built on, as the game names its eras
//! packs = ["sample"]        # the load order, folders relative to this file
//! imports = []              # overlay folders an import made, laid under the packs
//! out = "build"             # the default output folder, relative to this file
//! [base]                    # the base database under every pack, where the game has one
//! db = "world-db"
//! version = "v0.3.1"        # left out, the release the build pins
//! [ids]
//! wcid = 100000             # where a counted kind's new ids start, where the world may say
//!
//! [[override]]              # where two packs disagree, which wins ([`crate::conflict`])
//! pack = "holtburg-plus"
//! over = "sample"
//! items = ["weenie 1234"]   # these only; without a list, everything both set
//! ```
//!
//! The world's lockfile, `world.lock`, sits beside it ([`crate::lock`]).

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

use crate::conflict::{Override, Scope};
use crate::dialect::{check_game, Dialect};
use crate::error::{Error, Loc};
use crate::store::{DiskStore, SourceStore};
use crate::symbol::{Numbering, SymbolKind};

/// A world, read.
pub struct World<D: Dialect> {
    /// The world file.
    pub path: PathBuf,
    /// The world's key.
    pub key: String,
    /// The era whose files the world is built on.
    pub era: D::Era,
    /// The packs' folders, in load order.
    pub packs: Vec<PathBuf>,
    /// The imported overlay folders, in order.
    pub imports: Vec<PathBuf>,
    /// The output folder.
    pub out: PathBuf,
    /// Where new ids of a counted kind start, for each the world sets ([`World::id_start`]).
    pub ids: BTreeMap<D::Kind, u32>,
    /// Which pack wins where two disagree, as the world says.
    pub overrides: Vec<Override>,
    /// The base database under every pack, when the world names one.
    pub base: Option<Base>,
}

/// The base database a world names (`[base]`): released rows that lie under every pack, which a
/// pack replaces or takes out (`[[remove]]`, [`crate::pack::Remove`]) by their keys, and never
/// conflicts with. Only a game whose builder takes one has it ([`Dialect::NAMES_BASE`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Base {
    /// The database's name.
    pub db: String,
    /// Its release; `None` for the one the build pins.
    pub version: Option<String>,
}

impl<D: Dialect> Clone for World<D> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            key: self.key.clone(),
            era: self.era,
            packs: self.packs.clone(),
            imports: self.imports.clone(),
            out: self.out.clone(),
            ids: self.ids.clone(),
            overrides: self.overrides.clone(),
            base: self.base.clone(),
        }
    }
}

impl<D: Dialect> PartialEq for World<D> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.key == other.key
            && self.era == other.era
            && self.packs == other.packs
            && self.imports == other.imports
            && self.out == other.out
            && self.ids == other.ids
            && self.overrides == other.overrides
            && self.base == other.base
    }
}

impl<D: Dialect> Eq for World<D> {}

impl<D: Dialect> fmt::Debug for World<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World")
            .field("path", &self.path)
            .field("key", &self.key)
            .field("era", &self.era)
            .field("packs", &self.packs)
            .field("imports", &self.imports)
            .field("out", &self.out)
            .field("ids", &self.ids)
            .field("overrides", &self.overrides)
            .field("base", &self.base)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    #[serde(default, rename = "format_version")]
    _format_version: u32,
    key: String,
    game: Option<String>,
    era: Option<String>,
    #[serde(default)]
    packs: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
    out: Option<String>,
    ids: Option<Spanned<BTreeMap<String, Spanned<i64>>>>,
    #[serde(default, rename = "override")]
    overrides: Vec<Spanned<OverrideRow>>,
    base: Option<Spanned<Base>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideRow {
    pack: String,
    over: String,
    items: Option<Vec<String>>,
}

/// The kinds whose start a world's `[ids]` may set.
fn settable<D: Dialect>() -> impl Iterator<Item = D::Kind> {
    D::Kind::KINDS.iter().copied().filter(|k| {
        matches!(
            k.numbering(),
            Numbering::Counted {
                world_sets: true,
                ..
            }
        )
    })
}

impl<D: Dialect> World<D> {
    /// Read the world file at `path`.
    ///
    /// # Errors
    /// A file that does not read, a world of another game or without a key, or an override that
    /// names no item, wins over its own pack, or wins everything over a pack that wins everything
    /// over it.
    pub fn read(path: &Path) -> Result<Self, Error> {
        Self::read_from(&DiskStore, path)
    }

    /// Read the world file at `path` in `store`.
    ///
    /// # Errors
    /// As [`World::read`].
    pub fn read_from(store: &dyn SourceStore, path: &Path) -> Result<Self, Error> {
        let text = store.read_to_string(path).map_err(|e| Error::io(path, e))?;
        Self::parse(path, &text)
    }

    /// A world file's text, read; `path` is where it is, which its folders are relative to.
    ///
    /// # Errors
    /// As [`World::read`].
    pub fn parse(path: &Path, text: &str) -> Result<Self, Error> {
        crate::format::check::<D>(path, text)?;
        let file: WorldFile = toml::from_str(text).map_err(|e| Error::toml(path, text, &e))?;
        check_game::<D>(file.game.as_deref(), "world")
            .map_err(|why| Error::source(&Loc::new(path, 0), why))?;
        if file.key.trim().is_empty() {
            return Err(Error::source(&Loc::new(path, 0), "a world has a key"));
        }
        let era =
            D::era(file.era.as_deref()).map_err(|why| Error::source(&Loc::new(path, 0), why))?;
        let mut ids = BTreeMap::new();
        if let Some(table) = &file.ids {
            for (name, start) in table.get_ref() {
                let at = Loc::at(path, text, start.span().start);
                let Some(kind) = settable::<D>().find(|k| k.kind_name() == name) else {
                    let expected: Vec<String> = settable::<D>().map(|k| format!("`{k}`")).collect();
                    return Err(Error::source(
                        &Loc::at(path, text, table.span().start),
                        format!(
                            "unknown field `{name}`, expected {}",
                            if expected.is_empty() {
                                "none".to_owned()
                            } else {
                                expected.join(" or ")
                            }
                        ),
                    ));
                };
                let start = u32::try_from(*start.get_ref()).map_err(|_| {
                    Error::source(&at, format!("{name}: an id from 0 to {}", u32::MAX))
                })?;
                ids.insert(kind, start);
            }
        }
        let mut overrides = Vec::new();
        for row in &file.overrides {
            let at = Loc::at(path, text, row.span().start);
            let r = row.get_ref();
            if r.pack == r.over {
                return Err(Error::source(
                    &at,
                    format!("{} cannot win over itself", r.pack),
                ));
            }
            let items = r
                .items
                .as_ref()
                .map(|list| list.iter().map(|i| Scope::parse::<D>(i)).collect())
                .transpose()
                .map_err(|why: String| Error::source(&at, why))?;
            overrides.push(Override {
                pack: r.pack.clone(),
                over: r.over.clone(),
                items,
                at,
            });
        }
        // Two packs that each win everything over the other say nothing.
        for (i, a) in overrides.iter().enumerate() {
            if let Some(b) = overrides[..i].iter().find(|b| {
                a.items.is_none() && b.items.is_none() && a.pack == b.over && a.over == b.pack
            }) {
                return Err(Error::source(
                    &a.at,
                    format!(
                        "{} and {} each win everything over the other ({} says so too)",
                        a.pack, a.over, b.at
                    ),
                ));
            }
        }
        if let Some(base) = &file.base {
            if !D::NAMES_BASE {
                return Err(Error::source(
                    &Loc::at(path, text, base.span().start),
                    format!("a {} world names no base database", D::PRODUCT),
                ));
            }
        }
        let dir = path.parent().unwrap_or(Path::new("."));
        Ok(Self {
            path: path.to_path_buf(),
            key: file.key,
            era,
            packs: file.packs.iter().map(|p| dir.join(p)).collect(),
            imports: file.imports.iter().map(|p| dir.join(p)).collect(),
            out: dir.join(file.out.as_deref().unwrap_or("build")),
            ids,
            overrides,
            base: file.base.map(Spanned::into_inner),
        })
    }

    /// Where new ids of the counted `kind` start: where the world says, else the kind's own
    /// start; 0 for a kind that is not counted.
    #[must_use]
    pub fn id_start(&self, kind: D::Kind) -> u32 {
        self.ids
            .get(&kind)
            .copied()
            .unwrap_or(match kind.numbering() {
                Numbering::Counted { start, .. } => start,
                _ => 0,
            })
    }

    /// The world's lockfile, beside the world file.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.path.with_file_name("world.lock")
    }
}
