//! What the server tells of crafting beyond the retail messages, and the craft questions plugins
//! answer.
//!
//! A server that knows the recipe extension offers it (`0xF7EF`), and then answers what the
//! character's items make used on one another (`0xF7F0`, answered by `0xF7F1`). The answers are kept
//! here, a few at a time, for the plugin that asked to read.
//!
//! A server that shows its chance of success before a combine asks the player to confirm it
//! (`0x0274` of type 5). When the combine was the one the client's interface for plugins sent, the
//! question is the plugin's: it is held here rather than shown, and answered by the plugin, or by
//! the plugin host with No once [`QUESTION_SECONDS`] pass, well inside the server's own wait.

use std::collections::VecDeque;

use dereth_primitives::ObjectId;
use dereth_protocol::admin::RecipeAnswer;

/// The answers kept at most; an older one is dropped for a newer.
pub const ANSWERS_KEPT: usize = 16;
/// How long a plugin's craft question waits for its answer before it is answered No.
pub const QUESTION_SECONDS: f64 = 10.0;
/// How long after the interface for plugins sent a combine a craft question may still be about it.
pub const USE_SECONDS: f64 = 30.0;

/// A craft question the server asked about a plugin's own combine.
#[derive(Debug, Clone, PartialEq)]
pub struct CraftQuestion {
    /// The server's context for the question, which the answer carries back.
    pub context: u32,
    /// The question as the server put it.
    pub text: String,
    /// The chance the text gives, 0 to 1, when it gives one.
    pub chance: Option<f32>,
    /// The combine it is about: the item used, and what it was used on.
    pub item: ObjectId,
    pub target: ObjectId,
    /// When it was asked, by the client's clock.
    pub asked_at: f64,
}

/// The crafting state the extension and the plugins' questions keep.
#[derive(Debug, Clone, Default)]
pub struct Crafting {
    /// The server answers recipe queries.
    pub recipes_offered: bool,
    /// The latest answers, oldest first.
    pub answers: VecDeque<RecipeAnswer>,
    /// The craft questions about a plugin's own combine, unanswered, oldest first.
    pub questions: Vec<CraftQuestion>,
    /// The last combine the interface for plugins sent, and when.
    pub plugin_use: Option<(ObjectId, ObjectId, f64)>,
    /// The last combine the client sent, whoever asked for it.
    pub last_use: Option<(ObjectId, ObjectId)>,
}

impl Crafting {
    /// An answer arrived: kept, the oldest dropped past [`ANSWERS_KEPT`].
    pub fn answered(&mut self, answer: RecipeAnswer) {
        self.answers.retain(|a| a.request != answer.request);
        self.answers.push_back(answer);
        while self.answers.len() > ANSWERS_KEPT {
            self.answers.pop_front();
        }
    }

    /// The answer to request `request`, once it has arrived.
    #[must_use]
    pub fn answer(&self, request: u32) -> Option<&RecipeAnswer> {
        self.answers.iter().find(|a| a.request == request)
    }

    /// The combine a craft question asked `now` is about, when it is the one the interface for
    /// plugins sent last and the client has sent no other since.
    #[must_use]
    pub fn plugins_combine(&self, now: f64) -> Option<(ObjectId, ObjectId)> {
        let (item, target, at) = self.plugin_use?;
        (now - at <= USE_SECONDS && self.last_use == Some((item, target))).then_some((item, target))
    }

    /// Question `context`, taken off the list to be answered.
    pub fn take_question(&mut self, context: u32) -> Option<CraftQuestion> {
        let i = self.questions.iter().position(|q| q.context == context)?;
        Some(self.questions.remove(i))
    }
}

/// The chance a craft question's text gives ("… a 38 percent chance to succeed."), 0 to 1.
#[must_use]
pub fn chance_in(text: &str) -> Option<f32> {
    let at = text.find(" percent chance")?;
    let digits: String = text[..at]
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let n: u32 = digits.parse().ok()?;
    Some((n.min(100) as f32) / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_craft_questions_chance_is_read_from_its_text() {
        assert_eq!(
            chance_in("You determine that you have a 38 percent chance to succeed."),
            Some(0.38)
        );
        assert_eq!(
            chance_in("You determine that you have a 100 percent chance to succeed.\n5 percent is due to your augmentation."),
            Some(1.0)
        );
        assert_eq!(chance_in("Are you sure?"), None);
    }

    #[test]
    fn a_question_is_the_plugins_only_while_its_combine_was_the_last_sent() {
        let (a, b, c) = (ObjectId(1), ObjectId(2), ObjectId(3));
        let mut k = Crafting {
            plugin_use: Some((a, b, 10.0)),
            last_use: Some((a, b)),
            ..Crafting::default()
        };
        assert_eq!(k.plugins_combine(12.0), Some((a, b)));
        assert_eq!(k.plugins_combine(10.0 + USE_SECONDS + 1.0), None);
        // The player combined something else since.
        k.last_use = Some((c, b));
        assert_eq!(k.plugins_combine(12.0), None);
    }

    #[test]
    fn a_question_is_taken_once_and_answers_are_kept_a_few_at_a_time() {
        let q = |context, asked_at| CraftQuestion {
            context,
            text: String::new(),
            chance: None,
            item: ObjectId(1),
            target: ObjectId(2),
            asked_at,
        };
        let mut k = Crafting {
            questions: vec![q(1, 0.0), q(2, 5.0)],
            ..Crafting::default()
        };
        assert_eq!(k.take_question(2).map(|q| q.context), Some(2));
        assert!(k.take_question(2).is_none());
        assert_eq!(k.questions.len(), 1);
        for request in 0..u32::try_from(ANSWERS_KEPT).unwrap() + 3 {
            k.answered(RecipeAnswer {
                request,
                ..RecipeAnswer::default()
            });
        }
        assert_eq!(k.answers.len(), ANSWERS_KEPT);
        assert!(k.answer(0).is_none());
        assert!(k.answer(u32::try_from(ANSWERS_KEPT).unwrap() + 2).is_some());
    }
}
