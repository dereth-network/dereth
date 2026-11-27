//! The game's date and time of day on the map window, placed and worded as the other interfaces'
//! map windows show them, in two rows under the picture with the coordinates and the zoom buttons;
//! the radar shows neither.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_horizon::art::Family;
use dereth_horizon::draw::{Quad, Rect};
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

/// The least space between the coordinates' ink and the top of the zoom buttons under them, and
/// between the time's ink and the first button, in pixels at 1920 by 1080.
const BUTTON_ROOM: f32 = 8.0;

/// A line of text as drawn: what it reads (spaces draw nothing, so they are not read) and the ink
/// of each of its letters (the texels not wholly clear), left to right, in screen pixels.
#[derive(Debug)]
struct Line {
    text: String,
    glyphs: Vec<Rect>,
}

impl Line {
    fn left(&self) -> f32 {
        self.glyphs.iter().map(|r| r.x).fold(f32::MAX, f32::min)
    }

    fn right(&self) -> f32 {
        self.glyphs.iter().map(Rect::right).fold(f32::MIN, f32::max)
    }

    fn bottom(&self) -> f32 {
        self.glyphs
            .iter()
            .map(Rect::bottom)
            .fold(f32::MIN, f32::max)
    }

    /// Halfway down the ink of its first letter, a capital: the middle of the line as it is read.
    fn middle(&self) -> f32 {
        self.glyphs.first().map_or(0.0, |r| r.y + r.h / 2.0)
    }

    /// The bottom of the ink of its first letter, a capital: where the line stands.
    fn baseline(&self) -> f32 {
        self.glyphs.first().map_or(0.0, Rect::bottom)
    }
}

/// The ink of quad `q`'s picture, the texels of its part not wholly clear, where it lands on
/// screen.
fn ink(h: &Harness, q: &Quad) -> Option<Rect> {
    let image = h.ui.art.image(q.tex?)?;
    let w = image.width as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, x1, y1) = (
        q.src.x as usize,
        q.src.y as usize,
        q.src.right() as usize,
        q.src.bottom() as usize,
    );
    let mut span: Option<(usize, usize, usize, usize)> = None;
    for y in y0..y1 {
        for x in x0..x1 {
            if image.bgra[(y * w + x) * 4 + 3] > 0 {
                let (l, t, r, b) = span.unwrap_or((x, y, x + 1, y + 1));
                span = Some((l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)));
            }
        }
    }
    let (l, t, r, b) = span?;
    let k = q.dst.w / q.src.w;
    #[allow(clippy::cast_precision_loss)]
    let at = |texel: usize, origin: f32, dst: f32| dst + (texel as f32 - origin) * k;
    #[allow(clippy::cast_precision_loss)]
    Some(Rect::new(
        at(l, q.src.x, q.dst.x),
        at(t, q.src.y, q.dst.y),
        (r - l) as f32 * k,
        (b - t) as f32 * k,
    ))
}

/// The lines drawn in Horizon's body face at `points` inside `within`, top to bottom, each read
/// left to right from its glyphs, one reading for each color drawn there.
fn lines_in(h: &Harness, points: f32, within: Rect) -> Vec<Line> {
    let face = h.ui.art.font(Family::Body, points).expect("the face");
    let letters: Vec<(Option<_>, Rect, char)> = (' '..='~')
        .filter_map(|ch| {
            let g = *face.font.glyph(ch)?;
            let src = Rect::new(
                f32::from(g.x),
                f32::from(g.y),
                f32::from(g.w),
                f32::from(g.h),
            );
            Some((h.ui.art.font_page(&face, g.page), src, ch))
        })
        .collect();
    let mut glyphs: Vec<(u32, Rect, Rect, char)> = h
        .list
        .quads
        .iter()
        .filter(|q| within.contains(q.dst.x, q.dst.y))
        .filter_map(|q| {
            let (_, _, ch) = letters
                .iter()
                .find(|(tex, src, _)| *tex == q.tex && *src == q.src)?;
            Some((q.colour, q.dst, ink(h, q)?, *ch))
        })
        .collect();
    glyphs.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    // Rows by their color and their glyphs' bottoms, a few pixels apart at most within a line;
    // a line ends where a gap wider than a few letters opens.
    let mut rows: Vec<(u32, f32, f32, Line)> = Vec::new();
    for (colour, r, inked, ch) in glyphs {
        let bottom = r.bottom();
        match rows
            .iter_mut()
            .find(|(c, b, end, _)| *c == colour && (b - bottom).abs() < 6.0 && r.x - *end < 30.0)
        {
            Some((_, _, end, line)) => {
                line.text.push(ch);
                line.glyphs.push(inked);
                *end = r.right();
            }
            None => rows.push((
                colour,
                bottom,
                r.right(),
                Line {
                    text: ch.to_string(),
                    glyphs: vec![inked],
                },
            )),
        }
    }
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    rows.into_iter().map(|(_, _, _, line)| line).collect()
}

