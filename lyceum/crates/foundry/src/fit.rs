//! How a model's collision fits: a body walked at it from every side by the client's own
//! movement, and where each walk ends.

use std::sync::Arc;

use dereth_assets::GfxObj;
use dereth_physics::source::SetupPart;
use dereth_physics::{PhysicsWorld, ScriptedMotion, SetupGeometry, StaticLandSource, V3};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

mod measure;
mod read;
pub use measure::{measure, Model, Shape, SurfaceFit, VolumeFit};
pub use read::{check_files, load, Report};

/// One comparison of the same approach through two models.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comparison {
    pub candidate: Walk,
    pub original: Walk,
    pub candidate_stopped: bool,
    pub original_stopped: bool,
    pub separation: f32,
}

/// Walk a sphere of player width toward both models from the same directions.
///
/// A stop means at least 25 cm less forward progress than the unobstructed control.
/// Endpoints also capture sliding and stepping, even when both walks stop.
///
/// # Errors
/// Non-finite bounds, models too wide for the walk area, or an invalid direction count.
pub fn compare(
    candidate: &Arc<SetupGeometry>,
    original: &Arc<SetupGeometry>,
    reach: f32,
    directions: usize,
) -> Result<Vec<Comparison>, String> {
    if !reach.is_finite() || !(0.0..=30.0).contains(&reach) || !(4..=128).contains(&directions) {
        return Err("walks need a reach up to 30 meters and 4 to 128 directions".into());
    }
    let body = Arc::new(SetupGeometry {
        spheres: vec![dereth_physics::Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        radius: 0.48,
        height: 0.96,
        step_up_height: 0.4,
        step_down_height: 0.4,
        ..SetupGeometry::default()
    });
    let empty = Arc::new(SetupGeometry::default());
    Ok((0..directions)
        .map(|k| {
            let (start, heading) = approach(reach, k, directions);
            let control = walk(&empty, &body, start, heading);
            let candidate = walk(candidate, &body, start, heading);
            let original = walk(original, &body, start, heading);
            Comparison {
                candidate_stopped: control.end.sub(candidate.end).dot(heading) > 0.25,
                original_stopped: control.end.sub(original.end).dot(heading) > 0.25,
                separation: candidate.end.sub(original.end).mag2().sqrt(),
                candidate,
                original,
            }
        })
        .collect())
}

/// The ground the walks are on.
const GROUND: f32 = 20.0;

/// Where the model stands: the middle of a landblock, so no walk leaves the nine around it.
const AT: Vec3 = Vec3::new(96.0, 96.0, GROUND);

/// How far a walk steps each frame, at 30 frames a second: a run.
pub const STEP: f32 = 0.12;

/// How far from the model's edge a walk starts.
pub const LEAD: f32 = 2.0;

/// The longest walk, in frames.
const MOST_FRAMES: usize = 600;

/// One walk: where it started and where it ended, both from the model's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub start: Vec3,
    pub end: Vec3,
}

/// A model standing alone, colliding by `tree` (its physics tree in the client's form).
#[must_use]
pub fn standing(model: &GfxObj, tree: dereth_physics::BspTree) -> Arc<SetupGeometry> {
    let bound_box = dereth_world_data::setup::gfx_bound_box(model);
    let sphere = dereth_world_data::setup::drawing_sphere(model)
        .or_else(|| tree.nodes.first().map(|n| n.sphere))
        .unwrap_or_default();
    Arc::new(SetupGeometry {
        sorting_sphere: sphere,
        selection_sphere: sphere,
        radius: sphere.radius,
        height: bound_box.max.z,
        parts: vec![SetupPart {
            physics_bsp: Some(Arc::new(tree)),
            bound_box: Some(bound_box),
            drawing_sphere: Some(sphere),
            ..SetupPart::default()
        }],
        ..SetupGeometry::default()
    })
}

/// The land cell a point stands in.
fn cell_at(p: Vec3) -> CellId {
    let mut c = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut origin = p;
    dereth_physics::landdefs::adjust_to_outside(&mut c, &mut origin);
    c
}

