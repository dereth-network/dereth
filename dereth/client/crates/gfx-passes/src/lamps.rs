//! The lamps: lights worked out per pixel, from lists of the lights that reach each piece of the
//! view, and the soft halos round the ones the eye can see.
//!
//! The view is cut into [`GRID`] pieces, its tiles of the screen by its slices of depth, and each
//! piece lists the lamps whose reach meets it (a compute pass, `lamps.slang`'s `cs_clusters`);
//! the game's own shading then adds, at each pixel, the light of its piece's lamps into
//! [`LampsPass`]'s target, the lamps' light, which the game's frame takes as it lights each
//! surface. [`clusters`] is the same listing on the processor. The halos (`halo.slang`) are added
//! over the scene's colors after everything the world drew, and what stands nearer the eye than a
//! lamp cuts its halo.
//!
//! A lamp is three rows of four 32-bit values ([`LAMP_BYTES`]): where it stands in the frame's
//! space and how far its light reaches, then two rows of the game's own (its color, its kind, its
//! strengths), which only the game's shading reads.
//!
//! The game's shading is a fragment shader over the whole target, given these push words: the
//! pass's constants ([`frame_rows`]), the lamps, the pieces' lists, then the G-buffer's normal and
//! material, the depth, and the depth the outdoor surfaces are placed by (each by its index in the
//! table). Its alpha is the game's own.

use dereth_gfx::{
    Blend, Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ComputeDesc, Cull,
    Device, Factor, Format, GraphicsDesc, Load, Location, Pipeline, RasterPass, Recorder, Rect,
    Shader, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use crate::frame::push_words;
use crate::gbuffer::Targets;
use crate::shaders::{shader, Entry};

/// The view's pieces: tiles across, tiles down, slices of depth.
pub const GRID: [u32; 3] = [16, 9, 24];

/// The lamps one piece lists at most.
pub const PER_PIECE: u32 = 63;

/// The words one piece takes in the list: its count, then its lamps.
const PIECE_WORDS: u32 = PER_PIECE + 1;

/// The depth the slices start at and end at, metres from the eye. Nearer than the first is the
/// first slice; nothing farther than the last is lit.
pub const SLICE_NEAR: f32 = 0.5;
pub const SLICE_FAR: f32 = 240.0;

/// The depth each slice starts at, and the last one's end, metres from the eye, as the bits of
/// each: from [`SLICE_NEAR`] to [`SLICE_FAR`] in equal ratios, `SLICE_NEAR * (SLICE_FAR /
/// SLICE_NEAR)^(k / GRID[2])` worked out by the workspace's `powf` (which this crate does not
/// reach).
const SLICE_DEPTHS: [u32; GRID[2] as usize + 1] = [
    0x3f00_0000,
    0x3f25_8cb8,
    0x3f56_1d65,
    0x3f8a_76a4,
    0x3fb3_1529,
    0x3fe7_9e26,
    0x4015_c840,
    0x4041_b8cc,
    0x407a_8d32,
    0x40a2_06ba,
    0x40d1_8ecb,
    0x4107_843b,
    0x412f_456f,
    0x4162_b032,
    0x4192_9829,
    0x41bd_9950,
    0x41f5_3814,
    0x421e_93ed,
    0x424d_1907,
    0x4284_a1e0,
    0x42ab_8a77,
    0x42dd_dd16,
    0x430f_7975,
    0x4339_9048,
    0x4370_0000,
];

/// The bytes one lamp takes in the lamps' buffer: three rows of four 32-bit values.
pub const LAMP_BYTES: usize = 48;

/// The format of the lamps' light.
pub const LAMP_LIGHT: Format = Format::Rgba16Float;

/// One lamp's halo: where it stands, how wide, and its color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Halo {
    pub position: Vec3,
    pub size: f32,
    pub color: [f32; 3],
}

/// The halos' constants as rows of four 32-bit values: the view's right and up, then for each its
/// place and width, and its color.
fn halo_rows(view: Mat4, halos: &[Halo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + halos.len() * 32);
    let mut f = |v: [f32; 4]| {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    };
    let to_world = view.inverse();
    let (right, up) = (to_world.x_axis, to_world.y_axis);
    f([right.x, right.y, right.z, 0.0]);
    f([up.x, up.y, up.z, 0.0]);
    for h in halos {
        f([h.position.x, h.position.y, h.position.z, h.size]);
        f([h.color[0], h.color[1], h.color[2], 0.0]);
    }
    out
}

