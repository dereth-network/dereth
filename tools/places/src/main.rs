//! Builds a world's portal table and places for an era from the era's public world database,
//! fetching and caching the database first.
//!
//! `dereth-places` builds them the way a launcher builds them for a world it joins.
//!
//! **Depends on** `dereth-world-places` (the documents, the dump reader and where each era's
//! database is published) and `dereth-primitives`. **Used by** nothing: it is a program.
//!
//! **Must never** write anywhere but the cache folder and the folder it is given.
//!
//! ```text
//! dereth-places build --era <launch|infiltration|eor> [--sql <dump.sql | release.zip>] [--cache <folder>]
//!                     [--out <folder>] [--world <name>] [--source hint|published]
//! dereth-places fetch --era <era> [--cache <folder>]
//! ```
//!
//! - `fetch` downloads the era's database ([`dereth_world_places::eras`]), checks the zip's
//!   SHA-256 (the pinned one, or the one the release lists), and keeps its SQL in the cache. A
//!   release already in the cache is not downloaded again. The Launch era follows its newest
//!   release.
//! - `build` reads the era's database (the cached copy, fetched first when it is not there, or the
//!   dump or release zip `--sql` names) and writes `portals.tsv` and `places.tsv` into `--out`
//!   (default `places-<era>` in the current folder). `--world` names the world the documents are
//!   of (default `Dereth`); `--source` says where their knowledge came from (default `hint`: a
//!   world built from the public database may have changed it).
//! - The cache is the per-user folder that Empyrean's import tool keeps the same databases in,
//!   under the same names: `%LOCALAPPDATA%\Empyrean\world-database` on Windows,
//!   `~/Library/Caches/Empyrean/world-database` on macOS, and
//!   `$XDG_CACHE_HOME/empyrean/world-database` (else `~/.cache/empyrean/world-database`)
//!   elsewhere. `--cache` names another.
//!
//! It prints how many portals, places and towns it wrote. Exit status 0 is success, 2 a usage or
//! input error.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use dereth_primitives::EraId;
use dereth_world_places::eras::{EraDatabase, Form};
use dereth_world_places::{sql, Source, WorldPlaces};
use sha2::{Digest, Sha256};

/// The largest zip fetched (the releases are about 20 MB).
const ZIP_LIMIT: u64 = 512 * 1024 * 1024;

fn usage() -> ExitCode {
    eprintln!(
        "usage: dereth-places build --era launch|infiltration|eor [--sql <dump.sql | release.zip>] [--cache <folder>]\n\
         \x20                          [--out <folder>] [--world <name>] [--source hint|published]\n\
         \x20      dereth-places fetch --era launch|infiltration|eor [--cache <folder>]"
    );
    ExitCode::from(2)
}

#[derive(Debug, Default)]
struct Options {
    era: Option<EraId>,
    sql: Option<PathBuf>,
    cache: Option<PathBuf>,
    out: Option<PathBuf>,
    world: Option<String>,
    source: Option<Source>,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--era" => {
                let v = value()?;
                o.era = Some(EraId::parse(&v).ok_or_else(|| {
                    format!("--era wants launch, infiltration or eor, got {v:?}")
                })?);
            }
            "--sql" => o.sql = Some(PathBuf::from(value()?)),
            "--cache" => o.cache = Some(PathBuf::from(value()?)),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--world" => o.world = Some(value()?),
            "--source" => {
                o.source = Some(match value()?.as_str() {
                    "hint" => Source::Hint,
                    "published" => Source::Published,
                    other => {
                        return Err(format!("--source wants hint or published, got {other:?}"))
                    }
                });
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if o.era.is_none() {
        return Err("--era is required".into());
    }
    Ok(o)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        return usage();
    };
    let options = match parse(rest) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("dereth-places: {e}");
            return usage();
        }
    };
    let result = match command.as_str() {
        "fetch" => fetch(&options).map(|path| println!("cached {}", path.display())),
        "build" => build(&options),
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dereth-places: {e}");
            ExitCode::from(2)
        }
    }
}

