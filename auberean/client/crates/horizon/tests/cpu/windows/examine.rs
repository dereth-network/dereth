//! The examine window: what is examined, its profile's lines on the tabs asked for, the tabs,
//! opening and closing it, and its countdowns.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::notice::examination::{kind, line, tab};
use auberean_client_sdk::contract::notice::{
    ExaminationLine, ExaminationNotice, ExaminationProfile, ExaminationRequest, Notice, Said,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::Action;
use auberean_horizon::examined::Piece;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::{Frame, GameState};
use auberean_horizon::ui::examine::{countdown, parts, tab_state, TabState, TABS};

use crate::{centre, in_world, Harness, PLAYER};

const DRUDGE: u64 = 0x7000_0000_0000_0042;

fn said(x: ExaminationNotice) -> Notice {
    Notice {
        about: Some(PLAYER),
        said: Said::Examination(x),
    }
}

fn follow(s: &mut GameState, notices: &[Notice]) {
    s.follow(&Frame {
        view: None,
        notices,
        events: &[],
        told: &[],
        lines: &[],
        write: &|t: &TextRef| t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
        unknown_command: "",
    });
}

fn text(tabs: u32, words: &str) -> ExaminationLine {
    ExaminationLine {
        kind: line::STRING,
        tabs,
        text: Some(TextRef {
            literal: Some(words.encode_utf16().collect()),
            ..TextRef::default()
        }),
        ..ExaminationLine::default()
    }
}

fn number(tabs: u32, kind: u32, value: u64) -> ExaminationLine {
    ExaminationLine {
        kind,
        tabs,
        value,
        ..ExaminationLine::default()
    }
}

fn request(tabs: u32) -> ExaminationRequest {
    ExaminationRequest {
        kind: kind::ITEM,
        object: DRUDGE,
        tabs,
        ..ExaminationRequest::default()
    }
}

fn profile(tabs: u32, lines: Vec<ExaminationLine>) -> ExaminationProfile {
    let made = lines.iter().fold(0, |m, l| m | l.tabs);
    ExaminationProfile {
        request: request(tabs),
        tabs: made,
        lines,
    }
}

fn words(s: &GameState) -> Vec<Piece> {
    s.examined.pieces.clone()
}

#[test]
fn a_profiles_lines_show_on_the_tabs_its_request_asked_for() {
    let mut s = GameState::default();
    let lines = vec![
        text(tab::GENERAL, "Drudge Ravener"),
        number(tab::GENERAL, line::INT, u64::from(-3_i32 as u32)),
        number(tab::DETAILS, line::FLOAT, u64::from(1.5_f32.to_bits())),
        number(tab::USAGE, line::BOOL, 1),
        ExaminationLine {
            kind: line::BREAK,
            tabs: tab::GENERAL,
            ..ExaminationLine::default()
        },
    ];
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::GENERAL,
            lines.clone(),
        ))))],
    );
    assert_eq!(
        words(&s),
        vec![
            Piece::Text(Some("Drudge Ravener".into())),
            Piece::Text(Some("-3".into())),
            Piece::Break
        ]
    );
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::DETAILS,
            lines.clone(),
        ))))],
    );
    assert_eq!(words(&s), vec![Piece::Text(Some("1.50".into()))]);
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::USAGE,
            lines,
        ))))],
    );
    assert_eq!(words(&s), vec![Piece::Text(Some("Yes".into()))]);
}

#[test]
fn a_request_whose_tabs_have_no_line_shows_the_first_tabs_and_an_effects_own_lines_show_in_its_place(
) {
    let mut s = GameState::default();
    let effect = ExaminationLine {
        kind: line::EFFECT,
        tabs: tab::GENERAL,
        effect: vec![
            text(tab::GENERAL, "Fire"),
            ExaminationLine {
                kind: line::EFFECT,
                tabs: tab::GENERAL,
                effect: vec![text(tab::GENERAL, "never")],
                ..ExaminationLine::default()
            },
            ExaminationLine {
                kind: line::INDENT,
                tabs: tab::GENERAL,
                value: 0,
                ..ExaminationLine::default()
            },
        ],
        ..ExaminationLine::default()
    };
    let p = profile(tab::EFFECTS, vec![text(tab::GENERAL, "Sword"), effect]);
    follow(&mut s, &[said(ExaminationNotice::Profile(Some(p)))]);
    assert_eq!(s.examined.asked, tab::EFFECTS | tab::GENERAL);
    assert_eq!(
        words(&s),
        vec![
            Piece::Text(Some("Sword".into())),
            Piece::Text(Some("Fire".into())),
            Piece::Indent(1),
        ]
    );
}

