//! `cargo xtask package launcher`: the launcher's own release files: the launcher with no game
//! client beside it. Every install updates from these once the updater reads the launcher's
//! releases; until then nothing does. A game's package is built around them: it names the
//! launcher version in the tree, and takes that release ([`fetch`], [`take`]) or fails.
//!
//! ```text
//! cargo xtask package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>] [--friends-dll <file>]
//! cargo xtask package launcher --gather <dir>
//! cargo xtask package launcher --fetch <dir> [--wait] [--repo <owner/name>]
//! ```
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `launcher-<v>-windows-x86_64.zip`: `dereth.exe`, the retail client's friends library and the notices, in one folder |
//! | macOS | `Launcher-<v>-macos-aarch64.app.tar.gz`: `Dereth.app`, with the launcher alone in `Contents/MacOS` |
//! | Linux | `Launcher-<v>-linux-x86_64.AppImage` |
//!
//! It is built as [`super::auberean`] builds its files, from the same launcher, with no client put
//! in. The dat guard, the header checks, the manifest lines, the update signatures and the index
//! are the same too.

use std::path::{Path, PathBuf};

use super::auberean::{self, Asset, Flavor};
use super::dereth::{self, BuiltLauncher};
use super::targets::{self, Os, Target};
use super::version::Product;
use crate::publish::{self, fetch_release};
use crate::util::workspace_root;

pub const USAGE: &str = "usage: cargo xtask package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>] [--friends-dll <file>]
       cargo xtask package launcher --gather <dir>
       cargo xtask package launcher --fetch <dir> [--wait] [--repo <owner/name>]";

/// The checksums file in a folder of a release's files.
const SUMS: &str = "SHA256SUMS";

/// The launcher's release files.
pub(crate) const FLAVOR: Flavor = Flavor {
    product: Product::Launcher,
    client: None,
    release_file_name,
    archive_root,
    contents,
    classify,
    notes: |version, _| format!("The Dereth Network launcher {version}"),
};

/// The release file for `target`.
pub fn release_file_name(target: Target, version: &str) -> String {
    let p = target.platform_name();
    match target.os {
        Os::Windows => format!("launcher-{version}-{p}.zip"),
        Os::Mac => format!("Launcher-{version}-{p}.app.tar.gz"),
        Os::Linux => format!("Launcher-{version}-{p}.AppImage"),
    }
}

/// The one top-level folder of the release file's archive; `None` for the AppImage.
pub fn archive_root(target: Target, version: &str) -> Option<String> {
    match target.os {
        Os::Windows => Some(format!("launcher-{version}")),
        Os::Mac => Some(dereth::LAUNCHER_APP.to_owned()),
        Os::Linux => None,
    }
}

/// The files the release file's archive holds below its top-level folder: those it must hold, and
/// those it may (what the bundler writes on some machines and not others). Empty for the AppImage.
pub fn contents(target: Target) -> (Vec<String>, Vec<String>) {
    let notices_in = |dir: &str| -> Vec<String> {
        dereth::NOTICES
            .iter()
            .map(|n| format!("{dir}{n}"))
            .collect()
    };
    let friends = super::friends_dll::FILE;
    let mut required = Vec::new();
    let mut optional = Vec::new();
    match target.os {
        Os::Linux => {}
        Os::Windows => {
            required.push(format!("dereth{}", target.exe_suffix()));
            required.push(friends.to_owned());
            required.extend(notices_in(""));
        }
        Os::Mac => {
            required.push("Contents/Info.plist".to_owned());
            required.push("Contents/MacOS/dereth".to_owned());
            required.push(format!("Contents/Resources/{friends}"));
            required.extend(notices_in("Contents/Resources/"));
            optional.push("Contents/PkgInfo".to_owned());
            optional.push("Contents/Resources/Assets.car".to_owned());
            optional.push("Contents/_CodeSignature/CodeResources".to_owned());
            optional.push(format!("Contents/Resources/{}", dereth::launcher_icns()));
            optional.extend(dereth::client_icons());
        }
    }
    required.sort();
    optional.sort();
    (required, optional)
}

