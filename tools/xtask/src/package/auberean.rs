//! `cargo xtask package auberean`: Auberean's release files. Each is the launcher Dereth's
//! package carries, with the Auberean client beside it in place of Dereth's, and a small defaults
//! file that opens a first run in Auberean's realm and takes the launcher's updates from
//! Auberean's releases.
//!
//! ```text
//! cargo xtask package auberean [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
//! cargo xtask package auberean --gather <dir>
//! ```
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `auberean-<v>-windows-x86_64.zip`: `dereth.exe`, `auberean.exe`, `launcher-defaults.json` and the notices, under `auberean-<v>/` |
//! | macOS | `Auberean-<v>-macos-<arch>.app.tar.gz`: `Dereth.app`, with `auberean` beside the launcher in `Contents/MacOS`, and `launcher-defaults.json` and the notices in `Contents/Resources` |
//! | Linux | `Auberean-<v>-linux-x86_64.AppImage`: the launcher's AppImage, with `auberean` and `launcher-defaults.json` in its `usr/bin` |
//!
//! **One launcher.** There is no launcher of Auberean's own. The launcher is built exactly as
//! `package dereth` builds it ([`super::dereth::build_launcher_as_dereth`]): the same command,
//! configuration and environment from the same commit, at the launcher's own version (Dereth's),
//! so the two packages carry the same launcher program, byte for byte (on macOS, until each bundle
//! is signed). That build takes Dereth's client and tool in, as Dereth's package has them; here
//! they are taken out again, with MoltenVK on macOS (Auberean draws through Metal), and the
//! Auberean client, the defaults file ([`DEFAULTS`]) and Auberean's notices put in. The macOS
//! bundle is signed again; the Linux AppImage is unpacked and packed again with `appimagetool`.
//!
//! **Updates.** Each release file is signed with the launcher's update key, like Dereth's, and its
//! piece of `latest.json` names the launcher it holds by that launcher's version, for the installs
//! that read this release's `latest.json` before the launcher had releases of its own. They move to
//! the launcher's stream by installing it. Nothing else reads it.
//!
//! The dat guard, the header checks, the manifest lines and the index are those of Dereth's
//! package.

use std::path::{Path, PathBuf};

use super::archive::{self, Member};
use super::dereth::{self, fresh_dir, mkdir, run_in, BuiltLauncher};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::sign::{self, Signer};
use super::targets::{self, Arch, Os, Target};
use super::version::{self, Product};
use super::{read, sha256_hex, write, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// What separates the release files built here: Auberean's, with its client beside the launcher,
/// and the launcher's own ([`super::launcher`]), with no client at all.
pub(crate) struct Flavor {
    pub product: Product,
    /// The client beside the launcher, by program name; `None` for the launcher alone.
    pub client: Option<&'static str>,
    pub release_file_name: fn(Target, &str) -> String,
    pub archive_root: fn(Target, &str) -> Option<String>,
    pub contents: fn(Target) -> (Vec<String>, Vec<String>),
    pub classify: fn(&str, &str) -> Result<Asset, String>,
    /// What `latest.json`'s notes say of the release, from its version and the launcher's.
    pub notes: fn(&str, &str) -> String,
}

/// Auberean's release files.
pub(crate) const AUBEREAN: Flavor = Flavor {
    product: Product::Auberean,
    client: Some(CLIENT),
    release_file_name,
    archive_root,
    contents,
    classify,
    notes: |version, launcher| format!("Auberean {version}, with the launcher {launcher}"),
};

/// The client's program name.
pub const CLIENT: &str = "auberean";

/// The client's package and manifest.
const CLIENT_PACKAGE: (&str, &str) = ("auberean-client", "auberean/client/Cargo.toml");

/// The defaults file's name beside the launcher, as the launcher reads it.
pub const DEFAULTS_FILE: &str = "launcher-defaults.json";

/// What the defaults file says: open a first run in Auberean's realm, and update from Auberean's
/// releases. Dereth's package has no such file, which the launcher reads as Dereth for both.
pub const DEFAULTS: &str = "{\"realm\": \"auberean\", \"updates\": \"auberean\"}\n";

/// The variable naming the repository the client's notice names.
pub const SOURCE_URL_ENV: &str = "AUBEREAN_BUILD_SOURCE_URL";

/// The launcher application's name on macOS: the same bundle as Dereth's.
const APP: &str = dereth::LAUNCHER_APP;

/// The notices beside the programs.
const NOTICES: &[&str] = dereth::NOTICES;

/// The largest file, and the most in all: the launcher's caps.
const FILE_CAP: u64 = dereth::FILE_CAP;
const TOTAL_CAP: u64 = dereth::TOTAL_CAP;

/// The release file for `target`: the launcher, with the Auberean client beside it.
pub fn release_file_name(target: Target, version: &str) -> String {
    let p = target.platform_name();
    match target.os {
        Os::Windows => format!("auberean-{version}-{p}.zip"),
        Os::Mac => format!("Auberean-{version}-{p}.app.tar.gz"),
        Os::Linux => format!("Auberean-{version}-{p}.AppImage"),
    }
}

/// The one top-level folder of the release file's archive; `None` for the AppImage.
pub fn archive_root(target: Target, version: &str) -> Option<String> {
    match target.os {
        Os::Windows => Some(format!("auberean-{version}")),
        Os::Mac => Some(APP.to_owned()),
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
            required.push(format!("{CLIENT}{exe}"));
            required.push(DEFAULTS_FILE.to_owned());
            required.push(super::friends_dll::FILE.to_owned());
            required.extend(notices_in(""));
        }
        Os::Mac => {
            required.extend(own(&[
                "Contents/Info.plist",
                "Contents/MacOS/dereth",
                "Contents/MacOS/auberean",
                "Contents/Resources/launcher-defaults.json",
            ]));
            required.push(format!("Contents/Resources/{}", super::friends_dll::FILE));
            required.extend(notices_in("Contents/Resources/"));
            optional.extend(own(&[
                "Contents/PkgInfo",
                "Contents/Resources/Assets.car",
                "Contents/_CodeSignature/CodeResources",
            ]));
            optional.push(format!(
                "Contents/Resources/{}",
                super::dereth::launcher_icns()
            ));
            optional.extend(super::dereth::client_icons());
        }
    }
    required.sort();
    optional.sort();
    (required, optional)
}

