// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/UiEffects.cs
// @generated from ACE's `Source/ACE.Entity/Enum/UiEffects.cs`; do not edit by hand

/// ACE enum `UiEffects` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct UiEffects(pub u32);

#[allow(non_upper_case_globals)]
impl UiEffects {
    pub const Undef: Self = Self(0x0);
    pub const Magical: Self = Self(0x1);
    pub const Poisoned: Self = Self(0x2);
    pub const BoostHealth: Self = Self(0x4);
    pub const BoostMana: Self = Self(0x8);
    pub const BoostStamina: Self = Self(0x10);
    pub const Fire: Self = Self(0x20);
    pub const Lightning: Self = Self(0x40);
    pub const Frost: Self = Self(0x80);
    pub const Acid: Self = Self(0x100);
    pub const Bludgeoning: Self = Self(0x200);
    pub const Slashing: Self = Self(0x400);
    pub const Piercing: Self = Self(0x800);
    pub const Nether: Self = Self(0x1000);
}

impl UiEffects {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Magical, Self::Poisoned, Self::BoostHealth, Self::BoostMana, Self::BoostStamina, Self::Fire, Self::Lightning, Self::Frost, Self::Acid, Self::Bludgeoning, Self::Slashing, Self::Piercing, Self::Nether];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Magical", "Poisoned", "BoostHealth", "BoostMana", "BoostStamina", "Fire", "Lightning", "Frost", "Acid", "Bludgeoning", "Slashing", "Piercing", "Nether"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[9, 10, 3, 4, 5, 6, 8, 7, 1, 13, 12, 2, 11, 0];
}

super::support::ace_enum!(UiEffects, u32, flags);
super::support::ace_enum_from!(UiEffects, u32 => u64, i64);
