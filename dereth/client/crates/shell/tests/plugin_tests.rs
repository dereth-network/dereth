//! The plugins under each interface. A plugin's hotkey, its marks in the world, its windows and
//! HUDs work under the modern, the classic and the Horizon interface alike; a file a plugin asks
//! for is asked in a dialog of the interface's own, answered by its buttons; and switching
//! interfaces with plugins loaded leaves nothing of the last interface behind.
//!
//! Fixture: the client at the gameplay screen on the retail dats, on a device-free presentation
//! that builds the world, with a body loaded, the classic and Horizon interfaces brought up beside
//! the modern one (Horizon with no art), and a plugin host with the Luau engine installed as the
//! frame hook. Input is the window's: events queued as the window queues them.

use super::*;
use crate::{app::CoreApp, platform::host::NullHost};
use dereth_client_contract::options::{
    interface::{Interface, INTERFACE},
    store,
};
use dereth_client_contract::PrefValue;
use dereth_input::host::HostEvent;
use dereth_input::keys::{Key, MouseButton};
use dereth_plugin_host::{HostConfig, MemoryStorage, Package, PluginHook, PluginHost};

const CTRL: Key = Key::new(0x11, 0x1D);
const F5: Key = Key::new(0x74, 0x3F);
const ESCAPE: Key = Key::new(0x1B, 0x01);

/// A plugin with a hotkey (Ctrl+F5 runs its `/hk`, which says so) and a marker in the world.
const KEYS: &str = r#"
commands.add("hk", "run by a hotkey", function() print("hotkey") end)
ui.window({ id = "main", title = "Keys", ui.hotkey({ id = "go", command = "hk", key = "Ctrl+F5" }) })
markers.world:add({ position = { ns = 42.25, ew = 33.8 }, label = "Here" })
"#;

/// A plugin with a window holding a button (a click says so) and a HUD with a line, which opens
/// as the player opens the window.
const WINDOWED: &str = r#"
ui.window({ id = "main", title = "Windowed", ui.button({ id = "go", text = "Go" }) })
local hud = ui.hud({ id = "hud", anchor = "bottom_right", ui.label({ text = "On the HUD" }) })
on("ui.clicked", function(e) print("clicked " .. e.id) end)
on("ui.opened", function(e) hud:show() end)
"#;

/// A plugin that asks the player for a file as it loads, and says what it was given.
const PICKER: &str = r#"
task.spawn(function() print("file", pawait(files.import({ prompt = "a meta to run" }))) end)
"#;
const READS: &str = "[permissions]\n\"files.import\" = \"Reads a meta.\"\n";

fn package(id: &str, main: &str, extra: &str) -> Package {
    let manifest = format!(
        "id = \"{id}\"\nname = \"Test {id}\"\nversion = \"1.0.0\"\napi = \"0.6\"\ncontexts = [\"pregame\", \"world\"]\n{extra}"
    );
    Package {
        origin: id.to_owned(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into_iter()
        .collect(),
    }
}

/// A desktop's plugin host over `packages`.
fn plugin_host(packages: Vec<Package>) -> PluginHost {
    use dereth_plugin_api::HostCapability as C;
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            C::Core,
            C::UiWindow,
            C::UiHud,
            C::OverlayWorld,
            C::MapClicks,
            C::RetailUi,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(dereth_plugin_lua::LuauEngine),
        Box::new(dereth_plugin_host::StaticSource { packages }),
        Box::new(MemoryStorage::default()),
    );
    host.load_all();
    host
}

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
    state: std::path::PathBuf,
}

impl Client {
    /// The client at the gameplay screen with a body in the world, the three interfaces up and
    /// the modern one shown.
    fn new(tag: &str) -> Self {
        let state =
            std::env::temp_dir().join(format!("dereth-plugins-{tag}-{}", std::process::id()));
        let cfg = dereth_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| {
                Ok(Box::new(
                    dereth_client_runtime::sim_present::SimPresentation::new(800, 600),
                ))
            },
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
        let mut c = Self { app, shell, state };
        c.install_horizon();
        c.install_classic();
        let files = std::sync::Arc::clone(c.app.probe().dat_store());
        let cfg = dereth_client_runtime::scene::SceneConfig {
            land_radius: 0,
            scenery_radius: 0,
            ..Default::default()
        };
        c.app
            .probe_mut()
            .load_world(&files, cfg)
            .expect("a world with the player's body");
        c.frame();
        c
    }

