//! Behaviour: none (tooling: publishing a release from local builds, against an in-memory GitHub).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::files::{self, Kind};
use super::github::{GitHub, HttpRequest, HttpResponse, Method, Transport};
use super::*;
use crate::package::archive::{self, Member};
use crate::package::targets::{self, Os};

const REPO: &str = "owner/repo";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

// ---------------------------------------------------------------- an in-memory GitHub

#[derive(Debug, Clone)]
struct FakeAsset {
    id: u64,
    name: String,
    label: String,
    state: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct FakeRelease {
    id: u64,
    tag: String,
    name: String,
    body: String,
    draft: bool,
    prerelease: bool,
    make_latest: Option<String>,
    assets: Vec<FakeAsset>,
}

/// The parts of GitHub's REST API `publish` calls, answered from memory, with every call that
/// changes anything recorded.
#[derive(Debug, Default)]
struct FakeGitHub {
    /// tag -> the commit it names.
    tags: BTreeMap<String, String>,
    /// The commits `main` holds.
    main: Vec<String>,
    releases: Vec<FakeRelease>,
    next_id: u64,
    /// Every call sent that changes anything, as `METHOD path`.
    writes: Vec<String>,
    /// Whether assets carry GitHub's `digest` (they do since 2025; older ones do not).
    no_digests: bool,
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).expect("hex");
            out.push(u8::from_str_radix(hex, 16).expect("hex"));
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).expect("utf-8")
}

