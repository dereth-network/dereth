// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EnchantmentTypeFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EnchantmentTypeFlags.cs`; do not edit by hand

/// These flags are used to determine what enchantments stack.
///
/// ACE enum `EnchantmentTypeFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EnchantmentTypeFlags(pub i32);

#[allow(non_upper_case_globals)]
impl EnchantmentTypeFlags {
    pub const Undef: Self = Self(0x0);
    pub const Attribute: Self = Self(0x1);
    pub const SecondAtt: Self = Self(0x2);
    pub const Int: Self = Self(0x4);
    pub const Float: Self = Self(0x8);
    pub const Skill: Self = Self(0x10);
    pub const BodyDamageValue: Self = Self(0x20);
    pub const BodyDamageVariance: Self = Self(0x40);
    pub const BodyArmorValue: Self = Self(0x80);
    pub const SingleStat: Self = Self(0x1000);
    pub const MultipleStat: Self = Self(0x2000);
    pub const Multiplicative: Self = Self(0x4000);
    pub const Additive: Self = Self(0x8000);
    pub const AttackSkills: Self = Self(0x10000);
    pub const DefenseSkills: Self = Self(0x20000);
    pub const Multiplicative_Degrade: Self = Self(0x100000);
    pub const Additive_Degrade: Self = Self(0x200000);
    pub const Vitae: Self = Self(0x800000);
    pub const Cooldown: Self = Self(0x1000000);
    pub const Beneficial: Self = Self(0x2000000);
    pub const StatTypes: Self = Self(0xFF);
}

impl EnchantmentTypeFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Attribute, Self::SecondAtt, Self::Int, Self::Float, Self::Skill, Self::BodyDamageValue, Self::BodyDamageVariance, Self::BodyArmorValue, Self::StatTypes, Self::SingleStat, Self::MultipleStat, Self::Multiplicative, Self::Additive, Self::AttackSkills, Self::DefenseSkills, Self::Multiplicative_Degrade, Self::Additive_Degrade, Self::Vitae, Self::Cooldown, Self::Beneficial];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Attribute", "SecondAtt", "Int", "Float", "Skill", "BodyDamageValue", "BodyDamageVariance", "BodyArmorValue", "StatTypes", "SingleStat", "MultipleStat", "Multiplicative", "Additive", "AttackSkills", "DefenseSkills", "Multiplicative_Degrade", "Additive_Degrade", "Vitae", "Cooldown", "Beneficial"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[13, 17, 14, 1, 20, 8, 6, 7, 19, 15, 4, 3, 11, 12, 16, 2, 10, 5, 9, 0, 18];
}

super::support::ace_enum!(EnchantmentTypeFlags, i32, flags);
super::support::ace_enum_from!(EnchantmentTypeFlags, i32 => i64);
