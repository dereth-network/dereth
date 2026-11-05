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
use auberean_world::inside::{BlockLights, Outside};
use auberean_world::look::{Look, PartLook};
use auberean_world::math::Mat4;
use auberean_world::place::{cell_look, entity_parts, parts_of, Part};
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::{block_scenery, KeptClear, SceneryInput, SceneryObject};
use auberean_world::sky::{day_light, sky_pieces, DayLight};
use auberean_world::terrain::{build_block, TerrainBlock};
use auberean_world::water::BlockWater;
use dereth_landscape::{Detail, LandblockWindow, SlotAction, WindowSpec};

use crate::cull::LookIn;
use crate::effects::{entity_place, object_effect_look, PlacedLook};
use crate::material::{
    device_flags, draw_layers, ground_picture, instance_material, instance_material_with,
    material_kind, shadow_layer, DrawLayer,
};
use crate::mesh::{level_at, marked_levels, vertices, MeshVertex, PosedVertex};
use crate::shadow::{CloudShadows, ShadowMap};

mod interior;

pub use interior::{inside_light, light_view_draw, Outlook, LIT_IN_VERTICES};

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
    pub vertices: Arc<Vec<TerrainVertex>>,
    pub indices: Arc<Vec<u32>>,
}

/// One drawn piece of a mesh: its vertices, one fragment's triangles, where it stands, and one
/// layer of its material.
#[derive(Clone, Debug)]
pub struct ObjectBatch {
    pub vertices: Arc<Vec<MeshVertex>>,
    /// A body's: where each of `vertices` stands this frame and which way it faces, in place of
    /// the positions and normals they hold.
    pub posed: Option<Arc<Vec<PosedVertex>>>,
    pub indices: Arc<Vec<u16>>,
    pub transform: Mat4,
    pub layer: DrawLayer,
    /// The draw's tint (white, or the sky object's color and opacity).
    pub tint: Color,
    /// Inside, the point lights the draw is lit by in place of the sun; outdoors, those it is lit
    /// by beside the sun (the lights objects carry).
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
    /// For an object the world places (a landblock's or a cell's) that casts a volume of shadow:
    /// which of the frame's placed objects it is, where it stands and which way its shadow falls.
    pub caster: Option<PlacedCaster>,
    /// Its object is drawn with those seen through (its drawing properties say so), every layer
    /// of it, after the water; every layer of another object goes with the opaque objects', blended
    /// or not, marking how near it is or not.
    pub seen_through: bool,
    /// Where a cell, or an object standing in one or outdoors, keeps its lights: the lights
    /// objects carry are taken in with them each frame ([`SceneBuilder::light_carried`]).
    pub kept_by: Option<KeptBy>,
    /// Where its object stands in the frame's world, for an object of several parts each placed
    /// apart: each step draws its objects farthest from the camera first, each object's draws in
    /// their order. `None`: its object stands where the draw's own frame puts it.
    pub anchor: Option<Vec3>,
    /// The step it is drawn in whatever its object: a frill's, a particle's, or one of the
    /// draws that are not the world's objects (portal space's).
    pub step: Option<Pass>,
    /// How far off its object is seen, when it degrades with distance: past this it is gone, and
    /// it fades over the last 7.5% of it ([`fade_objects`]).
    pub visible: Option<f32>,
    /// The landblock its object stands on, for an object standing on the land: it fades in with
    /// the block ([`fade_objects`]).
    pub block: Option<(i32, i32)>,
    /// Its object stands more than 40 m from the camera: a draw of it not wholly opaque is not
    /// tested against its alpha reference ([`crate::material::DrawLayer::faded`]).
    pub far: bool,
    /// Where a body stands on the land: its light and specular take the sun's shadow map about
    /// that spot, as they take the shadow the land throws in the tint's color.
    pub ground: Option<Vec3>,
    /// Which of its object's levels of detail and material it is, for an object that degrades with
    /// distance; its object has a draw for each it may need before the frame is built again.
    pub lod: Option<Lod>,
}

/// One choice of a part's detail and materials among those its distance may give, shown or not
/// from where the camera stands this frame ([`fade_objects`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lod {
    distances: auberean_assets::setup::DetailDistances,
    /// What the distances are scaled by.
    scale: f32,
    /// The material detail preference.
    material_detail: u32,
    /// The last of the levels the model detail marks.
    last: usize,
    /// This draw's place among those levels.
    level: usize,
    /// This draw's materials.
    materials: u32,
    /// It is what the camera sees of the object this frame.
    pub shown: bool,
    /// It is what the water's reflection shows of the object this frame.
    pub reflected: bool,
}

impl Lod {
    /// A draw of a body, built afresh each frame: what the camera sees of it, what the reflection
    /// does, or both.
    #[must_use]
    pub fn of_body(shown: bool, reflected: bool) -> Self {
        Self {
            distances: auberean_assets::setup::DetailDistances::default(),
            scale: 1.0,
            material_detail: 0,
            last: 0,
            level: 0,
            materials: 0,
            shown,
            reflected,
        }
    }

    /// Whether this draw is the choice for an object at `distance`, in a picture taking its
    /// distances at `share`.
    fn is_choice(&self, distance: f32, share: f32) -> bool {
        crate::mesh::degrade_in(
            &self.distances,
            distance,
            self.scale,
            share,
            self.material_detail,
        )
        .is_some_and(|g| {
            crate::mesh::level_place(g.detail, self.last) == self.level
                && g.material_detail == self.materials
        })
    }

    fn update(&mut self, distance: f32) {
        self.shown = self.is_choice(distance, 1.0);
        self.reflected = self.is_choice(distance, REFLECTED_DISTANCE);
    }
}

/// How much of an object's distances the water's reflection sees it by.
pub const REFLECTED_DISTANCE: f32 = 0.4;

/// How much farther or nearer the camera may stand from a part before the frame is built again: the
/// diagonal of the land cell the frame is built for, and a little.
const CELL_REACH: f32 = 36.0;

/// A placed object that casts a volume of shadow, as each of its draws carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedCaster {
    /// Which of the frame's placed objects it is: its draws all carry the same number.
    pub object: u32,
    /// Where it stands, in the frame's world.
    pub at: Vec3,
    /// The way its shadow falls (a unit direction), when its own lights throw it (inside);
    /// `None` for the sun's way.
    pub toward: Option<Vec3>,
    /// The distances its first part takes its detail by, at the builder's model detail.
    pub detail: Option<auberean_assets::setup::DetailDistances>,
}

/// Where a draw keeps its lights: its key among the draws (inside, its cell, and for an object one
/// more than its place in the cell's list, 0 for the cell's own mesh), its block, the cell it stands
/// in (a full id; outdoors, any of its block's land cells), where it stands in its block's frame
/// and how far about that its lights reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeptBy {
    pub key: (u32, u32),
    pub block: (i32, i32),
    pub cell: u32,
    pub at: Vec3,
    pub radius: f32,
    /// It took carried lights the last time it was lit with them.
    pub took_carried: bool,
    /// It keeps lights of its own besides the carried ones: the lights of the cells about a camera
    /// inside. Outdoors, and in an interior cell seen from outside, there are none.
    pub own: bool,
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
    /// An interior cell's own mesh seen from outside, by its block and low word: drawn when a view
    /// onto the land looks into it.
    Looked { block: (i32, i32), cell: u16 },
    /// An object standing in an interior cell seen from outside: drawn when the cell is, and the
    /// object's box (twice its size about its middle) meets a view the cell was entered by.
    LookedIn {
        block: (i32, i32),
        cell: u16,
        lo: Vec3,
        hi: Vec3,
    },
    /// A body standing in interior cells of a block, by their low words (0 past the last): drawn
    /// when one of them is, and the body's box (twice its size about its middle) meets a view that
    /// cell was entered by.
    Body {
        block: (i32, i32),
        cells: [u16; crate::cull::BODY_CELLS],
        lo: Vec3,
        hi: Vec3,
    },
}

