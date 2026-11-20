//! The recorder: a UDP proxy on loopback between a game client and its server, which passes every
//! datagram on and writes each to the capture file as it goes.
//!
//! # Ports
//!
//! A server listens on a pair of adjacent ports, `P` and `P + 1`: the client sends to `P`, but its
//! `ConnectResponse` to `P + 1`, and checks only the address of what comes back, never the port.
//! So the recorder listens on a pair of its own and forwards each to the matching server port,
//! through one socket toward the server per port, so that what the server sends back leaves on
//! the listener it belongs to.
//!
//! # Referrals
//!
//! A server with a separate world service (Yalain) sends the client a `Referral` naming the world's
//! address, and the client goes there for the rest of the session. Passed on as it is, that would
//! take the client past the recorder. So the referral is the one datagram the client receives
//! changed: the address in it becomes a second pair of the recorder's own, which forwards to the
//! world, and the datagram's checksum is made again with the key it was sent under (recovered from
//! the datagram itself). The file holds the referral as the server sent it. A datagram that would
//! not come out of the transport's own serializer byte for byte is passed on unchanged instead.
//!
//! # Only loopback
//!
//! The recorder listens on a loopback address and nowhere else, and takes from its sockets toward
//! a server only what comes from that server's address: nothing on the network can reach the
//! client through it, and the recording stays on the player's machine.

use std::fs::File;
use std::io::{self, BufWriter};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use dereth_transport::conn::Referral;
use dereth_transport::{OutPacket, PacketFlags, ParsedPacket};

use crate::capture::CaptureWriter;

/// Where the recorder listens unless told otherwise: the client is pointed here.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:9100";
/// The server's address unless told otherwise: a server on this machine, on the port Yalain and
/// Empyrean both listen on by default.
pub const DEFAULT_SERVER: &str = "127.0.0.1:9000";

/// How often a waiting socket looks up to see whether the recording has stopped.
const WAKE: Duration = Duration::from_millis(100);

/// How many pairs of adjacent ports above its own the recorder tries for a referred server before
/// it takes any free pair.
const NEARBY_PAIRS: u16 = 8;

/// What to record.
#[derive(Debug, Clone)]
pub struct Options {
    /// Where the client is pointed; the port above it is listened on too. Port 0 takes any free
    /// pair.
    pub listen: SocketAddr,
    /// The server's first port; the one above it is forwarded to too.
    pub server: SocketAddrV4,
    /// The capture file. It must not exist yet.
    pub out: PathBuf,
}

/// The listening address, if it is an IPv4 loopback address; why not, otherwise.
///
/// # Errors
/// Any other address: the recorder never listens where another machine could reach it.
pub fn loopback_only(listen: SocketAddr) -> Result<SocketAddrV4, String> {
    match listen {
        SocketAddr::V4(v4) if v4.ip().is_loopback() => Ok(v4),
        SocketAddr::V4(v4) => Err(format!(
            "--listen {v4}: the recorder listens on loopback only (such as 127.0.0.1), so that \
             nothing else on the network can reach the game through it"
        )),
        SocketAddr::V6(v6) => Err(format!(
            "--listen {v6}: the recorder listens on an IPv4 loopback address (such as 127.0.0.1)"
        )),
    }
}

/// How many datagrams crossed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// From the client to a server.
    pub to_server: u64,
    /// From a server to the client.
    pub to_client: u64,
    /// Referrals pointed at the recorder.
    pub referrals: u64,
}

/// One pair of the recorder's ports and the server pair it forwards to.
#[derive(Debug)]
struct Pair {
    /// The client's side.
    listen: [UdpSocket; 2],
    /// The server's side.
    upstream: [UdpSocket; 2],
    /// The first of the listening ports.
    listen_at: SocketAddrV4,
    /// The server's first port.
    server: SocketAddrV4,
}

