//! Plugin windows and the plugin bar as the player arranges them, driven frame by frame with no
//! art, no device and no host: a window's edges and corners dragged to size it, the size and the
//! place the player left it at given back, kept on the screen as the screen and the scale change,
//! and the bar scrolled and narrowed when its buttons do not all fit.

use dereth_ui_kit::draw::Rect;

use crate::kit_tests::{node, view, Harness};
use crate::{BarItem, BarPlace, Input, WindowView};

/// A window of a few nodes, declared resizable (`Some(true)`), fixed (`Some(false)`) or neither.
fn arranged(resizable: Option<bool>) -> WindowView {
    let mut tree = node("window")
        .with("id", "main")
        .with("title", "main")
        .with(
            "children",
            vec![
                node("label").with("text", "a line of text"),
                node("button").with("id", "go").with("text", "Go"),
            ],
        );
    if let Some(r) = resizable {
        tree = tree.with("resizable", r);
    }
    view(
        dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
        1,
    )
}

/// The left button pressed at `from`, the pointer moved to `to` and let go there, a frame each.
fn drag(h: &mut Harness, views: &[WindowView], from: (f32, f32), to: (f32, f32)) -> Vec<Input> {
    h.input.mouse = from;
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let mut out = h.frame(views);
    h.input.mouse = to;
    out.extend(h.frame(views));
    h.input.down[0] = false;
    h.input.released[0] = true;
    out.extend(h.frame(views));
    out.extend(h.frame(views));
    out
}

/// Whether `inner` lies within `outer`, give or take a hundredth of a pixel.
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x - 0.01
        && inner.y >= outer.y - 0.01
        && inner.right() <= outer.right() + 0.01
        && inner.bottom() <= outer.bottom() + 0.01
}

/// A window declared neither way takes the size the player drags its right edge, its left edge
/// and its bottom right corner to, the edges not held staying put, its nodes laid out again at
/// each size; narrowed, it is never narrower than its nodes and its title need, and nothing of it
/// is drawn past its frame. Its plugin hears nothing.
/// Behaviour: plugins.ui.a-window-takes-the-size-the-player-drags-its-edges-to
#[test]
fn a_windows_edges_and_corner_take_the_size_the_player_drags_them_to() {
    let views = [arranged(None)];
    let mut h = Harness::new();
    h.layer.ui.place_hint("test.kit", "main", 300.0, 100.0);
    h.frame(&views);
    h.frame(&views);
    let a = h.rect("main");
    let mid = a.y + a.h / 2.0;
    let out = drag(
        &mut h,
        &views,
        (a.right() - 2.0, mid),
        (a.right() + 98.0, mid),
    );
    let b = h.rect("main");
    assert_eq!((b.x, b.y, b.h), (a.x, a.y, a.h), "the left edge stays");
    assert!((b.w - (a.w + 100.0)).abs() < 0.5, "{a:?} to {b:?}");
    let out2 = drag(&mut h, &views, (b.x + 2.0, mid), (b.x - 48.0, mid));
    let c = h.rect("main");
    assert!((c.right() - b.right()).abs() < 0.5, "the right edge stays");
    assert!((c.w - (b.w + 50.0)).abs() < 0.5, "{b:?} to {c:?}");
    let out3 = drag(
        &mut h,
        &views,
        (c.right() - 2.0, c.bottom() - 2.0),
        (c.right() + 38.0, c.bottom() + 28.0),
    );
    let d = h.rect("main");
    assert_eq!((d.x, d.y), (c.x, c.y));
    assert!((d.w - (c.w + 40.0)).abs() < 0.5 && (d.h - (c.h + 30.0)).abs() < 0.5);
    assert!(
        out.iter()
            .chain(&out2)
            .chain(&out3)
            .all(|i| !matches!(i, Input::Resize { .. })),
        "its plugin hears nothing of the frame's size"
    );
    assert_eq!(
        h.layer.ui.sizes(),
        vec![(("test.kit".into(), "main".into()), (d.w, d.h))],
        "the size is there for the caller to remember"
    );
    // Narrowed far past its nodes.
    drag(&mut h, &views, (d.right() - 2.0, mid), (d.x - 2000.0, mid));
    let e = h.rect("main");
    assert!(
        e.w > 40.0 && e.w < a.w,
        "as narrow as its nodes and title allow: {e:?}"
    );
    let list = &h.drawn().windows[0].2;
    for q in &list.quads {
        if let Some(shown) = q.clip.map_or(Some(q.dst), |c| q.dst.intersect(&c)) {
            assert!(within(shown, e), "{shown:?} is drawn past the window {e:?}");
        }
    }
}