    fn install_horizon(&mut self) {
        let mut ui = dereth_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(dereth_horizon::art::Art::empty()),
            dereth_horizon::options::HorizonOptions {
                screen: dereth_horizon::options::StartScreen::Game,
                ..Default::default()
            },
            None,
        );
        ui.start(&mut self.app.ui_context());
        self.shell.horizon.ui = Some(ui);
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
        let mut ui = dereth_classic_ui::runtime::ClassicUi::new(
            dereth_classic_ui::resources::Resources::new(
                art,
                Err("World creation tables unavailable".into()),
                None,
            ),
            dereth_classic_ui::art::ClassicPaths {
                state: self.state.join("classic"),
            },
            dereth_classic_ui::panels::factory,
            (800, 600),
        );
        ui.start(&mut self.app.ui_context())
            .expect("the classic interface starts");
        self.shell.classic.ui = Some(ui);
    }

    fn install(&mut self, host: PluginHost) {
        self.app.install_frame_hook(Box::new(PluginHook { host }));
    }

    fn host(&mut self) -> &mut PluginHost {
        &mut self
            .app
            .frame_hook_mut()
            .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
            .expect("the plugin host")
            .host
    }

    fn console(&mut self, id: &str) -> Vec<String> {
        self.host().console(id).unwrap_or_default()
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }

    /// The player opens plugin `id`'s main window from the plugin bar: a plugin's window opens
    /// only when the player asks.
    fn open(&mut self, id: &str) {
        self.host().toggle_window(id).expect("on the plugin bar");
        self.frames(2);
    }

    fn choose(&mut self, interface: Interface) {
        store::set_value(INTERFACE, PrefValue::Int(interface.value()));
        self.frames(2);
        assert_eq!(self.shell.shown_interface(), interface);
    }

    fn event(&mut self, event: HostEvent) {
        self.shell.queue_window_event(event);
    }

    fn key(&mut self, key: Key, pressed: bool) {
        self.event(HostEvent::KeyboardInput {
            key,
            pressed,
            text: None,
        });
    }

    /// Ctrl+F5, pressed and let go, one frame for each.
    fn ctrl_f5(&mut self) {
        self.key(CTRL, true);
        self.key(F5, true);
        self.frame();
        self.key(F5, false);
        self.key(CTRL, false);
        self.frames(2);
    }

    /// A press and a release of the left button at `(x, y)`.
    fn click(&mut self, (x, y): (f64, f64)) {
        self.event(HostEvent::CursorMoved { x, y });
        self.frame();
        for pressed in [true, false] {
            self.event(HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed,
            });
            self.frame();
        }
    }

    /// How many marks the world was last given to draw.
    fn marks(&self) -> u64 {
        self.app
            .present
            .as_any()
            .downcast_ref::<dereth_client_runtime::sim_present::SimPresentation>()
            .expect("the device-free presentation")
            .device()
            .counts()
            .world_marks
    }

    /// Where the plugin host's question is drawn under the shown interface: Horizon's own top
    /// tier under Horizon, the plugins' layer under the others.
    fn asking(&self) -> &dereth_plugin_ui::host::Asking {
        match self.shell.shown_interface() {
            Interface::Horizon => {
                &self
                    .shell
                    .horizon
                    .ui
                    .as_ref()
                    .expect("Horizon")
                    .ui
                    .windows
                    .plugins
                    .host
                    .asking
            }
            _ => &self.shell.plugins.layer.layer.host.asking,
        }
    }

    /// The id of the host's question the shown interface draws.
    fn question_shown(&self) -> Option<u64> {
        self.asking().id()
    }

    /// The interfaces not shown that still ask the host's question.
    fn questions_left_behind(&self) -> Vec<Interface> {
        let shown = self.shell.shown_interface();
        let mut left = Vec::new();
        if shown != Interface::Horizon
            && self
                .shell
                .horizon
                .ui
                .as_ref()
                .and_then(|u| u.host_question())
                .is_some()
        {
            left.push(Interface::Horizon);
        }
        if shown == Interface::Horizon && self.shell.plugins.layer.asking() {
            left.push(Interface::Modern);
        }
        left
    }

    /// The player's press on the first (`yes`) or the last button of the host's question under
    /// the interface shown.
    fn answer(&mut self, yes: bool) {
        let buttons = &self.asking().drawn.buttons;
        let r = if yes {
            buttons[0]
        } else {
            buttons[buttons.len() - 1]
        };
        self.click((f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0)));
        self.frames(3);
    }

    /// The controls the shown interface noted last frame for the pointer and the pad, with the
    /// panels they are in: the plugins' layer's under the modern and the classic interface,
    /// Horizon's own under Horizon.
    fn controls(
        &self,
    ) -> (
        Vec<dereth_ui_kit::nav::Target>,
        Vec<dereth_ui_kit::nav::Layer>,
    ) {
        let snap = match self.shell.shown_interface() {
            Interface::Horizon => {
                &self
                    .shell
                    .horizon
                    .ui
                    .as_ref()
                    .expect("Horizon")
                    .frame_input
                    .nav
                    .last
            }
            _ => &self.shell.plugins.layer.input.nav.last,
        };
        (snap.targets.clone(), snap.layers.clone())
    }

    /// Where the plugin window's button is: the control in the window's panel.
    fn plugin_button(&self) -> dereth_ui_kit::draw::Rect {
        let (targets, layers) = self.controls();
        let at = 1 + layers
            .iter()
            .position(|l| l.name == "Windowed" || l.name.starts_with("Plugin"))
            .expect("the plugin's window is drawn");
        targets
            .iter()
            .find(|t| t.layer == at)
            .map(|t| t.rect)
            .expect("its button is noted")
    }

    fn finish(self) {
        let Self {
            app,
            mut shell,
            state,
        } = self;
        app.shutdown(&mut shell);
        let _ = std::fs::remove_dir_all(&state);
    }
}

/// Behaviour: plugins.interfaces.a-plugins-hotkey-and-marks-work-under-every-interface
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_plugins_hotkey_runs_its_command_and_its_mark_is_drawn_in_the_world_under_every_interface() {
    let mut c = Client::new("keys");
    c.install(plugin_host(vec![package("test.keys", KEYS, "")]));
    c.frames(3);
    for interface in [Interface::Horizon, Interface::Classic, Interface::Modern] {
        c.choose(interface);
        assert_eq!(
            c.marks(),
            1,
            "{interface:?}: the plugin's marker is drawn in the world"
        );
        let before = c.console("test.keys").len();
        c.ctrl_f5();
        assert_eq!(
            c.console("test.keys")[before..],
            ["hotkey"],
            "{interface:?}: Ctrl+F5 ran the plugin's command once"
        );
    }
    c.finish();
}

