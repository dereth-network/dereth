//! The UDP socket: host/port parsing, interface selection, both port modes, the `SO_RCVBUF` search
//! and the non-blocking receive.
//!
//! The client links **WSOCK32.DLL** and imports exactly fourteen entry points: no `WSAEventSelect`,
//! no overlapped I/O, no `select`, no second thread. The socket is polled from the main loop with a
//! 50 ms budget, because packet processing mutates game state (the game clock and blob queues)
//! that the rest of the frame reads without locking.
//!
//! **This crate is permitted `unsafe` for socket setup and does not use any.** `socket2` covers
//! `SO_RCVBUF` and interface binding safely, so `lib.rs` keeps `#![deny(unsafe_code)]`.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};

#[cfg(not(target_arch = "wasm32"))]
use socket2::{Domain, Protocol, Socket, Type};
#[cfg(not(target_arch = "wasm32"))]
use std::net::SocketAddrV4;

use crate::NetError;

/// The client's default port is `0x1C88`.
///
/// **7304, not 9000.** ACE and the community tooling use 9000/19000 and the retail launcher always
/// passed an explicit `-h host:port`, so every test configuration must be explicit.
pub const DEFAULT_SERVER_PORT: u16 = 0x1C88;

/// The `SO_RCVBUF` the binary search aims for: 128 KiB.
pub const RCVBUF_TARGET: i32 = 0x2_0000;

/// The modulus in `compute_unique_port_sequence`: 64007.
pub const UNIQUE_PORT_MODULUS: u32 = 0xFA07;

/// Ports below this are skipped by the unique-port search.
pub const UNIQUE_PORT_FLOOR: u32 = 0x400;

/// `WSAEWOULDBLOCK` — the receive loop's normal exit.
pub const WSAEWOULDBLOCK: i32 = 10035;

/// `WSAECONNRESET`, which Windows raises on an ICMP port-unreachable for a previous `sendto`.
///
/// It ends the receive loop and makes the network step return `false`; **the client does not act on
/// that return value**, so it is not an error condition.
pub const WSAECONNRESET: i32 = 10054;

/// The net init sets the use-time limit interval to `0x32`.
pub const RECV_BUDGET_MS: u64 = 50;

/// A parsed `-h host[:port]` specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSpec {
    pub host: String,
    pub port: u16,
}

/// The socket bring-up, steps 1-2.
///
/// 1. Truncate at the first `,` — `"a.example.com,b.example.com"` uses only the first entry, a
///    vestige of a multi-host launcher format.
/// 2. Truncate at the first `:`; if one was found, `strtoul(rest, nullptr, 0)` is the port, and a
///    parse result of 0 falls back to 7304.
///
/// `strtoul` with base 0 is C's auto-detecting form: `0x` is hex, a leading `0` is octal, anything
/// else decimal. It also stops at the first character it cannot use rather than failing, so
/// `"9000junk"` is 9000. Both are reproduced.
#[must_use]
pub fn parse_host_spec(spec: &str, default_port: u16) -> HostSpec {
    let spec = spec.split(',').next().unwrap_or("");
    match spec.split_once(':') {
        None => HostSpec {
            host: spec.to_string(),
            port: default_port,
        },
        Some((host, rest)) => {
            let parsed = strtoul_base0(rest);
            let port = u16::try_from(parsed).unwrap_or(0);
            HostSpec {
                host: host.to_string(),
                // "a parse result of 0 falls back to 0x1C88"
                port: if port == 0 { DEFAULT_SERVER_PORT } else { port },
            }
        }
    }
}