impl FakeGitHub {
    fn with_tag(tag: &str) -> Self {
        let mut f = Self {
            next_id: 1,
            ..Self::default()
        };
        f.tags.insert(tag.to_owned(), COMMIT.to_owned());
        f.main.push(COMMIT.to_owned());
        f
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn release_json(r: &FakeRelease) -> Value {
        json!({
            "id": r.id, "tag_name": r.tag, "name": r.name, "body": r.body, "draft": r.draft,
            "prerelease": r.prerelease,
            "upload_url": format!("https://uploads.github.com/repos/{REPO}/releases/{}/assets{{?name,label}}", r.id),
            "html_url": format!("https://github.com/{REPO}/releases/{}", r.id),
        })
    }

    fn asset_json(&self, a: &FakeAsset) -> Value {
        let mut v = json!({
            "id": a.id, "name": a.name, "label": a.label, "size": a.bytes.len(), "state": a.state,
        });
        if !self.no_digests {
            v["digest"] = json!(format!("sha256:{}", sha256_hex(&a.bytes)));
        }
        v
    }

    fn release(&self, tag: &str) -> &FakeRelease {
        self.releases
            .iter()
            .find(|r| r.tag == tag)
            .expect("a release for the tag")
    }

    fn asset_names(&self, tag: &str) -> Vec<String> {
        let mut n: Vec<String> = self
            .release(tag)
            .assets
            .iter()
            .map(|a| a.name.clone())
            .collect();
        n.sort();
        n
    }

    fn asset_bytes(&self, tag: &str, name: &str) -> Vec<u8> {
        self.release(tag)
            .assets
            .iter()
            .find(|a| a.name == name)
            .expect("the asset")
            .bytes
            .clone()
    }
}

fn ok(status: u16, v: &Value) -> Result<HttpResponse, String> {
    Ok(HttpResponse {
        status,
        body: serde_json::to_vec(v).expect("json"),
    })
}

impl Transport for FakeGitHub {
    fn send(&mut self, r: &HttpRequest<'_>) -> Result<HttpResponse, String> {
        let upload_prefix = format!("https://uploads.github.com/repos/{REPO}/releases/");
        let api_prefix = format!("https://api.github.com/repos/{REPO}");
        if r.method.writes() {
            let shown = r.url.split('?').next().unwrap_or_default().to_owned();
            self.writes.push(format!("{} {shown}", r.method.name()));
        }
        if let Some(rest) = r.url.strip_prefix(&upload_prefix) {
            assert_eq!(r.method, Method::Post);
            let (id, query) = rest.split_once("/assets?").expect("an upload address");
            let id: u64 = id.parse().expect("an id");
            let mut name = String::new();
            let mut label = String::new();
            for pair in query.split('&') {
                let (k, v) = pair.split_once('=').expect("k=v");
                match k {
                    "name" => name = percent_decode(v),
                    "label" => label = percent_decode(v),
                    _ => {}
                }
            }
            let new_id = self.id();
            let release = self
                .releases
                .iter_mut()
                .find(|x| x.id == id)
                .expect("release");
            if release.assets.iter().any(|a| a.name == name) {
                return ok(422, &json!({"message": "already_exists"}));
            }
            let asset = FakeAsset {
                id: new_id,
                name,
                label,
                state: "uploaded".to_owned(),
                bytes: r.body.expect("a body").to_vec(),
            };
            release.assets.push(asset.clone());
            let v = self.asset_json(&asset);
            return ok(201, &v);
        }
        let path = r.url.strip_prefix(&api_prefix).expect("this repository");
        let (path, _query) = path.split_once('?').unwrap_or((path, ""));
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        match (r.method, parts.as_slice()) {
            (Method::Get, []) => ok(200, &json!({"full_name": REPO})),
            (Method::Get, ["git", "ref", "tags", tag]) => match self.tags.get(&percent_decode(tag))
            {
                Some(sha) => ok(200, &json!({"object": {"type": "commit", "sha": sha}})),
                None => ok(404, &json!({"message": "Not Found"})),
            },
            (Method::Get, ["compare", range]) => {
                let (sha, _) = range.split_once("...").expect("a range");
                let status = if self.main.iter().any(|c| c == sha) {
                    "ahead"
                } else {
                    "diverged"
                };
                ok(200, &json!({ "status": status }))
            }
            (Method::Get, ["releases"]) => {
                let page = if r.url.contains("page=1") {
                    self.releases.iter().map(Self::release_json).collect()
                } else {
                    Vec::new()
                };
                ok(200, &Value::Array(page))
            }
            (Method::Get, ["releases", "assets", id]) => {
                let id: u64 = id.parse().expect("id");
                let a = self
                    .releases
                    .iter()
                    .flat_map(|x| &x.assets)
                    .find(|a| a.id == id)
                    .expect("asset");
                assert_eq!(r.accept, "application/octet-stream");
                Ok(HttpResponse {
                    status: 200,
                    body: a.bytes.clone(),
                })
            }
            (Method::Get, ["releases", id, "assets"]) => {
                let id: u64 = id.parse().expect("id");
                let list: Vec<Value> = if r.url.contains("page=1") {
                    let assets = self
                        .releases
                        .iter()
                        .find(|x| x.id == id)
                        .expect("release")
                        .assets
                        .clone();
                    assets.iter().map(|a| self.asset_json(a)).collect()
                } else {
                    Vec::new()
                };
                ok(200, &Value::Array(list))
            }
            (Method::Post, ["releases"]) => {
                let body: Value = serde_json::from_slice(r.body.expect("body")).expect("json");
                let id = self.id();
                let release = FakeRelease {
                    id,
                    tag: body["tag_name"].as_str().expect("tag").to_owned(),
                    name: body["name"].as_str().unwrap_or_default().to_owned(),
                    body: body["body"].as_str().unwrap_or_default().to_owned(),
                    draft: body["draft"].as_bool().unwrap_or(false),
                    prerelease: body["prerelease"].as_bool().unwrap_or(false),
                    make_latest: None,
                    assets: Vec::new(),
                };
                let v = Self::release_json(&release);
                self.releases.push(release);
                ok(201, &v)
            }
            (Method::Patch, ["releases", id]) => {
                let id: u64 = id.parse().expect("id");
                let body: Value = serde_json::from_slice(r.body.expect("body")).expect("json");
                let release = self
                    .releases
                    .iter_mut()
                    .find(|x| x.id == id)
                    .expect("release");
                if let Some(v) = body["name"].as_str() {
                    release.name = v.to_owned();
                }
                if let Some(v) = body["body"].as_str() {
                    release.body = v.to_owned();
                }
                if let Some(v) = body["draft"].as_bool() {
                    release.draft = v;
                }
                if let Some(v) = body["prerelease"].as_bool() {
                    release.prerelease = v;
                }
                if let Some(v) = body["make_latest"].as_str() {
                    release.make_latest = Some(v.to_owned());
                }
                let v = Self::release_json(release);
                ok(200, &v)
            }
            (Method::Delete, ["releases", "assets", id]) => {
                let id: u64 = id.parse().expect("id");
                for x in &mut self.releases {
                    x.assets.retain(|a| a.id != id);
                }
                Ok(HttpResponse {
                    status: 204,
                    body: Vec::new(),
                })
            }
            other => panic!("the fake GitHub has no {other:?}"),
        }
    }
}

// ---------------------------------------------------------------- helpers

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-publish-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp directory");
    dir
}

fn write(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).expect("write");
}

/// An upload of the files written into `dir`, built from `COMMIT`.
fn upload_of(dir: &Path, kind: Kind, version: &str, names: &[&str]) -> Upload {
    let files = names
        .iter()
        .map(|n| {
            LocalFile::read(dir, n, files::label(kind, version, n).expect("a label")).expect("read")
        })
        .collect();
    Upload {
        files,
        commit: COMMIT.to_owned(),
    }
}

