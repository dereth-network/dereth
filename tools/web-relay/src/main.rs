//! The local proxy a player runs to play in a browser on a server that speaks only UDP: one game
//! datagram per WebSocket message, on the player's own machine.
//!
//! **Depends on** `dereth-transport` for the frame (`dereth_transport::web_frame`) and
//! `tungstenite` for the WebSocket. **Used by** no crate: the web client's worker opens one
//! WebSocket to it per session, exactly as it opens one to a server that accepts WebSocket
//! connections itself, so the client has one path.
//!
//! **Must never** listen anywhere but loopback, send to any host but the one server it was started
//! for, or to any port of it but the logon port and the one above it, or accept a browser page
//! from any origin but a local one and the one the player names: it is a hole through the
//! browser's sandbox, and those rules keep it the size of one game session.
//!
//! ```text
//! dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]
//! ```
//!
//! It is only for servers that do not accept WebSocket connections (ACE, GDLE, or an Empyrean with
//! its WebSocket endpoint off); an Empyrean with the endpoint on is reached directly.
//!
//! Each WebSocket gets its own UDP socket, so the server sees one client per browser session, as
//! it would see one per game client. A message's port names one of the server's two ports in the
//! client's own numbering, fixed by the session's first datagram (its logon port, and the one
//! above it), and the relay maps them onto the server's real two; a datagram coming back is framed
//! with the port it came from in the same numbering. A message addressed to port 0 is the
//! session's **will**: the datagrams to send when its WebSocket closes, however it closes, so a
//! page that is closed still logs off. The frame is specified in
//! `docs/networking/05-websocket-frame.md`.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use dereth_transport::web_frame::{self, PortPair, ServerPort};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{Message, WebSocket};

/// The client's receive buffer: no datagram is longer.
const MAX_DATAGRAM: usize = 65_504;

/// How long a read of the WebSocket waits before the relay looks at the UDP socket again.
const POLL: Duration = Duration::from_millis(2);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Config {
    listen: SocketAddr,
    server: SocketAddr,
    /// The one page origin, besides local ones, that may open the relay.
    allow_origin: Option<String>,
}

fn usage() -> ! {
    eprintln!(
        "usage: dereth-web-relay --server <host:port> [--listen 127.0.0.1:9180] [--allow-origin <origin>]"
    );
    std::process::exit(2);
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut listen: SocketAddr = (Ipv4Addr::LOCALHOST, 9180).into();
    let mut server = None;
    let mut allow_origin = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = || -> Result<&String, String> {
            it.clone()
                .next()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--listen" => listen = value()?.parse().map_err(|e| format!("{a}: {e}"))?,
            "--server" => {
                server = Some(resolve_server(value()?).map_err(|e| format!("{a}: {e}"))?);
            }
            "--allow-origin" => {
                if allow_origin.is_some() {
                    return Err(format!(
                        "{a}: the relay admits one page origin besides local ones"
                    ));
                }
                allow_origin = Some(page_origin(value()?).map_err(|e| format!("{a}: {e}"))?);
            }
            other => return Err(format!("unknown argument {other}")),
        }
        it.next();
    }
    if !listen.ip().is_loopback() {
        return Err(format!(
            "--listen {listen}: the relay listens on loopback only"
        ));
    }
    let server = server.ok_or("--server is required")?;
    Ok(Config {
        listen,
        server,
        allow_origin,
    })
}

/// The server as `host:port`, where the host is an address or a name. A name is looked up once,
/// at start, and its first IPv4 address kept: the game's protocol carries a server address as four
/// bytes, so an IPv6-only name cannot be a server.
fn resolve_server(spec: &str) -> Result<SocketAddr, String> {
    if let Ok(addr) = spec.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let (host, port) = spec
        .rsplit_once(':')
        .ok_or_else(|| format!("{spec}: give the server as host:port"))?;
    let port: u16 = port.parse().map_err(|e| format!("{spec}: port: {e}"))?;
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("{spec}: {e}"))?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| format!("{spec}: no IPv4 address"))
}

