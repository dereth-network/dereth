// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CharacterOptions1.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CharacterOptions1.cs`; do not edit by hand

/// This is a list of all the options that are sent in the CharacterOptions1 flag
/// Used with F7B0 0013: GameEvent -> PlayerDescription - To send some of the options (the others are sent in the CharacterOptions2 flag)
/// Used with F7B1 01A1: GameAction -> Set Character Options - Sent as a flag with the "true" values ORed
/// /// In the client, this is named CharacterOption.
///
/// ACE enum `CharacterOptions1` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CharacterOptions1(pub u32);

#[allow(non_upper_case_globals)]
impl CharacterOptions1 {
    pub const NotUsed1: Self = Self(0x1);
    pub const AutoRepeatAttack: Self = Self(0x2);
    pub const IgnoreAllegianceRequests: Self = Self(0x4);
    pub const IgnoreFellowshipRequests: Self = Self(0x8);
    pub const NotUsed2: Self = Self(0x10);
    pub const NotUsed3: Self = Self(0x20);
    pub const AllowGive: Self = Self(0x40);
    pub const ViewCombatTarget: Self = Self(0x80);
    pub const ShowTooltips: Self = Self(0x100);
    pub const UseDeception: Self = Self(0x200);
    pub const ToggleRun: Self = Self(0x400);
    pub const StayInChatMode: Self = Self(0x800);
    pub const AdvancedCombatUI: Self = Self(0x1000);
    pub const AutoTarget: Self = Self(0x2000);
    pub const NotUsed4: Self = Self(0x4000);
    pub const VividTargetingIndicator: Self = Self(0x8000);
    pub const DisableMostWeatherEffects: Self = Self(0x10000);
    pub const IgnoreTradeRequests: Self = Self(0x20000);
    pub const FellowshipShareXP: Self = Self(0x40000);
    pub const AcceptLootPermits: Self = Self(0x80000);
    pub const FellowshipShareLoot: Self = Self(0x100000);
    pub const SideBySideVitals: Self = Self(0x200000);
    pub const CoordinatesOnRadar: Self = Self(0x400000);
    pub const SpellDuration: Self = Self(0x800000);
    pub const NotUsed5: Self = Self(0x1000000);
    pub const DisableHouseRestrictionEffects: Self = Self(0x2000000);
    pub const DragItemOnPlayerOpensSecureTrade: Self = Self(0x4000000);
    pub const DisplayAllegianceLogonNotifications: Self = Self(0x8000000);
    pub const UseChargeAttack: Self = Self(0x10000000);
    pub const AutoAcceptFellowRequest: Self = Self(0x20000000);
    pub const HearAllegianceChat: Self = Self(0x40000000);
    pub const UseCraftSuccessDialog: Self = Self(0x80000000);
    pub const Default: Self = Self(0x50C4A54A);
}

impl CharacterOptions1 {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NotUsed1, Self::AutoRepeatAttack, Self::IgnoreAllegianceRequests, Self::IgnoreFellowshipRequests, Self::NotUsed2, Self::NotUsed3, Self::AllowGive, Self::ViewCombatTarget, Self::ShowTooltips, Self::UseDeception, Self::ToggleRun, Self::StayInChatMode, Self::AdvancedCombatUI, Self::AutoTarget, Self::NotUsed4, Self::VividTargetingIndicator, Self::DisableMostWeatherEffects, Self::IgnoreTradeRequests, Self::FellowshipShareXP, Self::AcceptLootPermits, Self::FellowshipShareLoot, Self::SideBySideVitals, Self::CoordinatesOnRadar, Self::SpellDuration, Self::NotUsed5, Self::DisableHouseRestrictionEffects, Self::DragItemOnPlayerOpensSecureTrade, Self::DisplayAllegianceLogonNotifications, Self::UseChargeAttack, Self::AutoAcceptFellowRequest, Self::HearAllegianceChat, Self::Default, Self::UseCraftSuccessDialog];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NotUsed1", "AutoRepeatAttack", "IgnoreAllegianceRequests", "IgnoreFellowshipRequests", "NotUsed2", "NotUsed3", "AllowGive", "ViewCombatTarget", "ShowTooltips", "UseDeception", "ToggleRun", "StayInChatMode", "AdvancedCombatUI", "AutoTarget", "NotUsed4", "VividTargetingIndicator", "DisableMostWeatherEffects", "IgnoreTradeRequests", "FellowshipShareXP", "AcceptLootPermits", "FellowshipShareLoot", "SideBySideVitals", "CoordinatesOnRadar", "SpellDuration", "NotUsed5", "DisableHouseRestrictionEffects", "DragItemOnPlayerOpensSecureTrade", "DisplayAllegianceLogonNotifications", "UseChargeAttack", "AutoAcceptFellowRequest", "HearAllegianceChat", "Default", "UseCraftSuccessDialog"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[19, 12, 6, 29, 1, 13, 22, 31, 25, 16, 27, 26, 20, 18, 30, 2, 3, 17, 0, 4, 5, 14, 24, 8, 21, 23, 11, 10, 28, 32, 9, 7, 15];
}

super::support::ace_enum!(CharacterOptions1, u32, flags);
super::support::ace_enum_from!(CharacterOptions1, u32 => u64, i64);
