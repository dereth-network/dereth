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
        Self::with_presentation(|| Box::new(crate::present::NullPresentation::new(800, 600)))
    }
    fn with_presentation(make: fn() -> Box<dyn crate::present::ClientPresentation>) -> Self {
        let cfg = dereth_client_runtime::config::Config {
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
            |_, _, _, _| Ok(make()),
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
            dereth_classic_ui::resources::Resources::new(
                art,
                Err("World creation tables unavailable".into()),
                None,
            ),
            dereth_classic_ui::art::ClassicPaths {
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
    store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
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
    store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
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
        c.shell.follow_interface(&mut c.app.ui_context());
        refresh(&mut c, true);
        refresh(&mut c, true);
        let lines = &c.shell.classic.ui.as_ref().unwrap().classic.chat;
        let matches: Vec<_> = lines.iter().filter(|(_, s)| s.contains(text)).collect();
        assert_eq!(matches.len(), 1, "one callback is drawn once");
        assert!(matches[0].1.contains(name));
    }
    // The next frame's step records what the chat windows were handed.
    c.shell.follow_interface(&mut c.app.ui_context());
    assert_eq!(c.shell.classic.history().count(), 2);
    deliver(&mut c, 123, "pending at logoff");
    c.app
        .apply_hud_events(&mut c.shell, &[SessionEvent::LoggedOff]);
    assert!(c.app.hud.pending_chat.is_empty());
    assert!(c.shell.classic.ui.as_ref().unwrap().classic.chat.is_empty());
    assert_eq!(c.shell.classic.history().count(), 0);
    assert!(c.shell.classic.take_missed().is_empty());
    refresh(&mut c, false);
    deliver(&mut c, 123, "after logoff");
    refresh(&mut c, true);
    let lines = &c.shell.classic.ui.as_ref().unwrap().classic.chat;
    assert!(!lines.iter().any(|(_, s)| s.contains("before gameplay")
        || s.contains("pending at logoff")
        || s.contains("after logoff")));
    store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
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

/// Behaviour: none (actual intro media and physical input ordering).
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn skipped_intro_movies_finish_without_an_extra_click() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::intro::IntroScreen;
    let mut c = Client::new();
    c.ui().queue(mode::INTRO);
    for _ in 0..4 {
        assert!(c.app.frame(&mut c.shell));
    }
    let state = |c: &mut Client| {
        let screen = c.ui().flow.current_mut().expect("intro screen");
        let any: &mut dyn std::any::Any = screen.as_mut();
        any.downcast_ref::<IntroScreen>()
            .expect("intro")
            .current_state
    };
    assert_eq!(state(&mut c), Some(0x1000_0039));
    for expected in [0x1000_003a, 0x1000_003e] {
        c.event(HostEvent::CursorMoved { x: 300.0, y: 200.0 });
        c.event(HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed: true,
        });
        c.event(HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed: false,
        });
        assert_eq!(state(&mut c), Some(expected));
    }
    for _ in 0..4 {
        assert!(c.app.frame(&mut c.shell));
    }
    assert_eq!(c.ui().flow.current_mode(), Some(mode::CHARACTER_MANAGEMENT));
    c.finish();
}

/// Behaviour: none (the first physical panel click after interface transitions).
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn first_toolbar_click_after_mode_and_interface_switch_toggles_once() {
    use dereth_client_contract::options::{
        interface::{Interface, INTERFACE},
        store,
    };
    use dereth_client_contract::PrefValue;
    let mut c = Client::new();
    c.install_classic();
    let button = {
        let screen = crate::hud_drive::game_screen(&mut c.ui().flow).expect("gameplay");
        let any: &mut dyn std::any::Any = screen;
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .unwrap()
            .toolbar
            .buttons[0]
            .handle
    };
    for switch in [false, true] {
        if switch {
            store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
            c.shell.follow_interface(&mut c.app.ui_context());
            store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
            c.shell.follow_interface(&mut c.app.ui_context());
        }
        let box_ = c.ui().ui.screen_box(button);
        let (x, y) = ((box_.x0 + box_.x1) / 2, (box_.y0 + box_.y1) / 2);
        assert_eq!(c.ui().ui.hit_test_screen(x, y), Some(button));
        for state in [
            dereth_ui_screens::toolbar::STATE_PANEL_OPEN,
            dereth_ui_screens::toolbar::STATE_PANEL_CLOSED,
        ] {
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
            assert_eq!(
                c.ui().ui.node(button).unwrap().state,
                state,
                "switch={switch}"
            );
            assert!(c.app.frame(&mut c.shell));
            assert_eq!(
                c.ui().ui.node(button).unwrap().state,
                state,
                "frame does not repeat the click"
            );
        }
    }
    c.finish();
}

