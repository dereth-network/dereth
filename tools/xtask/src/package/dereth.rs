//! `cargo xtask package dereth`: Dereth's release files (the launcher, with the client inside it),
//! built the same way on a contributor's machine and in the release workflow.
//!
//! ```text
//! cargo xtask package dereth [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
//! cargo xtask package dereth --gather <dir>
//! ```
//!
//! For each target (the host's own by default; each builds on its own operating system) it builds
//! the client with cargo and the launcher with the Tauri CLI, then writes into
//! `<target dir>/package/dereth-<version>/` (or `--out`):
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `dereth-<v>-windows-x86_64.zip`: `dereth.exe`, `dereth-client.exe` and the notices, under `dereth-<v>/` |
//! | macOS | `Dereth-<v>-macos-<arch>.app.tar.gz`: `Dereth.app`, the client in `Contents/MacOS`, MoltenVK in `Contents/Frameworks` |
//! | Linux | `Dereth-<v>-linux-x86_64.AppImage`, the client inside it |
//!
//! With an update-signing key in the environment ([`sign`]) the launcher's file is signed, and
//! the signature goes into its platform's piece of `latest.json`, `latest-<platform>.json`: the
//! updater reads the signature there, so no separate signature file is written. Each target's
//! lines of `MANIFEST.txt` go in `<file>.manifest`. Then the index is written over the folder:
//! `MANIFEST.txt`, `release.json`, `latest.json` (only when every launcher file is signed) and
//! `SHA256SUMS`. `--gather <dir>` builds nothing: it checks the
//! files already in `<dir>` (every runner's, in the release workflow) and writes the index over
//! them.
//!
//! **The dat guard.** Only named files are copied, and the deny scan runs over what was staged and
//! again over each written archive: a file that is not on the package's list, a game data file or
//! anything named like one, a link, or a file over the cap fails the package. An AppImage is one
//! file whose inside only the launcher's own build fills; its binaries are checked as built.
//! A file that names one of this machine's folders (the checkout, the target folder, the home
//! folder or cargo's) fails it too: the builds rename those folders, and the launcher's
//! configuration names its files relative to the app's folder ([`bundle_config`]).
//!
//! **macOS** needs MoltenVK's macOS release unpacked (`--moltenvk <dir>` or `DERETH_MOLTENVK_DIR`):
//! its `libMoltenVK.dylib` goes into the bundle, and its licence into the notices. The bundle is
//! signed ad hoc unless an Apple identity is given ([`mac_signing`]).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::archive::{self, Member};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::sign::{self, Signer};
use super::targets::{self, Os, Target};
use super::version::{self, Product};
use super::{build_facts, read, sha256_hex, write, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// The licences of everything `manifest`'s package is built from for `target`, as cargo-about's
/// page, written as `<name>.html` in `dir`.
pub(super) fn licence_page(
    ws: &Path,
    manifest: &str,
    target: Target,
    dir: &Path,
    name: &str,
) -> Result<String, String> {
    let out = dir.join(format!("{name}.html"));
    let out_arg = out.display().to_string();
    run_in(
        ws,
        "cargo",
        &[
            "about",
            "generate",
            "--locked",
            "--fail",
            "-m",
            manifest,
            "-c",
            ABOUT_CONFIG,
            "--target",
            target.triple,
            "-o",
            &out_arg,
            ABOUT_TEMPLATE,
        ],
        &[],
        &[],
    )?;
    Ok(String::from_utf8_lossy(&read(&out)?).into_owned())
}

/// The launcher's Tauri app, a workspace of its own.
pub const LAUNCHER_DIR: &str = "dereth/launcher";

/// cargo-about's configuration and template for Dereth.
const ABOUT_CONFIG: &str = "dereth/about.toml";
const ABOUT_TEMPLATE: &str = "dereth/about.hbs";

/// The typefaces the package carries: (typeface, what carries it and for what, its licence).
pub const FONTS: &[(&str, &str, &str)] = &[
    (
        "Cinzel",
        "The launcher's page is set in Cinzel, which the launcher carries",
        "dereth/launcher/assets/fonts/OFL-Cinzel.txt",
    ),
    (
        "EB Garamond",
        "The launcher's page is set in EB Garamond, which the launcher carries",
        "dereth/launcher/assets/fonts/OFL-EBGaramond.txt",
    ),
    (
        "Liberation",
        "The classic interface's text is drawn in the Liberation fonts (Liberation Serif, Mono and\n\
         Sans, version 2.1.5), which the client carries",
        "dereth/client/crates/classic-fonts/fonts/OFL.txt",
    ),
];

/// The notices every package carries beside its programs.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// No single file of a Dereth package is larger than this: the AppImage carries the web view's
/// libraries. The smallest retail data file is hundreds of megabytes more.
pub const FILE_CAP: u64 = 320 << 20;

/// No package holds more than this in all.
pub const TOTAL_CAP: u64 = 512 << 20;

/// Where MoltenVK's unpacked macOS release is, when `--moltenvk` does not say.
pub const MOLTENVK_ENV: &str = "DERETH_MOLTENVK_DIR";

/// The public repository the release files are downloaded from, when the default is not it.
pub const SOURCE_URL_ENV: &str = "DERETH_BUILD_SOURCE_URL";

/// The macOS bundle's name.
pub const LAUNCHER_APP: &str = "Dereth.app";

/// The release file for `target`: the launcher, with the client inside it.
pub fn release_file_name(target: Target, version: &str) -> String {
    let p = target.platform_name();
    match target.os {
        Os::Windows => format!("dereth-{version}-{p}.zip"),
        Os::Mac => format!("Dereth-{version}-{p}.app.tar.gz"),
        Os::Linux => format!("Dereth-{version}-{p}.AppImage"),
    }
}

/// The one top-level folder of the release file's archive; `None` for the AppImage, which is no
/// archive.
pub fn archive_root(target: Target, version: &str) -> Option<String> {
    match target.os {
        Os::Windows => Some(format!("dereth-{version}")),
        Os::Mac => Some(LAUNCHER_APP.to_owned()),
        Os::Linux => None,
    }
}

/// The files the release file's archive holds below its top-level folder: those it must hold, and
/// those it may (what the bundler writes on some machines and not others). Empty for the AppImage.
pub fn contents(target: Target) -> (Vec<String>, Vec<String>) {
    let exe = target.exe_suffix();
    let own = |names: &[&str]| -> Vec<String> { names.iter().map(|n| (*n).to_owned()).collect() };
    let notices_in =
        |dir: &str| -> Vec<String> { NOTICES.iter().map(|n| format!("{dir}{n}")).collect() };
    let mut required = Vec::new();
    let mut optional = Vec::new();
    match target.os {
        Os::Linux => {}
        Os::Windows => {
            required.push(format!("dereth{exe}"));
            required.push(format!("dereth-client{exe}"));
            required.extend(notices_in(""));
        }
        Os::Mac => {
            required.extend(own(&[
                "Contents/Info.plist",
                "Contents/MacOS/dereth",
                "Contents/MacOS/dereth-client",
                "Contents/Frameworks/libMoltenVK.dylib",
            ]));
            required.extend(notices_in("Contents/Resources/"));
            // The bundler names the icon after the product, or `icon`, by version.
            optional.extend(own(&[
                "Contents/PkgInfo",
                "Contents/Resources/Assets.car",
                "Contents/Resources/Dereth.icns",
                "Contents/Resources/icon.icns",
                "Contents/_CodeSignature/CodeResources",
            ]));
        }
    }
    required.sort();
    optional.sort();
    (required, optional)
}

/// The programs in the release file's archive, which the header checks read.
pub fn programs(target: Target) -> Vec<String> {
    let exe = target.exe_suffix();
    let dir = if target.os == Os::Mac {
        "Contents/MacOS/"
    } else {
        ""
    };
    vec![
        format!("{dir}dereth{exe}"),
        format!("{dir}dereth-client{exe}"),
    ]
}

/// The name of a target's piece of `latest.json`.
pub fn piece_name(target: Target) -> String {
    format!("latest-{}.json", target.updater_platform())
}

/// What a file of a Dereth release folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    /// The release file: the launcher, with the client inside it.
    Launcher(Target),
    /// One file's lines of `MANIFEST.txt`.
    ManifestPart,
    /// One platform's entry of `latest.json`.
    Piece(Target),
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
const INDEX_FILES: &[&str] = &["SHA256SUMS", "release.json", "MANIFEST.txt", "latest.json"];

/// Classify a file of the release folder, or refuse it: the folder holds this version's release
/// files, their manifest parts and `latest.json` pieces, and the index.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    for triple in targets::DERETH_TRIPLES {
        let t = targets::find(triple)?;
        if name == piece_name(t) {
            return Ok(Asset::Piece(t));
        }
        let file = release_file_name(t, version);
        if name == file {
            return Ok(Asset::Launcher(t));
        }
        if name == format!("{file}.manifest") {
            return Ok(Asset::ManifestPart);
        }
    }
    Err(format!(
        "`{name}` is not a file of the Dereth {version} release (a launcher file, its .manifest, a \
         latest-<platform>.json piece or the index)"
    ))
}

