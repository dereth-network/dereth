//! Packet and handshake codecs for the ac1999 dialect: the November 1999 client's transport.
//!
//! The header keeps the full recipient word and the two separate time/table bytes; nothing here
//! reinterprets an end-of-retail header or infers the dialect from a version string. All input is
//! bounded before allocation. Login, grant, acknowledgment and control packets carry an unkeyed
//! checksum even though their flags contain bit two; data checksums mix in a key from the
//! direction's own ISAAC stream. See `empyrean/relay/docs/07-ac1999-dialect.md`.

use dereth_primitives::NetQueue;
use dereth_transport::wire::{Fragment, HEADER_SIZE, MAX_DATAGRAM_OUT};
use dereth_transport::CryptoSystem;

/// The data connection: ordering, reassembly, flow reports and resends.
pub mod connection;

/// The version the November 1999 client reports and expects a grant to echo.
pub const CLIENT_VERSION: &str = "4.35752";

/// Header flag bits, by the role the November 1999 client gives them.
pub mod flags {
    /// A packet the client resends carries this bit besides its original flags.
    pub const RESENT: u32 = 0x1;
    /// Set on every packet the client accepts: the checksum field is present.
    pub const CHECKSUM: u32 = 0x2;
    /// A list of sequences the sender wants resent.
    pub const RESEND_REQUEST: u32 = 0x8;
    /// A clock synchronisation: from the server an 8-byte time, from the client a bare request.
    pub const TIME_SYNC: u32 = 0x10;
    /// A flow value.
    pub const FLOW: u32 = 0x20;
    /// The client's log-off.
    pub const LOGOFF: u32 = 0x40;
    /// A list of sequences the sender will never resend; the receiver stops waiting for them.
    pub const SKIP: u32 = 0x80;
    /// The server switch that drives the connect progress; the client answers each one.
    pub const SERVER_SWITCH: u32 = 0x100;
    /// The connect grant and its acknowledgment.
    pub const GRANT: u32 = 0x200;
    /// Application fragments.
    pub const DATA: u32 = 0x800;
    /// The client's report of the bytes it received while the server's time byte held a value.
    pub const FLOW_REPORT: u32 = 0x1000;
    /// From ac1999's server, the sequences it will never resend.
    pub const DENY: u32 = 0x2000;
    /// A reset the client sends before logging in, and that ac1999's server ends a session on.
    pub const RESET: u32 = 0x8000;
    /// A grant that carries an error instead of the connection's parameters.
    pub const GRANT_ERROR: u32 = 0x1_0000;
    /// The server ends the session; the client answers and closes.
    pub const DISCONNECT: u32 = 0x2_0000;
    /// A network error.
    pub const NET_ERROR: u32 = 0x10_0000;
}

/// The login request's first word.
const LOGIN_MAGIC: u32 = 0x075B_CD15;
/// The checksum field's stand-in while the header is hashed.
const CHECKSUM_SEED: u32 = 0x0329_1975;
/// Each account field opens with this marker.
const ACCOUNT_MARKER: u32 = 0xBEEF_BEEF;
/// An account field holds this many characters, with no room promised for a terminator.
pub const ACCOUNT_CHARS: usize = 20;
/// The client accepts only a grant whose header names a connection below this.
pub const MAX_SERVER_ID: u32 = 400;
/// Bounds a version string's length; the client itself sets none.
const MAX_VERSION: usize = 63;

/// A malformed or unsupported packet. No error contains credentials or packet bodies.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("truncated packet or field")]
    Truncated,
    #[error("datagram exceeds the 484-byte budget")]
    Oversized,
    #[error("declared payload length does not fit the datagram")]
    Length,
    #[error("checksum mismatch")]
    Checksum,
    #[error("unsupported packet flags")]
    Flags,
    #[error("invalid connection grant")]
    Grant,
    #[error("version must be 1 to 63 printable ASCII bytes")]
    Version,
    #[error("credentials must be nonempty printable ASCII, with no colon in the account, and fit 20 bytes including the separator")]
    Credentials,
    #[error("a login must report both data files' iterations")]
    Iterations,
    #[error("invalid fragment")]
    Fragment,
    #[error("unsupported queue")]
    Queue,
}

/// The 20-byte ac1999 header. The recipient field is never narrowed to a later header's u16.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub sequence: u32,
    pub flags: u32,
    pub recipient: u32,
    pub checksum: u32,
    pub payload_len: u16,
    pub time: u8,
    pub table: u8,
}

