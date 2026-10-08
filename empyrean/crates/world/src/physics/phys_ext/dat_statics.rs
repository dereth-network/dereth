// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Common/Landblock.cs, Source/ACE.Server/Physics/Common/EnvCell.cs, Source/ACE.Server/Physics/Common/ObjectDesc.cs, Source/ACE.Server/Physics/PhysicsObj.cs, Source/ACE.Server/Physics/Animation/AFrame.cs
//! The collision bodies of the objects the cell dat places: every room and dungeon static
//! (`EnvCell.init_static_objects`), every landblock object (`Landblock.init_static_objs`) and the
//! generated landscape scenery (`Landblock.get_land_scenes`).
//!
//! ACE gives each one a static `PhysicsObj` (`makeObject(id, 0, false)`, `DatObject = true`) and
//! adds it to its cell with `add_obj_to_cell`, which registers it in the cells its geometry
//! reaches by the static cross-cell rule (`calc_cross_cells_static`). Here the placements are
//! ACE's, computed below, and the bodies are made and registered by the shared
//! `CellStaticObjects` (`dereth-world-data`), the client's own path, so both sides list a static
//! in the same cells by the same rule. A body made there has no server record: every server
//! list treats it as ACE treats a `DatObject` (never sent, never a server object, never a
//! visible object), and a mover that meets it reports an environment collision, as ACE's
//! transition does for a `Static` object.
//!
//! Where this differs from ACE:
//!
//! * ACE builds a room's statics when the room is first looked up (`LScape.get_landcell`), which
//!   loads every cell it sees and so, in practice, the whole building or dungeon. The server
//!   loads every interior cell of a landblock with the landblock, so their statics are built then
//!   too. A body can only meet a static in a cell its own movement has already looked up, so
//!   which statics a body meets is the same.
//! * A static whose geometry reaches into a landblock that is not loaded is first registered
//!   without that landblock's cells: ACE's lookup of such a cell loads the landblock
//!   (`LScape.get_landblock` on the server is `LandblockManager.GetLandblock`), which the server
//!   does not do. When that landblock loads, the static is registered again
//!   ([`register_into`]), which leaves it in the cells ACE's registration gives it.
//! * The scale of generated scenery is set after the body is registered, as ACE does
//!   (`add_obj_to_cell`, then `SetScaleStatic`): its cells are those of the unscaled object.
//! * Scenery is turned to its heading as the client turns it (V446). A scene object whose base
//!   orientation is pitched keeps its pitch about its own X axis, where ACE's heading step turns
//!   it about Y instead; every other object is turned as ACE turns it.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_physics::{PhysHandle, SetupGeometry};
use dereth_primitives::{CellId, DataId, Frame, LandblockId, Quat, Vec3};
use dereth_world_data::env_cells::{CellStatic, CellStaticObjects};
use empyrean_common::dotnet::numerics::{Quaternion, Vector3};
use empyrean_common::math;

use crate::entity::scenery;
use crate::World;

/// `PhysicsGlobals.EPSILON`.
const EPSILON: f32 = 0.0002;
/// `LandDefs.BlockLength`.
const BLOCK_LENGTH: f32 = 192.0;
/// `LandDefs.CellLength`.
const CELL_LENGTH: f32 = 24.0;
/// `LandDefs.VertexDim`.
const VERTEX_DIM: u32 = 9;
/// The literal ACE scales a 32-bit hash by (about 2^-32), as a `double`.
const INV_2_32: f64 = 2.328_306_4e-10;

/// Which of ACE's three initialisers made a static.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DatStaticKind {
    /// A room's or dungeon cell's static object (`EnvCell.init_static_objects`).
    Interior,
    /// One of the landblock information's objects (`Landblock.init_static_objs`).
    LandblockObject,
    /// Generated landscape scenery (`Landblock.get_land_scenes`).
    Scenery,
}

