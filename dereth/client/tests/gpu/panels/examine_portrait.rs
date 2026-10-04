//! The identify window's 3D portrait: appraising a creature paints its model into the `<EXAM>`
//! viewport element `0x10000148` (engine class `0x0D`, same box as the stat list `0x10000149`),
//! while an appraised item gets no model because the item pane keeps the base initialization.
//! The viewport clears depth only and draws its own preview scene over the panel's background,
//! so the observable is pixels inside the element's rect: the panel with the viewport drawn
//! against the same panel with the element hidden, each arm on a fresh device and replay with
//! the same frame count, plus a noise-floor control and an empty-space control.
//! Fixture: the retail dats and the `long-solo-play` / `early-inventory-and-casting` recordings
//! replayed into a headless `App` on the gameplay screen.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::AppraisalProfile;
use dereth_protocol::{Message, Opcode};
use dereth_ui::framework::Screen;
use dereth_ui::{Box2D, ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::examination::ExamineSubUi;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

/// The portrait element found recursively beneath the creature pane's parent window; it is kept
/// as the portrait only if it is a viewport.
const CREATURE_PAPER_DOLL: ElementId = ElementId(0x1000_0148);
/// The creature's attribute list, laid over the viewport.
const CREATURE_STAT_LIST: ElementId = ElementId(0x1000_0149);
/// The viewport's registered engine element class.
const VIEWPORT_CLASS: u32 = 0x0D;

/// A Sparring Golem, in the corpus at `long-solo-play`.
const GOLEM: ObjectId = ObjectId(0x8000_09D9);
/// `early-inventory-and-casting`'s Leather Gauntlets — an *item*, whose examination pane gets no model.
const GAUNTLETS: ObjectId = ObjectId(0x8000_0674);

// ---------------------------------------------------------------------------------------------
// Harness — a recorded-session replay up to one appraisal, plus a frame capture
// ---------------------------------------------------------------------------------------------

fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

fn app_in_gameplay(frames: u32) -> App {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("an application");
    app.start_shell().expect("the shell starts");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is up");
    (ui, screen)
}

fn deliver(app: &mut App, object: ObjectId, profile: &AppraisalProfile) {
    let mut sink = dereth_client_model::RecordingSink::default();
    app.objects_mut()
        .world
        .set_appraise_info(object, profile.clone(), &mut sink);
    app.frame();
}

/// Replay `session` until the `0x00C9` for `want` lands, press identify on it and deliver the
/// reply, stopping **inside** the session so the object survives.
fn examine_recorded(app: &mut App, session: &str, want: ObjectId) -> AppraisalProfile {
    let records = shared_session(session);
    let csn = connection_sequence_number(records).unwrap_or(0);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn)
        .expect("a replay client network");
    let mut entered = false;
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(r.t);
        net.feed(&r.raw, r.peer(), now);
        net.tick(now);
        let _ = net.take_outgoing();
        let events = app.objects_mut().pump(&mut net, now);
        let _ = app.apply_hud_events(&events);
        for e in &events {
            match e {
                SessionEvent::CharacterSet(set) if !entered => {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
                SessionEvent::UiEvent { opcode, .. }
                    if *opcode == Opcode::ITEM_SET_APPRAISE_INFO =>
                {
                    let (_, body) = e.ui_body().expect("UI event");
                    let mut rd = dereth_protocol::archive::Reader::new(body);
                    let m = dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut rd)
                        .expect("a recorded 0x00C9 decodes");
                    if m.object != want {
                        continue;
                    }
                    {
                        let (_ui, screen) = gameplay_screen(app);
                        screen.examination.examine_object(m.object);
                    }
                    deliver(app, m.object, &m.profile);
                    return m.profile;
                }
                _ => {}
            }
        }
    }
    panic!("{session} carries no 0x00C9 for {:#010X}", want.0);
}

