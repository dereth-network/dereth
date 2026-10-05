//! The resolution drop-down lists the adapter's display modes (filtered, deduplicated, sorted by
//! area, with a 1024x768 fallback), a choice is written as the stored display resolution, and
//! the presentation resizes in the frame that produced the write, before the confirmation that
//! keeps it.
//!
//! Fixture: the shipped options layout on the retail dats, filled from a made-up adapter mode
//! list, for the control; and a headless App on a software device for the write and the resize,
//! asserted on the computed rectangle, the store and the UI layout. The list's own derivation is
//! the cpu tier's `presentation::display_mode_list`. Nothing here touches a real monitor, sends a
//! datagram or opens a socket; a machine without the retail dats fails.
//!
//! # What the original client does
//!
//! ## Where the list comes from
//!
//! The list is not stored in the options layout or supplied by an enum-choice table. During
//! graphics-device initialization, the original client reads the modes reported by D3D9's
//! `EnumAdapterModes`. It accepts modes at least 800 pixels wide and 600 pixels high with at
//! least 32 bits per pixel. The loop includes one synthetic final entry, so 1024x768 at refresh
//! rate zero is always considered even when the adapter reports no modes.
//!
//! Each accepted row stores its area as `width * height` and its mode descriptor as
//! `width << 16 | height`. Rows are deduplicated by width and height, while the refresh rate is
//! recorded even when a duplicate mode contributes no row. The rows are sorted by area, not by
//! width. Their labels use `"%ix%i"`, their values are the mode descriptors, and 1024x768 is the
//! default. Refresh rate zero is labelled `"Auto"`; other rates use `"%ihz"`. Initialization
//! also defaults the original client's full-screen preference to true.
//!
//! ## Which control, and how it is filled
//!
//! The original options initializer registers `Display.Resolution` as the only menu backed by
//! user-choice values rather than UI enum strings, and marks that menu as requiring confirmation.
//! The user-choice path reads the registered values and labels, inserts each ASCII label as a
//! literal menu entry, and stores its associated mode descriptor in attribute `0x10000025`.
//!
//! ## What the control writes
//!
//! When a row is chosen, the menu reads attribute `0x10000025`, records it as the current value,
//! and applies the preference. The registered variable for `Display.Resolution` is the original
//! display-preference resolution field, so the stored value is `width << 16 | height`.
//!
//! ## When it applies — **live, in the producing frame. Not at restart.**
//!
//! Each frame, after UI processing and before the start-of-frame stage, the original client
//! compares the live resolution, full-screen, refresh-rate, vertical-sync, and antialiasing
//! preferences with a shadow copy. Those values occupy relative offsets 0, 4, 8, 13, and 14 in
//! the preference record. If any changed and the device is initialized, it rebuilds the
//! presentation: decode width and height from the mode descriptor, update the window style and
//! position with `SetWindowLongA` and `SetWindowPos`, reset the rendering device, position the
//! resized window, then broadcast global UI message `0x0E` with data 0 for re-layout.
//!
//! The current implementation preserves that producing-frame timing. Its corresponding re-layout
//! path is `UiSystem::refresh_event`, reached through `UiShell::set_display`.
//!
//! # What is asserted
//!
//! Four pieces: the list (`store::display_choices`, which the menu reads instead of the empty
//! enum-choice strings), the control's user-choice leg in the menu-option builder, the write
//! (`UiRequest::SetPreference("Display.Resolution", …)` reaching the stored preference), and the
//! apply (`App::change_presentation`, with `Config::load_display_preferences` as the start-up
//! load). On a real monitor `client == outer` would be the observable, not the style bits, and
//! neither is reachable from an automated test.

#![cfg(gpu)]

use std::rc::Rc;

use dereth_dat::RetailDatStore;
use dereth_primitives::{AssetSource, DataId};
use dereth_ui::framework::{DidMapperResolver, Screen};
use dereth_ui::msg::Delivery;
use dereth_ui::UiSystem;
use dereth_ui_screens::options::page::PlayerOptionPage;
use dereth_ui_screens::options::store::{self, DisplayMode};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::{PrefValue, UiRequest};

/// The preference under test, spelled once.
const RESOLUTION: &str = "Display.Resolution";

