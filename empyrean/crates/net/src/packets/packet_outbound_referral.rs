// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketOutboundReferral.cs
//
// ACE declares this and never sends it: one ACE process is login and world server both.

use std::net::{Ipv4Addr, SocketAddrV4};

use dereth_transport::conn::Referral;
use dereth_transport::wire::{PacketFlags, ProtoHeader};

use crate::server_packet::ServerPacket;

// ACE: PacketOutboundReferral.PacketOutboundReferral
/// `session_ip_address` is the client's dotted quad split on `.`; a private (RFC 1918) client
/// is referred to `internal_host` when `send_internal_host_on_local_network` is set.
///
/// ACE's `Convert.ToInt16` throws on a non-numeric second octet; here such an octet simply fails
/// the 172.16/12 test.
#[must_use]
pub fn packet_outbound_referral(
    world_connection_key: u64,
    session_ip_address: &[&str],
    host: [u8; 4],
    port: u16,
    send_internal_host_on_local_network: bool,
    internal_host: [u8; 4],
) -> ServerPacket {
    let mut packet = ServerPacket::new();
    packet.header = ProtoHeader {
        header: PacketFlags(PacketFlags::ENCRYPTED_CHECKSUM | PacketFlags::REFERRAL),
        ..ProtoHeader::default()
    };
    let octet = |i: usize| session_ip_address.get(i).copied().unwrap_or("");
    let second = octet(1).parse::<i16>().ok();
    let local = send_internal_host_on_local_network
        && (octet(0) == "10"
            || (octet(0) == "172" && second.is_some_and(|s| (16..=31).contains(&s)))
            || (octet(0) == "192" && octet(1) == "168"));
    // The key, `AF_INET` (2), the port big-endian, the host, eight zero bytes, the server id and
    // six bytes of padding: the shared layout.
    let body = Referral {
        cookie: world_connection_key,
        addr: SocketAddrV4::new(
            Ipv4Addr::from(if local { internal_host } else { host }),
            port,
        ),
        // "This value is currently the hard coded Server ID. It can be something different..."
        id_server: 0x18,
        family: 2,
    }
    .to_bytes();
    packet.initialize_data_writer().extend_from_slice(&body);
    packet
}
