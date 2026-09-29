// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketRejectRetransmit.cs

use dereth_transport::wire::{PacketFlags, ProtoHeader};

use crate::server_packet::ServerPacket;

// ACE: PacketRejectRetransmit.PacketRejectRetransmit
/// Tells the client these sequences are no longer cached and will never be resent.
#[must_use]
pub fn packet_reject_retransmit(sequences: &[u32]) -> ServerPacket {
    let mut packet = ServerPacket::new();
    packet.header = ProtoHeader {
        header: PacketFlags(PacketFlags::REJECT_RETRANSMIT),
        ..ProtoHeader::default()
    };
    let w = packet.initialize_data_writer();
    // `DataWriter.Write(sequences.Count)`: an `int`.
    w.extend_from_slice(
        &i32::try_from(sequences.len())
            .unwrap_or(i32::MAX)
            .to_le_bytes(),
    );
    for sequence in sequences {
        w.extend_from_slice(&sequence.to_le_bytes());
    }
    packet
}
