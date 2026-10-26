//! `cargo xtask ship`: CI waited for on exactly the commit that was preflighted (which the
//! preflight pushed to the remote's `preflight` branch, for the mirror to carry to GitHub), `main`
//! fast-forwarded to it, and every product it changed released (its version commit and tag, as
//! `cargo xtask release` makes them, tier 0 left out) and pushed, each release cut in this process.
//! A Dereth release brings the web client's, which carries Dereth's version: its tag on Dereth's
//! release commit, pushed. It refuses when the branch preflighted has moved since, here or on the
//! remote, and when a check failed: what was checked is what ships. Each step is recorded in the
//! ledger as it is done, so a ship that stopped carries on from the next release, with `main` past
//! the preflighted commit by its own release commits and nothing else.

use std::path::Path;

use super::ci;
use super::config::Config;
use super::machine::Machine;
use super::plan::Product;
use super::steps::{Ledger, Stage};
use crate::package::version::DERETH_TAG_PREFIX;

/// How a ship is run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    /// `--dry-run`: says what it would do, and does nothing past the checks.
    pub dry_run: bool,
    /// `--skip-checks`: CI is not asked.
    pub skip_checks: bool,
    /// `--allow-failing`: ships despite failed checks, naming each.
    pub allow_failing: bool,
    /// `--local-tier0`: tier 0 run here too, on the commit, before anything moves.
    pub local_tier0: bool,
}

/// One product's release cut and pushed, `cargo xtask release <product> <version> --push`'s
/// steps run in this process, tier 0 left out: the product and the version.
pub type Cut<'a> = dyn FnMut(Product, &str) -> Result<(), String> + 'a;

/// Tier 0 run on the checkout, its output shown and kept in the file given: whether it passed, or
/// why it could not run.
pub type Tier0<'a> = dyn FnMut(&Path) -> Result<bool, String> + 'a;

/// Tier 0's output, in the work folder.
pub const TIER0_LOG: &str = "tier0.log";

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

/// The name `cargo xtask release` gives `product`'s release commits, and its tags' prefix.
fn release_names(product: Product) -> (&'static str, &'static str) {
    use crate::package::version::Product as Released;
    use crate::release::network::Part;
    let name = product.command_name();
    match (
        Released::from_command_name(name),
        Part::from_command_name(name),
    ) {
        (Some(p), _) => (p.display_name(), p.tag_prefix()),
        (None, Some(p)) => (p.display_name(), p.tag_prefix()),
        (None, None) => (name, name),
    }
}

/// The commands that send each tag the ship has made and pushed to `remote` again, on its own,
/// each deleted and pushed again there for the mirror to carry to GitHub, so that its release
/// workflow starts: GitHub makes no tag events for a push that carries more than three tags.
/// Nothing here pushes to GitHub itself. `None` when it has made none.
pub fn tag_resends(ledger: &Ledger, remote: &str) -> Option<String> {
    let mut tags = Vec::new();
    for r in &ledger.releases {
        if !ledger.shipped.released.contains(&r.product) {
            continue;
        }
        tags.push(format!("{}{}", release_names(r.product).1, r.version));
        if r.product == Product::Dereth && ledger.shipped.web {
            tags.push(web_tag(&r.version));
        }
    }
    let lines: Vec<String> = tags
        .iter()
        .map(|t| format!("git push {remote} :refs/tags/{t}\ngit push {remote} refs/tags/{t}"))
        .collect();
    (!lines.is_empty()).then(|| {
        format!(
            "If GitHub did not start the release workflows (it makes no tag events when one push \
             carries more than three tags), send each tag again on its own: run the mirror after \
             each command, so that it carries each tag to GitHub alone:\n{}",
            lines.join("\n")
        )
    })
}

