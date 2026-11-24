//! `empyrean-import fetch --world <line>` for a world kept in a repository of its own: one layer's
//! pack, from the release the line follows.
//!
//! ```text
//! empyrean-import fetch --world <line> [--layer <id>] [--version <n> | --latest] [--dir <folder>] [--pack [--out <world.pack>]]
//! ```
//!
//! A line (`empyrean_common::world_release::WORLD_REPOS`) is a repository, a release pinned by its
//! tag and its manifest's SHA-256 (none: its newest published release, checked against the
//! SHA-256 GitHub publishes), and the layer it serves; `--layer` serves another of the world's
//! layers, and `--version` or `--latest` another release, as for the other worlds.
//!
//! The release's `manifest.json` names every layer, its world's dataset id and era, and the release
//! and asset holding its pack ([`crate::world::release`]). `fetch` reads it, refuses a manifest of
//! another world or release, and downloads the one pack the layer names, which may be in an earlier
//! release; the pack must have the manifest's SHA-256, dataset id and era. A layer whose world no
//! release packs is refused: it is built with `world build` from the repository at that release.
//! A pack is cached by its world's dataset id, so layers and releases of one world share the file;
//! a cached pack is used again once its dataset id is checked. `--pack` copies the pack to `--out`
//! (`world.pack` when left out).

use std::path::{Path, PathBuf};

use empyrean_common::world_release::{self, WorldRepo, MANIFEST};

use crate::fetch::{sha256_of, verify, Expected, Release, Version};
use crate::world::release::{Layer, Manifest};

/// The largest pack `fetch` accepts: GitHub's largest asset (an end-of-retail pack is about
/// 110 MB).
const PACK_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// The largest manifest `fetch` accepts.
const MANIFEST_LIMIT: u64 = 16 * 1024 * 1024;

/// `fetch`'s options for a world repository's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoOptions {
    pub line: &'static WorldRepo,
    /// The layer served: the line's own unless `--layer` names another.
    pub layer: String,
    pub version: Version,
    /// Where to cache the pack instead of the per-user cache folder.
    pub dir: Option<PathBuf>,
    /// Where `--pack` copies the pack.
    pub out: Option<PathBuf>,
}

/// Whether `args` (after `fetch`) name a world repository's line with `--world`.
#[must_use]
pub fn names_a_repo(args: &[String]) -> bool {
    args.windows(2)
        .any(|w| w[0] == "--world" && WorldRepo::by_id(&w[1]).is_some())
}

/// Reads the options for a world repository's line (the arguments after `fetch`).
///
/// # Errors
/// An argument the line doesn't take, or a value that doesn't read.
pub fn parse_args(args: &[String]) -> Result<RepoOptions, String> {
    let mut line = None;
    let mut layer = None;
    let mut version = Version::Pinned;
    let mut dir = None;
    let mut pack = false;
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--latest" | "--version" if version != Version::Pinned => {
                return Err("give one of --version and --latest".into());
            }
            "--latest" => version = Version::Latest,
            "--version" => {
                let v = value()?;
                let tag = world_release::parse_tag(&v)
                    .ok_or_else(|| format!("--version wants a release such as 0.1.0, got {v:?}"))?;
                version = Version::Tag(tag);
            }
            "--world" => {
                let v = value()?;
                line = Some(
                    WorldRepo::by_id(&v)
                        .ok_or_else(|| format!("{v:?} is not a world repository's line"))?,
                );
            }
            "--layer" => layer = Some(value()?),
            "--dir" => dir = Some(PathBuf::from(value()?)),
            "--pack" => pack = true,
            "--out" => out = Some(PathBuf::from(value()?)),
            "--era" | "--report" => {
                return Err(format!("{a} does not apply to a world repository's line"));
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let line = line.ok_or("--world names no world repository's line")?;
    if !pack && out.is_some() {
        return Err("--out goes with --pack".into());
    }
    Ok(RepoOptions {
        line,
        layer: layer.unwrap_or_else(|| line.layer.to_owned()),
        version,
        dir,
        out: pack.then(|| out.unwrap_or_else(|| PathBuf::from("world.pack"))),
    })
}

