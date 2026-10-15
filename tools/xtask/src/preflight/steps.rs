//! The preflight's steps, on a [`Machine`]: the builds, the dev hub on a defanged copy of the real
//! registry, and each dev world stopped, backed up, given the new build, pointed at the dev hub
//! and started; then rolling it all back, or finishing with the final builds. Every build is made
//! here, a world on another machine taking its copy over scp: Empyrean for a Linux machine with
//! `cargo zigbuild`, the mod against the ACE server of the world it goes to. A world is in the
//! [`Ledger`] from the moment its backup is whole, and the ledger is saved then and after every
//! world, so that a run cut short can still be rolled back.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::config::{Config, Host, Kind, Place, World};
use super::edit;
use super::machine::{powershell, Machine, Service};
use super::plan::{Bump, Product};
use super::{linux, windows};

/// The ledger's file, in the work folder.
pub const LEDGER: &str = "preflight.json";

/// Where a preflight stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Nothing is installed.
    #[default]
    Idle,
    /// A commit of a branch is installed in the dev worlds, which link to the dev hub.
    Preflighted,
    /// That commit is on `main` and released; the dev worlds still run its candidates.
    Shipped,
}

/// One product's release, as the preflight worked it out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub product: Product,
    /// The version the tree carries now.
    pub from: String,
    /// The release it takes.
    pub version: String,
    /// The candidate the preflight built and installed.
    pub candidate: String,
    pub bump: Bump,
    pub why: String,
}

/// The dev hub's processes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevHub {
    pub pid: u32,
    pub tunnel: Option<u32>,
}

/// One dev world as the preflight left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Installed {
    pub name: String,
    /// The hub address its configuration named before the preflight (none: the network's own).
    pub original_hub: Option<String>,
    /// Its files as they were, kept in the work folder (or, on another machine, beside the world).
    pub backup: String,
    /// The version installed, when the preflight installed one.
    pub version: Option<String>,
    /// For an Empyrean world on another machine: its database files, as that machine spells
    /// them, which a rollback puts back from the backup there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub databases: Vec<String>,
}

/// What the preflight did, and what it needs to undo it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub stage: Stage,
    /// The commit that was preflighted, and the branch it was on.
    pub commit: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    pub releases: Vec<Release>,
    /// The candidates built of each release, by `<product> <version>`: the next is one past.
    #[serde(default)]
    pub stamped: BTreeMap<String, u32>,
    pub hub: Option<DevHub>,
    /// Each table of the copied registry: its rows before the candidate hub migrated it, and after
    /// it was migrated and defanged.
    #[serde(default)]
    pub counts: Vec<(String, Option<i64>, Option<i64>)>,
    pub worlds: Vec<Installed>,
}

impl Ledger {
    /// The ledger in `work`, or a new one.
    pub fn load(m: &mut dyn Machine, work: &Path) -> Result<Self, String> {
        let path = work.join(LEDGER);
        if !m.exists(&path) {
            return Ok(Self::default());
        }
        serde_json::from_str(&m.read(&path)?).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn save(&self, m: &mut dyn Machine, work: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        m.write(&work.join(LEDGER), &format!("{text}\n"))
    }

    fn world(&self, name: &str) -> Option<&Installed> {
        self.worlds.iter().find(|w| w.name == name)
    }
}

/// A build, ready to install: the file (the hub, Empyrean's server) or folder (the mod).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    pub version: String,
    pub path: PathBuf,
}

/// What a run builds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    /// Each product's version.
    pub versions: BTreeMap<Product, String>,
    /// The targets Empyrean's server is built for: a Linux machine's, or none for this machine's.
    pub targets: BTreeSet<Option<String>>,
    /// The ACE server folder each ACE world's mod is built against, by world.
    pub ace: BTreeMap<String, PathBuf>,
}

/// The builds of a run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Builds {
    /// The hub, for this machine.
    pub hub: Option<Build>,
    /// Empyrean's server, by target (none: this machine's).
    pub empyrean: BTreeMap<Option<String>, Build>,
    /// The mod, by the ACE world it was built against.
    pub mods: BTreeMap<String, Build>,
}

impl Builds {
    /// The build made for `w`, if there is one.
    pub fn for_world(&self, w: &World) -> Option<&Build> {
        match w.kind {
            Kind::Empyrean => self.empyrean.get(&target_of(w)),
            Kind::Ace => self.mods.get(&w.name),
        }
    }
}

/// The product a world runs.
pub fn product_of(w: &World) -> Product {
    match w.kind {
        Kind::Empyrean => Product::Empyrean,
        Kind::Ace => Product::Ace,
    }
}

