//! The classic interface in the retail interface's place, when the player chooses it.
//!
//! The client shows one interface at a time, chosen by the `UI.Interface` option, and switches on
//! the frame after the choice changes, in the world or out of it. The classic interface draws from
//! the early-2005 portal (the world's own on a world of that era, else the older portal attached
//! beside a later world) and the host's system fonts: without either it is refused, the retail
//! interface stays, and the chat says why.
//!
//! A switch keeps the game: the character, the selection, the world and everything the server has
//! said are the game's and both interfaces read them. Each interface keeps its own windows; the
//! chat history the player has seen is handed to the interface switched to.

use dereth_classic_ui::runtime::ClassicUi;
use dereth_client_contract::options::interface::{self, Interface};
use dereth_client_contract::PrefValue;
use std::collections::VecDeque;

/// How many recent chat lines a switch hands over.
const HISTORY: usize = 200;

/// The classic interface, when it has been brought up, and whether it is the one shown.
#[derive(Debug, Default)]
pub(crate) struct ClassicFace {
    pub ui: Option<ClassicUi>,
    pub active: bool,
    /// The choice last followed.
    seen: Option<Interface>,
    /// The chat lines the game delivered lately, kept for the interface switched to.
    history: VecDeque<dereth_client_contract::chat::interface::ChatMessage>,
    /// The lines delivered while the classic interface was shown, which the retail one missed.
    missed: Vec<dereth_client_contract::chat::interface::ChatMessage>,
}

/// Why the classic interface could not be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The early-2005 portal is not beside the game's files.
    Files,
    /// The host has no system fonts to draw its text.
    Fonts,
    /// It came up and failed.
    Failed(String),
}

impl Refusal {
    /// What the chat says.
    #[must_use]
    pub fn notice(&self) -> String {
        match self {
            Self::Files => interface::REQUIRES_LEGACY_FILES.into(),
            Self::Fonts => interface::REQUIRES_FONTS.into(),
            Self::Failed(why) => format!("The classic interface could not start: {why}"),
        }
    }
}

impl ClassicFace {
    /// The classic interface, while it is the one shown.
    pub fn active_mut(&mut self) -> Option<&mut ClassicUi> {
        if self.active {
            self.ui.as_mut()
        } else {
            None
        }
    }

    /// The classic interface, while it is the one shown.
    pub fn active(&self) -> Option<&ClassicUi> {
        if self.active {
            self.ui.as_ref()
        } else {
            None
        }
    }

    /// Keep a copy of the chat lines the game is about to deliver.
    pub fn remember(&mut self, lines: &[dereth_client_contract::chat::interface::ChatMessage]) {
        for line in lines {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(line.clone());
            if self.active && self.missed.len() < HISTORY {
                self.missed.push(line.clone());
            }
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.missed.clear();
        if let Some(ui) = &mut self.ui {
            ui.classic.chat.clear();
        }
    }

    /// The lines the retail interface missed while the classic one was shown.
    pub fn take_missed(&mut self) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        std::mem::take(&mut self.missed)
    }

    /// The chat history, oldest first.
    pub fn history(
        &self,
    ) -> impl Iterator<Item = &dereth_client_contract::chat::interface::ChatMessage> {
        self.history.iter()
    }

    /// The choice, if it changed since it was last followed.
    pub fn changed_choice(&mut self) -> Option<Interface> {
        let want = Interface::chosen();
        if self.seen == Some(want) {
            return None;
        }
        self.seen = Some(want);
        Some(want)
    }

    /// The choice is put back to the retail interface, as a refused choice is.
    pub fn refused(&mut self) {
        dereth_client_contract::options::store::set_value(
            interface::INTERFACE,
            PrefValue::Int(Interface::Retail.value()),
        );
        self.seen = Some(Interface::Retail);
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: presentation.interface.a-switch-follows-the-choice-and-a-refused-one-goes-back
    #[test]
    fn a_refused_classic_choice_goes_back_to_the_retail_interface() {
        dereth_client_contract::options::store::init();
        let mut face = ClassicFace::default();
        assert_eq!(face.changed_choice(), Some(Interface::Retail));
        assert_eq!(face.changed_choice(), None);
        dereth_client_contract::options::store::set_value(
            interface::INTERFACE,
            PrefValue::Int(Interface::Classic.value()),
        );
        assert_eq!(face.changed_choice(), Some(Interface::Classic));
        face.refused();
        assert_eq!(Interface::chosen(), Interface::Retail);
        assert_eq!(face.changed_choice(), None);
        assert!(face.active_mut().is_none());
    }

    /// Behaviour: presentation.interface.a-switch-follows-the-choice-and-a-refused-one-goes-back
    #[test]
    fn the_chat_history_a_switch_hands_over_keeps_the_latest_lines() {
        let mut face = ClassicFace::default();
        let line = |n: usize| dereth_client_contract::chat::interface::ChatMessage {
            body: format!("line {n}"),
            ..Default::default()
        };
        let lines: Vec<_> = (0..HISTORY + 5).map(line).collect();
        face.remember(&lines);
        assert_eq!(face.history().count(), HISTORY);
        assert_eq!(
            face.history().next().map(|m| m.body.as_str()),
            Some("line 5")
        );
    }
}
