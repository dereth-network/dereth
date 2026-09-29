// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AllegianceHouseAction.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AllegianceHouseAction.cs`; do not edit by hand

/// Actions related to /allegiance house
///
/// ACE enum `AllegianceHouseAction`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AllegianceHouseAction(pub u32);

#[allow(non_upper_case_globals)]
impl AllegianceHouseAction {
    pub const Undef: Self = Self(0);
    pub const Help: Self = Self(1);
    pub const CheckStatus: Self = Self(1);
    pub const GuestOpen: Self = Self(2);
    pub const GuestClose: Self = Self(3);
    pub const StorageOpen: Self = Self(4);
    pub const StorageClose: Self = Self(5);
}

impl AllegianceHouseAction {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Help, Self::CheckStatus, Self::GuestOpen, Self::GuestClose, Self::StorageOpen, Self::StorageClose];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Help", "CheckStatus", "GuestOpen", "GuestClose", "StorageOpen", "StorageClose"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 4, 3, 1, 6, 5, 0];
}

super::support::ace_enum!(AllegianceHouseAction, u32, plain);
super::support::ace_enum_from!(AllegianceHouseAction, u32 => u64, i64);
