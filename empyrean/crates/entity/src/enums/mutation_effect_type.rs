// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MutationEffectType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MutationEffectType.cs`; do not edit by hand

/// ACE enum `MutationEffectType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MutationEffectType(pub i32);

#[allow(non_upper_case_globals)]
impl MutationEffectType {
    pub const Assign: Self = Self(0);
    pub const Add: Self = Self(1);
    pub const Subtract: Self = Self(2);
    pub const Multiply: Self = Self(3);
    pub const Divide: Self = Self(4);
    pub const AtLeastAdd: Self = Self(5);
    pub const AtMostSubtract: Self = Self(6);
    pub const AddMultiply: Self = Self(7);
    pub const AddDivide: Self = Self(8);
    pub const SubtractMultiply: Self = Self(9);
    pub const SubtractDivide: Self = Self(10);
    pub const AssignAdd: Self = Self(11);
    pub const AssignSubtract: Self = Self(12);
    pub const AssignMultiply: Self = Self(13);
    pub const AssignDivide: Self = Self(14);
}

impl MutationEffectType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Assign, Self::Add, Self::Subtract, Self::Multiply, Self::Divide, Self::AtLeastAdd, Self::AtMostSubtract, Self::AddMultiply, Self::AddDivide, Self::SubtractMultiply, Self::SubtractDivide, Self::AssignAdd, Self::AssignSubtract, Self::AssignMultiply, Self::AssignDivide];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Assign", "Add", "Subtract", "Multiply", "Divide", "AtLeastAdd", "AtMostSubtract", "AddMultiply", "AddDivide", "SubtractMultiply", "SubtractDivide", "AssignAdd", "AssignSubtract", "AssignMultiply", "AssignDivide"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 8, 7, 0, 11, 14, 13, 12, 5, 6, 4, 3, 2, 10, 9];
}

super::support::ace_enum!(MutationEffectType, i32, plain);
super::support::ace_enum_from!(MutationEffectType, i32 => i64);
