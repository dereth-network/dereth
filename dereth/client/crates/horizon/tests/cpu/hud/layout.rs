//! The HUD Layout window: what it changes shows at once, Save keeps it, and Close with changes
//! unsaved puts back the layout last kept.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_horizon::options::HorizonOptions;
use dereth_horizon::ui::chat;
use dereth_horizon::ui::game::{ChatWindow, GameState};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::Outcome;

use crate::Harness;

fn in_world() -> GameState {
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    state.chat_windows = chat::WINDOWS
        .iter()
        .map(|id| ChatWindow {
            id: *id,
            filter: 0,
            title: None,
        })
        .collect();
    state
}

/// The HUD Layout window's buttons, left to right: Save, Reset, Close.
fn button(h: &Harness, which: u8) -> (f32, f32) {
    let (x, y) = h.ui.windows.state(WindowId::Layout).pos.expect("placed");
    (x + 70.0 + 120.0 * f32::from(which), y + 171.0)
}

/// A click at `at`, and what it and the two frames after it asked for.
fn click_at(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Outcome {
    h.move_to(at.0, at.1);
    h.press();
    let mut out = h.frame(state);
    h.release();
    out.settings.extend(h.frame(state).settings);
    out.settings.extend(h.frame(state).settings);
    out
}

/// A drag with the left button from `from` to `to`, and what it asked for.
fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Outcome {
    h.move_to(from.0, from.1);
    h.press();
    let mut out = h.frame(state);
    h.move_to(to.0, to.1);
    out.settings.extend(h.frame(state).settings);
    h.release();
    out.settings.extend(h.frame(state).settings);
    out
}

/// The interface in the world with its layout kept in `file`.
fn in_world_with(file: &std::path::Path, options: HorizonOptions, state: &GameState) -> Harness {
    let mut h = Harness::new(options);
    h.frame(state);
    h.ui.hud.layout = dereth_horizon::ui::layout::HudLayout::load(file.to_owned());
    h.frame(state);
    h
}

fn open_layout(h: &mut Harness, state: &GameState) {
    h.ui.windows.open(WindowId::Layout, -1.0);
    h.frame(state);
}

fn minimap_middle(h: &Harness) -> (f32, f32) {
    let (_, r) =
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| *n == "minimap")
            .copied()
            .expect("the minimap's outline");
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// A layout file in a fresh folder of its own.
fn layout_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dereth-horizon-layout-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("horizon-hud-Tester.txt")
}

fn pop_out(h: &mut Harness, state: &GameState, name: &str) {
    let r = h.ui.hud.log.control(name).expect("the tab");
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
    let r = h.ui.hud.log.control("Pop Out").expect("the menu's Pop Out");
    click_at(h, state, (r.x + r.w / 2.0, r.y + r.h / 2.0));
    h.move_to(1900.0, 5.0);
    h.frame(state);
}

#[test]
fn moving_an_element_lights_save_and_only_save_keeps_it() {
    let state = in_world();
    let file = layout_file("save");
    let mut h = in_world_with(&file, HorizonOptions::default(), &state);
    open_layout(&mut h, &state);
    assert!(!h.ui.hud.layout_unsaved(), "nothing to save yet");
    let c = minimap_middle(&h);
    drag(&mut h, &state, c, (c.0 - 40.0, c.1 + 30.0));
    assert!(h.ui.hud.layout_unsaved(), "Save lights up");
    assert!(!file.exists(), "nothing is kept before Save");
    {
        let b = button(&h, 0);
        click_at(&mut h, &state, b)
    };
    let kept = std::fs::read_to_string(&file).expect("Save keeps the layout");
    assert_eq!(kept, "minimap -40.0 30.0 1.00\n");
    assert!(!h.ui.hud.layout_unsaved(), "and goes dark");
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn closing_hud_layout_with_changes_unsaved_puts_back_the_layout_last_kept() {
    let state = in_world();
    let file = layout_file("close");
    let mut h = in_world_with(&file, HorizonOptions::default(), &state);
    open_layout(&mut h, &state);
    let c = minimap_middle(&h);
    drag(&mut h, &state, c, (c.0 - 40.0, c.1 + 30.0));
    {
        let b = button(&h, 0);
        click_at(&mut h, &state, b)
    };
    let c = minimap_middle(&h);
    drag(&mut h, &state, c, (c.0 - 100.0, c.1));
    assert!(h.ui.hud.layout_unsaved());
    {
        let b = button(&h, 2);
        click_at(&mut h, &state, b)
    };
    assert!(!h.ui.windows.is_open(WindowId::Layout));
    let pl = h.ui.hud.layout.placements["minimap"];
    assert_eq!((pl.dx, pl.dy), (-40.0, 30.0), "back where it was saved");
    assert!(!h.ui.hud.layout_unsaved());
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn a_chat_window_moved_in_hud_layout_is_kept_by_save_and_put_back_by_close() {
    let state = in_world();
    let file = layout_file("chat");
    let mut options = HorizonOptions::default();
    options.chat_popped[1] = Some((1300.0, 600.0));
    let mut h = in_world_with(&file, options, &state);
    pop_out(&mut h, &state, "Combat");
    let at = h.ui.hud.log.popped_at(1).unwrap();
    open_layout(&mut h, &state);
    let mid = (at.0 + 100.0, at.1 + 120.0);
    let out = drag(&mut h, &state, mid, (mid.0 + 50.0, mid.1 + 30.0));
    assert_eq!(h.ui.hud.log.popped_at(1), Some((at.0 + 50.0, at.1 + 30.0)));
    assert!(out.settings.is_empty(), "not kept on letting go");
    assert!(h.ui.hud.layout_unsaved(), "Save lights up");
    // Close puts it back.
    {
        let b = button(&h, 2);
        click_at(&mut h, &state, b)
    };
    h.frame(&state);
    assert_eq!(h.ui.hud.log.popped_at(1), Some(at));
    // Moved again and saved, it is kept.
    open_layout(&mut h, &state);
    drag(&mut h, &state, mid, (mid.0 + 50.0, mid.1 + 30.0));
    let out = {
        let b = button(&h, 0);
        click_at(&mut h, &state, b)
    };
    assert!(out.settings.contains(&(
        "chat-tab-2-at".to_owned(),
        format!("{},{}", at.0 + 50.0, at.1 + 30.0)
    )));
    assert!(!h.ui.hud.layout_unsaved());
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
