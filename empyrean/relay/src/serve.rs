//! The relay over sockets.
//!
//! The side towards Dereth listens on the loopback address and nothing else, and [`listen`] refuses
//! any other: the relay sees the player's password and retail-derived data, so it runs on the player's
//! own machine and is never hosted.
//!
//! The side towards the server listens for nothing. Each session has one ephemeral socket,
//! `connect()`ed to the server, so the system hands it datagrams from that peer alone. It binds the
//! loopback address when the server is on it. Only for a server elsewhere does it bind the unspecified
//! address, which is what lets the system route to that server.

use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use empyrean_common::clock::{Clock, SystemClock};
use empyrean_net::driver::udp::UdpDriver;
use empyrean_net::{ClockSnapshot, PortKind, SessionId};

use crate::relay::{Config, Finished, Relay};

/// How long one turn waits for a datagram from Dereth when nothing else is moving.
const TURN: Duration = Duration::from_millis(2);

/// A socket towards one server, reading only that server. Loopback when the server is.
fn server_socket(server: SocketAddr) -> io::Result<UdpSocket> {
    let local: IpAddr = if server.ip().is_loopback() {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    };
    let socket = UdpSocket::bind(SocketAddr::new(local, 0))?;
    socket.connect(server)?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

/// The listeners towards Dereth, on `host`, which must be a loopback address.
///
/// # Errors
/// Any other address, or a socket that does not bind.
pub fn listen(host: IpAddr, port: u16) -> io::Result<UdpDriver> {
    if !host.is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the relay listens on the loopback address only, not {host}"),
        ));
    }
    UdpDriver::bind(host, port)
}

/// When the relay has nothing left to do: no session has run and Dereth has sent nothing for
/// `limit`. A relay a launcher started outlives a launcher that went away by no more than that.
#[derive(Debug)]
struct Idle {
    limit: Option<f64>,
    since: f64,
}

impl Idle {
    fn new(limit: Option<Duration>, now: f64) -> Self {
        Self {
            limit: limit.map(|l| l.as_secs_f64()),
            since: now,
        }
    }

    /// Whether the relay has been idle for its limit, at `now`, with `busy` saying whether it is
    /// doing anything this turn.
    fn over(&mut self, busy: bool, now: f64) -> bool {
        if busy {
            self.since = now;
        }
        self.limit.is_some_and(|limit| now - self.since >= limit)
    }
}

/// Run until `done` says to stop (`done` is told each session that ends), or until nothing has
/// happened for `idle_exit`.
///
/// # Errors
/// A socket that does not bind.
pub fn run(
    config: Config,
    fresh_secret: empyrean_net::status_ping::SecretSource,
    idle_exit: Option<Duration>,
    mut done: impl FnMut(&Finished) -> bool,
) -> io::Result<()> {
    let clock = SystemClock::new();
    let driver = listen(IpAddr::V4(Ipv4Addr::LOCALHOST), config.port)?;
    let mut relay = Relay::new(config, fresh_secret);
    let mut sockets: BTreeMap<SessionId, UdpSocket> = BTreeMap::new();
    let mut buffer = [0u8; 2048];
    let mut idle = Idle::new(idle_exit, clock.monotonic().as_secs_f64());
    log::info!(
        "listening on {} and {}",
        driver.local_addr(PortKind::C2S)?,
        driver.local_addr(PortKind::S2C)?
    );
    loop {
        let snapshot = |clock: &SystemClock| -> ClockSnapshot {
            ClockSnapshot::take(clock, clock.monotonic().as_secs_f64())
        };
        let mut heard = false;
        if let Some(d) = driver.recv_timeout(TURN) {
            heard = true;
            let now = snapshot(&clock);
            relay.on_later_datagram(d.port_kind, d.from, &d.bytes, now);
            while let Some(d) = driver.try_recv() {
                relay.on_later_datagram(d.port_kind, d.from, &d.bytes, now);
            }
        }
        let now = snapshot(&clock);
        for (session, socket) in &sockets {
            loop {
                match socket.recv_from(&mut buffer) {
                    Ok((n, from)) => relay.on_server_datagram(*session, from, &buffer[..n], now),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    // A closed port on the far side answers a send with this on Windows.
                    Err(e) if e.kind() == io::ErrorKind::ConnectionReset => break,
                    Err(e) => {
                        log::warn!("reading from the server: {e}");
                        break;
                    }
                }
            }
        }
        relay.step(now);
        for out in relay.take_for_dereth() {
            if let Err(e) = driver.transmit(&out) {
                if let Some(session) = out.session {
                    relay.net().on_send_error(session, &e.to_string(), now);
                }
            }
        }
        for out in relay.take_for_servers() {
            let socket = match sockets.entry(out.session) {
                std::collections::btree_map::Entry::Occupied(s) => s.into_mut(),
                std::collections::btree_map::Entry::Vacant(s) => s.insert(server_socket(out.to)?),
            };
            if let Err(e) = socket.send(&out.bytes) {
                log::debug!("sending to the server: {e}");
            }
        }
        sockets.retain(|session, _| relay.bridge(*session).is_some());
        let mut stop = false;
        for finished in relay.take_finished() {
            stop |= done(&finished);
        }
        if stop
            || idle.over(
                heard || !sockets.is_empty(),
                clock.monotonic().as_secs_f64(),
            )
        {
            driver.shutdown();
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (where the relay's sockets may listen).

    use super::*;

    #[test]
    fn a_relay_stops_only_after_its_limit_with_nothing_happening() {
        let limit = Some(Duration::from_secs(600));
        let mut idle = Idle::new(limit, 0.0);
        assert!(!idle.over(false, 599.0));
        assert!(idle.over(false, 600.0), "nothing since it started");
        let mut idle = Idle::new(limit, 0.0);
        assert!(!idle.over(true, 599.0), "a session");
        assert!(
            !idle.over(false, 1198.0),
            "counted from the session's last turn"
        );
        assert!(idle.over(false, 1199.0));
        let mut never = Idle::new(None, 0.0);
        assert!(!never.over(false, 1e9), "no limit: it runs until stopped");
    }

    #[test]
    fn the_listener_refuses_anything_but_the_loopback_address() {
        for host in ["0.0.0.0", "192.168.1.10", "10.0.0.1", "::"] {
            let refused = listen(host.parse().unwrap(), 0).map(drop);
            assert_eq!(
                refused.map_err(|e| e.kind()),
                Err(io::ErrorKind::InvalidInput),
                "{host}"
            );
        }
        let driver = listen(IpAddr::V4(Ipv4Addr::LOCALHOST), 0).unwrap();
        assert!(driver.local_addr(PortKind::C2S).unwrap().ip().is_loopback());
        assert!(driver.local_addr(PortKind::S2C).unwrap().ip().is_loopback());
        driver.shutdown();
    }

    #[test]
    fn a_loopback_server_is_reached_from_the_loopback_address_and_no_other_peer_is_heard() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let stranger = UdpSocket::bind("127.0.0.1:0").unwrap();
        let socket = server_socket(server.local_addr().unwrap()).unwrap();
        let local = socket.local_addr().unwrap();
        assert!(local.ip().is_loopback());
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        // The stranger's datagram goes first: a socket that took it would read it first.
        stranger.send_to(b"stranger", local).unwrap();
        server.send_to(b"server", local).unwrap();
        let mut buffer = [0u8; 16];
        let (n, from) = socket.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], b"server");
        assert_eq!(from, server.local_addr().unwrap());
    }
}
