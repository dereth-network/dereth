// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/BasePropertyType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/BasePropertyType.cs`; do not edit by hand

/// This is technically "PropertyType", but renamed to avoid confusion with ACE.Entity.Enum.Properties.PropertyType
///
/// ACE enum `BasePropertyType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct BasePropertyType(pub i32);

#[allow(non_upper_case_globals)]
impl BasePropertyType {
    pub const Invalid: Self = Self(0);
    pub const Bool: Self = Self(1);
    pub const Integer: Self = Self(2);
    pub const LongInteger: Self = Self(3);
    pub const Float: Self = Self(4);
    pub const Vector: Self = Self(5);
    pub const Color: Self = Self(6);
    pub const String: Self = Self(7);
    pub const StringInfo: Self = Self(8);
    pub const Enum: Self = Self(9);
    pub const DataFile: Self = Self(10);
    pub const Waveform: Self = Self(11);
    pub const InstanceID: Self = Self(12);
    pub const Position: Self = Self(13);
    pub const TimeStamp: Self = Self(14);
    pub const Bitfield32: Self = Self(15);
    pub const Bitfield64: Self = Self(16);
    pub const Array: Self = Self(17);
    pub const Struct: Self = Self(18);
    pub const StringToken: Self = Self(19);
    pub const PropertyName: Self = Self(20);
    pub const TriState: Self = Self(21);
}

impl BasePropertyType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Bool, Self::Integer, Self::LongInteger, Self::Float, Self::Vector, Self::Color, Self::String, Self::StringInfo, Self::Enum, Self::DataFile, Self::Waveform, Self::InstanceID, Self::Position, Self::TimeStamp, Self::Bitfield32, Self::Bitfield64, Self::Array, Self::Struct, Self::StringToken, Self::PropertyName, Self::TriState];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Bool", "Integer", "LongInteger", "Float", "Vector", "Color", "String", "StringInfo", "Enum", "DataFile", "Waveform", "InstanceID", "Position", "TimeStamp", "Bitfield32", "Bitfield64", "Array", "Struct", "StringToken", "PropertyName", "TriState"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[17, 15, 16, 1, 6, 10, 9, 4, 12, 2, 0, 3, 13, 20, 7, 8, 19, 18, 14, 21, 5, 11];
}

super::support::ace_enum!(BasePropertyType, i32, plain);
super::support::ace_enum_from!(BasePropertyType, i32 => i64);
