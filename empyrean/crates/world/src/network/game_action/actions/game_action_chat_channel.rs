// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChatChannel.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChatChannel.cs`.
//!
//! The legacy group channels: the staff channels by access level, Help, the fellowship and the
//! allegiance channels. Fellowship and allegiance members are reached through the helpers at the
//! end of this file, over 5.11's and 5.10's ports.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    AccessLevel, AllegiancePermissionLevel, Channel, ChatMessageType, WeenieError,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::managers::player_manager;
use crate::network::chat_packet;
use crate::network::game_event::events::game_event_channel_broadcast::game_event_channel_broadcast;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::managers::squelch_manager;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// `session.Network.EnqueueSend(new GameEventWeenieError(session, error))`.
fn send_weenie_error(w: &mut World, session: SessionId, error: WeenieError) {
    let status_message = game_event_weenie_error(session_data(w, session), error);
    enqueue_send(w, session, status_message);
}

/// `x.Session.Network.EnqueueSend(new GameEventChannelBroadcast(x.Session, channel, senderName, message))`.
fn send_channel_broadcast(
    w: &mut World,
    to: SessionId,
    channel: Channel,
    sender_name: &str,
    message: &str,
) {
    let msg = game_event_channel_broadcast(session_data(w, to), channel, sender_name, message);
    enqueue_send(w, to, msg);
}

/// `player.Session` for an online player.
fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    shims::player_session(w, player).expect("ACE: player.Session is null (NullReferenceException)")
}

