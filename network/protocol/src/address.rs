//! Addresses: `Name@world`, the one way a character, or a channel, is named across worlds.
//!
//! An address is a character name, the separator [`SEPARATOR`], and the slug of the character's
//! home world, as the registry knows it: `Bob@coldeve`. A channel is addressed the same way with
//! the reserved world [`CHANNEL_WORLD`]: `oldguard@channel`. The world half is what makes an
//! address unmistakable for a local name (no character name may hold the separator) and what keeps
//! a vanity world from passing as staff: the world always shows.
//!
//! Matching is case-insensitive on both halves (as a world matches its own character names); the
//! name keeps the case it was written with for display, and the world half is always written in
//! lower case.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The character between the name and the world.
pub const SEPARATOR: char = '@';

/// The reserved world half that names a channel rather than a character's home world.
pub const CHANNEL_WORLD: &str = "channel";

/// The longest character name an address may carry, in characters.
pub const MAX_NAME_CHARS: usize = 32;

/// The shortest and longest world slug an address may carry. The registry allows longer slugs,
/// but a world must have one of at most [`MAX_SLUG_CHARS`] to join the network, so that the whole
/// address fits the friends panel and the chat line.
pub const MIN_SLUG_CHARS: usize = 2;
/// See [`MIN_SLUG_CHARS`].
pub const MAX_SLUG_CHARS: usize = 12;

/// The shortest and longest channel name.
pub const MIN_CHANNEL_CHARS: usize = 2;
/// See [`MIN_CHANNEL_CHARS`].
pub const MAX_CHANNEL_CHARS: usize = 20;

/// The slugs no world may take on the network: the nine retail worlds' names (as the community
/// lists already reserve them), the channel pseudo-world, the world kept for speakers from Discord
/// and the hub's own words.
pub const RESERVED_SLUGS: &[&str] = &[
    "darktide",
    "frostfell",
    "harvestgain",
    "leafcull",
    "morningthaw",
    "solclaim",
    "thistledown",
    "verdantine",
    "wintersebb",
    CHANNEL_WORLD,
    "discord",
    "hub",
    "net",
    "network",
    "local",
    "localhost",
    "world",
    "all",
    "any",
];

/// Why a string is not an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    /// No separator, or more than one.
    Separator,
    /// The name half is empty, too long, or holds a character no name may hold.
    Name,
    /// The world half is not a slug of [`MIN_SLUG_CHARS`] to [`MAX_SLUG_CHARS`] lower-case
    /// letters, digits and single inner hyphens.
    World,
    /// The world half is [`CHANNEL_WORLD`] and the name half is not a channel name.
    Channel,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Separator => "an address is one name, one @ and one world",
            Self::Name => "the name half is not a character name",
            Self::World => "the world half is not a world's network name",
            Self::Channel => "the name half is not a channel name",
        })
    }
}

impl std::error::Error for AddressError {}

/// A parsed address. Two addresses are the same identity when [`Address::key`] is equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address {
    name: String,
    world: String,
}

impl Address {
    /// Parses `Name@world`. Surrounding white space is trimmed from each half, a leading `+`
    /// (the administrator marker) is removed from the name, and the world half is lower-cased.
    ///
    /// # Errors
    /// When the string is not an address (see [`AddressError`]).
    pub fn parse(s: &str) -> Result<Self, AddressError> {
        let mut parts = s.split(SEPARATOR);
        let (Some(name), Some(world), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(AddressError::Separator);
        };
        Self::new(name, world)
    }

    /// An address from its halves, checked as [`Address::parse`] checks them.
    ///
    /// # Errors
    /// When either half is not valid.
    pub fn new(name: &str, world: &str) -> Result<Self, AddressError> {
        let world = world.trim().to_ascii_lowercase();
        if !is_slug(&world) {
            return Err(AddressError::World);
        }
        let name = name.trim();
        let name = name.strip_prefix('+').unwrap_or(name).trim();
        if world == CHANNEL_WORLD {
            if !is_channel_name(name) {
                return Err(AddressError::Channel);
            }
            return Ok(Self {
                name: name.to_ascii_lowercase(),
                world,
            });
        }
        if !is_character_name(name) {
            return Err(AddressError::Name);
        }
        Ok(Self {
            name: name.to_owned(),
            world,
        })
    }

    /// The name half, as written (a channel's in lower case).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The world half: a registry slug, or [`CHANNEL_WORLD`].
    #[must_use]
    pub fn world(&self) -> &str {
        &self.world
    }

    /// Whether this names a channel.
    #[must_use]
    pub fn is_channel(&self) -> bool {
        self.world == CHANNEL_WORLD
    }

    /// The identity key: the address in lower case. Two addresses that differ only in case name
    /// the same character.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}{SEPARATOR}{}", self.name.to_lowercase(), self.world)
    }

    /// Whether `other` names the same identity.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        self.world == other.world && self.name.to_lowercase() == other.name.to_lowercase()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.name, self.world)
    }
}

