//! A frame of the world at a spot: the terrain around the viewer as layered batches, the placed
//! objects and interiors seen from outside as mesh batches with their materials, the water, and the
//! sky, with the moment's light and fog, the sun's shadow map, the clouds' shadows and the water's
//! reflection plane.

use dereth_primitives::num::floor_to_i32;
use std::collections::HashMap;
use std::sync::Arc;

use auberean_assets::landblock::Landblock;
use auberean_assets::mesh::{CellMesh, Mesh};
use auberean_primitives::{DataId, Vec3};
use auberean_world::color::Color;
use auberean_world::look::{Look, PartLook};
use auberean_world::math::Mat4;
use auberean_world::place::{cell_look, entity_parts, parts_of, Part};
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::{block_scenery, KeptClear, SceneryInput, SceneryObject};
use auberean_world::sky::{day_light, sky_pieces, DayLight};
use auberean_world::terrain::{build_block, TerrainBlock};
use auberean_world::water::BlockWater;
use dereth_landscape::{Detail, LandblockWindow, SlotAction, WindowSpec};

use crate::effects::{entity_place, object_effect_look, PlacedLook};
use crate::material::{
    draw_layers, ground_picture, instance_material, instance_material_with, material_kind,
    shadow_layer, DrawLayer, DEVICE_FLAGS,
};
use crate::mesh::{detail_at, level_at, vertices, MeshVertex};
use crate::shadow::{CloudShadows, ShadowMap};

mod interior;

pub use interior::Outlook;

/// Where a frame is seen from and when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    /// The block the viewer stands in, x east and y north.
    pub block: (i32, i32),
    /// The viewer's position in that block's frame (meters from its south-west corner).
    pub position: Vec3,
    /// Degrees clockwise from north.
    pub heading: f32,
    /// Degrees above the horizon.
    pub pitch: f32,
    /// The fraction of the day, 0 to 1.
    pub time_of_day: f32,
    /// The sky's day (`.dyd`), or the region's first day when `None`.
    pub day: Option<DataId>,
}

/// The viewer's camera, in the frame of the block it stands in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub heading: f32,
    pub pitch: f32,
    /// Vertical field of view, degrees.
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
    /// Seen straight on (no perspective): the height seen, metres, from the picture's foot to its
    /// top; its width by the picture's shape.
    pub orthographic: Option<f32>,
}

/// A terrain vertex: position, lit color, the ground picture's coordinates and the mask's.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub mask_uv: [f32; 2],
}

/// The terrain of one ground and mask: the bottom layer of every square showing that ground, or
/// one masked layer, or the far landscape.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TerrainBatch {
    /// The ground picture (a picture list).
    pub ground: u32,
    /// The mask picture, for layers above the bottom one.
    pub mask: Option<u32>,
    /// 0 for a bottom layer, 1 or 2 for the layers blended over it.
    pub layer: u8,
    /// Whether this is the far landscape (one picture added to the lit color).
    pub far: bool,
    pub vertices: Vec<TerrainVertex>,
    pub indices: Vec<u32>,
}

/// One drawn piece of a mesh: its vertices, one fragment's triangles, where it stands, and one
/// layer of its material.
#[derive(Clone, Debug)]
pub struct ObjectBatch {
    pub vertices: Arc<Vec<MeshVertex>>,
    pub indices: Arc<Vec<u16>>,
    pub transform: Mat4,
    pub layer: DrawLayer,
    /// The draw's tint (white, or the sky object's color and opacity).
    pub tint: Color,
    /// Inside, the point lights the draw is lit by in place of the sun.
    pub points: PointLights,
    /// The kind its mesh names itself (a head, a torso, an arm...), which a pick on a figure
    /// answers with; 0 for a draw of no mesh.
    pub mesh_type: u32,
    /// A frill's spot on the ground: its vertices take the color of the shadow map about it
    /// there in place of their own.
    pub shade: Option<Vec3>,
    /// The draw stands outdoors in a frame seen from inside: it is drawn under the frame's
    /// outdoor light ([`Frame::outside`]) rather than the frame's own.
    pub outside: bool,
    /// What decides whether the draw is drawn in a frame seen from inside ([`crate::cull`]).
    pub cull: Cull,
    /// The mesh record the draw is part of (0 for a draw of no mesh): the volume of shadow its
    /// object casts is made from it.
    pub mesh: u32,
    /// Its object casts a volume of shadow and is drawn after the volumes have darkened the
    /// frame, so its own shadow never falls on it.
    pub casts_volume: bool,
}

/// What decides whether a draw is drawn in a frame seen from inside, as the walk through the
/// doorways from the camera's cell finds it ([`crate::cull::Sight::shows`]); a frame seen from
/// outside draws everything.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Cull {
    /// Drawn whatever the camera sees.
    #[default]
    Never,
    /// An interior cell's own mesh, by its low word: drawn when the walk reaches it.
    Cell(u16),
    /// An object standing in an interior cell: drawn when the cell is reached and the object's
    /// box (twice its size about its middle) meets a view the cell was entered by.
    InCell { cell: u16, lo: Vec3, hi: Vec3 },
    /// An object standing outdoors, its box: drawn when the box meets a view out.
    Box { lo: Vec3, hi: Vec3 },
}

/// The box about the box from `lo` to `hi` carried by `m`: about its eight corners.
#[must_use]
pub fn placed_box(lo: Vec3, hi: Vec3, m: &Mat4) -> (Vec3, Vec3) {
    let mut a = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut b = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for k in 0..8 {
        let c = m.point(Vec3::new(
            if k & 1 == 0 { lo.x } else { hi.x },
            if k & 2 == 0 { lo.y } else { hi.y },
            if k & 4 == 0 { lo.z } else { hi.z },
        ));
        a = Vec3::new(a.x.min(c.x), a.y.min(c.y), a.z.min(c.z));
        b = Vec3::new(b.x.max(c.x), b.y.max(c.y), b.z.max(c.z));
    }
    (a, b)
}

/// The point lights an object inside is lit by, as many as its program takes: each its place in
/// the frame's world, its diffuse and specular colors and its range. Outdoors a draw has none
/// and takes the sun.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLights {
    /// Whether the draw is lit by these lights in place of the sun.
    pub inside: bool,
    pub count: usize,
    pub lights: [auberean_world::inside::DeviceLight; 4],
}

impl PointLights {
    /// No point lights: the draw takes the sun.
    pub const NONE: Self = Self {
        inside: false,
        count: 0,
        lights: [auberean_world::inside::DeviceLight {
            origin: Vec3::ZERO,
            diffuse: Color::BLACK,
            specular: Color::BLACK,
            range: 1.0,
            flicker: None,
        }; 4],
    };

    /// The lights of a draw inside, at most four, placed in the frame's world by `place`.
    #[must_use]
    pub fn inside(lights: &[auberean_world::inside::DeviceLight], place: &Mat4) -> Self {
        let mut out = Self {
            inside: true,
            ..Self::NONE
        };
        for (slot, l) in out.lights.iter_mut().zip(lights) {
            *slot = auberean_world::inside::DeviceLight {
                origin: place.point(l.origin),
                ..*l
            };
            out.count += 1;
        }
        out
    }
}

