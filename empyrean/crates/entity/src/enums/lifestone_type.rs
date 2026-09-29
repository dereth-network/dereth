// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/LifestoneType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/LifestoneType.cs`; do not edit by hand

/// ACE enum `LifestoneType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct LifestoneType(pub u32);

#[allow(non_upper_case_globals)]
impl LifestoneType {
    pub const Original: Self = Self(0x20002EE);
    pub const New: Self = Self(0x2000EAD);
    pub const CandethKeep: Self = Self(0x20002EE);
    pub const Lugian: Self = Self(0x200107D);
    pub const Broken: Self = Self(0x20002EE);
}

impl LifestoneType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Original, Self::CandethKeep, Self::Broken, Self::New, Self::Lugian];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Original", "CandethKeep", "Broken", "New", "Lugian"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 4, 3, 0];
}

super::support::ace_enum!(LifestoneType, u32, flags);
super::support::ace_enum_from!(LifestoneType, u32 => u64, i64);
