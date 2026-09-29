// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTell.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTell.cs`.

use dereth_protocol as proto;
use empyrean_entity::enums::{ChatMessageType, WeenieError, WeenieErrorWithString};
use empyrean_net::SessionId;

use crate::managers::player_manager;
use crate::network::game_event::events::game_event_tell::game_event_tell_from;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::events::game_event_weenie_error_with_string::game_event_weenie_error_with_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::managers::squelch_manager;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::world_objects::{player, player_networking, player_properties};
use crate::World;

// ACE: GameActionTell.Handle
/// A tell by name to an online player. An unknown or offline name answers `CharacterNotAvailable`;
/// a target who squelches the sender's tells silently drops it after the echo; an away target answers
/// its AFK message and still gets the tell.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode_padded::<proto::comms::CommunicationTalkDirectByName>()?;
    let text = m.message; // The client seems to do the trimming for us
    let target = m.target_name; // Needs to be trimmed because it may contain white spaces after the name and before the ,

    let player = session_player(w, session);

    if w.objects.get(player).is_some_and(WorldObject::is_gagged) {
        player::send_gag_error(w, player);
        return Ok(());
    }

    let target = target.trim();
    let target_player = player_manager::get_online_player_by_name(w, target);

    let Some(target_player) = target_player else {
        let status_message =
            game_event_weenie_error(session_data(w, session), WeenieError::CharacterNotAvailable);
        enqueue_send(w, session, status_message);
        return Ok(());
    };

    let target_name = crate::dispatch::name::name(w, target_player).unwrap_or_default();

    if player != target_player {
        enqueue_send(
            w,
            session,
            game_message_system_chat(
                &format!("You tell {target_name}, \"{text}\""),
                ChatMessageType::OutgoingTell,
            ),
        );
    }

    // Not ACE's (retail, V284; the retail captures): a squelched sender gets the echo and
    // no "<name> has you squelched." notice; the tell is not delivered. Squelch was private.
    if squelch_manager::squelches_contains(w, target_player, Some(player), ChatMessageType::Tell) {
        //log.Warn($"Tell from {session.Player.Name} (0x{session.Player.Guid.ToString()}) to {targetPlayer.Name} (0x{targetPlayer.Guid.ToString()}) blocked due to squelch");
        return Ok(());
    }

    let target_object = w
        .objects
        .get(target_player)
        .expect("ACE: targetPlayer is null");
    if target_object.is_afk() {
        let afk_message = target_object.afk_message();
        // `string.IsNullOrWhiteSpace(targetPlayer.AfkMessage) ? DefaultAFKMessage : targetPlayer.AfkMessage`
        let afk_message = match afk_message.as_deref() {
            Some(m) if !m.chars().all(char::is_whitespace) => m.to_owned(),
            _ => player_networking::DEFAULT_AFK_MESSAGE.to_owned(),
        };
        let msg = game_event_weenie_error_with_string(
            session_data(w, session),
            WeenieErrorWithString::AFK,
            &format!("{target_name} is away: {afk_message}"),
        );
        enqueue_send(w, session, msg);
        //return;
    }

    let target_session = shims::player_session(w, target_player)
        .expect("ACE: targetPlayer.Session is null (NullReferenceException)");
    let sender_name = player_properties::get_name_with_suffix(w, player);
    let tell = game_event_tell_from(
        session_data(w, target_session),
        &text,
        &sender_name,
        player.full(),
        target_player.full(),
        ChatMessageType::Tell,
    );
    enqueue_send(w, target_session, tell);
    Ok(())
}
