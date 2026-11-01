//! The release files of Asheron's Call 2's desktop products that are not the launcher's:
//! Athenaeum, the world builder ([`super::athenaeum`]), built the same way on a contributor's
//! machine and in its release workflow. A product is a [`Spec`] of one program or several; this
//! module is what they share. (Auberean's package is the launcher's, [`super::auberean`].)
//!
//! ```text
//! cargo xtask package athenaeum [--target <triple>]... [--out <dir>] [--allow-dirty]
//! cargo xtask package athenaeum --gather <dir>
//! ```
//!
//! For each target (the host's own by default; each builds on its own operating system) it builds
//! the product's programs with cargo, then writes into `<target dir>/package/<name>-<version>/` (or
//! `--out`):
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `<name>-<v>-windows-x86_64.zip`: the programs, the notices and the documents, under `<name>-<v>/` |
//! | macOS | `<Title>-<v>-macos-<arch>.app.tar.gz`: `<Title>.app`, the programs in `Contents/MacOS`, the icon, the notices and the documents in `Contents/Resources` |
//! | Linux | `<Title>-<v>-linux-x86_64.AppImage`: the programs, the desktop entry, the icon, the notices and the documents in one file that runs |
//!
//! Each release file's lines of `MANIFEST.txt` go in `<file>.manifest`. Then the index is written
//! over the folder: `MANIFEST.txt`, `release.json` and `SHA256SUMS`. `--gather <dir>` builds
//! nothing: it checks the files already in `<dir>` (every runner's, in the release workflow) and
//! writes the index over them.
//!
//! No such product has an updater of its own, so nothing is signed for one: the launcher downloads
//! a release and checks it against `SHA256SUMS`. The macOS bundle is code-signed as Lyceum's is: ad
//! hoc, or with a Developer ID when `APPLE_SIGNING_IDENTITY` names one ([`Spec::codesign_runs`]).
//! Asheron's Call 2's programs draw through wgpu, which draws through Metal on macOS, so no bundle
//! carries MoltenVK.
//!
//! **The AppImage** is packed by `appimagetool` (the program `APPIMAGETOOL` names, else the one on
//! `PATH`) from a folder staged here: `AppRun`, which starts the first program, or another of the
//! product's programs when the first argument names it (`Athenaeum-0.1.0-linux-x86_64.AppImage
//! athenaeum new <folder>`); the desktop entry and its icon; the programs in `usr/bin`; and the
//! notices and the documents. `APPIMAGE_RUNTIME` names the AppImage runtime to embed, so a release
//! build fetches none.
//!
//! **The dat guard** is Lyceum's: only named files are copied, and the deny scan runs over what was
//! staged and again over each written archive (over the staged folder alone for the AppImage, whose
//! inside is compressed). A file that is not on the package's list, a game data file of either game
//! or anything named like one, a link, or a file over the cap fails the package, and so does a file
//! that names one of this machine's folders.

use std::path::{Path, PathBuf};

use super::archive::{self, Member};
use super::dereth::{about_page, copy, crates_of, executable, fresh_dir, mkdir, run_in, succeeds};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::targets::{self, Arch, Os, Target};
use super::version::{self, Product};
use super::{build_facts, read, sha256_hex, write, BuildFacts};
use crate::util::target_dir;

/// One program a desktop product's package carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    /// The cargo package it is built from.
    pub package: &'static str,
    /// That package's manifest, relative to the workspace.
    pub manifest: &'static str,
    /// The program's name, without the system's executable suffix.
    pub binary: &'static str,
    /// Whether it opens a window of its own: on Windows it is then a windowed program, with its
    /// icon in its header; the others are console programs.
    pub windowed: bool,
}

