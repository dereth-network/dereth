//! Flying that obeys collision: the camera as a small sphere swept through the client's own
//! physics, as the client sweeps its camera, against the ground, the walls and floors of
//! buildings' and dungeons' cells, buildings and every static and scenery object. It still flies:
//! there is no gravity, and nothing steps it down.
//!
//! The physics world is a body's, as the client's is: the body is built and never placed or
//! drawn, and only its world is used. Each landblock round the camera is given to it as the
//! client gives its resident blocks: its cells loaded, and its statics, its scenery and its
//! cells' statics made bodies.

use std::collections::BTreeSet;
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

/// How far the camera's sphere reaches, in meters: the client's own camera sphere.
pub const RADIUS: f32 = dereth_physics::globals::VIEWER_SPHERE_RADIUS;

/// The physics world the flying camera is swept through.
pub struct Collider {
    body: Character,
    statics: CellStaticObjects,
    loader: EnvCellLoader,
    /// The landblocks given to the world.
    pub blocks: BTreeSet<u16>,
    /// The cell the camera was left in, and where (world meters).
    cell: Option<(CellId, Vec3)>,
    /// Sweeps run, and how many stopped short of where the camera was flying to.
    pub sweeps: u64,
    pub stopped: u64,
}

impl std::fmt::Debug for Collider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collider")
            .field("blocks", &self.blocks)
            .field("sweeps", &self.sweeps)
            .field("stopped", &self.stopped)
            .finish_non_exhaustive()
    }
}

/// A landblock's south-west corner, in world meters.
fn origin_of(lb: u16) -> Vec3 {
    lyceum_foundry::pick::block_origin(lb)
}

impl Collider {
    /// A world over `store` with no landblock in it yet.
    ///
    /// # Errors
    /// The body the world belongs to will not build.
    pub fn new(
        store: &Arc<RetailDatStore>,
        region: &Region,
        cfg: &SceneConfig,
    ) -> Result<Self, String> {
        let body = world_build::body_for(store, region, cfg).map_err(|e| e.to_string())?;
        let mut statics = CellStaticObjects::new();
        statics.mesh_collision = cfg.mesh_collision;
        Ok(Self {
            body,
            statics,
            loader: EnvCellLoader::new(),
            blocks: BTreeSet::new(),
            cell: None,
            sweeps: 0,
            stopped: 0,
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
                let inside: Vec<_> = per_cell.into_iter().flatten().collect();
                self.statics.init(store, &mut self.body.world, &inside);
                if let Some(t) = terrain(block) {
                    let mesh = generate_landblock(&t, region, x, y, 1, Direction::InViewerBlock);
                    let land = world_build::land_content(store, region, &t, &mesh, x, y, true);
                    self.statics
                        .init(store, &mut self.body.world, &land.land_statics);
                }
            }
        }
    }

    /// Forget where the camera was: it was put somewhere, not flown there.
    pub fn lost(&mut self) {
        self.cell = None;
    }

    /// The cell world meters `p` stands in outdoors.
    fn outdoor_cell(p: Vec3) -> Option<(CellId, Vec3)> {
        let (lb, x, y) = crate::camera::landblock_of(p)?;
        let mut cell = CellId((u32::from(lb) << 16) | 1);
        let mut local = Vec3::new(x, y, p.z);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local).then_some((cell, local))
    }

    /// Sweep the camera's sphere from `from` to `to` (world meters), starting in the cell
    /// `indoor` when it is in one of a building's or a dungeon's: where it stops. A camera whose
    /// start the sphere does not fit (in a wall when collision was turned on) moves freely, so
    /// it is never held.
    pub fn sweep(&mut self, from: Vec3, to: Vec3, indoor: Option<CellId>) -> Vec3 {
        let start = match self.cell {
            Some((c, at)) if at == from => Some(c),
            _ => indoor.or_else(|| Self::outdoor_cell(from).map(|(c, _)| c)),
        };
        let Some(start) = start else {
            self.cell = None;
            return to;
        };
        let lb = u16::try_from(start.0 >> 16).unwrap_or(0);
        let o = origin_of(lb);
        let at = |p: Vec3| Position::new(start, Frame::new(p.sub(o), Quat::IDENTITY));
        self.sweeps += 1;
        let swept = self.body.world.sweep_sphere(
            self.body.handle,
            VIEWER_OBJECT_INFO_STATE,
            dereth_client_runtime::camera::viewer_sphere(),
            start,
            &at(from),
            &at(to),
        );
        let Some(t) = swept else {
            self.cell = None;
            return to;
        };
        let pos = t.sphere_path.curr_pos;
        let cell = t.sphere_path.curr_cell.unwrap_or(pos.cell);
        let here = origin_of(u16::try_from(pos.cell.0 >> 16).unwrap_or(0)).add(pos.frame.origin);
        if here.sub(to).magnitude() > 1e-3 {
            self.stopped += 1;
        }
        self.cell = Some((cell, here));
        here
    }
}