/// Behaviour: none (persistent spellbook adapter observes receipts after the real session reset).
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn a_persistent_spellbook_selects_the_next_sessions_first_new_spell() {
    use dereth_client_contract::{snapshot::GameSnapshot, SpellEntry};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_primitives::{LocalTime, ObjectId};
    use dereth_ui_screens::panels::{remaining::SPELL_PAGE, spellbook::SpellbookPanel};
    let mut client = Client::new();
    let window = client
        .ui()
        .ui
        .get_element(SPELL_PAGE)
        .expect("magic window");
    let mut panel = SpellbookPanel::default();
    panel.post_init(&mut client.ui().ui, window);
    for (player, spell, serial) in [(1, 7, 1), (2, 8, 2)] {
        client.app.objects.world.player = Some(ObjectId(player));
        client.app.objects.world.research_spell_update(spell, true);
        let mut view = GameSnapshot::from_view(&dereth_client_runtime::hud::HudView {
            hud: &client.app.hud,
            world: &client.app.objects.world,
        });
        view.spellbook = vec![SpellEntry {
            id: spell,
            name: format!("Spell {spell}"),
            school: 4,
            level: 1,
            icon: None,
            icon_power: 1,
            display_order: 0,
            bitfield: 0,
        }];
        assert_eq!(view.last_learned_spell, Some((serial, spell)));
        panel.update(&mut client.ui().ui, &view);
        assert_eq!(panel.selected_spell, spell);
        assert!(panel
            .list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .any(|s| s.selected && s.spell == Some(spell)));
        client
            .app
            .objects
            .apply_event(&SessionEvent::WorldReset, LocalTime(0.0));
        assert!(client.app.objects.world.magic.last_learned_spell.is_none());
    }
    client.finish();
}

/// Behaviour: none (physical pending-connection Cancel reaches the real shell and App shutdown).
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn modern_pending_connection_cancel_stops_without_another_click() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::datapatch::{DataPatchScreen, QUIT_BUTTON};
    let mut client = Client::new();
    let network =
        dereth_client_runtime::net::ClientNetwork::new("127.0.0.1", 19601, "pending", "pending", 1)
            .unwrap();
    client
        .app
        .attach_replay_network(network)
        .expect("socket-free pending connection");
    client.ui().queue(mode::DATA_PATCH);
    for _ in 0..2 {
        assert!(client.app.frame(&mut client.shell));
    }
    assert_eq!(client.ui().flow.current_mode(), Some(mode::DATA_PATCH));
    let screen = client.ui().flow.current_mut().unwrap();
    let any: &mut dyn std::any::Any = screen.as_mut();
    let progress = any.downcast_ref::<DataPatchScreen>().unwrap();
    assert!(progress.has_packet_controller);
    assert!(!progress.connected && !progress.received_set);
    let button = client.ui().ui.get_element(QUIT_BUTTON).unwrap();
    let rect = client.ui().ui.screen_box(button);
    let (x, y) = ((rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2);
    assert_eq!(client.ui().ui.hit_test_screen(x, y), Some(button));
    client.event(HostEvent::CursorMoved {
        x: f64::from(x),
        y: f64::from(y),
    });
    client.event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    client.event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    let mut trace = Vec::new();
    let stopped = (0..4).any(|_| {
        let keep = client.app.frame(&mut client.shell);
        let mode = client.ui().flow.current_mode();
        let epilogue = client.ui().flow.current_mut().and_then(|screen| {
            let any: &mut dyn std::any::Any = screen.as_mut();
            any.downcast_ref::<dereth_ui_screens::screens::epilogue::EpilogueScreen>()
                .map(|s| (s.done, s.log_off_requested))
        });
        trace.push((mode, epilogue));
        !keep
    });
    assert!(
        stopped,
        "the first Cancel click ends the pending connection: {trace:?}"
    );
    client.finish();
}

