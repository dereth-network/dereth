//! `dereth-network-hub self-update`: the hub installs its own releases, on its operator's command.
//!
//! 1. **Find** ([`Updater::check`]): the GitHub releases of the repository this build names as its
//!    source ([`SOURCE_URL`], or `--source`), tagged `network-hub-v<version>`. The policy picks the
//!    newest it allows ([`select`]): `patch` takes releases of this build's major and minor version,
//!    `minor` those of its major version. A major release, a pre-release, one without an archive for
//!    this build's target or without `SHA256SUMS`, and one that already failed here are reported
//!    and never installed.
//! 2. **Install** ([`Updater::apply`]): download this target's archive and `SHA256SUMS`, and refuse
//!    the archive unless its SHA-256 is the one listed (nothing is signed, so this catches a
//!    damaged or altered download, not a release published by someone else); unpack the binary
//!    beside the installed one; run it, which must name the release's version and say which
//!    registry schema it reads; back the registry up with the consistent copy `backup` makes; then
//!    move the installed binary aside as `<name>.previous-v<old>` and the new one into its place.
//!
//! A new binary that does not run here, names another version, or cannot read this hub's
//! registry is recorded as failed, in a file beside the binary, and not tried again until that
//! file is removed or a newer release installs. Under `patch` a release whose binary reads another
//! registry schema than this build's is refused, so that `patch` only ever changes the binary.
//!
//! The updater replaces the file it runs from, so it needs write access to that file's folder; it
//! never starts or stops a hub, and the running hub never replaces itself. A service manager
//! restarts the hub after an update (`network/hub/DEPLOY.md`).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fetch::{any_url, Fetch, Url};

/// The repository this build's releases come from, when `--source` names no other. A release build
/// sets `DERETH_HUB_BUILD_SOURCE_URL` (at compile time) so that a fork's binaries look for the
/// fork's releases.
pub const SOURCE_URL: &str = match option_env!("DERETH_HUB_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// The prefix of the hub's release tags (`network-hub-v0.1.1`).
pub const TAG_PREFIX: &str = "network-hub-v";

/// The program's name: its release archives' and its binary's.
pub const PROGRAM: &str = "dereth-network-hub";

/// The file beside the binary that records the releases that failed here.
pub const STATE_FILE: &str = ".dereth-network-hub-update.json";

/// The file the updater writes and removes to learn that it may write in the binary's folder.
pub const WRITE_CHECK_FILE: &str = ".dereth-network-hub.write-check";

/// How many of the registry backups the updater makes are kept: the newest.
pub const BACKUPS_KEPT: usize = 3;

/// The exit status of `self-update` when no release is to be installed (`0` when one was, or
/// would be).
pub const NOTHING_TO_INSTALL: u8 = 3;

/// The largest release list read.
const LIST_LIMIT: usize = 32 * 1024 * 1024;

/// The largest download, and the largest binary unpacked from it.
const DOWNLOAD_LIMIT: usize = 256 * 1024 * 1024;

/// How long a new binary may take to answer `--version` or `--registry-schema`.
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------------------------
// Versions

/// A release version: `MAJOR.MINOR.PATCH`, optionally with a pre-release suffix (`-rc.1`), ordered
/// as semantic versioning orders them (a pre-release before its release).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// The pre-release suffix without its `-` (`rc.1`).
    pub pre: Option<String>,
}

impl Version {
    /// Reads `1.2.3` or `1.2.3-rc.1`.
    ///
    /// # Errors
    /// When `text` is not such a version.
    pub fn parse(text: &str) -> Result<Self, String> {
        let t = text.trim();
        let (core, pre) = match t.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (t, None),
        };
        let bad = || format!("`{text}` is not a version such as 0.1.0 or 0.2.0-rc.1");
        let mut parts = core.split('.');
        let mut number = || -> Result<u64, String> {
            let p = parts.next().ok_or_else(bad)?;
            if p.is_empty()
                || !p.bytes().all(|b| b.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
            {
                return Err(bad());
            }
            p.parse().map_err(|_| bad())
        };
        let (major, minor, patch) = (number()?, number()?, number()?);
        if parts.next().is_some() {
            return Err(bad());
        }
        if let Some(p) = pre {
            if p.split('.').any(|id| {
                id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }) {
                return Err(bad());
            }
        }
        Ok(Self {
            major,
            minor,
            patch,
            pre: pre.map(str::to_owned),
        })
    }

    /// The version a hub release tag names (`network-hub-v0.1.1`); `None` for any other tag.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::parse(tag.strip_prefix(TAG_PREFIX)?).ok()
    }

    /// This build's version.
    #[must_use]
    pub fn this_build() -> Self {
        Self::parse(env!("CARGO_PKG_VERSION")).expect("the crate's version is a release version")
    }

    /// Whether it is a pre-release.
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some()
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_pre(a, b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Pre-release precedence: identifier by identifier, numbers numerically and below words, and a
/// shorter list first when all else is equal.
fn compare_pre(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Targets and archives

/// The target a release archive is built for, for a binary of `arch`, `os` and `env` (as
/// `std::env::consts` and `cfg(target_env)` name them); `None` where no archive is built.
#[must_use]
pub fn target_for(arch: &str, os: &str, env: &str) -> Option<&'static str> {
    match (arch, os, env) {
        ("x86_64", "linux", "gnu") => Some("x86_64-unknown-linux-gnu"),
        ("aarch64", "linux", "gnu") => Some("aarch64-unknown-linux-gnu"),
        ("x86_64", "windows", "msvc") => Some("x86_64-pc-windows-msvc"),
        ("aarch64", "macos", _) => Some("aarch64-apple-darwin"),
        ("x86_64", "macos", _) => Some("x86_64-apple-darwin"),
        _ => None,
    }
}

/// The target this binary was built for, as release archives name it.
#[must_use]
pub fn this_target() -> Option<&'static str> {
    let env = if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else {
        ""
    };
    target_for(std::env::consts::ARCH, std::env::consts::OS, env)
}

/// The release archive for `target`: `dereth-network-hub-<version>-<target>.zip` on Windows,
/// `.tar.gz` elsewhere.
#[must_use]
pub fn archive_name(version: &Version, target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("{PROGRAM}-{version}-{target}.{ext}")
}

/// The binary's name in a release for `target`.
#[must_use]
pub fn binary_name(target: &str) -> String {
    if target.contains("windows") {
        format!("{PROGRAM}.exe")
    } else {
        PROGRAM.to_owned()
    }
}

/// The binary in the archive `archive` (named as [`archive_name`] names it, for `target`): the
/// file `<archive without its extension>/<binary>`, which must be a plain file. The archive's
/// other files are not read.
///
/// # Errors
/// When the archive does not read, holds no such file, or holds it as anything but a plain file.
pub fn binary_from_archive(archive: &str, bytes: &[u8], target: &str) -> Result<Vec<u8>, String> {
    let root = archive
        .strip_suffix(".zip")
        .or_else(|| archive.strip_suffix(".tar.gz"))
        .ok_or_else(|| format!("{archive} is neither a .zip nor a .tar.gz"))?;
    let wanted = format!("{root}/{}", binary_name(target));
    let found = if archive.ends_with(".zip") {
        zip_file(bytes, &wanted)
    } else {
        let mut tar = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(DOWNLOAD_LIMIT as u64 + 1)
            .read_to_end(&mut tar)
            .map_err(|e| format!("{archive}: gzip: {e}"))?;
        if tar.len() > DOWNLOAD_LIMIT {
            return Err(format!("{archive} unpacks to more than 256 MiB: refused"));
        }
        tar_file(&tar, &wanted)
    }
    .map_err(|e| format!("{archive}: {e}"))?;
    found.ok_or_else(|| format!("{archive} holds no {wanted}: refused"))
}

