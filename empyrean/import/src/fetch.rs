//! `empyrean-import fetch`: downloads ACE-World's SQL release from GitHub, verifies it, unzips it
//! and keeps the dump in the per-user cache the tests also look in.
//!
//! ```text
//! empyrean-import fetch [--version <n> | --latest] [--dir <folder>]
//!             [--pack [--out <world.pack>] [--report <report.json>]]
//! ```
//!
//! - With no version it fetches the pinned release (`empyrean_common::world_release`) from its
//!   stable download address and checks the zip against the committed SHA-256.
//! - `--version <n>` (`0.9.294` or `v0.9.294`) fetches another release; `--latest` the newest.
//!   Both ask GitHub's releases API for the asset, and check the zip against the SHA-256 GitHub
//!   publishes for it. `--latest` prints the tag and the SHA-256, which are the two constants a pin
//!   bump changes.
//! - A SHA-256 that does not match fails the fetch, and nothing is cached.
//! - `--dir` caches somewhere else than the per-user cache folder.
//! - A release already in the cache is not downloaded again.
//! - `--pack` then builds `world.pack` from the dump, as `--sql <dump> --out <world.pack>` does.
//!
//! The zip is held in memory (about 20 MB); the dump (about 150 MB) is written beside its final
//! name and renamed into place only when it is complete, so the cache never holds a partial dump.

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use empyrean_common::world_release::{self, PINNED_TAG, PINNED_ZIP_SHA256, REPOSITORY};
use sha2::{Digest, Sha256};

/// Which release to fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    /// The release this build pins.
    Pinned,
    /// A release named by its tag.
    Tag(String),
    /// The newest release.
    Latest,
}

/// `fetch`'s options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub version: Version,
    /// Where to cache the dump instead of the per-user cache folder.
    pub dir: Option<PathBuf>,
    /// Build a pack from the dump: its path and its report's.
    pub pack: Option<(PathBuf, Option<PathBuf>)>,
}

/// Reads `fetch`'s options (the arguments after `fetch`).
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut version = Version::Pinned;
    let mut dir = None;
    let mut pack = false;
    let mut out = None;
    let mut report = None;
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
                let tag = world_release::parse_tag(&v).ok_or_else(|| {
                    format!("--version wants a release such as 0.9.295, got {v:?}")
                })?;
                version = Version::Tag(tag);
            }
            "--dir" => dir = Some(PathBuf::from(value()?)),
            "--pack" => pack = true,
            "--out" => out = Some(PathBuf::from(value()?)),
            "--report" => report = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if !pack && (out.is_some() || report.is_some()) {
        return Err("--out and --report go with --pack".into());
    }
    let pack = pack.then(|| (out.unwrap_or_else(|| PathBuf::from("world.pack")), report));
    Ok(Options { version, dir, pack })
}

/// The SHA-256 a downloaded zip must have, and where that value comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    /// The value committed with the pin.
    Pinned(String),
    /// The value GitHub publishes for the asset.
    Published(String),
    /// GitHub publishes none for this asset.
    Unknown,
}

impl Expected {
    fn hex(&self) -> Option<&str> {
        match self {
            Self::Pinned(h) | Self::Published(h) => Some(h),
            Self::Unknown => None,
        }
    }
}

/// A release's SQL asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub expected: Expected,
}

/// The pinned release: its stable download address and the committed SHA-256, with no API call.
#[must_use]
pub fn pinned() -> Release {
    Release {
        tag: PINNED_TAG.to_owned(),
        url: world_release::download_url(PINNED_TAG),
        expected: Expected::Pinned(PINNED_ZIP_SHA256.to_owned()),
    }
}

