//! The game's questions and messages in the Horizon interface's boxes.
//!
//! What each answer does is the shared dialog service's ([`dereth_client_runtime::dialogs`]): it
//! asks, this keeps the boxes the interface draws and reports each answer back. A fellowship
//! invitation and an offer to swear allegiance are worded from the game's panel strings; the
//! house questions use the house panel's own sentences; every other question carries its sentence.

use std::collections::BTreeMap;

use dereth_client_contract::UiRequest;
use dereth_client_runtime::dialogs::{Answer, DialogPresenter, Prompt, Question, QuestionId};

/// One box on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub id: QuestionId,
    pub text: String,
    /// One OK rather than Yes and No.
    pub message: bool,
    /// It takes every press until it is answered.
    pub modal: bool,
}

/// The boxes, and the answers the interface has given them.
#[derive(Debug, Default)]
pub struct HorizonDialogs {
    /// Which screen the boxes were shown on: a change takes them all down unanswered.
    pub generation: u64,
    shown: Vec<Shown>,
    answers: BTreeMap<QuestionId, bool>,
    /// Answers the service handed back for a window to act on.
    pub delivered: Vec<UiRequest>,
}

impl HorizonDialogs {
    /// The boxes on screen, oldest first.
    #[must_use]
    pub fn shown(&self) -> &[Shown] {
        &self.shown
    }

    /// The box for question `id` was answered: `yes` is Yes, or OK.
    pub fn answer(&mut self, id: QuestionId, yes: bool) {
        if self.shown.iter().any(|s| s.id == id) {
            self.answers.insert(id, yes);
        }
    }
}

/// What a question's box says. `panel` words a panel string-table token for a player's name.
#[must_use]
pub fn prompt_text(prompt: &Prompt, panel: &mut dyn FnMut(&str, &str) -> String) -> String {
    use dereth_ui_screens::panels::{allegiance, fellowship, slumlord};
    match prompt {
        Prompt::Text(text) | Prompt::HouseBuyAs(text) => text.clone(),
        Prompt::FellowshipRequest { name } => panel(fellowship::ID_FELLOWSHIP_REQUEST, name),
        Prompt::AcceptSwear { name } => panel(allegiance::ID_ACCEPT_SWEAR_CONFIRMATION, name),
        Prompt::HouseBuy => slumlord::BUY_CONFIRMATION.to_owned(),
        Prompt::HouseRentByProxy => slumlord::RENT_BY_PROXY_CONFIRMATION.to_owned(),
    }
}

/// The dialog service's view of the boxes for one pass.
pub struct Presenter<'a> {
    pub dialogs: &'a mut HorizonDialogs,
    /// Whether the game screen is up.
    pub accepting: bool,
    /// Words a panel string-table token for a player's name.
    pub panel: &'a mut dyn FnMut(&str, &str) -> String,
}

impl std::fmt::Debug for Presenter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presenter")
            .field("dialogs", &self.dialogs)
            .field("accepting", &self.accepting)
            .finish_non_exhaustive()
    }
}

impl DialogPresenter for Presenter<'_> {
    fn accepting(&self) -> bool {
        self.accepting
    }
    fn generation(&self) -> u64 {
        self.dialogs.generation
    }
    fn open(&mut self, question: &Question) -> bool {
        let text = prompt_text(&question.prompt, self.panel);
        self.dialogs.shown.push(Shown {
            id: question.id,
            text,
            message: question.message,
            modal: question.modal,
        });
        true
    }
    fn poll(&mut self, id: QuestionId) -> Answer {
        if let Some(yes) = self.dialogs.answers.remove(&id) {
            return Answer::Closed(Some(yes));
        }
        if self.dialogs.shown.iter().any(|s| s.id == id) {
            Answer::Waiting
        } else {
            Answer::Closed(None)
        }
    }
    fn close(&mut self, id: QuestionId) {
        self.dialogs.shown.retain(|s| s.id != id);
        self.dialogs.answers.remove(&id);
    }
    fn deliver(&mut self, request: UiRequest) {
        self.dialogs.delivered.push(request);
    }
    fn reset(&mut self) {
        self.dialogs.shown.clear();
        self.dialogs.answers.clear();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface; the answer rules are the dialog service's)
    use super::*;

    fn question(id: QuestionId, message: bool) -> Question {
        Question {
            id,
            prompt: Prompt::Text(format!("question {id}")),
            message,
            modal: false,
            all_at_once: false,
            subject: None,
            target: None,
        }
    }

    #[test]
    fn an_answer_reaches_the_question_its_box_was_shown_for_once() {
        let mut dialogs = HorizonDialogs::default();
        let mut panel = |token: &str, name: &str| format!("{token} {name}");
        let mut p = Presenter {
            dialogs: &mut dialogs,
            accepting: true,
            panel: &mut panel,
        };
        assert!(p.open(&question(7, false)));
        assert_eq!(p.poll(7), Answer::Waiting);
        p.dialogs.answer(7, true);
        p.dialogs.answer(8, true);
        assert_eq!(p.poll(7), Answer::Closed(Some(true)));
        p.close(7);
        assert_eq!(p.poll(7), Answer::Closed(None));
        assert!(p.dialogs.shown().is_empty());
        assert_eq!(p.poll(8), Answer::Closed(None));
    }

    #[test]
    fn an_invitation_is_worded_from_the_panel_strings_with_the_inviter_s_name() {
        let mut panel = |token: &str, name: &str| format!("[{token}] {name}");
        let text = prompt_text(
            &Prompt::FellowshipRequest {
                name: "Aurelia".into(),
            },
            &mut panel,
        );
        assert!(text.ends_with("] Aurelia"), "{text}");
        assert_eq!(
            prompt_text(&Prompt::Text("Sure?".into()), &mut panel),
            "Sure?"
        );
    }

    #[test]
    fn a_new_screen_takes_every_box_down_unanswered() {
        let mut dialogs = HorizonDialogs::default();
        let mut panel = |_: &str, n: &str| n.to_owned();
        let mut p = Presenter {
            dialogs: &mut dialogs,
            accepting: true,
            panel: &mut panel,
        };
        p.open(&question(1, true));
        p.open(&question(2, false));
        p.dialogs.answer(1, true);
        p.reset();
        assert_eq!(p.poll(1), Answer::Closed(None));
        assert!(dialogs.shown().is_empty());
    }
}
