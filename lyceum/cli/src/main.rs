//! Lyceum's command line: builds a world's content packs into its overlay, bakes an overlay into a
//! copy of a player's files, exports a world for a server that runs it elsewhere, takes modified
//! files apart into an overlay, and packs pictures into Dereth's client layer.
//!
//! **Depends on** the Foundry (`lyceum-foundry`), whose every command this is, and `dereth-dat`
//! (the data files a world is built over). **Used by** creators and the build, from a terminal, as
//! `lyceum`.
//!
//! **Must never** write a player's data files: a build writes an overlay folder and SQL files, a
//! bake writes copies, and the originals are only read.
//!
//! ```text
//! lyceum new <folder> [--empty] [--era eor|infiltration]   a new world, from the sample or
//!                                               empty, built on that era's files
//! lyceum build <world.toml> --dats <dir> [--ac2 <dir>] [--out <dir>] [--after <overlay>] [--check]
//! lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or dir>] [--world <sql>]
//!                 [--schema <json>]
//! lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>]
//!             [--dry-run]
//! lyceum edit <world.toml> --dats <dir> --weenies <sql> add-building <model> <landblock> <x> <y>
//!             <heading> [--layout <n>] [--furnished <XXYY/n> | --bare] [--plain] [--pack <name>]
//!             [--dry-run]           a house drops as one players can buy, unless --plain
//! lyceum template --world <sql> --class <n> --name <symbol> [--blank] [--out <file>]
//! lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
//! lyceum export <world.toml> --dats <dir> --for empyrean|ace|gdle --out <dir>
//!               [--era launch|infiltration|eor] [--world-sql <dump>] [--empyrean <dir>]
//!               [--gdle-quests <quests.json>]   GDLE's quests.json, the world's merged into it
//! lyceum import --base <dir> --world <dir> --out <dir> --world-key <key>
//!               [--iterations revision|world] [--verify]
//! lyceum facts <world.toml>                     what the world changes, counted, as one line
//!                                               of JSON
//! lyceum rows <world.toml> --schema <json> [--layer <id>] [--item <item>] [--as-sql]
//!                                               a world repository's rows folded at a layer
//!                                               (the last it can show), counted by kind, or
//!                                               one item's fields with the layer and the place
//!                                               that set each and where it came from; the
//!                                               vocabulary is `empyrean-import schema`'s
//! lyceum lock <world.toml> --dats <dir> [--on <overlay>]
//!                                               the world's lockfile regenerated after a
//!                                               merge: every id kept, what two branches
//!                                               numbered alike numbered again, its rows too
//! lyceum publish <world.toml> --dats <dir> --out <dir> [--on <overlay>] [--layer <id>]
//!               [--number] [--empyrean <dir>]   a world repository built for its server: the
//!                                               land checked, the overlay numbered from the
//!                                               ledger, the world pack, what the server reads;
//!                                               --number, the author's, numbers new content
//! lyceum pull-dungeon <from-world.toml> <landblock> --into <world.toml> --dats <dir> --schema <json>
//!               [--database <dump>] [--on <overlay>] [--pack <name>] [--alongside] [--dry-run]
//!               [--no-library]
//!               [--entrance <cell>,<x>,<y>,<z>[,<heading>]] [--portal <class>]
//!                                               a dungeon of another world pulled into a world
//!                                               repository: what that world places in it as
//!                                               the repository's rows, a portal into it, and
//!                                               the exits and classes left to its author
//! lyceum layer <manifest> [--out <container>]   write a client layer
//! lyceum layer <manifest> --check               say whether a client layer is current
//! lyceum fit --dats <dir> --model <id> [--overlay <dir>] [--against <id>]
//!             [--grid <n>] [--directions <n>]
//! lyceum --version
//! ```
//!
//! Exit status 0 on success, 1 when `build --check` finds the build stale or `validate` finds a
//! fault, 2 for a usage or input error. The formats are the Foundry's ([`lyceum_foundry`]).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lyceum_foundry::{
    bake, compile, content, content_check, dats, export, facts, import, layer, revise, sample,
    sql_ids, validate,
};

