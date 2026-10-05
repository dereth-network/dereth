use super::*;
use dereth_client_runtime::{
    app::{App, Platform},
    config::Config,
    hud::{Hud, HudSlot},
    present::NullPresentation,
};
use dereth_input::{
    host::HostEvent,
    keys::{Key, MouseButton},
    pump::DeviceMessages,
    CallbackId, InputManager, InputMapId,
};
use std::sync::Arc;

#[derive(Debug, Default)]
pub(super) struct TestHud {
    model: Hud,
    panels: ClassicHudPanels,
}
impl std::ops::Deref for TestHud {
    type Target = Hud;
    fn deref(&self) -> &Hud {
        &self.model
    }
}
impl std::ops::DerefMut for TestHud {
    fn deref_mut(&mut self) -> &mut Hud {
        &mut self.model
    }
}
impl HudSlot for TestHud {
    fn split(&mut self) -> (&mut Hud, &mut dyn HudPanels) {
        (&mut self.model, &mut self.panels)
    }
}
impl ClassicHudSlot for TestHud {
    fn classic_panels(&mut self) -> &mut ClassicHudPanels {
        &mut self.panels
    }
}
pub(super) struct TestShell {
    ui: ClassicUi,
    events: Vec<HostEvent>,
    input: InputManager,
    devices: DeviceMessages,
    mapped: Vec<dereth_input::InputEvent>,
    captured: Vec<dereth_input::ControlChord>,
    open_edit: bool,
    world_inputs: bool,
    arm_target: bool,
    pub(super) no_ui: bool,
    in_world: bool,
    nearby: Option<dereth_primitives::ObjectId>,
}
impl TestShell {
    fn sync_input(&mut self, key: Option<u16>, capture: bool) {
        let (editing, modal, own_capture) = self.ui.input_scope(None);
        self.input.maps.unregister_callback(CallbackId(2));
        if editing {
            for (map, priority) in [(10, 3000), (1, 2990), (7, 3000), (8, 3000)] {
                self.input
                    .register_input_map(InputMapId(map), priority, CallbackId(2));
            }
        } else if self.ui.keyboard_barrier(key) {
            self.input
                .register_input_map(InputMapId(1), 2990, CallbackId(2));
        }
        if self.ui.chat_focused() && !self.ui.keyboard_barrier(key) {
            self.input.maps.register_scoped(
                dereth_input::dereth::INPUT_MAP,
                3001,
                CallbackId(2),
                Some(dereth_client_contract::actions::dereth::REPEAT_LAST_MESSAGE),
            );
        }
        if modal {
            self.input
                .register_input_map(InputMapId(9), 3000, CallbackId(2));
        }
        if self.input.text.text_mode != editing {
            self.input.set_text_mode(editing);
        }
        self.input.set_key_hit_handler(capture || own_capture);
    }

    // Drive the interface boundary with normalized messages and the manager's real action state.
    // The desktop receives editing keys; accepted characters follow their mapped key action.
    fn deliver(&mut self, cx: &mut Cx<'_, Self>, event: HostEvent) {
        self.ui.prepare_host_input(cx, &event);
        let key = match &event {
            HostEvent::KeyboardInput { key, .. } => u16::try_from(key.virtual_key).ok(),
            _ => None,
        };
        let capturing = self.ui.input_scope(key).2;
        self.sync_input(key, capturing);
        let widget = match &event {
            HostEvent::KeyboardInput { key, pressed, .. } => HostEvent::KeyboardInput {
                key: *key,
                pressed: *pressed,
                text: None,
            },
            _ => event.clone(),
        };
        let mut widget_key_pending = matches!(event, HostEvent::KeyboardInput { .. });
        if !widget_key_pending {
            self.ui
                .window_input(cx, std::slice::from_ref(&widget), &mut EmptyClipboard);
        }
        for message in self.devices.map_device_event(&event, 0) {
            self.input.on_window_event(&message, &|_| false);
            self.captured.extend(self.input.take_key_hits());
            let events = self.input.take_events();
            let consumed = events
                .iter()
                .any(|e| !matches!(e.input_map.0, 1 | 7 | 8 | 9 | 10));
            self.mapped.extend_from_slice(&events);
            for event in events {
                self.input.begin_action_dispatch(event.from_key_down);
                self.ui.mapped_action(cx, event);
                let editing = self.ui.input_scope(None).0;
                if self.input.text.text_mode != editing {
                    self.input.set_text_mode(editing);
                }
                self.input.end_action_dispatch();
            }
            self.ui.mapped_characters(cx, self.input.take_characters());
            if widget_key_pending {
                widget_key_pending = false;
                if !consumed || capturing {
                    self.ui
                        .window_input(cx, std::slice::from_ref(&widget), &mut EmptyClipboard);
                }
                self.sync_input(key, capturing);
            }
        }
        self.sync_input(None, false);
    }
}

