// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/WeenieErrorWithString.cs
// @generated from ACE's `Source/ACE.Entity/Enum/WeenieErrorWithString.cs`; do not edit by hand

/// These were tested against the last available client version: 0.0.11.6096
/// The WeenieErrorWithString identifies the specific message to be displayed in the chat window along with a custom string.
/// Used with F7B0 028B: Game Event -> Display an error message in the chat window.
/// WeenieError and WeenieErrorWithString are actually a single enum in the client.
/// The enum is used in handling 0x028A and 0x028B messages and also some other messages like UseDone.
/// We split the enum up into 2 enums because each function uses only a specific set of the enum values.
/// There are cases where the value was used by multiple messages e.g. 0x0036 ActionCancelled.
///
/// ACE enum `WeenieErrorWithString`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WeenieErrorWithString(pub i32);

#[allow(non_upper_case_globals)]
impl WeenieErrorWithString {
    pub const _IsTooBusyToAcceptGifts: Self = Self(30);
    pub const _CannotCarryAnymore: Self = Self(43);
    pub const YouFailToAffect_YouCannotAffectAnyone: Self = Self(78);
    pub const YouFailToAffect_TheyCannotBeHarmed: Self = Self(79);
    pub const YouFailToAffect_WithBeneficialSpells: Self = Self(80);
    pub const YouFailToAffect_YouAreNotPK: Self = Self(81);
    pub const YouFailToAffect_TheyAreNotPK: Self = Self(82);
    pub const YouFailToAffect_NotSamePKType: Self = Self(83);
    pub const YouFailToAffect_AcrossHouseBoundary: Self = Self(84);
    pub const _IsNotAcceptingGiftsRightNow: Self = Self(1007);
    pub const _IsAlreadyOneOfYourFollowers: Self = Self(1043);
    pub const _CannotHaveAnyMoreVassals: Self = Self(1046);
    pub const _DoesntKnowWhatToDoWithThat: Self = Self(1130);
    pub const YouMustBeAboveLevel_ToBuyHouse: Self = Self(1160);
    pub const YouMustBeAtOrBelowLevel_ToBuyHouse: Self = Self(1161);
    pub const YouMustBeAboveAllegianceRank_ToBuyHouse: Self = Self(1163);
    pub const YouMustBeAtOrBelowAllegianceRank_ToBuyHouse: Self = Self(1164);
    pub const The_WasNotSuitableForSalvaging: Self = Self(1215);
    pub const The_ContainseTheWrongMaterial: Self = Self(1216);
    pub const YouMustBe_ToUseItemMagic: Self = Self(1222);
    pub const Your_IsTooLowToUseItemMagic: Self = Self(1225);
    pub const Only_MayUseItemMagic: Self = Self(1226);
    pub const YouMustSpecialize_ToUseItemMagic: Self = Self(1227);
    pub const AiRefuseItemDuringEmote: Self = Self(1230);
    pub const _CannotAcceptStackedItems: Self = Self(1231);
    pub const Your_SkillMustBeTrained: Self = Self(1233);
    pub const NotEnoughSkillCreditsToSpecialize: Self = Self(1234);
    pub const TooMuchXPToRecoverFromSkill: Self = Self(1235);
    pub const Your_SkillIsAlreadyUntrained: Self = Self(1236);
    pub const CannotLowerSkillWhileWieldingItem: Self = Self(1237);
    pub const YouHaveSucceededSpecializing_Skill: Self = Self(1238);
    pub const YouHaveSucceededUnspecializing_Skill: Self = Self(1239);
    pub const YouHaveSucceededUntraining_Skill: Self = Self(1240);
    pub const CannotUntrain_SkillButRecoveredXP: Self = Self(1241);
    pub const TooManyCreditsInSpecializedSkills: Self = Self(1242);
    pub const AttributeTransferFromTooLow: Self = Self(1246);
    pub const AttributeTransferToTooHigh: Self = Self(1247);
    pub const ItemUnusableOnHook_CannotOpen: Self = Self(1256);
    pub const ItemUnusableOnHook_CanOpen: Self = Self(1257);
    pub const ItemOnlyUsableOnHook: Self = Self(1258);
    pub const _FailsToAffectYou_TheyCannotAffectAnyone: Self = Self(1268);
    pub const _FailsToAffectYou_YouCannotBeHarmed: Self = Self(1269);
    pub const _FailsToAffectYou_TheyAreNotPK: Self = Self(1270);
    pub const _FailsToAffectYou_YouAreNotPK: Self = Self(1271);
    pub const _FailsToAffectYou_NotSamePKType: Self = Self(1272);
    pub const _FailsToAffectYouAcrossHouseBoundary: Self = Self(1273);
    pub const _IsAnInvalidTarget: Self = Self(1274);
    pub const YouAreInvalidTargetForSpellOf_: Self = Self(1275);
    pub const _IsAtFullHealth: Self = Self(1279);
    pub const _HasNoSpellTargets: Self = Self(1289);
    pub const YouHaveNoTargetsForSpellOf_: Self = Self(1290);
    pub const _IsNowOpenFellowship: Self = Self(1291);
    pub const _IsNowClosedFellowship: Self = Self(1292);
    pub const _IsNowLeaderOfFellowship: Self = Self(1293);
    pub const YouHavePassedFellowshipLeadershipTo_: Self = Self(1294);
    pub const MaxNumberOf_Hooked: Self = Self(1296);
    pub const MaxNumberOf_HookedUntilOneIsRemoved: Self = Self(1300);
    pub const NoLongerMaxNumberOf_Hooked: Self = Self(1301);
    pub const _IsNotCloseEnoughToYourLevel: Self = Self(1303);
    pub const LockedFellowshipCannotRecruit_: Self = Self(1304);
    pub const YouHaveEnteredThe_Channel: Self = Self(1307);
    pub const YouHaveLeftThe_Channel: Self = Self(1308);
    pub const _WillNotReceiveMessage: Self = Self(1310);
    pub const MessageBlocked_: Self = Self(1311);
    pub const _HasBeenAddedToHearList: Self = Self(1313);
    pub const _HasBeenRemovedFromHearList: Self = Self(1314);
    pub const FailToRemove_FromLoudList: Self = Self(1317);
    pub const YouCannotOpenLockedFellowship: Self = Self(1320);
    pub const YouAreNowSnoopingOn_: Self = Self(1324);
    pub const YouAreNoLongerSnoopingOn_: Self = Self(1325);
    pub const YouFailToSnoopOn_: Self = Self(1326);
    pub const _AttemptedToSnoopOnYou: Self = Self(1327);
    pub const _IsAlreadyBeingSnoopedOn: Self = Self(1328);
    pub const _IsInLimbo: Self = Self(1329);
    pub const YouHaveBeenBootedFromAllegianceChat: Self = Self(1331);
    pub const _HasBeenBootedFromAllegianceChat: Self = Self(1332);
    pub const AccountOf_IsAlreadyBannedFromAllegiance: Self = Self(1334);
    pub const AccountOf_IsNotBannedFromAllegiance: Self = Self(1335);
    pub const AccountOf_WasNotUnbannedFromAllegiance: Self = Self(1336);
    pub const AccountOf_IsBannedFromAllegiance: Self = Self(1337);
    pub const AccountOf_IsUnbannedFromAllegiance: Self = Self(1338);
    pub const ListOfBannedCharacters: Self = Self(1339);
    pub const _IsBannedFromAllegiance: Self = Self(1342);
    pub const YouAreBannedFromAllegiance: Self = Self(1343);
    pub const _IsNowAllegianceOfficer: Self = Self(1345);
    pub const ErrorSetting_AsAllegianceOfficer: Self = Self(1346);
    pub const _IsNoLongerAllegianceOfficer: Self = Self(1347);
    pub const ErrorRemoving_AsAllegianceOFficer: Self = Self(1348);
    pub const YouMustWait_BeforeCommunicating: Self = Self(1351);
    pub const YourAllegianceOfficerStatusChanged: Self = Self(1353);
    pub const _IsAlreadyAllegianceOfficerOfThatLevel: Self = Self(1355);
    pub const The_IsCurrentlyInUse: Self = Self(1357);
    pub const YouAreNotListeningTo_Channel: Self = Self(1361);
    pub const AugmentationSkillNotTrained: Self = Self(1370);
    pub const YouSuccededAcquiringAugmentation: Self = Self(1371);
    pub const YouSucceededRecoveringXPFromSkill_AugmentationNotUntrainable: Self = Self(1372);
    pub const AFK: Self = Self(1374);
    pub const _IsAlreadyOnYourFriendsList: Self = Self(1378);
    pub const YouMayOnlyChangeAllegianceNameOnceEvery24Hours: Self = Self(1386);
    pub const _IsTheMonarchAndCannotBePromotedOrDemoted: Self = Self(1389);
    pub const ThatLevelOfAllegianceOfficerIsNowKnownAs_: Self = Self(1390);
    pub const YourAllegianceIsCurrently_: Self = Self(1396);
    pub const YourAllegianceIsNow_: Self = Self(1397);
    pub const YouCannotAcceptAllegiance_YourAllegianceIsLocked: Self = Self(1398);
    pub const YouCannotSwearAllegiance_AllegianceOf_IsLocked: Self = Self(1399);
    pub const YouHavePreApproved_ToJoinAllegiance: Self = Self(1400);
    pub const _IsAlreadyMemberOfYourAllegiance: Self = Self(1402);
    pub const _HasBeenPreApprovedToJoinYourAllegiance: Self = Self(1403);
    pub const YourAllegianceChatPrivilegesRemoved: Self = Self(1407);
    pub const _IsTemporarilyGaggedInAllegianceChat: Self = Self(1408);
    pub const YourAllegianceChatPrivilegesRestoredBy_: Self = Self(1410);
    pub const YouRestoreAllegianceChatPrivilegesTo_: Self = Self(1411);
    pub const _CowersFromYou: Self = Self(1418);
}

