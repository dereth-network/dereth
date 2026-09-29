//! Compile the Windows resources: the application icon.
//!
//! The icon has to be a linked resource rather than a file the server loads, because Explorer,
//! the task bar and Alt-Tab all read it straight out of the executable before any of our code
//! runs -- which is the whole of what an icon means for a program with no window. A server
//! started from a shell shows this icon nowhere; a server someone finds in a folder, pins to a
//! task bar, or runs as a service shows it in all three.
//!
//! `assets/` holds four files and this script uses one of them. The other three are the same
//! artwork in the forms the other platforms read, kept here so that a later window, desktop entry
//! or application bundle does not start by redrawing the picture:
//!
//! | file | who reads it | wired |
//! |---|---|---|
//! | `empyrean.ico` | Windows, out of the executable | yes, by this script |
//! | `empyrean-server.rc` | the resource compiler | yes, by this script |
//! | `empyrean-256.png` | an X11 window, or a `.desktop` entry's `Icon=` | no |
//! | `Empyrean.icon/` | macOS, compiled into an application bundle | no |
//!
//! The two unwired forms are deliberate and not an omission: the server is headless, so it has no
//! window to carry a picture and no bundle to be read out of. `dereth-client`'s `build.rs` and
//! `cargo xtask bundle-macos` are what those two forms look like when something does use them.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/empyrean-server.rc");
    println!("cargo:rerun-if-changed=assets/empyrean.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `empyrean.ico` as its sibling, and the two resource compilers
        // disagree about what a relative name is relative to: the `rc.exe`/`llvm-rc` path runs
        // with the script's own directory as its working directory, the `windres` path (the GNU
        // targets) runs in the crate root. Passing `assets/` as an include directory is what makes
        // the name resolve under both, so a `-gnu` build links the same icon a `-msvc` build does
        // rather than failing on a file it cannot find.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/empyrean-server.rc",
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("the icon resource must compile");
    }
}
