//! Explore mode (`--explore [LOC]`): the world with no server and no body, and a free camera flown
//! through it at anything from a walk to thousands of meters a second.
//!
//! The camera starts where the location given says, or over Holtburg with none. A location is
//! read in any of the forms the client writes one: `@loc`'s text (with or without its
//! `Your location is:` in front, and with or without its turn), the visual report's whole line,
//! or its `loc` or `camera` field alone. A place a body stands is looked at from a little above
//! it, facing the way the body faces.
//!
//! What this module holds is the mode's own state (how fast the camera flies) and its words (the
//! location line it copies). The keys that fly it are the front end's.

use dereth_primitives::frame::V3 as _;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::camera::FreeCamera;
use crate::report::{block_id, block_xy, outdoor_cell, ReportCamera};

/// The flying speeds, in meters a second, slowest first.
pub const SPEEDS: [f32; 11] = [
    2.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0,
];

/// The speed the camera starts at: 100 m/s.
pub const START_SPEED: usize = 5;

/// How far above the place a body stands the camera starts, meters.
pub const ABOVE_A_BODY: f32 = 2.0;

/// How close above the drawn ground the camera may fly, meters.
pub const ABOVE_THE_GROUND: f32 = 1.0;

/// A landblock's side, meters.
const BLOCK: f32 = 192.0;

/// The mode's state while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Explore {
    /// An index into [`SPEEDS`].
    pub speed: usize,
}

impl Default for Explore {
    fn default() -> Self {
        Self { speed: START_SPEED }
    }
}

impl Explore {
    /// The flying speed, meters a second.
    #[must_use]
    pub fn meters_per_second(&self) -> f32 {
        SPEEDS[self.speed.min(SPEEDS.len() - 1)]
    }

    /// `steps` speeds faster (or slower, below zero), held between the slowest and the fastest.
    pub fn change_speed(&mut self, steps: i32) {
        let last = i64::try_from(SPEEDS.len() - 1).unwrap_or(0);
        let at = i64::try_from(self.speed).unwrap_or(0) + i64::from(steps);
        self.speed = usize::try_from(at.clamp(0, last)).unwrap_or(0);
    }
}

/// A way the camera flies while its control is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fly {
    Forward,
    Back,
    Left,
    Right,
    Up,
    Down,
}

impl Fly {
    /// `input` with this way held or let go.
    pub fn hold(self, input: &mut crate::camera::CameraInput, held: bool) {
        let slot = match self {
            Self::Forward => &mut input.forward,
            Self::Back => &mut input.back,
            Self::Left => &mut input.left,
            Self::Right => &mut input.right,
            Self::Up => &mut input.up,
            Self::Down => &mut input.down,
        };
        *slot = held;
    }
}

/// What the screen says while the mode runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    /// The flying speed, meters a second.
    pub meters_per_second: f32,
    /// The camera's spot, short ([`short_location`]); `None` before the world is up.
    pub location: Option<String>,
}

/// Where the camera stands, as a body standing there would: the cell, the place in its landblock
/// and the camera's heading as the turn. `camera` stands in the render space whose origin is the
/// south-west corner of `viewer_block`.
#[must_use]
pub fn camera_position(camera: &FreeCamera, viewer_block: (i32, i32)) -> Position {
    let p = camera.position;
    let (bx, by) = (
        dereth_primitives::num::floor_to_i32(p.x / BLOCK),
        dereth_primitives::num::floor_to_i32(p.y / BLOCK),
    );
    let block = block_id((viewer_block.0 + bx, viewer_block.1 + by));
    // LINT-OK: a block index, at most a few blocks from the block the world is drawn around.
    #[allow(clippy::cast_precision_loss)]
    let (x, y) = (p.x - bx as f32 * BLOCK, p.y - by as f32 * BLOCK);
    let half = camera.yaw * 0.5;
    Position::new(
        CellId(outdoor_cell(block, x, y)),
        Frame::new(
            Vec3::new(x, y, p.z),
            Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half)),
        ),
    )
}

