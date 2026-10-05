//! Every action the retail key page lists outside its not-used section does something in the
//! retail interface: walked over the one set of rows both key pages list, each one,
//! pressed and released in the place it belongs, is answered -- a stage of the action handling
//! takes it, one of the gameplay screen's key handlers recognises it, or a window of the
//! interface opens, closes or takes the focus because of it -- rather than expiring unclaimed.
//! Fixture: a headless `App` on the gameplay screen with the retail dats, the default landblock
//! and an object selected; actions are injected through the production input manager in their
//! own input maps, a combat key in its combat mode. Nothing opens a socket.

use dereth_client_model::combat::CombatMode;
use dereth_input::{ActionId, InputEvent, InputMapId, ToggleType};
use dereth_primitives::ObjectId;
use {dereth_client::app::App, dereth_client_runtime::config::Config};

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.frame();
    }
}

fn in_the_world() -> App {
    // Preferences and action paths use a folder of the test's own. Capture is only routed here.
    let dir = std::env::temp_dir().join(format!("dereth-every-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let mut app = crate::common::sim_app::new(Config {
        preferences_file: dir.join("UserPreferences.ini"),
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("required retail DATs and a simulated presentation");
    app.start_shell().expect("the UI shell");
    let scene = dereth_client_runtime::scene::SceneConfig {
        landblock: dereth_world_data::landblock::DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(scene)
        .expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4);
    app
}

fn event(action: ActionId, map: InputMapId, toggle: ToggleType, start: bool) -> InputEvent {
    InputEvent {
        action,
        input_map: map,
        toggle,
        extent: 1.0,
        start,
        repeat_delta: 0,
        repeat_total: 0,
        from_key_down: start,
    }
}

/// What the interface shows: every element's visibility under the screen, the focus and the
/// screen.
fn shown(app: &App) -> Vec<u64> {
    let Some(shell) = app.ui() else {
        return Vec::new();
    };
    let ui = &shell.ui;
    let mut out = vec![
        u64::from(shell.flow.current_mode().map_or(0, |m| m.0)),
        ui.focus_element().map_or(0, |h| h.index() as u64 + 1),
    ];
    let mut stack = vec![ui.root()];
    while let Some(h) = stack.pop() {
        if let Some(n) = ui.node(h) {
            out.push(((h.index() as u64) << 1) | u64::from(n.region.flags.visible));
            out.push(u64::from(n.state.0));
        }
        stack.extend(ui.children(h));
    }
    out
}

/// How many unclaimed key presses the gameplay screen's own key handlers recognised.
fn keys_answered(app: &App) -> u64 {
    app.ui()
        .and_then(|shell| shell.flow.current())
        .and_then(|s| {
            let any: &dyn std::any::Any = s;
            any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        })
        .map_or(0, |s| s.keys_answered)
}

/// The combat mode an action's input map belongs to.
fn mode_of(map: InputMapId) -> CombatMode {
    match map.0 {
        0x1000_0003 => CombatMode::Melee,
        0x1000_0004 => CombatMode::Missile,
        0x1000_0005 => CombatMode::Magic,
        _ => CombatMode::NonCombat,
    }
}

/// The actions that end the session or the process, pressed last.
const LAST: [&str; 2] = ["LOGOUT", "EXITGAME"];

/// Behaviour: keys.shared.every-bindable-action-does-something-in-the-retail-interface
#[test]
fn every_bindable_action_is_taken_by_some_handler_in_the_retail_interface() {
    let mut app = in_the_world();
    let entries: Vec<(InputMapId, ActionId, ToggleType)> = {
        use dereth_input::presentation::{Interface, ROWS};
        let input = app
            .input_manager_mut()
            .expect("the production input manager");
        let m = &input.manager.action_map;
        ROWS.iter()
            .filter(|r| r.not_used(Interface::Retail).is_none())
            .map(|r| {
                (
                    r.input_map(),
                    r.action(),
                    m.toggle_type(r.input_map(), r.action()),
                )
            })
            .collect()
    };
    assert_eq!(
        entries.len(),
        dereth_input::presentation::ROWS.len() - 5,
        "every row but the five not used here"
    );
    let name = |a: ActionId| dereth_input::names::enum_name_for_action(a);
    let mut order: Vec<(InputMapId, ActionId, ToggleType)> = entries
        .iter()
        .copied()
        .filter(|(_, a, _)| !LAST.contains(&name(*a).as_str()))
        .collect();
    order.extend(
        entries
            .iter()
            .copied()
            .filter(|(_, a, _)| LAST.contains(&name(*a).as_str())),
    );

    let mut unanswered = Vec::new();
    for (map, action, toggle) in order {
        // Something selected, and a selection before it, as a player has most of the time.
        {
            let world = &mut app.probe_mut().objects_mut().world;
            world.selected = Some(ObjectId(0x5000_0001));
            world.prev_selected = Some(ObjectId(0x5000_0002));
            world.combat.combat_mode = mode_of(map);
        }
        let expired = app.actions.stats().expired;
        let answered = keys_answered(&app);
        let before = shown(&app);
        // A one-shot key fires once, as it is pressed; any other is pressed and let go.
        let edges: &[bool] = if toggle == ToggleType::OneShot {
            &[true]
        } else {
            &[true, false]
        };
        for &start in edges {
            app.input_manager_mut()
                .expect("the production input manager")
                .inject_action(event(action, map, toggle, start));
            frames(&mut app, 1);
        }
        frames(&mut app, 2);
        let taken = app.actions.stats().expired == expired || keys_answered(&app) > answered;
        if !taken && shown(&app) == before {
            unanswered.push(format!(
                "{} ({:#010X} in {:#010X})",
                name(action),
                action.0,
                map.0
            ));
        }
    }
    assert!(
        unanswered.is_empty(),
        "{} bindable action(s) are answered by nothing:\n{}",
        unanswered.len(),
        unanswered.join("\n")
    );
}

fn press(app: &mut App, action: u32, start: bool) {
    let toggle =
        if start || action == dereth_client_contract::actions::dereth::MOVEMENT_HOLD_SIDESTEP.0 {
            ToggleType::Hold
        } else {
            ToggleType::OneShot
        };
    app.input_manager_mut()
        .expect("the production input manager")
        .inject_action(event(
            ActionId(action),
            dereth_input::dereth::INPUT_MAP,
            toggle,
            start,
        ));
    frames(app, 2);
}

fn on(name: &str) -> bool {
    matches!(
        dereth_client_contract::options::store::inq_value(name),
        Some(dereth_client_contract::PrefValue::Bool(true))
    )
}

/// Behaviour: keys.shared.the-retail-interface-answers-this-clients-own-keys
#[test]
fn the_retail_interface_answers_this_clients_own_keys_with_its_own_equivalents() {
    use dereth_client_contract::actions::dereth as own;
    let mut app = in_the_world();

    // Hold sidestep is held for as long as its key is.
    press(&mut app, own::MOVEMENT_HOLD_SIDESTEP.0, true);
    assert!(app.movement.lists.hold_sidestep, "held");
    press(&mut app, own::MOVEMENT_HOLD_SIDESTEP.0, false);
    assert!(!app.movement.lists.hold_sidestep, "let go");

    // The shared settings flip, both ways.
    for (action, name) in [
        (
            own::TOGGLE_INVERT_MOUSE_LOOK.0,
            "Input.InvertMouseLookYAxis",
        ),
        (
            own::TOGGLE_MUTE_ON_LOSING_FOCUS.0,
            "Sound.PlaySoundOnlyWhenActive",
        ),
        (
            own::TOGGLE_STRETCH_UI.0,
            dereth_client_contract::options::classic::STRETCH_UI,
        ),
        (
            own::TOGGLE_RIGHT_CLICK_MOUSE_LOOK.0,
            dereth_client_contract::options::classic::RIGHT_CLICK_MOUSE_LOOK,
        ),
    ] {
        let was = on(name);
        press(&mut app, action, true);
        assert_eq!(on(name), !was, "{name} flips");
        press(&mut app, action, true);
        assert_eq!(on(name), was, "{name} flips back");
    }

    // The trade key shows the secure-trade window and hides it again.
    let trade = {
        let shell = app.ui().expect("the shell");
        let root = shell
            .flow
            .current()
            .and_then(|s| s.roots().first().copied())
            .expect("the gameplay root");
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::panels::trade::WINDOW)
            .expect("the trade window")
    };
    let visible = |app: &App| {
        app.ui()
            .and_then(|s| s.ui.node(trade))
            .is_some_and(|n| n.region.flags.visible)
    };
    assert!(!visible(&app));
    press(&mut app, own::TOGGLE_TRADE_PANEL.0, true);
    assert!(visible(&app), "the trade key shows the window");
    press(&mut app, own::TOGGLE_TRADE_PANEL.0, true);
    assert!(!visible(&app), "and hides it");
}
