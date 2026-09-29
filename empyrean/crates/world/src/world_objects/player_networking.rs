// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Networking.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Networking.cs`.
//!
//! Entering the world (the login message sequence), the turbine chat channels, broadcasting the
//! player's movement, fog, PK status and the small send helpers. Calls into systems ported in
//! other files go through pointer functions, in ACE's order.

use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, CommandMask, EnvironChangeType, FactionBits, MotionCommand,
    MotionStance, PlayerKillerStatus, PropertyBool, PropertyInt, WeenieError,
    WeenieErrorWithString,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::turbine_chat_channel;
use crate::managers::property_manager;
use crate::network::game_event::events::{
    game_event_character_title, game_event_communication_transient_string,
    game_event_friends_list_update, game_event_player_description,
    game_event_send_client_contract_tracker_table, game_event_set_turbine_chat_channels,
    game_event_view_contents, game_event_weenie_error, game_event_weenie_error_with_string,
};
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{self, GameMessage};
use crate::network::game_messages::messages::{
    game_message_admin_environs, game_message_create_object, game_message_player_create,
    game_message_private_update_property_bool, game_message_public_update_property_int,
    game_message_system_chat, game_message_update_motion,
};
use crate::network::motion::move_to_state::MoveToState;
use crate::network::motion::movement_data::MovementData;
use crate::network::motion::raw_motion_state::RawMotionFlags;
use crate::network::sequence::sequence_type::SequenceType;
use crate::network::sequence::u_short_sequence::UShortSequence;
use crate::world_objects::world_object_networking::{enqueue_broadcast, shims};
use crate::World;

/// Non-property fields declared in `Player_Networking.cs`.
#[derive(Debug, Default)]
pub struct PlayerNetworkingFields {
    // ACE: Player.LastSoulEmote
    pub last_soul_emote: MotionCommand,
    // ACE: Player.LastSoulEmoteEndTime
    pub last_soul_emote_end_time: DotNetDateTime,
    // ACE: Player.currentFogColor
    pub current_fog_color: Option<EnvironChangeType>,
}

fn session_of(w: &World, this: ObjectGuid) -> SessionId {
    shims::player_session(w, this).expect("ACE: Player.Session is null (NullReferenceException)")
}

fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let session = session_of(w, this);
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.PlayerEnterWorld
/// The login: the timestamps and login count, the instance sequence, the account-age and PK
/// checks, `SendSelf` (the portal-space entrance), the chat channels, allegiance, house and
/// squelch updates, and the login audits.
#[allow(clippy::too_many_lines)]
pub fn player_enter_world(w: &mut World, this: ObjectGuid) {
    crate::managers::player_manager::switch_player_from_offline_to_online(w, this);
    let now_unix = w.now.unix_time;
    let allegiance_node_rank =
        crate::world_objects::player_allegiance::allegiance_node_rank(w, this);
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.wo.world_object.teleporting = true;

    // Save the LoginTimestamp
    let last_login_timestamp = now_unix;

    o.set_login_timestamp(Some(last_login_timestamp));
    o.set_last_teleport_start_timestamp(Some(last_login_timestamp));

    let character = o
        .player
        .as_mut()
        .and_then(|p| p.player.character.as_mut())
        .expect("ACE: Player.Character is null (NullReferenceException)");
    character.last_login_timestamp = last_login_timestamp;
    character.total_logins = character.total_logins.wrapping_add(1);
    let total_logins = character.total_logins;
    o.player
        .as_mut()
        .expect("a player")
        .player_database
        .character_changes_detected = true;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `(ushort)Character.TotalLogins`
    o.sequences.set_sequence(
        SequenceType::ObjectInstance,
        Box::new(UShortSequence::new(total_logins as u16, u16::MAX)),
    );

    if o.barber_active() {
        o.set_barber_active(false);
    }

    // `AllegianceNode != null ? (int)AllegianceNode.Rank : null`
    o.set_allegiance_rank(allegiance_node_rank.map(u32::cast_signed));

    if !o.account15_days() {
        let create_time = o
            .player
            .as_ref()
            .and_then(|p| p.player.account.as_ref())
            .expect("ACE: Player.Account is null (NullReferenceException)")
            .create_time;
        let account_age = w.now.utc - create_time;

        let o = w.objects.get_mut(this).expect("ACE: this is null");
        if account_age.total_days() >= 15.0 {
            o.set_account15_days(true);
        }

        crate::world_objects::player_house::manage_account15_days_house_purchase_timestamp(w, this);

        if !w
            .objects
            .get(this)
            .expect("ACE: this is null")
            .account15_days()
            && is_olthoi_player(w, this)
        {
            send(
                w,
                this,
                game_message_system_chat::game_message_system_chat(
                    "You may not leave Olthoi Island until your account and this character have been active on this game world for 15 days.",
                    ChatMessageType::Broadcast,
                ),
            );
        }
    }

    let o = w.objects.get(this).expect("ACE: this is null");
    if o.player_killer_status() == PlayerKillerStatus::PKLite
        && !property_manager::get_bool(w, "pkl_server", false, true).item
    {
        w.objects
            .get_mut(this)
            .expect("ACE: this is null")
            .set_player_killer_status_prop(PlayerKillerStatus::NPK);

        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, f64::from(3.0f32));
        action_chain.add_action(Actor::Object(this), move |w| {
            let session = session_of(w, this);
            let msg = game_event_weenie_error::game_event_weenie_error(
                session_data(w, session),
                WeenieError::YouAreNonPKAgain,
            );
            game_message::enqueue_send(w, session, msg);
        });
        action_chain.enqueue_chain(w);
    }

    handle_pre_order_items(w, this);

    // SendSelf will trigger the entrance into portal space
    send_self(w, this);

    // Update or override certain properties sent to client.

    // bugged: do not send this here, or else a freshly loaded acclient will overrwrite the values
    // wait until first enter world is completed

    //SendPropertyUpdatesAndOverrides();

    if property_manager::get_bool(w, "use_turbine_chat", false, true).item {
        // Init the client with the chat channel ID's, and then notify the player that they've joined the associated channels.
        let session = session_of(w, this);
        let msg = game_event_weenie_error::game_event_weenie_error(
            session_data(w, session),
            WeenieError::TurbineChatIsEnabled,
        );
        game_message::enqueue_send(w, session, msg);

        if is_olthoi_player(w, this) {
            join_turbine_chat_channel(w, this, "Olthoi");
        } else {
            // `&& Allegiance != null` is left out here: join_turbine_chat_channel checks the allegiance itself.
            if shims::player_get_character_option(w, this, CharacterOption::ListenToAllegianceChat)
            {
                join_turbine_chat_channel(w, this, "Allegiance");
            }
            if shims::player_get_character_option(w, this, CharacterOption::ListenToGeneralChat) {
                join_turbine_chat_channel(w, this, "General");
            }
            if shims::player_get_character_option(w, this, CharacterOption::ListenToTradeChat) {
                join_turbine_chat_channel(w, this, "Trade");
            }
            if shims::player_get_character_option(w, this, CharacterOption::ListenToLFGChat) {
                join_turbine_chat_channel(w, this, "LFG");
            }
            if shims::player_get_character_option(w, this, CharacterOption::ListenToRoleplayChat) {
                join_turbine_chat_channel(w, this, "Roleplay");
            }
            if shims::player_get_character_option(w, this, CharacterOption::ListenToSocietyChat)
                && society(w, this) != FactionBits::None
            {
                join_turbine_chat_channel(w, this, "Society");
            }
        }
    }

    // check if vassals earned XP while offline
    crate::world_objects::player_allegiance::handle_allegiance_on_login(w, this);
    crate::world_objects::player_house::handle_house_on_login(w, this);

    // retail appeared to send the squelch list very early,
    // even before the CreatePlayer, but doing it here
    if crate::world_objects::managers::squelch_manager::has_squelches(w, this) {
        crate::world_objects::managers::squelch_manager::send_squelch_db(w, this);
    }

    crate::world_objects::player_spells::audit_item_spells(w, this);
    crate::world_objects::player_inventory::audit_equipped_items(w, this);

    crate::world_objects::player_xp::handle_missing_xp(w, this);
    crate::world_objects::player_skills::handle_skill_credit_refund(w, this);
    crate::world_objects::player_skills::handle_skill_temples_reset(w, this);
    crate::world_objects::player_skills::handle_skill_spec_credit_refund(w, this);
    crate::world_objects::player_skills::handle_free_skill_reset_renewal(w, this);
    crate::world_objects::player_skills::handle_free_attribute_reset_renewal(w, this);
    crate::world_objects::player_skills::handle_free_mastery_reset_renewal(w, this);

    crate::world_objects::player_skills::handle_db_updates(w, this);

    if w.server_manager.shutdown_initiated {
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, 10.0);
        action_chain.add_action(Actor::Object(this), move |w| {
            let text = crate::managers::server_manager::shutdown_notice_text(w);
            crate::world_objects::player::send_message(
                w,
                this,
                &text,
                ChatMessageType::WorldBroadcast,
            );
        });
        action_chain.enqueue_chain(w);
    }

    log::debug!(
        "[LOGIN] Account entered the world with character (0x{:08X}).",
        this.full()
    );
}

