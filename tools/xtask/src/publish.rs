//! `cargo xtask publish`: a product's GitHub release made from release files built on the
//! maintainers' own machines, through GitHub's REST API, without the release workflows.
//!
//! ```text
//! cargo xtask publish dereth|empyrean|web <version> --upload <dir> [--repo <owner/name>] [--yes]
//! cargo xtask publish dereth|empyrean|web <version> --finish [--repo <owner/name>] [--yes]
//! cargo xtask publish dereth|empyrean|web <version> --publish [--repo <owner/name>] [--yes]
//! ```
//!
//! The release it makes is the one the product's release workflow drafts from the same tag: the
//! same files with the same labels, the same `SHA256SUMS`, `MANIFEST.txt`, `release.json` and
//! `latest.json` (written by the same code, `cargo xtask package <product> --gather`, over every
//! machine's files), the same title and description, and the same pre-release and "Latest"
//! rules ([`files`]). Three steps:
//!
//! 1. **`--upload <dir>`**, on each build machine, of the folder
//!    `cargo xtask package <product> --out <dir>` wrote: checks it (every file belongs to the
//!    release; every release file passes the
//!    data guard again; for Dereth, every launcher file is signed and its signature verifies
//!    against the launcher's key; the files were built from the commit the tag names on GitHub),
//!    then creates the **draft** release for the tag when there is none yet and adds the folder's
//!    files to it. A file already there with the same SHA-256 is skipped, and a file of the same
//!    name with other bytes refuses the whole upload before anything is sent. The tag must already
//!    be on GitHub and on `main`: nothing here makes or pushes a tag.
//! 2. **`--finish`**, once, from a clean checkout of the tag: lists by name every file the
//!    release lacks and stops there, or downloads every machine's files, writes the merged files
//!    over them, and adds those and the notes (`cargo xtask release-notes`, then the downloads
//!    text) to the draft. The release stays a draft, for review.
//! 3. **`--publish`**: checks the draft is finished (every file there, `SHA256SUMS` matching every
//!    file), removes the staging files, and publishes it.
//!
//! **A dry run by default.** Without `--yes` nothing that changes the release is sent: each step
//! reads what it needs, prints what it would create, upload, replace or remove, and stops.
//!
//! **The token** is read from `DERETH_PUBLISH_TOKEN`, else `GITHUB_TOKEN`: a fine-grained token
//! for the one repository with only "Contents: read and write". It is sent to GitHub's API in
//! the `Authorization` header and nowhere else, never printed, logged or written. Without one,
//! `--upload` checks the folder alone, and the other steps refuse.

mod files;
mod github;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::package::{self, dereth as launcher, sha256_hex, version, web};
use crate::release::notes;
use crate::util::{target_dir, workspace_root};
use files::{Kind, Role};
use github::{GitHub, Https, Release, RemoteAsset, Token};

pub const USAGE: &str =
    "usage: cargo xtask publish dereth|empyrean|web <version> --upload <dir> [--repo <owner/name>] [--yes]
       cargo xtask publish dereth|empyrean|web <version> --finish [--repo <owner/name>] [--yes]
       cargo xtask publish dereth|empyrean|web <version> --publish [--repo <owner/name>] [--yes]";

/// Which step a run is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Upload(PathBuf),
    Finish,
    Publish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    kind: Kind,
    version: String,
    step: Step,
    repo: Option<String>,
    yes: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut it = args.iter();
    let kind = it
        .next()
        .and_then(|a| Kind::from_command_name(a))
        .ok_or_else(|| USAGE.to_owned())?;
    let version = it.next().cloned().ok_or_else(|| USAGE.to_owned())?;
    version::parse_release_version(&version)?;
    let mut steps = Vec::new();
    let mut repo = None;
    let mut yes = false;
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--upload" => steps.push(Step::Upload(PathBuf::from(value()?))),
            "--finish" => steps.push(Step::Finish),
            "--publish" => steps.push(Step::Publish),
            "--repo" => repo = Some(value()?),
            "--yes" => yes = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    if steps.len() != 1 {
        return Err(format!(
            "name one step: --upload <dir>, --finish or --publish\n{USAGE}"
        ));
    }
    if let Some(r) = &repo {
        check_repo_name(r)?;
    }
    Ok(Options {
        kind,
        version,
        step: steps.remove(0),
        repo,
        yes,
    })
}

