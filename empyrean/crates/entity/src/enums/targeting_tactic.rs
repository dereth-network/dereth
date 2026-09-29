// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TargetingTactic.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TargetingTactic.cs`; do not edit by hand

/// Determines the monster behavior for which players are targetted
///
/// ACE enum `TargetingTactic` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TargetingTactic(pub i32);

#[allow(non_upper_case_globals)]
impl TargetingTactic {
    pub const None: Self = Self(0x0);
    pub const Random: Self = Self(0x1);
    pub const Focused: Self = Self(0x2);
    pub const LastDamager: Self = Self(0x4);
    pub const TopDamager: Self = Self(0x8);
    pub const Weakest: Self = Self(0x10);
    pub const Strongest: Self = Self(0x20);
    pub const Nearest: Self = Self(0x40);
}

impl TargetingTactic {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Random, Self::Focused, Self::LastDamager, Self::TopDamager, Self::Weakest, Self::Strongest, Self::Nearest];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Random", "Focused", "LastDamager", "TopDamager", "Weakest", "Strongest", "Nearest"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 7, 0, 1, 6, 4, 5];
}

super::support::ace_enum!(TargetingTactic, i32, flags);
super::support::ace_enum_from!(TargetingTactic, i32 => i64);