/// The options of one `package dereth` run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
    moltenvk: Option<PathBuf>,
}

pub const USAGE: &str = "usage: cargo xtask package dereth [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
       cargo xtask package dereth --gather <dir>";

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
            "--moltenvk" => o.moltenvk = Some(PathBuf::from(value()?)),
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    if o.gather.is_some() && (!o.targets.is_empty() || o.out.is_some() || o.moltenvk.is_some()) {
        return Err(format!(
            "--gather builds nothing: it takes no --target, --out or --moltenvk\n{USAGE}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package dereth ...` (the arguments after `dereth`).
pub fn package(args: &[String]) -> i32 {
    let options = match parse_options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = workspace_root();
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
            build_packages(
                &ws,
                &triples,
                options.out.as_deref(),
                options.allow_dirty,
                options.moltenvk.as_deref(),
            )
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
fn dereth_facts(ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    let mut facts = build_facts(ws, allow_dirty)?;
    if let Ok(url) = std::env::var(SOURCE_URL_ENV) {
        if !url.trim().is_empty() {
            facts.source_url = url.trim().trim_end_matches('/').to_owned();
        }
    }
    Ok(facts)
}

/// MoltenVK's dylib and licence in its unpacked macOS release: `dir` is the release's top-level
/// `MoltenVK` folder, or the folder that holds it.
pub fn moltenvk_files(dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    for base in [dir.to_path_buf(), dir.join("MoltenVK")] {
        let dylib = base.join("MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib");
        let licence = base.join("LICENSE");
        if dylib.is_file() && licence.is_file() {
            return Ok((dylib, licence));
        }
    }
    Err(format!(
        "{} is not MoltenVK's unpacked macOS release (MoltenVK-macos.tar from \
         https://github.com/KhronosGroup/MoltenVK/releases): no \
         MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib and LICENSE in it",
        dir.display()
    ))
}

/// Whether `bytes` begin as a Mach-O library: 64-bit, or universal.
pub fn is_macho_library(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some([0xCF, 0xFA, 0xED, 0xFE] | [0xCA, 0xFE, 0xBA, 0xBE])
    )
}

fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Build, check and write every target's files, then the index. The folder they are in.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
    moltenvk: Option<&Path>,
) -> Result<PathBuf, String> {
    let version = Product::Dereth.version(ws)?;
    version::parse_release_version(&version)?;
    // Everything a host lacks is said before anything is built.
    let host = super::host();
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    for triple in triples {
        match targets::find_dereth(triple).and_then(|t| targets::plan_dereth(t, &host).map(|()| t))
        {
            Ok(t) => plans.push(t),
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
    if !succeeds("cargo", &["tauri", "--version"]) {
        refusals.push(
            "  the Tauri CLI is not installed, or does not run on this host; it builds the launcher: \
             `cargo install --locked tauri-cli --version ^2`"
                .to_owned(),
        );
    }
    let mut moltenvk_found = None;
    if plans.iter().any(|t| t.os == Os::Mac) {
        let dir = moltenvk
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(MOLTENVK_ENV).map(PathBuf::from));
        match dir {
            Some(d) => match moltenvk_files(&d) {
                Ok(files) => moltenvk_found = Some(files),
                Err(e) => refusals.push(format!("  {e}")),
            },
            None => refusals.push(format!(
                "  a macOS package carries MoltenVK: unpack MoltenVK-macos.tar from \
                 https://github.com/KhronosGroup/MoltenVK/releases and pass --moltenvk <dir> \
                 (or set {MOLTENVK_ENV})"
            )),
        }
    }
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let signer = Signer::from_env()?;
    match &signer {
        Some(_) => println!(
            "package: the launcher's update files are signed ({})",
            sign::KEY_VAR
        ),
        None => println!(
            "package: NOT SIGNED: {} is not set, so the launcher's files get no update signature \
             and no latest.json is written; a release needs the key",
            sign::KEY_VAR
        ),
    }
    let facts = dereth_facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("dereth-{version}")));
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    for target in plans {
        println!("\n=== dereth {version} for {}", target.triple);
        let job = Job {
            ws,
            target,
            facts: &facts,
            version: &version,
            out_dir: &out_dir,
            stage: &base.join("stage").join(format!("dereth-{}", target.triple)),
            signer: signer.as_ref(),
            moltenvk: moltenvk_found.as_ref(),
        };
        job.run()?;
    }
    write_index(ws, &out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the files there and write the index over them.
fn gather(ws: &Path, dir: &Path, allow_dirty: bool) -> Result<PathBuf, String> {
    let version = Product::Dereth.version(ws)?;
    let facts = dereth_facts(ws, allow_dirty)?;
    write_index(ws, dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

/// The environment a release build runs with: the build stamp, the repository the launcher looks
/// for its updates in (the one the release files are downloaded from), and the flags that keep
/// local paths out of the binaries. On Windows the C runtime is linked in, so neither program needs a
/// Visual C++ redistributable, and the linker writes no time.
pub fn build_env(ws: &Path, facts: &BuildFacts, target: Target) -> Vec<(String, String)> {
    let mut flags: Vec<String> = super::local_remaps(ws)
        .iter()
        .map(|(from, to)| format!("--remap-path-prefix={}={to}", from.display()))
        .collect();
    if target.os == Os::Windows {
        flags.push("-Ctarget-feature=+crt-static".to_owned());
        flags.push("-Clink-arg=/Brepro".to_owned());
    }
    let mut env = vec![
        ("CARGO_ENCODED_RUSTFLAGS".to_owned(), flags.join("\u{1f}")),
        ("SOURCE_DATE_EPOCH".to_owned(), facts.epoch.to_string()),
        ("DERETH_BUILD_COMMIT".to_owned(), facts.commit.clone()),
        ("DERETH_BUILD_TARGET".to_owned(), target.triple.to_owned()),
        (SOURCE_URL_ENV.to_owned(), facts.source_url.clone()),
    ];
    if target.os == Os::Mac {
        env.push((
            "MACOSX_DEPLOYMENT_TARGET".to_owned(),
            format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1),
        ));
    }
    env
}

/// Run `program` in `dir`, with `env` added and `remove` taken out of the environment.
fn run_in(
    dir: &Path,
    program: &str,
    args: &[&str],
    env: &[(String, String)],
    remove: &[&str],
) -> Result<(), String> {
    println!("$ {program} {}", args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(dir);
    for r in remove {
        cmd.env_remove(r);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("{program} could not be run: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{program} {}` failed ({status})", args.join(" ")))
    }
}

fn mkdir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

fn fresh_dir(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    mkdir(dir)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        mkdir(parent)?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// Whether a file on disk is executable (always false where there is no such bit).
fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// The crates `cargo tree` says the package in `manifest` is built from, for `target`.
pub(super) fn crates_of(
    ws: &Path,
    manifest: &str,
    package: &str,
    target: Target,
) -> Result<Vec<notice::Crate>, String> {
    let tree = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "-e",
            "normal",
            "--target",
            target.triple,
        ])
        .args(["--manifest-path", manifest, "-p", package])
        .args(["--prefix", "none", "--format", "{p}|{l}"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo tree could not be run: {e}"))?;
    if !tree.status.success() {
        return Err(format!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&tree.stderr)
        ));
    }
    Ok(notice::parse_cargo_tree(&String::from_utf8_lossy(
        &tree.stdout,
    )))
}

/// One target's build.
struct Job<'a> {
    ws: &'a Path,
    target: Target,
    facts: &'a BuildFacts,
    version: &'a str,
    out_dir: &'a Path,
    /// This target's staging folder, made anew.
    stage: &'a Path,
    signer: Option<&'a Signer>,
    /// MoltenVK's dylib and licence (macOS).
    moltenvk: Option<&'a (PathBuf, PathBuf)>,
}

impl Job<'_> {
    fn run(&self) -> Result<(), String> {
        fresh_dir(self.stage)?;
        let client = self.build_client()?;
        let notices = self.notices()?;
        let launcher = self.build_launcher(&client, &notices)?;
        match self.target.os {
            Os::Windows => {
                let mut members = vec![
                    ("dereth.exe".to_owned(), launcher),
                    ("dereth-client.exe".to_owned(), client),
                ];
                members.extend(NOTICES.iter().map(|n| ((*n).to_owned(), notices.join(n))));
                self.write_archive(&members)
            }
            Os::Linux => self.write_appimage(&launcher, &client),
            Os::Mac => self.write_bundle(&launcher),
        }
    }

    /// The client, built with cargo in the workspace.
    fn build_client(&self) -> Result<PathBuf, String> {
        let env = build_env(self.ws, self.facts, self.target);
        run_in(
            self.ws,
            "cargo",
            &[
                "build",
                "--locked",
                "--release",
                "-p",
                "dereth-client",
                "--target",
                self.target.triple,
            ],
            &env,
            &[],
        )?;
        let built = target_dir()
            .join(self.target.triple)
            .join("release")
            .join(format!("dereth-client{}", self.target.exe_suffix()));
        // A copy of its own, so a later build in the same target folder cannot change what this
        // package holds.
        let staged = self.stage.join("bin").join(format!(
            "dereth-client-{}{}",
            self.target.triple,
            self.target.exe_suffix()
        ));
        copy(&built, &staged)?;
        Ok(staged)
    }

    /// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: the folder they are written in.
    fn notices(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let launcher_manifest = format!("{LAUNCHER_DIR}/Cargo.toml");
        let mut all_crates = crates_of(self.ws, &launcher_manifest, "dereth-launcher", t)?;
        all_crates.extend(crates_of(self.ws, "Cargo.toml", "dereth-client", t)?);
        all_crates.sort();
        all_crates.dedup();
        let mit = String::from_utf8_lossy(&read(&self.ws.join("LICENSE"))?).into_owned();
        let mut fonts = Vec::new();
        for (name, carried, path) in FONTS {
            fonts.push((
                *name,
                *carried,
                String::from_utf8_lossy(&read(&self.ws.join(path))?).into_owned(),
            ));
        }
        let moltenvk = match self.moltenvk {
            Some((_, licence)) => Some(String::from_utf8_lossy(&read(licence)?).into_owned()),
            None => None,
        };

        let about = self.stage.join("about");
        mkdir(&about)?;
        let page = |manifest: &str, name: &str| licence_page(self.ws, manifest, t, &about, name);
        let exe = t.exe_suffix();
        let launcher_page = page(&launcher_manifest, "dereth")?;
        let client_page = page("dereth/client/Cargo.toml", "dereth-client")?;

        let dir = self.stage.join("notices");
        mkdir(&dir)?;
        let text = notice::dereth_notice(&notice::DerethFacts {
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &self.facts.source_url,
            crates: &all_crates,
            mit_licence: &mit,
            fonts: &fonts,
            moltenvk_licence: moltenvk.as_deref(),
        });
        write(&dir.join("NOTICE.txt"), text.as_bytes())?;
        write(&dir.join("LICENSE"), mit.as_bytes())?;
        let client_name = format!("dereth-client{exe}");
        let launcher_name = format!("dereth{exe}");
        let pages = [
            (launcher_name.as_str(), launcher_page),
            (client_name.as_str(), client_page),
        ];
        write(
            &dir.join("THIRD-PARTY-LICENSES.html"),
            notice::splice_licence_pages(&pages)?.as_bytes(),
        )?;
        Ok(dir)
    }

    /// The launcher, built with the Tauri CLI: `dereth.exe` on Windows, `Dereth.app` on macOS
    /// (with the client and MoltenVK inside), the AppImage on Linux (with the client inside).
    fn build_launcher(&self, client: &Path, notices: &Path) -> Result<PathBuf, String> {
        let t = self.target;
        let tauri_dir = self.ws.join(LAUNCHER_DIR);
        let cargo_target = target_dir().join("launcher");
        let release = cargo_target.join(t.triple).join("release");
        let bundle_dir = release.join("bundle");
        if bundle_dir.exists() {
            std::fs::remove_dir_all(&bundle_dir)
                .map_err(|e| format!("{}: {e}", bundle_dir.display()))?;
        }
        // What the release adds to the app's configuration: the client beside the launcher (the
        // bundler takes it as `<name>-<target>` and drops the suffix), the notices, and on macOS
        // MoltenVK, signed with the bundle ([`mac_signing`]). The launcher carries its whole
        // configuration inside it, so these are named relative to the app's folder, never by
        // where this machine keeps them ([`bundle_config`]).
        let client_base = client
            .parent()
            .unwrap_or(Path::new("."))
            .join("dereth-client");
        let mut bundle = bundle_config(t, &tauri_dir, &client_base, notices)?;
        if t.os == Os::Mac {
            let (dylib, _) = self.moltenvk.ok_or("a macOS launcher needs MoltenVK")?;
            let identity = std::env::var("APPLE_SIGNING_IDENTITY").ok();
            bundle["macOS"] = mac_signing(relative_to(&tauri_dir, dylib)?, identity.as_deref());
        }
        // On Windows the whole C runtime is linked in (`crt-static`, as the client links it),
        // rather than the app builder's own mix of a static Visual C++ runtime over the system's
        // dynamic one.
        let config = serde_json::json!({
            "bundle": bundle,
            "build": { "windows": { "staticVCRuntime": false } },
        });
        let config_path = self.stage.join("tauri-release.json");
        write(
            &config_path,
            serde_json::to_string_pretty(&config)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )?;
        let config_arg = config_path.display().to_string();
        let mut args = vec![
            "tauri",
            "build",
            "--ci",
            "--target",
            t.triple,
            "--config",
            &config_arg,
        ];
        match t.os {
            Os::Windows => args.push("--no-bundle"),
            Os::Mac => args.extend(["--bundles", "app"]),
            Os::Linux => args.extend(["--bundles", "appimage"]),
        }
        args.extend(["--", "--locked"]);
        let mut env = build_env(self.ws, self.facts, t);
        env.push((
            "CARGO_TARGET_DIR".to_owned(),
            cargo_target.display().to_string(),
        ));
        // The bundler signs nothing itself: the release's signatures are made below, from the
        // files as they are published.
        run_in(
            &tauri_dir,
            "cargo",
            &args,
            &env,
            &[sign::KEY_VAR, sign::PASSWORD_VAR],
        )?;
        match t.os {
            Os::Windows => Ok(release.join("dereth.exe")),
            Os::Mac => Ok(bundle_dir.join("macos").join(LAUNCHER_APP)),
            Os::Linux => {
                let dir = bundle_dir.join("appimage");
                let found: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .map_err(|e| format!("{}: {e}", dir.display()))?
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "AppImage"))
                    .collect();
                match found.as_slice() {
                    [one] => Ok(one.clone()),
                    _ => Err(format!(
                        "expected one AppImage in {}, found {}",
                        dir.display(),
                        found.len()
                    )),
                }
            }
        }
    }

    /// Check a program for the target, by its name in a package.
    fn check_program(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        headers::check_app_binary(bytes, self.target, targets::DERETH_GLIBC_FLOOR).map_err(
            |problems| {
                format!(
                    "the header check refused {name} for {}:\n  {}",
                    self.target.triple,
                    problems.join("\n  ")
                )
            },
        )
    }

    /// A flat archive of `members` (name in the package, file on disk), scanned, checked,
    /// written and scanned again.
    fn write_archive(&self, members: &[(String, PathBuf)]) -> Result<(), String> {
        let root = archive_root(self.target, self.version).ok_or("not an archive")?;
        let stage = self.stage.join(&root);
        fresh_dir(&stage)?;
        for (name, from) in members {
            copy(from, &stage.join(name))?;
        }
        self.finish_tree(&stage, &root)
    }

    /// A bundle on disk, archived as it is.
    fn write_bundle(&self, app: &Path) -> Result<(), String> {
        let root = archive_root(self.target, self.version).ok_or("not an archive")?;
        self.finish_tree(app, &root)
    }

    /// Scan the folder `dir`, check its programs, archive it under `root`, scan the archive, sign
    /// it, and write its manifest lines.
    fn finish_tree(&self, dir: &Path, root: &str) -> Result<(), String> {
        let t = self.target;
        let (required, optional) = contents(t);
        let staged = guard::staged_entries(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let findings = guard::scan_tree(&staged, &required, &optional, FILE_CAP, TOTAL_CAP);
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                dir.display(),
                findings.join("\n  ")
            ));
        }
        let mut described = Vec::new();
        for name in programs(t) {
            let d = self.check_program(&name, &read(&dir.join(&name))?)?;
            println!("  {name}: {d}");
            described.push((name, d));
        }
        if t.os == Os::Mac {
            let name = "Contents/Frameworks/libMoltenVK.dylib";
            if !is_macho_library(&read(&dir.join(name))?) {
                return Err(format!("{name} is not a Mach-O library"));
            }
        }
        let mut members = Vec::new();
        for e in &staged {
            if e.kind == Kind::File {
                let path = dir.join(&e.path);
                members.push(Member {
                    name: e.path.clone(),
                    bytes: read(&path)?,
                    executable: executable(&path) || programs(t).contains(&e.path),
                });
            }
        }
        let named: Vec<(&str, &[u8])> = members
            .iter()
            .map(|m| (m.name.as_str(), m.bytes.as_slice()))
            .collect();
        self.refuse_machine_folders(dir, &named)?;
        let bytes = if t.os == Os::Windows {
            archive::zip_bytes(root, &members, self.facts.epoch)?
        } else {
            archive::tar_gz_bytes(root, &members, self.facts.epoch)?
        };
        let file = release_file_name(t, self.version);
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        check_archive(&path, t, self.version)?;
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        let lines: Vec<(String, Vec<u8>, String)> = members
            .into_iter()
            .map(|m| {
                let d = described
                    .iter()
                    .find(|(n, _)| *n == m.name)
                    .map_or(String::new(), |(_, d)| d.clone());
                (m.name, m.bytes, d)
            })
            .collect();
        self.write_manifest(&file, &lines)?;
        self.sign_launcher(&file, &bytes)
    }

    /// The AppImage, as the bundler wrote it, with the programs that went into it checked.
    fn write_appimage(&self, appimage: &Path, client: &Path) -> Result<(), String> {
        let t = self.target;
        let launcher = target_dir()
            .join("launcher")
            .join(t.triple)
            .join("release")
            .join("dereth");
        let mut lines = Vec::new();
        for (name, path) in [("dereth", launcher.as_path()), ("dereth-client", client)] {
            let bytes = read(path)?;
            let d = self.check_program(name, &bytes)?;
            println!("  {name}: {d}");
            lines.push((name.to_owned(), bytes, d));
        }
        // The AppImage compresses what it holds, so the two programs are looked at as built.
        let named: Vec<(&str, &[u8])> = lines
            .iter()
            .map(|(n, b, _)| (n.as_str(), b.as_slice()))
            .collect();
        self.refuse_machine_folders(appimage, &named)?;
        let bytes = read(appimage)?;
        let file = release_file_name(t, self.version);
        let findings = check_appimage(&file, &bytes);
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                appimage.display(),
                findings.join("\n  ")
            ));
        }
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        lines.insert(0, (String::new(), bytes.clone(), "AppImage".to_owned()));
        self.write_manifest(&file, &lines)?;
        self.sign_launcher(&file, &bytes)
    }

    /// Refuse the package at `what` when any of `files` (name in the package, contents) names one
    /// of this machine's folders: the checkout, the target folder, the home folder or cargo's.
    fn refuse_machine_folders(&self, what: &Path, files: &[(&str, &[u8])]) -> Result<(), String> {
        let folders = guard::local_folders(self.ws);
        let findings: Vec<String> = files
            .iter()
            .flat_map(|(name, bytes)| {
                guard::local_paths_in(bytes, &folders)
                    .into_iter()
                    .map(move |f| format!("{name}: names this machine's folder {f}"))
            })
            .collect();
        if findings.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "the deny scan refused {}:\n  {}",
                what.display(),
                findings.join("\n  ")
            ))
        }
    }

    /// `<file>.manifest`: each member's size, SHA-256 and what the header check read.
    fn write_manifest(
        &self,
        file: &str,
        lines: &[(String, Vec<u8>, String)],
    ) -> Result<(), String> {
        let mut text = String::new();
        for (name, bytes, d) in lines {
            let shown = if name.is_empty() {
                file.to_owned()
            } else {
                format!("{file}/{name}")
            };
            text.push_str(&format!(
                "{}\t{shown}\t{}\t{}\t{d}\n",
                self.target.triple,
                bytes.len(),
                sha256_hex(bytes)
            ));
        }
        write(
            &self.out_dir.join(format!("{file}.manifest")),
            text.as_bytes(),
        )
    }

    /// With a key: the launcher file's signature, verified as the launcher's updater verifies it,
    /// in this platform's piece of `latest.json`. Without one, no piece, and one left from an
    /// earlier run is removed.
    fn sign_launcher(&self, file: &str, bytes: &[u8]) -> Result<(), String> {
        let t = self.target;
        let piece_path = self.out_dir.join(piece_name(t));
        let Some(signer) = self.signer else {
            let _ = std::fs::remove_file(&piece_path);
            return Ok(());
        };
        let signature = signer.sign(bytes, file, self.version, self.facts.epoch)?;
        let public_key = launcher_public_key(self.ws)?;
        sign::verify(bytes, &signature, &public_key, self.version).map_err(|e| {
            format!(
                "the update signature of {file} does not verify against the launcher's public key \
                 (plugins.updater.pubkey): {e}; is {} the launcher's key?",
                sign::KEY_VAR
            )
        })?;
        let url = download_url(&self.facts.source_url, self.version, file);
        let piece = sign::platform_piece(&t.updater_platform(), &url, &signature);
        write(
            &piece_path,
            serde_json::to_string_pretty(&piece)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )?;
        println!("  {file}: signed; {} written", piece_name(t));
        Ok(())
    }
}

