//! The `world.pack` builder: builds the pack from ACE's world database dump, compares two packs,
//! and reports what the server's corrections do to one.
//!
//! **Depends on** `empyrean-common` and the content crate (`empyrean-content`), whose importer it
//! runs, for `dat-overlay` on the data-file container (`dereth-dat`, `dereth-primitives`), and for
//! `fetch` on an HTTP client (`ureq`, over rustls), `zip` and `sha2`. **Used by**
//! nothing: it is the `empyrean-import` binary.
//!
//! **Must never** read the wall clock or overwrite the pack an overlay sits on: `--now` (default
//! 2000-01-01 00:00:00) stands in for every timestamp MySQL or ACE would stamp, and is the only way
//! to set it (no environment variable is read), so a build is reproducible.
//!
//! ```text
//! empyrean-import fetch [--world <world>] [--version <n> | --latest] [--dir <folder>] [--pack [--out <world.pack>] [--report <report.json>]]
//! empyrean-import --sql <dump.sql> [--patches <dir|file>]... [--json <dir|file>]...
//!             [--now <YYYY-MM-DD HH:MM:SS>] [--era <era>] [--allow-skipped] [--allow-unknown-tables]
//!             [--allow-unknown-columns] --out <world.pack> [--report <report.json>]
//! empyrean-import --check <old.pack> <new.pack> [--fields] [--overlap [<dir|file>]]... [--overlap-json <dir|file>]...
//!             [--overlap-overlay <overlay.sqlite>] [--sql <new dump.sql>] [--report <diff.json>]
//! empyrean-import --check <old.pack> --sql <dump.sql> [--patches …] [--json …] [--now …] [--fields] [--overlap …]
//!             [--report <diff.json>]
//! empyrean-import --corrections <world.pack> [--report <corrections.json>]
//! empyrean-import --version
//! empyrean-import dat-overlay --base <dir> --world <dir> --out <dir> --world-key <name> [--era <era>]
//! empyrean-import --sql <dump.sql> [--patches …] [--json …] [--era <era>] --overlay <overlay.sqlite>
//!             [--overlay-journal <dir>] --out <world.pack> [--report <report.json>]
//! ```
//!
//! `--era` names the era the content is for (`eor`, the default, or `infiltration`); the pack's
//! header records it, and the server refuses a pack whose era is not its configured `[era]
//! profile`.
//!
//! `fetch` downloads the ACE-World release this build pins (or `--version <n>`, or `--latest`)
//! from GitHub, checks its SHA-256, and caches the dump in the per-user cache folder the
//! real-dump tests also read; `--pack` then builds `world.pack` from it, for the world's era.
//! `--world` picks the world database: `patches` (the default: the end of retail) or `16py` (the
//! February 2005 world). See [`fetch`].
//!
//! The base is a full `mysqldump` of ACE's world database. `--patches` and `--json` inputs apply
//! over it in command-line order: ACE-style per-object SQL files (`DELETE` + `INSERT`, and
//! `UPDATE`), and ACE's JSON content (weenies, recipes, landblocks, quests) converted as ACE's
//! `import-json` converts it. A directory means every `*.sql` (or `*.json`) beneath it, in sorted
//! path order; later inputs override earlier ones. The report (rows per table, records per pack
//! table, what each input added, replaced and deleted, and the content hash) is always written with
//! the pack. A pack already at `--out` is kept beside it as `<out>.backup-<UTC timestamp>` before
//! the new one replaces it, and only the newest three such backups are kept.
//!
//! The dump may be `mysqldump`'s or SQLyog's (or any plain SQL dump): it is read statement by
//! statement, whatever its line layout. An import that leaves anything unread writes no pack: no
//! rows at all, or none for `weenie` or `landblock_instance`, always; statements the reader cannot
//! read (`REPLACE`, `UPDATE`, `INSERT … SELECT`, …) unless `--allow-skipped`; tables that are not
//! ACE world tables unless `--allow-unknown-tables`; and columns no ACE world model has unless
//! `--allow-unknown-columns`. Each is named, with its count; what was allowed is still printed.
//! `--check <old.pack> --sql <dump.sql>` refuses the same dumps and takes the same flags.
//!
//! `--overlay` publishes a content overlay: the journal the server's developer content commands
//! wrote over the pack built from the same inputs is applied after them, with the overlay's own
//! clock (so no `--now`). The base is checked against the one the overlay was made over, and the
//! pack the overlay sits on is never overwritten. `--overlay-journal` also writes the journal as
//! numbered SQL files, which rebuild the same pack as `--patches <dir>` after the base inputs.
//!
//! `--check` writes no pack: it lists the records the new pack (or what the inputs would build)
//! adds, changes and removes relative to the old one. `--fields` adds the fields that changed, old
//! -> new. `--overlap` adds the changed records our own content also touches, since ours would
//! override upstream's newer version: the corrections compiled into this build always, plus the
//! content files given to `--overlap` / `--overlap-json` and the records of an `--overlap-overlay`
//! file. Compare packs built from ACE's dumps alone, so the diff is upstream's change and not ours.
//! Exit status 0 means no differences, 1 differences, 2 a usage or input error.
//!
//! `dat-overlay` writes a world's data overlay: see [`dat_overlay`].
//!
//! `--corrections` lists every correction entry of this build (applies, stale or absent, with the
//! stored and corrected values and the divergence row), every value each correction rule changes,
//! and the corrections digest the server logs at start-up. Exit status 0 means every entry applies,
//! 1 that some entry is stale or absent, 2 an error.

