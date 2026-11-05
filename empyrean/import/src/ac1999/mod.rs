//! `ac1999`: a revision-pinned ACE-schema candidate and its Launch-era pack.
//!
//! Uses committed Git objects and the existing SQL engine; never executes upstream scripts or
//! connects to MySQL. The candidate preserves unsupported rows and external loot inputs, and
//! requires explicit acceptance of incomplete gameplay support.

mod cells;
mod convert;
mod loot;
mod package;
mod recipes;
mod references;
mod source;
mod world;

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use empyrean_common::era::EraId;
use empyrean_content::import::{self, patch, Allow};
use serde_json::json;

#[derive(Debug)]
struct Options {
    revision: String,
    source: Option<PathBuf>,
    out: PathBuf,
    target: cells::Target,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let (mut revision, mut source, mut out, mut incomplete) = (None, None, None, false);
    let mut target = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--allow-incomplete" {
            incomplete = true;
            continue;
        }
        let value = it.next().ok_or_else(|| format!("{arg} takes a value"))?;
        match arg.as_str() {
            "--revision" if revision.is_none() => revision = Some(value.to_ascii_lowercase()),
            "--source" if source.is_none() => source = Some(PathBuf::from(value)),
            "--out" if out.is_none() => out = Some(PathBuf::from(value)),
            "--target" if target.is_none() => target = Some(cells::Target::parse(value)?),
            _ => return Err(format!("unknown or repeated ac1999 option {arg}")),
        }
    }
    let revision = revision.ok_or("--revision requires a full ac1999 commit SHA")?;
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("--revision requires exactly 40 hexadecimal characters".into());
    }
    if !incomplete {
        return Err("this candidate quarantines unsupported recipes and broken direct references; review the documentation and use --allow-incomplete to build it".into());
    }
    Ok(Options {
        revision,
        source,
        target: target.unwrap_or_default(),
        out: out.ok_or("--out requires a new output directory")?,
    })
}

