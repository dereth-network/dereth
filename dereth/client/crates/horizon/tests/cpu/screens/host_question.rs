//! The plugin host's question to the player (a file a plugin asks for), drawn as a dialog in
//! Horizon's own look over everything on every screen. A press on it is its own and Escape
//! refuses it; everything else goes on as it would, and the plugins' windows stay drawn.
//!
//! Behaviour: none (this client's own interface; the plugins' rows pin the question's rules)

use dereth_client_runtime::dialogs::HostQuestion;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::vk;

use crate::Harness;

fn question() -> HostQuestion {
    HostQuestion {
        id: 7,
        title: "A file for a plugin".into(),
        asker: "Test 1.0.0 (test.picker)".into(),
        text: "Test 1.0.0 asks for a file: a meta to run".into(),
        picks: Vec::new(),
        needs_pick: false,
        yes: "Give it".into(),
        no: Some("Refuse".into()),
    }
}

/// A press on the middle of `r`, one frame: the answer given, as its id and whether it was yes.
fn press_on(
    h: &mut Harness,
    state: &GameState,
    r: dereth_horizon::draw::Rect,
) -> Option<(u64, bool)> {
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.press();
    let out = h.frame(state);
    h.release();
    h.frame(state);
    out.host_reply.map(|r| (r.id, r.yes))
}

#[test]
fn before_the_world_the_question_is_a_dialog_and_a_press_on_its_first_button_says_yes() {
    let state = GameState {
        host_question: Some(question()),
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    assert_eq!(h.frame(&state).host_reply, None);
    assert_eq!(
        h.ui.host_buttons.len(),
        2,
        "drawn over the screens before the world"
    );
    let yes = h.ui.host_buttons[0];
    assert_eq!(press_on(&mut h, &state, yes), Some((7, true)));
    let no = h.ui.host_buttons[1];
    assert_eq!(press_on(&mut h, &state, no), Some((7, false)));
    h.frame(&GameState::default());
    assert!(h.ui.host_buttons.is_empty(), "gone once nothing is asked");
}

#[test]
fn in_the_world_escape_refuses_the_question() {
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        host_question: Some(question()),
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    assert_eq!(h.ui.host_buttons.len(), 2, "drawn over the game");
    h.input.keys.push(vk::ESCAPE);
    let out = h.frame(&state);
    assert_eq!(
        out.host_reply.map(|r| (r.id, r.yes)),
        Some((7, false)),
        "Escape refuses it"
    );
}

/// While the host's question is up a plugin's window is still drawn, among the windows and a place
/// for the pad's focus, and its minimap blip stays.
#[test]
fn beside_the_hosts_question_a_plugins_window_and_marks_are_still_drawn() {
    use std::sync::Arc;
    let node = |kind: &str| dereth_plugin_api::Value::map().with("kind", kind);
    let window = dereth_plugin_api::UiNode::window_from_value(
        &node("window")
            .with("id", "main")
            .with("title", "Looter")
            .with(
                "children",
                vec![node("button").with("id", "go").with("text", "Go")],
            ),
    )
    .expect("a window");
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = dereth_horizon::ui::plugins::PluginFrame {
        views: vec![dereth_plugin_ui::WindowView {
            tier: dereth_plugin_ui::Tier::Plugin,
            plugin: "test.looter".into(),
            window: "main".into(),
            tree: Arc::new(window),
            open: true,
            revision: 1,
            images: Vec::new(),
            size: None,
            by_player: true,
        }],
        ..Default::default()
    };
    h.ui.windows.plugins.frame.marks.radar = vec![dereth_plugin_ui::RadarMark {
        right: 10.0,
        ahead: 10.0,
        color: 0x0000_FF00,
    }];
    let plugin_drawn = |h: &Harness| {
        (
            h.input.nav.last.layers.iter().any(|l| {
                l.name.starts_with("Plugin ")
                    && l.kind == dereth_horizon::ui::nav::PanelKind::Window
            }),
            !h.ui.hud.plugin_radar.is_empty(),
        )
    };
    h.frame(&state);
    h.frame(&state);
    assert_eq!(plugin_drawn(&h), (true, true), "drawn with no question");
    state.host_question = Some(question());
    h.frame(&state);
    h.frame(&state);
    assert_eq!(
        plugin_drawn(&h),
        (true, true),
        "still drawn beside the question"
    );
    assert_eq!(h.ui.host_buttons.len(), 2, "and the question with them");
}
