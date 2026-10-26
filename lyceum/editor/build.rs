//! Select the device-backed code for this package and target, as the client's crates do, and on
//! Windows link the application icon into the executable.
//!
//! `assets/` holds Lyceum's icon: `lyceum.ico` is linked here, because Explorer, the task bar and
//! Alt-Tab read it out of the executable before any of our code runs; `lyceum-256.png` is the
//! window's icon on the window systems that take one as pixels (`Product::ICON_PNG`), and
//! `lyceum.icns` is the macOS application bundle's. They are made from the two pictures beside
//! them: `lyceum-small.png` for 48 pixels and down, `lyceum-main.png` for the rest. `Lyceum.icon`
//! is the layered icon macOS 26 draws itself, from `lyceum-main.png`; the release package compiles
//! it into the bundle where Xcode is installed.

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
    println!("cargo:rerun-if-changed=assets/lyceum-editor.rc");
    println!("cargo:rerun-if-changed=assets/lyceum.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `lyceum.ico` as its sibling, and the two resource compilers
        // disagree about what a relative name is relative to; passing `assets/` as an include
        // directory makes it resolve under both, as the client's and the server's builds do.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/lyceum-editor.rc",
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("the icon resource must compile");
    }
}
