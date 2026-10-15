//! `cargo xtask ship`: `main` fast-forwarded to exactly the commit that was preflighted, and every product it changed released (its version commit and tag, as
//! `cargo xtask release` makes them) and pushed. It refuses when the branch preflighted has moved since,
//! here or on the remote: what was checked is what ships.

use std::path::Path;

use super::machine::Machine;
use super::steps::{Ledger, Stage};

fn s(x: &str) -> String {
    x.to_owned()
}

fn git(m: &mut dyn Machine, root: &Path, args: &[&str]) -> Result<String, String> {
    let args: Vec<String> = args.iter().map(|a| s(a)).collect();
    m.run(Some(root), "git", &args, &[])
        .map(|o| o.trim().to_owned())
}

/// Ships what the ledger preflighted: each product's release cut and pushed with `cargo xtask
/// release <product> <version> --push`, with `env` added, in the order the ledger lists them. With
/// `dry_run`, says what it would do and does nothing past the checks.
pub fn ship(
    m: &mut dyn Machine,
    root: &Path,
    remote: &str,
    env: &[(String, String)],
    ledger: &mut Ledger,
    dry_run: bool,
) -> Result<(), String> {
    let commit = match (ledger.stage, &ledger.commit) {
        (Stage::Preflighted, Some(c)) => c.clone(),
        (Stage::Shipped, _) => {
            return Err("this preflight is shipped already; `cargo xtask preflight finish` installs its final builds".into())
        }
        _ => return Err("nothing is preflighted: run `cargo xtask preflight` on the branch to release first".into()),
    };
    let dirty = git(m, root, &["status", "--porcelain", "--untracked-files=no"])?;
    if !dirty.is_empty() {
        return Err(format!("the tree has uncommitted changes:\n{dirty}"));
    }
    let Some(branch) = ledger.branch.clone() else {
        return Err(
            "the preflight does not say which branch it ran on: run `cargo xtask preflight` again"
                .into(),
        );
    };
    git(m, root, &["fetch", remote])?;
    // The branch here, and on the remote when it is there (main on the remote is behind it, as it
    // should be).
    let mut tips = vec![(
        branch.clone(),
        git(m, root, &["rev-parse", &format!("refs/heads/{branch}")])?,
    )];
    if branch != "main" {
        let there = format!("{remote}/{branch}");
        let tip = git(m, root, &["rev-parse", "--verify", "--quiet", &there]).unwrap_or_default();
        if !tip.is_empty() {
            tips.push((there, tip));
        }
    }
    for (name, tip) in tips {
        if tip != commit {
            return Err(format!(
                "{name} has moved since the preflight ({} then, {} now): preflight it again, so that \
                 what ships is what was checked",
                short(&commit),
                short(&tip)
            ));
        }
    }
    let main = git(m, root, &["rev-parse", "main"])?;
    if git(m, root, &["merge-base", "--is-ancestor", "main", &commit]).is_err() {
        return Err(format!(
            "main ({}) is not behind the preflighted commit ({}): it cannot be fast-forwarded",
            short(&main),
            short(&commit)
        ));
    }
    println!(
        "main {} -> {} ({branch}, as preflighted)",
        short(&main),
        short(&commit)
    );
    for r in &ledger.releases {
        println!(
            "  release {} {}: {}",
            r.product.command_name(),
            r.version,
            r.why
        );
    }
    if dry_run {
        println!("A dry run: nothing is changed.");
        return Ok(());
    }
    git(m, root, &["checkout", "main"])?;
    git(m, root, &["merge", "--ff-only", &commit])?;
    for r in ledger.releases.clone() {
        let args = [
            "xtask",
            "release",
            r.product.command_name(),
            &r.version,
            "--push",
            "--remote",
            remote,
        ]
        .map(s);
        let said = m
            .run(Some(root), "cargo", &args, env)
            .map_err(|e| format!("releasing {} {}: {e}", r.product.command_name(), r.version))?;
        print!("{said}");
    }
    if ledger.releases.is_empty() {
        git(m, root, &["push", remote, "main"])?;
    }
    ledger.stage = Stage::Shipped;
    Ok(())
}

fn short(c: &str) -> &str {
    &c[..c.len().min(10)]
}
