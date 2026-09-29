// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Tolerance.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Tolerance.cs`; do not edit by hand

/// Determines when a monster will attack
///
/// ACE enum `Tolerance` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Tolerance(pub i32);

#[allow(non_upper_case_globals)]
impl Tolerance {
    pub const None: Self = Self(0x0);
    pub const NoAttack: Self = Self(0x1);
    pub const Appraise: Self = Self(0x2);
    pub const Unknown: Self = Self(0x4);
    pub const Provoke: Self = Self(0x8);
    pub const Unknown2: Self = Self(0x10);
    pub const Target: Self = Self(0x20);
    pub const Retaliate: Self = Self(0x40);
    pub const Monster: Self = Self(0x80);
}

impl Tolerance {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::NoAttack, Self::Appraise, Self::Unknown, Self::Provoke, Self::Unknown2, Self::Target, Self::Retaliate, Self::Monster];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "NoAttack", "Appraise", "Unknown", "Provoke", "Unknown2", "Target", "Retaliate", "Monster"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 8, 1, 0, 4, 7, 6, 3, 5];
}

super::support::ace_enum!(Tolerance, i32, flags);
super::support::ace_enum_from!(Tolerance, i32 => i64);
