//! Discovery: the four places an identity is *named by the protocol*, and nothing else.
//!
//! The scrubber never guesses what a string is. It decodes exactly four things and takes the
//! identities out of their named fields:
//!
//! | source | direction | what it yields |
//! |---|---|---|
//! | the `LoginRequest` optional header | c2s | the account, and the password or ticket |
//! | `0xF658 Login_LoginCharacterSet` | s2c | every character name on the account, and the account |
//! | `0xF657 Login_SendEnterWorld`, `0xF655 Character_CharacterDelete` | c2s | the account |
//! | `0xF745 Item_CreateObject` / `0xF7DB Item_UpdateObject`, **known character id** | s2c | that character's name |
//!
//! The direction is part of the key, never just the opcode. `0xF655` is
//! `CharacterDeleteRequest` from the client and `CharacterDeleteAck` from the server, and
//! decoding the acknowledgement as the request is exactly the kind of silent mis-read that would
//! leave a name in place.
//!
//! Everything else — chat, tells, the friends list, an allegiance roster, an `@`-command echo — is
//! reached by *substitution*, not by discovery: once a name is known, every occurrence of it
//! anywhere in the recording is replaced. That split is what keeps the decoder surface small
//! enough to review.
//!
//! The create-object arm is a cross-check rather than a new source: it is keyed on an id the
//! character set already supplied, so a name it finds that the character set did not is a
//! disagreement worth seeing, and it is reported as one.
//!
//! # The `LoginRequest` authenticator, as the recordings actually carry it
//!
//! The authenticator writes `u32 authType | u32 authFlags | u32 connectionSequence |
//! PString account | [PString accountToLogonAs if authFlags & 2] | u32 cbCryptoData + bytes |
//! u32 cbExtraData + bytes`. The retail client's *extra data* is **not** a `PString`: it is a
//! single length byte followed by that many characters. Both shapes are accepted here and the one
//! that accounts for the whole field wins, because a password read one byte short would be
//! substituted one byte short.

use std::collections::{BTreeMap, BTreeSet};

use dereth_protocol::login::{CharacterDeleteRequest, LoginCharacterSet, LoginSendEnterWorld};
use dereth_protocol::objects::{ItemCreateObject, ItemUpdateObject};
use dereth_protocol::read_body;
use dereth_transport::wire::PacketFlags;

use crate::raw::Dir;
use crate::regions::{Kind, Region};

/// What an identity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdKind {
    /// An account name.
    Account,
    /// A password or GLS ticket.
    Password,
    /// A character name.
    Character,
    /// Something named in the manual extras file.
    Extra,
}

impl IdKind {
    /// The token used in the scrub map and the counts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Password => "password",
            Self::Character => "character",
            Self::Extra => "extra",
        }
    }

    /// The kind a scrub-map row names, by the token [`Self::as_str`] writes.
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        [Self::Account, Self::Password, Self::Character, Self::Extra]
            .into_iter()
            .find(|k| k.as_str() == token)
    }
}

/// One discovered identity, with where it was first seen.
#[derive(Debug, Clone)]
pub struct Found {
    /// What it is.
    pub kind: IdKind,
    /// The bytes as the protocol carried them.
    pub value: String,
    /// The recording it was first seen in.
    pub session: String,
    /// The datagram its first occurrence arrived in.
    pub datagram: usize,
    /// The opcode or `header_` mask it was decoded out of, for the private map.
    pub source: String,
}

/// What one recording's discovery pass found.
#[derive(Debug, Default)]
pub struct Discovery {
    /// The identities, in first-appearance order.
    pub found: Vec<Found>,
    /// Character ids the character set named, so create-object can be keyed on them.
    pub character_ids: BTreeSet<u32>,
    /// Blobs whose decode failed where the opcode said it should not. Reported, never ignored.
    pub decode_failures: Vec<(u32, String)>,
    /// Names a player create-object carried that the character set did not name.
    pub create_object_only: usize,
    /// `LoginRequest` sections whose extra data matched neither known shape, so no secret was
    /// taken out of them. A non-zero count is a password this pass did not remove.
    pub unread_secrets: usize,
    /// `LoginRequest` sections seen.
    pub login_requests: usize,
}

