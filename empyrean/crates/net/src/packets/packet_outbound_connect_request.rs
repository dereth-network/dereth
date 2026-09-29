// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketoutboundConnectRequest.cs

use dereth_transport::conn::{ConnectRequest, ConnectSeeds};
use dereth_transport::wire::{PacketFlags, ProtoHeader};

use crate::server_packet::ServerPacket;

// ACE: PacketOutboundConnectRequest.PacketOutboundConnectRequest
/// The `ConnectRequest`: server time, cookie, the client's id, then the server-to-client seed and
/// the client-to-server seed (the order the client reads its connect header in), then four bytes
/// of padding.
#[must_use]
pub fn packet_outbound_connect_request(
    server_time: f64,
    cookie: u64,
    client_id: u32,
    isaac_server_seed: u32,
    isaac_client_seed: u32,
) -> ServerPacket {
    let mut packet = ServerPacket::new();
    packet.header = ProtoHeader {
        header: PacketFlags(PacketFlags::CONNECT_REQUEST),
        ..ProtoHeader::default()
    };
    let body = ConnectRequest::new(
        server_time,
        cookie,
        client_id,
        ConnectSeeds {
            server_to_client: isaac_server_seed,
            client_to_server: isaac_client_seed,
        },
    )
    .to_bytes();
    packet.initialize_data_writer().extend_from_slice(&body);
    packet
}
