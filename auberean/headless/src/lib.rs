//! The client with no window: logs in to a server on loopback, enters the world and plays a script
//! of commands, on the SDK alone.
//!
//! - [`script`]: the script, one command a line, and its reading;
//! - [`act`]: the contract's actions in text, for the script's `act`;
//! - [`run`]: the script run on the SDK's driver, one frame at a time ([`run::Runner`]);
//! - [`builtin`]: the program's flags (`--walk`, `--say`, `--craft` and the rest) as a script;
//! - [`snapshot`]: the view as text;
//! - [`plugins`]: the plugins, their packages, stores and the lines that manage them.
//!
//! The program is `src/main.rs`: its options, a UDP port and the system's steady clock.
//!
//! **Depends on** the SDK alone (`auberean-client-sdk`). **Used by** the `auberean-headless`
//! program and its tests.
//!
//! **Must never** read a device or reach past the SDK: everything it does is an action of the
//! SDK's driver, and it learns what happens from the events, the scripts' notices and the view
//! (`cargo xtask seams`).

pub mod act;
pub mod builtin;
pub mod plugins;
pub mod run;
pub mod script;
pub mod snapshot;
