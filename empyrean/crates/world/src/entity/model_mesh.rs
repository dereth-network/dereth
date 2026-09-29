// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/ModelMesh.cs
//! Port of `Source/ACE.Server/Entity/ModelMesh.cs`: an instanced static mesh.
//!
//! ACE builds these only from `Landblock.LoadMeshes`, which ACE never calls. The mesh itself
//! (`StaticMeshCache`, `StaticMesh`, `Polygon`, `ModelPolygon`) and `Physics.BoundingBox` are not
//! ported, so building the polygons and the bounding box are pointers; the instance data and its
//! position/rotation are ported.

use dereth_assets::world::ObjectDesc;
use dereth_primitives::Frame;
use empyrean_common::dotnet::numerics::{Quaternion, Vector2, Vector3};

// ACE: ModelMeshType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelMeshType {
    #[default]
    Building,
    LandObject,
    Scenery,
    Weenie,
}

// ACE: ModelMesh
/// An instanced static mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelMesh {
    /// The model id of the static mesh (`StaticMesh.ModelId`; the mesh is not ported).
    pub model_id: u32,
    /// The position and orientation from the original data.
    pub frame: Frame,
    /// The position backing store.
    position: Option<Vector3>,
    /// The rotation backing store.
    rotation: Option<Quaternion>,
    /// The cell offsets within the landblock.
    pub cell: Vector2,
    /// The scale of the model.
    pub scale: f32,
    /// For scenery object types.
    pub object_desc: Option<ObjectDesc>,
}

impl ModelMesh {
    // ACE: ModelMesh.ModelMesh
    /// Constructs a static mesh instance from a model and frame (also ACE's `Stab` and
    /// `BuildInfo` overloads, which pass their id and frame).
    #[must_use]
    pub fn new(model_id: u32, frame: Frame) -> Self {
        let mut this = Self {
            model_id,
            frame,
            position: None,
            rotation: None,
            cell: Vector2::default(),
            scale: 1.0,
            object_desc: None,
        };
        this.init(model_id, frame);
        this
    }

    // ACE: ModelMesh.Position
    /// The position of the model instance.
    #[must_use]
    pub fn position(&self) -> Vector3 {
        match self.position {
            None => Vector3::new(
                self.frame.origin.x,
                self.frame.origin.y,
                self.frame.origin.z,
            ),
            Some(p) => p,
        }
    }

    // ACE: ModelMesh.Position
    pub fn set_position(&mut self, value: Vector3) {
        self.position = Some(value);
    }

    // ACE: ModelMesh.Rotation
    /// The rotation of the model instance.
    #[must_use]
    pub fn rotation(&self) -> Quaternion {
        match self.rotation {
            None => {
                let q = self.frame.rotation;
                Quaternion::new(q.x, q.y, q.z, q.w)
            }
            Some(r) => r,
        }
    }

    // ACE: ModelMesh.Rotation
    pub fn set_rotation(&mut self, value: Quaternion) {
        self.rotation = Some(value);
    }

    // ACE: ModelMesh.Init
    /// Initializes a new mesh instance.
    pub fn init(&mut self, model_id: u32, frame: Frame) {
        self.get_mesh(model_id);
        self.frame = frame;

        self.build_polygons();
        self.build_bounding_box();
    }

    // ACE: ModelMesh.GetMesh
    /// `StaticMesh = StaticMeshCache.GetMesh(modelId)`. Only the model id is kept.
    pub fn get_mesh(&mut self, model_id: u32) {
        self.model_id = model_id;
        // ACE: StaticMeshCache.GetMesh
        // Not ported: nothing builds a `ModelMesh` (ACE never calls `Landblock.LoadMeshes`), so
        // there is no render mesh to cache.
    }

    // ACE: ModelMesh.BuildPolygons
    /// Builds the polygons for this model mesh into world space: one
    /// `new Polygon(polygon, Position, Rotation, Scale)` per static mesh polygon.
    pub fn build_polygons(&mut self) {
        // ACE: Entity.Polygon.Polygon
        // Not ported, as the mesh in `get_mesh`.
    }

    // ACE: ModelMesh.BuildBoundingBox
    /// `BoundingBox = new BoundingBox(this)`.
    pub fn build_bounding_box(&mut self) {
        // ACE: BoundingBox.BoundingBox
        // Not ported, as the mesh in `get_mesh`.
    }
}
