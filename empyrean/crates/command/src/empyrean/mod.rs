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
//! - [`dn`]: `dn`, `derethnetwork` and `dng`, new names: the Dereth Network's command, and its
//!   short form for a line in the Global room.
//! - [`reload_dat_overlay`]: `reload-dat-overlay`, a new name: the world's data overlay read again
//!   and landblocks built again from it, for a world builder's live loop.
//! - [`save_world`]: `save-world`, a new name: every player and what the world keeps saved now, as
//!   a clean shutdown saves them, with the world left running.
//! - [`playtest_character`]: `playtest-character`, a new name: a world builder's playtest
//!   characters, made from recipes, backed up, restored and exported.
//! - [`character_file`]: `import-character`, a new name: a character exported from another world
//!   brought into an account here, when the operator allows it.
//! - [`treasure`]: `import-treasure` and `roll-treasure`, new names: a world builder's loot tables
//!   imported into the running world, and one drawn without anything made in it.
//!
//! [`command_handlers`] is added to the table after every ported file, so a row here with an ACE
//! command's name replaces ACE's row in its slot, and a new name is added at the end.
//!
//! A future Empyrean-only command goes in a file here and in [`command_handlers`]; where its
//! behaviour is ACE's, it forwards to the ported handler rather than copying it.

pub mod character_file;
pub mod dn;
pub mod help_commands;
pub mod playtest_character;
pub mod reload_dat_overlay;
pub mod report_bug;
pub mod save_world;
pub mod source;
pub mod treasure;

use crate::command_handler_info::CommandHandlerInfo;

/// Empyrean's command rows, added to the table after ACE's.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let mut all = help_commands::command_handlers();
    all.extend(report_bug::command_handlers());
    all.extend(source::command_handlers());
    all.extend(dn::command_handlers());
    all.extend(reload_dat_overlay::command_handlers());
    all.extend(playtest_character::command_handlers());
    all.extend(save_world::command_handlers());
    all.extend(character_file::command_handlers());
    all.extend(treasure::command_handlers());
    all
}
