//! A plugin's window seated among the interface's own: framed, stacked and closed like them, its
//! controls answering the pointer and the pad's focus, its HUD among the HUD's elements, its
//! marks on the minimap, and an entry on the pad's menu for each plugin with a window.
//!
//! Fixture: the interface with no art, a plugin's declared window handed over as the host hands
//! it.

use std::sync::Arc;

use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::nav::{Cursor, Dir, Kind, PanelKind, Stepped};
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::plugins::PluginFrame;
use dereth_plugin_api::{UiNode, Value};
use dereth_plugin_ui::{BarItem, Input, Tier, WindowView};

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

fn node(kind: &str) -> Value {
    Value::map().with("kind", kind)
}

/// The plugin's window: a button, a choice, a slider, a line to type in and tabs of two pages.
fn tree() -> UiNode {
    UiNode::window_from_value(
        &node("window")
            .with("id", "main")
            .with("title", "Looter")
            .with(
                "children",
                vec![
                    node("button").with("id", "go").with("text", "Loot now"),
                    node("choice")
                        .with("id", "mode")
                        .with("options", vec![Value::from("stop"), Value::from("loot")])
                        .with("value", "stop"),
                    node("slider")
                        .with("id", "range")
                        .with("min", 0_i64)
                        .with("max", 10_i64)
                        .with("value", 5_i64),
                    node("input").with("id", "name").with("value", ""),
                    node("tabs").with("id", "pages").with(
                        "children",
                        vec![
                            node("group")
                                .with("title", "One")
                                .with("children", vec![node("label").with("text", "a")]),
                            node("group")
                                .with("title", "Two")
                                .with("children", vec![node("label").with("text", "b")]),
                        ],
                    ),
                ],
            ),
    )
    .expect("a valid window")
}

fn plugin(open: bool) -> PluginFrame {
    PluginFrame {
        views: vec![WindowView {
            tier: Tier::Plugin,
            plugin: "test.looter".into(),
            window: "main".into(),
            tree: Arc::new(tree()),
            open,
            revision: 1,
            images: Vec::new(),
            size: None,
            by_player: true,
        }],
        bar: vec![BarItem {
            plugin: "test.looter".into(),
            name: "Looter".into(),
            state: "running".into(),
            open,
        }],
        bar_at: dereth_plugin_ui::BarPlace {
            at: (8.0, 300.0),
            width: None,
        },
        marks: dereth_plugin_ui::Marks::default(),
        commands: Vec::new(),
        huds_unlocked: false,
    }
}

/// The plugin window's layer for the pad's focus, as last drawn.
fn plugin_layer(h: &Harness) -> usize {
    let snap = &h.input.nav.last;
    1 + snap
        .layers
        .iter()
        .position(|l| l.name.starts_with("Plugin") && l.kind == PanelKind::Window)
        .expect("the plugin's window is a window of the interface")
}

fn press_at(h: &mut Harness, state: &GameState, r: Rect) -> Vec<Input> {
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.press();
    h.frame(state);
    h.release();
    h.frame(state);
    h.ui.windows.plugins.take_inputs()
}

/// A plugin's window opens as one of the interface's windows: in the window order, its own layer
/// for the pad's focus, and a press on its button reaches the plugin.
#[test]
fn a_plugin_window_is_one_of_the_interface_s_windows_and_its_button_reaches_the_plugin() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = plugin(true);
    h.frame(&state);
    h.frame(&state);
    let id = WindowId::Plugin(0);
    assert!(h.ui.windows.is_open(id));
    let rect =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(w, _)| *w == id)
            .map(|(_, r)| r)
            .expect("in the window order");
    let layer = plugin_layer(&h);
    let snap = h.input.nav.last.clone();
    let mut targets: Vec<_> = snap.targets.iter().filter(|t| t.layer == layer).collect();
    targets.sort_by(|a, b| a.rect.y.total_cmp(&b.rect.y));
    assert!(targets.iter().all(|t| rect.contains(t.rect.x, t.rect.y)));
    let go = targets[0].rect;
    let out = press_at(&mut h, &state, go);
    assert_eq!(
        out,
        vec![Input::Click {
            plugin: "test.looter".into(),
            window: "main".into(),
            node: "go".into(),
        }]
    );
    // The host closes it: so does the interface.
    h.ui.windows.plugins.frame = plugin(false);
    h.frame(&state);
    assert!(!h.ui.windows.is_open(id));
}