/// The socket bring-up, step 3: resolving the server host.
///
/// Retail fills the server address's `sin_addr` with **`inet_addr()` first, then
/// `gethostbyname()`**, and raises the bad-server-address network error only when *both* fail.
/// Accepting a dotted quad and nothing else would refuse any name -- `localhost`, or a real
/// shard's hostname -- before a packet was sent.
///
/// **IPv4 only, and that is not an oversight.** `gethostbyname` is `AF_INET`, the server address is
/// a `sockaddr_in`, and the protocol carries a server address as four bytes, so an `AAAA` answer
/// has nowhere to go. A host that resolves to IPv6 only is `BadServerAddress` -- which is what
/// retail does with it.
///
/// **`None` is not covered by a test, deliberately.** On some machines
/// the resolver answers a *wildcard* address -- `10.21.23.94` -- for **every** input tried,
/// including a bogus name and the empty host. Many ISP and corporate resolvers do this. So on
/// such a network the second leg never fails, `BadServerAddress` is unreachable through this
/// function, and an assertion that a name is refused would pass or fail on whose network the
/// suite runs. It is left unwritten rather than written and flaky.
///
/// A consequence worth knowing while testing here: a mistyped `-h` does not error, it resolves
/// to the wildcard and the client sends its logon there instead.
#[must_use]
pub fn resolve_sin_addr(host: &str, port: u16) -> Option<SocketAddr> {
    // `inet_addr()`: a literal dotted quad never reaches the resolver.
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(SocketAddr::new(IpAddr::V4(ip), port));
    }
    // `gethostbyname()`.
    (host, port)
        .to_socket_addrs()
        .ok()?
        .find(SocketAddr::is_ipv4)
}

/// C's `strtoul(s, nullptr, 0)`: skip leading whitespace, optional sign, auto-detect the base from
/// a `0x` / `0` prefix, and stop at the first character that is not a digit in that base.
#[must_use]
pub fn strtoul_base0(s: &str) -> u32 {
    let s = s.trim_start();
    let s = s.strip_prefix('+').unwrap_or(s);
    let (radix, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
    {
        (16, rest)
    } else if s.starts_with('0') && s.len() > 1 {
        (8, &s[1..])
    } else {
        (10, s)
    };
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    u32::from_str_radix(&digits[..end], radix).unwrap_or(0)
}

/// 1 to 4 dot-separated decimal octets, with the missing ones
/// left-shifted in.
///
/// So `"10"` is `10.0.0.0` and `"10/8"`'s network half works. Returns the address as the big-endian
/// reading of its octets, which is what `htonl(sin_addr)` gives on x86.
#[must_use]
pub fn parse_in_addr(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut value = 0u32;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let octet: u32 = part.parse().ok()?;
        if octet > 255 {
            return None;
        }
        value |= octet << (24 - 8 * i);
    }
    Some(value)
}

/// `A.B.C.D/M`, where `M` is either a dotted-quad mask or a
/// CIDR prefix length of at most 32.
///
/// Returns `(network, mask)`.
///
/// # Errors
/// Returns an error on anything it cannot read.
pub fn parse_interface_string(spec: &str) -> Result<(u32, u32), NetError> {
    let bad = |reason| NetError::BadInterface {
        spec: spec.to_string(),
        reason,
    };
    let (net_str, mask_str) = spec
        .split_once('/')
        .ok_or_else(|| bad("expected A.B.C.D/M"))?;
    let network = parse_in_addr(net_str).ok_or_else(|| bad("bad network address"))?;

    // A CIDR length has no dots and is at most 32; anything else is read as a dotted-quad mask.
    let mask = if !mask_str.contains('.') {
        let bits: u32 = mask_str.parse().map_err(|_| bad("bad mask"))?;
        if bits > 32 {
            return Err(bad("CIDR prefix length above 32"));
        }
        if bits == 0 {
            0
        } else {
            u32::MAX << (32 - bits)
        }
    } else {
        parse_in_addr(mask_str).ok_or_else(|| bad("bad dotted-quad mask"))?
    };
    Ok((network, mask))
}

/// The interface bind, plus the `Net.BindInterface` filter.
///
/// If the preference is a non-empty string, the **first** enumerated interface satisfying
/// `(iface & mask) == network` is used. Otherwise the **first** enumerated interface is used —
/// note that "first" is `gethostbyname`'s order, which is not stable across machines and is the
/// reason the preference exists.
#[must_use]
pub fn select_interface(interfaces: &[Ipv4Addr], filter: Option<(u32, u32)>) -> Option<Ipv4Addr> {
    match filter {
        None => interfaces.first().copied(),
        Some((network, mask)) => interfaces
            .iter()
            .copied()
            .find(|a| u32::from(*a) & mask == network & mask),
    }
}