/// A page origin as the player names it: `https://` or `http://`, a host and an optional port,
/// nothing after them. Kept without a trailing slash.
fn page_origin(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('/');
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or_else(|| format!("{value}: an origin is https://host or http://host[:port]"))?;
    if rest.is_empty() || rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return Err(format!(
            "{value}: an origin has a host and a port and nothing else"
        ));
    }
    Ok(value.to_owned())
}

/// A browser page's origin the relay accepts: one served from this machine, or the one the player
/// named. A request with no origin is not from a browser page, and is accepted.
fn origin_allowed(origin: Option<&str>, named: Option<&str>) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    if named.is_some_and(|n| n.eq_ignore_ascii_case(origin.trim_end_matches('/'))) {
        return true;
    }
    let host = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

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
fn destination<'a>(
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
fn will(
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
fn reply(
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

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut))
}

// The handshake callback's error type is the library's own response.
#[allow(clippy::result_large_err)]
fn serve(stream: TcpStream, config: &Config) -> Result<(), String> {
    let peer = stream.peer_addr().map_err(|e| e.to_string())?;
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
        if origin_allowed(origin, config.allow_origin.as_deref()) {
            Ok(resp)
        } else {
            eprintln!("{peer}: refused the page origin {}", origin.unwrap_or(""));
            let mut refusal = ErrorResponse::new(Some("origin not allowed".into()));
            *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
            Err(refusal)
        }
    };
    let mut ws: WebSocket<TcpStream> =
        tungstenite::accept_hdr(stream, check).map_err(|e| format!("{peer}: handshake: {e}"))?;
    ws.get_ref()
        .set_read_timeout(Some(POLL))
        .map_err(|e| e.to_string())?;

    let server = config.server;
    let local: IpAddr = if server.ip().is_loopback() {
        Ipv4Addr::LOCALHOST.into()
    } else {
        Ipv4Addr::UNSPECIFIED.into()
    };
    let udp = UdpSocket::bind((local, 0)).map_err(|e| e.to_string())?;
    udp.set_nonblocking(true).map_err(|e| e.to_string())?;
    eprintln!(
        "{peer}: session on {}",
        udp.local_addr().map_err(|e| e.to_string())?
    );

    let mut counts = (0u64, 0u64);
    let mut last_will = Vec::new();
    let result = relay(&mut ws, &udp, server, peer, &mut counts, &mut last_will);
    for (to, datagram) in &last_will {
        let _ = udp.send_to(datagram, *to);
    }
    let (out, back) = counts;
    eprintln!(
        "{peer}: closed after {out} datagrams out, {back} back{}",
        if last_will.is_empty() {
            String::new()
        } else {
            format!(", and its will of {}", last_will.len())
        }
    );
    result
}

