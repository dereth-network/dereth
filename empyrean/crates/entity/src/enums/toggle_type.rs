// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ToggleType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ToggleType.cs`; do not edit by hand

/// ACE enum `ToggleType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ToggleType(pub i32);

#[allow(non_upper_case_globals)]
impl ToggleType {
    pub const Invalid: Self = Self(0);
    pub const Momentary: Self = Self(1);
    pub const Toggle: Self = Self(2);
    pub const Impulse: Self = Self(3);
    pub const AutoRepeat: Self = Self(4);
    pub const Continuous: Self = Self(5);
}

impl ToggleType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Momentary, Self::Toggle, Self::Impulse, Self::AutoRepeat, Self::Continuous];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Momentary", "Toggle", "Impulse", "AutoRepeat", "Continuous"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 5, 3, 0, 1, 2];
}

super::support::ace_enum!(ToggleType, i32, plain);
super::support::ace_enum_from!(ToggleType, i32 => i64);