const USAGE: &str = "\
lyceum new <folder> [--empty] [--era eor|infiltration] [--spacing 24|12|8 | --on <overlay>]
lyceum upgrade <world.toml> [--check]
lyceum build <world.toml> --dats <dir> [--ac2 <dir>] [--out <dir>] [--after <overlay>] [--check]
lyceum validate --dats <dir> [--overlay <dir>] [--sql <file or dir>] [--world <sql>] [--schema <json>]
lyceum edit <world.toml> --dats <dir> --edits <file> [--pack <name>] [--weenies <sql>] [--dry-run]
lyceum edit <world.toml> --dats <dir> --weenies <sql> add-building <model> <landblock> <x> <y> <heading> [--layout <n>] [--furnished <XXYY/n> | --bare] [--plain] [--pack <name>] [--dry-run]
lyceum template --world <sql> --class <n> --name <symbol> [--blank] [--out <file>]
lyceum bake --overlay <dir> --dats <dir> --to <dir> [--touched-only]
lyceum export <world.toml> --dats <dir> --for empyrean|ace|gdle --out <dir> [--era launch|infiltration|eor] [--world-sql <dump>] [--empyrean <dir>] [--gdle-quests <quests.json>]
lyceum import --base <dir> --world <dir> --out <dir> --world-key <key> [--iterations revision|world] [--verify]
lyceum facts <world.toml>
lyceum rows <world.toml> --schema <json> [--layer <id>] [--item <item>] [--as-sql]
lyceum lock <world.toml> --dats <dir> [--on <overlay>]
lyceum publish <world.toml> --dats <dir> --out <dir> [--on <overlay>] [--layer <id>] [--number] [--empyrean <dir>]
lyceum pull-dungeon <from-world.toml> <landblock> --into <world.toml> --dats <dir> --schema <json> [--database <dump>] [--on <overlay>] [--pack <name>] [--alongside] [--dry-run] [--no-library] [--entrance <cell>,<x>,<y>,<z>[,<heading>]] [--portal <class>]
lyceum layer <manifest> [--out <container>] [--check]
lyceum fit --dats <dir> --model <id> [--overlay <dir>] [--against <id>] [--grid <n>] [--directions <n>]";

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

