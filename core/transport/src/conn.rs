//! The connection state machine: the three-way handshake, the port + 1 rule, referrals, server
//! switching, every disconnect path and the 21 `NetError` codes.
//!
//! ```text
//! client -> server:P     LoginRequest       ("1802" + ConnectionAuthenticator)   seq 0, plaintext
//! server -> client       ConnectRequest     (cookie, NetID, two seeds)    seq 0, plaintext
//! client -> server:P+1   ConnectResponse    (the cookie, echoed)          seq 0, plaintext
//! server -> client       anything else                                    -> Connected
//! ```
//!
//! **The session keys arrive in clear.** The client's Diffie-Hellman and key-exchange objects are
//! constructed for every connection and never packed onto the wire; there is no key exchange to
//! implement, and implementing one is a deviation.
//!
//! See `docs/networking/03-connection-state-machine.md`.

use dereth_primitives::LocalTime;

use crate::wire::PacketFlags;

/// `ConnectionState`, stored in both receive and recipient data and kept in sync by
/// the connection-state setter.
///
/// States 1-3 exist because `oldnet` is the shared client/server library. **The client only ever
/// occupies 0, 4, 5, 6 and 7**; a rebuild that reaches 1, 2 or 3 has invented a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u32)]
pub enum ConnectionState {
    /// Initial value; connection teardown also returns the receiver to this state.
    #[default]
    Disconnected = 0,
    /// Never assigned in the client — a server-side state.
    AwaitingWorldAuth = 1,
    /// Never assigned in the client.
    AuthSent = 2,
    /// Never assigned in the client.
    ConnectionRequestSent = 3,
    /// The connection-request handler, after it has answered with
    /// `ConnectResponse`.
    ConnectionRequestAcked = 4,
    /// The packet processor, on the first packet that is **not** a
    /// `ConnectRequest`.
    Connected = 5,
    /// A `NetErrorDisconnect` arrived, or the 140 s no-data timeout fired.
    DisconnectReceived = 6,
    /// Our own `NetErrorDisconnect` went out.
    DisconnectSent = 7,
}

impl ConnectionState {
    /// The five the client actually occupies.
    pub const CLIENT_REACHABLE: [ConnectionState; 5] = [
        ConnectionState::Disconnected,
        ConnectionState::ConnectionRequestAcked,
        ConnectionState::Connected,
        ConnectionState::DisconnectReceived,
        ConnectionState::DisconnectSent,
    ];

    /// The flow queue's enqueue refuses to queue a blob at or past
    /// [`ConnectionState::DisconnectReceived`].
    #[must_use]
    pub fn accepts_blobs(self) -> bool {
        self < ConnectionState::DisconnectReceived
    }
}

/// `Protocol` — the transport tag inside the packet record. The client always sets `fe_udp`; the
/// other two exist because the network layer is shared with Turbine's server, which used TCP
/// between back-end processes. **There is no TCP anywhere in the retail client.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Protocol {
    FeTcp = 0,
    BeTcp = 1,
    FeUdp = 2,
}

/// These exact protocol bytes are hashed for the insufficient-privilege error.
/// Correcting their spelling would change the wire identifier.
pub const WIRE_INSUFFICIENT_PRIVILEGE_ID: &str = "ID_ConnectionError_InsufficientPriveledge";

/// One of the 21 `NetError` codes.
///
/// The wire form is `{u32 string_id; i32 table_id}`, 8 bytes, with
/// `string_id` the string hash of `"ID_..."` and `table_id` always 8. This crate's own error type
/// is `dereth_client_net::NetError`; this one is the protocol's, and both keep the client's name because the
/// knowledge base indexes them that way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetErrorCode {
    /// The "no error" sentinel — it has no table entry.
    None,
    BadServerAddress,
    CantBind,
    CantSocket,
    CantCrypto,
    AbortedHandshake,
    RunningSpeedhack,
    NoLogonServer,
    NetVersionMismatch,
    ServerFull,
    BadCryptoKey,
    InsufficientPrivilege,
    SecondLogon,
    ServerClosedConnection,
    ServerTimedOutClient,
    ClientTimedOutServer,
    PlayerAlreadyLoggedOn,
    ClientLogOnFailed,
    AccountAuthenticationFailed,
    LogonServerMigrated,
    Generic,
}

/// The table id — always 8, the connection-error string table.
pub const NET_ERROR_TABLE_ID: i32 = 8;

impl NetErrorCode {
    /// All 21, in the client's order.
    pub const ALL: [NetErrorCode; 21] = [
        NetErrorCode::None,
        NetErrorCode::BadServerAddress,
        NetErrorCode::CantBind,
        NetErrorCode::CantSocket,
        NetErrorCode::CantCrypto,
        NetErrorCode::AbortedHandshake,
        NetErrorCode::RunningSpeedhack,
        NetErrorCode::NoLogonServer,
        NetErrorCode::NetVersionMismatch,
        NetErrorCode::ServerFull,
        NetErrorCode::BadCryptoKey,
        NetErrorCode::InsufficientPrivilege,
        NetErrorCode::SecondLogon,
        NetErrorCode::ServerClosedConnection,
        NetErrorCode::ServerTimedOutClient,
        NetErrorCode::ClientTimedOutServer,
        NetErrorCode::PlayerAlreadyLoggedOn,
        NetErrorCode::ClientLogOnFailed,
        NetErrorCode::AccountAuthenticationFailed,
        NetErrorCode::LogonServerMigrated,
        NetErrorCode::Generic,
    ];