/// Hold `camera`, in the render space whose origin is the south-west corner of `viewer_block`,
/// over the world's 255 blocks a side (0x00 to 0xFE), a centimeter short of its edges: past them is nothing to
/// draw and no block to measure a place from.
pub fn keep_in_world(camera: &mut FreeCamera, viewer_block: (i32, i32)) {
    // LINT-OK: block indexes, 0 to 255.
    #[allow(clippy::cast_precision_loss)]
    let (x, y) = (viewer_block.0 as f32 * BLOCK, viewer_block.1 as f32 * BLOCK);
    let edge = 255.0 * BLOCK - 0.01;
    camera.position.x = camera.position.x.clamp(0.01 - x, edge - x);
    camera.position.y = camera.position.y.clamp(0.01 - y, edge - y);
}

/// The height of the ground drawn under `(x, y)` in the render space whose origin is the
/// south-west corner of the window's own block; `None` where the window holds no ground there yet.
#[must_use]
pub fn ground_under(window: &crate::world_stream::LandblockWindow, x: f32, y: f32) -> Option<f32> {
    use dereth_terrain::land::mesh::{plane_set_height, LandblockMesh};
    let (bx, by) = (
        dereth_primitives::num::floor_to_i32(x / BLOCK),
        dereth_primitives::num::floor_to_i32(y / BLOCK),
    );
    let r = i64::from(window.mid_radius());
    let (xi, yi) = (
        u32::try_from(r + i64::from(bx)).ok()?,
        u32::try_from(r + i64::from(by)).ok()?,
    );
    if xi >= window.mid_width() || yi >= window.mid_width() {
        return None;
    }
    let mesh = window.slot(xi, yi)?.mesh.as_ref()?.get::<LandblockMesh>()?;
    // LINT-OK: a block index, at most the window's radius from the window's own block.
    #[allow(clippy::cast_precision_loss)]
    let (lx, ly) = (x - bx as f32 * BLOCK, y - by as f32 * BLOCK);
    // The squares a side: the cells on the default grid.
    let side = u8::try_from(mesh.squares_per_side()).unwrap_or(8).max(1);
    let size = BLOCK / f32::from(side);
    // LINT-OK: a block-local place to its square, from 0 to one less than the squares a side.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let square = |v: f32| ((v / size).floor().max(0.0) as usize).min(usize::from(side) - 1);
    let (i, j) = (square(lx), square(ly));
    let (a, b) = mesh.cell_polygons(i, j);
    let corner = |k: u32| mesh.vertices.get(usize::try_from(k).ok()?).copied();
    // The triangle of the two the place is in; on their shared edge either answers.
    let holds = |t: &dereth_terrain::land::mesh::LandPolygon| -> Option<bool> {
        let [p, q, w] = [corner(t.v[0])?, corner(t.v[1])?, corner(t.v[2])?];
        let turn = |u: Vec3, v: Vec3| (v.x - u.x) * (ly - u.y) - (v.y - u.y) * (lx - u.x);
        let (d0, d1, d2) = (turn(p, q), turn(q, w), turn(w, p));
        let eps = 1e-3;
        Some((d0 >= -eps && d1 >= -eps && d2 >= -eps) || (d0 <= eps && d1 <= eps && d2 <= eps))
    };
    let t = if holds(a)? { a } else { b };
    plane_set_height(&t.plane, lx, ly)
}

/// The landblock `camera` is over, by its coordinates, in the render space whose origin is the
/// south-west corner of `viewer_block`.
fn block_of(camera: &FreeCamera, viewer_block: (i32, i32)) -> (i32, i32) {
    let at = camera_position(camera, viewer_block);
    #[allow(clippy::cast_possible_wrap)] // a landblock index, 0..=255
    (
        ((at.cell.0 >> 24) & 0xFF) as i32,
        ((at.cell.0 >> 16) & 0xFF) as i32,
    )
}

