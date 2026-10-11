//! A request's client address behind the proxies the hub's configuration trusts.
//!
//! A connection's own address is its client's unless it is a trusted proxy (an address or a CIDR
//! range the configuration names). Behind one, the header the configuration names says who the
//! client is: `X-Forwarded-For`, read from the right across at most the configured number of
//! proxies (its hops), or Cloudflare's `CF-Connecting-IP`, believed as it is. Without a trusted
//! proxy no forwarded header is read, so a client cannot choose the address the hub's limits count
//! it under; and a header that does not read is refused, never taken to mean the proxy itself.

use std::net::IpAddr;
use std::str::FromStr;

/// An address, or a range of them in CIDR form (`173.245.48.0/20`, `2400:cb00::/32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyRange {
    base: IpAddr,
    bits: u8,
}

impl ProxyRange {
    /// Whether `ip` is in the range. An IPv4 address mapped into IPv6 is the IPv4 address.
    #[must_use]
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical(ip);
        match (self.base, ip) {
            (IpAddr::V4(b), IpAddr::V4(a)) => {
                let mask = u32::MAX.checked_shl(32 - u32::from(self.bits)).unwrap_or(0);
                u32::from(a) & mask == u32::from(b) & mask
            }
            (IpAddr::V6(b), IpAddr::V6(a)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.bits))
                    .unwrap_or(0);
                u128::from(a) & mask == u128::from(b) & mask
            }
            _ => false,
        }
    }
}

impl FromStr for ProxyRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (addr, bits) = match s.split_once('/') {
            Some((a, b)) => (a, Some(b)),
            None => (s, None),
        };
        let base = canonical(
            addr.parse::<IpAddr>()
                .map_err(|_| format!("{s}: an address or a CIDR range"))?,
        );
        let most = if base.is_ipv4() { 32 } else { 128 };
        let bits = match bits {
            None => most,
            Some(b) => b
                .parse::<u8>()
                .ok()
                .filter(|b| *b <= most)
                .ok_or_else(|| format!("{s}: a prefix length of at most {most}"))?,
        };
        Ok(Self { base, bits })
    }
}

impl std::fmt::Display for ProxyRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.base, self.bits)
    }
}

/// The ranges in a list: one address or CIDR range to a line; `#` starts a comment, and blank
/// lines are skipped (the format of Cloudflare's published lists, which may be used as they are).
///
/// # Errors
/// The first line that is not a range, with its number.
pub fn read_ranges(text: &str) -> Result<Vec<ProxyRange>, String> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        out.push(line.parse().map_err(|e| format!("line {}: {e}", n + 1))?);
    }
    Ok(out)
}

/// Which header names the client behind a trusted proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientHeader {
    /// `X-Forwarded-For`, from the right across the proxies' hops.
    #[default]
    XForwardedFor,
    /// Cloudflare's `CF-Connecting-IP`.
    CfConnectingIp,
}

impl FromStr for ClientHeader {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "x-forwarded-for" => Ok(Self::XForwardedFor),
            "cf-connecting-ip" => Ok(Self::CfConnectingIp),
            _ => Err(format!("{s}: x-forwarded-for or cf-connecting-ip")),
        }
    }
}

/// An IPv4 address mapped into IPv6 as the IPv4 address; any other as it is.
#[must_use]
pub fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// Whether `ip` is one of `proxies`.
#[must_use]
pub fn is_trusted(proxies: &[ProxyRange], ip: IpAddr) -> bool {
    proxies.iter().any(|p| p.contains(ip))
}

