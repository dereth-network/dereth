//! What the in-game HUD draws: it rasterises over the world only inside the rectangles its draw
//! list claims, it fits in the descriptor heap and exhausts nothing, and the vitals vial fills in
//! proportion to the recorded value.
//! Fixture: `panels::gameplay_hud`'s headless App in gameplay over the retail dats, drawn through
//! the selected GPU backend at 800x600, with the recorded sessions replayed for the vial's value.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::panels::gameplay_hud::{
    app_in_gameplay, events_in_world, gameplay_screen, have_dats, replay, root_of,
};

use dereth_client_net::client_session::SessionEvent;
use dereth_ui::{ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::window;

type Shot = (
    u32,
    u32,
    Vec<u8>,
    Vec<dereth_ui::UiDrawCmd>,
    dereth_client_shell::ui_draw::UiTextureStats,
);

type VitalShot = ((u32, u32), Vec<u8>, u32, u32);

/// The HUD draws over the world: **two runs of the same scene differing only in the HUD**,
/// through the real
/// selected GPU device, with every changed pixel inside the rectangles the draw list claimed and none
/// outside.
///
/// This is also what catches the compositing hazard `GamePlayScreen::let_the_world_through`
/// describes: the gameplay root carries a full-screen background image, and painting it from the
/// UI overlay — which runs *after* the 3D pass here and *before* the smart box's own blit in the
/// retail client — can obscure the world with a brown rectangle. The checks below require a
/// nonempty HUD covering less than half the frame and no changed pixels outside its claimed boxes.
#[test]
fn the_hud_rasterises_over_the_world_and_only_where_it_said_it_would() {
    have_dats();
    let shot = |ui: bool| -> Option<Shot> {
        let mut app = app_in_gameplay(ui, 8);
        let list = app.ui_draw_list().to_vec();
        let stats = app.renderer_mut().ui_stats;
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, list, stats))
    };

    let (w, h, without, _, _) = shot(false).expect("a shot of the HUD");
    let (w2, h2, with, list, stats) = shot(true).expect("a shot of the HUD");
    assert_eq!((w, h), (w2, h2));

    assert!(stats.quads_drawn > 0, "the HUD submitted no geometry");
    assert!(stats.uploaded > 0, "no HUD image was uploaded");
    assert_eq!(stats.decode_failures, 0, "a HUD image would not decode");
    assert_eq!(
        stats.skipped_draws, 0,
        "a draw was skipped for want of a texture"
    );

    // The world was drawn in both runs: without the HUD the frame is not black.
    let lit = without
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] | p[1] | p[2] != 0)
        .count();
    assert!(
        lit > (w * h / 4) as usize,
        "the world drew only {lit} lit pixels"
    );

    // The rectangles the HUD claimed. A command claims its rectangle when it has an image **or**
    // glyphs: a transparent text element emits a command with no image and only a glyph run.
    let mut covered = vec![false; (w * h) as usize];
    for cmd in &list {
        if cmd.image.is_none() && cmd.glyphs.is_empty() {
            continue;
        }
        let x0 = cmd.screen.x0.max(cmd.clip.x0).max(0);
        let y0 = cmd.screen.y0.max(cmd.clip.y0).max(0);
        let x1 = cmd.screen.x1.min(cmd.clip.x1).min(w as i32 - 1);
        let y1 = cmd.screen.y1.min(cmd.clip.y1).min(h as i32 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                covered[(y as u32 * w + x as u32) as usize] = true;
            }
        }
    }
    let claimed = covered.iter().filter(|c| **c).count();
    assert!(claimed > 0, "the HUD claimed no pixels");
    // The world must still be most of the frame: the HUD is chrome around a viewport, not a
    // full-screen panel. A regression that leaves the panel stack up claims far more than this.
    assert!(
        claimed < (w * h / 2) as usize,
        "the HUD claimed {claimed} of {} pixels -- it is covering the world",
        w * h
    );

    let mut inside = 0usize;
    let mut outside = 0usize;
    for (i, c) in covered.iter().enumerate() {
        if without[i * 4..i * 4 + 4] != with[i * 4..i * 4 + 4] {
            if *c {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixel(s) changed outside every rectangle the HUD claimed"
    );
    assert!(
        inside > 0,
        "the HUD claimed {claimed} pixels and changed none of them"
    );
}

/// Descriptor discipline: the whole gameplay layout is 114 UI images, and the HUD as it comes up
/// holds a fraction of them.
///
/// This test uses the current reported capacity, requires no exhaustion or unknown releases,
/// and keeps HUD texture count below 114. It does not perform a mode switch or measure reclamation.
#[test]
fn the_hud_fits_in_the_descriptor_heap_and_exhausts_nothing() {
    have_dats();
    let app = app_in_gameplay(true, 8);
    let d = app.renderer().descriptor_usage();
    let s = app.renderer().descriptor_stats();
    assert_eq!(s.exhaustions, 0, "the descriptor heap was exhausted");
    assert!(d.live <= d.capacity, "{} live of {}", d.live, d.capacity);
    assert!(
        d.high_water < d.capacity,
        "high water {} of {}",
        d.high_water,
        d.capacity
    );
    let r = app.ui_release_report();
    assert_eq!(r.unknown, 0, "a UI texture was released twice");

    // And the panel-visibility fix is what keeps it small: the whole gameplay layout is 114 images
    // and the HUD as it comes up is a fraction of that.
    let held = app.renderer().ui_texture_count();
    assert!(held > 0, "the HUD uploaded no images at all");
    assert!(
        held < 114,
        "the HUD holds {held} images -- every panel is drawing again"
    );
}

/// Every change of `(cur, max)` produced by a recorded session for one vital, in event order.
/// Consecutive repeats are collapsed; a value that recurs later is retained again.
///
/// The numbers are read out of the capture — its `0x0013 Login_PlayerDescription` and the
/// `Qualities_*Attribute2nd*` updates that follow — through the current secondary-attribute query.
/// Nothing here hardcodes a vital value. `first-login-walk-jump`'s stamina is the interesting series: the
/// recorded player jumped, and ACE sent the drain and the regeneration ticks back.
fn recorded_series(
    events: &[SessionEvent],
    which: dereth_ui_screens::view::Vital,
) -> Vec<(u32, u32)> {
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("dats");
    let table = {
        use dereth_assets::Decode as _;
        use dereth_primitives::AssetSource as _;
        let id = dereth_client_runtime::hud::ATTRIBUTE_2ND_TABLE;
        dereth_assets::tables::Attribute2ndTable::decode_payload(id, &store.read(id).expect("read"))
            .expect("Attribute2ndTable")
    };
    let filter = {
        use dereth_assets::Decode as _;
        use dereth_primitives::AssetSource as _;
        let id = dereth_primitives::DataId(0x0E01_0001);
        dereth_assets::tables::QualityFilter::decode_payload(id, &store.read(id).expect("read"))
            .expect("QualityFilter")
    };
    let (cur, max) = which.stats();
    let mut hud = dereth_client_shell::hud::Hud::default();
    // `apply_events` takes the object tables (`0x0013`'s two inventory lists land in them); this
    // loop cares only about the qualities, which live in that world too, on the player's own
    // weenie. The description belongs only to the identified player object, so this scratch
    // world creates that row and sets the player before replay. The query reads the shared world
    // qualities.
    let mut world = dereth_client_model::World::new();
    {
        let player = dereth_primitives::ObjectId(0x5000_0001);
        world
            .tables
            .weenies
            .insert(player, dereth_client_model::weenie::Weenie::new(player));
        assert!(world.set_player(player), "the identity is adopted once");
    }
    let mut out: Vec<(u32, u32)> = Vec::new();
    for e in events {
        hud.apply_events(std::slice::from_ref(e), &mut world);
        let Some(q) = world.player_qualities() else {
            continue;
        };
        let (Some(c), Some(m)) = (
            dereth_rules::attributes::inq_attribute_2nd(q, &table, cur, false, Some(&filter)),
            dereth_rules::attributes::inq_attribute_2nd(q, &table, max, false, Some(&filter)),
        ) else {
            continue;
        };
        if out.last() != Some(&(c, m)) {
            out.push((c, m));
        }
    }
    out
}

/// A `GameView` that answers exactly one question — what one vital is — with a pair of numbers a
/// recorded ACE server sent. It exists so that two renders can differ in **the level and nothing
/// else**, which a replay prefix cannot give: a prefix moves the chat log too.
#[derive(Debug)]
struct OneVital {
    which: dereth_ui_screens::view::Vital,
    value: (u32, u32),
}

impl dereth_ui_screens::view::GameView for OneVital {
    fn player(&self) -> Option<dereth_primitives::ObjectId> {
        Some(dereth_primitives::ObjectId(0x5000_0001))
    }
    fn vital(
        &self,
        _id: dereth_primitives::ObjectId,
        which: dereth_ui_screens::view::Vital,
    ) -> Option<(u32, u32)> {
        (which == self.which).then_some(self.value)
    }
}

/// The stacked vitals meter's own box and its fill child's, off the live tree.
fn vitals_meter_box(ui: &UiSystem, meter: ElementId) -> (dereth_ui::Box2D, dereth_ui::Box2D) {
    let root = root_of(ui).expect("the gameplay root");
    let vits = ui
        .get_child_recursive(root, window::STACKED_VITALS)
        .expect("<VITS>");
    let m = ui.get_child_recursive(vits, meter).expect("the meter");
    let fill = ui
        .get_child_recursive(m, ElementId(2))
        .expect("meter fill image child (element 2)");
    (ui.screen_box(m), ui.screen_box(fill))
}

/// The **first contiguous run of lit columns** inside a rectangle: `(first, last, length)`.
///
/// The trough graphics under a vital bar are near-black (the health trough's brightest channel is
/// 20/255, measured off `0x0600747E`); the fill graphics are saturated (208/255 for `0x06007481`).
/// So "is this column filled" is a threshold on the brightest channel and needs no reference image,
/// which is what makes the measurement absolute rather than a diff.
///
/// The run has to be contiguous rather than a count, because the *trough's* own right-hand end cap
/// carries a highlight (`0x06007486` reaches 67/255 at its inner edge and more on some rows) that a
/// short bar leaves uncovered — four lit columns at the far right of a bar that stops at 110.
fn lit_columns(bgra: &[u8], w: u32, b: dereth_ui::Box2D) -> (i32, i32, i32) {
    let lit = |x: i32| {
        (b.y0..=b.y1).any(|y| {
            let i = (y as u32 * w + x as u32) as usize * 4;
            bgra[i].max(bgra[i + 1]).max(bgra[i + 2]) > 100
        })
    };
    let Some(first) = (b.x0..=b.x1).find(|x| lit(*x)) else {
        return (-1, -1, 0);
    };
    let mut last = first;
    while last < b.x1 && lit(last + 1) {
        last += 1;
    }
    (first, last, last - first + 1)
}

/// Behaviour: hud.vitals.the-vitals-show-what-the-server-sent
///
/// **A vial at 16/33 is drawn about half full.**
///
/// Two rules shape the result:
///
/// 1. Element ordering breaks z-level ties by layout read order, so the fill child (read order 2)
///    paints **after** the empty trough (read order 1) rather than being buried under it;
/// 2. The meter clips child drawing to the level's filled region, so the fill does not appear
///    full at every level.
///
/// Oracle for the *values*: `fixtures/packet-captures/first-login-walk-jump` — the stamina the recorded ACE server
/// sent while the retail client was connected to it, which is a series and not one number because
/// the recorded player jumped. Oracle for the *geometry*: the meter's clipping rule, evaluated
/// against the rectangle `0x21000005` gives the fill child, read off the live tree.
///
/// The assertion is the **proportion**, at every level the capture recorded: a bar at 22/30 covers
/// 22/30 of its width using the client's truncating float-to-integer conversion, with the explicit
/// artwork allowances below (3 columns at the right edge, 8 for the extreme-level difference). A test that only
/// asked "is it non-empty" would pass on a full vial.
#[test]
fn the_vitals_vial_fills_in_proportion_to_the_recorded_value() {
    have_dats();
    let (events, _o) = replay("first-login-walk-jump");
    let series = recorded_series(
        events_in_world(&events),
        dereth_ui_screens::view::Vital::Stamina,
    );
    assert!(
        series.len() >= 2 && series.iter().any(|(c, m)| c < m),
        "first-login-walk-jump must carry a partly-drained vital for this test to mean anything: {series:?}"
    );

    // One shot of `app` at `value`: the level is injected and then drawn by one frame.
    // Offline, the application's own `HudView` has no player and `update_vitals` is inert, so
    // nothing puts the number back.
    let shot = |app: &mut dereth_client::app::App,
                value: (u32, u32)|
     -> Option<(Vec<u8>, u32, u32, dereth_ui::Box2D, dereth_ui::Box2D)> {
        let boxes = {
            let (ui, screen) = gameplay_screen(app)?;
            let view = OneVital {
                which: dereth_ui_screens::view::Vital::Stamina,
                value,
            };
            screen.update_vitals(ui, &view);
            vitals_meter_box(ui, ElementId(0x1000_00EC))
        };
        app.frame();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((bgra, w, h, boxes.0, boxes.1))
    };

    // One shot per recorded level, all from one application in gameplay. The proportion is read
    // inside the fill child's own box, so the frames the world advances between two shots do not
    // reach it.
    let mut app = app_in_gameplay(true, 4);
    let mut shots: Vec<VitalShot> = Vec::new();
    let mut meter_box = dereth_ui::Box2D::default();
    let mut fill_box = dereth_ui::Box2D::default();
    for value in &series {
        let (bgra, w, h, mb, fb) = shot(&mut app, *value).expect("a shot of the HUD");
        meter_box = mb;
        fill_box = fb;
        shots.push((*value, bgra, w, h));
    }
    let w_px = fill_box.width();
    assert!(
        w_px > 100,
        "the stamina fill is {w_px} px wide -- the layout is not what we think"
    );

    // 1. The proportion, at every level the capture recorded.
    //
    // Measured on the **right-hand edge** of the lit run, because the fill graphic is three pieces
    // (`0x100000E8`, `0x100000E9`, `0x100000EA`) and the rounded end caps have dark corners a
    // brightness test does not see: at a full bar seven of the hundred and fifty columns are cap, and the last three of
    // them fall under the threshold. That artwork is the whole reason for the +/- 3, and three
    // columns of a hundred and fifty still separate recorded levels that are fifteen columns
    // apart. The series is derived from the recording, not fixed.
    for ((cur, max), bgra, w, _h) in &shots {
        let (first, last, n) = lit_columns(bgra, *w, fill_box);
        #[allow(clippy::cast_precision_loss)]
        let level = *cur as f32 / *max as f32;
        let want = dereth_primitives::num::to_i32((w_px as f32) * level);
        assert!(
            first >= fill_box.x0,
            "{cur}/{max}: lit pixels outside the fill child's own box"
        );
        assert!(
            (last - fill_box.x0 + 1 - want).abs() <= 3,
            "{cur}/{max} is level {level}: the fill reaches column {} of {w_px}, {want} expected \
             ({n} lit)",
            last - fill_box.x0 + 1
        );
    }

    // 2. It is a proportion and not a threshold: the drained bar is shorter than the full one by
    //    the number of columns the two levels differ by.
    let full = shots
        .iter()
        .max_by_key(|((c, m), ..)| (c * 1000) / m)
        .expect("a shot");
    let low = shots
        .iter()
        .min_by_key(|((c, m), ..)| (c * 1000) / m)
        .expect("a shot");
    let (_, _, nf) = lit_columns(&full.1, full.2, fill_box);
    let (_, _, nl) = lit_columns(&low.1, low.2, fill_box);
    #[allow(clippy::cast_precision_loss)]
    let d = dereth_primitives::num::to_i32(
        (w_px as f32) * (full.0 .0 as f32 / full.0 .1 as f32 - low.0 .0 as f32 / low.0 .1 as f32),
    );
    assert!(
        nl < nf && (nf - nl - d).abs() <= 8,
        "{:?} vs {:?}: {nl} and {nf} columns lit, a difference of {d} expected",
        low.0,
        full.0
    );

    // 3. …and nothing else on the screen moved. The two extreme levels are each shot from a
    //    **fresh application** that has run the identical number of frames, so the scene behind
    //    the HUD is the same picture in both and the level is the one input that differs. Every
    //    pixel that differs between the two shots is inside the one meter the level belongs to,
    //    and there are none outside it.
    let (full_px, w, h, _, _) =
        shot(&mut app_in_gameplay(true, 4), full.0).expect("a shot of the HUD");
    let (low_px, ..) = shot(&mut app_in_gameplay(true, 4), low.0).expect("a shot of the HUD");
    let mut outside = 0usize;
    let mut inside = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize * 4;
            if full_px[i..i + 4] != low_px[i..i + 4] {
                if meter_box.contains(x as i32, y as i32) {
                    inside += 1;
                } else {
                    outside += 1;
                }
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixel(s) changed outside the stamina meter's own box"
    );
    assert!(inside > 0, "the level changed and no pixel moved");
}