/// `Player.IsOlthoiPlayer` (Player_Properties.cs), as `SetEphemeralValues` set it.
fn is_olthoi_player(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .and_then(|o| o.player.as_ref())
        .is_some_and(|p| p.player_properties.is_olthoi_player)
}

/// `Player.IsAdmin` (`Player_Properties.cs`).
fn is_admin(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_admin_prop)
}

/// `Player.Allegiance` (`Player_Allegiance.cs`).
fn player_allegiance(w: &World, this: ObjectGuid) -> Option<ObjectGuid> {
    crate::world_objects::player_allegiance::i_player_allegiance(
        w,
        crate::entity::i_player::IPlayer::Online(this),
    )
}

/// `Creature.Society` (`Creature_Properties.cs`): `Faction1Bits ?? FactionBits.None`.
fn society(w: &World, this: ObjectGuid) -> FactionBits {
    w.objects
        .get(this)
        .and_then(|o| o.faction1_bits())
        .unwrap_or(FactionBits::None)
}

// ACE: Player.SendTurbineChatChannels
/// `break_allegiance` defaults to false in ACE.
pub fn send_turbine_chat_channels(w: &mut World, this: ObjectGuid, break_allegiance: bool) {
    let allegiance = player_allegiance(w, this);
    let allegiance_channel = match allegiance {
        Some(a) if !break_allegiance => {
            w.objects
                .get(a)
                .expect("ACE: Allegiance is null (NullReferenceException)")
                .biota
                .id
        }
        _ => 0u32,
    };

    let society_channel = match society(w, this) {
        s if s == FactionBits::CelestialHand => turbine_chat_channel::SOCIETY_CELESTIAL_HAND,
        s if s == FactionBits::EldrytchWeb => turbine_chat_channel::SOCIETY_ELDRYTCH_WEB,
        s if s == FactionBits::RadiantBlood => turbine_chat_channel::SOCIETY_RADIANT_BLOOD,
        _ => 0,
    };

    let session = session_of(w, this);
    let msg = game_event_set_turbine_chat_channels::game_event_set_turbine_chat_channels(
        session_data(w, session),
        allegiance_channel,
        society_channel,
    );
    game_message::enqueue_send(w, session, msg);
}

/// The society's channel name, for `Join`/`LeaveTurbineChatChannel`.
fn society_channel_name(society: FactionBits, channel_name: &str) -> String {
    match society {
        s if s == FactionBits::CelestialHand => "Celestial Hand".to_owned(),
        s if s == FactionBits::EldrytchWeb => "Eldrytch Web".to_owned(),
        s if s == FactionBits::RadiantBlood => "Radiant Blood".to_owned(),
        _ => channel_name.to_owned(),
    }
}