/// The steps a frame of the world is drawn in. The sky's early objects come first, then the fog's
/// veil over them, the landscape, the objects that hide what lies behind them, and the water. The
/// objects seen through follow, the particle systems last among them, and then the sky's late
/// objects. Water drawn after a flame or a mist would hide it, for neither marks how near it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Sky,
    Veil,
    Terrain,
    /// The decals laid on the ground.
    Decals,
    Opaque,
    /// The volumes of shadow darken what they enclose.
    Volumes,
    /// The opaque draws of the objects that cast the volumes.
    Casters,
    Water,
    Translucent,
    LateSky,
}

/// Every step of a frame of the world, in the order it is drawn.
pub const PASSES: [Pass; 10] = [
    Pass::Sky,
    Pass::Veil,
    Pass::Terrain,
    Pass::Decals,
    Pass::Opaque,
    Pass::Volumes,
    Pass::Casters,
    Pass::Water,
    Pass::Translucent,
    Pass::LateSky,
];

impl ObjectBatch {
    /// The step an object's draw belongs to: the opaque objects', when it marks how near it is
    /// and covers what lies behind (the casters' after the volumes, for an object that casts
    /// one); else the step of those seen through.
    #[must_use]
    pub fn pass(&self) -> Pass {
        if self.layer.depth_write && !self.layer.blends() {
            if self.casts_volume {
                Pass::Casters
            } else {
                Pass::Opaque
            }
        } else {
            Pass::Translucent
        }
    }
}

/// Everything one frame draws.
#[derive(Clone, Debug)]
pub struct Frame {
    pub camera: Camera,
    pub light: DayLight,
    pub sky_before: Vec<ObjectBatch>,
    pub terrain: Vec<TerrainBatch>,
    pub objects: Vec<ObjectBatch>,
    pub sky_after: Vec<ObjectBatch>,
    /// Where frills fade out, from the camera: fully drawn nearer than the first distance, gone
    /// past the second.
    pub frill_fade: [f32; 2],
    /// What was drawn, by kind, for a caption or a test.
    pub counts: Counts,
    /// The sun's shadow map of the detailed blocks' objects, laid over the detailed terrain.
    pub shadow: Option<ShadowMap>,
    /// The clouds' shadows over all the terrain.
    pub clouds: Option<CloudShadows>,
    /// How long the client has been running, in seconds: the clock scrolling pictures follow.
    pub uptime: f64,
    /// The water surfaces of the detailed and far blocks, drawn after the opaque objects and
    /// before those seen through.
    pub water: Vec<ObjectBatch>,
    /// The water's reflection, when some water lies below the camera.
    pub reflection: Option<Reflection>,
    /// The objects drawn with the world whose looks play startup effects.
    pub placed: Vec<PlacedLook>,
    /// The sky objects' lens flares, drawn after the sky's late objects.
    pub flares: Vec<crate::flare::SkyFlare>,
    /// Inside, when the cells about the camera see out through their portals: the light and fog
    /// outdoors. The sky, the landscape, the water and the draws standing outdoors are drawn
    /// under it, the rest under the frame's own light (the interior's).
    pub outside: Option<DayLight>,
    /// Inside: the camera's cell and the cells it sees, which the device walks through each
    /// frame to draw only what the doorways show ([`crate::cull::Rooms::sight`]).
    pub rooms: Option<Arc<crate::cull::Rooms>>,
    /// The dynamic objects' volumes of shadow, drawn after the opaque objects that cast none.
    pub volumes: crate::volume::Volumes,
    /// The decals laid on the ground this frame, drawn after the landscape.
    pub decals: Vec<ObjectBatch>,
}

impl Frame {
    /// Every draw's layer as it is drawn `t` seconds into the client's run (pictures sliding,
    /// turning and stretching, specular power and alpha reference moving), rolling `rng` for any
    /// noise, in the frame's drawing order.
    ///
    /// The point lights whose intensity moves (flickering torches) shine as they stand then too,
    /// each light's intensity taken once for the frame, as the device fills a set of lights.
    // D2-13334
    pub fn move_materials(&mut self, t: f64, rng: &auberean_primitives::random::Random) {
        let mut taken: HashMap<u32, f32> = HashMap::new();
        for b in self
            .sky_before
            .iter_mut()
            .chain(self.objects.iter_mut())
            .chain(self.water.iter_mut())
            .chain(self.sky_after.iter_mut())
        {
            b.layer.move_on(t, rng);
            let count = b.points.count;
            for l in b.points.lights.iter_mut().take(count) {
                if let Some(f) = l.flicker {
                    let i = *taken.entry(f.key).or_insert_with(|| f.intensity(t, rng));
                    l.diffuse = f.shine(i);
                }
            }
        }
    }
}

/// The water's reflection: the scene seen from the camera mirrored about one water plane, laid
/// over every water surface, rippled by a scrolling bump picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reflection {
    /// The plane's height, in meters.
    pub height: f32,
}

/// How many of each kind of thing a frame holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Counts {
    pub blocks: usize,
    pub squares: usize,
    pub holes: usize,
    pub objects: usize,
    pub parts: usize,
    pub cells: usize,
    pub sky_objects: usize,
    pub missing_meshes: usize,
    /// Scenery objects drawn, frills among them.
    pub scenery: usize,
    pub frills: usize,
    /// Blocks whose water is drawn.
    pub water_blocks: usize,
    /// Batches drawn into the shadow map.
    pub shadow_casters: usize,
}

/// How far the detailed landscape and the far landscape reach, in blocks from the viewer's block:
/// the far radius is the landscape distance preference's ([`SceneBuilder::set_far_radius`]), 8 at
/// its greatest (the shipped one) and the builder's own until set.
pub const NEAR_RADIUS: i32 = 1;
pub const FAR_RADIUS: i32 = 8;

/// How many land cells around the viewer's own carry their frills until the frill distance
/// preference says ([`SceneBuilder::set_frills`]): its medium setting's.
pub const FRILL_CELLS: i32 = 1;

/// Frills fade out over the last eight meters before the edge of those cells' reach.
const FRILL_FADE: f32 = 8.0;

/// A mesh as the builder keeps it: none when it does not decode.
type MeshEntry = Option<(Arc<Mesh>, Arc<Vec<MeshVertex>>)>;

/// An interior cell's vertices lit for a program's point lights, with the day's light (in 8-bit
/// steps) they were lit with.
pub(crate) type LitCell = ([u32; 7], Arc<Vec<MeshVertex>>);

