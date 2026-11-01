//! The options window on the retail layouts: its character page, its commands, its questions.

use auberean_ui::token::attr;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui, Value};
use auberean_ui_game::gameplay::{dialog, GamePlay, Notice, Request, View};
use auberean_ui_game::options::character::label;
use auberean_ui_game::options::helper::Control;
use auberean_ui_game::options::{self, notice, OptionWindow, Values};
use auberean_ui_game::toolbar::Toolbar;

use crate::common::records;

/// The player's options the tests start from: auto-target, the shortcut numbers and melting
/// without a question on; the radar shows monsters and people; two rows; others' damage at half.
const START: Values = Values {
    bits: 0x20 | 0x1 | 0x400,
    radar_mask: 0x3,
    shortcut_height: 2,
    damage_range: 0.5,
    read: 1,
};

fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

/// One frame with the player's options and the notices given.
fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, notices: &[Notice]) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let mut out = Vec::new();
    let view = View {
        options: Some(START),
        ..View::default()
    };
    p.tick(ui, &view, notices, &mut out);
    out
}

fn window(p: &GamePlay) -> &OptionWindow {
    p.child::<Toolbar>()
        .and_then(|t| t.window("OptionField"))
        .and_then(|c| c.child.as_any().downcast_ref::<OptionWindow>())
        .expect("the options window")
}

fn el(ui: &Ui, name: &str) -> Handle {
    ui.element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"))
}

/// Point at the middle of an element and click it, the clock moving between.
fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, h: Handle) -> Vec<Request> {
    let r = ui.screen_rect(h).expect("placed");
    ui.pointer_move((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let mut out = tick(ui, p, t, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(ui, p, t, &[]));
    }
    out
}

/// A page's button.
fn button(ui: &Ui, page: &str, name: &str) -> Handle {
    ui.child_recursive(el(ui, page), Token::of(name))
        .unwrap_or_else(|| panic!("{page}'s {name}"))
}

fn ghosted(ui: &Ui, h: Handle) -> bool {
    ui.node(h).is_some_and(|n| n.bool_attr(attr::GHOSTED))
}

/// The check box of a yes-or-no option on the character page.
fn checkbox(p: &GamePlay, l: u32) -> Handle {
    let c = window(p).character.as_ref().expect("the character page");
    match c.row(l).map(|r| &r.control) {
        Some(Control::Flag {
            checkbox: Some(b), ..
        }) => *b,
        _ => panic!("a check box"),
    }
}

/// The window open on its character page.
fn open_character_page(ui: &mut Ui, p: &mut GamePlay, t: &mut f64) {
    let b = el(ui, "OptionButton");
    click(ui, p, t, b);
    assert!(
        ui.shown(el(ui, "OptionField")),
        "the button shows the window"
    );
    let tab = el(ui, "CharacterTab");
    click(ui, p, t, tab);
    assert!(ui.shown(el(ui, "CharacterPage")), "the tab shows its page");
}

fn dialogs(out: &[Request]) -> Vec<(u32, u32, String)> {
    out.iter()
        .filter_map(|r| match r {
            Request::Dialog(d) => Some((d.id, d.kind, d.text.clone())),
            _ => None,
        })
        .collect()
}

fn answer(id: u32, yes: bool) -> Notice {
    Notice {
        native: notice::CLOSE_CONFIRMATION,
        words: vec![id, u32::from(yes)],
        ..Notice::default()
    }
}

/// The character page shows the player's options as the client read them (melting without a
/// question shown as its opposite, the radar's filters from its mask, the rows, others' damage
/// range) with Apply and Recall ghosted; a box ticked differently lights them; Apply sets the
/// changed option, tells the other panels and sends the options, and ghosts them again.
///
/// Behaviour: ui.options.the-character-page-shows-the-players-options-and-applies-the-changed-ones
#[test]
fn the_character_page_shows_the_players_options_and_applies_the_changed_ones() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    open_character_page(&mut ui, &mut p, &mut t);
    let c = window(&p).character.as_ref().expect("the character page");
    let on = |l| c.row(l).expect("a row").flag_now(&ui);
    assert!(on(label::AUTO_TARGET));
    assert!(on(label::SHORTCUT_NUMBERS));
    assert!(!on(label::DAMAGE_NUMBERS));
    assert!(!on(label::GOLD_MELTING), "melting without a question is on");
    assert!(on(label::RADAR_MONSTERS) && on(label::RADAR_NPCS));
    assert!(!on(label::RADAR_PCS));
    let range = c.row(label::OTHERS_DAMAGE_RANGE).expect("the range");
    assert!((range.number_now(&ui) - 0.5).abs() < 1e-4);
    assert!(!c.changed(&ui));
    let apply = button(&ui, "CharacterPage", "ApplyButton");
    let recall = button(&ui, "CharacterPage", "RecallButton");
    assert!(ghosted(&ui, apply) && ghosted(&ui, recall));
    // The box unticked, as a click unticks it.
    let b = checkbox(&p, label::AUTO_TARGET);
    ui.set_attr(b, attr::HIGHLIGHTED, Value::Bool(false));
    tick(&mut ui, &mut p, &mut t, &[]);
    assert!(
        !ghosted(&ui, apply) && !ghosted(&ui, recall),
        "a change lights them"
    );
    let out = click(&mut ui, &mut p, &mut t, apply);
    let set: Vec<&options::Request> = out
        .iter()
        .filter_map(|r| match r {
            Request::Options(o @ options::Request::Set { .. }) => Some(o),
            _ => None,
        })
        .collect();
    assert_eq!(
        set,
        [&options::Request::Set {
            changes: vec![(label::AUTO_TARGET, 0)],
            tell: true
        }]
    );
    assert!(ghosted(&ui, apply), "applied, nothing changed");
}

