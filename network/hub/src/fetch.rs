//! The hub's own outbound requests: reading a world's verification page, and Discord's sign-in.
//!
//! One small HTTP/1.1 client over a socket the hub opens itself, so that it decides where every
//! request may go:
//!
//! - **Addresses.** A URL's host is resolved once; the request goes only if every address it
//!   resolves to passes the caller's [`AddressRule`], and connects to an address it checked (never
//!   resolving the name again, so the name cannot be pointed elsewhere between the check and the
//!   connection). [`is_public`] is the rule for a URL an operator names: no loopback, private,
//!   link-local, shared, multicast, documentation or otherwise reserved address, in either family,
//!   nor an IPv4 address mapped or translated into IPv6 from one of those.
//! - **Redirects** are followed only as far as the caller allows, each one checked again by the
//!   caller ([`Fetch::check`]) and by the rule.
//! - **Limits.** One deadline for the whole request, redirects included; the body is read only up
//!   to a limit, the rest left unread.
//! - **TLS** for `https://` through rustls with the web's root certificates, the name checked
//!   against the certificate.
//!
//! Nothing here logs a request's body or its headers.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Whether the hub may connect to an address.
pub type AddressRule = fn(IpAddr) -> bool;

/// A URL the client can fetch: `http://` or `https://`, a host, a port and a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub tls: bool,
    /// A host name in lower case, or an address (an IPv6 one without its brackets).
    pub host: String,
    pub port: u16,
    /// The path and query, starting with `/`: printable ASCII only, anything else
    /// percent-encoded.
    pub target: String,
}

/// A port as a URL writes it: one to five digits, not zero.
fn port_of(p: &str) -> Result<u16, String> {
    if p.is_empty() || p.len() > 5 || !p.bytes().all(|b| b.is_ascii_digit()) {
        return Err("a port".into());
    }
    p.parse().map_err(|_| "a port".into())
}

/// A path and query (fragment already dropped) as a request line may carry it: starting with `/`,
/// every byte outside printable ASCII (a space or a control character included) percent-encoded.
fn target_of(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 1);
    if !raw.starts_with('/') {
        out.push('/');
    }
    for b in raw.bytes() {
        if b.is_ascii_graphic() {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Url {
    /// Reads a URL: `http://` or `https://`, no user or password, an ASCII host, an optional
    /// port and a path; a fragment is dropped. Any string at all is read without panicking.
    ///
    /// # Errors
    /// What is wrong with it.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s.len() > 2_000 || s.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err("not a URL".into());
        }
        let (tls, rest) = if let Some(r) = strip_prefix_ignore_case(s, "https://") {
            (true, r)
        } else if let Some(r) = strip_prefix_ignore_case(s, "http://") {
            (false, r)
        } else {
            return Err("an http:// or https:// URL".into());
        };
        let rest = rest.split('#').next().unwrap_or("");
        let split = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, target) = rest.split_at(split);
        if authority.contains('@') {
            return Err("a URL without a user or password".into());
        }
        let target = target_of(target);
        let default_port = if tls { 443 } else { 80 };
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (addr, after) = v6.split_once(']').ok_or("an IPv6 address in brackets")?;
            let port = match after.strip_prefix(':') {
                Some(p) => port_of(p)?,
                None if after.is_empty() => default_port,
                None => return Err("a port".into()),
            };
            let ip: Ipv6Addr = addr.parse().map_err(|_| "an IPv6 address")?;
            (ip.to_string(), port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (h.to_owned(), port_of(p)?),
                None => (authority.to_owned(), default_port),
            }
        };
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() || port == 0 {
            return Err("a host and a port".into());
        }
        let name_ok = host.parse::<IpAddr>().is_ok()
            || host.split('.').all(|l| {
                !l.is_empty()
                    && l.len() <= 63
                    && !l.starts_with('-')
                    && !l.ends_with('-')
                    && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            });
        if !name_ok {
            return Err("a host name or an address".into());
        }
        Ok(Self {
            tls,
            host,
            port,
            target,
        })
    }

    /// The URL another resolves against: an absolute URL as it is, a path on this one's host.
    ///
    /// # Errors
    /// When `location` is neither.
    pub fn join(&self, location: &str) -> Result<Self, String> {
        let l = location.trim();
        if strip_prefix_ignore_case(l, "https://").is_some()
            || strip_prefix_ignore_case(l, "http://").is_some()
        {
            return Self::parse(l);
        }
        if let Some(rest) = l.strip_prefix("//") {
            return Self::parse(&format!(
                "{}://{rest}",
                if self.tls { "https" } else { "http" }
            ));
        }
        if l.starts_with('/') {
            let mut u = self.clone();
            u.target = target_of(l.split('#').next().unwrap_or("/"));
            return Ok(u);
        }
        Err("a redirect to somewhere that is not a URL or a path".into())
    }

    /// The host as a `Host` header writes it.
    fn host_header(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == if self.tls { 443 } else { 80 } {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }
}

