//! Panel pixel differentials: installing a cooldown repaints only the wedge on that item's slot,
//! the open-container frame draws its own pixels inside its slot, and a `+` on the char-gen Skills
//! page changes pixels only inside the skills list and the credit meter. Fixture: offline headless
//! `App`s on the device; the inventory tests replay the early-inventory-and-casting recording
//! through a `ClientNetwork` that talks to nobody, and the Skills test drives the char-gen wizard
//! at 800x600. Frames are compared in memory; nothing is written to disk.
//!
//! # The cooldown wedge
//!
//! Two renders of **one** application through the real device, differing only in whether the
//! player's own cooldown registry holds an entry for the item's cooldown id + 0x8000. Nothing else
//! on this screen reads that list: the vitae lamp reads the vitae value, the effects counts are
//! left at the trait's default, and the object's own `PublicWeenieDesc` is untouched between the
//! two runs. So the declared rectangle is **one item slot**, and every changed pixel has to be
//! inside it. A headless capture is not the windowed path; this is the deterministic half.
//!
//! # The char-gen Skills page before and after a `+`
//!
//! The same page, the same character, before and after the click. The claim "the row moved" is a
//! claim about pixels, and a differ that cannot report a non-zero supplies no evidence for a zero,
//! so the differ is calibrated **in both directions inside the test**: a frame against itself must
//! be 0, and the before/after pair must not be. The frames use an 800x600 panel size, comparable
//! with a separately captured windowed frame.

#![cfg(gpu)]

use crate::common::client_dir;

use dereth_chargen::SkillAdvancementClass;
use dereth_client::app::App;
use dereth_client_net::client_session::testing::{peer, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::framework::mode;
use dereth_ui::{Box2D, ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::chargen::{self, CharGenScreen, EcgProgress};
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

// ---------------------------------------------------------------------------------------------
// The cooldown wedge and the open-container frame
// ---------------------------------------------------------------------------------------------

const SESSION: &str = "early-inventory-and-casting";

/// Replay the whole capture, returning the events **and** the object stream they built.
///
/// Both are needed: `Hud::apply_events` fills the qualities and the two inventory lists, but the
/// `Weenie` rows every decoration reads come from `0xF745 Item_CreateObject` and live in the
/// `ObjectStream`. An app fed only the events has an empty object table and decorates nothing.
fn replay(session: &str) -> (Vec<SessionEvent>, ObjectStream) {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).unwrap_or(0),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
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

/// Everything up to the capture's logout, measured before that logout destroys every object.
///
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

/// The record index at which the session held the most objects -- every capture ends with a logout
/// that destroys all of them, so the measurement must precede that event.
fn busiest(session: &str) -> ObjectStream {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).unwrap_or(0),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut best: Option<usize> = None;
    let mut peak = 0usize;
    for (i, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
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
        }
        if objects.world.tables.weenies.len() > peak {
            peak = objects.world.tables.weenies.len();
            best = Some(i + 1);
        }
    }
    let stop = best.expect("the capture creates objects");
    // Replay again, stopping at the peak.
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).unwrap_or(0),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    for r in records.iter().take(stop) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, peer(r.pair), now);
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
        }
    }
    assert!(
        objects.world.player.is_some(),
        "the replay reached a player"
    );
    objects
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

