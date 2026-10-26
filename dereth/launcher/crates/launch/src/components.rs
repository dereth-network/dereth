//! The two programs the launcher fetches only when asked: Empyrean, the server a player runs their
//! own world on, and Lyceum, the world builder.
//!
//! Each comes from its own newest published GitHub release in the repository the launcher updates
//! from: tagged `empyrean-v<version>` or `lyceum-v<version>`, not a draft, not a pre-release, and
//! newest by version. The launcher takes the release's file for this system, named as the release
//! packaging names it ([`Product::archive_name`]), and checks it against the release's
//! `SHA256SUMS` before unpacking it. Nothing is signed: the checksums come from the same release as
//! the file, so they catch a damaged or altered download, as the server's own updater does.
//!
//! This module is the choosing and the checking, over bytes the launcher fetched.

use crate::releases::{Need, Version};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Component as PathComponent, Path, PathBuf};

/// A separately released program the launcher can fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    Empyrean,
    Lyceum,
}

impl Product {
    /// Both, in the order the page shows them.
    pub const ALL: [Self; 2] = [Self::Empyrean, Self::Lyceum];

    /// The lower-case name its tags, files and folders use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Empyrean => "empyrean",
            Self::Lyceum => "lyceum",
        }
    }

    /// The name a person reads.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Empyrean => "Empyrean",
            Self::Lyceum => "Lyceum",
        }
    }

    /// The program the launcher starts, without the system's executable suffix.
    #[must_use]
    pub const fn program(self) -> &'static str {
        match self {
            Self::Empyrean => "empyrean-server",
            Self::Lyceum => "lyceum-editor",
        }
    }

    /// A release's tag: `empyrean-v0.3.3`.
    #[must_use]
    pub fn tag(self, version: &str) -> String {
        format!("{}-v{version}", self.name())
    }

    /// What the release list is read for to offer this product: its newest release.
    #[must_use]
    pub fn newest_release(self) -> Need {
        Need::Newest(self.tag(""))
    }

    /// What the release list is read for to update from `version`: every newer release.
    #[must_use]
    pub fn releases_since(self, version: Version) -> Need {
        Need::Since(self.tag(""), version)
    }

    /// The release file for the target `triple`, as the release packaging names it; `None` when
    /// the product is not released for that target.
    ///
    /// Empyrean's is `empyrean-<version>-<triple>`, a `.zip` on Windows and a `.tar.gz` elsewhere.
    /// Lyceum's names the platform rather than the triple: `lyceum-<version>-windows-x86_64.zip`,
    /// `lyceum-<version>-linux-x86_64.tar.gz` and `Lyceum-<version>-macos-<arch>.app.tar.gz`.
    #[must_use]
    pub fn archive_name(self, triple: &str, version: &str) -> Option<String> {
        match self {
            Self::Empyrean => {
                let ext = match triple {
                    "x86_64-pc-windows-msvc" => "zip",
                    "x86_64-unknown-linux-gnu"
                    | "aarch64-unknown-linux-gnu"
                    | "aarch64-apple-darwin"
                    | "x86_64-apple-darwin" => "tar.gz",
                    _ => return None,
                };
                Some(format!("empyrean-{version}-{triple}.{ext}"))
            }
            Self::Lyceum => Some(match triple {
                "x86_64-pc-windows-msvc" => format!("lyceum-{version}-windows-x86_64.zip"),
                "x86_64-unknown-linux-gnu" => format!("lyceum-{version}-linux-x86_64.tar.gz"),
                "aarch64-apple-darwin" => format!("Lyceum-{version}-macos-aarch64.app.tar.gz"),
                "x86_64-apple-darwin" => format!("Lyceum-{version}-macos-x86_64.app.tar.gz"),
                _ => return None,
            }),
        }
    }
}

/// The release file that lists every other file's SHA-256.
pub const SUMS_ASSET: &str = "SHA256SUMS";

/// The release file that says what the release is; Empyrean's says what upgrading to it involves.
pub const FACTS_ASSET: &str = "release.json";

/// A strict release version, which also makes a safe folder name.
#[must_use]
pub fn version(value: &str) -> Option<Version> {
    let parsed = Version::from_tag(&format!("dereth-v{value}"))?;
    (parsed.to_string() == value).then_some(parsed)
}

/// One published release of a product, as the releases API lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    /// The release's notes, as its page shows them, without the list of downloads.
    pub notes: String,
    /// Each file's name and download address.
    pub assets: BTreeMap<String, String>,
    /// Each file's size, as GitHub gives it.
    #[serde(default)]
    pub sizes: BTreeMap<String, u64>,
}

