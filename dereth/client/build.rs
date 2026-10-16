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
//! It also names the commit the client is built from ([`build_commit`]).

use std::path::{Path, PathBuf};

fn main() {
    build_commit();
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
