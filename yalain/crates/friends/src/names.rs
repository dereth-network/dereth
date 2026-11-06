//! Names on the stream: what a name the client wrote names, and how the world writes a name back.
//!
//! A name whose world part is the world's own name (in any case, spaces kept) or its slug, or that
//! has no world part, is a character of this world. Any other is another world's character, by
//! its network address. A character of this world is written back as `Name@` and the world's
//! name exactly as its login names it, so that the client recognizes its own world; another
//! world's as its address.
//!
//! A name may be one in the world as a client shows it, with its suffix ([`Service::set_suffixed`]):
//! the client's own add button and a tell by the name it shows send that. The character is meant,
//! and its own name wins, as in the world's chat.

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
    /// A channel, by its address: told and squelched as a character is, never a friend.
    Channel(Address),
    /// No address: two separators, or a name half that is no name.
    NoSuchUser,
}

/// A name as a list keeps it: a character of this world by its name, another world's by its
/// address.
pub(crate) fn read_entry(entry: &str) -> Target {
    if entry.contains('@') {
        by_address(entry)
    } else {
        Target::Local(entry.to_owned())
    }
}

/// A list's entry for a character of this world under its old name (`from`, in small letters) now
/// names it `new` (`to`, in small letters), keeping its place; a list that held both holds it once.
pub(crate) fn rename(list: &mut Vec<String>, from: &str, to: &str, new: &str) {
    if !list.iter().any(|e| e.to_lowercase() == from) {
        return;
    }
    list.retain(|e| e.to_lowercase() != to);
    for e in list.iter_mut().filter(|e| e.to_lowercase() == from) {
        new.clone_into(e);
    }
}

/// What an address names.
fn by_address(address: &str) -> Target {
    match Address::parse(address) {
        Ok(a) if a.is_channel() => Target::Channel(a),
        Ok(a) => Target::Remote(a),
        Err(_) => Target::NoSuchUser,
    }
}

/// The entry an ignored list keeps for a whole world (`@dn squelch`): no name half, so that it reads
/// as no address, and the client is not shown it.
pub(crate) fn world_entry(world: &str) -> String {
    format!("@{world}")
}

/// Whether an entry of an ignored list is a whole world's.
pub(crate) fn is_world_entry(entry: &str) -> bool {
    entry.starts_with('@')
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
            let lower = name.to_lowercase();
            if !self.here.contains_key(&lower) {
                if let Some(own) = self.suffixed.get(&lower) {
                    return Target::Local(own.clone());
                }
            }
            return if is_character_name(name) {
                Target::Local(name.to_owned())
            } else {
                Target::NoSuchUser
            };
        }
        by_address(user)
    }

    /// A character of this world as the client is shown it.
    #[must_use]
    pub fn written_local(&self, name: &str) -> String {
        format!("{name}@{}", self.config.world_name)
    }
}
