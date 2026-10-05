//! A glyph outline turned into coverage the way the Windows font system draws a grey glyph: each
//! pixel's coverage is the number of points of an 8 by 8 grid, at the centres of the grid's cells,
//! that fall inside the outline by the nonzero winding rule. No smoothing filter, 65 levels.

use skrifa::outline::OutlinePen;

/// A point in pixels, y up.
type Point = (f64, f64);

/// One path command, in pixels with y up and the pen at the origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Command {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
    Close,
}

/// The commands a glyph is drawn with.
#[derive(Debug, Default)]
pub(crate) struct Recorder(pub(crate) Vec<Command>);

impl OutlinePen for Recorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::Move((x.into(), y.into())));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::Line((x.into(), y.into())));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.push(Command::Quad(
            (cx0.into(), cy0.into()),
            (x.into(), y.into()),
        ));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Command::Cubic(
            (cx0.into(), cy0.into()),
            (cx1.into(), cy1.into()),
            (x.into(), y.into()),
        ));
    }

    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}

/// The same glyph hinted at two em sizes, combined: the x of every point from `xs`, the y from
/// `ys`. `None` if the two are not the same path.
pub(crate) fn combine(xs: &[Command], ys: &[Command]) -> Option<Vec<Command>> {
    if xs.len() != ys.len() {
        return None;
    }
    let p = |a: Point, b: Point| (a.0, b.1);
    xs.iter()
        .zip(ys)
        .map(|(a, b)| {
            Some(match (*a, *b) {
                (Command::Move(a), Command::Move(b)) => Command::Move(p(a, b)),
                (Command::Line(a), Command::Line(b)) => Command::Line(p(a, b)),
                (Command::Quad(a0, a1), Command::Quad(b0, b1)) => {
                    Command::Quad(p(a0, b0), p(a1, b1))
                }
                (Command::Cubic(a0, a1, a2), Command::Cubic(b0, b1, b2)) => {
                    Command::Cubic(p(a0, b0), p(a1, b1), p(a2, b2))
                }
                (Command::Close, Command::Close) => Command::Close,
                _ => return None,
            })
        })
        .collect()
}

/// Every x of a path scaled by `k`.
pub(crate) fn stretch(commands: &[Command], k: f64) -> Vec<Command> {
    let s = |q: Point| (q.0 * k, q.1);
    commands
        .iter()
        .map(|c| match *c {
            Command::Move(a) => Command::Move(s(a)),
            Command::Line(a) => Command::Line(s(a)),
            Command::Quad(a, b) => Command::Quad(s(a), s(b)),
            Command::Cubic(a, b, e) => Command::Cubic(s(a), s(b), s(e)),
            Command::Close => Command::Close,
        })
        .collect()
}

/// A piece of an outline that only rises or only falls: a quadratic arc (a line is one whose
/// control point is its middle), with its winding direction.
#[derive(Debug, Clone, Copy)]
struct Edge {
    p0: Point,
    p1: Point,
    p2: Point,
    y_min: f64,
    y_max: f64,
    up: bool,
}

impl Edge {
    fn new(p0: Point, p1: Point, p2: Point) -> Option<Self> {
        // A level piece crosses no sample row.
        #[allow(clippy::float_cmp)]
        if p0.1 == p2.1 {
            return None;
        }
        Some(Self {
            p0,
            p1,
            p2,
            y_min: p0.1.min(p2.1),
            y_max: p0.1.max(p2.1),
            up: p2.1 > p0.1,
        })
    }

