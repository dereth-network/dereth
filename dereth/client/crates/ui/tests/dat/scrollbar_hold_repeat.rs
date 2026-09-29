//! A held scrollbar arrow repeats at attribute `0x11`'s period, not once a frame, so one second of
//! held arrow scrolls the same rows at 30 and at 240 fps. The scrollbar's default hot-click setup
//! turns the hot-click bit `0x0F` on and, where the element does not already carry them, writes
//! `0x10 = 0.5` (the wait before the first repeat) and `0x11 = 0.125` (the repeat period). A press
//! broadcasts the hot click and stamps the next hot-click time at now plus `0x10`; the tick, with
//! the pointer on the button, fires when the clock has reached that time and advances it by `0x11`
//! from the **stamped** time, not the clock; with the pointer off the button the stamp is parked at
//! the clock. The repeat boundary is inclusive and the park boundary is not.
//!
//! In the shipped layouts 30 of the 46 hot-click elements, every one a scrollbar arrow, author
//! `0x11 = 0.1`; the 16 that fall back to 0.125 are the bars themselves, whose own hot click is the
//! page click on the empty track. A repeat rate is invisible in any single frame and to a test that
//! runs at one tick, so each count is derived from the attributes and asserted at two frame rates.
//! Fixture: a log and a vertical bar from a `LayoutDesc` written here; the census reads the dats.

// Every cast here turns a frame count or a repeat count -- both bounded by a few hundred in this
// file, and asserted exactly -- between `f64` and `usize`/`i32`. A truncation would show up as a
// wrong count in the very assertions these values feed.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use crate::common::NoAssets;
use dereth_primitives::{AssetSource, DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::props::attr;
use dereth_ui::widgets::scrollbar::attr as bar_attr;
use dereth_ui::{ElemHandle, ElementId, ElementType, ListenerId, NullInputPump, UiSystem};

// ---------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------

fn desc(id: u32, ty: ElementType, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x,
            y,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

const CONTAINER: u32 = 0x1000_0010;
const LOG: u32 = 0x1000_0011;
const BAR: u32 = 0x1000_0012;
/// Attribute `0x78`, the **decrement** arrow — authored at the top and placed by
/// the scrolling-area update at `(right, bottom)`, i.e. the bottom of a vertical bar.
const ARROW_DEC: u32 = 0x1000_0071;
/// Attribute `0x77`, the **increment** arrow — placed at `(0, 0)`.
const ARROW_INC: u32 = 0x1000_0072;
const THUMB: u32 = 1;

/// One line, as computes it from the default metrics.
/// The same 16 `scrollbar` uses.
const LINE: i32 = 16;

/// A 200x100 text log at the origin with a 16x100 vertical bar beside it at x = 200, the bar
/// carrying a thumb and the retail chat bar's own two arrow ids. The same shape as
/// `scrollbar`'s tree, because that is the one whose arrow placement is already pinned.
fn tree(ui: &mut UiSystem) -> (ElemHandle, ElemHandle, ElemHandle) {
    let mut container = desc(CONTAINER, ty::FIELD, 0, 0, 220, 100);
    let mut log = desc(LOG, ty::TEXT, 0, 0, 200, 100);
    log.base
        .properties
        .set(attr::TEXT_SELECTABLE, dereth_ui::PropertyValue::Bool(true));
    container.children.insert(ElementId(LOG), log);

    let mut bar = desc(BAR, ty::SCROLLBAR, 200, 0, 16, 100);
    bar.base.properties.set(
        bar_attr::DECREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_DEC),
    );
    bar.base.properties.set(
        bar_attr::INCREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_INC),
    );
    bar.base
        .properties
        .set(bar_attr::PROPORTIONAL, dereth_ui::PropertyValue::Bool(true));
    bar.children
        .insert(ElementId(THUMB), desc(THUMB, ty::FIELD, 0, 0, 16, 16));
    bar.children.insert(
        ElementId(ARROW_DEC),
        desc(ARROW_DEC, ty::BUTTON, 0, 0, 16, 16),
    );
    bar.children.insert(
        ElementId(ARROW_INC),
        desc(ARROW_INC, ty::BUTTON, 0, 84, 16, 16),
    );
    container.children.insert(ElementId(BAR), bar);

    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let log = ui.get_child(c, ElementId(LOG)).expect("the log");
    let bar = ui.get_child(c, ElementId(BAR)).expect("the bar");
    ui.on_set_attribute(
        log,
        dereth_ui::scrollable::attr::V_SCROLLBAR,
        Some(&dereth_ui::PropertyValue::Enum(BAR)),
    );
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);
    ui.initialize_tree(c);

    // 20 lines of 16 px in a 100 px viewport: 320 of content, so there is 220 px of travel and
    // nothing in this file can run into the clamp.
    let text: String = (0..20)
        .map(|i| format!("line {i}\n"))
        .collect::<Vec<_>>()
        .concat();
    {
        let t = ui.text_element_mut(log).expect("a text element");
        t.set_text(&text);
        t.bits.set_dirty(true);
    }
    let n = dereth_ui::scrollable::recalculate_dirty_text(ui, c);
    assert_eq!(n, 1, "the log re-measured itself");
    (c, log, bar)
}

