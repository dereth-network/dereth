// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SpellType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SpellType.cs`; do not edit by hand

/// ACE enum `SpellType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SpellType(pub i32);

#[allow(non_upper_case_globals)]
impl SpellType {
    pub const Undef: Self = Self(0);
    pub const Enchantment: Self = Self(1);
    pub const Projectile: Self = Self(2);
    pub const Boost: Self = Self(3);
    pub const Transfer: Self = Self(4);
    pub const PortalLink: Self = Self(5);
    pub const PortalRecall: Self = Self(6);
    pub const PortalSummon: Self = Self(7);
    pub const PortalSending: Self = Self(8);
    pub const Dispel: Self = Self(9);
    pub const LifeProjectile: Self = Self(10);
    pub const FellowBoost: Self = Self(11);
    pub const FellowEnchantment: Self = Self(12);
    pub const FellowPortalSending: Self = Self(13);
    pub const FellowDispel: Self = Self(14);
    pub const EnchantmentProjectile: Self = Self(15);
}

impl SpellType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Enchantment, Self::Projectile, Self::Boost, Self::Transfer, Self::PortalLink, Self::PortalRecall, Self::PortalSummon, Self::PortalSending, Self::Dispel, Self::LifeProjectile, Self::FellowBoost, Self::FellowEnchantment, Self::FellowPortalSending, Self::FellowDispel, Self::EnchantmentProjectile];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Enchantment", "Projectile", "Boost", "Transfer", "PortalLink", "PortalRecall", "PortalSummon", "PortalSending", "Dispel", "LifeProjectile", "FellowBoost", "FellowEnchantment", "FellowPortalSending", "FellowDispel", "EnchantmentProjectile"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 9, 1, 15, 11, 14, 12, 13, 10, 5, 6, 8, 7, 2, 4, 0];
}

super::support::ace_enum!(SpellType, i32, plain);
super::support::ace_enum_from!(SpellType, i32 => i64);
