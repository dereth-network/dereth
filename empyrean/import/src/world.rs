//! `empyrean-import world`: a world of layers (a `world.toml`, its base and its patches) checked
//! layer by layer, and built at any layer into Empyrean's pack and ACE's world SQL.
//!
//! ```text
//! empyrean-import world check --world <world.toml> [--dumps <folder>] [--dats <folder>]...
//! empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>] [--dats <folder>]...
//! ```
//!
//! `--dumps` names the folder a checkpoint's pinned dump is read from; without it, the folder
//! `EMPYREAN_WORLD_DUMPS` names, else the per-user cache `empyrean-import fetch` fills.
//!
//! `--dats` names a folder of data files (`*.dat`, in it or the folders below it), and may be
//! given more than once. Each file is known by its header and its iteration, and serves every
//! layer that names that iteration (`portal_iteration` in a `patch.toml`, or in the world file's
//! `[land]` for the base). Without it, closure isn't checked.
//!
//! `check` folds every layer and lists, layer by layer, what the fold found (conflicts, stale
//! removals, changes that change nothing, edits a later layer hides), the references to nothing
//! the layer makes, its loot (the swap from the source's loot to ACE's is all three of its parts
//! at once, and is reported where it happens) and, with `--dats`, the records it names that its
//! data files lack; it fails on any error. What a snapshot brings, the base's or a checkpoint's,
//! is its source's: warnings, said once. A layer that changes nothing has the world below it. A
//! layer whose data files aren't among those given isn't checked for closure, and says so.
//!
//! `build` folds the world up to `--layer` (the last, when left out), checks the layers up to it as
//! `check` does and refuses on any error, numbers it by rule and writes its pack, the same bytes
//! for the same rows; `--sql` also writes `world.sql` and `empyrean.sql`
//! into the folder ([`crate::world_sql`]). Loaded into an empty database, in that order, they are
//! the world; `empyrean-import --sql <an empty file> --patches world.sql --patches empyrean.sql`
//! builds the same pack from them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;
use empyrean_world_db::closure::Dats;
use empyrean_world_db::compare::dataset_of;
use empyrean_world_db::files::default_stamp;
use empyrean_world_db::layers::{Folding, Severity};

/// The world at layer `layer` (its id; the last when `None`), numbered by rule.
///
/// # Errors
/// A world that doesn't read, a layer it doesn't have, and every error `world check` finds in the
/// layers up to it.
pub fn content(
    world: &Path,
    layer: Option<&str>,
    dumps: Option<&Path>,
    dats: &[PathBuf],
) -> Result<WorldContent, String> {
    let f = open(world, dumps, dats)?;
    let n = match layer {
        None => f.len() - 1,
        Some(id) => f
            .ids
            .iter()
            .position(|l| l == id)
            .ok_or_else(|| format!("{}: no layer {id}", world.display()))?,
    };
    let mut c = f.checked_content_at(n)?;
    empyrean_world_db::number(&mut c);
    Ok(c)
}

/// The world's layers, read, with the data files in `dats` (none: closure isn't checked).
///
/// # Errors
/// A world that doesn't read, and a folder of data files that doesn't.
pub fn open(world: &Path, dumps: Option<&Path>, dats: &[PathBuf]) -> Result<Folding, String> {
    let f = empyrean_world_db::world::open(world, dumps)?;
    if dats.is_empty() {
        return Ok(f);
    }
    Ok(f.with_dats(Dats::scan(dats)?))
}

/// The world's pack, its dataset id from its rows.
///
/// # Errors
/// Content the pack refuses.
pub fn pack_of(c: &WorldContent) -> Result<Vec<u8>, String> {
    let files = empyrean_world_db::write(c, default_stamp())?;
    c.to_pack(dataset_of(&files))
        .map(|(bytes, _)| bytes)
        .map_err(|e| e.to_string())
}

