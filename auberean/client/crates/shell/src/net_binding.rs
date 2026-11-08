//! Where the client's game socket stands: on any interface, or the one the preferences' filter
//! names; on the port the preferences say, or the first free one of a series worked out from the
//! interface, so several clients behind one router keep steady ports that differ.

use std::net::Ipv4Addr;

use crate::preferences::{key, section, Preferences};

/// The series of ports is worked out by this.
const UNIQUE_PORT_MODULUS: u32 = 0xFA07;
/// Ports of the series under this are passed over.
const LOWEST_PORT: u32 = 0x400;

/// How the game socket is bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetBinding {
    /// An interface's network and mask.
    pub filter: Option<(u32, u32)>,
    /// Take the first free port of the series; else `port`.
    pub compute_unique: bool,
    /// The port named (0: the system picks).
    pub port: u16,
}

impl Default for NetBinding {
    fn default() -> Self {
        Self {
            filter: None,
            compute_unique: true,
            port: 0,
        }
    }
}

/// `A.B.C.D/M`, the mask dotted or a count of bits up to 32: its network and mask.
#[must_use]
pub fn parse_interface(text: &str) -> Option<(u32, u32)> {
    let (network, mask) = text.split_once('/')?;
    let network = u32::from(network.parse::<Ipv4Addr>().ok()?);
    let mask = if mask.contains('.') {
        u32::from(mask.parse::<Ipv4Addr>().ok()?)
    } else {
        let bits: u32 = mask.parse().ok()?;
        match bits {
            0 => 0,
            1..=32 => u32::MAX << (32 - bits),
            _ => return None,
        }
    };
    Some((network, mask))
}

impl NetBinding {
    /// Read as a connection starts: the filter, whether to work the port out (it does unless told
    /// not to), and the port the line named, else the preferences'.
    #[must_use]
    pub fn read(prefs: &Preferences, line_port: i32) -> Self {
        let port = u16::try_from(line_port)
            .ok()
            .filter(|p| *p != 0)
            .or_else(|| {
                prefs
                    .inq_uint32(section::NET, key::USER_SPECIFIED_PORT)
                    .and_then(|p| u16::try_from(p).ok())
            })
            .unwrap_or(0);
        Self {
            filter: prefs
                .inq_string(section::NET, key::BIND_INTERFACE)
                .and_then(|t| parse_interface(&t)),
            compute_unique: prefs
                .inq_bool(section::NET, key::COMPUTE_UNIQUE_PORT)
                .unwrap_or(true),
            port,
        }
    }

    /// The address to bind, `here` being the interface toward the server: `here` when the filter
    /// takes it, else any.
    #[must_use]
    pub fn address(&self, here: Ipv4Addr) -> Ipv4Addr {
        match self.filter {
            Some((network, mask)) if u32::from(here) & mask == network & mask => here,
            _ => Ipv4Addr::UNSPECIFIED,
        }
    }

    /// The ports to try, in order, `here` being the interface the series is worked out from: the
    /// one port named, or the series, 0x100 at a time from past its start, until it comes round.
    #[must_use]
    pub fn ports(&self, here: Ipv4Addr) -> Vec<u16> {
        if !self.compute_unique {
            return vec![self.port];
        }
        let start = u32::from(here) % UNIQUE_PORT_MODULUS;
        let mut out = Vec::new();
        let mut p = (start + 0x100) & 0xFFFF;
        while p != start {
            if p >= LOWEST_PORT {
                out.extend(u16::try_from(p));
            }
            p = (p + 0x100) & 0xFFFF;
        }
        out
    }

    /// What the first port `bind` takes gives.
    ///
    /// # Errors
    /// The last refusal, or `none` when there was no port to try.
    pub fn bind<T, E>(
        &self,
        here: Ipv4Addr,
        none: E,
        mut bind: impl FnMut(u16) -> Result<T, E>,
    ) -> Result<T, E> {
        let mut last = none;
        for port in self.ports(here) {
            match bind(port) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(last)
    }
}
