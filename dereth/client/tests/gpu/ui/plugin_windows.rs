//! Plugin windows on the desktop, under each interface: a window a plugin declares is drawn over
//! the game, in the game's own window art and font under the modern interface, in the classic
//! interface's own 2005 art and font under it, and in Horizon's art under Horizon, and a click on
//! its button reaches the plugin; a HUD is drawn at the screen corner it names, with no frame or
//! title; the plugin bar's button closes the main window; the host's dialog for a file is drawn
//! beside the plugins in the game's frame, and the plugin panel as a window of each interface;
//! the Tank sample's window on its pages, a page scrolling under its tabs on a screen too short
//! for it; an unlocked HUD dragged where the player puts it.
//! Fixture: a headless `App` in gameplay on
//! the retail dats with a plugin host installed and plugin packages built in memory; the pointer
//! is driven through the window's own events, and the frame is read back from the device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_contract::options::interface::{Interface, INTERFACE};
use dereth_client_contract::PrefValue;
use dereth_client_runtime::config::Config;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_plugin_api::HostCapability;
use dereth_plugin_host::{
    HostConfig, MemoryFiles, MemoryStorage, Package, PluginHook, PluginHost, StaticSource,
};
use dereth_ui::framework::mode;

const MAIN: &str = r#"
local win = ui.window{ id = "main", title = "Plugin",
    ui.label{ id = "said", text = "no" },
    ui.button{ id = "go", text = "Go" },
    ui.toggle{ id = "auto", text = "Auto", value = true },
    ui.tabs{ id = "pages", selected = 2,
        ui.group{ title = "First", ui.label{ text = "first page" } },
        ui.group{ title = "Second",
            ui.number{ id = "range", text = "Range", value = 8, min = 0, max = 10, step = 2 },
            ui.choice{ id = "mode", text = "Mode", options = { "stop", "recall" }, value = "stop" },
        },
    },
}
local hud = ui.hud{ id = "hud", anchor = "top_right", ui.label{ id = "line", text = "HUD here" } }
on("ui.clicked", function(e) if e.id == "go" then win:get("said").text = "yes" end end)
on("ui.opened", function(e) hud:show() end)
"#;

const HUD: &str = r#"
ui.hud{ id = "hud", anchor = "top_right", ui.label{ id = "line", text = "HUD here" } }
"#;

/// A window of every kind of box a plugin window holds: fields, a disabled field, a list in a
/// group, a picture's place.
const RICH: &str = r#"
local rows = {}
for i = 1, 12 do rows[i] = { "Pyreal Mote " .. i, i * 100, "salvage" } end
local win = ui.window{ id = "main", title = "Plugin",
    ui.label{ id = "said", text = "Status: idle" },
    ui.row{ ui.button{ id = "go", text = "Start" }, ui.button{ id = "stop", text = "Stop" },
        ui.toggle{ id = "auto", text = "Auto", value = true } },
    ui.number{ id = "range", text = "Range", value = 8, min = 0, max = 10 },
    ui.choice{ id = "mode", text = "Mode", options = { "stop", "recall" }, value = "stop" },
    ui.input{ id = "name", text = "Name", value = "Tester" },
    ui.input{ id = "off", text = "Off", value = "x", enabled = false },
    ui.group{ title = "Loot",
        ui.list{ id = "loot", columns = { "Item", "Value", "Rule" }, rows = rows, height = 5 } },
    ui.image{ id = "map", src = "map.png", text = "no map", width = 12, height = 3 },
}
on("ui.clicked", function(e) if e.id == "go" then win:get("said").text = "yes" end end)
"#;

/// The plain frame's edge: the plugin bar's.
const EDGE: (u8, u8, u8) = (0x8C, 0x7A, 0x4E);
/// The game's text in the plugin windows under the modern interface, where a letter covers the
/// pixel whole.
const TEXT: (u8, u8, u8) = (0xE8, 0xD8, 0xA8);
/// The classic interface's text.
const CLASSIC_TEXT: (u8, u8, u8) = (0xD2, 0xD2, 0xC8);
/// The bronze of the game's panels' left edge (the data's 0x060074C0), its two drawn columns.
const RETAIL_EDGE: [(u8, u8, u8); 2] = [(103, 85, 59), (144, 120, 84)];

fn host() -> PluginHost {
    host_of(MAIN)
}

