//! The dialogs on the retail layouts: one up at a time, their answers reaching the panels that
//! asked.

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::dialog::{self, kind, notice as said, Dialogs};
use auberean_ui_game::gameplay::{notice, GamePlay, Notice, Record, Request, View};
use auberean_ui_game::quests::{change, status, Quest};
use auberean_ui_game::toolbar::action as tb;

use crate::common::records;

/// The play screen in the world, the dialogs' factory beside it, the clock, the dialogs' notices
/// waiting for the next frame, and the skill window's answers the frames are given.
pub(crate) struct World {
    pub(crate) ui: Ui,
    pub(crate) play: GamePlay,
    pub(crate) dialogs: Dialogs,
    said: Vec<Notice>,
    t: f64,
    pub(crate) skills: Option<auberean_ui_game::skill_tree::Facts>,
    pub(crate) acts: Option<Vec<auberean_ui_game::story_quests::Act>>,
    pub(crate) craft: Option<auberean_ui_game::craft::Facts>,
}

impl World {
    pub(crate) fn new() -> Self {
        Self::over(records())
    }

    /// The same over other records.
    pub(crate) fn over(assets: std::sync::Arc<dyn auberean_ui::Assets + Send + Sync>) -> Self {
        let mut ui = Ui::new(assets, 1024, 768);
        let mut out = Vec::new();
        let mut play = GamePlay::new(&mut ui, &mut out);
        play.login_finished(&mut ui, &mut out);
        Self {
            ui,
            play,
            dialogs: Dialogs::default(),
            said: Vec::new(),
            t: 0.0,
            skills: None,
            acts: None,
            craft: None,
        }
    }

    /// A frame as the interface runs one: what the dialog up heard, then the play screen with the
    /// dialogs' notices and the scripts', then its dialogs carried out. Returns the play screen's
    /// other requests.
    pub(crate) fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        self.dialogs.listen(&mut self.ui, &mut self.said);
        let mut all: Vec<Notice> = std::mem::take(&mut self.said);
        all.extend_from_slice(notices);
        let mut out = Vec::new();
        let view = View {
            skills: self.skills.as_ref(),
            acts: self.acts.as_deref(),
            craft: self.craft.as_ref(),
            ..View::default()
        };
        self.play.tick(&mut self.ui, &view, &all, &mut out);
        let parent = Some(self.play.frame());
        let mut rest = Vec::new();
        for r in out {
            match r {
                Request::Dialog(d) => {
                    self.dialogs
                        .make_dialog(&mut self.ui, parent, &(&d).into(), &mut self.said);
                    rest.push(Request::Dialog(d));
                }
                Request::DialogDown(id) => {
                    self.dialogs.close_dialog(&mut self.ui, id, &mut self.said);
                    rest.push(Request::DialogDown(id));
                }
                r => rest.push(r),
            }
        }
        rest
    }

    pub(crate) fn key(&mut self, k: u32) -> Vec<Request> {
        self.ui.key(k, true);
        let mut out = self.tick(&[]);
        self.ui.key(k, false);
        out.extend(self.tick(&[]));
        out
    }

    /// Press and let go of an element, a frame between each step and two after.
    pub(crate) fn click(&mut self, h: Handle) -> Vec<Request> {
        let r = self.ui.screen_rect(h).expect("placed");
        self.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        let mut out = self.tick(&[]);
        for down in [true, false] {
            self.ui.pointer_button(button_id::LEFT, down);
            out.extend(self.tick(&[]));
        }
        out.extend(self.tick(&[]));
        out
    }

    /// A button of the dialog up.
    pub(crate) fn button(&self, name: &str) -> Handle {
        let w = self.dialogs.window().expect("a dialog is up");
        self.ui
            .child_recursive(w, Token::of(name))
            .unwrap_or_else(|| panic!("{name} in the dialog"))
    }

    /// The text of an element of the dialog up.
    pub(crate) fn text(&self, name: &str) -> String {
        let w = self.dialogs.window().expect("a dialog is up");
        let t = self.ui.child_recursive(w, Token::of(name)).expect("text");
        self.ui.text(t).unwrap_or_default()
    }
}

fn request(context: u32, k: u32, text: &str, first: bool) -> dialog::Request {
    dialog::Request {
        context,
        kind: k,
        text: text.into(),
        info: None,
        first,
    }
}

fn can_mouse_look(n: &[Notice]) -> Vec<u32> {
    n.iter()
        .filter(|n| n.native == said::UPDATE_CAN_MOUSE_LOOK)
        .map(|n| n.words[0])
        .collect()
}

