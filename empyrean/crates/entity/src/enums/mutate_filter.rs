// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MutateFilter.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MutateFilter.cs`; do not edit by hand

/// ACE enum `MutateFilter` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MutateFilter(pub i32);

#[allow(non_upper_case_globals)]
impl MutateFilter {
    pub const Undef: Self = Self(0x0);
    pub const ArmorModVsAcid: Self = Self(0x1);
    pub const ArmorModVsCold: Self = Self(0x2);
    pub const ArmorModVsElectric: Self = Self(0x4);
    pub const ArmorModVsFire: Self = Self(0x8);
    pub const EncumbranceVal: Self = Self(0x10);
    pub const Icon: Self = Self(0x20);
    pub const ItemWorkmanship: Self = Self(0x40);
    pub const LongDesc: Self = Self(0x80);
    pub const Name: Self = Self(0x100);
    pub const ResistItemAppraisal: Self = Self(0x200);
    pub const Setup: Self = Self(0x400);
    pub const ShieldValue: Self = Self(0x800);
    pub const ShortDesc: Self = Self(0x1000);
    pub const Value: Self = Self(0x2000);
    pub const WeaponTime: Self = Self(0x4000);
    pub const ArmorModVsType: Self = Self(0xF);
    pub const Base: Self = Self(0x37E0);
}

impl MutateFilter {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::ArmorModVsAcid, Self::ArmorModVsCold, Self::ArmorModVsElectric, Self::ArmorModVsFire, Self::ArmorModVsType, Self::EncumbranceVal, Self::Icon, Self::ItemWorkmanship, Self::LongDesc, Self::Name, Self::ResistItemAppraisal, Self::Setup, Self::ShieldValue, Self::ShortDesc, Self::Value, Self::Base, Self::WeaponTime];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "ArmorModVsAcid", "ArmorModVsCold", "ArmorModVsElectric", "ArmorModVsFire", "ArmorModVsType", "EncumbranceVal", "Icon", "ItemWorkmanship", "LongDesc", "Name", "ResistItemAppraisal", "Setup", "ShieldValue", "ShortDesc", "Value", "Base", "WeaponTime"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0, 15, 17];
}

super::support::ace_enum!(MutateFilter, i32, flags);
super::support::ace_enum_from!(MutateFilter, i32 => i64);