/// Beside the host's question a plugin's marks in the world are still drawn.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn beside_the_hosts_question_a_plugins_marks_in_the_world_are_still_drawn() {
    let mut c = Client::new("beside");
    c.install(plugin_host(vec![
        package("test.keys", KEYS, ""),
        package("test.picker", PICKER, READS),
    ]));
    c.frames(4);
    let q = c.host().question().expect("test.picker's file");
    assert_eq!(c.marks(), 1, "the mark beside the question");
    c.host().answer_question(q.id, false).expect("answered");
    c.frames(3);
    assert_eq!(c.marks(), 1, "and after it");
    c.finish();
}

/// A plugin with a window holding a button (a press says so), and a command after which it opens
/// a second window of its own accord, a moment later.
const PADDED: &str = r#"
ui.window({ id = "main", title = "Padded", ui.button({ id = "go", text = "Go" }) })
on("ui.clicked", function(e) print("clicked " .. e.id) end)
commands.add("later", "opens a window later", function()
    task.after(50, function()
        ui.window({ id = "later", title = "Later", ui.button({ id = "late", text = "Late" }) })
    end)
end)
"#;

impl Client {
    /// The pad's `buttons` pressed for a frame, then let go for two.
    fn pad(&mut self, buttons: &[dereth_input::pad::PadButton]) {
        let mut state = dereth_input::pad::PadState::default();
        for b in buttons {
            state.set(*b, true);
        }
        self.shell.shared.scripted_pad = Some(state);
        self.frame();
        self.shell.shared.scripted_pad = Some(dereth_input::pad::PadState::default());
        self.frames(2);
    }

    fn window_open(&mut self, window: &str) -> bool {
        self.host()
            .window("test.padded", window)
            .is_some_and(|w| w.open)
    }
}

/// Under the modern and the classic interface, which take no pad themselves, the pad drives the
/// plugins' layer: the d-pad brings a focus to the plugin's window, A presses its button, a
/// window the plugin then declares of its own accord stays closed and does not take the focus,
/// and B closes the window the focus is in.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn under_modern_and_classic_the_pad_moves_a_focus_on_the_plugins_layer_presses_and_closes() {
    use dereth_input::pad::PadButton as B;
    for interface in [Interface::Modern, Interface::Classic] {
        let mut c = Client::new(&format!("pad-{interface:?}"));
        c.install(plugin_host(vec![package("test.padded", PADDED, "")]));
        c.frames(3);
        c.choose(interface);
        c.frames(2);
        assert_eq!(
            c.shell.plugins.layer.pad_focus(),
            None,
            "{interface:?}: no focus before the pad is used, nor any window open"
        );
        c.open("test.padded");
        c.pad(&[B::DPadDown]);
        let focus = c
            .shell
            .plugins
            .layer
            .pad_focus()
            .expect("the d-pad brings the focus in");
        let window = c
            .shell
            .plugins
            .layer
            .layer
            .ui
            .rect_of("test.padded", "main")
            .expect("the window is drawn");
        assert!(
            window.contains(focus.x + 1.0, focus.y + 1.0),
            "{interface:?}: in the plugin's window"
        );
        c.pad(&[B::South]);
        assert_eq!(
            c.console("test.padded").last().map(String::as_str),
            Some("clicked go"),
            "{interface:?}: A presses the button in focus"
        );
        c.host()
            .run_command("test.padded", "later")
            .expect("the plugin's command");
        c.frames(3);
        assert!(
            !c.window_open("later"),
            "{interface:?}: the second window, declared of the plugin's own accord, stays closed"
        );
        assert_eq!(
            c.shell.plugins.layer.pad_focus(),
            Some(focus),
            "{interface:?}: and does not take the focus"
        );
        c.pad(&[B::East]);
        assert!(
            !c.window_open("main"),
            "{interface:?}: B closes the window the focus is in"
        );
        // With no window open, the d-pad brings the focus to the plugin bar, and A there opens the
        // plugin's main window, which, opened by the pad, takes the focus.
        let bar = c.shell.plugins.layer.layer.bar_rect().expect("the bar");
        let on_bar = |c: &Client| {
            c.shell
                .plugins
                .layer
                .pad_focus()
                .is_some_and(|f| bar.contains(f.x + 1.0, f.y + 1.0))
        };
        c.pad(&[B::DPadDown]);
        for _ in 0..3 {
            if on_bar(&c) {
                break;
            }
            c.pad(&[B::Back]);
        }
        assert!(
            on_bar(&c),
            "{interface:?}: the d-pad brings the focus to the bar"
        );
        c.pad(&[B::South]);
        assert!(
            c.window_open("main"),
            "{interface:?}: A on the bar opens it"
        );
        let window = c
            .shell
            .plugins
            .layer
            .layer
            .ui
            .rect_of("test.padded", "main")
            .expect("the window is drawn");
        let focus = c.shell.plugins.layer.pad_focus().expect("a focus");
        assert!(
            window.contains(focus.x + 1.0, focus.y + 1.0),
            "{interface:?}: and the focus goes into the window the pad opened"
        );
        c.finish();
    }
}

