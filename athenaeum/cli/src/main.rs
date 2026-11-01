//! Athenaeum's command line: worlds made over the world database, built into the server's tables,
//! checked and counted; and the Asheron's Call 2 data files read, listed, decoded and counted.
//! Its census counts every install's.
//!
//! **Depends on** the world machinery (`athenaeum-foundry`), the world database
//! (`yalain-world-db`, for its reports), the container (`auberean-dat`) and the decoders
//! (`auberean-assets`). **Used by** nothing: it is the `athenaeum` command.
//!
//! ```text
//! athenaeum new <folder> [--key KEY]               a new world over the world database, with an
//!                                                  empty pack its edits go into
//! athenaeum build <world> --dats <install> [--base <folder>] [--out <folder>]
//!                                                  the world compiled against the install into
//!                                                  the server's tables, written as text
//! athenaeum validate <world> [--dats <install>] [--base <folder>]
//!                                                  the world checked over its base (and, with
//!                                                  --dats, compiled) without writing anything
//! athenaeum facts <world> [--base <folder>]        what the world changes, counted, as JSON
//! athenaeum info <file.dat>                         header, structure and iteration list of one file
//! athenaeum ls <file.dat> [<type>]                  every record (id, type, size, date), or one type's
//! athenaeum verify <install> [<type>]               decode every record of an install; per-type rates
//! athenaeum rates <installs> <install>...           the same for several installs, one TSV row per type
//! athenaeum failures <install> <type> [<n>]         the first n records of a type that do not decode
//! athenaeum show <install> <id> [<chars>]           decode one record and print it
//! athenaeum hex <install> <id> [<from> [<len>]]     a record's bytes
//! athenaeum census <installs> <out> [<install>...]  the census of every install (TSV)
//! athenaeum types                                   the type table and the id chain
//! ```
//!
//! A world's base is the release of the world database it names, read from the per-user cache
//! (`yalain world fetch`), or the folder `--base` gives (`worlds.rs`). `<type>` is a short name (`mesh`, `rgn`, `landblock`...) or a type number (`4000000A`). Build
//! with `cargo build --profile test-release -p athenaeum-cli` in the workspace.
//!
//! **Must never** write to an install or a base database: it opens the data files read-only, and
//! a world's commands write only a new world's files and a build's output folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use auberean_assets::verify::{failures_of, type_in, verify_dat, Tally};
use auberean_assets::{Context, Record};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::types::{ID_CHAIN, TYPES};
use auberean_dat::{DatFile, RecordType};

mod census;
mod worlds;

fn parse_type(s: &str) -> Option<RecordType> {
    if let Some(t) = TYPES
        .iter()
        .find(|t| t.short.eq_ignore_ascii_case(s.trim_start_matches('.')))
    {
        return Some(t.ty);
    }
    u32::from_str_radix(s.trim_start_matches("0x"), 16)
        .ok()
        .map(RecordType)
}