/// Behaviour: presentation.settings.both-interfaces-edit-one-store
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn interface_switches_apply_inversion_to_present_and_later_bodies() {
    use dereth_client_contract::{
        options::{
            interface::{Interface, INTERFACE},
            names, store,
        },
        PrefValue,
    };
    let mut c = Client::with_presentation(|| {
        Box::new(dereth_client_runtime::sim_present::SimPresentation::new(
            800, 600,
        ))
    });
    c.install_classic();
    store::set_value(names::INVERT_MOUSE_LOOK_Y_AXIS, PrefValue::Bool(true));
    store::set_value(names::FIELD_OF_VIEW, PrefValue::Float(75.0));
    store::set_value(names::PLAY_SOUND_ONLY_WHEN_ACTIVE, PrefValue::Bool(true));
    c.app.audio = Some(dereth_client_runtime::audio::Audio::with_output(
        Default::default(),
        1,
        None,
    ));
    c.shell
        .classic
        .ui
        .as_mut()
        .unwrap()
        .start(&mut c.app.ui_context())
        .unwrap();
    let sound = c.app.audio.as_ref().unwrap().stats.preferences_applied;
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(c.app.hud.classic_active);
    assert!(c.app.probe_mut().world_state_mut().is_none());
    assert!(c.app.frame(&mut c.shell));

    let store_files = std::sync::Arc::clone(c.app.probe().dat_store());
    let mut cfg = dereth_client_runtime::scene::SceneConfig {
        land_radius: 0,
        scenery_radius: 0,
        ..Default::default()
    };
    cfg.mouse_look.invert_y = true;
    c.app.probe_mut().load_world(&store_files, cfg).unwrap();
    assert!(
        c.app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .prefs
            .invert_y
    );
    assert!(c.app.frame(&mut c.shell));
    assert!(
        !c.app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .prefs
            .invert_y,
        "Classic applies its input override when the delayed body appears"
    );

    for face in [Interface::Modern, Interface::Classic, Interface::Modern] {
        store::set_value(INTERFACE, PrefValue::Int(face.value()));
        c.shell.follow_interface(&mut c.app.ui_context());
        assert_eq!(c.app.hud.classic_active, face == Interface::Classic);
        assert_eq!(
            c.app
                .probe_mut()
                .world_state_mut()
                .unwrap()
                .character
                .as_ref()
                .unwrap()
                .camera
                .prefs
                .invert_y,
            face == Interface::Modern,
            "successful activation applies the face's input policy before another frame"
        );
    }
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    c.app.probe_mut().world_state_mut().unwrap().character = None;
    assert!(c.app.frame(&mut c.shell));
    c.app.probe_mut().load_world(&store_files, cfg).unwrap();
    assert!(c.app.frame(&mut c.shell));
    assert!(
        !c.app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .camera
            .prefs
            .invert_y,
        "a replacement body receives the override even with unchanged option words"
    );
    assert_eq!(
        store::inq_value(names::INVERT_MOUSE_LOOK_Y_AXIS),
        Some(PrefValue::Bool(true))
    );
    assert_eq!(
        store::inq_value(names::FIELD_OF_VIEW),
        Some(PrefValue::Float(75.0))
    );
    assert_eq!(
        store::inq_value(names::PLAY_SOUND_ONLY_WHEN_ACTIVE),
        Some(PrefValue::Bool(true))
    );
    assert_eq!(
        c.app.audio.as_ref().unwrap().stats.preferences_applied,
        sound,
        "interface overrides do not resend shared sound preferences"
    );
    c.finish();
}