/// `s` after `prefix`, matched without regard to ASCII case; none when `s` does not start with
/// it (a multi-byte character where the prefix would end included).
fn strip_prefix_ignore_case<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| s.get(prefix.len()..))
}

/// Whether `ip` is an address on the public internet: not unspecified, loopback, private,
/// link-local, shared (carrier-grade NAT), multicast, broadcast, documentation, benchmarking or
/// otherwise reserved, in either family; an IPv6 address that maps, embeds or translates an IPv4
/// one is judged by that IPv4 address.
#[must_use]
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0                                     // "this network"
        || a == 10                               // private
        || a == 127                              // loopback
        || (a == 100 && (64..=127).contains(&b)) // shared address space
        || (a == 169 && b == 254)                // link-local
        || (a == 172 && (16..=31).contains(&b))  // private
        || (a == 192 && b == 0 && c == 0)        // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)        // documentation
        || (a == 192 && b == 88 && c == 99)      // 6to4 relay anycast (deprecated)
        || (a == 192 && b == 168)                // private
        || (a == 198 && (18..=19).contains(&b))  // benchmarking
        || (a == 198 && b == 51 && c == 100)     // documentation
        || (a == 203 && b == 0 && c == 113)      // documentation
        || a >= 224) // multicast, reserved, broadcast
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    // An IPv4 address mapped into IPv6 (::ffff:a.b.c.d), or the old compatible form (::a.b.c.d).
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let v4_in = |hi: u16, lo: u16| {
        let [a, b] = hi.to_be_bytes();
        let [c, d] = lo.to_be_bytes();
        Ipv4Addr::new(a, b, c, d)
    };
    if s[..6] == [0; 6] {
        return ip != Ipv6Addr::UNSPECIFIED
            && ip != Ipv6Addr::LOCALHOST
            && is_public_v4(v4_in(s[6], s[7]));
    }
    // NAT64 (64:ff9b::/96; the local-use 64:ff9b:1::/48 is never public) carries an IPv4 address.
    if s[0] == 0x64 && s[1] == 0xff9b {
        return s[2..6] == [0; 4] && is_public_v4(v4_in(s[6], s[7]));
    }
    // 6to4 (2002::/16) carries an IPv4 address in its next 32 bits.
    if s[0] == 0x2002 {
        return is_public_v4(v4_in(s[1], s[2]));
    }
    // Only global unicast (2000::/3) is public, less its special blocks.
    (s[0] & 0xe000) == 0x2000
        && !(s[0] == 0x2001 && s[1] < 0x0200) // IETF protocol assignments, Teredo, benchmarking, ORCHID
        && !(s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        && !(s[0] == 0x3fff && s[1] < 0x1000) // documentation (2024)
}

