//! Explore mode's world (`--explore`): the land, the objects placed with it and the sky drawn
//! about a free camera, with no server and no body; and the camera's own words, where it stands as
//! a location line, read back to start it.
//!
//! A location is read in the forms the client writes one: a position as the game's scripts write
//! it (`0x6FA90015 [45.24 90.51 85.26] 1 0 0 0`, its turn four parts, one heading in degrees, or
//! none for north), or map coordinates as `/loc` says them (`You are at: 41.2S, 12.6E`, or the
//! coordinates alone). A place a body stands is looked at from a little above it, facing its way;
//! map coordinates name a land cell, which the camera starts over the middle of, a little above
//! the ground there once it is drawn.
//!
//! The keys that fly the camera are the shell's.

use auberean_draw::scene::Spot;
use auberean_physics::geom::{frame_heading, set_heading, set_rotate, Heading};
use auberean_primitives::land::{cell_at, is_valid_cell, map_coords};
use auberean_primitives::Vec3;
use dereth_primitives::num::{floor_to_i32, math};

use crate::{block_of, Built, Scene, BLOCK_LENGTH};

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

/// The world's blocks along each side.
const BLOCKS: i32 = 255;

/// Land cells along a block's side, and a land cell's side in meters.
const CELLS: i32 = 8;
const CELL: f32 = BLOCK_LENGTH / 8.0;

/// The land's size in land cells.
const LAND: (i32, i32) = (BLOCKS * CELLS, BLOCKS * CELLS);

/// How far the camera may look up or down, degrees.
const MOST_PITCH: f32 = 89.0;

/// What `--explore` with no location starts at: a lawn in block 6FA9 where players arrive in the
/// world, facing south.
pub const DEFAULT_START: &str = "0x6FA90015 [45.24 90.51 85.26] 178";

/// The camera: where it stands and which way it looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreeCamera {
    /// The block it stands in, x east and y north.
    pub block: (i32, i32),
    /// Where it stands in that block's frame, meters from its south-west corner.
    pub position: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
    /// Its height is not known yet: it goes a little above the ground once the ground under it is
    /// drawn.
    pub find_ground: bool,
}

/// The ways the camera flies while their controls are held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Ways {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl Ways {
    /// Whether any way is held.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.forward || self.back || self.left || self.right || self.up || self.down
    }
}

/// Plus one for `a`, minus one for `b`.
fn axis(a: bool, b: bool) -> f32 {
    f32::from(u8::from(a)) - f32::from(u8::from(b))
}

