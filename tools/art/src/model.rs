//! What a model is made of: its size, triangles, parts, pictures and collision, and the
//! triangles its picture is drawn from.
//!
//! A graphics object is one mesh. A setup is its parts, each a graphics object posed by the
//! setup's resting placement and scaled by its default scale, as static objects and scenery are
//! drawn. An environment piece is one cell structure, drawn with the surface list of a cell built
//! from it.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use dereth_assets::{Decode, Environment, GfxObj, Setup};
use dereth_client_runtime::models::{resolve_parts, triangulate, triangulate_cell, SurfaceGroup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::frame::{l2g, localtoglobal, localtoglobalvec, V3};
use dereth_primitives::{DataId, Frame, Vec3};

use crate::tally::ModelId;

/// One vertex of a drawn triangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vert {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// The triangles drawn with one surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// The surface record (`0x08…`), when the polygons name one.
    pub surface: Option<DataId>,
    pub two_sided: bool,
    pub triangles: Vec<[Vert; 3]>,
}

/// Everything a model draws, in model space at scale 1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub groups: Vec<Group>,
}

impl Mesh {
    #[must_use]
    pub fn triangles(&self) -> u64 {
        self.groups.iter().map(|g| g.triangles.len() as u64).sum()
    }

    /// The smallest box holding every drawn vertex.
    #[must_use]
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut it = self
            .groups
            .iter()
            .flat_map(|g| g.triangles.iter().flatten())
            .map(|v| v.position);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])],
                [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])],
            )
        }))
    }
}

/// What the client collides an instance of the model against.
#[derive(Debug, Clone, PartialEq)]
pub enum Collision {
    None,
    /// The setup's cylinders, each `(radius, height)`.
    Cylinders(Vec<(f32, f32)>),
    /// The setup's collision spheres, each a radius.
    Spheres(Vec<f32>),
    /// A physics mesh, with its polygon count.
    Bsp {
        polygons: u64,
    },
}

impl Collision {
    /// `none`, `cylinder r=0.40 h=2.10`, `sphere r=0.50`, `bsp 132`.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::None => "none".to_owned(),
            Self::Cylinders(c) => {
                let all: Vec<String> = c
                    .iter()
                    .map(|(r, h)| format!("r={r:.2} h={h:.2}"))
                    .collect();
                format!("cylinder {}", all.join(", "))
            }
            Self::Spheres(s) => {
                let all: Vec<String> = s.iter().map(|r| format!("r={r:.2}")).collect();
                format!("sphere {}", all.join(", "))
            }
            Self::Bsp { polygons } => format!("bsp {polygons}"),
        }
    }

    /// `none`, `cylinder`, `sphere` or `bsp`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cylinders(_) => "cylinder",
            Self::Spheres(_) => "sphere",
            Self::Bsp { .. } => "bsp",
        }
    }
}

/// One picture a model's surfaces draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Texture {
    /// The surface's texture record (`0x05…`).
    pub id: DataId,
    pub width: u32,
    pub height: u32,
}

/// What a model is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    /// The records it is built from were found and decoded.
    pub found: bool,
    pub bounds: Option<([f32; 3], [f32; 3])>,
    pub triangles: u64,
    /// A setup's parts, in part order; empty otherwise.
    pub parts: Vec<DataId>,
    pub textures: Vec<Texture>,
    /// Surfaces that draw a flat colour rather than a picture.
    pub solid_surfaces: u64,
    pub collision: Collision,
}

impl Facts {
    /// The box's extent along x, y and z.
    #[must_use]
    pub fn size(&self) -> Option<[f32; 3]> {
        self.bounds
            .map(|(lo, hi)| [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]])
    }
}

/// Reads models from the dats, decoding each graphics object and environment once.
#[derive(Debug)]
pub struct Models<'a> {
    store: &'a RetailDatStore,
    // ORDER-OK: decode memos, only ever looked up.
    gfxobjs: HashMap<DataId, Option<Arc<GfxObj>>>,
    environments: HashMap<DataId, Option<Arc<Environment>>>,
    surfaces: HashMap<DataId, Option<dereth_assets::Surface>>,
}

