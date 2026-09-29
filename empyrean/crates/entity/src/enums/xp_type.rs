// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/XpType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/XpType.cs`; do not edit by hand

/// For leveling up items, only kill and quest XP are taken into consideration
///
/// ACE enum `XpType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct XpType(pub i32);

#[allow(non_upper_case_globals)]
impl XpType {
    pub const Kill: Self = Self(0);
    pub const Quest: Self = Self(1);
    pub const Proficiency: Self = Self(2);
    pub const Fellowship: Self = Self(3);
    pub const Allegiance: Self = Self(4);
    pub const Admin: Self = Self(5);
    pub const Emote: Self = Self(6);
}

impl XpType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Kill, Self::Quest, Self::Proficiency, Self::Fellowship, Self::Allegiance, Self::Admin, Self::Emote];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Kill", "Quest", "Proficiency", "Fellowship", "Allegiance", "Admin", "Emote"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 4, 6, 3, 0, 2, 1];
}

super::support::ace_enum!(XpType, i32, plain);
super::support::ace_enum_from!(XpType, i32 => i64);