/// How the view is cut into [`GRID`]'s pieces for a frame: its camera's view and projection and
/// the world's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub projection: Mat4,
    pub extent: (u32, u32),
}

impl View {
    /// Each tile's size in pixels.
    #[must_use]
    pub fn tile(&self) -> (u32, u32) {
        (
            self.extent.0.div_ceil(GRID[0]).max(1),
            self.extent.1.div_ceil(GRID[1]).max(1),
        )
    }

    /// The depth slice `k` starts at, metres from the eye ([`SLICE_DEPTHS`]).
    #[must_use]
    pub fn slice_depth(k: u32) -> f32 {
        f32::from_bits(SLICE_DEPTHS[(k as usize).min(SLICE_DEPTHS.len() - 1)])
    }

    /// The direction from the eye, in the eye's space, through the point `ndc` of the screen,
    /// scaled to depth 1.
    fn ray(&self, ndc: (f32, f32)) -> Vec3 {
        let p = self.projection.inverse() * Vec4::new(ndc.0, ndc.1, 1.0, 1.0);
        let v = p.xyz() / p.w;
        if v.z.abs() < 1e-9 {
            return Vec3::Z;
        }
        v / v.z
    }

    /// Piece `(i, j, k)`'s bounds in the eye's space: its corner nearest the origin of each
    /// axis, and its farthest.
    #[must_use]
    pub fn bounds(&self, i: u32, j: u32, k: u32) -> (Vec3, Vec3) {
        let (tw, th) = self.tile();
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let ndc = |px: u32, py: u32| {
            (
                px.min(self.extent.0) as f32 / self.extent.0.max(1) as f32 * 2.0 - 1.0,
                1.0 - py.min(self.extent.1) as f32 / self.extent.1.max(1) as f32 * 2.0,
            )
        };
        let (z0, z1) = (Self::slice_depth(k), Self::slice_depth(k + 1));
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for (px, py) in [
            (i * tw, j * th),
            ((i + 1) * tw, j * th),
            (i * tw, (j + 1) * th),
            ((i + 1) * tw, (j + 1) * th),
        ] {
            let r = self.ray(ndc(px, py));
            for z in [z0, z1] {
                let p = r * z;
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
        (lo, hi)
    }
}

/// Whether the sphere at `centre` of radius `radius` meets the box from `lo` to `hi`.
#[must_use]
pub fn meets(centre: Vec3, radius: f32, lo: Vec3, hi: Vec3) -> bool {
    let nearest = centre.clamp(lo, hi);
    nearest.distance_squared(centre) <= radius * radius
}

/// Each piece's lamps, as the compute pass lists them, for lamps standing at `lamps`' places with
/// their reaches: piece `(i, j, k)` at `(k * GRID[1] + j) * GRID[0] + i`, its lamps in order, at
/// most [`PER_PIECE`].
#[must_use]
pub fn clusters(lamps: &[(Vec3, f32)], v: &View) -> Vec<Vec<u32>> {
    let eye_space: Vec<Vec3> = lamps
        .iter()
        .map(|(p, _)| v.view.transform_point3(*p))
        .collect();
    let mut out = Vec::new();
    for k in 0..GRID[2] {
        for j in 0..GRID[1] {
            for i in 0..GRID[0] {
                let (lo, hi) = v.bounds(i, j, k);
                let mut list = Vec::new();
                for (n, ((_, reach), c)) in lamps.iter().zip(&eye_space).enumerate() {
                    if list.len() >= PER_PIECE as usize {
                        break;
                    }
                    if meets(*c, *reach, lo, hi) {
                        list.push(u32::try_from(n).unwrap_or(u32::MAX));
                    }
                }
                out.push(list);
            }
        }
    }
    out
}

/// The pass's constants for a frame cut as `v`, its eye at `eye`: the view, the projection's
/// inverse, the view and projection's inverse, the eye, the world's size and a tile's, the slices
/// with `soften` (the distance the game's lamps stop growing nearer than), what `lit` says is lit
/// (the indoors, the outdoors), and the grid.
#[must_use]
pub fn frame_rows(v: &View, eye: Vec3, soften: f32, lit: [bool; 2]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut f = |x: [f32; 4]| {
        for w in x {
            out.extend_from_slice(&w.to_le_bytes());
        }
    };
    for m in [
        v.view,
        v.projection.inverse(),
        (v.projection * v.view).inverse(),
    ] {
        for c in m.to_cols_array_2d() {
            f(c);
        }
    }
    f([eye.x, eye.y, eye.z, 1.0]);
    let (tw, th) = v.tile();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    f([v.extent.0 as f32, v.extent.1 as f32, tw as f32, th as f32]);
    #[allow(clippy::cast_precision_loss)] // a few dozen slices
    f([SLICE_NEAR, SLICE_FAR, GRID[2] as f32, soften]);
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    f([flag(lit[0]), flag(lit[1]), 0.0, 0.0]);
    for w in [GRID[0], GRID[1], GRID[2], PER_PIECE] {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// One frame's lamps, as the pass records them.
#[derive(Debug, Clone, Copy)]
pub struct LampsPlan {
    /// The lamps' light, and its index in the table: what the game's shading wrote.
    pub light: Texture,
    pub light_index: u32,
    lamps: Buffer,
    lamps_index: u32,
    pieces: Buffer,
    pieces_index: u32,
    rows: u32,
    count: u32,
    /// Whether the viewer stands inside with the outdoors drawn: the outdoor surfaces are then
    /// placed by the outdoor depth.
    inside: bool,
    clusters: Pipeline,
    shade: Pipeline,
    /// The halos' constants, how many, and how they are drawn on the frame's kind of target.
    halos: Option<(u32, u32, Pipeline)>,
}

/// What the pass draws of one frame, as the game works it out.
#[derive(Debug, Clone, Copy)]
pub struct LampsFrame<'a> {
    /// The lamps, [`LAMP_BYTES`] each, and their places and reaches, in the same order.
    pub lamps: &'a [u8],
    pub reaches: &'a [(Vec3, f32)],
    /// The halos to stand round the lamps.
    pub halos: &'a [Halo],
    /// How the view is cut, and the pass's constants ([`frame_rows`]).
    pub view: View,
    pub rows: &'a [u8],
    /// Whether the scene's colors are linear this frame, as the post passes want them.
    pub linear: bool,
    /// Whether the viewer stands inside with the outdoors drawn.
    pub inside: bool,
}

/// The lamps' pass: its pipelines, its buffers, and its target.
#[derive(Debug)]
pub struct LampsPass {
    clusters: Pipeline,
    shade: Pipeline,
    /// The halos over each kind of target: the world's display colors, and linear light.
    halo: [(Format, Pipeline); 2],
    halos: Grow,
    lamps: Grow,
    rows: Grow,
    pieces: Buffer,
    pieces_index: u32,
    light: Option<(Texture, u32, u32, u32)>,
}

/// A storage buffer rewritten every frame, grown when a frame needs more.
#[derive(Debug)]
struct Grow {
    buffer: Buffer,
    index: u32,
    size: u64,
    label: &'static str,
}

impl Grow {
    fn new(d: &mut Device, label: &'static str, size: u64) -> Result<Self, dereth_gfx::Error> {
        let buffer = d.create_buffer(&BufferDesc {
            label,
            size,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        let index = d.bind_buffer(buffer)?;
        Ok(Self {
            buffer,
            index,
            size,
            label,
        })
    }

    fn write(&mut self, d: &mut Device, bytes: &[u8]) -> Result<(), dereth_gfx::Error> {
        let need = (bytes.len() as u64).max(16);
        if need > self.size {
            let size = need.next_power_of_two();
            let bigger = d.create_buffer(&BufferDesc {
                label: self.label,
                size,
                usage: BufferUsage::STORAGE,
                location: Location::Gpu,
            })?;
            d.destroy_buffer(std::mem::replace(&mut self.buffer, bigger));
            self.index = d.bind_buffer(self.buffer)?;
            self.size = size;
        }
        if bytes.is_empty() {
            return Ok(());
        }
        d.write_buffer(self.buffer, 0, bytes)
    }
}

/// The pieces' list, every piece's words.
const PIECES_BYTES: u64 = (GRID[0] * GRID[1] * GRID[2] * PIECE_WORDS) as u64 * 4;

impl LampsPass {
    /// The pass on `gfx`, its pipelines made: the game's shading `shade`, and the halos over the
    /// world's display colors of format `color` and over linear light.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(
        gfx: &mut Device,
        shade: Shader<'_>,
        color: Format,
    ) -> Result<Self, dereth_gfx::Error> {
        let clusters = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster lamp lists",
            shader: shader(Entry::LampsCsClusters),
        })?;
        let shade = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster lamps",
            vertex: crate::shaders::fullscreen(),
            fragment: Some(shade),
            color: &[ColorTarget {
                format: LAMP_LIGHT,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        let linear = crate::post::scene_format(gfx);
        let mut halo = |format| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster lamp halos",
                vertex: shader(Entry::HaloVsHalo),
                fragment: Some(shader(Entry::HaloFsHalo)),
                color: &[ColorTarget {
                    format,
                    blend: Some(Blend {
                        src: Factor::One,
                        dst: Factor::One,
                    }),
                    write_alpha: false,
                }],
                depth: None,
                cull: Cull::None,
            })
            .map(|p| (format, p))
        };
        let halo = [halo(color)?, halo(linear)?];
        let pieces = gfx.create_buffer(&BufferDesc {
            label: "remaster lamp pieces",
            size: PIECES_BYTES,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })?;
        let pieces_index = gfx.bind_buffer(pieces)?;
        Ok(Self {
            clusters,
            shade,
            halo,
            halos: Grow::new(gfx, "remaster lamp halos", 1 << 12)?,
            lamps: Grow::new(gfx, "remaster lamps", 1 << 14)?,
            rows: Grow::new(gfx, "remaster lamp frame", 512)?,
            pieces,
            pieces_index,
            light: None,
        })
    }

    /// The frame's lamps written, their light's target made at the world's size (`t`'s).
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        t: &Targets,
        f: &LampsFrame<'_>,
    ) -> Result<LampsPlan, dereth_gfx::Error> {
        let light = match self.light {
            Some((tex, index, w, h)) if (w, h) == (t.width, t.height) => (tex, index),
            old => {
                if let Some((tex, ..)) = old {
                    gfx.destroy_texture(tex);
                }
                let tex = gfx.create_texture(&TextureDesc {
                    label: "remaster lamps' light",
                    width: t.width,
                    height: t.height,
                    mips: 1,
                    format: LAMP_LIGHT,
                    usage: TextureUsage::COLOR_TARGET
                        | TextureUsage::SAMPLED
                        | TextureUsage::TRANSFER_SRC,
                })?;
                let index = gfx.bind_texture(tex)?;
                self.light = Some((tex, index, t.width, t.height));
                (tex, index)
            }
        };
        self.lamps.write(gfx, f.lamps)?;
        self.rows.write(gfx, f.rows)?;
        let count = u32::try_from(f.reaches.len()).unwrap_or(u32::MAX);
        let halos = if f.halos.is_empty() {
            None
        } else {
            self.halos.write(gfx, &halo_rows(f.view.view, f.halos))?;
            let format = if f.linear {
                crate::post::scene_format(gfx)
            } else {
                self.halo[0].0
            };
            self.halo.iter().find(|(h, _)| *h == format).map(|(_, p)| {
                (
                    self.halos.index,
                    u32::try_from(f.halos.len()).unwrap_or(0),
                    *p,
                )
            })
        };
        Ok(LampsPlan {
            light: light.0,
            light_index: light.1,
            lamps: self.lamps.buffer,
            lamps_index: self.lamps.index,
            pieces: self.pieces,
            pieces_index: self.pieces_index,
            rows: self.rows.index,
            count,
            inside: f.inside,
            clusters: self.clusters,
            shade: self.shade,
            halos,
        })
    }