/// What fetching takes from the network: GitHub's answer about a release, and a file's bytes, at
/// most `limit` of them.
pub trait Get {
    /// The text at `url` (a releases API answer).
    ///
    /// # Errors
    /// A request that fails, said for someone who has to act on it.
    fn json(&self, url: &str) -> Result<String, String>;
    /// The bytes at `url`.
    ///
    /// # Errors
    /// A request that fails, or a file over `limit`.
    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}

/// The release's manifest asset from GitHub's releases API answer: its tag, its address and the
/// SHA-256 GitHub publishes for it.
///
/// # Errors
/// An answer that isn't a release, or a release with no manifest.
pub fn parse_manifest_asset(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("GitHub's release answer is not JSON: {e}"))?;
    let raw_tag = v["tag_name"]
        .as_str()
        .ok_or("GitHub's release answer has no tag_name")?;
    let tag = world_release::parse_tag(raw_tag)
        .ok_or_else(|| format!("the release tag {raw_tag:?} is not a version such as v0.1.0"))?;
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(MANIFEST)))
        .ok_or_else(|| format!("release {tag} has no {MANIFEST}"))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| format!("{MANIFEST} has no download address"))?
        .to_owned();
    let expected = match asset["digest"].as_str() {
        Some(d) => {
            let hex = d.strip_prefix("sha256:").unwrap_or(d).to_ascii_lowercase();
            if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!(
                    "{MANIFEST}'s published digest {d:?} is not a SHA-256"
                ));
            }
            Expected::Published(hex)
        }
        None => Expected::Unknown,
    };
    Ok(Release { tag, url, expected })
}

/// Which release's manifest `version` names: the pin needs no API call.
fn manifest_release(line: &WorldRepo, version: &Version, get: &dyn Get) -> Result<Release, String> {
    let pinned = || Release {
        tag: line.pinned_tag.to_owned(),
        url: line.asset_url(line.pinned_tag, MANIFEST),
        expected: Expected::Pinned(line.pinned_manifest_sha256.to_owned()),
    };
    match version {
        Version::Pinned if line.has_pin() => Ok(pinned()),
        Version::Tag(t) if line.has_pin() && t == line.pinned_tag => Ok(pinned()),
        Version::Tag(t) => parse_manifest_asset(&get.json(&line.release_api(Some(t)))?),
        Version::Pinned | Version::Latest => {
            parse_manifest_asset(&get.json(&line.release_api(None))?)
        }
    }
}

/// The layer `id` of `manifest`, or an error listing the layers there are.
///
/// # Errors
/// A layer the world doesn't have.
pub fn layer_of<'m>(manifest: &'m Manifest, id: &str) -> Result<&'m Layer, String> {
    manifest.layers.iter().find(|l| l.id == id).ok_or_else(|| {
        let ids: Vec<&str> = manifest.layers.iter().map(|l| l.id.as_str()).collect();
        let shown = if ids.len() > 6 {
            format!(
                "{}, …, {} ({} layers)",
                ids[..3].join(", "),
                ids[ids.len() - 2..].join(", "),
                ids.len()
            )
        } else {
            ids.join(", ")
        };
        format!(
            "{} {} has no layer {id:?}; its layers are {shown}",
            manifest.world, manifest.release
        )
    })
}

/// Whether the pack at `path` is layer `layer`'s world: its dataset id and era.
fn check_pack(path: &Path, layer: &Layer) -> Result<(), String> {
    let p =
        empyrean_content::pack::Pack::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dataset: String = p
        .header()
        .dataset_id
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if dataset != layer.dataset_id {
        return Err(format!(
            "{} is dataset {dataset}, and layer {} is {}",
            path.display(),
            layer.id,
            layer.dataset_id
        ));
    }
    if p.era().name() != layer.era {
        return Err(format!(
            "{} is of the era {}, and layer {} is of {}",
            path.display(),
            p.era().name(),
            layer.id,
            layer.era
        ));
    }
    Ok(())
}

