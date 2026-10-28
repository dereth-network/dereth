//! The host's own tier over the plugins, driven frame by frame with no art, no device and no
//! host: its question a dialog among the plugin windows, answered by its own buttons; its own
//! windows over every plugin window.

use dereth_ui_kit::draw::Rect;
use dereth_ui_kit::input::vk;

use crate::kit_tests::{controls, node, view, window, Harness};
use crate::{Input, Tier, WindowView};

/// The host's question for a file: two to pick from, give or refuse.
fn file_question(id: u64) -> dereth_client_contract::host_question::HostQuestion {
    dereth_client_contract::host_question::HostQuestion {
        id,
        title: "A file for a plugin".into(),
        asker: "Picker 1.0.0 (test.picker)".into(),
        text: "Picker asks for a file: a meta to run".into(),
        picks: vec!["a.met".into(), "b.met".into()],
        needs_pick: true,
        yes: "Give it".into(),
        no: Some("Refuse".into()),
    }
}

/// A plugin's window with one button.
fn plugin_window() -> WindowView {
    view(
        window(
            "main",
            vec![node("button").with("id", "go").with("text", "Go")],
        ),
        1,
    )
}

/// The host's question is a dialog: the plugins' windows are still drawn beside it, and only the
/// dialog itself, not the whole screen, is the layer's for a press.
/// Behaviour: plugins.files.the-hosts-question-for-a-file-is-a-dialog-beside-the-plugin-windows
#[test]
fn the_hosts_question_is_a_dialog_and_the_plugin_windows_are_still_drawn_beside_it() {
    let mut h = Harness::new();
    let views = [plugin_window()];
    h.frame(&views);
    h.question = Some(file_question(7));
    h.frame(&views);
    h.frame(&views);
    let drawn = h.drawn();
    assert!(
        drawn.windows.iter().all(|w| !w.2.quads.is_empty()),
        "the plugin's window is still drawn"
    );
    let dialog = h.layer.host.asking.drawn.frame;
    assert!(dialog.w > 0.0 && dialog.w < 1280.0, "{dialog:?}");
    assert!(drawn.hits.contains(&dialog), "the dialog takes its presses");
    assert!(
        !drawn.hits.contains(&Rect::new(0.0, 0.0, 1280.0, 720.0)),
        "the rest of the screen is not the dialog's"
    );
}

/// With the host's question up, a press on a plugin's window off the dialog reaches the plugin,
/// and the dialog's own first button answers it, with its id and the line picked.
#[test]
fn a_press_beside_the_question_reaches_the_plugin_and_its_own_buttons_answer_it() {
    let mut h = Harness::new();
    let views = [plugin_window()];
    h.frame(&views);
    let go = controls(&h)[0];
    h.question = Some(file_question(7));
    h.frame(&views);
    h.frame(&views);
    let dialog = h.layer.host.asking.drawn.frame;
    assert!(
        !dialog.contains(go.x + 4.0, go.y + 4.0),
        "the plugin's button is beside the dialog"
    );
    let out = h.click(&views, go.x + 4.0, go.y + 4.0);
    assert_eq!(
        out,
        [Input::Click {
            plugin: "test.kit".into(),
            window: "main".into(),
            node: "go".into(),
        }],
        "the plugin took its press"
    );
    assert_eq!(
        h.layer.take_reply(),
        None,
        "and the question was not answered"
    );
    let drawn = h.layer.host.asking.drawn.clone();
    let (_, row) = drawn.picks[1];
    h.click(&views, row.x + 4.0, row.y + row.h / 2.0);
    let yes = drawn.buttons[0];
    h.click(&views, yes.x + 4.0, yes.y + 4.0);
    assert_eq!(
        h.layer.take_reply(),
        Some(dereth_client_contract::host_question::HostReply {
            id: 7,
            yes: true,
            picked: Some(1),
        })
    );
}

/// Escape refuses the question.
/// Behaviour: plugins.files.escape-refuses-the-hosts-question
#[test]
fn escape_refuses_the_question() {
    let mut h = Harness::new();
    h.question = Some(file_question(3));
    h.frame(&[]);
    h.input.keys.push(vk::ESCAPE);
    h.frame(&[]);
    let reply = h.layer.take_reply().expect("answered");
    assert!(!reply.yes);
    assert_eq!(reply.id, 3);
}