fn input_manager(store: &dyn dereth_primitives::AssetSource) -> InputManager {
    let read = |group, id| {
        store
            .read(dereth_client_runtime::assets::enum_did(store, group, id).unwrap())
            .unwrap()
    };
    let action = read(8, 1);
    let engine = read(10, 1);
    let game = read(10, 0x1000_0001);
    let mut input = InputManager::on_startup(&action, &engine).unwrap();
    input.init_keymap(None, &game, &engine).unwrap();
    let shipped = input.shipped_maps.as_ref().unwrap();
    input.keymap = crate::default_keys::default_map(
        &input.keymap,
        &input.action_map,
        &[&shipped.0, &shipped.1],
    );
    for (map, priority) in [
        (4, 1000),
        (0x1000_0007, 1000),
        (0x1000_0009, 1000),
        (0x1000_000a, 1000),
        (0x1000_000d, 3010),
        (0x2000_0000, 0),
    ] {
        input.register_input_map(InputMapId(map), priority, CallbackId(1));
    }
    input
}

struct EmptyClipboard;
impl Clipboard for EmptyClipboard {
    fn get(&mut self) -> Option<String> {
        None
    }
    fn set(&mut self, _: &str) {}
}
impl Shell for TestShell {
    type Hud = TestHud;
    type Present = dyn Presentation;
    fn set_display(&mut self, display: (i32, i32)) {
        self.ui.set_display(display);
    }
    fn in_gameplay(&self) -> bool {
        self.in_world
    }
    fn has_ui(&self) -> bool {
        !self.no_ui
    }
    fn service_dialogs(&mut self, cx: &mut Cx<'_, Self>, now: dereth_primitives::LocalTime) {
        cx.service_dialogs(None, now);
        if let Some(id) = self.nearby {
            cx.hud_mut().radar = vec![dereth_client_contract::view::RadarEntry {
                id,
                player_space: (2.0, 0.0, 0.0),
                in_world: true,
                ..Default::default()
            }];
        }
    }
    fn ui_frame(
        &mut self,
        cx: &mut Cx<'_, Self>,
        _: dereth_primitives::LocalTime,
        _: dereth_client_runtime::shell::UiNotices,
    ) {
        cx.set_chat_interface(dereth_client_contract::options::interface::Interface::Classic);
        if !self.ui.desktop.is_open("hud") {
            let view = cx.hud().view(cx.objects());
            let context = Context {
                resources: &self.ui.resources,
                layout: self.ui.desktop.layout(),
                now: dereth_primitives::LocalTime(0.0),
                game: &view,
                pregame: cx.pregame(),
                keyboard: &self.ui.keyboard,
                settings: &self.ui.settings,
                map_teleport_allowed: false,
                classic: &self.ui.classic,
            };
            self.ui.desktop.open("hud", &context);
        }
        if self.open_edit {
            self.open_edit = false;
            let view = cx.hud().view(cx.objects());
            let context = Context {
                resources: &self.ui.resources,
                layout: self.ui.desktop.layout(),
                now: dereth_primitives::LocalTime(0.0),
                game: &view,
                pregame: cx.pregame(),
                keyboard: &self.ui.keyboard,
                settings: &self.ui.settings,
                map_teleport_allowed: false,
                classic: &self.ui.classic,
            };
            self.ui.desktop.open("test-edit", &context);
            self.ui.desktop.set_position("test-edit", 500, 100);
            self.ui.desktop.focus_control("chat:input");
            self.ui.desktop.screen();
        }
        if self.arm_target {
            self.arm_target = false;
            cx.run_request(
                UiRequest::SetTargetMode(dereth_client_contract::view::TargetMode::Use),
                dereth_primitives::LocalTime(cx.now()),
                &mut |_, _| false,
            );
        }
        self.ui.finish_chat_keyboard(cx, self.ui.inputs.len());
        for event in std::mem::take(&mut self.events) {
            self.deliver(cx, event);
        }
        if self.world_inputs {
            self.ui
                .process_inputs(cx, dereth_primitives::LocalTime(cx.now()), true);
        }
    }
}
#[derive(Debug)]
struct TestEdit;
impl crate::panels::Panel for TestEdit {
    fn id(&self) -> &'static str {
        "test-edit"
    }
    fn frame(&self, _: &Context<'_>) -> crate::panels::PanelFrame {
        let mut frame = crate::panels::PanelFrame::new(300, 100);
        frame.edit("other-input", rect(0, 0, 100, 20), "", 100, false, true);
        frame.button("paper", rect(0, 30, 100, 30), "model", true);
        frame.preview(crate::panels::Preview {
            kind: PreviewKind::PaperDoll,
            rect: rect(0, 30, 100, 30),
            object: None,
            appearance: None,
        });
        frame
    }
    fn event(&mut self, event: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        if matches!(event, ControlEvent::Activate(id) if id == "paper") {
            return vec![PanelAction::Game(UiRequest::Select(
                dereth_primitives::ObjectId(88),
            ))];
        }
        vec![]
    }
}
struct Fonts;
impl dereth_classic_dat::fonts::FontSource for Fonts {
    fn rasterize(
        &self,
        _: &dereth_classic_dat::fonts::FontSpec,
    ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
        Ok(Default::default())
    }
}
pub(super) fn fixture() -> (App<TestShell>, TestShell) {
    let store = Arc::new(dereth_dat::testing::open_store().expect("retail DATs"));
    let path = std::path::PathBuf::from(
        std::env::var_os("DERETH_CLASSIC_PORTAL").expect("classic portal"),
    );
    let art = Arc::new(
        crate::art::ClassicArt::new(
            dereth_classic_dat::ClassicPortal::open(&path).unwrap(),
            &Fonts,
        )
        .unwrap(),
    );
    let ui = ClassicUi::new(
        crate::resources::Resources::new(
            art,
            Err("World creation tables unavailable".into()),
            None,
        ),
        crate::art::ClassicPaths {
            state: std::env::temp_dir().join("dereth-chat-keyboard-no-writes"),
        },
        |id, now, resources| {
            if id == "test-edit" {
                Some(Box::new(TestEdit))
            } else {
                crate::panels::factory(id, now, resources)
            }
        },
        (800, 600),
    );
    let input = input_manager(&*store);
    let config = Config {
        headless: true,
        frames: None,
        connect: false,
        sound: false,
        ui: false,
        preferences_file: std::env::temp_dir().join("dereth-chat-keyboard-no-writes/prefs.ini"),
        ..Config::default()
    };
    let app = App::bring_up_with_store(
        config,
        Some(store),
        |_| Ok(Platform::headless(800, 600)),
        |_, _, _, _| Ok(Box::new(NullPresentation::new(800, 600)) as Box<dyn Presentation>),
    )
    .unwrap();
    (
        app,
        TestShell {
            ui,
            events: Vec::new(),
            input,
            devices: DeviceMessages::default(),
            mapped: Vec::new(),
            captured: Vec::new(),
            open_edit: false,
            world_inputs: false,
            arm_target: false,
            no_ui: false,
            in_world: false,
            nearby: None,
        },
    )
}
fn key(vk: usize, text: Option<&str>) -> HostEvent {
    HostEvent::KeyboardInput {
        key: Key::new(vk, {
            let scan = u16::try_from(vk)
                .ok()
                .and_then(crate::default_keys::scan_code)
                .unwrap_or(0);
            if scan & 0x80 != 0 {
                0xe000 | (scan & 0x7f)
            } else {
                scan
            }
        }),
        pressed: true,
        text: text.map(str::to_owned),
    }
}
fn click(x: f64, y: f64) -> Vec<HostEvent> {
    vec![
        HostEvent::CursorMoved { x, y },
        HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed: true,
        },
        HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed: false,
        },
    ]
}
pub(super) fn batch(app: &mut App<TestShell>, shell: &mut TestShell, events: Vec<HostEvent>) {
    shell.events = events
        .into_iter()
        .flat_map(|event| {
            let release = match &event {
                HostEvent::KeyboardInput {
                    key, pressed: true, ..
                } => Some(HostEvent::KeyboardInput {
                    key: *key,
                    pressed: false,
                    text: None,
                }),
                _ => None,
            };
            std::iter::once(event).chain(release)
        })
        .collect();
    assert!(app.frame(shell));
    assert!(shell.events.is_empty());
}

