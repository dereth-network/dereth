// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Usable.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Usable.cs`; do not edit by hand

/// ACE enum `Usable` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Usable(pub u32);

#[allow(non_upper_case_globals)]
impl Usable {
    pub const Undef: Self = Self(0x0);
    pub const No: Self = Self(0x1);
    /// `Self` in ACE; `Self` is a Rust keyword. DIVERGE: renamed.
    pub const Self_: Self = Self(0x2);
    pub const Wielded: Self = Self(0x4);
    pub const Contained: Self = Self(0x8);
    pub const Viewed: Self = Self(0x10);
    pub const Remote: Self = Self(0x20);
    pub const NeverWalk: Self = Self(0x40);
    pub const ObjSelf: Self = Self(0x80);
    pub const ContainedViewed: Self = Self(0x18);
    pub const ContainedViewedRemote: Self = Self(0x38);
    pub const ContainedViewedRemoteNeverWalk: Self = Self(0x78);
    pub const ViewedRemote: Self = Self(0x30);
    pub const ViewedRemoteNeverWalk: Self = Self(0x70);
    pub const RemoteNeverWalk: Self = Self(0x60);
    pub const SourceWieldedTargetWielded: Self = Self(0x40004);
    pub const SourceWieldedTargetContained: Self = Self(0x80004);
    pub const SourceWieldedTargetViewed: Self = Self(0x100004);
    pub const SourceWieldedTargetRemote: Self = Self(0x200004);
    pub const SourceWieldedTargetRemoteNeverWalk: Self = Self(0x600004);
    pub const SourceContainedTargetWielded: Self = Self(0x40008);
    pub const SourceContainedTargetContained: Self = Self(0x80008);
    pub const SourceContainedTargetObjselfOrContained: Self = Self(0x880008);
    pub const SourceContainedTargetSelfOrContained: Self = Self(0xA0008);
    pub const SourceContainedTargetViewed: Self = Self(0x100008);
    pub const SourceContainedTargetRemote: Self = Self(0x200008);
    pub const SourceContainedTargetRemoteNeverWalk: Self = Self(0x600008);
    pub const SourceContainedTargetRemoteOrSelf: Self = Self(0x220008);
    pub const SourceViewedTargetWielded: Self = Self(0x40010);
    pub const SourceViewedTargetContained: Self = Self(0x80010);
    pub const SourceViewedTargetViewed: Self = Self(0x100010);
    pub const SourceViewedTargetRemote: Self = Self(0x200010);
    pub const SourceRemoteTargetWielded: Self = Self(0x40020);
    pub const SourceRemoteTargetContained: Self = Self(0x80020);
    pub const SourceRemoteTargetViewed: Self = Self(0x100020);
    pub const SourceRemoteTargetRemote: Self = Self(0x200020);
    pub const SourceRemoteTargetRemoteNeverWalk: Self = Self(0x600020);
    pub const SourceMask: Self = Self(0xFFFF);
    pub const TargetMask: Self = Self(0xFFFF0000);
}

impl Usable {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::No, Self::Self_, Self::Wielded, Self::Contained, Self::Viewed, Self::ContainedViewed, Self::Remote, Self::ViewedRemote, Self::ContainedViewedRemote, Self::NeverWalk, Self::RemoteNeverWalk, Self::ViewedRemoteNeverWalk, Self::ContainedViewedRemoteNeverWalk, Self::ObjSelf, Self::SourceMask, Self::SourceWieldedTargetWielded, Self::SourceContainedTargetWielded, Self::SourceViewedTargetWielded, Self::SourceRemoteTargetWielded, Self::SourceWieldedTargetContained, Self::SourceContainedTargetContained, Self::SourceViewedTargetContained, Self::SourceRemoteTargetContained, Self::SourceContainedTargetSelfOrContained, Self::SourceWieldedTargetViewed, Self::SourceContainedTargetViewed, Self::SourceViewedTargetViewed, Self::SourceRemoteTargetViewed, Self::SourceWieldedTargetRemote, Self::SourceContainedTargetRemote, Self::SourceViewedTargetRemote, Self::SourceRemoteTargetRemote, Self::SourceContainedTargetRemoteOrSelf, Self::SourceWieldedTargetRemoteNeverWalk, Self::SourceContainedTargetRemoteNeverWalk, Self::SourceRemoteTargetRemoteNeverWalk, Self::SourceContainedTargetObjselfOrContained, Self::TargetMask];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "No", "Self", "Wielded", "Contained", "Viewed", "ContainedViewed", "Remote", "ViewedRemote", "ContainedViewedRemote", "NeverWalk", "RemoteNeverWalk", "ViewedRemoteNeverWalk", "ContainedViewedRemoteNeverWalk", "ObjSelf", "SourceMask", "SourceWieldedTargetWielded", "SourceContainedTargetWielded", "SourceViewedTargetWielded", "SourceRemoteTargetWielded", "SourceWieldedTargetContained", "SourceContainedTargetContained", "SourceViewedTargetContained", "SourceRemoteTargetContained", "SourceContainedTargetSelfOrContained", "SourceWieldedTargetViewed", "SourceContainedTargetViewed", "SourceViewedTargetViewed", "SourceRemoteTargetViewed", "SourceWieldedTargetRemote", "SourceContainedTargetRemote", "SourceViewedTargetRemote", "SourceRemoteTargetRemote", "SourceContainedTargetRemoteOrSelf", "SourceWieldedTargetRemoteNeverWalk", "SourceContainedTargetRemoteNeverWalk", "SourceRemoteTargetRemoteNeverWalk", "SourceContainedTargetObjselfOrContained", "TargetMask"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 6, 9, 13, 10, 1, 14, 7, 11, 2, 21, 37, 30, 35, 33, 24, 26, 17, 15, 23, 32, 36, 28, 19, 22, 31, 27, 18, 20, 29, 34, 25, 16, 38, 0, 5, 8, 12, 3];
}

super::support::ace_enum!(Usable, u32, flags);
super::support::ace_enum_from!(Usable, u32 => u64, i64);
