//! Typed chat gestures and live shell observations shared by the DAT and GPU tests.
//!
//! Behaviour: none (shared fixtures)

use dereth_client::{
    app::App,
    config::Config,
    pump::{Pump, Win32Message},
};
use dereth_client_model::Request;
use dereth_ui::{framework::mode, ElemHandle, ElementId};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// The main-chat text entry.
const CHAT_ENTRY: ElementId = ElementId(0x1000_0016);
/// The speech-bubble list, the only surface whose shipped filter accepts chat type `0x1A`.
const SPEW_LIST: ElementId = dereth_ui_screens::hud::speech_bubbles::LIST_BOX;

pub(crate) fn app_with(make: impl FnOnce(Config) -> App) -> App {
    let mut app = make(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        preferences_file: std::env::temp_dir().join("dereth-chat-commands/preferences.ini"),
        ..Config::default()
    });
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..4 {
        assert!(app.frame());
    }
    app
}

/// The glyph text of every live element in the speech-bubble list, in list order; this does not
/// inspect rendered pixels.
pub(crate) fn bubbles(app: &mut App) -> Vec<String> {
    let list = {
        let shell = app.ui().expect("shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
        shell
            .ui
            .get_child_recursive(screen.root().expect("root"), SPEW_LIST)
            .expect("the spew box list is in the shipped layout")
    };
    let shell = app.ui_mut().expect("shell");
    let kids = shell.ui.children(list);
    kids.into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

/// The main chat window's log glyphs — the visible destination for type-zero local lines.
pub(crate) fn chat_log(app: &mut App) -> String {
    let log = {
        let shell = app.ui().expect("shell");
        let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
        let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
        shell
            .ui
            .get_child_recursive(
                screen.root().expect("root"),
                dereth_ui_screens::chat::window::LOG,
            )
            .expect("the main chat log is in the shipped layout")
    };
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(log)
        .expect("the chat log is a text element")
        .glyphs
        .inq_text(false)
}

pub(crate) fn gameplay_element(app: &App, id: ElementId) -> dereth_ui::ElemHandle {
    let shell = app.ui().expect("shell");
    let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
    let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
    shell
        .ui
        .get_child_recursive(screen.root().expect("root"), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped gameplay tree"))
}

pub(crate) fn screen_rect(app: &App, id: ElementId) -> ((i32, i32), (i32, i32)) {
    let shell = app.ui().expect("shell");
    let h = gameplay_element(app, id);
    let bounds = shell.ui.screen_box(h);
    (shell.ui.screen_origin(h), (bounds.width(), bounds.height()))
}

pub(crate) fn install_player_module(app: &mut App) -> dereth_protocol::login::PlayerModule {
    let module = dereth_protocol::login::PlayerModule::default();
    app.probe_mut().hud_mut().player_module = Some(module.clone());
    app.probe_mut()
        .objects_mut()
        .world
        .player_system
        .apply_player_module(&module);
    module
}

pub(crate) fn forced_saved_player_module(app: &mut App) -> dereth_protocol::login::PlayerModule {
    let mut requests = dereth_client_model::RecordingRequests::default();
    assert!(
        app.probe_mut()
            .objects_mut()
            .world
            .player_system
            .save_to_server(&mut requests, true),
        "the forced save path has an authoritative module"
    );
    let [Request::CharacterOptionsEvent(saved)] = requests.0.as_slice() else {
        panic!("forced save produced {:?}", requests.0)
    };
    saved.module.clone()
}

pub(crate) struct Hand {
    pub(crate) pump: Pump,
    pub(crate) time: u32,
}

impl Hand {
    pub(crate) fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 400_000,
        }
    }

    pub(crate) fn send(&mut self, app: &mut App, message: Win32Message) {
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("the input shell exists in a UI build")
            .on_message(message);
    }

    pub(crate) fn click(&mut self, app: &mut App, id: ElementId) {
        let ((x, y), (width, height)) = screen_rect(app, id);
        self.time += 10;
        let message = self.pump.mouse_move_message(
            f64::from(x + width / 2),
            f64::from(y + height / 2),
            self.time,
        );
        self.send(app, message);
        for down in [true, false] {
            self.time += 10;
            let message = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("the left button is in the 0x201 block");
            self.send(app, message);
        }
        assert!(app.frame());
    }

    /// Click the chat entry, type the line one `WM_CHAR` at a time, then `VK_RETURN`.
    pub(crate) fn submit(&mut self, app: &mut App, text: &str) {
        let entry = {
            let shell = app.ui().expect("shell");
            let any: &dyn std::any::Any = shell.flow.current().expect("a screen");
            let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
            shell
                .ui
                .get_child_recursive(screen.root().expect("root"), CHAT_ENTRY)
                .expect("the chat entry is in the shipped layout")
        };
        self.submit_to(app, entry, text);
    }

    /// The same physical input journey through one particular popup's entry.
    pub(crate) fn submit_to(&mut self, app: &mut App, entry: ElemHandle, text: &str) {
        let bounds = app.ui().expect("shell").ui.screen_box(entry);
        self.time += 10;
        let message = self.pump.mouse_move_message(
            f64::from((bounds.x0 + bounds.x1) / 2),
            f64::from((bounds.y0 + bounds.y1) / 2),
            self.time,
        );
        self.send(app, message);
        for down in [true, false] {
            self.time += 10;
            let message = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("the left button is in the 0x201 block");
            self.send(app, message);
        }
        assert!(app.frame());
        for byte in text.bytes() {
            assert!(
                byte.is_ascii(),
                "this station is deliberately ASCII: {text:?}"
            );
            self.time += 10;
            self.send(
                app,
                Win32Message::new(
                    dereth_input::win32::msg::WM_CHAR,
                    usize::from(byte),
                    0,
                    self.time,
                ),
            );
        }
        assert!(app.frame());
        for (msg, lp) in [
            (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
            (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
        ] {
            self.time += 10;
            self.send(app, Win32Message::new(msg, 13, lp as isize, self.time));
        }
        assert!(app.frame());
    }
}