fn run_upload(fake: &mut FakeGitHub, kind: Kind, version: &str, up: &Upload) -> Result<(), String> {
    let mut gh = GitHub::new(fake, REPO, true);
    let plan = plan_upload(&mut gh, kind, version, up)?;
    apply(&mut gh, plan).map(|_| ())
}

/// One Empyrean archive of `triple` holding its allowlist, and its lines of `MANIFEST.txt`, written
/// into `dir` with a `release.json` naming the commit, as `cargo xtask package empyrean` writes them.
fn empyrean_machine(dir: &Path, version: &str, triples: &[&str], build: &str) -> Vec<String> {
    let mut names = Vec::new();
    for triple in triples {
        let t = targets::find(triple).expect("a target");
        let members: Vec<Member> = crate::package::allowlist(t)
            .into_iter()
            .map(|name| Member {
                bytes: format!("{name} for {triple}, build {build}").into_bytes(),
                executable: name.starts_with("empyrean-"),
                name,
            })
            .collect();
        let root = t.archive_root(version);
        let bytes = if t.os == Os::Windows {
            archive::zip_bytes(&root, &members, 1_700_000_000).expect("zip")
        } else {
            archive::tar_gz_bytes(&root, &members, 1_700_000_000).expect("tar.gz")
        };
        let file = t.archive_name(version);
        write(dir, &file, &bytes);
        let part: String = members
            .iter()
            .map(|m| {
                format!(
                    "{triple}\t{}\t{}\t{}\t\n",
                    m.name,
                    m.bytes.len(),
                    sha256_hex(&m.bytes)
                )
            })
            .collect();
        write(dir, &format!("{file}.manifest"), part.as_bytes());
        names.push(file);
    }
    let facts = json!({
        "version": version, "commit": COMMIT, "source_url": source_url(REPO),
    });
    write(dir, "release.json", facts.to_string().as_bytes());
    // This machine's own index, which the upload leaves behind.
    write(dir, "SHA256SUMS", b"this machine's alone\n");
    write(dir, "MANIFEST.txt", b"this machine's alone\n");
    names
}

fn empyrean_version() -> String {
    crate::package::version::empyrean_version(&crate::util::workspace_root()).expect("a version")
}

fn facts() -> crate::package::BuildFacts {
    crate::package::BuildFacts {
        commit: COMMIT.to_owned(),
        branch: "main".to_owned(),
        number: "100".to_owned(),
        epoch: 1_700_000_000,
        source_url: source_url(REPO),
    }
}

// ---------------------------------------------------------------- the upload

/// The first upload creates the draft for the tag with its files; a second upload of the same
/// files sends nothing, and another machine's upload adds only its own files.
#[test]
fn a_file_already_on_the_draft_with_the_same_sha256_is_skipped() {
    let v = "0.2.0";
    let tag = Kind::Web.tag(v);
    let dir = temp("dedup");
    write(&dir, &web::archive_name(v), b"bundle");
    write(&dir, web::MANIFEST, b"{}");
    let up = upload_of(&dir, Kind::Web, v, &[&web::archive_name(v), web::MANIFEST]);
    let mut fake = FakeGitHub::with_tag(&tag);
    run_upload(&mut fake, Kind::Web, v, &up).expect("the first upload");
    assert!(fake.release(&tag).draft, "the release is a draft");
    assert_eq!(fake.release(&tag).name, "Dereth 0.2.0 for the web");
    assert_eq!(fake.asset_names(&tag), ["dereth-web-0.2.0.zip", "web.json"]);
    assert_eq!(
        fake.release(&tag).assets[0].label,
        "Dereth · web client (static files)"
    );
    let first = fake.writes.len();
    assert_eq!(first, 3, "{:?}", fake.writes);

    // The same files again: nothing is sent, digests or none.
    fake.no_digests = true;
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let plan = plan_upload(&mut gh, Kind::Web, v, &up).expect("the same upload");
    assert!(plan.actions.is_empty(), "{:?}", plan.actions);
    assert_eq!(fake.writes.len(), first);
}

