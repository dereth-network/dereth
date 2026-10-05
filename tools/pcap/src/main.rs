//! `dereth-pcap`: ingest the retail captures into an index, and ask it questions.

use std::path::PathBuf;
use std::process::ExitCode;

use dereth_pcap::corpus::{index_path, roots_from_env, ENV_ROOTS};
use dereth_pcap::flows::PortSet;
use dereth_pcap::{index, ingest, report};

const USAGE: &str = "\
usage: dereth-pcap <command> [options]

commands:
  ingest [--rebuild] [--jobs N] [--server-ports 9000-9013] [--verbose]
                        index every capture under the roots (incremental by content hash)
  stats                 captures, sessions by game (ac1/ac2), messages by direction, top types,
                        decode failures (AC1 sessions), the AC2 type census
  query [--csv] [--width N] \"<SQL>\"
                        run SQL against the index; a table, or CSV with --csv.
                        Cite sessions by session_key (stable across rebuilds); sid('<key>')
                        gives a key's current session_id
  triage --out FILE [--examples N]
                        decode failures grouped by type and error kind, with example sessions
                        and hex around the failing offset, written as markdown to FILE

common options:
  --roots <paths>       corpus roots, a path list (default: $DERETH_RETAIL_PCAPS)
  --index <file>        the index (default: $DERETH_PCAP_INDEX, else <first root>/.index/retail.sqlite)

The schema: the tables of the crate's `index` module.";

struct Args {
    cmd: String,
    rest: Vec<String>,
    roots: Option<Vec<PathBuf>>,
    index: Option<PathBuf>,
}

fn take(rest: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    if let Some(i) = rest.iter().position(|a| a == flag) {
        if i + 1 >= rest.len() {
            return Err(format!("{flag} needs a value"));
        }
        let v = rest.remove(i + 1);
        rest.remove(i);
        return Ok(Some(v));
    }
    Ok(None)
}

fn flag(rest: &mut Vec<String>, flag: &str) -> bool {
    if let Some(i) = rest.iter().position(|a| a == flag) {
        rest.remove(i);
        return true;
    }
    false
}

fn parse() -> Result<Args, String> {
    let mut rest: Vec<String> = std::env::args().skip(1).collect();
    if rest.is_empty() || rest.iter().any(|a| a == "-h" || a == "--help") {
        return Err(String::new());
    }
    let cmd = rest.remove(0);
    let roots = take(&mut rest, "--roots")?.map(|v| {
        std::env::split_paths(&v)
            .filter(|p| !p.as_os_str().is_empty())
            .collect()
    });
    let index = take(&mut rest, "--index")?.map(PathBuf::from);
    Ok(Args {
        cmd,
        rest,
        roots,
        index,
    })
}

fn resolve(a: &Args) -> Result<(Vec<PathBuf>, PathBuf), String> {
    let roots = a.roots.clone().or_else(roots_from_env).unwrap_or_default();
    let index = a
        .index
        .clone()
        .or_else(|| index_path(&roots))
        .ok_or_else(|| format!("no corpus: set {ENV_ROOTS} or pass --roots (or --index)"))?;
    Ok((roots, index))
}

fn run() -> Result<(), String> {
    let mut a = parse()?;
    let (roots, index_file) = resolve(&a)?;
    match a.cmd.as_str() {
        "ingest" => {
            if roots.is_empty() {
                return Err(format!(
                    "ingest needs corpus roots: set {ENV_ROOTS} or pass --roots"
                ));
            }
            for r in &roots {
                if !r.is_dir() {
                    return Err(format!("{}: not a directory", r.display()));
                }
            }
            let rebuild = flag(&mut a.rest, "--rebuild");
            let verbose = flag(&mut a.rest, "--verbose");
            let jobs = match take(&mut a.rest, "--jobs")? {
                Some(j) => j.parse().map_err(|_| format!("bad --jobs {j}"))?,
                None => std::thread::available_parallelism().map_or(4, |n| n.get().min(16)),
            };
            let ports = match take(&mut a.rest, "--server-ports")? {
                Some(p) => PortSet::parse(&p)?,
                None => PortSet::default(),
            };
            if let Some(x) = a.rest.first() {
                return Err(format!("ingest: unexpected `{x}`"));
            }
            let opts = ingest::Options {
                roots,
                index: index_file.clone(),
                ports,
                jobs,
                rebuild,
                verbose,
            };
            let r = ingest::run(&opts)?;
            println!(
                "ingest: {} files ({} unchanged, {} processed); {} captures ingested, {} already \
                 indexed; {} sessions, {} packets, {} messages; {} incomplete captures redone, {} \
                 vanished captures removed",
                r.files_seen,
                r.files_unchanged,
                r.files_processed,
                r.captures_ingested,
                r.captures_already_indexed,
                r.sessions,
                r.packets,
                r.messages,
                r.incomplete_removed,
                r.orphans_removed
            );
            println!(
                "ingest: sessions by game: {} ac1, {} ac2, {} unknown; {} decoded again after the \
                 first {} messages voted for another game",
                r.games[0],
                r.games[1],
                r.games[2],
                r.redecoded,
                dereth_pcap::transport::LEAD_IN
            );
            let mib = r.index_bytes / 1_048_576;
            println!(
                "ingest: wall {:.1} s with {jobs} workers; index {} ({mib} MiB)",
                r.wall.as_secs_f64(),
                index_file.display()
            );
        }
        "stats" => {
            let conn = index::open_for_read(&index_file)?;
            print!("{}", report::stats(&conn)?);
        }
        "query" => {
            let csv = flag(&mut a.rest, "--csv");
            let width = match take(&mut a.rest, "--width")? {
                Some(w) => w.parse().map_err(|_| format!("bad --width {w}"))?,
                None => 60,
            };
            let sql = a.rest.join(" ");
            if sql.trim().is_empty() {
                return Err("query needs SQL".into());
            }
            let conn = index::open_for_read(&index_file)?;
            let t = std::time::Instant::now();
            let (cols, rows) = report::rows(&conn, &sql, if csv { 0 } else { 32 })?;
            let took = t.elapsed();
            if csv {
                print!("{}", report::csv(&cols, &rows));
            } else {
                print!("{}", report::table(&cols, &rows, width));
                eprintln!("({} rows, {:.3} s)", rows.len(), took.as_secs_f64());
            }
        }
        "triage" => {
            let out = take(&mut a.rest, "--out")?
                .map(PathBuf::from)
                .ok_or("triage writes a file: name it with --out FILE")?;
            let examples = match take(&mut a.rest, "--examples")? {
                Some(n) => n.parse().map_err(|_| format!("bad --examples {n}"))?,
                None => 3,
            };
            let conn = index::open_for_read(&index_file)?;
            let md = report::triage(&conn, examples)?;
            std::fs::write(&out, md).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("triage: wrote {}", out.display());
        }
        other => return Err(format!("unknown command `{other}`")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-pcap: {e}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
