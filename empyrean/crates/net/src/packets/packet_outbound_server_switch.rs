// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketOutboundServerSwitch.cs
//
// ACE declares this and never sends it.

use dereth_transport::conn::{ServerSwitch, ServerSwitchType};
use dereth_transport::wire::{PacketFlags, ProtoHeader};

use crate::server_packet::ServerPacket;

// ACE: PacketOutboundServerSwitch.PacketOutboundServerSwitch
#[must_use]
pub fn packet_outbound_server_switch() -> ServerPacket {
    let mut packet = ServerPacket::new();
    packet.header = ProtoHeader {
        header: PacketFlags(PacketFlags::ENCRYPTED_CHECKSUM | PacketFlags::SERVER_SWITCH),
        ..ProtoHeader::default()
    };
    // "This value is currently the hard coded Server ID. It can be something different..." ACE
    // writes it where the client reads the switch stamp, then a type of 0 (a world switch).
    let body = ServerSwitch {
        seq_no: 0x18,
        switch_type: ServerSwitchType::WorldSwitch,
    }
    .to_bytes();
    packet.initialize_data_writer().extend_from_slice(&body);
    packet
}
