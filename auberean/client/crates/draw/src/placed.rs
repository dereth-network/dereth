//! The objects the world places that play. Each look with a behavior table is played through one
//! body, kept for the look and stepped once however many objects show it: they all stand in its
//! pose, each where it is placed, and so all move together. The wind blows on that body as it does
//! on any other. A body plays once something showing its look has been drawn.
//!
//! The draws keep what they are until they are posed here, frame by frame. A frame that is never
//! posed shows every object as stored.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_animation::body::{Body, BodyRecords, PartPose};
use auberean_primitives::{DataId, Vec3};
use auberean_world::math::Mat4;
use auberean_world::records::WorldRecords;

use crate::body::BodyDrawer;
use crate::mesh::PosedList;
use crate::scene::{placed_box, Cull, ObjectBatch};

/// Below this many vertices to walk, a frame's poses are not worth waking other threads for.
const WORTH_SHARING: usize = 16_000;

/// A draw's share of a body: which body and part, and where the object stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Played {
    /// The look the body is made from.
    pub look: DataId,
    /// The part group of the look.
    pub group: u32,
    /// The mesh's place in the group's setup.
    pub setup_mesh: usize,
    /// Carries the pose to where the draw's object stands.
    pub frame: Mat4,
}

/// A look's body, and what it last stood in.
#[derive(Debug)]
struct Held {
    body: Body,
    poses: Vec<PartPose>,
    /// Something showing it was drawn since it last stepped.
    drawn: bool,
}

/// A mesh of a part in this frame's pose.
#[derive(Debug)]
struct Taken {
    list: Arc<PosedList>,
    lo: Vec3,
    hi: Vec3,
}

/// The bodies the world's placed objects play through, by look.
#[derive(Debug, Default)]
pub struct PlacedBodies {
    /// `None` for a look that makes no body.
    bodies: HashMap<u32, Option<Held>>,
    /// This frame's meshes in their poses, by look, group and draw's mesh and place.
    taken: HashMap<(u32, u32, u32, usize), Option<Taken>>,
}

impl PlacedBodies {
    /// `dt` seconds pass for every body something showing it was drawn since the last step.
    // D2-05768, D2-06487, D2-06489
    pub fn advance(&mut self, records: &dyn BodyRecords, dt: f32) {
        for held in self.bodies.values_mut().flatten() {
            if !std::mem::take(&mut held.drawn) {
                continue;
            }
            held.body.advance(records, dt);
            // What its animations set off goes nowhere: it has no object of its own to play it on.
            // D2-06437, D2-06532
            let _ = held.body.take_events();
            let _ = held.body.take_completions();
            held.body.triggers = Default::default();
            held.poses = held.body.pose(records);
        }
    }

    /// Stand every draw of `objects` that plays and is drawn this frame in its body's pose, the
    /// list of each mesh shared by all that show it, and its box about where the pose puts it.
    // D2-06554
    pub fn pose(
        &mut self,
        records: &WorldRecords,
        bodies: &dyn BodyRecords,
        drawer: &mut BodyDrawer,
        objects: &mut [ObjectBatch],
    ) {
        self.taken.clear();
        let mut lists: Vec<Arc<PosedList>> = Vec::new();
        let mut weight = 0;
        // The layers and fragments of a part follow one another and share all of this.
        let mut last: Option<(*const Played, Arc<PosedList>, Cull)> = None;
        for b in objects.iter_mut() {
            let Some(played) = b.played.as_ref() else {
                continue;
            };
            if !b.drawn() {
                continue;
            }
            let this = Arc::as_ptr(played);
            if let Some((_, list, cull)) = last.as_ref().filter(|l| l.0 == this) {
                b.transform = played.frame;
                b.posed = Some(list.clone());
                b.cull = *cull;
                continue;
            }
            let held = self
                .bodies
                .entry(played.look.0)
                .or_insert_with(|| {
                    let body = Body::assemble(bodies, played.look)?;
                    let poses = body.pose(bodies);
                    Some(Held {
                        body,
                        poses,
                        drawn: true,
                    })
                })
                .as_mut();
            let Some(held) = held else {
                continue;
            };
            held.drawn = true;
            let key = (played.look.0, played.group, b.mesh, played.setup_mesh);
            let taken = self.taken.entry(key).or_insert_with(|| {
                let at = held.body.parts.iter().position(|p| p.key == played.group)?;
                let pose = held.poses.get(at)?;
                let (list, lo, hi) =
                    drawer.placed_mesh(records, b.mesh, played.setup_mesh, pose)?;
                lists.push(list.clone());
                weight += b.vertices.len();
                Some(Taken { list, lo, hi })
            });
            let Some(taken) = taken else {
                continue;
            };
            let (lo, hi) = placed_box(taken.lo, taken.hi, &played.frame);
            let cull = moved_cull(b.cull, lo, hi);
            b.transform = played.frame;
            b.posed = Some(taken.list.clone());
            b.cull = cull;
            last = Some((this, taken.list.clone(), cull));
        }
        if weight >= WORTH_SHARING {
            crate::workers::share(lists, |list| {
                let _ = list.shared();
            });
        }
    }
}

/// A draw's way of being culled with its box now `lo` to `hi`; in a cell a box is taken twice its
/// size about its middle.
fn moved_cull(cull: Cull, lo: Vec3, hi: Vec3) -> Cull {
    let twice = || {
        let at = |l: f32, h: f32| (l + 0.5 * (l - h), h + 0.5 * (h - l));
        let (lx, hx) = at(lo.x, hi.x);
        let (ly, hy) = at(lo.y, hi.y);
        let (lz, hz) = at(lo.z, hi.z);
        (Vec3::new(lx, ly, lz), Vec3::new(hx, hy, hz))
    };
    match cull {
        Cull::Box { .. } => Cull::Box { lo, hi },
        Cull::InCell { cell, .. } => {
            let (lo, hi) = twice();
            Cull::InCell { cell, lo, hi }
        }
        Cull::LookedIn { block, cell, .. } => {
            let (lo, hi) = twice();
            Cull::LookedIn {
                block,
                cell,
                lo,
                hi,
            }
        }
        other => other,
    }
}
