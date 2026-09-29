// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/StatType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/StatType.cs`; do not edit by hand

/// ACE enum `StatType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct StatType(pub i32);

#[allow(non_upper_case_globals)]
impl StatType {
    pub const Undef: Self = Self(0);
    pub const Int: Self = Self(1);
    pub const Float: Self = Self(2);
    pub const Position: Self = Self(3);
    pub const Skill: Self = Self(4);
    pub const String: Self = Self(5);
    pub const DataID: Self = Self(6);
    pub const InstanceID: Self = Self(7);
    pub const DID: Self = Self(6);
    pub const IID: Self = Self(7);
    pub const Attribute: Self = Self(8);
    pub const Attribute2nd: Self = Self(9);
    pub const BodyDamageValue: Self = Self(10);
    pub const BodyDamageVariance: Self = Self(11);
    pub const BodyArmorValue: Self = Self(12);
    pub const Bool: Self = Self(13);
    pub const Int64: Self = Self(14);
    pub const NumStatTypes: Self = Self(15);
}

impl StatType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Int, Self::Float, Self::Position, Self::Skill, Self::String, Self::DID, Self::DataID, Self::InstanceID, Self::IID, Self::Attribute, Self::Attribute2nd, Self::BodyDamageValue, Self::BodyDamageVariance, Self::BodyArmorValue, Self::Bool, Self::Int64, Self::NumStatTypes];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Int", "Float", "Position", "Skill", "String", "DID", "DataID", "InstanceID", "IID", "Attribute", "Attribute2nd", "BodyDamageValue", "BodyDamageVariance", "BodyArmorValue", "Bool", "Int64", "NumStatTypes"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[10, 11, 14, 12, 13, 15, 6, 7, 2, 9, 8, 1, 16, 17, 3, 4, 5, 0];
}

super::support::ace_enum!(StatType, i32, plain);
super::support::ace_enum_from!(StatType, i32 => i64);
