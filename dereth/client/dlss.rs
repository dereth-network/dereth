//! NVIDIA's DLSS library for a client built from this tree: the files a release carries, pinned
//! in `dlss.env` beside this crate's manifest; where a machine keeps them once
//! `cargo xtask dlss` has fetched them; and where the client's build puts the library, beside the
//! built executable. The client's build script and `cargo xtask dlss` both read this file, so
//! they look in the same place for the same files.
//!
//! The cache is `%LOCALAPPDATA%\Dereth\dlss\<commit>` on Windows,
//! `~/Library/Caches/Dereth/dlss/<commit>` on macOS, and `$XDG_CACHE_HOME/dereth/dlss/<commit>`
//! (else `~/.cache/dereth/dlss/<commit>`) elsewhere: the client's own cache folder, one folder for
//! each pinned commit.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The pin: `KEY=value` lines, which the release workflow reads as shell assignments.
pub const PIN_FILE: &str = include_str!("dlss.env");

/// The library the client loads, as NVIDIA's repository names it.
pub const LIBRARY: &str = "nvngx_dlss.dll";
/// The SDK's license, as NVIDIA's repository names it.
pub const LICENSE: &str = "LICENSE.txt";
/// Where the library is in NVIDIA's repository: the release build, never the development one.
pub const LIBRARY_PATH: &str = "lib/Windows_x86_64/rel/nvngx_dlss.dll";
/// Where the license is in NVIDIA's repository.
pub const LICENSE_PATH: &str = "LICENSE.txt";
/// The folder a build takes the library from in place of the cache.
pub const DIR_ENV: &str = "DERETH_DLSS_DIR";

/// NVIDIA's repository at one commit, and the SHA-256 of each file taken from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub commit: String,
    pub library_sha256: String,
    pub license_sha256: String,
}

impl Pin {
    /// The pin in [`PIN_FILE`].
    ///
    /// # Panics
    /// When the file is not a whole pin, which its own test rules out.
    #[must_use]
    pub fn pinned() -> Self {
        Self::parse(PIN_FILE).expect("dlss.env is a whole pin")
    }

    /// The pin in `text`: `DLSS_COMMIT`, `DLSS_DLL_SHA256` and `DLSS_LICENSE_SHA256`, each once,
    /// as lowercase hexadecimal of their lengths; blank lines and `#` comments between them.
    ///
    /// # Errors
    /// What is missing or malformed.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut commit = None;
        let mut library = None;
        let mut license = None;
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("not a KEY=value line: {line}"))?;
            let (slot, len) = match key {
                "DLSS_COMMIT" => (&mut commit, 40),
                "DLSS_DLL_SHA256" => (&mut library, 64),
                "DLSS_LICENSE_SHA256" => (&mut license, 64),
                _ => return Err(format!("an unknown key: {key}")),
            };
            if value.len() != len
                || !value
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                return Err(format!("{key} is not {len} lowercase hexadecimal digits"));
            }
            if slot.replace(value.to_owned()).is_some() {
                return Err(format!("{key} is given twice"));
            }
        }
        Ok(Self {
            commit: commit.ok_or("no DLSS_COMMIT")?,
            library_sha256: library.ok_or("no DLSS_DLL_SHA256")?,
            license_sha256: license.ok_or("no DLSS_LICENSE_SHA256")?,
        })
    }

    /// Where `file` (a path in NVIDIA's repository) is downloaded from at this commit.
    #[must_use]
    pub fn url(&self, file: &str) -> String {
        format!(
            "https://raw.githubusercontent.com/NVIDIA/DLSS/{}/{file}",
            self.commit
        )
    }
}

/// The host a cache folder is found on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Windows,
    MacOs,
    Other,
}

impl Host {
    /// The host this is compiled for: in a build script, the machine the build runs on.
    #[must_use]
    pub fn this() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }
}

/// The cache folder for `commit` on `host`, from the environment `var` reads; `None` when the
/// variable it is found from is not set to an absolute path.
#[must_use]
pub fn cache_dir(
    host: Host,
    var: impl Fn(&str) -> Option<OsString>,
    commit: &str,
) -> Option<PathBuf> {
    let var = |n: &str| var(n).map(PathBuf::from).filter(|p| p.is_absolute());
    let base = match host {
        Host::Windows => var("LOCALAPPDATA")?.join("Dereth"),
        Host::MacOs => var("HOME")?.join("Library/Caches/Dereth"),
        Host::Other => var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("dereth"),
    };
    Some(base.join("dlss").join(commit))
}

/// The environment variables [`cache_dir`] reads, so a build runs again when one changes.
pub const CACHE_VARS: [&str; 3] = ["LOCALAPPDATA", "HOME", "XDG_CACHE_HOME"];

/// The folder the built executable is in, from the build script's `OUT_DIR`
/// (`<target>/<profile>/build/<package>-<hash>/out`).
#[must_use]
pub fn exe_dir(out_dir: &Path) -> Option<PathBuf> {
    out_dir.ancestors().nth(3).map(Path::to_path_buf)
}

/// What a build does with the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Copy it from `from` to `to`, beside the executable.
    Copy { from: PathBuf, to: PathBuf },
    /// There is none to copy: it was looked for at `looked`, or there was nowhere to look.
    Missing { looked: Option<PathBuf> },
}

