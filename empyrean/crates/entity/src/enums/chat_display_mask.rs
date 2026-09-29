// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatDisplayMask.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatDisplayMask.cs`; do not edit by hand

/// The ChatDisplayMask identifies that types of chat that are displayed in each chat window.
/// Used by CharacterOptionData: The CharacterOptionData structure contains character options.
///
/// ACE enum `ChatDisplayMask`, underlying `long`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatDisplayMask(pub i64);

#[allow(non_upper_case_globals)]
impl ChatDisplayMask {
    pub const Gameplay: Self = Self(0x3912021);
    pub const Mandatory: Self = Self(49922);
    pub const AreaChat: Self = Self(4100);
    pub const Tells: Self = Self(24);
    pub const Combat: Self = Self(0x600040);
    pub const Magic: Self = Self(0x20080);
    pub const Allegiance: Self = Self(0x40C00);
    pub const Fellowship: Self = Self(0x80000);
    pub const Errors: Self = Self(0x4000000);
    pub const GeneralChannel: Self = Self(0x8000000);
    pub const TradeChannel: Self = Self(0x10000000);
    pub const LFGChannel: Self = Self(0x20000000);
    pub const RoleplayChannel: Self = Self(0x40000000);
}

impl ChatDisplayMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Tells, Self::AreaChat, Self::Mandatory, Self::Magic, Self::Allegiance, Self::Fellowship, Self::Combat, Self::Gameplay, Self::Errors, Self::GeneralChannel, Self::TradeChannel, Self::LFGChannel, Self::RoleplayChannel];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Tells", "AreaChat", "Mandatory", "Magic", "Allegiance", "Fellowship", "Combat", "Gameplay", "Errors", "GeneralChannel", "TradeChannel", "LFGChannel", "RoleplayChannel"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 6, 8, 5, 7, 9, 11, 3, 2, 12, 0, 10];
}

super::support::ace_enum!(ChatDisplayMask, i64, plain);
super::support::ace_enum_from!(ChatDisplayMask, i64);