/// A made-up adapter. Every test that needs a list uses this one, so the expected rows are
/// arithmetic rather than whatever this machine happens to report.
///
/// It carries, deliberately: one mode below each of the two extent floors, one 16-bit mode at a
/// legal size, one mode **on** both floors (800x600, which `jb` keeps), two refresh rates for the
/// same 1920x1080, and 1024x768 **absent** so the synthetic fallback's append is visible.
fn made_up_adapter() -> Vec<DisplayMode> {
    let m = |width, height, refresh_rate, bits_per_pixel| DisplayMode {
        width,
        height,
        refresh_rate,
        bits_per_pixel,
    };
    vec![
        m(1920, 1080, 60, 32),
        m(640, 480, 60, 32), // below the 800-pixel width floor
        m(1280, 720, 60, 32),
        m(1920, 1080, 144, 32), // a duplicate (w, h) with a second refresh rate
        m(800, 600, 60, 32),    // exactly on both inclusive size floors
        m(1280, 1024, 75, 16),  // below the 32-bit color-depth floor
        m(2560, 1440, 60, 32),
        m(1024, 480, 60, 32), // below the 600-pixel height floor
    ]
}

// =============================================================================================
// 1. The shipped options page, driven the way a player drives it
// =============================================================================================

/// The shipped UI over the retail dats, with the made-up adapter enumerated before the options
/// controls are initialized, matching the original initialization order.
fn ui_env() -> UiSystem {
    let dir = dereth_dat::testing::dat_dir();
    let store_ = RetailDatStore::open_dir(&dir).expect("the retail dats open");
    let master_id = DataId(0x3900_0001);
    let bytes = store_.read(master_id).expect("MasterProperty");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("MasterProperty decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    let store_ = Rc::new(store_);
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store_.as_ref()).expect("the DidMapper loads"));
    dereth_ui_screens::env::install(&mut ui, store_, resolver);
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    store::initialize_display_preferences(&made_up_adapter());
    ui
}

/// Drain the UI outbox into the active screen until message delivery settles.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..8 {
        let out = ui.drain_outbox();
        if out.is_empty() {
            break;
        }
        for d in out {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

fn index_of(p: &PlayerOptionPage, pref: &str) -> usize {
    p.options
        .iter()
        .position(|o| o.preference == pref)
        .unwrap_or_else(|| panic!("{pref}"))
}

/// Show every ancestor of an element and scroll its list-box row into view — the options page is
/// one of sixteen stacked pages and its rows live in a clipping list box.
fn reveal(ui: &mut UiSystem, s: &mut GamePlayScreen, h: dereth_ui::ElemHandle) {
    let mut a = h;
    loop {
        ui.set_visible(a, true);
        match ui.parent(a) {
            Some(p) => a = p,
            None => break,
        }
    }
    if let Some(list) = s.config_page.option_box.as_mut() {
        let mut a = h;
        let idx = loop {
            if let Some(i) = list.index_of(a) {
                break Some(i);
            }
            match ui.parent(a) {
                Some(p) => a = p,
                None => break None,
            }
        };
        if let Some(i) = idx {
            list.scroll_to_view(ui, i);
        }
    }
    ui.drain_outbox();
}

/// A real `mouse_down`/`mouse_up` at the centre of an element, asserted to land on it first.
fn press(ui: &mut UiSystem, s: &mut GamePlayScreen, h: dereth_ui::ElemHandle, what: &str) {
    let b = ui.screen_box(h);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(cx, cy),
        Some(h),
        "the press does not land on {what}"
    );
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    pump(ui, s);
}

/// **The drop-down really has the adapter's modes in it.**
///
/// The one test that goes red on the list itself: built through the enum-choice path, which is
/// empty for this preference, the menu's zero-row guard would mean the control **could not be
/// opened at all**.
#[test]
fn the_shipped_resolution_dropdown_holds_a_row_per_enumerated_mode() {
    let mut ui = ui_env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let i = index_of(&s.config_page, RESOLUTION);
    let menu = s.config_page.options[i].element;

    let n = dereth_ui::widgets::menu::num_items(&ui, menu);
    assert_eq!(
        usize::try_from(n).unwrap_or(0),
        store::display_choices(RESOLUTION).len(),
        "the resolution drop-down holds {n} row(s); user-choice registration inserts one row per \
         enumerated mode"
    );
    assert!(n > 0, "a zero-row drop-down cannot be opened");

    // The user-choice path writes two values per row: the literal caption and the mode descriptor
    // in attribute `0x10000025`. Check both against the device's registered choice list.
    for (k, c) in store::display_choices(RESOLUTION).iter().enumerate() {
        let item = dereth_ui::widgets::menu::get_item(&ui, menu, k).expect("a row");
        assert_eq!(
            dereth_ui_screens::bind::attr_int(
                &ui,
                item,
                dereth_ui_screens::options::page::ATTR_MENU_VALUE
            ),
            Some(c.value),
            "row {k} carries packed mode descriptor {:#010x}",
            c.value
        );
    }
    store::clear_display_choices();
}

