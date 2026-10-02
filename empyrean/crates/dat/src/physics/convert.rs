//! Decoded dat geometry to the shapes `dereth-physics` collides against.
//!
//! The conversions of decoded records are the shared adapters in `dereth-world-data`
//! (`dereth_world_data`), re-exported here under the names the server
//! uses. What stays here is the half that *reads* the dats: `dereth-world-data` reads through a
//! `RetailDatStore`, while the server reads through its [`DatManager`] (its decoded-object cache,
//! and a `FakeDats` in the unit tier), so [`first_degrade_mode`], [`setup_geometry_with_parts_at`]
//! and [`simple_setup_geometry`] are the shared functions' bodies over that reader, without the
//! client's statistics counters. `tests/all/real_content.rs` checks they build what the shared ones
//! build over the retail dats. Nothing here is ported from ACE.

use std::sync::Arc;

use dereth_assets::geometry::CellStruct;
use dereth_assets::{EnvCell, GfxObj, GfxObjDegradeInfo, Setup};
use dereth_physics::source::{EnvCellGeometry, SetupPart};
use dereth_physics::SetupGeometry;
use dereth_primitives::{CellId, Frame, Vec3};
use dereth_world_data::env_cells::{physics_geometry, DecodedCell};

pub use dereth_world_data::env_cells::gfxobj_physics_bsp;
pub use dereth_world_data::setup::{
    drawing_sphere, gfx_bound_box, setup_geometry, PLACEMENT_FRAME_DEFAULT,
};

use crate::DatManager;

/// One interior cell's collision geometry from its `EnvCell` and its environment's `CellStruct`:
/// the shared conversion, which takes the pair by value.
#[must_use]
pub fn env_cell_geometry(id: CellId, cell: &EnvCell, cs: &CellStruct) -> EnvCellGeometry {
    physics_geometry(&DecodedCell {
        id,
        cell: cell.clone(),
        structure: cs.clone(),
    })
}

/// The first degrade mode of the graphics object's degrade info.
#[must_use]
pub fn first_degrade_mode(dats: &DatManager, g: &GfxObj) -> Option<i32> {
    let id = g.did_degrade?;
    // An object from before Throne of Destiny names no degrade record; the one its own id implies
    // usually does not exist, and its absence is not a missing record.
    if id == dereth_assets::geometry::implicit_degrade_id(g.id)
        && !dats.portal_dat().contains_file(id.0)
    {
        return None;
    }
    let info = dats.portal_dat().read_from_dat::<GfxObjDegradeInfo>(id.0)?;
    info.degrades.first().map(|d| d.degrade_mode)
}

/// A setup's full collision geometry: [`setup_geometry`] plus each part's graphics-object BSP,
/// placed by placement frame `placement_id` (falling back to frame 0).
#[must_use]
pub fn setup_geometry_with_parts_at(
    dats: &DatManager,
    s: &Setup,
    placement_id: u32,
) -> SetupGeometry {
    let mut g = setup_geometry(s);
    let placement = s
        .placement_frames
        .get(&placement_id)
        .or_else(|| s.placement_frames.get(&0));
    let Some(placement) = placement else { return g };
    let mut parts = Vec::with_capacity(s.parts.len());
    for (i, part_id) in s.parts.iter().enumerate() {
        let Some(frame) = placement.frames.get(i).copied() else {
            break;
        };
        let gfx = dats.portal_dat().read_from_dat::<GfxObj>(part_id.0);
        let (physics_bsp, bound_box, drawing, degrade) = match gfx {
            Some(gfx) => (
                gfxobj_physics_bsp(&gfx).map(Arc::new),
                Some(gfx_bound_box(&gfx)),
                drawing_sphere(&gfx),
                first_degrade_mode(dats, &gfx),
            ),
            None => (None, None, None, None),
        };
        parts.push(SetupPart {
            placement_frame: frame,
            default_scale: s
                .default_scale
                .as_ref()
                .and_then(|d| d.get(i).copied())
                .unwrap_or(Vec3::new(1.0, 1.0, 1.0)),
            physics_bsp,
            bound_box,
            drawing_sphere: drawing,
            first_degrade_mode: degrade,
        });
    }
    g.parts = parts;
    g
}

/// The one-part setup the client builds around a bare graphics object (a `0x01` id where a setup
/// is expected).
#[must_use]
pub fn simple_setup_geometry(dats: &DatManager, gfxobj: u32) -> Option<SetupGeometry> {
    let g = dats.portal_dat().read_from_dat::<GfxObj>(gfxobj)?;
    let bsp = gfxobj_physics_bsp(&g).map(Arc::new);
    let sorting_sphere = bsp
        .as_ref()
        .and_then(|t| t.root().map(|n| n.sphere))
        .unwrap_or_default();
    Some(SetupGeometry {
        sorting_sphere,
        parts: vec![SetupPart {
            placement_frame: Frame::default(),
            default_scale: Vec3::new(1.0, 1.0, 1.0),
            physics_bsp: bsp,
            bound_box: Some(gfx_bound_box(&g)),
            drawing_sphere: drawing_sphere(&g),
            first_degrade_mode: first_degrade_mode(dats, &g),
        }],
        ..SetupGeometry::default()
    })
}
