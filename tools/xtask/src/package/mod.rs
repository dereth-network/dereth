//! `cargo xtask package empyrean`: Empyrean's release archives, built the same way on a
//! contributor's machine and in the release workflow.
//!
//! For each target it builds `empyrean-server` and `empyrean-import` in the `server-release`
//! profile, stages an allowlist of files, checks every file and every binary, and writes one
//! archive; then it writes the release's index beside the archives: `MANIFEST.txt`,
//! `release.json` and `SHA256SUMS`. The source is the tagged commit, whose archives GitHub
//! attaches to the release itself; `NOTICE.txt` names both. `release.json` carries the release's
//! upgrade declaration ([`declaration`]), and a release whose declaration breaks its rules (a
//! patch release that declares anything) is refused before anything is built.
//!
//! ```text
//! cargo xtask package empyrean [--target <triple>]... [--out <dir>] [--allow-dirty]
//! cargo xtask package empyrean --gather <dir>
//! ```
//!
//! Without `--target` it packages the host's own target. `--gather` builds nothing: it checks the
//! archives already in `<dir>` (the release workflow downloads each runner's there) and writes the
//! index over them. The archives land in `<target dir>/package/empyrean-<version>/` unless `--out`
//! names another folder; the staging folders are under `<target dir>/package/stage/`.
//!
//! **The dat guard.** Only the allowlist is ever copied, from named paths, and the deny scan
//! ([`guard`]) runs over the staging folder and again over the written archive: a game data file,
//! anything named like one, a world pack, a database, a private `empyrean.toml`, a link or an
//! oversized file fails the package whatever its name.
//!
//! **Reproducible where cheap.** The build time the binaries carry is the commit's
//! (`SOURCE_DATE_EPOCH` when set), paths are remapped, `--locked` pins the dependencies, the
//! Windows linker writes no timestamp (`/Brepro`), and the archives carry the commit's time and
//! fixed permissions.

mod archive;
pub mod declaration;
pub mod dereth;
mod guard;
mod headers;
mod notice;
mod sign;
mod targets;
pub mod version;
pub mod web;

#[cfg(test)]
mod dereth_tests;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::util::{target_dir, workspace_root};
use targets::{Builder, Host, Os, Target};

/// The public repository a release's source is offered from, when the build environment names
/// no other (`EMPYREAN_BUILD_SOURCE_URL`).
pub const DEFAULT_SOURCE_URL: &str = "https://github.com/dereth-network/dereth";

/// The two binaries, and the package each is built from.
const BINARIES: &[(&str, &str, headers::Role)] = &[
    (
        "empyrean-server",
        "empyrean/server/Cargo.toml",
        headers::Role::Server,
    ),
    (
        "empyrean-import",
        "empyrean/import/Cargo.toml",
        headers::Role::Import,
    ),
];

/// The documents a package carries, as (name in the package, path in the tree).
const DOCUMENTS: &[(&str, &str)] = &[
    ("README.md", "empyrean/README.md"),
    ("SETUP.md", "empyrean/SETUP.md"),
    ("DIVERGENCES.md", "empyrean/DIVERGENCES.md"),
    ("ACE-BUGS.md", "empyrean/ACE-BUGS.md"),
    ("LICENSE", "empyrean/LICENSE"),
    (
        "empyrean.toml.example",
        "empyrean/server/empyrean.toml.example",
    ),
];

