//! `dereth-art`: surveys the models the world is built from, and packs art packs.
//!
//! ```text
//! dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] [--tags <tags.tsv>]
//!                   [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails] [--pixels]
//! dereth-art pack <folder> [--out <file>] [--check]
//! dereth-art mesh <id> [--dat-dir <dir>]
//! dereth-art skeleton <setup> [--dat-dir <dir>] [--out <dir>] [--anim <id>@<fps>,...]
//! dereth-art ac2-look --ac2 <dir> [--ac1 <dir>] [--out <file>] [--parts <part>,...] [--keep-folder]
//! dereth-art ac2-look --versions
//! ```
//!
//! `survey`: `--dat-dir` defaults to `DERETH_TEST_DAT_DIR`, `--out` to `target/art-survey`,
//! `--tags` to `tags.tsv` in the output folder when it is there, and `--threads` to the
//! machine's. `--pixels` also writes every retail picture the world draws to `pictures/` there.
//!
//! `pack`: the art pack in `<folder>` packed into one file, `--out` or `<folder>.dat` beside it;
//! with `--check`, whether that file is what the folder packs into now, and which records are
//! not.
//!
//! `mesh`: the triangles the model `<id>` draws, as JSON on the standard output, by group: its
//! surface, and each corner's position, normal and picture coordinates.
//! `skeleton`: the setup as a glTF skeleton with its parts as a reference body
//! ([`dereth_art::skeleton`]), in `--out` (default `target/skeleton/<setup>`), with each animation of
//! `--anim` as a clip at its rate.
//!
//! `ac2-look`: the AC2 look built from the Asheron's Call 2 install in `--ac2` and the Asheron's
//! Call files in `--ac1` (default `DERETH_TEST_DAT_DIR`; only the rocks read them) into the packed
//! file `--out` (default `ac2-look.dat`), with the parts `--parts` names (default all: terrain,
//! red-desert, trees, rocks, sky) ([`dereth_art::ac2_look`]). It says what it is doing a line at a
//! time on the standard output, `step <what>`, and last `built <file> <version>`; with
//! `--keep-folder` the pack's folder stays beside the file. `--versions` prints the look's version
//! and each part's table version, `<part> <version>` a line, and builds nothing.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "dereth-art survey [--dat-dir <dir>] [--out <dir>] [--spawns <spawns.tsv>] \
[--tags <tags.tsv>] [--landblocks <XXYY,XXYY,...>] [--threads <n>] [--no-thumbnails] [--pixels]
       dereth-art pack <folder> [--out <file>] [--check]
       dereth-art mesh <id> [--dat-dir <dir>]
       dereth-art skeleton <setup> [--dat-dir <dir>] [--out <dir>] [--anim <id>@<fps>,...]
       dereth-art ac2-look --ac2 <dir> [--ac1 <dir>] [--out <file>] [--parts <part>,...] [--keep-folder]
       dereth-art ac2-look --versions";

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
        Some("mesh") => mesh(&args[1..]),
        Some("skeleton") => match skeleton_options(&args[1..]) {
            Ok((dat_dir, req)) => skeleton(&dat_dir, &req),
            Err(e) => {
                eprintln!(
                    "dereth-art skeleton: {e}
usage: {USAGE}"
                );
                ExitCode::from(2)
            }
        },
        Some("ac2-look") => ac2_look(&args[1..]),
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
    let mut pixels = false;
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
            "--pixels" => pixels = true,
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
        pixels,
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

