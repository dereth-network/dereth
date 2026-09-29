//! `source`: where this server's source code is, and its licence. Not ACE.
//!
//! The server is AGPL-3.0-only, which asks whoever runs it (modified) for players over a network to
//! offer them its source. The login welcome names the address; `@source` repeats it, with the licence,
//! on demand, for every player and at the console. The address is `server.source_url` when the
//! operator sets it, else the repository the build came from (`empyrean_common::brand::source_url`).

use empyrean_common::brand;
use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::write_output_info;

/// The `source` row (a new name, added after ACE's rows).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle_source);
    vec![CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_count(
            "source",
            AccessLevel::Player,
            CommandHandlerFlag::None,
            0,
            "Shows where this server's source code is, and its licence.",
            "",
        ),
    }]
}

/// The text `source` sends.
#[must_use]
pub fn source_message() -> String {
    brand::source_message(&brand::source_url())
}

/// `source`: tells the player (or the console) where the server's source is. Any parameters are
/// ignored.
pub fn handle_source(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    write_output_info(w, session, &source_message(), ChatMessageType::Broadcast);
}
