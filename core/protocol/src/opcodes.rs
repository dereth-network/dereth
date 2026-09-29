//! Every opcode this client knows.
//!
//! **This file is the authority** for the opcode table: value, name, direction and the send and
//! receive queues of every message. It is maintained by hand. It was built from the client's own
//! switch statements and senders — not from the community catalogue — and the specification
//! (`docs/networking/messages/00-dispatch-and-queues.md` §7) points here rather than repeating it.
//! A new or corrected row goes into [`OPCODES`], which must stay sorted by value; a constant goes
//! into `impl Opcode`. Four constants have no [`OPCODES`] row, so [`Opcode::name`] answers `None`
//! for them as for any opcode the client does not know: `0x00B5` and `0x01C8` name opcodes the
//! retail server sends and the client has no handler for, and `0x0317` and `0x0318` are string
//! events the community catalogue leaves unnamed.
//!
//! `0xF7B0` and `0xF7B1` are *headers*, not message types; the real type is the dword that
//! follows. The master table follows the community convention of listing the sub-types as
//! opcodes in their own right, and so does this file. See [`crate::order`].

use dereth_primitives::NetQueue;

/// A message type: the first dword of a blob, or of an ordered blob's body.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Opcode(pub u32);

impl core::fmt::Debug for Opcode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.name() {
            Some(n) => write!(f, "Opcode(0x{:04X} {})", self.0, n),
            None => write!(f, "Opcode(0x{:04X})", self.0),
        }
    }
}

/// Which way a message travels. A handful travel both ways under one opcode.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Direction {
    /// Server to client only.
    S2C,
    /// Client to server only.
    C2S,
    /// Both, with different bodies.
    Both,
}

/// One row of the master opcode table.
#[derive(Copy, Clone, Debug)]
pub struct OpcodeInfo {
    pub opcode: Opcode,
    /// The community catalogue's name, which the knowledge base indexes by.
    pub name: &'static str,
    pub direction: Direction,
    /// The queue the client **sends** this opcode on, when it sends it at all.
    pub send_queue: Option<NetQueue>,
    /// The queue the client **receives** this opcode on, when it receives it at all.
    ///
    /// The two differ for the handful of opcodes that travel both ways: the client sends
    /// `0xF653 Login_ExecuteLogOff` on the Logon queue and receives it on the UI queue,
    /// because the login server's replies all come back on the world server's UI queue.
    pub recv_queue: Option<NetQueue>,
}