/// **Picking a row writes the preference.** Two real presses first open the closed control and
/// then select a row. The row-selection message records its value and applies the preference with
/// confirmation enabled.
#[test]
fn choosing_a_row_writes_display_resolution() {
    let mut ui = ui_env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let i = index_of(&s.config_page, RESOLUTION);
    let menu = s.config_page.options[i].element;
    reveal(&mut ui, &mut s, menu);
    press(&mut ui, &mut s, menu, "the resolution drop-down");
    let popup = dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("the drop-down's popup");
    assert!(
        ui.node(popup).expect("live").region.flags.visible,
        "registered resolution rows must make the drop-down open"
    );

    // Row 2 of the made-up adapter's list is 1280x720.
    let item = dereth_ui::widgets::menu::get_item(&ui, menu, 2).expect("row 2");
    ui.requests.clear();
    press(&mut ui, &mut s, item, "the drop-down row");

    let want = store::mode_desc(1280, 720);
    let mut reqs = ui.requests.take();
    reqs.retain(|r| matches!(r, UiRequest::Resolution(_)));
    assert_eq!(
        reqs,
        vec![UiRequest::Resolution(
            dereth_client_contract::resolution::ResolutionAction::Begin {
                size: (1280, 720),
                policy: dereth_client_contract::resolution::ResolutionPolicy::Modern,
                persist: false,
            }
        )],
        "row selection stores the packed mode descriptor 1280 << 16 | 720"
    );
    // A registered choice list converts the numeric value back to its label. The profile therefore
    // stores a legible label, and the same choice table parses it when preferences are loaded.
    assert_eq!(
        store::convert_to_string(RESOLUTION, &PrefValue::Int(want)),
        "1280x720"
    );
    store::clear_display_choices();
}

/// A normal menu still applies directly and never enters the confirmation state. Resolution is
/// the only row that the original options initializer marks for confirmation.
#[test]
fn a_nonconfirming_menu_never_opens_the_resolution_dialog() {
    let mut ui = ui_env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let i = index_of(&s.config_page, "UI.ChatFontSize");
    assert!(!s.config_page.options[i].confirm_change);
    let menu = s.config_page.options[i].element;
    reveal(&mut ui, &mut s, menu);
    press(&mut ui, &mut s, menu, "the chat-font-size drop-down");
    let item = dereth_ui::widgets::menu::get_item(&ui, menu, 4).expect("the XL row");
    press(&mut ui, &mut s, item, "the XL row");
    assert!(s.config_page.confirmation_tick(&mut ui).is_empty());
    assert!(s.config_page.confirmation_tick(&mut ui).is_empty());
    assert!(
        ui.dialogs.non_queued().is_empty(),
        "ordinary menus do not ask for confirmation"
    );
    assert_eq!(store::inq_value("UI.ChatFontSize"), Some(PrefValue::Int(4)));
    store::clear_display_choices();
}

// =============================================================================================
// 2. The apply — a real `App` on headless WARP
// =============================================================================================

mod wired {
    use std::path::PathBuf;

