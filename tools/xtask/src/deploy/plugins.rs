//! `cargo xtask deploy plugins`: Dereth's own plugin source put on its Cloudflare Pages project, the
//! host of `https://plugins.dereth.network/plugins.json`.
//!
//! ```text
//! cargo xtask deploy plugins [--project <name>] [--force] [--dry-run]
//! ```
//!
//! 1. **The tree.** The source is built from the checkout as it is, so the checkout must be a
//!    commit: no uncommitted change to a tracked file. That commit must be one `main` has (`HEAD`
//!    an ancestor of `origin/main`; fetch first when it was pushed from elsewhere), so what is
//!    deployed is what anyone can build from it, unless **`--force`** deploys a commit of a
//!    branch, to try plugins before they reach `main`.
//! 2. **The source**, as `cargo xtask package plugins` writes it ([`package::plugins::source`]):
//!    `plugins.json`, and each plugin's zip at `<id>/<id>-<version>-<hash>.zip`.
//! 3. **`_headers`** beside them ([`HEADERS`]): every file readable from any page
//!    (`Access-Control-Allow-Origin: *`, which the web client needs, since it reads the source
//!    from another host), the list kept five minutes, and each zip, whose name carries its
//!    version and hash and so never changes, kept a year as immutable.
//! 4. **Deployed** with `wrangler pages deploy <folder> --project-name <project>`, as `deploy web`
//!    deploys. The project is `--project`, else [`PROJECT_VAR`]; with neither the deploy is
//!    refused.
//!
//! Plugins are experimental: nothing deploys this until the owner does. The credentials are
//! wrangler's, as for `deploy web`. **`--dry-run`** does the first three steps and prints the
//! command it would run, and keeps the folder for a look; for a commit `main` lacks it says the
//! deploy needs `--force`, and goes on.

use std::path::Path;

use crate::package;

use super::{check_project, find_wrangler, wrangler_args, Programs, Real};

pub const USAGE: &str =
    "usage: cargo xtask deploy plugins [--project <name>] [--force] [--dry-run]";

/// The variable the Pages project is read from when `--project` is not given.
pub const PROJECT_VAR: &str = "DERETH_PLUGINS_PAGES_PROJECT";

/// The host's headers for the source: every file readable from any page, the list short-lived,
/// the versioned zips immutable.
pub const HEADERS: &str = "\
/*
  Access-Control-Allow-Origin: *
  X-Content-Type-Options: nosniff
/plugins.json
  Cache-Control: public, max-age=300
