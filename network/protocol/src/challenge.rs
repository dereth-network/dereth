//! The challenge datagram: what the hub sends a world's game port to prove the world answers at
//! its registered address, when its operator verifies it in game (`docs/network/05-hub.md`
//! section 2.5).
//!
//! The hub sends a fresh random nonce to each of the endpoint's addresses, at its game port. A world
//! that asked the hub to verify it in the last [`WINDOW_SECS`] reads the datagram on its game port
//! and posts the nonce back to the hub over HTTPS, from whatever address its outgoing calls leave
//! by. It never answers over UDP, so the datagram draws nothing back to whoever its source address
//! names.
//!
//! Every datagram starts with a 20-byte header shaped as a packet header's, whose size field
//! ([`IMPOSSIBLE_SIZE`]) is larger than any datagram a server reads: a server that does not know the
//! challenge (ACE, GDLE, or a world with the network off) drops it at the size check, before it
//! reads anything else, as it drops the status ping.
//!
//! This module builds and reads the datagram; the sockets and the HTTPS call are the caller's.

/// The first dword of a challenge, in place of a packet's sequence number.
pub const MAGIC: [u8; 4] = *b"DNVC";

/// The header's size field: more than any datagram's payload can be, so every server's packet
/// reader drops the datagram as truncated.
pub const IMPOSSIBLE_SIZE: u16 = 0xFFFF;

/// The header in front of the challenge.
pub const HEADER_LEN: usize = 20;

/// The byte after the header that says the datagram is a challenge.
pub const KIND: u8 = 0x01;

/// The version of the layout this module writes. A later version only appends fields.
pub const VERSION: u8 = 1;

/// The nonce's length.
pub const NONCE_LEN: usize = 16;

/// The longest slug a challenge carries: a registry slug's longest.
pub const MAX_SLUG_LEN: usize = 40;

/// The longest challenge this version writes.
pub const MAX_LEN: usize = HEADER_LEN + 2 + NONCE_LEN + 1 + MAX_SLUG_LEN;

/// How long after a world asks the hub to verify it the world answers a challenge, in seconds;
/// outside that window it ignores every challenge.
pub const WINDOW_SECS: f64 = 60.0;

/// The most distinct nonces a world answers for one verification: the hub sends one for each
/// address it tries, and no more than these are posted however many datagrams arrive.
pub const MAX_ANSWERS: usize = 8;

/// A challenge: the slug of the world it is for, and the nonce to post back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub slug: String,
    pub nonce: [u8; NONCE_LEN],
}

/// Whether `slug` can be carried: 1 to [`MAX_SLUG_LEN`] bytes of `a-z`, `0-9` and `-`.
fn carried(slug: &str) -> bool {
    (1..=MAX_SLUG_LEN).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `datagram` is shaped as a challenge: the magic and the impossible size. A server checks
/// this before anything else, so a challenge never reaches its packet reader.
#[must_use]
pub fn is_challenge(datagram: &[u8]) -> bool {
    datagram.len() >= HEADER_LEN + 2
        && datagram[..4] == MAGIC
        && datagram[16..18] == IMPOSSIBLE_SIZE.to_le_bytes()
}

impl Challenge {
    /// The datagram, or `None` for a slug that cannot be carried.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        if !carried(&self.slug) {
            return None;
        }
        let mut out = Vec::with_capacity(MAX_LEN);
        out.extend_from_slice(&MAGIC);
        // Flags, checksum, recipient id and time: zero.
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&IMPOSSIBLE_SIZE.to_le_bytes());
        // Iteration: zero.
        out.extend_from_slice(&[0; 2]);
        out.push(KIND);
        out.push(VERSION);
        out.extend_from_slice(&self.nonce);
        out.push(u8::try_from(self.slug.len()).ok()?);
        out.extend_from_slice(self.slug.as_bytes());
        Some(out)
    }

    /// Reads a challenge. Bytes after the fields this version knows are a later version's and are
    /// ignored. `None` for anything else: another kind, version 0, a datagram cut short, or a slug
    /// that cannot be carried.
    #[must_use]
    pub fn parse(datagram: &[u8]) -> Option<Self> {
        if !is_challenge(datagram)
            || datagram[HEADER_LEN] != KIND
            || datagram[HEADER_LEN + 1] == 0
        {
            return None;
        }
        let at = HEADER_LEN + 2;
        let nonce: [u8; NONCE_LEN] = datagram.get(at..at + NONCE_LEN)?.try_into().ok()?;
        let len = usize::from(*datagram.get(at + NONCE_LEN)?);
        let start = at + NONCE_LEN + 1;
        let slug = std::str::from_utf8(datagram.get(start..start + len)?).ok()?;
        carried(slug).then(|| Self {
            slug: slug.to_owned(),
            nonce,
        })
    }
}

/// The nonce as the world posts it back: 32 lowercase hex digits.
#[must_use]
pub fn nonce_hex(nonce: &[u8; NONCE_LEN]) -> String {
    nonce.iter().map(|b| format!("{b:02x}")).collect()
}

