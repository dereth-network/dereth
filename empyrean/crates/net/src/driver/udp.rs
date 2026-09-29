// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/ConnectionListener.cs
//
// The socket half of ACE's `ConnectionListener` and `SocketManager.Initialize`, on std sockets.
//
// The v1 server read port `P + 1` without blocking and then blocked on `P` for 333 ms, while the
// client's `ConnectResponse` waited on `P + 1`: 1.4-2.4 s per login. Here each socket has its own
// thread blocked in `recv_from`, feeding one channel, so a datagram on either port reaches the
// caller the moment it arrives.
//
// The receive threads block with **no** read timeout, as ACE's `BeginReceiveFrom` does. On
// Windows a blocking `recv_from` whose `SO_RCVTIMEO` expires can discard a datagram that arrives
// at that moment ("If a blocking receive call times out, the socket state is indeterminate"). On
// loopback, with the old 100 ms timeout, most datagrams sent on the timer tick it expired on were
// lost (`net::real_sockets::the_listeners_lose_no_datagram_after_a_quiet_spell`).
// Shutdown instead wakes each thread with an empty datagram to its own socket.

use std::io;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;

use crate::client_packet::MAX_PACKET_SIZE;
use crate::{Outgoing, PortKind, ServerNet};

/// How often, and how many times, shutdown re-sends the wake-up datagram to a listener that has
/// not stopped yet (a datagram to oneself can still be dropped when the receive buffer is full).
const WAKE_RETRY: Duration = Duration::from_millis(10);
const WAKE_TRIES: u32 = 100;

/// One datagram read by a listener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub port_kind: PortKind,
    pub from: SocketAddr,
    pub bytes: Vec<u8>,
}