/// The game's panels a plugin hides or moves reach the classic interface (hidden there, its panels
/// docked) and Horizon (hidden and moved there), as each has them.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_plugins_hidden_and_moved_panels_reach_the_classic_interface_and_horizon() {
    let mut c = Client::new("retail-ui");
    c.install(plugin_host(vec![package(
        "test.panels",
        "retail_ui.hide('chat') retail_ui.move('radar', 100, 200)",
        "[permissions]
\"retail_ui\" = \"Tidies the screen.\"
",
    )]));
    c.frames(3);
    c.choose(Interface::Classic);
    c.frames(2);
    let classic = &c.shell.classic.ui.as_ref().expect("Classic").classic;
    assert_eq!(
        classic.retail_hidden.iter().collect::<Vec<_>>(),
        ["chat"],
        "the classic chat hidden; the radar, docked, is not moved"
    );
    c.choose(Interface::Horizon);
    c.frames(2);
    let ui = &c.shell.horizon.ui.as_ref().expect("Horizon").ui;
    assert!(
        ui.hud.retail_hidden.contains("chat"),
        "Horizon's log window"
    );
    assert_eq!(ui.hud.layout.pinned.get("minimap"), Some(&(100.0, 200.0)));
    c.finish();
}

/// Under Horizon the plugins' commands are handed to the interface for its cross hotbars, and one
/// run from a slot reaches its plugin as the player's own.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn under_horizon_a_plugins_command_is_offered_to_the_cross_hotbars_and_one_run_from_a_slot_reaches_it(
) {
    let mut c = Client::new("slots");
    c.install(plugin_host(vec![package("test.keys", KEYS, "")]));
    c.frames(3);
    c.choose(Interface::Horizon);
    fn seat(c: &mut Client) -> &mut dereth_horizon::ui::plugins::PluginSeat {
        &mut c
            .shell
            .horizon
            .ui
            .as_mut()
            .expect("Horizon")
            .ui
            .windows
            .plugins
    }
    let offered: Vec<(String, String)> = seat(&mut c)
        .frame
        .commands
        .iter()
        .map(|e| (e.plugin.clone(), e.verb.clone()))
        .collect();
    assert_eq!(offered, [("test.keys".to_owned(), "hk".to_owned())]);
    let before = c.console("test.keys").len();
    seat(&mut c).push_input(dereth_plugin_ui::Input::Command {
        plugin: "test.keys".into(),
        verb: "hk".into(),
    });
    c.frames(3);
    assert_eq!(c.console("test.keys")[before..], ["hotkey"], "it ran once");
    c.finish();
}

/// Behaviour: plugins.files.every-interface-asks-for-a-file-in-a-dialog-of-its-own
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn under_every_interface_a_plugins_file_is_asked_for_in_a_dialog_its_buttons_and_escape_answer() {
    for interface in [Interface::Modern, Interface::Classic, Interface::Horizon] {
        let mut c = Client::new("ask");
        c.choose(interface);
        c.install(plugin_host(vec![
            package("test.first", PICKER, READS),
            package("test.second", PICKER, READS),
        ]));
        c.frames(4);
        let first = c.host().question().expect("the host asks");
        assert_eq!(first.asker, "Test test.first 1.0.0 (test.first)");
        assert_eq!(
            c.question_shown(),
            Some(first.id),
            "{interface:?}: the question is drawn"
        );
        c.answer(true);
        assert_eq!(
            c.question_shown(),
            Some(first.id),
            "{interface:?}: with no file to give, Give it answers nothing"
        );
        c.answer(false);
        assert_eq!(
            c.console("test.first"),
            ["file\tfalse\trefused"],
            "{interface:?}: refused"
        );
        c.frames(2);
        let second = c.host().question().expect("the next plugin's question");
        assert_ne!(second.id, first.id);
        assert_eq!(c.question_shown(), Some(second.id), "{interface:?}");
        // Escape refuses it.
        c.key(ESCAPE, true);
        c.key(ESCAPE, false);
        c.frames(3);
        assert_eq!(
            c.console("test.second"),
            ["file\tfalse\trefused"],
            "{interface:?}: refused by Escape"
        );
        assert_eq!(c.host().question(), None);
        assert_eq!(
            c.question_shown(),
            None,
            "{interface:?}: the question is gone"
        );
        c.finish();
    }
}

/// Behaviour: plugins.interfaces.switching-interfaces-leaves-no-plugin-surface-behind
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn switching_interfaces_with_plugins_loaded_leaves_nothing_of_the_last_interface_behind() {
    let mut c = Client::new("switch");
    c.install(plugin_host(vec![
        package("test.keys", KEYS, ""),
        package("test.picker", PICKER, READS),
    ]));
    c.frames(4);
    // Under the modern interface the host's question is up, in the plugins' layer.
    assert!(c.shell.plugins.layer.asking());
    // Ctrl goes down here and comes up under Horizon: it is not held when Modern comes back.
    c.key(CTRL, true);
    c.frame();
    for interface in [
        Interface::Horizon,
        Interface::Classic,
        Interface::Horizon,
        Interface::Modern,
        Interface::Classic,
    ] {
        c.choose(interface);
        if interface == Interface::Horizon {
            c.key(CTRL, false);
            c.frame();
        }
        assert_eq!(
            c.questions_left_behind(),
            Vec::<Interface>::new(),
            "{interface:?}: no other interface still asks"
        );
        assert_eq!(c.marks(), 1, "{interface:?}: the mark beside the question");
        assert!(c.question_shown().is_some(), "{interface:?}: it asks");
    }
    // Answered in Classic, it is asked nowhere.
    c.answer(false);
    assert_eq!(c.console("test.picker"), ["file\tfalse\trefused"]);
    for interface in [Interface::Horizon, Interface::Modern] {
        c.choose(interface);
        assert_eq!(c.question_shown(), None, "{interface:?}");
        assert_eq!(c.questions_left_behind(), Vec::<Interface>::new());
        assert!(!c.shell.plugins.layer.asking(), "{interface:?}");
    }
    // F5 alone under Modern is no Ctrl+F5: the Ctrl let go under Horizon was followed.
    c.key(F5, true);
    c.key(F5, false);
    c.frames(2);
    assert!(
        !c.console("test.keys").iter().any(|l| l == "hotkey"),
        "{:?}",
        c.console("test.keys")
    );
    c.finish();
}

