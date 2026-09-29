// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Quadrant.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Quadrant.cs`; do not edit by hand

/// ACE enum `Quadrant` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Quadrant(pub i32);

#[allow(non_upper_case_globals)]
impl Quadrant {
    pub const None: Self = Self(0x0);
    pub const High: Self = Self(0x1);
    pub const Medium: Self = Self(0x2);
    pub const Low: Self = Self(0x4);
    pub const Left: Self = Self(0x8);
    pub const Right: Self = Self(0x10);
    pub const Front: Self = Self(0x20);
    pub const Back: Self = Self(0x40);
}

impl Quadrant {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::High, Self::Medium, Self::Low, Self::Left, Self::Right, Self::Front, Self::Back];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "High", "Medium", "Low", "Left", "Right", "Front", "Back"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 6, 1, 4, 3, 2, 0, 5];
}

super::support::ace_enum!(Quadrant, i32, flags);
super::support::ace_enum_from!(Quadrant, i32 => i64);
