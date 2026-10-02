//! The Dereth 3D client: the window, the graphics device, the renderer, the retail UI and the sound
//! device, plugged into the runtime's frame loop.
//!
//! **Depends on** the client shell (`dereth-client-shell`) and the scene (`dereth-scene`), which are
//! the application, the desktop host under them (`dereth-desktop`), and the runtime and everything
//! they are built from (`dereth-client-runtime`, `dereth-client-model`, `dereth-client-contract`,
//! `dereth-client-net`, `dereth-audio`, `dereth-primitives`, `dereth-dat`, `dereth-assets`,
//! `dereth-physics`, `dereth-animation`, `dereth-transport`, `dereth-protocol`) and its own crates
//! under `dereth/client/crates/`: drawing (`dereth-render`, `dereth-world-render`), the UI
//! (`dereth-ui`, `dereth-ui-screens`), the device input (`dereth-input`) and the console
//! (`dereth-console`); on Linux, the launcher's desktop-entry registration (`dereth-launch`).
//! **Used by** nothing but the client's test kit (`dereth-testkit`).
//!
//! **Must never** do another crate's work: if something here starts decoding an asset, transforming
//! a vertex or parsing a message, that work belongs elsewhere and the move is to widen that crate's
//! API. It is the one crate permitted to depend on many subsystems at once, which is exactly why
//! nothing but the test kit may depend on it. It keeps the workspace's `forbid(unsafe_code)`.
//!
//! The frame loop is the runtime's and the application is the client shell's, generic over its
//! host. The host is the desktop's ([`dereth_desktop`]), and what this crate adds is the product
//! itself, [`Dereth`]: its name, its settings folder, its icon. Every module of the shell and of
//! the desktop host is re-exported here at its old path, with [`app::App`] and
//! [`app::ClientShell`] on the [`Desktop`] host.
//!
//! ```text
//! dereth-client -- --headless --frames 1 --capture out.png
//! ```

pub use dereth_client_shell::*;

pub mod app;
pub use dereth_desktop::{audio, clipboard, cursor, folders, platform, pump};
// The HUD, with the desktop's platform answers.
pub mod hud;

/// The Dereth client, as a product on the desktop host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dereth;

impl dereth_desktop::Product for Dereth {
    const BINARY_NAME: &'static str = "dereth-client";
    const BUILD_ID: &'static str = concat!("dereth-client ", env!("CARGO_PKG_VERSION"));
    const SETTINGS_DIR_NAME: &'static str = folders::CLIENT_DIR_NAME;
    const TITLE: &'static str = "Dereth";
    const ICON_PNG: &'static [u8] = include_bytes!("../assets/dereth-256.png");
    /// `dereth_launch::desktop::CLIENT_APP_ID`, which the launcher writes the entry under too.
    const APP_ID: &'static str = "network.dereth.dereth-client";

    /// The client window's hidden `.desktop` entry and the icon it names, under the user's data
    /// folder, each written only when it is missing or its content differs. A client the launcher
    /// started finds both as the launcher wrote them and writes nothing; a client started on its
    /// own writes them itself. A failure is logged: the client runs with a generic icon rather than
    /// not at all.
    #[cfg(target_os = "linux")]
    fn before_window() {
        use dereth_launch::desktop;

        let var = |v: &str| std::env::var_os(v);
        let (Some(home), Ok(me)) = (desktop::data_home(&var), std::env::current_exe()) else {
            return;
        };
        let exec = desktop::entry_target(&var, me);
        match desktop::write_changed(&desktop::client_files(&home, &exec, Self::ICON_PNG)) {
            Ok(written) => {
                for path in written {
                    tracing::debug!("desktop entry written: {}", path.display());
                }
            }
            Err(e) => tracing::warn!("desktop entry: {e}"),
        }
    }
}

/// The desktop, as the client shell's host, for the Dereth client.
pub type Desktop = dereth_desktop::Desktop<Dereth>;

/// What `dereth-client --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
#[must_use]
pub fn version_text() -> String {
    format!(
        "dereth-client {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("DERETH_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("DERETH_BUILD_TARGET").unwrap_or("unknown"),
    )
}

#[cfg(test)]
mod tests {
    /// Behaviour: none (the release build's identification, read by the release smoke run).
    #[test]
    fn the_version_text_names_the_client_and_its_version_first() {
        let text = super::version_text();
        let first = text.lines().next().unwrap();
        assert_eq!(
            first,
            format!("dereth-client {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(text.lines().any(|l| l.starts_with("target: ")), "{text}");
    }
}
