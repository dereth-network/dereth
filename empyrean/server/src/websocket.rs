//! Not ACE: the WebSocket endpoint, one more kind of client endpoint beside the UDP listeners.
//!
//! A browser cannot send UDP, so a browser client sends each datagram as one binary WebSocket
//! message behind a two-byte port (`dereth_transport::web_frame`, specified in
//! `docs/networking/05-websocket-frame.md`). Each connection is one client address to the
//! sessions: its datagrams reach [`ServerNet::on_datagram`](empyrean_net::ServerNet::on_datagram)
//! exactly as a UDP client's do, with the port the frame names mapped onto `P` or `P + 1`, and
//! what the sessions send to that address
//! goes back over the same connection, framed with the port it came from. Everything above the
//! transport (the packets, the checksums and the key stream, the sessions, the world) is the same.
//!
//! The endpoint itself is the game servers' shared one (`dereth_server_websocket::endpoint`); here
//! it is told this server's two ports. One thread accepts, and one thread per connection carries
//! its frames both ways. What a connection received goes into one channel the world thread drains
//! ([`WebSocketEndpoint::try_recv`]), beside the UDP listeners' channel.
//!
//! The rules it keeps:
//!
//! - **TLS by default.** With a certificate and key it serves `wss://` (rustls). Plain `ws://` only
//!   where the settings allow it
//!   ([`WebSocketSettings::validate`](empyrean_common::web_socket_settings::WebSocketSettings::validate)):
//!   on loopback, or behind a reverse proxy that terminates TLS.
//! - **Origins.** A browser page may connect only from an origin the settings list; none listed
//!   refuses every page. A connection that sends no `Origin` is not a browser page, and is
//!   accepted.
//! - **The client's address** is the connection's peer, or, from a trusted proxy, the last address
//!   in its `X-Forwarded-For` header; with the peer's port it is the session's address.
//! - **Limits.** Connections open at once from one client address, and an idle timeout, after
//!   which the connection is closed.
//! - **The will.** When a connection closes, however it closes, its will's datagrams are delivered
//!   as if the client had sent them, so a page that went away logs off at once.
//!
//! A connection's address stays reserved for a while after it closes ([`RETIRED_FOR`]), so what the
//! sessions still send to it is dropped here rather than sent as a UDP datagram to the same
//! address. A UDP client and a WebSocket connection could in principle share an address (the same
//! IP, and a UDP port equal to a TCP port); the WebSocket connection's traffic then wins.

use std::io;
use std::net::SocketAddr;
use std::time::Duration;

pub use dereth_server_websocket::endpoint::{
    client_ip, origin_allowed, tls_config, EndpointConfig, HANDSHAKE_TIMEOUT, POLL, RETIRED_FOR,
};
use empyrean_net::driver::udp::Inbound;
use empyrean_net::{Outgoing, PortKind};

/// The ports one connection numbers: `P` and `P + 1`.
const PORTS: u16 = 2;

const fn index_of(kind: PortKind) -> u16 {
    match kind {
        PortKind::C2S => 0,
        PortKind::S2C => 1,
    }
}

fn inbound(d: dereth_server_websocket::endpoint::Inbound) -> Inbound {
    Inbound {
        port_kind: if d.port == 0 {
            PortKind::C2S
        } else {
            PortKind::S2C
        },
        from: d.from,
        bytes: d.bytes,
    }
}

/// The WebSocket endpoint.
#[derive(Debug)]
pub struct WebSocketEndpoint(dereth_server_websocket::endpoint::Endpoint);

impl WebSocketEndpoint {
    /// Binds `config.listen` and starts accepting.
    ///
    /// # Errors
    /// The bind error.
    pub fn bind(config: EndpointConfig) -> io::Result<Self> {
        dereth_server_websocket::endpoint::Endpoint::bind(config, PORTS).map(Self)
    }

    /// The address it listens on.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.0.local_addr()
    }

    /// Whether it serves `wss://`.
    #[must_use]
    pub const fn tls(&self) -> bool {
        self.0.tls()
    }

    /// A datagram a connection received, if one is waiting.
    #[must_use]
    pub fn try_recv(&self) -> Option<Inbound> {
        self.0.try_recv().map(inbound)
    }

    /// Waits up to `timeout` for a datagram.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Inbound> {
        self.0.recv_timeout(timeout).map(inbound)
    }

    /// Sends `out` if its address is a connection's: `None` when it is not this endpoint's (it is
    /// a UDP client's), `Some(Ok)` when it was queued or dropped for a closed connection, and
    /// `Some(Err)` when the connection's thread has gone.
    #[must_use]
    pub fn transmit(&self, out: &Outgoing) -> Option<io::Result<()>> {
        self.0
            .transmit(out.to, index_of(out.via_port_kind), &out.bytes)
    }

    /// Open connections.
    #[must_use]
    pub fn connections(&self) -> usize {
        self.0.connections()
    }

    /// Stops accepting and waits for the accept thread. Open connections close as their threads
    /// see the stop flag, delivering their wills to a server that no longer reads them.
    pub fn shutdown(self) {
        self.0.shutdown();
    }
}

#[cfg(test)]
mod tests {
    //! Divergence: an Empyrean-only endpoint; ACE speaks UDP only.
    //! Behaviour: none (the endpoint is this server's own; the rules are its settings' contract).
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn only_a_listed_page_origin_or_no_origin_is_admitted() {
        let allowed = vec!["https://play.example.org".to_owned()];
        assert!(origin_allowed(None, &allowed));
        assert!(origin_allowed(Some("https://play.example.org"), &allowed));
        assert!(origin_allowed(Some("HTTPS://Play.Example.org/"), &allowed));
        assert!(!origin_allowed(Some("https://evil.example.org"), &allowed));
        assert!(!origin_allowed(Some("http://play.example.org"), &allowed));
        assert!(!origin_allowed(Some("https://play.example.org"), &[]));
    }

    #[test]
    fn only_a_trusted_proxy_names_the_client() {
        let proxy: IpAddr = "127.0.0.1".parse().unwrap();
        let client: IpAddr = "203.0.113.9".parse().unwrap();
        assert_eq!(
            client_ip(proxy, Some("198.51.100.1, 203.0.113.9"), &[proxy]),
            client
        );
        assert_eq!(client_ip(proxy, Some("not an address"), &[proxy]), proxy);
        assert_eq!(client_ip(proxy, None, &[proxy]), proxy);
        let stranger: IpAddr = "192.0.2.4".parse().unwrap();
        assert_eq!(client_ip(stranger, Some("203.0.113.9"), &[proxy]), stranger);
    }
}
