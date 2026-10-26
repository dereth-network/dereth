//! The questions the game asks, for a client with no window: kept until the script answers them
//! (`answer yes` or `answer no`), and printed with the chat lines, as a player would read them in
//! a box. A pop-up message (one button) is read and closed at once.

use std::collections::VecDeque;

use dereth_client_sdk::contract::UiRequest;
use dereth_client_sdk::primitives::LocalTime;
use dereth_client_sdk::runtime::dialogs::{Answer, DialogPresenter, Prompt, Question, QuestionId};
use dereth_client_sdk::runtime::present::Presentation;
use dereth_client_sdk::runtime::shell::{PlainHud, Shell};
use dereth_client_sdk::runtime::ui_context::UiContext;

/// The front end with no UI but the game's questions.
#[derive(Debug, Default)]
pub struct HeadlessShell {
    pub questions: Questions,
}

impl Shell for HeadlessShell {
    type Hud = PlainHud;
    type Present = dyn Presentation;

    fn service_dialogs(&mut self, cx: &mut UiContext<'_, Self>, now: LocalTime) {
        cx.service_dialogs(Some(&mut self.questions), now);
    }
}

/// The questions on screen, oldest first, and what was shown since the script last read it.
#[derive(Debug, Default)]
pub struct Questions {
    /// Each open question: its id, its words, and the script's answer once given.
    open: VecDeque<(QuestionId, String, Option<bool>)>,
    /// Pop-ups read, and questions shown, since the last `take_shown`.
    shown: Vec<String>,
}

impl Questions {
    /// Answer the oldest question still open; its words, or `None` when none is open.
    pub fn answer(&mut self, yes: bool) -> Option<String> {
        let q = self.open.iter_mut().find(|(_, _, a)| a.is_none())?;
        q.2 = Some(yes);
        Some(q.1.clone())
    }

    /// What was shown since the last take: `popup <words>` and `question <words>` lines.
    pub fn take_shown(&mut self) -> Vec<String> {
        std::mem::take(&mut self.shown)
    }
}

/// A question's words, as a box would show them.
fn words(p: &Prompt) -> String {
    match p {
        Prompt::Text(t) => t.trim().to_owned(),
        Prompt::FellowshipRequest { name } => format!("{name} invites you to a fellowship"),
        Prompt::AcceptSwear { name } => format!("{name} asks to swear allegiance to you"),
        Prompt::HouseBuy | Prompt::HouseBuyAs(_) => String::from("Buy this house?"),
        Prompt::HouseRentByProxy => String::from("Pay this house's rent?"),
    }
}

impl DialogPresenter for Questions {
    fn accepting(&self) -> bool {
        true
    }

    fn generation(&self) -> u64 {
        0
    }

    fn open(&mut self, question: &Question) -> bool {
        let text = words(&question.prompt);
        if question.message {
            self.shown
                .push(format!("popup {}", text.replace('\n', " ")));
            self.open.push_back((question.id, text, Some(true)));
        } else {
            self.shown
                .push(format!("question {}", text.replace('\n', " ")));
            self.open.push_back((question.id, text, None));
        }
        true
    }

    fn poll(&mut self, id: QuestionId) -> Answer {
        match self.open.iter().find(|(q, _, _)| *q == id) {
            None => Answer::Closed(None),
            Some((_, _, None)) => Answer::Waiting,
            Some((_, _, Some(yes))) => Answer::Closed(Some(*yes)),
        }
    }

    fn close(&mut self, id: QuestionId) {
        self.open.retain(|(q, _, _)| *q != id);
    }

    fn deliver(&mut self, _request: UiRequest) {}
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the headless client's own question box, answered by its script).
    use super::*;

    fn question(id: QuestionId, text: &str, message: bool) -> Question {
        Question {
            id,
            prompt: Prompt::Text(text.to_owned()),
            message,
            modal: false,
            all_at_once: false,
            subject: None,
            target: None,
        }
    }

    #[test]
    fn a_question_waits_for_the_scripts_answer_and_a_pop_up_closes_at_once() {
        let mut q = Questions::default();
        assert!(q.open(&question(1, "Read this.", true)));
        assert!(q.open(&question(2, "Leave?", false)));
        assert!(q.open(&question(3, "And this?", false)));
        assert_eq!(
            q.take_shown(),
            ["popup Read this.", "question Leave?", "question And this?"]
        );
        assert_eq!(q.poll(1), Answer::Closed(Some(true)));
        assert_eq!(q.poll(2), Answer::Waiting);
        // The oldest question still open is the one answered.
        assert_eq!(q.answer(false).as_deref(), Some("Leave?"));
        assert_eq!(q.poll(2), Answer::Closed(Some(false)));
        assert_eq!(q.poll(3), Answer::Waiting);
        q.close(2);
        assert_eq!(q.poll(2), Answer::Closed(None), "a closed question is gone");
        assert_eq!(q.answer(true).as_deref(), Some("And this?"));
        q.close(3);
        q.close(1);
        assert_eq!(q.answer(true), None, "nothing left to answer");
    }
}
