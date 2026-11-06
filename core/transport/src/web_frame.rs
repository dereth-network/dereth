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
//!   real ports, so a page needs no server port of its own. A server with more ports numbers them
//!   on upward the same way ([`PortRun`]).
//! - **Port 0 is the will**: not a datagram but the datagrams to deliver for the session when its
//!   WebSocket closes, however it closes, each as its length and its port (both big-endian `u16`)
//!   and its bytes. A later will replaces an earlier one and an empty one clears it. A page that is
//!   closed never gets to log off; its will, handed over ahead of time, logs it off in its place.

/// The bytes in front of each datagram.
pub const PORT_PREFIX: usize = 2;

/// The port a message is addressed to when it is the session's will. No server listens on port 0.
pub const WILL_PORT: u16 = 0;

/// How far above a connection's first port a world numbers its friends service's stream: the
/// stream's bytes in order, not a datagram, and an empty message for its end. Past the game's four
/// ports, so a server without the stream drops it as any other port it does not number.
pub const FRIENDS_STREAM: u16 = 4;

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

/// The ports one connection addresses, in the client's numbering: a run of them from the port of
/// its first datagram, which is the logon port. Each is known by its index in the run, 0 being the
/// logon port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortRun {
    first: Option<u16>,
    count: u16,
}

impl PortRun {
    /// A connection that has sent nothing yet, to a server of `count` ports.
    #[must_use]
    pub const fn new(count: u16) -> Self {
        Self { first: None, count }
    }

    /// The index of the port a datagram addressed to `port` is for; `None` for a port outside the
    /// run, or for the will's port. The first datagram's port becomes the logon port.
    pub fn index(&mut self, port: u16) -> Option<u16> {
        if port == WILL_PORT {
            return None;
        }
        let first = *self.first.get_or_insert(port);
        port.checked_sub(first).filter(|i| *i < self.count)
    }

    /// The port, in the client's numbering, a datagram from the port of `index` is framed with;
    /// `None` before the connection's first datagram, or outside the run.
    #[must_use]
    pub fn port(&self, index: u16) -> Option<u16> {
        let first = self.first?;
        if index < self.count {
            first.checked_add(index)
        } else {
            None
        }
    }
}

/// The two ports one connection addresses, in the client's numbering: fixed by its first
/// datagram, whose port is the logon port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortPair(PortRun);

impl Default for PortPair {
    fn default() -> Self {
        Self::new()
    }
}

impl PortPair {
    /// A connection that has sent nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self(PortRun::new(2))
    }

    /// Which server port a datagram addressed to `port` is for; `None` for a port outside the pair,
    /// or for the will's port. The first datagram's port becomes the logon port.
    pub fn side(&mut self, port: u16) -> Option<ServerPort> {
        match self.0.index(port)? {
            0 => Some(ServerPort::Logon),
            _ => Some(ServerPort::Next),
        }
    }

    /// The port, in the client's numbering, a datagram from `side` is framed with; `None` before
    /// the connection's first datagram.
    #[must_use]
    pub fn port(&self, side: ServerPort) -> Option<u16> {
        self.0.port(match side {
            ServerPort::Logon => 0,
            ServerPort::Next => 1,
        })
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

    #[test]
    fn a_run_of_four_ports_counts_up_from_the_first_datagrams_port() {
        let mut run = PortRun::new(4);
        assert_eq!(run.port(0), None);
        assert_eq!(run.index(WILL_PORT), None);
        assert_eq!(run.index(9050), Some(0));
        assert_eq!(run.index(9053), Some(3));
        assert_eq!(run.index(9049), None);
        assert_eq!(run.index(9054), None);
        assert_eq!(run.port(2), Some(9052));
        assert_eq!(run.port(4), None);
        let mut top = PortRun::new(4);
        assert_eq!(top.index(u16::MAX), Some(0));
        assert_eq!(top.port(1), None, "no port above the last");
    }
}