impl Header {
    /// # Errors
    /// Returns [`Error::Truncated`] when the fixed header is incomplete.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader(bytes);
        Ok(Self {
            sequence: r.u32()?,
            flags: r.u32()?,
            recipient: r.u32()?,
            checksum: r.u32()?,
            payload_len: u16::from_le_bytes(r.array()?),
            time: r.array::<1>()?[0],
            table: r.array::<1>()?[0],
        })
    }

    #[must_use]
    pub fn encode(self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0; HEADER_SIZE];
        for (chunk, word) in bytes[..16].as_chunks_mut::<4>().0.iter_mut().zip([
            self.sequence,
            self.flags,
            self.recipient,
            self.checksum,
        ]) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        bytes[16..18].copy_from_slice(&self.payload_len.to_le_bytes());
        bytes[18] = self.time;
        bytes[19] = self.table;
        bytes
    }

    /// The header's own share of every checksum: its hash with the checksum field replaced.
    #[must_use]
    pub fn checksum_base(mut self) -> u32 {
        self.checksum = CHECKSUM_SEED;
        dereth_transport::hash32(&self.encode())
    }

    #[must_use]
    pub fn has(self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// A borrowed datagram with no interpretation of application bytes. `body` is the declared
/// payload; `extra` is whatever the datagram carries past it. Debug output omits both, which may
/// be an authentication request.
#[derive(Clone, Copy)]
pub struct Packet<'a> {
    pub header: Header,
    pub body: &'a [u8],
    pub extra: &'a [u8],
}

impl std::fmt::Debug for Packet<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Packet")
            .field("header", &self.header)
            .finish_non_exhaustive()
    }
}

impl<'a> Packet<'a> {
    /// # Errors
    /// Rejects a short header, an oversized datagram or a payload length past its end.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_DATAGRAM_OUT {
            return Err(Error::Oversized);
        }
        let header = Header::decode(bytes)?;
        let rest = &bytes[HEADER_SIZE..];
        let (body, extra) = rest
            .split_at_checked(usize::from(header.payload_len))
            .ok_or(Error::Length)?;
        Ok(Self {
            header,
            body,
            extra,
        })
    }

    /// The checksum of a packet with nothing mixed in: the header's share plus the body's words.
    #[must_use]
    pub fn generic_checksum_ok(self) -> bool {
        self.header.checksum
            == self
                .header
                .checksum_base()
                .wrapping_add(body_hash(self.body))
    }

    /// A header-only checksum, which the disconnect and error notices carry.
    #[must_use]
    pub fn bare_checksum_ok(self) -> bool {
        self.header.checksum == self.header.checksum_base()
    }

    /// The fragments of a data packet, checked against the caller's key. No reassembly, delivery
    /// or sequence state is touched here.
    ///
    /// # Errors
    /// Rejects a packet without the data bit, empty or malformed fragments, or a wrong checksum.
    pub fn fragments(self, key: u32) -> Result<Vec<Fragment>, Error> {
        if !self.header.has(flags::DATA) || self.header.sequence == 0 || self.body.is_empty() {
            return Err(Error::Flags);
        }
        let mut fragments = Vec::new();
        let mut rest = self.body;
        let mut hash = 0u32;
        while !rest.is_empty() {
            let (fragment, used) = Fragment::from_bytes(rest).map_err(|_| Error::Fragment)?;
            if fragment.header.num_frags == 0
                || fragment.header.blob_num >= fragment.header.num_frags
                || fragment.payload.is_empty()
            {
                return Err(Error::Fragment);
            }
            hash = hash.wrapping_add(fragment.hash());
            fragments.push(fragment);
            rest = &rest[used..];
        }
        if self
            .header
            .checksum
            .wrapping_sub(self.header.checksum_base())
            ^ key
            != hash
        {
            return Err(Error::Checksum);
        }
        Ok(fragments)
    }
}

/// The hash of a body or of a lone word, which never folds in a length term.
#[must_use]
pub fn body_hash(bytes: &[u8]) -> u32 {
    dereth_transport::hash32(bytes)
        .wrapping_sub(u32::try_from(bytes.len()).unwrap_or(u32::MAX) << 16)
}

/// Frame a header and body with the unkeyed checksum. Computes the length, never truncates.
///
/// # Errors
/// Returns [`Error::Oversized`] when the header and body exceed the datagram budget.
pub fn encode_generic(mut header: Header, body: &[u8]) -> Result<Vec<u8>, Error> {
    if body.len() > MAX_DATAGRAM_OUT - HEADER_SIZE {
        return Err(Error::Oversized);
    }
    header.payload_len = u16::try_from(body.len()).map_err(|_| Error::Oversized)?;
    header.checksum = header.checksum_base().wrapping_add(body_hash(body));
    let mut packet = header.encode().to_vec();
    packet.extend_from_slice(body);
    Ok(packet)
}

