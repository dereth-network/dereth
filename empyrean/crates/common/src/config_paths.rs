//! Not ACE: how a path written in `empyrean.toml` becomes a file on disk. One rule for every
//! path-valued key (`dat_files_directory`, `world_pack_path`,
//! `world_overlay_path`, `world_base_sql`, `world_base_patches`, `shard_db_path`, `auth_db_path`):
//!
//! 1. a leading `~` (alone, or followed by `/` or `\`) is the home directory; `~user/...` is kept
//!    as written. Without a home directory the path is kept as written and a warning is logged;
//! 2. an absolute path is used as it is;
//! 3. a relative path is relative to the folder the loaded configuration file is in, or to the
//!    working directory when the server runs on the defaults (no file).
//!
//! Two keys search when they keep their default: an empty `dat_files_directory` and the default
//! `world_pack_path` (`./world.pack`) are looked for in that base folder, then beside the server
//! executable (see [`PathBase::search`]).
//!
//! ACE resolves every path against its working directory.

use std::path::{Path, PathBuf};

/// The folder relative paths resolve against, and the home directory `~` stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathBase {
    /// The loaded configuration file's folder, or the working directory without one. Absolute.
    pub base: PathBuf,
    /// Whether [`Self::base`] is a configuration file's folder (false: the working directory).
    pub from_config: bool,
    /// The home directory, when there is one.
    pub home: Option<PathBuf>,
    /// The server executable's folder, searched after [`Self::base`] by [`Self::search`].
    pub exe_dir: Option<PathBuf>,
}

impl PathBase {
    /// The base for a configuration read from `config_file` (`None`: running on the defaults),
    /// with `cwd` the working directory (a relative `config_file` is under it).
    #[must_use]
    pub fn new(
        config_file: Option<&Path>,
        cwd: &Path,
        home: Option<PathBuf>,
        exe_dir: Option<PathBuf>,
    ) -> Self {
        let cwd = absolute(cwd);
        let (base, from_config) = match config_file {
            Some(file) => {
                let file = if file.is_absolute() {
                    file.to_path_buf()
                } else {
                    cwd.join(file)
                };
                let dir = file.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
                (absolute(&dir), true)
            }
            None => (cwd, false),
        };
        Self {
            base,
            from_config,
            home,
            exe_dir,
        }
    }

    /// [`Self::new`] for this process: its working directory, the user's home directory and the
    /// executable's folder.
    #[must_use]
    pub fn for_process(config_file: Option<&Path>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        Self::new(config_file, &cwd, std::env::home_dir(), exe_dir)
    }

    /// `raw` with a leading `~` (alone, `~/` or `~\`) replaced by the home directory. Any other
    /// path, `~user/...` included, is returned as written; so is a `~` path when there is no home
    /// directory, with a warning.
    #[must_use]
    pub fn expand_home(&self, raw: &str) -> PathBuf {
        let rest = if raw == "~" {
            Some("")
        } else {
            raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\"))
        };
        match (rest, &self.home) {
            (Some(rest), Some(home)) => {
                if rest.is_empty() {
                    home.clone()
                } else {
                    home.join(rest)
                }
            }
            (Some(_), None) => {
                log::warn!("Configuration: {raw} starts with ~ but there is no home directory: the path is used as written");
                PathBuf::from(raw)
            }
            (None, _) => PathBuf::from(raw),
        }
    }

    /// The file `raw` names: `~` expanded, an absolute path kept, a relative one joined to
    /// [`Self::base`]. The value is trimmed first. An empty value stays empty (the key is unset).
    #[must_use]
    pub fn resolve(&self, raw: &str) -> PathBuf {
        let raw = raw.trim();
        if raw.is_empty() {
            return PathBuf::new();
        }
        let path = self.expand_home(raw);
        if path.is_absolute() {
            path
        } else {
            absolute(&self.base.join(path))
        }
    }

    /// A key that searches while it keeps its default (an empty `dat_files_directory`, the
    /// default `world_pack_path`): `relative` under [`Self::base`], then beside the executable;
    /// the first for which `found` holds. When none does, the one under the base, so the error
    /// names it.
    #[must_use]
    pub fn search(&self, relative: &str, found: &dyn Fn(&Path) -> bool) -> PathBuf {
        let candidates = self.search_candidates(relative);
        candidates
            .iter()
            .find(|c| found(c))
            .unwrap_or(&candidates[0])
            .clone()
    }

    /// The folders [`Self::search`] looks in, in order: `relative` under [`Self::base`], then
    /// beside the executable.
    #[must_use]
    pub fn search_candidates(&self, relative: &str) -> Vec<PathBuf> {
        std::iter::once(&self.base)
            .chain(self.exe_dir.as_ref())
            .map(|dir| absolute(&dir.join(relative)))
            .collect()
    }

    /// What [`Self::base`] is, for the logs.
    #[must_use]
    pub fn describe(&self) -> String {
        let what = if self.from_config {
            "the configuration file's folder"
        } else {
            "the working directory"
        };
        format!("{} ({what})", self.base.display())
    }
}

/// `path` made absolute (and its `.` components dropped) without touching the disk; unchanged if
/// that fails.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