/// Behaviour: presentation.settings.both-interfaces-edit-one-store
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn classic_projection_follows_committed_values_and_mute_binding_delivers_once() {
    use dereth_client_contract::{
        options::{
            interface::{Interface, INTERFACE},
            store,
        },
        PrefValue,
    };
    let mut c = Client::new();
    c.install_classic();
    store::set_value("Sound.SoundVolume", PrefValue::Float(0.4));
    store::set_value("Input.InvertMouseLookYAxis", PrefValue::Bool(true));
    store::set_value("Render.FieldOfView", PrefValue::Float(0.75));
    store::set_value("Sound.PlaySoundOnlyWhenActive", PrefValue::Bool(false));
    c.shell
        .classic
        .ui
        .as_mut()
        .unwrap()
        .start(&mut c.app.ui_context())
        .unwrap();
    store::set_value(INTERFACE, PrefValue::Int(Interface::Classic.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(c.app.hud.classic_active);
    assert_eq!(
        c.shell.classic.ui.as_ref().unwrap().settings.effects_volume,
        0.4
    );
    store::set_value("Sound.SoundVolume", PrefValue::Float(0.2));
    assert!(c.app.frame(&mut c.shell));
    assert_eq!(
        c.shell.classic.ui.as_ref().unwrap().settings.effects_volume,
        0.2
    );
    c.app.audio = Some(dereth_client_runtime::audio::Audio::with_output(
        Default::default(),
        1,
        None,
    ));
    let before = c.app.audio.as_ref().unwrap().stats.preferences_applied;
    assert!(dereth_classic_ui::keyboard_runtime::handle(
        &mut c.app.ui_context(),
        "MuteOnLosingFocus",
        true
    )
    .unwrap());
    assert_eq!(
        store::inq_value("Sound.PlaySoundOnlyWhenActive"),
        Some(PrefValue::Bool(true))
    );
    assert_eq!(
        c.app.audio.as_ref().unwrap().stats.preferences_applied,
        before + 1
    );
    assert!(dereth_classic_ui::keyboard_runtime::handle(
        &mut c.app.ui_context(),
        "MuteOnLosingFocus",
        false
    )
    .unwrap());
    assert!(dereth_classic_ui::keyboard_runtime::handle(
        &mut c.app.ui_context(),
        "InvertMouseLook",
        true
    )
    .unwrap());
    assert_eq!(
        c.app.audio.as_ref().unwrap().stats.preferences_applied,
        before + 1
    );
    store::set_value(INTERFACE, PrefValue::Int(Interface::Modern.value()));
    c.shell.follow_interface(&mut c.app.ui_context());
    assert!(!c.app.hud.classic_active);
    assert_eq!(
        c.app.audio.as_ref().unwrap().stats.preferences_applied,
        before + 1
    );
    assert_eq!(
        store::inq_value("Sound.PlaySoundOnlyWhenActive"),
        Some(PrefValue::Bool(true))
    );
    assert_eq!(
        store::inq_value("Input.InvertMouseLookYAxis"),
        Some(PrefValue::Bool(true))
    );
    assert_eq!(
        store::inq_value("Render.FieldOfView"),
        Some(PrefValue::Float(0.75))
    );
    c.finish();
}

/// Behaviour: presentation.settings.both-interfaces-edit-one-store
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail layouts and key maps"
)]
fn classic_live_preview_and_cancel_leave_committed_sound_and_camera_steps_intact() {
    use dereth_classic_ui::{panels::ClassicSettings, settings_host::SettingsHost};
    use dereth_client_contract::{options::store, PrefValue};
    let mut c = Client::new();
    let mut host = SettingsHost::load(ClassicSettings {
        resolutions: vec![(800, 600), (1024, 768)],
        ..Default::default()
    })
    .unwrap();
    let mut applied = host.snapshot();
    applied.brightness = 0.5;
    applied.effects_volume = 0.4;
    applied.camera_stiffness = 0.3;
    assert!(host
        .apply(&mut c.app.ui_context(), applied, true)
        .unwrap()
        .is_empty());
    let saved = host.snapshot();
    let camera = store::inq_value("Camera.Stiffness");
    let mut draft = saved.clone();
    draft.brightness = 0.9;
    draft.effects_volume = 0.8;
    draft.resolution = 1;
    draft.camera_stiffness = 0.0;
    host.preview(&mut c.app.ui_context(), &draft).unwrap();
    assert_eq!(host.snapshot().brightness, 0.9);
    assert_eq!(host.snapshot().effects_volume, 0.4);
    assert_eq!(host.snapshot().resolution, 0);
    assert_eq!(store::inq_value("Camera.Stiffness"), camera);
    assert_eq!(
        store::inq_value("Render.ScreenBrightness"),
        Some(PrefValue::Float(0.0))
    );
    store::set_value("Sound.SoundVolume", PrefValue::Float(0.2));
    host.sync(&mut c.app.ui_context()).unwrap();
    assert_eq!(host.snapshot().effects_volume, 0.2);
    assert_eq!(host.snapshot().brightness, 0.9);
    host.reset(&mut c.app.ui_context()).unwrap();
    assert_eq!(host.snapshot().brightness, 0.5);
    assert_eq!(
        host.snapshot().camera_stiffness.to_bits(),
        0.3_f32.to_bits()
    );
    assert_eq!(host.snapshot().effects_volume, 0.2);
    assert_eq!(store::inq_value("Camera.Stiffness"), camera);
    store::set_value("Render.ScreenBrightness", PrefValue::Float(0.3));
    host.sync(&mut c.app.ui_context()).unwrap();
    assert_eq!(
        host.snapshot().brightness,
        0.65,
        "Cancel ended the live preview"
    );
    assert_eq!(host.snapshot().effects_volume, 0.2);
    assert_eq!(
        host.snapshot().camera_stiffness.to_bits(),
        0.3_f32.to_bits()
    );
    c.finish();
}

