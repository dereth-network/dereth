//! `reportbug`: where a player reports a bug. Not ACE.
//!
//! ACE's `reportbug` (ported as [`crate::handlers::player_commands::handle_reportbug`]) builds a
//! URL for the ACE community's tracker that carries the world name, the character, the target, the
//! location and the description. Ours points the player at the project's issue tracker and sends
//! nothing: no character data leaves the server in a URL. The operator's `reportbug_enabled`
//! switch still turns the command on and off (off by default, as in ACE).

use empyrean_common::brand;
use empyrean_entity::enums::{AccessLevel, ChatMessageType};
use empyrean_net::SessionId;
use empyrean_world::managers::property_manager;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;

/// The `reportbug` row. It has ACE's name, so it takes ACE's slot in the table.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle_reportbug);
    vec![CommandHandlerInfo {
        handler,
        handler_name,
        attribute: CommandHandlerAttribute::with_description(
            "reportbug",
            AccessLevel::Player,
            CommandHandlerFlag::RequiresWorld,
            "Shows where to report a bug",
            "",
        ),
    }]
}

/// The text `reportbug` sends: the issue tracker's address, framed as ACE frames its URL.
#[must_use]
pub fn report_bug_message() -> String {
    format!("\n\n\n\nBug Report - Open the following page in your browser to report a bug\n-=-\n{}\n-=-\n\n\n\n\n", brand::ISSUES_URL)
}

/// `reportbug`: tells the player where to report a bug. Any parameters are ignored.
///
/// # Panics
/// Without a session (the command requires the world, so the table never calls it without one).
pub fn handle_reportbug(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let session = session.expect("reportbug requires a session");
    if !property_manager::get_bool(w, "reportbug_enabled", false, true).item {
        let msg = game_message_system_chat(
            "The command \"reportbug\" is not currently enabled on this server.",
            ChatMessageType::Broadcast,
        );
        enqueue_send(w, session, msg);
        return;
    }
    enqueue_send(
        w,
        session,
        game_message_system_chat(&report_bug_message(), ChatMessageType::AdminTell),
    );
}
