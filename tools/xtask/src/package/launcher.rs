//! `cargo xtask package launcher`: the launcher's own release files: the launcher with no game
//! client beside it. Every install updates from these once the updater reads the launcher's
//! releases; until then nothing does.
//!
//! ```text
//! cargo xtask package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>] [--friends-dll <file>]
//! cargo xtask package launcher --gather <dir>
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

use super::auberean::{self, Asset, Flavor};
use super::dereth;
use super::targets::{self, Os, Target};
use super::version::Product;

pub const USAGE: &str = "usage: cargo xtask package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>] [--friends-dll <file>]
       cargo xtask package launcher --gather <dir>";

/// The launcher's release files.
pub(crate) const FLAVOR: Flavor = Flavor {
    product: Product::Launcher,
    client: None,
    release_file_name,
    archive_root,
    contents,
    classify,
    signed_as: |_, version| version.to_owned(),
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

/// `cargo xtask package launcher ...` (the arguments after `launcher`).
pub fn package(args: &[String]) -> i32 {
    auberean::package_flavor(&FLAVOR, USAGE, versions, args)
}