fn handle_of(app: &mut App, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = *screen.roots().first().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

/// Where the viewport element is on screen, which is the only region this file measures.
fn viewport_rect(app: &mut App) -> Box2D {
    let h = handle_of(app, CREATURE_PAPER_DOLL);
    let (ui, _) = gameplay_screen(app);
    ui.screen_clip_box(h)
}

/// How many pixels inside `rect` differ between two captures. The rect is the element's own box,
/// so a difference outside it — the text half redrawing, a cursor — cannot be mistaken for the
/// model.
fn differing_in(rect: Box2D, w: u32, h: u32, a: &[u8], b: &[u8]) -> u32 {
    assert_eq!(a.len(), b.len(), "two captures of the same device");
    let x0 = rect.x0.max(0) as u32;
    let y0 = rect.y0.max(0) as u32;
    let x1 = (rect.x1.max(0) as u32).min(w.saturating_sub(1));
    let y1 = (rect.y1.max(0) as u32).min(h.saturating_sub(1));
    let mut n = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = ((y * w + x) * 4) as usize;
            if a.get(i..i + 4) != b.get(i..i + 4) {
                n += 1;
            }
        }
    }
    n
}

/// One complete, independently driven arm of the differential.
///
/// **A fresh device and a fresh replay each time**, in `rendering::object_viewcone_cull`'s style, because the
/// world behind `<EXAM>` is animating: 21 of the capture's 37 objects carry motion, and the
/// headless clock steps by a fixed amount per frame (`Clock::fixed_step`). Two captures taken one
/// frame apart therefore differ by tens of thousands of pixels that have nothing to do with the
/// portrait.
///
/// So both arms run the **same number of frames from the same start**, and differ only in the two
/// flags. [`the_control_is_the_noise_floor`] is what proves that is true.
struct Shot {
    w: u32,
    h: u32,
    px: Vec<u8>,
    rect: Box2D,
    objects: usize,
}

/// Frames between the reply landing and the measured frame, so the panel has settled.
const SETTLE: u32 = 6;

fn arm(hide_viewport: bool, empty_space: bool) -> Shot {
    arm_with(hide_viewport, empty_space, false)
}

/// [`arm`], with the creature's attribute list (laid over the viewport) hidden or not.
fn arm_with(hide_viewport: bool, empty_space: bool, hide_list: bool) -> Shot {
    have_dats();
    let mut app = app_in_gameplay(4);
    let profile = examine_recorded(&mut app, "long-solo-play", GOLEM);
    assert!(
        profile.creature_profile.is_some(),
        "0x800009D9 must carry the 0x0100 creature block"
    );

    // The panel must really be open on the creature pane, or every count below is a confident
    // zero taken of a hidden element.
    {
        let (ui, screen) = gameplay_screen(&mut app);
        assert!(
            screen.examination.is_open(ui),
            "<EXAM> is open after a reply"
        );
        assert_eq!(
            screen.examination.active,
            Some(ExamineSubUi::Creature),
            "a creature with no Template and no title takes the creature pane"
        );
    }
    for _ in 0..SETTLE {
        app.frame();
    }

    if empty_space {
        app.renderer_mut().clear_examine_preview();
    }
    if hide_viewport {
        let h = handle_of(&mut app, CREATURE_PAPER_DOLL);
        let (ui, _) = gameplay_screen(&mut app);
        ui.set_visible(h, false);
    }
    if hide_list {
        let h = handle_of(&mut app, CREATURE_STAT_LIST);
        let (ui, _) = gameplay_screen(&mut app);
        ui.set_visible(h, false);
    }
    let rect = viewport_rect(&mut app);
    let objects = app.renderer().examine_preview_objects();

    app.frame();
    let (w, h, px) = app
        .renderer_mut()
        .capture_bgra()
        .expect("a headless frame reads back");
    app.shutdown();
    Shot {
        w,
        h,
        px,
        rect,
        objects,
    }
}

// =================================================================================================
// 1. The premise: the element the client binds is in the shipped layout, and it is a viewport
// =================================================================================================

