//! Horizon's extras in the shell against the server double: the host's pad playing the game in
//! gamepad mode; the log-out key's question; Horizon's own settings and HUD layout kept in the
//! player's files and read back in the next session; and a plugin the shell runs, its window
//! drawn by Horizon.

use auberean_client_sdk::contract::control::movement;
use auberean_horizon::ui::layout::{element, HudLayout};
use auberean_horizon::ui::settings::{Settings, SCALES};
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::pad::{PadButton, PadState};

use auberean_shell::interface::Phase;

use crate::session::{Folder, Session};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// A key pressed and let go, a frame after each.
fn press(s: &mut Session, key: Key) {
    for down in [true, false] {
        s.shell.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
        s.step();
    }
}

/// The left button pressed at `from`, the pointer moved to `to`, and let go there, a frame after
/// each.
fn drag(s: &mut Session, from: (f32, f32), to: (f32, f32)) {
    s.shell.event(&Event::Pointer {
        x: from.0,
        y: from.1,
    });
    s.step();
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: true,
    });
    s.step();
    s.shell.event(&Event::Pointer { x: to.0, y: to.1 });
    s.step();
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: false,
    });
    s.step();
}

fn centre(r: auberean_horizon::draw::Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Whether the client holds the forward control in force.
fn forward_held(s: &Session) -> bool {
    s.shell
        .client
        .as_ref()
        .expect("the session")
        .controls
        .is_down(movement::FORWARD)
}

/// In the world under Horizon the host's pad, read by the shell once a frame, plays the game: a
/// first press switches gamepad mode on and does nothing else, the left stick pushed ahead holds
/// the forward control in the client as a key would, and a key pressed switches gamepad mode off
/// and lets go of it.
///
/// Behaviour: desktop.horizon.the-host-s-pad-plays-the-game-through-the-game-s-controls
#[test]
fn the_hosts_pad_plays_the_game_in_gamepad_mode_and_a_key_switches_it_off() {
    let mut s = Session::new(Some(HORIZON));
    s.enter_the_world();
    s.step();
    let ahead = PadState {
        left: (0.0, 1.0),
        ..PadState::default()
    };
    s.shell.host.pad = Some(PadState::holding(&[PadButton::Start]));
    s.step();
    assert!(
        !s.horizon().borrow().ui().settings.is_open(),
        "the waking press does nothing else"
    );
    s.shell.host.pad = Some(PadState::default());
    s.step();
    s.shell.host.pad = Some(ahead);
    s.step();
    s.step();
    assert!(forward_held(&s), "the stick holds forward");
    s.shell.event(&Event::Key {
        key: Key::Q,
        down: true,
        repeat: false,
    });
    s.step();
    s.step();
    assert!(
        !forward_held(&s),
        "a key switched gamepad mode off and let go"
    );
    // Out of gamepad mode the stick does nothing until a press switches it on again.
    s.step();
    assert!(!forward_held(&s));
}

/// In the world under Horizon the log-out key (Shift+Escape) asks first, in the game's own words
/// from its string table; nothing is sent and no wait shown until Yes, and Yes asks the server to
/// leave the world and shows the wait.
///
/// Behaviour: desktop.horizon.log-out-asks-first-in-the-game-s-words
#[test]
fn horizons_log_out_key_asks_first_in_the_games_words_and_yes_asks_to_leave() {
    let mut s = Session::new(Some(HORIZON));
    s.enter_the_world();
    s.step();
    for (key, down) in [
        (Key::ShiftLeft, true),
        (Key::Escape, true),
        (Key::Escape, false),
        (Key::ShiftLeft, false),
    ] {
        s.shell.event(&Event::Key {
            key,
            down,
            repeat: false,
        });
        s.step();
    }
    let (words, drawn) = {
        let h = s.horizon();
        let h = h.borrow();
        let q = &h.ui().log_out;
        assert!(q.is_up(), "asked");
        (q.text.clone(), q.drawn().cloned())
    };
    assert_eq!(
        words, "Are you sure you want to log out?",
        "the game's words"
    );
    for _ in 0..30 {
        s.step();
    }
    assert!(!logged_off(&s), "nothing sent before the answer");
    assert!(
        !s.horizon().borrow().logging_out(),
        "no wait shown before the answer"
    );
    assert_eq!(s.shell.phase(), Phase::World);
    let (_, buttons) = drawn.expect("drawn");
    s.click(centre(buttons[0]));
    let asked = s.until(2.0, logged_off);
    assert!(asked, "Yes asks the server to leave the world");
    assert!(
        s.horizon().borrow().logging_out(),
        "and the wait shows after Yes"
    );
}

/// Whether the client asked the server to take the character out of the world.
fn logged_off(s: &Session) -> bool {
    s.shell
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .any(|m| m.message.message_type() == LOG_OFF)
}

/// The login server's message that takes the character out of the world.
const LOG_OFF: u32 = 0x10027;

/// Horizon keeps its own settings and each character's HUD layout in the player's files: the
/// scale chosen in its settings, and the layout saved in its editor, are written there, and read
/// back when Horizon is next shown and the character is next in the world.
///
/// Behaviour: desktop.horizon.its-scale-and-hud-layout-are-kept-in-the-player-s-files
#[test]
fn horizons_scale_and_hud_layout_are_kept_in_the_players_files_and_read_back() {
    let folder = Folder::default();
    let mut s = Session::new(Some(HORIZON));
    s.shell.host.folder = Some(folder.clone());
    s.enter_the_world();
    s.step();
    // The scale: 100% chosen in the settings.
    press(&mut s, Key::K);
    let k = auberean_horizon::scale::layout_scale((1920.0, 1080.0));
    let body = s.horizon().borrow().ui().settings.body().expect("open");
    let c = Settings::controls(body, k);
    #[allow(clippy::cast_precision_loss)]
    let hundred = (
        c.scale.x + c.scale.w / SCALES.len() as f32 + 9.0 * k,
        c.scale.y + c.scale.h / 2.0,
    );
    s.click(hundred);
    s.step();
    let kept = |name: &str| folder.0.lock().unwrap().get(name).cloned();
    assert_eq!(
        kept(auberean_horizon::own::FILE).as_deref(),
        Some(b"scale 1\nvista random\n".as_slice())
    );
    // The HUD layout: the menu button moved in the editor and saved.
    s.click(centre(c.layout));
    s.step();
    assert!(s.horizon().borrow().ui().layout.editing());
    let menu = s
        .horizon()
        .borrow()
        .ui()
        .layout
        .outlines
        .iter()
        .find(|(n, _)| *n == element::MENU)
        .map(|(_, r)| *r)
        .expect("outlined");
    let from = centre(menu);
    drag(&mut s, from, (from.0 - 300.0 * k, from.1 - 200.0 * k));
    let body = s
        .horizon()
        .borrow()
        .ui()
        .layout_editor_body()
        .expect("open");
    s.click(centre(HudLayout::controls(body, k).save));
    s.step();
    let menu_at = |s: &Session| {
        s.horizon()
            .borrow()
            .ui()
            .layout
            .outlines
            .iter()
            .find(|(n, _)| *n == element::MENU)
            .map(|(_, r)| *r)
            .expect("outlined")
    };
    let moved = menu_at(&s);
    let file = folder
        .0
        .lock()
        .unwrap()
        .keys()
        .find(|n| n.starts_with("auberean-horizon-hud-"))
        .cloned()
        .expect("the layout kept");
    assert!(file.ends_with("-Rowan.txt"), "{file}");
    let text = String::from_utf8(kept(&file).unwrap()).unwrap();
    assert!(
        text.starts_with("menu ") && text.ends_with(" 1.00 rb\n"),
        "kept by the right edge and the foot: {text}"
    );
    // The next session reads both back.
    let mut next = Session::new(Some(HORIZON));
    next.shell.host.folder = Some(folder.clone());
    next.enter_the_world();
    next.step();
    next.step();
    assert_eq!(next.horizon().borrow().ui().settings.own.scale, Some(1.0));
    let placed = menu_at(&next);
    assert!(
        (placed.x - moved.x).abs() < 0.5 && (placed.y - moved.y).abs() < 0.5,
        "{placed:?} where it was kept, {moved:?}"
    );
}

/// A plugin for the tests: a window with a line and a button, the line set as the player enters
/// the world; `/hello` prints a line and shows the window; the button prints another.
const HELLO: &str = r#"
local win = ui.window({
    id = "main",
    title = "Hello",
    width = 20,
    ui.label({ id = "where", text = "Not yet." }),
    ui.button({ id = "wave", text = "Wave" }),
})

on("session.enter_world", function(e)
    win:get("where").text = "In the world."
end)

on("ui.clicked", function(e)
    if e.id == "wave" then
        chat.print("Waved.")
    end
end)

commands.add("hello", "/hello shows the window", function()
    chat.print("Hello there.")
    win:show()
end)
"#;

const HELLO_MANIFEST: &str = r#"
id = "example.hello"
name = "Hello"
version = "0.1.0"
api = "0.7"
game = "auberean"
entry = "main.lua"
contexts = ["pregame", "world"]

[capabilities]
optional = ["ui.window"]
"#;

/// A clock that stands still: no plugin's entry takes any time.
#[derive(Debug, Default)]
struct Still;

impl auberean_client_sdk::plugins::VmClock for Still {
    fn now(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}

/// The plugin host with the test's plugin asked to load.
fn hello_host() -> auberean_client_sdk::plugins::PluginHost {
    use auberean_client_sdk::plugins::{bindings, HostConfig, MemoryStorage, Package, PluginHost};
    let mut host = PluginHost::new(
        HostConfig::new("auberean", "test"),
        Box::<Still>::default(),
        Box::<MemoryStorage>::default(),
    )
    .with_engine(Box::new(bindings::engine()));
    let mut files = std::collections::BTreeMap::new();
    files.insert("plugin.toml".to_owned(), HELLO_MANIFEST.as_bytes().to_vec());
    files.insert("main.lua".to_owned(), HELLO.as_bytes().to_vec());
    host.request_load(Package {
        origin: "hello".into(),
        files,
    });
    host
}

/// Whether Horizon's log has a line holding `text`.
fn logged(s: &Session, text: &str) -> bool {
    s.horizon()
        .borrow()
        .state()
        .log
        .iter()
        .any(|l| l.text.contains(text))
}

/// The chat lines the client sent the server so far.
fn chats(s: &Session) -> usize {
    s.shell
        .client
        .as_ref()
        .expect("the session")
        .sent()
        .iter()
        .filter(|m| {
            matches!(
                m.message,
                auberean_client_sdk::protocol::ClientMessage::Chat(_)
            )
        })
        .count()
}

/// The desktop shell runs the plugins its product hands it as the session's frame hook: a plugin
/// hears the player enter the world; a line typed in Horizon that the plugin claims is the
/// plugin's and never said; what the plugin prints shows in Horizon's log in the plugins' own
/// color, not as an error; the window it shows is drawn by Horizon, and a press on its button
/// reaches the plugin.
///
/// Behaviour: desktop.horizon.the-shell-s-plugins-hear-the-game-and-horizon-draws-their-windows
#[test]
fn the_shells_plugins_hear_the_game_and_horizon_draws_their_windows() {
    let mut s = Session::new(Some(HORIZON));
    s.shell.set_plugins(hello_host());
    s.enter_the_world();
    for _ in 0..10 {
        s.step();
    }
    let where_text = |s: &Session| {
        s.shell_window("example.hello", "main")
            .and_then(|w| w.tree.find("where").map(|n| n.text("text").to_owned()))
    };
    assert_eq!(
        where_text(&s).as_deref(),
        Some("In the world."),
        "the plugin heard the entry"
    );
    let said = chats(&s);
    press(&mut s, Key::Enter);
    s.shell.event(&Event::Text("/hello".into()));
    s.step();
    press(&mut s, Key::Enter);
    assert!(
        s.until(2.0, |s| logged(s, "Hello there.")),
        "the plugin's line in the log"
    );
    assert_eq!(chats(&s), said, "the line was the plugin's, never said");
    let kind = s
        .horizon()
        .borrow()
        .state()
        .log
        .iter()
        .find(|l| l.text.contains("Hello there."))
        .map(|l| l.kind);
    assert_eq!(
        kind,
        Some(auberean_horizon::state::PLUGIN_TEXT),
        "in the plugins' own kind of text, not an error"
    );
    assert!(
        s.until(1.0, |s| !s
            .horizon()
            .borrow()
            .ui()
            .plugins
            .rects()
            .is_empty()),
        "Horizon draws the window the command showed"
    );
    let wave = s
        .horizon()
        .borrow()
        .ui()
        .plugins
        .node_rect("example.hello", "main", "wave")
        .expect("its button");
    s.click(centre(wave));
    assert!(
        s.until(2.0, |s| logged(s, "Waved.")),
        "the press reached the plugin"
    );
}
