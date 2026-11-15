//! The remaster's device, what it keeps there, and each frame's draws.
//!
//! The device is the backend layer's own ([`Remaster::new`], no window). A frame is drawn in the
//! retail order (`auberean_draw::scene::PASSES`): the sky's objects with no depth, the fog veil,
//! the landscape, the cells, the decals, the opaque objects, the casters, the water, the frills,
//! the objects seen through, the particle systems, the streaks and the sky's late objects; each
//! step's objects farthest first, each object's draws in their order. Each draw takes the frame's
//! light it is made under (its own, the outdoors', the sky's, or the light of the cells looked
//! into) and its own constants, both as the retail device's shaders read them.
//!
//! Before the frame, when some water lies below the camera, its reflection: the same scene from the
//! camera mirrored about the water's plane, into a picture of its own, and the ripples' picture
//! the water's surfaces push it about by.
//!
//! Not yet drawn here: the shores' alpha (the water is laid over a frame whose alpha is the same
//! everywhere), what stands under the water seen past it in the reflection, the sun's shadow map of
//! the near objects, the bodies' shade from it, the volumes of shadow, and the sky's lens flares.
//! The shared passes' masks replace the shadows.

use std::collections::HashMap;
use std::sync::Arc;

use auberean_draw::constants::{
    bound_stages, draw_uniforms, mat, view_proj, DrawUniforms, FrameUniforms, MeshGpuVertex,
    TerrainGpuVertex,
};
use auberean_draw::cull::Sight;
use auberean_draw::material::d3d;
use auberean_draw::picture::Image;
use auberean_draw::scene::{
    in_order, Cull as Seen, Frame, ObjectBatch, Pass, TerrainBatch, Textures, PASSES,
};
use bytemuck::Zeroable;
use dereth_gfx::{
    Address, Blend, Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Compare, Cull,
    DepthAttachment, DepthState, Device, DeviceDesc, Factor, Filter, Format, GraphicsDesc,
    IndexType, Load, Location, Pipeline, RasterPass, SamplerDesc, Texture, TextureDesc,
    TextureUsage, Touch, Use,
};
use dereth_gfx_passes::frame::{push_words, PerFrame, Rows};
use dereth_primitives::num::math;

use crate::shaders::{shader, Entry};

/// The world's colors, as the retail device keeps them.
const COLOR: Format = Format::Rgba8Unorm;
/// Its depth.
const DEPTH: Format = Format::D32Float;

/// How many frames a list's or a picture's own buffer is kept after the last frame that drew it.
const KEEP_FRAMES: u64 = 240;

/// The first of a frame's constants' rows that are the game's own (`LIGHT_ROWS` in
/// `shaders/light.slang`): the shared rows come before it.
const LIGHT_ROWS: usize = 32;

/// How the landscape's layers blend: the bottom layer as it is, the others by their alpha.
const OPAQUE: (u32, u32) = (d3d::BLEND_ONE, d3d::BLEND_ZERO);
const BLENDED: (u32, u32) = (d3d::BLEND_SRC_ALPHA, d3d::BLEND_INV_SRC_ALPHA);

/// The reflection's picture and the ripples', square whatever the screen's size.
const REFLECTION_SIZE: u32 = 512;
/// How far the ripples push the reflection: 0.04 of the picture per unit of offset.
const RIPPLE_PUSH: f32 = 0.04;

/// The frame's alpha the draws that blend by the destination's alpha take, as a factor the device
/// is told (the shaders hold the same, `FRAME_ALPHA` in `shaders/mesh.slang`).
const ALPHA_KEEP: u32 = 0;
const ALPHA_FRAME: u32 = 1;
const ALPHA_SATURATED: u32 = 2;

/// A drawn picture: `width * height` texels of red, green, blue, alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why a frame could not be drawn.
#[derive(Debug)]
pub enum RemasterError {
    /// The backend layer refused.
    Device(dereth_gfx::Error),
}

impl std::fmt::Display for RemasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Device(e) => write!(f, "the remaster's device: {e}"),
        }
    }
}

impl std::error::Error for RemasterError {}

impl From<dereth_gfx::Error> for RemasterError {
    fn from(e: dereth_gfx::Error) -> Self {
        Self::Device(e)
    }
}

/// Which pipeline a draw takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Program {
    Mesh,
    Terrain(Entry),
    Veil,
    /// The reflection laid over the water's surface.
    Water,
    /// The water's surface into the ripples' picture.
    Ripple,
}

/// A pipeline's key: its program, how it blends (the stored factors), how it tests and writes
/// depth, and how it culls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    program: Program,
    src: u32,
    dst: u32,
    depth_test: bool,
    depth_write: bool,
    cull: u32,
}

/// The light a draw is made under: the frame's own, the outdoors' (seen from inside), the sky's,
/// or the light of the cells looked into from the land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Env {
    Own = 0,
    Outside = 1,
    Sky = 2,
    Looked = 3,
}

/// Something kept on the device while frames go on using it.
#[derive(Debug)]
struct Kept {
    buffer: Buffer,
    /// Its index in the table, for a buffer the shaders read.
    index: u32,
    used: u64,
    /// The list it was made from, held so its address stays its own while it keys the buffer. Once
    /// nothing else holds it (a frame's own list, gone with the frame), no frame draws it again.
    list: Arc<dyn std::any::Any + Send + Sync>,
}

/// One draw of the frame, ready to record.
#[derive(Debug, Clone, Copy)]
struct Draw {
    key: Key,
    /// The vertices' buffer, by its index in the table (the landscape's or a mesh's).
    vertices: u32,
    /// The frame's posed vertices' first for this draw, when it is a moved body's.
    posed: Option<u32>,
    indices: Buffer,
    index_type: IndexType,
    index_count: u32,
    /// The draw's constants' place among the frame's.
    record: u32,
    textures: [u32; 3],
    samplers: [u32; 3],
    env: Env,
    alpha_mode: u32,
    /// It is seen in the frame's own picture, and in the water's reflection.
    shown: bool,
    reflected: bool,
    /// How the reflection draws it when that differs (fainter nearer the edge of its reach): its
    /// pipeline and its constants.
    faint: Option<(Key, u32)>,
}

/// The frame's targets.
#[derive(Debug, Clone, Copy)]
struct Targets {
    width: u32,
    height: u32,
    color: Texture,
    depth: Texture,
}

