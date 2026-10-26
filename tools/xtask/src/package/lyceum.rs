//! `cargo xtask package lyceum`: Lyceum's release files (the world builder's editor and its
//! `lyceum` command), built the same way on a contributor's machine and in the release workflow.
//!
//! ```text
//! cargo xtask package lyceum [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
//! cargo xtask package lyceum --gather <dir>
//! ```
//!
//! For each target (the host's own by default; each builds on its own operating system) it builds
//! all four programs with cargo, then writes into `<target dir>/package/lyceum-<version>/` (or
//! `--out`):
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `lyceum-<v>-windows-x86_64.zip`: `lyceum-editor.exe`, `lyceum.exe`, the notices, README and CHANGES, under `lyceum-<v>/` |
//! | macOS | `Lyceum-<v>-macos-<arch>.app.tar.gz`: `Lyceum.app`, all four programs in `Contents/MacOS`, MoltenVK in `Contents/Frameworks`, the icon, the notices, README and CHANGES in `Contents/Resources` |
//! | Linux | `lyceum-<v>-linux-x86_64.tar.gz`: all four programs, the desktop entry and its icon, the notices, README and CHANGES, under `lyceum-<v>/` |
//!
//! Each release file's lines of `MANIFEST.txt` go in `<file>.manifest`. Then the index is written
//! over the folder: `MANIFEST.txt`, `release.json` and `SHA256SUMS`. `--gather <dir>` builds
//! nothing: it checks the files already in `<dir>` (every runner's, in the release workflow) and
//! writes the index over them.
//!
//! Lyceum has no updater, so nothing is signed for one: a release is downloaded by hand, and
//! checked against `SHA256SUMS`. The macOS bundle is code-signed as Dereth's is: ad hoc, or with a
//! Developer ID when `APPLE_SIGNING_IDENTITY` names one ([`codesign_runs`]).
//!
//! **The dat guard** is Dereth's: only named files are copied, and the deny scan runs over what was
//! staged and again over each written archive. A file that is not on the package's list, a game
//! data file or anything named like one, a link, or a file over the cap fails the package, and so
//! does a file that names one of this machine's folders.
//!
//! **macOS** needs MoltenVK's macOS release unpacked (`--moltenvk <dir>` or `DERETH_MOLTENVK_DIR`,
//! as Dereth's package takes it): its `libMoltenVK.dylib` goes into the bundle, where the editor
//! looks for it, and its license into the notices.

use std::path::{Path, PathBuf};

