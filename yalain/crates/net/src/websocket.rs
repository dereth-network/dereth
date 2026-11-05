//! The world's WebSocket endpoint (`[server.websocket]`), beside its UDP sockets: the sockets the
//! world talks over, as one [`Wire`].
//!
//! A browser cannot send UDP, so a browser client sends each datagram as one WebSocket message
//! behind a port number (`docs/networking/05-websocket-frame.md`). The endpoint and its settings
//! are the game servers' shared ones (`dereth_server_websocket`), Empyrean's too; what is this
//! server's is what its ports are.
//!
//! **One WebSocket reaches both services.** The world has four ports where Empyrean has two. One
//! connection numbers all four, in a row, from the port of its first datagram, which is the login:
//! `P` and `P + 1` are the login service's pair, `P + 2` and `P + 3` the world service's. A client
//! told to go to the world is told `P + 2` ([`crate::ServerNet::refer_carried`]), whatever
//! port the world service really has, and goes on sending over the same connection.
//!
//! So the page needs one URL and opens one connection, as a page does for Empyrean; the limit on
//! connections from one address counts a player once; the connection's will covers both of its
//! sessions; and a world login comes from its login's address, as it must, because the two share
//! their connection. Its address is the connection's, the client's own behind a trusted proxy.
//!
//! **The friends service rides it too.** `P + 4` ([`FRIENDS`]) carries the friends stream's bytes,
//! for a browser, which can open no TCP connection of its own. [`Wire`] keeps them apart from the
//! datagrams for the friends service's host, which sees the connection as one more stream.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::server::{Datagram, Port, Service};
use crate::udp::UdpPorts;
use dereth_server_websocket::endpoint::{Endpoint, EndpointConfig, Numbering};
use dereth_server_websocket::settings::WebSocketSettings;

/// The ports one connection numbers.
pub const PORTS: u16 = 5;

/// The index of the friends stream in a connection's numbering.
pub const FRIENDS: u16 = dereth_transport::web_frame::FRIENDS_STREAM;

/// The index of the world service's even port in a connection's numbering.
pub const WORLD: u16 = 2;

/// A port's index in a connection's numbering.
#[must_use]
pub const fn index_of(port: Port) -> u16 {
    let service = match port.service {
        Service::Login => 0,
        Service::World => WORLD,
    };
    service + if port.odd { 1 } else { 0 }
}

/// The port of an index in a connection's numbering.
#[must_use]
pub fn port_of(index: u16) -> Option<Port> {
    Port::ALL.into_iter().find(|p| index_of(*p) == index)
}

/// The endpoint `settings` asks for, listening: none when it is off.
///
/// # Errors
/// A setting that cannot be served, a certificate or key that cannot be read, or an address that
/// cannot be bound. Each stops the start, as Empyrean's do.
pub fn start(settings: &WebSocketSettings) -> Result<Option<Endpoint>, String> {
    if !settings.enabled {
        return Ok(None);
    }
    let config = EndpointConfig::from_settings(settings, |p| PathBuf::from(p))
        .map_err(|e| format!("WebSocket: {e}"))?;
    let listen = config.listen;
    let ws = Endpoint::bind(config, PORTS).map_err(|e| format!("WebSocket {listen}: {e}"))?;
    tracing::info!(
        "WebSocket endpoint on {} ({}), reached at {}",
        ws.local_addr(),
        if ws.tls() {
            "wss://"
        } else if settings.behind_tls_proxy {
            "ws://, behind a TLS proxy"
        } else {
            "ws://, loopback"
        },
        settings.url(ws.local_addr(), ws.tls())
    );
    Ok(Some(ws))
}

/// How a connection numbers the world service, for the login's referrals: the port a client at
/// `address` is told, when `address` is a connection's.
#[must_use]
pub fn world_port(numbering: &Numbering, address: SocketAddr) -> Option<u16> {
    numbering.port(address, WORLD)
}

/// The sockets the world talks over: its UDP ports, and the endpoint when it has one.
#[derive(Debug)]
pub struct Wire {
    pub ports: UdpPorts,
    pub websocket: Option<Endpoint>,
    /// What connections sent on the friends stream since it was last taken.
    pub friends: Vec<(SocketAddr, Vec<u8>)>,
}

impl Wire {
    /// Hand every datagram waiting, on any socket or connection, to `each`. Returns how many there
    /// were.
    pub fn receive(&mut self, mut each: impl FnMut(Port, SocketAddr, &[u8])) -> usize {
        let mut n = self.ports.receive(&mut each);
        if let Some(ws) = &self.websocket {
            while let Some(d) = ws.try_recv() {
                if d.port == FRIENDS {
                    n += 1;
                    self.friends.push((d.from, d.bytes));
                } else if let Some(port) = port_of(d.port) {
                    n += 1;
                    each(port, d.from, &d.bytes);
                }
            }
        }
        n
    }

    /// Send datagrams: each to the connection whose address it names, else from its UDP socket.
    pub fn send(&self, datagrams: &[Datagram]) {
        for d in datagrams {
            let carried = self
                .websocket
                .as_ref()
                .and_then(|ws| ws.transmit(d.to, index_of(d.from), &d.bytes));
            match carried {
                None => self.ports.send(std::slice::from_ref(d)),
                Some(sent) => log_failed(sent, d.to),
            }
        }
    }

    /// What connections sent on the friends stream since the last call, an empty message for a
    /// stream the client ended.
    pub fn take_friends(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        std::mem::take(&mut self.friends)
    }

    /// Send on the friends stream of the connection at `to`, an empty message to end it.
    pub fn send_friends(&self, to: SocketAddr, bytes: &[u8]) {
        if let Some(ws) = &self.websocket {
            if let Some(sent) = ws.transmit(to, FRIENDS, bytes) {
                log_failed(sent, to);
            }
        }
    }

    /// Whether the connection at `address` is still open.
    #[must_use]
    pub fn is_open(&self, address: SocketAddr) -> bool {
        self.websocket
            .as_ref()
            .is_some_and(|ws| ws.numbering().port(address, 0).is_some())
    }

    /// Stop listening for connections; those open close.
    pub fn stop(self) {
        if let Some(ws) = self.websocket {
            ws.shutdown();
        }
    }
}

/// A connection whose thread has gone: its sessions end by their own silence.
fn log_failed(sent: io::Result<()>, to: SocketAddr) {
    if let Err(e) = sent {
        tracing::debug!("WebSocket {to}: {e}");
    }
}