/// The two files generated for each package.
const GENERATED: &[&str] = &["NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// cargo-about's configuration and template.
const ABOUT_CONFIG: &str = "empyrean/about.toml";
const ABOUT_TEMPLATE: &str = "empyrean/about.hbs";

/// The Lifestoned data model's MIT notice.
const LIFESTONED_NOTICE: &str = "empyrean/licenses/Lifestoned-MIT.txt";

/// Every file a package for `target` holds, and nothing else.
pub fn allowlist(target: Target) -> Vec<String> {
    let mut names: Vec<String> = BINARIES
        .iter()
        .map(|(b, _, _)| format!("{b}{}", target.exe_suffix()))
        .collect();
    names.extend(DOCUMENTS.iter().map(|(n, _)| (*n).to_owned()));
    names.extend(GENERATED.iter().map(|n| (*n).to_owned()));
    names.sort();
    names
}

/// What the build stamps into the binaries, and what the index records.
#[derive(Debug, Clone)]
pub struct BuildFacts {
    pub commit: String,
    pub branch: String,
    pub number: String,
    /// Seconds since 1970: `SOURCE_DATE_EPOCH`, else the commit's time.
    pub epoch: i64,
    pub source_url: String,
}

impl BuildFacts {
    /// `yyyyMMddHHmmss`, the form `EMPYREAN_BUILD_UTC` takes.
    pub fn utc_stamp(&self) -> String {
        let (y, mo, d, h, mi, s) = archive::civil(self.epoch);
        format!("{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}")
    }

    /// ISO 8601, UTC.
    pub fn iso_time(&self) -> String {
        let (y, mo, d, h, mi, s) = archive::civil(self.epoch);
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    }
}

/// Run `git` in the workspace; its trimmed output, or why it failed.
pub fn git(ws: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(ws)
        .output()
        .map_err(|e| format!("git could not be run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The tracked files with uncommitted changes (`git status`), untracked files not counted.
pub fn uncommitted(ws: &Path) -> Result<Vec<String>, String> {
    Ok(git(ws, &["status", "--porcelain", "--untracked-files=no"])?
        .lines()
        .map(str::to_owned)
        .collect())
}

/// The facts of the checked-out commit. A shallow clone is refused (its build number would be
/// wrong), and so are uncommitted changes unless `allow_dirty`.
pub fn build_facts(ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    if git(ws, &["rev-parse", "--is-shallow-repository"])? == "true" {
        return Err(
            "this is a shallow clone, so the build number (the commit count) would be wrong: \
             fetch the whole history (`git fetch --unshallow`, or `fetch-depth: 0` in a workflow)"
                .to_owned(),
        );
    }
    let dirty = uncommitted(ws)?;
    if !dirty.is_empty() && !allow_dirty {
        return Err(format!(
            "the tree has uncommitted changes, so the binaries would not be the commit they name \
             ({}); commit them, or pass --allow-dirty for a local trial",
            dirty.join("; ")
        ));
    }
    let commit = git(ws, &["rev-parse", "HEAD"])?;
    let number = git(ws, &["rev-list", "--count", "HEAD"])?;
    let epoch = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(v) => v
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("SOURCE_DATE_EPOCH is not a number of seconds: `{v}`"))?,
        Err(_) => git(ws, &["show", "-s", "--format=%ct", "HEAD"])?
            .parse::<i64>()
            .map_err(|_| "the commit's time is not a number".to_owned())?,
    };
    let branch = match std::env::var("EMPYREAN_BUILD_BRANCH") {
        Ok(b) if !b.trim().is_empty() => b.trim().to_owned(),
        _ => {
            let tags = git(ws, &["tag", "--points-at", "HEAD", "--list", "empyrean-v*"])?;
            match tags.lines().next() {
                Some(tag) => tag.to_owned(),
                None => git(ws, &["rev-parse", "--abbrev-ref", "HEAD"])?,
            }
        }
    };
    let source_url = match std::env::var("EMPYREAN_BUILD_SOURCE_URL") {
        Ok(u) if !u.trim().is_empty() => u.trim().trim_end_matches('/').to_owned(),
        _ => DEFAULT_SOURCE_URL.to_owned(),
    };
    Ok(BuildFacts {
        commit,
        branch,
        number,
        epoch,
        source_url,
    })
}

/// The environment a release build runs with: the build stamp, and the flags that make it
/// reproducible and self-contained.
///
/// `remaps` are the local folders whose paths would otherwise be compiled into panic messages
/// (the checkout, cargo's crate cache, the toolchain's own library source), each with the
/// neutral name it is given instead.
pub fn build_env(
    facts: &BuildFacts,
    target: Target,
    remaps: &[(PathBuf, &str)],
) -> Vec<(String, String)> {
    let mut flags: Vec<String> = remaps
        .iter()
        .map(|(from, to)| format!("--remap-path-prefix={}={to}", from.display()))
        .collect();
    if target.os == Os::Windows {
        // The C runtime inside the binaries, so they need no Visual C++ redistributable, and no
        // link time in the header.
        flags.push("-Ctarget-feature=+crt-static".to_owned());
        flags.push("-Clink-arg=/Brepro".to_owned());
    }
    let mut env = vec![
        ("CARGO_ENCODED_RUSTFLAGS".to_owned(), flags.join("\u{1f}")),
        ("SOURCE_DATE_EPOCH".to_owned(), facts.epoch.to_string()),
        ("EMPYREAN_BUILD_COMMIT".to_owned(), facts.commit.clone()),
        ("EMPYREAN_BUILD_BRANCH".to_owned(), facts.branch.clone()),
        ("EMPYREAN_BUILD_NUMBER".to_owned(), facts.number.clone()),
        ("EMPYREAN_BUILD_UTC".to_owned(), facts.utc_stamp()),
        (
            "EMPYREAN_BUILD_SOURCE_URL".to_owned(),
            facts.source_url.clone(),
        ),
        ("EMPYREAN_BUILD_TARGET".to_owned(), target.triple.to_owned()),
    ];
    if target.os == Os::Mac {
        env.push((
            "MACOSX_DEPLOYMENT_TARGET".to_owned(),
            format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1),
        ));
    }
    env
}

fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// What this host can build with.
fn host() -> Host {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect()
        });
    Host {
        os: Os::host(),
        zigbuild: succeeds("cargo", &["zigbuild", "--help"]),
        zig: succeeds("zig", &["version"])
            || succeeds("python3", &["-m", "ziglang", "version"])
            || succeeds("python", &["-m", "ziglang", "version"]),
        installed,
    }
}