/// Open the inventory page the way the toolbar's backpack button opens it.
fn open_the_backpack(app: &mut App) {
    let (ui, screen) = gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

/// The screen rectangle of the slot holding `id`, in any of the inventory page's lists.
fn slot_box(app: &mut App, id: ObjectId) -> Box2D {
    let (ui, screen) = gameplay_screen(app);
    let p = &screen.inventory;
    let h = p
        .top_container
        .iter()
        .chain(p.container_list.iter())
        .chain(p.item_list.iter())
        .chain(p.doll.iter().map(|(_, w)| w))
        .find_map(|w| {
            w.slots
                .iter()
                .find(|s| s.item == Some(id))
                .map(|s| s.handle)
        })
        .unwrap_or_else(|| panic!("{id:?} has no live slot"));
    ui.screen_box(h)
}

/// The capture's own cooldown-bearing objects.
fn cooldown_items(app: &App) -> Vec<(ObjectId, String, u32, f64)> {
    let mut v: Vec<_> = app
        .objects()
        .world
        .tables
        .weenies
        .iter()
        .filter_map(|(id, x)| {
            let cd = x.pwd.cooldown_id.unwrap_or(0);
            (cd > 0).then(|| {
                (
                    id,
                    x.pwd.name.clone(),
                    cd,
                    x.pwd.cooldown_duration.unwrap_or(0.0),
                )
            })
        })
        .collect();
    v.sort_by_key(|(id, ..)| id.0);
    v
}

fn install_cooldown(app: &mut App, cooldown_id: u32, start: f64, duration: f64) {
    use dereth_client_model::enchant::{ench_type, Enchantment};
    let q = app
        .probe_mut()
        .objects_mut()
        .world
        .player_qualities_mut()
        .expect("the capture's 0x0013");
    q.enchantments.cooldown_list.push(Enchantment {
        id: cooldown_id + 0x8000,
        spell_category: 0,
        power_level: 0,
        start_time: start,
        duration,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: start,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: ench_type::COOLDOWN,
            key: 0,
            value: 0.0,
        },
        spell_set_id: None,
    });
}

/// Behaviour: inventory.cooldown.installing-a-cooldown-repaints-only-its-wedge
///
/// **Two renders differing only in one entry of the player's cooldown list.**
///
/// **Falsified by** deleting the ten visibility updates in `ItemSlot::update_cooldown_display`,
/// which drops the differential to **0** changed pixels; and by pointing the wedge write at a
/// different child, which makes the outside count non-zero.
#[test]
fn installing_a_cooldown_repaints_only_the_wedge_on_that_items_slot() {
    let (events, _) = replay(SESSION);
    let in_world = events_in_world(&events).to_vec();

    let shot = |cooldown: bool, name: &str| -> (u32, u32, Vec<u8>, Box2D, Option<usize>) {
        let mut app = app_in_gameplay(4);
        *app.probe_mut().objects_mut() = busiest(SESSION);
        let _ = app.apply_hud_events(&in_world);
        open_the_backpack(&mut app);
        for _ in 0..6 {
            app.frame();
        }
        let (id, item, cd, duration) = cooldown_items(&app)
            .into_iter()
            .next()
            .expect("the capture's cooldown item");
        if cooldown {
            // The corpus carries items with a cooldown id but no registry with a cooldown entry,
            // so the lit arm cannot come from the recording; everything else here is
            // early-inventory-and-casting's own.
            install_cooldown(&mut app, cd, 0.0, duration);
        }
        // Past the one-second heartbeat interval, so the global frame notice has fired.
        // `--headless` steps the current time by `HEADLESS_STEP` (1/30 s) per frame, so a second
        // is 30 frames and not 4; the same count runs in both arms, so the two frames still differ
        // only in the cooldown.
        for _ in 0..40 {
            app.frame();
        }
        let b = slot_box(&mut app, id);
        let wedge = {
            let (_, screen) = gameplay_screen(&mut app);
            let p = &screen.inventory;
            p.top_container
                .iter()
                .chain(p.container_list.iter())
                .chain(p.item_list.iter())
                .chain(p.doll.iter().map(|(_, w)| w))
                .find_map(|w| w.slots.iter().find(|s| s.item == Some(id)))
                .and_then(|s| s.cooldown_wedge)
        };
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("an offscreen capture");
        eprintln!(
            "cooldown shot {name}: {item} {id:?} cd={cd} dur={duration} slot={b:?} wedge={wedge:?}"
        );
        (w, h, bgra, b, wedge)
    };

    let (w, h, a, box_a, wedge_a) = shot(false, "backpack-no-cooldown");
    let (w2, h2, b, box_b, wedge_b) = shot(true, "backpack-cooldown-wedge");
    assert_eq!((w, h), (w2, h2));
    assert_eq!(box_a, box_b, "the slot moved between the two runs");
    assert!(
        box_a.width() > 0 && box_a.height() > 0,
        "the slot has no box: {box_a:?}"
    );
    assert_eq!(wedge_a, None, "no cooldown, no wedge");
    assert!(
        wedge_b.is_some(),
        "a live cooldown lights one, or the differential proves nothing"
    );

    let inside = |r: Box2D, x: i32, y: i32| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1;
    let (mut in_slot, mut outside) = (0usize, 0usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if a[i..i + 4] == b[i..i + 4] {
                continue;
            }
            if inside(box_a, x, y) {
                in_slot += 1;
            } else {
                outside += 1;
            }
        }
    }
    eprintln!(
        "cooldown differential: wedge {wedge_a:?} -> {wedge_b:?}; changed pixels: slot {in_slot} \
         {box_a:?}, outside {outside}"
    );
    assert!(
        in_slot > 0,
        "installing a cooldown repainted no pixel of the item's slot"
    );
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the slot the cooldown belongs to"
    );
}

