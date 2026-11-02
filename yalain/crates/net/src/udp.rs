//! The native host's sockets: the four ports of the two services, non-blocking.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

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

/// This machine's own address on the way to `client`, as its routes choose it: loopback for a
/// client on loopback, its address on a network for a client on that network. Nothing is sent.
#[must_use]
pub fn address_toward(client: Ipv4Addr) -> Option<Ipv4Addr> {
    let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    // Any port: connecting a datagram socket only chooses the route.
    s.connect((client, 9)).ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(own) if !own.is_unspecified() => Some(own),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the host's route lookup, on this machine's own sockets)
    use super::*;

    #[test]
    fn the_address_toward_a_client_on_loopback_is_loopback() {
        assert_eq!(
            address_toward(Ipv4Addr::LOCALHOST),
            Some(Ipv4Addr::LOCALHOST)
        );
    }
}
