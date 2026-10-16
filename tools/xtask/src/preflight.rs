//! `cargo xtask preflight` and `cargo xtask ship`: the branch being released tried on the owner's
//! own dev setup before anything is pushed, then shipped in one command.
//!
//! - **`preflight`** (on the branch checked out): works out which products it changed since `main` and the
//!   release each takes, and why; checks that what the builds need here and what the setup file
//!   names are there, before anything is built or stopped; builds each in a temporary worktree of
//!   the commit, stamped `<next version>-rc.N` (Dereth's as the web client); runs the dev hub on a
//!   copy of the real hub's registry, backed up over ssh, migrated by the candidate and defanged
//!   (no Discord mirror, no sign-in, no person's link but the dev hub's admins'), behind its
//!   tunnel, and waits for it to answer; and for each dev world stops it, backs it up, installs
//!   the candidate, points its hub setting at the dev hub (keeping the original) and starts it,
//!   then says which worlds linked to the dev hub and which did not. A world on another machine,
//!   Windows or Linux, is reached over ssh, and takes a build made here: nothing is built on a
//!   world's machine.
//! - **`preflight play`**: the client and the launcher built, and the launcher started on the dev
//!   hub's world list; with `--web`, the web client the preflight built served on this machine
//!   alone, on the same list.
//! - **`preflight rollback`**: every world put back from its backup, the dev hub stopped.
//! - **`ship`**: `main` fast-forwarded to exactly the preflighted commit (refused when the branch
//!   has moved since), and each changed product released and pushed, as `cargo xtask release`
//!   does it, in this process; with Dereth, the web client's release tagged on Dereth's.
//! - **`preflight finish`**: the final builds, from `main`, installed in the dev worlds, each
//!   pointed back at the hub it named, and the dev hub stopped.
//! - **`preflight setup`**: the one-time setup scripts, one per machine, written from the setup
//!   file.
//!
//! The setup file is the owner's ([`config`]); nothing here names a machine. What a run changed is
//! kept in the work folder ([`steps::Ledger`]), so any step can be undone.

pub mod config;
pub mod edit;
pub mod linux;
pub mod machine;
pub mod plan;
pub mod setup;
pub mod ship;
pub mod steps;
pub mod windows;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use config::{Config, Kind, Os};
use machine::{Machine, Real};
use plan::{Bump, Product};
use steps::{Ledger, Release, Stage, Wanted};

use crate::util::workspace_root;

const USAGE: &str = "\
usage: cargo xtask preflight [--with <world>]... [--dry-run]   on a branch: build, dev hub, dev worlds
       cargo xtask preflight play                             the client and launcher on the dev hub
       cargo xtask preflight play --web [--port <n>]          the web client's build, served here alone
       cargo xtask preflight rollback                         every world put back, the dev hub stopped
       cargo xtask preflight finish                           after ship: the final builds, the real hub
       cargo xtask preflight status                           what is installed where
       cargo xtask preflight setup [--out <folder>]           the one-time setup, a script per machine
       cargo xtask ship [--dry-run]                           main to the preflighted commit, released

The setup file is %APPDATA%\\Dereth\\preflight.toml (DERETH_PREFLIGHT names another); see
network/RELEASING.md.";

/// What the repository says before a preflight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub branch: String,
    pub commit: String,
    pub dirty: Vec<String>,
    /// The products the branch changed since `main`.
    pub products: BTreeSet<Product>,
    /// Each product's version in the tree now.
    pub versions: BTreeMap<Product, String>,
    /// Each product's release and why.
    pub bumps: BTreeMap<Product, (Bump, String)>,
}

/// The releases a preflight of `repo` works on, each stamped as its next candidate in `stamped`.
pub fn releases(repo: &Repo, stamped: &mut BTreeMap<String, u32>) -> Result<Vec<Release>, String> {
    let mut out = Vec::new();
    for p in &repo.products {
        let from = repo
            .versions
            .get(p)
            .ok_or_else(|| format!("no version of {}", p.command_name()))?;
        let (bump, why) = repo
            .bumps
            .get(p)
            .cloned()
            .unwrap_or((Bump::Patch, "a patch release".to_owned()));
        let version = plan::next_version(from, bump)?;
        let candidate = plan::stamp(*p, &version, stamped);
        out.push(Release {
            product: *p,
            from: from.clone(),
            version,
            candidate,
            bump,
            why,
        });
    }
    Ok(out)
}

