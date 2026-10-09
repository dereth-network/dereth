//! A physical sky and aerial perspective, calibrated to the authored fog and sky colours.
//!
//! The sky is a model of the Earth's air: Rayleigh scattering by the air itself, Mie scattering
//! by haze and absorption by ozone, drawn from four lookup tables in the manner of the
//! production sky models (transmittance, multiple scattering, and the sky as seen from the eye).
//! The tables are drawn again only when the weather, the sun or the eye's height moves enough to
//! show.
//!
//! **The sky.** In the sky pass before the landscape, once the authored dome has been drawn,
//! the physical sky is drawn over it, as opaque as the day is bright: by day it covers most of
//! the dome, whose own cast still shows through it a little, at night the authored dark sky
//! shows, and at dawn and dusk the two blend. The star field, the sun, the moons, the clouds and
//! the weather layer are drawn after it exactly as they always are, so they keep their places
//! and their look. Once they are drawn, and before the land, the haze's veil is drawn over the
//! horizon, so the sky meets the land in the haze's colour; whatever the world draws after it,
//! drawn over the sky or not, is never veiled as sky.
//!
//! **The air.** The world is replayed without the authored linear fog, through copies of the
//! frame's constant blocks with the fog switched off (the recorded blocks are never changed).
//! Then every pixel of land is seen through two layers of air: the atmosphere's own, which
//! tints the distance toward the sky's colour at the horizon in that direction, and a low haze
//! calibrated to the authored fog's range and the day's weather, thickest over low ground, lit
//! by the same light as the horizon's veil. Near the edge of the resident landscape the land
//! fades further into the haze by how near it lies to that edge, so the end of the world is
//! softened against the sky the same way wherever the eye stands.
//!
//! Interiors, and frames split between an interior and the outdoors, are drawn as they always
//! are: the sky and the air belong to the outdoors only.
//!
//! The work is three passes sharing one state: the tables, before anything is drawn; the sky and
//! the unfogged replay, acting on the world replay; and the air, over the replayed world. The
//! tables are published to the shared resources for the passes that light or reflect the sky.

pub mod model;

use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::resources::{Resource, ResourceName, TextureSpec};
use crate::shared::fullscreen::{COLOUR_WGSL, FULLSCREEN_WGSL};
use crate::{
    Caps, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, Level, Mark, ReplayFilter,
    SidecarContext, SkipRule,
};
use model::{SkyViewKey, TablesKey};

pub(crate) const ATMOSPHERE_WGSL: &str = include_str!("atmosphere.wgsl");
const TABLES_WGSL: &str = include_str!("tables.wgsl");
const SKY_WGSL: &str = include_str!("sky.wgsl");
const AERIAL_WGSL: &str = include_str!("aerial.wgsl");
const AIR_WGSL: &str = include_str!("air.wgsl");

/// The transmittance table's size.
const TRANSMITTANCE_SIZE: (u32, u32) = (256, 64);
/// The multiple-scattering table's size.
const MULTIPLE_SIZE: (u32, u32) = (32, 32);
/// The tables' texel format.
const TABLE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Below this share of the frame the physical sky is not drawn at all.
const LEAST_WEIGHT: f32 = 1.0 / 512.0;
/// From this share of the frame up the authored dome is left out under the physical sky.
const WHOLE_WEIGHT: f32 = 0.998;

