//! The layout rules on trees built here, at the reference measures: a cell 7 wide, a line 16, a
//! control 22, a gap 3, each character a cell.

use super::*;
use crate::value::Value;

fn node(kind: &str) -> Value {
    Value::map().with("kind", kind)
}

fn window(children: Vec<Value>) -> UiNode {
    UiNode::window_from_value(&node("window").with("id", "w").with("children", children))
        .expect("a valid window")
}

/// `tree`'s nodes laid out in a body of `size`.
fn lay(tree: &UiNode, size: (f32, f32)) -> Laid {
    let m = Metrics::reference();
    let w = want(tree, &m, &Cells(m.cell));
    place_body(tree, &w, &m, size)
}

fn rows(n: usize, text: &str) -> Value {
    Value::List(
        (0..n)
            .map(|i| Value::List(vec![Value::from(format!("{text} {i}"))]))
            .collect(),
    )
}

/// Across a column every node takes the column's width, but a picture, which keeps its own size;
/// down it each takes its natural height, a gap apart.
/// Behaviour: plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size
#[test]
fn a_columns_nodes_take_its_width_and_a_picture_keeps_its_own() {
    let t = window(vec![
        node("label").with("text", "abc"),
        node("button").with("text", "Go"),
        node("canvas").with("width", 50_i64).with("height", 20_i64),
    ]);
    let l = lay(&t, (200.0, 100.0));
    let [label, button, canvas] = &l.children[..] else {
        panic!("three nodes");
    };
    assert_eq!(
        (label.x, label.y, label.w, label.h),
        (0.0, 0.0, 200.0, 16.0)
    );
    assert_eq!((button.y, button.w, button.h), (19.0, 200.0, 22.0));
    assert_eq!(
        (canvas.x, canvas.y, canvas.w, canvas.h),
        (0.0, 44.0, 50.0, 20.0)
    );
}

/// In a row, a node that keeps its own height (a button, a label) stands in the row's middle, and
/// one that stretches (a list) takes the row's height.
/// Behaviour: plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size
#[test]
fn in_a_row_a_list_takes_the_rows_height_and_a_button_stands_in_its_middle() {
    let t = window(vec![node("row").with(
        "children",
        vec![
            node("button").with("text", "Go"),
            node("list")
                .with("rows", rows(4, "x"))
                .with("height", 4_i64),
        ],
    )]);
    let l = lay(&t, (300.0, 200.0));
    let row = &l.children[0];
    let [go, list] = &row.children[..] else {
        panic!("two nodes");
    };
    assert_eq!(list.h, row.h, "the list is as tall as the row");
    assert_eq!(go.h, 22.0);
    assert_eq!(
        go.y,
        ((row.h - 22.0) / 2.0).round(),
        "the button in its middle"
    );
    assert_eq!(list.x, go.w + 6.0, "two gaps apart");
}

/// The room left down a window goes to its growing nodes in proportion to their shares, none past
/// its most; a node that does not grow keeps its natural height.
/// Behaviour: plugins.ui.growing-nodes-share-the-room-left-in-proportion-within-their-bounds
#[test]
fn growing_nodes_share_the_room_left_in_proportion_and_stop_at_their_most() {
    let list = |grow: f64| {
        node("list")
            .with("rows", rows(2, "r"))
            .with("height", 2_i64)
            .with("grow", grow)
    };
    let t = window(vec![node("label").with("text", "x"), list(1.0), list(3.0)]);
    let l = lay(&t, (200.0, 400.0));
    let [label, a, b] = &l.children[..] else {
        panic!("three nodes");
    };
    assert_eq!(label.h, 16.0);
    let natural = 2.0 * 18.0 + 4.0;
    let left = 400.0 - 16.0 - 2.0 * 3.0 - 2.0 * natural;
    assert!((a.h - (natural + left / 4.0)).abs() <= 1.0, "{}", a.h);
    assert!((b.h - (natural + 3.0 * left / 4.0)).abs() <= 1.0, "{}", b.h);
    assert_eq!(b.y + b.h, 400.0, "the window's whole height is used");
    // The second stops at five lines; the first takes what it leaves.
    let t = window(vec![
        node("label").with("text", "x"),
        list(1.0),
        list(3.0).with("max_height", 5_i64),
    ]);
    let l = lay(&t, (200.0, 400.0));
    assert_eq!(l.children[2].h, 80.0);
    assert_eq!(l.children[1].h, 400.0 - 16.0 - 6.0 - 80.0);
}

