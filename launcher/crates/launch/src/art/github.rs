//! A GitHub repository as an art pack source: its releases are one pack's versions.
//!
//! The source's address is the repository's, `https://github.com/<owner>/<repo>`. Its list is read
//! from GitHub's releases API ([`api_url`]), newest first; the newest release that is neither a
//! draft nor a pre-release and whose tag is a version (`v1.2.0` or `1.2.0`) is the pack's latest
//! ([`newest`]). That release holds the pack as its one `*.dat` asset, and the file's SHA-256 in
//! `<file>.sha256` beside it (a `sha256sum` line); where GitHub also says the asset's digest, the two
//! must agree. The release's body is the pack's description.
//!
//! What it comes to is a list in the shape of an `art.json` ([`catalog`]), so the shelf keeps and
//! checks it as it keeps any source's. Until a pack is installed its id is not known, since it is in
//! the pack's own manifest: the list names it `com.github.<owner>.<repo>` ([`fallback_id`]), and the
//! shelf takes the manifest's id when it installs it.
//!
//! GitHub limits how often it is asked, so a list is asked for again only with the tag GitHub gave
//! the last answer (`If-None-Match`): an answer of "not modified" costs nothing. The fetching is the
//! caller's. A web page cannot fetch a release's files (GitHub sends no CORS headers for them), so
//! only the desktop launcher reads these sources.

use dereth_art_catalog::{Catalog, Entry, MAX_PACK_BYTES};
use serde::Deserialize;

use crate::releases::Version;

/// The checksum file beside a release's pack: its file name and this.
pub const SUMS_SUFFIX: &str = ".sha256";

/// The largest checksum file read.
pub const MAX_SUMS: usize = 4096;

/// The owner and the repository a source's address names, when it is a GitHub repository's:
/// `https://github.com/<owner>/<repo>`, with or without a `/` or `.git` at its end.
#[must_use]
pub fn repository(url: &str) -> Option<(String, String)> {
    let rest = url.trim().strip_prefix("https://github.com/")?;
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let word = |w: &str| {
        !w.is_empty()
            && w.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    };
    (parts.next().is_none() && word(owner) && word(repo))
        .then(|| (owner.to_owned(), repo.to_owned()))
}

/// The address a GitHub repository's source is kept by: `https://github.com/<owner>/<repo>`.
#[must_use]
pub fn source_url(owner: &str, repo: &str) -> String {
    format!("https://github.com/{owner}/{repo}")
}

/// The releases API's first page for the repository.
#[must_use]
pub fn api_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=30")
}

/// The id a GitHub repository's pack is listed by until its manifest names its own:
/// `com.github.<owner>.<repo>`, lowercase, any character a pack id cannot hold made `-`.
#[must_use]
pub fn fallback_id(owner: &str, repo: &str) -> String {
    let word = |w: &str| {
        let w: String = w
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let w = w.trim_matches('-');
        if w.is_empty() {
            "x".to_owned()
        } else {
            w.to_owned()
        }
    };
    format!("com.github.{}.{}", word(owner), word(repo))
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// A repository's newest release, as far as its pack goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newest {
    pub version: String,
    /// The release's title, else its tag.
    pub title: String,
    /// The release's body.
    pub notes: String,
    /// The day it was published, `YYYY-MM-DD`.
    pub released: String,
    /// The pack's file: its name, size and address.
    pub file: String,
    pub size: u64,
    pub url: String,
    /// The SHA-256 GitHub says of the file, when it says one.
    pub digest: Option<String>,
    /// Where its checksum file is, when the release has one.
    pub sums_url: Option<String>,
}

