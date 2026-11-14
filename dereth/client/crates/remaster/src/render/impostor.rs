//! Far trees as impostors: each tree's full mesh drawn once into a row of an atlas page, a
//! picture from each of [`VIEWS`] directions round it, level with it; then, wherever the levels
//! of detail stand a tree's impostor in for its meshes ([`super::lod`]), an upright card turned
//! to the eye, drawn into the G-buffer after the meshes, showing the pictures either side of the
//! eye's direction and lit as the sun lights what stands outdoors.
//!
//! The pictures are drawn [`FINE`] times finer each way than the row keeps them ([`Fine`]), and
//! each texel of the row takes the share of its square of pixels the mesh covers, with its color
//! multiplied by that share: a crown's needles, each thinner than a texel, keep their coverage
//! rather than thinning to a few blots with holes between.
//!
//! A tree's pictures are made the frame it is first drawn so, or, for a tree whose levels the
//! device chooses ([`super::instances`]), the frame its block comes, and kept while the renderer
//! is. The device writes its own trees' cards, which are drawn after the processor's.

use std::collections::HashMap;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Compare, Cull, DepthAttachment,
    DepthState, Device, Draw, Format, GraphicsDesc, IndexType, Load, Location, Pipeline,
    RasterPass, Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4Swizzles};

use super::scene::GpuScene;
use super::{push_words, shader, Targets, ALBEDO, DEPTH, LIGHT, MATERIAL, NORMAL};
use crate::scene::{BlockId, FrameDelta};
use crate::shaders::Entry;

/// The pictures of a tree, one each from as many directions round it.
pub const VIEWS: u32 = 16;

/// A picture's side, pixels.
pub const TILE: u32 = 256;

/// The trees an atlas page holds, a row each.
pub const ROWS: u32 = 8;

/// The pages' format: a picture's color and coverage, and its normals.
pub(crate) const PICTURE: Format = Format::Rgba8Unorm;

/// How many times finer each way a tree's pictures are drawn than they are kept: each texel kept
/// is gathered from a square of this many pixels a side.
pub const FINE: u32 = 2;

/// The views drawn finer at once.
const SPAN: u32 = 2;

/// Where a row's views are drawn finer before they are gathered into it, [`SPAN`] at a time:
/// their colors and coverage and their normals, each with its index in the table, and their
/// depth; and the pipeline that gathers them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fine {
    color: (Texture, u32),
    normal: (Texture, u32),
    depth: Texture,
    gather: Pipeline,
}

impl Fine {
    /// The bytes it holds on the device.
    pub(crate) const BYTES: u64 = (SPAN * TILE * FINE) as u64 * (TILE * FINE) as u64 * 4 * 3;

