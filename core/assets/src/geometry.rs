//! `GfxObj`, `Setup`, `Animation`, `Environment`.
//!
//! Record layouts are described in `docs/formats/10-gfxobj.md`, `docs/formats/11-setup.md`,
//! `docs/formats/12-animation.md` and `docs/formats/16-environment.md`. Independent reference
//! parsers decode 15,318 + 5,935 + 2,066 + 772 objects with zero trailing bytes.

use std::collections::BTreeMap;

use dereth_dat::{packobj::read_n, ContainerEra, Cursor, DbType};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::common::{decode_sphere, BspKind, BspTree, Polygon, Sphere, VertexArray};
use crate::error::AssetError;
use crate::hook::{read_hooks, AnimHook};
use crate::Decode;

// ---------------------------------------------------------------------------------------------
// 0x01 GfxObj
// ---------------------------------------------------------------------------------------------

/// A decoded `0x01` GfxObj: one renderable mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct GfxObj {
    pub id: DataId,
    /// bit 0 = physics polygons + BSP, bit 1 = drawing polygons + BSP, bit 3 = a degrade id.
    pub flags: u32,
    pub surfaces: Vec<DataId>,
    pub vertex_array: VertexArray,
    pub physics_polygons: Vec<Polygon>,
    pub physics_bsp: Option<BspTree>,
    pub sort_center: Vec3,
    pub polygons: Vec<Polygon>,
    pub drawing_bsp: Option<BspTree>,
    pub did_degrade: Option<DataId>,
}

impl Decode for GfxObj {
    const TYPE: DbType = DbType::GfxObj;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        decode_gfxobj(c, ContainerEra::Tod)
    }

    /// Before Throne of Destiny the counts are plain `u32`s rather than compressed, the polygons
    /// and BSP nodes carry the older alignment, there is no degrade id, and bit 2 would add a
    /// triangle-strip block (which no shipped record sets, and which is refused) before the
    /// record ends aligned to four bytes.
    fn decode_pre_tod(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        decode_gfxobj(c, ContainerEra::PreTod)
    }
}

fn decode_gfxobj(c: &mut Cursor<'_>, era: ContainerEra) -> Result<GfxObj, AssetError> {
    let pre_tod = era == ContainerEra::PreTod;
    let count = |c: &mut Cursor<'_>| -> Result<usize, AssetError> {
        Ok(if pre_tod {
            c.u32()? as usize
        } else {
            // The later count is a compressed integer, not a plain dword.
            c.compressed_u32()? as usize
        })
    };
    let id = c.data_id()?;
    let flags = c.u32()?;
    let n = count(c)?;
    let surfaces = read_n(c, n, Cursor::data_id)?;
    let vertex_array = VertexArray::decode_in(c, era)?;
    let mut physics_polygons = Vec::new();
    let mut physics_bsp = None;
    if flags & 1 != 0 {
        let n = count(c)?;
        physics_polygons = polygons(c, n, era)?;
        physics_bsp = Some(BspTree::decode_in(c, BspKind::Physics, era)?);
    }
    let sort_center = c.vec3()?;
    let mut polygons_ = Vec::new();
    let mut drawing_bsp = None;
    if flags & 2 != 0 {
        let n = count(c)?;
        polygons_ = polygons(c, n, era)?;
        drawing_bsp = Some(BspTree::decode_in(c, BspKind::Drawing, era)?);
    }
    let did_degrade = if pre_tod {
        if flags & 4 != 0 {
            return Err(AssetError::Unsupported {
                what: "triangle-strip block",
                value: flags,
            });
        }
        c.align_ptr();
        None
    } else if flags & 8 != 0 {
        Some(c.data_id()?)
    } else {
        None
    };
    Ok(GfxObj {
        id,
        flags,
        surfaces,
        vertex_array,
        physics_polygons,
        physics_bsp,
        sort_center,
        polygons: polygons_,
        drawing_bsp,
        did_degrade,
    })
}

fn polygons(c: &mut Cursor<'_>, n: usize, era: ContainerEra) -> Result<Vec<Polygon>, AssetError> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(Polygon::decode_in(c, era)?);
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// 0x02 Setup
// ---------------------------------------------------------------------------------------------

/// A cylinder-sphere. The serialized order is **`radius` before `height`**. The
/// shape is `dereth_primitives`', shared with the animation and physics crates.
pub use dereth_primitives::shape::CylSphere;

/// One entry of `holding_locations` / `connection_points`; the record is `dereth_primitives`', shared
/// with the animation crate.
pub use dereth_primitives::records::LocationEntry;

/// One `placement_frames` entry: a frame per part, plus hooks.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub frames: Vec<Frame>,
    pub hooks: Vec<AnimHook>,
}

