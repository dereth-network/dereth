//! Settings, driven by the pointer and the keys: each of its pages asks the game for what the
//! other interfaces' pages ask for, from the shared options sheet's rows.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::options::{interface, sheet, store};
use dereth_client_contract::view::PrefValue;
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::options::HorizonOptions;
use dereth_horizon::ui::chat;
use dereth_horizon::ui::game::{
    ChatWindow, GameState, KeyBindingsView, KeyCapture, KeyRequest, KeyRow,
};
use dereth_horizon::ui::input::vk;
use dereth_horizon::ui::panels::WindowId;
use dereth_horizon::ui::Outcome;

use crate::Harness;

fn in_world() -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    }
}

/// The interface with Settings open on `tab`.
fn options_on(tab: &str, state: &GameState) -> Harness {
    let mut h = Harness::new(Default::default());
    h.frame(state);
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(state);
    click(&mut h, state, tab);
    h
}

fn control(h: &Harness, label: &str) -> Rect {
    h.ui.windows
        .options_page
        .control(label)
        .unwrap_or_else(|| panic!("no control {label:?}"))
}

/// Scroll the page's list until `label` is wholly in it.
fn scroll_to(h: &mut Harness, state: &GameState, label: &str) {
    for _ in 0..200 {
        let list = control(h, "list");
        if let Some(r) = h.ui.windows.options_page.control(label) {
            if r.y >= list.y && r.bottom() <= list.bottom() {
                return;
            }
        }
        let page = h.ui.windows.options_page.tab;
        h.ui.windows.options_page.scroll[page] += 20.0;
        h.frame(state);
    }
    panic!("{label:?} never came into view");
}

/// A left click at `(x, y)`, and the frame after it; what both frames asked for.
fn click_at(h: &mut Harness, state: &GameState, x: f32, y: f32) -> Outcome {
    h.move_to(x, y);
    h.press();
    let mut out = h.frame(state);
    h.release();
    let after = h.frame(state);
    out.requests.extend(after.requests);
    out.settings.extend(after.settings);
    out.key_requests.extend(after.key_requests);
    out
}

fn click(h: &mut Harness, state: &GameState, label: &str) -> Outcome {
    let r = control(h, label);
    click_at(h, state, r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Open the dropdown box of the row captioned `label` and click the `n`th choice in its list,
/// which opens under the box, each row as tall as the box; what that asked for.
fn choose(h: &mut Harness, state: &GameState, label: &str, n: usize) -> Outcome {
    let r = control(h, label);
    let box_ = if label == "Interface" {
        // The interface's box leaves room for Apply at its right.
        Rect::new(r.x, r.y, r.w - 88.0, r.h)
    } else {
        r
    };
    click_at(h, state, box_.x + 10.0, box_.y + box_.h / 2.0);
    assert!(
        h.ui.windows.options_page.menu.is_some(),
        "{label}'s list opened"
    );
    #[allow(clippy::cast_precision_loss)]
    let y = box_.bottom() + 5.0 + box_.h * (n as f32 + 0.5);
    let mut out = click_at(h, state, box_.x + 20.0, y);
    assert!(
        h.ui.windows.options_page.menu.is_none(),
        "a choice closes it"
    );
    // The row acts on the choice as it is drawn.
    let after = h.frame(state);
    out.requests.extend(after.requests);
    out.settings.extend(after.settings);
    out
}

/// A key, and the frame that takes it.
fn key(h: &mut Harness, state: &GameState, vk: usize) -> Outcome {
    h.input.keys.push(vk);
    h.frame(state)
}

#[test]
fn a_chat_filter_box_turns_its_group_off_in_that_window_and_defaults_put_every_window_back() {
    let main = sheet::window::MAIN;
    let mut state = in_world();
    state.chat_filters = vec![(main, 0xFFFF_FFFF)];
    let mut h = options_on("Chat", &state);
    let combat = group("Combat");
    let label = format!("Combat@{main:x}");
    scroll_to(&mut h, &state, &label);
    let out = click(&mut h, &state, &label);
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: main,
            mask: 0xFFFF_FFFF & !combat
        }]
    );
    let out = click(&mut h, &state, "Defaults");
    assert_eq!(
        out.requests,
        chat::WINDOWS.map(|window| UiRequest::SetChatWindowFilter {
            window,
            mask: chat::default_filter(window)
        })
    );
}

/// A filter group's mask, by its caption on the pages.
fn group(caption: &str) -> u64 {
    sheet::FILTER_GROUPS
        .iter()
        .find(|(c, _)| *c == caption)
        .map(|(_, m)| *m)
        .unwrap()
}

/// In the world with the game's five chat windows, keeping no filters for them.
fn with_chat_windows() -> GameState {
    let mut state = in_world();
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

/// A right click on the log window's tab named `name`, and the frame after it.
fn right_click_tab(h: &mut Harness, state: &GameState, name: &str) {
    let r =
        h.ui.hud
            .log
            .control(name)
            .unwrap_or_else(|| panic!("no tab {name:?}"));
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.input.down[1] = true;
    h.input.pressed[1] = true;
    h.frame(state);
    h.input.down[1] = false;
    h.input.released[1] = true;
    h.frame(state);
}

/// A click on the open tab menu's control captioned `label`; what it asked for.
fn click_menu(h: &mut Harness, state: &GameState, label: &str) -> Outcome {
    let r =
        h.ui.hud
            .log
            .control(label)
            .unwrap_or_else(|| panic!("no menu control {label:?}"));
    click_at(h, state, r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn a_tab_s_filters_menu_and_the_chat_page_edit_one_filter_for_its_window() {
    let combat_tab = chat::WINDOWS[1];
    let mut state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    right_click_tab(&mut h, &state, "Combat");
    assert!(h.ui.hud.log.filters_open());
    // Until the character keeps a filter, the second tab shows combat and magic; turning magic
    // off there asks the game to keep combat alone.
    let out = click_menu(&mut h, &state, "Magic");
    let kept = group("Combat");
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: combat_tab,
            mask: kept
        }]
    );
    // The game keeps it; the Chat page shows that filter, so its magic box turns magic back on
    // over combat, not over some other window's or an empty one.
    state.chat_filters = vec![(combat_tab, kept)];
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(&state);
    click(&mut h, &state, "Chat");
    let label = format!("Magic@{combat_tab:x}");
    scroll_to(&mut h, &state, &label);
    let out = click(&mut h, &state, &label);
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: combat_tab,
            mask: kept | group("Magic")
        }]
    );
    // And the other way: society turned off for the main window on the page shows off in the
    // main tab's menu, whose box turns it back on.
    let main = chat::WINDOWS[0];
    let label = format!("Society@{main:x}");
    scroll_to(&mut h, &state, &label);
    let out = click(&mut h, &state, &label);
    let without = chat::default_filter(main) & !group("Society");
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: main,
            mask: without
        }]
    );
    state.chat_filters.push((main, without));
    h.ui.windows.close(WindowId::Options);
    h.frame(&state);
    right_click_tab(&mut h, &state, "Main");
    let out = click_menu(&mut h, &state, "Society");
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: main,
            mask: chat::default_filter(main)
        }]
    );
}