/// `Net.ComputeUniquePort` — the deterministic port sequence.
///
/// ```text
/// p = htonl(addr) % 0xFA07
/// repeat: try bind(p); p = (p + 0x100) & 0xFFFF; skip any p < 0x400
/// until p returns to the starting value  => ID_NetError_CantBind
/// ```
///
/// It exists so several clients behind one NAT get stable, distinct source ports. Because `0x100`
/// divides `0x10000` the sequence is 256 values with the same low byte, of which the four below
/// `0x400` are skipped — so 252 candidates, always in the same order for a given interface.
///
/// `htonl(sin_addr)` on x86 is the big-endian reading of the octets, i.e. `u32::from(addr)`.
#[must_use]
pub fn compute_unique_port_sequence(addr: Ipv4Addr) -> Vec<u16> {
    let start = u32::from(addr) % UNIQUE_PORT_MODULUS;
    let mut out = Vec::with_capacity(256);
    let mut p = start;
    loop {
        if p >= UNIQUE_PORT_FLOOR {
            out.push(u16::try_from(p).unwrap_or(0));
        }
        p = (p + 0x100) & 0xFFFF;
        if p == start {
            break;
        }
    }
    out
}

/// The `SO_RCVBUF` binary search, as an algorithm over whatever actually grants the buffer.
///
/// ```text
/// getsockopt(SO_RCVBUF, &cur)
/// hi = 0x20000
/// while (cur < hi) {
///     mid = (cur + hi + 1) / 2          // round-half-up
///     setsockopt(SO_RCVBUF, mid); getsockopt(SO_RCVBUF, &got)
///     if (got < mid) hi = mid - 1; else cur = mid;
/// }
/// ```
///
/// `grant` takes the requested size and returns what the stack actually gave, which is what makes
/// this testable without a socket. The round-half-up in `mid` matters: with `(cur + hi) / 2` a
/// stack that grants exactly what is asked settles one byte low.
pub fn rcvbuf_binary_search(mut cur: i32, mut grant: impl FnMut(i32) -> i32) -> i32 {
    let mut hi = RCVBUF_TARGET;
    while cur < hi {
        let mid = (cur + hi + 1) / 2;
        let got = grant(mid);
        if got < mid {
            hi = mid - 1;
        } else {
            cur = mid;
        }
    }
    cur
}

/// Is this the error that ends the receive loop quietly?
///
/// `WSAEWOULDBLOCK` is the normal drain-complete signal. `WSAECONNRESET` ends the loop and makes
/// the network step return `false`, which nothing reads — so it is not an error either.
#[must_use]
pub fn recv_error_ends_loop_quietly(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(WSAEWOULDBLOCK | WSAECONNRESET))
        || err.kind() == io::ErrorKind::WouldBlock
        || err.kind() == io::ErrorKind::ConnectionReset
}

/// How the socket picks its local port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortMode {
    /// `Net.ComputeUniquePort = false`. `0` lets the OS pick.
    Fixed(u16),
    /// `Net.ComputeUniquePort = true`.
    ComputeUnique,
}

/// The client's UDP socket. One socket for both directions (the read and write sockets are the same).
#[derive(Debug)]
pub struct NetSocket {
    socket: UdpSocket,
    /// The `SO_RCVBUF` the stack actually granted.
    pub rcvbuf: i32,
    /// The address it bound to.
    pub local_addr: SocketAddr,
}

impl NetSocket {
    /// The send path, minus the address resolution the caller does.
    ///
    /// # Errors
    /// Returns an error if no candidate port binds.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn bind(interface: Ipv4Addr, mode: PortMode) -> Result<Self, NetError> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

