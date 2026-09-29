// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/TurbineChatHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/TurbineChatHandler.cs`.
//!
//! The global chat rooms (General, Trade, LFG, Roleplay, Society, Olthoi and Allegiance): the
//! client's `NETBLOB_REQUEST_BINARY` blob is read, the room and chat type are normalised, the
//! message goes to every online listener as a `NETBLOB_EVENT_BINARY` and the sender gets a
//! `NETBLOB_RESPONSE_BINARY` for its context. Allegiances are not ported: the allegiance room's
//! `AllegianceManager.GetAllegiance` is a pointer answering none (ACE then sends nothing).

use dereth_protocol::MessageError;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, ChatNetworkBlobDispatchType, ChatNetworkBlobType, ChatType,
    FactionBits,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::turbine_chat_channel;
use crate::managers::{player_manager, property_manager};
use crate::network::chat_packet;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::game_messages::messages::game_message_turbine_chat::game_message_turbine_chat;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::managers::squelch_manager;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::world_objects::{player, player_character};
use crate::World;

/// `PropertyManager.GetBool(key).Item` (ACE's default fallback, `false`).
fn get_bool(w: &World, key: &str) -> bool {
    property_manager::get_bool(w, key, false, true).item
}

/// `PropertyManager.GetLong(key).Item` (ACE's default fallback, `0`).
fn get_long(w: &World, key: &str) -> i64 {
    property_manager::get_long(w, key, 0, true).item
}

/// `Encoding.Unicode.GetString(bytes)`: UTF-16LE, an unpaired surrogate or a trailing odd byte
/// becoming U+FFFD.
fn unicode_get_string(bytes: &[u8]) -> String {
    let units = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b));
    let mut s: String = char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    if bytes.len() % 2 == 1 {
        s.push(char::REPLACEMENT_CHARACTER);
    }
    s
}

/// `new GameMessageTurbineChat(NETBLOB_RESPONSE_BINARY, ASYNCMETHOD_SENDTOROOMBYNAME, contextId,
/// null, null, 0, chatType)`: the sender's acknowledgement.
fn response(context_id: u32, chat_type: ChatType) -> GameMessage {
    game_message_turbine_chat(
        ChatNetworkBlobType::NETBLOB_RESPONSE_BINARY,
        ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYNAME,
        context_id,
        "",
        "",
        0,
        chat_type,
    )
}

fn object(w: &World, guid: ObjectGuid) -> &WorldObject {
    w.objects
        .get(guid)
        .expect("ACE: the player is null (NullReferenceException)")
}

