//! Border-drag resizing: a window's resize handle drags the window it belongs to, in one of eight
//! directions chosen by the handle's four edge booleans; the drag is clamped by the window's four
//! min/max attributes (re-derived from the drag origin on every event, so excess is not lost) and
//! by its parent frame; shift snaps a resize to a ten-pixel grid, and a move on x always and on y
//! only off the bottom edge. The right button does not resize, and a handle that names no edge is
//! not a handle. The shipped layouts' resize and drag handles are counted.
//!
//! A resize that clamps to the wrong attribute, loses the excess of a drag or snaps the wrong
//! coordinates *looks* like one that does not, so every number is asserted. Fixture: a window with
//! a corner handle inside a frame, from a `LayoutDesc` written here; the census reads the retail
//! dats.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::widgets::resizebar;
use dereth_ui::{
    BorderLocation, ElemHandle, ElementId, ElementType, InputPump, SizeClamps, UiSystem,
};

// ---------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------

/// `InputPump::shift_key_down` with an answer a test can choose. The default `InputPump` says
/// `false`; this is the calibration positive for it.
#[derive(Debug, Default)]
struct Pump {
    shift: bool,
}
impl InputPump for Pump {
    fn use_time(&mut self, _now: LocalTime) {}
    fn shift_key_down(&self) -> bool {
        self.shift
    }
}

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

/// A window 200x150 at (100,100) inside a 400x300 frame, with one 8x8 resize handle in its
/// bottom-right corner. Returns `(frame, window, handle)`.
///
/// The nesting is the client's: a `Resizebar` acts on its **parent**, and the parent is clamped
/// against **its** parent, so three levels are the minimum that exercises the whole function.
fn tree(ui: &mut UiSystem) -> (ElemHandle, ElemHandle, ElemHandle) {
    let mut frame = desc(1, ty::FIELD, 0, 0, 400, 300);
    let mut window = desc(2, ty::FIELD, 100, 100, 200, 150);
    window
        .children
        .insert(ElementId(3), desc(3, ty::RESIZEBAR, 192, 142, 8, 8));
    frame.children.insert(ElementId(2), window);
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(1), frame)).collect(),
    };
    let d = l.access_element(ElementId(1)).cloned().expect("root");
    let f = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(f, Some(root));
    // The element's initialisation is what gives the handle its mouse visibility, and
    // without it the hit test walks straight past a resize bar. Five element types are always
    // mouse-visible and `Resizebar` is one of them.
    ui.initialize_tree(f);
    let w = ui.get_child(f, ElementId(2)).expect("the window");
    let h = ui.get_child(w, ElementId(3)).expect("the handle");
    (f, w, h)
}

fn box_of(ui: &UiSystem, h: ElemHandle) -> (i32, i32, i32, i32) {
    let b = ui.node(h).expect("alive").region.box_;
    (b.x0, b.y0, b.width(), b.height())
}

// ---------------------------------------------------------------------------------------------
// the constants, pinned as literals
// ---------------------------------------------------------------------------------------------

