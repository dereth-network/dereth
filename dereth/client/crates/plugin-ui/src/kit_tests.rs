//! Plugin windows drawn with the widget kit, driven frame by frame with no art, no device and no
//! host: what the player's presses, drags, wheel and typing on them hand back.
//!
//! Behaviour: none (this client's own drawing of the declared tree; the conformance suite pins
//! the tree and its events, and the registry rows name the stations that prove each control)

use std::sync::Arc;

use dereth_plugin_api::{UiNode, Value};
use dereth_ui_kit::art::Art;
use dereth_ui_kit::colours::Colours;
use dereth_ui_kit::draw::Rect;
use dereth_ui_kit::input::{vk, InputFrame};
use dereth_ui_kit::widgets::Ctx;

use crate::{BarItem, Drawn, Input, Layer, Scene, Tier, WindowView};

/// A node of `kind`.
pub(crate) fn node(kind: &str) -> Value {
    Value::map().with("kind", kind)
}

/// A window `id` holding `children`.
pub(crate) fn window(id: &str, children: Vec<Value>) -> UiNode {
    UiNode::window_from_value(
        &node("window")
            .with("id", id)
            .with("title", id)
            .with("children", children),
    )
    .expect("a valid window")
}

pub(crate) fn view(tree: UiNode, revision: u64) -> WindowView {
    WindowView {
        tier: Tier::Plugin,
        plugin: "test.kit".to_owned(),
        window: tree.id.clone().unwrap_or_default(),
        tree: Arc::new(tree),
        open: true,
        revision,
        images: Vec::new(),
        size: None,
        by_player: true,
    }
}

/// A PNG file of `w` by `h` pixels, all of `rgba`.
pub(crate) fn png_file(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().expect("a header");
        let data: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
        wr.write_image_data(&data).expect("the pixels");
    }
    out
}

/// The layer on a 1280 by 720 screen (unless a test says another) with no art behind it.
pub(crate) struct Harness {
    pub(crate) layer: Layer,
    art: Art,
    pub(crate) input: InputFrame,
    /// The screen's size in pixels, and the interface's scale.
    pub(crate) screen: (f32, f32),
    pub(crate) scale: f32,
    /// Where the player left the bar.
    pub(crate) bar_at: crate::BarPlace,
    time: f64,
    /// The host's question up, if any.
    pub(crate) question: Option<dereth_client_contract::host_question::HostQuestion>,
    /// The interface's own controls under the layer.
    pub(crate) game_controls: Vec<dereth_ui_kit::draw::Rect>,
    /// The player has unlocked the HUDs.
    pub(crate) huds_unlocked: bool,
}

impl Harness {
    pub(crate) fn new() -> Self {
        Self::with_art(Art::empty())
    }

    /// The layer drawn in `art`.
    pub(crate) fn with_art(art: Art) -> Self {
        Self {
            layer: Layer::new(),
            art,
            input: InputFrame::default(),
            screen: (1280.0, 720.0),
            scale: 1.0,
            bar_at: crate::BarPlace {
                at: (8.0, 8.0),
                width: None,
            },
            time: 10.0,
            question: None,
            game_controls: Vec::new(),
            huds_unlocked: false,
        }
    }

    pub(crate) fn art(&self) -> &Art {
        &self.art
    }

    /// One frame of `views`; what the player did.
    pub(crate) fn frame(&mut self, views: &[WindowView]) -> Vec<Input> {
        self.frame_with(views, &[])
    }

    pub(crate) fn frame_with(&mut self, views: &[WindowView], bar: &[BarItem]) -> Vec<Input> {
        self.frame_all(views, bar, &[])
    }

    /// One frame of `views`, the bar's `bar` and `marks` (on the radar, the map, and as labels in
    /// the world); what the player did.
    pub(crate) fn frame_all(
        &mut self,
        views: &[WindowView],
        bar: &[BarItem],
        marks: &[crate::Glyph],
    ) -> Vec<Input> {
        self.time += 1.0 / 60.0;
        let colours = Colours::default();
        let mut ctx = Ctx {
            time: self.time,
            dt: 1.0 / 60.0,
            input: &mut self.input,
            colours: &colours,
            hot: false,
        };
        self.layer.frame(
            &self.art,
            self.scale,
            self.screen,
            &mut ctx,
            Scene {
                views,
                bar,
                bar_at: self.bar_at,
                marks,
                labels: marks,
                question: self.question.as_ref(),
                bar_look: crate::BarLook::Plain,
                game_controls: &self.game_controls,
                huds_unlocked: self.huds_unlocked,
            },
        );
        self.input.next_frame();
        self.layer.ui.take_inputs()
    }

