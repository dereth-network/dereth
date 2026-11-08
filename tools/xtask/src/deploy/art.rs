//! `cargo xtask deploy art`: Dereth's own art pack source put on its Cloudflare Pages project, the
//! host of `https://art.dereth.network/art.json`.
//!
//! ```text
//! cargo xtask deploy art [--packs <dir>] [--guide] [--project <name>] [--force] [--uploaded]
//!                        [--dry-run]
//! ```
//!
//! 1. **The tree.** As `deploy plugins`: no uncommitted change to a tracked file, and a commit
//!    `main` has unless **`--force`**. The tree gives the guide, the tools and the source's config
//!    (`dereth/art/source.toml`); the packs come from the packs folder (`--packs`, else
//!    `DERETH_ART_PACKS`).
//! 2. **The source**, as `cargo xtask package art` writes it ([`package::art`]): `art.json`, each
//!    pack's file at `<id>/<id>-<version>-<hash>.dat`, and with `--guide` (or `guide = true` in
//!    the config) the guide as the front page, or with the config's `front` a `_redirects`
//!    sending the front page there.
//! 3. **`_headers`** beside them ([`HEADERS`]): every file readable from any page, the list kept
//!    five minutes, each pack's file, whose name carries its version and hash, kept a year as
//!    immutable.
//! 4. **Deployed** with `wrangler pages deploy <folder> --project-name <project>`. The project is
//!    `--project`, else [`PROJECT_VAR`]; with neither the deploy is refused.
//!
//! A pack larger than a pages host serves is listed under the source's `files_url` and is not on
//! the Pages project: it is uploaded there by hand first, and the deploy is refused until
//! **`--uploaded`** says it has been. **`--dry-run`** does the first three steps, prints the
//! command it would run and what is to be uploaded, and keeps the folder for a look.

use std::path::{Path, PathBuf};

use crate::package;

use super::{check_project, find_wrangler, wrangler_args, Programs, Real};

pub const USAGE: &str =
    "usage: cargo xtask deploy art [--packs <dir>] [--guide] [--project <name>] \
                         [--force] [--uploaded] [--dry-run]";

/// The variable the Pages project is read from when `--project` is not given.
pub const PROJECT_VAR: &str = "DERETH_ART_PAGES_PROJECT";

/// The host's headers for the source: every file readable from any page, the list short-lived,
/// the versioned pack files immutable.
pub const HEADERS: &str = "\
/*
  Access-Control-Allow-Origin: *
  X-Content-Type-Options: nosniff
/art.json
  Cache-Control: public, max-age=300
