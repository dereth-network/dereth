//! The interface's own frame art drawn over boxes of every size, at every scale step: the frame of
//! a window, of a panel, of a well and of a field never reaches past the box it frames, however
//! small the box is beside the frame's corner pieces.
//!
//! Fixture: the art built into the client, drawn into a draw list with no device.

use std::sync::Arc;

use dereth_horizon::art::Art;
use dereth_horizon::draw::{DrawList, Rect};
use dereth_horizon::pieces::Pieces;
use dereth_horizon::ui::paint::Painter;
use dereth_ui_kit::colours::Colours;
use dereth_ui_kit::input::InputFrame;
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

/// The boxes framed: smaller than one corner piece, narrower than two, and roomy.
const BOXES: [(f32, f32); 6] = [
    (14.0, 10.0),
    (60.0, 22.0),
    (120.0, 60.0),
    (240.0, 32.0),
    (330.0, 200.0),
    (700.0, 500.0),
];

/// The interface's scale steps, and the shrunken scales of a small window.
const SCALES: [f32; 6] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0];

fn art() -> Art {
    Art::new(Arc::new(Pieces::built_in().expect("the art is built in")))
}

/// What `draw` puts in a fresh list at `scale`, over a large screen.
fn drawn(art: &Art, scale: f32, draw: impl FnOnce(&mut Painter<'_>)) -> DrawList {
    let mut list = DrawList::default();
    let mut p = Painter {
        list: &mut list,
        art,
        scale,
        screen: (8000.0, 8000.0),
        fade: 1.0,
    };
    draw(&mut p);
    list
}

/// Every quad of `list` that `r` does not hold, as it is shown (cut to its clip).
fn outside(list: &DrawList, r: Rect) -> Vec<Rect> {
    const SLACK: f32 = 0.01;
    list.quads
        .iter()
        .filter_map(|q| q.clip.map_or(Some(q.dst), |c| q.dst.intersect(&c)))
        .filter(|d| d.w > SLACK && d.h > SLACK)
        .filter(|d| {
            d.x < r.x - SLACK
                || d.y < r.y - SLACK
                || d.right() > r.right() + SLACK
                || d.bottom() > r.bottom() + SLACK
        })
        .collect()
}

/// A panel's frame, a well and a field stay inside the box they are drawn over, at every size and
/// scale: a corner piece wider than half the box keeps its outer part only.
#[test]
fn a_panel_a_well_and_a_field_never_draw_past_their_box() {
    let art = art();
    assert!(
        art.has_piece("window.plain.tl"),
        "the art has its own frames"
    );
    for scale in SCALES {
        for (w, h) in BOXES {
            let r = Rect::new(100.0, 100.0, w * scale, h * scale);
            for (what, list) in [
                ("panel", drawn(&art, scale, |p| kit::panel(p, r, 1.0))),
                ("well", drawn(&art, scale, |p| kit::well(p, r))),
                (
                    "field",
                    drawn(&art, scale, |p| kit::field_ground(p, r, false)),
                ),
            ] {
                assert!(!list.quads.is_empty(), "the {what} is drawn");
                let past = outside(&list, r);
                assert!(
                    past.is_empty(),
                    "a {what} over {w}x{h} at scale {scale} draws past its box: {past:?}"
                );
            }
        }
    }
}

/// A window's frame (its title bar, sides and corners) stays inside the window at every size
/// and scale, down to a window narrower than its two top corner pieces side by side.
#[test]
fn a_windows_frame_never_draws_past_the_window() {
    let art = art();
    let colours = Colours::default();
    for scale in SCALES {
        for (w, h) in [
            (120.0, 80.0),
            (200.0, 120.0),
            (330.0, 200.0),
            (700.0, 500.0),
        ] {
            let mut input = InputFrame::default();
            let mut ctx = Ctx {
                time: 10.0,
                dt: 1.0 / 60.0,
                input: &mut input,
                colours: &colours,
                hot: false,
            };
            let mut state = WindowState {
                open: true,
                pos: Some((100.0, 100.0)),
                opened_at: 0.0,
                ..WindowState::default()
            };
            let mut rect = Rect::default();
            let list = drawn(&art, scale, |p| {
                rect = kit::window(p, &mut ctx, &mut state, "Title", (w, h), (0.0, 0.0)).rect;
            });
            let past = outside(&list, rect);
            assert!(
                past.is_empty(),
                "a window of {w}x{h} at scale {scale} draws past itself ({rect:?}): {past:?}"
            );
        }
    }
}

/// Where a frame piece's border is opaque, read from its picture at scale 1: the first and last
/// opaque column (`across`) or row, in layout units from the piece's left or top.
fn opaque_span(name: &str, across: bool) -> (f32, f32) {
    let pieces = Pieces::built_in().expect("the art is built in");
    let manifest: serde_json::Value =
        serde_json::from_slice(pieces.get("manifest.json").expect("the manifest")).expect("JSON");
    let piece = &manifest["pieces"][name];
    let at: Vec<u32> = piece["at"]["1"]
        .as_array()
        .expect("placed at scale 1")
        .iter()
        .map(|v| u32::try_from(v.as_u64().expect("a number")).expect("small"))
        .collect();
    let size: Vec<f64> = piece["size"]
        .as_array()
        .expect("a size")
        .iter()
        .map(|v| v.as_f64().expect("a number"))
        .collect();
    let (atlas, x0, y0, w, h) = (at[0], at[1], at[2], at[3], at[4]);
    let file = format!("atlas-1-{atlas}.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(pieces.get(&file).expect("the atlas")))
        .read_info()
        .expect("a PNG");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("its pixels");
    assert_eq!(info.color_type, png::ColorType::Rgba, "the atlas is RGBA");
    let stride = info.line_size;
    let alpha = |x: u32, y: u32| buf[(y as usize) * stride + (x as usize) * 4 + 3];
    let (lines, length) = if across { (w, h) } else { (h, w) };
    let opaque: Vec<u32> = (0..lines)
        .filter(|&i| {
            let solid = (0..length)
                .filter(|&j| {
                    let (x, y) = if across {
                        (x0 + i, y0 + j)
                    } else {
                        (x0 + j, y0 + i)
                    };
                    alpha(x, y) > 128
                })
                .count();
            solid * 2 > length as usize
        })
        .collect();
    let per_unit = f64::from(if across { w } else { h }) / size[if across { 0 } else { 1 }];
    let first = f64::from(*opaque.first().expect("an opaque line")) / per_unit;
    let last = f64::from(*opaque.last().expect("an opaque line") + 1) / per_unit;
    #[allow(clippy::cast_possible_truncation)]
    (first as f32, last as f32)
}

/// A window's body keeps about as much room from the frame's border below it as at its sides: the
/// frame adds no dead space under what a window holds.
#[test]
fn a_windows_body_keeps_as_much_room_from_the_border_below_it_as_at_its_sides() {
    let pieces = Pieces::built_in().expect("the art is built in");
    let manifest: serde_json::Value =
        serde_json::from_slice(pieces.get("manifest.json").expect("the manifest")).expect("JSON");
    let piece_size = |name: &str, i: usize| {
        #[allow(clippy::cast_possible_truncation)]
        let v = manifest["pieces"][name]["size"][i]
            .as_f64()
            .expect("a size") as f32;
        v
    };
    let body: Vec<f32> = manifest["values"]["window.body"]
        .as_array()
        .expect("the body's inset")
        .iter()
        .map(|v| {
            #[allow(clippy::cast_possible_truncation)]
            let v = v.as_f64().expect("a number") as f32;
            v
        })
        .collect();
    // The border's inner edge, from each edge of the window: the left bar's last opaque column,
    // the right bar's first, the bottom bar's first opaque row (each piece stands at its edge).
    let left = opaque_span("window.left", true).1;
    let right = piece_size("window.right", 0) - opaque_span("window.right", true).0;
    let bottom = piece_size("window.bottom", 1) - opaque_span("window.bottom", false).0;
    let (gap_left, gap_right, gap_bottom) = (body[0] - left, body[2] - right, body[3] - bottom);
    assert!(
        gap_left > 0.0 && gap_right > 0.0 && gap_bottom > 0.0,
        "the body keeps clear of the border: {gap_left} {gap_right} {gap_bottom}"
    );
    assert!(
        gap_bottom <= gap_left.max(gap_right) + 1.0,
        "below the body {gap_bottom} units to the border, at its sides {gap_left} and {gap_right}"
    );
}
