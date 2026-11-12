//! The desktop client: a window in which Asheron's Call 2's world is drawn and played.
//!
//! The program is `main.rs`. This library is what of its desktop host another program shares: the
//! window system's keyboard and mouse as the shell reads them ([`keys`]), and the friends service's
//! stream on the desktop ([`friends`]).
//!
//! **Depends on** the shell's input and its friends news (`auberean-shell`) and the window system
//! (`winit`); the rest of the package's dependencies are the program's: the game's interface
//! (`auberean-interface`, `auberean-ui`), the world it shows (`auberean-scene`), the world's device
//! (`auberean-gpu`, `auberean-present`) and its frames (`auberean-draw`), the data files
//! (`auberean-dat`, `auberean-assets`, `auberean-world`, `auberean-client-sdk`), the shared math
//! (`dereth-primitives`), the console (`dereth-console`), the clipboard (`dereth-clipboard`) and the
//! sound device (`dereth-audio-out`, `dereth-audio`). **Used by** the `auberean` program beside it,
//! and the world builder's editor (`athenaeum-editor`), which plays the game in its own window.
//!
//! **Must never** hold the game or open a window: it reads the window system's events and carries
//! the friends stream for a host that has a window of its own.

pub mod friends;
pub mod keys;
