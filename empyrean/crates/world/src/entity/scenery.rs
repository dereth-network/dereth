// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Scenery.cs
//! Port of `Source/ACE.Server/Entity/Scenery.cs`: generates the scenery for a landblock.
//!
//! ACE calls `Load` only from `Landblock.LoadMeshes`, which ACE never calls. The placement
//! formulas are ported; `Load` and `Collision` need the model meshes' polygons and bounding boxes
//! (`StaticMesh`, `Polygon`, `Physics.BoundingBox`, not ported) and are pointers.
//!
//! C# evaluates the hash expressions in `uint` (an `int` constant times a `uint` is `uint`
//! arithmetic; ACE's one negative constant, `-1109124029 * globalCellX`, is `long` arithmetic that
//! is narrowed back to `uint`, the same value mod 2^32), so they are wrapping `u32` here.

use dereth_assets::world::ObjectDesc;
use empyrean_common::dotnet::numerics::Vector2;

use crate::entity::landblock_mesh::{LandblockMesh, CELL_DIM, CELL_SIZE};
use crate::entity::model_mesh::ModelMesh;

/// `2.3283064e-10` (a `double` literal: about 2^-32).
const INV_2_32: f64 = 2.328_306_4e-10;

/// `1813693831 * y - (k + c) * (1360117743 * y * x + 1888038839) - 1109124029 * x` over `uint`.
fn hash(x: u32, y: u32, k: u32, c: u32) -> u32 {
    1_813_693_831u32
        .wrapping_mul(y)
        .wrapping_sub(
            k.wrapping_add(c).wrapping_mul(
                1_360_117_743u32
                    .wrapping_mul(y)
                    .wrapping_mul(x)
                    .wrapping_add(1_888_038_839),
            ),
        )
        .wrapping_sub(1_109_124_029u32.wrapping_mul(x))
}

// ACE: Scenery.Load
/// Generates the scenery for a landblock.
///
/// Not ported: its only caller, `Landblock.LoadMeshes`, is never called (in ACE either). ACE's
/// scenery collision is its physics landblock's scenery (`Landblock.get_land_scenes`), which the
/// server does not build yet: no landscape scenery, landblock object or room furniture from the
/// dats has a collision body here (DIVERGENCES.md V443).
pub fn load() -> Vec<ModelMesh> {
    Vec::new()
}

/// `Load`'s scene pick for one terrain vertex: the index into a scene list of `scenes_count`
/// scenes at global cell (`global_cell_x`, `global_cell_y`).
#[must_use]
pub fn scene_index(scenes_count: usize, global_cell_x: u32, global_cell_y: u32) -> usize {
    let cell_mat = global_cell_y
        .wrapping_mul(
            712_977_289u32
                .wrapping_mul(global_cell_x)
                .wrapping_add(1_813_693_831),
        )
        .wrapping_sub(1_109_124_029u32.wrapping_mul(global_cell_x))
        .wrapping_add(2_139_937_281);
    let offset = f64::from(cell_mat) * INV_2_32;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let scene_idx = (scenes_count as f64 * offset) as i32;
    match usize::try_from(scene_idx) {
        Ok(i) if i < scenes_count => i,
        _ => 0,
    }
}

/// `Load`'s frequency noise for the objects of the scene at global cell (`x`, `y`). As in ACE,
/// the object index is not part of it.
#[must_use]
pub fn object_noise(x: u32, y: u32) -> f64 {
    // `var cellXMat = -1109124029 * globalCellX` is long; the uint narrowing makes it mod 2^32.
    let cell_x_mat = 0u32.wrapping_sub(1_109_124_029u32.wrapping_mul(x));
    let cell_y_mat = 1_813_693_831u32.wrapping_mul(y);
    let cell_mat = 1_360_117_743u32
        .wrapping_mul(x)
        .wrapping_mul(y)
        .wrapping_add(1_888_038_839);
    f64::from(
        cell_x_mat
            .wrapping_add(cell_y_mat)
            .wrapping_sub(cell_mat.wrapping_mul(23399)),
    ) * INV_2_32
}