mod dat_overlay;
mod fetch;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use empyrean_common::backups;
use empyrean_common::clock::{Clock, SystemClock};
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::era::EraId;
use empyrean_common::world_release;
use empyrean_content::corrections::report::CorrectionsReport;
use empyrean_content::import::check::{fields, overlap};
use empyrean_content::import::patch::{Input, InputKind};
use empyrean_content::import::{self, check, Allow, Build};
use empyrean_content::pack::{self, Pack};
use empyrean_content::PackContent;

fn usage() -> ExitCode {
    eprintln!(
        "usage: empyrean-import fetch [--world patches|16py] [--version <n> | --latest] [--dir <folder>] [--pack [--out <world.pack>] [--report <report.json>]]\n\
         \x20                  (downloads ACE-World {pin} by default into {cache})\n\
         \x20      empyrean-import --sql <dump.sql> [--patches <dir|file>]... [--json <dir|file>]...\n\
         \x20                  [--now <YYYY-MM-DD HH:MM:SS>] [--era eor|infiltration] [--allow-skipped] [--allow-unknown-tables]\n\
         \x20                  [--allow-unknown-columns] --out <world.pack> [--report <report.json>]\n\
         \x20                  (--now defaults to 2000-01-01 00:00:00)\n\
         \x20      empyrean-import --check <old.pack> <new.pack> [--fields] [--overlap [<dir|file>]]... [--overlap-json <dir|file>]...\n\
         \x20                  [--overlap-overlay <overlay.sqlite>] [--sql <new dump.sql>] [--report <diff.json>]\n\
         \x20      empyrean-import --check <old.pack> --sql <dump.sql> [--patches ...] [--json ...] [--now ...] [--fields] [--overlap ...]\n\
         \x20                  [--report <diff.json>]\n\
         \x20      empyrean-import --corrections <world.pack> [--report <corrections.json>]\n\
         \x20      empyrean-import --version\n\
         \x20      empyrean-import dat-overlay --base <dir> --world <dir> --out <dir> --world-key <name> [--era eor|infiltration]\n\
         \x20                  (a world's data files taken apart against their base, as the overlay the server serves)\n\
         \x20      empyrean-import --sql <dump.sql> [--patches ...] [--json ...] [--era ...] --overlay <overlay.sqlite> [--overlay-journal <dir>]\n\
         \x20                  --out <world.pack> [--report <report.json>]\n\
         \x20  --era      the era the pack is for, recorded in it: eor (default, the end of retail) or infiltration\n\
         \x20  --fields   under each changed record, the fields that changed (old -> new)\n\
         \x20  --overlap  the changed records our corrections (always) and the given content files or overlay also touch\n\
         \x20  --allow-skipped, --allow-unknown-tables, --allow-unknown-columns\n\
         \x20             build the pack although the dump has statements the reader cannot read, tables that\n\
         \x20             are not ACE world tables, or columns no world model reads (refused otherwise)",
        pin = world_release::PINNED_TAG,
        cache = world_release::cache_dir()
            .map_or_else(|| "the per-user cache folder".to_owned(), |d| d.display().to_string()),
    );
    ExitCode::from(2)
}

