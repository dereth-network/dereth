//! The game's questions in the classic interface's message boxes.
//!
//! What each answer does is the shared dialog service's: it asks, this shows the question in a
//! classic box and reports the answer back. The boxes carry the classic interface's own wording
//! for the questions whose wording is the interface's (a fellowship invitation, an offer to swear
//! allegiance, the house purchase and the rent paid for another), and the service's sentence for
//! the rest.
use crate::desktop::Desktop;
use crate::panels::{HostAction, PanelAction};
use dereth_client_contract::UiRequest;
use dereth_client_runtime::dialogs::{Answer, DialogPresenter, Prompt, Question, QuestionId};
use std::collections::{BTreeMap, BTreeSet};

/// The questions on screen and the answers given to them.
#[derive(Debug, Default)]
pub struct ClassicDialogs {
    /// Which interface framework is up; the front end bumps it when the screens are rebuilt.
    pub generation: u64,
    shown: BTreeSet<QuestionId>,
    answers: BTreeMap<QuestionId, bool>,
    /// Answers the service handed back for a panel to act on.
    pub delivered: Vec<UiRequest>,
}

impl ClassicDialogs {
    /// The box for question `id` was answered.
    pub fn answer(&mut self, id: &str, accepted: bool) {
        if let Some(id) = question_of(id) {
            self.answers.insert(id, accepted);
        }
    }
}

/// The box id a question is shown under.
#[must_use]
pub fn box_id(id: QuestionId) -> String {
    format!("classic-dialog:{id}")
}

fn question_of(id: &str) -> Option<QuestionId> {
    id.strip_prefix("classic-dialog:")?.parse().ok()
}

/// What a question's box says.
#[must_use]
pub fn prompt_text(prompt: &Prompt) -> String {
    match prompt {
        Prompt::Text(text) => text.clone(),
        Prompt::FellowshipRequest { name } => format!(
            "\n{name} has invited you to join their fellowship. Do you accept?\n\n(Default is No)"
        ),
        Prompt::AcceptSwear { name } => format!(
            "\n{name} would like to swear allegiance to you. Do you accept?\n\n(Default is No)"
        ),
        Prompt::HouseBuy => "\n\nWhen you buy a landscape house like this one, you are restricted \
                             from buying another for 30 days. Are you sure you want to buy this \
                             house?\n\n(Default is No)"
            .into(),
        Prompt::HouseRentByProxy => "\n\nYou are paying maintenance on someone else's house. Are \
                                     you sure you wish to continue?\n\n(Default is No)"
            .into(),
    }
}

/// The dialog service's view of the classic boxes for one pass.
#[derive(Debug)]
pub struct Presenter<'a> {
    pub dialogs: &'a mut ClassicDialogs,
    pub desktop: &'a mut Desktop,
    /// Whether the world screen is up.
    pub accepting: bool,
}

impl DialogPresenter for Presenter<'_> {
    fn accepting(&self) -> bool {
        self.accepting
    }
    fn generation(&self) -> u64 {
        self.dialogs.generation
    }
    fn open(&mut self, question: &Question) -> bool {
        let id = box_id(question.id);
        self.desktop.show_dialog(
            id.clone(),
            prompt_text(&question.prompt),
            vec![PanelAction::Host(HostAction::DialogAnswer {
                id: id.clone(),
                accepted: true,
            })],
            vec![PanelAction::Host(HostAction::DialogAnswer {
                id: id.clone(),
                accepted: false,
            })],
        );
        if question.message {
            self.desktop.set_dialog_labels(&id, "OK".into(), None);
        }
        self.dialogs.shown.insert(question.id);
        true
    }
    fn poll(&mut self, id: QuestionId) -> Answer {
        if let Some(yes) = self.dialogs.answers.remove(&id) {
            return Answer::Closed(Some(yes));
        }
        if !self.dialogs.shown.contains(&id) {
            return Answer::Closed(None);
        }
        if self.desktop.dialog_showing(&box_id(id)) {
            Answer::Waiting
        } else {
            Answer::Closed(None)
        }
    }
    fn close(&mut self, id: QuestionId) {
        self.dialogs.shown.remove(&id);
        self.dialogs.answers.remove(&id);
        self.desktop.dismiss_dialog(&box_id(id));
    }
    fn deliver(&mut self, request: UiRequest) {
        self.dialogs.delivered.push(request);
    }
    fn reset(&mut self) {
        for id in std::mem::take(&mut self.dialogs.shown) {
            self.desktop.dismiss_dialog(&box_id(id));
        }
        self.dialogs.answers.clear();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; the answer rules are the dialog service's).
    use super::*;

    #[test]
    fn an_answer_reaches_the_question_its_box_was_shown_for_once() {
        let mut dialogs = ClassicDialogs::default();
        dialogs.shown.insert(7);
        dialogs.answer(&box_id(7), true);
        dialogs.answer("classic-dialog:other", true);
        assert_eq!(dialogs.answers.len(), 1);
        assert_eq!(dialogs.answers.remove(&7), Some(true));
    }

    #[test]
    fn the_interfaces_own_questions_keep_their_classic_wording() {
        let text = prompt_text(&Prompt::FellowshipRequest {
            name: "Lark".into(),
        });
        assert!(text.contains("Lark has invited you to join their fellowship"));
        assert_eq!(prompt_text(&Prompt::Text("Sure?".into())), "Sure?");
    }
}
