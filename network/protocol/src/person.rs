//! The person layer's words: what a world may ask the hub about persons, how the hub answers, a
//! person view's person friends and their limits, and the shape of a link code.
//!
//! A person is a Discord account, as the hub knows it; a player links characters to it on the
//! site, with a one-time code its world asks the hub for. The hub keeps persons; a world only asks,
//! for its own characters, and shows what the hub answers (`docs/network/02-link-protocol.md`
//! section 6).

use serde::{Deserialize, Serialize};

use crate::address::Address;

/// The most person friends that may hold for one character, and a person view names: as many
/// friends as the final retail client's list holds.
pub const MAX_PERSON_FRIENDS: usize = 100;

/// The most characters one person friend lists in a view, and a `characters` answer names: as
/// many as one person may link.
pub const MAX_PERSON_CHARACTERS: usize = 20;

/// The most characters a person view names in all: the most remote friends a character lists.
pub const MAX_VIEW_CHARACTERS: usize = 100;

/// The most characters one `carry` request names: as many friends as a character lists.
pub const MAX_CARRIED: usize = 100;

/// The longest a link code may last, in seconds, as a result may say.
pub const MAX_CODE_SECS: u32 = 3_600;

/// The longest page address a result may name.
pub const MAX_URL_CHARS: usize = 200;

/// The symbols of a link code: Crockford's base 32, which has no `I`, `L`, `O` or `U`.
pub const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// What a person request asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonOp {
    /// A one-time code to link `who` on the site.
    Code,
    /// Unlink `who` from its person.
    Unlink,
    /// Sent by worlds of the network's first version alone, which kept a setting of the person's
    /// that hid its characters from each other (`shown`): nothing of a person is hidden now, and
    /// the hub changes nothing and refuses it, naming the site's page.
    Alts,
    /// List `target`'s person as a person friend of `who`'s person.
    Befriend,
    /// Remove the person friend whose handle is `target`.
    Unfriend,
    /// The characters of `target`'s person. Sent by worlds of the network's first version alone.
    Characters,
    /// List each of `targets`' persons as a person friend of `who`'s person, as `befriend` does,
    /// until the limit: the friends `who` had before it was linked.
    Carry,
}

impl PersonOp {
    /// Whether a request of this kind names a `target` (a `carry` names its `targets`).
    #[must_use]
    pub fn needs_target(self) -> bool {
        matches!(self, Self::Befriend | Self::Unfriend | Self::Characters)
    }
}

/// How a person request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonOutcome {
    Ok,
    /// The request needs `who` to be linked to a person, and it is not.
    NotLinked,
    /// Over a limit.
    RateLimited,
    /// This hub does not link characters (it has no sign-in).
    Unavailable,
    /// A limit on the person (how many person friends it may have).
    Refused,
}

/// The `alts` word a person view carries for worlds of the network's first version, which read it:
/// always `shown`, since nothing of a person is hidden. A reader takes any word, or none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EarlierAlts;

impl EarlierAlts {
    /// The word the hub writes.
    pub const WORD: &'static str = "shown";
}

impl Serialize for EarlierAlts {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(Self::WORD)
    }
}

impl<'de> Deserialize<'de> for EarlierAlts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(|_| Self)
    }
}

/// One person friend in a person view: the handle it was added by, then the characters it lists,
/// the handle first among them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonFriend {
    pub handle: Address,
    pub characters: Vec<Address>,
}

