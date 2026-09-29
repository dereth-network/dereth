// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ModificationOperation.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ModificationOperation.cs`; do not edit by hand

/// ACE enum `ModificationOperation`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ModificationOperation(pub i32);

#[allow(non_upper_case_globals)]
impl ModificationOperation {
    pub const None: Self = Self(0);
    pub const SetValue: Self = Self(1);
    pub const Add: Self = Self(2);
    pub const CopyFromSourceToTarget: Self = Self(3);
    pub const CopyFromSourceToResult: Self = Self(4);
    pub const Unknown1: Self = Self(5);
    pub const Unknown2: Self = Self(6);
    pub const AddSpell: Self = Self(7);
    pub const SetBitsOn: Self = Self(8);
    pub const SetBitsOff: Self = Self(9);
}

impl ModificationOperation {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::SetValue, Self::Add, Self::CopyFromSourceToTarget, Self::CopyFromSourceToResult, Self::Unknown1, Self::Unknown2, Self::AddSpell, Self::SetBitsOn, Self::SetBitsOff];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "SetValue", "Add", "CopyFromSourceToTarget", "CopyFromSourceToResult", "Unknown1", "Unknown2", "AddSpell", "SetBitsOn", "SetBitsOff"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 7, 4, 3, 0, 9, 8, 1, 5, 6];
}

super::support::ace_enum!(ModificationOperation, i32, plain);
super::support::ace_enum_from!(ModificationOperation, i32 => i64);
