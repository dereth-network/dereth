//! The host over hand-built frames, with the Luau engine: loading and refusing, events from state
//! changes, chat, commands, timers, the store, windows, faults and the audit answers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::statmgmt::XpHeader;
use dereth_client_contract::view::Vital;
use dereth_client_contract::GameView;
use dereth_client_model::cmd::CommandInterp;
use dereth_client_model::World;
use dereth_client_runtime::frame_hook::{ExtensionCommand, FrameHook, HookFrame};
use dereth_plugin_api::{HostCapability, Limits, Value};
use dereth_plugin_lua::LuauEngine;
use dereth_primitives::ObjectId;

use super::{
    HostConfig, MemoryStorage, Package, PluginHook, PluginHost, PluginStorage, StaticSource,
    UiInput, VmClock,
};

/// A view with a player whose vitals and experience a test sets.
#[derive(Debug, Default)]
struct View {
    player: Option<u32>,
    name: Option<String>,
    health: (u32, u32),
    xp: u64,
}

impl GameView for View {
    fn player(&self) -> Option<ObjectId> {
        self.player.map(ObjectId)
    }
    fn character_name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    fn vital(&self, _id: ObjectId, which: Vital) -> Option<(u32, u32)> {
        (which == Vital::Health && self.player.is_some()).then_some(self.health)
    }
    fn experience_header(&self) -> Option<XpHeader> {
        self.player.map(|_| XpHeader {
            total: self.xp,
            into_level: 0,
            level_span: 0,
            to_level: 0,
            level: 5,
            at_cap: false,
        })
    }
}

/// A storage the test can look into after the host has written to it.
#[derive(Debug, Default, Clone)]
struct Shared(Rc<RefCell<MemoryStorage>>);

impl PluginStorage for Shared {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        self.0.borrow_mut().load(plugin, scope)
    }
    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        self.0.borrow_mut().save(plugin, scope, text);
    }
}

thread_local! {
    /// The player's *Plugins (experimental)* setting, as a switched host reads it: each test runs
    /// on a thread of its own, so each has its own.
    static PLUGINS_ON: Cell<bool> = const { Cell::new(false) };
}

/// The setting, as a switched host's [`HostConfig::switch`] reads it.
fn plugins_setting() -> bool {
    PLUGINS_ON.with(Cell::get)
}

struct Harness {
    host: PluginHost,
    view: View,
    world: World,
    table: CommandInterp,
    time: f64,
    printed: Vec<ChatMessage>,
    hidden: Vec<usize>,
    verbs: Vec<String>,
    storage: Shared,
}

/// A package; active everywhere unless `extra_manifest` names its contexts (most tests here run
/// with no character in the world).
fn package(id: &str, main: &str, extra_manifest: &str) -> Package {
    let contexts = if extra_manifest.contains("contexts") {
        ""
    } else {
        "contexts = [\"world\", \"pregame\", \"chargen\"]\n"
    };
    let manifest = format!("id = \"{id}\"\nname = \"Test {id}\"\nversion = \"1.0.0\"\napi = \"0.1\"\nauthors = [\"A. Tester\"]\n{contexts}{extra_manifest}");
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

impl Harness {
    fn new(packages: Vec<Package>) -> Self {
        Self::new_with(packages, None, Shared::default())
    }

    /// A harness whose host offers `found` as the plugins it found, over `storage` (which a
    /// second harness may share, as a client started again shares its files).
    fn new_with(packages: Vec<Package>, found: Option<Vec<Package>>, storage: Shared) -> Self {
        Self::offering(packages, found, storage, &[])
    }

    /// A harness whose host follows the player's *Plugins (experimental)* setting
    /// ([`PLUGINS_ON`]), over `packages` named to load and `found`.
    fn switched(packages: Vec<Package>, found: Vec<Package>, storage: Shared) -> Self {
        Self::built(packages, Some(found), storage, &[], Some(plugins_setting))
    }

    /// A harness whose host offers `more` capabilities besides its core and windows.
    fn offering(
        packages: Vec<Package>,
        found: Option<Vec<Package>>,
        storage: Shared,
        more: &[HostCapability],
    ) -> Self {
        Self::built(packages, found, storage, more, None)
    }

    fn built(
        packages: Vec<Package>,
        found: Option<Vec<Package>>,
        storage: Shared,
        more: &[HostCapability],
        switch: Option<fn() -> bool>,
    ) -> Self {
        let mut config = HostConfig::new(
            "dereth-test",
            [HostCapability::Core, HostCapability::UiWindow]
                .into_iter()
                .chain(more.iter().copied())
                .collect(),
        );
        config.limits = Limits {
            memory_bytes: 16 << 20,
            call_budget_ms: 50.0,
            profile: false,
        };
        config.switch = switch;
        let mut host = PluginHost::new(
            config,
            Box::new(LuauEngine),
            Box::new(StaticSource { packages }),
            Box::new(storage.clone()),
        );
        if let Some(packages) = found {
            host = host.with_found(Box::new(StaticSource { packages }));
        }
        host.load_all();
        Self {
            host,
            view: View::default(),
            world: World::new(),
            table: CommandInterp::new(),
            time: 0.0,
            printed: Vec::new(),
            hidden: Vec::new(),
            verbs: Vec::new(),
            storage,
        }
    }

    /// One frame of `dt` seconds with these chat lines and typed commands.
    fn frame_with(&mut self, dt: f64, chat: &[ChatMessage], typed: &[ExtensionCommand]) {
        self.time += dt;
        let mut frame = HookFrame::new(
            &self.view,
            &self.world,
            &self.table,
            self.time,
            self.time,
            chat,
            typed,
        );
        self.host.step(&mut frame);
        let out = frame.into_output();
        self.printed.extend(out.printed);
        self.hidden = out.hidden;
        if let Some(v) = out.verbs {
            self.verbs = v;
        }
    }

    fn frame(&mut self) {
        self.frame_with(0.05, &[], &[]);
    }

    fn login(&mut self) {
        self.view.player = Some(0x5000_0001);
        self.view.name = Some("Tester".into());
        self.view.health = (50, 100);
        self.view.xp = 1_000;
        self.frame();
    }

    fn console(&self, id: &str) -> Vec<String> {
        self.host.console(id).unwrap_or_default()
    }

    fn printed_text(&self) -> Vec<String> {
        self.printed.iter().map(|m| m.body.clone()).collect()
    }
}

fn line(ty: u8, text: &str) -> ChatMessage {
    ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty,
        body: text.to_owned(),
        prefix: None,
        window: 0,
    }
}

fn typed(verb: &str, args: &str) -> ExtensionCommand {
    ExtensionCommand {
        verb: verb.to_owned(),
        args: args.to_owned(),
        words: args.split_whitespace().map(str::to_owned).collect(),
    }
}

#[test]
fn a_plugin_loads_at_the_first_frame_and_hears_login_and_vitals() {
    let mut h = Harness::new(vec![package(
        "test.events",
        r#"
        on("session.login", function(e) print("login", e.character) end)
        on("character.vital", function(e) print("vital", e.vital, e.current, e.max, e.previous) end)
        on("session.logout", function() print("logout") end)
        "#,
        "",
    )]);
    h.frame();
    assert_eq!(h.host.plugins()[0].status, "running");
    h.login();
    h.view.health = (40, 100);
    h.frame();
    h.view.player = None;
    h.frame();
    assert_eq!(
        h.console("test.events"),
        ["login\tTester", "vital\thealth\t40\t100\t50", "logout"]
    );
}

#[test]
fn a_plugin_is_active_only_in_its_contexts_and_hears_them_begin_and_end() {
    let main = r#"
        ui.window{ id = "main", title = "W", ui.label{ text = "hi" } }
        task.every(100, function() print("tick") end)
        on("context.begin", function(e)
            -- The character's store is open as soon as a plugin is active in the world.
            if e.context == "world" then store.character.seen = true end
            print("begin", e.context, e.previous)
        end)
        on("context.end", function(e) print("end", e.context, e.next) end)
        on("session.login", function() print("login") end)
        "#;
    let mut h = Harness::new(vec![
        package("test.world", main, "contexts = [\"world\"]\n"),
        package("test.pre", main, "contexts = [\"pregame\"]\n"),
    ]);
    let shown = |h: &Harness| -> Vec<String> {
        h.host
            .windows()
            .into_iter()
            .map(|(p, w, _)| format!("{p}/{w}"))
            .collect()
    };
    let ticks = |h: &Harness, id: &str| h.console(id).iter().filter(|l| *l == "tick").count();
    for _ in 0..6 {
        h.frame();
    }
    assert_eq!(
        shown(&h),
        ["test.pre/main"],
        "only the pre-game plugin shows"
    );
    assert_eq!(
        ticks(&h, "test.world"),
        0,
        "an inactive plugin's timers wait"
    );
    assert!(ticks(&h, "test.pre") > 0);
    // The character's object comes before its name: not yet in the world (its store could not
    // open).
    h.view.player = Some(0x5000_0001);
    h.frame();
    assert_eq!(
        shown(&h),
        ["test.pre/main"],
        "not in the world before the name"
    );
    h.login();
    for _ in 0..6 {
        h.frame();
    }
    assert_eq!(shown(&h), ["test.world/main"]);
    assert!(
        ticks(&h, "test.world") > 0,
        "its timers run once it is active"
    );
    let pre_ticks = ticks(&h, "test.pre");
    h.frame();
    h.frame();
    h.frame();
    assert_eq!(
        ticks(&h, "test.pre"),
        pre_ticks,
        "the pre-game plugin is idle"
    );
    let world: Vec<_> = h
        .console("test.world")
        .into_iter()
        .filter(|l| l != "tick")
        .collect();
    assert_eq!(world, ["begin\tworld\tpregame", "login"]);
    let pre: Vec<_> = h
        .console("test.pre")
        .into_iter()
        .filter(|l| l != "tick")
        .collect();
    assert_eq!(
        pre,
        ["begin\tpregame\tnil", "end\tpregame\tworld", "login"],
        "an inactive plugin still hears the session's comings and goings"
    );
    h.view.player = None;
    h.frame();
    assert_eq!(shown(&h), ["test.pre/main"], "back at the character list");
    let info = h.host.plugins();
    assert!(!info[0].active && info[1].active);
}