    /// The pipeline that gathers a row's texels from its views drawn finer.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn gather_pipeline(gfx: &mut Device) -> Result<Pipeline, dereth_gfx::Error> {
        let targets = [PICTURE, PICTURE].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster tree pictures gathered",
            vertex: shader(Entry::FullscreenVsMain),
            fragment: Some(shader(Entry::ImpostorFsGather)),
            color: &targets,
            depth: None,
            cull: Cull::None,
        })
    }

    /// Its pictures made on `gfx`, gathered by `gather`.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn new(gfx: &mut Device, gather: Pipeline) -> Result<Self, dereth_gfx::Error> {
        let (width, height) = (SPAN * TILE * FINE, TILE * FINE);
        let mut make = |label, format, usage| {
            gfx.create_texture(&TextureDesc {
                label,
                width,
                height,
                mips: 1,
                format,
                usage,
            })
        };
        let both = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED;
        let color = make("remaster tree pictures drawn finer", PICTURE, both)?;
        let normal = make("remaster tree normals drawn finer", PICTURE, both)?;
        let depth = make(
            "remaster tree pictures' depth",
            DEPTH,
            TextureUsage::DEPTH_TARGET,
        )?;
        Ok(Self {
            color: (color, gfx.bind_texture(color)?),
            normal: (normal, gfx.bind_texture(normal)?),
            depth,
            gather,
        })
    }

    /// Record a row of a tree's views into `into` (a page's color and normal pictures), its top
    /// `y` texels down: [`SPAN`] views at a time drawn finer by `draw` (given the pass, with
    /// `bake` and `indices` bound, and the view), each pass over what `touches` names, then
    /// gathered into the row.
    ///
    /// # Errors
    /// What the layer refuses.
    pub(crate) fn record(
        &self,
        r: &mut Recorder<'_>,
        (bake, indices): (Pipeline, Buffer),
        touches: &[Touch],
        into: (Texture, Texture),
        y: u32,
        draw: &mut dyn FnMut(&mut Draw<'_>, u32) -> Result<(), dereth_gfx::Error>,
    ) -> Result<(), dereth_gfx::Error> {
        let side = TILE * FINE;
        for first in (0..VIEWS).step_by(SPAN as usize) {
            r.raster(
                &RasterPass {
                    name: "tree pictures drawn finer",
                    color: &[
                        ColorAttachment {
                            texture: self.color.0,
                            load: Load::Clear([0.0; 4]),
                        },
                        ColorAttachment {
                            texture: self.normal.0,
                            load: Load::Clear([0.5, 0.5, 1.0, 0.0]),
                        },
                    ],
                    depth: Some(DepthAttachment {
                        texture: self.depth,
                        clear: Some(1.0),
                        write: true,
                    }),
                    touches,
                    area: Some(Rect {
                        x: 0,
                        y: 0,
                        width: SPAN * side,
                        height: side,
                    }),
                },
                |d| {
                    d.set_pipeline(bake)?;
                    d.bind_index_buffer(indices, 0, IndexType::U32)?;
                    for k in 0..SPAN {
                        let tile = Rect {
                            x: i32::try_from(k * side).unwrap_or(0),
                            y: 0,
                            width: side,
                            height: side,
                        };
                        d.set_viewport(tile);
                        d.set_scissor(tile);
                        draw(d, first + k)?;
                    }
                    Ok(())
                },
            )?;
            let (x, width) = (first * TILE, SPAN * TILE);
            r.raster(
                &RasterPass {
                    name: "tree pictures gathered",
                    color: &[
                        ColorAttachment {
                            texture: into.0,
                            load: Load::Keep,
                        },
                        ColorAttachment {
                            texture: into.1,
                            load: Load::Keep,
                        },
                    ],
                    depth: None,
                    touches: &[
                        Touch::Texture(self.color.0, Use::Sampled),
                        Touch::Texture(self.normal.0, Use::Sampled),
                    ],
                    area: Some(Rect {
                        x: i32::try_from(x).unwrap_or(0),
                        y: i32::try_from(y).unwrap_or(0),
                        width,
                        height: TILE,
                    }),
                },
                |d| {
                    d.set_pipeline(self.gather)?;
                    d.push(0, &push_words(&[self.color.1, self.normal.1, FINE, x, y]))?;
                    d.draw(3, 1, 0, 0);
                    Ok(())
                },
            )?;
        }
        Ok(())
    }
}

/// Where a tree's impostor pictures are in the atlas: its page's color and normal pictures, each
/// with its index in the table, its row, and the sphere round the mesh they are of, in its own
/// space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasRow {
    pub albedo: (Texture, u32),
    pub normal: (Texture, u32),
    pub row: u32,
    pub centre: Vec3,
    pub radius: f32,
}

/// The push words that draw view `view` of a mesh's pictures: its sphere (`centre`, `radius`),
/// the mesh placed by `frame` in the space pictured; drawn by the draw items it names (`items`'s
/// index) or, where `items` is `NONE`, as `own` says: its first vertex, its material and its
/// flags.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn bake_words(
    items: u32,
    vertices: u32,
    materials: u32,
    samplers: [u32; 2],
    view: u32,
    (centre, radius): (Vec3, f32),
    frame: Mat4,
    own: [u32; 3],
) -> Vec<u8> {
    let mut words = push_words(&[items, vertices, materials, samplers[0], samplers[1], view]);
    let rows = frame.transpose();
    for x in [centre.x, centre.y, centre.z, radius]
        .into_iter()
        .chain(rows.x_axis.to_array())
        .chain(rows.y_axis.to_array())
        .chain(rows.z_axis.to_array())
    {
        words.extend_from_slice(&x.to_le_bytes());
    }
    words.extend(push_words(&own));
    words
}