/// Decode the four named sources of one recording.
#[must_use]
pub fn discover(session: &str, regions: &[Region]) -> Discovery {
    let mut out = Discovery::default();
    let mut seen: BTreeSet<(IdKind, String)> = BTreeSet::new();
    let push = |out: &mut Discovery,
                seen: &mut BTreeSet<(IdKind, String)>,
                kind: IdKind,
                value: &str,
                datagram: usize,
                source: &str| {
        // **The shard's `+` marker is a prefix, not part of the name.**
        //
        // ACE puts a leading `+` on every character of an admin-flagged account, and the retail
        // client strips it before the name goes on the wire (the client's tell wire test pins
        // this). So the *same* character appears in a recording both as `+Name` and as `Name`,
        // and taking `+Name` as the identity gets both of those wrong at once:
        //
        // * the bare form is not a pattern at all, so it is published in the clear (68
        //   occurrences across six recordings); and
        // * the substitution eats the marker, so the public corpus carries no `+` anywhere and
        //   the one client behaviour the marker is evidence for stops being testable.
        //
        // Stripping it here fixes both, and costs nothing: the pattern `Name` still matches
        // inside `+Name` (the byte before it is `+`, which is not alphanumeric, so the
        // word-boundary guard passes), and the stand-in is the same length as the bare name, so
        // `+Name` -> `+Stand-in` stays byte-for-byte the same width. The marker is not an
        // identity: it says the account is an admin one, which the recordings say anyway.
        let v = value.trim();
        let v = if kind == IdKind::Character {
            v.strip_prefix('+').unwrap_or(v)
        } else {
            v
        };
        if v.is_empty() || !seen.insert((kind, v.to_owned())) {
            return false;
        }
        out.found.push(Found {
            kind,
            value: v.to_owned(),
            session: session.to_owned(),
            datagram,
            source: source.to_owned(),
        });
        true
    };

    // Pass one: the login request and the character set, which between them supply every id the
    // create-object pass is allowed to key on.
    for r in regions {
        match r.kind {
            Kind::Section { datagram, mask, .. } if mask == PacketFlags::LOGIN_REQUEST => {
                match parse_login_request(&r.buf) {
                    Some(lr) => {
                        out.login_requests += 1;
                        push(
                            &mut out,
                            &mut seen,
                            IdKind::Account,
                            &lr.account,
                            datagram,
                            "LoginRequest.account",
                        );
                        match lr.secret {
                            Some(p) => {
                                push(
                                    &mut out,
                                    &mut seen,
                                    IdKind::Password,
                                    &p,
                                    datagram,
                                    "LoginRequest.extraData",
                                );
                            }
                            None => out.unread_secrets += 1,
                        }
                    }
                    None => out
                        .decode_failures
                        .push((PacketFlags::LOGIN_REQUEST, "LoginRequest".into())),
                }
            }
            Kind::Blob {
                dir,
                opcode,
                first_datagram,
                complete,
            } if complete => {
                let body = &r.buf[4.min(r.buf.len())..];
                // Three of these four opcodes carry a *different* body in the other direction --
                // `0xF655` is the request one way and the acknowledgement the other -- so the
                // direction is part of the key, never just the opcode.
                match (dir, opcode) {
                    (Dir::S2c, 0xF658) => match read_body::<LoginCharacterSet>(body) {
                        Ok(cs) => {
                            push(
                                &mut out,
                                &mut seen,
                                IdKind::Account,
                                &cs.account,
                                first_datagram,
                                "LoginCharacterSet.account",
                            );
                            for c in cs.characters.iter().chain(cs.deleted.iter()) {
                                out.character_ids.insert(c.gid.0);
                                push(
                                    &mut out,
                                    &mut seen,
                                    IdKind::Character,
                                    &c.name,
                                    first_datagram,
                                    "LoginCharacterSet.characters[].name",
                                );
                            }
                        }
                        Err(e) => out.decode_failures.push((opcode, format!("{e:?}"))),
                    },
                    (Dir::C2s, 0xF657) => match read_body::<LoginSendEnterWorld>(body) {
                        Ok(ew) => {
                            out.character_ids.insert(ew.character.0);
                            push(
                                &mut out,
                                &mut seen,
                                IdKind::Account,
                                &ew.account,
                                first_datagram,
                                "LoginSendEnterWorld.account",
                            );
                        }
                        Err(e) => out.decode_failures.push((opcode, format!("{e:?}"))),
                    },
                    (Dir::C2s, 0xF655) => match read_body::<CharacterDeleteRequest>(body) {
                        Ok(d) => {
                            push(
                                &mut out,
                                &mut seen,
                                IdKind::Account,
                                &d.account,
                                first_datagram,
                                "CharacterDeleteRequest.account",
                            );
                        }
                        Err(e) => out.decode_failures.push((opcode, format!("{e:?}"))),
                    },
                    _ => {}
                }
            }
            _ => {}
        }
    }

    // Pass two: the player's own create-object, keyed on an id pass one supplied.
    for r in regions {
        let Kind::Blob {
            dir: Dir::S2c,
            opcode,
            first_datagram,
            complete: true,
        } = r.kind
        else {
            continue;
        };
        if opcode != 0xF745 && opcode != 0xF7DB {
            continue;
        }
        let body = &r.buf[4.min(r.buf.len())..];
        let payload = if opcode == 0xF745 {
            read_body::<ItemCreateObject>(body).map(|m| m.0)
        } else {
            read_body::<ItemUpdateObject>(body).map(|m| m.0)
        };
        let Ok(p) = payload else { continue };
        if !out.character_ids.contains(&p.id.0) {
            continue;
        }
        if push(
            &mut out,
            &mut seen,
            IdKind::Character,
            &p.wdesc.name,
            first_datagram,
            "ItemCreateObject.wdesc.name",
        ) {
            out.create_object_only += 1;
        }
    }
    out
}

