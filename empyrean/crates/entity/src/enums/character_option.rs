// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CharacterOption.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CharacterOption.cs`; do not edit by hand

use super::CharacterOptions1;
use super::CharacterOptions2;

/// This is a combination of the CharacterOption1 and CharacterOption2 enums. For the client, these are split into two groups because they can't be contained in a single uint field.
/// Only some of these have values, which is intentional.
/// Used with F7B1 0005: GameAction -> Set Single Character Option - Only those that have values will trigger that GameAction.
/// In the client, this is named PlayerOption.
///
/// ACE enum `CharacterOption`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CharacterOption(pub i32);

#[allow(non_upper_case_globals)]
impl CharacterOption {
    pub const AutoRepeatAttacks: Self = Self(0);
    pub const IgnoreAllegianceRequests: Self = Self(1);
    pub const IgnoreFellowshipRequests: Self = Self(2);
    pub const IgnoreAllTradeRequests: Self = Self(3);
    pub const DisableMostWeatherEffects: Self = Self(4);
    pub const AlwaysDaylightOutdoors: Self = Self(5);
    pub const LetOtherPlayersGiveYouItems: Self = Self(6);
    pub const KeepCombatTargetsInView: Self = Self(7);
    pub const Display3dTooltips: Self = Self(8);
    pub const AttemptToDeceiveOtherPlayers: Self = Self(9);
    pub const RunAsDefaultMovement: Self = Self(10);
    pub const StayInChatModeAfterSendingMessage: Self = Self(11);
    pub const AdvancedCombatInterface: Self = Self(12);
    pub const AutoTarget: Self = Self(13);
    pub const VividTargetingIndicator: Self = Self(14);
    pub const ShareFellowshipExpAndLuminance: Self = Self(15);
    pub const AcceptCorpseLootingPermissions: Self = Self(16);
    pub const ShareFellowshipLoot: Self = Self(17);
    pub const AutomaticallyAcceptFellowshipRequests: Self = Self(18);
    pub const SideBySideVitals: Self = Self(19);
    pub const ShowCoordinatesByTheRadar: Self = Self(20);
    pub const DisplaySpellDurations: Self = Self(21);
    pub const DisableHouseRestrictionEffects: Self = Self(22);
    pub const DragItemToPlayerOpensTrade: Self = Self(23);
    pub const ShowAllegianceLogons: Self = Self(24);
    pub const UseChargeAttack: Self = Self(25);
    pub const UseCraftingChanceOfSuccessDialog: Self = Self(26);
    pub const ListenToAllegianceChat: Self = Self(27);
    pub const AllowOthersToSeeYourDateOfBirth: Self = Self(28);
    pub const AllowOthersToSeeYourAge: Self = Self(29);
    pub const AllowOthersToSeeYourChessRank: Self = Self(30);
    pub const AllowOthersToSeeYourFishingSkill: Self = Self(31);
    pub const AllowOthersToSeeYourNumberOfDeaths: Self = Self(32);
    pub const DisplayTimestamps: Self = Self(33);
    pub const SalvageMultipleMaterialsAtOnce: Self = Self(34);
    pub const ListenToGeneralChat: Self = Self(35);
    pub const ListenToTradeChat: Self = Self(36);
    pub const ListenToLFGChat: Self = Self(37);
    pub const ListenToRoleplayChat: Self = Self(38);
    pub const AppearOffline: Self = Self(39);
    pub const AllowOthersToSeeYourNumberOfTitles: Self = Self(40);
    pub const UseMainPackAsDefaultForPickingUpItems: Self = Self(41);
    pub const LeadMissileTargets: Self = Self(42);
    pub const UseFastMissiles: Self = Self(43);
    pub const FilterLanguage: Self = Self(44);
    pub const ConfirmUseOfRareGems: Self = Self(45);
    pub const ListenToSocietyChat: Self = Self(46);
    pub const ShowYourHelmOrHeadGear: Self = Self(47);
    pub const DisableDistanceFog: Self = Self(48);
    pub const UseMouseTurning: Self = Self(49);
    pub const ShowYourCloak: Self = Self(50);
    pub const LockUI: Self = Self(51);
    pub const ListenToPKDeathMessages: Self = Self(52);
    pub const CharacterOptions1Default: Self = Self(53);
    pub const CharacterOptions2Default: Self = Self(54);
}

