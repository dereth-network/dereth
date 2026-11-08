//! Compile the Windows resources: the application icon.
//!
//! The icon has to be a linked resource rather than a file the client loads, because Explorer,
//! the taskbar and Alt-Tab all read it straight out of the executable before any of our code runs.
//! The other two platforms need no build step either: X11 takes the icon from the window, which
//! `dereth/desktop/src/platform/window.rs` sets from `assets/dereth-256.png`, and macOS takes it from the
//! application bundle that `cargo xtask bundle-macos` builds out of `assets/Dereth.icon` -- a
//! binary run straight out of `target/` is not a bundle there and has no icon, which is the
//! platform's own rule.
//!
//! `assets/dereth.ico` is this crate's **own** copy, deliberately duplicated from the launcher's
//! `launcher/assets/dereth.ico` rather than referenced across the two trees: the launcher and the
//! client are separate applications in separate workspaces, and either one may want a different
//! icon without disturbing the other. Change this file's icon by replacing `assets/dereth.ico`.
//!
//! It also names the commit the client is built from ([`build_commit`]), and puts NVIDIA's DLSS
//! library beside a Windows client built with it ([`dlss_beside_client`]).

use std::path::{Path, PathBuf};

// Shared with `cargo xtask dlss`, which uses the half of it this script does not.
#[allow(dead_code)]
mod dlss;

fn main() {
    build_commit();
    dlss_beside_client();
    println!("cargo:rustc-check-cfg=cfg(gpu)");
    let feature = |name| std::env::var_os(name).is_some();
    if feature("CARGO_FEATURE_VULKAN")
        || feature("CARGO_FEATURE_WGPU")
        || (feature("CARGO_CFG_WINDOWS") && feature("CARGO_FEATURE_D3D12"))
    {
        println!("cargo:rustc-cfg=gpu");
    }
    println!("cargo:rerun-if-changed=assets/dereth-client.rc");
    println!("cargo:rerun-if-changed=assets/dereth.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `dereth.ico` as its sibling, and the two resource compilers
        // disagree about what a relative name is relative to: the `rc.exe`/`llvm-rc` path runs
        // with the script's own directory as its working directory, the `windres` path (the GNU
        // targets) runs in the crate root. Passing `assets/` as an include directory is what
        // makes the name resolve under both, so a `-gnu` build links the same icon a `-msvc`
        // build does rather than failing on a file it cannot find.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/dereth-client.rc",
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("the icon resource must compile");
    }
}

/// NVIDIA's DLSS library beside the executable of a Windows client built with DLSS, so a client run
/// from the target folder loads it as the release does: copied from the folder `DERETH_DLSS_DIR`
/// names, else from the per-user cache `cargo xtask dlss` fills (see `dlss.rs`). Nothing is
/// downloaded here. Without the library the build says how to fetch it and goes on; the client
/// then draws its DLSS modes with FSR.
fn dlss_beside_client() {
    println!("cargo:rerun-if-changed=dlss.rs");
    println!("cargo:rerun-if-changed=dlss.env");
    println!("cargo:rerun-if-env-changed={}", dlss::DIR_ENV);
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    if !windows || std::env::var_os("CARGO_FEATURE_DLSS").is_none() {
        return;
    }
    for var in dlss::CACHE_VARS {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let Some(exe_dir) = std::env::var_os("OUT_DIR").and_then(|o| dlss::exe_dir(Path::new(&o)))
    else {
        return;
    };
    let pin = dlss::Pin::pinned();
    let cache = dlss::cache_dir(dlss::Host::this(), |n| std::env::var_os(n), &pin.commit);
    let dir = std::env::var_os(dlss::DIR_ENV).map(PathBuf::from);
    match dlss::plan(dir, cache, &exe_dir, Path::is_file) {
        dlss::Plan::Copy { from, to } => {
            // Both are watched, so a new library in the cache or one removed from beside the
            // client copies again.
            println!("cargo:rerun-if-changed={}", from.display());
            let same = |a: &Path, b: &Path| {
                std::fs::read(a)
                    .ok()
                    .is_some_and(|a| std::fs::read(b).is_ok_and(|b| a == b))
            };
            if !same(&from, &to) {
                if let Err(e) = std::fs::copy(&from, &to) {
                    println!(
                        "cargo:warning=NVIDIA's DLSS library could not be copied to {}: {e}",
                        to.display()
                    );
                    return;
                }
            }
            println!("cargo:rerun-if-changed={}", to.display());
        }
        dlss::Plan::Missing { looked } => {
            let looked = looked.map_or_else(
                || "no per-user cache folder was found".to_owned(),
                |l| format!("none at {}", l.display()),
            );
            println!(
                "cargo:warning=NVIDIA's DLSS library is not beside the client ({looked}): run \
                 `cargo xtask dlss` to fetch it; until then the client upscales with FSR"
            );
        }
    }
}

/// The commit the client is built from, as `@report` and `--version` name it:
/// `DERETH_BUILD_COMMIT` when the packaging names it, else the checkout's own `HEAD` where git can
/// say, so a client built straight from a checkout names its commit too. Built again when `HEAD`
/// moves.
fn build_commit() {
    println!("cargo:rerun-if-env-changed=DERETH_BUILD_COMMIT");
    if std::env::var_os("DERETH_BUILD_COMMIT").is_some() {
        return;
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let git = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(here)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        (out.status.success() && !text.is_empty()).then_some(text)
    };
    let Some(commit) = git(&["rev-parse", "--short=12", "HEAD"]) else {
        return;
    };
    // `HEAD`, its log (written whenever `HEAD` moves, a commit on its branch too), the branch it
    // names and the packed branches, where each is a file.
    let branch = git(&["symbolic-ref", "-q", "HEAD"]);
    for name in ["HEAD", "logs/HEAD", "packed-refs"]
        .into_iter()
        .map(str::to_owned)
        .chain(branch)
    {
        if let Some(path) = git(&["rev-parse", "--git-path", &name]) {
            let path = here.join(path);
            if path.is_file() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    println!("cargo:rustc-env=DERETH_BUILD_COMMIT={commit}");
}
