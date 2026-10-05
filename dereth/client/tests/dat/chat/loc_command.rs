//! `/loc` prints the body's location line. With no arguments it prints
//! `Your location is: 0x%08X [%f %f %f] %f %f %f %f` -- the cell, the origin xyz, then the
//! rotation quaternion wxyz, each to six decimals -- on chat type 0 in the main chat log. With
//! arguments it is refused with "Unexpected arguments to @loc" on the spew channel (chat type
//! 0x1A), and it is never the catch-all "not a valid command". The expected line is formatted
//! here, independently of the client's formatter, and checked against a line read off a retail
//! screenshot in Holtburg.
//! Fixture: a headless App with a static scene and its body teleported to the screenshot's pose;
//! keys go through `Pump` and the input manager, and the chat log's glyphs are read back.
use crate::common::client_dir_or_workspace_client as client_dir;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::window::{ENTRY, LOG};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use winit::keyboard::KeyCode;
use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

/// The pose read off a retail screenshot: Holtburg, cell `0xA9B4002A`.
fn holtburg() -> Position {
    Position::new(
        CellId(0xA9B4_002A),
        Frame::new(
            Vec3 {
                x: 133.168_503,
                y: 27.490_582,
                z: 94.005_005,
            },
            Quat {
                w: -0.991_022,
                x: 0.0,
                y: 0.0,
                z: -0.133_696,
            },
        ),
    )
}