/// Under the classic interface the host's question stands over the interface's own modal box: a
/// press on the question's button answers it with the box still up.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn under_classic_the_hosts_question_stands_over_the_interfaces_modal_box() {
    let mut c = Client::new("modal");
    c.choose(Interface::Classic);
    c.install(plugin_host(vec![package("test.picker", PICKER, READS)]));
    c.frames(4);
    assert!(c.shell.plugins.layer.asking(), "the host's question is up");
    let ui = c.shell.classic.ui.as_mut().expect("classic");
    ui.desktop.show_dialog(
        "box".into(),
        "A box of the interface's own".into(),
        vec![],
        vec![],
    );
    c.frames(2);
    let ui = c.shell.classic.ui.as_ref().expect("classic");
    assert!(ui.modal_up(), "the interface's modal box is up");
    c.answer(false);
    assert!(c.host().question().is_none(), "the question is answered");
    assert_eq!(c.console("test.picker"), ["file\tfalse\trefused"]);
    c.finish();
}

/// A window's tree changing faster than 30 times a second is shown afresh at most that often, the
/// latest change each time; a window opened or closed is shown at once.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_windows_changes_are_shown_at_most_thirty_times_a_second() {
    use dereth_plugin_host::UiInput;
    let mut c = Client::new("coalesce");
    c.install(plugin_host(vec![package(
        "test.toggled",
        r#"ui.window({ id = "main", title = "Toggled", ui.toggle({ id = "t", text = "On" }) })"#,
        "",
    )]));
    c.frames(3);
    c.open("test.toggled");
    let mut cache = super::plugins::ViewCache::default();
    let seen = |views: Vec<dereth_plugin_ui::WindowView>| {
        views
            .iter()
            .find(|v| v.window == "main")
            .map(|v| (v.revision, v.open))
            .expect("the window")
    };
    let (first, _) = seen(cache.refresh(c.host(), 10.0));
    let flip = |c: &mut Client, on: bool| {
        c.host()
            .ui_input(
                "test.toggled",
                "main",
                UiInput::Change {
                    node: "t".into(),
                    value: dereth_plugin_api::Value::Bool(on),
                },
            )
            .expect("a change");
    };
    flip(&mut c, true);
    assert_eq!(
        seen(cache.refresh(c.host(), 10.01)),
        (first, true),
        "a hundredth of a second on, the tree shown is the last"
    );
    flip(&mut c, false);
    assert_eq!(
        seen(cache.refresh(c.host(), 10.04)),
        (first + 2, true),
        "a thirtieth on, the latest"
    );
    flip(&mut c, true);
    c.host().toggle_window("test.toggled").expect("closed");
    assert_eq!(
        seen(cache.refresh(c.host(), 10.05)),
        (first + 4, false),
        "closed: shown at once"
    );
    c.finish();
}

/// A plugin with a HUD at each of the screen's corners, edges and middle, each holding a button
/// named for its place; a click says which, and `/huds` shows them all.
const OVER: &str = r#"
local huds = {}
for _, at in ipairs({ "top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right" }) do
  table.insert(huds, ui.hud({ id = "hud_" .. at, anchor = at, ui.button({ id = at, text = at, width = 12 }) }))
end
on("ui.clicked", function(e) print("clicked " .. e.id) end)
commands.add("huds", "shows the HUDs", function() for _, h in ipairs(huds) do h:show() end end)
"#;

/// A plugin with a main window it keeps closed: the plugin bar has a button to open it.
const BARRED: &str = r#"
local w = ui.window({ id = "main", title = "Barred", ui.label({ text = "Behind the bar" }) })
w:close()
"#;

