//! Every opcode this client knows.
//!
//! **This file is the authority** for the opcode table: value, name, direction and the send and
//! receive queues of every message. It is maintained by hand. It was built from the client's own
//! switch statements and senders — not from the community catalogue — and the specification
//! (`docs/networking/messages/00-dispatch-and-queues.md` §7) points here rather than repeating it.
//! The `op!` declarations generate both constants and [`OPCODES`], sorted by value.
//! Four explicitly unlisted constants have no table row, so [`Opcode::name`] answers `None`
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

macro_rules! op {
    (
        rows { $( $(#[$attr:meta])* $constant:ident = $value:literal,
            $name:literal, $direction:expr, $send:expr, $recv:expr; )* }
        unlisted { $( $(#[$extra_attr:meta])* $extra:ident = $extra_value:literal; )* }
    ) => {
        impl Opcode {
            $( $(#[$attr])* pub const $constant: Self = Self($value); )*
            $( $(#[$extra_attr])* pub const $extra: Self = Self($extra_value); )*
        }

        /// The master opcode table, ascending by opcode so that [`Opcode::info`] can bisect it.
        pub static OPCODES: &[OpcodeInfo] = &[
            $( OpcodeInfo {
                opcode: Opcode::$constant,
                name: $name,
                direction: $direction,
                send_queue: $send,
                recv_queue: $recv,
            }, )*
        ];
    };
}

op! {
    rows {
        /// `0x0003` Allegiance_AllegianceUpdateAborted
        ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED = 0x0003, "Allegiance_AllegianceUpdateAborted", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0004` Communication_PopUpString
        COMMUNICATION_POP_UP_STRING = 0x0004, "Communication_PopUpString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0005` Character_PlayerOptionChangedEvent
        CHARACTER_PLAYER_OPTION_CHANGED_EVENT = 0x0005, "Character_PlayerOptionChangedEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0008` Combat_TargetedMeleeAttack
        COMBAT_TARGETED_MELEE_ATTACK = 0x0008, "Combat_TargetedMeleeAttack", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x000A` Combat_TargetedMissileAttack
        COMBAT_TARGETED_MISSILE_ATTACK = 0x000A, "Combat_TargetedMissileAttack", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x000F` Communication_SetAFKMode
        COMMUNICATION_SET_AFKMODE = 0x000F, "Communication_SetAFKMode", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0010` Communication_SetAFKMessage
        COMMUNICATION_SET_AFKMESSAGE = 0x0010, "Communication_SetAFKMessage", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0013` Login_PlayerDescription
        LOGIN_PLAYER_DESCRIPTION = 0x0013, "Login_PlayerDescription", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0015` Communication_Talk
        COMMUNICATION_TALK = 0x0015, "Communication_Talk", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0017` Social_RemoveFriend
        SOCIAL_REMOVE_FRIEND = 0x0017, "Social_RemoveFriend", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0018` Social_AddFriend
        SOCIAL_ADD_FRIEND = 0x0018, "Social_AddFriend", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0019` Inventory_PutItemInContainer
        INVENTORY_PUT_ITEM_IN_CONTAINER = 0x0019, "Inventory_PutItemInContainer", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x001A` Inventory_GetAndWieldItem
        INVENTORY_GET_AND_WIELD_ITEM = 0x001A, "Inventory_GetAndWieldItem", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x001B` Inventory_DropItem
        INVENTORY_DROP_ITEM = 0x001B, "Inventory_DropItem", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x001D` Allegiance_SwearAllegiance
        ALLEGIANCE_SWEAR_ALLEGIANCE = 0x001D, "Allegiance_SwearAllegiance", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x001E` Allegiance_BreakAllegiance
        ALLEGIANCE_BREAK_ALLEGIANCE = 0x001E, "Allegiance_BreakAllegiance", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x001F` Allegiance_UpdateRequest
        ALLEGIANCE_UPDATE_REQUEST = 0x001F, "Allegiance_UpdateRequest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0020` Allegiance_AllegianceUpdate
        ALLEGIANCE_ALLEGIANCE_UPDATE = 0x0020, "Allegiance_AllegianceUpdate", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0021` Social_FriendsUpdate
        SOCIAL_FRIENDS_UPDATE = 0x0021, "Social_FriendsUpdate", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0022` Item_ServerSaysContainID
        ITEM_SERVER_SAYS_CONTAIN_ID = 0x0022, "Item_ServerSaysContainID", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0023` Item_WearItem
        ITEM_WEAR_ITEM = 0x0023, "Item_WearItem", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0024` Item_ServerSaysRemove
        ITEM_SERVER_SAYS_REMOVE = 0x0024, "Item_ServerSaysRemove", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0025` Social_ClearFriends
        SOCIAL_CLEAR_FRIENDS = 0x0025, "Social_ClearFriends", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0026` Character_TeleToPKLArena
        CHARACTER_TELE_TO_PKLARENA = 0x0026, "Character_TeleToPKLArena", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0027` Character_TeleToPKArena
        CHARACTER_TELE_TO_PKARENA = 0x0027, "Character_TeleToPKArena", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0029` Social_CharacterTitleTable
        SOCIAL_CHARACTER_TITLE_TABLE = 0x0029, "Social_CharacterTitleTable", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x002B` Social_AddOrSetCharacterTitle
        SOCIAL_ADD_OR_SET_CHARACTER_TITLE = 0x002B, "Social_AddOrSetCharacterTitle", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x002C` Social_SetDisplayCharacterTitle
        SOCIAL_SET_DISPLAY_CHARACTER_TITLE = 0x002C, "Social_SetDisplayCharacterTitle", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0030` Allegiance_QueryAllegianceName
        ALLEGIANCE_QUERY_ALLEGIANCE_NAME = 0x0030, "Allegiance_QueryAllegianceName", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0031` Allegiance_ClearAllegianceName
        ALLEGIANCE_CLEAR_ALLEGIANCE_NAME = 0x0031, "Allegiance_ClearAllegianceName", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0032` Communication_TalkDirect
        COMMUNICATION_TALK_DIRECT = 0x0032, "Communication_TalkDirect", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0033` Allegiance_SetAllegianceName
        ALLEGIANCE_SET_ALLEGIANCE_NAME = 0x0033, "Allegiance_SetAllegianceName", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0035` Inventory_UseWithTargetEvent
        INVENTORY_USE_WITH_TARGET_EVENT = 0x0035, "Inventory_UseWithTargetEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0036` Inventory_UseEvent
        INVENTORY_USE_EVENT = 0x0036, "Inventory_UseEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x003B` Allegiance_SetAllegianceOfficer
        ALLEGIANCE_SET_ALLEGIANCE_OFFICER = 0x003B, "Allegiance_SetAllegianceOfficer", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x003C` Allegiance_SetAllegianceOfficerTitle
        ALLEGIANCE_SET_ALLEGIANCE_OFFICER_TITLE = 0x003C, "Allegiance_SetAllegianceOfficerTitle", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x003D` Allegiance_ListAllegianceOfficerTitles
        ALLEGIANCE_LIST_ALLEGIANCE_OFFICER_TITLES = 0x003D, "Allegiance_ListAllegianceOfficerTitles", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x003E` Allegiance_ClearAllegianceOfficerTitles
        ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICER_TITLES = 0x003E, "Allegiance_ClearAllegianceOfficerTitles", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x003F` Allegiance_DoAllegianceLockAction
        ALLEGIANCE_DO_ALLEGIANCE_LOCK_ACTION = 0x003F, "Allegiance_DoAllegianceLockAction", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0040` Allegiance_SetAllegianceApprovedVassal
        ALLEGIANCE_SET_ALLEGIANCE_APPROVED_VASSAL = 0x0040, "Allegiance_SetAllegianceApprovedVassal", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0041` Allegiance_AllegianceChatGag
        ALLEGIANCE_ALLEGIANCE_CHAT_GAG = 0x0041, "Allegiance_AllegianceChatGag", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0042` Allegiance_DoAllegianceHouseAction
        ALLEGIANCE_DO_ALLEGIANCE_HOUSE_ACTION = 0x0042, "Allegiance_DoAllegianceHouseAction", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0044` Train_TrainAttribute2nd
        TRAIN_TRAIN_ATTRIBUTE2ND = 0x0044, "Train_TrainAttribute2nd", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0045` Train_TrainAttribute
        TRAIN_TRAIN_ATTRIBUTE = 0x0045, "Train_TrainAttribute", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0046` Train_TrainSkill
        TRAIN_TRAIN_SKILL = 0x0046, "Train_TrainSkill", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0047` Train_TrainSkillAdvancementClass
        TRAIN_TRAIN_SKILL_ADVANCEMENT_CLASS = 0x0047, "Train_TrainSkillAdvancementClass", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0048` Magic_CastUntargetedSpell
        MAGIC_CAST_UNTARGETED_SPELL = 0x0048, "Magic_CastUntargetedSpell", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x004A` Magic_CastTargetedSpell
        MAGIC_CAST_TARGETED_SPELL = 0x004A, "Magic_CastTargetedSpell", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x004B` Magic_TestSpellFormula: a spell formula tried on a target. Clients up to
        /// January 2002 sent it from their spell research panel; the name is this table's, as the
        /// final client has no such action.
        MAGIC_TEST_SPELL_FORMULA = 0x004B, "Magic_TestSpellFormula", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0052` Item_StopViewingObjectContents
        ITEM_STOP_VIEWING_OBJECT_CONTENTS = 0x0052, "Item_StopViewingObjectContents", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0053` Combat_ChangeCombatMode
        COMBAT_CHANGE_COMBAT_MODE = 0x0053, "Combat_ChangeCombatMode", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0054` Inventory_StackableMerge
        INVENTORY_STACKABLE_MERGE = 0x0054, "Inventory_StackableMerge", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0055` Inventory_StackableSplitToContainer
        INVENTORY_STACKABLE_SPLIT_TO_CONTAINER = 0x0055, "Inventory_StackableSplitToContainer", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0056` Inventory_StackableSplitTo3D
        INVENTORY_STACKABLE_SPLIT_TO3_D = 0x0056, "Inventory_StackableSplitTo3D", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0058` Communication_ModifyCharacterSquelch
        COMMUNICATION_MODIFY_CHARACTER_SQUELCH = 0x0058, "Communication_ModifyCharacterSquelch", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0059` Communication_ModifyAccountSquelch
        COMMUNICATION_MODIFY_ACCOUNT_SQUELCH = 0x0059, "Communication_ModifyAccountSquelch", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x005B` Communication_ModifyGlobalSquelch
        COMMUNICATION_MODIFY_GLOBAL_SQUELCH = 0x005B, "Communication_ModifyGlobalSquelch", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x005D` Communication_TalkDirectByName
        COMMUNICATION_TALK_DIRECT_BY_NAME = 0x005D, "Communication_TalkDirectByName", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x005F` Vendor_Buy
        VENDOR_BUY = 0x005F, "Vendor_Buy", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0060` Vendor_Sell
        VENDOR_SELL = 0x0060, "Vendor_Sell", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0062` Vendor_VendorInfo
        VENDOR_VENDOR_INFO = 0x0062, "Vendor_VendorInfo", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0063` Character_TeleToLifestone
        CHARACTER_TELE_TO_LIFESTONE = 0x0063, "Character_TeleToLifestone", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0075` Character_StartBarber
        CHARACTER_START_BARBER = 0x0075, "Character_StartBarber", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00A0` Character_ServerSaysAttemptFailed
        CHARACTER_SERVER_SAYS_ATTEMPT_FAILED = 0x00A0, "Character_ServerSaysAttemptFailed", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00A1` Character_LoginCompleteNotification
        CHARACTER_LOGIN_COMPLETE_NOTIFICATION = 0x00A1, "Character_LoginCompleteNotification", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00A2` Fellowship_Create
        FELLOWSHIP_CREATE = 0x00A2, "Fellowship_Create", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00A3` Fellowship_Quit
        FELLOWSHIP_QUIT = 0x00A3, "Fellowship_Quit", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x00A4` Fellowship_Dismiss
        FELLOWSHIP_DISMISS = 0x00A4, "Fellowship_Dismiss", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x00A5` Fellowship_Recruit
        FELLOWSHIP_RECRUIT = 0x00A5, "Fellowship_Recruit", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00A6` Fellowship_UpdateRequest
        FELLOWSHIP_UPDATE_REQUEST = 0x00A6, "Fellowship_UpdateRequest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00AA` Writing_BookData
        WRITING_BOOK_DATA = 0x00AA, "Writing_BookData", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00AB` Writing_BookModifyPage
        WRITING_BOOK_MODIFY_PAGE = 0x00AB, "Writing_BookModifyPage", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00AC` Writing_BookAddPage
        WRITING_BOOK_ADD_PAGE = 0x00AC, "Writing_BookAddPage", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00AD` Writing_BookDeletePage
        WRITING_BOOK_DELETE_PAGE = 0x00AD, "Writing_BookDeletePage", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00AE` Writing_BookPageData
        WRITING_BOOK_PAGE_DATA = 0x00AE, "Writing_BookPageData", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00B4` Writing_BookOpen
        WRITING_BOOK_OPEN = 0x00B4, "Writing_BookOpen", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00B6` Writing_BookAddPageResponse
        WRITING_BOOK_ADD_PAGE_RESPONSE = 0x00B6, "Writing_BookAddPageResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00B7` Writing_BookDeletePageResponse
        WRITING_BOOK_DELETE_PAGE_RESPONSE = 0x00B7, "Writing_BookDeletePageResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00B8` Writing_BookPageDataResponse
        WRITING_BOOK_PAGE_DATA_RESPONSE = 0x00B8, "Writing_BookPageDataResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00BF` Writing_SetInscription
        WRITING_SET_INSCRIPTION = 0x00BF, "Writing_SetInscription", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00C3` Item_GetInscriptionResponse
        ITEM_GET_INSCRIPTION_RESPONSE = 0x00C3, "Item_GetInscriptionResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00C8` Item_Appraise
        ITEM_APPRAISE = 0x00C8, "Item_Appraise", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00C9` Item_SetAppraiseInfo
        ITEM_SET_APPRAISE_INFO = 0x00C9, "Item_SetAppraiseInfo", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x00CD` Inventory_GiveObjectRequest
        INVENTORY_GIVE_OBJECT_REQUEST = 0x00CD, "Inventory_GiveObjectRequest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x00D6` Advocate_Teleport
        ADVOCATE_TELEPORT = 0x00D6, "Advocate_Teleport", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0140` Character_AbuseLogRequest
        CHARACTER_ABUSE_LOG_REQUEST = 0x0140, "Character_AbuseLogRequest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0145` Communication_AddToChannel
        COMMUNICATION_ADD_TO_CHANNEL = 0x0145, "Communication_AddToChannel", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0146` Communication_RemoveFromChannel
        COMMUNICATION_REMOVE_FROM_CHANNEL = 0x0146, "Communication_RemoveFromChannel", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0147` Communication_ChannelBroadcast
        COMMUNICATION_CHANNEL_BROADCAST = 0x0147, "Communication_ChannelBroadcast", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x0148` Communication_ChannelList
        COMMUNICATION_CHANNEL_LIST = 0x0148, "Communication_ChannelList", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x0149` Communication_ChannelIndex
        COMMUNICATION_CHANNEL_INDEX = 0x0149, "Communication_ChannelIndex", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x0195` Inventory_NoLongerViewingContents
        INVENTORY_NO_LONGER_VIEWING_CONTENTS = 0x0195, "Inventory_NoLongerViewingContents", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0196` Item_OnViewContents
        ITEM_ON_VIEW_CONTENTS = 0x0196, "Item_OnViewContents", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0197` Item_UpdateStackSize
        ITEM_UPDATE_STACK_SIZE = 0x0197, "Item_UpdateStackSize", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x019A` Item_ServerSaysMoveItem
        ITEM_SERVER_SAYS_MOVE_ITEM = 0x019A, "Item_ServerSaysMoveItem", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x019B` Inventory_StackableSplitToWield
        INVENTORY_STACKABLE_SPLIT_TO_WIELD = 0x019B, "Inventory_StackableSplitToWield", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x019C` Character_AddShortCut
        CHARACTER_ADD_SHORT_CUT = 0x019C, "Character_AddShortCut", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x019D` Character_RemoveShortCut
        CHARACTER_REMOVE_SHORT_CUT = 0x019D, "Character_RemoveShortCut", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x019E` Combat_HandlePlayerDeathEvent
        COMBAT_HANDLE_PLAYER_DEATH_EVENT = 0x019E, "Combat_HandlePlayerDeathEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01A1` Character_CharacterOptionsEvent
        CHARACTER_CHARACTER_OPTIONS_EVENT = 0x01A1, "Character_CharacterOptionsEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01A7` Combat_HandleAttackDoneEvent
        COMBAT_HANDLE_ATTACK_DONE_EVENT = 0x01A7, "Combat_HandleAttackDoneEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01A8` Magic_RemoveSpell
        MAGIC_REMOVE_SPELL = 0x01A8, "Magic_RemoveSpell", Direction::Both, Some(NetQueue::Weenie), Some(NetQueue::UiQueue);
        /// `0x01AC` Combat_HandleVictimNotificationEventSelf
        COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_SELF = 0x01AC, "Combat_HandleVictimNotificationEventSelf", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01AD` Combat_HandleVictimNotificationEventOther
        COMBAT_HANDLE_VICTIM_NOTIFICATION_EVENT_OTHER = 0x01AD, "Combat_HandleVictimNotificationEventOther", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01B1` Combat_HandleAttackerNotificationEvent
        COMBAT_HANDLE_ATTACKER_NOTIFICATION_EVENT = 0x01B1, "Combat_HandleAttackerNotificationEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01B2` Combat_HandleDefenderNotificationEvent
        COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT = 0x01B2, "Combat_HandleDefenderNotificationEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01B3` Combat_HandleEvasionAttackerNotificationEvent
        COMBAT_HANDLE_EVASION_ATTACKER_NOTIFICATION_EVENT = 0x01B3, "Combat_HandleEvasionAttackerNotificationEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01B4` Combat_HandleEvasionDefenderNotificationEvent
        COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT = 0x01B4, "Combat_HandleEvasionDefenderNotificationEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01B7` Combat_CancelAttack
        COMBAT_CANCEL_ATTACK = 0x01B7, "Combat_CancelAttack", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01B8` Combat_HandleCommenceAttackEvent
        COMBAT_HANDLE_COMMENCE_ATTACK_EVENT = 0x01B8, "Combat_HandleCommenceAttackEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01BF` Combat_QueryHealth
        COMBAT_QUERY_HEALTH = 0x01BF, "Combat_QueryHealth", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01C0` Combat_QueryHealthResponse
        COMBAT_QUERY_HEALTH_RESPONSE = 0x01C0, "Combat_QueryHealthResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01C2` Character_QueryAge
        CHARACTER_QUERY_AGE = 0x01C2, "Character_QueryAge", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01C3` Character_QueryAgeResponse
        CHARACTER_QUERY_AGE_RESPONSE = 0x01C3, "Character_QueryAgeResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01C4` Character_QueryBirth
        CHARACTER_QUERY_BIRTH = 0x01C4, "Character_QueryBirth", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01C7` Item_UseDone
        ITEM_USE_DONE = 0x01C7, "Item_UseDone", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01C9` Fellowship_FellowUpdateDone
        FELLOWSHIP_FELLOW_UPDATE_DONE = 0x01C9, "Fellowship_FellowUpdateDone", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01CA` Fellowship_FellowStatsDone
        FELLOWSHIP_FELLOW_STATS_DONE = 0x01CA, "Fellowship_FellowStatsDone", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01CB` Item_AppraiseDone
        ITEM_APPRAISE_DONE = 0x01CB, "Item_AppraiseDone", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D1` Qualities_PrivateRemoveIntEvent
        QUALITIES_PRIVATE_REMOVE_INT_EVENT = 0x01D1, "Qualities_PrivateRemoveIntEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D2` Qualities_RemoveIntEvent
        QUALITIES_REMOVE_INT_EVENT = 0x01D2, "Qualities_RemoveIntEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D3` Qualities_PrivateRemoveBoolEvent
        QUALITIES_PRIVATE_REMOVE_BOOL_EVENT = 0x01D3, "Qualities_PrivateRemoveBoolEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D4` Qualities_RemoveBoolEvent
        QUALITIES_REMOVE_BOOL_EVENT = 0x01D4, "Qualities_RemoveBoolEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D5` Qualities_PrivateRemoveFloatEvent
        QUALITIES_PRIVATE_REMOVE_FLOAT_EVENT = 0x01D5, "Qualities_PrivateRemoveFloatEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D6` Qualities_RemoveFloatEvent
        QUALITIES_REMOVE_FLOAT_EVENT = 0x01D6, "Qualities_RemoveFloatEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D7` Qualities_PrivateRemoveStringEvent
        QUALITIES_PRIVATE_REMOVE_STRING_EVENT = 0x01D7, "Qualities_PrivateRemoveStringEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D8` Qualities_RemoveStringEvent
        QUALITIES_REMOVE_STRING_EVENT = 0x01D8, "Qualities_RemoveStringEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01D9` Qualities_PrivateRemoveDataIDEvent
        QUALITIES_PRIVATE_REMOVE_DATA_IDEVENT = 0x01D9, "Qualities_PrivateRemoveDataIDEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DA` Qualities_RemoveDataIDEvent
        QUALITIES_REMOVE_DATA_IDEVENT = 0x01DA, "Qualities_RemoveDataIDEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DB` Qualities_PrivateRemoveInstanceIDEvent
        QUALITIES_PRIVATE_REMOVE_INSTANCE_IDEVENT = 0x01DB, "Qualities_PrivateRemoveInstanceIDEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DC` Qualities_RemoveInstanceIDEvent
        QUALITIES_REMOVE_INSTANCE_IDEVENT = 0x01DC, "Qualities_RemoveInstanceIDEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DD` Qualities_PrivateRemovePositionEvent
        QUALITIES_PRIVATE_REMOVE_POSITION_EVENT = 0x01DD, "Qualities_PrivateRemovePositionEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DE` Qualities_RemovePositionEvent
        QUALITIES_REMOVE_POSITION_EVENT = 0x01DE, "Qualities_RemovePositionEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01DF` Communication_Emote
        COMMUNICATION_EMOTE = 0x01DF, "Communication_Emote", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01E0` Communication_HearEmote
        COMMUNICATION_HEAR_EMOTE = 0x01E0, "Communication_HearEmote", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01E1` Communication_SoulEmote
        COMMUNICATION_SOUL_EMOTE = 0x01E1, "Communication_SoulEmote", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01E2` Communication_HearSoulEmote
        COMMUNICATION_HEAR_SOUL_EMOTE = 0x01E2, "Communication_HearSoulEmote", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01E3` Character_AddSpellFavorite
        CHARACTER_ADD_SPELL_FAVORITE = 0x01E3, "Character_AddSpellFavorite", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01E4` Character_RemoveSpellFavorite
        CHARACTER_REMOVE_SPELL_FAVORITE = 0x01E4, "Character_RemoveSpellFavorite", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01E9` Character_RequestPing
        CHARACTER_REQUEST_PING = 0x01E9, "Character_RequestPing", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01EA` Character_ReturnPing
        CHARACTER_RETURN_PING = 0x01EA, "Character_ReturnPing", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01F4` Communication_SetSquelchDB
        COMMUNICATION_SET_SQUELCH_DB = 0x01F4, "Communication_SetSquelchDB", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01F6` Trade_OpenTradeNegotiations
        TRADE_OPEN_TRADE_NEGOTIATIONS = 0x01F6, "Trade_OpenTradeNegotiations", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01F7` Trade_CloseTradeNegotiations
        TRADE_CLOSE_TRADE_NEGOTIATIONS = 0x01F7, "Trade_CloseTradeNegotiations", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01F8` Trade_AddToTrade
        TRADE_ADD_TO_TRADE = 0x01F8, "Trade_AddToTrade", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01FA` Trade_AcceptTrade
        TRADE_ACCEPT_TRADE = 0x01FA, "Trade_AcceptTrade", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01FB` Trade_DeclineTrade
        TRADE_DECLINE_TRADE = 0x01FB, "Trade_DeclineTrade", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x01FD` Trade_RegisterTrade
        TRADE_REGISTER_TRADE = 0x01FD, "Trade_RegisterTrade", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01FE` Trade_OpenTrade
        TRADE_OPEN_TRADE = 0x01FE, "Trade_OpenTrade", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x01FF` Trade_CloseTrade
        TRADE_CLOSE_TRADE = 0x01FF, "Trade_CloseTrade", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0200` Trade_AddToTrade_Recv
        TRADE_ADD_TO_TRADE_RECV = 0x0200, "Trade_AddToTrade_Recv", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0201` Trade_RemoveFromTrade
        TRADE_REMOVE_FROM_TRADE = 0x0201, "Trade_RemoveFromTrade", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0202` Trade_AcceptTrade_Recv
        TRADE_ACCEPT_TRADE_RECV = 0x0202, "Trade_AcceptTrade_Recv", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0203` Trade_DeclineTrade_Recv
        TRADE_DECLINE_TRADE_RECV = 0x0203, "Trade_DeclineTrade_Recv", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0204` Trade_ResetTrade
        TRADE_RESET_TRADE = 0x0204, "Trade_ResetTrade", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0205` Trade_ResetTrade_Recv
        TRADE_RESET_TRADE_RECV = 0x0205, "Trade_ResetTrade_Recv", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0207` Trade_TradeFailure
        TRADE_TRADE_FAILURE = 0x0207, "Trade_TradeFailure", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0208` Trade_ClearTradeAcceptance
        TRADE_CLEAR_TRADE_ACCEPTANCE = 0x0208, "Trade_ClearTradeAcceptance", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0216` Character_ClearPlayerConsentList
        CHARACTER_CLEAR_PLAYER_CONSENT_LIST = 0x0216, "Character_ClearPlayerConsentList", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0217` Character_DisplayPlayerConsentList
        CHARACTER_DISPLAY_PLAYER_CONSENT_LIST = 0x0217, "Character_DisplayPlayerConsentList", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0218` Character_RemoveFromPlayerConsentList
        CHARACTER_REMOVE_FROM_PLAYER_CONSENT_LIST = 0x0218, "Character_RemoveFromPlayerConsentList", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0219` Character_AddPlayerPermission
        CHARACTER_ADD_PLAYER_PERMISSION = 0x0219, "Character_AddPlayerPermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x021A` Character_RemovePlayerPermission
        CHARACTER_REMOVE_PLAYER_PERMISSION = 0x021A, "Character_RemovePlayerPermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x021C` House_BuyHouse
        HOUSE_BUY_HOUSE = 0x021C, "House_BuyHouse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x021D` House_HouseProfile
        HOUSE_HOUSE_PROFILE = 0x021D, "House_HouseProfile", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x021E` House_QueryHouse
        HOUSE_QUERY_HOUSE = 0x021E, "House_QueryHouse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x021F` House_AbandonHouse
        HOUSE_ABANDON_HOUSE = 0x021F, "House_AbandonHouse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0220` Character_RemovePlayerPermission_UnusedCatalogueOpcode
        CHARACTER_REMOVE_PLAYER_PERMISSION_UNUSED_CATALOGUE_OPCODE = 0x0220, "Character_RemovePlayerPermission_UnusedCatalogueOpcode", Direction::C2S, None, None;
        /// `0x0221` House_RentHouse
        HOUSE_RENT_HOUSE = 0x0221, "House_RentHouse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0224` Character_SetDesiredComponentLevel
        CHARACTER_SET_DESIRED_COMPONENT_LEVEL = 0x0224, "Character_SetDesiredComponentLevel", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0225` House_HouseData
        HOUSE_HOUSE_DATA = 0x0225, "House_HouseData", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0226` House_HouseStatus
        HOUSE_HOUSE_STATUS = 0x0226, "House_HouseStatus", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0227` House_UpdateRentTime
        HOUSE_UPDATE_RENT_TIME = 0x0227, "House_UpdateRentTime", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0228` House_UpdateRentPayment
        HOUSE_UPDATE_RENT_PAYMENT = 0x0228, "House_UpdateRentPayment", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0245` House_AddPermanentGuest
        HOUSE_ADD_PERMANENT_GUEST = 0x0245, "House_AddPermanentGuest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0246` House_RemovePermanentGuest
        HOUSE_REMOVE_PERMANENT_GUEST = 0x0246, "House_RemovePermanentGuest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0247` House_SetOpenHouseStatus
        HOUSE_SET_OPEN_HOUSE_STATUS = 0x0247, "House_SetOpenHouseStatus", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0248` House_UpdateRestrictions
        HOUSE_UPDATE_RESTRICTIONS = 0x0248, "House_UpdateRestrictions", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0249` House_ChangeStoragePermission
        HOUSE_CHANGE_STORAGE_PERMISSION = 0x0249, "House_ChangeStoragePermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x024A` House_BootSpecificHouseGuest
        HOUSE_BOOT_SPECIFIC_HOUSE_GUEST = 0x024A, "House_BootSpecificHouseGuest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x024C` House_RemoveAllStoragePermission
        HOUSE_REMOVE_ALL_STORAGE_PERMISSION = 0x024C, "House_RemoveAllStoragePermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x024D` House_RequestFullGuestList
        HOUSE_REQUEST_FULL_GUEST_LIST = 0x024D, "House_RequestFullGuestList", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0254` Allegiance_SetMotd
        ALLEGIANCE_SET_MOTD = 0x0254, "Allegiance_SetMotd", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0255` Allegiance_QueryMotd
        ALLEGIANCE_QUERY_MOTD = 0x0255, "Allegiance_QueryMotd", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0256` Allegiance_ClearMotd
        ALLEGIANCE_CLEAR_MOTD = 0x0256, "Allegiance_ClearMotd", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0257` House_UpdateHAR
        HOUSE_UPDATE_HAR = 0x0257, "House_UpdateHAR", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0258` House_QueryLord
        HOUSE_QUERY_LORD = 0x0258, "House_QueryLord", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0259` House_HouseTransaction
        HOUSE_HOUSE_TRANSACTION = 0x0259, "House_HouseTransaction", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x025C` House_AddAllStoragePermission
        HOUSE_ADD_ALL_STORAGE_PERMISSION = 0x025C, "House_AddAllStoragePermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x025E` House_RemoveAllPermanentGuests
        HOUSE_REMOVE_ALL_PERMANENT_GUESTS = 0x025E, "House_RemoveAllPermanentGuests", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x025F` House_BootEveryone
        HOUSE_BOOT_EVERYONE = 0x025F, "House_BootEveryone", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0262` House_TeleToHouse
        HOUSE_TELE_TO_HOUSE = 0x0262, "House_TeleToHouse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0263` Item_QueryItemMana
        ITEM_QUERY_ITEM_MANA = 0x0263, "Item_QueryItemMana", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0264` Item_QueryItemManaResponse
        ITEM_QUERY_ITEM_MANA_RESPONSE = 0x0264, "Item_QueryItemManaResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0266` House_SetHooksVisibility
        HOUSE_SET_HOOKS_VISIBILITY = 0x0266, "House_SetHooksVisibility", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0267` House_ModifyAllegianceGuestPermission
        HOUSE_MODIFY_ALLEGIANCE_GUEST_PERMISSION = 0x0267, "House_ModifyAllegianceGuestPermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0268` House_ModifyAllegianceStoragePermission
        HOUSE_MODIFY_ALLEGIANCE_STORAGE_PERMISSION = 0x0268, "House_ModifyAllegianceStoragePermission", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0269` Game_Join
        GAME_JOIN = 0x0269, "Game_Join", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x026A` Game_Quit
        GAME_QUIT = 0x026A, "Game_Quit", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x026B` Game_Move
        GAME_MOVE = 0x026B, "Game_Move", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x026D` Game_MovePass
        GAME_MOVE_PASS = 0x026D, "Game_MovePass", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x026E` Game_Stalemate
        GAME_STALEMATE = 0x026E, "Game_Stalemate", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0270` House_ListAvailableHouses
        HOUSE_LIST_AVAILABLE_HOUSES = 0x0270, "House_ListAvailableHouses", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0271` House_AvailableHouses
        HOUSE_AVAILABLE_HOUSES = 0x0271, "House_AvailableHouses", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0274` Character_ConfirmationRequest
        CHARACTER_CONFIRMATION_REQUEST = 0x0274, "Character_ConfirmationRequest", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0275` Character_ConfirmationResponse
        CHARACTER_CONFIRMATION_RESPONSE = 0x0275, "Character_ConfirmationResponse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0276` Character_ConfirmationDone
        CHARACTER_CONFIRMATION_DONE = 0x0276, "Character_ConfirmationDone", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0277` Allegiance_BreakAllegianceBoot
        ALLEGIANCE_BREAK_ALLEGIANCE_BOOT = 0x0277, "Allegiance_BreakAllegianceBoot", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0278` House_TeleToMansion
        HOUSE_TELE_TO_MANSION = 0x0278, "House_TeleToMansion", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0279` Character_Suicide
        CHARACTER_SUICIDE = 0x0279, "Character_Suicide", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x027A` Allegiance_AllegianceLoginNotificationEvent
        ALLEGIANCE_ALLEGIANCE_LOGIN_NOTIFICATION_EVENT = 0x027A, "Allegiance_AllegianceLoginNotificationEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x027B` Allegiance_AllegianceInfoRequest
        ALLEGIANCE_ALLEGIANCE_INFO_REQUEST = 0x027B, "Allegiance_AllegianceInfoRequest", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x027C` Allegiance_AllegianceInfoResponseEvent
        ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT = 0x027C, "Allegiance_AllegianceInfoResponseEvent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x027D` Inventory_CreateTinkeringTool
        INVENTORY_CREATE_TINKERING_TOOL = 0x027D, "Inventory_CreateTinkeringTool", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0281` Game_JoinGameResponse
        GAME_JOIN_GAME_RESPONSE = 0x0281, "Game_JoinGameResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0282` Game_StartGame
        GAME_START_GAME = 0x0282, "Game_StartGame", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0283` Game_MoveResponse
        GAME_MOVE_RESPONSE = 0x0283, "Game_MoveResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0284` Game_OpponentTurn
        GAME_OPPONENT_TURN = 0x0284, "Game_OpponentTurn", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0285` Game_OpponentStalemateState
        GAME_OPPONENT_STALEMATE_STATE = 0x0285, "Game_OpponentStalemateState", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0286` Character_SpellbookFilterEvent
        CHARACTER_SPELLBOOK_FILTER_EVENT = 0x0286, "Character_SpellbookFilterEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x028A` Communication_WeenieError
        COMMUNICATION_WEENIE_ERROR = 0x028A, "Communication_WeenieError", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x028B` Communication_WeenieErrorWithString
        COMMUNICATION_WEENIE_ERROR_WITH_STRING = 0x028B, "Communication_WeenieErrorWithString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x028C` Game_GameOver
        GAME_GAME_OVER = 0x028C, "Game_GameOver", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x028D` Character_TeleToMarketplace
        CHARACTER_TELE_TO_MARKETPLACE = 0x028D, "Character_TeleToMarketplace", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x028F` Character_EnterPKLite
        CHARACTER_ENTER_PKLITE = 0x028F, "Character_EnterPKLite", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0290` Fellowship_AssignNewLeader
        FELLOWSHIP_ASSIGN_NEW_LEADER = 0x0290, "Fellowship_AssignNewLeader", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0291` changes fellowship openness.
        FELLOWSHIP_CHANGE_FELLOW_OPENNESS = 0x0291, "Fellowship_ChangeFellowOpenness", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0295` Communication_ChatRoomTracker
        COMMUNICATION_CHAT_ROOM_TRACKER = 0x0295, "Communication_ChatRoomTracker", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02A0` Allegiance_AllegianceChatBoot
        ALLEGIANCE_ALLEGIANCE_CHAT_BOOT = 0x02A0, "Allegiance_AllegianceChatBoot", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A1` Allegiance_AddAllegianceBan
        ALLEGIANCE_ADD_ALLEGIANCE_BAN = 0x02A1, "Allegiance_AddAllegianceBan", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A2` Allegiance_RemoveAllegianceBan
        ALLEGIANCE_REMOVE_ALLEGIANCE_BAN = 0x02A2, "Allegiance_RemoveAllegianceBan", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A3` Allegiance_ListAllegianceBans
        ALLEGIANCE_LIST_ALLEGIANCE_BANS = 0x02A3, "Allegiance_ListAllegianceBans", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A5` Allegiance_RemoveAllegianceOfficer
        ALLEGIANCE_REMOVE_ALLEGIANCE_OFFICER = 0x02A5, "Allegiance_RemoveAllegianceOfficer", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A6` Allegiance_ListAllegianceOfficers
        ALLEGIANCE_LIST_ALLEGIANCE_OFFICERS = 0x02A6, "Allegiance_ListAllegianceOfficers", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02A7` Allegiance_ClearAllegianceOfficers
        ALLEGIANCE_CLEAR_ALLEGIANCE_OFFICERS = 0x02A7, "Allegiance_ClearAllegianceOfficers", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02AB` Allegiance_RecallAllegianceHometown
        ALLEGIANCE_RECALL_ALLEGIANCE_HOMETOWN = 0x02AB, "Allegiance_RecallAllegianceHometown", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02AE` Admin_QueryPluginList
        ADMIN_QUERY_PLUGIN_LIST = 0x02AE, "Admin_QueryPluginList", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02AF` Admin_QueryPluginListResponse
        ADMIN_QUERY_PLUGIN_LIST_RESPONSE = 0x02AF, "Admin_QueryPluginListResponse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02B1` Admin_QueryPlugin
        ADMIN_QUERY_PLUGIN = 0x02B1, "Admin_QueryPlugin", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02B2` Admin_QueryPluginResponse
        ADMIN_QUERY_PLUGIN_RESPONSE = 0x02B2, "Admin_QueryPluginResponse", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x02B3` Admin_QueryPluginResponse_Recv
        ADMIN_QUERY_PLUGIN_RESPONSE_RECV = 0x02B3, "Admin_QueryPluginResponse_Recv", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02B4` Inventory_SalvageOperationsResultData
        INVENTORY_SALVAGE_OPERATIONS_RESULT_DATA = 0x02B4, "Inventory_SalvageOperationsResultData", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02B8` Qualities_PrivateRemoveInt64Event
        QUALITIES_PRIVATE_REMOVE_INT64_EVENT = 0x02B8, "Qualities_PrivateRemoveInt64Event", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02B9` Qualities_RemoveInt64Event
        QUALITIES_REMOVE_INT64_EVENT = 0x02B9, "Qualities_RemoveInt64Event", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02BB` Communication_HearSpeech
        COMMUNICATION_HEAR_SPEECH = 0x02BB, "Communication_HearSpeech", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02BC` Communication_HearRangedSpeech
        COMMUNICATION_HEAR_RANGED_SPEECH = 0x02BC, "Communication_HearRangedSpeech", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02BD` Communication_HearDirectSpeech
        COMMUNICATION_HEAR_DIRECT_SPEECH = 0x02BD, "Communication_HearDirectSpeech", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02BE` Fellowship_FullUpdate
        FELLOWSHIP_FULL_UPDATE = 0x02BE, "Fellowship_FullUpdate", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02BF` Fellowship_Disband
        FELLOWSHIP_DISBAND = 0x02BF, "Fellowship_Disband", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C0` Fellowship_UpdateFellow
        FELLOWSHIP_UPDATE_FELLOW = 0x02C0, "Fellowship_UpdateFellow", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C1` Magic_UpdateSpell
        MAGIC_UPDATE_SPELL = 0x02C1, "Magic_UpdateSpell", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C2` Magic_UpdateEnchantment
        MAGIC_UPDATE_ENCHANTMENT = 0x02C2, "Magic_UpdateEnchantment", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C3` Magic_RemoveEnchantment
        MAGIC_REMOVE_ENCHANTMENT = 0x02C3, "Magic_RemoveEnchantment", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C4` Magic_UpdateMultipleEnchantments
        MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS = 0x02C4, "Magic_UpdateMultipleEnchantments", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C5` Magic_RemoveMultipleEnchantments
        MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS = 0x02C5, "Magic_RemoveMultipleEnchantments", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C6` Magic_PurgeEnchantments
        MAGIC_PURGE_ENCHANTMENTS = 0x02C6, "Magic_PurgeEnchantments", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C7` Magic_DispelEnchantment
        MAGIC_DISPEL_ENCHANTMENT = 0x02C7, "Magic_DispelEnchantment", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C8` Magic_DispelMultipleEnchantments
        MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS = 0x02C8, "Magic_DispelMultipleEnchantments", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02C9` Misc_PortalStormBrewing
        MISC_PORTAL_STORM_BREWING = 0x02C9, "Misc_PortalStormBrewing", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CA` Misc_PortalStormImminent
        MISC_PORTAL_STORM_IMMINENT = 0x02CA, "Misc_PortalStormImminent", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CB` Misc_PortalStorm
        MISC_PORTAL_STORM = 0x02CB, "Misc_PortalStorm", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CC` Misc_PortalStormSubsided
        MISC_PORTAL_STORM_SUBSIDED = 0x02CC, "Misc_PortalStormSubsided", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CD` Qualities_PrivateUpdateInt
        QUALITIES_PRIVATE_UPDATE_INT = 0x02CD, "Qualities_PrivateUpdateInt", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CE` Qualities_UpdateInt
        QUALITIES_UPDATE_INT = 0x02CE, "Qualities_UpdateInt", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02CF` Qualities_PrivateUpdateInt64
        QUALITIES_PRIVATE_UPDATE_INT64 = 0x02CF, "Qualities_PrivateUpdateInt64", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D0` Qualities_UpdateInt64
        QUALITIES_UPDATE_INT64 = 0x02D0, "Qualities_UpdateInt64", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D1` Qualities_PrivateUpdateBool
        QUALITIES_PRIVATE_UPDATE_BOOL = 0x02D1, "Qualities_PrivateUpdateBool", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D2` Qualities_UpdateBool
        QUALITIES_UPDATE_BOOL = 0x02D2, "Qualities_UpdateBool", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D3` Qualities_PrivateUpdateFloat
        QUALITIES_PRIVATE_UPDATE_FLOAT = 0x02D3, "Qualities_PrivateUpdateFloat", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D4` Qualities_UpdateFloat
        QUALITIES_UPDATE_FLOAT = 0x02D4, "Qualities_UpdateFloat", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D5` Qualities_PrivateUpdateString
        QUALITIES_PRIVATE_UPDATE_STRING = 0x02D5, "Qualities_PrivateUpdateString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D6` Qualities_UpdateString
        QUALITIES_UPDATE_STRING = 0x02D6, "Qualities_UpdateString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D7` Qualities_PrivateUpdateDataID
        QUALITIES_PRIVATE_UPDATE_DATA_ID = 0x02D7, "Qualities_PrivateUpdateDataID", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D8` Qualities_UpdateDataID
        QUALITIES_UPDATE_DATA_ID = 0x02D8, "Qualities_UpdateDataID", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02D9` Qualities_PrivateUpdateInstanceID
        QUALITIES_PRIVATE_UPDATE_INSTANCE_ID = 0x02D9, "Qualities_PrivateUpdateInstanceID", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DA` Qualities_UpdateInstanceID
        QUALITIES_UPDATE_INSTANCE_ID = 0x02DA, "Qualities_UpdateInstanceID", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DB` Qualities_PrivateUpdatePosition
        QUALITIES_PRIVATE_UPDATE_POSITION = 0x02DB, "Qualities_PrivateUpdatePosition", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DC` Qualities_UpdatePosition
        QUALITIES_UPDATE_POSITION = 0x02DC, "Qualities_UpdatePosition", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DD` Qualities_PrivateUpdateSkill
        QUALITIES_PRIVATE_UPDATE_SKILL = 0x02DD, "Qualities_PrivateUpdateSkill", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DE` Qualities_UpdateSkill
        QUALITIES_UPDATE_SKILL = 0x02DE, "Qualities_UpdateSkill", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02DF` Qualities_PrivateUpdateSkillLevel
        QUALITIES_PRIVATE_UPDATE_SKILL_LEVEL = 0x02DF, "Qualities_PrivateUpdateSkillLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E0` Qualities_UpdateSkillLevel
        QUALITIES_UPDATE_SKILL_LEVEL = 0x02E0, "Qualities_UpdateSkillLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E1` Qualities_PrivateUpdateSkillAC
        QUALITIES_PRIVATE_UPDATE_SKILL_AC = 0x02E1, "Qualities_PrivateUpdateSkillAC", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E2` Qualities_UpdateSkillAC
        QUALITIES_UPDATE_SKILL_AC = 0x02E2, "Qualities_UpdateSkillAC", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E3` Qualities_PrivateUpdateAttribute
        QUALITIES_PRIVATE_UPDATE_ATTRIBUTE = 0x02E3, "Qualities_PrivateUpdateAttribute", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E4` Qualities_UpdateAttribute
        QUALITIES_UPDATE_ATTRIBUTE = 0x02E4, "Qualities_UpdateAttribute", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E5` Qualities_PrivateUpdateAttributeLevel
        QUALITIES_PRIVATE_UPDATE_ATTRIBUTE_LEVEL = 0x02E5, "Qualities_PrivateUpdateAttributeLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E6` Qualities_UpdateAttributeLevel
        QUALITIES_UPDATE_ATTRIBUTE_LEVEL = 0x02E6, "Qualities_UpdateAttributeLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E7` Qualities_PrivateUpdateAttribute2nd
        QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND = 0x02E7, "Qualities_PrivateUpdateAttribute2nd", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E8` Qualities_UpdateAttribute2nd
        QUALITIES_UPDATE_ATTRIBUTE2ND = 0x02E8, "Qualities_UpdateAttribute2nd", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02E9` Qualities_PrivateUpdateAttribute2ndLevel
        QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND_LEVEL = 0x02E9, "Qualities_PrivateUpdateAttribute2ndLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02EA` Qualities_UpdateAttribute2ndLevel
        QUALITIES_UPDATE_ATTRIBUTE2ND_LEVEL = 0x02EA, "Qualities_UpdateAttribute2ndLevel", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x02EB` Communication_TransientString
        COMMUNICATION_TRANSIENT_STRING = 0x02EB, "Communication_TransientString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0311` Character_FinishBarber
        CHARACTER_FINISH_BARBER = 0x0311, "Character_FinishBarber", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0x0312` Magic_PurgeBadEnchantments
        MAGIC_PURGE_BAD_ENCHANTMENTS = 0x0312, "Magic_PurgeBadEnchantments", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0314` Social_SendClientContractTrackerTable
        SOCIAL_SEND_CLIENT_CONTRACT_TRACKER_TABLE = 0x0314, "Social_SendClientContractTrackerTable", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0315` Social_SendClientContractTracker
        SOCIAL_SEND_CLIENT_CONTRACT_TRACKER = 0x0315, "Social_SendClientContractTracker", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0x0316` Social_AbandonContract
        SOCIAL_ABANDON_CONTRACT = 0x0316, "Social_AbandonContract", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xEA60` Admin_Environs
        ADMIN_ENVIRONS = 0xEA60, "Admin_Environs", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF619` Movement_PositionAndMovementEvent
        MOVEMENT_POSITION_AND_MOVEMENT_EVENT = 0xF619, "Movement_PositionAndMovementEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF61B` Movement_Jump
        MOVEMENT_JUMP = 0xF61B, "Movement_Jump", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF61C` Movement_MoveToState
        MOVEMENT_MOVE_TO_STATE = 0xF61C, "Movement_MoveToState", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF61E` Movement_DoMovementCommand
        MOVEMENT_DO_MOVEMENT_COMMAND = 0xF61E, "Movement_DoMovementCommand", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF625` Item_ObjDescEvent
        ITEM_OBJ_DESC_EVENT = 0xF625, "Item_ObjDescEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF630` Character_SetPlayerVisualDesc
        CHARACTER_SET_PLAYER_VISUAL_DESC = 0xF630, "Character_SetPlayerVisualDesc", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF643` Character_CharGenVerificationResponse
        CHARACTER_CHAR_GEN_VERIFICATION_RESPONSE = 0xF643, "Character_CharGenVerificationResponse", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF649` Movement_TurnToEvent
        MOVEMENT_TURN_TO_EVENT = 0xF649, "Movement_TurnToEvent", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF651` Login_AwaitingSubscriptionExpiration
        LOGIN_AWAITING_SUBSCRIPTION_EXPIRATION = 0xF651, "Login_AwaitingSubscriptionExpiration", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF653` Login_ExecuteLogOff
        LOGIN_EXECUTE_LOG_OFF = 0xF653, "Login_ExecuteLogOff", Direction::Both, Some(NetQueue::Logon), Some(NetQueue::UiQueue);
        /// `0xF655` Character_CharacterDelete
        CHARACTER_CHARACTER_DELETE = 0xF655, "Character_CharacterDelete", Direction::Both, Some(NetQueue::Logon), Some(NetQueue::UiQueue);
        /// `0xF656` Character_SendCharGenResult
        CHARACTER_SEND_CHAR_GEN_RESULT = 0xF656, "Character_SendCharGenResult", Direction::C2S, Some(NetQueue::Logon), None;
        /// `0xF657` Login_SendEnterWorld
        LOGIN_SEND_ENTER_WORLD = 0xF657, "Login_SendEnterWorld", Direction::C2S, Some(NetQueue::Logon), None;
        /// `0xF658` Login_LoginCharacterSet
        LOGIN_LOGIN_CHARACTER_SET = 0xF658, "Login_LoginCharacterSet", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF659` Character_CharacterError
        CHARACTER_CHARACTER_ERROR = 0xF659, "Character_CharacterError", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF65A` Login_CharacterScreenMessage: the text the character screen's message box shows.
        /// Clients before Throne of Destiny showed it; the end-of-retail client has no such message,
        /// so the name is this table's.
        LOGIN_CHARACTER_SCREEN_MESSAGE = 0xF65A, "Login_CharacterScreenMessage", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF661` Movement_StopMovementCommand
        MOVEMENT_STOP_MOVEMENT_COMMAND = 0xF661, "Movement_StopMovementCommand", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF6EA` Object_SendForceObjdesc
        OBJECT_SEND_FORCE_OBJDESC = 0xF6EA, "Object_SendForceObjdesc", Direction::C2S, Some(NetQueue::Control), None;
        /// `0xF745` Item_CreateObject
        ITEM_CREATE_OBJECT = 0xF745, "Item_CreateObject", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF746` Login_CreatePlayer
        LOGIN_CREATE_PLAYER = 0xF746, "Login_CreatePlayer", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF747` Item_DeleteObject
        ITEM_DELETE_OBJECT = 0xF747, "Item_DeleteObject", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF748` Movement_PositionEvent
        MOVEMENT_POSITION_EVENT = 0xF748, "Movement_PositionEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF749` Item_ParentEvent
        ITEM_PARENT_EVENT = 0xF749, "Item_ParentEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF74A` Inventory_PickupEvent
        INVENTORY_PICKUP_EVENT = 0xF74A, "Inventory_PickupEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF74B` Item_SetState
        ITEM_SET_STATE = 0xF74B, "Item_SetState", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF74C` Movement_SetObjectMovement
        MOVEMENT_SET_OBJECT_MOVEMENT = 0xF74C, "Movement_SetObjectMovement", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF74E` Movement_VectorUpdate
        MOVEMENT_VECTOR_UPDATE = 0xF74E, "Movement_VectorUpdate", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF750` Effects_SoundEvent
        EFFECTS_SOUND_EVENT = 0xF750, "Effects_SoundEvent", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF751` Effects_PlayerTeleport
        EFFECTS_PLAYER_TELEPORT = 0xF751, "Effects_PlayerTeleport", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF752` Movement_AutonomyLevel
        MOVEMENT_AUTONOMY_LEVEL = 0xF752, "Movement_AutonomyLevel", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF753` Movement_AutonomousPosition
        MOVEMENT_AUTONOMOUS_POSITION = 0xF753, "Movement_AutonomousPosition", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF754` Effects_PlayScriptID
        EFFECTS_PLAY_SCRIPT_ID = 0xF754, "Effects_PlayScriptID", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF755` Effects_PlayScriptType
        EFFECTS_PLAY_SCRIPT_TYPE = 0xF755, "Effects_PlayScriptType", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF7C1` Login_AccountBanned
        LOGIN_ACCOUNT_BANNED = 0xF7C1, "Login_AccountBanned", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7C8` Login_SendEnterWorldRequest
        LOGIN_SEND_ENTER_WORLD_REQUEST = 0xF7C8, "Login_SendEnterWorldRequest", Direction::C2S, Some(NetQueue::Logon), None;
        /// `0xF7C9` Movement_Jump_NonAutonomous
        MOVEMENT_JUMP_NON_AUTONOMOUS = 0xF7C9, "Movement_Jump_NonAutonomous", Direction::C2S, Some(NetQueue::Weenie), None;
        /// `0xF7CA` Admin_ReceiveAccountData
        ADMIN_RECEIVE_ACCOUNT_DATA = 0xF7CA, "Admin_ReceiveAccountData", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7CB` Admin_ReceivePlayerData
        ADMIN_RECEIVE_PLAYER_DATA = 0xF7CB, "Admin_ReceivePlayerData", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7CC` Admin_SendAdminGetServerVersion
        ADMIN_SEND_ADMIN_GET_SERVER_VERSION = 0xF7CC, "Admin_SendAdminGetServerVersion", Direction::C2S, Some(NetQueue::Control), None;
        /// `0xF7CD` Social_SendFriendsCommand
        SOCIAL_SEND_FRIENDS_COMMAND = 0xF7CD, "Social_SendFriendsCommand", Direction::C2S, Some(NetQueue::Control), None;
        /// `0xF7D9` Admin_SendAdminRestoreCharacter
        ADMIN_SEND_ADMIN_RESTORE_CHARACTER = 0xF7D9, "Admin_SendAdminRestoreCharacter", Direction::C2S, Some(NetQueue::Control), None;
        /// `0xF7DB` Item_UpdateObject
        ITEM_UPDATE_OBJECT = 0xF7DB, "Item_UpdateObject", Direction::S2C, None, Some(NetQueue::WorldObjects);
        /// `0xF7DC` Login_AccountBooted
        LOGIN_ACCOUNT_BOOTED = 0xF7DC, "Login_AccountBooted", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7DE` Communication_TurbineChat
        COMMUNICATION_TURBINE_CHAT = 0xF7DE, "Communication_TurbineChat", Direction::Both, Some(NetQueue::Logon), Some(NetQueue::Logon);
        /// `0xF7DF` Login_EnterGame_ServerReady
        LOGIN_ENTER_GAME_SERVER_READY = 0xF7DF, "Login_EnterGame_ServerReady", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7E0` Communication_TextboxString
        COMMUNICATION_TEXTBOX_STRING = 0xF7E0, "Communication_TextboxString", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7E1` Login_WorldInfo
        LOGIN_WORLD_INFO = 0xF7E1, "Login_WorldInfo", Direction::S2C, None, Some(NetQueue::UiQueue);
        /// `0xF7E2` DDD_DataMessage
        DDD_DATA_MESSAGE = 0xF7E2, "DDD_DataMessage", Direction::S2C, None, Some(NetQueue::ClientCache);
        /// `0xF7E3` DDD_RequestDataMessage
        DDD_REQUEST_DATA_MESSAGE = 0xF7E3, "DDD_RequestDataMessage", Direction::C2S, Some(NetQueue::ClientCache), None;
        /// `0xF7E4` DDD_ErrorMessage
        DDD_ERROR_MESSAGE = 0xF7E4, "DDD_ErrorMessage", Direction::S2C, None, Some(NetQueue::ClientCache);
        /// `0xF7E5` DDD_InterrogationMessage
        DDD_INTERROGATION_MESSAGE = 0xF7E5, "DDD_InterrogationMessage", Direction::S2C, None, Some(NetQueue::ClientCache);
        /// `0xF7E6` DDD_InterrogationResponseMessage
        DDD_INTERROGATION_RESPONSE_MESSAGE = 0xF7E6, "DDD_InterrogationResponseMessage", Direction::C2S, Some(NetQueue::ClientCache), None;
        /// `0xF7E7` DDD_BeginDDDMessage
        DDD_BEGIN_DDDMESSAGE = 0xF7E7, "DDD_BeginDDDMessage", Direction::S2C, None, Some(NetQueue::ClientCache);
        /// `0xF7EA` DDD_OnEndDDD
        DDD_ON_END_DDD = 0xF7EA, "DDD_OnEndDDD", Direction::Both, Some(NetQueue::ClientCache), Some(NetQueue::ClientCache);
        /// `0xF7EB` DDD_EndDDDMessage
        DDD_END_DDDMESSAGE = 0xF7EB, "DDD_EndDDDMessage", Direction::S2C, None, Some(NetQueue::ClientCache);
    }
    unlisted {
        /// `0x01C8` Allegiance_AllegianceUpdateDone: the retail server sends it; the client has no
        /// handler and drops it.
        ALLEGIANCE_ALLEGIANCE_UPDATE_DONE = 0x01C8;
        /// `0x00B5` Writing_BookModifyPageResponse: the retail server sends it; the client has no
        /// handler and drops it.
        WRITING_BOOK_MODIFY_PAGE_RESPONSE = 0x00B5;
        /// `0x0317`, a game event carrying one string that the client shows exactly as it shows
        /// `0x02EB Communication_TransientString`. The community catalogue has no name for it, so
        /// it has no row in [`OPCODES`]; the name here says what it behaves as.
        COMMUNICATION_TRANSIENT_STRING_0317 = 0x0317;
        /// `0x0318`, a game event carrying one string that the client shows exactly as it shows
        /// `0x0004 Communication_PopUpString`. Unnamed in the community catalogue, like `0x0317`.
        COMMUNICATION_POP_UP_STRING_0318 = 0x0318;
        /// `0xF7EC` DDD_OverlayManifestMessage: not a retail message. The overlay extension's
        /// manifest, which only this client and Empyrean send and read (see
        /// [`crate::admin::DddOverlayManifest`]); it has no row in [`OPCODES`].
        DDD_OVERLAY_MANIFEST = 0xF7EC;
    }
}

impl Opcode {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The table must stay sorted for [`Opcode::info`]'s bisection, and every row must be found
    /// by it. The count is the number of distinct opcodes in the table: a row added or dropped by
    /// hand has to change it deliberately.
    #[test]
    fn the_table_is_sorted_and_complete() {
        assert_eq!(OPCODES.len(), 354);
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