/// Keep in `ws` the rooms of the landblocks round the free camera's, read from `store` as the
/// camera comes near them, so [`hold_over_the_world`] knows which room the camera is in and
/// which it flies over. The rest are let go.
pub fn keep_rooms(ws: &mut crate::world_state::WorldState, store: &dereth_dat::RetailDatStore) {
    let Some(viewer) = ws.viewer_block() else {
        return;
    };
    let (bx, by) = block_of(&ws.camera, viewer);
    ws.rooms
        .retain(|&(x, y), _| (x - bx).abs() <= 1 && (y - by).abs() <= 1);
    for dx in -1..=1 {
        for dy in -1..=1 {
            let b = (bx + dx, by + dy);
            if ws.rooms.contains_key(&b) {
                continue;
            }
            if !(0..=0xFE).contains(&b.0) || !(0..=0xFE).contains(&b.1) {
                continue;
            }
            let id = block_id(b);
            let rooms = dereth_world_data::env_cells::EnvCellLoader::new()
                .load_block(store, id)
                .iter()
                .filter_map(dereth_world_data::env_cells::room_shape)
                .collect();
            ws.rooms.insert(b, rooms);
        }
    }
}

/// Hold the free camera of `ws` where it may fly: over the world's blocks, and at least
/// [`ABOVE_THE_GROUND`] over the ground drawn under it, except in or over a room its landblock
/// holds. A room may lie under the ground (a cellar, a dungeon), so a camera in one stands in it
/// and the world is drawn from inside it, and a camera over or under one may fly through the
/// ground to it and back out.
pub fn hold_over_the_world(ws: &mut crate::world_state::WorldState) {
    let Some(block) = ws.viewer_block() else {
        return;
    };
    keep_in_world(&mut ws.camera, block);
    let at = camera_position(&ws.camera, block);
    let lb = block_of(&ws.camera, block);
    // A room may reach past its own block's edge, so the blocks round the camera's are asked
    // too, each of the point in its own block-local space.
    let mut nearby = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            if let Some(rooms) = ws.rooms.get(&(lb.0 + dx, lb.1 + dy)) {
                #[allow(clippy::cast_precision_loss)] // -1, 0 or 1
                let local =
                    at.frame
                        .origin
                        .sub(Vec3::new(dx as f32 * BLOCK, dy as f32 * BLOCK, 0.0));
                nearby.extend(rooms.iter().map(|r| (r, local)));
            }
        }
    }
    ws.free_camera_cell = nearby
        .iter()
        .find(|(r, p)| r.holds(*p))
        .map(|(r, _)| (r.id, r.seen_outside));
    if ws.free_camera_cell.is_some() || nearby.iter().any(|(r, p)| r.spans(p.x, p.y)) {
        return;
    }
    let p = ws.camera.position;
    if let Some(ground) = ground_under(&ws.streamer.window, p.x, p.y) {
        ws.camera.position.z = p.z.max(ground + ABOVE_THE_GROUND);
    }
}

/// The camera's spot as `@loc` says a place: `Your location is: 0x… [x y z] w x y z`.
#[must_use]
pub fn location_line(camera: &FreeCamera, viewer_block: (i32, i32)) -> String {
    format!(
        "Your location is: {}",
        crate::report::loc_text(&camera_position(camera, viewer_block))
    )
}

/// The camera's spot, short, for the screen: the cell and the place to a tenth of a meter. In
/// `room`, it is that room and the place measured from the room's block, which may be a
/// neighbor of the block the camera is over.
#[must_use]
pub fn short_location(
    camera: &FreeCamera,
    viewer_block: (i32, i32),
    room: Option<dereth_primitives::CellId>,
) -> String {
    let p = camera_position(camera, viewer_block);
    let (cell, o) = match room {
        Some(room) => {
            let block = |c: u32| ((c >> 24) & 0xFF, (c >> 16) & 0xFF);
            let ((rx, ry), (cx, cy)) = (block(room.0), block(p.cell.0));
            // LINT-OK: block indices, 0..=255, and their differences.
            #[allow(clippy::cast_precision_loss)]
            let shift = Vec3::new(
                (rx as f32 - cx as f32) * BLOCK,
                (ry as f32 - cy as f32) * BLOCK,
                0.0,
            );
            (room.0, p.frame.origin.sub(shift))
        }
        None => (p.cell.0, p.frame.origin),
    };
    format!("{cell:#010X} [{:.1} {:.1} {:.1}]", o.x, o.y, o.z)
}

/// What the screen says of the camera's spot: [`short_location`], in the room the camera is in
/// when it is in one; `None` before the world is up.
#[must_use]
pub fn screen_location(ws: &crate::world_state::WorldState) -> Option<String> {
    ws.viewer_block()
        .map(|block| short_location(&ws.camera, block, ws.free_camera_cell.map(|c| c.0)))
}

