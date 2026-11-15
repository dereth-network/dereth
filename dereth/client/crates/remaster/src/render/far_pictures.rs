//! The far forest's pictures made on the device: a tree's pictures from round it, made as the
//! impostors make theirs ([`super::impostor`]), drawn finer and gathered into a row, and their
//! smaller copies, which the far forest reads far off (`shaders/card.slang`).
//!
//! A tree of one mesh takes its impostor's atlas row as its full-size pictures where the atlas
//! holds one; otherwise, and for a tree of several meshes, its pictures are first drawn into a row
//! of their own, used and drawn over again. The smaller copies are made from that row into a far
//! picture, a slot of a pair of pages, and kept while the renderer is: every tree of the mesh, or
//! of the model, reads the same one. So the far forest's pictures are the impostors' own, and a
//! tree handed between the two draws alike. The odd levels lie in one page and the even in the
//! other, so each level is made from the one before it, four texels a texel.

use std::collections::HashMap;

use dereth_gfx::{
    Buffer, ColorAttachment, ColorTarget, Device, GraphicsDesc, Load, Pipeline, RasterPass,
    Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3};

use super::impostor::{bake_words, Fine, PICTURE, TILE, VIEWS};
use super::{push_words, shader};
use crate::shaders::Entry;

/// The smaller copies a far picture holds: half an atlas row's size, a quarter, and so on, to a
/// texel a view.
pub const LEVELS: u32 = 8;
/// A far picture's width and height in the page of its odd levels, and in that of its even ones,
/// texels: the page's first level across its top, the rest side by side under it.
pub const ODD: (u32, u32) = (VIEWS * TILE / 2, TILE / 2 + TILE / 8);
pub const EVEN: (u32, u32) = (VIEWS * TILE / 4, TILE / 4 + TILE / 16);
/// The far pictures a pair of pages holds, one over another.
pub const A_PAGE: u32 = 8;
/// The far pictures a frame makes, at most.
pub const MADE_A_FRAME: usize = 8;

/// What a far picture is made of: one mesh, its pictures shared by every model drawn with it
/// alone, or a model of several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PictureKey {
    Mesh(u32),
    Model(u32),
}

/// One mesh drawn into a model's pictures: where it stands in the space pictured, and each of its
/// subsets' first index, index count, first vertex, material and flags.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeMesh {
    pub frame: Mat4,
    pub draws: Vec<[u32; 5]>,
}

/// Where a far picture's full-size pictures come from.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// An impostor's atlas row: its page's color and normal pictures, each with its index in the
    /// table, and the row.
    Row {
        albedo: (Texture, u32),
        normal: (Texture, u32),
        row: u32,
    },
    /// Meshes drawn round the sphere `centre`, `radius`, into a row of their own.
    Bake {
        centre: Vec3,
        radius: f32,
        meshes: Vec<BakeMesh>,
    },
}

/// One far picture a frame makes: its slot, and where its full-size pictures come from.
#[derive(Debug, Clone, PartialEq)]
pub struct Making {
    pub slot: u32,
    pub source: Source,
}

/// A page of far pictures' levels: their colors and their normals, each with its index in the
/// table.
#[derive(Debug, Clone, Copy)]
struct Page {
    color: (Texture, u32),
    normal: (Texture, u32),
}

/// The row a model's full-size pictures are gathered into where no atlas row holds them, and
/// where they are drawn finer first.
#[derive(Debug, Clone, Copy)]
struct Scratch {
    color: (Texture, u32),
    normal: (Texture, u32),
    fine: Fine,
}

/// Where level `level` (1 and on) of a far picture lies in its slot of its page, texels.
#[must_use]
pub fn level_corner(level: u32) -> (u32, u32) {
    if level <= 2 {
        return (0, 0);
    }
    let x = (if level % 2 == 1 { 3 } else { 4 }..level)
        .step_by(2)
        .map(|k| (VIEWS * TILE) >> k)
        .sum();
    (x, TILE >> (2 - level % 2))
}

/// The far pictures made on the device: their pages, and which slot holds each.
#[derive(Debug)]
pub struct FarPictures {
    chain: Pipeline,
    gather: Pipeline,
    /// The pages, the odd levels' and the even levels' of each pair.
    pages: Vec<[Page; 2]>,
    // ORDER-OK: looked up by what a picture is made of.
    slots: HashMap<PictureKey, u32>,
    scratch: Option<Scratch>,
}