fn host_of(main: &str) -> PluginHost {
    let files = [
        (
            "plugin.toml".to_owned(),
            // The fixture has no character in the world: the plugin is active at any screen.
            b"id = \"test.window\"\nname = \"Window\"\nversion = \"1.0.0\"\napi = \"0.2\"\ncontexts = [\"pregame\", \"world\"]\n"
                .to_vec(),
        ),
        ("main.lua".to_owned(), main.as_bytes().to_vec()),
    ];
    let package = Package {
        origin: "test".into(),
        files: files.into_iter().collect(),
    };
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            HostCapability::Core,
            HostCapability::UiWindow,
            HostCapability::UiHud,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(dereth_plugin_lua::LuauEngine),
        Box::new(StaticSource {
            packages: vec![package],
        }),
        Box::new(MemoryStorage::default()),
    );
    host.load_all();
    host
}

fn hook(app: &mut App) -> &mut PluginHook {
    app.frame_hook_mut()
        .and_then(|h| h.as_any_mut().downcast_mut::<PluginHook>())
        .expect("the plugin host is installed")
}

fn said(app: &mut App) -> String {
    hook(app)
        .host
        .window("test.window", "main")
        .map(|w| w.tree.dump())
        .unwrap_or_default()
}

fn main_open(app: &mut App) -> Option<bool> {
    hook(app).host.window("test.window", "main").map(|w| w.open)
}

/// A headless `App` in gameplay on the retail dats, with `host` installed, eight frames run and
/// each plugin's main window opened from the plugin bar, the older portal beside the dats when
/// `classic` (the classic interface draws from it).
fn gameplay_with(host: PluginHost, classic: bool) -> App {
    gameplay_at(host, classic, (800, 600))
}

/// As [`gameplay_with`], on a display `size` pixels across and down.
fn gameplay_at(host: PluginHost, classic: bool, (width, height): (u32, u32)) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's fixture: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let classic_dat_dir = classic.then(|| {
        if let Some(why) = dereth_dat::testing::classic_shortfall() {
            panic!("the classic interface's files are this test's fixture: {why}");
        }
        dereth_dat::testing::classic_dat_dir().unwrap_or_default()
    });
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        classic_dat_dir,
        width,
        height,
        ..Config::default()
    })
    .unwrap_or_else(|e| panic!("a headless App on a GPU device: {e:?}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell starts: {e:?}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first scene loads: {e:?}"));
    app.queue_ui_mode(mode::GAME_PLAY);
    app.install_frame_hook(Box::new(PluginHook { host }));
    for _ in 0..8 {
        app.frame();
    }
    // The player opens each plugin's main window from the plugin bar: a plugin's windows open
    // only when the player asks.
    for entry in hook(&mut app).host.bar() {
        if entry.window.is_some() && !entry.open {
            hook(&mut app)
                .host
                .toggle_window(&entry.id)
                .unwrap_or_else(|e| panic!("{} is on the bar: {e}", entry.id));
        }
    }
    for _ in 0..4 {
        app.frame();
    }
    app
}

/// Show `interface`, as the options page's choice does, and run frames until it is shown (Horizon
/// waits for its art).
fn choose(app: &mut App, interface: Interface) {
    dereth_client_contract::options::store::set_value(INTERFACE, PrefValue::Int(interface.value()));
    for _ in 0..600 {
        app.frame();
        if app.shown_interface() == interface {
            break;
        }
    }
    assert_eq!(app.shown_interface(), interface, "the interface is shown");
    for _ in 0..4 {
        app.frame();
    }
}

/// A left click at `(x, y)`, through the window's events, a frame for each.
fn click(app: &mut App, (x, y): (f32, f32)) {
    app.queue_window_event(HostEvent::CursorMoved {
        x: f64::from(x),
        y: f64::from(y),
    });
    app.frame();
    for pressed in [true, false] {
        app.queue_window_event(HostEvent::MouseInput {
            button: MouseButton::Left,
            pressed,
        });
        app.frame();
    }
    app.frame();
}

/// The panel titled `title` the shown interface noted last frame: where it is, and its first
/// control.
struct Panel {
    rect: (f32, f32, f32, f32),
    first: (f32, f32),
}

fn panel(app: &App, title: &str) -> Panel {
    let snap = app.kit_controls();
    let (i, layer) = snap
        .layers
        .iter()
        .enumerate()
        .find(|(_, l)| l.name == title)
        .unwrap_or_else(|| {
            panic!(
                "no panel {title:?} is drawn: {:?}",
                snap.layers.iter().map(|l| &l.name).collect::<Vec<_>>()
            )
        });
    let first = snap
        .targets
        .iter()
        .find(|t| t.layer == i + 1)
        .map(|t| (t.rect.x + t.rect.w / 2.0, t.rect.y + t.rect.h / 2.0))
        .expect("its first control is noted");
    let r = layer.rect;
    Panel {
        rect: (r.x, r.y, r.w, r.h),
        first,
    }
}