fn check_repo_name(repo: &str) -> Result<(), String> {
    let ok = repo.split_once('/').is_some_and(|(o, n)| {
        let part = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        part(o) && part(n)
    });
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{repo}` is not a repository name (<owner>/<name>)"
        ))
    }
}

/// The repository: `--repo`, else the one the build environment names as the release files'
/// source (a `https://github.com/<owner>/<name>` address), else the default.
fn repository(kind: Kind, given: Option<&str>) -> Result<String, String> {
    if let Some(r) = given {
        return Ok(r.to_owned());
    }
    let var = match kind {
        Kind::Empyrean => "EMPYREAN_BUILD_SOURCE_URL",
        Kind::Dereth | Kind::Web => launcher::SOURCE_URL_ENV,
    };
    let url = std::env::var(var)
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| package::DEFAULT_SOURCE_URL.to_owned());
    let repo = url
        .trim()
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")
        .ok_or_else(|| format!("{var} is {url}, not a GitHub repository: pass --repo"))?
        .to_owned();
    check_repo_name(&repo)?;
    Ok(repo)
}

/// The address the release files name as their source, and download from.
pub fn source_url(repo: &str) -> String {
    format!("https://github.com/{repo}")
}

/// `cargo xtask publish ...`.
pub fn publish(args: &[String]) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match run(&o) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\npublish: STOPPED\n{e}");
            1
        }
    }
}

fn run(o: &Options) -> Result<(), String> {
    let ws = workspace_root();
    let repo = repository(o.kind, o.repo.as_deref())?;
    let tag = o.kind.tag(&o.version);
    println!(
        "publish: {} {} ({tag}) on {repo}{}",
        o.kind.command_name(),
        o.version,
        if o.yes { "" } else { ", a dry run" }
    );
    let token = Token::from_env();
    if let Some((_, var)) = &token {
        println!("publish: the token is read from {var}");
    }
    if o.yes && token.is_none() {
        return Err(no_token());
    }
    match &o.step {
        Step::Upload(dir) => {
            let upload = read_upload_dir(&ws, o.kind, &o.version, &repo, dir)?;
            println!("\n{} holds, for {tag}:", dir.display());
            for f in &upload.files {
                println!(
                    "  {}  {} bytes  sha256 {}  \"{}\"",
                    f.name, f.size, f.sha256, f.label
                );
            }
            println!("built from {}", upload.commit);
            let Some((token, _)) = token else {
                println!(
                    "\nno token ({}): the folder is checked, and the release on GitHub was not \
                     read. With a token, a dry run also says what it would upload.",
                    github::TOKEN_VARS.join(" or ")
                );
                return Ok(());
            };
            let mut https = Https::new(token);
            let mut gh = GitHub::new(&mut https, &repo, o.yes);
            let plan = plan_upload(&mut gh, o.kind, &o.version, &upload)?;
            finish_run(&mut gh, plan, o)
        }
        Step::Finish => {
            let (token, _) = token.ok_or_else(no_token)?;
            let context = finish_context(&ws, o.kind, &o.version, &repo)?;
            let mut https = Https::new(token);
            let mut gh = GitHub::new(&mut https, &repo, o.yes);
            let ws2 = ws.clone();
            let facts = context.facts.clone();
            let kind = o.kind;
            let version = o.version.clone();
            let url = source_url(&repo);
            let gather = move |dir: &Path| gather(&ws2, kind, &version, &facts, &url, dir);
            let plan = plan_finish(&mut gh, o.kind, &o.version, &context.finish(&gather))?;
            finish_run(&mut gh, plan, o)
        }
        Step::Publish => {
            let (token, _) = token.ok_or_else(no_token)?;
            let mut https = Https::new(token);
            let mut gh = GitHub::new(&mut https, &repo, o.yes);
            let plan = plan_publish(&mut gh, o.kind, &o.version)?;
            finish_run(&mut gh, plan, o)
        }
    }
}

fn no_token() -> String {
    format!(
        "no token: set {} to a fine-grained token for this repository with Contents: read and \
         write (CONTRIBUTING.md, \"Releasing without GitHub Actions\")",
        github::TOKEN_VARS.join(" or ")
    )
}

/// Print the plan and, with `--yes`, carry it out.
fn finish_run(gh: &mut GitHub<'_>, plan: Plan, o: &Options) -> Result<(), String> {
    println!();
    for line in &plan.said {
        println!("{line}");
    }
    if plan.actions.is_empty() {
        println!("nothing to change on GitHub");
        return Ok(());
    }
    if !o.yes {
        println!("\na dry run: with --yes this run would");
        for a in &plan.actions {
            println!("  {}", a.describe());
        }
        println!("\nnothing was changed; run it again with --yes to do it");
        return Ok(());
    }
    // `apply` says each change as it makes it.
    println!("\nchanging the release:");
    let release = apply(gh, plan)?;
    println!("\ndone: {}", release.html_url);
    match &o.step {
        Step::Upload(_) => println!(
            "Upload each machine's files the same way; then `cargo xtask publish {} {} --finish`.",
            o.kind.command_name(),
            o.version
        ),
        Step::Finish => println!(
            "The release is a draft: review it on GitHub, then `cargo xtask publish {} {} --publish`.",
            o.kind.command_name(),
            o.version
        ),
        Step::Publish => println!("published"),
    }
    Ok(())
}