/// A light attached to the setup; the record is `dereth_primitives`', shared with the animation crate.
pub use dereth_primitives::records::LightInfo;

/// A decoded `0x02` Setup: the parts of a composite object, where they hang and what they collide with.
#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub id: DataId,
    pub flags: u32,
    pub parts: Vec<DataId>,
    pub parent_index: Option<Vec<u32>>,
    pub default_scale: Option<Vec<Vec3>>,
    /// `flags & 4`.
    pub allow_free_heading: bool,
    /// `flags & 8`.
    pub has_physics_bsp: bool,
    pub holding_locations: BTreeMap<u32, LocationEntry>,
    pub connection_points: BTreeMap<u32, LocationEntry>,
    /// Eight keys have no known name (60, 99, 104, 105, 106, 120, 132, 1010). They are
    /// kept as raw `u32`s; naming them is the animation crate's problem.
    pub placement_frames: BTreeMap<u32, Placement>,
    pub cylspheres: Vec<CylSphere>,
    pub spheres: Vec<Sphere>,
    pub height: f32,
    pub radius: f32,
    /// `step_up_height` is read **before** `step_down_height`.
    pub step_up_height: f32,
    pub step_down_height: f32,
    pub sorting_sphere: Sphere,
    pub selection_sphere: Sphere,
    pub lights: BTreeMap<u32, LightInfo>,
    pub default_anim_id: DataId,
    pub default_script_id: DataId,
    pub default_mtable_id: DataId,
    pub default_stable_id: DataId,
    pub default_phstable_id: DataId,
}

impl Decode for Setup {
    const TYPE: DbType = DbType::Setup;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let flags = c.u32()?;
        let num_parts = c.u32()? as usize;
        let parts = read_n(c, num_parts, Cursor::data_id)?;
        let parent_index = if flags & 1 != 0 {
            Some(read_n(c, num_parts, Cursor::u32)?)
        } else {
            None
        };
        let default_scale = if flags & 2 != 0 {
            Some(read_n(c, num_parts, Cursor::vec3)?)
        } else {
            None
        };

        let holding_locations = location_table(c)?;
        let connection_points = location_table(c)?;

        let n = c.u32()? as usize;
        let mut placement_frames = BTreeMap::new();
        for _ in 0..n {
            let key = c.u32()?;
            let frames = read_n(c, num_parts, Cursor::frame)?;
            let hooks = read_hooks(c)?;
            placement_frames.insert(key, Placement { frames, hooks });
        }

        let n = c.u32()? as usize;
        // Radius before height.
        let cylspheres = read_n(c, n, |c| {
            Ok(CylSphere {
                low_pt: c.vec3()?,
                radius: c.f32()?,
                height: c.f32()?,
            })
        })?;
        let n = c.u32()? as usize;
        let spheres = read_n(c, n, decode_sphere)?;

        let height = c.f32()?;
        let radius = c.f32()?;
        // step_up before step_down.
        let step_up_height = c.f32()?;
        let step_down_height = c.f32()?;
        let sorting_sphere = decode_sphere(c)?;
        let selection_sphere = decode_sphere(c)?;

        let n = c.u32()? as usize;
        let mut lights = BTreeMap::new();
        for _ in 0..n {
            let key = c.u32()?;
            lights.insert(
                key,
                LightInfo {
                    frame: c.placed_frame()?,
                    color_argb: c.u32()?,
                    intensity: c.f32()?,
                    falloff: c.f32()?,
                    cone_angle: c.f32()?,
                },
            );
        }

        let default_anim_id = c.data_id()?;
        let default_script_id = c.data_id()?;
        let default_mtable_id = c.data_id()?;
        let default_stable_id = c.data_id()?;
        let default_phstable_id = c.data_id()?;
        c.align_ptr();

        Ok(Self {
            id,
            flags,
            parts,
            parent_index,
            default_scale,
            allow_free_heading: flags & 4 != 0,
            has_physics_bsp: flags & 8 != 0,
            holding_locations,
            connection_points,
            placement_frames,
            cylspheres,
            spheres,
            height,
            radius,
            step_up_height,
            step_down_height,
            sorting_sphere,
            selection_sphere,
            lights,
            default_anim_id,
            default_script_id,
            default_mtable_id,
            default_stable_id,
            default_phstable_id,
        })
    }
}