/// A window its plugin declares resizable is laid out again at the size the player drags it to,
/// and its plugin hears that size in cells and lines when the player lets go.
/// Behaviour: plugins.ui.a-window-takes-the-size-the-player-drags-its-edges-to
#[test]
fn a_window_declared_resizable_is_laid_out_at_the_players_size_and_its_plugin_hears_it() {
    let views = [arranged(Some(true))];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let a = h.rect("main");
    let out = drag(
        &mut h,
        &views,
        (a.right() - 2.0, a.bottom() - 2.0),
        (a.right() + 68.0, a.bottom() + 58.0),
    );
    let b = h.rect("main");
    assert!((b.w - (a.w + 70.0)).abs() < 0.5 && (b.h - (a.h + 60.0)).abs() < 0.5);
    let resized: Vec<&Input> = out
        .iter()
        .filter(|i| matches!(i, Input::Resize { .. }))
        .collect();
    assert_eq!(resized.len(), 1, "heard once, on letting go: {out:?}");
    let Input::Resize { width, height, .. } = resized[0] else {
        unreachable!("filtered")
    };
    assert!(
        *width > 24 && *height > 3,
        "{width} cells by {height} lines"
    );
}

/// A window its plugin declares `resizable = false` keeps the size its nodes need: a drag on its
/// edge leaves it as it was.
/// Behaviour: plugins.ui.a-window-takes-the-size-the-player-drags-its-edges-to
#[test]
fn a_window_declared_not_resizable_keeps_its_size() {
    let views = [arranged(Some(false))];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let a = h.rect("main");
    let mid = a.y + a.h / 2.0;
    drag(
        &mut h,
        &views,
        (a.right() - 2.0, mid),
        (a.right() + 98.0, mid),
    );
    assert_eq!(h.rect("main"), a);
    assert!(h.layer.ui.sizes().is_empty());
}

/// The grip is part of the frame: a window the player may size is as tall as the same window
/// they may not (the grip takes no room from its body), and a press in the frame's bottom right
/// corner takes hold of both its right and bottom edges.
/// Behaviour: plugins.ui.a-windows-grip-is-part-of-its-frame
#[test]
fn a_windows_grip_takes_no_room_from_its_body_and_stands_in_the_frames_corner() {
    let mut h = Harness::new();
    let fixed = [arranged(Some(false))];
    h.frame(&fixed);
    h.frame(&fixed);
    let kept = h.rect("main");
    let mut h = Harness::new();
    let views = [arranged(None)];
    h.frame(&views);
    h.frame(&views);
    let a = h.rect("main");
    assert_eq!(
        (a.w, a.h),
        (kept.w, kept.h),
        "the grip takes no room: {a:?} against {kept:?}"
    );
    // The corner's last pixel is the frame's, outside the body.
    let corner = (a.right() - 1.0, a.bottom() - 1.0);
    drag(&mut h, &views, corner, (corner.0 + 30.0, corner.1 + 20.0));
    let b = h.rect("main");
    assert!(
        (b.w - (a.w + 30.0)).abs() < 0.5 && (b.h - (a.h + 20.0)).abs() < 0.5,
        "the corner sizes it both ways: {a:?} to {b:?}"
    );
}

