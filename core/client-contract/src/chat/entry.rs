//! Device-free operations on one chat entry.

use crate::actions::{chat_entry, ActionId};

/// The remembered sender selected by a reply action or alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyTarget {
    LastTeller,
    Monarch,
    Patron,
}
impl ReplyTarget {
    #[must_use]
    pub fn from_action(action: ActionId) -> Option<Self> {
        match action {
            chat_entry::REPLY => Some(Self::LastTeller),
            chat_entry::MONARCH_REPLY => Some(Self::Monarch),
            chat_entry::PATRON_REPLY => Some(Self::Patron),
            _ => None,
        }
    }
}

/// Entry operations after an interface has applied its input/focus guards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryAction {
    Draft,
    Previous,
    Next,
    RecallLast,
    Reply { target: ReplyTarget, prefix: String },
    ExpandAlias,
    StartTell { name: String },
}

/// A text update consumed once by the active interface, never replayed on a switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryUpdate {
    pub window: u32,
    pub text: String,
    pub cursor: usize,
    pub focus: bool,
}