/// Where a release file is downloaded from.
pub fn download_url(source_url: &str, version: &str, file: &str) -> String {
    format!(
        "{}/releases/download/{}/{file}",
        source_url.trim_end_matches('/'),
        Product::Dereth.tag_for(version)
    )
}

/// The launcher's update-signing public key, from its configuration.
pub(crate) fn launcher_public_key(ws: &Path) -> Result<String, String> {
    let conf = ws.join(LAUNCHER_DIR).join("tauri.conf.json");
    sign::launcher_public_key(&String::from_utf8_lossy(&read(&conf)?))
}

/// What the deny scan finds in an AppImage: its name, the data-file header, the cap, and that it
/// is an executable at all.
pub fn check_appimage(name: &str, bytes: &[u8]) -> Vec<String> {
    let mut findings = Vec::new();
    if let Some(why) = guard::name_finding(name) {
        findings.push(format!("{name}: {why}"));
    }
    if guard::has_dat_magic(&bytes[..bytes.len().min(guard::HEAD_LEN)]) {
        findings.push(format!("{name}: carries the game's data-file header"));
    }
    if bytes.len() as u64 > FILE_CAP {
        findings.push(format!(
            "{name}: {} bytes, over the {FILE_CAP} byte cap for one file",
            bytes.len()
        ));
    }
    if bytes.get(..4) != Some(b"\x7fELF") {
        findings.push(format!("{name}: not an executable (no ELF header)"));
    }
    findings
}

