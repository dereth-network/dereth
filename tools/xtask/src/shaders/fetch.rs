//! The shader compilers `cargo xtask shaders` runs: pinned releases, downloaded once into a
//! per-user cache folder and checked against the SHA-256 committed here.
//!
//! - **Slang** ([`SLANG_VERSION`]), from its GitHub release, one zip per host. Its `bin/` and
//!   `lib/` folders are kept, with its licence (Apache-2.0 with the LLVM exception) and its
//!   third-party notices beside them; the rest of the archive (documentation, headers) is not.
//! - **DXC** ([`DXC_TAG`]), for the DXIL target only, from its GitHub release, on Windows only:
//!   `dxcompiler.dll` and `dxil.dll` for this machine's architecture, with DXC's licences. `slangc`
//!   finds it on the `PATH` it is run with.
//!
//! A download whose SHA-256 differs from the pin is refused and nothing is cached. An archive is
//! unpacked beside its final folder and renamed into place only when complete, with a stamp naming
//! the archive's SHA-256, so the cache never holds a partial or a different tool. Nothing here ships
//! with the client: the tools stay in the cache, and the blobs they make are checked in.

use std::ffi::OsString;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::package::sha256_hex;

/// The Slang release the blobs are compiled with. A bump changes this, every row of
/// [`SLANG_ASSETS`] and, through the manifests, every blob: run `cargo xtask shaders` after it.
pub const SLANG_VERSION: &str = "2026.18.3";

/// Slang's release zips, per host: `(operating system, architecture, asset, SHA-256)`. The
/// SHA-256 values are the ones GitHub publishes for the assets of release `v2026.18.3`.
pub const SLANG_ASSETS: &[(&str, &str, &str, &str)] = &[
    (
        "windows",
        "x86_64",
        "slang-2026.18.3-windows-x86_64.zip",
        "1339d7b3050ae680c11152b8ccff977173591ce8e9c1d2a5cff2eb904d52e50d",
    ),
    (
        "windows",
        "aarch64",
        "slang-2026.18.3-windows-aarch64.zip",
        "b2b8d9f9492698b4836c3a3326fa97666c25574f7e23b022aa2d551c300fa054",
    ),
    (
        "linux",
        "x86_64",
        "slang-2026.18.3-linux-x86_64.zip",
        "516cbcb524f89a8e6185c12e59d4dcd203292e7d75d162325234ce6525f5f614",
    ),
    (
        "linux",
        "aarch64",
        "slang-2026.18.3-linux-aarch64.zip",
        "3fa8e91d5562b9151509dea1430b6bc4126d6f6d12571135a202d2fc65a11e25",
    ),
    (
        "macos",
        "aarch64",
        "slang-2026.18.3-macos-aarch64.zip",
        "d06187a6f90aaee68a5ceef453cf973016e2d3adfc61ce51b1cd6a0d204cfca2",
    ),
    (
        "macos",
        "x86_64",
        "slang-2026.18.3-macos-x86_64.zip",
        "af1f4f51a472fc6a56541db2396eaacff1bdf7efe028928156b471798b3aa092",
    ),
];

/// The DXC release the DXIL target is compiled with, its Windows zip and that zip's SHA-256 (the
/// value GitHub publishes for it).
pub const DXC_TAG: &str = "v1.9.2609";
const DXC_ASSET: &str = "dxc_2026_09_29.zip";
const DXC_SHA256: &str = "ad31b1fc8443175d204f77a611fdb3ef2ec42759bdc2f1167368de24a4a7e7f1";

/// The largest archive accepted (Slang's zips are 29–87 MB).
const ZIP_LIMIT: u64 = 512 * 1024 * 1024;

/// One pinned download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// The folder it is unpacked into, under the cache folder: the tool and its version.
    pub folder: String,
    pub url: String,
    pub sha256: &'static str,
}