#[test]
fn a_new_request_waits_for_its_profile() {
    let mut s = GameState::default();
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::GENERAL,
            vec![text(tab::GENERAL, "x")],
        ))))],
    );
    assert!(s.examined.profile.is_some());
    follow(
        &mut s,
        &[said(ExaminationNotice::RequestChanged(Some(request(
            tab::GENERAL,
        ))))],
    );
    assert_eq!(s.examined.profile, None);
    assert!(words(&s).is_empty());
    assert_eq!(s.examined.request, Some(request(tab::GENERAL)));
}

#[test]
fn a_tab_is_active_when_asked_for_disabled_when_it_has_no_line_and_else_may_be_asked_for() {
    assert_eq!(
        tab_state(tab::GENERAL, tab::GENERAL, tab::GENERAL | tab::USAGE),
        TabState::Active
    );
    assert_eq!(
        tab_state(tab::USAGE, tab::GENERAL, tab::GENERAL | tab::USAGE),
        TabState::Inactive
    );
    assert_eq!(
        tab_state(tab::EFFECTS, tab::GENERAL, tab::GENERAL | tab::USAGE),
        TabState::Disabled
    );
}

/// In the world as the player's options keep it, the classic window off.
fn tabbed() -> GameState {
    let mut s = in_world();
    s.examined.classic_option = Some(false);
    s
}

/// The examine key opens the window on what the game examines and asks only that, whatever is
/// selected, as the game's own key does: the game moves what it examines to the selection only
/// while the window is open, so a selection made with it closed is not what it opens on.
#[test]
fn opening_the_window_asks_to_examine_with_its_tabs_and_closing_it_to_stop() {
    let mut h = Harness::new();
    let mut s = tabbed();
    s.target = Some(auberean_horizon::state::Target {
        id: 0x7000_0000_0000_0042,
        name: "Wolfgang the Trader".to_owned(),
        health: None,
    });
    let open = h.ui.keyed(Keyed::Examine, &s);
    assert_eq!(
        open,
        vec![Action::Examination {
            tabs: Some(tab::GENERAL)
        }]
    );
    assert!(h.ui.examine.is_open());
    let closed = h.ui.keyed(Keyed::Examine, &s);
    assert_eq!(closed, vec![Action::Examination { tabs: None }]);
    // The scripts ask for it to open.
    let mut s = tabbed();
    s.examined.open_asked = true;
    let out = h.frame(&s);
    assert_eq!(
        out.acts,
        vec![Action::Examination {
            tabs: Some(tab::GENERAL)
        }]
    );
    assert!(h.ui.examine.is_open());
}

#[test]
fn a_tab_pressed_asks_for_its_lines_and_one_without_lines_does_nothing() {
    let mut h = Harness::new();
    let mut s = tabbed();
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::GENERAL,
            vec![text(tab::GENERAL, "a"), text(tab::USAGE, "b")],
        ))))],
    );
    s.in_world = true;
    h.ui.keyed(Keyed::Examine, &s);
    h.frame(&s);
    let (tabs, _) = parts(h.ui.examine.body().expect("drawn"), h.k());
    assert_eq!(
        h.click(&s, centre(tabs[3])).acts,
        vec![Action::Examination {
            tabs: Some(tab::USAGE)
        }]
    );
    assert!(h.click(&s, centre(tabs[2])).acts.is_empty(), "no effects");
    // The keys step to the next tab with lines, round past the end.
    assert_eq!(
        h.ui.keyed(Keyed::StepTab { next: true }, &s),
        vec![Action::Examination {
            tabs: Some(tab::GENERAL)
        }]
    );
    assert_eq!(
        h.ui.keyed(Keyed::StepTab { next: false }, &s),
        vec![Action::Examination {
            tabs: Some(tab::USAGE)
        }]
    );
}