/// The programs in the release file's archive, which the header checks read: the launcher and the
/// client, both windowed.
#[cfg(test)]
pub fn programs(target: Target) -> Vec<String> {
    programs_of(target, Some(CLIENT))
}

/// The programs of a package of the launcher and `client`.
pub(crate) fn programs_of(target: Target, client: Option<&str>) -> Vec<String> {
    let exe = target.exe_suffix();
    let dir = if target.os == Os::Mac {
        "Contents/MacOS/"
    } else {
        ""
    };
    let mut out = vec![format!("{dir}dereth{exe}")];
    if let Some(c) = client {
        out.push(format!("{dir}{c}{exe}"));
    }
    out
}

/// What Dereth's bundle holds that Auberean's does not: Dereth's client and tool, and on macOS
/// MoltenVK, which only Dereth's client draws through, and the bundle's old signature. Paths inside
/// the macOS bundle, or inside the AppImage's folder.
pub fn taken_out(target: Target) -> Vec<String> {
    match target.os {
        Os::Windows => Vec::new(),
        Os::Mac => vec![
            "Contents/MacOS/dereth-client".to_owned(),
            format!("Contents/MacOS/{}", dereth::TOOL),
            "Contents/Frameworks".to_owned(),
            "Contents/_CodeSignature".to_owned(),
        ],
        Os::Linux => vec![
            "usr/bin/dereth-client".to_owned(),
            format!("usr/bin/{}", dereth::TOOL),
        ],
    }
}

/// What a file of an Auberean release folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    /// The release file: the launcher, with the client beside it.
    Release(Target),
    /// One file's lines of `MANIFEST.txt`.
    ManifestPart,
    /// One platform's entry of `latest.json`.
    Piece(Target),
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
pub(crate) const INDEX_FILES: &[&str] =
    &["SHA256SUMS", "release.json", "MANIFEST.txt", "latest.json"];

/// Classify a file of the release folder, or refuse it.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    for triple in targets::DESKTOP_TRIPLES {
        let t = targets::find_dereth(triple)?;
        if name == dereth::piece_name(t) {
            return Ok(Asset::Piece(t));
        }
        let file = release_file_name(t, version);
        if name == file {
            return Ok(Asset::Release(t));
        }
        if name == format!("{file}.manifest") {
            return Ok(Asset::ManifestPart);
        }
    }
    Err(format!(
        "`{name}` is not a file of the Auberean {version} release (a release file, its .manifest, \
         a latest-<platform>.json piece or the index)"
    ))
}

/// Where a release file is downloaded from.
#[cfg(test)]
pub fn download_url(source_url: &str, version: &str, file: &str) -> String {
    download_url_of(Product::Auberean, source_url, version, file)
}

/// Where a release file of `product` is downloaded from.
pub(crate) fn download_url_of(
    product: Product,
    source_url: &str,
    version: &str,
    file: &str,
) -> String {
    format!(
        "{}/releases/download/{}/{file}",
        source_url.trim_end_matches('/'),
        product.tag_for(version)
    )
}

/// The options of one run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
    moltenvk: Option<PathBuf>,
    friends_dll: Option<PathBuf>,
    launcher: Option<PathBuf>,
}