fn parse_now(s: &str) -> Option<DotNetDateTime> {
    let s = s.trim();
    let n = |r: core::ops::Range<usize>| s.get(r).and_then(|t| t.parse::<i32>().ok());
    let b = s.as_bytes();
    if s.len() == 10 || s.len() == 19 {
        if b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let (y, mo, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
        let (h, mi, se) = if s.len() == 19 {
            (n(11..13)?, n(14..16)?, n(17..19)?)
        } else {
            (0, 0, 0)
        };
        if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 59 {
            return None;
        }
        return Some(DotNetDateTime::new_hms(y, mo, d, h, mi, se));
    }
    None
}

/// What `--check` adds to the record list.
#[derive(Default)]
struct CheckExtras {
    fields: bool,
    /// Any `--overlap` flag was given.
    overlap: bool,
    overlap_inputs: Vec<Input>,
    overlap_overlay: Option<PathBuf>,
}

fn main() -> ExitCode {
    let mut sql: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut report: Option<PathBuf> = None;
    let mut now: Option<DotNetDateTime> = None;
    let mut inputs: Vec<Input> = Vec::new();
    let mut check_packs: Option<Vec<PathBuf>> = None;
    let mut corrections: Option<PathBuf> = None;
    let mut extras = CheckExtras::default();
    let mut overlay: Option<PathBuf> = None;
    let mut overlay_journal: Option<PathBuf> = None;
    let mut allow = Allow::default();
    let mut era: Option<EraId> = None;
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().is_some_and(|a| a == "--version") {
        print!(
            "{}",
            empyrean_common::brand::version_text("empyrean-import")
        );
        return ExitCode::SUCCESS;
    }
    if argv.first().is_some_and(|a| a == "fetch") {
        return run_fetch(&argv[1..]);
    }
    if argv.first().is_some_and(|a| a == "dat-overlay") {
        return dat_overlay::run(&argv[1..]);
    }
    let mut args = argv.into_iter().peekable();
    while let Some(a) = args.next() {
        // Switches, and `--overlap` with or without a path.
        match a.as_str() {
            "--fields" => {
                extras.fields = true;
                continue;
            }
            "--allow-skipped" => {
                allow.skipped_statements = true;
                continue;
            }
            "--allow-unknown-tables" => {
                allow.unknown_tables = true;
                continue;
            }
            "--allow-unknown-columns" => {
                allow.unknown_columns = true;
                continue;
            }
            "--overlap" => {
                extras.overlap = true;
                if args.peek().is_some_and(|n| !n.starts_with("--")) {
                    let path = PathBuf::from(args.next().expect("peeked"));
                    extras.overlap_inputs.push(Input {
                        kind: InputKind::Sql,
                        path,
                    });
                }
                continue;
            }
            _ => {}
        }
        let Some(v) = args.next() else { return usage() };
        match a.as_str() {
            "--sql" => sql = Some(PathBuf::from(v)),
            "--out" => out = Some(PathBuf::from(v)),
            "--report" => report = Some(PathBuf::from(v)),
            "--patches" => inputs.push(Input {
                kind: InputKind::Sql,
                path: PathBuf::from(v),
            }),
            "--json" => inputs.push(Input {
                kind: InputKind::Json,
                path: PathBuf::from(v),
            }),
            "--overlay" => overlay = Some(PathBuf::from(v)),
            "--overlay-journal" => overlay_journal = Some(PathBuf::from(v)),
            "--overlap-json" => {
                extras.overlap = true;
                extras.overlap_inputs.push(Input {
                    kind: InputKind::Json,
                    path: PathBuf::from(v),
                });
            }
            "--overlap-overlay" => {
                extras.overlap = true;
                extras.overlap_overlay = Some(PathBuf::from(v));
            }
            "--corrections" => corrections = Some(PathBuf::from(v)),
            "--era" => match EraId::parse(&v) {
                Some(e) => era = Some(e),
                None => {
                    eprintln!("empyrean-import: --era wants eor or infiltration, got {v:?}");
                    return ExitCode::from(2);
                }
            },
            "--now" => match parse_now(&v) {
                Some(d) => now = Some(d),
                None => {
                    eprintln!("empyrean-import: --now wants YYYY-MM-DD HH:MM:SS, got {v:?}");
                    return ExitCode::from(2);
                }
            },
            "--check" => {
                let mut packs = vec![PathBuf::from(v)];
                if args.peek().is_some_and(|n| !n.starts_with("--")) {
                    packs.push(PathBuf::from(args.next().expect("peeked")));
                }
                check_packs = Some(packs);
            }
            _ => return usage(),
        }
    }
    if (extras.fields || extras.overlap) && check_packs.is_none() {
        return usage();
    }
    if let Some(pack) = corrections {
        let (None, None, None, None, true, None, true, None) = (
            sql,
            out,
            &check_packs,
            now,
            inputs.is_empty(),
            &overlay,
            allow == Allow::default(),
            era,
        ) else {
            return usage();
        };
        return run_corrections(&pack, report.as_deref());
    }
    if let Some(overlay) = overlay {
        let (Some(sql), Some(out), None, None, true) =
            (sql, out, &check_packs, now, allow == Allow::default())
        else {
            return usage();
        };
        return run_publish(
            sql,
            inputs,
            era.unwrap_or_default(),
            &overlay,
            &out,
            report.as_deref(),
            overlay_journal.as_deref(),
        );
    }
    if overlay_journal.is_some() {
        return usage();
    }
    let now = now.unwrap_or_else(import::default_now);

    if let Some(packs) = check_packs {
        if out.is_some() || era.is_some() {
            return usage();
        }
        return run_check(&packs, sql, inputs, now, allow, report, &extras);
    }
    let (Some(sql), Some(out)) = (sql, out) else {
        return usage();
    };

    run_build(
        &Build { sql, inputs, now },
        era.unwrap_or_default(),
        allow,
        &out,
        report.as_deref(),
    )
}

/// `empyrean-import fetch …`: fetches the release, then builds the pack when `--pack` asks.
fn run_fetch(args: &[String]) -> ExitCode {
    let opts = match fetch::parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("empyrean-import fetch: {e}");
            return usage();
        }
    };
    let sql = match fetch::fetch(&opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("empyrean-import fetch: {e}");
            return ExitCode::FAILURE;
        }
    };
    match &opts.pack {
        Some((out, report)) => run_build(
            &Build {
                sql,
                inputs: Vec::new(),
                now: import::default_now(),
            },
            opts.world.era,
            Allow::default(),
            out,
            report.as_deref(),
        ),
        None => ExitCode::SUCCESS,
    }
}

