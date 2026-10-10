//! How the world reaches the target: replayed with its ordinary pipelines and worked over in
//! screen space (overlay), or replayed with pipelines derived from them into high dynamic range
//! with normals (re-shade); then composited into the target.
//!
//! **The overlay replay is the ordinary frame.** Everything recorded before the world's end mark
//! is replayed with the recorded pipelines, in recorded order, into a colour target of the
//! frame target's own format and a depth target of the recorded depth format, both cleared as
//! the ordinary frame clears them. Its pixels are therefore exactly the ordinary frame's up to
//! that point. The composite copies the finished picture into the frame target texel for texel,
//! with a load rather than a filtered sample, so with no pass in between the frame target holds
//! exactly what the ordinary frame would.
//!
//! **The passes read the depth each pixel sees.** Before the rooms of a building are drawn from
//! outdoors, each of its openings is stamped into the depth just short of the far plane, with no
//! colour, so the rooms draw over whatever was behind the opening. Where no room covers the
//! opening afterwards (through a window on the far side of the room, say) the stamp stays, over
//! the colour of the land or the sky beyond. The seen depth puts back, at just those pixels, the
//! depth the world had there with no stamp, so a pass that reads the depth finds the land at its
//! real distance and the sky only where the sky was drawn. A frame that steps indoors clears the
//! depth and stamps each opening of its rooms at the opening's own depth, so nothing beyond an
//! opening draws over what is seen through it; there, and through an opening the step leaves
//! cleared, the seen depth puts back the depth seen before the step.

use dereth_render::PipelineKey;
use dereth_render_cpu::pso::{portal_stamp_mask, PORTAL_STAMP_FAR_DEPTH};

use crate::resources::{ResourceName, Resources, TextureSpec};
use crate::timing::GpuTimer;
use crate::{
    DebugView, DrawAction, DrawNote, HifiError, HifiSettings, ReplayFilter, SideTables,
    SidecarContext, SkipRule,
};

/// How the world is replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositeMode {
    /// The world's own pixels, with screen-space passes over them.
    Overlay,
    /// The world re-shaded per pixel into high dynamic range.
    Reshade,
}

impl CompositeMode {
    /// The mode `settings` need: re-shading for the light per pixel (which the shadows, the
    /// bounced light and the lamps are drawn inside) and for the views of what re-shading writes
    /// (normals, the census, the parity view), overlay otherwise.
    #[must_use]
    pub fn for_settings(settings: &HifiSettings) -> Self {
        let per_pixel = settings.lighting.is_on()
            || matches!(
                settings.debug,
                DebugView::Normals | DebugView::Census | DebugView::Parity
            );
        if per_pixel {
            Self::Reshade
        } else {
            Self::Overlay
        }
    }
}

/// The composite's shader: a full-target triangle that copies the picture texel for texel, or
/// shows the world's depth inside the world viewport.
const COMPOSITE_WGSL: &str = r"
struct Params {
    viewport: vec4<f32>,
    near_far: vec4<f32>,
};
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var<uniform> params: Params;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_copy(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(picture, vec2<i32>(p.xy), 0);
}

// Depth as grey: near white, far black, on a logarithmic scale between the planes.
@fragment
fn fs_depth(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(p.xy);
    let c = textureLoad(picture, px, 0);
    let v = params.viewport;
    if (p.x < v.x || p.y < v.y || p.x >= v.x + v.z || p.y >= v.y + v.w) {
        return c;
    }
    let d = textureLoad(depth, px, 0);
    let n = params.near_far.x;
    let f = params.near_far.y;
    let z = n * f / (f - d * (f - n));
    let g = 1.0 - clamp(log2(z / n) / log2(f / n), 0.0, 1.0);
    return vec4<f32>(g, g, g, c.a);
}
";