        let mut last_err = None;
        let mut bound = false;
        match mode {
            PortMode::Fixed(port) => {
                let addr = SocketAddrV4::new(interface, port);
                socket.bind(&addr.into())?;
                bound = true;
            }
            PortMode::ComputeUnique => {
                for port in compute_unique_port_sequence(interface) {
                    let addr = SocketAddrV4::new(interface, port);
                    match socket.bind(&addr.into()) {
                        Ok(()) => {
                            bound = true;
                            break;
                        }
                        Err(e) => last_err = Some(e),
                    }
                }
            }
        }
        if !bound {
            return Err(NetError::Io(last_err.unwrap_or_else(|| {
                io::Error::new(io::ErrorKind::AddrInUse, "ID_NetError_CantBind")
            })));
        }

        // The SO_RCVBUF binary search, capped at 128 KiB.
        let current = i32::try_from(socket.recv_buffer_size()?).unwrap_or(0);
        let rcvbuf = rcvbuf_binary_search(current, |mid| {
            let want = usize::try_from(mid).unwrap_or(0);
            if socket.set_recv_buffer_size(want).is_err() {
                return 0;
            }
            // Windows reports back exactly what it granted; Linux doubles it, which the search
            // handles because it only ever compares `got < mid`.
            socket
                .recv_buffer_size()
                .map_or(0, |g| i32::try_from(g).unwrap_or(i32::MAX))
        });