use super::archive::{self, Member};
use super::dereth::{
    about_page, copy, crates_of, executable, fresh_dir, is_macho_library, mkdir, moltenvk_files,
    run_in, succeeds, MOLTENVK_ENV,
};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::targets::{self, Os, Target};
use super::version::{self, Product};
use super::{build_facts, read, sha256_hex, write, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// The editor's program name.
pub const EDITOR: &str = "lyceum-editor";

/// The command's program name (the `lyceum-cli` package's binary).
pub const COMMAND: &str = "lyceum";

/// The bundled server and its database tool, run as separate processes.
pub const SERVER: &str = "empyrean-server";
pub const IMPORT: &str = "empyrean-import";

/// The packages the four programs are built from, with their manifests.
const PACKAGES: &[(&str, &str)] = &[
    ("lyceum-editor", "lyceum/editor/Cargo.toml"),
    ("lyceum-cli", "lyceum/cli/Cargo.toml"),
    ("empyrean-server", "empyrean/server/Cargo.toml"),
    ("empyrean-import", "empyrean/import/Cargo.toml"),
];

/// cargo-about's configuration and template for Lyceum.
const ABOUT: (&str, &str) = ("lyceum/about.toml", "lyceum/about.hbs");

/// The documents every package carries beside the notices, as (name in the package, path in the
/// tree).
pub const DOCUMENTS: &[(&str, &str)] = &[
    ("README.md", "lyceum/README.md"),
    ("PACKS.md", "lyceum/PACKS.md"),
    ("CHANGES.md", "lyceum/CHANGES.md"),
    ("EMPYREAN-LICENSE", "empyrean/LICENSE"),
    (
        "LIFESTONED-LICENSE.txt",
        "empyrean/licenses/Lifestoned-MIT.txt",
    ),
];

/// The notices every package carries: Dereth's three.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// The typefaces the package carries: (typeface, what carries it and for what, its license).
pub const FONTS: &[(&str, &str, &str)] = &[(
    "Liberation",
    "The editor's text is drawn in the Liberation fonts (version 2.1.5), which the editor\n\
     carries",
    "dereth/client/crates/classic-fonts/fonts/OFL.txt",
)];

/// The macOS bundle's icon for every macOS before 26, with its own picture for the small sizes.
pub const ICNS: &str = "lyceum/editor/assets/lyceum.icns";

/// The layered icon macOS 26 draws itself (its square, glass and shading), compiled into the
/// bundle's `Assets.car` where Xcode is installed.
pub const ICON: &str = "lyceum/editor/assets/Lyceum.icon";

/// The name of the icon inside `Assets.car`: the `.icon`'s own name.
const ICON_NAME: &str = "Lyceum";

/// The picture the Linux desktop entry names, the one the editor's window carries.
pub const ICON_PNG: &str = "lyceum/editor/assets/lyceum-256.png";

/// What the desktops know Lyceum by: the bundle's identifier on macOS, and on Linux the window's
/// application id, which names the desktop entry and its icon. The editor's window gives the same.
pub const APP_ID: &str = "network.dereth.lyceum";

/// The macOS bundle's name.
pub const APP: &str = "Lyceum.app";

/// The repository the release files are downloaded from and the notices name as the source, when
/// it is not the default.
pub const SOURCE_URL_ENV: &str = "LYCEUM_BUILD_SOURCE_URL";

/// No single file of a Lyceum package is larger than this: the editor, the largest, is some
/// thirty megabytes. Every retail data file but the small language file is larger, and the name
/// rules and the data-file header catch that one too.
pub const FILE_CAP: u64 = 96 << 20;

/// No package holds more than this in all.
pub const TOTAL_CAP: u64 = 192 << 20;

/// The release file for `target`.
pub fn release_file_name(target: Target, version: &str) -> String {
    let p = target.platform_name();
    match target.os {
        Os::Windows => format!("lyceum-{version}-{p}.zip"),
        Os::Mac => format!("Lyceum-{version}-{p}.app.tar.gz"),
        Os::Linux => format!("lyceum-{version}-{p}.tar.gz"),
    }
}

/// The one top-level folder of the release file's archive.
pub fn archive_root(target: Target, version: &str) -> String {
    match target.os {
        Os::Mac => APP.to_owned(),
        Os::Windows | Os::Linux => format!("lyceum-{version}"),
    }
}

/// The desktop entry's file name: the application id, which a desktop finds it by.
pub fn desktop_entry_name() -> String {
    format!("{APP_ID}.desktop")
}

/// The desktop entry's icon's file name, which the entry names by the application id too.
pub fn desktop_icon_name() -> String {
    format!("{APP_ID}.png")
}

/// Where the notices and the documents go below the top-level folder.
fn resources_dir(target: Target) -> &'static str {
    if target.os == Os::Mac {
        "Contents/Resources/"
    } else {
        ""
    }
}

/// The programs in the release file's archive, editor first, each with whether it is the windowed
/// one: the editor, a windowed program with its icon in its header on Windows, where Explorer
/// reads it; the command is a console program.
pub fn programs(target: Target) -> Vec<(String, bool)> {
    let exe = target.exe_suffix();
    let dir = if target.os == Os::Mac {
        "Contents/MacOS/"
    } else {
        ""
    };
    vec![
        (format!("{dir}{EDITOR}{exe}"), true),
        (format!("{dir}{COMMAND}{exe}"), false),
        (format!("{dir}{SERVER}{exe}"), false),
        (format!("{dir}{IMPORT}{exe}"), false),
    ]
}