/// The file `wanted` in a zip archive.
fn zip_file(bytes: &[u8], wanted: &str) -> Result<Option<Vec<u8>>, String> {
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;
    for i in 0..z.len() {
        let f = z.by_index(i).map_err(|e| format!("zip: {e}"))?;
        if f.name().replace('\\', "/") != wanted {
            continue;
        }
        let link = f.unix_mode().is_some_and(|m| m & 0o170_000 == 0o120_000);
        if f.is_dir() || link {
            return Err(format!("`{wanted}` is not a plain file: refused"));
        }
        let mut data = Vec::new();
        f.take(DOWNLOAD_LIMIT as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| format!("zip: {e}"))?;
        if data.len() > DOWNLOAD_LIMIT {
            return Err(format!("`{wanted}` is larger than 256 MiB: refused"));
        }
        return Ok(Some(data));
    }
    Ok(None)
}

/// A number in a tar header: octal digits, ended by a space or a NUL.
fn tar_number(field: &[u8]) -> Result<usize, String> {
    let text: String = field
        .iter()
        .take_while(|&&c| c != 0 && c != b' ')
        .map(|&c| char::from(c))
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(0);
    }
    usize::from_str_radix(text, 8).map_err(|_| format!("tar: a bad number `{text}`"))
}

/// The file `wanted` in a tar archive (POSIX, GNU or pax headers; a pax header's or GNU long
/// name's own data is skipped, and a long name is the next entry's).
fn tar_file(bytes: &[u8], wanted: &str) -> Result<Option<Vec<u8>>, String> {
    let mut at = 0usize;
    let mut long_name: Option<String> = None;
    while at + 512 <= bytes.len() {
        let h = &bytes[at..at + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let text = |r: std::ops::Range<usize>| {
            let s = &h[r];
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            String::from_utf8_lossy(&s[..end]).into_owned()
        };
        let size = tar_number(&h[124..136])?;
        let start = at + 512;
        let data = bytes
            .get(start..start.saturating_add(size))
            .ok_or("tar: an entry runs past the end")?;
        at = start + size.div_ceil(512) * 512;
        let kind = h[156];
        if kind == b'L' {
            long_name = Some(
                String::from_utf8_lossy(data)
                    .trim_end_matches('\0')
                    .to_owned(),
            );
            continue;
        }
        let name = match long_name.take() {
            Some(n) => n,
            // Only a POSIX header has a prefix field; a GNU one keeps other things there.
            None if &h[257..263] == b"ustar\0" && h[345] != 0 => {
                format!("{}/{}", text(345..500), text(0..100))
            }
            None => text(0..100),
        };
        if matches!(kind, b'x' | b'g' | b'K') || name.trim_end_matches('/') != wanted {
            continue;
        }
        if kind != b'0' && kind != 0 {
            return Err(format!("`{wanted}` is not a plain file: refused"));
        }
        return Ok(Some(data.to_vec()));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// Checksums

/// A file's SHA-256, lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The lines of a `SHA256SUMS` file: (SHA-256, file name).
#[must_use]
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (sum, name) = l.trim().split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| (sum.to_ascii_lowercase(), name.to_owned()))
        })
        .collect()
}

/// Checks `bytes`, downloaded as `name`, against `sums`. Its SHA-256.
///
/// # Errors
/// When `name` is not listed or its SHA-256 is another.
pub fn check_sum(name: &str, bytes: &[u8], sums: &[(String, String)]) -> Result<String, String> {
    let actual = sha256_hex(bytes);
    match sums.iter().find(|(_, n)| n == name) {
        None => Err(format!("{name} is not listed in SHA256SUMS: refused")),
        Some((expected, _)) if *expected != actual => Err(format!(
            "{name}: its SHA-256 {actual} is not the {expected} SHA256SUMS lists: refused (a damaged or altered download; checked by SHA-256 only)"
        )),
        Some(_) => Ok(actual),
    }
}

// ---------------------------------------------------------------------------------------------
// Where releases come from

/// A repository's releases, and how to reach them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The repository's address in the releases API (`https://api.github.com/repos/<owner>/<name>`).
    pub api: String,
    /// `<owner>/<name>`.
    pub repo: String,
}

impl Source {
    /// The source `url` names: a GitHub repository (`https://github.com/<owner>/<name>`), or the
    /// address of a mirror serving GitHub's releases API for one
    /// (`http(s)://<host>/.../repos/<owner>/<name>`).
    ///
    /// # Errors
    /// When `url` is neither.
    pub fn parse(url: &str) -> Result<Self, String> {
        let u = url.trim().trim_end_matches('/');
        if let Some(rest) = u.strip_prefix("https://github.com/") {
            let mut parts = rest.trim_end_matches(".git").split('/');
            return match (parts.next(), parts.next(), parts.next()) {
                (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => {
                    Ok(Self {
                        api: format!("https://api.github.com/repos/{owner}/{name}"),
                        repo: format!("{owner}/{name}"),
                    })
                }
                _ => Err(format!(
                    "{url} is not a GitHub repository (https://github.com/<owner>/<name>)"
                )),
            };
        }
        let mirror = u.starts_with("https://") || u.starts_with("http://");
        let mut tail = u.rsplit('/');
        match (tail.next(), tail.next(), tail.next()) {
            (Some(name), Some(owner), Some("repos"))
                if mirror && !name.is_empty() && !owner.is_empty() =>
            {
                Ok(Self {
                    api: u.to_owned(),
                    repo: format!("{owner}/{name}"),
                })
            }
            _ => Err(format!(
                "{url} is neither a GitHub repository (https://github.com/<owner>/<name>) nor a releases API address (https://<host>/repos/<owner>/<name>)"
            )),
        }
    }
}

/// A hub release as the releases API lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub version: Version,
    /// Published as a pre-release.
    pub prerelease: bool,
    /// Each file's name and download address.
    pub assets: Vec<(String, String)>,
}

impl Published {
    /// The download address of the file named `name`.
    #[must_use]
    pub fn asset(&self, name: &str) -> Option<&str> {
        self.assets
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, u)| u.as_str())
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// Reads a releases API answer: the published hub releases (drafts and every other tag left out).
///
/// # Errors
/// When `json` is not such an answer.
pub fn parse_releases(json: &[u8]) -> Result<Vec<Published>, String> {
    let list: Vec<ApiRelease> =
        serde_json::from_slice(json).map_err(|e| format!("the release list does not read: {e}"))?;
    Ok(list
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            Some(Published {
                version: Version::from_tag(&r.tag_name)?,
                prerelease: r.prerelease,
                assets: r
                    .assets
                    .into_iter()
                    .map(|a| (a.name, a.browser_download_url))
                    .collect(),
            })
        })
        .collect())
}