/:id/*
  Cache-Control: public, max-age=31536000, immutable
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    project: String,
    /// Deploy a commit `main` does not have.
    force: bool,
    dry_run: bool,
}

/// The options of `deploy plugins ...`, the project read through `env` when no flag gives it.
fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Options, String> {
    let mut it = args.iter();
    let (mut project, mut force, mut dry_run) = (None, false, false);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--project" => {
                project = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| format!("--project needs a value\n{USAGE}"))?,
                );
            }
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    let project = project
        .or_else(|| env(PROJECT_VAR))
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            format!(
                "name the Cloudflare Pages project: --project <name>, or set {PROJECT_VAR} \
                 (for example {PROJECT_VAR}=dereth-plugins)"
            )
        })?;
    check_project(&project)?;
    Ok(Options {
        project,
        force,
        dry_run,
    })
}

/// The commit the source is built from, when the checkout is one: nothing tracked changed. With
/// it, whether `main` has it (`HEAD` an ancestor of `origin/main`). `git` runs git in the
/// checkout.
fn tree_ready(git: &dyn Fn(&[&str]) -> Result<String, String>) -> Result<(String, bool), String> {
    let changed = git(&["status", "--porcelain", "--untracked-files=no"])?;
    if !changed.trim().is_empty() {
        return Err(format!(
            "the checkout has uncommitted changes; the source is built from a commit main has:\n{}",
            changed.trim_end()
        ));
    }
    let head = git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let on_main = git(&["merge-base", "--is-ancestor", "HEAD", "origin/main"]).is_ok();
    Ok((head, on_main))
}

/// What to say of a commit `main` lacks: refused unless forced, and a dry run goes on.
fn off_main(o: &Options, head: &str) -> Result<String, String> {
    if o.force {
        Ok(format!(
            "deploy: {head} is not on origin/main; deploying it anyway (--force)"
        ))
    } else if o.dry_run {
        Ok(format!(
            "deploy: {head} is not on origin/main; deploying it would need --force"
        ))
    } else {
        Err(format!(
            "{head} is not on origin/main: deploy a commit main has (push it, or `git fetch` if \
             it was pushed from elsewhere), or a branch's with --force"
        ))
    }
}

/// Write the source's `files` (path on the host, bytes) and [`HEADERS`] into `folder`, made
/// afresh.
fn stage(folder: &Path, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    match std::fs::remove_dir_all(folder) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(format!("{}: {e}", folder.display()))
        }
        _ => {}
    }
    let mut all: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    all.push(("_headers", HEADERS.as_bytes()));
    for (path, bytes) in all {
        let to = folder.join(path);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&to, bytes).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    Ok(())
}

/// `cargo xtask deploy plugins ...`.
pub fn deploy(args: &[String]) -> i32 {
    let o = match parse(args, &|k| std::env::var(k).ok()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = crate::util::workspace_root();
    let work = std::env::temp_dir().join(format!("dereth-plugins-deploy-{}", std::process::id()));
    let git = |args: &[&str]| package::git(&ws, args);
    let source = || package::plugins::source(&ws);
    match run(&o, &git, &source, &mut Real, &work) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\ndeploy: STOPPED\n{e}");
            1
        }
    }
}

/// One deploy: the checkout checked, the source built and staged in `work` with its headers, then
/// given to wrangler unless this is a dry run.
fn run(
    o: &Options,
    git: &dyn Fn(&[&str]) -> Result<String, String>,
    source: &dyn Fn() -> Result<Vec<(String, Vec<u8>)>, String>,
    programs: &mut dyn Programs,
    work: &Path,
) -> Result<(), String> {
    let (commit, on_main) = tree_ready(git)?;
    if !on_main {
        println!("{}", off_main(o, &commit)?);
    }
    let files = source()?;
    let folder = work.join("plugins");
    stage(&folder, &files)?;
    println!(
        "deploy: Dereth's plugin source at {commit}: {} plugins in {}",
        files.len().saturating_sub(1),
        folder.display()
    );
    let args = wrangler_args(&folder, &o.project, None);
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
    let _ = std::fs::remove_dir_all(work);
    deployed?;
    println!(
        "deploy: Dereth's plugin source is deployed to the Pages project {}, for {}",
        o.project,
        package::plugins::URL
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: deploying Dereth's plugin source, over a fake git and wrangler).
    use super::*;

    #[derive(Default)]
    struct Recorder {
        ran: Vec<(String, Vec<String>)>,
        staged: Vec<String>,
    }

    impl Programs for Recorder {
        fn probe(&mut self, program: &str, _args: &[String]) -> bool {
            program.starts_with("wrangler")
        }

        fn run(&mut self, _dir: &Path, program: &str, args: &[String]) -> Result<(), String> {
            let at = args
                .iter()
                .position(|a| a == "deploy")
                .expect("pages deploy");
            let folder = Path::new(&args[at + 1]);
            for f in [
                "plugins.json",
                "_headers",
                "network.dereth.meter/network.dereth.meter-1.0.0.zip",
            ] {
                if folder.join(f).is_file() {
                    self.staged.push(f.to_owned());
                }
            }
            self.ran.push((program.to_owned(), args.to_vec()));
            Ok(())
        }
    }

    fn source() -> Result<Vec<(String, Vec<u8>)>, String> {
        Ok(vec![
            (
                "network.dereth.meter/network.dereth.meter-1.0.0.zip".to_owned(),
                b"zip".to_vec(),
            ),
            ("plugins.json".to_owned(), b"{}".to_vec()),
        ])
    }

    /// A git that answers as a clean checkout on `main` does, or as one with `changed` files, or
    /// one `HEAD` main does not have.
    fn git(changed: &'static str, on_main: bool) -> impl Fn(&[&str]) -> Result<String, String> {
        move |args: &[&str]| match args.first().copied() {
            Some("status") => Ok(changed.to_owned()),
            Some("rev-parse") => Ok("c0ffee".to_owned()),
            Some("merge-base") if on_main => Ok(String::new()),
            Some("merge-base") => Err("not an ancestor".to_owned()),
            other => Err(format!("unexpected git {other:?}")),
        }
    }

    fn work(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("xtask-deploy-plugins-{tag}-{}", std::process::id()))
    }

    #[test]
    fn the_project_comes_from_the_flag_or_the_variable() {
        let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        let none = |_: &str| None;
        let set = |k: &str| (k == PROJECT_VAR).then(|| "dereth-plugins".to_owned());
        assert_eq!(
            parse(&args("--project mine --dry-run"), &none),
            Ok(Options {
                project: "mine".into(),
                force: false,
                dry_run: true
            })
        );
        assert!(parse(&args("--project mine --force"), &none).is_ok_and(|o| o.force && !o.dry_run));
        assert_eq!(
            parse(&[], &set).map(|o| o.project).as_deref(),
            Ok("dereth-plugins")
        );
        assert!(parse(&[], &none).is_err_and(|e| e.contains(PROJECT_VAR)));
        assert!(parse(&args("--project Not_A_Project"), &none).is_err());
        assert!(parse(&args("--branch x"), &set).is_err());
    }

    /// A clean checkout on `main` is built, staged with its headers and handed to wrangler; a dry
    /// run hands nothing over and keeps the folder.
    #[test]
    fn a_checkout_main_has_is_staged_with_its_headers_and_deployed() {
        let dir = work("deploy");
        let o = Options {
            project: "dereth-plugins".into(),
            force: false,
            dry_run: false,
        };
        let mut programs = Recorder::default();
        run(&o, &git("", true), &source, &mut programs, &dir).expect("deployed");
        assert_eq!(programs.ran.len(), 1);
        let (program, args) = &programs.ran[0];
        assert!(program.starts_with("wrangler"), "{program}");
        assert_eq!(args[..2], ["pages", "deploy"]);
        assert_eq!(args[3..], ["--project-name", "dereth-plugins"]);
        assert_eq!(
            programs.staged,
            [
                "plugins.json",
                "_headers",
                "network.dereth.meter/network.dereth.meter-1.0.0.zip"
            ]
        );
        assert!(
            !dir.exists(),
            "the staged folder is removed after the deploy"
        );

        let dry = Options { dry_run: true, ..o };
        let mut programs = Recorder::default();
        run(&dry, &git("", true), &source, &mut programs, &dir).expect("a dry run");
        assert!(programs.ran.is_empty());
        let headers = std::fs::read_to_string(dir.join("plugins").join("_headers")).expect("kept");
        assert_eq!(headers, HEADERS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A checkout with uncommitted changes, or whose `HEAD` main does not have, deploys nothing.
    #[test]
    fn a_dirty_or_unpushed_checkout_is_refused() {
        let dir = work("refused");
        let o = Options {
            project: "dereth-plugins".into(),
            force: false,
            dry_run: false,
        };
        let mut programs = Recorder::default();
        let dirty = run(
            &o,
            &git(" M dereth/plugins/xp-meter/main.lua", true),
            &source,
            &mut programs,
            &dir,
        );
        assert!(dirty.is_err_and(|e| e.contains("uncommitted")));
        let unpushed = run(&o, &git("", false), &source, &mut programs, &dir);
        assert!(unpushed.is_err_and(|e| e.contains("not on origin/main") && e.contains("--force")));
        let forced = Options { force: true, ..o };
        let dirty = run(
            &forced,
            &git(" M dereth/plugins/xp-meter/main.lua", false),
            &source,
            &mut programs,
            &dir,
        );
        assert!(
            dirty.is_err_and(|e| e.contains("uncommitted")),
            "--force deploys a commit, never uncommitted changes"
        );
        assert!(programs.ran.is_empty() && !dir.exists());
    }

    /// `--force` deploys a branch's commit; a dry run of one goes on and says it would need it.
    #[test]
    fn a_branch_is_deployed_only_when_forced() {
        let dir = work("forced");
        let o = Options {
            project: "dereth-plugins".into(),
            force: true,
            dry_run: false,
        };
        let mut programs = Recorder::default();
        run(&o, &git("", false), &source, &mut programs, &dir).expect("deployed");
        assert_eq!(programs.ran.len(), 1);
        assert_eq!(programs.staged.len(), 3);

        let dry = Options {
            force: false,
            dry_run: true,
            ..o
        };
        assert!(off_main(&dry, "c0ffee").is_ok_and(|m| m.contains("would need --force")));
        let mut programs = Recorder::default();
        run(&dry, &git("", false), &source, &mut programs, &dir).expect("a dry run");
        assert!(programs.ran.is_empty());
        assert!(dir.join("plugins").join("plugins.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The headers make every file readable from any page, keep the list a short while and each
    /// versioned zip a year, as immutable.
    #[test]
    fn the_headers_open_every_file_to_any_page_and_cache_the_zips_for_good() {
        let rules: Vec<(&str, Vec<&str>)> =
            HEADERS
                .split('\n')
                .filter(|l| !l.is_empty())
                .fold(Vec::new(), |mut rules, l| {
                    if l.starts_with(' ') {
                        if let Some((_, lines)) = rules.last_mut() {
                            lines.push(l.trim());
                        }
                    } else {
                        rules.push((l, Vec::new()));
                    }
                    rules
                });
        assert_eq!(
            rules[0],
            (
                "/*",
                vec![
                    "Access-Control-Allow-Origin: *",
                    "X-Content-Type-Options: nosniff"
                ]
            )
        );
        assert_eq!(
            rules[1],
            ("/plugins.json", vec!["Cache-Control: public, max-age=300"])
        );
        assert_eq!(
            rules[2],
            (
                "/:id/*",
                vec!["Cache-Control: public, max-age=31536000, immutable"]
            )
        );
        // No file gets one header from two rules: Cloudflare joins the values of a header two
        // matching rules set.
        let matches = |rule: &str, path: &str| match rule {
            "/*" => true,
            "/:id/*" => path.matches('/').count() >= 2,
            exact => exact == path,
        };
        for path in [
            "/plugins.json",
            "/network.dereth.tank/network.dereth.tank-0.3.0.zip",
        ] {
            let mut names: Vec<&str> = rules
                .iter()
                .filter(|(r, _)| matches(r, path))
                .flat_map(|(_, l)| l.iter().map(|h| h.split(':').next().unwrap_or_default()))
                .collect();
            let all = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), all, "{path}: {names:?}");
            assert!(
                names.contains(&"Cache-Control") && names.contains(&"Access-Control-Allow-Origin")
            );
        }
    }
}
