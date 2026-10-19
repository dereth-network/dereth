//! Lyceum's command line: builds a world's content packs into its overlay, bakes an overlay into a
//! copy of a player's files, takes modified files apart into an overlay, and packs pictures into
//! Dereth's client layer.
//!
//! **Depends on** the Foundry (`lyceum-foundry`), whose every command this is, and `dereth-dat`
//! (the data files a world is built over). **Used by** creators and the build, from a terminal, as
//! `lyceum`.
//!
//! **Must never** write a player's data files: a build writes an overlay folder and SQL files, a
//! bake writes copies, and the originals are only read.
//!
//! ```text
//! lyceum build <world.toml> --dats <dir> [--out <dir>] [--check]
//! lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or dir>] [--world <sql>]
//!                 [--schema <json>]
//! lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>]
//!             [--dry-run]
//! lyceum template --world <sql> --class <n> --name <symbol> [--blank] [--out <file>]
//! lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
//! lyceum import --base <dir> --world <dir> --out <dir> --world-key <key>
//!               [--iterations revision|world] [--verify]
//! lyceum layer <manifest> [--out <container>]   write a client layer
//! lyceum layer <manifest> --check               say whether a client layer is current
//! ```
//!
//! Exit status 0 on success, 1 when `build --check` finds the build stale or `validate` finds a
//! fault, 2 for a usage or input error. The formats are the Foundry's ([`lyceum_foundry`]).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lyceum_foundry::{
    bake, compile, content, content_check, dats, import, layer, sql_ids, validate,
};

const USAGE: &str = "\
lyceum build <world.toml> --dats <dir> [--out <dir>] [--check]
lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or dir>] [--world <sql>] [--schema <json>]
lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>] [--dry-run]
lyceum template --world <sql> --class <n> --name <symbol> [--blank] [--out <file>]
lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
lyceum import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
lyceum layer <manifest> [--out <container>] [--check]";