/// The frame as the device drew it, kept as a PNG under `DERETH_TEST_PLUGIN_WINDOWS_DIR` when it
/// is set: width, height and BGRA bytes.
fn capture(app: &mut App, name: &str) -> (u32, u32, Vec<u8>) {
    let (w, h, bgra) = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame reads back");
    if let Ok(dir) = std::env::var("DERETH_TEST_PLUGIN_WINDOWS_DIR") {
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let file = std::fs::File::create(std::path::Path::new(&dir).join(format!("{name}.png")))
            .expect("png file");
        let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .expect("png header")
            .write_image_data(&rgba)
            .expect("png pixels");
    }
    (w, h, bgra)
}

/// The colour at `(x, y)` of a captured frame `w` wide.
fn px(frame: &(u32, u32, Vec<u8>), x: f32, y: f32) -> (u8, u8, u8) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y) = (x as u32, y as u32);
    let i = ((y * frame.0 + x) * 4) as usize;
    (frame.2[i + 2], frame.2[i + 1], frame.2[i])
}

fn near(a: (u8, u8, u8), b: (u8, u8, u8)) -> bool {
    a.0.abs_diff(b.0) <= 2 && a.1.abs_diff(b.1) <= 2 && a.2.abs_diff(b.2) <= 2
}

/// Whether the first pixels in from `(x, y)` are the classic inventory's pack rule (the classic
/// portal's 0x060011BA, three columns such as (128, 78, 7), (190, 115, 29), (113, 55, 0)): an
/// orange column brighter than one either side of it. Its colours change down its length, a grain
/// in the wood, so its shape is what is looked for, not one row's colours.
fn classic_rule_at(frame: &(u32, u32, Vec<u8>), x: f32, y: f32) -> bool {
    (1..6_u8).any(|dx| {
        let [a, b, c] = [-1.0, 0.0, 1.0].map(|d| px(frame, x + f32::from(dx) + d, y));
        let orange = |p: (u8, u8, u8)| p.0 > p.2.saturating_add(60) && p.0 > p.1;
        orange(a)
            && orange(b)
            && orange(c)
            && b.0 > a.0.saturating_add(40)
            && b.0 > c.0.saturating_add(40)
    })
}

/// As [`classic_rule_at`], for a frame drawn at a place between pixels (a window the host centres):
/// its columns blend into their neighbours, so the middle one stands out by less, and the rule may
/// start at `x` itself.
fn classic_rule_blended(frame: &(u32, u32, Vec<u8>), x: f32, y: f32) -> bool {
    (0..6_u8).any(|dx| {
        let [a, b, c] = [-1.0, 0.0, 1.0].map(|d| px(frame, x + f32::from(dx) + d, y));
        let orange = |p: (u8, u8, u8)| p.0 > p.2.saturating_add(60) && p.0 > p.1;
        orange(a)
            && orange(b)
            && orange(c)
            && b.0 > a.0.saturating_add(20)
            && b.0 > c.0.saturating_add(20)
    })
}

/// Whether the first pixels in from `(x, y)` are one of `colours`: the frame's own edge.
fn edge_of(frame: &(u32, u32, Vec<u8>), x: f32, y: f32, colours: &[(u8, u8, u8)]) -> bool {
    (0..6_u8).any(|dx| {
        colours
            .iter()
            .any(|c| near(px(frame, x + f32::from(dx), y), *c))
    })
}

/// How many pixels of `(x, y, w, h)` are the modern interface's plugin text colour: the letters
/// drawn there.
fn text_pixels(frame: &(u32, u32, Vec<u8>), rect: (f32, f32, f32, f32)) -> usize {
    text_pixels_in(frame, rect, TEXT)
}

/// How many pixels of `(x, y, w, h)` are `text`: the letters drawn there.
#[allow(clippy::cast_precision_loss)]
fn text_pixels_in(
    frame: &(u32, u32, Vec<u8>),
    (x, y, w, h): (f32, f32, f32, f32),
    text: (u8, u8, u8),
) -> usize {
    let mut n = 0;
    let mut sy = y.max(0.0);
    while sy < (y + h).min(frame.1 as f32) {
        let mut sx = x.max(0.0);
        while sx < (x + w).min(frame.0 as f32) {
            if near(px(frame, sx, sy), text) {
                n += 1;
            }
            sx += 1.0;
        }
        sy += 1.0;
    }
    n
}

