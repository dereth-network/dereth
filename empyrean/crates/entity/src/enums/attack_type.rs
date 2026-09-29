// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttackType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AttackType.cs`; do not edit by hand

/// ACE enum `AttackType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AttackType(pub i32);

#[allow(non_upper_case_globals)]
impl AttackType {
    pub const Undef: Self = Self(0x0);
    pub const Punch: Self = Self(0x1);
    pub const Thrust: Self = Self(0x2);
    pub const Slash: Self = Self(0x4);
    pub const Kick: Self = Self(0x8);
    pub const OffhandPunch: Self = Self(0x10);
    pub const DoubleSlash: Self = Self(0x20);
    pub const TripleSlash: Self = Self(0x40);
    pub const DoubleThrust: Self = Self(0x80);
    pub const TripleThrust: Self = Self(0x100);
    pub const OffhandThrust: Self = Self(0x200);
    pub const OffhandSlash: Self = Self(0x400);
    pub const OffhandDoubleSlash: Self = Self(0x800);
    pub const OffhandTripleSlash: Self = Self(0x1000);
    pub const OffhandDoubleThrust: Self = Self(0x2000);
    pub const OffhandTripleThrust: Self = Self(0x4000);
    pub const Unarmed: Self = Self(0x19);
    pub const DoubleStrike: Self = Self(0x28A0);
    pub const TripleStrike: Self = Self(0x5140);
    pub const Offhand: Self = Self(0x7E00);
    pub const Thrusts: Self = Self(0x6382);
    pub const Slashes: Self = Self(0x1C64);
    pub const Punches: Self = Self(0x11);
    pub const MultiStrike: Self = Self(0x79E0);
}

impl AttackType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Punch, Self::Thrust, Self::Slash, Self::Kick, Self::OffhandPunch, Self::Punches, Self::Unarmed, Self::DoubleSlash, Self::TripleSlash, Self::DoubleThrust, Self::TripleThrust, Self::OffhandThrust, Self::OffhandSlash, Self::OffhandDoubleSlash, Self::OffhandTripleSlash, Self::Slashes, Self::OffhandDoubleThrust, Self::DoubleStrike, Self::OffhandTripleThrust, Self::TripleStrike, Self::Thrusts, Self::MultiStrike, Self::Offhand];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Punch", "Thrust", "Slash", "Kick", "OffhandPunch", "Punches", "Unarmed", "DoubleSlash", "TripleSlash", "DoubleThrust", "TripleThrust", "OffhandThrust", "OffhandSlash", "OffhandDoubleSlash", "OffhandTripleSlash", "Slashes", "OffhandDoubleThrust", "DoubleStrike", "OffhandTripleThrust", "TripleStrike", "Thrusts", "MultiStrike", "Offhand"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 18, 10, 4, 22, 23, 14, 17, 5, 13, 12, 15, 19, 1, 6, 3, 16, 2, 21, 9, 20, 11, 7, 0];
}

super::support::ace_enum!(AttackType, i32, flags);
super::support::ace_enum_from!(AttackType, i32 => i64);