/// Layer `opts.layer`'s pack, fetched through `get` into the cache (or `--dir`): its path.
///
/// # Errors
/// What [`Get`] refuses; a manifest or a pack that isn't the one it must be; a layer the world
/// doesn't have, or one whose world no release packs; and a cache that can't be written.
pub fn fetch(opts: &RepoOptions, get: &dyn Get) -> Result<PathBuf, String> {
    let dir = match &opts.dir {
        Some(d) => d.clone(),
        None => world_release::cache_dir().ok_or(
            "no per-user cache folder: set LOCALAPPDATA (Windows) or HOME, or pass --dir <folder>",
        )?,
    };
    let line = opts.line;
    let release = manifest_release(line, &opts.version, get)?;
    let bytes = get.bytes(&release.url, MANIFEST_LIMIT)?;
    verify(&release, MANIFEST, &sha256_of(&bytes))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("{MANIFEST} is not text"))?;
    let manifest = Manifest::parse(&text).map_err(|e| format!("{MANIFEST}: {e}"))?;
    if manifest.world != line.key() {
        return Err(format!(
            "{} {}'s {MANIFEST} is of {}",
            line.key(),
            release.tag,
            manifest.world
        ));
    }
    if manifest.release != release.tag {
        return Err(format!(
            "{} {}'s {MANIFEST} is of {}",
            line.key(),
            release.tag,
            manifest.release
        ));
    }
    let layer = layer_of(&manifest, &opts.layer)?;
    let Some(pack) = &layer.pack else {
        return Err(format!(
            "layer {} of {} {} has no pack in any release: build it with `empyrean-import world build \
             --world world.toml --layer {} --out world.pack` from https://github.com/{} at {}",
            layer.id, manifest.world, manifest.release, layer.id, line.repository, manifest.release
        ));
    };
    let dest = dir.join(line.pack_name(&layer.dataset_id));
    if dest.is_file() {
        check_pack(&dest, layer).map_err(|e| {
            format!(
                "the cached pack isn't the layer's: {e}. Delete {} and fetch again",
                dest.display()
            )
        })?;
        println!(
            "layer {} of {} {} is dataset {}, already cached: {}",
            layer.id,
            manifest.world,
            manifest.release,
            layer.dataset_id,
            dest.display()
        );
        return Ok(dest);
    }
    let url = line.asset_url(&pack.release, &pack.asset);
    println!("downloading {url}");
    let bytes = get.bytes(&url, PACK_LIMIT)?;
    let actual = sha256_of(&bytes);
    if actual != pack.sha256 {
        return Err(format!(
            "{} does not match the SHA-256 {MANIFEST} gives: expected {}, downloaded {actual}. Nothing was cached.",
            pack.asset, pack.sha256
        ));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!("{}.part", line.pack_name(&layer.dataset_id)));
    std::fs::write(&part, &bytes).map_err(|e| format!("{}: {e}", part.display()))?;
    drop(bytes);
    if let Err(e) = check_pack(&part, layer) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{e}. Nothing was cached."));
    }
    std::fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    println!(
        "cached layer {} of {} {} (dataset {}, from {}) as {}",
        layer.id,
        manifest.world,
        manifest.release,
        layer.dataset_id,
        pack.release,
        dest.display()
    );
    Ok(dest)
}

/// The network as `fetch` reaches it.
struct Network(ureq::Agent);

impl Network {
    fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .user_agent(concat!("empyrean-import/", env!("CARGO_PKG_VERSION")))
                .timeout_global(Some(std::time::Duration::from_secs(1800)))
                .build()
                .into(),
        )
    }
}

fn said(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(404) => {
            format!("{url} was not found (HTTP 404): is there such a release?")
        }
        ureq::Error::StatusCode(c @ (403 | 429)) => format!(
            "GitHub refused {url} (HTTP {c}), usually its hourly limit on unauthenticated API calls: try again later"
        ),
        ureq::Error::StatusCode(c) => format!("{url} answered HTTP {c}"),
        other => format!(
            "could not reach {url}: {other}. Check the network connection (a proxy is read from HTTPS_PROXY or ALL_PROXY)"
        ),
    }
}