/// The host's own target triple (`rustc -vV`).
fn host_triple() -> Result<String, String> {
    let out = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|e| format!("rustc could not be run: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(|h| h.trim().to_owned())
        .ok_or_else(|| "rustc -vV named no host".to_owned())
}

/// The local folders a build would otherwise name in the binaries: the checkout, the target
/// folder (where build scripts write the source they generate, and which can be anywhere),
/// cargo's crate cache (`CARGO_HOME`, else `~/.cargo`) and the toolchain (`rustc --print
/// sysroot`), whose library source the compiler names when it is installed.
///
/// The compiler applies the last remap that matches, so the target folder, which is often inside
/// the checkout, comes after it.
fn local_remaps(ws: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut remaps = vec![(ws.to_path_buf(), "/dereth")];
    let target = target_dir();
    if target != ws {
        remaps.push((target, "/target"));
    }
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .ok()
                .map(|h| PathBuf::from(h).join(".cargo"))
        });
    if let Some(home) = cargo_home {
        remaps.push((home, "/cargo"));
    }
    if let Some(sysroot) = Command::new("rustc")
        .args(["--print", "sysroot"])
        .current_dir(ws)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
    {
        remaps.push((PathBuf::from(sysroot), "/rustc/sysroot"));
    }
    remaps
}

/// The options of one `package` run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
}

