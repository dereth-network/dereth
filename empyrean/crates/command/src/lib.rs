//! The port of `Source/ACE.Server/Command`: the console and in-game admin commands, with Empyrean's
//! own commands kept apart.
//!
//! **Depends on** the server crates it commands (`empyrean-common`, `empyrean-entity`,
//! `empyrean-tables`, `empyrean-content`, `empyrean-store`, `empyrean-dat`, `empyrean-net`,
//! `empyrean-world`) and the shared `dereth-primitives` and `dereth-physics`. **Used by** the
//! server (`empyrean-server`) and, in tests, the test kit (`empyrean-testkit`).
//!
//! **Must never** mix Empyrean's own commands into the ported handler files: [`handlers`] is the
//! port of ACE's handler files, one per ACE file, and [`empyrean`] holds every command that is
//! ours, so an upstream ACE change touches only the ported files.

// @scaffold-mods begin
pub mod command_handler;
pub mod command_handler_attribute;
pub mod command_handler_flag;
pub mod command_handler_info;
pub mod command_handler_response;
pub mod command_manager;
pub mod command_parameter_helpers;
pub mod handlers;
// @scaffold-mods end
pub mod console;
pub mod empyrean;