/// A file the draft already has under the same name with other bytes refuses the whole upload:
/// nothing of it is sent, not even the files that would have been new.
#[test]
fn a_same_named_file_with_other_bytes_refuses_the_whole_upload() {
    let v = "0.2.0";
    let tag = Kind::Empyrean.tag(v);
    let ws = crate::util::workspace_root();
    let one = temp("conflict-one");
    let names = empyrean_machine(&one, v, &["x86_64-pc-windows-msvc"], "one");
    let first = read_upload_dir(&ws, Kind::Empyrean, v, REPO, &one).expect("reads");
    let mut fake = FakeGitHub::with_tag(&tag);
    run_upload(&mut fake, Kind::Empyrean, v, &first).expect("the first upload");
    let writes = fake.writes.len();

    // Another machine's build of the same target, with other bytes, and a new target.
    let two = temp("conflict-two");
    empyrean_machine(
        &two,
        v,
        &["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"],
        "two",
    );
    let other = read_upload_dir(&ws, Kind::Empyrean, v, REPO, &two).expect("reads");
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let e = plan_upload(&mut gh, Kind::Empyrean, v, &other).expect_err("refused");
    assert!(e.contains(&names[0]) && e.contains("other bytes"), "{e}");
    assert_eq!(fake.writes.len(), writes, "nothing more was sent");
}

/// What to do with each file, from what the draft has under its name.
#[test]
fn each_file_is_uploaded_skipped_retried_or_refused_by_what_the_draft_has() {
    assert_eq!(decide("aa", None), Decision::Upload);
    assert_eq!(decide("aa", Some((true, "aa"))), Decision::Skip);
    assert_eq!(decide("aa", Some((false, ""))), Decision::Retry);
    assert_eq!(
        decide("aa", Some((true, "bb"))),
        Decision::Refuse("bb".to_owned())
    );
}

/// A tag GitHub does not have refuses every step before anything is sent: publishing never makes
/// or pushes a tag.
#[test]
fn a_tag_missing_on_github_refuses_the_upload_and_every_later_step() {
    let v = "0.2.0";
    let dir = temp("no-tag");
    write(&dir, &web::archive_name(v), b"bundle");
    write(&dir, web::MANIFEST, b"{}");
    let up = upload_of(&dir, Kind::Web, v, &[&web::archive_name(v), web::MANIFEST]);
    let mut fake = FakeGitHub::with_tag("dereth-web-v0.1.0");
    let e = run_upload(&mut fake, Kind::Web, v, &up).expect_err("refused");
    assert!(
        e.contains("dereth-web-v0.2.0 is not on owner/repo") && e.contains("never makes"),
        "{e}"
    );
    let mut gh = GitHub::new(&mut fake, REPO, true);
    assert!(plan_publish(&mut gh, Kind::Web, v)
        .expect_err("refused")
        .contains("is not on"));
    assert!(fake.writes.is_empty(), "{:?}", fake.writes);
    assert!(fake.releases.is_empty());
}

/// Files built from another commit than the tag names, or a tag off `main`, are refused.
#[test]
fn files_built_from_another_commit_or_a_tag_off_main_are_refused() {
    let v = "0.2.0";
    let tag = Kind::Web.tag(v);
    let dir = temp("commit");
    write(&dir, &web::archive_name(v), b"bundle");
    write(&dir, web::MANIFEST, b"{}");
    let mut up = upload_of(&dir, Kind::Web, v, &[&web::archive_name(v), web::MANIFEST]);
    up.commit = "f".repeat(40);
    let mut fake = FakeGitHub::with_tag(&tag);
    let e = run_upload(&mut fake, Kind::Web, v, &up).expect_err("refused");
    assert!(e.contains("package the tagged commit"), "{e}");
    up.commit = COMMIT.to_owned();
    fake.main.clear();
    let e = run_upload(&mut fake, Kind::Web, v, &up).expect_err("refused");
    assert!(e.contains("not on owner/repo's main"), "{e}");
    assert!(fake.writes.is_empty());
}

/// A dry run reads what it needs and sends nothing that changes the release, even when its plan
/// is carried out by mistake.
#[test]
fn a_dry_run_sends_nothing_that_changes_the_release() {
    let v = "0.2.0";
    let tag = Kind::Web.tag(v);
    let dir = temp("dry");
    write(&dir, &web::archive_name(v), b"bundle");
    write(&dir, web::MANIFEST, b"{}");
    let up = upload_of(&dir, Kind::Web, v, &[&web::archive_name(v), web::MANIFEST]);
    let mut fake = FakeGitHub::with_tag(&tag);
    let mut gh = GitHub::new(&mut fake, REPO, false);
    let plan = plan_upload(&mut gh, Kind::Web, v, &up).expect("a plan");
    assert_eq!(
        plan.actions.len(),
        3,
        "create the draft and upload two files"
    );
    let e = apply(&mut gh, plan).expect_err("a dry run refuses to write");
    assert!(e.contains("a dry run sends nothing"), "{e}");
    assert!(fake.writes.is_empty(), "{:?}", fake.writes);
    assert!(fake.releases.is_empty());
}

