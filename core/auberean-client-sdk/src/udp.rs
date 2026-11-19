//! A native host's port: a UDP socket that never blocks.

use std::io;
use std::net::{SocketAddr, UdpSocket};

use auberean_client_net::LocalTime;

use crate::driver::Port;

/// The largest datagram read.
const LARGEST: usize = 65_536;

/// A UDP socket as the client's port: what arrived is read until none waits, and a datagram the
/// socket refuses to send is dropped (the session sends it again).
#[derive(Debug)]
pub struct UdpPort {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl UdpPort {
    /// A socket bound to `at`.
    ///
    /// # Errors
    /// The socket could not be bound, or not be made to never block.
    pub fn bind(at: SocketAddr) -> io::Result<Self> {
        Self::new(UdpSocket::bind(at)?)
    }

    /// A socket the host bound itself.
    ///
    /// # Errors
    /// The socket could not be made to never block.
    pub fn new(socket: UdpSocket) -> io::Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            socket,
            buf: vec![0; LARGEST],
        })
    }

    /// Where the socket is bound.
    ///
    /// # Errors
    /// The socket could not say.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

impl Port for UdpPort {
    fn receive(&mut self, _now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        let (n, from) = self.socket.recv_from(&mut self.buf).ok()?;
        Some((from, self.buf[..n].to_vec()))
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], _now: LocalTime) {
        let _ = self.socket.send_to(bytes, to);
    }
}
