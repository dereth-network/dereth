//! Empyrean's own commands. Not ACE.
//!
//! **Layout.** [`crate::handlers`] holds the port of ACE's handler files, one file per ACE file,
//! each registering ACE's commands under ACE's names. This module holds every command that is
//! Empyrean's rather than ACE's, so an upstream ACE change lands only in the ported files and
//! never has to be merged with ours:
//!
//! - [`help_commands`]: `emphelp`, `empcommands` and `empversion`, our names for ACE's `acehelp`,
//!   `acecommands` and `aceversion`. Their behaviour is ACE's, so each forwards to the ported
//!   handler; ACE's names stay registered by the ported files (listed as "Same as @emphelp
//!   (ACE's name)." and so on) and keep working.
//! - [`report_bug`]: `reportbug`, which replaces ACE's in the table (same name, same slot): ours
//!   points the player at the project's issue tracker instead of building a URL for ACE's.
//! - [`source`]: `source`, a new name: where the server's source code is and its licence (the AGPL
//!   source offer; ACE makes none).
//!
//! [`command_handlers`] is added to the table after every ported file, so a row here with an ACE
//! command's name replaces ACE's row in its slot, and a new name is added at the end.
//!
//! A future Empyrean-only command goes in a file here and in [`command_handlers`]; where its
//! behaviour is ACE's, it forwards to the ported handler rather than copying it.

pub mod help_commands;
pub mod report_bug;
pub mod source;

use crate::command_handler_info::CommandHandlerInfo;

/// Empyrean's command rows, added to the table after ACE's.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let mut all = help_commands::command_handlers();
    all.extend(report_bug::command_handlers());
    all.extend(source::command_handlers());
    all
}
