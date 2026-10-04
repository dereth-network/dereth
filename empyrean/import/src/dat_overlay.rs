//! `empyrean-import dat-overlay`: a server's world data files taken apart against the base they
//! were made from, written as the overlay the server serves and a client lays over its own base.
//!
//! ```text
//! empyrean-import dat-overlay --base <dir> --world <dir> --out <dir> --world-key <name>
//!     [--era eor|infiltration] [--iterations revision|world] [--verify]
//! ```
//!
//! `--era` picks the files: `portal.dat` and `cell.dat` for an era before Throne of Destiny, the
//! later four (`client_portal.dat`, `client_cell_1.dat`, `client_local_English.dat`,
//! `client_highres.dat`) otherwise; each file present in both folders is compared, the names
//! matched without regard to case. Bytes decide what changed ([`dereth_dat::decompose`]); the
//! overlay puts every change in one revision of its own, one past the base's iterations, and each
//! removed record is a tombstone (a cell-file landblock the world has nothing of is one family
//! tombstone). `--iterations world` keeps the world's own iteration list and each record's world
//! iteration instead, for a world made outside Dereth whose server compares a client's iterations
//! with its own numbering. `--verify` then lays each container over its base and checks every
//! record of the world reads through it as the world's own, and nothing else is there.
//! `--world-key` is the name the world gives itself, which every
//! container records and a client checks. `--out` must not hold base data files, and an overlay
//! already there is replaced file by file.
//!
//! Exit status 0 when the overlay is written, 1 when the world is its base (nothing to write),
//! 2 for a usage or input error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dereth_dat::overlay::{self, OverlayDir};
use dereth_dat::{decompose, DatFile, RetailDat};

fn usage() -> ExitCode {
    eprintln!(
        "usage: empyrean-import dat-overlay --base <dir> --world <dir> --out <dir> --world-key <name> [--era eor|infiltration] [--iterations revision|world] [--verify]"
    );
    ExitCode::from(2)
}

/// `name` in `dir`, matched without regard to case.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().into_owned();
        (n.eq_ignore_ascii_case(name) && e.path().is_file()).then(|| e.path())
    })
}

/// The files `--era` compares, by the overlay target each lies under and its own name.
fn targets(pre_tod: bool) -> Vec<(RetailDat, &'static str)> {
    if pre_tod {
        vec![
            (RetailDat::Portal, dereth_dat::PreTodDat::Portal.file_name()),
            (RetailDat::Cell, dereth_dat::PreTodDat::Cell.file_name()),
        ]
    } else {
        RetailDat::ALL.iter().map(|t| (*t, t.file_name())).collect()
    }
}

/// Run the subcommand over its arguments.
pub fn run(args: &[String]) -> ExitCode {
    let (mut base, mut world, mut out, mut key) = (None, None, None, None);
    let mut pre_tod = false;
    let mut iterations = decompose::Iterations::Revision;
    let mut verify = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned();
        match a.as_str() {
            "--base" => base = value().map(PathBuf::from),
            "--world" => world = value().map(PathBuf::from),
            "--out" => out = value().map(PathBuf::from),
            "--world-key" => key = value(),
            "--verify" => verify = true,
            "--iterations" => match value().as_deref() {
                Some("revision") => iterations = decompose::Iterations::Revision,
                Some("world") => iterations = decompose::Iterations::World,
                _ => return usage(),
            },
            "--era" => match value().as_deref().and_then(dereth_primitives::EraId::parse) {
                Some(e) => pre_tod = e.container_era() == dereth_primitives::ContainerEra::PreTod,
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    let (Some(base), Some(world), Some(out), Some(key)) = (base, world, out, key) else {
        return usage();
    };
    let dir = match OverlayDir::new(&out) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("empyrean-import dat-overlay: {e}");
            return ExitCode::from(2);
        }
    };
    let mut wrote = false;
    for (target, name) in targets(pre_tod) {
        let (Some(b), Some(w)) = (find(&base, name), find(&world, name)) else {
            continue;
        };
        let opened = DatFile::open(&b).and_then(|bf| Ok((bf, DatFile::open(&w)?)));
        let (bf, wf) = match opened {
            Ok(f) => f,
            Err(e) => {
                eprintln!("empyrean-import dat-overlay: {name}: {e}");
                return ExitCode::from(2);
            }
        };
        if bf.era() != wf.era() {
            eprintln!(
                "empyrean-import dat-overlay: {} and {} are in different layouts",
                b.display(),
                w.display()
            );
            return ExitCode::from(2);
        }
        let d = match decompose::diff(&bf, &wf) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("empyrean-import dat-overlay: {name}: {e}");
                return ExitCode::from(2);
            }
        };
        println!(
            "{name}: {} added, {} changed ({} at an unchanged iteration), {} removed, {} re-stamped \
             and left to the base; {} bytes; base {}",
            d.added.len(),
            d.changed.len(),
            d.changed_same_iteration,
            d.removed.len(),
            d.restamped,
            d.bytes,
            overlay::hex(&overlay::fingerprint(&bf))
        );
        if d.is_empty() {
            continue;
        }
        let cell = target == RetailDat::Cell;
        let gone = decompose::deletions(&bf, &d, cell);
        match decompose::write(
            &bf,
            &wf,
            &d,
            &dir.container(target),
            name,
            &key,
            cell,
            iterations,
        ) {
            Ok(revision) => {
                println!(
                    "  -> {} (iteration {revision}; {} tombstone(s), {} of them whole landblocks)",
                    dir.container(target).display(),
                    gone.len(),
                    gone.iter().filter(|(_, mask)| *mask != 0).count()
                );
                wrote = true;
                if verify {
                    match check(&bf, &wf, &dir.container(target), &key) {
                        Ok(n) => println!("  verified: {n} record(s) read as the world's"),
                        Err(e) => {
                            eprintln!("empyrean-import dat-overlay: {name}: verify: {e}");
                            return ExitCode::from(2);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("empyrean-import dat-overlay: {name}: {e}");
                return ExitCode::from(2);
            }
        }
    }
    if wrote {
        ExitCode::SUCCESS
    } else {
        println!("the world is its base: no overlay was written");
        ExitCode::from(1)
    }
}

/// Lay the container at `path` over `base` and check it reads as `world`: as many records, each
/// of the world's read as the world's, and the world's iterations.
fn check(base: &DatFile, world: &DatFile, path: &Path, key: &str) -> Result<usize, String> {
    let layer = overlay::Layer::over(
        base,
        DatFile::open(path).map_err(|e| e.to_string())?,
        Some(key),
    )
    .map_err(|e| e.to_string())?;
    let exact = layer.manifest().exact_iterations;
    let layered = base.layered(std::sync::Arc::new(layer));
    if layered.len() != world.len() {
        return Err(format!(
            "{} records, the world has {}",
            layered.len(),
            world.len()
        ));
    }
    let mut n = 0;
    for id in world.iter_ids() {
        if id == dereth_dat::ITERATION_LIST {
            continue;
        }
        if layered.read(id).ok() != world.read(id).ok() {
            return Err(format!("{id:?} does not read as the world's"));
        }
        n += 1;
    }
    if exact && layered.iteration_list().ok() != world.iteration_list().ok() {
        return Err("the iterations are not the world's".into());
    }
    Ok(n)
}
