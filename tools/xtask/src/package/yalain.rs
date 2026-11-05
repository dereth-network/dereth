//! `cargo xtask package yalain`: Yalain's release archives (the Asheron's Call 2 server, the
//! `yalain` program with its world database tool built in), built the same way on a contributor's
//! machine and in the release workflow, and the way Empyrean's are.
//!
//! ```text
//! cargo xtask package yalain [--target <triple>]... [--out <dir>] [--allow-dirty]
//! cargo xtask package yalain --gather <dir>
//! ```
//!
//! For each target it builds `yalain` in the `server-release` profile (Linux with `cargo zigbuild`,
//! against Empyrean's glibc floor, so a release runs on the servers Empyrean's does), stages an
//! allowlist (the program, README, CHANGES and the notices), checks every file and the program's
//! header, and writes one archive, `yalain-<version>-<target>.zip` on Windows and `.tar.gz`
//! elsewhere, its one folder the same name without the extension; then the release's index beside
//! the archives: `MANIFEST.txt`, `release.json` (with the release's upgrade declaration,
//! [`super::yalain_declaration`], from `yalain/releases.toml` and the code) and `SHA256SUMS`, which
//! the program's own updater reads (`yalain update`, `server.update`). Without `--target` it packages
//! the host's own target. `--gather` builds nothing: it checks the archives already in `<dir>` (the
//! release workflow downloads each runner's there) and writes the index over them.
//!
//! **The dat guard** is Empyrean's: only the allowlist is ever copied, from named paths, and the
//! deny scan runs over the staging folder and again over the written archive. A game data file of
//! either game, anything named like one, a database, a link or an oversized file fails the package
//! whatever its name, and so does a program that names one of this machine's folders.

use std::path::{Path, PathBuf};

use super::archive::{self, Member};
use super::dereth::{about_page, copy, crates_of, fresh_dir, mkdir, run_in, succeeds};
use super::guard;
use super::headers::{self, Role};
use super::notice;
use super::targets::{self, Builder, Os, Target};
use super::version::{self, Product};
use super::{build_facts, read, sha256_hex, write, BuildFacts};
use crate::util::target_dir;

/// The program's name, and the package and manifest it is built from.
pub const PROGRAM: &str = "yalain";
const MANIFEST: &str = "yalain/server/Cargo.toml";

/// The documents an archive carries beside the notices, as (name in the archive, path in the
/// tree).
pub const DOCUMENTS: &[(&str, &str)] = &[
    ("README.md", "yalain/README.md"),
    ("CHANGES.md", "yalain/CHANGES.md"),
];

/// The notices every archive carries.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// cargo-about's configuration and template: the one Asheron's Call 2's programs share.
const ABOUT: (&str, &str) = ("auberean/about.toml", "auberean/about.hbs");

/// The repository the release files are downloaded from and the notices name as the source, when
/// it is not the default.
pub const SOURCE_URL_ENV: &str = "YALAIN_BUILD_SOURCE_URL";

/// No single file of an archive is larger than this: the program is some tens of megabytes; every
/// retail data file but the language files is larger, and the name rules and both games' data-file
/// headers catch those.
pub const FILE_CAP: u64 = 64 << 20;

/// No archive holds more than this in all.
pub const TOTAL_CAP: u64 = 128 << 20;

/// The archive's one top-level folder: `yalain-<version>-<target>`.
pub fn archive_root(target: Target, version: &str) -> String {
    format!("{PROGRAM}-{version}-{}", target.triple)
}

/// The archive's file name: its folder's name and `.zip` on Windows, `.tar.gz` elsewhere.
pub fn archive_name(target: Target, version: &str) -> String {
    format!("{}.{}", archive_root(target, version), target.archive_ext())
}

/// Every file an archive for `target` holds, and nothing else.
pub fn allowlist(target: Target) -> Vec<String> {
    let mut names = vec![format!("{PROGRAM}{}", target.exe_suffix())];
    names.extend(NOTICES.iter().map(|n| (*n).to_owned()));
    names.extend(DOCUMENTS.iter().map(|(n, _)| (*n).to_owned()));
    names.sort();
    names
}