/// The `k`th of `directions` walks evenly round a model reaching `reach` from its origin across
/// the ground: where it starts, [`LEAD`] beyond that, and which way it heads, at the middle.
#[must_use]
pub fn approach(reach: f32, k: usize, directions: usize) -> (Vec3, Vec3) {
    #[allow(clippy::cast_precision_loss)]
    let angle = std::f32::consts::TAU * k as f32 / directions as f32;
    let side = Vec3::new(math::cosf(angle), math::sinf(angle), 0.0);
    // A little off the middle, so no walk meets a face exactly square.
    let across = Vec3::new(-side.y, side.x, 0.0).mul(0.031);
    (side.mul(reach + LEAD).add(across), side.mul(-1.0))
}

/// A body walked at the middle of `model` from `directions` sides evenly round it (see
/// [`approach`]), each walk twice as long as the way in.
#[must_use]
pub fn walks(
    model: &Arc<SetupGeometry>,
    body: &Arc<SetupGeometry>,
    reach: f32,
    directions: usize,
) -> Vec<Walk> {
    (0..directions)
        .map(|k| {
            let (start, heading) = approach(reach, k, directions);
            walk(model, body, start, heading)
        })
        .collect()
}

/// One walk from `start` (from the model's origin) along `heading`.
#[must_use]
pub fn walk(
    model: &Arc<SetupGeometry>,
    body: &Arc<SetupGeometry>,
    start: Vec3,
    heading: Vec3,
) -> Walk {
    let path = path(model, body, start, heading);
    Walk {
        start,
        end: path.last().copied().unwrap_or(start),
    }
}

/// Where a walk from `start` along `heading` is after each frame, from the model's origin.
#[must_use]
pub fn path(
    model: &Arc<SetupGeometry>,
    body: &Arc<SetupGeometry>,
    start: Vec3,
    heading: Vec3,
) -> Vec<Vec3> {
    let mut land = StaticLandSource::linear();
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            land.add_flat_block(LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8), 10);
        }
    }
    let mut w = PhysicsWorld::new(Arc::new(land));
    let blocker = w.create(ObjectId(2), Arc::clone(model), false);
    let at_cell = cell_at(AT);
    w.enter_cell(blocker, at_cell);
    if let Some(o) = w.get_mut(blocker) {
        o.position = Position::new(at_cell, Frame::new(AT, Quat::IDENTITY));
    }
    w.calc_cross_cells(blocker, false);

    let from = AT.add(start);
    let cell = cell_at(from);
    let mover = w.create(ObjectId(1), Arc::clone(body), true);
    w.enter_cell(mover, cell);
    if let Some(o) = w.get_mut(mover) {
        o.position = Position::new(cell, Frame::new(from, Quat::IDENTITY));
        o.set_motion(Box::new(ScriptedMotion::new(Vec::new(), true)));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(mover, false);
    let length = 2.0 * start.dot(start).sqrt();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = ((length / STEP).ceil() as usize).min(MOST_FRAMES);
    let offsets = vec![Frame::new(heading.mul(STEP), Quat::IDENTITY); frames + 1];
    if let Some(o) = w.get_mut(mover) {
        o.set_motion(Box::new(ScriptedMotion::new(offsets, true)));
    }
    let mut t = 0.0;
    let mut out = Vec::with_capacity(frames);
    for _ in 0..frames {
        t += 1.0 / 30.0;
        w.use_time(LocalTime(t), false);
        let at = w.get(mover).map_or(from, |o| {
            // A position is held from the corner of its landblock.
            let lb = o.position.cell.landblock();
            let block = Vec3::new(
                (f32::from(lb.x()) - f32::from(0xA9_u8)) * 192.0,
                (f32::from(lb.y()) - f32::from(0xB4_u8)) * 192.0,
                0.0,
            );
            o.position.frame.origin.add(block)
        });
        out.push(at.sub(AT));
    }
    out
}
