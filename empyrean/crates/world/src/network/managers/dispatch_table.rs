// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Managers/InboundMessageManager.cs
// @generated from the `[GameMessage]` and `[GameAction]` attributes in ACE's `Source/ACE.Server`; do not edit by hand
//! ACE's reflection-built handler dictionaries, as build-time tables: every `[GameMessage]` and
//! `[GameAction]` method in ACE.Server, sorted by opcode. Lookups are binary searches.

use empyrean_net::SessionState;

use crate::network::game_action::game_action_attribute::GameActionAttribute;
use crate::network::game_messages::game_message_attribute::GameMessageAttribute;
use crate::network::managers::inbound_message_manager::{ActionHandlerInfo, MessageHandlerInfo};

// ACE: InboundMessageManager.DefineMessageHandlers
/// ACE `messageHandlers`: the 14 `[GameMessage]` handlers, by opcode.
pub static MESSAGE_HANDLERS: &[MessageHandlerInfo] = &[
    // CharacterHandler.CharacterLogOff (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterLogOff",
        handler: crate::network::handlers::character_handler::character_log_off,
        attribute: GameMessageAttribute::new(0xF653, SessionState::WorldConnected),
    },
    // CharacterHandler.CharacterDelete (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterDelete",
        handler: crate::network::handlers::character_handler::character_delete,
        attribute: GameMessageAttribute::new(0xF655, SessionState::AuthConnected),
    },
    // CharacterHandler.CharacterCreate (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterCreate",
        handler: crate::network::handlers::character_handler::character_create,
        attribute: GameMessageAttribute::new(0xF656, SessionState::AuthConnected),
    },
    // CharacterHandler.CharacterEnterWorld (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterEnterWorld",
        handler: crate::network::handlers::character_handler::character_enter_world,
        attribute: GameMessageAttribute::new(0xF657, SessionState::AuthConnected),
    },
    // ControlHandler.ControlResponse (Source/ACE.Server/Network/Handlers/ControlHandler.cs)
    MessageHandlerInfo {
        name: "ForceObjectDescSend",
        handler: crate::network::handlers::control_handler::control_response,
        attribute: GameMessageAttribute::new(0xF6EA, SessionState::WorldConnected),
    },
    // GameActionPacket.HandleGameAction (Source/ACE.Server/Network/GameAction/GameActionPacket.cs)
    MessageHandlerInfo {
        name: "GameAction",
        handler: crate::network::game_action::game_action_packet::handle_game_action,
        attribute: GameMessageAttribute::new(0xF7B1, SessionState::WorldConnected),
    },
    // CharacterHandler.CharacterEnterWorldRequest (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterEnterWorldRequest",
        handler: crate::network::handlers::character_handler::character_enter_world_request,
        attribute: GameMessageAttribute::new(0xF7C8, SessionState::AuthConnected),
    },
    // GetServerVersionHandler.GetServerVersion (Source/ACE.Server/Network/Handlers/GetServerVersionHandler.cs)
    MessageHandlerInfo {
        name: "GetServerVersion",
        handler: crate::network::handlers::get_server_version_handler::get_server_version,
        attribute: GameMessageAttribute::new(0xF7CC, SessionState::WorldConnected),
    },
    // FriendsOldHandler.FriendsOld (Source/ACE.Server/Network/Handlers/FriendsOldHandler.cs)
    MessageHandlerInfo {
        name: "FriendsOld",
        handler: crate::network::handlers::friends_old_handler::friends_old,
        attribute: GameMessageAttribute::new(0xF7CD, SessionState::WorldConnected),
    },
    // CharacterHandler.CharacterRestore (Source/ACE.Server/Network/Handlers/CharacterHandler.cs)
    MessageHandlerInfo {
        name: "CharacterRestore",
        handler: crate::network::handlers::character_handler::character_restore,
        attribute: GameMessageAttribute::new(0xF7D9, SessionState::AuthConnected),
    },
    // TurbineChatHandler.TurbineChatReceived (Source/ACE.Server/Network/Handlers/TurbineChatHandler.cs)
    MessageHandlerInfo {
        name: "TurbineChat",
        handler: crate::network::handlers::turbine_chat_handler::turbine_chat_received,
        attribute: GameMessageAttribute::new(0xF7DE, SessionState::WorldConnected),
    },
    // DDDHandler.DDD_RequestDataMessage (Source/ACE.Server/Network/Handlers/DDDHandler.cs)
    MessageHandlerInfo {
        name: "DDD_RequestDataMessage",
        handler: crate::network::handlers::ddd_handler::ddd_request_data_message,
        attribute: GameMessageAttribute::new(0xF7E3, SessionState::WorldConnected),
    },
    // DDDHandler.DDD_InterrogationResponse (Source/ACE.Server/Network/Handlers/DDDHandler.cs)
    MessageHandlerInfo {
        name: "DDD_InterrogationResponse",
        handler: crate::network::handlers::ddd_handler::ddd_interrogation_response,
        attribute: GameMessageAttribute::new(0xF7E6, SessionState::AuthConnected),
    },
    // DDDHandler.DDD_EndDDD (Source/ACE.Server/Network/Handlers/DDDHandler.cs)
    MessageHandlerInfo {
        name: "DDD_EndDDD",
        handler: crate::network::handlers::ddd_handler::ddd_end_ddd,
        attribute: GameMessageAttribute::new(0xF7EA, SessionState::AuthConnected),
    },
];