/// Two drafts for one tag, or a release already published, are refused.
#[test]
fn a_published_release_or_two_drafts_for_the_tag_are_refused() {
    let v = "0.2.0";
    let tag = Kind::Dereth.tag(v);
    let release = |id, draft| FakeRelease {
        id,
        tag: tag.clone(),
        name: String::new(),
        body: String::new(),
        draft,
        prerelease: false,
        make_latest: None,
        assets: Vec::new(),
    };
    let mut fake = FakeGitHub::with_tag(&tag);
    fake.releases = vec![release(5, true), release(6, true)];
    let mut gh = GitHub::new(&mut fake, REPO, true);
    assert!(plan_publish(&mut gh, Kind::Dereth, v)
        .expect_err("two drafts")
        .contains("2 draft releases"));
    fake.releases = vec![release(5, false)];
    let mut gh = GitHub::new(&mut fake, REPO, true);
    assert!(plan_publish(&mut gh, Kind::Dereth, v)
        .expect_err("published")
        .contains("already published"));
}

// ---------------------------------------------------------------- the finish

/// Three machines upload their targets; the finish downloads every machine's files, writes the
/// merged index over them with the package's own index writer, and adds it: `SHA256SUMS` covers
/// every machine's archive, and `MANIFEST.txt` every machine's lines. The release stays a draft.
#[test]
fn the_merged_sha256sums_and_manifest_cover_every_machines_files() {
    let ws = crate::util::workspace_root();
    let v = empyrean_version();
    let tag = Kind::Empyrean.tag(&v);
    let mut fake = FakeGitHub::with_tag(&tag);
    let machines: [&[&str]; 3] = [
        &["x86_64-pc-windows-msvc"],
        &["aarch64-apple-darwin", "x86_64-apple-darwin"],
        &["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"],
    ];
    let mut archives = Vec::new();
    for (i, triples) in machines.iter().enumerate() {
        let dir = temp(&format!("merge-{i}"));
        archives.extend(empyrean_machine(&dir, &v, triples, "one"));
        let up = read_upload_dir(&ws, Kind::Empyrean, &v, REPO, &dir).expect("reads");
        assert!(
            !up.files.iter().any(|f| f.name == "SHA256SUMS"),
            "a machine's own index stays behind"
        );
        run_upload(&mut fake, Kind::Empyrean, &v, &up).expect("an upload");
    }

    let work = temp("merge-work");
    let f = facts();
    let gather = |dir: &Path| crate::package::write_index(&ws, dir, &v, &f);
    let finish = Finish {
        work: work.clone(),
        commit: COMMIT.to_owned(),
        notes: "## Highlights\n\n- A line\n".to_owned(),
        gather: &gather,
    };
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let plan = plan_finish(&mut gh, Kind::Empyrean, &v, &finish).expect("a finish");
    apply(&mut gh, plan).expect("applied");

    let sums = String::from_utf8(fake.asset_bytes(&tag, "SHA256SUMS")).expect("text");
    let listed = files::parse_sums(&sums).expect("sums");
    let names: Vec<&str> = listed.iter().map(|(n, _)| n.as_str()).collect();
    let mut want: Vec<String> = archives.clone();
    want.extend(["MANIFEST.txt".to_owned(), "release.json".to_owned()]);
    want.sort();
    assert_eq!(names, want);
    for (name, hash) in &listed {
        assert_eq!(*hash, sha256_hex(&fake.asset_bytes(&tag, name)), "{name}");
    }
    let manifest = String::from_utf8(fake.asset_bytes(&tag, "MANIFEST.txt")).expect("text");
    for triple in machines.iter().flat_map(|m| m.iter()) {
        assert!(
            manifest.contains(&format!("{triple}\tNOTICE.txt\t")),
            "{triple} in\n{manifest}"
        );
    }
    let release = fake.release(&tag);
    assert!(release.draft, "the finish leaves a draft");
    assert!(release
        .body
        .starts_with("## Highlights\n\n- A line\n\n## Downloads\n\n"));
    assert!(release.body.contains(&format!("/tree/{COMMIT}")));
    assert_eq!(release.name, format!("Empyrean {v}"));

    // A second finish finds everything as written and changes nothing.
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let again = plan_finish(&mut gh, Kind::Empyrean, &v, &finish).expect("a finish");
    assert!(again.actions.is_empty(), "{:?}", again.actions);

    // Publishing removes the staging files and leaves exactly the workflow's release.
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let plan = plan_publish(&mut gh, Kind::Empyrean, &v).expect("publishable");
    apply(&mut gh, plan).expect("published");
    let e = files::expected(Kind::Empyrean, &v);
    let mut final_names = e.release;
    final_names.extend(e.index);
    final_names.sort();
    assert_eq!(fake.asset_names(&tag), final_names);
    assert!(!fake.release(&tag).draft);
}

