//! Far trees as impostors: each tree's full mesh drawn once into a row of an atlas page, a
//! picture from each of [`VIEWS`] directions round it, level with it; then, wherever the levels
//! of detail stand a tree's impostor in for its meshes ([`super::lod`]), an upright card turned
//! to the eye, drawn into the G-buffer after the meshes, showing the pictures either side of the
//! eye's direction and lit as the sun lights what stands outdoors.
//!
//! A tree's pictures are made the frame it is first drawn so, and kept while the renderer is.

use std::collections::HashMap;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Compare, Cull, DepthAttachment,
    DepthState, Device, Format, GraphicsDesc, IndexType, Load, Location, Pipeline, RasterPass,
    Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4Swizzles};

use super::scene::GpuScene;
use super::{push_words, shader, Targets, ALBEDO, DEPTH, LIGHT, MATERIAL, NORMAL};
use crate::scene::{BlockId, FrameDelta};
use crate::shaders::Entry;

/// The pictures of a tree, one each from as many directions round it.
pub const VIEWS: u32 = 16;

/// A picture's side, pixels.
pub const TILE: u32 = 128;

/// The trees an atlas page holds, a row each.
pub const ROWS: u32 = 16;

/// The pages' format: a picture's color and coverage, and its normals.
const PICTURE: Format = Format::Rgba8Unorm;

/// One tree a frame draws as an impostor: its block, its place among the block's parts, how
/// much of it the impostor draws (1 all of it), and the tree's first level's GfxObj.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImpostorDraw {
    pub block: BlockId,
    pub part: usize,
    pub share: f32,
    pub gfxobj: u32,
}

/// Where a tree's pictures are, and the sphere round the mesh they are of, in its own space.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    page: usize,
    row: u32,
    centre: Vec3,
    radius: f32,
}

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
    depth: Option<Texture>,
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
}