/// One dat static the server has a body for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DatStatic {
    pub kind: DatStaticKind,
    /// The cell it was added to, its setup (or graphics object) and its frame; the placement's
    /// scale is 1, the scale the body was registered at.
    pub placement: CellStatic,
    /// The scale set after registration (`SetScaleStatic`); 1 for everything but scenery.
    pub scale: f32,
    pub body: PhysHandle,
}

/// The server's dat-static bodies, by landblock.
#[derive(Debug, Default)]
pub struct DatStatics {
    pub(crate) bodies: CellStaticObjects,
    pub(crate) by_block: BTreeMap<u16, Vec<DatStatic>>,
}

impl DatStatics {
    /// The landblocks whose statics are built.
    #[must_use]
    pub fn landblocks(&self) -> Vec<u16> {
        self.by_block.keys().copied().collect()
    }

    /// How many bodies are live.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_block.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_block.is_empty()
    }

    /// What building the bodies did: placements offered, bodies made, and each refusal.
    #[must_use]
    pub fn stats(&self) -> dereth_world_data::env_cells::CellStaticStats {
        self.bodies.stats
    }
}

/// The statics of one landblock's dat records, in ACE's order: the landblock objects, the
/// scenery, then each interior cell's statics in cell order. The cells are not looked up here;
/// a placement in a cell the physics world does not hold gets no body when it is registered.
#[must_use]
pub fn placements(w: &World, landblock: u16) -> Vec<(DatStaticKind, CellStatic, f32)> {
    let block = LandblockId(landblock);
    let mut out = Vec::new();
    let info = w
        .dats
        .cell_dat()
        .read_from_dat::<empyrean_dat::file_types::LandblockInfo>(block.info_id().0);
    if let Some(info) = &info {
        for (s, scale) in landblock_objects(block, info) {
            out.push((DatStaticKind::LandblockObject, s, scale));
        }
    }
    // `UseSceneFiles` is true.
    for (s, scale) in land_scenes(w, block, info.as_deref()) {
        out.push((DatStaticKind::Scenery, s, scale));
    }
    let num_cells = info.as_ref().map_or(0, |i| i.num_cells);
    let land = w.physics.land();
    for i in 0..num_cells {
        let id = CellId((u32::from(landblock) << 16) | (0x100 + i));
        let Some(cell) = land.env_cell(id) else {
            continue;
        };
        for &(sid, frame) in &cell.static_objects {
            // `makeObject(0)` has no part array, so `add_obj_to_cell` leaves it without a cell
            // and it is destroyed.
            if sid == DataId(0) {
                continue;
            }
            out.push((
                DatStaticKind::Interior,
                CellStatic {
                    cell: id,
                    id: sid,
                    frame,
                    scale: 1.0,
                },
                1.0,
            ));
        }
    }
    out
}

/// `Landblock.init_static_objs`, the branch that creates them: each landblock object at its
/// frame, in the land cell its origin falls in. One whose origin is in another landblock finds
/// no cell of this one (`get_landcell` matches the whole id) and is dropped.
// ACE: Landblock.init_static_objs
fn landblock_objects(
    block: LandblockId,
    info: &empyrean_dat::file_types::LandblockInfo,
) -> Vec<(CellStatic, f32)> {
    let mut out = Vec::new();
    for o in &info.objects {
        let mut cell = block.cell(1);
        let mut origin = o.frame.origin;
        // `LandDefs.AdjustToOutside(position)`, which also moves an origin within epsilon of
        // the block's edge onto it; the object is added at the adjusted frame.
        if !dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin)
            || cell.landblock() != block
        {
            continue;
        }
        out.push((
            CellStatic {
                cell,
                id: o.id,
                frame: Frame::new(origin, o.frame.rotation),
                scale: 1.0,
            },
            1.0,
        ));
    }
    out
}