/// What the forwarding threads share under one lock: the file, the counts, and where the client
/// and the servers are.
#[derive(Debug)]
struct State {
    file: CaptureWriter<BufWriter<File>>,
    counts: Counts,
    failure: Option<String>,
    /// Every pair's listening and server addresses, by pair.
    routes: Vec<(SocketAddrV4, SocketAddrV4)>,
    /// The client's address on each port of each pair, once it has sent there.
    clients: Vec<[Option<SocketAddrV4>; 2]>,
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    stop: AtomicBool,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A recording in progress.
#[derive(Debug)]
pub struct Recorder {
    shared: Arc<Shared>,
    listen: SocketAddrV4,
}

impl Recorder {
    /// Bind the listening pair and the file, and start passing datagrams on.
    ///
    /// # Errors
    /// A listening address that is not loopback, a port that will not bind, or a file that exists
    /// already or cannot be made.
    pub fn start(options: &Options) -> Result<Self, String> {
        let listen = loopback_only(options.listen)?;
        let first = (listen.port() != 0).then_some(listen.port());
        let pair = bind_pair(*listen.ip(), first, options.server).map_err(|e| {
            format!(
                "cannot listen on {listen} and the port above it: {e}. Is another recorder \
                 still running?"
            )
        })?;
        let file = create_new(&options.out)?;
        let listen_at = pair.listen_at;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                file: CaptureWriter::new(BufWriter::new(file))
                    .map_err(|e| format!("{}: {e}", options.out.display()))?,
                counts: Counts::default(),
                failure: None,
                routes: Vec::new(),
                clients: Vec::new(),
            }),
            stop: AtomicBool::new(false),
            threads: Mutex::new(Vec::new()),
        });
        {
            let mut s = shared.state();
            add_pair(&shared, &mut s, pair).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            shared,
            listen: listen_at,
        })
    }

    /// Where the client is pointed: the first port of the listening pair.
    #[must_use]
    pub const fn listen(&self) -> SocketAddrV4 {
        self.listen
    }

    /// How many datagrams have crossed so far.
    #[must_use]
    pub fn counts(&self) -> Counts {
        self.shared.state().counts
    }

    /// Why the recording failed, if it has.
    #[must_use]
    pub fn failure(&self) -> Option<String> {
        self.shared.state().failure.clone()
    }

    /// Stop passing datagrams on, and finish the file.
    ///
    /// # Errors
    /// When the file could not be written.
    pub fn stop(self) -> Result<Counts, String> {
        self.shared.stop.store(true, Ordering::SeqCst);
        loop {
            let threads = std::mem::take(
                &mut *self
                    .shared
                    .threads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            );
            if threads.is_empty() {
                break;
            }
            for t in threads {
                let _ = t.join();
            }
        }
        let mut s = self.shared.state();
        s.file
            .flush()
            .map_err(|e| format!("finishing the file: {e}"))?;
        if let Some(f) = s.failure.take() {
            return Err(f);
        }
        Ok(s.counts)
    }
}

/// A new file, never one that is there already.
fn create_new(path: &Path) -> Result<File, String> {
    File::create_new(path).map_err(|e| {
        if e.kind() == io::ErrorKind::AlreadyExists {
            format!(
                "{} already exists; a recording is never written over: name another file",
                path.display()
            )
        } else {
            format!("{}: {e}", path.display())
        }
    })
}

/// Bind a listening pair on `ip` (at `first` and the port above it, or any free pair), and the two
/// sockets toward `server`.
fn bind_pair(ip: Ipv4Addr, first: Option<u16>, server: SocketAddrV4) -> io::Result<Pair> {
    let listen = match first {
        Some(p) => {
            let above = p.checked_add(1).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "no port above 65535")
            })?;
            [UdpSocket::bind((ip, p))?, UdpSocket::bind((ip, above))?]
        }
        None => any_adjacent_pair(ip)?,
    };
    // Toward a server on this machine the sockets stay on loopback too.
    let toward = if server.ip().is_loopback() {
        Ipv4Addr::LOCALHOST
    } else {
        Ipv4Addr::UNSPECIFIED
    };
    let upstream = [UdpSocket::bind((toward, 0))?, UdpSocket::bind((toward, 0))?];
    let listen_at = match listen[0].local_addr()? {
        SocketAddr::V4(a) => a,
        SocketAddr::V6(_) => return Err(io::Error::other("an IPv4 socket with an IPv6 address")),
    };
    Ok(Pair {
        listen,
        upstream,
        listen_at,
        server,
    })
}