/// The target `w`'s server is built for: its Linux machine's, or none for this machine's (a
/// world here or on another Windows machine).
pub fn target_of(w: &World) -> Option<String> {
    match w.place() {
        Place::Linux(h) => Some(h.target().to_owned()),
        Place::Here | Place::Windows(_) => None,
    }
}

/// The targets Empyrean's server is built for when `versions` has it, for `worlds`: each one's
/// (this machine's when none of them runs it, so that it is still built).
pub fn targets(
    versions: &BTreeMap<Product, String>,
    worlds: &[&World],
) -> BTreeSet<Option<String>> {
    if !versions.contains_key(&Product::Empyrean) {
        return BTreeSet::new();
    }
    let mut out: BTreeSet<Option<String>> = worlds
        .iter()
        .filter(|w| w.kind == Kind::Empyrean)
        .map(|w| target_of(w))
        .collect();
    if out.is_empty() {
        out.insert(None);
    }
    out
}

/// A zigbuild target as rustup names it: without the glibc floor (`x86_64-unknown-linux-gnu.2.28`
/// is `x86_64-unknown-linux-gnu`). Its builds are in the target folder under this name.
pub fn rust_target(target: &str) -> &str {
    match target.split_once('.') {
        Some((triple, floor))
            if floor
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) =>
        {
            triple
        }
        _ => target,
    }
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| (*x).to_owned()).collect()
}

/// What building for `targets` needs here, asked before anything is built or stopped: for a Linux
/// machine's target, `cargo zigbuild`, the zig it runs, and the target's standard library.
pub fn check_tools(
    m: &mut dyn Machine,
    targets: &BTreeSet<Option<String>>,
    ace: bool,
) -> Result<(), String> {
    let mut missing = Vec::new();
    // The ACE mod is built here, for .NET 10.
    if ace {
        let sdks = m
            .probe("dotnet", &args(&["--list-sdks"]))
            .unwrap_or_default();
        if !sdks.lines().any(|l| l.trim_start().starts_with("10.")) {
            missing.push(
                "the .NET 10 SDK, which builds the ACE mod: `winget install --exact --id Microsoft.DotNet.SDK.10`"
                    .to_owned(),
            );
        }
    }
    let linux: BTreeSet<&str> = targets.iter().flatten().map(|t| rust_target(t)).collect();
    if linux.is_empty() {
        return finish_check(missing);
    }
    // Its own binary: `cargo zigbuild` takes no `--version`, and refuses one.
    if m.probe("cargo-zigbuild", &args(&["--version"])).is_err() {
        missing.push("cargo zigbuild: `cargo install --locked cargo-zigbuild`".to_owned());
    }
    if m.probe("zig", &args(&["version"])).is_err()
        && m.probe("python3", &args(&["-m", "ziglang", "version"]))
            .is_err()
    {
        missing.push(
            "zig, which cargo zigbuild runs: `winget install --exact --id zig.zig`".to_owned(),
        );
    }
    match m.probe("rustup", &args(&["target", "list", "--installed"])) {
        Ok(installed) => {
            for t in linux {
                if !installed.lines().any(|l| l.trim() == t) {
                    missing.push(format!("the {t} standard library: `rustup target add {t}`"));
                }
            }
        }
        Err(e) => missing.push(format!(
            "rustup, which says what targets are installed: {e}"
        )),
    }
    finish_check(missing)
}

/// The tools check's answer: nothing missing, or what is, before anything was built or stopped.
fn finish_check(missing: Vec<String>) -> Result<(), String> {
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "The builds made here need tools this machine lacks:\n  {}\nNothing was built or \
             stopped. The setup script for this machine (`cargo xtask preflight setup`) installs \
             them.",
            missing.join("\n  ")
        ))
    }
}

/// The ACE server folder the mod is built against for each ACE world of `worlds`: its own folder
/// on this machine, or, for a world on another Windows machine, a copy of its server's assemblies
/// fetched into the work folder.
pub fn ace_folders(
    m: &mut dyn Machine,
    cfg: &Config,
    worlds: &[&World],
) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut out = BTreeMap::new();
    for w in worlds.iter().filter(|w| w.kind == Kind::Ace) {
        let folder = match w.place() {
            Place::Here => PathBuf::from(&w.dir),
            Place::Windows(h) => {
                m.ssh(&h.ssh, &powershell(&windows::assemblies(w)))?;
                let local = cfg.fetched_ace(w);
                m.remove(&local)?;
                m.fetch(&h.ssh, &windows::staged(w, "ace"), &local)?;
                local
            }
            Place::Linux(_) => {
                return Err(format!(
                    "{}: an ACE world on a Linux machine is not installed by the preflight",
                    w.name
                ))
            }
        };
        out.insert(w.name.clone(), folder);
    }
    Ok(out)
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

