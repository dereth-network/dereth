// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DamageType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/DamageType.cs`; do not edit by hand

/// ACE enum `DamageType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DamageType(pub i32);

#[allow(non_upper_case_globals)]
impl DamageType {
    pub const Undef: Self = Self(0x0);
    pub const Slash: Self = Self(0x1);
    pub const Pierce: Self = Self(0x2);
    pub const Bludgeon: Self = Self(0x4);
    pub const Cold: Self = Self(0x8);
    pub const Fire: Self = Self(0x10);
    pub const Acid: Self = Self(0x20);
    pub const Electric: Self = Self(0x40);
    pub const Health: Self = Self(0x80);
    pub const Stamina: Self = Self(0x100);
    pub const Mana: Self = Self(0x200);
    pub const Nether: Self = Self(0x400);
    pub const Base: Self = Self(0x10000000);
    pub const Physical: Self = Self(0x7);
    pub const Elemental: Self = Self(0x78);
}

impl DamageType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Slash, Self::Pierce, Self::Bludgeon, Self::Physical, Self::Cold, Self::Fire, Self::Acid, Self::Electric, Self::Elemental, Self::Health, Self::Stamina, Self::Mana, Self::Nether, Self::Base];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Slash", "Pierce", "Bludgeon", "Physical", "Cold", "Fire", "Acid", "Electric", "Elemental", "Health", "Stamina", "Mana", "Nether", "Base"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 14, 3, 5, 8, 9, 6, 10, 12, 13, 4, 2, 1, 11, 0];
}

super::support::ace_enum!(DamageType, i32, flags);
super::support::ace_enum_from!(DamageType, i32 => i64);