/// A nonce read back from its hex form (either case); `None` for anything that is not 32 hex
/// digits.
#[must_use]
pub fn parse_nonce_hex(text: &str) -> Option<[u8; NONCE_LEN]> {
    let b = text.as_bytes();
    if b.len() != NONCE_LEN * 2 {
        return None;
    }
    let digit = |c: u8| char::from(c).to_digit(16).and_then(|d| u8::try_from(d).ok());
    let mut out = [0u8; NONCE_LEN];
    for (o, [high, low]) in out.iter_mut().zip(b.as_chunks::<2>().0) {
        *o = (digit(*high)? << 4) | digit(*low)?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(slug: &str) -> Challenge {
        Challenge {
            slug: slug.to_owned(),
            nonce: [0xA5; NONCE_LEN],
        }
    }

    /// Spec: hub: a challenge reads back as it was written, and a later version's appended bytes
    /// are ignored.
    #[test]
    fn a_challenge_reads_back_and_ignores_what_a_later_version_appends() {
        let c = challenge("coldeve");
        let mut bytes = c.encode().expect("a slug it carries");
        assert_eq!(bytes.len(), HEADER_LEN + 2 + NONCE_LEN + 1 + 7);
        assert_eq!(Challenge::parse(&bytes), Some(c.clone()));
        bytes[HEADER_LEN + 1] = 2;
        bytes.extend_from_slice(&[9, 9, 9]);
        assert_eq!(Challenge::parse(&bytes), Some(c));
        let longest = challenge(&"a".repeat(MAX_SLUG_LEN));
        assert_eq!(longest.encode().map(|b| b.len()), Some(MAX_LEN));
    }

    /// Spec: hub: the challenge's header is a packet header whose size field exceeds the
    /// datagram, which is what makes every server's packet reader drop it.
    #[test]
    fn a_challenge_has_its_magic_and_a_size_larger_than_itself() {
        let d = challenge("aa").encode().expect("a slug it carries");
        assert!(is_challenge(&d));
        assert_eq!(d[..4], MAGIC);
        let size = u16::from_le_bytes([d[16], d[17]]);
        assert!(usize::from(size) > d.len() - HEADER_LEN);
        assert_eq!(d[4..16], [0; 12], "no flags, checksum, recipient or time");
        assert!(!is_challenge(b"DNVC"));
        assert!(!is_challenge(&[0; 40]));
    }

    /// Spec: hub: anything but a whole challenge with a slug it can carry is not read as one.
    #[test]
    fn anything_but_a_whole_challenge_is_refused() {
        let good = challenge("aa").encode().expect("a slug it carries");
        let mut status_ping = good.clone();
        status_ping[..4].copy_from_slice(b"ACSP");
        let mut other_kind = good.clone();
        other_kind[HEADER_LEN] = 0x02;
        let mut version_zero = good.clone();
        version_zero[HEADER_LEN + 1] = 0;
        let mut a_real_size = good.clone();
        a_real_size[16..18].copy_from_slice(&[0x20, 0x00]);
        let mut upper_case = good.clone();
        upper_case[HEADER_LEN + 2 + NONCE_LEN + 1] = b'A';
        let mut no_slug = good.clone();
        no_slug.truncate(HEADER_LEN + 2 + NONCE_LEN + 1);
        no_slug[HEADER_LEN + 2 + NONCE_LEN] = 0;
        for (why, d) in [
            ("the status ping's magic", status_ping),
            ("another kind", other_kind),
            ("version 0", version_zero),
            ("a size a packet could have", a_real_size),
            ("an upper-case slug", upper_case),
            ("no slug", no_slug),
            ("cut in its slug", good[..good.len() - 1].to_vec()),
            ("cut in its nonce", good[..HEADER_LEN + 10].to_vec()),
        ] {
            assert_eq!(Challenge::parse(&d), None, "{why}");
        }
        for slug in ["", "UPPER", "a b", &"a".repeat(MAX_SLUG_LEN + 1)] {
            assert_eq!(challenge(slug).encode(), None, "{slug:?} is not carried");
        }
    }

    /// Spec: hub: the nonce is posted back as 32 hex digits, and nothing else reads as one.
    #[test]
    fn a_nonce_is_posted_back_as_hex() {
        let nonce: [u8; NONCE_LEN] = std::array::from_fn(|i| u8::try_from(i * 17).unwrap_or(0));
        let hex = nonce_hex(&nonce);
        assert_eq!(hex, "00112233445566778899aabbccddeeff");
        assert_eq!(parse_nonce_hex(&hex), Some(nonce));
        assert_eq!(parse_nonce_hex(&hex.to_ascii_uppercase()), Some(nonce));
        for bad in ["", "00", &format!("{hex}0"), &hex.replace('a', "g"), "+0112233445566778899aabbccddeef"] {
            assert_eq!(parse_nonce_hex(bad), None, "{bad:?}");
        }
    }
}
