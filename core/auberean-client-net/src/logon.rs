//! The game login: the body of the first packet a client sends to the login server.
//!
//! It is the shared transport's login body with this game's strings scrambled: the version
//! string; the length of what follows; the way the client authenticates (a ticket), a flags word
//! and the time; the account; a counted session-key blob; a counted ticket (a 16-bit length and
//! its bytes). Strings pad to four bytes from the start of the body.
//!
//! The session-key blob is a key-exchange blob of the operating system's cryptography service: a
//! twelve-byte header naming a simple blob of a stream-cipher key exchanged by public key, then the
//! 64 bytes of the key, encrypted to the game's public key. No server reads it.

use auberean_primitives::scramble;
use dereth_transport::conn::{build_login_request_as, ConnectionAuthenticator, NetAuthType};

/// The version string of the September 2013 client.
pub const VERSION: &[u8] = b"1802";

/// The flags word every retail login sent.
pub const FLAGS: u32 = 1;

/// The session-key blob's header: a simple blob, version 2, of a stream-cipher key exchanged by
/// public key.
pub const KEY_BLOB_HEADER: [u8; 12] = [1, 2, 0, 0, 1, 0x68, 0, 0, 0, 0xA4, 0, 0];

/// The length of the encrypted session key after the header.
pub const KEY_LEN: usize = 64;

/// What a login carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Logon {
    /// The account, as typed (plain bytes; scrambled on the wire).
    pub account: Vec<u8>,
    /// The ticket the client was started with.
    pub ticket: Vec<u8>,
    /// Seconds since 1970 when the login starts.
    pub timestamp: u32,
    /// The encrypted session key: the host's own bytes.
    pub session_key: [u8; KEY_LEN],
}

impl Logon {
    /// The authenticator: a ticket login with the account and the session key.
    #[must_use]
    pub fn authenticator(&self) -> ConnectionAuthenticator {
        let mut crypto = KEY_BLOB_HEADER.to_vec();
        crypto.extend_from_slice(&self.session_key);
        let n = u16::try_from(self.ticket.len()).unwrap_or(u16::MAX);
        let mut ticket = n.to_le_bytes().to_vec();
        ticket.extend_from_slice(&self.ticket[..usize::from(n)]);
        ConnectionAuthenticator {
            auth_type: NetAuthType::GlsTicket,
            auth_flags: FLAGS,
            connection_sequence_number: self.timestamp,
            account: scramble::conceal(&self.account),
            account_to_logon_as: None,
            crypto_data: crypto,
            extra_data: ticket,
        }
    }

    /// The login section's body.
    // D2-05275
    #[must_use]
    pub fn body(&self) -> Vec<u8> {
        build_login_request_as(&scramble::conceal(VERSION), &self.authenticator())
    }
}