const USAGE: &str =
    "usage: cargo xtask package empyrean [--target <triple>]... [--out <dir>] [--allow-dirty]
       cargo xtask package empyrean --gather <dir>";

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut it = args.iter();
    match it.next().map(String::as_str) {
        Some("empyrean") => {}
        _ => return Err(format!("{USAGE}\n{}\n{}", dereth::USAGE, web::USAGE)),
    }
    let mut o = Options::default();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--target" => o.targets.push(value()?),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--gather" => o.gather = Some(PathBuf::from(value()?)),
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    if o.gather.is_some() && (!o.targets.is_empty() || o.out.is_some()) {
        return Err(format!(
            "--gather builds nothing: it takes no --target or --out\n{USAGE}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package ...`.
pub fn package(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("dereth") {
        return dereth::package(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("web") {
        return web::package(&args[1..]);
    }
    let options = match parse_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = workspace_root();
    let result = match &options.gather {
        Some(dir) => gather(&ws, dir),
        None => {
            let targets = if options.targets.is_empty() {
                match host_triple() {
                    Ok(h) => vec![h],
                    Err(e) => {
                        eprintln!("package: {e}");
                        return 1;
                    }
                }
            } else {
                options.targets.clone()
            };
            build_packages(&ws, &targets, options.out.as_deref(), options.allow_dirty)
        }
    };
    match result {
        Ok(dir) => {
            println!(
                "\npackage: done; the release files are in {}",
                dir.display()
            );
            0
        }
        Err(e) => {
            eprintln!("\npackage: FAILED\n{e}");
            1
        }
    }
}

/// Build, stage, check and archive every target, then write the index. The folder the release
/// files are in.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
) -> Result<PathBuf, String> {
    let version = version::empyrean_version(ws)?;
    // A release whose upgrade declaration is refused is refused before anything is built.
    declaration::for_release(ws, &version)?;
    // Everything a host lacks is said before anything is built.
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    let host = host();
    for triple in triples {
        match targets::find(triple).and_then(|t| targets::plan(t, &host).map(|b| (t, b))) {
            Ok(p) => plans.push(p),
            Err(e) => refusals.push(format!("  {triple}: {e}")),
        }
    }
    if !succeeds("cargo", &["about", "--version"]) {
        refusals.push(
            "  cargo-about is not installed, or does not run on this host; it writes THIRD-PARTY-LICENSES.html: \
             `cargo install --locked cargo-about --features cli`"
                .to_owned(),
        );
    }
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let facts = build_facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("empyrean-{version}")));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    for (target, builder) in plans {
        println!("\n=== empyrean {version} for {}", target.triple);
        package_target(
            ws,
            target,
            builder,
            &facts,
            &version,
            &out_dir,
            &base.join("stage"),
        )?;
    }
    write_index(ws, &out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the archives there and write the index over them.
fn gather(ws: &Path, dir: &Path) -> Result<PathBuf, String> {
    let version = version::empyrean_version(ws)?;
    let facts = build_facts(ws, false)?;
    write_index(ws, dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

fn run_with_env(
    ws: &Path,
    program: &str,
    args: &[&str],
    env: &[(String, String)],
) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(ws)
        .status()
        .map_err(|e| format!("{program} could not be run: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed ({status})", args.join(" ")))
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn package_target(
    ws: &Path,
    target: Target,
    builder: Builder,
    facts: &BuildFacts,
    version: &str,
    out_dir: &Path,
    stage_root: &Path,
) -> Result<(), String> {
    // 1. Build.
    let env = build_env(facts, target, &local_remaps(ws));
    let target_arg = target.build_target_arg();
    let verb = match builder {
        Builder::Cargo => "build",
        Builder::Zigbuild => "zigbuild",
    };
    let mut args = vec![
        verb,
        "--locked",
        "--profile",
        "server-release",
        "--target",
        &target_arg,
    ];
    for (bin, _, _) in BINARIES {
        args.extend_from_slice(&["-p", bin]);
    }
    run_with_env(ws, "cargo", &args, &env)?;
    let bin_dir = target_dir().join(target.triple).join("server-release");

    // 2. Stage the allowlist, each file written anew from a named path.
    let root = target.archive_root(version);
    let stage = stage_root.join(&root);
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    }
    std::fs::create_dir_all(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    for (bin, _, _) in BINARIES {
        let name = format!("{bin}{}", target.exe_suffix());
        write(&stage.join(&name), &read(&bin_dir.join(&name))?)?;
    }
    for (name, from) in DOCUMENTS {
        write(&stage.join(name), &read(&ws.join(from))?)?;
    }

    // 3. The notices.
    let tree = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "-e",
            "normal",
            "--target",
            target.triple,
        ])
        .args(["--prefix", "none", "--format", "{p}|{l}"])
        .args(BINARIES.iter().flat_map(|(b, _, _)| ["-p", b]))
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo tree could not be run: {e}"))?;
    if !tree.status.success() {
        return Err(format!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&tree.stderr)
        ));
    }
    let crates = notice::parse_cargo_tree(&String::from_utf8_lossy(&tree.stdout));
    let mit = String::from_utf8_lossy(&read(&ws.join("LICENSE"))?).into_owned();
    let lifestoned = String::from_utf8_lossy(&read(&ws.join(LIFESTONED_NOTICE))?).into_owned();
    let text = notice::notice(&notice::Facts {
        version,
        target: target.triple,
        commit: &facts.commit,
        source_url: &facts.source_url,
        crates: &crates,
        mit_licence: &mit,
        lifestoned_notice: &lifestoned,
    });
    write(&stage.join("NOTICE.txt"), text.as_bytes())?;
    let mut pages = Vec::new();
    let about_dir = stage_root.join(format!("{root}.about"));
    std::fs::create_dir_all(&about_dir).map_err(|e| format!("{}: {e}", about_dir.display()))?;
    for (bin, manifest, _) in BINARIES {
        let page = about_dir.join(format!("{bin}.html"));
        let page_arg = page.display().to_string();
        run_with_env(
            ws,
            "cargo",
            &[
                "about",
                "generate",
                "--locked",
                "--fail",
                "-m",
                manifest,
                "-c",
                ABOUT_CONFIG,
                "--target",
                target.triple,
                "-o",
                &page_arg,
                ABOUT_TEMPLATE,
            ],
            &[],
        )?;
        let html = String::from_utf8_lossy(&read(&page)?).into_owned();
        pages.push((*bin, html));
    }
    write(
        &stage.join("THIRD-PARTY-LICENSES.html"),
        notice::splice_licence_pages(&pages)?.as_bytes(),
    )?;

    // 4. The deny scan of the staging folder.
    let allow = allowlist(target);
    let staged = guard::staged_entries(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    let findings = guard::scan(&staged, &allow, guard::ARCHIVE_CAP);
    if !findings.is_empty() {
        return Err(format!(
            "the deny scan refused the staging folder {}:\n  {}",
            stage.display(),
            findings.join("\n  ")
        ));
    }

    // 5. The header checks, and no program naming this machine's folders.
    let folders = guard::local_folders(ws);
    let mut described = Vec::new();
    for (bin, _, role) in BINARIES {
        let name = format!("{bin}{}", target.exe_suffix());
        let bytes = read(&stage.join(&name))?;
        let named = guard::local_paths_in(&bytes, &folders);
        if !named.is_empty() {
            return Err(format!(
                "the deny scan refused {name}: it names this machine's folder {}",
                named.join(", ")
            ));
        }
        match headers::check_binary(&bytes, target, *role) {
            Ok(d) => {
                println!("  {name}: {d}");
                described.push((name, d));
            }
            Err(problems) => {
                return Err(format!(
                    "the header check refused {name} for {}:\n  {}",
                    target.triple,
                    problems.join("\n  ")
                ))
            }
        }
    }

    // 6. The archive, and the deny scan of what was written.
    let mut members = Vec::new();
    for name in &allow {
        members.push(archive::Member {
            name: name.clone(),
            bytes: read(&stage.join(name))?,
            executable: BINARIES
                .iter()
                .any(|(b, _, _)| *name == format!("{b}{}", target.exe_suffix())),
        });
    }
    let bytes = if target.os == Os::Windows {
        archive::zip_bytes(&root, &members, facts.epoch)?
    } else {
        archive::tar_gz_bytes(&root, &members, facts.epoch)?
    };
    let archive_path = out_dir.join(target.archive_name(version));
    write(&archive_path, &bytes)?;
    check_archive(&archive_path, target, version)?;
    println!(
        "  {}: {} bytes, sha256 {}",
        archive_path.display(),
        bytes.len(),
        sha256_hex(&bytes)
    );

    // 7. This target's lines of MANIFEST.txt.
    let mut manifest = String::new();
    for m in &members {
        let description = described
            .iter()
            .find(|(n, _)| *n == m.name)
            .map_or("", |(_, d)| d.as_str());
        manifest.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            target.triple,
            m.name,
            m.bytes.len(),
            sha256_hex(&m.bytes),
            description
        ));
    }
    write(
        &out_dir.join(format!("{}.manifest", target.archive_name(version))),
        manifest.as_bytes(),
    )
}

/// The deny scan of a written archive.
fn check_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let entries = archive::archive_entries(path, &target.archive_root(version))?;
    let findings = guard::scan(&entries, &allowlist(target), guard::ARCHIVE_CAP);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the deny scan refused {}:\n  {}",
            path.display(),
            findings.join("\n  ")
        ))
    }
}