/// A file of a machine's release folder that goes to the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub label: String,
}

impl LocalFile {
    fn read(dir: &Path, name: &str, label: String) -> Result<Self, String> {
        let path = dir.join(name);
        let bytes = read(&path)?;
        Ok(Self {
            name: name.to_owned(),
            size: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            path,
            label,
        })
    }
}

/// What a machine uploads: its release and staging files, and the commit they were built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub files: Vec<LocalFile>,
    pub commit: String,
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// The facts a package's `release.json` or `web.json` states, checked against the release:
/// the version, and the repository the files name as their source. The commit they were built
/// from.
fn built_from(path: &Path, version: &str, repo: &str) -> Result<String, String> {
    let facts = read_json(path)?;
    let name = path.display();
    if facts["version"].as_str() != Some(version) {
        return Err(format!(
            "{name} is for version {}, not {version}",
            facts["version"]
        ));
    }
    let source = facts["source_url"].as_str().unwrap_or_default();
    if source != source_url(repo) {
        return Err(format!(
            "{name}: the files were built for {source}, not {}: package them again with the \
             repository set (DERETH_BUILD_SOURCE_URL or EMPYREAN_BUILD_SOURCE_URL), or pass the \
             --repo they name",
            source_url(repo)
        ));
    }
    facts["commit"]
        .as_str()
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{name} names no commit"))
}

/// The web client's manifest checked against the bundle beside it in `dir`: its version, its
/// archive's name, size, SHA-256 and address. The commit it names.
fn check_web_manifest(dir: &Path, version: &str, repo: &str) -> Result<String, String> {
    let path = dir.join(web::MANIFEST);
    let commit = built_from(&path, version, repo)?;
    let m = read_json(&path)?;
    let zip_name = web::archive_name(version);
    let zip = read(&dir.join(&zip_name))?;
    let a = &m["archive"];
    let url = format!(
        "{}/releases/download/{}{version}/{zip_name}",
        source_url(repo),
        web::TAG_PREFIX
    );
    if m["product"].as_str() != Some("dereth-web")
        || a["file"].as_str() != Some(zip_name.as_str())
        || a["size"].as_u64() != Some(zip.len() as u64)
        || a["sha256"].as_str() != Some(sha256_hex(&zip).as_str())
        || a["url"].as_str() != Some(url.as_str())
    {
        return Err(format!(
            "{} does not describe {zip_name} beside it (its name, size, SHA-256 or address \
             {url}): package them again together",
            web::MANIFEST
        ));
    }
    Ok(commit)
}