/// Whether every commit on `main` past `left` (where this ship's last step left it) is the next
/// release's own, left by its cut when it stopped partway: named `<Product> <version>`, or
/// `<Product> <version>: ...` as releases once named their changes' and fixtures' commits. Its cut
/// takes them up again; it does not bump twice.
fn own_commits(
    m: &mut dyn Machine,
    root: &Path,
    ledger: &Ledger,
    left: &str,
) -> Result<bool, String> {
    let Some(next) = ledger.releases.iter().find(|r| !ledger.released(r.product)) else {
        return Ok(false);
    };
    if git(m, root, &["merge-base", "--is-ancestor", left, "main"]).is_err() {
        return Ok(false);
    }
    let subjects = git(m, root, &["log", "--format=%s", &format!("{left}..main")])?;
    let named = format!("{} {}", release_names(next.product).0, next.version);
    let own = |s: &str| s == named || s.starts_with(&format!("{named}: "));
    Ok(!subjects.is_empty() && subjects.lines().all(own))
}

/// The tests a test run's output names as failing: each `failures:` list's names.
pub fn failing_tests(output: &str) -> Vec<String> {
    let lines: Vec<&str> = output.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim_end() == "failures:" {
            out.extend(
                lines[i + 1..]
                    .iter()
                    .take_while(|n| n.starts_with("    ") && !n.trim().is_empty())
                    .map(|n| n.trim().to_owned()),
            );
        }
    }
    out
}

