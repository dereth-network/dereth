//! The address the login refers clients to for the world (`--advertise`).
//!
//! A server listening on one IPv4 address refers clients to it. One listening on every address
//! (`--bind 0.0.0.0`) cannot tell which of them a client reached it by: a datagram's destination
//! address is not something the standard sockets report, and behind a router that forwards the
//! ports it would be the machine's own private address anyway, not the one players reach. So such
//! a server, or one listening on IPv6, needs `--advertise`, and will not start without it.
//! Referrals carry an IPv4 address only.

use std::net::{IpAddr, Ipv4Addr};

/// The address clients are referred to: `--advertise`, else the listening address when it is one
/// IPv4 address.
///
/// # Errors
/// What the operator must give instead: an `--advertise` that is no address a client can reach,
/// or none where the listening address does not say one.
pub fn advertised(bind: IpAddr, advertise: Option<Ipv4Addr>) -> Result<Ipv4Addr, String> {
    match (advertise, bind) {
        (Some(a), _) if a.is_unspecified() || a.is_broadcast() || a.is_multicast() => Err(format!(
            "--advertise {a}: the address clients reach this machine at, such as 203.0.113.10"
        )),
        (Some(a), _) => Ok(a),
        (None, IpAddr::V4(v)) if !v.is_unspecified() => Ok(v),
        (None, IpAddr::V4(v)) => Err(format!(
            "--bind {v} listens on every address, so give --advertise with the IPv4 address \
             players reach this machine at: the login sends each client there for the world"
        )),
        (None, IpAddr::V6(v)) => Err(format!(
            "--bind {v}: the login sends clients to the world by an IPv4 address, so give \
             --advertise with the IPv4 address players reach this machine at"
        )),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the executable's option checking: which address a referral names)
    use super::*;

    #[test]
    fn one_ipv4_address_listened_on_is_the_address_clients_are_referred_to() {
        let lan = Ipv4Addr::new(192, 168, 1, 20);
        assert_eq!(advertised(IpAddr::V4(lan), None), Ok(lan));
        assert_eq!(
            advertised(IpAddr::V4(Ipv4Addr::LOCALHOST), None),
            Ok(Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn advertise_wins_over_the_address_listened_on() {
        let public = Ipv4Addr::new(203, 0, 113, 10);
        assert_eq!(
            advertised(IpAddr::V4(Ipv4Addr::UNSPECIFIED), Some(public)),
            Ok(public)
        );
        assert_eq!(
            advertised(IpAddr::V4(Ipv4Addr::LOCALHOST), Some(public)),
            Ok(public)
        );
    }

    #[test]
    fn listening_on_every_address_or_on_ipv6_without_advertise_is_refused() {
        let every = advertised(IpAddr::V4(Ipv4Addr::UNSPECIFIED), None);
        assert!(
            every.as_ref().is_err_and(|e| e.contains("--advertise")),
            "{every:?}"
        );
        let v6 = advertised("::".parse().expect("an address"), None);
        assert!(
            v6.as_ref().is_err_and(|e| e.contains("--advertise")),
            "{v6:?}"
        );
        let v6_one = advertised("2001:db8::1".parse().expect("an address"), None);
        assert!(v6_one.is_err(), "{v6_one:?}");
    }

    #[test]
    fn an_advertised_address_no_client_can_reach_is_refused() {
        for a in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(224, 0, 0, 1),
        ] {
            let got = advertised(IpAddr::V4(Ipv4Addr::LOCALHOST), Some(a));
            assert!(got.is_err(), "{a}: {got:?}");
        }
    }
}