/// Behaviour: chat.entry-adapters
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail DATs and DERETH_CLASSIC_PORTAL"
)]
fn classic_host_batches_apply_reply_history_aliases_and_pointer_focus_before_following_text() {
    let (mut app, mut shell) = fixture();
    app.probe_mut().objects_mut().world.chat.last_teller_name = "Peer".into();
    shell.ui.ui_actions.push("Reply".into());
    batch(
        &mut app,
        &mut shell,
        vec![key(0x48, Some("H")), key(0x0d, None)],
    );
    assert_eq!(
        app.objects().world.chat.entries[&8].history(),
        ["@t Peer, H"]
    );
    assert_eq!(app.interaction().stats.chat_lines_sent, 1);
    shell.ui.ui_actions.push("EnterChat".into());
    batch(
        &mut app,
        &mut shell,
        vec![
            key(0xff, None),
            key(0x26, None),
            key(0x58, Some("X")),
            key(0x0d, None),
        ],
    );
    assert_eq!(
        app.objects().world.chat.entries[&8].history(),
        ["@t Peer, H", "@t Peer, HX"]
    );
    assert_eq!(app.interaction().stats.chat_lines_sent, 2);
    shell.ui.ui_actions.push("EnterChat".into());
    batch(
        &mut app,
        &mut shell,
        vec![
            key(0x52, Some("@r")),
            key(0x20, Some(" ")),
            key(0x41, Some("A")),
            key(0x0d, None),
        ],
    );
    assert_eq!(
        app.objects().world.chat.entries[&8]
            .history()
            .last()
            .unwrap(),
        "@t Peer,  A"
    );
    shell.ui.ui_actions.push("EnterChat".into());
    batch(&mut app, &mut shell, vec![key(0xff, None)]);
    shell
        .ui
        .desktop
        .requests
        .push(UiRequest::Select(dereth_primitives::ObjectId(77)));
    shell.open_edit = true;
    let mut events = click(510.0, 110.0);
    events.push(key(0x5a, Some("Z")));
    batch(&mut app, &mut shell, events);
    assert_eq!(app.objects().world.chat.entries[&8].text, "");
    assert_eq!(shell.ui.desktop.focused_control(), Some("other-input"));
    assert!(shell
        .ui
        .desktop
        .requests
        .contains(&UiRequest::Select(dereth_primitives::ObjectId(77))));
    assert_eq!(
        app.objects().world.selected,
        None,
        "non-chat requests retain their queue"
    );
    let mut events = click(60.0, 591.0);
    events.push(key(0x59, Some("Y")));
    batch(&mut app, &mut shell, events);
    assert_eq!(shell.ui.desktop.focused_control(), Some("chat:input"));
    assert_eq!(app.objects().world.chat.entries[&8].text, "Y");
    shell.ui.ui_actions.push("IssueSlashCommand".into());
    batch(&mut app, &mut shell, vec![key(0xff, None)]);
    assert_eq!(
        app.objects().world.chat.entries[&8].text,
        "/",
        "an immediate switch reads the current draft"
    );
    app.shutdown(&mut shell);
}

