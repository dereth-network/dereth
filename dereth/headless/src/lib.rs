//! The Dereth client with no window, no device and no UI, driven by a text script of actions.
//!
//! **Depends on** the client SDK (`dereth-client-sdk`) alone among this workspace's crates; the
//! binary also installs a `tracing-subscriber` log on stderr. **Used by** the client's test kit
//! (`dereth-testkit`); the binary is `--script <file>` around the library.
//!
//! **Must never** depend on any other `dereth-*` crate or on anything under `dereth/client/`
//! (`cargo xtask seams`, `seam: headless deps`), name a key or device message (`seam: action seam
//! code`), or open a socket, a window or a graphics device. The only files it reads besides a
//! script are a recorded session under `fixtures/packet-captures/` and the retail data in the
//! directory `--dat-dir` names (else the working directory or the executable's).
//!
//! It is the runtime's application with nothing plugged in: `NullShell` for the front end,
//! `NullPresentation` for the device, `Platform::headless` for the window and the clock. [`script`]
//! parses, [`run`](mod@run) executes, [`capture`] reads the recordings `login` replays; the library is what
//! the crate's own tests drive, so an end-to-end case is a function call and not a subprocess.
//!
//! One command per line, `#` to the end of a line a comment; `say` keeps everything after its verb
//! verbatim:
//!
//! | command | what it does |
//! |---|---|
//! | `login <session> [<character>]` | replay a recorded session's login; with a character, select it and carry on into the world |
//! | `tick <n>` | run `n` frames |
//! | `hold <action> <secs>` | begin the action, run `secs` of simulated time, end it |
//! | `press <action>` | begin the action, run a frame, end it, run a frame |
//! | `begin <action>` / `end <action>` | one edge of the action alone, and the frame it lands in |
//! | `walk <direction> <secs>` | `hold` of a movement action: `forward`, `back`, `left`, `right`, `strafe-left`, `strafe-right` |
//! | `use <object-id>` | the Use request for an object (decimal or `0x` hex) |
//! | `say <text>` | a chat line, as the chat bar sends it |
//! | `dump events` / `dump steps` | the last frame's event log, or only its steps |
//! | `snapshot` | a summary of the game view |
//! | `world <landblock>` | with `--world`, load a world offline with a body at the landblock's middle |
//! | `position` | where the body stands: its cell and block-local origin |
//! | `quit` | stop and shut the client down |
//!
//! An `<action>` is a retail action name (`MovementForward`, `MovementJump`, `CameraRotateLeft`) or
//! an id in decimal or `0x` hex; each edge goes into the runtime's action queue and reaches the
//! same handlers a key's action does.
//!
//! By default it loads no world, so `snapshot` reports no coordinates. With `--world` it runs the
//! runtime's device-free world presentation instead of the null one: entering the world over a
//! recording (or `world <landblock>`) builds the world and a local body, and movement actions walk
//! it. Either way it has no UI, because the
//! pre-game screens and the HUD belong to the windowed client; and it captures no images, because
//! there is no device to read one from.

pub mod capture;
pub mod run;
pub mod script;

pub use run::{run, Options, RunError, Summary};
pub use script::{parse, Command, Dump, ScriptError};
