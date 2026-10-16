//! An art pack's ground: a material for each retail terrain type, laid where retail lays that
//! type's picture, through retail's own blend masks, in place of the landscape's pictures.
//!
//! The landscape is drawn and re-shaded as it always is, and the landscape-normal pass marks
//! its pixels with the landscape's class. This pass runs over those pixels after it, before the
//! light per pixel: it finds each pixel on the ground from the depth, reads the layers of the cell
//! it lies in, and lays them again from the pack's materials, laid out in the broad patches of the
//! retail pictures they replace, with their heights deciding the edges between them close by,
//! detail near the eye and gentle variation far off (see the shader). It
//! writes the colour the light per pixel divides its old light out of and the normal the
//! materials bend, so the ground then takes the light, the shadows, the occlusion and the
//! bounced light as everything else does.
//!
//! Only pixels change. The landscape's shape, what can be picked and everything the game reads
//! are the ordinary frame's. Where a cell has no layers (a block the frame has not laid out yet),
//! the landscape keeps the pictures it was drawn with.
//!
//! **A cell's layers** are four words ([`cell_words`]): the ground under every overlay, then up
//! to [`CELL_OVERLAYS`] overlays, each a ground (a terrain type, or [`ROAD`]), the retail mask
//! it is blended in through and the mask's quarter turns; and how many cells across the cell of
//! the landscape as drawn is ([`cell_span`]). Far off the landscape is drawn in coarser cells,
//! two, four or eight cells across, and a coarse cell's layers are laid across all of it, as
//! retail lays them there: their masks stretched over it, softer and broader.

use glam::{Vec2, Vec3};

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::passes::lighting::legacy_light;
use crate::resources::{Resource, ResourceName, TextureSpec};
use crate::shared::camera::HEIGHT_UP;
use crate::shared::fullscreen::COLOUR_WGSL;
use crate::shared::pack_image;
use crate::shared::terrain_field::{BLOCK_CELLS, BLOCK_METRES};
use crate::snapshot::{HifiGround, HifiPackImage, HifiTerrainBlock};
use crate::{Caps, HifiError, HifiFrame, HifiSettings};

const GROUND_WGSL: &str = include_str!("ground.wgsl");

/// The pass's shader, for validation without a device.
#[must_use]
pub fn shader_source() -> String {
    format!(
        "{GROUND_WGSL}\n{COLOUR_WGSL}\n{}",
        crate::passes::water::SHORE_WGSL
    )
}

/// The most overlays a cell lays over its ground: three of terrain and two of road.
pub const CELL_OVERLAYS: usize = 5;
/// The road's ground, after the 32 terrain types.
pub const ROAD: u8 = 32;
/// The retail grounds: the terrain types and the road.
pub const GROUNDS: usize = 33;
/// The ground the materials' repeats are counted over, metres: 64 cells. Each material repeats
/// a whole number of times across it, so its repeats meet seamlessly at every cell.
pub const PERIOD_METRES: f32 = 1536.0;
/// How much finer the detail near the eye repeats than the material.
pub const DETAIL_REPEATS: u32 = 4;
/// About how much coarser the variation far off repeats than the material.
pub const MACRO_SPREAD: f32 = 3.3;
/// The share of the retail ground's broad patches kept far off, where retail's own picture
/// still shows them.
pub const BROAD_FAR_SHARE: f32 = 1.0;

/// The word bit of a cell some block lays.
const COVERED: u32 = 1 << 31;
/// The ground number of a layer with no ground.
const NO_GROUND: u32 = 63;

/// One overlay of a cell: a ground blended in through a retail mask at a quarter turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellLayer {
    /// The ground, a terrain type or [`ROAD`]; `None` where retail names none.
    pub ground: Option<u8>,
    /// The mask, by its index in [`HifiGround::masks`].
    pub mask: u8,
    /// The mask's quarter turns, 0 to 3.
    pub rotation: u8,
}

fn ground_bits(g: Option<u8>) -> u32 {
    g.map_or(NO_GROUND, |g| u32::from(g.min(62)))
}

