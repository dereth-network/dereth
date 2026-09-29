// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TransferFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TransferFlags.cs`; do not edit by hand

/// Indicates the source and destination for life magic transfer spells
///
/// ACE enum `TransferFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TransferFlags(pub i32);

#[allow(non_upper_case_globals)]
impl TransferFlags {
    pub const CasterSource: Self = Self(0x1);
    pub const TargetSource: Self = Self(0x2);
    pub const CasterDestination: Self = Self(0x4);
    pub const TargetDestination: Self = Self(0x8);
}

impl TransferFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::CasterSource, Self::TargetSource, Self::CasterDestination, Self::TargetDestination];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["CasterSource", "TargetSource", "CasterDestination", "TargetDestination"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 3, 1];
}

super::support::ace_enum!(TransferFlags, i32, flags);
super::support::ace_enum_from!(TransferFlags, i32 => i64);
