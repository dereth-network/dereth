//! `cargo xtask dlss`: NVIDIA's DLSS library and the SDK's license, downloaded from NVIDIA's
//! repository at the commit `dereth/client/dlss.env` pins, each checked against its pinned hash,
//! into the per-user cache the client's build copies the library from. A Windows client built
//! with DLSS (the default there) then finds it beside its executable in the target folder, as the
//! release carries it. It also puts the library beside each client already built here, whose
//! build would not otherwise look again.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::util::workspace_root;

// The pin, the cache and the build's copy, shared with the client's build script.
#[allow(dead_code)]
#[path = "../../../dereth/client/dlss.rs"]
pub mod pin;

pub fn dlss(args: &[String]) -> i32 {
    if let Some(other) = args.first() {
        eprintln!("cargo xtask dlss takes no arguments, not {other:?}");
        return 2;
    }
    match fetch() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("cargo xtask dlss: {e}");
            1
        }
    }
}

fn fetch() -> Result<(), String> {
    let pin = pin::Pin::pinned();
    let dir = pin::cache_dir(pin::Host::this(), |n| std::env::var_os(n), &pin.commit)
        .ok_or("no per-user cache folder: set LOCALAPPDATA (Windows) or HOME")?;
    let files = [
        (pin::LIBRARY_PATH, pin::LIBRARY, &pin.library_sha256),
        (pin::LICENSE_PATH, pin::LICENSE, &pin.license_sha256),
    ];
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (path, name, sha256) in files {
        let to = dir.join(name);
        if std::fs::read(&to).is_ok_and(|held| checked(&held, sha256).is_ok()) {
            println!("{} is already there", to.display());
            continue;
        }
        let url = pin.url(path);
        let bytes = download(&url)?;
        checked(&bytes, sha256).map_err(|e| format!("{url}: {e}"))?;
        // Written whole beside it, then put in its place, so a cut-off write is never taken for
        // the file.
        let part = dir.join(format!("{name}.part"));
        std::fs::write(&part, &bytes).map_err(|e| format!("{}: {e}", part.display()))?;
        std::fs::rename(&part, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        println!("{} fetched", to.display());
    }
    let library = dir.join(pin::LIBRARY);
    for exe_dir in built_clients(&target_dir()) {
        let to = exe_dir.join(pin::LIBRARY);
        std::fs::copy(&library, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        println!("{} placed beside the client built there", to.display());
    }
    Ok(())
}

/// `bytes` when their SHA-256 is `sha256` (lowercase hexadecimal); otherwise what was found.
///
/// # Errors
/// The hash the bytes have, beside the one pinned.
pub fn checked<'a>(bytes: &'a [u8], sha256: &str) -> Result<&'a [u8], String> {
    let found: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if found == sha256 {
        Ok(bytes)
    } else {
        Err(format!("its SHA-256 is {found}, not the pinned {sha256}"))
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("dereth-xtask-dlss/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .into();
    agent
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?
        .body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// The workspace's target folder: `CARGO_TARGET_DIR` when set, else `target` at its root.
fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map_or_else(
            || workspace_root().join("target"),
            |d| {
                if d.is_absolute() {
                    d
                } else {
                    workspace_root().join(d)
                }
            },
        )
}

/// The folders under `target` holding a built Windows client: each profile's
/// (`target/<profile>`) and each cross-built target's (`target/<triple>/<profile>`).
pub fn built_clients(target: &Path) -> Vec<PathBuf> {
    let dirs = |d: &Path| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(d)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        found
    };
    dirs(target)
        .into_iter()
        .flat_map(|d| {
            let below = dirs(&d);
            std::iter::once(d).chain(below)
        })
        .filter(|d| d.join("dereth-client.exe").is_file())
        .collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: fetching NVIDIA's DLSS library for builds from this tree).

    use super::*;

    /// A file whose SHA-256 is not the pinned one is refused; the pinned one's is taken.
    #[test]
    fn a_download_whose_hash_is_not_the_pinned_one_is_refused() {
        // The SHA-256 of "abc", from FIPS 180-2's own example.
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(checked(b"abc", abc), Ok(&b"abc"[..]));
        let wrong = checked(b"abd", abc).expect_err("another file");
        assert!(wrong.contains(abc), "{wrong}");
        // Nothing but the exact lowercase digits is taken.
        assert!(checked(b"abc", &abc.to_uppercase()).is_err());
    }

    /// The library goes beside each built client in the target folder, whether built for this host
    /// or for a named target, and nowhere else.
    #[test]
    fn the_library_goes_beside_each_client_built_in_the_target_folder() {
        let target = std::env::temp_dir().join(format!("xtask-dlss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&target);
        for d in ["release", "debug", "x86_64-pc-windows-msvc/release"] {
            std::fs::create_dir_all(target.join(d)).unwrap();
        }
        std::fs::create_dir_all(target.join("test-release")).unwrap();
        for d in ["release", "x86_64-pc-windows-msvc/release"] {
            std::fs::write(target.join(d).join("dereth-client.exe"), b"").unwrap();
        }
        let found = built_clients(&target);
        let _ = std::fs::remove_dir_all(&target);
        assert_eq!(
            found,
            [
                target.join("release"),
                target.join("x86_64-pc-windows-msvc").join("release"),
            ]
        );
    }
}