/// The Slang release zip for `os` and `arch` (Rust's `std::env::consts` spellings), when Slang
/// publishes one.
#[must_use]
pub fn slang_pin(os: &str, arch: &str) -> Option<Pin> {
    SLANG_ASSETS
        .iter()
        .find(|(o, a, _, _)| *o == os && *a == arch)
        .map(|(_, _, asset, sha256)| Pin {
            folder: format!("slang-{SLANG_VERSION}"),
            url: format!(
                "https://github.com/shader-slang/slang/releases/download/v{SLANG_VERSION}/{asset}"
            ),
            sha256,
        })
}

/// The DXC release zip, which serves every Windows architecture; `None` off Windows.
#[must_use]
pub fn dxc_pin(os: &str) -> Option<Pin> {
    (os == "windows").then(|| Pin {
        folder: format!("dxc-{}", DXC_TAG.trim_start_matches('v')),
        url: format!(
            "https://github.com/microsoft/DirectXShaderCompiler/releases/download/{DXC_TAG}/{DXC_ASSET}"
        ),
        sha256: DXC_SHA256,
    })
}

/// The per-user folder the tools are cached in, on `os`, reading the environment through `env`
/// (a variable that is unset, empty or not an absolute path counts as absent): `Dereth\shader-tools`
/// under `LOCALAPPDATA` on Windows, `Library/Caches/Dereth/shader-tools` under `HOME` on macOS,
/// and `dereth/shader-tools` under `XDG_CACHE_HOME` or `HOME/.cache` elsewhere.
#[must_use]
pub fn cache_dir_for(os: &str, env: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name: &str| {
        env(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    Some(match os {
        "windows" => var("LOCALAPPDATA")?.join("Dereth").join("shader-tools"),
        "macos" => var("HOME")?.join("Library/Caches/Dereth/shader-tools"),
        _ => var("XDG_CACHE_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".cache")))?
            .join("dereth/shader-tools"),
    })
}

/// [`cache_dir_for`] on this host.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    cache_dir_for(std::env::consts::OS, |name| std::env::var_os(name))
}

/// Which archive entries a tool keeps, and where: `Some(path inside the tool's folder)`, or `None`
/// for an entry left out. `name` has forward slashes.
pub type Keep = fn(&str) -> Option<String>;

/// Slang keeps its programs and libraries, its licence and its notices.
#[must_use]
pub fn keep_slang(name: &str) -> Option<String> {
    let top = name.split('/').next().unwrap_or("");
    matches!(
        top,
        "bin" | "lib" | "LICENSE" | "LICENSES" | "third-party-notices"
    )
    .then(|| name.to_owned())
}

/// DXC keeps this machine's two libraries, flattened into the folder, and its licences.
#[must_use]
pub fn keep_dxc(name: &str) -> Option<String> {
    let arch = if std::env::consts::ARCH == "aarch64" {
        "arm64"
    } else {
        "x64"
    };
    let file = name.rsplit('/').next().unwrap_or("");
    if name.starts_with("LICEN") && !name.contains('/') {
        return Some(file.to_owned());
    }
    (name == format!("bin/{arch}/{file}") && matches!(file, "dxcompiler.dll" | "dxil.dll"))
        .then(|| file.to_owned())
}

/// The stamp a complete tool folder holds: the SHA-256 of the archive it was unpacked from.
const STAMP: &str = ".fetched";

/// The tool folder for `pin` under `cache`, downloading and unpacking it first unless a complete
/// one is already there.
pub fn ensure(cache: &Path, pin: &Pin, keep: Keep) -> Result<PathBuf, String> {
    let dir = cache.join(&pin.folder);
    if std::fs::read_to_string(dir.join(STAMP)).is_ok_and(|s| s.trim() == pin.sha256) {
        return Ok(dir);
    }
    println!("fetching {}", pin.url);
    let zip = download(&pin.url)?;
    verify(pin, &sha256_hex(&zip))?;
    std::fs::create_dir_all(cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let partial = cache.join(format!("{}.partial", pin.folder));
    let unpacked = (|| -> Result<(), String> {
        if partial.exists() {
            std::fs::remove_dir_all(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
        }
        unpack(std::io::Cursor::new(zip), &partial, keep)?;
        std::fs::write(partial.join(STAMP), pin.sha256)
            .map_err(|e| format!("{}: {e}", partial.display()))?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::rename(&partial, &dir).map_err(|e| format!("{}: {e}", dir.display()))
    })();
    if let Err(e) = unpacked {
        std::fs::remove_dir_all(&partial).ok();
        return Err(format!(
            "unpacking {} into {}: {e}",
            pin.url,
            cache.display()
        ));
    }
    println!("  kept in {}", dir.display());
    Ok(dir)
}

/// The download's SHA-256 against the pin; a mismatch names both and caches nothing.
pub fn verify(pin: &Pin, actual: &str) -> Result<(), String> {
    if pin.sha256.eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(format!(
            "{} does not match the SHA-256 committed with the pin: expected {}, downloaded \
             {actual}. Nothing was cached.",
            pin.url, pin.sha256
        ))
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("dereth-xtask/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(900)))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(ZIP_LIMIT).read_to_vec())
        .map_err(|e| {
            format!(
                "downloading {url}: {e}. Check the network connection (a proxy is read from \
                 HTTPS_PROXY or ALL_PROXY), or download it by hand and unpack it into the folder \
                 `--tools` names"
            )
        })
}

/// An archive entry's name as a relative path of plain components, with forward slashes; `None`
/// for one that is absolute or climbs out of the folder.
#[must_use]
pub fn safe_name(raw: &str) -> Option<String> {
    let name = raw.replace('\\', "/");
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
    let plain = parts
        .iter()
        .all(|p| *p != "." && *p != ".." && !p.contains(':'));
    (plain && !parts.is_empty() && !name.starts_with('/')).then(|| parts.join("/"))
}

/// Unpacks the entries `keep` keeps into `dir`. Executables keep their mode and symbolic links stay
/// links on Unix, where Slang's libraries are linked by version.
pub fn unpack<R: Read + Seek>(zip: R, dir: &Path, keep: Keep) -> Result<usize, String> {
    let mut archive =
        zip::ZipArchive::new(zip).map_err(|e| format!("the download is not a zip: {e}"))?;
    let mut kept = 0;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("zip entry {i}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = safe_name(entry.name())
            .ok_or_else(|| format!("zip entry {:?} leaves the folder", entry.name()))?;
        let Some(rel) = keep(&name) else { continue };
        let dest = dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mode = entry.unix_mode();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{name}: {e}"))?;
        if mode.is_some_and(|m| m & 0o170_000 == 0o120_000) {
            link(&dest, &bytes)?;
        } else {
            std::fs::write(&dest, &bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
            executable(&dest, mode)?;
        }
        kept += 1;
    }
    Ok(kept)
}

#[cfg(unix)]
fn link(dest: &Path, target: &[u8]) -> Result<(), String> {
    let target = std::str::from_utf8(target).map_err(|e| format!("{}: {e}", dest.display()))?;
    std::os::unix::fs::symlink(target, dest).map_err(|e| format!("{}: {e}", dest.display()))
}

#[cfg(not(unix))]
fn link(dest: &Path, _target: &[u8]) -> Result<(), String> {
    Err(format!(
        "{}: the archive holds a symbolic link, which this host's tools do not use",
        dest.display()
    ))
}

#[cfg(unix)]
fn executable(dest: &Path, mode: Option<u32>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(m) if m & 0o111 != 0 => {
            std::fs::set_permissions(dest, std::fs::Permissions::from_mode(m & 0o777))
                .map_err(|e| format!("{}: {e}", dest.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
fn executable(_dest: &Path, _mode: Option<u32>) -> Result<(), String> {
    Ok(())
}
