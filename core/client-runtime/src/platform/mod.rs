//! The device-free half of the client's platform seam.
//!
//! What is here is the plain data the host resolves its own types into before the client sees
//! them. The traits whose signatures still name a windowing or graphics type, and every
//! implementation of them, stay in the front end. Keys and window messages are not here: the
//! runtime is told what the player does as actions ([`crate::actions`]).

/// The host's text conversion and the chat log's file handle: what `dereth-client-model` needs
/// the kernel32 NLS arm and `std::fs` for. It lives here because `objects.rs`'s `new_world`
/// installs it on every `World` the client builds.
pub mod text;

/// The clock seam without the zone: `Clock`, `Pacer` and `system_unix_time`.
/// `local_utc_offset_secs`, the two `Clock` implementations and `Timer` live in
/// `dereth_client::platform::clock`, which re-exports the three here.
pub mod clock;

/// The window seam: `WindowHost`, `HostEvent`, `PumpedEvents` and `NullWindow`. `WinitWindow`
/// and every `winit` name live in `dereth_client::platform::window`, which re-exports what is
/// here.
pub mod window;

/// The modal error box the client shows when its first connection fails, before any screen
/// exists: `ErrorDialogHost` and the headless `NoDialog`. The OS message box stays in
/// `dereth_client::platform::dialog`.
pub mod dialog;

/// The audio endpoint the host opens for the mixer.
pub mod audio_out;

/// The desktop URL launch the host installs.
pub mod shell;

/// The caret blink interval the host installs.
pub mod caret;

/// The client's own files -- the preferences, the keymaps, the screen layouts -- as the host keeps
/// them: the disk, unless the host installs a store of its own.
pub mod files;