    /// The protocol string whose exact bytes determine the wire error identifier.
    #[must_use]
    pub fn id_string(self) -> &'static str {
        match self {
            NetErrorCode::None => "ID_NetError_None",
            NetErrorCode::BadServerAddress => "ID_NetError_BadServerAddress",
            NetErrorCode::CantBind => "ID_NetError_CantBind",
            NetErrorCode::CantSocket => "ID_NetError_CantSocket",
            NetErrorCode::CantCrypto => "ID_NetError_CantCrypto",
            NetErrorCode::AbortedHandshake => "ID_NetError_AbortedHandshake",
            NetErrorCode::RunningSpeedhack => "ID_ConnectionError_RunningSpeedhack",
            NetErrorCode::NoLogonServer => "ID_ConnectionError_NoLogonServer",
            NetErrorCode::NetVersionMismatch => "ID_ConnectionError_NetVersionMismatch",
            NetErrorCode::ServerFull => "ID_ConnectionError_ServerFull",
            NetErrorCode::BadCryptoKey => "ID_ConnectionError_BadCryptoKey",
            NetErrorCode::InsufficientPrivilege => WIRE_INSUFFICIENT_PRIVILEGE_ID,
            NetErrorCode::SecondLogon => "ID_ConnectionError_SecondLogon",
            NetErrorCode::ServerClosedConnection => "ID_ConnectionError_ServerClosedConnection",
            NetErrorCode::ServerTimedOutClient => "ID_ConnectionError_ServerTimedOutClient",
            NetErrorCode::ClientTimedOutServer => "ID_ConnectionError_ClientTimedOutServer",
            NetErrorCode::PlayerAlreadyLoggedOn => "ID_ConnectionError_PlayerAlreadyLoggedOn",
            NetErrorCode::ClientLogOnFailed => "ID_ConnectionError_ClientLogOnFailed",
            NetErrorCode::AccountAuthenticationFailed => {
                "ID_ConnectionError_AccountAuthenticationFailed"
            }
            NetErrorCode::LogonServerMigrated => "ID_ConnectionError_LogonServerMigrated",
            NetErrorCode::Generic => "ID_ConnectionError_Generic",
        }
    }

    /// The string id — the hash of [`NetErrorCode::id_string`].
    ///
    /// Computed rather than tabulated, so the hash and the string cannot drift apart. The retail
    /// values are asserted against this in the tests.
    #[must_use]
    pub fn string_id(self) -> u32 {
        dereth_primitives::num::hash::str_hash(self.id_string().as_bytes())
    }

    /// The 8-byte wire form.
    #[must_use]
    pub fn pack(self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..4].copy_from_slice(&self.string_id().to_le_bytes());
        b[4..8].copy_from_slice(&NET_ERROR_TABLE_ID.to_le_bytes());
        b
    }

    /// The error unpack. Both fields are optional if the buffer is short — the client
    /// reads the string id if at least 4 bytes remain and the table id if at least 8 — and a zero in
    /// either field yields the literal `"unknown"`.
    #[must_use]
    pub fn unpack(buf: &[u8]) -> Option<Self> {
        let s = buf.get(..4)?;
        let string_id = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        Self::ALL.into_iter().find(|c| c.string_id() == string_id)
    }
}

/// The client version string the end-of-retail client sends. **ACE compares it exactly.** A world
/// whose server wants another is sent that instead ([`build_login_request_as`]).
pub const CLIENT_VERSION: &str = "1802";

/// `NetAuthType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NetAuthType {
    Undef = 0,
    Account = 1,
    AccountPassword = 2,
    GlsTicket = 0x4000_0002,
}

/// `ConnectionAuthenticator`, as writes it.
///
/// The account-to-logon-as string is present **only** when the auth flags have bit 2, and the
/// retail client never sets that bit. ACE's `PacketInboundLoginRequest` reads the field
/// unconditionally and works by accident: with the flag clear, the four bytes it consumes are the
/// crypto data's zero length word,
/// which `ReadString16L` decodes as an empty string. A client that ever set the bit would
/// desynchronise ACE's parser.
#[derive(Debug, Clone)]
pub struct ConnectionAuthenticator {
    pub auth_type: NetAuthType,
    /// Bit 1 (`0x2`) means an "account to log on as" string follows. Never set by retail.
    pub auth_flags: u32,
    /// Real time at connection establishment; ACE reads it as `Timestamp`.
    pub connection_sequence_number: u32,
    /// Account name, **lower-cased** before serialization.
    pub account: String,
    pub account_to_logon_as: Option<String>,
    /// Always empty in the retail client.
    pub crypto_data: Vec<u8>,
    /// The password or GLS ticket as an archive string: the compressed length and the bytes, with
    /// no padding. The account name above uses the other, packed string form.
    pub extra_data: Vec<u8>,
}

impl ConnectionAuthenticator {
    /// The usual `-a account -v password` form.
    #[must_use]
    pub fn account_password(account: &str, password: &str) -> Self {
        Self {
            auth_type: NetAuthType::AccountPassword,
            auth_flags: 0,
            connection_sequence_number: 0,
            account: account.to_ascii_lowercase(),
            account_to_logon_as: None,
            crypto_data: Vec::new(),
            extra_data: crate::wire::optional::astring_pack(password.as_bytes()),
        }
    }

    /// The authenticator's stream pack.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        use crate::wire::optional::pstring_pack;
        let mut out = Vec::new();
        out.extend_from_slice(&(self.auth_type as u32).to_le_bytes());
        out.extend_from_slice(&self.auth_flags.to_le_bytes());
        out.extend_from_slice(&self.connection_sequence_number.to_le_bytes());
        out.extend_from_slice(&pstring_pack(self.account.as_bytes()));
        if self.auth_flags & 2 != 0 {
            let as_who = self.account_to_logon_as.as_deref().unwrap_or("");
            out.extend_from_slice(&pstring_pack(as_who.as_bytes()));
        }
        out.extend_from_slice(
            &u32::try_from(self.crypto_data.len())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        out.extend_from_slice(&self.crypto_data);
        out.extend_from_slice(
            &u32::try_from(self.extra_data.len())
                .unwrap_or(0)
                .to_le_bytes(),
        );
        out.extend_from_slice(&self.extra_data);
        out
    }

