//! `dereth-pack`: content packs into a world's overlay, the overlay into a copy of a player's
//! files, any modified files into an overlay, and pictures into Dereth's client layer.
//!
//! ```text
//! dereth-pack build <world.toml> --dats <dir> [--out <dir>] [--check]
//! dereth-pack validate --dats <dir> [--overlay <dir>]
//! dereth-pack bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
//! dereth-pack import --base <dir> --world <dir> --out <dir> --world-key <key>
//!                    [--iterations revision|world] [--verify]
//! dereth-pack <manifest> [--out <container>]   write a client layer
//! dereth-pack <manifest> --check               say whether a client layer is current
//! ```
//!
//! Exit status 0 on success, 1 when `build --check` finds the build stale or `validate` finds a
//! fault, 2 for a usage or input error. The formats are the library's ([`dereth_pack`]).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dereth_pack::{bake, compile, dats, import, layer, validate};

const USAGE: &str = "\
dereth-pack build <world.toml> --dats <dir> [--out <dir>] [--check]
dereth-pack validate --dats <dir> [--overlay <dir>]
dereth-pack bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
dereth-pack import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
dereth-pack <manifest> [--out <container>] | <manifest> --check";

fn usage(why: &str) -> ExitCode {
    eprintln!("dereth-pack: {why}\n{USAGE}");
    ExitCode::from(2)
}

/// The flags of a subcommand: `--name value` pairs and bare switches, and the words left over.
struct Args {
    values: Vec<(String, String)>,
    switches: Vec<String>,
    words: Vec<String>,
}

fn parse(args: &[String], switches: &[&str]) -> Result<Args, String> {
    let mut out = Args {
        values: Vec::new(),
        switches: Vec::new(),
        words: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(name) = a.strip_prefix("--") {
            if switches.contains(&name) {
                out.switches.push(name.to_owned());
            } else {
                let v = it.next().ok_or_else(|| format!("--{name} takes a value"))?;
                out.values.push((name.to_owned(), v.clone()));
            }
        } else {
            out.words.push(a.clone());
        }
    }
    Ok(out)
}

impl Args {
    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    fn path(&self, name: &str) -> Option<PathBuf> {
        self.value(name).map(PathBuf::from)
    }
    fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }
    fn known(&self, names: &[&str]) -> Result<(), String> {
        match self
            .values
            .iter()
            .find(|(n, _)| !names.contains(&n.as_str()))
        {
            Some((n, _)) => Err(format!("--{n} is not a flag here")),
            None => Ok(()),
        }
    }
}

fn fail(e: &dereth_pack::Error) -> ExitCode {
    eprintln!("dereth-pack: {e}");
    ExitCode::from(2)
}