    /// Record the lamps: their lists, then their light, from the G-buffer's targets `t`.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        plan: &LampsPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
    ) -> Result<(), dereth_gfx::Error> {
        r.timestamp("lamp lists");
        r.compute(
            &[
                Touch::Buffer(plan.lamps, Use::StorageRead),
                Touch::Buffer(plan.pieces, Use::StorageWrite),
            ],
            |c| {
                c.set_pipeline(plan.clusters)?;
                c.push(
                    0,
                    &push_words(&[plan.rows, plan.lamps_index, plan.count, plan.pieces_index]),
                )?;
                c.dispatch((GRID[0] * GRID[1] * GRID[2]).div_ceil(64), 1, 1);
                Ok(())
            },
        )?;
        let outdoor_depth = if plan.inside {
            t.outdoor_depth
        } else {
            t.depth
        };
        let outdoor_depth_index = if plan.inside { t.index[6] } else { t.index[4] };
        let words = push_words(&[
            plan.rows,
            plan.lamps_index,
            plan.pieces_index,
            t.index[1],
            t.index[3],
            t.index[4],
            outdoor_depth_index,
        ]);
        r.timestamp("lamps");
        r.raster(
            &RasterPass {
                name: "lamps",
                color: &[ColorAttachment {
                    texture: plan.light,
                    load: Load::Clear([0.0; 4]),
                }],
                depth: None,
                touches: &[
                    Touch::Buffer(plan.lamps, Use::StorageRead),
                    Touch::Buffer(plan.pieces, Use::StorageRead),
                    Touch::Texture(t.normal, Use::Sampled),
                    Touch::Texture(t.material, Use::Sampled),
                    Touch::Texture(t.depth, Use::Sampled),
                    Touch::Texture(outdoor_depth, Use::Sampled),
                ],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(plan.shade)?;
                d.push(0, &words)?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// Record the lamps' halos over the scene's colors, for frame constants `frame`. It goes after
    /// everything the world drew, and the depth tells what stands in front of each.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record_halos(
        plan: &LampsPlan,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
    ) -> Result<(), dereth_gfx::Error> {
        let Some((rows, count, pipeline)) = plan.halos else {
            return Ok(());
        };
        let mut touches = vec![Touch::Texture(t.depth, Use::Sampled)];
        if plan.inside {
            touches.push(Touch::Texture(t.outdoor_depth, Use::Sampled));
        }
        r.timestamp("lamp halos");
        let words = push_words(&[
            frame,
            rows,
            t.index[4],
            if plan.inside { t.index[6] } else { t.index[4] },
        ]);
        r.raster(
            &RasterPass {
                name: "lamp halos",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(pipeline)?;
                d.push(0, &words)?;
                d.draw(6, count, 0, 0);
                Ok(())
            },
        )
    }

    /// The last frame's lamps' light and lamp lists, read back once the device has finished
    /// them: the light as four half floats a pixel, and each piece's lamps.
    pub fn read_back(&self, gfx: &mut Device) -> Option<(Vec<u8>, Vec<Vec<u32>>)> {
        let (tex, ..) = self.light?;
        gfx.wait_idle().ok()?;
        let light = gfx.read_texture(tex, 0).ok()?;
        let readback = gfx
            .create_buffer(&BufferDesc {
                label: "remaster lamp pieces read back",
                size: PIECES_BYTES,
                usage: BufferUsage::TRANSFER_DST,
                location: Location::Readback,
            })
            .ok()?;
        let bytes = (|| {
            let mut r = gfx.begin()?;
            r.copy_buffer(self.pieces, 0, readback, 0, PIECES_BYTES)?;
            r.touch(&[Touch::Buffer(readback, Use::HostRead)])?;
            let ticket = r.submit()?;
            gfx.wait(ticket)?;
            gfx.read_buffer(readback, 0, PIECES_BYTES)
        })();
        gfx.destroy_buffer(readback);
        let words: Vec<u32> = bytes
            .ok()?
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let lists = words
            .chunks(PIECE_WORDS as usize)
            .map(|p| {
                let n = (p[0] as usize).min(PER_PIECE as usize);
                p[1..=n].to_vec()
            })
            .collect();
        Some((light, lists))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use dereth_primitives::num::math;

    #[test]
    fn each_slice_starts_where_the_ratios_put_it() {
        for k in 0..=GRID[2] {
            #[allow(clippy::cast_precision_loss)] // a few dozen slices
            let t = k as f32 / GRID[2] as f32;
            let want = SLICE_NEAR * math::powf(SLICE_FAR / SLICE_NEAR, t);
            assert_eq!(View::slice_depth(k).to_bits(), want.to_bits(), "slice {k}");
        }
    }
}
