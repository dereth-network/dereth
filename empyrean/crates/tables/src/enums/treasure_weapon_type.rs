// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/TreasureWeaponType.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/TreasureWeaponType.cs`; do not edit by hand

/// ACE enum `TreasureWeaponType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureWeaponType(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureWeaponType {
    pub const Undef: Self = Self(0);
    pub const MeleeWeapon: Self = Self(1);
    pub const Axe: Self = Self(2);
    pub const Dagger: Self = Self(3);
    pub const DaggerMS: Self = Self(4);
    pub const Mace: Self = Self(5);
    pub const MaceJitte: Self = Self(6);
    pub const Spear: Self = Self(7);
    pub const Staff: Self = Self(8);
    pub const Sword: Self = Self(9);
    pub const SwordMS: Self = Self(10);
    pub const Unarmed: Self = Self(11);
    pub const MissileWeapon: Self = Self(12);
    pub const Bow: Self = Self(13);
    pub const Crossbow: Self = Self(14);
    pub const Atlatl: Self = Self(15);
    pub const Caster: Self = Self(16);
    pub const TwoHandedWeapon: Self = Self(17);
    pub const TwoHandedAxe: Self = Self(18);
    pub const TwoHandedMace: Self = Self(19);
    pub const TwoHandedSpear: Self = Self(20);
    pub const TwoHandedSword: Self = Self(21);
}

impl TreasureWeaponType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::MeleeWeapon, Self::Axe, Self::Dagger, Self::DaggerMS, Self::Mace, Self::MaceJitte, Self::Spear, Self::Staff, Self::Sword, Self::SwordMS, Self::Unarmed, Self::MissileWeapon, Self::Bow, Self::Crossbow, Self::Atlatl, Self::Caster, Self::TwoHandedWeapon, Self::TwoHandedAxe, Self::TwoHandedMace, Self::TwoHandedSpear, Self::TwoHandedSword];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "MeleeWeapon", "Axe", "Dagger", "DaggerMS", "Mace", "MaceJitte", "Spear", "Staff", "Sword", "SwordMS", "Unarmed", "MissileWeapon", "Bow", "Crossbow", "Atlatl", "Caster", "TwoHandedWeapon", "TwoHandedAxe", "TwoHandedMace", "TwoHandedSpear", "TwoHandedSword"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[15, 2, 13, 16, 14, 3, 4, 5, 6, 1, 12, 7, 8, 9, 10, 18, 19, 20, 21, 17, 11, 0];
}

super::support::ace_enum!(TreasureWeaponType, i32, plain);
super::support::ace_enum_from!(TreasureWeaponType, i32 => i64);