/// `Landblock.get_land_scenes`: the scenery generated on each of the block's 81 terrain
/// vertices, as ACE places it. Each is returned at its frame with the scale `ScaleObj` gives it.
// ACE: Landblock.get_land_scenes
fn land_scenes(
    w: &World,
    block: LandblockId,
    info: Option<&empyrean_dat::file_types::LandblockInfo>,
) -> Vec<(CellStatic, f32)> {
    let mut out = Vec::new();
    let Some(region) = w.dats.portal_dat().try_region_desc() else {
        return out;
    };
    let Some(land) = w.physics.land().landblock(block) else {
        return out;
    };
    let buildings = building_cells(block, info);
    let block_x = u32::from(block.0 >> 8) * 8;
    let block_y = u32::from(block.0 & 0xFF) * 8;

    for (i, &terrain) in land.terrain.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let terrain_type = usize::from(terrain >> 2 & 0x1F);
        let scene_type = usize::from(terrain >> 11);

        // ACE indexes the region's tables directly; an index out of range, or a scene type of -1,
        // would throw. The retail region has neither.
        let Some(&scene_info) = region
            .terrain_types
            .get(terrain_type)
            .and_then(|t| t.scene_types.get(scene_type))
        else {
            continue;
        };
        let Some(scenes) = usize::try_from(scene_info)
            .ok()
            .and_then(|k| region.scene_info.as_ref()?.get(k))
            .map(|d| &d.scenes)
        else {
            continue;
        };
        if scenes.is_empty() {
            continue;
        }

        let cell_x = i / VERTEX_DIM;
        let cell_y = i % VERTEX_DIM;
        let global_cell_x = cell_x + block_x;
        let global_cell_y = cell_y + block_y;

        let scene_idx = scenery::scene_index(scenes.len(), global_cell_x, global_cell_y);
        let scene_id = scenes[scene_idx];
        let Some(scene) = w
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::Scene>(scene_id.0)
        else {
            continue;
        };

        for (j, obj) in scene.objects.iter().enumerate() {
            let j = u32::try_from(j).unwrap_or(u32::MAX);
            let noise = object_noise(global_cell_x, global_cell_y, j);
            if !(noise < f64::from(obj.freq) && obj.weenie_obj == 0) {
                continue;
            }
            // pseudo-randomized placement
            let position = scenery::displace(obj, global_cell_x, global_cell_y, j);
            #[allow(clippy::cast_precision_loss)] // cell_x, cell_y <= 8
            let lx = cell_x as f32 * CELL_LENGTH + position.x;
            #[allow(clippy::cast_precision_loss)]
            let ly = cell_y as f32 * CELL_LENGTH + position.y;
            let mut loc = Vec3::new(lx, ly, obj.base_loc.origin.z);

            // ensure within landblock range, and not near road
            if lx < 0.0 || ly < 0.0 || lx >= BLOCK_LENGTH || ly >= BLOCK_LENGTH || land.on_road(loc)
            {
                continue;
            }

            // load scenery: `new Position(ID)` with this origin, then `AdjustToOutside`
            let mut cell = block.cell(1);
            if !dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut loc)
                || cell.landblock() != block
            {
                continue;
            }

            // check for buildings
            if buildings.contains(&cell.index()) {
                continue;
            }

            let Some(walkable) = land.find_terrain_poly(cell.index(), loc) else {
                continue;
            };
            let plane = walkable.plane;

            // ensure walkable slope
            if !check_slope(obj, plane.normal.z) {
                continue;
            }

            set_height(plane, &mut loc);

            // rotation
            let orientation = if obj.align != 0 {
                obj_align(obj, plane.normal)
            } else {
                rotate_obj(obj, global_cell_x, global_cell_y, j)
            };
            let frame = Frame::new(loc, quat_of(orientation));

            // build object
            let Some(geometry) = super::setup_geometry(w, obj.obj_id.0) else {
                // `makeObject` of an id with no setup has no part array: `obj_within_block`
                // tests the bare origin, and the body is refused when it is registered.
                if in_block(frame.origin, 0.0) {
                    out.push((placed(cell, obj.obj_id, frame), 1.0));
                }
                continue;
            };
            if !obj_within_block(&geometry, &frame) {
                continue;
            }

            let scale = scenery::scale_obj(obj, global_cell_x, global_cell_y, j);
            out.push((placed(cell, obj.obj_id, frame), scale));
        }
    }
    out
}