/// **An independent literal pin.** Every other test in this file reaches these
/// numbers through their symbols, so changing a symbol's value would change both sides of those
/// assertions at once and none of them could see it. This test states the numbers.
///
/// The eight ids and what fixes each:
///
/// | id | meaning | where it is read |
/// |---|---|---|
/// | `0x2A` | resizebar drags the bottom | |
/// | `0x2B` | ... the left | same |
/// | `0x2C` | ... the right | same |
/// | `0x2D` | ... the top | same |
/// | `0x3C` | **max** height | both clamps |
/// | `0x3D` | **max** width | same |
/// | `0x3E` | **min** height | same |
/// | `0x3F` | **min** width | same |
///
/// An earlier description listed the bottom four in the opposite order, as did
/// `dereth_ui_screens::bind::attr`, which
/// transcribed that list; see `dereth_ui::props::attr::MIN_WIDTH`.
#[test]
fn the_eight_attribute_ids_are_the_numbers_the_client_reads() {
    assert_eq!(resizebar::attr::BOTTOM, 0x2A);
    assert_eq!(resizebar::attr::LEFT, 0x2B);
    assert_eq!(resizebar::attr::RIGHT, 0x2C);
    assert_eq!(resizebar::attr::TOP, 0x2D);

    assert_eq!(dereth_ui::props::attr::MAX_HEIGHT, 0x3C);
    assert_eq!(dereth_ui::props::attr::MAX_WIDTH, 0x3D);
    assert_eq!(dereth_ui::props::attr::MIN_HEIGHT, 0x3E);
    assert_eq!(dereth_ui::props::attr::MIN_WIDTH, 0x3F);

    // And the direction each clamps in, which is what actually names them: `0x3D` may only make a
    // window narrower and `0x3F` may only make it wider.
    let wide = SizeClamps {
        min_w: None,
        max_w: Some(120),
        min_h: None,
        max_h: None,
    };
    let narrow = SizeClamps {
        min_w: Some(280),
        max_w: None,
        min_h: None,
        max_h: None,
    };
    let start = (0, 0, 200, 150);
    assert_eq!(
        dereth_ui::layout::mouse_resize(start, BorderLocation::Right, 500, 0, wide, false).2,
        120,
        "0x3D is a maximum"
    );
    assert_eq!(
        dereth_ui::layout::mouse_resize(start, BorderLocation::Right, -500, 0, narrow, false).2,
        280,
        "0x3F is a minimum"
    );
}

/// Oracle: the resize bar's mouse-resize start, its nested `if`s, including the two
/// arithmetic forms `BORDER_LEFT - (bottom != 0)` and `BORDER_RIGHT + (bottom != 0)`.
#[test]
fn the_four_edge_booleans_fold_into_the_nine_border_zones() {
    use resizebar::border_from_edges as f;
    // (left, top, right, bottom)
    assert_eq!(
        f(false, false, false, false),
        BorderLocation::None,
        "no edge is not a handle"
    );
    assert_eq!(f(false, false, false, true), BorderLocation::Bottom);
    assert_eq!(f(false, true, false, false), BorderLocation::Top);
    assert_eq!(f(true, false, false, false), BorderLocation::Left);
    assert_eq!(f(false, false, true, false), BorderLocation::Right);
    assert_eq!(f(true, true, false, false), BorderLocation::UpperLeft);
    assert_eq!(f(true, false, false, true), BorderLocation::LowerLeft);
    assert_eq!(f(false, true, true, false), BorderLocation::UpperRight);
    assert_eq!(f(false, false, true, true), BorderLocation::LowerRight);
    // The client's tree tests `right` first and `top` before `bottom`, so a corner bar that names
    // three edges resolves to the two the tree reaches, not to an error.
    assert_eq!(f(true, true, true, true), BorderLocation::UpperRight);
}

// ---------------------------------------------------------------------------------------------
// the cluster, driven end to end
// ---------------------------------------------------------------------------------------------

/// Oracle: the whole chain — `0x1C` -> the bar's mouse-resize start -> `start_resizing`, `0x1E` ->
/// `mouse_resize_element`, `0x1D` -> `stop_resizing`.
///
/// Every piece must be reachable from the one before it: a chain whose parts each work alone
/// still fails here.
#[test]
fn dragging_the_corner_handle_resizes_the_window_it_belongs_to() {
    let mut ui = UiSystem::new((800, 600));
    let (_frame, window, handle) = tree(&mut ui);
    ui.set_attribute_bool(handle, resizebar::attr::RIGHT, true);
    ui.set_attribute_bool(handle, resizebar::attr::BOTTOM, true);

    assert_eq!(box_of(&ui, window), (100, 100, 200, 150), "before");

    // The press lands inside the 8x8 handle, whose screen box is (292,242)..(299,249).
    ui.mouse_down(action::PRIMARY_CLICK, 295, 245);
    assert!(
        ui.node(window).expect("alive").flags.is_resizing(),
        "the press armed the parent, not the handle"
    );
    assert_eq!(
        ui.node(window).expect("alive").current_border,
        BorderLocation::LowerRight,
        "and it read the zone off the handle's own two booleans"
    );

    // +30, +20 — well outside the handle, which is the point: the capture keeps 0x1E coming.
    ui.mouse_move(LocalTime(1.0), 325, 265);
    assert_eq!(
        box_of(&ui, window),
        (100, 100, 230, 170),
        "the far edges followed the pointer"
    );

    ui.mouse_up(action::PRIMARY_CLICK, 325, 265, false);
    assert!(
        !ui.node(window).expect("alive").flags.is_resizing(),
        "the release disarmed it"
    );
    ui.mouse_move(LocalTime(2.0), 400, 400);
    assert_eq!(
        box_of(&ui, window),
        (100, 100, 230, 170),
        "and motion after the release is inert"
    );
}

