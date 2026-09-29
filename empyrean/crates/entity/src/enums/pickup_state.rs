// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PickupState.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PickupState.cs`; do not edit by hand

/// ACE enum `PickupState`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PickupState(pub i32);

#[allow(non_upper_case_globals)]
impl PickupState {
    pub const None: Self = Self(0);
    pub const Start: Self = Self(1);
    pub const Return: Self = Self(2);
}

impl PickupState {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Start, Self::Return];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Start", "Return"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 1];
}

super::support::ace_enum!(PickupState, i32, plain);
super::support::ace_enum_from!(PickupState, i32 => i64);
