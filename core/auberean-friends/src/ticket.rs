//! The friends ticket: the game login's own account and secret, wrapped as
//! `a1.<account>.<secret>`, each part UTF-8 in URL-safe base64 without padding.
//!
//! The clients pass a ticket through untouched; only the world reads it, and checks the account
//! and secret by the same rule as its game login. This crate writes and reads the form and checks
//! nothing else. A ticket is a secret: its `Debug` form hides it.

use core::fmt;

use crate::element::MAX_TICKET;

/// The form's prefix.
pub const A1: &str = "a1.";

/// An account and its secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Ticket {
    pub account: String,
    pub secret: String,
}

impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ticket")
            .field("account", &self.account)
            .field("secret", &"***")
            .finish()
    }
}

/// Why a string is not a ticket of the `a1.` form, or a ticket cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketError {
    /// Not of the `a1.` form: another prefix, or not two parts after it.
    NotA1,
    /// A part that is not URL-safe base64 without padding.
    NotBase64,
    /// A part that is not UTF-8.
    NotUtf8,
    /// No account.
    NoAccount,
    /// Past the stream's bound of 512 characters.
    TooLong,
}

impl fmt::Display for TicketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotA1 => "not a ticket of the a1. form",
            Self::NotBase64 => "a part that is not URL-safe base64 without padding",
            Self::NotUtf8 => "a part that is not UTF-8",
            Self::NoAccount => "a ticket with no account",
            Self::TooLong => "a ticket past 512 characters",
        })
    }
}

impl Ticket {
    /// A ticket for an account and its secret.
    #[must_use]
    pub fn new(account: &str, secret: &str) -> Self {
        Self {
            account: account.to_owned(),
            secret: secret.to_owned(),
        }
    }

    /// The ticket as a client is given it.
    ///
    /// # Errors
    /// [`TicketError::NoAccount`] for an empty account, [`TicketError::TooLong`] for one longer
    /// than the stream carries.
    pub fn encode(&self) -> Result<String, TicketError> {
        if self.account.is_empty() {
            return Err(TicketError::NoAccount);
        }
        let mut out = String::from(A1);
        encode_base64url(self.account.as_bytes(), &mut out);
        out.push('.');
        encode_base64url(self.secret.as_bytes(), &mut out);
        if out.len() > MAX_TICKET {
            return Err(TicketError::TooLong);
        }
        Ok(out)
    }

    /// Read a ticket of the `a1.` form.
    ///
    /// # Errors
    /// [`TicketError`] for anything else.
    pub fn decode(text: &str) -> Result<Self, TicketError> {
        let rest = text.strip_prefix(A1).ok_or(TicketError::NotA1)?;
        let (account, secret) = rest.split_once('.').ok_or(TicketError::NotA1)?;
        if secret.contains('.') {
            return Err(TicketError::NotA1);
        }
        let part = |s: &str| {
            String::from_utf8(decode_base64url(s).ok_or(TicketError::NotBase64)?)
                .map_err(|_| TicketError::NotUtf8)
        };
        let account = part(account)?;
        if account.is_empty() {
            return Err(TicketError::NoAccount);
        }
        Ok(Self {
            account,
            secret: part(secret)?,
        })
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_base64url(bytes: &[u8], out: &mut String) {
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..=chunk.len() {
            let six = (n >> (18 - 6 * i)) & 63;
            out.push(char::from(ALPHABET[six as usize]));
        }
    }
}

fn decode_base64url(text: &str) -> Option<Vec<u8>> {
    let sixes: Vec<u32> = text
        .bytes()
        .map(|c| {
            ALPHABET
                .iter()
                .position(|&a| a == c)
                .and_then(|p| u32::try_from(p).ok())
        })
        .collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(sixes.len() * 3 / 4);
    for chunk in sixes.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, s)| n | s << (18 - 6 * i));
        let bytes = n.to_be_bytes();
        let keep = chunk.len() - 1;
        // The bits past the last whole byte must be zero, so that each ticket has one spelling.
        let spare = n & ((1u32 << (24 - 8 * keep)) - 1);
        if spare != 0 {
            return None;
        }
        out.extend_from_slice(&bytes[1..=keep]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (base64url against the standard's own examples)
    use super::*;

    #[test]
    fn base64url_matches_the_standards_examples() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
            ("\u{fb}\u{ff}", "w7vDvw"),
        ] {
            let mut out = String::new();
            encode_base64url(plain.as_bytes(), &mut out);
            assert_eq!(out, coded);
            assert_eq!(decode_base64url(coded).as_deref(), Some(plain.as_bytes()));
        }
        assert_eq!(decode_base64url("Zh"), None, "spare bits set");
        assert_eq!(decode_base64url("Zm9vY"), None, "one character over");
        assert_eq!(decode_base64url("Zg=="), None, "padding");
    }
}