/// What a desktop product's package is made of.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub product: Product,
    /// The lower-case name its files and folders use: `auberean`.
    pub name: &'static str,
    /// The name a person reads, which names the macOS bundle and the AppImage: `Auberean`.
    pub title: &'static str,
    /// What the desktops know it by: the macOS bundle's identifier, and the Linux desktop entry's
    /// and its icon's name.
    pub app_id: &'static str,
    /// The programs, the one the bundle and the AppImage start first.
    pub programs: &'static [Program],
    /// The documents every package carries beside the notices, as (name in the package, path in
    /// the tree).
    pub documents: &'static [(&'static str, &'static str)],
    /// cargo-about's configuration and template.
    pub about: (&'static str, &'static str),
    /// The typefaces the package carries: (typeface, what carries it and for what, its license).
    pub fonts: &'static [(&'static str, &'static str, &'static str)],
    /// The picture the macOS icon (`.icns`) and the Linux desktop entry's icon are made from: a
    /// square PNG of 128, 256, 512 or 1024 pixels.
    pub icon_png: &'static str,
    /// The layered icon macOS 26 draws itself, compiled into the bundle's `Assets.car` where Xcode
    /// is installed.
    pub icon_layered: &'static str,
    /// The bundle's `LSApplicationCategoryType`.
    pub mac_category: &'static str,
    /// The desktop entry's `Categories`.
    pub linux_categories: &'static str,
    /// The desktop entry's `Comment`.
    pub comment: &'static str,
    /// The repository the release files are downloaded from and the notices name as the source,
    /// when it is not the default.
    pub source_url_env: &'static str,
    /// No single file of a package is larger than this.
    pub file_cap: u64,
    /// No package holds more than this in all.
    pub total_cap: u64,
}

/// The notices every package carries.
pub const NOTICES: &[&str] = &["LICENSE", "NOTICE.txt", "THIRD-PARTY-LICENSES.html"];

/// The variables a release build of Asheron's Call 2's programs names its commit and its target
/// in, which each program's `--version` prints. Each program reads its own product's; every build
/// sets all of them, so a program built for one product's package is the same build as for
/// another's (Athenaeum's carries Yalain and Auberean).
pub const BUILD_VARS: &[(&str, &str)] = &[
    ("AUBEREAN_BUILD_COMMIT", "AUBEREAN_BUILD_TARGET"),
    ("YALAIN_BUILD_COMMIT", "YALAIN_BUILD_TARGET"),
    ("ATHENAEUM_BUILD_COMMIT", "ATHENAEUM_BUILD_TARGET"),
];

/// The environment a release build of Asheron's Call 2's programs runs with: the commit and the
/// target the programs name in `--version` ([`BUILD_VARS`]), the commit's time, and the flags that
/// keep local paths out of the programs. On Windows the C runtime is linked in and the linker
/// writes no time; on macOS the programs claim the release's oldest macOS.
pub fn build_env(ws: &Path, facts: &BuildFacts, target: Target) -> Vec<(String, String)> {
    let mut env = super::build_env(facts, target, &super::local_remaps(ws));
    for (commit, triple) in BUILD_VARS {
        env.push(((*commit).to_owned(), facts.commit.clone()));
        env.push(((*triple).to_owned(), target.triple.to_owned()));
    }
    env
}

/// The variable naming `appimagetool`.
pub const APPIMAGETOOL_ENV: &str = "APPIMAGETOOL";

/// The variable naming the AppImage runtime `appimagetool` embeds.
pub const APPIMAGE_RUNTIME_ENV: &str = "APPIMAGE_RUNTIME";

impl Spec {
    /// The release file for `target`.
    pub fn release_file_name(&self, target: Target, version: &str) -> String {
        let p = target.platform_name();
        match target.os {
            Os::Windows => format!("{}-{version}-{p}.zip", self.name),
            Os::Mac => format!("{}-{version}-{p}.app.tar.gz", self.title),
            Os::Linux => format!("{}-{version}-{p}.AppImage", self.title),
        }
    }

    /// The macOS bundle's name.
    pub fn app(&self) -> String {
        format!("{}.app", self.title)
    }

    /// The one top-level folder of the release file's archive, and of the folder an AppImage is
    /// packed from.
    pub fn archive_root(&self, target: Target, version: &str) -> String {
        match target.os {
            Os::Mac => self.app(),
            Os::Windows => format!("{}-{version}", self.name),
            Os::Linux => format!("{}.AppDir", self.title),
        }
    }