/// `persons` cut to a view's limits, in order: past [`MAX_PERSON_FRIENDS`] person friends and
/// past [`MAX_VIEW_CHARACTERS`] handles the rest are left out; then each keeps its handle, and
/// its other characters, at most [`MAX_PERSON_CHARACTERS`] in all for one, take what is left of
/// [`MAX_VIEW_CHARACTERS`] in order. So a person friend with many characters never pushes another
/// person friend out of the view.
#[must_use]
pub fn within_view_limits(persons: Vec<PersonFriend>) -> Vec<PersonFriend> {
    let mut out: Vec<PersonFriend> = persons
        .into_iter()
        .take(MAX_PERSON_FRIENDS.min(MAX_VIEW_CHARACTERS))
        .collect();
    let handles: usize = out.iter().map(|p| p.characters.len().min(1)).sum();
    let mut left = MAX_VIEW_CHARACTERS - handles;
    for p in &mut out {
        let own = p.characters.len().min(1);
        let others = p
            .characters
            .len()
            .min(MAX_PERSON_CHARACTERS)
            .saturating_sub(own)
            .min(left);
        left -= others;
        p.characters.truncate(own + others);
    }
    out
}

/// Whether `persons` is within a view's limits, and names no channel.
#[must_use]
pub fn is_within_view_limits(persons: &[PersonFriend]) -> bool {
    persons.len() <= MAX_PERSON_FRIENDS
        && persons
            .iter()
            .all(|p| p.characters.len() <= MAX_PERSON_CHARACTERS)
        && persons.iter().map(|p| p.characters.len()).sum::<usize>() <= MAX_VIEW_CHARACTERS
        && persons
            .iter()
            .all(|p| !p.handle.is_channel() && p.characters.iter().all(|c| !c.is_channel()))
}

/// Whether `s` is a link code as the hub writes one: two groups of four of [`CODE_ALPHABET`],
/// joined by a hyphen.
#[must_use]
pub fn is_link_code(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 9
        && b[4] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || CODE_ALPHABET.contains(c))
}

/// A code as a player entered it, in the form the hub writes it: case, spaces and hyphens do not
/// matter, `I` and `L` are read as `1` and `O` as `0`. `None` when it is not eight symbols of the
/// alphabet.
#[must_use]
pub fn read_link_code(entered: &str) -> Option<String> {
    let mut symbols = String::with_capacity(9);
    for c in entered.chars() {
        let c = match c.to_ascii_uppercase() {
            ' ' | '-' => continue,
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        };
        match u8::try_from(c) {
            Ok(b) if CODE_ALPHABET.contains(&b) => {}
            _ => return None,
        }
        if symbols.len() == 4 {
            symbols.push('-');
        }
        symbols.push(c);
        if symbols.len() > 9 {
            return None;
        }
    }
    is_link_code(&symbols).then_some(symbols)
}

/// A link code from 40 random bits, as the hub writes it.
#[must_use]
pub fn link_code_from(bits: [u8; 5]) -> String {
    let mut n = bits.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
    let mut symbols = [0u8; 8];
    for s in symbols.iter_mut().rev() {
        *s = CODE_ALPHABET[usize::try_from(n & 31).unwrap_or(0)];
        n >>= 5;
    }
    let text = String::from_utf8_lossy(&symbols).into_owned();
    format!("{}-{}", &text[..4], &text[4..])
}

