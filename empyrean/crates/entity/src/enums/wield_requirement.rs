// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/WieldRequirement.cs
// @generated from ACE's `Source/ACE.Entity/Enum/WieldRequirement.cs`; do not edit by hand

/// ACE enum `WieldRequirement`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WieldRequirement(pub i32);

#[allow(non_upper_case_globals)]
impl WieldRequirement {
    pub const Invalid: Self = Self(0);
    pub const Skill: Self = Self(1);
    pub const RawSkill: Self = Self(2);
    pub const Attrib: Self = Self(3);
    pub const RawAttrib: Self = Self(4);
    pub const SecondaryAttrib: Self = Self(5);
    pub const RawSecondaryAttrib: Self = Self(6);
    pub const Level: Self = Self(7);
    pub const Training: Self = Self(8);
    pub const IntStat: Self = Self(9);
    pub const BoolStat: Self = Self(10);
    pub const CreatureType: Self = Self(11);
    pub const HeritageType: Self = Self(12);
}

impl WieldRequirement {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Skill, Self::RawSkill, Self::Attrib, Self::RawAttrib, Self::SecondaryAttrib, Self::RawSecondaryAttrib, Self::Level, Self::Training, Self::IntStat, Self::BoolStat, Self::CreatureType, Self::HeritageType];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Skill", "RawSkill", "Attrib", "RawAttrib", "SecondaryAttrib", "RawSecondaryAttrib", "Level", "Training", "IntStat", "BoolStat", "CreatureType", "HeritageType"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 10, 11, 12, 9, 0, 7, 4, 6, 2, 5, 1, 8];
}

super::support::ace_enum!(WieldRequirement, i32, plain);
super::support::ace_enum_from!(WieldRequirement, i32 => i64);
