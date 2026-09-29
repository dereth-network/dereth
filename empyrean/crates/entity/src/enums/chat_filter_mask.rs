// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatFilterMask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatFilterMask.cs`; do not edit by hand

/// The ChatFilterMask identifies types of messages that are squelched or filtered (/messagetypes).
/// Used with F7B0 01F4: Game Event -> Squelch and Filter List
///
/// ACE enum `ChatFilterMask` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatFilterMask(pub i32);

#[allow(non_upper_case_globals)]
impl ChatFilterMask {
    pub const Speech: Self = Self(0x4);
    pub const Tell: Self = Self(0x8);
    pub const Combat: Self = Self(0x40);
    pub const Magic: Self = Self(0x80);
    pub const Emote: Self = Self(0x1000);
    pub const Appraisal: Self = Self(0x10000);
    pub const Spellcasting: Self = Self(0x20000);
    pub const Allegiance: Self = Self(0x40000);
    pub const Fellowship: Self = Self(0x80000);
    pub const Combat_Enemy: Self = Self(0x200000);
    pub const Combat_Self: Self = Self(0x400000);
    pub const Recall: Self = Self(0x800000);
    pub const Craft: Self = Self(0x1000000);
    pub const Salvaging: Self = Self(0x2000000);
    pub const AllMessageTypes: Self = Self(-1);
}

impl ChatFilterMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Speech, Self::Tell, Self::Combat, Self::Magic, Self::Emote, Self::Appraisal, Self::Spellcasting, Self::Allegiance, Self::Fellowship, Self::Combat_Enemy, Self::Combat_Self, Self::Recall, Self::Craft, Self::Salvaging, Self::AllMessageTypes];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Speech", "Tell", "Combat", "Magic", "Emote", "Appraisal", "Spellcasting", "Allegiance", "Fellowship", "Combat_Enemy", "Combat_Self", "Recall", "Craft", "Salvaging", "AllMessageTypes"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[14, 7, 5, 2, 9, 10, 12, 4, 8, 3, 11, 13, 0, 6, 1];
}

super::support::ace_enum!(ChatFilterMask, i32, flags);
super::support::ace_enum_from!(ChatFilterMask, i32 => i64);
