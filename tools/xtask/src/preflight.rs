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
//!   world's machine. Before it builds, it pushes the commit to the remote's `preflight` branch,
//!   which the owner's mirror carries to GitHub, so that CI runs on it meanwhile.
//! - **`preflight play`**: the client and the launcher built, and the launcher started on the dev
//!   hub's world list; with `--web`, the web client the preflight built served on this machine
//!   alone, on the same list.
//! - **`preflight rollback`**: every world put back from its backup, the dev hub stopped.
//! - **`ship`**: CI waited for on exactly the preflighted commit, and refused when a check failed
//!   (`--skip-checks` does not ask, `--allow-failing` ships despite failures, `--local-tier0` runs
//!   tier 0 here too); `main` fast-forwarded to it (refused when the branch has moved since), and
//!   each changed product released and pushed, as `cargo xtask release` does it, in this process;
//!   with Dereth, the web client's release tagged on Dereth's. A ship that stopped carries on from
//!   the next release.
//! - **`preflight finish`**: the final builds of what was released, from `main`, installed in the
//!   dev worlds, each pointed back at the hub it named, and the dev hub stopped; after a ship that
//!   stopped partway, what it did not release is named, and its worlds keep their candidates.
//! - **`preflight setup`**: the one-time setup scripts, one per machine, written from the setup
//!   file.
//!
//! The setup file is the owner's ([`config`]); nothing here names a machine. What a run changed is
//! kept in the work folder ([`steps::Ledger`]), so any step can be undone.

pub mod ci;
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
       cargo xtask ship [--dry-run]                           once CI is green on the preflighted commit:
                        [--skip-checks | --allow-failing]     main to it, released (CI not asked; or
                        [--local-tier0]                       shipped despite failures; tier 0 here)

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

