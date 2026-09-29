// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionTalkDirect.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionTalkDirect.cs`.

use dereth_protocol as proto;
use empyrean_entity::enums::{ChatMessageType, WeenieError};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::landblock;
use crate::network::game_event::events::game_event_tell::game_event_tell_from;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::managers::{emote_manager, squelch_manager};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::world_objects::{player, player_properties};
use crate::World;

// ACE: GameActionTalkDirect.Handle
// Not ACE's (a fix, V266): a gagged sender telling a player gets the gag message and no "You tell ..." echo; ACE sent the echo before the gag check.
/// A tell to a creature in the player's landblock (or its adjacents): a player gets the tell, an
/// NPC hears it through its emotes.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationTalkDirect>()?;
    let (text, target_guid) = (m.message, m.target.0);

    let player = session_player(w, session);

    // `session.Player.CurrentLandblock?.GetObject(targetGuid) as Creature`
    let creature = w
        .objects
        .get(player)
        .and_then(|o| o.current_landblock)
        .and_then(|lb| landblock::get_object(w, lb, ObjectGuid::new(target_guid), true))
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature));
    let Some(creature) = creature else {
        let status_message =
            game_event_weenie_error(session_data(w, session), WeenieError::CharacterNotAvailable);
        enqueue_send(w, session, status_message);
        return Ok(());
    };

    let target_is_player = w.objects.get(creature).is_some_and(WorldObject::is_player);
    if target_is_player && w.objects.get(player).is_some_and(WorldObject::is_gagged) {
        player::send_gag_error(w, player);
        return Ok(());
    }

    let creature_name = crate::dispatch::name::name(w, creature).unwrap_or_default();
    enqueue_send(
        w,
        session,
        game_message_system_chat(
            &format!("You tell {creature_name}, \"{text}\""),
            ChatMessageType::OutgoingTell,
        ),
    );

    if target_is_player {
        let target_player = creature;
        // Not ACE's (retail, V284; the retail captures): a squelched sender has already had
        // the normal echo and gets no "<name> has you squelched." notice; the tell is not
        // delivered. Retail squelch was private, so nothing tells the sender they are squelched.
        if squelch_manager::squelches_contains(
            w,
            target_player,
            Some(player),
            ChatMessageType::Tell,
        ) {
            //log.Warn($"Tell from {session.Player.Name} (0x{session.Player.Guid.ToString()}) to {targetPlayer.Name} (0x{targetPlayer.Guid.ToString()}) blocked due to squelch");
            return Ok(());
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
    } else {
        emote_manager::on_talk_direct(w, creature, player, &text);
    }
    Ok(())
}
