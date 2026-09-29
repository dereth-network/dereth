// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PropertyDatFileType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PropertyDatFileType.cs`; do not edit by hand

/// ACE enum `PropertyDatFileType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyDatFileType(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyDatFileType {
    pub const ClientOnlyData: Self = Self(0);
    pub const ServerOnlyData: Self = Self(1);
    pub const SharedData: Self = Self(2);
}

impl PropertyDatFileType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::ClientOnlyData, Self::ServerOnlyData, Self::SharedData];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["ClientOnlyData", "ServerOnlyData", "SharedData"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1, 2];
}

super::support::ace_enum!(PropertyDatFileType, i32, plain);
super::support::ace_enum_from!(PropertyDatFileType, i32 => i64);
