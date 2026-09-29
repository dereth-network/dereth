// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PropertyPropagationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PropertyPropagationType.cs`; do not edit by hand

/// ACE enum `PropertyPropagationType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyPropagationType(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyPropagationType {
    pub const NetPredictedSharedVisually: Self = Self(0);
    pub const NetPredictedSharedPrivately: Self = Self(1);
    pub const NetSharedVisually: Self = Self(2);
    pub const NetSharedPrivately: Self = Self(3);
    pub const NetNotShared: Self = Self(4);
    pub const WorldSharedWithServers: Self = Self(5);
    pub const WorldSharedWithServersAndClients: Self = Self(6);
}

impl PropertyPropagationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NetPredictedSharedVisually, Self::NetPredictedSharedPrivately, Self::NetSharedVisually, Self::NetSharedPrivately, Self::NetNotShared, Self::WorldSharedWithServers, Self::WorldSharedWithServersAndClients];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NetPredictedSharedVisually", "NetPredictedSharedPrivately", "NetSharedVisually", "NetSharedPrivately", "NetNotShared", "WorldSharedWithServers", "WorldSharedWithServersAndClients"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 0, 3, 2, 5, 6];
}

super::support::ace_enum!(PropertyPropagationType, i32, plain);
super::support::ace_enum_from!(PropertyPropagationType, i32 => i64);
