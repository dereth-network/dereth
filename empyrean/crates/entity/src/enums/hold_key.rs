// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HoldKey.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HoldKey.cs`; do not edit by hand

/// ACE enum `HoldKey`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HoldKey(pub u32);

#[allow(non_upper_case_globals)]
impl HoldKey {
    pub const Invalid: Self = Self(0);
    pub const None: Self = Self(1);
    pub const Run: Self = Self(2);
}

impl HoldKey {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::None, Self::Run];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "None", "Run"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1, 2];
}

super::support::ace_enum!(HoldKey, u32, plain);
super::support::ace_enum_from!(HoldKey, u32 => u64, i64);