impl FarPictures {
    /// # Errors
    /// What the layer refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let targets = [PICTURE, PICTURE].map(|format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        });
        let chain = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster far pictures",
            vertex: dereth_gfx_passes::shaders::fullscreen(),
            fragment: Some(shader(Entry::ImpostorFsChain)),
            color: &targets,
            depth: None,
            cull: dereth_gfx::Cull::None,
        })?;
        Ok(Self {
            chain,
            gather: Fine::gather_pipeline(gfx)?,
            pages: Vec::new(),
            slots: HashMap::new(),
            scratch: None,
        })
    }

    /// The slot of the far picture made of `key`, once it is being made.
    #[must_use]
    pub fn slot(&self, key: PictureKey) -> Option<u32> {
        self.slots.get(&key).copied()
    }

    /// Where slot `slot` lies: its pages' color and normal pictures' indices, the odd levels' then
    /// the even's, and its place in them.
    #[must_use]
    pub fn place(&self, slot: u32) -> Option<([u32; 4], u32)> {
        let [o, e] = self.pages.get((slot / A_PAGE) as usize)?;
        Some((
            [o.color.1, o.normal.1, e.color.1, e.normal.1],
            slot % A_PAGE,
        ))
    }

    /// How many far pictures are made, and the bytes their pages and the row they are drawn into
    /// take on the device.
    #[must_use]
    pub fn held(&self) -> (u32, u64) {
        let page = u64::from(ODD.0 * ODD.1 + EVEN.0 * EVEN.1) * u64::from(A_PAGE) * 4 * 2;
        let scratch = if self.scratch.is_some() {
            u64::from(VIEWS * TILE) * u64::from(TILE) * 4 * 2 + Fine::BYTES
        } else {
            0
        };
        (
            u32::try_from(self.slots.len()).unwrap_or(u32::MAX),
            page * self.pages.len() as u64 + scratch,
        )
    }

    /// Make the far picture of `key` from `source`: a slot for it, on a new page when the last is
    /// full. What a frame records to make it.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn make(
        &mut self,
        gfx: &mut Device,
        key: PictureKey,
        source: Source,
    ) -> Result<Making, dereth_gfx::Error> {
        let slot = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        while self.pages.len() <= (slot / A_PAGE) as usize {
            let mut make =
                |label, (w, h): (u32, u32)| -> Result<(Texture, u32), dereth_gfx::Error> {
                    let t = gfx.create_texture(&TextureDesc {
                        label,
                        width: w,
                        height: h * A_PAGE,
                        mips: 1,
                        format: PICTURE,
                        usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                    })?;
                    Ok((t, gfx.bind_texture(t)?))
                };
            let odd = Page {
                color: make("remaster far picture colors", ODD)?,
                normal: make("remaster far picture normals", ODD)?,
            };
            let even = Page {
                color: make("remaster far picture colors", EVEN)?,
                normal: make("remaster far picture normals", EVEN)?,
            };
            self.pages.push([odd, even]);
        }
        if matches!(source, Source::Bake { .. }) && self.scratch.is_none() {
            let mut make = |label| -> Result<Texture, dereth_gfx::Error> {
                gfx.create_texture(&TextureDesc {
                    label,
                    width: VIEWS * TILE,
                    height: TILE,
                    mips: 1,
                    format: PICTURE,
                    usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                })
            };
            let color = make("remaster far picture row colors")?;
            let normal = make("remaster far picture row normals")?;
            self.scratch = Some(Scratch {
                color: (color, gfx.bind_texture(color)?),
                normal: (normal, gfx.bind_texture(normal)?),
                fine: Fine::new(gfx, self.gather)?,
            });
        }
        self.slots.insert(key, slot);
        Ok(Making { slot, source })
    }

    /// What a frame records to make `makings`, drawing meshes from the scene's buffers
    /// (`scene`: its vertices', materials' and indices' buffers, each with its index in the table
    /// where it has one) with the samplers that repeat and that hold at the edges.
    #[must_use]
    pub fn plan(
        &self,
        makings: Vec<Making>,
        bake: Pipeline,
        scene: ((Buffer, u32), (Buffer, u32), Buffer),
        samplers: [u32; 2],
    ) -> MakePlan {
        MakePlan {
            makings,
            pages: self.pages.clone(),
            scratch: self.scratch,
            chain: self.chain,
            bake,
            vertices: scene.0,
            materials: scene.1,
            indices: scene.2,
            samplers,
        }
    }

    /// Every page's pictures, for a pass that reads them.
    pub fn textures(&self) -> impl Iterator<Item = Texture> + '_ {
        self.pages
            .iter()
            .flatten()
            .flat_map(|p| [p.color.0, p.normal.0])
    }
}

/// What a frame records to make its far pictures.
#[derive(Debug, Clone)]
pub struct MakePlan {
    makings: Vec<Making>,
    pages: Vec<[Page; 2]>,
    scratch: Option<Scratch>,
    chain: Pipeline,
    bake: Pipeline,
    vertices: (Buffer, u32),
    materials: (Buffer, u32),
    indices: Buffer,
    samplers: [u32; 2],
}

