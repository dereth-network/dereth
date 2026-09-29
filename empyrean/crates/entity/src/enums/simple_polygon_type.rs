// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SimplePolygonType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SimplePolygonType.cs`; do not edit by hand

/// ACE enum `SimplePolygonType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SimplePolygonType(pub i32);

#[allow(non_upper_case_globals)]
impl SimplePolygonType {
    pub const SimplePolygon: Self = Self(0);
    pub const PathPolygon: Self = Self(1);
    pub const PlanarPolygon: Self = Self(2);
}

impl SimplePolygonType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::SimplePolygon, Self::PathPolygon, Self::PlanarPolygon];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["SimplePolygon", "PathPolygon", "PlanarPolygon"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(SimplePolygonType, i32, plain);
super::support::ace_enum_from!(SimplePolygonType, i32 => i64);
