// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/VendorType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/VendorType.cs`; do not edit by hand

/// ACE enum `VendorType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct VendorType(pub i32);

#[allow(non_upper_case_globals)]
impl VendorType {
    pub const Undef: Self = Self(0);
    pub const Open: Self = Self(1);
    pub const Close: Self = Self(2);
    pub const Sell: Self = Self(3);
    pub const Buy: Self = Self(4);
    pub const Heartbeat: Self = Self(5);
}

impl VendorType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Open, Self::Close, Self::Sell, Self::Buy, Self::Heartbeat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Open", "Close", "Sell", "Buy", "Heartbeat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 2, 5, 1, 3, 0];
}

super::support::ace_enum!(VendorType, i32, plain);
super::support::ace_enum_from!(VendorType, i32 => i64);
