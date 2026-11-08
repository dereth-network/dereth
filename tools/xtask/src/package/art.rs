//! Dereth's own art pack source: packed art packs listed in an `art.json`, published on a host of
//! their own ([`URL`]), from which the launcher reads it (`docs/art/sources.md`).
//!
//! ```text
//! cargo xtask package art [--packs <dir>] [--out <dir>] [--guide]
//! ```
//!
//! reads the packed packs in `--packs` (else the folder [`PACKS_VAR`] names; with neither it
//! refuses): every `*.dat` directly in it that is a packed art pack, as `dereth-art pack` writes
//! them. The source's config is the tree's [`TREE_CONFIG`]: it names the source and the files it
//! publishes; a file it does not name is held back. Where it names none, every packed file is
//! published. A [`CONFIG`] in the packs folder is refused, so the tree's is the only one.
//! It may also give, for each file, the `[pack]` listing keys its manifest leaves out (a pack
//! packed before it named its id and version), and the notes and the day of each release.
//!
//! ```toml
//! name = "Dereth"
//! guide = true
//! files_url = "https://files.example.org/art/"
//! # front = "https://example.org/art/"
//!
//! [[pack]]
//! file = "remastered.dat"
//! id = "network.dereth.remastered"
//! version = "1.0.0"
//! license = "CC-BY-4.0"
//! made_for = ["end-of-retail", "1999"]
//! notes = "- First release."
//! released = "2026-11-08"
//! ```
//!
//! The source is written into `<target dir>/package/art/` (or `--out`): `site/` is what the
//! source's host serves, `art.json` and each pack's file at `<id>/<id>-<version>-<hash>.dat`,
//! which the list names relative to itself. The hash is the start of the file's SHA-256, so a
//! file whose bytes change under the same version has a name of its own, and its host keeps each
//! a year as immutable. The files are linked from the packs folder where the disk allows it, and
//! copied where not; each is hashed as it is read, never held whole.
//!
//! A pages host serves no file larger than [`PAGES_FILE_LIMIT`], and a pack runs to 2 GiB. A
//! larger pack's file goes into `upload/` at the same path instead, and the list names it under
//! `files_url`, where it is to be uploaded before the list is deployed; with no `files_url` such
//! a pack is refused.
//!
//! With `--guide` (or `guide = true`), the guide to making an art pack ([`GUIDE`]) is written into
//! `site/` too, as `index.html` and a page beside it for each of its other pages, so the source's
//! front page is the guide.
//!
//! With `front` instead, the source's front page is another site's: `site/_redirects` sends the
//! host's `/` there, so the guide can live with a site of its own while the list and the packs
//! stay on the source's host. A config gives one or the other, never both.
//!
//! `cargo xtask deploy art` puts `site/` on its Cloudflare Pages project (`crate::deploy::art`);
//! the owner runs that by hand.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use dereth_art_catalog::{Catalog, Entry, FILE_NAME};
use dereth_assets::art_pack::About;
use dereth_assets::packed_art;
use sha2::{Digest, Sha256};

use super::markdown;

/// The source's list's address, the packs beside it. The launcher starts with this source.
pub const URL: &str = dereth_art_catalog::DERETH_SOURCE;

/// The source's name when its config gives none.
pub const NAME: &str = "Dereth";

/// The list's file name.
pub const LIST: &str = FILE_NAME;

/// The pages host's redirects, written when the config gives a `front`.
pub const REDIRECTS: &str = "_redirects";

/// The redirects file sending the host's front page to `front`.
#[must_use]
pub fn redirects(front: &str) -> String {
    format!(
        "/ {front} 302
"
    )
}

/// The source's config in the packs folder.
pub const CONFIG: &str = "art-source.toml";

/// Dereth's own source's config, in the tree.
pub const TREE_CONFIG: &str = "dereth/art/source.toml";

/// The tree's [`TREE_CONFIG`], read from the workspace `ws`.
///
/// # Errors
/// It will not read, or is not a config.
pub fn tree_config(ws: &Path) -> Result<Config, String> {
    let path = ws.join(TREE_CONFIG);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    config(&text).map_err(|e| format!("{TREE_CONFIG}: {e}"))
}

/// The variable naming the packs folder when `--packs` is not given.
pub const PACKS_VAR: &str = "DERETH_ART_PACKS";

/// The largest file a pages host serves: a larger pack is uploaded elsewhere ([`CONFIG`]'s
/// `files_url`).
pub const PAGES_FILE_LIMIT: u64 = 25 << 20;