/// Oracle: that same start's bare `return;` when none of the four booleans is set.
#[test]
fn a_handle_that_declares_no_edge_is_not_a_handle() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, _h) = tree(&mut ui);
    ui.mouse_down(action::PRIMARY_CLICK, 295, 245);
    assert!(!ui.node(window).expect("alive").flags.is_resizing());
    ui.mouse_move(LocalTime(1.0), 325, 265);
    assert_eq!(box_of(&ui, window), (100, 100, 200, 150), "nothing moved");
}

/// Oracle: the resize bar's message listener, which tests `p1 == 7`.
#[test]
fn the_right_button_does_not_resize_windows() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, handle) = tree(&mut ui);
    ui.set_attribute_bool(handle, resizebar::attr::RIGHT, true);
    ui.mouse_down(action::SECONDARY_CLICK, 295, 245);
    assert!(!ui.node(window).expect("alive").flags.is_resizing());
}

/// Behaviour: ui.window.a-border-drag-resizes-within-its-clamps-and-shift-snaps
///
/// Oracle: `mouse_resize_element`'s four integer-attribute clamps, read off the **parent** (the
/// element being resized), not off the handle.
///
/// The second half is the property an incremental implementation cannot have: the arms are
/// re-derived from the recorded drag-start x/y/width/height on every event, so a drag that buries
/// itself in a clamp and comes back out lands where the pointer is, not where the clamp left it.
#[test]
fn the_four_clamps_bound_the_drag_and_the_excess_is_not_lost() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, handle) = tree(&mut ui);
    ui.set_attribute_bool(handle, resizebar::attr::RIGHT, true);
    ui.set_attribute_bool(handle, resizebar::attr::BOTTOM, true);
    ui.set_attribute_int(window, dereth_ui::props::attr::MIN_WIDTH, 150);
    ui.set_attribute_int(window, dereth_ui::props::attr::MAX_WIDTH, 250);
    ui.set_attribute_int(window, dereth_ui::props::attr::MIN_HEIGHT, 100);
    ui.set_attribute_int(window, dereth_ui::props::attr::MAX_HEIGHT, 180);

    ui.mouse_down(action::PRIMARY_CLICK, 295, 245);
    ui.mouse_move(LocalTime(1.0), 1000, 1000);
    assert_eq!(box_of(&ui, window), (100, 100, 250, 180), "the two maxima");
    ui.mouse_move(LocalTime(2.0), 0, 0);
    assert_eq!(box_of(&ui, window), (100, 100, 150, 100), "the two minima");
    // Back to +30/+20 from the origin: if the arms were incremental the 705 px and 745 px eaten by
    // the clamps above would still be owed and this would not move at all.
    ui.mouse_move(LocalTime(3.0), 325, 265);
    assert_eq!(
        box_of(&ui, window),
        (100, 100, 230, 170),
        "the drag origin is intact"
    );
}