/// Under the modern and the classic interface a click on one of the game's own panels or controls
/// is the game's even with a plugin's HUD drawn over it, and a click on a HUD's button where the
/// game has none reaches the plugin; the plugin bar, the client's own, stands over the game's
/// panels and takes the click there.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_click_on_the_games_panels_is_the_games_with_a_plugins_hud_over_them() {
    let mut c = Client::new("over");
    c.install(plugin_host(vec![
        package("test.over", OVER, ""),
        package("test.bar", BARRED, ""),
    ]));
    // The bar where the classic interface's screen has its controls.
    c.host().remember(
        "bar",
        dereth_plugin_api::Value::map()
            .with("x", 400_i64)
            .with("y", 400_i64),
    );
    c.frames(3);
    c.host()
        .run_command("test.over", "huds")
        .expect("the command");
    c.frames(2);
    for interface in [Interface::Modern, Interface::Classic] {
        c.choose(interface);
        c.frames(2);
        let huds: Vec<dereth_ui_kit::draw::Rect> = [
            "top_left",
            "top",
            "top_right",
            "left",
            "center",
            "right",
            "bottom_left",
            "bottom",
            "bottom_right",
        ]
        .iter()
        .filter_map(|at| {
            c.shell
                .plugins
                .layer
                .layer
                .ui
                .rect_of("test.over", &format!("hud_{at}"))
        })
        .collect();
        assert_eq!(huds.len(), 9, "{interface:?}: a button on each HUD");
        // What is under a place: one of the game's own panels or controls (`Some(true)`: under
        // the modern interface its chat window, toolbar, radar or vitals, under the classic one
        // any of its panels or controls), or nothing of the game's (`Some(false)`: the world
        // under the modern interface).
        let game_at = |c: &Client, (x, y): (f32, f32)| match interface {
            Interface::Classic => Some(
                c.shell
                    .classic
                    .ui
                    .as_ref()
                    .expect("classic")
                    .game_control_at(
                        dereth_primitives::num::to_i32(x),
                        dereth_primitives::num::to_i32(y),
                    ),
            ),
            _ => {
                let ui = &c.shell.modern.ui.as_ref().expect("UI").ui;
                let hit = ui.hit_test_screen(
                    dereth_primitives::num::to_i32(x),
                    dereth_primitives::num::to_i32(y),
                );
                let mut at = hit;
                let mut ids = Vec::new();
                while let Some(h) = at {
                    ids.push(ui.node(h).map_or(0, |n| n.element_id().0));
                    at = ui.parent(h);
                }
                if [0x1000_0601, 0x1000_0611, 0x1000_06D2, 0x1000_05FA]
                    .iter()
                    .any(|id| ids.contains(id))
                {
                    Some(true)
                } else if ids.first() == Some(&0x1000_049A) {
                    Some(false)
                } else {
                    None
                }
            }
        };
        let centres: Vec<(f32, f32)> = huds
            .iter()
            .map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
            .collect();
        let over_game = *centres
            .iter()
            .find(|p| game_at(&c, **p) == Some(true))
            .expect("a HUD over one of the game's controls");
        let over_nothing = *centres
            .iter()
            .find(|p| game_at(&c, **p) == Some(false))
            .expect("a HUD where the game has none");
        let before = c.console("test.over").len();
        c.click((f64::from(over_game.0), f64::from(over_game.1)));
        c.frames(2);
        assert_eq!(
            c.console("test.over")[before..],
            [] as [String; 0],
            "{interface:?}: the click over the game's control is the game's"
        );
        c.click((f64::from(over_nothing.0), f64::from(over_nothing.1)));
        c.frames(2);
        let heard = c.console("test.over")[before..].to_vec();
        assert!(
            heard.len() == 1 && heard[0].starts_with("clicked "),
            "{interface:?}: the click where the game has nothing is the plugin's: {heard:?}"
        );
        let bar = c
            .shell
            .plugins
            .layer
            .layer
            .bar_rect()
            .expect("the plugin bar");
        // The plugin's button, short of the grip at the bar's right end.
        let button = (bar.right() - 20.0, bar.y + bar.h / 2.0);
        if interface == Interface::Classic {
            assert_eq!(
                game_at(&c, button),
                Some(true),
                "the bar over one of the screen's controls"
            );
        }
        c.click((f64::from(button.0), f64::from(button.1)));
        c.frames(2);
        let open = |c: &mut Client| c.host().window("test.bar", "main").is_some_and(|w| w.open);
        assert!(
            open(&mut c),
            "{interface:?}: the bar's button opened the window"
        );
        c.host().toggle_window("test.bar").expect("closed again");
        c.frames(2);
        assert!(!open(&mut c));
    }
    c.finish();
}

/// Behaviour: plugins.interfaces.a-plugins-windows-and-huds-are-drawn-under-every-interface
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_plugins_window_and_hud_are_drawn_under_every_interface_and_a_click_on_its_button_reaches_it() {
    let mut c = Client::new("windows");
    c.install(plugin_host(vec![package("test.win", WINDOWED, "")]));
    c.frames(3);
    c.open("test.win");
    for interface in [
        Interface::Modern,
        Interface::Classic,
        Interface::Horizon,
        Interface::Modern,
    ] {
        c.choose(interface);
        c.frames(2);
        match interface {
            Interface::Horizon => {
                let ui = &c.shell.horizon.ui.as_ref().expect("Horizon").ui;
                assert!(
                    ui.windows
                        .is_open(dereth_horizon::ui::panels::WindowId::Plugin(0)),
                    "Horizon seats the window among its own"
                );
                assert!(
                    ui.windows
                        .plugins
                        .frame
                        .views
                        .iter()
                        .any(|v| v.window == "main" && v.by_player),
                    "opened by the player from the plugin bar"
                );
                assert!(
                    !ui.windows.plugins.hits.is_empty(),
                    "and the HUD and the bar among its HUD's elements"
                );
            }
            _ => {
                let drawn = c.shell.plugins.layer.layer.drawn();
                assert_eq!(drawn.windows.len(), 1, "{interface:?}: the window is drawn");
                assert!(
                    !c.shell.plugins.layer.slices.under.is_empty(),
                    "{interface:?}: the HUD and the bar are drawn"
                );
            }
        }
        let go = c.plugin_button();
        let before = c.console("test.win").len();
        c.click((f64::from(go.x + go.w / 2.0), f64::from(go.y + go.h / 2.0)));
        c.frames(2);
        assert_eq!(
            c.console("test.win")[before..],
            ["clicked go"],
            "{interface:?}: the click reached the plugin, once"
        );
    }
    c.finish();
}