/// The shard's welcome, as Empyrean sends it on entering the world.
const WELCOME: &str = "Welcome to Dereth\n powered by Empyrean\n\nFor more information on commands supported by this server, type @emphelp\n";

/// The welcome as the session hands it to the HUD.
fn welcome_event() -> dereth_client_net::client_session::SessionEvent {
    use dereth_protocol::Message as _;
    type M = dereth_protocol::comms::CommunicationTextboxString;
    let m = M {
        text: WELCOME.to_owned(),
        text_type: 0,
    };
    let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
    blob.extend(dereth_protocol::write_body(&m).unwrap());
    dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob,
    }
}

/// Behaviour: feedback.delivery.a-line-is-in-the-classic-log-once-after-relogs-and-switches
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn the_welcome_is_in_the_classic_log_once_after_relogs_and_switches() {
    use dereth_client_contract::options::{
        interface::{Interface, INTERFACE},
        store,
    };
    use dereth_client_contract::PrefValue;
    use dereth_client_net::client_session::SessionEvent;
    let mut c = Client::new();
    c.install_classic();
    let frames = |c: &mut Client, n| {
        for _ in 0..n {
            assert!(c.app.frame(&mut c.shell));
        }
    };
    // Entering the world: the welcome arrives and waits a few frames before a chat window takes
    // it (the shell follows the interface choice on every one of them), then frames run.
    let log_in = |c: &mut Client| {
        c.app.apply_hud_events(&mut c.shell, &[welcome_event()]);
        for _ in 0..5 {
            c.shell.follow_interface(&mut c.app.ui_context());
        }
        frames(c, 3);
    };
    let choose = |c: &mut Client, interface: Interface| {
        store::set_value(INTERFACE, PrefValue::Int(interface.value()));
        frames(c, 3);
        assert_eq!(c.shell.classic.active, interface == Interface::Classic);
    };
    let welcomes = |c: &Client| {
        let log = c
            .shell
            .classic
            .ui
            .as_ref()
            .expect("the classic interface is up")
            .classic
            .chat
            .iter()
            .filter(|(_, s)| s.contains("Welcome to Dereth"))
            .count();
        let kept = c
            .shell
            .classic
            .history()
            .filter(|m| m.body.contains("Welcome to Dereth"))
            .count();
        (log, kept)
    };
    c.app.host_state.in_world = true;
    log_in(&mut c);
    // Modern -> classic -> modern -> classic.
    choose(&mut c, Interface::Classic);
    assert_eq!(
        welcomes(&c),
        (1, 1),
        "the first switch to the classic interface"
    );
    choose(&mut c, Interface::Modern);
    choose(&mut c, Interface::Classic);
    assert_eq!(
        welcomes(&c),
        (1, 1),
        "the second switch to the classic interface"
    );
    // Two relogs in the classic interface, then the switches again.
    for relog in 1..=2 {
        c.app
            .apply_hud_events(&mut c.shell, &[SessionEvent::LoggedOff]);
        frames(&mut c, 3);
        assert_eq!(welcomes(&c), (0, 0), "logging off empties the log");
        log_in(&mut c);
        assert_eq!(welcomes(&c), (1, 1), "after relog {relog}");
    }
    for round in 1..=2 {
        choose(&mut c, Interface::Modern);
        choose(&mut c, Interface::Classic);
        assert_eq!(
            welcomes(&c),
            (1, 1),
            "switch round {round} after the relogs"
        );
    }
    choose(&mut c, Interface::Modern);
    c.finish();
}
