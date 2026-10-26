//! Free drawing: what a `canvas` or a `chart` node holds, and the same drawing as filled
//! rectangles and placed text, so every host draws it alike from the solid fills and text it
//! already draws.
//!
//! A canvas's `draw` is a list of commands in the node's own pixels, the origin at its top left:
//!
//! ```text
//! { op = "line", x1, y1, x2, y2, color, width }   -- width 1 to 8 pixels
//! { op = "rect", x, y, w, h, color, fill }        -- fill: true for solid, else an outline
//! { op = "circle", x, y, r, color, fill }
//! { op = "text", x, y, text, color }
//! ```
//!
//! A chart's `series` is a list of `{ values = { … }, color, label }`, drawn as lines (`style =
//! "line"`, the default) or bars across the node, between `min` and `max` (the values' own when
//! left out). A chart is drawn as the canvas commands [`chart_commands`] answers.

use crate::value::Value;

/// The most commands one canvas may hold.
pub const MAX_COMMANDS: usize = 1_000;
/// The most rectangles one canvas is drawn with.
pub const MAX_RECTS: usize = 20_000;

/// One drawing command.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: u32,
        width: f64,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: u32,
        fill: bool,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        color: u32,
        fill: bool,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        color: u32,
    },
}

/// The draw operations, as a refusal names them.
pub const OPS: &[&str] = &["line", "rect", "circle", "text"];

/// A colour: `"#RRGGBB"`, a number `0xRRGGBB`, or white when absent.
///
/// # Errors
/// Anything else.
pub fn color(v: &Value) -> Result<u32, String> {
    match v {
        Value::Nil => Ok(0xFF_FF_FF),
        Value::Str(s) => s
            .strip_prefix('#')
            .filter(|h| h.len() == 6)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .ok_or_else(|| format!("{s:?} is not a colour (\"#RRGGBB\")")),
        other => other
            .as_i64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= 0xFF_FF_FF)
            .ok_or_else(|| "a colour is \"#RRGGBB\" or a number 0xRRGGBB".to_owned()),
    }
}

fn number(c: &Value, k: &str) -> Result<f64, String> {
    match c.get(k) {
        Value::Nil => Ok(0.0),
        v => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("a draw command's {k} is a number")),
    }
}

/// A canvas's `draw` list read into commands.
///
/// # Errors
/// A command of no known op, a field of the wrong type, or too many commands.
pub fn commands(v: &Value) -> Result<Vec<Command>, String> {
    let list = match v {
        Value::Nil => return Ok(Vec::new()),
        v => v.as_list().ok_or("draw is a list of commands")?,
    };
    if list.len() > MAX_COMMANDS {
        return Err(format!("a canvas holds at most {MAX_COMMANDS} commands"));
    }
    list.iter()
        .map(|c| {
            let n = |k: &str| number(c, k);
            let col = color(c.get("color"))?;
            let fill = c.get("fill").as_bool().unwrap_or(false);
            Ok(match c.get("op").as_str().unwrap_or_default() {
                "line" => Command::Line {
                    x1: n("x1")?,
                    y1: n("y1")?,
                    x2: n("x2")?,
                    y2: n("y2")?,
                    color: col,
                    width: match c.get("width") {
                        Value::Nil => 1.0,
                        _ => n("width")?.clamp(1.0, 8.0),
                    },
                },
                "rect" => Command::Rect {
                    x: n("x")?,
                    y: n("y")?,
                    w: n("w")?,
                    h: n("h")?,
                    color: col,
                    fill,
                },
                "circle" => Command::Circle {
                    x: n("x")?,
                    y: n("y")?,
                    r: n("r")?.abs(),
                    color: col,
                    fill,
                },
                "text" => Command::Text {
                    x: n("x")?,
                    y: n("y")?,
                    text: c.get("text").as_str().unwrap_or_default().to_owned(),
                    color: col,
                },
                other => {
                    return Err(format!(
                        "a draw command's op is one of {}, not {other:?}",
                        OPS.join(", ")
                    ))
                }
            })
        })
        .collect()
}

/// The chart styles.
pub const CHART_STYLES: &[&str] = &["line", "bar"];

/// The default colours of a chart's series, in turn.
const SERIES_COLORS: [u32; 6] = [
    0xFF_D7_00, 0x40_C0_FF, 0xFF_60_60, 0x60_E0_60, 0xC0_80_FF, 0xFF_A0_40,
];