/// The shaders' sources, each complete: the full-target triangle, the colour conversions, the
/// shared atmosphere, for the sky and the air the light of the air they share, and the shader's
/// own code. In the order tables, sky, air.
#[must_use]
pub fn shader_sources() -> [(&'static str, String); 3] {
    let with = |own: &[&str]| {
        [FULLSCREEN_WGSL, COLOUR_WGSL, ATMOSPHERE_WGSL]
            .iter()
            .chain(own)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    };
    [
        ("tables", with(&[TABLES_WGSL])),
        ("sky", with(&[AIR_WGSL, SKY_WGSL])),
        ("aerial", with(&[AIR_WGSL, AERIAL_WGSL])),
    ]
}

/// The air's shader for the re-shaded picture: the same air over linear light, unclamped, with
/// no encoding on the way in or out.
#[must_use]
pub fn aerial_hdr_source() -> String {
    let [_, _, (_, src)] = shader_sources();
    src.replace("let lin = srgb_to_linear(c.rgb);", "let lin = c.rgb;")
        .replace(
            "let s = mix(linear_to_srgb(max(haze, vec3<f32>(0.0))), linear_to_srgb(max(out, vec3<f32>(0.0))), edge);",
            "let s = mix(max(haze, vec3<f32>(0.0)), max(out, vec3<f32>(0.0)), edge);",
        )
        .replace(
            "return vec4<f32>(clamp(d, vec3<f32>(0.0), vec3<f32>(1.0)), c.a);",
            "return vec4<f32>(max(s, vec3<f32>(0.0)), c.a);",
        )
}

/// The three passes, sharing their state: the tables, the sky in the world replay, and the air.
#[must_use]
pub fn passes() -> (SkyTables, Sky, Atmosphere) {
    let shared = Rc::new(RefCell::new(Shared::default()));
    (
        SkyTables(Rc::clone(&shared)),
        Sky(Rc::clone(&shared)),
        Atmosphere(shared),
    )
}

/// Whether the sky and the air are wanted at all: their option is on and the viewer is
/// outdoors.
fn wanted(s: &HifiSettings, f: &HifiFrame) -> bool {
    s.sky.is_on() && f.sky.outdoor
}

/// What the three passes share.
#[derive(Debug, Default)]
struct Shared {
    /// Whether the sky and the air act on this frame.
    active: bool,
    /// The level in force.
    level: Level,
    /// How opaque the physical sky is drawn over the authored dome this frame.
    weight: f32,
    /// Each recorded per-frame block with its fog on, and the copy of it with the fog off.
    unfogged: Vec<(u32, u32)>,
    /// Whether the replay is inside the sky pass before the landscape.
    in_sky: bool,
    /// Whether the sky object being replayed is the authored dome.
    in_dome: bool,
    /// Whether the physical sky has been drawn this frame.
    sky_drawn: bool,
    /// Whether the horizon's veil has been drawn this frame.
    veil_drawn: bool,
    /// Whether the transmittance and multiple-scattering tables are to be drawn this frame.
    draw_tables: bool,
    /// Whether the sky-view table is to be drawn this frame.
    draw_sky_view: bool,
    /// What the tables now hold.
    tables_key: Option<TablesKey>,
    /// What the sky-view table now holds.
    sky_view_key: Option<SkyViewKey>,
    /// The device objects, made on first use.
    gpu: Option<Gpu>,
}

impl Shared {
    /// The copy of the recorded per-frame block at `offset` with its fog off, if one was made.
    fn unfogged(&self, offset: u32) -> Option<u32> {
        self.unfogged
            .binary_search_by_key(&offset, |(o, _)| *o)
            .ok()
            .map(|i| self.unfogged[i].1)
    }
}

/// The shared state. The passes run one after another on one thread and never hold it across
/// a call into another, so it is never borrowed twice.
fn state(shared: &RefCell<Shared>) -> RefMut<'_, Shared> {
    shared.borrow_mut()
}

/// The device objects.
#[derive(Debug)]
struct Gpu {
    format: wgpu::TextureFormat,
    constants: wgpu::Buffer,
    sampler: wgpu::Sampler,
    tables_layout: [wgpu::BindGroupLayout; 3],
    sky_layout: wgpu::BindGroupLayout,
    aerial_layout: wgpu::BindGroupLayout,
    transmittance: wgpu::RenderPipeline,
    multiple: wgpu::RenderPipeline,
    sky_view: wgpu::RenderPipeline,
    sky: wgpu::RenderPipeline,
    veil: wgpu::RenderPipeline,
    aerial: wgpu::RenderPipeline,
    /// The air over the re-shaded picture.
    aerial_hdr: wgpu::RenderPipeline,
    /// The multiple-scattering table, which only the tables read.
    multiple_table: wgpu::TextureView,
    /// The bind groups over the shared tables, and the tables they were made for.
    binds: Option<TableBinds>,
    /// The air's bind group, and the picture and depth it was made for.
    aerial_bind: Option<(wgpu::TextureView, wgpu::TextureView, wgpu::BindGroup)>,
}

