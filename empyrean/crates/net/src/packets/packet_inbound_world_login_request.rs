// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketInboundWorldLoginRequest.cs
//
// ACE declares this and never constructs it: one ACE process is login and world server both.

use crate::client_packet::ClientPacket;

/// ACE `PacketInboundWorldLoginRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PacketInboundWorldLoginRequest {
    pub connection_key: u64,
}

impl PacketInboundWorldLoginRequest {
    // ACE: PacketInboundWorldLoginRequest.PacketInboundWorldLoginRequest
    #[must_use]
    pub fn new(packet: &ClientPacket) -> Option<Self> {
        Some(Self {
            connection_key: packet.header_optional.world_login_request?,
        })
    }
}