/// Frame a client packet: the header, then a flow word the header's length does not count, then
/// the body. The checksum is the header's share, the flow word's, and `body_sum`: for a control
/// body its unkeyed hash, for data the fragments' hash mixed with a key.
///
/// # Errors
/// Returns [`Error::Oversized`] when the datagram would exceed its budget.
pub fn encode_with_flow(
    mut header: Header,
    flow: u32,
    body: &[u8],
    body_sum: u32,
) -> Result<Vec<u8>, Error> {
    if body.len() > MAX_DATAGRAM_OUT - HEADER_SIZE - 4 {
        return Err(Error::Oversized);
    }
    header.payload_len = u16::try_from(body.len()).map_err(|_| Error::Oversized)?;
    header.checksum = header
        .checksum_base()
        .wrapping_add(body_hash(&flow.to_le_bytes()))
        .wrapping_add(body_sum);
    let mut packet = header.encode().to_vec();
    packet.extend_from_slice(&flow.to_le_bytes());
    packet.extend_from_slice(body);
    Ok(packet)
}

/// The dat iterations a login reports. They belong to the login, not to a gameplay era.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LoginMetadata {
    pub portal_iteration: u32,
    pub cell_iteration: u32,
}

fn valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= MAX_VERSION
        && version.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

/// The account field's text: `account:password`, lowercased as the client lowercases it, so an
/// account made by either client is the same account.
///
/// # Errors
/// Refuses text the 20-character field cannot hold whole, an empty part, a colon in the account
/// name, or anything outside printable ASCII.
pub fn account_text(account: &str, password: &str) -> Result<String, Error> {
    let printable = |s: &str| s.bytes().all(|b| (0x20..=0x7e).contains(&b));
    if account.is_empty()
        || password.is_empty()
        || account.contains(':')
        || account.len() + 1 + password.len() > ACCOUNT_CHARS
        || !printable(account)
        || !printable(password)
    {
        return Err(Error::Credentials);
    }
    Ok(format!("{account}:{password}").to_ascii_lowercase())
}

/// An account field: the marker, the characters zero-padded to twenty, and a reserved word.
#[must_use]
pub fn account_field(text: &str) -> [u8; 28] {
    let mut field = [0; 28];
    field[..4].copy_from_slice(&ACCOUNT_MARKER.to_le_bytes());
    let raw = text.as_bytes();
    let n = raw.len().min(ACCOUNT_CHARS);
    field[4..4 + n].copy_from_slice(&raw[..n]);
    field
}

/// Build the login request as the November 1999 client lays it out: the magic word, the version,
/// the account field, an empty second account field, a one, an empty ticket, both dat iterations,
/// a zero and an all-ones word. The caller must treat the returned bytes as credentials.
///
/// A login with no iteration is never made: an ac1999 server writes such a login to its log whole,
/// password included.
///
/// # Errors
/// Rejects an invalid version or credentials, or a missing iteration, before producing a datagram.
pub fn login_request(
    version: &str,
    account: &str,
    password: &str,
    metadata: LoginMetadata,
) -> Result<Vec<u8>, Error> {
    if !valid_version(version) {
        return Err(Error::Version);
    }
    let text = account_text(account, password)?;
    if metadata.portal_iteration == 0 || metadata.cell_iteration == 0 {
        return Err(Error::Iterations);
    }
    let mut body = LOGIN_MAGIC.to_le_bytes().to_vec();
    let length = u16::try_from(version.len() + 1).map_err(|_| Error::Version)?;
    body.extend_from_slice(&length.to_le_bytes());
    body.extend_from_slice(version.as_bytes());
    body.push(0);
    body.resize(body.len().next_multiple_of(4), 0);
    body.extend_from_slice(&account_field(&text));
    body.extend_from_slice(&account_field(""));
    for word in [
        1,
        0,
        metadata.portal_iteration,
        metadata.cell_iteration,
        0,
        u32::MAX,
    ] {
        body.extend_from_slice(&word.to_le_bytes());
    }
    encode_generic(
        Header {
            sequence: 1,
            flags: flags::CHECKSUM,
            ..Header::default()
        },
        &body,
    )
}

/// A grant: the server's connection id and table byte from its header, then its body's clocks,
/// the recipient id the client adopts, the version, and one seed per direction.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    pub server: u32,
    pub table: u8,
    pub server_time: f64,
    pub world_epoch: f64,
    pub recipient: u32,
    pub version: String,
    pub server_to_client_seed: u32,
    pub client_to_server_seed: u32,
}