#[test]
fn a_tab_renamed_on_the_chat_page_is_kept_in_the_settings_and_named_so_on_the_log_window() {
    let state = with_chat_windows();
    let mut h = options_on("Chat", &state);
    scroll_to(&mut h, &state, "Name 2");
    click(&mut h, &state, "Name 2");
    h.input.chars = vec!['!'];
    let out = h.frame(&state);
    assert_eq!(
        out.settings,
        [("chat-tab-2".to_owned(), "Combat!".to_owned())],
        "the box holds the tab's name, and an edit is a setting to keep"
    );
    let mut options = HorizonOptions::default();
    options.chat_tabs.set(1, "Fights");
    let mut h = Harness::new(options);
    h.frame(&state);
    h.frame(&state);
    assert!(h.ui.hud.log.control("Fights").is_some());
    assert!(h.ui.hud.log.control("Combat").is_none());
    assert!(h.ui.hud.log.control("Main").is_some());
}

#[test]
fn the_client_page_sets_the_shared_preferences_and_tries_a_screen_size_before_keeping_it() {
    store::init();
    store::initialize_display_preferences(&[
        store::DisplayMode {
            width: 800,
            height: 600,
            refresh_rate: 60,
            bits_per_pixel: 32,
        },
        store::DisplayMode {
            width: 1920,
            height: 1080,
            refresh_rate: 60,
            bits_per_pixel: 32,
        },
    ]);
    let state = in_world();
    let mut h = options_on("Client", &state);
    // A check box: the shared store, at once, and the game's preference chain.
    let row = sheet::rows_for(sheet::PageId::Client, interface::Interface::Horizon)
        .find(|r| matches!(r.value, sheet::Value::Check(_)))
        .expect("a check row");
    let sheet::Value::Check(name) = row.value else {
        unreachable!()
    };
    let Some(PrefValue::Bool(was)) = store::inq_value(name) else {
        panic!("{name} is registered")
    };
    scroll_to(&mut h, &state, row.caption);
    let r = control(&h, row.caption);
    let out = click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert_eq!(
        out.requests,
        [UiRequest::SetPreference(name, PrefValue::Bool(!was))]
    );
    assert_eq!(store::inq_value(name), Some(PrefValue::Bool(!was)));
    // The interface choice: chosen from its dropdown's list, and switched to only on Apply,
    // which the shell then follows.
    scroll_to(&mut h, &state, "Interface");
    let out = choose(&mut h, &state, "Interface", 1);
    assert!(out.requests.is_empty(), "{:?}", out.requests);
    let out = click(&mut h, &state, "Apply");
    assert!(
        out.requests.contains(&UiRequest::SetPreference(
            interface::INTERFACE,
            PrefValue::Int(interface::Interface::Classic.value())
        )),
        "{:?}",
        out.requests
    );
    // The screen size is tried, not set: the runtime asks before it keeps it.
    let resolution = sheet::rows_for(sheet::PageId::Client, interface::Interface::Horizon)
        .find(|r| r.confirm_change)
        .expect("the screen size row");
    scroll_to(&mut h, &state, resolution.caption);
    let before = store::inq_value(store::DISPLAY_RESOLUTION);
    // Whichever of the two sizes is not the one in use.
    let mut out = choose(&mut h, &state, resolution.caption, 0);
    if out.requests.is_empty() {
        out = choose(&mut h, &state, resolution.caption, 1);
    }
    assert!(
        out.requests.iter().any(|q| matches!(
            q,
            UiRequest::Resolution(
                dereth_client_contract::resolution::ResolutionAction::Begin { .. }
            )
        )),
        "{:?}",
        out.requests
    );
    assert_eq!(store::inq_value(store::DISPLAY_RESOLUTION), before);
}

fn keys_state() -> GameState {
    let mut state = in_world();
    state.key_bindings = Some(KeyBindingsView {
        rows: vec![KeyRow {
            map: 3,
            action: 0x10,
            group: 0,
            caption: "Walk Forward".into(),
            keys: vec!["W".into()],
            changed: false,
        }],
        capture: None,
        refused: None,
    });
    state
}

#[test]
fn a_key_button_waits_for_a_key_in_its_place_or_beside_the_keys_and_a_right_click_clears_it() {
    let state = keys_state();
    let mut h = options_on("Key Bindings", &state);
    let out = click(&mut h, &state, "Walk Forward#0");
    assert_eq!(
        out.key_requests,
        [KeyRequest::Capture {
            map: 3,
            action: 0x10,
            slot: Some(0)
        }]
    );
    let out = click(&mut h, &state, "Walk Forward#1");
    assert_eq!(
        out.key_requests,
        [KeyRequest::Capture {
            map: 3,
            action: 0x10,
            slot: None
        }]
    );
    let r = control(&h, "Walk Forward#0");
    h.move_to(r.x + r.w / 2.0, r.y + r.h / 2.0);
    h.input.pressed[1] = true;
    let out = h.frame(&state);
    assert_eq!(
        out.key_requests,
        [KeyRequest::Clear {
            map: 3,
            action: 0x10,
            slot: 0
        }]
    );
    let out = click(&mut h, &state, "Restore Defaults");
    assert_eq!(out.key_requests, [KeyRequest::RestoreDefaults]);
}