/// What is being fetched, which sets what is asked for and how much is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetching {
    /// The releases API's list.
    List,
    /// A release's file.
    Download,
}

/// Reads a URL: the body of a `200` answer.
pub trait Fetcher: std::fmt::Debug {
    /// The body `url` answers with.
    ///
    /// # Errors
    /// When it does not answer `200`, or answers more than the limit for `what`.
    fn get(&self, url: &str, what: Fetching) -> Result<Vec<u8>, String>;

    /// A page of the releases API's list: its body, and the next page's address when the answer's
    /// `Link` header names one ([`next_page`]). A fetcher that reads no headers names none.
    ///
    /// # Errors
    /// As [`Fetcher::get`].
    fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        Ok((self.get(url, Fetching::List)?, None))
    }
}

/// The most pages of the releases API's list the updater reads: a thousand releases.
pub const MAX_PAGES: usize = 10;

/// The next page's address in a releases API answer's `Link` header
/// (`<https://...&page=2>; rel="next", <https://...&page=4>; rel="last"`), when it names one.
#[must_use]
pub fn next_page(link: &str) -> Option<String> {
    link.split(',').find_map(|entry| {
        let (url, params) = entry.trim().split_once(';')?;
        let url = url.trim().strip_prefix('<')?.strip_suffix('>')?;
        let next = params.split(';').any(|p| {
            p.trim().strip_prefix("rel=").is_some_and(|rel| {
                rel.trim_matches('"')
                    .split_whitespace()
                    .any(|r| r.eq_ignore_ascii_case("next"))
            })
        });
        next.then(|| url.to_owned())
    })
}

/// `scheme://host` of `url`.
fn origin(url: &str) -> &str {
    let after = url.find("://").map_or(0, |i| i + 3);
    let end = url[after..].find('/').map_or(url.len(), |i| after + i);
    &url[..end]
}

/// The hub releases `source` lists, read a page at a time until a page holds one at or below
/// `current`: the API lists the newest first, so the pages after it hold none newer. At most
/// [`MAX_PAGES`] pages; a next page on another host is not followed.
///
/// # Errors
/// When a page cannot be fetched or read.
pub fn list_releases(
    fetcher: &dyn Fetcher,
    source: &Source,
    current: &Version,
) -> Result<Vec<Published>, String> {
    let first = format!("{}/releases?per_page=100", source.api);
    let mut url = first.clone();
    let mut all = Vec::new();
    for _ in 0..MAX_PAGES {
        let (body, next) = fetcher.list_page(&url)?;
        let page = parse_releases(&body)?;
        let reached = page.iter().any(|p| p.version <= *current);
        all.extend(page);
        match next {
            Some(next) if !reached && origin(&next) == origin(&first) => url = next,
            _ => break,
        }
    }
    Ok(all)
}

/// [`Fetcher`] over the hub's own HTTP client. Any address will do: the operator names the
/// source on the command line, and what is downloaded is checked against the release's checksums.
#[derive(Debug, Clone, Copy, Default)]
pub struct Https;

impl Https {
    /// The `200` answer `url` gives, within the limit for `what`.
    fn answer(url: &str, what: Fetching) -> Result<crate::fetch::Response, String> {
        let (accept, limit, timeout) = match what {
            Fetching::List => (
                "application/vnd.github+json",
                LIST_LIMIT,
                Duration::from_secs(60),
            ),
            Fetching::Download => (
                "application/octet-stream",
                DOWNLOAD_LIMIT,
                Duration::from_secs(600),
            ),
        };
        let fetch = Fetch {
            method: "GET",
            url: Url::parse(url).map_err(|e| format!("{url}: {e}"))?,
            headers: vec![("Accept".into(), accept.into())],
            body: Vec::new(),
            timeout,
            // One byte more than is allowed, so that a longer answer is seen to be longer.
            max_body: limit + 1,
            // A release's file is served from another host, through a redirect.
            max_redirects: 5,
            rule: crate::discord::any_address,
            deny: Vec::new(),
            check: any_url,
            check_with: Vec::new(),
        };
        let r = fetch.send().map_err(|e| format!("{url}: {e}"))?;
        match r.status {
            200 => {}
            403 | 429 => return Err(format!(
                "{url} refused the request (HTTP {}), usually GitHub's hourly limit on unauthenticated requests: try again later",
                r.status
            )),
            s => return Err(format!("{url} answered HTTP {s}")),
        }
        if r.body.len() > limit {
            return Err(format!(
                "{url}: the answer is larger than {} MiB: refused",
                limit / (1024 * 1024)
            ));
        }
        Ok(r)
    }
}

impl Fetcher for Https {
    fn get(&self, url: &str, what: Fetching) -> Result<Vec<u8>, String> {
        Ok(Self::answer(url, what)?.body)
    }

    fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
        let r = Self::answer(url, Fetching::List)?;
        let next = r
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("link"))
            .and_then(|(_, link)| next_page(link));
        Ok((r.body, next))
    }
}

// ---------------------------------------------------------------------------------------------
// Running a new binary

/// Runs a binary once with one argument.
pub trait Runner: std::fmt::Debug {
    /// What `exe arg` prints, trimmed.
    ///
    /// # Errors
    /// When it does not run, fails, or takes too long.
    fn output(&self, exe: &Path, arg: &str) -> Result<String, String>;
}

/// [`Runner`] by starting the program, with its input closed, for at most [`RUN_TIMEOUT`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Processes;

impl Runner for Processes {
    fn output(&self, exe: &Path, arg: &str) -> Result<String, String> {
        let mut child = Command::new(exe)
            .arg(arg)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("{} does not run here: {e}", exe.display()))?;
        let out = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(out) = out {
                let _ = out.take(64 * 1024).read_to_end(&mut text);
            }
            text
        });
        let end = Instant::now() + RUN_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= end => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} {arg} did not finish within {} s",
                        exe.display(),
                        RUN_TIMEOUT.as_secs()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(format!("waiting for {}: {e}", exe.display())),
            }
        };
        let text = reader.join().unwrap_or_default();
        if !status.success() {
            return Err(format!("{} {arg} failed ({status})", exe.display()));
        }
        Ok(String::from_utf8_lossy(&text).trim().to_owned())
    }
}

// ---------------------------------------------------------------------------------------------
// Choosing a release

/// How far `self-update` may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Releases of this build's major and minor version, whose binary reads the same registry
    /// schema: only the binary changes.
    Patch,
    /// Releases of this build's major version; a newer hub moves the registry's schema forward
    /// when it first opens it.
    Minor,
}

impl Policy {
    /// Reads `patch` or `minor`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "patch" => Some(Self::Patch),
            "minor" => Some(Self::Minor),
            _ => None,
        }
    }

    /// Its name on the command line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Minor => "minor",
        }
    }
}

/// A release that failed here, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub version: String,
    pub reason: String,
}

/// The updater's record beside the binary ([`STATE_FILE`]): the releases that failed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub failed: Vec<Failure>,
}