impl Serialize for Address {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Address::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Whether `s` holds the separator, which is how a world tells a network address from a local
/// name before it parses it.
#[must_use]
pub fn looks_remote(s: &str) -> bool {
    s.contains(SEPARATOR)
}

/// Whether `s` is a network world slug: [`MIN_SLUG_CHARS`] to [`MAX_SLUG_CHARS`] of `a-z` and
/// `0-9`, with single hyphens between them (the registry's slug form, shortened).
#[must_use]
pub fn is_slug(s: &str) -> bool {
    let len = s.chars().count();
    (MIN_SLUG_CHARS..=MAX_SLUG_CHARS).contains(&len)
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Whether a world may take `slug` on the network: a slug, and not reserved.
#[must_use]
pub fn is_network_slug(slug: &str) -> bool {
    is_slug(slug) && !RESERVED_SLUGS.contains(&slug)
}

/// Whether `s` is a channel name: [`MIN_CHANNEL_CHARS`] to [`MAX_CHANNEL_CHARS`] of `a-z`, `A-Z`
/// and `0-9`, with single hyphens between them. Channel names are matched and written in lower
/// case.
#[must_use]
pub fn is_channel_name(s: &str) -> bool {
    let len = s.chars().count();
    (MIN_CHANNEL_CHARS..=MAX_CHANNEL_CHARS).contains(&len)
        && s.split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Whether `s` may be the name half of a character's address.
///
/// A name is 1 to [`MAX_NAME_CHARS`] characters, starts and ends with a letter, and holds only
/// letters, single spaces, apostrophes and hyphens: the characters the final retail client lets a
/// player give a new character. So it never holds the separator, a comma (which ends the name in a
/// typed tell), a markup bracket, a control character, or `^` or `&` (which the client reads, from
/// anywhere in a speaker's name, as the Olthoi speech marks). Letters are any of Windows-1252's
/// (`Ëlwen`), because every client can show those and other worlds' rules may allow them.
#[must_use]
pub fn is_character_name(s: &str) -> bool {
    let count = s.chars().count();
    if count == 0 || count > MAX_NAME_CHARS {
        return false;
    }
    let is_letter = |c: char| c.is_alphabetic() && crate::text::cp1252_exact(c);
    let first = s.chars().next().is_some_and(is_letter);
    let last = s.chars().next_back().is_some_and(is_letter);
    first
        && last
        && s.chars()
            .all(|c| is_letter(c) || matches!(c, ' ' | '\'' | '-'))
        && !s.contains("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: addresses: an address is one name, one separator and one world slug.
    #[test]
    fn an_address_is_a_name_the_separator_and_a_world_slug() {
        let a = Address::parse("Bob@coldeve").expect("an address");
        assert_eq!((a.name(), a.world()), ("Bob", "coldeve"));
        assert_eq!(a.to_string(), "Bob@coldeve");
        assert_eq!(
            Address::parse("Bob@Coldeve").expect("an address").world(),
            "coldeve",
            "the world half is written in lower case"
        );
        assert_eq!(
            Address::parse(" +Bob @ coldeve ")
                .expect("an address")
                .to_string(),
            "Bob@coldeve",
            "white space trimmed and the administrator marker removed"
        );
        for bad in [
            "Bob",
            "Bob@",
            "@coldeve",
            "Bob@cold@eve",
            "Bob@c",
            "Bob@cold_eve",
        ] {
            assert!(Address::parse(bad).is_err(), "{bad} is not an address");
        }
    }

    /// Spec: addresses: matching is case-insensitive and the world always shows.
    #[test]
    fn two_addresses_differing_only_in_case_are_one_identity() {
        let a = Address::parse("Bob@coldeve").expect("an address");
        let b = Address::parse("BOB@COLDEVE").expect("an address");
        assert!(a.same(&b));
        assert_eq!(a.key(), b.key());
        assert_ne!(
            a.key(),
            Address::parse("Bob@frostfell2").expect("an address").key()
        );
    }

    /// Spec: addresses: a name never ends in an Olthoi speech mark nor holds markup or a comma.
    #[test]
    fn a_name_refuses_the_olthoi_marks_markup_commas_and_overlong_names() {
        for bad in [
            "Bob^",
            "Bob&",
            "Bo,b",
            "<Bob>",
            "Bob\n",
            "",
            "a234567890123456789012345678901234",
            "Bob  Smith",
            "St. Ann",
            "Bo^b",
        ] {
            assert!(!is_character_name(bad), "{bad:?} is not a name");
        }
        for good in ["Bob", "Ëlwen", "Mary-Jane", "O'Neil", "Sir Bob"] {
            assert!(is_character_name(good), "{good:?} is a name");
        }
    }

    /// Spec: addresses: a channel is addressed at the reserved world `channel`, in lower case.
    #[test]
    fn a_channel_is_addressed_at_the_reserved_channel_world() {
        let c = Address::parse("OldGuard@channel").expect("a channel address");
        assert!(c.is_channel());
        assert_eq!(c.to_string(), "oldguard@channel");
        assert_eq!(
            Address::parse("Old Guard@channel"),
            Err(AddressError::Channel)
        );
        assert!(
            !is_network_slug("channel"),
            "no world may take the channel's slug"
        );
        assert!(
            !is_network_slug("darktide"),
            "the retail names are reserved"
        );
        assert!(is_network_slug("coldeve"));
        assert!(!is_slug("abcdefghijklm"), "thirteen characters is too long");
    }
}