impl MakePlan {
    /// Whether the frame makes none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.makings.is_empty()
    }

    /// Record the far pictures' making: each one's full-size pictures drawn where no atlas row
    /// holds them, then its smaller copies.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(&self, r: &mut Recorder<'_>) -> Result<(), dereth_gfx::Error> {
        if self.makings.is_empty() {
            return Ok(());
        }
        r.timestamp("far pictures");
        for m in &self.makings {
            let (color, normal, row) = match &m.source {
                Source::Row {
                    albedo,
                    normal,
                    row,
                } => (*albedo, *normal, row * TILE),
                Source::Bake {
                    centre,
                    radius,
                    meshes,
                } => {
                    let Some(s) = self.scratch else {
                        continue;
                    };
                    self.record_bake(r, s, (*centre, *radius), meshes)?;
                    (s.color, s.normal, 0)
                }
            };
            let Some(pages) = self.pages.get((m.slot / A_PAGE) as usize) else {
                continue;
            };
            let place = m.slot % A_PAGE;
            // Each level from the one before: the first from the row of full-size pictures.
            let mut from = (color, normal, (0, row));
            for level in 1..=LEVELS {
                let (page, height) = if level % 2 == 1 {
                    (pages[0], ODD.1)
                } else {
                    (pages[1], EVEN.1)
                };
                let (x, y) = level_corner(level);
                let y = place * height + y;
                let side = TILE >> level;
                let rect = Rect {
                    x: i32::try_from(x).unwrap_or(0),
                    y: i32::try_from(y).unwrap_or(0),
                    width: VIEWS * side,
                    height: side,
                };
                let (src_color, src_normal, (fx, fy)) = from;
                r.raster(
                    &RasterPass {
                        name: "far pictures",
                        color: &[
                            ColorAttachment {
                                texture: page.color.0,
                                load: Load::Keep,
                            },
                            ColorAttachment {
                                texture: page.normal.0,
                                load: Load::Keep,
                            },
                        ],
                        depth: None,
                        touches: &[
                            Touch::Texture(src_color.0, Use::Sampled),
                            Touch::Texture(src_normal.0, Use::Sampled),
                        ],
                        area: Some(rect),
                    },
                    |d| {
                        d.set_pipeline(self.chain)?;
                        d.push(
                            0,
                            &push_words(&[
                                src_color.1,
                                src_normal.1,
                                fx,
                                fy,
                                u32::from(level == 1),
                                x,
                                y,
                            ]),
                        )?;
                        d.draw(3, 1, 0, 0);
                        Ok(())
                    },
                )?;
                from = (page.color, page.normal, (x, y));
            }
        }
        Ok(())
    }

    /// Draw `meshes` from round the sphere `centre`, `radius` into the row `s`, a view a picture,
    /// finer, and gather them into it.
    fn record_bake(
        &self,
        r: &mut Recorder<'_>,
        s: Scratch,
        (centre, radius): (Vec3, f32),
        meshes: &[BakeMesh],
    ) -> Result<(), dereth_gfx::Error> {
        let touches = [
            Touch::Buffer(self.vertices.0, Use::StorageRead),
            Touch::Buffer(self.materials.0, Use::StorageRead),
            Touch::Buffer(self.indices, Use::Index),
        ];
        s.fine.record(
            r,
            (self.bake, self.indices),
            &touches,
            (s.color.0, s.normal.0),
            0,
            &mut |d, v| {
                for m in meshes {
                    for [first, count, vertex, material, flags] in &m.draws {
                        d.push(
                            0,
                            &bake_words(
                                super::scene::NONE,
                                self.vertices.1,
                                self.materials.1,
                                self.samplers,
                                v,
                                (centre, radius),
                                m.frame,
                                [*vertex, *material, *flags],
                            ),
                        )?;
                        d.draw_indexed(*count, 1, *first, 0, 0);
                    }
                }
                Ok(())
            },
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A far picture's levels lie side by side in its two pages and fit them, each half the one
    /// before, the first of each page across its top.
    #[test]
    fn a_far_pictures_levels_fit_its_pages_side_by_side() {
        assert_eq!((ODD, EVEN), ((2048, 160), (1024, 80)));
        for (first, size) in [(1, ODD), (2, EVEN)] {
            assert_eq!(level_corner(first), (0, 0));
            assert_eq!(VIEWS * (TILE >> first), size.0, "the first fills the width");
            let under = TILE >> first;
            let mut end = 0;
            for level in (first + 2..=LEVELS).step_by(2) {
                let (x, y) = level_corner(level);
                assert_eq!((x, y), (end, under), "level {level}");
                end = x + VIEWS * (TILE >> level);
                assert!(y + (TILE >> level) <= size.1, "level {level}");
            }
            assert!(end <= size.0, "{end}");
        }
        assert_eq!(TILE >> LEVELS, 1, "the last level a texel a view");
    }
}
