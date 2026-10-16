//! The visual report (`@report`, CD-report-commands): one line saying where the player stands,
//! where the camera is and how the frame is drawn, so a picture seen on one machine can be drawn
//! again on another, exactly.
//!
//! The line is a list of fields, `; ` between them, each a name and its value:
//!
//! ```text
//! dereth report 1; loc 0xA8B3003A [186.312866 32.373905 89.739655] -0.710460 0.000000 -0.000000 -0.703737;
//! camera 0xA8B3 [182.21875 35.5 91.25] yaw -1.5707964 pitch -0.2617994 cell 0xA8B3003A;
//! eye 0xA8B3 [182.21875 35.5 91.4]; fov 90 vertical 61.92751 aspect 1; mode chase;
//! time 0.412 Midsong, server's clock; clock 1234567.25; weather auto; renderer wgpu;
//! better lighting on; retail look off; effects Lighting=4,Shadows=4; render scale 100;
//! art pack 1a2b3c4d5e6f7a8b Remastered; knobs 0f1e2d3c4b5a6978 walk_cap=0.3,cliffs=0;
//! draw distance 8; window 1920x1080; viewport 0 0 1920 1080; build dereth-client 0.4.4 1f1943a4
//! ```
//!
//! (one line, broken here to be read). `loc` is `@loc`'s own text. The camera's place is in meters
//! from the south-west corner of the landblock it names, which is the block the world is drawn
//! around and need not be the one it stands in, and its numbers are written so they read back as
//! the very same numbers: a camera read back from the line is the camera the frame was drawn
//! from. `eye` is where the frame was drawn from when Better Lighting draws the camera moved with
//! the ground it draws. Fields this client does not know are left out; a reader skips fields it
//! does not know.

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::camera::FreeCamera;

/// The first field of every report line, with the line's version.
pub const HEADER: &str = "dereth report 1";

/// Between two fields.
pub const SEPARATOR: &str = "; ";

/// A landblock's side, meters.
const BLOCK: f32 = 192.0;

/// How a presentation draws the frame, as it answers [`crate::present::Presentation::render_context`].
/// A presentation with no device answers the default: nothing known.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RenderContext {
    /// The renderer drawing: `wgpu`, `d3d12` or `vulkan`; empty without one.
    pub renderer: String,
    /// The graphics adapter's name.
    pub adapter: String,
    /// Its driver, where the renderer can say.
    pub driver: String,
    /// How frames are presented: the swap chain's mode, or how often the device waits for the
    /// screen.
    pub present_mode: String,
    /// Whether Better Lighting draws the frame.
    pub better_lighting: bool,
    /// Whether Better Lighting draws it with the retail look.
    pub retail_look: bool,
    /// Better Lighting's effects as the options page has them, `Name=value` a comma between.
    pub effects: String,
    /// The art pack drawn: its name, and the hash of its manifest and every file it read.
    pub art_pack: Option<(String, String)>,
    /// The share of the frame's pixels Better Lighting draws the world at, each way, percent.
    pub render_scale: u32,
    /// The hash of the drawn ground's tuning file as it was last read, once it has been.
    pub knobs_hash: Option<String>,
    /// The drawn ground's knobs that differ from their defaults, `name` and value.
    pub changed_knobs: Vec<(String, String)>,
    /// Where the frame is drawn from, in render space, when the presentation draws the camera
    /// moved from where the camera stands.
    pub drawn_eye: Option<Vec3>,
}