/// Two sockets on adjacent free ports of `ip`.
pub(crate) fn any_adjacent_pair(ip: Ipv4Addr) -> io::Result<[UdpSocket; 2]> {
    for _ in 0..256 {
        let a = UdpSocket::bind((ip, 0))?;
        let Some(above) = a.local_addr()?.port().checked_add(1) else {
            continue;
        };
        if let Ok(b) = UdpSocket::bind((ip, above)) {
            return Ok([a, b]);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        "no two adjacent free ports",
    ))
}

/// Register a pair and start its four forwarding threads. Returns its index.
fn add_pair(shared: &Arc<Shared>, s: &mut State, pair: Pair) -> io::Result<usize> {
    let index = s.routes.len();
    s.routes.push((pair.listen_at, pair.server));
    s.clients.push([None, None]);
    for socket in pair.listen.iter().chain(&pair.upstream) {
        socket.set_read_timeout(Some(WAKE))?;
    }
    let pair = Arc::new(pair);
    let mut threads = shared
        .threads
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    for port in 0..2 {
        for to_server in [true, false] {
            let shared = Arc::clone(shared);
            let pair = Arc::clone(&pair);
            threads.push(std::thread::spawn(move || {
                pump(&shared, &pair, index, port, to_server);
            }));
        }
    }
    Ok(index)
}

/// `at` with its port moved up by `by`.
fn above(at: SocketAddrV4, by: usize) -> SocketAddrV4 {
    let port = at
        .port()
        .saturating_add(u16::try_from(by).unwrap_or(u16::MAX));
    SocketAddrV4::new(*at.ip(), port)
}

/// One direction of one port of one pair: receive, record, pass on, until the recording stops.
///
/// The record is written and the datagram sent under one lock, so nothing is recorded ahead of
/// what caused it.
fn pump(shared: &Arc<Shared>, pair: &Pair, index: usize, port: usize, to_server: bool) {
    let recv = if to_server {
        &pair.listen[port]
    } else {
        &pair.upstream[port]
    };
    let mut buf = vec![0u8; 65_536];
    while !shared.stop.load(Ordering::SeqCst) {
        let (n, from) = match recv.recv_from(&mut buf) {
            Ok(got) => got,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::Interrupted
                ) =>
            {
                // A reset or a refusal is the system reporting that an earlier send found no one
                // listening; the socket is still good.
                continue;
            }
            Err(e) => {
                let mut s = shared.state();
                s.failure = Some(format!("receiving on {}: {e}", pair.listen_at));
                shared.stop.store(true, Ordering::SeqCst);
                return;
            }
        };
        let SocketAddr::V4(from) = from else {
            continue;
        };
        let data = &buf[..n];
        let now = SystemTime::now();
        let mut s = shared.state();
        let written = if to_server {
            s.clients[index][port] = Some(from);
            let to = above(pair.server, port);
            let w = s.file.datagram(now, from, to, data);
            let _ = pair.upstream[port].send_to(data, to);
            s.counts.to_server += 1;
            w
        } else {
            // Only the server's own address is listened to on its side.
            if from.ip() != pair.server.ip() {
                continue;
            }
            let [here, other] = [port, 1 - port].map(|p| s.clients[index][p]);
            let Some(client) = here.or(other) else {
                // Nothing to pass it on to yet.
                continue;
            };
            let passed = referral_here(shared, &mut s, pair, data);
            let w = s.file.datagram(now, from, client, data);
            let _ = pair.listen[port].send_to(passed.as_deref().unwrap_or(data), client);
            s.counts.to_client += 1;
            w
        };
        if let Err(e) = written.and_then(|()| s.file.flush()) {
            s.failure = Some(format!("writing the file: {e}"));
            shared.stop.store(true, Ordering::SeqCst);
            return;
        }
    }
}

