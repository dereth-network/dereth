//! The journal window: the quests under way and finished from the view's journal, what the game's
//! notices mark, giving a quest up, and the story's acts with their scenes played again.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::{Notice, Quest as Given, QuestNotice, Said};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_client_sdk::contract::Action;
use auberean_horizon::journal::{change, quests, status, time_left};
use auberean_horizon::keys::{keyed, Keyed};
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::journal::{JournalWindow, Page};

use crate::{centre, Harness, PLAYER};

fn given(id: u32, name: &str, status: u32, given: f64) -> Given {
    Given {
        id,
        name: TextRef::literal(name),
        description: TextRef::literal(&format!("{name}: what to do")),
        journal: TextRef::literal(&format!("{name}: so far")),
        status,
        challenge: -999,
        given,
        cancellable: true,
        ..Given::default()
    }
}

/// A view of the player in the world at game time 1000, with these quests in its journal.
pub(crate) fn view(journal: Vec<Given>, version: u64) -> View {
    View {
        player_id: Some(PLAYER),
        player: Some(v::Player {
            id: PLAYER,
            ..v::Player::default()
        }),
        clock: v::Clock {
            world_time: 0.0,
            game_time: 1000.0,
        },
        journal: Some(v::Journal {
            version,
            quests: journal,
        }),
        ..View::default()
    }
}

pub(crate) fn literal(t: &TextRef) -> Option<String> {
    t.literal.as_ref().map(|l| String::from_utf16_lossy(l))
}

pub(crate) fn follow(s: &mut GameState, view: &View, notices: &[Notice]) {
    s.follow(&Frame {
        view: Some(view),
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &literal,
        unknown_command: "",
    });
}

fn journal_notice(change: u32, quest: Given) -> Notice {
    Notice {
        about: None,
        said: Said::Quest(QuestNotice::Journal {
            change,
            quest: Some(quest),
            all: None,
        }),
    }
}

#[test]
fn the_quests_are_under_way_first_then_finished_each_newest_given_first_with_their_times() {
    let mut a = given(1, "Old errand", status::UNDER_WAY, 10.0);
    a.seconds_remaining = 3600;
    a.challenge = 12;
    let b = given(2, "New errand", status::UNDER_WAY, 50.0);
    let mut c = given(3, "Done long ago", status::COMPLETED, 5.0);
    c.seconds_remaining = 90;
    let d = given(4, "Done lately", status::FAILED, 40.0);
    let unknown = given(5, "Not a quest", 0x7000_0000, 60.0);
    let q = quests(&view(vec![a, b, c, d, unknown], 1), &literal);
    let ids: Vec<u32> = q.iter().map(|q| q.id).collect();
    assert_eq!(ids, vec![2, 1, 4, 3]);
    assert_eq!(q[1].until, Some(4600.0), "an hour from the game's time");
    assert_eq!(q[1].challenge, Some(12));
    assert_eq!(q[0].challenge, None, "any level");
    assert_eq!(q[0].until, None);
    assert_eq!(q[2].ending(), Some("Failed"));
    assert_eq!(
        JournalWindow::time_of(&q[1], 1000.0),
        "1h 00m left",
        "under way: before it runs out"
    );
    assert_eq!(
        JournalWindow::time_of(&q[3], 1030.0),
        "Again in 1m 00s",
        "finished: before it may be taken again"
    );
    assert_eq!(time_left(2.0 * 86_400.0 + 3.0 * 3600.0 + 59.0), "2d 3h");
    assert_eq!(time_left(65.2), "1m 06s");
}

/// The window open on the journal key, at 1080 lines.
fn opened(s: &GameState) -> Harness {
    let mut h = Harness::new();
    assert_eq!(
        keyed(auberean_horizon::keys::action::QUESTS),
        Some(Keyed::Journal)
    );
    h.ui.keyed(Keyed::Journal, s);
    h.frame(s);
    assert!(h.ui.journal.is_open());
    h
}