fn usage(why: &str) -> ExitCode {
    eprintln!("lyceum: {why}\n{USAGE}");
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

fn fail(e: &lyceum_foundry::Error) -> ExitCode {
    eprintln!("lyceum: {e}");
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
            for o in &b.overridden {
                println!("  overrode {o}");
            }
            for w in &b.warnings {
                println!("warning: {w}");
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
                    eprintln!("the build is not what the world makes; run lyceum build {world}");
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
    if let Err(why) = args.known(&["dats", "overlay", "sql", "world", "schema"]) {
        return usage(&why);
    }
    let Some(dir) = args.path("dats") else {
        return usage("validate takes --dats");
    };
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| lyceum_foundry::Error::io(p, e));
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
    let mut report = validate::validate(&set, &validate::Scope::All);
    if let Some(sql) = args.path("sql") {
        let files = match sql_files(&sql) {
            Ok(f) => f,
            Err(e) => return fail(&e),
        };
        let mut texts = Vec::new();
        for f in files {
            match read(&f) {
                Ok(t) => texts.push((f, t)),
                Err(e) => return fail(&e),
            }
        }
        // What the content may name: the world database's classes and quests, the files' own
        // quests, the spell table, and the server's names.
        let mut known = content_check::Known {
            spells: Some(content_check::spells(&set)),
            ..content_check::Known::default()
        };
        if let Some(w) = args.path("world") {
            let text = match read(&w) {
                Ok(t) => t,
                Err(e) => return fail(&e),
            };
            known.world_classes = Some(content::classes(&text).into_keys().collect());
            known.quests.extend(
                content_check::quests(&text)
                    .iter()
                    .map(|q| content_check::quest_key(q)),
            );
            known.world_quests = true;
        }
        for (_, t) in &texts {
            known.quests.extend(
                content_check::quests(t)
                    .iter()
                    .map(|q| content_check::quest_key(q)),
            );
        }
        if let Some(p) = args.path("schema") {
            match read(&p).map(|t| content::Schema::read(&t)) {
                Ok(Ok(s)) => known.schema = Some(s),
                Ok(Err(why)) => return usage(&format!("{}: {why}", p.display())),
                Err(e) => return fail(&e),
            }
        }
        let mut used = std::collections::BTreeSet::new();
        let mut found = Vec::new();
        for (f, text) in &texts {
            let named = sql_ids::named_ids(f, text);
            report.checked += named.len();
            report.problems.extend(sql_ids::check(&set, &named));
            content_check::check(f, "content", text, &known, &mut used, &mut found);
        }
        for x in found {
            let line = format!("{}: {}", x.at, x.text);
            if x.warning {
                report.warnings.push(line);
            } else {
                report.problems.push(line);
            }
        }
    }
    for p in &report.problems {
        println!("{p}");
    }
    for w in &report.warnings {
        println!("warning: {w}");
    }
    println!(
        "{} record(s) checked, {} fault(s), {} warning(s)",
        report.checked,
        report.problems.len(),
        report.warnings.len()
    );
    if report.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// `path` itself when it is a file, else every `.sql` file under it, in path order.
fn sql_files(path: &Path) -> Result<Vec<PathBuf>, lyceum_foundry::Error> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut out = Vec::new();
    let mut stack = vec![path.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| lyceum_foundry::Error::io(&d, e))? {
            let p = e.map_err(|e| lyceum_foundry::Error::io(&d, e))?.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("sql")) {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn run_edit(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["dats", "edits", "pack", "weenies"]) {
        return usage(&why);
    }
    let ([world], Some(dats), Some(edits)) =
        (args.words.as_slice(), args.path("dats"), args.path("edits"))
    else {
        return usage("edit takes a world file, --dats and --edits");
    };
    let mut session =
        match lyceum_foundry::edit::Session::open(Path::new(world), &dats, args.value("pack")) {
            Ok(s) => s,
            Err(e) => return fail(&e),
        };
    if let Some(sql) = args.path("weenies") {
        match std::fs::read_to_string(&sql) {
            Ok(text) => session.learn_weenies(&text),
            Err(e) => return fail(&lyceum_foundry::Error::io(&sql, e)),
        }
    }
    let text = match std::fs::read_to_string(&edits) {
        Ok(t) => t,
        Err(e) => return fail(&lyceum_foundry::Error::io(&edits, e)),
    };
    let pack = session.packs()[session.target()].name.clone();
    let actions = match lyceum_foundry::script::actions(&text, &pack) {
        Ok(a) => a,
        Err(why) => return usage(&format!("{}: {why}", edits.display())),
    };
    for (i, a) in actions.iter().enumerate() {
        match session.act(a) {
            Ok(done) => {
                let blocks: Vec<String> = done
                    .touched
                    .landblocks
                    .iter()
                    .map(|lb| format!("{lb:04X}"))
                    .collect();
                println!(
                    "edit {}: landblocks {}{}",
                    i + 1,
                    if blocks.is_empty() {
                        "none".to_owned()
                    } else {
                        blocks.join(" ")
                    },
                    if done.touched.sql { ", SQL" } else { "" }
                );
            }
            Err(e) => {
                eprintln!("lyceum: edit {}: {e}", i + 1);
                return ExitCode::from(2);
            }
        }
    }
    if args.switch("dry-run") {
        return ExitCode::SUCCESS;
    }
    match session.save() {
        Ok(written) => {
            for p in written {
                println!("{}: written", p.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// `template`: a new weenie's SQL from a class of the world database's SQL (a dump or an
/// export), named `--name` (its symbol in the pack), to `--out` or the standard output.
fn run_template(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["world", "class", "name", "out"]) {
        return usage(&why);
    }
    let (Some(world), Some(class), Some(name)) =
        (args.path("world"), args.value("class"), args.value("name"))
    else {
        return usage("template takes --world, --class and --name");
    };
    let Some(class) = sql_ids::number(class) else {
        return usage(&format!("{class} is not a class id"));
    };
    let text = match std::fs::read_to_string(&world) {
        Ok(t) => t,
        Err(e) => return fail(&lyceum_foundry::Error::io(&world, e)),
    };
    let Some(sql) = lyceum_foundry::content::template(&text, class, name, args.switch("blank"))
    else {
        eprintln!("lyceum: {} has no class {class}", world.display());
        return ExitCode::FAILURE;
    };
    match args.path("out") {
        Some(out) => match std::fs::write(&out, &sql) {
            Ok(()) => {
                println!("{}: written", out.display());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&lyceum_foundry::Error::io(&out, e)),
        },
        None => {
            print!("{sql}");
            ExitCode::SUCCESS
        }
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
                    "{}: the container is not what the manifest makes; run lyceum layer {}",
                    manifest.display(),
                    manifest.display()
                );
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("lyceum: {e}");
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
            eprintln!("lyceum: {e}");
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
    let switches = ["check", "touched-only", "verify", "dry-run", "blank"];
    let parsed = |a: &[String]| parse(a, &switches);
    match first.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        "build" | "validate" | "bake" | "import" | "edit" | "template" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            match first.as_str() {
                "build" => build(&args),
                "validate" => run_validate(&args),
                "bake" => run_bake(&args),
                "edit" => run_edit(&args),
                "template" => run_template(&args),
                _ => run_import(&args),
            }
        }
        "layer" => match parsed(rest) {
            Ok(a) => client_layer(&a),
            Err(why) => usage(&why),
        },
        other => usage(&format!("{other} is not a command")),
    }
}
