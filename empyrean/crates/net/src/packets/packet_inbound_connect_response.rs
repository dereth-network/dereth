// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketInboundConnectResponse.cs

use crate::client_packet::ClientPacket;

/// ACE `PacketInboundConnectResponse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketInboundConnectResponse {
    /// The cookie the server sent in its `ConnectRequest`.
    pub check: u64,
}

impl PacketInboundConnectResponse {
    // ACE: PacketInboundConnectResponse.PacketInboundConnectResponse
    /// `None` when the packet carries no `ConnectResponse` section, where ACE's `ReadUInt64` would
    /// read whatever the payload starts with; the caller only builds this for a packet flagged
    /// `ConnectResponse`, which the parser guarantees carries its 8 bytes.
    #[must_use]
    pub fn new(packet: &ClientPacket) -> Option<Self> {
        Some(Self {
            check: packet.header_optional.connect_response?,
        })
    }
}
