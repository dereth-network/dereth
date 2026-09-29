// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/CommandHandlerInfo.cs
//! Port of `Source/ACE.Server/Command/CommandHandlerInfo.cs`.

use crate::command_handler::CommandHandler;
use crate::command_handler_attribute::CommandHandlerAttribute;

// ACE: CommandHandlerInfo
/// A registered command: its handler and its attribute.
#[derive(Debug, Clone)]
pub struct CommandHandlerInfo {
    // ACE: CommandHandlerInfo.Handler
    pub handler: CommandHandler,
    /// Not ACE: the handler's path (`crate::module::function`), which identifies it. Two
    /// functions need not have different addresses (identical bodies may be folded into one), so
    /// the handler itself cannot say which function a row registered.
    pub handler_name: &'static str,
    // ACE: CommandHandlerInfo.Attribute
    pub attribute: CommandHandlerAttribute,
}

/// Not ACE: a handler with its name, as [`handler!`](crate::handler) makes it.
pub type NamedHandler = (CommandHandler, &'static str);

/// Not ACE: a [`NamedHandler`] for a function, named by its path so no row can disagree with its
/// own name. `handler!(handle_pop)` names `handle_pop` in the calling module;
/// `handler!(crate::handlers::player_commands::handle_pop)` names that path in the calling crate.
#[macro_export]
macro_rules! handler {
    ($name:ident) => {
        (
            $name as $crate::command_handler::CommandHandler,
            concat!(module_path!(), "::", stringify!($name)),
        )
    };
    (crate $(:: $segment:ident)+) => {
        (
            crate $(:: $segment)+ as $crate::command_handler::CommandHandler,
            concat!(env!("CARGO_CRATE_NAME") $(, "::", stringify!($segment))+),
        )
    };
}
