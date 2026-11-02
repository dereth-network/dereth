//! One browser session: its WebSocket carried to and from its own UDP socket, until it closes.
//!
//! A message's port names one of the server's two ports in the client's own numbering, fixed by
//! the session's first datagram (its logon port, and the one above it); the relay maps them onto
//! the server's real two, and frames a datagram coming back with the port it came from in the same
//! numbering. Anything to another port, or from another address, is dropped. A message to port 0
//! is the session's **will**: the datagrams to send when its WebSocket closes, however it closes,
//! so a page that is closed still logs off. The frame is specified in
//! `docs/networking/05-websocket-frame.md`.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use dereth_transport::conn::LoginRequest;
use dereth_transport::web_frame::{self, PortPair, ServerPort};
use dereth_transport::{PacketFlags, ParsedPacket};
use tungstenite::{Message, WebSocket};

use crate::limits::{Bucket, Clock, SendRate};

/// The client's receive buffer: no datagram is longer.
pub const MAX_DATAGRAM: usize = 65_504;

/// How long a read of the WebSocket waits before the relay looks at the UDP socket again.
pub const POLL: Duration = Duration::from_millis(2);

/// The server's real port for one side of the pair.
fn real_port(server: SocketAddr, side: ServerPort) -> Option<SocketAddr> {
    let port = match side {
        ServerPort::Logon => server.port(),
        ServerPort::Next => server.port().checked_add(1)?,
    };
    Some(SocketAddr::new(server.ip(), port))
}

/// Where an outgoing message's datagram goes, if the relay may send it there: the server's logon
/// port or the one above it, as the message's port names them in the session's numbering.
#[must_use]
pub fn destination<'a>(
    server: SocketAddr,
    pair: &mut PortPair,
    message: &'a [u8],
) -> Option<(SocketAddr, &'a [u8])> {
    let (port, datagram) = web_frame::decode(message)?;
    let to = real_port(server, pair.side(port)?)?;
    Some((to, datagram))
}

/// The datagrams of a will, each to the server port it names; `None` for a message that is not a
/// will. A datagram to a port outside the pair is left out.
#[must_use]
pub fn will(
    server: SocketAddr,
    pair: &mut PortPair,
    message: &[u8],
) -> Option<Vec<(SocketAddr, Vec<u8>)>> {
    let datagrams = web_frame::decode_will(message)?;
    Some(
        datagrams
            .into_iter()
            .filter_map(|(port, d)| Some((real_port(server, pair.side(port)?)?, d)))
            .collect(),
    )
}

/// The frame for a datagram that came from `from`, in the session's numbering; `None` for one
/// from any other address or port.
#[must_use]
pub fn reply(
    server: SocketAddr,
    pair: &PortPair,
    from: SocketAddr,
    datagram: &[u8],
) -> Option<Vec<u8>> {
    if from.ip() != server.ip() {
        return None;
    }
    let side = if from.port() == server.port() {
        ServerPort::Logon
    } else if Some(from.port()) == server.port().checked_add(1) {
        ServerPort::Next
    } else {
        return None;
    };
    Some(web_frame::encode(pair.port(side)?, datagram))
}

/// The account a login request names, made safe to print; `None` for any other datagram. The
/// password beside it is never read out.
#[must_use]
pub fn login_account(datagram: &[u8]) -> Option<String> {
    let packet = ParsedPacket::parse_from_client(datagram).ok()?;
    let body = packet.optional.get(&PacketFlags::LOGIN_REQUEST)?;
    let login = LoginRequest::parse(body)?;
    Some(
        login
            .auth
            .account
            .iter()
            .take(64)
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    char::from(b)
                } else {
                    '?'
                }
            })
            .collect(),
    )
}

/// Whether a WebSocket error is only a read that timed out.
#[must_use]
pub fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
}

/// Why a session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The page closed it, or the connection dropped.
    Closed,
    /// The page sent nothing for the idle limit.
    Idle,
}

/// What a session did, for its closing log line.
#[derive(Debug, Default)]
pub struct Counts {
    pub out: u64,
    pub back: u64,
    /// Datagrams dropped for going over the session's rate.
    pub over_rate: u64,
    /// The account its login named, once seen.
    pub account: Option<String>,
}

/// What a session needs besides its sockets.
pub struct Carry<'a> {
    pub server: SocketAddr,
    pub clock: &'a Clock,
    pub idle: Option<Duration>,
    pub send: Option<SendRate>,
    /// Called once with the account the session's login names.
    pub on_login: &'a dyn Fn(&str),
}

impl std::fmt::Debug for Carry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Carry")
            .field("server", &self.server)
            .field("idle", &self.idle)
            .field("send", &self.send)
            .finish_non_exhaustive()
    }
}

