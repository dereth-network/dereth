//! Screen-layout persistence: the save-layout command writes the automatic layout file beside the
//! preferences file and a moved panel survives save, exit and reload; the three layout paths
//! resolve beside the preferences file; and a wire login loads the layout for the restored
//! gameplay resolution, not the forced 800x600 one. Fixture: headless `App`s with the retail dats
//! and a scratch preferences file; the login test drives framed messages through a socket-free
//! server peer and types `@saveautoui` through scripted window messages. There are no skips.
//!
//! # Saving and loading
//!
//! Exit does not save the layout. Ending a session sets its ending flag, checks logout
//! confirmation and logs off the character; destruction runs that sequence, unregisters, closes
//! three dialogs and restores the forced 800x600 presentation, without saving. The save-UI notice
//! comes only from the two save commands: `@saveui [name]`, which rejects names over 16
//! characters, and `@saveautoui`, which supplies the literal `#auto`.
//!
//! Player-description delivery loads `#auto` and stores whether loading succeeded.
//! `SessionEvent::PlayerDescription` schedules that work for `App::ui_use_time` after gameplay
//! exists. The first test calls `App::note_player_description` directly and seeds identity fields:
//! it checks persistence, not login production or resolution ordering. The last test drives framed
//! 0xF7E1/0xF658, the character row's LogOn action and 0x0013, then submits `@saveautoui` through
//! scripted mouse/character/Enter messages, whose frames drain the command to
//! `UiShell::save_ui_layout`.
//!
//! # Evidence and boundaries
//!
//! The sixteen window tags/IDs/order come from the shared `dereth_ui::persist::WINDOWS` table,
//! not an independent copy. Disk checks independently pin no newline/carriage return, a final
//! space, sixteen X fields, irregular X spacing and the exact moved CHAT row. Full rectangle
//! round-trip checks cover every table entry. Exact filenames pin height before width and
//! default/named/automatic paths beside the preferences file. Loading resizes before moving
//! because movement clamps against the parent; restoring the moved/resized rectangle exercises the
//! result, not a call-order trace.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use std::path::PathBuf;

use dereth_client::app::App;
use dereth_client::config::{Config, Preferences};
use dereth_client::net::ClientNetwork;
use dereth_client::pump::{Pump, Win32Message};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::login::{
    CharacterIdentity, LoginCharacterSet, LoginEnterGameServerReady, LoginPlayerDescription,
    LoginWorldInfo,
};
use dereth_protocol::objects::{ItemCreateObject, LoginCreatePlayer, ObjectCreatePayload};
use dereth_ui::framework::mode;
use dereth_ui::persist::ScreenLayout;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const PLAYER: ObjectId = ObjectId(0x5000_0246);
const UI_QUEUE: u16 = 9;
const SMARTBOX_QUEUE: u16 = 10;
const CHAT_ENTRY: ElementId = ElementId(0x1000_0016);

/// Scratch preferences location: layout paths resolve beside the configured preferences
/// file, so naming that file supplies the directory without a process working-directory fallback.
fn prefs_file() -> PathBuf {
    let dir = std::env::temp_dir().join("dereth-screen-layout-persistence");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join("UserPreferences.ini")
}

/// A socket-free server peer. Messages are sent
/// through the real transport and session; nothing is injected into `App` as a `SessionEvent`.
struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "screen-layout-login", "unused", 0)
                .expect("a socket-free client network");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("literal address")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("envelope");
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("the transport accepts its own envelope");
    }

    fn player_description(&mut self, app: &mut App) {
        self.stamp += 1;
        let bytes = dereth_protocol::events::pack_event(
            PLAYER,
            self.stamp,
            &LoginPlayerDescription::default(),
        )
        .expect("0x0013 frames");
        self.send(app, UI_QUEUE, bytes);
    }
}

fn frames(app: &mut App, count: u32, phase: &str) {
    for i in 0..count {
        assert!(app.frame(), "{phase}: frame {i}");
    }
}

