//! The in-game HUD's state: vitals, radar, toolbar, indicator strip and the chat window come up
//! as the six retail windows and no others, every registered panel page starts down, a toolbar
//! button opens and closes its page, and the HUD updates from server state (vitals, chat lines,
//! window placements, radar coordinates) replayed from the recordings. The link lamp starts in its
//! good-link state and exactly one stance icon shows. What the HUD draws is
//! `rendering::hud_raster`.
//! Fixture: the retail dats and the recorded sessions in `fixtures/packet-captures` (replayed
//! through the real transport and object stream), on a headless App at 800x600; missing dats or
//! recordings fail.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::recorded_world_sessions;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::LocalTime;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::{window, GamePlayScreen};

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// **An `expect`, never a skip**: a test that returns early passes, and an early return is
/// invisible in the summary line.
pub(crate) fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

pub(crate) fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// Give an application's world the player row that owns its shared qualities.
///
/// The player object owns the player description and the qualities shared by vitals,
/// purse and character panel. These tests replay HUD events without the `0xF746`/`0xF745`
/// sequence that creates that object. Without this row, the parked `0x0013` has no owner from
/// which the vitals can read their current and maximum values.
///
/// The id must be the one the bar is asking about: `HudView::vital` takes the player's numbers
/// from the world's player qualities and other objects' values from their rows. `HudView::player`
/// uses `world.player.or(hud.player)`. The world's `set_player` method replays the parked
/// description onto the row once its owner is known, so this runs *after* the events, not
/// before.
fn embody(app: &mut App) {
    let player = app
        .hud()
        .player
        .unwrap_or(dereth_primitives::ObjectId(0x5000_0001));
    let w = &mut app.objects_mut().world;
    w.player = None;
    w.tables
        .weenies
        .insert(player, dereth_client_model::weenie::Weenie::new(player));
    assert!(w.set_player(player), "the identity is adopted once");
}

pub(crate) fn app_in_gameplay(ui: bool, frames: u32) -> App {
    let cfg = Config {
        ui,
        ..base_config()
    };
    // **Every fixture path here is an `expect`**, so a missing fixture fails rather than passing
    // without running.
    let mut app = App::new(cfg).unwrap_or_else(|e| panic!("the application must start: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell must come up: {e}"));
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s)
        .unwrap_or_else(|e| panic!("the static scene must load: {e}"));
    if ui {
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    }
    for _ in 0..frames {
        app.frame();
    }
    app
}

/// The live `GamePlayScreen` after the application's mode machine has switched to gameplay.
pub(crate) fn gameplay_screen(app: &mut App) -> Option<(&mut UiSystem, &mut GamePlayScreen)> {
    let shell = app.ui_mut()?;
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut()?;
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any.downcast_mut::<GamePlayScreen>()?;
    Some((ui, screen))
}

pub(crate) fn root_of(ui: &UiSystem) -> Option<ElemHandle> {
    ui.get_element(ElementId(0x1000_0495))
}

fn visible(ui: &UiSystem, root: ElemHandle, id: ElementId) -> Option<bool> {
    let h = ui.get_child_recursive(root, id)?;
    Some(ui.node(h)?.region.flags.visible)
}

// ---------------------------------------------------------------------------------------------
// The capture corpus, replayed exactly as `objects/populated_world.rs` replays it.
// ---------------------------------------------------------------------------------------------

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

/// The captures whose character reaches the world, **measured rather than named**: those whose
/// server stream carries a `0x0013 Login_PlayerDescription`. The rest are login-only -- the account
/// authenticates and disconnects without a character entering the world.
fn world_sessions() -> &'static [String] {
    static CACHE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut world = Vec::new();
        for s in corpus_sessions() {
            let (events, _) = replay(&s);
            if player_description(&events).is_some() {
                world.push(s);
            }
        }
        assert!(!world.is_empty(), "no recording enters the world");
        // The count is how many recordings carry
        // a `0x0013`, read by `dereth_client_net::client_session::testing` out of the blob streams -- a different
        // reader from the replay above, which observes the same fact through the client.
        assert_eq!(
            world.len(),
            recorded_world_sessions(),
            "the captures that enter the world, as many as carry a recorded 0x0013"
        );
        world
    })
}

/// Replay one recorded session through the real transport, session and object stream, and hand back
/// everything the session decoded.
pub(crate) fn replay(session: &str) -> (Vec<SessionEvent>, ObjectStream) {
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
    (events, objects)
}

/// The events up to the recorded session's **logout**.
///
/// Session shutdown clears the player module and description; applying the logged-off
/// event likewise clears HUD state. Keep the prefix before the first logged-off, character-select
/// or disconnected event after a player description, so a display test sees the in-world state.
/// If none appears, retain the whole event list. `objects/populated_world.rs` uses its `last_populated` boundary
/// for the same purpose.
pub(crate) fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen_desc = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture never reached 0x0013");
    let end = events[seen_desc..]
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
        .map_or(events.len(), |i| seen_desc + i);
    &events[..end]
}

