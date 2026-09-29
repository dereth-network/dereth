// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CommandMasks.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CommandMasks.cs`; do not edit by hand

/// ACE enum `CommandMask` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CommandMask(pub u32);

#[allow(non_upper_case_globals)]
impl CommandMask {
    pub const Style: Self = Self(0x80000000);
    pub const SubState: Self = Self(0x40000000);
    pub const Modifier: Self = Self(0x20000000);
    pub const Action: Self = Self(0x10000000);
    pub const UI: Self = Self(0x8000000);
    pub const Toggle: Self = Self(0x4000000);
    pub const ChatEmote: Self = Self(0x2000000);
    pub const Mappable: Self = Self(0x1000000);
    pub const Command: Self = Self(0xFFFFFF);
}

impl CommandMask {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Command, Self::Mappable, Self::ChatEmote, Self::Toggle, Self::UI, Self::Action, Self::Modifier, Self::SubState, Self::Style];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Command", "Mappable", "ChatEmote", "Toggle", "UI", "Action", "Modifier", "SubState", "Style"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 2, 0, 1, 6, 8, 7, 3, 4];
}

super::support::ace_enum!(CommandMask, u32, flags);
super::support::ace_enum_from!(CommandMask, u32 => u64, i64);