/// The four words a cell's layers are read from: `base`, the ground under the overlays, and up
/// to [`CELL_OVERLAYS`] of `overlays`, in the order they are laid, over a drawn cell `span`
/// cells across (1, 2, 4 or 8; the nearest of them otherwise).
#[must_use]
pub fn cell_words(base: Option<u8>, overlays: &[CellLayer], span: u8) -> [u32; 4] {
    let n = overlays.len().min(CELL_OVERLAYS);
    let mut w = [0u32; 4];
    let span_bits = match span {
        0..=1 => 0,
        2..=3 => 1,
        4..=7 => 2,
        _ => 3,
    };
    // LINT-OK: at most five overlays.
    #[allow(clippy::cast_possible_truncation)]
    {
        w[0] = COVERED | ground_bits(base) | ((n as u32) << 8) | (span_bits << 12);
    }
    for (k, l) in overlays.iter().take(n).enumerate() {
        let bits = ground_bits(l.ground)
            | (u32::from(l.mask & 0x3F) << 6)
            | (u32::from(l.rotation & 3) << 12);
        w[1 + k / 2] |= bits << (16 * (k % 2));
    }
    w
}

/// The layers in `words`, as [`cell_words`] wrote them; `None` for a cell no block lays.
#[must_use]
pub fn cell_layers(words: [u32; 4]) -> Option<(Option<u8>, Vec<CellLayer>)> {
    if words[0] & COVERED == 0 {
        return None;
    }
    // LINT-OK: six-bit fields.
    #[allow(clippy::cast_possible_truncation)]
    let ground = |bits: u32| (bits != NO_GROUND).then_some(bits as u8);
    let count = ((words[0] >> 8) & 7).min(5) as usize;
    let overlays = (0..count)
        .map(|k| {
            let o = (words[1 + k / 2] >> (16 * (k % 2))) & 0xFFFF;
            // LINT-OK: six- and two-bit fields.
            #[allow(clippy::cast_possible_truncation)]
            CellLayer {
                ground: ground(o & 0x3F),
                mask: ((o >> 6) & 0x3F) as u8,
                rotation: ((o >> 12) & 3) as u8,
            }
        })
        .collect();
    Some((ground(words[0] & 0x3F), overlays))
}

/// How many cells across the drawn cell the layers in `words` are laid over: 1, 2, 4 or 8.
#[must_use]
pub fn cell_span(words: [u32; 4]) -> u8 {
    1 << ((words[0] >> 12) & 3)
}

/// How many times a material `size` metres a repeat repeats across the [`PERIOD_METRES`]: the
/// nearest whole number, at least one.
#[must_use]
pub fn repeats(size: f32) -> u32 {
    if !(size.is_finite() && size > 0.0) {
        return 1;
    }
    // LINT-OK: a positive count far inside u32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (PERIOD_METRES / size).round().max(1.0) as u32;
    n
}

/// The cells of the blocks that carry layers, one texel each, over the window they span.
#[derive(Debug, Clone, PartialEq)]
pub struct CellWindow {
    /// The block coordinates of the window's south-west block.
    pub first_block: (u8, u8),
    /// The render-space position of the south-west corner of texel (0, 0).
    pub origin: Vec2,
    /// Texels across (east) and up (north).
    pub size: (u32, u32),
    /// Each texel's words, row by row from the south: `north * width + east`.
    pub words: Vec<[u32; 4]>,
}

impl CellWindow {
    /// The window over the blocks of `blocks` that carry layers, or `None` when none does.
    #[must_use]
    pub fn build(blocks: &[HifiTerrainBlock]) -> Option<Self> {
        let laid: Vec<&HifiTerrainBlock> = blocks.iter().filter(|b| b.layers.is_some()).collect();
        // LINT-OK: a block id's two bytes.
        #[allow(clippy::cast_possible_truncation)]
        let coords = |b: &HifiTerrainBlock| ((b.block >> 8) as u8, (b.block & 0xFF) as u8);
        let x0 = laid.iter().map(|b| coords(b).0).min()?;
        let y0 = laid.iter().map(|b| coords(b).1).min()?;
        let x1 = laid.iter().map(|b| coords(b).0).max()?;
        let y1 = laid.iter().map(|b| coords(b).1).max()?;
        let width = (usize::from(x1 - x0) + 1) * BLOCK_CELLS;
        let height = (usize::from(y1 - y0) + 1) * BLOCK_CELLS;
        let any = laid.first()?;
        let (ax, ay) = coords(any);
        let origin = Vec2::new(
            any.origin.x - f32::from(ax - x0) * BLOCK_METRES,
            any.origin.y - f32::from(ay - y0) * BLOCK_METRES,
        );
        let mut words = vec![[0u32; 4]; width * height];
        for b in &laid {
            let Some(layers) = b.layers.as_ref() else {
                continue;
            };
            let (bx, by) = coords(b);
            let (ox, oy) = (
                usize::from(bx - x0) * BLOCK_CELLS,
                usize::from(by - y0) * BLOCK_CELLS,
            );
            for e in 0..BLOCK_CELLS {
                for n in 0..BLOCK_CELLS {
                    words[(oy + n) * width + ox + e] = layers[e * BLOCK_CELLS + n];
                }
            }
        }
        Some(Self {
            first_block: (x0, y0),
            origin,
            size: (u32::try_from(width).ok()?, u32::try_from(height).ok()?),
            words,
        })
    }