        socket.set_nonblocking(true)?;
        let socket: UdpSocket = socket.into();
        let local_addr = socket.local_addr()?;
        Ok(Self {
            socket,
            rcvbuf,
            local_addr,
        })
    }

    /// A browser has no UDP socket: the WebAssembly build refuses to bind, and a client there moves
    /// the session's datagrams over a channel of its own.
    ///
    /// # Errors
    /// Always [`io::ErrorKind::Unsupported`].
    #[cfg(target_arch = "wasm32")]
    pub fn bind(_interface: Ipv4Addr, _mode: PortMode) -> Result<Self, NetError> {
        Err(NetError::Io(io::Error::new(
            io::ErrorKind::Unsupported,
            "no UDP socket on this platform",
        )))
    }

    /// `recvfrom`, with the client's 65,504-byte length.
    ///
    /// Returns `Ok(None)` when the loop should stop quietly — `WSAEWOULDBLOCK` (the receive buffer
    /// is drained) or `WSAECONNRESET` (an ICMP port-unreachable for a previous `sendto`).
    ///
    /// # Errors
    /// an I/O error for anything else.
    pub fn recv_from(&self, buf: &mut [u8]) -> Result<Option<(usize, SocketAddr)>, NetError> {
        let len = buf.len().min(dereth_transport::wire::RECV_BUFFER_SIZE);
        match self.socket.recv_from(&mut buf[..len]) {
            Ok(v) => Ok(Some(v)),
            Err(e) if recv_error_ends_loop_quietly(&e) => Ok(None),
            Err(e) => Err(NetError::Io(e)),
        }
    }

    /// `sendto`. A short write is reported as failure, which
    /// makes `transmit_new_packets` stop for this tick.
    ///
    /// # Errors
    /// Returns an error when the datagram cannot be sent. The client discards the corresponding
    /// socket error code.
    pub fn send_to(&self, buf: &[u8], to: SocketAddr) -> Result<bool, NetError> {
        match self.socket.send_to(buf, to) {
            Ok(n) => Ok(n == buf.len()),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(e) => Err(NetError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `inet_addr()` first, then `gethostbyname()`.
    ///
    /// No datagram leaves this process -- `to_socket_addrs` resolves a name, it does not connect.
    #[test]
    fn a_dotted_quad_resolves_without_a_name_lookup() {
        // The `inet_addr()` leg, and the port riding through it.
        assert_eq!(
            resolve_sin_addr("127.0.0.1", 19000),
            Some("127.0.0.1:19000".parse().unwrap())
        );
        assert_eq!(
            resolve_sin_addr("10.0.0.7", 7304),
            Some("10.0.0.7:7304".parse().unwrap())
        );
        // Not asserted here: that `256.0.0.1` comes back `None`. It does on this machine, but it
        // is a non-quad and therefore a resolver question, and this network's resolver answers a
        // wildcard for everything -- see `resolve_sin_addr`. Same reason the refusal test is gone.
    }

    /// The `gethostbyname()` leg, which this build did not have: before it, every name was
    /// the bad-server-address network error before a packet was sent.
    #[test]
    fn a_name_is_resolved_and_only_ipv4_comes_back() {
        let got = resolve_sin_addr("localhost", 19000).expect("localhost must resolve");
        assert!(got.is_ipv4(), "the protocol carries four bytes: {got}");
        assert!(got.ip().is_loopback(), "{got}");
        assert_eq!(
            got.port(),
            19000,
            "the port rides through the resolver: {got}"
        );
    }

    /// Oracle: the host-spec parser transcribed in the recovered transport behavior §1.1.
    #[test]
    fn host_spec_parsing() {
        // No port: the default, which is 7304 and not 9000.
        assert_eq!(
            parse_host_spec("ac.turbine.com", DEFAULT_SERVER_PORT),
            HostSpec {
                host: "ac.turbine.com".into(),
                port: 7304
            }
        );
        assert_eq!(DEFAULT_SERVER_PORT, 7304);

        // A comma truncates -- only the first entry is used.
        assert_eq!(
            parse_host_spec("a.example.com,b.example.com", DEFAULT_SERVER_PORT).host,
            "a.example.com"
        );
        assert_eq!(
            parse_host_spec("a.example.com:9000,b.example.com:9001", DEFAULT_SERVER_PORT),
            HostSpec {
                host: "a.example.com".into(),
                port: 9000
            }
        );

        // An explicit port.
        assert_eq!(
            parse_host_spec("127.0.0.1:19000", DEFAULT_SERVER_PORT),
            HostSpec {
                host: "127.0.0.1".into(),
                port: 19000
            }
        );

        // A zero port falls back to 7304, not to the caller's default.
        assert_eq!(parse_host_spec("127.0.0.1:0", 9000).port, 7304);
        assert_eq!(parse_host_spec("127.0.0.1:", 9000).port, 7304);
        assert_eq!(parse_host_spec("127.0.0.1:junk", 9000).port, 7304);
    }

    /// `strtoul(rest, nullptr, 0)` auto-detects the base and stops at the first unusable character.
    #[test]
    fn port_parsing_is_strtoul_base_zero() {
        assert_eq!(strtoul_base0("9000"), 9000);
        assert_eq!(strtoul_base0("0x1C88"), 7304);
        assert_eq!(strtoul_base0("017"), 15, "a leading zero is octal");
        assert_eq!(
            strtoul_base0("9000junk"),
            9000,
            "stops at the first bad char"
        );
        assert_eq!(strtoul_base0("junk"), 0);
        assert_eq!(strtoul_base0(""), 0);
    }

    /// The address parser accepts 1-4 octets and left-shifts the missing ones.
    #[test]
    fn partial_addresses_shift_left() {
        assert_eq!(parse_in_addr("10.0.0.1"), Some(0x0A00_0001));
        assert_eq!(parse_in_addr("10"), Some(0x0A00_0000));
        assert_eq!(parse_in_addr("192.168"), Some(0xC0A8_0000));
        assert_eq!(parse_in_addr("172.16.5"), Some(0xAC10_0500));
        assert_eq!(parse_in_addr("1.2.3.4.5"), None);
        assert_eq!(parse_in_addr("256.0.0.1"), None);
        assert_eq!(parse_in_addr("10..0.1"), None);
    }

    /// `A.B.C.D/M` takes either a dotted-quad mask or a CIDR length, and `10/8` works because of
    /// the left-shift rule above.
    #[test]
    fn interface_string_accepts_both_mask_forms() {
        assert_eq!(
            parse_interface_string("10/8").expect("10/8 must parse"),
            (0x0A00_0000, 0xFF00_0000)
        );
        assert_eq!(
            parse_interface_string("192.168.1.0/255.255.255.0").expect("dotted quad"),
            (0xC0A8_0100, 0xFFFF_FF00)
        );
        assert_eq!(
            parse_interface_string("192.168.1.0/24").expect("cidr"),
            (0xC0A8_0100, 0xFFFF_FF00)
        );
        assert_eq!(parse_interface_string("0.0.0.0/0").expect("all"), (0, 0));
        assert_eq!(
            parse_interface_string("10.0.0.0/32").expect("host"),
            (0x0A00_0000, u32::MAX)
        );

        assert!(parse_interface_string("10.0.0.0/33").is_err());
        assert!(parse_interface_string("10.0.0.0").is_err());
        assert!(parse_interface_string("nonsense/8").is_err());
    }

    /// The first interface matching `(iface & mask) == network`; without a filter, simply the first.
    #[test]
    fn interface_selection() {
        let ifaces = [
            Ipv4Addr::new(169, 254, 3, 4),
            Ipv4Addr::new(192, 168, 1, 55),
            Ipv4Addr::new(10, 1, 2, 3),
        ];
        assert_eq!(select_interface(&ifaces, None), Some(ifaces[0]));
        let filter = parse_interface_string("10/8").expect("parse");
        assert_eq!(select_interface(&ifaces, Some(filter)), Some(ifaces[2]));
        let filter = parse_interface_string("192.168.1.0/24").expect("parse");
        assert_eq!(select_interface(&ifaces, Some(filter)), Some(ifaces[1]));
        let filter = parse_interface_string("172.16/12").expect("parse");
        assert_eq!(select_interface(&ifaces, Some(filter)), None);
        assert_eq!(select_interface(&[], None), None);
    }

    /// The `htonl(addr) % 0xFA07` sequence, its `+ 0x100` step and its `< 0x400` skip.
    ///
    /// Oracle: the recovered transport behavior §1.2.
    #[test]
    fn compute_unique_port_reproduces_the_documented_sequence() {
        let addr = Ipv4Addr::new(192, 168, 1, 55);
        let raw = u32::from(addr); // 0xC0A80137
        assert_eq!(raw, 0xC0A8_0137);
        let start = raw % UNIQUE_PORT_MODULUS;
        let seq = compute_unique_port_sequence(addr);

        assert_eq!(
            u32::from(seq[0]),
            start,
            "the starting value is tried first"
        );
        // Every step is +0x100 mod 0x10000, so the low byte never changes.
        let low = seq[0] & 0xFF;
        assert!(seq.iter().all(|p| p & 0xFF == low));
        // Four of the 256 candidates are below 0x400 and are skipped.
        assert_eq!(seq.len(), 252);
        assert!(seq.iter().all(|p| u32::from(*p) >= UNIQUE_PORT_FLOOR));
        // The sequence terminates rather than cycling.
        let mut sorted = seq.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), seq.len(), "no repeats");

        // Deterministic: the same interface always yields the same sequence, which is the point.
        assert_eq!(compute_unique_port_sequence(addr), seq);
        assert_ne!(
            compute_unique_port_sequence(Ipv4Addr::new(192, 168, 1, 56)),
            seq
        );
    }

    /// An address whose starting value is below 0x400 skips it and starts higher.
    #[test]
    fn compute_unique_port_skips_the_low_range() {
        // Find an address whose `% 0xFA07` lands under 0x400.
        let addr = Ipv4Addr::from(UNIQUE_PORT_MODULUS * 3 + 0x100);
        let seq = compute_unique_port_sequence(addr);
        assert!(seq.iter().all(|p| u32::from(*p) >= UNIQUE_PORT_FLOOR));
        // The walk starts at 0x100 and wraps through 0x0000, so 0x000, 0x100, 0x200 and 0x300 are
        // all below the floor and skipped; 0x400 is the first candidate actually tried.
        assert_eq!(u32::from(seq[0]), 0x400);
        assert_eq!(seq.len(), 252);
    }

    /// The binary search settles on 128 KiB when the stack is generous, and on the largest granted
    /// value when it is not.
    ///
    /// Oracle: the client's receive-buffer search, which halves the interval between the last
    /// refused and the last granted size, rounding `mid` half up.
    #[test]
    fn rcvbuf_search_finds_128k_or_the_largest_granted() {
        // A generous stack grants exactly what is asked.
        let mut calls = 0;
        let got = rcvbuf_binary_search(8192, |mid| {
            calls += 1;
            mid
        });
        assert_eq!(got, RCVBUF_TARGET);
        assert_eq!(got, 131_072);
        assert!(calls <= 18, "a binary search over 2^17, not a linear walk");

        // A stack capped at 40000 grants at most that.
        let got = rcvbuf_binary_search(8192, |mid| mid.min(40_000));
        assert_eq!(got, 40_000);

        // One already at the target does nothing at all.
        let mut calls = 0;
        let got = rcvbuf_binary_search(RCVBUF_TARGET, |mid| {
            calls += 1;
            mid
        });
        assert_eq!(got, RCVBUF_TARGET);
        assert_eq!(calls, 0);

        // A stack that refuses everything leaves the current value untouched.
        let got = rcvbuf_binary_search(8192, |_| 0);
        assert_eq!(got, 8192);
    }

    /// Round-half-up is load-bearing: `(cur + hi) / 2` never reaches `hi` and the search would
    /// settle one short. Pinned so a "simplification" fails here rather than in a capture.
    #[test]
    fn the_midpoint_rounds_half_up() {
        let mut cur = 131_071;
        let hi = RCVBUF_TARGET;
        let mid = (cur + hi + 1) / 2;
        assert_eq!(mid, RCVBUF_TARGET);
        cur = mid;
        assert_eq!(cur, hi, "the loop terminates");
        assert_eq!(
            (131_071 + hi) / 2,
            131_071,
            "the naive midpoint does not move"
        );
    }

    /// `WSAECONNRESET` (10054) from `recv_from` ends the loop without erroring, and so does
    /// `WSAEWOULDBLOCK` (10035).
    #[test]
    fn conn_reset_and_would_block_end_the_loop_quietly() {
        assert!(recv_error_ends_loop_quietly(&io::Error::from_raw_os_error(
            WSAECONNRESET
        )));
        assert!(recv_error_ends_loop_quietly(&io::Error::from_raw_os_error(
            WSAEWOULDBLOCK
        )));
        assert!(recv_error_ends_loop_quietly(&io::Error::from(
            io::ErrorKind::WouldBlock
        )));
        assert!(!recv_error_ends_loop_quietly(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
    }

    /// The receive budget is 50 ms. Compatibility note #68, resolved.
    #[test]
    fn the_receive_budget_is_fifty_milliseconds() {
        assert_eq!(RECV_BUDGET_MS, 50);
    }

    /// A real socket: binds on loopback, is non-blocking, and round-trips a datagram.
    ///
    /// Oracle: the client's own `ioctlsocket(FIONBIO, 1)` and the
    /// `SO_RCVBUF` search.
    #[test]
    fn a_real_socket_binds_non_blocking_with_a_large_receive_buffer() {
        let s = NetSocket::bind(Ipv4Addr::LOCALHOST, PortMode::Fixed(0)).expect("bind");
        assert_ne!(s.local_addr.port(), 0, "the OS picked a port");
        assert!(s.rcvbuf > 0);

        // Non-blocking: an empty socket returns "nothing to read", not a hang and not an error.
        let mut buf = [0u8; 64];
        assert_eq!(s.recv_from(&mut buf).expect("drained"), None);

        let peer = NetSocket::bind(Ipv4Addr::LOCALHOST, PortMode::Fixed(0)).expect("bind peer");
        assert!(peer.send_to(b"hello", s.local_addr).expect("send"));
        // Give the loopback a moment; the loop mirrors the client's drain-until-would-block.
        for _ in 0..1000 {
            if let Some((n, _from)) = s.recv_from(&mut buf).expect("recv") {
                assert_eq!(&buf[..n], b"hello");
                return;
            }
        }
        panic!("the loopback datagram never arrived");
    }
}
