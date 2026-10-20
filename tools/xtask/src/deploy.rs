//! `cargo xtask deploy web`: a published web client release put on a Cloudflare Pages project.
//!
//! ```text
//! cargo xtask deploy web <version>|latest [--project <name>] [--branch <name>] [--zip <file>] [--dry-run]
//! ```
//!
//! 1. **The release.** `latest` is the newest published web client release, found as a host finds
//!    it (`dereth/web/DEPLOY.md`, "Updating a host from the releases"): the first of the
//!    repository's releases that is neither a draft nor a pre-release and is tagged
//!    `dereth-web-v<version>`. A version names its own release, a pre-release too. GitHub's public
//!    API is read without a token, and it shows no drafts, so a release not yet published is
//!    refused as missing. `--zip <file>` deploys a local package instead (what
//!    `cargo xtask package web` wrote), with the `web.json` beside it; a version given with it
//!    must be the package's.
//! 2. **The archive** must have the size and SHA-256 its `web.json` gives.
//! 3. **Unpacked** into a fresh temporary folder, the bundle is checked again as it lies there
//!    ([`web::check_unpacked`]): exactly the files `web.json` lists, each its size and SHA-256,
//!    the page, `_headers` and the module among them, and none over the 25 MiB Cloudflare Pages
//!    serves as one file.
//! 4. **Deployed** with `wrangler pages deploy <folder> --project-name <project>`, and
//!    `--branch <branch>` when given: `wrangler` on the `PATH`, else `npx wrangler`. The project
//!    is `--project`, else [`PROJECT_VAR`]; with neither the deploy is refused.
//!
//! **The credentials are wrangler's.** This reads, stores and prints none: wrangler uses its own
//! login, or `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`, which reach it in this process's
//! environment untouched.
//!
//! **`--dry-run`** does the first three steps and prints the command it would run. It runs no
//! program, and keeps the unpacked folder for a look.

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::package::{self, archive, sha256_hex, version, web};
use crate::publish::github::{GitHub, Https, Release, Transport};

pub const USAGE: &str = "usage: cargo xtask deploy web <version>|latest [--project <name>] \
                         [--branch <name>] [--zip <file>] [--dry-run]";

/// The variable the Pages project is read from when `--project` is not given.
pub const PROJECT_VAR: &str = "DERETH_PAGES_PROJECT";

/// Which release to deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Wanted {
    Latest,
    Version(String),
}

/// Where the bundle comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Release(Wanted),
    /// A local package, and the version it must be when one was given.
    Zip(PathBuf, Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    source: Source,
    project: String,
    branch: Option<String>,
    dry_run: bool,
}

/// The options of `deploy web ...`, the project read through `env` when no flag gives it.
fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Options, String> {
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("web") {
        return Err(USAGE.to_owned());
    }
    let (mut named, mut project, mut branch, mut zip, mut dry_run) =
        (None, None, None, None, false);
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--project" => project = Some(value()?),
            "--branch" => branch = Some(value()?),
            "--zip" => zip = Some(PathBuf::from(value()?)),
            "--dry-run" => dry_run = true,
            other if !other.starts_with('-') && named.is_none() => named = Some(other.to_owned()),
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    let wanted = match named.as_deref() {
        None => None,
        Some("latest") => Some(Wanted::Latest),
        Some(v) => {
            version::parse_release_version(v)?;
            Some(Wanted::Version(v.to_owned()))
        }
    };
    let source = match (zip, wanted) {
        (Some(file), None) => Source::Zip(file, None),
        (Some(file), Some(Wanted::Version(v))) => Source::Zip(file, Some(v)),
        (Some(_), Some(Wanted::Latest)) => {
            return Err(format!(
                "--zip deploys that package, not the latest release: give one of them\n{USAGE}"
            ))
        }
        (None, Some(w)) => Source::Release(w),
        (None, None) => return Err(format!("name the release: a version, or latest\n{USAGE}")),
    };
    let project = project
        .or_else(|| env(PROJECT_VAR))
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            format!(
                "name the Cloudflare Pages project: --project <name>, or set {PROJECT_VAR} \
                 (for example {PROJECT_VAR}=my-pages-project)"
            )
        })?;
    check_project(&project)?;
    if let Some(b) = &branch {
        check_branch(b)?;
    }
    Ok(Options {
        source,
        project,
        branch,
        dry_run,
    })
}

