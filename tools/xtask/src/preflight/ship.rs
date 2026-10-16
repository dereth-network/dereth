//! `cargo xtask ship`: `main` fast-forwarded to exactly the commit that was preflighted, and every
//! product it changed released (its version commit and tag, as `cargo xtask release` makes them)
//! and pushed, each release cut in this process. A Dereth release brings the web client's, which
//! carries Dereth's version: its tag on Dereth's release commit, pushed. It refuses when the branch
//! preflighted has moved since, here or on the remote: what was checked is what ships.

use std::path::Path;

use super::machine::Machine;
use super::plan::Product;
use super::steps::{Ledger, Stage};
use crate::package::version::DERETH_TAG_PREFIX;

/// One product's release cut and pushed, `cargo xtask release <product> <version> --push`'s
/// steps run in this process: the product and the version.
pub type Cut<'a> = dyn FnMut(Product, &str) -> Result<(), String> + 'a;

fn s(x: &str) -> String {
    x.to_owned()
}

fn git(m: &mut dyn Machine, root: &Path, args: &[&str]) -> Result<String, String> {
    let args: Vec<String> = args.iter().map(|a| s(a)).collect();
    m.run(Some(root), "git", &args, &[])
        .map(|o| o.trim().to_owned())
}

/// The web client's release tag for Dereth's `version`.
pub fn web_tag(version: &str) -> String {
    format!("{}{version}", crate::package::web::TAG_PREFIX)
}

/// Ships what the ledger preflighted: each product's release cut and pushed with `cut`, in the
/// order the ledger lists them, and after Dereth's, the web client's tag. With `dry_run`, says
/// what it would do and does nothing past the checks.
pub fn ship(
    m: &mut dyn Machine,
    root: &Path,
    remote: &str,
    cut: &mut Cut<'_>,
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
    // The web client's tag is made after Dereth's release is pushed: one already there would
    // stop the run halfway, so it is asked after first.
    let web = ledger
        .releases
        .iter()
        .find(|r| r.product == Product::Dereth)
        .map(|r| web_tag(&r.version));
    if let Some(tag) = &web {
        let here = git(
            m,
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/tags/{tag}"),
            ],
        )
        .unwrap_or_default();
        let there = git(
            m,
            root,
            &["ls-remote", "--tags", remote, &format!("refs/tags/{tag}")],
        )?;
        if !here.is_empty() || !there.is_empty() {
            return Err(format!(
                "the web client's tag {tag} is there already{}: nothing is shipped",
                if here.is_empty() {
                    " on the remote"
                } else {
                    ""
                }
            ));
        }
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
    if let Some(tag) = &web {
        println!("  and tag {tag} on Dereth's release, for the web client");
    }
    if dry_run {
        println!("A dry run: nothing is changed.");
        return Ok(());
    }
    git(m, root, &["checkout", "main"])?;
    git(m, root, &["merge", "--ff-only", &commit])?;
    for r in ledger.releases.clone() {
        cut(r.product, &r.version)
            .map_err(|e| format!("releasing {} {}: {e}", r.product.command_name(), r.version))?;
        if r.product == Product::Dereth {
            tag_web(m, root, remote, &r.version)
                .map_err(|e| format!("the web client's release {}: {e}", r.version))?;
        }
    }
    if ledger.releases.is_empty() {
        git(m, root, &["push", remote, "main"])?;
    }
    ledger.stage = Stage::Shipped;
    Ok(())
}

/// The web client's release of Dereth's `version`: its tag on the commit Dereth's release tag
/// names, pushed, which starts its release workflow.
fn tag_web(m: &mut dyn Machine, root: &Path, remote: &str, version: &str) -> Result<(), String> {
    let tag = web_tag(version);
    let at = git(
        m,
        root,
        &[
            "rev-parse",
            &format!("{DERETH_TAG_PREFIX}{version}^{{commit}}"),
        ],
    )?;
    git(
        m,
        root,
        &[
            "tag",
            "-a",
            &tag,
            "-m",
            &format!("Dereth (Web) {version}"),
            &at,
        ],
    )?;
    git(m, root, &["push", remote, &tag])?;
    println!("tagged {tag} at {at} and pushed it: its workflow drafts the web client's release");
    Ok(())
}

fn short(c: &str) -> &str {
    &c[..c.len().min(10)]
}