#[test]
fn escape_closes_the_examine_window_over_the_skill_window() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.skills.toggle();
    h.ui.keyed(Keyed::Examine, &s);
    let mut asked = Vec::new();
    assert!(h.ui.close_top(&mut asked));
    assert_eq!(asked, vec![Action::Examination { tabs: None }]);
    assert!(!h.ui.examine.is_open() && h.ui.skills.is_open());
    assert!(h.ui.close_top(&mut asked));
    assert!(!h.ui.window_open());
    assert!(!h.ui.close_top(&mut asked));
}

#[test]
fn a_countdown_reads_the_time_left_in_its_two_largest_units() {
    assert_eq!(countdown(59.2), "(1m 0s)");
    assert_eq!(countdown(125.0), "(2m 5s)");
    assert_eq!(countdown(3_725.0), "(1h 2m)");
    assert_eq!(countdown(90_000.0), "(1d 1h)");
    assert_eq!(countdown(31.0 * 86_400.0), "(1mo 1d)");
    assert_eq!(countdown(-5.0), "(0m 0s)");
}

#[test]
fn a_long_description_wraps_within_the_window_and_a_word_wider_than_a_line_is_broken() {
    let mut s = tabbed();
    let long = "A worn trader's coat of good wool, patched at the elbows and smelling faintly of \
                the road, with deep pockets for coins and a collar turned up against the rain.";
    let unbroken = "W".repeat(80);
    s.examined.request = Some(request(tab::GENERAL));
    s.examined.profile = Some(profile(tab::GENERAL, vec![text(tab::GENERAL, long)]));
    s.examined.pieces = vec![
        Piece::Text(Some(long.into())),
        Piece::Break,
        Piece::Text(Some(unbroken)),
    ];
    s.examined.asked = tab::GENERAL;
    s.examined.made = tab::GENERAL;
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.keyed(Keyed::Examine, &s);
        // The window drawn with no lines, then with them: the quads the lines add.
        let mut bare = s.clone();
        bare.examined.pieces.clear();
        h.frame(&bare);
        let chrome: Vec<_> = h.list.quads.iter().map(|q| q.dst).collect();
        h.frame(&s);
        let (_, lines) = parts(h.ui.examine.body().expect("drawn"), h.k());
        let glyphs: Vec<_> = h
            .list
            .quads
            .iter()
            .filter(|q| q.tex.is_some() && !chrome.contains(&q.dst))
            // Letters, not the window's frame: small, on the lines' rows, right of their start.
            .filter(|q| {
                let y = q.dst.y + 1.0;
                q.dst.h <= 24.0 * h.k()
                    && q.dst.x >= lines.x - 2.0
                    && y >= lines.y
                    && y <= lines.bottom()
            })
            .collect();
        let rows: std::collections::BTreeSet<i32> = glyphs
            .iter()
            .map(|q| dereth_primitives::num::to_i32(q.dst.y.round()))
            .collect();
        assert!(
            rows.len() >= 4,
            "{screen:?}: wrapped over {} rows",
            rows.len()
        );
        // A glyph's box may stand a few pixels past its letter: the edge it is drawn with.
        let over = glyphs.iter().map(|q| q.dst.right()).fold(0.0_f32, f32::max);
        assert!(
            over <= lines.right() + 4.0 * h.k(),
            "{screen:?}: drawn to {over}, the lines end at {}",
            lines.right()
        );
    }
}

