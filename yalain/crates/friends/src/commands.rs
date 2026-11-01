//! `@dn`, said in chat: the character's place on the Dereth Network.
//!
//! The client handles every `/` command itself, so the network's command is a line said without
//! one: `@dn`, `@dn on`, `@dn off`, `@dn help`. The world takes it before anyone hears it and
//! answers in the speaker's chat window with its own lines.

use dereth_network_protocol::Address;
use yalain_federation::{Federation, Friends as _, LinkState, WORLD_OFF};
use yalain_store::Store;

use crate::names::{read_entry, Target};
use crate::Service;

/// The commands, one line each, as `@dn help` shows them.
pub const HELP: [&str; 6] = [
    "@dn - show your Dereth Network status",
    "@dn on - put this character on the Dereth Network",
    "@dn off - take this character off the Dereth Network",
    "@dn help - show these commands",
    "/tell <Name@world>, <text> - tell a character on another world",
    "/friend add <Name@world> - add a friend on another world",
];

impl Service {
    /// A character said `@dn` and these words after it: the lines to answer it with.
    pub fn said(
        &mut self,
        store: &mut impl Store,
        fed: Option<&mut Federation>,
        character: u64,
        name: &str,
        words: &str,
    ) -> Vec<String> {
        let Some(f) = fed.filter(|f| f.enabled()) else {
            return vec![WORLD_OFF.to_owned()];
        };
        let word = words.split_whitespace().next().unwrap_or_default();
        let address = Address::new(name, &f.settings().slug).ok();
        match word.to_ascii_lowercase().as_str() {
            "" => {
                let Some(a) = address else {
                    return vec![no_address()];
                };
                if !f.is_on(name) {
                    return vec![format!(
                        "You are not on the Dereth Network. Say @dn on to join it as {a}."
                    )];
                }
                let hub = match f.link() {
                    LinkState::Up => "connected",
                    LinkState::Greeting => "connecting",
                    LinkState::Down => "not connected",
                };
                let remote = self.remote_count(character);
                let friends = match remote {
                    0 => "You have no friends on other worlds.".to_owned(),
                    1 => "You have 1 friend on other worlds.".to_owned(),
                    n => format!("You have {n} friends on other worlds."),
                };
                vec![format!(
                    "You are on the Dereth Network as {a}. The hub is {hub}. {friends}"
                )]
            }
            "on" => {
                let Some(a) = address else {
                    return vec![no_address()];
                };
                if f.is_on(name) {
                    return vec![format!("You are on the Dereth Network already, as {a}.")];
                }
                match f.set_on(store, self, character, name, true) {
                    Ok(_) => vec![
                        format!(
                            "You are on the Dereth Network as {a}. Anyone on the network can see \
                             when you are online and send you tells. Say @dn help for the commands."
                        ),
                        "Say @dn off to leave the network.".to_owned(),
                    ],
                    Err(e) => {
                        tracing::warn!("Dereth Network: the store: {e}");
                        vec!["The Dereth Network can't keep that now. Try again later.".to_owned()]
                    }
                }
            }
            "off" => {
                if !f.is_on(name) {
                    return vec!["You are not on the Dereth Network.".to_owned()];
                }
                match f.set_on(store, self, character, name, false) {
                    Ok(_) => {
                        // Nothing of another world is watched for it now: each such friend shows
                        // offline, until its world answers a watch again after `@dn on`.
                        for friend in self.remote_friends(name) {
                            self.show_presence(name, &friend, false, false);
                        }
                        vec![
                            "You are off the Dereth Network. Nobody on another world can see you \
                              or tell you."
                                .to_owned(),
                        ]
                    }
                    Err(e) => {
                        tracing::warn!("Dereth Network: the store: {e}");
                        vec!["The Dereth Network can't keep that now. Try again later.".to_owned()]
                    }
                }
            }
            _ => HELP.iter().map(|l| (*l).to_owned()).collect(),
        }
    }

    /// How many friends on other worlds a character lists.
    fn remote_count(&self, character: u64) -> usize {
        self.lists.get(&character).map_or(0, |l| {
            l.friends
                .iter()
                .filter(|e| matches!(read_entry(e), Target::Remote(_)))
                .count()
        })
    }
}

/// What a character whose name cannot be an address is told.
fn no_address() -> String {
    "This character's name cannot be a network address, so it cannot join the Dereth Network."
        .to_owned()
}
