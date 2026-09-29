// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyInstanceId.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Properties/PropertyInstanceId.cs`; do not edit by hand

/// ACE enum `PropertyInstanceId`, underlying `ushort`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyInstanceId(pub u16);

#[allow(non_upper_case_globals)]
impl PropertyInstanceId {
    pub const Undef: Self = Self(0);
    pub const Owner: Self = Self(1);
    pub const Container: Self = Self(2);
    pub const Wielder: Self = Self(3);
    pub const Freezer: Self = Self(4);
    pub const Viewer: Self = Self(5);
    pub const Generator: Self = Self(6);
    pub const Scribe: Self = Self(7);
    pub const CurrentCombatTarget: Self = Self(8);
    pub const CurrentEnemy: Self = Self(9);
    pub const ProjectileLauncher: Self = Self(10);
    pub const CurrentAttacker: Self = Self(11);
    pub const CurrentDamager: Self = Self(12);
    pub const CurrentFollowTarget: Self = Self(13);
    pub const CurrentAppraisalTarget: Self = Self(14);
    pub const CurrentFellowshipAppraisalTarget: Self = Self(15);
    pub const ActivationTarget: Self = Self(16);
    pub const Creator: Self = Self(17);
    pub const Victim: Self = Self(18);
    pub const Killer: Self = Self(19);
    pub const Vendor: Self = Self(20);
    pub const Customer: Self = Self(21);
    pub const Bonded: Self = Self(22);
    pub const Wounder: Self = Self(23);
    pub const Allegiance: Self = Self(24);
    pub const Patron: Self = Self(25);
    pub const Monarch: Self = Self(26);
    pub const CombatTarget: Self = Self(27);
    pub const HealthQueryTarget: Self = Self(28);
    pub const LastUnlocker: Self = Self(29);
    pub const CrashAndTurnTarget: Self = Self(30);
    pub const AllowedActivator: Self = Self(31);
    pub const HouseOwner: Self = Self(32);
    pub const House: Self = Self(33);
    pub const Slumlord: Self = Self(34);
    pub const ManaQueryTarget: Self = Self(35);
    pub const CurrentGame: Self = Self(36);
    pub const RequestedAppraisalTarget: Self = Self(37);
    pub const AllowedWielder: Self = Self(38);
    pub const AssignedTarget: Self = Self(39);
    pub const LimboSource: Self = Self(40);
    pub const Snooper: Self = Self(41);
    pub const TeleportedCharacter: Self = Self(42);
    pub const Pet: Self = Self(43);
    pub const PetOwner: Self = Self(44);
    pub const PetDevice: Self = Self(45);
    pub const PCAPRecordedObjectIID: Self = Self(8000);
    pub const PCAPRecordedParentIID: Self = Self(8008);
}

impl PropertyInstanceId {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Owner, Self::Container, Self::Wielder, Self::Freezer, Self::Viewer, Self::Generator, Self::Scribe, Self::CurrentCombatTarget, Self::CurrentEnemy, Self::ProjectileLauncher, Self::CurrentAttacker, Self::CurrentDamager, Self::CurrentFollowTarget, Self::CurrentAppraisalTarget, Self::CurrentFellowshipAppraisalTarget, Self::ActivationTarget, Self::Creator, Self::Victim, Self::Killer, Self::Vendor, Self::Customer, Self::Bonded, Self::Wounder, Self::Allegiance, Self::Patron, Self::Monarch, Self::CombatTarget, Self::HealthQueryTarget, Self::LastUnlocker, Self::CrashAndTurnTarget, Self::AllowedActivator, Self::HouseOwner, Self::House, Self::Slumlord, Self::ManaQueryTarget, Self::CurrentGame, Self::RequestedAppraisalTarget, Self::AllowedWielder, Self::AssignedTarget, Self::LimboSource, Self::Snooper, Self::TeleportedCharacter, Self::Pet, Self::PetOwner, Self::PetDevice, Self::PCAPRecordedObjectIID, Self::PCAPRecordedParentIID];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Owner", "Container", "Wielder", "Freezer", "Viewer", "Generator", "Scribe", "CurrentCombatTarget", "CurrentEnemy", "ProjectileLauncher", "CurrentAttacker", "CurrentDamager", "CurrentFollowTarget", "CurrentAppraisalTarget", "CurrentFellowshipAppraisalTarget", "ActivationTarget", "Creator", "Victim", "Killer", "Vendor", "Customer", "Bonded", "Wounder", "Allegiance", "Patron", "Monarch", "CombatTarget", "HealthQueryTarget", "LastUnlocker", "CrashAndTurnTarget", "AllowedActivator", "HouseOwner", "House", "Slumlord", "ManaQueryTarget", "CurrentGame", "RequestedAppraisalTarget", "AllowedWielder", "AssignedTarget", "LimboSource", "Snooper", "TeleportedCharacter", "Pet", "PetOwner", "PetDevice", "PCAPRecordedObjectIID", "PCAPRecordedParentIID"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[16, 24, 31, 38, 39, 22, 27, 2, 30, 17, 14, 11, 8, 12, 9, 15, 13, 36, 21, 4, 6, 28, 33, 32, 19, 29, 40, 35, 26, 1, 46, 47, 25, 43, 45, 44, 10, 37, 7, 34, 41, 42, 0, 20, 18, 5, 3, 23];

    /// The members marked `[Ephemeral]`, in declaration order (ACE's reflection order).
    pub const EPHEMERAL: &'static [Self] = &[Self::Viewer, Self::Generator, Self::CurrentCombatTarget, Self::CurrentEnemy, Self::CurrentAttacker, Self::CurrentDamager, Self::CurrentFollowTarget, Self::CurrentAppraisalTarget, Self::CurrentFellowshipAppraisalTarget, Self::CombatTarget, Self::HealthQueryTarget, Self::LastUnlocker, Self::ManaQueryTarget, Self::RequestedAppraisalTarget, Self::Pet, Self::PetDevice];
    /// Whether this value is marked `[Ephemeral]` (by value, like ACE's `HashSet` lookups).
    pub fn is_ephemeral(self) -> bool {
        Self::EPHEMERAL.contains(&self)
    }

    /// The members marked `[SendOnLogin]`, in declaration order (ACE's reflection order).
    pub const SEND_ON_LOGIN: &'static [Self] = &[Self::Allegiance, Self::Patron];
    /// Whether this value is marked `[SendOnLogin]` (by value, like ACE's `HashSet` lookups).
    pub fn is_send_on_login(self) -> bool {
        Self::SEND_ON_LOGIN.contains(&self)
    }
}

super::support::ace_enum!(PropertyInstanceId, u16, plain);
super::support::ace_enum_from!(PropertyInstanceId, u16 => u32, u64, i32, i64);