#[test]
fn while_a_key_is_awaited_enter_and_escape_reach_the_game_and_its_question_is_answered() {
    let mut state = keys_state();
    let mut h = options_on("Key Bindings", &state);
    if let Some(k) = state.key_bindings.as_mut() {
        k.capture = Some(KeyCapture {
            map: 3,
            action: 0x10,
            caption: "Walk Forward".into(),
            question: None,
        });
    }
    let out = key(&mut h, &state, vk::ENTER);
    assert!(out.key_requests.is_empty(), "Enter can be the key bound");
    let out = key(&mut h, &state, vk::ESCAPE);
    assert!(
        out.key_requests.is_empty(),
        "the input manager cancels on Escape"
    );
    assert!(
        h.ui.windows.is_open(WindowId::Options),
        "Escape closes nothing"
    );
    if let Some(k) = state.key_bindings.as_mut() {
        k.capture.as_mut().unwrap().question = Some("Bind it anyway?".into());
    }
    h.frame(&state);
    let out = key(&mut h, &state, vk::ENTER);
    assert_eq!(out.key_requests, [KeyRequest::Answer(true)]);
}

#[test]
fn the_pages_are_character_chat_client_controls_and_key_bindings() {
    let state = in_world();
    let h = options_on("Character", &state);
    for gone in ["Game / Support", "Style"] {
        assert!(h.ui.windows.options_page.control(gone).is_none(), "{gone}");
    }
    for tab in ["Character", "Chat", "Client", "Controls", "Key Bindings"] {
        assert!(h.ui.windows.options_page.control(tab).is_some(), "{tab}");
    }
}

#[test]
fn the_interface_scale_is_one_of_four_chosen_by_its_radio_buttons() {
    store::init();
    let state = in_world();
    let mut h = options_on("Client", &state);
    scroll_to(&mut h, &state, "Interface Scale");
    let r = control(&h, "Interface Scale");
    // Four marks across the row, each at the left of its quarter: the second is 150%.
    let out = click_at(&mut h, &state, r.x + r.w / 4.0 + 9.0, r.y + r.h / 2.0);
    assert_eq!(out.settings, [("scale".to_owned(), "1.50".to_owned())]);
    let out = click_at(&mut h, &state, r.x + r.w * 0.75 + 9.0, r.y + r.h / 2.0);
    assert_eq!(out.settings, [("scale".to_owned(), "3.00".to_owned())]);
}

#[test]
fn smooth_movement_is_this_interface_s_own_box_under_smooth_animation_and_off_until_ticked() {
    store::init();
    let state = in_world();
    let mut h = options_on("Client", &state);
    scroll_to(&mut h, &state, "Smooth Movement");
    let animation = control(&h, "Smooth Animation");
    let r = control(&h, "Smooth Movement");
    assert!(
        r.y > animation.y,
        "the box stands under Smooth Animation: {r:?} against {animation:?}"
    );
    let out = click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert_eq!(
        out.settings,
        [("smooth-movement".to_owned(), "true".to_owned())],
        "ticked, it is this interface's own setting, not a preference"
    );
    assert!(out.requests.is_empty(), "{:?}", out.requests);
}

#[test]
fn smooth_animation_is_this_interface_s_own_box_on_the_client_page_and_off_until_ticked() {
    store::init();
    let state = in_world();
    let mut h = options_on("Client", &state);
    scroll_to(&mut h, &state, "Smooth Animation");
    let r = control(&h, "Smooth Animation");
    let out = click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert_eq!(
        out.settings,
        [("smooth-animation".to_owned(), "true".to_owned())],
        "ticked, it is this interface's own setting, not a preference"
    );
    assert!(out.requests.is_empty(), "{:?}", out.requests);
}

#[test]
fn the_controls_page_sets_the_movement_scheme_and_the_turning_directions() {
    let state = in_world();
    let mut h = options_on("Controls", &state);
    let out = choose(&mut h, &state, "Movement", 0);
    assert_eq!(out.settings, [("movement".to_owned(), "camera".to_owned())]);
    let out = choose(&mut h, &state, "Movement in Gamepad Mode", 1);
    assert_eq!(
        out.settings,
        [("gamepad-movement".to_owned(), "character".to_owned())]
    );
    let r = control(&h, "Reverse Horizontal Turning");
    let out = click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert_eq!(out.settings, [("reverse-x".to_owned(), "true".to_owned())]);
}

#[test]
fn the_controls_page_switches_gamepad_mode_on_and_sets_its_dead_zone_and_camera_speed() {
    let state = in_world();
    let mut h = options_on("Controls", &state);
    let r = control(&h, "Gamepad Mode");
    let out = click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert_eq!(out.settings, [("gamepad".to_owned(), "true".to_owned())]);
    for name in ["Stick Dead Zone", "Camera Speed"] {
        let r = control(&h, name);
        let out = click_at(&mut h, &state, r.x + 4.0, r.y + r.h / 2.0);
        let [(setting, value)] = out.settings.as_slice() else {
            panic!("{name}: {:?}", out.settings);
        };
        assert!(setting.starts_with("gamepad-"), "{name}: {setting}");
        let v: f32 = value.parse().unwrap();
        assert!(v < 0.3, "{name}: the left end is near the least: {v}");
    }
}