impl Grant {
    /// # Errors
    /// Rejects a packet that is not a grant, a server id outside the client's table, a wrong
    /// checksum, truncated fields, a version that is not printable ASCII, or nonfinite clocks.
    /// Bytes after the seeds are ignored, as the client ignores them.
    pub fn decode(packet: Packet<'_>) -> Result<Self, Error> {
        let h = packet.header;
        if !h.has(flags::CHECKSUM) || !h.has(flags::GRANT) || h.has(flags::GRANT_ERROR) {
            return Err(Error::Flags);
        }
        if h.recipient == 0 || h.recipient >= MAX_SERVER_ID {
            return Err(Error::Grant);
        }
        if !packet.generic_checksum_ok() {
            return Err(Error::Checksum);
        }
        let mut r = Reader(packet.body);
        let server_time = f64::from_le_bytes(r.array()?);
        let world_epoch = f64::from_le_bytes(r.array()?);
        let recipient = r.u32()?;
        let version_len = usize::try_from(r.u32()?).map_err(|_| Error::Version)?;
        if version_len == 0 || version_len > MAX_VERSION {
            return Err(Error::Version);
        }
        let raw = r.take(version_len)?;
        let version = std::str::from_utf8(raw)
            .ok()
            .map(|v| v.trim_end_matches('\0'))
            .filter(|v| valid_version(v))
            .ok_or(Error::Version)?;
        let server_to_client_seed = r.u32()?;
        let client_to_server_seed = r.u32()?;
        if !server_time.is_finite() || !world_epoch.is_finite() {
            return Err(Error::Grant);
        }
        Ok(Self {
            server: h.recipient,
            table: h.table,
            server_time,
            world_epoch,
            recipient,
            version: version.to_owned(),
            server_to_client_seed,
            client_to_server_seed,
        })
    }

    /// Whether the advertised version is the client's own. The client compares the numbers the
    /// two strings begin with, exactly, so text after the number does not matter.
    #[must_use]
    pub fn accepts_version(&self, own: &str) -> bool {
        match (leading_number(&self.version), leading_number(own)) {
            (Some(theirs), Some(ours)) => theirs == ours,
            _ => false,
        }
    }

    /// The acknowledgment: header only, to the adopted recipient, with the grant's table byte.
    ///
    /// # Errors
    /// Never in practice; framing an empty body cannot overflow.
    pub fn acknowledgment(&self) -> Result<Vec<u8>, Error> {
        encode_generic(
            Header {
                flags: flags::GRANT | flags::CHECKSUM,
                recipient: self.recipient,
                table: self.table,
                ..Header::default()
            },
            &[],
        )
    }
}

/// The decimal number a string begins with, read the way a C string-to-double conversion reads it.
fn leading_number(text: &str) -> Option<f64> {
    let s = text.trim_start();
    let b = s.as_bytes();
    let digits = |from: usize| b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut end = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let whole = digits(end);
    end += whole;
    let mut fraction = 0;
    if b.get(end) == Some(&b'.') {
        fraction = digits(end + 1);
        end += 1 + fraction;
    }
    if whole + fraction == 0 {
        return None;
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let mut at = end + 1;
        if matches!(b.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent = digits(at);
        if exponent > 0 {
            end = at + exponent;
        }
    }
    s[..end].parse().ok()
}

/// Map a received application queue. Bodies still need the dialect's application codecs.
///
/// # Errors
/// Returns [`Error::Queue`] for a queue the server does not send on.
pub fn incoming_queue(queue: u16) -> Result<NetQueue, Error> {
    match queue {
        3 => Ok(NetQueue::WorldObjects),
        4 => Ok(NetQueue::UiQueue),
        7 => Ok(NetQueue::ClientCache),
        _ => Err(Error::Queue),
    }
}

/// The queues the client sends on: control, game actions, login and the dat cache.
pub const OUTGOING_QUEUES: [u16; 4] = [1, 5, 6, 7];

/// The queue login, character and world-entry requests go out on.
pub const LOGIN_QUEUE: u16 = 6;

/// One direction's key stream. Each new sequence draws the next key, so keys belong to sequences
/// in order; whoever needs a key again for a resend keeps it beside the packet.
#[derive(Debug, Clone)]
pub struct KeyStream(CryptoSystem);

impl KeyStream {
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self(CryptoSystem::new(seed))
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.0.next()
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let (head, tail) = self.0.split_at_checked(count).ok_or(Error::Truncated)?;
        self.0 = tail;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?.try_into().map_err(|_| Error::Truncated)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }
}

#[cfg(test)]
mod tests;