/// Behaviour: plugins.ui.a-plugin-hud-is-drawn-at-its-corner-without-a-frame
#[test]
fn a_plugin_hud_is_drawn_at_its_corner_without_a_frame() {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(host_of(HUD), false);
    let frame = capture(&mut app, "plugin-hud");
    let hud = app
        .plugin_layer()
        .ui
        .rect_of("test.window", "hud")
        .expect("the HUD is drawn");
    #[allow(clippy::cast_precision_loss)]
    let width = frame.0 as f32;
    assert!(
        hud.x > width / 2.0 && hud.y < 40.0,
        "the HUD sits at the top right, not at {},{} of a {width}-wide frame",
        hud.x,
        hud.y
    );
    let edge: Vec<(u8, u8, u8)> = (0..4_u8)
        .map(|i| px(&frame, hud.x, hud.y + hud.h * (0.2 + 0.2 * f32::from(i))))
        .collect();
    assert!(
        !edge.iter().any(|c| near(*c, EDGE)),
        "a HUD draws no frame: its left edge is {edge:?}"
    );
    assert!(
        app.kit_controls().layers.is_empty(),
        "a HUD is no window: it has no title bar or panel"
    );
    let letters = text_pixels(&frame, (hud.x, hud.y, hud.w, hud.h));
    assert!(
        letters > 10,
        "the HUD's line is drawn: {letters} pixels of its letters"
    );
}

/// Behaviour: plugins.ui.a-plugin-window-is-drawn-over-the-game-and-its-button-reaches-the-plugin
#[test]
fn a_plugin_window_is_drawn_over_the_game_and_its_button_reaches_the_plugin() {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(host(), false);
    let window = panel(&app, "Plugin");
    let frame = capture(&mut app, "plugin-window");
    let (x, y, _, h) = window.rect;
    assert!(
        edge_of(&frame, x, y + h / 2.0, &RETAIL_EDGE),
        "the game's own frame is drawn where the window is: {:?}",
        px(&frame, x + 2.0, y + h / 2.0)
    );
    let letters = text_pixels(&frame, window.rect);
    assert!(
        letters > 40,
        "its text is drawn: {letters} pixels of its letters"
    );
    click(&mut app, window.first);
    assert!(
        said(&mut app).contains("label #said text=\"yes\""),
        "{}",
        said(&mut app)
    );
}

/// Behaviour: plugins.ui.the-plugin-bar-is-drawn-and-a-click-on-its-button-closes-the-main-window
#[test]
fn the_plugin_bar_is_drawn_and_a_click_on_its_button_closes_the_main_window() {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(host(), false);
    assert_eq!(main_open(&mut app), Some(true));
    // The bar's one button, the plugin's, at the bar's first place in the top left.
    let bar = app.plugin_layer().bar_rect().expect("the bar is drawn");
    assert!(
        bar.x < 60.0 && bar.y < 30.0,
        "the bar sits at the top left, not {},{}",
        bar.x,
        bar.y
    );
    let frame = capture(&mut app, "plugin-bar");
    let edge = px(&frame, bar.x, bar.y + bar.h / 2.0);
    assert!(near(edge, EDGE), "the bar's frame is drawn: {edge:?}");
    // The plugin's button, short of the grip at the bar's right end.
    click(&mut app, (bar.right() - 20.0, bar.y + bar.h / 2.0));
    assert_eq!(
        main_open(&mut app),
        Some(false),
        "the click closes the main window"
    );
}

/// Behaviour: plugins.interfaces.under-classic-a-plugin-window-and-hud-are-drawn-on-the-device
#[test]
fn under_the_classic_interface_a_plugin_window_and_hud_are_drawn_in_its_own_art_and_its_button_reaches_the_plugin(
) {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(host(), true);
    choose(&mut app, Interface::Classic);
    let window = panel(&app, "Plugin");
    let frame = capture(&mut app, "plugin-window-classic");
    let (x, y, _, h) = window.rect;
    assert!(
        classic_rule_at(&frame, x, y + h / 2.0),
        "the classic frame is drawn over the classic interface: {:?}",
        (0..6_u8)
            .map(|dx| px(&frame, x + f32::from(dx), y + h / 2.0))
            .collect::<Vec<_>>()
    );
    let letters = text_pixels_in(&frame, window.rect, CLASSIC_TEXT);
    assert!(
        letters > 40,
        "its text is drawn: {letters} pixels of its letters"
    );
    assert!(
        app.plugin_layer()
            .ui
            .rect_of("test.window", "hud")
            .is_some(),
        "and the HUD"
    );
    click(&mut app, window.first);
    assert!(
        said(&mut app).contains("label #said text=\"yes\""),
        "{}",
        said(&mut app)
    );
}