/// ACE's pair of `ConnectionListener`s for one host: `P` and `P + 1`.
#[derive(Debug)]
pub struct UdpDriver {
    c2s: Arc<UdpSocket>,
    s2c: Arc<UdpSocket>,
    rx: Receiver<Inbound>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl UdpDriver {
    // ACE: SocketManager.Initialize, ConnectionListener.Start
    /// Binds `host:port` and `host:port + 1` and starts a receive thread on each (ACE starts the
    /// `P + 1` listener first). `port` 0 picks a free pair, for tests.
    ///
    /// DIVERGE: ACE binds one pair per configured host and sets `SO_REUSEADDR`; this binds one
    /// host (ACE's default configuration) and leaves `SO_REUSEADDR` off, which on Windows would
    /// let another process take the port.
    ///
    /// # Errors
    /// The bind error of either socket.
    pub fn bind(host: IpAddr, port: u16) -> io::Result<Self> {
        connection_listener_new(host, u32::from(port));
        log::info!("Binding ConnectionListener to {host}:{port}");
        connection_listener_new(host, u32::from(port) + 1);
        log::info!(
            "Binding ConnectionListener to {host}:{}",
            u32::from(port) + 1
        );
        let (c2s, s2c) = bind_pair(host, port)?;
        let (c2s, s2c) = (Arc::new(c2s), Arc::new(s2c));
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let threads = vec![
            spawn_listener(
                Arc::clone(&s2c),
                PortKind::S2C,
                tx.clone(),
                Arc::clone(&stop),
            )?,
            spawn_listener(Arc::clone(&c2s), PortKind::C2S, tx, Arc::clone(&stop))?,
        ];
        Ok(Self {
            c2s,
            s2c,
            rx,
            stop,
            threads,
        })
    }

    /// The local address of one listener.
    ///
    /// # Errors
    /// The socket's `local_addr` error.
    pub fn local_addr(&self, kind: PortKind) -> io::Result<SocketAddr> {
        self.socket(kind).local_addr()
    }

    fn socket(&self, kind: PortKind) -> &UdpSocket {
        match kind {
            PortKind::C2S => &self.c2s,
            PortKind::S2C => &self.s2c,
        }
    }

    /// Waits up to `timeout` for a datagram from either listener.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Inbound> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// A datagram if one is already waiting.
    #[must_use]
    pub fn try_recv(&self) -> Option<Inbound> {
        self.rx.try_recv().ok()
    }

    /// ACE `socket.SendTo` in `NetworkSession.SendPacketRaw`, on the listener the session chose.
    ///
    /// # Errors
    /// The socket's send error.
    pub fn transmit(&self, out: &Outgoing) -> io::Result<()> {
        self.socket(out.via_port_kind)
            .send_to(&out.bytes, out.to)
            .map(|_| ())
    }

    /// One turn of a network loop: waits up to `wait` for traffic, hands every waiting datagram to
    /// `net`, polls it, and sends what it produced. A failed send is reported back to the session
    /// (ACE's `SendToSocketException`). `clock` is read after the wait, so the packets are stamped
    /// with the time they are handled at. Returns the pass's session count (ACE `DoSessionWork`'s
    /// result, which counts a session dropped in this pass).
    pub fn pump(
        &self,
        net: &mut ServerNet,
        wait: Duration,
        clock: impl Fn() -> ClockSnapshot,
    ) -> usize {
        let first = self.recv_timeout(wait);
        let now = clock();
        for d in first
            .into_iter()
            .chain(std::iter::from_fn(|| self.try_recv()))
        {
            net.on_datagram(d.port_kind, d.from, &d.bytes, now);
        }
        let session_count = net.do_session_work(now);
        let outgoing: Vec<Outgoing> = net.drain_outgoing().collect();
        for out in &outgoing {
            if let Err(e) = self.transmit(out) {
                if let Some(session) = out.session {
                    net.on_send_error(session, &e.to_string(), now);
                }
            }
        }
        session_count
    }

    // ACE: ConnectionListener.Shutdown
    /// Stops both receive threads and waits for them.
    ///
    /// DIVERGE: ACE closes the socket, which ends its pending `BeginReceiveFrom`. A std socket
    /// cannot be closed under a thread blocked on it, so each listener is sent an empty datagram
    /// from its own socket, and its thread exits on seeing the stop flag.
    pub fn shutdown(mut self) {
        self.stop_listeners();
        for t in std::mem::take(&mut self.threads) {
            for _ in 0..WAKE_TRIES {
                if t.is_finished() {
                    break;
                }
                self.wake_listeners();
                std::thread::sleep(WAKE_RETRY);
            }
            if t.is_finished() {
                let _ = t.join();
            } else {
                log::error!("ConnectionListener: a receive thread did not stop; leaving it");
            }
        }
    }

    fn stop_listeners(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.wake_listeners();
    }

    fn wake_listeners(&self) {
        for socket in [&self.c2s, &self.s2c] {
            if let Err(e) = wake(socket) {
                log::debug!("ConnectionListener wake-up: {e}");
            }
        }
    }
}

impl Drop for UdpDriver {
    fn drop(&mut self) {
        if !self.threads.is_empty() {
            self.stop_listeners();
        }
    }
}

/// An empty datagram from `socket` to itself, to end its blocked `recv_from`.
fn wake(socket: &UdpSocket) -> io::Result<()> {
    let mut to = socket.local_addr()?;
    if to.ip().is_unspecified() {
        to.set_ip(match to.ip() {
            IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        });
    }
    socket.send_to(&[], to).map(|_| ())
}

// ACE: ConnectionListener.ConnectionListener
/// `new ConnectionListener(host, port)`: ACE's constructor only logs and keeps the address, which
/// the socket bound in [`UdpDriver::bind`] keeps here.
fn connection_listener_new(host: IpAddr, port: u32) {
    log::debug!("ConnectionListener ctor, host {host} port {port}");
}

fn bind_pair(host: IpAddr, port: u16) -> io::Result<(UdpSocket, UdpSocket)> {
    if port != 0 {
        let c2s = UdpSocket::bind(SocketAddr::new(host, port))?;
        let s2c = UdpSocket::bind(SocketAddr::new(
            host,
            port.checked_add(1).ok_or(io::ErrorKind::InvalidInput)?,
        ))?;
        return Ok((c2s, s2c));
    }
    // An ephemeral `P` whose `P + 1` is also free.
    let mut last = io::Error::from(io::ErrorKind::AddrInUse);
    for _ in 0..64 {
        let c2s = UdpSocket::bind(SocketAddr::new(host, 0))?;
        let p = c2s.local_addr()?.port();
        let Some(p1) = p.checked_add(1) else { continue };
        match UdpSocket::bind(SocketAddr::new(host, p1)) {
            Ok(s2c) => return Ok((c2s, s2c)),
            Err(e) => last = e,
        }
    }
    Err(last)
}

// ACE: ConnectionListener.Listen
/// The receive loop of one listener. ACE's buffer is 1024 bytes (`ClientPacket.MaxPacketSize`);
/// one more byte is read here so that an oversized datagram is seen as oversized and dropped by
/// `ServerNet::on_datagram`, as ACE drops it, on every platform.
///
/// The receive blocks without a timeout (see the module comment); the stop flag is read after
/// each datagram, and `UdpDriver::shutdown` sends one to wake the thread.
fn spawn_listener(
    socket: Arc<UdpSocket>,
    kind: PortKind,
    tx: Sender<Inbound>,
    stop: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    socket.set_read_timeout(None)?;
    std::thread::Builder::new()
        .name(format!("empyrean-net-{kind:?}"))
        .spawn(move || {
            let mut buf = vec![0u8; MAX_PACKET_SIZE + 1];
            loop {
                let received = socket.recv_from(&mut buf);
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                match received {
                    Ok((n, from)) => {
                        if tx
                            .send(Inbound {
                                port_kind: kind,
                                from,
                                bytes: buf[..n].to_vec(),
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(e) => match e.kind() {
                        io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted => {}
                        // "If we get 'Connection has been forcibly closed...' error, just eat the
                        // exception and continue on"; likewise MessageSize and NetworkReset.
                        io::ErrorKind::ConnectionReset | io::ErrorKind::NetworkDown => {
                            log::debug!("ConnectionListener({kind:?}) receive: {e}");
                        }
                        _ if e.raw_os_error() == Some(10040) => {
                            // WSAEMSGSIZE: a datagram larger than the buffer (Windows).
                            log::debug!("ConnectionListener({kind:?}) receive: {e}");
                        }
                        _ => {
                            log::error!("ConnectionListener({kind:?}) receive has thrown: {e}");
                            return;
                        }
                    },
                }
            }
        })
}