// ACE: Player.JoinTurbineChatChannel
pub fn join_turbine_chat_channel(w: &mut World, this: ObjectGuid, channel_name: &str) {
    let mut channel_name = channel_name.to_owned();
    if channel_name == "Allegiance" && player_allegiance(w, this).is_none() {
        return;
    } else if channel_name == "Society" {
        let society = society(w, this);
        if society == FactionBits::None {
            return;
        }

        channel_name = society_channel_name(society, &channel_name);
    } else if channel_name == "Olthoi" && !is_olthoi_player(w, this) {
        return;
    }

    let session = session_of(w, this);
    let msg = game_event_weenie_error_with_string::game_event_weenie_error_with_string(
        session_data(w, session),
        WeenieErrorWithString::YouHaveEnteredThe_Channel,
        &channel_name,
    );
    game_message::enqueue_send(w, session, msg);

    send_turbine_chat_channels(w, this, false);
}

// ACE: Player.LeaveTurbineChatChannel
/// `break_allegiance` defaults to false in ACE.
///
/// Not ACE's (fix, V319): the Olthoi channel is left by an Olthoi player or
/// an admin, so an ordinary Olthoi player is told "You have left the Olthoi channel." when it
/// logs out. ACE tested `!IsOlthoiPlayer || !IsAdmin`, which only an Olthoi admin passed.
pub fn leave_turbine_chat_channel(
    w: &mut World,
    this: ObjectGuid,
    channel_name: &str,
    break_allegiance: bool,
) {
    let mut channel_name = channel_name.to_owned();
    if channel_name == "Allegiance" && !break_allegiance && player_allegiance(w, this).is_none() {
        return;
    } else if channel_name == "Society" {
        let society = society(w, this);
        if society == FactionBits::None {
            return;
        }

        channel_name = society_channel_name(society, &channel_name);
    } else if (channel_name == "Olthoi" && !is_olthoi_player(w, this) && !is_admin(w, this))
        || (is_olthoi_player(w, this) && !is_admin(w, this) && channel_name != "Olthoi")
    {
        // ACE's two `else if` arms, both `return`.
        return;
    }

    let session = session_of(w, this);
    let msg = game_event_weenie_error_with_string::game_event_weenie_error_with_string(
        session_data(w, session),
        WeenieErrorWithString::YouHaveLeftThe_Channel,
        &channel_name,
    );
    game_message::enqueue_send(w, session, msg);

    send_turbine_chat_channels(w, this, break_allegiance);
}

// ACE: Player.SendSelf
/// The player description, title and friends list, then `PlayerCreate` and the player's own
/// `CreateObject` (with no placement), the inventory and wielded items, and the contract table.
fn send_self(w: &mut World, this: ObjectGuid) {
    let session = session_of(w, this);
    let player = game_event_player_description::game_event_player_description(w, session);
    let title = game_event_character_title::game_event_character_title(w, session);
    let friends = game_event_friends_list_update::game_event_friends_list_update(w, session);

    game_message::enqueue_send_many(w, session, [player, title, friends]);

    // Player objects don't get a placement
    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_placement(None);
    let create_player = game_message_player_create::game_message_player_create(this);
    let create_object =
        game_message_create_object::game_message_create_object(w, this, false, false);
    game_message::enqueue_send_many(w, session, [create_player, create_object]);

    send_inventory_and_wielded_items(w, this);

    send_contract_tracker_table(w, this);
}

// ACE: Player.SendPropertyUpdatesAndOverrides
pub fn send_property_updates_and_overrides(w: &mut World, this: ObjectGuid) {
    if !shims::property_manager_get_bool(w, "require_spell_comps", false) {
        let o = w.objects.get_mut(this).expect("ACE: this is null");
        let msg =
            game_message_private_update_property_bool::game_message_private_update_property_bool(
                o,
                PropertyBool::SpellComponentsRequired,
                false,
            );
        send(w, this, msg);
    }
}