    /// Where the edge crosses the level `y`, for `y_min <= y < y_max`.
    fn x_at(&self, y: f64) -> f64 {
        let (y0, y1, y2) = (self.p0.1, self.p1.1, self.p2.1);
        let a = y0 - 2.0 * y1 + y2;
        let b = 2.0 * (y1 - y0);
        let c = y0 - y;
        let t = if a.abs() < 1e-12 {
            -c / b
        } else {
            let root = (b * b - 4.0 * a * c).max(0.0).sqrt();
            let t0 = (-b + root) / (2.0 * a);
            if (-1e-9..=1.0 + 1e-9).contains(&t0) {
                t0
            } else {
                (-b - root) / (2.0 * a)
            }
        }
        .clamp(0.0, 1.0);
        let u = 1.0 - t;
        u * u * self.p0.0 + 2.0 * u * t * self.p1.0 + t * t * self.p2.0
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn line(edges: &mut Vec<Edge>, a: Point, b: Point) {
    edges.extend(Edge::new(a, lerp(a, b, 0.5), b));
}

/// A quadratic arc, split where it turns in y so that each piece only rises or only falls.
fn quad(edges: &mut Vec<Edge>, p0: Point, p1: Point, p2: Point) {
    let denom = p0.1 - 2.0 * p1.1 + p2.1;
    let t = if denom.abs() > 1e-12 {
        (p0.1 - p1.1) / denom
    } else {
        -1.0
    };
    if t > 0.0 && t < 1.0 {
        let a = lerp(p0, p1, t);
        let b = lerp(p1, p2, t);
        let m = lerp(a, b, t);
        // Both halves are level where they meet, so their control points share the turn's y.
        edges.extend(Edge::new(p0, (a.0, m.1), m));
        edges.extend(Edge::new(m, (b.0, m.1), p2));
    } else {
        edges.extend(Edge::new(p0, p1, p2));
    }
}

/// A cubic arc, which TrueType outlines never hold, as sixteen lines.
fn cubic(edges: &mut Vec<Edge>, p: [Point; 4]) {
    let at = |t: f64| {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        (
            w.iter().zip(&p).map(|(w, q)| w * q.0).sum(),
            w.iter().zip(&p).map(|(w, q)| w * q.1).sum(),
        )
    };
    let mut previous = p[0];
    for i in 1..=16 {
        let next = at(f64::from(i) / 16.0);
        line(edges, previous, next);
        previous = next;
    }
}

/// The closed outline of a path: every contour is closed back to its start.
fn edges_of(commands: &[Command]) -> Vec<Edge> {
    let mut edges = Vec::new();
    let (mut start, mut pen) = ((0.0, 0.0), (0.0, 0.0));
    for command in commands {
        match *command {
            Command::Move(to) => {
                line(&mut edges, pen, start);
                (start, pen) = (to, to);
            }
            Command::Line(to) => {
                line(&mut edges, pen, to);
                pen = to;
            }
            Command::Quad(control, to) => {
                quad(&mut edges, pen, control, to);
                pen = to;
            }
            Command::Cubic(c0, c1, to) => {
                cubic(&mut edges, [pen, c0, c1, to]);
                pen = to;
            }
            Command::Close => {
                line(&mut edges, pen, start);
                pen = start;
            }
        }
    }
    line(&mut edges, pen, start);
    edges
}

/// Sample points per pixel along each axis.
const SAMPLES: i32 = 8;

/// For every pixel of a `size` by `size` box, the number of its sample points inside the path,
/// 0 to 64, rows top first. The box's top left corner is at `(left, top)` in the path's pixels,
/// y up. `None` when the path has no outline.
pub(crate) fn sample(commands: &[Command], left: i32, top: i32, size: i32) -> Option<Vec<u8>> {
    let edges = edges_of(commands);
    if edges.is_empty() {
        return None;
    }
    let mut counts = vec![0u8; usize::try_from(size * size).unwrap_or(0)];
    let step = 1.0 / f64::from(SAMPLES);
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for row in 0..size * SAMPLES {
        let y = f64::from(top) - (f64::from(row) + 0.5) * step;
        crossings.clear();
        crossings.extend(
            edges
                .iter()
                .filter(|e| e.y_min <= y && y < e.y_max)
                .map(|e| (e.x_at(y), if e.up { 1 } else { -1 })),
        );
        if crossings.is_empty() {
            continue;
        }
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let pixel_row = row / SAMPLES;
        let (mut next, mut winding) = (0, 0);
        for column in 0..size * SAMPLES {
            let x = f64::from(left) + (f64::from(column) + 0.5) * step;
            while let Some(&(at, direction)) = crossings.get(next) {
                if at > x {
                    break;
                }
                winding += direction;
                next += 1;
            }
            if winding != 0 {
                let at = usize::try_from(pixel_row * size + column / SAMPLES).unwrap_or(0);
                if let Some(c) = counts.get_mut(at) {
                    *c += 1;
                }
            }
        }
    }
    Some(counts)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the carried fonts' rasteriser; the Windows comparison is in tests/cpu).
    use super::*;

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Command> {
        vec![
            Command::Move((x0, y0)),
            Command::Line((x1, y0)),
            Command::Line((x1, y1)),
            Command::Line((x0, y1)),
            Command::Close,
        ]
    }

    #[test]
    fn a_pixel_counts_the_sample_points_its_outline_covers() {
        // A whole pixel, a pixel half covered from the left, and one covered three eighths of
        // its height.
        let counts = sample(&square(0.0, 0.0, 1.5, 1.0), 0, 1, 2).unwrap();
        assert_eq!(counts, [64, 32, 0, 0]);
        let counts = sample(&square(0.0, 0.0, 1.0, 0.375), 0, 1, 1).unwrap();
        assert_eq!(counts, [24]);
    }

    #[test]
    fn overlapping_contours_cover_once_and_a_reversed_contour_cuts_a_hole() {
        let mut both = square(0.0, 0.0, 2.0, 2.0);
        both.extend(square(0.0, 0.0, 2.0, 2.0));
        assert_eq!(sample(&both, 0, 2, 2).unwrap(), [64; 4]);
        let mut hole = square(0.0, 0.0, 3.0, 3.0);
        hole.extend([
            Command::Move((1.0, 1.0)),
            Command::Line((1.0, 2.0)),
            Command::Line((2.0, 2.0)),
            Command::Line((2.0, 1.0)),
            Command::Close,
        ]);
        let counts = sample(&hole, 0, 3, 3).unwrap();
        assert_eq!(counts[4], 0);
        assert_eq!(counts.iter().filter(|c| **c == 64).count(), 8);
    }

    #[test]
    fn a_quadratic_arc_is_sampled_on_its_curve_not_its_chord() {
        // An arch from (0,0) through (1,2) to (2,0): its top is at y 1, so the row from y 1 to 2
        // is empty and the row below is covered at its middle.
        let arch = vec![
            Command::Move((0.0, 0.0)),
            Command::Quad((1.0, 2.0), (2.0, 0.0)),
            Command::Close,
        ];
        let counts = sample(&arch, 0, 2, 2).unwrap();
        assert_eq!(&counts[..2], &[0, 0]);
        assert!(counts[2] > 32 && counts[3] > 32, "{counts:?}");
        assert!(counts[2] < 64);
    }

    #[test]
    fn two_hintings_combine_x_from_one_and_y_from_the_other() {
        let xs = square(0.0, 0.0, 3.0, 9.0);
        let ys = square(9.0, 0.0, 9.0, 2.0);
        assert_eq!(combine(&xs, &ys).unwrap(), square(0.0, 0.0, 3.0, 2.0));
        assert_eq!(combine(&xs, &ys[1..]), None);
        assert_eq!(stretch(&xs, 2.0), square(0.0, 0.0, 6.0, 9.0));
    }

    #[test]
    fn an_empty_path_has_no_outline() {
        assert_eq!(sample(&[], 0, 1, 1), None);
    }
}