/// The per-user cache folder, from the environment.
fn cache_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    if cfg!(windows) {
        var("LOCALAPPDATA").map(|d| d.join("Empyrean").join("world-database"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Caches/Empyrean/world-database"))
    } else {
        var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))
            .map(|d| d.join("empyrean").join("world-database"))
    }
}

fn cache(o: &Options) -> Result<PathBuf, String> {
    o.cache
        .clone()
        .or_else(cache_dir)
        .ok_or_else(|| "no per-user cache folder (set LOCALAPPDATA, HOME or --cache)".to_owned())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(concat!("dereth-places/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .into()
}

fn http(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(c @ (403 | 429)) => format!(
            "GitHub refused {url} (HTTP {c}), usually its hourly limit on unauthenticated API calls: try again later"
        ),
        ureq::Error::StatusCode(c) => format!("{url} answered HTTP {c}"),
        other => format!("could not reach {url}: {other}"),
    }
}

/// A release to fetch: its tag, its zip's address and the SHA-256 the zip must have.
struct Release {
    tag: String,
    url: String,
    sha256: String,
}

/// The release of `db` to fetch: the pinned one, or the newest the release list names.
fn release(db: &EraDatabase, agent: &ureq::Agent) -> Result<Release, String> {
    if db.pinned() {
        return Ok(Release {
            tag: db.tag.to_owned(),
            url: db.download_url(db.tag),
            sha256: db.zip_sha256.to_owned(),
        });
    }
    let url = db.latest_url();
    let text = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| http(&url, &e))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{url}: not the answer expected: {e}"))?;
    let tag = v["tag_name"]
        .as_str()
        .ok_or_else(|| format!("{url}: the release has no tag"))?
        .to_owned();
    let asset = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| db.is_world_asset(n, &tag))
        })
        .ok_or_else(|| format!("release {tag} of {} has no world zip", db.repository))?;
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| format!("release {tag} lists no SHA-256 for its world zip"))?
        .to_ascii_lowercase();
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| format!("release {tag}'s world zip has no address"))?
        .to_owned();
    Ok(Release { tag, url, sha256 })
}

/// The SQL of `db`'s release `tag` in `dir`, when a fetch has kept it there.
fn cached(db: &EraDatabase, dir: &Path, tag: &str) -> Option<PathBuf> {
    Some(dir.join(db.sql_name(tag))).filter(|p| p.is_file())
}

/// Fetches the era's database into the cache, unless it is there; its path.
fn fetch(o: &Options) -> Result<PathBuf, String> {
    let db = EraDatabase::of(o.era.unwrap_or_default());
    let dir = cache(o)?;
    if db.pinned() {
        if let Some(path) = cached(db, &dir, db.tag) {
            return Ok(path);
        }
    }
    let agent = agent();
    let r = release(db, &agent)?;
    if let Some(path) = cached(db, &dir, &r.tag) {
        return Ok(path);
    }
    let t0 = Instant::now();
    println!("downloading {}", r.url);
    let zip = agent
        .get(&r.url)
        .call()
        .and_then(|mut res| res.body_mut().with_config().limit(ZIP_LIMIT).read_to_vec())
        .map_err(|e| http(&r.url, &e))?;
    let actual: String = Sha256::digest(&zip)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if actual != r.sha256 {
        return Err(format!(
            "{} has SHA-256 {actual}, not {}: nothing is kept",
            r.url, r.sha256
        ));
    }
    let path = unzip(db, &r.tag, &zip, &dir)?;
    println!(
        "kept {} in {:.1}s",
        path.display(),
        t0.elapsed().as_secs_f64()
    );
    Ok(path)
}

