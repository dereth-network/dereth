//! What each product's GitHub release carries, as its release workflow drafts it: the files, their
//! labels, the title, the downloads text under the notes, and the pre-release and "Latest" rules.
//!
//! Each machine's upload adds two kinds of file the workflow's release never carries, because the
//! workflow keeps them between its jobs instead: each release file's lines of `MANIFEST.txt`
//! (`<file>.manifest`) and, for Dereth, each platform's entry of `latest.json`
//! (`latest-<platform>.json`). They are the draft's **staging** files: `--finish` reads them to
//! write the merged files, and `--publish` removes them.

use crate::package::dereth as launcher;
use crate::package::targets::{self, TARGETS};
use crate::package::version::{self, Product};
use crate::package::web;

/// A product released on its own tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The launcher, with the client inside it.
    Dereth,
    Empyrean,
    /// The web client, which carries Dereth's version.
    Web,
}

impl Kind {
    pub fn from_command_name(name: &str) -> Option<Self> {
        match name {
            "dereth" => Some(Self::Dereth),
            "empyrean" => Some(Self::Empyrean),
            "web" => Some(Self::Web),
            _ => None,
        }
    }

    pub fn command_name(self) -> &'static str {
        match self {
            Self::Dereth => "dereth",
            Self::Empyrean => "empyrean",
            Self::Web => "web",
        }
    }

    /// The product whose version the tree carries, and whose notes the release gets.
    pub fn product(self) -> Product {
        match self {
            Self::Empyrean => Product::Empyrean,
            Self::Dereth | Self::Web => Product::Dereth,
        }
    }

    /// The release's tag.
    pub fn tag(self, version: &str) -> String {
        match self {
            Self::Web => format!("{}{version}", web::TAG_PREFIX),
            _ => self.product().tag_for(version),
        }
    }

    /// The release's title.
    pub fn title(self, version: &str) -> String {
        match self {
            Self::Dereth => format!("Dereth {version}"),
            Self::Empyrean => format!("Empyrean {version}"),
            Self::Web => format!("Dereth {version} for the web"),
        }
    }
}

/// What a file of a release is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A download: an archive, the launcher, the web bundle or its manifest.
    Release,
    /// A file a machine uploads for `--finish` to merge, removed when the release is published.
    Staging,
    /// A file `--finish` writes over every machine's files.
    Index,
}

/// What `name` is in `kind`'s release of `version`, or why it belongs to none.
pub fn role(kind: Kind, version: &str, name: &str) -> Result<Role, String> {
    match kind {
        Kind::Dereth => Ok(match launcher::classify(name, version)? {
            launcher::Asset::Launcher(_) => Role::Release,
            launcher::Asset::ManifestPart | launcher::Asset::Piece(_) => Role::Staging,
            launcher::Asset::Index => Role::Index,
        }),
        Kind::Empyrean => Ok(match crate::package::classify(name, version)? {
            crate::package::Asset::Archive(_) => Role::Release,
            crate::package::Asset::ManifestPart => Role::Staging,
            crate::package::Asset::Index => Role::Index,
        }),
        Kind::Web => {
            if name == web::archive_name(version) || name == web::MANIFEST {
                Ok(Role::Release)
            } else if name == "SHA256SUMS" {
                Ok(Role::Index)
            } else {
                Err(format!(
                    "`{name}` is not a file of the web client {version} release ({}, {} or \
                     SHA256SUMS)",
                    web::archive_name(version),
                    web::MANIFEST
                ))
            }
        }
    }
}

/// Every file a finished release holds, by role, each list sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub release: Vec<String>,
    pub staging: Vec<String>,
    pub index: Vec<String>,
}