/// A chart, `w` by `h` pixels, as canvas commands: a frame, each series as a line through its
/// values (or bars side by side), and the series' labels along the top.
///
/// # Errors
/// A series that is not `{ values, color, label }` with numbers for values.
pub fn chart_commands(
    series: &Value,
    min: Option<f64>,
    max: Option<f64>,
    bars: bool,
    w: f64,
    h: f64,
) -> Result<Vec<Command>, String> {
    let list = match series {
        Value::Nil => Vec::new(),
        v => v
            .as_list()
            .ok_or("series is a list of { values, color, label }")?
            .to_vec(),
    };
    let mut all = Vec::new();
    let mut read = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let values: Vec<f64> = s
            .get("values")
            .as_list()
            .ok_or("a series has values, a list of numbers")?
            .iter()
            .map(|v| {
                v.as_f64()
                    .filter(|n| n.is_finite())
                    .ok_or("a series' values are numbers")
            })
            .collect::<Result<_, _>>()?;
        all.extend(values.iter().copied());
        let col = match s.get("color") {
            Value::Nil => SERIES_COLORS[i % SERIES_COLORS.len()],
            c => color(c)?,
        };
        read.push((
            values,
            col,
            s.get("label").as_str().unwrap_or_default().to_owned(),
        ));
    }
    let lo = min.unwrap_or_else(|| all.iter().copied().fold(f64::INFINITY, f64::min).min(0.0));
    let hi = max.unwrap_or_else(|| all.iter().copied().fold(f64::NEG_INFINITY, f64::max));
    let span = if hi > lo { hi - lo } else { 1.0 };
    let top = 12.0;
    let plot_h = (h - top - 1.0).max(1.0);
    let y_of = |v: f64| top + plot_h * (1.0 - ((v - lo) / span).clamp(0.0, 1.0));
    let mut out = vec![Command::Rect {
        x: 0.0,
        y: top,
        w,
        h: h - top,
        color: 0x60_60_60,
        fill: false,
    }];
    let mut label_x = 2.0;
    let n_series = count(read.len().max(1));
    for (si, (values, col, label)) in read.iter().enumerate() {
        let n = values.len();
        if n > 0 {
            if bars {
                let slot = w / count(n);
                let bw = (slot / n_series).max(1.0);
                for (i, v) in values.iter().enumerate() {
                    let y = y_of(*v);
                    out.push(Command::Rect {
                        x: slot * count(i) + bw * count(si),
                        y,
                        w: (bw - 1.0).max(1.0),
                        h: (top + plot_h - y).max(1.0),
                        color: *col,
                        fill: true,
                    });
                }
            } else {
                let step = if n > 1 { (w - 1.0) / count(n - 1) } else { 0.0 };
                for i in 1..n {
                    out.push(Command::Line {
                        x1: step * count(i - 1),
                        y1: y_of(values[i - 1]),
                        x2: step * count(i),
                        y2: y_of(values[i]),
                        color: *col,
                        width: 1.0,
                    });
                }
                if n == 1 {
                    out.push(Command::Rect {
                        x: 0.0,
                        y: y_of(values[0]),
                        w: 2.0,
                        h: 2.0,
                        color: *col,
                        fill: true,
                    });
                }
            }
        }
        if !label.is_empty() {
            out.push(Command::Text {
                x: label_x,
                y: 0.0,
                text: label.clone(),
                color: *col,
            });
            label_x += 7.0 * (count(label.chars().count()) + 2.0);
        }
    }
    Ok(out)
}

/// One filled rectangle in the node's pixels: `x, y, w, h, 0xRRGGBB`.
pub type Rect = (i32, i32, i32, i32, u32);

/// One text at a place in the node's pixels.
pub type Placed = (i32, i32, String, u32);

fn px(v: f64) -> i32 {
    // Pixels on a node a few hundred wide: rounding to the nearest is the drawing.
    dereth_primitives::num::to_i32_f64(v.round().clamp(-100_000.0, 100_000.0))
}

/// A count as a number.
fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

