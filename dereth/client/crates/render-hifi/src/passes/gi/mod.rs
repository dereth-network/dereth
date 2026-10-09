//! Bounced light: sky visibility and one screen-space bounce over the relit world.
//!
//! After the relight has lit the opaque world, a half-resolution search through the depth buffer
//! finds, for each surface, how much of its sky is shut out and by what. What shuts it out lends
//! its own lit colour (one bounce of sun, sky and lamp light), and what stays open sees the sky.
//! The result is gathered over frames with the camera's reprojection, filtered along depth and
//! normal edges, brought back to full resolution, and put in place of the relight's flat sky
//! term. Shadowed ground takes the sky's blue and the grass's green, walls by sunlit ground warm
//! up, and corners and the ground under trees darken.

use glam::{Mat4, Vec3};

use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::passes::lighting::{env_f32, legacy_light, sun_up};
use crate::resources::ResourceName;
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::{Caps, DebugView, HifiError, HifiFrame, HifiSettings};

pub(crate) const GI_WGSL: &str = include_str!("gi.wgsl");
const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[derive(Debug)]
struct GiGpu {
    layout: wgpu::BindGroupLayout,
    trace: wgpu::RenderPipeline,
    accumulate: wgpu::RenderPipeline,
    filter: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    view: wgpu::RenderPipeline,
    params: [wgpu::Buffer; 2],
}

fn tex(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl GiGpu {
    fn new(device: &wgpu::Device) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity bounced light"),
            entries: &[
                tex(0, float),
                tex(1, float),
                tex(2, wgpu::TextureSampleType::Depth),
                tex(3, float),
                tex(4, float),
                tex(5, float),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
            label: Some("high-fidelity bounced light"),
            source: wgpu::ShaderSource::Wgsl(GI_WGSL.into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity bounced light"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make = |entry: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
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
                        format: HDR,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let add = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let uniform = || {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("high-fidelity bounced light"),
                size: 512,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            trace: make("fs_trace", None),
            accumulate: make("fs_accumulate", None),
            filter: make("fs_filter", None),
            composite: make("fs_composite", Some(add)),
            view: make("fs_view", None),
            params: [uniform(), uniform()],
            layout,
        }
    }
}

/// The half-resolution targets: this frame's search, two histories and a filter target.
#[derive(Debug)]
struct Targets {
    size: (u32, u32),
    raw: wgpu::TextureView,
    history: [wgpu::TextureView; 2],
    filtered: wgpu::TextureView,
    _textures: Vec<wgpu::Texture>,
}

impl Targets {
    fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let make = || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("high-fidelity bounced light"),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let textures: Vec<wgpu::Texture> = (0..4).map(|_| make()).collect();
        let view = |i: usize| textures[i].create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            size,
            raw: view(0),
            history: [view(1), view(2)],
            filtered: view(3),
            _textures: textures,
        }
    }
}

/// The bounced light pass.
#[derive(Debug, Default)]
pub struct GlobalIllumination {
    gpu: Option<GiGpu>,
    targets: Option<Targets>,
    split: bool,
    frame: u32,
    /// Which history this frame writes.
    flip: usize,
    /// Last frame's world to clip and eye, when it drew bounced light.
    previous: Option<(Mat4, Vec3)>,
}

impl HifiPass for GlobalIllumination {
    fn name(&self) -> &'static str {
        "gi"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        s.global_illumination.is_on() && s.lighting.is_on()
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        self.split =
            crate::reshade::ReshadePlan::new(cx.seam.tables()).is_some_and(|p| p.is_split());
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let Some(rt) = cx.reshade else {
            self.previous = None;
            return Ok(());
        };
        if self.split || !cx.frame.sky.outdoor {
            self.previous = None;
            return Ok(());
        }
        let Some(crate::resources::Resource::Texture(_, surface)) =
            cx.resources.touch(ResourceName::GiSurface)
        else {
            return Ok(());
        };
        let surface = surface.clone();
        let frame = cx.frame;
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let gpu = self.gpu.get_or_insert_with(|| GiGpu::new(&device));
        let size = cx.seam.surface_size();
        let half = (size.0.div_ceil(2), size.1.div_ceil(2));
        if self.targets.as_ref().is_none_or(|t| t.size != half) {
            self.targets = Some(Targets::new(&device, half));
            self.previous = None;
        }
        let Some(targets) = self.targets.as_ref() else {
            return Ok(());
        };