/// **Oracle: the live `<EXAM>` tree.** `0x10000148` exists, is engine class `0x0D`, is visible and
/// has a real box. If any of that were false the rest of this file would be measuring nothing.
///
/// It also records the fact that makes the compositing work: `0x10000148` and the stat list
/// `0x10000149` have the **same box**, so retail draws the model behind the numbers rather than
/// beside them.
#[test]
fn the_examine_window_hosts_a_viewport_element_at_0x10000148() {
    have_dats();
    let mut app = app_in_gameplay(4);
    let h = handle_of(&mut app, CREATURE_PAPER_DOLL);
    let stats = handle_of(&mut app, ElementId(0x1000_0149));
    let (ui, _) = gameplay_screen(&mut app);
    let n = ui.node(h).expect("the node");
    assert_eq!(n.ty().0, VIEWPORT_CLASS, "viewport element, class 0x0D");
    let b = n.region.box_;
    assert!(
        b.x1 > b.x0 && b.y1 > b.y0,
        "the viewport has a real box: {b:?}"
    );
    let s = ui.node(stats).expect("the stat list").region.box_;
    assert_eq!(
        (b.x0, b.y0, b.x1, b.y1),
        (s.x0, s.y0, s.x1, s.y1),
        "model behind the numbers"
    );
    app.shutdown();
}

// =================================================================================================
// 2. The noise floor, taken before the measurement
// =================================================================================================

/// **Taken first.** Two runs of the *same* arm must agree pixel for pixel inside the rect.
///
/// This is the assertion that makes the two below mean anything: it says the replay, the clock and
/// the world's animation are reproducible, so a difference between the arms can only be the flag
/// that separates them.
#[test]
fn the_control_is_the_noise_floor() {
    let a = arm(false, false);
    let b = arm(false, false);
    assert_eq!((a.w, a.h), (b.w, b.h), "the same device size");
    assert_eq!(a.rect, b.rect, "the same element rect");
    let n = differing_in(a.rect, a.w, a.h, &a.px, &b.px);
    assert_eq!(
        n, 0,
        "two identical arms must agree; {n} pixels differ, so the arms are not equal"
    );
}

// =================================================================================================
// 3. The measurement
// =================================================================================================

/// Behaviour: examine.portrait.appraising-a-creature-paints-its-model-and-an-item-gets-none
///
/// **The portrait is drawn.** Two identical arms but for the viewport element's visibility, and
/// the pixels that change inside its box.
///
/// On every appraisal reply the chosen pane is initialized (object id and data) before its text
/// is updated. Creature-pane initialization poses the preview scene:
///
/// ```text
/// set a distant light (type 1), intensity 2.0, direction (0.3, 1.9, 0.65);
/// remove all previous preview objects;
/// look up the live appraised object by ID and clone it;
/// set clone heading to 191.3679 degrees with the second argument 1;
/// obtain clone bounding-box extents dx, dy, dz and viewport width w, height h;
/// if dz/dx < h/w: dz := dx * (h/w); // otherwise retain dz, including unordered comparison
/// distance := dz * 1.2071068 + dy * 0.5;
/// set the preview camera;
/// add the clone to the preview scene;
/// ```
///
/// The single-precision constants have bit patterns `0x3E99999A`, `0x3FF33333`, `0x3F266666` for
/// the light direction, `0x40000000` for intensity, `0x433F5E2F` for heading and `0x3F9A827A` for
/// the fit factor. `1.2071068` is `cot(22.5 deg) / 2`; the preview's initial FOV is `0.7853982`
/// radians (45 degrees), so the distance fits the selected vertical extent into that FOV and adds
/// half the object's depth. The camera setter's second argument is not a look-at target: it
/// resets orientation to `(0,0,0)` and rotates by that argument, and initialization passes
/// `(0,0,0)`, leaving identity orientation looking down `+Y`.
///
/// The viewport render sets the device viewport to the element's box, clears **depth only** to
/// 1.0 (the black colour does not clear the colour buffer), draws the preview and restores the
/// viewport, so the model composites over the panel. Taking the element away must therefore take
/// a large patch of pixels with it; with no `PreviewSpace` queued for `0x10000148`, the count is 0.
#[test]
fn appraising_a_creature_paints_its_model_into_the_identify_panel() {
    let shown = arm(false, false);
    let hidden = arm(true, false);

    // The denominator: the preview scene must contain the creature, or a
    // non-zero pixel count below would be measuring something else entirely.
    assert!(
        shown.objects >= 1,
        "the identify window's creature-preview mode holds {} objects; initialization adds the appraised \
         creature's cloned physics object",
        shown.objects
    );

    let n = differing_in(shown.rect, shown.w, shown.h, &shown.px, &hidden.px);
    println!(
        "portrait: {n} pixels inside {:?} depend on the viewport element",
        shown.rect
    );
    assert!(
        n > 500,
        "the identify panel's viewport 0x10000148 painted {n} pixels; retail draws the appraised \
         creature's model there during basic creature-examination initialization"
    );
}