/// Leave World and Quit ask first, the commands ghosted while a question is up; yes to leaving
/// ends the session at the next tick; yes to quitting sends the options to keep and ends the
/// game.
///
/// Behaviour: ui.options.leave-world-and-quit-ask-first
#[test]
fn leave_world_and_quit_ask_first() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let b = el(&ui, "OptionButton");
    click(&mut ui, &mut p, &mut t, b);
    let leave = el(&ui, "LeaveButton");
    let out = click(&mut ui, &mut p, &mut t, leave);
    let asked = dialogs(&out);
    assert_eq!(asked.len(), 1, "{out:?}");
    assert_eq!(asked[0].1, dialog::CONFIRMATION);
    assert!(!asked[0].2.is_empty(), "the question's words");
    assert!(ghosted(&ui, leave) && ghosted(&ui, el(&ui, "QuitButton")));
    let out = tick(&mut ui, &mut p, &mut t, &[answer(asked[0].0, true)]);
    assert!(!ghosted(&ui, leave), "answered, the commands are back");
    assert!(!out.contains(&Request::LogOff), "not before the next tick");
    let out = tick(&mut ui, &mut p, &mut t, &[]);
    assert!(out.contains(&Request::LogOff), "{out:?}");

    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let b = el(&ui, "OptionButton");
    click(&mut ui, &mut p, &mut t, b);
    let quit = el(&ui, "QuitButton");
    let out = click(&mut ui, &mut p, &mut t, quit);
    let asked = dialogs(&out);
    assert_eq!(asked.len(), 1);
    let out = tick(&mut ui, &mut p, &mut t, &[answer(asked[0].0, false)]);
    assert!(!out.contains(&Request::Quit), "no keeps the game going");
    let out = click(&mut ui, &mut p, &mut t, quit);
    let asked = dialogs(&out);
    let out = tick(&mut ui, &mut p, &mut t, &[answer(asked[0].0, true)]);
    assert!(out.contains(&Request::KeepOptions) && out.contains(&Request::Quit));
}

/// Hidden with an option changed and not applied, the window asks whether to apply it; no puts
/// back what was applied.
///
/// Behaviour: ui.options.hidden-with-changes-the-window-asks-to-apply-them
#[test]
fn hidden_with_changes_the_window_asks_to_apply_them() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    open_character_page(&mut ui, &mut p, &mut t);
    let b = checkbox(&p, label::DAMAGE_NUMBERS);
    ui.set_attr(b, attr::HIGHLIGHTED, Value::Bool(true));
    let close = el(&ui, "OptionCloseButton");
    let out = click(&mut ui, &mut p, &mut t, close);
    assert!(!ui.shown(el(&ui, "OptionField")));
    let asked = dialogs(&out);
    assert_eq!(asked.len(), 1, "{out:?}");
    tick(&mut ui, &mut p, &mut t, &[answer(asked[0].0, false)]);
    let c = window(&p).character.as_ref().expect("the character page");
    assert!(!c.row(label::DAMAGE_NUMBERS).expect("row").flag_now(&ui));
    assert!(!c.changed(&ui), "put back");
}

