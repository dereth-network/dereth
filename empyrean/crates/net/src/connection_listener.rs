// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ConnectionListener.cs
//
// The receive half of a listener, without its socket: the socket is the driver's
// (`crate::driver::udp`), which calls [`ServerNet::on_datagram`] for every datagram it reads.

use std::net::SocketAddr;

use empyrean_common::clock::ClockSnapshot;

use crate::client_packet::{ClientPacket, MAX_PACKET_SIZE};
use crate::{PortKind, ServerNet};

impl ServerNet {
    // ACE: ConnectionListener.OnDataReceive
    /// One datagram from `from`, read by the listener on `local_port_kind`.
    ///
    /// A datagram larger than ACE's 1024-byte receive buffer is dropped, as ACE's
    /// `EndReceiveFrom` fails it with `SocketError.MessageSize` on Windows. A datagram that does not
    /// unpack is dropped silently.
    ///
    /// DIVERGE: a status-ping datagram on `P` is answered (or dropped) by [`ServerNet::status_ping`]
    /// before it reaches the packet reader, and never touches a session (V442). With the ping off
    /// it is dropped, as ACE's packet reader drops it for its impossible size.
    pub fn on_datagram(
        &mut self,
        local_port_kind: PortKind,
        from: SocketAddr,
        bytes: &[u8],
        now: ClockSnapshot,
    ) {
        if bytes.len() > MAX_PACKET_SIZE {
            log::debug!("ConnectionListener: MessageSize from client {from}");
            return;
        }
        if dereth_transport::status_ping::is_status_ping(bytes) {
            if local_port_kind == PortKind::C2S {
                self.answer_status_ping(from, bytes, now);
            }
            return;
        }
        if let Some(packet) = ClientPacket::unpack(bytes) {
            self.process_packet(local_port_kind, packet, from, now);
        }
    }
}