/// Every file the release file's archive holds below its top-level folder, and nothing else.
pub fn contents(target: Target) -> Vec<String> {
    let res = resources_dir(target);
    let mut files: Vec<String> = programs(target).into_iter().map(|(p, _)| p).collect();
    files.extend(NOTICES.iter().map(|n| format!("{res}{n}")));
    files.extend(DOCUMENTS.iter().map(|(n, _)| format!("{res}{n}")));
    match target.os {
        Os::Windows => {}
        Os::Linux => {
            files.push(desktop_entry_name());
            files.push(desktop_icon_name());
        }
        Os::Mac => files.extend(
            [
                "Contents/Info.plist",
                "Contents/PkgInfo",
                "Contents/Frameworks/libMoltenVK.dylib",
                "Contents/Resources/lyceum.icns",
                "Contents/_CodeSignature/CodeResources",
            ]
            .map(str::to_owned),
        ),
    }
    files.sort();
    files
}

/// What the release file's archive may hold besides [`contents`]: the compiled layered icon, which
/// only a machine with Xcode makes.
pub fn optional_contents(target: Target) -> Vec<String> {
    match target.os {
        Os::Mac => vec!["Contents/Resources/Assets.car".to_owned()],
        Os::Windows | Os::Linux => Vec::new(),
    }
}

/// The numeric core of a version (`0.2.0` of `0.2.0-rc.1`), the form a bundle's version takes.
fn version_core(version: &str) -> &str {
    version.split('-').next().unwrap_or(version)
}

/// The macOS bundle's `Info.plist`: its name, identifier, version, executable and icon. With
/// `composed`, the bundle holds the compiled layered icon too, which `CFBundleIconName` names and
/// macOS 26 looks for first; every macOS reads `CFBundleIconFile`.
pub fn info_plist(version: &str, composed: bool) -> String {
    let v = version_core(version);
    let icon_name = if composed {
        format!("\n\t<key>CFBundleIconName</key>\n\t<string>{ICON_NAME}</string>")
    } else {
        String::new()
    };
    let floor = format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Lyceum</string>
	<key>CFBundleDisplayName</key>
	<string>Lyceum</string>
	<key>CFBundleIdentifier</key>
	<string>{APP_ID}</string>
	<key>CFBundleExecutable</key>
	<string>{EDITOR}</string>
	<key>CFBundleIconFile</key>
	<string>lyceum.icns</string>{icon_name}
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleShortVersionString</key>
	<string>{v}</string>
	<key>CFBundleVersion</key>
	<string>{v}</string>
	<key>LSMinimumSystemVersion</key>
	<string>{floor}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.developer-tools</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
    )
}

/// The Linux desktop entry: the editor, opening the world file it is given.
pub fn desktop_entry() -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Lyceum\nComment=The world builder\n\
         Exec={EDITOR} %f\nIcon={APP_ID}\nTerminal=false\nCategories=Development;\n\
         StartupWMClass={APP_ID}\n"
    )
}

/// The `codesign` runs that sign the bundle at `app`, inside out: MoltenVK, the command, then the
/// bundle itself, which signs the editor (its executable) and seals its resources.
///
/// With a Developer ID (`identity`, as `APPLE_SIGNING_IDENTITY` names it) each is signed with it
/// under the hardened runtime, with a secure timestamp, as notarization requires: the editor loads
/// MoltenVK, which carries the same team. Without one each is signed ad hoc, which Apple silicon
/// needs to run it at all, and the hardened runtime stays off: an ad-hoc signature carries no team,
/// so library validation would refuse MoltenVK. Dereth's bundle is signed the same way.
pub fn codesign_runs(app: &str, identity: Option<&str>) -> Vec<Vec<String>> {
    let id = identity
        .map(str::trim)
        .filter(|i| !i.is_empty() && *i != "-");
    let mut base: Vec<String> = vec!["--force".into(), "--sign".into(), id.unwrap_or("-").into()];
    if id.is_some() {
        base.extend(["--options".into(), "runtime".into(), "--timestamp".into()]);
    }
    [
        format!("{app}/Contents/Frameworks/libMoltenVK.dylib"),
        format!("{app}/Contents/MacOS/{COMMAND}"),
        format!("{app}/Contents/MacOS/{SERVER}"),
        format!("{app}/Contents/MacOS/{IMPORT}"),
        app.to_owned(),
    ]
    .into_iter()
    .map(|path| {
        let mut run = base.clone();
        run.push(path);
        run
    })
    .collect()
}

