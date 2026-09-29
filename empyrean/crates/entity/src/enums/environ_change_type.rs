// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EnvironChangeType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EnvironChangeType.cs`; do not edit by hand

/// ACE enum `EnvironChangeType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EnvironChangeType(pub i32);

#[allow(non_upper_case_globals)]
impl EnvironChangeType {
    pub const Clear: Self = Self(0);
    pub const RedFog: Self = Self(1);
    pub const BlueFog: Self = Self(2);
    pub const WhiteFog: Self = Self(3);
    pub const GreenFog: Self = Self(4);
    pub const BlackFog: Self = Self(5);
    pub const BlackFog2: Self = Self(6);
    pub const RoarSound: Self = Self(101);
    pub const BellSound: Self = Self(102);
    pub const Chant1Sound: Self = Self(103);
    pub const Chant2Sound: Self = Self(104);
    pub const DarkWhispers1Sound: Self = Self(105);
    pub const DarkWhispers2Sound: Self = Self(106);
    pub const DarkLaughSound: Self = Self(107);
    pub const DarkWindSound: Self = Self(108);
    pub const DarkSpeechSound: Self = Self(109);
    pub const DrumsSound: Self = Self(110);
    pub const GhostSpeakSound: Self = Self(111);
    pub const BreathingSound: Self = Self(112);
    pub const HowlSound: Self = Self(113);
    pub const LostSoulsSound: Self = Self(114);
    pub const SquealSound: Self = Self(117);
    pub const Thunder1Sound: Self = Self(118);
    pub const Thunder2Sound: Self = Self(119);
    pub const Thunder3Sound: Self = Self(120);
    pub const Thunder4Sound: Self = Self(121);
    pub const Thunder5Sound: Self = Self(122);
    pub const Thunder6Sound: Self = Self(123);
}

impl EnvironChangeType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Clear, Self::RedFog, Self::BlueFog, Self::WhiteFog, Self::GreenFog, Self::BlackFog, Self::BlackFog2, Self::RoarSound, Self::BellSound, Self::Chant1Sound, Self::Chant2Sound, Self::DarkWhispers1Sound, Self::DarkWhispers2Sound, Self::DarkLaughSound, Self::DarkWindSound, Self::DarkSpeechSound, Self::DrumsSound, Self::GhostSpeakSound, Self::BreathingSound, Self::HowlSound, Self::LostSoulsSound, Self::SquealSound, Self::Thunder1Sound, Self::Thunder2Sound, Self::Thunder3Sound, Self::Thunder4Sound, Self::Thunder5Sound, Self::Thunder6Sound];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Clear", "RedFog", "BlueFog", "WhiteFog", "GreenFog", "BlackFog", "BlackFog2", "RoarSound", "BellSound", "Chant1Sound", "Chant2Sound", "DarkWhispers1Sound", "DarkWhispers2Sound", "DarkLaughSound", "DarkWindSound", "DarkSpeechSound", "DrumsSound", "GhostSpeakSound", "BreathingSound", "HowlSound", "LostSoulsSound", "SquealSound", "Thunder1Sound", "Thunder2Sound", "Thunder3Sound", "Thunder4Sound", "Thunder5Sound", "Thunder6Sound"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 5, 6, 2, 18, 9, 10, 0, 13, 15, 11, 12, 14, 16, 17, 4, 19, 20, 1, 7, 21, 22, 23, 24, 25, 26, 27, 3];
}

super::support::ace_enum!(EnvironChangeType, i32, plain);
super::support::ace_enum_from!(EnvironChangeType, i32 => i64);
