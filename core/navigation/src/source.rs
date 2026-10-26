//! What the planner reads, and the body it plans for.

use std::sync::Arc;

use dereth_physics::land::LandblockCollision;
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry, SetupGeometry};
use dereth_physics::LandSource;
use dereth_primitives::{CellId, DataId, Frame, LandblockId};

/// One object placed outdoors: a tree, a rock, a fence, a statue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticPlacement {
    /// A setup (`0x02……`) or a bare graphics object (`0x01……`).
    pub id: DataId,
    /// Where, in its landblock's space.
    pub frame: Frame,
    /// Its scale: 1 for everything but generated scenery.
    pub scale: f32,
}

/// Everything the planner needs from outside itself: cells, the land, the buildings and the
/// collision half of the setups placed in them.
///
/// It reads only what is resident and never loads: the caller decides what is loaded, exactly as
/// it does for physics. A cell the source cannot answer for is treated as absent (see
/// [`crate::BuildReport::missing_cells`]). The outdoor half has empty defaults, so a source for
/// interiors alone implements two methods.
pub trait NavSource {
    /// One interior cell's collision geometry, as physics sees it.
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>>;

    /// The collision half of a setup (`0x02……`) or of a bare graphics object (`0x01……`, a
    /// one-part setup), with its parts, as a static object placed with that id collides.
    /// `None` makes the placement intangible to the planner, as it is to physics.
    fn setup(&self, id: DataId) -> Option<Arc<SetupGeometry>>;

    /// A landblock's terrain, or `None` where there is none.
    fn landblock(&self, _id: LandblockId) -> Option<Arc<LandblockCollision>> {
        None
    }

    /// The building standing in a land cell, if any.
    fn building(&self, _land_cell: CellId) -> Option<Arc<BuildingGeometry>> {
        None
    }

    /// The interior cells a land cell's building opens onto.
    fn building_cells(&self, _land_cell: CellId) -> Vec<CellId> {
        Vec::new()
    }

    /// Everything placed outdoors in a landblock: its generated scenery and its static objects.
    fn land_statics(&self, _block: LandblockId) -> Vec<StaticPlacement> {
        Vec::new()
    }
}

/// A source of outdoor placements, per landblock.
pub trait LandStatics {
    /// The placements of one landblock.
    fn statics(&self, block: LandblockId) -> Vec<StaticPlacement>;
}

impl<F: Fn(LandblockId) -> Vec<StaticPlacement>> LandStatics for F {
    fn statics(&self, block: LandblockId) -> Vec<StaticPlacement> {
        self(block)
    }
}

/// No outdoor placements: an interior-only source.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLandStatics;

impl LandStatics for NoLandStatics {
    fn statics(&self, _block: LandblockId) -> Vec<StaticPlacement> {
        Vec::new()
    }
}

/// A [`LandSource`] plus a setup resolver (and, for outdoor routes, the outdoor placements) is a
/// [`NavSource`]: the cells, the land and the buildings come from the same resident tables physics
/// walks.
#[derive(Debug)]
pub struct LandNavSource<L, F, G = NoLandStatics> {
    /// The cells, the land and the buildings.
    pub land: L,
    /// The setups.
    pub setups: F,
    /// The outdoor placements.
    pub land_statics: G,
}

impl<L, F> LandNavSource<L, F, NoLandStatics> {
    /// A source with no outdoor placements.
    pub const fn new(land: L, setups: F) -> Self {
        Self {
            land,
            setups,
            land_statics: NoLandStatics,
        }
    }
}

impl<L, F, G> NavSource for LandNavSource<L, F, G>
where
    L: std::ops::Deref,
    L::Target: LandSource,
    F: Fn(DataId) -> Option<Arc<SetupGeometry>>,
    G: LandStatics,
{
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.land.env_cell(cell)
    }
    fn setup(&self, id: DataId) -> Option<Arc<SetupGeometry>> {
        (self.setups)(id)
    }
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        self.land.landblock(id)
    }
    fn building(&self, land_cell: CellId) -> Option<Arc<BuildingGeometry>> {
        self.land.building(land_cell)
    }
    fn building_cells(&self, land_cell: CellId) -> Vec<CellId> {
        self.land.building_cells(land_cell)
    }
    fn land_statics(&self, block: LandblockId) -> Vec<StaticPlacement> {
        self.land_statics.statics(block)
    }
}