fn placed(cell: CellId, id: DataId, frame: Frame) -> CellStatic {
    CellStatic {
        cell,
        id,
        frame,
        scale: 1.0,
    }
}

/// The land cell indices of this block that hold a building (`SortCell.has_building`): ACE's
/// `init_buildings` adds each building to the land cell its origin falls in, when that cell is
/// one of this block's.
// ACE: Landblock.init_buildings
fn building_cells(
    block: LandblockId,
    info: Option<&empyrean_dat::file_types::LandblockInfo>,
) -> Vec<u16> {
    let mut cells = Vec::new();
    for b in info.map_or(&[][..], |i| &i.buildings[..]) {
        let mut cell = block.cell(1);
        let mut origin = b.frame.origin;
        if dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin)
            && cell.landblock() == block
            && !cells.contains(&cell.index())
        {
            cells.push(cell.index());
        }
    }
    cells
}

/// The frequency noise for object `j` of the scene at global cell (`x`, `y`). ACE's
/// `-1109124029 * globalCellX` is `long` arithmetic and the sum is narrowed to `uint`, so the
/// whole is the wrapping 32-bit value; unlike `Scenery.Load`'s, it includes the object index.
#[must_use]
pub fn object_noise(x: u32, y: u32, j: u32) -> f64 {
    let cell_x_mat = 0u32.wrapping_sub(1_109_124_029u32.wrapping_mul(x));
    let cell_y_mat = 1_813_693_831u32.wrapping_mul(y);
    let cell_mat = 1_360_117_743u32
        .wrapping_mul(x)
        .wrapping_mul(y)
        .wrapping_add(1_888_038_839);
    f64::from(
        cell_x_mat
            .wrapping_add(cell_y_mat)
            .wrapping_sub(cell_mat.wrapping_mul(23399u32.wrapping_add(j))),
    ) * INV_2_32
}

/// `Physics.Common.ObjectDesc.CheckSlope`: the terrain normal's Z within the object's bounds.
// ACE: Physics.Common.ObjectDesc.CheckSlope
#[must_use]
pub fn check_slope(obj: &dereth_assets::world::ObjectDesc, z: f32) -> bool {
    z >= obj.min_slope && z <= obj.max_slope
}

/// `PlaneExtensions.set_height`: drops the point onto the plane, unless the plane is nearly
/// vertical (ACE ignores the refusal and keeps the point's Z).
fn set_height(p: dereth_physics::geom::Plane, v: &mut Vec3) {
    if p.normal.z.abs() <= EPSILON {
        return;
    }
    v.z = -((v.y * p.normal.y + v.x * p.normal.x + p.d) / p.normal.z);
}

/// `Physics.Common.ObjectDesc.RotateObj`: the object's base orientation with its heading set
/// from the rotation hash (degrees, scaled by `MaxRotation`); a pitched base orientation keeps
/// its pitch, as the client turns it.
// ACE: Physics.Common.ObjectDesc.RotateObj
#[must_use]
pub fn rotate_obj(obj: &dereth_assets::world::ObjectDesc, x: u32, y: u32, k: u32) -> Quaternion {
    let base = quaternion_of(obj.base_loc.rotation);
    if obj.max_rot <= 0.0 {
        return base;
    }
    let hash = 1_813_693_831u32
        .wrapping_mul(y)
        .wrapping_sub(
            k.wrapping_add(63127).wrapping_mul(
                1_360_117_743u32
                    .wrapping_mul(y)
                    .wrapping_mul(x)
                    .wrapping_add(1_888_038_839),
            ),
        )
        .wrapping_sub(1_109_124_029u32.wrapping_mul(x));
    #[allow(clippy::cast_possible_truncation)]
    let degrees = (f64::from(hash) * INV_2_32 * f64::from(obj.max_rot)) as f32;
    set_heading(base, degrees)
}