/// What `kind`'s release of `version` holds when every machine has uploaded and `--finish` has
/// run: every target's files, as the workflow's release.
pub fn expected(kind: Kind, version: &str) -> Expected {
    let mut e = Expected {
        release: Vec::new(),
        staging: Vec::new(),
        index: Vec::new(),
    };
    match kind {
        Kind::Dereth => {
            for triple in targets::DERETH_TRIPLES {
                let Ok(t) = targets::find(triple) else {
                    continue;
                };
                let file = launcher::release_file_name(t, version);
                e.staging.push(format!("{file}.manifest"));
                e.staging.push(launcher::piece_name(t));
                e.release.push(file);
            }
            e.index.extend(
                ["SHA256SUMS", "MANIFEST.txt", "release.json", "latest.json"].map(String::from),
            );
        }
        Kind::Empyrean => {
            for t in TARGETS {
                let file = t.archive_name(version);
                e.staging.push(format!("{file}.manifest"));
                e.release.push(file);
            }
            e.index
                .extend(["SHA256SUMS", "MANIFEST.txt", "release.json"].map(String::from));
        }
        Kind::Web => {
            e.release.push(web::archive_name(version));
            e.release.push(web::MANIFEST.to_owned());
            e.index.push("SHA256SUMS".to_owned());
        }
    }
    e.release.sort();
    e.staging.sort();
    e.index.sort();
    e
}

/// The files `SHA256SUMS` lists in a finished release: every download and the index beside it.
pub fn summed(kind: Kind, version: &str) -> Vec<String> {
    let e = expected(kind, version);
    let mut names = e.release;
    names.extend(e.index.into_iter().filter(|n| n != "SHA256SUMS"));
    names.sort();
    names
}

/// The label a staging file is shown with while the release is a draft.
pub const STAGING_LABEL: &str = "Staging file (removed when the release is published)";

/// The display label GitHub shows for `name`, the workflow's for the same file; `None` for a file
/// the release does not carry.
pub fn label(kind: Kind, version: &str, name: &str) -> Option<String> {
    let common = match name {
        "SHA256SUMS" => Some("Checksums (SHA-256)"),
        "MANIFEST.txt" if kind != Kind::Web => Some("Release manifest"),
        "release.json" if kind != Kind::Web => Some("Release facts (JSON)"),
        "latest.json" if kind == Kind::Dereth => Some("Updater manifest"),
        _ => None,
    };
    if let Some(l) = common {
        return Some(l.to_owned());
    }
    if role(kind, version, name).ok() == Some(Role::Staging) {
        return Some(STAGING_LABEL.to_owned());
    }
    let label = match kind {
        Kind::Dereth => match name.strip_prefix(&format!("dereth-{version}-")) {
            Some("windows-x86_64.zip") => "Dereth · Windows x64 (zip, no installer)",
            Some(_) => return None,
            None => match name.strip_prefix(&format!("Dereth-{version}-"))? {
                "macos-aarch64.app.tar.gz" => "Dereth · macOS Apple silicon",
                "macos-x86_64.app.tar.gz" => "Dereth · macOS Intel",
                "linux-x86_64.AppImage" => "Dereth · Linux x86-64 AppImage (glibc 2.35+)",
                _ => return None,
            },
        },
        Kind::Empyrean => match name.strip_prefix(&format!("empyrean-{version}-"))? {
            "x86_64-pc-windows-msvc.zip" => "Empyrean server · Windows x64",
            "x86_64-unknown-linux-gnu.tar.gz" => "Empyrean server · Linux x86-64 (glibc 2.28+)",
            "aarch64-unknown-linux-gnu.tar.gz" => "Empyrean server · Linux arm64 (glibc 2.28+)",
            "aarch64-apple-darwin.tar.gz" => "Empyrean server · macOS Apple silicon",
            "x86_64-apple-darwin.tar.gz" => "Empyrean server · macOS Intel",
            _ => return None,
        },
        Kind::Web => {
            if name == web::MANIFEST {
                "Web client manifest (JSON)"
            } else if name == web::archive_name(version) {
                "Dereth · web client (static files)"
            } else {
                return None;
            }
        }
    };
    Some(label.to_owned())
}

/// How a release is published: whether it is a pre-release, and GitHub's `make_latest`.
///
/// A version with a pre-release suffix (`0.2.0-rc.1`) is a pre-release, which is never the
/// repository's "Latest". The web client's release is never "Latest" either: that stays the
/// launcher's and the server's, which people download by hand, and the web client's consumers list
/// the releases instead. A final Dereth or Empyrean release is made "Latest", as publishing a draft
/// by hand does by default.
pub fn publish_flags(kind: Kind, version: &str) -> (bool, &'static str) {
    let pre = version::is_prerelease(version);
    let latest = if pre || kind == Kind::Web {
        "false"
    } else {
        "true"
    };
    (pre, latest)
}