    /// The global cell coordinates of texel (0, 0).
    #[must_use]
    pub fn first_cell(&self) -> (i32, i32) {
        (
            i32::from(self.first_block.0) * 8,
            i32::from(self.first_block.1) * 8,
        )
    }

    /// The texels' bytes, `rgba32uint`.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.words
            .iter()
            .flatten()
            .flat_map(|w| w.to_le_bytes())
            .collect()
    }
}

/// What the cell texture was made from: each laid block's id, generation, layers and origin.
type CellSignature = Vec<(u16, u32, usize, [u32; 2])>;

fn cell_signature(blocks: &[HifiTerrainBlock]) -> CellSignature {
    let mut s: CellSignature = blocks
        .iter()
        .filter_map(|b| {
            let layers = b.layers.as_ref()?;
            Some((
                b.block,
                b.generation,
                std::sync::Arc::as_ptr(layers) as usize,
                [b.origin.x.to_bits(), b.origin.y.to_bits()],
            ))
        })
        .collect();
    s.sort_unstable();
    s
}

/// Where the shader's table holds each ground's material for its steep slopes: that many rows
/// past the ground's own.
pub const STEEP_ROWS: usize = 64;
/// The rows of the shader's table: the retail grounds, then their steep materials.
pub const TABLE_ROWS: usize = STEEP_ROWS + GROUNDS;

/// The two vectors of each row of the shader's table (see `grounds` in the shader): each retail
/// ground's material, then from [`STEEP_ROWS`] each ground's material for its steep slopes.
#[must_use]
pub fn ground_table(ground: &HifiGround) -> Vec<f32> {
    let mut t = Vec::with_capacity(TABLE_ROWS * 8);
    for row in 0..TABLE_ROWS {
        let (g, of) = if row >= STEEP_ROWS {
            (row - STEEP_ROWS, &ground.steep_of)
        } else {
            (row, &ground.of_ground)
        };
        // The ground's broad picture's repeats across a cell; nought where it has none.
        // LINT-OK: a handful of repeats.
        #[allow(clippy::cast_precision_loss)]
        let broad = ground.broad.get(g).map_or(0.0, |(_, k)| *k as f32);
        let m = of
            .get(g)
            .copied()
            .flatten()
            .filter(|_| g < GROUNDS)
            .and_then(|i| ground.materials.get(usize::from(i)).map(|m| (i, m)));
        match m {
            Some((i, m)) => {
                let n = repeats(m.size);
                // LINT-OK: repeat counts and indices far inside f32's exact range.
                #[allow(clippy::cast_precision_loss)]
                {
                    let tiling = (crate::shared::terrain_field::CELL_METRES / m.size).max(1.0);
                    t.extend([
                        f32::from(i),
                        f32::from(u8::from(m.retail)),
                        tiling,
                        m.normal_scale,
                    ]);
                    let coarse = (n as f32 / MACRO_SPREAD).round().max(1.0);
                    t.extend([n as f32, (n * DETAIL_REPEATS) as f32, coarse, broad]);
                }
            }
            None => {
                t.extend([-1.0, 0.0, 1.0, 1.0]);
                t.extend([1.0, 1.0, 1.0, broad]);
            }
        }
    }
    t
}

/// How the ground is tuned: the detail near the eye, the variation far off, how much the
/// materials' heights move the edges between them, each from the environment where it is set
/// (`DERETH_HIFI_GROUND_DETAIL`, `DERETH_HIFI_GROUND_VARIATION`, `DERETH_HIFI_GROUND_HEIGHTS`),
/// and the layers view (`DERETH_HIFI_GROUND_LAYERS=1`); then how strongly the retail ground's
/// broad patches lay out the materials (`DERETH_HIFI_GROUND_BROAD`), and the share of that they
/// keep far off (`DERETH_HIFI_GROUND_BROAD_FAR`).
fn tune() -> [f32; 8] {
    use crate::passes::lighting::env_f32;
    [
        env_f32("DERETH_HIFI_GROUND_DETAIL", 1.0),
        env_f32("DERETH_HIFI_GROUND_VARIATION", 1.0),
        env_f32("DERETH_HIFI_GROUND_HEIGHTS", 1.0),
        env_f32("DERETH_HIFI_GROUND_LAYERS", 0.0),
        env_f32("DERETH_HIFI_GROUND_BROAD", 1.0),
        env_f32("DERETH_HIFI_GROUND_BROAD_FAR", BROAD_FAR_SHARE),
        0.0,
        0.0,
    ]
}

