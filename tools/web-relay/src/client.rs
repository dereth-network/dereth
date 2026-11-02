//! Who a browser connection is: the address the relay counts it under and logs it by.
//!
//! A connection's own peer address is its client's, unless the peer is a proxy the operator named
//! as trusted (an address or a CIDR range). Behind one, the header the operator names says who the
//! client is: `X-Forwarded-For`, read from the right across the proxies a request crosses (its
//! hops), or Cloudflare's `CF-Connecting-IP`, taken as it is. A forwarded header from any other
//! peer is never read, so a client cannot choose the address it is counted under; and a header
//! from a trusted proxy that does not read is refused, never taken to mean the proxy itself.

use std::net::{IpAddr, Ipv6Addr};
use std::str::FromStr;

/// An address, or a range of them in CIDR form (`173.245.48.0/20`, `2400:cb00::/32`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    base: IpAddr,
    bits: u8,
}

impl Range {
    /// Whether `ip` is in the range. An IPv4 address mapped into IPv6 is the IPv4 address.
    #[must_use]
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.base, canonical(ip)) {
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

impl FromStr for Range {
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

/// Which header names the client behind a trusted proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Header {
    /// `X-Forwarded-For`, from the right across the proxies' hops.
    #[default]
    XForwardedFor,
    /// Cloudflare's `CF-Connecting-IP`.
    CfConnectingIp,
}

impl Header {
    /// The header's name, as a request carries it (any case).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::XForwardedFor => "x-forwarded-for",
            Self::CfConnectingIp => "cf-connecting-ip",
        }
    }
}

impl FromStr for Header {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "x-forwarded-for" => Ok(Self::XForwardedFor),
            "cf-connecting-ip" => Ok(Self::CfConnectingIp),
            _ => Err(format!("{s}: x-forwarded-for or cf-connecting-ip")),
        }
    }
}

/// The proxies the operator trusts to name the client, and how they name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proxies {
    pub trusted: Vec<Range>,
    pub header: Header,
    /// How many trusted proxies a request crosses: one for a proxy of the operator's own, two
    /// for Cloudflare and then it.
    pub hops: usize,
}

impl Default for Proxies {
    fn default() -> Self {
        Self {
            trusted: Vec::new(),
            header: Header::XForwardedFor,
            hops: 1,
        }
    }
}

impl Proxies {
    /// Whether `ip` is a trusted proxy.
    #[must_use]
    pub fn trusts(&self, ip: IpAddr) -> bool {
        self.trusted.iter().any(|r| r.contains(ip))
    }