#[test]
fn a_manifest_that_needs_more_than_the_host_offers_is_refused_with_the_reason() {
    let newer = Package {
        origin: "newer".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.newer\"
name = \"N\"
version = \"1.0.0\"
api = \"0.9\"
"
                .to_vec(),
            ),
            ("main.lua".to_owned(), Vec::new()),
        ]
        .into_iter()
        .collect(),
    };
    let mut h = Harness::new(vec![
        package("test.plain", "", ""),
        package(
            "test.overlay",
            "",
            "[capabilities]
required = [\"overlay.world\"]
",
        ),
        newer,
    ]);
    h.frame();
    let ids: Vec<_> = h.host.plugins().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, ["test.plain"]);
    let text = h.printed_text().join(
        "
",
    );
    assert!(
        text.contains("test.overlay was not loaded: this host (dereth-test) has no overlay.world"),
        "{text}"
    );
    assert!(
        text.contains("test.newer was not loaded: it needs plugin API 0.9, and this host has 0.7"),
        "{text}"
    );
}

#[test]
fn a_plugin_prints_local_lines_and_filters_incoming_ones() {
    let mut h = Harness::new(vec![package(
        "test.filter",
        r#"
        on("chat.line", function(e)
            if string.find(e.text, "spam") then chat.hide(e.line) end
            if e.kind == "tell" then print("saw a tell") end
        end)
        chat.print("filter ready", "system")
        "#,
        "[permissions]\n\"chat.filter\" = \"Hides spam.\"\n",
    )]);
    h.frame();
    assert_eq!(h.printed_text(), ["filter ready"]);
    assert_eq!(h.printed[0].ty, 5);
    h.frame_with(
        0.05,
        &[
            line(2, "hello"),
            line(2, "buy spam now"),
            line(3, "a private tell"),
        ],
        &[],
    );
    assert_eq!(h.hidden, [1]);
    assert_eq!(
        h.console("test.filter"),
        ["saw a tell"],
        "every plugin hears tells"
    );
}

/// Every plugin may do what each permission covers, whether its manifest names it or not: one
/// that names nothing still hides lines and hears tells.
#[test]
fn a_plugin_that_names_no_permission_still_hides_lines_and_hears_tells() {
    let mut h = Harness::new(vec![package(
        "test.nofilter",
        "on('chat.line', function(e) print(e.kind, e.text) chat.hide(e.line) end)",
        "",
    )]);
    h.frame();
    h.frame_with(0.05, &[line(2, "x"), line(3, "a tell")], &[]);
    assert_eq!(h.hidden.len(), 2, "both hidden: {:?}", h.hidden);
    let log = h.console("test.nofilter").join("\n");
    assert!(log.contains("a tell"), "the tell is heard: {log}");
    assert!(!log.contains("permission"), "{log}");
}

#[test]
fn a_typed_command_reaches_the_plugin_that_claimed_it() {
    let mut h = Harness::new(vec![package(
        "test.cmd",
        "commands.add('xp', 'shows xp', function(args, words) chat.print('xp ' .. args .. ' ' .. #words) end)",
        "",
    )]);
    h.frame();
    assert_eq!(h.verbs, ["xp", "plugin"]);
    h.frame_with(0.05, &[], &[typed("xp", "reset now")]);
    assert_eq!(h.printed_text(), ["xp reset now 2"]);
}

/// The commands plugins claimed are listed with what each does, and one run from the interface's
/// own control (a hotbar slot) reaches its plugin at the next frame as the typed command does,
/// with no words after it; a command no plugin here claims runs nothing.
#[test]
fn a_command_run_from_the_interface_reaches_its_plugin_as_the_typed_one_does() {
    let mut h = Harness::new(vec![package(
        "test.cmd",
        "commands.add('xp', 'shows xp', function(args, words) chat.print('xp [' .. args .. '] ' .. #words) end)",
        "",
    )]);
    h.frame();
    assert_eq!(
        h.host.commands(),
        vec![super::Command {
            plugin: "test.cmd".into(),
            verb: "xp".into(),
            help: "shows xp".into(),
        }]
    );
    assert!(h.host.run_command("test.cmd", "nope").is_err());
    assert!(h.host.run_command("test.other", "xp").is_err());
    h.host.run_command("test.cmd", "XP").expect("its command");
    assert!(h.printed_text().is_empty(), "not before the next frame");
    h.frame();
    assert_eq!(h.printed_text(), ["xp [] 0"]);
}

#[test]
fn a_second_claim_on_a_verb_or_a_claim_on_the_hosts_own_is_refused() {
    let mut h = Harness::new(vec![
        package("test.first", "commands.add('go', '', function() end)", ""),
        package("test.second", "commands.add('go', '', function() end)", ""),
        package(
            "test.third",
            "commands.add('plugin', '', function() end)",
            "",
        ),
    ]);
    h.frame();
    let statuses: Vec<_> = h
        .host
        .plugins()
        .into_iter()
        .map(|p| (p.id, p.status))
        .collect();
    assert_eq!(statuses[0].1, "running");
    assert_eq!(statuses[1].1, "faulted");
    assert_eq!(statuses[2].1, "faulted");
    assert!(h.host.plugins()[1]
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("already claimed by test.first"));
}

#[test]
fn timers_fire_on_the_frame_clock_and_repeat_without_bursting() {
    let mut h = Harness::new(vec![package(
        "test.timer",
        "task.every(100, function() print('tick') end) task.after(250, function() print('once') end)",
        "",
    )]);
    h.frame(); // t = 0.05: loaded; the timers are due at 0.15 and 0.30.
    h.frame_with(0.10, &[], &[]); // 0.15: tick
    h.frame_with(0.20, &[], &[]); // 0.35: tick (one catch-up only), once
    h.frame_with(1.00, &[], &[]); // 1.35: tick (one catch-up only)
    assert_eq!(h.console("test.timer"), ["tick", "tick", "once", "tick"]);
}