    /// The stream unpack, the inverse of [`Self::pack`]. `None` when the buffer is truncated or
    /// the auth type is not one of the four.
    ///
    /// The receiving half of the shared transport: the client never reads an authenticator, a
    /// server reads one per `LoginRequest`. The account-to-log-on-as string is read only when
    /// bit 2 of the auth flags is set, as the pack writes it. Strings are single-byte: each byte becomes the
    /// `char` of the same value.
    #[must_use]
    pub fn unpack(buf: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        let auth_type = NetAuthType::from_wire(read_u32(buf, &mut at)?)?;
        let auth_flags = read_u32(buf, &mut at)?;
        let connection_sequence_number = read_u32(buf, &mut at)?;
        let account = read_pstring(buf, &mut at)?;
        let account_to_logon_as = if auth_flags & 2 != 0 {
            Some(read_pstring(buf, &mut at)?)
        } else {
            None
        };
        let crypto_data = read_counted(buf, &mut at)?;
        let extra_data = read_counted(buf, &mut at)?;
        Some(Self {
            auth_type,
            auth_flags,
            connection_sequence_number,
            account,
            account_to_logon_as,
            crypto_data,
            extra_data,
        })
    }
}

impl NetAuthType {
    /// Decode the wire value; `None` for anything but the four.
    #[must_use]
    pub const fn from_wire(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Undef),
            1 => Some(Self::Account),
            2 => Some(Self::AccountPassword),
            0x4000_0002 => Some(Self::GlsTicket),
            _ => None,
        }
    }
}

/// The `LoginRequest` optional-header body: `PString ClientVersion`, the `u32` authenticator
/// length, the authenticator, as the logon header builds it.
#[must_use]
pub fn build_login_request(auth: &ConnectionAuthenticator) -> Vec<u8> {
    build_login_request_as(CLIENT_VERSION, auth)
}

/// [`build_login_request`] with another client version string.
#[must_use]
pub fn build_login_request_as(client_version: &str, auth: &ConnectionAuthenticator) -> Vec<u8> {
    let packed = auth.pack();
    let mut out = crate::wire::optional::pstring_pack(client_version.as_bytes());
    out.extend_from_slice(&u32::try_from(packed.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&packed);
    out
}

/// A decoded `LoginRequest` optional-header body, the inverse of [`build_login_request`].
#[derive(Debug, Clone)]
pub struct LoginRequest {
    /// `ClientVersion`; [`CLIENT_VERSION`] from this client.
    pub client_version: String,
    pub auth: ConnectionAuthenticator,
}

impl LoginRequest {
    /// Decode a `LoginRequest` section body: the version string, the authenticator length, and
    /// exactly that many bytes of authenticator. `None` when it is truncated or the authenticator
    /// is malformed.
    #[must_use]
    pub fn parse(body: &[u8]) -> Option<Self> {
        let mut at = 0usize;
        let client_version = read_pstring(body, &mut at)?;
        let cb_auth = read_u32(body, &mut at)? as usize;
        let auth = ConnectionAuthenticator::unpack(body.get(at..at.checked_add(cb_auth)?)?)?;
        Some(Self {
            client_version,
            auth,
        })
    }
}

use crate::wire::le::take_u32 as read_u32;

/// A packed `PString` as single-byte characters.
fn read_pstring(buf: &[u8], at: &mut usize) -> Option<String> {
    let (bytes, used) = crate::wire::optional::pstring_unpack(buf.get(*at..)?)?;
    *at += used;
    Some(bytes.iter().map(|&b| char::from(b)).collect())
}

/// A `u32` count, then that many bytes.
fn read_counted(buf: &[u8], at: &mut usize) -> Option<Vec<u8>> {
    let n = read_u32(buf, at)? as usize;
    let bytes = buf.get(*at..at.checked_add(n)?)?.to_vec();
    *at += n;
    Some(bytes)
}

/// The 32-byte `ConnectRequest` body. Server -> client only.
///
/// The field names are the client's own and are from the **server's** point of view:
/// `OutgoingSeed` is server -> client and keys the client's incoming checksum stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectRequest {
    /// The server's game time. The client feeds it to the game clock on the following TimeSync,
    /// **not** here — which is why ACE comments that it must send a TimeSync early.
    pub server_time: f64,
    /// The cookie the client must echo in the `ConnectResponse`.
    pub cookie: u64,
    /// The id the client must put in the packet header's recipient-id field when talking to this
    /// server.
    pub net_id: u32,
    /// ISAAC seed for **server -> client**.
    pub outgoing_seed: u32,
    /// ISAAC seed for **client -> server**.
    pub incoming_seed: u32,
}

/// The two ISAAC seeds of a connection, named by the direction of the traffic they key rather
/// than by the `ConnectRequest`'s server-relative `OutgoingSeed`/`IncomingSeed`, which read
/// backwards from the client's side and forwards from the server's. Either end can use these
/// names without translating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectSeeds {
    /// Keys the checksums of server -> client packets (`OutgoingSeed`; the client's incoming
    /// stream).
    pub server_to_client: u32,
    /// Keys the checksums of client -> server packets (`IncomingSeed`; the client's outgoing
    /// stream).
    pub client_to_server: u32,
}

impl ConnectRequest {
    /// A `ConnectRequest` with its seeds placed by direction.
    #[must_use]
    pub const fn new(server_time: f64, cookie: u64, net_id: u32, seeds: ConnectSeeds) -> Self {
        Self {
            server_time,
            cookie,
            net_id,
            outgoing_seed: seeds.server_to_client,
            incoming_seed: seeds.client_to_server,
        }
    }

    /// The two seeds, by direction.
    #[must_use]
    pub const fn seeds(&self) -> ConnectSeeds {
        ConnectSeeds {
            server_to_client: self.outgoing_seed,
            client_to_server: self.incoming_seed,
        }
    }