impl Cull {
    /// Whether it culls something standing in a cell, seen from the land.
    #[must_use]
    pub const fn is_looked_in(&self) -> bool {
        matches!(
            self,
            Self::Looked { .. } | Self::LookedIn { .. } | Self::Body { .. }
        )
    }
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
        Self {
            inside: true,
            ..Self::outdoors(lights, place)
        }
    }

    /// The point lights of a draw outdoors, lit by the sun and by them (the lights objects carry
    /// that reach it), at most four, placed in the frame's world by `place`.
    #[must_use]
    pub fn outdoors(lights: &[auberean_world::inside::DeviceLight], place: &Mat4) -> Self {
        let mut out = Self::NONE;
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
/// veil over them, the landscape, the interior cells, the decals laid on either, the objects not
/// seen through (every layer of each), and the water; then the frills, the objects seen through,
/// the particle systems, the streaks and the sky's late objects. The water drawn after an object's flame or
/// mist hides it, for neither marks how near it is; the frills and what comes after it, it does
/// not. Each step of objects draws them farthest from the camera first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    Sky,
    Veil,
    Terrain,
    /// The interior cells' own meshes, each layer in turn: their floors are the ground decals
    /// inside are laid on.
    Cells,
    /// The decals laid on the ground.
    Decals,
    Opaque,
    /// The volumes of shadow darken what they enclose.
    Volumes,
    /// The draws of the objects that cast the volumes.
    Casters,
    Water,
    /// The landscape's frills, after the water and the volumes' darkening.
    Frills,
    Translucent,
    /// The particle systems, after every object.
    Particles,
    /// The streaks objects' moving parts leave, after the particle systems.
    Streaks,
    LateSky,
}

/// Every step of a frame of the world, in the order it is drawn.
pub const PASSES: [Pass; 14] = [
    Pass::Sky,
    Pass::Veil,
    Pass::Terrain,
    Pass::Cells,
    Pass::Decals,
    Pass::Opaque,
    Pass::Volumes,
    Pass::Casters,
    Pass::Water,
    Pass::Frills,
    Pass::Translucent,
    Pass::Particles,
    Pass::Streaks,
    Pass::LateSky,
];

impl Pass {
    /// Whether the step draws its objects farthest from the camera first (each object's draws
    /// in their order): the steps of the world's objects.
    #[must_use]
    pub const fn farthest_first(self) -> bool {
        matches!(self, Self::Opaque | Self::Casters | Self::Translucent)
    }
}

/// How far past which a faded draw is no longer tested against its layer's alpha reference.
const FAR_FADE: f32 = 40.0;

/// How opaque each object of the world is drawn from `eye` this frame: at the edge of its visible
/// distance it fades out, and on a landblock of `blocks` (each with its opacity) it fades in with
/// the block. Its visible distance past, it is wholly transparent, and not drawn. Each object's
/// level of detail and materials are chosen here too, from among its draws.
// D2-03680, D2-03691, D2-03784, D2-03789, D2-06658, D2-06218, D2-06330, D2-06576
pub fn fade_objects(objects: &mut [ObjectBatch], eye: Vec3, blocks: &[((u8, u8), f32)]) {
    for b in objects {
        let d = b.distance_from(eye);
        if let Some(lod) = &mut b.lod {
            lod.update(d);
        }
        let mut opacity = b.visible.map_or(1.0, |v| crate::mesh::edge_opacity(v, d));
        if let Some((x, y)) = b.block {
            if let Some((_, o)) = blocks
                .iter()
                .find(|((bx, by), _)| i32::from(*bx) == x && i32::from(*by) == y)
            {
                opacity *= o;
            }
        }
        b.tint.a = opacity.clamp(0.0, 1.0);
        b.far = d > FAR_FADE;
    }
}

/// A place in the frame of block `home` as a place in the frame of `block`.
#[allow(clippy::cast_precision_loss)] // a few blocks
fn sub_block(p: Vec3, block: (i32, i32), home: (i32, i32), block_len: f32) -> Vec3 {
    Vec3::new(
        p.x - (block.0 - home.0) as f32 * block_len,
        p.y - (block.1 - home.1) as f32 * block_len,
        p.z,
    )
}

/// How much of the sun the land lets through at a place of a detailed block: the mean of the four
/// corners of the square holding it, each lit or not by the land's own shadow; nothing under a
/// tenth, whole over nine tenths.
// D2-10970, D2-10925
#[must_use]
pub fn land_shadow(tb: &TerrainBlock, local: Vec3, square_side: f32, light_direction: Vec3) -> f32 {
    let bit = auberean_world::sky::self_shadow_bit(light_direction);
    let side = tb.geometry.squares_per_side;
    let square = |v: f32| {
        usize::try_from(floor_to_i32(v / square_side))
            .unwrap_or(0)
            .min(side - 1)
    };
    let (i, j) = (square(local.x), square(local.y));
    let mean: f32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .iter()
        .map(|&(a, b)| {
            auberean_world::sky::sun_reaches(tb.shadow_bits[tb.vertex(i + a, j + b)], bit)
        })
        .sum::<f32>()
        * 0.25;
    if mean < 0.1 {
        0.0
    } else if mean > 0.9 {
        1.0
    } else {
        mean
    }
}

/// `lo` and `hi` widened to take in `p`. What is not a number widens nothing.
#[inline]
pub fn widen(lo: &mut [f32; 3], hi: &mut [f32; 3], p: [f32; 3]) {
    for i in 0..3 {
        lo[i] = if p[i] < lo[i] { p[i] } else { lo[i] };
        hi[i] = if p[i] > hi[i] { p[i] } else { hi[i] };
    }
}

impl ObjectBatch {
    /// What tells the list of places its vertices stand at from another draw's: draws with the
    /// same one, under the same transform, stand in the same place.
    #[must_use]
    pub fn places(&self) -> usize {
        self.posed
            .as_ref()
            .map_or(Arc::as_ptr(&self.vertices) as usize, |p| {
                Arc::as_ptr(p) as usize
            })
    }

    /// Where vertex `i` stands in the draw's own frame.
    #[must_use]
    pub fn position(&self, i: usize) -> Option<[f32; 3]> {
        match &self.posed {
            Some(p) => p.get(i).map(|v| v.position),
            None => self.vertices.get(i).map(|v| v.position),
        }
    }

    /// `f` of where each vertex stands in the draw's own frame, in order.
    pub fn each_position(&self, mut f: impl FnMut([f32; 3])) {
        match &self.posed {
            Some(p) => p.iter().for_each(|v| f(v.position)),
            None => self.vertices.iter().for_each(|v| f(v.position)),
        }
    }

    /// Whether the draw shows at all: an object fades out to nothing, an opaque layer included.
    // D2-06658
    #[must_use]
    pub fn drawn(&self) -> bool {
        let seen = self.lod.is_none_or(|l| l.shown || l.reflected);
        seen && ((self.visible.is_none() && self.block.is_none()) || self.tint.a > 0.0)
    }

    /// Whether the draw is what the camera sees of its object from where it stands this frame, as
    /// against a level of detail only the reflection shows.
    #[must_use]
    pub fn shown(&self) -> bool {
        self.lod.is_none_or(|l| l.shown)
    }

    /// Whether the draw is seen in the water's reflection, which takes its object's distances at
    /// 40%: it reaches 40% of its visible distance, and its detail falls that much sooner.
    // D2-06658
    #[must_use]
    pub fn reflected(&self, eye: Vec3) -> bool {
        self.lod.map_or_else(
            || {
                self.visible
                    .is_none_or(|v| self.distance_from(eye) <= v * REFLECTED_DISTANCE)
            },
            |l| l.reflected,
        )
    }