/// Behaviour: chat.entry-adapters
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn queued_world_escape_and_paper_doll_releases_keep_their_interception() {
    let (mut app, mut shell) = fixture();
    shell.open_edit = true;
    batch(&mut app, &mut shell, vec![]);
    shell.ui.desktop.focus_control("");
    app.probe_mut().objects_mut().world.selected = Some(dereth_primitives::ObjectId(77));
    shell.world_inputs = true;
    batch(&mut app, &mut shell, vec![key(0x1b, None)]);
    assert!(shell.ui.desktop.is_open("test-edit"));
    assert!(shell
        .ui
        .desktop
        .requests
        .contains(&UiRequest::Select(dereth_primitives::ObjectId(0))));
    shell.ui.desktop.requests.clear();
    shell.arm_target = true;
    batch(&mut app, &mut shell, vec![key(0x1b, None)]);
    assert!(shell.ui.leave_target_after_click);
    assert!(shell.ui.desktop.is_open("test-edit"));
    assert!(shell.ui.desktop.requests.is_empty());
    shell.world_inputs = false;
    let mut events = click(510.0, 140.0);
    events.push(key(0xff, None));
    batch(&mut app, &mut shell, events);
    assert!(!shell
        .ui
        .desktop
        .requests
        .contains(&UiRequest::Select(dereth_primitives::ObjectId(88))));
    assert!(shell
        .ui
        .callbacks
        .iter()
        .any(|(_, event)| matches!(event, ControlEvent::PreviewHit { .. })));
    app.shutdown(&mut shell);
}

