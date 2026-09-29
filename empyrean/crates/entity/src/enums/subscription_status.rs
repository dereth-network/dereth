// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SubscriptionStatus.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SubscriptionStatus.cs`; do not edit by hand

/// ACE enum `SubscriptionStatus`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SubscriptionStatus(pub i32);

#[allow(non_upper_case_globals)]
impl SubscriptionStatus {
    pub const No_Subscription: Self = Self(0);
    pub const AsheronsCall_Subscription: Self = Self(1);
    pub const DarkMajesty_Subscription: Self = Self(2);
    pub const ThroneOfDestiny_Subscription: Self = Self(3);
    pub const ThroneOfDestiny_Preordered: Self = Self(4);
}

impl SubscriptionStatus {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::No_Subscription, Self::AsheronsCall_Subscription, Self::DarkMajesty_Subscription, Self::ThroneOfDestiny_Subscription, Self::ThroneOfDestiny_Preordered];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["No_Subscription", "AsheronsCall_Subscription", "DarkMajesty_Subscription", "ThroneOfDestiny_Subscription", "ThroneOfDestiny_Preordered"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0, 4, 3];
}

super::support::ace_enum!(SubscriptionStatus, i32, plain);
super::support::ace_enum_from!(SubscriptionStatus, i32 => i64);
