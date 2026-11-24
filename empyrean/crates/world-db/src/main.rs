//! `empyrean-world-db`: a world pack's rows written out, a world of rows folded into a pack, and
//! the two compared.
//!
//! ```text
//! empyrean-world-db rows <world.pack> <empty folder> [--source <project> --revision <commit>]
//! empyrean-world-db pack <world.toml | folder of rows> <out.pack>
//! empyrean-world-db check <world.toml | folder of rows> <world.pack>
//! empyrean-world-db checkpoints <world.toml> [--keys]
//! ```
//!
//! `--dumps <folder>`, anywhere, names where a checkpoint's pinned dumps are; without it, the folder
//! `EMPYREAN_WORLD_DUMPS` names, else the per-user cache `empyrean-import fetch` fills.
//!
//! `rows` writes every record of a pack as rows, the same bytes for the same pack; `--source` and
//! `--revision` say where they came from. `pack` folds a world's layers (or reads one folder of
//! rows) and builds its pack, numbered by rule. `check` builds that pack and lists every record of
//! the given pack it doesn't hold field for field, ids aside; it fails on any.

use std::path::Path;
use std::process::ExitCode;

use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;
use empyrean_world_db::compare::{dataset_of, differences};
use empyrean_world_db::files::{default_stamp, set_source};
use empyrean_world_db::{from_pack, number, read_dir, world, write, write_dir};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The world `at` names, folded and numbered: a `world.toml`'s layers, or one folder of rows.
fn world_at(at: &Path, dumps: Option<&Path>) -> Result<WorldContent, String> {
    let mut c = if at.is_file() {
        empyrean_world_db::world::fold_world(at, dumps)?
    } else {
        return world(&read_dir(at)?);
    };
    number(&mut c);
    Ok(c)
}

/// The world's pack, its dataset id from its rows written back.
fn pack_of(c: &WorldContent) -> Result<Vec<u8>, String> {
    let files = write(c, default_stamp())?;
    c.to_pack(dataset_of(&files))
        .map(|(bytes, _)| bytes)
        .map_err(|e| e.to_string())
}

fn run(args: &[String]) -> Result<bool, String> {
    let usage = "usage: empyrean-world-db rows <world.pack> <folder> [--source <project> --revision <commit>] | pack <world> <out.pack> | check <world> <world.pack> | checkpoints <world.toml> [--keys]; any of them with --dumps <folder>";
    let mut args = args.to_vec();
    let dumps = match args.iter().position(|a| a == "--dumps") {
        Some(i) if i + 1 < args.len() => {
            let dir = args.remove(i + 1);
            args.remove(i);
            Some(std::path::PathBuf::from(dir))
        }
        Some(_) => return Err(usage.into()),
        None => None,
    };
    let dumps = dumps.as_deref();
    match args.as_slice() {
        [cmd, pack, out, rest @ ..] if cmd == "rows" => {
            let p = Pack::open(Path::new(pack)).map_err(|e| e.to_string())?;
            let mut c = from_pack(&p).map_err(|e| e.to_string())?;
            number(&mut c);
            let mut files = write(&c, default_stamp())?;
            match rest {
                [] => {}
                [s, source, r, revision] if s == "--source" && r == "--revision" => {
                    set_source(&mut files, source, revision)?;
                }
                _ => return Err(usage.into()),
            }
            write_dir(Path::new(out), &files)?;
            println!("{} files written to {out}", files.len());
            Ok(true)
        }
        [cmd, at, out] if cmd == "pack" => {
            let bytes = pack_of(&world_at(Path::new(at), dumps)?)?;
            std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
            let p = Pack::from_bytes(bytes).map_err(|e| e.to_string())?;
            println!("{out}: content hash {}", hex(&p.header().content_hash));
            Ok(true)
        }
        [cmd, at, pack] if cmd == "check" => {
            let original = Pack::open(Path::new(pack)).map_err(|e| e.to_string())?;
            let rebuilt = Pack::from_bytes(pack_of(&world_at(Path::new(at), dumps)?)?)
                .map_err(|e| e.to_string())?;
            let found = differences(&original, &rebuilt).map_err(|e| e.to_string())?;
            for line in &found {
                println!("{line}");
            }
            println!(
                "{pack} {}; the rows' pack {}; {} difference(s), ids aside",
                hex(&original.header().content_hash),
                hex(&rebuilt.header().content_hash),
                found.len()
            );
            Ok(found.is_empty())
        }
        [cmd, at, rest @ ..] if cmd == "checkpoints" => {
            let fresh = match rest {
                [] => false,
                [k] if k == "--keys" => true,
                _ => return Err(usage.into()),
            };
            checkpoints(Path::new(at), fresh, dumps)
        }
        _ => Err(usage.into()),
    }
}

