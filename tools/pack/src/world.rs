//! A world: the packs it is built from, in order, and where its build goes.
//!
//! ```toml
//! # world.toml
//! key = "sample-world"      # the world's key, which every overlay container records
//! packs = ["sample"]        # the load order, folders relative to this file
//! imports = []              # overlay folders an import made, laid under the packs
//! out = "build"             # the default output folder, relative to this file
//! [ids]
//! wcid = 100000             # where new weenie class ids start
//! ```
//!
//! The world's lockfile, `world.lock`, sits beside it ([`crate::lock`]).

use std::path::{Path, PathBuf};

use serde::Deserialize;

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
    /// The packs' folders, in load order.
    pub packs: Vec<PathBuf>,
    /// The imported overlay folders, in order.
    pub imports: Vec<PathBuf>,
    /// The output folder.
    pub out: PathBuf,
    /// Where new weenie class ids start.
    pub wcid_start: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    key: String,
    #[serde(default)]
    packs: Vec<String>,
    #[serde(default)]
    imports: Vec<String>,
    out: Option<String>,
    #[serde(default)]
    ids: Ids,
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
    /// A file that does not read, or a world without a key.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let file: WorldFile = toml::from_str(&text).map_err(|e| {
            let at = e
                .span()
                .map_or_else(|| Loc::new(path, 0), |s| Loc::at(path, &text, s.start));
            Error::source(&at, e.message().to_owned())
        })?;
        if file.key.trim().is_empty() {
            return Err(Error::source(&Loc::new(path, 0), "a world has a key"));
        }
        let dir = path.parent().unwrap_or(Path::new("."));
        Ok(Self {
            path: path.to_path_buf(),
            key: file.key,
            packs: file.packs.iter().map(|p| dir.join(p)).collect(),
            imports: file.imports.iter().map(|p| dir.join(p)).collect(),
            out: dir.join(file.out.as_deref().unwrap_or("build")),
            wcid_start: file.ids.wcid.unwrap_or(DEFAULT_WCID_START),
        })
    }

    /// The world's lockfile, beside the world file.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.path.with_file_name("world.lock")
    }
}
