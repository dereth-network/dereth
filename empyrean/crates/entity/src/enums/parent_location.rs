// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ParentLocation.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ParentLocation.cs`; do not edit by hand

/// ACE enum `ParentLocation`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ParentLocation(pub i32);

#[allow(non_upper_case_globals)]
impl ParentLocation {
    pub const None: Self = Self(0);
    pub const RightHand: Self = Self(1);
    pub const LeftHand: Self = Self(2);
    pub const Shield: Self = Self(3);
    pub const Belt: Self = Self(4);
    pub const Quiver: Self = Self(5);
    pub const Hearldry: Self = Self(6);
    pub const Mouth: Self = Self(7);
    pub const LeftWeapon: Self = Self(8);
    pub const LeftUnarmed: Self = Self(9);
}

impl ParentLocation {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::RightHand, Self::LeftHand, Self::Shield, Self::Belt, Self::Quiver, Self::Hearldry, Self::Mouth, Self::LeftWeapon, Self::LeftUnarmed];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "RightHand", "LeftHand", "Shield", "Belt", "Quiver", "Hearldry", "Mouth", "LeftWeapon", "LeftUnarmed"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 6, 2, 9, 8, 7, 0, 5, 1, 3];
}

super::support::ace_enum!(ParentLocation, i32, plain);
super::support::ace_enum_from!(ParentLocation, i32 => i64);