/// Carries datagrams both ways until the WebSocket closes or goes idle, keeping the session's
/// latest will in `last_will`.
///
/// # Errors
/// The connection failed.
pub fn carry<S: Read + Write>(
    ws: &mut WebSocket<S>,
    udp: &UdpSocket,
    c: &Carry<'_>,
    counts: &mut Counts,
    last_will: &mut Vec<(SocketAddr, Vec<u8>)>,
) -> Result<Ended, String> {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut pair = PortPair::new();
    let now = || (c.clock)();
    let mut last_heard = now();
    let mut buckets = c
        .send
        .map(|r| (Bucket::new(r.datagrams, now()), Bucket::new(r.bytes, now())));
    loop {
        match ws.read() {
            Ok(Message::Binary(message)) => {
                last_heard = now();
                if let Some(w) = will(c.server, &mut pair, &message) {
                    *last_will = w;
                } else if let Some((to, datagram)) = destination(c.server, &mut pair, &message) {
                    let allowed = buckets.as_mut().is_none_or(|(datagrams, bytes)| {
                        let t = now();
                        let len = u32::try_from(datagram.len()).unwrap_or(u32::MAX);
                        datagrams.take(1, t) && bytes.take(len, t)
                    });
                    if allowed {
                        if counts.account.is_none() && counts.out < 32 {
                            if let Some(account) = login_account(datagram) {
                                (c.on_login)(&account);
                                counts.account = Some(account);
                            }
                        }
                        let _ = udp.send_to(datagram, to);
                        counts.out += 1;
                    } else {
                        counts.over_rate += 1;
                    }
                }
            }
            Ok(Message::Close(_)) => return Ok(Ended::Closed),
            Ok(_) => last_heard = now(),
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(Ended::Closed)
            }
            Err(e) => return Err(e.to_string()),
        }
        if c.idle
            .is_some_and(|idle| now().saturating_sub(last_heard) > idle)
        {
            let _ = ws.close(None);
            let _ = ws.flush();
            return Ok(Ended::Idle);
        }
        loop {
            match udp.recv_from(&mut buf) {
                Ok((len, from)) => {
                    if let Some(framed) = reply(c.server, &pair, from, &buf[..len]) {
                        counts.back += 1;
                        // A write that would block has queued the message; one past the write
                        // buffer's limit (a page that stopped reading) ends the session.
                        match ws.write(Message::Binary(framed.into())) {
                            Ok(()) => {}
                            Err(e) if would_block(&e) => {}
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // An ICMP port-unreachable for an earlier send; the client ignores it too.
                Err(e) if e.kind() == ErrorKind::ConnectionReset => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(Ended::Closed)
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The session's first datagram fixes its numbering of the server's two ports; the relay sends
    /// to the server's real logon port and the one above it, and nowhere else.
    #[test]
    fn it_sends_only_to_the_servers_two_ports() {
        let server: SocketAddr = "10.0.0.5:9050".parse().expect("an address");
        let mut pair = PortPair::new();
        let framed = web_frame::encode(9000, &[7]);
        let (to, d) = destination(server, &mut pair, &framed).expect("the logon port");
        assert_eq!(
            (to, d),
            ("10.0.0.5:9050".parse().expect("an address"), &[7u8][..])
        );
        assert_eq!(
            destination(server, &mut pair, &web_frame::encode(9001, &[])).map(|(to, _)| to),
            Some("10.0.0.5:9051".parse().expect("an address"))
        );
        assert!(destination(server, &mut pair, &web_frame::encode(9002, &[1])).is_none());
        assert!(destination(server, &mut pair, &web_frame::encode(22, &[1])).is_none());
        assert!(destination(server, &mut pair, &[0x23]).is_none());
        assert_eq!(
            reply(
                server,
                &pair,
                "10.0.0.5:9051".parse().expect("an address"),
                &[3]
            ),
            Some(web_frame::encode(9001, &[3]))
        );
        assert_eq!(
            reply(
                server,
                &pair,
                "10.0.0.6:9050".parse().expect("an address"),
                &[3]
            ),
            None,
            "another host"
        );
        assert_eq!(
            reply(
                server,
                &pair,
                "10.0.0.5:9052".parse().expect("an address"),
                &[3]
            ),
            None,
            "another port"
        );
    }

    /// A will names its datagrams by length and port, keeps only those to the server's two
    /// ports, and is told apart from a datagram by its port 0.
    #[test]
    fn a_will_is_read_and_kept_to_the_servers_ports() {
        let server: SocketAddr = "10.0.0.5:9000".parse().expect("an address");
        let mut pair = PortPair::new();
        let message = web_frame::encode_will(&[(9000, vec![1, 2]), (22, vec![3]), (9001, vec![])]);
        assert_eq!(
            will(server, &mut pair, &message),
            Some(vec![
                ("10.0.0.5:9000".parse().expect("an address"), vec![1, 2]),
                ("10.0.0.5:9001".parse().expect("an address"), vec![]),
            ])
        );
        assert_eq!(
            will(server, &mut pair, &web_frame::encode(9000, &[1])),
            None
        );
        assert!(
            destination(server, &mut pair, &message).is_none(),
            "port 0 is never a destination"
        );
    }

    /// The account of a login request is read out, and nothing is read from any other datagram.
    #[test]
    fn a_login_names_its_account_and_nothing_else_does() {
        assert_eq!(
            login_account(&crate::tests::login_datagram("Ironbeard", "hunter2")),
            Some("ironbeard".to_owned())
        );
        assert_eq!(login_account(b"hello"), None);
        assert_eq!(login_account(&[0; 20]), None);
    }
}
