// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Channel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Channel.cs`; do not edit by hand

/// The Channel identifies the type of chat message.
/// Used with F7B0 0147: Game Event -> Group Chat (ChatChannel)
///
/// ACE enum `Channel` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Channel(pub i32);

#[allow(non_upper_case_globals)]
impl Channel {
    pub const Undef: Self = Self(0x0);
    pub const Abuse: Self = Self(0x1);
    pub const Admin: Self = Self(0x2);
    pub const Audit: Self = Self(0x4);
    pub const Advocate1: Self = Self(0x8);
    pub const Advocate2: Self = Self(0x10);
    pub const Advocate3: Self = Self(0x20);
    pub const QA1: Self = Self(0x40);
    pub const QA2: Self = Self(0x80);
    pub const Debug: Self = Self(0x100);
    pub const Sentinel: Self = Self(0x200);
    pub const Help: Self = Self(0x400);
    pub const AllBroadcast: Self = Self(0x401);
    pub const ValidChans: Self = Self(0x73F);
    pub const Fellow: Self = Self(0x800);
    pub const Vassals: Self = Self(0x1000);
    pub const Patron: Self = Self(0x2000);
    pub const Monarch: Self = Self(0x4000);
    pub const AlArqas: Self = Self(0x8000);
    pub const Holtburg: Self = Self(0x10000);
    pub const Lytelthorpe: Self = Self(0x20000);
    pub const Nanto: Self = Self(0x40000);
    pub const Rithwic: Self = Self(0x80000);
    pub const Samsur: Self = Self(0x100000);
    pub const Shoushi: Self = Self(0x200000);
    pub const Yanshi: Self = Self(0x400000);
    pub const Yaraq: Self = Self(0x800000);
    pub const TownChans: Self = Self(0xFF8000);
    pub const CoVassals: Self = Self(0x1000000);
    pub const AllegianceBroadcast: Self = Self(0x2000000);
    pub const FellowBroadcast: Self = Self(0x4000000);
    pub const SocietyCelHanBroadcast: Self = Self(0x8000000);
    pub const SocietyEldWebBroadcast: Self = Self(0x10000000);
    pub const SocietyRadBloBroadcast: Self = Self(0x20000000);
    pub const Olthoi: Self = Self(0x40000000);
    pub const GhostChans: Self = Self(0x7F007800);
    pub const AllChans: Self = Self(0x7F007F3F);
}

impl Channel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Abuse, Self::Admin, Self::Audit, Self::Advocate1, Self::Advocate2, Self::Advocate3, Self::QA1, Self::QA2, Self::Debug, Self::Sentinel, Self::Help, Self::AllBroadcast, Self::ValidChans, Self::Fellow, Self::Vassals, Self::Patron, Self::Monarch, Self::AlArqas, Self::Holtburg, Self::Lytelthorpe, Self::Nanto, Self::Rithwic, Self::Samsur, Self::Shoushi, Self::Yanshi, Self::Yaraq, Self::TownChans, Self::CoVassals, Self::AllegianceBroadcast, Self::FellowBroadcast, Self::SocietyCelHanBroadcast, Self::SocietyEldWebBroadcast, Self::SocietyRadBloBroadcast, Self::Olthoi, Self::GhostChans, Self::AllChans];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Abuse", "Admin", "Audit", "Advocate1", "Advocate2", "Advocate3", "QA1", "QA2", "Debug", "Sentinel", "Help", "AllBroadcast", "ValidChans", "Fellow", "Vassals", "Patron", "Monarch", "AlArqas", "Holtburg", "Lytelthorpe", "Nanto", "Rithwic", "Samsur", "Shoushi", "Yanshi", "Yaraq", "TownChans", "CoVassals", "AllegianceBroadcast", "FellowBroadcast", "SocietyCelHanBroadcast", "SocietyEldWebBroadcast", "SocietyRadBloBroadcast", "Olthoi", "GhostChans", "AllChans"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 4, 5, 6, 18, 12, 36, 29, 3, 28, 9, 14, 30, 35, 11, 19, 20, 17, 21, 34, 16, 7, 8, 22, 23, 10, 24, 31, 32, 33, 27, 0, 13, 15, 25, 26];
}

super::support::ace_enum!(Channel, i32, flags);
super::support::ace_enum_from!(Channel, i32 => i64);
