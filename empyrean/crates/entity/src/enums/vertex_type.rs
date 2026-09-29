// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/VertexType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/VertexType.cs`; do not edit by hand

/// ACE enum `VertexType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct VertexType(pub i32);

#[allow(non_upper_case_globals)]
impl VertexType {
    pub const Unkonwn: Self = Self(0);
    pub const CSWVertexType: Self = Self(1);
}

impl VertexType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Unkonwn, Self::CSWVertexType];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Unkonwn", "CSWVertexType"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0];
}

super::support::ace_enum!(VertexType, i32, plain);
super::support::ace_enum_from!(VertexType, i32 => i64);