fn s(x: impl Into<String>) -> String {
    x.into()
}

fn hub_path(work: &Path, version: &str) -> PathBuf {
    work.join("builds")
        .join(format!("{}-{version}", Product::Hub.command_name()))
        .join(exe("dereth-network-hub"))
}

fn empyrean_path(work: &Path, version: &str, target: Option<&str>) -> PathBuf {
    let name = Product::Empyrean.command_name();
    match target {
        None => work
            .join("builds")
            .join(format!("{name}-{version}"))
            .join(exe("empyrean-server")),
        Some(t) => work
            .join("builds")
            .join(format!("{name}-{version}-{t}"))
            .join("empyrean-server"),
    }
}

fn mod_path(work: &Path, version: &str, world: &str) -> PathBuf {
    work.join("builds")
        .join(format!("{}-{version}-{world}", Product::Ace.command_name()))
        .join("Dereth.Network")
}

/// Where the builds `wanted` would be, nothing built (a dry run).
pub fn planned(work: &Path, wanted: &Wanted) -> Builds {
    let mut out = Builds::default();
    let build = |version: &String, path| Build {
        version: version.clone(),
        path,
    };
    if let Some(v) = wanted.versions.get(&Product::Hub) {
        out.hub = Some(build(v, hub_path(work, v)));
    }
    if let Some(v) = wanted.versions.get(&Product::Empyrean) {
        for t in &wanted.targets {
            out.empyrean
                .insert(t.clone(), build(v, empyrean_path(work, v, t.as_deref())));
        }
    }
    if let Some(v) = wanted.versions.get(&Product::Ace) {
        for world in wanted.ace.keys() {
            out.mods
                .insert(world.clone(), build(v, mod_path(work, v, world)));
        }
    }
    out
}

/// The builds: a temporary worktree of `commit`, each product's version set to its build's, each
/// built for what `wanted` names, its results copied under the work folder's `builds`, and the
/// worktree removed.
pub fn build(
    m: &mut dyn Machine,
    root: &Path,
    work: &Path,
    commit: &str,
    wanted: &Wanted,
) -> Result<Builds, String> {
    let src = work.join("src");
    let target = work.join("target");
    // A worktree a run left behind goes first.
    let _ = m.run(
        Some(root),
        "git",
        &[
            s("worktree"),
            s("remove"),
            s("--force"),
            src.display().to_string(),
        ],
        &[],
    );
    m.remove(&src)?;
    m.run(
        Some(root),
        "git",
        &[
            s("worktree"),
            s("add"),
            s("--detach"),
            src.display().to_string(),
            s(commit),
        ],
        &[],
    )?;
    let built = build_in(m, &src, &target, work, wanted);
    if let Err(e) = built {
        // Kept to look into; the next run removes it first.
        return Err(format!(
            "{e}\nThe worktree it was built in is kept at {}; the next preflight removes it.",
            src.display()
        ));
    }
    let removed = m.run(
        Some(root),
        "git",
        &[
            s("worktree"),
            s("remove"),
            s("--force"),
            src.display().to_string(),
        ],
        &[],
    );
    let built = built?;
    removed?;
    Ok(built)
}

/// The version of the packages `manifests` set to `version`.
fn stamp(m: &mut dyn Machine, manifests: &[PathBuf], version: &str) -> Result<(), String> {
    for manifest in manifests {
        let text = m.read(manifest)?;
        let edited = crate::package::version::set_package_version(&text, version)
            .map_err(|e| format!("{}: {e}", manifest.display()))?;
        m.write(manifest, &edited)?;
    }
    Ok(())
}

