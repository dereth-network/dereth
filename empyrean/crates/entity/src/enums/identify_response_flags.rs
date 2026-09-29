// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/IdentifyResponseFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/IdentifyResponseFlags.cs`; do not edit by hand

/// ACE enum `IdentifyResponseFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct IdentifyResponseFlags(pub i32);

#[allow(non_upper_case_globals)]
impl IdentifyResponseFlags {
    pub const None: Self = Self(0x0);
    pub const IntStatsTable: Self = Self(0x1);
    pub const BoolStatsTable: Self = Self(0x2);
    pub const FloatStatsTable: Self = Self(0x4);
    pub const StringStatsTable: Self = Self(0x8);
    pub const SpellBook: Self = Self(0x10);
    pub const WeaponProfile: Self = Self(0x20);
    pub const HookProfile: Self = Self(0x40);
    pub const ArmorProfile: Self = Self(0x80);
    pub const CreatureProfile: Self = Self(0x100);
    pub const ArmorEnchantmentBitfield: Self = Self(0x200);
    pub const ResistEnchantmentBitfield: Self = Self(0x400);
    pub const WeaponEnchantmentBitfield: Self = Self(0x800);
    pub const DidStatsTable: Self = Self(0x1000);
    pub const Int64StatsTable: Self = Self(0x2000);
    pub const ArmorLevels: Self = Self(0x4000);
}

impl IdentifyResponseFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::IntStatsTable, Self::BoolStatsTable, Self::FloatStatsTable, Self::StringStatsTable, Self::SpellBook, Self::WeaponProfile, Self::HookProfile, Self::ArmorProfile, Self::CreatureProfile, Self::ArmorEnchantmentBitfield, Self::ResistEnchantmentBitfield, Self::WeaponEnchantmentBitfield, Self::DidStatsTable, Self::Int64StatsTable, Self::ArmorLevels];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "IntStatsTable", "BoolStatsTable", "FloatStatsTable", "StringStatsTable", "SpellBook", "WeaponProfile", "HookProfile", "ArmorProfile", "CreatureProfile", "ArmorEnchantmentBitfield", "ResistEnchantmentBitfield", "WeaponEnchantmentBitfield", "DidStatsTable", "Int64StatsTable", "ArmorLevels"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[10, 15, 8, 2, 9, 13, 3, 7, 14, 1, 0, 11, 5, 4, 12, 6];
}

super::support::ace_enum!(IdentifyResponseFlags, i32, flags);
super::support::ace_enum_from!(IdentifyResponseFlags, i32 => i64);
