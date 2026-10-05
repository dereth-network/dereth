// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/CommandHandlerHelper.cs
//! Port of `Source/ACE.Server/Command/Handlers/CommandHandlerHelper.cs`.

use empyrean_entity::enums::ChatMessageType;
use empyrean_entity::ObjectGuid;
use empyrean_net::{SessionId, SessionState};
use empyrean_world::world_objects::player_inventory::{self, SearchLocations};
use empyrean_world::World;

use crate::command_manager::{console_log_debug, console_log_error, console_log_info};

/// `ChatPacket.SendServerMessage(session, message, chatMessageType)` (empyrean-world's `chat_packet`).
pub fn send_server_message(
    w: &mut World,
    session: Option<SessionId>,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    empyrean_world::network::chat_packet::send_server_message(
        w,
        session,
        message,
        chat_message_type,
    );
}

/// `session.State == SessionState.WorldConnected && session.Player != null`.
fn world_connected_with_player(w: &World, session: SessionId) -> bool {
    w.sessions
        .get(session)
        .is_some_and(|s| s.state == SessionState::WorldConnected && s.player.is_some())
}

// ACE: CommandHandlerHelper.WriteOutputInfo
/// This will determine where a command handler should output to, the console or a client session.
/// If the session is null, the output will be sent to the console. If the session is not null, and
/// the session.Player is in the world, it will be sent to the session. Messages sent to the
/// console will be sent using log.Info()
pub fn write_output_info(
    w: &mut World,
    session: Option<SessionId>,
    output: &str,
    chat_message_type: ChatMessageType,
) {
    if let Some(s) = session {
        if world_connected_with_player(w, s) {
            send_server_message(w, session, output, chat_message_type);
        }
    } else {
        console_log_info(output);
    }
}

// ACE: CommandHandlerHelper.WriteOutputDebug
/// As [`write_output_info`]; messages sent to the console will be sent using log.Debug()
pub fn write_output_debug(
    w: &mut World,
    session: Option<SessionId>,
    output: &str,
    chat_message_type: ChatMessageType,
) {
    if let Some(s) = session {
        if world_connected_with_player(w, s) {
            send_server_message(w, session, output, chat_message_type);
        }
    } else {
        console_log_debug(output);
    }
}

// ACE: CommandHandlerHelper.WriteOutputError
/// As [`write_output_info`]; messages sent to the console will be sent using log.Error()
pub fn write_output_error(
    w: &mut World,
    session: Option<SessionId>,
    output: &str,
    chat_message_type: ChatMessageType,
) {
    if let Some(s) = session {
        if world_connected_with_player(w, s) {
            send_server_message(w, session, output, chat_message_type);
        }
    } else {
        console_log_error(output);
    }
}

// ACE: CommandHandlerHelper.GetLastAppraisedObject
/// Returns the last appraised WorldObject.
///
/// # Panics
/// A session without a player (ACE's `NullReferenceException`).
pub fn get_last_appraised_object(w: &mut World, session: SessionId) -> Option<ObjectGuid> {
    let player = w
        .sessions
        .player(session)
        .expect("ACE: session.Player is null (NullReferenceException)");
    let target_id = w
        .objects
        .get(player)
        .and_then(|p| p.requested_appraisal_target());
    let Some(target_id) = target_id else {
        write_output_info(
            w,
            Some(session),
            "GetLastAppraisedObject() - no appraisal target",
            ChatMessageType::Broadcast,
        );
        return None;
    };

    let target = player_inventory::find_object(
        w,
        player,
        ObjectGuid::new(target_id),
        SearchLocations::Everywhere,
    )
    .result;
    if target.is_none() {
        write_output_info(
            w,
            Some(session),
            &format!(
                "GetLastAppraisedObject() - couldn't find {}",
                empyrean_common::dotnet::format(target_id, "X8")
            ),
            ChatMessageType::Broadcast,
        );
        return None;
    }
    target
}
