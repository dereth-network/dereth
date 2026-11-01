//! The operator's network commands, typed at the server's console: the blocks, and the state of
//! the world's link.
//!
//! ```text
//! dn                                  the world's link to the network, and what is blocked
//! dn block <world or Name@world>      block a world or a character for everyone on this world
//! dn unblock <world or Name@world>    lift a block
//! dn help                             these lines
//! ```
//!
//! A block refuses what the world or character sends (a tell is answered `offline`, a watch
//! `offline` and then told nothing), and what this world's characters send it (a tell ends as to
//! a character not available); it takes hold at once, and lifting it gives back what it took.

use dereth_network_protocol::address::is_slug;
use dereth_network_protocol::Address;
use yalain_store::Store;

use crate::{Federation, Friends, LinkState, WORLD_OFF};

/// The commands, one line each.
pub const HELP: &[&str] = &[
    "dn - show the world's link to the Dereth Network and what is blocked",
    "dn block <world or Name@world> - block a world or a character for everyone on this world",
    "dn unblock <world or Name@world> - lift a block",
];

impl Federation {
    /// A `dn` command typed at the console, without its `dn` (`block coldeve`): the lines to
    /// answer it with. The blocks are kept in the store.
    pub fn operator(
        &mut self,
        store: &mut impl Store,
        friends: &mut impl Friends,
        words: &[&str],
    ) -> Vec<String> {
        let help = || HELP.iter().map(|&l| l.to_owned()).collect();
        if !matches!(words, [] | ["block" | "unblock", ..]) {
            return help();
        }
        if !self.enabled() {
            return vec![WORLD_OFF.to_owned()];
        }
        let (op, named) = match words {
            [] | ["block" | "unblock"] => return self.status(),
            [op, rest @ ..] => (*op, rest.join(" ")),
        };
        let key = match Address::parse(&named) {
            Ok(a) => a.key(),
            Err(_) if is_slug(&named.to_ascii_lowercase()) => named.to_ascii_lowercase(),
            Err(_) => return vec![HELP[if op == "block" { 1 } else { 2 }].to_owned()],
        };
        let add = op == "block";
        let held = self.blocked.contains(&key);
        if add == held {
            return vec![if add {
                format!("{key} was already blocked.")
            } else {
                format!("{key} was not blocked.")
            }];
        }
        if let Err(e) = store.set_network_block(&key, add) {
            return vec![format!("The block could not be kept: {e}")];
        }
        if add {
            // What the block covers is told this world's characters are offline first.
            self.block_beginning(friends, &key);
            self.blocked.insert(key.clone());
            vec![format!("{key} is blocked for everyone on this world.")]
        } else {
            self.blocked.remove(&key);
            self.block_ended(friends, &key);
            vec![format!("{key} is no longer blocked.")]
        }
    }

    /// The world's link to the network, then what is blocked.
    fn status(&self) -> Vec<String> {
        let hub = match self.link {
            LinkState::Up => "connected",
            LinkState::Greeting => "connecting",
            LinkState::Down => "not connected",
        };
        let online = match self.worlds_online() {
            1 => "1 world is".to_owned(),
            n => format!("{n} worlds are"),
        };
        let queued = match self.queued() {
            1 => "1 message is".to_owned(),
            n => format!("{n} messages are"),
        };
        let blocked = if self.blocked.is_empty() {
            "Nothing is blocked.".to_owned()
        } else {
            format!("Blocked: {}.", self.blocks().join(", "))
        };
        vec![
            format!(
                "This world is on the Dereth Network as {}. The hub is {hub}. {online} online. \
                 {queued} waiting for the hub.",
                self.settings.slug
            ),
            blocked,
        ]
    }
}