/// `lock`: a world's lockfile regenerated after a merge, its conflict markers and all: every id
/// kept, and what two branches numbered alike numbered again, with the rows that named it.
fn lock(args: &Args) -> ExitCode {
    use lyceum_foundry::repo_world;
    if args.known(&["dats", "on"]).is_err() || args.words.len() != 1 {
        return usage("lock takes a world file and --dats");
    }
    let Some(dats) = args.path("dats") else {
        return usage("lock takes --dats (the client's files the world is built over)");
    };
    let path = Path::new(&args.words[0]);
    let run = || -> Result<Vec<lyceum_foundry::lock_merge::Renumbered>, String> {
        let world = if repo_world::is_repository(path) {
            let repo = lyceum_foundry::repo::Repo::open(path)?;
            let out = std::env::temp_dir().join(format!("lyceum-lock-{}", std::process::id()));
            let land = args.path("on");
            let w = repo_world::world_of(&repo, repo.shown_layers() - 1, land.as_deref(), &out)?;
            repo_world::check_inputs(&repo, &dats)?;
            w
        } else {
            lyceum_foundry::world::World::read(path).map_err(|e| e.to_string())?
        };
        lyceum_foundry::lock_merge::rebuild(world, &dats).map_err(|e| e.to_string())
    };
    match run() {
        Ok(done) => {
            for r in &done {
                println!(
                    "{} {}: 0x{:08X} is now 0x{:08X}",
                    r.kind, r.symbol, r.from, r.to
                );
                for f in &r.rows {
                    println!("  {}", f.display());
                }
            }
            println!(
                "the lockfile is whole; {} symbol(s) numbered again",
                done.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lyceum: {e}");
            ExitCode::from(2)
        }
    }
}

/// `rows`: a world repository folded at a layer.
fn rows(args: &Args) -> ExitCode {
    use lyceum_foundry::repo::{Repo, RowSchema};
    if args.known(&["schema", "layer", "item", "as-sql"]).is_err() || args.words.len() != 1 {
        return usage("rows takes a world file and --schema");
    }
    let Some(schema) = args.path("schema") else {
        return usage("rows takes --schema (empyrean-import schema writes it)");
    };
    let run = || -> Result<(), String> {
        let text =
            std::fs::read_to_string(&schema).map_err(|e| format!("{}: {e}", schema.display()))?;
        let vocabulary = RowSchema::read(&text)
            .map_err(|e| format!("{}: {e}", schema.display()))?
            .ok_or_else(|| {
                format!(
                    "{}: no rows; a newer empyrean-import writes them",
                    schema.display()
                )
            })?;
        let names = lyceum_foundry::content::Schema::read(&text)
            .map_err(|e| format!("{}: {e}", schema.display()))?;
        let repo = Repo::open(Path::new(&args.words[0]))?;
        let sql = args.switch("as-sql");
        let view =
            |kind: &str| lyceum_foundry::row_sql::RowSql::new(&names, kind, repo.stamp.as_deref());
        let at = match args.value("layer") {
            Some(id) => repo
                .ids()
                .iter()
                .position(|l| *l == id)
                .ok_or_else(|| format!("no layer {id}; the layers are {:?}", repo.ids()))?,
            None => repo.shown_layers() - 1,
        };
        let world = repo.fold(&vocabulary, at)?;
        match args.value("item") {
            Some(text) => {
                let name = lyceum_foundry::repo::item_name(&vocabulary, text)?;
                let item = world
                    .items
                    .get(&name)
                    .ok_or_else(|| format!("{name}: not in the world at {}", world.layer))?;
                if sql {
                    let doc = view(&item.kind)?.to_sql(&item.row(&vocabulary))?;
                    print!("{}", doc.render());
                    return Ok(());
                }
                println!("{name} ({}), at {}:", item.kind, world.layer);
                for f in &item.fields {
                    let at = f.at.as_ref().map_or_else(String::new, |a| format!(", {a}"));
                    println!(
                        "  {} = {}    # {}{at}; {}",
                        f.name, f.value, f.layer, f.prov
                    );
                }
            }
            None => {
                let mut by_kind: std::collections::BTreeMap<&str, usize> =
                    std::collections::BTreeMap::new();
                for i in world.items.values() {
                    *by_kind.entry(i.kind.as_str()).or_default() += 1;
                }
                let kinds: Vec<String> = by_kind.iter().map(|(k, n)| format!("{n} {k}")).collect();
                println!(
                    "{}: {} items at {} ({}); {} finding(s)",
                    repo.title.as_deref().unwrap_or("the world"),
                    world.items.len(),
                    world.layer,
                    kinds.join(", "),
                    world.findings.len()
                );
                for f in &world.findings {
                    println!("  {f}");
                }
                if sql {
                    // Each item of a kind written as SQL, viewed and read back: changed when its
                    // SQL is (what it means), and written otherwise when only its row is (a
                    // default written out, which a row may leave out).
                    let (mut seen, mut otherwise, mut changed) = (0, 0, Vec::new());
                    for (name, item) in &world.items {
                        let Ok(v) = view(&item.kind) else { continue };
                        let row = item.row(&vocabulary);
                        seen += 1;
                        let back = v.to_sql(&row).and_then(|d| Ok((d.render(), v.to_row(&d)?)));
                        match back.and_then(|(sql, b)| Ok((sql, v.to_sql(&b)?.render(), b))) {
                            Ok((a, b, _)) if a != b => {
                                changed.push(format!("{name}: read back changed"));
                            }
                            Ok((_, _, b)) if b != row => otherwise += 1,
                            Ok(_) => {}
                            Err(e) => changed.push(format!("{name}: {e}")),
                        }
                    }
                    println!(
                        "{seen} item(s) viewed as SQL, {} read back changed, {otherwise} written otherwise",
                        changed.len()
                    );
                    for c in &changed {
                        println!("  {c}");
                    }
                }
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lyceum: {e}");
            ExitCode::from(2)
        }
    }
}

fn build(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["dats", "out", "after", "ac2"]) {
        return usage(&why);
    }
    let ([world], Some(dats)) = (args.words.as_slice(), args.path("dats")) else {
        return usage("build takes a world file and --dats");
    };
    let ac2 = match args.path("ac2").map(|d| lyceum_foundry::ac2_kit::open(&d)) {
        None => None,
        Some(Ok(a)) => Some(a),
        Some(Err(why)) => {
            eprintln!("lyceum: --ac2: {why}");
            return ExitCode::from(2);
        }
    };
    let check = args.switch("check");
    let after = args.path("after");
    if check && after.is_some() {
        return usage("--after writes a revision, which --check never does");
    }
    match compile::build_with(
        Path::new(world),
        &dats,
        args.path("out").as_deref(),
        check,
        ac2.as_ref(),
    ) {
        Ok(b) => {
            if let Some(last) = &after {
                if let Err(e) = revise_after(Path::new(world), &dats, last, &b.out) {
                    return fail(&e);
                }
            }
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

/// Number the overlay just built into `out` as the revision after `last`, the overlay the world was
/// last published with, and say what each file's revision holds.
fn revise_after(
    world: &Path,
    dats: &Path,
    last: &Path,
    out: &Path,
) -> Result<(), lyceum_foundry::Error> {
    let world = lyceum_foundry::world::World::read(world)?;
    let followed = revise::after(dats, world.era, last, &out.join("overlay"), &world.key)?;
    if let Some(why) = &followed.restarted {
        println!("revisions start again: {why}");
    }
    for (file, r) in &followed.files {
        match r.revision {
            Some(n) => println!(
                "{}: revision {n}, {} changed, {} deleted, {} kept",
                file.file_name(),
                r.changed,
                r.deleted,
                r.kept
            ),
            None if !r.revisions.is_empty() => println!(
                "{}: unchanged, revision {}",
                file.file_name(),
                r.revisions.last().copied().unwrap_or_default()
            ),
            None => {}
        }
    }
    Ok(())
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
            known.loot_tables = Some(sql_ids::loot_ids(&text).into_iter().collect());
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
            if let Some(tables) = known.loot_tables.as_mut() {
                tables.extend(sql_ids::loot_ids(t));
            }
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
    if let Err(why) = args.known(&["dats", "edits", "pack", "weenies", "layout", "furnished"]) {
        return usage(&why);
    }
    let (Some((world, verb)), Some(dats)) = (args.words.split_first(), args.path("dats")) else {
        return usage("edit takes a world file and --dats");
    };
    let verb = match verb.split_first() {
        None => None,
        Some((v, words)) if v == "add-building" => match lyceum_foundry::script::add_building(
            words,
            args.value("layout"),
            args.value("furnished"),
            args.switch("bare"),
            args.switch("plain"),
        ) {
            Ok(a) => Some(a),
            Err(why) => return usage(&why),
        },
        Some((v, _)) => return usage(&format!("{v} is not an edit")),
    };
    let edits = args.path("edits");
    if verb.is_some() == edits.is_some() {
        return usage("edit takes --edits, or one edit (add-building ...)");
    }
    let mut session =
        match lyceum_foundry::edit::Session::open(Path::new(world), &dats, args.value("pack")) {
            Ok(s) => s,
            Err(e) => return fail(&e),
        };
    let mut server = None;
    if let Some(sql) = args.path("weenies") {
        match std::fs::read_to_string(&sql) {
            Ok(text) => server = Some(sql_ids::server_content(&text)),
            Err(e) => return fail(&lyceum_foundry::Error::io(&sql, e)),
        }
    }
    let pack = session.packs()[session.target()].name.clone();
    let actions = match (&edits, verb) {
        (Some(edits), _) => {
            let text = match std::fs::read_to_string(edits) {
                Ok(t) => t,
                Err(e) => return fail(&lyceum_foundry::Error::io(edits, e)),
            };
            match lyceum_foundry::script::actions(&text, &pack) {
                Ok(a) => a,
                Err(why) => return usage(&format!("{}: {why}", edits.display())),
            }
        }
        (None, verb) => verb.into_iter().collect(),
    };
    // A drop copies from the world's buildings, with the doors the server's world stands at them.
    if actions
        .iter()
        .any(|a| matches!(a, lyceum_foundry::script::Action::AddBuilding { .. }))
    {
        let catalog =
            lyceum_foundry::buildings::Catalog::read(session.base(), server.as_ref(), &|_| None);
        session.learn_buildings(catalog);
    }
    if let Some(content) = server {
        session.learn_server(content);
    }
    for (i, a) in actions.iter().enumerate() {
        match session.act(a) {
            Ok(done) => {
                for w in &done.warnings {
                    eprintln!("lyceum: edit {}: warning: {w}", i + 1);
                }
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

/// The server's import tool: in the folder `--empyrean` names, else beside this command.
fn import_program(dir: Option<&Path>) -> Option<PathBuf> {
    let file = format!("empyrean-import{}", std::env::consts::EXE_SUFFIX);
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_exe().ok()?.parent()?.to_path_buf(),
    };
    Some(dir.join(file)).filter(|p| p.is_file())
}

/// `publish`: a world repository built for its operator's server.
/// `pull-dungeon`: a dungeon of another world pulled into a world repository.
fn run_pull(args: &Args) -> ExitCode {
    use lyceum_foundry::pull::{pull, Request};
    if args
        .known(&[
            "into", "dats", "schema", "database", "on", "pack", "entrance", "portal",
        ])
        .is_err()
        || args.words.len() != 2
        || args
            .switches
            .iter()
            .any(|s| !["alongside", "dry-run", "no-library"].contains(&s.as_str()))
    {
        return usage(
            "pull-dungeon takes the source world file and a landblock, --into, --dats and --schema",
        );
    }
    let (Some(into), Some(dats), Some(schema)) =
        (args.path("into"), args.path("dats"), args.path("schema"))
    else {
        return usage("pull-dungeon takes --into (the world repository), --dats and --schema");
    };
    let Ok(landblock) = u16::from_str_radix(args.words[1].trim_start_matches("0x"), 16) else {
        return usage(&format!(
            "{} is not a landblock (XXYY, in hex)",
            args.words[1]
        ));
    };
    let source = Path::new(&args.words[0]);
    let database = match args.path("database") {
        Some(d) => d,
        None => {
            let era = match lyceum_foundry::world::World::read(source) {
                Ok(w) => export::Era::of(w.era),
                Err(e) => return fail(&e),
            };
            match export::database_cache().and_then(|c| export::newest_database_for(&c, era)) {
                Some(d) => d,
                None => {
                    return usage(
                        "no server database in the cache: name the source's with --database",
                    )
                }
            }
        }
    };
    let entrance = match args.value("entrance").map(parse_entrance) {
        None => None,
        Some(Some(e)) => Some(e),
        Some(None) => {
            return usage("--entrance is <cell>,<x>,<y>,<z>[,<heading>]: a cell in hex, landblock meters and degrees")
        }
    };
    let portal = match args.value("portal").map(sql_ids::number) {
        None => None,
        Some(Some(c)) => Some(c),
        Some(None) => return usage("--portal is a class id"),
    };
    let text = match std::fs::read_to_string(&schema) {
        Ok(t) => t,
        Err(e) => return usage(&format!("{}: {e}", schema.display())),
    };
    let vocabulary = match lyceum_foundry::repo::RowSchema::read(&text) {
        Ok(Some(v)) => v,
        Ok(None) => {
            return usage(&format!(
                "{}: no rows; a newer empyrean-import writes them",
                schema.display()
            ))
        }
        Err(e) => return usage(&format!("{}: {e}", schema.display())),
    };
    let land = args.path("on");
    let r = Request {
        source,
        database: &database,
        landblock,
        target: &into,
        schema: &vocabulary,
        land: land.as_deref(),
        pack: args.value("pack"),
        entrance,
        portal,
        alongside: args.switch("alongside"),
        dry_run: args.switch("dry-run"),
        library: !args.switch("no-library"),
    };
    match pull(&r, &dats) {
        Ok(done) => {
            let p = &done.plan;
            println!(
                "{landblock:04X}: {} thing(s) placed from {}{}",
                p.rows.len(),
                source.display(),
                if done.already > 0 {
                    format!(", beside the {} the repository placed there", done.already)
                } else {
                    String::new()
                }
            );
            if let Some((guid, class)) = done.entrance {
                println!("the way in: {guid:#010X}, class {class}");
            } else if let Some((class, n, name)) = p.portals.first() {
                println!(
                    "no way in placed; the source's commonest is class {class} ({}, {n} placed): give --entrance",
                    name.as_deref().unwrap_or("unnamed")
                );
            }
            for e in &p.exits {
                println!(
                    "{}: {:#010X} (class {}, {}) leads to {:#010X}{}",
                    if e.outdoors() {
                        "exit onto the land, to re-aim".to_owned()
                    } else {
                        format!(
                            "exit into dungeon {:04X}, to pull too or re-aim",
                            e.to >> 16
                        )
                    },
                    e.guid,
                    e.class,
                    e.name.as_deref().unwrap_or("unnamed"),
                    e.to,
                    e.at.map_or_else(String::new, |a| format!(
                        " at {:.1}, {:.1}, {:.1}",
                        a[0], a[1], a[2]
                    ))
                );
            }
            for (class, name) in &done.missing {
                println!(
                    "class the repository lacks: weenie {class}  # {}",
                    name.as_deref().unwrap_or("unnamed")
                );
            }
            if let Some((list, added)) = &done.library {
                let verb = if r.dry_run { "would add" } else { "added" };
                println!(
                    "{verb} {} class(es) to {}: {}",
                    added.len(),
                    list.display(),
                    added
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            } else if !done.missing.is_empty() && r.library {
                println!(
                    "the repository has no library list (library.txt): add the classes by hand"
                );
            }
            for s in &p.skipped {
                println!("left out (no number): {s}");
            }
            for f in &done.files {
                let verb = if r.dry_run { "would write" } else { "wrote" };
                println!("{verb} {}", f.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// `--entrance`'s value: a cell in hex, landblock meters, and a heading in degrees (0 when left
/// out).
fn parse_entrance(v: &str) -> Option<lyceum_foundry::pull::Entrance> {
    let f: Vec<&str> = v.split(',').map(str::trim).collect();
    if !(4..=5).contains(&f.len()) {
        return None;
    }
    let cell = u32::from_str_radix(f[0].trim_start_matches("0x"), 16).ok()?;
    let n = |s: &str| s.parse::<f32>().ok();
    Some(lyceum_foundry::pull::Entrance {
        cell,
        at: [n(f[1])?, n(f[2])?, n(f[3])?],
        heading: f.get(4).map_or(Some(0.0), |h| n(h))?,
    })
}

fn run_publish(args: &Args) -> ExitCode {
    use lyceum_foundry::publish::{publish, Publish};
    if args
        .known(&["dats", "out", "on", "layer", "number", "empyrean"])
        .is_err()
        || args.words.len() != 1
    {
        return usage("publish takes a world file, --dats and --out");
    }
    let (Some(dats), Some(out)) = (args.path("dats"), args.path("out")) else {
        return usage("publish takes --dats (the client's files) and --out (the folder to serve)");
    };
    let named = args.path("empyrean");
    let Some(program) = import_program(named.as_deref()) else {
        return usage("no empyrean-import: name the server's folder with --empyrean");
    };
    let importer = lyceum_foundry::export::ImportProgram(program);
    let land = args.path("on");
    let p = Publish {
        layer: args.value("layer"),
        land: land.as_deref(),
        number: args.switch("number"),
        importer: &importer,
    };
    match publish(Path::new(&args.words[0]), &dats, &out, &p) {
        Ok(done) => {
            println!(
                "{}: {} at {}, its overlay generation {}{}",
                done.out.display(),
                args.words[0],
                done.layer,
                done.generation,
                if done.numbered {
                    " (new: commit the ledger's line)"
                } else {
                    ""
                }
            );
            for (f, h) in &done.files {
                println!("  {h}  {}", f.display());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn run_export(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&[
        "dats",
        "for",
        "out",
        "era",
        "world-sql",
        "empyrean",
        "gdle-quests",
    ]) {
        return usage(&why);
    }
    let ([world], Some(dats), Some(target), Some(out)) = (
        args.words.as_slice(),
        args.path("dats"),
        args.value("for"),
        args.path("out"),
    ) else {
        return usage("export takes a world file, --dats, --for and --out");
    };
    let Some(target) = export::Target::parse(target) else {
        return usage(&format!("--for is empyrean, ace or gdle, not {target}"));
    };
    let gdle_quests = args.path("gdle-quests");
    if gdle_quests.is_some() && target != export::Target::Gdle {
        return usage("--gdle-quests is for --for gdle");
    }
    // The era of the world's files, unless the command line names one.
    let era = match args.value("era").map(export::Era::parse) {
        None => match lyceum_foundry::world::World::read(Path::new(world)) {
            Ok(w) => export::Era::of(w.era),
            Err(e) => return fail(&e),
        },
        Some(Some(era)) => era,
        Some(None) => return usage("--era is launch, infiltration or eor"),
    };
    let named = args.path("empyrean");
    let program = import_program(named.as_deref());
    if named.is_some() && program.is_none() {
        return usage("--empyrean names a folder without empyrean-import");
    }
    let importer = program.map(export::ImportProgram);
    let world_sql = args.path("world-sql");
    let opts = export::Options {
        target,
        out: &out,
        era,
        world_sql: world_sql.as_deref(),
        importer: importer.as_ref().map(|i| i as &dyn export::Importer),
        gdle_quests: gdle_quests.as_deref(),
    };
    match export::export(Path::new(world), &dats, &opts) {
        Ok(done) => {
            for w in &done.warnings {
                println!("warning: {w}");
            }
            for line in &done.wrote {
                println!("{line}");
            }
            println!("exported for {} to {}", target.name(), done.out.display());
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

/// What `lyceum --version` prints: the version, and the commit and target a release build was
/// made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "{}\ncommit: {}\ntarget: {}\n",
        concat!("lyceum ", env!("CARGO_PKG_VERSION")),
        option_env!("LYCEUM_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("LYCEUM_BUILD_TARGET").unwrap_or("unknown"),
    )
}

fn run_fit(args: &Args) -> ExitCode {
    if let Err(why) = args.known(&["dats", "overlay", "model", "against", "grid", "directions"]) {
        return usage(&why);
    }
    let Some(dats) = args.path("dats") else {
        return usage("fit takes --dats and --model");
    };
    let id = |s: &str| {
        u32::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16)
            .ok()
            .filter(|n| matches!(n >> 24, 1 | 2))
    };
    let Some(candidate) = args.value("model").and_then(id) else {
        return usage("fit --model is a hexadecimal GfxObj or Setup id");
    };
    let original = match args.value("against") {
        Some(s) => match id(s) {
            Some(n) => Some(n),
            None => return usage("fit --against is a hexadecimal GfxObj or Setup id"),
        },
        None => None,
    };
    let grid = args
        .value("grid")
        .map_or(Some(32), |s| s.parse::<usize>().ok());
    let directions = args
        .value("directions")
        .map_or(Some(32), |s| s.parse::<usize>().ok());
    let (Some(grid), Some(directions)) = (
        grid.filter(|n| (4..=64).contains(n)),
        directions.filter(|n| (4..=128).contains(n)),
    ) else {
        return usage("fit needs --grid 4 to 64 and --directions 4 to 128");
    };
    if !args.words.is_empty() || !args.switches.is_empty() {
        return usage("fit takes named values only");
    }
    let report = match lyceum_foundry::fit::check_files(
        &dats,
        args.path("overlay").as_deref(),
        candidate,
        original,
        grid,
        directions,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("lyceum: {e}");
            return ExitCode::from(2);
        }
    };
    let surface = report.surface;
    println!(
        "surface samples within 5 cm: {} / {}",
        surface.within_five_cm, surface.samples
    );
    match surface.largest_gap {
        Some(gap) => println!("largest sampled surface gap: {gap:.4} m"),
        None => println!("largest sampled surface gap: unavailable (no collision surface)"),
    }
    match surface.volume {
        Some(v) => println!(
            "estimated excess collision volume: {:.4} m3 ({} / {} grid cells, {:.6} m3 each)",
            v.excess_volume(),
            v.excess_cells,
            v.cells,
            v.cell_volume
        ),
        None => println!("excess collision volume: unknown (open or non-manifold mesh)"),
    }
    if !report.walks.is_empty() {
        println!(
            "approaches with different stop outcomes: {} / {}",
            report
                .walks
                .iter()
                .filter(|w| w.candidate_stopped != w.original_stopped)
                .count(),
            report.walks.len()
        );
        let largest = report
            .walks
            .iter()
            .map(|w| w.separation)
            .fold(0.0, f32::max);
        println!("largest endpoint separation: {largest:.4} m");
        for (i, w) in report.walks.iter().enumerate() {
            println!("approach {i}: new stopped={}, base stopped={}, separation={:.4} m; new end=({:.3}, {:.3}, {:.3}); base end=({:.3}, {:.3}, {:.3})",w.candidate_stopped,w.original_stopped,w.separation,w.candidate.end.x,w.candidate.end.y,w.candidate.end.z,w.original.end.x,w.original.end.y,w.original.end.z);
        }
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let Some(first) = all.first() else {
        return usage("nothing to do");
    };
    let rest = &all[1..];
    let switches = [
        "check",
        "touched-only",
        "verify",
        "dry-run",
        "blank",
        "empty",
        "bare",
        "plain",
        "as-sql",
        "number",
        "alongside",
        "no-library",
    ];
    let parsed = |a: &[String]| parse(a, &switches);
    match first.as_str() {
        "new" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            if args.known(&["era", "spacing", "on"]).is_err()
                || args.words.len() != 1
                || args.switches.iter().any(|s| s != "empty")
            {
                return usage("new takes a folder, optional --empty, --era and --spacing or --on");
            }
            let era = match args.value("era").map(lyceum_foundry::world::EraId::parse) {
                None => lyceum_foundry::world::EraId::Eor,
                Some(Some(era)) => era,
                Some(None) => return usage("--era is eor or infiltration"),
            };
            let spacing = match args.value("spacing").map(str::parse::<u32>) {
                None => 24,
                Some(Ok(m)) if lyceum_foundry::world::land_steps(m).is_some() => m,
                Some(_) => return usage("--spacing is 24, 12 or 8 (meters between vertices)"),
            };
            let made = match args.value("on") {
                Some(_) if args.value("spacing").is_some() => {
                    return usage("a world --on an imported world takes its land from it")
                }
                Some(on) => sample::make_on(Path::new(&args.words[0]), era, Path::new(on)),
                None => sample::make(
                    Path::new(&args.words[0]),
                    args.switch("empty"),
                    era,
                    spacing,
                ),
            };
            match made {
                Ok(world) => {
                    println!("{}", world.display());
                    ExitCode::SUCCESS
                }
                Err(why) => {
                    eprintln!("lyceum: {why}");
                    ExitCode::from(2)
                }
            }
        }
        "upgrade" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            if args.known(&[]).is_err()
                || args.words.len() != 1
                || args.switches.iter().any(|s| s != "check")
            {
                return usage("upgrade takes a world file and optional --check");
            }
            let check = args.switch("check");
            match lyceum_foundry::format::upgrade(Path::new(&args.words[0]), check) {
                Ok(paths) => {
                    println!(
                        "{} source file(s) {}; world.lock is unchanged.",
                        paths.len(),
                        if check { "need upgrading" } else { "upgraded" }
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error),
            }
        }
        "facts" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            if args.known(&[]).is_err() || args.words.len() != 1 || !args.switches.is_empty() {
                return usage("facts takes a world file");
            }
            match facts::facts(Path::new(&args.words[0])) {
                Ok(f) => {
                    println!("{}", f.json());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(&e),
            }
        }
        "-h" | "--help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        "--version" if rest.is_empty() => {
            print!("{}", version_text());
            ExitCode::SUCCESS
        }
        "build" | "validate" | "bake" | "export" | "import" | "edit" | "template" | "fit" => {
            let args = match parsed(rest) {
                Ok(a) => a,
                Err(why) => return usage(&why),
            };
            match first.as_str() {
                "build" => build(&args),
                "validate" => run_validate(&args),
                "bake" => run_bake(&args),
                "export" => run_export(&args),
                "edit" => run_edit(&args),
                "template" => run_template(&args),
                "fit" => run_fit(&args),
                _ => run_import(&args),
            }
        }
        "layer" => match parsed(rest) {
            Ok(a) => client_layer(&a),
            Err(why) => usage(&why),
        },
        "rows" => match parsed(rest) {
            Ok(a) => rows(&a),
            Err(why) => usage(&why),
        },
        "publish" => match parsed(rest) {
            Ok(a) => run_publish(&a),
            Err(why) => usage(&why),
        },
        "pull-dungeon" => match parsed(rest) {
            Ok(a) => run_pull(&a),
            Err(why) => usage(&why),
        },
        "lock" => match parsed(rest) {
            Ok(a) => lock(&a),
            Err(why) => usage(&why),
        },
        other => usage(&format!("{other} is not a command")),
    }
}