impl State {
    /// The record at `path` (empty when there is none, or it does not read).
    #[must_use]
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|t| serde_json::from_slice(&t).ok())
            .unwrap_or_default()
    }

    /// Writes the record to `path`.
    ///
    /// # Errors
    /// When it cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n";
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Records `version` as failed for `reason`, in place of any earlier record of it.
    pub fn record(&mut self, version: &Version, reason: &str) {
        let v = version.to_string();
        self.failed.retain(|f| f.version != v);
        self.failed.push(Failure {
            version: v,
            reason: reason.to_owned(),
        });
    }
}

/// The policy's decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The release to install.
    pub take: Option<Version>,
    /// Every other newer release, oldest first, with why it is not installed.
    pub reported: Vec<(Version, String)>,
}

/// Chooses among `found` (any order; those not newer than `current` are ignored) the newest
/// release `policy` takes on a hub running `current` built for `target` (`None` when no archive
/// is built for it), where the releases `failed` failed before.
#[must_use]
pub fn select(
    current: &Version,
    target: Option<&str>,
    policy: Policy,
    failed: &[Failure],
    found: &[Published],
) -> Selection {
    let mut newer: Vec<&Published> = found.iter().filter(|p| p.version > *current).collect();
    newer.sort_by(|a, b| a.version.cmp(&b.version));
    newer.dedup_by(|a, b| a.version == b.version);
    let mut eligible = Vec::new();
    let mut reported = Vec::new();
    for p in newer {
        match verdict(current, target, policy, failed, p) {
            Ok(()) => eligible.push(p.version.clone()),
            Err(why) => reported.push((p.version.clone(), why)),
        }
    }
    let take = eligible.pop();
    if let Some(newest) = &take {
        for v in eligible {
            reported.push((v, format!("{newest} is newer and is taken instead")));
        }
    }
    reported.sort_by(|a, b| a.0.cmp(&b.0));
    Selection { take, reported }
}

