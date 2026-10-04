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
    open_edit: bool,
    world_inputs: bool,
    arm_target: bool,
    pub(super) no_ui: bool,
    nearby: Option<dereth_primitives::ObjectId>,
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
        self.ui
            .window_input(cx, &std::mem::take(&mut self.events), &mut EmptyClipboard);
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
        art,
        crate::art::ClassicPaths {
            portal_dir: path.parent().map(ToOwned::to_owned),
            state: std::env::temp_dir().join("dereth-chat-keyboard-no-writes"),
        },
        |id| {
            if id == "test-edit" {
                Some(Box::new(TestEdit))
            } else {
                crate::panels::factory(id)
            }
        },
        (800, 600),
    );
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
            open_edit: false,
            world_inputs: false,
            arm_target: false,
            no_ui: false,
            nearby: None,
        },
    )
}
fn key(vk: usize, text: Option<&str>) -> HostEvent {
    HostEvent::KeyboardInput {
        key: Key::new(vk, 0),
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
    shell.events = events;
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
    app.objects_mut().world.chat.last_teller_name = "Peer".into();
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
    app.objects_mut().world.selected = Some(dereth_primitives::ObjectId(77));
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
        peer.pwd.obj_type = dereth_client_model::weenie::item_type::CREATURE;
        app.objects_mut().world.tables.weenies.insert(id, peer);
        app.objects_mut().world.selected = Some(id);
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