/// A window opens where and at the size the player left it, as the caller hands them back, kept
/// on the screen; it stays on the screen as the screen narrows and the scale grows; an open
/// window is not moved by a place handed back, and one opened again comes where it was left.
/// Behaviour: plugins.ui.a-window-and-the-bar-come-back-where-the-player-left-them-on-the-screen
#[test]
fn a_window_opens_where_and_at_the_size_left_and_stays_on_the_screen() {
    let open = [arranged(None)];
    let mut closed = arranged(None);
    closed.open = false;
    let closed = [closed];
    let mut h = Harness::new();
    h.frame(&closed);
    // Left past the bottom right of this screen, at a size of its own.
    h.layer.ui.place_hint("test.kit", "main", 1100.0, 600.0);
    h.layer.ui.size_hint("test.kit", "main", 400.0, 300.0);
    h.frame(&open);
    h.frame(&open);
    let screen = |h: &Harness| Rect::new(0.0, 0.0, h.screen.0, h.screen.1);
    let a = h.rect("main");
    assert_eq!((a.w, a.h), (400.0, 300.0), "the size it was left at");
    assert!(within(a, screen(&h)), "{a:?} is on the screen");
    assert_eq!(
        (a.x, a.y),
        (880.0, 420.0),
        "as near its place as the screen allows"
    );
    // The screen narrows.
    h.screen = (800.0, 600.0);
    h.frame(&open);
    let b = h.rect("main");
    assert!(within(b, screen(&h)), "{b:?} is on the narrower screen");
    // The scale grows.
    h.scale = 1.5;
    h.frame(&open);
    let c = h.rect("main");
    assert!(
        within(c, screen(&h)),
        "{c:?} is on the screen at the larger scale"
    );
    // A place handed back while it is open does not move it.
    h.layer.ui.place_hint("test.kit", "main", 5.0, 5.0);
    h.frame(&open);
    assert_eq!(h.rect("main"), c);
    // Closed and opened again, it comes where it was last left.
    h.frame(&closed);
    h.layer.ui.place_hint("test.kit", "main", 30.0, 40.0);
    h.frame(&open);
    h.frame(&open);
    let d = h.rect("main");
    assert_eq!((d.x, d.y), (30.0, 40.0));
}

/// Plugins enough for a long bar.
fn plugins(n: usize) -> Vec<BarItem> {
    (0..n)
        .map(|i| BarItem {
            plugin: format!("test.p{i}"),
            name: format!("Plugin {i}"),
            state: "running".into(),
            open: false,
        })
        .collect()
}

/// The plugin a click at `(x, y)` on the bar opens the window of, if any.
fn bar_click(h: &mut Harness, bar: &[BarItem], (x, y): (f32, f32)) -> Option<String> {
    h.input.mouse = (x, y);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let mut out = h.frame_with(&[], bar);
    h.input.down[0] = false;
    h.input.released[0] = true;
    out.extend(h.frame_with(&[], bar));
    out.into_iter().find_map(|i| match i {
        Input::Bar { plugin } => Some(plugin),
        _ => None,
    })
}

