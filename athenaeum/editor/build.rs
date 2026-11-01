//! On Windows, link the application icon into the executable.
//!
//! Explorer, the task bar and Alt-Tab read the icon out of the executable before any of our code
//! runs, so it has to be a linked resource. `assets/` holds the same picture in the forms the
//! other platforms read: `athenaeum-256.png` is the window's icon where the window system takes
//! pixels (`src/icon.rs`), and `Athenaeum.icon` is the macOS application bundle's.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/athenaeum-editor.rc");
    println!("cargo:rerun-if-changed=assets/athenaeum.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `athenaeum.ico` as its sibling, and the two resource
        // compilers disagree about what a relative name is relative to; passing `assets/` as an
        // include directory makes it resolve under both, as the other programs' builds do.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/athenaeum-editor.rc",
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("the icon resource must compile");
    }
}