fn build_in(
    m: &mut dyn Machine,
    src: &Path,
    target: &Path,
    work: &Path,
    wanted: &Wanted,
) -> Result<Builds, String> {
    let paths = planned(work, wanted);
    let mut out = Builds::default();
    let env = [(s("CARGO_TARGET_DIR"), target.display().to_string())];
    if let Some(b) = paths.hub {
        stamp(m, &[src.join("network/hub/Cargo.toml")], &b.version)?;
        m.run(
            Some(src),
            "cargo",
            &args(&["build", "--release", "-p", "dereth-network-hub"]),
            &env,
        )?;
        m.copy(
            &target.join("release").join(exe("dereth-network-hub")),
            &b.path,
        )?;
        out.hub = Some(b);
    }
    if let Some(v) = wanted.versions.get(&Product::Empyrean) {
        let manifests = empyrean_manifests(m, src)?;
        stamp(m, &manifests, v)?;
    }
    for (t, b) in paths.empyrean {
        let built = match &t {
            None => {
                m.run(
                    Some(src),
                    "cargo",
                    &args(&["build", "--release", "-p", "empyrean-server"]),
                    &env,
                )?;
                target.join("release").join(exe("empyrean-server"))
            }
            Some(t) => {
                // A Linux machine's server, built here: nothing is built on a world's machine.
                m.run(
                    Some(src),
                    "cargo",
                    &args(&[
                        "zigbuild",
                        "--release",
                        "-p",
                        "empyrean-server",
                        "--target",
                        t,
                    ]),
                    &env,
                )?;
                target
                    .join(rust_target(t))
                    .join("release")
                    .join("empyrean-server")
            }
        };
        m.copy(&built, &b.path)?;
        out.empyrean.insert(t, b);
    }
    if let Some(v) = wanted
        .versions
        .get(&Product::Ace)
        .filter(|_| !paths.mods.is_empty())
    {
        let meta = src.join(crate::release::network::ACE_META);
        let project = src.join(crate::release::network::ACE_PROJECT);
        let (meta_text, project_text) =
            crate::release::network::set_ace_version(&m.read(&meta)?, &m.read(&project)?, v)?;
        m.write(&meta, &meta_text)?;
        m.write(&project, &project_text)?;
        for (world, b) in paths.mods {
            let ace = &wanted.ace[&world];
            // The mod is built against this server: without it, the build has nothing to go on.
            if !m.exists(&ace.join("ACE.Server.dll")) {
                return Err(format!(
                    "{world}: there is no ACE.Server.dll in {}: the world's `dir` is the folder \
                     that holds its server's ACE.Server.dll",
                    ace.display()
                ));
            }
            m.run(
                Some(src),
                "dotnet",
                &[
                    s("build"),
                    project.display().to_string(),
                    s("-c"),
                    s("Release"),
                    format!("-p:AceDirectory={}", ace.display()),
                    s("-o"),
                    b.path.display().to_string(),
                ],
                &[],
            )?;
            out.mods.insert(world, b);
        }
    }
    Ok(out)
}

/// Empyrean's manifests in the tree at `src`, as the release command finds them: the workspace's
/// members under `empyrean/` that carry a version of their own.
fn empyrean_manifests(m: &mut dyn Machine, src: &Path) -> Result<Vec<PathBuf>, String> {
    let root: toml::Table = toml::from_str(&m.read(&src.join("Cargo.toml"))?)
        .map_err(|e| format!("the workspace's Cargo.toml: {e}"))?;
    let members: Vec<String> = root
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or("the workspace's Cargo.toml has no members")?
        .iter()
        .filter_map(toml::Value::as_str)
        .filter(|m| m.starts_with("empyrean/"))
        .map(str::to_owned)
        .collect();
    let mut out = Vec::new();
    for member in members {
        let p = src.join(member).join("Cargo.toml");
        if crate::package::version::package_version(&m.read(&p)?).is_some() {
            out.push(p);
        }
    }
    Ok(out)
}

/// The row counts `table-counts` printed, by table.
pub fn parse_counts(text: &str) -> BTreeMap<String, i64> {
    text.lines()
        .filter_map(|l| {
            let (t, n) = l.trim().rsplit_once(' ')?;
            Some((t.to_owned(), n.parse().ok()?))
        })
        .collect()
}