/// A bar of more buttons than the screen holds is no wider than the screen and scrolls: the
/// wheel and its step right bring later buttons under the pointer, as far as the last; its grip
/// narrows it, keeping a little of its buttons, and the width is there to remember; left past
/// the screen's edge it is drawn on the screen.
/// Behaviour: plugins.ui.the-plugin-bar-scrolls-when-its-buttons-do-not-fit-and-takes-the-width-given-it
#[test]
fn the_plugin_bar_scrolls_when_its_buttons_do_not_fit_and_takes_the_width_given_it() {
    let bar = plugins(24);
    let mut h = Harness::new();
    h.screen = (300.0, 400.0);
    h.frame_with(&[], &bar);
    h.frame_with(&[], &bar);
    let r = h.layer.bar_rect().expect("the bar is drawn");
    assert!(r.w <= 300.0 && r.x >= 0.0, "on the screen: {r:?}");
    let at = (r.x + 20.0, r.y + r.h / 2.0);
    let first = bar_click(&mut h, &bar, at).expect("a button is under the pointer");
    assert_eq!(first, "test.p0");
    // The wheel down over the bar.
    h.input.mouse = at;
    h.input.wheel = -1.0;
    h.frame_with(&[], &bar);
    let later = bar_click(&mut h, &bar, at).expect("a button is under the pointer");
    assert_ne!(later, first, "the wheel scrolled the buttons along");
    // Scrolled to the end, the last button is under the end of the buttons' room.
    for _ in 0..80 {
        h.input.mouse = at;
        h.input.wheel = -1.0;
        h.frame_with(&[], &bar);
    }
    let r = h.layer.bar_rect().expect("the bar is drawn");
    // Past the two steps (a bar's height each) and the grip.
    let end = (r.right() - 2.0 * r.h - 10.0 - 6.0, r.y + r.h / 2.0);
    assert_eq!(bar_click(&mut h, &bar, end).as_deref(), Some("test.p23"));
    // Its grip, at its right end, drawn leftwards narrows it.
    let grip = (r.right() - 4.0, r.y + r.h / 2.0);
    h.input.mouse = grip;
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame_with(&[], &bar);
    h.input.mouse = (grip.0 - 100.0, grip.1);
    h.frame_with(&[], &bar);
    h.input.down[0] = false;
    h.input.released[0] = true;
    h.frame_with(&[], &bar);
    let narrow = h.layer.bar_rect().expect("the bar is drawn");
    assert!(
        (narrow.w - (r.w - 100.0)).abs() < 0.5,
        "{r:?} to {narrow:?}"
    );
    assert_eq!(
        h.layer.bar_place().and_then(|b| b.width),
        Some(narrow.w),
        "the width is there to remember"
    );
    // Left past the screen's bottom right, it is drawn on the screen.
    let mut h = Harness::new();
    h.screen = (600.0, 400.0);
    h.bar_at = BarPlace {
        at: (5000.0, 5000.0),
        width: Some(300.0),
    };
    h.frame_with(&[], &bar);
    let r = h.layer.bar_rect().expect("the bar is drawn");
    assert!(within(r, Rect::new(0.0, 0.0, 600.0, 400.0)), "{r:?}");
    assert_eq!(r.w, 300.0, "at the width it was given");
}

/// A bar whose buttons all fit is as wide as they need and does not scroll.
/// Behaviour: plugins.ui.the-plugin-bar-scrolls-when-its-buttons-do-not-fit-and-takes-the-width-given-it
#[test]
fn a_bar_whose_buttons_fit_is_as_wide_as_they_need() {
    let bar = plugins(2);
    let mut h = Harness::new();
    h.frame_with(&[], &bar);
    let r = h.layer.bar_rect().expect("the bar is drawn");
    let at = (r.x + 20.0, r.y + r.h / 2.0);
    h.input.mouse = at;
    h.input.wheel = -1.0;
    h.frame_with(&[], &bar);
    assert_eq!(h.layer.bar_rect(), Some(r));
    assert_eq!(bar_click(&mut h, &bar, at).as_deref(), Some("test.p0"));
}

/// A font of the tests' own, every character 8 pixels across, drawn at its own size.
fn eight_wide_art() -> dereth_ui_kit::art::Art {
    let glyphs = [crate::skin::AtlasGlyph {
        ch: '?',
        x: 0,
        y: 0,
        w: 6,
        h: 9,
        bearing_x: 1,
        bearing_y: -9,
        advance: 8,
    }];
    let font = crate::skin::atlas_font(&glyphs, 15, 12, 4, 2, vec![255; 32]);
    dereth_ui_kit::art::Art::plain(Some(font), dereth_ui_kit::palette::Palette::retail_plain())
}

