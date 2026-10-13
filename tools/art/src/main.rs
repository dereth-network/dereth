//! `dereth-art`: surveys the models the world is built from.
//!
//! ```text
//! dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] [--tags <tags.tsv>]
//!                   [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails]
//! ```
//!
//! `--dat-dir` defaults to `DERETH_TEST_DAT_DIR`, `--out` to `target/art-survey`, `--tags` to
//! `tags.tsv` in the output folder when it is there, and `--threads` to the machine's.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] \
[--tags <tags.tsv>] [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails]";

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
