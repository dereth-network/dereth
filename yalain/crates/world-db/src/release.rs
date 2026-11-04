//! A database's releases: the release this server build is tested against, where `yalain world
//! fetch` keeps what it downloads, and the release archive itself.
//!
//! A release is a tag `vX.Y.Z` of the database's repository and one archive,
//! `yalain-world-<tag>.zip`: the world file, the lockfile, every pack the world names, the licence
//! and readme, and `manifest.json` (the tag, the format, the era, rows by kind and provenance).
//! The archive is made by [`archive`] and is the same bytes for the same files: entries sorted,
//! stored uncompressed, with a fixed time, so anyone can rebuild a release and check its SHA-256.
//! Bumping the pin is the two constants below.
//!
//! The cache is a per-user folder, outside any checkout:
//!
//! | host | folder |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\Yalain\world-database` |
//! | macOS | `~/Library/Caches/Yalain/world-database` |
//! | Linux and other Unix | `$XDG_CACHE_HOME/yalain/world-database`, else `~/.cache/yalain/world-database` |
//!
//! It holds one folder per fetched release, named by its tag.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::rows::{Database, Row};
use crate::Files;

/// The release this server build is tested against.
pub const PINNED_TAG: &str = "v0.30.0";

/// The SHA-256 of [`PINNED_TAG`]'s archive, lowercase hex.
pub const PINNED_ZIP_SHA256: &str =
    "f1cdce8fd4755712257310de93958fa7caeb106eb5b0d73b78cf56a42b44e57b";

/// Where releases are downloaded from: `{tag}` stands for the tag. `yalain world fetch --from`
/// and the `YALAIN_WORLD_URL` variable give another address (a mirror, a local file).
pub const DEFAULT_SOURCE: &str =
    "https://github.com/dereth-network/yalain-world/releases/download/{tag}/yalain-world-{tag}.zip";

/// The release's archive: `yalain-world-<tag>.zip`.
#[must_use]
pub fn zip_name(tag: &str) -> String {
    format!("yalain-world-{tag}.zip")
}

/// A source address with the tag put in.
#[must_use]
pub fn source_url(template: &str, tag: &str) -> String {
    template.replace("{tag}", tag)
}

/// A release tag from what a user types: `0.1.0` or `v0.1.0` (three dot-separated numbers) gives
/// `v0.1.0`; anything else is `None`.
#[must_use]
pub fn parse_tag(s: &str) -> Option<String> {
    let s = s.trim();
    let digits = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = digits.split('.').collect();
    let numeric = |p: &&str| !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit());
    (parts.len() == 3 && parts.iter().all(numeric)).then(|| format!("v{digits}"))
}

/// A tag's three numbers, for putting releases in order; `None` when it is not a tag.
#[must_use]
pub fn tag_numbers(tag: &str) -> Option<[u32; 3]> {
    let tag = parse_tag(tag)?;
    let mut n = tag[1..].split('.').map(|p| p.parse::<u32>().ok());
    Some([n.next()??, n.next()??, n.next()??])
}

/// The release notes written beside the archive: `yalain-world-<tag>.notes.md`.
#[must_use]
pub fn notes_name(tag: &str) -> String {
    format!("yalain-world-{tag}.notes.md")
}

/// The release notes of `tag`, from the database's change log (`CHANGES.md`: a `## vX.Y.Z`
/// section for each version, other sections ignored): the tag's own section or, with `since`,
/// every section after that release up to the tag, newest first (sections of list items as one
/// list); then what the archive holds and its SHA-256.
///
/// # Errors
/// The change log has no section for the tag, or `since` is not an earlier release.
pub fn notes(
    changes: &str,
    tag: &str,
    since: Option<&str>,
    sha256: &str,
) -> Result<String, String> {
    let want = tag_numbers(tag).ok_or_else(|| format!("{tag} is not a release tag"))?;
    let after = match since {
        Some(s) => {
            let n = tag_numbers(s).ok_or_else(|| format!("{s} is not a release tag"))?;
            if n >= want {
                return Err(format!("{s} is not a release before {tag}"));
            }
            Some(n)
        }
        None => None,
    };
    let wanted = |v: &[u32; 3]| after.map_or(*v == want, |a| a < *v && *v <= want);
    let mut sections: Vec<([u32; 3], Vec<&str>)> = Vec::new();
    let mut current: Option<usize> = None;
    for line in changes.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            current = tag_numbers(heading.trim()).filter(wanted).map(|v| {
                sections.push((v, Vec::new()));
                sections.len() - 1
            });
        } else if line.starts_with("# ") {
            current = None;
        } else if let Some(i) = current {
            sections[i].1.push(line.trim_end());
        }
    }
    let body = |lines: &[&str]| {
        let start = lines
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map_or(start, |e| e + 1);
        lines[start..end].join("\n")
    };
    if !sections
        .iter()
        .any(|(v, l)| *v == want && !body(l).is_empty())
    {
        return Err(format!("CHANGES.md has no section for {tag}"));
    }
    sections.sort_by_key(|s| std::cmp::Reverse(s.0));
    let parts: Vec<String> = sections
        .iter()
        .map(|(_, l)| body(l))
        .filter(|b| !b.is_empty())
        .collect();
    // Sections of list items run together as one list; anything else stays a paragraph apart.
    let list = |b: &str| {
        b.lines()
            .all(|l| l.starts_with("- ") || l.starts_with("  "))
    };
    let mut out = String::new();
    if let Some(s) = since {
        out.push_str(&format!("Changes since {s}:\n\n"));
    }
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(if list(&parts[i - 1]) && list(part) {
                "\n"
            } else {
                "\n\n"
            });
        }
        out.push_str(part);
    }
    out.push_str(&format!(
        "\n\n## Download\n\n`{zip}` holds the database: `world.toml`, `world.lock`, its packs and \
         a `manifest.json` counting rows by kind and provenance. It holds no game data: its rows \
         reference your own Asheron's Call 2 install by data id. `yalain world fetch --version \
         {tag}` downloads and checks it.\n\nSHA-256: `{sha256}`\n",
        zip = zip_name(tag)
    ));
    Ok(out)
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
/// unset, empty or not an absolute path counts as absent).
#[must_use]
pub fn cache_dir_for(host: Host, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = match host {
        Host::Windows => var("LOCALAPPDATA")?.join("Yalain"),
        Host::MacOs => var("HOME")?.join("Library/Caches/Yalain"),
        Host::Unix => var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("yalain"),
    };
    Some(base.join("world-database"))
}

