//! Which of the repository's GitHub releases the launcher updates from.
//!
//! One repository publishes both Dereth's releases (tagged `dereth-v<version>`) and Empyrean's, so
//! "the repository's latest release" names whichever a maintainer last marked, and says nothing
//! about Dereth. Instead the launcher lists the releases through GitHub's REST API and picks the
//! newest published Dereth release itself: not a draft, not a pre-release, and newest by its
//! version (compared as semver), never by date or by its place in the list. That release's
//! `latest.json` is what the updater reads.
//!
//! The API lists a hundred releases a page, newest first, and every product's releases share the
//! list, so a product that has not released for a while is on a later page. The list is read a
//! page at a time ([`Listing`]), following each answer's `Link` header, and only as far as what is
//! looked for ([`Need`]) is found: one page when every product's newest release is on the first.
//! At most [`MAX_PAGES`] pages are read.
//!
//! This module is the choosing and the paging, over the API's answers; the launcher does the
//! fetching.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// The repository the launcher's releases come from. A build sets `DERETH_BUILD_SOURCE_URL`
/// (compile time; the release workflow names the repository it runs in), and a fork's build names
/// its own; otherwise this project's repository.
pub const SOURCE_URL: &str = match option_env!("DERETH_BUILD_SOURCE_URL") {
    Some(url) if !url.is_empty() => url,
    _ => "https://github.com/dereth-network/dereth",
};

/// The prefix of a Dereth release's tag; the rest is its version.
pub const TAG_PREFIX: &str = "dereth-v";

/// The release asset the updater reads.
pub const MANIFEST_ASSET: &str = "latest.json";

/// The releases API address of the first page of the releases of the GitHub repository at
/// `source_url` (`https://github.com/<owner>/<name>`), newest first, as many as a page holds (100).
///
/// `None` when `source_url` is not a GitHub repository address.
#[must_use]
pub fn releases_api_url(source_url: &str) -> Option<String> {
    let rest = source_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => Some(format!(
            "https://api.github.com/repos/{owner}/{name}/releases?per_page=100"
        )),
        _ => None,
    }
}

/// The most pages of the release list that are read: a thousand releases.
pub const MAX_PAGES: usize = 10;

/// How long a release list once read answers again before it is asked for anew.
pub const LIST_KEPT: Duration = Duration::from_secs(10 * 60);

/// The next page's address in an answer's `Link` header
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

/// One page of the release list as fetched: its body, and the next page's address from its
/// `Link` header ([`next_page`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    pub body: Vec<u8>,
    pub next: Option<String>,
}

/// What a reader looks for in the release list, which says how many of its pages to read. GitHub
/// lists the newest release first, so what is looked for is found by the page that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Need {
    /// The newest published release whose tag is the prefix and a version: found once one is
    /// listed that is not a draft or a pre-release.
    Newest(String),
    /// Every release whose tag is the prefix and a version newer than this one: found once a
    /// release at or below it is listed.
    Since(String, Version),
}

impl Need {
    fn found_by(&self, r: &Listed) -> bool {
        let version = |prefix: &str| {
            r.tag
                .strip_prefix(prefix)
                .and_then(crate::components::version)
        };
        match self {
            Self::Newest(prefix) => !r.draft && !r.prerelease && version(prefix).is_some(),
            Self::Since(prefix, since) => version(prefix).is_some_and(|v| v <= *since),
        }
    }
}

/// What the needs are weighed on: a listed release's tag and marks.
#[derive(Debug, Clone)]
struct Listed {
    tag: String,
    draft: bool,
    prerelease: bool,
}

/// The release list, read a page at a time and only as far as its readers need.
#[derive(Debug, Clone)]
pub struct Listing {
    /// Every release on the pages read, in the API's order.
    releases: Vec<serde_json::Value>,
    listed: Vec<Listed>,
    /// The page to read next; none once the last one is read.
    next: Option<String>,
    /// Where the first page is: a next page elsewhere is not followed.
    origin: String,
    pages: usize,
}

impl Listing {
    /// Nothing read yet; the first page is at `first` ([`releases_api_url`]).
    #[must_use]
    pub fn new(first: &str) -> Self {
        Self {
            releases: Vec::new(),
            listed: Vec::new(),
            next: Some(first.to_owned()),
            origin: origin(first).to_owned(),
            pages: 0,
        }
    }