/// A machine's release folder, as `cargo xtask package <product> --out <dir>` wrote it, checked
/// and read: every file belongs to the release, every release file passes the data guard again,
/// every Dereth launcher file is signed with a signature the launcher accepts, and every release
/// file has its staging files beside it. The index files `package` wrote are this machine's alone
/// and are left behind (`--finish` writes the release's).
pub fn read_upload_dir(
    ws: &Path,
    kind: Kind,
    version: &str,
    repo: &str,
    dir: &Path,
) -> Result<Upload, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut problems = Vec::new();
    let mut release = Vec::new();
    let mut staging = Vec::new();
    let mut index = Vec::new();
    for name in &names {
        match files::role(kind, version, name) {
            Ok(Role::Release) => release.push(name.clone()),
            Ok(Role::Staging) => staging.push(name.clone()),
            Ok(Role::Index) => index.push(name.clone()),
            Err(e) => problems.push(e),
        }
    }
    if !problems.is_empty() {
        return Err(format!(
            "{} holds files that are not this release's:\n  {}",
            dir.display(),
            problems.join("\n  ")
        ));
    }
    if release.is_empty() {
        return Err(format!(
            "{} holds no {} {version} release files: run `cargo xtask package {} --out {}` first",
            dir.display(),
            kind.command_name(),
            kind.command_name(),
            dir.display()
        ));
    }

    // The data guard again, and each release file's companions.
    let url = source_url(repo);
    for name in &release {
        let path = dir.join(name);
        match kind {
            Kind::Dereth => {
                let launcher::Asset::Launcher(t) = launcher::classify(name, version)? else {
                    continue;
                };
                launcher::check_archive(&path, t, version)?;
                if !staging.contains(&format!("{name}.manifest")) {
                    problems.push(format!("{name}: its {name}.manifest is not beside it"));
                }
                let piece = launcher::piece_name(t);
                if !staging.contains(&piece) {
                    problems.push(format!(
                        "{name} is not signed (no {piece} beside it): set {} and {} and package \
                         it again; a release's launcher files are signed",
                        package::sign::KEY_VAR,
                        package::sign::PASSWORD_VAR
                    ));
                    continue;
                }
                let value = read_json(&dir.join(&piece))?;
                let entry = &value[t.updater_platform()];
                let want = launcher::download_url(&url, version, name);
                if entry["url"].as_str() != Some(want.as_str()) {
                    problems.push(format!("{piece} points at {}, not at {want}", entry["url"]));
                }
                let signature = entry["signature"].as_str().unwrap_or_default();
                let key = launcher::launcher_public_key(ws)?;
                if let Err(e) = package::sign::verify(&read(&path)?, signature, &key, version) {
                    problems.push(format!(
                        "{name}: its update signature in {piece} is refused: {e}"
                    ));
                }
            }
            Kind::Empyrean => {
                let package::Asset::Archive(t) = package::classify(name, version)? else {
                    continue;
                };
                package::check_archive(&path, t, version)?;
                if !staging.contains(&format!("{name}.manifest")) {
                    problems.push(format!("{name}: its {name}.manifest is not beside it"));
                }
            }
            Kind::Web => {
                if *name == web::archive_name(version) {
                    web::check_bundle(&path, version)?;
                }
            }
        }
    }
    for name in &staging {
        let owner = name
            .strip_suffix(".manifest")
            .map(str::to_owned)
            .or_else(|| {
                // A latest-<platform>.json piece belongs to its platform's launcher file.
                launcher::classify(name, version)
                    .ok()
                    .and_then(|a| match a {
                        launcher::Asset::Piece(t) => Some(launcher::release_file_name(t, version)),
                        _ => None,
                    })
            });
        if owner.is_some_and(|o| !release.contains(&o)) {
            problems.push(format!("{name} has no release file beside it"));
        }
    }
    let commit = match kind {
        Kind::Web => {
            if release.len() != 2 {
                problems.push(format!(
                    "a web client release is {} and {} together",
                    web::archive_name(version),
                    web::MANIFEST
                ));
                String::new()
            } else {
                check_web_manifest(dir, version, repo)?
            }
        }
        Kind::Dereth | Kind::Empyrean => {
            let facts = dir.join("release.json");
            if facts.is_file() {
                built_from(&facts, version, repo)?
            } else {
                problems.push(format!(
                    "{} has no release.json, which names the commit the files were built from: \
                     upload the folder `cargo xtask package` wrote, as it wrote it",
                    dir.display()
                ));
                String::new()
            }
        }
    };
    if !problems.is_empty() {
        let read = |names: &[String]| {
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(", ")
            }
        };
        return Err(format!(
            "{} cannot be uploaded:\n  {}\nIt holds release files {} (the data guard passed \
             over each), staging files {}, and this machine's own index {}, which stays behind.",
            dir.display(),
            problems.join("\n  "),
            read(&release),
            read(&staging),
            read(&index)
        ));
    }
    let mut out = Vec::new();
    for name in release.iter().chain(&staging) {
        let label =
            files::label(kind, version, name).ok_or_else(|| format!("{name} has no label"))?;
        out.push(LocalFile::read(dir, name, label)?);
    }
    Ok(Upload { files: out, commit })
}

/// One change to GitHub.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Create the draft release with these fields.
    CreateDraft(Value),
    /// Add a file.
    Upload(LocalFile),
    /// Remove a file (an unfinished upload, a stale index file before its replacement, or a
    /// staging file when the release is published).
    Delete(RemoteAsset, &'static str),
    /// Change the release's fields.
    Update(Value, &'static str),
}

impl Action {
    fn describe(&self) -> String {
        match self {
            Self::CreateDraft(f) => format!(
                "create the draft release {} titled \"{}\"{}",
                f["tag_name"].as_str().unwrap_or_default(),
                f["name"].as_str().unwrap_or_default(),
                if f["prerelease"] == json!(true) {
                    ", a pre-release"
                } else {
                    ""
                }
            ),
            Self::Upload(f) => format!(
                "upload {} ({} bytes, sha256 {}) labelled \"{}\"",
                f.name, f.size, f.sha256, f.label
            ),
            Self::Delete(a, why) => format!("remove {} ({why})", a.name),
            Self::Update(_, what) => (*what).to_owned(),
        }
    }
}

/// What a step would do: the release it acts on (none until the first upload creates it), what
/// it changes, and what it found.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub release: Option<Release>,
    pub actions: Vec<Action>,
    pub said: Vec<String>,
}