/// The dev hub: the real registry backed up on its machine and copied down, counted, migrated and
/// defanged by the candidate `hub`, counted again, and served, with the tunnel that carries its
/// address. A dev hub a run left running is stopped first.
pub fn dev_hub(
    m: &mut dyn Machine,
    cfg: &Config,
    hub: &Path,
    ledger: &mut Ledger,
) -> Result<(), String> {
    stop_dev_hub(m, ledger);
    let h = &cfg.hub;
    let dir = cfg.work_dir.join("hub");
    let real = dir.join("real.sqlite");
    let dev = dir.join("dev.sqlite");
    m.ssh(
        &h.ssh,
        &format!(
            "rm -f {tmp} && {bin} --db {db} backup {tmp}",
            tmp = h.remote_tmp,
            bin = h.remote_binary,
            db = h.remote_db
        ),
    )?;
    m.remove(&real)?;
    m.fetch(&h.ssh, &h.remote_tmp, &real)?;
    m.ssh(&h.ssh, &format!("rm -f {}", h.remote_tmp))?;
    for f in ["dev.sqlite", "dev.sqlite-wal", "dev.sqlite-shm"] {
        m.remove(&dir.join(f))?;
    }
    m.copy(&real, &dev)?;
    let program = hub.display().to_string();
    let db = || vec![s("--db"), dev.display().to_string()];
    let before = parse_counts(&m.run(
        None,
        &program,
        &[db(), vec![s("table-counts")]].concat(),
        &[],
    )?);
    let mut defang = [db(), vec![s("defang-copy"), s("--this-is-a-copy")]].concat();
    for id in &h.admins {
        defang.extend([s("--keep"), id.clone()]);
    }
    let said = m.run(None, &program, &defang, &[])?;
    println!("{}", said.trim());
    let after = parse_counts(&m.run(
        None,
        &program,
        &[db(), vec![s("table-counts")]].concat(),
        &[],
    )?);
    let tables: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    ledger.counts = tables
        .into_iter()
        .map(|t| (t.clone(), before.get(t).copied(), after.get(t).copied()))
        .collect();

    let mut args = db();
    args.extend([s("--public-url"), h.public_url.clone()]);
    for o in &h.site_origins {
        args.extend([s("--site-origin"), o.clone()]);
    }
    args.extend([s("serve"), s("--listen"), h.listen.clone()]);
    for id in &h.admins {
        args.extend([s("--admin"), id.clone()]);
    }
    if let Some(id) = &h.discord_client_id {
        args.extend([s("--discord-client-id"), id.clone()]);
    }
    if let Some(f) = &h.discord_client_secret_file {
        args.extend([s("--discord-client-secret-file"), f.display().to_string()]);
    }
    if let Some(f) = &h.discord_bot_token_file {
        args.extend([s("--discord-bot-token-file"), f.display().to_string()]);
    }
    args.extend(h.serve_args.iter().cloned());
    let pid = m.spawn(&program, &args, &[], &dir.join("hub.log"))?;
    let tunnel = match &h.tunnel {
        Some(name) => Some(m.spawn(
            &h.cloudflared,
            &[
                s("tunnel"),
                s("--url"),
                format!("http://{}", h.listen),
                s("run"),
                name.clone(),
            ],
            &[],
            &dir.join("tunnel.log"),
        )?),
        None => None,
    };
    ledger.hub = Some(DevHub { pid, tunnel });
    Ok(())
}

/// The dev hub and its tunnel stopped, if the ledger has them running.
pub fn stop_dev_hub(m: &mut dyn Machine, ledger: &mut Ledger) {
    if let Some(h) = ledger.hub.take() {
        if let Some(t) = h.tunnel {
            let _ = m.kill(t);
        }
        let _ = m.kill(h.pid);
    }
}

/// The paths of Empyrean's databases named in its configuration `toml`, relative to its folder.
pub fn empyrean_databases(toml_text: &str) -> Vec<String> {
    let Ok(v) = toml_text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let db = v.get("database").and_then(toml::Value::as_table);
    ["shard_db_path", "auth_db_path"]
        .iter()
        .map(|k| {
            db.and_then(|t| t.get(*k))
                .and_then(toml::Value::as_str)
                .map_or_else(|| default_database(k), str::to_owned)
        })
        .collect()
}

fn default_database(key: &str) -> String {
    if key == "shard_db_path" {
        "shard.db"
    } else {
        "auth.db"
    }
    .to_owned()
}

/// A world's database path resolved against its folder, as its machine spells paths (with `/` on
/// Linux).
fn in_dir(dir: &str, file: &str, linux: bool) -> String {
    let p = file.trim_start_matches("./").trim_start_matches(".\\");
    if p.starts_with('/') || p.contains(':') {
        return p.to_owned();
    }
    let sep = if linux { "/" } else { "\\" };
    format!("{}{sep}{p}", dir.trim_end_matches(['/', '\\']))
}

/// `w`'s configuration `text` pointed at the hub `link`: the text, and the address it had.
fn to_link(w: &World, text: &str, link: &str) -> Result<(String, Option<String>), String> {
    match w.kind {
        Kind::Empyrean => edit::empyrean_hub(text, link),
        Kind::Ace => edit::ace_hub(text, link),
    }
}

/// `w`'s configuration `text` pointed back at the hub it named, `old`.
fn to_original(w: &World, text: &str, old: Option<&str>) -> Result<String, String> {
    match w.kind {
        Kind::Empyrean => edit::empyrean_hub_back(text, old),
        Kind::Ace => edit::ace_hub_back(text, old),
    }
}

/// The database files of `w`, an Empyrean world whose configuration is `text`, as its machine
/// spells them.
fn databases(w: &World, text: &str, linux: bool) -> Vec<String> {
    match w.kind {
        Kind::Empyrean => empyrean_databases(text)
            .iter()
            .map(|db| in_dir(&w.dir, db, linux))
            .collect(),
        Kind::Ace => Vec::new(),
    }
}

/// `w` recorded with its backup, which is whole: from now on a rollback puts it back, even when
/// the run stops before the world is started again.
fn record(
    m: &mut dyn Machine,
    cfg: &Config,
    ledger: &mut Ledger,
    installed: Installed,
) -> Result<(), String> {
    ledger.worlds.push(installed);
    ledger.save(m, &cfg.work_dir)
}

