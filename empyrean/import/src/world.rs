//! `empyrean-import world`: a world of layers (a `world.toml`, its base and its patches) checked
//! layer by layer, and built at any layer into Empyrean's pack and ACE's world SQL.
//!
//! ```text
//! empyrean-import world check --world <world.toml> [--dumps <folder>]
//! empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>]
//! ```
//!
//! `--dumps` names the folder a checkpoint's pinned dump is read from; without it, the folder
//! `EMPYREAN_WORLD_DUMPS` names, else the per-user cache `empyrean-import fetch` fills.
//!
//! `check` folds every layer and lists, layer by layer, what the fold found (conflicts, stale
//! removals, changes that change nothing, edits a later layer hides) and the references to nothing
//! the layer makes; it fails on any error. The references to nothing a snapshot brings, the base's
//! or a checkpoint's, are its source's: warnings, said once. A layer that changes nothing has the
//! world below it.
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
) -> Result<WorldContent, String> {
    let f = empyrean_world_db::world::open(world, dumps)?;
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
            _ => return Err(USAGE.into()),
        }
    }
    let world = world.ok_or(USAGE)?;
    let f = empyrean_world_db::world::open(&world, dumps.as_deref())?;
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
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => world = Some(value()?.into()),
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
    let c = content(&world, layer.as_deref(), dumps.as_deref())?;
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

const USAGE: &str = "usage: empyrean-import world check --world <world.toml> [--dumps <folder>]\n       empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>]";

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
