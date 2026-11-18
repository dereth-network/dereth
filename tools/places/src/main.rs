//! Builds a world's portal table and places for an era from the era's public world database,
//! fetching and caching the database first.
//!
//! `dereth-places` builds them the way a launcher builds them for a world it joins.
//!
//! **Depends on** its own library (`dereth_places`: the fetching, the cache and the build), over
//! `dereth-world-places` and `dereth-primitives`. **Used by** nothing: it is a program.
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

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use dereth_places::Fetch;
use dereth_primitives::EraId;
use dereth_world_places::eras::EraDatabase;
use dereth_world_places::Source;

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

fn cache(o: &Options) -> Result<PathBuf, String> {
    o.cache
        .clone()
        .or_else(dereth_places::cache_dir)
        .ok_or_else(|| "no per-user cache folder (set LOCALAPPDATA, HOME or --cache)".to_owned())
}

/// The program's HTTP client.
struct Http(ureq::Agent);

impl Http {
    fn new() -> Self {
        Self(
            ureq::Agent::config_builder()
                .user_agent(concat!("dereth-places/", env!("CARGO_PKG_VERSION")))
                .timeout_global(Some(Duration::from_secs(600)))
                .build()
                .into(),
        )
    }
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

impl Fetch for Http {
    fn text(&self, url: &str, accept: &str) -> Result<String, String> {
        self.0
            .get(url)
            .header("Accept", accept)
            .call()
            .and_then(|mut r| r.body_mut().read_to_string())
            .map_err(|e| http(url, &e))
    }

    fn bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        self.0
            .get(url)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(limit).read_to_vec())
            .map_err(|e| http(url, &e))
    }
}

/// Fetches the era's database into the cache, unless it is there; its path.
fn fetch(o: &Options) -> Result<PathBuf, String> {
    let t0 = Instant::now();
    let (path, _) = dereth_places::fetch_database(
        o.era.unwrap_or_default(),
        &cache(o)?,
        &Http::new(),
        &mut |line| println!("{line}"),
    )?;
    if t0.elapsed() > Duration::from_secs(1) {
        println!("in {:.1}s", t0.elapsed().as_secs_f64());
    }
    Ok(path)
}

fn build(o: &Options) -> Result<(), String> {
    let era = o.era.unwrap_or_default();
    let db = EraDatabase::of(era);
    let path = match &o.sql {
        Some(p) => p.clone(),
        None => fetch(o)?,
    };
    let t0 = Instant::now();
    let content = dereth_places::read_database(db, &path)?;
    let read = t0.elapsed();
    let out = o
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("places-{}", era.name())));
    let world = o.world.as_deref().unwrap_or("Dereth");
    let b = dereth_places::build(&content, world, o.source.unwrap_or(Source::Hint), &out)?;
    println!(
        "{}: {} portals ({} spawned), {} places, {} towns and {} landmarks into {} (read in {:.1}s, built in {:.1}s)",
        path.display(),
        b.portals,
        b.spawned,
        b.places,
        b.towns,
        b.landmarks,
        out.display(),
        read.as_secs_f64(),
        (t0.elapsed() - read).as_secs_f64(),
    );
    Ok(())
}
