//! Behaviour: none (tooling: deploying a web client release to Cloudflare Pages, against an
//! in-memory GitHub and stand-in programs).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::*;
use crate::package::archive::Member;
use crate::package::BuildFacts;
use crate::publish::github::{HttpRequest, HttpResponse, Method};

const REPO: &str = "owner/repo";
const PROJECT: &str = "my-pages-project";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-deploy-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// The files of a small bundle: the page, its headers, the module and its bindings.
fn files() -> Vec<Member> {
    [
        ("index.html", b"<!doctype html>".to_vec()),
        ("_headers", b"/*\n  Cache-Control: no-cache\n".to_vec()),
        ("pkg/dereth_web_bg.wasm", b"\0asm\x01\0\0\0".to_vec()),
        ("pkg/dereth_web.js", b"// bindings".to_vec()),
    ]
    .into_iter()
    .map(|(name, bytes)| Member {
        name: name.to_owned(),
        bytes,
        executable: false,
    })
    .collect()
}

/// The archive of `zipped` and a manifest, written by the packaging's own code, that lists
/// `listed` (the same files, unless a test wants them to differ).
fn package(version: &str, zipped: &[Member], listed: &[Member]) -> (Vec<u8>, Vec<u8>) {
    let zip = archive::zip_bytes(&web::archive_root(version), zipped, 1_759_000_000).expect("zip");
    let facts = BuildFacts {
        commit: "abc123".into(),
        branch: "main".into(),
        number: "1".into(),
        epoch: 1_759_000_000,
        source_url: format!("https://github.com/{REPO}"),
    };
    let m = web::manifest(version, &facts, (&web::archive_name(version), &zip), listed);
    (zip, serde_json::to_vec(&m).expect("json"))
}

/// A release's files, by name.
type Files = Vec<(String, Vec<u8>)>;

/// A release's files as the release workflow attaches them.
fn release_files(version: &str, zip: Vec<u8>, manifest: Vec<u8>) -> Files {
    vec![
        (web::archive_name(version), zip),
        (web::MANIFEST.to_owned(), manifest),
        ("SHA256SUMS".to_owned(), b"sums".to_vec()),
    ]
}

/// GitHub's public API for one repository, answered from memory: the release list, each
/// release's files, and their downloads. It answers nothing but reads.
#[derive(Debug, Default)]
struct FakeGitHub {
    releases: Vec<(Value, Files)>,
    asked: Vec<String>,
}

impl FakeGitHub {
    fn with(mut self, tag: &str, draft: bool, files: Files) -> Self {
        let id = self.releases.len() as u64 + 1;
        self.releases.push((
            json!({
                "id": id, "tag_name": tag, "name": tag, "body": "", "draft": draft,
                "prerelease": false, "upload_url": "",
                "html_url": format!("https://github.com/{REPO}/releases/tag/{tag}"),
            }),
            files,
        ));
        self
    }
}

impl Transport for FakeGitHub {
    fn send(&mut self, r: &HttpRequest<'_>) -> Result<HttpResponse, String> {
        assert_eq!(r.method, Method::Get, "a deploy changes nothing on GitHub");
        self.asked.push(r.url.clone());
        let api = format!("https://api.github.com/repos/{REPO}");
        let path = r.url.strip_prefix(&api).expect("the repository's API");
        let answer = |body: Vec<u8>| Ok(HttpResponse { status: 200, body });
        let list = |v: Vec<Value>| answer(serde_json::to_vec(&v).expect("json"));
        // A file's id is its release's id times 100, plus its place.
        if let Some(id) = path.strip_prefix("/releases/assets/") {
            let id: usize = id.parse().expect("an asset id");
            return answer(self.releases[id / 100 - 1].1[id % 100].1.clone());
        }
        if path.starts_with("/releases?") {
            return list(self.releases.iter().map(|(r, _)| r.clone()).collect());
        }
        let id: usize = path
            .strip_prefix("/releases/")
            .and_then(|p| p.split('/').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(|| panic!("an unexpected call: {}", r.url));
        list(
            self.releases[id - 1]
                .1
                .iter()
                .enumerate()
                .map(|(i, (name, bytes))| {
                    json!({
                        "id": id * 100 + i, "name": name, "label": "", "size": bytes.len(),
                        "state": "uploaded",
                    })
                })
                .collect(),
        )
    }
}

/// Programs that are not there unless named, and a record of what was asked and run.
#[derive(Debug, Default)]
struct FakePrograms {
    on_path: Vec<&'static str>,
    probed: Vec<String>,
    ran: Vec<(String, Vec<String>)>,
}

impl Programs for FakePrograms {
    fn probe(&mut self, program: &str, _args: &[String]) -> bool {
        self.probed.push(program.to_owned());
        self.on_path.contains(&program)
    }