/// Load a local static scene, attach/teleport its body and queue gameplay mode. This supplies
/// the body directly rather than replaying world entry. DATs and application creation must
/// succeed; frames may be zero for the immediate transcript-format check.
fn app_with_body_at(at: Position, frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = crate::common::sim_app::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let block = at.cell.landblock();
    app.load_static_scene(SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("the static scene and the body load");
    {
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.land().load_block_cells(block);
        c.teleport(at);
    }
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn body_position(app: &App) -> Position {
    app.world_state()
        .and_then(|w| w.character.as_ref())
        .expect("a body")
        .position()
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn text_of(app: &mut App, h: ElemHandle) -> String {
    app.ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn focus_of(app: &App) -> Option<ElemHandle> {
    app.ui().expect("shell").ui.focus_element()
}

/// Read stored glyph text and color runs. This does not inspect pixel visibility, clipping or
/// rasterized glyphs; log_text discards the colors before the assertions.
fn runs(ui: &mut UiSystem, log: ElemHandle) -> Vec<(String, u32)> {
    let t = ui
        .text_element_mut(log)
        .expect("the chat log is a text element");
    let mut out: Vec<(String, u32)> = Vec::new();
    for g in &t.glyphs.glyphs {
        let ch = char::from_u32(u32::from(g.data)).unwrap_or('\u{FFFD}');
        match out.last_mut() {
            Some((s, c)) if *c == g.color => s.push(ch),
            _ => out.push((ch.to_string(), g.color)),
        }
    }
    out
}

fn log_text(app: &mut App) -> String {
    let log = find(app, LOG);
    let (ui, _) = gameplay(app);
    runs(ui, log).into_iter().map(|(s, _)| s).collect()
}

/// Read pending and retained spew-strip model strings, where chat type 0x1A is routed.
fn spew(app: &App) -> Vec<String> {
    let mut out = app.hud().panels.spew.model.pending.clone();
    out.extend(app.hud().panels.spew.model.items.iter().cloned());
    out
}

/// Read submitted-command history from main chat window 8.
fn history(app: &mut App) -> Vec<String> {
    app.objects()
        .world
        .chat
        .entries
        .get(&8)
        .map_or_else(Vec::new, |entry| entry.history().to_vec())
}

/// An independent spelling of the location format: eight uppercase hexadecimal cell
/// digits, origin xyz and quaternion wxyz, each float widened from f32 and printed to six
/// decimals. The helper supplies the prefix; the output comparison trims leading and trailing
/// newlines.
fn retail_loc_line(p: &Position) -> String {
    let o = p.frame.origin;
    let q = p.frame.rotation;
    format!(
        "Your location is: 0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
        p.cell.0,
        f64::from(o.x),
        f64::from(o.y),
        f64::from(o.z),
        f64::from(q.w),
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z)
    )
}

/// Synthetic keyboard messages, the same shape as `chat::heard_speech_formatting`'s: key messages
/// derived with `Pump`, dispatched locally and handed to the input manager. No OS event queue, desktop input
/// injection or window procedure is exercised.
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 400_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn key(&mut self, app: &mut App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit maps this key to a virtual key and a scan code");
        self.send(app, m);
    }

    fn tap(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
        self.key(app, code, false);
        app.frame();
    }

    /// Construct WM_CHAR directly, in the form normally produced by key translation.
    fn character(&mut self, app: &mut App, ch: char) {
        self.time_ms += 10;
        let m = Win32Message::new(
            dereth_input::win32::msg::WM_CHAR,
            ch as usize,
            0,
            self.time_ms,
        );
        self.send(app, m);
    }
}

/// Focus the entry with Enter, type `line`, send it with Enter.
fn submit(app: &mut App, hand: &mut Hand, line: &str) {
    let entry = find(app, ENTRY);
    let _ = app.ui_mut().expect("the UI shell is up").ui.requests.take();
    hand.tap(app, KeyCode::Enter);
    assert_eq!(focus_of(app), Some(entry), "Enter focused the chat entry");
    // Supply the Enter character following keydown; the entry suppresses that opening character.
    hand.character(app, char::from(0x0D_u8));
    app.frame();
    for ch in line.chars() {
        hand.character(app, ch);
        app.frame();
    }
    assert_eq!(text_of(app, entry), line, "the box holds the whole line");
    hand.tap(app, KeyCode::Enter);
    assert_eq!(
        text_of(app, entry),
        "",
        "submitting the command emptied the entry"
    );
}

/// Behaviour: chat.loc.prints-the-bodys-location-line
///
/// Submit `/loc` through the normalized keyboard/entry route, then compare newly appended log
/// glyph text with the independent formatter. Check the same body position before/after,
/// one history entry, one printed line and zero refusal/catch-all counts. Leading/trailing
/// newlines are trimmed; exact newline count and chat color are not asserted.
#[test]
fn typing_slash_loc_and_enter_prints_your_location_is_with_the_bodys_position() {
    let mut app = app_with_body_at(holtburg(), 12);
    let mut hand = Hand::new();
    let before = log_text(&mut app);
    let at = body_position(&app);
    assert_ne!(at.cell.0, 0, "the body stands in a cell");
    assert_eq!(
        at.cell.0 >> 16,
        0xA9B4,
        "and it is the screenshot's Holtburg block"
    );

    submit(&mut app, &mut hand, "/loc");
    assert_eq!(
        history(&mut app),
        vec!["/loc".to_owned()],
        "the line was taken out of the box"
    );
    app.frame();

    let after = log_text(&mut app);
    let printed = after
        .strip_prefix(&before)
        .unwrap_or_else(|| panic!("the log grew at the end: before {before:?}, after {after:?}"));
    let now = body_position(&app);
    assert_eq!(
        now, at,
        "the body did not move between the read and the print"
    );
    let expected = retail_loc_line(&at);
    assert_eq!(
        printed.trim_end_matches('\n').trim_start_matches('\n'),
        expected,
        "the chat line must match the independently formatted body position"
    );
    assert_eq!(app.interaction().stats.loc_lines_printed, 1);
    assert_eq!(
        app.interaction().stats.chat_commands_unimplemented,
        0,
        "not the catch-all"
    );
    assert_eq!(
        app.interaction().stats.chat_commands_refused,
        0,
        "not a refusal either"
    );
    app.shutdown();
}

/// Teleport to the recorded screenshot pose with zero subsequent frames. Compare the local
/// expected-line formatter and the production position formatter with exact transcript literals,
/// pinning cell, seven float fields, quaternion w-first order and six decimals. This test does
/// not submit the command or read the screenshot image.
#[test]
fn the_screenshot_position_renders_as_the_screenshot_line() {
    let app = app_with_body_at(holtburg(), 0);
    let at = body_position(&app);
    assert_eq!(
        retail_loc_line(&at),
        "Your location is: 0xA9B4002A [133.168503 27.490582 94.005005] -0.991022 0.000000 0.000000 -0.133696",
        "the position transcript read from a retail Holtburg screenshot"
    );
    assert_eq!(
        dereth_client_runtime::interaction::position_to_string(&at),
        "0xA9B4002A [133.168503 27.490582 94.005005] -0.991022 0.000000 0.000000 -0.133696"
    );
}

/// Extra arguments produce the exact handled-refusal text in the spew model, without an
/// invalid-command response or a location line. Refusal/printed/catch-all counters distinguish
/// that path.
#[test]
fn loc_with_arguments_is_refused_on_the_spew_channel_and_is_not_an_invalid_command() {
    let mut app = app_with_body_at(holtburg(), 12);
    let mut hand = Hand::new();
    submit(&mut app, &mut hand, "/loc here");
    app.frame();
    // Read the strip's model, which receives type 0x1A; no visual strip placement is asserted.
    let printed = spew(&app);
    assert!(
        printed.iter().any(|l| l == "Unexpected arguments to @loc"),
        "the expected argument-refusal text must be in the spew strip: {printed:?}"
    );
    assert!(
        !printed.iter().any(|l| l.contains("not a valid command")),
        "the handled argument refusal must not add an invalid-command response"
    );
    assert!(
        !log_text(&mut app).contains("Your location is"),
        "and no location line was printed"
    );
    assert_eq!(app.interaction().stats.chat_commands_refused, 1);
    assert_eq!(app.interaction().stats.loc_lines_printed, 0);
    assert_eq!(app.interaction().stats.chat_commands_unimplemented, 0);
    app.shutdown();
}