/// A SHA-256 as lowercase hex, from GitHub's `sha256:<hex>` digest form or bare hex.
fn sha256_hex(s: &str) -> Option<String> {
    let hex = s.strip_prefix("sha256:").unwrap_or(s).to_ascii_lowercase();
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// The SQL asset of one release from GitHub's releases API response (`releases/latest` or
/// `releases/tags/<tag>`).
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("GitHub's release answer is not JSON: {e}"))?;
    let raw_tag = v["tag_name"]
        .as_str()
        .ok_or("GitHub's release answer has no tag_name")?;
    let tag = world_release::parse_tag(raw_tag)
        .ok_or_else(|| format!("the release tag {raw_tag:?} is not a version such as v0.9.295"))?;
    let name = world_release::zip_name(&tag);
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(name.as_str())))
        .ok_or_else(|| format!("release {tag} has no asset named {name}"))?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| format!("{name} has no download address"))?
        .to_owned();
    let expected = match asset["digest"].as_str() {
        Some(d) => Expected::Published(
            sha256_hex(d)
                .ok_or_else(|| format!("{name}'s published digest {d:?} is not a SHA-256"))?,
        ),
        None => Expected::Unknown,
    };
    Ok(Release { tag, url, expected })
}

/// The zip's SHA-256, lowercase hex.
#[must_use]
pub fn sha256_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Checks the zip against what it must be. A mismatch is an error naming both values.
pub fn verify(release: &Release, actual: &str) -> Result<(), String> {
    let Some(want) = release.expected.hex() else {
        return Ok(());
    };
    if want.eq_ignore_ascii_case(actual) {
        return Ok(());
    }
    let whose = match release.expected {
        Expected::Pinned(_) => "the SHA-256 committed with the pin",
        _ => "the SHA-256 GitHub publishes",
    };
    Err(format!(
        "{} does not match {whose}: expected {want}, downloaded {actual}. Nothing was cached.",
        world_release::zip_name(&release.tag)
    ))
}

/// Unzips the dump `sql_name` (or the archive's only `.sql` file) into `dir` as `sql_name`,
/// writing beside it first and renaming it into place when complete. Returns its path.
pub fn unzip_sql<R: Read + Seek>(zip: R, sql_name: &str, dir: &Path) -> Result<PathBuf, String> {
    let mut archive =
        zip::ZipArchive::new(zip).map_err(|e| format!("the download is not a zip: {e}"))?;
    let mut sql_entries = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;
        let base = entry
            .name()
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("")
            .to_owned();
        if entry.is_file() && base.to_ascii_lowercase().ends_with(".sql") {
            sql_entries.push((i, base));
        }
    }
    let index = match sql_entries.iter().find(|(_, b)| b == sql_name) {
        Some((i, _)) => *i,
        None if sql_entries.len() == 1 => sql_entries[0].0,
        None => {
            let names: Vec<&str> = sql_entries.iter().map(|(_, b)| b.as_str()).collect();
            return Err(format!(
                "the zip holds no {sql_name} (its .sql files: {names:?})"
            ));
        }
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(sql_name);
    let partial = dir.join(format!("{sql_name}.partial"));
    let written = (|| -> std::io::Result<()> {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut file = std::io::BufWriter::new(std::fs::File::create(&partial)?);
        std::io::copy(&mut entry, &mut file)?;
        file.flush()?;
        file.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        std::fs::rename(&partial, &dest)
    })();
    if let Err(e) = written {
        std::fs::remove_file(&partial).ok();
        return Err(format!("unzipping {sql_name} into {}: {e}", dir.display()));
    }
    Ok(dest)
}

/// The largest zip `fetch` accepts (the releases are about 20 MB).
const ZIP_LIMIT: u64 = 512 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("empyrean-import/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}

/// A failed request, said for someone who has to act on it.
fn http_error(what: &str, url: &str, e: &ureq::Error) -> String {
    let why = match e {
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
    };
    format!(
        "{what}: {why}\n  By hand: download the release's ACE-World-Database-<tag>.sql.zip from \
         https://github.com/{REPOSITORY}/releases, unzip it, and build with --sql <dump> --out world.pack"
    )
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    agent
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| http_error("asking GitHub for the release", url, &e))
}

fn get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    agent
        .get(url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(ZIP_LIMIT).read_to_vec())
        .map_err(|e| http_error("downloading the release", url, &e))
}

/// Which release `version` names: the pin needs no network; the others ask GitHub's API.
fn resolve(agent: &ureq::Agent, version: &Version) -> Result<Release, String> {
    let api = format!("https://api.github.com/repos/{REPOSITORY}/releases");
    match version {
        Version::Pinned => Ok(pinned()),
        Version::Tag(t) if t == PINNED_TAG => Ok(pinned()),
        Version::Tag(t) => parse_release(&get_json(agent, &format!("{api}/tags/{t}"))?),
        Version::Latest => parse_release(&get_json(agent, &format!("{api}/latest"))?),
    }
}

