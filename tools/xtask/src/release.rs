//! `cargo xtask release empyrean|dereth <version> [--push] [--remote <name>]`: cut a release of
//! Empyrean or of Dereth (the launcher and the client, one version).
//!
//! On a clean `main` it sets the version of every crate that carries the product's version
//! (Empyrean: every `empyrean-*` crate; Dereth: the client and the launcher) to `<version>` and
//! commits that with the lock files (skipped when the tree already carries it), runs
//! `cargo xtask ci tier0` and packages the host's own target as a smoke test, then makes the
//! annotated tag `empyrean-v<version>` or `dereth-v<version>`. An Empyrean release also makes its
//! database upgrade fixture (`empyrean/crates/store/UPGRADES.md`) when the tree has none for that
//! release, and commits it with the version. A final Dereth release turns the Unreleased section of
//! `dereth/CHANGES.md` into the release's own, dated, under a new empty Unreleased section, and
//! commits that with the version: the section is the release notes' highlights ([`notes`]). It
//! prints the two push commands and runs them only with `--push`: the product's release workflow
//! starts when the tag reaches the public repository, and builds, checks and drafts the release
//! from there.

use std::path::Path;

use crate::package::version::Product;
use crate::package::{self, git, version};
use crate::util::{workspace_root, Profile};

pub mod notes;

/// What the release command reads from the repository before it changes anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoState {
    /// The checked-out branch (`HEAD` when detached).
    pub branch: String,
    /// Tracked files with uncommitted changes.
    pub uncommitted: Vec<String>,
    /// The version the `empyrean-*` crates carry now.
    pub current: String,
    /// The `empyrean-v*` tags that exist here.
    pub local_tags: Vec<String>,
    /// The `empyrean-v*` tags on the remote, when it could be asked.
    pub remote_tags: Option<Vec<String>>,
}

/// What the release will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The crates' version must be moved (and the move committed) first.
    pub bump: bool,
    pub tag: String,
}

/// Whether Empyrean `requested` may be released from `state`, and how; every reason it may not.
#[cfg(test)]
pub fn preconditions(requested: &str, state: &RepoState) -> Result<Plan, Vec<String>> {
    preconditions_for(Product::Empyrean, requested, state)
}

/// Whether `product`'s `requested` may be released from `state`, and how; every reason it may
/// not.
pub fn preconditions_for(
    product: Product,
    requested: &str,
    state: &RepoState,
) -> Result<Plan, Vec<String>> {
    let mut problems = Vec::new();
    let wanted = version::parse_release_version(requested).map_err(|e| vec![e])?;
    if state.branch != "main" {
        problems.push(format!(
            "releases are cut from `main`; this checkout is on `{}`",
            state.branch
        ));
    }
    if !state.uncommitted.is_empty() {
        problems.push(format!(
            "the tree has uncommitted changes: {}",
            state.uncommitted.join("; ")
        ));
    }
    let tag = product.tag_for(requested);
    if state.local_tags.contains(&tag) {
        problems.push(format!("the tag {tag} already exists here"));
    }
    if state
        .remote_tags
        .as_ref()
        .is_some_and(|tags| tags.contains(&tag))
    {
        problems.push(format!("the tag {tag} already exists on the remote"));
    }
    let bump = match version::parse_release_version(&state.current) {
        Ok(current) if wanted < current => {
            problems.push(format!(
                "{requested} is older than the version the crates carry ({})",
                state.current
            ));
            false
        }
        Ok(_) => requested != state.current,
        Err(_) => true,
    };
    if problems.is_empty() {
        Ok(Plan { bump, tag })
    } else {
        Err(problems)
    }
}

/// The tags in `git tag -l` or `git ls-remote --tags` output.
pub fn tag_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|l| {
            let name = l.split_whitespace().last()?;
            let name = name.strip_prefix("refs/tags/").unwrap_or(name);
            let name = name.strip_suffix("^{}").unwrap_or(name);
            Some(name.to_owned())
        })
        .collect()
}

/// The commands that publish a tagged release.
pub fn push_commands(remote: &str, tag: &str) -> [Vec<String>; 2] {
    [
        vec!["git".into(), "push".into(), remote.into(), "main".into()],
        vec!["git".into(), "push".into(), remote.into(), tag.into()],
    ]
}

const USAGE: &str =
    "usage: cargo xtask release empyrean|dereth <MAJOR.MINOR.PATCH> [--push] [--remote <name>]";

/// `cargo xtask release ...`.
pub fn release(args: &[String]) -> i32 {
    let (product, requested, push, remote) = match parse(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match run(product, &requested, push, &remote) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\nrelease: STOPPED\n{e}");
            1
        }
    }
}

fn parse(args: &[String]) -> Result<(Product, String, bool, String), String> {
    let mut it = args.iter();
    let product = it
        .next()
        .and_then(|a| Product::from_command_name(a))
        .ok_or(USAGE)?;
    let mut requested = None;
    let mut push = false;
    let mut remote = "origin".to_owned();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--push" => push = true,
            "--remote" => remote = it.next().cloned().ok_or(USAGE)?,
            v if !v.starts_with("--") && requested.is_none() => requested = Some(v.to_owned()),
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    Ok((product, requested.ok_or(USAGE)?, push, remote))
}