/// The usual setup file, or the one `DERETH_PREFLIGHT` names.
fn config_path() -> Result<PathBuf, String> {
    config::default_path(&|k| std::env::var(k).ok(), cfg!(windows))
        .ok_or_else(|| "no setup file: set DERETH_PREFLIGHT, or APPDATA".to_owned())
}

fn load_config() -> Result<(Config, PathBuf), String> {
    let path = config_path()?;
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "the setup file {}: {e}\n(network/RELEASING.md says what it holds)",
            path.display()
        )
    })?;
    Ok((Config::parse(&text)?, path))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    crate::package::git(root, args).map(|o| o.trim().to_owned())
}

/// The workspace's packages, each as (name, folder relative to the root).
type Packages = Vec<(String, String)>;

/// Each product's packages: its own and everything it depends on in the workspace.
type Depends = BTreeMap<Product, BTreeSet<String>>;

/// The workspace's packages and each product's, read from `cargo metadata` at `root`.
fn packages(root: &Path) -> Result<(Packages, Depends), String> {
    let out = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--offline"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    let mut names = BTreeMap::new();
    let mut dirs = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let id = p["id"].as_str().unwrap_or_default();
        if !members.contains(id) {
            continue;
        }
        let name = p["name"].as_str().unwrap_or_default().to_owned();
        let manifest = PathBuf::from(p["manifest_path"].as_str().unwrap_or_default());
        let dir = manifest
            .parent()
            .and_then(|d| d.strip_prefix(root).ok())
            .map(|d| d.display().to_string().replace('\\', "/"))
            .unwrap_or_default();
        names.insert(id.to_owned(), name.clone());
        dirs.push((name, dir));
    }
    let mut deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in meta["resolve"]["nodes"].as_array().into_iter().flatten() {
        let Some(name) = n["id"].as_str().and_then(|id| names.get(id)) else {
            continue;
        };
        let to = n["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d.as_str().and_then(|id| names.get(id)).cloned())
            .collect();
        deps.insert(name.clone(), to);
    }
    let mut depends = BTreeMap::new();
    for p in Product::ALL {
        let Some(root_pkg) = p.package() else {
            continue;
        };
        let mut seen = BTreeSet::new();
        let mut todo = vec![root_pkg.to_owned()];
        // The web client is Dereth's too, and carries its version, though the client does not
        // depend on it.
        if p == Product::Dereth {
            todo.push("dereth-web".to_owned());
        }
        while let Some(x) = todo.pop() {
            if seen.insert(x.clone()) {
                todo.extend(deps.get(&x).cloned().unwrap_or_default());
            }
        }
        // The launcher's own crate is Dereth's, though the client does not depend on it.
        if p == Product::Dereth {
            seen.insert("dereth-launch".to_owned());
        }
        depends.insert(p, seen);
    }
    Ok((dirs, depends))
}

