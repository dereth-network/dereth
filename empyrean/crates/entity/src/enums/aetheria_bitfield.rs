// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AetheriaBitfield.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AetheriaBitfield.cs`; do not edit by hand

/// ACE enum `AetheriaBitfield` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AetheriaBitfield(pub i32);

#[allow(non_upper_case_globals)]
impl AetheriaBitfield {
    pub const None: Self = Self(0x0);
    pub const Blue: Self = Self(0x1);
    pub const Yellow: Self = Self(0x2);
    pub const Red: Self = Self(0x4);
    pub const All: Self = Self(0x7);
}

impl AetheriaBitfield {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Blue, Self::Yellow, Self::Red, Self::All];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Blue", "Yellow", "Red", "All"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 0, 3, 2];
}

super::support::ace_enum!(AetheriaBitfield, i32, flags);
super::support::ace_enum_from!(AetheriaBitfield, i32 => i64);