/// The frame's constants, as the shader's `Params` lays them out.
fn params(
    frame: &HifiFrame,
    settings: &HifiSettings,
    viewport: [f32; 4],
    fogged: bool,
    window: &CellWindow,
) -> Vec<f32> {
    let cam = &frame.camera;
    let clip_from_render = cam.jittered(cam.projection * cam.view * HEIGHT_UP);
    let (sun, amb) = legacy_light(frame);
    let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let mut p = Vec::with_capacity(PARAM_FLOATS);
    p.extend(clip_from_render.inverse().to_cols_array());
    p.extend((cam.view * HEIGHT_UP).to_cols_array());
    p.extend(viewport);
    p.extend([cam.eye.x, cam.eye.y, cam.eye.z, 0.0]);
    p.extend([l.x, l.y, l.z, 0.0]);
    p.extend([sun[0], sun[1], sun[2], 0.0]);
    p.extend([amb[0], amb[1], amb[2], 0.0]);
    match frame.sky.fog.filter(|_| fogged) {
        Some(fog) => {
            p.extend([fog.min, fog.max, 1.0, 0.0]);
            p.extend(fog.color.map(|c| f32::from(c) / 255.0));
            p.push(0.0);
        }
        None => p.extend([0.0; 8]),
    }
    // LINT-OK: texel counts and cell coordinates far inside f32's exact range.
    #[allow(clippy::cast_precision_loss)]
    {
        p.extend([
            window.origin.x,
            window.origin.y,
            window.size.0 as f32,
            window.size.1 as f32,
        ]);
        let (cx, cy) = window.first_cell();
        p.extend([cx as f32, cy as f32, 0.0, 0.0]);
    }
    p.extend(tune());
    // The drawn ground: where render space lies in the world, and the scree and rock laid by its
    // slope, unless they are turned off.
    match frame.drawn.as_ref() {
        Some(drawn) => {
            p.extend([1.0, drawn.global.x, drawn.global.y, 0.0]);
            match drawn.knobs.slope.filter(|_| settings.slope_materials) {
                Some(s) => {
                    p.extend([s.scree.0, s.scree.1, s.rock.0, s.rock.1]);
                    p.extend([
                        s.strength,
                        f32::from(s.grounds.0),
                        f32::from(s.grounds.1),
                        s.smooth,
                    ]);
                    p.extend([s.on_snow, 0.0, 0.0, 0.0]);
                }
                None => p.extend([0.0; 12]),
            }
            // The forest cover: how strongly it tints the far ground (nought while there is none),
            // where it comes in, and how much its crowns shade the ground past the sun's shadows.
            let (from, to) = crate::passes::drawn::cover_band(&frame.far);
            let strength = if drawn.cover.is_some() {
                frame.far.cover
            } else {
                0.0
            };
            p.extend([strength, from, to, FLOOR_SHADE * strength.min(1.0)]);
        }
        None => p.extend([0.0; 20]),
    }
    // The water, while it is drawn as water: its bed's ground, how dark its wet bank is, and the
    // grass grown in over a muddy bank.
    match frame.drawn.as_ref().and_then(|d| d.knobs.water) {
        Some(w) => {
            p.extend([1.0, f32::from(w.bed.min(31)), w.shore_wet, w.bank_green]);
            p.extend([f32::from(w.bank_grass.min(31)), 0.0, 0.0, 0.0]);
        }
        None => p.extend([0.0; 8]),
    }
    p
}

/// How much of the light the crowns of the forest cover keep off the ground under them, past
/// the reach of the sun's shadows: there their trees cast none.
const FLOOR_SHADE: f32 = 0.45;

/// The floats of the shader's `Params`.
const PARAM_FLOATS: usize = 104;

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// `images`, all one size and format, as the layers of one array texture with their levels: the
/// first image's size and format, the fewest levels any has. An image of another size or format
/// is left unwritten.
fn upload_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    images: &[&HifiPackImage],
) -> wgpu::Texture {
    let blocks = pack_image::samples_blocks(device);
    let first = images.first().copied();
    let (w, h) = first.map_or((1, 1), |i| (i.width.max(1), i.height.max(1)));
    let (srgb, kind) = first.map_or((false, None), |i| (i.srgb, i.format));
    let levels = images
        .iter()
        .map(|i| i.levels.len())
        .min()
        .unwrap_or(1)
        .max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("art pack ground"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: u32::try_from(images.len().max(1)).unwrap_or(1),
        },
        mip_level_count: u32::try_from(levels).unwrap_or(1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: first.map_or(wgpu::TextureFormat::Rgba8Unorm, |i| {
            pack_image::format(i, blocks)
        }),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, image) in (0u32..).zip(images) {
        if (image.width, image.height, image.srgb, image.format) != (w, h, srgb, kind) {
            continue;
        }
        pack_image::write(queue, &texture, layer, image, levels);
    }
    texture
}