/// The pad's focus goes into a plugin's window and steps down its controls, reaching its slider
/// (left and right move its knob) and its line of text (the on-screen keyboard's), the shoulders
/// turn its tabs, and the pad's cancel asks the plugin to close it.
#[test]
fn the_pad_moves_through_a_plugin_window_turns_its_tabs_and_cancels_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = plugin(true);
    h.frame(&state);
    h.frame(&state);
    let layer = plugin_layer(&h);
    let snap = h.input.nav.last.clone();
    let mut cursor = Cursor::default();
    cursor.enter(&snap, layer);
    let mut kinds = Vec::new();
    for _ in 0..12 {
        if let Some(t) = cursor.focus {
            assert_eq!(t.layer, layer, "the focus keeps to the plugin's window");
            kinds.push(t.kind);
        }
        if cursor.step(&snap, Dir::Down) != Stepped::Moved {
            break;
        }
    }
    assert!(
        kinds.iter().any(|k| matches!(k, Kind::Slider { .. })),
        "the d-pad reaches the slider: {kinds:?}"
    );
    assert!(
        kinds.iter().any(|k| matches!(k, Kind::Text)),
        "and the line to type in: {kinds:?}"
    );
    // The shoulders turn the window's tabs.
    let bar = snap.tabs_in(layer).expect("the plugin's tabs are noted");
    h.input.pad.tab_step = Some((bar, 1));
    h.frame(&state);
    assert_eq!(
        h.ui.windows.plugins.take_inputs(),
        vec![Input::Select {
            plugin: "test.looter".into(),
            window: "main".into(),
            node: "pages".into(),
            row: 2,
        }]
    );
    // The pad's cancel is the window's close: the plugin hears it, and closes it.
    let window = snap.layer(layer).expect("its layer").rect;
    h.input.pad.close = Some(window);
    h.frame(&state);
    assert_eq!(
        h.ui.windows.plugins.take_inputs(),
        vec![Input::Close {
            plugin: "test.looter".into(),
            window: "main".into(),
        }]
    );
    assert!(
        h.ui.windows.is_open(WindowId::Plugin(0)),
        "the window stays until the plugin closes it"
    );
}

/// The pad's menu has an entry for a plugin with a window, and choosing it opens or closes that
/// window as the plugin bar's button does.
#[test]
fn the_pad_s_menu_has_an_entry_for_a_plugin_that_opens_its_window() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = plugin(false);
    h.input.pad.menu = true;
    h.frame(&state);
    h.frame(&state);
    let snap = h.input.nav.last.clone();
    let top = snap.layers.len();
    let mut buttons: Vec<Rect> = snap
        .targets
        .iter()
        .filter(|t| t.layer == top && t.rect.w > 100.0)
        .map(|t| t.rect)
        .collect();
    buttons.sort_by(|a, b| a.y.total_cmp(&b.y));
    assert_eq!(
        buttons.len(),
        11,
        "the game's windows, the plugin, logging out"
    );
    let out = press_at(&mut h, &state, buttons[9]);
    assert_eq!(
        out,
        vec![Input::Bar {
            plugin: "test.looter".into()
        }]
    );
}

/// A plugin's blip is drawn on the minimap.
#[test]
fn a_plugin_s_blip_is_drawn_on_the_minimap() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    let mut frame = plugin(false);
    frame.marks.radar = vec![dereth_plugin_ui::RadarMark {
        right: 0.0,
        ahead: 20.0,
        color: 0x12_34_56,
    }];
    h.ui.windows.plugins.frame = frame;
    h.frame(&state);
    assert!(
        h.list
            .quads
            .iter()
            .any(|q| q.tex.is_none() && q.colour == 0xFF12_3456),
        "a dot in the plugin's colour"
    );
}

