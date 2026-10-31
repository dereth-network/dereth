//! Which game a world runs.
//!
//! The network lists worlds of more than one game: Asheron's Call (`dereth`) and Asheron's Call 2
//! (`auberean`). A world's registration and its heartbeats may name its game; a world that names
//! none runs `dereth`, so everything written before worlds had a game reads as it did. Each game's
//! players are shown that game's worlds alone, since a client of one game cannot play another's.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A world's game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Game {
    /// Asheron's Call: every world that names no game.
    #[default]
    Dereth,
    /// Asheron's Call 2.
    Auberean,
}

impl Game {
    /// Every game, in the order the network lists them.
    pub const ALL: [Self; 2] = [Self::Dereth, Self::Auberean];

    /// The game's word, as registrations, heartbeats and the lists of worlds spell it.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Dereth => "dereth",
            Self::Auberean => "auberean",
        }
    }

    /// Whether this is the game a world that names none runs.
    #[must_use]
    pub fn is_dereth(&self) -> bool {
        *self == Self::Dereth
    }
}

impl fmt::Display for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// Reads a game's word exactly, in small letters: anything else names no game this network knows.
impl FromStr for Game {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        Self::ALL.into_iter().find(|g| g.word() == s).ok_or(())
    }
}

impl Serialize for Game {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.word())
    }
}

impl<'de> Deserialize<'de> for Game {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let word = String::deserialize(d)?;
        word.parse().map_err(|()| {
            serde::de::Error::custom(format!(
                "game: {}, not \"{word}\"",
                Self::ALL.map(Self::word).join(" or ")
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: each game reads and writes its own word, and a world that names none runs
    /// `dereth`.
    #[test]
    fn each_game_reads_and_writes_its_word_and_none_named_is_dereth() {
        for g in Game::ALL {
            assert_eq!(g.word().parse::<Game>(), Ok(g));
            let json = serde_json::to_string(&g).expect("written");
            assert_eq!(json, format!("\"{}\"", g.word()));
            assert_eq!(serde_json::from_str::<Game>(&json).expect("read"), g);
        }
        assert_eq!(Game::default(), Game::Dereth);
        assert!(Game::Dereth.is_dereth() && !Game::Auberean.is_dereth());
    }

    /// Spec: hub: only the words themselves name a game, in small letters and nothing around
    /// them.
    #[test]
    fn anything_but_a_games_own_word_names_no_game() {
        for word in [
            "",
            "Dereth",
            "AUBEREAN",
            " dereth",
            "ac1",
            "ac2",
            "all",
            "auberean2",
        ] {
            assert!(word.parse::<Game>().is_err(), "{word:?}");
            assert!(
                serde_json::from_str::<Game>(&format!("\"{word}\"")).is_err(),
                "{word:?}"
            );
        }
        assert!(serde_json::from_str::<Game>("1").is_err());
    }
}
