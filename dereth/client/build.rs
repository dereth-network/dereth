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

use std::path::PathBuf;

fn main() {
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
