// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/PacketHeaderFlagsUtil.cs
//
// `PacketHeaderFlags` itself is shared: `dereth_transport::wire::PacketFlags` carries the same 22
// values under their retail names. This file keeps ACE's names for log output only.

/// ACE's `PacketHeaderFlags` names, in the enum's declaration (value) order.
pub const PACKET_HEADER_FLAG_NAMES: [(u32, &str); 23] = [
    (0x0000_0000, "None"),
    (0x0000_0001, "Retransmission"),
    (0x0000_0002, "EncryptedChecksum"),
    (0x0000_0004, "BlobFragments"),
    (0x0000_0100, "ServerSwitch"),
    (0x0000_0200, "LogonServerAddr"),
    (0x0000_0400, "EmptyHeader1"),
    (0x0000_0800, "Referral"),
    (0x0000_1000, "RequestRetransmit"),
    (0x0000_2000, "RejectRetransmit"),
    (0x0000_4000, "AckSequence"),
    (0x0000_8000, "Disconnect"),
    (0x0001_0000, "LoginRequest"),
    (0x0002_0000, "WorldLoginRequest"),
    (0x0004_0000, "ConnectRequest"),
    (0x0008_0000, "ConnectResponse"),
    (0x0010_0000, "NetError"),
    (0x0020_0000, "NetErrorDisconnect"),
    (0x0040_0000, "CICMDCommand"),
    (0x0100_0000, "TimeSync"),
    (0x0200_0000, "EchoRequest"),
    (0x0400_0000, "EchoResponse"),
    (0x0800_0000, "Flow"),
];

// ACE: PacketHeaderFlagsUtil.UnfoldFlags
/// `" | "`-joined names of the set flags. `None` (value 0) never matches (`flags & 0 == 0`), so an
/// empty set gives the empty string, as ACE's `DefaultIfEmpty().Aggregate` yields `null`.
#[must_use]
pub fn unfold_flags(flags: u32) -> String {
    PACKET_HEADER_FLAG_NAMES
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(" | ")
}
