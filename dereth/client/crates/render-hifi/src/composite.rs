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

use crate::resources::{ResourceName, Resources, TextureSpec};
use crate::{DebugView, HifiError, HifiSettings, ReplayFilter, SidecarContext};

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

/// The composite's pipelines, made on first use for the target's format.
#[derive(Debug)]
struct Pipelines {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    copy: wgpu::RenderPipeline,
    depth: wgpu::RenderPipeline,
    params: wgpu::Buffer,
}

/// The world replay and the composite into the frame target.
#[derive(Debug, Default)]
pub struct Compositor {
    pipelines: Option<Pipelines>,
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

/// The composite's shader, for the device-free validation test.
#[must_use]
pub const fn composite_shader() -> &'static str {
    COMPOSITE_WGSL
}