    /// How opaque the water's reflection draws it: it takes the distances at 40%, so it fades over
    /// the last 7.5% of its reach there, as it does in the frame over the last of its own.
    // D2-06658
    #[must_use]
    pub fn reflected_alpha(&self, eye: Vec3) -> f32 {
        let Some(v) = self.visible else {
            return self.tint.a;
        };
        let d = self.distance_from(eye);
        let frame = crate::mesh::edge_opacity(v, d);
        if frame <= 0.0 {
            return 0.0;
        }
        (self.tint.a * crate::mesh::edge_opacity(v * REFLECTED_DISTANCE, d) / frame).clamp(0.0, 1.0)
    }

    /// Whether its object stands above `height`, as the objects the water's reflection shows do.
    #[must_use]
    pub fn stands_above(&self, height: f32) -> bool {
        self.anchor
            .unwrap_or_else(|| self.transform.point(Vec3::ZERO))
            .z
            > height
    }

    /// The step an object's draw belongs to, by its object, never its layer: the step it is
    /// given ([`Self::step`]); an interior cell's own mesh, every layer of it in turn, before the
    /// decals; an object seen through, every layer of it with those seen through; another, every
    /// layer of it with the opaque objects (the casters' after the volumes, for an object that
    /// casts one), blended or not, marking how near it is or not.
    // D2-11012, D2-06220
    #[must_use]
    pub fn pass(&self) -> Pass {
        if let Some(step) = self.step {
            return step;
        }
        if matches!(self.cull, Cull::Cell(_)) {
            Pass::Cells
        } else if self.seen_through {
            Pass::Translucent
        } else if self.casts_volume {
            Pass::Casters
        } else {
            Pass::Opaque
        }
    }

    /// The step a draw that is no object of the world goes in by its layer alone (the sky's,
    /// the water's): with what hides what lies behind it when it marks how near it is, else
    /// after them.
    #[must_use]
    pub fn by_layer(&self) -> Pass {
        if self.layer.depth_write {
            Pass::Opaque
        } else {
            Pass::Translucent
        }
    }

    /// How far its object stands from `eye` (see [`Self::anchor`]).
    #[must_use]
    pub fn distance_from(&self, eye: Vec3) -> f32 {
        let at = self
            .anchor
            .unwrap_or_else(|| self.transform.point(Vec3::ZERO));
        auberean_world::math::length(auberean_world::math::sub(at, eye))
    }
}

/// `batches`, the draws of step `pass`, with those drawn under the frame's own light first, then
/// those standing outdoors (the objects on the camera's side, inside or out, then the others, each
/// list in turn); in a step of the world's objects each group's objects farthest from the camera
/// at `eye` first, each object's draws in their order (the sort keeps draws at one distance in
/// turn, where the original's sort may not), else each group in its order. Draws given a step of
/// their own (portal space's) are no objects of the world: they keep their order, before the rest.
// D2-11012, D2-11024
#[must_use]
pub fn in_order<'a>(batches: Vec<&'a ObjectBatch>, pass: Pass, eye: Vec3) -> Vec<&'a ObjectBatch> {
    let (outside, own): (Vec<&ObjectBatch>, Vec<&ObjectBatch>) =
        batches.into_iter().partition(|b| b.outside);
    let order = |group: Vec<&'a ObjectBatch>| -> Vec<&'a ObjectBatch> {
        if !pass.farthest_first() {
            return group;
        }
        let key = |b: &ObjectBatch| {
            if b.step.is_some() {
                f32::INFINITY
            } else {
                b.distance_from(eye)
            }
        };
        let mut keyed: Vec<(f32, &ObjectBatch)> = group.into_iter().map(|b| (key(b), b)).collect();
        keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
        keyed.into_iter().map(|(_, b)| b).collect()
    };
    let mut out = order(own);
    out.extend(order(outside));
    out
}

/// Everything one frame draws.
#[derive(Clone, Debug)]
pub struct Frame {
    pub camera: Camera,
    pub light: DayLight,
    pub sky_before: Vec<ObjectBatch>,
    pub terrain: Vec<TerrainBatch>,
    /// The land as the water's reflection shows it: all of it as the far landscape, the detailed
    /// blocks too.
    pub reflection_land: Option<TerrainBatch>,
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
    /// The strips along the detailed blocks' shores, drawn before the water into the frame's
    /// alpha alone (how much of the water shows near the shore): the shore's alpha material,
    /// black at half alpha.
    pub shore: Vec<ObjectBatch>,
    /// The foam along the shores, drawn after the water and its reflection.
    pub foam: Vec<ObjectBatch>,
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
    /// The blocks' cells a view onto the land may look into.
    pub look_in: Arc<Vec<crate::cull::LookIn>>,
    /// The light those cells are drawn under, with the camera on the land: the interior ambient
    /// and fog of the camera's own block.
    pub looked_light: Option<DayLight>,
    /// The dynamic objects' volumes of shadow, drawn after the opaque objects that cast none.
    pub volumes: crate::volume::Volumes,
    /// The decals laid on the ground this frame, drawn after the landscape.
    pub decals: Vec<ObjectBatch>,
    /// How the frame's textures are taken.
    pub textures: Textures,
}

/// How textures are taken, as the graphics preferences say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Textures {
    /// The texture detail preference, 0 (very low) to 4 (very high): how many of each picture
    /// list's largest levels are left out ([`crate::picture::dropped_levels`]).
    pub detail: u32,
    /// The trilinear filtering preference: whether a texture blends between its two nearest
    /// levels, or takes the nearest level alone.
    pub trilinear: bool,
    /// The anisotropic filtering preference: whether textures seen at a slant are filtered along
    /// the slant, as far as the device goes.
    pub anisotropic: bool,
}

impl Default for Textures {
    /// The highest detail, filtered both ways.
    fn default() -> Self {
        Self {
            detail: 4,
            trilinear: true,
            anisotropic: true,
        }
    }
}