fn state(ws: &Path, product: Product, remote: &str) -> Result<RepoState, String> {
    let pattern = format!("{}*", product.tag_prefix());
    let remote_pattern = format!("refs/tags/{pattern}");
    let remote_tags = match git(ws, &["ls-remote", "--tags", remote, &remote_pattern]) {
        Ok(out) => Some(tag_names(&out)),
        Err(e) => {
            println!("note: the remote `{remote}` could not be asked for its tags ({e}); only local tags are checked");
            None
        }
    };
    Ok(RepoState {
        branch: git(ws, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        uncommitted: package::uncommitted(ws)?,
        current: product.version(ws)?,
        local_tags: tag_names(&git(ws, &["tag", "--list", &pattern])?),
        remote_tags,
    })
}

/// The lock files a version change updates, as (the folder cargo runs in, the lock file),
/// relative to the workspace root.
fn locks(product: Product) -> &'static [(&'static str, &'static str)] {
    match product {
        Product::Empyrean => &[("", "Cargo.lock")],
        Product::Dereth => version::DERETH_LOCKS,
    }
}

/// Where Empyrean release `requested` keeps its database upgrade fixture, relative to the
/// workspace: one folder per release, named by its numeric version (`0.2.0-rc.1` is `0.2.0`), as
/// the store's upgrade harness reads them.
pub fn upgrade_fixture_dir(requested: &str) -> String {
    let core = requested.split('-').next().unwrap_or(requested);
    format!("empyrean/crates/store/tests/fixtures/upgrade/{core}")
}

/// For an Empyrean release whose fixture folder does not exist yet: make it with the store's
/// generator (after the version is set, so it is the new release's store that writes it) and stage
/// it. Returns the folder to commit, or `None` when there is nothing to make.
fn make_upgrade_fixture(
    ws: &Path,
    product: Product,
    requested: &str,
) -> Result<Option<String>, String> {
    if product != Product::Empyrean {
        return Ok(None);
    }
    let dir = upgrade_fixture_dir(requested);
    if ws.join(&dir).exists() {
        println!("=== the upgrade fixture {dir} is already there");
        return Ok(None);
    }
    println!("=== making the upgrade fixture {dir}");
    if !crate::util::run(
        ws,
        "cargo",
        &[
            "run",
            "-q",
            "--profile",
            "test-release",
            "-p",
            "empyrean-store",
            "--example",
            "upgrade_fixture",
            "--",
            &dir,
        ],
    ) {
        return Err(format!(
            "the upgrade fixture generator failed for {dir}; nothing is committed"
        ));
    }
    git(ws, &["add", "--", &dir])?;
    Ok(Some(dir))
}

/// The highlights file, as the release command reads and writes it.
fn changes_path(ws: &Path) -> std::path::PathBuf {
    ws.join(notes::CHANGES)
}

/// For a final Dereth release whose highlights file has no section for `requested` yet: stamp
/// the Unreleased section with it and today's date, and return the file to commit. `None` for
/// Empyrean, for a pre-release (its notes show the Unreleased section, which stays for the final
/// release), and when the section is already there.
fn stamp_changes(ws: &Path, product: Product, requested: &str) -> Result<Option<String>, String> {
    if product != Product::Dereth {
        return Ok(None);
    }
    if version::is_prerelease(requested) {
        println!(
            "=== {requested} is a pre-release: {} keeps its Unreleased section",
            notes::CHANGES
        );
        return Ok(None);
    }
    let path = changes_path(ws);
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("reading {}: {e}", notes::CHANGES))?;
    if notes::section(&text, requested).is_some() {
        println!(
            "=== {} already has a section for {requested}",
            notes::CHANGES
        );
        return Ok(None);
    }
    if notes::section(&text, notes::UNRELEASED).is_some_and(|s| s.is_empty()) {
        println!(
            "note: the Unreleased section of {} is empty; the release's notes will have no \
             highlights",
            notes::CHANGES
        );
    }
    let stamped = notes::stamp(&text, requested, &notes::today())?;
    std::fs::write(&path, stamped).map_err(|e| format!("writing {}: {e}", notes::CHANGES))?;
    println!("=== {}: Unreleased is now {requested}", notes::CHANGES);
    Ok(Some(notes::CHANGES.to_owned()))
}