/// Builds frames from one world, keeping what it decoded.
pub struct SceneBuilder<'a> {
    records: &'a WorldRecords,
    region: &'a RegionSet,
    meshes: HashMap<u32, MeshEntry>,
    materials: HashMap<u32, Vec<DrawLayer>>,
    /// Materials as a placed object's look changes them: by material, replacement template and
    /// added modifiers.
    looked: HashMap<(u32, Option<u32>, Vec<u32>), Vec<DrawLayer>>,
    material_kinds: HashMap<u32, u32>,
    grounds: HashMap<u32, Option<u32>>,
    scenery: HashMap<(i32, i32), Arc<Vec<SceneryObject>>>,
    frill_vertices: HashMap<u32, Arc<Vec<MeshVertex>>>,
    shadow_layers: HashMap<u32, Option<DrawLayer>>,
    /// The model detail preference, 0 to 2.
    pub model_detail: u32,
    /// How long the client has been running, in seconds: scrolling pictures (the clouds'
    /// shadows) follow it, so frames match another client's only at the same running time.
    pub uptime: f64,
    /// The static object shadows preference: the sun's shadow map of the near objects.
    pub static_shadows: bool,
    /// The cloud shadows preference. Clouds are drawn only with static shadows on, as the client
    /// builds them into the same terrain pass.
    pub cloud_shadows: bool,
    /// The water reflections preference.
    pub water_reflections: bool,
    /// The light coronas preference: the sky objects' lens flares.
    pub light_coronas: bool,
    /// How many blocks about the viewer's the far landscape reaches.
    far_radius: i32,
    /// How many land cells about the viewer's carry their frills, whether frills are drawn at
    /// all, and how much of that reach they show before they fade (all of it, or less at the
    /// lowest frill distance).
    frill_radius: i32,
    frills: bool,
    frill_reach: f32,
    /// Each block's interior light, each interior cell's mesh vertices lit by it (with the day
    /// outside they were lit with), and the lights each interior cell keeps, from the first frame
    /// that drew it.
    pub(crate) block_lights: HashMap<(i32, i32), Option<Arc<auberean_world::inside::BlockLights>>>,
    pub(crate) lit_cells: HashMap<u32, LitCell>,
    /// By cell id and 0 for a cell, by cell id and one more than its place in the cell's list for
    /// an object standing in it.
    pub(crate) kept_lights: HashMap<(u32, u32), Vec<usize>>,
    /// Each placed object's look when it plays startup effects, with its scale.
    effect_looks: HashMap<u32, Option<(Arc<Look>, Vec3)>>,
    /// The detailed blocks' ground the last frame outdoors was built on, by block: what decals
    /// are laid on.
    ground: HashMap<(i32, i32), Arc<TerrainBlock>>,
    /// Each interior cell's appearance, by full cell id.
    cell_looks: HashMap<u32, Option<Arc<PartLook>>>,
    /// The sky's objects as made, by what each is made from.
    sky_parts: HashMap<u32, Arc<Vec<Part>>>,
    /// Each interior cell's vertices lit by the lights its record lists for them, by full id, as
    /// baked when the cell was first made ready.
    pub(crate) cell_bakes: HashMap<u32, Arc<auberean_world::inside::CellBake>>,
    /// The client's generator, which a light whose intensity is noise rolls when a cell is baked.
    pub dice: auberean_primitives::random::Random,
    /// Each cell mesh's doorways, by mesh id: each portal's polygon in the mesh's own frame.
    pub(crate) cell_doorways: HashMap<u32, Arc<Vec<Vec<Vec3>>>>,
    /// The interior fog's far distance each interior cell (by full id) was given when it last held
    /// the camera ([`crate::cull`]).
    pub(crate) fog_cuts: HashMap<u32, Option<f32>>,
}

impl std::fmt::Debug for SceneBuilder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneBuilder")
            .field("meshes", &self.meshes.len())
            .finish_non_exhaustive()
    }
}

/// A cell id's low word.
fn cell_low(low: u32) -> u16 {
    u16::try_from(low & 0xFFFF).unwrap_or(0)
}

/// The landblocks a frame reads, by block coordinates.
type Blocks = HashMap<(i32, i32), Landblock>;