    /// Where the programs go below the top-level folder.
    fn programs_dir(target: Target) -> &'static str {
        match target.os {
            Os::Windows => "",
            Os::Mac => "Contents/MacOS/",
            Os::Linux => "usr/bin/",
        }
    }

    /// Where the notices and the documents go below the top-level folder.
    fn resources_dir(target: Target) -> &'static str {
        if target.os == Os::Mac {
            "Contents/Resources/"
        } else {
            ""
        }
    }

    /// The programs in the release file, the first first, each with whether it is windowed.
    pub fn program_paths(&self, target: Target) -> Vec<(String, bool)> {
        let dir = Self::programs_dir(target);
        let exe = target.exe_suffix();
        self.programs
            .iter()
            .map(|p| (format!("{dir}{}{exe}", p.binary), p.windowed))
            .collect()
    }

    /// The desktop entry's file name: the application id, which a desktop finds it by.
    pub fn desktop_entry_name(&self) -> String {
        format!("{}.desktop", self.app_id)
    }

    /// The desktop entry's icon's file name, which the entry names by the application id too.
    pub fn desktop_icon_name(&self) -> String {
        format!("{}.png", self.app_id)
    }

    /// The macOS bundle's flat icon's file name.
    pub fn icns_name(&self) -> String {
        format!("{}.icns", self.name)
    }

    /// The layered icon's name inside `Assets.car`: the `.icon`'s own.
    pub fn icon_name(&self) -> &'static str {
        let file = self
            .icon_layered
            .rsplit('/')
            .next()
            .unwrap_or(self.icon_layered);
        file.strip_suffix(".icon").unwrap_or(file)
    }

    /// Every file the release file holds below its top-level folder (for the AppImage, the folder
    /// it is packed from), and nothing else.
    pub fn contents(&self, target: Target) -> Vec<String> {
        let res = Self::resources_dir(target);
        let mut files: Vec<String> = self
            .program_paths(target)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        files.extend(NOTICES.iter().map(|n| format!("{res}{n}")));
        files.extend(self.documents.iter().map(|(n, _)| format!("{res}{n}")));
        match target.os {
            Os::Windows => {}
            Os::Linux => files.extend([
                "AppRun".to_owned(),
                self.desktop_entry_name(),
                self.desktop_icon_name(),
            ]),
            Os::Mac => files.extend([
                "Contents/Info.plist".to_owned(),
                "Contents/PkgInfo".to_owned(),
                format!("Contents/Resources/{}", self.icns_name()),
                "Contents/_CodeSignature/CodeResources".to_owned(),
            ]),
        }
        files.sort();
        files
    }

    /// What the release file may hold besides [`Spec::contents`]: the compiled layered icon, which
    /// only a machine with Xcode makes.
    pub fn optional_contents(&self, target: Target) -> Vec<String> {
        match target.os {
            Os::Mac => vec!["Contents/Resources/Assets.car".to_owned()],
            Os::Windows | Os::Linux => Vec::new(),
        }
    }

    /// The macOS bundle's `Info.plist`: its name, identifier, version, executable and icon. With
    /// `composed`, the bundle holds the compiled layered icon too, which `CFBundleIconName` names
    /// and macOS 26 looks for first; every macOS reads `CFBundleIconFile`.
    pub fn info_plist(&self, version: &str, composed: bool) -> String {
        let v = version.split('-').next().unwrap_or(version);
        let icon_name = if composed {
            format!(
                "\n\t<key>CFBundleIconName</key>\n\t<string>{}</string>",
                self.icon_name()
            )
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
	<string>{title}</string>
	<key>CFBundleDisplayName</key>
	<string>{title}</string>
	<key>CFBundleIdentifier</key>
	<string>{id}</string>
	<key>CFBundleExecutable</key>
	<string>{exe}</string>
	<key>CFBundleIconFile</key>
	<string>{icns}</string>{icon_name}
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
	<string>{category}</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#,
            title = self.title,
            id = self.app_id,
            exe = self.programs[0].binary,
            icns = self.icns_name(),
            category = self.mac_category,
        )
    }

    /// The Linux desktop entry: the first program.
    pub fn desktop_entry(&self) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName={}\nComment={}\nExec={}\nIcon={}\n\
             Terminal=false\nCategories={}\n",
            self.title, self.comment, self.programs[0].binary, self.app_id, self.linux_categories,
        )
    }

    /// The AppImage's `AppRun`: the first program, given every argument; or, when the first
    /// argument names another of the product's programs, that one, given the rest.
    pub fn app_run(&self) -> String {
        let others: Vec<&str> = self.programs[1..].iter().map(|p| p.binary).collect();
        let dispatch = if others.is_empty() {
            String::new()
        } else {
            format!(
                "case \"${{1:-}}\" in\n  {})\n    program=\"$1\"\n    shift\n    exec \"$here/usr/bin/$program\" \"$@\" ;;\nesac\n",
                others.join("|")
            )
        };
        format!(
            "#!/bin/sh\n# {title}'s AppImage: `{first}`, or the program named first ({names}).\nhere=\"$(dirname \"$(readlink -f \"$0\")\")\"\n{dispatch}exec \"$here/usr/bin/{first}\" \"$@\"\n",
            title = self.title,
            first = self.programs[0].binary,
            names = if others.is_empty() {
                "none other".to_owned()
            } else {
                others.join(", ")
            },
        )
    }

    /// The `codesign` runs that sign the bundle at `app`, inside out: every program but the first,
    /// then the bundle itself, which signs the first (its executable) and seals its resources.
    /// With a Developer ID (`identity`) each is signed with it under the hardened runtime, with a
    /// secure timestamp, as notarization requires; without one each is signed ad hoc, which Apple
    /// silicon needs to run it at all.
    pub fn codesign_runs(&self, app: &str, identity: Option<&str>) -> Vec<Vec<String>> {
        let id = identity
            .map(str::trim)
            .filter(|i| !i.is_empty() && *i != "-");
        let mut base: Vec<String> =
            vec!["--force".into(), "--sign".into(), id.unwrap_or("-").into()];
        if id.is_some() {
            base.extend(["--options".into(), "runtime".into(), "--timestamp".into()]);
        }
        self.programs[1..]
            .iter()
            .map(|p| format!("{app}/Contents/MacOS/{}", p.binary))
            .chain(std::iter::once(app.to_owned()))
            .map(|path| {
                let mut run = base.clone();
                run.push(path);
                run
            })
            .collect()
    }

    /// Classify a file of the release folder, or refuse it: the folder holds this version's
    /// release files, their manifest parts, and the index.
    pub fn classify(&self, name: &str, version: &str) -> Result<Asset, String> {
        if INDEX_FILES.contains(&name) {
            return Ok(Asset::Index);
        }
        for triple in targets::DESKTOP_TRIPLES {
            let t = targets::find(triple)?;
            let file = self.release_file_name(t, version);
            if name == file {
                return Ok(Asset::Release(t));
            }
            if name == format!("{file}.manifest") {
                return Ok(Asset::ManifestPart);
            }
        }
        Err(format!(
            "`{name}` is not a file of the {} {version} release (a release file, its .manifest \
             or the index)",
            self.title
        ))
    }

    /// The release target named `triple`.
    pub fn find(&self, triple: &str) -> Result<Target, String> {
        targets::find_among(targets::DESKTOP_TRIPLES, self.title, triple)
    }
}