/// A forwarded header that does not read as addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// The client address of a request that came from `peer` with `header`'s value (the named header,
/// all its values joined with commas), under `proxies`, of which a request crosses `hops` (one for
/// the hub's own reverse proxy; two for Cloudflare and then it).
///
/// `X-Forwarded-For` is read from the right, one entry for each proxy: the rightmost was written
/// by the last proxy, about the one before it, and so on. The client is the entry `hops` from the
/// right, or the first from the right before it that is not a trusted proxy (a request that came
/// round one); a header with fewer entries gives its leftmost. Whatever a client wrote further
/// left, a Worker's or anyone's, is never read.
///
/// # Errors
/// [`Unreadable`] for a header from a trusted proxy that does not read as addresses: the request
/// is refused, never taken to come from the proxy itself.
pub fn client_address(
    peer: IpAddr,
    proxies: &[ProxyRange],
    which: ClientHeader,
    hops: usize,
    header: Option<&str>,
) -> Result<IpAddr, Unreadable> {
    let peer = canonical(peer);
    if !is_trusted(proxies, peer) {
        return Ok(peer);
    }
    let Some(value) = header else {
        return Ok(peer);
    };
    match which {
        ClientHeader::CfConnectingIp => value.trim().parse().map(canonical).map_err(|_| Unreadable),
        ClientHeader::XForwardedFor => {
            let list = value
                .split(',')
                .map(|h| h.trim().parse::<IpAddr>().map(canonical))
                .collect::<Result<Vec<IpAddr>, _>>()
                .map_err(|_| Unreadable)?;
            let hops = hops.max(1);
            for (i, h) in list.iter().rev().enumerate() {
                if i + 1 == hops || !is_trusted(proxies, *h) {
                    return Ok(*h);
                }
            }
            list.first().copied().ok_or(Unreadable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address")
    }

    fn ranges(list: &[&str]) -> Vec<ProxyRange> {
        list.iter().map(|r| r.parse().expect("a range")).collect()
    }

    /// Spec: hub: a trusted proxy is an address or a CIDR range, in either family.
    #[test]
    fn a_trusted_proxy_is_an_address_or_a_cidr_range() {
        let r: ProxyRange = "173.245.48.0/20".parse().expect("a range");
        assert!(r.contains(ip("173.245.48.1")) && r.contains(ip("173.245.63.255")));
        assert!(!r.contains(ip("173.245.64.0")) && !r.contains(ip("173.245.47.255")));
        assert!(
            r.contains(ip("::ffff:173.245.50.1")),
            "a mapped address is its IPv4 one"
        );
        let r: ProxyRange = "2400:cb00::/32".parse().expect("a range");
        assert!(r.contains(ip("2400:cb00:1::1")) && !r.contains(ip("2400:cb01::1")));
        let one: ProxyRange = "127.0.0.1".parse().expect("an address");
        assert!(one.contains(ip("127.0.0.1")) && !one.contains(ip("127.0.0.2")));
        let all: ProxyRange = "0.0.0.0/0".parse().expect("a range");
        assert!(all.contains(ip("8.8.8.8")) && !all.contains(ip("::1")));
        for bad in [
            "10.0.0.0/33",
            "::/129",
            "example.org",
            "10.0.0/8",
            "10.0.0.0/x",
        ] {
            assert!(bad.parse::<ProxyRange>().is_err(), "{bad}");
        }
    }

    /// Spec: hub: trusted proxies may be listed one range to a line, as Cloudflare publishes them.
    #[test]
    fn a_list_of_trusted_proxies_reads_one_range_to_a_line() {
        let list = "# Cloudflare\n173.245.48.0/20\n\n2400:cb00::/32  # v6\n127.0.0.1\n";
        let r = read_ranges(list).expect("a list");
        assert_eq!(r.len(), 3);
        assert!(is_trusted(&r, ip("173.245.50.1")) && is_trusted(&r, ip("2400:cb00::9")));
        assert_eq!(
            read_ranges("10.0.0.0/8\nnot a range\n"),
            Err("line 2: not a range: an address or a CIDR range".into())
        );
    }

    /// Spec: hub: without a trusted proxy, forwarded headers are ignored.
    #[test]
    fn a_forwarded_header_from_an_untrusted_peer_is_ignored() {
        let none = ranges(&[]);
        for which in [ClientHeader::XForwardedFor, ClientHeader::CfConnectingIp] {
            assert_eq!(
                client_address(ip("198.51.100.9"), &none, which, 1, Some("203.0.113.5")),
                Ok(ip("198.51.100.9"))
            );
        }
        let proxy = ranges(&["127.0.0.1"]);
        assert_eq!(
            client_address(
                ip("198.51.100.9"),
                &proxy,
                ClientHeader::XForwardedFor,
                1,
                Some("not an address")
            ),
            Ok(ip("198.51.100.9")),
            "only the trusted proxy's header is read"
        );
    }

    /// Spec: hub: `X-Forwarded-For` is read from the right, one entry for each proxy a request
    /// crosses, so what a client writes in it is never taken for its address.
    #[test]
    fn x_forwarded_for_is_read_from_the_right_one_entry_for_each_proxy() {
        // Cloudflare's edge, then the hub's own reverse proxy on loopback: two hops.
        let proxies = ranges(&["127.0.0.1", "173.245.48.0/20"]);
        let xff = ClientHeader::XForwardedFor;
        let peer = ip("127.0.0.1");
        assert_eq!(
            client_address(
                peer,
                &proxies,
                xff,
                2,
                Some("192.0.2.66, 203.0.113.5, 173.245.48.7")
            ),
            Ok(ip("203.0.113.5")),
            "the client wrote 192.0.2.66 itself; the edge appended the real one"
        );
        assert_eq!(
            client_address(peer, &proxies, xff, 2, Some("203.0.113.5")),
            Ok(ip("203.0.113.5"))
        );
        assert_eq!(
            client_address(peer, &proxies, xff, 2, Some("173.245.48.9, 127.0.0.1")),
            Ok(ip("173.245.48.9")),
            "two hops, both proxies: the second from the right"
        );
        assert_eq!(
            client_address(peer, &proxies, xff, 1, Some("203.0.113.5, 173.245.48.7")),
            Ok(ip("173.245.48.7")),
            "one hop: the rightmost, whatever it is"
        );
        assert_eq!(client_address(peer, &proxies, xff, 2, None), Ok(peer));
        assert_eq!(
            client_address(
                ip("::ffff:127.0.0.1"),
                &proxies,
                xff,
                1,
                Some("2001:4860::8888")
            ),
            Ok(ip("2001:4860::8888"))
        );
    }

    /// Spec: hub: a Worker that writes its own `X-Forwarded-For` before it reaches the edge is
    /// counted by the address it reaches the edge from, never by what it wrote.
    #[test]
    fn a_workers_own_forwarded_header_is_never_its_address() {
        let proxies = ranges(&["127.0.0.1", "173.245.48.0/20"]);
        let peer = ip("127.0.0.1");
        // The Worker wrote 198.51.100.66; it reached the edge from Cloudflare's own 173.245.48.20,
        // which the edge appended; the hub's proxy appended the edge.
        let header = Some("198.51.100.66, 173.245.48.20, 173.245.48.7");
        assert_eq!(
            client_address(peer, &proxies, ClientHeader::XForwardedFor, 2, header),
            Ok(ip("173.245.48.20"))
        );
        // A request that came round the edge to the proxy: the proxy's entry is the client.
        assert_eq!(
            client_address(
                peer,
                &proxies,
                ClientHeader::XForwardedFor,
                2,
                Some("173.245.48.9, 198.51.100.77")
            ),
            Ok(ip("198.51.100.77"))
        );
    }

    /// Spec: hub: a forwarded header that does not read is refused, never taken to mean the proxy.
    #[test]
    fn an_unreadable_forwarded_header_is_refused() {
        let proxies = ranges(&["127.0.0.1"]);
        let peer = ip("127.0.0.1");
        for (which, value) in [
            (ClientHeader::XForwardedFor, "203.0.113.5, not-an-address"),
            (ClientHeader::XForwardedFor, ""),
            (ClientHeader::CfConnectingIp, "203.0.113.5, 192.0.2.1"),
            (ClientHeader::CfConnectingIp, "nonsense"),
        ] {
            assert_eq!(
                client_address(peer, &proxies, which, 1, Some(value)),
                Err(Unreadable),
                "{value:?}"
            );
        }
    }

    /// Spec: hub: `CF-Connecting-IP` from a trusted proxy is the client's one address.
    #[test]
    fn cf_connecting_ip_from_a_trusted_proxy_is_the_client() {
        let proxies = ranges(&["10.0.0.0/8"]);
        let cf = ClientHeader::CfConnectingIp;
        assert_eq!(
            client_address(ip("10.1.2.3"), &proxies, cf, 1, Some(" 203.0.113.5 ")),
            Ok(ip("203.0.113.5"))
        );
        assert_eq!("CF-Connecting-IP".parse::<ClientHeader>(), Ok(cf));
        assert!("forwarded".parse::<ClientHeader>().is_err());
    }
}