    /// # Errors
    /// [`crate::wire::WireError::SectionTruncated`] if fewer than 32 bytes are supplied.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, crate::wire::WireError> {
        let b = buf
            .get(..32)
            .ok_or(crate::wire::WireError::SectionTruncated {
                mask: PacketFlags::CONNECT_REQUEST,
                need: 32,
                have: buf.len(),
            })?;
        let rd32 = |at: usize| crate::wire::le::read_u32(b, at).expect("field in admitted section");
        let rd64 = |at: usize| u64::from(rd32(at)) | (u64::from(rd32(at + 4)) << 32);
        Ok(Self {
            server_time: f64::from_bits(rd64(0)),
            cookie: rd64(8),
            net_id: rd32(0x10),
            outgoing_seed: rd32(0x14),
            incoming_seed: rd32(0x18),
        })
    }

    /// The 32 bytes, including the trailing 4 bytes of structure alignment that ACE writes as an
    /// explicit `0u` (the struct begins with a `double`).
    #[must_use]
    pub fn to_bytes(self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..8].copy_from_slice(&self.server_time.to_bits().to_le_bytes());
        b[8..16].copy_from_slice(&self.cookie.to_le_bytes());
        b[16..20].copy_from_slice(&self.net_id.to_le_bytes());
        b[20..24].copy_from_slice(&self.outgoing_seed.to_le_bytes());
        b[24..28].copy_from_slice(&self.incoming_seed.to_le_bytes());
        b
    }
}

/// `ServerSwitchType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ServerSwitchType {
    /// Sets both the current server recipient and the shared network id to this packet's recipient id.
    WorldSwitch = 0,
    /// Sets the logon server's recipient id, and the current server's too if the client is not in
    /// the world yet.
    LogonSwitch = 1,
}

impl ServerSwitchType {
    /// The wire value, as the server-switch body's type field carries it.
    ///
    /// The server-switch handler switches on exactly 0 and 1. A third value selects neither
    /// history, leaves the history pointer null and walks into a null dereference -- unreachable
    /// because no sender emits one. Modelled as `None`: the section is ignored.
    #[must_use]
    pub const fn from_wire(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::WorldSwitch),
            1 => Some(Self::LogonSwitch),
            _ => None,
        }
    }
}

/// The 8-byte `ServerSwitch` optional-header body. Server -> client only.
///
/// The client copies two dwords out
/// of the receive buffer, so the wire layout is the struct's:
///
/// ```text
/// +0  u32 seq_no         (timestamp)
/// +4  u32 Type            (ServerSwitchType)
/// ```
///
/// ACE's `PacketOutboundServerSwitch` writes `0x18` then `0`, i.e. it puts its hard-coded server
/// id where the client reads the switch **stamp**. That still works: the client only compares the
/// stamp against the last one it saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerSwitch {
    /// The switch stamp, compared with [`crate::session::lhs_newer`]
    /// against the matching history's [`SwitchHistory::last_switch_stamp`].
    pub seq_no: u32,
    /// Which of the two histories this switch belongs to.
    pub switch_type: ServerSwitchType,
}

impl ServerSwitch {
    /// Decode. `Ok(None)` means a `Type` the client's switch has no arm for.
    ///
    /// # Errors
    /// [`crate::wire::WireError::SectionTruncated`] if fewer than 8 bytes are supplied.
    pub fn from_bytes(buf: &[u8]) -> Result<Option<Self>, crate::wire::WireError> {
        let b = buf
            .get(..8)
            .ok_or(crate::wire::WireError::SectionTruncated {
                mask: PacketFlags::SERVER_SWITCH,
                need: 8,
                have: buf.len(),
            })?;
        let rd32 = |at: usize| crate::wire::le::read_u32(b, at).expect("field in admitted section");
        Ok(
            ServerSwitchType::from_wire(rd32(4)).map(|switch_type| Self {
                seq_no: rd32(0),
                switch_type,
            }),
        )
    }

    /// The 8 bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..4].copy_from_slice(&self.seq_no.to_le_bytes());
        b[4..8].copy_from_slice(&(self.switch_type as u32).to_le_bytes());
        b
    }
}

/// World- and logon-server switch history.
///
/// On a server switch the **first** switch of each kind is accepted whatever its
/// stamp (a history with no earlier switch skips the comparison); after that a switch
/// is accepted only when its stamp is strictly newer under the 32-bit wrapping rule implemented
/// by [`crate::session::lhs_newer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwitchHistory {
    /// Whether a switch of this kind has been accepted before.
    pub been_switched_before: bool,
    /// The stamp of the last accepted switch.
    pub last_switch_stamp: u32,
}

impl SwitchHistory {
    /// Apply one switch stamp; `true` when the switch takes effect.
    ///
    /// A history with no earlier switch accepts at once. Otherwise a stamp equal to the last one is
    /// refused, as is one the wrapping comparison does not rank strictly newer (result below 1).
    /// Acceptance sets the been-switched flag and records the stamp.
    pub fn accept(&mut self, stamp: u32) -> bool {
        if self.been_switched_before && !crate::session::lhs_newer(stamp, self.last_switch_stamp) {
            return false;
        }
        self.been_switched_before = true;
        self.last_switch_stamp = stamp;
        true
    }
}

/// The 32-byte `Referral` optional-header body. Server -> client only.
///
/// The client copies eight dwords verbatim, so the wire layout is the struct's own.
/// The referral handler reads it at these offsets:
///
/// ```text
/// +0   u64          cookie
/// +8   sockaddr_in  Addr          ; four dwords
/// +24  u16          server id
/// +26  u16          padding
/// +28  u32          (alignment: the struct starts with a u64, so sizeof is 32)
/// ```
///
/// ACE's `PacketOutboundReferral` writes exactly that: `ulong key`, `ushort 2` (`AF_INET`),
/// `WriteUInt16BE(port)`, four host bytes, `0ul` (`sin_zero`), `ushort 0x18` (**the server id**),
/// `ushort 0`, `uint 0`. The two oracles agree field for field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Referral {
    /// The cookie -- what the client puts in the `WorldLoginRequest` it sends to `addr`.
    pub cookie: u64,
    /// The referred endpoint. `sin_port` is big-endian on the wire, as `sockaddr_in`
    /// always is.
    pub addr: std::net::SocketAddrV4,
    /// The recipient id the referred server will use when it answers with a
    /// `ConnectRequest`, and the receiver slot this referral is about. Must be < 0x100.
    pub id_server: u16,
    /// The endpoint's `sin_family`, carried so a decode/encode round trip is byte-exact. Always 2.
    pub family: u16,
}

