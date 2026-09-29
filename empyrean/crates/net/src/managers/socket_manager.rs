// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Managers/SocketManager.cs
//
// The socket work itself (binding and receiving) is the UDP driver's (`crate::driver::udp`); what
// is here is ACE's configuration logic around it.

use std::net::{IpAddr, Ipv4Addr};

use crate::PortKind;

// ACE: SocketManager.Initialize
/// The host list: `Config.Server.Network.Host` split on `,`, each parsed as an address. Any
/// failure falls back to `IPAddress.Any` alone ("Using IPAddress.Any as host instead."). Each
/// host then gets a listener on `Port` and one on `Port + 1`.
#[must_use]
pub fn parse_hosts(host: &str) -> Vec<IpAddr> {
    let parsed: Result<Vec<IpAddr>, _> = host.split(',').map(str::parse::<IpAddr>).collect();
    match parsed {
        Ok(hosts) => hosts,
        Err(e) => {
            log::error!("Unable to use {host} as host due to: {e}");
            log::error!("Using IPAddress.Any as host instead.");
            vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)]
        }
    }
}

// ACE: SocketManager.GetMatchedConnectionListener
/// The other listener of the same host's pair: C2S gives S2C and S2C gives C2S.
#[must_use]
pub const fn get_matched_connection_listener(listener: PortKind) -> PortKind {
    match listener {
        PortKind::C2S => PortKind::S2C,
        PortKind::S2C => PortKind::C2S,
    }
}
