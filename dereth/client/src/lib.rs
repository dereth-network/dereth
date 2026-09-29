//! The Dereth 3D client: the window, the graphics device, the renderer, the retail UI and the sound
//! device, plugged into the runtime's frame loop.
//!
//! **Depends on** the client shell (`dereth-client-shell`) and the scene (`dereth-scene`), which are
//! the application, and the runtime and everything they are built from (`dereth-client-runtime`,
//! `dereth-client-model`, `dereth-client-contract`, `dereth-client-net`, `dereth-audio`,
//! `dereth-primitives`, `dereth-dat`, `dereth-assets`, `dereth-physics`, `dereth-animation`,
//! `dereth-transport`, `dereth-protocol`) and its own crates under `dereth/client/crates/`: drawing
//! (`dereth-render`, `dereth-world-render`), the UI (`dereth-ui`, `dereth-ui-screens`), the device
//! input (`dereth-input`), the clipboard (`dereth-clipboard`) and the console (`dereth-console`); on
//! Linux, the launcher's desktop-entry registration (`dereth-launch`).
//! **Used by** nothing but the client's test kit (`dereth-testkit`).
//!
//! **Must never** do another crate's work: if something here starts decoding an asset, transforming
//! a vertex or parsing a message, that work belongs elsewhere and the move is to widen that crate's
//! API. It is the one crate permitted to depend on many subsystems at once, which is exactly why
//! nothing but the test kit may depend on it. It keeps the workspace's `forbid(unsafe_code)`: the
//! unsafe calls it needs live in `dereth-clipboard` and `dereth-console`.
//!
//! The frame loop is the runtime's and the application is the client shell's, generic over its
//! host. What this crate is is the desktop platform under it: the [`Desktop`] host, the `winit`
//! window and its event loop ([`platform`], [`pump`]), the operating system's time zone and URL
//! launch, the `cpal` sound device ([`audio`]), the system clipboard ([`clipboard`]) and the system
//! cursors ([`cursor`]). Every module of the shell is re-exported here at its old path, with
//! [`app::App`] and [`app::ClientShell`] on the desktop host.
//!
//! ```text
//! dereth-client -- --headless --frames 1 --capture out.png
//! ```

pub use dereth_client_shell::*;

pub mod app;
// The sound device: the runtime's mixer over `cpal`.
pub mod audio;
// The clipboard bridge over the system clipboard.
pub mod clipboard;
// The cursor state over the system cursors.
pub mod cursor;
// Where the client keeps its files on this platform, and the one-time move from where it kept
// them before.
pub mod folders;
mod host;
// The HUD, with the desktop's platform answers.
pub mod hud;
/// The operating system's half of the platform: the window and its event loop, the local time zone
/// every date the client draws needs, and the OS error box.
pub mod platform;
// The message pump: `winit` events in, Win32 messages out, the window procedure in the middle.
pub mod pump;

pub use host::Desktop;

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