/// One dialog is up at a time: a second waits its turn; one asked for first goes up at once and
/// the one it displaced waits at the head of the line; a dialog closed by its number leaves the
/// line, and when the one up closes the next goes up. Each dialog keeps mouse look off from the
/// moment it is asked for until it goes. A question's yes answers yes, any other of its buttons
/// no; a message's button reads it.
///
/// Behaviour: ui.dialogs.one-dialog-is-up-at-a-time-and-the-rest-wait-their-turn
#[test]
fn one_dialog_is_up_at_a_time_and_the_rest_wait_their_turn() {
    let mut w = World::new();
    let parent = Some(w.play.frame());
    let mut n = Vec::new();
    let (a, b, c, d) = (
        dialog::new_context(),
        dialog::new_context(),
        dialog::new_context(),
        dialog::new_context(),
    );
    for (k, kd, text) in [
        (a, kind::CONFIRMATION, "First?"),
        (b, kind::MESSAGE, "Second."),
        (c, kind::WAIT, "Third."),
    ] {
        w.dialogs
            .make_dialog(&mut w.ui, parent, &request(k, kd, text, false), &mut n);
    }
    assert_eq!(w.dialogs.up(), Some(a));
    assert_eq!(w.dialogs.waiting(), vec![b, c]);
    assert_eq!(w.text("QuestionText"), "First?");
    w.dialogs.make_dialog(
        &mut w.ui,
        parent,
        &request(d, kind::RESURRECT, "You died.", true),
        &mut n,
    );
    assert_eq!(w.dialogs.up(), Some(d), "asked for first: up at once");
    assert_eq!(
        w.dialogs.waiting(),
        vec![a, b, c],
        "the one it displaced first in line"
    );
    assert!(
        w.ui.element(Token::of("ConfirmationField")).is_none(),
        "taken down"
    );
    assert_eq!(w.text("TextField"), "You died.");
    assert_eq!(can_mouse_look(&n), vec![0, 0, 0, 0]);

    n.clear();
    w.dialogs.close_dialog(&mut w.ui, c, &mut n);
    assert_eq!(w.dialogs.waiting(), vec![a, b], "closed while waiting");
    assert_eq!(can_mouse_look(&n), vec![1]);

    // Death's dialog answered: its notice, then the question back up.
    let button = w.button("ResurrectButton");
    w.click(button);
    assert_eq!(w.dialogs.up(), Some(a));
    assert_eq!(w.text("QuestionText"), "First?", "made again");

    // The question's no button answers no; then the message is up and its button reads it.
    let mut heard = Vec::new();
    let no = w.button("NoButton");
    let r = w.ui.screen_rect(no).expect("placed");
    w.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    for down in [true, false] {
        w.ui.pointer_button(button_id::LEFT, down);
        w.dialogs.listen(&mut w.ui, &mut heard);
    }
    let answer: Vec<_> = heard
        .iter()
        .filter_map(dialog::confirmation_closed)
        .collect();
    assert_eq!(answer, vec![(a, false)]);
    assert_eq!(w.dialogs.up(), Some(b));
    assert_eq!(w.text("TextField"), "Second.");
    heard.clear();
    let ok = w.button("OkButton");
    let r = w.ui.screen_rect(ok).expect("placed");
    w.ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    for down in [true, false] {
        w.ui.pointer_button(button_id::LEFT, down);
        w.dialogs.listen(&mut w.ui, &mut heard);
    }
    assert!(heard
        .iter()
        .any(|n| n.native == said::CLOSE_MESSAGE_DIALOG && n.words == vec![b]));
    assert_eq!(w.dialogs.up(), None);
    assert!(w.ui.element(Token::of("MessageField")).is_none(), "gone");
}

fn quest(id: u32, st: u32, given: f64, name: &str) -> Quest {
    Quest {
        id,
        name: StringInfo::literal(name),
        status: st,
        given,
        cancellable: true,
        ..Quest::default()
    }
}