fn release(vk: usize) -> HostEvent {
    let HostEvent::KeyboardInput { key, .. } = key(vk, None) else {
        unreachable!()
    };
    HostEvent::KeyboardInput {
        key,
        pressed: false,
        text: None,
    }
}

fn raw_batch(app: &mut App<TestShell>, shell: &mut TestShell, events: Vec<HostEvent>) {
    shell.events = events;
    assert!(app.frame(shell));
    assert!(shell.events.is_empty());
}

/// Behaviour: chat.entry-adapters
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn mapped_chat_toggle_recall_and_capture_keep_physical_messages_in_order() {
    use dereth_client_contract::actions::{dereth as own, movement};
    let (mut app, mut shell) = fixture();
    // A held world key must stop even after a later key gives the chat bar focus.
    raw_batch(
        &mut app,
        &mut shell,
        vec![
            key(0x57, None),
            key(0x0d, Some("\r")),
            release(0x0d),
            release(0x57),
            key(0x48, Some("H")),
            release(0x48),
            key(0x0d, Some("\r")),
            release(0x0d),
        ],
    );
    assert_eq!(app.objects().world.chat.entries[&8].history(), ["H"]);
    assert_eq!(app.interaction().stats.chat_lines_sent, 1);
    assert!(!shell.ui.chat_focused(), "submission does not reopen chat");
    let edges: Vec<_> = shell
        .mapped
        .iter()
        .filter(|e| e.action == movement::MOVE_FORWARD)
        .map(|e| e.start)
        .collect();
    assert_eq!(edges, [true, false]);

    // Ctrl-R remains reachable through the focused text barrier; its release is not another recall.
    raw_batch(
        &mut app,
        &mut shell,
        vec![
            key(0x0d, Some("\r")),
            release(0x0d),
            key(0xa2, None),
            key(0x52, Some("\u{12}")),
            release(0x52),
            release(0xa2),
        ],
    );
    assert_eq!(app.objects().world.chat.entries[&8].text, "H");
    assert_eq!(
        shell
            .mapped
            .iter()
            .filter(|e| e.action == own::REPEAT_LAST_MESSAGE && e.start)
            .count(),
        1
    );
    shell.mapped.clear();
    batch(
        &mut app,
        &mut shell,
        vec![key(0x09, Some("\t")), key(0x57, None)],
    );
    assert!(
        !shell.ui.chat_focused(),
        "Tab closes rather than cycling away and reopening"
    );
    assert_eq!(app.objects().world.chat.entries[&8].text, "H");
    assert_eq!(
        shell
            .mapped
            .iter()
            .filter(|e| e.action == movement::MOVE_FORWARD)
            .map(|e| e.start)
            .collect::<Vec<_>>(),
        [true, false]
    );

    // The editor takes a new key exclusively, but cannot swallow a previously held key's end.
    shell.ui.bindings = Some(crate::keybindings::KeyBindings::new(&Default::default()));
    raw_batch(&mut app, &mut shell, vec![key(0x57, None)]);
    shell
        .ui
        .bindings
        .as_mut()
        .unwrap()
        .handle(&HostAction::CaptureBinding {
            action: movement::MOVE_FORWARD.0,
            map: 4,
            slot: 0,
        })
        .unwrap();
    shell.mapped.clear();
    raw_batch(
        &mut app,
        &mut shell,
        vec![release(0x57), key(0x56, None), release(0x56)],
    );
    assert_eq!(
        shell
            .mapped
            .iter()
            .filter(|e| e.action == movement::MOVE_FORWARD)
            .map(|e| e.start)
            .collect::<Vec<_>>(),
        [false]
    );
    assert!(shell
        .captured
        .iter()
        .any(|hit| hit.control.offset() == 0x2f));
    assert!(shell.ui.take_key_store_requests().iter().any(|request| matches!(request,
        crate::keystore::KeyStoreRequest::Bind { scan: 0x2f, action, .. } if *action == movement::MOVE_FORWARD.0
    )));
    app.shutdown(&mut shell);
}

