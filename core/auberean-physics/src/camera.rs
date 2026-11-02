//! The chase camera: where it wants to be behind the body, and where it ends up once kept off the
//! world.
//!
//! The camera orbits the body: a direction (turned about the body's heading, pitched up from
//! level) and a distance from a look-at point above the body. Each frame three lines of sight
//! through the world's cells keep it off the world ([`sight_lines`]): one from the body up to the
//! look-at point against the walls, floors and ground, one from where that ended to where the
//! camera wants to be against those and what stands in the world, and inside, one 0.2 m out from
//! the surface the second stopped at. The camera stands where the last ended, in the cell it ended
//! in.

use dereth_physics::V3;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};

use crate::body::ENVIRONMENT_ONLY;
use crate::ethereal::{ObjectEthereality, SightLine};
use crate::geom::Ray;
use crate::interior::is_interior;
use crate::land::Land;
use crate::world::{World, PHYSICS_SHAPES};

/// How far off the surface it stopped at the camera stands inside, meters.
pub const OFF_THE_WALL: f32 = 0.2;

/// The camera's settings, as the options and the body set them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSettings {
    /// Meters from the look-at point.
    pub distance: f32,
    /// Radians above level.
    pub tilt: f32,
    /// The look-at point's height above the body's origin, meters: the top of the body.
    pub look_at_height: f32,
    /// Vertical field of view, degrees: 65 plus the camera effects' sum, which is never under 10.
    pub fov: f32,
    /// The highest and lowest pitch, radians.
    pub pitch_limit: f32,
}

/// The camera's own field of view, degrees, before its effects.
pub const BASE_FOV: f32 = 65.0;
/// The least the camera effects add to it.
pub const MIN_FOV_EFFECTS: f32 = 10.0;
/// The distance option's default, meters, before the body's height scale.
pub const DEFAULT_DISTANCE: f32 = 4.5;
/// The height of the player's body (its root setup's), meters, before its height scale.
pub const BODY_HEIGHT: f32 = 2.06;

impl Default for CameraSettings {
    /// At rest behind a body of scale 1: 4.5 m from the top of the body (2.06 m up), tilted 0.25
    /// rad, a 75 degree view, pitch within 1.3744467 rad of level.
    // D2-10689, D2-10707
    fn default() -> Self {
        Self::for_body(BODY_HEIGHT, 1.0)
    }
}

impl CameraSettings {
    /// The settings at rest behind a body `height` meters high (its root setup's) with height
    /// scale `scale_z`: the distance and the look-at height both scale with it.
    #[must_use]
    pub fn for_body(height: f32, scale_z: f32) -> Self {
        Self {
            distance: DEFAULT_DISTANCE * scale_z,
            tilt: 0.25,
            look_at_height: height * scale_z,
            fov: BASE_FOV + MIN_FOV_EFFECTS,
            pitch_limit: 1.374_446_7,
        }
    }
}

/// The chase camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub settings: CameraSettings,
    /// Radians the camera is turned about the body from straight behind it.
    pub yaw: f32,
    /// Radians above level.
    pub pitch: f32,
    pub distance: f32,
}

/// Where the camera is and what it looks at, in the body's landblock's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub eye: Vec3,
    pub look_at: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    /// Degrees above level (negative looks down).
    pub pitch: f32,
}

impl Camera {
    #[must_use]
    pub fn new(settings: CameraSettings) -> Self {
        Self {
            settings,
            yaw: 0.0,
            pitch: settings.tilt,
            distance: settings.distance,
        }
    }

    /// Where the camera wants to be for a body at `origin` facing `heading` (degrees clockwise
    /// from north): behind and above the look-at point.
    #[must_use]
    pub fn wanted(&self, origin: Vec3, heading: f32) -> (Vec3, Vec3) {
        let look = Vec3::new(origin.x, origin.y, origin.z + self.settings.look_at_height);
        let pitch = self
            .pitch
            .clamp(-self.settings.pitch_limit, self.settings.pitch_limit);
        let h = heading.to_radians() + self.yaw;
        let back = self.distance * math::cosf(pitch);
        let eye = Vec3::new(
            look.x - math::sinf(h) * back,
            look.y - math::cosf(h) * back,
            look.z + self.distance * math::sinf(pitch),
        );
        (look, eye)
    }

    /// The camera kept off the world: the line from the body to the look-at point stops at the
    /// ground; the line from there to the wanted place stops at the ground or a solid object.
    // D2-10662, D2-10676
    #[must_use]
    pub fn place(&self, land: &Land, block: LandblockId, origin: Vec3, heading: f32) -> View {
        let (look, eye) = self.wanted(origin, heading);
        let start = Vec3::new(origin.x, origin.y, origin.z + 0.0004);
        let look = land
            .trace(block, start, look, false)
            .map_or(look, |(p, _)| p);
        let eye = land.trace(block, look, eye, true).map_or(eye, |(p, _)| p);
        let d = Vec3::new(look.x - eye.x, look.y - eye.y, look.z - eye.z);
        let flat = (d.x * d.x + d.y * d.y).sqrt();
        View {
            eye,
            look_at: look,
            heading: math::atan2f(d.x, d.y).to_degrees().rem_euclid(360.0),
            pitch: math::atan2f(d.z, flat).to_degrees(),
        }
    }
}