fn array_view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..wgpu::TextureViewDescriptor::default()
    })
}

/// The pass's pipeline and samplers, made once.
#[derive(Debug)]
struct Pipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    wrap: wgpu::Sampler,
    clamp: wgpu::Sampler,
    params: wgpu::Buffer,
    /// One terrain word, bound where the world's are not.
    no_words: wgpu::TextureView,
    /// One height, likewise.
    no_heights: wgpu::TextureView,
    /// One vertex's water, likewise.
    no_water: wgpu::TextureView,
    /// No forest cover, likewise.
    no_cover: wgpu::TextureView,
}

/// A ground's materials and masks on the device, with its table.
#[derive(Debug)]
struct Materials {
    serial: u64,
    albedo: wgpu::TextureView,
    normal: wgpu::TextureView,
    surface: wgpu::TextureView,
    masks: wgpu::TextureView,
    broad: wgpu::TextureView,
    table: wgpu::Buffer,
    bytes: u64,
}

/// The cell texture over the window, with what it was made from.
#[derive(Debug)]
struct Cells {
    signature: CellSignature,
    window: CellWindow,
    view: wgpu::TextureView,
}

/// The art pack's ground pass.
#[derive(Debug, Default)]
pub struct Ground {
    pipeline: Option<Pipeline>,
    materials: Option<Materials>,
    cells: Option<Cells>,
    /// Whether the ground was laid in the last frame.
    laid: bool,
}

/// The pass.
#[must_use]
pub fn pass() -> Ground {
    Ground::default()
}

fn layout_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn texture(
    sample_type: wgpu::TextureSampleType,
    dim: wgpu::TextureViewDimension,
) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type,
        view_dimension: dim,
        multisampled: false,
    }
}