// ACE: InboundMessageManager.DefineActionHandlers
/// ACE `actionHandlers`: the 149 `[GameAction]` handlers, by opcode.
pub static ACTION_HANDLERS: &[ActionHandlerInfo] = &[
    // GameActionSetSingleCharacterOption.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetSingleCharacterOption.cs)
    ActionHandlerInfo {
        name: "SetSingleCharacterOption",
        handler: crate::network::game_action::actions::game_action_set_single_character_option::handle,
        attribute: GameActionAttribute::new(0x0005),
    },
    // GameActionTargetedMeleeAttack.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMeleeAttack.cs)
    ActionHandlerInfo {
        name: "TargetedMeleeAttack",
        handler: crate::network::game_action::actions::game_action_targeted_melee_attack::handle,
        attribute: GameActionAttribute::new(0x0008),
    },
    // GameActionTargetedMissileAttack.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTargetedMissileAttack.cs)
    ActionHandlerInfo {
        name: "TargetedMissileAttack",
        handler: crate::network::game_action::actions::game_action_targeted_missile_attack::handle,
        attribute: GameActionAttribute::new(0x000A),
    },
    // GameActionSetAFKMode.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMode.cs)
    ActionHandlerInfo {
        name: "SetAfkMode",
        handler: crate::network::game_action::actions::game_action_set_afk_mode::handle,
        attribute: GameActionAttribute::new(0x000F),
    },
    // GameActionSetAFKMessage.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAFKMessage.cs)
    ActionHandlerInfo {
        name: "SetAfkMessage",
        handler: crate::network::game_action::actions::game_action_set_afk_message::handle,
        attribute: GameActionAttribute::new(0x0010),
    },
    // GameActionTalk.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTalk.cs)
    ActionHandlerInfo {
        name: "Talk",
        handler: crate::network::game_action::actions::game_action_talk::handle,
        attribute: GameActionAttribute::new(0x0015),
    },
    // GameActionRemoveFriend.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveFriend.cs)
    ActionHandlerInfo {
        name: "RemoveFriend",
        handler: crate::network::game_action::actions::game_action_remove_friend::handle,
        attribute: GameActionAttribute::new(0x0017),
    },
    // GameActionAddFriend.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddFriend.cs)
    ActionHandlerInfo {
        name: "AddFriend",
        handler: crate::network::game_action::actions::game_action_add_friend::handle,
        attribute: GameActionAttribute::new(0x0018),
    },
    // GameActionPutItemInContainer.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionPutItemInContainer.cs)
    ActionHandlerInfo {
        name: "PutItemInContainer",
        handler: crate::network::game_action::actions::game_action_put_item_in_container::handle,
        attribute: GameActionAttribute::new(0x0019),
    },
    // GameActionGetAndWieldItem.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionGetAndWieldItem.cs)
    ActionHandlerInfo {
        name: "GetAndWieldItem",
        handler: crate::network::game_action::actions::game_action_get_and_wield_item::handle,
        attribute: GameActionAttribute::new(0x001A),
    },
    // GameActionDropItem.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDropItem.cs)
    ActionHandlerInfo {
        name: "DropItem",
        handler: crate::network::game_action::actions::game_action_drop_item::handle,
        attribute: GameActionAttribute::new(0x001B),
    },
    // GameActionAllegianceSwearAllegiance.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceSwearAllegiance.cs)
    ActionHandlerInfo {
        name: "SwearAllegiance",
        handler: crate::network::game_action::actions::game_action_allegiance_swear_allegiance::handle,
        attribute: GameActionAttribute::new(0x001D),
    },
    // GameActionAllegianceBreakAllegiance.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceBreakAllegiance.cs)
    ActionHandlerInfo {
        name: "BreakAllegiance",
        handler: crate::network::game_action::actions::game_action_allegiance_break_allegiance::handle,
        attribute: GameActionAttribute::new(0x001E),
    },
    // GameActionAllegianceUpdateRequest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceUpdateRequest.cs)
    ActionHandlerInfo {
        name: "AllegianceUpdateRequest",
        handler: crate::network::game_action::actions::game_action_allegiance_update_request::handle,
        attribute: GameActionAttribute::new(0x001F),
    },
    // GameActionRemoveAllFriends.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveAllFriends.cs)
    ActionHandlerInfo {
        name: "RemoveAllFriends",
        handler: crate::network::game_action::actions::game_action_remove_all_friends::handle,
        attribute: GameActionAttribute::new(0x0025),
    },
    // GameActionTeleToPklArena.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToPklArena.cs)
    ActionHandlerInfo {
        name: "TeleToPklArena",
        handler: crate::network::game_action::actions::game_action_tele_to_pkl_arena::handle,
        attribute: GameActionAttribute::new(0x0026),
    },
    // GameActionTeleToPkArena.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToPkArena.cs)
    ActionHandlerInfo {
        name: "TeleToPkArena",
        handler: crate::network::game_action::actions::game_action_tele_to_pk_arena::handle,
        attribute: GameActionAttribute::new(0x0027),
    },
    // GameActionSetTitle.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetTitle.cs)
    ActionHandlerInfo {
        name: "TitleSet",
        handler: crate::network::game_action::actions::game_action_set_title::handle,
        attribute: GameActionAttribute::new(0x002C),
    },
    // GameActionQueryAllegianceName.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryAllegianceName.cs)
    ActionHandlerInfo {
        name: "QueryAllegianceName",
        handler: crate::network::game_action::actions::game_action_query_allegiance_name::handle,
        attribute: GameActionAttribute::new(0x0030),
    },
    // GameActionClearAllegianceName.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionClearAllegianceName.cs)
    ActionHandlerInfo {
        name: "ClearAllegianceName",
        handler: crate::network::game_action::actions::game_action_clear_allegiance_name::handle,
        attribute: GameActionAttribute::new(0x0031),
    },
    // GameActionTalkDirect.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTalkDirect.cs)
    ActionHandlerInfo {
        name: "TalkDirect",
        handler: crate::network::game_action::actions::game_action_talk_direct::handle,
        attribute: GameActionAttribute::new(0x0032),
    },
    // GameActionSetAllegianceName.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceName.cs)
    ActionHandlerInfo {
        name: "SetAllegianceName",
        handler: crate::network::game_action::actions::game_action_set_allegiance_name::handle,
        attribute: GameActionAttribute::new(0x0033),
    },
    // GameActionUseWithTarget.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionUseWithTarget.cs)
    ActionHandlerInfo {
        name: "UseWithTarget",
        handler: crate::network::game_action::actions::game_action_use_with_target::handle,
        attribute: GameActionAttribute::new(0x0035),
    },
    // GameActionUseItem.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionUseItem.cs)
    ActionHandlerInfo {
        name: "Use",
        handler: crate::network::game_action::actions::game_action_use_item::handle,
        attribute: GameActionAttribute::new(0x0036),
    },
    // GameActionSetAllegianceOfficer.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceOfficer.cs)
    ActionHandlerInfo {
        name: "SetAllegianceOfficer",
        handler: crate::network::game_action::actions::game_action_set_allegiance_officer::handle,
        attribute: GameActionAttribute::new(0x003B),
    },
    // GameActionSetAllegianceOfficerTitle.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceOfficerTitle.cs)
    ActionHandlerInfo {
        name: "SetAllegianceOfficerTitle",
        handler: crate::network::game_action::actions::game_action_set_allegiance_officer_title::handle,
        attribute: GameActionAttribute::new(0x003C),
    },
    // GameActionListAllegianceOfficerTitles.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionListAllegianceOfficerTitles.cs)
    ActionHandlerInfo {
        name: "ListAllegianceOfficerTitles",
        handler: crate::network::game_action::actions::game_action_list_allegiance_officer_titles::handle,
        attribute: GameActionAttribute::new(0x003D),
    },
    // GameActionClearAllegianceOfficerTitles.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionClearAllegianceOfficerTitles.cs)
    ActionHandlerInfo {
        name: "ClearAllegianceOfficerTitles",
        handler: crate::network::game_action::actions::game_action_clear_allegiance_officer_titles::handle,
        attribute: GameActionAttribute::new(0x003E),
    },
    // GameActionDoAllegianceLockAction.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDoAllegianceLockAction.cs)
    ActionHandlerInfo {
        name: "DoAllegianceLockAction",
        handler: crate::network::game_action::actions::game_action_do_allegiance_lock_action::handle,
        attribute: GameActionAttribute::new(0x003F),
    },
    // GameActionSetAllegianceApprovedVassal.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceApprovedVassal.cs)
    ActionHandlerInfo {
        name: "SetAllegianceApprovedVassal",
        handler: crate::network::game_action::actions::game_action_set_allegiance_approved_vassal::handle,
        attribute: GameActionAttribute::new(0x0040),
    },
    // GameActionAllegianceChatGag.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatGag.cs)
    ActionHandlerInfo {
        name: "AllegianceChatGag",
        handler: crate::network::game_action::actions::game_action_allegiance_chat_gag::handle,
        attribute: GameActionAttribute::new(0x0041),
    },
    // GameActionDoAllegianceHouseAction.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDoAllegianceHouseAction.cs)
    ActionHandlerInfo {
        name: "DoAllegianceHouseAction",
        handler: crate::network::game_action::actions::game_action_do_allegiance_house_action::handle,
        attribute: GameActionAttribute::new(0x0042),
    },
    // GameActionRaiseVital.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseVital.cs)
    ActionHandlerInfo {
        name: "RaiseVital",
        handler: crate::network::game_action::actions::game_action_raise_vital::handle,
        attribute: GameActionAttribute::new(0x0044),
    },
    // GameActionRaiseAttribute.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseAttribute.cs)
    ActionHandlerInfo {
        name: "RaiseAttribute",
        handler: crate::network::game_action::actions::game_action_raise_attribute::handle,
        attribute: GameActionAttribute::new(0x0045),
    },
    // GameActionRaiseSkill.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRaiseSkill.cs)
    ActionHandlerInfo {
        name: "RaiseSkill",
        handler: crate::network::game_action::actions::game_action_raise_skill::handle,
        attribute: GameActionAttribute::new(0x0046),
    },
    // GameActionTrainSkill.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTrainSkill.cs)
    ActionHandlerInfo {
        name: "TrainSkill",
        handler: crate::network::game_action::actions::game_action_train_skill::handle,
        attribute: GameActionAttribute::new(0x0047),
    },
    // GameActionMagicCastUnTargetedSpell.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionMagicCastUntargetedSpell.cs)
    ActionHandlerInfo {
        name: "CastUntargetedSpell",
        handler: crate::network::game_action::actions::game_action_magic_cast_untargeted_spell::handle,
        attribute: GameActionAttribute::new(0x0048),
    },
    // GameActionMagicCastTargetedSpell.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionMagicCastTargetedSpell.cs)
    ActionHandlerInfo {
        name: "CastTargetedSpell",
        handler: crate::network::game_action::actions::game_action_magic_cast_targeted_spell::handle,
        attribute: GameActionAttribute::new(0x004A),
    },
    // GameActionChangeCombatMode.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChangeCombatMode.cs)
    ActionHandlerInfo {
        name: "ChangeCombatMode",
        handler: crate::network::game_action::actions::game_action_change_combat_mode::handle,
        attribute: GameActionAttribute::new(0x0053),
    },
    // GameActionStackableMerge.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionStackableMerge.cs)
    ActionHandlerInfo {
        name: "StackableMerge",
        handler: crate::network::game_action::actions::game_action_stackable_merge::handle,
        attribute: GameActionAttribute::new(0x0054),
    },
    // GameActionStackableSplitToContainer.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitToContainer.cs)
    ActionHandlerInfo {
        name: "StackableSplitToContainer",
        handler: crate::network::game_action::actions::game_action_stackable_split_to_container::handle,
        attribute: GameActionAttribute::new(0x0055),
    },
    // GameActionStackableSplitTo3D.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitTo3D.cs)
    ActionHandlerInfo {
        name: "StackableSplitTo3D",
        handler: crate::network::game_action::actions::game_action_stackable_split_to3_d::handle,
        attribute: GameActionAttribute::new(0x0056),
    },
    // GameActionModifyCharacterSquelch.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionModifyCharacterSquelch.cs)
    ActionHandlerInfo {
        name: "ModifyCharacterSquelch",
        handler: crate::network::game_action::actions::game_action_modify_character_squelch::handle,
        attribute: GameActionAttribute::new(0x0058),
    },
    // GameActionModifyAccountSquelch.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionModifyAccountSquelch.cs)
    ActionHandlerInfo {
        name: "ModifyAccountSquelch",
        handler: crate::network::game_action::actions::game_action_modify_account_squelch::handle,
        attribute: GameActionAttribute::new(0x0059),
    },
    // GameActionModifyGlobalSquelch.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionModifyGlobalSquelch.cs)
    ActionHandlerInfo {
        name: "ModifyGlobalSquelch",
        handler: crate::network::game_action::actions::game_action_modify_global_squelch::handle,
        attribute: GameActionAttribute::new(0x005B),
    },
    // GameActionTell.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTell.cs)
    ActionHandlerInfo {
        name: "Tell",
        handler: crate::network::game_action::actions::game_action_tell::handle,
        attribute: GameActionAttribute::new(0x005D),
    },
    // GameActionBuyItems.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBuyItems.cs)
    ActionHandlerInfo {
        name: "Buy",
        handler: crate::network::game_action::actions::game_action_buy_items::handle,
        attribute: GameActionAttribute::new(0x005F),
    },
    // GameActionSellItems.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSellItems.cs)
    ActionHandlerInfo {
        name: "Sell",
        handler: crate::network::game_action::actions::game_action_sell_items::handle,
        attribute: GameActionAttribute::new(0x0060),
    },
    // GameActionTeleToLifestone.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToLifestone.cs)
    ActionHandlerInfo {
        name: "TeleToLifestone",
        handler: crate::network::game_action::actions::game_action_tele_to_lifestone::handle,
        attribute: GameActionAttribute::new(0x0063),
    },
    // GameActionLoginComplete.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionLoginComplete.cs)
    ActionHandlerInfo {
        name: "LoginComplete",
        handler: crate::network::game_action::actions::game_action_login_complete::handle,
        attribute: GameActionAttribute::new(0x00A1),
    },
    // GameActionFellowshipCreate.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipCreate.cs)
    ActionHandlerInfo {
        name: "FellowshipCreate",
        handler: crate::network::game_action::actions::game_action_fellowship_create::handle,
        attribute: GameActionAttribute::new(0x00A2),
    },
    // GameActionFellowshipQuit.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipQuit.cs)
    ActionHandlerInfo {
        name: "FellowshipQuit",
        handler: crate::network::game_action::actions::game_action_fellowship_quit::handle,
        attribute: GameActionAttribute::new(0x00A3),
    },
    // GameActionFellowshipDismiss.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipDismiss.cs)
    ActionHandlerInfo {
        name: "FellowshipDismiss",
        handler: crate::network::game_action::actions::game_action_fellowship_dismiss::handle,
        attribute: GameActionAttribute::new(0x00A4),
    },
    // GameActionFellowshipRecruit.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipRecruit.cs)
    ActionHandlerInfo {
        name: "FellowshipRecruit",
        handler: crate::network::game_action::actions::game_action_fellowship_recruit::handle,
        attribute: GameActionAttribute::new(0x00A5),
    },
    // GameActionFellowshipUpdateRequest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipUpdateRequest.cs)
    ActionHandlerInfo {
        name: "FellowshipUpdateRequest",
        handler: crate::network::game_action::actions::game_action_fellowship_update_request::handle,
        attribute: GameActionAttribute::new(0x00A6),
    },
    // GameActionBookData.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBookData.cs)
    ActionHandlerInfo {
        name: "BookData",
        handler: crate::network::game_action::actions::game_action_book_data::handle,
        attribute: GameActionAttribute::new(0x00AA),
    },
    // GameActionBookModifyPage.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBookModifyPage.cs)
    ActionHandlerInfo {
        name: "BookModifyPage",
        handler: crate::network::game_action::actions::game_action_book_modify_page::handle,
        attribute: GameActionAttribute::new(0x00AB),
    },
    // GameActionBookAddPage.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBookAddPage.cs)
    ActionHandlerInfo {
        name: "BookAddPage",
        handler: crate::network::game_action::actions::game_action_book_add_page::handle,
        attribute: GameActionAttribute::new(0x00AC),
    },
    // GameActionBookDeletePage.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBookDeletePage.cs)
    ActionHandlerInfo {
        name: "BookDeletePage",
        handler: crate::network::game_action::actions::game_action_book_delete_page::handle,
        attribute: GameActionAttribute::new(0x00AD),
    },
    // GameActionBookPageData.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBookPageData.cs)
    ActionHandlerInfo {
        name: "BookPageData",
        handler: crate::network::game_action::actions::game_action_book_page_data::handle,
        attribute: GameActionAttribute::new(0x00AE),
    },
    // GameActionSetInscription.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetInscription.cs)
    ActionHandlerInfo {
        name: "SetInscription",
        handler: crate::network::game_action::actions::game_action_set_inscription::handle,
        attribute: GameActionAttribute::new(0x00BF),
    },
    // GameActionIdentifyObject.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs)
    ActionHandlerInfo {
        name: "IdentifyObject",
        handler: crate::network::game_action::actions::game_action_identify_object::handle,
        attribute: GameActionAttribute::new(0x00C8),
    },
    // GameActionGiveObjectRequest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionGiveObjectRequest.cs)
    ActionHandlerInfo {
        name: "GiveObjectRequest",
        handler: crate::network::game_action::actions::game_action_give_object_request::handle,
        attribute: GameActionAttribute::new(0x00CD),
    },
    // GameActionAdvocateTeleport.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAdvocateTeleport.cs)
    ActionHandlerInfo {
        name: "AdvocateTeleport",
        handler: crate::network::game_action::actions::game_action_advocate_teleport::handle,
        attribute: GameActionAttribute::new(0x00D6),
    },
    // GameActionAddChannel.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddChannel.cs)
    ActionHandlerInfo {
        name: "AddChannel",
        handler: crate::network::game_action::actions::game_action_add_channel::handle,
        attribute: GameActionAttribute::new(0x0145),
    },
    // GameActionRemoveChannel.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveChannel.cs)
    ActionHandlerInfo {
        name: "RemoveChannel",
        handler: crate::network::game_action::actions::game_action_remove_channel::handle,
        attribute: GameActionAttribute::new(0x0146),
    },
    // GameActionChatChannel.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChatChannel.cs)
    ActionHandlerInfo {
        name: "ChatChannel",
        handler: crate::network::game_action::actions::game_action_chat_channel::handle,
        attribute: GameActionAttribute::new(0x0147),
    },
    // GameActionChannelList.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChannelList.cs)
    ActionHandlerInfo {
        name: "ListChannels",
        handler: crate::network::game_action::actions::game_action_channel_list::handle,
        attribute: GameActionAttribute::new(0x0148),
    },
    // GameActionChannelIndex.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChannelIndex.cs)
    ActionHandlerInfo {
        name: "IndexChannels",
        handler: crate::network::game_action::actions::game_action_channel_index::handle,
        attribute: GameActionAttribute::new(0x0149),
    },
    // GameActionNoLongerViewingContents.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionNoLongerViewingContents.cs)
    ActionHandlerInfo {
        name: "NoLongerViewingContents",
        handler: crate::network::game_action::actions::game_action_no_longer_viewing_contents::handle,
        attribute: GameActionAttribute::new(0x0195),
    },
    // GameActionStackableSplitToWield.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitToWield.cs)
    ActionHandlerInfo {
        name: "StackableSplitToWield",
        handler: crate::network::game_action::actions::game_action_stackable_split_to_wield::handle,
        attribute: GameActionAttribute::new(0x019B),
    },
    // GameActionAddShortcut.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddShortcut.cs)
    ActionHandlerInfo {
        name: "AddShortCut",
        handler: crate::network::game_action::actions::game_action_add_shortcut::handle,
        attribute: GameActionAttribute::new(0x019C),
    },
    // GameActionRemoveShortcut.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveShortcut.cs)
    ActionHandlerInfo {
        name: "RemoveShortCut",
        handler: crate::network::game_action::actions::game_action_remove_shortcut::handle,
        attribute: GameActionAttribute::new(0x019D),
    },
    // GameActionSetCharacterOptions.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetCharacterOptions.cs)
    ActionHandlerInfo {
        name: "SetCharacterOptions",
        handler: crate::network::game_action::actions::game_action_set_character_options::handle,
        attribute: GameActionAttribute::new(0x01A1),
    },
    // GameActionMagicRemoveSpellId.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionMagicRemoveSpellId.cs)
    ActionHandlerInfo {
        name: "RemoveSpellC2S",
        handler: crate::network::game_action::actions::game_action_magic_remove_spell_id::handle,
        attribute: GameActionAttribute::new(0x01A8),
    },
    // GameActionCancelAttack.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionCancelAttack.cs)
    ActionHandlerInfo {
        name: "CancelAttack",
        handler: crate::network::game_action::actions::game_action_cancel_attack::handle,
        attribute: GameActionAttribute::new(0x01B7),
    },
    // GameActionQueryHealth.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryHealth.cs)
    ActionHandlerInfo {
        name: "QueryHealth",
        handler: crate::network::game_action::actions::game_action_query_health::handle,
        attribute: GameActionAttribute::new(0x01BF),
    },
    // GameActionQueryAge.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryAge.cs)
    ActionHandlerInfo {
        name: "QueryAge",
        handler: crate::network::game_action::actions::game_action_query_age::handle,
        attribute: GameActionAttribute::new(0x01C2),
    },
    // GameActionQueryBirth.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryBirth.cs)
    ActionHandlerInfo {
        name: "QueryBirth",
        handler: crate::network::game_action::actions::game_action_query_birth::handle,
        attribute: GameActionAttribute::new(0x01C4),
    },
    // GameActionEmote.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionEmote.cs)
    ActionHandlerInfo {
        name: "Emote",
        handler: crate::network::game_action::actions::game_action_emote::handle,
        attribute: GameActionAttribute::new(0x01DF),
    },
    // GameActionSoulEmote.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSoulEmote.cs)
    ActionHandlerInfo {
        name: "SoulEmote",
        handler: crate::network::game_action::actions::game_action_soul_emote::handle,
        attribute: GameActionAttribute::new(0x01E1),
    },
    // GameActionAddSpellFavorite.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddSpellFavorite.cs)
    ActionHandlerInfo {
        name: "AddSpellFavorite",
        handler: crate::network::game_action::actions::game_action_add_spell_favorite::handle,
        attribute: GameActionAttribute::new(0x01E3),
    },
    // GameActionRemoveSpellFavorite.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveSpellFavorite.cs)
    ActionHandlerInfo {
        name: "RemoveSpellFavorite",
        handler: crate::network::game_action::actions::game_action_remove_spell_favorite::handle,
        attribute: GameActionAttribute::new(0x01E4),
    },
    // GameActionPingRequest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionPingRequest.cs)
    ActionHandlerInfo {
        name: "PingRequest",
        handler: crate::network::game_action::actions::game_action_ping_request::handle,
        attribute: GameActionAttribute::new(0x01E9),
    },
    // GameActionOpenTradeNegotiations.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionOpenTradeNegotiations.cs)
    ActionHandlerInfo {
        name: "OpenTradeNegotiations",
        handler: crate::network::game_action::actions::game_action_open_trade_negotiations::handle,
        attribute: GameActionAttribute::new(0x01F6),
    },
    // GameActionCloseTradeNegotiations.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionCloseTradeNegotiations.cs)
    ActionHandlerInfo {
        name: "CloseTradeNegotiations",
        handler: crate::network::game_action::actions::game_action_close_trade_negotiations::handle,
        attribute: GameActionAttribute::new(0x01F7),
    },
    // GameActionAddToTrade.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddToTrade.cs)
    ActionHandlerInfo {
        name: "AddToTrade",
        handler: crate::network::game_action::actions::game_action_add_to_trade::handle,
        attribute: GameActionAttribute::new(0x01F8),
    },
    // GameActionAcceptTrade.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAcceptTrade.cs)
    ActionHandlerInfo {
        name: "AcceptTrade",
        handler: crate::network::game_action::actions::game_action_accept_trade::handle,
        attribute: GameActionAttribute::new(0x01FA),
    },
    // GameActionDeclineTrade.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDeclineTrade.cs)
    ActionHandlerInfo {
        name: "DeclineTrade",
        handler: crate::network::game_action::actions::game_action_decline_trade::handle,
        attribute: GameActionAttribute::new(0x01FB),
    },
    // GameActionResetTrade.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionResetTrade.cs)
    ActionHandlerInfo {
        name: "ResetTrade",
        handler: crate::network::game_action::actions::game_action_reset_trade::handle,
        attribute: GameActionAttribute::new(0x0204),
    },
    // GameActionClearPlayerConsentList.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionClearPlayerConsentList.cs)
    ActionHandlerInfo {
        name: "ClearPlayerConsentList",
        handler: crate::network::game_action::actions::game_action_clear_player_consent_list::handle,
        attribute: GameActionAttribute::new(0x0216),
    },
    // GameActionDisplayPlayerConsentList.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDisplayPlayerConsentList.cs)
    ActionHandlerInfo {
        name: "DisplayPlayerConsentList",
        handler: crate::network::game_action::actions::game_action_display_player_consent_list::handle,
        attribute: GameActionAttribute::new(0x0217),
    },
    // GameActionRemoveFromPlayerConsentList.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveFromPlayerConsentList.cs)
    ActionHandlerInfo {
        name: "RemoveFromPlayerConsentList",
        handler: crate::network::game_action::actions::game_action_remove_from_player_consent_list::handle,
        attribute: GameActionAttribute::new(0x0218),
    },
    // GameActionAddPlayerPermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddPlayerPermission.cs)
    ActionHandlerInfo {
        name: "AddPlayerPermission",
        handler: crate::network::game_action::actions::game_action_add_player_permission::handle,
        attribute: GameActionAttribute::new(0x0219),
    },
    // GameActionRemovePlayerPermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemovePlayerPermission.cs)
    ActionHandlerInfo {
        name: "RemovePlayerPermission",
        handler: crate::network::game_action::actions::game_action_remove_player_permission::handle,
        attribute: GameActionAttribute::new(0x021A),
    },
    // GameActionHouseBuyHouse.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseBuyHouse.cs)
    ActionHandlerInfo {
        name: "BuyHouse",
        handler: crate::network::game_action::actions::game_action_house_buy_house::handle,
        attribute: GameActionAttribute::new(0x021C),
    },
    // GameActionHouseQuery.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseQuery.cs)
    ActionHandlerInfo {
        name: "HouseQuery",
        handler: crate::network::game_action::actions::game_action_house_query::handle,
        attribute: GameActionAttribute::new(0x021E),
    },
    // GameActionHouseAbandon.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAbandon.cs)
    ActionHandlerInfo {
        name: "AbandonHouse",
        handler: crate::network::game_action::actions::game_action_house_abandon::handle,
        attribute: GameActionAttribute::new(0x021F),
    },
    // GameActionHouseRentHouse.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRentHouse.cs)
    ActionHandlerInfo {
        name: "RentHouse",
        handler: crate::network::game_action::actions::game_action_house_rent_house::handle,
        attribute: GameActionAttribute::new(0x0221),
    },
    // GameActionSetDesiredComponentLevel.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetDesiredComponentLevel.cs)
    ActionHandlerInfo {
        name: "SetDesiredComponentLevel",
        handler: crate::network::game_action::actions::game_action_set_desired_component_level::handle,
        attribute: GameActionAttribute::new(0x0224),
    },
    // GameActionHouseAddPermanentGuest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddPermanentGuest.cs)
    ActionHandlerInfo {
        name: "AddPermanentGuest",
        handler: crate::network::game_action::actions::game_action_house_add_permanent_guest::handle,
        attribute: GameActionAttribute::new(0x0245),
    },
    // GameActionHouseRemovePermanentGuest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRemovePermanentGuest.cs)
    ActionHandlerInfo {
        name: "RemovePermanentGuest",
        handler: crate::network::game_action::actions::game_action_house_remove_permanent_guest::handle,
        attribute: GameActionAttribute::new(0x0246),
    },
    // GameActionHouseSetOpenStatus.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseSetOpenStatus.cs)
    ActionHandlerInfo {
        name: "SetOpenHouseStatus",
        handler: crate::network::game_action::actions::game_action_house_set_open_status::handle,
        attribute: GameActionAttribute::new(0x0247),
    },
    // GameActionHouseChangeStoragePermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseChangeStoragePermission.cs)
    ActionHandlerInfo {
        name: "ChangeStoragePermission",
        handler: crate::network::game_action::actions::game_action_house_change_storage_permission::handle,
        attribute: GameActionAttribute::new(0x0249),
    },
    // GameActionHouseBootSpecificGuest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseBootSpecificGuest.cs)
    ActionHandlerInfo {
        name: "BootSpecificHouseGuest",
        handler: crate::network::game_action::actions::game_action_house_boot_specific_guest::handle,
        attribute: GameActionAttribute::new(0x024A),
    },
    // GameActionHouseRemoveAllStoragePermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRemoveAllStoragePermission.cs)
    ActionHandlerInfo {
        name: "RemoveAllStoragePermission",
        handler: crate::network::game_action::actions::game_action_house_remove_all_storage_permission::handle,
        attribute: GameActionAttribute::new(0x024C),
    },
    // GameActionHouseRequestFullGuestList.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRequestFullGuestList.cs)
    ActionHandlerInfo {
        name: "RequestFullGuestList",
        handler: crate::network::game_action::actions::game_action_house_request_full_guest_list::handle,
        attribute: GameActionAttribute::new(0x024D),
    },
    // GameActionSetMotd.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSetMotd.cs)
    ActionHandlerInfo {
        name: "SetMotd",
        handler: crate::network::game_action::actions::game_action_set_motd::handle,
        attribute: GameActionAttribute::new(0x0254),
    },
    // GameActionQueryMotd.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryMotd.cs)
    ActionHandlerInfo {
        name: "QueryMotd",
        handler: crate::network::game_action::actions::game_action_query_motd::handle,
        attribute: GameActionAttribute::new(0x0255),
    },
    // GameActionClearMotd.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionClearMotd.cs)
    ActionHandlerInfo {
        name: "ClearMotd",
        handler: crate::network::game_action::actions::game_action_clear_motd::handle,
        attribute: GameActionAttribute::new(0x0256),
    },
    // GameActionHouseQueryLord.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseQueryLord.cs)
    ActionHandlerInfo {
        name: "QueryLord",
        handler: crate::network::game_action::actions::game_action_house_query_lord::handle,
        attribute: GameActionAttribute::new(0x0258),
    },
    // GameActionHouseAddAllStoragePermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddAllStoragePermission.cs)
    ActionHandlerInfo {
        name: "AddAllStoragePermission",
        handler: crate::network::game_action::actions::game_action_house_add_all_storage_permission::handle,
        attribute: GameActionAttribute::new(0x025C),
    },
    // GameActionHouseRemoveAllPermanentGuests.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseRemoveAllPermanentGuests.cs)
    ActionHandlerInfo {
        name: "RemoveAllPermanentGuests",
        handler: crate::network::game_action::actions::game_action_house_remove_all_permanent_guests::handle,
        attribute: GameActionAttribute::new(0x025E),
    },
    // GameActionHouseBootEveryone.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseBootEveryone.cs)
    ActionHandlerInfo {
        name: "BootEveryone",
        handler: crate::network::game_action::actions::game_action_house_boot_everyone::handle,
        attribute: GameActionAttribute::new(0x025F),
    },
    // GameActionTeleToHouse.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToHouse.cs)
    ActionHandlerInfo {
        name: "TeleToHouse",
        handler: crate::network::game_action::actions::game_action_tele_to_house::handle,
        attribute: GameActionAttribute::new(0x0262),
    },
    // GameActionQueryItemMana.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionQueryItemMana.cs)
    ActionHandlerInfo {
        name: "QueryItemMana",
        handler: crate::network::game_action::actions::game_action_query_item_mana::handle,
        attribute: GameActionAttribute::new(0x0263),
    },
    // GameActionHouseSetHooksVisibility.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseSetHooksVisibility.cs)
    ActionHandlerInfo {
        name: "SetHooksVisibility",
        handler: crate::network::game_action::actions::game_action_house_set_hooks_visibility::handle,
        attribute: GameActionAttribute::new(0x0266),
    },
    // GameActionHouseModifyAllegianceGuestPermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseModifyAllegianceGuestPermission.cs)
    ActionHandlerInfo {
        name: "ModifyAllegianceGuestPermission",
        handler: crate::network::game_action::actions::game_action_house_modify_allegiance_guest_permission::handle,
        attribute: GameActionAttribute::new(0x0267),
    },
    // GameActionHouseModifyAllegianceStoragePermission.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseModifyAllegianceStoragePermission.cs)
    ActionHandlerInfo {
        name: "ModifyAllegianceStoragePermission",
        handler: crate::network::game_action::actions::game_action_house_modify_allegiance_storage_permission::handle,
        attribute: GameActionAttribute::new(0x0268),
    },
    // GameActionChessJoin.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChessJoin.cs)
    ActionHandlerInfo {
        name: "ChessJoin",
        handler: crate::network::game_action::actions::game_action_chess_join::handle,
        attribute: GameActionAttribute::new(0x0269),
    },
    // GameActionChessQuit.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChessQuit.cs)
    ActionHandlerInfo {
        name: "ChessQuit",
        handler: crate::network::game_action::actions::game_action_chess_quit::handle,
        attribute: GameActionAttribute::new(0x026A),
    },
    // GameActionChessMove.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChessMove.cs)
    ActionHandlerInfo {
        name: "ChessMove",
        handler: crate::network::game_action::actions::game_action_chess_move::handle,
        attribute: GameActionAttribute::new(0x026B),
    },
    // GameActionChessMovePass.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChessMovePass.cs)
    ActionHandlerInfo {
        name: "ChessMovePass",
        handler: crate::network::game_action::actions::game_action_chess_move_pass::handle,
        attribute: GameActionAttribute::new(0x026D),
    },
    // GameActionChessStalemate.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionChessStalemate.cs)
    ActionHandlerInfo {
        name: "ChessStalemate",
        handler: crate::network::game_action::actions::game_action_chess_stalemate::handle,
        attribute: GameActionAttribute::new(0x026E),
    },
    // GameActionHouseListAvailable.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionHouseListAvailable.cs)
    ActionHandlerInfo {
        name: "ListAvailableHouses",
        handler: crate::network::game_action::actions::game_action_house_list_available::handle,
        attribute: GameActionAttribute::new(0x0270),
    },
    // GameActionConfirmationResponse.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionConfirmationResponse.cs)
    ActionHandlerInfo {
        name: "ConfirmationResponse",
        handler: crate::network::game_action::actions::game_action_confirmation_response::handle,
        attribute: GameActionAttribute::new(0x0275),
    },
    // GameActionBreakAllegianceBoot.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionBreakAllegianceBoot.cs)
    ActionHandlerInfo {
        name: "BreakAllegianceBoot",
        handler: crate::network::game_action::actions::game_action_break_allegiance_boot::handle,
        attribute: GameActionAttribute::new(0x0277),
    },
    // GameActionTeleToMansion.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToMansion.cs)
    ActionHandlerInfo {
        name: "TeleToMansion",
        handler: crate::network::game_action::actions::game_action_tele_to_mansion::handle,
        attribute: GameActionAttribute::new(0x0278),
    },
    // GameActionDie.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionDie.cs)
    ActionHandlerInfo {
        name: "Suicide",
        handler: crate::network::game_action::actions::game_action_die::handle,
        attribute: GameActionAttribute::new(0x0279),
    },
    // GameActionAllegianceInfoRequest.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceInfoRequest.cs)
    ActionHandlerInfo {
        name: "AllegianceInfoRequest",
        handler: crate::network::game_action::actions::game_action_allegiance_info_request::handle,
        attribute: GameActionAttribute::new(0x027B),
    },
    // GameActionCreateTinkeringTool.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionCreateTinkeringTool.cs)
    ActionHandlerInfo {
        name: "CreateTinkeringTool",
        handler: crate::network::game_action::actions::game_action_create_tinkering_tool::handle,
        attribute: GameActionAttribute::new(0x027D),
    },
    // GameActionSpellbookFilter.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionSpellbookFilter.cs)
    ActionHandlerInfo {
        name: "SpellbookFilter",
        handler: crate::network::game_action::actions::game_action_spellbook_filter::handle,
        attribute: GameActionAttribute::new(0x0286),
    },
    // GameActionTeleToMarketPlace.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionTeleToMarketplace.cs)
    ActionHandlerInfo {
        name: "TeleToMarketPlace",
        handler: crate::network::game_action::actions::game_action_tele_to_marketplace::handle,
        attribute: GameActionAttribute::new(0x028D),
    },
    // GameActionEnterPkLite.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionEnterPkLite.cs)
    ActionHandlerInfo {
        name: "EnterPkLite",
        handler: crate::network::game_action::actions::game_action_enter_pk_lite::handle,
        attribute: GameActionAttribute::new(0x028F),
    },
    // GameActionFellowshipAssignNewLeader.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipAssignNewLeader.cs)
    ActionHandlerInfo {
        name: "FellowshipAssignNewLeader",
        handler: crate::network::game_action::actions::game_action_fellowship_assign_new_leader::handle,
        attribute: GameActionAttribute::new(0x0290),
    },
    // GameActionFellowshipChangeOpenness.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFellowshipChangeOpenness.cs)
    ActionHandlerInfo {
        name: "FellowshipChangeOpenness",
        handler: crate::network::game_action::actions::game_action_fellowship_change_openness::handle,
        attribute: GameActionAttribute::new(0x0291),
    },
    // GameActionAllegianceChatBoot.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceChatBoot.cs)
    ActionHandlerInfo {
        name: "AllegianceChatBoot",
        handler: crate::network::game_action::actions::game_action_allegiance_chat_boot::handle,
        attribute: GameActionAttribute::new(0x02A0),
    },
    // GameActionAddAllegianceBan.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAddAllegianceBan.cs)
    ActionHandlerInfo {
        name: "AddAllegianceBan",
        handler: crate::network::game_action::actions::game_action_add_allegiance_ban::handle,
        attribute: GameActionAttribute::new(0x02A1),
    },
    // GameActionRemoveAllegianceBan.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveAllegianceBan.cs)
    ActionHandlerInfo {
        name: "RemoveAllegianceBan",
        handler: crate::network::game_action::actions::game_action_remove_allegiance_ban::handle,
        attribute: GameActionAttribute::new(0x02A2),
    },
    // GameActionListAllegianceBans.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionListAllegianceBans.cs)
    ActionHandlerInfo {
        name: "ListAllegianceBans",
        handler: crate::network::game_action::actions::game_action_list_allegiance_bans::handle,
        attribute: GameActionAttribute::new(0x02A3),
    },
    // GameActionRemoveAllegianceOfficer.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveAllegianceOfficer.cs)
    ActionHandlerInfo {
        name: "RemoveAllegianceOfficer",
        handler: crate::network::game_action::actions::game_action_remove_allegiance_officer::handle,
        attribute: GameActionAttribute::new(0x02A5),
    },
    // GameActionListAllegianceOfficers.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionListAllegianceOfficers.cs)
    ActionHandlerInfo {
        name: "ListAllegianceOfficers",
        handler: crate::network::game_action::actions::game_action_list_allegiance_officers::handle,
        attribute: GameActionAttribute::new(0x02A6),
    },
    // GameActionClearAllegianceOfficers.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionClearAllegianceOfficers.cs)
    ActionHandlerInfo {
        name: "ClearAllegianceOfficers",
        handler: crate::network::game_action::actions::game_action_clear_allegiance_officers::handle,
        attribute: GameActionAttribute::new(0x02A7),
    },
    // GameActionRecallAllegianceHometown.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionRecallAllegianceHometown.cs)
    ActionHandlerInfo {
        name: "RecallAllegianceHometown",
        handler: crate::network::game_action::actions::game_action_recall_allegiance_hometown::handle,
        attribute: GameActionAttribute::new(0x02AB),
    },
    // GameActionFinishBarber.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionFinishBarber.cs)
    ActionHandlerInfo {
        name: "FinishBarber",
        handler: crate::network::game_action::actions::game_action_finish_barber::handle,
        attribute: GameActionAttribute::new(0x0311),
    },
    // GameActionAbandonContract.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAbandonContract.cs)
    ActionHandlerInfo {
        name: "AbandonContract",
        handler: crate::network::game_action::actions::game_action_abandon_contract::handle,
        attribute: GameActionAttribute::new(0x0316),
    },
    // GameActionJump.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionJump.cs)
    ActionHandlerInfo {
        name: "Jump",
        handler: crate::network::game_action::actions::game_action_jump::handle,
        attribute: GameActionAttribute::new(0xF61B),
    },
    // GameActionMoveToState.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionMoveToState.cs)
    ActionHandlerInfo {
        name: "MoveToState",
        handler: crate::network::game_action::actions::game_action_move_to_state::handle,
        attribute: GameActionAttribute::new(0xF61C),
    },
    // GameActionAutonomousPosition.Handle (Source/ACE.Server/Network/GameAction/Actions/GameActionAutonomousPosition.cs)
    ActionHandlerInfo {
        name: "AutonomousPosition",
        handler: crate::network::game_action::actions::game_action_autonomous_position::handle,
        attribute: GameActionAttribute::new(0xF753),
    },
];