/// Why a request was not made, or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The URL, or a redirect, is not one the caller allows ([`Fetch::check`]); the message says
    /// why.
    NotAllowed(String),
    /// The host resolves to an address the rule refuses.
    AddressNotAllowed,
    /// A redirect the caller does not follow, or one too many.
    Redirect,
    /// No answer within the deadline.
    Timeout,
    /// Anything else: no address, no connection, TLS, an answer that is not HTTP.
    Failed(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAllowed(why) => write!(f, "not allowed: {why}"),
            Self::AddressNotAllowed => {
                f.write_str("the host's address is not one the hub may reach")
            }
            Self::Redirect => f.write_str("a redirect the hub does not follow"),
            Self::Timeout => f.write_str("no answer in time"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

/// An answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// The body, at most the request's limit of it.
    pub body: Vec<u8>,
    /// The URL that answered, after redirects.
    pub url: Url,
}

/// One request and its limits.
#[derive(Debug, Clone)]
pub struct Fetch<'a> {
    pub method: &'a str,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The whole request's time, redirects included.
    pub timeout: Duration,
    /// The most of a body that is read.
    pub max_body: usize,
    /// How many redirects are followed (a `GET` only).
    pub max_redirects: usize,
    /// Which addresses may be connected to.
    pub rule: AddressRule,
    /// Ranges never connected to, whatever the rule says: the hub's own network, its public
    /// address, the ranges its operator names.
    pub deny: Vec<crate::forwarded::ProxyRange>,
    /// Every URL, the first and each redirect, must pass this.
    pub check: fn(&Url, &[String]) -> Result<(), String>,
    /// What `check` is given beside the URL (the hosts a verification may read, say).
    pub check_with: Vec<String>,
}

/// A check that allows every URL.
///
/// # Errors
/// Never.
pub fn any_url(_: &Url, _: &[String]) -> Result<(), String> {
    Ok(())
}

impl Fetch<'_> {
    /// Makes the request.
    ///
    /// # Errors
    /// [`FetchError`].
    pub fn send(&self) -> Result<Response, FetchError> {
        let deadline = Instant::now() + self.timeout;
        let mut url = self.url.clone();
        let mut redirects = 0;
        loop {
            (self.check)(&url, &self.check_with).map_err(FetchError::NotAllowed)?;
            let r = self.once(&url, deadline)?;
            if (300..400).contains(&r.status) && r.status != 304 {
                let location = r
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("location"))
                    .map(|(_, v)| v.clone());
                if redirects >= self.max_redirects || self.method != "GET" {
                    return Err(FetchError::Redirect);
                }
                let Some(location) = location else {
                    return Err(FetchError::Redirect);
                };
                url = url.join(&location).map_err(|_| FetchError::Redirect)?;
                redirects += 1;
                continue;
            }
            return Ok(r);
        }
    }

    fn once(&self, url: &Url, deadline: Instant) -> Result<Response, FetchError> {
        let addrs: Vec<SocketAddr> = match url.host.parse::<IpAddr>() {
            Ok(ip) => vec![SocketAddr::new(ip, url.port)],
            Err(_) => (url.host.as_str(), url.port)
                .to_socket_addrs()
                .map_err(|e| FetchError::Failed(format!("{}: {e}", url.host)))?
                .collect(),
        };
        if addrs.is_empty() {
            return Err(FetchError::Failed(format!("{}: no address", url.host)));
        }
        // Every address the name gives must pass, so the one connected to cannot be chosen
        // against the check.
        if !addrs
            .iter()
            .all(|a| (self.rule)(a.ip()) && !crate::forwarded::is_trusted(&self.deny, a.ip()))
        {
            return Err(FetchError::AddressNotAllowed);
        }
        let left = remaining(deadline)?;
        let tcp = TcpStream::connect_timeout(&addrs[0], left.min(Duration::from_secs(5))).map_err(
            |e| {
                if e.kind() == io::ErrorKind::TimedOut {
                    FetchError::Timeout
                } else {
                    FetchError::Failed(e.to_string())
                }
            },
        )?;
        let _ = tcp.set_nodelay(true);
        let mut stream = if url.tls {
            Stream::Tls(Box::new(tls(url, tcp)?))
        } else {
            Stream::Plain(tcp)
        };
        let mut head = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.method,
            url.target,
            url.host_header(),
        );
        // The hub names itself, unless the request names its client otherwise (Discord's bot API
        // asks for a form of its own).
        if !self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("user-agent"))
        {
            head.push_str(&format!("User-Agent: {}\r\n", crate::server::SOFTWARE));
        }
        for (k, v) in &self.headers {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        if !self.body.is_empty() || matches!(self.method, "POST" | "PUT") {
            head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        head.push_str("\r\n");
        stream.timeouts(deadline)?;
        io_step(stream.write_all(head.as_bytes()))?;
        io_step(stream.write_all(&self.body))?;
        io_step(stream.flush())?;
        read_response(&mut stream, deadline, self.max_body, url.clone())
    }
}

