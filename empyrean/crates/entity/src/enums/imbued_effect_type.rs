// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ImbuedEffectType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ImbuedEffectType.cs`; do not edit by hand

/// ACE enum `ImbuedEffectType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ImbuedEffectType(pub u32);

#[allow(non_upper_case_globals)]
impl ImbuedEffectType {
    pub const Undef: Self = Self(0x0);
    pub const CriticalStrike: Self = Self(0x1);
    pub const CripplingBlow: Self = Self(0x2);
    pub const ArmorRending: Self = Self(0x4);
    pub const SlashRending: Self = Self(0x8);
    pub const PierceRending: Self = Self(0x10);
    pub const BludgeonRending: Self = Self(0x20);
    pub const AcidRending: Self = Self(0x40);
    pub const ColdRending: Self = Self(0x80);
    pub const ElectricRending: Self = Self(0x100);
    pub const FireRending: Self = Self(0x200);
    pub const MeleeDefense: Self = Self(0x400);
    pub const MissileDefense: Self = Self(0x800);
    pub const MagicDefense: Self = Self(0x1000);
    pub const Spellbook: Self = Self(0x2000);
    pub const NetherRending: Self = Self(0x4000);
    pub const IgnoreSomeMagicProjectileDamage: Self = Self(0x20000000);
    pub const AlwaysCritical: Self = Self(0x40000000);
    pub const IgnoreAllArmor: Self = Self(0x80000000);
}

impl ImbuedEffectType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::CriticalStrike, Self::CripplingBlow, Self::ArmorRending, Self::SlashRending, Self::PierceRending, Self::BludgeonRending, Self::AcidRending, Self::ColdRending, Self::ElectricRending, Self::FireRending, Self::MeleeDefense, Self::MissileDefense, Self::MagicDefense, Self::Spellbook, Self::NetherRending, Self::IgnoreSomeMagicProjectileDamage, Self::AlwaysCritical, Self::IgnoreAllArmor];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "CriticalStrike", "CripplingBlow", "ArmorRending", "SlashRending", "PierceRending", "BludgeonRending", "AcidRending", "ColdRending", "ElectricRending", "FireRending", "MeleeDefense", "MissileDefense", "MagicDefense", "Spellbook", "NetherRending", "IgnoreSomeMagicProjectileDamage", "AlwaysCritical", "IgnoreAllArmor"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 17, 3, 6, 8, 2, 1, 9, 10, 18, 16, 13, 11, 12, 15, 5, 4, 14, 0];
}

super::support::ace_enum!(ImbuedEffectType, u32, flags);
super::support::ace_enum_from!(ImbuedEffectType, u32 => u64, i64);