/// A button is as wide as its label as the button draws it and, at each end, half its height and
/// a gap: the label stands in its middle with the same room at both ends, past the cap and a gap.
/// Behaviour: plugins.ui.controls-are-as-wide-as-their-text-needs
#[test]
fn a_button_is_as_wide_as_its_drawn_label_with_even_room_at_both_ends() {
    let tree = node("window")
        .with("id", "main")
        .with("title", "main")
        .with(
            "children",
            vec![node("row").with(
                "children",
                vec![node("button")
                    .with("id", "start")
                    .with("text", "Start the macro")],
            )],
        );
    let views = [view(
        dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
        1,
    )];
    let mut h = Harness::with_art(eight_wide_art());
    h.frame(&views);
    h.frame(&views);
    let body = h.rect("main");
    // The button: the one control below the title bar.
    let button = crate::kit_tests::controls(&h)
        .into_iter()
        .find(|r| r.y > body.y + 20.0)
        .expect("the button is noted");
    let list = &h.drawn().windows[0].2;
    // The label's letters: the font's quads within the button.
    let letters: Vec<Rect> = list
        .quads
        .iter()
        .filter(|q| q.tex.is_some() && within(q.dst, button))
        .map(|q| q.dst)
        .collect();
    let left = letters.iter().map(|r| r.x).fold(f32::MAX, f32::min);
    let right = letters.iter().map(Rect::right).fold(f32::MIN, f32::max);
    let (before, after) = (left - button.x, button.right() - right);
    assert!(
        (before - after).abs() <= 1.5,
        "the same room at both ends: {before} and {after} in {button:?}"
    );
    assert!(
        before >= button.h / 2.0 + 2.0,
        "room at each end past the cap (half the button's height) and a gap: {before} of {}",
        button.h
    );
}

/// A window is never dragged narrower than its nodes' least width: a list with a column 40 cells
/// wide holds a window that asks for 60 at least 40 wide, however far the player drags its edge.
/// Behaviour: plugins.ui.a-window-is-never-narrower-than-its-nodes-least-width
#[test]
fn a_window_is_never_dragged_narrower_than_its_nodes_least_width() {
    let tree = node("window")
        .with("id", "main")
        .with("title", "main")
        .with("width", 60_i64)
        .with(
            "children",
            vec![node("list").with(
                "columns",
                vec![dereth_plugin_api::Value::map()
                    .with("title", "Line")
                    .with("width", 40_i64)],
            )],
        );
    let views = [view(
        dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
        1,
    )];
    let mut h = Harness::new();
    h.layer.ui.place_hint("test.kit", "main", 300.0, 100.0);
    h.frame(&views);
    h.frame(&views);
    let a = h.rect("main");
    let mid = a.y + a.h / 2.0;
    drag(&mut h, &views, (a.right() - 2.0, mid), (a.x - 2000.0, mid));
    let b = h.rect("main");
    // The column, a cell's room between it and the list's rim and gap either side.
    let least = 40.0 * 7.0 + 2.0 * (2.0 + 3.0);
    assert!(
        b.w >= least,
        "never narrower than its list's least width: {b:?}"
    );
    assert!(b.w < a.w, "but narrowed: {a:?} to {b:?}");
}

/// A list scrolled to its end, or with every row in sight, keeps its newest row in sight as rows
/// are added at its end: the bottom row of a three-row list is its last after seven more arrive.
/// Behaviour: plugins.ui.a-list-scrolled-to-its-end-keeps-its-newest-row-in-sight
#[test]
fn a_list_at_its_end_keeps_its_newest_row_in_sight_as_rows_are_added() {
    let log = |n: usize, revision: u64| {
        let rows: Vec<dereth_plugin_api::Value> = (1..=n)
            .map(|i| {
                dereth_plugin_api::Value::List(vec![dereth_plugin_api::Value::from(format!(
                    "line {i}"
                ))])
            })
            .collect();
        let tree = node("window")
            .with("id", "main")
            .with("title", "main")
            .with(
                "children",
                vec![node("list")
                    .with("id", "log")
                    .with("rows", rows)
                    .with("height", 3_i64)],
            );
        view(
            dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
            revision,
        )
    };
    let mut h = Harness::new();
    let three = [log(3, 1)];
    h.frame(&three);
    h.frame(&three);
    let ten = [log(10, 2)];
    h.frame(&ten);
    h.frame(&ten);
    // The bottom row in sight: just above the list's bottom rim.
    let rows = crate::kit_tests::controls(&h);
    let bottom = rows
        .iter()
        .filter(|r| r.h < 30.0)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .copied()
        .expect("the rows are noted");
    let chosen = h.click(&ten, bottom.x + 4.0, bottom.y + bottom.h / 2.0);
    assert!(
        chosen
            .iter()
            .any(|i| matches!(i, Input::Select { row: 10, .. })),
        "the newest row is the bottom one in sight: {chosen:?}"
    );
}

