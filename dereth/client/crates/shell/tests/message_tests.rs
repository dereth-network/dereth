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
    fn install_classic(&mut self) {
        struct Fonts;
        impl dereth_classic_dat::fonts::FontSource for Fonts {
            fn rasterize(
                &self,
                _: &dereth_classic_dat::fonts::FontSpec,
            ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
                Ok(Default::default())
            }
        }
        let portal = std::path::PathBuf::from(
            std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
        );
        let art = std::sync::Arc::new(
            dereth_classic_ui::art::ClassicArt::new(
                dereth_classic_dat::ClassicPortal::open(&portal).unwrap(),
                &Fonts,
            )
            .unwrap(),
        );
        self.shell.classic.ui = Some(dereth_classic_ui::runtime::ClassicUi::new(
            art,
            dereth_classic_ui::art::ClassicPaths {
                portal_dir: portal.parent().map(ToOwned::to_owned),
                state: std::env::temp_dir().join("dereth-feedback-no-writes"),
            },
            dereth_classic_ui::panels::factory,
            (800, 600),
        ));
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

/// Behaviour: feedback.delivery.active-face-switches-do-not-replay-pending-transients
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn active_face_switches_drop_old_pending_transients_without_replaying_chat_history() {
    use dereth_client_contract::feedback::Feedback;
    use dereth_client_contract::options::{
        interface::{Interface, INTERFACE},
        store,
    };
    use dereth_client_contract::{panels::HudPanels, PrefValue};
    let mut c = Client::new();
    c.install_classic();
    c.app
        .ui_context()
        .add_feedback_line("before Classic switch", 0x1a, Feedback::WARNING);
    c.app.apply_hud_events(&mut c.shell, &[]);
    assert_eq!(
        c.app.hud.panels.spew.model.pending,
        ["before Classic switch"]
    );
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(c.app.hud.classic_active);
    assert!(c.app.hud.panels.spew.model.pending.is_empty());
    assert!(c.app.hud.classic.transient.is_empty());
    c.app
        .ui_context()
        .add_feedback_line("before Modern switch", 0x1a, Feedback::INFORMATION);
    c.app.apply_hud_events(&mut c.shell, &[]);
    assert_eq!(c.app.hud.classic.transient.len(), 1);
    store::set_value(INTERFACE, PrefValue::Int(Interface::Retail.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(!c.app.hud.classic_active);
    assert!(c.app.hud.classic.transient.is_empty());
    assert!(c.app.hud.panels.spew.model.pending.is_empty());
    // A queued network/scroll notice at logoff is not delivered to the next session.
    c.app
        .ui_context()
        .add_feedback_line("before logoff", 0x1a, Feedback::WARNING);
    c.app.apply_hud_events(
        &mut c.shell,
        &[dereth_client_net::client_session::SessionEvent::LoggedOff],
    );
    assert!(c.app.hud.panels.spew.model.pending.is_empty());
    assert!(c.app.hud.pending_chat.is_empty());
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(c.app.hud.classic.transient.is_empty());
    // The ordinary textbox route may still reach Modern's channel-based receiver.
    assert!(!c
        .app
        .hud
        .classic
        .spew_offer(0x1a, "ordinary", Feedback::ORDINARY));
    store::set_value(INTERFACE, PrefValue::Int(Interface::Retail.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
}

/// Behaviour: feedback.modern.typed-lines-keep-yellow-replacement-and-expiry
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn typed_lines_reach_the_bound_modern_bubble_and_keep_replacement_and_expiry() {
    use dereth_client_contract::feedback::Feedback;
    let mut c = Client::new();
    for feedback in [Feedback::INFORMATION, Feedback::WARNING, Feedback::ORDINARY] {
        c.app
            .ui_context()
            .add_feedback_line("same bubble", 0x1a, feedback);
        for _ in 0..3 {
            assert!(c.app.frame(&mut c.shell));
        }
        let spew = &c.app.hud.panels.spew;
        assert_eq!(spew.model.items, ["same bubble"]);
        let list = spew.list.as_ref().expect("actual bound list");
        assert_eq!(list.items.len(), 1);
        let handle = list.items[0];
        assert_eq!(
            c.ui().ui.text_element_mut(handle).unwrap().font_color,
            0xffffff00
        );
    }
    for _ in 0..180 {
        assert!(c.app.frame(&mut c.shell));
    }
    assert!(c.app.hud.panels.spew.model.items.is_empty());
    assert!(c
        .app
        .hud
        .panels
        .spew
        .list
        .as_ref()
        .unwrap()
        .items
        .is_empty());
}

/// Behaviour: chat.classic.global-room-callbacks-reach-the-active-chat-window
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn classic_global_room_callbacks_reach_chat_once_and_stop_outside_gameplay() {
    use dereth_client_contract::options::{
        interface::{Interface, INTERFACE},
        store,
    };
    use dereth_client_contract::PrefValue;
    use dereth_client_net::client_session::SessionEvent;
    let mut c = Client::new();
    c.install_classic();
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    let refresh = |c: &mut Client, in_world| {
        c.app.host_state.in_world = in_world;
        c.shell.classic.ui.as_mut().unwrap().ui_frame(
            &mut c.app.ui_context(),
            dereth_primitives::LocalTime(1.0),
            Default::default(),
        );
    };
    let deliver = |c: &mut Client, room, text| {
        c.app.apply_hud_events(
            &mut c.shell,
            &[SessionEvent::TurbineChat(global_room_event(room, text))],
        );
    };
    c.app.objects.world.chat.startup_turbine_chat();
    c.app
        .objects
        .world
        .chat
        .recv_chat_room_tracker(dereth_protocol::comms::ChatRoomMembership {
            general_room: 123,
            trade_room: 124,
            ..Default::default()
        });
    refresh(&mut c, false);
    deliver(&mut c, 123, "before gameplay");
    assert!(c.app.hud.pending_chat.is_empty());
    refresh(&mut c, true);
    for (room, name, text) in [
        (123, "General", "general echo"),
        (124, "Trade", "trade echo"),
    ] {
        deliver(&mut c, room, text);
        refresh(&mut c, true);
        refresh(&mut c, true);
        let lines = &c.shell.classic.ui.as_ref().unwrap().classic.chat;
        let matches: Vec<_> = lines.iter().filter(|(_, s)| s.contains(text)).collect();
        assert_eq!(matches.len(), 1, "one callback is drawn once");
        assert!(matches[0].1.contains(name));
    }
    deliver(&mut c, 123, "pending at logoff");
    c.app
        .apply_hud_events(&mut c.shell, &[SessionEvent::LoggedOff]);
    assert!(c.app.hud.pending_chat.is_empty());
    refresh(&mut c, false);
    deliver(&mut c, 123, "after logoff");
    refresh(&mut c, true);
    let lines = &c.shell.classic.ui.as_ref().unwrap().classic.chat;
    assert!(!lines.iter().any(|(_, s)| s.contains("before gameplay")
        || s.contains("pending at logoff")
        || s.contains("after logoff")));
    store::set_value(INTERFACE, PrefValue::Int(Interface::Retail.value()));
    c.finish();
}

fn global_room_event(room: u32, text: &str) -> Vec<u8> {
    let mut body = dereth_protocol::Writer::new();
    body.u32(room);
    for text in ["Speaker", text] {
        let units: Vec<_> = text.encode_utf16().collect();
        body.compressed_u32(u32::try_from(units.len()).unwrap());
        for unit in units {
            body.u16(unit);
        }
    }
    body.u32(12);
    body.u32(0x5000_0017);
    body.u32(0);
    body.u32(2);
    let body = body.into_inner();
    let mut packet = dereth_protocol::Writer::new();
    packet.u32(u32::try_from(body.len()).unwrap() + 32);
    for word in [1, 1, 1, 0, 0, 0, 0] {
        packet.u32(word);
    }
    packet.u32(u32::try_from(body.len()).unwrap());
    let mut packet = packet.into_inner();
    packet.extend(body);
    packet
}