/// The guide to making an art pack, in the tree; its `README.md` is the source's front page.
pub const GUIDE: &str = "docs/art";

/// How many hexadecimal digits of the file's SHA-256 its name carries.
const HASH_LEN: usize = 12;

/// A pack file's line in [`CONFIG`].
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PackConfig {
    file: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    license: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    made_for: Vec<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    released: String,
}

impl PackConfig {
    fn about(&self) -> About {
        About {
            id: self.id.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            authors: self.authors.clone(),
            license: self.license.clone(),
            homepage: self.homepage.clone(),
            made_for: self.made_for.clone(),
        }
    }
}

/// [`CONFIG`], read.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    name: String,
    #[serde(default)]
    guide: bool,
    #[serde(default)]
    files_url: String,
    /// Where the host's front page sends a visitor, instead of the guide.
    #[serde(default)]
    front: String,
    /// The files published; `None` when it names none, and every file is.
    #[serde(default, rename = "pack")]
    packs: Option<Vec<PackConfig>>,
}

/// [`CONFIG`]'s text, read.
///
/// # Errors
/// It is not TOML, or names a key it does not know.
pub fn config(text: &str) -> Result<Config, String> {
    toml::from_str(text).map_err(|e| format!("{CONFIG}: {e}"))
}

/// One file the source serves: its path on the host and where it is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub path: String,
    pub from: PathBuf,
    pub size: u64,
}

/// The source, worked out from a packs folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// `art.json`.
    pub list: String,
    /// The packs the pages host serves beside the list.
    pub site: Vec<Placed>,
    /// The packs too large for the pages host, uploaded under `files_url` at their paths.
    pub upload: Vec<Placed>,
    /// Where those are listed, ending in `/`; empty when none is given.
    pub files_url: String,
    /// The packed files in the folder the config does not name.
    pub held_back: Vec<String>,
    /// Whether the config asks for the guide on the front page.
    pub guide: bool,
    /// Where the front page sends a visitor instead; empty when it serves the guide or nothing.
    pub front: String,
}

/// The size and SHA-256 of the file at `path`, read a piece at a time.
///
/// # Errors
/// The file will not read.
pub fn hash_file(path: &Path) -> Result<(u64, String), String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut size = 0u64;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        size += n as u64;
    }
    let hex = h.finalize().iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    });
    Ok((size, hex))
}

