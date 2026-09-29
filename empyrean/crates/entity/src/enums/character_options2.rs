// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CharacterOptions2.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CharacterOptions2.cs`; do not edit by hand

/// This is a list of all the options that are sent in the CharacterOptions2 flag Used with F7B0 0013: GameEvent -> PlayerDescription - To send some of the options (the others are sent in the CharacterOptions1 flag) Used with F7B1 01A1: GameAction -> Set Character Options - Sent as a flag with the "true" values ORed
///
/// ACE enum `CharacterOptions2` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CharacterOptions2(pub u32);

#[allow(non_upper_case_globals)]
impl CharacterOptions2 {
    pub const PersistentAtDay: Self = Self(0x1);
    pub const DisplayDateOfBirth: Self = Self(0x2);
    pub const DisplayChessRank: Self = Self(0x4);
    pub const DisplayFishingSkill: Self = Self(0x8);
    pub const DisplayNumberDeaths: Self = Self(0x10);
    pub const DisplayAge: Self = Self(0x20);
    pub const TimeStamp: Self = Self(0x40);
    pub const SalvageMultiple: Self = Self(0x80);
    pub const HearGeneralChat: Self = Self(0x100);
    pub const HearTradeChat: Self = Self(0x200);
    pub const HearLFGChat: Self = Self(0x400);
    pub const HearRoleplayChat: Self = Self(0x800);
    pub const AppearOffline: Self = Self(0x1000);
    pub const DisplayNumberCharacterTitles: Self = Self(0x2000);
    pub const MainPackPreferred: Self = Self(0x4000);
    pub const LeadMissileTargets: Self = Self(0x8000);
    pub const UseFastMissiles: Self = Self(0x10000);
    pub const FilterLanguage: Self = Self(0x20000);
    pub const ConfirmVolatileRareUse: Self = Self(0x40000);
    pub const HearSocietyChat: Self = Self(0x80000);
    pub const ShowHelm: Self = Self(0x100000);
    pub const DisableDistanceFog: Self = Self(0x200000);
    pub const UseMouseTurning: Self = Self(0x400000);
    pub const ShowCloak: Self = Self(0x800000);
    pub const LockUI: Self = Self(0x1000000);
    pub const HearPKDeath: Self = Self(0x2000000);
    pub const NotUsed1: Self = Self(0x4000000);
    pub const NotUsed2: Self = Self(0x8000000);
    pub const NotUsed3: Self = Self(0x10000000);
    pub const NotUsed4: Self = Self(0x20000000);
    pub const NotUsed5: Self = Self(0x40000000);
    pub const NotUsed6: Self = Self(0x80000000);
    /// DIVERGE: the retail client's value, not ACE's (V384).
    pub const Default: Self = Self(0x2948700);
}

impl CharacterOptions2 {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::PersistentAtDay, Self::DisplayDateOfBirth, Self::DisplayChessRank, Self::DisplayFishingSkill, Self::DisplayNumberDeaths, Self::DisplayAge, Self::TimeStamp, Self::SalvageMultiple, Self::HearGeneralChat, Self::HearTradeChat, Self::HearLFGChat, Self::HearRoleplayChat, Self::AppearOffline, Self::DisplayNumberCharacterTitles, Self::MainPackPreferred, Self::LeadMissileTargets, Self::UseFastMissiles, Self::FilterLanguage, Self::ConfirmVolatileRareUse, Self::HearSocietyChat, Self::ShowHelm, Self::DisableDistanceFog, Self::UseMouseTurning, Self::ShowCloak, Self::LockUI, Self::HearPKDeath, Self::Default, Self::NotUsed1, Self::NotUsed2, Self::NotUsed3, Self::NotUsed4, Self::NotUsed5, Self::NotUsed6];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["PersistentAtDay", "DisplayDateOfBirth", "DisplayChessRank", "DisplayFishingSkill", "DisplayNumberDeaths", "DisplayAge", "TimeStamp", "SalvageMultiple", "HearGeneralChat", "HearTradeChat", "HearLFGChat", "HearRoleplayChat", "AppearOffline", "DisplayNumberCharacterTitles", "MainPackPreferred", "LeadMissileTargets", "UseFastMissiles", "FilterLanguage", "ConfirmVolatileRareUse", "HearSocietyChat", "ShowHelm", "DisableDistanceFog", "UseMouseTurning", "ShowCloak", "LockUI", "HearPKDeath", "Default", "NotUsed1", "NotUsed2", "NotUsed3", "NotUsed4", "NotUsed5", "NotUsed6"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[12, 18, 26, 21, 5, 2, 1, 3, 13, 4, 17, 8, 10, 25, 11, 19, 9, 15, 24, 14, 27, 28, 29, 30, 31, 32, 0, 7, 23, 20, 6, 16, 22];
}

super::support::ace_enum!(CharacterOptions2, u32, flags);
super::support::ace_enum_from!(CharacterOptions2, u32 => u64, i64);
