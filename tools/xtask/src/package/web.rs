//! `cargo xtask package web`: the web client's release files, built the same way on a
//! contributor's machine and in the release workflow.
//!
//! ```text
//! cargo xtask package web [--out <dir>] [--tag <tag>] [--no-build] [--allow-dirty]
//! ```
//!
//! It builds the module (`cargo xtask web --build-only`: the `web-release` profile and the bindgen
//! step into `dereth/web/www/`), takes the page's files from `www/` by an allowlist, checks them,
//! and writes three release files into `<target dir>/package/dereth-web-<version>/` (or `--out`):
//!
//! - `dereth-web-<version>.zip`: the files under one folder, `dereth-web-<version>/`, ready to be
//!   served as they are ([`FILES`]), with the notices the module needs beside them ([`NOTICES`]:
//!   the licence, what the module is built from and the typefaces it carries, and every
//!   third-party crate's licence text);
//! - `web.json`: the release's manifest, which a host or the launcher reads to find out what the
//!   newest web client is and to check what it downloaded: the version, the commit, the archive's
//!   name, size and SHA-256, and every file's path, size and SHA-256 ([`manifest`]);
//! - `SHA256SUMS`: both, for `sha256sum -c`.
//!
//! The web client is the Dereth client in a browser, so it carries Dereth's version (the client's
//! and the launcher's, which `cargo xtask release dereth` moves for all three), and its release tag
//! is `dereth-web-v<version>` ([`TAG_PREFIX`]); `--tag` refuses a tag that names another version.
//!
//! **The data guard.** Only the allowlist is staged, a file in `www/` that is on no list fails the
//! package (so a new page file cannot be left out unseen, and nothing else can slip in), and the
//! deny scan ([`super::guard`]) runs over the staged files and again over the written archive: a
//! game data file, anything named like one, a database, a world pack or an oversized file fails it
//! whatever its name. The module and scripts are also refused when they carry this machine's
//! folders ([`guard::local_paths_in`]).

use std::path::{Path, PathBuf};

