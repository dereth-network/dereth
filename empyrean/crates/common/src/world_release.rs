//! Not ACE: the ACE-World release this server build is tested against, and where
//! `empyrean-import fetch` keeps the SQL dump it downloads.
//!
//! The release is the SQL asset of `ACEmulator/ACE-World-16PY-Patches` on GitHub. Bumping it is
//! the two constants below: the tag, and the SHA-256 of the release's `.sql.zip`
//! (`empyrean-import fetch --latest` prints both for the newest release).
//!
//! The cache is a per-user folder, not a place in the workspace, so it survives `cargo clean` and
//! every checkout of the workspace on the machine shares one download:
//!
//! | host | folder |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\Empyrean\world-database` |
//! | macOS | `~/Library/Caches/Empyrean/world-database` |
//! | Linux and other Unix | `$XDG_CACHE_HOME/empyrean/world-database`, else `~/.cache/empyrean/world-database` |
//!
//! It holds one `ACE-World-Database-<tag>.sql` per fetched release, and nothing else once a
//! fetch has finished.

use std::ffi::OsString;
use std::path::PathBuf;

/// The GitHub repository whose releases carry ACE's world database.
pub const REPOSITORY: &str = "ACEmulator/ACE-World-16PY-Patches";

/// The release this build is tested against.
pub const PINNED_TAG: &str = "v0.9.295";

/// The SHA-256 of [`PINNED_TAG`]'s `.sql.zip` asset, lowercase hex.
pub const PINNED_ZIP_SHA256: &str =
    "fd35cff8b2cea8408ae13839b9b1862c30452a1013f43eb62ec83c25349bd148";

/// A release tag from what a user types: `0.9.295` or `v0.9.295` (three dot-separated numbers)
/// gives `v0.9.295`; anything else is `None`.
#[must_use]
pub fn parse_tag(s: &str) -> Option<String> {
    let s = s.trim();
    let digits = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = digits.split('.').collect();
    let numeric = |p: &&str| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit());
    (parts.len() == 3 && parts.iter().all(numeric)).then(|| format!("v{digits}"))
}

/// The release's SQL asset: `ACE-World-Database-<tag>.sql.zip`.
#[must_use]
pub fn zip_name(tag: &str) -> String {
    format!("ACE-World-Database-{tag}.sql.zip")
}

/// The dump inside the asset: `ACE-World-Database-<tag>.sql`.
#[must_use]
pub fn sql_name(tag: &str) -> String {
    format!("ACE-World-Database-{tag}.sql")
}

/// The asset's stable download address (GitHub redirects it to the file).
#[must_use]
pub fn download_url(tag: &str) -> String {
    format!(
        "https://github.com/{REPOSITORY}/releases/download/{tag}/{}",
        zip_name(tag)
    )
}

/// The host family the cache folder follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Windows,
    MacOs,
    Unix,
}

impl Host {
    /// The host this was built for.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Unix
        }
    }
}

/// The per-user cache folder on `host`, reading the environment through `env` (a variable that is
/// unset, empty or not an absolute path counts as absent). `None` when the variables it needs are
/// all absent.
#[must_use]
pub fn cache_dir_for(host: Host, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = match host {
        Host::Windows => var("LOCALAPPDATA")?.join("Empyrean"),
        Host::MacOs => var("HOME")?.join("Library/Caches/Empyrean"),
        Host::Unix => var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("empyrean"),
    };
    Some(base.join("world-database"))
}

/// The per-user cache folder on this host.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_for(Host::current(), |name| std::env::var_os(name))
}

/// The cached dump of `tag`, when a fetch has put it in the cache.
#[must_use]
pub fn cached_sql(tag: &str) -> Option<PathBuf> {
    cache_dir()
        .map(|d| d.join(sql_name(tag)))
        .filter(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: the release pin and the cache location)
    use super::*;
    use std::collections::HashMap;

    fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn a_tag_is_three_numbers_with_or_without_the_v() {
        assert_eq!(parse_tag("0.9.295").as_deref(), Some("v0.9.295"));
        assert_eq!(parse_tag("v0.9.295").as_deref(), Some("v0.9.295"));
        assert_eq!(parse_tag(" v1.10.0 ").as_deref(), Some("v1.10.0"));
        for bad in [
            "",
            "v",
            "latest",
            "0.9",
            "0.9.295.1",
            "v0.9.x",
            "0..295",
            "vv0.9.295",
            "0.9.295/../x",
            "-1.2.3",
            "0.9.2951234567",
        ] {
            assert_eq!(parse_tag(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_pin_is_a_tag_and_a_sha256() {
        assert_eq!(parse_tag(PINNED_TAG).as_deref(), Some(PINNED_TAG));
        assert_eq!(PINNED_ZIP_SHA256.len(), 64);
        assert!(PINNED_ZIP_SHA256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn the_asset_and_its_dump_are_named_after_the_tag() {
        assert_eq!(zip_name("v0.9.295"), "ACE-World-Database-v0.9.295.sql.zip");
        assert_eq!(sql_name("v0.9.295"), "ACE-World-Database-v0.9.295.sql");
        assert_eq!(
            download_url("v0.9.295"),
            "https://github.com/ACEmulator/ACE-World-16PY-Patches/releases/download/v0.9.295/ACE-World-Database-v0.9.295.sql.zip"
        );
    }

    #[test]
    fn the_cache_is_the_hosts_per_user_cache_folder() {
        let (win, home, xdg) = if cfg!(windows) {
            (r"C:\Users\p\AppData\Local", r"C:\Users\p", r"C:\x")
        } else {
            ("/win/local", "/home/p", "/xdg")
        };
        let win_dir = cache_dir_for(Host::Windows, env(&[("LOCALAPPDATA", win), ("HOME", home)]));
        assert_eq!(
            win_dir,
            Some(PathBuf::from(win).join("Empyrean").join("world-database"))
        );
        assert_eq!(
            cache_dir_for(Host::MacOs, env(&[("HOME", home)])),
            Some(PathBuf::from(home).join("Library/Caches/Empyrean/world-database"))
        );
        assert_eq!(
            cache_dir_for(Host::Unix, env(&[("HOME", home), ("XDG_CACHE_HOME", xdg)])),
            Some(PathBuf::from(xdg).join("empyrean").join("world-database"))
        );
        assert_eq!(
            cache_dir_for(Host::Unix, env(&[("HOME", home)])),
            Some(
                PathBuf::from(home)
                    .join(".cache")
                    .join("empyrean")
                    .join("world-database")
            )
        );
        // A relative or empty XDG_CACHE_HOME is ignored, as the XDG specification says.
        assert_eq!(
            cache_dir_for(
                Host::Unix,
                env(&[("HOME", home), ("XDG_CACHE_HOME", "rel")])
            ),
            cache_dir_for(Host::Unix, env(&[("HOME", home), ("XDG_CACHE_HOME", "")])),
        );
        // Without the variables it needs there is no cache folder.
        assert_eq!(cache_dir_for(Host::Windows, env(&[("HOME", home)])), None);
        assert_eq!(cache_dir_for(Host::Unix, env(&[])), None);
    }
}
