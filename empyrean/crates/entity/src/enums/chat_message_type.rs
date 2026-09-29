// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatMessageType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatMessageType.cs`; do not edit by hand

/// The ChatMessageType categorizes chat window messages to control color and filtering.
/// Used with 02BB: Creature Message
/// 0x02, 0x0C, 0x11 Used with 02BC: Creature Message (Ranged)
/// 0x0C Used with F7B0 02BD: Game Event -> Someone has sent you a @tell.
/// 0x03, Used with F7E0: Server Message 0x00, 0x03, 0x04, 0x05, 0x06, 0x07, 0x0D, 0x10, 0x11, 0x17, 0x18
///
/// ACE enum `ChatMessageType`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatMessageType(pub u32);

#[allow(non_upper_case_globals)]
impl ChatMessageType {
    pub const Broadcast: Self = Self(0);
    pub const AllChannels: Self = Self(1);
    pub const Speech: Self = Self(2);
    pub const Tell: Self = Self(3);
    pub const OutgoingTell: Self = Self(4);
    pub const System: Self = Self(5);
    pub const Combat: Self = Self(6);
    pub const Magic: Self = Self(7);
    pub const Channel: Self = Self(8);
    pub const ChannelSend: Self = Self(9);
    pub const Social: Self = Self(10);
    pub const SocialSend: Self = Self(11);
    pub const Emote: Self = Self(12);
    pub const Advancement: Self = Self(13);
    pub const Abuse: Self = Self(14);
    pub const Help: Self = Self(15);
    pub const Appraisal: Self = Self(16);
    pub const Spellcasting: Self = Self(17);
    pub const Allegiance: Self = Self(18);
    pub const Fellowship: Self = Self(19);
    pub const WorldBroadcast: Self = Self(20);
    pub const CombatEnemy: Self = Self(21);
    pub const CombatSelf: Self = Self(22);
    pub const Recall: Self = Self(23);
    pub const Craft: Self = Self(24);
    pub const Salvaging: Self = Self(25);
    pub const x1B: Self = Self(27);
    pub const x1C: Self = Self(28);
    pub const x1D: Self = Self(29);
    pub const x1E: Self = Self(30);
    pub const AdminTell: Self = Self(31);
}

impl ChatMessageType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Broadcast, Self::AllChannels, Self::Speech, Self::Tell, Self::OutgoingTell, Self::System, Self::Combat, Self::Magic, Self::Channel, Self::ChannelSend, Self::Social, Self::SocialSend, Self::Emote, Self::Advancement, Self::Abuse, Self::Help, Self::Appraisal, Self::Spellcasting, Self::Allegiance, Self::Fellowship, Self::WorldBroadcast, Self::CombatEnemy, Self::CombatSelf, Self::Recall, Self::Craft, Self::Salvaging, Self::x1B, Self::x1C, Self::x1D, Self::x1E, Self::AdminTell];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Broadcast", "AllChannels", "Speech", "Tell", "OutgoingTell", "System", "Combat", "Magic", "Channel", "ChannelSend", "Social", "SocialSend", "Emote", "Advancement", "Abuse", "Help", "Appraisal", "Spellcasting", "Allegiance", "Fellowship", "WorldBroadcast", "CombatEnemy", "CombatSelf", "Recall", "Craft", "Salvaging", "x1B", "x1C", "x1D", "x1E", "AdminTell"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[14, 30, 13, 1, 18, 16, 0, 8, 9, 6, 21, 22, 24, 12, 19, 15, 7, 4, 23, 25, 10, 11, 2, 17, 5, 3, 20, 26, 27, 28, 29];
}

super::support::ace_enum!(ChatMessageType, u32, plain);
super::support::ace_enum_from!(ChatMessageType, u32 => u64, i64);