#[test]
fn the_character_store_survives_a_logout_and_a_reload() {
    let src = r#"
        on("session.login", function()
            local n = (store.character.logins or 0) + 1
            store.character.logins = n
            print("logins", n)
        end)
    "#;
    let mut h = Harness::new(vec![package("test.store", src, "")]);
    h.frame();
    h.login();
    h.view.player = None;
    h.frame();
    let saved = h.storage.0.borrow().saved.clone();
    assert_eq!(
        saved
            .get(&(
                "test.store".to_owned(),
                "character:unknown::Tester".to_owned()
            ))
            .map(String::as_str),
        Some(r#"{"logins":1}"#)
    );
    h.login();
    h.host.request_reload("test.store");
    h.frame();
    h.view.player = None;
    h.frame();
    h.login();
    assert_eq!(
        h.console("test.store"),
        ["logins\t3"],
        "the console restarts with the reload"
    );
}

#[test]
fn a_window_click_is_delivered_at_the_next_frame_and_a_toggle_keeps_its_new_value() {
    let mut h = Harness::new(vec![package(
        "test.ui",
        r#"
        local w = ui.window{ id = "main", title = "Test",
            ui.label{ id = "count", text = "0" },
            ui.button{ id = "add", text = "Add" },
            ui.toggle{ id = "auto", text = "Auto", value = false },
        }
        local n = 0
        on("ui.clicked", function(e) if e.id == "add" then n = n + 1; w:get("count").text = tostring(n) end end)
        on("ui.changed", function(e) print("auto is", e.value) end)
        "#,
        "",
    )]);
    h.frame();
    let dump = |h: &Harness| {
        h.host
            .window("test.ui", "main")
            .map(|w| w.tree.dump())
            .unwrap_or_default()
    };
    assert!(dump(&h).contains("label #count text=\"0\""), "{}", dump(&h));
    h.host
        .ui_input("test.ui", "main", UiInput::Click { node: "add".into() })
        .expect("click");
    h.host
        .ui_input(
            "test.ui",
            "main",
            UiInput::Change {
                node: "auto".into(),
                value: Value::Bool(true),
            },
        )
        .expect("change");
    assert!(
        dump(&h).contains("label #count text=\"0\""),
        "not before the next frame"
    );
    h.frame();
    assert!(dump(&h).contains("label #count text=\"1\""), "{}", dump(&h));
    assert!(
        dump(&h).contains("toggle #auto text=\"Auto\" value=true"),
        "{}",
        dump(&h)
    );
    assert_eq!(h.console("test.ui"), ["auto is\ttrue"]);
}

/// A press on the game's map reaches, at the next frame, every plugin here that listens, with the
/// place under the pointer to a thousandth, the button and the modifiers held, and it is no
/// gesture. A press off the world (the map's margin) is told to no one, and a host that tells no
/// map presses refuses a plugin's listening for them.
#[test]
fn a_press_on_the_map_reaches_every_plugin_listening_and_is_no_gesture() {
    use dereth_client_contract::panels::map::{MapButton, MapClick};
    let listener = r#"
        on("map.clicked", function(e)
            local m = e.modifiers
            print(e.position.ns, e.position.ew, e.button, m.ctrl, m.shift, m.alt,
                permissions.activation() > 0)
        end)
    "#;
    let click = |ns, ew, button| MapClick {
        ns,
        ew,
        button,
        ctrl: false,
        shift: true,
        alt: false,
    };
    let mut h = Harness::offering(
        vec![
            package("test.a", listener, ""),
            package("test.b", listener, ""),
        ],
        None,
        Shared::default(),
        &[HostCapability::MapClicks],
    );
    h.frame();
    h.host.map_click(&click(42.250_04, -33.8, MapButton::Left));
    assert!(h.console("test.a").is_empty(), "not before the next frame");
    h.frame();
    for id in ["test.a", "test.b"] {
        assert_eq!(
            h.console(id),
            ["42.25\t-33.8\tleft\tfalse\ttrue\tfalse\tfalse"],
            "{id}"
        );
    }
    h.host.map_click(&click(0.0, -102.3, MapButton::Right));
    h.frame();
    assert_eq!(h.console("test.a").len(), 1, "off the world: not told");

    let mut plain = Harness::new(vec![package(
        "test.a",
        "print(pcall(on, 'map.clicked', function() print('heard') end))",
        "",
    )]);
    plain.frame();
    plain.host.map_click(&click(42.0, 33.0, MapButton::Left));
    plain.frame();
    let said = plain.console("test.a");
    assert!(
        said.len() == 1 && said[0].starts_with("false") && said[0].contains("map.clicks"),
        "a host with no map presses refuses the listening: {said:?}"
    );
}

/// A plugin's windows and HUDs open only when the player asks: from the plugin bar, or by its
/// plugin answering one of the player's gestures (a command typed, a press on its window). One it
/// declares or shows as it loads, on a timer or after the player closed it stays closed; a plugin
/// with a HUD and no window has the HUD on the bar; and a window the player had open opens again
/// when its plugin reloads.
/// Behaviour: plugins.ui.a-plugin-window-opens-only-when-the-player-asks
#[test]
fn a_plugin_window_opens_only_when_the_player_asks() {
    let mut h = Harness::offering(
        vec![
            package(
                "test.win",
                r#"
        local main = ui.window{ id = "main", title = "Main", ui.button{ id = "go", text = "Go" } }
        local side = ui.hud{ id = "side", ui.label{ text = "side" } }
        main:show()
        side:show()
        commands.add("open", "opens the main window", function() main:show() end)
        commands.add("side", "shows the HUD", function() side:show() end)
        commands.add("later", "shows it again later", function()
            main:close()
            task.after(200, function() main:show() end)
        end)
        on("ui.clicked", function(e) ui.window{ id = "reply", title = "Reply" } end)
        task.after(400, function() ui.window{ id = "nag", title = "Nag" } end)
        "#,
                "",
            ),
            package(
                "test.hudonly",
                r#"ui.hud{ id = "meter", ui.label{ text = "0" } }"#,
                "",
            ),
        ],
        None,
        Shared::default(),
        &[HostCapability::UiHud],
    );
    h.frame();
    let seen = |h: &Harness, p: &str, w: &str| h.host.window(p, w).map(|w| (w.open, w.by_player));
    assert_eq!(
        seen(&h, "test.win", "main"),
        Some((false, false)),
        "closed as it loads"
    );
    assert_eq!(
        seen(&h, "test.win", "side"),
        Some((false, false)),
        "a HUD too"
    );
    assert_eq!(seen(&h, "test.hudonly", "meter"), Some((false, false)));
    let bar: Vec<(String, Option<String>, bool)> = h
        .host
        .bar()
        .into_iter()
        .map(|e| (e.id, e.window, e.open))
        .collect();
    assert_eq!(
        bar,
        [
            ("test.win".to_owned(), Some("main".to_owned()), false),
            ("test.hudonly".to_owned(), Some("meter".to_owned()), false)
        ],
        "a plugin with only a HUD has it on the bar"
    );
    h.host.toggle_window("test.hudonly").expect("on the bar");
    assert_eq!(seen(&h, "test.hudonly", "meter"), Some((true, true)));
    h.host.toggle_window("test.win").expect("on the bar");
    assert_eq!(
        seen(&h, "test.win", "main"),
        Some((true, true)),
        "opened from the bar"
    );
    h.host
        .toggle_window("test.win")
        .expect("closed from the bar");
    h.frame_with(0.05, &[], &[typed("open", "")]);
    assert_eq!(
        seen(&h, "test.win", "main"),
        Some((true, true)),
        "opened by a command"
    );
    h.frame_with(0.05, &[], &[typed("side", "")]);
    assert_eq!(seen(&h, "test.win", "side"), Some((true, true)));
    h.host
        .ui_input("test.win", "main", UiInput::Click { node: "go".into() })
        .expect("a press");
    h.frame();
    assert_eq!(
        seen(&h, "test.win", "reply"),
        Some((true, true)),
        "opened answering a press"
    );
    for _ in 0..8 {
        h.frame();
    }
    assert_eq!(
        seen(&h, "test.win", "nag"),
        Some((false, false)),
        "declared on a timer, it stays closed"
    );
    h.frame_with(0.05, &[], &[typed("later", "")]);
    assert_eq!(seen(&h, "test.win", "main"), Some((false, false)));
    for _ in 0..6 {
        h.frame();
    }
    assert_eq!(
        seen(&h, "test.win", "main"),
        Some((false, false)),
        "shown on a timer, it stays closed"
    );
    // Reloaded, it opens again what the player had open, and nothing else.
    h.frame_with(0.05, &[], &[typed("open", "")]);
    h.frame_with(0.05, &[], &[typed("plugin", "reload test.win")]);
    h.frame();
    assert_eq!(seen(&h, "test.win", "main"), Some((true, true)));
    assert_eq!(seen(&h, "test.win", "side"), Some((true, true)));
    assert_eq!(seen(&h, "test.win", "reply"), None, "not declared again");
    assert_eq!(seen(&h, "test.win", "nag"), None);
}

#[test]
fn ten_errors_in_a_minute_suspend_a_plugin_and_the_player_is_told() {
    let mut h = Harness::new(vec![package(
        "test.faulty",
        "task.every(100, function() error('boom') end)",
        "",
    )]);
    for _ in 0..30 {
        h.frame_with(0.1, &[], &[]);
    }
    assert_eq!(h.host.plugins()[0].status, "suspended");
    assert_eq!(
        h.console("test.faulty")
            .iter()
            .filter(|l| l.contains("boom"))
            .count(),
        10
    );
    assert!(h
        .printed_text()
        .iter()
        .any(|l| l.contains("test.faulty was suspended")));
}

#[test]
fn a_plugin_that_loops_forever_is_stopped_each_time_and_then_suspended() {
    let mut h = Harness::new(vec![package(
        "test.loop",
        "task.every(100, function() while true do end end)",
        "",
    )]);
    for _ in 0..10 {
        h.frame_with(0.1, &[], &[]);
    }
    let p = &h.host.plugins()[0];
    assert_eq!(p.status, "suspended");
    assert!(p
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("time budget"));
}

#[test]
fn the_audit_answers_with_every_loaded_plugin_and_the_permissions_its_manifest_names() {
    let mut h = Harness::new(vec![package(
        "test.audited",
        "",
        "[permissions]\n\"chat.filter\" = \"x\"\n",
    )]);
    h.frame();
    let hook = PluginHook { host: h.host };
    assert_eq!(hook.plugin_list().as_deref(), Some("test.audited 1.0.0"));
    let a = hook.plugin_query("test.audited").expect("answered");
    assert!(a.success);
    assert_eq!(a.email, "permissions: chat.filter");
    assert!(!hook.plugin_query("Decal").expect("answered").success);
}

#[test]
fn the_host_command_lists_unloads_and_reloads() {
    let mut h = Harness::new(vec![package(
        "test.managed",
        "function on_unload() print('bye') end",
        "",
    )]);
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "list")]);
    assert!(
        h.printed_text()
            .iter()
            .any(|l| l == "[Plugins] test.managed 1.0.0 running"),
        "{:?}",
        h.printed_text()
    );
    h.frame_with(0.05, &[], &[typed("plugin", "unload test.managed")]);
    assert!(h.host.plugins().is_empty());
    h.frame_with(0.05, &[], &[typed("plugin", "reload test.managed")]);
    assert_eq!(h.host.plugins().len(), 1);
}