/// Read a location to start from. See the module header for the forms it reads.
///
/// # Errors
/// The text is none of them, as a sentence a player can act on.
pub fn parse_start(text: &str) -> Result<ReportCamera, String> {
    let text = text.trim();
    let camera = if text.contains(crate::report::HEADER) {
        let report = crate::report::VisualReport::parse(text)?;
        report
            .camera
            .or_else(|| report.loc.map(|p| from_body(&p)))
            .ok_or_else(|| "the report line has no camera and no loc".to_owned())?
    } else {
        let text = text
            .strip_prefix("Your location is:")
            .unwrap_or(text)
            .trim();
        if let Some(camera) = text.strip_prefix("camera ") {
            crate::report::parse_camera(camera.trim())
                .ok_or_else(|| format!("not a camera field: {text:?}"))?
        } else {
            let loc = text.strip_prefix("loc ").unwrap_or(text).trim();
            from_body(&parse_loc(loc).ok_or_else(|| {
                format!(
                    "not a location: {text:?}; expected one like 0xA9B4002A [133.2 27.5 94.0], \
                     as /loc writes it"
                )
            })?)
        }
    };
    Ok(in_its_block(camera))
}

/// `0xCCCCCCCC [x y z]`, then the turn `w x y z` or nothing (facing north).
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
    let numbers = |s: &str| -> Option<Vec<f32>> {
        s.split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()
    };
    let (p, q) = (numbers(point)?, numbers(turn)?);
    let [x, y, z] = p.as_slice() else {
        return None;
    };
    let turn = match q.as_slice() {
        [] => Quat::new(1.0, 0.0, 0.0, 0.0),
        [w, qx, qy, qz] => Quat::new(*w, *qx, *qy, *qz),
        _ => return None,
    };
    Some(Position::new(
        CellId(cell),
        Frame::new(Vec3::new(*x, *y, *z), turn),
    ))
}

/// The camera that looks from a little above where a body stands, the way it faces, level.
fn from_body(p: &Position) -> ReportCamera {
    let q = p.frame.rotation;
    let o = p.frame.origin;
    ReportCamera {
        // LINT-OK: a cell id's top half is its landblock.
        #[allow(clippy::cast_possible_truncation)]
        block: (p.cell.0 >> 16) as u16,
        origin: Vec3::new(o.x, o.y, o.z + ABOVE_A_BODY),
        // The heading of a turn about up alone.
        yaw: 2.0 * math::atan2f(q.z, q.w),
        pitch: 0.0,
        cell: Some(p.cell.0),
    }
}

