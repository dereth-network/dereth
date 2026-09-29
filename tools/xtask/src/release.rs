//! `cargo xtask release empyrean <version> [--push] [--remote <name>]`: cut an Empyrean release.
//!
//! On a clean `main` it sets every `empyrean-*` crate's version to `<version>` and commits that
//! (skipped when the tree already carries it), runs `cargo xtask ci tier0` and packages the host's
//! own target as a smoke test, then makes the annotated tag `empyrean-v<version>`. It prints the
//! two push commands and runs them only with `--push`: the release workflow starts when the tag
//! reaches the public repository, and builds, checks and drafts the release from there.

use std::path::Path;

use crate::package::{self, git, version};
use crate::util::{workspace_root, Profile};

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

/// Whether `requested` may be released from `state`, and how; every reason it may not.
pub fn preconditions(requested: &str, state: &RepoState) -> Result<Plan, Vec<String>> {
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
    let tag = version::tag_for(requested);
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
    "usage: cargo xtask release empyrean <MAJOR.MINOR.PATCH> [--push] [--remote <name>]";

/// `cargo xtask release ...`.
pub fn release(args: &[String]) -> i32 {
    let (requested, push, remote) = match parse(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match run(&requested, push, &remote) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\nrelease: STOPPED\n{e}");
            1
        }
    }
}

fn parse(args: &[String]) -> Result<(String, bool, String), String> {
    let mut it = args.iter();
    if it.next().map(String::as_str) != Some("empyrean") {
        return Err(USAGE.to_owned());
    }
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
    Ok((requested.ok_or(USAGE)?, push, remote))
}

fn state(ws: &Path, remote: &str) -> Result<RepoState, String> {
    let remote_tags = match git(
        ws,
        &["ls-remote", "--tags", remote, "refs/tags/empyrean-v*"],
    ) {
        Ok(out) => Some(tag_names(&out)),
        Err(e) => {
            println!("note: the remote `{remote}` could not be asked for its tags ({e}); only local tags are checked");
            None
        }
    };
    Ok(RepoState {
        branch: git(ws, &["rev-parse", "--abbrev-ref", "HEAD"])?,
        uncommitted: package::uncommitted(ws)?,
        current: version::empyrean_version(ws)?,
        local_tags: tag_names(&git(ws, &["tag", "--list", "empyrean-v*"])?),
        remote_tags,
    })
}

fn run(requested: &str, push: bool, remote: &str) -> Result<(), String> {
    let ws = workspace_root();
    let state = state(&ws, remote)?;
    let plan = preconditions(requested, &state).map_err(|p| p.join("\n"))?;
    // The upgrade declaration (`empyrean/releases.toml` and the code) is checked before anything
    // changes: a patch release that declares anything is refused here.
    package::declaration::for_release(&ws, requested)?;

    if plan.bump {
        println!(
            "=== setting every empyrean-* crate to {requested} (from {})",
            state.current
        );
        let mut paths = Vec::new();
        for manifest in version::empyrean_manifests(&ws)? {
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
        // The lock file's entries for the workspace's own packages, and nothing else.
        if !crate::util::run(&ws, "cargo", &["update", "--workspace", "--offline"]) {
            return Err("`cargo update --workspace` failed; the manifests are edited but nothing is committed".to_owned());
        }
        let now = version::empyrean_version(&ws)?;
        if now != requested {
            return Err(format!(
                "after the edit the crates carry {now}, not {requested}"
            ));
        }
        paths.push("Cargo.lock".to_owned());
        let message = format!("Empyrean {requested}");
        let mut args = vec!["commit", "-m", &message, "--"];
        args.extend(paths.iter().map(String::as_str));
        git(&ws, &args)?;
        println!("committed: {message}");
    } else {
        println!("=== the crates already carry {requested}; nothing to bump");
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
    package::build_packages(&ws, &[host], None, false)
        .map_err(|e| format!("packaging failed; no tag was made.{undo}\n{e}"))?;

    let message = format!("Empyrean {requested}");
    git(&ws, &["tag", "-a", &plan.tag, "-m", &message, &head])?;
    println!("\ntagged {} at {head}", plan.tag);

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