/// The body a route is planned for: a stack of spheres standing on the floor, and how far it can
/// step.
#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    /// The widest sphere's radius.
    pub radius: f32,
    /// The lowest sphere centre above the feet.
    pub low_center: f32,
    /// The highest sphere centre above the feet.
    pub top_center: f32,
    /// How high a ledge the body can step up onto.
    pub step_up: f32,
    /// How far down the body follows the floor without leaving it.
    pub step_down: f32,
}

impl Agent {
    /// The body of a setup at `scale`: its collision spheres and step heights, scaled the way the
    /// object scales them.
    #[must_use]
    pub fn from_setup(g: &SetupGeometry, scale: f32) -> Self {
        let spheres = g.path_spheres();
        let widest = spheres.iter().map(|s| s.radius).fold(0.0_f32, f32::max);
        let radius = if widest > 0.0 { widest } else { g.radius } * scale;
        let top_center = spheres.iter().map(|s| s.center.z).fold(0.0_f32, f32::max) * scale;
        let low_center = spheres
            .iter()
            .map(|s| s.center.z * scale)
            .fold(top_center, f32::min);
        Self {
            radius,
            low_center,
            top_center,
            step_up: g.step_up_height * scale,
            step_down: g.step_down_height * scale,
        }
    }

    /// The heights above the floor of the sphere centres the body is tested with: its own, the
    /// lowest raised by `step_allowance` so that what it steps over is not in its way.
    #[must_use]
    pub fn test_heights(&self, step_allowance: f32) -> Vec<f32> {
        let low = self.low_center + step_allowance;
        if low >= self.top_center - 1e-3 {
            vec![low]
        } else {
            vec![low, self.top_center]
        }
    }

    /// The player's body: two 0.48 m spheres centred 0.475 m and 1.35 m above the feet, a
    /// 0.6 m step up and a 1.5 m step down (the human setup at scale 1).
    #[must_use]
    pub const fn human() -> Self {
        Self {
            radius: 0.48,
            low_center: 0.475,
            top_center: 1.35,
            step_up: 0.6,
            step_down: 1.5,
        }
    }
}

/// How the walkable space is sampled and how much room the route keeps.
#[derive(Debug, Clone, PartialEq)]
pub struct NavConfig {
    /// The body.
    pub agent: Agent,
    /// The spacing of the floor samples, in metres.
    pub spacing: f32,
    /// Extra room kept between the body and anything solid, beyond its radius.
    pub margin: f32,
    /// Solid geometry lower than this above the floor is treated as something the body steps over
    /// rather than walks into. Never more than the body's step-up height.
    pub step_allowance: f32,
    /// The largest rise between neighbouring samples the route will climb.
    pub max_rise: f32,
    /// The largest drop between neighbouring samples the route will descend.
    pub max_drop: f32,
    /// The extra cost, in metres, of passing a closed door (the route has to stop and open it).
    pub door_cost: f32,
    /// The extra cost, in metres, of passing through where a creature stands.
    pub creature_cost: f32,
    /// Whether the static objects baked into the cells are obstacles. Always true outside of a
    /// test that measures what they change.
    pub statics: bool,
    /// Whether the route may wade: the body can walk through water deeper than its ankles (only a
    /// landblock that is all water refuses it). Off, such water is left out.
    pub wade: bool,
    /// The extra cost of wading, per metre waded, in metres: wading is chosen only where it is
    /// clearly shorter than the way round.
    pub wade_cost: f32,
    /// How far an outdoor region reaches past the landblocks of a route's two ends, in landblocks.
    pub margin_blocks: i32,
    /// The widest an outdoor region grows, in landblocks on a side.
    pub max_blocks: i32,
}

impl NavConfig {
    /// The defaults for a body.
    #[must_use]
    pub fn for_agent(agent: Agent) -> Self {
        // Low objects and walkable tops are planned over: the body steps up onto anything its step
        // height reaches, and the walk check refines a leg it cannot actually take.
        let step_allowance = agent.step_up;
        let max_rise = agent.step_up;
        Self {
            agent,
            spacing: 0.25,
            margin: 0.07,
            step_allowance,
            max_rise,
            max_drop: max_rise,
            door_cost: 4.0,
            creature_cost: 30.0,
            statics: true,
            wade: true,
            wade_cost: 1.0,
            margin_blocks: 1,
            max_blocks: 5,
        }
    }
}

impl Default for NavConfig {
    fn default() -> Self {
        Self::for_agent(Agent::human())
    }
}
