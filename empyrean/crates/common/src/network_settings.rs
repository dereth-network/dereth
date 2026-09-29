// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/NetworkSettings.cs
//! `NetworkSettings` (`Config.js` → `Server.Network`).

use serde::{Deserialize, Serialize};

use crate::json;

// ACE: NetworkSettings
/// Listener and session limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    // ACE: NetworkSettings.Host
    /// The address to listen on.
    #[serde(rename = "Host")]
    pub host: String,

    // ACE: NetworkSettings.Port
    /// The first of the two UDP ports (`Port` and `Port + 1`).
    #[serde(rename = "Port", deserialize_with = "json::num_u32")]
    pub port: u32,

    // ACE: NetworkSettings.MaximumAllowedSessions
    /// Must be above 0 for anyone to connect.
    #[serde(rename = "MaximumAllowedSessions", deserialize_with = "json::num_u32")]
    pub maximum_allowed_sessions: u32,

    // ACE: NetworkSettings.DefaultSessionTimeout
    /// Seconds until an idle session is declared dead.
    #[serde(rename = "DefaultSessionTimeout", deserialize_with = "json::num_u32")]
    pub default_session_timeout: u32,

    // ACE: NetworkSettings.MaximumAllowedSessionsPerIPAddress
    /// -1 is unlimited.
    #[serde(
        rename = "MaximumAllowedSessionsPerIPAddress",
        deserialize_with = "json::num_i32"
    )]
    pub maximum_allowed_sessions_per_ip_address: i32,

    // ACE: NetworkSettings.AllowUnlimitedSessionsFromIPAddresses
    /// Addresses exempt from the per-address limit.
    #[serde(rename = "AllowUnlimitedSessionsFromIPAddresses")]
    pub allow_unlimited_sessions_from_ip_addresses: Vec<String>,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 9000,
            maximum_allowed_sessions: 128,
            default_session_timeout: 60,
            maximum_allowed_sessions_per_ip_address: -1,
            allow_unlimited_sessions_from_ip_addresses: Vec::new(),
        }
    }
}