/// A press on the map window's picture is told to the plugins with the place under the pointer,
/// the button and the modifiers held: a left press on Holtburg's pixel reads back the place whose
/// marker lands on that pixel, a right press with Shift says so, and a press on the picture's
/// margin, beyond the world, is told to no one.
#[test]
fn a_press_on_the_map_tells_the_plugins_the_place_under_the_pointer() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    use dereth_ui_screens::mapradar::map::{place_marker_on_map, SHIPPED_NOTE_BINDING};
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(&state);
    let area =
        h.ui.windows
            .map_picture
            .expect("the map window draws its picture");
    let fit = area.w / 257.0;
    let markers = SHIPPED_NOTE_BINDING.2;
    // The middle of the picture's pixel `(px, py)`, on the screen.
    #[allow(clippy::cast_precision_loss)]
    let at = |px: i32, py: i32| {
        (
            area.x + (px as f32 + 0.5) * fit,
            area.y + (py as f32 + 0.5) * fit,
        )
    };
    let press = |h: &mut Harness, (x, y): (f32, f32), button: usize, shift: bool| {
        h.move_to(x, y);
        h.input.shift = shift;
        h.input.down[button] = true;
        h.input.pressed[button] = true;
        h.frame(&state);
        h.input.down[button] = false;
        h.input.released[button] = true;
        h.input.shift = false;
        h.frame(&state);
        h.ui.windows.plugins.take_inputs()
    };

    let holtburg = place_marker_on_map(markers, 33.8, 42.2, (0, 0));
    let told = press(&mut h, at(holtburg.0, holtburg.1), 0, false);
    let [Input::Map(MapClick {
        ns,
        ew,
        button: MapButton::Left,
        ctrl: false,
        shift: false,
        alt: false,
    })] = told.as_slice()
    else {
        panic!("one left press on the map: {told:?}");
    };
    #[allow(clippy::cast_possible_truncation)]
    let back = place_marker_on_map(markers, *ew as f32, *ns as f32, (0, 0));
    assert_eq!(
        back, holtburg,
        "a marker at {ns}, {ew} lands on the pixel pressed"
    );
    assert!(
        (ns - 42.2).abs() < 0.85 && (ew - 33.8).abs() < 0.85,
        "Holtburg to within a pixel: {ns}, {ew}"
    );

    let told = press(&mut h, at(holtburg.0, holtburg.1), 1, true);
    assert!(
        matches!(
            told.as_slice(),
            [Input::Map(MapClick {
                button: MapButton::Right,
                shift: true,
                ..
            })]
        ),
        "a right press with Shift: {told:?}"
    );

    let told = press(&mut h, at(markers.x0 - 3, markers.y0 + 50), 0, false);
    assert!(told.is_empty(), "the margin is beyond the world: {told:?}");
}

/// The places a HUD may be anchored to.
const ANCHORS: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

/// A plugin with a HUD at each of the screen's corners, edges and middle, each holding one wide
/// button named for its place.
fn huds_everywhere() -> PluginFrame {
    PluginFrame {
        views: ANCHORS
            .iter()
            .map(|at| WindowView {
                tier: Tier::Plugin,
                plugin: "test.over".into(),
                window: format!("hud_{at}"),
                tree: Arc::new(
                    UiNode::window_from_value(
                        &node("hud")
                            .with("id", format!("hud_{at}"))
                            .with("anchor", *at)
                            .with(
                                "children",
                                vec![node("button")
                                    .with("id", *at)
                                    .with("text", *at)
                                    .with("width", 16_i64)],
                            ),
                    )
                    .expect("a valid HUD"),
                ),
                open: true,
                revision: 1,
                images: Vec::new(),
                size: None,
                by_player: true,
            })
            .collect(),
        ..PluginFrame::default()
    }
}

/// A press in the middle of `r`: what reached the plugins.
fn click_middle(h: &mut Harness, state: &GameState, r: Rect) -> Vec<Input> {
    h.ui.windows.plugins.take_inputs();
    press_at(h, state, r)
}