/// `checkpoints`: each checkpoint's keys (written to its `keys.toml` with `--keys`), what it
/// changed of the snapshot before it by kind, and what its fold finds; false when a layer has an
/// error.
fn checkpoints(at: &Path, fresh: bool, dumps: Option<&Path>) -> Result<bool, String> {
    use empyrean_world_db::checkpoint::{change_by_kind, count_by_kind, keys_text};
    use empyrean_world_db::layers::{fold_inputs, snapshot_of};
    use empyrean_world_db::world::{layers_of, KeysFrom};
    let from = if fresh {
        KeysFrom::Fresh
    } else {
        KeysFrom::Files
    };
    let (layers, _, read) = layers_of(at, from, dumps)?;
    let dir = at.parent().unwrap_or(Path::new("."));
    if fresh {
        for c in &read {
            let path = dir.join("patches").join(&c.id).join("keys.toml");
            std::fs::write(&path, keys_text(&c.keys, &c.pin))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            println!("{}: {} keys written", path.display(), c.keys.map.len());
        }
    }
    let (base, _) = snapshot_of(&layers.base)?;
    let mut prev = (String::from("base"), &base);
    for p in &layers.patches {
        let Some(c) = &p.checkpoint else { continue };
        println!("\n## {} (over {})", p.id, prev.0);
        if let Some(k) = read.iter().find(|r| r.id == p.id) {
            let m = k.keys.matched;
            println!(
                "pin: {} {}; keys: {} the same, {} near, {} new kept, {} new renumbered; {} differ from the source's guids",
                k.pin.source, k.pin.tag, m.same, m.near, m.new_kept, m.new_renumbered, k.keys.map.len()
            );
        }
        let before = count_by_kind(prev.1);
        let after = count_by_kind(&c.snapshot);
        println!("| kind | before | after | added | removed | changed | fields | restamped only |");
        println!("|---|---:|---:|---:|---:|---:|---:|---:|");
        let changes = change_by_kind(prev.1, &c.snapshot);
        for (kind, ch) in &changes {
            println!(
                "| {kind} | {} | {} | {} | {} | {} | {} | {} |",
                before.get(kind).copied().unwrap_or(0),
                after.get(kind).copied().unwrap_or(0),
                ch.added,
                ch.removed,
                ch.changed,
                ch.fields,
                ch.restamped
            );
        }
        println!("\nThe fields changed most, by kind:");
        for (kind, ch) in &changes {
            let mut top: Vec<(&String, &usize)> = ch.by_field.iter().collect();
            top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            if !top.is_empty() {
                let words: Vec<String> = top
                    .iter()
                    .take(6)
                    .map(|(f, n)| format!("{f} {n}"))
                    .collect();
                println!("- {kind}: {}", words.join(", "));
            }
        }
        prev = (p.id.clone(), &c.snapshot);
    }
    drop(base);
    let (folded, _, _) = fold_inputs(layers)?;
    let mut clean = true;
    println!("\n## What the fold finds");
    for l in &folded.layers {
        let mut by: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for f in &l.findings {
            use dereth_pack::fold::Finding as F;
            let word = |item: &str| item.split_whitespace().next().unwrap_or("").to_owned();
            let what = match f {
                F::Drift(d) => format!(
                    "drift, {} {}",
                    word(&d.item),
                    if d.field.is_some() { "fields" } else { "items" }
                ),
                F::Conflict(c) => format!("conflicts, {}", word(&c.what)),
                F::SetTwice { item, .. } => format!("set twice in one pack, {}", word(item)),
                F::StaleRemoval { item, .. } => format!("stale removals, {}", word(item)),
                F::StaleAck { item, .. } => format!("acknowledgements of no drift, {}", word(item)),
                F::NoOp { item, .. } => format!("changes to nothing, {}", word(item)),
                F::Shadowed { item, .. } => format!("edits a later layer changes, {}", word(item)),
            };
            *by.entry(what).or_default() += 1;
        }
        if l.errors().next().is_some() {
            clean = false;
        }
        for (what, n) in by {
            println!("{}: {n} {what}", l.id);
        }
    }
    println!("\n{}", if clean { "no errors" } else { "errors found" });
    Ok(clean)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("empyrean-world-db: {e}");
            ExitCode::from(2)
        }
    }
}