impl Referral {
    /// # Errors
    /// [`crate::wire::WireError::SectionTruncated`] if fewer than 32 bytes are supplied.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, crate::wire::WireError> {
        let b = buf
            .get(..32)
            .ok_or(crate::wire::WireError::SectionTruncated {
                mask: PacketFlags::REFERRAL,
                need: 32,
                have: buf.len(),
            })?;
        let rd32 = |at: usize| crate::wire::le::read_u32(b, at).expect("field in admitted section");
        let rd16 = |at: usize| crate::wire::le::read_u16(b, at).expect("field in admitted section");
        Ok(Self {
            cookie: u64::from(rd32(0)) | (u64::from(rd32(4)) << 32),
            family: rd16(8),
            addr: std::net::SocketAddrV4::new(
                std::net::Ipv4Addr::new(b[12], b[13], b[14], b[15]),
                u16::from_be_bytes([b[10], b[11]]),
            ),
            id_server: rd16(24),
        })
    }

    /// The 32 bytes, in ACE's `PacketOutboundReferral` order. `sin_zero` is written as eight
    /// zeroes, which is what ACE sends and what the client copies into its queue entry.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 32] {
        let mut b = [0u8; 32];
        b[0..8].copy_from_slice(&self.cookie.to_le_bytes());
        b[8..10].copy_from_slice(&self.family.to_le_bytes());
        b[10..12].copy_from_slice(&self.addr.port().to_be_bytes());
        b[12..16].copy_from_slice(&self.addr.ip().octets());
        b[24..26].copy_from_slice(&self.id_server.to_le_bytes());
        b
    }
}

/// The connect-ack send puts the `ConnectResponse` on the server's port **+ 1**,
/// and sends the `CICMDCommand` keep-alive
/// there too. Everything else goes to the port the server replied from.
///
/// The server binds two UDP sockets, `P` and `P+1`. The `LoginRequest` goes to `P` and the server
/// replies from `P`, so the recorded server address is `P`. Sending the `ConnectResponse` to `P+1` tells the
/// server which source port the client transmits from, and it then uses its `P+1` socket for
/// everything afterwards. `verify_header` compares the source **address but not the source port**,
/// which is what makes the asymmetry legal — get either half wrong and the first post-handshake
/// packet is rejected.
#[must_use]
pub fn handshake_port(server_port: u16) -> u16 {
    server_port.wrapping_add(1)
}

/// The command code of a standalone keep-alive/echo command packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IcmdCommand {
    Nop = 1,
    /// `"ERpl"`.
    EchoReply = 0x6C70_5245,
    /// `"ERqe"`.
    EchoRequest = 0x7165_5245,
}

/// The login state machine's own progress.
#[derive(Debug)]
pub struct LoginState {
    /// Login requests sent. `> 0x13` — i.e. more than 20 sent — ends the state with
    /// `ID_ConnectionError_ClientTimedOutServer`.
    pub requests_sent: u32,
    /// When the last `LoginRequest` went out.
    pub last_sent: Option<LocalTime>,
}

impl Default for LoginState {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            requests_sent: 0,
            last_sent: None,
        }
    }

    /// Should a `LoginRequest` go out now?
    ///
    /// Immediately the first time, then every 2.0 s, for at most 20 attempts. Returns `Err` with
    /// the code the client raises when it gives up.
    ///
    /// # Errors
    /// [`NetErrorCode::ClientTimedOutServer`] after 20 unanswered attempts.
    pub fn should_send(&mut self, now: LocalTime) -> Result<bool, NetErrorCode> {
        if self.requests_sent > crate::flow::LOGIN_RESEND_TRIES {
            return Err(NetErrorCode::ClientTimedOutServer);
        }
        let due = match self.last_sent {
            None => true,
            Some(t) => now.seconds_since(t) >= crate::flow::LOGIN_RESEND,
        };
        if due {
            if self.requests_sent >= crate::flow::LOGIN_RESEND_TRIES {
                self.requests_sent += 1;
                return Err(NetErrorCode::ClientTimedOutServer);
            }
            self.requests_sent += 1;
            self.last_sent = Some(now);
        }
        Ok(due)
    }
}

/// The connection-request handler's step 1: what to do with a `ConnectRequest` for a
/// recipient id that is already in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterationVerdict {
    /// Equal or older iteration: **ignore the packet entirely**.
    Ignore,
    /// Newer iteration: remove the connection for that recipient and rebuild. That is how a
    /// re-handshake replaces
    /// a stale connection.
    TearDownAndRebuild,
    /// The slot was free.
    Fresh,
}