impl Release {
    /// The download address of the file `name`.
    ///
    /// # Errors
    /// The release has no such file.
    pub fn asset(&self, name: &str) -> Result<&str, String> {
        self.assets
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("This release has no {name}."))
    }

    /// This system's file: its name and download address.
    ///
    /// # Errors
    /// The product is not released for `triple`, or this release lacks the file.
    pub fn archive(&self, product: Product, triple: &str) -> Result<(String, &str), String> {
        let name = product
            .archive_name(triple, &self.version)
            .ok_or_else(|| format!("{} has no download for this system.", product.title()))?;
        let url = self.asset(&name)?;
        Ok((name, url))
    }

    /// The size of this system's file, when GitHub said.
    #[must_use]
    pub fn archive_size(&self, product: Product, triple: &str) -> Option<u64> {
        let name = product.archive_name(triple, &self.version)?;
        self.sizes.get(&name).copied().filter(|s| *s > 0)
    }
}

/// The notes of a release body: everything above its `## Downloads` heading, which only lists the
/// files.
fn notes_of(body: &str) -> String {
    let mut at = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_end().eq_ignore_ascii_case("## Downloads") {
            break;
        }
        at += line.len();
    }
    body[..at].trim().to_owned()
}

/// What a releases API answer that is not a list says, in words for a person: GitHub answers a
/// rate limit, or an unknown repository, with a document holding a `message`.
pub(crate) fn listing_error(json: &[u8]) -> String {
    let message = serde_json::from_slice::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.get("message")?.as_str().map(str::to_owned));
    match message {
        Some(m) if m.to_ascii_lowercase().contains("rate limit") => RATE_LIMITED.to_owned(),
        Some(m) => format!("GitHub did not list the releases: {m}"),
        None => "GitHub's list of releases could not be read. Try again later.".to_owned(),
    }
}

/// What a person reads when GitHub refuses for now because this network asked too often.
pub const RATE_LIMITED: &str =
    "GitHub is limiting requests from this network for now. Try again in an hour.";

/// Every published release of `product` in a releases API answer, newest first: tagged
/// `<product>-v<version>`, not a draft, not a pre-release.
///
/// # Errors
/// When the answer is not a list of releases, in words for a person ([`RATE_LIMITED`] for a rate
/// limit).
pub fn releases(json: &[u8], product: Product) -> Result<Vec<Release>, String> {
    #[derive(Deserialize)]
    struct Entry {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
        #[serde(default)]
        size: u64,
    }
    let entries: Vec<Entry> = serde_json::from_slice(json).map_err(|_| listing_error(json))?;
    let prefix = format!("{}-v", product.name());
    let mut found: Vec<(Version, Release)> = entries
        .into_iter()
        .filter(|e| !e.draft && !e.prerelease)
        .filter_map(|e| {
            let v = version(e.tag_name.strip_prefix(&prefix)?)?;
            Some((
                v,
                Release {
                    version: v.to_string(),
                    notes: notes_of(e.body.as_deref().unwrap_or("")),
                    sizes: e.assets.iter().map(|a| (a.name.clone(), a.size)).collect(),
                    assets: e
                        .assets
                        .into_iter()
                        .filter(|a| a.browser_download_url.starts_with("https://"))
                        .map(|a| (a.name, a.browser_download_url))
                        .collect(),
                },
            ))
        })
        .collect();
    found.sort_by_key(|(v, _)| std::cmp::Reverse(*v));
    Ok(found.into_iter().map(|(_, r)| r).collect())
}

/// The newest published release of `product`, if it has one.
///
/// # Errors
/// As [`releases`].
pub fn newest(json: &[u8], product: Product) -> Result<Option<Release>, String> {
    Ok(releases(json, product)?.into_iter().next())
}

/// What the page offers for a product: its newest release to download when there is none here
/// yet, or to update to when it is newer than the one here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Offer {
    /// Nothing to offer, or nothing known yet.
    None,
    Download {
        version: String,
    },
    Update {
        version: String,
    },
}

