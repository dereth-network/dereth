//! Which of the repository's GitHub releases the launcher updates from.
//!
//! One repository publishes both Dereth's releases (tagged `dereth-v<version>`) and Empyrean's, so
//! "the repository's latest release" names whichever a maintainer last marked, and says nothing
//! about Dereth. Instead the launcher lists the releases through GitHub's REST API and picks the
//! newest published Dereth release itself: not a draft, not a pre-release, and newest by its
//! version (compared as semver), never by date or by its place in the list. That release's
//! `latest.json` is what the updater reads.
//!
//! This module is the choosing, over the API's answer; the launcher does the fetching.

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

/// The releases API address listing the releases of the GitHub repository at `source_url`
/// (`https://github.com/<owner>/<name>`), newest first, as many as one page holds (100).
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
}