/// The description a draft carries until `--finish` writes the notes.
pub const PLACEHOLDER_BODY: &str = "Draft: the release files are being uploaded from each \
     build machine. `cargo xtask publish <product> <version> --finish` writes the notes.";

const DERETH_DOWNLOADS: &str = "
## Downloads

Dereth {VERSION}: the launcher, with the Dereth client inside it, for Windows (10 or newer, x86-64), macOS (11 or newer, Apple silicon and Intel) and Linux (x86-64, glibc 2.35 or newer: Ubuntu 22.04 and later).

| system | download | |
|---|---|---|
| Windows | `dereth-{VERSION}-windows-x86_64.zip` | unzip anywhere and run `dereth.exe`; no installer |
| macOS | `Dereth-{VERSION}-macos-aarch64.app.tar.gz` (Apple silicon), `-x86_64` (Intel) | unpack and move `Dereth.app` to Applications |
| Linux | `Dereth-{VERSION}-linux-x86_64.AppImage` | mark it executable and run it |

The launcher keeps itself and the client up to date from this repository's newest Dereth release.

**No game data is included**: the client plays with the data files of your own Asheron's Call install.

These builds are not code-signed yet. Windows SmartScreen asks once (More info, then Run anyway); macOS asks once (System Settings, Privacy & Security, Open Anyway).

Verifying a download: `sha256sum -c SHA256SUMS --ignore-missing`
";

const EMPYREAN_DOWNLOADS: &str = "
## Downloads

Empyrean {VERSION}, the Asheron's Call server (a Rust port of ACE), for Windows, Linux (glibc 2.28 or newer, x86-64 and arm64) and macOS (11 or newer, Apple silicon and Intel).

**No game data is included**: the server reads the client's data files from your own install, and `empyrean-import fetch --pack` builds the world from ACE-World's database release. SETUP.md in each archive is the guide.

The source of this release is https://github.com/{REPO}/tree/{COMMIT}, and GitHub's source archives of the tag, **Source code (zip)** and **Source code (tar.gz)**, below.

Verifying a download: `sha256sum -c SHA256SUMS --ignore-missing`
";

const WEB_DOWNLOADS: &str = "
## Downloads

The Dereth client {VERSION} for web browsers: static files to serve from any web host (`dereth/web/DEPLOY.md` says how, and which headers to send).

| file | what |
|---|---|
| `dereth-web-{VERSION}.zip` | the page, its scripts and the WebAssembly module, under `dereth-web-{VERSION}/` |
| `web.json` | the release's manifest: version, commit, and every file's size and SHA-256, for hosts and tools that update themselves |

**No game data is included**: each player's browser reads the data files of their own Asheron's Call install, on their own machine.

Verifying a download: `sha256sum -c SHA256SUMS --ignore-missing`
";

/// The release's description: its notes, then what the downloads are, as the workflow writes it.
pub fn description(kind: Kind, version: &str, repo: &str, commit: &str, notes: &str) -> String {
    let template = match kind {
        Kind::Dereth => DERETH_DOWNLOADS,
        Kind::Empyrean => EMPYREAN_DOWNLOADS,
        Kind::Web => WEB_DOWNLOADS,
    };
    let downloads = template
        .replace("{VERSION}", version)
        .replace("{REPO}", repo)
        .replace("{COMMIT}", commit);
    format!("{notes}{downloads}")
}

/// The `(name, sha256)` lines of a `SHA256SUMS` file, or the first line that is not one.
pub fn parse_sums(text: &str) -> Result<Vec<(String, String)>, String> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (hash, name) = l
                .split_once("  ")
                .ok_or_else(|| format!("`{l}` is not a SHA256SUMS line"))?;
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("`{l}` carries no SHA-256"));
            }
            Ok((name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

/// `SHA256SUMS` over `(name, sha256)`, in the form `sha256sum -c` reads, sorted by name.
pub fn sums_text(files: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
        .iter()
        .map(|(name, hash)| format!("{hash}  {name}\n"))
        .collect()
}