// ACE: GameActionChatChannel.Handle
// Not ACE's (a fix, V264): on the Fellow channel the sender gets its own line once; ACE echoed it once for every member that was the sender or squelched the sender.
// Not ACE's (a fix, V265): a monarch on the Patron or Covassals channel gets its echo and the line reaches nobody; ACE threw on the missing patron and dropped the line.
#[allow(clippy::too_many_lines)]
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode_padded::<proto::comms::CommunicationChannelBroadcast>()?;
    let group_chat_type = Channel(m.channel.cs_cast());
    let message = m.message;

    let player = session_player(w, session);
    let access_level = w.sessions.get(session).expect("the session").access_level;
    let player_name = crate::dispatch::name::name(w, player).unwrap_or_default();

    match group_chat_type {
        Channel::Abuse => {
            // Should anyone be able to post to this channel? If so should they just a response back stating their message has been posted.
            // Should messages here also be written to a log?
            if access_level < AccessLevel::Advocate {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Admin => {
            if !w.objects.get(player).is_some_and(|o| o.is_admin_prop()) {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Audit => {
            if access_level < AccessLevel::Sentinel {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Advocate1 | Channel::Advocate2 | Channel::Advocate3 => {
            if access_level < AccessLevel::Advocate {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Sentinel => {
            if access_level < AccessLevel::Sentinel {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Help => {
            chat_packet::send_server_message(
                w,
                Some(session),
                "GameActionChatChannel TellHelp Needs work.",
                ChatMessageType::Broadcast,
            );

            player_manager::broadcast_to_channel(w, group_chat_type, player, &message, true, false);
        }
        Channel::Fellow => {
            let Some(fellowship_members) = player_fellowship_members(w, player) else {
                send_weenie_error(w, session, WeenieError::YouDoNotBelongToAFellowship);
                return Ok(());
            };

            // the sender's own echo goes out once, however many fellows squelch the sender
            let mut echoed = false;
            for fellowmember in fellowship_members {
                let fellow_session = session_of(w, fellowmember);
                if fellow_session != session
                    && !squelch_manager::squelches_contains(
                        w,
                        fellowmember,
                        Some(player),
                        ChatMessageType::Fellowship,
                    )
                {
                    send_channel_broadcast(
                        w,
                        fellow_session,
                        group_chat_type,
                        &player_name,
                        &message,
                    );
                } else if !echoed {
                    echoed = true;
                    send_channel_broadcast(w, session, group_chat_type, "", &message);
                }
            }
        }
        Channel::Vassals => {
            if !player_has_allegiance(w, player) {
                send_weenie_error(w, session, WeenieError::YouAreNotInAllegiance);
                return Ok(());
            }

            if allegiance_node_total_vassals(w, player) == 0 {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            for vassal_guid in allegiance_node_vassals(w, player) {
                let vassal_player = player_manager::get_online_player(w, vassal_guid);

                if let Some(vassal_player) = vassal_player {
                    if !squelch_manager::squelches_contains(
                        w,
                        vassal_player,
                        Some(player),
                        ChatMessageType::Allegiance,
                    ) {
                        let to = session_of(w, vassal_player);
                        send_channel_broadcast(w, to, group_chat_type, &player_name, &message);
                    }
                }
            }
            send_channel_broadcast(w, session, group_chat_type, "", &message);
        }
        Channel::Patron => {
            if !player_has_allegiance(w, player) {
                send_weenie_error(w, session, WeenieError::YouAreNotInAllegiance);
                return Ok(());
            }

            if w.objects.get(player).and_then(|o| o.patron_id()).is_none() {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            let patron_player = allegiance_node_patron_player_guid(w, player)
                .and_then(|g| player_manager::get_online_player(w, g));

            if let Some(patron_player) = patron_player {
                if !squelch_manager::squelches_contains(
                    w,
                    patron_player,
                    Some(player),
                    ChatMessageType::Allegiance,
                ) {
                    let to = session_of(w, patron_player);
                    send_channel_broadcast(w, to, group_chat_type, &player_name, &message);
                }
            }

            send_channel_broadcast(w, session, group_chat_type, "", &message);
        }
        Channel::Monarch => {
            if !player_has_allegiance(w, player) {
                send_weenie_error(w, session, WeenieError::YouAreNotInAllegiance);
                return Ok(());
            }

            if w.objects.get(player).and_then(|o| o.monarch_id()).is_none() {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            let monarch_player = allegiance_node_monarch_player_guid(w, player)
                .and_then(|g| player_manager::get_online_player(w, g));

            if let Some(monarch_player) = monarch_player {
                if !squelch_manager::squelches_contains(
                    w,
                    monarch_player,
                    Some(player),
                    ChatMessageType::Allegiance,
                ) {
                    let to = session_of(w, monarch_player);
                    send_channel_broadcast(w, to, Channel::Monarch, &player_name, &message);
                }
            }

            send_channel_broadcast(w, session, group_chat_type, "", &message);
        }
        Channel::CoVassals => {
            if !player_has_allegiance(w, player) {
                send_weenie_error(w, session, WeenieError::YouAreNotInAllegiance);
                return Ok(());
            }

            if w.objects.get(player).and_then(|o| o.patron_id()).is_none() {
                send_weenie_error(w, session, WeenieError::YouCantUseThatChannel);
                return Ok(());
            }

            let patron_player = allegiance_node_patron_player_guid(w, player)
                .and_then(|g| player_manager::get_online_player(w, g));

            if let Some(patron_player) = patron_player {
                if !squelch_manager::squelches_contains(
                    w,
                    patron_player,
                    Some(player),
                    ChatMessageType::Allegiance,
                ) {
                    let to = session_of(w, patron_player);
                    send_channel_broadcast(w, to, Channel::Patron, &player_name, &message);
                }
            }

            // a monarch has no patron and so no co-vassals: only its own echo
            let Some(covassals) = allegiance_node_patron_vassals(w, player) else {
                send_channel_broadcast(w, session, group_chat_type, "", &message);
                return Ok(());
            };
            for covassal_guid in covassals {
                if covassal_guid == player.full() {
                    send_channel_broadcast(w, session, group_chat_type, "", &message);
                } else {
                    let covassal_player = player_manager::get_online_player(w, covassal_guid);

                    if let Some(covassal_player) = covassal_player {
                        if !squelch_manager::squelches_contains(
                            w,
                            covassal_player,
                            Some(player),
                            ChatMessageType::Allegiance,
                        ) {
                            let to = session_of(w, covassal_player);
                            send_channel_broadcast(w, to, group_chat_type, &player_name, &message);
                        }
                    }
                }
            }
        }
        Channel::AllegianceBroadcast => {
            // The client knows if we're in an allegiance or not, and will throw an error to the user if they try to /ab, and no message will be dispatched to the server.
            // Check anyway
            let Some(members) = player_allegiance_members(w, player) else {
                send_weenie_error(w, session, WeenieError::YouAreNotInAllegiance);
                return Ok(());
            };

            if player_allegiance_permission_level(w, player) < AllegiancePermissionLevel::Speaker {
                send_weenie_error(w, session, WeenieError::YouDoNotHaveAuthorityInAllegiance);
                return Ok(());
            }

            // iterate through all allegiance members
            for member in members {
                // is this allegiance member online?
                let online = player_manager::get_online_player(w, member);
                let Some(online) = online else { continue };
                if squelch_manager::squelches_contains(
                    w,
                    online,
                    Some(player),
                    ChatMessageType::Allegiance,
                ) {
                    continue;
                }

                let to = session_of(w, online);
                send_channel_broadcast(w, to, group_chat_type, &player_name, &message);
            }
        }
        _ => {
            empyrean_common::console_write_line!(
                "Unhandled ChatChannel GroupChatType: 0x{:04X}",
                group_chat_type.0.cast_unsigned()
            );
        }
    }
    Ok(())
}

// ---- Player_Fellowship and Player_Allegiance members ---------------------------------------------

/// `session.Player.Fellowship?.GetFellowshipMembers().Values` (`Player_Fellowship.cs`,
/// `Fellowship.cs`; `GetFellowshipMembers` drops members who left the world).
fn player_fellowship_members(w: &mut World, player: ObjectGuid) -> Option<Vec<ObjectGuid>> {
    let fellowship = crate::world_objects::player_fellowship::fellowship(w, player)?;
    Some(
        crate::entity::fellowship::get_fellowship_members(w, &fellowship)
            .values()
            .copied()
            .collect(),
    )
}

/// `session.Player.HasAllegiance` (`Player_Allegiance.cs`).
fn player_has_allegiance(w: &World, player: ObjectGuid) -> bool {
    crate::world_objects::player_allegiance::has_allegiance(w, player)
}

/// `session.Player.AllegianceNode`: its tree and index (`NullReferenceException` when null).
fn allegiance_node(
    w: &World,
    player: ObjectGuid,
) -> (&crate::entity::allegiance_node::AllegianceTree, usize) {
    use crate::world_objects::{allegiance, player_allegiance};
    let node = player_allegiance::i_player_allegiance_node(
        w,
        crate::entity::i_player::IPlayer::Online(player),
    )
    .expect("ACE: Player.AllegianceNode is null (NullReferenceException)");
    let id = allegiance::member_node(w, node)
        .expect("ACE: Player.AllegianceNode is null (NullReferenceException)");
    (allegiance::tree(w, node.allegiance).expect("loaded"), id)
}

/// `session.Player.AllegianceNode.TotalVassals`.
fn allegiance_node_total_vassals(w: &World, player: ObjectGuid) -> u32 {
    let (tree, id) = allegiance_node(w, player);
    tree.total_vassals(id).cast_unsigned()
}

/// `session.Player.AllegianceNode.Vassals.Keys`.
fn allegiance_node_vassals(w: &World, player: ObjectGuid) -> Vec<u32> {
    let (tree, id) = allegiance_node(w, player);
    tree.vassals(id)
        .into_iter()
        .map(|v| tree.node(v).player_guid.full())
        .collect()
}

/// `session.Player.AllegianceNode.Patron.PlayerGuid`; `None` for a monarch, which has no patron
/// (ACE throws here; V265).
fn allegiance_node_patron_player_guid(w: &World, player: ObjectGuid) -> Option<u32> {
    let (tree, id) = allegiance_node(w, player);
    let patron = tree.node(id).patron?;
    Some(tree.node(patron).player_guid.full())
}

/// `session.Player.AllegianceNode.Monarch.PlayerGuid`.
fn allegiance_node_monarch_player_guid(w: &World, player: ObjectGuid) -> Option<u32> {
    let (tree, id) = allegiance_node(w, player);
    Some(tree.node(tree.node(id).monarch).player_guid.full())
}

/// `session.Player.AllegianceNode.Patron.Vassals.Keys`; `None` for a monarch, which has no
/// patron (ACE throws here; V265).
fn allegiance_node_patron_vassals(w: &World, player: ObjectGuid) -> Option<Vec<u32>> {
    let (tree, id) = allegiance_node(w, player);
    let patron = tree.node(id).patron?;
    Some(
        tree.vassals(patron)
            .into_iter()
            .map(|v| tree.node(v).player_guid.full())
            .collect(),
    )
}

/// `player.Allegiance?.Members.Keys` (null: not in an allegiance).
fn player_allegiance_members(w: &World, player: ObjectGuid) -> Option<Vec<u32>> {
    let allegiance = crate::world_objects::player_allegiance::i_player_allegiance(
        w,
        crate::entity::i_player::IPlayer::Online(player),
    )?;
    Some(
        crate::world_objects::allegiance::members(w, allegiance)
            .into_iter()
            .map(|(g, _)| g.full())
            .collect(),
    )
}

/// `player.AllegiancePermissionLevel` (`Player_Allegiance.cs`).
fn player_allegiance_permission_level(w: &World, player: ObjectGuid) -> AllegiancePermissionLevel {
    crate::world_objects::player_allegiance::allegiance_permission_level(w, player)
}