/// The finish names every file the draft lacks, and writes nothing.
#[test]
fn the_finish_lists_every_missing_file_by_name_and_writes_nothing() {
    let ws = crate::util::workspace_root();
    let v = empyrean_version();
    let tag = Kind::Empyrean.tag(&v);
    let mut fake = FakeGitHub::with_tag(&tag);
    let dir = temp("missing");
    empyrean_machine(&dir, &v, &["x86_64-pc-windows-msvc"], "one");
    let up = read_upload_dir(&ws, Kind::Empyrean, &v, REPO, &dir).expect("reads");
    run_upload(&mut fake, Kind::Empyrean, &v, &up).expect("an upload");
    let writes = fake.writes.len();
    let gather = |_: &Path| -> Result<(), String> { panic!("nothing is gathered") };
    let finish = Finish {
        work: temp("missing-work"),
        commit: COMMIT.to_owned(),
        notes: String::new(),
        gather: &gather,
    };
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let e = plan_finish(&mut gh, Kind::Empyrean, &v, &finish).expect_err("incomplete");
    for triple in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
    ] {
        let file = format!("empyrean-{v}-{triple}.tar.gz");
        assert!(e.contains(&format!("{file} (a release file)")), "{e}");
        assert!(e.contains(&format!("{file}.manifest (its lines")), "{e}");
    }
    assert!(e.contains("lacks 8 of the files"), "{e}");
    assert!(!e.contains("windows"), "{e}");
    assert_eq!(fake.writes.len(), writes);
}

/// Dereth's release lacks a platform's update signature until its launcher's machine uploads it,
/// and the missing-file check says so.
#[test]
fn a_launcher_without_its_update_signature_is_missing_from_the_release() {
    let v = "0.2.0";
    let e = files::expected(Kind::Dereth, v);
    let mut have = e.release.clone();
    have.extend(
        e.staging
            .iter()
            .filter(|n| n.ends_with(".manifest"))
            .cloned(),
    );
    let mut wanted = e.release.clone();
    wanted.extend(e.staging.iter().cloned());
    let lacking = missing(Kind::Dereth, v, &wanted, &have);
    assert_eq!(
        lacking,
        [
            "latest-darwin-aarch64.json (its platform's update signature, uploaded with the launcher)",
            "latest-darwin-x86_64.json (its platform's update signature, uploaded with the launcher)",
            "latest-linux-x86_64.json (its platform's update signature, uploaded with the launcher)",
            "latest-windows-x86_64.json (its platform's update signature, uploaded with the launcher)",
        ]
    );
}

// ---------------------------------------------------------------- publishing

/// A draft as `--finish` leaves it: every file, staging files beside them, and a `SHA256SUMS`
/// over the summed files.
fn finished_draft(kind: Kind, version: &str) -> FakeGitHub {
    let tag = kind.tag(version);
    let mut fake = FakeGitHub::with_tag(&tag);
    let e = files::expected(kind, version);
    let mut assets = Vec::new();
    let mut sums = Vec::new();
    for name in e.release.iter().chain(&e.staging).chain(&e.index) {
        if name == "SHA256SUMS" {
            continue;
        }
        let bytes = format!("{name} bytes").into_bytes();
        if files::summed(kind, version).contains(name) {
            sums.push((name.clone(), sha256_hex(&bytes)));
        }
        assets.push((name.clone(), bytes));
    }
    assets.push((
        "SHA256SUMS".to_owned(),
        files::sums_text(&sums).into_bytes(),
    ));
    let mut release = FakeRelease {
        id: 2,
        tag,
        name: kind.title(version),
        body: "## Highlights\n".to_owned(),
        draft: true,
        prerelease: version::is_prerelease(version),
        make_latest: None,
        assets: Vec::new(),
    };
    for (i, (name, bytes)) in assets.into_iter().enumerate() {
        release.assets.push(FakeAsset {
            id: 100 + i as u64,
            label: files::label(kind, version, &name).expect("label"),
            name,
            state: "uploaded".to_owned(),
            bytes,
        });
    }
    fake.releases.push(release);
    fake.next_id = 1000;
    fake
}

fn publish_finished(kind: Kind, version: &str) -> FakeRelease {
    let mut fake = finished_draft(kind, version);
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let plan = plan_publish(&mut gh, kind, version).expect("publishable");
    apply(&mut gh, plan).expect("published");
    fake.release(&kind.tag(version)).clone()
}