/// What a file of a Lyceum release folder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asset {
    /// One target's release file.
    Release(Target),
    /// One release file's lines of `MANIFEST.txt`.
    ManifestPart,
    /// A file the index writes, replaced on every run.
    Index,
}

/// The index files.
const INDEX_FILES: &[&str] = &["SHA256SUMS", "release.json", "MANIFEST.txt"];

/// Classify a file of the release folder, or refuse it: the folder holds this version's release
/// files, their manifest parts, and the index.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if INDEX_FILES.contains(&name) {
        return Ok(Asset::Index);
    }
    for triple in targets::LYCEUM_TRIPLES {
        let t = targets::find(triple)?;
        let file = release_file_name(t, version);
        if name == file {
            return Ok(Asset::Release(t));
        }
        if name == format!("{file}.manifest") {
            return Ok(Asset::ManifestPart);
        }
    }
    Err(format!(
        "`{name}` is not a file of the Lyceum {version} release (a release file, its .manifest \
         or the index)"
    ))
}

/// The options of one `package lyceum` run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
    moltenvk: Option<PathBuf>,
}

pub const USAGE: &str = "usage: cargo xtask package lyceum [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
       cargo xtask package lyceum --gather <dir>";

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

/// `cargo xtask package lyceum ...` (the arguments after `lyceum`).
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
fn lyceum_facts(ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    let mut facts = build_facts(ws, allow_dirty)?;
    if let Ok(url) = std::env::var(SOURCE_URL_ENV) {
        if !url.trim().is_empty() {
            facts.source_url = url.trim().trim_end_matches('/').to_owned();
        }
    }
    Ok(facts)
}