/// What the presentation's own parts of the last frame took, for a performance capture
/// ([`crate::present::Presentation::frame_costs`]). Milliseconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PresentCosts {
    /// The walk of the world that records the frame's draws.
    pub walk: f32,
    /// The copy of the frame handed to Better Lighting.
    pub snapshot: f32,
    /// The drawn ground's upkeep.
    pub drawn_ground: f32,
    /// Better Lighting's whole time on the processor.
    pub better_lighting: f32,
    /// Better Lighting's steps of the frame, by name.
    pub steps: Vec<(&'static str, f32)>,
    /// Each pass's time preparing the frame.
    pub prepare: Vec<(&'static str, f32)>,
    /// Each pass's time recording its work.
    pub encode: Vec<(&'static str, f32)>,
    /// Each pass's time on the GPU, where the device has timestamps; a few frames late.
    pub gpu: Vec<(&'static str, f32)>,
}

/// A short hash of some content: the first eight bytes of its BLAKE3 hash, in hex.
#[derive(Debug, Clone, Default)]
pub struct ContentHash(blake3::Hasher);

impl ContentHash {
    /// A hash of nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `bytes`.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Add a named part: its name, its length and its bytes, so two parts never run together.
    pub fn part(&mut self, name: &str, bytes: &[u8]) {
        let len = |n: usize| u64::try_from(n).unwrap_or(u64::MAX).to_le_bytes();
        self.0.update(&len(name.len()));
        self.0.update(name.as_bytes());
        self.0.update(&len(bytes.len()));
        self.0.update(bytes);
    }

    /// The hash, sixteen hex digits.
    #[must_use]
    pub fn finish(&self) -> String {
        self.0.finalize().as_bytes()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The hash of `bytes` alone.
    #[must_use]
    pub fn of(bytes: &[u8]) -> String {
        let mut h = Self::new();
        h.update(bytes);
        h.finish()
    }
}

/// Where the camera stands and how it is turned, in meters from the south-west corner of a
/// landblock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReportCamera {
    /// The landblock the place is measured from, `0xXXYY`.
    pub block: u16,
    /// Meters east, north and up from its south-west corner: outside 0 to 192 when the camera
    /// stands in another block.
    pub origin: Vec3,
    /// Radians counter-clockwise from north.
    pub yaw: f32,
    /// Radians, up positive.
    pub pitch: f32,
    /// The cell the camera stands in, where it is known.
    pub cell: Option<u32>,
}

impl ReportCamera {
    /// `camera`, which stands in the render space whose origin is the south-west corner of
    /// `viewer_block`, measured from that block's corner, in `cell` where it is known.
    #[must_use]
    pub fn from_render(camera: &FreeCamera, viewer_block: (i32, i32), cell: Option<u32>) -> Self {
        Self {
            block: block_id(viewer_block),
            origin: camera.position,
            yaw: camera.yaw,
            pitch: camera.pitch,
            cell,
        }
    }

    /// The camera in the render space whose origin is the south-west corner of `viewer_block`:
    /// exactly the camera it was made from when that is the block it was measured from.
    #[must_use]
    pub fn to_render(&self, viewer_block: (i32, i32)) -> FreeCamera {
        let (bx, by) = block_xy(self.block);
        // LINT-OK: a block index difference, at most 255, far inside f32's exact range.
        #[allow(clippy::cast_precision_loss)]
        let (dx, dy) = (
            (bx - viewer_block.0) as f32 * BLOCK,
            (by - viewer_block.1) as f32 * BLOCK,
        );
        FreeCamera {
            position: Vec3::new(self.origin.x + dx, self.origin.y + dy, self.origin.z),
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }
}

/// The landblock id `0xXXYY` of block `(x, y)`.
#[must_use]
pub fn block_id((x, y): (i32, i32)) -> u16 {
    // LINT-OK: block coordinates are 0 to 255.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let id = ((x.clamp(0, 255) as u16) << 8) | (y.clamp(0, 255) as u16);
    id
}

/// Block `(x, y)` of the landblock id `0xXXYY`.
#[must_use]
pub fn block_xy(block: u16) -> (i32, i32) {
    (i32::from(block >> 8), i32::from(block & 0xFF))
}

/// The outdoor cell of `block` holding the block-local point `(x, y)`.
#[must_use]
pub fn outdoor_cell(block: u16, x: f32, y: f32) -> u32 {
    // LINT-OK: a block-local position to its cell's column and row, 0 to 7.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (cx, cy) = (
        (x / 24.0).floor().clamp(0.0, 7.0) as u32,
        (y / 24.0).floor().clamp(0.0, 7.0) as u32,
    );
    (u32::from(block) << 16) | (cx * 8 + cy + 1)
}

/// The sky clock, as the report has it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReportTime {
    /// The fraction of the day, 0 midnight and 0.5 noon.
    pub fraction: f32,
    /// The region's name for that part of the day.
    pub name: String,
    /// Whether this client has moved its clock off the server's (`@tod`).
    pub set_here: bool,
    /// The absolute Dereth second the sky is drawn at: the calendar's day and its hour.
    pub clock: f64,
}

/// Everything a report line says. See the module header.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VisualReport {
    /// Where the player stands, as `@loc` says it.
    pub loc: Option<Position>,
    pub camera: Option<ReportCamera>,
    /// Where the frame is drawn from, measured from the camera's block, when that is not where
    /// the camera stands.
    pub eye: Option<Vec3>,
    /// The field of view the player chose, degrees, and the one the frame is drawn with
    /// upright, degrees.
    pub fov: f32,
    pub vertical_fov: f32,
    /// The aspect ratio the player chose, as the option stores it.
    pub aspect: u32,
    /// Which camera: `chase`, `first person`, `map`, `orbit` or `free`.
    pub mode: String,
    pub time: Option<ReportTime>,
    /// The weather asked for (`@weather`'s word), and whether the player's options leave the
    /// falling weather out.
    pub weather: String,
    pub weather_off: bool,
    pub render: RenderContext,
    /// The landscape's draw distance, blocks each way.
    pub draw_distance: u32,
    /// The window's size, pixels.
    pub window: (u32, u32),
    /// The 3D view's rectangle in it: x, y, width, height.
    pub viewport: Option<(u32, u32, u32, u32)>,
    /// The client's name and version, and the commit it was built from.
    pub build: String,
}

/// `@loc`'s text for `p`: the cell, the place and the turn, six places each.
#[must_use]
pub fn loc_text(p: &Position) -> String {
    crate::interaction::position_to_string(p)
}

fn on_off(on: bool) -> &'static str {
    if on {
        "on"
    } else {
        "off"
    }
}

