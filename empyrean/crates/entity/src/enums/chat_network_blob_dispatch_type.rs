// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatNetworkBlobDispatchType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatNetworkBlobDispatchType.cs`; do not edit by hand

/// ACE enum `ChatNetworkBlobDispatchType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatNetworkBlobDispatchType(pub i32);

#[allow(non_upper_case_globals)]
impl ChatNetworkBlobDispatchType {
    pub const ASYNCMETHOD_UNKNOWN: Self = Self(0);
    pub const ASYNCMETHOD_SENDTOROOMBYNAME: Self = Self(1);
    pub const ASYNCMETHOD_SENDTOROOMBYID: Self = Self(2);
    pub const ASYNCMETHOD_CREATEROOM: Self = Self(3);
    pub const ASYNCMETHOD_INVITECLIENTTOROOMBYID: Self = Self(4);
    pub const ASYNCMETHOD_EJECTCLIENTFROMROOMBYID: Self = Self(5);
}

impl ChatNetworkBlobDispatchType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::ASYNCMETHOD_UNKNOWN, Self::ASYNCMETHOD_SENDTOROOMBYNAME, Self::ASYNCMETHOD_SENDTOROOMBYID, Self::ASYNCMETHOD_CREATEROOM, Self::ASYNCMETHOD_INVITECLIENTTOROOMBYID, Self::ASYNCMETHOD_EJECTCLIENTFROMROOMBYID];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["ASYNCMETHOD_UNKNOWN", "ASYNCMETHOD_SENDTOROOMBYNAME", "ASYNCMETHOD_SENDTOROOMBYID", "ASYNCMETHOD_CREATEROOM", "ASYNCMETHOD_INVITECLIENTTOROOMBYID", "ASYNCMETHOD_EJECTCLIENTFROMROOMBYID"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 5, 4, 2, 1, 0];
}

super::support::ace_enum!(ChatNetworkBlobDispatchType, i32, plain);
super::support::ace_enum_from!(ChatNetworkBlobDispatchType, i32 => i64);