    pub(crate) fn drawn(&self) -> &Drawn {
        self.layer.drawn()
    }

    /// The window's place on screen, as last drawn.
    pub(crate) fn rect(&self, window: &str) -> Rect {
        self.layer
            .ui
            .rect_of("test.kit", window)
            .expect("the window is drawn")
    }

    pub(crate) fn click(&mut self, views: &[WindowView], x: f32, y: f32) -> Vec<Input> {
        self.input.mouse = (x, y);
        self.input.down[0] = true;
        self.input.pressed[0] = true;
        let mut out = self.frame(views);
        self.input.down[0] = false;
        self.input.released[0] = true;
        out.extend(self.frame(views));
        out
    }
}

/// Every control noted in the frame just drawn, top to bottom.
pub(crate) fn controls(h: &Harness) -> Vec<Rect> {
    let mut r: Vec<Rect> = h.input.nav.last.targets.iter().map(|t| t.rect).collect();
    r.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    r
}

/// A list longer than it shows scrolls under the wheel, and a click on a row scrolled into
/// sight chooses that row, counted from the list's first.
#[test]
fn a_long_list_scrolls_and_a_row_scrolled_into_sight_is_the_one_chosen() {
    let rows: Vec<Value> = (1..=40)
        .map(|i| Value::List(vec![Value::from(format!("row {i}"))]))
        .collect();
    let views = [view(
        window(
            "main",
            vec![node("list")
                .with("id", "log")
                .with("rows", rows)
                .with("height", 5_i64)],
        ),
        1,
    )];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let rows = controls(&h);
    let first = rows.first().copied().expect("rows are noted");
    let row_h = rows[1].y - rows[0].y;
    assert!(row_h > 0.0);
    // Ten notches down over the list.
    for _ in 0..10 {
        h.input.mouse = (first.x + 4.0, first.y + 4.0);
        h.input.wheel = -1.0;
        h.frame(&views);
    }
    h.frame(&views);
    let shown = controls(&h);
    let top_row = shown
        .iter()
        .find(|r| r.y >= first.y - 0.5)
        .copied()
        .expect("a row in sight");
    let out = h.click(&views, top_row.x + 4.0, top_row.y + row_h / 2.0);
    let Some(Input::Select { row, node, .. }) = out.first() else {
        panic!("a row is chosen: {out:?}");
    };
    assert_eq!(node, "log");
    assert!(
        *row > 5,
        "a row past the first page is chosen, not row {row}"
    );
}

/// A choice opens its list of options on a click, and a click on an option chooses that
/// option: not the next one after the value, as a cycling button would.
#[test]
fn a_choice_opens_its_options_and_the_one_clicked_is_chosen() {
    let views = [view(
        window(
            "main",
            vec![node("choice")
                .with("id", "mode")
                .with("text", "Mode")
                .with(
                    "options",
                    vec![Value::from("stop"), Value::from("walk"), Value::from("run")],
                )
                .with("value", "stop")],
        ),
        1,
    )];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let field = controls(&h)[0];
    let out = h.click(&views, field.x + field.w / 2.0, field.y + field.h / 2.0);
    assert!(out.is_empty(), "opening the list chooses nothing: {out:?}");
    assert!(h.layer.ui.choosing(), "the list is open");
    // The options' rows stand under the box, one a row: the third is "run".
    let rows: Vec<Rect> = controls(&h)
        .into_iter()
        .filter(|r| r.y > field.bottom())
        .collect();
    assert_eq!(rows.len(), 3, "a row for each option: {rows:?}");
    let out = h.click(&views, rows[2].x + 4.0, rows[2].y + rows[2].h / 2.0);
    assert_eq!(
        out,
        vec![Input::Set {
            plugin: "test.kit".into(),
            window: "main".into(),
            node: "mode".into(),
            value: Value::from("run"),
        }]
    );
    assert!(!h.layer.ui.choosing(), "the list closes on a choice");
    // Escape closes an open list and chooses nothing.
    h.click(&views, field.x + field.w / 2.0, field.y + field.h / 2.0);
    assert!(h.layer.ui.choosing());
    h.input.keys.push(vk::ESCAPE);
    let out = h.frame(&views);
    assert!(out.is_empty() && !h.layer.ui.choosing());
}