/// The commands as filled rectangles and placed text, clipped to `w` by `h`.
#[must_use]
pub fn rasterize(commands: &[Command], w: i32, h: i32) -> (Vec<Rect>, Vec<Placed>) {
    let mut rects: Vec<Rect> = Vec::new();
    let mut texts = Vec::new();
    let put = |x: i32, y: i32, rw: i32, rh: i32, c: u32, rects: &mut Vec<Rect>| {
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = ((x + rw).min(w), (y + rh).min(h));
        if x1 > x0 && y1 > y0 && rects.len() < MAX_RECTS {
            rects.push((x0, y0, x1 - x0, y1 - y0, c));
        }
    };
    for c in commands {
        match c {
            Command::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => {
                let (dx, dy) = (x2 - x1, y2 - y1);
                let steps = dx.abs().max(dy.abs()).ceil().max(1.0);
                let size = px(*width).max(1);
                let half = size / 2;
                let n = px(steps);
                for i in 0..=n {
                    let t = f64::from(i) / steps;
                    put(
                        px(x1 + dx * t) - half,
                        px(y1 + dy * t) - half,
                        size,
                        size,
                        *color,
                        &mut rects,
                    );
                }
            }
            Command::Rect {
                x,
                y,
                w: rw,
                h: rh,
                color,
                fill,
            } => {
                let (x, y, rw, rh) = (px(*x), px(*y), px(*rw), px(*rh));
                if *fill {
                    put(x, y, rw, rh, *color, &mut rects);
                } else {
                    put(x, y, rw, 1, *color, &mut rects);
                    put(x, y + rh - 1, rw, 1, *color, &mut rects);
                    put(x, y, 1, rh, *color, &mut rects);
                    put(x + rw - 1, y, 1, rh, *color, &mut rects);
                }
            }
            Command::Circle {
                x,
                y,
                r,
                color,
                fill,
            } => {
                let ri = px(*r).max(1);
                let (cx, cy) = (px(*x), px(*y));
                for dy in -ri..=ri {
                    let half = (f64::from(ri * ri - dy * dy)).max(0.0).sqrt();
                    let hw = px(half);
                    if *fill {
                        put(cx - hw, cy + dy, 2 * hw + 1, 1, *color, &mut rects);
                    } else {
                        put(cx - hw, cy + dy, 1, 1, *color, &mut rects);
                        put(cx + hw, cy + dy, 1, 1, *color, &mut rects);
                    }
                }
                if !*fill {
                    for dx in -ri..=ri {
                        let hh = px((f64::from(ri * ri - dx * dx)).max(0.0).sqrt());
                        put(cx + dx, cy - hh, 1, 1, *color, &mut rects);
                        put(cx + dx, cy + hh, 1, 1, *color, &mut rects);
                    }
                }
            }
            Command::Text { x, y, text, color } => {
                texts.push((px(*x), px(*y), text.clone(), *color));
            }
        }
    }
    (rects, texts)
}

#[cfg(test)]
mod tests {
    use super::{chart_commands, commands, rasterize, Command};
    use crate::value::Value;

    #[test]
    fn a_canvas_reads_its_commands_and_draws_them_clipped_as_rectangles_and_text() {
        let draw = Value::List(vec![
            Value::map()
                .with("op", "line")
                .with("x1", 0_i64)
                .with("y1", 0_i64)
                .with("x2", 9_i64)
                .with("y2", 0_i64)
                .with("color", "#FF0000"),
            Value::map()
                .with("op", "rect")
                .with("x", 5_i64)
                .with("y", 5_i64)
                .with("w", 100_i64)
                .with("h", 4_i64)
                .with("fill", true),
            Value::map()
                .with("op", "text")
                .with("x", 1_i64)
                .with("y", 2_i64)
                .with("text", "hi"),
        ]);
        let cmds = commands(&draw).expect("reads");
        let (rects, texts) = rasterize(&cmds, 20, 20);
        assert_eq!(rects.iter().filter(|r| r.4 == 0xFF_00_00).count(), 10);
        assert!(
            rects.contains(&(5, 5, 15, 4, 0xFF_FF_FF)),
            "clipped: {rects:?}"
        );
        assert_eq!(texts, vec![(1, 2, "hi".to_owned(), 0xFF_FF_FF)]);
        assert!(commands(&Value::List(vec![Value::map().with("op", "blob")])).is_err());
    }

    #[test]
    fn a_chart_draws_a_line_through_each_series_between_its_bounds() {
        let series = Value::List(vec![Value::map().with(
            "values",
            Value::List(vec![
                Value::from(0_i64),
                Value::from(10_i64),
                Value::from(5_i64),
            ]),
        )]);
        let cmds = chart_commands(&series, None, None, false, 101.0, 52.0).expect("reads");
        let lines: Vec<_> = cmds
            .iter()
            .filter_map(|c| match c {
                Command::Line { y1, y2, .. } => Some((*y1, *y2)),
                _ => None,
            })
            .collect();
        // The plot runs from y 12 (the top, 10) to y 51 (the bottom, 0).
        assert_eq!(lines, vec![(51.0, 12.0), (12.0, 31.5)]);
    }
}