/// A field growing across its row fills the row's width and leaves the row one control high: the
/// room left down the window goes to the list that grows, not to the row; a list growing in a row
/// makes the row take the room down.
/// Behaviour: plugins.ui.growing-nodes-share-the-room-left-in-proportion-within-their-bounds
#[test]
fn a_field_growing_across_its_row_leaves_the_rows_height_alone() {
    let field_row = || {
        node("row").with(
            "children",
            vec![
                node("input").with("id", "f").with("grow", 1.0),
                node("button").with("text", "Go"),
            ],
        )
    };
    let list = node("list").with("rows", rows(2, "r")).with("grow", 1.0);
    let t = window(vec![field_row(), list.clone()]);
    let l = lay(&t, (200.0, 300.0));
    let [row, grown] = &l.children[..] else {
        panic!("two nodes");
    };
    assert_eq!(row.h, 22.0, "the row stays one control high");
    assert!(
        row.children[0].w > 200.0 / 2.0,
        "the field takes the row's width: {}",
        row.children[0].w
    );
    assert_eq!(grown.y + grown.h, 300.0, "the list takes the room left");
    // A list growing in a row: the row grows down the window.
    let t = window(vec![
        node("label").with("text", "x"),
        node("row").with("children", vec![list]),
    ]);
    let l = lay(&t, (200.0, 300.0));
    assert_eq!(l.children[1].y + l.children[1].h, 300.0);
}

/// Where no node of a window says `grow`, its last list or text box grows; on each tab's page the
/// same, page by page; a `grow` said anywhere outside the pages takes the window's own over.
/// Behaviour: plugins.ui.where-nothing-says-the-last-list-or-text-box-grows
#[test]
fn where_nothing_says_the_last_list_or_text_box_grows_and_each_page_has_its_own() {
    let list = || {
        node("list")
            .with("rows", rows(2, "r"))
            .with("height", 2_i64)
    };
    let t = window(vec![
        list(),
        node("textbox"),
        node("label").with("text", "end"),
    ]);
    let l = lay(&t, (300.0, 500.0));
    assert_eq!(l.children[0].h, 40.0, "the first list keeps its size");
    assert!(
        l.children[1].h > 100.0,
        "the text box grows: {}",
        l.children[1].h
    );
    assert_eq!(l.children[2].y + 16.0, 500.0, "the label stays at the end");
    let t = window(vec![node("tabs").with(
        "children",
        vec![
            node("group")
                .with("title", "One")
                .with("children", vec![list()]),
            node("group").with("title", "Two").with(
                "children",
                vec![list(), list(), node("label").with("text", "x")],
            ),
        ],
    )]);
    let l = lay(&t, (300.0, 500.0));
    let tabs = &l.children[0];
    assert_eq!(tabs.h, 500.0, "the tabs grow with the lists on their pages");
    let one = &tabs.children[0];
    assert_eq!(one.children[0].h, one.h, "page one's list fills it");
    let two = &tabs.children[1];
    assert_eq!(two.children[0].h, 40.0);
    assert!(two.children[1].h > 300.0, "page two's last list grows");
    let t = window(vec![list().with("grow", 1_i64), list()]);
    let l = lay(&t, (300.0, 500.0));
    assert_eq!(l.children[1].h, 40.0, "said: only the first grows");
}

/// A list's columns: a fixed one is its width in cells, a growing one shares what is left, and
/// where no column says, the last one grows to the list's right edge; a column's least width
/// holds the list's least.
/// Behaviour: plugins.ui.a-lists-columns-are-fixed-or-share-its-width
#[test]
fn a_lists_columns_are_fixed_or_grow_and_the_last_grows_where_none_says() {
    let col = |title: &str| Value::map().with("title", title);
    let t = window(vec![node("list")
        .with(
            "columns",
            vec![col("Time").with("width", 6_i64), Value::from("Line")],
        )
        .with("rows", rows(3, "a line"))]);
    let l = lay(&t, (300.0, 200.0));
    let list = &l.children[0];
    let chrome = 2.0 * (2.0 + 3.0) + 7.0;
    assert_eq!(list.parts, vec![42.0, 300.0 - chrome - 42.0]);
    let t = window(vec![node("list").with(
        "columns",
        vec![
            col("Plugin").with("grow", 1_i64),
            col("Version").with("width", 8_i64),
            col("State").with("width", 10_i64),
        ],
    )]);
    let l = lay(&t, (400.0, 200.0));
    assert_eq!(
        l.children[0].parts,
        vec![400.0 - (10.0 + 14.0) - 56.0 - 70.0, 56.0, 70.0]
    );
    let t = window(vec![node("list").with(
        "columns",
        vec![
            col("A").with("grow", 1_i64).with("min_width", 30_i64),
            Value::from("B"),
        ],
    )]);
    let m = Metrics::reference();
    let w = want(&t, &m, &Cells(m.cell));
    assert!(w.least().0 >= 210.0 + 21.0, "{:?}", w.least());
}