/// In the world a click on one of the HUD's own controls is the game's even with a plugin's HUD
/// drawn over it, and a click on a plugin HUD's button where the HUD has nothing reaches the
/// plugin.
#[test]
fn in_the_world_a_click_on_the_huds_own_elements_is_the_games_with_a_plugins_hud_over_them() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    // The stance's toggle between peace and combat.
    let stance =
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(name, _)| name == "stance")
            .map(|(_, r)| *r)
            .expect("the stance");
    let toggle = h
        .input
        .nav
        .last
        .targets
        .iter()
        .find(|t| t.kind == Kind::Plain && stance.contains(t.rect.x + 1.0, t.rect.y + 1.0))
        .map(|t| t.rect)
        .expect("the stance's toggle");
    h.ui.windows.plugins.frame = huds_everywhere();
    h.frame(&state);
    h.frame(&state);
    let chat =
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| n == "chat")
            .map(|(_, r)| *r)
            .expect("the chat");
    // The player puts the bottom left HUD over the toggle, and the bottom one over the chat's log.
    let log_hud = "plugin:test.over/hud_bottom";
    let under_log =
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| n == log_hud)
            .map(|(_, r)| *r)
            .expect("the bottom HUD");
    h.ui.hud.layout.placements.insert(
        log_hud.into(),
        dereth_horizon::ui::layout::Placement {
            dx: (chat.x + chat.w / 2.0 - under_log.x - under_log.w / 2.0).round(),
            dy: (chat.y + 40.0 - under_log.y - under_log.h / 2.0).round(),
            scale: 1.0,
        },
    );
    let name = "plugin:test.over/hud_bottom_left";
    let hud =
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| *r)
            .expect("the bottom left HUD");
    h.ui.hud.layout.placements.insert(
        name.into(),
        dereth_horizon::ui::layout::Placement {
            dx: (toggle.x + toggle.w / 2.0 - hud.x - hud.w / 2.0).round(),
            dy: (toggle.y + toggle.h / 2.0 - hud.y - hud.h / 2.0).round(),
            scale: 1.0,
        },
    );
    h.frame(&state);
    h.frame(&state);
    let outlines = h.ui.hud.layout.outlines.clone();
    let game: Vec<Rect> = outlines
        .iter()
        .filter(|(name, _)| !name.starts_with("plugin:"))
        .map(|(_, r)| *r)
        .collect();
    let over = outlines
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| *r)
        .expect("the bottom left HUD");
    let middle = (over.x + over.w / 2.0, over.y + over.h / 2.0);
    assert!(
        toggle.contains(middle.0, middle.1),
        "the HUD over the toggle"
    );
    h.ui.windows.plugins.take_inputs();
    h.move_to(middle.0, middle.1);
    h.press();
    let pressed = h.frame(&state);
    h.release();
    let released = h.frame(&state);
    assert_eq!(
        h.ui.windows.plugins.take_inputs(),
        Vec::new(),
        "the click over the toggle is not the plugin's"
    );
    assert!(
        pressed
            .actions
            .iter()
            .chain(&released.actions)
            .any(|a| *a == dereth_horizon::ui::ACTION_TOGGLE_COMBAT),
        "it is the toggle's"
    );
    let on_log = outlines
        .iter()
        .find(|(n, _)| n == log_hud)
        .map(|(_, r)| *r)
        .expect("the bottom HUD");
    assert!(
        chat.contains(on_log.x + on_log.w / 2.0, on_log.y + on_log.h / 2.0),
        "the HUD over the chat's log"
    );
    assert_eq!(
        click_middle(&mut h, &state, on_log),
        Vec::new(),
        "the click over the chat's log is not the plugin's"
    );
    let clear = outlines
        .iter()
        .filter(|(n, _)| n.starts_with("plugin:"))
        .map(|(_, r)| *r)
        .find(|r| {
            !game
                .iter()
                .any(|g| g.contains(r.x + r.w / 2.0, r.y + r.h / 2.0))
        })
        .expect("a plugin HUD where the HUD has none");
    assert!(
        matches!(
            click_middle(&mut h, &state, clear).as_slice(),
            [Input::Click { .. }]
        ),
        "the click where the HUD has nothing is the plugin's"
    );
}

