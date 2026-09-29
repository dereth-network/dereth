// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MediaType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MediaType.cs`; do not edit by hand

/// ACE enum `MediaType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MediaType(pub i32);

#[allow(non_upper_case_globals)]
impl MediaType {
    pub const Undef: Self = Self(0);
    pub const Movie: Self = Self(1);
    pub const Alpha: Self = Self(2);
    pub const Animation: Self = Self(3);
    pub const Cursor: Self = Self(4);
    pub const Image: Self = Self(5);
    pub const Jump: Self = Self(6);
    pub const Message: Self = Self(7);
    pub const Pause: Self = Self(8);
    pub const Sound: Self = Self(9);
    pub const State: Self = Self(10);
    pub const Fade: Self = Self(11);
    pub const Stretch: Self = Self(12);
}

impl MediaType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Movie, Self::Alpha, Self::Animation, Self::Cursor, Self::Image, Self::Jump, Self::Message, Self::Pause, Self::Sound, Self::State, Self::Fade, Self::Stretch];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Movie", "Alpha", "Animation", "Cursor", "Image", "Jump", "Message", "Pause", "Sound", "State", "Fade", "Stretch"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 3, 4, 11, 5, 6, 7, 1, 8, 9, 10, 12, 0];
}

super::support::ace_enum!(MediaType, i32, plain);
super::support::ace_enum_from!(MediaType, i32 => i64);