    fn run(&mut self, _dir: &Path, program: &str, args: &[String]) -> Result<(), String> {
        let at = args
            .iter()
            .position(|a| a == "deploy")
            .expect("pages deploy");
        let folder = Path::new(&args[at + 1]);
        assert!(
            folder.join("index.html").is_file() && folder.join("_headers").is_file(),
            "wrangler is given the unpacked bundle"
        );
        self.ran.push((program.to_owned(), args.to_vec()));
        Ok(())
    }
}

fn options(source: Source, branch: Option<&str>, dry_run: bool) -> Options {
    Options {
        source,
        project: PROJECT.to_owned(),
        branch: branch.map(str::to_owned),
        dry_run,
    }
}

fn release(id: u64, tag: &str, draft: bool, prerelease: bool) -> Release {
    Release {
        id,
        tag_name: tag.to_owned(),
        name: String::new(),
        body: String::new(),
        draft,
        prerelease,
        upload_url: String::new(),
        html_url: String::new(),
    }
}

/// Newest first, as GitHub lists them.
fn releases() -> Vec<Release> {
    vec![
        release(1, "dereth-web-v0.5.0", true, false),
        release(2, "dereth-web-v0.5.0-rc.1", false, true),
        release(3, "dereth-v0.4.4", false, false),
        release(4, "empyrean-v0.3.2", false, false),
        release(5, "dereth-web-v0.4.4", false, false),
        release(6, "dereth-web-v0.4.3", false, false),
    ]
}

/// `latest` is the first web client release that is published and final: a newer draft, a
/// newer pre-release and the other products' newer releases are passed over.
#[test]
fn latest_is_the_newest_published_web_release_not_a_draft_a_prerelease_or_another_product() {
    let list = releases();
    assert_eq!(pick(&list, &Wanted::Latest, REPO).expect("one").id, 5);
    let none = pick(&list[..4], &Wanted::Latest, REPO).expect_err("no final web release");
    assert!(none.contains("no published web client release"), "{none}");
}

/// A version names `dereth-web-v<version>`, never another product's tag of the same version; a
/// pre-release is taken when named; a draft and a missing release are refused, each saying so.
#[test]
fn a_version_names_its_web_tag_and_a_draft_or_missing_release_is_refused() {
    let list = releases();
    let version = |v: &str| pick(&list, &Wanted::Version(v.to_owned()), REPO);
    assert_eq!(version("0.4.4").expect("published").id, 5);
    assert_eq!(version("0.4.3").expect("published").id, 6);
    assert_eq!(version("0.5.0-rc.1").expect("named").id, 2);
    let draft = version("0.5.0").expect_err("a draft");
    assert!(draft.contains("dereth-web-v0.5.0 is a draft"), "{draft}");
    let missing = version("0.9.0").expect_err("no such release");
    assert!(
        missing.contains("no published release dereth-web-v0.9.0"),
        "{missing}"
    );
}

/// A dry run finds, downloads, checks and unpacks the release, prints the command, and runs no
/// program at all, not even to look for wrangler.
#[test]
fn a_dry_run_downloads_and_checks_the_release_but_runs_no_program() {
    let work = scratch("dry-run");
    let (zip, manifest) = package("0.4.4", &files(), &files());
    let mut gh = FakeGitHub::default()
        .with("dereth-web-v0.5.0", true, Vec::new())
        .with(
            "dereth-web-v0.4.4",
            false,
            release_files("0.4.4", zip, manifest),
        );
    let mut programs = FakePrograms {
        on_path: vec!["wrangler", "wrangler.cmd"],
        ..FakePrograms::default()
    };
    let o = options(Source::Release(Wanted::Latest), None, true);
    run(&o, REPO, &mut gh, &mut programs, &work).expect("a dry run");
    assert!(programs.probed.is_empty() && programs.ran.is_empty());
    assert!(gh.asked.iter().any(|u| u.ends_with("/releases/assets/200")));
    let folder = work.join("dereth-web-0.4.4");
    assert_eq!(
        std::fs::read(folder.join("pkg/dereth_web_bg.wasm")).expect("unpacked"),
        b"\0asm\x01\0\0\0"
    );
    let _ = std::fs::remove_dir_all(&work);
}

/// A deploy gives wrangler the unpacked folder, the project and the branch, and removes the
/// folder afterwards.
#[test]
fn a_deploy_runs_wrangler_pages_deploy_on_the_unpacked_bundle_and_removes_it_after() {
    let work = scratch("real");
    let (zip, manifest) = package("0.4.4", &files(), &files());
    let mut gh = FakeGitHub::default().with(
        "dereth-web-v0.4.4",
        false,
        release_files("0.4.4", zip, manifest),
    );
    let mut programs = FakePrograms {
        on_path: vec!["wrangler", "wrangler.cmd"],
        ..FakePrograms::default()
    };
    let o = options(
        Source::Release(Wanted::Version("0.4.4".into())),
        Some("preview"),
        false,
    );
    run(&o, REPO, &mut gh, &mut programs, &work).expect("deployed");
    let folder = work.join("dereth-web-0.4.4").to_string_lossy().into_owned();
    assert_eq!(programs.ran.len(), 1);
    let (program, args) = &programs.ran[0];
    assert!(program.starts_with("wrangler"), "{program}");
    assert_eq!(
        args,
        &[
            "pages",
            "deploy",
            &folder,
            "--project-name",
            PROJECT,
            "--branch",
            "preview"
        ]
    );
    assert!(!work.exists(), "the unpacked folder is removed");
}

/// The command line: the folder, the project, and the branch only when one is given.
#[test]
fn the_wrangler_command_names_the_folder_and_project_and_the_branch_when_given() {
    let folder = Path::new("bundle");
    assert_eq!(
        wrangler_args(folder, PROJECT, None),
        ["pages", "deploy", "bundle", "--project-name", PROJECT]
    );
    assert_eq!(
        wrangler_args(folder, PROJECT, Some("preview")),
        [
            "pages",
            "deploy",
            "bundle",
            "--project-name",
            PROJECT,
            "--branch",
            "preview"
        ]
    );
}

/// wrangler on the PATH is used as it is, else through npx; with neither, the deploy says so.
/// On Windows both are npm's `.cmd` scripts.
#[test]
fn wrangler_is_found_on_the_path_else_through_npx_and_neither_is_refused() {
    let mut p = FakePrograms {
        on_path: vec!["wrangler", "npx"],
        ..FakePrograms::default()
    };
    assert_eq!(
        find_wrangler(&mut p, false).expect("found"),
        ("wrangler".to_owned(), Vec::new())
    );
    let mut p = FakePrograms {
        on_path: vec!["npx.cmd"],
        ..FakePrograms::default()
    };
    assert_eq!(
        find_wrangler(&mut p, true).expect("found"),
        ("npx.cmd".to_owned(), vec!["wrangler".to_owned()])
    );
    assert_eq!(p.probed, ["wrangler.cmd", "npx.cmd"]);
    let mut p = FakePrograms::default();
    let e = find_wrangler(&mut p, false).expect_err("neither");
    assert!(e.contains("neither wrangler nor npx"), "{e}");
}

/// An archive that is not the one its manifest describes, by its bytes or its size, is refused
/// before anything is unpacked or run.
#[test]
fn an_archive_whose_size_or_sha256_is_not_its_manifest_s_is_refused() {
    let work = scratch("mismatch");
    let (mut zip, manifest) = package("0.4.4", &files(), &files());
    let last = zip.len() - 1;
    zip[last] ^= 1;
    let mut gh = FakeGitHub::default().with(
        "dereth-web-v0.4.4",
        false,
        release_files("0.4.4", zip, manifest),
    );
    let mut programs = FakePrograms::default();
    let o = options(Source::Release(Wanted::Latest), None, false);
    let e = run(&o, REPO, &mut gh, &mut programs, &work).expect_err("tampered");
    assert!(e.contains("a broken download"), "{e}");
    assert!(programs.ran.is_empty());
    assert!(!work.join("dereth-web-0.4.4").exists(), "nothing unpacked");

    let (zip, manifest) = package("0.4.4", &files(), &files());
    let m = web::Manifest::read(&manifest).expect("a manifest");
    assert!(check_archive(&m, &zip).is_ok());
    let longer = [zip.as_slice(), b"\0"].concat();
    assert!(check_archive(&m, &longer).is_err());
    let _ = std::fs::remove_dir_all(&work);
}

/// Deploys a local package from `dir` and returns why it was refused.
fn refused(dir: &Path, zipped: &[Member], listed: &[Member]) -> String {
    let (zip, manifest) = package("0.4.4", zipped, listed);
    std::fs::write(dir.join("dereth-web-0.4.4.zip"), zip).expect("zip");
    std::fs::write(dir.join("web.json"), manifest).expect("manifest");
    let o = options(
        Source::Zip(dir.join("dereth-web-0.4.4.zip"), None),
        None,
        false,
    );
    let mut programs = FakePrograms {
        on_path: vec!["wrangler", "wrangler.cmd"],
        ..FakePrograms::default()
    };
    let e = run(
        &o,
        REPO,
        &mut FakeGitHub::default(),
        &mut programs,
        &dir.join("work"),
    )
    .expect_err("refused");
    assert!(programs.ran.is_empty());
    e
}

/// Unpacked, the bundle must hold every file its manifest lists, the page, the headers and the
/// module among them, and no file over the 25 MiB Cloudflare Pages serves as one file.
#[test]
fn a_bundle_missing_a_file_or_holding_one_over_25_mib_is_refused_after_unpacking() {
    let dir = scratch("unpacked");
    let mut extra = files();
    extra.push(Member {
        name: "play.js".into(),
        bytes: b"// play".to_vec(),
        executable: false,
    });
    let e = refused(&dir, &files(), &extra);
    assert!(e.contains("play.js: on the allowlist but missing"), "{e}");

    let no_headers: Vec<Member> = files()
        .into_iter()
        .filter(|m| m.name != "_headers")
        .collect();
    let e = refused(&dir, &no_headers, &no_headers);
    assert!(e.contains("_headers: not in the bundle"), "{e}");

    let mut big = files();
    big.push(Member {
        name: "pkg/horizon/atlas-3-0.png".into(),
        bytes: vec![0; (25 << 20) + 1],
        executable: false,
    });
    let e = refused(&dir, &big, &big);
    assert!(
        e.contains("pkg/horizon/atlas-3-0.png: ") && e.contains("cap for one file"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--zip` deploys a local package against the `web.json` beside it, and a version given with
/// it must be the package's.
#[test]
fn a_local_package_is_checked_against_the_manifest_beside_it() {
    let dir = scratch("local");
    let (zip, manifest) = package("0.4.4", &files(), &files());
    let file = dir.join("dereth-web-0.4.4.zip");
    std::fs::write(&file, zip).expect("zip");
    std::fs::write(dir.join("web.json"), manifest).expect("manifest");
    let mut programs = FakePrograms {
        on_path: vec!["npx", "npx.cmd"],
        ..FakePrograms::default()
    };
    let o = options(Source::Zip(file.clone(), Some("0.4.4".into())), None, false);
    run(
        &o,
        REPO,
        &mut FakeGitHub::default(),
        &mut programs,
        &dir.join("work"),
    )
    .expect("deployed");
    assert_eq!(programs.ran.len(), 1);
    assert_eq!(programs.ran[0].1[..3], ["wrangler", "pages", "deploy"]);
    let o = options(Source::Zip(file, Some("0.4.3".into())), None, false);
    let e = run(
        &o,
        REPO,
        &mut FakeGitHub::default(),
        &mut programs,
        &dir.join("work"),
    )
    .expect_err("another version");
    assert!(e.contains("is the package of 0.4.4, not 0.4.3"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The project is `--project`, else the environment's; with neither the deploy is refused, and
/// the message says how to give one.
#[test]
fn the_project_comes_from_the_flag_else_the_environment_and_none_is_refused() {
    let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let env = |k: &str| (k == PROJECT_VAR).then(|| "env-project".to_owned());
    let none = |_: &str| None;
    let o = parse(&args("web latest --project flag-project"), &env).expect("parses");
    assert_eq!(o.project, "flag-project");
    let o = parse(&args("web latest"), &env).expect("parses");
    assert_eq!(o.project, "env-project");
    let e = parse(&args("web latest"), &none).expect_err("no project");
    assert!(
        e.contains("--project <name>") && e.contains(PROJECT_VAR),
        "{e}"
    );
    let e = parse(&args("web latest --project Not_A_Name"), &none).expect_err("a bad name");
    assert!(e.contains("not a Pages project name"), "{e}");
}

#[test]
fn the_options_read_as_given() {
    let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    let env = |_: &str| Some(PROJECT.to_owned());
    let o = parse(&args("web 0.4.4 --branch preview --dry-run"), &env).expect("parses");
    assert_eq!(
        o,
        Options {
            source: Source::Release(Wanted::Version("0.4.4".into())),
            project: PROJECT.into(),
            branch: Some("preview".into()),
            dry_run: true,
        }
    );
    let o = parse(&args("web --zip d/x.zip"), &env).expect("parses");
    assert_eq!(o.source, Source::Zip(PathBuf::from("d/x.zip"), None));
    assert!(!o.dry_run && o.branch.is_none());
    assert!(parse(&args("web"), &env).is_err(), "no release named");
    assert!(parse(&args("web latest --zip x.zip"), &env).is_err());
    assert!(parse(&args("web notaversion"), &env).is_err());
    assert!(parse(&args("web latest --branch -x"), &env).is_err());
    assert!(parse(&args("web latest --yes"), &env).is_err());
    assert!(parse(&args("empyrean latest"), &env).is_err());
}