    /// How many pages have been read.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.pages
    }

    /// Whether every page there is to read has been, or [`MAX_PAGES`] of them.
    #[must_use]
    pub fn read_all(&self) -> bool {
        self.next.is_none() || self.pages >= MAX_PAGES
    }

    /// Whether the pages read answer `needs`: each is found, or there is no more to read.
    #[must_use]
    pub fn answers(&self, needs: &[Need]) -> bool {
        self.read_all()
            || needs
                .iter()
                .all(|n| self.listed.iter().any(|r| n.found_by(r)))
    }

    /// Reads pages through `fetch` until they answer `needs`.
    ///
    /// # Errors
    /// The first page that could not be fetched, or is not a list of releases, in words for a
    /// person (a rate limit as [`crate::components::RATE_LIMITED`]). The pages before it are kept,
    /// and reading again starts at it.
    pub fn read(
        &mut self,
        needs: &[Need],
        fetch: &mut dyn FnMut(&str) -> Result<Page, String>,
    ) -> Result<(), String> {
        while !self.answers(needs) {
            let Some(url) = self.next.clone() else { break };
            let page = fetch(&url)?;
            let list: Vec<serde_json::Value> = serde_json::from_slice(&page.body)
                .map_err(|_| crate::components::listing_error(&page.body))?;
            for r in &list {
                self.listed.push(Listed {
                    tag: r["tag_name"].as_str().unwrap_or_default().to_owned(),
                    draft: r["draft"].as_bool().unwrap_or(false),
                    prerelease: r["prerelease"].as_bool().unwrap_or(false),
                });
            }
            self.releases.extend(list);
            self.pages += 1;
            self.next = page.next.filter(|n| origin(n) == self.origin);
        }
        Ok(())
    }

    /// Every release read so far, as one list in the API's own form.
    #[must_use]
    pub fn json(&self) -> Vec<u8> {
        serde_json::to_vec(&self.releases).unwrap_or_else(|_| b"[]".to_vec())
    }
}

/// The release list the launcher keeps between asks, and when its first page was read.
#[derive(Debug)]
pub struct Kept {
    at: Instant,
    listing: Listing,
    json: Arc<Vec<u8>>,
}

/// The release list starting at `first`, read through `fetch` as far as `needs` take it, as one
/// list in the API's own form. What `kept` holds answers when it was read less than [`LIST_KEPT`]
/// before `now`, and `fresh` is not asked; read further when it does not reach `needs` yet. So the
/// launcher's screens, its downloads and every server's update check share their requests.
///
/// # Errors
/// As [`Listing::read`]. The pages read are kept all the same, unless the first failed.
pub fn kept_list(
    kept: &mut Option<Kept>,
    now: Instant,
    fresh: bool,
    first: &str,
    needs: &[Need],
    fetch: &mut dyn FnMut(&str) -> Result<Page, String>,
) -> Result<Arc<Vec<u8>>, String> {
    if fresh
        || kept
            .as_ref()
            .is_some_and(|k| now.saturating_duration_since(k.at) >= LIST_KEPT)
    {
        *kept = None;
    }
    if let Some(k) = kept.as_ref() {
        if k.listing.answers(needs) {
            return Ok(k.json.clone());
        }
    }
    let mut k = kept.take().unwrap_or_else(|| Kept {
        at: now,
        listing: Listing::new(first),
        json: Arc::default(),
    });
    let read = k.listing.read(needs, fetch);
    k.json = Arc::new(k.listing.json());
    let list = k.json.clone();
    if k.listing.pages() > 0 {
        *kept = Some(k);
    }
    read.map(|()| list)
}

/// A release's version, `MAJOR.MINOR.PATCH`, ordered as semver orders release versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// The version a Dereth release's tag names: `dereth-v<MAJOR>.<MINOR>.<PATCH>`, build metadata
    /// (`+...`) allowed and ignored. `None` for any other tag, and for a pre-release version
    /// (`-rc.1`), which launchers never update to whatever the release is marked.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        let version = tag.strip_prefix(TAG_PREFIX)?;
        let core = version.split_once('+').map_or(version, |(core, _)| core);
        let mut parts = core.split('.');
        let (Some(major), Some(minor), Some(patch), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        Some(Self {
            major: number(major)?,
            minor: number(minor)?,
            patch: number(patch)?,
        })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// One semver numeric identifier: digits only, and no leading zero unless it is `0`.
fn number(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return None;
    }
    s.parse().ok()
}

/// The release the launcher updates from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRelease {
    pub tag: String,
    pub version: Version,
    /// The download address of the release's `latest.json`.
    pub manifest_url: String,
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