/// A final Dereth or Empyrean release is published as the repository's "Latest"; a pre-release
/// is published as one and never "Latest"; the web client's release is never "Latest".
#[test]
fn published_releases_follow_the_pre_release_and_latest_rules() {
    for (kind, version, pre, latest) in [
        (Kind::Dereth, "0.2.0", false, "true"),
        (Kind::Empyrean, "0.2.0", false, "true"),
        (Kind::Web, "0.2.0", false, "false"),
        (Kind::Dereth, "0.2.0-rc.1", true, "false"),
        (Kind::Empyrean, "0.2.0-rc.1", true, "false"),
        (Kind::Web, "0.2.0-rc.1", true, "false"),
    ] {
        assert_eq!(
            files::publish_flags(kind, version),
            (pre, latest),
            "{kind:?} {version}"
        );
        let r = publish_finished(kind, version);
        assert!(!r.draft, "{kind:?} {version}");
        assert_eq!(r.prerelease, pre, "{kind:?} {version}");
        assert_eq!(r.make_latest.as_deref(), Some(latest), "{kind:?} {version}");
        let e = files::expected(kind, version);
        assert!(
            r.assets.iter().all(|a| !e.staging.contains(&a.name)),
            "the staging files are removed"
        );
        assert_eq!(r.assets.len(), e.release.len() + e.index.len());
    }
}

/// A file uploaded or replaced after the finish leaves `SHA256SUMS` stale, and publishing is
/// refused until the finish runs again; so is a draft without its notes.
#[test]
fn publishing_a_draft_whose_sha256sums_is_stale_or_whose_notes_are_missing_is_refused() {
    let v = "0.2.0";
    let tag = Kind::Dereth.tag(v);
    let mut fake = finished_draft(Kind::Dereth, v);
    let r = fake
        .releases
        .iter_mut()
        .find(|r| r.tag == tag)
        .expect("draft");
    let zip = r
        .assets
        .iter_mut()
        .find(|a| a.name == "dereth-0.2.0-windows-x86_64.zip")
        .expect("zip");
    zip.bytes = b"rebuilt".to_vec();
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let e = plan_publish(&mut gh, Kind::Dereth, v).expect_err("stale");
    assert!(
        e.contains("SHA256SUMS lists dereth-0.2.0-windows-x86_64.zip with another SHA-256"),
        "{e}"
    );

    let mut fake = finished_draft(Kind::Dereth, v);
    fake.releases[0].body = files::PLACEHOLDER_BODY.to_owned();
    let mut gh = GitHub::new(&mut fake, REPO, true);
    assert!(plan_publish(&mut gh, Kind::Dereth, v)
        .expect_err("no notes")
        .contains("no notes yet"));

    let mut fake = finished_draft(Kind::Dereth, v);
    fake.releases[0].assets.retain(|a| a.name != "latest.json");
    let mut gh = GitHub::new(&mut fake, REPO, true);
    let e = plan_publish(&mut gh, Kind::Dereth, v).expect_err("unfinished");
    assert!(e.contains("latest.json (written by --finish)"), "{e}");
    assert!(fake.writes.is_empty());
}

/// The checksum check names each file listed wrongly, missing, or not the release's.
#[test]
fn the_checksum_check_names_each_file_listed_wrongly() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let remote: BTreeMap<String, String> =
        [("x".to_owned(), a.clone()), ("y".to_owned(), a.clone())]
            .into_iter()
            .collect();
    let wanted = ["x".to_owned(), "y".to_owned()];
    assert_eq!(
        check_sums(&format!("{a}  x\n{a}  y\n"), &wanted, &remote),
        Ok(())
    );
    let problems = check_sums(&format!("{b}  x\n{a}  z\n"), &wanted, &remote).expect_err("bad");
    assert_eq!(
        problems,
        [
            "SHA256SUMS lists x with another SHA-256 than the draft's file has",
            "SHA256SUMS does not list y",
            "SHA256SUMS lists z, which the release does not carry",
        ]
    );
}

// ---------------------------------------------------------------- the workflows' release

fn workflow(name: &str) -> String {
    std::fs::read_to_string(
        crate::util::workspace_root()
            .join(".github/workflows")
            .join(name),
    )
    .expect("the workflow")
}

/// The text a workflow appends under the notes: its here-document, as the shell writes it.
fn workflow_downloads(text: &str, version: &str) -> String {
    let start = text.find("<<EOF\n").expect("a here-document") + "<<EOF\n".len();
    let end = start + text[start..].find("\n          EOF\n").expect("its end") + 1;
    text[start..end]
        .lines()
        .map(|l| format!("{}\n", l.strip_prefix("          ").unwrap_or(l)))
        .collect::<String>()
        .replace("\\`", "`")
        .replace("$VERSION", version)
        .replace("$GITHUB_REPOSITORY", REPO)
        .replace("$GITHUB_SHA", COMMIT)
}