/// Where a build takes the library from, and where it puts it: from `dir` ([`DIR_ENV`]) when that
/// is set, else from `cache`, to `exe_dir`; `is_file` says what is there.
#[must_use]
pub fn plan(
    dir: Option<PathBuf>,
    cache: Option<PathBuf>,
    exe_dir: &Path,
    is_file: impl Fn(&Path) -> bool,
) -> Plan {
    let Some(from) = dir.or(cache).map(|d| d.join(LIBRARY)) else {
        return Plan::Missing { looked: None };
    };
    if is_file(&from) {
        Plan::Copy {
            from,
            to: exe_dir.join(LIBRARY),
        }
    } else {
        Plan::Missing { looked: Some(from) }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: where a build from this tree finds NVIDIA's DLSS library).

    use super::*;

    /// The pinned file is a whole pin.
    #[test]
    fn the_pin_file_is_a_whole_pin() {
        let pin = Pin::pinned();
        assert_eq!(pin.commit.len(), 40);
        assert!(pin
            .url(LIBRARY_PATH)
            .ends_with("/lib/Windows_x86_64/rel/nvngx_dlss.dll"));
    }

    /// A pin missing a key, giving one twice, or with a hash of the wrong shape is refused.
    #[test]
    fn a_malformed_pin_is_refused() {
        let commit = format!("DLSS_COMMIT={}", "a".repeat(40));
        let dll = format!("DLSS_DLL_SHA256={}", "b".repeat(64));
        let license = format!("DLSS_LICENSE_SHA256={}", "c".repeat(64));
        let whole = [commit.as_str(), &dll, &license].join("\n");
        assert!(Pin::parse(&whole).is_ok());
        assert!(Pin::parse(&[commit.as_str(), &dll].join("\n")).is_err());
        assert!(Pin::parse(&[whole.as_str(), &dll].join("\n")).is_err());
        assert!(Pin::parse(&whole.replace(&"b".repeat(64), &"B".repeat(64))).is_err());
        assert!(Pin::parse(&whole.replace(&"a".repeat(40), "abc")).is_err());
    }

    /// The cache is the client's own per-user cache folder, one folder for each commit, found from
    /// absolute paths only.
    #[test]
    fn the_cache_is_the_clients_own_cache_folder_by_commit() {
        let root = std::env::temp_dir();
        let (local, xdg, home) = (root.join("local"), root.join("xdg"), root.join("home"));
        let env = |pairs: Vec<(&'static str, PathBuf)>| {
            move |n: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == n)
                    .map(|(_, v)| v.clone().into_os_string())
            }
        };
        let at = |host, pairs| cache_dir(host, env(pairs), "c0");
        assert_eq!(
            at(Host::Windows, vec![("LOCALAPPDATA", local.clone())]),
            Some(local.join("Dereth").join("dlss").join("c0"))
        );
        assert_eq!(
            at(Host::MacOs, vec![("HOME", home.clone())]),
            Some(home.join("Library/Caches/Dereth").join("dlss").join("c0"))
        );
        assert_eq!(
            at(
                Host::Other,
                vec![("XDG_CACHE_HOME", xdg.clone()), ("HOME", home.clone())]
            ),
            Some(xdg.join("dereth").join("dlss").join("c0"))
        );
        assert_eq!(
            at(Host::Other, vec![("HOME", home.clone())]),
            Some(home.join(".cache").join("dereth").join("dlss").join("c0"))
        );
        assert_eq!(
            at(Host::Other, vec![("HOME", PathBuf::from("relative"))]),
            None
        );
        assert_eq!(at(Host::Windows, vec![("HOME", home)]), None);
    }

    /// The executable's folder is three above the build script's own output folder.
    #[test]
    fn the_executable_is_three_folders_above_the_build_scripts_output() {
        let out = Path::new("t")
            .join("release")
            .join("build")
            .join("dereth-client-0123")
            .join("out");
        assert_eq!(exe_dir(&out), Some(Path::new("t").join("release")));
    }

    /// A build copies the library from the folder it is pointed at, else from the cache, beside the
    /// executable; where neither holds it, it copies nothing and says where it looked.
    #[test]
    fn a_build_copies_the_library_from_the_folder_or_the_cache_and_only_where_it_is() {
        let exe = Path::new("exe");
        let dir = PathBuf::from("dir");
        let cache = PathBuf::from("cache");
        let has = |held: &'static str| move |p: &Path| p.starts_with(held);
        let copy = |from: &PathBuf| Plan::Copy {
            from: from.join(LIBRARY),
            to: exe.join(LIBRARY),
        };
        assert_eq!(
            plan(None, Some(cache.clone()), exe, has("cache")),
            copy(&cache)
        );
        assert_eq!(
            plan(Some(dir.clone()), Some(cache.clone()), exe, has("dir")),
            copy(&dir)
        );
        // The folder named wins over the cache, even where only the cache holds the library.
        assert_eq!(
            plan(Some(dir.clone()), Some(cache.clone()), exe, has("cache")),
            Plan::Missing {
                looked: Some(dir.join(LIBRARY))
            }
        );
        assert_eq!(
            plan(None, Some(cache.clone()), exe, has("nowhere")),
            Plan::Missing {
                looked: Some(cache.join(LIBRARY))
            }
        );
        assert_eq!(
            plan(None, None, exe, has("cache")),
            Plan::Missing { looked: None }
        );
    }
}