/// A complete synthetic login identity and framed 0x0013 producer. The character-management
/// row's scripted double click supplies the selected character; no host-state field is seeded here.
fn login_at_saved_resolution() -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "retail dats at {}",
        d.display()
    );
    let mut cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        enter_world: true,
        start_char: "Kupo".to_owned(),
        dat_dir: d,
        preferences_file: prefs_file(),
        ..Config::default()
    };
    cfg.apply_preferences(&Preferences::parse("[Display]\r\nResolution=1024x768\r\n"));
    let mut app = App::new(cfg).expect("application");
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(mode::CHARACTER_MANAGEMENT);
    frames(&mut app, 3, "character screen");

    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("socket-free endpoint");
    peer.send(
        &mut app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginWorldInfo {
            connections: 1,
            max_connections: 100,
            world_name: "Frostfell".to_owned(),
        })
        .expect("0xF7E1"),
    );
    peer.send(
        &mut app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginCharacterSet {
            characters: vec![CharacterIdentity {
                gid: PLAYER,
                name: "Kupo".to_owned(),
                seconds_greyed_out: 0,
            }],
            num_allowed_characters: 5,
            account: "screen-layout-login".to_owned(),
            ..Default::default()
        })
        .expect("0xF658"),
    );
    frames(&mut app, 6, "character selection and its real LogOn action");
    assert_eq!(app.host_state().entered_character.as_deref(), Some("Kupo"));
    assert_eq!(app.host_state().world_name.as_deref(), Some("Frostfell"));

    peer.send(
        &mut app,
        UI_QUEUE,
        dereth_protocol::write_blob(&LoginEnterGameServerReady).expect("0xF7DF"),
    );
    frames(&mut app, 2, "enter-world phase two");
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        dereth_protocol::write_blob(&LoginCreatePlayer { player_id: PLAYER }).expect("0xF746"),
    );
    let mut create = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    create.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    create.physicsdesc.setup_id = Some(0x0200_0001);
    create.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        SMARTBOX_QUEUE,
        dereth_protocol::write_blob(&ItemCreateObject(create)).expect("player create"),
    );
    frames(&mut app, 3, "player arrival");
    peer.player_description(&mut app);
    frames(&mut app, 3, "wire 0x0013 and gameplay construction");
    assert_eq!(
        app.renderer().size(),
        (1024, 768),
        "gameplay lifted the login force"
    );
    assert!(!app.uses_forced_resolution());
    app
}

struct Hand {
    pump: Pump,
    time: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time: 246_000,
        }
    }

    fn send(&mut self, app: &mut App, message: Win32Message) {
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("input shell")
            .on_message(message);
    }

    fn submit(&mut self, app: &mut App, text: &str) {
        let bounds = {
            let shell = app.ui().expect("shell");
            let any: &dyn std::any::Any = shell.flow.current().expect("screen");
            let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
            let entry = shell
                .ui
                .get_child_recursive(screen.root().expect("root"), CHAT_ENTRY)
                .expect("chat entry");
            shell.ui.screen_box(entry)
        };
        self.time += 10;
        let motion = self.pump.mouse_move_message(
            f64::from((bounds.x0 + bounds.x1) / 2),
            f64::from((bounds.y0 + bounds.y1) / 2),
            self.time,
        );
        self.send(app, motion);
        for down in [true, false] {
            self.time += 10;
            let click = self
                .pump
                .mouse_button_message(winit::event::MouseButton::Left, down, self.time)
                .expect("left button message");
            self.send(app, click);
        }
        assert!(app.frame());
        for byte in text.bytes() {
            self.time += 10;
            self.send(
                app,
                Win32Message::new(
                    dereth_input::win32::msg::WM_CHAR,
                    usize::from(byte),
                    0,
                    self.time,
                ),
            );
        }
        assert!(app.frame());
        for (message, lparam) in [
            (dereth_input::win32::msg::WM_KEYDOWN, 0x001c_0001_u32),
            (dereth_input::win32::msg::WM_KEYUP, 0xc01c_0001_u32),
        ] {
            self.time += 10;
            self.send(
                app,
                Win32Message::new(message, 13, lparam as isize, self.time),
            );
        }
        assert!(app.frame());
    }
}