fn v3(v: Vec3) -> [f32; 3] {
    [v.x, v.y, v.z]
}

impl<'a> Models<'a> {
    #[must_use]
    pub fn new(store: &'a RetailDatStore) -> Self {
        Self {
            store,
            gfxobjs: HashMap::new(),
            environments: HashMap::new(),
            surfaces: HashMap::new(),
        }
    }

    #[must_use]
    pub fn store(&self) -> &'a RetailDatStore {
        self.store
    }

    fn gfxobj(&mut self, id: DataId) -> Option<Arc<GfxObj>> {
        let store = self.store;
        self.gfxobjs
            .entry(id)
            .or_insert_with(|| {
                let bytes = store.read_typed(DbType::GfxObj, id).ok()?;
                GfxObj::decode_payload_in(store.era_of(id), id, &bytes)
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }

    fn environment(&mut self, id: DataId) -> Option<Arc<Environment>> {
        let store = self.store;
        self.environments
            .entry(id)
            .or_insert_with(|| {
                let bytes = store.read_typed(DbType::Environment, id).ok()?;
                Environment::decode_payload_in(store.era_of(id), id, &bytes)
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }

    fn setup(&self, id: DataId) -> Option<Setup> {
        let bytes = self.store.read_typed(DbType::Setup, id).ok()?;
        Setup::decode_payload_in(self.store.era_of(id), id, &bytes).ok()
    }

    /// The surface record `id`, decoded.
    pub fn surface(&mut self, id: DataId) -> Option<dereth_assets::Surface> {
        let store = self.store;
        *self.surfaces.entry(id).or_insert_with(|| {
            let bytes = store.read_typed(DbType::Surface, id).ok()?;
            dereth_assets::Surface::decode_payload_in(store.era_of(id), id, &bytes).ok()
        })
    }

    /// The triangles `id` draws. A piece needs `piece_surfaces`, the surface list of a cell
    /// built from it; an object ignores it.
    pub fn mesh(&mut self, id: ModelId, piece_surfaces: &[DataId]) -> Option<Mesh> {
        match id {
            ModelId::Object(obj) => match obj.0 >> 24 {
                0x01 => {
                    let g = self.gfxobj(obj)?;
                    Some(Mesh {
                        groups: triangulate(&g).into_iter().map(group_of).collect(),
                    })
                }
                0x02 => {
                    // Fails the same way the parts do: a setup that will not read has none.
                    self.setup(obj)?;
                    let mut groups = Vec::new();
                    for part in resolve_parts(self.store, obj) {
                        let Some(g) = self.gfxobj(part.gfxobj) else {
                            continue;
                        };
                        for sg in triangulate(&g) {
                            groups.push(posed(group_of(sg), &part.frame, part.scale));
                        }
                    }
                    Some(Mesh { groups })
                }
                _ => None,
            },
            ModelId::Piece {
                environment,
                cell_struct,
            } => {
                let env = self.environment(environment)?;
                let cs = env.cells.get(usize::from(cell_struct))?;
                Some(Mesh {
                    groups: triangulate_cell(cs, piece_surfaces)
                        .into_iter()
                        .map(group_of)
                        .collect(),
                })
            }
        }
    }

    /// What `id` is made of, and the triangles it draws. `texture_size` answers a texture
    /// record's width and height.
    pub fn facts(
        &mut self,
        id: ModelId,
        piece_surfaces: &[DataId],
        texture_size: &mut dyn FnMut(DataId) -> Option<(u32, u32)>,
    ) -> (Facts, Mesh) {
        let mesh = self.mesh(id, piece_surfaces);
        let found = mesh.is_some();
        let mesh = mesh.unwrap_or_default();
        let (parts, collision) = match id {
            ModelId::Object(obj) if obj.0 >> 24 == 0x02 => self.setup_collision(obj),
            ModelId::Object(obj) => (
                Vec::new(),
                self.gfxobj(obj)
                    .map_or(Collision::None, |g| gfxobj_collision(&g)),
            ),
            ModelId::Piece {
                environment,
                cell_struct,
            } => (
                Vec::new(),
                self.environment(environment)
                    .and_then(|e| {
                        e.cells
                            .get(usize::from(cell_struct))
                            .map(|cs| Collision::Bsp {
                                polygons: cs.physics_polygons.len() as u64,
                            })
                    })
                    .unwrap_or(Collision::None),
            ),
        };
        let mut textures: BTreeMap<DataId, Texture> = BTreeMap::new();
        let mut solid = 0;
        let mut surfaces: Vec<DataId> = mesh.groups.iter().filter_map(|g| g.surface).collect();
        surfaces.sort_unstable();
        surfaces.dedup();
        for s in surfaces {
            match self.surface(s).and_then(|r| r.orig_texture_id) {
                Some(t) => {
                    if let Some((width, height)) = texture_size(t) {
                        textures.insert(
                            t,
                            Texture {
                                id: t,
                                width,
                                height,
                            },
                        );
                    }
                }
                None => solid += 1,
            }
        }
        let facts = Facts {
            found,
            bounds: mesh.bounds(),
            triangles: mesh.triangles(),
            parts,
            textures: textures.into_values().collect(),
            solid_surfaces: solid,
            collision,
        };
        (facts, mesh)
    }

    /// A setup's parts and its collision: a physics mesh when any part has one, else its
    /// cylinders, else its spheres, the order the client tests them in.
    fn setup_collision(&mut self, id: DataId) -> (Vec<DataId>, Collision) {
        let Some(setup) = self.setup(id) else {
            return (Vec::new(), Collision::None);
        };
        let mut bsp = false;
        let mut polygons = 0u64;
        for &p in &setup.parts {
            if let Some(g) = self.gfxobj(p) {
                if g.physics_bsp.is_some() {
                    bsp = true;
                    polygons += g.physics_polygons.len() as u64;
                }
            }
        }
        let collision = if bsp {
            Collision::Bsp { polygons }
        } else if !setup.cylspheres.is_empty() {
            Collision::Cylinders(
                setup
                    .cylspheres
                    .iter()
                    .map(|c| (c.radius, c.height))
                    .collect(),
            )
        } else if !setup.spheres.is_empty() {
            Collision::Spheres(setup.spheres.iter().map(|s| s.radius).collect())
        } else {
            Collision::None
        };
        (setup.parts.clone(), collision)
    }
}

fn gfxobj_collision(g: &GfxObj) -> Collision {
    if g.physics_bsp.is_some() {
        Collision::Bsp {
            polygons: g.physics_polygons.len() as u64,
        }
    } else {
        Collision::None
    }
}

fn group_of(sg: SurfaceGroup) -> Group {
    let triangles = sg
        .vertices
        .as_chunks::<3>()
        .0
        .iter()
        .zip(sg.normals.as_chunks::<3>().0)
        .map(|(v, n)| {
            let vert = |k: usize| Vert {
                position: v3(v[k].0),
                normal: v3(n[k]),
                uv: [v[k].1, v[k].2],
            };
            [vert(0), vert(1), vert(2)]
        })
        .collect();
    Group {
        surface: sg.surface,
        two_sided: sg.two_sided,
        triangles,
    }
}

/// A part's triangles placed in the setup: scaled by the part's default scale, then turned and
/// moved by its placement frame.
fn posed(mut g: Group, frame: &Frame, scale: Vec3) -> Group {
    let turn = l2g(frame.rotation);
    for t in &mut g.triangles {
        for v in t.iter_mut() {
            let p = Vec3::new(v.position[0], v.position[1], v.position[2]).mul_componentwise(scale);
            v.position = v3(localtoglobal(frame, p));
            let n = Vec3::new(v.normal[0], v.normal[1], v.normal[2]);
            v.normal = v3(localtoglobalvec(turn, n));
        }
    }
    g
}
