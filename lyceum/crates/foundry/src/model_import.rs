//! Static glTF scenes compiled into models, placement frames, surfaces and collision shapes.
//! Source dimensions are meters. The same axis rotation as the client maps `(x, y, z)` to
//! `(x, -z, y)`. Node scale is baked into geometry; placements keep translation and rotation.

pub mod piece;
mod read;

use std::collections::BTreeMap;
use std::path::Path;

use dereth_assets::geometry::{CylSphere, Placement, SetupOrder};
use dereth_assets::{GfxObj, Setup, Sphere};
use dereth_primitives::frame::V3;
use dereth_primitives::num::math;
use dereth_primitives::{DataId, EraId, Vec3};
use serde::Deserialize;

use crate::error::{Error, Loc};
use crate::source::{Located, NewRecord, RecordRef, Target};
use crate::symbol::Kind;
use read::{PartModel as _, SceneRecords as _};

/// A static model's collision representation.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Collision {
    #[default]
    Mesh,
    Cylinder,
    Sphere,
    None,
}

/// Records whose references receive the world's ids after the source has been read.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Environment {
        environment: Box<dereth_assets::Environment>,
        surfaces: Vec<RecordRef>,
    },
    Mesh {
        model: GfxObj,
        surfaces: Vec<RecordRef>,
        degrade: Option<RecordRef>,
    },
    Setup {
        setup: Box<Setup>,
        parts: Vec<RecordRef>,
    },
    Degrade {
        levels: Vec<RecordRef>,
        distances: Vec<f32>,
    },
    Image {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
}

impl Record {
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self {
            Self::Environment { .. } => Kind::Environment,
            Self::Mesh { .. } => Kind::GfxObj,
            Self::Setup { .. } => Kind::Setup,
            Self::Degrade { .. } => Kind::Degrade,
            Self::Image { .. } => Kind::RenderSurface,
        }
    }

    #[must_use]
    pub fn references(&self) -> Vec<(Kind, &RecordRef)> {
        match self {
            Self::Environment { surfaces, .. } => {
                surfaces.iter().map(|r| (Kind::Surface, r)).collect()
            }
            Self::Mesh {
                surfaces, degrade, ..
            } => surfaces
                .iter()
                .map(|r| (Kind::Surface, r))
                .chain(degrade.iter().map(|r| (Kind::Degrade, r)))
                .collect(),
            Self::Setup { parts, .. } | Self::Degrade { levels: parts, .. } => {
                parts.iter().map(|r| (Kind::GfxObj, r)).collect()
            }
            Self::Image { .. } => Vec::new(),
        }
    }

    /// The picture an image record holds, for the image textures of the files whose textures
    /// hold their pixels.
    #[must_use]
    pub fn picture(&self) -> Option<crate::pictures::Rgba> {
        match self {
            Self::Image {
                width,
                height,
                pixels,
            } => Some(crate::pictures::Rgba::from_bgra(*width, *height, pixels)),
            _ => None,
        }
    }

    /// Encode in the layout of the files of `era` after resolving every reference against this
    /// world's lockfile.
    ///
    /// # Errors
    /// A reference that no pack defines, or a record the era's files cannot hold.
    pub fn encode(
        &self,
        era: EraId,
        id: DataId,
        mut resolve: impl FnMut(Kind, &RecordRef) -> Result<DataId, Error>,
    ) -> Result<Vec<u8>, Error> {
        let held = |r: Result<Vec<u8>, dereth_assets::AssetError>| {
            r.map_err(|e| Error::Refused(format!("{:#010X}: {e}", id.raw())))
        };
        Ok(match self {
            Self::Environment { environment, .. } => {
                let mut environment = environment.as_ref().clone();
                environment.id = id;
                held(environment.encode_for(era))?
            }
            Self::Mesh {
                model,
                surfaces,
                degrade,
            } => {
                let mut model = model.clone();
                model.id = id;
                model.surfaces = surfaces
                    .iter()
                    .map(|r| resolve(Kind::Surface, r))
                    .collect::<Result<_, _>>()?;
                model.did_degrade = degrade
                    .as_ref()
                    .map(|r| resolve(Kind::Degrade, r))
                    .transpose()?;
                held(model.encode_for(era))?
            }
            Self::Setup { setup, parts } => {
                let mut setup = setup.as_ref().clone();
                setup.id = id;
                setup.parts = parts
                    .iter()
                    .map(|r| resolve(Kind::GfxObj, r))
                    .collect::<Result<_, _>>()?;
                held(setup.encode_for(era))?
            }
            Self::Degrade { levels, distances } => {
                let level = |gfxobj_id, d: f32| dereth_assets::motion::GfxObjInfo {
                    gfxobj_id,
                    degrade_mode: 1,
                    min_dist: d,
                    ideal_dist: d,
                    max_dist: d,
                };
                let mut degrades = Vec::with_capacity(levels.len() + 1);
                for (l, distance) in levels.iter().zip(distances) {
                    degrades.push(level(resolve(Kind::GfxObj, l)?, *distance));
                }
                degrades.push(level(DataId(0), f32::MAX));
                held(dereth_assets::motion::GfxObjDegradeInfo { id, degrades }.encode_for(era))?
            }
            Self::Image {
                width,
                height,
                pixels,
            } => held(crate::pictures::image_record(
                era,
                id,
                &crate::pictures::Rgba::from_bgra(*width, *height, pixels),
                || (21, 4, pixels.clone()),
            ))?,
        })
    }
}