/// What a file of a desktop product's release folder is.
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

/// A macOS icon file (`.icns`) holding one picture: the PNG `png`, square and 128, 256, 512 or
/// 1024 pixels on a side, which every macOS this releases for reads and scales.
pub fn icns_from_png(png: &[u8]) -> Result<Vec<u8>, String> {
    if png.get(..8) != Some(b"\x89PNG\r\n\x1a\n") || png.get(12..16) != Some(b"IHDR") {
        return Err("the icon picture is not a PNG".to_owned());
    }
    let side = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    let (w, h) = (side(16), side(20));
    let kind: &[u8; 4] = match (w, h) {
        (128, 128) => b"ic07",
        (256, 256) => b"ic08",
        (512, 512) => b"ic09",
        (1024, 1024) => b"ic10",
        _ => {
            return Err(format!(
                "the icon picture is {w}x{h}; an .icns takes a square of 128, 256, 512 or 1024"
            ))
        }
    };
    let entry = u32::try_from(png.len() + 8).map_err(|_| "the icon picture is too large")?;
    let total = entry + 8;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&total.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(&entry.to_be_bytes());
    out.extend_from_slice(png);
    Ok(out)
}

/// What the deny scan finds in an AppImage: its name, either game's data-file header, the cap, and
/// that it is an executable at all.
pub fn check_appimage(name: &str, bytes: &[u8], cap: u64) -> Vec<String> {
    let mut findings = Vec::new();
    if let Some(why) = guard::name_finding(name) {
        findings.push(format!("{name}: {why}"));
    }
    if guard::has_dat_magic(&bytes[..bytes.len().min(guard::HEAD_LEN)]) {
        findings.push(format!("{name}: carries the game's data-file header"));
    }
    if bytes.len() as u64 > cap {
        findings.push(format!(
            "{name}: {} bytes, over the {cap} byte cap for one file",
            bytes.len()
        ));
    }
    if bytes.get(..4) != Some(b"\x7fELF") {
        findings.push(format!("{name}: not an executable (no ELF header)"));
    }
    findings
}

/// The options of one `package <product>` run.
#[derive(Debug, Default)]
struct Options {
    targets: Vec<String>,
    out: Option<PathBuf>,
    gather: Option<PathBuf>,
    allow_dirty: bool,
}

