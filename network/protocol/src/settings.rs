//! A character's network settings, and a world's federation flag.
//!
//! Every character starts off the network. `/dn on` opts one character in, seen by mutual friends
//! and reachable by anyone ([`CharacterSettings::OPTED_IN`]); each setting can then be widened or narrowed, and
//! `/dn off` takes the character off again. Settings are per character, so an alt or a mule
//! stays hidden unless its owner opts it in too.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Who sees a character's online status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visible {
    /// Nobody: the character always shows offline.
    Nobody,
    /// Mutual friends only: characters it lists that list it too.
    Friends,
    /// Mutual friends and the members of its channels.
    Channels,
    /// Anyone who lists it.
    Anyone,
}

/// Who may send a character a first tell. A character can always be answered by anyone it told
/// within the last [`crate::REPLY_WINDOW_SECS`] seconds, so a conversation it starts is never
/// one-sided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reachable {
    /// Nobody.
    Nobody,
    /// Mutual friends only.
    Friends,
    /// Anyone on a federated world.
    Anyone,
}

/// Whether a character's profile is published in the hub's directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Listed {
    /// Not published.
    Off,
    /// Name, level, heritage and title published (never the inventory).
    On,
}

/// One character's network settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterSettings {
    /// Whether the character is on the network at all.
    pub on: bool,
    pub visible: Visible,
    pub reachable: Reachable,
    pub listed: Listed,
}

impl CharacterSettings {
    /// A character that never opted in: off the network, every setting off.
    pub const OFF: Self = Self {
        on: false,
        visible: Visible::Nobody,
        reachable: Reachable::Nobody,
        listed: Listed::Off,
    };

    /// What `/dn on` gives a character the first time: on, seen by mutual friends only, reachable
    /// by anyone, not listed.
    pub const OPTED_IN: Self = Self {
        on: true,
        visible: Visible::Friends,
        reachable: Reachable::Anyone,
        listed: Listed::Off,
    };
}

impl Default for CharacterSettings {
    fn default() -> Self {
        Self::OFF
    }
}

/// A world's federation flag in the registry: whether the world takes part at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Federation {
    /// The operator turned the network on: the world may link and be addressed.
    On,
    /// The operator said no: the hub refuses everything that names the world, from any source.
    Off,
    /// The operator has said nothing: the world's server features stay off (it may not link).
    Unset,
}

impl Federation {
    /// Whether a world with this flag may hold a link and be addressed by server features.
    #[must_use]
    pub fn links(self) -> bool {
        self == Self::On
    }
}

macro_rules! words {
    ($t:ty { $($v:ident = $s:literal),* $(,)? }) => {
        impl $t {
            /// The setting's word, as `/dn` and the wire spell it.
            #[must_use]
            pub fn word(self) -> &'static str {
                match self { $(Self::$v => $s),* }
            }
        }
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.word())
            }
        }
        impl FromStr for $t {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, ()> {
                match s.trim().to_ascii_lowercase().as_str() {
                    $($s => Ok(Self::$v),)*
                    _ => Err(()),
                }
            }
        }
    };
}

words!(Visible { Nobody = "nobody", Friends = "friends", Channels = "channels", Anyone = "anyone" });
words!(Reachable { Nobody = "nobody", Friends = "friends", Anyone = "anyone" });
words!(Listed { Off = "off", On = "on" });
words!(Federation { On = "on", Off = "off", Unset = "unset" });

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: settings: every setting is off until the character opts in, and opting in makes it
    /// reachable by anyone and visible to mutual friends.
    #[test]
    fn a_character_is_off_the_network_until_it_opts_in() {
        let s = CharacterSettings::default();
        assert!(!s.on);
        assert_eq!(
            (s.visible, s.reachable, s.listed),
            (Visible::Nobody, Reachable::Nobody, Listed::Off)
        );
        let o = CharacterSettings::OPTED_IN;
        assert!(o.on);
        assert_eq!(
            (o.visible, o.reachable, o.listed),
            (Visible::Friends, Reachable::Anyone, Listed::Off),
            "opting in shows the character to mutual friends and lets anyone tell it"
        );
    }

    /// Spec: settings: the words `/dn` and the wire use.
    #[test]
    fn each_setting_reads_and_writes_its_word() {
        assert_eq!("Anyone".parse::<Visible>(), Ok(Visible::Anyone));
        assert_eq!("friends".parse::<Reachable>(), Ok(Reachable::Friends));
        assert!("channel".parse::<Reachable>().is_err());
        assert_eq!(Federation::Unset.to_string(), "unset");
        assert!(Federation::On.links() && !Federation::Off.links() && !Federation::Unset.links());
    }
}