/// What the products are built from ([`plan::Built`]), read from `cargo metadata` of each
/// workspace a product ships from ([`plan::LOCKS`]): the main one at `root`, and the launcher's.
fn built(root: &Path) -> Result<plan::Built, String> {
    let mut metas = Vec::new();
    for lock in plan::LOCKS {
        let manifest = root.join(lock).with_file_name("Cargo.toml");
        let out = std::process::Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--offline",
                "--manifest-path",
            ])
            .arg(&manifest)
            .current_dir(root)
            .output()
            .map_err(|e| format!("cargo metadata: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "cargo metadata for {}: {}",
                manifest.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("cargo metadata for {}: {e}", manifest.display()))?;
        metas.push((lock, meta));
    }
    let built = plan::Built::read(&metas, root);
    match built.missing.first() {
        Some((product, package)) => Err(format!(
            "{}'s release ships {package}, which no workspace holds: name what it ships now in \
             `Product::ships` (tools/xtask/src/preflight/plan.rs)",
            product.command_name()
        )),
        None => Ok(built),
    }
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
    let built = built(root)?;
    // A lock file is weighed at the commit the changed files are counted from: the merge base.
    let base = git(root, &["merge-base", "main", "HEAD"])?;
    let moved = plan::lock_moves(
        &changed,
        &|lock| {
            let at = |commit: &str| {
                git(root, &["show", &format!("{commit}:{lock}")]).unwrap_or_default()
            };
            (at(&base), at("HEAD"))
        },
        &built.dev_only,
    )?;
    let products = plan::products_of(&changed, &built.packages, &built.depends, &moved);
    let web_alone = plan::web_alone(&changed, &built.packages, &built.depends, &moved);
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
    versions.insert(
        Product::Lyceum,
        crate::package::version::Product::Lyceum.version(root)?,
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
    bumps.insert(Product::Lyceum, plan::lyceum_bump());
    for (product, moves) in &moved {
        if let Some((_, why)) = bumps.get_mut(product) {
            why.push_str("; ");
            why.push_str(&plan::moves_why(moves));
        }
    }
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
        Some("finish") => with_config(|cfg, _| {
            finish(&mut Real, cfg, &workspace_root()).map(|said| println!("{said}"))
        }),
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
    if ledger.shipped.main.is_some() {
        return Err(
            "the last preflight is shipped partway: `cargo xtask ship` carries on from \
                    where it stopped, or `cargo xtask preflight finish` ends it"
                .into(),
        );
    }
    let releases = releases(repo, &mut ledger.stamped)?;
    // A Lyceum release that `ship` would refuse for want of highlights is said now, before
    // anything is built or stopped.
    if let Some(r) = releases.iter().find(|r| r.product == Product::Lyceum) {
        let file = crate::release::notes::LYCEUM_CHANGES;
        let text = m.read(&root.join(file))?;
        crate::release::check_highlights(file, &text, &r.version)?;
    }
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
    // The mirror carries the commit to GitHub, where CI runs on it while the candidates build and
    // the worlds are tried: `ship` waits for it.
    println!(
        "{}",
        ci::push(m, root, &cfg.remote, &repo.commit, opts.dry_run)?
    );
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
    ledger.pushed = (!opts.dry_run).then(|| repo.commit.clone());
    ledger.releases = releases;
    ledger.shipped = steps::Shipped::default();
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
    if m.port_taken(port) {
        return Err(format!(
            "port {port} is in use on this machine: give another with `--port <n>`"
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

/// After `ship`, or a ship that stopped partway: the final builds of the products it released,
/// from `main`, installed in the dev worlds that took their candidates. Answers what to say,
/// naming what was not released: no final build of it is made, and its worlds keep their
/// candidates.
fn finish(m: &mut dyn Machine, cfg: &Config, root: &Path) -> Result<String, String> {
    let mut ledger = Ledger::load(m, &cfg.work_dir)?;
    let partway = ledger.stage == Stage::Preflighted && !ledger.shipped.released.is_empty();
    if ledger.stage != Stage::Shipped && !partway {
        return Err(
            "nothing is shipped to finish: `cargo xtask ship` first (or `preflight rollback`)"
                .into(),
        );
    }
    let commit = m
        .run(Some(root), "git", &["rev-parse".into(), "main".into()], &[])?
        .trim()
        .to_owned();
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
        .filter(|r| ledger.released(r.product))
        .filter(|r| took.iter().any(|w| steps::product_of(w) == r.product))
        .map(|r| (r.product, r.version.clone()))
        .collect();
    let mut unreleased = Vec::new();
    for r in ledger
        .releases
        .iter()
        .filter(|r| !ledger.released(r.product))
    {
        let kept: Vec<String> = ledger
            .worlds
            .iter()
            .filter(|d| {
                took.iter()
                    .any(|w| w.name == d.name && steps::product_of(w) == r.product)
            })
            .map(|d| {
                format!(
                    "{} keeps its candidate {}",
                    d.name,
                    d.version.as_deref().unwrap_or_default()
                )
            })
            .collect();
        unreleased.push(format!(
            "  {} {}{}",
            r.product.command_name(),
            r.version,
            if kept.is_empty() {
                String::new()
            } else {
                format!(" ({})", kept.join(", "))
            }
        ));
    }
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
    let builds = steps::build(m, root, &cfg.work_dir, &commit, &wanted)?;
    let r = steps::finish(m, cfg, &builds, &mut ledger);
    ledger.save(m, &cfg.work_dir)?;
    r?;
    let mut said = "The dev worlds run the released builds and link to their own hubs again; the \
                    dev hub is stopped."
        .to_owned();
    if !unreleased.is_empty() {
        said.push_str(&format!(
            "\nNot released, so no final build of it is installed:\n{}",
            unreleased.join("\n")
        ));
    }
    Ok(said)
}

/// Tier 0 run on this checkout as `cargo xtask ci tier0` runs it, by this program itself (never by
/// starting cargo, whose build of this program could not replace it while it runs), its output
/// shown as it comes and kept in `log`. Whether it passed.
fn tier0_logged(root: &Path, log: &Path) -> Result<bool, String> {
    use std::io::{Read, Write};
    let failed = |e: std::io::Error| format!("{}: {e}", log.display());
    let file = std::fs::File::create(log).map_err(failed)?;
    let err = file.try_clone().map_err(failed)?;
    let mut shown = std::fs::File::open(log).map_err(failed)?;
    let exe = std::env::current_exe().map_err(|e| format!("this program: {e}"))?;
    let mut child = std::process::Command::new(exe)
        .args(["ci", "tier0"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(file)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("cargo xtask ci tier0: {e}"))?;
    let mut out = std::io::stdout();
    let mut chunk = Vec::new();
    loop {
        // Asked before the file is read, so that the last read shows everything it wrote.
        let done = child
            .try_wait()
            .map_err(|e| format!("cargo xtask ci tier0: {e}"))?;
        chunk.clear();
        shown.read_to_end(&mut chunk).map_err(failed)?;
        let _ = out.write_all(&chunk);
        let _ = out.flush();
        if let Some(status) = done {
            return Ok(status.success());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// `ship`'s options.
fn parse_ship(args: &[String]) -> Result<ship::Options, String> {
    let mut o = ship::Options::default();
    for a in args {
        match a.as_str() {
            "--dry-run" => o.dry_run = true,
            "--skip-checks" => o.skip_checks = true,
            "--allow-failing" => o.allow_failing = true,
            "--local-tier0" => o.local_tier0 = true,
            other => return Err(format!("unexpected `{other}`\n\n{USAGE}")),
        }
    }
    if o.skip_checks && o.allow_failing {
        return Err(format!(
            "--allow-failing takes the checks `--skip-checks` does not ask for: give one\n\n{USAGE}"
        ));
    }
    Ok(o)
}

/// `cargo xtask ship [--dry-run] [--skip-checks | --allow-failing] [--local-tier0]`.
pub fn ship_command(args: &[String]) -> i32 {
    let opts = match parse_ship(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let dry_run = opts.dry_run;
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
        // build of this program could not replace it while it runs. The releases leave tier 0
        // out: CI ran it on the commit `ship` ships, and `--local-tier0` runs it here once.
        let mut cut = |product: Product, version: &str| {
            crate::release::cut(
                product.command_name(),
                version,
                true,
                &remote,
                ace.as_deref(),
                false,
            )
        };
        let mut tier0 = |log: &Path| tier0_logged(&root, log);
        let r = ship::ship(m, cfg, &root, &mut cut, &mut tier0, &mut ledger, &opts);
        ledger.save(m, &cfg.work_dir)?;
        // Finished or stopped, how to send each tag it made again, on its own.
        if let Some(resends) = ship::tag_resends(&ledger, &cfg.remote).filter(|_| !dry_run) {
            println!("\n{resends}");
        }
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
