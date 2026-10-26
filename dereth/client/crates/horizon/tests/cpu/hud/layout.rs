//! The HUD Layout window: what it changes shows at once, Save keeps it, and Close with changes
//! unsaved puts back the layout last kept. A layout is kept per world and character.
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
            .cloned()
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

/// In the world at `host`, given port `port`, as Tester.
fn on_world(host: &str, port: u32) -> GameState {
    GameState {
        host: host.into(),
        address: dereth_horizon::state::world_address(host, port),
        ..in_world()
    }
}

/// A fresh settings folder of its own.
fn settings_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dereth-horizon-layouts-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The interface in the world as `state` says, keeping its settings in `dir`.
fn in_folder(dir: &std::path::Path, state: &GameState) -> Harness {
    let mut h = Harness::new(HorizonOptions::default());
    h.ui.hud.settings_dir = Some(dir.to_owned());
    h.frame(state);
    h
}

#[test]
fn two_worlds_on_one_host_keep_their_own_layouts_for_characters_of_one_name() {
    let dir = settings_folder("worlds");
    let one = on_world("retail.dereth.network", 19000);
    let two = on_world("retail.dereth.network", 29000);
    let mut h = in_folder(&dir, &one);
    open_layout(&mut h, &one);
    let c = minimap_middle(&h);
    drag(&mut h, &one, c, (c.0 - 40.0, c.1 + 30.0));
    {
        let b = button(&h, 0);
        click_at(&mut h, &one, b)
    };
    assert!(
        dir.join("horizon-hud-retail_dereth_network_19000-Tester.txt")
            .exists(),
        "kept by the world's host and port and the character's name"
    );
    let other = in_folder(&dir, &two);
    assert!(
        other.ui.hud.layout.placements.is_empty(),
        "the other world's Tester keeps the layout as it starts"
    );
    let again = in_folder(&dir, &one);
    let pl = again.ui.hud.layout.placements["minimap"];
    assert_eq!((pl.dx, pl.dy), (-40.0, 30.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_layout_kept_by_the_name_alone_is_carried_over_to_each_world_and_left_in_place() {
    use dereth_horizon::ui::layout::file_name;
    let dir = settings_folder("carried");
    let earlier = dir.join("horizon-hud-Tester.txt");
    std::fs::write(&earlier, "minimap -40.0 30.0 1.00\n").unwrap();
    let one = on_world("retail.dereth.network", 19000);
    let h = in_folder(&dir, &one);
    let pl = h.ui.hud.layout.placements["minimap"];
    assert_eq!((pl.dx, pl.dy), (-40.0, 30.0));
    assert!(!h.ui.hud.layout_unsaved(), "nothing to save");
    let kept = dir.join(file_name(&one.address, "Tester"));
    assert_eq!(h.ui.hud.layout.file.as_deref(), Some(kept.as_path()));
    assert_eq!(
        std::fs::read_to_string(&kept).unwrap(),
        "minimap -40.0 30.0 1.00\n",
        "kept by the world"
    );
    assert_eq!(
        std::fs::read_to_string(&earlier).unwrap(),
        "minimap -40.0 30.0 1.00\n",
        "and left in place"
    );
    // The other world on the host carries it over too; a world that has its own keeps it.
    let two = on_world("retail.dereth.network", 29000);
    assert!(in_folder(&dir, &two)
        .ui
        .hud
        .layout
        .placements
        .contains_key("minimap"));
    let three = on_world("retail.dereth.network", 39000);
    std::fs::write(
        dir.join(file_name(&three.address, "Tester")),
        "chat 5.0 5.0 1.00\n",
    )
    .unwrap();
    let own = in_folder(&dir, &three);
    assert_eq!(
        own.ui.hud.layout.placements.keys().collect::<Vec<_>>(),
        ["chat"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A plugin's HUD with one button, at the middle of the screen.
fn plugin_hud(window: &str) -> dereth_horizon::ui::plugins::PluginFrame {
    use dereth_plugin_api::{UiNode, Value};
    let node = |kind: &str| Value::map().with("kind", kind);
    let tree = UiNode::window_from_value(
        &node("hud")
            .with("id", window)
            .with("anchor", "center")
            .with(
                "children",
                vec![node("button").with("id", "go").with("text", "Go")],
            ),
    )
    .expect("a valid HUD");
    dereth_horizon::ui::plugins::PluginFrame {
        views: vec![dereth_plugin_ui::WindowView {
            tier: dereth_plugin_ui::Tier::Plugin,
            plugin: "test.looter".into(),
            window: window.into(),
            tree: std::sync::Arc::new(tree),
            open: true,
            revision: 1,
            images: Vec::new(),
            size: None,
            by_player: true,
        }],
        ..Default::default()
    }
}

/// A plugin's HUD is an element of the HUD layout: outlined and named in the editor, moved by a
/// drag, kept by Save under its plugin and window, and once the editor is closed its button
/// answers where it was moved to.
#[test]
fn a_plugin_s_hud_is_moved_in_hud_layout_kept_by_save_and_answers_where_it_was_put() {
    let state = in_world();
    let file = layout_file("plugin");
    let mut h = in_world_with(&file, HorizonOptions::default(), &state);
    h.ui.windows.plugins.frame = plugin_hud("my hud");
    h.frame(&state);
    h.frame(&state);
    let name = dereth_horizon::ui::layout::plugin_hud("test.looter", "my hud");
    assert_eq!(name, "plugin:test.looter/my%20hud", "one word for the file");
    assert_eq!(
        dereth_horizon::ui::layout::element_title(&name),
        "test.looter: my hud"
    );
    let outline = |h: &Harness| {
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
            .expect("the plugin's HUD is outlined")
    };
    let before = outline(&h);
    open_layout(&mut h, &state);
    let c = (before.x + before.w / 2.0, before.y + before.h / 2.0);
    drag(&mut h, &state, c, (c.0 + 200.0, c.1 + 100.0));
    {
        let b = button(&h, 0);
        click_at(&mut h, &state, b)
    };
    let kept = std::fs::read_to_string(&file).expect("Save keeps the layout");
    assert_eq!(kept, format!("{name} 200.0 100.0 1.00\n"));
    {
        let b = button(&h, 2);
        click_at(&mut h, &state, b)
    };
    assert!(!h.ui.windows.is_open(WindowId::Layout));
    h.frame(&state);
    let after = outline(&h);
    assert_eq!(
        (after.x - before.x, after.y - before.y),
        (200.0, 100.0),
        "drawn where it was put"
    );
    let go = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|r| after.contains(r.x + 1.0, r.y + 1.0))
        .expect("its button, a stop for the pad where the HUD now is");
    h.move_to(go.x + go.w / 2.0, go.y + go.h / 2.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert_eq!(
        h.ui.windows.plugins.take_inputs(),
        vec![dereth_plugin_ui::Input::Click {
            plugin: "test.looter".into(),
            window: "my hud".into(),
            node: "go".into(),
        }],
        "its button answers where it was put"
    );
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}

/// While the player has the HUDs unlocked, a plugin's HUD is outlined and a press on it moves it,
/// with no editor open: its place is kept in the character's layout file at once, and its button
/// takes no click; locked again, it stands where it was put and its button answers.
#[test]
fn unlocked_a_plugins_hud_is_moved_by_a_drag_and_kept_at_once() {
    let state = in_world();
    let file = layout_file("unlocked");
    let mut h = in_world_with(&file, HorizonOptions::default(), &state);
    let mut frame = plugin_hud("hud");
    frame.huds_unlocked = true;
    h.ui.windows.plugins.frame = frame;
    h.frame(&state);
    h.frame(&state);
    let name = dereth_horizon::ui::layout::plugin_hud("test.looter", "hud");
    let outline = |h: &Harness| {
        h.ui.hud
            .layout
            .outlines
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
            .expect("the plugin's HUD is drawn")
    };
    let before = outline(&h);
    let c = (before.x + before.w / 2.0, before.y + before.h / 2.0);
    drag(&mut h, &state, c, (c.0 - 150.0, c.1 + 60.0));
    assert!(
        h.ui.windows.plugins.take_inputs().is_empty(),
        "unlocked, its button takes no click"
    );
    let kept = std::fs::read_to_string(&file).expect("the place is kept at once");
    assert_eq!(kept, format!("{name} -150.0 60.0 1.00\n"));
    let mut frame = plugin_hud("hud");
    frame.huds_unlocked = false;
    h.ui.windows.plugins.frame = frame;
    h.frame(&state);
    let after = outline(&h);
    assert_eq!(
        (after.x - before.x, after.y - before.y),
        (-150.0, 60.0),
        "drawn where it was put"
    );
    let go = h
        .input
        .nav
        .last
        .targets
        .iter()
        .map(|t| t.rect)
        .find(|r| after.contains(r.x + 1.0, r.y + 1.0))
        .expect("its button");
    h.move_to(go.x + go.w / 2.0, go.y + go.h / 2.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert_eq!(
        h.ui.windows.plugins.take_inputs(),
        vec![dereth_plugin_ui::Input::Click {
            plugin: "test.looter".into(),
            window: "hud".into(),
            node: "go".into(),
        }],
        "locked, its button answers"
    );
    let _ = std::fs::remove_dir_all(file.parent().unwrap());
}