/// The day `time` falls on, `YYYY-MM-DD`, in UTC.
#[must_use]
pub fn day_of(time: std::time::SystemTime) -> String {
    let secs = time
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = i64::try_from(secs / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The packed files directly in `dir`, by name, sorted.
fn packed_files(dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        let is_dat = Path::new(&name)
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("dat"));
        if is_dat && e.path().is_file() {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

/// The source the packs folder `dir` makes, with its config `config` (read from the folder when
/// `None`).
///
/// # Errors
/// Every problem at once: a file the config names that is not in the folder or not a packed art
/// pack, a pack naming no id or version that the config does not give either, two packs of one
/// id, or a pack too large for the pages host with no `files_url`.
pub fn plan(dir: &Path, config: Option<Config>) -> Result<Source, String> {
    let config = match config {
        Some(c) => c,
        None => match std::fs::read_to_string(dir.join(CONFIG)) {
            Ok(text) => self::config(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(format!("{}: {e}", dir.join(CONFIG).display())),
        },
    };
    let found = packed_files(dir)?;
    let mut problems = Vec::new();
    let named: Vec<PackConfig> = match &config.packs {
        Some(packs) => packs.clone(),
        None => found
            .iter()
            .map(|f| PackConfig {
                file: f.clone(),
                ..PackConfig::default()
            })
            .collect(),
    };
    let files_url = match config.files_url.trim() {
        "" => String::new(),
        url if url.starts_with("https://") => format!("{}/", url.trim_end_matches('/')),
        url => {
            problems.push(format!(
                "{CONFIG}: files_url {url:?} is not an https:// address"
            ));
            String::new()
        }
    };
    let front = match config.front.trim() {
        "" => String::new(),
        _ if config.guide => {
            problems.push(format!(
                "{CONFIG}: front and guide both name the front page; give one"
            ));
            String::new()
        }
        url if url.starts_with("https://") => url.to_owned(),
        url => {
            problems.push(format!(
                "{CONFIG}: front {url:?} is not an https:// address"
            ));
            String::new()
        }
    };
    let mut list = Catalog {
        name: if config.name.trim().is_empty() {
            NAME.to_owned()
        } else {
            config.name.trim().to_owned()
        },
        packs: Vec::new(),
    };
    let (mut site, mut upload) = (Vec::new(), Vec::new());
    for p in &named {
        if !found.contains(&p.file) {
            problems.push(format!(
                "{CONFIG}: `{}` is not a packed file in the folder",
                p.file
            ));
            continue;
        }
        let from = dir.join(&p.file);
        let made = packed_art::read_manifest(&from)
            .map_err(|e| format!("{}: {e}", p.file))
            .and_then(|packed| {
                let (size, sha) = hash_file(&from)?;
                let released = if p.released.is_empty() {
                    std::fs::metadata(&from)
                        .and_then(|m| m.modified())
                        .map(day_of)
                        .unwrap_or_default()
                } else {
                    p.released.clone()
                };
                let entry =
                    Entry::for_packed(&packed, &p.about(), "", size, &sha, &p.notes, &released)
                        .map_err(|e| format!("{}: {e}", p.file))?;
                let path = format!(
                    "{0}/{0}-{1}-{2}.dat",
                    entry.id,
                    entry.version,
                    &sha[..HASH_LEN]
                );
                Ok((entry, path, size))
            });
        let (entry, path, size) = match made {
            Ok(m) => m,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        if list.packs.iter().any(|o| o.id == entry.id) {
            problems.push(format!("{}: another pack has the id {}", p.file, entry.id));
            continue;
        }
        let placed = Placed {
            path: path.clone(),
            from,
            size,
        };
        let file = if size > PAGES_FILE_LIMIT {
            if files_url.is_empty() {
                problems.push(format!(
                    "{}: {size} bytes, more than a pages host serves ({PAGES_FILE_LIMIT}); give \
                     {CONFIG} a files_url to upload it to",
                    p.file
                ));
                continue;
            }
            upload.push(placed);
            format!("{files_url}{path}")
        } else {
            site.push(placed);
            path
        };
        list.packs.push(Entry { file, ..entry });
    }
    if !problems.is_empty() {
        return Err(format!("the art pack source:\n  {}", problems.join("\n  ")));
    }
    let held_back = found
        .into_iter()
        .filter(|f| !named.iter().any(|p| p.file == *f))
        .collect();
    Ok(Source {
        list: list.to_json(),
        site,
        upload,
        files_url,
        held_back,
        guide: config.guide,
        front,
    })
}

/// The guide's pages, `README.md` as `index.html` and each other page `<name>.html`, from the
/// Markdown files of the folder `docs` (the tree's [`GUIDE`]).
///
/// # Errors
/// The folder or a page will not read, or holds no `README.md`.
pub fn guide_pages(docs: &Path) -> Result<Vec<(String, String)>, String> {
    let mut pages: BTreeMap<String, String> = BTreeMap::new();
    for e in std::fs::read_dir(docs).map_err(|e| format!("{}: {e}", docs.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", docs.display()))?;
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".md") {
            let text = std::fs::read_to_string(e.path())
                .map_err(|err| format!("{}: {err}", e.path().display()))?;
            pages.insert(stem.to_owned(), text);
        }
    }
    if !pages.contains_key("README") {
        return Err(format!("{}: no README.md", docs.display()));
    }
    let local: Vec<String> = pages.keys().cloned().collect();
    let link = |target: &str| -> String {
        let (path, anchor) = target.split_once('#').map_or((target, ""), |(p, a)| (p, a));
        let page = |stem: &str| {
            if stem == "README" {
                "index.html".to_owned()
            } else {
                format!("{stem}.html")
            }
        };
        let joined = |p: String| {
            if anchor.is_empty() {
                p
            } else {
                format!("{p}#{anchor}")
            }
        };
        if path.is_empty() || path.contains("://") || path.starts_with("mailto:") {
            return target.to_owned();
        }
        if let Some(stem) = path
            .strip_suffix(".md")
            .filter(|s| local.iter().any(|l| l == s))
        {
            return joined(page(stem));
        }
        // A page of the specification outside the guide: the tree's copy of it.
        let mut parts: Vec<&str> = GUIDE.split('/').collect();
        for seg in path.split('/') {
            match seg {
                ".." => {
                    parts.pop();
                }
                "." | "" => {}
                s => parts.push(s),
            }
        }
        joined(format!(
            "{}/blob/main/{}",
            super::DEFAULT_SOURCE_URL,
            parts.join("/")
        ))
    };
    Ok(pages
        .iter()
        .map(|(stem, text)| {
            let file = if stem == "README" {
                "index.html".to_owned()
            } else {
                format!("{stem}.html")
            };
            (file, markdown::page(text, &link))
        })
        .collect())
}

/// Link `from` at `to`, or copy it where the disk will not link.
fn place(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    if std::fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// What [`write`](fn@write) wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub site: PathBuf,
    pub upload: PathBuf,
    pub source: Source,
    pub guide: bool,
}

/// Write the source `source` into `out` (`site/` and `upload/`), replacing what a run before left
/// there, with the guide from `docs` when `guide`.
///
/// # Errors
/// A file will not write, or the guide will not read.
pub fn write(source: Source, out: &Path, docs: &Path, guide: bool) -> Result<Written, String> {
    match std::fs::remove_dir_all(out) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", out.display()))
        }
        _ => {}
    }
    let (site, upload) = (out.join("site"), out.join("upload"));
    std::fs::create_dir_all(&site).map_err(|e| format!("{}: {e}", site.display()))?;
    for p in &source.site {
        place(&p.from, &site.join(&p.path))?;
    }
    for p in &source.upload {
        place(&p.from, &upload.join(&p.path))?;
    }
    super::write(&site.join(LIST), source.list.as_bytes())?;
    if !source.front.is_empty() {
        super::write(&site.join(REDIRECTS), redirects(&source.front).as_bytes())?;
    }
    if guide {
        for (file, html) in guide_pages(docs)? {
            super::write(&site.join(file), html.as_bytes())?;
        }
    }
    Ok(Written {
        site,
        upload,
        source,
        guide,
    })
}

/// What a run says of what it wrote.
#[must_use]
pub fn said(w: &Written) -> String {
    let mut out = format!(
        "Dereth's art pack source: {} packs in {}{}; `cargo xtask deploy art` publishes it at \
         {URL}.",
        w.source.site.len() + w.source.upload.len(),
        w.site.display(),
        if w.guide { ", with the guide" } else { "" }
    );
    if !w.source.upload.is_empty() {
        out.push_str(&format!(
            "\nUpload these to {} at the same paths before the list is deployed (they are in {}):",
            w.source.files_url,
            w.upload.display()
        ));
        for p in &w.source.upload {
            out.push_str(&format!("\n  {} ({} bytes)", p.path, p.size));
        }
    }
    if !w.source.held_back.is_empty() {
        out.push_str(&format!(
            "\nHeld back (not in {CONFIG}): {}",
            w.source.held_back.join(", ")
        ));
    }
    out
}

/// The packs folder: `--packs`, else [`PACKS_VAR`].
///
/// # Errors
/// Neither names one.
pub fn packs_dir(
    flag: Option<PathBuf>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf, String> {
    flag.or_else(|| {
        env(PACKS_VAR)
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
    })
    .ok_or_else(|| {
        format!(
            "name the folder of packed art packs: --packs <dir>, or set {PACKS_VAR}; it holds the \
             files `dereth-art pack` writes, and the tree's {TREE_CONFIG} says which to publish"
        )
    })
}

pub const USAGE: &str = "usage: cargo xtask package art [--packs <dir>] [--out <dir>] [--guide]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    packs: Option<PathBuf>,
    out: Option<PathBuf>,
    guide: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        packs: None,
        out: None,
        guide: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--packs" => o.packs = Some(value()?),
            "--out" => o.out = Some(value()?),
            "--guide" => o.guide = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    Ok(o)
}

/// Build the source from the packs folder `packs` into `out` by the tree's [`TREE_CONFIG`], the
/// guide too when `guide` or the config asks.
///
/// # Errors
/// As [`plan`] and [`write`](fn@write), the tree's config will not read, or the packs folder
/// holds a [`CONFIG`] of its own.
pub fn build(ws: &Path, packs: &Path, out: &Path, guide: bool) -> Result<Written, String> {
    if packs.join(CONFIG).exists() {
        return Err(format!(
            "{}: Dereth's source is configured by the tree's {TREE_CONFIG}; remove this file",
            packs.join(CONFIG).display()
        ));
    }
    let source = plan(packs, Some(tree_config(ws)?))?;
    let guide = guide || source.guide;
    write(source, out, &ws.join(GUIDE), guide)
}

/// `cargo xtask package art ...`.
pub fn package(args: &[String]) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let packs = match packs_dir(o.packs, &|k| std::env::var(k).ok()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let out = o
        .out
        .unwrap_or_else(|| crate::util::target_dir().join("package").join("art"));
    match build(&crate::util::workspace_root(), &packs, &out, o.guide) {
        Ok(w) => {
            println!("{}", said(&w));
            0
        }
        Err(e) => {
            eprintln!("package: FAILED\n{e}");
            1
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