/// The dialogs' answers reach the panel that asked: death's dialog brings the player back; a
/// question of the player's scripts answered yes is answered with what it carried, no not at all;
/// the journal's question whether to give a quest up, answered yes, gives it up. While a dialog
/// of the world is up, mouse look waits.
///
/// Behaviour: ui.dialogs.the-answers-reach-the-panel-that-asked
#[test]
fn the_answers_reach_the_panel_that_asked() {
    let mut w = World::new();
    // Death.
    let dead = Notice {
        native: notice::UPDATE_DEATH_STATE,
        words: vec![1],
        ..Notice::default()
    };
    let out = w.tick(&[dead]);
    assert!(
        out.iter()
            .any(|r| matches!(r, Request::Dialog(d) if d.kind == kind::RESURRECT)),
        "{out:?}"
    );
    w.tick(&[]);
    let middle = w.ui.display();
    w.ui.key(auberean_ui_game::camera::action::TOGGLE_MOUSE_LOOK, true);
    w.tick(&[]);
    w.ui.key(auberean_ui_game::camera::action::TOGGLE_MOUSE_LOOK, false);
    w.tick(&[]);
    assert!(
        !w.play.camera().expect("camera").mouse_look(),
        "mouse look waits on the dialog ({middle:?})"
    );
    let out = w.click(w.button("ResurrectButton"));
    assert!(out.contains(&Request::Resurrect), "{out:?}");
    w.tick(&[]);
    assert!(
        w.play.camera().expect("camera").mouse_look(),
        "switched on, and nothing waits now"
    );
    w.ui.key(auberean_ui_game::camera::action::TOGGLE_MOUSE_LOOK, true);
    w.tick(&[]);
    w.ui.key(auberean_ui_game::camera::action::TOGGLE_MOUSE_LOOK, false);
    // The scripts say the player lives again: the play screen back.
    let alive = Notice {
        native: notice::UPDATE_DEATH_STATE,
        words: vec![0],
        ..Notice::default()
    };
    w.tick(&[alive]);
    assert!(w.play.shown(&w.ui));

    // The server's question whether an effect may go on: yes answers with what it carried.
    let question = Notice {
        native: notice::SHOULD_APPLY_TOTEM,
        about: Some(1),
        words: vec![0, 7, 0, 8, 11],
        records: vec![Some(Record::Text(auberean_text::StringInfo::literal(
            "Accept?",
        )))],
    };
    w.tick(&[question]);
    w.tick(&[]);
    assert_eq!(w.text("QuestionText"), "Accept?");
    let out = w.click(w.button("YesButton"));
    assert!(
        out.contains(&Request::Confirm {
            objects: [7, 8],
            value: 11,
        }),
        "{out:?}"
    );

    // The journal's question.
    let start = Notice {
        native: notice::UPDATE_QUESTS,
        about: Some(1),
        words: vec![change::ONE, 0],
        records: vec![
            None,
            Some(Record::Quest(quest(
                5,
                status::UNDER_WAY,
                1.0,
                "The Bridge",
            ))),
        ],
    };
    w.tick(&[start]);
    w.key(tb::QUEST_PANEL);
    w.tick(&[]);
    let cancel =
        w.ui.element(Token::of("QuestCancelButton"))
            .expect("the open quest's cancel button");
    let out = w.click(cancel);
    assert!(
        out.iter()
            .any(|r| matches!(r, Request::Dialog(d) if d.kind == kind::CONFIRMATION)),
        "{out:?}"
    );
    assert!(!w.text("QuestionText").is_empty(), "the question asked");
    let out = w.click(w.button("YesButton"));
    assert!(out.contains(&Request::CancelQuest { quest: 5 }), "{out:?}");
}

/// A timed question says how many of its seconds are left, counting down each second, and
/// answers itself no when they have run out.
///
/// Behaviour: ui.dialogs.a-timed-question-counts-down-and-answers-no
#[test]
fn a_timed_question_counts_down_and_answers_no() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut f = Dialogs::default();
    let mut n = Vec::new();
    let table = ui.assets().data_id_by_enum(
        auberean_ui_game::flow::MESSAGES,
        auberean_ui_game::flow::TABLE_MAPPING,
    );
    // The render options' question, which writes its `time` variable.
    let info = StringInfo::entry(table, Token::of("ID_Option_RenderText").raw());
    let probe = |ui: &Ui, s: i32| {
        let mut i = info.clone();
        i.add_var(Token::of("time").raw(), auberean_text::Value::Int(s));
        ui.assets().write(&i).unwrap_or_default()
    };
    assert_ne!(
        probe(&ui, 15),
        probe(&ui, 14),
        "the string writes the seconds"
    );
    ui.use_time(100.0);
    let c = dialog::new_context();
    let r = dialog::Request {
        context: c,
        kind: kind::TIMED,
        text: String::new(),
        info: Some(info.clone()),
        first: false,
    };
    f.make_dialog(&mut ui, None, &r, &mut n);
    assert!(
        can_mouse_look(&n).is_empty(),
        "a screen of its own: mouse look alone"
    );
    let question = |ui: &Ui, f: &Dialogs| {
        let w = f.window().expect("up");
        let t = ui
            .child_recursive(w, Token::of("QuestionText"))
            .expect("text");
        ui.text(t).unwrap_or_default()
    };
    assert_eq!(question(&ui, &f), probe(&ui, 15));
    let mut answered = None;
    let mut half_seconds = 0;
    while half_seconds < 40 && answered.is_none() {
        half_seconds += 1;
        ui.use_time(100.0 + f64::from(half_seconds) * 0.5);
        f.listen(&mut ui, &mut n);
        answered = n.iter().find_map(dialog::confirmation_closed);
        if half_seconds == 2 {
            assert_eq!(question(&ui, &f), probe(&ui, 14), "a second later");
        }
    }
    assert_eq!(answered, Some((c, false)));
    assert_eq!(
        half_seconds, 32,
        "answered no once its 15 seconds have run out"
    );
    assert_eq!(f.up(), None);
}