/// Classify a file of the release folder, or refuse it.
pub fn classify(name: &str, version: &str) -> Result<Asset, String> {
    if auberean::INDEX_FILES.contains(&name) {
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
        "`{name}` is not a file of the launcher {version} release (a release file, its .manifest, \
         a latest-<platform>.json piece or the index)"
    ))
}

/// The version a package is built at, twice: the release's and the launcher's are the same.
fn versions(ws: &std::path::Path) -> Result<(String, String), String> {
    let version = Product::Launcher.version(ws)?;
    super::version::parse_release_version(&version)?;
    Ok((version.clone(), version))
}

/// `--fetch <dir> [--wait] [--repo <owner/name>]`'s options.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FetchOptions {
    pub dir: PathBuf,
    pub wait: bool,
    pub repo: Option<String>,
}

/// What follows `package launcher --fetch`, starting at the folder.
pub(super) fn parse_fetch(args: &[String]) -> Result<FetchOptions, String> {
    let mut it = args.iter();
    let dir = it
        .next()
        .filter(|d| !d.starts_with("--"))
        .ok_or_else(|| format!("--fetch needs a folder\n{USAGE}"))?;
    let mut o = FetchOptions {
        dir: PathBuf::from(dir),
        wait: false,
        repo: None,
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--wait" => o.wait = true,
            "--repo" => {
                o.repo = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| format!("--repo needs a value\n{USAGE}"))?,
                );
            }
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    Ok(o)
}

/// `--fetch`: the release of the launcher version in the tree, its files written to the folder.
fn fetch(o: &FetchOptions) -> Result<(), String> {
    let version = Product::Launcher.version(&workspace_root())?;
    let tag = Product::Launcher.tag_for(&version);
    publish::fetch_tagged_release(&tag, &o.dir, o.wait, o.repo.as_deref())
}

/// `cargo xtask package launcher ...` (the arguments after `launcher`).
pub fn package(args: &[String]) -> i32 {
    if args.first().is_some_and(|a| a == "--fetch") {
        let done = parse_fetch(&args[1..]).map(|o| fetch(&o));
        return match done {
            Ok(Ok(())) => 0,
            Ok(Err(e)) => {
                eprintln!("\npackage: FAILED\n{e}");
                1
            }
            Err(e) => {
                eprintln!("{e}");
                2
            }
        };
    }
    auberean::package_flavor(&FLAVOR, USAGE, versions, args)
}

/// The launcher release file for `target` in the folder `dir`, unpacked under `stage` and checked
/// against the folder's checksums. Anything but exactly that version's file is refused: a package
/// never holds some other launcher.
pub(crate) fn take(
    dir: &Path,
    target: Target,
    version: &str,
    stage: &Path,
) -> Result<BuiltLauncher, String> {
    let file = release_file_name(target, version);
    let path = dir.join(&file);
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "the launcher {version}, which this tree names, has no {file} in {}: {e}\n\
             Get its release with `cargo xtask package launcher --fetch <dir>`, or build one with \
             `cargo xtask package launcher --out <dir>`, and pass --launcher <dir>",
            dir.display()
        )
    })?;
    let sums = std::fs::read_to_string(dir.join(SUMS))
        .map_err(|e| format!("{} has no {SUMS}: {e}", dir.display()))?;
    fetch_release::check_sum(&sums, &file, &bytes)?;
    dereth::fresh_dir(stage)?;
    let library = super::friends_dll::FILE;
    match target.os {
        Os::Windows => {
            let root = archive_root(target, version).ok_or("not an archive")?;
            super::archive::unpack_zip(&bytes, &root, stage)?;
            Ok(BuiltLauncher {
                bundle: stage.join("dereth.exe"),
                library: stage.join(library),
            })
        }
        Os::Mac => {
            let root = archive_root(target, version).ok_or("not an archive")?;
            let app = stage.join(&root);
            super::archive::unpack_tar_gz(&bytes, &root, &app)?;
            let library = app.join("Contents/Resources").join(library);
            Ok(BuiltLauncher {
                bundle: app,
                library,
            })
        }
        Os::Linux => {
            let appimage = stage.join(&file);
            dereth::copy(&path, &appimage)?;
            Ok(BuiltLauncher {
                bundle: appimage,
                // Inside the AppImage, where the package finds it when it unpacks that.
                library: PathBuf::from(library),
            })
        }
    }
}