/// **The instrument can fail, deliberately.** The same differential with the model taken out of
/// the preview space first must collapse to zero.
///
/// The viewport render leaves the device untouched when its space holds no object. An empty
/// space is therefore exactly "the panel as the UI left it", and hiding the element
/// can move nothing. An instrument that still reported a large number here would be reading the
/// window, the text or the world rather than the model.
#[test]
fn taking_the_model_out_of_the_space_collapses_the_difference() {
    let shown = arm(false, true);
    let hidden = arm(true, true);
    assert_eq!(
        shown.objects, 0,
        "the space was emptied before the measured frame"
    );
    let n = differing_in(shown.rect, shown.w, shown.h, &shown.px, &hidden.px);
    assert_eq!(
        n, 0,
        "an empty viewport draws nothing, so hiding it must move 0 pixels, not {n} -- if this is \
         non-zero the differential above is not measuring the model"
    );
}

// =================================================================================================
// 4. The item pane inherits base initialization without a portrait
// =================================================================================================

/// Base pane initialization only stores the object ID and object data; the item pane does not
/// override it. An appraised **item** therefore gets no model, rather than a portrait on
/// everything.
#[test]
fn an_item_gets_no_portrait_because_its_init_is_the_base_one() {
    have_dats();
    let mut app = app_in_gameplay(4);
    let profile = examine_recorded(&mut app, "early-inventory-and-casting", GAUNTLETS);
    assert!(
        profile.creature_profile.is_none(),
        "the gauntlets are not a creature"
    );
    {
        let (_ui, screen) = gameplay_screen(&mut app);
        assert_eq!(
            screen.examination.active,
            Some(ExamineSubUi::Item),
            "the item pane"
        );
    }
    for _ in 0..SETTLE {
        app.frame();
    }
    assert_eq!(
        app.renderer().examine_preview_objects(),
        0,
        "the item-examination panel inherits shared examination-subpanel initialization, which adds no object"
    );
    app.shutdown();
}

/// Behaviour: examine.portrait.the-model-is-drawn-over-the-attribute-lists-ground
///
/// Oracle: the same model with the attribute list hidden. The model's pixels are those that
/// differ from an empty space; wherever the list's text does not fall on them, they are the same
/// with the list shown, so the list's translucent rows do not shade the model.
#[test]
fn the_creatures_attribute_list_does_not_shade_its_model() {
    let shown = arm_with(false, false, false);
    let bare = arm_with(false, false, true);
    let empty_shown = arm_with(false, true, false);
    let empty_bare = arm_with(false, true, true);
    let rect = shown.rect;
    let (w, h) = (shown.w, shown.h);
    let (mut model, mut shaded) = (0u32, 0u32);
    let x1 = (rect.x1.max(0) as u32).min(w - 1);
    let y1 = (rect.y1.max(0) as u32).min(h - 1);
    for y in rect.y0.max(0) as u32..=y1 {
        for x in rect.x0.max(0) as u32..=x1 {
            let i = ((y * w + x) * 4) as usize;
            let px = |s: &Shot| s.px.get(i..i + 4).map(<[u8]>::to_vec);
            // A model pixel, where no text of the list falls.
            if px(&bare) == px(&empty_bare)
                || px(&empty_shown) != px(&empty_bare) && {
                    let p = px(&empty_shown).unwrap_or_default();
                    p.iter().take(3).any(|&c| c > 150)
                }
            {
                continue;
            }
            model += 1;
            if px(&shown) != px(&bare) {
                shaded += 1;
            }
        }
    }
    assert!(model > 1000, "the model covers {model} pixels");
    assert!(
        shaded * 10 < model,
        "{shaded} of the model's {model} pixels change when the list is shown"
    );
}
