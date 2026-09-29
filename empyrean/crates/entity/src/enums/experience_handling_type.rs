// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ExperienceHandlingType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ExperienceHandlingType.cs`; do not edit by hand

/// ACE enum `ExperienceHandlingType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ExperienceHandlingType(pub i32);

#[allow(non_upper_case_globals)]
impl ExperienceHandlingType {
    pub const Undef: Self = Self(0x0);
    pub const ApplyLevelMod: Self = Self(0x1);
    pub const ShareWithFellows: Self = Self(0x2);
    pub const AddFellowshipBonus: Self = Self(0x4);
    pub const ShareWithAllegiance: Self = Self(0x8);
    pub const ApplyToVitae: Self = Self(0x10);
    pub const EarnsCP: Self = Self(0x20);
    pub const ReducedByDistance: Self = Self(0x40);
    pub const Monster: Self = Self(0x5F);
    pub const NormalQuest: Self = Self(0x1A);
    pub const NoShareQuest: Self = Self(0x10);
    pub const PassupQuest: Self = Self(0x18);
    pub const ReceivedFromFellowship: Self = Self(0x18);
    pub const PPEarnedFromUse: Self = Self(0x7F);
    pub const AdminRaiseXP: Self = Self(0x10);
    pub const AdminRaiseSkillXP: Self = Self(0x10);
    pub const ReceivedFromAllegiance: Self = Self(0x0);
}

impl ExperienceHandlingType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::ReceivedFromAllegiance, Self::ApplyLevelMod, Self::ShareWithFellows, Self::AddFellowshipBonus, Self::ShareWithAllegiance, Self::ApplyToVitae, Self::AdminRaiseSkillXP, Self::NoShareQuest, Self::AdminRaiseXP, Self::PassupQuest, Self::ReceivedFromFellowship, Self::NormalQuest, Self::EarnsCP, Self::ReducedByDistance, Self::Monster, Self::PPEarnedFromUse];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "ReceivedFromAllegiance", "ApplyLevelMod", "ShareWithFellows", "AddFellowshipBonus", "ShareWithAllegiance", "ApplyToVitae", "AdminRaiseSkillXP", "NoShareQuest", "AdminRaiseXP", "PassupQuest", "ReceivedFromFellowship", "NormalQuest", "EarnsCP", "ReducedByDistance", "Monster", "PPEarnedFromUse"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 7, 9, 2, 6, 13, 15, 8, 12, 16, 10, 1, 11, 14, 5, 3, 0];
}

super::support::ace_enum!(ExperienceHandlingType, i32, flags);
super::support::ace_enum_from!(ExperienceHandlingType, i32 => i64);