    use dereth_client::app::App;
    use dereth_desktop::pump::Pump;
    use dereth_ui::{ElemHandle, ElementId, UiSystem};
    use dereth_ui_screens::options::store;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::{PrefValue, UiRequest};
    use winit::event::MouseButton;
    use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};

    use super::RESOLUTION;

    fn client_dir() -> PathBuf {
        dereth_dat::testing::dat_dir()
    }

    fn app() -> App {
        assert!(
            dereth_dat::testing::have_dats(),
            "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
            client_dir().display()
        );
        let cfg = Config {
            ui: true,
            headless: true,
            sound: false,
            connect: false,
            dat_dir: client_dir(),
            preferences_file: std::env::temp_dir()
                .join("dereth-resolution-dropdown-not-created/prefs.ini"),
            ..Config::default()
        };
        let mut a = App::new(cfg).expect("an application");
        a.start_shell().expect("the shell starts");
        a.ui_mut().expect("the UI shell is up").ui.requests.clear();
        a
    }

    /// Stabilize a queued screen transition before a test exercises its subject.
    fn settle(a: &mut App) {
        a.frame();
        a.frame();
    }

    fn with_screen<R>(a: &mut App, f: impl FnOnce(&mut UiSystem, &mut GamePlayScreen) -> R) -> R {
        let shell = a.ui_mut().expect("a UI shell");
        let screen = shell.flow.current_mut().expect("a current screen");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen is current");
        f(&mut shell.ui, gameplay)
    }

    fn open_config_tab(a: &mut App, pump: &mut Pump, stamp: &mut u32) {
        let (panel_id, container, page) = with_screen(a, |ui, s| {
            let root = s.root().expect("the screen root");
            let page = ui
                .get_child_recursive(
                    root,
                    dereth_ui_screens::options::config::CONFIG_PAGE_ELEMENT,
                )
                .expect("the Client Options page");
            let info = s
                .panels
                .pages
                .iter()
                .copied()
                .find(|candidate| {
                    let mut h = Some(page);
                    while let Some(current) = h {
                        if current == candidate.handle {
                            return true;
                        }
                        h = ui.parent(current);
                    }
                    false
                })
                .expect("the page sits in a toolbar panel");
            (info.panel_id, info.handle, page)
        });
        let toolbar = with_screen(a, |_, s| {
            s.toolbar
                .buttons
                .iter()
                .find(|button| button.panel_id == panel_id)
                .expect("the Client Options toolbar button")
                .handle
        });
        press(a, pump, stamp, toolbar, "the Client Options toolbar button");
        let tab = with_screen(a, |ui, _| {
            let id = ui
                .node(container)
                .and_then(|node| {
                    node.behaviour
                        .as_ref()?
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::panel::Panel>()
                })
                .and_then(|panel| {
                    panel.page_to_tab.get(&ElementId(
                        dereth_ui_screens::options::config::CONFIG_PAGE_ELEMENT.0,
                    ))
                })
                .copied()
                .expect("the Client Options tab");
            ui.get_child_recursive(container, id)
                .expect("the tab element")
        });
        press(a, pump, stamp, tab, "the Client Options tab");
        assert!(
            a.ui().expect("shell").ui.is_visible(page),
            "the Client Options tab is up"
        );
    }

    fn reveal_resolution(a: &mut App) -> ElemHandle {
        with_screen(a, |ui, s| {
            let i = s
                .config_page
                .options
                .iter()
                .position(|o| o.preference == RESOLUTION)
                .expect("the resolution row");
            let h = s.config_page.options[i].element;
            let row = s.config_page.options[i].row;
            if let Some(list) = s.config_page.option_box.as_mut() {
                let k = list
                    .items
                    .iter()
                    .position(|&candidate| candidate == row)
                    .expect("row");
                list.scroll_to_view(ui, k);
            }
            h
        })
    }

    fn press(a: &mut App, pump: &mut Pump, stamp: &mut u32, h: ElemHandle, what: &str) {
        let b = a.ui().expect("shell").ui.screen_box(h);
        let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            a.ui().expect("shell").ui.hit_test_screen(x, y),
            Some(h),
            "the pointer does not land on {what} at {b:?}"
        );
        *stamp += 100;
        let message = pump.mouse_move_message(f64::from(x), f64::from(y), *stamp);
        pump.dispatch(message);
        a.input_manager_mut().expect("input").on_message(message);
        for (offset, down) in [(10, true), (20, false)] {
            let message = pump
                .mouse_button_message(MouseButton::Left, down, *stamp + offset)
                .expect("the left button message");
            pump.dispatch(message);
            a.input_manager_mut().expect("input").on_message(message);
        }
        assert!(a.frame());
        assert!(a.frame());
    }

    /// Behaviour: presentation.resolution.a-choice-in-the-dropdown-resizes-at-once-and-a-yes-keeps-it
    ///
    /// The actual menu writes first; only the second later global tick opens the
    /// non-queued confirmation dialog. A physical Yes keeps the already-applied extent.
    #[test]
    fn physical_resolution_change_applies_before_the_delayed_confirmation_and_yes_keeps_it() {
        let mut a = app();
        a.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        for _ in 0..4 {
            assert!(a.frame());
        }
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        let mut stamp = 100_000;

        open_config_tab(&mut a, &mut pump, &mut stamp);
        let saved_resolution = store::inq_value(RESOLUTION).unwrap();
        with_screen(&mut a, |ui, screen| {
            let i = super::index_of(&screen.config_page, "Render.ScreenBrightness");
            assert_eq!(screen.config_page.options[i].saved, PrefValue::Float(0.0));
            screen.config_page.options[i].current = PrefValue::Float(0.4);
            screen.config_page.apply(&mut ui.requests, i);
        });
        assert!(a.frame());
        let menu = reveal_resolution(&mut a);
        press(&mut a, &mut pump, &mut stamp, menu, "the resolution menu");
        let (row, selected_size) = {
            let ui = &a.ui().expect("shell").ui;
            let row =
                dereth_ui::widgets::menu::get_item(ui, menu, 3).expect("a different mode row");
            let mode = dereth_ui_screens::bind::attr_int(
                ui,
                row,
                dereth_ui_screens::options::page::ATTR_MENU_VALUE,
            )
            .expect("the row's native mode descriptor");
            (row, store::mode_size(mode))
        };
        assert_ne!(
            selected_size,
            (800, 600),
            "the selected row must visibly resize the fixture"
        );
        let selected_ui_size = (
            i32::try_from(selected_size.0).expect("mode width fits UI coordinates"),
            i32::try_from(selected_size.1).expect("mode height fits UI coordinates"),
        );
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            row,
            "the selected resolution row",
        );

        assert_eq!(
            a.renderer().size(),
            selected_size,
            "Apply(true) precedes confirmation"
        );
        assert_eq!(a.ui().expect("shell").ui.display(), selected_ui_size);
        assert!(
            a.ui().expect("shell").ui.dialogs.non_queued().is_empty(),
            "only one later global tick has elapsed"
        );

        assert!(a.frame(), "the second later global tick runs");
        let dialog = a
            .ui()
            .expect("shell")
            .ui
            .dialogs
            .non_queued()
            .first()
            .and_then(|info| info.element)
            .expect("the resolution confirmation is visible after two ticks");
        let prompt_element = a
            .ui()
            .expect("shell")
            .ui
            .get_child_recursive(dialog, dereth_ui::dialog::base::child::TEXT)
            .expect("the confirmation prompt");
        let prompt = with_screen(&mut a, |ui, _| {
            ui.text_element_mut(prompt_element)
                .map(|text| text.glyphs.inq_text(false))
                .unwrap_or_default()
        });
        let expected_prompt = a
            .ui()
            .expect("shell")
            .ui
            .resolve_string_rendered(
                dereth_ui_screens::options::keybinding::string_table(
                    &a.ui().expect("shell").ui,
                    dereth_ui_screens::options::keybinding::STRING_TABLE_ENUM,
                ),
                dereth_primitives::num::hash::str_hash(b"ID_Option_ConfirmChange"),
                &["10".to_owned()],
            )
            .expect("the shipped prompt resolves with its native `time` variable");
        assert_eq!(
            prompt, expected_prompt,
            "the dialog renders the native localized token"
        );
        assert!(
            prompt.contains("10"),
            "the initial countdown value is visible: {prompt:?}"
        );
        let yes = a
            .ui()
            .expect("shell")
            .ui
            .get_child_recursive(dialog, dereth_ui::dialog::base::child::BUTTON1)
            .expect("the physical Yes button");
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            yes,
            "the confirmation's Yes button",
        );
        assert!(a.ui().expect("shell").ui.dialogs.non_queued().is_empty());
        assert_eq!(
            a.renderer().size(),
            selected_size,
            "Yes keeps the applied resolution"
        );
        let cancel = with_screen(&mut a, |ui, screen| {
            let i = super::index_of(&screen.config_page, "Render.ScreenBrightness");
            assert_eq!(
                screen.config_page.options[i].saved,
                PrefValue::Float(0.0),
                "resolution Yes cannot apply unrelated page drafts"
            );
            assert_eq!(screen.config_page.options[i].current, PrefValue::Float(0.4));
            ui.get_child_recursive(
                screen.config_page.page.unwrap(),
                dereth_ui_screens::options::config::button::CANCEL,
            )
            .unwrap()
        });
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            cancel,
            "Cancel after keeping a tested size",
        );
        assert_eq!(
            store::inq_value("Render.ScreenBrightness"),
            Some(PrefValue::Float(0.0))
        );
        assert_eq!(store::inq_value(RESOLUTION), Some(saved_resolution));
    }

    /// Physical No and the strict ten-second expiry both restore the actual size before the test. The deadline is tested at equality and just after it: the dialog expires only when
    /// `now > expiration`.
    #[test]
    fn no_and_expiry_restore_the_saved_resolution() {
        let mut a = app();
        a.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        for _ in 0..4 {
            assert!(a.frame());
        }
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        let mut stamp = 200_000;
        open_config_tab(&mut a, &mut pump, &mut stamp);
        let saved = a.renderer().size();
        assert_eq!(
            saved,
            (800, 600),
            "the actual screen, distinct from the uncommitted registered default"
        );
        let menu = reveal_resolution(&mut a);
        let row = {
            let ui = &a.ui().expect("shell").ui;
            dereth_ui::widgets::menu::get_item(ui, menu, 3).expect("a different mode row")
        };

        press(&mut a, &mut pump, &mut stamp, menu, "the resolution menu");
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            row,
            "the selected resolution row",
        );
        assert!(a.frame(), "the second delayed tick opens the confirmation");
        let dialog = a
            .ui()
            .expect("shell")
            .ui
            .dialogs
            .non_queued()
            .first()
            .and_then(|info| info.element)
            .expect("the No confirmation");
        let no = a
            .ui()
            .expect("shell")
            .ui
            .get_child_recursive(dialog, dereth_ui::dialog::base::child::BUTTON2)
            .expect("the physical No button");
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            no,
            "the confirmation's No button",
        );
        assert_eq!(
            a.renderer().size(),
            saved,
            "No restores the saved resolution"
        );
        assert!(a.ui().expect("shell").ui.dialogs.non_queued().is_empty());

        // The same physical change, left unanswered this time.
        let menu = reveal_resolution(&mut a);
        press(&mut a, &mut pump, &mut stamp, menu, "the resolution menu");
        let row = {
            let ui = &a.ui().expect("shell").ui;
            dereth_ui::widgets::menu::get_item(ui, menu, 3).expect("the same different mode row")
        };
        press(
            &mut a,
            &mut pump,
            &mut stamp,
            row,
            "the selected resolution row",
        );
        assert!(
            a.frame(),
            "the second delayed tick opens the expiry confirmation"
        );
        let (context, dialog_root, expiration) = {
            let info = a
                .ui()
                .expect("shell")
                .ui
                .dialogs
                .non_queued()
                .first()
                .expect("the expiry confirmation");
            (
                info.context,
                info.element.expect("the expiry dialog element"),
                a.resolution
                    .prompt()
                    .and_then(|p| p.deadline)
                    .expect("runtime deadline"),
            )
        };
        a.clock = Box::new(dereth_client_runtime::platform::clock::FixedStepClock::new(
            expiration - 5.0 - a.timer.cur_time,
        ));
        assert!(a.frame());
        let five_second_prompt = {
            let child = a
                .ui()
                .expect("shell")
                .ui
                .get_child_recursive(dialog_root, dereth_ui::dialog::base::child::TEXT)
                .expect("the countdown prompt");
            with_screen(&mut a, |ui, _| {
                ui.text_element_mut(child)
                    .map(|text| text.glyphs.inq_text(false))
                    .unwrap_or_default()
            })
        };
        let expected_five = a
            .ui()
            .expect("shell")
            .ui
            .resolve_string_rendered(
                dereth_ui_screens::options::keybinding::string_table(
                    &a.ui().expect("shell").ui,
                    dereth_ui_screens::options::keybinding::STRING_TABLE_ENUM,
                ),
                dereth_primitives::num::hash::str_hash(b"ID_Option_ConfirmChange"),
                &["5".to_owned()],
            )
            .expect("the five-second prompt resolves");
        assert_eq!(
            five_second_prompt, expected_five,
            "global ticks refresh native `time`"
        );
        a.clock = Box::new(dereth_client_runtime::platform::clock::FixedStepClock::new(
            expiration - a.timer.cur_time,
        ));
        assert!(a.frame());
        assert!(
            a.ui().expect("shell").ui.dialogs.info(context).is_some(),
            "equality is not expiry"
        );
        a.clock = Box::new(dereth_client_runtime::platform::clock::FixedStepClock::new(
            expiration + 0.001 - a.timer.cur_time,
        ));
        assert!(a.frame());
        assert!(
            a.ui().expect("shell").ui.dialogs.info(context).is_none(),
            "just after expiry closes"
        );
        a.clock = Box::new(dereth_client_runtime::platform::clock::FixedStepClock::new(
            dereth_client_runtime::platform::clock::HEADLESS_STEP,
        ));
        assert!(a.frame(), "the restore request reaches the presentation");
        assert_eq!(
            a.renderer().size(),
            saved,
            "expiry restores the saved resolution"
        );
    }

    /// **Apply timing.** The frame loop advances the UI before preparing the
    /// graphics device. Device preparation checks for a lost device and then compares display
    /// preferences with their shadow copy. Therefore the `Display.Resolution` write produced by
    /// this UI request changes the presentation before the same frame's start-of-frame stage,
    /// rather than one frame later.
    #[test]
    fn the_option_write_resizes_the_presentation_in_its_producing_frame() {
        let mut a = app();
        a.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        settle(&mut a);
        assert_eq!(a.renderer().size(), (800, 600));

        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1280, 720)),
            ));
        assert!(a.frame(), "the producing frame runs");

        assert_eq!(
            a.config().display.resolution,
            0x0500_02D0,
            "the UI write was consumed"
        );
        assert_eq!(
            a.renderer().size(),
            (1280, 720),
            "graphics-device preparation applies the new presentation before the start-of-frame stage"
        );
        assert_eq!(
            a.ui().expect("a shell").ui.display(),
            (1280, 720),
            "the same presentation change re-lays out the UI"
        );

        assert!(a.frame(), "the unchanged next frame runs");
        assert_eq!(a.renderer().size(), (1280, 720));
        assert_eq!(a.ui().expect("a shell").ui.display(), (1280, 720));
    }

    /// **End to end.** The options-page write reaches the stored display
    /// resolution, the same frame's preference-shadow comparison notices the change, and the
    /// presentation update resizes the renderer and re-lays out the UI.
    ///
    /// Asserted on the swap chain's own extent and on the UI display extent updated by global
    /// message `0x0E` with data 0 — never on a monitor.
    #[test]
    fn the_option_write_resizes_the_presentation() {
        let mut a = app();
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "the renderer starts at the configured 800x600 extent"
        );
        assert_eq!(a.ui().expect("a shell").ui.display(), (800, 600));
        // These two lines are not scaffolding. In the client, a forced-resolution flag overrides the preference-derived width and height.
        // Client initialization forces 800x600 before reading preferences; entering the gameplay
        // UI clears that force. The resolution menu therefore applies from gameplay, the screen
        // from which Client Options can be opened. Queueing gameplay models that transition here.
        a.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        settle(&mut a);

        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1280, 720)),
            ));
        settle(&mut a);

        assert_eq!(
            a.config().display.resolution,
            0x0500_02D0,
            "the preference store retains the selected 1280x720 packed descriptor"
        );
        assert_eq!(
            a.renderer().size(),
            (1280, 720),
            "restarting the rendering system resets the presentation to the stored display width \
             and height"
        );
        assert_eq!(
            a.ui().expect("a shell").ui.display(),
            (1280, 720),
            "global UI message 0x0E with data 0 triggers re-layout"
        );

        // …and back, because a one-way change cannot tell a real apply from a seed.
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1024, 768)),
            ));
        settle(&mut a);
        assert_eq!(a.renderer().size(), (1024, 768));
        assert_eq!(a.ui().expect("a shell").ui.display(), (1024, 768));
    }

    /// The root element is re-laid out at the new extent, not merely told about it —
    /// `UiSystem::refresh_event`'s `resize_to(root, w, h)`. Without this the screens keep their
    /// 800x600 boxes on a 1280x720 back buffer.
    #[test]
    fn the_ui_root_is_relaid_out_at_the_new_extent() {
        let mut a = app();
        // These two lines are not scaffolding. The client begins with a forced 800x600 resolution that overrides preference-derived dimensions.
        // Entering gameplay clears the force, allowing a resolution selected through Client
        // Options to resize both the presentation and the UI root.
        a.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        settle(&mut a);
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(1280, 1024)),
            ));
        settle(&mut a);
        let shell = a.ui().expect("a shell");
        let root = shell.ui.root();
        let b = shell.ui.screen_box(root);
        assert_eq!(
            (b.x1 - b.x0 + 1, b.y1 - b.y0 + 1),
            (1280, 1024),
            "the root's rectangle follows the presentation"
        );
    }

    /// The negative control preserves the original client's refusal: decoding display preferences
    /// returns false below 800x600, so presentation change also returns false without resizing.
    /// The preference is still stored because the write happens before the frame polls it.
    #[test]
    fn a_resolution_below_the_floor_leaves_the_presentation_alone() {
        let mut a = app();
        a.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .emit(UiRequest::SetPreference(
                RESOLUTION,
                PrefValue::Int(store::mode_desc(640, 480)),
            ));
        settle(&mut a);
        assert_eq!(
            a.config().display.resolution,
            0x0280_01E0,
            "the write itself still lands"
        );
        assert_eq!(
            a.renderer().size(),
            (800, 600),
            "a sub-floor resolution must not resize the presentation"
        );
        assert_eq!(a.ui().expect("a shell").ui.display(), (800, 600));
    }

    /// **The drop-down the running client builds is not empty either.** `App::start_shell`
    /// initializes the display choices from the modes reported by the machine. In headless runs
    /// there is no monitor, so the declared `dereth_client_runtime::platform::window::STANDARD_DISPLAY_MODES` table stands
    /// in. Either source yields rows that satisfy the original filter and include the fallback,
    /// never an empty list.
    #[test]
    fn a_running_client_registers_a_non_empty_resolution_list() {
        let a = app();
        let rows = store::display_choices(RESOLUTION);
        assert!(
            !rows.is_empty(),
            "display initialization registered no rows: the drop-down would be empty"
        );
        for c in &rows {
            let (w, h) = store::mode_size(c.value);
            assert!(
                w >= 800 && h >= 600,
                "{}x{h} is below the inclusive 800x600 floor",
                w
            );
            assert_eq!(c.label, format!("{w}x{h}"));
        }
        assert!(
            rows.iter().any(|c| c.value == store::mode_desc(1024, 768)),
            "display initialization must append the 1024x768 fallback"
        );
        let _ = a;
    }

    /// **The start-up half.** Device initialization passes preference-derived presentation
    /// parameters into graphics-engine startup. `Config::load_display_preferences` models that
    /// load.
    ///
    /// What is asserted below
    /// is the preference loader's answer with forced resolution disabled, which is what `Config`
    /// models. The original running client instead forces 800x600 before loading preferences, so
    /// its initial window remains 800x600 regardless of the saved profile. Entering gameplay lifts
    /// that force and lets the saved size reach the presentation; see
    /// `presentation/screens_at_resolution.rs`.
    #[test]
    fn a_saved_resolution_is_the_size_the_client_starts_at() {
        let prefs = Preferences::parse("[Display]\r\nResolution=1280x720\r\n");
        let mut cfg = Config::default();
        assert_eq!(
            (cfg.width, cfg.height),
            (800, 600),
            "configuration begins with 800x600 device dimensions"
        );
        cfg.apply_preferences(&prefs);
        assert_eq!(
            cfg.display.resolution, 0x0500_02D0,
            "`%ix%i` decoded by display_choice"
        );
        assert_eq!(
            (cfg.width, cfg.height),
            (1280, 720),
            "saved display dimensions reach the presentation configuration"
        );

        // A file that names no resolution keeps device initialization's 800x600 — the registered default
        // is 1024x768 and taking it unconditionally would move every default run.
        let mut quiet = Config::default();
        quiet.apply_preferences(&Preferences::parse("[Display]\r\nFullScreen=False\r\n"));
        assert_eq!((quiet.width, quiet.height), (800, 600));
    }
}
