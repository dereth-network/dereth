// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/WeaponType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/WeaponType.cs`; do not edit by hand

/// ACE enum `WeaponType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct WeaponType(pub i32);

#[allow(non_upper_case_globals)]
impl WeaponType {
    pub const Undef: Self = Self(0);
    pub const Unarmed: Self = Self(1);
    pub const Sword: Self = Self(2);
    pub const Axe: Self = Self(3);
    pub const Mace: Self = Self(4);
    pub const Spear: Self = Self(5);
    pub const Dagger: Self = Self(6);
    pub const Staff: Self = Self(7);
    pub const Bow: Self = Self(8);
    pub const Crossbow: Self = Self(9);
    pub const Thrown: Self = Self(10);
    pub const TwoHanded: Self = Self(11);
    pub const Magic: Self = Self(12);
}

impl WeaponType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Unarmed, Self::Sword, Self::Axe, Self::Mace, Self::Spear, Self::Dagger, Self::Staff, Self::Bow, Self::Crossbow, Self::Thrown, Self::TwoHanded, Self::Magic];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Unarmed", "Sword", "Axe", "Mace", "Spear", "Dagger", "Staff", "Bow", "Crossbow", "Thrown", "TwoHanded", "Magic"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 8, 9, 6, 4, 12, 5, 7, 2, 10, 11, 1, 0];
}

super::support::ace_enum!(WeaponType, i32, plain);
super::support::ace_enum_from!(WeaponType, i32 => i64);