impl Get for Network {
    fn json(&self, url: &str) -> Result<String, String> {
        self.0
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .and_then(|mut r| r.body_mut().read_to_string())
            .map_err(|e| said(url, &e))
    }

    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(limit).read_to_vec())
            .map_err(|e| said(url, &e))
    }
}

/// `empyrean-import fetch --world <a world repository's line> …`.
///
/// # Errors
/// What [`parse_args`] and [`fetch`] refuse, and a pack that can't be copied to `--out`.
pub fn run(args: &[String]) -> Result<(), String> {
    let opts = parse_args(args)?;
    let path = fetch(&opts, &Network::new())?;
    if let Some(out) = &opts.out {
        std::fs::copy(&path, out).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("wrote {}", out.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: a world repository's line, without the network)
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A line serves its own layer unless `--layer` names another, and refuses what applies only
    /// to the other worlds.
    #[test]
    fn a_line_serves_its_layer_unless_another_is_named() {
        let o = parse_args(&args(&["--world", "dereth-eor"])).expect("reads");
        assert_eq!(o.line.id, "dereth-eor");
        assert_eq!(o.layer, "2014-05-may-patch");
        assert_eq!(o.line.key(), "empyrean-world");
        assert_eq!(o.version, Version::Pinned);
        assert_eq!(o.out, None);
        let o = parse_args(&args(&[
            "--world",
            "dereth-launch",
            "--layer",
            "2005-02-infiltration",
            "--version",
            "0.1.0",
            "--pack",
        ]))
        .expect("reads");
        assert_eq!(o.layer, "2005-02-infiltration");
        assert_eq!(o.version, Version::Tag("v0.1.0".into()));
        assert_eq!(o.out, Some(PathBuf::from("world.pack")));
        for bad in [
            &["--world", "dereth-eor", "--era", "eor"][..],
            &["--world", "dereth-eor", "--report", "r.json", "--pack"],
            &["--world", "dereth-eor", "--out", "w.pack"],
            &["--world", "dereth-eor", "--latest", "--version", "0.1.0"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
        assert!(names_a_repo(&args(&["--world", "newdereth-emergence"])));
        assert!(!names_a_repo(&args(&["--world", "patches"])));
        assert!(!names_a_repo(&args(&["--layer", "dereth-eor"])));
    }

    /// The manifest asset of a release from GitHub's answer, with the digest it publishes.
    #[test]
    fn a_releases_manifest_is_its_asset_with_the_digest_github_publishes() {
        let hex = "ab".repeat(32);
        let answer = serde_json::json!({
            "tag_name": "v0.1.0",
            "assets": [
                { "name": "base.pack", "browser_download_url": "https://x/base.pack" },
                { "name": "manifest.json", "browser_download_url": "https://x/manifest.json",
                  "digest": format!("sha256:{hex}") },
            ],
        });
        let r = parse_manifest_asset(&answer.to_string()).expect("reads");
        assert_eq!(r.tag, "v0.1.0");
        assert_eq!(r.url, "https://x/manifest.json");
        assert_eq!(r.expected, Expected::Published(hex));
        let none = serde_json::json!({ "tag_name": "v0.1.0", "assets": [] });
        assert!(parse_manifest_asset(&none.to_string())
            .expect_err("refused")
            .contains("no manifest.json"));
        // The lines address their repositories' releases, and cache by dataset id.
        let l = WorldRepo::by_id("NEWDERETH-EMERGENCE").expect("a line");
        assert_eq!(
            l.asset_url("v0.2.0", "base.pack"),
            "https://github.com/dereth-network/empyrean-world-newdereth/releases/download/v0.2.0/base.pack"
        );
        assert_eq!(
            l.release_api(None),
            "https://api.github.com/repos/dereth-network/empyrean-world-newdereth/releases/latest"
        );
        assert_eq!(l.pack_name("00ff"), "empyrean-world-newdereth-00ff.pack");
    }
}