/// The seen depth's shader: a full-target triangle that writes the world's depth, or, where a
/// stamp holds it, the depth of what the picture shows there.
fn seen_depth_wgsl() -> String {
    format!(
        r"
const STAMP: f32 = {PORTAL_STAMP_FAR_DEPTH:?}f;
@group(0) @binding(0) var world: texture_depth_2d;
@group(0) @binding(1) var unstamped: texture_depth_2d;
@group(0) @binding(2) var before: texture_depth_2d;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {{
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}}

// Outdoors: a building's stamp carries the constant with w = 1, so a stamped pixel holds it
// exactly, and the world with no stamp holds what is seen there.
@fragment
fn fs_seen(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {{
    let px = vec2<i32>(p.xy);
    let d = textureLoad(world, px, 0);
    if (d == STAMP) {{
        return textureLoad(unstamped, px, 0);
    }}
    return d;
}}

// After an indoor step: a room's stamp holds the depth of its opening, nearer than anything the
// step draws there without it; where it holds the depth, or where the step drew none, the
// picture is the one drawn before the step.
@fragment
fn fs_step(@builtin(position) p: vec4<f32>) -> @builtin(frag_depth) f32 {{
    let px = vec2<i32>(p.xy);
    let d = textureLoad(world, px, 0);
    if (d < textureLoad(unstamped, px, 0) || d >= 1.0) {{
        return textureLoad(before, px, 0);
    }}
    return d;
}}
"
    )
}

/// The composite's pipelines, made on first use for the target's format.
#[derive(Debug)]
struct Pipelines {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    copy: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    params: wgpu::Buffer,
}

/// One of the seen depth's pipelines, made on first use.
#[derive(Debug)]
struct SeenPipeline {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

/// The world replay and the composite into the frame target.
#[derive(Debug, Default)]
pub struct Compositor {
    pipelines: Option<Pipelines>,
    seen: Option<SeenPipeline>,
    seen_after_step: Option<SeenPipeline>,
}

/// Whether `note` is a stamp of an opening: a building's, drawn before its rooms from outdoors,
/// or a room's, drawn after an indoor step. The two share their pipeline.
fn is_stamp(note: &DrawNote) -> bool {
    !note.splat && note.key == PipelineKey::portal_stamp(portal_stamp_mask::BUILDING)
}

/// The command of the last building stamp drawn before the frame steps indoors, if it draws one.
#[must_use]
pub fn last_building_stamp(tables: &SideTables) -> Option<u32> {
    let cut = crate::reshade::ReshadePlan::new(tables)?.rest.start;
    tables
        .draws
        .iter()
        .rev()
        .filter(|d| d.cmd < cut)
        .find(|d| is_stamp(d))
        .map(|d| d.cmd)
}

/// The commands from the frame's indoor step to the world's end, when it steps indoors: the
/// rooms it draws over the depth it clears, and the stamps of their openings where it draws any.
#[must_use]
pub fn indoor_step(tables: &SideTables) -> Option<std::ops::Range<u32>> {
    Some(crate::reshade::ReshadePlan::new(tables)?.rest).filter(|rest| !rest.is_empty())
}

/// The replay that leaves every stamp out. It is not counted in the frame's census, so the rule
/// a stamp is left out under says nothing.
struct Unstamped;

impl ReplayFilter for Unstamped {
    fn draw(&mut self, _cmd: u32, note: Option<&DrawNote>) -> DrawAction<'_> {
        if note.is_some_and(is_stamp) {
            DrawAction::Skip(SkipRule::ReplacedByPass)
        } else {
            DrawAction::Legacy
        }
    }
}

/// The two targets the world is replayed into, as views.
#[derive(Debug, Clone)]
pub struct WorldTargets {
    /// The colour.
    pub colour: wgpu::TextureView,
    /// The depth.
    pub depth: wgpu::TextureView,
}

impl Compositor {
    /// The colour and depth targets the world is replayed into, at the frame target's size,
    /// made or remade as the size or format changes.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when they do not fit the video memory budget.
    pub fn targets(
        cx: &SidecarContext<'_>,
        resources: &mut Resources,
    ) -> Result<WorldTargets, HifiError> {
        let (width, height) = cx.surface_size();
        let colour = resources.texture(
            cx.device(),
            ResourceName::WorldColour,
            TextureSpec {
                width,
                height,
                format: cx.surface_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            },
        )?;
        let depth = resources.texture(
            cx.device(),
            ResourceName::WorldDepth,
            TextureSpec {
                width,
                height,
                format: SidecarContext::depth_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            },
        )?;
        Ok(WorldTargets { colour, depth })
    }

    /// A copy of the world's depth as it stands, for the passes after an indoor step's clear.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when it does not fit the video memory budget.
    pub fn keep_depth(
        cx: &SidecarContext<'_>,
        resources: &mut Resources,
        encoder: &mut wgpu::CommandEncoder,
        depth: &wgpu::TextureView,
    ) -> Result<wgpu::TextureView, HifiError> {
        let (width, height) = cx.surface_size();
        let kept = resources.texture(
            cx.device(),
            ResourceName::WorldDepthBeforeClear,
            TextureSpec {
                width,
                height,
                format: SidecarContext::depth_format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: depth.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: kept.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(kept)
    }

    /// The depth the commands in `range` leave with no stamp: replayed with the recorded
    /// pipelines and every stamp left out, into depth of its own (and a scratch picture nothing
    /// reads), cleared as the world replay clears. From the world's start, that is the world's
    /// depth with no building stamped; from an indoor step, the depth the step's rooms leave, which
    /// the step clears before it stamps or draws them. The replay is not counted in the frame's
    /// census.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when its targets do not fit the video memory budget.
    pub fn unstamped_depth(
        cx: &mut SidecarContext<'_>,
        resources: &mut Resources,
        encoder: &mut wgpu::CommandEncoder,
        range: std::ops::Range<u32>,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> Result<wgpu::TextureView, HifiError> {
        let (width, height) = cx.surface_size();
        let colour = resources.texture(
            cx.device(),
            ResourceName::ScratchColour,
            TextureSpec {
                width,
                height,
                format: cx.surface_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            },
        )?;
        let depth = resources.texture(
            cx.device(),
            ResourceName::WorldDepthUnstamped,
            TextureSpec {
                width,
                height,
                format: SidecarContext::depth_format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("unstamped depth"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &colour,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.count_pass();
        cx.replay_uncounted(range, &mut pass, &mut Unstamped);
        drop(pass);
        Ok(depth)
    }

    /// The depth each pixel sees: `world`, except where it holds a building stamp, which takes
    /// `unstamped` there instead.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when it does not fit the video memory budget.
    pub fn seen_depth(
        &mut self,
        cx: &mut SidecarContext<'_>,
        resources: &mut Resources,
        encoder: &mut wgpu::CommandEncoder,
        world: &wgpu::TextureView,
        unstamped: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> Result<wgpu::TextureView, HifiError> {
        let p = self
            .seen
            .get_or_insert_with(|| make_seen_pipeline(cx.device(), "fs_seen", 2));
        draw_seen(
            p,
            cx,
            resources,
            encoder,
            ResourceName::WorldDepthSeen,
            &[world, unstamped],
            timestamps,
        )
    }

    /// The depth each pixel sees after an indoor step, `step`: `world`, the depth the step left,
    /// except where a room's stamp holds it or the step drew nothing, which take `before`, the
    /// depth seen before the step. A step that stamps its rooms' openings is replayed once more
    /// with its stamps left out, and a stamp holds a pixel where it is nearer than anything the
    /// step draws there without it; a step that stamps none leaves its openings cleared.
    ///
    /// # Errors
    /// [`HifiError::Budget`] when its targets do not fit the video memory budget.
    #[allow(clippy::too_many_arguments)]
    pub fn seen_after_step(
        &mut self,
        cx: &mut SidecarContext<'_>,
        resources: &mut Resources,
        encoder: &mut wgpu::CommandEncoder,
        timer: &mut GpuTimer,
        world: &wgpu::TextureView,
        before: &wgpu::TextureView,
        step: std::ops::Range<u32>,
    ) -> Result<wgpu::TextureView, HifiError> {
        let stamped = cx
            .tables()
            .draws
            .iter()
            .any(|d| step.contains(&d.cmd) && is_stamp(d));
        let unstamped = if stamped {
            Self::unstamped_depth(
                cx,
                resources,
                encoder,
                step,
                timer.writes("unstamped depth"),
            )?
        } else {
            world.clone()
        };
        let p = self
            .seen_after_step
            .get_or_insert_with(|| make_seen_pipeline(cx.device(), "fs_step", 3));
        draw_seen(
            p,
            cx,
            resources,
            encoder,
            ResourceName::WorldDepthSeenAfterStep,
            &[world, &unstamped, before],
            timer.writes("seen depth"),
        )
    }

    /// Replay `range` with the recorded pipelines (unless `filter` says otherwise) into
    /// `targets`: cleared as the ordinary frame clears when `clear`, else drawn over.
    pub fn replay_legacy(
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &WorldTargets,
        range: std::ops::Range<u32>,
        clear: bool,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        filter: &mut dyn ReplayFilter,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(if clear {
                "world replay"
            } else {
                "world replay, resumed"
            }),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.colour,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 1.0,
                        })
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: if clear {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.count_pass();
        cx.replay(range, &mut pass, filter);
    }

    /// Replay everything recorded up to the world's end into `targets`, cleared as the ordinary
    /// frame clears its own, asking `filter` about each draw.
    ///
    /// # Errors
    /// [`HifiError::NotReady`] when the frame drew no whole world.
    pub fn replay_world(
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        targets: &WorldTargets,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
        filter: &mut dyn ReplayFilter,
    ) -> Result<(), HifiError> {
        let span = cx
            .tables()
            .world_span()
            .ok_or(HifiError::NotReady("the frame drew no whole world"))?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("world replay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.colour,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &targets.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.count_pass();
        cx.replay(0..span.end, &mut pass, filter);
        Ok(())
    }

    /// Copy `picture` into the frame target texel for texel; with the depth view asked for,
    /// show `depth` inside the world viewport instead.
    #[allow(clippy::too_many_arguments)]
    pub fn composite(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        picture: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        target: &wgpu::TextureView,
        view: DebugView,
        near_far: (f32, f32),
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        let format = cx.surface_format();
        if self.pipelines.as_ref().is_none_or(|p| p.format != format) {
            self.pipelines = Some(make_pipelines(cx.device(), format));
        }
        let Some(p) = self.pipelines.as_ref() else {
            return;
        };
        let viewport = cx.world_viewport().map_or([0.0; 4], |v| {
            #[allow(clippy::cast_precision_loss)] // pixel extents
            [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
        });
        let mut params = [0u8; 32];
        for (i, f) in viewport
            .iter()
            .chain(
                [
                    near_far.0.max(1e-3),
                    near_far.1.max(near_far.0 + 1.0),
                    0.0,
                    0.0,
                ]
                .iter(),
            )
            .enumerate()
        {
            params[i * 4..i * 4 + 4].copy_from_slice(&f.to_le_bytes());
        }
        cx.queue().write_buffer(&p.params, 0, &params);
        let bind = cx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &p.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(picture),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: p.params.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite"),
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
        cx.count_pass();
        pass.set_pipeline(if view == DebugView::Depth {
            &p.depth
        } else {
            &p.copy
        });
        pass.set_bind_group(0, &bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn make_pipelines(device: &wgpu::Device, format: wgpu::TextureFormat) -> Pipelines {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("composite"),
        source: wgpu::ShaderSource::Wgsl(COMPOSITE_WGSL.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("composite"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("composite"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = |entry: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
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
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("composite"),
        size: 32,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    Pipelines {
        format,
        layout,
        copy: pipeline("fs_copy"),
        depth: pipeline("fs_depth"),
        params,
    }
}

/// Draw `p` over the whole of the depth target `name`, its depth textures `views` bound in
/// order; the target's view.
fn draw_seen(
    p: &SeenPipeline,
    cx: &mut SidecarContext<'_>,
    resources: &mut Resources,
    encoder: &mut wgpu::CommandEncoder,
    name: ResourceName,
    views: &[&wgpu::TextureView],
    timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
) -> Result<wgpu::TextureView, HifiError> {
    let (width, height) = cx.surface_size();
    let seen = resources.texture(
        cx.device(),
        name,
        TextureSpec {
            width,
            height,
            format: SidecarContext::depth_format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        },
    )?;
    let entries: Vec<wgpu::BindGroupEntry<'_>> = (0u32..)
        .zip(views)
        .map(|(binding, view)| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        })
        .collect();
    let bind = cx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("seen depth"),
        layout: &p.layout,
        entries: &entries,
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("seen depth"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &seen,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: timestamps,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    cx.count_pass();
    pass.set_pipeline(&p.pipeline);
    pass.set_bind_group(0, &bind, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);
    Ok(seen)
}

/// The seen depth's pipeline drawing with the fragment stage `entry`, which reads `bindings`
/// depth textures.
fn make_seen_pipeline(device: &wgpu::Device, entry: &str, bindings: u32) -> SeenPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("seen depth"),
        source: wgpu::ShaderSource::Wgsl(seen_depth_wgsl().into()),
    });
    let entries: Vec<wgpu::BindGroupLayoutEntry> = (0..bindings)
        .map(|binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("seen depth"),
        entries: &entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("seen depth"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("seen depth"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SidecarContext::depth_format(),
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[],
        }),
        multiview_mask: None,
        cache: None,
    });
    SeenPipeline { layout, pipeline }
}

/// The composite's shader, for the device-free validation test.
#[must_use]
pub const fn composite_shader() -> &'static str {
    COMPOSITE_WGSL
}

/// The seen depth's shader, for the device-free validation test.
#[must_use]
pub fn seen_depth_shader() -> String {
    seen_depth_wgsl()
}
