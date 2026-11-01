//! `cargo xtask package athenaeum`: Athenaeum's release files (the Asheron's Call 2 world builder:
//! its editor and its `athenaeum` command, with Yalain and Auberean beside them, which the editor
//! serves and plays a world with), built as every desktop product of Asheron's Call 2's is
//! ([`super::desktop`]).
//!
//! ```text
//! cargo xtask package athenaeum [--target <triple>]... [--out <dir>] [--allow-dirty]
//! cargo xtask package athenaeum --gather <dir>
//! ```
//!
//! | system | the release file |
//! |---|---|
//! | Windows | `athenaeum-<v>-windows-x86_64.zip`: `athenaeum-editor.exe`, `athenaeum.exe`, `yalain.exe`, `auberean.exe`, the notices, README and CHANGES, under `athenaeum-<v>/` |
//! | macOS | `Athenaeum-<v>-macos-<arch>.app.tar.gz`: `Athenaeum.app`, all four programs in `Contents/MacOS`, the icon, the notices, README and CHANGES in `Contents/Resources` |
//! | Linux | `Athenaeum-<v>-linux-x86_64.AppImage`: the editor, or the program named first (`athenaeum`, `yalain`, `auberean`) |
//!
//! The editor finds the server and the client beside itself, as it does in a build from the
//! repository.

use super::desktop::{Program, Spec};
use super::version::Product;

/// The editor's program name.
pub const EDITOR: &str = "athenaeum-editor";

/// The command's program name (the `athenaeum-cli` package's binary).
pub const COMMAND: &str = "athenaeum";

/// Athenaeum's package.
pub const SPEC: Spec = Spec {
    product: Product::Athenaeum,
    name: "athenaeum",
    title: "Athenaeum",
    app_id: "network.dereth.athenaeum",
    programs: &[
        Program {
            package: "athenaeum-editor",
            manifest: "athenaeum/editor/Cargo.toml",
            binary: EDITOR,
            windowed: true,
        },
        Program {
            package: "athenaeum-cli",
            manifest: "athenaeum/cli/Cargo.toml",
            binary: COMMAND,
            windowed: false,
        },
        Program {
            package: "yalain",
            manifest: "yalain/server/Cargo.toml",
            binary: "yalain",
            windowed: false,
        },
        Program {
            package: "auberean-client",
            manifest: "auberean/client/Cargo.toml",
            binary: "auberean",
            windowed: true,
        },
    ],
    documents: &[
        ("README.md", "athenaeum/README.md"),
        ("CHANGES.md", "athenaeum/CHANGES.md"),
    ],
    about: ("auberean/about.toml", "auberean/about.hbs"),
    fonts: &[(
        "Liberation",
        "The editor's panels are drawn in the Liberation fonts (Liberation Sans and Mono, version\n\
         2.1.5), which the editor carries",
        "dereth/client/crates/classic-fonts/fonts/OFL.txt",
    )],
    icon_png: "athenaeum/editor/assets/athenaeum-256.png",
    icon_layered: "athenaeum/editor/assets/Athenaeum.icon",
    mac_category: "public.app-category.developer-tools",
    linux_categories: "Development;",
    comment: "The Asheron's Call 2 world builder",
    source_url_env: "ATHENAEUM_BUILD_SOURCE_URL",
    // The editor and the client are some tens of megabytes each. Every retail data file but the
    // language files is larger, and the name rules and both games' data-file headers catch those.
    file_cap: 96 << 20,
    total_cap: 256 << 20,
};

/// `cargo xtask package athenaeum ...` (the arguments after `athenaeum`).
pub fn package(args: &[String]) -> i32 {
    super::desktop::package(&SPEC, args)
}