/// `camera` measured from the block it stands in, so the world is drawn around that block.
fn in_its_block(camera: ReportCamera) -> ReportCamera {
    let o = camera.origin;
    let (bx, by) = (
        dereth_primitives::num::floor_to_i32(o.x / BLOCK),
        dereth_primitives::num::floor_to_i32(o.y / BLOCK),
    );
    if (bx, by) == (0, 0) {
        return camera;
    }
    let (x, y) = block_xy(camera.block);
    // LINT-OK: a block index difference, at most 255.
    #[allow(clippy::cast_precision_loss)]
    let origin = Vec3::new(o.x - bx as f32 * BLOCK, o.y - by as f32 * BLOCK, o.z);
    ReportCamera {
        block: block_id((x + bx, y + by)),
        origin,
        ..camera
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a developer's camera flown through the world with no server).
    use super::*;

    #[test]
    fn a_loc_line_starts_the_camera_a_little_above_the_place_facing_its_way() {
        let c = parse_start(
            "Your location is: 0xA9B4002A [133.168503 27.490582 94.005005] \
             -0.991022 0.000000 0.000000 -0.133696",
        )
        .expect("reads");
        assert_eq!(c.block, 0xA9B4);
        assert_eq!(c.cell, Some(0xA9B4_002A));
        assert!((c.origin.z - (94.005_005 + ABOVE_A_BODY)).abs() < 1e-4);
        // The turn's heading: the body faces the way the camera looks.
        let look = FreeCamera::new(Vec3::ZERO, c.yaw, 0.0).forward();
        let q = Quat::new(-0.991_022, 0.0, 0.0, -0.133_696);
        let body = dereth_primitives::frame::get_vector_heading(&Frame::new(Vec3::ZERO, q));
        assert!((look.x - body.x).abs() < 1e-3 && (look.y - body.y).abs() < 1e-3);
    }

    #[test]
    fn a_loc_without_its_turn_faces_north() {
        let c = parse_start("0xB2AA003E [178.5 137.3 52.0]").expect("reads");
        assert_eq!(c.block, 0xB2AA);
        assert_eq!((c.origin.x, c.origin.y), (178.5, 137.3));
        assert!(c.yaw.abs() < 1e-6);
    }

    #[test]
    fn a_report_line_starts_the_camera_where_the_report_s_camera_stood() {
        let line = "dereth report 1; loc none; camera 0xA8B3 [182.21875 35.5 91.25] \
                    yaw -1.5707964 pitch -0.2617994 cell 0xA8B3003A; mode free";
        let c = parse_start(line).expect("reads");
        assert_eq!(c.block, 0xA8B3);
        assert_eq!(c.origin, Vec3::new(182.218_75, 35.5, 91.25));
        assert!((c.pitch + 0.261_799_4).abs() < 1e-6);
        // The field alone reads the same.
        let field =
            parse_start("camera 0xA8B3 [182.21875 35.5 91.25] yaw -1.5707964 pitch -0.2617994")
                .expect("reads");
        assert_eq!((field.block, field.origin), (c.block, c.origin));
    }

    #[test]
    fn a_camera_measured_from_another_block_is_measured_from_its_own() {
        let c = parse_start("camera 0xA8B3 [200 -10 50] yaw 0 pitch 0").expect("reads");
        assert_eq!(c.block, 0xA9B2);
        assert!((c.origin.x - 8.0).abs() < 1e-4 && (c.origin.y - 182.0).abs() < 1e-4);
    }

    #[test]
    fn text_that_is_no_location_is_refused() {
        assert!(parse_start("Holtburg").is_err());
        assert!(parse_start("0xA9B4002A [1 2]").is_err());
        assert!(parse_start("dereth report 1; mode free").is_err());
    }

    #[test]
    fn the_copied_location_reads_back_as_the_camera_s_spot() {
        // A camera a block east of the one the world is drawn around.
        let camera = FreeCamera::new(Vec3::new(250.0, 20.0, 300.0), 1.0, -0.3);
        let line = location_line(&camera, (0xA9, 0xB4));
        assert!(line.starts_with("Your location is: 0xAAB40"), "{line}");
        let back = parse_start(&line).expect("reads back");
        assert_eq!(back.block, 0xAAB4);
        assert!((back.origin.x - 58.0).abs() < 1e-3 && (back.origin.y - 20.0).abs() < 1e-3);
        assert!((back.origin.z - (300.0 + ABOVE_A_BODY)).abs() < 1e-3);
        assert!((back.yaw - 1.0).abs() < 1e-4, "{}", back.yaw);
        assert_eq!(
            short_location(&camera, (0xA9, 0xB4), None),
            format!(
                "{:#010X} [58.0 20.0 300.0]",
                outdoor_cell(0xAAB4, 58.0, 20.0)
            )
        );
    }

    /// With no device, the camera flown east at the top speed for three seconds of frames
    /// crosses the blocks it should, and the world's window follows it there.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_camera_flown_at_the_top_speed_is_followed_by_the_world_s_window() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = crate::scene::SceneConfig {
            landblock: 0xA9B4,
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            ..crate::scene::SceneConfig::default()
        };
        let (mut ws, mut residency) =
            crate::world_build::load(&store, &cfg).expect("the world loads");
        let mut explore = Explore::default();
        explore.change_speed(100);
        ws.fly_speed = Some(explore.meters_per_second());
        // Level, facing east.
        ws.camera.yaw = -std::f32::consts::FRAC_PI_2;
        ws.camera.pitch = 0.0;
        let start = camera_position(&ws.camera, ws.viewer_block().expect("a window"));
        let forward = crate::camera::CameraInput {
            forward: true,
            ..crate::camera::CameraInput::default()
        };
        let mut stats = crate::world_step::StepCounters::default();
        let (mut now, dt) = (1.0, 1.0 / 30.0);
        for _ in 0..90 {
            now += f64::from(dt);
            crate::world_step::update(
                &mut ws,
                &mut residency,
                &cfg,
                forward,
                crate::character::CharacterInput::default(),
                dereth_primitives::LocalTime(now),
                dt,
                &mut stats,
            );
            residency.stream(&mut ws, &store, &cfg);
        }
        let block = ws.viewer_block().expect("a window");
        let end = camera_position(&ws.camera, block);
        // 5000 m/s for three seconds: 15 km east, 78 blocks and a bit.
        let landblock = |p: &Position| u16::try_from(p.cell.0 >> 16).expect("a landblock");
        let (from, to) = (block_xy(landblock(&start)), block_xy(landblock(&end)));
        assert_eq!(to.0 - from.0, 78, "{start:?} to {end:?}");
        assert_eq!(to.1, from.1, "{start:?} to {end:?}");
        // The window is drawn around the block the camera is in.
        assert_eq!(block_id(block), landblock(&end), "{end:?}");
    }

    /// With no device, the camera flown straight down onto a place a body stood stops a meter
    /// over the ground there: the body's height, as `/loc` gave it, plus one.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_camera_flown_down_stops_a_meter_over_the_ground() {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = crate::scene::SceneConfig {
            landblock: 0xA9B4,
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            ..crate::scene::SceneConfig::default()
        };
        let (mut ws, mut residency) =
            crate::world_build::load(&store, &cfg).expect("the world loads");
        // Where a body stood in Holtburg, on the ground at 94.005005 (`/loc`'s own words), with
        // the camera 50 m over it.
        let start = parse_start("0xA9B4002A [133.168503 27.490582 144.005005]").expect("reads");
        ws.camera = start.to_render(ws.viewer_block().expect("a window"));
        ws.fly_speed = Some(100.0);
        let down = crate::camera::CameraInput {
            down: true,
            ..crate::camera::CameraInput::default()
        };
        let mut stats = crate::world_step::StepCounters::default();
        let (mut now, dt) = (1.0, 1.0 / 30.0);
        for _ in 0..60 {
            now += f64::from(dt);
            crate::world_step::update(
                &mut ws,
                &mut residency,
                &cfg,
                down,
                crate::character::CharacterInput::default(),
                dereth_primitives::LocalTime(now),
                dt,
                &mut stats,
            );
            residency.stream(&mut ws, &store, &cfg);
        }
        let z = ws.camera.position.z;
        assert!(
            (z - (94.005_005 + ABOVE_THE_GROUND)).abs() < 0.01,
            "the camera stopped at {z}"
        );
        assert!((ws.camera.position.x - 133.1685).abs() < 1e-3);
    }

    /// Holtburg's world with no body and the explore camera flying, and the block's coordinates.
    fn holtburg_explored() -> (
        std::sync::Arc<dereth_dat::RetailDatStore>,
        crate::scene::SceneConfig,
        crate::world_state::WorldState,
        crate::world_build::BlockResidency,
    ) {
        let store = std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let cfg = crate::scene::SceneConfig {
            landblock: 0xA9B4,
            character: false,
            land_radius: 1,
            scenery_radius: 0,
            ..crate::scene::SceneConfig::default()
        };
        let (mut ws, residency) = crate::world_build::load(&store, &cfg).expect("the world loads");
        ws.fly_speed = Some(10.0);
        keep_rooms(&mut ws, &store);
        (store, cfg, ws, residency)
    }

    /// `n` frames of the explore camera held as `input` says.
    fn fly(
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        cfg: &crate::scene::SceneConfig,
        ws: &mut crate::world_state::WorldState,
        residency: &mut crate::world_build::BlockResidency,
        now: &mut f64,
        input: crate::camera::CameraInput,
        n: usize,
    ) {
        let mut stats = crate::world_step::StepCounters::default();
        let dt = 1.0 / 30.0;
        for _ in 0..n {
            *now += f64::from(dt);
            keep_rooms(ws, store);
            crate::world_step::update(
                ws,
                residency,
                cfg,
                input,
                crate::character::CharacterInput::default(),
                dereth_primitives::LocalTime(*now),
                dt,
                &mut stats,
            );
            residency.stream(ws, store, cfg);
        }
    }

    /// A room of Holtburg's block reaching under the ground, and a point it holds at least a
    /// meter under the ground there, block-local.
    fn a_cellar(ws: &crate::world_state::WorldState) -> (dereth_primitives::CellId, Vec3) {
        let rooms = &ws.rooms[&(0xA9, 0xB4)];
        let block = ws.viewer_block().expect("a window");
        assert_eq!(block, (0xA9, 0xB4), "the world is drawn around Holtburg");
        for r in rooms {
            for i in 0..=4u8 {
                for j in 0..=4u8 {
                    let x = r.min.x + (r.max.x - r.min.x) * f32::from(i) / 4.0;
                    let y = r.min.y + (r.max.y - r.min.y) * f32::from(j) / 4.0;
                    if !(1.0..191.0).contains(&x) || !(1.0..191.0).contains(&y) {
                        continue;
                    }
                    let Some(ground) = ground_under(&ws.streamer.window, x, y) else {
                        continue;
                    };
                    for k in 0..=8u8 {
                        let z = r.min.z + (r.max.z - r.min.z) * f32::from(k) / 8.0;
                        let p = Vec3::new(x, y, z);
                        if z < ground - 1.0 && r.holds(p) {
                            return (r.id, p);
                        }
                    }
                }
            }
        }
        panic!(
            "Holtburg's block holds no room under the ground among {}",
            rooms.len()
        )
    }

    /// With no device, a camera started in a room under the ground (a cellar) stays there and
    /// stands in it.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_camera_started_in_a_cellar_stays_there_and_stands_in_it() {
        let (store, cfg, mut ws, mut residency) = holtburg_explored();
        let mut now = 1.0;
        let (cellar, p) = a_cellar(&ws);
        ws.camera.position = p;
        fly(
            &store,
            &cfg,
            &mut ws,
            &mut residency,
            &mut now,
            crate::camera::CameraInput::default(),
            30,
        );
        assert_eq!(ws.camera.position, p, "the camera moved");
        assert_eq!(ws.free_camera_cell.map(|c| c.0), Some(cellar));
    }

    /// With no device, a camera flown straight down from over a cellar goes through the ground into
    /// it, and flown back up comes out over the ground, outdoors again.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_camera_flies_down_into_a_cellar_and_back_out() {
        let (store, cfg, mut ws, mut residency) = holtburg_explored();
        let mut now = 1.0;
        let (cellar, p) = a_cellar(&ws);
        let ground = ground_under(&ws.streamer.window, p.x, p.y).expect("the ground");
        ws.camera.position = Vec3::new(p.x, p.y, ground + 5.0);
        let (down, up) = (
            crate::camera::CameraInput {
                down: true,
                ..crate::camera::CameraInput::default()
            },
            crate::camera::CameraInput {
                up: true,
                ..crate::camera::CameraInput::default()
            },
        );
        let mut reached = false;
        for _ in 0..300 {
            fly(&store, &cfg, &mut ws, &mut residency, &mut now, down, 1);
            if ws.free_camera_cell.map(|c| c.0) == Some(cellar) {
                reached = true;
                break;
            }
        }
        assert!(
            reached,
            "flown down from {:.2} the camera stopped at {:.2}, the cellar at {:.2}",
            ground + 5.0,
            ws.camera.position.z,
            p.z
        );
        let shown = screen_location(&ws).expect("the world is up");
        assert!(
            shown.starts_with(&format!("{:#010X} [", cellar.0)),
            "in the cellar the screen says {shown}"
        );
        fly(&store, &cfg, &mut ws, &mut residency, &mut now, up, 150);
        assert!(
            ws.camera.position.z > ground + ABOVE_THE_GROUND,
            "flown up the camera is at {:.2}, the ground at {ground:.2}",
            ws.camera.position.z
        );
        assert_eq!(
            ws.free_camera_cell, None,
            "out over the ground the camera is outdoors"
        );
        let shown = screen_location(&ws).expect("the world is up");
        assert!(
            shown.starts_with("0xA9B400"),
            "out over the ground the screen says {shown}"
        );
    }

    /// With no device, a camera under the ground where no room lies over or under it is put back
    /// over the ground.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_camera_under_the_ground_away_from_every_room_is_put_back_over_it() {
        let (store, cfg, mut ws, mut residency) = holtburg_explored();
        let mut now = 1.0;
        let rooms = ws.rooms[&(0xA9, 0xB4)].clone();
        let (x, y) = (0u8..48)
            .flat_map(|i| {
                (0u8..48).map(move |j| (f32::from(i) * 4.0 + 2.0, f32::from(j) * 4.0 + 2.0))
            })
            .find(|&(x, y)| !rooms.iter().any(|r| r.spans(x, y)))
            .expect("open land in Holtburg's block");
        let ground = ground_under(&ws.streamer.window, x, y).expect("the ground");
        ws.camera.position = Vec3::new(x, y, ground - 5.0);
        fly(
            &store,
            &cfg,
            &mut ws,
            &mut residency,
            &mut now,
            crate::camera::CameraInput::default(),
            1,
        );
        assert!(
            (ws.camera.position.z - (ground + ABOVE_THE_GROUND)).abs() < 1e-3,
            "the camera is at {:.3}, the ground at {ground:.3}",
            ws.camera.position.z
        );
        assert_eq!(ws.free_camera_cell, None);
    }

    #[test]
    fn in_a_room_the_screen_names_the_room_and_measures_from_its_block() {
        let camera = FreeCamera::new(Vec3::new(58.0, 20.0, -30.0), 0.0, 0.0);
        let outdoors = short_location(&camera, (0xA9, 0xB4), None);
        assert!(outdoors.starts_with("0xA9B400"), "{outdoors}");
        assert_eq!(
            short_location(
                &camera,
                (0xA9, 0xB4),
                Some(dereth_primitives::CellId(0xA9B4_0105))
            ),
            "0xA9B40105 [58.0 20.0 -30.0]"
        );
        // A room of the block to the east reaching back over this one.
        assert_eq!(
            short_location(
                &camera,
                (0xA9, 0xB4),
                Some(dereth_primitives::CellId(0xAAB4_0100))
            ),
            "0xAAB40100 [-134.0 20.0 -30.0]"
        );
    }

    #[test]
    fn the_camera_is_held_over_the_world() {
        // Past the north-east corner, from a window around block (0xFE, 0xFE).
        let mut c = FreeCamera::new(Vec3::new(1000.0, 500.0, 80.0), 0.0, 0.0);
        keep_in_world(&mut c, (0xFE, 0xFE));
        assert!(c.position.x < BLOCK && c.position.x > BLOCK - 0.1);
        assert!(c.position.y < BLOCK && c.position.y > BLOCK - 0.1);
        let p = camera_position(&c, (0xFE, 0xFE));
        assert_eq!(p.cell.0 >> 16, 0xFEFE, "{p:?}");
        // Past the south-west one.
        let mut c = FreeCamera::new(Vec3::new(-500.0, -10.0, 80.0), 0.0, 0.0);
        keep_in_world(&mut c, (1, 0));
        assert!(c.position.x > -BLOCK && c.position.x < -BLOCK + 0.1);
        assert!(c.position.y > 0.0 && c.position.y < 0.1);
        // Inside, it stays where it is.
        let mut c = FreeCamera::new(Vec3::new(50.0, 60.0, 80.0), 0.0, 0.0);
        keep_in_world(&mut c, (0xA9, 0xB4));
        assert_eq!(c.position, Vec3::new(50.0, 60.0, 80.0));
    }

    #[test]
    fn the_speed_steps_between_a_walk_and_thousands_of_meters_a_second() {
        let mut e = Explore::default();
        assert!((e.meters_per_second() - 100.0).abs() < f32::EPSILON);
        e.change_speed(100);
        assert!((e.meters_per_second() - 5000.0).abs() < f32::EPSILON);
        e.change_speed(-1);
        assert!((e.meters_per_second() - 2500.0).abs() < f32::EPSILON);
        e.change_speed(-100);
        assert!((e.meters_per_second() - 2.0).abs() < f32::EPSILON);
    }
}
