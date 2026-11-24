//! `empyrean-import world`: a world of layers (a `world.toml`, its base and its patches) checked
//! layer by layer, and built at any layer into Empyrean's pack and ACE's world SQL.
//!
//! ```text
//! empyrean-import world check --world <world.toml> [--json] [--dumps <folder>] [--dats <folder>]...
//! empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>] [--dats <folder>]...
//! empyrean-import world release --world <world.toml> --version <x.y.z> --out <folder> [--previous <manifest.json>] [--dumps <folder>] [--dats <folder>]...
//! empyrean-import world cutover --world <world.toml> --layer <id> --shard <shard database>
//! ```
//!
//! `--dumps` names the folder a checkpoint's pinned dump is read from; without it, the folder
//! `EMPYREAN_WORLD_DUMPS` names, else the per-user cache `empyrean-import fetch` fills.
//!
//! `--dats` names a folder of data files (`*.dat`, in it or the folders below it), and may be
//! given more than once. Each file is known by its header and its iteration, and serves every
//! layer that names that iteration (`portal_iteration` in a `patch.toml`, or in the world file's
//! `[land]` for the base); of two of one iteration, the one holding more records. A player's
//! partial cell file serves too: what it lacks says nothing, and what it holds is whole. Without
//! `--dats`, closure and rooms aren't checked.
//!
//! `check` folds every layer and lists, layer by layer, what the fold found (conflicts, stale
//! removals, changes that change nothing, edits a later layer hides), the references to nothing
//! the layer makes, its loot (the swap from the source's loot to ACE's is all three of its parts
//! at once, and is reported where it happens) and, with `--dats`, the records it names that its
//! data files lack (a room is there when its landblock's info claims it) and the rooms it names
//! that its files give another shape or place than the files below did: a placed portal's
//! destination outside its room now is an error, and every such room is reported by landblock. It
//! fails on any error. What a snapshot brings, the base's or a checkpoint's, is its source's:
//! warnings, said once. A layer that changes nothing has the world below it. A layer whose data
//! files aren't among those given isn't checked for closure, and says so. What a patch's data files
//! find is a warning until its `patch.toml` says `status = "written"`, and its error after.
//!
//! `check --json` says the same, one line of JSON for each thing found (`layer`, `severity`:
//! `error`, `warning` or `report`, `text` as the plain check says it, and `item`, `file` and `line`
//! when it is known), then one of the counts (`layers`, `quiet`, `errors`): what a world editor
//! reads to list them.
//!
//! `build` folds the world up to `--layer` (the last, when left out), checks the layers up to it as
//! `check` does and refuses on any error, numbers it by rule and writes its pack, the same bytes
//! for the same rows; `--sql` also writes `world.sql` and `empyrean.sql`
//! into the folder ([`crate::world_sql`]). Loaded into an empty database, in that order, they are
//! the world; `empyrean-import --sql <an empty file> --patches world.sql --patches empyrean.sql`
//! builds the same pack from them.
//!
//! `release` makes a release of the world: the packs of its written layers and a manifest of every
//! layer ([`release`]).
//!
//! `cutover` checks a server's shard database before the server is pointed at the world's layer:
//! it refuses when a saved house, or a house permission, is under a guid the world's newest
//! checkpoint up to the layer re-keys ([`cutover`]). The file is only read.

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

/// One thing the check says of a layer: how bad, the item it is of and where it is written, when
/// known, and what it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub layer: String,
    pub severity: &'static str,
    pub item: Option<String>,
    pub at: Option<dereth_pack::Loc>,
    pub text: String,
}

impl Said {
    /// As one line of JSON: `layer`, `severity`, `text`, and `item`, `file` and `line` when known.
    #[must_use]
    pub fn json(&self) -> String {
        let mut o = serde_json::json!({
            "layer": self.layer,
            "severity": self.severity,
            "text": self.text,
        });
        if let Some(i) = &self.item {
            o["item"] = i.clone().into();
        }
        if let Some(at) = &self.at {
            o["file"] = at.file.display().to_string().into();
            o["line"] = at.line.into();
        }
        o.to_string()
    }
}

/// The item a line of the check begins with (`<item>: ...`), when it begins with one.
fn item_said(text: &str) -> Option<String> {
    let (head, _) = text.split_once(": ")?;
    empyrean_world_db::layers::item_form(head)
}