/// `Physics.Common.ObjectDesc.ObjAlign`: the object's base orientation with its heading set to
/// the compass heading of the plane's downhill normal. Only the heading: the object stays
/// upright and faces down the slope, as retail places it.
// ACE: Physics.Common.ObjectDesc.ObjAlign
#[must_use]
pub fn obj_align(obj: &dereth_assets::world::ObjectDesc, normal: Vec3) -> Quaternion {
    let base = quaternion_of(obj.base_loc.rotation);
    let neg = Vector3::new(-normal.x, -normal.y, -normal.z);
    set_heading(base, get_heading(neg))
}

/// `float.ToRadians()`: `(float)(Math.PI / 180.0f * angle)`.
fn to_radians(angle: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let r = (std::f64::consts::PI / 180.0 * f64::from(angle)) as f32;
    r
}

/// `float.ToDegrees()`: `(float)(180.0f / Math.PI * rads)`.
fn to_degrees(rads: f32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let d = (180.0 / std::f64::consts::PI * f64::from(rads)) as f32;
    d
}

/// `Vec.NormalizeCheckSmall`: `true` (and the vector untouched) when it is shorter than epsilon,
/// otherwise the vector scaled by the reciprocal of its length.
fn normalize_check_small(v: &mut Vector3) -> bool {
    let dist = v.length();
    if dist < EPSILON {
        return true;
    }
    *v = *v * (1.0 / dist);
    false
}

