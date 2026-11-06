//! The world commands: a new world over the world database, its build into the server's tables,
//! its check, and its counts.
//!
//! ```text
//! athenaeum new <folder> [--key KEY]
//! athenaeum build <world> --dats <install> [--base <folder>] [--out <folder>]
//! athenaeum validate <world> [--dats <install>] [--base <folder>]
//! athenaeum facts <world> [--base <folder>]
//! ```
//!
//! `<world>` is a world's folder (or its `world.toml`). Its base is the release of the world
//! database it names, from the per-user cache `yalain world fetch` fills, unless `--base` gives a
//! folder (a checkout of the database).

use std::path::{Path, PathBuf};

use athenaeum_foundry::{build, facts, new, overlay, world};
use yalain_world_db::report;

/// The options after a command's first argument.
#[derive(Debug, Default)]
struct Options {
    dats: Option<PathBuf>,
    base: Option<PathBuf>,
    out: Option<PathBuf>,
    key: Option<String>,
}

fn options(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--dats" => o.dats = Some(PathBuf::from(value()?)),
            "--base" => o.base = Some(PathBuf::from(value()?)),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--key" => o.key = Some(value()?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(o)
}

/// The world's folder an argument names: the folder, or the folder of its `world.toml`.
fn world_dir(arg: &str) -> PathBuf {
    let p = Path::new(arg);
    if p.file_name().is_some_and(|n| n == "world.toml") {
        p.parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    } else {
        p.to_path_buf()
    }
}

/// `athenaeum new`.
fn new_cmd(dir: &str, o: &Options) -> Result<(), String> {
    let dir = PathBuf::from(dir);
    let made = new::new_world(&dir, o.key.as_deref())?;
    println!(
        "{}: world {} over the world database, its edits going into {}",
        dir.display(),
        made.key,
        made.pack
    );
    Ok(())
}

/// `athenaeum build`.
fn build_cmd(dir: &str, o: &Options) -> Result<(), String> {
    let dats = o
        .dats
        .as_deref()
        .ok_or("build needs --dats <install>, the install the world is built against")?;
    let opened = world::open(&world_dir(dir), o.base.as_deref())?;
    let files = build::InstallFiles::open(dats)?;
    let install = files.install()?;
    let t = build::compile_world(&opened, &install)?;
    let out = o.out.clone().unwrap_or_else(|| opened.dir.join("build"));
    build::write(&t, &out)?;
    let (folder, overlay) = overlay::write(&opened, dats, &opened.dir)?;
    if overlay.blocks > 0 {
        println!(
            "overlay: {} landblocks, {} vertices -> {}",
            overlay.blocks,
            overlay.vertices,
            folder.display()
        );
    }
    for w in &t.warnings {
        println!("  {w}");
    }
    println!(
        "{} -> {}",
        t.counts()
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", "),
        out.display()
    );
    Ok(())
}

/// `athenaeum validate`.
fn validate_cmd(dir: &str, o: &Options) -> Result<(), String> {
    let opened = world::open(&world_dir(dir), o.base.as_deref())?;
    let problems = opened.check();
    if !problems.is_empty() {
        return Err(format!(
            "{}\n{} problems",
            report(&problems),
            problems.len()
        ));
    }
    if let Some(dats) = &o.dats {
        let files = build::InstallFiles::open(dats)?;
        let install = files.install()?;
        let t = build::compile_world(&opened, &install)?;
        for w in &t.warnings {
            println!("  {w}");
        }
    }
    let f = facts::facts(&opened);
    let rows: usize = f.rows.values().sum();
    println!(
        "{}: {} packs, {rows} rows over {} of the base: no problems{}",
        f.key,
        f.packs,
        f.base_rows,
        if o.dats.is_some() {
            ", and every row builds against the install"
        } else {
            ""
        }
    );
    Ok(())
}

/// `athenaeum facts`.
fn facts_cmd(dir: &str, o: &Options) -> Result<(), String> {
    let opened = world::open(&world_dir(dir), o.base.as_deref())?;
    println!("{}", facts::facts(&opened).to_json());
    Ok(())
}

/// Run a world command, if `cmd` is one. `None` when it is not.
pub fn run(cmd: &str, args: &[String]) -> Option<Result<(), String>> {
    let run = |f: fn(&str, &Options) -> Result<(), String>| {
        let (first, rest) = args
            .split_first()
            .ok_or_else(|| format!("{cmd} takes a world's folder"))?;
        f(first, &options(rest)?)
    };
    Some(match cmd {
        "new" => run(new_cmd),
        "build" => run(build_cmd),
        "validate" => run(validate_cmd),
        "facts" => run(facts_cmd),
        _ => return None,
    })
}
