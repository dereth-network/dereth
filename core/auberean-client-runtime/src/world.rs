//! The world the client keeps its objects in: the region the world is in, when the objects'
//! upkeep runs, and what the client asks of the scene for its objects.
//!
//! - **The region.** The player's creation names the region the world is in by its number; the
//!   number names the region's record through the region mapping. A new region is taken up by the
//!   scene ([`ClientWorld::take_region_change`]): its cells, landscape and ambient sounds start
//!   afresh. The same region named again changes nothing; a number with no record leaves the
//!   world where it was.
//! - **The upkeep.** Each frame the objects' upkeep runs (creations made from their records, the
//!   objects asked to be destroyed, the queues given up), unless the scene is waiting for the
//!   cells around the player ([`ClientWorld::blocking_for_cells`]).
//! - **The scene's side** ([`WorldScene`]): the physics object made for each game object the
//!   client creates, whether a physics object never moves or hangs from another, an object leaving
//!   visibility when the cell it stands in is let go of, the landscape's resources made again. The
//!   scene tells the client the cell each physics object enters or leaves
//!   ([`auberean_client_model::EventQueue::change_cell`]); the stand-ins it keeps (the camera's)
//!   and the objects it makes itself are in the client's objects too
//!   ([`auberean_client_model::EventQueue::create_dummy_object`],
//!   [`auberean_client_model::EventQueue::initialize_temporary_object`]).

use auberean_client_model::{EventQueue, Model, Object};
use auberean_protocol::types::{VisualDesc, WeenieDesc};

/// The mapping a region's number is looked up in for its record.
pub const REGION_MAPPING: u32 = 0x4000_0007;

/// The quality naming the physics record an object's physics object is made from.
pub const PHYSICS_RECORD: u32 = 1;

/// What the physics object of one of the client's game objects is made from.
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsObjOrder {
    pub id: u64,
    /// The physics record its qualities name (0 for none).
    pub physics_record: u32,
    /// Its weenie description: whether it moves follows from the qualities record its entity, or
    /// else its package, names.
    pub weenie: WeenieDesc,
    /// The visual description the physics object takes.
    pub visual: VisualDesc,
}

/// The scene's side of the client's objects.
pub trait WorldScene {
    /// Make the physics object `order` describes, in no cell yet; whether it was made.
    fn make_physics_obj(&mut self, order: &PhysicsObjOrder) -> bool;
    /// Whether object `id`'s physics object never moves.
    fn is_static(&self, id: u64) -> bool;
    /// Whether object `id`'s physics object hangs from another (asked of the stand-ins and the
    /// objects the client made itself; a game object's parent is the model's).
    fn has_parent(&self, id: u64) -> bool;
    /// Object `id`'s physics object leaves visibility: it is lost in the cell it stood in until
    /// that cell is loaded again.
    fn leave_visibility(&mut self, id: u64);
    /// The landscape's resources made again; whether they were.
    fn refresh_landscape(&mut self) -> bool;
}

/// The client's world: the region the world is in, and whether the objects' upkeep waits
/// for the scene. Dropping it lets go of the world.
// D2-10806, D2-05762
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientWorld {
    /// The current region's number (0 for none).
    region: u32,
    /// The region record the scene is to take up (0 to let the region go), once the region
    /// changed and until the scene takes it.
    region_change: Option<u32>,
    /// The scene is waiting for the cells around the player: the objects' upkeep does not run.
    pub blocking_for_cells: bool,
}

impl ClientWorld {
    /// A world in no region, its upkeep running.
    // D2-10799
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current region's number (0 for none).
    #[must_use]
    pub fn region(&self) -> u32 {
        self.region
    }

    /// The world is in region number `region` (0 for none); `load` finds a number's region record
    /// and that record's own number. A region other than the current one is taken up when its
    /// record is found (the scene is asked to take it up); 0 lets the current one go. Whether the
    /// world is in the region asked for: it was already, it let its region go, or the record was
    /// found.
    // D2-10804
    pub fn set_region(
        &mut self,
        region: u32,
        load: impl FnOnce(u32) -> Option<(u32, u32)>,
    ) -> bool {
        if region == self.region {
            return true;
        }
        if region == 0 {
            self.region = 0;
            self.region_change = Some(0);
            return true;
        }
        let Some((record, number)) = load(region) else {
            return false;
        };
        self.region = number;
        self.region_change = Some(record);
        true
    }

    /// The region record the scene is to take up since it last asked (0 to let its region go):
    /// its cells, landscape and ambient sounds start afresh from it.
    pub fn take_region_change(&mut self) -> Option<u32> {
        self.region_change.take()
    }

    /// The world was left: the next region named is taken up again.
    pub(crate) fn leave_region(&mut self) {
        self.region = 0;
        self.region_change = None;
    }

    /// The world's resources made again: the landscape's (the frills' are the device's). Whether
    /// they were.
    // D2-10805
    pub fn refresh(&self, scene: &mut dyn WorldScene) -> bool {
        scene.refresh_landscape()
    }
}

/// The physics object of the client's game object `id`, made by the scene from the physics record
/// its qualities name (`physics_record`, 0 for none), its weenie description and its visual
/// description. Whether it was made; an object the client does not have makes none.
// D2-05640
pub fn create_physics_obj(
    model: &Model,
    id: u64,
    physics_record: u32,
    scene: &mut dyn WorldScene,
) -> bool {
    let Some(o) = model.object(id) else {
        return false;
    };
    let order = PhysicsObjOrder {
        id,
        physics_record,
        weenie: o.weenie.clone(),
        visual: o.visual.clone(),
    };
    scene.make_physics_obj(&order)
}

/// Cell `cell` is let go of: every object standing in it whose physics object moves and hangs from
/// nothing leaves visibility. The objects that left, in id order.
// D2-05673
pub fn release_obj_cell(
    queue: &EventQueue,
    model: &Model,
    cell: u32,
    scene: &mut dyn WorldScene,
) -> Vec<u64> {
    let leaving: Vec<u64> = queue
        .objects_in_cell(cell)
        .into_iter()
        .filter(|id| {
            !scene.is_static(*id)
                && !model
                    .object(*id)
                    .map_or_else(|| scene.has_parent(*id), Object::has_parent)
        })
        .collect();
    for id in &leaving {
        scene.leave_visibility(*id);
    }
    leaving
}