/// Build, check and write every target's release file, then the index. The folder they are in.
pub fn build_packages(
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
    moltenvk: Option<&Path>,
) -> Result<PathBuf, String> {
    let version = Product::Lyceum.version(ws)?;
    version::parse_release_version(&version)?;
    // Everything a host lacks is said before anything is built.
    let host = super::host();
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    for triple in triples {
        match targets::find_lyceum(triple).and_then(|t| targets::plan_lyceum(t, &host).map(|()| t))
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
    let facts = lyceum_facts(ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("lyceum-{version}")));
    mkdir(&out_dir)?;
    for target in plans {
        println!("\n=== lyceum {version} for {}", target.triple);
        let job = Job {
            ws,
            target,
            facts: &facts,
            version: &version,
            out_dir: &out_dir,
            stage: &base.join("stage").join(format!("lyceum-{}", target.triple)),
            moltenvk: moltenvk_found.as_ref(),
        };
        job.run()?;
    }
    write_index(&out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the files there and write the index over them.
fn gather(ws: &Path, dir: &Path, allow_dirty: bool) -> Result<PathBuf, String> {
    let version = Product::Lyceum.version(ws)?;
    let facts = lyceum_facts(ws, allow_dirty)?;
    write_index(dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

/// The environment a release build runs with: the commit and the target all four programs name in
/// `--version`, the commit's time, and the flags that keep local paths out of the programs. On
/// Windows the C runtime is linked in, so neither program needs a Visual C++ redistributable, and
/// the linker writes no time; on macOS the programs claim the release's oldest macOS.
pub fn build_env(ws: &Path, facts: &BuildFacts, target: Target) -> Vec<(String, String)> {
    let mut env = super::build_env(facts, target, &super::local_remaps(ws));
    env.extend([
        ("LYCEUM_BUILD_COMMIT".to_owned(), facts.commit.clone()),
        ("LYCEUM_BUILD_TARGET".to_owned(), target.triple.to_owned()),
        ("LYCEUM_BUNDLED_SERVER".to_owned(), "1".to_owned()),
    ]);
    env
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
    /// MoltenVK's dylib and license (macOS).
    moltenvk: Option<&'a (PathBuf, PathBuf)>,
}

impl Job<'_> {
    fn run(&self) -> Result<(), String> {
        fresh_dir(self.stage)?;
        let built = self.build()?;
        let notices = self.notices()?;
        let root = archive_root(self.target, self.version);
        let tree = self.stage.join(&root);
        self.lay_out(&tree, &built, &notices)?;
        self.finish_tree(&tree, &root)
    }

    /// All four programs, built with cargo in the workspace: each copied out of the target folder, so
    /// a later build there cannot change what this package holds. Editor first.
    fn build(&self) -> Result<[PathBuf; 4], String> {
        let t = self.target;
        let mut args = vec!["build", "--locked", "--release", "--target", t.triple];
        for (package, _) in PACKAGES {
            args.extend(["-p", package]);
        }
        run_in(
            self.ws,
            "cargo",
            &args,
            &build_env(self.ws, self.facts, t),
            &[],
        )?;
        let release = target_dir().join(t.triple).join("release");
        let exe = t.exe_suffix();
        let staged = |program: &str| -> Result<PathBuf, String> {
            let to = self.stage.join("bin").join(format!("{program}{exe}"));
            copy(&release.join(format!("{program}{exe}")), &to)?;
            Ok(to)
        };
        Ok([
            staged(EDITOR)?,
            staged(COMMAND)?,
            staged(SERVER)?,
            staged(IMPORT)?,
        ])
    }

    /// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: the folder they are written in.
    fn notices(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let mut crates = Vec::new();
        for (package, _) in PACKAGES {
            crates.extend(crates_of(self.ws, "Cargo.toml", package, t)?);
        }
        crates.sort();
        crates.dedup();
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
            Some((_, license)) => Some(String::from_utf8_lossy(&read(license)?).into_owned()),
            None => None,
        };
        let about = self.stage.join("about");
        mkdir(&about)?;
        let exe = t.exe_suffix();
        let names = [EDITOR, COMMAND, SERVER, IMPORT].map(|name| format!("{name}{exe}"));
        let mut pages = Vec::new();
        for (((_, manifest), program), name) in PACKAGES
            .iter()
            .zip([EDITOR, COMMAND, SERVER, IMPORT])
            .zip(&names)
        {
            let config = if program == SERVER || program == IMPORT {
                ("empyrean/about.toml", "empyrean/about.hbs")
            } else {
                ABOUT
            };
            let page = about_page(self.ws, config, manifest, t, &about, program)?;
            pages.push((name.as_str(), page));
        }
        let dir = self.stage.join("notices");
        mkdir(&dir)?;
        let text = notice::lyceum_notice(&notice::LyceumFacts {
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &self.facts.source_url,
            crates: &crates,
            mit_license: &mit,
            fonts: &fonts,
            moltenvk_license: moltenvk.as_deref(),
        });
        write(&dir.join("NOTICE.txt"), text.as_bytes())?;
        write(&dir.join("LICENSE"), mit.as_bytes())?;
        write(
            &dir.join("THIRD-PARTY-LICENSES.html"),
            notice::splice_licence_pages(&pages)?.as_bytes(),
        )?;
        Ok(dir)
    }

    /// The release file's tree in `tree`, every file written anew from a named path; on macOS the
    /// bundle, signed.
    fn lay_out(&self, tree: &Path, built: &[PathBuf; 4], notices: &Path) -> Result<(), String> {
        let t = self.target;
        fresh_dir(tree)?;
        for ((name, _), from) in programs(t).iter().zip(built) {
            copy(from, &tree.join(name))?;
        }
        let res = resources_dir(t);
        for n in NOTICES {
            copy(&notices.join(n), &tree.join(format!("{res}{n}")))?;
        }
        for (name, from) in DOCUMENTS {
            copy(&self.ws.join(from), &tree.join(format!("{res}{name}")))?;
        }
        match t.os {
            Os::Windows => Ok(()),
            Os::Linux => {
                write(&tree.join(desktop_entry_name()), desktop_entry().as_bytes())?;
                copy(&self.ws.join(ICON_PNG), &tree.join(desktop_icon_name()))
            }
            Os::Mac => {
                let (dylib, _) = self.moltenvk.ok_or("a macOS package needs MoltenVK")?;
                copy(dylib, &tree.join("Contents/Frameworks/libMoltenVK.dylib"))?;
                copy(
                    &self.ws.join(ICNS),
                    &tree.join("Contents/Resources/lyceum.icns"),
                )?;
                let composed = self.compose_icon(&tree.join("Contents/Resources"))?;
                write(
                    &tree.join("Contents/Info.plist"),
                    info_plist(self.version, composed).as_bytes(),
                )?;
                // Four bytes of classic Mac OS that the Finder still reads before the plist.
                write(&tree.join("Contents/PkgInfo"), b"APPL????")?;
                self.sign_bundle(tree)
            }
        }
    }

    /// Sign the bundle at `app` ([`codesign_runs`]) and verify it as the system will.
    /// [`ICON`] compiled into `resources/Assets.car` with Xcode's `actool`, when it is here and
    /// compiles it; the bundle keeps [`ICNS`] either way, for every earlier macOS. Whether it did.
    fn compose_icon(&self, resources: &Path) -> Result<bool, String> {
        if !crate::bundle::actool_is_installed() {
            println!("package: no actool (Xcode) here; {APP} has its flat icon only");
            return Ok(false);
        }
        let work = self.stage.join("icon");
        fresh_dir(&work)?;
        let floor = format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1);
        let source = self.ws.join(ICON);
        if !crate::bundle::actool_compile(self.ws, &source, ICON_NAME, &floor, &work) {
            println!(
                "package: actool could not compile {}; {APP} has its flat icon only",
                source.display()
            );
            return Ok(false);
        }
        copy(&work.join("Assets.car"), &resources.join("Assets.car"))?;
        Ok(true)
    }

    fn sign_bundle(&self, app: &Path) -> Result<(), String> {
        let identity = std::env::var("APPLE_SIGNING_IDENTITY").ok();
        let path = app.display().to_string();
        match identity
            .as_deref()
            .map(str::trim)
            .filter(|i| !i.is_empty() && *i != "-")
        {
            Some(_) => println!("package: signing {APP} with APPLE_SIGNING_IDENTITY"),
            None => println!("package: signing {APP} ad hoc (no APPLE_SIGNING_IDENTITY)"),
        }
        for run in codesign_runs(&path, identity.as_deref()) {
            let args: Vec<&str> = run.iter().map(String::as_str).collect();
            run_in(self.ws, "codesign", &args, &[], &[])?;
        }
        run_in(
            self.ws,
            "codesign",
            &["--verify", "--deep", "--strict", &path],
            &[],
            &[],
        )
    }

    /// Scan the folder `dir`, check its programs, archive it under `root`, scan the archive, and
    /// write its manifest lines.
    fn finish_tree(&self, dir: &Path, root: &str) -> Result<(), String> {
        let t = self.target;
        let allowed = contents(t);
        let staged = guard::staged_entries(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let findings = guard::scan_tree(
            &staged,
            &allowed,
            &optional_contents(t),
            FILE_CAP,
            TOTAL_CAP,
        );
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                dir.display(),
                findings.join("\n  ")
            ));
        }
        let mut described = Vec::new();
        for (name, windowed) in programs(t) {
            let bytes = read(&dir.join(&name))?;
            let floor = targets::LYCEUM_GLIBC_FLOOR;
            let checked = if windowed {
                headers::check_app_binary(&bytes, t, floor)
            } else {
                headers::check_console_app(&bytes, t, floor)
            };
            let d = checked.map_err(|problems| {
                format!(
                    "the header check refused {name} for {}:\n  {}",
                    t.triple,
                    problems.join("\n  ")
                )
            })?;
            println!("  {name}: {d}");
            described.push((name, d));
        }
        if t.os == Os::Mac {
            let name = "Contents/Frameworks/libMoltenVK.dylib";
            if !is_macho_library(&read(&dir.join(name))?) {
                return Err(format!("{name} is not a Mach-O library"));
            }
        }
        let program_names: Vec<String> = programs(t).into_iter().map(|(p, _)| p).collect();
        let mut members = Vec::new();
        for e in &staged {
            if e.kind == Kind::File {
                let path = dir.join(&e.path);
                members.push(Member {
                    name: e.path.clone(),
                    bytes: read(&path)?,
                    executable: executable(&path) || program_names.contains(&e.path),
                });
            }
        }
        let folders = guard::local_folders(self.ws);
        let named: Vec<String> = members
            .iter()
            .flat_map(|m| {
                guard::local_paths_in(&m.bytes, &folders)
                    .into_iter()
                    .map(move |f| format!("{}: names this machine's folder {f}", m.name))
            })
            .collect();
        if !named.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                dir.display(),
                named.join("\n  ")
            ));
        }
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
        let mut text = String::new();
        for m in &members {
            let d = described
                .iter()
                .find(|(n, _)| *n == m.name)
                .map_or("", |(_, d)| d.as_str());
            text.push_str(&format!(
                "{}\t{file}/{}\t{}\t{}\t{d}\n",
                t.triple,
                m.name,
                m.bytes.len(),
                sha256_hex(&m.bytes)
            ));
        }
        write(
            &self.out_dir.join(format!("{file}.manifest")),
            text.as_bytes(),
        )
    }
}