/// A slider's knob is dragged: the value follows the pointer in its steps, held between its
/// least and its most however far the pointer goes.
#[test]
fn a_slider_is_dragged_in_its_steps_between_its_least_and_most() {
    let views = [view(
        window(
            "main",
            vec![node("slider")
                .with("id", "range")
                .with("min", 0_i64)
                .with("max", 100_i64)
                .with("step", 10_i64)
                .with("value", 0_i64)],
        ),
        1,
    )];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let track = h.input.nav.last.targets[0].rect;
    h.input.mouse = (track.x + 8.0, track.y + track.h / 2.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&views);
    let mut seen = Vec::new();
    // Dragged over a third of the way along, between two steps, then past the end.
    for x in [track.x + track.w * 0.37, track.right() + 400.0] {
        h.input.mouse.0 = x;
        for i in h.frame(&views) {
            if let Input::Set { value, .. } = i {
                seen.push(value);
            }
        }
    }
    h.input.down[0] = false;
    h.frame(&views);
    assert!(
        seen.iter().all(|v| v.as_i64().is_some_and(|n| n % 10 == 0)),
        "every value on a step: {seen:?}"
    );
    assert_eq!(
        seen.last(),
        Some(&Value::Int(100)),
        "held at its most: {seen:?}"
    );
    assert!(
        seen.iter().any(|v| matches!(v.as_i64(), Some(30..=50))),
        "{seen:?}"
    );
}

/// A number steps by its step, and a button and a toggle report their clicks.
#[test]
fn a_number_steps_and_a_button_and_a_toggle_report_their_clicks() {
    let views = [view(
        window(
            "main",
            vec![
                node("number")
                    .with("id", "n")
                    .with("value", 4_i64)
                    .with("step", 2_i64)
                    .with("max", 5_i64),
                node("button").with("id", "go").with("text", "Go"),
                node("toggle").with("id", "auto").with("value", true),
            ],
        ),
        1,
    )];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let c = controls(&h);
    // The number's two arrows share a line; the button is under them, the toggle under that.
    let (down, up) = (c[0], c[1]);
    let out = h.click(&views, up.x + 2.0, up.y + 2.0);
    assert_eq!(
        out,
        vec![Input::Set {
            plugin: "test.kit".into(),
            window: "main".into(),
            node: "n".into(),
            value: Value::Int(5),
        }],
        "a step up held at the most"
    );
    let out = h.click(&views, down.x + 2.0, down.y + 2.0);
    assert!(
        matches!(
            &out[..],
            [Input::Set {
                value: Value::Int(2),
                ..
            }]
        ),
        "{out:?}"
    );
    let go = c[2];
    let out = h.click(&views, go.x + 2.0, go.y + 2.0);
    assert!(
        matches!(&out[..], [Input::Click { node, .. }] if node == "go"),
        "{out:?}"
    );
    let tick = c[3];
    let out = h.click(&views, tick.x + 2.0, tick.y + 2.0);
    assert!(
        matches!(&out[..], [Input::Toggle { value: false, .. }]),
        "{out:?}"
    );
}

/// A line takes the keyboard on a click, and what is typed in it reaches the plugin as the
/// line's whole text.
#[test]
fn what_is_typed_in_a_line_reaches_the_plugin_as_its_whole_text() {
    let views = [view(
        window(
            "main",
            vec![node("input").with("id", "name").with("value", "Ho")],
        ),
        1,
    )];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let field = controls(&h)[0];
    h.click(&views, field.right() - 4.0, field.y + field.h / 2.0);
    assert!(h.layer.ui.typing(), "the line has the keyboard");
    h.input.keys.push(vk::END);
    h.input.chars.extend("lt".chars());
    let out = h.frame(&views);
    assert_eq!(
        out.last(),
        Some(&Input::Set {
            plugin: "test.kit".into(),
            window: "main".into(),
            node: "name".into(),
            value: Value::from("Holt"),
        })
    );
}