fn build(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["dats", "out"]) {
        return usage(&why);
    }
    let ([world], Some(dats)) = (args.words.as_slice(), args.path("dats")) else {
        return usage("build takes a world file and --dats");
    };
    let check = args.switch("check");
    match compile::build(Path::new(world), &dats, args.path("out").as_deref(), check) {
        Ok(b) => {
            for (kind, symbol, id) in &b.numbered {
                if kind.hex() {
                    println!("  numbered {kind} {symbol} = {id:#010X}");
                } else {
                    println!("  numbered {kind} {symbol} = {id}");
                }
            }
            for line in &b.summary {
                println!("  {line}");
            }
            for (path, records, deleted) in &b.containers {
                println!(
                    "{}: {records} record(s), {deleted} deletion(s)",
                    path.display()
                );
            }
            println!(
                "{}: {} SQL file(s)",
                b.out.join("sql").display(),
                b.sql_files
            );
            if check {
                if b.current {
                    println!("the build is current");
                    ExitCode::SUCCESS
                } else {
                    eprintln!(
                        "the build is not what the world makes; run dereth-pack build {world}"
                    );
                    ExitCode::from(1)
                }
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => fail(&e),
    }
}

fn run_validate(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["dats", "overlay"]) {
        return usage(&why);
    }
    let Some(dir) = args.path("dats") else {
        return usage("validate takes --dats");
    };
    let set = match dats::DatSet::open(&dir) {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let set = match args.path("overlay") {
        Some(o) => match set.with_overlay(&o, None) {
            Ok(s) => s,
            Err(e) => return fail(&e),
        },
        None => set,
    };
    let report = validate::validate(&set, &validate::Scope::All);
    for p in &report.problems {
        println!("{p}");
    }
    println!(
        "{} record(s) checked, {} fault(s)",
        report.checked,
        report.problems.len()
    );
    if report.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_bake(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["overlay", "dats", "to"]) {
        return usage(&why);
    }
    let (Some(overlay), Some(dats), Some(to)) =
        (args.path("overlay"), args.path("dats"), args.path("to"))
    else {
        return usage("bake takes --overlay, --dats and --to");
    };
    match bake::bake(&overlay, &dats, &to, args.switch("touched-only")) {
        Ok(b) => {
            println!("world {}", b.world_key);
            for (path, records, deleted) in &b.written {
                println!(
                    "{}: {records} record(s) written, {deleted} deletion(s), checked",
                    path.display()
                );
            }
            for path in &b.copied {
                println!("{}: copied", path.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn run_import(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["base", "world", "out", "world-key", "iterations"]) {
        return usage(&why);
    }
    let (Some(base), Some(world), Some(out), Some(key)) = (
        args.path("base"),
        args.path("world"),
        args.path("out"),
        args.value("world-key"),
    ) else {
        return usage("import takes --base, --world, --out and --world-key");
    };
    let iterations = match args.value("iterations") {
        None | Some("revision") => dereth_dat::decompose::Iterations::Revision,
        Some("world") => dereth_dat::decompose::Iterations::World,
        Some(other) => return usage(&format!("--iterations is revision or world, not {other}")),
    };
    match import::import(&base, &world, &out, key, iterations, args.switch("verify")) {
        Ok(files) => {
            let mut wrote = false;
            for f in &files {
                let d = &f.diff;
                println!(
                    "{}: {} added, {} changed, {} removed",
                    f.file.file_name(),
                    d.added.len(),
                    d.changed.len(),
                    d.removed.len()
                );
                if let Some((path, revision)) = &f.written {
                    wrote = true;
                    println!("  -> {} (iteration {revision})", path.display());
                }
                if let Some(n) = f.verified {
                    println!("  verified: {n} record(s) read as the world's");
                }
            }
            if !wrote {
                println!("the world is its base: no overlay was written");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn client_layer(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["out"]) {
        return usage(&why);
    }
    let [manifest] = args.words.as_slice() else {
        return usage("a client layer takes one manifest");
    };
    let manifest = Path::new(manifest);
    if args.switch("check") {
        return match layer::is_current(manifest) {
            Ok(true) => {
                println!("{}: the container is current", manifest.display());
                ExitCode::SUCCESS
            }
            Ok(false) => {
                eprintln!(
                    "{}: the container is not what the manifest makes; run dereth-pack {}",
                    manifest.display(),
                    manifest.display()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("dereth-pack: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match layer::pack(manifest, args.path("out").as_deref()) {
        Ok(packed) => {
            println!("{}: {} records", packed.out.display(), packed.ids.len());
            for id in &packed.flattened {
                println!(
                    "  {:#010X}: transparent pixels laid on the background",
                    id.raw()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-pack: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let Some(first) = all.first() else {
        return usage("nothing to do");
    };
    let rest = &all[1..];
    let switches = ["check", "touched-only", "verify"];
    let parsed = |a: &[String]| parse(a, &switches);
    match first.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        "build" | "validate" | "bake" | "import" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            match first.as_str() {
                "build" => build(&args),
                "validate" => run_validate(&args),
                "bake" => run_bake(&args),
                _ => run_import(&args),
            }
        }
        _ => match parsed(&all) {
            Ok(a) => client_layer(&a),
            Err(why) => usage(&why),
        },
    }
}
