//! The burden meter repaints only itself: two renders of one application differ only in the
//! character's `PropertyInt 5 EncumbranceVal` (the one number the load lookup reads and nothing
//! else on this screen consumes), and every changed pixel lies inside the fill (`0x100001D9`) or
//! the percentage text (`0x100001D8`), each of which must itself contain changed pixels. The
//! load-level update writes both, and at the meter's 300% scale a one-pixel move needs about five
//! percentage points, which the `"%d%%"` text always shows.
//! Fixture: the `first-login-walk-jump` recording replayed into a headless gameplay `App` with the
//! retail dats and the backpack opened.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::LocalTime;
use dereth_ui::framework::Screen;
use dereth_ui::{Box2D, ElementId, UiSystem};
use dereth_ui_screens::panels::inventory::{BURDEN_METER, BURDEN_TEXT};
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

const SESSION: &str = "first-login-walk-jump";

fn replay(session: &str) -> Vec<SessionEvent> {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push(e);
        }
    }
    events
}

/// Everything up to the capture's logout, which destroys every object.
fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture never reached 0x0013");
    let end = events[seen..]
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(events.len(), |i| seen + i);
    &events[..end]
}

fn app_in_gameplay(frames: u32) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the D3D12 device and the shipped UI");
    app.start_shell().expect("the shell starts");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

fn box_of(app: &mut App, id: ElementId) -> Box2D {
    let (ui, screen) = gameplay_screen(app);
    let root = *screen.roots().first().expect("root");
    let h = ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"));
    ui.screen_box(h)
}

/// Behaviour: inventory.burden.the-bar-and-the-number-are-what-the-character-is-carrying
///
/// Two renders differing only in `EncumbranceVal`, with every changed pixel inside the burden
/// widgets' own rectangles and zero outside. Without `InventoryPanels::set_load_level`'s two
/// element writes the differential is zero pixels; with the meter write aimed at another element
/// the outside count is non-zero.
#[test]
fn changing_only_the_burden_repaints_only_the_burden_widgets() {
    let events = replay(SESSION);
    let in_world = events_in_world(&events).to_vec();

    let shot = |encumbrance: Option<i32>| -> (u32, u32, Vec<u8>, Box2D, Box2D, i32) {
        let mut app = app_in_gameplay(4);
        let _ = app.apply_hud_events(&in_world);
        // The load lookup reads integer property 5 from the player's description qualities, and
        // that quality store exists only for the object whose id matches the world view's player
        // id. This test replays the HUD's events alone, never the `0xF746`/`0xF745` that build that
        // weenie, so it installs the player here; installing replays the parked `0x0013` onto the
        // new row, so this runs after the events rather than before them.
        {
            let player = app
                .hud()
                .player
                .unwrap_or(dereth_primitives::ObjectId(0x5000_0001));
            let w = &mut app.probe_mut().objects_mut().world;
            w.player = None;
            w.tables
                .weenies
                .insert(player, dereth_client_model::weenie::Weenie::new(player));
            assert!(w.set_player(player), "the identity is adopted once");
        }
        // The backpack has to be **open** for any of this to be on screen; the burden meter lives
        // inside the backpack panel, which is a sub-panel of the inventory page. Opened the way the
        // toolbar's backpack button opens it, not by writing visibility onto the element.
        {
            let (ui, screen) = gameplay_screen(&mut app);
            let page = screen
                .panels
                .pages
                .iter()
                .find(|p| p.element == INVENTORY_PAGE)
                .copied()
                .expect("the inventory page is in the shipped panel stack");
            screen.recv_set_panel_visibility(ui, page.panel_id, true);
        }
        for _ in 0..6 {
            app.frame();
        }
        if let Some(v) = encumbrance {
            use dereth_client_model::qualities::{StatKey, StatType, StatValue};
            let q = app
                .probe_mut()
                .objects_mut()
                .world
                .player_qualities_mut()
                .expect("the capture's 0x0013 qualities");
            // Encumbrance value: the load lookup's only integer-property lookup for key 5.
            // Nothing else on this screen reads it.
            q.set(StatKey::new(StatType::Int, 5), StatValue::Int(v));
        }
        app.frame();
        let percent = {
            let (_, screen) = gameplay_screen(&mut app);
            screen
                .inventory
                .burden
                .expect("the load-level update ran")
                .1
        };
        let meter = box_of(&mut app, BURDEN_METER);
        let text = box_of(&mut app, BURDEN_TEXT);
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("an offscreen capture");
        (w, h, bgra, meter, text, percent)
    };

    // The capture's own burden, then a deliberately different one. The second value is the *only*
    // difference between the two runs.
    let (w, h, a, meter, text, pa) = shot(None);
    let (w2, h2, b, meter2, text2, pb) = shot(Some(300));
    assert_eq!((w, h), (w2, h2));
    assert_eq!(
        (meter, text),
        (meter2, text2),
        "the two widgets moved between runs"
    );
    assert_ne!(
        pa, pb,
        "the two runs produced the same burden and prove nothing: {pa}% both times"
    );
    assert!(
        meter.width() > 0 && meter.height() > 0,
        "the burden meter has no box: {meter:?}"
    );

    let inside = |r: Box2D, x: i32, y: i32| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1;
    let (mut in_meter, mut in_text, mut outside) = (0usize, 0usize, 0usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if a[i..i + 4] == b[i..i + 4] {
                continue;
            }
            if inside(meter, x, y) {
                in_meter += 1;
            } else if inside(text, x, y) {
                in_text += 1;
            } else {
                outside += 1;
            }
        }
    }
    println!(
        "burden {pa}% -> {pb}%; changed pixels: meter {in_meter} {meter:?}, \
         text {in_text} {text:?}, outside {outside}"
    );
    assert!(
        in_meter > 0,
        "changing the burden repainted no pixel of the meter itself"
    );
    assert!(
        in_text > 0,
        "changing the burden repainted no pixel of the text"
    );
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the two burden widgets"
    );
}