/// Behaviour: plugins.interfaces.under-horizon-a-plugin-window-is-drawn-on-the-device-in-its-art
#[test]
fn under_horizon_a_plugin_window_is_drawn_in_its_art_its_button_reaches_the_plugin_and_closed_it_is_gone(
) {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(host(), false);
    choose(&mut app, Interface::Horizon);
    let window = panel(&app, "Plugin");
    let open = capture(&mut app, "plugin-window-horizon");
    click(&mut app, window.first);
    assert!(
        said(&mut app).contains("label #said text=\"yes\""),
        "{}",
        said(&mut app)
    );
    // The fixture has no character: Horizon is before the world, where the plugins' windows are
    // drawn over its screens. Closed (as the bar's button closes it), the window is gone.
    hook(&mut app)
        .host
        .toggle_window("test.window")
        .expect("the window closes");
    assert_eq!(main_open(&mut app), Some(false));
    for _ in 0..4 {
        app.frame();
    }
    let closed = capture(&mut app, "plugin-window-horizon-closed");
    // Where the window stood, the frame is not the same once it is closed.
    let (x, y, w, h) = window.rect;
    let mut changed = 0;
    let mut seen = 0;
    for j in 1..10_u8 {
        for i in 1..10_u8 {
            let (sx, sy) = (x + w * f32::from(i) / 10.0, y + h * f32::from(j) / 10.0);
            seen += 1;
            let (a, b) = (px(&open, sx, sy), px(&closed, sx, sy));
            let d = |x: u8, y: u8| u16::from(x.abs_diff(y));
            if d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 24 {
                changed += 1;
            }
        }
    }
    assert!(
        changed * 2 > seen,
        "the window was drawn where it stood: {changed} of {seen} samples changed when it closed"
    );
}

/// Under Horizon, on a display of 800 by 600 (its layouts shrunk to fit), of 1920 by 1080 and of
/// 3840 by 2160 (drawn at twice the size), a plugin window holding fields, a list in a group and a
/// picture's place is drawn whole on the screen, and its button, where the window is drawn,
/// reaches the plugin.
/// Behaviour: plugins.interfaces.under-horizon-a-plugin-window-is-drawn-on-the-device-in-its-art
#[test]
fn under_horizon_a_plugin_window_of_fields_and_lists_is_drawn_whole_at_each_scale() {
    let _gpu = gpu_lock();
    for (w, h) in [(800_u32, 600_u32), (1920, 1080), (3840, 2160)] {
        let mut app = gameplay_at(host_of(RICH), false, (w, h));
        choose(&mut app, Interface::Horizon);
        let window = panel(&app, "Plugin");
        let (x, y, ww, wh) = window.rect;
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (w as f32, h as f32);
        assert!(
            x >= 0.0 && y >= 0.0 && x + ww <= sw && y + wh <= sh,
            "{w}x{h}: the window is on the screen: {:?}",
            window.rect
        );
        capture(&mut app, &format!("plugin-window-horizon-{w}x{h}"));
        click(&mut app, window.first);
        assert!(
            said(&mut app).contains("label #said text=\"yes\""),
            "{w}x{h}: {}",
            said(&mut app)
        );
    }
}

/// A plugin host with a file to give: `test.pick` asks for one as it loads, and `test.cover` lays a
/// HUD over the top left of the screen.
fn picking_host() -> PluginHost {
    let package = |id: &str, main: &str| {
        let manifest = format!(
            "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.7\"\ncontexts = [\"pregame\", \"world\"]\n"
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
    };
    let cover = format!(
        "ui.hud{{ id = \"cover\", anchor = \"top_left\", {} }}",
        (0..6)
            .map(|i| format!("ui.label{{ text = \"{}{i}\" }}", "COVER ".repeat(4)))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let pick = r#"
task.spawn(function()
    local ok, f = pawait(files.import({ prompt = "A route to walk." }))
    print("given", ok, ok and f.name or f)
end)
"#;
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            HostCapability::Core,
            HostCapability::UiWindow,
            HostCapability::UiHud,
        ],
    );
    let mut files = MemoryFiles::default();
    for name in ["import/east.nav", "import/west.nav"] {
        files
            .files
            .insert(("dereth.plugins".into(), name.into()), b"route".to_vec());
    }
    let mut host = PluginHost::new(
        config,
        Box::new(dereth_plugin_lua::LuauEngine),
        Box::new(StaticSource {
            packages: vec![package("test.pick", pick), package("test.cover", &cover)],
        }),
        Box::new(MemoryStorage::default()),
    )
    .with_files(Box::new(files));
    host.load_all();
    host
}

