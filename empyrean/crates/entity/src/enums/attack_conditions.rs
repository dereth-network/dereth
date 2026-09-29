// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttackConditions.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AttackConditions.cs`; do not edit by hand

/// ACE enum `AttackConditions` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AttackConditions(pub i32);

#[allow(non_upper_case_globals)]
impl AttackConditions {
    pub const None: Self = Self(0x0);
    pub const CriticalProtectionAugmentation: Self = Self(0x1);
    pub const Recklessness: Self = Self(0x2);
    pub const SneakAttack: Self = Self(0x4);
    pub const Overpower: Self = Self(0x8);
}

impl AttackConditions {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::CriticalProtectionAugmentation, Self::Recklessness, Self::SneakAttack, Self::Overpower];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "CriticalProtectionAugmentation", "Recklessness", "SneakAttack", "Overpower"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 0, 4, 2, 3];
}

super::support::ace_enum!(AttackConditions, i32, flags);
super::support::ace_enum_from!(AttackConditions, i32 => i64);