/// `Player.IsOlthoiPlayer` (`Player_Properties.cs`), as `SetEphemeralValues` set it.
fn is_olthoi_player(w: &World, guid: ObjectGuid) -> bool {
    object(w, guid)
        .player
        .as_ref()
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// `Player.IsAdmin`.
fn is_admin(w: &World, guid: ObjectGuid) -> bool {
    object(w, guid).is_admin_prop()
}

/// `Creature.Society`: `Faction1Bits ?? FactionBits.None`.
fn society(w: &World, guid: ObjectGuid) -> FactionBits {
    object(w, guid).faction1_bits().unwrap_or(FactionBits::None)
}

/// `recipient.Session.Network.EnqueueSend(msg)`.
fn send_to(w: &mut World, recipient: ObjectGuid, msg: &GameMessage) {
    let session = shims::player_session(w, recipient)
        .expect("ACE: recipient.Session is null (NullReferenceException)");
    enqueue_send(w, session, msg.clone());
}

/// Not ACE's (retail, V302): the chat text's packed character count as the
/// client writes it. Below 0x80 it is one byte; with the high bit set and the next bit clear, two
/// bytes, `((b0 & 0x7F) << 8) | b1` (up to 0x3FFF); with both top bits set, `((b0 & 0x3F) << 8 |
/// b1) << 16` plus a little-endian `u16` after them (from 0x4000).
pub fn read_packed_length(message: &mut Payload<'_>) -> Result<usize, MessageError> {
    let b0 = u32::from(message.read_byte()?);
    let length = if b0 & 0x80 == 0 {
        b0
    } else {
        let b1 = u32::from(message.read_byte()?);
        if b0 & 0x40 == 0 {
            ((b0 & 0x7F) << 8) | b1
        } else {
            let tail = u32::from(message.read_u16()?);
            ((((b0 & 0x3F) << 8) | b1) << 16) | tail
        }
    };
    Ok(usize::try_from(length).expect("a 30-bit count fits"))
}

// ACE: TurbineChatHandler.TurbineChatReceived
/// Not ACE's (a fix, V319): the listen filters and the `chat_disable_*` checks
/// of the rooms open to all compare the adjusted room, the one the message goes to. ACE compared
/// the raw `channelID`, so a `SENDTOROOMBYNAME` request (the client only sends by id) whose raw
/// room id was not the room its chat type names skipped the listeners' General/Trade/LFG/Roleplay
/// options and the disable switches.
///
/// Not ACE's (retail, V302): the message's character count is read in the
/// client's packed form ([`read_packed_length`]), so a count of 0x4000 or more reads right.
#[allow(clippy::too_many_lines)]
pub fn turbine_chat_received(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    if !get_bool(w, "use_turbine_chat") {
        return Ok(());
    }

    message.read_u32()?; // Bytes to follow
    let chat_blob_type = ChatNetworkBlobType(message.read_u32()?.cs_cast());
    let chat_blob_dispatch_type = ChatNetworkBlobDispatchType(message.read_u32()?.cs_cast());
    message.read_u32()?; // Always 1
    message.read_u32()?; // Always 0
    message.read_u32()?; // Always 0
    message.read_u32()?; // Always 0
    message.read_u32()?; // Always 0
    message.read_u32()?; // Bytes to follow

    let player = session_player(w, session);

    if object(w, player).is_gagged() {
        player::send_gag_error(w, player);
        return Ok(());
    }

    if chat_blob_type == ChatNetworkBlobType::NETBLOB_REQUEST_BINARY {
        let context_id = message.read_u32()?; // 0x01 - 0x71 (maybe higher), typically though 0x01 - 0x0F
        message.read_u32()?; // Always 2
        message.read_u32()?; // Always 2
        let channel_id = message.read_u32()?;

        let message_len = read_packed_length(message)?;
        let message_bytes = message.read_bytes(message_len.saturating_mul(2));
        let text = unicode_get_string(&message_bytes);

        message.read_u32()?; // Always 0x0C
        let sender_id = message.read_u32()?;
        message.read_u32()?; // Always 0
        let chat_type = ChatType(message.read_u32()?.cs_cast());

        let mut adjusted_channel_id = channel_id;
        let mut adjusted_chat_type = chat_type;

        if chat_blob_dispatch_type == ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYNAME {
            adjusted_channel_id = match chat_type {
                ChatType::Allegiance => turbine_chat_channel::ALLEGIANCE,
                ChatType::General => turbine_chat_channel::GENERAL,
                ChatType::Trade => turbine_chat_channel::TRADE,
                ChatType::LFG => turbine_chat_channel::LFG,
                ChatType::Roleplay => turbine_chat_channel::ROLEPLAY,
                ChatType::Society
                | ChatType::SocietyCelHan
                | ChatType::SocietyEldWeb
                | ChatType::SocietyRadBlo => turbine_chat_channel::SOCIETY,
                ChatType::Olthoi => turbine_chat_channel::OLTHOI,
                _ => turbine_chat_channel::GENERAL,
            };

            adjusted_chat_type = ChatType(adjusted_channel_id.cs_cast());
        } else if chat_blob_dispatch_type == ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYID
        {
            if channel_id > turbine_chat_channel::OLTHOI
                || channel_id == turbine_chat_channel::ALLEGIANCE
            {
                // Channel must be an allegiance channel
                adjusted_chat_type = ChatType::Allegiance;
            } else if channel_id == turbine_chat_channel::OLTHOI {
                // Channel must be the Olthoi play channel
                adjusted_chat_type = ChatType::Olthoi;
            } else if channel_id >= turbine_chat_channel::SOCIETY {
                // Channel must be a society restricted channel
                adjusted_chat_type = ChatType::Society;
            } else {
                // Channel must be one of the channels available to all players
                if channel_id == turbine_chat_channel::GENERAL {
                    adjusted_chat_type = ChatType::General;
                } else if channel_id == turbine_chat_channel::TRADE {
                    adjusted_chat_type = ChatType::Trade;
                } else if channel_id == turbine_chat_channel::LFG {
                    adjusted_chat_type = ChatType::LFG;
                } else if channel_id == turbine_chat_channel::ROLEPLAY {
                    adjusted_chat_type = ChatType::Roleplay;
                }
            }
        }

        if channel_id != adjusted_channel_id {
            log::debug!("[CHAT] ChannelID ({channel_id}) was adjusted to {adjusted_channel_id} | ChatNetworkBlobDispatchType: {chat_blob_dispatch_type}");
        }

        if chat_type != adjusted_chat_type {
            log::debug!("[CHAT] ChatType ({chat_type}) was adjusted to {adjusted_chat_type} | ChatNetworkBlobDispatchType: {chat_blob_dispatch_type}");
        }

        let player_name = crate::dispatch::name::name(w, player).unwrap_or_default();
        let game_message_turbine_chat = game_message_turbine_chat(
            ChatNetworkBlobType::NETBLOB_EVENT_BINARY,
            ChatNetworkBlobDispatchType::ASYNCMETHOD_SENDTOROOMBYNAME,
            adjusted_channel_id,
            &player_name,
            &text,
            sender_id,
            adjusted_chat_type,
        );

        if adjusted_channel_id > turbine_chat_channel::OLTHOI
            || adjusted_channel_id == turbine_chat_channel::ALLEGIANCE
        {
            // Channel must be an allegiance channel
            //var allegiance = AllegianceManager.FindAllegiance(channelID);
            if let Some(allegiance) = allegiance_manager_get_allegiance(w, player) {
                // is sender booted / gagged?
                if !allegiance.is_member(player) {
                    return Ok(());
                }
                if allegiance.is_filtered(player) {
                    return Ok(());
                }

                // iterate through all allegiance members
                for member in allegiance.members.clone() {
                    // is this allegiance member online?
                    let Some(online) = player_manager::get_online_player(w, member.full()) else {
                        continue;
                    };

                    // is this member booted / gagged?
                    if allegiance.is_filtered(member)
                        || squelch_manager::squelches_contains(
                            w,
                            online,
                            Some(player),
                            ChatMessageType::Allegiance,
                        )
                    {
                        continue;
                    }

                    // does this player have allegiance chat filtered?
                    if !player_character::get_character_option(
                        w,
                        online,
                        CharacterOption::ListenToAllegianceChat,
                    ) {
                        continue;
                    }

                    send_to(w, online, &game_message_turbine_chat);
                }

                enqueue_send(w, session, response(context_id, adjusted_chat_type));
            }
        } else if adjusted_channel_id == turbine_chat_channel::OLTHOI {
            // Channel must be the Olthoi play channel
            if !is_olthoi_player(w, player) {
                return Ok(());
            }

            if get_bool(w, "chat_disable_olthoi") {
                handle_chat_reject(
                    w,
                    session,
                    context_id,
                    chat_type,
                    &game_message_turbine_chat,
                    "",
                );
                return Ok(());
            }

            if get_bool(w, "chat_echo_only") {
                enqueue_send(w, session, game_message_turbine_chat);
                enqueue_send(w, session, response(context_id, adjusted_chat_type));
                return Ok(());
            }

            //if (PropertyManager.GetBool("chat_requires_account_15days").Item && !session.Player.Account15Days)
            //{
            //    HandleChatReject(session, contextId, chatType, gameMessageTurbineChat, "because this account is not 15 days old");
            //    return;
            //}

            //var chat_requires_account_time_seconds = PropertyManager.GetLong("chat_requires_account_time_seconds").Item;
            //if (chat_requires_account_time_seconds > 0 && (DateTime.UtcNow - session.Player.Account.CreateTime).TotalSeconds < chat_requires_account_time_seconds)
            //{
            //    HandleChatReject(session, contextId, chatType, gameMessageTurbineChat, "because this account is not old enough");
            //    return;
            //}

            //var chat_requires_player_age = PropertyManager.GetLong("chat_requires_player_age").Item;
            //if (chat_requires_player_age > 0 && session.Player.Age < chat_requires_player_age)
            //{
            //    HandleChatReject(session, contextId, chatType, gameMessageTurbineChat, "because this character has not been played enough");
            //    return;
            //}

            //var chat_requires_player_level = PropertyManager.GetLong("chat_requires_player_level").Item;
            //if (chat_requires_player_level > 0 && session.Player.Level < chat_requires_player_level)
            //{
            //    HandleChatReject(session, contextId, chatType, gameMessageTurbineChat, $"because this character has reached level {chat_requires_player_level}");
            //    return;
            //}

            for recipient in player_manager::get_all_online(w) {
                // handle filters
                if !is_olthoi_player(w, recipient) && !is_admin(w, recipient) {
                    continue;
                }

                if get_bool(w, "chat_disable_olthoi") {
                    if get_bool(w, "chat_echo_reject") {
                        enqueue_send(w, session, game_message_turbine_chat.clone());
                    }

                    enqueue_send(w, session, response(context_id, adjusted_chat_type));
                    return Ok(());
                }

                if squelch_manager::squelches_contains(
                    w,
                    recipient,
                    Some(player),
                    ChatMessageType::AllChannels,
                ) {
                    continue;
                }

                send_to(w, recipient, &game_message_turbine_chat);
            }

            enqueue_send(w, session, response(context_id, adjusted_chat_type));
        } else if adjusted_channel_id >= turbine_chat_channel::SOCIETY {
            // Channel must be a society restricted channel
            let sender_society = society(w, player);

            //var adjustedChatType = senderSociety switch
            //{
            //    FactionBits.CelestialHand => ChatType.SocietyCelHan,
            //    FactionBits.EldrytchWeb => ChatType.SocietyEldWeb,
            //    FactionBits.RadiantBlood => ChatType.SocietyRadBlo,
            //    _ => ChatType.Society
            //};

            //gameMessageTurbineChat = new GameMessageTurbineChat(ChatNetworkBlobType.NETBLOB_EVENT_BINARY, channelID, session.Player.Name, message, senderID, adjustedChatType);

            if sender_society == FactionBits::None {
                chat_packet::send_server_message(
                    w,
                    Some(session),
                    "You do not belong to a society.",
                    ChatMessageType::Broadcast,
                ); // I don't know if this is how it was done on the live servers
                return Ok(());
            }

            for recipient in player_manager::get_all_online(w) {
                // handle filters
                if sender_society != society(w, recipient) && !is_admin(w, recipient) {
                    continue;
                }

                if !player_character::get_character_option(
                    w,
                    recipient,
                    CharacterOption::ListenToSocietyChat,
                ) {
                    continue;
                }

                if squelch_manager::squelches_contains(
                    w,
                    recipient,
                    Some(player),
                    ChatMessageType::AllChannels,
                ) {
                    continue;
                }

                send_to(w, recipient, &game_message_turbine_chat);
            }

            enqueue_send(w, session, response(context_id, adjusted_chat_type));
        } else {
            // Channel must be one of the channels available to all players
            if is_olthoi_player(w, player) {
                //HandleChatReject(session, contextId, chatType, gameMessageTurbineChat, "because this account is not 15 days old");
                return Ok(());
            }

            if get_bool(w, "chat_echo_only") {
                enqueue_send(w, session, game_message_turbine_chat);
                enqueue_send(w, session, response(context_id, adjusted_chat_type));
                return Ok(());
            }

            if get_bool(w, "chat_requires_account_15days") && !object(w, player).account15_days() {
                handle_chat_reject(
                    w,
                    session,
                    context_id,
                    chat_type,
                    &game_message_turbine_chat,
                    "because this account is not 15 days old",
                );
                return Ok(());
            }

            let chat_requires_account_time_seconds =
                get_long(w, "chat_requires_account_time_seconds");
            if chat_requires_account_time_seconds > 0 {
                let create_time = object(w, player)
                    .player
                    .as_ref()
                    .and_then(|p| p.player.account.as_ref())
                    .expect("ACE: session.Player.Account is null (NullReferenceException)")
                    .create_time;
                #[allow(clippy::cast_precision_loss)]
                // C#'s `double < long` compare widens the long
                if (w.now.utc - create_time).total_seconds()
                    < chat_requires_account_time_seconds as f64
                {
                    handle_chat_reject(
                        w,
                        session,
                        context_id,
                        chat_type,
                        &game_message_turbine_chat,
                        "because this account is not old enough",
                    );
                    return Ok(());
                }
            }

            // `session.Player.Age < chat_requires_player_age`: a lifted `int?` compare, false for null.
            let chat_requires_player_age = get_long(w, "chat_requires_player_age");
            if chat_requires_player_age > 0
                && object(w, player)
                    .age()
                    .is_some_and(|age| i64::from(age) < chat_requires_player_age)
            {
                handle_chat_reject(
                    w,
                    session,
                    context_id,
                    chat_type,
                    &game_message_turbine_chat,
                    "because this character has not been played enough",
                );
                return Ok(());
            }

            let chat_requires_player_level = get_long(w, "chat_requires_player_level");
            if chat_requires_player_level > 0
                && object(w, player)
                    .level()
                    .is_some_and(|level| i64::from(level) < chat_requires_player_level)
            {
                handle_chat_reject(
                    w,
                    session,
                    context_id,
                    chat_type,
                    &game_message_turbine_chat,
                    &format!(
                        "because this character has not reached level {chat_requires_player_level}"
                    ),
                );
                return Ok(());
            }

            for recipient in player_manager::get_all_online(w) {
                // handle filters
                let room = adjusted_channel_id;
                if room == turbine_chat_channel::GENERAL
                    && !player_character::get_character_option(
                        w,
                        recipient,
                        CharacterOption::ListenToGeneralChat,
                    )
                    || room == turbine_chat_channel::TRADE
                        && !player_character::get_character_option(
                            w,
                            recipient,
                            CharacterOption::ListenToTradeChat,
                        )
                    || room == turbine_chat_channel::LFG
                        && !player_character::get_character_option(
                            w,
                            recipient,
                            CharacterOption::ListenToLFGChat,
                        )
                    || room == turbine_chat_channel::ROLEPLAY
                        && !player_character::get_character_option(
                            w,
                            recipient,
                            CharacterOption::ListenToRoleplayChat,
                        )
                {
                    continue;
                }

                if (room == turbine_chat_channel::GENERAL && get_bool(w, "chat_disable_general"))
                    || (room == turbine_chat_channel::TRADE && get_bool(w, "chat_disable_trade"))
                    || (room == turbine_chat_channel::LFG && get_bool(w, "chat_disable_lfg"))
                    || (room == turbine_chat_channel::ROLEPLAY
                        && get_bool(w, "chat_disable_roleplay"))
                {
                    if get_bool(w, "chat_echo_reject") {
                        enqueue_send(w, session, game_message_turbine_chat.clone());
                    }

                    enqueue_send(w, session, response(context_id, adjusted_chat_type));
                    return Ok(());
                }

                if is_olthoi_player(w, recipient) {
                    continue;
                }

                if squelch_manager::squelches_contains(
                    w,
                    recipient,
                    Some(player),
                    ChatMessageType::AllChannels,
                ) {
                    continue;
                }

                send_to(w, recipient, &game_message_turbine_chat);
            }

            enqueue_send(w, session, response(context_id, adjusted_chat_type));
        }

        log_turbine_chat(
            w,
            adjusted_channel_id,
            &player_name,
            &text,
            sender_id,
            adjusted_chat_type,
        );
    } else {
        empyrean_common::console_write_line!(
            "Unhandled TurbineChatHandler ChatNetworkBlobType: 0x{:04X}",
            chat_blob_type.0.cast_unsigned()
        );
    }
    Ok(())
}

// ACE: TurbineChatHandler.HandleChatReject
/// The sender's echo (with `chat_echo_reject`), the reason (with `chat_inform_reject`) and the
/// response, for the client's own `chat_type`.
fn handle_chat_reject(
    w: &mut World,
    session: SessionId,
    context_id: u32,
    chat_type: ChatType,
    game_message_turbine_chat: &GameMessage,
    reject_reason: &str,
) {
    if get_bool(w, "chat_echo_reject") {
        enqueue_send(w, session, game_message_turbine_chat.clone());
    }

    if get_bool(w, "chat_inform_reject") {
        let reason = if reject_reason.is_empty() {
            String::new()
        } else {
            format!(" for you {reject_reason}")
        };
        let text = format!("{chat_type} is currently disabled{reason}.");
        let transient = game_event_communication_transient_string(session_data(w, session), &text);
        enqueue_send(w, session, transient);
        enqueue_send(
            w,
            session,
            game_message_system_chat(&text, ChatMessageType::Broadcast),
        );
    }

    enqueue_send(w, session, response(context_id, chat_type));
}

// ACE: TurbineChatHandler.LogTurbineChat
fn log_turbine_chat(
    w: &World,
    channel_id: u32,
    name: &str,
    message: &str,
    _sender_id: u32,
    chat_type: ChatType,
) {
    let key = match chat_type {
        ChatType::Allegiance => "chat_log_allegiance",
        ChatType::General => "chat_log_general",
        ChatType::LFG => "chat_log_lfg",
        ChatType::Olthoi => "chat_log_olthoi",
        ChatType::Roleplay => "chat_log_roleplay",
        ChatType::Society
        | ChatType::SocietyCelHan
        | ChatType::SocietyEldWeb
        | ChatType::SocietyRadBlo => "chat_log_society",
        ChatType::Trade => "chat_log_trade",
        _ => return,
    };
    if !get_bool(w, key) {
        return;
    }

    let room = if chat_type == ChatType::Allegiance {
        format!("[{channel_id}]")
    } else {
        String::new()
    };
    log::info!("[CHAT][{chat_type}]{room} {name} says, \"{message}\"");
}

// ---- pointers: AllegianceManager and Allegiance (not ported) ----------------------------------

/// What the allegiance room reads of an `Allegiance`.
struct AllegianceView {
    /// `Allegiance.Members.Keys`, in dictionary order.
    members: Vec<ObjectGuid>,
    /// The booted and gagged members (`Allegiance.IsFiltered`).
    filtered: Vec<ObjectGuid>,
}

impl AllegianceView {
    /// `Allegiance.IsMember(guid)`.
    fn is_member(&self, guid: ObjectGuid) -> bool {
        self.members.contains(&guid)
    }

    /// `Allegiance.IsFiltered(guid)`.
    fn is_filtered(&self, guid: ObjectGuid) -> bool {
        self.filtered.contains(&guid)
    }
}

/// `AllegianceManager.GetAllegiance(player)`, read as the room needs it. `IsFiltered` is taken for
/// every member once here: it drops an expired filter, which nothing else observes.
fn allegiance_manager_get_allegiance(w: &mut World, player: ObjectGuid) -> Option<AllegianceView> {
    use crate::world_objects::allegiance;
    let allegiance = crate::managers::allegiance_manager::get_allegiance(
        w,
        Some(crate::entity::i_player::IPlayer::Online(player)),
    )?;
    let members: Vec<ObjectGuid> = allegiance::members(w, allegiance)
        .into_iter()
        .map(|(g, _)| g)
        .collect();
    let mut filtered = Vec::new();
    for &member in members.iter().chain(std::iter::once(&player)) {
        if allegiance::is_filtered(w, allegiance, member) {
            filtered.push(member);
        }
    }
    Some(AllegianceView { members, filtered })
}