#[test]
fn a_quest_row_pressed_is_chosen_and_giving_it_up_asks_first() {
    let mut s = GameState::default();
    let finished = given(9, "Done", status::COMPLETED, 1.0);
    let mut fixed = given(8, "Bound", status::UNDER_WAY, 2.0);
    fixed.cancellable = false;
    let quest = given(7, "Errand", status::UNDER_WAY, 3.0);
    follow(&mut s, &view(vec![quest, fixed, finished], 1), &[]);
    let mut h = opened(&s);
    let k = h.k();
    let body = h.ui.journal.body().expect("drawn");
    let parts = JournalWindow::parts(body, k);
    let first = JournalWindow::row(parts.list, 0, k, 0.0);
    h.click(&s, centre(first));
    assert_eq!(h.ui.journal.chosen, Some(7));
    let out = h.click(&s, centre(parts.give_up));
    assert!(out.acts.is_empty(), "asks first");
    assert_eq!(h.ui.journal.giving_up, Some(7));
    let out = h.click(&s, centre(parts.no));
    assert!(out.acts.is_empty());
    assert_eq!(h.ui.journal.giving_up, None, "no: not given up");
    h.click(&s, centre(parts.give_up));
    let out = h.click(&s, centre(parts.yes));
    assert_eq!(out.acts, vec![Action::CancelQuest { quest: 7 }]);

    // One the game won't let go has no Give up.
    let second = JournalWindow::row(parts.list, 1, k, 0.0);
    h.click(&s, centre(second));
    assert_eq!(h.ui.journal.chosen, Some(8));
    let out = h.click(&s, centre(parts.give_up));
    assert_eq!(h.ui.journal.giving_up, None);
    assert!(out.acts.is_empty());
    // Finished quests are on their own page.
    assert_eq!(
        JournalWindow::listed(&s, Page::Finished)
            .iter()
            .map(|q| q.id)
            .collect::<Vec<_>>(),
        vec![9]
    );
}

/// Abandon, and the question's Yes and No, stand under the quest's pane, clear of its border, a
/// margin inside the window's own: at 1080 and 960 lines, with the art the client carries.
#[test]
fn abandon_stands_under_the_quests_pane_a_margin_inside_the_window() {
    let mut s = GameState::default();
    follow(
        &mut s,
        &view(vec![given(7, "Errand", status::UNDER_WAY, 3.0)], 1),
        &[],
    );
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.keyed(Keyed::Journal, &s);
        h.frame(&s);
        let k = h.k();
        let body = h.ui.journal.body().expect("drawn");
        let window = h.ui.journal.rect().expect("drawn");
        let parts = JournalWindow::parts(body, k);
        for (what, b) in [
            ("Abandon", parts.give_up),
            ("Yes", parts.yes),
            ("No", parts.no),
        ] {
            assert!(
                b.y >= parts.pane.bottom() + 4.0 * k,
                "{screen:?} {what}: under the pane, clear of its border"
            );
            assert!(
                b.right() <= body.right() - 8.0 * k && b.bottom() <= body.bottom() - 8.0 * k,
                "{screen:?} {what}: {b:?} a margin inside the body {body:?}"
            );
            assert!(window.contains(b.x, b.y) && window.contains(b.right(), b.bottom()));
        }
        assert!(parts.yes.right() <= parts.no.x, "{screen:?}: Yes before No");
    }
}

/// A quest's description longer than its pane scrolls there to its last line, and no line of it
/// meets the pane's scrollbar: at 1080 and 960 lines, with the art the client carries.
#[test]
fn a_long_quests_text_scrolls_to_its_end_and_no_line_meets_the_scrollbar() {
    let mut quest = given(7, "Errand", status::UNDER_WAY, 3.0);
    // One-letter words, so each line wrapped runs to within a letter of its room.
    quest.description = TextRef::literal(&vec!["a"; 1500].join(" "));
    quest.journal = TextRef::literal("The end.");
    let mut s = GameState::default();
    follow(&mut s, &view(vec![quest], 1), &[]);
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.keyed(Keyed::Journal, &s);
        h.frame(&s);
        let k = h.k();
        let parts = JournalWindow::parts(h.ui.journal.body().expect("drawn"), k);
        h.click(&s, centre(JournalWindow::row(parts.list, 0, k, 0.0)));
        h.frame(&s);
        let text = parts.text;
        let w = crate::measured(&h, dereth_ui_kit::widgets::scrollbar_width)
            .expect("the art's scrollbar")
            * k;
        let bar = auberean_horizon::draw::Rect::new(text.right() - w, text.y, w, text.h);
        let clear = |h: &Harness, when: &str| {
            for (words, at) in h.ui.journal.pane_lines() {
                assert!(
                    at.right() <= bar.x,
                    "{screen:?} {when}: {words:?} runs to {}, the bar starts at {}",
                    at.right(),
                    bar.x
                );
            }
            // The letters drawn in the pane, as well as the lines measured.
            let drawn: Vec<_> = h
                .list
                .quads
                .iter()
                .filter(|q| {
                    q.tex.is_some() && q.clip.is_some_and(|c| text.contains(c.x + 0.5, c.y + 0.5))
                })
                .collect();
            assert!(!drawn.is_empty(), "{screen:?} {when}: the text drawn");
            for q in drawn {
                assert!(
                    q.dst.right() <= bar.x,
                    "{screen:?} {when}: a letter at {:?}",
                    q.dst
                );
            }
        };
        let last = |h: &Harness| h.ui.journal.pane_lines().last().cloned().expect("lines");
        let (words, at) = last(&h);
        assert_eq!(words, "The end.");
        assert!(at.y > text.bottom(), "{screen:?}: below the pane at first");
        clear(&h, "at the top");
        for _ in 0..200 {
            h.input.mouse = centre(text);
            h.input.wheel = -1.0;
            h.frame(&s);
        }
        let (_, at) = last(&h);
        assert!(
            at.y >= text.y && at.bottom() <= text.bottom(),
            "{screen:?}: the last line {at:?} in the pane {text:?}"
        );
        clear(&h, "at the end");
    }
}