/// The shortcut bar resized by hand tells the window its rows: they are set and the options sent
/// at once (no other panel told), and the page shows them.
///
/// Behaviour: ui.options.the-bars-rows-set-by-hand-are-kept-at-once
#[test]
fn the_bars_rows_set_by_hand_are_kept_at_once() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    tick(&mut ui, &mut p, &mut t, &[]);
    let rows = Notice {
        native: notice::SET_SHORTCUT_HEIGHT,
        words: vec![3],
        ..Notice::default()
    };
    let out = tick(&mut ui, &mut p, &mut t, &[rows]);
    assert!(out.contains(&Request::Options(options::Request::Set {
        changes: vec![(label::SHORTCUT_HEIGHT, 3)],
        tell: false
    })));
    let c = window(&p).character.as_ref().expect("the character page");
    let h = c.row(label::SHORTCUT_HEIGHT).expect("the rows");
    assert_eq!(h.choice_now(&ui), 3);
    assert!(!h.changed(&ui), "applied");
}

/// The client page, opened, asks the client to list its sections' preferences; a listing the
/// client gives fills the section it names with a row for each preference (a choice and a
/// yes-or-no one here), and the same listing is taken in once.
///
/// Behaviour: ui.options.the-client-page-shows-the-preferences-the-client-lists
#[test]
fn the_client_page_shows_the_preferences_the_client_lists() {
    use auberean_ui_game::options::client::{self, section, Listing};
    use auberean_ui_game::options::helper::Change;
    use auberean_ui_game::options::preference::{Choice, Description};
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let b = el(&ui, "OptionButton");
    let mut out = click(&mut ui, &mut p, &mut t, b);
    let tab = el(&ui, "ClientTab");
    out.extend(click(&mut ui, &mut p, &mut t, tab));
    assert!(ui.shown(el(&ui, "ClientPage")), "the tab shows its page");
    let asked = out.iter().any(|r| {
        matches!(r, Request::Options(options::Request::Preferences(client::Request::Attach(s)))
            if s.contains(&section::GRAPHICS) && s.len() == 8)
    });
    assert!(asked, "the page asks for its eight sections");
    let describe = |key: u32| Description {
        section: section::GRAPHICS,
        key,
        table: client::PREFERENCE_STRINGS,
        label: Token::of("ID_Graphics_TextureDetail").raw(),
        help: 0,
    };
    let mut detail = Choice::new();
    for (v, name) in [(0, "ID_Graphics_Value_Low"), (1, "ID_Graphics_Value_High")] {
        detail.assign_choice(v, (client::PREFERENCE_STRINGS, Token::of(name).raw()));
    }
    detail.set_default(1);
    detail.description = describe(0x4000_004F);
    let listing = Listing {
        serial: 1,
        sections: vec![(
            section::GRAPHICS,
            vec![
                Change::Choice(detail),
                client::flag(describe(0x4000_0042), true, false),
            ],
        )],
    };
    let rows_of_graphics = |p: &GamePlay| {
        let c = window(p).client.as_ref().expect("the client page");
        let (rows, sections) = c.rows();
        sections
            .iter()
            .find(|(s, _)| *s == section::GRAPHICS)
            .and_then(|(_, r)| match &rows.all[*r].control {
                Control::Section { rows, .. } => Some(rows.len()),
                _ => None,
            })
            .expect("the graphics section")
    };
    assert_eq!(rows_of_graphics(&p), 0, "nothing listed yet");
    for _ in 0..2 {
        t += 0.05;
        ui.use_time(t);
        let view = View {
            options: Some(START),
            preferences: Some(&listing),
            ..View::default()
        };
        p.tick(&mut ui, &view, &[], &mut Vec::new());
    }
    assert_eq!(rows_of_graphics(&p), 2, "a row for each listed preference");
}
