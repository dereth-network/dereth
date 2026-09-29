// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SpellBitfield.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SpellBitfield.cs`; do not edit by hand

/// ACE enum `SpellBitfield` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SpellBitfield(pub i32);

#[allow(non_upper_case_globals)]
impl SpellBitfield {
    pub const Resistable: Self = Self(0x1);
    pub const PKSensitive: Self = Self(0x2);
    pub const Beneficial: Self = Self(0x4);
    pub const SelfTargeted: Self = Self(0x8);
    pub const Reversed: Self = Self(0x10);
    pub const NotIndoor: Self = Self(0x20);
    pub const NotOutdoor: Self = Self(0x40);
    pub const NotResearchable: Self = Self(0x80);
    pub const Projectile: Self = Self(0x100);
    pub const CreatureSpell: Self = Self(0x200);
    pub const ExcludedFromItemDescriptions: Self = Self(0x400);
    pub const IgnoresManaConversion: Self = Self(0x800);
    pub const NonTrackingProjectile: Self = Self(0x1000);
    pub const FellowshipSpell: Self = Self(0x2000);
    pub const FastCast: Self = Self(0x4000);
    pub const IndoorLongRange: Self = Self(0x8000);
    pub const DamageOverTime: Self = Self(0x10000);
    pub const UNKNOWN: Self = Self(0x20000);
}

impl SpellBitfield {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Resistable, Self::PKSensitive, Self::Beneficial, Self::SelfTargeted, Self::Reversed, Self::NotIndoor, Self::NotOutdoor, Self::NotResearchable, Self::Projectile, Self::CreatureSpell, Self::ExcludedFromItemDescriptions, Self::IgnoresManaConversion, Self::NonTrackingProjectile, Self::FellowshipSpell, Self::FastCast, Self::IndoorLongRange, Self::DamageOverTime, Self::UNKNOWN];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Resistable", "PKSensitive", "Beneficial", "SelfTargeted", "Reversed", "NotIndoor", "NotOutdoor", "NotResearchable", "Projectile", "CreatureSpell", "ExcludedFromItemDescriptions", "IgnoresManaConversion", "NonTrackingProjectile", "FellowshipSpell", "FastCast", "IndoorLongRange", "DamageOverTime", "UNKNOWN"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 9, 16, 10, 14, 13, 11, 15, 12, 5, 6, 7, 1, 8, 0, 4, 3, 17];
}

super::support::ace_enum!(SpellBitfield, i32, flags);
super::support::ace_enum_from!(SpellBitfield, i32 => i64);