/// Short of room down a window, its lists give up rows toward their least (three rows) before
/// anything else; shorter than its nodes' least, the body scrolls through them, laid out at their
/// least height and as wide as the body less the scrollbar's room.
/// Behaviour: plugins.ui.a-window-shorter-than-its-nodes-scrolls-through-them
#[test]
fn short_of_room_lists_give_up_rows_and_shorter_still_the_body_scrolls() {
    let list = || {
        node("list")
            .with("rows", rows(20, "r"))
            .with("height", 10_i64)
    };
    let t = window(vec![list(), list()]);
    let natural = 18.0 * 10.0 + 4.0;
    let l = lay(&t, (300.0, 300.0));
    assert!(l.scroll.is_none());
    let (a, b) = (&l.children[0], &l.children[1]);
    assert!(a.h < natural && b.h < natural && (a.h - b.h).abs() <= 1.0);
    assert_eq!(b.y + b.h, 300.0);
    let t = window(
        (0..10)
            .map(|i| node("button").with("text", format!("B{i}")))
            .collect(),
    );
    let l = lay(&t, (200.0, 100.0));
    let least = 10.0 * 22.0 + 9.0 * 3.0;
    assert_eq!(l.scroll, Some(least));
    assert_eq!((l.w, l.h), (200.0, 100.0), "the body keeps its size");
    assert_eq!(l.children[0].w, 200.0 - 11.0, "less the scrollbar's room");
    assert_eq!(l.children[9].y + 22.0, least);
}

/// A button is as wide as its label and a cap and a gap at each end; a choice's box is as wide as
/// its longest option; the labelled controls of a column have their labels as wide as the
/// widest.
/// Behaviour: plugins.ui.controls-are-as-wide-as-their-text-needs
#[test]
fn a_button_keeps_room_at_its_ends_a_choice_fits_its_options_and_labels_line_up() {
    let t = window(vec![node("row").with(
        "children",
        vec![
            node("button").with("text", "Start"),
            node("choice").with("text", "Mode").with(
                "options",
                vec![Value::from("a"), Value::from("a much longer one")],
            ),
        ],
    )]);
    let l = lay(&t, (500.0, 100.0));
    let [start, mode] = &l.children[0].children[..] else {
        panic!("two nodes");
    };
    assert_eq!(start.w, 5.0 * 7.0 + 2.0 * 14.0);
    assert_eq!(mode.parts, vec![35.0]);
    assert_eq!(mode.w, 35.0 + 17.0 * 7.0 + 14.0 + 22.0);
    let t = window(vec![
        node("number").with("text", "A"),
        node("number").with("text", "Longer"),
    ]);
    let l = lay(&t, (300.0, 100.0));
    assert_eq!(l.children[0].parts, vec![49.0]);
    assert_eq!(l.children[1].parts, vec![49.0]);
}

/// A container's `justify` puts the room its nodes leave at their start, between them or around
/// them, and its `align` sets them at their own size across.
/// Behaviour: plugins.ui.a-node-stretches-across-its-container-unless-it-keeps-its-own-size
#[test]
fn justify_places_the_room_left_and_align_keeps_nodes_at_their_own_size() {
    let buttons = || {
        vec![
            node("button").with("text", "Go"),
            node("button").with("text", "Stop"),
        ]
    };
    let row = |justify: &str| {
        window(vec![node("row")
            .with("justify", justify)
            .with("children", buttons())])
    };
    let l = lay(&row("end"), (300.0, 60.0));
    let [a, b] = &l.children[0].children[..] else {
        panic!("two");
    };
    assert_eq!(b.x + b.w, 300.0);
    assert_eq!(a.x + a.w + 6.0, b.x);
    let l = lay(&row("between"), (300.0, 60.0));
    let [a, b] = &l.children[0].children[..] else {
        panic!("two");
    };
    assert_eq!((a.x, b.x + b.w), (0.0, 300.0));
    let t = window(vec![node("column")
        .with("align", "center")
        .with("children", buttons())]);
    let l = lay(&t, (300.0, 60.0));
    let go = &l.children[0].children[0];
    assert_eq!(go.w, 2.0 * 7.0 + 28.0, "its natural width");
    assert_eq!(go.x, ((300.0 - go.w) / 2.0).round());
}