#[test]
fn the_controls_page_sets_how_quickly_the_camera_comes_round_slowly_at_the_left_at_once_at_the_right(
) {
    use dereth_client_runtime::orbit::limits;
    let state = in_world();
    let mut h = options_on("Controls", &state);
    let r = control(&h, "Camera Recentre Speed");
    // Its track leaves room at its right for the value shown beside it.
    let mut set_at = |x: f32| {
        let out = click_at(&mut h, &state, x, r.y + r.h / 2.0);
        let [(setting, value)] = out.settings.as_slice() else {
            panic!("{:?}", out.settings);
        };
        assert_eq!(setting, "camera-recentre");
        value.parse::<f32>().unwrap()
    };
    let right = set_at(r.right() - 60.0);
    assert!(
        right.abs() < 1e-6,
        "the right end comes round at once: {right}"
    );
    let left = set_at(r.x - 4.0);
    assert!(
        (left - limits::RECENTRE.1).abs() < 1e-6,
        "the left end is the slowest: {left}"
    );
    let middle = set_at(r.x + (r.w - 64.0) * 0.75);
    assert!(
        middle > 0.2 && middle < 0.3,
        "three quarters along, a quarter of the slowest: {middle}"
    );
    // The page, with the slider among the camera's, is still all inside the window.
    let window =
        h.ui.windows
            .rects(1.0)
            .into_iter()
            .find(|(id, _)| *id == WindowId::Options)
            .map(|(_, w)| w)
            .expect("Settings open");
    let last = control(&h, "Camera Speed");
    assert!(
        last.bottom() <= window.bottom(),
        "the pad's last slider {last:?} in the window {window:?}"
    );
}

#[test]
fn an_open_dropdown_list_takes_the_presses_over_it_and_a_press_elsewhere_only_closes_it() {
    let state = in_world();
    let mut h = options_on("Controls", &state);
    let r = control(&h, "Movement");
    click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert!(h.ui.windows.options_page.menu.is_some());
    // A press on the list, over the check box under it, is the list's: choosing the scheme
    // already in use asks for nothing, and the box under it does not change.
    let reverse = control(&h, "Reverse Horizontal Turning");
    let out = click_at(
        &mut h,
        &state,
        reverse.x + 10.0,
        r.y + r.h + 2.0 + 1.5 * r.h.max(24.0),
    );
    assert!(out.settings.is_empty(), "{:?}", out.settings);
    assert!(
        h.ui.windows.options_page.menu.is_none(),
        "a choice closes it"
    );
    // A press off the list only closes it: the tab under it is not changed.
    click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert!(h.ui.windows.options_page.menu.is_some());
    let chat = control(&h, "Chat");
    click_at(&mut h, &state, chat.x + chat.w / 2.0, chat.y + chat.h / 2.0);
    assert!(h.ui.windows.options_page.menu.is_none());
    assert_eq!(h.ui.windows.options_page.tab, 3, "still on Controls");
    // Escape closes the list, not the window.
    click_at(&mut h, &state, r.x + 10.0, r.y + r.h / 2.0);
    assert!(h.ui.windows.options_page.menu.is_some());
    key(&mut h, &state, vk::ESCAPE);
    assert!(h.ui.windows.options_page.menu.is_none());
    assert!(h.ui.windows.is_open(WindowId::Options));
}

/// The ground a log window draws at `opacity`, where it was drawn this frame at `x, y`.
fn ground_at(h: &Harness, x: f32, y: f32) -> Option<u32> {
    h.list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && (q.dst.x - x).abs() < 0.5 && (q.dst.y - y).abs() < 0.5)
        .map(|q| q.colour)
}

#[test]
fn pop_out_makes_the_tab_a_window_like_the_docked_one_moved_by_dragging_its_tab() {
    let state = with_chat_windows();
    let mut options = HorizonOptions::default();
    options
        .chat_opacity
        .set_by_key("chat-tab-2-opacity-inactive", 0.25);
    let mut h = Harness::new(options);
    h.frame(&state);
    right_click_tab(&mut h, &state, "Combat");
    click_menu(&mut h, &state, "Pop Out");
    assert!(h.ui.hud.log.floating()[1]);
    assert!(!h.ui.hud.log.filters_open(), "the button closes the menu");
    h.move_to(5.0, 5.0);
    h.frame(&state);
    let (x, y) = h.ui.hud.log.popped_at(1).expect("placed");
    let dock = h.ui.hud.log.dock;
    // Its tab on top, its lines under it on the ground the docked window has, at the tab's own
    // opacity popped out.
    let tab = h.ui.hud.log.control("Combat").expect("its tab");
    assert!((tab.x - x).abs() < 0.5 && (tab.y - y).abs() < 0.5);
    assert_eq!(
        ground_at(&h, x, y + tab.h),
        Some(dereth_horizon::draw::with_alpha(0xFF10_1010, 0.25))
    );
    let lines = h
        .list
        .quads
        .iter()
        .find(|q| q.tex.is_none() && (q.dst.x - x).abs() < 0.5 && (q.dst.y - y - tab.h).abs() < 0.5)
        .expect("its ground")
        .dst;
    assert!(
        (lines.w - dock.w).abs() < 0.5,
        "as wide as the docked window"
    );
    // A drag on the tab carries the window.
    h.move_to(tab.x + 10.0, tab.y + 10.0);
    h.press();
    h.frame(&state);
    h.move_to(tab.x + 10.0 + 60.0, tab.y + 10.0 + 40.0);
    h.frame(&state);
    h.release();
    let out = h.frame(&state);
    assert_eq!(h.ui.hud.log.popped_at(1), Some((x + 60.0, y + 40.0)));
    // Where it was let go is kept, in layout units.
    assert_eq!(
        out.settings,
        [(
            "chat-tab-2-at".to_owned(),
            format!("{},{}", x + 60.0, y + 40.0)
        )]
    );
    // Its menu docks it again.
    right_click_tab(&mut h, &state, "Combat");
    click_menu(&mut h, &state, "Dock");
    assert!(!h.ui.hud.log.floating()[1]);
}