/// Carry out `plan`, in order. The release as it is afterwards.
pub fn apply(gh: &mut GitHub<'_>, plan: Plan) -> Result<Release, String> {
    let mut release = plan.release;
    for action in plan.actions {
        println!("  {}", action.describe());
        match action {
            Action::CreateDraft(fields) => release = Some(gh.create_release(&fields)?),
            Action::Upload(f) => {
                let r = release.as_ref().ok_or("no release to upload to")?;
                let bytes = read(&f.path)?;
                if sha256_hex(&bytes) != f.sha256 {
                    return Err(format!(
                        "{} changed while this run read it; nothing more is sent",
                        f.path.display()
                    ));
                }
                gh.upload(r, &f.name, &f.label, &bytes)?;
            }
            Action::Delete(asset, _) => gh.delete_asset(&asset)?,
            Action::Update(fields, _) => {
                let r = release.as_ref().ok_or("no release to change")?;
                release = Some(gh.update_release(r.id, &fields)?);
            }
        }
    }
    release.ok_or_else(|| "no release".to_owned())
}

/// The commit `tag` names on GitHub, which must be on `main`; refused when GitHub has no such tag.
fn remote_tag(gh: &mut GitHub<'_>, tag: &str) -> Result<String, String> {
    gh.check_access()?;
    let commit = gh.tag_commit(tag)?.ok_or_else(|| {
        format!(
            "the tag {tag} is not on {}: push it first (`git push <remote> {tag}`; `cargo xtask \
             release` makes it). Publishing never makes or pushes a tag",
            gh.repo
        )
    })?;
    if !gh.on_branch(&commit, "main")? {
        return Err(format!(
            "{tag} names {commit}, which is not on {}'s main",
            gh.repo
        ));
    }
    Ok(commit)
}

/// The draft release for `tag`, if there is one. A published release for the tag, or two drafts,
/// are refused.
fn find_draft(gh: &mut GitHub<'_>, tag: &str) -> Result<Option<Release>, String> {
    let mut found: Vec<Release> = gh
        .releases()?
        .into_iter()
        .filter(|r| r.tag_name == tag)
        .collect();
    if let Some(published) = found.iter().find(|r| !r.draft) {
        return Err(format!(
            "{tag} is already published ({}): publishing changes only a draft",
            published.html_url
        ));
    }
    if found.len() > 1 {
        return Err(format!(
            "{} draft releases name {tag} ({}): delete all but one on GitHub",
            found.len(),
            found
                .iter()
                .map(|r| r.html_url.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(found.pop())
}

/// The SHA-256 of a release file: the one GitHub computed, or that of its bytes downloaded.
fn remote_sha256(gh: &mut GitHub<'_>, asset: &RemoteAsset) -> Result<String, String> {
    match &asset.digest {
        Some(d) => Ok(d.clone()),
        None => Ok(sha256_hex(&gh.download(asset)?)),
    }
}

/// What to do with a file a machine uploads, given the draft's file of the same name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Upload,
    /// The same bytes are there already.
    Skip,
    /// An upload of it never finished: remove it and upload again.
    Retry,
    /// Other bytes are there under the name.
    Refuse(String),
}

/// What to do with a local file of SHA-256 `local`, when the draft has `remote` (its state and
/// SHA-256) under the same name.
pub fn decide(local: &str, remote: Option<(bool, &str)>) -> Decision {
    match remote {
        None => Decision::Upload,
        Some((false, _)) => Decision::Retry,
        Some((true, sha)) if sha == local => Decision::Skip,
        Some((true, sha)) => Decision::Refuse(sha.to_owned()),
    }
}

/// `--upload`: create the draft when there is none, and add this machine's files to it.
pub fn plan_upload(
    gh: &mut GitHub<'_>,
    kind: Kind,
    version: &str,
    upload: &Upload,
) -> Result<Plan, String> {
    let tag = kind.tag(version);
    let commit = remote_tag(gh, &tag)?;
    if commit != upload.commit {
        return Err(format!(
            "the files were built from {}, but {tag} names {commit}: package the tagged commit",
            upload.commit
        ));
    }
    let release = find_draft(gh, &tag)?;
    let mut actions = Vec::new();
    let mut said = Vec::new();
    let remote = match &release {
        Some(r) => {
            said.push(format!("the draft {} is there: {}", r.tag_name, r.html_url));
            gh.assets(r.id)?
        }
        None => {
            let (pre, _) = files::publish_flags(kind, version);
            actions.push(Action::CreateDraft(json!({
                "tag_name": tag,
                "name": kind.title(version),
                "body": files::PLACEHOLDER_BODY,
                "draft": true,
                "prerelease": pre,
            })));
            Vec::new()
        }
    };
    let mut refused = Vec::new();
    for f in &upload.files {
        let there = remote.iter().find(|a| a.name == f.name);
        let state = match there {
            Some(a) if a.complete() => Some((true, remote_sha256(gh, a)?)),
            Some(_) => Some((false, String::new())),
            None => None,
        };
        match decide(&f.sha256, state.as_ref().map(|(c, s)| (*c, s.as_str()))) {
            Decision::Upload => actions.push(Action::Upload(f.clone())),
            Decision::Skip => said.push(format!(
                "{}: already there with the same SHA-256; skipped",
                f.name
            )),
            Decision::Retry => {
                if let Some(a) = there {
                    actions.push(Action::Delete(a.clone(), "an upload that never finished"));
                }
                actions.push(Action::Upload(f.clone()));
            }
            Decision::Refuse(theirs) => refused.push(format!(
                "{}: the draft has other bytes under this name (sha256 {theirs}; this one is {}). \
                 Two machines built different files, or the file was rebuilt: delete the draft's \
                 copy on GitHub if this one is right",
                f.name, f.sha256
            )),
        }
    }
    if !refused.is_empty() {
        return Err(format!("nothing is uploaded:\n  {}", refused.join("\n  ")));
    }
    Ok(Plan {
        release,
        actions,
        said,
    })
}

/// The draft's files sorted by role; a file the release does not carry is refused, and an upload
/// that never finished counts as missing.
fn sort_assets(
    kind: Kind,
    version: &str,
    assets: &[RemoteAsset],
) -> Result<BTreeMap<String, RemoteAsset>, String> {
    let mut unknown = Vec::new();
    let mut out = BTreeMap::new();
    for a in assets {
        match files::role(kind, version, &a.name) {
            Ok(_) if a.complete() => {
                out.insert(a.name.clone(), a.clone());
            }
            Ok(_) => {}
            Err(e) => unknown.push(e),
        }
    }
    if unknown.is_empty() {
        Ok(out)
    } else {
        Err(format!(
            "the draft carries files that are not this release's; delete them on GitHub:\n  {}",
            unknown.join("\n  ")
        ))
    }
}

/// The names in `wanted` that `have` lacks, each said with what it is.
pub fn missing(kind: Kind, version: &str, wanted: &[String], have: &[String]) -> Vec<String> {
    wanted
        .iter()
        .filter(|n| !have.contains(n))
        .map(|n| {
            let what = match files::role(kind, version, n) {
                Ok(Role::Staging) if n.ends_with(".manifest") => {
                    "its lines of MANIFEST.txt, uploaded with the release file"
                }
                Ok(Role::Staging) => "its platform's update signature, uploaded with the launcher",
                Ok(Role::Index) => "written by --finish",
                _ => "a release file",
            };
            format!("{n} ({what})")
        })
        .collect()
}

/// What `--finish` is given beside the draft: where to download to, the commit checked out, the
/// notes, and the product's index writer.
pub struct Finish<'a> {
    pub work: PathBuf,
    pub commit: String,
    pub notes: String,
    pub gather: &'a dyn Fn(&Path) -> Result<(), String>,
}

impl std::fmt::Debug for Finish<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Finish")
            .field("work", &self.work)
            .field("commit", &self.commit)
            .finish_non_exhaustive()
    }
}