/// The `0x0013 Login_PlayerDescription` a recorded session carries.
fn player_description(
    events: &[SessionEvent],
) -> Option<&dereth_protocol::login::LoginPlayerDescription> {
    events.iter().find_map(|e| match e {
        SessionEvent::PlayerDescription(d) => Some(&**d),
        _ => None,
    })
}

// ---------------------------------------------------------------------------------------------
// 1. What is on screen when the game phase begins.
// ---------------------------------------------------------------------------------------------

/// Behaviour: hud.windows.the-hud-comes-up-with-the-six-retail-windows
///
/// **The gameplay screen does not show every panel at once.**
///
/// Retail client observation at 800×600 in Holtburg: the screen showed the
/// viewport, the indicator strip, the stacked vitals, the radar with its coordinate line, the main
/// chat window and the toolbar — **six** of the gameplay root's eighteen children — and nothing
/// else. Everything else on that root is down.
///
/// The mechanism behind each row is in `GamePlayScreen::HUD_START_VISIBILITY`, which says per row
/// whether it is a recovered rule or this observation; the assertion here is on the *result*,
/// because the result is what a player sees.
#[test]
fn the_hud_comes_up_with_the_six_windows_the_retail_client_shows_and_no_others() {
    have_dats();
    let mut app = app_in_gameplay(true, 4);
    let Some((ui, _screen)) = gameplay_screen(&mut app) else {
        panic!("the mode machine did not reach the gameplay screen");
    };
    let root = root_of(ui).expect("the gameplay root");

    // The six the retail frame shows.
    for (id, what) in [
        (window::SMART_BOX, "<SBOX> the 3D viewport"),
        (window::MAIN_CHAT, "<CHAT> the main chat window"),
        (window::STACKED_VITALS, "<VITS> the stacked vitals"),
        (window::TOOLBAR, "<TBAR> the toolbar"),
        (ElementId(0x1000_0611), "<INDI> the indicator strip"),
        (window::RADAR, "<RADA> the radar"),
    ] {
        assert_eq!(visible(ui, root, id), Some(true), "{what} must be up");
    }

    // The twelve it does not.
    for (id, what) in [
        (ElementId(0x1000_0505), "<FCH1>"),
        (ElementId(0x1000_050E), "<FCH2>"),
        (ElementId(0x1000_050F), "<FCH3>"),
        (ElementId(0x1000_0510), "<FCH4>"),
        (window::EXAMINATION, "<EXAM>"),
        (window::SIDE_VITALS, "<SVIT>"),
        (window::ENV_PANEL, "<ENVP>"),
        (window::PANEL_STACK, "<PANS>"),
        (window::POWER_BAR, "<PBAR>"),
        (window::COMBAT_PANEL, "<COMB>"),
        (window::KEYBOARD, "classic_keyboard"),
        (window::ADMIN, "classic_admin"),
    ] {
        assert_eq!(visible(ui, root, id), Some(false), "{what} must be down");
    }

    // The in-viewport controls that must remain hidden at startup.
    assert_eq!(
        visible(ui, root, window::SMART_BOX_POWER_BAR),
        Some(false),
        "the in-viewport bar"
    );
    assert_eq!(
        visible(ui, root, window::BARBER),
        Some(false),
        "barber panel"
    );
    assert_eq!(
        visible(ui, root, window::TARGET_ON_SCREEN),
        Some(false),
        "no target selected"
    );
}

