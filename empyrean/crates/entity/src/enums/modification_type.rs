// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ModificationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ModificationType.cs`; do not edit by hand

/// ACE enum `ModificationType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ModificationType(pub i32);

#[allow(non_upper_case_globals)]
impl ModificationType {
    pub const SuccessTarget: Self = Self(0);
    pub const SuccessSource: Self = Self(1);
    pub const SuccessPlayer: Self = Self(2);
    pub const SuccessResult: Self = Self(3);
    pub const FailureTarget: Self = Self(4);
    pub const FailureSource: Self = Self(5);
    pub const FailurePlayer: Self = Self(6);
    pub const FailureResult: Self = Self(7);
}

impl ModificationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::SuccessTarget, Self::SuccessSource, Self::SuccessPlayer, Self::SuccessResult, Self::FailureTarget, Self::FailureSource, Self::FailurePlayer, Self::FailureResult];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["SuccessTarget", "SuccessSource", "SuccessPlayer", "SuccessResult", "FailureTarget", "FailureSource", "FailurePlayer", "FailureResult"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 7, 5, 4, 2, 3, 1, 0];
}

super::support::ace_enum!(ModificationType, i32, plain);
super::support::ace_enum_from!(ModificationType, i32 => i64);