/// `--finish`: every file there, the merged files written over every machine's and added, and the
/// notes set; the release stays a draft.
pub fn plan_finish(
    gh: &mut GitHub<'_>,
    kind: Kind,
    version: &str,
    finish: &Finish<'_>,
) -> Result<Plan, String> {
    let tag = kind.tag(version);
    let commit = remote_tag(gh, &tag)?;
    if commit != finish.commit {
        return Err(format!(
            "this checkout is at {}, but {tag} names {commit}: check out the tag \
             (`git checkout {tag}`)",
            finish.commit
        ));
    }
    let release = find_draft(gh, &tag)?.ok_or_else(|| {
        format!(
            "there is no draft release for {tag}: upload the release files first (--upload <dir>)"
        )
    })?;
    let assets = sort_assets(kind, version, &gh.assets(release.id)?)?;
    let expected = files::expected(kind, version);
    let have: Vec<String> = assets.keys().cloned().collect();
    let mut wanted = expected.release.clone();
    wanted.extend(expected.staging.iter().cloned());
    wanted.sort();
    let lacking = missing(kind, version, &wanted, &have);
    if !lacking.is_empty() {
        return Err(format!(
            "the draft {tag} lacks {} of the files the release carries; upload them from the \
             machine that builds them:\n  {}",
            lacking.len(),
            lacking.join("\n  ")
        ));
    }

    // Every machine's files, downloaded and checked against GitHub's hashes.
    if finish.work.exists() {
        std::fs::remove_dir_all(&finish.work)
            .map_err(|e| format!("{}: {e}", finish.work.display()))?;
    }
    std::fs::create_dir_all(&finish.work).map_err(|e| format!("{}: {e}", finish.work.display()))?;
    let mut said = vec![format!(
        "downloading {} files of the draft into {}",
        wanted.len(),
        finish.work.display()
    )];
    for name in &wanted {
        let asset = &assets[name];
        let bytes = gh.download(asset)?;
        if let Some(d) = &asset.digest {
            if *d != sha256_hex(&bytes) {
                return Err(format!(
                    "{name} downloaded with another SHA-256 than GitHub states ({d}); try again"
                ));
            }
        }
        std::fs::write(finish.work.join(name), &bytes)
            .map_err(|e| format!("writing {name}: {e}"))?;
    }
    // The merged files, by the code the workflow runs over every runner's files.
    (finish.gather)(&finish.work)?;

    let mut actions = Vec::new();
    for name in &expected.index {
        let path = finish.work.join(name);
        if !path.is_file() {
            return Err(format!(
                "{name} was not written over the release files (a launcher file unsigned?)"
            ));
        }
        let label = files::label(kind, version, name).ok_or_else(|| format!("{name}: no label"))?;
        let local = LocalFile::read(&finish.work, name, label)?;
        match assets.get(name) {
            Some(a) if remote_sha256(gh, a)? == local.sha256 => {
                said.push(format!("{name}: already there as written; kept"));
            }
            Some(a) => {
                actions.push(Action::Delete(a.clone(), "replaced by the merged file"));
                actions.push(Action::Upload(local));
            }
            None => actions.push(Action::Upload(local)),
        }
    }
    let body = files::description(kind, version, &gh.repo, &commit, &finish.notes);
    let (pre, _) = files::publish_flags(kind, version);
    let title = kind.title(version);
    if release.body != body || release.name != title || release.prerelease != pre {
        actions.push(Action::Update(
            json!({ "name": title, "body": body, "prerelease": pre, "draft": true }),
            "set the title and the description (the release notes, then the downloads)",
        ));
    } else {
        said.push("the title and the description are already as written".to_owned());
    }
    Ok(Plan {
        release: Some(release),
        actions,
        said,
    })
}