/// Under the modern interface the host's dialog for a file is drawn in the game's own frame, as a
/// plugin window is, with the plugins' HUD still drawn beside it; the player's pick and its first
/// button give the plugin the file.
/// Behaviour: plugins.files.the-hosts-dialog-for-a-file-is-drawn-in-the-games-frame-beside-the-plugins
#[test]
fn the_hosts_dialog_for_a_file_is_drawn_in_the_games_frame_beside_the_plugins_and_its_buttons_answer(
) {
    let _gpu = gpu_lock();
    let mut app = gameplay_with(picking_host(), false);
    assert!(app.plugin_layer().asking(), "the dialog is up");
    let hud = app
        .plugin_layer()
        .ui
        .rect_of("test.cover", "cover")
        .expect("the HUD is still drawn");
    let drawn = app.plugin_layer().host.asking.drawn.clone();
    let dialog = drawn.frame;
    let frame = capture(&mut app, "plugin-file-dialog");
    assert!(
        edge_of(&frame, dialog.x, dialog.y + dialog.h / 2.0, &RETAIL_EDGE),
        "the game's own frame is drawn where the dialog is: {:?}",
        px(&frame, dialog.x + 2.0, dialog.y + dialog.h / 2.0)
    );
    let beside = text_pixels(&frame, (hud.x, hud.y, hud.w, hud.h));
    assert!(
        beside > 10,
        "the HUD's lines are drawn: {beside} pixels of its letters"
    );
    let (_, row) = drawn.picks[1];
    click(&mut app, (row.x + 8.0, row.y + row.h / 2.0));
    let give = drawn.buttons[0];
    click(&mut app, (give.x + give.w / 2.0, give.y + give.h / 2.0));
    for _ in 0..3 {
        app.frame();
    }
    let console = hook(&mut app).host.console("test.pick").unwrap_or_default();
    assert_eq!(console, ["given\ttrue\twest.nav"]);
    assert!(!app.plugin_layer().asking(), "the dialog is gone");
}

/// The Tank sample read from its folder, active before the world (the fixture has no character)
/// and asking nothing of the host's movement, with a few lines in its log.
fn tank_host() -> PluginHost {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/tank");
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the Tank's folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .strip_prefix(&root)
                .expect("under the folder")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(name, std::fs::read(&path).expect("a file"));
        }
    }
    let manifest = String::from_utf8(files["plugin.toml"].clone())
        .expect("text")
        .replace(
            "contexts = [\"world\"]",
            "contexts = [\"pregame\", \"world\"]",
        )
        .replace(
            "required = [\"nav.moveto\", \"ui.window\"]",
            "required = [\"ui.window\"]",
        );
    files.insert("plugin.toml".into(), manifest.into_bytes());
    let mut main = files["main.lua"].clone();
    main.extend_from_slice(
        b"\nfor i = 1, 14 do E.note(\"a line of the run's log, number %d, longer than the others\", i) end\nwindow.refresh(E)\n",
    );
    files.insert("main.lua".into(), main);
    let config = HostConfig::new(
        "dereth-desktop",
        vec![
            HostCapability::Core,
            HostCapability::UiWindow,
            HostCapability::UiHud,
        ],
    );
    let mut host = PluginHost::new(
        config,
        Box::new(dereth_plugin_lua::LuauEngine),
        Box::new(StaticSource {
            packages: vec![Package {
                origin: "tank".into(),
                files,
            }],
        }),
        Box::new(MemoryStorage::default()),
    );
    host.load_all();
    host
}