fn offset(ui: &mut UiSystem, log: ElemHandle) -> i32 {
    ui.text_element_mut(log).expect("a text element").scroll.y
}

fn box_of(ui: &UiSystem, h: ElemHandle) -> dereth_ui::Box2D {
    ui.node(h).expect("alive").region.box_
}

/// The centre of an element in **window** coordinates, which is what `mouse_move` / `mouse_down`
/// take.
fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// What one held press produced.
#[derive(Debug, PartialEq, Eq)]
struct Held {
    /// Element message `0x02` broadcasts, the press's own included.
    clicks: usize,
    /// The log's scroll offset when the press ended.
    scrolled: i32,
    /// Frames of `use_time` the run spent.
    frames: usize,
}

/// Hold the **decrement** arrow — the one at the bottom of the bar, which walks the log forward —
/// from `t = 0` to `t = seconds`, stepping the frame clock at `fps`.
///
/// The press is delivered through
/// ([`UiSystem::mouse_down`]) after a real hit test, so the pointer-over-the-button flag is set the
/// way the client sets it and not by hand — the repeat arm reads it and a hand-set flag would be
/// asserting this file's own fixture.
fn hold(fps: f64, seconds: f64) -> Held {
    hold_with(fps, seconds, None)
}

/// As [`hold`], optionally overwriting attribute `0x11` on the arrow first — which is how the
/// **shipped** period (0.1, authored in the layout) is driven through a synthetic tree.
fn hold_with(fps: f64, seconds: f64, repeat: Option<f32>) -> Held {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);
    let mut pump = NullInputPump;
    ui.use_time(LocalTime(0.0), &mut pump);

    let dec = ui
        .get_child(bar, ElementId(ARROW_DEC))
        .expect("the decrement arrow");
    if let Some(r) = repeat {
        ui.set_attribute_float(dec, attr::HOT_CLICK_REPEAT_INTERVAL, r);
    }
    assert_eq!(
        (box_of(&ui, dec).x0, box_of(&ui, dec).y0),
        (0, 84),
        " puts the decrement arrow at (right, bottom)"
    );

    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(ARROW_DEC), msgid::BUTTON_HOT_CLICK, who);
    ui.drain_outbox();

    let (px, py) = centre(&ui, dec);
    ui.mouse_move(LocalTime(0.0), px, py);
    ui.mouse_down(action::PRIMARY_CLICK, px, py);

    let mut clicks = count_hot_clicks(&mut ui);
    assert_eq!(
        clicks, 1,
        "the press raises the first hot click itself, before any tick"
    );

    let frames = (seconds * fps).round() as usize;
    for i in 1..=frames {
        ui.use_time(LocalTime(i as f64 / fps), &mut pump);
        clicks += count_hot_clicks(&mut ui);
    }
    Held {
        clicks,
        scrolled: offset(&mut ui, log),
        frames,
    }
}

fn count_hot_clicks(ui: &mut UiSystem) -> usize {
    ui.drain_outbox()
        .into_iter()
        .filter(|d| {
            matches!(d, dereth_ui::Delivery::Element { msg, .. } if msg.id == msgid::BUTTON_HOT_CLICK)
        })
        .count()
}

// ---------------------------------------------------------------------------------------------
// 1. the two constants, pinned against the shipped tree and against retail
// ---------------------------------------------------------------------------------------------