/// The symbol of a model part's detail level `level`, beside its detail record `part`.
fn level_symbol(part: &str, level: usize) -> String {
    format!("{part}-lod-{level}")
}

/// The symbol of the nearest level of the model part whose detail record is `degrade`: in the
/// files whose models reach their detail records by their own ids, the model the record's id is
/// made from.
#[must_use]
pub fn nearest_level(degrade: &str) -> String {
    level_symbol(degrade, 0)
}

fn named(name: &str) -> RecordRef {
    RecordRef::Symbol(name.to_owned())
}

fn add(out: &mut Vec<Located<NewRecord>>, at: &Loc, name: String, record: Record) {
    out.push(Located {
        at: at.clone(),
        item: NewRecord::Geometry {
            target: Target::New(name),
            record: Box::new(record),
        },
    });
}

/// Import a model source. Named `*_lod1`, `*_lod2` nodes supply lower detail for their base node;
/// `*_collision` supplies collision for that node instead of its drawn triangles.
///
/// # Errors
/// Invalid or unsupported glTF, missing pictures or levels, or geometry beyond record limits.
pub fn records(
    root: &Path,
    path: &Path,
    at: &Loc,
    name: &str,
    replace: Option<DataId>,
    collision: Collision,
    distances: &[f32],
) -> Result<Vec<Located<NewRecord>>, Error> {
    let bad = |s: String| Error::source(at, s);
    let scene = read::scene(root, path).map_err(bad)?;
    let mut out = Vec::new();
    let surfaces = scene.materials(at, name, &mut out)?;
    let mut groups: BTreeMap<String, BTreeMap<usize, &read::Part>> = BTreeMap::new();
    let mut collisions = BTreeMap::new();
    for part in &scene.parts {
        if let Some(base) = part.name.strip_suffix("_collision") {
            if collisions.insert(base.to_owned(), part).is_some() {
                return Err(bad(format!("duplicate collision node for {base}")));
            }
            continue;
        }
        let (base, level) = match part.name.rsplit_once("_lod") {
            Some((base, n)) => (
                base,
                n.parse::<usize>()
                    .map_err(|_| bad(format!("{}: invalid detail level", part.name)))?,
            ),
            None => (part.name.as_str(), 0),
        };
        if groups
            .entry(base.to_owned())
            .or_default()
            .insert(level, part)
            .is_some()
        {
            return Err(bad(format!(
                "duplicate node {base} at detail level {level}"
            )));
        }
    }
    if groups.is_empty() {
        return Err(bad("the scene has no drawn mesh".into()));
    }
    if let Some(orphan) = collisions.keys().find(|n| !groups.contains_key(*n)) {
        return Err(bad(format!("{orphan}_collision has no drawn node")));
    }
    let mut frames = Vec::new();
    let mut parts = Vec::new();
    let mut corners = Vec::new();
    let mut collision_corners = Vec::new();
    for (ordinal, (base, levels)) in groups.iter().enumerate() {
        let Some(first) = levels.get(&0) else {
            return Err(bad(format!("{base} has lower detail but no base mesh")));
        };
        if levels.keys().copied().ne(0..levels.len()) {
            return Err(bad(format!(
                "{base}: detail levels must be consecutive from zero"
            )));
        }
        let part_name = format!("{name}-part-{ordinal}");
        let mut refs = Vec::new();
        for (level, part) in levels {
            let sym = level_symbol(&part_name, *level);
            let mut model = part.model(first.frame, &surfaces).map_err(bad)?;
            if *level == 0 && collision == Collision::Mesh {
                if let Some(shape) = collisions.get(base) {
                    collision_corners.extend(shape.world_vertices());
                    let shape = shape.model(first.frame, &surfaces).map_err(bad)?;
                    let offset = model.vertex_array.vertices.len();
                    if offset + shape.vertex_array.vertices.len() > i16::MAX as usize {
                        return Err(bad(format!(
                            "{base}: drawing and collision exceed 32767 vertices"
                        )));
                    }
                    for mut v in shape.vertex_array.vertices {
                        v.id = i16::try_from(model.vertex_array.vertices.len()).unwrap_or(0);
                        model.vertex_array.vertices.push(v);
                    }
                    model.physics_polygons = shape.polygons;
                    for p in &mut model.physics_polygons {
                        for v in &mut p.vertex_ids {
                            *v += u16::try_from(offset).unwrap_or(0);
                        }
                    }
                } else {
                    model.physics_polygons.clone_from(&model.polygons);
                }
                model.physics_bsp = Some(crate::bsp::physics(
                    &model.vertex_array,
                    &model.physics_polygons,
                    true,
                ));
                model.flags |= 1;
            }
            if *level == 0 {
                corners.extend(part.world_vertices());
            }
            let degrade = (levels.len() > 1 && *level == 0).then(|| named(&part_name));
            if degrade.is_some() {
                model.flags |= 8;
            }
            add(
                &mut out,
                at,
                sym.clone(),
                Record::Mesh {
                    model,
                    surfaces: surfaces.clone(),
                    degrade,
                },
            );
            refs.push(named(&sym));
        }
        if levels.len() > 1 {
            let bands = if distances.is_empty() {
                (1..=levels.len())
                    .map(|k| {
                        #[allow(clippy::cast_precision_loss)]
                        let d = k as f32 * 40.0;
                        d
                    })
                    .collect()
            } else {
                if distances.len() != levels.len() {
                    return Err(bad(format!("{base}: give one detail distance per level")));
                }
                distances.to_vec()
            };
            if bands.iter().any(|d| !d.is_finite() || *d <= 0.0)
                || bands.windows(2).any(|w| w[0] >= w[1])
            {
                return Err(bad(
                    "detail distances must be finite, positive and increasing".into(),
                ));
            }
            add(
                &mut out,
                at,
                part_name,
                Record::Degrade {
                    levels: refs.clone(),
                    distances: bands,
                },
            );
        }
        parts.push(refs[0].clone());
        frames.push(first.frame);
    }
    let (lo, hi) = bounds(&corners);
    let center = lo.add(hi).mul(0.5);
    let radius = corners
        .iter()
        .map(|p| p.sub(center).dot(p.sub(center)).sqrt())
        .fold(0.0, f32::max);
    let sphere = Sphere { center, radius };
    let horizontal = corners
        .iter()
        .map(|p| math::hypotf(p.x - center.x, p.y - center.y))
        .fold(0.0, f32::max);
    let mut broad_corners = corners.clone();
    broad_corners.extend(collision_corners);
    let (_, broad_hi) = bounds(&broad_corners);
    let setup = Setup {
        id: DataId(0),
        flags: 4 | if collision == Collision::Mesh { 8 } else { 0 },
        parts: vec![DataId(0); parts.len()],
        parent_index: None,
        default_scale: None,
        allow_free_heading: true,
        has_physics_bsp: collision == Collision::Mesh,
        holding_locations: BTreeMap::new(),
        connection_points: BTreeMap::new(),
        placement_frames: BTreeMap::from([(
            0,
            Placement {
                frames,
                hooks: Vec::new(),
            },
        )]),
        cylspheres: if collision == Collision::Cylinder {
            vec![CylSphere {
                low_pt: Vec3::new(center.x, center.y, lo.z),
                radius: horizontal,
                height: hi.z - lo.z,
            }]
        } else {
            Vec::new()
        },
        spheres: if collision == Collision::Sphere {
            vec![sphere]
        } else {
            Vec::new()
        },
        height: broad_hi.z,
        radius: broad_corners
            .iter()
            .map(|p| math::hypotf(p.x, p.y))
            .fold(0.0, f32::max),
        step_up_height: 0.0,
        step_down_height: 0.0,
        sorting_sphere: sphere,
        selection_sphere: sphere,
        lights: BTreeMap::new(),
        default_anim_id: DataId(0),
        default_script_id: DataId(0),
        default_mtable_id: DataId(0),
        default_stable_id: DataId(0),
        default_phstable_id: DataId(0),
        written_order: SetupOrder::default(),
    };
    out.push(Located {
        at: at.clone(),
        item: NewRecord::Geometry {
            target: replace.map_or_else(|| Target::New(name.to_owned()), Target::Replace),
            record: Box::new(Record::Setup {
                setup: Box::new(setup),
                parts,
            }),
        },
    });
    Ok(out)
}

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    points.iter().fold(
        (
            Vec3::new(f32::MAX, f32::MAX, f32::MAX),
            Vec3::new(f32::MIN, f32::MIN, f32::MIN),
        ),
        |(lo, hi), p| {
            (
                Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
            )
        },
    )
}