/// Ships what the ledger preflighted: CI on the commit waited for and taken
/// ([`ci::await_checks`]), tier 0 run on it with `--local-tier0`, then each product's release cut
/// and pushed with `cut`, in the order the ledger lists them, and after Dereth's, the web
/// client's tag. What a ship that stopped did already is not done again, and once `main` has
/// moved, its checks are not asked again. With `--dry-run`, says what it would do and does nothing
/// past the checks, asking CI once and not waiting for it.
pub fn ship(
    m: &mut dyn Machine,
    cfg: &Config,
    root: &Path,
    cut: &mut Cut<'_>,
    tier0: &mut Tier0<'_>,
    ledger: &mut Ledger,
    opts: &Options,
) -> Result<(), String> {
    let dry_run = opts.dry_run;
    let remote = cfg.remote.as_str();
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
    // A ship that stopped left `main` where its last step put it: past the preflighted commit by
    // its own release commits, and nothing else.
    let shipped_main = ledger.shipped.main.clone();
    // The branch here, and on the remote when it is there (main on the remote is behind it, as it
    // should be). A preflight of `main` itself finds it where the ship left it.
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
        // `main` itself, when it is the branch, is checked below with what this ship put on it.
        if name == "main" && shipped_main.is_some() {
            continue;
        }
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
    match &shipped_main {
        Some(left) if main != *left && !own_commits(m, root, ledger, left)? => {
            return Err(format!(
                "main has moved since this ship stopped ({} then, {} now): what is on it now was \
                 not preflighted",
                short(left),
                short(&main)
            ))
        }
        Some(_) => {}
        None => {
            if git(m, root, &["merge-base", "--is-ancestor", "main", &commit]).is_err() {
                return Err(format!(
                    "main ({}) is not behind the preflighted commit ({}): it cannot be \
                     fast-forwarded",
                    short(&main),
                    short(&commit)
                ));
            }
        }
    }
    // The web client's tag is made after Dereth's release is pushed: one already there would
    // stop the run halfway, so it is asked after first.
    let web = ledger
        .releases
        .iter()
        .find(|r| r.product == Product::Dereth)
        .map(|r| web_tag(&r.version))
        .filter(|_| !ledger.shipped.web);
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
                "the web client's tag {tag} is there already{}: nothing more is shipped",
                if here.is_empty() {
                    " on the remote"
                } else {
                    ""
                }
            ));
        }
    }
    match &shipped_main {
        Some(_) => println!(
            "main {} ({branch}, as preflighted, and this ship's releases so far)",
            short(&main)
        ),
        None => println!(
            "main {} -> {} ({branch}, as preflighted)",
            short(&main),
            short(&commit)
        ),
    }
    for r in &ledger.releases {
        println!(
            "  release {} {}: {}{}",
            r.product.command_name(),
            r.version,
            r.why,
            if ledger.released(r.product) {
                " (released already)"
            } else {
                ""
            }
        );
    }
    if let Some(tag) = &web {
        println!("  and tag {tag} on Dereth's release, for the web client");
    }
    // CI on the commit, before anything moves. A ship that stopped after `main` moved is past it.
    if shipped_main.is_none() {
        println!("{}", checks(m, cfg, ledger, &commit, opts)?);
    }
    let local_tier0 = opts.local_tier0
        && shipped_main.is_none()
        && ledger.shipped.tier0.as_ref() != Some(&commit);
    if dry_run {
        if local_tier0 {
            println!("would run tier 0 here on {commit} (--local-tier0)");
        }
        println!("A dry run: nothing is changed.");
        return Ok(());
    }
    // Tier 0 here, with `--local-tier0`, once, on the commit itself, before anything moves; the
    // releases leave it out. A ship that stopped after it passed does not run it again: the checks
    // above found the tree as it was then, but for this ship's own release commits.
    if local_tier0 {
        git(m, root, &["checkout", &branch])?;
        let log = cfg.work_dir.join(TIER0_LOG);
        println!(
            "\n=== cargo xtask ci tier0 on {commit} (its output is kept in {})",
            log.display()
        );
        if !tier0(&log).map_err(|e| format!("tier 0 could not run: {e}"))? {
            let output = m.read(&log).unwrap_or_default();
            let tests = failing_tests(&output);
            let named = if tests.is_empty() {
                String::new()
            } else {
                format!("\nThe tests that failed:\n    {}", tests.join("\n    "))
            };
            return Err(format!(
                "tier 0 failed on {}; nothing is shipped.{named}\nIts whole output is in {}",
                short(&commit),
                log.display()
            ));
        }
        ledger.shipped.tier0 = Some(commit.clone());
        ledger.save(m, &cfg.work_dir)?;
    }
    git(m, root, &["checkout", "main"])?;
    if shipped_main.is_none() {
        git(m, root, &["merge", "--ff-only", &commit])?;
        ledger.shipped.main = Some(git(m, root, &["rev-parse", "main"])?);
        ledger.save(m, &cfg.work_dir)?;
    }
    for r in ledger.releases.clone() {
        if !ledger.released(r.product) {
            cut(r.product, &r.version).map_err(|e| {
                format!(
                    "releasing {} {}: {e}\n`cargo xtask ship` carries on from it once that is fixed.",
                    r.product.command_name(),
                    r.version
                )
            })?;
            ledger.shipped.released.push(r.product);
            ledger.shipped.main = Some(git(m, root, &["rev-parse", "main"])?);
            ledger.save(m, &cfg.work_dir)?;
        }
        if r.product == Product::Dereth && !ledger.shipped.web {
            tag_web(m, root, remote, &r.version)
                .map_err(|e| format!("the web client's release {}: {e}", r.version))?;
            ledger.shipped.web = true;
            ledger.save(m, &cfg.work_dir)?;
        }
    }
    if ledger.releases.is_empty() {
        git(m, root, &["push", remote, "main"])?;
    }
    ledger.stage = Stage::Shipped;
    Ok(())
}

/// CI on `commit`, the preflighted one, as `opts` takes it: not asked with `--skip-checks`;
/// otherwise waited for ([`ci::await_checks`]), and refused when the preflight did not push the
/// commit for CI to run on. What to say of it.
pub fn checks(
    m: &mut dyn Machine,
    cfg: &Config,
    ledger: &Ledger,
    commit: &str,
    opts: &Options,
) -> Result<String, String> {
    if opts.skip_checks {
        return Ok("CI is not asked (--skip-checks).".to_owned());
    }
    if ledger.pushed.as_deref() != Some(commit) {
        return Err(format!(
            "the preflight did not push {} to {}'s {} branch, so no CI ran on it for this ship: \
             run `cargo xtask preflight` again, or ship with `--skip-checks` (and \
             `--local-tier0` for tier 0 here)",
            short(commit),
            cfg.remote,
            ci::BRANCH
        ));
    }
    ci::await_checks(
        m,
        &cfg.github_repo,
        commit,
        opts.allow_failing,
        opts.dry_run,
        &mut |l| println!("{l}"),
    )
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
