//! Names on the stream: what a name the client wrote names, and how the world writes a name back.
//!
//! A name whose world part is the world's own name (in any case, spaces kept) or its slug, or that
//! has no world part, is a character of this world. Any other is another world's character, by
//! its network address. A character of this world is written back as `Name@` and the world's
//! name exactly as its login names it, so that the client recognizes its own world; another
//! world's as its address.

use dereth_network_protocol::address::is_character_name;
use dereth_network_protocol::Address;

use crate::Service;

/// What a name names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A character of this world, by the name half as written.
    Local(String),
    /// Another world's character.
    Remote(Address),
    /// No address: two separators, a name half that is no name, or a channel.
    NoSuchUser,
}

/// A name as a list keeps it: a character of this world by its name, another world's by its
/// address.
pub(crate) fn read_entry(entry: &str) -> Target {
    if entry.contains('@') {
        match Address::parse(entry) {
            Ok(a) if !a.is_channel() => Target::Remote(a),
            _ => Target::NoSuchUser,
        }
    } else {
        Target::Local(entry.to_owned())
    }
}

impl Service {
    /// What a name the client wrote names.
    #[must_use]
    pub fn target(&self, user: &str) -> Target {
        let mut parts = user.split('@');
        let (Some(name), world, None) = (parts.next(), parts.next(), parts.next()) else {
            return Target::NoSuchUser;
        };
        let name = name.trim();
        let local = match world {
            None => true,
            Some(w) => {
                w.eq_ignore_ascii_case(&self.config.world_name)
                    || (!self.config.slug.is_empty()
                        && w.trim().eq_ignore_ascii_case(&self.config.slug))
            }
        };
        if local {
            return if is_character_name(name) {
                Target::Local(name.to_owned())
            } else {
                Target::NoSuchUser
            };
        }
        match Address::parse(user) {
            Ok(a) if !a.is_channel() => Target::Remote(a),
            _ => Target::NoSuchUser,
        }
    }

    /// A character of this world as the client is shown it.
    #[must_use]
    pub fn written_local(&self, name: &str) -> String {
        format!("{name}@{}", self.config.world_name)
    }
}
