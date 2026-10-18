//! The rooms' own light per pixel: the static lights the rooms' vertex colors are burned from,
//! worked out again at every pixel of a room's surface with a smooth falloff and the surface's
//! turn toward each, so the relight can give the room pools of light round its torches, and shade
//! the faces turned from them, in place of the even light the vertices hold, without lighting the
//! room twice. With the lamps on a device with ray queries, a light whose way to the pixel is
//! shut gives it only the little the room scatters, so no light reaches through a wall.

use crate::build::Build;
use crate::shared::camera::{render_from_clip, HEIGHT_UP};
use crate::snapshot::HifiLight;
use crate::HifiFrame;

const SOURCE: &str = include_str!("cells.wgsl");
const TRACE: &str = include_str!("cells_trace.wgsl");
const OUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The most lights a pixel walks.
pub const MAX_CELL_LIGHTS: usize = 128;
/// The bytes of one light in the shader's list.
const LIGHT_BYTES: u64 = 32;
/// How much farther than its falloff a static light reaches, as the burn takes it: a third
/// again, nearly.
pub const RANGE_OVER_FALLOFF: f32 = 1.3;
/// How far short of a light a ray toward it stops, meters: the light's own sconce, lantern or
/// brazier does not shade it.
const STOP_SHORT: f32 = 0.6;

/// The shader, traced (with ray queries) or not.
#[must_use]
pub fn shader(traced: bool) -> String {
    if traced {
        format!(
            "enable wgpu_ray_query;\n{}",
            SOURCE.replace("{{TRACE}}", TRACE)
        )
    } else {
        SOURCE.replace(
            "{{TRACE}}",
            "fn blocked(origin: vec3<f32>, dir: vec3<f32>, t_max: f32) -> bool {\n    return false;\n}\n",
        )
    }
}

/// The lights burned into the rooms that can reach what the frame shows, nearest the eye first,
/// at most [`MAX_CELL_LIGHTS`] of them.
#[must_use]
pub fn burned_lights(frame: &HifiFrame) -> Vec<&HifiLight> {
    let eye = frame.camera.eye;
    let mut lights: Vec<&HifiLight> = frame
        .lights
        .iter()
        .filter(|l| l.burned && l.falloff > 0.0 && l.intensity > 0.0)
        .filter(|l| (l.position - eye).length() - reach(l) < frame.camera.far.max(1.0))
        .collect();
    lights.sort_by(|a, b| {
        (a.position - eye)
            .length()
            .total_cmp(&(b.position - eye).length())
    });
    lights.truncate(MAX_CELL_LIGHTS);
    lights
}

/// How much of a light's reach `x` (the distance over the range) leaves of its light, as the
/// shader's `room_window` has it: smoothly down to nothing at the range.
#[must_use]
pub fn room_window(x: f32) -> f32 {
    let w = (1.0 - x * x).clamp(0.0, 1.0);
    w * w
}

/// The reach of `light`, meters, as its light is burned into the rooms.
#[must_use]
pub fn reach(light: &HifiLight) -> f32 {
    light.falloff * RANGE_OVER_FALLOFF
}

fn entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty,
        count: None,
    }
}

fn tex(sample_type: wgpu::TextureSampleType, dim: wgpu::TextureViewDimension) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type,
        view_dimension: dim,
        multisampled: false,
    }
}

fn buf(ty: wgpu::BufferBindingType) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|f| f.to_le_bytes()).collect()
}

#[derive(Debug)]
struct Gpu {
    traced: bool,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    lights: wgpu::Buffer,
}

impl Gpu {
    fn new(device: &wgpu::Device, traced: bool) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let d2 = wgpu::TextureViewDimension::D2;
        let mut entries = vec![
            entry(0, tex(wgpu::TextureSampleType::Depth, d2)),
            entry(1, tex(float, d2)),
            entry(
                2,
                wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: OUT,
                    view_dimension: d2,
                },
            ),
            entry(3, buf(wgpu::BufferBindingType::Uniform)),
            entry(4, buf(wgpu::BufferBindingType::Storage { read_only: true })),
        ];
        if traced {
            entries.extend([
                entry(
                    5,
                    wgpu::BindingType::AccelerationStructure {
                        vertex_return: false,
                    },
                ),
                entry(
                    6,
                    tex(
                        wgpu::TextureSampleType::Float { filterable: true },
                        wgpu::TextureViewDimension::D2Array,
                    ),
                ),
                entry(
                    7,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(8, buf(wgpu::BufferBindingType::Storage { read_only: true })),
                entry(9, buf(wgpu::BufferBindingType::Storage { read_only: true })),
            ]);
        }
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity room light"),
            entries: &entries,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity room light"),
            source: wgpu::ShaderSource::Wgsl(shader(traced).into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity room light"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.build_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("high-fidelity room light"),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        Self {
            traced,
            layout,
            pipeline,
            params: buffer("high-fidelity room light", 256, wgpu::BufferUsages::UNIFORM),
            lights: buffer(
                "high-fidelity room lights",
                MAX_CELL_LIGHTS as u64 * LIGHT_BYTES,
                wgpu::BufferUsages::STORAGE,
            ),
        }
    }
}