/// Each layer's errors, warnings and reports, as lines; and how many errors there were.
#[must_use]
pub fn check_lines(f: Folding) -> (Vec<String>, usize) {
    let layers = f.len();
    let mut lines = Vec::new();
    let mut errors = 0;
    let mut quiet = 0;
    for l in &f.check() {
        let shown: Vec<String> = l
            .findings
            .iter()
            .map(|x| {
                let word = match x.severity() {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Report => "report",
                };
                format!("  {word}: {x}")
            })
            .collect();
        errors += l
            .findings
            .iter()
            .filter(|x| x.severity() == Severity::Error)
            .count();
        errors += l.problems.len();
        let problems: Vec<String> = l
            .problems
            .iter()
            .map(|p| format!("  error: {p}"))
            .chain(
                l.inherited
                    .iter()
                    .map(|p| format!("  warning: {p} (its source's own)")),
            )
            .chain(l.warnings.iter().map(|w| format!("  warning: {w}")))
            .chain(l.reports.iter().map(|r| format!("  report: {r}")))
            .collect();
        if shown.is_empty() && problems.is_empty() {
            quiet += 1;
            continue;
        }
        lines.push(format!("{}:", l.id));
        lines.extend(shown);
        lines.extend(problems);
    }
    lines.push(format!(
        "{} layers, {quiet} with nothing to say; {errors} errors",
        layers
    ));
    (lines, errors)
}

fn run_check(args: &[String]) -> Result<bool, String> {
    let mut world: Option<PathBuf> = None;
    let mut dumps: Option<PathBuf> = None;
    let mut dats: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => world = Some(value()?.into()),
            "--dumps" => dumps = Some(value()?.into()),
            "--dats" => dats.push(value()?.into()),
            _ => return Err(USAGE.into()),
        }
    }
    let world = world.ok_or(USAGE)?;
    let mut f = empyrean_world_db::world::open(&world, dumps.as_deref())?;
    if !dats.is_empty() {
        let d = Dats::scan(&dats)?;
        println!("{}", d.summary());
        f = f.with_dats(d);
    }
    let (lines, errors) = check_lines(f);
    for l in &lines {
        println!("{l}");
    }
    Ok(errors == 0)
}

fn run_build(args: &[String]) -> Result<bool, String> {
    let mut world: Option<PathBuf> = None;
    let mut layer: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut sql: Option<PathBuf> = None;
    let mut dumps: Option<PathBuf> = None;
    let mut dats: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => world = Some(value()?.into()),
            "--dats" => dats.push(value()?.into()),
            "--layer" => layer = Some(value()?),
            "--out" => out = Some(value()?.into()),
            "--sql" => sql = Some(value()?.into()),
            "--dumps" => dumps = Some(value()?.into()),
            _ => return Err(USAGE.into()),
        }
    }
    let (Some(world), Some(out)) = (world, out) else {
        return Err(USAGE.into());
    };
    let c = content(&world, layer.as_deref(), dumps.as_deref(), &dats)?;
    let bytes = pack_of(&c)?;
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    let p = Pack::from_bytes(bytes).map_err(|e| e.to_string())?;
    let hash: String = p
        .header()
        .content_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("{}: content hash {hash}", out.display());
    if let Some(dir) = sql {
        let s = crate::world_sql::write(&c)?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (name, text) in [("world.sql", &s.world), ("empyrean.sql", &s.empyrean)] {
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("{}", path.display());
        }
    }
    Ok(true)
}

const USAGE: &str = "usage: empyrean-import world check --world <world.toml> [--dumps <folder>] [--dats <folder>]...\n       empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>] [--dats <folder>]...";

/// `empyrean-import world ...`.
pub fn run(args: &[String]) -> ExitCode {
    let result = match args.first().map(String::as_str) {
        Some("check") => run_check(&args[1..]),
        Some("build") => run_build(&args[1..]),
        _ => Err(USAGE.into()),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("empyrean-import world: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests;
