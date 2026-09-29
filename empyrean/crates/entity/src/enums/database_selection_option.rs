// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DatabaseSelectionOption.cs
// @generated from ACE's `Source/ACE.Entity/Enum/DatabaseSelectionOption.cs`; do not edit by hand

/// Used to select a database when calling a function.
///
/// ACE enum `DatabaseSelectionOption`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DatabaseSelectionOption(pub i32);

#[allow(non_upper_case_globals)]
impl DatabaseSelectionOption {
    pub const None: Self = Self(0);
    pub const Authentication: Self = Self(1);
    pub const Shard: Self = Self(2);
    pub const World: Self = Self(3);
    pub const All: Self = Self(4);
}

impl DatabaseSelectionOption {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Authentication, Self::Shard, Self::World, Self::All];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Authentication", "Shard", "World", "All"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 0, 2, 3];
}

super::support::ace_enum!(DatabaseSelectionOption, i32, plain);
super::support::ace_enum_from!(DatabaseSelectionOption, i32 => i64);
