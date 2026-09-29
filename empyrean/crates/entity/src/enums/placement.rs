// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Placement.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Placement.cs`; do not edit by hand

/// ACE enum `Placement`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Placement(pub u32);

#[allow(non_upper_case_globals)]
impl Placement {
    pub const Default: Self = Self(0);
    pub const RightHandCombat: Self = Self(1);
    pub const RightHandNonCombat: Self = Self(2);
    pub const LeftHand: Self = Self(3);
    pub const Belt: Self = Self(4);
    pub const Quiver: Self = Self(5);
    pub const Shield: Self = Self(6);
    pub const LeftWeapon: Self = Self(7);
    pub const LeftUnarmed: Self = Self(8);
    pub const SpecialCrowssbowBolt: Self = Self(51);
    pub const MissileFlight: Self = Self(52);
    pub const Resting: Self = Self(101);
    pub const Other: Self = Self(102);
    pub const Hook: Self = Self(103);
    pub const Random1: Self = Self(121);
    pub const Random2: Self = Self(122);
    pub const Random3: Self = Self(123);
    pub const Random4: Self = Self(124);
    pub const Random5: Self = Self(125);
    pub const Random6: Self = Self(126);
    pub const Random7: Self = Self(127);
    pub const Random8: Self = Self(128);
    pub const Random9: Self = Self(129);
    pub const Random10: Self = Self(130);
    pub const XXXUnknownA: Self = Self(10);
    pub const XXXUnknownF: Self = Self(15);
    pub const XXXUnknown14: Self = Self(20);
    pub const XXXUnknown1E: Self = Self(30);
    pub const XXXUnknown20: Self = Self(32);
    pub const XXXUnknown3C: Self = Self(60);
    pub const XXXUnknown69: Self = Self(105);
    pub const XXXUnknown6A: Self = Self(106);
    pub const XXXUnknown63: Self = Self(99);
    pub const XXXUnknown68: Self = Self(104);
    pub const XXXUnknown78: Self = Self(120);
    pub const XXXUnknown84: Self = Self(132);
    pub const XXXUnknownF0: Self = Self(240);
    pub const XXXUnknown3F2: Self = Self(1010);
}

impl Placement {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Default, Self::RightHandCombat, Self::RightHandNonCombat, Self::LeftHand, Self::Belt, Self::Quiver, Self::Shield, Self::LeftWeapon, Self::LeftUnarmed, Self::XXXUnknownA, Self::XXXUnknownF, Self::XXXUnknown14, Self::XXXUnknown1E, Self::XXXUnknown20, Self::SpecialCrowssbowBolt, Self::MissileFlight, Self::XXXUnknown3C, Self::XXXUnknown63, Self::Resting, Self::Other, Self::Hook, Self::XXXUnknown68, Self::XXXUnknown69, Self::XXXUnknown6A, Self::XXXUnknown78, Self::Random1, Self::Random2, Self::Random3, Self::Random4, Self::Random5, Self::Random6, Self::Random7, Self::Random8, Self::Random9, Self::Random10, Self::XXXUnknown84, Self::XXXUnknownF0, Self::XXXUnknown3F2];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Default", "RightHandCombat", "RightHandNonCombat", "LeftHand", "Belt", "Quiver", "Shield", "LeftWeapon", "LeftUnarmed", "XXXUnknownA", "XXXUnknownF", "XXXUnknown14", "XXXUnknown1E", "XXXUnknown20", "SpecialCrowssbowBolt", "MissileFlight", "XXXUnknown3C", "XXXUnknown63", "Resting", "Other", "Hook", "XXXUnknown68", "XXXUnknown69", "XXXUnknown6A", "XXXUnknown78", "Random1", "Random2", "Random3", "Random4", "Random5", "Random6", "Random7", "Random8", "Random9", "Random10", "XXXUnknown84", "XXXUnknownF0", "XXXUnknown3F2"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 0, 20, 3, 8, 7, 15, 19, 5, 25, 34, 26, 27, 28, 29, 30, 31, 32, 33, 18, 1, 2, 6, 14, 11, 12, 13, 16, 37, 17, 21, 22, 23, 24, 35, 9, 10, 36];
}

super::support::ace_enum!(Placement, u32, plain);
super::support::ace_enum_from!(Placement, u32 => u64, i64);