/// **A held arrow in the shipped client repeats every 0.1 s, not every 0.125 s.**
///
/// **0.125 is the client's fallback, and no scrollbar arrow in the shipped data ever uses it.**
///
/// Censused over all **101** shipped layouts, built and initialised:
///
/// | population | `0x10` | `0x11` | n |
/// |---|---|---|---:|
/// | scrollbar **arrows** (`0x71`, `0x72`, `0x36B`, `0x36C`) | 0.5 | **0.1**, authored in the layout | 30 |
/// | scrollbar **bars** themselves | 0.5 | 0.125, `setup_default_hot_click`'s default | 16 |
/// | | | | **46** |
///
/// So the shipped behaviour is **two different periods for two different gestures**: an arrow
/// held down steps a row every 0.1 s, and a click held on the empty *track* pages every 0.125 s,
/// because the scrollbar calls `setup_default_hot_click` on the bar itself and no layout
/// authors an interval there. Twelve element descriptors carry the authored pair, in three
/// layouts (`0x21000008`, `0x2100003E`, `0x2100004C`); the other 18 built arrows inherit it
/// through `inq_full_desc`'s partial merge.
///
/// The literals are pinned in both directions, which are different claims:
///
/// * every period the rest of this file uses is **read off the built tree**, so nothing types one;
/// * and the values that come back are asserted against the literals — `0.1` from the shipped
///   layout, and `0.125` whose provenance is the literal the hot-click setup writes when the
///   layout names no period (`0.5` for the first wait, `0.125` for the repeat).
///
/// The second half supplies an independent literal: a test that reads a constant through
/// the same symbol it writes it through cannot detect a wrong constant.
///
/// **The census states its denominator** — 101 layouts built, 46 hot-click elements found, and
/// the two buckets required to sum to 46 — so "no shipped bar disagrees" cannot be the answer of
/// a census that looked at nothing. And the split is asserted **by role** rather than by value:
/// every 0.1 is one of the four arrow ids and no bar is, so it is a mechanism and not a
/// coincidence of this data.
#[test]
fn shipped_arrows_repeat_at_a_tenth_of_a_second_and_only_the_bars_fall_back_to_an_eighth() {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        dir.display()
    );
    let store = dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dat store opens");

    // `MasterProperty 0x39000001` first: the layout stream is undecodable without the property
    // type table.
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property table");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");
    let types = master.property_types();

    let mut layouts = Vec::new();
    for did in 0x2100_0000..=0x2100_0075u32 {
        let did = DataId(did);
        if !store.exists(did) {
            continue;
        }
        let b = store.read(did).unwrap_or_else(|e| panic!("{did}: {e}"));
        layouts.push(LayoutDesc::read(did, &b, &types).unwrap_or_else(|e| panic!("{did}: {e}")));
    }
    assert_eq!(
        layouts.len(),
        101,
        "the shipped layout count from 36-ui-layouts.md"
    );

    // **Which layouts author the intervals.** **12 elements across three layouts author both**, at
    // `0x10 = 0.5` and `0x11 = 0.1`, and every one of them is a scrollbar **arrow**.
    // `setup_default_hot_click`'s 0.125 is therefore not what a held arrow repeats at, anywhere in
    // the shipped client — it is only what the *bar itself* pages at.
    //
    // The rest of the authored set is inheritance: `inq_full_desc` merges partial descs, so the
    // 12 authored descs reach 30 built elements.
    let mut authored: Vec<(DataId, ElementId, u32, f32)> = Vec::new();
    for l in &layouts {
        let mut stack: Vec<&ElementDesc> = l.elements.values().collect();
        while let Some(d) = stack.pop() {
            for id in [
                attr::HOT_CLICK_FIRST_INTERVAL,
                attr::HOT_CLICK_REPEAT_INTERVAL,
            ] {
                if let Some(v) = d.base.properties.get_float(id) {
                    authored.push((l.did, d.element_id, id, v));
                }
            }
            stack.extend(d.children.values());
        }
    }
    assert_eq!(
        authored.len(),
        24,
        "12 elements x 2 attributes, authored in the shipped layouts"
    );
    for (did, id, prop, v) in &authored {
        let want = if *prop == attr::HOT_CLICK_FIRST_INTERVAL {
            0.5
        } else {
            0.1
        };
        assert_eq!(*v, want, "{did}/{id:?} attribute {prop:#04X}");
    }
    let layouts_authoring: std::collections::BTreeSet<u32> =
        authored.iter().map(|(d, _, _, _)| d.0).collect();
    assert_eq!(
        layouts_authoring.into_iter().collect::<Vec<_>>(),
        vec![0x2100_0008, 0x2100_003E, 0x2100_004C],
        "the layouts that author an interval at all"
    );

    // Build every layout and initialise it, which is where `setup_default_hot_click` runs.
    let mut hot = 0usize;
    let mut built = 0usize;
    let mut census: Vec<(DataId, ElementId, Option<f32>, Option<f32>)> = Vec::new();
    for l in &layouts {
        let mut ui = UiSystem::new((800, 600));
        ui.property_types.clone_from(&types);
        for other in &layouts {
            ui.lib.insert(other.clone());
        }
        for root in l.elements.values() {
            if let Ok(Some(h)) = ui.create_element(&store, l, root) {
                let r = ui.root();
                ui.set_parent(h, Some(r));
            }
        }
        built += 1;
        for root in ui.children(ui.root()) {
            ui.initialize_tree(root);
        }
        for h in ui.element_list().to_vec() {
            let Some(n) = ui.node(h) else { continue };
            let p = n.merged_properties();
            if p.get_bool(attr::HOT_CLICK) != Some(true) {
                continue;
            }
            hot += 1;
            census.push((
                l.did,
                n.element_id(),
                p.get_float(attr::HOT_CLICK_FIRST_INTERVAL),
                p.get_float(attr::HOT_CLICK_REPEAT_INTERVAL),
            ));
        }
    }
    assert_eq!(built, 101, "every layout was built");
    assert!(
        hot >= 20,
        "the shipped tree has hot-click elements to census; found {hot}"
    );
    assert_eq!(hot, 46, "the shipped tree's whole hot-click population");

    // Every one of the 46 gets the same first interval, 0.5 (`0x3f000000`),
    // either way round: no shipped layout authors anything else for 0x10.
    for (did, id, first, _) in &census {
        assert_eq!(*first, Some(0.5), "{did}/{id:?}: attribute 0x10");
    }

    // The repeat period splits in two. The arrows
    // carry `0.1`, authored in the layout; only the bars fall back to `setup_default_hot_click`'s
    // `0.125`, and a bar's own hot click is the *page* click on the empty track, not an arrow.
    let (arrows, bars): (Vec<_>, Vec<_>) = census.iter().partition(|(_, _, _, r)| *r == Some(0.1));
    assert_eq!(arrows.len(), 30, "elements whose layout authors 0x11 = 0.1");
    assert_eq!(bars.len(), 16, "elements that fall back to 0x11 = 0.125");
    assert_eq!(
        arrows.len() + bars.len(),
        hot,
        "the two buckets are the whole population"
    );
    for (did, id, _, r) in &bars {
        assert_eq!(
            *r,
            Some(0.125),
            "{did}/{id:?}: the fallback period the setup writes"
        );
    }

    // And the split is by *role*, which is what makes it a mechanism rather than a coincidence
    // of the data: every 0.1 is on one of the four arrow ids a bar names through its
    // attributes 0x77/0x78, and no bar is.
    const ARROW_IDS: [u32; 4] = [0x1000_0071, 0x1000_0072, 0x1000_036B, 0x1000_036C];
    for (did, id, _, _) in &arrows {
        assert!(
            ARROW_IDS.contains(&id.0),
            "{did}/{id:?} carries 0.1 but is not an arrow"
        );
    }
    for (did, id, _, _) in &bars {
        assert!(
            !ARROW_IDS.contains(&id.0),
            "{did}/{id:?} is an arrow but fell back to 0.125"
        );
    }

    // The two named cases the rest of this file talks about, so a later reader can find them.
    let has = |did: u32, id: u32| census.iter().any(|(d, e, _, _)| d.0 == did && e.0 == id);
    assert!(
        has(0x2100_003E, 0x1000_0071),
        "the chat bar's decrement arrow"
    );
    assert!(
        has(0x2100_0045, 0x1000_023E),
        "the skills list's own bar (0.125, the page click)"
    );
}