pub const USAGE: &str = "usage: cargo xtask package auberean --launcher <dir> [--target <triple>]... [--out <dir>] [--allow-dirty]
       cargo xtask package auberean --gather <dir>";

fn parse_options(args: &[String], usage: &str, has_client: bool) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value\n{usage}"))
        };
        match a.as_str() {
            "--target" => o.targets.push(value()?),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--gather" => o.gather = Some(PathBuf::from(value()?)),
            "--moltenvk" if !has_client => o.moltenvk = Some(PathBuf::from(value()?)),
            "--friends-dll" if !has_client => o.friends_dll = Some(PathBuf::from(value()?)),
            "--launcher" if has_client => o.launcher = Some(PathBuf::from(value()?)),
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{usage}")),
        }
    }
    if o.gather.is_some()
        && (!o.targets.is_empty()
            || o.out.is_some()
            || o.moltenvk.is_some()
            || o.friends_dll.is_some()
            || o.launcher.is_some())
    {
        return Err(format!(
            "--gather builds nothing: it takes only the folder\n{usage}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package auberean ...` (the arguments after `auberean`).
pub fn package(args: &[String]) -> i32 {
    package_flavor(&AUBEREAN, USAGE, versions, args)
}

/// A `cargo xtask package` command for `flavor`, whose versions `versions` reads: the release's,
/// then the launcher's.
pub(crate) fn package_flavor(
    flavor: &Flavor,
    usage: &str,
    versions: fn(&Path) -> Result<(String, String), String>,
    args: &[String],
) -> i32 {
    let options = match parse_options(args, usage, flavor.client.is_some()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = workspace_root();
    let result = versions(&ws).and_then(|(version, launcher)| match &options.gather {
        Some(dir) => {
            let facts = dereth::launcher_facts(&ws, options.allow_dirty)?;
            write_index(flavor, &ws, dir, &version, &launcher, &facts)?;
            Ok(dir.clone())
        }
        None => {
            let triples = if options.targets.is_empty() {
                vec![super::host_triple()?]
            } else {
                options.targets.clone()
            };
            build_flavor(
                flavor,
                &ws,
                &version,
                &launcher,
                &triples,
                options.out.as_deref(),
                options.allow_dirty,
                options.moltenvk.as_deref(),
                options.friends_dll.as_deref(),
                options.launcher.as_deref(),
            )
        }
    });
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

/// The versions a package is built at: Auberean's own, and the launcher's.
pub(super) fn versions(ws: &Path) -> Result<(String, String), String> {
    let auberean = Product::Auberean.version(ws)?;
    version::parse_release_version(&auberean)?;
    let launcher = Product::Launcher.version(ws)?;
    version::parse_release_version(&launcher)?;
    Ok((auberean, launcher))
}

/// Build, check and write every target's files, then the index. The folder they are in. The
/// launcher comes from the release files in the folder `launcher`.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
    launcher: Option<&Path>,
) -> Result<PathBuf, String> {
    let (version, launcher_version) = versions(ws)?;
    build_flavor(
        &AUBEREAN,
        ws,
        &version,
        &launcher_version,
        triples,
        out,
        allow_dirty,
        None,
        None,
        launcher,
    )
}

/// [`build_packages`], for `flavor` at `version`. A package with a client takes its launcher from
/// the release files in the folder `launcher`; the launcher's own package builds it, with MoltenVK
/// and the retail client's friends library (`friends_dll` when given, else built here).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_flavor(
    flavor: &Flavor,
    ws: &Path,
    version: &str,
    launcher_version: &str,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
    moltenvk: Option<&Path>,
    friends_dll: Option<&Path>,
    launcher: Option<&Path>,
) -> Result<PathBuf, String> {
    let name = flavor.product.command_name();
    let builds_launcher = flavor.client.is_none();
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
    if !dereth::succeeds("cargo", &["about", "--version"]) {
        refusals.push(
            "  cargo-about is not installed, or does not run on this host; it writes THIRD-PARTY-LICENSES.html: \
             `cargo install --locked cargo-about --features cli`"
                .to_owned(),
        );
    }
    if builds_launcher && !dereth::succeeds("cargo", &["tauri", "--version"]) {
        refusals.push(
            "  the Tauri CLI is not installed, or does not run on this host; it builds the launcher: \
             `cargo install --locked tauri-cli --version ^2`"
                .to_owned(),
        );
    }
    if !builds_launcher && launcher.is_none() {
        refusals.push(format!("  {}", dereth::no_launcher_given(launcher_version)));
    }
    let mut moltenvk_found = None;
    if builds_launcher && plans.iter().any(|t| t.os == Os::Mac) {
        // The launcher is built as Dereth's is, MoltenVK and all; it is taken out afterwards.
        let dir = moltenvk
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os(dereth::MOLTENVK_ENV).map(PathBuf::from));
        match dir {
            Some(d) => match dereth::moltenvk_files(&d) {
                Ok(files) => moltenvk_found = Some(files),
                Err(e) => refusals.push(format!("  {e}")),
            },
            None => refusals.push(format!(
                "  the macOS launcher is built as Dereth's is, with MoltenVK: unpack \
                 MoltenVK-macos.tar from https://github.com/KhronosGroup/MoltenVK/releases and pass \
                 --moltenvk <dir> (or set {})",
                dereth::MOLTENVK_ENV
            )),
        }
    }
    if builds_launcher {
        if let Some(why) = super::friends_dll::refusal(friends_dll, &host) {
            refusals.push(format!("  {why}"));
        }
    }
    let appimagetool = if plans.iter().any(|t| t.os == Os::Linux) {
        match super::desktop::appimagetool() {
            Ok(t) => Some(t),
            Err(e) => {
                refusals.push(format!("  {e}"));
                None
            }
        }
    } else {
        None
    };
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let signer = Signer::from_env()?;
    match &signer {
        Some(_) => println!(
            "package: the release files are signed for the launcher's updates ({})",
            sign::KEY_VAR
        ),
        None => println!(
            "package: NOT SIGNED: {} is not set, so the release files get no update signature and \
             no latest.json is written; a release needs the key",
            sign::KEY_VAR
        ),
    }
    let facts = dereth::launcher_facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("{name}-{version}")));
    mkdir(&out_dir)?;
    let library = if builds_launcher {
        Some(super::friends_dll::find(
            ws,
            &facts,
            friends_dll,
            &base.join("stage").join("friends-dll"),
        )?)
    } else {
        None
    };
    for target in plans {
        println!(
            "\n=== {name} {version} (the launcher {launcher_version}) for {}",
            target.triple
        );
        let job = Job {
            flavor,
            ws,
            target,
            facts: &facts,
            version,
            launcher_version,
            out_dir: &out_dir,
            stage: &base.join("stage").join(format!("{name}-{}", target.triple)),
            signer: signer.as_ref(),
            moltenvk: moltenvk_found.as_ref(),
            appimagetool: appimagetool.as_deref(),
            library: library.as_deref(),
            launcher,
        };
        job.run()?;
    }
    write_index(flavor, ws, &out_dir, version, launcher_version, &facts)?;
    Ok(out_dir)
}