fn location_table(c: &mut Cursor<'_>) -> Result<BTreeMap<u32, LocationEntry>, AssetError> {
    let n = c.u32()? as usize;
    let mut m = BTreeMap::new();
    for _ in 0..n {
        let key = c.u32()?;
        let part_id = c.u32()?;
        let frame = c.placed_frame()?;
        m.insert(key, LocationEntry { part_id, frame });
    }
    Ok(m)
}

// ---------------------------------------------------------------------------------------------
// 0x03 Animation
// ---------------------------------------------------------------------------------------------

/// One animation frame: a `Frame` per part plus hooks.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimFrame {
    pub frames: Vec<Frame>,
    pub hooks: Vec<AnimHook>,
}

/// A decoded `0x03` Animation: the per-frame part transforms and the hooks hung off them.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub id: DataId,
    pub flags: u32,
    pub num_parts: u32,
    pub num_frames: u32,
    /// `flags & 2` has no reader in the client. Decoded anyway.
    pub has_hooks: bool,
    /// `flags & 1`: a whole-object frame per animation frame.
    pub pos_frames: Option<Vec<Frame>>,
    pub part_frames: Vec<AnimFrame>,
}

impl Decode for Animation {
    const TYPE: DbType = DbType::Anim;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let flags = c.u32()?;
        let num_parts = c.u32()?;
        let num_frames = c.u32()?;
        let pos_frames = if flags & 1 != 0 {
            Some(read_n(c, num_frames as usize, Cursor::frame)?)
        } else {
            None
        };
        let mut part_frames = Vec::new();
        for _ in 0..num_frames {
            let frames = read_n(c, num_parts as usize, Cursor::frame)?;
            let hooks = read_hooks(c)?;
            part_frames.push(AnimFrame { frames, hooks });
        }
        Ok(Self {
            id,
            flags,
            num_parts,
            num_frames,
            has_hooks: flags & 2 != 0,
            pos_frames,
            part_frames,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0D Environment
// ---------------------------------------------------------------------------------------------

/// One cell of a `0x0D` Environment: its geometry, its portals and its BSP trees.
#[derive(Debug, Clone, PartialEq)]
pub struct CellStruct {
    pub cellstruct_id: u32,
    pub vertex_array: VertexArray,
    pub polygons: Vec<Polygon>,
    pub portals: Vec<u16>,
    pub cell_bsp: BspTree,
    pub physics_polygons: Vec<Polygon>,
    pub physics_bsp: BspTree,
    pub drawing_bsp: Option<BspTree>,
}

/// A decoded `0x0D` Environment: the indoor cell shapes a building is assembled from.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    pub id: DataId,
    pub cells: Vec<CellStruct>,
}

impl Decode for Environment {
    const TYPE: DbType = DbType::Environment;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        decode_environment(c, ContainerEra::Tod)
    }

    /// Before Throne of Destiny the shape is the same, and each polygon and BSP node carries the
    /// older alignment ([`Polygon::decode_in`], [`BspTree::decode_in`]).
    fn decode_pre_tod(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        decode_environment(c, ContainerEra::PreTod)
    }
}

fn decode_environment(c: &mut Cursor<'_>, era: ContainerEra) -> Result<Environment, AssetError> {
    let id = c.data_id()?;
    let n = c.u32()? as usize;
    let mut cells = Vec::new();
    for _ in 0..n {
        let cellstruct_id = c.u32()?;
        let num_polygons = c.u32()? as usize;
        let num_physics_polygons = c.u32()? as usize;
        let num_portals = c.u32()? as usize;
        let vertex_array = VertexArray::decode_in(c, era)?;
        let polys = polygons(c, num_polygons, era)?;
        let portals = read_n(c, num_portals, Cursor::u16)?;
        c.align_ptr();
        let cell_bsp = BspTree::decode_in(c, BspKind::Cell, era)?;
        let physics_polygons = polygons(c, num_physics_polygons, era)?;
        let physics_bsp = BspTree::decode_in(c, BspKind::Physics, era)?;
        let has_drawing = c.u32()?;
        let drawing_bsp = if has_drawing != 0 {
            Some(BspTree::decode_in(c, BspKind::Drawing, era)?)
        } else {
            None
        };
        c.align_ptr();
        cells.push(CellStruct {
            cellstruct_id,
            vertex_array,
            polygons: polys,
            portals,
            cell_bsp,
            physics_polygons,
            physics_bsp,
            drawing_bsp,
        });
    }
    Ok(Environment { id, cells })
}
