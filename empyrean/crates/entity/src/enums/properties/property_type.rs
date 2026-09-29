// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyType.cs`; do not edit by hand

/// ACE enum `PropertyType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyType(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyType {
    pub const PropertyAttribute: Self = Self(0);
    pub const PropertyAttribute2nd: Self = Self(1);
    pub const PropertyBook: Self = Self(2);
    pub const PropertyBool: Self = Self(3);
    pub const PropertyDataId: Self = Self(4);
    pub const PropertyDouble: Self = Self(5);
    pub const PropertyInstanceId: Self = Self(6);
    pub const PropertyInt: Self = Self(7);
    pub const PropertyInt64: Self = Self(8);
    pub const PropertyString: Self = Self(9);
    pub const PropertyPosition: Self = Self(10);
}

impl PropertyType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::PropertyAttribute, Self::PropertyAttribute2nd, Self::PropertyBook, Self::PropertyBool, Self::PropertyDataId, Self::PropertyDouble, Self::PropertyInstanceId, Self::PropertyInt, Self::PropertyInt64, Self::PropertyString, Self::PropertyPosition];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["PropertyAttribute", "PropertyAttribute2nd", "PropertyBook", "PropertyBool", "PropertyDataId", "PropertyDouble", "PropertyInstanceId", "PropertyInt", "PropertyInt64", "PropertyString", "PropertyPosition"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 9];
}

super::support::ace_enum!(PropertyType, i32, plain);
super::support::ace_enum_from!(PropertyType, i32 => i64);