/// Initialization registers sixteen panel pages, five environment pages and two combat
/// pages, then hides every page and leaves no current or previous page in any stack.
///
/// Twenty-three pages, and the shipped `classic_gameplay` layout marks **all** of them visible: the
/// admin panel, the inventory, the options pages and twenty more, stacked on top of each other. The
/// assertion is on the live tree, so it fails if any of the three stacks leaves a registered page up.
#[test]
fn all_twenty_three_registered_pages_are_down_and_the_stacks_know_it() {
    have_dats();
    let mut app = app_in_gameplay(true, 4);
    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");

    assert_eq!(
        screen.panels.pages.len(),
        16,
        "the panel controller registered its sixteen pages"
    );
    assert_eq!(
        screen.env_panel.pages.len(),
        5,
        "the environment panel registered its five"
    );
    assert_eq!(
        screen.combat_panel.pages.len(),
        2,
        "the combat panel registered its two"
    );
    for s in [&screen.panels, &screen.env_panel, &screen.combat_panel] {
        assert_eq!(
            s.current, None,
            "no page is current before the first notice"
        );
        assert_eq!(s.previous, None);
    }

    let ids = dereth_ui_screens::panels::catalogue::PANEL_PAGES
        .iter()
        .chain(dereth_ui_screens::panels::catalogue::ENV_PANEL_PAGES.iter())
        .chain(dereth_ui_screens::panels::catalogue::COMBAT_PANEL_PAGES.iter());
    let mut checked = 0;
    for id in ids {
        let Some(h) = ui.get_child_recursive(root, ElementId(*id)) else {
            panic!("page {id:#010X} is not in classic_gameplay");
        };
        assert!(
            !ui.node(h).unwrap().region.flags.visible,
            "page {id:#010X} is still up"
        );
        checked += 1;
    }
    assert_eq!(checked, 23);

    // Each toolbar button selects a page by panel id. Attribute `0x10000029` must therefore be
    // nonzero on every one of the sixteen pages.
    for p in &screen.panels.pages {
        assert_ne!(p.panel_id, 0, "page {:#010X} has no panel id", p.element.0);
    }
    // …and the seven toolbar buttons name seven of them.
    assert_eq!(screen.toolbar.buttons.len(), 7);
    for b in &screen.toolbar.buttons {
        assert!(
            screen.panels.pages.iter().any(|p| p.panel_id == b.panel_id),
            "toolbar button {:#010X} opens panel id {} and no page carries it",
            b.element.0,
            b.panel_id
        );
    }
}

/// A toolbar button broadcasts a panel-visibility notice with `(panelId, !visible)`.
/// The stack shows that page, hides its predecessor and makes itself visible.
///
/// The button/page pairing is attribute `0x10000029`, read off the live
/// tree, so this test names no panel — it takes the first button the layout gives and asks what the
/// stack did.
#[test]
fn a_toolbar_button_opens_its_page_and_closing_it_puts_the_stack_away() {
    have_dats();
    let mut app = app_in_gameplay(true, 4);
    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");

    let button = *screen
        .toolbar
        .buttons
        .first()
        .expect("the toolbar has panel buttons");
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.panel_id == button.panel_id)
        .copied()
        .expect("a page carries the button's panel id");

    assert_eq!(
        visible(ui, root, window::PANEL_STACK),
        Some(false),
        "the stack starts away"
    );
    assert!(!ui.node(page.handle).unwrap().region.flags.visible);

    // The button-release message is id 1 with `p1 = 7`. Broadcast through the element
    // manager rather than call the screen directly, exercising its root-listener registration.
    let (element, panel_id, handle) = (button.element, button.panel_id, button.handle);
    let page_element = page.element;
    let page_handle = page.handle;
    let click = |app: &mut App| {
        let shell = app.ui_mut().expect("the shell");
        shell.ui.broadcast_element_message(
            handle,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
            7,
            0,
        );
        // `UiShell::frame` drains the element outbox to the current screen's message handler.
        app.frame();
    };

    ui.requests.clear();
    click(&mut app);
    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");
    assert_eq!(
        screen.panels.current,
        Some(page_element),
        "the page is now current"
    );
    assert!(
        ui.node(page_handle).unwrap().region.flags.visible,
        "the page is up"
    );
    assert_eq!(
        visible(ui, root, window::PANEL_STACK),
        Some(true),
        "and so is the stack"
    );
    // The panel-visibility notice the click broadcasts is not read back
    // here: `UiShell::frame` drains `dereth_ui_screens::requests` inside the same `app.frame()` that
    // delivered the click, which is the whole point of that queue. That the notice carries the raw
    // panel id is asserted in `dereth_ui_screens`' own unit test; what this test owns is the effect.
    let _ = (element, panel_id);

    // Clicking it again closes the page, and with nothing left current the stack goes away.
    click(&mut app);
    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");
    assert_eq!(screen.panels.current, None);
    assert!(!ui.node(page_handle).unwrap().region.flags.visible);
    assert_eq!(visible(ui, root, window::PANEL_STACK), Some(false));
}

// ---------------------------------------------------------------------------------------------
// 2. It draws over the world, and only where it said it would.
// ---------------------------------------------------------------------------------------------

/// One rendered frame taken for a named vitals value: `(value, BGRA, width, height)`.

// ---------------------------------------------------------------------------------------------
// 3. It updates from server state, and the state comes from the capture corpus.
// ---------------------------------------------------------------------------------------------