use super::archive::{self, Member};
use super::guard::{self, Entry, Kind, HEAD_LEN};
use super::version;
use super::{build_facts, read, sha256_hex, write, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// The prefix of a web client release tag: `dereth-web-v<version>`.
pub const TAG_PREFIX: &str = "dereth-web-v";

/// The manifest's name among the release files.
pub const MANIFEST: &str = "web.json";

/// The manifest's format; a consumer refuses a number it does not know.
pub const SCHEMA: u64 = 1;

/// Where the page's files are, relative to the workspace.
pub const WWW: &str = "dereth/web/www";

/// Every file the bundle holds, relative to `www/`, and nothing else.
pub const FILES: &[&str] = &[
    "_headers",
    "audio-worklet.js",
    "datfiles.js",
    "index.html",
    "pkg/dereth_web.js",
    "pkg/dereth_web_bg.wasm",
    "play-worker.js",
    "play.js",
];

/// The notices the bundle carries beside the page's files, written by the package itself.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// The module's target, for the licences of what it is built from.
const WASM: super::targets::Target = super::targets::Target {
    triple: "wasm32-unknown-unknown",
    os: super::targets::Os::Linux,
    arch: super::targets::Arch::X86_64,
};

/// Files under `www/` that are never released: the folder's own ignore file.
const NOT_RELEASED: &[&str] = &[".gitignore"];

/// No file is larger than this: the module is about 13 MB.
const FILE_CAP: u64 = 48 << 20;

/// No bundle is larger than this in all.
const TOTAL_CAP: u64 = 64 << 20;

/// The archive's file name for `version`.
#[must_use]
pub fn archive_name(version: &str) -> String {
    format!("dereth-web-{version}.zip")
}

/// The folder inside the archive.
#[must_use]
pub fn archive_root(version: &str) -> String {
    format!("dereth-web-{version}")
}

/// A tag must name exactly the version the tree carries.
///
/// # Errors
/// The tag is not a web client release tag, or names another version.
pub fn check_tag(tag: &str, version: &str) -> Result<(), String> {
    let tag = tag.strip_prefix("refs/tags/").unwrap_or(tag);
    let Some(tagged) = tag.strip_prefix(TAG_PREFIX) else {
        return Err(format!(
            "tag `{tag}` is not a web client release tag ({TAG_PREFIX}<version>)"
        ));
    };
    version::parse_release_version(tagged)?;
    if tagged == version {
        Ok(())
    } else {
        Err(format!(
            "tag `{tag}` names {tagged}, but the client, the launcher and the web client carry \
             {version}: tag the commit that carries the version"
        ))
    }
}

/// The files of `www` to release, from its listing (`/`-separated paths below it, with their
/// bytes): every [`FILES`] entry, in order. A file on no list is refused.
///
/// # Errors
/// Every listed file that is missing, and every file present that is on no list.
pub fn stage(listing: &[(String, Vec<u8>)]) -> Result<Vec<Member>, Vec<String>> {
    let mut findings = Vec::new();
    for (path, _) in listing {
        if !FILES.contains(&path.as_str()) && !NOT_RELEASED.contains(&path.as_str()) {
            findings.push(format!(
                "{WWW}/{path}: not a released file; add it to the web package's list, or remove it"
            ));
        }
    }
    let mut members = Vec::new();
    for name in FILES {
        match listing.iter().find(|(p, _)| p == name) {
            Some((_, bytes)) => members.push(Member {
                name: (*name).to_owned(),
                bytes: bytes.clone(),
                executable: false,
            }),
            None => findings.push(format!(
                "{WWW}/{name}: missing (`cargo xtask web --build-only` writes pkg/)"
            )),
        }
    }
    if findings.is_empty() {
        Ok(members)
    } else {
        Err(findings)
    }
}

/// The deny scan's entries for `members`, with their folders, as the archive holds them.
#[must_use]
pub fn entries(members: &[Member]) -> Vec<Entry> {
    let mut out = vec![Entry {
        path: String::new(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }];
    let mut dirs: Vec<String> = members
        .iter()
        .filter_map(|m| m.name.rsplit_once('/').map(|(d, _)| d.to_owned()))
        .collect();
    dirs.sort();
    dirs.dedup();
    out.extend(dirs.into_iter().map(|path| Entry {
        path,
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }));
    out.extend(members.iter().map(|m| Entry {
        path: m.name.clone(),
        kind: Kind::File,
        size: m.bytes.len() as u64,
        head: m.bytes[..m.bytes.len().min(HEAD_LEN)].to_vec(),
    }));
    out
}

/// Every finding of the deny scan over `entries`.
#[must_use]
pub fn scan(entries: &[Entry]) -> Vec<String> {
    let required: Vec<String> = FILES
        .iter()
        .chain(NOTICES)
        .map(|f| (*f).to_owned())
        .collect();
    guard::scan_tree(entries, &required, &[], FILE_CAP, TOTAL_CAP)
}

/// The release's manifest: what a consumer reads to learn the newest web client and to check
/// what it downloaded.
#[must_use]
pub fn manifest(
    version: &str,
    facts: &BuildFacts,
    archive: (&str, &[u8]),
    members: &[Member],
) -> serde_json::Value {
    let files: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            serde_json::json!({
                "path": m.name,
                "size": m.bytes.len(),
                "sha256": sha256_hex(&m.bytes),
            })
        })
        .collect();
    serde_json::json!({
        "schema": SCHEMA,
        "product": "dereth-web",
        "version": version,
        "tag": format!("{TAG_PREFIX}{version}"),
        "prerelease": version::is_prerelease(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "build_number": facts.number,
        "source_url": facts.source_url,
        "entry": "index.html",
        "archive": {
            "file": archive.0,
            "root": archive_root(version),
            "size": archive.1.len(),
            "sha256": sha256_hex(archive.1),
            "url": format!(
                "{}/releases/download/{TAG_PREFIX}{version}/{}",
                facts.source_url, archive.0
            ),
        },
        "size": members.iter().map(|m| m.bytes.len()).sum::<usize>(),
        "files": files,
    })
}

/// The options of one run.
#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    out: Option<PathBuf>,
    tag: Option<String>,
    build: bool,
    allow_dirty: bool,
}

pub const USAGE: &str =
    "usage: cargo xtask package web [--out <dir>] [--tag <tag>] [--no-build] [--allow-dirty]";

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        build: true,
        ..Options::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--tag" => o.tag = Some(value()?),
            "--no-build" => o.build = false,
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    Ok(o)
}

/// `cargo xtask package web ...`.
pub fn package(args: &[String]) -> i32 {
    let options = match parse_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    match build(&workspace_root(), &options) {
        Ok(dir) => {
            println!(
                "\npackage: done; the release files are in {}",
                dir.display()
            );
            0
        }
        Err(e) => {
            eprintln!("\npackage: FAILED\n{e}");
            1
        }
    }
}