/// A window taller than the screen leaves is drawn as tall as it can be, and its body scrolls
/// through its nodes under the wheel: a press where its first button stood reaches a later one
/// once the body has scrolled.
/// Behaviour: plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them
#[test]
fn a_window_shorter_than_its_nodes_scrolls_its_body_under_the_wheel() {
    let buttons: Vec<dereth_plugin_api::Value> = (0..20)
        .map(|i| {
            node("button")
                .with("id", format!("b{i}"))
                .with("text", format!("B{i}"))
        })
        .collect();
    let tree = node("window")
        .with("id", "main")
        .with("title", "main")
        .with("resizable", false)
        .with("children", buttons);
    let views = [view(
        dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
        1,
    )];
    let mut h = Harness::new();
    h.screen = (800.0, 300.0);
    h.layer.ui.place_hint("test.kit", "main", 100.0, 0.0);
    h.frame(&views);
    h.frame(&views);
    let r = h.rect("main");
    assert!(r.bottom() <= 300.0 + 0.5, "kept on the screen: {r:?}");
    let first = crate::kit_tests::controls(&h)
        .into_iter()
        .find(|c| c.y > r.y + 20.0)
        .expect("the first button");
    let at = (first.x + first.w / 2.0, first.y + first.h / 2.0);
    let clicked = |out: &[Input]| {
        out.iter().find_map(|i| match i {
            Input::Click { node, .. } => Some(node.clone()),
            _ => None,
        })
    };
    assert_eq!(clicked(&h.click(&views, at.0, at.1)).as_deref(), Some("b0"));
    h.input.mouse = at;
    h.input.wheel = -2.0;
    h.frame(&views);
    h.frame(&views);
    let later = clicked(&h.click(&views, at.0, at.1));
    assert!(
        later.as_deref().is_some_and(|n| n != "b0"),
        "a later button under the pointer: {later:?}"
    );
}

/// The pad's focus stepping down a window too short for its nodes, past the last one in sight,
/// scrolls the body just far enough to bring the next one into sight.
/// Behaviour: plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them
#[test]
fn the_pads_focus_stepping_past_a_short_windows_last_control_in_sight_scrolls_its_body() {
    use dereth_ui_kit::nav::{Cursor, Dir, Stepped};
    let buttons: Vec<dereth_plugin_api::Value> = (0..20)
        .map(|i| {
            node("button")
                .with("id", format!("b{i}"))
                .with("text", format!("B{i}"))
        })
        .collect();
    let tree = node("window")
        .with("id", "main")
        .with("title", "main")
        .with("resizable", false)
        .with("children", buttons);
    let views = [view(
        dereth_plugin_api::UiNode::window_from_value(&tree).expect("a valid window"),
        1,
    )];
    let mut h = Harness::new();
    h.screen = (800.0, 300.0);
    h.layer.ui.place_hint("test.kit", "main", 100.0, 0.0);
    h.frame(&views);
    h.frame(&views);
    let r = h.rect("main");
    let snap = h.input.nav.last.clone();
    let mut cursor = Cursor::default();
    cursor.focus = snap.targets.iter().find(|t| t.rect.y > r.y + 20.0).copied();
    let first = cursor.focus.expect("the first button").rect;
    let mut scrolled = None;
    for _ in 0..20 {
        match cursor.step(&snap, Dir::Down) {
            Stepped::Moved => {}
            Stepped::ScrollBy { area, by } => {
                scrolled = Some((area, by));
                break;
            }
            other => panic!("the focus stopped before the body's end: {other:?}"),
        }
    }
    let (area, by) = scrolled.expect("the body scrolls under the focus");
    assert!(
        by > 0.0 && r.contains(area.x + 1.0, area.y + 1.0),
        "{area:?} by {by}"
    );
    h.input.pad.scroll_list = Some((area, by));
    h.frame(&views);
    h.frame(&views);
    let moved = h
        .input
        .nav
        .last
        .targets
        .iter()
        .filter(|t| t.rect.x == first.x)
        .map(|t| t.rect.y)
        .fold(f32::MAX, f32::min);
    assert!(
        moved < first.y - 1.0 || h.input.nav.last.targets.iter().all(|t| t.rect != first),
        "the buttons moved up: the first was at {}, now the topmost is at {moved}",
        first.y
    );
}