/// A name or a word a field holds, kept to one field: no field separator in it.
fn clean(text: &str) -> String {
    text.replace(';', ",").replace(['\r', '\n'], " ")
}

impl VisualReport {
    /// The report as one line. See the module header.
    #[must_use]
    pub fn line(&self) -> String {
        let mut f: Vec<String> = vec![HEADER.to_owned()];
        f.push(match &self.loc {
            Some(p) => format!("loc {}", loc_text(p)),
            None => "loc none".to_owned(),
        });
        if let Some(c) = &self.camera {
            let mut field = format!(
                "camera {:#06X} [{} {} {}] yaw {} pitch {}",
                c.block, c.origin.x, c.origin.y, c.origin.z, c.yaw, c.pitch
            );
            if let Some(cell) = c.cell {
                field.push_str(&format!(" cell {cell:#010X}"));
            }
            f.push(field);
            if let Some(eye) = self.eye {
                f.push(format!(
                    "eye {:#06X} [{} {} {}]",
                    c.block, eye.x, eye.y, eye.z
                ));
            }
        }
        f.push(format!(
            "fov {} vertical {} aspect {}",
            self.fov, self.vertical_fov, self.aspect
        ));
        f.push(format!("mode {}", clean(&self.mode)));
        if let Some(t) = &self.time {
            let name = if t.name.is_empty() {
                String::new()
            } else {
                format!(" {}", clean(&t.name))
            };
            let whose = if t.set_here {
                "set here"
            } else {
                "server's clock"
            };
            f.push(format!("time {:.3}{name}, {whose}", t.fraction.max(0.0)));
            f.push(format!("clock {}", t.clock));
        }
        f.push(if self.weather_off {
            format!("weather {}, falling weather off", clean(&self.weather))
        } else {
            format!("weather {}", clean(&self.weather))
        });
        let r = &self.render;
        f.push(format!(
            "renderer {}",
            if r.renderer.is_empty() {
                "none".to_owned()
            } else {
                clean(&r.renderer)
            }
        ));
        f.push(format!("better lighting {}", on_off(r.better_lighting)));
        if r.better_lighting {
            f.push(format!("retail look {}", on_off(r.retail_look)));
            if !r.effects.is_empty() {
                f.push(format!("effects {}", clean(&r.effects)));
            }
            f.push(format!("render scale {}", r.render_scale));
        }
        f.push(match &r.art_pack {
            Some((name, hash)) => format!("art pack {hash} {}", clean(name)),
            None => "art pack none".to_owned(),
        });
        if r.knobs_hash.is_some() || !r.changed_knobs.is_empty() {
            let hash = r.knobs_hash.as_deref().unwrap_or("none");
            let changed = r
                .changed_knobs
                .iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            if changed.is_empty() {
                f.push(format!("knobs {hash}"));
            } else {
                f.push(format!("knobs {hash} {}", clean(&changed)));
            }
        }
        f.push(format!("draw distance {}", self.draw_distance));
        f.push(format!("window {}x{}", self.window.0, self.window.1));
        if let Some((x, y, w, h)) = self.viewport {
            f.push(format!("viewport {x} {y} {w} {h}"));
        }
        f.push(format!("build {}", clean(&self.build)));
        f.join(SEPARATOR)
    }