impl Opcode {
    /// `0x0003` Allegiance_AllegianceUpdateAborted
    pub const ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED: Self = Self(0x0003);
    /// `0x0004` Communication_PopUpString
    pub const COMMUNICATION_POP_UP_STRING: Self = Self(0x0004);
    /// `0x0005` Character_PlayerOptionChangedEvent
    pub const CHARACTER_PLAYER_OPTION_CHANGED_EVENT: Self = Self(0x0005);
    /// `0x0008` Combat_TargetedMeleeAttack
    pub const COMBAT_TARGETED_MELEE_ATTACK: Self = Self(0x0008);
    /// `0x000A` Combat_TargetedMissileAttack
    pub const COMBAT_TARGETED_MISSILE_ATTACK: Self = Self(0x000A);
    /// `0x000F` Communication_SetAFKMode
    pub const COMMUNICATION_SET_AFKMODE: Self = Self(0x000F);
    /// `0x0010` Communication_SetAFKMessage
    pub const COMMUNICATION_SET_AFKMESSAGE: Self = Self(0x0010);
    /// `0x0013` Login_PlayerDescription
    pub const LOGIN_PLAYER_DESCRIPTION: Self = Self(0x0013);
    /// `0x0015` Communication_Talk
    pub const COMMUNICATION_TALK: Self = Self(0x0015);
    /// `0x0017` Social_RemoveFriend
    pub const SOCIAL_REMOVE_FRIEND: Self = Self(0x0017);
    /// `0x0018` Social_AddFriend
    pub const SOCIAL_ADD_FRIEND: Self = Self(0x0018);
    /// `0x0019` Inventory_PutItemInContainer
    pub const INVENTORY_PUT_ITEM_IN_CONTAINER: Self = Self(0x0019);
    /// `0x001A` Inventory_GetAndWieldItem
    pub const INVENTORY_GET_AND_WIELD_ITEM: Self = Self(0x001A);
    /// `0x001B` Inventory_DropItem
    pub const INVENTORY_DROP_ITEM: Self = Self(0x001B);
    /// `0x001D` Allegiance_SwearAllegiance
    pub const ALLEGIANCE_SWEAR_ALLEGIANCE: Self = Self(0x001D);
    /// `0x001E` Allegiance_BreakAllegiance
    pub const ALLEGIANCE_BREAK_ALLEGIANCE: Self = Self(0x001E);
    /// `0x001F` Allegiance_UpdateRequest
    pub const ALLEGIANCE_UPDATE_REQUEST: Self = Self(0x001F);
    /// `0x0020` Allegiance_AllegianceUpdate
    pub const ALLEGIANCE_ALLEGIANCE_UPDATE: Self = Self(0x0020);
    /// `0x01C8` Allegiance_AllegianceUpdateDone: the retail server sends it; the client has no
    /// handler and drops it.
    pub const ALLEGIANCE_ALLEGIANCE_UPDATE_DONE: Self = Self(0x01C8);
    /// `0x0021` Social_FriendsUpdate
    pub const SOCIAL_FRIENDS_UPDATE: Self = Self(0x0021);
    /// `0x0022` Item_ServerSaysContainID
    pub const ITEM_SERVER_SAYS_CONTAIN_ID: Self = Self(0x0022);
    /// `0x0023` Item_WearItem
    pub const ITEM_WEAR_ITEM: Self = Self(0x0023);
    /// `0x0024` Item_ServerSaysRemove
    pub const ITEM_SERVER_SAYS_REMOVE: Self = Self(0x0024);
    /// `0x0025` Social_ClearFriends
    pub const SOCIAL_CLEAR_FRIENDS: Self = Self(0x0025);
    /// `0x0026` Character_TeleToPKLArena
    pub const CHARACTER_TELE_TO_PKLARENA: Self = Self(0x0026);
    /// `0x0027` Character_TeleToPKArena
    pub const CHARACTER_TELE_TO_PKARENA: Self = Self(0x0027);
    /// `0x0029` Social_CharacterTitleTable
    pub const SOCIAL_CHARACTER_TITLE_TABLE: Self = Self(0x0029);
    /// `0x002B` Social_AddOrSetCharacterTitle
    pub const SOCIAL_ADD_OR_SET_CHARACTER_TITLE: Self = Self(0x002B);
    /// `0x002C` Social_SetDisplayCharacterTitle
    pub const SOCIAL_SET_DISPLAY_CHARACTER_TITLE: Self = Self(0x002C);
    /// `0x0030` Allegiance_QueryAllegianceName
    pub const ALLEGIANCE_QUERY_ALLEGIANCE_NAME: Self = Self(0x0030);
    /// `0x0031` Allegiance_ClearAllegianceName
    pub const ALLEGIANCE_CLEAR_ALLEGIANCE_NAME: Self = Self(0x0031);
    /// `0x0032` Communication_TalkDirect
    pub const COMMUNICATION_TALK_DIRECT: Self = Self(0x0032);
    /// `0x0033` Allegiance_SetAllegianceName
    pub const ALLEGIANCE_SET_ALLEGIANCE_NAME: Self = Self(0x0033);
    /// `0x0035` Inventory_UseWithTargetEvent
    pub const INVENTORY_USE_WITH_TARGET_EVENT: Self = Self(0x0035);
    /// `0x0036` Inventory_UseEvent
    pub const INVENTORY_USE_EVENT: Self = Self(0x0036);
    /// `0x003B` Allegiance_SetAllegianceOfficer
    pub const ALLEGIANCE_SET_ALLEGIANCE_OFFICER: Self = Self(0x003B);
    /// `0x003C` Allegiance_SetAllegianceOfficerTitle
    pub const ALLEGIANCE_SET_ALLEGIANCE_OFFICER_TITLE: Self = Self(0x003C);
    /// `0x003D` Allegiance_ListAllegianceOfficerTitles
    pub const ALLEGIANCE_LIST_ALLEGIANCE_OFFICER_TITLES: Self = Self(0x003D);
    /// `0x003E` Allegiance_ClearAllegianceOfficerTitles
    pub const ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICER_TITLES: Self = Self(0x003E);
    /// `0x003F` Allegiance_DoAllegianceLockAction
    pub const ALLEGIANCE_DO_ALLEGIANCE_LOCK_ACTION: Self = Self(0x003F);
    /// `0x0040` Allegiance_SetAllegianceApprovedVassal
    pub const ALLEGIANCE_SET_ALLEGIANCE_APPROVED_VASSAL: Self = Self(0x0040);
    /// `0x0041` Allegiance_AllegianceChatGag
    pub const ALLEGIANCE_ALLEGIANCE_CHAT_GAG: Self = Self(0x0041);
    /// `0x0042` Allegiance_DoAllegianceHouseAction
    pub const ALLEGIANCE_DO_ALLEGIANCE_HOUSE_ACTION: Self = Self(0x0042);
    /// `0x0044` Train_TrainAttribute2nd
    pub const TRAIN_TRAIN_ATTRIBUTE2ND: Self = Self(0x0044);
    /// `0x0045` Train_TrainAttribute
    pub const TRAIN_TRAIN_ATTRIBUTE: Self = Self(0x0045);
    /// `0x0046` Train_TrainSkill
    pub const TRAIN_TRAIN_SKILL: Self = Self(0x0046);
    /// `0x0047` Train_TrainSkillAdvancementClass
    pub const TRAIN_TRAIN_SKILL_ADVANCEMENT_CLASS: Self = Self(0x0047);
    /// `0x0048` Magic_CastUntargetedSpell
    pub const MAGIC_CAST_UNTARGETED_SPELL: Self = Self(0x0048);
    /// `0x004A` Magic_CastTargetedSpell
    pub const MAGIC_CAST_TARGETED_SPELL: Self = Self(0x004A);
    /// `0x0052` Item_StopViewingObjectContents
    pub const ITEM_STOP_VIEWING_OBJECT_CONTENTS: Self = Self(0x0052);
    /// `0x0053` Combat_ChangeCombatMode
    pub const COMBAT_CHANGE_COMBAT_MODE: Self = Self(0x0053);
    /// `0x0054` Inventory_StackableMerge
    pub const INVENTORY_STACKABLE_MERGE: Self = Self(0x0054);
    /// `0x0055` Inventory_StackableSplitToContainer
    pub const INVENTORY_STACKABLE_SPLIT_TO_CONTAINER: Self = Self(0x0055);
    /// `0x0056` Inventory_StackableSplitTo3D
    pub const INVENTORY_STACKABLE_SPLIT_TO3_D: Self = Self(0x0056);
    /// `0x0058` Communication_ModifyCharacterSquelch
    pub const COMMUNICATION_MODIFY_CHARACTER_SQUELCH: Self = Self(0x0058);
    /// `0x0059` Communication_ModifyAccountSquelch
    pub const COMMUNICATION_MODIFY_ACCOUNT_SQUELCH: Self = Self(0x0059);
    /// `0x005B` Communication_ModifyGlobalSquelch
    pub const COMMUNICATION_MODIFY_GLOBAL_SQUELCH: Self = Self(0x005B);
    /// `0x005D` Communication_TalkDirectByName
    pub const COMMUNICATION_TALK_DIRECT_BY_NAME: Self = Self(0x005D);
    /// `0x005F` Vendor_Buy
    pub const VENDOR_BUY: Self = Self(0x005F);
    /// `0x0060` Vendor_Sell
    pub const VENDOR_SELL: Self = Self(0x0060);
    /// `0x0062` Vendor_VendorInfo
    pub const VENDOR_VENDOR_INFO: Self = Self(0x0062);
    /// `0x0063` Character_TeleToLifestone
    pub const CHARACTER_TELE_TO_LIFESTONE: Self = Self(0x0063);
    /// `0x0075` Character_StartBarber
    pub const CHARACTER_START_BARBER: Self = Self(0x0075);
    /// `0x00A0` Character_ServerSaysAttemptFailed
    pub const CHARACTER_SERVER_SAYS_ATTEMPT_FAILED: Self = Self(0x00A0);
    /// `0x00A1` Character_LoginCompleteNotification
    pub const CHARACTER_LOGIN_COMPLETE_NOTIFICATION: Self = Self(0x00A1);
    /// `0x00A2` Fellowship_Create
    pub const FELLOWSHIP_CREATE: Self = Self(0x00A2);
    /// `0x00A3` Fellowship_Quit
    pub const FELLOWSHIP_QUIT: Self = Self(0x00A3);
    /// `0x00A4` Fellowship_Dismiss
    pub const FELLOWSHIP_DISMISS: Self = Self(0x00A4);
    /// `0x00A5` Fellowship_Recruit
    pub const FELLOWSHIP_RECRUIT: Self = Self(0x00A5);
    /// `0x00A6` Fellowship_UpdateRequest
    pub const FELLOWSHIP_UPDATE_REQUEST: Self = Self(0x00A6);
    /// `0x00AC` Writing_BookAddPage
    pub const WRITING_BOOK_ADD_PAGE: Self = Self(0x00AC);
    /// `0x00AB` Writing_BookModifyPage
    pub const WRITING_BOOK_MODIFY_PAGE: Self = Self(0x00AB);
    /// `0x00AA` Writing_BookData
    pub const WRITING_BOOK_DATA: Self = Self(0x00AA);
    /// `0x00AD` Writing_BookDeletePage
    pub const WRITING_BOOK_DELETE_PAGE: Self = Self(0x00AD);
    /// `0x00AE` Writing_BookPageData
    pub const WRITING_BOOK_PAGE_DATA: Self = Self(0x00AE);
    /// `0x00B4` Writing_BookOpen
    pub const WRITING_BOOK_OPEN: Self = Self(0x00B4);
    /// `0x00B5` Writing_BookModifyPageResponse: the retail server sends it; the client has no
    /// handler and drops it.
    pub const WRITING_BOOK_MODIFY_PAGE_RESPONSE: Self = Self(0x00B5);
    /// `0x00B6` Writing_BookAddPageResponse
    pub const WRITING_BOOK_ADD_PAGE_RESPONSE: Self = Self(0x00B6);
    /// `0x00B7` Writing_BookDeletePageResponse
    pub const WRITING_BOOK_DELETE_PAGE_RESPONSE: Self = Self(0x00B7);
    /// `0x00B8` Writing_BookPageDataResponse
    pub const WRITING_BOOK_PAGE_DATA_RESPONSE: Self = Self(0x00B8);
    /// `0x00BF` Writing_SetInscription
    pub const WRITING_SET_INSCRIPTION: Self = Self(0x00BF);
    /// `0x00C3` Item_GetInscriptionResponse
    pub const ITEM_GET_INSCRIPTION_RESPONSE: Self = Self(0x00C3);
    /// `0x00C8` Item_Appraise
    pub const ITEM_APPRAISE: Self = Self(0x00C8);
    /// `0x00C9` Item_SetAppraiseInfo
    pub const ITEM_SET_APPRAISE_INFO: Self = Self(0x00C9);
    /// `0x00CD` Inventory_GiveObjectRequest
    pub const INVENTORY_GIVE_OBJECT_REQUEST: Self = Self(0x00CD);
    /// `0x00D6` Advocate_Teleport
    pub const ADVOCATE_TELEPORT: Self = Self(0x00D6);
    /// `0x0140` Character_AbuseLogRequest
    pub const CHARACTER_ABUSE_LOG_REQUEST: Self = Self(0x0140);
    /// `0x0145` Communication_AddToChannel
    pub const COMMUNICATION_ADD_TO_CHANNEL: Self = Self(0x0145);
    /// `0x0146` Communication_RemoveFromChannel
    pub const COMMUNICATION_REMOVE_FROM_CHANNEL: Self = Self(0x0146);
    /// `0x0147` Communication_ChannelBroadcast
    pub const COMMUNICATION_CHANNEL_BROADCAST: Self = Self(0x0147);
    /// `0x0148` Communication_ChannelList
    pub const COMMUNICATION_CHANNEL_LIST: Self = Self(0x0148);
    /// `0x0149` Communication_ChannelIndex
    pub const COMMUNICATION_CHANNEL_INDEX: Self = Self(0x0149);
    /// `0x0195` Inventory_NoLongerViewingContents
    pub const INVENTORY_NO_LONGER_VIEWING_CONTENTS: Self = Self(0x0195);
    /// `0x0196` Item_OnViewContents
    pub const ITEM_ON_VIEW_CONTENTS: Self = Self(0x0196);
    /// `0x0197` Item_UpdateStackSize
    pub const ITEM_UPDATE_STACK_SIZE: Self = Self(0x0197);
    /// `0x019A` Item_ServerSaysMoveItem
    pub const ITEM_SERVER_SAYS_MOVE_ITEM: Self = Self(0x019A);
    /// `0x019B` Inventory_StackableSplitToWield
    pub const INVENTORY_STACKABLE_SPLIT_TO_WIELD: Self = Self(0x019B);
    /// `0x019C` Character_AddShortCut
    pub const CHARACTER_ADD_SHORT_CUT: Self = Self(0x019C);
    /// `0x019D` Character_RemoveShortCut
    pub const CHARACTER_REMOVE_SHORT_CUT: Self = Self(0x019D);
    /// `0x019E` Combat_HandlePlayerDeathEvent
    pub const COMBAT_HANDLE_PLAYER_DEATH_EVENT: Self = Self(0x019E);
    /// `0x01A1` Character_CharacterOptionsEvent
    pub const CHARACTER_CHARACTER_OPTIONS_EVENT: Self = Self(0x01A1);
    /// `0x01A7` Combat_HandleAttackDoneEvent
    pub const COMBAT_HANDLE_ATTACK_DONE_EVENT: Self = Self(0x01A7);
    /// `0x01A8` Magic_RemoveSpell
    pub const MAGIC_REMOVE_SPELL: Self = Self(0x01A8);
    /// `0x01AC` Combat_HandleVictimNotificationEventSelf
    pub const COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF: Self = Self(0x01AC);
    /// `0x01AD` Combat_HandleVictimNotificationEventOther
    pub const COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER: Self = Self(0x01AD);
    /// `0x01B1` Combat_HandleAttackerNotificationEvent
    pub const COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT: Self = Self(0x01B1);
    /// `0x01B2` Combat_HandleDefenderNotificationEvent
    pub const COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT: Self = Self(0x01B2);
    /// `0x01B3` Combat_HandleEvasionAttackerNotificationEvent
    pub const COMBAT_HANDLE_EVASION_ATTACKER_NOTIFICATION_EVENT: Self = Self(0x01B3);
    /// `0x01B4` Combat_HandleEvasionDefenderNotificationEvent
    pub const COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT: Self = Self(0x01B4);
    /// `0x01B7` Combat_CancelAttack
    pub const COMBAT_CANCEL_ATTACK: Self = Self(0x01B7);
    /// `0x01B8` Combat_HandleCommenceAttackEvent
    pub const COMBAT_HANDLE_COMMENCE_ATTACK_EVENT: Self = Self(0x01B8);
    /// `0x01BF` Combat_QueryHealth
    pub const COMBAT_QUERY_HEALTH: Self = Self(0x01BF);
    /// `0x01C0` Combat_QueryHealthResponse
    pub const COMBAT_QUERY_HEALTH_RESPONSE: Self = Self(0x01C0);
    /// `0x01C2` Character_QueryAge
    pub const CHARACTER_QUERY_AGE: Self = Self(0x01C2);
    /// `0x01C3` Character_QueryAgeResponse
    pub const CHARACTER_QUERY_AGE_RESPONSE: Self = Self(0x01C3);
    /// `0x01C4` Character_QueryBirth
    pub const CHARACTER_QUERY_BIRTH: Self = Self(0x01C4);
    /// `0x01C7` Item_UseDone
    pub const ITEM_USE_DONE: Self = Self(0x01C7);
    /// `0x01C9` Fellowship_FellowUpdateDone
    pub const FELLOWSHIP_FELLOW_UPDATE_DONE: Self = Self(0x01C9);
    /// `0x01CA` Fellowship_FellowStatsDone
    pub const FELLOWSHIP_FELLOW_STATS_DONE: Self = Self(0x01CA);
    /// `0x01CB` Item_AppraiseDone
    pub const ITEM_APPRAISE_DONE: Self = Self(0x01CB);
    /// `0x01D1` Qualities_PrivateRemoveIntEvent
    pub const QUALITIES_PRIVATE_REMOVE_INT_EVENT: Self = Self(0x01D1);
    /// `0x01D2` Qualities_RemoveIntEvent
    pub const QUALITIES_REMOVE_INT_EVENT: Self = Self(0x01D2);
    /// `0x01D3` Qualities_PrivateRemoveBoolEvent
    pub const QUALITIES_PRIVATE_REMOVE_BOOL_EVENT: Self = Self(0x01D3);
    /// `0x01D4` Qualities_RemoveBoolEvent
    pub const QUALITIES_REMOVE_BOOL_EVENT: Self = Self(0x01D4);
    /// `0x01D5` Qualities_PrivateRemoveFloatEvent
    pub const QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT: Self = Self(0x01D5);
    /// `0x01D6` Qualities_RemoveFloatEvent
    pub const QUALITIES_REMOVE_FLOAT_EVENT: Self = Self(0x01D6);
    /// `0x01D7` Qualities_PrivateRemoveStringEvent
    pub const QUALITIES_PRIVATE_REMOVE_STRING_EVENT: Self = Self(0x01D7);
    /// `0x01D8` Qualities_RemoveStringEvent
    pub const QUALITIES_REMOVE_STRING_EVENT: Self = Self(0x01D8);
    /// `0x01D9` Qualities_PrivateRemoveDataIDEvent
    pub const QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT: Self = Self(0x01D9);
    /// `0x01DA` Qualities_RemoveDataIDEvent
    pub const QUALITIES_REMOVE_DATA_IDEVENT: Self = Self(0x01DA);
    /// `0x01DB` Qualities_PrivateRemoveInstanceIDEvent
    pub const QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT: Self = Self(0x01DB);
    /// `0x01DC` Qualities_RemoveInstanceIDEvent
    pub const QUALITIES_REMOVE_INSTANCE_IDEVENT: Self = Self(0x01DC);
    /// `0x01DD` Qualities_PrivateRemovePositionEvent
    pub const QUALITIES_PRIVATE_REMOVE_POSITION_EVENT: Self = Self(0x01DD);
    /// `0x01DE` Qualities_RemovePositionEvent
    pub const QUALITIES_REMOVE_POSITION_EVENT: Self = Self(0x01DE);
    /// `0x01DF` Communication_Emote
    pub const COMMUNICATION_EMOTE: Self = Self(0x01DF);
    /// `0x01E0` Communication_HearEmote
    pub const COMMUNICATION_HEAR_EMOTE: Self = Self(0x01E0);
    /// `0x01E1` Communication_SoulEmote
    pub const COMMUNICATION_SOUL_EMOTE: Self = Self(0x01E1);
    /// `0x01E2` Communication_HearSoulEmote
    pub const COMMUNICATION_HEAR_SOUL_EMOTE: Self = Self(0x01E2);
    /// `0x01E3` Character_AddSpellFavorite
    pub const CHARACTER_ADD_SPELL_FAVORITE: Self = Self(0x01E3);
    /// `0x01E4` Character_RemoveSpellFavorite
    pub const CHARACTER_REMOVE_SPELL_FAVORITE: Self = Self(0x01E4);
    /// `0x01E9` Character_RequestPing
    pub const CHARACTER_REQUEST_PING: Self = Self(0x01E9);
    /// `0x01EA` Character_ReturnPing
    pub const CHARACTER_RETURN_PING: Self = Self(0x01EA);
    /// `0x01F4` Communication_SetSquelchDB
    pub const COMMUNICATION_SET_SQUELCH_DB: Self = Self(0x01F4);
    /// `0x01F6` Trade_OpenTradeNegotiations
    pub const TRADE_OPEN_TRADE_NEGOTIATIONS: Self = Self(0x01F6);
    /// `0x01F7` Trade_CloseTradeNegotiations
    pub const TRADE_CLOSE_TRADE_NEGOTIATIONS: Self = Self(0x01F7);
    /// `0x01F8` Trade_AddToTrade
    pub const TRADE_ADD_TO_TRADE: Self = Self(0x01F8);
    /// `0x01FA` Trade_AcceptTrade
    pub const TRADE_ACCEPT_TRADE: Self = Self(0x01FA);
    /// `0x01FB` Trade_DeclineTrade
    pub const TRADE_DECLINE_TRADE: Self = Self(0x01FB);
    /// `0x01FD` Trade_RegisterTrade
    pub const TRADE_REGISTER_TRADE: Self = Self(0x01FD);
    /// `0x01FE` Trade_OpenTrade
    pub const TRADE_OPEN_TRADE: Self = Self(0x01FE);
    /// `0x01FF` Trade_CloseTrade
    pub const TRADE_CLOSE_TRADE: Self = Self(0x01FF);
    /// `0x0200` Trade_AddToTrade_Recv
    pub const TRADE_ADD_TO_TRADE_RECV: Self = Self(0x0200);
    /// `0x0201` Trade_RemoveFromTrade
    pub const TRADE_REMOVE_FROM_TRADE: Self = Self(0x0201);
    /// `0x0202` Trade_AcceptTrade_Recv
    pub const TRADE_ACCEPT_TRADE_RECV: Self = Self(0x0202);
    /// `0x0203` Trade_DeclineTrade_Recv
    pub const TRADE_DECLINE_TRADE_RECV: Self = Self(0x0203);
    /// `0x0204` Trade_ResetTrade
    pub const TRADE_RESET_TRADE: Self = Self(0x0204);
    /// `0x0205` Trade_ResetTrade_Recv
    pub const TRADE_RESET_TRADE_RECV: Self = Self(0x0205);
    /// `0x0207` Trade_TradeFailure
    pub const TRADE_TRADE_FAILURE: Self = Self(0x0207);
    /// `0x0208` Trade_ClearTradeAcceptance
    pub const TRADE_CLEAR_TRADE_ACCEPTANCE: Self = Self(0x0208);
    /// `0x0216` Character_ClearPlayerConsentList
    pub const CHARACTER_CLEAR_PLAYER_CONSENT_LIST: Self = Self(0x0216);
    /// `0x0217` Character_DisplayPlayerConsentList
    pub const CHARACTER_DISPLAY_PLAYER_CONSENT_LIST: Self = Self(0x0217);
    /// `0x0218` Character_RemoveFromPlayerConsentList
    pub const CHARACTER_REMOVE_FROM_PLAYER_CONSENT_LIST: Self = Self(0x0218);
    /// `0x0219` Character_AddPlayerPermission
    pub const CHARACTER_ADD_PLAYER_PERMISSION: Self = Self(0x0219);
    /// `0x021A` Character_RemovePlayerPermission
    pub const CHARACTER_REMOVE_PLAYER_PERMISSION: Self = Self(0x021A);
    /// `0x021C` House_BuyHouse
    pub const HOUSE_BUY_HOUSE: Self = Self(0x021C);
    /// `0x021D` House_HouseProfile
    pub const HOUSE_HOUSE_PROFILE: Self = Self(0x021D);
    /// `0x021E` House_QueryHouse
    pub const HOUSE_QUERY_HOUSE: Self = Self(0x021E);
    /// `0x021F` House_AbandonHouse
    pub const HOUSE_ABANDON_HOUSE: Self = Self(0x021F);
    /// `0x0220` Character_RemovePlayerPermission_UnusedCatalogueOpcode
    pub const CHARACTER_REMOVE_PLAYER_PERMISSION_UNUSED_CATALOGUE_OPCODE: Self = Self(0x0220);
    /// `0x0221` House_RentHouse
    pub const HOUSE_RENT_HOUSE: Self = Self(0x0221);
    /// `0x0224` Character_SetDesiredComponentLevel
    pub const CHARACTER_SET_DESIRED_COMPONENT_LEVEL: Self = Self(0x0224);
    /// `0x0225` House_HouseData
    pub const HOUSE_HOUSE_DATA: Self = Self(0x0225);
    /// `0x0226` House_HouseStatus
    pub const HOUSE_HOUSE_STATUS: Self = Self(0x0226);
    /// `0x0227` House_UpdateRentTime
    pub const HOUSE_UPDATE_RENT_TIME: Self = Self(0x0227);
    /// `0x0228` House_UpdateRentPayment
    pub const HOUSE_UPDATE_RENT_PAYMENT: Self = Self(0x0228);
    /// `0x0245` House_AddPermanentGuest
    pub const HOUSE_ADD_PERMANENT_GUEST: Self = Self(0x0245);
    /// `0x0246` House_RemovePermanentGuest
    pub const HOUSE_REMOVE_PERMANENT_GUEST: Self = Self(0x0246);
    /// `0x0247` House_SetOpenHouseStatus
    pub const HOUSE_SET_OPEN_HOUSE_STATUS: Self = Self(0x0247);
    /// `0x0248` House_UpdateRestrictions
    pub const HOUSE_UPDATE_RESTRICTIONS: Self = Self(0x0248);
    /// `0x0249` House_ChangeStoragePermission
    pub const HOUSE_CHANGE_STORAGE_PERMISSION: Self = Self(0x0249);
    /// `0x024A` House_BootSpecificHouseGuest
    pub const HOUSE_BOOT_SPECIFIC_HOUSE_GUEST: Self = Self(0x024A);
    /// `0x024C` House_RemoveAllStoragePermission
    pub const HOUSE_REMOVE_ALL_STORAGE_PERMISSION: Self = Self(0x024C);
    /// `0x024D` House_RequestFullGuestList
    pub const HOUSE_REQUEST_FULL_GUEST_LIST: Self = Self(0x024D);
    /// `0x0254` Allegiance_SetMotd
    pub const ALLEGIANCE_SET_MOTD: Self = Self(0x0254);
    /// `0x0255` Allegiance_QueryMotd
    pub const ALLEGIANCE_QUERY_MOTD: Self = Self(0x0255);
    /// `0x0256` Allegiance_ClearMotd
    pub const ALLEGIANCE_CLEAR_MOTD: Self = Self(0x0256);
    /// `0x0257` House_UpdateHAR
    pub const HOUSE_UPDATE_HAR: Self = Self(0x0257);
    /// `0x0258` House_QueryLord
    pub const HOUSE_QUERY_LORD: Self = Self(0x0258);
    /// `0x0259` House_HouseTransaction
    pub const HOUSE_HOUSE_TRANSACTION: Self = Self(0x0259);
    /// `0x025C` House_AddAllStoragePermission
    pub const HOUSE_ADD_ALL_STORAGE_PERMISSION: Self = Self(0x025C);
    /// `0x025E` House_RemoveAllPermanentGuests
    pub const HOUSE_REMOVE_ALL_PERMANENT_GUESTS: Self = Self(0x025E);
    /// `0x025F` House_BootEveryone
    pub const HOUSE_BOOT_EVERYONE: Self = Self(0x025F);
    /// `0x0262` House_TeleToHouse
    pub const HOUSE_TELE_TO_HOUSE: Self = Self(0x0262);
    /// `0x0263` Item_QueryItemMana
    pub const ITEM_QUERY_ITEM_MANA: Self = Self(0x0263);
    /// `0x0264` Item_QueryItemManaResponse
    pub const ITEM_QUERY_ITEM_MANA_RESPONSE: Self = Self(0x0264);
    /// `0x0266` House_SetHooksVisibility
    pub const HOUSE_SET_HOOKS_VISIBILITY: Self = Self(0x0266);
    /// `0x0267` House_ModifyAllegianceGuestPermission
    pub const HOUSE_MODIFY_ALLEGIANCE_GUEST_PERMISSION: Self = Self(0x0267);
    /// `0x0268` House_ModifyAllegianceStoragePermission
    pub const HOUSE_MODIFY_ALLEGIANCE_STORAGE_PERMISSION: Self = Self(0x0268);
    /// `0x0269` Game_Join
    pub const GAME_JOIN: Self = Self(0x0269);
    /// `0x026A` Game_Quit
    pub const GAME_QUIT: Self = Self(0x026A);
    /// `0x026B` Game_Move
    pub const GAME_MOVE: Self = Self(0x026B);
    /// `0x026D` Game_MovePass
    pub const GAME_MOVE_PASS: Self = Self(0x026D);
    /// `0x026E` Game_Stalemate
    pub const GAME_STALEMATE: Self = Self(0x026E);
    /// `0x0270` House_ListAvailableHouses
    pub const HOUSE_LIST_AVAILABLE_HOUSES: Self = Self(0x0270);
    /// `0x0271` House_AvailableHouses
    pub const HOUSE_AVAILABLE_HOUSES: Self = Self(0x0271);
    /// `0x0274` Character_ConfirmationRequest
    pub const CHARACTER_CONFIRMATION_REQUEST: Self = Self(0x0274);
    /// `0x0275` Character_ConfirmationResponse
    pub const CHARACTER_CONFIRMATION_RESPONSE: Self = Self(0x0275);
    /// `0x0276` Character_ConfirmationDone
    pub const CHARACTER_CONFIRMATION_DONE: Self = Self(0x0276);
    /// `0x0277` Allegiance_BreakAllegianceBoot
    pub const ALLEGIANCE_BREAK_ALLEGIANCE_BOOT: Self = Self(0x0277);
    /// `0x0278` House_TeleToMansion
    pub const HOUSE_TELE_TO_MANSION: Self = Self(0x0278);
    /// `0x0279` Character_Suicide
    pub const CHARACTER_SUICIDE: Self = Self(0x0279);
    /// `0x027A` Allegiance_AllegianceLoginNotificationEvent
    pub const ALLEGIANCE_ALLEGIANCE_LOGIN_NOTIFICATION_EVENT: Self = Self(0x027A);
    /// `0x027B` Allegiance_AllegianceInfoRequest
    pub const ALLEGIANCE_ALLEGIANCE_INFO_REQUEST: Self = Self(0x027B);
    /// `0x027C` Allegiance_AllegianceInfoResponseEvent
    pub const ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT: Self = Self(0x027C);
    /// `0x027D` Inventory_CreateTinkeringTool
    pub const INVENTORY_CREATE_TINKERING_TOOL: Self = Self(0x027D);
    /// `0x0281` Game_JoinGameResponse
    pub const GAME_JOIN_GAME_RESPONSE: Self = Self(0x0281);
    /// `0x0282` Game_StartGame
    pub const GAME_START_GAME: Self = Self(0x0282);
    /// `0x0283` Game_MoveResponse
    pub const GAME_MOVE_RESPONSE: Self = Self(0x0283);
    /// `0x0284` Game_OpponentTurn
    pub const GAME_OPPONENT_TURN: Self = Self(0x0284);
    /// `0x0285` Game_OpponentStalemateState
    pub const GAME_OPPONENT_STALEMATE_STATE: Self = Self(0x0285);
    /// `0x0286` Character_SpellbookFilterEvent
    pub const CHARACTER_SPELLBOOK_FILTER_EVENT: Self = Self(0x0286);
    /// `0x028A` Communication_WeenieError
    pub const COMMUNICATION_WEENIE_ERROR: Self = Self(0x028A);
    /// `0x028B` Communication_WeenieErrorWithString
    pub const COMMUNICATION_WEENIE_ERROR_WITH_STRING: Self = Self(0x028B);
    /// `0x028C` Game_GameOver
    pub const GAME_GAME_OVER: Self = Self(0x028C);
    /// `0x028D` Character_TeleToMarketplace
    pub const CHARACTER_TELE_TO_MARKETPLACE: Self = Self(0x028D);
    /// `0x028F` Character_EnterPKLite
    pub const CHARACTER_ENTER_PKLITE: Self = Self(0x028F);
    /// `0x0290` Fellowship_AssignNewLeader
    pub const FELLOWSHIP_ASSIGN_NEW_LEADER: Self = Self(0x0290);
    /// `0x0291` changes fellowship openness.
    pub const FELLOWSHIP_CHANGE_FELLOW_OPENNESS: Self = Self(0x0291);
    /// `0x0295` Communication_ChatRoomTracker
    pub const COMMUNICATION_CHAT_ROOM_TRACKER: Self = Self(0x0295);
    /// `0x02A0` Allegiance_AllegianceChatBoot
    pub const ALLEGIANCE_ALLEGIANCE_CHAT_BOOT: Self = Self(0x02A0);
    /// `0x02A1` Allegiance_AddAllegianceBan
    pub const ALLEGIANCE_ADD_ALLEGIANCE_BAN: Self = Self(0x02A1);
    /// `0x02A2` Allegiance_RemoveAllegianceBan
    pub const ALLEGIANCE_REMOVE_ALLEGIANCE_BAN: Self = Self(0x02A2);
    /// `0x02A3` Allegiance_ListAllegianceBans
    pub const ALLEGIANCE_LIST_ALLEGIANCE_BANS: Self = Self(0x02A3);
    /// `0x02A5` Allegiance_RemoveAllegianceOfficer
    pub const ALLEGIANCE_REMOVE_ALLEGIANCE_OFFICER: Self = Self(0x02A5);
    /// `0x02A6` Allegiance_ListAllegianceOfficers
    pub const ALLEGIANCE_LIST_ALLEGIANCE_OFFICERS: Self = Self(0x02A6);
    /// `0x02A7` Allegiance_ClearAllegianceOfficers
    pub const ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICERS: Self = Self(0x02A7);
    /// `0x02AB` Allegiance_RecallAllegianceHometown
    pub const ALLEGIANCE_RECALL_ALLEGIANCE_HOMETOWN: Self = Self(0x02AB);
    /// `0x02AE` Admin_QueryPluginList
    pub const ADMIN_QUERY_PLUGIN_LIST: Self = Self(0x02AE);
    /// `0x02AF` Admin_QueryPluginListResponse
    pub const ADMIN_QUERY_PLUGIN_LIST_RESPONSE: Self = Self(0x02AF);
    /// `0x02B1` Admin_QueryPlugin
    pub const ADMIN_QUERY_PLUGIN: Self = Self(0x02B1);
    /// `0x02B2` Admin_QueryPluginResponse
    pub const ADMIN_QUERY_PLUGIN_RESPONSE: Self = Self(0x02B2);
    /// `0x02B3` Admin_QueryPluginResponse_Recv
    pub const ADMIN_QUERY_PLUGIN_RESPONSE_RECV: Self = Self(0x02B3);
    /// `0x02B4` Inventory_SalvageOperationsResultData
    pub const INVENTORY_SALVAGE_OPERATIONS_RESULT_DATA: Self = Self(0x02B4);
    /// `0x02B8` Qualities_PrivateRemoveInt64Event
    pub const QUALITIES_PRIVATE_REMOVE_INT64_EVENT: Self = Self(0x02B8);
    /// `0x02B9` Qualities_RemoveInt64Event
    pub const QUALITIES_REMOVE_INT64_EVENT: Self = Self(0x02B9);
    /// `0x02BB` Communication_HearSpeech
    pub const COMMUNICATION_HEAR_SPEECH: Self = Self(0x02BB);
    /// `0x02BC` Communication_HearRangedSpeech
    pub const COMMUNICATION_HEAR_RANGED_SPEECH: Self = Self(0x02BC);
    /// `0x02BD` Communication_HearDirectSpeech
    pub const COMMUNICATION_HEAR_DIRECT_SPEECH: Self = Self(0x02BD);
    /// `0x02BE` Fellowship_FullUpdate
    pub const FELLOWSHIP_FULL_UPDATE: Self = Self(0x02BE);
    /// `0x02BF` Fellowship_Disband
    pub const FELLOWSHIP_DISBAND: Self = Self(0x02BF);
    /// `0x02C0` Fellowship_UpdateFellow
    pub const FELLOWSHIP_UPDATE_FELLOW: Self = Self(0x02C0);
    /// `0x02C1` Magic_UpdateSpell
    pub const MAGIC_UPDATE_SPELL: Self = Self(0x02C1);
    /// `0x02C2` Magic_UpdateEnchantment
    pub const MAGIC_UPDATE_ENCHANTMENT: Self = Self(0x02C2);
    /// `0x02C3` Magic_RemoveEnchantment
    pub const MAGIC_REMOVE_ENCHANTMENT: Self = Self(0x02C3);
    /// `0x02C4` Magic_UpdateMultipleEnchantments
    pub const MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS: Self = Self(0x02C4);
    /// `0x02C5` Magic_RemoveMultipleEnchantments
    pub const MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS: Self = Self(0x02C5);
    /// `0x02C6` Magic_PurgeEnchantments
    pub const MAGIC_PURGE_ENCHANTMENTS: Self = Self(0x02C6);
    /// `0x02C7` Magic_DispelEnchantment
    pub const MAGIC_DISPEL_ENCHANTMENT: Self = Self(0x02C7);
    /// `0x02C8` Magic_DispelMultipleEnchantments
    pub const MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS: Self = Self(0x02C8);
    /// `0x02C9` Misc_PortalStormBrewing
    pub const MISC_PORTAL_STORM_BREWING: Self = Self(0x02C9);
    /// `0x02CA` Misc_PortalStormImminent
    pub const MISC_PORTAL_STORM_IMMINENT: Self = Self(0x02CA);
    /// `0x02CB` Misc_PortalStorm
    pub const MISC_PORTAL_STORM: Self = Self(0x02CB);
    /// `0x02CC` Misc_PortalStormSubsided
    pub const MISC_PORTAL_STORM_SUBSIDED: Self = Self(0x02CC);
    /// `0x02CD` Qualities_PrivateUpdateInt
    pub const QUALITIES_PRIVATE_UPDATE_INT: Self = Self(0x02CD);
    /// `0x02CE` Qualities_UpdateInt
    pub const QUALITIES_UPDATE_INT: Self = Self(0x02CE);
    /// `0x02CF` Qualities_PrivateUpdateInt64
    pub const QUALITIES_PRIVATE_UPDATE_INT64: Self = Self(0x02CF);
    /// `0x02D0` Qualities_UpdateInt64
    pub const QUALITIES_UPDATE_INT64: Self = Self(0x02D0);
    /// `0x02D1` Qualities_PrivateUpdateBool
    pub const QUALITIES_PRIVATE_UPDATE_BOOL: Self = Self(0x02D1);
    /// `0x02D2` Qualities_UpdateBool
    pub const QUALITIES_UPDATE_BOOL: Self = Self(0x02D2);
    /// `0x02D3` Qualities_PrivateUpdateFloat
    pub const QUALITIES_PRIVATE_UPDATE_FLOAT: Self = Self(0x02D3);
    /// `0x02D4` Qualities_UpdateFloat
    pub const QUALITIES_UPDATE_FLOAT: Self = Self(0x02D4);
    /// `0x02D5` Qualities_PrivateUpdateString
    pub const QUALITIES_PRIVATE_UPDATE_STRING: Self = Self(0x02D5);
    /// `0x02D6` Qualities_UpdateString
    pub const QUALITIES_UPDATE_STRING: Self = Self(0x02D6);
    /// `0x02D7` Qualities_PrivateUpdateDataID
    pub const QUALITIES_PRIVATE_UPDATE_DATA_ID: Self = Self(0x02D7);
    /// `0x02D8` Qualities_UpdateDataID
    pub const QUALITIES_UPDATE_DATA_ID: Self = Self(0x02D8);
    /// `0x02D9` Qualities_PrivateUpdateInstanceID
    pub const QUALITIES_PRIVATE_UPDATE_INSTANCE_ID: Self = Self(0x02D9);
    /// `0x02DA` Qualities_UpdateInstanceID
    pub const QUALITIES_UPDATE_INSTANCE_ID: Self = Self(0x02DA);
    /// `0x02DB` Qualities_PrivateUpdatePosition
    pub const QUALITIES_PRIVATE_UPDATE_POSITION: Self = Self(0x02DB);
    /// `0x02DC` Qualities_UpdatePosition
    pub const QUALITIES_UPDATE_POSITION: Self = Self(0x02DC);
    /// `0x02DD` Qualities_PrivateUpdateSkill
    pub const QUALITIES_PRIVATE_UPDATE_SKILL: Self = Self(0x02DD);
    /// `0x02DE` Qualities_UpdateSkill
    pub const QUALITIES_UPDATE_SKILL: Self = Self(0x02DE);
    /// `0x02DF` Qualities_PrivateUpdateSkillLevel
    pub const QUALITIES_PRIVATE_UPDATE_SKILL_LEVEL: Self = Self(0x02DF);
    /// `0x02E0` Qualities_UpdateSkillLevel
    pub const QUALITIES_UPDATE_SKILL_LEVEL: Self = Self(0x02E0);
    /// `0x02E1` Qualities_PrivateUpdateSkillAC
    pub const QUALITIES_PRIVATE_UPDATE_SKILL_AC: Self = Self(0x02E1);
    /// `0x02E2` Qualities_UpdateSkillAC
    pub const QUALITIES_UPDATE_SKILL_AC: Self = Self(0x02E2);
    /// `0x02E3` Qualities_PrivateUpdateAttribute
    pub const QUALITIES_PRIVATE_UPDATE_ATTRIBUTE: Self = Self(0x02E3);
    /// `0x02E4` Qualities_UpdateAttribute
    pub const QUALITIES_UPDATE_ATTRIBUTE: Self = Self(0x02E4);
    /// `0x02E5` Qualities_PrivateUpdateAttributeLevel
    pub const QUALITIES_PRIVATE_UPDATE_ATTRIBUTE_LEVEL: Self = Self(0x02E5);
    /// `0x02E6` Qualities_UpdateAttributeLevel
    pub const QUALITIES_UPDATE_ATTRIBUTE_LEVEL: Self = Self(0x02E6);
    /// `0x02E7` Qualities_PrivateUpdateAttribute2nd
    pub const QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND: Self = Self(0x02E7);
    /// `0x02E8` Qualities_UpdateAttribute2nd
    pub const QUALITIES_UPDATE_ATTRIBUTE2ND: Self = Self(0x02E8);
    /// `0x02E9` Qualities_PrivateUpdateAttribute2ndLevel
    pub const QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND_LEVEL: Self = Self(0x02E9);
    /// `0x02EA` Qualities_UpdateAttribute2ndLevel
    pub const QUALITIES_UPDATE_ATTRIBUTE2ND_LEVEL: Self = Self(0x02EA);
    /// `0x02EB` Communication_TransientString
    pub const COMMUNICATION_TRANSIENT_STRING: Self = Self(0x02EB);
    /// `0x0311` Character_FinishBarber
    pub const CHARACTER_FINISH_BARBER: Self = Self(0x0311);
    /// `0x0312` Magic_PurgeBadEnchantments
    pub const MAGIC_PURGE_BAD_ENCHANTMENTS: Self = Self(0x0312);
    /// `0x0314` Social_SendClientContractTrackerTable
    pub const SOCIAL_SEND_CLIENT_CONTRACT_TRACKER_TABLE: Self = Self(0x0314);
    /// `0x0315` Social_SendClientContractTracker
    pub const SOCIAL_SEND_CLIENT_CONTRACT_TRACKER: Self = Self(0x0315);
    /// `0x0316` Social_AbandonContract
    pub const SOCIAL_ABANDON_CONTRACT: Self = Self(0x0316);
    /// `0x0317`, a game event carrying one string that the client shows exactly as it shows
    /// `0x02EB Communication_TransientString`. The community catalogue has no name for it, so
    /// it has no row in [`OPCODES`]; the name here says what it behaves as.
    pub const COMMUNICATION_TRANSIENT_STRING_0317: Self = Self(0x0317);
    /// `0x0318`, a game event carrying one string that the client shows exactly as it shows
    /// `0x0004 Communication_PopUpString`. Unnamed in the community catalogue, like `0x0317`.
    pub const COMMUNICATION_POP_UP_STRING_0318: Self = Self(0x0318);
    /// `0xEA60` Admin_Environs
    pub const ADMIN_ENVIRONS: Self = Self(0xEA60);
    /// `0xF619` Movement_PositionAndMovementEvent
    pub const MOVEMENT_POSITION_AND_MOVEMENT_EVENT: Self = Self(0xF619);
    /// `0xF61B` Movement_Jump
    pub const MOVEMENT_JUMP: Self = Self(0xF61B);
    /// `0xF61C` Movement_MoveToState
    pub const MOVEMENT_MOVE_TO_STATE: Self = Self(0xF61C);
    /// `0xF61E` Movement_DoMovementCommand
    pub const MOVEMENT_DO_MOVEMENT_COMMAND: Self = Self(0xF61E);
    /// `0xF625` Item_ObjDescEvent
    pub const ITEM_OBJ_DESC_EVENT: Self = Self(0xF625);
    /// `0xF630` Character_SetPlayerVisualDesc
    pub const CHARACTER_SET_PLAYER_VISUAL_DESC: Self = Self(0xF630);
    /// `0xF643` Character_CharGenVerificationResponse
    pub const CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE: Self = Self(0xF643);
    /// `0xF649` Movement_TurnToEvent
    pub const MOVEMENT_TURN_TO_EVENT: Self = Self(0xF649);
    /// `0xF651` Login_AwaitingSubscriptionExpiration
    pub const LOGIN_AWAITING_SUBSCRIPTION_EXPIRATION: Self = Self(0xF651);
    /// `0xF653` Login_ExecuteLogOff
    pub const LOGIN_EXECUTE_LOG_OFF: Self = Self(0xF653);
    /// `0xF655` Character_CharacterDelete
    pub const CHARACTER_CHARACTER_DELETE: Self = Self(0xF655);
    /// `0xF656` Character_SendCharGenResult
    pub const CHARACTER_SEND_CHAR_GEN_RESULT: Self = Self(0xF656);
    /// `0xF657` Login_SendEnterWorld
    pub const LOGIN_SEND_ENTER_WORLD: Self = Self(0xF657);
    /// `0xF658` Login_LoginCharacterSet
    pub const LOGIN_LOGIN_CHARACTER_SET: Self = Self(0xF658);
    /// `0xF659` Character_CharacterError
    pub const CHARACTER_CHARACTER_ERROR: Self = Self(0xF659);
    /// `0xF661` Movement_StopMovementCommand
    pub const MOVEMENT_STOP_MOVEMENT_COMMAND: Self = Self(0xF661);
    /// `0xF6EA` Object_SendForceObjdesc
    pub const OBJECT_SEND_FORCE_OBJDESC: Self = Self(0xF6EA);
    /// `0xF745` Item_CreateObject
    pub const ITEM_CREATE_OBJECT: Self = Self(0xF745);
    /// `0xF746` Login_CreatePlayer
    pub const LOGIN_CREATE_PLAYER: Self = Self(0xF746);
    /// `0xF747` Item_DeleteObject
    pub const ITEM_DELETE_OBJECT: Self = Self(0xF747);
    /// `0xF748` Movement_PositionEvent
    pub const MOVEMENT_POSITION_EVENT: Self = Self(0xF748);
    /// `0xF749` Item_ParentEvent
    pub const ITEM_PARENT_EVENT: Self = Self(0xF749);
    /// `0xF74A` Inventory_PickupEvent
    pub const INVENTORY_PICKUP_EVENT: Self = Self(0xF74A);
    /// `0xF74B` Item_SetState
    pub const ITEM_SET_STATE: Self = Self(0xF74B);
    /// `0xF74C` Movement_SetObjectMovement
    pub const MOVEMENT_SET_OBJECT_MOVEMENT: Self = Self(0xF74C);
    /// `0xF74E` Movement_VectorUpdate
    pub const MOVEMENT_VECTOR_UPDATE: Self = Self(0xF74E);
    /// `0xF750` Effects_SoundEvent
    pub const EFFECTS_SOUND_EVENT: Self = Self(0xF750);
    /// `0xF751` Effects_PlayerTeleport
    pub const EFFECTS_PLAYER_TELEPORT: Self = Self(0xF751);
    /// `0xF752` Movement_AutonomyLevel
    pub const MOVEMENT_AUTONOMY_LEVEL: Self = Self(0xF752);
    /// `0xF753` Movement_AutonomousPosition
    pub const MOVEMENT_AUTONOMOUS_POSITION: Self = Self(0xF753);
    /// `0xF754` Effects_PlayScriptID
    pub const EFFECTS_PLAY_SCRIPT_ID: Self = Self(0xF754);
    /// `0xF755` Effects_PlayScriptType
    pub const EFFECTS_PLAY_SCRIPT_TYPE: Self = Self(0xF755);
    /// `0xF7C1` Login_AccountBanned
    pub const LOGIN_ACCOUNT_BANNED: Self = Self(0xF7C1);
    /// `0xF7C8` Login_SendEnterWorldRequest
    pub const LOGIN_SEND_ENTER_WORLD_REQUEST: Self = Self(0xF7C8);
    /// `0xF7C9` Movement_Jump_NonAutonomous
    pub const MOVEMENT_JUMP_NON_AUTONOMOUS: Self = Self(0xF7C9);
    /// `0xF7CA` Admin_ReceiveAccountData
    pub const ADMIN_RECEIVE_ACCOUNT_DATA: Self = Self(0xF7CA);
    /// `0xF7CB` Admin_ReceivePlayerData
    pub const ADMIN_RECEIVE_PLAYER_DATA: Self = Self(0xF7CB);
    /// `0xF7CC` Admin_SendAdminGetServerVersion
    pub const ADMIN_SEND_ADMIN_GET_SERVER_VERSION: Self = Self(0xF7CC);
    /// `0xF7CD` Social_SendFriendsCommand
    pub const SOCIAL_SEND_FRIENDS_COMMAND: Self = Self(0xF7CD);
    /// `0xF7D9` Admin_SendAdminRestoreCharacter
    pub const ADMIN_SEND_ADMIN_RESTORE_CHARACTER: Self = Self(0xF7D9);
    /// `0xF7DB` Item_UpdateObject
    pub const ITEM_UPDATE_OBJECT: Self = Self(0xF7DB);
    /// `0xF7DC` Login_AccountBooted
    pub const LOGIN_ACCOUNT_BOOTED: Self = Self(0xF7DC);
    /// `0xF7DE` Communication_TurbineChat
    pub const COMMUNICATION_TURBINE_CHAT: Self = Self(0xF7DE);
    /// `0xF7DF` Login_EnterGame_ServerReady
    pub const LOGIN_ENTER_GAME_SERVER_READY: Self = Self(0xF7DF);
    /// `0xF7E0` Communication_TextboxString
    pub const COMMUNICATION_TEXTBOX_STRING: Self = Self(0xF7E0);
    /// `0xF7E1` Login_WorldInfo
    pub const LOGIN_WORLD_INFO: Self = Self(0xF7E1);
    /// `0xF7E2` DDD_DataMessage
    pub const DDD_DATA_MESSAGE: Self = Self(0xF7E2);
    /// `0xF7E3` DDD_RequestDataMessage
    pub const DDD_REQUEST_DATA_MESSAGE: Self = Self(0xF7E3);
    /// `0xF7E4` DDD_ErrorMessage
    pub const DDD_ERROR_MESSAGE: Self = Self(0xF7E4);
    /// `0xF7E5` DDD_InterrogationMessage
    pub const DDD_INTERROGATION_MESSAGE: Self = Self(0xF7E5);
    /// `0xF7E6` DDD_InterrogationResponseMessage
    pub const DDD_INTERROGATION_RESPONSE_MESSAGE: Self = Self(0xF7E6);
    /// `0xF7E7` DDD_BeginDDDMessage
    pub const DDD_BEGIN_DDDMESSAGE: Self = Self(0xF7E7);
    /// `0xF7EA` DDD_OnEndDDD
    pub const DDD_ON_END_DDD: Self = Self(0xF7EA);
    /// `0xF7EB` DDD_EndDDDMessage
    pub const DDD_END_DDDMESSAGE: Self = Self(0xF7EB);