/// The Tank's window is drawn whole on the screen under each interface, on its Run, Combat, Items,
/// Nav, Route, Loot, Vitals, Vendor and Meta pages; on a screen too short for its Vitals page, the
/// page scrolls through it under its row of tabs (under Horizon, which draws everything smaller on
/// a short screen, the page fits).
/// Behaviour: plugins.ui.with-tabs-only-the-pages-content-scrolls
#[test]
fn the_tanks_window_is_drawn_on_its_pages_under_each_interface_and_scrolls_on_a_short_screen() {
    let _gpu = gpu_lock();
    // The pages shown, and each one's place among the window's tabs.
    let pages = [
        ("run", 1),
        ("combat", 2),
        ("items", 4),
        ("nav", 5),
        ("route", 6),
        ("loot", 7),
        ("vitals", 8),
        ("vendor", 10),
        ("meta", 11),
    ];
    for interface in [Interface::Modern, Interface::Classic, Interface::Horizon] {
        for (screen, shown) in [((800, 600), &pages[..]), ((800, 360), &pages[6..7])] {
            let mut app = gameplay_at(tank_host(), interface == Interface::Classic, screen);
            if interface != Interface::Modern {
                choose(&mut app, interface);
            }
            for &(page, at) in shown {
                hook(&mut app)
                    .host
                    .ui_input(
                        "network.dereth.tank",
                        "main",
                        dereth_plugin_host::UiInput::Select {
                            node: "pages".into(),
                            rows: vec![at],
                        },
                    )
                    .expect("the page is chosen");
                for _ in 0..4 {
                    app.frame();
                }
                let window = panel(&app, "Tank");
                let short = screen.1 < 600;
                let shot = format!(
                    "tank-{}-{page}{}",
                    format!("{interface:?}").to_lowercase(),
                    if short { "-short" } else { "" }
                );
                let frame = capture(&mut app, &shot);
                let (x, y, w, h) = window.rect;
                #[allow(clippy::cast_precision_loss)]
                let (sw, sh) = (frame.0 as f32, frame.1 as f32);
                assert!(
                    x >= 0.0 && y >= 0.0 && x + w <= sw + 0.5 && y + h <= sh + 0.5,
                    "{interface:?} {page}: the Tank's window is on the screen: {:?}",
                    window.rect
                );
                // Horizon draws its whole interface smaller on a short screen, so the page fits.
                if short && interface != Interface::Horizon {
                    // The body scrolls: a list as wide as the window's body, its rows its controls.
                    let scrolling = app.kit_controls().lists.iter().any(|l| {
                        l.area.x >= x - 0.5
                            && l.area.right() <= x + w + 0.5
                            && l.area.w > w * 0.8
                            && l.area.y > y
                    });
                    assert!(
                        scrolling,
                        "{interface:?}: the Vitals page scrolls on a short screen: {:?}",
                        app.kit_controls().lists
                    );
                }
            }
        }
    }
}

/// The plugin panel (`/plugin`) is drawn as a window of each interface: in the game's own frame
/// under the modern interface, in the classic interface's own art under it, and in Horizon's
/// art under Horizon.
/// Behaviour: plugins.ui.the-plugin-panel-is-drawn-as-a-window-of-each-interface
#[test]
fn the_plugin_panel_is_drawn_as_a_window_of_each_interface() {
    let _gpu = gpu_lock();
    for interface in [Interface::Modern, Interface::Classic, Interface::Horizon] {
        let mut app = gameplay_with(host_of(HUD), interface == Interface::Classic);
        if interface != Interface::Modern {
            choose(&mut app, interface);
        }
        let name = format!("plugin-panel-{}", format!("{interface:?}").to_lowercase());
        let before = capture(&mut app, &format!("{name}-before"));
        hook(&mut app).host.show_panel();
        for _ in 0..4 {
            app.frame();
        }
        let window = panel(&app, "Plugins");
        let frame = capture(&mut app, &name);
        let (x, y, w, h) = window.rect;
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (frame.0 as f32, frame.1 as f32);
        assert!(
            x >= 0.0 && y >= 0.0 && x + w <= sw && y + h <= sh,
            "{interface:?}: the panel is on the screen: {:?}",
            window.rect
        );
        match interface {
            Interface::Modern => assert!(
                edge_of(&frame, x, y + h / 2.0, &RETAIL_EDGE),
                "the game's own frame is drawn where the panel is: {:?}",
                px(&frame, x + 2.0, y + h / 2.0)
            ),
            Interface::Classic => assert!(
                classic_rule_blended(&frame, x, y + h / 2.0),
                "the classic frame is drawn where the panel is: {:?}",
                (0..6_u8)
                    .map(|dx| px(&frame, x + f32::from(dx), y + h / 2.0))
                    .collect::<Vec<_>>()
            ),
            _ => {}
        }
        // Where the panel stands, the frame is not what it was before it opened.
        let mut changed = 0;
        let mut seen = 0;
        for j in 1..10_u8 {
            for i in 1..10_u8 {
                let (sx, sy) = (x + w * f32::from(i) / 10.0, y + h * f32::from(j) / 10.0);
                seen += 1;
                let (a, b) = (px(&before, sx, sy), px(&frame, sx, sy));
                let d = |x: u8, y: u8| u16::from(x.abs_diff(y));
                if d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2) > 24 {
                    changed += 1;
                }
            }
        }
        assert!(
            changed * 2 > seen,
            "{interface:?}: the panel is drawn where it stands: {changed} of {seen} samples changed"
        );
    }
}

