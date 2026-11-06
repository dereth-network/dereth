//! An object's parts as the physics keeps them: the part groups its look puts together (a body, a
//! head hung from it...), each one setup's meshes with their transforms, and what is asked of them:
//! the setup's spheres, cylinders and heights, whether a part has a physics tree, the boxes around
//! what is drawn, collisions and rays through the parts' trees, and where a group hangs from its
//! parent.
//!
//! A part group ([`PartGroup`]) is a part array ([`PartArray`]: a setup, its parts, its animation
//! map and the transforms its bones take) with its key, the connection point of its parent's setup
//! it hangs from and the groups hung from it. Each part ([`PhysicsPart`]) is one mesh of the setup,
//! as the look swaps it, with its transform into the object's frame.
//!
//! Collisions and rays are tested against every part's physics tree in the object's own frame,
//! scaled by the object's scale: a part's own transform is for drawing and for the box around what
//! is drawn, never for collisions.

mod array;
mod group;
mod part;
mod tree;

use std::cell::RefCell;
use std::sync::Arc;

use auberean_assets::animation::AnimMap;
use auberean_assets::mesh::Mesh;
use auberean_assets::setup::Setup;
use auberean_assets::visual::{Appearance, VisualDesc};
use auberean_primitives::DataId;
use auberean_world::look::LookRecords;
use auberean_world::place::{LoadedSetup, PlaceRecords};
use auberean_world::records::WorldRecords;
use dereth_physics::geom::BspTree;

use crate::mesh::Trees;

pub(crate) use array::relative;
pub use array::{BonePoint, PartArray};
pub use group::PartGroup;
pub use part::{PartMesh, PhysicsPart};
pub use tree::{trace_ray, trace_ray_from_root};

/// The owner a part array and its parts have before they are given one.
pub const NO_OWNER: u64 = 0;

/// The records parts are put together from: besides placement's (setups, looks), the setups as
/// loaded, the meshes themselves, the animation maps, and whether a material or a material's
/// modifier is in the data.
pub trait PartRecords: PlaceRecords {
    /// Setup `id` as loaded: the same one for every part array made from it while the records keep
    /// it, so that what one array changes in its setup (its step-down height, its spheres) every
    /// other array of that setup sees.
    fn loaded_setup(&self, id: DataId) -> Option<Arc<LoadedSetup>>;

    /// A mesh, when it is in the data.
    fn mesh(&self, id: DataId) -> Option<Arc<Mesh>>;

    /// The physics tree of a mesh, when the mesh is in the data and has one.
    fn physics_tree(&self, id: DataId) -> Option<Arc<BspTree>> {
        self.mesh(id)
            .and_then(|m| crate::mesh::physics_tree(&m))
            .map(Arc::new)
    }

    /// An animation map, when it is in the data.
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>>;

    /// Whether a material or a material's modifier is in the data.
    fn has_material(&self, id: DataId) -> bool;

    /// What a part needs of mesh `id` ([`PartMesh::read`]), when the mesh is in the data.
    fn part_mesh(&self, id: DataId) -> Option<PartMesh> {
        PartMesh::read(self, id)
    }
}

impl PartRecords for WorldRecords {
    fn loaded_setup(&self, id: DataId) -> Option<Arc<LoadedSetup>> {
        WorldRecords::loaded_setup(self, id)
    }

    fn mesh(&self, id: DataId) -> Option<Arc<Mesh>> {
        self.get::<Mesh>(id.0).map(Arc::new)
    }

    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        self.get::<AnimMap>(id.0).map(Arc::new)
    }

    fn has_material(&self, id: DataId) -> bool {
        self.holds(id.0)
    }
}

/// The setup's pose of a kind: the last stored of the kind, as the setup's table finds it.
fn placement_frame(setup: &Setup, kind: u32) -> Option<&auberean_assets::setup::Pose> {
    setup.poses.iter().rev().find(|p| p.kind == kind)
}

/// Where things of a kind attach to the setup: the last stored of the kind.
fn connection_point(setup: &Setup, kind: u32) -> Option<&auberean_assets::setup::Location> {
    setup
        .connection_points
        .iter()
        .rev()
        .find(|l| l.kind == kind)
}

/// A bone's parent as the setup gives it: the stored index, all ones for a root or a bone past
/// the setup's.
fn parent_index(setup: &Setup, bone: usize) -> u32 {
    setup.bones.get(bone).map_or(u32::MAX, |b| b.parent)
}

/// The world's records with what a part needs of each mesh kept in a [`Trees`], so that putting
/// many objects together reads each mesh once.
pub struct CachedParts<'a> {
    records: &'a WorldRecords,
    trees: RefCell<&'a mut Trees>,
}

impl std::fmt::Debug for CachedParts<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedParts").finish_non_exhaustive()
    }
}

impl<'a> CachedParts<'a> {
    #[must_use]
    pub fn new(records: &'a WorldRecords, trees: &'a mut Trees) -> Self {
        Self {
            records,
            trees: RefCell::new(trees),
        }
    }
}

impl LookRecords for CachedParts<'_> {
    fn visual(&self, id: DataId) -> Option<Arc<VisualDesc>> {
        self.records.visual(id)
    }
    fn appearance(&self, id: DataId) -> Option<Arc<Appearance>> {
        self.records.appearance(id)
    }
}

impl PlaceRecords for CachedParts<'_> {
    fn setup(&self, id: DataId) -> Option<Arc<Setup>> {
        PlaceRecords::setup(self.records, id)
    }
    fn entity(&self, id: DataId) -> Option<Arc<auberean_assets::entity::Entity>> {
        PlaceRecords::entity(self.records, id)
    }
    fn mesh_kind(&self, id: DataId) -> Option<u32> {
        self.records.mesh_kind(id)
    }
}

impl PartRecords for CachedParts<'_> {
    fn loaded_setup(&self, id: DataId) -> Option<Arc<LoadedSetup>> {
        self.records.loaded_setup(id)
    }
    fn mesh(&self, id: DataId) -> Option<Arc<Mesh>> {
        PartRecords::mesh(self.records, id)
    }
    fn physics_tree(&self, id: DataId) -> Option<Arc<BspTree>> {
        self.trees
            .borrow_mut()
            .get(id.0, |m| self.records.get::<Mesh>(m))
    }
    fn anim_map(&self, id: DataId) -> Option<Arc<AnimMap>> {
        PartRecords::anim_map(self.records, id)
    }
    fn has_material(&self, id: DataId) -> bool {
        PartRecords::has_material(self.records, id)
    }
    fn part_mesh(&self, id: DataId) -> Option<PartMesh> {
        self.trees
            .borrow_mut()
            .part_mesh(id.0, |m| self.records.get::<Mesh>(m))
    }
}