/// Pop the tab named `name` out from its menu, and draw a frame with it out.
fn pop_out(h: &mut Harness, state: &GameState, name: &str) {
    right_click_tab(h, state, name);
    click_menu(h, state, "Pop Out");
    h.move_to(1900.0, 5.0);
    h.frame(state);
}

#[test]
fn popped_out_tabs_stand_in_a_column_over_the_docked_window_until_put_elsewhere() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    for name in ["Combat", "Allegiance", "Fellowship", "Global"] {
        pop_out(&mut h, &state, name);
    }
    let dock = h.ui.hud.log.dock;
    let at: Vec<(f32, f32)> = (1..5)
        .map(|slot| h.ui.hud.log.popped_at(slot).expect("out"))
        .collect();
    // Each a window (its tab and its lines) and a gap apart, the fifth tab's just above the
    // docked window's tab bar, the second at the top, held on the screen.
    let step = 24.0 + (dock.h - 26.0) + 8.0;
    for (n, (x, y)) in at.iter().enumerate() {
        assert!((x - dock.x).abs() < 0.5, "at the docked window's left edge");
        #[allow(clippy::cast_precision_loss)]
        let above = (4 - n) as f32;
        let want = (dock.y - 24.0 - above * step).max(0.0);
        assert!((y - want).abs() < 0.5, "tab {} at {y}, not {want}", n + 2);
    }
    assert!(at[0].1 < at[1].1 && at[1].1 < at[2].1 && at[2].1 < at[3].1);
    assert!(
        at[3].1 + step - 8.0 <= dock.y - 24.0 + 0.5,
        "above the docked window"
    );
}