/// The bind groups over the shared tables.
#[derive(Debug)]
struct TableBinds {
    transmittance_view: wgpu::TextureView,
    sky_view_view: wgpu::TextureView,
    tables: [wgpu::BindGroup; 3],
    sky: wgpu::BindGroup,
}

fn texture_entry(binding: u32, filterable: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(model::CONSTANTS_SIZE as u64),
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

impl Gpu {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let [tables_src, sky_src, aerial_src] = shader_sources();
        let module = |label: &str, src: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(src.into()),
            })
        };
        let tables_module = module("sky tables", &tables_src.1);
        let sky_module = module("sky", &sky_src.1);
        let aerial_module = module("atmosphere", &aerial_src.1);
        let aerial_hdr_module = module("atmosphere hdr", &aerial_hdr_source());
        let layout = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let tables_layout = [
            layout("sky tables", &[uniform_entry()]),
            layout(
                "sky tables",
                &[uniform_entry(), texture_entry(1, true), sampler_entry(2)],
            ),
            layout(
                "sky tables",
                &[
                    uniform_entry(),
                    texture_entry(1, true),
                    sampler_entry(2),
                    texture_entry(3, true),
                ],
            ),
        ];
        let sky_layout = layout(
            "sky",
            &[uniform_entry(), texture_entry(1, true), sampler_entry(2)],
        );
        let aerial_layout = layout(
            "atmosphere",
            &[
                uniform_entry(),
                texture_entry(1, true),
                sampler_entry(2),
                texture_entry(3, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        );
        let pipeline = |label: &str,
                        module: &wgpu::ShaderModule,
                        group: &wgpu::BindGroupLayout,
                        entry: &str,
                        vertex: &str,
                        target: wgpu::ColorTargetState,
                        depth: Option<wgpu::DepthStencilState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(target)],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let table_target = wgpu::ColorTargetState {
            format: TABLE_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let transmittance = pipeline(
            "sky transmittance",
            &tables_module,
            &tables_layout[0],
            "fs_transmittance",
            "vs_fullscreen",
            table_target.clone(),
            None,
        );
        let multiple = pipeline(
            "sky multiple scattering",
            &tables_module,
            &tables_layout[1],
            "fs_multiple",
            "vs_fullscreen",
            table_target.clone(),
            None,
        );
        let sky_view = pipeline(
            "sky view",
            &tables_module,
            &tables_layout[2],
            "fs_sky_view",
            "vs_fullscreen",
            table_target,
            None,
        );
        // Drawn inside the world replay, in the sky pass: blended by the shader's alpha, leaving
        // the target's alpha alone, and touching no depth, as every sky object draws. The sky
        // goes over the dome and under everything after it; the veil over all the sky.
        let in_sky = |entry: &str| {
            pipeline(
                "sky",
                &sky_module,
                &sky_layout,
                entry,
                "vs_fullscreen",
                wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                },
                Some(wgpu::DepthStencilState {
                    format: SidecarContext::depth_format(),
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
            )
        };
        let sky = in_sky("fs_sky");
        let veil = in_sky("fs_veil");
        let aerial = pipeline(
            "atmosphere",
            &aerial_module,
            &aerial_layout,
            "fs_aerial",
            "vs_fullscreen",
            wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            },
            None,
        );
        let aerial_hdr = pipeline(
            "atmosphere hdr",
            &aerial_hdr_module,
            &aerial_layout,
            "fs_aerial",
            "vs_fullscreen",
            wgpu::ColorTargetState {
                format: crate::derive::pipeline::HDR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            },
            None,
        );
        let constants = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("atmosphere"),
            size: model::CONSTANTS_SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sky tables"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let multiple_table = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("sky multiple scattering"),
                size: wgpu::Extent3d {
                    width: MULTIPLE_SIZE.0,
                    height: MULTIPLE_SIZE.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TABLE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            format,
            constants,
            sampler,
            tables_layout,
            sky_layout,
            aerial_layout,
            transmittance,
            multiple,
            sky_view,
            sky,
            veil,
            aerial,
            aerial_hdr,
            multiple_table,
            binds: None,
            aerial_bind: None,
        }
    }

    /// The bind groups over `transmittance` and `sky_view`, made again when either table is a
    /// new texture. Returns whether they were made again, so the tables are drawn into it.
    fn bind_tables(
        &mut self,
        device: &wgpu::Device,
        transmittance: &wgpu::TextureView,
        sky_view: &wgpu::TextureView,
    ) -> bool {
        if self
            .binds
            .as_ref()
            .is_some_and(|b| b.transmittance_view == *transmittance && b.sky_view_view == *sky_view)
        {
            return false;
        }
        let constants = wgpu::BindGroupEntry {
            binding: 0,
            resource: self.constants.as_entire_binding(),
        };
        fn view(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(v),
            }
        }
        let sampler = wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(&self.sampler),
        };
        let group = |layout: &wgpu::BindGroupLayout, entries: &[wgpu::BindGroupEntry<'_>]| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sky"),
                layout,
                entries,
            })
        };
        let tables = [
            group(&self.tables_layout[0], std::slice::from_ref(&constants)),
            group(
                &self.tables_layout[1],
                &[constants.clone(), view(1, transmittance), sampler.clone()],
            ),
            group(
                &self.tables_layout[2],
                &[
                    constants.clone(),
                    view(1, transmittance),
                    sampler.clone(),
                    view(3, &self.multiple_table),
                ],
            ),
        ];
        let sky = group(&self.sky_layout, &[constants, view(1, sky_view), sampler]);
        self.binds = Some(TableBinds {
            transmittance_view: transmittance.clone(),
            sky_view_view: sky_view.clone(),
            tables,
            sky,
        });
        true
    }

    /// The air's bind group over `picture` and `depth`.
    fn aerial_bind(
        &mut self,
        device: &wgpu::Device,
        picture: &wgpu::TextureView,
        depth: &wgpu::TextureView,
    ) -> Option<&wgpu::BindGroup> {
        let sky_view = self.binds.as_ref()?.sky_view_view.clone();
        let fresh = self
            .aerial_bind
            .as_ref()
            .is_none_or(|(p, d, _)| p != picture || d != depth);
        if fresh {
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("atmosphere"),
                layout: &self.aerial_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.constants.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&sky_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(picture),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                ],
            });
            self.aerial_bind = Some((picture.clone(), depth.clone(), bind));
        }
        self.aerial_bind.as_ref().map(|(_, _, b)| b)
    }
}