impl CharacterOption {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::AutoRepeatAttacks, Self::IgnoreAllegianceRequests, Self::IgnoreFellowshipRequests, Self::IgnoreAllTradeRequests, Self::DisableMostWeatherEffects, Self::AlwaysDaylightOutdoors, Self::LetOtherPlayersGiveYouItems, Self::KeepCombatTargetsInView, Self::Display3dTooltips, Self::AttemptToDeceiveOtherPlayers, Self::RunAsDefaultMovement, Self::StayInChatModeAfterSendingMessage, Self::AdvancedCombatInterface, Self::AutoTarget, Self::VividTargetingIndicator, Self::ShareFellowshipExpAndLuminance, Self::AcceptCorpseLootingPermissions, Self::ShareFellowshipLoot, Self::AutomaticallyAcceptFellowshipRequests, Self::SideBySideVitals, Self::ShowCoordinatesByTheRadar, Self::DisplaySpellDurations, Self::DisableHouseRestrictionEffects, Self::DragItemToPlayerOpensTrade, Self::ShowAllegianceLogons, Self::UseChargeAttack, Self::UseCraftingChanceOfSuccessDialog, Self::ListenToAllegianceChat, Self::AllowOthersToSeeYourDateOfBirth, Self::AllowOthersToSeeYourAge, Self::AllowOthersToSeeYourChessRank, Self::AllowOthersToSeeYourFishingSkill, Self::AllowOthersToSeeYourNumberOfDeaths, Self::DisplayTimestamps, Self::SalvageMultipleMaterialsAtOnce, Self::ListenToGeneralChat, Self::ListenToTradeChat, Self::ListenToLFGChat, Self::ListenToRoleplayChat, Self::AppearOffline, Self::AllowOthersToSeeYourNumberOfTitles, Self::UseMainPackAsDefaultForPickingUpItems, Self::LeadMissileTargets, Self::UseFastMissiles, Self::FilterLanguage, Self::ConfirmUseOfRareGems, Self::ListenToSocietyChat, Self::ShowYourHelmOrHeadGear, Self::DisableDistanceFog, Self::UseMouseTurning, Self::ShowYourCloak, Self::LockUI, Self::ListenToPKDeathMessages, Self::CharacterOptions1Default, Self::CharacterOptions2Default];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["AutoRepeatAttacks", "IgnoreAllegianceRequests", "IgnoreFellowshipRequests", "IgnoreAllTradeRequests", "DisableMostWeatherEffects", "AlwaysDaylightOutdoors", "LetOtherPlayersGiveYouItems", "KeepCombatTargetsInView", "Display3dTooltips", "AttemptToDeceiveOtherPlayers", "RunAsDefaultMovement", "StayInChatModeAfterSendingMessage", "AdvancedCombatInterface", "AutoTarget", "VividTargetingIndicator", "ShareFellowshipExpAndLuminance", "AcceptCorpseLootingPermissions", "ShareFellowshipLoot", "AutomaticallyAcceptFellowshipRequests", "SideBySideVitals", "ShowCoordinatesByTheRadar", "DisplaySpellDurations", "DisableHouseRestrictionEffects", "DragItemToPlayerOpensTrade", "ShowAllegianceLogons", "UseChargeAttack", "UseCraftingChanceOfSuccessDialog", "ListenToAllegianceChat", "AllowOthersToSeeYourDateOfBirth", "AllowOthersToSeeYourAge", "AllowOthersToSeeYourChessRank", "AllowOthersToSeeYourFishingSkill", "AllowOthersToSeeYourNumberOfDeaths", "DisplayTimestamps", "SalvageMultipleMaterialsAtOnce", "ListenToGeneralChat", "ListenToTradeChat", "ListenToLFGChat", "ListenToRoleplayChat", "AppearOffline", "AllowOthersToSeeYourNumberOfTitles", "UseMainPackAsDefaultForPickingUpItems", "LeadMissileTargets", "UseFastMissiles", "FilterLanguage", "ConfirmUseOfRareGems", "ListenToSocietyChat", "ShowYourHelmOrHeadGear", "DisableDistanceFog", "UseMouseTurning", "ShowYourCloak", "LockUI", "ListenToPKDeathMessages", "CharacterOptions1Default", "CharacterOptions2Default"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[16, 12, 29, 30, 28, 31, 32, 40, 5, 39, 9, 0, 13, 18, 53, 54, 45, 48, 22, 4, 8, 21, 33, 23, 44, 3, 1, 2, 7, 42, 6, 27, 35, 37, 52, 38, 46, 36, 51, 10, 34, 15, 17, 24, 20, 50, 47, 19, 11, 25, 26, 43, 41, 49, 14];

    /// The `[CharacterOptions1(…)]` argument of this value's member, if it has one.
    pub fn character_options1(self) -> Option<CharacterOptions1> {
        match self {
            Self::AutoRepeatAttacks => Some(CharacterOptions1::AutoRepeatAttack),
            Self::IgnoreAllegianceRequests => Some(CharacterOptions1::IgnoreAllegianceRequests),
            Self::IgnoreFellowshipRequests => Some(CharacterOptions1::IgnoreFellowshipRequests),
            Self::IgnoreAllTradeRequests => Some(CharacterOptions1::IgnoreTradeRequests),
            Self::DisableMostWeatherEffects => Some(CharacterOptions1::DisableMostWeatherEffects),
            Self::LetOtherPlayersGiveYouItems => Some(CharacterOptions1::AllowGive),
            Self::KeepCombatTargetsInView => Some(CharacterOptions1::ViewCombatTarget),
            Self::Display3dTooltips => Some(CharacterOptions1::ShowTooltips),
            Self::AttemptToDeceiveOtherPlayers => Some(CharacterOptions1::UseDeception),
            Self::RunAsDefaultMovement => Some(CharacterOptions1::ToggleRun),
            Self::StayInChatModeAfterSendingMessage => Some(CharacterOptions1::StayInChatMode),
            Self::AdvancedCombatInterface => Some(CharacterOptions1::AdvancedCombatUI),
            Self::AutoTarget => Some(CharacterOptions1::AutoTarget),
            Self::VividTargetingIndicator => Some(CharacterOptions1::VividTargetingIndicator),
            Self::ShareFellowshipExpAndLuminance => Some(CharacterOptions1::FellowshipShareXP),
            Self::AcceptCorpseLootingPermissions => Some(CharacterOptions1::AcceptLootPermits),
            Self::ShareFellowshipLoot => Some(CharacterOptions1::FellowshipShareLoot),
            Self::AutomaticallyAcceptFellowshipRequests => Some(CharacterOptions1::AutoAcceptFellowRequest),
            Self::SideBySideVitals => Some(CharacterOptions1::SideBySideVitals),
            Self::ShowCoordinatesByTheRadar => Some(CharacterOptions1::CoordinatesOnRadar),
            Self::DisplaySpellDurations => Some(CharacterOptions1::SpellDuration),
            Self::DisableHouseRestrictionEffects => Some(CharacterOptions1::DisableHouseRestrictionEffects),
            Self::DragItemToPlayerOpensTrade => Some(CharacterOptions1::DragItemOnPlayerOpensSecureTrade),
            Self::ShowAllegianceLogons => Some(CharacterOptions1::DisplayAllegianceLogonNotifications),
            Self::UseChargeAttack => Some(CharacterOptions1::UseChargeAttack),
            Self::UseCraftingChanceOfSuccessDialog => Some(CharacterOptions1::UseCraftSuccessDialog),
            Self::ListenToAllegianceChat => Some(CharacterOptions1::HearAllegianceChat),
            Self::CharacterOptions1Default => Some(CharacterOptions1::Default),
            _ => None,
        }
    }

    /// The `[CharacterOptions2(…)]` argument of this value's member, if it has one.
    pub fn character_options2(self) -> Option<CharacterOptions2> {
        match self {
            Self::AlwaysDaylightOutdoors => Some(CharacterOptions2::PersistentAtDay),
            Self::AllowOthersToSeeYourDateOfBirth => Some(CharacterOptions2::DisplayDateOfBirth),
            Self::AllowOthersToSeeYourAge => Some(CharacterOptions2::DisplayAge),
            Self::AllowOthersToSeeYourChessRank => Some(CharacterOptions2::DisplayChessRank),
            Self::AllowOthersToSeeYourFishingSkill => Some(CharacterOptions2::DisplayFishingSkill),
            Self::AllowOthersToSeeYourNumberOfDeaths => Some(CharacterOptions2::DisplayNumberDeaths),
            Self::DisplayTimestamps => Some(CharacterOptions2::TimeStamp),
            Self::SalvageMultipleMaterialsAtOnce => Some(CharacterOptions2::SalvageMultiple),
            Self::ListenToGeneralChat => Some(CharacterOptions2::HearGeneralChat),
            Self::ListenToTradeChat => Some(CharacterOptions2::HearTradeChat),
            Self::ListenToLFGChat => Some(CharacterOptions2::HearLFGChat),
            Self::ListenToRoleplayChat => Some(CharacterOptions2::HearRoleplayChat),
            Self::AppearOffline => Some(CharacterOptions2::AppearOffline),
            Self::AllowOthersToSeeYourNumberOfTitles => Some(CharacterOptions2::DisplayNumberCharacterTitles),
            Self::UseMainPackAsDefaultForPickingUpItems => Some(CharacterOptions2::MainPackPreferred),
            Self::LeadMissileTargets => Some(CharacterOptions2::LeadMissileTargets),
            Self::UseFastMissiles => Some(CharacterOptions2::UseFastMissiles),
            Self::FilterLanguage => Some(CharacterOptions2::FilterLanguage),
            Self::ConfirmUseOfRareGems => Some(CharacterOptions2::ConfirmVolatileRareUse),
            Self::ListenToSocietyChat => Some(CharacterOptions2::HearSocietyChat),
            Self::ShowYourHelmOrHeadGear => Some(CharacterOptions2::ShowHelm),
            Self::DisableDistanceFog => Some(CharacterOptions2::DisableDistanceFog),
            Self::UseMouseTurning => Some(CharacterOptions2::UseMouseTurning),
            Self::ShowYourCloak => Some(CharacterOptions2::ShowCloak),
            Self::LockUI => Some(CharacterOptions2::LockUI),
            Self::ListenToPKDeathMessages => Some(CharacterOptions2::HearPKDeath),
            Self::CharacterOptions2Default => Some(CharacterOptions2::Default),
            _ => None,
        }
    }
}

super::support::ace_enum!(CharacterOption, i32, plain);
super::support::ace_enum_from!(CharacterOption, i32 => i64);