fn unspaced(s: &str) -> String {
    s.chars().filter(|c| *c != ' ').collect()
}

fn find<'a>(lines: &'a [Line], want: &str) -> &'a Line {
    lines.iter().find(|l| l.text == want).unwrap_or_else(|| {
        let read: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        panic!("{want:?} is not drawn: {read:?}")
    })
}

/// With the map window open, under its picture: the date the game's clock gives on the left of
/// the first row and the coordinates on its right; the time of day on the left of the second row
/// and the zoom buttons on its right (the last, Reset), centered on the time's line and clear of
/// the coordinates and the time.
/// The date's and the time's ink start exactly at the picture's left edge, and the coordinates'
/// ink ends exactly at its right. `Date: ` and `Time: ` stand before them as the other interfaces'
/// map windows word them. Nothing of it is on the picture; the radar's corner of the screen shows
/// neither, and with no time from the game the map shows none.
#[test]
fn the_games_date_and_time_stand_under_the_map_with_the_coordinates_and_the_buttons() {
    let (date, time) = dereth_presentation::map::date_time_strings(
        "Morningthaw",
        14,
        10,
        "P.Y.",
        "Dawnsong-and-Half",
    );
    let (date_line, time_line) = (
        unspaced(&format!("Date: {date}")),
        unspaced(&format!("Time: {time}")),
    );
    let mut h = Harness::with_built_in_art();
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        coords: Some((42.1, -33.6)),
        game_time: Some((date.clone(), time.clone())),
        ..GameState::default()
    };
    let radar = Rect::new(1600.0, 0.0, 320.0, 400.0);
    let screen = Rect::new(0.0, 0.0, 1920.0, 1080.0);
    let says_when = |lines: &[Line]| {
        lines
            .iter()
            .any(|l| l.text.contains("Date:") || l.text.contains("Time:"))
    };
    h.frame(&state);
    for points in [12.0, 14.0] {
        assert!(
            !says_when(&lines_in(&h, points, screen)),
            "the date and time drawn with the map closed"
        );
    }

    let under = rows_hold(&mut h, &state, &date_line, &time_line);

    for points in [12.0, 14.0] {
        let lines = lines_in(&h, points, radar);
        assert!(
            !lines
                .iter()
                .any(|l| l.text.contains(&unspaced(&date)) || l.text.contains(&unspaced(&time))),
            "the radar shows the date or the time"
        );
    }

    let none = GameState {
        game_time: None,
        ..state
    };
    h.frame(&none);
    assert!(
        !says_when(&lines_in(&h, 14.0, under)),
        "no time, and still a date or a time"
    );
}

