//! The game's own panels a plugin hides or moves, as Horizon has them: the HUD elements and the
//! windows that do what each panel does, hidden or put elsewhere while the plugin asks, and back
//! where the player left them once it lets go, nothing the player keeps changed.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use dereth_horizon::draw::Rect;
use dereth_horizon::retail_ui::{follow, Ask};
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

fn ask(panel: &str, hidden: Option<bool>, at: Option<(f32, f32)>) -> Ask {
    Ask {
        panel: panel.into(),
        hidden,
        at,
    }
}

fn outline(h: &Harness, name: &str) -> Option<Rect> {
    h.ui.hud
        .layout
        .outlines
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| *r)
}

fn window(h: &Harness, id: WindowId) -> Option<Rect> {
    h.ui.windows
        .rects(1.0)
        .into_iter()
        .find(|(w, _)| *w == id)
        .map(|(_, r)| r)
}

/// The chat hidden is not drawn; the radar moved stands at the place asked for, its top-left
/// there; let go, each is back where it was, and the player's own layout is untouched.
#[test]
fn a_plugin_hides_the_chat_and_moves_the_radar_and_letting_go_puts_both_back() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    assert!(outline(&h, "chat").is_some(), "the log window is up");
    let minimap = outline(&h, "minimap").expect("the minimap");
    follow(
        &mut h.ui,
        &[
            ask("chat", Some(true), None),
            ask("radar", None, Some((100.0, 200.0))),
        ],
    );
    h.frame(&state);
    h.frame(&state);
    assert_eq!(outline(&h, "chat"), None, "the log window is hidden");
    let moved = outline(&h, "minimap").expect("the minimap");
    assert_eq!(
        (moved.x, moved.y),
        (100.0, 200.0),
        "where it was asked to be"
    );
    assert_eq!(
        (moved.w, moved.h),
        (minimap.w, minimap.h),
        "at its own size"
    );
    follow(&mut h.ui, &[]);
    h.frame(&state);
    h.frame(&state);
    assert!(outline(&h, "chat").is_some(), "the log window is back");
    assert_eq!(outline(&h, "minimap"), Some(minimap), "and the minimap");
    assert!(h.ui.hud.layout.placements.is_empty(), "the player's layout");
}

/// The panels hidden take every one of their windows away, open still; the examination window
/// moved stands at the place asked for; let go, each window is back where the player left it.
#[test]
fn a_plugin_hides_the_panels_and_moves_the_examination_window_and_letting_go_puts_them_back() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.ui.windows.open(WindowId::Examine, -1.0);
    h.frame(&state);
    h.frame(&state);
    let inventory = window(&h, WindowId::Inventory).expect("the inventory is open");
    let examine = window(&h, WindowId::Examine).expect("the examination window is open");
    follow(
        &mut h.ui,
        &[
            ask("panels", Some(true), None),
            ask("examination", None, Some((300.0, 100.0))),
        ],
    );
    h.frame(&state);
    h.frame(&state);
    assert_eq!(
        window(&h, WindowId::Inventory),
        None,
        "the inventory is hidden"
    );
    assert!(h.ui.windows.is_open(WindowId::Inventory), "and open still");
    let moved = window(&h, WindowId::Examine).expect("the examination window");
    assert_eq!((moved.x, moved.y), (300.0, 100.0));
    let drawn = h
        .input
        .nav
        .last
        .layers
        .iter()
        .find(|l| l.name == "Examine")
        .map(|l| l.rect)
        .expect("drawn, a panel for the pad's focus");
    assert_eq!((drawn.x, drawn.y), (300.0, 100.0), "and drawn there");
    assert!(
        !h.input
            .nav
            .last
            .layers
            .iter()
            .any(|l| l.name == "Inventory"),
        "the hidden inventory is drawn nowhere"
    );
    follow(&mut h.ui, &[]);
    h.frame(&state);
    h.frame(&state);
    assert_eq!(window(&h, WindowId::Inventory), Some(inventory));
    assert_eq!(
        window(&h, WindowId::Examine),
        Some(examine),
        "where the player left it"
    );
}

/// The panels this interface has nothing like are none of its own.
#[test]
fn the_side_vitals_and_the_power_bar_have_no_counterpart_here() {
    use dereth_horizon::retail_ui::counterpart;
    assert_eq!(counterpart("side_vitals"), None);
    assert_eq!(counterpart("power_bar"), None);
    for name in [
        "chat",
        "vitals",
        "toolbar",
        "radar",
        "panels",
        "environment",
        "combat",
        "examination",
    ] {
        assert!(counterpart(name).is_some(), "{name}");
    }
}