    /// Read a report line back. Fields it does not know are skipped, and so is anything before
    /// the report's first field, so a line pasted with a chat stamp in front of it reads.
    ///
    /// # Errors
    /// The line is not a report, or a field it knows does not read, as a sentence.
    pub fn parse(line: &str) -> Result<Self, String> {
        let start = line
            .find(HEADER)
            .ok_or_else(|| format!("not a report line: it does not start {HEADER:?}"))?;
        let mut r = Self::default();
        for field in line[start..].trim().split(';').map(str::trim).skip(1) {
            let bad = |what: &str| format!("the report's {what} does not read: {field:?}");
            if let Some(v) = field.strip_prefix("loc ") {
                if v != "none" {
                    r.loc = Some(parse_loc(v).ok_or_else(|| bad("loc"))?);
                }
            } else if let Some(v) = field.strip_prefix("camera ") {
                r.camera = Some(parse_camera(v).ok_or_else(|| bad("camera"))?);
            } else if let Some(v) = field.strip_prefix("eye ") {
                let (_, p, _) = parse_block_point(v).ok_or_else(|| bad("eye"))?;
                r.eye = Some(p);
            } else if let Some(v) = field.strip_prefix("fov ") {
                let w: Vec<&str> = v.split_whitespace().collect();
                let num = |i: usize| w.get(i).and_then(|s| s.parse::<f32>().ok());
                r.fov = num(0).ok_or_else(|| bad("fov"))?;
                for pair in w[1..].chunks(2) {
                    match pair {
                        ["vertical", v] => {
                            r.vertical_fov = v.parse().map_err(|_| bad("fov"))?;
                        }
                        ["aspect", v] => r.aspect = v.parse().map_err(|_| bad("fov"))?,
                        _ => {}
                    }
                }
            } else if let Some(v) = field.strip_prefix("mode ") {
                r.mode = v.to_owned();
            } else if let Some(v) = field.strip_prefix("time ") {
                let (head, whose) = v.rsplit_once(", ").unwrap_or((v, ""));
                let (fraction, name) = head.split_once(' ').unwrap_or((head, ""));
                let t = r.time.get_or_insert_with(ReportTime::default);
                t.fraction = fraction.parse().map_err(|_| bad("time"))?;
                t.name = name.to_owned();
                t.set_here = whose == "set here";
            } else if let Some(v) = field.strip_prefix("clock ") {
                r.time.get_or_insert_with(ReportTime::default).clock =
                    v.parse().map_err(|_| bad("clock"))?;
            } else if let Some(v) = field.strip_prefix("weather ") {
                let (word, rest) = v.split_once(", ").unwrap_or((v, ""));
                r.weather = word.to_owned();
                r.weather_off = rest == "falling weather off";
            } else if let Some(v) = field.strip_prefix("renderer ") {
                r.render.renderer = if v == "none" {
                    String::new()
                } else {
                    v.to_owned()
                };
            } else if let Some(v) = field.strip_prefix("better lighting ") {
                r.render.better_lighting = v == "on";
            } else if let Some(v) = field.strip_prefix("retail look ") {
                r.render.retail_look = v == "on";
            } else if let Some(v) = field.strip_prefix("effects ") {
                r.render.effects = v.to_owned();
            } else if let Some(v) = field.strip_prefix("render scale ") {
                r.render.render_scale = v.parse().map_err(|_| bad("render scale"))?;
            } else if let Some(v) = field.strip_prefix("art pack ") {
                if v != "none" {
                    let (hash, name) = v.split_once(' ').unwrap_or((v, ""));
                    r.render.art_pack = Some((name.to_owned(), hash.to_owned()));
                }
            } else if let Some(v) = field.strip_prefix("knobs ") {
                let (hash, changed) = v.split_once(' ').unwrap_or((v, ""));
                r.render.knobs_hash = (hash != "none").then(|| hash.to_owned());
                r.render.changed_knobs = changed
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| {
                        let (n, v) = s.split_once('=').ok_or_else(|| bad("knobs"))?;
                        Ok((n.trim().to_owned(), v.trim().to_owned()))
                    })
                    .collect::<Result<_, String>>()?;
            } else if let Some(v) = field.strip_prefix("draw distance ") {
                r.draw_distance = v.parse().map_err(|_| bad("draw distance"))?;
            } else if let Some(v) = field.strip_prefix("window ") {
                let (w, h) = v.split_once('x').ok_or_else(|| bad("window"))?;
                r.window = (
                    w.parse().map_err(|_| bad("window"))?,
                    h.parse().map_err(|_| bad("window"))?,
                );
            } else if let Some(v) = field.strip_prefix("viewport ") {
                let n: Vec<&str> = v.split_whitespace().collect();
                let [x, y, w, h] = n.as_slice() else {
                    return Err(bad("viewport"));
                };
                r.viewport = Some((
                    x.parse().map_err(|_| bad("viewport"))?,
                    y.parse().map_err(|_| bad("viewport"))?,
                    w.parse().map_err(|_| bad("viewport"))?,
                    h.parse().map_err(|_| bad("viewport"))?,
                ));
            } else if let Some(v) = field.strip_prefix("build ") {
                r.build = v.to_owned();
            }
        }
        Ok(r)
    }

    /// The drawn ground's changed knobs in the tuning file's form, a line each.
    #[must_use]
    pub fn knobs_text(&self) -> String {
        self.render
            .changed_knobs
            .iter()
            .map(|(n, v)| format!("{n} = {v}\n"))
            .collect()
    }
}