    /// The client of a connection from `peer` whose request carried `header` (the configured
    /// header's value, its values joined with commas).
    ///
    /// `X-Forwarded-For` is read from the right, one entry for each proxy: the client is the entry
    /// [`Self::hops`] from the right, or the first from the right before it that is not a trusted
    /// proxy; a header with fewer entries gives its leftmost. What a client wrote further left is
    /// never read.
    ///
    /// # Errors
    /// A header from a trusted proxy that does not read as addresses.
    pub fn client(&self, peer: IpAddr, header: Option<&str>) -> Result<IpAddr, String> {
        let peer = canonical(peer);
        if !self.trusts(peer) {
            return Ok(peer);
        }
        let Some(value) = header else {
            return Ok(peer);
        };
        let unreadable = || format!("{}: not an address", self.header.name());
        match self.header {
            Header::CfConnectingIp => value
                .trim()
                .parse()
                .map(canonical)
                .map_err(|_| unreadable()),
            Header::XForwardedFor => {
                let list = value
                    .split(',')
                    .map(|h| h.trim().parse::<IpAddr>().map(canonical))
                    .collect::<Result<Vec<IpAddr>, _>>()
                    .map_err(|_| unreadable())?;
                for (i, h) in list.iter().rev().enumerate() {
                    if i + 1 == self.hops.max(1) || !self.trusts(*h) {
                        return Ok(*h);
                    }
                }
                list.first().copied().ok_or_else(unreadable)
            }
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

/// The address a client is counted under: an IPv4 address as it is, an IPv6 one by its /64, the
/// block one household or one machine is usually given.
#[must_use]
pub fn counted_as(ip: IpAddr) -> IpAddr {
    match canonical(ip) {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !u128::from(u64::MAX))),
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address")
    }

    fn behind(trusted: &[&str], header: Header, hops: usize) -> Proxies {
        Proxies {
            trusted: trusted
                .iter()
                .map(|r| r.parse().expect("a range"))
                .collect(),
            header,
            hops,
        }
    }

    #[test]
    fn a_trusted_proxy_is_an_address_or_a_cidr_range() {
        let r: Range = "173.245.48.0/20".parse().expect("a range");
        assert!(r.contains(ip("173.245.48.1")) && r.contains(ip("173.245.63.255")));
        assert!(!r.contains(ip("173.245.64.0")));
        assert!(r.contains(ip("::ffff:173.245.50.1")), "mapped is IPv4");
        let r: Range = "2400:cb00::/32".parse().expect("a range");
        assert!(r.contains(ip("2400:cb00:1::1")) && !r.contains(ip("2400:cb01::1")));
        for bad in ["10.0.0.0/33", "::/129", "example.org", "10.0.0/8"] {
            assert!(bad.parse::<Range>().is_err(), "{bad}");
        }
    }

    /// A forwarded header from a peer that is not a trusted proxy is never read: the client is
    /// the peer, whatever the header says.
    #[test]
    fn a_spoofed_forwarded_header_from_an_untrusted_peer_is_ignored() {
        for header in [Header::XForwardedFor, Header::CfConnectingIp] {
            let p = behind(&["10.0.0.1"], header, 1);
            assert_eq!(
                p.client(ip("198.51.100.9"), Some("203.0.113.5")),
                Ok(ip("198.51.100.9"))
            );
            assert_eq!(
                p.client(ip("198.51.100.9"), Some("not an address")),
                Ok(ip("198.51.100.9"))
            );
            let none = behind(&[], header, 1);
            assert_eq!(
                none.client(ip("127.0.0.1"), Some("203.0.113.5")),
                Ok(ip("127.0.0.1")),
                "no proxy trusted: loopback is no exception"
            );
        }
    }

    #[test]
    fn x_forwarded_for_is_read_from_the_right_one_entry_for_each_proxy() {
        let p = behind(&["127.0.0.1", "173.245.48.0/20"], Header::XForwardedFor, 2);
        let peer = ip("127.0.0.1");
        assert_eq!(
            p.client(peer, Some("192.0.2.66, 203.0.113.5, 173.245.48.7")),
            Ok(ip("203.0.113.5")),
            "the client wrote 192.0.2.66 itself"
        );
        assert_eq!(p.client(peer, Some("203.0.113.5")), Ok(ip("203.0.113.5")));
        assert_eq!(p.client(peer, None), Ok(peer));
        let one = behind(&["127.0.0.1"], Header::XForwardedFor, 1);
        assert_eq!(
            one.client(peer, Some("203.0.113.5, 198.51.100.7")),
            Ok(ip("198.51.100.7")),
            "one hop: the rightmost"
        );
        assert!(one.client(peer, Some("203.0.113.5, nonsense")).is_err());
        assert!(one.client(peer, Some("")).is_err());
    }

    #[test]
    fn cf_connecting_ip_from_a_trusted_proxy_is_the_client() {
        let p = behind(&["127.0.0.1"], Header::CfConnectingIp, 1);
        assert_eq!(
            p.client(ip("127.0.0.1"), Some(" 203.0.113.5 ")),
            Ok(ip("203.0.113.5"))
        );
        assert!(p
            .client(ip("127.0.0.1"), Some("203.0.113.5, 192.0.2.1"))
            .is_err());
        assert_eq!(
            "CF-Connecting-IP".parse::<Header>(),
            Ok(Header::CfConnectingIp)
        );
    }

    #[test]
    fn an_ipv6_client_is_counted_by_its_64() {
        assert_eq!(
            counted_as(ip("2001:db8:1:2:aaaa::1")),
            counted_as(ip("2001:db8:1:2:bbbb::9"))
        );
        assert_ne!(
            counted_as(ip("2001:db8:1:2::1")),
            counted_as(ip("2001:db8:1:3::1"))
        );
        assert_eq!(counted_as(ip("::ffff:192.0.2.1")), ip("192.0.2.1"));
    }
}