/// **The two clamp orders disagree, and only a literal can see it.**
///
/// The element's `resize_to` reads `0x3C` (max h) then `0x3E` (min h), then `0x3D` (max w)
/// then `0x3F` (min w) — **maximum first**, so a layout declaring a minimum larger than its
/// maximum ends up at the **minimum**. The mouse-driven resize reads them the
/// other way round in every arm — `0x3F` then `0x3D`, `0x3E` then `0x3C` — so the same
/// contradiction resolves to the **maximum**.
///
/// Neither order is reachable from the shipped layouts (none declares a contradictory pair), which
/// is exactly why this is written down: only a literal sees either order, and the two being
/// different is the kind of detail that gets "tidied" into agreement.
#[test]
fn resize_to_and_a_border_drag_resolve_a_contradictory_pair_the_opposite_way() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, _h) = tree(&mut ui);
    ui.set_attribute_int(window, dereth_ui::props::attr::MIN_WIDTH, 200);
    ui.set_attribute_int(window, dereth_ui::props::attr::MAX_WIDTH, 120);

    ui.resize_to(window, 300, 150);
    assert_eq!(
        box_of(&ui, window).2,
        200,
        "resize_to applies max then min, so the minimum wins"
    );

    let c = SizeClamps {
        min_w: Some(200),
        max_w: Some(120),
        min_h: None,
        max_h: None,
    };
    assert_eq!(
        dereth_ui::layout::mouse_resize(
            (100, 100, 200, 150),
            BorderLocation::Right,
            500,
            0,
            c,
            false
        )
        .2,
        120,
        "mouse_resize_element applies min then max, so the maximum wins"
    );
}

/// Oracle: the two parent-width and parent-height clamps at the tail of
/// `mouse_resize_element`. The 400x300 frame is what stops the window growing off it.
#[test]
fn a_window_cannot_be_dragged_larger_than_the_frame_that_holds_it() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, handle) = tree(&mut ui);
    ui.set_attribute_bool(handle, resizebar::attr::RIGHT, true);
    ui.set_attribute_bool(handle, resizebar::attr::BOTTOM, true);
    ui.mouse_down(action::PRIMARY_CLICK, 295, 245);
    ui.mouse_move(LocalTime(1.0), 4000, 4000);
    // right clamps to 400 and bottom to 300, both measured from the window's own origin (100,100).
    assert_eq!(box_of(&ui, window), (100, 100, 300, 200));
}

/// Behaviour: ui.window.a-border-drag-resizes-within-its-clamps-and-shift-snaps
///
/// Oracle: while shift is down, each snapped coordinate drops its remainder mod 10
/// (`x -= x % 10`), at **both** snapping sites: the resize drag snaps all four coordinates, and
/// the move drag snaps x always and y only off the bottom edge.
///
/// The pump is the calibration: the same drag with `shift = false` must land somewhere the snap
/// would have moved, or the test cannot tell a working snap from a dead one.
#[test]
fn shift_snaps_a_resize_to_the_ten_pixel_grid() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, handle) = tree(&mut ui);
    ui.set_attribute_bool(handle, resizebar::attr::RIGHT, true);
    ui.set_attribute_bool(handle, resizebar::attr::BOTTOM, true);

    // Calibration: without shift, a +3/+7 drag lands on 203x157 — neither a multiple of ten.
    let mut off = Pump { shift: false };
    ui.use_time(LocalTime(1.0), &mut off);
    ui.mouse_down(action::PRIMARY_CLICK, 295, 245);
    ui.mouse_move(LocalTime(1.0), 298, 252);
    assert_eq!(box_of(&ui, window), (100, 100, 203, 157), "unsnapped");

    // With shift, right 303 -> 300 and bottom 257 -> 250; left and top are already multiples.
    let mut on = Pump { shift: true };
    ui.use_time(LocalTime(2.0), &mut on);
    ui.mouse_move(LocalTime(2.0), 298, 252);
    assert_eq!(box_of(&ui, window), (100, 100, 200, 150), "snapped");
    ui.mouse_up(action::PRIMARY_CLICK, 298, 252, false);
}