/// An element of the modern interface's own over `r`, taking the pointer: one of its windows.
fn a_window_of_the_interface(
    ui: &mut dereth_ui::UiSystem,
    r: dereth_ui_kit::draw::Rect,
) -> dereth_ui::ElemHandle {
    struct NoAssets;
    impl dereth_primitives::AssetSource for NoAssets {
        fn read(
            &self,
            id: dereth_primitives::DataId,
        ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            Err(dereth_primitives::AssetError::NotFound(id))
        }
        fn exists(&self, _: dereth_primitives::DataId) -> bool {
            false
        }
        fn iter_type(
            &self,
            _: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
            Box::new(std::iter::empty())
        }
    }
    let desc = dereth_ui::ElementDesc {
        base: dereth_ui::StateDesc {
            incorporation: dereth_ui::desc::incorporation::LEGACY_ALL_GEOMETRY,
            width: 1,
            height: 1,
            ..dereth_ui::StateDesc::default()
        },
        element_id: dereth_ui::ElementId(0x7D00_0001),
        ty: dereth_ui::factory::ty::FIELD,
        ..dereth_ui::ElementDesc::default()
    };
    let layout = dereth_ui::LayoutDesc {
        did: dereth_primitives::DataId(0),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((desc.element_id, desc.clone())).collect(),
    };
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &desc)
        .expect("an element")
        .expect("an element");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    ui.initialize_tree(h);
    ui.set_mouse_visible(h, true);
    if let Some(n) = ui.node_mut(h) {
        n.region.surface_fills.push(dereth_ui::UiFill {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: 0xFF00_0000,
        });
    }
    let to = |v: f32| dereth_primitives::num::to_i32(v);
    ui.move_to(h, to(r.x), to(r.y));
    ui.resize_to(h, to(r.w), to(r.h));
    h
}

/// Behaviour: plugins.interfaces.under-modern-a-plugin-window-stands-among-the-interfaces-windows
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn under_modern_a_plugin_window_stands_among_the_interface_s_windows_and_a_press_brings_it_forward()
{
    let mut c = Client::new("interleave");
    c.install(plugin_host(vec![package("test.win", WINDOWED, "")]));
    c.frames(4);
    c.open("test.win");
    let go = c.plugin_button();
    let window = c.shell.plugins.layer.layer.drawn().windows[0].1;
    let centre = (f64::from(go.x + go.w / 2.0), f64::from(go.y + go.h / 2.0));
    // One of the interface's own windows opens over the plugin's button.
    let cover = a_window_of_the_interface(
        &mut c.shell.modern.ui.as_mut().expect("UI").ui,
        go.inset(-4.0),
    );
    c.frames(2);
    let place = |c: &Client, h| {
        let cmds = &c.shell.shared.ui_draw_list;
        (
            cmds.iter().position(|cmd| cmd.who == h),
            c.shell
                .plugins
                .layer
                .modern_inserts(cmds)
                .last()
                .map(|(i, _)| *i),
        )
    };
    let (at_cover, at_window) = place(&c, cover);
    assert!(
        at_cover.zip(at_window).is_some_and(|(c, w)| w <= c),
        "the window that opened later is drawn over the plugin's: {at_cover:?}, {at_window:?}"
    );
    let before = c.console("test.win").len();
    c.click(centre);
    c.frames(2);
    assert!(
        c.console("test.win")[before..].is_empty(),
        "the interface's window over it takes the press"
    );
    // A press on the part of the plugin window nothing covers (its title bar) brings it in front
    // of that window.
    c.click((f64::from(window.x + 4.0), f64::from(window.y + 4.0)));
    c.frames(2);
    let (at_cover, at_window) = place(&c, cover);
    assert!(
        at_cover.zip(at_window).is_some_and(|(c, w)| w > c),
        "brought forward, the plugin's window is drawn over it: {at_cover:?}, {at_window:?}"
    );
    c.click(centre);
    c.frames(2);
    assert_eq!(
        c.console("test.win")[before..],
        ["clicked go"],
        "in front, its button takes the press"
    );
    c.finish();
}

/// Behaviour: plugins.interfaces.switching-interfaces-leaves-no-plugin-surface-behind
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn switching_away_from_the_modern_interface_takes_its_plugin_stand_ins_and_the_layer_with_it() {
    let mut c = Client::new("unseat");
    let seats = |c: &Client| {
        let ui = &c.shell.modern.ui.as_ref().expect("UI").ui;
        ui.children(ui.root())
            .into_iter()
            .filter(|h| {
                ui.node(*h)
                    .is_some_and(|n| n.element_id().0 & 0xFF00_0000 == 0x7E00_0000)
            })
            .count()
    };
    assert_eq!(
        seats(&c),
        0,
        "with nothing of the plugins' to draw, no stand-in"
    );
    c.install(plugin_host(vec![package("test.win", WINDOWED, "")]));
    c.frames(4);
    c.open("test.win");
    assert_eq!(seats(&c), 2, "the window's stand-in and the HUDs'");
    for interface in [Interface::Horizon, Interface::Classic] {
        c.choose(interface);
        assert_eq!(
            seats(&c),
            0,
            "{interface:?}: none is left among the modern interface's"
        );
        if interface == Interface::Horizon {
            assert!(
                c.shell.plugins.layer.slices.labels.is_empty()
                    && c.shell.plugins.layer.slices.over_labels().is_empty(),
                "the layer draws nothing under Horizon"
            );
        }
    }
    c.choose(Interface::Modern);
    c.frames(2);
    assert_eq!(
        seats(&c),
        2,
        "seated again when the modern interface is back"
    );
    c.finish();
}

/// A plugin that says each press on the game's map: the button and where it fell.
const MAPPED: &str = r#"
on("map.clicked", function(e) print(e.button, e.position.ns, e.position.ew) end)
"#;

/// How an interface places a mark on its map: the box of the modern map page's picture on the
/// screen, the classic map page's own projection, or where Horizon's picture is drawn and how many
/// screen pixels a side one of its pixels is.
#[derive(Clone, Copy)]
enum MapPlacing {
    Modern(dereth_ui::Box2D),
    Classic,
    Horizon(dereth_ui_kit::draw::Rect, f32),
}

