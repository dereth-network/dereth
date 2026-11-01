//! The chase camera: where it wants to be behind the body, and where it ends up once kept off the
//! world.
//!
//! The camera orbits the body: a direction (turned about the body's heading, pitched up from
//! level) and a distance from a look-at point above the body. Each frame it traces a line from the
//! body up to the look-at point against the ground and walls, then from there to where it wants to
//! be against the ground, walls and solid objects; outdoors it stands where the second line stops.
//! Inside, the lines are traced against the walls, floors and ceilings of the body's cell and the
//! cells it sees, and where the second line stops on a surface a third goes 0.2 m out along the
//! surface's normal, and the camera stands there.

use dereth_physics::V3;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, Vec3};

use crate::land::Land;

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

    /// The camera inside interior cell `cell`: the first line stops at the cells' walls, floors and
    /// ceilings, the second at those and the solid parts of what stands in the cells, and where it
    /// stopped on a surface the camera stands 0.2 m out from it (where a third line from there
    /// reaches). Doorways do not stop a line.
    // D2-10662, D2-10676
    #[must_use]
    pub fn place_inside(
        &self,
        land: &Land,
        cell: dereth_primitives::CellId,
        origin: Vec3,
        heading: f32,
    ) -> View {
        use crate::interior::trace;
        let (look, eye) = self.wanted(origin, heading);
        let start = Vec3::new(origin.x, origin.y, origin.z + 0.0004);
        let look = trace(land, cell, start, look, false).map_or(look, |(p, _)| p);
        let eye = match trace(land, cell, look, eye, true) {
            Some((p, n)) => {
                let out = Vec3::new(
                    p.x + n.x * OFF_THE_WALL,
                    p.y + n.y * OFF_THE_WALL,
                    p.z + n.z * OFF_THE_WALL,
                );
                trace(land, cell, p, out, true).map_or(out, |(q, _)| q)
            }
            None => eye,
        };
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

/// The first thing the line from `from` to `to` (in `cell`'s landblock frame) meets, as the fraction
/// of the way, the way back out, and whether it is an object: the walls, floors and ground (along
/// their normals) and, unless only the surroundings count, the solid parts of what stands in the
/// world, the objects placed in it and whatever `objects` says it meets (straight back along the
/// line).
fn first_met(
    land: &Land,
    cell: dereth_primitives::CellId,
    from: Vec3,
    to: Vec3,
    env_only: bool,
    objects: &dyn Fn(Vec3, Vec3) -> Option<f32>,
) -> Option<(f32, Vec3, bool)> {
    let along = to.sub(from);
    let len = along.mag2().sqrt();
    let mut hit: Option<(f32, Vec3, bool)> = if crate::interior::is_interior(cell) {
        crate::interior::trace(land, cell, from, to, !env_only)
    } else {
        land.trace(cell.landblock(), from, to, !env_only)
    }
    .map(|(p, n)| {
        let t = if len > 0.0 {
            p.sub(from).mag2().sqrt() / len
        } else {
            0.0
        };
        (t, n, false)
    });
    if !env_only && len > 0.0 {
        let back = Vec3::new(-along.x / len, -along.y / len, -along.z / len);
        for t in [placed_hit(land, cell, from, to), objects(from, to)]
            .into_iter()
            .flatten()
        {
            if hit.is_none_or(|(b, _, _)| t < b) {
                hit = Some((t, back, true));
            }
        }
    }
    hit
}

/// Where the camera stands, kept off the world as the client keeps it each frame. The first line
/// runs from the body (0.0004 m up) along the target offset and stops at the walls, floors and
/// ground only. The second runs from where the first stopped to the wanted place and stops at
/// those and, unless only the surroundings count, at the solid parts of what stands in the world,
/// the objects placed in it, and whatever `objects` says it meets (a fraction of the way, as
/// [`placed_hit`] gives). Outside, the camera stands where the second line stopped. Inside, where
/// the second line stopped on a wall a third line runs 0.2 m out from it along the wall's normal,
/// stopping at the same things, and the camera stands where that one stops; where it stopped on an
/// object the camera stands there (the client's third line starts on the object and goes
/// nowhere). None when the body's cell is not loaded: the camera keeps its last place.
// D2-10662
#[must_use]
pub fn keep_off_the_world(
    lines: &Lines<'_>,
    objects: &dyn Fn(Vec3, Vec3) -> Option<f32>,
) -> Option<Vec3> {
    use crate::interior::{is_interior, trace};
    let Lines {
        land,
        cell,
        origin,
        target_offset,
        wanted,
        env_only,
    } = *lines;
    let inside = is_interior(cell);
    if inside && land.cell(cell).is_none() {
        return None;
    }
    let block = cell.landblock();
    let start = Vec3::new(origin.x, origin.y, origin.z + 0.0004);
    let look = start.add(target_offset);
    let look = if inside {
        trace(land, cell, start, look, false)
    } else {
        land.trace(block, start, look, false)
    }
    .map_or(look, |(p, _)| p);
    let Some((t, n, object)) = first_met(land, cell, look, wanted, env_only, objects) else {
        return Some(wanted);
    };
    let p = look.add(wanted.sub(look).mul(t));
    if !inside || object {
        return Some(p);
    }
    let out = p.add(n.mul(OFF_THE_WALL));
    Some(
        first_met(land, cell, p, out, false, objects)
            .map_or(out, |(t, _, _)| p.add(out.sub(p).mul(t))),
    )
}

/// Whether interior cell `cell` has a doorway out to the open world.
// D2-03515
#[must_use]
pub fn touches_outside(cell: &dereth_physics::source::EnvCellGeometry) -> bool {
    cell.portals
        .iter()
        .any(|p| p.other_cell_id == crate::cells::OUTDOORS)
}

/// The cell the camera at `point` stands in, which chooses what a frame of the world draws:
/// `cell` is the cell the camera's sight starts from (the body's), `point` in its landblock's
/// frame. From an interior cell, that cell or one it sees that holds the point; else, when one of
/// them has a doorway out to the open world, the land cell under the point outside (the camera
/// has gone out through the doorway), and otherwise `cell` itself (the camera keeps to the rooms,
/// as the client's own camera, moved from cell to cell through their doorways, always does). From
/// a land cell, the land cell under the point, unless an interior cell of its block with a doorway
/// out holds the point (the camera has slipped into a room through its doorway). The cell comes
/// back with the point carried into its landblock's frame; `None` when the point is off the map or
/// the interior cell is not loaded.
#[must_use]
pub fn camera_cell(
    land: &Land,
    cell: dereth_primitives::CellId,
    point: Vec3,
) -> Option<(dereth_primitives::CellId, Vec3)> {
    use crate::interior::is_interior;
    use dereth_physics::cell::CellResolver;
    let resolver = CellResolver::new(land);
    let (mut id, mut at) = (cell, point);
    let inside = is_interior(cell);
    if inside {
        let start = resolver.get_visible(cell)?;
        if let Some(child) = resolver.find_visible_child_cell(&start, point) {
            let to = child.id();
            let off = dereth_physics::landdefs::get_block_offset(cell, to);
            return Some((to, point.sub(off)));
        }
        let home = land.cell(cell)?;
        let leads_out = std::iter::once(cell)
            .chain(home.stab_list.iter().copied())
            .any(|id| land.cell(id).is_some_and(|c| touches_outside(&c)));
        if !leads_out {
            return Some((cell, point));
        }
    }
    if !crate::cells::adjust_to_outside(&mut id, &mut at) {
        return None;
    }
    if inside {
        return Some((id, at));
    }
    let block = id.0 & 0xFFFF_0000;
    for room in land.interior_cells_of(block) {
        let Some(geom) = land.cell(room) else {
            continue;
        };
        if !touches_outside(&geom) {
            continue;
        }
        if let Some(c) = resolver.get_visible(room) {
            if c.point_in_cell(at) {
                return Some((room, at));
            }
        }
    }
    Some((id, at))
}