#[test]
fn a_popped_out_tab_comes_back_where_it_was_put_at_any_scale() {
    let state = with_chat_windows();
    let mut options = HorizonOptions {
        scale: Some(2.0),
        ..HorizonOptions::default()
    };
    options.chat_popped[2] = Some((300.0, 150.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Allegiance");
    assert_eq!(h.ui.hud.log.popped_at(2), Some((600.0, 300.0)));
    // Put off the screen, it is held on it.
    let mut options = HorizonOptions::default();
    options.chat_popped[2] = Some((5000.0, -80.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Allegiance");
    let (x, y) = h.ui.hud.log.popped_at(2).unwrap();
    let dock = h.ui.hud.log.dock;
    assert!((x - (1920.0 - dock.w)).abs() < 0.5 && y.abs() < 0.5);
}

#[test]
fn the_main_tab_s_menu_has_no_pop_out_and_a_right_click_shows_the_tab_it_was_on() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    right_click_tab(&mut h, &state, "Main");
    assert!(h.ui.hud.log.filters_open());
    assert!(h.ui.hud.log.control("Pop Out").is_none());
    assert!(h.ui.hud.log.control("Society").is_some());
    right_click_tab(&mut h, &state, "Allegiance");
    assert!(h.ui.hud.log.filters_open());
    assert_eq!(h.ui.hud.log.shown_tab(), 2);
    assert!(h.ui.hud.log.control("Pop Out").is_some());
}

#[test]
fn the_filters_menu_closes_on_any_other_click_or_key_but_not_on_its_own_boxes() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    right_click_tab(&mut h, &state, "Main");
    click_menu(&mut h, &state, "Trade");
    assert!(
        h.ui.hud.log.filters_open(),
        "a box of its own keeps it open"
    );
    // Another tab, which takes the press itself.
    let other = h.ui.hud.log.control("Fellowship").unwrap();
    click_at(
        &mut h,
        &state,
        other.x + other.w / 2.0,
        other.y + other.h / 2.0,
    );
    assert!(!h.ui.hud.log.filters_open());
    assert_eq!(h.ui.hud.log.shown_tab(), 3);
    right_click_tab(&mut h, &state, "Main");
    assert!(h.ui.hud.log.filters_open());
    key(&mut h, &state, 0x57);
    assert!(!h.ui.hud.log.filters_open(), "a key closes it");
    right_click_tab(&mut h, &state, "Main");
    click_at(&mut h, &state, 1500.0, 300.0);
    assert!(!h.ui.hud.log.filters_open(), "so does a click in the world");
}

#[test]
fn the_chat_page_sets_the_docked_window_s_opacities_and_each_tab_s_but_the_main_one_s_undocked() {
    let state = with_chat_windows();
    let mut options = HorizonOptions::default();
    options
        .chat_opacity
        .set_by_key("chat-opacity-inactive", 0.25);
    options.chat_opacity.set_by_key("chat-opacity-active", 0.75);
    let mut h = options_on("Chat", &state);
    for label in [
        "Active Opacity",
        "Inactive Opacity",
        "Undocked Opacity (Active) 2",
        "Undocked Opacity (Inactive) 5",
    ] {
        scroll_to(&mut h, &state, label);
    }
    assert!(h
        .ui
        .windows
        .options_page
        .control("Undocked Opacity (Active) 1")
        .is_none());
    let r = control(&h, "Undocked Opacity (Inactive) 5");
    let out = click_at(&mut h, &state, r.x + 1.0, r.y + r.h / 2.0);
    assert_eq!(out.settings.len(), 1);
    assert_eq!(out.settings[0].0, "chat-tab-5-opacity-inactive");
    assert!(out.settings[0].1.parse::<f32>().unwrap() < 0.05);
    // Defaults puts the names and the opacities back too.
    let out = click(&mut h, &state, "Defaults");
    assert!(out
        .settings
        .contains(&("chat-tab-2".to_owned(), "Combat".to_owned())));
    assert!(out.settings.contains(&(
        "chat-tab-5-opacity-inactive".to_owned(),
        (144.0_f32 / 255.0).to_string()
    )));
    // The docked window is drawn at its own opacities: inactive, and active under the pointer.
    let mut h = Harness::new(options);
    h.frame(&state);
    let dock = h.ui.hud.log.dock;
    h.frame(&state);
    let inactive = ground_at(&h, dock.x, dock.y);
    h.move_to(dock.x + 50.0, dock.y + 50.0);
    h.frame(&state);
    let active = ground_at(&h, dock.x, dock.y);
    use dereth_horizon::draw::with_alpha;
    assert_eq!(inactive, Some(with_alpha(0xFF10_1010, 0.25)));
    assert_eq!(active, Some(with_alpha(0xFF10_1010, 0.75)));
}

#[test]
fn the_filters_menu_takes_a_press_over_a_popped_out_window_under_it() {
    let state = with_chat_windows();
    let mut options = HorizonOptions::default();
    options.chat_popped[2] = Some((500.0, 600.0));
    // Where the Allegiance tab's menu, opened from its popped-out window, shows its trade box.
    let mut h = Harness::new(options.clone());
    h.frame(&state);
    pop_out(&mut h, &state, "Allegiance");
    right_click_tab(&mut h, &state, "Allegiance");
    let trade = h.ui.hud.log.control("Trade").unwrap();
    // The Fellowship tab popped out with its tab right under that box.
    options.chat_popped[3] = Some((trade.x - 10.0, trade.y - 5.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Allegiance");
    pop_out(&mut h, &state, "Fellowship");
    let at = h.ui.hud.log.popped_at(3).unwrap();
    right_click_tab(&mut h, &state, "Allegiance");
    let out = click_menu(&mut h, &state, "Trade");
    assert_eq!(
        out.requests,
        [UiRequest::SetChatWindowFilter {
            window: chat::WINDOWS[2],
            mask: group("Allegiance") | group("Trade")
        }],
        "the box under the pointer took the press"
    );
    assert!(
        out.settings.is_empty(),
        "the window under it was not let go"
    );
    assert_eq!(h.ui.hud.log.popped_at(3), Some(at));
}

#[test]
fn the_hud_layout_s_reset_puts_the_popped_out_tabs_back_in_their_column() {
    let state = with_chat_windows();
    let mut options = HorizonOptions::default();
    options.chat_popped[1] = Some((1300.0, 600.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    assert_eq!(h.ui.hud.log.popped_at(1), Some((1300.0, 600.0)));
    h.ui.windows.open(WindowId::Layout, -1.0);
    h.frame(&state);
    // The HUD Layout window's middle button, Reset, at the foot of its body; then Save, at its
    // left.
    let (x, y) = h.ui.windows.state(WindowId::Layout).pos.expect("placed");
    let out = click_at(&mut h, &state, x + 190.0, y + 171.0);
    assert!(out.settings.is_empty(), "nothing is forgotten before Save");
    assert!(h.ui.hud.layout_unsaved());
    let mut out = click_at(&mut h, &state, x + 70.0, y + 171.0);
    out.settings.extend(h.frame(&state).settings);
    assert_eq!(
        out.settings,
        reset_settings(),
        "every kept place is forgotten"
    );
    // The settings taken, as the interface's runtime takes them.
    for (key, value) in &out.settings {
        let point = dereth_horizon::options::parse_point(value);
        if let Some(slot) = dereth_horizon::options::popped_slot(key) {
            h.ui.options.chat_popped[slot] = point;
        } else {
            h.ui.options.chat_sizes[dereth_horizon::options::size_slot(key).unwrap()] = point;
        }
    }
    h.frame(&state);
    let dock = h.ui.hud.log.dock;
    let (x, _) = h.ui.hud.log.popped_at(1).unwrap();
    assert!((x - dock.x).abs() < 0.5, "back over the docked window");
}

#[test]
fn a_window_over_a_popped_out_chat_window_takes_the_press() {
    let state = with_chat_windows();
    // Where Settings shows its Chat tab.
    let h = options_on("Character", &state);
    let chat_tab = control(&h, "Chat");
    // The Combat tab popped out with its own tab right under Settings' Chat tab.
    let mut options = HorizonOptions::default();
    options.chat_popped[1] = Some((chat_tab.x - 10.0, chat_tab.y - 5.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    let at = h.ui.hud.log.popped_at(1).unwrap();
    h.ui.windows.open(WindowId::Options, -1.0);
    h.frame(&state);
    let out = click(&mut h, &state, "Chat");
    assert_eq!(h.ui.windows.options_page.tab, 1, "Settings took the press");
    assert!(out.settings.is_empty(), "the chat window was not let go");
    assert_eq!(h.ui.hud.log.popped_at(1), Some(at));
    // Away from Settings, the chat window's tab still takes a drag.
    h.ui.windows.close(WindowId::Options);
    h.frame(&state);
    h.move_to(at.0 + 10.0, at.1 + 10.0);
    h.press();
    h.frame(&state);
    h.move_to(at.0 + 30.0, at.1 + 30.0);
    h.frame(&state);
    h.release();
    h.frame(&state);
    assert_eq!(h.ui.hud.log.popped_at(1), Some((at.0 + 20.0, at.1 + 20.0)));
}

#[test]
fn the_hud_layout_window_over_a_popped_out_chat_window_takes_the_press() {
    let state = with_chat_windows();
    // Where the HUD Layout window shows Reset: the middle of its three buttons.
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Layout, -1.0);
    h.frame(&state);
    let (x, y) = h.ui.windows.state(WindowId::Layout).pos.expect("placed");
    let reset = (x + 190.0, y + 171.0);
    // The Combat tab popped out with its own tab right under Reset.
    let mut options = HorizonOptions::default();
    options.chat_popped[1] = Some((reset.0 - 10.0, reset.1 - 5.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    let at = h.ui.hud.log.popped_at(1).unwrap();
    h.ui.windows.open(WindowId::Layout, -1.0);
    h.frame(&state);
    let out = click_at(&mut h, &state, reset.0, reset.1);
    assert!(out.settings.is_empty(), "the chat window was not let go");
    assert!(h.ui.hud.layout_unsaved(), "Reset took the press");
    // Not dragged: Reset shows it back in its column over the docked window.
    let (x, _) = h.ui.hud.log.popped_at(1).unwrap();
    assert!((x - h.ui.hud.log.dock.x).abs() < 0.5 && (x - at.0).abs() > 0.5);
}

/// What HUD Layout's Reset forgets of the log windows: every popped-out tab's place and every
/// log window's size.
fn reset_settings() -> Vec<(String, String)> {
    (1..5)
        .map(|n| format!("chat-tab-{}-at", n + 1))
        .chain((0..5).map(|n| format!("chat-tab-{}-size", n + 1)))
        .map(|k| (k, String::new()))
        .collect()
}

/// A drag with the left button from `from` to `to`, and the frame after it lets go; what the
/// frame that let go asked for.
fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Outcome {
    h.move_to(from.0, from.1);
    h.press();
    h.frame(state);
    h.move_to(to.0, to.1);
    h.frame(state);
    h.release();
    h.frame(state)
}

#[test]
fn taking_hold_of_a_chat_window_by_its_tab_closes_the_filters_menu() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    right_click_tab(&mut h, &state, "Combat");
    assert!(h.ui.hud.log.filters_open());
    let tab = h.ui.hud.log.control("Combat").unwrap();
    h.move_to(tab.x + 10.0, tab.y + 10.0);
    h.press();
    h.frame(&state);
    assert!(
        !h.ui.hud.log.filters_open(),
        "closed as the press takes hold"
    );
    h.release();
    h.frame(&state);
}

#[test]
fn every_chat_window_is_sized_by_its_bottom_corner_and_keeps_its_size() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    // The docked window, from its default size.
    let dock = h.ui.hud.log.dock;
    let (w, hh) = h.ui.hud.log.window_size(0, 1.0);
    let grip = h.ui.hud.log.control("Resize 1").unwrap();
    let c = (grip.x + grip.w / 2.0, grip.y + grip.h / 2.0);
    let out = drag(&mut h, &state, c, (c.0 + 100.0, c.1 - 60.0));
    assert_eq!(
        out.settings,
        [(
            "chat-tab-1-size".to_owned(),
            format!("{},{}", w + 100.0, hh - 60.0)
        )]
    );
    h.frame(&state);
    let now = h.ui.hud.log.dock;
    assert!((now.w - (dock.w + 100.0)).abs() < 0.5 && (now.x - dock.x).abs() < 0.5);
    assert!((now.y - dock.y).abs() < 0.5, "its top stays where it was");
    // Never smaller than its least.
    let grip = h.ui.hud.log.control("Resize 1").unwrap();
    let out = drag(&mut h, &state, (grip.x + 5.0, grip.y + 5.0), (0.0, 0.0));
    assert_eq!(
        out.settings,
        [("chat-tab-1-size".to_owned(), "200,120".to_owned())]
    );
    // A popped-out tab too, and it comes back at its size, at any scale.
    pop_out(&mut h, &state, "Combat");
    let (pw, ph) = h.ui.hud.log.window_size(1, 1.0);
    let grip = h.ui.hud.log.control("Resize 2").unwrap();
    let c = (grip.x + grip.w / 2.0, grip.y + grip.h / 2.0);
    let out = drag(&mut h, &state, c, (c.0 - 50.0, c.1 + 40.0));
    assert_eq!(
        out.settings,
        [(
            "chat-tab-2-size".to_owned(),
            format!("{},{}", pw - 50.0, ph + 40.0)
        )]
    );
    let mut options = HorizonOptions {
        scale: Some(2.0),
        ..HorizonOptions::default()
    };
    options.chat_sizes[1] = Some((300.0, 150.0));
    let mut h = Harness::new(options);
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    assert_eq!(h.ui.hud.log.window_size(1, 2.0), (600.0, 300.0));
    let tab = h.ui.hud.log.control("Combat").unwrap();
    let ground = h
        .list
        .quads
        .iter()
        .find(|q| {
            q.tex.is_none() && (q.dst.x - tab.x).abs() < 0.5 && (q.dst.y - tab.y - 48.0).abs() < 0.5
        })
        .expect("its ground")
        .dst;
    assert!((ground.w - 600.0).abs() < 0.5 && (ground.h - 252.0).abs() < 0.5);
}

#[test]
fn hud_layout_moves_the_log_window_alone_and_each_popped_out_tab_on_its_own() {
    let state = with_chat_windows();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    pop_out(&mut h, &state, "Combat");
    let at = h.ui.hud.log.popped_at(1).unwrap();
    // The log window moved in the HUD layout: the popped-out tab in its own place stays.
    h.ui.hud.layout.placements.insert(
        "chat".to_owned(),
        dereth_horizon::ui::layout::Placement {
            dx: 300.0,
            dy: -100.0,
            scale: 1.0,
        },
    );
    h.frame(&state);
    h.frame(&state);
    assert_eq!(h.ui.hud.log.popped_at(1), Some(at));
    // With HUD Layout open, the popped-out tab is moved from anywhere on it and sized by the
    // wheel; its lines have no pointer meanwhile.
    h.ui.windows.open(WindowId::Layout, -1.0);
    h.frame(&state);
    let mid = (at.0 + 100.0, at.1 + 120.0);
    let out = drag(&mut h, &state, mid, (mid.0 + 50.0, mid.1 + 30.0));
    assert_eq!(h.ui.hud.log.popped_at(1), Some((at.0 + 50.0, at.1 + 30.0)));
    assert!(out.settings.is_empty(), "kept once the layout is saved");
    let (w, hh) = h.ui.hud.log.window_size(1, 1.0);
    h.move_to(mid.0 + 50.0, mid.1 + 30.0);
    h.input.wheel = 1.0;
    let out = h.frame(&state);
    assert!(out.settings.is_empty());
    let (w2, h2) = h.ui.hud.log.window_size(1, 1.0);
    assert!(w2 > w && h2 > hh, "a notch up makes it larger");
    // Save, at the left of the HUD Layout window's foot, keeps its place and its size.
    let (x, y) = h.ui.windows.state(WindowId::Layout).pos.expect("placed");
    let mut out = click_at(&mut h, &state, x + 70.0, y + 171.0);
    out.settings.extend(h.frame(&state).settings);
    assert!(out.settings.contains(&(
        "chat-tab-2-at".to_owned(),
        format!("{},{}", at.0 + 50.0, at.1 + 30.0)
    )));
    assert!(out
        .settings
        .contains(&("chat-tab-2-size".to_owned(), format!("{w2},{h2}"))));
}

/// A desktop build that can create `offered`, drawing with `running`.
fn renderers(
    offered: &[dereth_client_contract::RendererChoice],
    running: dereth_client_contract::RendererChoice,
) -> dereth_client_contract::options::renderer::RendererStatus {
    dereth_client_contract::options::renderer::RendererStatus {
        offered: offered.to_vec(),
        running: Some(running),
        ..Default::default()
    }
}

/// The Client page scrolled to the end, where the renderer choice is.
fn client_page_end(state: &GameState) -> Harness {
    let mut h = options_on("Client", state);
    h.ui.windows.options_page.scroll[2] = 1.0e6;
    h.frame(state);
    h.frame(state);
    h
}

#[test]
fn the_renderer_choice_lists_only_the_renderers_this_build_can_create() {
    use dereth_client_contract::RendererChoice::{D3d12, Vulkan, Wgpu};
    for (offered, names) in [
        (&[Vulkan, Wgpu][..], &["Vulkan", "wgpu"][..]),
        (&[Vulkan, D3d12, Wgpu][..], &["Vulkan", "D3D12", "wgpu"][..]),
    ] {
        store::init();
        let state = GameState {
            renderers: renderers(offered, Vulkan),
            ..in_world()
        };
        let mut h = client_page_end(&state);
        click(&mut h, &state, "Renderer");
        let menu =
            h.ui.windows
                .options_page
                .menu
                .as_ref()
                .expect("the list opened");
        assert_eq!(menu.names, names);
        assert_eq!(menu.at, 0, "the default, with nothing chosen");
    }
}

#[test]
fn choosing_a_renderer_keeps_it_as_the_renderer_preference_for_the_next_start() {
    use dereth_client_contract::options::renderer::{self, RENDERER};
    use dereth_client_contract::RendererChoice::{Vulkan, Wgpu};
    store::init();
    let state = GameState {
        renderers: renderers(&[Vulkan, Wgpu], Vulkan),
        ..in_world()
    };
    let mut h = client_page_end(&state);
    assert!(
        h.ui.windows
            .options_page
            .control("In use now: Vulkan")
            .is_some(),
        "the renderer in use is said"
    );
    let out = choose(&mut h, &state, "Renderer", 1);
    assert!(
        out.requests
            .contains(&UiRequest::SetPreference(RENDERER, PrefValue::Int(3))),
        "{:?}",
        out.requests
    );
    assert_eq!(renderer::stored(), Some(Wgpu));
    h.frame(&state);
    assert!(
        h.ui.windows
            .options_page
            .control("Renderer: wgpu at the next restart (now Vulkan)")
            .is_some(),
        "the page says the choice waits for the restart: {:?}",
        h.ui.windows.options_page.controls
    );
    // The save at exit writes it where the next start reads `Renderer=`.
    let saved = store::save().to_text();
    assert!(
        saved.contains("[Render]") && saved.contains("Renderer=wgpu"),
        "{saved}"
    );
    // Choosing the one in use again leaves nothing waiting.
    choose(&mut h, &state, "Renderer", 0);
    h.frame(&state);
    assert!(h
        .ui
        .windows
        .options_page
        .control("In use now: Vulkan")
        .is_some());
}

#[test]
fn a_renderer_named_on_the_command_line_is_said_to_win_over_the_choice() {
    use dereth_client_contract::RendererChoice::{Vulkan, Wgpu};
    store::init();
    let state = GameState {
        renderers: dereth_client_contract::options::renderer::RendererStatus {
            command_line: Some(Vulkan),
            preference: Some(Wgpu),
            ..renderers(&[Vulkan, Wgpu], Vulkan)
        },
        ..in_world()
    };
    let h = client_page_end(&state);
    assert!(
        h.ui.windows
            .options_page
            .control(
                "Renderer: wgpu at a restart without --renderer (now Vulkan, from the command line)"
            )
            .is_some(),
        "{:?}",
        h.ui.windows.options_page.controls
    );
}

#[test]
fn with_no_renderer_to_choose_as_in_the_browser_the_page_has_no_renderer_choice() {
    store::init();
    let state = in_world();
    assert!(!state.renderers.shown());
    let h = client_page_end(&state);
    assert!(h.ui.windows.options_page.control("Renderer").is_none());
    assert!(!h
        .ui
        .windows
        .options_page
        .controls
        .iter()
        .any(|(l, _)| l.starts_with("In use now") || l.starts_with("Renderer:")));
}

/// The renderer choice's open list is drawn over every row of the page: after them all, and not
/// cut to the page's list.
#[test]
fn the_renderer_choice_s_open_list_is_drawn_over_every_row_of_the_page() {
    use dereth_client_contract::RendererChoice::{D3d12, Vulkan, Wgpu};
    store::init();
    let state = GameState {
        renderers: renderers(&[Vulkan, D3d12, Wgpu], Vulkan),
        ..in_world()
    };
    let mut h = client_page_end(&state);
    let list = control(&h, "list");
    let anchor = control(&h, "Renderer");
    click(&mut h, &state, "Renderer");
    assert!(h.ui.windows.options_page.menu.is_some(), "the list opened");
    // Its ground, as wide as the box, just under it (or just over it).
    let at = h
        .list
        .quads
        .iter()
        .position(|q| {
            q.tex.is_none()
                && (q.dst.x - anchor.x).abs() < 0.5
                && (q.dst.w - anchor.w).abs() < 0.5
                && ((q.dst.y - (anchor.bottom() + 2.0)).abs() < 0.5
                    || (q.dst.bottom() - (anchor.y - 2.0)).abs() < 0.5)
        })
        .expect("the list's ground");
    h.assert_over_rows(at, list, "the open list");
}