// ACE: Player.SendInventoryAndWieldedItems
/// This method iterates through your main pack, any packs and finds all the items contained. It
/// also iterates over your wielded items. It sends the create object messages needed by the login
/// process; it is called from SendSelf as part of the login message traffic.
pub fn send_inventory_and_wielded_items(w: &mut World, this: ObjectGuid) {
    let session = session_of(w, this);
    for item in crate::world_objects::container::inventory_values(w, this) {
        let msg = game_message_create_object::game_message_create_object(w, item, false, false);
        game_message::enqueue_send(w, session, msg);

        // Was the item I just send a container? If so, we need to send the items in the container as well. Og II
        if w.objects
            .get(item)
            .is_some_and(crate::world_objects::world_object::WorldObject::is_container)
        {
            let msg = game_event_view_contents::game_event_view_contents(w, session, item);
            game_message::enqueue_send(w, session, msg);

            for items_in_container in crate::world_objects::container::inventory_values(w, item) {
                let msg = game_message_create_object::game_message_create_object(
                    w,
                    items_in_container,
                    false,
                    false,
                );
                game_message::enqueue_send(w, session, msg);
            }
        }
    }

    for item in crate::world_objects::creature_equipment::equipped_objects_values(w, this) {
        if let Some(o) = w.objects.get_mut(item) {
            o.wielder = Some(this);
        }
        let msg = game_message_create_object::game_message_create_object(w, item, false, false);
        game_message::enqueue_send(w, session, msg);
    }
}

// ACE: Player.SendContractTrackerTable
pub fn send_contract_tracker_table(w: &mut World, this: ObjectGuid) {
    let contracts = w
        .objects
        .get(this)
        .and_then(shims::player_character)
        .expect("ACE: Player.Character is null (NullReferenceException)")
        .character_properties_contract_registry
        .len();
    if contracts > 0 {
        let session = session_of(w, this);
        let msg = game_event_send_client_contract_tracker_table::game_event_send_client_contract_tracker_table(w, session);
        game_message::enqueue_send(w, session, msg);
    }
}

// ACE: Player.SendFriendStatusUpdates
/// Will send out GameEventFriendsListUpdate packets to everyone online that has this player as a
/// friend.
pub fn send_friend_status_updates(
    w: &mut World,
    this: ObjectGuid,
    previously_online: bool,
    is_online: bool,
) {
    let appear_offline =
        shims::player_get_character_option(w, this, CharacterOption::AppearOffline);
    let previously_online_and_is_now_offline = previously_online && !is_online;
    let previously_offline_and_is_now_online = !previously_online && is_online;
    let previously_offline_and_appear_offline = !previously_online && appear_offline;

    if (previously_offline_and_is_now_online && !previously_offline_and_appear_offline)
        || previously_online_and_is_now_offline
    {
        let name = crate::dispatch::name::name(w, this).unwrap_or_default();
        let msg = format!(
            "{name} has {}line.",
            if is_online { "come on" } else { "gone off" }
        );

        let inverse_friends = crate::managers::player_manager::get_online_inverse_friends(w, this);

        for friend in inverse_friends {
            // `new CharacterPropertiesFriendList { CharacterId = friend.Guid.Full, FriendId = Guid.Full }`
            let session = session_of(w, friend);
            let update = crate::network::game_event::events::game_event_friends_list_update::game_event_friends_list_update_one(
                w,
                session,
                crate::network::game_event::events::game_event_friends_list_update::FriendsUpdateTypeFlag::FriendStatusChanged,
                this.full(),
                true,
                is_online,
            );
            game_message::enqueue_send(w, session, update);
            crate::world_objects::player::send_message(
                w,
                friend,
                &msg,
                empyrean_entity::enums::ChatMessageType::Broadcast,
            );
        }
    }
}

// ACE: Player.SetRequestedLocation
/// Records where the client thinks we are, for use by physics engine later. `broadcast` defaults
/// to true.
pub fn set_requested_location(w: &mut World, this: ObjectGuid, pos: Position, broadcast: bool) {
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.wo.world_object.requested_location = Some(pos);
    o.wo.world_object.requested_location_broadcast = broadcast;
}