/// The remaster: its device, the meshes, landscape and pictures it keeps there, its pipelines and
/// the frame's buffers.
#[derive(Debug)]
pub struct Remaster {
    gfx: Device,
    /// The adapter's name.
    pub adapter: String,
    /// Each vertex list's buffer, by the list's place in memory and length.
    vertices: HashMap<(usize, usize), Kept>,
    /// Each index list's buffer, by the same.
    indices: HashMap<(usize, usize), Kept>,
    /// Each picture, by its id, with its index in the table.
    pictures: HashMap<u32, (Texture, u32, u64)>,
    white: u32,
    /// The samplers that wrap and that clamp, by their index in the table, as `textures_as` says.
    wrap: u32,
    clamp: u32,
    textures_as: Textures,
    trilinear: Option<bool>,
    pipelines: HashMap<Key, Pipeline>,
    /// The four lights' constants, and every draw's.
    lights: [PerFrame; 4],
    draws: PerFrame,
    posed: PerFrame,
    targets: Option<Targets>,
    /// The four lights seen from the camera mirrored about the water.
    mirrored: [PerFrame; 4],
    /// The reflection's picture, the ripples' and their depth; the bump picture the ripples are
    /// drawn from, by its index in the table.
    reflection: Option<(Texture, Texture, Texture, u32, u32)>,
    bump: Option<u32>,
    frame: u64,
}

/// A stored blend factor as the device blends it, the destination's alpha read as a source's (the
/// shader then puts the frame's alpha in its own).
fn factor(f: u32) -> Factor {
    match f {
        1 => Factor::Zero,
        3 => Factor::SrcColor,
        4 => Factor::OneMinusSrcColor,
        5 | 7 | 11 => Factor::SrcAlpha,
        6 | 8 => Factor::OneMinusSrcAlpha,
        9 => Factor::DstColor,
        10 => Factor::OneMinusDstColor,
        _ => Factor::One,
    }
}

/// How a draw of the stored factors `src` and `dst` takes its alpha for the blend.
const fn alpha_mode(src: u32, dst: u32) -> u32 {
    match (src, dst) {
        (7 | 8, _) | (_, 7 | 8) => ALPHA_FRAME,
        (11, _) => ALPHA_SATURATED,
        _ => ALPHA_KEEP,
    }
}

/// A picture's levels: the picture and its smaller levels as stored.
fn levels(img: &Image) -> Vec<(u32, u32, &[u8])> {
    std::iter::once((img.width, img.height, img.bgra.as_slice()))
        .chain(
            img.smaller
                .iter()
                .map(|l| (l.width, l.height, l.bgra.as_slice())),
        )
        .collect()
}

/// The list `v` as bytes the device takes.
fn bytes_of<T: bytemuck::Pod>(v: &[T]) -> &[u8] {
    bytemuck::cast_slice(v)
}

impl Remaster {
    /// The remaster on a device of the layer's own, on the hardware adapter.
    ///
    /// # Errors
    /// When there is no such device, or it refuses what the remaster makes.
    pub fn new() -> Result<Self, RemasterError> {
        Self::with(&DeviceDesc::default())
    }

    /// The remaster on a device of the layer's own made as `desc` asks (with the validation layer,
    /// for one).
    ///
    /// # Errors
    /// When there is no such device, or it refuses what the remaster makes.
    pub fn with(desc: &DeviceDesc) -> Result<Self, RemasterError> {
        let gfx = Device::create(desc)?;
        Self::on(gfx)
    }