/// The impostors' pass: its pipelines, its atlas pages, and the trees they hold.
#[derive(Debug)]
pub struct ImpostorPass {
    bake: Pipeline,
    draw: Pipeline,
    pages: Vec<Page>,
    depth: Option<Texture>,
    // ORDER-OK: looked up by tree.
    slots: HashMap<u32, Slot>,
    cards: Buffer,
    cards_index: u32,
    cards_size: u64,
    stats: ImpostorStats,
    /// Where the last frame's cards stand their trees' bases, and how far each was lifted.
    bases: Vec<(Vec3, f32)>,
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
        let cards_size = 1 << 16;
        let cards = gfx.create_buffer(&BufferDesc {
            label: "remaster impostor cards",
            size: cards_size,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        let cards_index = gfx.bind_buffer(cards)?;
        Ok(Self {
            bake,
            draw,
            pages: Vec::new(),
            depth: None,
            slots: HashMap::new(),
            cards,
            cards_index,
            cards_size,
            stats: ImpostorStats::default(),
            bases: Vec::new(),
        })
    }

    /// Where the last frame's cards stand their trees' bases (the frame's space), and how far
    /// each was lifted onto the ground drawn over retail's.
    #[must_use]
    pub fn bases(&self) -> &[(Vec3, f32)] {
        &self.bases
    }

    /// What the last frame drew.
    #[must_use]
    pub fn stats(&self) -> ImpostorStats {
        self.stats
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
        if self.depth.is_none() {
            self.depth = Some(gfx.create_texture(&TextureDesc {
                label: "remaster impostor depth",
                width: VIEWS * TILE,
                height: ROWS * TILE,
                mips: 1,
                format: DEPTH,
                usage: TextureUsage::DEPTH_TARGET,
            })?);
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
        self.stats.baked = 0;
        self.bases.clear();
        if inputs.draws.is_empty() {
            return Ok(None);
        }
        let frame = inputs.frame;
        let scene = inputs.scene;
        let mut bakes = Vec::new();
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
                    let Some(mesh) = scene.meshes.get(&crate::assets::MeshKey::GfxObj(d.gfxobj))
                    else {
                        continue;
                    };
                    let Some(first) = p.levels.first() else {
                        continue;
                    };
                    let (centre, radius) = mesh.sphere.unwrap_or((mesh.sort_center, 0.0));
                    if radius <= 0.0 {
                        continue;
                    }
                    let items: Vec<(u32, u32, u32)> = first
                        .clone()
                        .filter_map(|i| {
                            let draw = scene.item_draws.get(i as usize)?;
                            Some((i, draw.first_index, draw.index_count))
                        })
                        .collect();
                    let (page, row) = self.row(gfx)?;
                    let slot = Slot {
                        page,
                        row,
                        centre: Vec3::from(centre),
                        radius,
                    };
                    self.slots.insert(d.gfxobj, slot);
                    self.stats.trees += 1;
                    self.stats.baked += 1;
                    bakes.push(Bake {
                        page,
                        row,
                        centre: slot.centre,
                        radius,
                        items,
                    });
                    slot
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
        if count == 0 && bakes.is_empty() {
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
            depth: self.depth,
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
        if let Some(depth) = plan.depth {
            for b in &plan.bakes {
                let page = plan.pages[b.page];
                let area = Rect {
                    x: 0,
                    y: i32::try_from(b.row * TILE).unwrap_or(0),
                    width: VIEWS * TILE,
                    height: TILE,
                };
                r.timestamp("impostor pictures");
                r.raster(
                    &RasterPass {
                        name: "impostor pictures",
                        color: &[
                            ColorAttachment {
                                texture: page.albedo,
                                load: Load::Clear([0.0; 4]),
                            },
                            ColorAttachment {
                                texture: page.normal,
                                load: Load::Clear([0.5, 0.5, 1.0, 0.0]),
                            },
                        ],
                        depth: Some(DepthAttachment {
                            texture: depth,
                            clear: Some(1.0),
                            write: true,
                        }),
                        touches: &[
                            Touch::Buffer(plan.items.0, Use::StorageRead),
                            Touch::Buffer(plan.vertices.0, Use::StorageRead),
                            Touch::Buffer(plan.materials.0, Use::StorageRead),
                            Touch::Buffer(plan.indices, Use::Index),
                        ],
                        area: Some(area),
                    },
                    |d| {
                        d.set_pipeline(plan.bake)?;
                        d.bind_index_buffer(plan.indices, 0, IndexType::U32)?;
                        for v in 0..VIEWS {
                            let tile = Rect {
                                x: i32::try_from(v * TILE).unwrap_or(0),
                                y: area.y,
                                width: TILE,
                                height: TILE,
                            };
                            d.set_viewport(tile);
                            d.set_scissor(tile);
                            let mut words = push_words(&[
                                plan.items.1,
                                plan.vertices.1,
                                plan.materials.1,
                                plan.samplers[0],
                                plan.samplers[1],
                                v,
                            ]);
                            for x in [b.centre.x, b.centre.y, b.centre.z, b.radius] {
                                words.extend_from_slice(&x.to_le_bytes());
                            }
                            d.push(0, &words)?;
                            for (item, first, count) in &b.items {
                                d.draw_indexed(*count, 1, *first, 0, *item);
                            }
                        }
                        Ok(())
                    },
                )?;
            }
        }
        if plan.count == 0 {
            return Ok(());
        }
        r.timestamp("impostors");
        let keep = [t.albedo, t.normal, t.light, t.material].map(|texture| ColorAttachment {
            texture,
            load: Load::Keep,
        });
        let mut touches = vec![Touch::Buffer(plan.cards, Use::StorageRead)];
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

    /// What the impostors did.
    #[must_use]
    pub fn impostor_stats(&self) -> ImpostorStats {
        self.impostors.stats()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    #[test]
    fn an_atlas_page_holds_a_row_of_pictures_for_each_of_its_trees() {
        assert_eq!(VIEWS * TILE, 2048);
        assert_eq!(ROWS * TILE, 2048);
        assert_eq!(CARD_ROWS * 16, 80);
    }
}