/// A table texture's spec.
const fn table_spec(width: u32, height: u32) -> TextureSpec {
    TextureSpec {
        width,
        height,
        format: TABLE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING),
    }
}

/// Whether the frame's world is split between an interior and the outdoors: its depth is
/// cleared part way, so it is not one scene's depth.
fn split(cx: &SidecarContext<'_>) -> bool {
    cx.tables()
        .marks
        .iter()
        .any(|(_, m)| matches!(m, Mark::IndoorFlush))
}

/// The sky's tables, drawn before anything else; the state the three passes share is worked out
/// here.
pub struct SkyTables(Rc<RefCell<Shared>>);

impl std::fmt::Debug for SkyTables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SkyTables")
    }
}

impl SkyTables {
    fn view(
        cx: &mut PrepareCx<'_, '_>,
        name: ResourceName,
        spec: TextureSpec,
    ) -> Result<wgpu::TextureView, HifiError> {
        cx.resources.texture(cx.seam.device(), name, spec)
    }
}

impl HifiPass for SkyTables {
    fn name(&self) -> &'static str {
        "sky tables"
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s, f)
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let mut shared = state(&self.0);
        let s = &mut *shared;
        s.unfogged.clear();
        s.in_sky = false;
        s.in_dome = false;
        s.sky_drawn = false;
        s.veil_drawn = false;
        s.draw_tables = false;
        s.draw_sky_view = false;
        s.active = !split(cx.seam) && cx.seam.tables().world_span().is_some();
        if !s.active {
            return Ok(());
        }
        let level = cx.settings.sky;
        s.level = level;
        s.weight = model::sky_share(&cx.frame.sky);
        let format = cx.seam.surface_format();
        if s.gpu.as_ref().is_none_or(|g| g.format != format) {
            s.gpu = Some(Gpu::new(cx.seam.device(), format));
            s.tables_key = None;
            s.sky_view_key = None;
        }
        let (sw, sh, _) = model::sky_view_quality(level);
        let transmittance = Self::view(
            cx,
            ResourceName::TransmittanceLut,
            table_spec(TRANSMITTANCE_SIZE.0, TRANSMITTANCE_SIZE.1),
        )?;
        let sky_view = Self::view(cx, ResourceName::SkyViewLut, table_spec(sw, sh))?;
        let Some(gpu) = s.gpu.as_mut() else {
            return Err(HifiError::NotReady("the sky's device objects"));
        };
        if gpu.bind_tables(cx.seam.device(), &transmittance, &sky_view) {
            s.tables_key = None;
            s.sky_view_key = None;
        }
        let key = SkyViewKey::of(cx.frame, level);
        s.draw_tables = s.tables_key != Some(key.tables);
        s.draw_sky_view = s.draw_tables || s.sky_view_key != Some(key);
        s.tables_key = Some(key.tables);
        s.sky_view_key = Some(key);
        cx.seam
            .queue()
            .write_buffer(&gpu.constants, 0, &model::constants(cx.frame, level));

        // The world without its linear fog: a copy of each fogged per-frame block it was
        // recorded with, the fog switched off.
        let span = cx
            .seam
            .tables()
            .world_span()
            .ok_or(HifiError::NotReady("the frame drew no whole world"))?;
        let mut offsets: Vec<u32> = cx
            .seam
            .tables()
            .draws
            .iter()
            .filter(|d| span.contains(&d.cmd))
            .map(|d| d.frame)
            .collect();
        offsets.sort_unstable();
        offsets.dedup();
        for offset in offsets {
            let fogged = cx
                .seam
                .frame_block(offset)
                .is_some_and(|b| b.fog_params[2] != 0.0);
            if !fogged {
                continue;
            }
            if let Some(copy) = cx
                .seam
                .derived_frame_block(offset, "fog off", |b| b.fog_params[2] = 0.0)
            {
                s.unfogged.push((offset, copy));
            }
        }
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let mut shared = state(&self.0);
        let s = &mut *shared;
        if !s.active || !(s.draw_tables || s.draw_sky_view) {
            return Ok(());
        }
        let Some(gpu) = s.gpu.as_ref() else {
            return Ok(());
        };
        let Some(binds) = gpu.binds.as_ref() else {
            return Ok(());
        };
        let draw = |cx: &mut EncodeCx<'_, '_>,
                    target: &wgpu::TextureView,
                    pipeline: &wgpu::RenderPipeline,
                    bind: &wgpu::BindGroup,
                    timed: Option<&'static str>| {
            let timestamps = timed.and_then(|n| cx.timer.writes(n));
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky tables"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: timestamps,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.draw(0..3, 0..1);
            drop(pass);
            cx.seam.count_pass();
        };
        if s.draw_tables {
            draw(
                cx,
                &binds.transmittance_view,
                &gpu.transmittance,
                &binds.tables[0],
                None,
            );
            draw(
                cx,
                &gpu.multiple_table,
                &gpu.multiple,
                &binds.tables[1],
                None,
            );
        }
        draw(
            cx,
            &binds.sky_view_view,
            &gpu.sky_view,
            &binds.tables[2],
            Some("sky tables"),
        );
        Ok(())
    }
}