/// **The open-container frame's own pixels.**
///
/// The open-container overlay is `0x10000450`, image `0x06005D9C`, a 36x36 element that exists on
/// exactly one of the twenty shipped `ItemSlot` roots -- `0x1000033F`, which only the two
/// `UI_ItemList_IsContainer` lists name. Its state machine is asserted against the element tree
/// elsewhere; this is the differential that says the frame reaches the screen.
///
/// The two renders differ by one open-container state update and nothing else -- not by which
/// container is open, which would refill the grid and change half the window. That is deliberate:
/// the question here is "does this element draw", and a differential whose two arms differ in more
/// than the subject cannot answer it.
///
/// **Falsified by** `ItemSlot::set_open_container_state` not writing visibility, which drops the
/// differential to 0 changed pixels.
#[test]
fn the_open_container_frame_draws_its_own_pixels() {
    let (events, _) = replay(SESSION);
    let in_world = events_in_world(&events).to_vec();

    let shot = |frame_on: bool| -> (u32, u32, Vec<u8>, Box2D) {
        let mut app = app_in_gameplay(4);
        *app.probe_mut().objects_mut() = busiest(SESSION);
        let _ = app.apply_hud_events(&in_world);
        open_the_backpack(&mut app);
        for _ in 0..6 {
            app.frame();
        }
        let player = app.objects().world.player.expect("a player");
        let b = slot_box(&mut app, player);
        {
            let (ui, screen) = gameplay_screen(&mut app);
            let w = screen
                .inventory
                .top_container
                .as_mut()
                .expect("the top inventory container exists");
            let s = w
                .slots
                .iter_mut()
                .find(|s| s.item == Some(player))
                .expect("the player's slot");
            s.set_open_container_state(ui, frame_on);
        }
        app.frame();
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("an offscreen capture");
        (w, h, bgra, b)
    };

    let (w, h, off, box_a) = shot(false);
    let (w2, h2, on, box_b) = shot(true);
    assert_eq!((w, h), (w2, h2));
    assert_eq!(box_a, box_b);
    assert_eq!(
        (box_a.width(), box_a.height()),
        (36, 36),
        "the container slot root is 36x36"
    );

    let inside = |r: Box2D, x: i32, y: i32| x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1;
    let (mut in_slot, mut outside) = (0usize, 0usize);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let i = (y as usize * w as usize + x as usize) * 4;
            if off[i..i + 4] == on[i..i + 4] {
                continue;
            }
            if inside(box_a, x, y) {
                in_slot += 1;
            } else {
                outside += 1;
            }
        }
    }
    eprintln!(
        "open-container differential: changed pixels: slot {in_slot} {box_a:?}, \
         outside {outside}"
    );
    assert!(in_slot > 0, "the open-container frame drew nothing");
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the container slot"
    );
}