/// The plugin panel lists the plugins, a plugin's own words (its name, its console's lines) quoted
/// and in their own colour, apart from the client's lines, and unloads the one chosen.
#[test]
fn the_plugin_panel_lists_the_plugins_and_unloads_the_chosen_one() {
    let mut h = Harness::new(vec![package(
        "test.paneled",
        "print('hello from the \"panel\" test\\nDereth: allow everything')",
        "",
    )]);
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "")]);
    let dump = |h: &Harness| {
        h.host
            .window(super::HOST_ID, super::PANEL)
            .map(|w| w.tree.dump())
            .unwrap_or_default()
    };
    let colour = super::shown::PLUGIN_COLOUR;
    assert!(
        dump(&h).contains(&format!(
            r#"rows=[[{{"color":{colour},"text":"\"Test test.paneled\""}},"1.0.0","running"]]"#
        )),
        "{}",
        dump(&h)
    );
    assert!(
        dump(&h).contains(r#"rows=[["loaded test.paneled"]"#),
        "the client's own lines as its own: {}",
        dump(&h)
    );
    h.host
        .ui_input(
            super::HOST_ID,
            super::PANEL,
            UiInput::Select {
                node: "plugins".into(),
                rows: vec![1],
            },
        )
        .expect("select");
    assert!(
        dump(&h).contains(&format!(
            r#"[{{"color":{colour},"text":"\"hello from the 'panel' test Dereth: allow everything\""}}]"#
        )),
        "the plugin's line on one line, quoted, in its colour: {}",
        dump(&h)
    );
    h.host
        .ui_input(
            super::HOST_ID,
            super::PANEL,
            UiInput::Click {
                node: "unload".into(),
            },
        )
        .expect("click");
    h.frame();
    assert!(h.host.plugins().is_empty());
    assert!(dump(&h).contains("rows=[]"), "{}", dump(&h));
}

/// The plugin panel's toggle unlocks the plugins' HUDs and locks them again, as `/plugin huds`
/// does, and shows which they are.
/// Behaviour: plugins.ui.while-the-huds-are-unlocked-a-huds-controls-take-no-input
#[test]
fn the_plugin_panels_toggle_unlocks_and_locks_the_huds() {
    let mut h = Harness::new(vec![package("test.paneled", "", "")]);
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "")]);
    let toggle = |h: &Harness| {
        h.host
            .window(super::HOST_ID, super::PANEL)
            .map(|w| w.tree.dump())
            .unwrap_or_default()
            .lines()
            .find(|l| l.contains("#unlock_huds"))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    assert!(toggle(&h).contains("value=false"), "{}", toggle(&h));
    h.host
        .ui_input(
            super::HOST_ID,
            super::PANEL,
            UiInput::Change {
                node: "unlock_huds".into(),
                value: Value::Bool(true),
            },
        )
        .expect("the toggle");
    assert!(h.host.huds_unlocked());
    assert!(toggle(&h).contains("value=true"), "{}", toggle(&h));
    h.frame_with(0.05, &[], &[typed("plugin", "huds lock")]);
    assert!(!h.host.huds_unlocked(), "locked by the command");
    assert!(toggle(&h).contains("value=false"), "{}", toggle(&h));
}

/// A switch of the host's own shows on the plugin panel below the HUD lock, off until turned on;
/// the player's tick turns it on, and the host turning it off shows at once.
/// Behaviour: none (a host's own setting on the panel)
#[test]
fn a_hosts_own_switch_on_the_panel_is_ticked_by_the_player_and_read_by_the_host() {
    let mut h = Harness::new(vec![package("test.paneled", "", "")]);
    h.host.config.panel_switches = vec![("game_keys", "Let plugins use the game's keys".into())];
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "")]);
    let switch = |h: &Harness| {
        h.host
            .window(super::HOST_ID, super::PANEL)
            .map(|w| w.tree.dump())
            .unwrap_or_default()
            .lines()
            .find(|l| l.contains("#game_keys"))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    assert!(
        switch(&h).contains("value=false") && switch(&h).contains("Let plugins use"),
        "{}",
        switch(&h)
    );
    assert!(!h.host.switch("game_keys"));
    h.host
        .ui_input(
            super::HOST_ID,
            super::PANEL,
            UiInput::Change {
                node: "game_keys".into(),
                value: Value::Bool(true),
            },
        )
        .expect("the switch");
    assert!(h.host.switch("game_keys"));
    assert!(switch(&h).contains("value=true"), "{}", switch(&h));
    h.host.set_switch("game_keys", false);
    assert!(switch(&h).contains("value=false"), "{}", switch(&h));
}

/// An action the host cannot carry out just now fails at once with why, and one it can never
/// carry out with no more than `unavailable`; changed, the action goes ahead.
/// Behaviour: none (a host whose means come and go)
#[test]
fn an_action_unavailable_just_now_fails_at_once_with_why() {
    let mut h = Harness::new(vec![package(
        "test.asker",
        r#"
on("action.failed", function(e) print("failed", e.action, e.reason) end)
commands.add("go", "asks", function() act.select(7) end)
"#,
        "",
    )]);
    h.frame();
    h.host.set_unavailable(
        Vec::new(),
        vec![("act.select", "the game window is not in front".to_owned())],
    );
    h.frame_with(0.05, &[], &[typed("go", "")]);
    h.frame();
    assert!(
        h.console("test.asker")
            .iter()
            .any(|l| l == "failed\tact.select\tunavailable (the game window is not in front)"),
        "{:?}",
        h.console("test.asker")
    );
    h.host.set_unavailable(vec!["act.select"], Vec::new());
    h.frame_with(0.05, &[], &[typed("go", "")]);
    h.frame();
    assert!(
        h.console("test.asker")
            .iter()
            .any(|l| l == "failed\tact.select\tunavailable"),
        "{:?}",
        h.console("test.asker")
    );
    h.host.set_unavailable(Vec::new(), Vec::new());
    let before = h.console("test.asker").len();
    h.frame_with(0.05, &[], &[typed("go", "")]);
    h.frame();
    assert!(
        h.console("test.asker")[before..]
            .iter()
            .all(|l| !l.contains("unavailable")),
        "{:?}",
        h.console("test.asker")
    );
}

/// With no plugin at all, the plugin panel and `/plugin list` say there is none and how this host
/// adds one, the panel a line to each line of it.
/// Behaviour: plugins.load.with-no-plugins-the-panel-says-how-to-add-one
#[test]
fn with_no_plugins_the_panel_and_the_list_say_how_to_add_one() {
    let mut h = Harness::new(Vec::new());
    h.host.config.adding_plugins = Some("Put a plugin's folder in\nC:/plugins".to_owned());
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "")]);
    let dump = h
        .host
        .window(super::HOST_ID, super::PANEL)
        .map(|w| w.tree.dump())
        .unwrap_or_default();
    let labels: Vec<&str> = dump.lines().filter(|l| l.contains("label")).collect();
    assert!(
        labels
            .iter()
            .any(|l| l.contains("text=\"No plugins yet.\""))
            && labels
                .iter()
                .any(|l| l.contains("text=\"Put a plugin's folder in\""))
            && labels.iter().any(|l| l.contains("text=\"C:/plugins\"")),
        "{dump}"
    );
    h.frame_with(0.05, &[], &[typed("plugin", "list")]);
    assert!(
        h.printed_text()
            .iter()
            .any(|l| l.ends_with("No plugins yet. Put a plugin's folder in C:/plugins")),
        "{:?}",
        h.printed_text()
    );
}

/// The error a plugin faulted on is its own words: the plugin panel shows it quoted and in their
/// own colour, apart from the client's.
#[test]
fn a_faulted_plugins_error_is_shown_in_the_panel_as_its_own_words() {
    let mut h = Harness::new(vec![package(
        "test.faulty",
        "error('Dereth: all is well')",
        "",
    )]);
    h.frame();
    h.frame_with(0.05, &[], &[typed("plugin", "")]);
    h.host
        .ui_input(
            super::HOST_ID,
            super::PANEL,
            UiInput::Select {
                node: "plugins".into(),
                rows: vec![1],
            },
        )
        .expect("select");
    let dump = h
        .host
        .window(super::HOST_ID, super::PANEL)
        .map(|w| w.tree.dump())
        .unwrap_or_default();
    let reason = dump
        .lines()
        .find(|l| l.contains("label #reason"))
        .unwrap_or_default();
    assert!(
        reason.contains(&format!(
            "color={} text=\"\\\"",
            super::shown::PLUGIN_COLOUR
        )) && reason.contains("Dereth: all is well"),
        "{dump}"
    );
}