impl Client {
    /// The pixel a mark at `ns`, `ew` is drawn on: a screen pixel under the modern and the classic
    /// interface, a pixel of the picture under Horizon, which scales it.
    #[allow(clippy::cast_possible_truncation)]
    fn map_pixel(&self, placing: MapPlacing, ns: f64, ew: f64) -> (i32, i32) {
        use dereth_ui_screens::mapradar::map::{place_marker_on_map, SHIPPED_NOTE_BINDING};
        let (mx, my) = place_marker_on_map(SHIPPED_NOTE_BINDING.2, ew as f32, ns as f32, (0, 0));
        match placing {
            MapPlacing::Modern(b) => (b.x0 + mx, b.y0 + my),
            MapPlacing::Classic => self
                .shell
                .classic
                .active()
                .and_then(|ui| ui.map_point(ns, ew))
                .expect("the place is on the classic map"),
            MapPlacing::Horizon(..) => (mx, my),
        }
    }

    /// Where on the screen the middle of `pixel` is.
    #[allow(clippy::cast_precision_loss)]
    fn map_press_point(placing: MapPlacing, (x, y): (i32, i32)) -> (f64, f64) {
        match placing {
            MapPlacing::Horizon(area, fit) => (
                f64::from(area.x + (x as f32 + 0.5) * fit),
                f64::from(area.y + (y as f32 + 0.5) * fit),
            ),
            _ => (f64::from(x) + 0.5, f64::from(y) + 0.5),
        }
    }
}

/// Under each interface a press on the world map reaches the plugin with the place under the
/// pointer: pressed on the pixel Holtburg's mark is drawn on, it reads back Holtburg to within a
/// pixel, and the interface draws a mark at the place told on the pixel pressed.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn a_press_on_each_interfaces_map_reaches_the_plugins_with_the_place_under_the_pointer() {
    use dereth_horizon::ui::panels::WindowId;
    let mut c = Client::new("map");
    c.install(plugin_host(vec![package("test.map", MAPPED, "")]));
    c.frames(3);
    for interface in [Interface::Modern, Interface::Classic, Interface::Horizon] {
        c.choose(interface);
        let placing = match interface {
            Interface::Modern => {
                // The toolbar's map page brought up, as its button brings it.
                let shell = c.shell.modern.ui.as_mut().expect("the modern interface");
                let screen = crate::hud_drive::game_screen(&mut shell.flow).expect("gameplay");
                let any: &mut dyn std::any::Any = screen;
                let screen = any
                    .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
                    .expect("the gameplay screen");
                let panel = screen
                    .panels
                    .pages
                    .iter()
                    .find(|p| p.element == dereth_ui_screens::panels::house::PAGE)
                    .map(|p| p.panel_id)
                    .expect("the map's page on the toolbar");
                screen.recv_set_panel_visibility(&mut shell.ui, panel, true);
                c.frames(3);
                let ui = &c.shell.modern.ui.as_ref().expect("the modern interface").ui;
                let image = ui
                    .get_child_recursive(
                        ui.root(),
                        dereth_ui_screens::mapradar::map::child::MAP_IMAGE,
                    )
                    .filter(|h| ui.is_visible(*h))
                    .expect("the map page is open");
                MapPlacing::Modern(ui.screen_box(image))
            }
            Interface::Classic => {
                // The map page opened, as its key opens it in the world (with no server, the
                // classic interface is not in the world here).
                let ui = c.shell.classic.ui.as_mut().expect("the classic interface");
                let cx = c.app.ui_context();
                let view = cx.hud().view(cx.objects());
                let context = dereth_classic_ui::panels::Context {
                    resources: &ui.resources,
                    layout: ui.desktop.layout(),
                    now: dereth_primitives::LocalTime(0.0),
                    game: &view,
                    pregame: cx.pregame(),
                    keyboard: &ui.keyboard,
                    settings: &ui.settings,
                    map_teleport_allowed: false,
                    classic: &ui.classic,
                };
                ui.desktop.open("map", &context);
                c.frames(3);
                MapPlacing::Classic
            }
            _ => {
                let ui = c.shell.horizon.ui.as_mut().expect("Horizon");
                ui.ui.windows.open(WindowId::Map, 0.0);
                c.frames(3);
                let ui = c.shell.horizon.ui.as_ref().expect("Horizon");
                let area = ui.ui.windows.map_picture.expect("the map window is open");
                MapPlacing::Horizon(area, area.w / 257.0)
            }
        };
        let pixel = c.map_pixel(placing, 42.2, 33.8);
        let before = c.console("test.map").len();
        c.click(Client::map_press_point(placing, pixel));
        c.frames(2);
        let said = c.console("test.map")[before..].to_vec();
        let told: Vec<&str> = said
            .first()
            .map(|l| l.split('\t').collect())
            .unwrap_or_default();
        let place = |i: usize| told.get(i).and_then(|v| v.parse::<f64>().ok());
        let (Some(ns), Some(ew)) = (place(1), place(2)) else {
            panic!("{interface:?}: the press told, with a place: {said:?}");
        };
        assert_eq!(
            (said.len(), told[0]),
            (1, "left"),
            "{interface:?}: one left press: {said:?}"
        );
        assert!(
            (ns - 42.2).abs() < 0.85 && (ew - 33.8).abs() < 0.85,
            "{interface:?}: Holtburg to within a pixel: {ns}, {ew}"
        );
        assert_eq!(
            c.map_pixel(placing, ns, ew),
            pixel,
            "{interface:?}: a mark at {ns}, {ew} is drawn on the pixel pressed"
        );
    }
    c.finish();
}