/// The entries of a written release file, as the deny scan reads them.
fn written_entries(path: &Path, target: Target, version: &str) -> Result<Vec<Entry>, String> {
    archive::archive_entries(path, &archive_root(target, version))
}

/// The deny scan of a written release file.
pub(crate) fn check_archive(path: &Path, target: Target, version: &str) -> Result<(), String> {
    let entries = written_entries(path, target, version)?;
    let findings = guard::scan_tree(
        &entries,
        &contents(target),
        &optional_contents(target),
        FILE_CAP,
        TOTAL_CAP,
    );
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

/// The index over a release folder: every release file scanned again, then `MANIFEST.txt`,
/// `release.json` and `SHA256SUMS`.
pub(crate) fn write_index(dir: &Path, version: &str, facts: &BuildFacts) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut releases = Vec::new();
    let mut parts = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match classify(name, version) {
            Ok(Asset::Release(t)) => releases.push((name.clone(), t)),
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
    if releases.is_empty() {
        return Err(format!(
            "no Lyceum {version} release files in {}",
            dir.display()
        ));
    }
    for (name, t) in &releases {
        check_archive(&dir.join(name), *t, version)?;
    }

    let mut manifest = format!(
        "# lyceum {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
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
            "target": t.triple,
            "platform": t.platform_name(),
            "file": name,
            "size": bytes.len(),
            "sha256": sha256_hex(&bytes),
        }));
    }
    let release = serde_json::json!({
        "schema": 1,
        "product": "lyceum",
        "version": version,
        "tag": Product::Lyceum.tag_for(version),
        "commit": facts.commit,
        "commit_time": facts.iso_time(),
        "source_url": facts.source_url,
        "prerelease": version::is_prerelease(version),
        "assets": assets,
    });
    let mut json = serde_json::to_string_pretty(&release).map_err(|e| e.to_string())?;
    json.push('\n');
    write(&dir.join("release.json"), json.as_bytes())?;

    let mut summed: Vec<String> = releases.iter().map(|(n, _)| n.clone()).collect();
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