/// The description is the notes, then the downloads text the product's workflow writes, to the
/// byte.
#[test]
fn the_description_is_the_notes_then_the_workflows_downloads_text() {
    for (kind, file) in [
        (Kind::Dereth, "release-dereth.yml"),
        (Kind::Empyrean, "release-empyrean.yml"),
        (Kind::Web, "release-web.yml"),
    ] {
        let want = format!("NOTES\n{}", workflow_downloads(&workflow(file), "0.2.0"));
        assert_eq!(
            files::description(kind, "0.2.0", REPO, COMMIT, "NOTES\n"),
            want,
            "{file}"
        );
    }
}

/// Every file the workflow's release carries has the workflow's label, and every label the
/// workflow gives belongs to one of them.
#[test]
fn every_release_file_has_the_workflows_label() {
    let pattern = regex::Regex::new(r#"(?m)^\s*"?([^"\s)]+)"?\) label="([^"]+)" ;;"#).expect("re");
    for (kind, file) in [
        (Kind::Dereth, "release-dereth.yml"),
        (Kind::Empyrean, "release-empyrean.yml"),
        (Kind::Web, "release-web.yml"),
    ] {
        let v = "0.2.0";
        let text = workflow(file);
        let labels: Vec<(String, String)> = pattern
            .captures_iter(&text)
            .map(|c| (c[1].replace("$VERSION", v), c[2].to_owned()))
            .collect();
        let e = files::expected(kind, v);
        let names: Vec<String> = e.release.into_iter().chain(e.index).collect();
        assert_eq!(labels.len(), names.len(), "{file}: {labels:?}");
        for name in &names {
            let (_, label) = labels
                .iter()
                .find(|(p, _)| name == p || name.ends_with(&format!("-{p}")))
                .unwrap_or_else(|| panic!("{file} labels no {name}"));
            assert_eq!(
                files::label(kind, v, name).as_deref(),
                Some(label.as_str()),
                "{name}"
            );
        }
        for name in &e.staging {
            assert_eq!(
                files::label(kind, v, name).as_deref(),
                Some(files::STAGING_LABEL)
            );
        }
    }
}

/// The options read as given; one step exactly, a release version and a repository name.
#[test]
fn the_options_read_as_given() {
    let args = |s: &str| -> Vec<String> { s.split_whitespace().map(String::from).collect() };
    let o = parse(&args("dereth 0.2.0 --upload dist --repo a/b --yes")).expect("parses");
    assert_eq!(o.kind, Kind::Dereth);
    assert_eq!(o.version, "0.2.0");
    assert_eq!(o.step, Step::Upload(PathBuf::from("dist")));
    assert_eq!(o.repo.as_deref(), Some("a/b"));
    assert!(o.yes);
    let o = parse(&args("web 0.2.0-rc.1 --finish")).expect("parses");
    assert_eq!((o.step, o.yes), (Step::Finish, false));
    assert!(parse(&args("dereth 0.2.0")).is_err(), "no step");
    assert!(
        parse(&args("dereth 0.2.0 --finish --publish")).is_err(),
        "two steps"
    );
    assert!(
        parse(&args("dereth v0.2.0 --finish")).is_err(),
        "not a version"
    );
    assert!(parse(&args("launcher 0.2.0 --finish")).is_err());
    assert!(parse(&args("dereth 0.2.0 --finish --repo nope")).is_err());
}

/// The token never shows in a debug print.
#[test]
fn the_token_is_never_shown() {
    let token = github::Token("github_pat_secret".to_owned());
    assert_eq!(format!("{token:?}"), "Token(hidden)");
    let https = github::Https::new(github::Token("github_pat_secret".to_owned()));
    assert!(!format!("{https:?}").contains("secret"));
}

/// A query value keeps only the unreserved characters.
#[test]
fn labels_and_names_are_percent_encoded_in_the_upload_address() {
    let label = "Dereth · macOS Intel (x)";
    let encoded = github::query_value(label);
    assert!(encoded.starts_with("Dereth%20"), "{encoded}");
    assert!(
        encoded
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~%".contains(&b)),
        "{encoded}"
    );
    assert_eq!(percent_decode(&encoded), label);
    assert_eq!(
        github::query_value("Dereth-0.2.0-macos-x86_64.app.tar.gz"),
        "Dereth-0.2.0-macos-x86_64.app.tar.gz"
    );
}