/// The newest release of the releases API's answer `json` that is neither a draft nor a
/// pre-release and is tagged with a version: GitHub lists the newest first.
///
/// # Errors
/// The answer is not a list of releases, there is no such release, or it holds no `*.dat` file or
/// more than one, or one larger than a packed pack can be.
pub fn newest(json: &[u8]) -> Result<Newest, String> {
    let list: Vec<ApiRelease> =
        serde_json::from_slice(json).map_err(|_| crate::components::listing_error(json))?;
    let r = list
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .find(|r| Version::parse(r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name)).is_some())
        .ok_or("the repository has no published release tagged with a version")?;
    let version = r
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&r.tag_name)
        .to_owned();
    let packs: Vec<&ApiAsset> = r
        .assets
        .iter()
        .filter(|a| a.name.to_ascii_lowercase().ends_with(".dat"))
        .collect();
    let pack = match packs.as_slice() {
        [one] => *one,
        [] => return Err(format!("release {} has no .dat file", r.tag_name)),
        _ => {
            return Err(format!(
                "release {} has more than one .dat file",
                r.tag_name
            ))
        }
    };
    if pack.size == 0 || pack.size > MAX_PACK_BYTES {
        return Err(format!(
            "release {}'s {} is {} bytes, not a packed pack's",
            r.tag_name, pack.name, pack.size
        ));
    }
    let sums = format!("{}{SUMS_SUFFIX}", pack.name);
    let sums_url = r
        .assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(&sums))
        .map(|a| a.browser_download_url.clone());
    let digest = pack
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| is_sha256(d))
        .map(str::to_ascii_lowercase);
    Ok(Newest {
        title: title_of(r.name.as_deref(), &r.tag_name, &version),
        version,
        notes: r.body.unwrap_or_default().trim().to_owned(),
        released: r
            .published_at
            .and_then(|p| p.get(..10).map(str::to_owned))
            .unwrap_or_default(),
        file: pack.name.clone(),
        size: pack.size,
        url: pack.browser_download_url.clone(),
        digest,
        sums_url,
    })
}

/// A release's title without the version it ends with (`Enhanced Environment 0.1.1` is the pack
/// `Enhanced Environment`), else its tag.
fn title_of(name: Option<&str>, tag: &str, version: &str) -> String {
    let name = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(tag);
    let bare = [tag, version]
        .iter()
        .find_map(|v| name.strip_suffix(v))
        .map(str::trim_end)
        .filter(|n| !n.is_empty());
    bare.unwrap_or(name).to_owned()
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The SHA-256 a checksum file `text` gives the file `name`: a `sha256sum` line (`<hex>  <name>`,
/// the name maybe marked binary with `*`), or a file holding the hex alone.
#[must_use]
pub fn sum_of(text: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let named = lines.iter().find_map(|l| {
        let (hex, file) = l.split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && is_sha256(hex)).then(|| hex.to_ascii_lowercase())
    });
    named.or_else(|| match lines.as_slice() {
        [one] if is_sha256(one) => Some(one.to_ascii_lowercase()),
        _ => None,
    })
}

/// The list the repository `owner`/`repo` comes to, as an `art.json`: its one pack, at its newest
/// release `newest`, with the SHA-256 its checksum file `sums` gives.
///
/// # Errors
/// The release has no checksum file, the file names no checksum for the pack, or GitHub's digest
/// of the file is another.
pub fn catalog(
    owner: &str,
    repo: &str,
    newest: &Newest,
    sums: Option<&str>,
) -> Result<Catalog, String> {
    let from_file = match sums {
        Some(text) => Some(sum_of(text, &newest.file).ok_or_else(|| {
            format!(
                "{}{SUMS_SUFFIX} names no SHA-256 for {}",
                newest.file, newest.file
            )
        })?),
        None => None,
    };
    let sha256 = match (from_file, &newest.digest) {
        (Some(file), Some(digest)) if &file != digest => {
            return Err(format!(
                "{}{SUMS_SUFFIX} says {file}, and GitHub says the file is {digest}",
                newest.file
            ))
        }
        (Some(file), _) => file,
        (None, _) => {
            return Err(format!(
                "release {} has no {}{SUMS_SUFFIX}",
                newest.version, newest.file
            ))
        }
    };
    Ok(Catalog {
        name: format!("{owner}/{repo}"),
        packs: vec![Entry {
            id: fallback_id(owner, repo),
            name: newest.title.clone(),
            version: newest.version.clone(),
            description: newest.notes.clone(),
            homepage: source_url(owner, repo),
            file: newest.url.clone(),
            sha256,
            size: newest.size,
            released: newest.released.clone(),
            ..Entry::default()
        }],
    })
}