impl FreeCamera {
    /// Which way it looks, a unit vector: x east, y north, z up.
    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let (h, p) = (self.heading.to_radians(), self.pitch.to_radians());
        let flat = math::cosf(p);
        Vec3::new(math::sinf(h) * flat, math::cosf(h) * flat, math::sinf(p))
    }

    /// Flown `meters` along the ways held: ahead where it looks, to the side level, and straight up
    /// or down; held over the world's blocks.
    pub fn fly(&mut self, ways: Ways, meters: f32) {
        let f = self.forward();
        let h = self.heading.to_radians();
        let right = Vec3::new(math::cosf(h), -math::sinf(h), 0.0);
        let (ahead, side, up) = (
            axis(ways.forward, ways.back),
            axis(ways.right, ways.left),
            axis(ways.up, ways.down),
        );
        let d = Vec3::new(
            f.x * ahead + right.x * side,
            f.y * ahead + right.y * side,
            f.z * ahead + up,
        );
        let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
        if len < 1e-6 {
            return;
        }
        let k = meters / len;
        self.position = Vec3::new(
            self.position.x + d.x * k,
            self.position.y + d.y * k,
            self.position.z + d.z * k,
        );
        self.rehome();
    }

    /// Turned `right` degrees and tilted `up` degrees, never past looking straight up or down.
    pub fn turn(&mut self, right: f32, up: f32) {
        self.heading = (self.heading + right).rem_euclid(360.0);
        self.pitch = (self.pitch + up).clamp(-MOST_PITCH, MOST_PITCH);
    }

    /// Measured from the block it stands in, held over the world's blocks a centimeter short of
    /// their edges: past them is nothing to draw.
    fn rehome(&mut self) {
        let shift = |v: f32| floor_to_i32(v / BLOCK_LENGTH);
        let (dx, dy) = (shift(self.position.x), shift(self.position.y));
        let (bx, by) = (self.block.0 + dx, self.block.1 + dy);
        // LINT-OK: a block step, a handful at most in one frame.
        #[allow(clippy::cast_precision_loss)]
        let (mut x, mut y) = (
            self.position.x - dx as f32 * BLOCK_LENGTH,
            self.position.y - dy as f32 * BLOCK_LENGTH,
        );
        let hold = |b: i32, v: &mut f32| {
            if b < 0 {
                *v = 0.01;
                0
            } else if b >= BLOCKS {
                *v = BLOCK_LENGTH - 0.01;
                BLOCKS - 1
            } else {
                b
            }
        };
        self.block = (hold(bx, &mut x), hold(by, &mut y));
        self.position = Vec3::new(x, y, self.position.z);
    }

    /// The land cell it stands over.
    #[must_use]
    pub fn cell(&self) -> u32 {
        let at = |b: i32, v: f32| b * CELLS + floor_to_i32(v / CELL).clamp(0, CELLS - 1);
        cell_at(
            at(self.block.0, self.position.x),
            at(self.block.1, self.position.y),
            LAND,
        )
    }

    /// Where it stands as the game's scripts write a position: the cell, the place in its block,
    /// and its heading as the turn (`0x6FA90015 [45.240000 90.510000 87.260000] w x y z`).
    #[must_use]
    pub fn location_line(&self) -> String {
        let mut frame = dereth_primitives::Frame::new(
            dereth_primitives::Vec3::ZERO,
            dereth_primitives::Quat::IDENTITY,
        );
        set_heading(&mut frame, Heading::new(self.heading));
        let (p, q) = (self.position, frame.rotation);
        format!(
            "0x{:08X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
            self.cell(),
            p.x,
            p.y,
            p.z,
            q.w,
            q.x,
            q.y,
            q.z
        )
    }

    /// Where it stands, short, for the screen: the map coordinates, then the cell and the place to
    /// a tenth of a meter.
    #[must_use]
    pub fn short_location(&self) -> String {
        let cell = self.cell();
        let p = self.position;
        format!(
            "{}   0x{cell:08X} [{:.1} {:.1} {:.1}]",
            map_text(cell),
            p.x,
            p.y,
            p.z
        )
    }
}

/// A land cell's map coordinates as `/loc` says them: north or south to a tenth, then east or west,
/// the letter left out on the middle line.
#[must_use]
pub fn map_text(cell: u32) -> String {
    let Some((east, north)) = map_coords(cell, LAND) else {
        return String::new();
    };
    let letter = |v: f32, negative: &'static str, positive: &'static str| {
        if v < 0.0 {
            negative
        } else if v > 0.0 {
            positive
        } else {
            ""
        }
    };
    format!(
        "{:.1}{}, {:.1}{}",
        north.abs(),
        letter(north, "S", "N"),
        east.abs(),
        letter(east, "W", "E")
    )
}