/// `Vector3Extensions.get_heading`: the compass heading of the vector's XY direction, degrees.
// ACE: Vector3Extensions.get_heading
fn get_heading(v: Vector3) -> f32 {
    let mut normal = Vector3::new(v.x, v.y, 0.0);
    if normalize_check_small(&mut normal) {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let a = math::atan2(f64::from(normal.y), f64::from(normal.x)) as f32;
    (450.0 - to_degrees(a)) % 360.0
}

/// `AFrame.set_heading(degrees)`, as the client's frame sets a heading: the new direction is
/// `(sin, cos, M23)`, `M23` the Z of the current orientation's forward (local Y) axis, handed to
/// [`set_vector_heading`], so a pitched orientation keeps its pitch.
// ACE: AFrame.set_heading
fn set_heading(orientation: Quaternion, degrees: f32) -> Quaternion {
    let rads = to_radians(degrees);
    let q = orientation;
    // `Matrix4x4.CreateFromQuaternion`: M23 = 2(yz + wx).
    // DIVERGE (V446): ACE adds M13, the Z of the local X axis, as well; the client keeps the
    // forward axis's Z alone.
    let (yz, wx) = (q.y * q.z, q.x * q.w);
    let m23 = 2.0 * (yz + wx);
    #[allow(clippy::cast_possible_truncation)]
    let heading = Vector3::new(
        math::sin(f64::from(rads)) as f32,
        math::cos(f64::from(rads)) as f32,
        m23,
    );
    set_vector_heading(orientation, heading)
}

/// `AFrame.set_vector_heading`, as the client's frame points itself along a direction: the
/// orientation is pitched about its X axis by the arcsine of the direction's Z, then turned
/// about Z to the direction's compass heading, with no roll; normalised (`set_rotate`). A
/// direction shorter than epsilon leaves the orientation as it was.
// ACE: AFrame.set_vector_heading
fn set_vector_heading(orientation: Quaternion, heading: Vector3) -> Quaternion {
    let mut normal = heading;
    if normalize_check_small(&mut normal) {
        return orientation;
    }
    #[allow(clippy::cast_possible_truncation)]
    let a = math::atan2(f64::from(normal.y), f64::from(normal.x)) as f32;
    let z_deg = 450.0 - to_degrees(a);
    let z_rot = -to_radians(z_deg % 360.0);
    #[allow(clippy::cast_possible_truncation)]
    let x_rot = math::asin(f64::from(normal.z)) as f32;
    // DIVERGE (V446): ACE passes the pitch as `CreateFromYawPitchRoll`'s yaw, a turn about the Y
    // axis, so a direction with any Z leans the object sideways or pitches it the other way,
    // depending on its heading, where the client pitches it. With no Z the pitch is the identity
    // and the rotation is ACE's.
    let yaw = Quaternion::create_from_yaw_pitch_roll(0.0, 0.0, z_rot);
    let pitch = Quaternion::create_from_axis_angle(Vector3::new(1.0, 0.0, 0.0), x_rot);
    Quaternion::normalize(yaw * pitch)
}

fn quaternion_of(q: Quat) -> Quaternion {
    Quaternion::new(q.x, q.y, q.z, q.w)
}

fn quat_of(q: Quaternion) -> Quat {
    Quat::new(q.w, q.x, q.y, q.z)
}

/// `AFrame.LocalToGlobal(point)`: `Origin + Vector3.Transform(point, Orientation)`.
fn local_to_global(frame: &Frame, p: Vec3) -> Vec3 {
    let r = Vector3::transform(Vector3::new(p.x, p.y, p.z), quaternion_of(frame.rotation));
    Vec3::new(
        frame.origin.x + r.x,
        frame.origin.y + r.y,
        frame.origin.z + r.z,
    )
}

/// `LandDefs.InBlock`.
fn in_block(pos: Vec3, radius: f32) -> bool {
    if pos.x < radius || pos.y < radius {
        return false;
    }
    let block_radius = BLOCK_LENGTH - radius;
    pos.x < block_radius && pos.y < block_radius
}

/// `PhysicsObj.obj_within_block` for an object of `geometry` at `frame` and scale 1: a physics
/// mesh object's sorting sphere, else every cylinder sphere's low point, else the sorting sphere
/// of an object with spheres, else the bare origin, must clear the block's edges by its radius.
// ACE: PhysicsObj.obj_within_block
fn obj_within_block(geometry: &SetupGeometry, frame: &Frame) -> bool {
    let sorting = geometry.sorting_sphere;
    let glob_center = local_to_global(frame, sorting.center);

    if geometry.caches_physics_bsp() {
        if glob_center.x >= sorting.radius && glob_center.y >= sorting.radius {
            let block_radius = BLOCK_LENGTH - sorting.radius;
            if glob_center.x < block_radius {
                return glob_center.y < block_radius;
            }
        }
        return false;
    }

    if !geometry.cyl_spheres.is_empty() {
        for cyl in &geometry.cyl_spheres {
            let c = local_to_global(frame, cyl.low_pt);
            if c.x < cyl.radius || c.y < cyl.radius {
                return false;
            }
            let block_radius = BLOCK_LENGTH - cyl.radius;
            if c.x >= block_radius || c.y >= block_radius {
                return false;
            }
        }
        return true;
    }

    if geometry.spheres.is_empty() {
        in_block(frame.origin, 0.0)
    } else {
        in_block(glob_center, sorting.radius)
    }
}

/// Builds the bodies of one landblock's dat statics (`PostInit`'s `init_static_objs` and
/// `get_land_scenes`, and each interior cell's `init_static_objects`). Does nothing for a block
/// already built, or in a world whose land is not the cell dat's (a test's synthetic land).
pub(super) fn build(w: &mut World, landblock: u16) {
    if w.phys_ext.land.is_none() || w.phys_ext.dat_statics.by_block.contains_key(&landblock) {
        return;
    }
    let placements = placements(w, landblock);

    // The land cells' statics (the objects, then the scenery) and the interior cells' statics are
    // registered by the one call each, as every placement of a cell must be handed over together.
    let mut made = Vec::with_capacity(placements.len());
    for interior in [false, true] {
        let part: Vec<&(DatStaticKind, CellStatic, f32)> = placements
            .iter()
            .filter(|p| (p.0 == DatStaticKind::Interior) == interior)
            .collect();
        let statics: Vec<CellStatic> = part.iter().map(|p| p.1).collect();
        let bodies = {
            let World {
                physics, phys_ext, ..
            } = &mut *w;
            let super::PhysExtState {
                setups,
                synthetic_setups,
                dat_statics,
                ..
            } = phys_ext;
            dat_statics.bodies.init_with(
                &mut |id, _| {
                    synthetic_setups
                        .get(&id.0)
                        .map(Arc::clone)
                        .or_else(|| setups.get(id.0))
                },
                physics,
                &statics,
            )
        };
        for (p, body) in part.into_iter().zip(bodies) {
            let Some(body) = body else { continue };
            // `SetScaleStatic(scale)` after `add_obj_to_cell`.
            #[allow(clippy::float_cmp)]
            if p.2 != 1.0 {
                if let Some(o) = w.physics.get_mut(body) {
                    o.scale = p.2;
                }
            }
            made.push(DatStatic {
                kind: p.0,
                placement: p.1,
                scale: p.2,
                body,
            });
        }
    }
    w.phys_ext.dat_statics.by_block.insert(landblock, made);
    register_into(w, landblock);
}

/// Registers again every static of a neighbouring landblock whose registration reached a cell of
/// `landblock` while it was not loaded, now that it is, so that the static is listed there as
/// ACE lists it. ACE registers it in that cell at once, because its lookup of the cell loads the
/// landblock. As in ACE, the registration is at scale 1: the scale is set again afterwards.
///
/// A static whose landblock unloads and loads again is not registered in its neighbours' cells
/// again: unloading takes every registration in its land cells away (`release_shadow_objs`), as
/// ACE does, and nothing registers a static again once it is listed only in loaded cells.
fn register_into(w: &mut World, landblock: u16) {
    let reaches = |w: &World, h: PhysHandle| {
        w.physics.get(h).is_some_and(|o| {
            o.shadow_objects
                .iter()
                .any(|s| !s.cell_present && s.cell_id.landblock().0 == landblock)
        })
    };
    // The eight landblocks around it (a static reaches no further than the next landblock).
    let (x, y) = (i32::from(landblock >> 8), i32::from(landblock & 0xFF));
    let neighbours = (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dy| (x + dx, y + dy)))
        .filter(|&(nx, ny)| (nx, ny) != (x, y))
        .filter_map(|(nx, ny)| Some((u16::try_from(nx).ok()? << 8) | u16::try_from(ny).ok()?))
        .filter(|&b| b >> 8 <= 0xFE && b & 0xFF <= 0xFE);
    let near: Vec<(PhysHandle, f32)> = neighbours
        .filter_map(|b| w.phys_ext.dat_statics.by_block.get(&b))
        .flat_map(|statics| statics.iter())
        .filter(|s| reaches(w, s.body))
        .map(|s| (s.body, s.scale))
        .collect();
    for (h, scale) in near {
        if let Some(o) = w.physics.get_mut(h) {
            o.scale = 1.0;
        }
        w.physics.calc_cross_cells_static(h);
        if let Some(o) = w.physics.get_mut(h) {
            o.scale = scale;
        }
    }
}

/// Destroys the bodies of one landblock's dat statics, with their registrations in every cell.
pub(super) fn release(w: &mut World, landblock: u16) {
    if w.phys_ext.dat_statics.by_block.remove(&landblock).is_none() {
        return;
    }
    let World {
        physics, phys_ext, ..
    } = &mut *w;
    phys_ext
        .dat_statics
        .bodies
        .release_block(physics, landblock);
}