/// Fetches the release into the cache (or `--dir`), and returns the dump's path. For `--latest`
/// it also prints the tag and the zip's SHA-256.
pub fn fetch(opts: &Options) -> Result<PathBuf, String> {
    let dir = match &opts.dir {
        Some(d) => d.clone(),
        None => world_release::cache_dir().ok_or(
            "no per-user cache folder: set LOCALAPPDATA (Windows) or HOME, or pass --dir <folder>",
        )?,
    };
    let agent = agent();
    let release = resolve(&agent, &opts.version)?;
    let sql_name = world_release::sql_name(&release.tag);
    let dest = dir.join(&sql_name);
    let sha = if dest.is_file() {
        println!("{} is already cached: {}", release.tag, dest.display());
        release.expected.hex().map(str::to_owned)
    } else {
        println!("downloading {}", release.url);
        let t0 = std::time::Instant::now();
        let zip = get_bytes(&agent, &release.url)?;
        let actual = sha256_of(&zip);
        verify(&release, &actual)?;
        match &release.expected {
            Expected::Pinned(_) => println!("zip SHA-256 {actual} matches the pin"),
            Expected::Published(_) => println!("zip SHA-256 {actual} matches GitHub's digest"),
            Expected::Unknown => println!(
                "warning: GitHub publishes no digest for this asset; its SHA-256 is {actual} (not verified)"
            ),
        }
        let path = unzip_sql(std::io::Cursor::new(zip), &sql_name, &dir)?;
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        println!(
            "cached {} ({len} bytes) in {:.1}s",
            path.display(),
            t0.elapsed().as_secs_f64()
        );
        Some(actual)
    };
    if opts.version == Version::Latest {
        let sha = sha.as_deref().unwrap_or("(not published)");
        println!("latest release: {}", release.tag);
        println!("zip SHA-256:    {sha}");
        if release.tag == PINNED_TAG {
            println!("this is the pinned release");
        } else {
            println!(
                "to pin it, set PINNED_TAG = \"{}\" and PINNED_ZIP_SHA256 = \"{sha}\" in empyrean/crates/common/src/world_release.rs",
                release.tag
            );
        }
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: the world-database fetch, without the network)
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("empyrean-fetch-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, body) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(body).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn the_options_name_a_version_a_folder_and_a_pack() {
        assert_eq!(
            parse_args(&[]).unwrap(),
            Options {
                version: Version::Pinned,
                dir: None,
                pack: None
            }
        );
        assert_eq!(
            parse_args(&args(&["--version", "0.9.294", "--dir", "d"])).unwrap(),
            Options {
                version: Version::Tag("v0.9.294".into()),
                dir: Some("d".into()),
                pack: None
            }
        );
        assert_eq!(
            parse_args(&args(&["--latest", "--pack"])).unwrap().pack,
            Some(("world.pack".into(), None))
        );
        assert_eq!(
            parse_args(&args(&["--pack", "--out", "w.pack", "--report", "r.json"]))
                .unwrap()
                .pack,
            Some(("w.pack".into(), Some("r.json".into())))
        );
        for bad in [
            &["--version", "latest"][..],
            &["--version"],
            &["--latest", "--version", "0.9.1"],
            &["--version", "0.9.1", "--latest"],
            &["--out", "w.pack"],
            &["--sql", "x"],
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_pin_needs_no_api_call_and_carries_the_committed_hash() {
        let p = pinned();
        assert_eq!(p.tag, PINNED_TAG);
        assert_eq!(p.url, world_release::download_url(PINNED_TAG));
        assert_eq!(p.expected, Expected::Pinned(PINNED_ZIP_SHA256.into()));
    }

    #[test]
    fn a_release_answer_gives_the_sql_asset_and_its_published_digest() {
        let json = r#"{
            "tag_name": "v0.9.295",
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://x/notes.txt"},
                {"name": "ACE-World-Database-v0.9.295.sql.zip",
                 "browser_download_url": "https://x/ACE-World-Database-v0.9.295.sql.zip",
                 "digest": "sha256:FD35CFF8B2CEA8408AE13839B9B1862C30452A1013F43EB62EC83C25349BD148"}
            ]
        }"#;
        assert_eq!(
            parse_release(json).unwrap(),
            Release {
                tag: "v0.9.295".into(),
                url: "https://x/ACE-World-Database-v0.9.295.sql.zip".into(),
                expected: Expected::Published(PINNED_ZIP_SHA256.into()),
            }
        );
        let undigested = json.replace(r#""digest""#, r#""other""#);
        assert_eq!(
            parse_release(&undigested).unwrap().expected,
            Expected::Unknown
        );
        for bad in [
            r#"{"tag_name": "nightly", "assets": []}"#,
            r#"{"tag_name": "v0.9.296", "assets": [{"name": "other.zip", "browser_download_url": "u"}]}"#,
            r#"{"assets": []}"#,
            r#"{"tag_name": "v1.0.0", "assets": [{"name": "ACE-World-Database-v1.0.0.sql.zip", "browser_download_url": "u", "digest": "md5:abc"}]}"#,
            "<html>",
        ] {
            assert!(parse_release(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_zip_whose_sha256_differs_from_the_expected_one_is_refused() {
        let body = b"not the release";
        let actual = sha256_of(body);
        assert_eq!(
            sha256_of(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut r = pinned();
        let err = verify(&r, &actual).unwrap_err();
        assert!(
            err.contains(PINNED_ZIP_SHA256) && err.contains(&actual),
            "{err}"
        );
        r.expected = Expected::Pinned(actual.to_ascii_uppercase());
        assert!(verify(&r, &actual).is_ok(), "hex case does not matter");
        r.expected = Expected::Published(PINNED_ZIP_SHA256.into());
        assert!(verify(&r, &actual)
            .unwrap_err()
            .contains("GitHub publishes"));
        r.expected = Expected::Unknown;
        assert!(verify(&r, &actual).is_ok());
    }

    #[test]
    fn the_dump_is_unzipped_into_the_cache_under_its_release_name() {
        let dir = scratch("unzip");
        let zip = zip_of(&[
            ("readme.txt", b"hello"),
            (
                "ACE-World-Database-v0.9.295.sql",
                b"INSERT INTO `weenie` VALUES (1);\n",
            ),
        ]);
        let name = world_release::sql_name("v0.9.295");
        let path = unzip_sql(std::io::Cursor::new(zip), &name, &dir).unwrap();
        assert_eq!(path, dir.join(&name));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"INSERT INTO `weenie` VALUES (1);\n"
        );
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            left,
            [std::ffi::OsString::from(&name)],
            "only the dump is left"
        );

        // An archive with one differently named dump (in a folder) still gives it, under the
        // release's name.
        let other = zip_of(&[("db/world.sql", b"-- x\n")]);
        let path = unzip_sql(
            std::io::Cursor::new(other),
            "ACE-World-Database-v1.2.3.sql",
            &dir,
        )
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"-- x\n");

        // No dump, or two that are not the release's, is an error and caches nothing.
        let before = std::fs::read_dir(&dir).unwrap().count();
        for bad in [
            zip_of(&[("readme.txt", b"hello")]),
            zip_of(&[("a.sql", b"1"), ("b.sql", b"2")]),
            b"not a zip".to_vec(),
        ] {
            assert!(unzip_sql(
                std::io::Cursor::new(bad),
                "ACE-World-Database-v9.9.9.sql",
                &dir
            )
            .is_err());
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_request_names_the_address_and_the_way_to_do_it_by_hand() {
        let url = "https://example.invalid/x.zip";
        let offline = http_error("downloading the release", url, &ureq::Error::HostNotFound);
        assert!(
            offline.contains(url) && offline.contains("network"),
            "{offline}"
        );
        assert!(offline.contains("--sql"), "{offline}");
        let missing = http_error(
            "downloading the release",
            url,
            &ureq::Error::StatusCode(404),
        );
        assert!(missing.contains("404"), "{missing}");
        let limited = http_error(
            "asking GitHub for the release",
            url,
            &ureq::Error::StatusCode(403),
        );
        assert!(
            limited.contains("403") && limited.contains("limit"),
            "{limited}"
        );
    }
}