/// The repository as a preflight reads it.
fn read_repo(root: &Path) -> Result<Repo, String> {
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let commit = git(root, &["rev-parse", "HEAD"])?;
    let dirty = crate::package::uncommitted(root)?;
    let changed: Vec<String> = git(root, &["diff", "--name-only", "main...HEAD"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let (dirs, depends) = packages(root)?;
    let products = plan::products_of(&changed, &dirs, &depends);
    let web_alone = plan::web_alone(&changed, &dirs, &depends);
    let mut versions = BTreeMap::new();
    versions.insert(
        Product::Hub,
        crate::release::network::Part::Hub.version(root)?,
    );
    versions.insert(
        Product::Ace,
        crate::release::network::Part::Ace.version(root)?,
    );
    versions.insert(
        Product::Empyrean,
        crate::package::version::Product::Empyrean.version(root)?,
    );
    versions.insert(
        Product::Dereth,
        crate::package::version::Product::Dereth.version(root)?,
    );
    let mut bumps = BTreeMap::new();
    let schema_now = plan::hub_schema(
        &std::fs::read_to_string(root.join("network/hub/src/registry.rs")).unwrap_or_default(),
    );
    let schema_main = plan::hub_schema(
        &git(root, &["show", "main:network/hub/src/registry.rs"]).unwrap_or_default(),
    );
    bumps.insert(Product::Hub, plan::hub_bump(schema_main, schema_now));
    let patch = plan::next_version(&versions[&Product::Empyrean], Bump::Patch)?;
    let refusal = crate::package::declaration::for_release(root, &patch).err();
    bumps.insert(Product::Empyrean, plan::empyrean_bump(refusal.as_deref()));
    bumps.insert(Product::Ace, (Bump::Patch, "the mod's changes".to_owned()));
    bumps.insert(Product::Dereth, plan::dereth_bump(web_alone));
    Ok(Repo {
        branch,
        commit,
        dirty,
        products,
        versions,
        bumps,
    })
}

/// `cargo xtask preflight ...`.
pub fn preflight(args: &[String]) -> i32 {
    let outcome = match args.first().map(String::as_str) {
        Some("play") => match parse_play(&args[1..]) {
            Ok(None) => with_config(|cfg, _| play(cfg)),
            Ok(Some(port)) => with_config(|cfg, _| {
                let m = &mut Real;
                let ledger = Ledger::load(m, &cfg.work_dir)?;
                let (pid, url) = play_web(m, cfg, &ledger, &workspace_root(), port)?;
                println!(
                    "The web client is served on this machine alone (process {pid}, its log {}), on \
                     the dev hub's world list. Open {url}",
                    cfg.work_dir.join("web.log").display()
                );
                Ok(())
            }),
            Err(e) => Err(e),
        },
        Some("rollback") => {
            with_config(|cfg, _| {
                let m = &mut Real;
                let mut ledger = Ledger::load(m, &cfg.work_dir)?;
                let r = steps::rollback(m, cfg, &mut ledger);
                ledger.save(m, &cfg.work_dir)?;
                r.map(|()| println!("Every world is as it was before the preflight, and the dev hub is stopped."))
            })
        }
        Some("finish") => with_config(|cfg, _| finish(cfg)),
        Some("status") => with_config(|cfg, _| {
            let ledger = Ledger::load(&mut Real, &cfg.work_dir)?;
            print!("{}", steps::summary(cfg, &ledger));
            Ok(())
        }),
        Some("setup") => with_config(|cfg, path| {
            let key = public_key(cfg)?;
            let scripts = setup::scripts(cfg, &path.display().to_string(), key.as_deref())?;
            match args.iter().position(|a| a == "--out") {
                Some(i) => {
                    let out = Path::new(args.get(i + 1).ok_or(USAGE)?);
                    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
                    for s in &scripts {
                        let file = out.join(&s.file);
                        std::fs::write(&file, &s.text)
                            .map_err(|e| format!("{}: {e}", file.display()))?;
                        println!("wrote {}: read it, then {}", file.display(), s.run);
                    }
                    Ok(())
                }
                None => {
                    for s in &scripts {
                        print!("# ---- {}: {}\n{}\n", s.file, s.run, s.text);
                    }
                    Ok(())
                }
            }
        }),
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        _ => run_args(args),
    };
    match outcome {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\npreflight: STOPPED\n{e}");
            1
        }
    }
}

fn with_config(f: impl FnOnce(&Config, &Path) -> Result<(), String>) -> Result<(), String> {
    let (cfg, path) = load_config()?;
    f(&cfg, &path)
}

/// The public key the Windows world machines authorize, when there are any: the setup file's
/// `ssh_key`, or the first of this account's usual ones.
fn public_key(cfg: &Config) -> Result<Option<String>, String> {
    if !cfg.machines.iter().any(|h| h.os == Os::Windows) {
        return Ok(None);
    }
    let files = match &cfg.ssh_key {
        Some(f) => vec![f.clone()],
        None => {
            let home = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_default();
            ["id_ed25519.pub", "id_ecdsa.pub", "id_rsa.pub"]
                .iter()
                .map(|f| home.join(".ssh").join(f))
                .collect()
        }
    };
    for f in &files {
        if let Ok(text) = std::fs::read_to_string(f) {
            return setup::public_key(&text)
                .map(Some)
                .map_err(|e| format!("{}: {e}", f.display()));
        }
    }
    Err(format!(
        "no public key for the Windows machines to authorize (looked for {}): give `ssh_key` in \
         the setup file, or make a key with ssh-keygen",
        files
            .iter()
            .map(|f| f.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// The options of a preflight run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub with: Vec<String>,
    pub dry_run: bool,
}

fn parse_run(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--with" => o.with.push(it.next().ok_or(USAGE)?.clone()),
            "--dry-run" => o.dry_run = true,
            other => return Err(format!("unexpected `{other}`\n\n{USAGE}")),
        }
    }
    Ok(o)
}

fn run_args(args: &[String]) -> Result<(), String> {
    let opts = parse_run(args)?;
    let (cfg, _) = load_config()?;
    let root = workspace_root();
    let repo = read_repo(&root)?;
    if opts.dry_run {
        run(&mut machine::DryRun, &cfg, &root, &repo, &opts)
    } else {
        run(&mut Real, &cfg, &root, &repo, &opts)
    }
}

/// A preflight of `repo` on `m`.
pub fn run(
    m: &mut dyn Machine,
    cfg: &Config,
    root: &Path,
    repo: &Repo,
    opts: &Options,
) -> Result<(), String> {
    if repo.branch == "HEAD" {
        return Err(
            "a preflight runs on a branch, the one to release; this checkout is detached".into(),
        );
    }
    if !repo.dirty.is_empty() {
        return Err(format!(
            "the tree has uncommitted changes: {}",
            repo.dirty.join("; ")
        ));
    }
    let worlds = cfg.worlds_for(&opts.with)?;
    let mut ledger = Ledger::load(m, &cfg.work_dir)?;
    if ledger.stage == Stage::Shipped {
        return Err("the last preflight is shipped: `cargo xtask preflight finish` first".into());
    }
    let releases = releases(repo, &mut ledger.stamped)?;
    // The hub is always built: the dev hub runs it, changed or not.
    let mut versions: BTreeMap<Product, String> = releases
        .iter()
        .map(|r| (r.product, r.candidate.clone()))
        .collect();
    versions
        .entry(Product::Hub)
        .or_insert_with(|| repo.versions[&Product::Hub].clone());
    // The mod only for a world that runs it.
    if !worlds.iter().any(|w| w.kind == Kind::Ace) {
        versions.remove(&Product::Ace);
    }
    let targets = steps::targets(&versions, &worlds);
    // Before anything is built or stopped: the tools the builds need here, and what the setup file
    // names, all of it in one answer.
    steps::check_ahead(m, cfg, &worlds, &versions, opts.dry_run)?;
    println!("Preflighting {} at {}:", repo.branch, repo.commit);
    if releases.is_empty() {
        println!("  no product changed since main");
    }
    for r in &releases {
        println!(
            "  {} {} -> {} ({:?}), built as {}: {}",
            r.product.command_name(),
            r.from,
            r.version,
            r.bump,
            r.candidate,
            r.why
        );
    }
    // Each ACE world's mod is built against its own server.
    let ace = if versions.contains_key(&Product::Ace) {
        steps::ace_folders(m, cfg, &worlds)?
    } else {
        BTreeMap::new()
    };
    let wanted = Wanted {
        versions,
        targets,
        ace,
    };
    let builds = if opts.dry_run {
        // Nothing is built: the steps after it are shown with where the builds would be.
        steps::planned(&cfg.work_dir, &wanted)
    } else {
        steps::build(m, root, &cfg.work_dir, &repo.commit, &wanted)?
    };
    ledger.stage = Stage::Preflighted;
    ledger.commit = Some(repo.commit.clone());
    ledger.branch = Some(repo.branch.clone());
    ledger.releases = releases;
    ledger.save(m, &cfg.work_dir)?;
    let hub = builds
        .hub
        .as_ref()
        .ok_or("the hub was not built")?
        .path
        .clone();
    steps::dev_hub(m, cfg, &hub, &mut ledger)?;
    ledger.save(m, &cfg.work_dir)?;
    // No world is moved onto a dev hub that is not there.
    if opts.dry_run {
        println!(
            "would wait for the dev hub to answer at http://{}/v1/servers",
            cfg.hub.listen
        );
    } else {
        let answered = steps::await_dev_hub(m, cfg, &mut ledger);
        ledger.save(m, &cfg.work_dir)?;
        answered?;
    }
    let mut started = Vec::new();
    for w in worlds {
        let product = steps::product_of(w);
        let changed = ledger.releases.iter().any(|r| r.product == product);
        let build = builds.for_world(w).filter(|_| changed);
        let r = steps::install(m, cfg, w, build, &mut ledger);
        ledger.save(m, &cfg.work_dir)?;
        let slug = r.map_err(|e| {
            format!(
                "{}: {e}\nWhat was done is kept: `cargo xtask preflight rollback` undoes it.",
                w.name
            )
        })?;
        started.push((w, slug));
    }
    let links = if opts.dry_run {
        println!("would wait for each world to link to the dev hub");
        String::new()
    } else {
        println!(
            "Waiting up to {} minutes for each world to link to the dev hub...",
            u64::from(steps::LINK_TRIES) * steps::LINK_PAUSE_MS / 60_000
        );
        steps::links_summary(cfg, &steps::await_links(m, cfg, &started))
    };
    print!("\n{}{links}", steps::summary(cfg, &ledger));
    Ok(())
}

/// The client and the launcher built from this checkout, and the launcher started on the dev
/// hub's world list.
fn play(cfg: &Config) -> Result<(), String> {
    let root = workspace_root();
    let target = crate::util::target_dir();
    let m = &mut Real;
    m.run(
        Some(&root),
        "cargo",
        &[
            "build".into(),
            "--release".into(),
            "-p".into(),
            "dereth-client".into(),
        ],
        &[],
    )?;
    let launcher_target = target.join("launcher");
    m.run(
        Some(&root),
        "cargo",
        &[
            "build".into(),
            "--release".into(),
            "--manifest-path".into(),
            "dereth/launcher/Cargo.toml".into(),
            "--target-dir".into(),
            launcher_target.display().to_string(),
        ],
        &[],
    )?;
    let client = target
        .join("release")
        .join(format!("dereth-client{}", std::env::consts::EXE_SUFFIX));
    let launcher = launcher_target
        .join("release")
        .join(format!("dereth{}", std::env::consts::EXE_SUFFIX));
    let list = format!("{}/v1/servers", cfg.hub.public_url.trim_end_matches('/'));
    let pid = m.spawn(
        &launcher.display().to_string(),
        &[],
        &[
            ("DERETH_HUB_LIST".to_owned(), list.clone()),
            ("DERETH_CLIENT".to_owned(), client.display().to_string()),
        ],
        &cfg.work_dir.join("launcher.log"),
    )?;
    println!("The launcher runs (process {pid}) on the dev hub's world list, {list}.");
    Ok(())
}

/// The port the web client is served on by default, as `cargo xtask web` serves it.
const WEB_PORT: u16 = 8080;

/// `preflight play`'s options: `None` for the desktop client and launcher, or the port the web
/// client is served on (`--web`, `--port`).
fn parse_play(args: &[String]) -> Result<Option<u16>, String> {
    let mut web = false;
    let mut port = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--web" => web = true,
            "--port" => {
                let p = it.next().ok_or(USAGE)?;
                port = Some(p.parse().map_err(|e| format!("--port {p}: {e}"))?);
            }
            other => return Err(format!("unexpected `{other}`\n\n{USAGE}")),
        }
    }
    match (web, port) {
        (false, Some(_)) => Err(format!("--port is the web client's: give --web\n\n{USAGE}")),
        (false, None) => Ok(None),
        (true, port) => Ok(Some(port.unwrap_or(WEB_PORT))),
    }
}

