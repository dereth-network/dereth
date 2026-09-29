// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EnchantmentCategory.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EnchantmentCategory.cs`; do not edit by hand

/// ACE enum `EnchantmentMask` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EnchantmentMask(pub i32);

#[allow(non_upper_case_globals)]
impl EnchantmentMask {
    pub const Multiplicative: Self = Self(0x1);
    pub const Additive: Self = Self(0x2);
    pub const Vitae: Self = Self(0x4);
    pub const Cooldown: Self = Self(0x8);
}

impl EnchantmentMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Multiplicative, Self::Additive, Self::Vitae, Self::Cooldown];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Multiplicative", "Additive", "Vitae", "Cooldown"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 3, 0, 2];
}

super::support::ace_enum!(EnchantmentMask, i32, flags);
super::support::ace_enum_from!(EnchantmentMask, i32 => i64);