    /// The master table row for this opcode, if the client knows it.
    #[must_use]
    pub fn info(self) -> Option<&'static OpcodeInfo> {
        OPCODES
            .binary_search_by_key(&self.0, |i| i.opcode.0)
            .ok()
            .map(|i| &OPCODES[i])
    }

    /// The community catalogue's name for this opcode.
    #[must_use]
    pub fn name(self) -> Option<&'static str> {
        self.info().map(|i| i.name)
    }
}

/// The master opcode table, ascending by opcode so that [`Opcode::info`] can bisect it.
pub static OPCODES: &[OpcodeInfo] = &[
    OpcodeInfo {
        opcode: Opcode(0x0003),
        name: "Allegiance_AllegianceUpdateAborted",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0004),
        name: "Communication_PopUpString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0005),
        name: "Character_PlayerOptionChangedEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0008),
        name: "Combat_TargetedMeleeAttack",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x000A),
        name: "Combat_TargetedMissileAttack",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x000F),
        name: "Communication_SetAFKMode",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0010),
        name: "Communication_SetAFKMessage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0013),
        name: "Login_PlayerDescription",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0015),
        name: "Communication_Talk",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0017),
        name: "Social_RemoveFriend",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0018),
        name: "Social_AddFriend",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0019),
        name: "Inventory_PutItemInContainer",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x001A),
        name: "Inventory_GetAndWieldItem",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x001B),
        name: "Inventory_DropItem",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x001D),
        name: "Allegiance_SwearAllegiance",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x001E),
        name: "Allegiance_BreakAllegiance",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x001F),
        name: "Allegiance_UpdateRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0020),
        name: "Allegiance_AllegianceUpdate",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0021),
        name: "Social_FriendsUpdate",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0022),
        name: "Item_ServerSaysContainID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0023),
        name: "Item_WearItem",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0024),
        name: "Item_ServerSaysRemove",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0025),
        name: "Social_ClearFriends",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0026),
        name: "Character_TeleToPKLArena",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0027),
        name: "Character_TeleToPKArena",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0029),
        name: "Social_CharacterTitleTable",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x002B),
        name: "Social_AddOrSetCharacterTitle",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x002C),
        name: "Social_SetDisplayCharacterTitle",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0030),
        name: "Allegiance_QueryAllegianceName",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0031),
        name: "Allegiance_ClearAllegianceName",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0032),
        name: "Communication_TalkDirect",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0033),
        name: "Allegiance_SetAllegianceName",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0035),
        name: "Inventory_UseWithTargetEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0036),
        name: "Inventory_UseEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x003B),
        name: "Allegiance_SetAllegianceOfficer",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x003C),
        name: "Allegiance_SetAllegianceOfficerTitle",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x003D),
        name: "Allegiance_ListAllegianceOfficerTitles",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x003E),
        name: "Allegiance_ClearAllegianceOfficerTitles",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x003F),
        name: "Allegiance_DoAllegianceLockAction",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0040),
        name: "Allegiance_SetAllegianceApprovedVassal",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0041),
        name: "Allegiance_AllegianceChatGag",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0042),
        name: "Allegiance_DoAllegianceHouseAction",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0044),
        name: "Train_TrainAttribute2nd",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0045),
        name: "Train_TrainAttribute",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0046),
        name: "Train_TrainSkill",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0047),
        name: "Train_TrainSkillAdvancementClass",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0048),
        name: "Magic_CastUntargetedSpell",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x004A),
        name: "Magic_CastTargetedSpell",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0052),
        name: "Item_StopViewingObjectContents",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0053),
        name: "Combat_ChangeCombatMode",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0054),
        name: "Inventory_StackableMerge",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0055),
        name: "Inventory_StackableSplitToContainer",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0056),
        name: "Inventory_StackableSplitTo3D",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0058),
        name: "Communication_ModifyCharacterSquelch",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0059),
        name: "Communication_ModifyAccountSquelch",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x005B),
        name: "Communication_ModifyGlobalSquelch",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x005D),
        name: "Communication_TalkDirectByName",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x005F),
        name: "Vendor_Buy",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0060),
        name: "Vendor_Sell",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0062),
        name: "Vendor_VendorInfo",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0063),
        name: "Character_TeleToLifestone",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0075),
        name: "Character_StartBarber",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00A0),
        name: "Character_ServerSaysAttemptFailed",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00A1),
        name: "Character_LoginCompleteNotification",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00A2),
        name: "Fellowship_Create",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00A3),
        name: "Fellowship_Quit",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00A4),
        name: "Fellowship_Dismiss",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00A5),
        name: "Fellowship_Recruit",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00A6),
        name: "Fellowship_UpdateRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00AA),
        name: "Writing_BookData",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00AB),
        name: "Writing_BookModifyPage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00AC),
        name: "Writing_BookAddPage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00AD),
        name: "Writing_BookDeletePage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00AE),
        name: "Writing_BookPageData",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00B4),
        name: "Writing_BookOpen",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00B6),
        name: "Writing_BookAddPageResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00B7),
        name: "Writing_BookDeletePageResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00B8),
        name: "Writing_BookPageDataResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00BF),
        name: "Writing_SetInscription",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00C3),
        name: "Item_GetInscriptionResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00C8),
        name: "Item_Appraise",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00C9),
        name: "Item_SetAppraiseInfo",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x00CD),
        name: "Inventory_GiveObjectRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x00D6),
        name: "Advocate_Teleport",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0140),
        name: "Character_AbuseLogRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0145),
        name: "Communication_AddToChannel",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0146),
        name: "Communication_RemoveFromChannel",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0147),
        name: "Communication_ChannelBroadcast",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0148),
        name: "Communication_ChannelList",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0149),
        name: "Communication_ChannelIndex",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0195),
        name: "Inventory_NoLongerViewingContents",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0196),
        name: "Item_OnViewContents",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0197),
        name: "Item_UpdateStackSize",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x019A),
        name: "Item_ServerSaysMoveItem",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x019B),
        name: "Inventory_StackableSplitToWield",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x019C),
        name: "Character_AddShortCut",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x019D),
        name: "Character_RemoveShortCut",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x019E),
        name: "Combat_HandlePlayerDeathEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01A1),
        name: "Character_CharacterOptionsEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01A7),
        name: "Combat_HandleAttackDoneEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01A8),
        name: "Magic_RemoveSpell",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01AC),
        name: "Combat_HandleVictimNotificationEventSelf",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01AD),
        name: "Combat_HandleVictimNotificationEventOther",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01B1),
        name: "Combat_HandleAttackerNotificationEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01B2),
        name: "Combat_HandleDefenderNotificationEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01B3),
        name: "Combat_HandleEvasionAttackerNotificationEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01B4),
        name: "Combat_HandleEvasionDefenderNotificationEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01B7),
        name: "Combat_CancelAttack",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01B8),
        name: "Combat_HandleCommenceAttackEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01BF),
        name: "Combat_QueryHealth",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01C0),
        name: "Combat_QueryHealthResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01C2),
        name: "Character_QueryAge",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01C3),
        name: "Character_QueryAgeResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01C4),
        name: "Character_QueryBirth",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01C7),
        name: "Item_UseDone",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01C9),
        name: "Fellowship_FellowUpdateDone",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01CA),
        name: "Fellowship_FellowStatsDone",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01CB),
        name: "Item_AppraiseDone",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D1),
        name: "Qualities_PrivateRemoveIntEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D2),
        name: "Qualities_RemoveIntEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D3),
        name: "Qualities_PrivateRemoveBoolEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D4),
        name: "Qualities_RemoveBoolEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D5),
        name: "Qualities_PrivateRemoveFloatEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D6),
        name: "Qualities_RemoveFloatEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D7),
        name: "Qualities_PrivateRemoveStringEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D8),
        name: "Qualities_RemoveStringEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01D9),
        name: "Qualities_PrivateRemoveDataIDEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DA),
        name: "Qualities_RemoveDataIDEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DB),
        name: "Qualities_PrivateRemoveInstanceIDEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DC),
        name: "Qualities_RemoveInstanceIDEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DD),
        name: "Qualities_PrivateRemovePositionEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DE),
        name: "Qualities_RemovePositionEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01DF),
        name: "Communication_Emote",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01E0),
        name: "Communication_HearEmote",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01E1),
        name: "Communication_SoulEmote",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01E2),
        name: "Communication_HearSoulEmote",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01E3),
        name: "Character_AddSpellFavorite",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01E4),
        name: "Character_RemoveSpellFavorite",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01E9),
        name: "Character_RequestPing",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01EA),
        name: "Character_ReturnPing",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01F4),
        name: "Communication_SetSquelchDB",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01F6),
        name: "Trade_OpenTradeNegotiations",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01F7),
        name: "Trade_CloseTradeNegotiations",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01F8),
        name: "Trade_AddToTrade",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01FA),
        name: "Trade_AcceptTrade",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01FB),
        name: "Trade_DeclineTrade",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x01FD),
        name: "Trade_RegisterTrade",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01FE),
        name: "Trade_OpenTrade",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x01FF),
        name: "Trade_CloseTrade",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0200),
        name: "Trade_AddToTrade_Recv",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0201),
        name: "Trade_RemoveFromTrade",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0202),
        name: "Trade_AcceptTrade_Recv",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0203),
        name: "Trade_DeclineTrade_Recv",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0204),
        name: "Trade_ResetTrade",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0205),
        name: "Trade_ResetTrade_Recv",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0207),
        name: "Trade_TradeFailure",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0208),
        name: "Trade_ClearTradeAcceptance",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0216),
        name: "Character_ClearPlayerConsentList",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0217),
        name: "Character_DisplayPlayerConsentList",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0218),
        name: "Character_RemoveFromPlayerConsentList",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0219),
        name: "Character_AddPlayerPermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x021A),
        name: "Character_RemovePlayerPermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x021C),
        name: "House_BuyHouse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x021D),
        name: "House_HouseProfile",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x021E),
        name: "House_QueryHouse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x021F),
        name: "House_AbandonHouse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0220),
        name: "Character_RemovePlayerPermission_UnusedCatalogueOpcode",
        direction: Direction::C2S,
        send_queue: None,
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0221),
        name: "House_RentHouse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0224),
        name: "Character_SetDesiredComponentLevel",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0225),
        name: "House_HouseData",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0226),
        name: "House_HouseStatus",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0227),
        name: "House_UpdateRentTime",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0228),
        name: "House_UpdateRentPayment",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0245),
        name: "House_AddPermanentGuest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0246),
        name: "House_RemovePermanentGuest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0247),
        name: "House_SetOpenHouseStatus",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0248),
        name: "House_UpdateRestrictions",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0249),
        name: "House_ChangeStoragePermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x024A),
        name: "House_BootSpecificHouseGuest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x024C),
        name: "House_RemoveAllStoragePermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x024D),
        name: "House_RequestFullGuestList",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0254),
        name: "Allegiance_SetMotd",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0255),
        name: "Allegiance_QueryMotd",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0256),
        name: "Allegiance_ClearMotd",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0257),
        name: "House_UpdateHAR",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0258),
        name: "House_QueryLord",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0259),
        name: "House_HouseTransaction",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x025C),
        name: "House_AddAllStoragePermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x025E),
        name: "House_RemoveAllPermanentGuests",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x025F),
        name: "House_BootEveryone",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0262),
        name: "House_TeleToHouse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0263),
        name: "Item_QueryItemMana",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0264),
        name: "Item_QueryItemManaResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0266),
        name: "House_SetHooksVisibility",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0267),
        name: "House_ModifyAllegianceGuestPermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0268),
        name: "House_ModifyAllegianceStoragePermission",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0269),
        name: "Game_Join",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x026A),
        name: "Game_Quit",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x026B),
        name: "Game_Move",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x026D),
        name: "Game_MovePass",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x026E),
        name: "Game_Stalemate",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0270),
        name: "House_ListAvailableHouses",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0271),
        name: "House_AvailableHouses",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0274),
        name: "Character_ConfirmationRequest",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0275),
        name: "Character_ConfirmationResponse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0276),
        name: "Character_ConfirmationDone",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0277),
        name: "Allegiance_BreakAllegianceBoot",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0278),
        name: "House_TeleToMansion",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0279),
        name: "Character_Suicide",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x027A),
        name: "Allegiance_AllegianceLoginNotificationEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x027B),
        name: "Allegiance_AllegianceInfoRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x027C),
        name: "Allegiance_AllegianceInfoResponseEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x027D),
        name: "Inventory_CreateTinkeringTool",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0281),
        name: "Game_JoinGameResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0282),
        name: "Game_StartGame",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0283),
        name: "Game_MoveResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0284),
        name: "Game_OpponentTurn",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0285),
        name: "Game_OpponentStalemateState",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0286),
        name: "Character_SpellbookFilterEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x028A),
        name: "Communication_WeenieError",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x028B),
        name: "Communication_WeenieErrorWithString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x028C),
        name: "Game_GameOver",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x028D),
        name: "Character_TeleToMarketplace",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x028F),
        name: "Character_EnterPKLite",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0290),
        name: "Fellowship_AssignNewLeader",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0291),
        name: "Fellowship_ChangeFellowOpenness",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0295),
        name: "Communication_ChatRoomTracker",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02A0),
        name: "Allegiance_AllegianceChatBoot",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A1),
        name: "Allegiance_AddAllegianceBan",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A2),
        name: "Allegiance_RemoveAllegianceBan",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A3),
        name: "Allegiance_ListAllegianceBans",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A5),
        name: "Allegiance_RemoveAllegianceOfficer",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A6),
        name: "Allegiance_ListAllegianceOfficers",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02A7),
        name: "Allegiance_ClearAllegianceOfficers",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02AB),
        name: "Allegiance_RecallAllegianceHometown",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02AE),
        name: "Admin_QueryPluginList",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02AF),
        name: "Admin_QueryPluginListResponse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02B1),
        name: "Admin_QueryPlugin",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02B2),
        name: "Admin_QueryPluginResponse",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x02B3),
        name: "Admin_QueryPluginResponse_Recv",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02B4),
        name: "Inventory_SalvageOperationsResultData",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02B8),
        name: "Qualities_PrivateRemoveInt64Event",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02B9),
        name: "Qualities_RemoveInt64Event",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02BB),
        name: "Communication_HearSpeech",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02BC),
        name: "Communication_HearRangedSpeech",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02BD),
        name: "Communication_HearDirectSpeech",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02BE),
        name: "Fellowship_FullUpdate",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02BF),
        name: "Fellowship_Disband",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C0),
        name: "Fellowship_UpdateFellow",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C1),
        name: "Magic_UpdateSpell",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C2),
        name: "Magic_UpdateEnchantment",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C3),
        name: "Magic_RemoveEnchantment",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C4),
        name: "Magic_UpdateMultipleEnchantments",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C5),
        name: "Magic_RemoveMultipleEnchantments",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C6),
        name: "Magic_PurgeEnchantments",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C7),
        name: "Magic_DispelEnchantment",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C8),
        name: "Magic_DispelMultipleEnchantments",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02C9),
        name: "Misc_PortalStormBrewing",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CA),
        name: "Misc_PortalStormImminent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CB),
        name: "Misc_PortalStorm",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CC),
        name: "Misc_PortalStormSubsided",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CD),
        name: "Qualities_PrivateUpdateInt",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CE),
        name: "Qualities_UpdateInt",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02CF),
        name: "Qualities_PrivateUpdateInt64",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D0),
        name: "Qualities_UpdateInt64",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D1),
        name: "Qualities_PrivateUpdateBool",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D2),
        name: "Qualities_UpdateBool",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D3),
        name: "Qualities_PrivateUpdateFloat",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D4),
        name: "Qualities_UpdateFloat",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D5),
        name: "Qualities_PrivateUpdateString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D6),
        name: "Qualities_UpdateString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D7),
        name: "Qualities_PrivateUpdateDataID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D8),
        name: "Qualities_UpdateDataID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02D9),
        name: "Qualities_PrivateUpdateInstanceID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DA),
        name: "Qualities_UpdateInstanceID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DB),
        name: "Qualities_PrivateUpdatePosition",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DC),
        name: "Qualities_UpdatePosition",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DD),
        name: "Qualities_PrivateUpdateSkill",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DE),
        name: "Qualities_UpdateSkill",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02DF),
        name: "Qualities_PrivateUpdateSkillLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E0),
        name: "Qualities_UpdateSkillLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E1),
        name: "Qualities_PrivateUpdateSkillAC",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E2),
        name: "Qualities_UpdateSkillAC",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E3),
        name: "Qualities_PrivateUpdateAttribute",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E4),
        name: "Qualities_UpdateAttribute",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E5),
        name: "Qualities_PrivateUpdateAttributeLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E6),
        name: "Qualities_UpdateAttributeLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E7),
        name: "Qualities_PrivateUpdateAttribute2nd",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E8),
        name: "Qualities_UpdateAttribute2nd",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02E9),
        name: "Qualities_PrivateUpdateAttribute2ndLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02EA),
        name: "Qualities_UpdateAttribute2ndLevel",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x02EB),
        name: "Communication_TransientString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0311),
        name: "Character_FinishBarber",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0x0312),
        name: "Magic_PurgeBadEnchantments",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0314),
        name: "Social_SendClientContractTrackerTable",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0315),
        name: "Social_SendClientContractTracker",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0x0316),
        name: "Social_AbandonContract",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xEA60),
        name: "Admin_Environs",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF619),
        name: "Movement_PositionAndMovementEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF61B),
        name: "Movement_Jump",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF61C),
        name: "Movement_MoveToState",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF61E),
        name: "Movement_DoMovementCommand",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF625),
        name: "Item_ObjDescEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF630),
        name: "Character_SetPlayerVisualDesc",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF643),
        name: "Character_CharGenVerificationResponse",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF649),
        name: "Movement_TurnToEvent",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF651),
        name: "Login_AwaitingSubscriptionExpiration",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF653),
        name: "Login_ExecuteLogOff",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Logon),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF655),
        name: "Character_CharacterDelete",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Logon),
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF656),
        name: "Character_SendCharGenResult",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Logon),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF657),
        name: "Login_SendEnterWorld",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Logon),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF658),
        name: "Login_LoginCharacterSet",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF659),
        name: "Character_CharacterError",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF661),
        name: "Movement_StopMovementCommand",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF6EA),
        name: "Object_SendForceObjdesc",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Control),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF745),
        name: "Item_CreateObject",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF746),
        name: "Login_CreatePlayer",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF747),
        name: "Item_DeleteObject",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF748),
        name: "Movement_PositionEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF749),
        name: "Item_ParentEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF74A),
        name: "Inventory_PickupEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF74B),
        name: "Item_SetState",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF74C),
        name: "Movement_SetObjectMovement",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF74E),
        name: "Movement_VectorUpdate",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF750),
        name: "Effects_SoundEvent",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF751),
        name: "Effects_PlayerTeleport",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF752),
        name: "Movement_AutonomyLevel",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF753),
        name: "Movement_AutonomousPosition",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF754),
        name: "Effects_PlayScriptID",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF755),
        name: "Effects_PlayScriptType",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7C1),
        name: "Login_AccountBanned",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7C8),
        name: "Login_SendEnterWorldRequest",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Logon),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7C9),
        name: "Movement_Jump_NonAutonomous",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Weenie),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7CA),
        name: "Admin_ReceiveAccountData",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7CB),
        name: "Admin_ReceivePlayerData",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7CC),
        name: "Admin_SendAdminGetServerVersion",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Control),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7CD),
        name: "Social_SendFriendsCommand",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Control),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7D9),
        name: "Admin_SendAdminRestoreCharacter",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::Control),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7DB),
        name: "Item_UpdateObject",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::WorldObjects),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7DC),
        name: "Login_AccountBooted",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7DE),
        name: "Communication_TurbineChat",
        direction: Direction::Both,
        send_queue: Some(NetQueue::Logon),
        recv_queue: Some(NetQueue::Logon),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7DF),
        name: "Login_EnterGame_ServerReady",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E0),
        name: "Communication_TextboxString",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E1),
        name: "Login_WorldInfo",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::UiQueue),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E2),
        name: "DDD_DataMessage",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::ClientCache),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E3),
        name: "DDD_RequestDataMessage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::ClientCache),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E4),
        name: "DDD_ErrorMessage",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::ClientCache),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E5),
        name: "DDD_InterrogationMessage",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::ClientCache),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E6),
        name: "DDD_InterrogationResponseMessage",
        direction: Direction::C2S,
        send_queue: Some(NetQueue::ClientCache),
        recv_queue: None,
    },
    OpcodeInfo {
        opcode: Opcode(0xF7E7),
        name: "DDD_BeginDDDMessage",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::ClientCache),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7EA),
        name: "DDD_OnEndDDD",
        direction: Direction::Both,
        send_queue: Some(NetQueue::ClientCache),
        recv_queue: Some(NetQueue::ClientCache),
    },
    OpcodeInfo {
        opcode: Opcode(0xF7EB),
        name: "DDD_EndDDDMessage",
        direction: Direction::S2C,
        send_queue: None,
        recv_queue: Some(NetQueue::ClientCache),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The table must stay sorted for [`Opcode::info`]'s bisection, and every row must be found
    /// by it. The count is the number of distinct opcodes in the table: a row added or dropped by
    /// hand has to change it deliberately.
    #[test]
    fn the_table_is_sorted_and_complete() {
        assert_eq!(OPCODES.len(), 352);
        for w in OPCODES.windows(2) {
            assert!(
                w[0].opcode.0 < w[1].opcode.0,
                "table must be sorted for bisection"
            );
        }
        for i in OPCODES {
            assert_eq!(i.opcode.info().map(|x| x.name), Some(i.name));
        }
    }

    /// The community catalogue calls this 0x0220; the client sends 0x021A.
    #[test]
    fn remove_player_permission_is_021a() {
        assert_eq!(Opcode::CHARACTER_REMOVE_PLAYER_PERMISSION.0, 0x021A);
        // 0x0220 is in the table only because the catalogue lists it; the client has no
        // handler and no sender for it.
        assert_eq!(
            Opcode(0x0220).name(),
            Some("Character_RemovePlayerPermission_UnusedCatalogueOpcode")
        );
    }

    #[test]
    fn an_unknown_opcode_has_no_row() {
        assert_eq!(Opcode(0xDEAD).info().map(|i| i.name), None);
    }
}
