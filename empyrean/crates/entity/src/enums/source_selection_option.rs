// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SourceSelectionOption.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SourceSelectionOption.cs`; do not edit by hand

/// Used to select a data source location.
///
/// ACE enum `SourceSelectionOption`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SourceSelectionOption(pub i32);

#[allow(non_upper_case_globals)]
impl SourceSelectionOption {
    pub const None: Self = Self(0);
    pub const Github: Self = Self(1);
    pub const LocalDisk: Self = Self(2);
}

impl SourceSelectionOption {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Github, Self::LocalDisk];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Github", "LocalDisk"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(SourceSelectionOption, i32, plain);
super::support::ace_enum_from!(SourceSelectionOption, i32 => i64);
