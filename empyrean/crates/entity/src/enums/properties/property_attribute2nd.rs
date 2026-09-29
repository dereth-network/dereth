// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyAttribute2nd.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyAttribute2nd.cs`; do not edit by hand

/// ACE enum `PropertyAttribute2nd`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyAttribute2nd(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyAttribute2nd {
    pub const Undef: Self = Self(0);
    pub const MaxHealth: Self = Self(1);
    pub const Health: Self = Self(2);
    pub const MaxStamina: Self = Self(3);
    pub const Stamina: Self = Self(4);
    pub const MaxMana: Self = Self(5);
    pub const Mana: Self = Self(6);
}

impl PropertyAttribute2nd {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::MaxHealth, Self::Health, Self::MaxStamina, Self::Stamina, Self::MaxMana, Self::Mana];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "MaxHealth", "Health", "MaxStamina", "Stamina", "MaxMana", "Mana"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 6, 1, 5, 3, 4, 0];
}

super::support::ace_enum!(PropertyAttribute2nd, u16, plain);
super::support::ace_enum_from!(PropertyAttribute2nd, u16 => u32, u64, i32, i64);