/// From a releases API answer (a JSON array of releases), the newest published Dereth release and
/// its `latest.json`. `Ok(None)` when there is no such release, or the newest has no `latest.json`.
///
/// # Errors
/// When `json` is not a list of releases: an error document (a rate limit, an unknown
/// repository) or anything else.
pub fn newest_update_release(json: &[u8]) -> Result<Option<UpdateRelease>, String> {
    let list: Vec<ApiRelease> = serde_json::from_slice(json)
        .map_err(|e| format!("the release list cannot be read: {e}"))?;
    let newest = list
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| Some((Version::from_tag(&r.tag_name)?, r)))
        .max_by_key(|(version, _)| *version);
    Ok(newest.and_then(|(version, r)| {
        let manifest_url = r
            .assets
            .into_iter()
            .find(|a| a.name == MANIFEST_ASSET)?
            .browser_download_url;
        Some(UpdateRelease {
            tag: r.tag_name,
            version,
            manifest_url,
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{self, Product, RATE_LIMITED};

    fn release(tag: &str, draft: bool, prerelease: bool, assets: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "name": tag,
            "draft": draft,
            "prerelease": prerelease,
            "created_at": "2026-10-01T00:00:00Z",
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "size": 1234,
                "browser_download_url":
                    format!("https://github.com/dereth-network/dereth/releases/download/{tag}/{name}"),
            })).collect::<Vec<_>>(),
        })
    }

    fn answer(releases: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(releases).unwrap()
    }

    #[test]
    fn the_newest_published_dereth_release_is_chosen_by_version_among_everything_else() {
        let json = answer(&[
            // Newest in the list, and the highest numbers of all, but Empyrean's.
            release("v9.0.0", false, false, &["release.json"]),
            release("empyrean-v9.1.0", false, false, &["latest.json"]),
            // Dereth, but not published, or not a release.
            release("dereth-v0.9.0", true, false, &["latest.json"]),
            release("dereth-v0.8.0", false, true, &["latest.json"]),
            release("dereth-v0.7.0-rc.1", false, false, &["latest.json"]),
            // Not semver.
            release("dereth-v0.6", false, false, &["latest.json"]),
            release("dereth-v0.06.0", false, false, &["latest.json"]),
            release("dereth-vnext", false, false, &["latest.json"]),
            release("dereth-v0.6.0.1", false, false, &["latest.json"]),
            // Published Dereth releases, out of order: 0.10.0 is newer than 0.3.5 and 0.2.0.
            release("dereth-v0.2.0", false, false, &["latest.json"]),
            release(
                "dereth-v0.10.0",
                false,
                false,
                &["SHA256SUMS", "latest.json"],
            ),
            release("dereth-v0.3.5", false, false, &["latest.json"]),
        ]);
        let chosen = newest_update_release(&json).unwrap().unwrap();
        assert_eq!(chosen.tag, "dereth-v0.10.0");
        assert_eq!(chosen.version.to_string(), "0.10.0");
        assert_eq!(
            chosen.manifest_url,
            "https://github.com/dereth-network/dereth/releases/download/dereth-v0.10.0/latest.json"
        );
    }

    #[test]
    fn no_published_dereth_release_is_nothing_to_update_from() {
        assert_eq!(newest_update_release(b"[]").unwrap(), None);
        let json = answer(&[
            release("empyrean-v1.0.0", false, false, &["release.json"]),
            release("dereth-v1.0.0", true, false, &["latest.json"]),
            release("dereth-v0.9.0", false, true, &["latest.json"]),
        ]);
        assert_eq!(newest_update_release(&json).unwrap(), None);
        // The newest has no latest.json (an unsigned build): an older one is not taken instead.
        let json = answer(&[
            release("dereth-v1.0.0", false, false, &["SHA256SUMS"]),
            release("dereth-v0.9.0", false, false, &["latest.json"]),
        ]);
        assert_eq!(newest_update_release(&json).unwrap(), None);
    }

    #[test]
    fn an_api_error_document_is_an_error() {
        assert!(newest_update_release(
            br#"{"message":"API rate limit exceeded","documentation_url":"https://docs.github.com"}"#
        )
        .is_err());
        assert!(newest_update_release(b"<html>").is_err());
        assert!(newest_update_release(b"").is_err());
    }

    #[test]
    fn versions_order_as_semver_and_build_metadata_is_ignored() {
        let v = |t| Version::from_tag(t).unwrap();
        assert!(v("dereth-v0.10.0") > v("dereth-v0.9.9"));
        assert!(v("dereth-v1.0.0") > v("dereth-v0.99.99"));
        assert!(v("dereth-v0.2.10") > v("dereth-v0.2.9"));
        assert_eq!(v("dereth-v0.2.0+build.5"), v("dereth-v0.2.0"));
        assert_eq!(v("dereth-v0.0.0").to_string(), "0.0.0");
        for not_a_release in [
            "dereth-v1.0.0-beta",
            "dereth-v1.0",
            "dereth-v1.0.0.",
            "dereth-v01.0.0",
            "dereth-v1.-0.0",
            "dereth-v",
            "v1.0.0",
            "empyrean-v1.0.0",
        ] {
            assert_eq!(Version::from_tag(not_a_release), None, "{not_a_release}");
        }
    }

    #[test]
    fn the_api_address_is_the_source_repositorys_release_list() {
        assert_eq!(
            releases_api_url("https://github.com/dereth-network/dereth").as_deref(),
            Some("https://api.github.com/repos/dereth-network/dereth/releases?per_page=100")
        );
        assert_eq!(
            releases_api_url(" https://github.com/someone/fork.git/ ").as_deref(),
            Some("https://api.github.com/repos/someone/fork/releases?per_page=100")
        );
        assert_eq!(releases_api_url("https://gitlab.com/someone/fork"), None);
        assert_eq!(releases_api_url("https://github.com/someone"), None);
        assert_eq!(releases_api_url("https://github.com/a/b/c"), None);
        assert!(releases_api_url(SOURCE_URL).is_some(), "{SOURCE_URL}");
    }

    const FIRST: &str = "https://api.github.com/repos/dereth-network/dereth/releases?per_page=100";

    /// The address GitHub gives page `n` in a `Link` header.
    fn page_url(n: usize) -> String {
        format!("https://api.github.com/repositories/1/releases?per_page=100&page={n}")
    }

    /// A list served page by page, each page naming the next, and what was asked of it.
    struct Pages {
        pages: Vec<Result<Vec<serde_json::Value>, String>>,
        asked: Vec<usize>,
    }

    impl Pages {
        fn new(pages: Vec<Vec<serde_json::Value>>) -> Self {
            Self {
                pages: pages.into_iter().map(Ok).collect(),
                asked: Vec::new(),
            }
        }

        fn fetch(&mut self, url: &str) -> Result<Page, String> {
            let n = if url == FIRST {
                1
            } else {
                url.rsplit("page=").next().unwrap().parse().unwrap()
            };
            self.asked.push(n);
            let releases = self.pages[n - 1].clone()?;
            let link = (n < self.pages.len()).then(|| {
                format!(
                    "<{}>; rel=\"next\", <{}>; rel=\"last\"",
                    page_url(n + 1),
                    page_url(self.pages.len())
                )
            });
            Ok(Page {
                body: answer(&releases),
                next: link.as_deref().and_then(next_page),
            })
        }
    }

    fn published(tag: &str) -> serde_json::Value {
        release(tag, false, false, &["SHA256SUMS", "latest.json"])
    }

    fn components_needed() -> [Need; 2] {
        [
            Product::Empyrean.newest_release(),
            Product::Lyceum.newest_release(),
        ]
    }

    #[test]
    fn the_link_header_names_the_next_page() {
        assert_eq!(
            next_page(&format!(
                "<{}>; rel=\"next\", <{}>; rel=\"last\"",
                page_url(2),
                page_url(4)
            )),
            Some(page_url(2))
        );
        // The last page names only the ones before it.
        assert_eq!(
            next_page(&format!(
                "<{}>; rel=\"prev\", <{}>; rel=\"first\"",
                page_url(3),
                page_url(1)
            )),
            None
        );
        assert_eq!(
            next_page("<https://example.test/r?page=2>; rel=next"),
            Some("https://example.test/r?page=2".to_owned())
        );
        assert_eq!(next_page(""), None);
        assert_eq!(
            next_page("https://example.test/r?page=2; rel=\"next\""),
            None
        );
    }

    #[test]
    fn one_page_is_read_when_every_product_looked_for_is_on_it() {
        let mut pages = Pages::new(vec![
            vec![
                published("dereth-v0.4.6"),
                published("empyrean-v0.3.4"),
                published("lyceum-v0.1.0"),
            ],
            vec![published("empyrean-v0.3.3")],
        ]);
        let mut listing = Listing::new(FIRST);
        listing
            .read(&components_needed(), &mut |u| pages.fetch(u))
            .unwrap();
        assert_eq!(pages.asked, [1]);
        let json = listing.json();
        assert_eq!(
            components::newest(&json, Product::Lyceum)
                .unwrap()
                .unwrap()
                .version,
            "0.1.0"
        );
    }

    #[test]
    fn a_product_only_on_page_three_is_found_by_reading_three_pages() {
        let mut pages = Pages::new(vec![
            vec![
                published("dereth-v0.4.6"),
                published("empyrean-v0.3.4"),
                // Neither is Lyceum's release: a pre-release, and another product's tag.
                release("lyceum-v0.2.0", false, true, &["SHA256SUMS"]),
                published("lyceum-plugin-v9.0.0"),
            ],
            vec![published("dereth-v0.4.5"), published("empyrean-v0.4.0")],
            vec![published("dereth-v0.4.4"), published("lyceum-v0.1.0")],
            vec![published("lyceum-v0.0.9")],
        ]);
        let mut listing = Listing::new(FIRST);
        listing
            .read(&components_needed(), &mut |u| pages.fetch(u))
            .unwrap();
        assert_eq!(pages.asked, [1, 2, 3], "the fourth page is not asked for");
        let json = listing.json();
        assert_eq!(
            components::newest(&json, Product::Lyceum)
                .unwrap()
                .unwrap()
                .version,
            "0.1.0"
        );
        // The newest by version, wherever it is in the list.
        assert_eq!(
            components::newest(&json, Product::Empyrean)
                .unwrap()
                .unwrap()
                .version,
            "0.4.0"
        );
        assert_eq!(
            components::releases(&json, Product::Empyrean)
                .unwrap()
                .len(),
            2
        );
        // The launcher's own update reads the same way.
        let mut pages = Pages::new(vec![
            vec![published("empyrean-v0.3.4")],
            vec![published("lyceum-v0.1.0")],
            vec![published("dereth-v0.4.6")],
        ]);
        let mut listing = Listing::new(FIRST);
        listing
            .read(&[Need::Newest(TAG_PREFIX.to_owned())], &mut |u| {
                pages.fetch(u)
            })
            .unwrap();
        assert_eq!(pages.asked, [1, 2, 3]);
        assert_eq!(
            newest_update_release(&listing.json()).unwrap().unwrap().tag,
            "dereth-v0.4.6"
        );
    }

    #[test]
    fn at_most_ten_pages_are_read_for_a_product_not_listed() {
        let mut pages = Pages::new(
            (0..MAX_PAGES + 5)
                .map(|i| vec![published(&format!("dereth-v0.{i}.0"))])
                .collect(),
        );
        let mut listing = Listing::new(FIRST);
        listing
            .read(&components_needed(), &mut |u| pages.fetch(u))
            .unwrap();
        assert_eq!(pages.asked, (1..=MAX_PAGES).collect::<Vec<_>>());
        assert!(listing.answers(&components_needed()));
        assert_eq!(
            components::newest(&listing.json(), Product::Lyceum).unwrap(),
            None
        );
        // A list that ends before the bound ends the reading.
        let mut pages = Pages::new(vec![vec![published("dereth-v0.4.6")], vec![]]);
        let mut listing = Listing::new(FIRST);
        listing
            .read(&components_needed(), &mut |u| pages.fetch(u))
            .unwrap();
        assert_eq!(pages.asked, [1, 2]);
    }

    #[test]
    fn a_refusal_midway_is_said_and_reading_again_starts_at_the_refused_page() {
        let mut pages = Pages::new(vec![
            vec![published("empyrean-v0.3.4")],
            vec![published("dereth-v0.4.6")],
            vec![published("lyceum-v0.1.0")],
        ]);
        pages.pages[1] = Err(RATE_LIMITED.to_owned());
        let mut listing = Listing::new(FIRST);
        assert_eq!(
            listing.read(&components_needed(), &mut |u| pages.fetch(u)),
            Err(RATE_LIMITED.to_owned())
        );
        assert_eq!(listing.pages(), 1);
        assert!(!listing.answers(&components_needed()));
        pages.pages[1] = Ok(vec![published("dereth-v0.4.6")]);
        pages.asked.clear();
        listing
            .read(&components_needed(), &mut |u| pages.fetch(u))
            .unwrap();
        assert_eq!(pages.asked, [2, 3]);

        // A page that is GitHub's rate-limit document instead of a list is the same refusal.
        let mut listing = Listing::new(FIRST);
        let mut asked = 0;
        let said = listing.read(&components_needed(), &mut |_| {
            asked += 1;
            Ok(Page {
                body: if asked == 1 {
                    answer(&[published("empyrean-v0.3.4")])
                } else {
                    br#"{"message":"API rate limit exceeded for 203.0.113.1."}"#.to_vec()
                },
                next: Some(page_url(asked + 1)),
            })
        });
        assert_eq!(said, Err(RATE_LIMITED.to_owned()));
        assert_eq!(listing.pages(), 1);
    }

    #[test]
    fn a_next_page_on_another_server_is_not_followed() {
        let mut asked = Vec::new();
        let mut listing = Listing::new(FIRST);
        listing
            .read(&components_needed(), &mut |u| {
                asked.push(u.to_owned());
                Ok(Page {
                    body: answer(&[published("dereth-v0.4.6")]),
                    next: Some("https://example.test/releases?page=2".to_owned()),
                })
            })
            .unwrap();
        assert_eq!(asked, [FIRST]);
        assert!(listing.read_all());
    }

    #[test]
    fn every_release_since_a_version_is_read_down_to_it() {
        let v = |s| crate::components::version(s).unwrap();
        let mut pages = Pages::new(vec![
            vec![published("empyrean-v0.4.1"), published("lyceum-v0.1.0")],
            vec![published("empyrean-v0.4.0")],
            vec![published("empyrean-v0.3.4"), published("empyrean-v0.3.3")],
            vec![published("empyrean-v0.3.2")],
        ]);
        let mut listing = Listing::new(FIRST);
        listing
            .read(&[Product::Empyrean.releases_since(v("0.3.4"))], &mut |u| {
                pages.fetch(u)
            })
            .unwrap();
        assert_eq!(pages.asked, [1, 2, 3]);
        // A version with no release of its own stops at the first one below it.
        pages.asked.clear();
        let mut listing = Listing::new(FIRST);
        listing
            .read(&[Product::Empyrean.releases_since(v("0.3.9"))], &mut |u| {
                pages.fetch(u)
            })
            .unwrap();
        assert_eq!(pages.asked, [1, 2, 3]);
    }

    #[test]
    fn the_kept_list_answers_for_ten_minutes_and_is_read_further_only_for_more() {
        let mut pages = Pages::new(vec![
            vec![published("empyrean-v0.3.4")],
            vec![published("lyceum-v0.1.0")],
        ]);
        let mut kept = None;
        let t0 = Instant::now();
        let empyrean = [Product::Empyrean.newest_release()];
        let lyceum = [Product::Lyceum.newest_release()];
        let mut list = |kept: &mut Option<Kept>, at, fresh, needs: &[Need]| {
            kept_list(kept, at, fresh, FIRST, needs, &mut |u| pages.fetch(u))
        };
        let first = list(&mut kept, t0, false, &empyrean).unwrap();
        assert!(components::newest(&first, Product::Empyrean)
            .unwrap()
            .is_some());
        let again = list(
            &mut kept,
            t0 + Duration::from_secs(9 * 60),
            false,
            &empyrean,
        )
        .unwrap();
        assert!(
            Arc::ptr_eq(&first, &again),
            "the same list, not asked again"
        );
        let more = list(&mut kept, t0 + Duration::from_secs(9 * 60), false, &lyceum).unwrap();
        assert!(components::newest(&more, Product::Lyceum)
            .unwrap()
            .is_some());
        list(&mut kept, t0 + LIST_KEPT, false, &empyrean).unwrap();
        list(&mut kept, t0 + LIST_KEPT, true, &empyrean).unwrap();
        assert_eq!(
            pages.asked,
            [1, 2, 1, 1],
            "page 2 for Lyceum alone; then anew after ten minutes, and when asked fresh"
        );

        // A refused first page keeps nothing, so the next ask asks again.
        pages.pages[0] = Err(RATE_LIMITED.to_owned());
        pages.asked.clear();
        let mut kept = None;
        let mut list = |kept: &mut Option<Kept>| {
            kept_list(kept, t0, false, FIRST, &empyrean, &mut |u| pages.fetch(u))
        };
        assert_eq!(list(&mut kept), Err(RATE_LIMITED.to_owned()));
        assert!(kept.is_none());
        assert_eq!(list(&mut kept), Err(RATE_LIMITED.to_owned()));
        assert_eq!(pages.asked, [1, 1]);
    }
}