/// The physical sky in the world replay, and the replay of the world without its fog.
pub struct Sky(Rc<RefCell<Shared>>);

impl std::fmt::Debug for Sky {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sky")
    }
}

impl Sky {
    /// Draw the physical sky into the open world pass, once a frame.
    fn draw_sky(s: &mut Shared, pass: &mut wgpu::RenderPass<'_>) {
        s.sky_drawn = true;
        if s.weight < LEAST_WEIGHT {
            return;
        }
        Self::draw_full(s, pass, |g| &g.sky);
    }

    /// Draw the horizon's veil over the sky into the open world pass, once a frame.
    fn draw_veil(s: &mut Shared, pass: &mut wgpu::RenderPass<'_>) {
        s.veil_drawn = true;
        Self::draw_full(s, pass, |g| &g.veil);
    }

    fn draw_full(
        s: &Shared,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: impl FnOnce(&Gpu) -> &wgpu::RenderPipeline,
    ) {
        let Some(gpu) = s.gpu.as_ref() else {
            return;
        };
        let Some(binds) = gpu.binds.as_ref() else {
            return;
        };
        pass.set_pipeline(pipeline(gpu));
        pass.set_bind_group(0, &binds.sky, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl HifiPass for Sky {
    fn name(&self) -> &'static str {
        "sky"
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s, f)
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn replay_filter(&mut self) -> Option<&mut dyn ReplayFilter> {
        Some(self)
    }

    fn encode(&mut self, _cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }
}

impl ReplayFilter for Sky {
    fn draw(&mut self, _cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        let s = state(&self.0);
        if !s.active {
            return DrawAction::Legacy;
        }
        if s.in_sky && s.in_dome && s.weight >= WHOLE_WEIGHT {
            return DrawAction::Skip(SkipRule::SkyDome);
        }
        DrawAction::Legacy
    }

    fn frame(&mut self, recorded: u32) -> Option<u32> {
        // The world without its linear fog, whichever pipeline draws it: the air puts the
        // distance back once, over the whole picture.
        let s = state(&self.0);
        if s.active {
            s.unfogged(recorded)
        } else {
            None
        }
    }

    fn mark(&mut self, mark: Mark, pass: &mut wgpu::RenderPass<'_>) {
        let mut shared = state(&self.0);
        let s = &mut *shared;
        if !s.active {
            return;
        }
        match mark {
            Mark::SkyBegin(0) => {
                s.in_sky = true;
                s.in_dome = false;
            }
            Mark::SkyObject { index, properties } if s.in_sky => {
                let dome = model::is_dome(index, properties);
                if !dome && !s.sky_drawn {
                    Self::draw_sky(s, pass);
                }
                s.in_dome = dome;
            }
            Mark::SkyEnd(0) => {
                if s.in_sky && !s.sky_drawn {
                    Self::draw_sky(s, pass);
                }
                if s.in_sky && !s.veil_drawn {
                    Self::draw_veil(s, pass);
                }
                s.in_sky = false;
                s.in_dome = false;
            }
            _ => {}
        }
    }
}

/// The air over the replayed world.
pub struct Atmosphere(Rc<RefCell<Shared>>);

impl std::fmt::Debug for Atmosphere {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Atmosphere")
    }
}

impl HifiPass for Atmosphere {
    fn name(&self) -> &'static str {
        "atmosphere"
    }

    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s, f)
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let mut shared = state(&self.0);
        let s = &mut *shared;
        if !s.active {
            return Ok(());
        }
        let Some(gpu) = s.gpu.as_mut() else {
            return Ok(());
        };
        if let Some(rt) = cx.reshade {
            if !*cx.resolved {
                // Over the re-shaded picture, in linear light, before the tone map: the copy
                // is read and the picture written.
                let copy = crate::reshade::hdr_copy(cx.seam, cx.resources, cx.encoder)?;
                let depth = cx.depth.clone();
                let device = cx.seam.device().clone();
                let Some(bind) = gpu.aerial_bind(&device, &copy, &depth).cloned() else {
                    return Ok(());
                };
                let timestamps = cx.timer.writes("atmosphere");
                let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("atmosphere hdr"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &rt.hdr,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: timestamps,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&gpu.aerial_hdr);
                pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
                drop(pass);
                cx.seam.count_pass();
                return Ok(());
            }
        }
        let input = cx.colour.clone();
        let world = match cx.resources.get(ResourceName::WorldColour) {
            Some(Resource::Texture(_, v)) => Some(v.clone()),
            _ => None,
        };
        // Written into whichever of the two pictures is not the one being read.
        let output = match world {
            Some(w) if w != input => w,
            _ => {
                let (width, height) = cx.seam.surface_size();
                cx.resources.texture(
                    cx.seam.device(),
                    ResourceName::ScratchColour,
                    TextureSpec {
                        width,
                        height,
                        format: cx.seam.surface_format(),
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::COPY_SRC,
                    },
                )?
            }
        };
        let depth = cx.depth.clone();
        let device = cx.seam.device().clone();
        let Some(bind) = gpu.aerial_bind(&device, &input, &depth).cloned() else {
            return Ok(());
        };
        let timestamps = cx.timer.writes("atmosphere");
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("atmosphere"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&gpu.aerial);
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        cx.seam.count_pass();
        *cx.colour = output;
        Ok(())
    }
}