/// The other half of the same seam: a window
/// **dragged** by a `Dragbar` snaps its origin, x unconditionally and y only while it is not
/// already resting on the parent's bottom edge.
#[test]
fn shift_snaps_a_move_on_x_always_and_on_y_only_off_the_bottom_edge() {
    let mut ui = UiSystem::new((800, 600));
    let (_f, window, _h) = tree(&mut ui);
    let mut on = Pump { shift: true };
    ui.use_time(LocalTime(1.0), &mut on);

    // A drag of +7/+3 from (100,100): 107 -> 100, 103 -> 100.
    ui.start_movement(window, 0, 0);
    ui.mouse_move_element(window, 7, 3);
    assert_eq!(
        box_of(&ui, window),
        (100, 100, 200, 150),
        "both axes snapped"
    );

    // Now the bottom edge, and **the numbers have to be chosen so the two readings differ**.
    // With a 150-high window in a 300-high frame the edge is y = 150, which is already a multiple
    // of ten: snapped and unsnapped agree, and a mutation that deletes the condition would pass.
    // Shrinking the window to 149 puts the edge at 151.
    ui.stop_movement(window);
    ui.resize_to(window, 200, 149);
    ui.start_movement(window, 0, 0);
    ui.mouse_move_element(window, 7, 51);
    assert_eq!(
        box_of(&ui, window),
        (100, 151, 200, 149),
        "x snapped 107->100; y is on the bottom edge at 151 and was NOT pulled back to 150"
    );
    // And one pixel off the edge it snaps as usual: 150 < 151, so the condition holds.
    ui.stop_movement(window);
    ui.start_movement(window, 0, 0);
    ui.mouse_move_element(window, 7, -1);
    assert_eq!(
        box_of(&ui, window),
        (100, 150, 200, 149),
        "150 is already on the grid"
    );
    ui.stop_movement(window);
    ui.start_movement(window, 0, 0);
    ui.mouse_move_element(window, 7, -4);
    assert_eq!(
        box_of(&ui, window),
        (100, 140, 200, 149),
        "147 snaps down to 140"
    );
}

// ---------------------------------------------------------------------------------------------
// the census
// ---------------------------------------------------------------------------------------------

/// **How many windows can actually be resized and dragged in the shipped UI**, counted over the
/// 101 layouts in the retail `client_local_English.dat` (decoded by the same pass as the layout
/// conformance gate). The figures were cross-checked against an independent reader of the same
/// data.
///
/// The numbers are asserted rather than printed, because a census with no expected value is not an
/// instrument: if a later change to the decoder drops the resizebars, nothing else in this file
/// would notice.
#[test]
fn the_shipped_layouts_carry_seventy_one_resize_handles_and_thirty_drag_handles() {
    use super::layout_conformance::{env, parse_all, walk_hist};
    use std::collections::BTreeMap;

    let e = env().expect("the retail client_local_English.dat");
    let layouts = parse_all(&e);

    let mut resizebars = 0_usize;
    let mut dragbars = 0_usize;
    let mut layouts_with_a_resizebar = 0_usize;
    let mut total = 0_usize;
    for l in layouts.values() {
        let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
        for r in l.elements.values() {
            walk_hist(r, &mut hist);
        }
        total += hist.values().sum::<usize>();
        let r = hist.get(&ty::RESIZEBAR.0).copied().unwrap_or(0);
        resizebars += r;
        if r > 0 {
            layouts_with_a_resizebar += 1;
        }
        dragbars += hist.get(&ty::DRAGBAR.0).copied().unwrap_or(0);
    }

    assert_eq!(layouts.len(), 101, "the shipped layout set");
    assert_eq!(total, 2162, "every element in it — the denominator");
    assert_eq!(resizebars, 71, "resize-bar element (type 9)");
    assert_eq!(dragbars, 30, "drag-bar element (type 2)");
    assert_eq!(
        layouts_with_a_resizebar, 13,
        "spread over this many layouts"
    );
}
