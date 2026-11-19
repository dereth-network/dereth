//! The game's questions in Horizon's box: which is up, its words, its buttons and what each
//! answers.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::question::{Asked, Question};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::View;
use auberean_client_sdk::Action;
use auberean_horizon::state::{Asking, Frame, GameState};
use auberean_horizon::ui::questions::{kind_title, labels, title};

use crate::{in_world, Harness};

fn asking(id: u32, asked: Asked, at_once: bool) -> Asking {
    Asking {
        id,
        one_answer: asked.one_answer(),
        asked,
        text: format!("question {id}"),
        at_once,
    }
}

fn fellowship() -> Asked {
    Asked::Fellowship {
        leader: 9,
        leader_name: Some(TextRef::literal("Cara")),
        fellowship: Some(TextRef::literal("The Hunt")),
    }
}

/// The words a reference is written as: a literal as itself, string 1 of a table as "From the
/// game", anything else not at all.
fn write(t: &TextRef) -> Option<String> {
    if let Some(l) = &t.literal {
        return Some(String::from_utf16_lossy(l));
    }
    (t.string == 1).then(|| "From the game".to_owned())
}

fn question(id: u32, asked: Asked, string: u32) -> Question {
    Question {
        id,
        asked,
        text: TextRef {
            string,
            ..TextRef::default()
        },
        at_once: false,
    }
}

#[test]
fn a_question_is_worded_from_the_game_or_else_in_plain_words_with_its_names() {
    let view = View {
        questions: vec![
            question(1, fellowship(), 1),
            question(2, fellowship(), 0),
            question(3, Asked::Died, 0),
            question(
                4,
                Asked::Trade {
                    partner: 9,
                    name: None,
                },
                0,
            ),
        ],
        ..View::default()
    };
    let mut s = GameState::default();
    s.follow(&Frame {
        view: Some(&view),
        notices: &[],
        events: &[],
        told: &[],
        lines: &[],
        write: &write,
        unknown_command: "",
    });
    let words: Vec<(u32, &str, bool)> = s
        .questions
        .iter()
        .map(|q| (q.id, q.text.as_str(), q.one_answer))
        .collect();
    assert_eq!(
        words,
        vec![
            (1, "From the game", false),
            (
                2,
                "Cara invites you to join the The Hunt fellowship. Will you join?",
                false
            ),
            (3, "You have died.", true),
            (4, "Someone wishes to trade with you. Do you accept?", false),
        ]
    );
}

#[test]
fn the_box_shows_the_question_in_front_first_and_its_buttons_answer_it() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.questions = vec![asking(1, fellowship(), false), asking(2, Asked::Died, true)];
    h.frame(&s);
    let up = h.ui.questions.drawn().cloned().expect("a box up");
    assert_eq!(up.id, 2, "the one in front first");
    assert_eq!(up.buttons.len(), 1, "death's one answer");
    assert_eq!(labels(&s.questions[1]), ["Resurrect"]);
    let middle = |r: auberean_horizon::draw::Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let out = h.click(&s, middle(up.buttons[0]));
    assert_eq!(
        out.acts,
        vec![Action::AnswerQuestion {
            question: 2,
            yes: true
        }]
    );
    // Answered, it is not shown again while the game still lists it: the next is up.
    h.frame(&s);
    let next = h.ui.questions.drawn().cloned().expect("the next up");
    assert_eq!(next.id, 1);
    assert_eq!(labels(&s.questions[0]), ["Yes", "No"]);
    let no = h.click(&s, middle(next.buttons[1]));
    assert_eq!(
        no.acts,
        vec![Action::AnswerQuestion {
            question: 1,
            yes: false
        }]
    );
    h.frame(&s);
    assert!(h.ui.questions.drawn().is_none(), "both answered: none up");
    // The game no longer lists them: forgotten, so a new question with a number of its own shows.
    s.questions.clear();
    h.frame(&s);
    s.questions = vec![asking(
        3,
        Asked::Message {
            text: Some(TextRef::literal("Hello")),
        },
        false,
    )];
    h.frame(&s);
    let message = h.ui.questions.drawn().cloned().expect("the message up");
    assert_eq!(labels(&s.questions[0]), ["OK"]);
    assert!(
        h.ui.covers(message.rect.x + 4.0, message.rect.y + 4.0),
        "the box answers the pointer"
    );
}

#[test]
fn a_press_on_the_boxs_words_asks_nothing_and_leaves_it_up() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.questions = vec![asking(1, fellowship(), false)];
    h.frame(&s);
    let up = h.ui.questions.drawn().cloned().expect("a box up");
    // Over the box's words, away from its buttons: nothing is asked.
    let out = h.click(&s, (up.rect.x + up.rect.w / 2.0, up.rect.y + 40.0 * h.k()));
    assert!(out.acts.is_empty(), "{:?}", out.acts);
    assert!(h.ui.questions.drawn().is_some(), "still up");
}

/// Every kind of question the client asks, with the names it carries.
fn every_kind() -> Vec<Asked> {
    let name = || Some(TextRef::literal("Cara"));
    vec![
        Asked::Died,
        Asked::Resurrection { by: name() },
        Asked::Effect {
            target: 1,
            source: 2,
            effect: 3,
            question: name(),
        },
        Asked::Message { text: name() },
        fellowship(),
        Asked::Oath {
            vassal: 9,
            name: name(),
        },
        Asked::Trade {
            partner: 9,
            name: name(),
        },
    ]
}

#[test]
fn every_kind_of_question_has_a_title_of_its_own() {
    let titles: Vec<_> = every_kind().iter().map(kind_title).collect();
    assert_eq!(
        titles,
        vec![
            Some(""),
            Some("Resurrection"),
            Some("Confirm"),
            Some("Notice"),
            Some("Fellowship"),
            Some("Allegiance"),
            Some("Trade"),
        ]
    );
    for (i, asked) in every_kind().into_iter().enumerate() {
        let q = asking(1, asked, false);
        assert_ne!(title(&q), "Question", "kind {i} has a title of its own");
    }
}

#[test]
fn death_s_box_has_no_title_and_is_answered_with_resurrect() {
    let mut h = Harness::new();
    let mut s = in_world();
    s.questions = vec![asking(1, Asked::Died, true)];
    h.frame(&s);
    let up = h.ui.questions.drawn().cloned().expect("death's box up");
    assert_eq!(up.title, "");
    assert_eq!(labels(&s.questions[0]), ["Resurrect"]);
}