/// Before the world a click on one of the screen's controls is the screen's even with a plugin's
/// HUD drawn over it, and a click on a plugin HUD's button where the screen has none reaches the
/// plugin.
#[test]
fn before_the_world_a_click_on_the_screens_controls_is_the_screens_with_a_plugins_hud_over_them() {
    let state = GameState::default();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let controls: Vec<Rect> = h.input.nav.last.targets.iter().map(|t| t.rect).collect();
    assert!(!controls.is_empty(), "the screen has controls");
    h.ui.windows.plugins.frame = huds_everywhere();
    h.frame(&state);
    h.frame(&state);
    let huds: Vec<Rect> = ANCHORS
        .iter()
        .filter_map(|at| {
            h.ui.windows
                .plugins
                .layer
                .ui
                .rect_of("test.over", &format!("hud_{at}"))
        })
        .collect();
    assert_eq!(huds.len(), 9);
    let on_control = |r: &Rect| {
        controls
            .iter()
            .any(|c| c.contains(r.x + r.w / 2.0, r.y + r.h / 2.0))
    };
    let over = *huds
        .iter()
        .find(|r| on_control(r))
        .expect("a plugin HUD over one of the screen's controls");
    assert!(
        !h.ui
            .windows
            .plugins
            .layer
            .drawn()
            .hits
            .iter()
            .any(|r| over.contains(r.x + r.w / 2.0, r.y + r.h / 2.0)),
        "the HUD's button over the control is no place for the pointer or the pad's focus"
    );
    let clear = *huds
        .iter()
        .find(|r| !on_control(r))
        .expect("a plugin HUD where the screen has none");
    assert_eq!(
        click_middle(&mut h, &state, over),
        Vec::new(),
        "the click over the screen's control is the screen's"
    );
    assert!(
        matches!(
            click_middle(&mut h, &state, clear).as_slice(),
            [Input::Click { .. }]
        ),
        "the click where the screen has nothing is the plugin's"
    );
}

/// Before the world a plugin's window over one of the screen's controls takes the click there:
/// the control under it does not act.
#[test]
fn before_the_world_a_plugins_window_over_the_screens_control_takes_the_click_there() {
    let state = GameState {
        connected: true,
        connect_phase: dereth_horizon::ui::game::ConnectPhase::Ready,
        host: "127.0.0.1:9000".into(),
        world: Some("Test".into()),
        ..GameState::default()
    };
    let exit = dereth_horizon::ui::pregame::LobbyLayout::new((1920.0, 1080.0), 1.0).exit;
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    let press_exit = |h: &mut Harness| {
        h.move_to(exit.x + exit.w / 2.0, exit.y + exit.h / 2.0);
        h.press();
        let pressed = h.frame(&state);
        h.release();
        let released = h.frame(&state);
        pressed.quit || released.quit
    };
    h.ui.windows.plugins.frame = plugin(true);
    h.ui.windows
        .plugins
        .layer
        .ui
        .place_hint("test.looter", "main", exit.x - 40.0, exit.y - 60.0);
    h.frame(&state);
    h.frame(&state);
    let window =
        h.ui.windows
            .plugins
            .layer
            .ui
            .rect_of("test.looter", "main")
            .expect("the plugin's window");
    assert!(
        window.contains(exit.x + exit.w / 2.0, exit.y + exit.h / 2.0),
        "the window stands over Exit"
    );
    assert!(!press_exit(&mut h), "the click on the window is not Exit's");
    h.ui.windows.plugins.frame = plugin(false);
    h.frame(&state);
    h.frame(&state);
    assert!(press_exit(&mut h), "with the window gone, Exit answers");
}

/// A plugin's window its plugin opened of its own accord does not take the pad's focus from the
/// window the player is in; one the player opened takes it.
#[test]
fn a_plugin_window_takes_the_pads_focus_only_when_the_player_opened_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    let mut cursor = Cursor::default();
    let frames = |h: &mut Harness, cursor: &mut Cursor, n: usize| {
        for _ in 0..n {
            h.frame(&state);
            cursor.follow(&h.input.nav.last);
        }
    };
    let focus_in = |h: &Harness, c: &Cursor| {
        c.focus
            .and_then(|f| h.input.nav.last.layer(f.layer))
            .map(|l| l.name.clone())
    };
    // The player opens the inventory: the focus goes into it.
    h.input.actions.push(0x1000_0019);
    frames(&mut h, &mut cursor, 4);
    let inventory = focus_in(&h, &cursor);
    assert!(inventory.is_some(), "the focus in the inventory");
    // The plugin opens its window of its own accord.
    let mut nag = plugin(true);
    nag.views[0].by_player = false;
    h.ui.windows.plugins.frame = nag;
    frames(&mut h, &mut cursor, 3);
    assert!(h.ui.windows.is_open(WindowId::Plugin(0)));
    assert_eq!(focus_in(&h, &cursor), inventory, "kept in the inventory");
    // Closed, and opened again by the player.
    h.ui.windows.plugins.frame = plugin(false);
    frames(&mut h, &mut cursor, 2);
    h.ui.windows.plugins.frame = plugin(true);
    frames(&mut h, &mut cursor, 3);
    assert_eq!(
        focus_in(&h, &cursor).as_deref(),
        Some("Plugin 0"),
        "the window the player opened takes it"
    );
}