impl Frame {
    /// What the frame shows at the moment, in a picture `aspect` times as wide as it is high: none
    /// when it draws everything, on the land with no cell to look into.
    // D2-03646
    #[must_use]
    pub fn sight(&self, aspect: f32) -> Option<crate::cull::Sight> {
        if let Some(rooms) = &self.rooms {
            let mut sight = rooms.sight(&self.camera, aspect);
            sight.look_out(&self.look_in);
            return Some(sight);
        }
        (!self.look_in.is_empty()).then(|| {
            let view = crate::cull::Frustum::of_camera(&self.camera, aspect);
            crate::cull::Sight::on_land(view, &self.look_in)
        })
    }

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
            .chain(self.shore.iter_mut())
            .chain(self.foam.iter_mut())
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
/// over every water surface, rippled by a scrolling bump picture when the reflections are at
/// their highest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reflection {
    /// The plane's height, in meters.
    pub height: f32,
    /// Whether the ripples push the reflection about; without them it is laid as it stands.
    pub ripples: bool,
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

/// How wide a detailed block's water polygons are, as a share of the block: four by four of them,
/// each with the water's picture once across it.
pub const NEAR_WATER_SHARE: f32 = 0.25;

/// How many land cells around the viewer's own carry their frills until the frill distance
/// preference says ([`SceneBuilder::set_frills`]): its medium setting's.
pub const FRILL_CELLS: i32 = 1;

/// Frills fade out over the last eight meters before the edge of those cells' reach.
const FRILL_FADE: f32 = 8.0;

/// A mesh as the builder keeps it: none when it does not decode.
type MeshEntry = Option<(Arc<Mesh>, Arc<Vec<MeshVertex>>)>;

/// A material as a placed object's look changes it: the material, the template that replaces its
/// own, the modifiers added after its own, and the material detail it is drawn at.
type LookedKey = (u32, Option<u32>, Vec<u32>, u32);

/// An interior cell's vertices lit for a program's point lights, with the day's light (in 8-bit
/// steps) they were lit with.
pub(crate) type LitCell = ([u32; 7], Arc<Vec<MeshVertex>>);

/// Builds frames from one world, keeping what it decoded.
pub struct SceneBuilder<'a> {
    records: &'a WorldRecords,
    region: &'a RegionSet,
    meshes: HashMap<u32, MeshEntry>,
    /// Materials' layers, by material and material detail.
    materials: HashMap<(u32, u32), Vec<DrawLayer>>,
    /// Materials as a placed object's look changes them.
    looked: HashMap<LookedKey, Vec<DrawLayer>>,
    material_kinds: HashMap<u32, u32>,
    grounds: HashMap<u32, Option<u32>>,
    scenery: HashMap<(i32, i32), Arc<Vec<SceneryObject>>>,
    frill_vertices: HashMap<u32, Arc<Vec<MeshVertex>>>,
    shadow_layers: HashMap<u32, Option<DrawLayer>>,
    /// The model detail preference, 0 to 2.
    pub model_detail: u32,
    /// Whether a frame is drawn from a camera moving on until the next is built: an object that
    /// degrades with distance then has a draw for each level it may come to meanwhile.
    pub camera_moves: bool,
    /// The material detail preference, 0 (low) to 2 (high): which of each material's layers are
    /// drawn.
    pub material_detail: u32,
    /// The scenery draw distance preference, 0 (low) to 2 (high): how far placed objects reach
    /// ([`crate::mesh::scenery_scale`]).
    pub scenery_draw_distance: u32,
    /// How the frames' textures are taken: their detail and filtering, as the graphics
    /// preferences say.
    pub textures: Textures,
    /// How long the client has been running, in seconds: scrolling pictures (the clouds'
    /// shadows) follow it, so frames match another client's only at the same running time.
    pub uptime: f64,
    /// The static object shadows preference: the sun's shadow map of the near objects.
    pub static_shadows: bool,
    /// The cloud shadows preference. Clouds are drawn only with static shadows on, as the client
    /// builds them into the same terrain pass.
    pub cloud_shadows: bool,
    /// The water reflections preference: whether the water reflects at all, and whether the
    /// reflection ripples (only at its highest setting).
    pub water_reflections: bool,
    pub water_ripples: bool,
    /// The camera stands under water: the water reflects nothing, and the sky shows only what
    /// shows from there.
    pub camera_in_water: bool,
    /// The landscape shore effects preference: 0 none, 1 the shores' alpha, 2 that and their foam.
    pub shore_effects: u32,
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
    /// The draws inside were lit with lights objects carry in the last frame that had any.
    pub(crate) carried_lit: bool,
    /// The placed objects that cast a volume of shadow numbered so far: the next one's number.
    placed_casters: u32,
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
    /// The detailed landscape's shores as last found: about which block and from which of the
    /// blocks about it, and the shores (none where no water meets the ground).
    shores: Option<(ShoresAt, Option<ShoreDraws>)>,
}

/// Where the shores were found: the viewer's block, and which of the nine about it were there.
type ShoresAt = ((i32, i32), u16);

/// The shores ready to draw: their strips, and their triangles all, and those that carry foam.
#[derive(Clone, Debug)]
struct ShoreDraws {
    shore: Arc<crate::shore::Shore>,
    all: Arc<Vec<u16>>,
    foamed: Arc<Vec<u16>>,
}

/// The shore's alpha material and the foam's, by their enumeration values (in the materials'
/// mapping).
const SHORE_ALPHA_MATERIAL: u32 = 0x4000_0024;
const SHORE_FOAM_MATERIAL: u32 = 0x4000_0017;
const MATERIAL_MAPPING: u32 = 0x4000_0002;

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

/// How many vertices apart a far block's squares are laid, `ring` blocks past the detailed ones: 1
/// in the first ring, then 2, 2, 4, 4, 8, 8, 16, no more than the block's `squares`.
// D2-10962, D2-10984
#[must_use]
pub fn far_stride(ring: i32, squares: usize) -> usize {
    if ring <= 1 {
        return 1;
    }
    let steps = u32::try_from((ring - 2) / 2 + 1).unwrap_or(0).min(30);
    (1usize << steps).min(squares.max(1))
}

/// The two triangles of a square, as corner slots, for its split.
fn square_triangles(cut: bool) -> [u32; 6] {
    if cut {
        [2, 0, 3, 0, 1, 3]
    } else {
        [0, 1, 2, 1, 3, 2]
    }
}

