//! `dereth-art`: surveys the models the world is built from, and packs art packs.
//!
//! ```text
//! dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] [--tags <tags.tsv>]
//!                   [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails]
//! dereth-art pack <folder> [--out <file>] [--check]
//! ```
//!
//! `survey`: `--dat-dir` defaults to `DERETH_TEST_DAT_DIR`, `--out` to `target/art-survey`,
//! `--tags` to `tags.tsv` in the output folder when it is there, and `--threads` to the
//! machine's.
//!
//! `pack`: the art pack in `<folder>` packed into one file, `--out` or `<folder>.dat` beside it;
//! with `--check`, whether that file is what the folder packs into now, and which records are
//! not.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] \
[--tags <tags.tsv>] [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails]
       dereth-art pack <folder> [--out <file>] [--check]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("survey") => match options(&args[1..]) {
            Ok(opts) => survey(&opts),
            Err(e) => {
                eprintln!("dereth-art survey: {e}\nusage: {USAGE}");
                ExitCode::from(2)
            }
        },
        Some("pack") => match pack_options(&args[1..]) {
            Ok((dir, out, check)) => pack(&dir, &out, check),
            Err(e) => {
                eprintln!("dereth-art pack: {e}\nusage: {USAGE}");
                ExitCode::from(2)
            }
        },
        Some("-h" | "--help") => {
            println!("usage: {USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: {USAGE}");
            ExitCode::from(2)
        }
    }
}

fn options(args: &[String]) -> Result<dereth_art::survey::Options, String> {
    let mut dat_dir = None;
    let mut out = None;
    let mut spawns = None;
    let mut tags = None;
    let mut landblocks = None;
    let mut threads = None;
    let mut thumbnails = true;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--dat-dir" => dat_dir = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--spawns" => spawns = Some(PathBuf::from(value()?)),
            "--tags" => tags = Some(PathBuf::from(value()?)),
            "--landblocks" => {
                let v = value()?;
                let ids = v
                    .split(',')
                    .map(|s| {
                        u16::from_str_radix(s.trim().trim_start_matches("0x"), 16)
                            .map_err(|_| format!("--landblocks wants XXYY ids, got {s:?}"))
                    })
                    .collect::<Result<Vec<u16>, String>>()?;
                landblocks = Some(ids);
            }
            "--threads" => {
                let v = value()?;
                threads = Some(
                    v.parse::<usize>()
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or_else(|| format!("--threads wants a count, got {v:?}"))?,
                );
            }
            "--no-thumbnails" => thumbnails = false,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let dat_dir = match dat_dir {
        Some(d) => d,
        None => std::env::var_os("DERETH_TEST_DAT_DIR")
            .map(PathBuf::from)
            .ok_or("no --dat-dir, and DERETH_TEST_DAT_DIR is not set")?,
    };
    Ok(dereth_art::survey::Options {
        dat_dir,
        out: out.unwrap_or_else(|| PathBuf::from("target/art-survey")),
        spawns,
        tags,
        landblocks,
        threads: threads.unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get)
        }),
        thumbnails,
    })
}

/// `pack`'s folder, its file and whether it only checks.
fn pack_options(args: &[String]) -> Result<(PathBuf, PathBuf, bool), String> {
    let mut dir = None;
    let mut out = None;
    let mut check = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => {
                out = Some(PathBuf::from(
                    it.next().ok_or_else(|| format!("{a} needs a value"))?,
                ));
            }
            "--check" => check = true,
            other if dir.is_none() && !other.starts_with("--") => dir = Some(PathBuf::from(other)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let dir = dir.ok_or("no folder named")?;
    let out = out.unwrap_or_else(|| dereth_art::pack::default_out(&dir));
    Ok((dir, out, check))
}

fn pack(dir: &std::path::Path, out: &std::path::Path, check: bool) -> ExitCode {
    use dereth_art::pack::Freshness;
    let started = std::time::Instant::now();
    if check {
        return match dereth_art::pack::check(dir, out) {
            Ok(Freshness::Current) => {
                println!("{} is what {} packs into", out.display(), dir.display());
                ExitCode::SUCCESS
            }
            Ok(Freshness::Stale(lines)) => {
                eprintln!(
                    "{} is stale; run dereth-art pack {}",
                    out.display(),
                    dir.display()
                );
                for line in lines {
                    eprintln!("  {line}");
                }
                ExitCode::FAILURE
            }
            Ok(Freshness::Missing) => {
                eprintln!(
                    "{} is not there; run dereth-art pack {}",
                    out.display(),
                    dir.display()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("dereth-art pack: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match dereth_art::pack::pack(dir, out) {
        Ok(r) => {
            for problem in &r.problems {
                eprintln!("left out: {problem}");
            }
            println!(
                "wrote {}: the {:?} pack, {} records, {} MB, in {:.1} s",
                r.out.display(),
                r.name,
                r.records,
                r.bytes.div_ceil(1 << 20),
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-art pack: {e}");
            ExitCode::FAILURE
        }
    }
}

fn survey(opts: &dereth_art::survey::Options) -> ExitCode {
    let log = |s: &str| eprintln!("{s}");
    match dereth_art::survey::run(opts, &log) {
        Ok(s) => {
            println!(
                "surveyed {} landblocks and {} models in {:.0} s; wrote {}",
                s.tally.world.landblocks,
                s.tally.uses.len(),
                s.seconds,
                opts.out.join("report.html").display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-art survey: {e}");
            ExitCode::FAILURE
        }
    }
}
