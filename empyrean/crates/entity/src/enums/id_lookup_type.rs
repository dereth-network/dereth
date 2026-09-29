// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/IdLookupType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/IdLookupType.cs`; do not edit by hand

/// Account lookup Types
///
/// ACE enum `AccountLookupType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AccountLookupType(pub i32);

#[allow(non_upper_case_globals)]
impl AccountLookupType {
    pub const Undef: Self = Self(0);
    pub const Subscription: Self = Self(1);
    pub const Character: Self = Self(2);
    pub const Iid: Self = Self(3);
}

impl AccountLookupType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Subscription, Self::Character, Self::Iid];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Subscription", "Character", "Iid"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 1, 0];
}

super::support::ace_enum!(AccountLookupType, i32, plain);
super::support::ace_enum_from!(AccountLookupType, i32 => i64);
