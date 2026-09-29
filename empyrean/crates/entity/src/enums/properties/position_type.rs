// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PositionType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PositionType.cs`; do not edit by hand

/// The enumerations for the different positions we want to track. This code provided by Ripley via pastebin
///
/// ACE enum `PositionType`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PositionType(pub u16);

#[allow(non_upper_case_globals)]
impl PositionType {
    pub const Undef: Self = Self(0);
    pub const Location: Self = Self(1);
    pub const Destination: Self = Self(2);
    pub const Instantiation: Self = Self(3);
    pub const Sanctuary: Self = Self(4);
    pub const Home: Self = Self(5);
    pub const ActivationMove: Self = Self(6);
    pub const Target: Self = Self(7);
    pub const LinkedPortalOne: Self = Self(8);
    pub const LastPortal: Self = Self(9);
    pub const PortalStorm: Self = Self(10);
    pub const CrashAndTurn: Self = Self(11);
    pub const PortalSummonLoc: Self = Self(12);
    pub const HouseBoot: Self = Self(13);
    pub const LastOutsideDeath: Self = Self(14);
    pub const LinkedLifestone: Self = Self(15);
    pub const LinkedPortalTwo: Self = Self(16);
    pub const Save1: Self = Self(17);
    pub const Save2: Self = Self(18);
    pub const Save3: Self = Self(19);
    pub const Save4: Self = Self(20);
    pub const Save5: Self = Self(21);
    pub const Save6: Self = Self(22);
    pub const Save7: Self = Self(23);
    pub const Save8: Self = Self(24);
    pub const Save9: Self = Self(25);
    pub const RelativeDestination: Self = Self(26);
    pub const TeleportedCharacter: Self = Self(27);
    pub const PCAPRecordedLocation: Self = Self(8040);
}

impl PositionType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Location, Self::Destination, Self::Instantiation, Self::Sanctuary, Self::Home, Self::ActivationMove, Self::Target, Self::LinkedPortalOne, Self::LastPortal, Self::PortalStorm, Self::CrashAndTurn, Self::PortalSummonLoc, Self::HouseBoot, Self::LastOutsideDeath, Self::LinkedLifestone, Self::LinkedPortalTwo, Self::Save1, Self::Save2, Self::Save3, Self::Save4, Self::Save5, Self::Save6, Self::Save7, Self::Save8, Self::Save9, Self::RelativeDestination, Self::TeleportedCharacter, Self::PCAPRecordedLocation];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Location", "Destination", "Instantiation", "Sanctuary", "Home", "ActivationMove", "Target", "LinkedPortalOne", "LastPortal", "PortalStorm", "CrashAndTurn", "PortalSummonLoc", "HouseBoot", "LastOutsideDeath", "LinkedLifestone", "LinkedPortalTwo", "Save1", "Save2", "Save3", "Save4", "Save5", "Save6", "Save7", "Save8", "Save9", "RelativeDestination", "TeleportedCharacter", "PCAPRecordedLocation"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 11, 2, 5, 13, 3, 14, 9, 15, 8, 16, 1, 28, 10, 12, 26, 4, 17, 18, 19, 20, 21, 22, 23, 24, 25, 7, 27, 0];

    /// The members marked `[Ephemeral]`, in declaration order (ACE's reflection order).
    pub const EPHEMERAL: &'static [Self] = &[Self::Home];
    /// Whether this value is marked `[Ephemeral]` (by value, like ACE's `HashSet` lookups).
    pub fn is_ephemeral(self) -> bool {
        Self::EPHEMERAL.contains(&self)
    }
}

super::support::ace_enum!(PositionType, u16, plain);
super::support::ace_enum_from!(PositionType, u16 => u32, u64, i32, i64);