/// The offer for a product whose downloaded copy is `have` when the newest release is `newest`.
#[must_use]
pub fn offer(have: Option<&str>, newest: Option<&str>) -> Offer {
    let Some(newest) = newest else {
        return Offer::None;
    };
    match have {
        None => Offer::Download {
            version: newest.to_owned(),
        },
        Some(have) if version(newest) > version(have) => Offer::Update {
            version: newest.to_owned(),
        },
        Some(_) => Offer::None,
    }
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

/// A file's SHA-256, lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Checks `bytes`, downloaded as `name`, against the release's checksums.
///
/// # Errors
/// When `name` is not listed, or its SHA-256 differs.
pub fn check_sum(name: &str, bytes: &[u8], sums: &[(String, String)]) -> Result<(), String> {
    match sums.iter().find(|(_, n)| n == name) {
        None => Err(format!("{name} is not in the release's checksums.")),
        Some((expected, _)) if *expected != sha256_hex(bytes) => Err(format!(
            "{name} did not download correctly: its checksum does not match. Try again."
        )),
        Some(_) => Ok(()),
    }
}

/// The largest release file the launcher downloads.
pub const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The most a release file may unpack to.
pub const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Archive names use relative portable paths and cannot address devices or streams on Windows.
#[must_use]
pub fn safe_member(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        && !name.chars().any(char::is_control)
        && !name.starts_with('/')
        && name.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with(['.', ' '])
                && !matches!(
                    p.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase()
                        .as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
        && Path::new(name)
            .components()
            .all(|c| matches!(c, PathComponent::Normal(_)))
}

/// Where a downloaded version is unpacked: `components/<product>/<version>` in the launcher's data
/// folder. A player's worlds and servers are never in this tree.
///
/// # Errors
/// An invalid version.
pub fn install_dir(root: &Path, product: Product, v: &str) -> Result<PathBuf, String> {
    version(v).ok_or("Invalid component version.")?;
    Ok(root.join("components").join(product.name()).join(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tag: &str, draft: bool, prerelease: bool, assets: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag, "draft": draft, "prerelease": prerelease,
            "body": format!("Notes for {tag}.\n\n## Downloads\n\nThe files."),
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "size": 16_352_529,
                "browser_download_url":
                    format!("https://github.com/dereth-network/dereth/releases/download/{tag}/{name}"),
            })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn the_newest_published_release_of_each_product_is_chosen_by_version() {
        let json = serde_json::to_vec(&[
            entry("lyceum-v0.9.0", false, false, &["SHA256SUMS"]),
            entry("lyceum-v0.10.0", false, false, &["SHA256SUMS"]),
            entry("empyrean-v8.0.0", false, false, &["SHA256SUMS"]),
            entry("dereth-v9.0.0", false, false, &["latest.json"]),
            entry("lyceum-v3.0.0", true, false, &["SHA256SUMS"]),
            entry("lyceum-v4.0.0", false, true, &["SHA256SUMS"]),
            entry("lyceum-v5.0.0-rc.1", false, false, &["SHA256SUMS"]),
            entry("empyrean-v0.3.3", false, false, &["SHA256SUMS"]),
        ])
        .unwrap();
        let lyceum = releases(&json, Product::Lyceum).unwrap();
        assert_eq!(
            lyceum
                .iter()
                .map(|r| r.version.as_str())
                .collect::<Vec<_>>(),
            ["0.10.0", "0.9.0"]
        );
        assert_eq!(lyceum[0].notes, "Notes for lyceum-v0.10.0.");
        assert_eq!(
            newest(&json, Product::Empyrean).unwrap().unwrap().version,
            "8.0.0"
        );
        let none = serde_json::to_vec(&[entry("dereth-v1.0.0", false, false, &[])]).unwrap();
        assert!(newest(&none, Product::Lyceum).unwrap().is_none());
    }

    #[test]
    fn a_release_without_signed_metadata_is_still_offered() {
        // What the release workflows publish: the platform files, SHA256SUMS, release.json and
        // MANIFEST.txt, and nothing else.
        let json = serde_json::to_vec(&[entry(
            "empyrean-v0.3.3",
            false,
            false,
            &[
                "empyrean-0.3.3-x86_64-pc-windows-msvc.zip",
                "MANIFEST.txt",
                "release.json",
                "SHA256SUMS",
            ],
        )])
        .unwrap();
        let release = newest(&json, Product::Empyrean).unwrap().unwrap();
        let (name, url) = release
            .archive(Product::Empyrean, "x86_64-pc-windows-msvc")
            .unwrap();
        assert_eq!(name, "empyrean-0.3.3-x86_64-pc-windows-msvc.zip");
        assert!(url.ends_with("/empyrean-v0.3.3/empyrean-0.3.3-x86_64-pc-windows-msvc.zip"));
        release.asset(SUMS_ASSET).unwrap();
        assert_eq!(
            release.archive_size(Product::Empyrean, "x86_64-pc-windows-msvc"),
            Some(16_352_529)
        );
        assert!(release
            .archive(Product::Empyrean, "x86_64-unknown-linux-gnu")
            .unwrap_err()
            .contains("no empyrean-0.3.3-x86_64-unknown-linux-gnu.tar.gz"));
    }

    #[test]
    fn a_rate_limit_or_another_error_document_is_said_plainly() {
        let limited = br#"{"message":"API rate limit exceeded for 203.0.113.1.","documentation_url":"https://docs.github.com"}"#;
        assert_eq!(
            releases(limited, Product::Empyrean).unwrap_err(),
            RATE_LIMITED
        );
        assert!(releases(br#"{"message":"Not Found"}"#, Product::Lyceum)
            .unwrap_err()
            .contains("Not Found"));
        assert!(releases(b"<html>", Product::Lyceum).is_err());
    }

    #[test]
    fn the_page_offers_a_download_then_only_a_newer_release() {
        assert_eq!(offer(None, None), Offer::None);
        assert_eq!(
            offer(None, Some("0.3.3")),
            Offer::Download {
                version: "0.3.3".into()
            }
        );
        assert_eq!(offer(Some("0.3.3"), Some("0.3.3")), Offer::None);
        assert_eq!(
            offer(Some("0.3.3"), Some("0.10.0")),
            Offer::Update {
                version: "0.10.0".into()
            }
        );
        assert_eq!(offer(Some("0.4.0"), Some("0.3.3")), Offer::None);
        assert_eq!(offer(Some("0.3.3"), None), Offer::None);
    }

    #[test]
    fn downloads_are_checked_against_the_release_checksums() {
        let sums = format!(
            "{}  lyceum-1.2.3-windows-x86_64.zip\n{}  release.json\nnot a line\n",
            sha256_hex(b"abc"),
            sha256_hex(b"facts").to_ascii_uppercase()
        );
        let sums = parse_sums(&sums);
        assert_eq!(sums.len(), 2);
        check_sum("lyceum-1.2.3-windows-x86_64.zip", b"abc", &sums).unwrap();
        check_sum("release.json", b"facts", &sums).unwrap();
        assert!(check_sum("lyceum-1.2.3-windows-x86_64.zip", b"abd", &sums)
            .unwrap_err()
            .contains("did not download correctly"));
        assert!(check_sum("other.zip", b"abc", &sums)
            .unwrap_err()
            .contains("not in the release's checksums"));
    }

    /// The names the release packaging gives each product's files, for every target it releases.
    #[test]
    fn release_files_are_named_as_the_release_packaging_names_them() {
        let v = "0.3.3";
        for (triple, empyrean, lyceum) in [
            (
                "x86_64-pc-windows-msvc",
                "empyrean-0.3.3-x86_64-pc-windows-msvc.zip",
                Some("lyceum-0.3.3-windows-x86_64.zip"),
            ),
            (
                "x86_64-unknown-linux-gnu",
                "empyrean-0.3.3-x86_64-unknown-linux-gnu.tar.gz",
                Some("lyceum-0.3.3-linux-x86_64.tar.gz"),
            ),
            (
                "aarch64-unknown-linux-gnu",
                "empyrean-0.3.3-aarch64-unknown-linux-gnu.tar.gz",
                None,
            ),
            (
                "aarch64-apple-darwin",
                "empyrean-0.3.3-aarch64-apple-darwin.tar.gz",
                Some("Lyceum-0.3.3-macos-aarch64.app.tar.gz"),
            ),
            (
                "x86_64-apple-darwin",
                "empyrean-0.3.3-x86_64-apple-darwin.tar.gz",
                Some("Lyceum-0.3.3-macos-x86_64.app.tar.gz"),
            ),
        ] {
            assert_eq!(
                Product::Empyrean.archive_name(triple, v).as_deref(),
                Some(empyrean)
            );
            assert_eq!(Product::Lyceum.archive_name(triple, v).as_deref(), lyceum);
        }
        assert_eq!(
            Product::Empyrean.archive_name("riscv64gc-unknown-linux-gnu", v),
            None
        );
    }

    #[test]
    fn archive_members_cannot_escape_or_name_windows_devices() {
        for bad in [
            "../x", "/x", "a/../x", "C:/x", "a\\b", "a:stream", "NUL.txt", "a/COM1", "a./x", "a//x",
        ] {
            assert!(!safe_member(bad), "{bad}");
        }
        assert!(safe_member("Lyceum.app/Contents/MacOS/lyceum-editor"));
    }

    #[test]
    fn installation_versions_cannot_escape_the_components_folder() {
        for v in ["../x", "1.2.3+foo", "01.2.3", "1.2.3-rc.1"] {
            assert!(install_dir(Path::new("root"), Product::Empyrean, v).is_err());
        }
        assert_eq!(
            install_dir(Path::new("root"), Product::Empyrean, "1.2.3").unwrap(),
            Path::new("root/components/empyrean/1.2.3")
        );
    }
}