/// The SQL member of the release zip `zip`, written into `dir` under its cache name.
fn unzip(db: &EraDatabase, tag: &str, zip: &[u8], dir: &Path) -> Result<PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|e| format!("the download is not a zip: {e}"))?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .is_ok_and(|e| e.is_file() && db.is_sql_member(e.name(), tag))
        })
        .or_else(|| {
            // A dump zip holding one SQL file under another name is that dump.
            let sql: Vec<usize> = (0..archive.len())
                .filter(|&i| {
                    archive
                        .by_index(i)
                        .is_ok_and(|e| e.is_file() && e.name().ends_with(".sql"))
                })
                .collect();
            (db.form == Form::SqlZip && sql.len() == 1).then(|| sql[0])
        })
        .ok_or_else(|| "the zip holds no world database".to_owned())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = db.sql_name(tag);
    let dest = dir.join(&name);
    let partial = dir.join(format!("{name}.partial"));
    let mut entry = archive
        .by_index(index)
        .map_err(|e| format!("the zip: {e}"))?;
    let mut out =
        std::fs::File::create(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    std::io::copy(&mut entry, &mut out).map_err(|e| format!("{}: {e}", partial.display()))?;
    out.flush()
        .map_err(|e| format!("{}: {e}", partial.display()))?;
    drop(out);
    std::fs::rename(&partial, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(dest)
}

/// Reads the world database at `path`: a dump, or a release zip holding one.
fn read_database(
    db: &EraDatabase,
    path: &Path,
) -> Result<dereth_world_places::WorldContent, String> {
    let is_zip = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    if is_zip {
        let file = std::fs::File::open(path).map_err(|e| at(&e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| at(&e))?;
        let index = (0..archive.len())
            .find(|&i| {
                archive.by_index(i).is_ok_and(|e| {
                    e.is_file()
                        && (e.name().ends_with("/world.sql")
                            || e.name() == "world.sql"
                            || (db.form == Form::SqlZip && e.name().ends_with(".sql")))
                })
            })
            .ok_or_else(|| at(&"the zip holds no world database"))?;
        let mut text = String::new();
        archive
            .by_index(index)
            .map_err(|e| at(&e))?
            .read_to_string(&mut text)
            .map_err(|e| at(&e))?;
        return sql::read_str(&text).map_err(|e| at(&e));
    }
    let file = std::fs::File::open(path).map_err(|e| at(&e))?;
    sql::read(std::io::BufReader::with_capacity(1 << 20, file)).map_err(|e| at(&e))
}

fn build(o: &Options) -> Result<(), String> {
    let era = o.era.unwrap_or_default();
    let db = EraDatabase::of(era);
    let path = match &o.sql {
        Some(p) => p.clone(),
        None => fetch(o)?,
    };
    let t0 = Instant::now();
    let content = read_database(db, &path)?;
    let read = t0.elapsed();
    let places = WorldPlaces::of(&content);
    let world = o.world.as_deref().unwrap_or("Dereth");
    let docs = places.documents(world, o.source.unwrap_or(Source::Hint));
    let out = o
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("places-{}", era.name())));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in [("portals.tsv", &docs.portals), ("places.tsv", &docs.places)] {
        let p = out.join(name);
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let spawned = places
        .portals
        .iter()
        .filter(|p| {
            matches!(
                p.standing,
                dereth_world_places::documents::Standing::Spawned(_)
            )
        })
        .count();
    let towns = places
        .towns
        .iter()
        .filter(|t| t.kind == dereth_world_places::towns::TownKind::Town)
        .count();
    println!(
        "{}: {} portals ({spawned} spawned), {} places, {towns} towns and {} landmarks into {} (read in {:.1}s, built in {:.1}s)",
        path.display(),
        places.portals.len(),
        places.places.len(),
        places.towns.len() - towns,
        out.display(),
        read.as_secs_f64(),
        (t0.elapsed() - read).as_secs_f64(),
    );
    Ok(())
}