/// Keeps the file at `out`, when there is one, as a backup beside it
/// (`<out>.backup-<UTC timestamp>`, [`empyrean_common::backups`]) before a new pack replaces it.
/// The backup is a hard link where the file system allows (the old pack's bytes stay where they
/// are, and the new pack is renamed over `out`), else a copy.
fn keep_old_pack(out: &Path) -> Result<Option<PathBuf>, String> {
    if !out.is_file() {
        return Ok(None);
    }
    let backup = backups::backup_path(out, None, None, SystemClock::new().utc_now());
    std::fs::hard_link(out, &backup)
        .or_else(|_| std::fs::copy(out, &backup).map(|_| ()))
        .map_err(|e| {
            format!(
                "could not keep {} as {} before replacing it: {e}; nothing was written",
                out.display(),
                backup.display()
            )
        })?;
    Ok(Some(backup))
}

/// Builds the pack and says what went in, and what was left unread where `allow` let it be. A pack
/// already at `out` is kept as a backup first; the newest [`backups::KEEP_BACKUPS`] are kept.
fn run_build(b: &Build, era: EraId, allow: Allow, out: &Path, report: Option<&Path>) -> ExitCode {
    let t0 = std::time::Instant::now();
    let built = import::build_for(b, era).and_then(|(bytes, imported)| {
        imported.verify(allow)?;
        Ok((bytes, imported))
    });
    let written = built
        .map_err(|e| e.to_string())
        .and_then(|(bytes, imported)| {
            let backup = keep_old_pack(out)?;
            match import::write_pack_and_report(&bytes, &imported, out, report) {
                Ok(()) => {
                    if let Some(backup) = backup {
                        println!("kept the previous pack as {}", backup.display());
                        for old in backups::prune(out, None, backups::KEEP_BACKUPS) {
                            println!("removed the old backup {}", old.display());
                        }
                    }
                    Ok(imported)
                }
                Err(e) => {
                    if let Some(backup) = backup {
                        let _ = std::fs::remove_file(backup);
                    }
                    Err(e.to_string())
                }
            }
        });
    match written {
        Ok(imported) => {
            let rows: u64 = imported.rows.iter().map(|(_, n)| n).sum();
            println!(
                "wrote {} for era {} ({} bytes, {} records from {} rows) in {:.1}s; content hash {}",
                out.display(),
                imported.stats.era,
                imported.stats.file_len,
                imported.stats.index_count,
                rows,
                t0.elapsed().as_secs_f64(),
                pack::hex(&imported.stats.content_hash),
            );
            if !imported.applied.is_empty() {
                let mut totals: std::collections::BTreeMap<&str, (u64, u64, u64)> =
                    std::collections::BTreeMap::new();
                for a in &imported.applied {
                    for (t, c) in &a.records {
                        let e = totals.entry(t).or_default();
                        e.0 += c.added;
                        e.1 += c.replaced;
                        e.2 += c.deleted;
                    }
                }
                println!("applied {} content files:", imported.applied.len());
                for (t, (a, r, d)) in totals {
                    println!("  {t}: {a} added, {r} replaced, {d} deleted");
                }
            }
            for (t, n) in &imported.orphans {
                println!("warning: {n} orphaned rows in {t}");
            }
            for t in &imported.missing_tables {
                println!("warning: the dump has no table `{t}`");
            }
            for (kind, s) in &imported.skipped_statements {
                println!(
                    "warning: {} {kind} statements not read (first at line {})",
                    s.count, s.first_line
                );
            }
            for (t, n) in &imported.unknown_tables {
                println!("warning: table `{t}` ({n} rows) is not an ACE world table; not read");
            }
            for (t, c) in &imported.unread_columns {
                println!("warning: column `{t}.{c}` is not in ACE's world schema; not read");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("empyrean-import: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_publish(
    sql: PathBuf,
    patches: Vec<Input>,
    era: EraId,
    overlay: &Path,
    out: &Path,
    report: Option<&Path>,
    journal: Option<&Path>,
) -> ExitCode {
    let t0 = std::time::Instant::now();
    let base = empyrean_content::overlay::BaseInputs { sql, patches, era };
    match empyrean_content::overlay::publish(&base, overlay, out, report, journal) {
        Ok(imported) => {
            println!(
                "published {} ({} content files over the base) to {} in {:.1}s; content hash {}",
                overlay.display(),
                imported.applied.len(),
                out.display(),
                t0.elapsed().as_secs_f64(),
                pack::hex(&imported.stats.content_hash),
            );
            if let Some(dir) = journal {
                println!(
                    "wrote the overlay's journal to {} (use it as --patches after the base inputs)",
                    dir.display()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("empyrean-import: {e}");
            ExitCode::FAILURE
        }
    }
}

fn fail(e: &dyn std::fmt::Display) -> ExitCode {
    eprintln!("empyrean-import: {e}");
    ExitCode::from(2)
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), ExitCode> {
    let text = serde_json::to_string_pretty(value).expect("json");
    pack::write_atomically(path, format!("{text}\n").as_bytes()).map_err(|e| fail(&e))
}

fn run_corrections(path: &Path, report: Option<&Path>) -> ExitCode {
    let content = match PackContent::open(path) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let header = content.base().pack().header();
    println!(
        "world.pack {}: content hash {}",
        path.display(),
        pack::hex(&header.content_hash)
    );
    let r = CorrectionsReport::of(content.base());
    print!("{}", r.render());
    if let Some(p) = report {
        if let Err(code) = write_json(p, &r.to_json()) {
            return code;
        }
    }
    let s = r.summary();
    if s.stale + s.absent == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_check(
    packs: &[PathBuf],
    sql: Option<PathBuf>,
    inputs: Vec<Input>,
    now: DotNetDateTime,
    allow: Allow,
    report: Option<PathBuf>,
    extras: &CheckExtras,
) -> ExitCode {
    let old = match Pack::open(&packs[0]) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let new = if let Some(p) = packs.get(1) {
        // With two packs, `--sql` only names the dump `--overlap` content files go over.
        if !inputs.is_empty()
            || (sql.is_some() && extras.overlap_inputs.is_empty())
            || allow != Allow::default()
        {
            return usage();
        }
        match Pack::open(p) {
            Ok(p) => p,
            Err(e) => return fail(&e),
        }
    } else if let Some(sql) = &sql {
        match import::build(&Build {
            sql: sql.clone(),
            inputs,
            now,
        }) {
            Ok((bytes, imported)) => {
                if let Err(e) = imported.verify(allow) {
                    return fail(&e);
                }
                match Pack::from_bytes(bytes) {
                    Ok(p) => p,
                    Err(e) => return fail(&e),
                }
            }
            Err(e) => return fail(&e),
        }
    } else {
        return usage();
    };
    let diffs = match check::diff(&old, &new) {
        Ok(d) => d,
        Err(e) => return fail(&e),
    };
    let changed_fields = if extras.fields {
        match fields::changed_fields(&diffs, &old, &new) {
            Ok(f) => Some(f),
            Err(e) => return fail(&e),
        }
    } else {
        None
    };
    let overlaps = if extras.overlap {
        let mut ours = match overlap::ours_from_corrections(&diffs, &old, &new) {
            Ok(o) => o,
            Err(e) => return fail(&e),
        };
        if !extras.overlap_inputs.is_empty() {
            let Some(base) = &sql else {
                eprintln!("empyrean-import: --overlap <content> needs --sql <dump> to apply it over (the dump the new pack came from)");
                return ExitCode::from(2);
            };
            match overlap::ours_from_patches(base, &extras.overlap_inputs, now) {
                Ok(o) => ours.extend(o),
                Err(e) => return fail(&e),
            }
        }
        if let Some(file) = &extras.overlap_overlay {
            match overlap::ours_from_overlay(file) {
                Ok(o) => ours.extend(o),
                Err(e) => return fail(&e),
            }
        }
        Some(overlap::overlap(&diffs, &ours))
    } else {
        None
    };
    print!(
        "{}",
        check::render_with(
            &diffs,
            changed_fields.as_deref(),
            overlaps.as_deref(),
            &old,
            &new
        )
    );
    if let Some(r) = report {
        let value = check::to_json_with(&diffs, changed_fields.as_deref(), overlaps.as_deref());
        if let Err(code) = write_json(&r, &value) {
            return code;
        }
    }
    if diffs.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