/// What a file of a Yalain release folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    Archive(Target),
    /// One archive's lines of `MANIFEST.txt`.
    ManifestPart,
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
const INDEX_FILES: &[&str] = &["SHA256SUMS", "release.json", "MANIFEST.txt"];

/// Classify a file of the release folder, or refuse it: the folder holds this version's archives,
/// their manifest parts and the index, and nothing else.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    let archive = name.strip_suffix(".manifest").unwrap_or(name);
    for t in targets::TARGETS {
        if archive == archive_name(*t, version) {
            return Ok(if archive.len() == name.len() {
                Asset::Archive(*t)
            } else {
                Asset::ManifestPart
            });
        }
    }
    Err(format!(
        "`{name}` is not a file of the Yalain {version} release (an archive \
         yalain-{version}-<target>.zip|.tar.gz, its .manifest or the index)"
    ))
}

/// The options of one `package yalain` run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
}

pub const USAGE: &str =
    "usage: cargo xtask package yalain [--target <triple>]... [--out <dir>] [--allow-dirty]
       cargo xtask package yalain --gather <dir>";

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{USAGE}"))
        };
        match a.as_str() {
            "--target" => o.targets.push(value()?),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--gather" => o.gather = Some(PathBuf::from(value()?)),
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    if o.gather.is_some() && (!o.targets.is_empty() || o.out.is_some()) {
        return Err(format!(
            "--gather builds nothing: it takes no --target or --out\n{USAGE}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package yalain ...` (the arguments after `yalain`).
pub fn package(args: &[String]) -> i32 {
    let options = match parse_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = crate::util::workspace_root();
    let result = match &options.gather {
        Some(dir) => gather(&ws, dir, options.allow_dirty),
        None => {
            let triples = if options.targets.is_empty() {
                match super::host_triple() {
                    Ok(h) => vec![h],
                    Err(e) => {
                        eprintln!("package: {e}");
                        return 1;
                    }
                }
            } else {
                options.targets.clone()
            };
            build_packages(&ws, &triples, options.out.as_deref(), options.allow_dirty)
        }
    };
    match result {
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

/// The build facts, with the repository the release files are downloaded from.
fn facts(ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    let mut facts = build_facts(ws, allow_dirty)?;
    if let Ok(url) = std::env::var(SOURCE_URL_ENV) {
        if !url.trim().is_empty() {
            facts.source_url = url.trim().trim_end_matches('/').to_owned();
        }
    }
    Ok(facts)
}

/// Build, stage, check and archive every target, then write the index. The folder the release
/// files are in.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
) -> Result<PathBuf, String> {
    let version = Product::Yalain.version(ws)?;
    version::parse_release_version(&version)?;
    // A release whose upgrade declaration is refused is refused before anything is built.
    super::yalain_declaration::for_release(ws, &version)?;
    // Everything a host lacks is said before anything is built.
    let host = super::host();
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    for triple in triples {
        let all: Vec<&str> = targets::TARGETS.iter().map(|t| t.triple).collect();
        match targets::find_among(&all, "Yalain", triple)
            .and_then(|t| targets::plan(t, &host).map(|b| (t, b)))
        {
            Ok(p) => plans.push(p),
            Err(e) => refusals.push(format!("  {triple}: {e}")),
        }
    }
    if !succeeds("cargo", &["about", "--version"]) {
        refusals.push(
            "  cargo-about is not installed, or does not run on this host; it writes THIRD-PARTY-LICENSES.html: \
             `cargo install --locked cargo-about --features cli`"
                .to_owned(),
        );
    }
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let facts = facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("{PROGRAM}-{version}")));
    mkdir(&out_dir)?;
    for (target, builder) in plans {
        println!("\n=== yalain {version} for {}", target.triple);
        package_target(
            ws,
            target,
            builder,
            &facts,
            &version,
            &out_dir,
            &base.join("stage").join(format!("yalain-{}", target.triple)),
        )?;
    }
    write_index(ws, &out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the archives there and write the index over them.
fn gather(ws: &Path, dir: &Path, allow_dirty: bool) -> Result<PathBuf, String> {
    let version = Product::Yalain.version(ws)?;
    let facts = facts(ws, allow_dirty)?;
    write_index(ws, dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

fn package_target(
    ws: &Path,
    target: Target,
    builder: Builder,
    facts: &BuildFacts,
    version: &str,
    out_dir: &Path,
    stage_root: &Path,
) -> Result<(), String> {
    fresh_dir(stage_root)?;
    // 1. Build.
    let mut env = super::desktop::build_env(ws, facts, target);
    // The repository the program looks for its own releases in.
    env.push((SOURCE_URL_ENV.to_owned(), facts.source_url.clone()));
    let target_arg = target.build_target_arg();
    let verb = match builder {
        Builder::Cargo => "build",
        Builder::Zigbuild => "zigbuild",
    };
    run_in(
        ws,
        "cargo",
        &[
            verb,
            "--locked",
            "--profile",
            "server-release",
            "--target",
            &target_arg,
            "-p",
            PROGRAM,
        ],
        &env,
        &[],
    )?;
    let exe = format!("{PROGRAM}{}", target.exe_suffix());
    let built = target_dir()
        .join(target.triple)
        .join("server-release")
        .join(&exe);

    // 2. Stage the allowlist, each file written anew from a named path.
    let root = archive_root(target, version);
    let stage = stage_root.join(&root);
    fresh_dir(&stage)?;
    copy(&built, &stage.join(&exe))?;
    for (name, from) in DOCUMENTS {
        copy(&ws.join(from), &stage.join(name))?;
    }

    // 3. The notices.
    let crates = crates_of(ws, "Cargo.toml", PROGRAM, target)?;
    let mit = String::from_utf8_lossy(&read(&ws.join("LICENSE"))?).into_owned();
    let text = notice::game_notice(&notice::GameFacts {
        product: Product::Yalain,
        version,
        target: target.triple,
        commit: &facts.commit,
        source_url: &facts.source_url,
        crates: &crates,
        mit_license: &mit,
        fonts: &[],
    });
    write(&stage.join("NOTICE.txt"), text.as_bytes())?;
    write(&stage.join("LICENSE"), mit.as_bytes())?;
    let about = stage_root.join("about");
    mkdir(&about)?;
    let page = about_page(ws, ABOUT, MANIFEST, target, &about, PROGRAM)?;
    write(
        &stage.join("THIRD-PARTY-LICENSES.html"),
        notice::splice_licence_pages(&[(exe.as_str(), page)])?.as_bytes(),
    )?;

    // 4. The deny scan of the staging folder.
    let allow = allowlist(target);
    let staged = guard::staged_entries(&stage).map_err(|e| format!("{}: {e}", stage.display()))?;
    let findings = guard::scan_tree(&staged, &allow, &[], FILE_CAP, TOTAL_CAP);
    if !findings.is_empty() {
        return Err(format!(
            "the deny scan refused the staging folder {}:\n  {}",
            stage.display(),
            findings.join("\n  ")
        ));
    }

    // 5. The header check, and no program naming this machine's folders.
    let bytes = read(&stage.join(&exe))?;
    let named = guard::local_paths_in(&bytes, &guard::local_folders(ws));
    if !named.is_empty() {
        return Err(format!(
            "the deny scan refused {exe}: it names this machine's folder {}",
            named.join(", ")
        ));
    }
    let described = headers::check_binary(&bytes, target, Role::Server).map_err(|problems| {
        format!(
            "the header check refused {exe} for {}:\n  {}",
            target.triple,
            problems.join("\n  ")
        )
    })?;
    println!("  {exe}: {described}");

    // 6. The archive, and the deny scan of what was written.
    let mut members = Vec::new();
    for name in &allow {
        members.push(Member {
            name: name.clone(),
            bytes: read(&stage.join(name))?,
            executable: *name == exe,
        });
    }
    let archive_bytes = if target.os == Os::Windows {
        archive::zip_bytes(&root, &members, facts.epoch)?
    } else {
        archive::tar_gz_bytes(&root, &members, facts.epoch)?
    };
    let file = archive_name(target, version);
    let path = out_dir.join(&file);
    write(&path, &archive_bytes)?;
    check_archive(&path, target, version)?;
    println!(
        "  {}: {} bytes, sha256 {}",
        path.display(),
        archive_bytes.len(),
        sha256_hex(&archive_bytes)
    );

    // 7. This target's lines of MANIFEST.txt.
    let mut manifest = String::new();
    for m in &members {
        let d = if m.name == exe {
            described.as_str()
        } else {
            ""
        };
        manifest.push_str(&format!(
            "{}\t{file}/{}\t{}\t{}\t{d}\n",
            target.triple,
            m.name,
            m.bytes.len(),
            sha256_hex(&m.bytes),
        ));
    }
    write(
        &out_dir.join(format!("{file}.manifest")),
        manifest.as_bytes(),
    )
}

/// The deny scan of a written archive.
pub(crate) fn check_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let entries = archive::archive_entries(path, &archive_root(target, version))?;
    let findings = guard::scan_tree(&entries, &allowlist(target), &[], FILE_CAP, TOTAL_CAP);
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

/// The index over a release folder: every archive scanned again, then `MANIFEST.txt`,
/// `release.json` and `SHA256SUMS`.
pub(crate) fn write_index(
    ws: &Path,
    dir: &Path,
    version: &str,
    facts: &BuildFacts,
) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut archives = Vec::new();
    let mut parts = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match classify(name, version) {
            Ok(Asset::Archive(t)) => archives.push((name.clone(), t)),
            Ok(Asset::ManifestPart) => parts.push(name.clone()),
            Ok(Asset::Index) => {}
            Err(e) => refused.push(e),
        }
    }
    if !refused.is_empty() {
        return Err(format!(
            "the release folder {} holds:\n  {}",
            dir.display(),
            refused.join("\n  ")
        ));
    }
    if archives.is_empty() {
        return Err(format!("no Yalain {version} archives in {}", dir.display()));
    }
    for (name, t) in &archives {
        check_archive(&dir.join(name), *t, version)?;
    }

    let mut manifest = format!(
        "# yalain {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
        facts.commit
    );
    for p in &parts {
        manifest.push_str(&String::from_utf8_lossy(&read(&dir.join(p))?));
    }
    write(&dir.join("MANIFEST.txt"), manifest.as_bytes())?;

    let mut assets = Vec::new();
    for (name, t) in &archives {
        let bytes = read(&dir.join(name))?;
        assets.push(serde_json::json!({
            "target": t.triple, "file": name, "size": bytes.len(), "sha256": sha256_hex(&bytes),
        }));
    }
    let release = serde_json::json!({
        "schema": 2,
        "product": "yalain",
        "version": version,
        "tag": Product::Yalain.tag_for(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "source_url": facts.source_url,
        "prerelease": version::is_prerelease(version),
        "assets": assets,
        "upgrade": super::yalain_declaration::for_release(ws, version)?,
    });
    let mut json = serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?;
    json.push('\n');
    write(&dir.join("release.json"), json.as_bytes())?;

    let mut summed: Vec<String> = archives.iter().map(|(n, _)| n.clone()).collect();
    summed.extend(["MANIFEST.txt".to_owned(), "release.json".to_owned()]);
    summed.sort();
    let mut sums = String::new();
    for name in &summed {
        sums.push_str(&format!(
            "{}  {name}\n",
            sha256_hex(&read(&dir.join(name))?)
        ));
    }
    write(&dir.join("SHA256SUMS"), sums.as_bytes())?;
    println!("\nSHA256SUMS\n{sums}");
    Ok(())
}