/// `0xCCCCCCCC [x y z] w x y z`, `@loc`'s text.
fn parse_loc(v: &str) -> Option<Position> {
    let (cell, rest) = v.split_once(' ')?;
    let cell = u32::from_str_radix(
        cell.trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X"),
        16,
    )
    .ok()?;
    let (point, turn) = rest.trim().strip_prefix('[')?.split_once(']')?;
    let p: Vec<f32> = point
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let q: Vec<f32> = turn
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let ([x, y, z], [w, qx, qy, qz]) = (p.as_slice(), q.as_slice()) else {
        return None;
    };
    Some(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(*x, *y, *z), Quat::new(*w, *qx, *qy, *qz)),
    ))
}

/// `0xXXYY [x y z]` and what follows it.
fn parse_block_point(v: &str) -> Option<(u16, Vec3, &str)> {
    let (block, rest) = v.split_once(' ')?;
    let block =
        u16::from_str_radix(block.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?;
    let (point, tail) = rest.trim().strip_prefix('[')?.split_once(']')?;
    let p: Vec<f32> = point
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    let [x, y, z] = p.as_slice() else {
        return None;
    };
    Some((block, Vec3::new(*x, *y, *z), tail.trim()))
}

/// `0xXXYY [x y z] yaw Y pitch P[ cell 0xCCCCCCCC]`.
fn parse_camera(v: &str) -> Option<ReportCamera> {
    let (block, origin, tail) = parse_block_point(v)?;
    let w: Vec<&str> = tail.split_whitespace().collect();
    let mut c = ReportCamera {
        block,
        origin,
        yaw: 0.0,
        pitch: 0.0,
        cell: None,
    };
    let (mut yaw, mut pitch) = (None, None);
    for pair in w.chunks(2) {
        match pair {
            ["yaw", v] => yaw = v.parse().ok(),
            ["pitch", v] => pitch = v.parse().ok(),
            ["cell", v] => {
                c.cell =
                    u32::from_str_radix(v.trim_start_matches("0x").trim_start_matches("0X"), 16)
                        .ok();
            }
            _ => {}
        }
    }
    c.yaw = yaw?;
    c.pitch = pitch?;
    Some(c)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the report line's own form; the command that writes it is asserted
    //! through the whole client).
    use super::*;

    fn sample() -> VisualReport {
        VisualReport {
            loc: Some(Position::new(
                CellId(0xA8B3_003A),
                Frame::new(
                    Vec3::new(186.312_87, 32.373_905, 89.739_655),
                    Quat::new(-0.710_46, 0.0, -0.0, -0.703_737),
                ),
            )),
            camera: Some(ReportCamera {
                block: 0xA8B3,
                origin: Vec3::new(182.218_75, -0.100_000_38, 91.250_01),
                yaw: -1.570_796_4,
                pitch: -0.261_799_4,
                cell: Some(0xA8B3_003A),
            }),
            eye: Some(Vec3::new(182.218_75, -0.100_000_38, 91.400_01)),
            fov: 90.0,
            vertical_fov: 61.927_51,
            aspect: 1,
            mode: "chase".into(),
            time: Some(ReportTime {
                fraction: 0.412_345,
                name: "Midsong".into(),
                set_here: true,
                clock: 1_234_567.123_456_789,
            }),
            weather: "rain".into(),
            weather_off: true,
            render: RenderContext {
                renderer: "wgpu".into(),
                better_lighting: true,
                retail_look: false,
                effects: "Lighting=4,Shadows=4".into(),
                art_pack: Some(("Remastered; v2".into(), "1a2b3c4d5e6f7a8b".into())),
                render_scale: 75,
                knobs_hash: Some("0f1e2d3c4b5a6978".into()),
                changed_knobs: vec![
                    ("walk_cap".into(), "0.3".into()),
                    ("cliffs".into(), "0".into()),
                ],
                ..RenderContext::default()
            },
            draw_distance: 8,
            window: (1920, 1080),
            viewport: Some((0, 0, 1920, 1080)),
            build: "dereth-client 0.4.4 1f1943a4".into(),
        }
    }

    #[test]
    fn a_report_line_reads_back_as_the_report_it_was_written_from() {
        let report = sample();
        let line = report.line();
        assert!(!line.contains('\n'), "{line}");
        let back = VisualReport::parse(&line).unwrap();
        // The pack's name loses its field separator; everything else reads back exactly, the
        // camera's numbers to the bit.
        let mut want = report.clone();
        want.render.art_pack = Some(("Remastered, v2".into(), "1a2b3c4d5e6f7a8b".into()));
        // `@loc` writes six places.
        let (have_loc, want_loc) = (back.loc.unwrap(), want.loc.unwrap());
        assert_eq!(have_loc.cell, want_loc.cell);
        assert!((have_loc.frame.origin.x - want_loc.frame.origin.x).abs() < 1e-5);
        want.loc = back.loc;
        // The time's fraction is written to three places.
        want.time.as_mut().unwrap().fraction = 0.412;
        assert_eq!(back, want);
        let c = back.camera.unwrap();
        let w = report.camera.unwrap();
        for (a, b) in [
            (c.origin.x, w.origin.x),
            (c.origin.y, w.origin.y),
            (c.origin.z, w.origin.z),
            (c.yaw, w.yaw),
            (c.pitch, w.pitch),
        ] {
            assert_eq!(a.to_bits(), b.to_bits());
        }
        assert_eq!(
            back.time.as_ref().unwrap().clock.to_bits(),
            report.time.unwrap().clock.to_bits()
        );
        assert_eq!(back.knobs_text(), "walk_cap = 0.3\ncliffs = 0\n");
    }

    #[test]
    fn a_line_pasted_after_a_chat_stamp_reads_and_unknown_fields_are_skipped() {
        let line = format!("[12:01] {}; a field from later 7", sample().line());
        let back = VisualReport::parse(&line).unwrap();
        assert_eq!(back.window, (1920, 1080));
        assert!(VisualReport::parse("Your location is: 0xA8B3003A [1 2 3] 1 0 0 0").is_err());
        let broken = sample().line().replace("yaw -1.5707964", "yaw west");
        assert!(VisualReport::parse(&broken).is_err());
    }

    #[test]
    fn a_camera_measured_from_the_block_the_world_is_drawn_around_comes_back_exactly() {
        let camera = FreeCamera {
            position: Vec3::new(-0.100_000_38, 191.999_98, 61.25),
            yaw: 3.1,
            pitch: -0.4,
        };
        let r = ReportCamera::from_render(&camera, (0xA8, 0xB3), None);
        assert_eq!(r.block, 0xA8B3);
        assert_eq!(r.to_render((0xA8, 0xB3)), camera);
        // Drawn around the next block east, the same place is a block further west.
        let moved = r.to_render((0xA9, 0xB3));
        assert_eq!(moved.position.x, camera.position.x - 192.0);
        assert_eq!(outdoor_cell(0xA8B3, 186.3, 32.4), 0xA8B3_003A);
    }

    #[test]
    fn a_hash_names_its_content_and_parts_never_run_together() {
        assert_eq!(ContentHash::of(b"abc").len(), 16);
        assert_eq!(ContentHash::of(b"abc"), ContentHash::of(b"abc"));
        assert_ne!(ContentHash::of(b"abc"), ContentHash::of(b"abd"));
        let mut a = ContentHash::new();
        a.part("ab", b"c");
        let mut b = ContentHash::new();
        b.part("a", b"bc");
        assert_ne!(a.finish(), b.finish());
    }
}
