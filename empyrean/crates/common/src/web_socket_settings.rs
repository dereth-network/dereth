//! Not ACE: the WebSocket endpoint's settings (`empyrean.toml` → `[server.websocket]`).
//!
//! ACE speaks UDP only. A browser cannot, so a server can also accept each game datagram as one
//! WebSocket message (the frame of `docs/networking/05-websocket-frame.md`); everything above the
//! transport is the same session. The endpoint is off unless `enabled` is set.
//!
//! The game protocol is not encrypted (the key stream only keys the checksums, and the login
//! carries the account's password), so the endpoint serves `wss://` from a certificate and key,
//! and plain `ws://` only on loopback or behind a reverse proxy that terminates TLS
//! ([`WebSocketSettings::plain_allowed`]).
//!
//! DIVERGE: an extra section; ACE has no WebSocket endpoint.

use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};

use crate::json;

/// The `[server.websocket]` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSocketSettings {
    /// Whether the endpoint listens at all.
    #[serde(rename = "Enabled")]
    pub enabled: bool,

    /// The address (`ip:port`) to listen on.
    #[serde(rename = "Listen")]
    pub listen: String,

    /// The PEM certificate chain served for `wss://`. Empty: no TLS.
    #[serde(rename = "TlsCertificate")]
    pub tls_certificate: String,

    /// The PEM private key for [`Self::tls_certificate`].
    #[serde(rename = "TlsPrivateKey")]
    pub tls_private_key: String,

    /// A reverse proxy in front terminates TLS, so this listener may speak plain `ws://` on a
    /// non-loopback address.
    #[serde(rename = "BehindTlsProxy")]
    pub behind_tls_proxy: bool,

    /// The proxies whose `X-Forwarded-For` names the client: a connection from any other address
    /// is the client itself, whatever it says.
    #[serde(rename = "TrustedProxies")]
    pub trusted_proxies: Vec<String>,

    /// The page origins (`https://play.example.org`) that may open a connection. Empty refuses
    /// every browser page; a client that sends no `Origin` is not a browser page and is accepted.
    #[serde(rename = "AllowedOrigins")]
    pub allowed_origins: Vec<String>,

    /// WebSocket connections open at once from one client address; -1 is unlimited.
    #[serde(
        rename = "MaximumConnectionsPerIPAddress",
        deserialize_with = "json::num_i32"
    )]
    pub maximum_connections_per_ip_address: i32,

    /// Seconds a connection may go without a message before it is closed (its will is delivered).
    #[serde(rename = "IdleTimeout", deserialize_with = "json::num_u32")]
    pub idle_timeout: u32,

    /// The URL clients reach the endpoint at, reported by the status endpoint. Empty: derived from
    /// `listen` (`wss://` with TLS, else `ws://`); set it when a proxy serves another address.
    #[serde(rename = "PublicUrl")]
    pub public_url: String,
}

impl Default for WebSocketSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: "0.0.0.0:9443".to_owned(),
            tls_certificate: String::new(),
            tls_private_key: String::new(),
            behind_tls_proxy: false,
            trusted_proxies: Vec::new(),
            allowed_origins: Vec::new(),
            maximum_connections_per_ip_address: 4,
            idle_timeout: 60,
            public_url: String::new(),
        }
    }
}

/// Why the endpoint's settings cannot be served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketSettingsError {
    /// `listen` is not an `ip:port`.
    Listen(String),
    /// Only one of the certificate and the key is set.
    HalfTls,
    /// Plain `ws://` on a non-loopback address without `behind_tls_proxy`.
    PlainOnPublicAddress(SocketAddr),
    /// A trusted proxy is not an IP address.
    TrustedProxy(String),
}

impl std::fmt::Display for WebSocketSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Listen(v) => write!(f, "server.websocket.listen = {v:?} is not an ip:port"),
            Self::HalfTls => f.write_str(
                "server.websocket: tls_certificate and tls_private_key must be set together",
            ),
            Self::PlainOnPublicAddress(addr) => write!(
                f,
                "server.websocket would serve plain ws:// on {addr}, which is not loopback: the game \
                 protocol is not encrypted and the login carries the password. Set tls_certificate and \
                 tls_private_key for wss://, or behind_tls_proxy = true if a reverse proxy in front \
                 terminates TLS"
            ),
            Self::TrustedProxy(v) => {
                write!(f, "server.websocket.trusted_proxies: {v:?} is not an IP address")
            }
        }
    }
}

impl std::error::Error for WebSocketSettingsError {}

impl WebSocketSettings {
    /// The listen address.
    ///
    /// # Errors
    /// [`WebSocketSettingsError::Listen`].
    pub fn listen_address(&self) -> Result<SocketAddr, WebSocketSettingsError> {
        self.listen
            .parse()
            .map_err(|_| WebSocketSettingsError::Listen(self.listen.clone()))
    }

    /// Whether TLS is configured (both the certificate and the key).
    ///
    /// # Errors
    /// [`WebSocketSettingsError::HalfTls`] when only one is set.
    pub fn tls(&self) -> Result<bool, WebSocketSettingsError> {
        match (
            self.tls_certificate.is_empty(),
            self.tls_private_key.is_empty(),
        ) {
            (true, true) => Ok(false),
            (false, false) => Ok(true),
            _ => Err(WebSocketSettingsError::HalfTls),
        }
    }

    /// Whether plain `ws://` may be served on `listen`: on loopback, or behind a proxy that
    /// terminates TLS.
    #[must_use]
    pub fn plain_allowed(&self, listen: SocketAddr) -> bool {
        listen.ip().is_loopback() || self.behind_tls_proxy
    }

    /// The trusted proxies' addresses.
    ///
    /// # Errors
    /// [`WebSocketSettingsError::TrustedProxy`] for one that is not an IP address.
    pub fn trusted_proxy_addresses(&self) -> Result<Vec<IpAddr>, WebSocketSettingsError> {
        self.trusted_proxies
            .iter()
            .map(|p| {
                p.parse()
                    .map_err(|_| WebSocketSettingsError::TrustedProxy(p.clone()))
            })
            .collect()
    }

    /// Everything the endpoint needs checked before it starts: the address, TLS set whole or not at
    /// all, a plain listener only where allowed, and the trusted proxies. Returns the listen address
    /// and whether it serves TLS.
    ///
    /// # Errors
    /// The first [`WebSocketSettingsError`] found.
    pub fn validate(&self) -> Result<(SocketAddr, bool), WebSocketSettingsError> {
        let listen = self.listen_address()?;
        let tls = self.tls()?;
        if !tls && !self.plain_allowed(listen) {
            return Err(WebSocketSettingsError::PlainOnPublicAddress(listen));
        }
        self.trusted_proxy_addresses()?;
        Ok((listen, tls))
    }

    /// The URL to report: [`Self::public_url`], else `wss://` or `ws://` and `listen` (an
    /// unspecified address reported as the loopback one, since nothing better is known).
    #[must_use]
    pub fn url(&self, listen: SocketAddr, tls: bool) -> String {
        if !self.public_url.is_empty() {
            return self.public_url.clone();
        }
        let mut shown = listen;
        if shown.ip().is_unspecified() {
            shown.set_ip(match shown.ip() {
                IpAddr::V4(_) => IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                IpAddr::V6(_) => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            });
        }
        format!("{}://{shown}/", if tls { "wss" } else { "ws" })
    }
}
