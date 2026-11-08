//! What the world database says of the tips a player is shown once: at which moment, and the effect
//! whose description is the tip.

use std::collections::BTreeMap;

/// A moment a tip is shown at, the first time it comes for a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Moment {
    /// The character's first entry into the world.
    FirstEntry,
    /// The character's first level gained.
    LevelUp,
    /// The first glyph that comes into the character's hands.
    Glyph,
}

impl Moment {
    /// Every moment, as a row names it.
    pub const WORDS: [(&'static str, Self); 3] = [
        ("first-entry", Self::FirstEntry),
        ("level-up", Self::LevelUp),
        ("glyph", Self::Glyph),
    ];

    /// The moment a row's word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Self::WORDS
            .iter()
            .find(|(word, _)| *word == w)
            .map(|(_, m)| *m)
    }

    /// The word a row names the moment with.
    #[must_use]
    pub fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, m)| *m == self)
            .map_or("", |(w, _)| w)
    }

    /// The mark a character keeps once it has been shown this moment's tip.
    #[must_use]
    pub const fn mark(self) -> u32 {
        match self {
            Self::FirstEntry => 1,
            Self::LevelUp => 2,
            Self::Glyph => 4,
        }
    }

    /// Every moment's mark: what a character kept before tips were shown holds, having passed them.
    pub const ALL_MARKS: u32 = 7;
}

/// The world database's tips: the effect whose description is shown, by moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PopupTable {
    pub by_moment: BTreeMap<Moment, u32>,
}

impl PopupTable {
    /// The effect whose description is a moment's tip, if the world has one.
    #[must_use]
    pub fn effect(&self, m: Moment) -> Option<u32> {
        self.by_moment.get(&m).copied()
    }

    /// The effects the tips show, by their saved states.
    pub fn effects(&self) -> impl Iterator<Item = u32> + '_ {
        self.by_moment.values().copied()
    }
}