/// The two identities a `LoginRequest` section carries.
#[derive(Debug, Clone)]
pub struct LoginRequestIds {
    /// Account name captured by the authenticator.
    pub account: String,
    /// The extra data, which for `AccountPassword` is the password.
    pub secret: Option<String>,
}

/// Decode a `LoginRequest` optional-header body far enough to name its account and secret.
///
/// Returns `None` when the section does not have the shape expected by the login-header parser; the
/// caller reports that rather than proceeding, because a login request the scrubber cannot read is
/// a login request whose account it cannot remove.
#[must_use]
pub fn parse_login_request(buf: &[u8]) -> Option<LoginRequestIds> {
    // PString ClientVersion, padded to four.
    let (_version, mut at) = pstring16(buf, 0)?;
    let cb_auth = read_u32(buf, at)? as usize;
    at += 4;
    let auth = buf.get(at..at.checked_add(cb_auth)?)?;

    // u32 authType | u32 authFlags | u32 connectionSequence, then the account.
    let auth_flags = read_u32(auth, 4)?;
    let (account, mut a) = pstring16(auth, 12)?;
    if auth_flags & 2 != 0 {
        a = pstring16(auth, a)?.1;
    }
    let cb_crypto = read_u32(auth, a)? as usize;
    a += 4 + cb_crypto;
    let cb_extra = read_u32(auth, a)? as usize;
    a += 4;
    let extra = auth.get(a..a.checked_add(cb_extra)?)?;
    Some(LoginRequestIds {
        account,
        secret: extra_string(extra),
    })
}

/// The extra data as the retail client writes it: one length byte then that many characters.
///
/// The `PString` shape (`u16` length, padded to four) is accepted as well, because `dereth-transport`'s
/// own account-password encoder writes that one and a synthesised recording would
/// carry it. Whichever shape accounts for the field exactly is the one taken; if neither does, the
/// field is left alone rather than guessed at.
#[must_use]
pub fn extra_string(extra: &[u8]) -> Option<String> {
    if extra.is_empty() {
        return None;
    }
    let byte_len = usize::from(extra[0]);
    if byte_len + 1 == extra.len() {
        return ascii(&extra[1..]);
    }
    if extra.len() >= 2 {
        let word_len = usize::from(u16::from_le_bytes([extra[0], extra[1]]));
        if 2 + word_len <= extra.len() && (2 + word_len).next_multiple_of(4) == extra.len() {
            return ascii(extra.get(2..2 + word_len)?);
        }
    }
    None
}

