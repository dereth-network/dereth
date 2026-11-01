//! The game login a client opens a connection with: its version, how it authenticates, its
//! account, the session key it made and the ticket it was given.
//!
//! The layout is the shared transport's login body with the game's scrambled strings: the
//! version string; the length of what follows; the authentication type, flags and a timestamp;
//! the account string; a counted session-key blob; a counted ticket (a 16-bit length, then its
//! bytes). Strings pad to four bytes from the start of the body.

use auberean_primitives::scramble;

/// Authentication by a ticket: what every retail login sent.
pub const TICKET: u32 = 0x4000_0002;

/// The version string of the September 2013 client.
pub const CLIENT_VERSION: &str = "1802";

/// A client's game login, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameLogin {
    pub version: String,
    pub auth_type: u32,
    pub auth_flags: u32,
    pub timestamp: u32,
    pub account: String,
    /// The session key, encrypted to a key no server holds: kept, never read.
    pub session_key: Vec<u8>,
    /// The ticket: the credential the client was started with.
    pub ticket: Vec<u8>,
}

struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<String> {
        let n = usize::from(self.u16()?);
        let plain = scramble::reveal(self.take(n)?);
        self.p += (4 - self.p % 4) % 4;
        (self.p <= self.b.len()).then(|| plain.iter().map(|&c| char::from(c)).collect())
    }

    fn counted(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

/// Read a login body. `None` when it is malformed.
#[must_use]
pub fn parse_login(body: &[u8]) -> Option<GameLogin> {
    let mut c = Cursor { b: body, p: 0 };
    let version = c.string()?;
    let rest = c.u32()? as usize;
    if c.p + rest != body.len() {
        return None;
    }
    let auth_type = c.u32()?;
    let auth_flags = c.u32()?;
    let timestamp = c.u32()?;
    let account = c.string()?;
    let session_key = c.counted()?.to_vec();
    let extra = c.counted()?;
    let ticket = match extra {
        [lo, hi, rest @ ..] if usize::from(u16::from_le_bytes([*lo, *hi])) == rest.len() => {
            rest.to_vec()
        }
        other => other.to_vec(),
    };
    Some(GameLogin {
        version,
        auth_type,
        auth_flags,
        timestamp,
        account,
        session_key,
        ticket,
    })
}

fn push_string(out: &mut Vec<u8>, plain: &[u8]) {
    let n = u16::try_from(plain.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&scramble::conceal(&plain[..usize::from(n)]));
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// A login body as the September 2013 client writes it, for test clients: the account and
/// ticket given, a session-key blob of the right form whose key is zeros.
#[must_use]
pub fn build_login(account: &str, ticket: &[u8], timestamp: u32) -> Vec<u8> {
    let mut body = Vec::new();
    push_string(&mut body, CLIENT_VERSION.as_bytes());
    let len_at = body.len();
    body.extend_from_slice(&[0; 4]);
    body.extend_from_slice(&TICKET.to_le_bytes());
    body.extend_from_slice(&1u32.to_le_bytes());
    body.extend_from_slice(&timestamp.to_le_bytes());
    push_string(&mut body, account.as_bytes());
    let mut key = vec![0u8; 76];
    key[..12].copy_from_slice(&[1, 2, 0, 0, 1, 0x68, 0, 0, 0, 0xA4, 0, 0]);
    body.extend_from_slice(&76u32.to_le_bytes());
    body.extend_from_slice(&key);
    let tn = u16::try_from(ticket.len()).unwrap_or(u16::MAX);
    body.extend_from_slice(&(u32::from(tn) + 2).to_le_bytes());
    body.extend_from_slice(&tn.to_le_bytes());
    body.extend_from_slice(&ticket[..usize::from(tn)]);
    let rest = u32::try_from(body.len() - len_at - 4).unwrap_or(0);
    body[len_at..len_at + 4].copy_from_slice(&rest.to_le_bytes());
    body
}
