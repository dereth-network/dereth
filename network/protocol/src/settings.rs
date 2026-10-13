//! A character's network setting, and a world's federation flag.
//!
//! Every character starts off the network. `/dn on` puts one on it as `Name@world`, and `/dn off`
//! takes it off again. On the network a character is seen online by anyone, anyone may tell it,
//! and its profile is in the network's directory; squelches and the operator's blocks still hold.
//! The setting is per character, so a character its player keeps off the network stays private.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// One character's network setting. A world that kept more for a character before (who saw it
/// online, who reached it, whether it was listed) reads only this from what it saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharacterSettings {
    /// Whether the character is on the network at all.
    pub on: bool,
}

impl CharacterSettings {
    /// A character off the network: every character until its player puts it on.
    pub const OFF: Self = Self { on: false };

    /// A character on the network.
    pub const ON: Self = Self { on: true };
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

    /// The flag's word, as the registry and the wire spell it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Unset => "unset",
        }
    }
}

impl fmt::Display for Federation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl FromStr for Federation {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            "unset" => Ok(Self::Unset),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: settings: a character is off the network until its player puts it on.
    #[test]
    fn a_character_is_off_the_network_until_it_opts_in() {
        assert_eq!(CharacterSettings::default(), CharacterSettings::OFF);
        assert_ne!(CharacterSettings::OFF, CharacterSettings::ON);
        assert_eq!(
            serde_json::to_string(&CharacterSettings::default()).expect("written"),
            r#"{"on":false}"#
        );
    }

    /// Spec: settings: what a world saved of a character before it kept only `on` reads as that
    /// alone, whatever it said of who saw or reached the character.
    #[test]
    fn a_saved_record_of_more_settings_reads_as_on_alone() {
        let saved = r#"{"on":true,"visible":"nobody","reachable":"friends","listed":"off"}"#;
        assert_eq!(
            serde_json::from_str::<CharacterSettings>(saved).expect("read"),
            CharacterSettings::ON
        );
        assert_eq!(
            serde_json::to_string(&CharacterSettings::ON).expect("written"),
            r#"{"on":true}"#
        );
    }

    /// Spec: settings: the words the registry and the wire use for a world's flag.
    #[test]
    fn the_federation_flag_reads_and_writes_its_word() {
        assert_eq!("On".parse::<Federation>(), Ok(Federation::On));
        assert!("maybe".parse::<Federation>().is_err());
        assert_eq!(Federation::Unset.to_string(), "unset");
        assert!(Federation::On.links() && !Federation::Off.links() && !Federation::Unset.links());
    }
}