/// The usage of `cargo xtask package <name>`.
pub fn usage(name: &str) -> String {
    format!(
        "usage: cargo xtask package {name} [--target <triple>]... [--out <dir>] [--allow-dirty]
       cargo xtask package {name} --gather <dir>"
    )
}

fn parse_options(name: &str, args: &[String]) -> Result<Options, String> {
    let usage = usage(name);
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
            "--allow-dirty" => o.allow_dirty = true,
            other => return Err(format!("unexpected argument `{other}`\n{usage}")),
        }
    }
    if o.gather.is_some() && (!o.targets.is_empty() || o.out.is_some()) {
        return Err(format!(
            "--gather builds nothing: it takes no --target or --out\n{usage}"
        ));
    }
    Ok(o)
}

/// `cargo xtask package <product> ...` (the arguments after the product's name).
pub fn package(spec: &Spec, args: &[String]) -> i32 {
    let options = match parse_options(spec.name, args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = crate::util::workspace_root();
    let result = match &options.gather {
        Some(dir) => gather(spec, &ws, dir, options.allow_dirty),
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
                spec,
                &ws,
                &triples,
                options.out.as_deref(),
                options.allow_dirty,
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
fn facts(spec: &Spec, ws: &Path, allow_dirty: bool) -> Result<BuildFacts, String> {
    let mut facts = build_facts(ws, allow_dirty)?;
    if let Ok(url) = std::env::var(spec.source_url_env) {
        if !url.trim().is_empty() {
            facts.source_url = url.trim().trim_end_matches('/').to_owned();
        }
    }
    Ok(facts)
}

/// `appimagetool`, as `APPIMAGETOOL` names it or as found on `PATH`; why not, when it does not run.
pub(super) fn appimagetool() -> Result<String, String> {
    let tool = std::env::var(APPIMAGETOOL_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "appimagetool".to_owned());
    let runs = std::process::Command::new(&tool)
        .arg("--version")
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .output()
        .is_ok();
    if runs {
        Ok(tool)
    } else {
        Err(format!(
            "appimagetool does not run here; it packs the Linux AppImage: download \
             appimagetool-x86_64.AppImage from https://github.com/AppImage/appimagetool/releases, \
             make it executable, and put it on PATH or name it in {APPIMAGETOOL_ENV}"
        ))
    }
}

/// Build, check and write every target's release file, then the index. The folder they are in.
pub fn build_packages(
    spec: &Spec,
    ws: &Path,
    triples: &[String],
    out: Option<&Path>,
    allow_dirty: bool,
) -> Result<PathBuf, String> {
    let version = spec.product.version(ws)?;
    version::parse_release_version(&version)?;
    // Everything a host lacks is said before anything is built.
    let host = super::host();
    let mut plans = Vec::new();
    let mut refusals = Vec::new();
    for triple in triples {
        match spec
            .find(triple)
            .and_then(|t| targets::plan_desktop(t, &host).map(|()| t))
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
    let mut tool = None;
    if plans.iter().any(|t| t.os == Os::Linux) {
        match appimagetool() {
            Ok(t) => tool = Some(t),
            Err(e) => refusals.push(format!("  {e}")),
        }
    }
    if !refusals.is_empty() {
        return Err(format!(
            "this host cannot package what was asked:\n{}",
            refusals.join("\n")
        ));
    }
    let facts = facts(spec, ws, allow_dirty)?;
    let base = target_dir().join("package");
    let out_dir = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.join(format!("{}-{version}", spec.name)));
    mkdir(&out_dir)?;
    for target in plans {
        println!("\n=== {} {version} for {}", spec.name, target.triple);
        let job = Job {
            spec,
            ws,
            target,
            facts: &facts,
            version: &version,
            out_dir: &out_dir,
            stage: &base
                .join("stage")
                .join(format!("{}-{}", spec.name, target.triple)),
            appimagetool: tool.as_deref(),
        };
        job.run()?;
    }
    write_index(spec, &out_dir, &version, &facts)?;
    Ok(out_dir)
}

/// `--gather <dir>`: check the files there and write the index over them.
fn gather(spec: &Spec, ws: &Path, dir: &Path, allow_dirty: bool) -> Result<PathBuf, String> {
    let version = spec.product.version(ws)?;
    let facts = facts(spec, ws, allow_dirty)?;
    write_index(spec, dir, &version, &facts)?;
    Ok(dir.to_path_buf())
}

/// Each program's name in the package, with what the header check read from it.
type Described = Vec<(String, String)>;

/// One target's build.
struct Job<'a> {
    spec: &'a Spec,
    ws: &'a Path,
    target: Target,
    facts: &'a BuildFacts,
    version: &'a str,
    out_dir: &'a Path,
    /// This target's staging folder, made anew.
    stage: &'a Path,
    /// `appimagetool` (Linux).
    appimagetool: Option<&'a str>,
}

impl Job<'_> {
    fn run(&self) -> Result<(), String> {
        fresh_dir(self.stage)?;
        let built = self.build()?;
        let notices = self.notices()?;
        let root = self.spec.archive_root(self.target, self.version);
        let tree = self.stage.join(&root);
        self.lay_out(&tree, &built, &notices)?;
        if self.target.os == Os::Linux {
            self.finish_appimage(&tree)
        } else {
            self.finish_archive(&tree, &root)
        }
    }

    /// Every program, built with cargo in the workspace, each copied out of the target folder, so a
    /// later build there cannot change what this package holds. In the spec's order.
    fn build(&self) -> Result<Vec<PathBuf>, String> {
        let t = self.target;
        let mut args = vec!["build", "--locked", "--release", "--target", t.triple];
        for p in self.spec.programs {
            args.extend(["-p", p.package]);
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
        self.spec
            .programs
            .iter()
            .map(|p| {
                let name = format!("{}{exe}", p.binary);
                let to = self.stage.join("bin").join(&name);
                copy(&release.join(&name), &to)?;
                Ok(to)
            })
            .collect()
    }

    /// `LICENSE`, `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: the folder they are written in.
    fn notices(&self) -> Result<PathBuf, String> {
        let t = self.target;
        let mut crates = Vec::new();
        for p in self.spec.programs {
            crates.extend(crates_of(self.ws, "Cargo.toml", p.package, t)?);
        }
        crates.sort();
        crates.dedup();
        let mit = String::from_utf8_lossy(&read(&self.ws.join("LICENSE"))?).into_owned();
        let mut fonts = Vec::new();
        for (name, carried, path) in self.spec.fonts {
            fonts.push((
                *name,
                *carried,
                String::from_utf8_lossy(&read(&self.ws.join(path))?).into_owned(),
            ));
        }
        let about = self.stage.join("about");
        mkdir(&about)?;
        let exe = t.exe_suffix();
        let names: Vec<String> = self
            .spec
            .programs
            .iter()
            .map(|p| format!("{}{exe}", p.binary))
            .collect();
        let mut pages = Vec::new();
        for (p, name) in self.spec.programs.iter().zip(&names) {
            let page = about_page(self.ws, self.spec.about, p.manifest, t, &about, p.binary)?;
            pages.push((name.as_str(), page));
        }
        let dir = self.stage.join("notices");
        mkdir(&dir)?;
        let text = notice::game_notice(&notice::GameFacts {
            product: self.spec.product,
            version: self.version,
            target: t.triple,
            commit: &self.facts.commit,
            source_url: &self.facts.source_url,
            crates: &crates,
            mit_license: &mit,
            fonts: &fonts,
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
    fn lay_out(&self, tree: &Path, built: &[PathBuf], notices: &Path) -> Result<(), String> {
        let t = self.target;
        let spec = self.spec;
        fresh_dir(tree)?;
        for ((name, _), from) in spec.program_paths(t).iter().zip(built) {
            copy(from, &tree.join(name))?;
        }
        let res = Spec::resources_dir(t);
        for n in NOTICES {
            copy(&notices.join(n), &tree.join(format!("{res}{n}")))?;
        }
        for (name, from) in spec.documents {
            copy(&self.ws.join(from), &tree.join(format!("{res}{name}")))?;
        }
        match t.os {
            Os::Windows => Ok(()),
            Os::Linux => {
                write(&tree.join("AppRun"), spec.app_run().as_bytes())?;
                set_executable(&tree.join("AppRun"))?;
                for (name, _) in spec.program_paths(t) {
                    set_executable(&tree.join(name))?;
                }
                write(
                    &tree.join(spec.desktop_entry_name()),
                    spec.desktop_entry().as_bytes(),
                )?;
                copy(
                    &self.ws.join(spec.icon_png),
                    &tree.join(spec.desktop_icon_name()),
                )
            }
            Os::Mac => {
                let icns = icns_from_png(&read(&self.ws.join(spec.icon_png))?)?;
                write(
                    &tree.join(format!("Contents/Resources/{}", spec.icns_name())),
                    &icns,
                )?;
                let composed = self.compose_icon(&tree.join("Contents/Resources"))?;
                write(
                    &tree.join("Contents/Info.plist"),
                    spec.info_plist(self.version, composed).as_bytes(),
                )?;
                // Four bytes of classic Mac OS that the Finder still reads before the plist.
                write(&tree.join("Contents/PkgInfo"), b"APPL????")?;
                self.sign_bundle(tree)
            }
        }
    }

    /// The layered icon compiled into `resources/Assets.car` with Xcode's `actool`, when it is here
    /// and compiles it; the bundle keeps its flat icon either way, for every earlier macOS. Whether
    /// it did.
    fn compose_icon(&self, resources: &Path) -> Result<bool, String> {
        let app = self.spec.app();
        if !crate::bundle::actool_is_installed() {
            println!("package: no actool (Xcode) here; {app} has its flat icon only");
            return Ok(false);
        }
        let work = self.stage.join("icon");
        fresh_dir(&work)?;
        let floor = format!("{}.{}", targets::MACOS_FLOOR.0, targets::MACOS_FLOOR.1);
        let source = self.ws.join(self.spec.icon_layered);
        if !crate::bundle::actool_compile(self.ws, &source, self.spec.icon_name(), &floor, &work) {
            println!(
                "package: actool could not compile {}; {app} has its flat icon only",
                source.display()
            );
            return Ok(false);
        }
        copy(&work.join("Assets.car"), &resources.join("Assets.car"))?;
        Ok(true)
    }

    /// Sign the bundle at `app` ([`Spec::codesign_runs`]) and verify it as the system will.
    fn sign_bundle(&self, app: &Path) -> Result<(), String> {
        let identity = std::env::var("APPLE_SIGNING_IDENTITY").ok();
        let path = app.display().to_string();
        match identity
            .as_deref()
            .map(str::trim)
            .filter(|i| !i.is_empty() && *i != "-")
        {
            Some(_) => println!(
                "package: signing {} with APPLE_SIGNING_IDENTITY",
                self.spec.app()
            ),
            None => println!(
                "package: signing {} ad hoc (no APPLE_SIGNING_IDENTITY)",
                self.spec.app()
            ),
        }
        for run in self.spec.codesign_runs(&path, identity.as_deref()) {
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

    /// The deny scan of the staged folder `dir` and the header checks of its programs: its files,
    /// and each program's line for the manifest.
    fn check_tree(&self, dir: &Path) -> Result<(Vec<Member>, Described), String> {
        let t = self.target;
        let spec = self.spec;
        let staged = guard::staged_entries(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let findings = guard::scan_tree(
            &staged,
            &spec.contents(t),
            &spec.optional_contents(t),
            spec.file_cap,
            spec.total_cap,
        );
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                dir.display(),
                findings.join("\n  ")
            ));
        }
        let mut described = Vec::new();
        for (name, windowed) in spec.program_paths(t) {
            let bytes = read(&dir.join(&name))?;
            let floor = targets::DESKTOP_GLIBC_FLOOR;
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
        let program_names: Vec<String> =
            spec.program_paths(t).into_iter().map(|(p, _)| p).collect();
        let mut members = Vec::new();
        for e in &staged {
            if e.kind == Kind::File {
                let path = dir.join(&e.path);
                members.push(Member {
                    name: e.path.clone(),
                    bytes: read(&path)?,
                    executable: executable(&path)
                        || program_names.contains(&e.path)
                        || e.path == "AppRun",
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
        Ok((members, described))
    }

    /// Scan the folder `dir`, check its programs, archive it under `root`, scan the archive, and
    /// write its manifest lines.
    fn finish_archive(&self, dir: &Path, root: &str) -> Result<(), String> {
        let t = self.target;
        let (members, described) = self.check_tree(dir)?;
        let bytes = if t.os == Os::Windows {
            archive::zip_bytes(root, &members, self.facts.epoch)?
        } else {
            archive::tar_gz_bytes(root, &members, self.facts.epoch)?
        };
        let file = self.spec.release_file_name(t, self.version);
        let path = self.out_dir.join(&file);
        write(&path, &bytes)?;
        check_archive(self.spec, &path, t, self.version)?;
        println!(
            "  {}: {} bytes, sha256 {}",
            path.display(),
            bytes.len(),
            sha256_hex(&bytes)
        );
        self.write_manifest(&file, None, &members, &described)
    }

    /// Scan the folder `dir`, check its programs, pack it into the AppImage with `appimagetool`,
    /// check that, and write its manifest lines.
    fn finish_appimage(&self, dir: &Path) -> Result<(), String> {
        let t = self.target;
        let (members, described) = self.check_tree(dir)?;
        let tool = self
            .appimagetool
            .ok_or("a Linux package needs appimagetool")?;
        let file = self.spec.release_file_name(t, self.version);
        let packed = self.stage.join(&file);
        let runtime = std::env::var(APPIMAGE_RUNTIME_ENV)
            .ok()
            .filter(|r| !r.trim().is_empty());
        let dir_arg = dir.display().to_string();
        let packed_arg = packed.display().to_string();
        let mut args = vec!["--no-appstream"];
        if let Some(r) = &runtime {
            args.extend(["--runtime-file", r.as_str()]);
        }
        args.extend([dir_arg.as_str(), packed_arg.as_str()]);
        let arch = match t.arch {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        };
        let env = vec![
            ("ARCH".to_owned(), arch.to_owned()),
            ("SOURCE_DATE_EPOCH".to_owned(), self.facts.epoch.to_string()),
            ("APPIMAGE_EXTRACT_AND_RUN".to_owned(), "1".to_owned()),
        ];
        run_in(self.ws, tool, &args, &env, &[])?;
        let bytes = read(&packed)?;
        let findings = check_appimage(&file, &bytes, self.spec.file_cap.max(self.spec.total_cap));
        if !findings.is_empty() {
            return Err(format!(
                "the deny scan refused {}:\n  {}",
                packed.display(),
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
        self.write_manifest(&file, Some(&bytes), &members, &described)
    }

    /// `<file>.manifest`: the release file itself when it is not an archive (`whole`), then each
    /// member's size, SHA-256 and what the header check read.
    fn write_manifest(
        &self,
        file: &str,
        whole: Option<&[u8]>,
        members: &[Member],
        described: &[(String, String)],
    ) -> Result<(), String> {
        let triple = self.target.triple;
        let mut text = String::new();
        if let Some(bytes) = whole {
            text.push_str(&format!(
                "{triple}\t{file}\t{}\t{}\tAppImage\n",
                bytes.len(),
                sha256_hex(bytes)
            ));
        }
        for m in members {
            let d = described
                .iter()
                .find(|(n, _)| *n == m.name)
                .map_or("", |(_, d)| d.as_str());
            text.push_str(&format!(
                "{triple}\t{file}/{}\t{}\t{}\t{d}\n",
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

/// Make a staged file executable (nothing to do where there is no such bit).
fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let _ = path;
    Ok(())
}

/// The entries of a written archive, as the deny scan reads them.
fn written_entries(
    spec: &Spec,
    path: &Path,
    target: Target,
    version: &str,
) -> Result<Vec<Entry>, String> {
    archive::archive_entries(path, &spec.archive_root(target, version))
}

/// The deny scan of a written release file: an archive's entries, or an AppImage as one file.
pub(crate) fn check_archive(
    spec: &Spec,
    path: &Path,
    target: Target,
    version: &str,
) -> Result<(), String> {
    let findings = if target.os == Os::Linux {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        check_appimage(&name, &read(path)?, spec.file_cap.max(spec.total_cap))
    } else {
        let entries = written_entries(spec, path, target, version)?;
        guard::scan_tree(
            &entries,
            &spec.contents(target),
            &spec.optional_contents(target),
            spec.file_cap,
            spec.total_cap,
        )
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

/// The index over a release folder: every release file scanned again, then `MANIFEST.txt`,
/// `release.json` and `SHA256SUMS`.
pub(crate) fn write_index(
    spec: &Spec,
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
    let mut releases = Vec::new();
    let mut parts = Vec::new();
    let mut refused = Vec::new();
    for name in &names {
        match spec.classify(name, version) {
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
            "no {} {version} release files in {}",
            spec.title,
            dir.display()
        ));
    }
    for (name, t) in &releases {
        check_archive(spec, &dir.join(name), *t, version)?;
    }

    let mut manifest = format!(
        "# {} {version}, commit {}\n# target\tfile\tsize\tsha256\theader\n",
        spec.name, facts.commit
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
        "product": spec.name,
        "version": version,
        "tag": spec.product.tag_for(version),
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
