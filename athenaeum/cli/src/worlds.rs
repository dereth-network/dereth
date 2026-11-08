//! The world commands: a new world over the world database, its build into the server's tables,
//! its check, and its counts.
//!
//! ```text
//! athenaeum new <folder> [--key KEY]
//! athenaeum build <world> --dats <install> [--base <folder>] [--out <folder>]
//! athenaeum validate <world> [--dats <install>] [--base <folder>]
//! athenaeum facts <world> [--base <folder>]
//! athenaeum pets <world> [<item> <field> <value>] [--base <folder>]
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

/// `athenaeum pets <world>`: the kinds of pet and what each skill allows, as the world has them;
/// `athenaeum pets <world> <item> <field> <value>` changes one and saves it. An item is what the
/// list prints first (`pet 0x7000092a`), quoted.
fn pets_cmd(args: &[String]) -> Result<(), String> {
    let (dir, rest) = args.split_first().ok_or("pets takes a world's folder")?;
    // What's not an option and the option's value is what the command is told to do.
    let (mut positional, mut flags) = (Vec::new(), Vec::new());
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a.starts_with("--") {
            flags.push(a.clone());
            flags.extend(it.next().cloned());
        } else {
            positional.push(a);
        }
    }
    let o = options(&flags)?;
    let opened = world::open(&world_dir(dir), o.base.as_deref())?;
    let mut session = athenaeum_foundry::Session::open(opened, None)?;
    if let [item, field, value] = positional[..] {
        session.set(item, field, value)?;
        session.save()?;
        println!("{item}: {field} = {value}");
        return Ok(());
    }
    if !positional.is_empty() {
        return Err("pets takes <item> <field> <value>, or nothing to list them".into());
    }
    for l in session.pets() {
        match &l.row {
            yalain_world_db::rows::Row::Pet(r) => println!(
                "pet {:#010x}\tin {}\ttemplate {:#010x}\tflags {}\tcount {}\t{}",
                r.effect,
                l.pack,
                r.template,
                r.flags,
                r.count,
                if r.stays { "stays" } else { "follows" }
            ),
            yalain_world_db::rows::Row::PetLimit(r) => {
                println!("pet_limit {:#010x}\tin {}\t{:?}", r.skill, l.pack, r.limits)
            }
            _ => {}
        }
    }
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
        "pets" => pets_cmd(args),
        _ => return None,
    })
}