fn ascii(b: &[u8]) -> Option<String> {
    if b.is_empty() || b.iter().any(|c| !(0x20..0x7F).contains(c)) {
        return None;
    }
    Some(String::from_utf8_lossy(b).into_owned())
}

fn read_u32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A packed `PString`: `u16` length, the bytes, padded to a multiple of four. Returns the string
/// and the offset just past the padding.
fn pstring16(b: &[u8], at: usize) -> Option<(String, usize)> {
    let len = usize::from(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]));
    if len == 0xFFFF {
        return None;
    }
    let s = b.get(at + 2..at + 2 + len)?;
    Some((
        String::from_utf8_lossy(s).into_owned(),
        at + (2 + len).next_multiple_of(4),
    ))
}

/// Merge the manual extras file into a discovery.
pub fn add_extras(found: &mut Vec<Found>, extras: &BTreeMap<IdKind, Vec<String>>) {
    // Case-folded, the same way the corpus-wide de-duplication in `main` folds: an extras entry
    // that differs from a discovered identity only in case is that identity, not a new one.
    let mut seen: BTreeSet<(IdKind, String)> = found
        .iter()
        .map(|f| (f.kind, f.value.to_ascii_lowercase()))
        .collect();
    for (kind, values) in extras {
        for v in values {
            if v.trim().is_empty() || !seen.insert((*kind, v.to_ascii_lowercase())) {
                continue;
            }
            found.push(Found {
                kind: *kind,
                value: v.clone(),
                session: "(extras)".into(),
                datagram: 0,
                source: "extras file".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retail shape: `dereth-transport` builds the section, and the length-byte secret is written by
    /// hand because that is what the recordings carry and what `ConnectionAuthenticator` does not write.
    #[test]
    fn the_login_request_yields_the_account_and_the_length_byte_secret() {
        let mut auth = Vec::new();
        auth.extend_from_slice(&2u32.to_le_bytes()); // AccountPassword
        auth.extend_from_slice(&0u32.to_le_bytes()); // authFlags
        auth.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        auth.extend_from_slice(&dereth_transport::wire::optional::pstring_pack(b"ac01"));
        auth.extend_from_slice(&0u32.to_le_bytes()); // cbCryptoData
        auth.extend_from_slice(&7u32.to_le_bytes()); // cbExtraData
        auth.push(6);
        auth.extend_from_slice(b"passwo");

        let mut body = dereth_transport::wire::optional::pstring_pack(b"1802");
        body.extend_from_slice(&u32::try_from(auth.len()).expect("fits").to_le_bytes());
        body.extend_from_slice(&auth);

        let ids = parse_login_request(&body).expect("the section decodes");
        assert_eq!(ids.account, "ac01");
        assert_eq!(ids.secret.as_deref(), Some("passwo"));
    }

    /// And the padded packed-string shape, which a synthesised recording may carry, so such a
    /// recording is scrubbed too.
    #[test]
    fn the_pstring_shape_of_the_secret_is_accepted_as_well() {
        let mut auth =
            dereth_transport::conn::ConnectionAuthenticator::account_password("Acct06", "secret");
        auth.extra_data = dereth_transport::wire::optional::pstring_pack(b"secret");
        let body = dereth_transport::conn::build_login_request(&auth);
        let ids = parse_login_request(&body).expect("the section decodes");
        assert_eq!(ids.account, "acct06", "the client lower-cases the account");
        assert_eq!(ids.secret.as_deref(), Some("secret"));
    }

    /// A field neither shape accounts for is left alone rather than guessed at: substituting a
    /// secret read one byte short would corrupt the recording and hide the rest of it.
    #[test]
    fn an_unrecognised_extra_field_is_not_guessed_at() {
        assert_eq!(extra_string(&[]), None);
        assert_eq!(extra_string(&[0x09, b'a', b'b']), None);
        assert_eq!(extra_string(&[0x02, b'a', b'b']).as_deref(), Some("ab"));
    }
}
