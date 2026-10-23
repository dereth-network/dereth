//! Read both halves of a static model at its resting placement.

use std::path::Path;
use std::sync::Arc;

use dereth_assets::{Decode, GfxObj, Polygon, Setup};
use dereth_dat::RetailDatStore;
use dereth_primitives::frame::{localtoglobal, V3};
use dereth_primitives::{DataId, Frame, Vec3};
use dereth_world_data::setup::{self, SetupPartStats};

use super::{compare, measure, Comparison, Model, Shape, SurfaceFit};

/// The surface fit and, when requested, a comparison against the unmodified model.
#[derive(Clone, Debug)]
pub struct Report {
    pub surface: SurfaceFit,
    pub walks: Vec<Comparison>,
}

fn decoded<T: Decode>(store: &RetailDatStore, id: DataId) -> Result<T, String> {
    let bytes = store.read_typed(T::TYPE, id).map_err(|e| e.to_string())?;
    T::decode_payload_in(store.era_of(id), id, &bytes).map_err(|e| e.to_string())
}

fn triangles(
    model: &GfxObj,
    polygons: &[Polygon],
    frame: Frame,
    scale: Vec3,
) -> Result<Vec<[Vec3; 3]>, String> {
    let mut out = Vec::new();
    for polygon in polygons {
        let points: Vec<_> = polygon
            .vertex_ids
            .iter()
            .map(|i| {
                model
                    .vertex_array
                    .vertices
                    .get(usize::from(*i))
                    .map(|v| localtoglobal(&frame, v.position.mul_componentwise(scale)))
                    .ok_or_else(|| format!("{} has an invalid vertex index", model.id))
            })
            .collect::<Result<_, _>>()?;
        for i in 1..points.len().saturating_sub(1) {
            out.push([points[0], points[i], points[i + 1]]);
        }
    }
    Ok(out)
}

/// A GfxObj or Setup's drawn parts and effective collision at its resting pose.
///
/// # Errors
/// Missing or malformed records, no resting frame, or an id of another record kind.
pub fn load(store: &RetailDatStore, id: DataId) -> Result<Model, String> {
    let mut stats = SetupPartStats::default();
    let (ids, physics) = match id.raw() >> 24 {
        1 => {
            let geometry = setup::simple_setup_geometry(store, id, &mut stats)
                .ok_or_else(|| format!("{id} is not a readable mesh"))?;
            (vec![id], geometry)
        }
        2 => {
            let s: Setup = decoded(store, id)?;
            let geometry = setup::setup_geometry_with_parts(store, &s, &mut stats);
            if stats.part_undecodable != 0 || geometry.parts.len() != s.parts.len() {
                return Err(format!("{id} has missing parts or no resting placement"));
            }
            (s.parts, geometry)
        }
        _ => return Err("fit takes a GfxObj (0x01) or Setup (0x02) id".into()),
    };
    let mut drawn = Vec::new();
    let mut collision = Vec::new();
    for (id, part) in ids.into_iter().zip(&physics.parts) {
        let mesh: GfxObj = decoded(store, id)?;
        drawn.push(triangles(
            &mesh,
            &mesh.polygons,
            part.placement_frame,
            part.default_scale,
        )?);
        if part.physics_bsp.is_some() {
            // Part collision uses the vertical default scale uniformly.
            let z = part.default_scale.z;
            collision.push(Shape::Mesh(triangles(
                &mesh,
                &mesh.physics_polygons,
                part.placement_frame,
                Vec3::new(z, z, z),
            )?));
        }
    }
    if !physics.caches_physics_bsp() {
        if physics.cyl_spheres.is_empty() {
            collision.extend(physics.spheres.iter().copied().map(Shape::Sphere));
        } else {
            collision.extend(physics.cyl_spheres.iter().copied().map(Shape::Cylinder));
        }
    }
    Ok(Model {
        drawn,
        collision,
        physics: Arc::new(physics),
    })
}

/// Check the candidate through an optional overlay, and compare against the base files alone.
///
/// # Errors
/// Invalid files, model geometry or sampling settings.
pub fn check_files(
    dats: &Path,
    overlay: Option<&Path>,
    candidate: u32,
    original: Option<u32>,
    grid: usize,
    directions: usize,
) -> Result<Report, String> {
    // The files before Throne of Destiny when the folder holds those alone.
    let base = if !dereth_dat::holds_modern_dats(dats) && dereth_dat::holds_classic_dats(dats) {
        RetailDatStore::open_classic_dir(dats)
    } else {
        RetailDatStore::open_dir(dats)
    }
    .map_err(|e| e.to_string())?;
    let store = match overlay {
        Some(dir) => base
            .clone()
            .with_overlay(
                &dereth_dat::overlay::OverlayDir::new(dir).map_err(|e| e.to_string())?,
                None,
            )
            .map_err(|e| e.to_string())?,
        None => base.clone(),
    };
    let candidate = load(&store, DataId(candidate))?;
    let surface = measure(&candidate, grid)?;
    let walks = match original {
        Some(id) => {
            let original = load(&base, DataId(id))?;
            compare(
                &candidate.physics,
                &original.physics,
                candidate.reach().max(original.reach()),
                directions,
            )?
        }
        None => Vec::new(),
    };
    Ok(Report { surface, walks })
}