// ACE: Player.BroadcastMovement
/// A client MoveToState: updates the motion state, drops a repeated soul emote, and broadcasts
/// the movement as a `MovementData`.
pub fn broadcast_movement(w: &mut World, this: ObjectGuid, move_to_state: &MoveToState) {
    let mut state = move_to_state.raw_motion_state.clone();

    // update current style
    if (state.flags & RawMotionFlags::CurrentStyle) != 0 {
        // this lowercase stance field in Player doesn't really seem to be used anywhere
        // (ACE keeps `stance = state.CurrentStyle` in a field nothing reads; it is not kept here)
    }

    let sub_state = CommandMask::SubState.0;

    // update CurrentMotionState here for substates?
    if (state.flags & RawMotionFlags::ForwardCommand) != 0 {
        if (state.forward_command.0 & sub_state) != 0 {
            set_forward_command(w, this, state.forward_command);
        }
    } else {
        set_forward_command(w, this, MotionCommand::Ready);
    }

    if state.command_list_length > 0 {
        let first = state
            .commands
            .as_ref()
            .and_then(|c| c.first())
            .expect("ACE: Commands[0] (ArgumentOutOfRangeException)");
        if (first.motion_command.0 & sub_state) != 0 {
            set_forward_command(w, this, first.motion_command);
        }
    }

    if state.has_soul_emote(false) {
        // prevent soul emote spam / bug where client sends multiples
        let first = *state
            .commands
            .as_ref()
            .and_then(|c| c.first())
            .expect("checked by HasSoulEmote");
        let soul_emote = first.motion_command;
        let now = w.now.utc;
        let fields = &w
            .objects
            .get(this)
            .expect("ACE: this is null")
            .player
            .as_ref()
            .expect("a player")
            .player_networking;
        if soul_emote == fields.last_soul_emote && now < fields.last_soul_emote_end_time {
            if let Some(c) = state.commands.as_mut() {
                c.clear();
            }
            state.command_list_length = 0;
        } else {
            let o = w.objects.get(this).expect("ACE: this is null");
            let stance =
                shims::current_motion_state(o).map_or(MotionStance::NonCombat, |m| m.stance);
            let anim_length = shims::motion_table_get_animation_length(
                w,
                o.motion_table_id(),
                stance,
                soul_emote,
                None,
                first.speed,
            );

            let end = now.add_seconds(f64::from(anim_length));
            let fields = &mut w
                .objects
                .get_mut(this)
                .expect("ACE: this is null")
                .player
                .as_mut()
                .expect("a player")
                .player_networking;
            fields.last_soul_emote = soul_emote;
            fields.last_soul_emote_end_time = end;
        }
    }

    let mut edited = move_to_state.clone();
    edited.raw_motion_state = state;
    let movement_data = MovementData::from_move_to_state(w, this, &edited);

    // copy some fields to CurrentMotionState?
    // this is a mess, fix this whole architecture.
    let invalid_state = movement_data
        .invalid
        .as_ref()
        .expect("set by the MoveToState constructor")
        .state
        .clone();
    if let Some(o) = w.objects.get_mut(this) {
        if let Some(current) = o.wo.world_object_properties.current_motion_state.as_mut() {
            current.motion_state.forward_command = invalid_state.forward_command;
            current.motion_state.forward_speed = invalid_state.forward_speed;
            current.motion_state.turn_command = invalid_state.turn_command;
            current.motion_state.turn_speed = invalid_state.turn_speed;
            current.motion_state.sidestep_command = invalid_state.sidestep_command;
            current.motion_state.sidestep_speed = invalid_state.sidestep_speed;
        } else {
            panic!("ACE: CurrentMotionState is null (NullReferenceException)");
        }
    }

    let o = w.objects.get_mut(this).expect("ACE: this is null");
    let movement_event = game_message_update_motion::game_message_update_motion(o, &movement_data);
    enqueue_broadcast(w, this, true, &[movement_event]); // shouldn't need to go to originating player?

    // TODO: use real motion / animation system from physics
    //CurrentMotionCommand = movementData.Invalid.State.ForwardCommand;
    if let Some(o) = w.objects.get_mut(this) {
        o.wo.world_object_properties.current_movement_data = movement_data;
    }
}

/// `CurrentMotionState.SetForwardCommand(command)` (speed 1.0); a null state throws in ACE.
fn set_forward_command(w: &mut World, this: ObjectGuid, command: MotionCommand) {
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.wo.world_object_properties
        .current_motion_state
        .as_mut()
        .expect("ACE: CurrentMotionState is null (NullReferenceException)")
        .set_forward_command(command, 1.0);
}