/// A window's least width is its widest fixed line of nodes, and its natural size what its nodes
/// ask for; a body narrower than the least lays its nodes out at the least.
/// Behaviour: plugins.ui.a-window-is-never-narrower-than-its-nodes-least-width
#[test]
fn a_windows_least_width_is_its_widest_fixed_row() {
    let t = window(vec![
        node("row").with(
            "children",
            vec![
                node("button").with("text", "One"),
                node("button").with("text", "Two"),
            ],
        ),
        node("list").with("rows", rows(3, "a line of the log")),
    ]);
    let m = Metrics::reference();
    let w = want(&t, &m, &Cells(m.cell));
    assert_eq!(w.least().0, 2.0 * (3.0 * 7.0 + 28.0) + 6.0);
    let l = lay(&t, (40.0, 300.0));
    assert_eq!(l.children[0].w, w.least().0, "laid out at the least");
}

/// The tree as text: kind, id, place and size, a list's columns and rows, a labelled control's
/// label, and the height a scrolling body scrolls through.
/// Behaviour: plugins.ui.the-layout-as-text-shows-each-nodes-place-and-size
#[test]
fn the_layout_as_text_shows_each_nodes_place_and_size() {
    let t = UiNode::window_from_value(
        &node("window")
            .with("id", "main")
            .with("title", "T")
            .with("resizable", true)
            .with(
                "children",
                vec![
                    node("number").with("id", "n").with("text", "N"),
                    node("list")
                        .with("id", "log")
                        .with("columns", vec![Value::from("Line")])
                        .with("rows", rows(2, "r")),
                ],
            ),
    )
    .expect("valid");
    assert_eq!(
        reference_dump(&t, Some((30, 10))),
        "window #main 210x160\n  number #n 0,0 210x22 label=14\n  list #log 0,25 210x135 columns=200 rows=6\n"
    );
    assert_eq!(
        reference_dump(&t, Some((30, 2))),
        "window #main 210x32 scrolls=83\n  number #n 0,0 199x22 label=14\n  list #log 0,25 199x58 columns=189 rows=2\n"
    );
}

/// With tabs, a window too short for the chosen page keeps its row of tabs where it is and
/// scrolls the page alone: the body does not scroll, and the page's nodes are laid out at their
/// least height, as wide as the page less its scrollbar's room.
/// Behaviour: plugins.ui.with-tabs-only-the-pages-content-scrolls
#[test]
fn with_tabs_a_short_window_scrolls_the_page_under_its_row_of_tabs() {
    let buttons: Vec<Value> = (0..10)
        .map(|i| node("button").with("text", format!("B{i}")))
        .collect();
    let t = window(vec![
        node("label").with("text", "Status"),
        node("tabs").with(
            "children",
            vec![
                node("group").with("children", buttons),
                node("group").with("children", vec![node("label").with("text", "x")]),
            ],
        ),
    ]);
    let l = lay(&t, (200.0, 150.0));
    assert!(l.scroll.is_none(), "the body does not scroll");
    let tabs = &l.children[1];
    assert_eq!(tabs.y + tabs.h, 150.0, "the tabs take the rest of the body");
    let page = &tabs.children[0];
    let least = 10.0 * 22.0 + 9.0 * 3.0;
    assert_eq!(page.y, 25.0, "the page starts under the row of tabs");
    assert_eq!(
        page.scroll,
        Some(least),
        "the page scrolls through its nodes"
    );
    assert_eq!(
        (page.w, page.h),
        (200.0, tabs.h - 25.0),
        "the page keeps its room"
    );
    assert_eq!(
        page.children[0].w,
        200.0 - 11.0,
        "less the scrollbar's room"
    );
    let tall = lay(&t, (200.0, 400.0));
    assert!(
        tall.children[1].children[0].scroll.is_none(),
        "with room for it, the page does not scroll"
    );
}
