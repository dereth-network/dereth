//! The WebSocket frame: one datagram in one binary WebSocket message.
//!
//! A browser cannot send UDP, so a browser client carries each datagram as one binary WebSocket
//! message, either to a server that accepts WebSocket connections itself or to a relay on the
//! player's machine that forwards it over UDP. This is not retail's: retail speaks UDP only. It is
//! specified in `docs/networking/05-websocket-frame.md`.
//!
//! Every message starts with a big-endian `u16` port, then the datagram:
//!
//! - **The port names one of the server's two ports.** A session talks to the logon port and to
//!   the one above it (the connect response goes there). The first datagram of a connection is the
//!   login, so its port is the logon port as the client addresses it; after that, that port and the
//!   one above it are the two ports ([`PortPair`]). A datagram coming back carries the port it came
//!   from, in the client's numbering. Whoever carries the frame maps the pair onto the server's
//!   real ports, so a page needs no server port of its own.
//! - **Port 0 is the will**: not a datagram but the datagrams to deliver for the session when its
//!   WebSocket closes, however it closes, each as its length and its port (both big-endian `u16`)
//!   and its bytes. A later will replaces an earlier one and an empty one clears it. A page that is
//!   closed never gets to log off; its will, handed over ahead of time, logs it off in its place.

/// The bytes in front of each datagram.
pub const PORT_PREFIX: usize = 2;

/// The port a message is addressed to when it is the session's will. No server listens on port 0.
pub const WILL_PORT: u16 = 0;

/// A datagram for `port`, framed.
#[must_use]
pub fn encode(port: u16, datagram: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(PORT_PREFIX + datagram.len());
    out.extend_from_slice(&port.to_be_bytes());
    out.extend_from_slice(datagram);
    out
}

/// The port and datagram of one framed message, or `None` when it is too short to carry a port.
#[must_use]
pub fn decode(message: &[u8]) -> Option<(u16, &[u8])> {
    let (port, datagram) = message.split_first_chunk::<PORT_PREFIX>()?;
    Some((u16::from_be_bytes(*port), datagram))
}

/// A will: the datagrams to deliver when the session's WebSocket closes, each with its port. A
/// datagram longer than a `u16` can count is cut to that length.
#[must_use]
pub fn encode_will(datagrams: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut out = WILL_PORT.to_be_bytes().to_vec();
    for (port, datagram) in datagrams {
        let len = u16::try_from(datagram.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&port.to_be_bytes());
        out.extend_from_slice(&datagram[..usize::from(len)]);
    }
    out
}

/// The datagrams of a will, each with its port; `None` for a message that is not a will. A
/// trailing entry cut short is dropped.
#[must_use]
pub fn decode_will(message: &[u8]) -> Option<Vec<(u16, Vec<u8>)>> {
    let (port, mut rest) = decode(message)?;
    if port != WILL_PORT {
        return None;
    }
    let mut out = Vec::new();
    while let Some((len, after)) = rest.split_first_chunk::<2>() {
        let len = usize::from(u16::from_be_bytes(*len));
        let Some((port, after)) = after.split_first_chunk::<PORT_PREFIX>() else {
            break;
        };
        if after.len() < len {
            break;
        }
        let (datagram, next) = after.split_at(len);
        out.push((u16::from_be_bytes(*port), datagram.to_vec()));
        rest = next;
    }
    Some(out)
}

/// Which of the server's two ports a frame names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPort {
    /// The logon port, where a session's datagrams go.
    Logon,
    /// The port above it, where the connect response goes.
    Next,
}

/// The two ports one connection addresses, in the client's numbering: fixed by its first
/// datagram, whose port is the logon port.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PortPair {
    logon: Option<u16>,
}

impl PortPair {
    /// A connection that has sent nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { logon: None }
    }

    /// Which server port a datagram addressed to `port` is for; `None` for a port outside the pair,
    /// or for the will's port. The first datagram's port becomes the logon port.
    pub fn side(&mut self, port: u16) -> Option<ServerPort> {
        if port == WILL_PORT {
            return None;
        }
        let logon = *self.logon.get_or_insert(port);
        if port == logon {
            Some(ServerPort::Logon)
        } else if Some(port) == logon.checked_add(1) {
            Some(ServerPort::Next)
        } else {
            None
        }
    }

    /// The port, in the client's numbering, a datagram from `side` is framed with; `None` before
    /// the connection's first datagram.
    #[must_use]
    pub fn port(&self, side: ServerPort) -> Option<u16> {
        let logon = self.logon?;
        match side {
            ServerPort::Logon => Some(logon),
            ServerPort::Next => logon.checked_add(1),
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the frame is this project's own format, specified beside it; these tests
    //! pin its bytes and the port pair's rule).
    use super::*;

    #[test]
    fn a_datagram_round_trips_with_its_port() {
        let framed = encode(9001, &[1, 2, 3]);
        assert_eq!(framed, [0x23, 0x29, 1, 2, 3]);
        assert_eq!(decode(&framed), Some((9001, &[1u8, 2, 3][..])));
        assert_eq!(decode(&[0x23]), None);
        assert_eq!(decode(&encode(9000, &[])), Some((9000, &[][..])));
    }

    #[test]
    fn a_will_round_trips_and_is_told_apart_from_a_datagram() {
        let datagrams = vec![(9000, vec![1, 2]), (9001, vec![])];
        let message = encode_will(&datagrams);
        assert_eq!(message, [0, 0, 0, 2, 0x23, 0x28, 1, 2, 0, 0, 0x23, 0x29]);
        assert_eq!(decode_will(&message), Some(datagrams));
        assert_eq!(decode_will(&encode(9000, &[1])), None);
        assert_eq!(decode_will(&encode_will(&[])), Some(Vec::new()));
        // A last entry that claims more bytes than follow is dropped, and the rest kept.
        let mut cut = encode_will(&[(9000, vec![7])]);
        cut.extend_from_slice(&[0, 9, 0x23, 0x28, 1]);
        assert_eq!(decode_will(&cut), Some(vec![(9000, vec![7])]));
    }

    #[test]
    fn the_first_datagram_names_the_logon_port_and_the_one_above_it_is_the_other() {
        let mut pair = PortPair::new();
        assert_eq!(pair.port(ServerPort::Logon), None);
        assert_eq!(pair.side(WILL_PORT), None);
        assert_eq!(pair.side(9050), Some(ServerPort::Logon));
        assert_eq!(pair.side(9051), Some(ServerPort::Next));
        assert_eq!(pair.side(9050), Some(ServerPort::Logon));
        assert_eq!(pair.side(9049), None);
        assert_eq!(pair.side(9052), None);
        assert_eq!(pair.port(ServerPort::Logon), Some(9050));
        assert_eq!(pair.port(ServerPort::Next), Some(9051));
    }
}