/// Read a location to start from. See the module header for the forms it reads.
///
/// # Errors
/// The text is none of them, or names no place in the world, as a sentence a player can act on.
pub fn parse_start(text: &str) -> Result<FreeCamera, String> {
    let text = text.trim();
    let text = text.strip_prefix("You are at:").unwrap_or(text).trim();
    let refused = || {
        format!(
            "not a location: {text:?}; expected a position like 0x6FA90015 [45.2 90.5 85.3], or \
             map coordinates like 41.2S, 12.6E"
        )
    };
    if text.starts_with("0x") || text.starts_with("0X") {
        let (cell, origin, heading) = parse_position(text).ok_or_else(refused)?;
        if !is_valid_cell(cell, LAND) {
            return Err(format!("{text:?} names no cell of the world"));
        }
        let mut camera = FreeCamera {
            block: (
                i32::try_from(cell >> 24).unwrap_or(0),
                i32::try_from((cell >> 16) & 0xFF).unwrap_or(0),
            ),
            position: Vec3::new(origin.x, origin.y, origin.z + ABOVE_A_BODY),
            heading,
            pitch: 0.0,
            find_ground: false,
        };
        camera.rehome();
        return Ok(camera);
    }
    let (east, north) = parse_map(text).ok_or_else(refused)?;
    let cell = auberean_physics::cells::map_coords_to_cell(east, north)
        .ok_or_else(|| format!("{text:?} is off the map"))?;
    let low = (cell.0 & 0xFFFF) - 1;
    // LINT-OK: a land cell's column and row in its block, 0 to 7.
    #[allow(clippy::cast_precision_loss)]
    let middle = |k: u32| k as f32 * CELL + CELL * 0.5;
    Ok(FreeCamera {
        block: (
            i32::try_from(cell.0 >> 24).unwrap_or(0),
            i32::try_from((cell.0 >> 16) & 0xFF).unwrap_or(0),
        ),
        position: Vec3::new(middle(low >> 3), middle(low & 7), 0.0),
        heading: 0.0,
        pitch: 0.0,
        find_ground: true,
    })
}

/// `0xCCCCCCCC [x y z]`, then the turn: four parts scalar first, a heading in degrees, or nothing
/// (north). The cell, the place and the heading.
fn parse_position(text: &str) -> Option<(u32, Vec3, f32)> {
    let rest = text.get(2..)?;
    let (cell, rest) = rest.split_once('[')?;
    let cell = u32::from_str_radix(cell.trim(), 16).ok()?;
    let (point, turn) = rest.split_once(']')?;
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
    let heading = match q.as_slice() {
        [] => 0.0,
        [h] => Heading::new(*h).degrees(),
        [w, qx, qy, qz] => {
            let mut frame = dereth_primitives::Frame::new(
                dereth_primitives::Vec3::ZERO,
                dereth_primitives::Quat::IDENTITY,
            );
            set_rotate(&mut frame, dereth_primitives::Quat::new(*w, *qx, *qy, *qz));
            frame_heading(&frame).degrees()
        }
        _ => return None,
    };
    Some((cell, Vec3::new(*x, *y, *z), heading))
}

/// Map coordinates as a player writes them: north or south, a comma if liked, then east or west,
/// the last letter left out meaning east. East and north.
fn parse_map(text: &str) -> Option<(f32, f32)> {
    let (first, second) = match text.split_once(',') {
        Some((a, b)) => (a.trim(), b.trim()),
        None => text.split_once(char::is_whitespace)?,
    };
    let signed = |s: &str, negative: char, positive: char, bare: bool| -> Option<f32> {
        let s = s.trim();
        let (number, letter) = match s.char_indices().last()? {
            (i, c) if c.is_ascii_alphabetic() => (&s[..i], Some(c.to_ascii_uppercase())),
            _ => (s, None),
        };
        let v: f32 = number.trim().parse().ok()?;
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        match letter {
            Some(c) if c == negative => Some(-v),
            Some(c) if c == positive => Some(v),
            None if bare => Some(v),
            _ => None,
        }
    };
    Some((
        signed(second, 'W', 'E', true)?,
        signed(first, 'S', 'N', false)?,
    ))
}