/// The four corners of a square in drawing order: south-west, south-east, north-west, north-east,
/// as offsets in vertex steps.
const CORNERS: [(usize, usize); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// A layer mask's coordinates at corner offset `(cx, cy)` for quarter turn `rotation` (1 to 4).
// D2-10888
#[must_use]
pub fn mask_uv(cx: f32, cy: f32, rotation: u8) -> [f32; 2] {
    match rotation {
        2 => [1.0 - cy, 1.0 - cx],
        3 => [1.0 - cx, cy],
        4 => [cy, cx],
        _ => [cx, 1.0 - cy],
    }
}

/// The two triangles of a square, as corner slots, for its split.
fn square_triangles(cut: bool) -> [u32; 6] {
    if cut {
        [2, 0, 3, 0, 1, 3]
    } else {
        [0, 1, 2, 1, 3, 2]
    }
}

impl<'a> SceneBuilder<'a> {
    #[must_use]
    pub fn new(records: &'a WorldRecords, region: &'a RegionSet) -> Self {
        Self {
            records,
            region,
            meshes: HashMap::new(),
            materials: HashMap::new(),
            looked: HashMap::new(),
            material_kinds: HashMap::new(),
            grounds: HashMap::new(),
            scenery: HashMap::new(),
            frill_vertices: HashMap::new(),
            shadow_layers: HashMap::new(),
            model_detail: 2,
            uptime: 0.0,
            static_shadows: true,
            cloud_shadows: true,
            water_reflections: true,
            light_coronas: true,
            far_radius: FAR_RADIUS,
            frill_radius: FRILL_CELLS,
            frills: true,
            frill_reach: 1.0,
            block_lights: HashMap::new(),
            ground: HashMap::new(),
            lit_cells: HashMap::new(),
            kept_lights: HashMap::new(),
            effect_looks: HashMap::new(),
            cell_looks: HashMap::new(),
            sky_parts: HashMap::new(),
            cell_bakes: HashMap::new(),
            dice: auberean_primitives::random::Random::new(),
            cell_doorways: HashMap::new(),
            fog_cuts: HashMap::new(),
        }
    }

    /// How many blocks about the viewer's the far landscape reaches, and with it the window of
    /// blocks the landscape keeps (twice as many and one across). The detailed blocks about the
    /// viewer are drawn however small it is.
    // D2-10771, D2-10758
    pub fn set_far_radius(&mut self, radius: i32) {
        self.far_radius = radius.max(0);
    }

    /// The far landscape's reach, in blocks.
    #[must_use]
    pub fn far_radius(&self) -> i32 {
        self.far_radius
    }

    /// Frills drawn or not; in the land cells within `radius` of the viewer's, fading out over the
    /// last eight meters of `reach` (a share) of those cells' distance.
    // D2-10848
    pub fn set_frills(&mut self, on: bool, radius: i32, reach: f32) {
        self.frills = on;
        self.frill_radius = radius.max(0);
        self.frill_reach = reach;
    }

    /// Whether frills are drawn, and the land cells about the viewer's that carry them.
    #[must_use]
    pub fn frills(&self) -> (bool, i32) {
        (self.frills, self.frill_radius)
    }

    pub(crate) fn mesh(&mut self, id: u32) -> Option<(Arc<Mesh>, Arc<Vec<MeshVertex>>)> {
        let records = self.records;
        self.meshes
            .entry(id)
            .or_insert_with(|| {
                let mesh = if id >> 24 == 0x01 {
                    records.get::<CellMesh>(id).map(|c| c.mesh)
                } else {
                    records.get::<Mesh>(id)
                }?;
                let v = vertices(&mesh.vertices)?;
                Some((Arc::new(mesh), Arc::new(v)))
            })
            .clone()
    }

    fn material(&mut self, id: u32) -> Vec<DrawLayer> {
        let records = self.records;
        self.materials
            .entry(id)
            .or_insert_with(|| {
                instance_material(records, id)
                    .map(|m| draw_layers(&m, DEVICE_FLAGS))
                    .unwrap_or_default()
            })
            .clone()
    }

    /// A fragment's material as a part's look leaves it.
    // D2-06635, D2-05609
    fn looked_material(&mut self, look: &PartLook, mesh_kind: u32, id: u32) -> Vec<DrawLayer> {
        let records = self.records;
        let kind = *self
            .material_kinds
            .entry(id)
            .or_insert_with(|| material_kind(records, id));
        let (template, extra) = look.material(mesh_kind, kind, &mut |a| {
            (a.modifier.is_none() || records.bytes(a.modifier.0).is_some())
                && (a.template.is_none() || records.bytes(a.template.0).is_some())
        });
        if template.is_none() && extra.is_empty() {
            return self.material(id);
        }
        let key = (
            id,
            template.map(|t| t.0),
            extra.iter().map(|d| d.0).collect(),
        );
        self.looked
            .entry(key)
            .or_insert_with_key(|(id, template, extra)| {
                instance_material_with(records, *id, *template, extra)
                    .map(|m| draw_layers(&m, DEVICE_FLAGS))
                    .unwrap_or_default()
            })
            .clone()
    }

    /// What interior cell `id` (its full id) of record `cell` looks like: its appearance's changes
    /// to its mesh's materials.
    pub(crate) fn cell_look(
        &mut self,
        id: u32,
        cell: &auberean_assets::landblock::Cell,
    ) -> Option<Arc<PartLook>> {
        let records = self.records;
        self.cell_looks
            .entry(id)
            .or_insert_with(|| cell_look(records, cell).map(Arc::new))
            .clone()
    }

    /// Entity `index` of cell `cell` (0: the block's own list) of block `block`, when its look
    /// plays startup effects.
    pub(crate) fn placed_look(
        &mut self,
        e: &auberean_assets::entity::Entity,
        block: (i32, i32),
        cell: u16,
        index: usize,
    ) -> Option<PlacedLook> {
        let (object, place, own) = entity_place(e)?;
        let records = self.records;
        let (look, scale) = self
            .effect_looks
            .entry(object)
            .or_insert_with(|| object_effect_look(records, object))
            .clone()?;
        Some(PlacedLook {
            key: PlacedLook::key(block, cell, index),
            block,
            look,
            place,
            scale: Vec3::new(own.x * scale.x, own.y * scale.y, own.z * scale.z),
        })
    }

    fn shadow_layer(&mut self, id: u32) -> Option<DrawLayer> {
        let records = self.records;
        self.shadow_layers
            .entry(id)
            .or_insert_with(|| instance_material(records, id).and_then(|m| shadow_layer(&m)))
            .clone()
    }

    /// A part as it is drawn into the shadow map: at full detail, each fragment with its
    /// material's first layer.
    // D2-05492, D2-03691
    fn caster_batches(&mut self, part: &Part, place: &Mat4, out: &mut Vec<ObjectBatch>) {
        let Some((mesh, verts)) = self.mesh(part.mesh.0) else {
            return;
        };
        let Some(level) = level_at(&mesh, 1.0, self.model_detail) else {
            return;
        };
        let transform = part.transform.then(place);
        for frag in &mesh.levels[level].fragments {
            let Some(mat) = usize::try_from(frag.material)
                .ok()
                .and_then(|k| mesh.materials.get(k))
            else {
                continue;
            };
            let Some(layer) = self.shadow_layer(mat.0) else {
                continue;
            };
            out.push(ObjectBatch {
                vertices: verts.clone(),
                indices: Arc::new(frag.indices.clone()),
                transform,
                layer,
                tint: Color::WHITE,
                points: PointLights::NONE,
                mesh_type: mesh.kind,
                shade: None,
                outside: false,
                cull: Cull::Never,
                mesh: part.mesh.0,
                casts_volume: false,
            });
        }
    }

    fn ground(&mut self, material: u32) -> Option<u32> {
        let records = self.records;
        *self
            .grounds
            .entry(material)
            .or_insert_with(|| ground_picture(records, material))
    }

    /// The batches of a part seen from `distance`, at the detail level its setup's distances give.
    pub(crate) fn part_batches(
        &mut self,
        part: &Part,
        place: &Mat4,
        distance: f32,
        tint: Color,
        counts: &mut Counts,
    ) -> Vec<ObjectBatch> {
        self.batches_of(part, place, distance, tint, false, counts)
    }

    /// The batches of a part; a frill's vertices all take its own up axis as their normal, so
    /// it is lit as the ground under it is.
    fn batches_of(
        &mut self,
        part: &Part,
        place: &Mat4,
        distance: f32,
        tint: Color,
        frill: bool,
        counts: &mut Counts,
    ) -> Vec<ObjectBatch> {
        let Some((mesh, verts)) = self.mesh(part.mesh.0) else {
            counts.missing_meshes += 1;
            return Vec::new();
        };
        let verts = if frill {
            self.frill_vertices
                .entry(part.mesh.0)
                .or_insert_with(|| {
                    Arc::new(
                        verts
                            .iter()
                            .map(|v| MeshVertex {
                                normal: [0.0, 0.0, 1.0],
                                ..*v
                            })
                            .collect(),
                    )
                })
                .clone()
        } else {
            verts
        };
        let model_detail = usize::try_from(self.model_detail).unwrap_or(2).min(2);
        let t = match &part.detail {
            Some(d) => match detail_at(&d[model_detail], distance) {
                Some(t) => t,
                None => return Vec::new(),
            },
            None => 1.0,
        };
        let Some(level) = level_at(&mesh, t, self.model_detail) else {
            return Vec::new();
        };
        counts.parts += 1;
        let transform = part.transform.then(place);
        // Frills are never culled; anything else standing outdoors by its box.
        let cull = if frill {
            Cull::Never
        } else {
            let b = mesh.bounding_box();
            let (lo, hi) = placed_box(
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
                &transform,
            );
            Cull::Box { lo, hi }
        };
        let mut out = Vec::new();
        for frag in &mesh.levels[level].fragments {
            let Some(mat) = usize::try_from(frag.material)
                .ok()
                .and_then(|k| mesh.materials.get(k))
            else {
                continue;
            };
            let layers = match &part.look {
                Some(look) => self.looked_material(look, mesh.kind, mat.0),
                None => self.material(mat.0),
            };
            let indices = Arc::new(frag.indices.clone());
            for layer in layers {
                out.push(ObjectBatch {
                    vertices: verts.clone(),
                    indices: indices.clone(),
                    transform,
                    layer,
                    tint,
                    points: PointLights::NONE,
                    mesh_type: mesh.kind,
                    shade: None,
                    outside: false,
                    cull,
                    mesh: part.mesh.0,
                    casts_volume: false,
                });
            }
        }
        out
    }

    /// The two triangles of the landscape square holding `at` (a point in the frame of block
    /// `frame_block`), each corner its place in that frame and its normal, as the last frame
    /// outdoors built the detailed blocks; none for a square it did not build or a hole.
    // D2-10926
    #[must_use]
    pub fn ground_square(
        &self,
        frame_block: (i32, i32),
        at: Vec3,
    ) -> Option<[crate::decal::Corner; 6]> {
        let grid = self.region.land.grid();
        let (block_len, square) = (grid.block_length(), grid.vertex_spacing());
        let (fx, fy) = (
            dereth_primitives::num::floor_to_i32(at.x / block_len),
            dereth_primitives::num::floor_to_i32(at.y / block_len),
        );
        let tb = self.ground.get(&(frame_block.0 + fx, frame_block.1 + fy))?;
        #[allow(clippy::cast_precision_loss)] // a few blocks
        let (ox, oy) = (fx as f32 * block_len, fy as f32 * block_len);
        let g = &tb.geometry;
        let i = usize::try_from(dereth_primitives::num::floor_to_i32((at.x - ox) / square)).ok()?;
        let j = usize::try_from(dereth_primitives::num::floor_to_i32((at.y - oy) / square)).ok()?;
        if i >= g.squares_per_side || j >= g.squares_per_side {
            return None;
        }
        let n = i * g.squares_per_side + j;
        if g.holes.get(n).copied().unwrap_or(false) {
            return None;
        }
        let corner = |v: u32| -> Option<crate::decal::Corner> {
            let k = usize::try_from(v).ok()?;
            let p = g.vertices.get(k)?;
            let normal = tb.normals.get(k)?;
            Some((Vec3::new(p.x + ox, p.y + oy, p.z), *normal))
        };
        let (a, b) = (g.triangles.get(2 * n)?, g.triangles.get(2 * n + 1)?);
        Some([
            corner(a.v[0])?,
            corner(a.v[1])?,
            corner(a.v[2])?,
            corner(b.v[0])?,
            corner(b.v[1])?,
            corner(b.v[2])?,
        ])
    }

    /// The length of a landscape square's side, in meters.
    #[must_use]
    pub fn square_side(&self) -> f32 {
        self.region.land.grid().vertex_spacing()
    }

    /// The frame seen from `spot`.
    #[must_use]
    pub fn frame(&mut self, spot: &Spot, fov_y: f32) -> Frame {
        self.frame_without(spot, fov_y, &[])
    }

    /// The frame seen from `spot`, leaving out the interior cells `inside` (full ids) where they
    /// would be seen from outside: a frame seen from inside draws those cells as rooms.
    pub(crate) fn frame_without(&mut self, spot: &Spot, fov_y: f32, inside: &[u32]) -> Frame {
        let land = &self.region.land;
        let block_len = land.grid().block_length();
        let mut counts = Counts::default();

        // The light of the moment.
        let day = self.day(spot);
        let light = self.light(spot, day.as_ref());

        // The blocks in the window, and one more ring for their normals.
        let far = self.far_radius.max(NEAR_RADIUS);
        let mut window = LandblockWindow::<()>::with_spec(
            u32::try_from(far).unwrap_or(4),
            WindowSpec {
                block_length: block_len,
                detail: Detail::Full,
            },
        );
        let wanted: Vec<(i32, i32)> = window
            .update_block(spot.block)
            .into_iter()
            .filter_map(|a| match a {
                SlotAction::Fetched {
                    block_x, block_y, ..
                } => Some((block_x, block_y)),
                _ => None,
            })
            .collect();
        let mut blocks: Blocks = HashMap::new();
        for dx in -(far + 1)..=far + 1 {
            for dy in -(far + 1)..=far + 1 {
                let (x, y) = (spot.block.0 + dx, spot.block.1 + dy);
                if let Some(lb) = self.records.landblock(x, y) {
                    blocks.insert((x, y), lb);
                }
            }
        }

        let mut terrain: HashMap<(u32, Option<u32>, u8, bool), TerrainBatch> = HashMap::new();
        let mut near_blocks: HashMap<(i32, i32), TerrainBlock> = HashMap::new();
        let mut water = Vec::new();
        for &(bx, by) in &wanted {
            let Some(tb) = build_block(land, self.region.surfaces(), &blocks, bx, by) else {
                continue;
            };
            counts.blocks += 1;
            // Where the block is drawn: its offset in blocks from the viewer's.
            // D2-10912
            #[allow(clippy::cast_precision_loss)] // window offsets are a few blocks
            let offset = Vec3::new(
                (bx - spot.block.0) as f32 * block_len,
                (by - spot.block.1) as f32 * block_len,
                0.0,
            );
            let near = (bx - spot.block.0).abs().max((by - spot.block.1).abs()) <= NEAR_RADIUS;
            if near {
                self.near_terrain(&tb, offset, &light, &mut terrain, &mut counts);
            } else {
                self.far_terrain(&tb, offset, &light, spot.block, &mut terrain);
                // The far blocks' water: one surface each, with the far water material.
                if let Some(w) = self.block_water(&tb) {
                    let material = self.region.surface.far_water_material;
                    let w = BlockWater {
                        above_material: if material.0 == 0 {
                            w.above_material
                        } else {
                            material
                        },
                        poly_width: block_len,
                        ..w
                    };
                    counts.water_blocks += 1;
                    water.extend(self.water_batches(&w, &Mat4::translation(offset)));
                }
            }
            if near {
                near_blocks.insert((bx, by), tb);
            }
        }
        let cell_len = land.grid().cell_length;
        let cells = i32::try_from(land.grid().cells_per_side).unwrap_or(8);
        let viewer_cell = (
            spot.block.0 * cells + floor_to_i32(spot.position.x / cell_len),
            spot.block.1 * cells + floor_to_i32(spot.position.y / cell_len),
        );

        // Placed objects, scenery and the interiors seen from outside, in the detailed blocks.
        // D2-11034, D2-10922
        let mut objects = Vec::new();
        let mut placed = Vec::new();
        let mut casters: Vec<(Part, Mat4)> = Vec::new();
        for &(bx, by) in &wanted {
            #[allow(clippy::cast_precision_loss)]
            let place = Mat4::translation(Vec3::new(
                (bx - spot.block.0) as f32 * block_len,
                (by - spot.block.1) as f32 * block_len,
                0.0,
            ));
            // Only the detailed blocks around the viewer have their objects; the far landscape is
            // ground alone.
            if (bx - spot.block.0).abs().max((by - spot.block.1).abs()) > NEAR_RADIUS {
                continue;
            }
            let info = self.records.landblock_info(bx, by);
            if let Some(w) = near_blocks
                .get(&(bx, by))
                .and_then(|tb| self.block_water(tb))
            {
                counts.water_blocks += 1;
                water.extend(self.water_batches(&w, &place));
            }
            let mut parts: Vec<Part> = Vec::new();
            let mut frill_parts: Vec<Part> = Vec::new();
            if let Some(tb) = near_blocks.get(&(bx, by)) {
                for s in self.block_scenery(tb, &blocks, info.as_ref()).iter() {
                    let cell = (
                        bx * cells + floor_to_i32(s.origin.x / cell_len),
                        by * cells + floor_to_i32(s.origin.y / cell_len),
                    );
                    // Frills only in the cells about the viewer's, and none when turned off.
                    // D2-10843, D2-10858
                    if s.frill
                        && (!self.frills
                            || (cell.0 - viewer_cell.0).abs() > self.frill_radius
                            || (cell.1 - viewer_cell.1).abs() > self.frill_radius)
                    {
                        continue;
                    }
                    let world = Mat4::from_parts(s.origin, s.rotation, Vec3::new(1.0, 1.0, 1.0));
                    let p = parts_of(self.records, s.object, &world, s.scale);
                    if !p.is_empty() {
                        counts.scenery += 1;
                        counts.frills += usize::from(s.frill);
                    }
                    if s.frill {
                        frill_parts.extend(p);
                    } else {
                        parts.extend(p);
                    }
                }
            }
            for part in &frill_parts {
                let at = part.transform.then(&place).point(Vec3::ZERO);
                let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                let first = objects.len();
                objects.extend(self.batches_of(part, &place, d, Color::WHITE, true, &mut counts));
                // D2-10837
                for b in &mut objects[first..] {
                    b.shade = Some(at);
                }
            }
            let Some(info) = info else {
                casters.extend(parts.iter().map(|p| (p.clone(), place)));
                for part in &parts {
                    let at = part.transform.then(&place).point(Vec3::ZERO);
                    let d =
                        auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                    objects.extend(self.part_batches(part, &place, d, Color::WHITE, &mut counts));
                }
                continue;
            };
            for (i, e) in info.entities.entities.iter().enumerate() {
                let p = entity_parts(self.records, e);
                if !p.is_empty() {
                    counts.objects += 1;
                    placed.extend(self.placed_look(e, (bx, by), 0, i));
                }
                parts.extend(p);
            }
            // The block's objects and its scenery cast shadows; the interiors seen from outside
            // do not.
            casters.extend(parts.iter().map(|p| (p.clone(), place)));
            for &low in &info.outside_visible {
                #[allow(clippy::cast_sign_loss)]
                let id = ((bx as u32) << 24) | ((by as u32) << 16) | (low & 0xFFFF);
                if inside.contains(&id) {
                    continue;
                }
                let Some(cell) = u16::try_from(low & 0xFFFF)
                    .ok()
                    .and_then(|l| self.records.cell(bx, by, l))
                else {
                    continue;
                };
                counts.cells += 1;
                let frame = Mat4::from_parts(
                    cell.frame.origin,
                    cell.frame.rotation,
                    Vec3::new(1.0, 1.0, 1.0),
                );
                let look = self.cell_look(id, &cell);
                parts.push(Part {
                    mesh: cell.mesh,
                    transform: frame,
                    detail: None,
                    look,
                });
                for (i, e) in cell
                    .entities
                    .iter()
                    .flat_map(|g| g.entities.iter())
                    .enumerate()
                {
                    let p = entity_parts(self.records, e);
                    if !p.is_empty() {
                        counts.objects += 1;
                        placed.extend(self.placed_look(e, (bx, by), cell_low(low), i));
                    }
                    parts.extend(p);
                }
            }
            for part in &parts {
                let at = part.transform.then(&place).point(Vec3::ZERO);
                let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                objects.extend(self.part_batches(part, &place, d, Color::WHITE, &mut counts));
            }
        }

        // The sun's shadow map over the detailed terrain, and the clouds' shadows.
        let shadow = if self.static_shadows {
            let mut batches = Vec::new();
            for (part, place) in &casters {
                self.caster_batches(part, place, &mut batches);
            }
            counts.shadow_casters = batches.len();
            let ground: Vec<Vec3> = near_blocks
                .values()
                .flat_map(|tb| {
                    #[allow(clippy::cast_precision_loss)]
                    let (ox, oy) = (
                        (tb.x - spot.block.0) as f32 * block_len,
                        (tb.y - spot.block.1) as f32 * block_len,
                    );
                    tb.geometry
                        .vertices
                        .iter()
                        .map(move |v| Vec3::new(v.x + ox, v.y + oy, v.z))
                })
                .collect();
            ShadowMap::new(light.light_direction, ground, light.shadow_color, batches)
        } else {
            None
        };
        let reflection = if self.water_reflections {
            self.reflection_plane(&near_blocks, spot)
        } else {
            None
        };
        self.ground = near_blocks
            .iter()
            .map(|(k, tb)| (*k, Arc::new(tb.clone())))
            .collect();
        let clouds = (shadow.is_some() && self.cloud_shadows)
            .then(|| {
                let s = &self.region.surface;
                (s.cloud_texture.0 != 0).then_some(CloudShadows {
                    picture: s.cloud_texture.0,
                    scale: s.cloud_texture_scale,
                    scroll: [s.cloud_scroll_u, s.cloud_scroll_v],
                })
            })
            .flatten();

        // The sky around the viewer, and its objects' flares.
        let (sky_before, sky_after) =
            self.sky(spot.position, day.as_ref(), spot.time_of_day, &mut counts);
        let flares = match (&day, self.light_coronas) {
            (Some(d), true) => crate::flare::sky_flares(d, spot.time_of_day),
            _ => Vec::new(),
        };

        let mut terrain: Vec<TerrainBatch> = terrain.into_values().collect();
        terrain.sort_by_key(|b| (b.far, b.layer, b.ground, b.mask));
        Frame {
            camera: Camera {
                position: spot.position,
                heading: spot.heading,
                pitch: spot.pitch,
                fov_y,
                near: 0.1,
                far: 2000.0,
                orthographic: None,
            },
            light,
            sky_before,
            terrain,
            objects,
            sky_after,
            frill_fade: {
                #[allow(clippy::cast_precision_loss)]
                let end = self.frill_radius as f32 * cell_len * self.frill_reach;
                [end - FRILL_FADE, end]
            },
            counts,
            shadow,
            clouds,
            uptime: self.uptime,
            water,
            reflection,
            placed,
            flares,
            outside: None,
            rooms: None,
            volumes: crate::volume::Volumes::default(),
            decals: Vec::new(),
        }
    }

    /// The sky's day at `spot`: its own, else the region's first.
    #[must_use]
    pub fn day(&self, spot: &Spot) -> Option<auberean_assets::region::DayDesc> {
        let day = spot.day.or_else(|| {
            self.region
                .sky
                .as_ref()
                .and_then(|s| s.days.first().map(|d| d.day))
        });
        day.and_then(|d| self.records.day(d))
    }

    /// The light and fog at `spot` on day `day`: the day's at the moment (or the light with no
    /// sky), with the region's fog blocks about the viewer combined and blended by where it stands,
    /// in the block it stands in (which may be beside the block `spot` names).
    // D2-10903, D2-10904, D2-10865, D2-10866
    #[must_use]
    pub fn light(&self, spot: &Spot, day: Option<&auberean_assets::region::DayDesc>) -> DayLight {
        let light = day.map_or(DayLight::NO_SKY, |d| day_light(d, spot.time_of_day));
        let len = self.region.land.grid().block_length();
        let (dx, dy) = (
            floor_to_i32(spot.position.x / len),
            floor_to_i32(spot.position.y / len),
        );
        #[allow(clippy::cast_precision_loss)] // a block or two
        let position = Vec3::new(
            spot.position.x - dx as f32 * len,
            spot.position.y - dy as f32 * len,
            spot.position.z,
        );
        match (&self.region.fog, &self.region.fog_kinds) {
            (Some(desc), Some(kinds)) => auberean_world::fog::block_light(
                &light,
                desc,
                kinds,
                (spot.block.0 + dx, spot.block.1 + dy),
                position,
                len,
            ),
            _ => light,
        }
    }

    /// The sky's objects at fraction `t` of day `day`, standing at the viewer: those drawn before
    /// the world, then those drawn after it. Each sky object is made from what the moment shows of
    /// it (its own model or a time of day's replacement) the first time it shows that, and kept;
    /// every frame it stands at the viewer's place, turned as the moment turns it.
    // D2-10942, D2-10939
    pub fn sky(
        &mut self,
        viewer: Vec3,
        day: Option<&auberean_assets::region::DayDesc>,
        t: f32,
        counts: &mut Counts,
    ) -> (Vec<ObjectBatch>, Vec<ObjectBatch>) {
        let (mut before, mut after) = (Vec::new(), Vec::new());
        let Some(day) = day else {
            return (before, after);
        };
        for piece in sky_pieces(day, t) {
            counts.sky_objects += 1;
            let world = Mat4::from_parts(viewer, piece.rotation, Vec3::new(1.0, 1.0, 1.0));
            let records = self.records;
            let parts = self
                .sky_parts
                .entry(piece.object.0)
                .or_insert_with(|| {
                    Arc::new(
                        parts_of(
                            records,
                            piece.object,
                            &Mat4::IDENTITY,
                            Vec3::new(1.0, 1.0, 1.0),
                        )
                        .into_iter()
                        .map(|part| Part {
                            detail: None,
                            ..part
                        })
                        .collect(),
                    )
                })
                .clone();
            for part in parts.iter() {
                let batches = self.part_batches(part, &world, 0.0, piece.color, counts);
                if piece.after {
                    after.extend(batches);
                } else {
                    before.extend(batches);
                }
            }
        }
        (before, after)
    }

    /// A frame of placed parts and bodies alone, as an interface's scene draws: no ground, no sky,
    /// no water, every part at its nearest detail, seen by `camera` in `light`.
    // D2-10472, D2-10600
    pub fn parts_frame(
        &mut self,
        parts: &[Part],
        bodies: Vec<ObjectBatch>,
        camera: Camera,
        light: DayLight,
    ) -> Frame {
        let mut counts = Counts::default();
        let mut objects = Vec::new();
        for part in parts {
            objects.extend(self.part_batches(
                part,
                &Mat4::IDENTITY,
                0.0,
                Color::WHITE,
                &mut counts,
            ));
        }
        counts.objects = parts.len();
        objects.extend(bodies);
        Frame {
            camera,
            light,
            sky_before: Vec::new(),
            terrain: Vec::new(),
            objects,
            sky_after: Vec::new(),
            frill_fade: [f32::MAX, f32::MAX],
            counts,
            shadow: None,
            clouds: None,
            uptime: self.uptime,
            water: Vec::new(),
            reflection: None,
            placed: Vec::new(),
            flares: Vec::new(),
            outside: None,
            rooms: None,
            volumes: crate::volume::Volumes::default(),
            decals: Vec::new(),
        }
    }

    /// The plane the water reflects about: the viewer's own block's water when it lies below the
    /// camera, else the first detailed block's that does.
    // D2-10921
    fn reflection_plane(
        &self,
        near_blocks: &HashMap<(i32, i32), TerrainBlock>,
        spot: &Spot,
    ) -> Option<Reflection> {
        let below = |b: (i32, i32)| {
            near_blocks
                .get(&b)
                .and_then(|tb| self.block_water(tb))
                .filter(|w| w.height < spot.position.z)
        };
        let mut order = vec![spot.block];
        for dx in -NEAR_RADIUS..=NEAR_RADIUS {
            for dy in -NEAR_RADIUS..=NEAR_RADIUS {
                order.push((spot.block.0 + dx, spot.block.1 + dy));
            }
        }
        order
            .into_iter()
            .find_map(below)
            .map(|w| Reflection { height: w.height })
    }

    /// A block's water, if it has any: its surface stands above the block's lowest ground.
    fn block_water(&self, tb: &TerrainBlock) -> Option<BlockWater> {
        let low = tb
            .geometry
            .vertices
            .iter()
            .map(|v| v.z)
            .fold(f32::INFINITY, f32::min);
        self.region
            .water
            .as_ref()?
            .block(&self.region.land, tb.x, tb.y, low)
    }

    /// A block's water surface: flat across the whole block at the water's height, one polygon
    /// per the water's polygon width, its picture repeating every tile width (the first
    /// coordinate set) and 0..1 across each polygon (the second, which the reflection's ripples
    /// follow), drawn with the material seen from above.
    // D2-10986, D2-10977, D2-10975
    fn water_batches(&mut self, water: &BlockWater, place: &Mat4) -> Vec<ObjectBatch> {
        let bl = self.region.land.grid().block_length();
        let poly = if water.poly_width > 0.0 {
            water.poly_width.min(bl)
        } else {
            bl
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((bl / poly).round() as usize).clamp(1, 64);
        #[allow(clippy::cast_precision_loss)]
        let step = bl / n as f32;
        let tile = if water.tile_width > 0.0 {
            water.tile_width
        } else {
            bl
        };
        let mut verts = Vec::with_capacity((n + 1) * (n + 1));
        for i in 0..=n {
            for j in 0..=n {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f32 * step, j as f32 * step);
                verts.push(MeshVertex {
                    position: [x, y, water.height],
                    normal: [0.0, 0.0, 1.0],
                    diffuse: 0xFFFF_FFFF,
                    // the picture's repeats, then each polygon's own 0..1 (the ripples' set)
                    uv: [
                        [x / tile, y / tile],
                        [x / step, y / step],
                        [x / tile, y / tile],
                        [x / tile, y / tile],
                    ],
                    tangent_u: [1.0, 0.0, 0.0],
                    tangent_v: [0.0, 1.0, 0.0],
                    to_light: [0.0; 3],
                });
            }
        }
        let mut indices = Vec::with_capacity(6 * n * n);
        for i in 0..n {
            for j in 0..n {
                let v = |a: usize, b: usize| u16::try_from(a * (n + 1) + b).unwrap_or(0);
                let (sw, se, nw, ne) = (v(i, j), v(i + 1, j), v(i, j + 1), v(i + 1, j + 1));
                indices.extend([sw, se, nw, se, ne, nw]);
            }
        }
        let (verts, indices) = (Arc::new(verts), Arc::new(indices));
        self.material(water.above_material.0)
            .into_iter()
            .map(|layer| ObjectBatch {
                vertices: verts.clone(),
                indices: indices.clone(),
                transform: *place,
                layer,
                tint: Color::WHITE,
                points: PointLights::NONE,
                mesh_type: 0,
                shade: None,
                outside: false,
                cull: Cull::Never,
                mesh: 0,
                casts_volume: false,
            })
            .collect()
    }

    /// The scenery of one block, placed once and kept.
    fn block_scenery(
        &mut self,
        tb: &TerrainBlock,
        blocks: &Blocks,
        info: Option<&auberean_assets::landblock::LandblockInfo>,
    ) -> Arc<Vec<SceneryObject>> {
        if let Some(s) = self.scenery.get(&(tb.x, tb.y)) {
            return s.clone();
        }
        let placed = match (self.region.scenes.as_ref(), blocks.get(&(tb.x, tb.y))) {
            (Some(scenes), Some(lb)) => {
                // The block's own objects keep scenery away from where they stand, or from what
                // their clearance properties ask to be kept clear.
                let records = self.records;
                let lookup =
                    |id: u32| auberean_world::place::PlaceRecords::entity(records, DataId(id));
                let obstacles: Vec<KeptClear> = info
                    .map(|i| {
                        i.entities
                            .entities
                            .iter()
                            .filter(|e| {
                                e.kind.is_some_and(|k| {
                                    auberean_world::place::DRAWN_KINDS.contains(&k)
                                })
                            })
                            .filter_map(|e| {
                                let f = e.offset.as_ref()?.frame;
                                let total = e.total_properties(&lookup);
                                Some(KeptClear::of_entity(f.origin, f.rotation, Some(&total)))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let input = SceneryInput {
                    land: &self.region.land,
                    terrain: &self.region.terrain,
                    scenes,
                    vertices: &lb.vertices,
                    block: tb,
                    obstacles: &obstacles,
                    degrade: 1.0,
                    water: self.block_water(tb).map(|w| w.height),
                };
                block_scenery(&input, self.records)
            }
            _ => Vec::new(),
        };
        let placed = Arc::new(placed);
        self.scenery.insert((tb.x, tb.y), placed.clone());
        placed
    }

    /// The detailed landscape of one block: each square's layers as quads.
    // D2-10981, D2-10886, D2-10890
    fn near_terrain(
        &mut self,
        tb: &TerrainBlock,
        offset: Vec3,
        light: &DayLight,
        out: &mut HashMap<(u32, Option<u32>, u8, bool), TerrainBatch>,
        counts: &mut Counts,
    ) {
        let squares = tb.geometry.squares_per_side;
        let step = self.region.land.grid().vertex_spacing();
        // A vertex the sun's direction leaves in its own hill's shadow gets no sunlight.
        let sun_bit = auberean_world::sky::self_shadow_bit(light.light_direction);
        for i in 0..squares {
            for j in 0..squares {
                let look = &tb.squares[i * squares + j];
                counts.squares += 1;
                if look.hole {
                    counts.holes += 1;
                    continue;
                }
                let cut = tb.geometry.sw_to_ne_cut[i * squares + j];
                for (n, layer) in look.layers.iter().enumerate() {
                    let pick = tb.layer_pick(i, j, layer);
                    let Some(ground) = self.ground(pick.base.0) else {
                        continue;
                    };
                    let mask = tb.layer_mask(i, j, layer).filter(|(m, _)| !m.is_none());
                    if n > 0 && mask.is_none() {
                        continue;
                    }
                    let key = (
                        ground,
                        mask.map(|m| m.0 .0),
                        u8::try_from(n.min(2)).unwrap_or(2),
                        false,
                    );
                    let batch = out.entry(key).or_insert_with(|| TerrainBatch {
                        ground,
                        mask: key.1,
                        layer: key.2,
                        far: false,
                        ..TerrainBatch::default()
                    });
                    let t = if pick.base_tile > 0.0 {
                        step / pick.base_tile
                    } else {
                        0.5
                    };
                    let base = u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX);
                    for (cx, cy) in CORNERS {
                        let k = tb.vertex(i + cx, j + cy);
                        let p = tb.geometry.vertices[k];
                        let own = tb.terrain[k].color;
                        let lit = auberean_world::sky::sun_reaches(tb.shadow_bits[k], sun_bit);
                        // D2-10946
                        let c = auberean_world::sky::lit_vertex(light, tb.normals[k], own, lit);
                        #[allow(clippy::cast_precision_loss)] // vertex indices within a block
                        let (u, v) = ((i + cx) as f32 * t, (j + 1 - cy) as f32 * t);
                        #[allow(clippy::cast_precision_loss)]
                        let (fx, fy) = (cx as f32, cy as f32);
                        batch.vertices.push(TerrainVertex {
                            position: [p.x + offset.x, p.y + offset.y, p.z],
                            color: [c.r, c.g, c.b, 1.0],
                            uv: [u, v],
                            mask_uv: mask.map_or([0.0, 0.0], |(_, rot)| mask_uv(fx, fy, rot)),
                        });
                    }
                    batch
                        .indices
                        .extend(square_triangles(cut).iter().map(|&s| base + s));
                }
            }
        }
    }

    /// The far landscape of one block: one picture over lit far colors.
    // D2-10962, D2-10984
    fn far_terrain(
        &mut self,
        tb: &TerrainBlock,
        offset: Vec3,
        light: &DayLight,
        _center: (i32, i32),
        out: &mut HashMap<(u32, Option<u32>, u8, bool), TerrainBatch>,
    ) {
        let surface = &self.region.surface;
        let sun_bit = auberean_world::sky::self_shadow_bit(light.light_direction);
        let ground = surface.far_texture.0;
        let scale = surface.far_texture_scale;
        let key = (ground, None, 0, true);
        let batch = out.entry(key).or_insert_with(|| TerrainBatch {
            ground,
            far: true,
            ..TerrainBatch::default()
        });
        let n = tb.geometry.vertices_per_side;
        let base = u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX);
        for k in 0..n * n {
            let p = tb.geometry.vertices[k];
            let lit = auberean_world::sky::sun_reaches(tb.shadow_bits[k], sun_bit);
            let c =
                auberean_world::sky::lit_vertex(light, tb.normals[k], tb.terrain[k].far_color, lit);
            let (x, y) = (p.x + offset.x, p.y + offset.y);
            batch.vertices.push(TerrainVertex {
                position: [x, y, p.z],
                color: [c.r, c.g, c.b, 1.0],
                uv: [x * scale, y * scale],
                mask_uv: [0.0, 0.0],
            });
        }
        let squares = n - 1;
        for i in 0..squares {
            for j in 0..squares {
                let v = |a: usize, b: usize| base + u32::try_from(a * n + b).unwrap_or(0);
                let (sw, se, nw, ne) = (v(i, j), v(i + 1, j), v(i, j + 1), v(i + 1, j + 1));
                let tri = if tb.geometry.sw_to_ne_cut[i * squares + j] {
                    [nw, sw, ne, sw, se, ne]
                } else {
                    [sw, se, nw, se, ne, nw]
                };
                batch.indices.extend(tri);
            }
        }
    }
}