/// That the draft's `SHA256SUMS` lists exactly the files the release's lists, each with the
/// SHA-256 the draft's file has: every problem, by name.
pub fn check_sums(
    sums: &str,
    wanted: &[String],
    remote: &BTreeMap<String, String>,
) -> Result<(), Vec<String>> {
    let lines = files::parse_sums(sums).map_err(|e| vec![e])?;
    let mut problems = Vec::new();
    for name in wanted {
        match lines.iter().find(|(n, _)| n == name) {
            None => problems.push(format!("SHA256SUMS does not list {name}")),
            Some((_, hash)) if remote.get(name) != Some(hash) => problems.push(format!(
                "SHA256SUMS lists {name} with another SHA-256 than the draft's file has"
            )),
            Some(_) => {}
        }
    }
    for (name, _) in &lines {
        if !wanted.contains(name) {
            problems.push(format!(
                "SHA256SUMS lists {name}, which the release does not carry"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// `--publish`: the finished draft, its staging files removed, published as the workflow's rules
/// say.
pub fn plan_publish(gh: &mut GitHub<'_>, kind: Kind, version: &str) -> Result<Plan, String> {
    let tag = kind.tag(version);
    remote_tag(gh, &tag)?;
    let release = find_draft(gh, &tag)?.ok_or_else(|| {
        format!("there is no draft release for {tag}: upload and finish it first")
    })?;
    let all = gh.assets(release.id)?;
    let assets = sort_assets(kind, version, &all)?;
    let expected = files::expected(kind, version);
    let have: Vec<String> = assets.keys().cloned().collect();
    let mut wanted = expected.release.clone();
    wanted.extend(expected.index.iter().cloned());
    let finish = format!(
        "`cargo xtask publish {} {version} --finish`",
        kind.command_name()
    );
    let lacking = missing(kind, version, &wanted, &have);
    if !lacking.is_empty() {
        return Err(format!(
            "the draft {tag} is not finished; it lacks:\n  {}\nUpload what is missing, then run \
             {finish}",
            lacking.join("\n  ")
        ));
    }
    if release.body.trim().is_empty() || release.body == files::PLACEHOLDER_BODY {
        return Err(format!("the draft {tag} has no notes yet: run {finish}"));
    }
    let mut remote = BTreeMap::new();
    for (name, a) in &assets {
        remote.insert(name.clone(), remote_sha256(gh, a)?);
    }
    let sums = String::from_utf8_lossy(&gh.download(&assets["SHA256SUMS"])?).into_owned();
    check_sums(&sums, &files::summed(kind, version), &remote).map_err(|p| {
        format!(
            "the draft's SHA256SUMS does not match its files (a file was uploaded or replaced \
             after --finish); run {finish} again:\n  {}",
            p.join("\n  ")
        )
    })?;
    let mut actions: Vec<Action> = all
        .iter()
        .filter(|a| !a.complete())
        .map(|a| Action::Delete(a.clone(), "an upload that never finished"))
        .collect();
    actions.extend(
        assets
            .values()
            .filter(|a| expected.staging.contains(&a.name))
            .map(|a| Action::Delete(a.clone(), "a staging file")),
    );
    let (pre, latest) = files::publish_flags(kind, version);
    actions.push(Action::Update(
        json!({ "draft": false, "prerelease": pre, "make_latest": latest }),
        match (pre, latest) {
            (true, _) => "publish it, as a pre-release (never \"Latest\")",
            (false, "true") => "publish it, as the repository's \"Latest\" release",
            _ => "publish it, not as the repository's \"Latest\" release",
        },
    ));
    let kept = assets
        .keys()
        .filter(|n| !expected.staging.contains(n))
        .count();
    Ok(Plan {
        said: vec![format!(
            "the draft {tag} is finished: {kept} files, SHA256SUMS matching them"
        )],
        release: Some(release),
        actions,
    })
}

/// What `--finish` reads from the checkout before it asks GitHub anything.
#[derive(Debug)]
struct FinishContext {
    work: PathBuf,
    commit: String,
    notes: String,
    facts: package::BuildFacts,
}

impl FinishContext {
    fn finish<'a>(&self, gather: &'a dyn Fn(&Path) -> Result<(), String>) -> Finish<'a> {
        Finish {
            work: self.work.clone(),
            commit: self.commit.clone(),
            notes: self.notes.clone(),
            gather,
        }
    }
}

/// The checkout `--finish` runs in: clean, at a commit carrying `version`, with the tags fetched
/// (the notes and the upgrade declaration read them); the notes it writes from there.
fn finish_context(
    ws: &Path,
    kind: Kind,
    version: &str,
    repo: &str,
) -> Result<FinishContext, String> {
    let carried = kind.product().version(ws)?;
    if carried != version {
        return Err(format!(
            "this checkout carries {carried}, not {version}: check out the tag ({})",
            kind.tag(version)
        ));
    }
    let mut facts = package::build_facts(ws, false)?;
    facts.source_url = source_url(repo);
    if kind != Kind::Web {
        let tag = kind.tag(version);
        let local = package::git(
            ws,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/tags/{tag}^{{commit}}"),
            ],
        )
        .map_err(|_| {
            format!("this checkout has no tag {tag}, which the notes read: `git fetch --tags`")
        })?;
        if local != facts.commit {
            return Err(format!(
                "this checkout is at {}, not at {tag} ({local}): `git checkout {tag}`",
                facts.commit
            ));
        }
    }
    let request = notes::Request {
        version: Some(version.to_owned()),
        source_url: Some(source_url(repo)),
        ..notes::Request::default()
    };
    let text = notes::notes(ws, kind.product(), &request)?;
    let base = target_dir().join("publish").join(kind.tag(version));
    std::fs::create_dir_all(&base).map_err(|e| format!("{}: {e}", base.display()))?;
    let notes_path = base.join("release-notes.md");
    std::fs::write(&notes_path, &text).map_err(|e| format!("{}: {e}", notes_path.display()))?;
    println!("the release notes are in {}", notes_path.display());
    Ok(FinishContext {
        work: base.join("files"),
        commit: facts.commit.clone(),
        notes: text,
        facts,
    })
}

/// The merged files over every machine's files in `dir`, written by the product's own index
/// writer: for Dereth and Empyrean `cargo xtask package <product> --gather`'s; for the web client,
/// its bundle scanned again, its manifest checked against it, and `SHA256SUMS`.
fn gather(
    ws: &Path,
    kind: Kind,
    version: &str,
    facts: &package::BuildFacts,
    url: &str,
    dir: &Path,
) -> Result<(), String> {
    match kind {
        Kind::Dereth => launcher::write_index(ws, dir, version, facts),
        Kind::Empyrean => package::write_index(ws, dir, version, facts),
        Kind::Web => {
            let repo = url.trim_start_matches("https://github.com/");
            let zip = web::archive_name(version);
            web::check_bundle(&dir.join(&zip), version)?;
            check_web_manifest(dir, version, repo)?;
            let mut sums = Vec::new();
            for name in [zip.as_str(), web::MANIFEST] {
                sums.push((name.to_owned(), sha256_hex(&read(&dir.join(name))?)));
            }
            std::fs::write(dir.join("SHA256SUMS"), files::sums_text(&sums))
                .map_err(|e| format!("writing SHA256SUMS: {e}"))
        }
    }
}
