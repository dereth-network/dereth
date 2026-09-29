// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Vital.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Vital.cs`; do not edit by hand

/// ACE enum `Vital`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Vital(pub u32);

#[allow(non_upper_case_globals)]
impl Vital {
    pub const Undefined: Self = Self(0);
    pub const MaxHealth: Self = Self(1);
    pub const Health: Self = Self(2);
    pub const MaxStamina: Self = Self(3);
    pub const Stamina: Self = Self(4);
    pub const MaxMana: Self = Self(5);
    pub const Mana: Self = Self(6);
}

impl Vital {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undefined, Self::MaxHealth, Self::Health, Self::MaxStamina, Self::Stamina, Self::MaxMana, Self::Mana];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undefined", "MaxHealth", "Health", "MaxStamina", "Stamina", "MaxMana", "Mana"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 6, 1, 5, 3, 4, 0];
}

super::support::ace_enum!(Vital, u32, plain);
super::support::ace_enum_from!(Vital, u32 => u64, i64);