/// One target's build.
struct Job<'a> {
    flavor: &'a Flavor,
    ws: &'a Path,
    target: Target,
    facts: &'a BuildFacts,
    /// The version that names the release.
    version: &'a str,
    /// The launcher's own version, Dereth's.
    launcher_version: &'a str,
    out_dir: &'a Path,
    stage: &'a Path,
    signer: Option<&'a Signer>,
    moltenvk: Option<&'a (PathBuf, PathBuf)>,
    appimagetool: Option<&'a str>,
    /// The retail client's friends library, checked, for building the launcher.
    library: Option<&'a Path>,
    /// The folder of the launcher's release files, for a package that takes them.
    launcher: Option<&'a Path>,
}

impl Job<'_> {
    fn run(&self) -> Result<(), String> {
        fresh_dir(self.stage)?;
        let launcher = match (self.flavor.client, self.launcher, self.library) {
            (Some(_), Some(dir), _) => super::launcher::take(
                dir,
                self.target,
                self.launcher_version,
                &self.stage.join("launcher"),
            )?,
            (None, _, Some(library)) => dereth::build_launcher_as_dereth(
                self.ws,
                self.target,
                self.facts,
                self.launcher_version,
                &dereth::launcher_stage(&target_dir().join("package"), self.target),
                self.moltenvk,
                library,
            )?,
            _ => return Err(dereth::no_launcher_given(self.launcher_version)),
        };
        // The client and the defaults file, for a package that has a client beside the launcher.
        let beside = match self.flavor.client {
            Some(_) => {
                let defaults = self.stage.join(DEFAULTS_FILE);
                write(&defaults, DEFAULTS.as_bytes())?;
                Some((self.build_client()?, defaults))
            }
            None => None,
        };
        let beside = beside.as_ref().map(|(c, d)| (c.as_path(), d.as_path()));
        let notices = self.notices()?;
        match self.target.os {
            Os::Windows => self.write_zip(&launcher, beside, &notices),
            Os::Mac => self.write_bundle(&launcher, beside, &notices),
            Os::Linux => self.write_appimage(&launcher, beside, &notices),
        }
    }

    /// The client, built with cargo in the workspace as each of Asheron's Call 2's programs is.
    fn build_client(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let env = super::desktop::build_env(self.ws, self.facts, t);
        let (package, _) = CLIENT_PACKAGE;
        run_in(
            self.ws,
            "cargo",
            &[
                "build",
                "--locked",
                "--release",
                "-p",
                package,
                "--target",
                t.triple,
            ],
            &env,
            &[],
        )?;
        let exe = format!("{CLIENT}{}", t.exe_suffix());
        let built = target_dir().join(t.triple).join("release").join(&exe);
        // A copy of its own, so a later build in the same target folder cannot change what this
        // package holds.
        let staged = self.stage.join("bin").join(&exe);
        dereth::copy(&built, &staged)?;
        Ok(staged)
    }

    /// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html` for the launcher and the client:
    /// the folder they are written in.
    fn notices(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let launcher_manifest = format!("{}/Cargo.toml", dereth::LAUNCHER_DIR);
        let (package, manifest) = CLIENT_PACKAGE;
        let has_client = self.flavor.client.is_some();
        let mut crates = dereth::crates_of(self.ws, &launcher_manifest, "dereth-launcher", t)?;
        if has_client {
            crates.extend(dereth::crates_of(self.ws, "Cargo.toml", package, t)?);
        }
        crates.sort();
        crates.dedup();
        let mit = String::from_utf8_lossy(&read(&self.ws.join("LICENSE"))?).into_owned();
        let mut fonts = Vec::new();
        for (name, carried, path) in dereth::FONTS {
            fonts.push((
                *name,
                *carried,
                String::from_utf8_lossy(&read(&self.ws.join(path))?).into_owned(),
            ));
        }
        let about = self.stage.join("about");
        mkdir(&about)?;
        let exe = t.exe_suffix();
        let launcher_page = dereth::licence_page(self.ws, &launcher_manifest, t, &about, "dereth")?;
        let client_page = if has_client {
            Some(dereth::about_page(
                self.ws,
                ("auberean/about.toml", "auberean/about.hbs"),
                manifest,
                t,
                &about,
                CLIENT,
            )?)
        } else {
            None
        };
        let source_url = std::env::var(SOURCE_URL_ENV)
            .ok()
            .map(|u| u.trim().trim_end_matches('/').to_owned())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| self.facts.source_url.clone());
        let dir = self.stage.join("notices");
        mkdir(&dir)?;
        let text = notice::game_notice(&notice::GameFacts {
            product: self.flavor.product,
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &source_url,
            crates: &crates,
            mit_license: &mit,
            fonts: &fonts,
        });
        write(&dir.join("NOTICE.txt"), text.as_bytes())?;
        write(&dir.join("LICENSE"), mit.as_bytes())?;
        let launcher_name = format!("dereth{exe}");
        let client_name = format!("{CLIENT}{exe}");
        let mut pages = vec![(launcher_name.as_str(), launcher_page)];
        if let Some(page) = client_page {
            pages.push((client_name.as_str(), page));
        }
        write(
            &dir.join("THIRD-PARTY-LICENSES.html"),
            notice::splice_licence_pages(&pages)?.as_bytes(),
        )?;
        Ok(dir)
    }

    /// Check a program for the target, by its name in a package.
    fn check_program(&self, name: &str, bytes: &[u8]) -> Result<String, String> {
        headers::check_app_binary(bytes, self.target, targets::DESKTOP_GLIBC_FLOOR).map_err(
            |problems| {
                format!(
                    "the header check refused {name} for {}:\n  {}",
                    self.target.triple,
                    problems.join("\n  ")
                )
            },
        )
    }

    /// Windows: the launcher Dereth's package has, the client, the defaults file and the notices,
    /// in one folder.
    fn write_zip(
        &self,
        launcher: &BuiltLauncher,
        beside: Option<(&Path, &Path)>,
        notices: &Path,
    ) -> Result<(), String> {
        let root = (self.flavor.archive_root)(self.target, self.version).ok_or("not an archive")?;
        let tree = self.stage.join("tree").join(&root);
        fresh_dir(&tree)?;
        dereth::copy(&launcher.bundle, &tree.join("dereth.exe"))?;
        if let Some((client, defaults)) = beside {
            dereth::copy(client, &tree.join(format!("{CLIENT}.exe")))?;
            dereth::copy(defaults, &tree.join(DEFAULTS_FILE))?;
        }
        dereth::copy(&launcher.library, &tree.join(super::friends_dll::FILE))?;
        for n in NOTICES {
            dereth::copy(&notices.join(n), &tree.join(n))?;
        }
        self.finish_tree(&tree, &root)
    }

    /// macOS: the launcher's bundle, with what its build took in from Dereth taken out, the client,
    /// the defaults file and this package's notices put in, and the whole signed again.
    fn write_bundle(
        &self,
        launcher: &BuiltLauncher,
        beside: Option<(&Path, &Path)>,
        notices: &Path,
    ) -> Result<(), String> {
        let app = self.stage.join("tree").join(APP);
        fresh_dir(&app)?;
        copy_tree(&launcher.bundle, &app)?;
        for gone in taken_out(self.target) {
            remove_path(&app.join(gone))?;
        }
        let macos = app.join("Contents/MacOS");
        let resources = app.join("Contents/Resources");
        if let Some((client, defaults)) = beside {
            dereth::copy(client, &macos.join(CLIENT))?;
            set_executable(&macos.join(CLIENT))?;
            dereth::copy(defaults, &resources.join(DEFAULTS_FILE))?;
        }
        for n in NOTICES {
            dereth::copy(&notices.join(n), &resources.join(n))?;
        }
        let identity = std::env::var("APPLE_SIGNING_IDENTITY").ok();
        let nested: Vec<String> = self
            .flavor
            .client
            .iter()
            .map(|c| format!("Contents/MacOS/{c}"))
            .collect();
        for args in codesign_runs_of(&app.display().to_string(), identity.as_deref(), &nested) {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            run_in(self.stage, "codesign", &refs, &[], &[])?;
        }
        self.finish_tree(&app, APP)
    }

    /// Linux: the launcher's AppImage, unpacked, with what its build took in from Dereth taken out,
    /// the client, the defaults file and this package's notices put in, and packed again.
    fn write_appimage(
        &self,
        launcher: &BuiltLauncher,
        beside: Option<(&Path, &Path)>,
        notices: &Path,
    ) -> Result<(), String> {
        let t = self.target;
        let file = (self.flavor.release_file_name)(t, self.version);
        let mut added = Vec::new();
        if let Some((client, defaults)) = beside {
            added.push((CLIENT, client, true));
            added.push((DEFAULTS_FILE, defaults, false));
        }
        let packed = repack_appimage(&AppImageRepack {
            ws: self.ws,
            target: t,
            facts: self.facts,
            stage: self.stage,
            appimagetool: self.appimagetool,
            appimage: &launcher.bundle,
            added: &added,
            notices,
            file: &file,
        })?;
        let mut lines = Vec::new();
        let mut programs = vec![("dereth".to_owned(), packed.launcher.clone())];
        if let Some((client, _)) = beside {
            programs.push((CLIENT.to_owned(), client.to_path_buf()));
        }
        for (name, path) in programs {
            let bytes = read(&path)?;
            let d = self.check_program(&name, &bytes)?;
            println!("  {name}: {d}");
            lines.push((name, bytes, d));
        }
        let named: Vec<(&str, &[u8])> = lines
            .iter()
            .map(|(n, b, _)| (n.as_str(), b.as_slice()))
            .collect();
        self.refuse_machine_folders(&packed.file, &named)?;
        let bytes = read(&packed.file)?;
        let findings = dereth::check_appimage(&file, &bytes);
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                packed.file.display(),
                findings.join("\n  ")
            ));
        }
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        set_executable(&path)?;
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        lines.insert(0, (String::new(), bytes.clone(), "AppImage".to_owned()));
        self.write_manifest(&file, &lines)?;
        self.sign(&file, &bytes)
    }

    /// Scan the folder `dir`, check its programs, archive it under `root`, scan the archive, sign
    /// it, and write its manifest lines.
    fn finish_tree(&self, dir: &Path, root: &str) -> Result<(), String> {
        let t = self.target;
        let (required, optional) = (self.flavor.contents)(t);
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
        let programs = programs_of(t, self.flavor.client);
        for name in &programs {
            let name = name.clone();
            let d = self.check_program(&name, &read(&dir.join(&name))?)?;
            println!("  {name}: {d}");
            described.push((name, d));
        }
        let mut members = Vec::new();
        for e in &staged {
            if e.kind == Kind::File {
                let path = dir.join(&e.path);
                members.push(Member {
                    name: e.path.clone(),
                    bytes: read(&path)?,
                    executable: dereth::executable(&path) || programs.contains(&e.path),
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
        let file = (self.flavor.release_file_name)(t, self.version);
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        check_archive(self.flavor, &path, t, self.version)?;
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
        self.sign(&file, &bytes)
    }

    /// Refuse the package at `what` when any of `files` names one of this machine's folders.
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

    /// With a key: the release file's signature for the launcher's updater, verified as the
    /// updater verifies it, in this platform's piece of `latest.json`, under the version that names
    /// the launcher it holds. Without one, no piece.
    fn sign(&self, file: &str, bytes: &[u8]) -> Result<(), String> {
        let t = self.target;
        let piece_path = self.out_dir.join(dereth::piece_name(t));
        let Some(signer) = self.signer else {
            let _ = std::fs::remove_file(&piece_path);
            return Ok(());
        };
        let signed_as = self.launcher_version;
        let signature = signer.sign(bytes, file, signed_as, self.facts.epoch)?;
        let public_key = dereth::launcher_public_key(self.ws)?;
        sign::verify(bytes, &signature, &public_key, signed_as).map_err(|e| {
            format!(
                "the update signature of {file} does not verify against the launcher's public key \
                 (plugins.updater.pubkey): {e}; is {} the launcher's key?",
                sign::KEY_VAR
            )
        })?;
        let url = download_url_of(
            self.flavor.product,
            &self.facts.source_url,
            self.version,
            file,
        );
        let piece = sign::platform_piece(&t.updater_platform(), &url, &signature);
        write(
            &piece_path,
            serde_json::to_string_pretty(&piece)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )?;
        println!("  {file}: signed; {} written", dereth::piece_name(t));
        Ok(())
    }
}

/// The `codesign` runs that sign the bundle at `app` again: the client first, then the bundle,
/// which seals the launcher and everything else in it. Ad hoc without a Developer ID (`identity`),
/// and with one under the hardened runtime and a timestamp.
#[cfg(test)]
pub fn codesign_runs(app: &str, identity: Option<&str>) -> Vec<Vec<String>> {
    codesign_runs_of(app, identity, &[format!("Contents/MacOS/{CLIENT}")])
}

/// `codesign_runs` for a bundle with `nested` code in it (paths below the bundle, the deepest
/// first), which is signed before the bundle is.
pub(crate) fn codesign_runs_of(
    app: &str,
    identity: Option<&str>,
    nested: &[String],
) -> Vec<Vec<String>> {
    let identity = identity
        .map(str::trim)
        .filter(|i| !i.is_empty() && *i != "-");
    let sign = |path: String| {
        let mut args = vec!["--force".to_owned(), "--sign".to_owned()];
        match identity {
            Some(id) => args.extend([
                id.to_owned(),
                "--options".to_owned(),
                "runtime".to_owned(),
                "--timestamp".to_owned(),
            ]),
            None => args.push("-".to_owned()),
        }
        args.push(path);
        args
    };
    let mut runs: Vec<Vec<String>> = nested.iter().map(|n| sign(format!("{app}/{n}"))).collect();
    runs.push(sign(app.to_owned()));
    runs
}

/// What goes into a launcher's AppImage to make a package of it.
pub(crate) struct AppImageRepack<'a> {
    pub ws: &'a Path,
    pub target: Target,
    pub facts: &'a BuildFacts,
    pub stage: &'a Path,
    pub appimagetool: Option<&'a str>,
    /// The launcher's AppImage.
    pub appimage: &'a Path,
    /// Each file put in `usr/bin`: its name there, where it is, and whether it runs.
    pub added: &'a [(&'a str, &'a Path, bool)],
    /// The folder of the package's notices, which replace the launcher's.
    pub notices: &'a Path,
    /// The AppImage's name in the release.
    pub file: &'a str,
}

/// An AppImage packed by [`repack_appimage`].
pub(crate) struct Repacked {
    /// The packed file.
    pub file: PathBuf,
    /// The launcher program unpacked from it.
    pub launcher: PathBuf,
}

/// Unpack a launcher's AppImage, take out what its build took in from Dereth, put in what `r` adds,
/// and pack it again with `appimagetool`.
pub(crate) fn repack_appimage(r: &AppImageRepack<'_>) -> Result<Repacked, String> {
    let t = r.target;
    let unpack = r.stage.join("unpack");
    fresh_dir(&unpack)?;
    let appimage = unpack.join("dereth.AppImage");
    dereth::copy(r.appimage, &appimage)?;
    set_executable(&appimage)?;
    let appimage_arg = appimage.display().to_string();
    run_in(&unpack, &appimage_arg, &["--appimage-extract"], &[], &[])?;
    let dir = unpack.join("squashfs-root");
    for gone in taken_out(t) {
        remove_path(&dir.join(gone))?;
    }
    let bin = dir.join("usr/bin");
    for (name, from, runs) in r.added {
        dereth::copy(from, &bin.join(name))?;
        if *runs {
            set_executable(&bin.join(name))?;
        }
    }
    // The notices go where the launcher's build put its own.
    let found = files_named(&dir, "NOTICE.txt")?;
    if found.is_empty() {
        return Err("the launcher's AppImage holds no notices to replace".to_owned());
    }
    for notice in found {
        let folder = notice.parent().unwrap_or(&dir).to_path_buf();
        for n in NOTICES {
            dereth::copy(&r.notices.join(n), &folder.join(n))?;
        }
    }
    let tool = r.appimagetool.ok_or("a Linux package needs appimagetool")?;
    let packed = r.stage.join(r.file);
    let runtime = std::env::var(super::desktop::APPIMAGE_RUNTIME_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty());
    let dir_arg = dir.display().to_string();
    let packed_arg = packed.display().to_string();
    let mut args = vec!["--no-appstream"];
    if let Some(v) = &runtime {
        args.extend(["--runtime-file", v.as_str()]);
    }
    args.extend([dir_arg.as_str(), packed_arg.as_str()]);
    let arch = match t.arch {
        Arch::X86_64 => "x86_64",
        Arch::Aarch64 => "aarch64",
    };
    let env = vec![
        ("ARCH".to_owned(), arch.to_owned()),
        ("SOURCE_DATE_EPOCH".to_owned(), r.facts.epoch.to_string()),
        ("APPIMAGE_EXTRACT_AND_RUN".to_owned(), "1".to_owned()),
    ];
    run_in(r.ws, tool, &args, &env, &[])?;
    Ok(Repacked {
        file: packed,
        launcher: bin.join("dereth"),
    })
}

/// Copy the folder `from` into `to`, keeping which files are executable.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    mkdir(to)?;
    for e in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let kind = e.file_type().map_err(|e| e.to_string())?;
        let dest = to.join(e.file_name());
        if kind.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            dereth::copy(&e.path(), &dest)?;
            if dereth::executable(&e.path()) {
                set_executable(&dest)?;
            }
        }
    }
    Ok(())
}

