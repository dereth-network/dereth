// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTalk.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTalk.cs`.

use std::sync::OnceLock;

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player;
use crate::World;

/// The `@command` branch of `GameActionTalk.Handle` (`CommandManager.ParseCommand`,
/// `GetCommandHandler`, the invoke and the error replies), given the whole message.
pub type CommandPath = fn(&mut World, SessionId, &str);

static COMMAND_PATH: OnceLock<CommandPath> = OnceLock::new();

/// Installs the `@command` branch. empyrean-command's `CommandManager.Initialize` calls this (unit
/// 6.1): empyrean-world cannot name empyrean-command, which depends on it. The first install wins.
pub fn set_command_path(path: CommandPath) {
    let _ = COMMAND_PATH.set(path);
}

// ACE: GameActionTalk.Handle
/// The `@command` branch is empyrean-command's (`command_manager::handle_talk_command`), reached
/// through [`set_command_path`]; plain speech is here.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let message = message
        .decode_padded::<proto::comms::CommunicationTalk>()?
        .message;

    if message.starts_with('@') {
        if let Some(path) = COMMAND_PATH.get() {
            path(w, session, &message);
        } else {
            // DIVERGE (arch): before CommandManager.Initialize has run (or in a host without
            // empyrean-command) the command is dropped and logged.
            log::warn!("GameActionTalk: no command path installed; dropped {message:?}");
        }
    } else {
        let player = session_player(w, session);
        player::handle_action_talk(w, player, &message);
    }
    Ok(())
}