fn run(product: Product, requested: &str, push: bool, remote: &str) -> Result<(), String> {
    let ws = workspace_root();
    let name = product.display_name();
    let state = state(&ws, product, remote)?;
    let plan = preconditions_for(product, requested, &state).map_err(|p| p.join("\n"))?;
    // The upgrade declaration (`empyrean/releases.toml` and the code) is checked before anything
    // changes: a patch release that declares anything is refused here.
    if product == Product::Empyrean {
        package::declaration::for_release(&ws, requested)?;
    }

    if plan.bump {
        println!(
            "=== setting {name}'s crates to {requested} (from {})",
            state.current
        );
        let mut paths = Vec::new();
        for manifest in product.manifests(&ws)? {
            let text = std::fs::read_to_string(&manifest)
                .map_err(|e| format!("reading {}: {e}", manifest.display()))?;
            let edited = version::set_package_version(&text, requested)
                .map_err(|e| format!("{}: {e}", manifest.display()))?;
            std::fs::write(&manifest, edited)
                .map_err(|e| format!("writing {}: {e}", manifest.display()))?;
            paths.push(
                manifest
                    .strip_prefix(&ws)
                    .unwrap_or(&manifest)
                    .display()
                    .to_string()
                    .replace('\\', "/"),
            );
        }
        // Each lock file's entries for its workspace's own packages, and nothing else.
        for (dir, lock) in locks(product) {
            if !crate::util::run(
                &ws.join(dir),
                "cargo",
                &["update", "--workspace", "--offline"],
            ) {
                return Err(format!(
                    "`cargo update --workspace` failed for {lock}; the manifests are edited but \
                     nothing is committed"
                ));
            }
            paths.push((*lock).to_owned());
        }
        let now = product.version(&ws)?;
        if now != requested {
            return Err(format!(
                "after the edit the crates carry {now}, not {requested}"
            ));
        }
        if let Some(dir) = make_upgrade_fixture(&ws, product, requested)? {
            paths.push(dir);
        }
        if let Some(file) = stamp_changes(&ws, product, requested)? {
            paths.push(file);
        }
        let message = format!("{name} {requested}");
        let mut args = vec!["commit", "-m", &message, "--"];
        args.extend(paths.iter().map(String::as_str));
        git(&ws, &args)?;
        println!("committed: {message}");
    } else {
        println!("=== the crates already carry {requested}; nothing to bump");
        if let Some(dir) = make_upgrade_fixture(&ws, product, requested)? {
            let message = format!("{name} {requested}: its upgrade fixture");
            git(&ws, &["commit", "-m", &message, "--", &dir])?;
            println!("committed: {message}");
        }
        if let Some(file) = stamp_changes(&ws, product, requested)? {
            let message = format!("{name} {requested}: its changes");
            git(&ws, &["commit", "-m", &message, "--", &file])?;
            println!("committed: {message}");
        }
    }

    let head = git(&ws, &["rev-parse", "HEAD"])?;
    let undo = if plan.bump {
        "\nThe version commit stays; fix the failure and run this again (it will not bump twice), \
         or drop the commit with `git reset --hard HEAD~1`."
    } else {
        ""
    };
    println!("\n=== cargo xtask ci tier0 on {head}");
    if crate::tier0(Profile::TestRelease) != 0 {
        return Err(format!("tier 0 failed; no tag was made.{undo}"));
    }
    println!("\n=== packaging the host's own target as a smoke test");
    let host = String::from_utf8_lossy(
        &std::process::Command::new("rustc")
            .arg("-vV")
            .output()
            .map_err(|e| format!("rustc: {e}"))?
            .stdout,
    )
    .lines()
    .find_map(|l| l.strip_prefix("host: ").map(str::to_owned))
    .ok_or("rustc -vV named no host")?;
    match product {
        Product::Empyrean => package::build_packages(&ws, &[host], None, false),
        // A macOS host needs MoltenVK for this, named as the package command takes it.
        Product::Dereth => package::dereth::build_packages(&ws, &[host], None, false, None),
    }
    .map_err(|e| format!("packaging failed; no tag was made.{undo}\n{e}"))?;

    let message = format!("{name} {requested}");
    git(&ws, &["tag", "-a", &plan.tag, "-m", &message, &head])?;
    println!("\ntagged {} at {head}", plan.tag);
    if product == Product::Dereth {
        let request = notes::Request {
            version: Some(requested.to_owned()),
            ..notes::Request::default()
        };
        match notes::notes(&ws, &request) {
            Ok(text) => {
                println!("\n=== the release notes the workflow will write from this tag\n\n{text}")
            }
            Err(e) => println!("note: the release notes could not be previewed: {e}"),
        }
    }

    let commands = push_commands(remote, &plan.tag);
    if push {
        for c in &commands {
            let args: Vec<&str> = c[1..].iter().map(String::as_str).collect();
            if !crate::util::run(&ws, "git", &args) {
                return Err(format!("`{}` failed; run it again by hand", c.join(" ")));
            }
        }
        println!(
            "\npushed. When the tag reaches GitHub, the release workflow builds every target and \
             drafts the release; publish the draft after reviewing it (RELEASING.md)."
        );
    } else {
        println!("\nNothing is pushed. To publish, run:");
        for c in &commands {
            println!("    {}", c.join(" "));
        }
        println!(
            "When the tag reaches GitHub, the release workflow builds every target and drafts \
             the release (RELEASING.md). To undo instead: `git tag -d {}`{}.",
            plan.tag,
            if plan.bump {
                " and `git reset --hard HEAD~1`"
            } else {
                ""
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
