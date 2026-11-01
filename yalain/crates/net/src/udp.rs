//! The native host's sockets: the four ports of the two services, non-blocking.

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};

use crate::server::{Datagram, Port};

/// The bound sockets.
#[derive(Debug)]
pub struct UdpPorts {
    sockets: [(Port, UdpSocket); 4],
    buf: Vec<u8>,
}

impl UdpPorts {
    /// Bind the login pair at `login_port` and the world pair at `world_port` (each an even
    /// port and the one above it) on `ip`.
    ///
    /// # Errors
    /// A port cannot be bound.
    pub fn bind(ip: IpAddr, login_port: u16, world_port: u16) -> io::Result<Self> {
        let bind = |port: u16| -> io::Result<UdpSocket> {
            let s = UdpSocket::bind(SocketAddr::new(ip, port))?;
            s.set_nonblocking(true)?;
            Ok(s)
        };
        Ok(Self {
            sockets: [
                (Port::LOGIN_EVEN, bind(login_port)?),
                (Port::LOGIN_ODD, bind(login_port + 1)?),
                (Port::WORLD_EVEN, bind(world_port)?),
                (Port::WORLD_ODD, bind(world_port + 1)?),
            ],
            buf: vec![0; 65536],
        })
    }

    /// Hand every datagram waiting on any socket to `each`. Returns how many there were.
    pub fn receive(&mut self, mut each: impl FnMut(Port, SocketAddr, &[u8])) -> usize {
        let mut n = 0;
        for (port, sock) in &self.sockets {
            loop {
                match sock.recv_from(&mut self.buf) {
                    Ok((len, from)) => {
                        n += 1;
                        each(*port, from, &self.buf[..len]);
                    }
                    // A send to a client that has gone away comes back as a reset on Windows; it
                    // says nothing about this socket.
                    Err(e) if e.kind() == io::ErrorKind::ConnectionReset => {}
                    Err(_) => break,
                }
            }
        }
        n
    }

    /// Send datagrams from their sockets.
    pub fn send(&self, datagrams: &[Datagram]) {
        for d in datagrams {
            if let Some((_, s)) = self.sockets.iter().find(|(p, _)| *p == d.from) {
                let _ = s.send_to(&d.bytes, d.to);
            }
        }
    }
}