/// The plugin window's place on screen, as last drawn.
fn plugin_rect(h: &Harness) -> Rect {
    h.ui.windows
        .rects(h.ui.layout_scale(h.screen))
        .into_iter()
        .find(|(w, _)| *w == WindowId::Plugin(0))
        .map(|(_, r)| r)
        .expect("the plugin's window is open")
}

/// The left button pressed at `from`, the pointer moved to `to` and let go there, a frame each.
fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) {
    h.move_to(from.0, from.1);
    h.press();
    h.frame(state);
    h.move_to(to.0, to.1);
    h.frame(state);
    h.release();
    h.frame(state);
    h.frame(state);
}

/// As the shell does each frame: the places the interface has are remembered, and the places
/// remembered are handed back.
fn remember(h: &mut Harness, kept: &mut dereth_plugin_ui::Places) {
    for (key, at) in h.ui.windows.plugin_places() {
        kept.retain(|(k, _)| *k != key);
        kept.push((key, at));
    }
    for ((plugin, window), (x, y)) in kept.clone() {
        h.ui.windows.plugins.ui.place_hint(&plugin, &window, x, y);
    }
}

/// A plugin's window is resized by its edges among the interface's windows: its right edge
/// widens it, its left edge widens it leftwards, the other edges staying put.
#[test]
fn a_plugin_window_takes_the_size_its_edges_are_dragged_to() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = plugin(true);
    h.ui.windows
        .plugins
        .ui
        .place_hint("test.looter", "main", 600.0, 200.0);
    h.frame(&state);
    h.frame(&state);
    let a = plugin_rect(&h);
    let mid = a.y + a.h / 2.0;
    drag(
        &mut h,
        &state,
        (a.right() - 2.0, mid),
        (a.right() + 118.0, mid),
    );
    let b = plugin_rect(&h);
    assert_eq!((b.x, b.y, b.h), (a.x, a.y, a.h));
    assert!((b.w - (a.w + 120.0)).abs() < 0.5, "{a:?} to {b:?}");
    drag(&mut h, &state, (b.x + 2.0, mid), (b.x - 78.0, mid));
    let c = plugin_rect(&h);
    assert!((c.right() - b.right()).abs() < 0.5, "{b:?} to {c:?}");
    assert!((c.w - (b.w + 80.0)).abs() < 0.5, "{b:?} to {c:?}");
}

/// A plugin's window closed and opened again comes back where the player last moved it, the
/// place going by way of the host's memory as the shell carries it.
#[test]
fn a_plugin_window_opened_again_comes_back_where_the_player_left_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    let mut kept = vec![(
        ("test.looter".to_owned(), "main".to_owned()),
        (500.0, 200.0),
    )];
    h.ui.windows.plugins.frame = plugin(true);
    remember(&mut h, &mut kept);
    h.frame(&state);
    remember(&mut h, &mut kept);
    h.frame(&state);
    let a = plugin_rect(&h);
    assert_eq!(
        (a.x, a.y),
        (500.0, 200.0),
        "opened where it was left before"
    );
    // Moved by its title bar.
    h.move_to(a.x + a.w / 2.0, a.y + 10.0);
    h.press();
    remember(&mut h, &mut kept);
    h.frame(&state);
    h.move_to(a.x + a.w / 2.0 + 300.0, a.y + 110.0);
    remember(&mut h, &mut kept);
    h.frame(&state);
    h.release();
    remember(&mut h, &mut kept);
    h.frame(&state);
    let moved = plugin_rect(&h);
    assert_eq!((moved.x, moved.y), (800.0, 300.0), "moved");
    // Closed and opened again.
    h.ui.windows.plugins.frame = plugin(false);
    for _ in 0..3 {
        remember(&mut h, &mut kept);
        h.frame(&state);
    }
    h.ui.windows.plugins.frame = plugin(true);
    for _ in 0..3 {
        remember(&mut h, &mut kept);
        h.frame(&state);
    }
    let back = plugin_rect(&h);
    assert_eq!(
        (back.x, back.y),
        (moved.x, moved.y),
        "back where it was left"
    );
}