#[test]
fn a_quest_the_game_changes_is_marked_and_chosen_on_its_page_until_its_row_is_pressed() {
    let mut s = GameState::default();
    let quest = given(7, "Errand", status::UNDER_WAY, 3.0);
    let other = given(6, "Other", status::UNDER_WAY, 1.0);
    follow(&mut s, &view(vec![quest.clone(), other], 1), &[]);
    let mut h = opened(&s);
    h.ui.journal.page = Page::Finished;
    let mut done = quest.clone();
    done.status = status::COMPLETED;
    let other = given(6, "Other", status::UNDER_WAY, 1.0);
    follow(
        &mut s,
        &view(vec![done.clone(), other], 2),
        &[journal_notice(change::ONE, done)],
    );
    h.frame(&s);
    assert!(h.ui.journal.marked.contains(&7));
    assert_eq!(h.ui.journal.chosen, Some(7));
    assert_eq!(h.ui.journal.page, Page::Finished, "its page");
    // The next frame brings no change: the mark stays until the row is pressed.
    let again = view(s_quests(&s), 2);
    follow(&mut s, &again, &[]);
    h.frame(&s);
    assert!(h.ui.journal.marked.contains(&7));
    let k = h.k();
    let parts = JournalWindow::parts(h.ui.journal.body().expect("drawn"), k);
    h.click(&s, centre(JournalWindow::row(parts.list, 0, k, 0.0)));
    assert!(!h.ui.journal.marked.contains(&7), "seen");
}

/// The quests as the view last gave them, given again.
fn s_quests(s: &GameState) -> Vec<Given> {
    s.journal
        .quests
        .iter()
        .map(|q| Given {
            id: q.id,
            name: TextRef::literal(&q.name),
            status: q.status,
            given: q.given,
            ..Given::default()
        })
        .collect()
}

#[test]
fn the_story_page_lists_the_acts_and_plays_a_scene_seen_again() {
    let mut s = GameState::default();
    let mut vw = view(Vec::new(), 1);
    let scene = |id, name: &str, hidden, playable| v::Scene {
        id,
        name: TextRef::literal(name),
        hidden,
        playable,
    };
    vw.acts = Some(v::Acts {
        version: 1,
        acts: vec![
            v::Act {
                id: 1,
                name: TextRef::literal("Act One"),
                visible: true,
                scenes: vec![
                    scene(10, "Seen", false, true),
                    scene(11, "Locked", true, false),
                    scene(12, "Not yet", false, false),
                ],
            },
            v::Act {
                id: 2,
                name: TextRef::literal("Hidden act"),
                visible: false,
                scenes: Vec::new(),
            },
        ],
    });
    follow(&mut s, &vw, &[]);
    let acts = s.journal.acts.clone().expect("the acts");
    assert_eq!(acts.len(), 1, "the hidden act is not listed");
    let names: Vec<&str> = acts[0].scenes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Seen", "Not yet"],
        "a locked scene is not listed"
    );
    let mut h = opened(&s);
    h.ui.journal.page = Page::Story;
    h.frame(&s);
    let k = h.k();
    let parts = JournalWindow::parts(h.ui.journal.body().expect("drawn"), k);
    // The act's row, then its first scene's Play.
    let act_row = (parts.list.x + 20.0 * k, parts.list.y + 20.0 * k);
    h.click(&s, act_row);
    assert!(h.ui.journal.open_acts.contains(&1));
    let right = parts.pane.right() - 10.0 * k - 39.0 * k;
    let play = (right, parts.list.y + (4.0 + 42.0 + 14.0) * k);
    let out = h.click(&s, play);
    assert_eq!(out.acts, vec![Action::PlayStoryScene { act: 1, scene: 10 }]);
    // The second scene has not been seen: no Play.
    let out = h.click(&s, (right, play.1 + 28.0 * k));
    assert!(out.acts.is_empty());
}