/:id/*
  Cache-Control: public, max-age=31536000, immutable
";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    project: String,
    packs: Option<PathBuf>,
    guide: bool,
    /// Deploy a commit `main` does not have.
    force: bool,
    /// The packs too large for the pages host are where the list names them.
    uploaded: bool,
    dry_run: bool,
}

/// The options of `deploy art ...`, the project read through `env` when no flag gives it.
fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Options, String> {
    let mut it = args.iter();
    let mut o = Options {
        project: String::new(),
        packs: None,
        guide: false,
        force: false,
        uploaded: false,
        dry_run: false,
    };
    let mut project = None;
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--project" => project = Some(value()?),
            "--packs" => o.packs = Some(PathBuf::from(value()?)),
            "--guide" => o.guide = true,
            "--force" => o.force = true,
            "--uploaded" => o.uploaded = true,
            "--dry-run" => o.dry_run = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    o.project = project
        .or_else(|| env(PROJECT_VAR))
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            format!(
                "name the Cloudflare Pages project: --project <name>, or set {PROJECT_VAR} \
                 (for example {PROJECT_VAR}=dereth-art)"
            )
        })?;
    check_project(&o.project)?;
    Ok(o)
}

/// The commit the source is built from, when the checkout is one: nothing tracked changed. With
/// it, whether `main` has it.
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

/// `cargo xtask deploy art ...`.
pub fn deploy(args: &[String]) -> i32 {
    let env = |k: &str| std::env::var(k).ok();
    let o = match parse(args, &env) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let packs = match package::art::packs_dir(o.packs.clone(), &env) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let ws = crate::util::workspace_root();
    let work = std::env::temp_dir().join(format!("dereth-art-deploy-{}", std::process::id()));
    let git = |args: &[&str]| package::git(&ws, args);
    let build = |out: &Path| package::art::build(&ws, &packs, out, o.guide);
    match run(&o, &git, &build, &mut Real, &work) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("\ndeploy: STOPPED\n{e}");
            1
        }
    }
}

/// One deploy: the checkout checked, the source built into `work` by `build` with its headers,
/// then given to wrangler unless this is a dry run.
fn run(
    o: &Options,
    git: &dyn Fn(&[&str]) -> Result<String, String>,
    build: &dyn Fn(&Path) -> Result<package::art::Written, String>,
    programs: &mut dyn Programs,
    work: &Path,
) -> Result<(), String> {
    let (commit, on_main) = tree_ready(git)?;
    if !on_main {
        println!("{}", off_main(o, &commit)?);
    }
    let written = build(work)?;
    let folder = &written.site;
    std::fs::write(folder.join("_headers"), HEADERS)
        .map_err(|e| format!("{}: {e}", folder.display()))?;
    println!("deploy: at {commit}: {}", package::art::said(&written));
    let waiting = !written.source.upload.is_empty();
    let args = wrangler_args(folder, &o.project, None);
    if o.dry_run {
        println!(
            "deploy: a dry run; it would run\n  wrangler {}\n(or `npx wrangler` when wrangler is \
             not on the PATH). The files stay in {}",
            args.join(" "),
            folder.display()
        );
        if waiting && !o.uploaded {
            println!("deploy: the packs to upload first would need --uploaded");
        }
        return Ok(());
    }
    if waiting && !o.uploaded {
        return Err(format!(
            "the list names packs at {} that a pages host cannot serve; upload them there from {} \
             at their paths, then deploy with --uploaded",
            written.source.files_url,
            written.upload.display()
        ));
    }
    let (program, mut argv) = find_wrangler(programs, cfg!(windows))?;
    argv.extend(args);
    println!("$ {program} {}", argv.join(" "));
    let deployed = programs.run(work, &program, &argv);
    let _ = std::fs::remove_dir_all(work);
    deployed?;
    println!(
        "deploy: Dereth's art pack source is deployed to the Pages project {}, for {}",
        o.project,
        package::art::URL
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: deploying Dereth's art pack source, over a fake git and wrangler).
    use super::*;
    use crate::package::art::{self, Placed, Source, Written};

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
            for f in ["art.json", "_headers", "a.b/a.b-1.0.0-0123456789ab.dat"] {
                if folder.join(f).is_file() {
                    self.staged.push(f.to_owned());
                }
            }
            self.ran.push((program.to_owned(), args.to_vec()));
            Ok(())
        }
    }

    /// A build that writes a list and one pack's file into its folder, and names `upload` packs
    /// to be uploaded.
    fn build(upload: usize) -> impl Fn(&Path) -> Result<Written, String> {
        move |out: &Path| {
            let site = out.join("site");
            let pack = site.join("a.b");
            std::fs::create_dir_all(&pack).map_err(|e| e.to_string())?;
            std::fs::write(site.join("art.json"), "{}").map_err(|e| e.to_string())?;
            std::fs::write(pack.join("a.b-1.0.0-0123456789ab.dat"), "pack")
                .map_err(|e| e.to_string())?;
            let placed = |path: &str| Placed {
                path: path.to_owned(),
                from: PathBuf::new(),
                size: 4,
            };
            Ok(Written {
                site,
                upload: out.join("upload"),
                source: Source {
                    list: "{}".into(),
                    site: vec![placed("a.b/a.b-1.0.0-0123456789ab.dat")],
                    upload: (0..upload).map(|_| placed("c.d/c.d-1.0.0-x.dat")).collect(),
                    files_url: "https://files.example.org/".into(),
                    held_back: Vec::new(),
                    guide: false,
                    front: String::new(),
                },
                guide: false,
            })
        }
    }

    fn git(changed: &'static str, on_main: bool) -> impl Fn(&[&str]) -> Result<String, String> {
        move |args: &[&str]| match args.first().copied() {
            Some("status") => Ok(changed.to_owned()),
            Some("rev-parse") => Ok("c0ffee".to_owned()),
            Some("merge-base") if on_main => Ok(String::new()),
            Some("merge-base") => Err("not an ancestor".to_owned()),
            other => Err(format!("unexpected git {other:?}")),
        }
    }

    fn work(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("xtask-deploy-art-{tag}-{}", std::process::id()))
    }

    fn options() -> Options {
        Options {
            project: "dereth-art".into(),
            packs: None,
            guide: false,
            force: false,
            uploaded: false,
            dry_run: false,
        }
    }

    #[test]
    fn the_project_comes_from_the_flag_or_the_variable() {
        let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        let none = |_: &str| None;
        let set = |k: &str| (k == PROJECT_VAR).then(|| "dereth-art".to_owned());
        let o = parse(
            &args("--project mine --packs /p --guide --uploaded --dry-run"),
            &none,
        )
        .expect("reads");
        assert_eq!(
            o,
            Options {
                project: "mine".into(),
                packs: Some(PathBuf::from("/p")),
                guide: true,
                force: false,
                uploaded: true,
                dry_run: true,
            }
        );
        assert_eq!(
            parse(&[], &set).map(|o| o.project).as_deref(),
            Ok("dereth-art")
        );
        assert!(parse(&[], &none).is_err_and(|e| e.contains(PROJECT_VAR)));
        assert!(parse(&args("--project Not_A_Project"), &none).is_err());
        assert!(parse(&args("--branch x"), &set).is_err());
    }

    /// A clean checkout on `main` is built, given its headers and handed to wrangler; a dry run
    /// hands nothing over and keeps the folder.
    #[test]
    fn a_checkout_main_has_is_staged_with_its_headers_and_deployed() {
        let dir = work("deploy");
        let mut programs = Recorder::default();
        run(&options(), &git("", true), &build(0), &mut programs, &dir).expect("deployed");
        assert_eq!(programs.ran.len(), 1);
        let (program, args) = &programs.ran[0];
        assert!(program.starts_with("wrangler"), "{program}");
        assert_eq!(args[..2], ["pages", "deploy"]);
        assert_eq!(args[3..], ["--project-name", "dereth-art"]);
        assert_eq!(
            programs.staged,
            ["art.json", "_headers", "a.b/a.b-1.0.0-0123456789ab.dat"]
        );
        assert!(!dir.exists(), "the folder is removed after the deploy");

        let dry = Options {
            dry_run: true,
            ..options()
        };
        let mut programs = Recorder::default();
        run(&dry, &git("", false), &build(1), &mut programs, &dir).expect("a dry run");
        assert!(programs.ran.is_empty());
        let headers = std::fs::read_to_string(dir.join("site").join("_headers")).expect("kept");
        assert_eq!(headers, HEADERS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dirty or unpushed checkout deploys nothing, and nor does a list naming packs to upload
    /// until `--uploaded` says they are there.
    #[test]
    fn a_dirty_or_unpushed_checkout_or_packs_not_uploaded_are_refused() {
        let dir = work("refused");
        let mut programs = Recorder::default();
        let dirty = run(
            &options(),
            &git(" M docs/art/README.md", true),
            &build(0),
            &mut programs,
            &dir,
        );
        assert!(dirty.is_err_and(|e| e.contains("uncommitted")));
        let unpushed = run(&options(), &git("", false), &build(0), &mut programs, &dir);
        assert!(unpushed.is_err_and(|e| e.contains("--force")));
        let waiting = run(&options(), &git("", true), &build(1), &mut programs, &dir);
        assert!(waiting.is_err_and(|e| e.contains("--uploaded") && e.contains("files.example.org")));
        assert!(programs.ran.is_empty());
        let uploaded = Options {
            uploaded: true,
            ..options()
        };
        run(&uploaded, &git("", true), &build(1), &mut programs, &dir).expect("deployed");
        assert_eq!(programs.ran.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The real build of a packs folder deploys as a dry run: the list, the file and the headers.
    #[test]
    fn a_folder_of_packs_is_built_and_staged_for_a_dry_run() {
        let dir = work("real");
        let packs = dir.join("packs");
        std::fs::create_dir_all(&packs).expect("made");
        let bytes = art::tests::packed_bytes(
            "[pack]\nname = \"A\"\nformat = 1\nid = \"com.example.a\"\nversion = \"1.0.0\"\n",
            "h",
        );
        std::fs::write(packs.join("a.dat"), &bytes).expect("written");
        let ws = crate::util::workspace_root();
        let built = |out: &Path| art::build(&ws, &packs, out, false);
        let dry = Options {
            dry_run: true,
            ..options()
        };
        let out = dir.join("work");
        run(&dry, &git("", true), &built, &mut Recorder::default(), &out).expect("a dry run");
        let site = out.join("site");
        assert!(site.join("art.json").is_file() && site.join("_headers").is_file());
        let list = std::fs::read(site.join("art.json")).expect("the list");
        let entry = dereth_art_catalog::Catalog::parse(&list)
            .expect("reads")
            .catalog
            .packs
            .remove(0);
        assert_eq!(
            std::fs::read(site.join(&entry.file)).expect("its file"),
            bytes
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_headers_open_every_file_to_any_page_and_cache_the_packs_for_good() {
        assert!(HEADERS.starts_with("/*\n  Access-Control-Allow-Origin: *\n"));
        assert!(HEADERS.contains("/art.json\n  Cache-Control: public, max-age=300\n"));
        assert!(HEADERS.contains("/:id/*\n  Cache-Control: public, max-age=31536000, immutable\n"));
    }
}