fn parse_id(s: &str) -> Option<u32> {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

fn open(path: &Path) -> Result<DatFile, String> {
    DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// An install's data files, opened, with the context its portal file gives.
fn open_install(install: &Path) -> Result<(Vec<(DatName, DatFile)>, Context), String> {
    let mut v = Vec::new();
    for (name, path) in install_dats(install).map_err(|e| format!("{}: {e}", install.display()))? {
        v.push((name, open(&path)?));
    }
    let ctx = v
        .iter()
        .find(|(n, _)| *n == DatName::Portal)
        .map(|(_, d)| Context::from_portal(d).map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or_default();
    Ok((v, ctx))
}

fn info(path: &Path) -> Result<(), String> {
    let dat = open(path)?;
    let h = dat.header();
    println!("{h:#x?}");
    println!(
        "guid {}",
        auberean_dat::container::guid_string(&h.version_major)
    );
    let s = dat.verify().map_err(|e| e.to_string())?;
    println!("{s:#?}");
    println!("sound: {}", auberean_dat::container::sound(&s));
    match dat.iterations() {
        Ok(v) => println!(
            "iterations: {} values, min {:?}, max {:?}",
            v.len(),
            v.iter().min(),
            v.iter().max()
        ),
        Err(e) => println!("iterations: {e}"),
    }
    Ok(())
}

fn ls(path: &Path, only: Option<RecordType>) -> Result<(), String> {
    let dat = open(path)?;
    for e in dat.entries() {
        let ty = type_in(&dat, e.id);
        if only.is_some_and(|o| Some(o) != ty) {
            continue;
        }
        let name = ty.map_or("unknown", RecordType::short_name);
        println!("{:08X}\t{name}\t{}\t{}", e.id, e.size, census::date(e.date));
    }
    Ok(())
}

fn tally_install(
    install: &Path,
    only: Option<RecordType>,
) -> Result<BTreeMap<RecordType, Tally>, String> {
    let (dats, ctx) = open_install(install)?;
    let mut all: BTreeMap<RecordType, Tally> = BTreeMap::new();
    for (_, dat) in &dats {
        for (ty, t) in verify_dat(dat, &ctx, only) {
            all.entry(ty).or_default().merge(&t);
        }
    }
    Ok(all)
}

fn verify(install: &Path, only: Option<RecordType>) -> Result<(), String> {
    let all = tally_install(install, only)?;
    let (mut total, mut exact) = (0usize, 0usize);
    for (ty, t) in &all {
        total += t.total;
        exact += t.exact;
        let mark = if t.all_exact() { "ok" } else { "  " };
        println!(
            "{mark} {:08X} {:<16} {:>7}/{:<7}",
            ty.0,
            ty.short_name(),
            t.exact,
            t.total
        );
        for (what, (n, id)) in t.failures.iter().take(4) {
            println!("        {n:>6} like {id:08X}: {what}");
        }
    }
    println!("all types: {exact}/{total} records decode exactly");
    Ok(())
}

fn rates(root: &Path, installs: &[String]) -> Result<(), String> {
    let mut by_install = Vec::new();
    for i in installs {
        by_install.push(tally_install(&root.join(i), None)?);
    }
    print!("type\tname");
    for i in installs {
        print!("\t{i}");
    }
    println!();
    let mut types: Vec<RecordType> = by_install.iter().flat_map(|m| m.keys().copied()).collect();
    types.sort();
    types.dedup();
    for ty in types {
        print!("{:08X}\t{}", ty.0, ty.short_name());
        for m in &by_install {
            match m.get(&ty) {
                Some(t) => print!("\t{}/{}", t.exact, t.total),
                None => print!("\t-"),
            }
        }
        println!();
    }
    Ok(())
}

fn failures(install: &Path, ty: RecordType, n: usize) -> Result<(), String> {
    let (dats, ctx) = open_install(install)?;
    let mut shown = 0;
    for (name, dat) in &dats {
        for (id, e) in failures_of(dat, &ctx, ty) {
            if shown >= n {
                return Ok(());
            }
            println!(
                "{}\t{id:08X}\t{} bytes\t{e}",
                name.file_name(),
                dat.entry(id).map_or(0, |e| e.size)
            );
            shown += 1;
        }
    }
    Ok(())
}

fn find(install: &Path, id: u32) -> Result<(Vec<u8>, RecordType, Context), String> {
    let (dats, ctx) = open_install(install)?;
    for (_, dat) in &dats {
        if dat.entry(id).is_some() {
            let bytes = dat.read(id).map_err(|e| e.to_string())?;
            let ty = type_in(dat, id).ok_or_else(|| format!("{id:08X} has no type"))?;
            return Ok((bytes, ty, ctx));
        }
    }
    Err(format!("{id:08X} is in no file of {}", install.display()))
}

fn show(install: &Path, id: u32, chars: usize) -> Result<(), String> {
    let (bytes, ty, ctx) = find(install, id)?;
    println!("{id:08X} {ty}, {} bytes", bytes.len());
    match Record::decode(ty, id, &bytes, &ctx) {
        None => println!("no decoder"),
        Some(Ok(rec)) => {
            let s = format!("{rec:#?}");
            println!("{}", s.chars().take(chars).collect::<String>());
            if s.chars().count() > chars {
                println!("... ({} chars in all)", s.chars().count());
            }
        }
        Some(Err(e)) => {
            println!("fails {e}");
            hexdump(&bytes, e.at.saturating_sub(64) & !15, 192);
        }
    }
    Ok(())
}

fn hexdump(bytes: &[u8], from: usize, len: usize) {
    let end = (from + len).min(bytes.len());
    for row in (from..end).step_by(16) {
        let chunk = &bytes[row..(row + 16).min(end)];
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let text: String = chunk
            .iter()
            .map(|b| {
                if b.is_ascii_graphic() {
                    char::from(*b)
                } else {
                    '.'
                }
            })
            .collect();
        println!("{row:08x}  {:<48} {text}", hex.join(" "));
    }
}

fn types() {
    for t in TYPES {
        println!(
            "{:08X}\t{}\t{}\t{:08X}-{:08X}\t{:?}\tdecoder:{}",
            t.ty.0,
            t.short,
            t.folder,
            t.base,
            t.top,
            t.dat,
            Record::has_decoder(t.ty)
        );
    }
    println!();
    for (lo, hi, ty) in ID_CHAIN {
        println!("{lo:08X}..{hi:08X}\t{ty}");
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let arg = |i: usize| args.get(i).map(String::as_str);
    let path = |i: usize| arg(i).map(PathBuf::from).ok_or("missing argument");
    if let Some(r) = arg(1).and_then(|c| worlds::run(c, &args[2.min(args.len())..])) {
        return r;
    }
    match arg(1) {
        Some("info") => info(&path(2)?),
        Some("ls") => ls(&path(2)?, arg(3).and_then(parse_type)),
        Some("verify") => verify(&path(2)?, arg(3).and_then(parse_type)),
        Some("rates") => rates(&path(2)?, &args[3.min(args.len())..]),
        Some("failures") => {
            let ty = arg(3).and_then(parse_type).ok_or("which type?")?;
            failures(&path(2)?, ty, arg(4).and_then(|n| n.parse().ok()).unwrap_or(20))
        }
        Some("show") => {
            let id = arg(3).and_then(parse_id).ok_or("which id?")?;
            show(&path(2)?, id, arg(4).and_then(|n| n.parse().ok()).unwrap_or(4000))
        }
        Some("hex") => {
            let id = arg(3).and_then(parse_id).ok_or("which id?")?;
            let (bytes, _, _) = find(&path(2)?, id)?;
            let from = arg(4).and_then(|n| usize::from_str_radix(n.trim_start_matches("0x"), 16).ok()).unwrap_or(0);
            let len = arg(5).and_then(|n| usize::from_str_radix(n.trim_start_matches("0x"), 16).ok()).unwrap_or(bytes.len());
            hexdump(&bytes, from, len);
            Ok(())
        }
        Some("census") => census::run(&path(2)?, &path(3)?, &args[4.min(args.len())..]).map_err(|e| e.to_string()),
        Some("types") => {
            types();
            Ok(())
        }
        _ => Err("usage: athenaeum new|build|validate|facts|info|ls|verify|rates|failures|show|hex|census|types (see the source header)".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("athenaeum: {e}");
            ExitCode::FAILURE
        }
    }
}
