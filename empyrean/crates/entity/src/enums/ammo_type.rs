// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AmmoType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AmmoType.cs`; do not edit by hand

/// ACE enum `AmmoType` (`[Flags]`), underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AmmoType(pub u16);

#[allow(non_upper_case_globals)]
impl AmmoType {
    pub const None: Self = Self(0x0);
    pub const Arrow: Self = Self(0x1);
    pub const Bolt: Self = Self(0x2);
    pub const Atlatl: Self = Self(0x4);
    pub const ArrowCrystal: Self = Self(0x8);
    pub const BoltCrystal: Self = Self(0x10);
    pub const AtlatlCrystal: Self = Self(0x20);
    pub const ArrowChorizite: Self = Self(0x40);
    pub const BoltChorizite: Self = Self(0x80);
    pub const AtlatlChorizite: Self = Self(0x100);
}

impl AmmoType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Arrow, Self::Bolt, Self::Atlatl, Self::ArrowCrystal, Self::BoltCrystal, Self::AtlatlCrystal, Self::ArrowChorizite, Self::BoltChorizite, Self::AtlatlChorizite];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Arrow", "Bolt", "Atlatl", "ArrowCrystal", "BoltCrystal", "AtlatlCrystal", "ArrowChorizite", "BoltChorizite", "AtlatlChorizite"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 7, 4, 3, 9, 6, 2, 8, 5, 0];
}

super::support::ace_enum!(AmmoType, u16, flags);
super::support::ace_enum_from!(AmmoType, u16 => u32, u64, i32, i64);