/// What a file in a release folder is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asset {
    Archive(Target),
    /// One target's lines of `MANIFEST.txt`.
    ManifestPart,
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
const INDEX_FILES: &[&str] = &["SHA256SUMS", "release.json", "MANIFEST.txt"];

/// Classify a file of the release folder, or refuse it: the folder holds this version's archives,
/// the manifest parts and the index, and nothing else.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    let archive = name.strip_suffix(".manifest").unwrap_or(name);
    for t in targets::TARGETS {
        if archive == t.archive_name(version) {
            return Ok(if archive.len() == name.len() {
                Asset::Archive(*t)
            } else {
                Asset::ManifestPart
            });
        }
    }
    Err(format!(
        "`{name}` is not a file of the empyrean {version} release (an archive \
         empyrean-{version}-<target>.zip|.tar.gz, its .manifest or the index)"
    ))
}

/// The index over a release folder: `MANIFEST.txt`, `release.json` and `SHA256SUMS`. Every
/// archive is scanned again first.
fn write_index(ws: &Path, dir: &Path, version: &str, facts: &BuildFacts) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut archives = Vec::new();
    let mut parts = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match classify(name, version) {
            Ok(Asset::Archive(t)) => archives.push((name.clone(), t)),
            Ok(Asset::ManifestPart) => parts.push(name.clone()),
            Ok(Asset::Index) => {}
            Err(e) => refused.push(e),
        }
    }
    if !refused.is_empty() {
        return Err(format!(
            "the release folder {} holds:\n  {}",
            dir.display(),
            refused.join("\n  ")
        ));
    }
    if archives.is_empty() {
        return Err(format!(
            "no empyrean {version} archives in {}",
            dir.display()
        ));
    }
    for (name, t) in &archives {
        check_archive(&dir.join(name), *t, version)?;
    }

    let mut manifest = format!(
        "# empyrean {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
        facts.commit
    );
    for p in &parts {
        manifest.push_str(&String::from_utf8_lossy(&read(&dir.join(p))?));
    }
    write(&dir.join("MANIFEST.txt"), manifest.as_bytes())?;

    let mut assets = Vec::new();
    for (name, t) in &archives {
        let bytes = read(&dir.join(name))?;
        assets.push(serde_json::json!({
            "target": t.triple, "file": name, "size": bytes.len(), "sha256": sha256_hex(&bytes),
        }));
    }
    let release = serde_json::json!({
        "schema": 2,
        "product": "empyrean",
        "version": version,
        "tag": version::tag_for(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "source_url": facts.source_url,
        "prerelease": version::is_prerelease(version),
        "assets": assets,
        "upgrade": declaration::for_release(ws, version)?,
    });
    let mut json = serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?;
    json.push('\n');
    write(&dir.join("release.json"), json.as_bytes())?;

    let mut sums = String::new();
    let mut summed: Vec<String> = archives.iter().map(|(n, _)| n.clone()).collect();
    summed.extend(["MANIFEST.txt".to_owned(), "release.json".to_owned()]);
    summed.sort();
    for name in &summed {
        sums.push_str(&format!(
            "{}  {name}\n",
            sha256_hex(&read(&dir.join(name))?)
        ));
    }
    write(&dir.join("SHA256SUMS"), sums.as_bytes())?;
    println!("\nSHA256SUMS\n{sums}");
    Ok(())
}

/// `cargo xtask version empyrean|dereth [--tag <tag>]`: print the product's version; with
/// `--tag`, fail unless the tag names exactly that version.
pub fn version_command(args: &[String]) -> i32 {
    let usage = "usage: cargo xtask version empyrean|dereth [--tag <tag>]";
    let Some(product) = args
        .first()
        .and_then(|a| version::Product::from_command_name(a))
    else {
        eprintln!("{usage}");
        return 2;
    };
    let tag = match &args[1..] {
        [] => None,
        [flag, tag] if flag == "--tag" => Some(tag.as_str()),
        _ => {
            eprintln!("{usage}");
            return 2;
        }
    };
    let ws = workspace_root();
    let checked = product.version(&ws).and_then(|v| {
        version::parse_release_version(&v)?;
        if let Some(tag) = tag {
            version::check_product_tag(product, tag, &v)?;
        }
        Ok(v)
    });
    match checked {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(e) => {
            eprintln!("version: {e}");
            1
        }
    }
}