/// A press on a window under another brings it to the front, and a press where two windows
/// overlap reaches only the one in front.
#[test]
fn a_press_lands_on_the_window_in_front_and_brings_a_window_forward() {
    let make = |id: &str| {
        view(
            window(
                id,
                vec![node("button").with("id", "b").with("text", "Press")],
            ),
            1,
        )
    };
    let views = [make("one"), make("two")];
    let mut h = Harness::new();
    h.layer.ui.place_hint("test.kit", "one", 100.0, 100.0);
    h.layer.ui.place_hint("test.kit", "two", 110.0, 110.0);
    h.frame(&views);
    h.frame(&views);
    assert_eq!(
        h.layer.stacking(),
        [
            ("test.kit".to_owned(), "one".to_owned()),
            ("test.kit".to_owned(), "two".to_owned())
        ]
    );
    // The button of the window behind, where the front one covers it.
    let (one, two) = (h.rect("one"), h.rect("two"));
    assert!(one.intersect(&two).is_some());
    let buttons: Vec<Rect> = controls(&h);
    let covered = buttons
        .iter()
        .find(|b| one.contains(b.x, b.y) && two.contains(b.x + 2.0, b.y + 2.0))
        .copied();
    if let Some(b) = covered {
        let out = h.click(&views, b.x + 2.0, b.y + 2.0);
        assert!(
            out.iter().all(|i| i.window() == Some(("test.kit", "two"))),
            "only the window in front hears it: {out:?}"
        );
    }
    // A press on the part of the back window nothing covers brings it to the front.
    h.click(&views, one.x + 3.0, one.y + 3.0);
    assert_eq!(h.layer.stacking().last().map(|k| k.1.as_str()), Some("one"));
    assert_eq!(
        h.drawn().windows.last().map(|w| w.0 .1.as_str()),
        Some("one")
    );
}