/// Opens the map window over `state` and holds its rows under the picture to their places (see
/// the tests below); the part of the window under the picture.
fn rows_hold(h: &mut Harness, state: &GameState, date_line: &str, time_line: &str) -> Rect {
    h.ui.windows.open(WindowId::Map, 0.0);
    h.frame(state);
    h.frame(state);
    let area = h.ui.windows.map_picture.expect("the map's picture");
    // The text is drawn at the window's scale on this screen.
    let k = h.ui.layout_scale(h.screen);
    let points = 14.0 * k;
    assert!(
        !lines_in(h, points, area)
            .iter()
            .any(|l| l.text.contains("Date:") || l.text.contains("Time:")),
        "the date or the time is on the picture"
    );
    let under = Rect::new(area.x - 8.0, area.bottom(), area.w + 16.0, 80.0);
    let lines = lines_in(h, points, under);
    let (date_at, time_at) = (find(&lines, date_line), find(&lines, time_line));
    let coords_at = lines
        .iter()
        .find(|l| l.text.contains("42.1N"))
        .expect("the coordinates under the picture");

    // The first row: the date on the left, the coordinates on the right, on one baseline; the
    // date's and the time's ink on the picture's left edge, the coordinates' on its right.
    assert!(
        date_at.baseline() < time_at.middle(),
        "the date, then the time"
    );
    assert_eq!(
        date_at.left(),
        area.x,
        "the date's ink and the picture's left edges"
    );
    assert_eq!(
        time_at.left(),
        date_at.left(),
        "the time's and the date's ink left edges"
    );
    assert_eq!(
        coords_at.baseline(),
        date_at.baseline(),
        "the coordinates' and the date's baselines"
    );
    assert_eq!(
        coords_at.right(),
        area.right(),
        "the coordinates' ink and the picture's right edges"
    );
    // Clear of the picture: the window's own margin between them.
    let first_top = date_at.glyphs[0].y;
    assert!(
        first_top - area.bottom() >= (8.0 * k).floor(),
        "the first row stands {} under the picture",
        first_top - area.bottom()
    );

    // The second row: the time on the left, the buttons on the right, centered on its line and
    // clear of the coordinates over them.
    let buttons = h.ui.windows.map_view.buttons.expect("the zoom buttons");
    assert!(
        (buttons[2].right() - area.right()).abs() <= 1.0,
        "the buttons' right edge at {} and the picture's at {}",
        buttons[2].right(),
        area.right()
    );
    for b in buttons {
        let (middle, line) = (b.y + b.h / 2.0, time_at.middle());
        assert!(
            (middle - line).abs() <= 1.0,
            "a button's middle at {middle} and the time's at {line}"
        );
        assert!(
            b.y - coords_at.bottom() >= (BUTTON_ROOM * k).floor(),
            "a button {} under the coordinates' ink",
            b.y - coords_at.bottom()
        );
    }

    // The last button reads Reset, whole and at the buttons' own size, inside it; the time's ink
    // ends clear of the first button.
    let reset = lines_in(h, points, buttons[2]);
    let label = find(&reset, "Reset");
    assert!(
        label.left() > buttons[2].x && label.right() < buttons[2].right(),
        "Reset's ink from {} to {} in a button from {} to {}",
        label.left(),
        label.right(),
        buttons[2].x,
        buttons[2].right()
    );
    assert!(
        buttons[0].x - time_at.right() >= (BUTTON_ROOM * k).floor(),
        "the time's ink ends {} before the buttons",
        buttons[0].x - time_at.right()
    );
    under
}

/// On the smallest screens the window opens on, the rows under the map keep their places: the
/// time clear of the buttons, Reset whole in its button.
#[test]
fn the_rows_under_the_map_keep_their_places_on_a_small_screen() {
    let (date, time) = dereth_presentation::map::date_time_strings(
        "Thistledown",
        30,
        10,
        "P.Y.",
        "Foredawn-and-Half",
    );
    let (date_line, time_line) = (
        unspaced(&format!("Date: {date}")),
        unspaced(&format!("Time: {time}")),
    );
    let state = GameState {
        in_world: true,
        name: "Tester".into(),
        coords: Some((42.1, -33.6)),
        game_time: Some((date, time)),
        ..GameState::default()
    };
    for screen in [(1024.0, 768.0), (800.0, 600.0)] {
        let mut h = Harness::with_built_in_art();
        h.screen = screen;
        h.frame(&state);
        rows_hold(&mut h, &state, &date_line, &time_line);
    }
}
