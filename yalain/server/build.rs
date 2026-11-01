//! Compile the Windows resources: the application icon.
//!
//! The icon has to be a linked resource rather than a file the server loads, because Explorer,
//! the task bar and Alt-Tab all read it straight out of the executable before any of our code
//! runs -- which is the whole of what an icon means for a program with no window.
//!
//! `assets/` holds four files and this script uses two of them. The other two are the same
//! artwork in the forms the other platforms read, kept here so that a later window, desktop entry
//! or application bundle does not start by redrawing the picture:
//!
//! | file | who reads it | wired |
//! |---|---|---|
//! | `yalain.ico` | Windows, out of the executable | yes, by this script |
//! | `yalain-server.rc` | the resource compiler | yes, by this script |
//! | `yalain-256.png` | an X11 window, or a `.desktop` entry's `Icon=` | no |
//! | `Yalain.icon/` | macOS, compiled into an application bundle | no |

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/yalain-server.rc");
    println!("cargo:rerun-if-changed=assets/yalain.ico");
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        // The resource script names `yalain.ico` as its sibling, and the two resource compilers
        // disagree about what a relative name is relative to; passing `assets/` as an include
        // directory makes it resolve under both, as the other servers' and clients' builds do.
        let assets: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
        embed_resource::compile(
            "assets/yalain-server.rc",
            embed_resource::ParamsIncludeDirs([assets]),
        )
        .manifest_optional()
        .expect("the icon resource must compile");
    }
}
