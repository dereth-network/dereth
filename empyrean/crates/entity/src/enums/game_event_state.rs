// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GameEventState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GameEventState.cs`; do not edit by hand

/// ACE enum `GameEventState`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GameEventState(pub i32);

#[allow(non_upper_case_globals)]
impl GameEventState {
    pub const Undef: Self = Self(0);
    pub const Enabled: Self = Self(1);
    pub const Disabled: Self = Self(2);
    pub const Off: Self = Self(3);
    pub const On: Self = Self(4);
}

impl GameEventState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Enabled, Self::Disabled, Self::Off, Self::On];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Enabled", "Disabled", "Off", "On"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 3, 4, 0];
}

super::support::ace_enum!(GameEventState, i32, plain);
super::support::ace_enum_from!(GameEventState, i32 => i64);
