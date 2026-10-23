//! A world: the packs it is built from, in order, and where its build goes.
//!
//! ```toml
//! # world.toml
//! key = "sample-world"      # the world's key, which every overlay container records
//! era = "eor"               # the era whose files it is built on: eor (default) or infiltration
//! packs = ["sample"]        # the load order, folders relative to this file
//! imports = []              # overlay folders an import made, laid under the packs
//! out = "build"             # the default output folder, relative to this file
//! [ids]
//! wcid = 100000             # where new weenie class ids start
//!
//! [[override]]              # where two packs disagree, which wins ([`crate::conflict`])
//! pack = "holtburg-plus"
//! over = "sample"
//! items = ["weenie 1234"]   # these only; without a list, everything both set
//! ```
//!
//! The world's lockfile, `world.lock`, sits beside it ([`crate::lock`]).
//!
//! A world is built on the files of one era, which it names: the end of retail's
//! (`client_portal.dat` and its siblings) or Infiltration's (`portal.dat` and `cell.dat`, from
//! February 2005). It is built over those files and no other, its records are written in their
//! layout, and its server content goes over that era's world database.

use std::path::{Path, PathBuf};

pub use dereth_primitives::EraId;

use serde::Deserialize;
use toml::Spanned;

use crate::conflict::{Override, Scope};
use crate::error::{Error, Loc};

/// Where new weenie class ids start unless the world says: above every class id of the
/// end-of-retail world database (its highest is 90,273).
pub const DEFAULT_WCID_START: u32 = 100_000;

/// A world, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    /// The world file.
    pub path: PathBuf,
    /// The world's key.
    pub key: String,
    /// The era whose files the world is built on.
    pub era: EraId,
    /// The packs' folders, in load order.
    pub packs: Vec<PathBuf>,
    /// The imported overlay folders, in order.
    pub imports: Vec<PathBuf>,
    /// The output folder.
    pub out: PathBuf,
    /// Where new weenie class ids start.
    pub wcid_start: u32,
    /// Which pack wins where two disagree, as the world says.
    pub overrides: Vec<Override>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    #[serde(default, rename = "format_version")]
    _format_version: u32,
    key: String,
    era: Option<String>,
    #[serde(default)]
    packs: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
    out: Option<String>,
    #[serde(default)]
    ids: Ids,
    #[serde(default, rename = "override")]
    overrides: Vec<Spanned<OverrideRow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideRow {
    pack: String,
    over: String,
    items: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Ids {
    wcid: Option<u32>,
}

impl World {
    /// Read the world file at `path`.
    ///
    /// # Errors
    /// A file that does not read, a world without a key, or an override that names no item,
    /// wins over its own pack, or wins everything over a pack that wins everything over it.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        crate::format::check(path, &text)?;
        let file: WorldFile = toml::from_str(&text).map_err(|e| {
            let at = e
                .span()
                .map_or_else(|| Loc::new(path, 0), |s| Loc::at(path, &text, s.start));
            Error::source(&at, e.message().to_owned())
        })?;
        if file.key.trim().is_empty() {
            return Err(Error::source(&Loc::new(path, 0), "a world has a key"));
        }
        let era = match &file.era {
            None => EraId::Eor,
            Some(name) => EraId::parse(name).ok_or_else(|| {
                Error::source(
                    &Loc::new(path, 0),
                    format!("{name:?} is not an era; a world is built on eor or infiltration"),
                )
            })?,
        };
        let mut overrides = Vec::new();
        for row in &file.overrides {
            let at = Loc::at(path, &text, row.span().start);
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
                .map(|list| list.iter().map(|i| Scope::parse(i)).collect())
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
        let dir = path.parent().unwrap_or(Path::new("."));
        Ok(Self {
            path: path.to_path_buf(),
            key: file.key,
            era,
            packs: file.packs.iter().map(|p| dir.join(p)).collect(),
            imports: file.imports.iter().map(|p| dir.join(p)).collect(),
            out: dir.join(file.out.as_deref().unwrap_or("build")),
            wcid_start: file.ids.wcid.unwrap_or(DEFAULT_WCID_START),
            overrides,
        })
    }

    /// The world's lockfile, beside the world file.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.path.with_file_name("world.lock")
    }
}
