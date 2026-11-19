//! Behaviour: none (the drawing both games' own world maps share, not a behaviour of either retail client)

use super::build::{OffFrame, Steps};
use super::*;

/// A picture whose pixel `(x, y)` is `(x, y, x + y)` in red, green and blue.
struct Ramp(usize, usize);

impl Rows for Ramp {
    fn size(&self) -> (usize, usize) {
        (self.0, self.1)
    }

    fn draw_row(&self, row: usize, first: usize, out: &mut [u8]) {
        for (n, px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x = first + n;
            let byte = |v: usize| u8::try_from(v % 256).unwrap_or(0);
            *px = [byte(x + row), byte(row), byte(x), 255];
        }
    }
}

#[test]
fn flat_ground_is_lit_one_a_slope_facing_the_north_west_more_and_one_facing_away_less() {
    assert!((lit(0.0, 0.0, 2.0) - 1.0).abs() < 1e-6);
    // Rising to the south-east, the ground faces the north-west.
    let facing = lit(0.2, -0.2, 2.0);
    let away = lit(-0.2, 0.2, 2.0);
    assert!(facing > 1.05 && away < 0.95, "{facing} {away}");
    // Steeper drawn, the same slope reads more.
    assert!(lit(0.2, -0.2, 4.0) > facing);
    assert!(
        (lit(-50.0, 50.0, 2.0) - 0.45).abs() < 1e-6,
        "held at the darkest"
    );
    assert!(lit(50.0, -50.0, 2.0) <= 1.45);
}

#[test]
fn a_grid_s_light_is_its_slope_across_each_vertex_s_neighbors() {
    // Rising a meter a vertex to the east, ten meters apart: a slope of a tenth, facing west.
    let light = relief_light((5, 3), 10.0, 2.0, |x, _| {
        #[allow(clippy::cast_precision_loss)]
        let h = x as f32;
        h
    });
    assert_eq!(light.len(), 15);
    for (i, l) in light.iter().enumerate() {
        assert!((l - lit(0.1, 0.0, 2.0)).abs() < 1e-6, "vertex {i}: {l}");
    }
    // A single peak: lit on its west and north sides, shaded on its east and south.
    let peak = relief_light(
        (3, 3),
        10.0,
        2.0,
        |x, y| if (x, y) == (1, 1) { 5.0 } else { 0.0 },
    );
    let at = |x: usize, y: usize| peak[y * 3 + x];
    assert!(at(0, 1) > 1.0 && at(1, 2) > 1.0, "{peak:?}");
    assert!(at(2, 1) < 1.0 && at(1, 0) < 1.0, "{peak:?}");
    assert!((at(1, 1) - 1.0).abs() < 1e-6, "its top is level");
}

#[test]
fn the_distance_from_a_source_counts_steps_and_diagonals_up_to_the_cap() {
    let d = distance_from((5, 5), 3.0, |i| i == 12);
    let at = |x: usize, y: usize| d[y * 5 + x];
    assert!(at(2, 2).abs() < 1e-6);
    assert!((at(3, 2) - 1.0).abs() < 1e-6);
    assert!((at(3, 3) - std::f32::consts::SQRT_2).abs() < 1e-6);
    assert!((at(4, 2) - 2.0).abs() < 1e-6);
    assert!((at(4, 4) - 2.0 * std::f32::consts::SQRT_2).abs() < 1e-6);
    let none = distance_from((4, 4), 3.0, |_| false);
    assert!(none.iter().all(|v| (v - 3.0).abs() < 1e-6), "no source");
}

#[test]
fn a_window_is_the_picture_s_pixels_there_cut_short_at_its_edges() {
    let pic = Ramp(300, 200);
    let w = window(&pic, (10, 20), (4, 3));
    assert_eq!((w.width, w.height), (4, 3));
    assert_eq!(&w.bgra[..4], &[30, 20, 10, 255]);
    let row2 = &w.bgra[2 * 16..2 * 16 + 4];
    assert_eq!(row2, &[32, 22, 10, 255]);
    let edge = window(&pic, (290, 190), (64, 64));
    assert_eq!((edge.width, edge.height), (10, 10));
}

#[test]
fn a_picture_shrunk_is_the_mean_of_what_each_pixel_covers_alike_in_bands_or_whole() {
    let pic = Ramp(200, 200);
    let half = shrink(&pic, (100, 100));
    assert_eq!((half.width, half.height), (100, 100));
    // Pixel (10, 20) covers (20..22, 40..42): red 20.5, green 40.5, blue 61.
    let at = (20 * 100 + 10) * 4;
    let rgb = [half.bgra[at + 2], half.bgra[at + 1], half.bgra[at]];
    assert!(
        rgb[0].abs_diff(21) <= 1 && rgb[1].abs_diff(41) <= 1 && rgb[2] == 61,
        "{rgb:?}"
    );
    let odd = shrink(&pic, (64, 48));
    let (across, down) = (spans(200, 64), spans(200, 48));
    let mut band = vec![0; 16 * 64 * 4];
    shrink_rows(&pic, &across, &down, 16, &mut band);
    assert_eq!(
        band,
        odd.bgra[16 * 64 * 4..32 * 64 * 4],
        "a band alone as the whole draws it"
    );
    // Every source pixel's share sums to the whole, on either axis.
    for s in [&across, &down] {
        for span in s.iter() {
            let total: f64 = span.iter().map(|(_, part)| part).sum();
            assert!((total - 1.0).abs() < 1e-9);
        }
    }
}

/// Counts to `n`, a step at a time, each step first waiting for leave to go on.
struct Count {
    at: u32,
    n: u32,
    go: std::sync::mpsc::Receiver<()>,
}

impl Steps for Count {
    type Done = u32;

    fn step(&mut self) -> Option<Result<u32, String>> {
        if self.go.recv().is_err() {
            return Some(Err("let go".to_owned()));
        }
        self.at += 1;
        (self.at == self.n).then_some(Ok(self.at))
    }
}

#[test]
fn a_build_runs_off_the_frame_and_its_end_is_taken_once_it_comes() {
    let (go, rx) = std::sync::mpsc::channel();
    let mut build = OffFrame::start(
        "count",
        Count {
            at: 0,
            n: 3,
            go: rx,
        },
    );
    assert!(build.poll().is_none(), "not done while it waits");
    for _ in 0..3 {
        go.send(()).expect("the build listens");
    }
    let done = loop {
        if let Some(done) = build.poll() {
            break done;
        }
        std::thread::yield_now();
    };
    assert_eq!(done, Ok(3));
    let (go, rx) = std::sync::mpsc::channel();
    let build = OffFrame::start(
        "count",
        Count {
            at: 0,
            n: 2,
            go: rx,
        },
    );
    drop(go);
    assert_eq!(
        build.wait(),
        Err("let go".to_owned()),
        "a build that cannot go on says why"
    );
}