fn remaining(deadline: Instant) -> Result<Duration, FetchError> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or(FetchError::Timeout)
}

fn io_error(e: &io::Error) -> FetchError {
    match e.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => FetchError::Timeout,
        _ => FetchError::Failed(e.to_string()),
    }
}

fn io_step<T>(r: io::Result<T>) -> Result<T, FetchError> {
    r.map_err(|e| io_error(&e))
}

fn tls(
    url: &Url,
    tcp: TcpStream,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, FetchError> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| FetchError::Failed(e.to_string()))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(url.host.clone())
        .map_err(|e| FetchError::Failed(e.to_string()))?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|e| FetchError::Failed(e.to_string()))?;
    Ok(rustls::StreamOwned::new(conn, tcp))
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(s) => s,
            Self::Tls(s) => s.get_ref(),
        }
    }

    /// Every read and write waits at most until the deadline.
    fn timeouts(&self, deadline: Instant) -> Result<(), FetchError> {
        let left = remaining(deadline)?;
        let tcp = self.tcp();
        io_step(tcp.set_read_timeout(Some(left)))?;
        io_step(tcp.set_write_timeout(Some(left)))
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

/// The most a response's head may be.
const MAX_HEAD: usize = 16 * 1024;

fn read_response(
    stream: &mut Stream,
    deadline: Instant,
    max_body: usize,
    url: Url,
) -> Result<Response, FetchError> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let (status, headers, head_len) = loop {
        stream.timeouts(deadline)?;
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            // A peer that closes without TLS's close_notify still sent what it sent.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(io_error(&e)),
        };
        if n == 0 {
            return Err(FetchError::Failed(
                "the answer ended before its head".into(),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        let mut hs = [httparse::EMPTY_HEADER; 64];
        let mut r = httparse::Response::new(&mut hs);
        match r.parse(&buf) {
            Ok(httparse::Status::Complete(len)) => {
                let headers: Vec<(String, String)> = r
                    .headers
                    .iter()
                    .map(|h| {
                        (
                            h.name.to_owned(),
                            String::from_utf8_lossy(h.value).into_owned(),
                        )
                    })
                    .collect();
                break (r.code.unwrap_or(0), headers, len);
            }
            Ok(httparse::Status::Partial) if buf.len() > MAX_HEAD => {
                return Err(FetchError::Failed("the answer's head is too long".into()))
            }
            Ok(httparse::Status::Partial) => {}
            Err(e) => return Err(FetchError::Failed(format!("not an HTTP answer: {e}"))),
        }
    };
    let header = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_owned())
    };
    let chunked =
        header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
    let length: Option<usize> = header("content-length").and_then(|v| v.parse().ok());
    let mut raw = buf.split_off(head_len);
    // What the body may still need to arrive: enough to decode `max_body` bytes of it.
    let want = |raw: &[u8]| -> bool {
        if chunked {
            decode_chunked(raw, max_body).1
        } else {
            raw.len() < length.unwrap_or(usize::MAX).min(max_body)
        }
    };
    // A redirect's body is not wanted; `HEAD`-like answers have none.
    let bodiless = (300..400).contains(&status) || status == 204 || status == 304;
    while !bodiless && want(&raw) && raw.len() < max_body + 64 * 1024 {
        stream.timeouts(deadline)?;
        let n = match stream.read(&mut chunk) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(io_error(&e)),
        };
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..n]);
    }
    let mut body = if bodiless {
        Vec::new()
    } else if chunked {
        decode_chunked(&raw, max_body).0
    } else {
        raw
    };
    if let Some(l) = length {
        body.truncate(l);
    }
    body.truncate(max_body);
    Ok(Response {
        status,
        headers,
        body,
        url,
    })
}

