// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ChatNetworkBlobType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ChatNetworkBlobType.cs`; do not edit by hand

/// The ChatNetworkBlobType identifies the type of Turbine Chat message.
/// Used with F7DE: Turbine Chat
///
/// ACE enum `ChatNetworkBlobType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ChatNetworkBlobType(pub i32);

#[allow(non_upper_case_globals)]
impl ChatNetworkBlobType {
    pub const NETBLOB_UNKNOWN: Self = Self(0);
    pub const NETBLOB_EVENT_BINARY: Self = Self(1);
    pub const NETBLOB_EVENT_XMLRPC: Self = Self(2);
    pub const NETBLOB_REQUEST_BINARY: Self = Self(3);
    pub const NETBLOB_REQUEST_XMLRPC: Self = Self(4);
    pub const NETBLOB_RESPONSE_BINARY: Self = Self(5);
    pub const NETBLOB_RESPONSE_XMLRPC: Self = Self(6);
}

impl ChatNetworkBlobType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::NETBLOB_UNKNOWN, Self::NETBLOB_EVENT_BINARY, Self::NETBLOB_EVENT_XMLRPC, Self::NETBLOB_REQUEST_BINARY, Self::NETBLOB_REQUEST_XMLRPC, Self::NETBLOB_RESPONSE_BINARY, Self::NETBLOB_RESPONSE_XMLRPC];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["NETBLOB_UNKNOWN", "NETBLOB_EVENT_BINARY", "NETBLOB_EVENT_XMLRPC", "NETBLOB_REQUEST_BINARY", "NETBLOB_REQUEST_XMLRPC", "NETBLOB_RESPONSE_BINARY", "NETBLOB_RESPONSE_XMLRPC"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 4, 5, 6, 0];
}

super::support::ace_enum!(ChatNetworkBlobType, i32, plain);
super::support::ace_enum_from!(ChatNetworkBlobType, i32 => i64);