/// The deny scan of a written bundle: every file in it, as [`scan`] reads them.
pub(crate) fn check_bundle(path: &Path, version: &str) -> Result<(), String> {
    let written = archive::archive_entries(path, &archive_root(version))?;
    let findings = scan(&written);
    if findings.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the deny scan refused {}:\n  {}",
            path.display(),
            findings.join("\n  ")
        ))
    }
}

/// Every file under `dir`, `/`-separated below it, with its bytes.
fn listing(dir: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let entries = guard::staged_entries(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for e in entries {
        match e.kind {
            Kind::File => {
                let bytes = read(&dir.join(&e.path))?;
                out.push((e.path, bytes));
            }
            Kind::Dir => {}
            Kind::Link(what) => return Err(format!("{WWW}/{}: {what}", e.path)),
        }
    }
    Ok(out)
}

/// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html` for the module.
fn notices(ws: &Path, version: &str, facts: &BuildFacts) -> Result<Vec<Member>, String> {
    let manifest = "dereth/web/Cargo.toml";
    let crates = super::dereth::crates_of(ws, manifest, "dereth-web", WASM)?;
    let mit = String::from_utf8_lossy(&read(&ws.join("LICENSE"))?).into_owned();
    let mut fonts = Vec::new();
    for (name, carried, path) in super::dereth::FONTS {
        if path.starts_with("dereth/client/") {
            fonts.push((
                *name,
                *carried,
                String::from_utf8_lossy(&read(&ws.join(path))?).into_owned(),
            ));
        }
    }
    let text = super::notice::web_notice(&super::notice::WebFacts {
        version,
        commit: &facts.commit,
        source_url: &facts.source_url,
        crates: &crates,
        mit_licence: &mit,
        fonts: &fonts,
    });
    let about = target_dir().join("package").join("web-about");
    std::fs::create_dir_all(&about).map_err(|e| format!("{}: {e}", about.display()))?;
    let page = super::dereth::licence_page(ws, manifest, WASM, &about, "dereth-web")?;
    let licences = super::notice::splice_licence_pages(&[("the web client's module", page)])?;
    Ok([
        ("LICENSE", mit.into_bytes()),
        ("NOTICE.txt", text.into_bytes()),
        ("THIRD-PARTY-LICENSES.html", licences.into_bytes()),
    ]
    .into_iter()
    .map(|(name, bytes)| Member {
        name: name.to_owned(),
        bytes,
        executable: false,
    })
    .collect())
}

fn build(ws: &Path, o: &Options) -> Result<PathBuf, String> {
    let version = version::dereth_version(ws)?;
    version::parse_release_version(&version)?;
    if let Some(tag) = &o.tag {
        check_tag(tag, &version)?;
    }
    let mut facts = build_facts(ws, o.allow_dirty)?;
    if let Ok(url) = std::env::var(super::dereth::SOURCE_URL_ENV) {
        if !url.trim().is_empty() {
            facts.source_url = url.trim().trim_end_matches('/').to_owned();
        }
    }
    if o.build {
        println!("=== building the web client {version}");
        let ok = crate::util::run(
            ws,
            "cargo",
            &[
                "run",
                "--release",
                "--locked",
                "-q",
                "-p",
                "dereth-web-dev",
                "--",
                "--build-only",
            ],
        );
        if !ok {
            return Err("the web client did not build".to_owned());
        }
    }
    let www = ws.join(WWW);
    let mut members = stage(&listing(&www)?).map_err(|f| f.join("\n"))?;
    members.extend(notices(ws, &version, &facts)?);
    let mut findings = scan(&entries(&members));
    let folders = guard::local_folders(ws);
    for m in &members {
        for folder in guard::local_paths_in(&m.bytes, &folders) {
            findings.push(format!("{}: names this machine's folder {folder}", m.name));
        }
    }
    if !members
        .iter()
        .any(|m| m.name.ends_with(".wasm") && m.bytes.starts_with(b"\0asm"))
    {
        findings.push("pkg/dereth_web_bg.wasm: not a WebAssembly module".to_owned());
    }
    if !findings.is_empty() {
        return Err(format!(
            "the web bundle is refused:\n  {}",
            findings.join("\n  ")
        ));
    }
    let out = o.out.clone().unwrap_or_else(|| {
        target_dir()
            .join("package")
            .join(format!("dereth-web-{version}"))
    });
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let name = archive_name(&version);
    let zip = archive::zip_bytes(&archive_root(&version), &members, facts.epoch)?;
    write(&out.join(&name), &zip)?;
    // The archive as written, scanned again.
    check_bundle(&out.join(&name), &version)?;
    let mut json =
        serde_json::to_string_pretty(&manifest(&version, &facts, (&name, &zip), &members))
            .map_err(|e| e.to_string())?;
    json.push('\n');
    write(&out.join(MANIFEST), json.as_bytes())?;
    let sums = format!(
        "{}  {name}\n{}  {MANIFEST}\n",
        sha256_hex(&zip),
        sha256_hex(json.as_bytes())
    );
    write(&out.join("SHA256SUMS"), sums.as_bytes())?;
    println!("\n{json}\nSHA256SUMS\n{sums}");
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the web client's release files, their deny scan, manifest and tag).
    use super::*;

    fn www() -> Vec<(String, Vec<u8>)> {
        FILES
            .iter()
            .map(|f| {
                let bytes = if f.ends_with(".wasm") {
                    b"\0asm\x01\0\0\0".to_vec()
                } else {
                    format!("// {f}\n").into_bytes()
                };
                ((*f).to_owned(), bytes)
            })
            .collect()
    }

    /// The staged page files with the notices the package writes beside them.
    fn bundle(listing: &[(String, Vec<u8>)]) -> Vec<Member> {
        let mut members = stage(listing).expect("stages");
        members.extend(NOTICES.iter().map(|n| Member {
            name: (*n).to_owned(),
            bytes: b"notice".to_vec(),
            executable: false,
        }));
        members
    }

    fn facts() -> BuildFacts {
        BuildFacts {
            commit: "abc123".into(),
            branch: "dereth-web-v0.2.0".into(),
            number: "4242".into(),
            epoch: 1_759_000_000,
            source_url: "https://github.com/example/dereth".into(),
        }
    }

    #[test]
    fn the_bundle_is_the_listed_page_files_and_a_stray_or_missing_one_refuses_it() {
        let mut listing = www();
        listing.push((".gitignore".into(), b"pkg/\n".to_vec()));
        let members = stage(&listing).expect("stages");
        assert_eq!(
            members.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            FILES
        );
        let missing = scan(&entries(&members));
        assert!(
            missing.iter().any(|f| f.contains("NOTICE.txt")),
            "{missing:?}"
        );
        assert!(scan(&entries(&bundle(&listing))).is_empty());

        let mut stray = www();
        stray.push(("notes.txt".into(), b"x".to_vec()));
        let e = stage(&stray).expect_err("a stray file");
        assert!(e.iter().any(|f| f.contains("notes.txt")), "{e:?}");

        let missing: Vec<_> = www()
            .into_iter()
            .filter(|(p, _)| !p.ends_with(".wasm"))
            .collect();
        let e = stage(&missing).expect_err("no module");
        assert!(e.iter().any(|f| f.contains("dereth_web_bg.wasm")), "{e:?}");
    }

    /// The deny scan refuses the game's data whatever it is called, and anything outsized.
    #[test]
    fn game_data_in_the_bundle_is_refused_whatever_its_name() {
        let mut members = stage(&www()).expect("stages");
        let mut dat = vec![0u8; 0x400];
        dat[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
        members[1].bytes = dat;
        let found = scan(&entries(&members));
        assert!(
            found.iter().any(|f| f.contains("data-file header")),
            "{found:?}"
        );
        let mut named = entries(&stage(&www()).expect("stages"));
        named.push(Entry {
            path: "client_portal.dat".into(),
            kind: Kind::File,
            size: 1,
            head: vec![0],
        });
        assert!(scan(&named).iter().any(|f| f.contains("client_portal.dat")));
        let mut big = stage(&www()).expect("stages");
        big[0].bytes = vec![0; usize::try_from(FILE_CAP).unwrap() + 1];
        assert!(scan(&entries(&big)).iter().any(|f| f.contains("cap")));
    }

    /// The manifest names the version, the archive and every file with its size and hash, and
    /// where the archive is downloaded from.
    #[test]
    fn the_manifest_names_the_release_its_archive_and_every_file_with_its_hash() {
        let members = stage(&www()).expect("stages");
        let zip = b"zip bytes".to_vec();
        let m = manifest("0.2.0", &facts(), ("dereth-web-0.2.0.zip", &zip), &members);
        assert_eq!(m["schema"], SCHEMA);
        assert_eq!(m["product"], "dereth-web");
        assert_eq!(m["version"], "0.2.0");
        assert_eq!(m["tag"], "dereth-web-v0.2.0");
        assert_eq!(m["prerelease"], false);
        assert_eq!(m["entry"], "index.html");
        assert_eq!(m["archive"]["root"], "dereth-web-0.2.0");
        assert_eq!(m["archive"]["sha256"], sha256_hex(&zip));
        assert_eq!(
            m["archive"]["url"],
            "https://github.com/example/dereth/releases/download/dereth-web-v0.2.0/dereth-web-0.2.0.zip"
        );
        let files = m["files"].as_array().expect("files");
        assert_eq!(files.len(), FILES.len());
        let wasm = files
            .iter()
            .find(|f| f["path"] == "pkg/dereth_web_bg.wasm")
            .expect("the module");
        assert_eq!(wasm["sha256"], sha256_hex(b"\0asm\x01\0\0\0"));
        assert_eq!(wasm["size"], 8);
        let rc = manifest("0.2.0-rc.1", &facts(), ("x.zip", &zip), &members);
        assert_eq!(rc["prerelease"], true);
    }

    /// The notice names the source and the release, says there is no game data, and carries
    /// the licence of the typeface the module carries.
    #[test]
    fn the_web_notice_names_the_source_and_carries_the_typeface_licence() {
        let crates = [super::super::notice::Crate {
            name: "skrifa".to_owned(),
            version: "0.48.0".to_owned(),
            licence: "MIT OR Apache-2.0".to_owned(),
        }];
        let fonts = [(
            "Liberation",
            "The classic interface's text is drawn in Liberation",
            "OFL text".to_owned(),
        )];
        let text = super::super::notice::web_notice(&super::super::notice::WebFacts {
            version: "0.2.0",
            commit: "abc123",
            source_url: "https://github.com/example/dereth/",
            crates: &crates,
            mit_licence: "MIT License",
            fonts: &fonts,
        });
        assert!(text.contains("https://github.com/example/dereth/tree/abc123"));
        assert!(text.contains("releases/tag/dereth-web-v0.2.0"));
        assert!(text.contains("NO GAME DATA") && text.contains("MIT License"));
        assert!(text.contains("THE LIBERATION TYPEFACE") && text.contains("OFL text"));
        assert!(text.contains("skrifa 0.48.0 (MIT OR Apache-2.0)"));
        let liberation = super::super::dereth::FONTS
            .iter()
            .filter(|(_, _, path)| path.starts_with("dereth/client/"))
            .count();
        assert_eq!(
            liberation, 1,
            "the module carries the classic interface's typeface"
        );
    }

    #[test]
    fn the_tag_names_the_version_the_tree_carries() {
        assert!(check_tag("dereth-web-v0.2.0", "0.2.0").is_ok());
        assert!(check_tag("refs/tags/dereth-web-v0.2.0", "0.2.0").is_ok());
        assert!(check_tag("dereth-web-v0.2.1", "0.2.0").is_err());
        assert!(check_tag("dereth-v0.2.0", "0.2.0").is_err());
        assert!(check_tag("dereth-web-vx", "x").is_err());
    }

    #[test]
    fn the_archive_holds_the_files_under_one_folder_and_scans_clean() {
        let members = bundle(&www());
        let zip = archive::zip_bytes(&archive_root("0.2.0"), &members, 1_759_000_000).unwrap();
        let read =
            archive::relative_to_root(archive::zip_entries(&zip).unwrap(), &archive_root("0.2.0"));
        assert!(scan(&read).is_empty(), "{:?}", scan(&read));
        assert!(read.iter().any(|e| e.path == "pkg/dereth_web_bg.wasm"));
    }

    #[test]
    fn the_options_read_as_given() {
        let args = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        let o = parse_options(&args("--out d --tag dereth-web-v1.0.0 --no-build")).unwrap();
        assert_eq!(o.out, Some(PathBuf::from("d")));
        assert_eq!(o.tag.as_deref(), Some("dereth-web-v1.0.0"));
        assert!(!o.build && !o.allow_dirty);
        assert!(parse_options(&args("--target x")).is_err());
    }
}