/// See [`IterationVerdict`].
#[must_use]
pub fn iteration_verdict(existing: Option<u16>, incoming: u16) -> IterationVerdict {
    match existing {
        None => IterationVerdict::Fresh,
        Some(stored) => {
            if crate::session::overflow_compare(incoming, stored) > 0 {
                IterationVerdict::TearDownAndRebuild
            } else {
                IterationVerdict::Ignore
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/03-connection-state-machine.md` §6, whose string-id
    /// column was read out of the retail `client_local_English.dat` by hashing each `ID_` name and
    /// locating the table-8 entry.
    ///
    /// The hash is `dereth_primitives::num::hash::str_hash`, so this test proves the code, the string and the
    /// shared hash all agree.
    #[test]
    fn all_21_net_error_ids_resolve_to_the_documented_hashes() {
        let expected: [(NetErrorCode, u32); 21] = [
            (NetErrorCode::None, 0x02D0_6795),
            (NetErrorCode::BadServerAddress, 0x0CAD_D483),
            (NetErrorCode::CantBind, 0x058A_41C4),
            (NetErrorCode::CantSocket, 0x0B6A_2444),
            (NetErrorCode::CantCrypto, 0x0A67_822F),
            (NetErrorCode::AbortedHandshake, 0x0FDD_06C5),
            (NetErrorCode::RunningSpeedhack, 0x0361_611B),
            (NetErrorCode::NoLogonServer, 0x09D0_0312),
            (NetErrorCode::NetVersionMismatch, 0x00A7_E948),
            (NetErrorCode::ServerFull, 0x00F9_982C),
            (NetErrorCode::BadCryptoKey, 0x082E_3779),
            (NetErrorCode::InsufficientPrivilege, 0x012F_0F15),
            (NetErrorCode::SecondLogon, 0x0787_EDFE),
            (NetErrorCode::ServerClosedConnection, 0x0DFB_256E),
            (NetErrorCode::ServerTimedOutClient, 0x0CB9_7674),
            (NetErrorCode::ClientTimedOutServer, 0x07CF_6BF2),
            (NetErrorCode::PlayerAlreadyLoggedOn, 0x0C55_9B1E),
            (NetErrorCode::ClientLogOnFailed, 0x04DF_9C54),
            (NetErrorCode::AccountAuthenticationFailed, 0x0058_3B44),
            (NetErrorCode::LogonServerMigrated, 0x0321_EDB4),
            (NetErrorCode::Generic, 0x00CD_0B73),
        ];
        assert_eq!(expected.len(), NetErrorCode::ALL.len());
        for (code, hash) in expected {
            assert_eq!(code.string_id(), hash, "{} ({code:?})", code.id_string());
        }
        // Every code is distinct, so `unpack` is unambiguous.
        let mut ids: Vec<u32> = NetErrorCode::ALL.iter().map(|c| c.string_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 21);
    }

    /// The historical wire spelling determines the hash and must not be corrected.
    #[test]
    fn the_misspelled_id_is_load_bearing() {
        assert_eq!(
            NetErrorCode::InsufficientPrivilege.id_string(),
            WIRE_INSUFFICIENT_PRIVILEGE_ID
        );
        assert_ne!(
            dereth_primitives::num::hash::str_hash(b"ID_ConnectionError_InsufficientPrivilege"),
            0x012F_0F15
        );
    }

    /// The 8-byte wire form, with the table id always 8.
    #[test]
    fn net_error_packs_to_eight_bytes_with_table_id_8() {
        let packed = NetErrorCode::ServerFull.pack();
        assert_eq!(&packed[0..4], &0x00F9_982Cu32.to_le_bytes());
        assert_eq!(&packed[4..8], &8i32.to_le_bytes());
        assert_eq!(
            NetErrorCode::unpack(&packed),
            Some(NetErrorCode::ServerFull)
        );
        assert_eq!(
            NetErrorCode::unpack(&[0u8; 4]),
            None,
            "0 yields \"unknown\""
        );
        assert_eq!(NetErrorCode::unpack(&[0u8; 3]), None, "short buffer");
    }

    /// `ConnectResponse` and the `CICMDCommand` keep-alive go to port + 1.
    #[test]
    fn connect_response_goes_to_port_plus_one() {
        assert_eq!(handshake_port(9000), 9001);
        assert_eq!(handshake_port(19000), 19001);
        // 0x1C88 is the client's default port, `dereth_client_net::socket::DEFAULT_SERVER_PORT`.
        assert_eq!(handshake_port(0x1C88), 7305);
        // It wraps rather than overflowing, which is what `htons(ntohs(p) + 1)` does.
        assert_eq!(handshake_port(0xFFFF), 0);
    }

    /// A `ConnectRequest` with a newer iteration tears down and rebuilds; an equal or older one is
    /// ignored entirely.
    ///
    /// Oracle: the connection-request handler's step 1.
    #[test]
    fn a_newer_iteration_tears_down_and_rebuilds() {
        assert_eq!(iteration_verdict(None, 1), IterationVerdict::Fresh);
        assert_eq!(iteration_verdict(Some(5), 5), IterationVerdict::Ignore);
        assert_eq!(iteration_verdict(Some(5), 4), IterationVerdict::Ignore);
        assert_eq!(
            iteration_verdict(Some(5), 6),
            IterationVerdict::TearDownAndRebuild
        );
        // The compare wraps, so a re-handshake still works across the 16-bit boundary.
        assert_eq!(
            iteration_verdict(Some(0xFFFF), 0),
            IterationVerdict::TearDownAndRebuild
        );
        assert_eq!(iteration_verdict(Some(0), 0xFFFF), IterationVerdict::Ignore);
    }

    /// The client occupies exactly five of the eight states.
    #[test]
    fn the_client_occupies_five_of_the_eight_states() {
        assert_eq!(ConnectionState::CLIENT_REACHABLE.len(), 5);
        for s in ConnectionState::CLIENT_REACHABLE {
            assert!(!matches!(
                s,
                ConnectionState::AwaitingWorldAuth
                    | ConnectionState::AuthSent
                    | ConnectionState::ConnectionRequestSent
            ));
        }
        // Blobs stop being queued at DisconnectReceived.
        assert!(ConnectionState::Connected.accepts_blobs());
        assert!(!ConnectionState::DisconnectReceived.accepts_blobs());
        assert!(!ConnectionState::DisconnectSent.accepts_blobs());
    }

    /// The LoginRequest cadence: immediately, then every 2.0 s, at most 20 tries (40 s).
    ///
    /// Oracle: the logon retry's `requests_sent > 0x13` test.
    #[test]
    fn login_request_resends_every_2s_for_20_tries() {
        let mut st = LoginState::new();
        assert_eq!(st.should_send(LocalTime(0.0)), Ok(true), "immediately");
        assert_eq!(st.should_send(LocalTime(1.9)), Ok(false), "too soon");
        assert_eq!(st.should_send(LocalTime(2.0)), Ok(true));
        assert_eq!(st.requests_sent, 2);

        let mut t = 2.0f64;
        while st.requests_sent < crate::flow::LOGIN_RESEND_TRIES {
            t += 2.0;
            assert_eq!(st.should_send(LocalTime(t)), Ok(true), "at t = {t}");
        }
        assert_eq!(st.requests_sent, 20);
        // The first send is at t = 0, so the twentieth is at t = 38 and the give-up at t = 40 --
        // the doc's "20 tries (40 s)" is the window, not the last send.
        assert_eq!(t, 38.0);

        t += 2.0;
        assert_eq!(
            st.should_send(LocalTime(t)),
            Err(NetErrorCode::ClientTimedOutServer)
        );
    }

    /// The `ConnectRequest` body round-trips, and the two seeds are the ones the ISAAC streams get.
    ///
    /// Oracle: `docs/networking/01-packet-format.md` §3.11 and
    /// the receiver's crypto init.
    #[test]
    fn connect_request_round_trips_and_names_its_seeds_from_the_servers_view() {
        let cr = ConnectRequest {
            server_time: 1234.5,
            cookie: 0x0123_4567_89AB_CDEF,
            net_id: 0x0B,
            outgoing_seed: 0xDEAD_BEEF,
            incoming_seed: 0x1234_5678,
        };
        let bytes = cr.to_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(
            &bytes[28..32],
            &[0, 0, 0, 0],
            "the alignment padding ACE writes as 0u"
        );
        assert_eq!(ConnectRequest::from_bytes(&bytes), Ok(cr));

        // OutgoingSeed is server -> client and becomes the client's `crypto_incoming`.
        let recv = crate::session::ReceiverData::new(0x0B, cr.outgoing_seed, cr.incoming_seed);
        let mut incoming = recv.window.crypto_incoming;
        assert_eq!(
            incoming.next(),
            0x5DA2_2D96,
            "CryptoSystem(0xDEADBEEF)'s first draw"
        );

        // The direction names are the same two fields, and `new` places them back.
        let seeds = cr.seeds();
        assert_eq!(
            seeds,
            ConnectSeeds {
                server_to_client: 0xDEAD_BEEF,
                client_to_server: 0x1234_5678
            }
        );
        assert_eq!(
            ConnectRequest::new(cr.server_time, cr.cookie, cr.net_id, seeds),
            cr
        );
        assert_eq!(
            ConnectRequest::from_bytes(&cr.to_bytes()).map(|c| c.seeds()),
            Ok(seeds)
        );
    }

    /// `LoginRequest::parse` inverts `build_login_request`, with and without the optional
    /// "account to log on as" string, and refuses a body cut short anywhere.
    #[test]
    fn the_login_request_parses_back_to_its_authenticator() {
        let mut auth = ConnectionAuthenticator::account_password("Ac01", "pass");
        auth.connection_sequence_number = 0x1234_5678;
        auth.crypto_data = vec![9, 8, 7];
        let body = build_login_request(&auth);
        let back = LoginRequest::parse(&body).expect("decodes");
        assert_eq!(back.client_version, CLIENT_VERSION);
        assert_eq!(back.auth.pack(), auth.pack());
        assert_eq!(back.auth.account, "ac01");
        assert_eq!(back.auth.account_to_logon_as, None);
        assert_eq!(back.auth.auth_type, NetAuthType::AccountPassword);
        assert_eq!(back.auth.connection_sequence_number, 0x1234_5678);
        assert_eq!(back.auth.crypto_data, vec![9, 8, 7]);
        assert_eq!(back.auth.extra_data, auth.extra_data);

        // Flag 2 carries the extra string; without the flag the same bytes are crypto data's
        // length word, which is why the field is conditional.
        auth.auth_flags = 2;
        auth.account_to_logon_as = Some("other".to_owned());
        let back = LoginRequest::parse(&build_login_request(&auth)).expect("decodes");
        assert_eq!(back.auth.account_to_logon_as.as_deref(), Some("other"));
        assert_eq!(back.auth.pack(), auth.pack());

        for cut in 0..body.len() {
            assert!(LoginRequest::parse(&body[..cut]).is_none(), "cut at {cut}");
        }
        let mut bad_type = auth.pack();
        bad_type[0] = 7;
        assert!(ConnectionAuthenticator::unpack(&bad_type).is_none());
    }

    /// The password travels as an archive string -- a one-byte length and the bytes, unpadded --
    /// where the account name is a padded packed string, so the authenticator is byte for byte
    /// the one a recorded retail login carries.
    #[test]
    fn the_password_is_packed_as_an_archive_string_not_a_pack_string() {
        let mut auth = ConnectionAuthenticator::account_password("ac01", "ac01");
        auth.connection_sequence_number = 0x6A99_C6CC;
        assert_eq!(auth.extra_data, [4, b'a', b'c', b'0', b'1']);
        // The authenticator of `first-login-walk-jump`'s login request, 33 bytes.
        let retail: [u8; 33] = [
            2, 0, 0, 0, 0, 0, 0, 0, 0xCC, 0xC6, 0x99, 0x6A, // type, flags, sequence
            4, 0, b'a', b'c', b'0', b'1', 0, 0, // account: packed string
            0, 0, 0, 0, // no crypto data
            5, 0, 0, 0, 4, b'a', b'c', b'0', b'1', // password: archive string
        ];
        assert_eq!(auth.pack(), retail);
        // The longer length forms.
        assert_eq!(
            &crate::wire::optional::astring_pack(&[b'x'; 200])[..2],
            &[0x80, 200]
        );
        assert_eq!(
            &crate::wire::optional::astring_pack(&[b'x'; 0x1_2345])[..4],
            &[0xC0, 0x01, 0x45, 0x23]
        );
    }

    /// The client version is exactly `"1802"` and ACE compares it exactly.
    #[test]
    fn the_login_request_carries_the_literal_1802() {
        let auth = ConnectionAuthenticator::account_password("Ac01", "pass");
        let body = build_login_request(&auth);
        // PString "1802": u16 len 4, the bytes, two pad bytes.
        assert_eq!(
            &body[0..8],
            &[0x04, 0x00, b'1', b'8', b'0', b'2', 0x00, 0x00]
        );
        assert_eq!(CLIENT_VERSION, "1802");

        // The account is lower-cased by the command-line handler.
        assert_eq!(auth.account, "ac01");

        // cbAuthData is the packed authenticator's length, and the section reads back at exactly
        // the length the wire parser computes.
        let cb = u32::from_le_bytes([body[8], body[9], body[10], body[11]]) as usize;
        assert_eq!(cb, auth.pack().len());
        assert_eq!(body.len(), 8 + 4 + cb);
        assert_eq!(
            crate::wire::optional::section_len(PacketFlags::LOGIN_REQUEST, &body),
            Ok(body.len())
        );
    }

    /// The account-to-logon-as string appears only when the auth flags have bit 2, which retail
    /// never sets. The difference is four bytes, and it is the four bytes ACE's parser mistakes for
    /// the crypto data.
    #[test]
    fn account_to_logon_as_is_conditional() {
        let mut auth = ConnectionAuthenticator::account_password("ac01", "pass");
        let without = auth.pack().len();
        auth.auth_flags = 2;
        auth.account_to_logon_as = Some(String::new());
        assert_eq!(
            auth.pack().len(),
            without + 4,
            "an empty PString is 4 bytes"
        );
    }

    /// A whole `LoginRequest` packet: unsequenced, plaintext, `header_ = 0x00010000`.
    ///
    /// Oracle: the login request's field table, `docs/networking/01-packet-format.md` §3.9 and
    /// `docs/networking/03-connection-state-machine.md` §3.1. This is the
    /// first datagram of the handshake, so getting the encrypted-iff rule wrong here means never
    /// connecting at all.
    #[test]
    fn the_login_request_packet_is_unsequenced_and_plaintext() {
        let auth = ConnectionAuthenticator::account_password("ac01", "pass");
        let mut p = crate::OutPacket::new(crate::ProtoHeader::default());
        p.add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&auth))
            .expect("login request");
        assert!(!p.needs_encryption(), "LoginRequest is disposable");
        let bytes = p.serialize(None).expect("plaintext");

        let parsed = crate::ParsedPacket::parse(&bytes).expect("round trip");
        assert_eq!(parsed.header.seq_id, 0);
        assert_eq!(parsed.header.header.0, PacketFlags::LOGIN_REQUEST);
        assert_eq!(parsed.header.rec_id, 0);
        assert_eq!(parsed.header.interval, 0);
        assert_eq!(parsed.header.iteration, 0);
        assert!(!parsed.header.header.is_encrypted());
        assert!(parsed.checksum_ok(None));
        assert!(
            parsed.pre_connection_ok,
            "it must be legal before a ReceiverData exists"
        );
    }

    /// A whole `ConnectResponse` packet, which goes to port + 1.
    ///
    /// Oracle: the connect ack's field list, `docs/networking/03-connection-state-machine.md` §3.3.
    #[test]
    fn the_connect_response_packet_echoes_the_cookie_in_the_clear() {
        let cookie = 0x0123_4567_89AB_CDEFu64;
        let mut p = crate::OutPacket::new(crate::ProtoHeader {
            seq_id: 0,
            rec_id: 0x0B,
            interval: 0,
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::CONNECT_RESPONSE, cookie.to_le_bytes().to_vec())
            .expect("connect response");
        assert!(!p.needs_encryption());
        let bytes = p.serialize(None).expect("plaintext");
        let parsed = crate::ParsedPacket::parse(&bytes).expect("round trip");
        assert_eq!(parsed.header.header.0, PacketFlags::CONNECT_RESPONSE);
        assert_eq!(parsed.header.seq_id, 0);
        assert_eq!(parsed.header.rec_id, 0x0B);
        assert_eq!(parsed.header.iteration, 1);
        assert_eq!(
            parsed.optional.get(&PacketFlags::CONNECT_RESPONSE),
            Some(&cookie.to_le_bytes().to_vec())
        );
    }

    /// The Diffie-Hellman machinery is dead: the session keys arrive in clear in the
    /// `ConnectRequest`, and there is nothing to decrypt them with because there is nothing
    /// encrypted. See `docs/networking/03-connection-state-machine.md` §9.
    ///
    /// So the absence is what is implemented: this asserts that the whole of the key material a
    /// connection needs comes out of the plaintext `ConnectRequest`.
    #[test]
    fn there_is_no_key_exchange() {
        let cr = ConnectRequest {
            server_time: 0.0,
            cookie: 1,
            net_id: 1,
            outgoing_seed: 0xDEAD_BEEF,
            incoming_seed: 0x1234_5678,
        };
        let mut p = crate::OutPacket::new(crate::ProtoHeader::default());
        p.add_optional_header(PacketFlags::CONNECT_REQUEST, cr.to_bytes().to_vec())
            .expect("connect request");
        let bytes = p.serialize(None).expect("plaintext -- it is disposable");
        // Both seeds are literally readable in the datagram.
        assert!(bytes.windows(4).any(|w| w == 0xDEAD_BEEFu32.to_le_bytes()));
        assert!(bytes.windows(4).any(|w| w == 0x1234_5678u32.to_le_bytes()));
    }
}