/// The classic window, as the player's options choose it (and before they are read): no tabs,
/// every tab asked for, and every tab's lines on its one page in the profile's order; the keys
/// for the next and the previous tab do nothing. The options switched while it is open, it asks
/// again: the tabbed window for its first tab, the classic one for every tab.
#[test]
fn the_classic_window_asks_for_every_tab_and_shows_their_lines_on_one_page() {
    let mut h = Harness::new();
    let mut s = in_world();
    assert!(s.examined.classic(), "classic until the options are read");
    s.examined.classic_option = Some(true);
    assert_eq!(
        h.ui.keyed(Keyed::Examine, &s),
        vec![Action::Examination {
            tabs: Some(tab::ALL)
        }]
    );
    let lines = vec![
        text(tab::GENERAL, "Iron Sword"),
        text(tab::DETAILS, "Armor Value"),
        text(tab::EFFECTS, "Enchantment Effects:"),
        text(tab::USAGE, "Requirements"),
    ];
    follow(
        &mut s,
        &[said(ExaminationNotice::Profile(Some(profile(
            tab::ALL,
            lines,
        ))))],
    );
    s.in_world = true;
    assert_eq!(
        words(&s),
        [
            "Iron Sword",
            "Armor Value",
            "Enchantment Effects:",
            "Requirements"
        ]
        .map(|w| Piece::Text(Some(w.into())))
    );
    h.frame(&s);
    let (tabs, _) = parts(h.ui.examine.body().expect("drawn"), h.k());
    assert!(
        h.click(&s, centre(tabs[3])).acts.is_empty(),
        "no tab to press"
    );
    assert!(h.ui.keyed(Keyed::StepTab { next: true }, &s).is_empty());
    s.examined.classic_option = Some(false);
    assert_eq!(
        h.frame(&s).acts,
        vec![Action::Examination {
            tabs: Some(tab::GENERAL)
        }]
    );
    assert!(h.frame(&s).acts.is_empty(), "asked once");
    s.examined.classic_option = Some(true);
    assert_eq!(
        h.frame(&s).acts,
        vec![Action::Examination {
            tabs: Some(tab::ALL)
        }]
    );
}

/// The player's options as the view gives them choose the window: the classic one with their
/// bit, the tabbed one without.
#[test]
fn the_players_options_choose_the_classic_window() {
    let mut s = in_world();
    for (bits, classic) in [(0x2_0000, true), (0x3F5, false)] {
        let view = auberean_client_sdk::contract::view::View {
            options: Some(auberean_client_sdk::contract::view::GameOptions {
                bits,
                ..Default::default()
            }),
            ..Default::default()
        };
        s.follow(&Frame {
            view: Some(&view),
            notices: &[],
            events: &[],
            told: &[],
            lines: &[],
            write: &|_: &TextRef| None,
            unknown_command: "",
        });
        assert_eq!(s.examined.classic(), classic, "{bits:#x}");
    }
}

/// The rows the glyphs drawn in `area` stand on, top first: glyphs whose tops are within 12
/// layout units of each other on one row.
fn glyph_rows(
    h: &Harness,
    chrome: &[auberean_horizon::draw::Rect],
    area: auberean_horizon::draw::Rect,
) -> Vec<f32> {
    let mut ys: Vec<f32> = h
        .list
        .quads
        .iter()
        .filter(|q| q.tex.is_some() && !chrome.contains(&q.dst))
        .filter(|q| q.dst.h <= 24.0 * h.k() && area.contains(q.dst.x + 0.5, q.dst.y + 1.0))
        .map(|q| q.dst.y)
        .collect();
    ys.sort_by(f32::total_cmp);
    let mut rows: Vec<f32> = Vec::new();
    for y in ys {
        if rows.last().is_none_or(|r| y - r > 12.0 * h.k()) {
            rows.push(y);
        }
    }
    rows
}

/// A teaching stone's page, as the server gives it: its title, a break, then "Training Stone 1 of
/// 10", a blank line and the lesson in one text. The text's line breaks start new lines, so the
/// lesson stands two lines under "Training Stone 1 of 10", at the left.
#[test]
fn a_texts_line_breaks_start_new_lines_in_the_window() {
    let mut s = in_world();
    let brk = ExaminationLine {
        kind: line::BREAK,
        tabs: 0x2,
        ..ExaminationLine::default()
    };
    let stone = ExaminationProfile {
        request: request(tab::GENERAL),
        tabs: 0x2,
        lines: vec![
            text(0x2, "Movement"),
            brk.clone(),
            text(
                0x2,
                "Training Stone 1 of 10\n\nYour first lesson is movement.",
            ),
            brk,
        ],
    };
    follow(
        &mut s,
        &[
            said(ExaminationNotice::RequestChanged(Some(request(
                tab::GENERAL,
            )))),
            said(ExaminationNotice::Open),
            said(ExaminationNotice::Profile(Some(stone))),
        ],
    );
    s.in_world = true;
    let mut h = Harness::with_built_in_art();
    let opened = h.frame(&s);
    assert_eq!(
        opened.acts,
        vec![Action::Examination {
            tabs: Some(tab::ALL)
        }],
        "the stone's use opens the window"
    );
    s.examined.open_asked = false;
    let mut bare = s.clone();
    bare.examined.pieces.clear();
    h.frame(&bare);
    let chrome: Vec<_> = h.list.quads.iter().map(|q| q.dst).collect();
    h.frame(&s);
    let lines =
        auberean_horizon::ui::examine::classic_lines(h.ui.examine.body().expect("drawn"), h.k());
    let rows = glyph_rows(&h, &chrome, lines);
    assert_eq!(
        rows.len(),
        3,
        "the title, the stone's number and the lesson: {rows:?}"
    );
    let step = rows[1] - rows[0];
    assert!(
        (rows[2] - rows[1] - 2.0 * step).abs() <= 2.0 * h.k(),
        "a blank line before the lesson: {rows:?}"
    );
}

