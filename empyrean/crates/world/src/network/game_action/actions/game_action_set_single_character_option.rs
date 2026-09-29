// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetSingleCharacterOption.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetSingleCharacterOption.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::CharacterOption;
use empyrean_net::SessionId;

use crate::network::game_messages::messages::game_message_obj_desc_event::game_message_obj_desc_event;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::{player_character, player_networking, world_object_networking};
use crate::World;

// ACE: GameActionSetSingleCharacterOption.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::login::CharacterPlayerOptionChangedEvent>()?;
    let option = CharacterOption(m.option.cs_cast());
    let option_value = m.value != 0;

    let player = session_player(w, session);

    match option {
        CharacterOption::AppearOffline => {
            player_character::set_appear_offline(w, player, option_value);
        }
        CharacterOption::AutomaticallyAcceptFellowshipRequests => {
            player_character::set_character_option(
                w,
                player,
                CharacterOption::AutomaticallyAcceptFellowshipRequests,
                option_value,
            );
        }
        CharacterOption::IgnoreFellowshipRequests => {
            player_character::set_character_option(
                w,
                player,
                CharacterOption::IgnoreFellowshipRequests,
                option_value,
            );
        }
        CharacterOption::ShowYourCloak => {
            player_character::set_character_option(
                w,
                player,
                CharacterOption::ShowYourCloak,
                option_value,
            );
            let msg = game_message_obj_desc_event(w, player);
            world_object_networking::enqueue_broadcast(w, player, true, &[msg]);
        }
        CharacterOption::ShowYourHelmOrHeadGear => {
            player_character::set_character_option(
                w,
                player,
                CharacterOption::ShowYourHelmOrHeadGear,
                option_value,
            );
            let msg = game_message_obj_desc_event(w, player);
            world_object_networking::enqueue_broadcast(w, player, true, &[msg]);
        }
        CharacterOption::ListenToAllegianceChat
        | CharacterOption::ListenToGeneralChat
        | CharacterOption::ListenToLFGChat
        | CharacterOption::ListenToRoleplayChat
        | CharacterOption::ListenToSocietyChat
        | CharacterOption::ListenToTradeChat => {
            let channel_name = option
                .to_string()
                .replace("ListenTo", "")
                .replace("Chat", "");
            player_character::set_character_option(w, player, option, option_value);
            if option_value {
                player_networking::join_turbine_chat_channel(w, player, &channel_name);
            } else {
                player_networking::leave_turbine_chat_channel(w, player, &channel_name, false);
            }
        }
        _ => {
            player_character::set_character_option(w, player, option, option_value);
        }
    }
    Ok(())
}