/// A plugin's window left near the screen's bottom right stays on the screen as the screen
/// shrinks.
#[test]
fn a_plugin_window_stays_on_the_screen_as_the_screen_shrinks() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.plugins.frame = plugin(true);
    h.ui.windows
        .plugins
        .ui
        .place_hint("test.looter", "main", 1500.0, 700.0);
    h.frame(&state);
    h.frame(&state);
    let inside = |h: &Harness, r: Rect| {
        r.x >= 0.0 && r.y >= 0.0 && r.right() <= h.screen.0 && r.bottom() <= h.screen.1
    };
    let a = plugin_rect(&h);
    assert!(inside(&h, a), "{a:?}");
    h.screen = (1280.0, 720.0);
    h.frame(&state);
    h.frame(&state);
    let b = plugin_rect(&h);
    assert!(inside(&h, b), "{b:?} on a 1280x720 screen");
}

/// The plugin bar, with more plugins than the screen holds, is no wider than the screen.
#[test]
fn a_plugin_bar_of_many_plugins_is_no_wider_than_the_screen() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    let mut frame = plugin(false);
    frame.bar = (0..100)
        .map(|i| BarItem {
            plugin: format!("test.p{i}"),
            name: format!("Plugin number {i}"),
            state: "running".into(),
            open: false,
        })
        .collect();
    h.ui.windows.plugins.frame = frame;
    h.frame(&state);
    h.frame(&state);
    let bar = h.ui.windows.plugins.hits.last().copied().expect("the bar");
    assert!(bar.x >= 0.0 && bar.right() <= h.screen.0, "{bar:?}");
}

/// In Horizon's own art, a plugin's button is as wide as its label as the button draws it (a point
/// larger than the window's text) and, at each end, its cap (half its height) and a gap: the room
/// either side of the label is past the cap, the same at both ends.
#[test]
fn a_plugin_button_in_horizon_s_art_keeps_room_past_its_caps_at_both_ends() {
    let state = in_world();
    let art = Arc::new(dereth_horizon::art::Art::new(Arc::new(
        dereth_horizon::pieces::Pieces::built_in().expect("the art is built in"),
    )));
    let mut h = Harness {
        ui: dereth_horizon::ui::HorizonUi::new(Arc::clone(&art), Default::default()),
        list: dereth_horizon::draw::DrawList::default(),
        input: dereth_horizon::ui::input::InputFrame::default(),
        screen: (1920.0, 1080.0),
    };
    let tree = UiNode::window_from_value(
        &node("window")
            .with("id", "main")
            .with("title", "Tank")
            .with(
                "children",
                vec![node("row").with(
                    "children",
                    vec![
                        node("button").with("id", "start").with("text", "Start"),
                        node("button").with("id", "stop").with("text", "Stop"),
                    ],
                )],
            ),
    )
    .expect("a valid window");
    let mut frame = plugin(true);
    frame.views[0].tree = Arc::new(tree);
    h.ui.windows.plugins.frame = frame;
    h.frame(&state);
    h.frame(&state);
    let layer = plugin_layer(&h);
    let mut buttons: Vec<Rect> = h
        .input
        .nav
        .last
        .targets
        .iter()
        .filter(|t| t.layer == layer)
        .map(|t| t.rect)
        .collect();
    buttons.sort_by(|a, b| a.x.total_cmp(&b.x));
    let scale = h.ui.layout_scale(h.screen);
    let mut list = dereth_horizon::draw::DrawList::default();
    let p = dereth_horizon::ui::paint::Painter {
        list: &mut list,
        art: &art,
        scale,
        screen: h.screen,
        fade: 1.0,
    };
    for (label, r) in ["Start", "Stop"].into_iter().zip(&buttons) {
        let drawn = dereth_ui_kit::widgets::button_label_width(&p, label);
        let room = (r.w - drawn) / 2.0;
        assert!(
            room >= r.h / 2.0 + 2.0 * scale,
            "{label}: {room} either side of a {drawn}-wide label on a {r:?} button"
        );
    }
}