impl WeenieErrorWithString {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::_IsTooBusyToAcceptGifts, Self::_CannotCarryAnymore, Self::YouFailToAffect_YouCannotAffectAnyone, Self::YouFailToAffect_TheyCannotBeHarmed, Self::YouFailToAffect_WithBeneficialSpells, Self::YouFailToAffect_YouAreNotPK, Self::YouFailToAffect_TheyAreNotPK, Self::YouFailToAffect_NotSamePKType, Self::YouFailToAffect_AcrossHouseBoundary, Self::_IsNotAcceptingGiftsRightNow, Self::_IsAlreadyOneOfYourFollowers, Self::_CannotHaveAnyMoreVassals, Self::_DoesntKnowWhatToDoWithThat, Self::YouMustBeAboveLevel_ToBuyHouse, Self::YouMustBeAtOrBelowLevel_ToBuyHouse, Self::YouMustBeAboveAllegianceRank_ToBuyHouse, Self::YouMustBeAtOrBelowAllegianceRank_ToBuyHouse, Self::The_WasNotSuitableForSalvaging, Self::The_ContainseTheWrongMaterial, Self::YouMustBe_ToUseItemMagic, Self::Your_IsTooLowToUseItemMagic, Self::Only_MayUseItemMagic, Self::YouMustSpecialize_ToUseItemMagic, Self::AiRefuseItemDuringEmote, Self::_CannotAcceptStackedItems, Self::Your_SkillMustBeTrained, Self::NotEnoughSkillCreditsToSpecialize, Self::TooMuchXPToRecoverFromSkill, Self::Your_SkillIsAlreadyUntrained, Self::CannotLowerSkillWhileWieldingItem, Self::YouHaveSucceededSpecializing_Skill, Self::YouHaveSucceededUnspecializing_Skill, Self::YouHaveSucceededUntraining_Skill, Self::CannotUntrain_SkillButRecoveredXP, Self::TooManyCreditsInSpecializedSkills, Self::AttributeTransferFromTooLow, Self::AttributeTransferToTooHigh, Self::ItemUnusableOnHook_CannotOpen, Self::ItemUnusableOnHook_CanOpen, Self::ItemOnlyUsableOnHook, Self::_FailsToAffectYou_TheyCannotAffectAnyone, Self::_FailsToAffectYou_YouCannotBeHarmed, Self::_FailsToAffectYou_TheyAreNotPK, Self::_FailsToAffectYou_YouAreNotPK, Self::_FailsToAffectYou_NotSamePKType, Self::_FailsToAffectYouAcrossHouseBoundary, Self::_IsAnInvalidTarget, Self::YouAreInvalidTargetForSpellOf_, Self::_IsAtFullHealth, Self::_HasNoSpellTargets, Self::YouHaveNoTargetsForSpellOf_, Self::_IsNowOpenFellowship, Self::_IsNowClosedFellowship, Self::_IsNowLeaderOfFellowship, Self::YouHavePassedFellowshipLeadershipTo_, Self::MaxNumberOf_Hooked, Self::MaxNumberOf_HookedUntilOneIsRemoved, Self::NoLongerMaxNumberOf_Hooked, Self::_IsNotCloseEnoughToYourLevel, Self::LockedFellowshipCannotRecruit_, Self::YouHaveEnteredThe_Channel, Self::YouHaveLeftThe_Channel, Self::_WillNotReceiveMessage, Self::MessageBlocked_, Self::_HasBeenAddedToHearList, Self::_HasBeenRemovedFromHearList, Self::FailToRemove_FromLoudList, Self::YouCannotOpenLockedFellowship, Self::YouAreNowSnoopingOn_, Self::YouAreNoLongerSnoopingOn_, Self::YouFailToSnoopOn_, Self::_AttemptedToSnoopOnYou, Self::_IsAlreadyBeingSnoopedOn, Self::_IsInLimbo, Self::YouHaveBeenBootedFromAllegianceChat, Self::_HasBeenBootedFromAllegianceChat, Self::AccountOf_IsAlreadyBannedFromAllegiance, Self::AccountOf_IsNotBannedFromAllegiance, Self::AccountOf_WasNotUnbannedFromAllegiance, Self::AccountOf_IsBannedFromAllegiance, Self::AccountOf_IsUnbannedFromAllegiance, Self::ListOfBannedCharacters, Self::_IsBannedFromAllegiance, Self::YouAreBannedFromAllegiance, Self::_IsNowAllegianceOfficer, Self::ErrorSetting_AsAllegianceOfficer, Self::_IsNoLongerAllegianceOfficer, Self::ErrorRemoving_AsAllegianceOFficer, Self::YouMustWait_BeforeCommunicating, Self::YourAllegianceOfficerStatusChanged, Self::_IsAlreadyAllegianceOfficerOfThatLevel, Self::The_IsCurrentlyInUse, Self::YouAreNotListeningTo_Channel, Self::AugmentationSkillNotTrained, Self::YouSuccededAcquiringAugmentation, Self::YouSucceededRecoveringXPFromSkill_AugmentationNotUntrainable, Self::AFK, Self::_IsAlreadyOnYourFriendsList, Self::YouMayOnlyChangeAllegianceNameOnceEvery24Hours, Self::_IsTheMonarchAndCannotBePromotedOrDemoted, Self::ThatLevelOfAllegianceOfficerIsNowKnownAs_, Self::YourAllegianceIsCurrently_, Self::YourAllegianceIsNow_, Self::YouCannotAcceptAllegiance_YourAllegianceIsLocked, Self::YouCannotSwearAllegiance_AllegianceOf_IsLocked, Self::YouHavePreApproved_ToJoinAllegiance, Self::_IsAlreadyMemberOfYourAllegiance, Self::_HasBeenPreApprovedToJoinYourAllegiance, Self::YourAllegianceChatPrivilegesRemoved, Self::_IsTemporarilyGaggedInAllegianceChat, Self::YourAllegianceChatPrivilegesRestoredBy_, Self::YouRestoreAllegianceChatPrivilegesTo_, Self::_CowersFromYou];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["_IsTooBusyToAcceptGifts", "_CannotCarryAnymore", "YouFailToAffect_YouCannotAffectAnyone", "YouFailToAffect_TheyCannotBeHarmed", "YouFailToAffect_WithBeneficialSpells", "YouFailToAffect_YouAreNotPK", "YouFailToAffect_TheyAreNotPK", "YouFailToAffect_NotSamePKType", "YouFailToAffect_AcrossHouseBoundary", "_IsNotAcceptingGiftsRightNow", "_IsAlreadyOneOfYourFollowers", "_CannotHaveAnyMoreVassals", "_DoesntKnowWhatToDoWithThat", "YouMustBeAboveLevel_ToBuyHouse", "YouMustBeAtOrBelowLevel_ToBuyHouse", "YouMustBeAboveAllegianceRank_ToBuyHouse", "YouMustBeAtOrBelowAllegianceRank_ToBuyHouse", "The_WasNotSuitableForSalvaging", "The_ContainseTheWrongMaterial", "YouMustBe_ToUseItemMagic", "Your_IsTooLowToUseItemMagic", "Only_MayUseItemMagic", "YouMustSpecialize_ToUseItemMagic", "AiRefuseItemDuringEmote", "_CannotAcceptStackedItems", "Your_SkillMustBeTrained", "NotEnoughSkillCreditsToSpecialize", "TooMuchXPToRecoverFromSkill", "Your_SkillIsAlreadyUntrained", "CannotLowerSkillWhileWieldingItem", "YouHaveSucceededSpecializing_Skill", "YouHaveSucceededUnspecializing_Skill", "YouHaveSucceededUntraining_Skill", "CannotUntrain_SkillButRecoveredXP", "TooManyCreditsInSpecializedSkills", "AttributeTransferFromTooLow", "AttributeTransferToTooHigh", "ItemUnusableOnHook_CannotOpen", "ItemUnusableOnHook_CanOpen", "ItemOnlyUsableOnHook", "_FailsToAffectYou_TheyCannotAffectAnyone", "_FailsToAffectYou_YouCannotBeHarmed", "_FailsToAffectYou_TheyAreNotPK", "_FailsToAffectYou_YouAreNotPK", "_FailsToAffectYou_NotSamePKType", "_FailsToAffectYouAcrossHouseBoundary", "_IsAnInvalidTarget", "YouAreInvalidTargetForSpellOf_", "_IsAtFullHealth", "_HasNoSpellTargets", "YouHaveNoTargetsForSpellOf_", "_IsNowOpenFellowship", "_IsNowClosedFellowship", "_IsNowLeaderOfFellowship", "YouHavePassedFellowshipLeadershipTo_", "MaxNumberOf_Hooked", "MaxNumberOf_HookedUntilOneIsRemoved", "NoLongerMaxNumberOf_Hooked", "_IsNotCloseEnoughToYourLevel", "LockedFellowshipCannotRecruit_", "YouHaveEnteredThe_Channel", "YouHaveLeftThe_Channel", "_WillNotReceiveMessage", "MessageBlocked_", "_HasBeenAddedToHearList", "_HasBeenRemovedFromHearList", "FailToRemove_FromLoudList", "YouCannotOpenLockedFellowship", "YouAreNowSnoopingOn_", "YouAreNoLongerSnoopingOn_", "YouFailToSnoopOn_", "_AttemptedToSnoopOnYou", "_IsAlreadyBeingSnoopedOn", "_IsInLimbo", "YouHaveBeenBootedFromAllegianceChat", "_HasBeenBootedFromAllegianceChat", "AccountOf_IsAlreadyBannedFromAllegiance", "AccountOf_IsNotBannedFromAllegiance", "AccountOf_WasNotUnbannedFromAllegiance", "AccountOf_IsBannedFromAllegiance", "AccountOf_IsUnbannedFromAllegiance", "ListOfBannedCharacters", "_IsBannedFromAllegiance", "YouAreBannedFromAllegiance", "_IsNowAllegianceOfficer", "ErrorSetting_AsAllegianceOfficer", "_IsNoLongerAllegianceOfficer", "ErrorRemoving_AsAllegianceOFficer", "YouMustWait_BeforeCommunicating", "YourAllegianceOfficerStatusChanged", "_IsAlreadyAllegianceOfficerOfThatLevel", "The_IsCurrentlyInUse", "YouAreNotListeningTo_Channel", "AugmentationSkillNotTrained", "YouSuccededAcquiringAugmentation", "YouSucceededRecoveringXPFromSkill_AugmentationNotUntrainable", "AFK", "_IsAlreadyOnYourFriendsList", "YouMayOnlyChangeAllegianceNameOnceEvery24Hours", "_IsTheMonarchAndCannotBePromotedOrDemoted", "ThatLevelOfAllegianceOfficerIsNowKnownAs_", "YourAllegianceIsCurrently_", "YourAllegianceIsNow_", "YouCannotAcceptAllegiance_YourAllegianceIsLocked", "YouCannotSwearAllegiance_AllegianceOf_IsLocked", "YouHavePreApproved_ToJoinAllegiance", "_IsAlreadyMemberOfYourAllegiance", "_HasBeenPreApprovedToJoinYourAllegiance", "YourAllegianceChatPrivilegesRemoved", "_IsTemporarilyGaggedInAllegianceChat", "YourAllegianceChatPrivilegesRestoredBy_", "YouRestoreAllegianceChatPrivilegesTo_", "_CowersFromYou"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[96, 76, 79, 77, 80, 78, 23, 35, 36, 93, 29, 33, 87, 85, 66, 39, 38, 37, 81, 59, 55, 56, 63, 57, 26, 21, 100, 18, 91, 17, 34, 27, 83, 47, 69, 92, 68, 103, 67, 104, 8, 7, 6, 3, 4, 5, 2, 70, 74, 60, 61, 50, 54, 105, 30, 31, 32, 98, 15, 13, 16, 14, 19, 22, 88, 111, 94, 95, 108, 110, 101, 102, 89, 20, 28, 25, 71, 24, 1, 11, 112, 12, 45, 44, 42, 40, 43, 41, 64, 75, 107, 65, 49, 90, 72, 106, 97, 10, 46, 48, 82, 73, 86, 9, 58, 84, 52, 53, 51, 109, 99, 0, 62];
}

super::support::ace_enum!(WeenieErrorWithString, i32, plain);
super::support::ace_enum_from!(WeenieErrorWithString, i32 => i64);
