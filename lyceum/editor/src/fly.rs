//! Flying that obeys collision: the camera as a small sphere swept through the client's own
//! physics, as the client sweeps its camera, against the ground, the walls and floors of
//! buildings' and dungeons' cells, buildings and every static and scenery object but doors, which
//! it passes through, open or closed. What it flies into it slides along, as a walking body
//! slides along a wall: the part of the move into the surface is taken out and the rest kept, so
//! it glides along walls, floors, ceilings and slopes, and stops only where two surfaces meet
//! against the move, as in a corner. Moving along the ground or a floor, it steps up onto a small
//! rise it meets, a stair, a sill or a curb, as a walking body does, with a walking body's step
//! height. It still flies: there is no gravity, it stands on nothing, and nothing takes it back
//! down after a step.
//!
//! The physics world is a body's, as the client's is: the body is built and never placed or
//! drawn, and only its world is used. Each landblock round the camera is given to it as the
//! client gives its resident blocks: its cells loaded, and its statics, its scenery and its
//! cells' statics made bodies, but those drawn with a door's model.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use dereth_assets::world::CellLandblock;
use dereth_assets::Region;
use dereth_client_runtime::character::Character;
use dereth_client_runtime::scene::SceneConfig;
use dereth_client_runtime::world_build;
use dereth_dat::RetailDatStore;
use dereth_physics::globals::VIEWER_OBJECT_INFO_STATE;
use dereth_primitives::frame::V3;
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};
use dereth_world_data::env_cells::{CellStaticObjects, EnvCellLoader};
use lyceum_foundry::models::Category;
use lyceum_foundry::sql_ids::WeenieFacts;

/// How far the camera's sphere reaches, in meters: the client's own camera sphere.
pub const RADIUS: f32 = dereth_physics::globals::VIEWER_SPHERE_RADIUS;

/// The physics world the flying camera is swept through.
pub struct Collider {
    body: Character,
    statics: CellStaticObjects,
    loader: EnvCellLoader,
    /// The landblocks given to the world.
    pub blocks: BTreeSet<u16>,
    /// The models of doors, which are left out of the world.
    doors: BTreeSet<u32>,
    /// The cell the camera was left in, and where (world meters).
    cell: Option<(CellId, Vec3)>,
    /// Held fast inside something: passing through till it is clear.
    passing: bool,
    /// Sweeps run, and how many ended short of where the camera was flying to: stopped, or slid
    /// along something.
    pub sweeps: u64,
    pub stopped: u64,
    /// Sweeps whose end was farther off than any sweep can take the camera ([`thrown`]), and
    /// which left it where it was.
    pub thrown: u64,
}

impl std::fmt::Debug for Collider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collider")
            .field("blocks", &self.blocks)
            .field("sweeps", &self.sweeps)
            .field("stopped", &self.stopped)
            .field("thrown", &self.thrown)
            .finish_non_exhaustive()
    }
}

/// How far past where any sweep can take the camera a sweep's end may be, in meters: room for
/// rounding and for the lifts out of the ground.
const THROW: f32 = 1.0;

/// Whether a sweep that ended at `here`, flown from `from` to `to` by a body that steps up `step`
/// meters, took the camera farther than any sweep can. A slide only shortens a move, so across
/// the ground it ends no farther off than its flight; a step up lifts it, by no more than twice
/// the step height for each of the sweep's steps, so up or down it ends no farther off than its
/// flight and those lifts. One that ends more than [`THROW`] past either, or nowhere, is not
/// believed.
fn thrown(from: Vec3, to: Vec3, here: Vec3, step: f32) -> bool {
    let asked = to.sub(from).magnitude();
    let d = here.sub(from);
    let across = (d.x * d.x + d.y * d.y).sqrt();
    // The sweep's steps, each no longer than the sphere's radius.
    let steps = (asked / RADIUS).floor() + 1.0;
    let lifts = asked + steps * 2.0 * step;
    !(across.is_finite() && d.z.is_finite()) || across > asked + THROW || d.z.abs() > lifts + THROW
}

/// A landblock's south-west corner, in world meters.
fn origin_of(lb: u16) -> Vec3 {
    lyceum_foundry::pick::block_origin(lb)
}

/// The models doors are drawn with: the setups of the weenies (`by_setup`, as
/// [`lyceum_foundry::edit::Session::weenies_by_setup`] lists them) that are doors.
#[must_use]
pub fn door_models(by_setup: &BTreeMap<u32, Vec<WeenieFacts>>) -> BTreeSet<u32> {
    by_setup
        .iter()
        .filter(|(_, ws)| ws.iter().any(|w| Category::of_weenie(w) == Category::Door))
        .map(|(setup, _)| *setup)
        .collect()
}