fn make_pipeline(device: &wgpu::Device) -> Pipeline {
    use wgpu::TextureSampleType as S;
    use wgpu::TextureViewDimension as D;
    let filtered = S::Float { filterable: true };
    let uniform = wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    };
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("art pack ground"),
        entries: &[
            layout_entry(0, texture(S::Depth, D::D2)),
            layout_entry(1, texture(S::Float { filterable: false }, D::D2)),
            layout_entry(2, texture(S::Uint, D::D2)),
            layout_entry(3, texture(filtered, D::D2Array)),
            layout_entry(4, texture(filtered, D::D2Array)),
            layout_entry(5, texture(filtered, D::D2Array)),
            layout_entry(
                6,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            layout_entry(
                7,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
            layout_entry(8, uniform),
            layout_entry(9, uniform),
            layout_entry(10, texture(filtered, D::D2Array)),
            layout_entry(11, texture(filtered, D::D2Array)),
            layout_entry(12, texture(S::Uint, D::D2)),
            layout_entry(13, texture(S::Float { filterable: false }, D::D2)),
            layout_entry(14, texture(S::Float { filterable: false }, D::D2)),
            layout_entry(15, texture(S::Float { filterable: false }, D::D2)),
            layout_entry(16, texture(filtered, D::D2)),
        ],
    });
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("art pack ground"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("art pack ground"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("art pack ground"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_ground"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::HDR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::COLOR,
                }),
                Some(wgpu::ColorTargetState {
                    format: crate::derive::pipeline::NORMAL_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
            ],
        }),
        multiview_mask: None,
        cache: None,
    });
    let sampler = |address: wgpu::AddressMode, anisotropy: u16| {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("art pack ground"),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: anisotropy,
            ..wgpu::SamplerDescriptor::default()
        })
    };
    Pipeline {
        layout,
        pipeline,
        wrap: sampler(wgpu::AddressMode::Repeat, 16),
        clamp: sampler(wgpu::AddressMode::ClampToEdge, 4),
        params: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("art pack ground"),
            size: (PARAM_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        no_words: device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("art pack ground, no words"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Uint,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
        no_heights: device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("art pack ground, no heights"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
        no_water: device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("art pack ground, no water"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
        no_cover: device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("art pack ground, no forest cover"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default()),
    }
}

fn make_materials(device: &wgpu::Device, queue: &wgpu::Queue, ground: &HifiGround) -> Materials {
    let albedo: Vec<&HifiPackImage> = ground.materials.iter().map(|m| &m.albedo).collect();
    let normal: Vec<&HifiPackImage> = ground.materials.iter().map(|m| &m.normal).collect();
    let surface: Vec<&HifiPackImage> = ground.materials.iter().map(|m| &m.surface).collect();
    let masks: Vec<&HifiPackImage> = ground.masks.iter().collect();
    let grey = HifiPackImage {
        width: 1,
        height: 1,
        srgb: true,
        levels: vec![std::sync::Arc::new(vec![128, 128, 128, 255])],
        format: None,
    };
    let mut broad: Vec<&HifiPackImage> = ground.broad.iter().map(|(i, _)| i).collect();
    if broad.is_empty() {
        broad.push(&grey);
    }
    let blocks = pack_image::samples_blocks(device);
    let bytes = |images: &[&HifiPackImage]| -> u64 {
        images
            .iter()
            .map(|i| pack_image::device_bytes(i, blocks))
            .sum()
    };
    let table = floats(&ground_table(ground));
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("art pack ground"),
        size: table.len() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, &table);
    let array = |images: &[&HifiPackImage]| array_view(&upload_array(device, queue, images));
    Materials {
        serial: ground.serial,
        albedo: array(&albedo),
        normal: array(&normal),
        surface: array(&surface),
        masks: array(&masks),
        table: buffer,
        broad: array(&broad),
        bytes: bytes(&albedo) + bytes(&normal) + bytes(&surface) + bytes(&masks) + bytes(&broad),
    }
}

fn make_cells(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window: &CellWindow,
) -> wgpu::TextureView {
    let (w, h) = window.size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("art pack ground cells"),
        size: wgpu::Extent3d {
            width: w.max(1),
            height: h.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Uint,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &window.bytes(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 16),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Whether the world is replayed with its authored fog: not under the physical sky outdoors,
/// which replays it without.
fn fogged(settings: &HifiSettings, frame: &HifiFrame) -> bool {
    !(settings.sky.is_on() && frame.sky.outdoor)
}

impl HifiPass for Ground {
    fn name(&self) -> &'static str {
        "art pack ground"
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        notes.push(("art pack ground laid", u64::from(self.laid)));
        notes.push((
            "art pack ground bytes",
            self.materials.as_ref().map_or(0, |m| m.bytes),
        ));
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        s.art_pack
            && s.lighting.is_on()
            && f.ground.as_ref().is_some_and(|g| !g.materials.is_empty())
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        self.laid = false;
        let Some(ground) = cx.frame.ground.clone() else {
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        if self.pipeline.is_none() {
            self.pipeline = Some(make_pipeline(&device));
        }
        if self
            .materials
            .as_ref()
            .is_none_or(|m| m.serial != ground.serial)
        {
            self.materials = Some(make_materials(&device, &queue, &ground));
        }
        let signature = cell_signature(&cx.frame.terrain);
        if self.cells.as_ref().is_none_or(|c| c.signature != signature) {
            self.cells = CellWindow::build(&cx.frame.terrain).map(|window| Cells {
                view: make_cells(&device, &queue, &window),
                signature,
                window,
            });
        }
        if let (Some(p), Some(c)) = (self.pipeline.as_ref(), self.cells.as_ref()) {
            #[allow(clippy::cast_precision_loss)] // pixel extents
            let viewport = cx.seam.world_viewport().map_or_else(
                || {
                    let (w, h) = cx.seam.surface_size();
                    [0.0, 0.0, w as f32, h as f32]
                },
                |v| [v.x as f32, v.y as f32, v.width as f32, v.height as f32],
            );
            let values = params(
                cx.frame,
                cx.settings,
                viewport,
                fogged(cx.settings, cx.frame),
                &c.window,
            );
            queue.write_buffer(&p.params, 0, &floats(&values));
        }
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let (Some(p), Some(m), Some(c), Some(rt)) = (
            self.pipeline.as_ref(),
            self.materials.as_ref(),
            self.cells.as_ref(),
            cx.reshade,
        ) else {
            return Ok(());
        };
        // The normals as they stand, read while the pass writes them.
        let (width, height) = cx.seam.surface_size();
        let prior = cx.resources.texture(
            cx.seam.device(),
            ResourceName::ScratchNormal,
            TextureSpec {
                width,
                height,
                format: crate::derive::pipeline::NORMAL_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            },
        )?;
        let (Some(Resource::Texture(from, _)), Some(Resource::Texture(to, _))) = (
            cx.resources.get(ResourceName::ViewNormal),
            cx.resources.get(ResourceName::ScratchNormal),
        ) else {
            return Ok(());
        };
        let words = match (
            cx.frame.drawn.as_ref(),
            cx.resources.get(ResourceName::DrawnGroundWords),
        ) {
            (Some(_), Some(Resource::Texture(_, view))) => view.clone(),
            _ => p.no_words.clone(),
        };
        let heights = match (
            cx.frame.drawn.as_ref(),
            cx.resources.get(ResourceName::DrawnGroundHeights),
        ) {
            (Some(_), Some(Resource::Texture(_, view))) => view.clone(),
            _ => p.no_heights.clone(),
        };
        let cover = match (
            cx.frame.drawn.as_ref(),
            cx.resources.get(ResourceName::ForestCover),
        ) {
            (Some(_), Some(Resource::Texture(_, view))) => view.clone(),
            _ => p.no_cover.clone(),
        };
        let water = match (
            cx.frame.drawn.as_ref(),
            cx.resources.get(ResourceName::DrawnGroundWater),
        ) {
            (Some(_), Some(Resource::Texture(_, view))) => view.clone(),
            _ => p.no_water.clone(),
        };
        cx.encoder.copy_texture_to_texture(
            from.as_image_copy(),
            to.as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        // While the presentation draws the ground itself, its pixels hold their distance from the
        // eye: a copy of the picture to read them from as the pass draws over it.
        let places = if cx.frame.drawn.is_some() {
            crate::reshade::hdr_copy(cx.seam, cx.resources, cx.encoder)?
        } else {
            prior.clone()
        };
        let bind = cx
            .seam
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("art pack ground"),
                layout: &p.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(cx.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&prior),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&c.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&m.masks),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&m.albedo),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&m.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&p.wrap),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::Sampler(&p.clamp),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: p.params.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: m.table.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&m.broad),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::TextureView(&m.surface),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(&words),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: wgpu::BindingResource::TextureView(&heights),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&places),
                    },
                    wgpu::BindGroupEntry {
                        binding: 15,
                        resource: wgpu::BindingResource::TextureView(&water),
                    },
                    wgpu::BindGroupEntry {
                        binding: 16,
                        resource: wgpu::BindingResource::TextureView(&cover),
                    },
                ],
            });
        let load = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("art pack ground"),
            color_attachments: &[load(&rt.hdr), load(&rt.normal)],
            depth_stencil_attachment: None,
            timestamp_writes: cx.timer.writes("art pack ground"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.seam.count_pass();
        if let Some(v) = cx.seam.world_viewport() {
            #[allow(clippy::cast_precision_loss)] // pixel extents
            pass.set_viewport(
                v.x as f32,
                v.y as f32,
                v.width as f32,
                v.height as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(v.x, v.y, v.width, v.height);
        }
        pass.set_pipeline(&p.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        self.laid = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;
    use crate::snapshot::HifiGroundMaterial;
    use std::sync::Arc;

    #[test]
    fn the_ground_shader_parses_and_validates_with_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(entries, ["vs", "fs_ground"]);
    }

    #[test]
    fn a_cells_layers_read_back_as_they_were_written() {
        let overlays = [
            CellLayer {
                ground: Some(9),
                mask: 3,
                rotation: 2,
            },
            CellLayer {
                ground: None,
                mask: 4,
                rotation: 1,
            },
            CellLayer {
                ground: Some(ROAD),
                mask: 7,
                rotation: 3,
            },
            CellLayer {
                ground: Some(0),
                mask: 0,
                rotation: 0,
            },
            CellLayer {
                ground: Some(31),
                mask: 63,
                rotation: 1,
            },
        ];
        for n in 0..=CELL_OVERLAYS {
            let words = cell_words(Some(3), &overlays[..n], 1);
            assert_eq!(cell_layers(words), Some((Some(3), overlays[..n].to_vec())));
        }
        assert_eq!(
            cell_layers(cell_words(None, &[], 1)),
            Some((None, Vec::new()))
        );
        assert_eq!(cell_layers([0; 4]), None, "a cell no block lays");
        // The drawn cell's span is kept beside the layers, and leaves them as they were.
        for (span, kept) in [(1, 1), (2, 2), (4, 4), (8, 8), (3, 2), (16, 8)] {
            let words = cell_words(Some(3), &overlays, span);
            assert_eq!(cell_span(words), kept, "{span}");
            assert_eq!(cell_layers(words), Some((Some(3), overlays.to_vec())));
        }
    }

    #[test]
    fn a_material_repeats_a_whole_number_of_times_across_the_period() {
        assert_eq!(repeats(4.0), 384);
        assert_eq!(repeats(6.0), 256);
        // 1536 over 7 is 219.4: the nearest whole number, so the repeat is 7.01 metres.
        assert_eq!(repeats(7.0), 219);
        assert_eq!(repeats(5000.0), 1);
        assert_eq!(repeats(0.0), 1);
        assert_eq!(repeats(f32::NAN), 1);
    }

    fn block(id: u16, origin: Vec2, first: u32) -> HifiTerrainBlock {
        let mut layers = [[0u32; 4]; 64];
        for (k, l) in layers.iter_mut().enumerate() {
            // LINT-OK: 64 cells.
            #[allow(clippy::cast_possible_truncation)]
            {
                *l = [COVERED | first, k as u32, 0, 0];
            }
        }
        HifiTerrainBlock {
            block: id,
            origin: origin.extend(0.0),
            layers: Some(Arc::new(layers)),
            ..HifiTerrainBlock::default()
        }
    }

    #[test]
    fn the_cell_window_lays_each_blocks_cells_east_by_north_from_its_south_west_corner() {
        let blocks = vec![
            block(0x4142, Vec2::new(BLOCK_METRES, 2.0 * BLOCK_METRES), 1),
            block(0x4041, Vec2::new(0.0, BLOCK_METRES), 2),
            // No layers: not in the window.
            HifiTerrainBlock {
                block: 0x3F40,
                ..HifiTerrainBlock::default()
            },
        ];
        let w = CellWindow::build(&blocks).expect("a window");
        assert_eq!(w.first_block, (0x40, 0x41));
        assert_eq!(w.size, (16, 16));
        assert_eq!(w.origin, Vec2::new(0.0, BLOCK_METRES));
        assert_eq!(w.first_cell(), (0x40 * 8, 0x41 * 8));
        // Block 0x4041's cell 3 east, 5 north.
        assert_eq!(w.words[5 * 16 + 3], [COVERED | 2, 3 * 8 + 5, 0, 0]);
        // Block 0x4142's cell 0, 0 is texel (8, 8).
        assert_eq!(w.words[8 * 16 + 8], [COVERED | 1, 0, 0, 0]);
        // The window's other corners are laid by no block.
        assert_eq!(w.words[15 * 16], [0; 4]);
        assert_eq!(w.bytes().len(), 16 * 16 * 16);
        assert!(CellWindow::build(&blocks[2..]).is_none());
    }

    #[test]
    fn the_table_names_each_grounds_material_and_its_repeats_and_none_for_the_rest() {
        let mut ground = HifiGround {
            serial: 1,
            materials: vec![
                HifiGroundMaterial {
                    size: 4.0,
                    normal_scale: 0.8,
                    ..HifiGroundMaterial::default()
                },
                HifiGroundMaterial {
                    size: 12.0,
                    normal_scale: 1.0,
                    retail: true,
                    ..HifiGroundMaterial::default()
                },
            ],
            ..HifiGround::default()
        };
        ground.of_ground = vec![None; GROUNDS];
        ground.of_ground[1] = Some(0);
        ground.of_ground[usize::from(ROAD)] = Some(1);
        let t = ground_table(&ground);
        assert_eq!(t.len(), TABLE_ROWS * 8);
        let row = |g: usize| &t[g * 8..g * 8 + 8];
        assert_eq!(row(1), [0.0, 0.0, 6.0, 0.8, 384.0, 1536.0, 116.0, 0.0]);
        // With the retail ground's broad pictures, each ground's repeats.
        ground.broad = vec![(HifiPackImage::default(), 2); GROUNDS];
        assert_eq!(ground_table(&ground)[8 + 7], 2.0);
        // Retail's own picture, two repeats to a cell.
        assert_eq!(&row(usize::from(ROAD))[..4], [1.0, 1.0, 2.0, 1.0]);
        assert!(row(0)[0] < 0.0, "a ground with no material");
        // A ground's material for its steep slopes, that many rows on; none for the rest.
        assert!((0..TABLE_ROWS).skip(GROUNDS).all(|r| row(r)[0] < 0.0));
        ground.steep_of = vec![None; GROUNDS];
        ground.steep_of[1] = Some(1);
        let t = ground_table(&ground);
        let row = |g: usize| &t[g * 8..g * 8 + 8];
        assert_eq!(row(STEEP_ROWS + 1)[0], 1.0);
        assert!(row(1)[0].abs() < 1e-6, "the flat ground keeps its own");
        assert!(row(STEEP_ROWS + 2)[0] < 0.0);
    }
}