/// Whether `s` may be a result's page address: 1 to [`MAX_URL_CHARS`] printable ASCII characters,
/// none of them a space, an angle bracket or a quote, so a world can put it in a line as it is.
#[must_use]
pub fn is_page_url(s: &str) -> bool {
    (1..=MAX_URL_CHARS).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'<' | b'>' | b'"' | b'\''))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// Spec: link messages: a link code is two groups of four of Crockford's base 32, and an
    /// entered one is read without regard to case, spaces or hyphens, `I` and `L` as `1`, `O` as
    /// `0`.
    #[test]
    fn a_link_code_is_read_however_it_was_typed() {
        assert!(is_link_code("K7QD-2MXP"));
        for bad in [
            "K7QD2MXP",
            "K7QD-2MXU",
            "k7qd-2mxp",
            "K7QD-2MX",
            "K7QD-2MXPP",
        ] {
            assert!(!is_link_code(bad), "{bad}");
        }
        for typed in ["k7qd-2mxp", " K7QD 2MXP ", "k7qd2mxp", "K7-QD-2M-XP"] {
            assert_eq!(
                read_link_code(typed).as_deref(),
                Some("K7QD-2MXP"),
                "{typed}"
            );
        }
        assert_eq!(read_link_code("ilo0-1111").as_deref(), Some("1100-1111"));
        for bad in [
            "",
            "K7QD-2MX",
            "K7QD-2MXPQ",
            "K7QD-2MXU",
            "K7QD-2MX!",
            "K7QD-2MXÉ",
        ] {
            assert_eq!(read_link_code(bad), None, "{bad}");
        }
    }

    /// Spec: link messages: a code written from 40 bits is a well-formed code, and different bits
    /// write different codes.
    #[test]
    fn a_code_from_random_bits_is_well_formed() {
        let all_zero = link_code_from([0; 5]);
        let all_one = link_code_from([0xFF; 5]);
        assert_eq!(all_zero, "0000-0000");
        assert_eq!(all_one, "ZZZZ-ZZZZ");
        assert!(is_link_code(&link_code_from([1, 2, 3, 4, 5])));
        assert_ne!(
            link_code_from([1, 2, 3, 4, 5]),
            link_code_from([1, 2, 3, 4, 6])
        );
    }

    /// Spec: link messages: a view over its limits is cut in order, past 100 person friends,
    /// each keeping its handle before any person friend's other characters take what is left of
    /// 100 characters in all, at most 20 for one.
    #[test]
    fn a_view_over_its_limits_is_cut_in_order() {
        // Names hold letters only: a number written in letters.
        let letters = |mut n: usize| {
            let mut s = String::new();
            loop {
                s.push(char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
                if n == 0 {
                    return s;
                }
            }
        };
        let friend = |i: usize, n: usize| PersonFriend {
            handle: a(&format!("Handle {}@cc", letters(i))),
            characters: (0..n)
                .map(|j| a(&format!("Char {} {}@cc", letters(i), letters(j))))
                .collect(),
        };
        let many: Vec<PersonFriend> = (0..61).map(|i| friend(i, 3)).collect();
        let cut = within_view_limits(many);
        assert_eq!(cut.len(), 61, "every person friend keeps its handle");
        assert_eq!(cut.iter().map(|p| p.characters.len()).sum::<usize>(), 100);
        assert_eq!(cut[18].characters.len(), 3, "the first 19 keep all three");
        assert_eq!(cut[19].characters.len(), 2, "the next one what is left");
        assert_eq!(cut[20].characters.len(), 1, "the rest their handle alone");
        assert!(is_within_view_limits(&cut));
        let crowded: Vec<PersonFriend> = (0..120)
            .map(|i| friend(i, if i < 5 { 20 } else { 1 }))
            .collect();
        let cut = within_view_limits(crowded);
        assert_eq!(cut.len(), MAX_PERSON_FRIENDS);
        assert!(cut.iter().all(|p| !p.characters.is_empty()));
        assert!(is_within_view_limits(&cut));
        let long = within_view_limits(vec![friend(1, 30)]);
        assert_eq!(long[0].characters.len(), MAX_PERSON_CHARACTERS);
        assert!(!is_within_view_limits(&[friend(1, 21)]));
    }

    /// Spec: link messages: a page address goes in a line as it is, so it holds no space, angle
    /// bracket or quote.
    #[test]
    fn a_page_address_holds_nothing_a_line_would_read_as_markup() {
        assert!(is_page_url("https://dereth.network/characters/"));
        for bad in [
            "",
            "https://x/ y",
            "https://x/<Tell:IIDString:1:a>",
            "https://x/\"",
            "https://x/\u{e9}",
        ] {
            assert!(!is_page_url(bad), "{bad:?}");
        }
        assert!(!is_page_url(&format!("https://{}", "x".repeat(200))));
    }
}