// ACE: Player.SetFogColor
pub fn set_fog_color(w: &mut World, this: ObjectGuid, fog_color: EnvironChangeType) {
    let current = |w: &World| {
        w.objects
            .get(this)
            .and_then(|o| o.player.as_ref())
            .expect("a player")
            .player_networking
            .current_fog_color
    };
    let set_current = |w: &mut World, v: Option<EnvironChangeType>| {
        w.objects
            .get_mut(this)
            .and_then(|o| o.player.as_mut())
            .expect("a player")
            .player_networking
            .current_fog_color = v;
    };

    if fog_color == EnvironChangeType::Clear && current(w).is_none() {
        return;
    }

    let global_fog_color = w.landblock_manager.global_fog_color;
    if let Some(global) = global_fog_color.filter(|_| current(w) != Some(fog_color)) {
        set_current(w, Some(global));
        send_environ_change(w, this, global);
    } else if current(w) != Some(fog_color) {
        set_current(w, Some(fog_color));
        send_environ_change(w, this, fog_color);
    }

    if current(w) == Some(EnvironChangeType::Clear) {
        set_current(w, None);
    }
}

// ACE: Player.ClearFogColor
pub fn clear_fog_color(w: &mut World, this: ObjectGuid) {
    set_fog_color(w, this, EnvironChangeType::Clear);
}

// ACE: Player.SendEnvironChange
pub fn send_environ_change(
    w: &mut World,
    this: ObjectGuid,
    environ_change_type: EnvironChangeType,
) {
    send(
        w,
        this,
        game_message_admin_environs::game_message_admin_environs(environ_change_type),
    );
}

// ACE: Player.SetPlayerKillerStatus
/// `broadcast` defaults to false in ACE.
pub fn set_player_killer_status(
    w: &mut World,
    this: ObjectGuid,
    player_killer_status: PlayerKillerStatus,
    broadcast: bool,
) {
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    match player_killer_status {
        s if s == PlayerKillerStatus::NPK
            || s == PlayerKillerStatus::PK
            || s == PlayerKillerStatus::PKLite =>
        {
            o.set_player_killer_status_prop(PlayerKillerStatus::NPK);
            o.set_minimum_time_since_pk_prop(Some(0.0));
        }
        s if s == PlayerKillerStatus::Free => {
            o.set_player_killer_status_prop(PlayerKillerStatus::Free);
        }
        _ => {}
    }

    if broadcast {
        let status = o.player_killer_status();
        let msg = game_message_public_update_property_int::game_message_public_update_property_int(
            o,
            PropertyInt::PlayerKillerStatus,
            status.0.cast_signed(),
        );
        enqueue_broadcast(w, this, true, &[msg]);
    }
}