// ACE: Scenery.Displace
/// Displaces a scenery object into a pseudo-randomized location. `ix`/`iy` are the global cell
/// offsets, `iq` the scene index of the object.
#[must_use]
pub fn displace(obj: &ObjectDesc, ix: u32, iy: u32, iq: u32) -> Vector2 {
    let loc = obj.base_loc.origin;

    #[allow(clippy::cast_possible_truncation)]
    let x = if obj.displace_x <= 0.0 {
        loc.x
    } else {
        (f64::from(hash(ix, iy, iq, 45773)) * INV_2_32 * f64::from(obj.displace_x)
            + f64::from(loc.x)) as f32
    };

    #[allow(clippy::cast_possible_truncation)]
    let y = if obj.displace_y <= 0.0 {
        loc.y
    } else {
        (f64::from(hash(ix, iy, iq, 72719)) * INV_2_32 * f64::from(obj.displace_y)
            + f64::from(loc.y)) as f32
    };

    let quadrant = f64::from(
        1_813_693_831u32
            .wrapping_mul(iy)
            .wrapping_sub(
                ix.wrapping_mul(
                    1_870_387_557u32
                        .wrapping_mul(iy)
                        .wrapping_add(1_109_124_029),
                ),
            )
            .wrapping_sub(402_451_965),
    ) * INV_2_32;

    if quadrant >= 0.75 {
        return Vector2::new(y, -x);
    }
    if quadrant >= 0.5 {
        return Vector2::new(-x, -y);
    }
    if quadrant >= 0.25 {
        return Vector2::new(-y, x);
    }

    Vector2::new(x, y)
}

// ACE: Scenery.ScaleObj
/// Returns the scale for a scenery object (`x`/`y` the global cell offsets, `k` the scene index).
#[must_use]
pub fn scale_obj(obj: &ObjectDesc, x: u32, y: u32, k: u32) -> f32 {
    let min_scale = obj.min_scale;
    let max_scale = obj.max_scale;

    #[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
    let scale = if min_scale == max_scale {
        max_scale
    } else {
        (empyrean_common::math::pow(
            f64::from(max_scale / min_scale),
            f64::from(hash(x, y, k, 32593)) * INV_2_32,
        ) * f64::from(min_scale)) as f32
    };

    scale
}

// ACE: Scenery.RotateObj
/// Returns the rotation for a scenery object.
#[must_use]
pub fn rotate_obj(obj: &ObjectDesc, x: u32, y: u32, k: u32) -> f32 {
    if obj.max_rot <= 0.0 {
        return 0.0;
    }

    #[allow(clippy::cast_possible_truncation)]
    let r = (f64::from(hash(x, y, k, 63127))
        * INV_2_32
        * f64::from(obj.max_rot)
        * f64::from(0.017_453_3f32)) as f32;
    r
}

// ACE: Scenery.OnRoad
/// Returns TRUE if x,y is located on a road cell. `terrain` is the landblock's `Terrain` list.
#[must_use]
pub fn on_road(_obj: &ObjectDesc, terrain: &[u16], x: f32, y: f32) -> bool {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let cell_x = f64::from(x / CELL_SIZE as f32).floor() as i32;
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let cell_y = f64::from(y / CELL_SIZE as f32).floor() as i32;
    // ACE-BUG: Terrain holds 9x9 vertices (Load walks it with VertexDim) but is indexed here with
    // CellDim (8), so the road flag comes from another vertex whenever cellX > 0 (dead code in ACE).
    let idx = usize::try_from(cell_x * CELL_DIM + cell_y).expect("ACE: Terrain index out of range"); // ensure within bounds?
    let terrain = terrain[idx];
    (terrain & 0x3) != 0 // TODO: more complicated check for within road range
}

// ACE: Scenery.Collision
/// Returns TRUE is object intersects with any models (`obj.BoundingBox.Intersect2D`).
pub fn collision(models: &[ModelMesh], _obj: &ModelMesh) -> bool {
    // ACE: BoundingBox.Intersect2D
    // Not ported, as `load` (the model meshes carry no bounding boxes).
    let _ = models;
    false
}

// ACE: Scenery.GetZ
/// Returns the landblock floor height for a scenery model.
#[must_use]
pub fn get_z(landblock_mesh: &LandblockMesh, model_mesh: &ModelMesh) -> f32 {
    // get the landblock x/y position
    #[allow(clippy::cast_precision_loss)]
    let x = model_mesh.cell.x * CELL_SIZE as f32 + model_mesh.position().x;
    #[allow(clippy::cast_precision_loss)]
    let y = model_mesh.cell.y * CELL_SIZE as f32 + model_mesh.position().y;

    landblock_mesh.get_z(Vector2::new(x, y))
}