/// When `data` is a referral, the datagram the client is given instead: the same referral, to a
/// pair of the recorder's that forwards to the referred server (made now if there is none yet).
fn referral_here(shared: &Arc<Shared>, s: &mut State, pair: &Pair, data: &[u8]) -> Option<Vec<u8>> {
    let referred = referred_to(data)?;
    // A referral that names no address means the server's own.
    let referred = if referred.ip().is_unspecified() {
        SocketAddrV4::new(*pair.server.ip(), referred.port())
    } else {
        referred
    };
    let listen_at = match s.routes.iter().find(|(_, server)| *server == referred) {
        Some((listen_at, _)) => *listen_at,
        None => {
            let ip = *pair.listen_at.ip();
            let nearby = (1..=NEARBY_PAIRS)
                .filter_map(|k| pair.listen_at.port().checked_add(2 * k))
                .find_map(|p| bind_pair(ip, Some(p), referred).ok());
            let new = match nearby {
                Some(p) => p,
                None => bind_pair(ip, None, referred).ok()?,
            };
            let at = new.listen_at;
            add_pair(shared, s, new).ok()?;
            at
        }
    };
    let changed = refer_to(data, listen_at)?;
    s.counts.referrals += 1;
    Some(changed)
}

/// The address a datagram refers the client to, if it carries a referral.
#[must_use]
pub fn referred_to(datagram: &[u8]) -> Option<SocketAddrV4> {
    let flags = u32::from_le_bytes(datagram.get(4..8)?.try_into().ok()?);
    if flags & PacketFlags::REFERRAL == 0 {
        return None;
    }
    let p = ParsedPacket::parse(datagram).ok()?;
    Referral::from_bytes(p.optional.get(&PacketFlags::REFERRAL)?)
        .ok()
        .map(|r| r.addr)
}

/// A referral datagram changed to refer the client to `to`, with its checksum made again under the
/// key it was sent with. `None` when it carries no referral, or when the transport would not write
/// it back byte for byte as it came.
#[must_use]
pub fn refer_to(datagram: &[u8], to: SocketAddrV4) -> Option<Vec<u8>> {
    referred_to(datagram)?;
    let p = ParsedPacket::parse(datagram).ok()?;
    let key = p.header.header.is_encrypted().then(|| p.recovered_key());
    let mut out = OutPacket::new(p.header);
    for (mask, body) in p.optional {
        out.add_optional_header(mask, body).ok()?;
    }
    for f in p.fragments {
        out.add_fragment(f).ok()?;
    }
    if out.serialize(key).ok()? != datagram {
        return None;
    }
    let body = out.optional.get_mut(&PacketFlags::REFERRAL)?;
    // The address is a `sockaddr_in` eight bytes in: its port big-endian, then the address.
    body.get_mut(10..12)?
        .copy_from_slice(&to.port().to_be_bytes());
    body.get_mut(12..16)?.copy_from_slice(&to.ip().octets());
    out.serialize(key).ok()
}

/// The server's address from what the player typed: a host and a port, the host's first IPv4
/// address.
///
/// # Errors
/// When the name does not resolve to an IPv4 address.
pub fn server_address(text: &str) -> Result<SocketAddrV4, String> {
    use std::net::ToSocketAddrs as _;
    let all = text
        .to_socket_addrs()
        .map_err(|e| format!("--server {text}: {e} (want HOST:PORT)"))?;
    all.into_iter()
        .find_map(|a| match a.ip() {
            IpAddr::V4(ip) => Some(SocketAddrV4::new(ip, a.port())),
            IpAddr::V6(_) => None,
        })
        .ok_or_else(|| format!("--server {text}: no IPv4 address"))
}

#[cfg(test)]
mod tests;