/// Remove a file or a folder, which may be missing already.
fn remove_path(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(m) if m.is_dir() => {
            std::fs::remove_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))
        }
        Ok(_) => std::fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display())),
    }
}

/// Every file named `name` below `dir`, links not followed.
fn files_named(dir: &Path, name: &str) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        let kind = e.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            found.extend(files_named(&e.path(), name)?);
        } else if kind.is_file() && e.file_name() == name {
            found.push(e.path());
        }
    }
    Ok(found)
}

pub(crate) fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Scan a written release file again: an archive by its listing, an AppImage as one file.
pub(crate) fn check_archive(
    flavor: &Flavor,
    path: &Path,
    target: Target,
    version: &str,
) -> Result<(), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let findings = match (flavor.archive_root)(target, version) {
        None => dereth::check_appimage(&name, &read(path)?),
        Some(root) => {
            let entries: Vec<Entry> = archive::archive_entries(path, &root)?;
            let (required, optional) = (flavor.contents)(target);
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

/// The index over a release folder: every release file scanned again and every signature verified
/// again, then `MANIFEST.txt`, `release.json`, `latest.json` (when every release file is signed)
/// and `SHA256SUMS`.
pub(crate) fn write_index(
    flavor: &Flavor,
    ws: &Path,
    dir: &Path,
    version: &str,
    launcher_version: &str,
    facts: &BuildFacts,
) -> Result<(), String> {
    let name = flavor.product.command_name();
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut releases = Vec::new();
    let mut parts = Vec::new();
    let mut pieces = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match (flavor.classify)(name, version) {
            Ok(Asset::Release(t)) => releases.push((name.clone(), t)),
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
    if releases.is_empty() {
        return Err(format!(
            "no {} {version} release files in {}",
            flavor.product.display_name(),
            dir.display()
        ));
    }
    for (name, t) in &releases {
        check_archive(flavor, &dir.join(name), *t, version)?;
    }

    // latest.json: every release file signed, each signature good, one piece per platform.
    let signed_as = launcher_version;
    let public_key = dereth::launcher_public_key(ws)?;
    let mut piece_values = Vec::new();
    let mut unsigned = Vec::new();
    for (name, t) in &releases {
        let Some((piece_file, _)) = pieces.iter().find(|(_, pt)| pt == t) else {
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
        let want = download_url_of(flavor.product, &facts.source_url, version, name);
        if url != want {
            return Err(format!("{piece_file} points at {url}, not at {want}"));
        }
        sign::verify(&read(&dir.join(name))?, signature, &public_key, signed_as)
            .map_err(|e| format!("{name}: {e}"))?;
        piece_values.push(value);
    }
    for (piece_file, t) in &pieces {
        if !releases.iter().any(|(_, rt)| rt == t) {
            return Err(format!("{piece_file} has no release file beside it"));
        }
    }
    let latest_path = dir.join("latest.json");
    let signed = unsigned.is_empty();
    if signed {
        let latest = sign::merge_latest(
            signed_as,
            &(flavor.notes)(version, launcher_version),
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
        "# {name} {version} (the launcher {launcher_version}), commit {}\n# target\tfile\tsize\tsha256\theader\n",
        facts.commit
    );
    for p in &parts {
        manifest.push_str(&String::from_utf8_lossy(&read(&dir.join(p))?));
    }
    write(&dir.join("MANIFEST.txt"), manifest.as_bytes())?;

    let mut assets = Vec::new();
    for (name, t) in &releases {
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
        "product": name,
        "version": version,
        "launcher_version": launcher_version,
        "tag": flavor.product.tag_for(version),
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

    let mut summed: Vec<String> = releases.iter().map(|(n, _)| n.clone()).collect();
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