/// The same two literals on the tree this file drives, so the cadence tests below are not resting
/// on a fixture the dats might not be present for.
#[test]
fn the_synthetic_bar_gets_the_same_two_intervals_from_setup_default_hot_click() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, _log, bar) = tree(&mut ui);
    for h in [
        bar,
        ui.get_child(bar, ElementId(ARROW_DEC)).expect("dec"),
        ui.get_child(bar, ElementId(ARROW_INC)).expect("inc"),
    ] {
        let p = ui.node(h).expect("alive").merged_properties();
        assert_eq!(p.get_bool(attr::HOT_CLICK), Some(true));
        assert_eq!(
            p.get_float(attr::HOT_CLICK_FIRST_INTERVAL),
            Some(0.5),
            "0x10"
        );
        assert_eq!(
            p.get_float(attr::HOT_CLICK_REPEAT_INTERVAL),
            Some(0.125),
            "0x11"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. the same elapsed time, two frame rates, one answer
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.scrollbar.a-held-arrow-repeats-at-its-authored-period-not-per-frame
///
/// **One second of held arrow scrolls the same number of rows at 30 fps and at 240 fps.**
///
/// The two rates are the whole point: one hot click per frame would answer **31** and **241**,
/// differing by a factor of eight for no reason but the frame clock.
///
/// The expected count is **derived** from the two attributes as they are read off the element,
/// not typed:
///
/// ```text
/// clicks(T) = 1                                  the press's own, at t = 0
///           + floor((T - first) / repeat) + 1    every repeat that fits in the span
/// ```
///
/// With `first = 0.5` and `repeat = 0.125` over `T = 1.0` that is `1 + 4 + 1 = 6`. It is
/// independent of the frame rate for any rate above `1 / repeat = 8` fps, **because the advance
/// accumulates from the previous scheduled time** (the period is added to the stamp, not to the
/// clock) — a schedule that re-stamped from `cur_time` would drift with the frame quantisation
/// and the two answers would differ by a row or two rather than agreeing exactly. That is the
/// second thing this test discriminates and it is why the assertion is `assert_eq!` on the count
/// rather than a tolerance.
///
/// **Falsified by** raising the hot click unconditionally on the tick (30 vs 240), by reading
/// `0x10` where `0x11` belongs (2 vs 2 — equal, but 2 rather than 6, which the derived expectation
/// catches), or by re-stamping `next = now + repeat` instead of accumulating (6 vs 5).
#[test]
fn a_held_arrow_scrolls_the_same_rows_per_second_at_thirty_and_at_two_hundred_and_forty_fps() {
    // The periods, read off the element rather than typed. `the_synthetic_bar_...` above is what
    // pins them to the literals; this is the derivation.
    let (first, repeat) = {
        let mut ui = UiSystem::new((800, 600));
        let (_c, _log, bar) = tree(&mut ui);
        let dec = ui.get_child(bar, ElementId(ARROW_DEC)).expect("dec");
        let p = ui.node(dec).expect("alive").merged_properties();
        (
            f64::from(p.get_float(attr::HOT_CLICK_FIRST_INTERVAL).expect("0x10")),
            f64::from(p.get_float(attr::HOT_CLICK_REPEAT_INTERVAL).expect("0x11")),
        )
    };
    let seconds = 1.0_f64;
    let want = 1 + ((seconds - first) / repeat).floor() as usize + 1;
    assert_eq!(want, 6, "1 + floor((1.0 - 0.5) / 0.125) + 1");

    let slow = hold(30.0, seconds);
    let fast = hold(240.0, seconds);

    assert_eq!(slow.frames, 30);
    assert_eq!(fast.frames, 240);
    assert_eq!(
        slow.clicks, want,
        "30 fps: the period is elapsed time, not frames ({slow:?})"
    );
    assert_eq!(
        fast.clicks, want,
        "240 fps: the same second, the same rows ({fast:?})"
    );
    assert_eq!(
        slow.clicks, fast.clicks,
        "an eightfold change in frame rate must change nothing: {slow:?} vs {fast:?}"
    );

    // And the rows themselves, which is what the player sees: one `inq_scroll_delta` line per hot
    // click, in the same direction at both rates.
    assert_eq!(
        slow.scrolled,
        want as i32 * LINE,
        "30 fps: one line per hot click"
    );
    assert_eq!(fast.scrolled, slow.scrolled, "240 fps: the same distance");
}

/// **The period is read, not baked: the shipped 0.1 gives a different count from the fallback
/// 0.125, and both are frame-rate independent.**
///
/// The test above could pass on a build that hard-coded `0.125` anywhere in the chain, because
/// `0.125` is what the synthetic tree's `setup_default_hot_click` happens to write. This one
/// changes only attribute `0x11` — to the value the shipped layouts actually author — and requires
/// the answer to move with it. Without this pair, "the count does not change with the frame rate"
/// is equally true of a constant.
///
/// 0.95 s deliberately, not 1.0: at 0.125 the repeat at `t = 1.0` lands exactly on a frame and is
/// the boundary station `a_repeat_due_at_exactly_the_current_time_fires` owns. Here the span ends
/// between repeats, so neither period is being asked a boundary question.
#[test]
fn changing_only_the_repeat_attribute_changes_the_count_at_both_frame_rates() {
    let seconds = 0.95;
    // The fallback, 0.125: repeats at 0.500 0.625 0.750 0.875, then 1.000 is past the end.
    let base_slow = hold_with(30.0, seconds, None);
    let base_fast = hold_with(240.0, seconds, None);
    assert_eq!(
        base_slow.clicks, 5,
        "1 + four repeats at 0x11 = 0.125 ({base_slow:?})"
    );
    assert_eq!(base_fast.clicks, base_slow.clicks);

    // The shipped arrows' own 0.1: repeats at 0.5 0.6 0.7 0.8 0.9 — one more in the same span.
    let ship_slow = hold_with(30.0, seconds, Some(0.1));
    let ship_fast = hold_with(240.0, seconds, Some(0.1));
    assert_eq!(
        ship_slow.clicks, 6,
        "1 + five repeats at the shipped 0x11 = 0.1 ({ship_slow:?})"
    );
    assert_eq!(
        ship_fast.clicks, ship_slow.clicks,
        "still frame-rate independent"
    );
    assert_ne!(
        ship_slow.clicks, base_slow.clicks,
        "the count follows the attribute; if these are equal the period is not being read"
    );

    // The other end of the range, and it says something the two rows above cannot: with `0x11`
    // set to 10 s the press still fires at `t = 0` and the **first** repeat still arrives at
    // `t = 0.5`, because that one is `0x10`'s and `0x11` only governs the ones after it. Two
    // clicks, not one and not none — which is the shape that separates "the two attributes are
    // read at the two places the client reads them" from "one number drives everything".
    let long = hold_with(240.0, seconds, Some(10.0));
    assert_eq!(
        long.clicks, 2,
        "the press, then 0x10's single repeat at 0.5; 0x11's next is at 10.5 ({long:?})"
    );
}

/// The **calibration positive** for the test above: the same
/// instrument, run over a span that contains no repeat at all, must answer 1 and not 0.
///
/// Without this, "the two rates agree" is satisfied just as well by an arrow that never repeats —
/// a dead hot click agrees with itself at every frame rate. A == B is satisfied by
/// nothing at all as readily as by
/// correctness, so the premise is asserted and not only the difference.
#[test]
fn a_press_shorter_than_the_first_interval_repeats_exactly_once_at_both_rates() {
    // 0.4 s, which is inside `0x10`'s 0.5: the press's own click and nothing after it.
    let slow = hold(30.0, 0.4);
    let fast = hold(240.0, 0.4);
    assert_eq!(slow.clicks, 1, "the first repeat is not due yet ({slow:?})");
    assert_eq!(
        fast.clicks, 1,
        "and 96 more frames do not make it due ({fast:?})"
    );
    assert_eq!(slow.scrolled, LINE, "one line moved, so the arrow is alive");
    assert_eq!(fast.scrolled, LINE);

    // The other end of the same calibration: a span long enough for many repeats still agrees,
    // and the count grows with the span rather than with the frames.
    let long_slow = hold(30.0, 2.0);
    let long_fast = hold(240.0, 2.0);
    assert_eq!(long_slow.clicks, 14, "1 + floor((2.0 - 0.5) / 0.125) + 1");
    assert_eq!(long_fast.clicks, long_slow.clicks);
    assert!(
        long_slow.clicks > slow.clicks,
        "a five-times-longer hold scrolls further: {long_slow:?} vs {slow:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. the two boundaries, and the arm that is easy to leave out
// ---------------------------------------------------------------------------------------------

/// **The repeat's flag test masks "below" alone, so a repeat that falls exactly on its due time
/// fires.**
///
/// Exercise the exact boundary decided by the flag test: 8 fps steps the clock in units of
/// `0.125`, which is `0x11` exactly, and
/// every frame from `t = 0.5` on lands precisely on a scheduled repeat. An exclusive comparison
/// would fire on none of them and answer 1.
#[test]
fn a_repeat_due_at_exactly_the_current_time_fires() {
    // 1/8 s per frame == the repeat period, so t = 0.500, 0.625, 0.750, ... are exact.
    let r = hold(8.0, 1.0);
    assert_eq!(r.frames, 8);
    assert_eq!(
        r.clicks, 6,
        "frames land on 0.500/0.625/0.750/0.875/1.000 exactly and all five fire ({r:?})"
    );
}

/// **The park arm: while the pointer is held down but off the arrow, the next hot-click time is
/// dragged forward with the clock, so re-entering does not fire a burst.**
///
/// The park arm is the one an implementation leaves out, and leaving it out is not cosmetic:
/// with the schedule left behind, coming back onto the arrow after two seconds fires **sixteen**
/// repeats on one frame — `next += 0.125` catches up one step per tick, so it takes sixteen ticks,
/// but every one of them scrolls a row and the list jumps.
///
/// Driven the way the client does it: the pointer moves off the arrow (`switch_mouse_over`, so
/// the pointer-over-the-button flag goes false) while the press is still down, time passes, and it
/// comes back.
#[test]
fn the_clock_is_parked_while_the_pointer_is_off_the_held_arrow() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);
    let mut pump = NullInputPump;
    ui.use_time(LocalTime(0.0), &mut pump);
    let dec = ui.get_child(bar, ElementId(ARROW_DEC)).expect("dec");
    let who = ListenerId::External(1);
    ui.register_for_element_message(ElementId(ARROW_DEC), msgid::BUTTON_HOT_CLICK, who);
    ui.drain_outbox();

    let (px, py) = centre(&ui, dec);
    ui.mouse_move(LocalTime(0.0), px, py);
    ui.mouse_down(action::PRIMARY_CLICK, px, py);
    assert_eq!(count_hot_clicks(&mut ui), 1, "the press's own");

    // Off the arrow at t = 0.1, well before the first repeat is due at 0.5.
    ui.mouse_move(LocalTime(0.1), 400, 400);
    let mut away = 0usize;
    let mut t = 0.1_f64;
    while t < 2.0 {
        t += 1.0 / 60.0;
        ui.use_time(LocalTime(t), &mut pump);
        away += count_hot_clicks(&mut ui);
    }
    assert_eq!(
        away, 0,
        "nothing repeats while the pointer is off the arrow"
    );
    let parked = offset(&mut ui, log);
    assert_eq!(
        parked, LINE,
        "and nothing scrolled beyond the press's own line"
    );

    // Back onto it. The clock was parked at `now` on every one of those frames, so the schedule
    // is due immediately -- one repeat -- and then resumes at 0x11's period.
    ui.mouse_move(LocalTime(t), px, py);
    ui.use_time(LocalTime(t + 1.0 / 60.0), &mut pump);
    let first_back = count_hot_clicks(&mut ui);
    assert_eq!(
        first_back, 1,
        "re-entering is due immediately and fires once"
    );
    assert_eq!(offset(&mut ui, log), parked + LINE);

    // **And this is the half that a missing park arm is actually visible in.** The repeat fires
    // at most once per tick either way, so the first frame back cannot tell the two apart. What
    // separates them is the next fifth of a second: a parked schedule is now at `now + 0.125` and
    // fires **once** more in it, while a schedule still sitting at 0.5 owes about twelve steps
    // and catches up at one per frame -- twelve rows where retail moves one.
    let resume_from = t + 1.0 / 60.0;
    let mut resumed = 0usize;
    let mut u = resume_from;
    while u < resume_from + 0.2 {
        u += 1.0 / 60.0;
        ui.use_time(LocalTime(u), &mut pump);
        resumed += count_hot_clicks(&mut ui);
    }
    assert_eq!(
        resumed, 1,
        "0.2 s at 0x11 = 0.125 is one further repeat; an unparked schedule fires on every frame"
    );
}