/// Where the configuration of `w`, a world on another Windows machine, is copied down to here.
fn copied_config(cfg: &Config, w: &World) -> PathBuf {
    let file = match w.kind {
        Kind::Empyrean => "empyrean.toml",
        Kind::Ace => "Settings.json",
    };
    cfg.work_dir.join("remote").join(&w.name).join(file)
}

/// One dev world given `build` (when its product changed) and pointed at the dev hub: stopped,
/// backed up the first time (its server or mod, its configuration and Empyrean's databases) and
/// recorded, the build put in place, its hub address changed (the original kept), and started.
pub fn install(
    m: &mut dyn Machine,
    cfg: &Config,
    w: &World,
    build: Option<&Build>,
    ledger: &mut Ledger,
) -> Result<(), String> {
    let link = cfg.dev_hub_link();
    let first = ledger.world(&w.name).is_none();
    match w.place() {
        Place::Here => install_here(m, cfg, w, build, &link, first, ledger)?,
        Place::Windows(h) => install_windows(m, cfg, w, h, build, &link, first, ledger)?,
        Place::Linux(h) => install_linux(m, cfg, w, h, build, &link, first, ledger)?,
    }
    if let Some(x) = ledger.worlds.iter_mut().find(|x| x.name == w.name) {
        x.version = build.map(|b| b.version.clone()).or(x.version.take());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // one world, and the run's facts
fn install_here(
    m: &mut dyn Machine,
    cfg: &Config,
    w: &World,
    build: Option<&Build>,
    link: &str,
    first: bool,
    ledger: &mut Ledger,
) -> Result<(), String> {
    let backup = cfg.work_dir.join("backups").join(&w.name);
    let config = PathBuf::from(w.config_file());
    let replaced = PathBuf::from(w.replaced());
    m.service(&w.service, Service::Stopped)?;
    let text = m.read(&config)?;
    let (edited, old) = to_link(w, &text, link)?;
    if first {
        m.remove(&backup)?;
        m.copy(&replaced, &backup.join("replaced"))?;
        m.copy(&config, &backup.join("config"))?;
        for db in databases(w, &text, false) {
            for suffix in ["", "-wal", "-shm"] {
                let file = PathBuf::from(format!("{db}{suffix}"));
                if m.exists(&file) {
                    let name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    m.copy(&file, &backup.join("db").join(name))?;
                }
            }
        }
        record(
            m,
            cfg,
            ledger,
            Installed {
                name: w.name.clone(),
                original_hub: old,
                backup: backup.display().to_string(),
                version: None,
                databases: Vec::new(),
            },
        )?;
    }
    if let Some(b) = build {
        m.copy(&b.path, &replaced)?;
    }
    m.write(&config, &edited)?;
    m.service(&w.service, Service::Running)
}

#[allow(clippy::too_many_arguments)] // one world, its machine, and the run's facts
fn install_windows(
    m: &mut dyn Machine,
    cfg: &Config,
    w: &World,
    host: &Host,
    build: Option<&Build>,
    link: &str,
    first: bool,
    ledger: &mut Ledger,
) -> Result<(), String> {
    let local = copied_config(cfg, w);
    let text = windows_config(m, w, host, &local)?;
    let (edited, old) = to_link(w, &text, link)?;
    if first {
        let dbs = databases(w, &text, false);
        m.ssh(&host.ssh, &powershell(&windows::backup(w, &dbs)))?;
        record(
            m,
            cfg,
            ledger,
            Installed {
                name: w.name.clone(),
                original_hub: old,
                backup: windows::backup_dir(w),
                version: None,
                databases: dbs,
            },
        )?;
    }
    windows_put(m, w, host, build, &local, &edited)
}

/// `w`, a world on another Windows machine, stopped, and its configuration copied down to `local`
/// and read.
fn windows_config(
    m: &mut dyn Machine,
    w: &World,
    host: &Host,
    local: &Path,
) -> Result<String, String> {
    m.ssh(&host.ssh, &powershell(&windows::stage_config(w)))?;
    m.remove(local)?;
    m.fetch(&host.ssh, &windows::staged(w, "config"), local)?;
    m.read(local)
}

/// `build` (when there is one) and the configuration `text` copied up to `w`'s machine, put in
/// place, and `w` started.
fn windows_put(
    m: &mut dyn Machine,
    w: &World,
    host: &Host,
    build: Option<&Build>,
    local: &Path,
    text: &str,
) -> Result<(), String> {
    if let Some(b) = build {
        m.send(&host.ssh, &b.path, &windows::staged(w, "build"))?;
    }
    m.write(local, text)?;
    m.send(&host.ssh, local, &windows::staged(w, "config"))?;
    m.ssh(&host.ssh, &powershell(&windows::place(w, build.is_some())))
        .map(|_| ())
}

#[allow(clippy::too_many_arguments)] // one world, its machine, and the run's facts
fn install_linux(
    m: &mut dyn Machine,
    cfg: &Config,
    w: &World,
    host: &Host,
    build: Option<&Build>,
    link: &str,
    first: bool,
    ledger: &mut Ledger,
) -> Result<(), String> {
    m.ssh(&host.ssh, &format!("sudo -n systemctl stop {}", w.service))?;
    let local = cfg.work_dir.join("remote").join(format!("{}.toml", w.name));
    m.fetch(&host.ssh, &w.config_file(), &local)?;
    let text = m.read(&local)?;
    let (edited, old) = edit::empyrean_hub(&text, link)?;
    if first {
        let dbs = databases(w, &text, true);
        m.ssh(&host.ssh, &linux::backup(w, &dbs))?;
        record(
            m,
            cfg,
            ledger,
            Installed {
                name: w.name.clone(),
                original_hub: old,
                backup: linux::backup_dir(w),
                version: None,
                databases: dbs,
            },
        )?;
    }
    linux_put(m, w, host, build, &local, &edited)
}

/// `build` (when there is one) copied up beside `w`'s server and copied over it, which keeps the
/// server's own mode; the configuration `text` copied up; and `w` started.
fn linux_put(
    m: &mut dyn Machine,
    w: &World,
    host: &Host,
    build: Option<&Build>,
    local: &Path,
    text: &str,
) -> Result<(), String> {
    if let Some(b) = build {
        m.send(&host.ssh, &b.path, &linux::incoming(w))?;
        m.ssh(&host.ssh, &linux::place(w))?;
    }
    m.write(local, text)?;
    m.send(&host.ssh, local, &w.config_file())?;
    m.ssh(&host.ssh, &format!("sudo -n systemctl start {}", w.service))
        .map(|_| ())
}

/// Every world the ledger installed put back as it was, from its backup, in the reverse order; the
/// dev hub and its tunnel stopped.
pub fn rollback(m: &mut dyn Machine, cfg: &Config, ledger: &mut Ledger) -> Result<(), String> {
    let mut problems = Vec::new();
    for done in ledger.worlds.clone().iter().rev() {
        let Some(w) = cfg.worlds.iter().find(|w| w.name == done.name) else {
            problems.push(format!(
                "{}: no longer in the setup file; its backup is {}",
                done.name, done.backup
            ));
            continue;
        };
        let restored = match w.place() {
            Place::Here => restore_here(m, w, Path::new(&done.backup)),
            Place::Windows(h) => m
                .ssh(&h.ssh, &powershell(&windows::restore(w, &done.databases)))
                .map(|_| ()),
            Place::Linux(h) => restore_linux(m, w, &h.ssh, &done.databases),
        };
        match restored {
            Ok(()) => ledger.worlds.retain(|x| x.name != done.name),
            Err(e) => problems.push(format!("{}: {e}", done.name)),
        }
    }
    stop_dev_hub(m, ledger);
    if problems.is_empty() {
        ledger.stage = Stage::Idle;
        ledger.commit = None;
        ledger.branch = None;
        ledger.releases.clear();
        Ok(())
    } else {
        Err(format!(
            "not every world was put back:\n  {}",
            problems.join("\n  ")
        ))
    }
}

fn restore_here(m: &mut dyn Machine, w: &World, backup: &Path) -> Result<(), String> {
    m.service(&w.service, Service::Stopped)?;
    let replaced = PathBuf::from(w.replaced());
    if w.kind == Kind::Ace {
        // The mod's folder as it was, nothing the build added left in it.
        m.remove(&replaced)?;
    }
    m.copy(&backup.join("replaced"), &replaced)?;
    m.copy(&backup.join("config"), Path::new(&w.config_file()))?;
    let dbs = backup.join("db");
    if w.kind == Kind::Empyrean && m.exists(&dbs) {
        let text = m.read(Path::new(&w.config_file()))?;
        for db in databases(w, &text, false) {
            for suffix in ["", "-wal", "-shm"] {
                let file = PathBuf::from(format!("{db}{suffix}"));
                let name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let kept = dbs.join(&name);
                if m.exists(&kept) {
                    m.copy(&kept, &file)?;
                } else {
                    // A file the world made since (a write-ahead log) goes with the database.
                    m.remove(&file)?;
                }
            }
        }
    }
    m.service(&w.service, Service::Running)
}

/// `w`, a world on a Linux machine, stopped, put back from its backup there, started, and the
/// backup removed once it runs.
fn restore_linux(m: &mut dyn Machine, w: &World, host: &str, dbs: &[String]) -> Result<(), String> {
    m.ssh(host, &format!("sudo -n systemctl stop {}", w.service))?;
    m.ssh(host, &linux::restore(w, dbs))?;
    m.ssh(host, &format!("sudo -n systemctl start {}", w.service))?;
    m.ssh(host, &linux::drop_backup(w))?;
    Ok(())
}

/// After `ship`: each world given its final build (`builds`, made here from `main`) when it took a
/// candidate, a world on another machine taking its copy over scp, pointed back at the hub it named
/// before, and started; the dev hub and its tunnel stopped. The backups stay where they are.
pub fn finish(
    m: &mut dyn Machine,
    cfg: &Config,
    builds: &Builds,
    ledger: &mut Ledger,
) -> Result<(), String> {
    for done in ledger.worlds.clone() {
        let Some(w) = cfg.worlds.iter().find(|w| w.name == done.name) else {
            return Err(format!("{}: no longer in the setup file", done.name));
        };
        let build = builds.for_world(w).filter(|_| done.version.is_some());
        let old = done.original_hub.as_deref();
        match w.place() {
            Place::Here => {
                let config = PathBuf::from(w.config_file());
                m.service(&w.service, Service::Stopped)?;
                if let Some(b) = build {
                    m.copy(&b.path, Path::new(&w.replaced()))?;
                }
                let text = m.read(&config)?;
                m.write(&config, &to_original(w, &text, old)?)?;
                m.service(&w.service, Service::Running)?;
            }
            Place::Windows(h) => {
                let local = copied_config(cfg, w);
                let text = windows_config(m, w, h, &local)?;
                windows_put(m, w, h, build, &local, &to_original(w, &text, old)?)?;
            }
            Place::Linux(h) => {
                m.ssh(&h.ssh, &format!("sudo -n systemctl stop {}", w.service))?;
                let local = cfg.work_dir.join("remote").join(format!("{}.toml", w.name));
                m.fetch(&h.ssh, &w.config_file(), &local)?;
                let text = m.read(&local)?;
                linux_put(m, w, h, build, &local, &to_original(w, &text, old)?)?;
            }
        }
        ledger.worlds.retain(|x| x.name != done.name);
        ledger.save(m, &cfg.work_dir)?;
    }
    stop_dev_hub(m, ledger);
    ledger.stage = Stage::Idle;
    ledger.commit = None;
    ledger.branch = None;
    ledger.releases.clear();
    Ok(())
}

/// What the run left, for the person who ran it: what is installed where, what the copy of the
/// registry held before and after, and how to undo it.
pub fn summary(cfg: &Config, ledger: &Ledger) -> String {
    let mut out = String::new();
    if let Some(c) = &ledger.commit {
        let branch = ledger.branch.as_deref().unwrap_or("the branch");
        out.push_str(&format!("Preflighted {branch} at {c}.\n"));
    }
    for r in &ledger.releases {
        out.push_str(&format!(
            "  {} {} -> {} (built as {}): {}\n",
            r.product.command_name(),
            r.from,
            r.version,
            r.candidate,
            r.why
        ));
    }
    if let Some(h) = &ledger.hub {
        out.push_str(&format!(
            "The dev hub runs on a defanged copy of the real registry at {} (process {}{}), as {}.\n",
            cfg.hub.listen,
            h.pid,
            h.tunnel.map(|t| format!(", tunnel {t}")).unwrap_or_default(),
            cfg.hub.public_url
        ));
    }
    if !ledger.counts.is_empty() {
        out.push_str(
            "The copy's tables, rows before the candidate migrated it and after it was defanged:\n",
        );
        for (t, b, a) in &ledger.counts {
            let n = |x: &Option<i64>| x.map_or_else(|| "-".to_owned(), |v| v.to_string());
            out.push_str(&format!("  {t:<24} {:>8} {:>8}\n", n(b), n(a)));
        }
    }
    for w in &ledger.worlds {
        let on = cfg
            .worlds
            .iter()
            .find(|x| x.name == w.name)
            .and_then(|x| x.host.as_ref())
            .map(|h| format!(" on {}", h.name))
            .unwrap_or_default();
        out.push_str(&format!(
            "  {}: {}, linked to the dev hub (it named {}); its files as they were are in {}{on}\n",
            w.name,
            w.version
                .as_deref()
                .map_or_else(|| "its own build".to_owned(), |v| format!("runs {v}")),
            w.original_hub.as_deref().unwrap_or("the network's hub"),
            w.backup
        ));
    }
    out.push_str(
        "Undo all of it with `cargo xtask preflight rollback`; release it with `cargo xtask ship`, \
         then `cargo xtask preflight finish`.\n",
    );
    out
}