/// Behaviour: chat.target-sweep
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "requires retail DATs")]
fn app_installs_the_chat_target_sweep_with_classic_or_no_interface() {
    for no_ui in [false, true] {
        let (mut app, mut shell) = fixture();
        shell.no_ui = no_ui;
        let id = dereth_primitives::ObjectId(2);
        shell.nearby = Some(id);
        let mut peer = dereth_client_model::Weenie::new(id);
        peer.pwd.name = "Nearby Peer".into();
        peer.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
        app.probe_mut()
            .objects_mut()
            .world
            .tables
            .weenies
            .insert(id, peer);
        app.probe_mut().objects_mut().world.selected = Some(id);
        batch(&mut app, &mut shell, vec![]);
        assert_eq!(app.objects().world.chat.last_speakable_target, Some(id));
        assert!(app
            .objects()
            .world
            .chat
            .is_talk_focus_enabled(dereth_client_model::chat::TalkFocus::Selected));
        assert_eq!(shell.ui.desktop.is_open("hud"), !no_ui);
        app.shutdown(&mut shell);
    }
}

/// Behaviour: options.classic.resolution-follows-the-live-display
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn classic_resolution_label_tracks_the_world_resize_after_pregame() {
    let (mut app, mut shell) = fixture();
    dereth_client_contract::options::store::init();
    app.use_forced_resolution = false;
    app.register_display_modes();
    shell.ui.start(&mut app.ui_context()).unwrap();
    assert_eq!(
        shell.ui.settings.resolutions[shell.ui.settings.resolution],
        (800, 600)
    );
    shell.in_world = true;
    app.cfg.width = 1024;
    app.cfg.height = 768;
    app.applied_resolution = (800, 600);
    assert!(app.frame(&mut shell));
    assert_eq!(app.present.size(), (1024, 768));
    let view = app.hud.view(&app.objects);
    let c = Context {
        resources: &shell.ui.resources,
        layout: shell.ui.desktop.layout(),
        now: dereth_primitives::LocalTime(0.0),
        game: &view,
        pregame: &app.host_state,
        keyboard: &shell.ui.keyboard,
        settings: &shell.ui.settings,
        map_teleport_allowed: false,
        classic: &shell.ui.classic,
    };
    let mut panel = crate::panels::factory(
        "sound-graphics",
        dereth_primitives::LocalTime(0.0),
        &shell.ui.resources,
    )
    .unwrap();
    panel.event(ControlEvent::Tick, &c);
    let frame = panel.frame(&c);
    let row = frame
        .controls
        .iter()
        .find(|c| c.id == "row:Display.Resolution")
        .unwrap();
    let crate::panels::ControlKind::Choice { options, selected } = &row.kind else {
        panic!()
    };
    assert_eq!(options[*selected], "1024 x 768");
    assert!(
        !app.resolution.pending(),
        "ordinary login resize does not start a confirmation"
    );
}