    /// The remaster on `gfx`.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn on(mut gfx: Device) -> Result<Self, RemasterError> {
        let adapter = gfx.caps().name.clone();
        let white = {
            let t = gfx.create_texture(&TextureDesc {
                label: "remaster white",
                width: 1,
                height: 1,
                mips: 1,
                format: Format::Bgra8Unorm,
                usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            })?;
            gfx.upload_texture(t, 0, &[255; 4])?;
            gfx.bind_texture(t)?
        };
        let textures_as = Textures::default();
        let (wrap, clamp) = Self::samplers(&mut gfx, textures_as)?;
        let mut light = |label| PerFrame::new(&mut gfx, label, 4096, BufferUsage::default());
        let lights = [
            light("remaster frame light")?,
            light("remaster outdoors light")?,
            light("remaster sky light")?,
            light("remaster looked light")?,
        ];
        let mirrored = [
            light("remaster mirrored frame light")?,
            light("remaster mirrored outdoors light")?,
            light("remaster mirrored sky light")?,
            light("remaster mirrored looked light")?,
        ];
        let draws = PerFrame::new(&mut gfx, "remaster draws", 1 << 20, BufferUsage::default())?;
        let posed = PerFrame::new(&mut gfx, "remaster posed", 1 << 20, BufferUsage::default())?;
        Ok(Self {
            gfx,
            adapter,
            vertices: HashMap::new(),
            indices: HashMap::new(),
            pictures: HashMap::new(),
            white,
            wrap,
            clamp,
            textures_as,
            trilinear: None,
            pipelines: HashMap::new(),
            lights,
            draws,
            posed,
            targets: None,
            mirrored,
            reflection: None,
            bump: None,
            frame: 0,
        })
    }

    /// The layer's device the remaster draws on.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.gfx
    }

    /// The samplers that wrap and that clamp, by their index in the table, filtering as `textures`
    /// says: between a texture's two nearest levels with trilinear filtering on, else from the
    /// nearest alone; along the slant as far as 16 with anisotropic filtering on, only between
    /// levels.
    // D2-09793, D2-09862, D2-09861
    fn samplers(gfx: &mut Device, textures: Textures) -> Result<(u32, u32), RemasterError> {
        let mut make = |address| -> Result<u32, RemasterError> {
            let s = gfx.create_sampler(&SamplerDesc {
                filter: Filter::Linear,
                mip: if textures.trilinear {
                    Filter::Linear
                } else {
                    Filter::Nearest
                },
                address,
                address_v: address,
                anisotropy: if textures.anisotropic && textures.trilinear {
                    16
                } else {
                    1
                },
                lod_bias: 0.0,
            })?;
            Ok(gfx.bind_sampler(s)?)
        };
        Ok((make(Address::Repeat)?, make(Address::Clamp)?))
    }

    /// Take the textures as `textures` says, as the retail device does: at another detail every
    /// picture is let go and loaded again; with other filtering the samplers are made again.
    /// Trilinear filtering is taken from the first frame's preference and kept.
    fn take_textures(&mut self, textures: Textures) -> Result<(), RemasterError> {
        let textures = match self.trilinear {
            Some(t) => Textures {
                trilinear: t,
                ..textures
            },
            None => {
                self.trilinear = Some(textures.trilinear);
                textures
            }
        };
        if textures == self.textures_as {
            return Ok(());
        }
        if textures.detail != self.textures_as.detail {
            for (_, (t, _, _)) in self.pictures.drain() {
                self.gfx.destroy_texture(t);
            }
        }
        if (textures.trilinear, textures.anisotropic)
            != (self.textures_as.trilinear, self.textures_as.anisotropic)
        {
            (self.wrap, self.clamp) = Self::samplers(&mut self.gfx, textures)?;
        }
        self.textures_as = textures;
        Ok(())
    }

    /// Picture `id`'s index in the table, loaded with `load` the first time it is asked for,
    /// without the largest levels the texture detail leaves out; white for none.
    fn picture(
        &mut self,
        id: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<u32, RemasterError> {
        if id == 0 {
            return Ok(self.white);
        }
        if let Some(p) = self.pictures.get_mut(&id) {
            p.2 = self.frame;
            return Ok(p.1);
        }
        let Some(img) = load(id) else {
            return Ok(self.white);
        };
        let img = img.at_detail(self.textures_as.detail);
        let levels = levels(&img);
        let t = self.gfx.create_texture(&TextureDesc {
            label: "remaster picture",
            width: img.width,
            height: img.height,
            mips: u32::try_from(levels.len()).unwrap_or(1),
            format: Format::Bgra8Unorm,
            usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        })?;
        for (level, (_, _, data)) in levels.iter().enumerate() {
            self.gfx
                .upload_texture(t, u32::try_from(level).unwrap_or(0), data)?;
        }
        let index = self.gfx.bind_texture(t)?;
        self.pictures.insert(id, (t, index, self.frame));
        Ok(index)
    }

    /// A list's buffer, kept while frames draw it: made and filled from `bytes` the first time.
    fn kept<L: std::any::Any + Send + Sync>(
        gfx: &mut Device,
        store: &mut HashMap<(usize, usize), Kept>,
        (list, len): (&Arc<L>, usize),
        frame: u64,
        usage: BufferUsage,
        bytes: impl FnOnce() -> Vec<u8>,
    ) -> Result<(Buffer, u32), RemasterError> {
        let key = (Arc::as_ptr(list) as usize, len);
        if let Some(k) = store.get_mut(&key) {
            k.used = frame;
            return Ok((k.buffer, k.index));
        }
        let data = bytes();
        let buffer = gfx.create_buffer(&BufferDesc {
            label: "remaster list",
            size: (data.len() as u64).max(16),
            usage: usage | BufferUsage::TRANSFER_DST,
            location: Location::Gpu,
        })?;
        gfx.write_buffer(buffer, 0, &data)?;
        let index = if usage.contains(BufferUsage::STORAGE) {
            gfx.bind_buffer(buffer)?
        } else {
            0
        };
        store.insert(
            key,
            Kept {
                buffer,
                index,
                used: frame,
                list: list.clone(),
            },
        );
        Ok((buffer, index))
    }

    /// A mesh's vertices' buffer, by its index in the table.
    fn mesh_vertices(
        &mut self,
        list: &Arc<Vec<auberean_draw::mesh::MeshVertex>>,
    ) -> Result<u32, RemasterError> {
        Self::kept(
            &mut self.gfx,
            &mut self.vertices,
            (list, list.len()),
            self.frame,
            BufferUsage::STORAGE,
            || {
                let v: Vec<MeshGpuVertex> = list.iter().map(MeshGpuVertex::of).collect();
                bytes_of(&v).to_vec()
            },
        )
        .map(|k| k.1)
    }

    /// The landscape's vertices' buffer, by its index in the table.
    fn land_vertices(
        &mut self,
        list: &Arc<Vec<auberean_draw::scene::TerrainVertex>>,
    ) -> Result<u32, RemasterError> {
        Self::kept(
            &mut self.gfx,
            &mut self.vertices,
            (list, list.len()),
            self.frame,
            BufferUsage::STORAGE,
            || {
                let v: Vec<TerrainGpuVertex> = list
                    .iter()
                    .map(|v| TerrainGpuVertex {
                        position: v.position,
                        color: v.color,
                        uv: v.uv,
                        mask_uv: v.mask_uv,
                    })
                    .collect();
                bytes_of(&v).to_vec()
            },
        )
        .map(|k| k.1)
    }

    /// An index list's buffer.
    fn index_list<T: bytemuck::Pod + Send + Sync>(
        &mut self,
        list: &Arc<Vec<T>>,
    ) -> Result<Buffer, RemasterError> {
        Self::kept(
            &mut self.gfx,
            &mut self.indices,
            (list, list.len()),
            self.frame,
            BufferUsage::INDEX,
            || bytes_of(list.as_slice()).to_vec(),
        )
        .map(|k| k.0)
    }

    /// The pipeline of `key`, made the first time it is asked for.
    fn pipeline(&mut self, key: Key) -> Result<Pipeline, RemasterError> {
        if let Some(p) = self.pipelines.get(&key) {
            return Ok(*p);
        }
        let (vertex, fragment) = match key.program {
            Program::Mesh => (Entry::MeshVsMesh, Entry::MeshFsMesh),
            Program::Water => (Entry::MeshVsMesh, Entry::MeshFsWater),
            Program::Ripple => (Entry::MeshVsMesh, Entry::MeshFsRipple),
            Program::Terrain(fs) => (Entry::TerrainVsTerrain, fs),
            Program::Veil => (Entry::TerrainVsVeil, Entry::TerrainFsVeil),
        };
        let blend = (key.src != d3d::BLEND_ONE || key.dst != d3d::BLEND_ZERO).then(|| Blend {
            src: factor(key.src),
            dst: factor(key.dst),
        });
        let p = self.gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster world",
            vertex: shader(vertex),
            fragment: Some(shader(fragment)),
            color: &[ColorTarget {
                format: COLOR,
                blend,
                write_alpha: false,
            }],
            depth: Some(DepthState {
                format: DEPTH,
                compare: Some(if key.depth_test {
                    Compare::LessEqual
                } else {
                    Compare::Always
                }),
                write: key.depth_write,
            }),
            // The layer winds its front faces counter-clockwise on the screen as the retail
            // device does, where the layer's front faces are its clockwise ones: the stored
            // culling of clockwise faces culls the layer's front ones.
            cull: match key.cull {
                d3d::CULL_CW => Cull::Front,
                d3d::CULL_CCW => Cull::Back,
                _ => Cull::None,
            },
        })?;
        self.pipelines.insert(key, p);
        Ok(p)
    }

    /// The reflection's picture, the ripples' picture, their depth, and the two pictures' indices in
    /// the table.
    fn reflection_targets(
        &mut self,
    ) -> Result<(Texture, Texture, Texture, u32, u32), RemasterError> {
        if let Some(r) = self.reflection {
            return Ok(r);
        }
        let mut color = |label| {
            self.gfx.create_texture(&TextureDesc {
                label,
                width: REFLECTION_SIZE,
                height: REFLECTION_SIZE,
                mips: 1,
                format: COLOR,
                usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
            })
        };
        let picture = color("remaster reflection")?;
        let ripples = color("remaster ripples")?;
        let depth = self.gfx.create_texture(&TextureDesc {
            label: "remaster reflection depth",
            width: REFLECTION_SIZE,
            height: REFLECTION_SIZE,
            mips: 1,
            format: DEPTH,
            usage: TextureUsage::DEPTH_TARGET,
        })?;
        let r = (
            picture,
            ripples,
            depth,
            self.gfx.bind_texture(picture)?,
            self.gfx.bind_texture(ripples)?,
        );
        self.reflection = Some(r);
        Ok(r)
    }

    /// The ripples' bump picture, by its index in the table: three waves' push across and down,
    /// each truncated on its own and summed in a signed byte, as the retail device makes it. The
    /// picture keeps what is above zero, which is all the ripples' picture keeps of it.
    fn bump_picture(&mut self) -> Result<u32, RemasterError> {
        if let Some(b) = self.bump {
            return Ok(b);
        }
        let mut data = vec![0u8; 256 * 256 * 4];
        for y in 0..256u32 {
            for x in 0..256u32 {
                #[allow(clippy::cast_precision_loss)]
                let (fx, fy) = (x as f32 / 256.0 - 0.5, y as f32 / 256.0 - 0.5);
                let r = (fx * fx + fy * fy).sqrt();
                let fade = math::expf(-5.0 * r);
                let (a, b, c) = (300.0 * r, 150.0 * (fx + fy), 140.0 * (fx * 0.85 - fy));
                let int = |v: f32| dereth_primitives::num::to_i32_f64(f64::from(v));
                let du = int(64.0 * math::cosf(a) * fade)
                    + int(32.0 * math::cosf(b))
                    + int(16.0 * math::cosf(c));
                let dv = int(64.0 * math::sinf(a) * fade)
                    + int(32.0 * math::sinf(b))
                    + int(16.0 * math::sinf(c));
                // A signed byte's value, -1 to 1, kept above zero as an unsigned one's.
                let kept = |d: i32| {
                    let signed = f32::from(d.to_le_bytes()[0].cast_signed()).max(-127.0) / 127.0;
                    u8::try_from(
                        dereth_primitives::num::to_i32(signed.max(0.0) * 255.0 + 0.5).clamp(0, 255),
                    )
                    .unwrap_or(0)
                };
                let k = ((y * 256 + x) * 4) as usize;
                data[k] = kept(du);
                data[k + 1] = kept(dv);
                data[k + 3] = 255;
            }
        }
        let t = self.gfx.create_texture(&TextureDesc {
            label: "remaster ripples' bump",
            width: 256,
            height: 256,
            mips: 1,
            format: Format::Rgba8Unorm,
            usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
        })?;
        self.gfx.upload_texture(t, 0, &data)?;
        let b = self.gfx.bind_texture(t)?;
        self.bump = Some(b);
        Ok(b)
    }

    /// The frame's targets at `width` x `height`.
    fn targets(&mut self, width: u32, height: u32) -> Result<Targets, RemasterError> {
        if let Some(t) = self.targets {
            if (t.width, t.height) == (width, height) {
                return Ok(t);
            }
            // On a device it shares, another drawer may still be reading the old picture.
            if !self.gfx.owned() {
                self.gfx.wait_idle()?;
            }
            self.gfx.destroy_texture(t.color);
            self.gfx.destroy_texture(t.depth);
        }
        let color = self.gfx.create_texture(&TextureDesc {
            label: "remaster color",
            width,
            height,
            mips: 1,
            format: COLOR,
            usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC,
        })?;
        let depth = self.gfx.create_texture(&TextureDesc {
            label: "remaster depth",
            width,
            height,
            mips: 1,
            format: DEPTH,
            usage: TextureUsage::DEPTH_TARGET | TextureUsage::SAMPLED,
        })?;
        let t = Targets {
            width,
            height,
            color,
            depth,
        };
        self.targets = Some(t);
        Ok(t)
    }

    /// The constants of a light: the shared rows (the camera, the sun, the ambient, the fog, the
    /// target's extent), then the game's own from `LIGHT_ROWS`.
    fn light_rows(u: &FrameUniforms, extent: (u32, u32)) -> Vec<u8> {
        let vp = glam::Mat4::from_cols_array_2d(&u.view_proj);
        let mut rows = Rows::default();
        rows.mat(vp);
        // The view alone is not kept apart: the passes that read it are not drawn yet.
        rows.mat(glam::Mat4::IDENTITY);
        rows.f(u.eye);
        rows.f(u.to_sun);
        rows.f(u.sun_color);
        rows.f(u.ambient);
        rows.f(u.ambient);
        rows.f([u.fog[0], u.fog[1], u.fog[3], u.fog[2]]);
        rows.f(u.fog_color);
        rows.f([1.0, 1.0, 0.0, 0.0]);
        rows.mat(vp.inverse());
        #[allow(clippy::cast_precision_loss)]
        rows.f([extent.0 as f32, extent.1 as f32, 0.0, 0.0]);
        while rows.0.len() < LIGHT_ROWS {
            rows.f([0.0; 4]);
        }
        let mut out = rows.bytes();
        out.extend_from_slice(bytemuck::bytes_of(u));
        out
    }

    /// Draw `frame` at `width` x `height` and read the picture back.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn render(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Picture, RemasterError> {
        let color = self.draw(frame, width, height, load)?;
        self.gfx.wait_idle()?;
        let rgba = self.gfx.read_texture(color, 0)?;
        Ok(Picture {
            width,
            height,
            rgba,
        })
    }

    /// Draw `frame` at `width` x `height` into a picture left on the device, loading pictures with
    /// `load` the first time each is used; the device's work is submitted.
    ///
    /// # Errors
    /// What the device refuses.
    #[allow(clippy::too_many_lines)] // one pass, in the world's drawing order
    pub fn draw(
        &mut self,
        frame: &Frame,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<Texture, RemasterError> {
        self.frame += 1;
        self.take_textures(frame.textures)?;
        let cam = &frame.camera;
        let (h, p) = (cam.heading.to_radians(), cam.pitch.to_radians());
        let forward = [
            math::sinf(h) * math::cosf(p),
            math::cosf(h) * math::cosf(p),
            math::sinf(p),
        ];
        let eye = [cam.position.x, cam.position.y, cam.position.z];
        #[allow(clippy::cast_precision_loss)]
        let aspect = width as f32 / height as f32;
        let vp = view_proj(
            eye,
            forward,
            cam.roll,
            cam.fov_y,
            cam.orthographic,
            aspect,
            cam.near,
            cam.far,
        );
        // Seen from inside, only what the doorways show at the moment.
        // D2-11042
        let sight = frame.sight(aspect);
        let outdoors = sight.as_ref().is_none_or(Sight::outdoors);
        let sky_shown = sight.as_ref().is_none_or(Sight::sky);
        let mut frame_u = FrameUniforms {
            view_proj: mat(&vp),
            eye: [eye[0], eye[1], eye[2], 1.0],
            forward: [forward[0], forward[1], forward[2], 0.0],
            alpha_fog: {
                let [start, end] = frame.frill_fade;
                let span = if end > start {
                    1.0 / (end - start)
                } else {
                    0.0
                };
                [start, end, span, 0.0]
            },
            shadow_uv: frame
                .shadow
                .as_ref()
                .map_or(auberean_world::math::Mat4::IDENTITY.0, |s| {
                    s.texture_coordinates().0
                }),
            cloud: frame.clouds.map_or([0.0; 4], |c| {
                let off = c.offset(frame.uptime);
                [c.scale, off[0], off[1], 1.0]
            }),
            terrain: [
                if frame.shadow.is_some() { 1.0 } else { 0.0 },
                0.0,
                0.0,
                0.0,
            ],
            #[allow(clippy::cast_precision_loss)]
            viewport: [width as f32, height as f32, 0.0, 0.0],
            ..FrameUniforms::zeroed()
        };
        let mut out_u = frame_u;
        frame_u.light(&frame.light);
        out_u.light(frame.outside.as_ref().unwrap_or(&frame.light));
        let sun = frame_u.sun_color;
        let out_sun = out_u.sun_color;
        let mut sky_u = out_u;
        sky_u.fog[3] = 0.0;
        let mut looked_u = frame_u;
        if let Some(l) = &frame.looked_light {
            looked_u.light(l);
        }
        let looked_sun = looked_u.sun_color;
        for (k, u) in [&frame_u, &out_u, &sky_u, &looked_u]
            .into_iter()
            .enumerate()
        {
            let bytes = Self::light_rows(u, (width, height));
            self.lights[k].write(&mut self.gfx, &bytes)?;
        }

        let mut records: Vec<DrawUniforms> = Vec::new();
        let mut posed: Vec<auberean_draw::mesh::PosedVertex> = Vec::new();
        let mut steps = Steps::default();
        let shown = |b: &&ObjectBatch| sight.as_ref().is_none_or(|s| s.shows(b));

        // The objects of `batches` under `under`, into `step`.
        let objects = |this: &mut Self,
                       batches: Vec<&ObjectBatch>,
                       under: Env,
                       step: Pass,
                       records: &mut Vec<DrawUniforms>,
                       posed: &mut Vec<auberean_draw::mesh::PosedVertex>,
                       steps: &mut Steps,
                       load: &mut dyn FnMut(u32) -> Option<Image>|
         -> Result<(), RemasterError> {
            for b in batches {
                if b.indices.is_empty() || !b.drawn() {
                    continue;
                }
                let env = match under {
                    Env::Own if b.outside => Env::Outside,
                    Env::Own if frame.looked_light.is_some() && b.cull.is_looked_in() => {
                        Env::Looked
                    }
                    e => e,
                };
                let sun = match env {
                    Env::Own => sun,
                    Env::Looked => looked_sun,
                    _ => out_sun,
                };
                let faded = b.layer.faded(b.tint.a, b.far);
                let key = Key {
                    program: Program::Mesh,
                    src: b.layer.source_blend,
                    dst: b.layer.dest_blend,
                    depth_test: b.layer.depth_test != d3d::CMP_ALWAYS && env != Env::Sky,
                    depth_write: b.layer.depth_write && env != Env::Sky && faded.depth_write,
                    cull: b.layer.cull,
                };
                let stages = bound_stages(&b.layer);
                let mut textures = [this.white; 3];
                let mut samplers = [this.wrap; 3];
                for (k, s) in stages.iter().enumerate() {
                    if let Some(s) = s {
                        textures[k] = this.picture(s.texture, load)?;
                        if s.clamp_u || s.clamp_v {
                            samplers[k] = this.clamp;
                        }
                    }
                }
                let mut u = draw_uniforms(&b.layer, &b.transform, b.tint, sun, &b.points);
                u.params[1] = faded.alpha_ref.unwrap_or(-1.0);
                let record = u32::try_from(records.len()).unwrap_or(u32::MAX);
                records.push(u);
                let vertices = this.mesh_vertices(&b.vertices)?;
                let pose = match &b.posed {
                    Some(list) => {
                        let base = u32::try_from(posed.len()).unwrap_or(0);
                        posed.extend_from_slice(list.shared().as_slice());
                        Some(base)
                    }
                    None => None,
                };
                let indices = this.index_list(&b.indices)?;
                // The reflection gathers what stands on the land alone, no cell's, above the
                // water's plane; it reaches its own edge, fading there as the frame does at its
                // own.
                // D2-11030, D2-06658
                let in_reflection = b.reflected(frame.camera.position)
                    && !matches!(
                        b.cull,
                        Seen::Cell(_)
                            | Seen::InCell { .. }
                            | Seen::Looked { .. }
                            | Seen::LookedIn { .. }
                            | Seen::Body { .. }
                    );
                let above = frame.reflection.is_none_or(|r| {
                    env == Env::Sky || b.step.is_some() || b.stands_above(r.height)
                });
                let faint = match frame.reflection {
                    Some(_) if in_reflection => {
                        let a = b.reflected_alpha(frame.camera.position);
                        if (a - b.tint.a).abs() < 0.001 {
                            None
                        } else {
                            let tint = auberean_world::color::Color { a, ..b.tint };
                            let faded = b.layer.faded(a, b.far);
                            let mut u = draw_uniforms(&b.layer, &b.transform, tint, sun, &b.points);
                            u.params[1] = faded.alpha_ref.unwrap_or(-1.0);
                            let at = u32::try_from(records.len()).unwrap_or(u32::MAX);
                            records.push(u);
                            Some((
                                Key {
                                    depth_write: b.layer.depth_write
                                        && env != Env::Sky
                                        && faded.depth_write,
                                    ..key
                                },
                                at,
                            ))
                        }
                    }
                    _ => None,
                };
                steps.push(
                    step,
                    Draw {
                        key,
                        vertices,
                        posed: pose,
                        indices,
                        index_type: IndexType::U16,
                        index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                        record,
                        textures,
                        samplers,
                        env,
                        alpha_mode: alpha_mode(b.layer.source_blend, b.layer.dest_blend),
                        shown: b.shown(),
                        reflected: in_reflection && above,
                        faint,
                    },
                );
            }
            Ok(())
        };

        let none: &[ObjectBatch] = &[];
        let (sky_before, sky_after) = if sky_shown {
            (&frame.sky_before[..], &frame.sky_after[..])
        } else {
            (none, none)
        };
        let (terrain, water, foam) = if outdoors {
            (&frame.terrain[..], &frame.water[..], &frame.foam[..])
        } else {
            (&[][..], none, none)
        };
        objects(
            self,
            opaque_first(sky_before),
            Env::Sky,
            Pass::Sky,
            &mut records,
            &mut posed,
            &mut steps,
            load,
        )?;
        // The landscape, under the outdoors' light.
        let land = |this: &mut Self,
                    b: &TerrainBatch,
                    fs: Entry,
                    textures: [u32; 2],
                    samplers: [u32; 2],
                    steps: &mut Steps,
                    (src, dst): (u32, u32)|
         -> Result<(), RemasterError> {
            if b.indices.is_empty() {
                return Ok(());
            }
            let vertices = this.land_vertices(&b.vertices)?;
            let indices = this.index_list(&b.indices)?;
            steps.push(
                Pass::Terrain,
                Draw {
                    key: Key {
                        program: Program::Terrain(fs),
                        src,
                        dst,
                        depth_test: true,
                        depth_write: (src, dst) == OPAQUE,
                        cull: d3d::CULL_CW,
                    },
                    vertices,
                    posed: None,
                    indices,
                    index_type: IndexType::U32,
                    index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                    record: 0,
                    textures: [textures[0], textures[1], this.white],
                    samplers: [samplers[0], samplers[1], this.wrap],
                    env: Env::Outside,
                    alpha_mode: ALPHA_KEEP,
                    shown: true,
                    reflected: true,
                    faint: None,
                },
            );
            Ok(())
        };
        let (near, far): (Vec<&TerrainBatch>, Vec<&TerrainBatch>) =
            terrain.iter().partition(|b| !b.far);
        let clouds = match frame.clouds {
            Some(c) => self.picture(c.picture, load)?,
            None => self.white,
        };
        for b in near.iter().filter(|b| b.layer == 0) {
            let ground = self.picture(b.ground, load)?;
            let (w, c) = (self.wrap, self.clamp);
            land(
                self,
                b,
                Entry::TerrainFsTerrainBase,
                [ground, self.white],
                [w, c],
                &mut steps,
                OPAQUE,
            )?;
        }
        for b in near.iter().filter(|b| b.layer > 0) {
            let ground = self.picture(b.ground, load)?;
            let mask = match b.mask {
                Some(m) => self.picture(m, load)?,
                None => self.white,
            };
            let (w, c) = (self.wrap, self.clamp);
            land(
                self,
                b,
                Entry::TerrainFsTerrainMask,
                [ground, mask],
                [w, c],
                &mut steps,
                BLENDED,
            )?;
        }
        // With the sun's shadow map, the near terrain's layers are drawn without fog; then each
        // square once more, multiplied by the shadow map and the clouds, then fogged. The remaster
        // lays white for the map: its own shadows come from the shared passes.
        if frame.shadow.is_some() && outdoors {
            for (fs, blend) in [
                (
                    Entry::TerrainFsTerrainShade,
                    (d3d::BLEND_DEST_COLOR, d3d::BLEND_ZERO),
                ),
                (Entry::TerrainFsTerrainFog, BLENDED),
            ] {
                for b in near.iter().filter(|b| b.layer == 0) {
                    let (w, c) = (self.wrap, self.clamp);
                    land(self, b, fs, [self.white, clouds], [c, w], &mut steps, blend)?;
                }
            }
        }
        for b in &far {
            let ground = self.picture(b.ground, load)?;
            let w = self.wrap;
            land(
                self,
                b,
                Entry::TerrainFsTerrainFar,
                [ground, clouds],
                [w, w],
                &mut steps,
                OPAQUE,
            )?;
        }
        // The land as the water's reflection shows it: all of it as the far landscape.
        let reflection = frame.reflection.filter(|_| !water.is_empty());
        let mut reflected_land = Steps::default();
        if let (Some(_), Some(l)) = (reflection, &frame.reflection_land) {
            let ground = self.picture(l.ground, load)?;
            let w = self.wrap;
            land(
                self,
                l,
                Entry::TerrainFsTerrainFar,
                [ground, clouds],
                [w, w],
                &mut reflected_land,
                OPAQUE,
            )?;
        }
        let world = |this: &mut Self,
                     pass: Pass,
                     records: &mut Vec<DrawUniforms>,
                     posed: &mut Vec<auberean_draw::mesh::PosedVertex>,
                     steps: &mut Steps,
                     load: &mut dyn FnMut(u32) -> Option<Image>|
         -> Result<(), RemasterError> {
            let batches: Vec<&ObjectBatch> = in_pass(&frame.objects, pass)
                .into_iter()
                .filter(shown)
                .collect();
            let batches = if pass == Pass::Streaks {
                batches
            } else {
                in_order(batches, pass, frame.camera.position)
            };
            objects(this, batches, Env::Own, pass, records, posed, steps, load)
        };
        for pass in [
            Pass::Cells,
            Pass::Opaque,
            Pass::Casters,
            Pass::Translucent,
            Pass::Frills,
            Pass::Particles,
            Pass::Streaks,
        ] {
            world(self, pass, &mut records, &mut posed, &mut steps, load)?;
        }
        objects(
            self,
            frame.decals.iter().collect(),
            Env::Own,
            Pass::Decals,
            &mut records,
            &mut posed,
            &mut steps,
            load,
        )?;
        // The water's own material, then the foam along the shores.
        // D2-10975, D2-10902, D2-10951
        objects(
            self,
            opaque_first(water),
            Env::Outside,
            Pass::Water,
            &mut records,
            &mut posed,
            &mut steps,
            load,
        )?;
        // The reflection laid over each water surface, after its own material, then the foam.
        // D2-10090
        let mut ripple_draws: Vec<Draw> = Vec::new();
        if let Some(r) = reflection {
            let (_, _, _, picture, ripples) = self.reflection_targets()?;
            let bump = self.bump_picture()?;
            let slide = {
                let s = 0.01 * frame.uptime;
                #[allow(clippy::cast_possible_truncation)]
                let f = (s - s.floor()) as f32;
                f
            };
            let mut surfaces = Vec::new();
            for b in water.iter().filter(|b| !b.indices.is_empty()) {
                let vertices = self.mesh_vertices(&b.vertices)?;
                let indices = self.index_list(&b.indices)?;
                let base = |this: &Self, key: Key, record: u32, textures: [u32; 3]| Draw {
                    key,
                    vertices,
                    posed: None,
                    indices,
                    index_type: IndexType::U16,
                    index_count: u32::try_from(b.indices.len()).unwrap_or(0),
                    record,
                    textures,
                    samplers: [this.clamp, this.clamp, this.wrap],
                    env: Env::Outside,
                    alpha_mode: ALPHA_KEEP,
                    shown: true,
                    reflected: false,
                    faint: None,
                };
                if r.ripples {
                    let mut u = draw_uniforms(&b.layer, &b.transform, b.tint, out_sun, &b.points);
                    u.uv_sets = [0, 0, 0, 0];
                    u.uv_rows[0] = [0.5, 0.0, 0.5 * slide, 0.0];
                    u.uv_rows[1] = [0.0, 0.5, 0.5 * slide, 0.0];
                    let at = u32::try_from(records.len()).unwrap_or(u32::MAX);
                    records.push(u);
                    let mut d = base(
                        self,
                        Key {
                            program: Program::Ripple,
                            src: d3d::BLEND_ONE,
                            dst: d3d::BLEND_ZERO,
                            depth_test: true,
                            depth_write: true,
                            cull: d3d::CULL_NONE,
                        },
                        at,
                        [bump, self.white, self.white],
                    );
                    d.samplers = [self.wrap, self.wrap, self.wrap];
                    ripple_draws.push(d);
                }
                let mut u = draw_uniforms(&b.layer, &b.transform, b.tint, out_sun, &b.points);
                u.params[2] = if r.ripples { RIPPLE_PUSH } else { 0.0 };
                let at = u32::try_from(records.len()).unwrap_or(u32::MAX);
                records.push(u);
                surfaces.push(base(
                    self,
                    Key {
                        program: Program::Water,
                        src: d3d::BLEND_DEST_ALPHA,
                        dst: d3d::BLEND_INV_DEST_ALPHA,
                        depth_test: true,
                        depth_write: false,
                        cull: b.layer.cull,
                    },
                    at,
                    [
                        picture,
                        if r.ripples { ripples } else { self.white },
                        self.white,
                    ],
                ));
            }
            for d in surfaces {
                steps.push(Pass::Water, d);
            }
        }
        objects(
            self,
            foam.iter().collect(),
            Env::Outside,
            Pass::Water,
            &mut records,
            &mut posed,
            &mut steps,
            load,
        )?;
        objects(
            self,
            opaque_first(sky_after),
            Env::Sky,
            Pass::LateSky,
            &mut records,
            &mut posed,
            &mut steps,
            load,
        )?;
        let outdoors_light = frame.outside.as_ref().unwrap_or(&frame.light);
        // D2-10936
        let skip_sky = (outdoors_light.fog_sky - 1.0).abs() < 0.0002;
        // The veil is the sky's: no sky drawn, no veil.
        // D2-10940
        let veil = (sky_shown && outdoors_light.fog_sky > 0.0)
            .then(|| {
                self.pipeline(Key {
                    program: Program::Veil,
                    src: d3d::BLEND_SRC_ALPHA,
                    dst: d3d::BLEND_INV_SRC_ALPHA,
                    depth_test: false,
                    depth_write: false,
                    cull: d3d::CULL_NONE,
                })
            })
            .transpose()?;
        let mut pipelines: HashMap<Key, Pipeline> = HashMap::new();
        let keys: Vec<Key> = steps
            .all()
            .chain(reflected_land.all())
            .chain(ripple_draws.iter())
            .flat_map(|d| std::iter::once(d.key).chain(d.faint.map(|f| f.0)))
            .collect();
        for key in keys {
            if let std::collections::hash_map::Entry::Vacant(e) = pipelines.entry(key) {
                e.insert(self.pipeline(key)?);
            }
        }

        // The frame's constants and poses.
        let mut bytes = Vec::with_capacity(records.len() * std::mem::size_of::<DrawUniforms>());
        for r in &records {
            bytes.extend_from_slice(bytemuck::bytes_of(r));
        }
        self.draws.write(&mut self.gfx, &bytes)?;
        self.posed.write(&mut self.gfx, bytes_of(&posed))?;
        // The lights seen from the camera mirrored about the water: its height mirrored, its pitch
        // turned over, the sky's objects moved round the mirrored camera.
        // D2-11030, D2-10552, D2-10553, D2-11031
        if let Some(r) = reflection {
            let h = r.height;
            let m_eye = [eye[0], eye[1], 2.0 * h - eye[2]];
            let m_forward = [forward[0], forward[1], -forward[2]];
            let m_vp = view_proj(
                m_eye,
                m_forward,
                -cam.roll,
                cam.fov_y,
                cam.orthographic,
                aspect,
                cam.near,
                cam.far,
            );
            let mirrored = |u: &FrameUniforms| {
                let mut m = *u;
                m.view_proj = mat(&m_vp);
                m.eye = [m_eye[0], m_eye[1], m_eye[2], 1.0];
                m.forward = [m_forward[0], m_forward[1], m_forward[2], 0.0];
                m
            };
            let lift = auberean_world::math::Mat4([
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, m_eye[2] - eye[2], 1.0],
            ]);
            let m_frame = mirrored(&frame_u);
            let mut m_sky = sky_u;
            m_sky.view_proj = mat(&lift.then(&m_vp));
            m_sky.eye = m_frame.eye;
            m_sky.forward = m_frame.forward;
            for (k, u) in [m_frame, mirrored(&out_u), m_sky, mirrored(&looked_u)]
                .iter()
                .enumerate()
            {
                let bytes = Self::light_rows(u, (REFLECTION_SIZE, REFLECTION_SIZE));
                self.mirrored[k].write(&mut self.gfx, &bytes)?;
            }
        }

        let t = self.targets(width, height)?;
        let index_of = |l: &[PerFrame; 4]| [l[0].index, l[1].index, l[2].index, l[3].index];
        let lights = index_of(&self.lights);
        let mirrored = index_of(&self.mirrored);
        let (draws, posed_index) = (self.draws.index, self.posed.index);
        let reflection_targets = match reflection {
            Some(_) => Some(self.reflection_targets()?),
            None => None,
        };
        // The world's frame starts black: the sky's dome adds itself onto it. Seen from inside
        // with a cell left out for the interior fog, the background is the fog's color, as
        // opaque as its alpha over that black.
        let backdrop = match &sight {
            Some(s) if s.fog => {
                let [r, g, b, a] = s.fog_color;
                [r * a, g * a, b * a, 1.0]
            }
            _ => [0.0, 0.0, 0.0, 1.0],
        };
        // One draw, under the lights `lights` (the frame's or the mirrored ones), as the
        // reflection draws it when `reflecting`.
        let run = |rec: &mut dereth_gfx::Draw<'_>,
                   d: &Draw,
                   lights: &[u32; 4],
                   reflecting: bool|
         -> Result<(), dereth_gfx::Error> {
            let (key, record) = match d.faint.filter(|_| reflecting) {
                Some(f) => f,
                None => (d.key, d.record),
            };
            rec.set_pipeline(pipelines[&key])?;
            let words = match key.program {
                Program::Mesh | Program::Water | Program::Ripple => push_words(&[
                    lights[d.env as usize],
                    draws,
                    record,
                    d.vertices,
                    if d.posed.is_some() {
                        posed_index
                    } else {
                        u32::MAX
                    },
                    d.posed.unwrap_or(0),
                    d.textures[0],
                    d.textures[1],
                    d.textures[2],
                    d.samplers[0],
                    d.samplers[1],
                    d.samplers[2],
                    u32::MAX,
                    d.alpha_mode,
                ]),
                _ => push_words(&[
                    lights[d.env as usize],
                    d.vertices,
                    d.textures[0],
                    d.samplers[0],
                    d.textures[1],
                    d.samplers[1],
                ]),
            };
            rec.push(0, &words)?;
            rec.bind_index_buffer(d.indices, 0, d.index_type)?;
            rec.draw_indexed(d.index_count, 1, 0, 0, 0);
            Ok(())
        };
        // The world's steps in their order, under `lights`; the reflection draws the land as it
        // shows it, and neither the water nor the particles and streaks.
        let steps_in = |rec: &mut dereth_gfx::Draw<'_>,
                        lights: &[u32; 4],
                        reflecting: bool|
         -> Result<(), dereth_gfx::Error> {
            for step in PASSES {
                if step == Pass::Veil {
                    if let Some(p) = veil {
                        rec.set_pipeline(p)?;
                        rec.push(0, &push_words(&[lights[Env::Outside as usize]]))?;
                        rec.draw(3, 1, 0, 0);
                    }
                    continue;
                }
                if step == Pass::Sky && skip_sky {
                    continue;
                }
                if reflecting && matches!(step, Pass::Water | Pass::Particles | Pass::Streaks) {
                    continue;
                }
                let list = match step {
                    Pass::Terrain if reflecting && reflected_land.get(step).is_some() => {
                        reflected_land.get(step)
                    }
                    _ => steps.get(step),
                };
                for d in list.unwrap_or(&[]) {
                    if if reflecting { d.reflected } else { d.shown } {
                        run(rec, d, lights, reflecting)?;
                    }
                }
            }
            Ok(())
        };
        let mut r = self.gfx.begin()?;
        if let Some((picture, ripples, depth, _, _)) = reflection_targets {
            r.raster(
                &RasterPass {
                    name: "remaster reflection",
                    color: &[ColorAttachment {
                        texture: picture,
                        load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                    }],
                    depth: Some(DepthAttachment {
                        texture: depth,
                        clear: Some(1.0),
                        write: true,
                    }),
                    touches: &[],
                    area: None,
                },
                |rec| steps_in(rec, &mirrored, true),
            )?;
            if !ripple_draws.is_empty() {
                r.raster(
                    &RasterPass {
                        name: "remaster ripples",
                        color: &[ColorAttachment {
                            texture: ripples,
                            load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                        }],
                        depth: Some(DepthAttachment {
                            texture: depth,
                            clear: Some(1.0),
                            write: true,
                        }),
                        touches: &[],
                        area: None,
                    },
                    |rec| {
                        for d in &ripple_draws {
                            run(rec, d, &lights, false)?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        let mut touches = Vec::new();
        if let Some((picture, ripples, _, _, _)) = reflection_targets {
            touches.push(Touch::Texture(picture, Use::Sampled));
            touches.push(Touch::Texture(ripples, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "remaster world",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Clear(backdrop),
                }],
                depth: Some(DepthAttachment {
                    texture: t.depth,
                    clear: Some(1.0),
                    write: true,
                }),
                touches: &touches,
                area: None,
            },
            |rec| steps_in(rec, &lights, false),
        )?;
        // Left ready to sample, for whoever shows the picture.
        r.touch(&[Touch::Texture(t.color, Use::Sampled)])?;
        r.submit()?;
        self.retire();
        Ok(t.color)
    }

    /// Let go of what no frame has drawn for [`KEEP_FRAMES`] frames, and of the lists only this
    /// holds, which no frame draws again.
    fn retire(&mut self) {
        let now = self.frame;
        let old = |used: u64| now.saturating_sub(used) > KEEP_FRAMES;
        for store in [&mut self.vertices, &mut self.indices] {
            let gone: Vec<(usize, usize)> = store
                .iter()
                .filter(|(_, k)| old(k.used) || Arc::strong_count(&k.list) == 1)
                .map(|(key, _)| *key)
                .collect();
            for key in gone {
                if let Some(k) = store.remove(&key) {
                    self.gfx.destroy_buffer(k.buffer);
                }
            }
        }
        let gone: Vec<u32> = self
            .pictures
            .iter()
            .filter(|(_, p)| old(p.2))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some((t, _, _)) = self.pictures.remove(&id) {
                self.gfx.destroy_texture(t);
            }
        }
    }
}

/// The frame's draws, step by step.
#[derive(Debug, Default)]
struct Steps(Vec<(Pass, Vec<Draw>)>);

impl Steps {
    fn push(&mut self, pass: Pass, d: Draw) {
        match self.0.iter_mut().find(|(p, _)| *p == pass) {
            Some((_, list)) => list.push(d),
            None => self.0.push((pass, vec![d])),
        }
    }

    fn get(&self, pass: Pass) -> Option<&[Draw]> {
        self.0
            .iter()
            .find(|(p, _)| *p == pass)
            .map(|(_, list)| list.as_slice())
    }

    fn all(&self) -> impl Iterator<Item = &Draw> {
        self.0.iter().flat_map(|(_, list)| list.iter())
    }
}

/// Every draw of `batches` in step `pass`.
fn in_pass(batches: &[ObjectBatch], pass: Pass) -> Vec<&ObjectBatch> {
    batches.iter().filter(|b| b.pass() == pass).collect()
}

/// Every draw of `batches` that are no objects of the world (the sky's, the water's), those that
/// mark how near they are first, each kind in its order.
fn opaque_first(batches: &[ObjectBatch]) -> Vec<&ObjectBatch> {
    let mut out: Vec<&ObjectBatch> = batches
        .iter()
        .filter(|b| b.by_layer() == Pass::Opaque)
        .collect();
    out.extend(batches.iter().filter(|b| b.by_layer() != Pass::Opaque));
    out
}