/// `s` as a URL's query value: every byte but the unreserved ones percent-encoded.
fn query_value(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The web client the preflight built, served from `root`'s web runner on this machine alone (it
/// listens on loopback only) at `port`, its page reading the dev hub's world list. Answers the
/// runner's process and the page's address. Nothing opens it.
fn play_web(
    m: &mut dyn Machine,
    cfg: &Config,
    ledger: &Ledger,
    root: &Path,
    port: u16,
) -> Result<(u32, String), String> {
    let list = format!("{}/v1/servers", cfg.hub.public_url.trim_end_matches('/'));
    let Some(r) = ledger
        .releases
        .iter()
        .find(|r| r.product == Product::Dereth)
    else {
        return Err(format!(
            "this preflight built no web client: Dereth did not change since main, so it is not \
             released. `cargo xtask web --hub {list}` serves this checkout's on the dev hub's \
             world list."
        ));
    };
    let bundle = steps::web_path(&cfg.work_dir, &r.candidate);
    if !m.exists(&bundle) {
        return Err(format!(
            "the web client {} is not in {}: run `cargo xtask preflight` again",
            r.candidate,
            bundle.display()
        ));
    }
    m.run(
        Some(root),
        "cargo",
        &[
            "build".into(),
            "--release".into(),
            "-p".into(),
            "dereth-web-dev".into(),
        ],
        &[],
    )?;
    let runner = crate::util::target_dir()
        .join("release")
        .join(format!("dereth-web-dev{}", std::env::consts::EXE_SUFFIX));
    let pid = m.spawn(
        &runner.display().to_string(),
        &[
            "--www".into(),
            bundle.display().to_string(),
            "--port".into(),
            port.to_string(),
            "--hub".into(),
            list.clone(),
        ],
        &[],
        &cfg.work_dir.join("web.log"),
    )?;
    Ok((
        pid,
        format!("http://127.0.0.1:{port}/?hub={}", query_value(&list)),
    ))
}

/// After `ship`: the final builds, from `main`, installed in the dev worlds.
fn finish(cfg: &Config) -> Result<(), String> {
    let root = workspace_root();
    let m = &mut Real;
    let mut ledger = Ledger::load(m, &cfg.work_dir)?;
    if ledger.stage != Stage::Shipped {
        return Err(
            "nothing is shipped to finish: `cargo xtask ship` first (or `preflight rollback`)"
                .into(),
        );
    }
    let commit = git(&root, &["rev-parse", "main"])?;
    // The final build of each world's product, for the worlds that took a candidate.
    let took: Vec<&config::World> = cfg
        .worlds
        .iter()
        .filter(|w| {
            ledger
                .worlds
                .iter()
                .any(|d| d.name == w.name && d.version.is_some())
        })
        .collect();
    let versions: BTreeMap<Product, String> = ledger
        .releases
        .iter()
        .filter(|r| took.iter().any(|w| steps::product_of(w) == r.product))
        .map(|r| (r.product, r.version.clone()))
        .collect();
    let targets = steps::targets(&versions, &took);
    steps::check_tools(m, &targets, versions.contains_key(&Product::Ace))?;
    let ace = if versions.contains_key(&Product::Ace) {
        steps::ace_folders(m, cfg, &took)?
    } else {
        BTreeMap::new()
    };
    let wanted = Wanted {
        versions,
        targets,
        ace,
    };
    let builds = steps::build(m, &root, &cfg.work_dir, &commit, &wanted)?;
    let r = steps::finish(m, cfg, &builds, &mut ledger);
    ledger.save(m, &cfg.work_dir)?;
    r?;
    println!("The dev worlds run the released builds and link to their own hubs again; the dev hub is stopped.");
    Ok(())
}

/// `cargo xtask ship [--dry-run]`.
pub fn ship_command(args: &[String]) -> i32 {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    if let Some(other) = args.iter().find(|a| *a != "--dry-run") {
        eprintln!("unexpected `{other}`\n\n{USAGE}");
        return 2;
    }
    let outcome = with_config(|cfg, _| {
        let root = workspace_root();
        let m = &mut Real;
        let mut ledger = Ledger::load(m, &cfg.work_dir)?;
        // The mod's release checks what needs ACE against this ACE server: `DERETH_ACE_DIR`'s, as
        // the release command reads it, or else the setup file's.
        let ace = std::env::var_os("DERETH_ACE_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| cfg.release_ace_dir());
        let remote = cfg.remote.clone();
        // Each release cut here, as `cargo xtask release` cuts it: never by starting cargo, whose
        // build of this program could not replace it while it runs.
        let mut cut = |product: Product, version: &str| {
            crate::release::cut(
                product.command_name(),
                version,
                true,
                &remote,
                ace.as_deref(),
            )
        };
        let r = ship::ship(m, &root, &cfg.remote, &mut cut, &mut ledger, dry_run);
        ledger.save(m, &cfg.work_dir)?;
        r?;
        if !dry_run {
            println!("Shipped. The release workflows draft each release; `cargo xtask preflight finish` puts the final builds in the dev worlds.");
        }
        Ok(())
    });
    match outcome {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\nship: STOPPED\n{e}");
            1
        }
    }
}