/// The plugin bar has a button for each plugin, and a click on one asks for its window.
#[test]
fn a_click_on_the_bar_asks_for_the_plugin_s_window() {
    let bar = [BarItem {
        plugin: "test.kit".into(),
        name: "Kit".into(),
        state: "running".into(),
        open: false,
    }];
    let mut h = Harness::new();
    h.frame_with(&[], &bar);
    h.frame_with(&[], &bar);
    let b = controls(&h)[0];
    h.input.mouse = (b.x + 2.0, b.y + 2.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    let out = h.frame_with(&[], &bar);
    assert_eq!(
        out,
        vec![Input::Bar {
            plugin: "test.kit".into()
        }]
    );
    assert!(
        h.layer.hits(b.x + 2.0, b.y + 2.0),
        "the bar is a place a press is the layer's"
    );
}

/// A HUD's controls take the pointer and the rest of its plate lets it through.
#[test]
fn a_hud_s_controls_are_the_layer_s_and_its_plate_is_not() {
    let hud = UiNode::window_from_value(
        &node("hud")
            .with("id", "hud")
            .with("anchor", "top_right")
            .with(
                "children",
                vec![
                    node("label").with("text", "a long line of plain words"),
                    node("button").with("id", "b").with("text", "B"),
                ],
            ),
    )
    .expect("a hud");
    let views = [view(hud, 1)];
    let mut h = Harness::new();
    h.frame(&views);
    h.frame(&views);
    let b = controls(&h)[0];
    assert!(b.x > 640.0, "at the top right");
    assert!(h.layer.hits(b.x + 2.0, b.y + 2.0));
    let plate = h.layer.ui.rect_of("test.kit", "hud").expect("drawn");
    assert!(
        !h.layer.hits(plate.x + 2.0, plate.y + 2.0),
        "the plate where nothing answers is not the layer's"
    );
}

/// A window's pictures are made one a frame, each image showing its caption until its turn; a
/// picture is told from another by the hash it was given as it arrived, its file not read again
/// while that hash stands; and one over 1,024 pixels a side is never made.
#[test]
fn a_windows_pictures_are_made_one_a_frame_and_known_by_their_hash() {
    let image = |src: &str| {
        node("image")
            .with("src", src)
            .with("text", "waiting")
            .with("width", 4_i64)
            .with("height", 2_i64)
    };
    let tree = window(
        "pics",
        vec![image("a.png"), image("b.png"), image("wide.png")],
    );
    let mut v = view(tree, 1);
    let picture = |src: &str, bytes: Vec<u8>, hash: u64| crate::Picture {
        src: src.into(),
        bytes: std::sync::Arc::from(bytes),
        hash,
    };
    v.images = vec![
        picture("a.png", png_file(4, 4, [255, 0, 0, 255]), 1),
        picture("b.png", png_file(4, 4, [0, 255, 0, 255]), 2),
        picture("wide.png", png_file(1025, 1, [0, 0, 255, 255]), 3),
    ];
    let pictures = |h: &Harness| {
        h.drawn()
            .windows
            .iter()
            .flat_map(|w| w.2.quads.iter())
            .filter(|q| q.tex.is_some_and(|t| h.art().image(t).is_some()))
            .count()
    };
    let mut h = Harness::new();
    h.frame(std::slice::from_ref(&v));
    assert_eq!(
        pictures(&h),
        1,
        "one made the first frame, the others waiting"
    );
    h.frame(std::slice::from_ref(&v));
    assert_eq!(pictures(&h), 2, "the second the next");
    for _ in 0..3 {
        h.frame(std::slice::from_ref(&v));
    }
    assert_eq!(pictures(&h), 2, "the one over 1,024 a side never");
    // The same hash, whatever the bytes: the picture made stands.
    v.images[0] = picture("a.png", b"not even a picture".to_vec(), 1);
    h.frame(std::slice::from_ref(&v));
    assert_eq!(pictures(&h), 2, "known by its hash, not read again");
    // A new hash: made again, from the file it now is.
    v.images[0] = picture("a.png", b"not even a picture".to_vec(), 4);
    h.frame(std::slice::from_ref(&v));
    assert_eq!(
        pictures(&h),
        1,
        "a file that is no picture shows its caption"
    );
}

/// A window taller than the screen draws only what is in view: none of its meters past the
/// screen's bottom edge is drawn.
#[test]
fn a_window_draws_only_the_nodes_in_view() {
    let meters: Vec<Value> = (0..400)
        .map(|_| node("meter").with("value", 1_i64).with("max", 2_i64))
        .collect();
    let v = view(window("long", meters), 1);
    let mut h = Harness::new();
    h.frame(std::slice::from_ref(&v));
    h.frame(std::slice::from_ref(&v));
    let quads: Vec<_> = h
        .drawn()
        .windows
        .iter()
        .flat_map(|w| w.2.quads.iter())
        .collect();
    assert!(!quads.is_empty(), "the meters in view are drawn");
    assert!(
        quads.iter().all(|q| q.dst.y < 720.0),
        "{} quads past the screen's bottom",
        quads.iter().filter(|q| q.dst.y >= 720.0).count()
    );
}

/// A canvas's drawing is made again only when what it draws changes, not when something else in
/// its window does.
#[test]
fn a_canvas_is_drawn_again_only_when_its_drawing_changes() {
    let tree = |count: &str, x: i64| {
        window(
            "c",
            vec![
                node("label").with("id", "count").with("text", count),
                node("canvas").with("id", "cv").with(
                    "draw",
                    vec![Value::map()
                        .with("op", "rect")
                        .with("x", x)
                        .with("y", 4_i64)
                        .with("w", 20_i64)
                        .with("h", 10_i64)
                        .with("color", 0x00FF_0000_i64)
                        .with("fill", true)],
                ),
            ],
        )
    };
    let mut h = Harness::new();
    h.frame(&[view(tree("0", 4), 1)]);
    assert_eq!(h.layer.ui.drawings_made, 1);
    for r in 2..10 {
        h.frame(&[view(tree(&r.to_string(), 4), r)]);
    }
    assert_eq!(
        h.layer.ui.drawings_made, 1,
        "the label changing leaves it be"
    );
    h.frame(&[view(tree("10", 8), 10)]);
    assert_eq!(
        h.layer.ui.drawings_made, 2,
        "its own drawing changing makes it again"
    );
}