pub fn run(args: &[String]) -> ExitCode {
    if args.first().is_some_and(|a| a == "package") {
        return package::run(&args[1..]);
    }
    match parse(args).and_then(|o| build(&o)) {
        Ok(out) => {
            println!(
                "wrote incomplete Launch candidate to {}; review manifest.json and changes.json",
                out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("empyrean-import ac1999: {e}");
            ExitCode::from(2)
        }
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

fn build(opts: &Options) -> Result<PathBuf, String> {
    if opts.out.exists() {
        return Err("output already exists; choose a new directory".into());
    }
    let parent = opts
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let name = opts
        .out
        .file_name()
        .ok_or("output requires a directory name")?;
    let out = parent.join(name);
    let stage = parent.join(format!(
        "{}.partial-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    std::fs::create_dir(&stage).map_err(|e| format!("create staging directory: {e}"))?;
    // A failed build keeps its isolated staging directory for inspection; it never publishes out.
    let (root, files) = source::snapshot(opts.source.as_deref(), &opts.revision, &stage)?;
    for path in files.keys() {
        if path.starts_with(source::DATABASE)
            && path.to_ascii_lowercase().ends_with(".sql")
            && path != &format!("{}/1-Base/WorldBase.sql", source::DATABASE)
            && !path.starts_with(&format!("{}/3-Core/", source::DATABASE))
        {
            return Err(format!("unreviewed SQL layer {path}"));
        }
    }
    let database = root.join(source::DATABASE);
    let schema_bytes =
        std::fs::read(database.join("1-Base/WorldBase.sql")).map_err(|e| e.to_string())?;
    convert::validate_schema(&schema_bytes)?;
    let inputs = [patch::Input {
        kind: patch::InputKind::Sql,
        path: database.join("3-Core"),
    }];
    let loaded = import::load_store(
        std::fs::File::open(database.join("1-Base/WorldBase.sql")).map_err(|e| e.to_string())?,
        patch::sources(&inputs).map(|src| {
            let src = src?;
            convert::validate_content(&src)?;
            Ok(src)
        }),
        import::default_now(),
    )
    .map_err(|e| e.to_string())?;
    if !loaded.skipped_statements.is_empty() {
        return Err("unread source schema statements".into());
    }
    if let Ok(index) = loaded.store.table_index("version") {
        if loaded
            .store
            .table(index)
            .is_some_and(|table| !table.is_empty())
        {
            return Err("upstream now supplies version rows; review lineage conversion".into());
        }
    }
    let canonical = import::load_store(convert::SCHEMA, std::iter::empty(), import::default_now())
        .map_err(|e| e.to_string())?;
    if !canonical.skipped_statements.is_empty() {
        return Err("unread canonical schema statements".into());
    }
    let census = convert::Census::read(&loaded.store).map_err(|e| e.to_string())?;
    let settled = census.references.settle().map_err(|e| e.to_string())?;
    let mut policy = world::Policy::new(opts.target);
    policy
        .settle(&census.placements)
        .map_err(|e| e.to_string())?;
    let sql_path = stage.join("world.sql");
    let mut sql = BufWriter::new(std::fs::File::create(&sql_path).map_err(|e| e.to_string())?);
    convert::write_schema(&mut sql, &canonical.store)?;
    let mut projection = convert::Projection::new(sql, &canonical.store, census);
    projection.cells = cells::Adaptation::new(opts.target);
    projection.world = policy;
    projection.references = settled;
    loaded
        .store
        .emit(&mut projection)
        .map_err(|e| e.to_string())?;
    projection.finish().map_err(|e| e.to_string())?;
    projection.cells.complete().map_err(|e| e.to_string())?;
    projection.world.complete().map_err(|e| e.to_string())?;
    let world_gone: Vec<String> = projection.world.gone.iter().cloned().collect();
    projection
        .references
        .write(&mut projection.output, &mut projection.ledger)
        .map_err(|e| e.to_string())?;
    writeln!(projection.output,"INSERT INTO `version` (`id`,`base_Version`,`patch_Version`,`last_Modified`) VALUES (1,'ac99:{}',NULL,'2000-01-01 00:00:00');\nSET FOREIGN_KEY_CHECKS=1;",opts.revision)
        .map_err(|e|e.to_string())?;
    projection.output.flush().map_err(|e| e.to_string())?;
    drop(projection.output);
    // Validate widths, numeric ranges, unique keys and row types as target SQL, not just pack fields.
    let validated = import::load_store(
        &b""[..],
        patch::sources(&[patch::Input {
            kind: patch::InputKind::Sql,
            path: sql_path.clone(),
        }]),
        import::default_now(),
    )
    .map_err(|e| e.to_string())?;
    if !validated.skipped_statements.is_empty() {
        return Err("unread projected SQL statements".into());
    }
    let (mut recipe_sql, mut recipe_changes, restored_recipes) = recipes::patch(&projection.ledger);
    let (loot_sql, loot_changes, loot_policy) = loot::patch(
        &root.join(source::TREASURE),
        &opts.revision,
        &projection.ledger,
    )?;
    let restored_treasure = loot_changes.len();
    // A strict check of what was written: no treasure or emote-spell reference may name nothing.
    let mut written = references::Index::default();
    validated
        .store
        .emit(&mut written)
        .map_err(|e| e.to_string())?;
    let death_types = loot_changes
        .iter()
        .filter_map(|c| c["treasure_type"].as_i64().map(i128::from))
        .collect();
    written.verify(&death_types).map_err(|e| e.to_string())?;
    recipe_sql.extend_from_slice(&loot_sql);
    recipe_changes.extend(loot_changes);
    std::fs::write(stage.join("empyrean.sql"), &recipe_sql).map_err(|e| e.to_string())?;
    let recipe_source = patch::Source {
        kind: patch::InputKind::Sql,
        path: PathBuf::from("empyrean.sql"),
        bytes: recipe_sql.clone(),
    };
    let era = EraId::parse("launch").ok_or("this build lacks the Launch era profile")?;
    let (pack, imported) = import::build_from_for(
        std::fs::File::open(&sql_path).map_err(|e| e.to_string())?,
        [Ok(recipe_source)],
        import::default_now(),
        era,
    )
    .map_err(|e| e.to_string())?;
    imported
        .verify(Allow::default())
        .map_err(|e| e.to_string())?;
    if !imported.orphans.is_empty() || !imported.missing_tables.is_empty() {
        return Err("projected content has assembly or schema gaps".into());
    }
    loot::validate_references(&loot_policy, &pack)?;
    std::fs::write(stage.join("world.pack"), &pack).map_err(|e| e.to_string())?;
    write_json(&stage.join("pack-report.json"), &imported.report_json())?;
    write_json(&stage.join("changes.json"), &json!(projection.ledger))?;
    write_json(&stage.join("empyrean-changes.json"), &json!(recipe_changes))?;
    // Preserve the complete committed input and license, including unsupported treasure data.
    let normalized = projection
        .ledger
        .iter()
        .filter(|r| r["action"] == "normalize")
        .count();
    let quarantined = projection
        .ledger
        .iter()
        .filter(|r| r["action"] == "quarantine")
        .count();
    let adapted = projection
        .ledger
        .iter()
        .filter(|r| r["action"] == "adapt")
        .count();
    let manifest = json!({
        "format":"empyrean-ac1999-candidate-v5", "converter_version":env!("CARGO_PKG_VERSION"),
        "converter":{"version":env!("CARGO_PKG_VERSION"),"commit":env!("EMPYREAN_IMPORT_COMMIT"),
            "cargo_lock_sha256":env!("EMPYREAN_IMPORT_CARGO_LOCK_SHA256"),
            "toolchain":env!("EMPYREAN_IMPORT_TOOLCHAIN")},
        "source":{"url":source::URL,"revision":opts.revision,"files_sha256":files},
        "schema":{"url":"https://github.com/ACEmulator/ACE","revision":convert::SCHEMA_REVISION,
            "path":"Database/Base/WorldBase.sql","sha256":source::hash(convert::SCHEMA)},
        "source_schema":{"revision":convert::SOURCE_SCHEMA_REVISION,"sha256":source::hash(convert::SOURCE_SCHEMA),
            "comparison":"SQL tokens; comments and whitespace ignored"},
        "era":"launch", "status":"incomplete", "ready_for_gameplay":false,
        "asset_target":opts.target.manifest(), "adapted_rows":adapted,
        "world_policy":{"name":world::POLICY,"reviewed_but_gone":world_gone},
        "normalized_rows":normalized,"quarantined_rows":quarantined,
        "empyrean_restored_recipes":restored_recipes,
        "empyrean_restored_treasure":restored_treasure,
        "loot_algorithm":empyrean_content::loot::ALGORITHM,
        "loot_algorithm_revision":empyrean_content::loot::SOURCE_REVISION,
        "empyrean_sql_sha256":source::hash(&recipe_sql),
        "projected_rows":projection.counts,
        "world_sql_sha256":source::hash(&std::fs::read(&sql_path).map_err(|e|e.to_string())?),
        "world_pack_sha256":source::hash(&pack),
        "limits":["Source-pinned six-group loot is executable; launch-era probability accuracy remains unproven.",
            "ACE SQL quarantines extended recipes. Apply empyrean.sql to restore supported vital-cost recipes with their preconditions.",
            "Direct spawn/create-list/generator/wielded-loot/spellbook, death and wielded treasure type and emote spell references are checked.",
            "The Launch profile and historical rule validation are still incomplete.",
            "No retail DATs are included; upstream licensing and data provenance still apply."],
        "provenance":"source/ contains the exact committed inputs; changes.json names transformed/quarantined table rows."
    });
    write_json(&stage.join("manifest.json"), &manifest)?;
    // The fetched repository is acquisition scratch, not part of the bundle.
    for scratch in ["repository", "bundle"] {
        if stage.join(scratch).exists() {
            std::fs::remove_dir_all(stage.join(scratch)).map_err(|e| e.to_string())?;
        }
    }
    std::fs::rename(&stage, &out).map_err(|e| e.to_string())?;
    Ok(out)
}
