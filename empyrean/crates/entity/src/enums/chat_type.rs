// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatType.cs`; do not edit by hand

/// ACE enum `ChatType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatType(pub i32);

#[allow(non_upper_case_globals)]
impl ChatType {
    pub const Undef: Self = Self(0);
    pub const Allegiance: Self = Self(1);
    pub const General: Self = Self(2);
    pub const Trade: Self = Self(3);
    pub const LFG: Self = Self(4);
    pub const Roleplay: Self = Self(5);
    pub const Society: Self = Self(6);
    pub const SocietyCelHan: Self = Self(7);
    pub const SocietyEldWeb: Self = Self(8);
    pub const SocietyRadBlo: Self = Self(9);
    pub const Olthoi: Self = Self(10);
}

impl ChatType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Allegiance, Self::General, Self::Trade, Self::LFG, Self::Roleplay, Self::Society, Self::SocietyCelHan, Self::SocietyEldWeb, Self::SocietyRadBlo, Self::Olthoi];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Allegiance", "General", "Trade", "LFG", "Roleplay", "Society", "SocietyCelHan", "SocietyEldWeb", "SocietyRadBlo", "Olthoi"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 4, 10, 5, 6, 7, 8, 9, 3, 0];
}

super::support::ace_enum!(ChatType, i32, plain);
super::support::ace_enum_from!(ChatType, i32 => i64);