fn verdict(
    current: &Version,
    target: Option<&str>,
    policy: Policy,
    failed: &[Failure],
    p: &Published,
) -> Result<(), String> {
    let v = &p.version;
    if p.prerelease || v.is_prerelease() {
        return Err("a pre-release: self-update never installs one".to_owned());
    }
    if v.major != current.major {
        return Err(
            "a major release: self-update never installs one; install it by hand".to_owned(),
        );
    }
    if policy == Policy::Patch && v.minor != current.minor {
        return Err(format!(
            "a minor release: --policy patch takes only {}.{}.x",
            current.major, current.minor
        ));
    }
    if let Some(f) = failed.iter().find(|f| f.version == v.to_string()) {
        return Err(format!("it failed here before: {}", f.reason));
    }
    if let Some(t) = target {
        let archive = archive_name(v, t);
        if p.asset(&archive).is_none() {
            return Err(format!("it publishes no {archive}"));
        }
    }
    if p.asset("SHA256SUMS").is_none() {
        return Err("it publishes no SHA256SUMS".to_owned());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The updater

/// What [`Updater::check`] found and decided.
#[derive(Debug, Clone)]
pub struct Check {
    pub policy: Policy,
    /// The newer releases.
    pub found: Vec<Published>,
    pub selection: Selection,
    /// Why `--apply` would refuse before downloading anything, if it would.
    pub blocked: Option<String>,
    /// The schema of the registry, when it reads.
    pub registry_schema: Option<i64>,
}

/// What [`Updater::apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub version: Version,
    /// The archive and its SHA-256.
    pub archive: String,
    pub sha256: String,
    /// The registry's copy, made before the binary was replaced.
    pub backup: PathBuf,
    /// The binary that was replaced, moved aside.
    pub previous: PathBuf,
}

/// The updater of one installed binary.
#[derive(Debug)]
pub struct Updater {
    pub source: Source,
    pub fetcher: Box<dyn Fetcher>,
    pub runner: Box<dyn Runner>,
    /// The installed binary's version.
    pub current: Version,
    /// The target it was built for (`None` where no archive is built).
    pub target: Option<String>,
    /// The registry schema it reads.
    pub schema: i64,
    /// The binary: the file replaced.
    pub binary: PathBuf,
    /// The registry, backed up before the binary is replaced.
    pub registry: PathBuf,
}

impl Updater {
    /// The updater of the binary this process runs from, for the registry at `registry`.
    ///
    /// # Errors
    /// When the binary's own file cannot be found.
    pub fn for_this_binary(source: Source, registry: &Path) -> Result<Self, String> {
        let binary = std::env::current_exe()
            .map_err(|e| format!("this program's own file cannot be found: {e}"))?;
        Ok(Self {
            source,
            fetcher: Box::new(Https),
            runner: Box::new(Processes),
            current: Version::this_build(),
            target: this_target().map(str::to_owned),
            schema: crate::registry::SCHEMA_VERSION,
            binary,
            registry: registry.to_path_buf(),
        })
    }

    /// The binary's folder.
    #[must_use]
    pub fn folder(&self) -> PathBuf {
        match self.binary.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        }
    }

    /// The updater's record ([`STATE_FILE`] in the binary's folder).
    #[must_use]
    pub fn state_file(&self) -> PathBuf {
        self.folder().join(STATE_FILE)
    }

    /// Why [`Updater::apply`] would refuse before downloading anything: no archive for this
    /// target, a binary's folder this account cannot write, or no registry to back up.
    #[must_use]
    pub fn blocked(&self) -> Option<String> {
        if self.target.is_none() {
            return Some(format!(
                "no release archive is built for {}-{}",
                std::env::consts::ARCH,
                std::env::consts::OS
            ));
        }
        if let Err(e) = writable(&self.folder()) {
            return Some(e);
        }
        if !self.registry.is_file() {
            return Some(format!(
                "there is no registry at {} to back up first: name it with --db or DERETH_HUB_DB",
                self.registry.display()
            ));
        }
        None
    }

    /// Lists the newer releases and decides which, if any, `policy` takes.
    ///
    /// # Errors
    /// When the releases cannot be listed.
    pub fn check(&self, policy: Policy) -> Result<Check, String> {
        let all = list_releases(self.fetcher.as_ref(), &self.source, &self.current)?;
        let found: Vec<Published> = all
            .into_iter()
            .filter(|p| p.version > self.current)
            .collect();
        let failed = State::load(&self.state_file()).failed;
        let selection = select(
            &self.current,
            self.target.as_deref(),
            policy,
            &failed,
            &found,
        );
        Ok(Check {
            policy,
            found,
            selection,
            blocked: self.blocked(),
            registry_schema: crate::registry::schema_of_file(&self.registry).ok(),
        })
    }

    /// Installs the release `check` takes, as the module documentation describes. `None` when it
    /// takes none. `say` is told each step as it starts.
    ///
    /// # Errors
    /// Why nothing was installed (or, when moving the new binary in failed, what was put back).
    pub fn apply(
        &self,
        check: &Check,
        say: &mut dyn FnMut(&str),
    ) -> Result<Option<Installed>, String> {
        let Some(version) = &check.selection.take else {
            return Ok(None);
        };
        if let Some(why) = self.blocked() {
            return Err(format!("{why}: nothing was installed"));
        }
        let target = self.target.as_deref().unwrap_or_default();
        let release = check
            .found
            .iter()
            .find(|p| &p.version == version)
            .ok_or("the release taken is not among those found")?;
        let archive = archive_name(version, target);
        let (Some(sums_url), Some(archive_url)) =
            (release.asset("SHA256SUMS"), release.asset(&archive))
        else {
            return Err(format!(
                "{version} publishes no {archive} or no SHA256SUMS: refused"
            ));
        };

        say(&format!("Downloading {archive_url}"));
        let sums = parse_sums(&String::from_utf8_lossy(
            &self.fetcher.get(sums_url, Fetching::Download)?,
        ));
        let bytes = self.fetcher.get(archive_url, Fetching::Download)?;
        let sha256 = check_sum(&archive, &bytes, &sums)?;
        say(&format!(
            "{archive}: its SHA-256 {sha256} is the one SHA256SUMS lists (checked by SHA-256 only: releases are not signed)"
        ));
        let binary = binary_from_archive(&archive, &bytes, target)?;

        // The new binary goes beside the installed one, so that it is moved into place, not
        // copied, and runs here from the folder it will run from. One an interrupted run left
        // there goes first.
        let staged_prefix = format!(".{PROGRAM}.new-v");
        if let Ok(entries) = fs::read_dir(self.folder()) {
            for e in entries.filter_map(Result::ok) {
                if e.file_name().to_string_lossy().starts_with(&staged_prefix) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        let staged = self.folder().join(format!(
            "{staged_prefix}{version}{}",
            std::env::consts::EXE_SUFFIX
        ));
        fs::write(&staged, &binary).map_err(|e| format!("{}: {e}", staged.display()))?;
        let refuse = |why: String, record: bool| {
            let _ = fs::remove_file(&staged);
            if record {
                let mut state = State::load(&self.state_file());
                state.record(version, &why);
                let _ = state.save(&self.state_file());
            }
            format!("{why}: nothing was installed")
        };
        if let Err(e) = make_executable(&staged) {
            return Err(refuse(e, false));
        }

        // It runs here, is the release, and reads this hub's registry.
        let named = match self.runner.output(&staged, "--version") {
            Ok(text) => text.lines().next().unwrap_or_default().trim().to_owned(),
            Err(e) => return Err(refuse(format!("{version}'s binary: {e}"), true)),
        };
        if named != format!("{PROGRAM} {version}") {
            return Err(refuse(
                format!("{version}'s binary names itself `{named}`, not `{PROGRAM} {version}`"),
                true,
            ));
        }
        let schema = match self
            .runner
            .output(&staged, "--registry-schema")
            .and_then(|t| {
                t.parse::<i64>()
                    .map_err(|_| format!("`{t}` is not a number"))
            }) {
            Ok(s) => s,
            Err(e) => {
                return Err(refuse(
                    format!("{version}'s binary does not say which registry schema it reads: {e}"),
                    true,
                ))
            }
        };
        let registry_schema = crate::registry::schema_of_file(&self.registry)
            .map_err(|e| refuse(format!("{}: {}", self.registry.display(), e.message), false))?;
        if registry_schema > schema {
            return Err(refuse(
                format!(
                    "{version} reads registry schema {schema}, and the registry is at {registry_schema}: it would refuse to start"
                ),
                true,
            ));
        }
        if check.policy == Policy::Patch && schema != self.schema {
            return Err(refuse(
                format!(
                    "{version} reads registry schema {schema}, not this build's {}: --policy patch installs only a release that changes nothing but the binary; install it with --policy minor",
                    self.schema
                ),
                false,
            ));
        }

        // The registry's copy, then the swap.
        let backup = match self.back_up_registry(version, &mut *say) {
            Ok(b) => b,
            Err(e) => return Err(refuse(e, false)),
        };
        let previous = match self.swap(&staged) {
            Ok(p) => p,
            Err(e) => return Err(refuse(e, false)),
        };
        let mut state = State::load(&self.state_file());
        if !state.failed.is_empty() {
            state.failed.clear();
            let _ = state.save(&self.state_file());
        }
        Ok(Some(Installed {
            version: version.clone(),
            archive,
            sha256,
            backup,
            previous,
        }))
    }

    /// Writes the registry's copy beside it, as `<registry>.backup-update-v<old>-v<new>-<UTC
    /// time>`, private to the registry's owner, and removes all but the newest [`BACKUPS_KEPT`] of
    /// those copies.
    fn back_up_registry(
        &self,
        version: &Version,
        say: &mut dyn FnMut(&str),
    ) -> Result<PathBuf, String> {
        let dir = match self.registry.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        let name = self
            .registry
            .file_name()
            .ok_or("the registry has no file name")?
            .to_string_lossy()
            .into_owned();
        let stamp: String = crate::registry::iso(SystemTime::now())
            .chars()
            .take(19)
            .filter(|c| c.is_ascii_digit() || *c == 'T')
            .collect();
        let prefix = format!("{name}.backup-update-");
        let backup = dir.join(format!("{prefix}v{}-v{version}-{stamp}Z", self.current));
        say(&format!(
            "Backing up {} to {}",
            self.registry.display(),
            backup.display()
        ));
        crate::registry::backup_file(&self.registry, &backup).map_err(|e| {
            let _ = fs::remove_file(&backup);
            format!(
                "backing up {} failed ({})",
                self.registry.display(),
                e.message
            )
        })?;
        same_owner_private(&self.registry, &backup);
        let mut older: Vec<(String, PathBuf)> = fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| {
                        let n = e.file_name().to_string_lossy().into_owned();
                        let stamp = n.strip_prefix(&prefix)?.rsplit('-').next()?.to_owned();
                        Some((stamp, e.path()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        older.sort();
        let excess = older.len().saturating_sub(BACKUPS_KEPT);
        for (_, path) in older.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
        Ok(backup)
    }

    /// Moves the installed binary aside as `<name>.previous-v<old>` (removing the ones earlier
    /// updates left, where they can be) and `staged` into its place; puts the old one back when the
    /// new one cannot be moved in. Where the old binary now is.
    fn swap(&self, staged: &Path) -> Result<PathBuf, String> {
        let dir = self.folder();
        let name = self
            .binary
            .file_name()
            .ok_or("the binary has no file name")?
            .to_string_lossy()
            .into_owned();
        let aside_prefix = format!("{name}.previous-v");
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.filter_map(Result::ok) {
                if e.file_name().to_string_lossy().starts_with(&aside_prefix) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        // A name an earlier binary still holds (one still running, on Windows) gets a number.
        let base = format!("{aside_prefix}{}", self.current);
        let mut previous = dir.join(&base);
        let mut n = 2;
        while previous.exists() {
            previous = dir.join(format!("{base}.{n}"));
            n += 1;
        }
        fs::rename(&self.binary, &previous).map_err(|e| {
            format!(
                "{} cannot be moved aside ({e}): is {} writable by this account?",
                self.binary.display(),
                dir.display()
            )
        })?;
        if let Err(e) = fs::rename(staged, &self.binary) {
            let back = fs::rename(&previous, &self.binary);
            return Err(match back {
                Ok(()) => format!(
                    "the new binary could not be moved to {} ({e}); the old one is back",
                    self.binary.display()
                ),
                Err(b) => format!(
                    "the new binary could not be moved to {} ({e}), and the old one could not be moved back from {} ({b}): move it back by hand",
                    self.binary.display(),
                    previous.display()
                ),
            });
        }
        Ok(previous)
    }
}

/// Whether this account can make a file in `dir`, by making and removing [`WRITE_CHECK_FILE`].
///
/// # Errors
/// The refusal, naming the folder.
pub fn writable(dir: &Path) -> Result<(), String> {
    let probe = dir.join(WRITE_CHECK_FILE);
    match fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&probe)
    {
        Ok(f) => {
            drop(f);
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(format!(
            "this account cannot write in {}, the folder the binary is in ({e}): run self-update as the account that installed the binary (root, for one in /usr/local/bin)",
            dir.display()
        )),
    }
}

/// Marks `path` executable (Unix; nothing to do elsewhere).
fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Gives `copy` the owner of `original` and makes it readable by that owner alone (Unix; where
/// this account may not, the copy stays this account's).
fn same_owner_private(original: &Path, copy: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let _ = fs::set_permissions(copy, fs::Permissions::from_mode(0o600));
        if let Ok(m) = fs::metadata(original) {
            let _ = std::os::unix::fs::chown(copy, Some(m.uid()), Some(m.gid()));
        }
    }
    #[cfg(not(unix))]
    let _ = (original, copy);
}

/// The lines `self-update` prints for `check`.
#[must_use]
pub fn describe(updater: &Updater, check: &Check) -> Vec<String> {
    let target = updater
        .target
        .as_deref()
        .unwrap_or("a target no release is built for");
    let registry = match check.registry_schema {
        Some(s) => format!("{} (schema {s})", updater.registry.display()),
        None => format!("{} (not found)", updater.registry.display()),
    };
    let mut lines = vec![
        format!("This hub: {PROGRAM} {} for {target}", updater.current),
        format!(
            "It reads registry schema {}; the registry: {registry}",
            updater.schema
        ),
        format!(
            "Releases of {}; --policy {}",
            updater.source.repo,
            check.policy.name()
        ),
    ];
    if check.found.is_empty() {
        lines.push("No newer release is published.".to_owned());
    }
    let mut failed_any = false;
    for (v, why) in &check.selection.reported {
        failed_any |= why.starts_with("it failed here before");
        lines.push(format!("{v}: not installed: {why}"));
    }
    if failed_any {
        lines.push(format!(
            "A release that failed here is tried again once {} is removed.",
            updater.state_file().display()
        ));
    }
    match &check.selection.take {
        Some(v) => lines.push(format!("{v}: would be installed")),
        None => lines.push("Nothing would be installed.".to_owned()),
    }
    if let Some(why) = &check.blocked {
        lines.push(format!("--apply would refuse: {why}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).expect("a version")
    }

    fn release(version: &str, prerelease: bool) -> Published {
        let ver = v(version);
        let mut assets = vec![("SHA256SUMS".to_owned(), format!("https://x/{version}/sums"))];
        for t in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
            let a = archive_name(&ver, t);
            assets.push((a.clone(), format!("https://x/{version}/{a}")));
        }
        Published {
            version: ver,
            prerelease,
            assets,
        }
    }

    const LINUX: Option<&str> = Some("x86_64-unknown-linux-gnu");

    /// Spec: hub: `self-update` takes the newest release its policy allows: `patch` this build's
    /// major and minor version, `minor` its major version; a major release, a pre-release, one that
    /// failed here before, and one without this target's archive or `SHA256SUMS` are reported and
    /// never installed.
    #[test]
    fn the_policy_takes_the_newest_release_it_allows_and_reports_the_rest() {
        let current = v("0.1.1");
        let found = vec![
            release("0.1.0", false),
            release("0.1.2", false),
            release("0.1.3", false),
            release("0.1.4-rc.1", false),
            release("0.1.5", true),
            release("0.2.0", false),
            release("1.0.0", false),
        ];
        let patch = select(&current, LINUX, Policy::Patch, &[], &found);
        assert_eq!(patch.take, Some(v("0.1.3")));
        let why = |s: &Selection, version: &str| {
            s.reported
                .iter()
                .find(|(x, _)| *x == v(version))
                .map(|(_, w)| w.clone())
                .unwrap_or_else(|| panic!("{version} is reported"))
        };
        assert!(why(&patch, "0.1.2").contains("0.1.3 is newer"));
        assert!(why(&patch, "0.1.4-rc.1").contains("pre-release"));
        assert!(
            why(&patch, "0.1.5").contains("pre-release"),
            "published as one"
        );
        assert!(why(&patch, "0.2.0").contains("--policy patch takes only 0.1.x"));
        assert!(why(&patch, "1.0.0").contains("a major release"));
        assert!(
            !patch.reported.iter().any(|(x, _)| *x == v("0.1.0")),
            "an older release is not a candidate"
        );

        let minor = select(&current, LINUX, Policy::Minor, &[], &found);
        assert_eq!(minor.take, Some(v("0.2.0")));
        assert!(why(&minor, "1.0.0").contains("a major release"));

        let failed = [Failure {
            version: "0.2.0".into(),
            reason: "it does not run here".into(),
        }];
        let minor = select(&current, LINUX, Policy::Minor, &failed, &found);
        assert_eq!(minor.take, Some(v("0.1.3")), "the one before it instead");
        assert!(why(&minor, "0.2.0").contains("failed here before: it does not run here"));

        let mut no_archive = release("0.1.6", false);
        no_archive
            .assets
            .retain(|(n, _)| !n.contains("linux") || n == "SHA256SUMS");
        let mut no_sums = release("0.1.7", false);
        no_sums.assets.retain(|(n, _)| n != "SHA256SUMS");
        let s = select(
            &current,
            LINUX,
            Policy::Patch,
            &[],
            &[release("0.1.2", false), no_archive, no_sums],
        );
        assert_eq!(s.take, Some(v("0.1.2")));
        assert!(why(&s, "0.1.6")
            .contains("publishes no dereth-network-hub-0.1.6-x86_64-unknown-linux-gnu.tar.gz"));
        assert!(why(&s, "0.1.7").contains("no SHA256SUMS"));

        assert_eq!(
            select(&v("1.0.0"), LINUX, Policy::Minor, &[], &found).take,
            None,
            "nothing newer"
        );
    }

    /// Spec: hub: each target's release archive is `dereth-network-hub-<version>-<target>`, a
    /// `.zip` on Windows and a `.tar.gz` elsewhere, holding the binary in a folder of that name;
    /// a binary built for a target no archive is built for finds none.
    #[test]
    fn each_target_has_its_archive_name() {
        let ver = v("0.1.2");
        for ((arch, os, env), archive, binary) in [
            (
                ("x86_64", "linux", "gnu"),
                "dereth-network-hub-0.1.2-x86_64-unknown-linux-gnu.tar.gz",
                "dereth-network-hub",
            ),
            (
                ("aarch64", "linux", "gnu"),
                "dereth-network-hub-0.1.2-aarch64-unknown-linux-gnu.tar.gz",
                "dereth-network-hub",
            ),
            (
                ("x86_64", "windows", "msvc"),
                "dereth-network-hub-0.1.2-x86_64-pc-windows-msvc.zip",
                "dereth-network-hub.exe",
            ),
            (
                ("aarch64", "macos", ""),
                "dereth-network-hub-0.1.2-aarch64-apple-darwin.tar.gz",
                "dereth-network-hub",
            ),
            (
                ("x86_64", "macos", ""),
                "dereth-network-hub-0.1.2-x86_64-apple-darwin.tar.gz",
                "dereth-network-hub",
            ),
        ] {
            let target = target_for(arch, os, env).expect("a target");
            assert_eq!(archive_name(&ver, target), archive, "{target}");
            assert_eq!(binary_name(target), binary, "{target}");
        }
        for (arch, os, env) in [
            ("x86_64", "linux", "musl"),
            ("x86_64", "windows", "gnu"),
            ("x86", "windows", "msvc"),
            ("riscv64", "linux", "gnu"),
            ("x86_64", "freebsd", ""),
        ] {
            assert_eq!(target_for(arch, os, env), None, "{arch}-{os}-{env}");
        }
        assert!(this_target().is_some(), "this build's target has archives");
    }

    /// A gzipped tar of `entries`: (path, type flag, contents).
    fn tar_gz(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut tar = Vec::new();
        for (path, kind, data) in entries {
            let mut h = [0u8; 512];
            h[..path.len()].copy_from_slice(path.as_bytes());
            h[100..107].copy_from_slice(b"0000755");
            h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
            h[156] = *kind;
            h[257..263].copy_from_slice(b"ustar\0");
            tar.extend_from_slice(&h);
            tar.extend_from_slice(data);
            tar.resize(tar.len().div_ceil(512) * 512, 0);
        }
        tar.resize(tar.len() + 1024, 0);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).expect("written");
        gz.finish().expect("finished")
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, data) in entries {
            if let Some(dir) = path.strip_suffix('/') {
                z.add_directory(dir, zip::write::SimpleFileOptions::default())
                    .expect("a folder");
            } else {
                z.start_file(*path, zip::write::SimpleFileOptions::default())
                    .expect("a file");
                z.write_all(data).expect("written");
            }
        }
        z.finish().expect("finished").into_inner()
    }

    /// Spec: hub: the binary is read from its folder in the archive, a `.tar.gz` (with a pax header
    /// or not) or a `.zip`, and an archive without it, or with it as a link, is refused.
    #[test]
    fn the_binary_is_read_from_either_kind_of_archive() {
        let linux = "x86_64-unknown-linux-gnu";
        let name = archive_name(&v("0.1.2"), linux);
        let root = "dereth-network-hub-0.1.2-x86_64-unknown-linux-gnu";
        let bin = format!("{root}/dereth-network-hub");
        let good = tar_gz(&[
            (&format!("{root}/"), b'5', b""),
            (&format!("{root}/LICENSE"), b'0', b"licence"),
            ("PaxHeader", b'x', b"30 mtime=1700000000.000000000\n"),
            (&bin, b'0', b"the binary"),
        ]);
        assert_eq!(
            binary_from_archive(&name, &good, linux).expect("read"),
            b"the binary"
        );
        let link = tar_gz(&[(&bin, b'2', b"")]);
        assert!(binary_from_archive(&name, &link, linux)
            .expect_err("a link")
            .contains("not a plain file"));
        let elsewhere = tar_gz(&[("dereth-network-hub", b'0', b"loose")]);
        assert!(binary_from_archive(&name, &elsewhere, linux)
            .expect_err("not in its folder")
            .contains("holds no"));
        assert!(binary_from_archive(&name, b"not gzip", linux).is_err());

        let windows = "x86_64-pc-windows-msvc";
        let name = archive_name(&v("0.1.2"), windows);
        let root = "dereth-network-hub-0.1.2-x86_64-pc-windows-msvc";
        let good = zip_of(&[
            (&format!("{root}/"), b""),
            (&format!("{root}/README.md"), b"guide"),
            (&format!("{root}/dereth-network-hub.exe"), b"the exe"),
        ]);
        assert_eq!(
            binary_from_archive(&name, &good, windows).expect("read"),
            b"the exe"
        );
        let missing = zip_of(&[(&format!("{root}/README.md"), b"guide")]);
        assert!(binary_from_archive(&name, &missing, windows)
            .expect_err("no binary")
            .contains("holds no"));
    }

    /// Spec: hub: a download is checked against `SHA256SUMS`, and one whose SHA-256 is not the one
    /// listed, or that is not listed, is refused, saying the check is by SHA-256 only.
    #[test]
    fn a_download_must_match_its_listed_sha256() {
        let sum = sha256_hex(b"archive");
        let sums = parse_sums(&format!(
            "{sum}  a.tar.gz\n{}  *b.zip\nnot a line\n",
            sha256_hex(b"other")
        ));
        assert_eq!(sums.len(), 2);
        assert_eq!(check_sum("a.tar.gz", b"archive", &sums), Ok(sum));
        let wrong = check_sum("a.tar.gz", b"altered", &sums).expect_err("refused");
        assert!(wrong.contains("checked by SHA-256 only"), "{wrong}");
        assert!(
            check_sum("b.zip", b"other", &sums).is_ok(),
            "a binary-mode line"
        );
        assert!(check_sum("c.tar.gz", b"archive", &sums)
            .expect_err("unlisted")
            .contains("not listed"));
    }

    /// Spec: hub: releases are the source's `network-hub-v<version>` tags, drafts left out, and the
    /// source is a GitHub repository or a mirror of its releases API.
    #[test]
    fn releases_are_the_sources_hub_tags() {
        let json = br#"[
            {"tag_name": "network-hub-v0.1.2", "prerelease": false, "assets": [
                {"name": "SHA256SUMS", "browser_download_url": "https://dl/sums"}]},
            {"tag_name": "network-hub-v0.2.0-rc.1", "prerelease": true, "assets": []},
            {"tag_name": "network-hub-v0.3.0", "draft": true, "assets": []},
            {"tag_name": "empyrean-v0.4.0", "assets": []},
            {"tag_name": "network-hub-vnext", "assets": []}
        ]"#;
        let found = parse_releases(json).expect("read");
        assert_eq!(
            found
                .iter()
                .map(|p| p.version.to_string())
                .collect::<Vec<_>>(),
            ["0.1.2", "0.2.0-rc.1"]
        );
        assert_eq!(found[0].asset("SHA256SUMS"), Some("https://dl/sums"));
        assert!(found[1].prerelease);
        assert!(parse_releases(b"{\"message\": \"Not Found\"}").is_err());

        let s = Source::parse("https://github.com/dereth-network/dereth.git/").expect("GitHub");
        assert_eq!(
            (s.api.as_str(), s.repo.as_str()),
            (
                "https://api.github.com/repos/dereth-network/dereth",
                "dereth-network/dereth"
            )
        );
        let m = Source::parse("http://127.0.0.1:9/mirror/repos/me/hub").expect("a mirror");
        assert_eq!(m.api, "http://127.0.0.1:9/mirror/repos/me/hub");
        for bad in [
            "https://github.com/dereth-network",
            "https://gitlab.com/a/b",
            "ftp://x/repos/a/b",
            "",
        ] {
            assert!(Source::parse(bad).is_err(), "{bad}");
        }
        assert!(Source::parse(SOURCE_URL).is_ok(), "this build's source");
        assert!(v("0.1.2-rc.1") < v("0.1.2") && v("0.1.2") < v("0.1.10"));
        assert!(Version::parse("01.2.3").is_err() && Version::parse("1.2").is_err());
    }

    /// The releases API's list served a page at a time, each page naming the next in its `Link`
    /// header as GitHub does, and the pages asked for.
    #[derive(Debug)]
    struct Pages {
        pages: Vec<Result<Vec<&'static str>, String>>,
        asked: std::cell::RefCell<Vec<usize>>,
        /// Where the next pages are said to be.
        next_at: &'static str,
    }

    impl Pages {
        fn new(pages: Vec<Vec<&'static str>>) -> Self {
            Self {
                pages: pages.into_iter().map(Ok).collect(),
                asked: std::cell::RefCell::default(),
                next_at: "https://api.github.com/repositories/1/releases?per_page=100",
            }
        }
    }

    impl Fetcher for Pages {
        fn get(&self, url: &str, _: Fetching) -> Result<Vec<u8>, String> {
            Err(format!("{url} is read as a page of the list"))
        }

        fn list_page(&self, url: &str) -> Result<(Vec<u8>, Option<String>), String> {
            let n = url
                .rsplit_once("&page=")
                .map_or(1, |(_, n)| n.parse::<usize>().expect("a page"));
            self.asked.borrow_mut().push(n);
            let tags = self.pages[n - 1].clone()?;
            let body = format!(
                "[{}]",
                tags.iter()
                    .map(|t| format!(r#"{{"tag_name": "{t}", "assets": []}}"#))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let link = format!(
                "<{}&page={}>; rel=\"next\", <{}&page={}>; rel=\"last\"",
                self.next_at,
                n + 1,
                self.next_at,
                self.pages.len()
            );
            let next = (n < self.pages.len()).then(|| next_page(&link)).flatten();
            Ok((body.into_bytes(), next))
        }
    }

    fn github() -> Source {
        Source::parse("https://github.com/dereth-network/dereth").expect("GitHub")
    }

    fn versions(found: &[Published]) -> Vec<String> {
        found.iter().map(|p| p.version.to_string()).collect()
    }

    /// Spec: hub: the releases API's `Link` header names the next page of the list.
    #[test]
    fn the_link_header_names_the_next_page() {
        let at = "https://api.github.com/repositories/1/releases?per_page=100";
        assert_eq!(
            next_page(&format!(
                "<{at}&page=2>; rel=\"next\", <{at}&page=5>; rel=\"last\""
            ))
            .as_deref(),
            Some(format!("{at}&page=2").as_str())
        );
        assert_eq!(
            next_page(&format!(
                "<{at}&page=4>; rel=\"prev\", <{at}&page=1>; rel=\"first\""
            )),
            None,
            "the last page"
        );
        assert_eq!(
            next_page("<https://mirror.test/r?page=2>; rel=next").as_deref(),
            Some("https://mirror.test/r?page=2")
        );
        assert_eq!(next_page(""), None);
    }

    /// Spec: hub: the list is read a page at a time, down to the first page holding a hub
    /// release at or below this build's version, and no further: the newer ones are all above it.
    #[test]
    fn the_list_is_read_down_to_a_release_at_or_below_this_build() {
        let pages = Pages::new(vec![
            vec!["empyrean-v0.3.5", "network-hub-v0.4.0", "dereth-v0.4.6"],
            vec!["lyceum-v0.1.0", "network-hub-v0.3.2"],
            vec![
                "empyrean-v0.3.4",
                "network-hub-v0.3.1",
                "network-hub-v0.3.0",
            ],
            vec!["network-hub-v0.2.0"],
        ]);
        let found = list_releases(&pages, &github(), &v("0.3.1")).expect("listed");
        assert_eq!(*pages.asked.borrow(), [1, 2, 3]);
        assert_eq!(versions(&found), ["0.4.0", "0.3.2", "0.3.1", "0.3.0"]);

        // A build whose own version was never released stops below it all the same.
        let pages = Pages::new(vec![
            vec!["network-hub-v0.4.0"],
            vec!["network-hub-v0.3.2-rc.1"],
            vec!["network-hub-v0.3.1"],
        ]);
        list_releases(&pages, &github(), &v("0.3.2-rc.2")).expect("listed");
        assert_eq!(*pages.asked.borrow(), [1, 2]);

        // The newest is on the first page: one page, as before.
        let pages = Pages::new(vec![vec!["network-hub-v0.3.1"], vec!["network-hub-v0.3.0"]]);
        list_releases(&pages, &github(), &v("0.3.1")).expect("listed");
        assert_eq!(*pages.asked.borrow(), [1]);
    }

    /// Spec: hub: a newer hub release a hundred other releases have pushed off the first page is
    /// still found, and the reading stops after ten pages.
    #[test]
    fn a_release_on_a_later_page_is_found_and_ten_pages_are_the_most_read() {
        let mut list = vec![vec!["empyrean-v0.3.5"]; 4];
        list.push(vec!["network-hub-v0.3.2", "network-hub-v0.3.1"]);
        let pages = Pages::new(list);
        let found = list_releases(&pages, &github(), &v("0.3.1")).expect("listed");
        assert_eq!(*pages.asked.borrow(), [1, 2, 3, 4, 5]);
        assert_eq!(versions(&found), ["0.3.2", "0.3.1"]);

        let pages = Pages::new(vec![vec!["empyrean-v0.3.5"]; MAX_PAGES + 3]);
        let found = list_releases(&pages, &github(), &v("0.3.1")).expect("listed");
        assert_eq!(*pages.asked.borrow(), (1..=MAX_PAGES).collect::<Vec<_>>());
        assert!(found.is_empty());
    }

    /// Spec: hub: a page refused partway (GitHub's hourly limit) fails the check, as a refused
    /// first page does, and a next page on another host is not read.
    #[test]
    fn a_page_refused_partway_fails_the_check_and_another_host_is_not_read() {
        let mut pages = Pages::new(vec![
            vec!["empyrean-v0.3.5"],
            vec!["network-hub-v0.4.0"],
            vec!["network-hub-v0.3.1"],
        ]);
        pages.pages[1] = Err("https://api.github.com/... refused the request (HTTP 403)".into());
        let e = list_releases(&pages, &github(), &v("0.3.1")).expect_err("refused");
        assert!(e.contains("HTTP 403"), "{e}");
        assert_eq!(*pages.asked.borrow(), [1, 2]);

        let mut pages = Pages::new(vec![vec!["empyrean-v0.3.5"], vec!["network-hub-v0.4.0"]]);
        pages.next_at = "https://elsewhere.test/releases?per_page=100";
        let found = list_releases(&pages, &github(), &v("0.3.1")).expect("listed");
        assert_eq!(*pages.asked.borrow(), [1]);
        assert!(found.is_empty());
    }
}