/// A SHA-256 as lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The files a release carries: the world file, the lockfile, the licence and readme when
/// present, and every file of every pack the world names.
#[must_use]
pub fn release_files(files: &Files, db: &Database) -> Files {
    let mut out = Files::new();
    for top in ["world.toml", "world.lock", "LICENSE", "README.md"] {
        if let Some(t) = files.get(top) {
            out.insert(top.to_string(), t.clone());
        }
    }
    for p in &db.packs {
        let prefix = format!("{}/", p.folder);
        for (k, v) in files.range(prefix.clone()..) {
            if !k.starts_with(&prefix) {
                break;
            }
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

/// The release's `manifest.json`: the tag, the format and era, the packs, rows by kind, and rows
/// by provenance (`capture` by confidence).
#[must_use]
pub fn manifest(tag: &str, db: &Database) -> String {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut prov: BTreeMap<String, usize> = BTreeMap::new();
    for l in db.rows() {
        *kinds.entry(l.row.kind()).or_default() += 1;
        if let Row::Remove(_) = l.row {
            continue;
        }
        if let Some(p) = l.row.prov() {
            let k = match (p.source, p.confidence) {
                (crate::rows::Source::Capture, Some(c)) => format!("capture {}", c.word()),
                (f, _) => f.word().to_string(),
            };
            *prov.entry(k).or_default() += 1;
        }
    }
    let v = serde_json::json!({
        "database": db.world.key,
        "tag": tag,
        "game": db.world.game,
        "era": db.world.era,
        "format_version": db.world.format_version,
        "packs": db.packs.iter().map(|p| serde_json::json!({
            "name": p.name, "version": p.version, "license": p.license,
        })).collect::<Vec<_>>(),
        "rows": kinds,
        "provenance": prov,
    });
    let mut s = serde_json::to_string_pretty(&v).unwrap_or_default();
    s.push('\n');
    s
}

/// The release archive of `files` (paths relative to the database's folder) with `manifest.json`
/// added: a zip of stored entries in path order, each dated 1980-01-01 00:00, so the same files
/// always give the same bytes.
#[must_use]
pub fn archive(files: &Files, manifest: &str) -> Vec<u8> {
    let mut entries: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_bytes()))
        .collect();
    entries.push(("manifest.json", manifest.as_bytes()));
    entries.sort_by(|a, b| a.0.cmp(b.0));
    stored_zip(&entries)
}

/// The DOS date of 1980-01-01 and the time 00:00.
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[allow(clippy::cast_possible_truncation)]
fn stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let n = name.len() as u16;
        // Local file header: version 2.0, UTF-8 names (bit 11), stored.
        le32(&mut out, 0x0403_4B50);
        le16(&mut out, 20);
        le16(&mut out, 0x0800);
        le16(&mut out, 0);
        le16(&mut out, DOS_TIME);
        le16(&mut out, DOS_DATE);
        le32(&mut out, crc);
        le32(&mut out, size);
        le32(&mut out, size);
        le16(&mut out, n);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // Central directory entry: made by version 2.0, the same fields, no attributes.
        le32(&mut central, 0x0201_4B50);
        le16(&mut central, 20);
        le16(&mut central, 20);
        le16(&mut central, 0x0800);
        le16(&mut central, 0);
        le16(&mut central, DOS_TIME);
        le16(&mut central, DOS_DATE);
        le32(&mut central, crc);
        le32(&mut central, size);
        le32(&mut central, size);
        le16(&mut central, n);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, 0);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    let count = entries.len() as u16;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4B50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, count);
    le16(&mut out, count);
    le32(&mut out, central.len() as u32);
    le32(&mut out, start);
    le16(&mut out, 0);
    out
}

/// The files of an archive made by [`archive`] (stored entries), by path; `None` when it is not
/// one. Archives of other shapes are read by the caller's zip reader.
#[must_use]
pub fn read_stored(zip: &[u8]) -> Option<Files> {
    let mut out = Files::new();
    let mut p = 0usize;
    let u16_at = |p: usize| zip.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |p: usize| {
        zip.get(p..p + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    while u32_at(p)? == 0x0403_4B50 {
        if u16_at(p + 8)? != 0 {
            return None;
        }
        let crc = u32_at(p + 14)?;
        let size = usize::try_from(u32_at(p + 18)?).ok()?;
        let n = usize::from(u16_at(p + 26)?);
        let extra = usize::from(u16_at(p + 28)?);
        let name = std::str::from_utf8(zip.get(p + 30..p + 30 + n)?).ok()?;
        let start = p + 30 + n + extra;
        let data = zip.get(start..start + size)?;
        if crc32(data) != crc {
            return None;
        }
        out.insert(name.to_string(), String::from_utf8(data.to_vec()).ok()?);
        p = start + size;
    }
    // The entries end where the central directory (or, for none, the end record) begins.
    let next = u32_at(p)?;
    (next == 0x0201_4B50 || (p == 0 && next == 0x0605_4B50)).then_some(out)
}
