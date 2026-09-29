//! `emphelp`, `empcommands` and `empversion`: Empyrean's names for ACE's `acehelp`, `acecommands`
//! and `aceversion`. Not ACE.
//!
//! Their behaviour is ACE's, so each row forwards to the ported handler (in
//! [`crate::handlers::help_commands`] and [`crate::handlers::player_commands`]) with ACE's
//! access level, flags, parameter count, description and usage. ACE's names stay registered by
//! the ported files and keep working; their descriptions say which of ours they equal
//! ([`alias_description`]).

use empyrean_entity::enums::AccessLevel;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::handler;

/// Each of our names with ACE's name for the same command.
pub const RENAMED: [(&str, &str); 3] = [
    ("emphelp", "acehelp"),
    ("empcommands", "acecommands"),
    ("empversion", "aceversion"),
];

/// The description ACE's name is listed with: `Same as @<ours> (ACE's name).`
#[must_use]
pub fn alias_description(ours: &str) -> String {
    format!("Same as @{ours} (ACE's name).")
}

/// Our three rows, added after ACE's.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let rows: [(CommandHandlerAttribute, NamedHandler); 3] = [
        (
            CommandHandlerAttribute::with_count(
                "emphelp",
                AccessLevel::Player,
                CommandHandlerFlag::None,
                0,
                "Displays help.",
                "(command)",
            ),
            handler!(crate::handlers::help_commands::handle_ace_help),
        ),
        (
            CommandHandlerAttribute::with_count(
                "empcommands",
                AccessLevel::Player,
                CommandHandlerFlag::None,
                0,
                "Lists all commands.",
                "<access level or search>",
            ),
            handler!(crate::handlers::help_commands::handle_ace_commands),
        ),
        (
            CommandHandlerAttribute::with_description(
                "empversion",
                AccessLevel::Player,
                CommandHandlerFlag::RequiresWorld,
                "Shows this server's version data",
                "",
            ),
            handler!(crate::handlers::player_commands::handle_ac_eversion),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}