/// One tree a frame draws as an impostor: its block, its place among the block's parts, how
/// much of it the impostor draws (1 all of it), and the tree's first level's GfxObj.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpostorDraw {
    pub block: BlockId,
    pub part: usize,
    pub share: f32,
    pub gfxobj: u32,
}

/// Where a tree's pictures are, and the sphere round the mesh they are of, in its own space; and
/// its place in the table of trees' rows the device reads.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    page: usize,
    row: u32,
    centre: Vec3,
    radius: f32,
    index: u32,
}

/// The cards the device writes for its trees: their rows by buffer and index, and their indirect
/// draw.
#[derive(Debug, Clone, Copy)]
pub struct DeviceCards {
    pub cards: (Buffer, u32),
    pub draw: Buffer,
}

/// The bytes of one tree's row in the table the device reads: its page's pictures, its row, the
/// rows a page holds; the sphere its pictures are of.
const TREE_BYTES: usize = 32;

/// An atlas page: its pictures' colors and normals, each with its index in the table.
#[derive(Debug, Clone, Copy)]
struct Page {
    albedo: Texture,
    albedo_index: u32,
    normal: Texture,
    normal_index: u32,
}

/// A tree whose pictures a frame makes: its row, its sphere, and the draw items of its first
/// level (each one's item, its first index and its index count).
#[derive(Debug, Clone, PartialEq)]
struct Bake {
    page: usize,
    row: u32,
    centre: Vec3,
    radius: f32,
    items: Vec<(u32, u32, u32)>,
}

/// What a frame's impostors record: the trees whose pictures it makes, and the cards.
#[derive(Debug, Clone)]
pub struct ImpostorPlan {
    bakes: Vec<Bake>,
    pages: Vec<Page>,
    fine: Option<Fine>,
    cards: Buffer,
    cards_index: u32,
    count: u32,
    frame: u32,
    items: (Buffer, u32),
    vertices: (Buffer, u32),
    materials: (Buffer, u32),
    indices: Buffer,
    samplers: [u32; 2],
    /// The weather's constants, `NONE` without them, and the overhead map they read.
    weather: (u32, Option<Texture>),
    bake: Pipeline,
    draw: Pipeline,
    /// The cards the device wrote, drawn after these, and the bytes of their draw.
    device: Option<DeviceCards>,
    draw_stride: u32,
}

/// What the pass needs of a frame.
#[derive(Debug)]
pub struct ImpostorInputs<'a> {
    pub frame: &'a FrameDelta,
    pub scene: &'a GpuScene,
    pub draws: &'a [ImpostorDraw],
    /// The frame's constants, and the samplers that repeat and that hold at the edges.
    pub constants: u32,
    pub samplers: [u32; 2],
    /// The weather's constants, which wet the cards out in the rain as the meshes are (`NONE`
    /// without them), and the overhead map they read.
    pub weather: (u32, Option<Texture>),
    /// The cards the device writes for its trees this frame.
    pub device: Option<DeviceCards>,
}

/// The impostors' pass: its pipelines, its atlas pages, and the trees they hold.
#[derive(Debug)]
pub struct ImpostorPass {
    bake: Pipeline,
    draw: Pipeline,
    gather: Pipeline,
    pages: Vec<Page>,
    fine: Option<Fine>,
    // ORDER-OK: looked up by tree.
    slots: HashMap<u32, Slot>,
    cards: Buffer,
    cards_index: u32,
    cards_size: u64,
    stats: ImpostorStats,
    /// Where the last frame's cards stand their trees' bases, and how far each was lifted.
    bases: Vec<(Vec3, f32)>,
    /// The last frame's cards.
    drawn: Vec<DrawnCard>,
    /// The trees given rows for the device since the last frame, whose pictures it makes.
    waiting: Vec<Bake>,
    /// Each tree's row as the device reads it, and the table's buffer, its index and its bytes.
    tree_rows: Vec<u8>,
    trees: (Buffer, u32, u64),
}

/// A card the last frame drew: its tree's first level's GfxObj, the centre of the sphere its
/// pictures are of (the frame's space) and its radius, the turn of the tree's x axis about its z
/// axis (radians), and how much of the tree the card draws (1 all of it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnCard {
    pub gfxobj: u32,
    pub centre: Vec3,
    pub radius: f32,
    pub turn: f32,
    pub share: f32,
}