/// Carry datagrams both ways until the WebSocket closes, keeping the session's latest will.
fn relay(
    ws: &mut WebSocket<TcpStream>,
    udp: &UdpSocket,
    server: SocketAddr,
    peer: SocketAddr,
    (out, back): &mut (u64, u64),
    last_will: &mut Vec<(SocketAddr, Vec<u8>)>,
) -> Result<(), String> {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    let mut pair = PortPair::new();
    loop {
        match ws.read() {
            Ok(Message::Binary(message)) => {
                if let Some(w) = will(server, &mut pair, &message) {
                    *last_will = w;
                } else if let Some((to, datagram)) = destination(server, &mut pair, &message) {
                    let _ = udp.send_to(datagram, to);
                    *out += 1;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => break,
            Err(e) => return Err(format!("{peer}: {e}")),
        }
        loop {
            match udp.recv_from(&mut buf) {
                Ok((len, from)) => {
                    if let Some(framed) = reply(server, &pair, from, &buf[..len]) {
                        *back += 1;
                        if let Err(e) = ws.write(Message::Binary(framed.into())) {
                            return Err(format!("{peer}: {e}"));
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // An ICMP port-unreachable for an earlier send; the client ignores it too.
                Err(e) if e.kind() == ErrorKind::ConnectionReset => {}
                Err(e) => return Err(format!("{peer}: {e}")),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => break,
            Err(e) => return Err(format!("{peer}: {e}")),
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("{e}");
        usage()
    });
    let listener = TcpListener::bind(config.listen).unwrap_or_else(|e| {
        eprintln!("{}: {e}", config.listen);
        std::process::exit(1);
    });
    eprintln!(
        "relaying ws://{} to udp {} and port {}{}",
        config.listen,
        config.server,
        config.server.port().saturating_add(1),
        config
            .allow_origin
            .as_deref()
            .map_or_else(String::new, |o| format!(", for local pages and {o}"))
    );
    for stream in listener.incoming().flatten() {
        let config = config.clone();
        std::thread::spawn(move || {
            if let Err(e) = serve(stream, &config) {
                eprintln!("{e}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    /// A relay for `server` on a loopback port, serving one connection.
    fn start(server: SocketAddr, allow_origin: Option<&str>) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let listen = listener.local_addr().unwrap();
        let config = Config {
            listen,
            server,
            allow_origin: allow_origin.map(str::to_owned),
        };
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let _ = serve(stream, &config);
        });
        listen
    }

    #[test]
    fn it_listens_on_loopback_only() {
        assert!(parse_args(&args("--server 127.0.0.1:9000")).is_ok());
        assert!(parse_args(&args("--server 127.0.0.1:9000 --listen 0.0.0.0:9180")).is_err());
        assert!(parse_args(&args("--listen 127.0.0.1:1")).is_err());
    }

    /// A server may be named: `localhost` resolves to its IPv4 address, an address is taken as it
    /// is, and a name without a port is refused.
    #[test]
    fn the_server_may_be_given_by_name() {
        assert_eq!(
            resolve_server("localhost:9000"),
            Ok("127.0.0.1:9000".parse().unwrap())
        );
        assert_eq!(
            resolve_server("10.0.0.5:9000"),
            Ok("10.0.0.5:9000".parse().unwrap())
        );
        assert!(resolve_server("localhost").is_err());
        assert!(parse_args(&args("--server localhost:9000")).is_ok());
    }

    #[test]
    fn only_a_local_page_may_open_it() {
        assert!(origin_allowed(None, None));
        assert!(origin_allowed(Some("http://127.0.0.1:8080"), None));
        assert!(origin_allowed(Some("http://localhost"), None));
        assert!(!origin_allowed(Some("https://example.com"), None));
        assert!(!origin_allowed(
            Some("http://127.0.0.1.example.com:8080"),
            None
        ));
        assert!(!origin_allowed(Some("null"), None));
    }

    /// Besides local pages, exactly one page origin the player names may open it, and it is named
    /// as an origin: a scheme, a host and a port, nothing more.
    #[test]
    fn the_one_page_origin_the_player_names_may_open_it() {
        let config = parse_args(&args(
            "--server 127.0.0.1:9000 --allow-origin https://play.example.org/",
        ))
        .unwrap();
        let named = config.allow_origin.as_deref();
        assert_eq!(named, Some("https://play.example.org"));
        assert!(origin_allowed(Some("https://play.example.org"), named));
        assert!(origin_allowed(Some("https://PLAY.example.org"), named));
        assert!(!origin_allowed(Some("http://play.example.org"), named));
        assert!(!origin_allowed(
            Some("https://play.example.org.evil.example"),
            named
        ));
        assert!(origin_allowed(Some("http://127.0.0.1:8080"), named));
        assert!(parse_args(&args(
            "--server 127.0.0.1:9000 --allow-origin https://a.example --allow-origin https://b.example"
        ))
        .is_err());
        assert!(parse_args(&args(
            "--server 127.0.0.1:9000 --allow-origin https://a.example/play"
        ))
        .is_err());
        assert!(parse_args(&args(
            "--server 127.0.0.1:9000 --allow-origin play.example.org"
        ))
        .is_err());
    }

    /// The session's first datagram fixes its numbering of the server's two ports; the relay sends
    /// to the server's real logon port and the one above it, and nowhere else.
    #[test]
    fn it_sends_only_to_the_servers_two_ports() {
        let server: SocketAddr = "10.0.0.5:9050".parse().unwrap();
        let mut pair = PortPair::new();
        let framed = web_frame::encode(9000, &[7]);
        let (to, d) = destination(server, &mut pair, &framed).unwrap();
        assert_eq!((to, d), ("10.0.0.5:9050".parse().unwrap(), &[7u8][..]));
        assert_eq!(
            destination(server, &mut pair, &web_frame::encode(9001, &[])).map(|(to, _)| to),
            Some("10.0.0.5:9051".parse().unwrap())
        );
        assert!(destination(server, &mut pair, &web_frame::encode(9002, &[1])).is_none());
        assert!(destination(server, &mut pair, &web_frame::encode(22, &[1])).is_none());
        assert!(destination(server, &mut pair, &[0x23]).is_none());
        assert_eq!(
            reply(server, &pair, "10.0.0.5:9051".parse().unwrap(), &[3]),
            Some(web_frame::encode(9001, &[3]))
        );
        assert_eq!(
            reply(server, &pair, "10.0.0.6:9050".parse().unwrap(), &[3]),
            None
        );
    }

    /// A whole round trip: a WebSocket client's message reaches a UDP "server", and its answer
    /// comes back framed with the port it was sent from, in the client's numbering.
    #[test]
    fn a_datagram_crosses_both_ways() {
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        game.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let server = game.local_addr().unwrap();
        let listen = start(server, None);

        let (mut ws, _) = tungstenite::connect(format!("ws://{listen}")).unwrap();
        ws.send(Message::Binary(web_frame::encode(9000, b"hello").into()))
            .unwrap();
        let mut buf = [0u8; 64];
        let (n, relay) = game.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello");
        game.send_to(b"welcome", relay).unwrap();
        let back = loop {
            if let Message::Binary(b) = ws.read().unwrap() {
                break b;
            }
        };
        assert_eq!(&back[..], web_frame::encode(9000, b"welcome").as_slice());
    }

    /// A will names its datagrams by length and port, keeps only those to the server's two
    /// ports, and is told apart from a datagram by its port 0.
    #[test]
    fn a_will_is_read_and_kept_to_the_servers_ports() {
        let server: SocketAddr = "10.0.0.5:9000".parse().unwrap();
        let mut pair = PortPair::new();
        let message = web_frame::encode_will(&[(9000, vec![1, 2]), (22, vec![3]), (9001, vec![])]);
        assert_eq!(
            will(server, &mut pair, &message),
            Some(vec![
                ("10.0.0.5:9000".parse().unwrap(), vec![1, 2]),
                ("10.0.0.5:9001".parse().unwrap(), vec![]),
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

    /// A page that goes away without closing its WebSocket still logs off: the relay sends the
    /// session's will from the session's own socket.
    #[test]
    fn a_session_that_drops_its_socket_leaves_its_will() {
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        game.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let server = game.local_addr().unwrap();
        let listen = start(server, None);

        let (mut ws, _) = tungstenite::connect(format!("ws://{listen}")).unwrap();
        ws.send(Message::Binary(web_frame::encode(9000, b"hello").into()))
            .unwrap();
        let mut buf = [0u8; 64];
        let (_, session) = game.recv_from(&mut buf).unwrap();
        ws.send(Message::Binary(
            web_frame::encode_will(&[(9000, b"goodbye".to_vec())]).into(),
        ))
        .unwrap();
        ws.flush().unwrap();
        // Gone without a close handshake, as a killed page's socket goes.
        drop(ws);
        let (n, from) = game.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"goodbye");
        assert_eq!(from, session, "from the session's own socket");
    }

    /// A page served from elsewhere, named at start, may open it; any other remote page may not.
    #[test]
    fn a_page_served_elsewhere_opens_it_only_when_named() {
        use tungstenite::client::IntoClientRequest;
        let game = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server = game.local_addr().unwrap();
        let connect = |listen: SocketAddr, origin: &str| {
            let mut request = format!("ws://{listen}").into_client_request().unwrap();
            request
                .headers_mut()
                .insert("Origin", origin.parse().unwrap());
            tungstenite::connect(request)
        };
        let named = start(server, Some("https://play.example.org"));
        assert!(connect(named, "https://play.example.org").is_ok());
        let unnamed = start(server, None);
        assert!(connect(unnamed, "https://play.example.org").is_err());
    }
}