/// The lines the camera is kept off the world by, from a body in `cell` standing at `origin` (in
/// its landblock's frame) to the place `wanted` the camera would stand.
#[derive(Clone, Copy, Debug)]
pub struct Lines<'a> {
    pub land: &'a Land,
    pub cell: dereth_primitives::CellId,
    pub origin: Vec3,
    /// From the body to the point the second line starts from (the look-at point and the shoulder
    /// offset).
    pub target_offset: Vec3,
    pub wanted: Vec3,
    /// Only the walls and the ground stop the second line, no object.
    pub env_only: bool,
}

/// How far along the line from `from` to `to` (in the landblock frame of `cell`'s block) the
/// objects placed in the world first meet it, as a fraction: inside, those standing in `cell` and
/// the cells it sees; outside, those of the nine landblocks around `cell`'s.
#[must_use]
pub fn placed_hit(
    land: &Land,
    cell: dereth_primitives::CellId,
    from: Vec3,
    to: Vec3,
) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut take = |t: Option<f32>| {
        if let Some(t) = t {
            if best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        }
    };
    if crate::interior::is_interior(cell) {
        let home = land.cell(cell)?;
        for id in std::iter::once(cell).chain(home.stab_list.iter().copied()) {
            for (at, shape) in land.cell_objects(id).iter().flat_map(|v| v.iter()) {
                take(shape.check_ray(&at.frame, from, to));
            }
        }
        return best;
    }
    let block = cell.landblock();
    for dx in -1i32..=1 {
        for dy in -1i32..=1 {
            let (Ok(x), Ok(y)) = (
                u8::try_from(i32::from(block.x()) + dx),
                u8::try_from(i32::from(block.y()) + dy),
            ) else {
                continue;
            };
            let Some(b) = land.block(x, y) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let shift = Vec3::new(
                dx as f32 * crate::rules::BLOCK_LENGTH,
                dy as f32 * crate::rules::BLOCK_LENGTH,
                0.0,
            );
            let (f, t) = (from.sub(shift), to.sub(shift));
            for (at, shape) in &b.objects {
                take(shape.check_ray(&at.frame, f, t));
            }
        }
    }
    best
}

/// Where the camera stands, and the cell it stands in, kept off the world as the client keeps it
/// each frame by three lines of sight through the world's cells and what stands in them
/// ([`World::los_check`]). The first runs from the body (0.0004 m up) along the target offset, from
/// the body's cell, and stops only at the walls, floors and ground. The second runs from where the
/// first ended, from the cell it ended in, to the wanted place, and stops at those and, unless only
/// the surroundings count, at what stands in the world that the camera does not pass through (the
/// physics types the camera passes, every object otherwise solid). Outside, the camera stands
/// where the second line ended, in the land cell there. Inside, a third line runs 0.2 m out from
/// where the second ended along the normal of the surface it stopped at (nowhere, when it stopped
/// at an object or at nothing), stopping at the same things, and the camera stands where it ends, in
/// the cell it ended in. Each place comes back in its cell's landblock's frame. None, the camera
/// keeping its last place, when a line has no loaded cell to start from.
// D2-10662
#[must_use]
pub fn sight_lines(world: &World, lines: &Lines<'_>) -> Option<Sighted> {
    let Lines {
        land,
        cell,
        origin,
        target_offset,
        wanted,
        env_only,
    } = *lines;
    let start = Vec3::new(origin.x, origin.y, origin.z + 0.0004);
    let mut first = SightLine::default();
    first.set_ray(Ray::from_offset(start, target_offset), cell.0);
    first.set_intersect_type(ENVIRONMENT_ONLY);
    if !world.los_check(land, &mut first) {
        return None;
    }
    let look = at(CellId(first.final_cell), first.end_point());
    let mut solid = ObjectEthereality::default();
    solid.set_to_solid();
    let passes = world.camera_ethereal();
    let mut second = SightLine::default();
    second.set_from_begin_and_end_position(&look, &at(cell, wanted));
    second.set_intersect_type(if env_only {
        ENVIRONMENT_ONLY
    } else {
        PHYSICS_SHAPES
    });
    second.set_los_params(&solid, passes);
    let mut traced = world.los_check(land, &mut second);
    let end = at(CellId(second.final_cell), second.end_point());
    let stands = if is_interior(end.cell) {
        let n = second.impact_normal;
        let mut third = SightLine::default();
        third.set_ray(
            Ray::from_offset(end.frame.origin, n.mul(OFF_THE_WALL)),
            end.cell.0,
        );
        third.set_intersect_type(PHYSICS_SHAPES);
        third.set_los_params(&solid, passes);
        traced &= world.los_check(land, &mut third);
        at(CellId(third.final_cell), third.end_point())
    } else {
        let (mut c, mut p) = (end.cell, end.frame.origin);
        if crate::cells::adjust_to_outside(&mut c, &mut p) {
            at(c, p)
        } else {
            end
        }
    };
    traced.then_some(Sighted { look, eye: stands })
}

/// Where the camera's lines of sight left it: the point it looks from the body to (where the first
/// line ended) and the place it stands, each with its cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighted {
    pub look: Position,
    pub eye: Position,
}

fn at(cell: CellId, origin: Vec3) -> Position {
    Position::new(cell, Frame::new(origin, Quat::IDENTITY))
}

/// Whether interior cell `cell` has a doorway out to the open world.
// D2-03515
#[must_use]
pub fn touches_outside(cell: &dereth_physics::source::EnvCellGeometry) -> bool {
    cell.portals
        .iter()
        .any(|p| p.other_cell_id == crate::cells::OUTDOORS)
}
