//! The classic interface in the modern interface's place, when the player chooses it, and the
//! choice of interface itself.
//!
//! The client shows one interface at a time, chosen by the `UI.Interface` option, and switches on
//! the frame after the choice changes, in the world or out of it. The classic interface draws from
//! the early-2005 portal (the world's own on a world of that era, else the older portal attached
//! beside a later world) and the host's system fonts; the Horizon interface draws from its own art,
//! which the host hands over. An interface that cannot be shown is refused, the one shown stays,
//! and the chat says why.
//!
//! A switch keeps the game: the character, the selection, the world and everything the server has
//! said are the game's and every interface reads them. Each interface keeps its own windows; the
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
    /// The lines delivered while another interface was shown, which the modern one missed.
    missed: Vec<dereth_client_contract::chat::interface::ChatMessage>,
}

/// Why an interface could not be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The early-2005 portal is not beside the game's files.
    Files,
    /// The host has no system fonts to draw its text.
    Fonts,
    /// The host cannot get the Horizon interface's art: why.
    HorizonArt(String),
    /// The Horizon interface's art is still loading: the choice waits for it.
    HorizonArtLoading,
    /// It came up and failed.
    Failed(String),
}

impl Refusal {
    /// What the chat says.
    #[must_use]
    pub fn notice(&self) -> String {
        match self {
            Self::Files => interface::REQUIRES_CLASSIC_FILES.into(),
            Self::Fonts => interface::REQUIRES_FONTS.into(),
            Self::HorizonArt(why) => why.clone(),
            Self::HorizonArtLoading => interface::HORIZON_ART_LOADING.into(),
            Self::Failed(why) => format!("The interface could not start: {why}"),
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

    /// Keep a copy of the chat lines the chat windows were handed, each once; `modern_shown` is
    /// whether the modern interface is the one they were handed to.
    pub fn remember(
        &mut self,
        lines: &[dereth_client_contract::chat::interface::ChatMessage],
        modern_shown: bool,
    ) {
        for line in lines {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(line.clone());
            if !modern_shown && self.missed.len() < HISTORY {
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

    /// The lines the modern interface missed while another was shown.
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

    /// The choice is left as it is and followed again on the next frame: a choice waiting for
    /// what its interface needs.
    pub fn wait(&mut self) {
        self.seen = None;
    }

    /// The choice is put back to `shown`, the interface still shown, as a refused choice is.
    pub fn refused(&mut self, shown: Interface) {
        dereth_client_contract::options::store::set_value(
            interface::INTERFACE,
            PrefValue::Int(shown.value()),
        );
        self.seen = Some(shown);
        if shown != Interface::Classic {
            self.active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: presentation.interface.a-switch-follows-the-choice-and-a-refused-one-goes-back
    #[test]
    fn a_refused_classic_choice_goes_back_to_the_modern_interface() {
        dereth_client_contract::options::store::init();
        let mut face = ClassicFace::default();
        assert_eq!(face.changed_choice(), Some(Interface::Modern));
        assert_eq!(face.changed_choice(), None);
        dereth_client_contract::options::store::set_value(
            interface::INTERFACE,
            PrefValue::Int(Interface::Classic.value()),
        );
        assert_eq!(face.changed_choice(), Some(Interface::Classic));
        face.refused(Interface::Modern);
        assert_eq!(Interface::chosen(), Interface::Modern);
        assert_eq!(face.changed_choice(), None);
        assert!(face.active_mut().is_none());
    }

    /// Behaviour: none (experimental Horizon interface)
    #[test]
    fn a_refused_horizon_choice_goes_back_to_the_interface_still_shown() {
        dereth_client_contract::options::store::init();
        let mut face = ClassicFace::default();
        let _ = face.changed_choice();
        dereth_client_contract::options::store::set_value(
            interface::INTERFACE,
            PrefValue::Int(Interface::Horizon.value()),
        );
        assert_eq!(face.changed_choice(), Some(Interface::Horizon));
        face.refused(Interface::Classic);
        assert_eq!(Interface::chosen(), Interface::Classic);
        assert_eq!(face.changed_choice(), None);
    }

    /// Behaviour: none (experimental Horizon interface)
    #[test]
    fn lines_the_modern_interface_did_not_see_are_kept_for_it() {
        let mut face = ClassicFace::default();
        let line = |n: usize| dereth_client_contract::chat::interface::ChatMessage {
            body: format!("line {n}"),
            ..Default::default()
        };
        face.remember(&[line(0)], true);
        face.remember(&[line(1)], false);
        assert_eq!(
            face.take_missed()
                .iter()
                .map(|m| m.body.as_str())
                .collect::<Vec<_>>(),
            ["line 1"]
        );
        assert_eq!(face.history().count(), 2);
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
        face.remember(&lines, true);
        assert_eq!(face.history().count(), HISTORY);
        assert_eq!(
            face.history().next().map(|m| m.body.as_str()),
            Some("line 5")
        );
    }
}
