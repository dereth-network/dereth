//! Shafts of sunlight through leaves and roofs, gathered on screen: the light in the air that the
//! shadow maps are too coarse to hold where a canopy breaks the sun into a thousand gaps. Drawn
//! with God Rays, while the sun is up and in front of the eye.

use glam::{Vec3, Vec4};

use crate::build::Build;
use crate::shared::camera::HEIGHT_UP;
use crate::HifiFrame;

const SHADER: &str = include_str!("sunshafts.wgsl");

/// The shafts' shader source, for validation.
#[must_use]
pub fn shader() -> String {
    SHADER.to_owned()
}

/// Where the sun stands in the viewport for `frame`, 0 to 1 across and down, and how much of
/// the shafts show: all with the sun on screen, fading out as it leaves it, none behind the eye.
#[must_use]
pub fn sun_on_screen(frame: &HifiFrame) -> Option<([f32; 2], f32)> {
    let cam = &frame.camera;
    let dir = frame.sky.sun_direction.normalize_or(Vec3::Z);
    let clip = cam.projection * cam.view * HEIGHT_UP * Vec4::new(dir.x, dir.y, dir.z, 0.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc = [clip.x / clip.w, clip.y / clip.w];
    let at = [ndc[0] * 0.5 + 0.5, 0.5 - ndc[1] * 0.5];
    // Out to half a screen past its edge the shafts still reach in.
    let out = [(-at[0]).max(at[0] - 1.0), (-at[1]).max(at[1] - 1.0)];
    let past = out[0].max(out[1]).max(0.0);
    let shown = (1.0 - past / 0.5).clamp(0.0, 1.0);
    (shown > 0.0).then_some((at, shown))
}

/// The shafts' pipeline and its parameters.
#[derive(Debug)]
pub struct SunShafts {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
}

impl SunShafts {
    #[must_use]
    pub fn new(device: &wgpu::Device, hdr: wgpu::TextureFormat) -> Self {
        let f = wgpu::ShaderStages::FRAGMENT;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity sun shafts"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: f,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: f,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity sun shafts"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity sun shafts"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let add = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let keep = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity sun shafts"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr,
                    blend: Some(wgpu::BlendState {
                        color: add,
                        alpha: keep,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("high-fidelity sun shafts"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            params,
        }
    }

    /// Draws the shafts of `sun`'s light, linear, standing at `at` in the viewport and `shown`
    /// as much, `strength` strong, into `target`, the lit picture, from `depth`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &HifiFrame,
        (at, shown): ([f32; 2], f32),
        strength: f32,
        sun: [f32; 3],
        depth: &wgpu::TextureView,
        target: &wgpu::TextureView,
        viewport: [f32; 4],
        timer: &mut crate::timing::GpuTimer,
    ) {
        let env = |name: &str, default: f32| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(default)
        };
        let strength = env("DERETH_HIFI_SUNSHAFTS", strength) * shown;
        #[allow(clippy::cast_precision_loss)] // the frame's number, wrapped to a few dozen
        let p: [f32; 16] = [
            at[0],
            at[1],
            strength,
            env("DERETH_HIFI_SUNSHAFT_REACH", 0.8),
            sun[0],
            sun[1],
            sun[2],
            env("DERETH_HIFI_SUNSHAFT_DECAY", 0.96),
            viewport[0],
            viewport[1],
            viewport[2],
            viewport[3],
            (frame.stamp % 64) as f32,
            env("DERETH_HIFI_SUNSHAFT_TIGHT", 10.0),
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = p.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&self.params, 0, &bytes);
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity sun shafts"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sun shafts"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timer.writes("sun shafts"),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_viewport(viewport[0], viewport[1], viewport[2], viewport[3], 0.0, 1.0);
        pass.draw(0..3, 0..1);
    }
}