/// A detailed block's ground as triangles, moved by `offset` (meters east and north): square by
/// square, x major, each square's two by its split, each triangle's corners in the order the
/// ground is drawn.
fn ground_triangles(tb: &TerrainBlock, offset: (f32, f32)) -> Vec<[[f32; 3]; 3]> {
    let squares = tb.geometry.squares_per_side;
    let mut out = Vec::with_capacity(2 * squares * squares);
    for i in 0..squares {
        for j in 0..squares {
            let order = square_triangles(tb.geometry.sw_to_ne_cut[i * squares + j]);
            let corner = |s: u32| {
                let (cx, cy) = CORNERS[s as usize];
                let p = tb.geometry.vertices[tb.vertex(i + cx, j + cy)];
                [p.x + offset.0, p.y + offset.1, p.z]
            };
            for t in order.as_chunks::<3>().0 {
                out.push([corner(t[0]), corner(t[1]), corner(t[2])]);
            }
        }
    }
    out
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
            camera_moves: false,
            material_detail: 2,
            scenery_draw_distance: 2,
            textures: Textures::default(),
            uptime: 0.0,
            static_shadows: true,
            cloud_shadows: true,
            water_reflections: true,
            camera_in_water: false,
            water_ripples: true,
            shore_effects: 2,
            light_coronas: true,
            far_radius: FAR_RADIUS,
            frill_radius: FRILL_CELLS,
            frills: true,
            frill_reach: 1.0,
            block_lights: HashMap::new(),
            ground: HashMap::new(),
            lit_cells: HashMap::new(),
            kept_lights: HashMap::new(),
            carried_lit: false,
            placed_casters: 0,
            effect_looks: HashMap::new(),
            cell_looks: HashMap::new(),
            sky_parts: HashMap::new(),
            cell_bakes: HashMap::new(),
            dice: auberean_primitives::random::Random::new(),
            cell_doorways: HashMap::new(),
            fog_cuts: HashMap::new(),
            shores: None,
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

    /// The mesh record `id` names (a cell's mesh for a cell mesh's id) and its vertices: read once
    /// and shared by every part that shows it.
    // D2-05591, D2-05583, D2-05597, D2-05598, D2-05604, D2-05567
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

    /// A material's layers at the material detail set.
    fn material(&mut self, id: u32) -> Vec<DrawLayer> {
        self.material_at(id, self.material_detail)
    }

    /// A material's layers at material detail `detail`.
    fn material_at(&mut self, id: u32, detail: u32) -> Vec<DrawLayer> {
        let records = self.records;
        self.materials
            .entry((id, detail))
            .or_insert_with(|| {
                instance_material(records, id)
                    .map(|m| draw_layers(&m, device_flags(detail)))
                    .unwrap_or_default()
            })
            .clone()
    }

    /// A fragment's material as a part's look leaves it: the mesh's own when the look changes
    /// nothing (or no longer changes anything), else a copy made once for those changes.
    // D2-06635, D2-05609, D2-05607, D2-05593, D2-05605, D2-05606
    fn looked_material(
        &mut self,
        look: &PartLook,
        mesh_kind: u32,
        id: u32,
        detail: u32,
    ) -> Vec<DrawLayer> {
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
            return self.material_at(id, detail);
        }
        let key = (
            id,
            template.map(|t| t.0),
            extra.iter().map(|d| d.0).collect(),
            detail,
        );
        self.looked
            .entry(key)
            .or_insert_with_key(|(id, template, extra, detail)| {
                instance_material_with(records, *id, *template, extra)
                    .map(|m| draw_layers(&m, device_flags(*detail)))
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

    /// What each draw of an object the world places from entity `e` (its parts `parts`, its
    /// block's frame `place`) carries when the object casts a volume of shadow, as its drawing
    /// properties say: a number of its own among the frame's placed objects, where it stands,
    /// the way its shadow falls (`toward`; `None` for the sun's), its first part's detail
    /// distances. `None` for an object that casts none.
    // D2-06038, D2-06187
    pub(crate) fn placed_caster(
        &mut self,
        e: &auberean_assets::entity::Entity,
        parts: &[Part],
        place: &Mat4,
        toward: Option<Vec3>,
    ) -> Option<PlacedCaster> {
        use auberean_physics::world::{entity_graphics_state, graphics};
        use auberean_world::place::PlaceRecords;
        let first = parts.first()?;
        let records = self.records;
        let lookup = |id: u32| records.entity(DataId(id));
        if entity_graphics_state(e, &lookup) & graphics::VOLUME_SHADOW == 0 {
            return None;
        }
        let at = e
            .offset
            .as_ref()
            .map_or(first.transform.point(Vec3::ZERO), |o| {
                let p = o.frame.origin;
                Vec3::new(p.x, p.y, p.z)
            });
        let model_detail = usize::try_from(self.model_detail).unwrap_or(2).min(2);
        self.placed_casters = self.placed_casters.wrapping_add(1);
        Some(PlacedCaster {
            object: self.placed_casters,
            at: place.point(at),
            toward,
            detail: first.detail.map(|d| d[model_detail]),
        })
    }

    /// Whether an object the world places from entity `e` is drawn with those seen through, as
    /// its drawing properties say.
    // D2-06038
    pub(crate) fn placed_seen_through(&self, e: &auberean_assets::entity::Entity) -> bool {
        use auberean_physics::world::{entity_graphics_state, graphics};
        use auberean_world::place::PlaceRecords;
        let records = self.records;
        let lookup = |id: u32| records.entity(DataId(id));
        entity_graphics_state(e, &lookup) & graphics::TRANSLUCENT != 0
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
                posed: None,
                indices: frag.indices.clone(),
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
                caster: None,
                seen_through: false,
                kept_by: None,
                anchor: None,
                step: None,
                visible: None,
                block: None,
                far: false,
                ground: None,
                lod: None,
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
    /// it is lit as the ground under it is. The part draws through its own frame carried by the
    /// object's the mesh its instance holds: the one every part showing it shares, or, when the
    /// part's look changes its materials, the part's own copy with them changed.
    // D2-06685, D2-05602, D2-05587, D2-05596, D2-05585
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
        // A placed object degrades with distance (its distances drawn in by the scenery draw
        // distance, its materials lowered); a frill keeps the material detail as set.
        let model_detail = usize::try_from(self.model_detail).unwrap_or(2).min(2);
        let scale = if frill {
            1.0
        } else {
            crate::mesh::scenery_scale(self.scenery_draw_distance)
        };
        let marked = marked_levels(&mesh, self.model_detail);
        let Some(last) = marked.len().checked_sub(1) else {
            return Vec::new();
        };
        // One choice unless the object degrades with distance: then each it may come to before the
        // frame is built again, in the frame and in the water's reflection.
        let mut choices: Vec<(usize, u32)> = Vec::new();
        let mut visible = None;
        let mut lod = None;
        match &part.detail {
            Some(d) if frill => {
                match crate::mesh::degrade(&d[model_detail], distance, 1.0, self.material_detail) {
                    Some(g) => choices.push((
                        crate::mesh::level_place(g.detail, last),
                        self.material_detail,
                    )),
                    None => return Vec::new(),
                }
            }
            Some(d) => {
                let d = &d[model_detail];
                let reach = if self.camera_moves { CELL_REACH } else { 0.0 };
                let mut high: Option<(usize, u32)> = None;
                let mut low: Option<(usize, u32)> = None;
                for share in [1.0, REFLECTED_DISTANCE] {
                    let near_end = (distance - reach).max(0.0);
                    // just short of where it is gone
                    let far_end =
                        (distance + reach).min((scale * d.visible).max(20.0) * share * 0.999);
                    if near_end > far_end {
                        continue;
                    }
                    let ends = [near_end, far_end].map(|at| {
                        crate::mesh::degrade_in(d, at, scale, share, self.material_detail)
                    });
                    for g in ends.into_iter().flatten() {
                        let at = crate::mesh::level_place(g.detail, last);
                        high = Some(high.map_or((at, g.material_detail), |(p, m)| {
                            (p.min(at), m.max(g.material_detail))
                        }));
                        low = Some(low.map_or((at, g.material_detail), |(p, m)| {
                            (p.max(at), m.min(g.material_detail))
                        }));
                    }
                }
                let (Some((first, top)), Some((end, bottom))) = (high, low) else {
                    return Vec::new();
                };
                for at in first..=end {
                    for materials in bottom..=top {
                        choices.push((at, materials));
                    }
                }
                visible = Some((scale * d.visible).max(20.0));
                lod = Some(Lod {
                    distances: *d,
                    scale,
                    material_detail: self.material_detail,
                    last,
                    level: 0,
                    materials: 0,
                    shown: false,
                    reflected: false,
                });
            }
            None => choices.push((crate::mesh::level_place(1.0, last), self.material_detail)),
        }
        counts.parts += 1;
        let transform = part.transform.then(place);
        // Frills are never culled; anything else standing outdoors by its box: its mesh's box
        // carried through the part's frame, the box about the eight corners.
        let cull = if frill {
            Cull::Never
        } else {
            // D2-05535
            let b = mesh.bounding_box();
            let (lo, hi) = placed_box(
                Vec3::new(b.min.x, b.min.y, b.min.z),
                Vec3::new(b.max.x, b.max.y, b.max.z),
                &transform,
            );
            Cull::Box { lo, hi }
        };
        let mut out = Vec::new();
        for (at, materials) in choices {
            let lod = lod.map(|l: Lod| {
                let mut l = Lod {
                    level: at,
                    materials,
                    ..l
                };
                l.update(distance);
                l
            });
            for frag in &mesh.levels[marked[at]].fragments {
                let Some(mat) = usize::try_from(frag.material)
                    .ok()
                    .and_then(|k| mesh.materials.get(k))
                else {
                    continue;
                };
                let layers = match &part.look {
                    Some(look) => self.looked_material(look, mesh.kind, mat.0, materials),
                    None => self.material_at(mat.0, materials),
                };
                let indices = frag.indices.clone();
                for layer in layers {
                    out.push(ObjectBatch {
                        vertices: verts.clone(),
                        posed: None,
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
                        caster: None,
                        seen_through: false,
                        kept_by: None,
                        anchor: None,
                        step: None,
                        visible,
                        block: None,
                        far: distance > FAR_FADE,
                        ground: None,
                        lod,
                    });
                }
            }
        }
        out
    }

    /// The rain's streaks as the last update left them, one draw for each layer of its material,
    /// standing right before the particle systems.
    // D2-10900
    pub fn rain_batches(&mut self, rain: &crate::rain::Rain) -> Vec<ObjectBatch> {
        let (vertices, indices) = rain.drawn();
        if indices.is_empty() {
            return Vec::new();
        }
        let id = self
            .records
            .data_id_by_enum(crate::rain::MATERIAL, crate::rain::MAPPING);
        let detail = self.material_detail;
        self.material_at(id, detail)
            .into_iter()
            .map(|layer| ObjectBatch {
                vertices: vertices.clone(),
                posed: None,
                indices: indices.clone(),
                transform: Mat4::IDENTITY,
                layer,
                tint: Color::WHITE,
                points: PointLights::NONE,
                mesh_type: 0,
                shade: None,
                outside: true,
                cull: Cull::Never,
                mesh: 0,
                casts_volume: false,
                caster: None,
                seen_through: false,
                kept_by: None,
                anchor: None,
                step: Some(Pass::Particles),
                visible: None,
                block: None,
                far: false,
                ground: None,
                lod: None,
            })
            .collect()
    }

    /// How much of the sun the land lets through at `at` (a place in the frame of block
    /// `frame_block`), as the last frame outdoors built the detailed blocks; whole off them.
    // D2-10925
    #[must_use]
    pub fn land_shadow_at(&self, at: Vec3, frame_block: (i32, i32), light_direction: Vec3) -> f32 {
        let len = self.region.land.grid().block_length();
        let (dx, dy) = (floor_to_i32(at.x / len), floor_to_i32(at.y / len));
        let block = (frame_block.0 + dx, frame_block.1 + dy);
        self.ground.get(&block).map_or(1.0, |tb| {
            land_shadow(
                tb,
                sub_block(at, block, frame_block, len),
                self.square_side(),
                light_direction,
            )
        })
    }

    /// The two triangles of the landscape square holding `at` (a point in the frame of block
    /// `frame_block`), each corner its place in that frame and its normal, as the last frame
    /// outdoors built the detailed blocks; none for a square it did not build or a hole.
    // D2-10926, D2-10964, D2-10953
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
        let far = NEAR_RADIUS + self.far_radius;
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
        let mut near_land: HashMap<(u32, Option<u32>, u8, bool), TerrainBatch> = HashMap::new();
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
                if self.water_reflections {
                    self.far_terrain(&tb, offset, &light, spot.block, &mut near_land);
                }
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
                        ..w
                    };
                    counts.water_blocks += 1;
                    // Its one set of coordinates runs a quarter of a block over the tile width
                    // from corner to corner.
                    let tile = if w.tile_width > 0.0 {
                        block_len * w.tile_width / (block_len * NEAR_WATER_SHARE)
                    } else {
                        block_len
                    };
                    water.extend(self.water_batches(
                        &w,
                        &Mat4::translation(offset),
                        block_len,
                        tile,
                    ));
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
        let mut look_in = Vec::new();
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
                let quarter = block_len * NEAR_WATER_SHARE;
                water.extend(self.water_batches(&w, &place, quarter, quarter));
            }
            let mut parts: Vec<Part> = Vec::new();
            // Where each part's object stands in the block's frame: each step draws the frame's
            // objects farthest first, an object's parts together.
            let mut anchors: Vec<Vec3> = Vec::new();
            let mut frill_parts: Vec<Part> = Vec::new();
            if let Some(tb) = near_blocks.get(&(bx, by)) {
                for s in self.block_scenery(tb, &blocks, info.as_ref()).iter() {
                    let cell = (
                        bx * cells + floor_to_i32(s.origin.x / cell_len),
                        by * cells + floor_to_i32(s.origin.y / cell_len),
                    );
                    // Frills only in the cells about the viewer's, and none when turned off.
                    // D2-10843, D2-10858, D2-03855
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
                        anchors.extend(std::iter::repeat_n(s.origin, p.len()));
                        parts.extend(p);
                    }
                }
            }
            for part in &frill_parts {
                let at = part.transform.then(&place).point(Vec3::ZERO);
                let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                let first = objects.len();
                objects.extend(self.batches_of(part, &place, d, Color::WHITE, true, &mut counts));
                // A frill takes the lights objects carry that reach it too, each frame; the frills
                // are drawn after the water, before the objects seen through.
                // D2-10837, D2-11042
                let kept_by = self.outdoor_kept_by((bx, by), part);
                for b in &mut objects[first..] {
                    b.shade = Some(at);
                    b.kept_by = Some(kept_by);
                    b.step = Some(Pass::Frills);
                }
            }
            let Some(info) = info else {
                casters.extend(parts.iter().map(|p| (p.clone(), place)));
                for (part, anchor) in parts.iter().zip(&anchors) {
                    let at = part.transform.then(&place).point(Vec3::ZERO);
                    let d =
                        auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                    let first = objects.len();
                    objects.extend(self.part_batches(part, &place, d, Color::WHITE, &mut counts));
                    let kept_by = self.outdoor_kept_by((bx, by), part);
                    for b in &mut objects[first..] {
                        b.kept_by = Some(kept_by);
                        b.anchor = Some(place.point(*anchor));
                        b.block = Some((bx, by));
                    }
                }
                continue;
            };
            // The placed objects that cast a volume of shadow, and those seen through, by the
            // parts they span.
            let mut part_casters: Vec<(std::ops::Range<usize>, PlacedCaster)> = Vec::new();
            let mut seen_through: Vec<std::ops::Range<usize>> = Vec::new();
            for (i, e) in info.entities.entities.iter().enumerate() {
                let p = entity_parts(self.records, e);
                if !p.is_empty() {
                    counts.objects += 1;
                    placed.extend(self.placed_look(e, (bx, by), 0, i));
                }
                if let Some(c) = self.placed_caster(e, &p, &place, None) {
                    part_casters.push((parts.len()..parts.len() + p.len(), c));
                }
                if self.placed_seen_through(e) {
                    seen_through.push(parts.len()..parts.len() + p.len());
                }
                if let Some(first) = p.first() {
                    anchors.extend(std::iter::repeat_n(
                        first.transform.point(Vec3::ZERO),
                        p.len(),
                    ));
                }
                parts.extend(p);
            }
            // The block's objects and its scenery cast shadows; the interiors seen from outside
            // do not.
            casters.extend(parts.iter().map(|p| (p.clone(), place)));
            let outdoors = parts.len();
            // The interior cells seen from outside, each once (the list may name a cell again),
            // lit as cells: each part's cell, and for an object standing in one its key, place
            // and reach.
            let mut in_cells: Vec<(u32, Option<interior::ObjectReach>, Cull)> = Vec::new();
            let mut taken: Vec<u32> = Vec::new();
            let mut order: Vec<u16> = Vec::new();
            for &low in &info.outside_visible {
                #[allow(clippy::cast_sign_loss)]
                let id = ((bx as u32) << 24) | ((by as u32) << 16) | (low & 0xFFFF);
                if inside.contains(&id) || taken.contains(&id) {
                    continue;
                }
                taken.push(id);
                let Some(cell) = u16::try_from(low & 0xFFFF)
                    .ok()
                    .and_then(|l| self.records.cell(bx, by, l))
                else {
                    continue;
                };
                order.push(cell_low(low));
                counts.cells += 1;
                let frame = Mat4::from_parts(
                    cell.frame.origin,
                    cell.frame.rotation,
                    Vec3::new(1.0, 1.0, 1.0),
                );
                let look = self.cell_look(id, &cell);
                anchors.push(cell.frame.origin);
                parts.push(Part {
                    mesh: cell.mesh,
                    transform: frame,
                    detail: None,
                    look,
                });
                let looked = Cull::Looked {
                    block: (bx, by),
                    cell: cell_low(low),
                };
                in_cells.push((id, None, looked));
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
                    if let Some(c) = self.placed_caster(e, &p, &place, None) {
                        part_casters.push((parts.len()..parts.len() + p.len(), c));
                    }
                    if self.placed_seen_through(e) {
                        seen_through.push(parts.len()..parts.len() + p.len());
                    }
                    let reach = p.first().map(|first| {
                        let at = e
                            .offset
                            .as_ref()
                            .map_or(first.transform.point(Vec3::ZERO), |o| {
                                let q = o.frame.origin;
                                Vec3::new(q.x, q.y, q.z)
                            });
                        let key = (id, u32::try_from(i + 1).unwrap_or(u32::MAX));
                        (key, at, self.object_reach(&p))
                    });
                    let cull = match self.object_cull(cell_low(low), &p, &place) {
                        Cull::InCell { cell, lo, hi } => Cull::LookedIn {
                            block: (bx, by),
                            cell,
                            lo,
                            hi,
                        },
                        _ => looked,
                    };
                    in_cells.extend(p.iter().map(|_| (id, reach, cull)));
                    if let Some(first) = p.first() {
                        anchors.extend(std::iter::repeat_n(
                            first.transform.point(Vec3::ZERO),
                            p.len(),
                        ));
                    }
                    parts.extend(p);
                }
            }
            let outside = auberean_world::inside::Outside::of(&light);
            let mut cell_lights = self.block_lights((bx, by)).map(|l| {
                let scene = l.scene();
                let sky = scene.sky_alone(&l);
                (scene, sky, l)
            });
            for (k, part) in parts.iter().enumerate() {
                let at = part.transform.then(&place).point(Vec3::ZERO);
                let d = auberean_world::math::length(auberean_world::math::sub(at, spot.position));
                let first = objects.len();
                objects.extend(self.part_batches(part, &place, d, Color::WHITE, &mut counts));
                let anchor = anchors.get(k).map(|a| place.point(*a));
                for b in &mut objects[first..] {
                    b.anchor = anchor;
                }
                if let Some((_, c)) = part_casters.iter().find(|(r, _)| r.contains(&k)) {
                    for b in &mut objects[first..] {
                        b.casts_volume = true;
                        b.caster = Some(*c);
                    }
                }
                if seen_through.iter().any(|r| r.contains(&k)) {
                    for b in &mut objects[first..] {
                        b.seen_through = true;
                    }
                }
                // What stands outdoors takes the lights objects carry that reach it, each frame;
                // what stands in a cell seen from outside is lit as in a cell.
                if k < outdoors {
                    let kept_by = self.outdoor_kept_by((bx, by), part);
                    for b in &mut objects[first..] {
                        b.kept_by = Some(kept_by);
                        b.block = Some((bx, by));
                    }
                } else if let Some(&(id, reach, cull)) = in_cells.get(k - outdoors) {
                    self.light_seen_from_outside(
                        &mut objects[first..],
                        part,
                        (bx, by),
                        id,
                        reach,
                        cell_lights.as_mut(),
                        &outside,
                    );
                    for b in &mut objects[first..] {
                        b.cull = cull;
                    }
                }
            }
            if !order.is_empty() {
                let rooms = self.block_rooms(spot, (bx, by), &order);
                look_in.push(LookIn {
                    block: (bx, by),
                    rooms,
                    order,
                });
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
            // D2-10999
            ShadowMap::new(light.light_direction, ground, light.shadow_color, batches)
        } else {
            None
        };
        let reflection = if self.water_reflections && !self.camera_in_water {
            self.reflection_plane(&near_blocks, spot)
        } else {
            None
        };
        // What stands on the land and is not seen through takes the land's shadow there.
        // D2-06220
        for b in &mut objects {
            let Some(block) = b.block else { continue };
            if b.seen_through {
                continue;
            }
            let Some(tb) = near_blocks.get(&block) else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let at = sub_block(
                b.anchor.unwrap_or_else(|| b.transform.point(Vec3::ZERO)),
                block,
                spot.block,
                block_len,
            );
            let s = land_shadow(tb, at, self.square_side(), light.light_direction);
            b.tint = Color::new(s, s, s, b.tint.a);
        }
        let (shore, foam) = self.shore_batches(spot.block, &near_blocks);
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

        // D2-10950
        let reflection_land = near_land.into_values().next().map(|mut land| {
            if let Some(far) = terrain.values().find(|b| b.far) {
                let base = u32::try_from(land.vertices.len()).unwrap_or(u32::MAX);
                Arc::make_mut(&mut land.vertices).extend_from_slice(&far.vertices);
                Arc::make_mut(&mut land.indices).extend(far.indices.iter().map(|i| i + base));
            }
            land
        });
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
            reflection_land,
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
            shore,
            foam,
            placed,
            flares,
            outside: None,
            rooms: None,
            look_in: Arc::new(look_in),
            looked_light: None,
            volumes: crate::volume::Volumes::default(),
            decals: Vec::new(),
            textures: self.textures,
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
    // D2-10903, D2-10904, D2-10865, D2-10866, D2-10880
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
            if !piece.shows(self.camera_in_water) {
                continue;
            }
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
        lights: Option<(&BlockLights, bool)>,
    ) -> Frame {
        let mut counts = Counts::default();
        let mut objects = Vec::new();
        let outside = Outside::of(&light);
        let mut order = lights.map(|(l, _)| l.scene());
        for part in parts {
            let mut batches =
                self.part_batches(part, &Mat4::IDENTITY, 0.0, Color::WHITE, &mut counts);
            if let (Some((lights, sun)), Some(order)) = (lights, order.as_mut()) {
                let at = part.transform.point(Vec3::ZERO);
                interior::light_view_draw(&mut batches, at, lights, order, &outside, sun);
            }
            objects.extend(batches);
        }
        counts.objects = parts.len();
        objects.extend(bodies);
        Frame {
            camera,
            light,
            sky_before: Vec::new(),
            terrain: Vec::new(),
            reflection_land: None,
            objects,
            sky_after: Vec::new(),
            frill_fade: [f32::MAX, f32::MAX],
            counts,
            shadow: None,
            clouds: None,
            uptime: self.uptime,
            water: Vec::new(),
            reflection: None,
            shore: Vec::new(),
            foam: Vec::new(),
            placed: Vec::new(),
            flares: Vec::new(),
            outside: None,
            rooms: None,
            look_in: Arc::default(),
            looked_light: None,
            volumes: crate::volume::Volumes::default(),
            decals: Vec::new(),
            textures: self.textures,
        }
    }

    /// The plane the water reflects about: the viewer's own block's water when it lies below the
    /// camera, else the first detailed block's that does; none with the camera under water.
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
        order.into_iter().find_map(below).map(|w| Reflection {
            height: w.height,
            ripples: self.water_ripples,
        })
    }

    /// The detailed landscape's shores about the viewer's block `at` (none with the shore effects
    /// off): found again only when the viewer has come to another block or another of the blocks
    /// about it has come in, each strip's offset across the foam rolled anew then. The strips with
    /// the shore's alpha material, black at half alpha; with the shore effects at their highest,
    /// those that carry foam with the foam's material, black.
    // D2-10965, D2-10956
    fn shore_batches(
        &mut self,
        at: (i32, i32),
        near_blocks: &HashMap<(i32, i32), TerrainBlock>,
    ) -> (Vec<ObjectBatch>, Vec<ObjectBatch>) {
        if self.shore_effects == 0 {
            return (Vec::new(), Vec::new());
        }
        let mut present = 0u16;
        let mut order = Vec::new();
        for (k, (dx, dy)) in (-NEAR_RADIUS..=NEAR_RADIUS)
            .flat_map(|dx| (-NEAR_RADIUS..=NEAR_RADIUS).map(move |dy| (dx, dy)))
            .enumerate()
        {
            if let Some(tb) = near_blocks.get(&(at.0 + dx, at.1 + dy)) {
                present |= 1 << k;
                order.push(((dx, dy), tb));
            }
        }
        let key = (at, present);
        let draws = match &self.shores {
            Some((k, d)) if *k == key => d.clone(),
            _ => {
                let block_len = self.region.land.grid().block_length();
                let blocks: Vec<crate::shore::ShoreBlock> = order
                    .iter()
                    .map(|&((dx, dy), tb)| {
                        #[allow(clippy::cast_precision_loss)] // a block or so away
                        let offset = (dx as f32 * block_len, dy as f32 * block_len);
                        crate::shore::ShoreBlock {
                            water: self.block_water(tb).map(|w| w.height),
                            own: (dx, dy) == (0, 0),
                            triangles: ground_triangles(tb, offset),
                        }
                    })
                    .collect();
                let d = crate::shore::generate(&blocks, &self.dice).map(|shore| ShoreDraws {
                    all: Arc::new(shore.triangles(false)),
                    foamed: Arc::new(shore.triangles(true)),
                    shore: Arc::new(shore),
                });
                self.shores = Some((key, d.clone()));
                d
            }
        };
        let Some(d) = draws else {
            return (Vec::new(), Vec::new());
        };
        let batches = |this: &mut Self, value: u32, indices: &Arc<Vec<u16>>, tint: Color| {
            if indices.is_empty() {
                return Vec::new();
            }
            let id = this.records.data_id_by_enum(value, MATERIAL_MAPPING);
            this.material(id)
                .into_iter()
                .map(|layer| ObjectBatch {
                    vertices: d.shore.vertices.clone(),
                    posed: None,
                    indices: indices.clone(),
                    transform: Mat4::IDENTITY,
                    layer,
                    tint,
                    points: PointLights::NONE,
                    mesh_type: 0,
                    shade: None,
                    outside: false,
                    cull: Cull::Never,
                    mesh: 0,
                    casts_volume: false,
                    caster: None,
                    seen_through: false,
                    kept_by: None,
                    anchor: None,
                    step: None,
                    visible: None,
                    block: None,
                    far: false,
                    ground: None,
                    lod: None,
                })
                .collect::<Vec<_>>()
        };
        let shore = batches(
            self,
            SHORE_ALPHA_MATERIAL,
            &d.all,
            Color::new(0.0, 0.0, 0.0, 0.5),
        );
        let foam = if self.shore_effects >= 2 {
            batches(
                self,
                SHORE_FOAM_MATERIAL,
                &d.foamed,
                Color::new(0.0, 0.0, 0.0, 1.0),
            )
        } else {
            Vec::new()
        };
        (shore, foam)
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

    /// A block's water surface: flat across the whole block at the water's height, in square
    /// polygons `poly` meters wide, its picture and the reflection's ripples on the one set of
    /// coordinates, repeating every `tile` meters, drawn with the material seen from above. A
    /// detailed block's water is four polygons by four, a quarter of the block wide, its picture
    /// once across each (whatever the water's own sizes say); a far block's is one polygon.
    // D2-10986, D2-10977, D2-10975
    fn water_batches(
        &mut self,
        water: &BlockWater,
        place: &Mat4,
        poly: f32,
        tile: f32,
    ) -> Vec<ObjectBatch> {
        let bl = self.region.land.grid().block_length();
        let poly = if poly > 0.0 { poly.min(bl) } else { bl };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((bl / poly).round() as usize).clamp(1, 64);
        #[allow(clippy::cast_precision_loss)]
        let step = bl / n as f32;
        let tile = if tile > 0.0 { tile } else { bl };
        let mut verts = Vec::with_capacity((n + 1) * (n + 1));
        for i in 0..=n {
            for j in 0..=n {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f32 * step, j as f32 * step);
                verts.push(MeshVertex {
                    position: [x, y, water.height],
                    normal: [0.0, 0.0, 1.0],
                    diffuse: 0xFFFF_FFFF,
                    uv: [[x / tile, y / tile]; 4],
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
                posed: None,
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
                caster: None,
                seen_through: false,
                kept_by: None,
                anchor: None,
                step: None,
                visible: None,
                block: None,
                far: false,
                ground: None,
                lod: None,
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
                        // D2-10946, D2-10542
                        let c = auberean_world::sky::lit_vertex(light, tb.normals[k], own, lit);
                        #[allow(clippy::cast_precision_loss)] // vertex indices within a block
                        let (u, v) = ((i + cx) as f32 * t, (j + 1 - cy) as f32 * t);
                        #[allow(clippy::cast_precision_loss)]
                        let (fx, fy) = (cx as f32, cy as f32);
                        Arc::make_mut(&mut batch.vertices).push(TerrainVertex {
                            position: [p.x + offset.x, p.y + offset.y, p.z],
                            color: [c.r, c.g, c.b, 1.0],
                            uv: [u, v],
                            mask_uv: mask.map_or([0.0, 0.0], |(_, rot)| mask_uv(fx, fy, rot)),
                        });
                    }
                    Arc::make_mut(&mut batch.indices)
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
        center: (i32, i32),
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
        let squares_here = n - 1;
        // The block's own squares, every `stride` vertices by how far off it lies.
        let ring = |x: i32, y: i32| (x - center.0).abs().max((y - center.1).abs());
        let stride = far_stride(ring(tb.x, tb.y) - NEAR_RADIUS, squares_here);
        let side = squares_here / stride + 1;
        let base = u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX);
        for i in 0..side {
            for j in 0..side {
                let k = i * stride * n + j * stride;
                let p = tb.geometry.vertices[k];
                let lit = auberean_world::sky::sun_reaches(tb.shadow_bits[k], sun_bit);
                let c = auberean_world::sky::lit_vertex(
                    light,
                    tb.normals[k],
                    tb.terrain[k].far_color,
                    lit,
                );
                let (x, y) = (p.x + offset.x, p.y + offset.y);
                Arc::make_mut(&mut batch.vertices).push(TerrainVertex {
                    position: [x, y, p.z],
                    color: [c.r, c.g, c.b, 1.0],
                    uv: [x * scale, y * scale],
                    mask_uv: [0.0, 0.0],
                });
            }
        }
        let v = |a: usize, b: usize| base + u32::try_from(a * side + b).unwrap_or(0);
        for i in 0..side - 1 {
            for j in 0..side - 1 {
                let (sw, se, nw, ne) = (v(i, j), v(i + 1, j), v(i, j + 1), v(i + 1, j + 1));
                Arc::make_mut(&mut batch.indices).extend([nw, sw, se, nw, se, ne]);
            }
        }
        // Where the block next door has half the vertices along their shared edge, a triangle over
        // each pair of its squares fills what its coarser edge leaves open, going round this block.
        // Its index runs along x, then y; a neighbor's stride comes of its own ring.
        let coarser = |x: i32, y: i32| {
            far_stride(ring(x, y) - NEAR_RADIUS, squares_here) == stride * 2
                && stride < squares_here
        };
        let last = side - 1;
        let (bx, by) = (tb.x, tb.y);
        let indices = Arc::make_mut(&mut batch.indices);
        for k in (0..last.saturating_sub(1)).step_by(2) {
            if coarser(bx + 1, by) {
                indices.extend([v(last, k), v(last, k + 2), v(last, k + 1)]);
            }
            if coarser(bx, by - 1) {
                indices.extend([v(k, 0), v(k + 2, 0), v(k + 1, 0)]);
            }
            if coarser(bx, by + 1) {
                indices.extend([v(k + 2, last), v(k, last), v(k + 1, last)]);
            }
            if coarser(bx - 1, by) {
                indices.extend([v(0, k + 2), v(0, k), v(0, k + 1)]);
            }
        }
    }
}