/// A HUD with a button, at the screen's top right.
const HUD_BUTTON: &str = r#"
ui.hud{ id = "hud", anchor = "top_right", ui.label{ id = "line", text = "HUD here" },
    ui.button{ id = "ping", text = "Ping" } }
on("ui.clicked", function(e) print("clicked", e.id) end)
"#;

/// The left button pressed at `from`, the pointer moved to `to` over a few frames and let go,
/// through the window's events.
fn drag(app: &mut App, (fx, fy): (f32, f32), (tx, ty): (f32, f32)) {
    app.queue_window_event(HostEvent::CursorMoved {
        x: f64::from(fx),
        y: f64::from(fy),
    });
    app.frame();
    app.queue_window_event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    app.frame();
    for step in 1..=4_u8 {
        let t = f32::from(step) / 4.0;
        app.queue_window_event(HostEvent::CursorMoved {
            x: f64::from(fx + (tx - fx) * t),
            y: f64::from(fy + (ty - fy) * t),
        });
        app.frame();
    }
    app.queue_window_event(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    app.frame();
    app.frame();
}

/// With the HUDs unlocked, a plugin's HUD is outlined and dragged to where the player puts it,
/// its button taking no click, and the place is kept for the character; under the classic
/// interface too.
/// Behaviour: plugins.ui.unlocked-a-huds-place-is-the-players-kept-on-the-screen
#[test]
fn unlocked_a_plugins_hud_is_dragged_where_the_player_puts_it_and_kept() {
    let _gpu = gpu_lock();
    for interface in [Interface::Modern, Interface::Classic] {
        let name = format!(
            "plugin-hud-unlocked-{}",
            format!("{interface:?}").to_lowercase()
        );
        let mut app = gameplay_with(host_of(HUD_BUTTON), interface == Interface::Classic);
        if interface != Interface::Modern {
            choose(&mut app, interface);
        }
        capture(&mut app, &format!("{name}-locked"));
        hook(&mut app).host.set_huds_unlocked(true);
        for _ in 0..3 {
            app.frame();
        }
        let before = app
            .plugin_layer()
            .ui
            .rect_of("test.window", "hud")
            .expect("the HUD is drawn");
        let from = (before.x + before.w / 2.0, before.y + before.h - 6.0);
        drag(&mut app, from, (300.0, 320.0));
        let frame = capture(&mut app, &name);
        let after = app
            .plugin_layer()
            .ui
            .rect_of("test.window", "hud")
            .expect("the HUD is drawn");
        assert!(
            (after.x + after.w / 2.0 - 300.0).abs() < 2.0
                && (after.y + after.h - 6.0 - 320.0).abs() < 2.0,
            "{interface:?}: the HUD stands where it was dragged: {before:?} to {after:?}"
        );
        let letters = text_pixels_in(
            &frame,
            (after.x, after.y, after.w, after.h),
            if interface == Interface::Classic {
                CLASSIC_TEXT
            } else {
                TEXT
            },
        );
        assert!(
            letters > 10,
            "{interface:?}: its line is drawn there: {letters}"
        );
        assert!(
            hook(&mut app)
                .host
                .console("test.window")
                .unwrap_or_default()
                .is_empty(),
            "{interface:?}: its button took no click"
        );
        let kept = hook(&mut app)
            .host
            .layout("at/test.window/hud")
            .cloned()
            .expect("the place is kept");
        #[allow(clippy::cast_possible_truncation)]
        let at = (after.x.round() as i64, after.y.round() as i64);
        assert_eq!(
            (kept.get("x").as_i64(), kept.get("y").as_i64()),
            (Some(at.0), Some(at.1)),
            "{interface:?}: kept for the character where it stands"
        );
    }
}