/// Behaviour: hud.vitals.the-vitals-show-what-the-server-sent
///
/// The HUD updates from server state, with the state taken from `fixtures/packet-captures`
/// rather than written here.
///
/// Oracle: `first-login-walk-jump`'s own `0x0013 Login_PlayerDescription`. The health, stamina and mana the
/// retail client was handed are queried independently through `dereth_client_model::attributes::inq_attribute_2nd`.
/// The assertion is that the three meters inside `<VITS>` carry
/// exactly `cur / max` in attribute `0x69` and that the three labels read `"cur/max"`.
/// The label's no-space separators come from DAT row ID_Vitals_VitalBarLabel; the capture
/// supplies the values, not the formatting.
///
/// Nothing in this test names a number: if the capture's character had different vitals the
/// expected values would move with it.
#[test]
fn the_vitals_show_what_the_recorded_server_sent() {
    have_dats();
    let (events, _objects) = replay("first-login-walk-jump");
    let Some(desc) = player_description(&events) else {
        panic!("first-login-walk-jump never reached 0x0013");
    };

    // What the capture says, computed independently of the HUD.
    let mut q = dereth_client_model::qualities::Qualities::new();
    q.apply_ac_qualities(&desc.qualities, LocalTime(0.0));
    let (table, filter) = {
        use dereth_assets::Decode as _;
        use dereth_primitives::AssetSource as _;
        let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("dats");
        let id = dereth_client::hud::ATTRIBUTE_2ND_TABLE;
        let filter_id = dereth_primitives::DataId(0x0E01_0001);
        (
            dereth_assets::tables::Attribute2ndTable::decode_payload(
                id,
                &store.read(id).expect("read"),
            )
            .expect("Attribute2ndTable"),
            dereth_assets::tables::QualityFilter::decode_payload(
                filter_id,
                &store.read(filter_id).expect("read"),
            )
            .expect("QualityFilter"),
        )
    };
    let expected: Vec<(u32, u32)> = dereth_ui_screens::view::Vital::ALL
        .iter()
        .map(|v| {
            let (cur, max) = v.stats();
            (
                dereth_client_model::attributes::inq_attribute_2nd(
                    &q,
                    &table,
                    cur,
                    false,
                    Some(&filter),
                )
                .expect("the capture carries a current value"),
                dereth_client_model::attributes::inq_attribute_2nd(
                    &q,
                    &table,
                    max,
                    false,
                    Some(&filter),
                )
                .expect("the capture carries a maximum"),
            )
        })
        .collect();
    assert!(
        expected.iter().all(|(_, m)| *m > 0),
        "the capture's maxima are all zero: {expected:?}"
    );

    // Now the HUD, fed the same events through the application's own path.
    let mut app = app_in_gameplay(true, 4);
    app.apply_hud_events(events_in_world(&events));
    embody(&mut app);
    app.frame();

    let Some((ui, _screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");
    let vits = ui
        .get_child_recursive(root, window::STACKED_VITALS)
        .expect("<VITS>");
    // Vitals initialization selects these six meter/label children, in `Vital::ALL` order.
    let children = [
        (ElementId(0x1000_00E6), ElementId(0x1000_00EB)),
        (ElementId(0x1000_00EC), ElementId(0x1000_00ED)),
        (ElementId(0x1000_00EE), ElementId(0x1000_00EF)),
    ];
    for (i, (meter, label)) in children.into_iter().enumerate() {
        let (cur, max) = expected[i];
        #[allow(clippy::cast_precision_loss)]
        let want = cur as f32 / max as f32;

        let m = ui.get_child_recursive(vits, meter).expect("the meter");
        let got = ui.node(m).unwrap().merged_properties().get_float(0x69);
        assert_eq!(
            got,
            Some(want),
            "meter {meter:?} is {got:?}, the capture says {cur}/{max}"
        );

        let l = ui.get_child_recursive(vits, label).expect("the label");
        let text: String = ui
            .node(l)
            .unwrap()
            .behaviour
            .as_ref()
            .map(|b| b.compose_text(ui.screen_box(l)))
            .unwrap_or_default()
            .iter()
            .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
            .collect();
        assert_eq!(text, format!("{cur}/{max}"), "label {label:?}");
    }
}

/// Oracle: `first-login-walk-jump`'s own text messages. Every `0xF7E0 Communication_TextboxString` the recorded
/// server sent must reach a chat window's log. The live scrollback element checked below is
/// `0x10000011` (`0x1000048C` is its small button).
///
/// The routing rule broadcasts `windowId == 0` subject to each window's 64-bit filter.
/// Here recorded lines must occur in a routed log, and the main log is compared with its element's
/// composed text before checking glyph emission and pixels in the scrollback rectangle.
#[test]
fn the_chat_window_shows_what_the_recorded_server_said() {
    have_dats();
    let (events, _objects) = replay("first-login-walk-jump");

    // What the capture said, taken from the session's own decoded stream.
    let mut said: Vec<String> = Vec::new();
    for e in &events {
        let Some((opcode, body)) = e.ui_body() else {
            continue;
        };
        if opcode != dereth_protocol::Opcode::COMMUNICATION_TEXTBOX_STRING {
            continue;
        }
        let mut r = dereth_protocol::archive::Reader::new(body);
        if let Ok(m) =
            <dereth_protocol::comms::CommunicationTextboxString as dereth_protocol::Message>::read(
                &mut r,
            )
        {
            said.push(m.text);
        }
    }
    // first-login-walk-jump carries two `0xF7E0 Communication_TextboxString`, so an empty list is
    // a decode regression.
    assert!(
        !said.is_empty(),
        "first-login-walk-jump carries two 0xF7E0; decoding none is a regression"
    );

    let mut app = app_in_gameplay(true, 4);
    let chat = app.apply_hud_events(events_in_world(&events));
    // At least one line per `0xF7E0`; the capture also carries speech, which is the other three
    // opcodes the chat consumes.
    assert!(
        chat.len() >= said.len(),
        "{} lines from {} 0xF7E0",
        chat.len(),
        said.len()
    );
    app.frame();

    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let root = root_of(ui).expect("the gameplay root");

    // Every recorded line occurs in at least one routed window; this membership check does not test order.
    for line in &said {
        assert!(
            screen
                .chat
                .iter()
                .any(|c| c.log_text().contains(line.as_str())),
            "{line:?} reached no chat window"
        );
    }
    // The visible main window is id 8; its default filter excludes only
    // overhead bubbles. Require a nonempty log here, then verify its element and the first recorded line.
    let main = screen
        .chat
        .iter()
        .find(|c| c.window_id == 8)
        .expect("the main chat interface");
    let log = main.log_text();
    assert!(!log.is_empty(), "the main chat window took nothing");

    // And the element a player reads carries exactly what that window's log says — a routed line
    // nobody draws is the same as no line at all.
    let chat_window = ui
        .get_child_recursive(root, window::MAIN_CHAT)
        .expect("<CHAT>");
    // The layout's 368x73 scrollback text element, not the 16x16 button beside it. The size
    // premise below rejects reading that wrong child.
    let log_element = ui
        .get_child_recursive(chat_window, ElementId(0x1000_0011))
        .expect("the chat scrollback 0x10000011");
    let b = ui.screen_box(log_element);
    assert!(
        b.width() > 300 && b.height() > 50,
        "the scrollback is {b:?}, not a 16x16 button"
    );
    let drawn: String = ui
        .node(log_element)
        .unwrap()
        .behaviour
        .as_ref()
        .map(|b| b.compose_text(ui.screen_box(log_element)))
        .unwrap_or_default()
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();
    // Compose-text output drops inter-run newlines. This comparison removes all whitespace
    // from both strings, so it checks the remaining characters rather than exact spacing.
    let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert_eq!(
        strip(&drawn),
        strip(&log),
        "the chat log element does not carry the window's log"
    );
    assert!(
        strip(&drawn).contains(&strip(&said[0])),
        "the chat log element does not carry {:?}",
        said[0]
    );

    // …and it **rasterises**: the draw list carries a command for that element with a glyph run,
    // and the pixels inside its rectangle differ from the same frame with no chat. A line that
    // reaches the element and not the screen is the same as no line at all.
    let box_of_log = ui.screen_box(log_element);
    let list = app.ui_draw_list().to_vec();
    let glyphs: usize = list
        .iter()
        .filter(|d| d.who == log_element)
        .map(|d| d.glyphs.len())
        .sum();
    assert!(glyphs > 0, "the chat log element emitted no glyphs");
    let (w, h, with) = app.renderer_mut().capture_bgra().expect("read back");

    let mut quiet = app_in_gameplay(true, 4);
    quiet.frame();
    let (w2, h2, without) = quiet.renderer_mut().capture_bgra().expect("read back");
    assert_eq!((w, h), (w2, h2));

    let mut changed = 0usize;
    for y in box_of_log.y0.max(0)..=box_of_log.y1.min(h as i32 - 1) {
        for x in box_of_log.x0.max(0)..=box_of_log.x1.min(w as i32 - 1) {
            let i = (y as usize * w as usize + x as usize) * 4;
            if with[i..i + 4] != without[i..i + 4] {
                changed += 1;
            }
        }
    }
    assert!(
        changed > 0,
        "{glyphs} glyph(s) changed no pixel inside {box_of_log:?}"
    );
}

/// The window-option blob shape, compared with recorded ACE player descriptions. The test
/// discovers the in-world recording with the largest decoded table rather than fixing a row
/// count.
///
/// Require sixteen distinct nonzero live window IDs, a nonempty decoded table, positive row keys
/// and a row for main window 8. This rejects an empty table or a zero row key, but does not pair
/// every decoded row with a live window or apply and compare every saved placement.
#[test]
fn the_window_placement_blob_decodes_and_addresses_real_windows() {
    have_dats();
    // Not every character carries an `Option_PlacementArray`, so each discovered in-world
    // recording is decoded to find one whose character saved its UI.
    let mut by_session: Vec<(String, usize)> = Vec::new();
    let mut best = None;
    let mut best_rows = 0usize;
    for session in world_sessions() {
        let (events, _objects) = replay(session);
        let Some(desc) = player_description(&events) else {
            panic!("{session} never reached 0x0013")
        };
        let p = dereth_client::hud::decode_placements(&desc.player_module);
        by_session.push((session.clone(), p.rows.len()));
        if best.is_none() || p.rows.len() > best_rows {
            best_rows = p.rows.len();
            best = Some(p);
        }
    }
    eprintln!("Option_Placement rows per capture: {by_session:?}");
    let places = best.expect("the corpus has in-world captures");

    let mut app = app_in_gameplay(true, 4);
    let Some((_ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let ids: Vec<u32> = dereth_ui_screens::GAMEPLAY_WINDOWS
        .iter()
        .map(|w| screen.window_id_of(w.element))
        .collect();
    // Every one of the sixteen windows carries a window-id attribute; the layout gives them 2…17.
    assert!(
        ids.iter().all(|i| *i != 0),
        "a window has no 0x1000007E: {ids:?}"
    );
    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        16
    );

    // A character whose UI has never been saved sends an empty bag, and then the layout's own
    // values stand when the saved-option lookup fails. For this test, if no capture in the
    // corpus carries a placement array then everything below this line is dead, so it fails.
    assert!(
        !places.rows.is_empty(),
        "no capture carries an Option_PlacementArray, so the rest of this test proves nothing: \
         {by_session:?}"
    );
    for id in places.rows.keys() {
        assert!(
            *id >= 1,
            "the array is indexed by windowID - 1, so window 0 cannot exist"
        );
    }
    // Row 7 is window 8, the main chat window, whichever way the table is walked.
    assert!(places
        .rows
        .contains_key(&dereth_ui_screens::chat::interface::window::MAIN));
}

/// Player-coordinate lookup and world-to-map conversion, with the retail Holtburg readout
/// `42.2N, 33.8E` as a geographic reference.
///
/// Compare the element text with the formatter's output, then check town-level north/east
/// bounds and axis order. The headless start differs from the retail readout's exact cell.
#[test]
fn the_radar_coordinate_line_reads_what_the_retail_client_reads() {
    have_dats();
    let mut app = app_in_gameplay(true, 4);
    let coords = app.hud().coords.expect("the player has a cell");

    let Some((ui, screen)) = gameplay_screen(&mut app) else {
        panic!("no gameplay screen")
    };
    let combined = screen
        .radar
        .combined_coords
        .expect("combined coordinate text element");
    let text: String = ui
        .node(combined)
        .unwrap()
        .behaviour
        .as_ref()
        .map(|b| b.compose_text(ui.screen_box(combined)))
        .unwrap_or_default()
        .iter()
        .map(|g| char::from_u32(u32::from(g.ch)).unwrap_or('?'))
        .collect();

    let want = dereth_ui_screens::mapradar::radar::update_coordinates(coords).combined;
    assert_eq!(
        text, want,
        "the coordinate element does not carry the computed pair"
    );

    // Holtburg. The retail client shows 42.2N, 33.8E standing in the town; the headless run starts
    // a few cells away, and a coordinate is one tenth per cell, so the assertion is the town and
    // not the exact cell.
    assert!(text.ends_with('E'), "east of the meridian: {text}");
    assert!(
        coords.0 > 41.0 && coords.0 < 44.0,
        "north {} is not Holtburg",
        coords.0
    );
    assert!(
        coords.1 > 32.0 && coords.1 < 36.0,
        "east {} is not Holtburg",
        coords.1
    );
    // And the axis order is the retail one: N/S first.
    assert!(coords.0 > coords.1);

    // Compass layout places the four letters around the authored centre at their own
    // magnitudes, a quarter turn apart. Here each radial distance is checked within 1.5 pixels;
    // angular spacing is not independently asserted. Attribute `0x1000002E` is one struct with
    // two integers, not separate float attributes (reading it as floats yields (0,0)).
    let root = root_of(ui).expect("the gameplay root");
    let radar = ui.get_child_recursive(root, window::RADAR).expect("<RADA>");
    let origin = ui.screen_origin(radar);
    let centre = screen.radar.center;
    assert!(
        centre.0 > 0.0 && centre.1 > 0.0,
        "the radar centre is {centre:?}"
    );
    assert!(screen.radar.radius > 0, "the radar has no blip radius");
    let mut placed = 0;
    for (i, which) in dereth_ui_screens::mapradar::radar::Compass::ALL
        .into_iter()
        .enumerate()
    {
        let h = match which {
            dereth_ui_screens::mapradar::radar::Compass::North => screen.radar.north,
            dereth_ui_screens::mapradar::radar::Compass::South => screen.radar.south,
            dereth_ui_screens::mapradar::radar::Compass::East => screen.radar.east,
            dereth_ui_screens::mapradar::radar::Compass::West => screen.radar.west,
        };
        let Some(h) = h else { continue };
        let b = ui.screen_box(h);
        // The token's centre, back in the radar's own coordinates.
        #[allow(clippy::cast_precision_loss)]
        let cx = (b.x0 + b.x1) as f32 / 2.0 - origin.0 as f32;
        #[allow(clippy::cast_precision_loss)]
        let cy = (b.y0 + b.y1) as f32 / 2.0 - origin.1 as f32;
        let r = ((cx - centre.0).powi(2) + (cy - centre.1).powi(2)).sqrt();
        let want = screen.radar.magnitudes[i];
        assert!(want > 0.0, "{which:?} has no orbit radius");
        assert!(
            (r - want).abs() <= 1.5,
            "{which:?} sits {r} from the centre and its magnitude is {want}"
        );
        placed += 1;
    }
    assert_eq!(
        placed, 4,
        "the shipped radar carries all four compass tokens"
    );
}

/// Oracle: the module's own counters. Everything the HUD reads is tolerant of a missing field by
/// design — a `PlayerModule` with no gameplay options, a vital the tables cannot store, a chat
/// opcode that will not decode — so every tolerance has a number and the number is asserted,
/// not merely logged.
#[test]
fn the_huds_tolerances_are_counted_and_the_ones_that_must_be_zero_are() {
    have_dats();
    let (events, _objects) = replay("first-login-walk-jump");
    let mut app = app_in_gameplay(true, 4);
    app.apply_hud_events(events_in_world(&events));
    embody(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    let s = app.hud().stats;
    assert_eq!(
        s.player_desc_applied, 1,
        "the capture's one 0x0013 was applied once"
    );
    assert_eq!(
        s.undecodable, 0,
        "a UI-queue message the HUD claims did not decode"
    );
    assert_eq!(
        s.vital_updates_unstorable, 0,
        "a vital update had nowhere to land"
    );
    assert!(s.vitals_written > 0, "the vitals bar never took a value");
    assert!(s.radar_written > 0, "the radar never took a coordinate");
    // `update_indicators` is the integration step that
    // gives the connection lamp a picture and the only thing that hides three of the four stance
    // icons, so a zero here is a HUD that silently looks wrong rather than one that fails.
    assert!(
        s.indicators_written > 0,
        "the indicator strip and the stance icon never ran"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The indicator strip and the toolbar's stance icon.
// ---------------------------------------------------------------------------------------------

/// All elements of a given type under the gameplay root, in tree-walk order.
fn only_of_type(ui: &UiSystem, ty: u32) -> Vec<ElemHandle> {
    fn walk(ui: &UiSystem, h: ElemHandle, ty: u32, out: &mut Vec<ElemHandle>) {
        if ui.node(h).is_some_and(|n| n.ty().0 == ty) {
            out.push(h);
        }
        for c in ui.children(h) {
            walk(ui, c, ty, out);
        }
    }
    let mut out = Vec::new();
    if let Some(root) = root_of(ui) {
        walk(ui, root, ty, &mut out);
    }
    out
}

/// Behaviour: hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all
///
/// **All the lamps are present.**
///
/// Retail client observation at 800x600 in Holtburg: the indicator
/// strip has **seven** occupied cells: the green chain-link lamp, two effects lamps, the vitae
/// lamp, the burden ring, the mini-game knight and the gold `X`. Element `0x100000F8` carries no
/// base image; initialization selects the good-link state, which supplies its picture.
///
/// Two claims, two pieces of evidence:
///
/// 1. **it is there**: on the live tree the lamp is in state `0x11` and carries a picture, which
///    is the state selection initialization makes;
/// 2. **that picture is the lamp's and nothing else's**: the same frame rendered twice through the
///    real device, comparing good and lost link states, with every
///    changed pixel inside the lamp's own rectangle and none outside.
#[test]
fn the_link_lamp_is_present_and_is_the_only_thing_its_state_changes() {
    have_dats();
    use dereth_ui_screens::hud::indicators::link_status::media;

    // 1. Initialization selected the good-link state, and the lamp has something to draw.
    {
        let mut app = app_in_gameplay(true, 4);
        let Some((ui, _)) = gameplay_screen(&mut app) else {
            panic!("no gameplay screen")
        };
        let lamps = only_of_type(ui, 0x1000_0003);
        assert!(
            !lamps.is_empty(),
            "the gameplay tree has no link-status indicator"
        );
        for h in &lamps {
            let n = ui.node(*h).unwrap();
            assert_eq!(
                n.state,
                dereth_ui::StateId(media::GOOD),
                "the lamp is not in its initialized good-link state"
            );
            assert!(n.region.image.is_some(), "the lamp has no picture");
        }
        // Every other lamp, and the log-out button, carries a base image of its own.
        for ty in [0x1000_0001u32, 0x1000_0002, 0x1000_0004, 0x1000_0006] {
            let found = only_of_type(ui, ty);
            assert!(
                !found.is_empty(),
                "no lamp of type {ty:#010X} in the gameplay tree"
            );
            for h in found {
                assert!(
                    ui.node(h).unwrap().region.image.is_some(),
                    "lamp {ty:#010X} draws nothing"
                );
            }
        }
        let root = root_of(ui).expect("root");
        let x = ui
            .get_child_recursive(root, dereth_ui_screens::hud::indicators::LOGOUT_BUTTON)
            .expect("the log-out button");
        assert!(
            ui.node(x).unwrap().region.image.is_some(),
            "the log-out X has no picture"
        );
    }

    // 2. The differential: state 0x11 (good link) against state 0x14 (lost), nothing else touched.
    //
    // The screen's own `update_indicators` cannot undo this inside the frame that follows:
    // The link-status timer re-reads the link only every four seconds, and this run is a
    // fraction of a second long, so it writes nothing.
    let shot = |state: u32| -> Option<(Vec<u8>, u32, u32, dereth_ui::Box2D)> {
        let mut app = app_in_gameplay(true, 4);
        let b = {
            let (ui, _) = gameplay_screen(&mut app)?;
            let h = *only_of_type(ui, 0x1000_0003).first()?;
            ui.set_state(h, dereth_ui::StateId(state));
            ui.screen_box(h)
        };
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((bgra, w, h, b))
    };
    let (good, w, h, lamp_box) = shot(media::GOOD).expect("a shot of the HUD");
    let (lost, w2, h2, _) = shot(media::LOST).expect("a shot of the HUD");
    assert_eq!((w, h), (w2, h2));

    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            if good[i..i + 4] != lost[i..i + 4] {
                if lamp_box.contains(x as i32, y as i32) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixel(s) changed outside the link lamp's own box"
    );
    assert!(inside > 0, "the four link states all draw the same thing");
}

/// **The stance icon reflects the player's combat mode.**
///
/// Retail client observation in peace mode: a white dove on green (`0x06004CEC`, element
/// `0x10000192`). `classic_gameplay` marks **all four** stance buttons visible on the same
/// rectangle; the combat-mode visibility update hides three of them, otherwise the last icon
/// drawn wins.
///
/// Same differential shape: one run with all four visible (the shipped layout state) against one
/// with the handler having run, changed pixels confined to the buttons' shared rectangle.
#[test]
fn exactly_one_stance_icon_is_shown_and_it_is_the_players_own() {
    have_dats();
    let buttons = dereth_ui_screens::toolbar::combat_mode::BUTTONS;
    let shot = |broken: bool| -> Option<(Vec<u8>, u32, u32, dereth_ui::Box2D)> {
        let mut app = app_in_gameplay(true, 4);
        let b = {
            let (ui, _) = gameplay_screen(&mut app)?;
            let root = root_of(ui)?;
            let mut b = dereth_ui::Box2D::empty();
            for (_, id) in buttons {
                let h = ui.get_child_recursive(root, id)?;
                if broken {
                    ui.set_visible(h, true); // the shipped layout's own state
                }
                let s = ui.screen_box(h);
                b = if b.is_valid() {
                    dereth_ui::Box2D::new(
                        b.x0.min(s.x0),
                        b.y0.min(s.y0),
                        b.x1.max(s.x1),
                        b.y1.max(s.y1),
                    )
                } else {
                    s
                };
            }
            b
        };
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((bgra, w, h, b))
    };

    // With the handler run, exactly one button is up, and it is the peace one — the mode a
    // `GameView` with no combat state reports and the mode the retail screenshot was taken in.
    {
        let mut app = app_in_gameplay(true, 4);
        let Some((ui, _)) = gameplay_screen(&mut app) else {
            panic!("no gameplay screen")
        };
        let root = root_of(ui).expect("root");
        let up: Vec<u32> = buttons
            .iter()
            .filter(|(_, id)| {
                ui.get_child_recursive(root, *id)
                    .and_then(|h| ui.node(h))
                    .is_some_and(|n| n.region.flags.visible)
            })
            .map(|(m, _)| *m)
            .collect();
        assert_eq!(up, vec![dereth_ui_screens::toolbar::combat_mode::NONCOMBAT]);
    }

    let (broken, w, h, box_) = shot(true).expect("a shot of the HUD");
    let (fixed, w2, h2, _) = shot(false).expect("a shot of the HUD");
    assert_eq!((w, h), (w2, h2));
    let mut inside = 0usize;
    let mut outside = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            if broken[i..i + 4] != fixed[i..i + 4] {
                if box_.contains(x as i32, y as i32) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixel(s) changed outside the stance buttons' rectangle"
    );
    assert!(
        inside > 0,
        "hiding three of the four stance icons changed nothing"
    );
}