/// The examine window opens 320 units wide, four fifths of what it was, and still holds its
/// contents on every screen and scale step: each tab's label whole on it at the button's own size,
/// the tabs in order and apart within the body.
#[test]
fn the_examine_window_opens_narrow_and_its_tabs_hold_their_labels_whole() {
    for (screen, step) in crate::play::fit::screens() {
        let mut h = crate::play::fit::on(screen, step);
        let s = tabbed();
        h.ui.keyed(Keyed::Examine, &s);
        h.frame(&s);
        let k = h.ui.layout_scale(screen);
        let body = h.ui.examine.body().expect("drawn");
        let at = h.ui.examine.rect().expect("drawn");
        assert!(
            (body.w - 320.0 * k).abs() <= 1.0,
            "{screen:?} {step:?}: {} wide",
            body.w / k
        );
        assert!(
            at.x >= 0.0 && at.right() <= screen.0 && at.y >= 0.0 && at.bottom() <= screen.1,
            "{screen:?} {step:?}: {at:?} on the screen"
        );
        let (tabs, lines) = parts(body, k);
        for (i, (r, (label, _))) in tabs.iter().zip(TABS).enumerate() {
            let wide =
                crate::measured(&h, |p| dereth_ui_kit::widgets::button_label_width(p, label));
            assert!(
                wide <= r.w - 12.0 * k,
                "{screen:?} {step:?} {label}: {wide} on a tab {} wide",
                r.w
            );
            assert!(
                r.x >= body.x && r.right() <= body.right(),
                "{screen:?} {step:?} {label}"
            );
            if let Some(next) = tabs.get(i + 1) {
                assert!(
                    r.right() < next.x,
                    "{screen:?} {step:?}: {label} apart from the next"
                );
            }
            assert!(
                r.bottom() <= lines.y,
                "{screen:?} {step:?}: above the lines"
            );
        }
    }
}

/// A profile longer than the window scrolls, and its lines wrap short of the scrollbar: no letter
/// drawn among them meets it, at the top or scrolled to the end.
#[test]
fn a_long_profiles_lines_stay_clear_of_the_scrollbar() {
    let mut s = tabbed();
    // One-letter words, so each line wrapped runs to within a letter of its room.
    let long = vec!["a"; 1500].join(" ");
    s.examined.request = Some(request(tab::GENERAL));
    s.examined.profile = Some(profile(tab::GENERAL, vec![text(tab::GENERAL, &long)]));
    s.examined.pieces = vec![Piece::Text(Some(long))];
    s.examined.asked = tab::GENERAL;
    s.examined.made = tab::GENERAL;
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.ui.keyed(Keyed::Examine, &s);
        h.frame(&s);
        let k = h.k();
        let (_, lines) = parts(h.ui.examine.body().expect("drawn"), k);
        let w = crate::measured(&h, dereth_ui_kit::widgets::scrollbar_width)
            .expect("the art's scrollbar")
            * k;
        let bar = lines.right() - w;
        for when in ["at the top", "at the end"] {
            let drawn: Vec<_> = h
                .list
                .quads
                .iter()
                .filter(|q| {
                    q.tex.is_some() && q.clip.is_some_and(|c| lines.contains(c.x + 0.5, c.y + 0.5))
                })
                .collect();
            assert!(drawn.len() > 100, "{screen:?} {when}: the lines drawn");
            for q in drawn {
                assert!(
                    q.dst.right() <= bar,
                    "{screen:?} {when}: a letter at {:?}, the bar from {bar}",
                    q.dst
                );
            }
            for _ in 0..200 {
                h.input.mouse = centre(lines);
                h.input.wheel = -1.0;
                h.frame(&s);
            }
        }
    }
}
