//! Which address a referral sends a client to for the world service.
//!
//! A server names one address for its world service ([`crate::NetConfig::world_addr`]): the one
//! players reach it at from elsewhere. A client close by may not reach the server there. A player
//! on the server's own machine logs in over loopback, and many home routers send nothing back in
//! to their own network that is addressed to the router's address on the internet. So a client
//! close by (on this machine, or on a private, link-local or shared network) is referred to an
//! address on its own side instead:
//!
//! - for services listening on one IPv4 address, that address, the only one the client can have
//!   reached them at;
//! - for services listening on every address, this machine's own address on the way to the client
//!   (the host's route lookup), when it is on the same kind of network as the client: loopback for
//!   a client on loopback, the machine's address on a home network for a client on it.
//!
//! Everyone else keeps the named address, and so does a client close by when no address on its
//! side is found (a server in a container whose clients arrive through the host's address
//! translation, for one). Referrals carry IPv4 addresses only: a client on IPv6 keeps the named
//! address too.

use std::net::{IpAddr, Ipv4Addr};

/// The kinds of network a client close by is on: loopback, private, link-local and shared
/// address space, each as its first address and prefix length.
const NEARBY: [(Ipv4Addr, u32); 6] = [
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
];

/// The kind of network close by an address is on, if it is on one.
fn nearby(ip: Ipv4Addr) -> Option<usize> {
    NEARBY.iter().position(|&(net, len)| {
        let mask = u32::MAX << (32 - len);
        u32::from(ip) & mask == u32::from(net)
    })
}

/// Whether a client at `client` is close by: on this machine, or on a private, link-local or
/// shared network.
#[must_use]
pub fn is_nearby(client: IpAddr) -> bool {
    matches!(client.to_canonical(), IpAddr::V4(v) if nearby(v).is_some())
}

/// The address a client at `client` is referred to, for services listening on `listening` that
/// name `named` for players from elsewhere. `toward` is this machine's own address on the way to
/// a client (asked only for services listening on every address, and only for a client close by).
#[must_use]
pub fn world_ip(
    listening: IpAddr,
    named: Ipv4Addr,
    client: IpAddr,
    toward: impl FnOnce(Ipv4Addr) -> Option<Ipv4Addr>,
) -> Ipv4Addr {
    let IpAddr::V4(client) = client.to_canonical() else {
        return named;
    };
    let Some(kind) = nearby(client) else {
        return named;
    };
    match listening.to_canonical() {
        IpAddr::V4(one) if !one.is_unspecified() => one,
        IpAddr::V6(one) if !one.is_unspecified() => named,
        _ => toward(client)
            .filter(|own| nearby(*own) == Some(kind))
            .unwrap_or(if client.is_loopback() {
                Ipv4Addr::LOCALHOST
            } else {
                named
            }),
    }
}
