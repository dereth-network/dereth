//! Real host messages reach the active interface before the next normalized message.

use super::*;
use crate::{
    app::CoreApp,
    platform::{
        host::NullHost,
        keys::{Key, MouseButton},
        window::HostEvent,
    },
};
use dereth_client_runtime::frame_events::{ActionRoute, FrameEvent};
use dereth_input::InputMapId;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
    time: u32,
}
impl Client {
    fn new() -> Self {
        let cfg = crate::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: std::env::temp_dir()
                .join("dereth-message-tests-not-created/prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
        )
        .expect("real tables and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        for _ in 0..2 {
            assert!(app.frame(&mut shell));
        }
        shell
            .modern
            .ui
            .as_mut()
            .expect("UI")
            .queue(dereth_ui::framework::mode::GAME_PLAY);
        for _ in 0..6 {
            assert!(app.frame(&mut shell));
        }
        app.actions.take();
        Self {
            app,
            shell,
            time: 100_000,
        }
    }
    fn event(&mut self, event: HostEvent) {
        self.time += 1;
        route_host_event(
            &mut self.app.ui_context(),
            &mut self.shell,
            &event,
            self.time,
        );
    }
    fn key(&mut self, key: Key, pressed: bool) {
        self.event(HostEvent::KeyboardInput {
            key,
            pressed,
            text: None,
        });
    }
    fn ui(&mut self) -> &mut crate::ui::UiShell {
        self.shell.modern.ui.as_mut().expect("UI")
    }
    fn broadcast_count(&self) -> u64 {
        self.shell
            .modern
            .ui
            .as_ref()
            .expect("UI")
            .stats
            .key_presses_broadcast
    }
    fn capture(&self) -> bool {
        self.shell
            .shared
            .input
            .as_ref()
            .expect("input")
            .manager
            .key_hit_handler_registered()
    }
    fn finish(self) {
        let Self { app, mut shell, .. } = self;
        app.shutdown(&mut shell);
    }
}

/// Behaviour: input.dispatch.declined-runtime-actions-are-offered-once
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn declined_runtime_actions_are_not_offered_again_to_ui_on_following_messages() {
    let mut c = Client::new();
    let before = c.broadcast_count();
    c.key(Key::new(0x57, 0x11), true);
    assert_eq!(
        c.broadcast_count(),
        before + 1,
        "one physical W reaches the UI handler once"
    );
    for x in [20.0, 21.0, 22.0] {
        c.event(HostEvent::CursorMoved { x, y: 20.0 });
        assert_eq!(
            c.broadcast_count(),
            before + 1,
            "later messages must not replay declined W"
        );
    }
    assert!(c.app.frame(&mut c.shell));
    assert!(
        c.app.char_input.forward,
        "the event survives frame expiry and reaches movement"
    );
    assert_eq!(movement_events(&c), 1);
    assert_eq!(
        c.broadcast_count(),
        before + 1,
        "frame routing does not replay UI dispatch"
    );
    c.key(Key::new(0x57, 0x11), false);
    assert!(c.app.frame(&mut c.shell));
    assert!(!c.app.char_input.forward);
    assert_eq!(movement_events(&c), 1, "one release reaches runtime");
    assert!(c.app.frame(&mut c.shell));
    assert_eq!(
        movement_events(&c),
        0,
        "no old action repeats in another frame"
    );
    c.key(Key::new(0x57, 0x11), true);
    c.event(HostEvent::CursorMoved { x: 23.0, y: 20.0 });
    c.event(HostEvent::Focused(false));
    assert!(c.app.frame(&mut c.shell));
    assert!(
        !c.app.char_input.forward,
        "same-batch focus loss must follow its pending Begin"
    );
    assert_eq!(movement_events(&c), 2);
    c.finish();
}

/// Behaviour: options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn capture_click_and_cancel_take_effect_before_the_next_host_key() {
    let mut c = Client::new();
    let cell = {
        let ui = c.ui();
        let screen = crate::hud_drive::game_screen(&mut ui.flow).expect("gameplay");
        let any: &mut dyn std::any::Any = screen;
        let screen = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("gameplay type");
        let row = screen
            .key_bindings
            .row_of(InputMapId(4), dereth_input::ActionId(0x29))
            .expect("forward row");
        screen.key_bindings.rows[row].key_buttons[0]
    };
    let mut current = Some(cell);
    while let Some(element) = current {
        let ui = &mut c.ui().ui;
        ui.set_visible(element, true);
        current = ui.parent(element);
    }
    assert!(c.app.frame(&mut c.shell));
    let rect = c.ui().ui.screen_box(cell);
    let (x, y) = ((rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2);
    assert_eq!(
        c.ui().ui.hit_test_screen(x, y),
        Some(cell),
        "physical click hits binding cell"
    );
    c.event(HostEvent::CursorMoved {
        x: f64::from(x),
        y: f64::from(y),
    });
    c.event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    c.event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    assert!(
        c.capture(),
        "click registers exclusive capture without another frame"
    );
    let before = c
        .shell
        .shared
        .input
        .as_ref()
        .expect("input")
        .manager
        .find_keys_for_action(dereth_input::ActionId(0x29), InputMapId(4));
    c.key(Key::new(0x1b, 0x01), true);
    c.key(Key::new(0x1b, 0x01), false);
    assert!(
        !c.capture(),
        "Escape removes capture before the following key"
    );
    let after = c
        .shell
        .shared
        .input
        .as_ref()
        .expect("input")
        .manager
        .find_keys_for_action(dereth_input::ActionId(0x29), InputMapId(4));
    assert_eq!(after, before, "cancel changes no binding");
    c.key(Key::new(0x57, 0x11), true);
    assert!(c.app.frame(&mut c.shell));
    assert!(
        c.app.char_input.forward,
        "the first key after cancel reaches runtime this frame"
    );
    assert_eq!(movement_events(&c), 1);
    c.key(Key::new(0x57, 0x11), false);
    assert!(c.app.frame(&mut c.shell));
    assert!(!c.app.char_input.forward);
    c.finish();
}

fn movement_events(c: &Client) -> usize {
    c.app
        .frame_events()
        .last_frame()
        .iter()
        .filter(|event| matches!(event, FrameEvent::ActionRouted(ActionRoute::Movement)))
        .count()
}
