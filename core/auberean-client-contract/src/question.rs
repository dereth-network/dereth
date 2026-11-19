//! The game's questions to the player and its messages, whatever front end shows them
//! ([`Question`]): the player died, someone offers to bring the player back, an effect asks
//! whether it may go on, the scripts' message to read, a fellowship's place offered, an oath
//! offered, a trade asked for.
//!
//! The runtime keeps the questions open in the view ([`crate::view::View::questions`]) from what the
//! player's scripts tell the panels, and takes each down when it is answered, by whichever front
//! end, or when the game takes it back (the offer ran out, the trade closed, the player lives
//! again). A front end answers one with [`crate::Action::AnswerQuestion`], and the runtime does
//! what the answer asks, as the game's own interface does: how a question looks, and how several
//! are shown, is the front end's.

use crate::text::TextRef;

/// A question open: its number, what is asked, its words, and whether it goes in front of the
/// others.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    /// Its number, unique for the client's life; an answer names it.
    pub id: u32,
    pub asked: Asked,
    /// Its words, as the game's own interface writes them (a reference into the game's string
    /// tables, or the scripts' own; empty where the client has no words for it).
    pub text: TextRef,
    /// It goes in front of any question up, which waits behind it, rather than waiting its turn.
    pub at_once: bool,
}

impl Question {
    /// Whether it has one answer (it is read, or taken), rather than yes or no.
    #[must_use]
    pub const fn one_answer(&self) -> bool {
        self.asked.one_answer()
    }
}

/// What a question asks.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Asked {
    /// The player died: its one answer brings the player back to life.
    Died,
    /// Someone offers to bring the player back to life, by name: yes is brought back; no turns
    /// the offer down, and the player is asked again as when it died.
    Resurrection { by: Option<TextRef> },
    /// Whether an effect may go on, from `source` to `target`: yes lets it.
    Effect {
        target: u64,
        source: u64,
        effect: u32,
        question: Option<TextRef>,
    },
    /// A message of the scripts to read: its one answer closes it.
    Message { text: Option<TextRef> },
    /// A fellowship's leader offers the player a place in it: yes takes it, no turns it down.
    Fellowship {
        leader: u64,
        leader_name: Option<TextRef>,
        fellowship: Option<TextRef>,
    },
    /// Another player offers its oath, to be the player's vassal: yes takes it, no refuses it.
    Oath { vassal: u64, name: Option<TextRef> },
    /// Another player asks to trade: yes opens the trade, no closes it.
    Trade { partner: u64, name: Option<TextRef> },
}

impl Asked {
    /// Whether it has one answer, rather than yes or no.
    #[must_use]
    pub const fn one_answer(&self) -> bool {
        matches!(self, Self::Died | Self::Message { .. })
    }
}