// ---------------------------------------------------------------------------------------------
// The char-gen Skills page
// ---------------------------------------------------------------------------------------------

/// Every fixture path is an `expect`, never a skip.
fn require_dats() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
}

fn wizard(app: &mut App) -> &mut CharGenScreen {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<CharGenScreen>()
        .expect("the wizard is up")
}

fn click(app: &mut App, id: ElementId) {
    let shell = app.ui_mut().expect("the shell is up");
    let root = shell.flow.current().expect("a screen is up").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("element {id:?} is in the shipped layout"));
    click_handle(app, h);
}

fn click_handle(app: &mut App, h: ElemHandle) {
    let shell = app.ui_mut().expect("the shell is up");
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    for _ in 0..4 {
        app.frame();
    }
}

/// The windowed run's panel size.
const SHOT: (u32, u32) = (800, 600);

/// A captured frame: width, height and BGRA bytes, read back in memory.
type Frame = (u32, u32, Vec<u8>);

/// Count the differing pixels between two frames, and the rectangle they fall in.
///
/// Written here rather than shelled out to, so that the calibration below is over the same code
/// that produces the number.
fn diff(a: &Frame, b: &Frame) -> (u64, Option<(u32, u32, u32, u32)>) {
    let ia = image_of(a);
    let ib = image_of(b);
    assert_eq!(ia.0, ib.0, "the two frames are the same size");
    assert_eq!(ia.1, ib.1, "the two frames are the same size");
    let (w, h) = (ia.0, ia.1);
    let mut n = 0u64;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if ia.2[i] != ib.2[i] {
                n += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (n, if n == 0 { None } else { Some((x0, y0, x1, y1)) })
}

/// One captured frame as `(width, height, colour triples)`; alpha is not compared.
fn image_of(f: &Frame) -> (u32, u32, Vec<[u8; 3]>) {
    let rgb = f.2.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
    (f.0, f.1, rgb)
}

/// **A frame before and after one `+`.**
///
/// Aluvian / Custom, 52 credits, then a `+` on *Armor Tinkering*, with a frame taken before the
/// click and a frame taken after it.
#[test]
fn the_skills_page_before_and_after_a_plus() {
    require_dats();
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: SHOT.0,
        height: SHOT.1,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    assert!(
        wizard(&mut app).tables.is_some(),
        "the host handed the wizard its tables"
    );

    click(&mut app, chargen::HERITAGE_BUTTONS[0].0); // Aluvian
    click(
        &mut app,
        EcgProgress::Profession
            .select_button()
            .expect("the profession tab"),
    );
    click(&mut app, chargen::PROFESSION_BUTTONS[0].0); // Custom -- 52 unspent credits
    click(
        &mut app,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    assert_eq!(wizard(&mut app).progress, EcgProgress::Skills);
    assert_eq!(
        wizard(&mut app).state.remaining_skill_credits,
        52,
        "the Custom profession's starting credits"
    );

    let before = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");

    // **The click target for the windowed run**, printed rather than eyeballed off a frame.
    // A drawn subject also needs a windowed run. The input driver accepts WINDOW coordinates;
    // these printed targets are CLIENT coordinates, so the driver converts them by measuring the
    // border rather than assuming it.
    for (what, id) in [
        ("heritage Aluvian", chargen::HERITAGE_BUTTONS[0].0),
        (
            "profession tab",
            EcgProgress::Profession.select_button().expect("tab"),
        ),
        ("profession Custom", chargen::PROFESSION_BUTTONS[0].0),
        (
            "skills tab",
            EcgProgress::Skills.select_button().expect("tab"),
        ),
    ] {
        let shell = app.ui().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let h = shell
            .ui
            .get_child_recursive(root, id)
            .expect("in the shipped layout");
        let b = shell.ui.screen_box(h);
        eprintln!(
            "skills click target: {what:20} client centre ({}, {})",
            (b.x0 + b.x1) / 2,
            (b.y0 + b.y1) / 2
        );
    }

    // The Armor Tinkering row, found through the model rather than by position.
    let i = wizard(&mut app)
        .skill_rows
        .iter()
        .position(|r| r.name == "Armor Tinkering")
        .expect("Armor Tinkering has a row");
    let row = wizard(&mut app).skill_rows[i]
        .element
        .expect("its row element");
    let y_before = app
        .ui()
        .expect("shell")
        .ui
        .node(row)
        .expect("live")
        .region
        .box_
        .y0;
    let plus = app
        .ui()
        .expect("shell")
        .ui
        .get_child_recursive(row, chargen::skills_page::ROW_INCREASE)
        .expect("the + button");
    {
        let shell = app.ui().expect("shell");
        let b = shell.ui.screen_box(plus);
        eprintln!(
            "skills click target: {:20} client centre ({}, {})  [row y0 {y_before}]",
            "+ Armor Tinkering",
            (b.x0 + b.x1) / 2,
            (b.y0 + b.y1) / 2
        );
    }
    click_handle(&mut app, plus);
    let skill = wizard(&mut app).skill_rows[i].skill;
    assert_eq!(
        wizard(&mut app).state.skill_level(skill),
        SkillAdvancementClass::Trained,
        "the + trained it"
    );
    let y_after = app
        .ui()
        .expect("shell")
        .ui
        .node(row)
        .expect("live")
        .region
        .box_
        .y0;

    let after = app
        .renderer_mut()
        .capture_bgra()
        .expect("the frame is captured");

    // ---- calibrate the differ in BOTH directions before believing either number -------------
    let (zero, _) = diff(&before, &before);
    assert_eq!(zero, 0, "the differ reads 0 on a frame against itself");
    let (moved, rect) = diff(&before, &after);
    assert!(
        moved > 0,
        "the differ reads a non-zero on a pair that is known to differ -- without this, \"0\" and \
         \"my differ is broken\" are the same reading"
    );
    let (rx0, ry0, rx1, ry1) = rect.expect("a non-zero difference has a rectangle");
    eprintln!(
        "skills page: calibration 0 px (before vs before), {moved} px (before vs after) in \
         x {rx0}..{rx1}, y {ry0}..{ry1}; Armor Tinkering's row y0 {y_before} -> {y_after}"
    );

    // The whole difference is inside the skills list box, which is what "the row moved and
    // nothing else did" means in pixels.
    let list = {
        let shell = app.ui().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let h = shell
            .ui
            .get_child_recursive(root, chargen::skills_page::LIST)
            .expect("the skills list box");
        shell.ui.screen_box(h)
    };
    let credits = {
        let shell = app.ui().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let h = shell
            .ui
            .get_child_recursive(root, chargen::skills_page::CREDITS_FIELD)
            .expect("the credits meter");
        shell.ui.screen_box(h)
    };
    let inside = |x: i32, y: i32, b: dereth_ui::region::Box2D| {
        x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1
    };
    let (ia, ib) = (image_of(&before), image_of(&after));
    let mut outside = 0u64;
    for y in 0..ia.1 {
        for x in 0..ia.0 {
            let i = (y * ia.0 + x) as usize;
            if ia.2[i] != ib.2[i] {
                let (xi, yi) = (x as i32, y as i32);
                if !inside(xi, yi, list) && !inside(xi, yi, credits) {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "every changed pixel is inside the skills list or the credit meter (list {list:?}, \
         credits {credits:?})"
    );
    eprintln!(
        "skills page: {moved} changed pixels, 0 outside the skills list {list:?} and the credit \
         meter {credits:?}"
    );
    app.shutdown();
}
