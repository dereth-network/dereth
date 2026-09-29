// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CoverageMask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CoverageMask.cs`; do not edit by hand

/// Used during Calculation of Damage This data is sent in the priority field of the list (equipped items) portion of the player description event F7B0 - 0013 Og II
///
/// ACE enum `CoverageMask` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CoverageMask(pub u32);

#[allow(non_upper_case_globals)]
impl CoverageMask {
    pub const Unknown: Self = Self(0x1);
    pub const UnderwearUpperLegs: Self = Self(0x2);
    pub const UnderwearLowerLegs: Self = Self(0x4);
    pub const UnderwearChest: Self = Self(0x8);
    pub const UnderwearAbdomen: Self = Self(0x10);
    pub const UnderwearUpperArms: Self = Self(0x20);
    pub const UnderwearLowerArms: Self = Self(0x40);
    pub const OuterwearUpperLegs: Self = Self(0x100);
    pub const OuterwearLowerLegs: Self = Self(0x200);
    pub const OuterwearChest: Self = Self(0x400);
    pub const OuterwearAbdomen: Self = Self(0x800);
    pub const OuterwearUpperArms: Self = Self(0x1000);
    pub const OuterwearLowerArms: Self = Self(0x2000);
    pub const Head: Self = Self(0x4000);
    pub const Hands: Self = Self(0x8000);
    pub const Feet: Self = Self(0x10000);
}

impl CoverageMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Unknown, Self::UnderwearUpperLegs, Self::UnderwearLowerLegs, Self::UnderwearChest, Self::UnderwearAbdomen, Self::UnderwearUpperArms, Self::UnderwearLowerArms, Self::OuterwearUpperLegs, Self::OuterwearLowerLegs, Self::OuterwearChest, Self::OuterwearAbdomen, Self::OuterwearUpperArms, Self::OuterwearLowerArms, Self::Head, Self::Hands, Self::Feet];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Unknown", "UnderwearUpperLegs", "UnderwearLowerLegs", "UnderwearChest", "UnderwearAbdomen", "UnderwearUpperArms", "UnderwearLowerArms", "OuterwearUpperLegs", "OuterwearLowerLegs", "OuterwearChest", "OuterwearAbdomen", "OuterwearUpperArms", "OuterwearLowerArms", "Head", "Hands", "Feet"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[15, 14, 13, 10, 9, 12, 8, 11, 7, 4, 3, 6, 2, 5, 1, 0];
}

super::support::ace_enum!(CoverageMask, u32, flags);
super::support::ace_enum_from!(CoverageMask, u32 => u64, i64);

/// ACE enum `CoverageMaskHelper`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CoverageMaskHelper(pub u32);

#[allow(non_upper_case_globals)]
impl CoverageMaskHelper {
    pub const Underwear: Self = Self(126);
    pub const Outerwear: Self = Self(0x1FF00);
    pub const UnderwearLegs: Self = Self(6);
    pub const UnderwearArms: Self = Self(96);
    pub const OuterwearLegs: Self = Self(768);
    pub const OuterwearArms: Self = Self(12288);
    pub const UnderwearShirt: Self = Self(104);
    pub const UnderwearPants: Self = Self(6);
    pub const Extremities: Self = Self(0x1C000);
}

impl CoverageMaskHelper {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::UnderwearLegs, Self::UnderwearPants, Self::UnderwearArms, Self::UnderwearShirt, Self::Underwear, Self::OuterwearLegs, Self::OuterwearArms, Self::Extremities, Self::Outerwear];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["UnderwearLegs", "UnderwearPants", "UnderwearArms", "UnderwearShirt", "Underwear", "OuterwearLegs", "OuterwearArms", "Extremities", "Outerwear"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 8, 6, 5, 4, 2, 0, 1, 3];
}

super::support::ace_enum!(CoverageMaskHelper, u32, plain);
super::support::ace_enum_from!(CoverageMaskHelper, u32 => u64, i64);