/// A HUD holding a button, anchored at the screen's top right.
fn hud_view() -> WindowView {
    view(
        dereth_plugin_api::UiNode::window_from_value(
            &node("hud")
                .with("id", "hud")
                .with("anchor", "top_right")
                .with(
                    "children",
                    vec![node("button").with("id", "ping").with("text", "Ping")],
                ),
        )
        .expect("a valid HUD"),
        1,
    )
}

/// Unlocked, a press on a HUD moves it where it is dragged and its button takes no click; its
/// place is there for the caller to keep for the character, kept on the screen as the screen
/// narrows, and handed back it puts the HUD there again. Locked, a drag leaves it where it is and
/// its button answers again.
/// Behaviour: plugins.ui.unlocked-a-huds-place-is-the-players-kept-on-the-screen
#[test]
fn an_unlocked_hud_is_dragged_kept_where_it_is_put_and_stays_on_the_screen() {
    let views = [hud_view()];
    let mut h = Harness::new();
    h.frame(&views);
    let a = h.rect("hud");
    assert!(a.right() > 1200.0, "at its anchor first: {a:?}");
    h.huds_unlocked = true;
    h.frame(&views);
    let out = drag(
        &mut h,
        &views,
        (a.x + a.w / 2.0, a.y + a.h / 2.0),
        (400.0, 300.0),
    );
    assert!(
        out.iter().all(|i| !matches!(i, Input::Click { .. })),
        "unlocked, its button takes no click: {out:?}"
    );
    let b = h.rect("hud");
    assert!(
        (b.x + b.w / 2.0 - 400.0).abs() < 1.0 && (b.y + b.h / 2.0 - 300.0).abs() < 1.0,
        "moved where it was dragged: {b:?}"
    );
    let key = ("test.kit".to_owned(), "hud".to_owned());
    assert_eq!(
        h.layer.ui.places(),
        vec![(key.clone(), (b.x, b.y))],
        "its place, for the caller to keep"
    );
    // Dragged past the screen's edge, it stays wholly on it.
    let out = drag(&mut h, &views, (b.x + 2.0, b.y + 2.0), (2000.0, 2000.0));
    assert!(out.iter().all(|i| !matches!(i, Input::Click { .. })));
    let c = h.rect("hud");
    assert!(
        within(c, Rect::new(0.0, 0.0, 1280.0, 720.0)),
        "on the screen: {c:?}"
    );
    // The screen narrows: still on it.
    h.screen = (800.0, 600.0);
    h.frame(&views);
    let d = h.rect("hud");
    assert!(within(d, Rect::new(0.0, 0.0, 800.0, 600.0)), "{d:?}");
    // Locked again: a drag does not move it, and its button answers.
    h.huds_unlocked = false;
    h.frame(&views);
    let e = h.rect("hud");
    let out = drag(&mut h, &views, (e.x + 3.0, e.y + 3.0), (100.0, 100.0));
    assert_eq!(h.rect("hud"), e, "locked, it stays");
    let ping = h
        .layer
        .ui
        .rect_of("test.kit", "hud")
        .map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
        .expect("drawn");
    let out2 = h.click(&views, ping.0, ping.1);
    assert!(
        out.iter()
            .chain(&out2)
            .any(|i| matches!(i, Input::Click { node, .. } if node == "ping")),
        "locked, its button answers: {out:?} {out2:?}"
    );
    // At the next start the kept place is handed back: the HUD opens there.
    let mut fresh = Harness::new();
    fresh.layer.ui.place_hint("test.kit", "hud", 200.0, 150.0);
    fresh.frame(&views);
    let f = fresh.rect("hud");
    assert_eq!((f.x, f.y), (200.0, 150.0), "where it was kept");
}