/// Behaviour: presentation.resolution.shared-transaction
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn classic_resolution_draft_survives_idle_frames_and_rejected_choice_reads_back_once() {
    use dereth_client_contract::{options::store, resolution::ResolutionAction};
    let (mut app, mut shell) = fixture();
    store::init();
    shell.in_world = true;
    app.use_forced_resolution = false;
    app.cfg.width = 800;
    app.cfg.height = 600;
    app.applied_resolution = (800, 600);
    app.cfg.display.full_screen = false;
    app.pump.state.full_screen = false;
    assert!(app.window.display_modes().is_empty());
    app.register_display_modes();
    store::set_value(
        "Display.FullScreen",
        dereth_client_contract::PrefValue::Bool(false),
    );
    shell.ui.start(&mut app.ui_context()).unwrap();
    assert!(shell.ui.settings.resolutions.contains(&(800, 600)));
    assert!(shell.ui.settings.resolutions.contains(&(1024, 768)));
    assert_eq!(shell.ui.settings.resolution, 0);
    let mut panel = crate::panels::factory(
        "sound-graphics",
        dereth_primitives::LocalTime(0.0),
        &shell.ui.resources,
    )
    .unwrap();
    let event =
        |shell: &TestShell, app: &App<TestShell>, panel: &mut dyn crate::panels::Panel, e| {
            let view = app.hud.view(&app.objects);
            let c = Context {
                resources: &shell.ui.resources,
                layout: shell.ui.desktop.layout(),
                now: dereth_primitives::LocalTime(0.0),
                game: &view,
                pregame: &app.host_state,
                keyboard: &shell.ui.keyboard,
                settings: &shell.ui.settings,
                map_teleport_allowed: false,
                classic: &shell.ui.classic,
            };
            panel.event(e, &c)
        };
    event(
        &shell,
        &app,
        &mut *panel,
        ControlEvent::Select {
            id: "row:Display.Resolution".into(),
            index: 1,
        },
    );
    shell.ui.sync_settings(&mut app.ui_context()).unwrap();
    let requests = event(
        &shell,
        &app,
        &mut *panel,
        ControlEvent::Activate("apply".into()),
    );
    let chosen = requests
        .into_iter()
        .find_map(|r| {
            if let PanelAction::Host(HostAction::ApplyClassicSettings(s)) = r {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        chosen.resolution, 1,
        "an idle frame cannot erase the uncommitted picker"
    );
    assert!(!chosen.full_screen);
    shell
        .ui
        .host_action(
            &mut app.ui_context(),
            0,
            HostAction::ApplyClassicSettings(chosen),
        )
        .unwrap();
    assert!(app.frame(&mut shell));
    let offer = app.resolution.prompt().unwrap();
    app.submit_requests(vec![UiRequest::Resolution(ResolutionAction::Answer {
        token: offer.token,
        yes: true,
    })]);
    assert!(app.frame(&mut shell));
    assert_eq!(app.present.size(), (1024, 768));
    let accept = app.resolution.prompt().unwrap();
    app.submit_requests(vec![UiRequest::Resolution(ResolutionAction::Answer {
        token: accept.token,
        yes: false,
    })]);
    assert!(app.frame(&mut shell));
    shell.ui.sync_settings(&mut app.ui_context()).unwrap();
    assert_eq!(shell.ui.settings.resolution, 0);
    assert_eq!(app.present.size(), (800, 600));
    event(
        &shell,
        &app,
        &mut *panel,
        ControlEvent::Check {
            id: "row:Render.AutomaticDegrades".into(),
            checked: false,
        },
    );
    let requests = event(
        &shell,
        &app,
        &mut *panel,
        ControlEvent::Activate("apply".into()),
    );
    for r in requests {
        if let PanelAction::Host(action) = r {
            shell
                .ui
                .host_action(&mut app.ui_context(), 0, action)
                .unwrap();
        }
    }
    assert!(app.frame(&mut shell));
    assert!(
        !app.resolution.pending(),
        "applying another setting cannot resubmit the rejected size"
    );
    shell.ui.settings.resolution = 1;
    shell.ui.sync_settings(&mut app.ui_context()).unwrap();
    assert_eq!(
        shell.ui.settings.resolution, 1,
        "one completed transaction is read back once"
    );
}

/// Behaviour: feedback.classic.explicit-emphasis-survives-identical-panel-text
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn actual_abuse_feedback_survives_an_identical_ordinary_line_and_controls_color_and_audio() {
    use dereth_client_contract::feedback::Feedback;
    let (mut app, mut shell) = fixture();
    let body = "Please specify the character to log.";
    for (name, expected, color) in [
        ("", Feedback::INFORMATION, 0xffd2d2c8),
        ("   ", Feedback::WARNING, 0xffffff00),
        ("", Feedback::INFORMATION, 0xffffffff),
    ] {
        let actions = {
            let cx = app.ui_context();
            let view = cx.hud().view(cx.objects());
            let context = Context {
                resources: &shell.ui.resources,
                layout: shell.ui.desktop.layout(),
                now: dereth_primitives::LocalTime(0.0),
                game: &view,
                pregame: cx.pregame(),
                keyboard: &shell.ui.keyboard,
                settings: &shell.ui.settings,
                map_teleport_allowed: false,
                classic: &shell.ui.classic,
            };
            let mut panel = crate::panels::factory(
                "abuse",
                dereth_primitives::LocalTime(0.0),
                &shell.ui.resources,
            )
            .unwrap();
            panel.event(ControlEvent::Activate("begin".into()), &context);
            panel.event(
                ControlEvent::Edit {
                    id: "name".into(),
                    text: name.into(),
                },
                &context,
            );
            panel.event(
                ControlEvent::Edit {
                    id: "text".into(),
                    text: "reason".into(),
                },
                &context,
            );
            panel.event(ControlEvent::Activate("send".into()), &context)
        };
        for action in actions {
            let PanelAction::Host(action) = action else {
                panic!("unexpected panel action")
            };
            shell
                .ui
                .host_action(&mut app.ui_context(), 0, action)
                .unwrap();
        }
        app.ui_context().add_scroll_line(body, 0x1a);
        assert!(app.frame(&mut shell));
        let queued = app.ui_context().hud().panels.transient.clone();
        assert_eq!(
            queued,
            [(body.into(), expected)],
            "ordinary same-text event is not a viewport callback"
        );
        shell.ui.ui_frame(
            &mut app.ui_context(),
            dereth_primitives::LocalTime(1.0),
            Default::default(),
        );
        let commands = shell.ui.overlay.commands(800, false, false, |s| {
            i32::try_from(s.len()).unwrap_or(i32::MAX)
        });
        assert!(commands.iter().any(|c| matches!(c, crate::Command::Text {text,color:c,..} if text == body && *c == color)), "{commands:?}");
        assert!(app.ui_context().hud().panels.transient.is_empty());
    }
    shell.ui.settings.interface = true;
    let sound = UiRequest::PlaySound {
        file: DataId(0x2000004b),
        sound_type: 0x6e,
    };
    assert_eq!(
        shell
            .ui
            .present_feedback("dynamic warning", Feedback::WARNING, 2.0),
        Some(sound.clone())
    );
    assert_eq!(
        shell
            .ui
            .present_feedback("replacement same tick", Feedback::WARNING, 2.0),
        None
    );
    shell.ui.settings.interface = false;
    assert_eq!(
        shell
            .ui
            .present_feedback("muted warning", Feedback::WARNING, 3.0),
        None
    );
    shell.ui.settings.interface = true;
    assert_eq!(
        shell
            .ui
            .present_feedback("same muted tick", Feedback::WARNING, 3.0),
        None
    );
    assert_eq!(
        shell
            .ui
            .present_feedback("new warning", Feedback::WARNING, 4.0),
        Some(sound)
    );
    assert_eq!(
        shell
            .ui
            .present_feedback("information", Feedback::INFORMATION, 5.0),
        None
    );
}

/// Behaviour: none (actual connection-panel startup and quit delivery).
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn pending_connection_opens_progress_and_its_first_cancel_click_quits() {
    let (mut app, mut shell) = fixture();
    dereth_client_contract::options::store::init();
    app.cfg.connect = true;
    app.host_state.has_packet_controller = true;
    shell.ui.start(&mut app.ui_context()).unwrap();
    assert!(shell.ui.desktop.is_open("startup"));
    assert!(!shell.ui.desktop.is_open("login"));
    shell.ui.window_input(
        &mut app.ui_context(),
        &click(742.0, 50.0),
        &mut EmptyClipboard,
    );
    shell.ui.process_inputs(
        &mut app.ui_context(),
        dereth_primitives::LocalTime(0.0),
        false,
    );
    shell
        .ui
        .carry_out_requests(&mut app.ui_context(), dereth_primitives::LocalTime(0.0));
    assert!(
        !app.frame(&mut shell),
        "Cancel terminates the actual App while connection is pending"
    );
}