        // --- parameters --------------------------------------------------------------------
        let cam = &frame.camera;
        let render_to_clip = cam.projection * cam.view * HEIGHT_UP;
        let (sun, amb) = legacy_light(frame);
        let lin = |c: [f32; 3]| c.map(|x| dereth_primitives::num::math::powf(x.max(0.0), 2.2));
        let sun_lin = lin(sun);
        let amb_lin = lin(amb);
        let sun_on = sun_up(frame);
        let sun_gain = if sun_on {
            env_f32("DERETH_HIFI_SUN", 2.6)
        } else {
            0.0
        };
        let amb_gain = env_f32("DERETH_HIFI_AMBIENT", 0.85);
        let l = frame.sky.sun_direction.normalize_or(Vec3::Z);
        let elevation = l.z.clamp(0.0, 1.0);
        // The sky's own light: the ambient the scene authored, and a blue share of the sun.
        let sky_k = env_f32("DERETH_HIFI_GI_SKY", 0.6);
        let sky_tint = [0.62, 0.80, 1.0];
        let sky: Vec<f32> = (0..3)
            .map(|i| {
                amb_lin[i] * amb_gain * [0.85, 0.95, 1.15][i]
                    + sun_lin[i] * sun_gain * sky_k * (0.25 + 0.75 * elevation) * sky_tint[i]
            })
            .collect();
        // The far ground: sunlit grass and earth seen from below the horizon.
        let ground_tint = [0.85, 1.0, 0.62];
        let ground: Vec<f32> = (0..3)
            .map(|i| {
                sun_lin[i] * sun_gain * elevation * 0.16 * ground_tint[i]
                    + amb_lin[i] * amb_gain * 0.3
            })
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let viewport = cx
            .seam
            .world_viewport()
            .map_or([0.0, 0.0, size.0 as f32, size.1 as f32], |v| {
                [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
            });
        let history_ok = self.previous.is_some();
        let (prev_clip, prev_eye) = self.previous.unwrap_or((render_to_clip, cam.eye));
        let mut p: Vec<f32> = Vec::with_capacity(128);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend(render_to_clip.to_cols_array());
        p.extend(prev_clip.to_cols_array());
        p.extend((cam.view * HEIGHT_UP).inverse().to_cols_array());
        p.extend(viewport);
        #[allow(clippy::cast_precision_loss)]
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, (self.frame % 4096) as f32]);
        p.extend([
            prev_eye.x,
            prev_eye.y,
            prev_eye.z,
            f32::from(u8::from(history_ok)),
        ]);
        p.extend([l.x, l.y, l.z, f32::from(u8::from(sun_on))]);
        p.extend([sun_lin[0], sun_lin[1], sun_lin[2], sun_gain]);
        p.extend([amb_lin[0], amb_lin[1], amb_lin[2], amb_gain]);
        p.extend([
            sky[0],
            sky[1],
            sky[2],
            env_f32("DERETH_HIFI_GI_RADIUS", 5.0),
        ]);
        p.extend([
            ground[0],
            ground[1],
            ground[2],
            env_f32("DERETH_HIFI_GI_THICKNESS", 0.8),
        ]);
        let px_per_m = viewport[3] * 0.5 * cam.projection.y_axis.y.abs();
        #[allow(clippy::cast_precision_loss)]
        p.extend([
            px_per_m,
            env_f32("DERETH_HIFI_GI", 1.0),
            viewport[3] * 0.14,
            0.0,
        ]);
        #[allow(clippy::cast_precision_loss)]
        let half_f = [half.0 as f32, half.1 as f32];
        let bounce = env_f32("DERETH_HIFI_GI_BOUNCE", 1.0);
        let mut p2 = p.clone();
        p.extend([half_f[0], half_f[1], 1.0, bounce]);
        p2.extend([half_f[0], half_f[1], 2.0, bounce]);
        queue.write_buffer(&gpu.params[0], 0, &floats(&p));
        queue.write_buffer(&gpu.params[1], 0, &floats(&p2));
        self.previous = Some((render_to_clip, cam.eye));
        self.frame = self.frame.wrapping_add(1);

        // --- the passes --------------------------------------------------------------------
        let bind = |hdr: &wgpu::TextureView,
                    a: &wgpu::TextureView,
                    b: &wgpu::TextureView,
                    params: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity bounced light"),
                layout: &gpu.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(hdr),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&rt.normal),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(cx.depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&surface),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: params.as_entire_binding(),
                    },
                ],
            })
        };
        let cur = self.flip;
        let prev = 1 - cur;
        self.flip = prev;
        let hist_cur = &targets.history[cur];
        let hist_prev = &targets.history[prev];
        let steps: [(
            &str,
            &wgpu::RenderPipeline,
            wgpu::BindGroup,
            &wgpu::TextureView,
        ); 4] = [
            (
                "gi trace",
                &gpu.trace,
                bind(&rt.hdr, &targets.filtered, hist_prev, &gpu.params[0]),
                &targets.raw,
            ),
            (
                "gi accumulate",
                &gpu.accumulate,
                bind(&rt.hdr, &targets.raw, hist_prev, &gpu.params[0]),
                hist_cur,
            ),
            (
                "gi filter",
                &gpu.filter,
                bind(&rt.hdr, hist_cur, hist_prev, &gpu.params[0]),
                &targets.filtered,
            ),
            (
                "gi filter wide",
                &gpu.filter,
                bind(&rt.hdr, &targets.filtered, hist_prev, &gpu.params[1]),
                &targets.raw,
            ),
        ];
        for (k, (label, pipeline, group, target)) in steps.iter().enumerate() {
            let writes = if k == 0 { cx.timer.writes("gi") } else { None };
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            cx.seam.count_pass();
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Into the picture: the bounced light in place of the flat sky term, or on its own.
        let shown = cx.settings.debug == DebugView::Ao;
        let group = bind(hist_prev, &targets.raw, hist_prev, &gpu.params[0]);
        let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gi composite"),
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
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        cx.seam.count_pass();
        pass.set_pipeline(if shown { &gpu.view } else { &gpu.composite });
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
        Ok(())
    }
}