/// The deny scan of a written release file.
pub(crate) fn check_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let findings = match archive_root(target, version) {
        None => check_appimage(&name, &read(path)?),
        Some(root) => {
            let entries: Vec<Entry> = archive::archive_entries(path, &root)?;
            let (required, optional) = contents(target);
            guard::scan_tree(&entries, &required, &optional, FILE_CAP, TOTAL_CAP)
        }
    };
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

/// The index over a release folder: every release file scanned again and every signature
/// verified again, then `MANIFEST.txt`, `release.json`, `latest.json` (when every launcher file
/// is signed) and `SHA256SUMS`.
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
    let mut launchers = Vec::new();
    let mut parts = Vec::new();
    let mut pieces = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match classify(name, version) {
            Ok(Asset::Launcher(t)) => launchers.push((name.clone(), t)),
            Ok(Asset::ManifestPart) => parts.push(name.clone()),
            Ok(Asset::Piece(t)) => pieces.push((name.clone(), t)),
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
    if launchers.is_empty() {
        return Err(format!(
            "no Dereth {version} release files in {}",
            dir.display()
        ));
    }
    for (name, t) in &launchers {
        check_archive(&dir.join(name), *t, version)?;
    }

    // latest.json: every launcher file signed, each signature good, one piece per platform.
    let public_key = launcher_public_key(ws)?;
    let mut piece_values = Vec::new();
    let mut unsigned = Vec::new();
    for (name, t) in &launchers {
        let piece = pieces.iter().find(|(_, pt)| pt == t);
        let Some((piece_file, _)) = piece else {
            unsigned.push(name.clone());
            continue;
        };
        let value: serde_json::Value = serde_json::from_slice(&read(&dir.join(piece_file))?)
            .map_err(|e| format!("{piece_file}: {e}"))?;
        let entry = &value[t.updater_platform()];
        let signature = entry["signature"]
            .as_str()
            .ok_or_else(|| format!("{piece_file} has no signature"))?;
        let url = entry["url"].as_str().unwrap_or_default();
        if url != download_url(&facts.source_url, version, name) {
            return Err(format!(
                "{piece_file} points at {url}, not at {}",
                download_url(&facts.source_url, version, name)
            ));
        }
        sign::verify(&read(&dir.join(name))?, signature, &public_key, version)
            .map_err(|e| format!("{name}: {e}"))?;
        piece_values.push(value);
    }
    for (piece_file, t) in &pieces {
        if !launchers.iter().any(|(_, lt)| lt == t) {
            return Err(format!("{piece_file} has no launcher file beside it"));
        }
    }
    let latest_path = dir.join("latest.json");
    let signed = unsigned.is_empty();
    if signed {
        let latest = sign::merge_latest(
            version,
            &format!("Dereth {version}"),
            &facts.iso_time(),
            &piece_values,
        )?;
        let mut json = serde_json::to_string_pretty(&latest).map_err(|e| e.to_string())?;
        json.push('\n');
        write(&latest_path, json.as_bytes())?;
    } else {
        let _ = std::fs::remove_file(&latest_path);
        println!(
            "\nNOT SIGNED: {} carries no update signature, so no latest.json was written; a \
             release needs {}",
            unsigned.join(", "),
            sign::KEY_VAR
        );
    }

    let mut manifest = format!(
        "# dereth {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
        facts.commit
    );
    for p in &parts {
        manifest.push_str(&String::from_utf8_lossy(&read(&dir.join(p))?));
    }
    write(&dir.join("MANIFEST.txt"), manifest.as_bytes())?;

    let mut assets = Vec::new();
    for (name, t) in &launchers {
        let bytes = read(&dir.join(name))?;
        assets.push(serde_json::json!({
            "program": "launcher",
            "target": t.triple,
            "platform": t.updater_platform(),
            "file": name,
            "size": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "signed": !unsigned.contains(name),
        }));
    }
    let release = serde_json::json!({
        "schema": 1,
        "product": "dereth",
        "version": version,
        "tag": Product::Dereth.tag_for(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "source_url": facts.source_url,
        "prerelease": version::is_prerelease(version),
        "updates_signed": signed,
        "assets": assets,
    });
    let mut json = serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?;
    json.push('\n');
    write(&dir.join("release.json"), json.as_bytes())?;

    let mut summed: Vec<String> = launchers.iter().map(|(n, _)| n.clone()).collect();
    summed.extend(["MANIFEST.txt".to_owned(), "release.json".to_owned()]);
    if signed {
        summed.push("latest.json".to_owned());
    }
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

/// The bundle section of the launcher's release configuration for `target`: the client, as the
/// base name the bundler adds `-<target>` to, and the notices.
///
/// The launcher is built with its whole configuration compiled in, so a folder named here is
/// carried by the program. Each is named relative to the app's folder `app_dir`, as the bundler
/// reads it, so no machine's folder is. On Windows the launcher is not bundled (the archive is
/// assembled from the files themselves, and the launcher finds the client beside it by name), so
/// neither is named at all.
pub fn bundle_config(
    target: Target,
    app_dir: &Path,
    client_base: &Path,
    notices: &Path,
) -> Result<serde_json::Value, String> {
    if target.os == Os::Windows {
        return Ok(serde_json::json!({ "createUpdaterArtifacts": false }));
    }
    let mut resources = serde_json::Map::new();
    for n in NOTICES {
        resources.insert(
            relative_to(app_dir, &notices.join(n))?,
            serde_json::Value::from(*n),
        );
    }
    Ok(serde_json::json!({
        "externalBin": [relative_to(app_dir, client_base)?],
        "resources": resources,
        "createUpdaterArtifacts": false,
    }))
}

/// `path` relative to the folder `base`, `/`-separated: `..` for each of `base`'s folders below
/// what the two share. Both are absolute; a path on another drive has no such name.
pub fn relative_to(base: &Path, path: &Path) -> Result<String, String> {
    use std::path::Component;
    let parts = |p: &Path| -> Vec<String> {
        p.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect()
    };
    let (b, p) = (parts(base), parts(path));
    let same = |x: &str, y: &str| {
        if cfg!(windows) {
            x.eq_ignore_ascii_case(y)
        } else {
            x == y
        }
    };
    let shared = b.iter().zip(&p).take_while(|(x, y)| same(x, y)).count();
    if shared == 0 || !base.is_absolute() || !path.is_absolute() {
        return Err(format!(
            "{} has no name relative to {}",
            path.display(),
            base.display()
        ));
    }
    let mut out: Vec<&str> = vec![".."; b.len() - shared];
    out.extend(p[shared..].iter().map(String::as_str));
    Ok(out.join("/"))
}

/// The macOS signing the launcher's bundle gets, with MoltenVK among its frameworks.
///
/// With a Developer ID (`identity`, Tauri's `APPLE_SIGNING_IDENTITY`) the bundle and every
/// framework in it are signed with that identity under the hardened runtime: its library
/// validation admits MoltenVK because it now carries the same team. Without one the bundle is
/// signed ad hoc, and the hardened runtime stays off: an ad-hoc signature has no team, so library
/// validation would refuse MoltenVK, and the hardened runtime means nothing without notarisation.
pub fn mac_signing(framework: String, identity: Option<&str>) -> serde_json::Value {
    match identity
        .map(str::trim)
        .filter(|i| !i.is_empty() && *i != "-")
    {
        Some(id) => serde_json::json!({
            "frameworks": [framework],
            "signingIdentity": id,
            "hardenedRuntime": true,
        }),
        None => serde_json::json!({
            "frameworks": [framework],
            "signingIdentity": "-",
            "hardenedRuntime": false,
        }),
    }
}