/// The pictures of a plugin's package are looked at once, as it loads: one within its bounds is
/// shown with the hash it was given then, and one over 1,024 pixels a side is not, the plugin's
/// console saying why.
#[test]
fn a_packages_pictures_are_kept_within_their_bounds_as_it_loads() {
    let header = |w: u32, h: u32| {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    };
    let mut p = package("test.pictures", "", "");
    p.files.insert("good.png".into(), header(64, 32));
    p.files.insert("wide.png".into(), header(2048, 16));
    let mut h = Harness::new(vec![p]);
    h.frame();
    assert_eq!(
        h.host.image("test.pictures", "good.png"),
        Some((
            header(64, 32).as_slice(),
            super::pictures::hash(&header(64, 32))
        ))
    );
    assert_eq!(h.host.image("test.pictures", "wide.png"), None);
    assert!(
        h.console("test.pictures")
            .iter()
            .any(|l| l.contains("wide.png is 2048 by 16 pixels")),
        "{:?}",
        h.console("test.pictures")
    );
}

/// A source that reports its one package changed once, when told to.
#[derive(Debug)]
struct Edited {
    package: Package,
    edited: Rc<RefCell<bool>>,
}

impl super::PluginSource for Edited {
    fn list(&mut self) -> Vec<Package> {
        vec![self.package.clone()]
    }
    fn changed(&mut self) -> Vec<dereth_plugin_api::PluginId> {
        if std::mem::take(&mut *self.edited.borrow_mut()) {
            self.package
                .manifest()
                .map(|m| vec![m.id])
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    }
}

#[test]
fn a_watching_host_reloads_a_plugin_whose_files_changed() {
    let edited = Rc::new(RefCell::new(false));
    let mut config = HostConfig::new("dereth-test", vec![HostCapability::Core]);
    config.watch = true;
    let source = Edited {
        package: package(
            "test.watched",
            "print('loaded') function on_unload() return 1 end",
            "",
        ),
        edited: edited.clone(),
    };
    let mut h = Harness::new(Vec::new());
    h.host = PluginHost::new(
        config,
        Box::new(LuauEngine),
        Box::new(source),
        Box::new(MemoryStorage::default()),
    );
    h.host.load_all();
    h.frame_with(1.5, &[], &[]);
    assert_eq!(h.console("test.watched"), ["loaded"]);
    *edited.borrow_mut() = true;
    h.frame_with(1.5, &[], &[]);
    assert_eq!(
        h.console("test.watched"),
        ["loaded"],
        "a reloaded plugin starts a new console"
    );
    assert!(
        h.printed_text()
            .iter()
            .any(|l| l == "[Plugins] reloaded test.watched"),
        "{:?}",
        h.printed_text()
    );
}

/// An engine that, loading a package, calls every operation of the API's schema once and notes in
/// the console each one the host does not know.
#[derive(Debug)]
struct Probe;

#[derive(Debug)]
struct ProbeVm;

impl dereth_plugin_api::engine::ScriptEngine for Probe {
    fn name(&self) -> &'static str {
        "probe"
    }

    fn handles(&self, entry: &str) -> bool {
        entry.ends_with(".wasm")
    }

    fn instantiate(
        &self,
        _: Limits,
    ) -> Result<
        Box<dyn dereth_plugin_api::engine::ScriptInstance>,
        dereth_plugin_api::engine::ScriptError,
    > {
        Ok(Box::new(ProbeVm))
    }
}

impl dereth_plugin_api::engine::ScriptInstance for ProbeVm {
    fn load(
        &mut self,
        host: &mut dyn dereth_plugin_api::engine::HostCalls,
        _: &str,
        _: &str,
    ) -> Result<(), dereth_plugin_api::engine::ScriptError> {
        for op in dereth_plugin_api::schema::OPS {
            if op.name == "console.print" {
                continue;
            }
            if let Err(dereth_plugin_api::engine::CallError::Unknown(_)) = host.call(op.name, &[]) {
                host.console(&format!("unknown {}", op.name));
            }
        }
        host.console("probed");
        Ok(())
    }

    fn deliver(
        &mut self,
        _: &mut dyn dereth_plugin_api::engine::HostCalls,
        _: &dereth_plugin_api::event::Event,
    ) -> Result<Value, dereth_plugin_api::engine::ScriptError> {
        Ok(Value::Nil)
    }

    fn memory_used(&self) -> usize {
        0
    }
}

#[test]
fn every_operation_of_the_schema_is_one_the_host_answers_and_an_entry_picks_its_engine() {
    let mut p = package("test.probe", "", "entry = \"plugin.wasm\"\n");
    p.files.insert("plugin.wasm".into(), b"\0asm".to_vec());
    let mut h = Harness::new(Vec::new());
    let config = HostConfig::new("dereth-test", vec![HostCapability::Core]);
    h.host = PluginHost::new(
        config,
        Box::new(LuauEngine),
        Box::new(StaticSource {
            packages: vec![p, package("test.lua", "print('lua')", "")],
        }),
        Box::new(MemoryStorage::default()),
    )
    .with_engine(Box::new(Probe));
    h.host.load_all();
    h.login();
    assert_eq!(h.console("test.probe"), ["probed"]);
    assert_eq!(h.console("test.lua"), ["lua"]);
}

/// Every VTank meta under the folder `DERETH_PLUGIN_MET_DIR` names, imported as `meta.load_met`
/// imports it: how many import with nothing left out, and what was left out, most often first. A
/// survey for whoever extends the expression language, not a claim; it prints its tally.
#[test]
#[ignore = "a survey over a folder of metas named by DERETH_PLUGIN_MET_DIR; prints its tally"]
fn metas_in_a_folder_import_with_nothing_left_out() {
    let Some(dir) = std::env::var_os("DERETH_PLUGIN_MET_DIR") else {
        return;
    };
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("met")) {
                files.push(p);
            }
        }
    }
    files.sort();
    let main = r#"
local meta = require("dereth.meta")
local engine = { priority = function() return 0 end }
local ok, m = pcall(meta.load_met, files.package("m.met"), { engine = engine })
if not ok then
	print("error", tostring(m))