impl Scene<'_> {
    /// The height of the ground the last frame drew under `camera`; `None` off the ground it drew.
    #[must_use]
    pub fn ground_under(&self, camera: &FreeCamera) -> Option<f32> {
        let built = self.built.as_ref()?;
        let block = built.key.0;
        let home = (i32::from(block.x()), i32::from(block.y()));
        // LINT-OK: a block difference, a few blocks at most.
        #[allow(clippy::cast_precision_loss)]
        let at = Vec3::new(
            camera.position.x + (camera.block.0 - home.0) as f32 * BLOCK_LENGTH,
            camera.position.y + (camera.block.1 - home.1) as f32 * BLOCK_LENGTH,
            camera.position.z,
        );
        let square = self.builder.ground_square(home, at)?;
        auberean_draw::decal::ground_height(&square, at.x, at.y)
    }

    /// Hold `camera` at least [`ABOVE_THE_GROUND`] over the ground the last frame drew under it,
    /// and a camera that has yet to find the ground [`ABOVE_A_BODY`] over it.
    pub fn hold_over_ground(&self, camera: &mut FreeCamera) {
        let Some(ground) = self.ground_under(camera) else {
            return;
        };
        if camera.find_ground {
            camera.position.z = ground + ABOVE_A_BODY;
            camera.find_ground = false;
        }
        camera.position.z = camera.position.z.max(ground + ABOVE_THE_GROUND);
    }

    /// Build the world's frame seen from `camera` at `now`, for [`Scene::frames`] to hand out: the
    /// land and the objects placed with it (rebuilt when the camera enters another land cell),
    /// the placed objects' bodies posed and their effects played, under the sky and light of the
    /// moment. No body is drawn and no portal space.
    pub fn build_explore(&mut self, now: f64, camera: &FreeCamera) {
        self.frame_rate.end_frame(now);
        let (bx, by) = camera.block;
        let eye = camera.position;
        let (time_of_day, day) = self.moment();
        let spot = Spot {
            block: (bx, by),
            position: eye,
            heading: camera.heading,
            pitch: camera.pitch,
            time_of_day,
            day,
        };
        let block = block_of(camera.cell());
        let viewer_cell = (floor_to_i32(eye.x / CELL), floor_to_i32(eye.y / CELL));
        self.builder.camera_in_water = false;
        let key = (block, viewer_cell, self.lightings, false);
        if self.built.as_ref().is_none_or(|b| b.key != key) {
            self.builder.uptime = now;
            let frame = self.builder.frame(&spot, self.fov);
            let world_objects = frame.objects.len();
            self.placed_effects
                .sync(&self.systems, &self.world.records, &frame.placed, now);
            self.built = Some(Built {
                frame,
                key,
                at: now,
                world_objects,
                outside: None,
            });
        }
        if let Some(step) = self.anim_clock.step(now) {
            self.placed_bodies.advance(&self.anims, step);
        }
        let Some(built) = self.built.as_mut() else {
            return;
        };
        built.frame.objects.truncate(built.world_objects);
        self.placed_bodies.pose(
            &self.world.records,
            &self.anims,
            &mut self.drawer,
            &mut built.frame.objects,
        );
        self.drawer.eye = Some(eye);
        let frame = &mut built.frame;
        frame.camera.position = eye;
        frame.camera.heading = camera.heading;
        frame.camera.pitch = camera.pitch;
        frame.camera.roll = 0.0;
        frame.camera.fov_y = self.fov;
        frame.uptime = now;
        // The sky stands at the camera and moves on with the moment, and the light and fog are the
        // moment's at the camera.
        let day = self.builder.day(&spot);
        frame.light = self.builder.light(&spot, day.as_ref());
        let mut counts = frame.counts;
        counts.sky_objects = 0;
        let (before, after, dome) =
            self.builder
                .sky(eye, day.as_ref(), spot.time_of_day, &mut counts);
        frame.sky_before = before;
        frame.sky_after = after;
        frame.sky_dome = dome;
        // The moment's rain, as the world's frame has it (explore mode draws no streaks).
        frame.rain = day.as_ref().map_or(0.0, |d| {
            auberean_world::sky::rain(d, spot.time_of_day)
                .0
                .clamp(0.0, 1.0)
        });
        frame.counts = counts;
        // The lights the placed objects' effects carry light what they reach.
        let block_len = self.world.region.land.grid().block_length();
        let carried: Vec<auberean_world::inside::Light> = self
            .placed_effects
            .lights((bx, by), block_len, now)
            .iter()
            .map(|l| l.standing((bx, by), block_len, now, &self.anims.dice))
            .collect();
        let outside = auberean_world::inside::Outside::of(&frame.light);
        self.builder.light_carried(
            &spot,
            &mut frame.objects[..built.world_objects],
            &carried,
            &outside,
        );
        frame.move_materials(now, &self.anims.dice);
        self.seen.camera = Some(crate::seen::SeenCamera {
            position: eye,
            heading: camera.heading,
            pitch: camera.pitch,
            roll: 0.0,
            fov_y: frame.camera.fov_y,
            near: frame.camera.near,
        });
        self.seen.block = (bx, by);
        // The placed objects' effects, lit as the frame's objects are.
        let l = frame.light;
        let light = auberean_world::color::Color::new(
            l.direct_color.r * l.direct_brightness,
            l.direct_color.g * l.direct_brightness,
            l.direct_color.b * l.direct_brightness,
            1.0,
        );
        self.placed_effects
            .see_from(&auberean_draw::effects::PlacedSight {
                eye,
                home: (bx, by),
                block_length: BLOCK_LENGTH,
                scale: auberean_draw::mesh::scenery_scale(self.builder.scenery_draw_distance),
                model_detail: usize::try_from(self.builder.model_detail).unwrap_or(2),
            });
        self.placed_effects.draw(&self.systems, now, light);
        if !carried.is_empty() {
            let carried = &carried;
            self.systems.light_queued(now, |home, at| {
                auberean_draw::scene::SceneBuilder::particle_lights_outdoors(
                    home, at, &outside, carried, light,
                )
            });
        }
        let axes = auberean_draw::particles::leaned_camera_axes(camera.heading, camera.pitch, 0.0);
        frame
            .objects
            .extend(self.systems.batches((bx, by), BLOCK_LENGTH, now, eye, axes));
        self.carried = carried;
        self.status.batches = frame.objects.len() + frame.terrain.len();
        self.status.vertices = (
            frame.terrain.iter().map(|b| b.vertices.len()).sum(),
            frame.objects.iter().map(|b| b.vertices.len()).sum(),
        );
        self.world_was_drawn = true;
        self.portal_frame = None;
        self.prepared = Some(true);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a developer's camera flown through the world with no server)
    use super::*;

    #[test]
    fn a_position_starts_the_camera_a_little_above_the_place_facing_its_way() {
        let c = parse_start("0x6FA90015 [45.24 90.51 85.26] 178").expect("reads");
        assert_eq!(c.block, (0x6F, 0xA9));
        assert!((c.position.z - (85.26 + ABOVE_A_BODY)).abs() < 1e-4);
        assert!((c.heading - 178.0).abs() < 1e-3, "{}", c.heading);
        assert!(!c.find_ground);
        // A turn of four parts: a quarter turn about up faces east or west.
        let q = parse_start("0x6FA90015 [45 90 85] 0.707107 0 0 -0.707107").expect("reads");
        assert!(
            (q.heading - 90.0).abs() < 0.01 || (q.heading - 270.0).abs() < 0.01,
            "{}",
            q.heading
        );
        // No turn faces north.
        let n = parse_start("0x6FA90015 [45 90 85]").expect("reads");
        assert!(n.heading.abs() < 1e-6);
    }

    #[test]
    fn the_copied_location_reads_back_as_the_camera_s_spot() {
        let camera = FreeCamera {
            block: (0x6F, 0xA9),
            position: Vec3::new(45.24, 90.51, 120.0),
            heading: 213.5,
            pitch: -20.0,
            find_ground: false,
        };
        let line = camera.location_line();
        assert!(line.starts_with("0x6FA90015 ["), "{line}");
        let back = parse_start(&line).expect("reads back");
        assert_eq!(back.block, camera.block);
        assert!((back.position.x - 45.24).abs() < 1e-4 && (back.position.y - 90.51).abs() < 1e-4);
        assert!((back.position.z - (120.0 + ABOVE_A_BODY)).abs() < 1e-3);
        assert!((back.heading - 213.5).abs() < 0.01, "{}", back.heading);
        assert_eq!(back.pitch, 0.0);
    }

    #[test]
    fn map_coordinates_start_the_camera_over_their_land_cell_to_find_the_ground() {
        let c = parse_start("You are at: 41.2S, 12.6E").expect("reads");
        assert!(c.find_ground);
        assert_eq!(map_text(c.cell()), "41.2S, 12.6E");
        // As a player writes them: no comma, small letters, east left out.
        for text in ["41.2s 12.6e", "41.2S 12.6"] {
            let d = parse_start(text).expect("reads");
            assert_eq!((d.block, d.position), (c.block, c.position), "{text}");
        }
        let w = parse_start("3N, 7W").expect("reads");
        assert_eq!(map_text(w.cell()), "3.0N, 7.0W");
    }

    #[test]
    fn text_that_is_no_location_is_refused_saying_why() {
        for text in [
            "Cragstone",
            "0x6FA90015 [1 2]",
            "0x6FA90015 [1 2 3] 1 2",
            "41.2Q, 12.6E",
            "",
        ] {
            let e = parse_start(text).expect_err(text);
            assert!(e.contains("expected a position"), "{text}: {e}");
        }
        assert!(parse_start("0xFF000001 [1 2 3]")
            .expect_err("no block")
            .contains("no cell"));
        assert!(parse_start("120N, 3E")
            .expect_err("past the edge")
            .contains("off the map"));
    }

    #[test]
    fn the_camera_flies_where_it_looks_and_is_held_over_the_world() {
        let mut c = FreeCamera {
            block: (0x6F, 0xA9),
            position: Vec3::new(150.0, 10.0, 100.0),
            heading: 90.0,
            pitch: 0.0,
            find_ground: false,
        };
        let ahead = Ways {
            forward: true,
            ..Ways::default()
        };
        // East across the block's edge, into the next block.
        c.fly(ahead, 20.0);
        assert_eq!(c.block, (0x70, 0xA9));
        assert!((c.position.x - 10.0).abs() < 1e-3 && (c.position.y - 10.0).abs() < 1e-3);
        // Up and down are straight, whatever it looks at.
        c.turn(0.0, -45.0);
        c.fly(
            Ways {
                up: true,
                ..Ways::default()
            },
            5.0,
        );
        assert!((c.position.z - 105.0).abs() < 1e-3 && (c.position.x - 10.0).abs() < 1e-3);
        // Left is level, to the north when it faces east.
        c.fly(
            Ways {
                left: true,
                ..Ways::default()
            },
            5.0,
        );
        assert!((c.position.y - 15.0).abs() < 1e-3 && (c.position.z - 105.0).abs() < 1e-3);
        // Past the world's north-east corner it stops at the edge.
        c.block = (BLOCKS - 1, BLOCKS - 1);
        c.turn(-45.0, 45.0);
        c.fly(ahead, 10_000.0);
        assert_eq!(c.block, (BLOCKS - 1, BLOCKS - 1));
        assert!(c.position.x < BLOCK_LENGTH && c.position.y < BLOCK_LENGTH);
        // Nor does it look past straight up.
        c.turn(0.0, 500.0);
        assert!((c.pitch - MOST_PITCH).abs() < f32::EPSILON);
    }
}