fn app_in_gameplay(frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: d,
        preferences_file: prefs_file(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    // Seed the two filename inputs for the direct persistence test. Production world-name
    // and entered-character producers are exercised separately by login_at_saved_resolution
    // using a socket-free peer; they do not require a live shard. App::host_state_mut exposes
    // this explicit test setup without making it evidence for the production ordering.
    let host = app.host_state_mut();
    host.entered_character = Some("Kupo".to_owned());
    host.world_name = Some("Frostfell".to_owned());
    app
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// Every one of the sixteen windows' live rectangle, in file order.
fn rects(app: &App) -> Vec<(&'static str, (i32, i32), (i32, i32))> {
    let shell = app.ui().expect("shell");
    dereth_ui::persist::WINDOWS
        .iter()
        .map(|w| {
            let h = find(app, w.element);
            let b = shell.ui.screen_box(h);
            (w.tag, shell.ui.screen_origin(h), (b.width(), b.height()))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------

/// A programmatically moved/resized panel survives an explicit save and automatic reload
/// across separate applications. No panel state is shared in memory; the disk bytes are
/// inspected before building the reader. This test uses the direct player-description
/// notification seam; the later test covers framed login and resolution ordering.
#[test]
fn a_moved_panel_survives_a_save_an_exit_and_a_reload_through_the_automatic_file() {
    let _gpu = gpu_lock();

    // The path both halves agree on, and the only thing they share.
    let expected_path = {
        let app = app_in_gameplay(4);
        let shell = app.ui().expect("shell");
        let p = shell
            .screen_layout_path(ScreenLayout::AUTO_NAME, &prefs_file(), "Kupo", "Frostfell")
            .expect("a preferences file was configured, so there is a directory");
        drop(app);
        p
    };
    assert_eq!(
        expected_path.file_name().and_then(std::ffi::OsStr::to_str),
        Some("UI-Kupo-Frostfell-600-800.txt"),
        "automatic layout path: UI-<char>-<world>-<HEIGHT>-<WIDTH>.txt, height first"
    );
    let _ = std::fs::remove_file(&expected_path);

    // ---- half one: move a window, save, and drop the application ---------------------------
    let moved: Vec<(&'static str, (i32, i32), (i32, i32))>;
    {
        let mut app = app_in_gameplay(4);
        // A missing file is a normal unsuccessful load, not an I/O failure. Check that the
        // pending request drains while the successful-load count remains zero before saving.
        app.note_player_description();
        app.frame();
        assert!(
            !app.auto_layout_pending(),
            "the load ran, on the first frame a screen was up"
        );
        assert_eq!(
            app.ui().expect("shell").stats.screen_layout_loads,
            0,
            "and reported no file rather than an error"
        );

        // `<CHAT>` is the main chat window: move and resize it somewhere it certainly was not.
        let chat = find(&app, dereth_ui::persist::WINDOWS[1].element);
        {
            let ui = &mut app.ui_mut().expect("shell").ui;
            ui.resize_to(chat, 321, 173);
            ui.move_to(chat, 37, 51);
        }
        app.frame();
        moved = rects(&app);
        let (origin, size) = (moved[1].1, moved[1].2);
        assert_eq!(
            (origin, size),
            ((37, 51), (321, 173)),
            "the window actually moved"
        );

        let wrote = app
            .ui_mut()
            .expect("shell")
            .save_ui_layout(&expected_path)
            .expect("the file is writable");
        assert!(
            wrote,
            "the layout writer reported success for the sixteen-window layout"
        );
    }

    // ---- the bytes, before anything reads them ---------------------------------------------
    let bytes = std::fs::read(&expected_path).expect("the file is on disk");
    assert!(
        !bytes.contains(&b'\n'),
        "retail's UI-*.txt is ONE line: no newline"
    );
    assert!(!bytes.contains(&b'\r'), "and no carriage return");
    assert_eq!(
        bytes.last(),
        Some(&b' '),
        "the format's own trailing space ends the file"
    );
    let text = String::from_utf8(bytes).expect("ascii");
    assert_eq!(text.matches(" X:").count(), 16, "sixteen rows");
    assert!(
        !text.contains("X: "),
        "the irregular spacing: no space after X:"
    );
    assert!(
        text.contains("<CHAT> X:37 Y: 51 W: 321 H: 173 "),
        "the moved window's own row, verbatim: {text}"
    );

    // ---- half two: a brand new application loads it automatically ---------------------------
    let mut app = app_in_gameplay(4);
    let before = rects(&app);
    assert_ne!(
        before[1].1,
        (37, 51),
        "a fresh application starts at the layout's own place"
    );

    app.note_player_description();
    assert!(
        app.auto_layout_pending(),
        "0x0013 arrived and the load is outstanding"
    );
    app.frame();
    assert!(!app.auto_layout_pending(), "and it ran");

    let stats = app.ui().expect("shell").stats;
    assert_eq!(
        stats.screen_layout_loads, 1,
        "the automatic layout file loaded once"
    );
    assert_eq!(
        stats.screen_layout_windows_placed, 16,
        "all sixteen windows were placed"
    );
    assert_eq!(
        rects(&app),
        moved,
        "every window is where the first application left it"
    );

    std::fs::remove_file(&expected_path).expect("cleanup");
}

/// All three path choices resolve beside the configured preferences file, matching the
/// directory rule also used by the input-keymap path. Automatic includes character/world and
/// height/width; empty selects the default filename, and an ordinary name selects name.txt.
#[test]
fn the_shell_resolves_all_three_layout_paths_beside_the_preferences_file() {
    let _gpu = gpu_lock();
    let app = app_in_gameplay(4);
    let shell = app.ui().expect("shell");
    let prefs = prefs_file();
    let dir = prefs.parent().expect("a directory");

    let p = |name: &str| {
        shell
            .screen_layout_path(name, &prefs, "Kupo", "Frostfell")
            .expect("a preferences file was configured")
    };
    assert_eq!(p("#auto"), dir.join("UI-Kupo-Frostfell-600-800.txt"));
    assert_eq!(
        p(""),
        dir.join("UI-Default.txt"),
        "the default layout filename"
    );
    assert_eq!(p("mine"), dir.join("mine.txt"));

    // With no preferences path there is no directory to resolve against. Require None
    // instead of accidentally reading/writing a path relative to the working directory.
    assert_eq!(
        shell.screen_layout_path("#auto", std::path::Path::new(""), "Kupo", "Frostfell"),
        None
    );
}

/// Behaviour: ui.layout.a-saved-layout-is-reloaded-at-the-next-login
///
/// Automatic reading waits until gameplay releases the login screen's forced 800x600 size, so
/// it searches the 768-1024 filename at a configured 1024x768, not the 600-800 one. This test
/// writes via typed @saveautoui, asserts no forced-size file was authored, and checks the framed
/// relogin restores the saved origin.
#[test]
fn wire_login_automatically_loads_the_layout_for_the_restored_gameplay_resolution() {
    let _gpu = gpu_lock();
    std::fs::create_dir_all(prefs_file().parent().expect("scratch directory")).expect("scratch");
    std::fs::write(&prefs_file(), "[Display]\r\nResolution=1024x768\r\n")
        .expect("disposable preferences");

    let expected_path = prefs_file()
        .parent()
        .unwrap()
        .join("UI-Kupo-Frostfell-768-1024.txt");
    let wrong_forced_path = prefs_file()
        .parent()
        .unwrap()
        .join("UI-Kupo-Frostfell-600-800.txt");
    let _ = std::fs::remove_file(&expected_path);
    let _ = std::fs::remove_file(&wrong_forced_path);

    let saved_origin;
    {
        let mut app = login_at_saved_resolution();
        let chat = find(&app, dereth_ui::persist::WINDOWS[1].element);
        {
            let ui = &mut app.ui_mut().expect("shell").ui;
            ui.move_to(chat, 71, 83);
        }
        app.frame();
        saved_origin = app.ui().expect("shell").ui.screen_origin(chat);
        assert_eq!(saved_origin, (71, 83), "the programmatic move reached CHAT");
        Hand::new().submit(&mut app, "@saveautoui");
        assert!(
            expected_path.exists(),
            "typed @saveautoui wrote the 1024x768 identity path"
        );
        assert!(
            !wrong_forced_path.exists(),
            "nothing authored an 800x600 automatic file"
        );
        app.shutdown();
    }

    let mut relogged = login_at_saved_resolution();
    assert_eq!(
        relogged.ui().expect("shell").stats.screen_layout_loads,
        1,
        "the inbound 0x0013 loaded #auto after gameplay restored the configured resolution"
    );
    let chat = find(&relogged, dereth_ui::persist::WINDOWS[1].element);
    assert_eq!(
        relogged.ui().expect("shell").ui.screen_origin(chat),
        saved_origin,
        "the saved visible CHAT position returned"
    );
    frames(&mut relogged, 2, "post-login frames");
    assert_eq!(
        relogged.ui().expect("shell").ui.screen_origin(chat),
        saved_origin,
        "later frames did not overwrite a layout loaded from file"
    );
    relogged.shutdown();

    std::fs::remove_file(&expected_path).expect("layout cleanup");
    std::fs::remove_file(&prefs_file()).expect("preferences cleanup");
}