/// A chunked body's bytes as far as they have arrived, at most `max`; and whether more is wanted
/// (the last chunk has not arrived and fewer than `max` bytes have).
fn decode_chunked(raw: &[u8], max: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        if out.len() >= max {
            out.truncate(max);
            return (out, false);
        }
        let Some(eol) = raw[i..].windows(2).position(|w| w == b"\r\n") else {
            return (out, true);
        };
        let line = String::from_utf8_lossy(&raw[i..i + eol]);
        let size = line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size, 16) else {
            return (out, false);
        };
        if size == 0 {
            return (out, false);
        }
        let start = i + eol + 2;
        let end = start.saturating_add(size);
        if end > raw.len() {
            out.extend_from_slice(&raw[start..]);
            out.truncate(max);
            return (out.clone(), out.len() < max);
        }
        out.extend_from_slice(&raw[start..end]);
        i = end + 2;
        if i > raw.len() {
            return (out, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address")
    }

    /// Spec: hub: a page an operator names is read only from a public address: every private,
    /// loopback, link-local, shared, multicast, documentation and reserved range is refused, in
    /// either family and in IPv6's forms of IPv4 addresses.
    #[test]
    fn only_public_addresses_pass_the_verification_rule() {
        for refused in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.254",
            "127.0.0.1",
            "127.8.9.10",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.10",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.7",
            "203.0.113.9",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b::7f00:1",
            "2002:7f00:1::",
            "2002:c0a8:101::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "2001:db8::1",
            "2001::1",
            "2001:2::1",
            "3fff::1",
            "100::1",
        ] {
            assert!(!is_public(ip(refused)), "{refused} is refused");
        }
        for allowed in [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.1",
            "172.15.255.255",
            "172.32.0.1",
            "192.0.1.1",
            "192.169.0.1",
            "198.17.255.255",
            "203.0.114.1",
            "223.255.255.254",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:808:808::1",
            "2606:4700:4700::1111",
            "2a00:1450:4001:80b::200e",
        ] {
            assert!(is_public(ip(allowed)), "{allowed} is public");
        }
    }

    /// Spec: hub: a URL to read is `http://` or `https://` without a user or a password.
    #[test]
    fn a_url_is_http_or_https_without_credentials() {
        let u = Url::parse("HTTPS://Coldeve.Example.org:8443/status?x=1#top").expect("a URL");
        assert_eq!(
            (u.tls, u.host.as_str(), u.port, u.target.as_str()),
            (true, "coldeve.example.org", 8443, "/status?x=1")
        );
        let u = Url::parse("http://[2001:db8::1]/").expect("a URL");
        assert_eq!((u.host.as_str(), u.port), ("2001:db8::1", 80));
        for bad in [
            "ftp://example.org/",
            "https://user:pw@example.org/",
            "https://example.org@10.0.0.1/",
            "file:///etc/passwd",
            "https:///nohost",
            "https://exa mple.org/",
            "https://example.org:0/",
            "https://example.org:99999/",
            "gopher://example.org:70/",
        ] {
            assert!(Url::parse(bad).is_err(), "{bad}");
        }
        let base = Url::parse("https://a.example.org/x").expect("a URL");
        assert_eq!(base.join("/y").expect("a path").target, "/y");
        assert_eq!(
            base.join("http://b.example.org/z").expect("a URL").host,
            "b.example.org"
        );
        assert_eq!(
            base.join("//c.example.org/").expect("a URL").host,
            "c.example.org"
        );
        assert!(base.join("javascript:alert(1)").is_err());
        assert_eq!(
            Url::parse("https://a.example.org/s\u{283}a?q=\u{283}")
                .expect("a URL")
                .target,
            "/s%CA%83a?q=%CA%83"
        );
        for bad in [
            "https:/\u{e9}/x",
            "http\u{e9}://a.example.org/",
            "https://\u{e9}xample.org/",
            "https://a.example.org:+80/",
            "https://a.example.org:080000/",
            "https://a.example.org\\@evil.example/",
            "https://a.example.org\\evil.example/",
        ] {
            assert!(Url::parse(bad).is_err(), "{bad}");
        }
    }

    /// Spec: hub: any URL an operator names, and any redirect a page answers, is read without the
    /// hub failing: refused, or a URL whose host is an ASCII name or address and whose path is
    /// printable ASCII.
    #[test]
    fn odd_urls_and_redirects_are_read_or_refused_never_a_failure() {
        let schemes = [
            "https://",
            "HTTP://",
            "http:/",
            "https:",
            "ftp://",
            "",
            "h\u{e9}://",
            "https://\u{e9}",
        ];
        let users = ["", "u@", "u:p@", "@", "\u{e9}@"];
        let hosts = [
            "a.example.org",
            "A.EXAMPLE.ORG.",
            "[2001:db8::1]",
            "[::1",
            "2001:db8::1",
            "127.0.0.1",
            "\u{e9}.example",
            "xn--caf-dma.example",
            "a..b",
            "-a.example",
            "",
            "a\\b.example",
            "\u{feff}a.example",
            "a.example.org%2F",
        ];
        let ports = [
            "", ":", ":80", ":0", ":65535", ":65536", ":+1", ":-1", ":\u{661}", ":8a",
        ];
        let paths = [
            "",
            "/",
            "/x?y=1#z",
            "?q",
            "#f",
            "/\u{e9}\u{2603}",
            "\\x",
            "/%zz",
            "/ x",
            "/\u{7f}",
            "//double",
            "/\u{10ffff}",
        ];
        let mut checked = 0;
        let base = Url::parse("https://a.example.org/x").expect("a URL");
        for scheme in schemes {
            for user in users {
                for host in hosts {
                    for port in ports {
                        for path in paths {
                            let s = format!("{scheme}{user}{host}{port}{path}");
                            for read in [Url::parse(&s), base.join(&s)] {
                                checked += 1;
                                let Ok(u) = read else { continue };
                                assert!(u.port != 0, "{s:?}");
                                assert!(u.target.starts_with('/'), "{s:?} {u:?}");
                                assert!(
                                    u.target.bytes().all(|b| b.is_ascii_graphic()),
                                    "{s:?} {u:?}"
                                );
                                assert!(
                                    u.host.parse::<IpAddr>().is_ok()
                                        || u.host.bytes().all(|b| b.is_ascii_lowercase()
                                            || b.is_ascii_digit()
                                            || b == b'.'
                                            || b == b'-'),
                                    "{s:?} {u:?}"
                                );
                                let scheme = if u.tls { "https" } else { "http" };
                                let again = Url::parse(&format!(
                                    "{scheme}://{}{}",
                                    u.host_header(),
                                    u.target
                                ));
                                assert_eq!(again.as_ref(), Ok(&u), "{s:?} reads back as itself");
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 10_000);
    }

    /// Spec: hub: a chunked body is read to its end, or as far as the limit.
    #[test]
    fn a_chunked_body_is_decoded_up_to_the_limit() {
        let raw = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(raw, 100), (b"hello world".to_vec(), false));
        assert_eq!(decode_chunked(raw, 7), (b"hello w".to_vec(), false));
        assert_eq!(decode_chunked(b"5\r\nhel", 100), (b"hel".to_vec(), true));
    }
}