/// What the impostors did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImpostorStats {
    /// Trees with pictures made, in all; atlas pages made.
    pub trees: u32,
    pub pages: u32,
    /// Cards the last frame drew, and trees whose pictures it made.
    pub cards: u32,
    pub baked: u32,
}

/// The rows a card takes: four float rows and one of words.
const CARD_ROWS: usize = 5;

impl ImpostorPass {
    /// The pass on `gfx`, its pipelines made, holding no tree.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let depth = Some(DepthState {
            format: DEPTH,
            compare: Some(Compare::LessEqual),
            write: true,
        });
        let pictures = [PICTURE, PICTURE].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        let bake = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster impostor pictures",
            vertex: shader(Entry::ImpostorVsBake),
            fragment: Some(shader(Entry::ImpostorFsBake)),
            color: &pictures,
            depth,
            cull: Cull::None,
        })?;
        let targets = [ALBEDO, NORMAL, LIGHT, MATERIAL].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        let draw = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster impostors",
            vertex: shader(Entry::ImpostorVsDraw),
            fragment: Some(shader(Entry::ImpostorFsDraw)),
            color: &targets,
            depth,
            cull: Cull::None,
        })?;
        let gather = Fine::gather_pipeline(gfx)?;
        let cards_size = 1 << 16;
        let cards = gfx.create_buffer(&BufferDesc {
            label: "remaster impostor cards",
            size: cards_size,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        let cards_index = gfx.bind_buffer(cards)?;
        let trees_size = 64 * TREE_BYTES as u64;
        let trees = gfx.create_buffer(&BufferDesc {
            label: "remaster impostor trees",
            size: trees_size,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        let trees_index = gfx.bind_buffer(trees)?;
        Ok(Self {
            bake,
            draw,
            gather,
            pages: Vec::new(),
            fine: None,
            slots: HashMap::new(),
            cards,
            cards_index,
            cards_size,
            stats: ImpostorStats::default(),
            bases: Vec::new(),
            drawn: Vec::new(),
            waiting: Vec::new(),
            tree_rows: Vec::new(),
            trees: (trees, trees_index, trees_size),
        })
    }

    /// The sphere the pictures of the tree whose first level's mesh is `gfxobj` are of, in its own
    /// space, once it has a row.
    #[must_use]
    pub(crate) fn tree_sphere(&self, gfxobj: u32) -> Option<(Vec3, f32)> {
        self.slots.get(&gfxobj).map(|s| (s.centre, s.radius))
    }

    /// The table of trees' rows the device reads, by buffer and index.
    #[must_use]
    pub(crate) fn trees(&self) -> (Buffer, u32) {
        (self.trees.0, self.trees.1)
    }

    /// The tree whose first level's mesh is `gfxobj`, drawn by part `p` of a block: its place in
    /// the table of trees' rows, given it a row (its pictures made with the next frame drawn) when
    /// it has none. `None` for a mesh not held, or with no sphere.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn tree_slot(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        gfxobj: u32,
        p: &super::scene::PartItems,
    ) -> Result<Option<u32>, dereth_gfx::Error> {
        if let Some(s) = self.slots.get(&gfxobj) {
            return Ok(Some(s.index));
        }
        let Some(bake) = self.slot_for(gfx, scene, gfxobj, p)? else {
            return Ok(None);
        };
        let index = self.slots.get(&gfxobj).map(|s| s.index);
        self.waiting.push(bake);
        Ok(index)
    }

    /// Give the tree whose first level's mesh is `gfxobj`, drawn by part `p`, a row and a place in
    /// the trees' table, and what makes its pictures; `None` for a mesh not held, or with no
    /// sphere.
    fn slot_for(
        &mut self,
        gfx: &mut Device,
        scene: &GpuScene,
        gfxobj: u32,
        p: &super::scene::PartItems,
    ) -> Result<Option<Bake>, dereth_gfx::Error> {
        let Some(mesh) = scene.meshes.get(&crate::assets::MeshKey::GfxObj(gfxobj)) else {
            return Ok(None);
        };
        let Some(first) = p.levels.first() else {
            return Ok(None);
        };
        let (centre, radius) = mesh.sphere.unwrap_or((mesh.sort_center, 0.0));
        if radius <= 0.0 {
            return Ok(None);
        }
        let items: Vec<(u32, u32, u32)> = first
            .clone()
            .filter_map(|i| {
                let draw = scene.item_draws.get(i as usize)?;
                Some((i, draw.first_index, draw.index_count))
            })
            .collect();
        let (page, row) = self.row(gfx)?;
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        let slot = Slot {
            page,
            row,
            centre: Vec3::from(centre),
            radius,
            index,
        };
        self.slots.insert(gfxobj, slot);
        self.stats.trees += 1;
        self.stats.baked += 1;
        self.tree_row(gfx, &slot)?;
        Ok(Some(Bake {
            page,
            row,
            centre: slot.centre,
            radius,
            items,
        }))
    }

    /// Write `slot`'s row into the table of trees' rows, the table grown when it is full.
    fn tree_row(&mut self, gfx: &mut Device, slot: &Slot) -> Result<(), dereth_gfx::Error> {
        let page = self.pages[slot.page];
        let mut row = Vec::with_capacity(TREE_BYTES);
        for w in [page.albedo_index, page.normal_index, slot.row, ROWS] {
            row.extend_from_slice(&w.to_le_bytes());
        }
        for f in [slot.centre.x, slot.centre.y, slot.centre.z, slot.radius] {
            row.extend_from_slice(&f.to_le_bytes());
        }
        let at = slot.index as usize * TREE_BYTES;
        let end = at + TREE_BYTES;
        if self.tree_rows.len() < end {
            self.tree_rows.resize(end, 0);
        }
        self.tree_rows[at..end].copy_from_slice(&row);
        if end as u64 > self.trees.2 {
            let size = (end as u64).next_power_of_two();
            let bigger = gfx.create_buffer(&BufferDesc {
                label: "remaster impostor trees",
                size,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            gfx.destroy_buffer(std::mem::replace(&mut self.trees.0, bigger));
            self.trees.1 = gfx.bind_buffer(self.trees.0)?;
            self.trees.2 = size;
            return gfx.write_buffer(self.trees.0, 0, &self.tree_rows);
        }
        gfx.write_buffer(self.trees.0, at as u64, &row)
    }

    /// Where the last frame's cards stand their trees' bases (the frame's space), and how far
    /// each was lifted onto the ground drawn over retail's.
    #[must_use]
    pub fn bases(&self) -> &[(Vec3, f32)] {
        &self.bases
    }

    /// The last frame's cards.
    #[must_use]
    pub fn drawn(&self) -> &[DrawnCard] {
        &self.drawn
    }

    /// What the last frame drew.
    #[must_use]
    pub fn stats(&self) -> ImpostorStats {
        self.stats
    }

    /// Where the impostor pictures of the mesh `gfxobj` are in the atlas, once made: from the
    /// frame that makes them, whose recording makes them before anything else reads them.
    #[must_use]
    pub fn atlas_row(&self, gfxobj: u32) -> Option<AtlasRow> {
        let s = self.slots.get(&gfxobj)?;
        let p = self.pages.get(s.page)?;
        Some(AtlasRow {
            albedo: (p.albedo, p.albedo_index),
            normal: (p.normal, p.normal_index),
            row: s.row,
            centre: s.centre,
            radius: s.radius,
        })
    }

    /// The atlas's pages, their colors and normals.
    pub fn pages(&self) -> impl Iterator<Item = Texture> + '_ {
        self.pages.iter().flat_map(|p| [p.albedo, p.normal])
    }

    /// The pipeline that draws a mesh's pictures from round it.
    #[must_use]
    pub(crate) fn bake_pipeline(&self) -> Pipeline {
        self.bake
    }

    /// A row for a tree's pictures: the next free one, on a new page when the last is full.
    fn row(&mut self, gfx: &mut Device) -> Result<(usize, u32), dereth_gfx::Error> {
        let held = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        let (page, row) = ((held / ROWS) as usize, held % ROWS);
        while self.pages.len() <= page {
            let mut make = |label| -> Result<(Texture, u32), dereth_gfx::Error> {
                let t = gfx.create_texture(&TextureDesc {
                    label,
                    width: VIEWS * TILE,
                    height: ROWS * TILE,
                    mips: 1,
                    format: PICTURE,
                    usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                })?;
                let i = gfx.bind_texture(t)?;
                Ok((t, i))
            };
            let (albedo, albedo_index) = make("remaster impostor colors")?;
            let (normal, normal_index) = make("remaster impostor normals")?;
            self.pages.push(Page {
                albedo,
                albedo_index,
                normal,
                normal_index,
            });
            self.stats.pages += 1;
        }
        if self.fine.is_none() {
            self.fine = Some(Fine::new(gfx, self.gather)?);
        }
        Ok((page, row))
    }

    /// The frame's cards written, and the trees first drawn so given rows; `None` when the frame
    /// draws no impostor.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        inputs: &ImpostorInputs<'_>,
    ) -> Result<Option<ImpostorPlan>, dereth_gfx::Error> {
        self.stats.cards = 0;
        self.stats.baked = u32::try_from(self.waiting.len()).unwrap_or(u32::MAX);
        self.bases.clear();
        self.drawn.clear();
        let mut bakes = std::mem::take(&mut self.waiting);
        if inputs.draws.is_empty() && inputs.device.is_none() && bakes.is_empty() {
            return Ok(None);
        }
        let frame = inputs.frame;
        let scene = inputs.scene;
        let mut rows: Vec<u8> = Vec::new();
        for d in inputs.draws {
            let Some(b) = scene.blocks.get(&d.block) else {
                continue;
            };
            let Some(p) = b.parts.get(d.part) else {
                continue;
            };
            let slot = match self.slots.get(&d.gfxobj) {
                Some(s) => *s,
                None => {
                    let Some(bake) = self.slot_for(gfx, scene, d.gfxobj, p)? else {
                        continue;
                    };
                    bakes.push(bake);
                    match self.slots.get(&d.gfxobj) {
                        Some(s) => *s,
                        None => continue,
                    }
                }
            };
            let page = self.pages[slot.page];
            let (x, y) = (i32::from(d.block >> 8), i32::from(d.block & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let offset = Vec3::new(
                (x - frame.origin[0]) as f32 * 192.0,
                (y - frame.origin[1]) as f32 * 192.0,
                0.0,
            );
            let t: Mat4 = p.transform;
            // Lifted onto the ground as the tree it stands for is.
            let base = t.w_axis.truncate() + offset;
            let lift = super::light::lifted(frame, p.lifts, base);
            self.bases.push((base + Vec3::new(0.0, 0.0, lift), lift));
            let centre = t.transform_point3(slot.centre) + offset + Vec3::new(0.0, 0.0, lift);
            let scale = t
                .x_axis
                .xyz()
                .length()
                .max(t.y_axis.xyz().length())
                .max(t.z_axis.xyz().length());
            let mut f = |v: [f32; 4]| {
                for x in v {
                    rows.extend_from_slice(&x.to_le_bytes());
                }
            };
            self.drawn.push(DrawnCard {
                gfxobj: d.gfxobj,
                centre,
                radius: slot.radius * scale,
                turn: dereth_primitives::num::math::atan2f(t.x_axis.y, t.x_axis.x),
                share: d.share,
            });
            f([centre.x, centre.y, centre.z, slot.radius * scale]);
            f([t.x_axis.x, t.x_axis.y, t.x_axis.z, d.share]);
            // The kind: foliage.
            f([t.y_axis.x, t.y_axis.y, t.y_axis.z, 1.0]);
            f([t.z_axis.x, t.z_axis.y, t.z_axis.z, 0.0]);
            for w in [page.albedo_index, page.normal_index, slot.row, ROWS] {
                rows.extend_from_slice(&w.to_le_bytes());
            }
        }
        let count = u32::try_from(rows.len() / (CARD_ROWS * 16)).unwrap_or(u32::MAX);
        if count == 0 && bakes.is_empty() && inputs.device.is_none() {
            return Ok(None);
        }
        let need = (rows.len() as u64).max(16);
        if need > self.cards_size {
            let size = need.next_power_of_two();
            let bigger = gfx.create_buffer(&BufferDesc {
                label: "remaster impostor cards",
                size,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            gfx.destroy_buffer(std::mem::replace(&mut self.cards, bigger));
            self.cards_index = gfx.bind_buffer(self.cards)?;
            self.cards_size = size;
        }
        if !rows.is_empty() {
            gfx.write_buffer(self.cards, 0, &rows)?;
        }
        self.stats.cards = count;
        Ok(Some(ImpostorPlan {
            bakes,
            pages: self.pages.clone(),
            fine: self.fine,
            cards: self.cards,
            cards_index: self.cards_index,
            count,
            frame: inputs.constants,
            items: (scene.items.buffer, scene.items.index),
            vertices: (scene.vertices.buffer, scene.vertices.index),
            materials: (scene.materials.buffer, scene.materials.index),
            indices: scene.indices.buffer,
            samplers: inputs.samplers,
            weather: inputs.weather,
            bake: self.bake,
            draw: self.draw,
            device: inputs.device,
            draw_stride: gfx.backend().command_words() * 4,
        }))
    }

    /// Record the impostors: the pictures of the trees first drawn this frame, then the cards
    /// into the G-buffer's targets `t`, after the meshes.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        plan: &ImpostorPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
    ) -> Result<(), dereth_gfx::Error> {
        if let Some(fine) = plan.fine {
            let touches = [
                Touch::Buffer(plan.items.0, Use::StorageRead),
                Touch::Buffer(plan.vertices.0, Use::StorageRead),
                Touch::Buffer(plan.materials.0, Use::StorageRead),
                Touch::Buffer(plan.indices, Use::Index),
            ];
            for b in &plan.bakes {
                let page = plan.pages[b.page];
                r.timestamp("impostor pictures");
                fine.record(
                    r,
                    (plan.bake, plan.indices),
                    &touches,
                    (page.albedo, page.normal),
                    b.row * TILE,
                    &mut |d, v| {
                        d.push(
                            0,
                            &bake_words(
                                plan.items.1,
                                plan.vertices.1,
                                plan.materials.1,
                                plan.samplers,
                                v,
                                (b.centre, b.radius),
                                Mat4::IDENTITY,
                                [0; 3],
                            ),
                        )?;
                        for (item, first, count) in &b.items {
                            d.draw_indexed(*count, 1, *first, 0, *item);
                        }
                        Ok(())
                    },
                )?;
            }
        }
        if plan.count == 0 && plan.device.is_none() {
            return Ok(());
        }
        r.timestamp("impostors");
        let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Keep,
        });
        let mut touches = vec![Touch::Buffer(plan.cards, Use::StorageRead)];
        if let Some(c) = plan.device {
            touches.push(Touch::Buffer(c.cards.0, Use::StorageRead));
            touches.push(Touch::Buffer(c.draw, Use::Indirect));
        }
        if let Some(cover) = plan.weather.1 {
            touches.push(Touch::Texture(cover, Use::Sampled));
        }
        for p in &plan.pages {
            touches.push(Touch::Texture(p.albedo, Use::Sampled));
            touches.push(Touch::Texture(p.normal, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "impostors",
                color: &keep,
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: None,
                    write: true,
                }),
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(plan.draw)?;
                if plan.count > 0 {
                    d.push(
                        0,
                        &push_words(&[
                            plan.frame,
                            plan.cards_index,
                            plan.samplers[1],
                            plan.weather.0,
                        ]),
                    )?;
                    d.draw(6, plan.count, 0, 0);
                }
                if let Some(c) = plan.device {
                    d.push(
                        0,
                        &push_words(&[plan.frame, c.cards.1, plan.samplers[1], plan.weather.0]),
                    )?;
                    d.draw_indirect(c.draw, 0, 1, plan.draw_stride)?;
                }
                Ok(())
            },
        )
    }
}

impl super::Renderer {
    /// What the far forest did.
    #[must_use]
    pub fn forest_stats(&self) -> super::forest::ForestStats {
        self.forest.stats()
    }

    /// What the impostors did: the cards the last frame drew are the processor's and the
    /// device's, read back after it has finished.
    pub fn impostor_stats(&mut self) -> ImpostorStats {
        let mut stats = self.impostors.stats();
        let device = self
            .instances
            .read_cards(&mut self.gfx)
            .map_or(0, |c| c.len());
        stats.cards += u32::try_from(device).unwrap_or(u32::MAX);
        stats
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    #[test]
    fn an_atlas_page_holds_a_row_of_pictures_for_each_of_its_trees() {
        assert_eq!(VIEWS * TILE, 4096);
        assert_eq!(ROWS * TILE, 2048);
        assert_eq!(CARD_ROWS * 16, 80);
    }
}