impl Collider {
    /// A world over `store` with no landblock in it yet, which leaves out what is drawn with
    /// one of `doors`' models.
    ///
    /// # Errors
    /// The body the world belongs to will not build.
    pub fn new(
        store: &Arc<RetailDatStore>,
        region: &Region,
        cfg: &SceneConfig,
        doors: BTreeSet<u32>,
    ) -> Result<Self, String> {
        let body = world_build::body_for(store, region, cfg).map_err(|e| e.to_string())?;
        let mut statics = CellStaticObjects::new();
        statics.mesh_collision = cfg.mesh_collision;
        Ok(Self {
            body,
            statics,
            loader: EnvCellLoader::new(),
            blocks: BTreeSet::new(),
            doors,
            cell: None,
            passing: false,
            sweeps: 0,
            stopped: 0,
            thrown: 0,
        })
    }

    /// Give the world landblock `lb` and the eight round it, those it has not been given: each
    /// one's cells, and its statics, scenery and cells' statics as bodies. `terrain` reads a
    /// landblock's terrain as the world is drawn.
    pub fn reach(
        &mut self,
        store: &RetailDatStore,
        region: &Region,
        terrain: &dyn Fn(u16) -> Option<CellLandblock>,
        lb: u16,
    ) {
        use dereth_terrain::land::mesh::{generate_landblock, Direction};
        let (bx, by) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (x, y) = (bx + dx, by + dy);
                if !(0..0xFF).contains(&x) || !(0..0xFF).contains(&y) {
                    continue;
                }
                let Ok(block) = u16::try_from((x << 8) | y) else {
                    continue;
                };
                if !self.blocks.insert(block) {
                    continue;
                }
                self.body.land().load_block_cells(LandblockId(block));
                let decoded = self.loader.load_block(store, block);
                let (_, per_cell) = world_build::interior_content(&decoded, true);
                let inside: Vec<_> = per_cell
                    .into_iter()
                    .flatten()
                    .filter(|s| !self.doors.contains(&s.id.raw()))
                    .collect();
                self.statics.init(store, &mut self.body.world, &inside);
                if let Some(t) = terrain(block) {
                    let mesh = generate_landblock(&t, region, x, y, 1, Direction::InViewerBlock);
                    let land = world_build::land_content(store, region, &t, &mesh, x, y, true);
                    let land: Vec<_> = land
                        .land_statics
                        .into_iter()
                        .filter(|s| !self.doors.contains(&s.id.raw()))
                        .collect();
                    self.statics.init(store, &mut self.body.world, &land);
                }
            }
        }
    }

    /// Forget where the camera was: it was put somewhere, not flown there.
    pub fn lost(&mut self) {
        self.cell = None;
        self.passing = false;
    }

    /// The cell world meters `p` stands in outdoors.
    fn outdoor_cell(p: Vec3) -> Option<(CellId, Vec3)> {
        let (lb, x, y) = crate::camera::landblock_of(p)?;
        let mut cell = CellId((u32::from(lb) << 16) | 1);
        let mut local = Vec3::new(x, y, p.z);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local).then_some((cell, local))
    }

    /// Sweep the camera's sphere from `from` to `to` (world meters), starting in the cell
    /// `indoor` when it is in one of a building's or a dungeon's: where it ends, slid along what
    /// it met or stepped up onto a small rise. A camera whose start the sphere does not fit (in a wall when collision was turned
    /// on) moves freely, so it is never held. A camera inside something, which the client's own
    /// camera could step no way from, passes through, as a body does after a portal, until it
    /// is clear of it; then it meets things again. A sweep that would take it farther than any
    /// sweep can ([`thrown`]) leaves it where it was.
    pub fn sweep(&mut self, from: Vec3, to: Vec3, indoor: Option<CellId>) -> Vec3 {
        if self.passing {
            self.cell = None;
            let start = indoor.or_else(|| Self::outdoor_cell(to).map(|(c, _)| c));
            if start.is_none_or(|c| !self.held_fast(c, to)) {
                self.passing = false;
            }
            return to;
        }
        let start = match self.cell {
            Some((c, at)) if at == from => Some(c),
            _ => indoor.or_else(|| Self::outdoor_cell(from).map(|(c, _)| c)),
        };
        let Some(start) = start else {
            self.cell = None;
            return to;
        };
        self.sweeps += 1;
        let Some((cell, here)) = self.sweep_in(start, from, to, true) else {
            self.cell = None;
            return to;
        };
        let step = self
            .body
            .world
            .get(self.body.handle)
            .map_or(0.0, dereth_physics::PhysicsObj::step_up_height);
        if thrown(from, to, here, step) {
            self.thrown += 1;
            self.cell = Some((start, from));
            return from;
        }
        if here.sub(to).magnitude() > 1e-3 {
            self.stopped += 1;
            // Inside a wall the sphere may still slide along its faces: whether it is held is
            // asked of the client's own camera sweep, which stops at the first face it meets.
            if to.sub(from).magnitude() > 1e-3 && self.held_fast(start, from) {
                self.passing = true;
                self.cell = None;
                return to;
            }
        }
        self.cell = Some((cell, here));
        here
    }

    /// Whether the camera passes through things till it is clear of them.
    #[must_use]
    pub fn passing(&self) -> bool {
        self.passing
    }

    /// Whether the sphere at `p`, in cell `start`, is held fast inside something: a short step
    /// each way along each axis, stopped where it touches as the client's camera is, takes it
    /// nowhere. (Against a wall, a step away still goes.)
    fn held_fast(&mut self, start: CellId, p: Vec3) -> bool {
        const STEP: f32 = 0.05;
        [
            Vec3::new(STEP, 0.0, 0.0),
            Vec3::new(-STEP, 0.0, 0.0),
            Vec3::new(0.0, STEP, 0.0),
            Vec3::new(0.0, -STEP, 0.0),
            Vec3::new(0.0, 0.0, STEP),
            Vec3::new(0.0, 0.0, -STEP),
        ]
        .into_iter()
        .all(|d| {
            self.sweep_in(start, p, p.add(d), false)
                .is_some_and(|(_, here)| here.sub(p).magnitude() < 1e-3)
        })
    }

    /// The sphere swept from `from` to `to`, starting in cell `start`, sliding along what it
    /// meets (`glide`) or stopping where it touches, as the client's camera does: the cell it
    /// ends in and where (world meters); `None` when its start does not fit.
    fn sweep_in(
        &mut self,
        start: CellId,
        from: Vec3,
        to: Vec3,
        glide: bool,
    ) -> Option<(CellId, Vec3)> {
        let lb = u16::try_from(start.0 >> 16).unwrap_or(0);
        let o = origin_of(lb);
        let at = |p: Vec3| Position::new(start, Frame::new(p.sub(o), Quat::IDENTITY));
        let world = &mut self.body.world;
        let sweep = if glide {
            dereth_physics::PhysicsWorld::glide_sphere
        } else {
            dereth_physics::PhysicsWorld::sweep_sphere
        };
        let t = sweep(
            world,
            self.body.handle,
            VIEWER_OBJECT_INFO_STATE,
            dereth_client_runtime::camera::viewer_sphere(),
            start,
            &at(from),
            &at(to),
        )?;
        let pos = t.sphere_path.curr_pos;
        let cell = t.sphere_path.curr_cell.unwrap_or(pos.cell);
        let here = origin_of(u16::try_from(pos.cell.0 >> 16).unwrap_or(0)).add(pos.frame.origin);
        Some((cell, here))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's flying camera)
    use super::*;

    fn facts(kind: u32, setup: u32) -> WeenieFacts {
        WeenieFacts {
            name: Some("x".into()),
            setup: Some(setup),
            kind: Some(kind),
            ..WeenieFacts::default()
        }
    }

    #[test]
    fn a_sweep_is_believed_across_the_ground_to_its_flight_and_up_to_its_steps_lifts_and_not_to_a_place_far_off(
    ) {
        // A walking body's step height, and two meters south: seven steps of the sweep.
        let step = 0.6;
        let from = Vec3::new(100.0, 100.0, 50.0);
        let to = Vec3::new(100.0, 98.0, 50.0);
        let far = |here| thrown(from, to, here, step);
        // Slid short, stepped up a few times on the way, or lifted out of the ground.
        assert!(!far(Vec3::new(100.0, 99.0, 50.3)));
        assert!(!far(Vec3::new(100.0, 98.0, 53.5)));
        assert!(!far(Vec3::new(100.1, 97.9, 50.2)));
        // Tens of meters on along the slope it was gliding up, across or up, or nowhere.
        assert!(far(Vec3::new(100.0, 64.0, 71.0)));
        assert!(far(Vec3::new(100.0, 96.5, 50.0)));
        assert!(far(Vec3::new(100.0, 98.0, 62.0)));
        assert!(far(Vec3::new(f32::NAN, 98.0, 50.0)));
    }

    #[test]
    fn a_model_any_door_is_drawn_with_is_a_doors_and_no_other_is() {
        let by_setup = BTreeMap::from([
            (0x0200_19FF, vec![facts(19, 0x0200_19FF)]),
            (
                0x0200_0281,
                vec![facts(1, 0x0200_0281), facts(19, 0x0200_0281)],
            ),
            (0x0200_07DD, vec![facts(10, 0x0200_07DD)]),
        ]);
        assert_eq!(
            door_models(&by_setup),
            BTreeSet::from([0x0200_0281, 0x0200_19FF])
        );
    }
}