/// The item a fold's finding is of.
fn item_found(x: &dereth_pack::fold::Finding) -> Option<String> {
    use dereth_pack::fold::Finding as F;
    match x {
        F::Conflict(c) => empyrean_world_db::layers::item_form(&c.what).or_else(|| {
            c.what
                .rsplit_once(' ')
                .and_then(|(i, _)| empyrean_world_db::layers::item_form(i))
        }),
        F::Drift(d) => Some(d.item.clone()),
        F::SetTwice { item, .. }
        | F::StaleRemoval { item, .. }
        | F::StaleAck { item, .. }
        | F::NoOp { item, .. }
        | F::Shadowed { item, .. } => Some(item.clone()),
    }
}

/// Everything the check says, layer by layer; how many layers there are, how many have nothing to
/// say, and how many errors there were.
#[must_use]
pub fn check_said(f: Folding) -> (Vec<Said>, usize, usize, usize) {
    let layers = f.len();
    let mut out = Vec::new();
    let mut errors = 0;
    let mut quiet = 0;
    for l in &f.check() {
        let before = out.len();
        let said = |severity: &'static str, item, at, text: String| Said {
            layer: l.id.clone(),
            severity,
            item,
            at,
            text,
        };
        for x in &l.findings {
            let severity = match x.severity() {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Report => "report",
            };
            if x.severity() == Severity::Error {
                errors += 1;
            }
            out.push(said(
                severity,
                item_found(x),
                x.at().cloned(),
                x.to_string(),
            ));
        }
        errors += l.problems.len();
        for p in &l.problems {
            out.push(said("error", item_said(p), None, p.clone()));
        }
        for p in &l.inherited {
            out.push(said(
                "warning",
                item_said(p),
                None,
                format!("{p} (its source's own)"),
            ));
        }
        for w in &l.warnings {
            out.push(said("warning", item_said(w), None, w.clone()));
        }
        for r in &l.reports {
            out.push(said("report", item_said(r), None, r.clone()));
        }
        if out.len() == before {
            quiet += 1;
        }
    }
    (out, layers, quiet, errors)
}

/// Each layer's errors, warnings and reports, as lines; and how many errors there were.
#[must_use]
pub fn check_lines(f: Folding) -> (Vec<String>, usize) {
    let (said, layers, quiet, errors) = check_said(f);
    let mut lines = Vec::new();
    let mut layer: Option<&str> = None;
    for s in &said {
        if layer != Some(s.layer.as_str()) {
            lines.push(format!("{}:", s.layer));
            layer = Some(&s.layer);
        }
        lines.push(format!("  {}: {}", s.severity, s.text));
    }
    lines.push(format!(
        "{layers} layers, {quiet} with nothing to say; {errors} errors"
    ));
    (lines, errors)
}

fn run_check(args: &[String]) -> Result<bool, String> {
    let mut world: Option<PathBuf> = None;
    let mut dumps: Option<PathBuf> = None;
    let mut dats: Vec<PathBuf> = Vec::new();
    let mut json = false;
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
            "--json" => json = true,
            _ => return Err(USAGE.into()),
        }
    }
    let world = world.ok_or(USAGE)?;
    let mut f = empyrean_world_db::world::open(&world, dumps.as_deref())?;
    if !dats.is_empty() {
        let d = Dats::scan(&dats)?;
        if !json {
            println!("{}", d.summary());
        }
        f = f.with_dats(d);
    }
    if json {
        // One line of JSON for each thing said, then one of the counts.
        let (said, layers, quiet, errors) = check_said(f);
        for s in &said {
            println!("{}", s.json());
        }
        println!(
            "{}",
            serde_json::json!({ "layers": layers, "quiet": quiet, "errors": errors })
        );
        return Ok(errors == 0);
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

const USAGE: &str = "usage: empyrean-import world check --world <world.toml> [--json] [--dumps <folder>] [--dats <folder>]...\n       empyrean-import world build --world <world.toml> [--layer <id>] --out <world.pack> [--sql <folder>] [--dumps <folder>] [--dats <folder>]...\n       empyrean-import world release --world <world.toml> --version <x.y.z> --out <folder> [--previous <manifest.json>] [--dumps <folder>] [--dats <folder>]...\n       empyrean-import world cutover --world <world.toml> --layer <id> --shard <shard database>";

/// `empyrean-import world ...`.
pub fn run(args: &[String]) -> ExitCode {
    let result = match args.first().map(String::as_str) {
        Some("check") => run_check(&args[1..]),
        Some("build") => run_build(&args[1..]),
        Some("release") => release::run(&args[1..]),
        Some("cutover") => cutover::run(&args[1..]),
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

pub mod cutover;
pub mod release;

#[cfg(test)]
mod tests;