// ACE: Player.SendWeenieError
pub fn send_weenie_error(w: &mut World, this: ObjectGuid, error: WeenieError) {
    let session = session_of(w, this);
    let msg = game_event_weenie_error::game_event_weenie_error(session_data(w, session), error);
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.SendWeenieErrorWithString
pub fn send_weenie_error_with_string(
    w: &mut World,
    this: ObjectGuid,
    error: WeenieErrorWithString,
    str: &str,
) {
    let session = session_of(w, this);
    let msg = game_event_weenie_error_with_string::game_event_weenie_error_with_string(
        session_data(w, session),
        error,
        str,
    );
    game_message::enqueue_send(w, session, msg);
}

// ACE: Player.SendTransientError
pub fn send_transient_error(w: &mut World, this: ObjectGuid, msg: &str) {
    let session = session_of(w, this);
    let m = game_event_communication_transient_string::game_event_communication_transient_string(
        session_data(w, session),
        msg,
    );
    game_message::enqueue_send(w, session, m);
}

// ACE: Player.HandleActionSetAFKMode
pub fn handle_action_set_afk_mode(w: &mut World, this: ObjectGuid, afk_status: bool) {
    let o = w.objects.get_mut(this).expect("ACE: this is null");
    o.set_is_afk(afk_status);

    let is_afk = o.is_afk();
    let msg = game_message_private_update_property_bool::game_message_private_update_property_bool(
        o,
        PropertyBool::Afk,
        is_afk,
    );
    send(w, this, msg);
}

// ACE: Player.DefaultAFKMessage
/// client default (/afk msg)
pub const DEFAULT_AFK_MESSAGE: &str = "I am currently away from the keyboard.";

// ACE: Player.HandleActionSetAFKMessage
pub fn handle_action_set_afk_message(w: &mut World, this: ObjectGuid, afk_message: &str) {
    // `string.IsNullOrWhiteSpace`
    let afk_message = if afk_message.chars().all(char::is_whitespace) {
        DEFAULT_AFK_MESSAGE // client default
    } else {
        afk_message
    };

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_afk_message(Some(afk_message.to_owned()));
}

// ACE: Player.HandlePreOrderItems
/// The Throne of Destiny gifts for the server's subscription level.
pub fn handle_pre_order_items(w: &mut World, this: ObjectGuid) {
    use empyrean_entity::enums::{SubscriptionStatus, WeenieClassName};

    // `(SubscriptionStatus)PropertyManager.GetLong(..)`: an int enum from a long, truncated.
    #[allow(clippy::cast_possible_truncation)]
    let subscription_status = SubscriptionStatus(
        property_manager::get_long(w, "default_subscription_level", 0, true).item as i32,
    );

    let status;
    let success;
    if subscription_status == SubscriptionStatus::ThroneOfDestiny_Preordered {
        status = "pre-ordering";
        try_create_pre_order_item(
            w,
            this,
            PropertyBool::ActdReceivedItems,
            WeenieClassName::W_GEMACTDPURCHASEREWARDARMOR_CLASS,
        ); // pcaps show this actually didn't occur on retail. odd
        success = try_create_pre_order_item(
            w,
            this,
            PropertyBool::ActdPreorderReceivedItems,
            WeenieClassName::W_GEMACTDPURCHASEREWARDHEALTH_CLASS,
        );
    } else {
        status = "purchasing";
        success = try_create_pre_order_item(
            w,
            this,
            PropertyBool::ActdReceivedItems,
            WeenieClassName::W_GEMACTDPURCHASEREWARDARMOR_CLASS,
        );
    }

    let msg = format!("Thank you for {status} the Throne of Destiny expansion! A special gift has been placed in your backpack.");

    if property_manager::get_bool(w, "show_first_login_gift", false, true).item && success {
        send(
            w,
            this,
            game_message_system_chat::game_message_system_chat(&msg, ChatMessageType::Magic),
        );
    }

    w.objects
        .get_mut(this)
        .expect("ACE: this is null")
        .set_account_requirements_player(subscription_status);
}

// ACE: Player.TryCreatePreOrderItem
fn try_create_pre_order_item(
    w: &mut World,
    this: ObjectGuid,
    property_bool: PropertyBool,
    weenie_class_name: empyrean_entity::enums::WeenieClassName,
) -> bool {
    let rcvd_blackmoors_favor = w
        .objects
        .get(this)
        .and_then(|o| o.get_property(property_bool))
        .unwrap_or(false);
    if !rcvd_blackmoors_favor {
        let wcid = u32::from(weenie_class_name.0);
        if crate::world_objects::container::get_inventory_items_of_wcid(w, this, wcid).is_empty() {
            let Some(cached_weenie) = w.content.get_cached_weenie(wcid) else {
                return false;
            };

            let Some(wo) = crate::factories::world_object_factory::create_new_world_object_in_world(
                w,
                cached_weenie,
            ) else {
                return false;
            };
            let guid = wo.guid;
            assert!(
                w.objects.insert(wo).is_ok(),
                "a new dynamic guid 0x{:08X} is already live",
                guid.full()
            );
            crate::world_objects::creature::post_insert(w, guid);

            if crate::world_objects::container::try_add_to_inventory(w, this, guid, 0, false, true)
            {
                w.objects
                    .get_mut(this)
                    .expect("ACE: this is null")
                    .set_property(property_bool, true);
                return true;
            }
            // Not ACE: the object ACE drops is unreachable (C# collects it).
            w.objects.remove(guid);
        } else {
            // already had the item, set the property to reflect item was received
            w.objects
                .get_mut(this)
                .expect("ACE: this is null")
                .set_property(property_bool, true);
        }
    }

    false
}
