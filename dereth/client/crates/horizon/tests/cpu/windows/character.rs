//! The character window's values: a value raised or lowered by magic, and its breakdown over its
//! row.
//!
//! Behaviour: none (experimental Horizon interface)

use std::sync::Arc;

use dereth_horizon::art::Art;
use dereth_horizon::draw::Rect;
use dereth_horizon::pieces::Pieces;
use dereth_horizon::ui::game::{GameState, Modifiers, Skill, StatRow};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::HorizonUi;

use crate::Harness;

fn character() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        stats: vec![
            StatRow {
                name: "Strength".into(),
                wire: 1,
                shown: "110".into(),
                modifiers: Modifiers::of(100, 110, 0),
                ..StatRow::default()
            },
            StatRow {
                name: "Health".into(),
                wire: 1,
                vital: true,
                shown: "120 / 130".into(),
                modifiers: Modifiers::of(150, 130, 0),
                ..StatRow::default()
            },
        ],
        skills: vec![
            Skill {
                name: "Melee Defense".into(),
                value: 235,
                modifiers: Modifiers::of(200, 235, 0),
                training: 3,
                id: 6,
                ..Skill::default()
            },
            Skill {
                name: "Healing".into(),
                value: 120,
                modifiers: Modifiers::of(150, 120, 0),
                training: 2,
                id: 21,
                ..Skill::default()
            },
            Skill {
                name: "Run".into(),
                value: 190,
                modifiers: Modifiers::of(200, 190, -10),
                training: 2,
                id: 24,
                ..Skill::default()
            },
        ],
        ..GameState::default()
    }
}

/// The rows of the page the character window shows, top to bottom: the stop at the left of each.
fn rows(h: &Harness) -> Vec<Rect> {
    let snap = &h.input.nav.last;
    let mut rows: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| snap.list_of(t).is_some())
        .map(|t| t.rect)
        .collect();
    rows.sort_by(|a, b| (a.y, a.x).partial_cmp(&(b.y, b.x)).unwrap());
    rows.dedup_by(|b, a| (a.y - b.y).abs() < 1.0);
    rows
}

/// The tooltip over the middle of `row`, a frame after the pointer is put there.
fn tip_over(h: &mut Harness, state: &GameState, row: Rect) -> Option<(String, Vec<String>)> {
    h.move_to(row.x + row.w / 2.0, row.y + row.h / 2.0);
    h.frame(state);
    h.ui.windows.tip().cloned()
}

fn opened(state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(state);
    h.frame(state);
    h
}

/// Turn the character window to its skills.
fn to_skills(h: &mut Harness, state: &GameState) {
    let snap = h.input.nav.last.clone();
    let bar = snap
        .tabs_in(snap.layers.len())
        .expect("the character window's tabs are noted");
    h.input.pad.tab_step = Some((bar, 1));
    h.frame(state);
    h.frame(state);
}

/// The colours of the text drawn inside `row`.
fn colours_in(h: &Harness, row: Rect) -> Vec<u32> {
    h.list
        .quads
        .iter()
        .filter(|q| {
            q.tex.is_some() && row.contains(q.dst.x + q.dst.w / 2.0, q.dst.y + q.dst.h / 2.0)
        })
        .map(|q| q.colour)
        .collect()
}

#[test]
fn an_attribute_row_s_tooltip_gives_its_base_and_what_magic_adds() {
    let state = character();
    let mut h = opened(&state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(
        tip_over(&mut h, &state, rows[0]),
        Some(("Strength".into(), vec!["100 (+10)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[1]),
        Some(("Health".into(), vec!["Maximum: 150 (-20)".into()]))
    );
}

#[test]
fn a_skill_row_s_tooltip_gives_its_base_and_what_magic_and_vitae_add() {
    let state = character();
    let mut h = opened(&state);
    to_skills(&mut h, &state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(
        tip_over(&mut h, &state, rows[0]),
        Some(("Melee Defense".into(), vec!["200 (+35)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[1]),
        Some(("Healing".into(), vec!["150 (-30)".into()]))
    );
    assert_eq!(
        tip_over(&mut h, &state, rows[2]),
        Some(("Run".into(), vec!["200".into(), "Vitae: -10".into()]))
    );
}

#[test]
fn a_skill_raised_by_magic_is_drawn_green_one_lowered_red_and_one_under_vitae_alone_plain() {
    const GREEN: u32 = 0xFF7C_E07C;
    const RED: u32 = 0xFFF0_6A5A;
    let state = character();
    let art = Art::new(Arc::new(Pieces::built_in().expect("the built-in pieces")));
    let mut h = Harness::new(Default::default());
    h.ui = HorizonUi::new(Arc::new(art), Default::default());
    h.ui.windows.open(WindowId::Character, -1.0);
    h.frame(&state);
    h.frame(&state);
    to_skills(&mut h, &state);
    let rows = rows(&h);
    assert_eq!(rows.len(), 3, "{rows:?}");
    // The value stands right of the row's stop, in the same line.
    let line = |r: Rect| Rect::new(r.x, r.y, 10_000.0, r.h);
    let buffed = colours_in(&h, line(rows[0]));
    assert!(buffed.contains(&GREEN), "Melee Defense: {buffed:x?}");
    assert!(!buffed.contains(&RED));
    let lowered = colours_in(&h, line(rows[1]));
    assert!(lowered.contains(&RED), "Healing: {lowered:x?}");
    assert!(!lowered.contains(&GREEN));
    let vitae = colours_in(&h, line(rows[2]));
    assert!(!vitae.is_empty(), "Run is drawn");
    assert!(
        !vitae.contains(&GREEN) && !vitae.contains(&RED),
        "Run: {vitae:x?}"
    );
}