fn mesh(args: &[String]) -> ExitCode {
    let id = args.first().and_then(|s| {
        u32::from_str_radix(s.trim_start_matches("0x"), 16)
            .ok()
            .map(dereth_primitives::DataId)
    });
    let dat_dir = match args.get(1..) {
        Some([flag, dir]) if flag == "--dat-dir" => Some(PathBuf::from(dir)),
        _ => std::env::var_os("DERETH_TEST_DAT_DIR").map(PathBuf::from),
    };
    let (Some(id), Some(dat_dir)) = (id, dat_dir) else {
        eprintln!("usage: {USAGE}");
        return ExitCode::from(2);
    };
    let store = match dereth_art::survey::open_dats(&dat_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dereth-art mesh: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut models = dereth_art::model::Models::new(&store);
    let Some(mesh) = models.mesh(dereth_art::tally::ModelId::Object(id), &[]) else {
        eprintln!("dereth-art mesh: {id:?} does not read");
        return ExitCode::FAILURE;
    };
    let groups: Vec<_> = mesh
        .groups
        .iter()
        .map(|g| {
            let corners: Vec<_> = g
                .triangles
                .iter()
                .flatten()
                .map(|v| (v.position, v.normal, v.uv))
                .collect();
            serde_json::json!({
                "surface": g.surface.map(|s| s.0),
                "two_sided": g.two_sided,
                "corners": corners,
            })
        })
        .collect();
    println!("{}", serde_json::Value::Array(groups));
    ExitCode::SUCCESS
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

/// A record id, `0x` and hexadecimal.
fn data_id(s: &str) -> Result<dereth_primitives::DataId, String> {
    u32::from_str_radix(s.trim().trim_start_matches("0x"), 16)
        .map(dereth_primitives::DataId)
        .map_err(|_| format!("{s:?} is not a record id"))
}

/// `skeleton`'s dat folder and request.
fn skeleton_options(args: &[String]) -> Result<(PathBuf, dereth_art::skeleton::Request), String> {
    let mut setup = None;
    let mut dat_dir = None;
    let mut out = None;
    let mut animations = Vec::new();
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
            "--anim" => {
                for one in value()?.split(',') {
                    let (id, fps) = one
                        .split_once('@')
                        .ok_or_else(|| format!("--anim wants <id>@<fps>, got {one:?}"))?;
                    let fps: f32 = fps
                        .parse()
                        .ok()
                        .filter(|f: &f32| *f > 0.0)
                        .ok_or_else(|| format!("{fps:?} is not a rate"))?;
                    animations.push((data_id(id)?, fps));
                }
            }
            other if setup.is_none() && !other.starts_with("--") => setup = Some(data_id(other)?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let setup = setup.ok_or("no setup named")?;
    let dat_dir = match dat_dir {
        Some(d) => d,
        None => std::env::var_os("DERETH_TEST_DAT_DIR")
            .map(PathBuf::from)
            .ok_or("no --dat-dir, and DERETH_TEST_DAT_DIR is not set")?,
    };
    let out = out.unwrap_or_else(|| PathBuf::from(format!("target/skeleton/0x{:08X}", setup.0)));
    Ok((
        dat_dir,
        dereth_art::skeleton::Request {
            setup,
            animations,
            out,
        },
    ))
}

fn skeleton(dat_dir: &std::path::Path, req: &dereth_art::skeleton::Request) -> ExitCode {
    let store = match dereth_art::survey::open_dats(dat_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("dereth-art skeleton: {e}");
            return ExitCode::FAILURE;
        }
    };
    match dereth_art::skeleton::export(&store, req) {
        Ok(w) => {
            let parents: Vec<String> = w
                .parents
                .iter()
                .enumerate()
                .map(|(k, p)| p.map_or(format!("{k}:root"), |p| format!("{k}<{p}")))
                .collect();
            println!(
                "wrote {}: {} joints, {} triangles, {} pictures, {} clips
parents: {}",
                w.gltf.display(),
                w.joints,
                w.triangles,
                w.pictures,
                w.clips,
                parents.join(" ")
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-art skeleton: {e}");
            ExitCode::FAILURE
        }
    }
}

fn ac2_look_request(args: &[String]) -> Result<dereth_art::ac2_look::build::Request, String> {
    use dereth_art::ac2_look::tables::Part;
    let mut ac2 = None;
    let mut ac1 = std::env::var_os("DERETH_TEST_DAT_DIR").map(PathBuf::from);
    let mut out = PathBuf::from("ac2-look.dat");
    let mut parts: std::collections::BTreeSet<Part> = Part::ALL.into_iter().collect();
    let mut keep_folder = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--ac2" => ac2 = Some(PathBuf::from(value()?)),
            "--ac1" => ac1 = Some(PathBuf::from(value()?)),
            "--out" => out = PathBuf::from(value()?),
            "--parts" => {
                parts = value()?
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| Part::from_name(s.trim()).ok_or_else(|| format!("no part {s:?}")))
                    .collect::<Result<_, String>>()?;
            }
            "--keep-folder" => keep_folder = true,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(dereth_art::ac2_look::build::Request {
        ac2: ac2.ok_or("--ac2 names no install")?,
        ac1,
        out,
        parts,
        keep_folder,
    })
}

fn ac2_look(args: &[String]) -> ExitCode {
    use dereth_art::ac2_look::{build, tables::Tables};
    let tables = match Tables::built_in() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("dereth-art ac2-look: {e}");
            return ExitCode::FAILURE;
        }
    };
    if args == ["--versions"] {
        println!("look {}", build::version(&tables));
        for (part, v) in tables.versions() {
            println!("{} {v}", part.name());
        }
        return ExitCode::SUCCESS;
    }
    let req = match ac2_look_request(args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "dereth-art ac2-look: {e}
usage: {USAGE}"
            );
            return ExitCode::from(2);
        }
    };
    let started = std::time::Instant::now();
    let mut progress = |what: &str| println!("step {what}");
    match build::build(&tables, &req, &mut progress) {
        Ok(b) => {
            for line in &b.left_out {
                eprintln!("left out: {line}");
            }
            eprintln!(
                "{} records, {} MB, in {:.1} s",
                b.records,
                b.bytes / 1_000_000,
                started.elapsed().as_secs_f64()
            );
            println!("built {} {}", b.out.display(), b.version);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-art ac2-look: {e}");
            ExitCode::FAILURE
        }
    }
}
