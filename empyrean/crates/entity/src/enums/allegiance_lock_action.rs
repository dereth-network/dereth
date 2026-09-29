// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AllegianceLockAction.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AllegianceLockAction.cs`; do not edit by hand

/// ACE enum `AllegianceLockAction`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AllegianceLockAction(pub u32);

#[allow(non_upper_case_globals)]
impl AllegianceLockAction {
    pub const Undef: Self = Self(0);
    pub const Off: Self = Self(1);
    pub const On: Self = Self(2);
    pub const Toggle: Self = Self(3);
    pub const Check: Self = Self(4);
    pub const CheckApproved: Self = Self(5);
    pub const ClearApproved: Self = Self(6);
}

impl AllegianceLockAction {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Off, Self::On, Self::Toggle, Self::Check, Self::CheckApproved, Self::ClearApproved];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Off", "On", "Toggle", "Check", "CheckApproved", "ClearApproved"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 5, 6, 1, 2, 3, 0];
}

super::support::ace_enum!(AllegianceLockAction, u32, plain);
super::support::ace_enum_from!(AllegianceLockAction, u32 => u64, i64);
