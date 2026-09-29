// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Packets/PacketInboundLoginRequest.cs

use empyrean_common::dotnet::BinaryReader;

use crate::enums::NetAuthType;

/// ACE `PacketInboundLoginRequest`, read from the `LoginRequest` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketInboundLoginRequest {
    pub net_auth_type: NetAuthType,
    /// ACE `Timestamp`: the authenticator's connection sequence number.
    pub timestamp: u32,
    pub account: String,
    pub password: Option<String>,
    pub gls_ticket: Option<String>,
    /// Should be `"1802"`, the end-of-retail client.
    pub client_version: String,
}

impl PacketInboundLoginRequest {
    // ACE: PacketInboundLoginRequest.PacketInboundLoginRequest
    /// `None` where ACE's reads throw (`EndOfStreamException` off the end, or `ReadChars`'
    /// `ArgumentException`), which `HandleLoginRequest` catches and logs. The reads are the shared
    /// `empyrean_common::dotnet::BinaryReader` and its ACE.Common `ReadString16L`/`ReadString32L`.
    ///
    /// Not ACE's (retail's): the account to log in as is read only when
    /// `AuthFlags & 2`, as the client writes it, and the crypto data after it is read as its
    /// length word and that many bytes (the retail client's is always empty). ACE read the account
    /// to log in as unconditionally, which with the flag clear consumed the empty crypto data's
    /// length word as an empty string, and with it set left the password misread.
    #[must_use]
    pub fn new(section: &[u8]) -> Option<Self> {
        let mut r = BinaryReader::new(section);
        let client_version = r.read_string16l().ok()?;
        let _len = r.read_u32().ok()?;
        let net_auth_type = NetAuthType::from_u32(r.read_u32().ok()?);
        let auth_flags = r.read_u32().ok()?;
        let timestamp = r.read_u32().ok()?;
        let account = r.read_string16l().ok()?;
        if auth_flags & 2 != 0 {
            let _account_to_login_as = r.read_string16l().ok()?;
        }
        let crypto_len = r.read_u32().ok()?;
        r.skip(usize::try_from(crypto_len).ok()?);
        let (mut password, mut gls_ticket) = (None, None);
        if net_auth_type == NetAuthType::AccountPassword {
            password = Some(r.read_string32l().ok()?);
        } else if net_auth_type == NetAuthType::GlsTicket {
            gls_ticket = Some(r.read_string32l().ok()?);
        }
        Some(Self {
            net_auth_type,
            timestamp,
            account,
            password,
            gls_ticket,
            client_version,
        })
    }
}