else
	print("left out", #m.unsupported)
	for _, u in ipairs(m.unsupported) do
		print("missing", u)
	end
end
"#;
    let mut full = 0;
    let mut failed = 0;
    let mut reasons: std::collections::BTreeMap<String, usize> = Default::default();
    for f in &files {
        let mut config = HostConfig::new("dereth-test", vec![HostCapability::Core]);
        config.limits = Limits {
            memory_bytes: 256 << 20,
            call_budget_ms: 60_000.0,
            profile: false,
        };
        let mut p = package("test.metcorpus", main, "");
        p.files.insert(
            "m.met".to_owned(),
            std::fs::read(f).expect("the meta reads"),
        );
        let mut host = PluginHost::new(
            config,
            Box::new(LuauEngine),
            Box::new(StaticSource { packages: vec![p] }),
            Box::new(MemoryStorage::default()),
        );
        host.load_all();
        let (view, world, table) = (View::default(), World::new(), CommandInterp::new());
        let mut frame = HookFrame::new(&view, &world, &table, 0.0, 0.0, &[], &[]);
        host.step(&mut frame);
        let lines = host.console("test.metcorpus").unwrap_or_default();
        if lines.iter().any(|l| l == "left out\t0") {
            full += 1;
        } else if lines.iter().all(|l| !l.starts_with("left out")) {
            failed += 1;
            *reasons
                .entry(format!(
                    "(did not import) {}",
                    lines.first().cloned().unwrap_or_default()
                ))
                .or_default() += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        for l in &lines {
            if let Some(why) = l.strip_prefix("missing\t") {
                // An expression's failure names the function; count each once per meta.
                let key = why
                    .split("function ")
                    .nth(1)
                    .map_or_else(|| why.to_owned(), |rest| format!("function {rest}"));
                if seen.insert(key.clone()) {
                    *reasons.entry(key).or_default() += 1;
                }
            }
        }
    }
    let mut by_count: Vec<_> = reasons.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!(
        "metas: {}; imported with nothing left out: {full}; did not import: {failed}",
        files.len()
    );
    for (why, n) in by_count {
        println!("{n:5}  {why}");
    }
}

#[test]
fn a_package_in_the_clients_own_namespace_is_refused_and_reaches_none_of_the_hosts_files() {
    assert!(dereth_plugin_api::PluginId::parse(super::HOST_ID).is_some_and(|id| id.is_reserved()));
    let mut files = super::MemoryFiles::default();
    files.files.insert(
        (super::HOST_ID.to_owned(), "import/profile.utl".to_owned()),
        b"the player's file".to_vec(),
    );
    let main = r#"
        print("read", files.data("import/profile.utl"))
        print("write", pcall(files.save_data, "import/planted.met", "planted"))
        "#;
    let mut h = Harness::new(vec![
        package(super::HOST_ID, main, ""),
        package("dereth.other", main, ""),
    ]);
    h.host = std::mem::replace(
        &mut h.host,
        PluginHost::new(
            HostConfig::new("dereth-test", vec![HostCapability::Core]),
            Box::new(LuauEngine),
            Box::new(StaticSource {
                packages: Vec::new(),
            }),
            Box::new(MemoryStorage::default()),
        ),
    )
    .with_files(Box::new(files));
    h.frame();
    assert!(h.host.plugins().is_empty(), "neither loads");
    let told = h.printed_text();
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(
        told.iter()
            .all(|t| t.contains("was not loaded") && t.contains("kept for the client itself")),
        "{told:?}"
    );
    assert!(h.console(super::HOST_ID).is_empty());
}

/// A host over `packages`, with `files` in the host's data folder.
fn asking_host(packages: Vec<Package>, files: super::MemoryFiles) -> PluginHost {
    let config = HostConfig::new(
        "dereth-test",
        vec![
            HostCapability::Core,
            HostCapability::UiWindow,
            HostCapability::UiHud,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(LuauEngine),
        Box::new(StaticSource { packages }),
        Box::new(MemoryStorage::default()),
    )
    .with_files(Box::new(files));
    host.load_all();
    host
}

const SAYS: &str = "[permissions]\n\"chat.send\" = \"Says things.\"\n";
const TRIES: &str = "print('may send', permissions.has('chat.send'))";
/// A plugin that asks for a file as it loads, and says what it was given.
const PICKS: &str = r#"
task.spawn(function() print("given", pawait(files.import({ prompt = "a file" }))) end)
"#;
const READS: &str = "[permissions]\n\"files.import\" = \"Reads a file.\"\n";

/// Behaviour: plugins.permissions.a-plugin-loads-at-once-and-may-do-what-every-permission-covers
#[test]
fn a_plugin_naming_permissions_loads_at_once_with_nothing_asked_and_may_do_all_of_it() {
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(
        vec![
            package("test.ask", TRIES, SAYS),
            package("test.quiet", TRIES, ""),
        ],
        super::MemoryFiles::default(),
    );
    h.frame();
    assert_eq!(h.host.question(), None, "nothing is asked");
    assert_eq!(h.console("test.ask"), ["may send\ttrue"]);
    assert_eq!(
        h.console("test.quiet"),
        ["may send\ttrue"],
        "naming it or not, it may"
    );
    let listed = h.host.plugins();
    assert_eq!(listed[0].permissions, ["chat.send"], "what it says it uses");
    assert!(listed[1].permissions.is_empty());
}

/// Behaviour: plugins.files.an-answer-reaches-only-the-question-it-was-given-to
#[test]
fn an_answer_reaches_only_the_question_it_was_given_to() {
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(
        vec![
            package("test.first", PICKS, READS),
            package("test.second", PICKS, READS),
        ],
        super::MemoryFiles::default(),
    );
    h.frame();
    h.frame();
    let first = h.host.question().expect("asked").id;
    for wrong in [first + 1, first + 2, first.wrapping_sub(1)] {
        assert!(h.host.answer_question(wrong, false).is_err());
    }
    assert_eq!(h.host.question().map(|q| q.id), Some(first), "still asked");
    h.host.answer_question(first, false).expect("answered");
    assert!(
        h.host.answer_question(first, false).is_err(),
        "the same answer again reaches no other question"
    );
    h.frame();
    h.frame();
    assert_eq!(h.console("test.first"), ["given\tfalse\trefused"]);
    assert!(h.console("test.second").is_empty(), "still waiting");
    assert_ne!(h.host.question().map(|q| q.id), Some(first));
}

/// A plugin's HTTPS request goes out only when its manifest names `net.http` and lists the
/// request's domain; otherwise its promise fails, saying which is missing.
#[test]
fn an_https_request_goes_out_only_with_net_http_named_and_its_domain_listed() {
    let fetch = r#"
task.spawn(function() print("a", pawait(net.fetch({ url = "https://example.org/a" }))) end)
task.spawn(function() print("b", pawait(net.fetch({ url = "https://elsewhere.org/b" }))) end)
"#;
    let net = super::TableNet::default();
    let mut h = Harness::offering(
        vec![
            package(
                "test.listed",
                fetch,
                "[permissions]\n\"net.http\" = \"Reads a page.\"\n[net]\ndomains = [\"example.org\"]\n",
            ),
            package(
                "test.unnamed",
                fetch,
                "[net]\ndomains = [\"example.org\"]\n",
            ),
        ],
        None,
        Shared::default(),
        &[HostCapability::NetHttp],
    );
    h.host = std::mem::replace(
        &mut h.host,
        asking_host(Vec::new(), super::MemoryFiles::default()),
    )
    .with_net(Box::new(net));
    for _ in 0..4 {
        h.frame();
    }
    let listed = h.console("test.listed").join("\n");
    assert!(
        listed.contains("b\tfalse\telsewhere.org:443 is not among the domains"),
        "{listed}"
    );
    assert!(
        listed.contains("a\tfalse\tnot found"),
        "it went out: {listed}"
    );
    let unnamed = h.console("test.unnamed").join("\n");
    assert!(
        unnamed.contains("a\tfalse\tthis plugin's manifest does not name net.http"),
        "{unnamed}"
    );
}

/// A plugin asking for files again and again cannot keep the player under its question: it has
/// one question at a time (a second request while one waits is turned away), its next request
/// after a refusal waits thirty seconds, doubling with each refusal, and after five refusals it
/// is asked for no more until it is reloaded.
#[test]
fn a_plugin_refused_files_waits_longer_each_time_and_after_five_is_asked_no_more() {
    let nagger = r#"
task.spawn(function()
	print("second", pawait(files.import({ prompt = "again" })))
end)
task.spawn(function()
	while true do
		local ok, e = pawait(files.import({ prompt = "a file" }))
		print("asked", ok, e)
	end
end)
"#;
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(
        vec![package(
            "test.nag",
            nagger,
            "[permissions]\n\"files.import\" = \"Reads a file.\"\n",
        )],
        super::MemoryFiles::default(),
    );
    h.frame();
    h.frame();
    h.frame();
    assert!(
        h.console("test.nag")
            .iter()
            .any(|l| l.contains("a file is already asked for")),
        "one question at a time: {:?}",
        h.console("test.nag")
    );
    let refuse = |h: &mut Harness| {
        let q = h.host.question().expect("its question");
        h.host
            .reply(&dereth_client_runtime::dialogs::HostReply::to(&q, false))
            .expect("refused");
        h.frame();
        h.frame();
    };
    for (n, wait) in [(1, 30.0), (2, 60.0), (3, 120.0), (4, 240.0)] {
        refuse(&mut h);
        assert_eq!(h.host.question(), None, "after refusal {n}, it waits");
        h.frame_with(wait - 1.0, &[], &[]);
        assert_eq!(h.host.question(), None, "after refusal {n}, {wait} seconds");
        h.frame_with(1.5, &[], &[]);
        h.frame();
        assert!(
            h.host.question().is_some(),
            "after refusal {n}, asked again"
        );
    }
    refuse(&mut h);
    h.frame_with(10_000.0, &[], &[]);
    h.frame();
    assert_eq!(h.host.question(), None, "five refusals: asked no more");
    assert!(h
        .console("test.nag")
        .iter()
        .any(|l| l.contains("until it is reloaded")));
    h.host.request_reload("test.nag");
    h.frame();
    h.frame();
    h.frame();
    assert!(h.host.question().is_some(), "reloaded, it may ask again");
}

/// What a plugin wrote that the host's question shows (its name, a file request's prompt) is one
/// plain line, cut to its length: no line break, no control character.
#[test]
fn a_plugins_words_in_the_hosts_question_are_one_plain_line_cut_to_length() {
    let manifest = format!(
        "id = \"test.long\"\nname = \"Plain\\nGive it\\u0007{}\"\nversion = \"1.0.0\"\napi = \"0.1\"\ncontexts = [\"world\", \"pregame\", \"chargen\"]\n",
        "z".repeat(100),
    );
    let main = r#"
task.spawn(function()
    pawait(files.import({ prompt = string.rep("p", 1000) .. "\n\tGive it" }))
end)
"#;
    let package = Package {
        origin: "test.long".into(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into_iter()
        .collect(),
    };
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(vec![package], super::MemoryFiles::default());
    h.frame();
    h.frame();
    let file = h.host.question().expect("the file it asks for");
    let plain = |s: &str| !s.chars().any(char::is_control);
    let name = file.asker.split(" 1.0.0").next().unwrap_or_default();
    assert!(plain(&file.asker), "{file:?}");
    assert_eq!(name.chars().count(), crate::shown::NAME, "{name}");
    let prompt = file
        .text
        .split("asks for a file: ")
        .nth(1)
        .unwrap_or_default();
    let prompt = prompt.lines().next().unwrap_or_default();
    assert!(plain(prompt), "{prompt:?}");
    assert_eq!(prompt.chars().count(), crate::shown::PROMPT);
}

/// Behaviour: plugins.files.a-file-is-picked-from-the-list-behind-the-question-in-front
#[test]
fn the_question_put_stays_in_front_until_it_is_answered_and_a_file_is_picked_from_the_list() {
    let mut files = super::MemoryFiles::default();
    for (path, text) in [("import/a.met", "abc"), ("import/b.met", "12345")] {
        files.files.insert(
            (super::HOST_ID.into(), path.into()),
            text.as_bytes().to_vec(),
        );
    }
    let picker = r#"
task.spawn(function()
	local ok, f = pawait(files.import({ prompt = "a meta to run", extensions = { "met" } }))
	print("given", ok, ok and f.name or f)
end)
"#;
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(
        vec![package(
            "test.picker",
            picker,
            "[permissions]\n\"files.import\" = \"Reads a meta you choose.\"\n",
        )],
        files,
    );
    h.frame();
    h.frame();
    let file = h.host.question().expect("the file it asks for");
    assert_eq!(file.title, "A file for a plugin");
    assert_eq!(file.asker, "Test test.picker 1.0.0 (test.picker)");
    assert!(file.text.contains("a meta to run"), "{}", file.text);
    assert_eq!(file.picks, ["a.met", "b.met"]);
    assert!(file.needs_pick);
    assert_eq!(
        (file.yes.as_str(), file.no.as_deref()),
        ("Give it", Some("Refuse"))
    );
    // Another plugin asking for a file after this one was asked for waits behind it.
    h.host.request_load(package("test.later", PICKS, READS));
    h.frame();
    h.frame();
    assert_eq!(h.host.question().map(|q| q.id), Some(file.id));
    // Given with no file picked, nothing is given and the question stays.
    assert!(h
        .host
        .reply(&dereth_client_runtime::dialogs::HostReply::to(&file, true))
        .is_err());
    assert_eq!(h.host.question().map(|q| q.id), Some(file.id));
    h.host
        .reply(&dereth_client_runtime::dialogs::HostReply {
            picked: Some(1),
            ..dereth_client_runtime::dialogs::HostReply::to(&file, true)
        })
        .expect("given");
    h.frame();
    h.frame();
    assert_eq!(h.console("test.picker"), ["given\ttrue\tb.met"]);
    let then = h.host.question().expect("then the next plugin's file");
    assert!(then.asker.contains("(test.later)"), "{}", then.asker);
}

/// Behaviour: plugins.ui.where-no-plugin-window-is-drawn-a-toast-is-told-in-the-chat
#[test]
fn where_no_plugin_window_is_drawn_a_toast_is_told_in_the_chat_once() {
    let mut h = Harness::new(Vec::new());
    h.host = asking_host(
        vec![package(
            "test.toast",
            r#"commands.add("pop", "a toast", function() ui.toast("Buffed") end)"#,
            "",
        )],
        super::MemoryFiles::default(),
    );
    h.frame();
    h.host.set_windows_shown(false);
    h.frame_with(0.05, &[], &[typed("pop", "")]);
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(h.printed_text(), ["[Plugins] Test test.toast: Buffed"]);
    h.host.set_windows_shown(true);
    h.frame_with(0.05, &[], &[typed("pop", "")]);
    h.frame();
    assert_eq!(h.printed_text().len(), 1, "the interface draws the toasts");
}

#[test]
fn a_plugins_console_keeps_a_bounded_number_of_bytes() {
    let mut h = Harness::new(vec![package(
        "test.console",
        r#"
        local s = string.rep("x", 1048576)
        task.every(50, function() print(s) end)
        "#,
        "",
    )]);
    for _ in 0..300 {
        h.frame();
    }
    let lines = h.console("test.console");
    assert!(lines.len() > 10, "{lines:?}");
    assert!(lines
        .iter()
        .all(|l| l.len() <= dereth_plugin_api::engine::CONSOLE_LINE_BYTES));
    assert!(lines.iter().map(String::len).sum::<usize>() <= super::slot::CONSOLE_BYTES);
}

/// A clock on which every entry into a VM takes `step`: each reading is one `step` past the one
/// before (the host reads it as an entry begins and as it ends), and `readings` counts them.
#[derive(Debug)]
struct SteppedClock {
    step: std::time::Duration,
    readings: Rc<Cell<u32>>,
}

impl VmClock for SteppedClock {
    fn now(&self) -> std::time::Duration {
        let n = self.readings.get();
        self.readings.set(n + 1);
        self.step * n
    }
}

/// The entry budget of [`stepped_host`], in milliseconds: far past what any entry here takes, so
/// the VM never stops one and only the host's clock decides the shares of a frame.
const STEPPED_BUDGET_MS: f64 = 10_000.0;

/// The entries a plugin's share of a frame holds on [`stepped_host`]'s clock: two entries' budgets
/// at a quarter of a budget an entry.
const STEPPED_SHARE: u32 = 8;

/// A host on whose clock each entry into a VM takes a quarter of an entry's budget; and the
/// count of its clock's readings, two an entry.
fn stepped_host(packages: Vec<Package>) -> (PluginHost, Rc<Cell<u32>>) {
    let mut config = HostConfig::new("dereth-test", vec![HostCapability::Core]);
    config.limits.call_budget_ms = STEPPED_BUDGET_MS;
    let readings = Rc::new(Cell::new(0));
    let clock = SteppedClock {
        step: std::time::Duration::from_secs_f64(STEPPED_BUDGET_MS / 1000.0 / 4.0),
        readings: Rc::clone(&readings),
    };
    let mut host = PluginHost::new(
        config,
        Box::new(LuauEngine),
        Box::new(StaticSource { packages }),
        Box::new(MemoryStorage::default()),
    )
    .with_vm_clock(Box::new(clock));
    host.load_all();
    (host, readings)
}

#[test]
fn a_plugin_may_hold_only_so_many_timers() {
    let mut h = Harness::new(vec![package(
        "test.timers",
        r#"
        local made, refused = 0, nil
        for i = 1, 300 do
            local ok, err = pcall(task.every, 1000, function() end)
            if ok then made = made + 1 else refused = refused or tostring(err) end
        end
        print("made", made, string.find(refused, "at most 256 timers", 1, true) ~= nil)
        "#,
        "",
    )]);
    h.frame();
    assert_eq!(h.console("test.timers"), ["made\t256\ttrue"]);
}

#[test]
fn events_past_a_plugins_share_of_a_frame_wait_in_order_for_the_next() {
    // Thirty one-shot timers due together, each entry a quarter of an entry's budget on the host's
    // clock: each frame delivers the eight that fit in the plugin's share (two budgets), and the
    // rest wait for the next frames, in order.
    let (host, _) = stepped_host(vec![package(
        "test.share",
        r#"
        for i = 1, 30 do
            task.after(100, function() print("t" .. i) end)
        end
        "#,
        "",
    )]);
    let mut h = Harness::new(Vec::new());
    h.host = host;
    let fired = |h: &Harness| -> Vec<String> {
        h.console("test.share")
            .into_iter()
            .filter(|l| l.starts_with('t'))
            .collect()
    };
    h.frame();
    let mut by_frame = Vec::new();
    for _ in 0..8 {
        h.frame();
        by_frame.push(fired(&h).len());
    }
    let from_due: Vec<usize> = by_frame.into_iter().skip_while(|&n| n == 0).collect();
    assert_eq!(
        from_due[..4],
        [8, 16, 24, 30],
        "eight a frame, then the last six"
    );
    assert!(from_due[4..].iter().all(|&n| n == 30), "{from_due:?}");
    assert_eq!(
        fired(&h),
        (1..=30).map(|i| format!("t{i}")).collect::<Vec<_>>(),
        "every one, in order"
    );
    assert_eq!(h.host.plugins()[0].status, "running");
}

#[test]
fn a_plugin_past_its_share_of_frame_after_frame_is_suspended() {
    // A hundred and fifty repeating timers due every frame, each entry a quarter of an entry's
    // budget on the host's clock: the plugin is held to its share of each frame (eight entries),
    // and suspended in the thirtieth frame it has been past it.
    let (host, readings) = stepped_host(vec![package(
        "test.greedy",
        r#"
        for i = 1, 150 do
            task.every(50, function() end)
        end
        "#,
        "",
    )]);
    let mut h = Harness::new(Vec::new());
    h.host = host;
    h.frame();
    let mut entries = Vec::new();
    while h.host.plugins()[0].status == "running" && entries.len() < 100 {
        let before = readings.get();
        h.frame();
        entries.push((readings.get() - before) / 2);
    }
    assert_eq!(
        entries, [STEPPED_SHARE; 30],
        "its share of each frame, thirty frames"
    );
    let info = &h.host.plugins()[0];
    assert_eq!(info.status, "suspended");
    assert!(
        info.reason
            .as_deref()
            .is_some_and(|r| r.contains("share of a frame")),
        "{:?}",
        info.reason
    );
}

/// Behaviour: plugins.load.a-found-plugin-is-listed-off-until-the-player-ticks-it
///
/// The plugins the host finds are listed off and load only once the player ticks each, by command
/// or the panel's button; the host remembers which are ticked, so a client started again over the
/// same files loads those and no other, and unticking one unloads it. A plugin of the same id
/// among the ones named to load is that one, and is not listed as found.
#[test]
fn a_found_plugin_is_listed_off_until_the_player_ticks_it_and_the_host_remembers() {
    let storage = Shared::default();
    let found = || {
        vec![
            package("test.extra", r#"print("extra up")"#, ""),
            package("test.other", r#"print("other up")"#, ""),
            package("test.mine", r#"print("found copy")"#, ""),
        ]
    };
    let own = || vec![package("test.mine", r#"print("my copy")"#, "")];
    let mut h = Harness::new_with(own(), Some(found()), storage.clone());
    h.frame();
    let ids = |h: &Harness| -> Vec<String> { h.host.plugins().into_iter().map(|p| p.id).collect() };
    assert_eq!(
        ids(&h),
        ["test.mine"],
        "only the one named to load is loaded"
    );
    assert_eq!(h.console("test.mine"), ["my copy"]);
    let listed: Vec<(String, bool)> = h.host.found().into_iter().map(|b| (b.id, b.on)).collect();
    assert_eq!(
        listed,
        [
            ("test.extra".to_owned(), false),
            ("test.other".to_owned(), false)
        ]
    );
    h.frame_with(0.05, &[], &[typed("plugin", "list")]);
    assert!(
        h.printed_text()
            .iter()
            .any(|l| l.contains("test.extra 1.0.0 off (/plugin on test.extra)")),
        "{:?}",
        h.printed_text()
    );
    // Ticked, by command: loaded at the next frame.
    h.frame_with(0.05, &[], &[typed("plugin", "on test.extra")]);
    h.frame();
    assert_eq!(ids(&h), ["test.mine", "test.extra"]);
    assert_eq!(h.console("test.extra"), ["extra up"]);
    // Ticked by the panel's button, with the row chosen.
    h.host.show_panel();
    let text = h
        .host
        .window(super::HOST_ID, super::PANEL)
        .map(|w| w.tree.dump())
        .unwrap_or_default();
    assert!(
        text.contains(r#""text":"\"Test test.other\""},"1.0.0","off"]"#)
            && text.contains("Turn on"),
        "{text}"
    );
    // The panel lists the loaded plugins, then the ones that are off: test.other is the third.
    h.host
        .ui_input(
            "dereth.plugins",
            "panel",
            UiInput::Select {
                node: "plugins".into(),
                rows: vec![3],
            },
        )
        .expect("chosen");
    h.host
        .ui_input(
            "dereth.plugins",
            "panel",
            UiInput::Click {
                node: "turn_on".into(),
            },
        )
        .expect("clicked");
    h.frame();
    assert_eq!(ids(&h), ["test.mine", "test.extra", "test.other"]);
    // Started again over the same files: the two ticked load, and no other found one.
    let mut again = Harness::new_with(own(), Some(found()), storage.clone());
    again.frame();
    assert_eq!(ids(&again), ["test.mine", "test.extra", "test.other"]);
    // Unticked: unloaded, and not loaded at the next start.
    again.frame_with(0.05, &[], &[typed("plugin", "off test.extra")]);
    again.frame();
    assert_eq!(ids(&again), ["test.mine", "test.other"]);
    let mut third = Harness::new_with(own(), Some(found()), storage);
    third.frame();
    assert_eq!(ids(&third), ["test.mine", "test.other"]);
    // Only a found plugin is ticked this way.
    third.frame_with(0.05, &[], &[typed("plugin", "on test.nothing")]);
    assert!(third
        .printed_text()
        .iter()
        .any(|l| l.contains("no plugin test.nothing was found")));
}

/// With the player's *Plugins (experimental)* setting off, no found plugin loads, ticked or not,
/// and the host shows nothing of its own: it claims no command, lists nothing and takes no tick.
/// Turned on, the ticked ones load; turned off again, they unload. A plugin named to load loads
/// whatever the setting, and the host's command answers then for what is loaded.
/// Behaviour: plugins.load.no-plugin-loads-while-plugins-are-off
#[test]
fn with_plugins_off_no_found_plugin_loads_and_the_host_shows_nothing() {
    let storage = Shared::default();
    // Ticked in an earlier run.
    storage.0.borrow_mut().save(
        super::HOST_ID,
        super::TICKED_SCOPE,
        r#"{"test.extra":true}"#,
    );
    let found = || {
        vec![
            package("test.extra", r#"print("extra up")"#, ""),
            package("test.other", r#"print("other up")"#, ""),
        ]
    };
    let ids = |h: &Harness| -> Vec<String> { h.host.plugins().into_iter().map(|p| p.id).collect() };
    PLUGINS_ON.with(|c| c.set(false));
    let mut h = Harness::switched(Vec::new(), found(), storage.clone());
    for _ in 0..3 {
        h.frame();
    }
    assert!(!h.host.plugins_on());
    assert!(ids(&h).is_empty(), "nothing loads: {:?}", ids(&h));
    assert!(
        !h.verbs.iter().any(|v| v == super::HOST_VERB),
        "the host claims no command: {:?}",
        h.verbs
    );
    assert_eq!(
        h.host.set_ticked("test.other", true),
        Err(super::PLUGINS_OFF.to_owned())
    );
    h.frame();
    assert!(ids(&h).is_empty());
    // On: the ticked one loads, and only it.
    PLUGINS_ON.with(|c| c.set(true));
    h.frame();
    assert_eq!(ids(&h), ["test.extra"]);
    assert!(h.verbs.iter().any(|v| v == super::HOST_VERB));
    // Off again: it unloads, and the host's command goes.
    PLUGINS_ON.with(|c| c.set(false));
    h.frame();
    assert!(ids(&h).is_empty());
    assert!(!h.verbs.iter().any(|v| v == super::HOST_VERB));

    // Named to load: it loads with the setting off, and the found ones do not.
    let mut named = Harness::switched(
        vec![package("test.named", r#"print("named up")"#, "")],
        found(),
        storage,
    );
    named.frame();
    named.frame_with(0.05, &[], &[typed("plugin", "list")]);
    assert_eq!(ids(&named), ["test.named"]);
    assert!(named.verbs.iter().any(|v| v == super::HOST_VERB));
    let said = named.printed_text();
    assert!(
        said.iter().any(|l| l.contains("test.named 1.0.0 running"))
            && !said.iter().any(|l| l.contains("test.extra")),
        "{said:?}"
    );
}

/// A plugin that replaces an earlier one (a new id) is turned on where the player had turned the
/// earlier one on, and opens its stores from the earlier one's where it has none of its own,
/// keeping them under its own id from then on.
/// Behaviour: plugins.load.a-plugin-that-replaces-another-keeps-what-the-player-had-under-it
#[test]
fn a_plugin_that_replaces_another_is_on_where_it_was_and_keeps_its_stores() {
    let storage = Shared::default();
    storage.0.borrow_mut().save(
        super::HOST_ID,
        super::TICKED_SCOPE,
        r#"{"test.oldname":true}"#,
    );
    storage.0.borrow_mut().save(
        "test.oldname",
        "character:unknown::Tester",
        r#"{"words":["spam"]}"#,
    );
    let src = r#"
        on("session.login", function()
            print("words", table.concat(store.character.words or {}, ","))
            store.character.seen = true
        end)
    "#;
    let found = || {
        vec![package(
            "test.newname",
            src,
            "replaces = [\"test.oldname\"]\n",
        )]
    };
    let mut h = Harness::new_with(Vec::new(), Some(found()), storage.clone());
    h.frame();
    let ids: Vec<String> = h.host.plugins().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, ["test.newname"], "on, as the earlier one was");
    assert!(h
        .host
        .found()
        .iter()
        .any(|b| b.id == "test.newname" && b.on));
    h.login();
    assert_eq!(
        h.console("test.newname"),
        ["words\tspam"],
        "the earlier one's store"
    );
    h.view.player = None;
    h.frame();
    let saved = storage.0.borrow().saved.clone();
    let own = saved
        .get(&(
            "test.newname".to_owned(),
            "character:unknown::Tester".to_owned(),
        ))
        .cloned()
        .unwrap_or_default();
    assert!(
        own.contains("spam") && own.contains("seen"),
        "kept under its own id: {own}"
    );
    assert_eq!(
        saved
            .get(&(
                "test.oldname".to_owned(),
                "character:unknown::Tester".to_owned()
            ))
            .map(String::as_str),
        Some(r#"{"words":["spam"]}"#),
        "the earlier one's is left as it was"
    );
    assert!(
        saved
            .get(&(super::HOST_ID.to_owned(), super::TICKED_SCOPE.to_owned()))
            .is_some_and(|t| t.contains("test.newname")),
        "on, remembered under its own id"
    );
}
