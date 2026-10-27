//! The author's tool run as a person runs it: from the repository's folder, naming the plugin's
//! folder relative to it.
//!
//! Behaviour: none (tooling: the plugin author's tool).

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder of its own for each test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("dereth-plugin-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("a scratch folder");
        Self(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The tool, started in `dir`.
fn tool(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_dereth-plugin"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("the tool runs")
}

/// A new plugin repository at `repo`, committed, with an identity of its own and its hooks in
/// `hooks`.
fn repository(repo: &Path, hooks: &str) {
    let made = tool(
        repo.parent().expect("a parent"),
        &["new", &repo.display().to_string(), "--name", "XP Tracker"],
    );
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    git(repo, &["init", "--quiet"]);
    for (k, v) in [
        ("user.name", "Author"),
        ("user.email", "author@example.org"),
        ("commit.gpgsign", "false"),
        ("tag.gpgsign", "false"),
        ("core.autocrlf", "false"),
        ("core.hooksPath", hooks),
    ] {
        git(repo, &["config", k, v]);
    }
    git(repo, &["add", "--all"]);
    git(repo, &["commit", "--quiet", "-m", "start"]);
}

/// From the repository's folder, `release xp-tracker 0.1.0` commits and tags the release.
#[test]
fn a_release_named_from_the_repositorys_folder_is_committed_and_tagged() {
    let dir = Scratch::new("relative");
    let repo = dir.0.join("xp-tracker");
    repository(&repo, ".git/no-hooks");
    let out = tool(&repo, &["release", "xp-tracker", "0.1.0"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s"]),
        "XP Tracker 0.1.0"
    );
    assert_eq!(git(&repo, &["tag", "--list"]), "v0.1.0");
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
}

/// A release that git refuses to commit leaves the plugin's files as they were, so it can be run
/// again once what stopped it is fixed.
#[test]
fn a_release_git_refuses_leaves_the_files_as_they_were() {
    let dir = Scratch::new("refused");
    let repo = dir.0.join("xp-tracker");
    repository(&repo, ".git/refuse");
    let hooks = repo.join(".git").join("refuse");
    std::fs::create_dir_all(&hooks).expect("the hooks folder");
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").expect("the hook");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let plugin = repo.join("xp-tracker");
    let manifest = std::fs::read(plugin.join("plugin.toml")).expect("plugin.toml");
    let changes = std::fs::read(plugin.join("CHANGES.md")).expect("CHANGES.md");
    // A version the manifest doesn't have yet, so the release has something to put back.
    let out = tool(&repo, &["release", "xp-tracker", "0.2.0"]);
    assert!(!out.status.success(), "the hook refuses the commit");
    assert_eq!(
        std::fs::read(plugin.join("plugin.toml")).expect("plugin.toml"),
        manifest
    );
    assert_eq!(
        std::fs::read(plugin.join("CHANGES.md")).expect("CHANGES.md"),
        changes
    );
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
    assert!(git(&repo, &["tag", "--list"]).is_empty());
}
