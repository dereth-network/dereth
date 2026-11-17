//! On Windows, link the application icon and the version information into the executable.
//!
//! The version information names the executable "Auberean" with the crate's version: Explorer
//! shows it in the file's properties, and the task bar labels the client's buttons with its file
//! description rather than the file's name.
//!
//! Explorer, the task bar and Alt-Tab read the icon out of the executable before any of our code
//! runs, so it has to be a linked resource. `assets/` holds the same picture in the forms the
//! other platforms read: `auberean-256.png` is the window's icon where the window system takes
//! pixels (`src/icon.rs`), and `Auberean.icon` is the macOS application bundle's.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/auberean-client.rc");
    println!("cargo:rerun-if-changed=assets/auberean.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `auberean.ico` as its sibling, and the two resource compilers
        // disagree about what a relative name is relative to; passing `assets/` as an include
        // directory makes it resolve under both, as the other programs' builds do.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/auberean-client.rc",
            embed_resource::ParamsMacrosAndIncludeDirs(&version_macros(), [assets]),
        )
        .manifest_optional()
        .expect("the icon and version resources must compile");
    }
}

/// The crate's version as the resource script reads it: each number, and the whole as text.
fn version_macros() -> [String; 4] {
    [
        format!("VERSION_MAJOR={}", env!("CARGO_PKG_VERSION_MAJOR")),
        format!("VERSION_MINOR={}", env!("CARGO_PKG_VERSION_MINOR")),
        format!("VERSION_PATCH={}", env!("CARGO_PKG_VERSION_PATCH")),
        format!("VERSION_TEXT={}", env!("CARGO_PKG_VERSION")),
    ]
}