/// The rooms' light per pixel, kept across frames.
#[derive(Debug, Default)]
pub struct CellLight {
    gpu: Option<Gpu>,
    /// The same, built ahead by the warm-up: traced and not.
    pre_traced: crate::build::Prebuilt<Gpu>,
    pre_plain: crate::build::Prebuilt<Gpu>,
    target: Option<((u32, u32), wgpu::Texture, wgpu::TextureView)>,
    /// How many lights the last frame walked.
    walked: usize,
}

impl CellLight {
    /// Whether the room lights' device objects for drawing traced or not (`traced`) are made, or
    /// are not being built.
    #[must_use]
    pub fn ready(&self, traced: bool) -> bool {
        let pre = if traced {
            &self.pre_traced
        } else {
            &self.pre_plain
        };
        self.gpu.as_ref().is_some_and(|g| g.traced == traced) || !pre.waits()
    }

    /// Ask for the room lights' device objects to be built ahead: drawn without tracing, and
    /// traced too when the lamps' scene is (`traced`).
    pub fn warm(&mut self, cx: &mut crate::graph::WarmCx<'_>, traced: bool) {
        if self.gpu.is_some() {
            return;
        }
        let device = cx.device.clone();
        self.pre_plain
            .ask(cx.warm, move || Gpu::new(&device, false));
        if traced {
            let device = cx.device.clone();
            self.pre_traced
                .ask(cx.warm, move || Gpu::new(&device, true));
        }
    }

    /// How many lights the last frame walked.
    #[must_use]
    pub fn walked(&self) -> usize {
        self.walked
    }

    /// Work out the rooms' light on every pixel of a room's surface in view, traced through
    /// `scene` when there is one; `None` when the frame holds no light burned into a room.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: Option<&crate::passes::rt::SceneBindings<'_>>,
        frame: &HifiFrame,
        depth: &wgpu::TextureView,
        normals: &wgpu::TextureView,
        size: (u32, u32),
        viewport: [f32; 4],
        timer: &mut crate::timing::GpuTimer,
    ) -> Option<&wgpu::TextureView> {
        let lights = burned_lights(frame);
        self.walked = lights.len();
        let traced = scene.is_some();
        if self.gpu.as_ref().is_none_or(|g| g.traced != traced) {
            let pre = if traced {
                &mut self.pre_traced
            } else {
                &mut self.pre_plain
            };
            self.gpu = Some(pre.take().unwrap_or_else(|| Gpu::new(device, traced)));
        }
        let gpu = self.gpu.as_ref()?;
        if self.target.as_ref().is_none_or(|t| t.0 != size) {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("high-fidelity room light"),
                size: wgpu::Extent3d {
                    width: size.0.max(1),
                    height: size.1.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: OUT,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some((size, t, v));
        }
        let (_, _, out) = self.target.as_ref()?;
        let mut lb: Vec<f32> = Vec::with_capacity(lights.len() * 8);
        for l in &lights {
            lb.extend([l.position.x, l.position.y, l.position.z, reach(l)]);
            lb.extend([l.color[0], l.color[1], l.color[2], 0.0]);
        }
        queue.write_buffer(&gpu.lights, 0, &floats(&lb));
        let cam = &frame.camera;
        let mut p: Vec<f32> = Vec::with_capacity(48);
        p.extend(render_from_clip(cam.view, cam.projection).to_cols_array());
        p.extend((cam.view * HEIGHT_UP).inverse().to_cols_array());
        p.extend(viewport);
        #[allow(clippy::cast_precision_loss)] // at most a hundred and twenty-eight
        p.extend([cam.eye.x, cam.eye.y, cam.eye.z, lights.len() as f32]);
        p.extend([f32::from(u8::from(traced)), STOP_SHORT, 0.0, 0.0]);
        queue.write_buffer(&gpu.params, 0, &floats(&p));
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(normals),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(out),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: gpu.params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: gpu.lights.as_entire_binding(),
            },
        ];
        if let Some(scene) = scene {
            entries.extend([
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: scene.tlas.as_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(scene.atlas),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(scene.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: scene.cuts.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: scene.uvs.as_entire_binding(),
                },
            ]);
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity room light"),
            layout: &gpu.layout,
            entries: &entries,
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("room light"),
            timestamp_writes: timer.compute_writes("room light"),
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(size.0.div_ceil(8), size.1.div_ceil(8), 1);
        drop(pass);
        Some(out)
    }
}