/// The scripts' message comes as a string reference and goes to its dialog's text as one: while
/// the string table it reads is on its way the dialog is up with no text, and the text shows once
/// the table has come.
///
/// Behaviour: ui.dialogs.a-message-waits-for-its-string-table
#[test]
fn a_message_waits_for_its_string_table() {
    use super::stage::{Lacking, LACKING};
    let assets = Lacking::new();
    let mut w = World::over(assets.clone());
    let message = Notice {
        native: notice::DISPLAY_MESSAGE,
        about: Some(1),
        records: vec![Some(Record::Text(StringInfo::entry(LACKING, 3)))],
        ..Notice::default()
    };
    w.tick(&[message]);
    w.tick(&[]);
    assert!(w.dialogs.window().is_some(), "the message is up");
    assert_eq!(w.text("TextField"), "", "waiting on the table");
    assets.arrive();
    w.tick(&[]);
    let text = w.text("TextField");
    assert!(text.starts_with("From the server"), "{text:?}");
}

/// The scripts' message says whether it goes up at once: one that does goes up in place of the
/// message up, which waits at the head of the line and comes back when it closes; one that does
/// not waits its turn behind them.
///
/// Behaviour: ui.dialogs.a-message-the-scripts-ask-for-first-goes-up-at-once
#[test]
fn a_message_the_scripts_ask_for_first_goes_up_at_once() {
    let mut w = World::new();
    let message = |text: &str, first: u32| Notice {
        native: notice::DISPLAY_MESSAGE,
        about: Some(1),
        words: vec![0, first],
        records: vec![Some(Record::Text(StringInfo::literal(text))), None],
    };
    w.tick(&[message("Waits", 0)]);
    w.tick(&[]);
    assert_eq!(w.text("TextField"), "Waits");
    w.tick(&[message("At once", 1)]);
    w.tick(&[]);
    assert_eq!(w.text("TextField"), "At once", "in place of the one up");
    w.tick(&[message("Last", 0)]);
    w.tick(&[]);
    assert_eq!(w.text("TextField"), "At once", "the next waits its turn");
    let up = w.dialogs.up().expect("a dialog is up");
    w.dialogs.close_dialog(&mut w.ui, up, &mut Vec::new());
    w.tick(&[]);
    assert_eq!(
        w.text("TextField"),
        "Waits",
        "the displaced one comes back first"
    );
}

/// Someone offering to bring the player back asks the question, naming them, in place of death's
/// own dialog; an offer with no name reads as no offer.
///
/// Behaviour: ui.dialogs.an-offer-to-bring-the-player-back-asks-by-name
#[test]
fn an_offer_to_bring_the_player_back_asks_by_name() {
    let mut w = World::new();
    let offer = |name: &str| Notice {
        native: notice::RESURRECTION_REQUESTED_BY_OTHER,
        records: vec![Some(Record::Text(StringInfo::literal(name)))],
        ..Notice::default()
    };
    let out = w.tick(&[offer("Asher")]);
    let asked: Vec<_> = out
        .iter()
        .filter_map(|r| match r {
            Request::Dialog(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(asked.len(), 1, "{out:?}");
    assert_eq!(asked[0].kind, kind::CONFIRMATION);
    assert!(asked[0].text.contains("Asher"), "{:?}", asked[0].text);
    let out = w.tick(&[offer("")]);
    assert!(
        out.iter()
            .any(|r| matches!(r, Request::Dialog(d) if d.kind == kind::RESURRECT)),
        "{out:?}"
    );
}