/// A question that asks for a line picked (a file) cannot be answered yes until one is: a press
/// on its first button with none picked answers nothing; with a line picked, the answer names it.
/// Behaviour: plugins.files.a-file-is-given-only-once-the-player-has-picked-one
#[test]
fn a_question_asking_for_a_line_is_answered_yes_only_with_one_picked() {
    let mut h = Harness::new();
    h.question = Some(file_question(5));
    h.frame(&[]);
    let drawn = h.layer.host.asking.drawn.clone();
    let yes = drawn.buttons[0];
    h.click(&[], yes.x + 4.0, yes.y + 4.0);
    assert_eq!(h.layer.take_reply(), None, "nothing picked, nothing given");
    let (i, row) = drawn.picks[1];
    assert_eq!(i, 1);
    h.click(&[], row.x + 4.0, row.y + row.h / 2.0);
    h.click(&[], yes.x + 4.0, yes.y + 4.0);
    let reply = h.layer.take_reply().expect("answered");
    assert!(reply.yes);
    assert_eq!(reply.picked, Some(1), "the line picked");
}

/// The host's own window (the plugin panel) is drawn over every plugin window, as a window of the
/// interface: a press on it there is its own, and the plugin window under it takes nothing.
/// Behaviour: plugins.ui.the-hosts-own-window-stands-over-every-plugin-window
#[test]
fn the_hosts_own_window_stands_over_every_plugin_window_and_takes_the_press_there() {
    let mut h = Harness::new();
    let mut panel = view(
        window(
            "panel",
            vec![node("button").with("id", "disable").with("text", "Disable")],
        ),
        1,
    );
    panel.tier = Tier::Host;
    panel.plugin = "dereth.plugins".into();
    let plugin = plugin_window();
    h.frame(std::slice::from_ref(&panel));
    let host = h
        .layer
        .host
        .ui
        .rect_of("dereth.plugins", "panel")
        .expect("the panel is drawn");
    let disable = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|t| host.contains(t.x + 1.0, t.y + 1.0))
        .expect("the panel's button");
    // Where the plugin's button falls in its window, opened at the screen's corner.
    let mut probe = Harness::new();
    probe.layer.ui.place_hint("test.kit", "main", 0.0, 0.0);
    probe.frame(std::slice::from_ref(&plugin));
    let go = controls(&probe)[0];
    // The plugin's window opens with its button under the panel's.
    h.layer
        .ui
        .place_hint("test.kit", "main", disable.x - go.x, disable.y - go.y);
    let views = [plugin, panel];
    h.frame(&views);
    h.frame(&views);
    let main = h.rect("main");
    assert!(
        main.contains(disable.x + 4.0, disable.y + 4.0),
        "the plugin's window, its button under the panel's: {main:?}, {disable:?}"
    );
    let out = h.click(&views, disable.x + 4.0, disable.y + 4.0);
    assert!(out.is_empty(), "no plugin took the press: {out:?}");
    assert_eq!(
        h.layer.host.ui.take_inputs(),
        [Input::Click {
            plugin: "dereth.plugins".into(),
            window: "panel".into(),
            node: "disable".into(),
        }]
    );
}

/// A question taller than the screen keeps its buttons on it: its words scroll in the room left.
#[test]
fn a_question_taller_than_the_screen_keeps_its_buttons_on_it() {
    let mut h = Harness::new();
    let mut q = file_question(9);
    q.text = (0..80)
        .map(|i| format!("line {i} of a prompt long enough to fill the screen"))
        .collect::<Vec<_>>()
        .join("\n");
    h.question = Some(q);
    h.frame(&[]);
    h.frame(&[]);
    let drawn = h.layer.host.asking.drawn.clone();
    assert!(
        drawn.frame.bottom() <= 720.0 && drawn.frame.y >= 0.0,
        "{:?}",
        drawn.frame
    );
    assert!(
        drawn.buttons.iter().all(|b| b.bottom() <= 720.0),
        "{:?}",
        drawn.buttons
    );
}