/// A Pages project's name: lowercase letters, digits and hyphens, at most 58, no hyphen at
/// either end.
fn check_project(name: &str) -> Result<(), String> {
    let ok = (1..=58).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{name}` is not a Pages project name (lowercase letters, digits and hyphens)"
        ))
    }
}

/// A branch name: letters, digits and `-_./`, not starting with a hyphen.
fn check_branch(name: &str) -> Result<(), String> {
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'));
    if ok {
        Ok(())
    } else {
        Err(format!("`{name}` is not a branch name"))
    }
}

/// The programs a deploy asks and runs. [`Real`] runs them; the tests hand the deploy a fake that
/// keeps a record and runs nothing.
pub trait Programs {
    /// Whether `program args` starts and succeeds. It changes nothing (it asks a version).
    fn probe(&mut self, program: &str, args: &[String]) -> bool;
    /// `program args` run in `dir` on this console, with this process's environment as it is.
    fn run(&mut self, dir: &Path, program: &str, args: &[String]) -> Result<(), String>;
}

/// This machine's programs.
#[derive(Debug, Default)]
pub struct Real;

impl Programs for Real {
    fn probe(&mut self, program: &str, args: &[String]) -> bool {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    fn run(&mut self, dir: &Path, program: &str, args: &[String]) -> Result<(), String> {
        let status = Command::new(program)
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| format!("{program}: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("`{program} {}` failed ({status})", args.join(" ")))
        }
    }
}

/// `cargo xtask deploy web ...`.
pub fn deploy(args: &[String]) -> i32 {
    let o = match parse(args, &|k| std::env::var(k).ok()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let repo = package::DEFAULT_SOURCE_URL
        .strip_prefix("https://github.com/")
        .unwrap_or(package::DEFAULT_SOURCE_URL);
    let work = std::env::temp_dir().join(format!("dereth-web-deploy-{}", std::process::id()));
    match run(&o, repo, &mut Https::anonymous(), &mut Real, &work) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\ndeploy: STOPPED\n{e}");
            1
        }
    }
}

/// One deploy: the bundle found and checked, unpacked into `work` (made afresh) and checked
/// again, then given to wrangler unless this is a dry run.
fn run(
    o: &Options,
    repo: &str,
    transport: &mut dyn Transport,
    programs: &mut dyn Programs,
    work: &Path,
) -> Result<(), String> {
    let (manifest, zip) = match &o.source {
        Source::Release(wanted) => fetch(&mut GitHub::new(transport, repo, false), wanted)?,
        Source::Zip(file, version) => local(file, version.as_deref())?,
    };
    check_archive(&manifest, &zip)?;
    let remove = |dir: &Path| match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", dir.display()))
        }
        _ => Ok(()),
    };
    remove(work)?;
    let folder = work.join(&manifest.archive_root);
    archive::unpack_zip(&zip, &manifest.archive_root, &folder)?;
    let findings = web::check_unpacked(&folder, &manifest);
    if !findings.is_empty() {
        remove(work)?;
        return Err(format!(
            "the unpacked bundle is refused:\n  {}",
            findings.join("\n  ")
        ));
    }
    println!(
        "deploy: dereth-web {} checked, {} files in {}",
        manifest.version,
        manifest.files.len(),
        folder.display()
    );
    let args = wrangler_args(&folder, &o.project, o.branch.as_deref());
    if o.dry_run {
        println!(
            "deploy: a dry run; it would run\n  wrangler {}\n(or `npx wrangler` when wrangler is \
             not on the PATH). The files stay in {}",
            args.join(" "),
            folder.display()
        );
        return Ok(());
    }
    let (program, mut argv) = find_wrangler(programs, cfg!(windows))?;
    argv.extend(args);
    println!("$ {program} {}", argv.join(" "));
    let deployed = programs.run(work, &program, &argv);
    remove(work)?;
    deployed?;
    println!(
        "deploy: dereth-web {} is deployed to the Pages project {}",
        manifest.version, o.project
    );
    Ok(())
}

/// The release `wanted` names among `releases`, newest first as GitHub lists them.
fn pick<'r>(releases: &'r [Release], wanted: &Wanted, repo: &str) -> Result<&'r Release, String> {
    let prefix = web::TAG_PREFIX;
    match wanted {
        Wanted::Latest => releases
            .iter()
            .find(|r| {
                !r.draft
                    && !r.prerelease
                    && r.tag_name
                        .strip_prefix(prefix)
                        .is_some_and(|v| version::parse_release_version(v).is_ok())
            })
            .ok_or_else(|| {
                format!(
                    "{repo} has no published web client release ({prefix}<version>, not a draft \
                     or a pre-release)"
                )
            }),
        Wanted::Version(v) => {
            let tag = format!("{prefix}{v}");
            match releases.iter().find(|r| r.tag_name == tag) {
                Some(r) if r.draft => Err(format!(
                    "{tag} is a draft on {repo}: publish it, then deploy it"
                )),
                Some(r) => Ok(r),
                None => Err(format!(
                    "{repo} has no published release {tag} (a draft is not seen until it is \
                     published)"
                )),
            }
        }
    }
}

/// The release's manifest and archive, downloaded.
fn fetch(gh: &mut GitHub<'_>, wanted: &Wanted) -> Result<(web::Manifest, Vec<u8>), String> {
    let releases = gh.releases()?;
    let release = pick(&releases, wanted, &gh.repo)?;
    println!("deploy: {} ({})", release.tag_name, release.html_url);
    let assets = gh.assets(release.id)?;
    let asset = |name: &str| {
        assets
            .iter()
            .find(|a| a.name == name && a.complete())
            .ok_or_else(|| format!("{} has no {name}", release.tag_name))
    };
    let manifest = web::Manifest::read(&gh.download(asset(web::MANIFEST)?)?)?;
    if manifest.tag != release.tag_name {
        return Err(format!(
            "the {} of {} describes {}",
            web::MANIFEST,
            release.tag_name,
            manifest.tag
        ));
    }
    let zip = gh.download(asset(&manifest.archive_file)?)?;
    Ok((manifest, zip))
}

/// A local package and the manifest beside it.
fn local(file: &Path, version: Option<&str>) -> Result<(web::Manifest, Vec<u8>), String> {
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("reading {}: {e}", p.display()));
    let beside = file
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(web::MANIFEST);
    let manifest = web::Manifest::read(&read(&beside)?)?;
    if let Some(v) = version {
        if manifest.version != v {
            return Err(format!(
                "{} is the package of {}, not {v}",
                file.display(),
                manifest.version
            ));
        }
    }
    Ok((manifest, read(file)?))
}

/// The archive is the one its manifest describes.
fn check_archive(m: &web::Manifest, zip: &[u8]) -> Result<(), String> {
    let (size, sha256) = (zip.len() as u64, sha256_hex(zip));
    if size == m.archive_size && sha256 == m.archive_sha256 {
        Ok(())
    } else {
        Err(format!(
            "{} is {size} bytes with SHA-256 {sha256}, where {} gives {} bytes and {}: a broken \
             download, or another release's archive",
            m.archive_file,
            web::MANIFEST,
            m.archive_size,
            m.archive_sha256
        ))
    }
}

/// wrangler's arguments for deploying `folder` to `project`, on `branch` when given.
fn wrangler_args(folder: &Path, project: &str, branch: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "pages".to_owned(),
        "deploy".to_owned(),
        folder.to_string_lossy().into_owned(),
        "--project-name".to_owned(),
        project.to_owned(),
    ];
    if let Some(b) = branch {
        args.extend(["--branch".to_owned(), b.to_owned()]);
    }
    args
}

/// How wrangler is started: itself on the `PATH`, else through npx, as the program and the
/// arguments before wrangler's own. On Windows both are npm's `.cmd` scripts, which are not found
/// without their extension.
fn find_wrangler(
    programs: &mut dyn Programs,
    windows: bool,
) -> Result<(String, Vec<String>), String> {
    let ext = if windows { ".cmd" } else { "" };
    for (program, before) in [("wrangler", None), ("npx", Some("wrangler"))] {
        let program = format!("{program}{ext}");
        if programs.probe(&program, &["--version".to_owned()]) {
            return Ok((program, before.map(str::to_owned).into_iter().collect()));
        }
    }
    Err(
        "neither wrangler nor npx is on the PATH: install wrangler (`npm install --global \
         wrangler`), or Node.js, whose npx runs it"
            .to_owned(),
    )
}
