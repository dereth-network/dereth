//! What stands over the ground round the viewer: a depth of the retained static geometry seen
//! straight down, so the weather knows where a roof or a porch keeps the rain off.
//!
//! The map is centred on the viewer and snapped to its texels, so it holds still as the viewer
//! walks. Only the blocks' static geometry is drawn into it, never the landscape: the ground is
//! what the rain falls on, not what shelters it.

use std::collections::HashMap;

use crate::snapshot::{HifiFrame, HifiStaticBlock};

/// The map's side, texels.
pub const SIDE: u32 = 2048;
/// How far the map reaches from its centre, metres.
pub const REACH: f32 = 128.0;
/// How far above and below the viewer the map sees, metres.
const DEPTH_REACH: f32 = 300.0;
/// One block's constant block, bytes, at the device's dynamic offset alignment.
const STRIDE: u64 = 256;
/// A block's side, metres.
const BLOCK: f32 = 192.0;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const COVER_WGSL: &str = r"
struct Cover {
    // x, y the map's centre, render space; z how far it reaches; w unused.
    centre: vec4<f32>,
    // x the height the map's depth starts at; y the depth's span; zw unused.
    span: vec4<f32>,
    // The block's south-west corner, render space.
    origin: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cover: Cover;

@vertex
fn vs(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {
    let q = p + cover.origin.xyz;
    return vec4<f32>(
        (q.xy - cover.centre.xy) / cover.centre.z,
        clamp((cover.span.x - q.z) / cover.span.y, 0.0, 1.0),
        1.0,
    );
}
";

/// Where the map stands this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Placement {
    /// Its centre, render space.
    pub centre: [f32; 2],
    /// How far it reaches from the centre, metres.
    pub reach: f32,
    /// The height its depth starts at: depth 0 is this high, depth 1 is `span` below it.
    pub top: f32,
    /// The depth's span, metres.
    pub span: f32,
}

impl Placement {
    /// The map round `eye`, its centre snapped to a whole texel.
    #[must_use]
    pub fn around(eye: glam::Vec3) -> Self {
        #[allow(clippy::cast_precision_loss)] // the map's side, a small power of two
        let texel = 2.0 * REACH / SIDE as f32;
        let snap = |v: f32| (v / texel).round() * texel;
        Self {
            centre: [snap(eye.x), snap(eye.y)],
            reach: REACH,
            top: eye.z + DEPTH_REACH,
            span: 2.0 * DEPTH_REACH,
        }
    }

    /// Whether the block whose south-west corner is at `origin` reaches into the map.
    #[must_use]
    pub fn overlaps(&self, origin: glam::Vec3) -> bool {
        let [cx, cy] = self.centre;
        origin.x < cx + self.reach
            && origin.x + BLOCK > cx - self.reach
            && origin.y < cy + self.reach
            && origin.y + BLOCK > cy - self.reach
    }

    /// The four floats the weather shader reads the map with, and four more.
    #[must_use]
    pub fn constants(&self, drawn: bool) -> [f32; 8] {
        #[allow(clippy::cast_precision_loss)] // the map's side, a small power of two
        let side = SIDE as f32;
        [
            self.centre[0],
            self.centre[1],
            self.reach,
            side,
            self.top,
            self.span,
            f32::from(u8::from(drawn)),
            0.0,
        ]
    }
}

/// The static geometry of one block that can shelter the ground: every solid batch drawn in the
/// world (roofs, porches, walls, rocks), as positions, block-local, three per triangle; the
/// landscape and cut-out leaves left out.
#[must_use]
pub fn shelter_positions(block: &HifiStaticBlock) -> Vec<f32> {
    let mut out = Vec::new();
    for batch in &block.batches {
        // Leaves and other cut-out cards are left out: from above they are a scatter of
        // crossed cards whose outline would print on the ground as a hard-edged dry patch.
        if batch.format.is_pre_transformed() || batch.cutout {
            continue;
        }
        let stride = batch.format.stride() as usize;
        let count = batch.bytes.len() / stride;
        let usable = count - count % 3;
        for v in 0..usable {
            let at = v * stride;
            let f = |k: usize| {
                let b = &batch.bytes[at + k * 4..at + k * 4 + 4];
                f32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            out.extend_from_slice(&[f(0), f(1), f(2)]);
        }
    }
    out
}

#[derive(Debug)]
struct Block {
    generation: u32,
    buffer: Option<wgpu::Buffer>,
    vertices: u32,
    /// The buffer's size, bytes.
    bytes: u64,
}

#[derive(Debug)]
struct Gpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    capacity: u64,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The overhead map and the geometry it is drawn from.
#[derive(Debug, Default)]
pub struct CoverMap {
    blocks: HashMap<u16, Block>,
    gpu: Option<Gpu>,
}

impl CoverMap {
    /// Take the frame's newly sent block geometry, and let go of blocks no longer resident.
    pub fn take_feed(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &HifiFrame) {
        for block in &frame.static_feed {
            if self
                .blocks
                .get(&block.block)
                .is_some_and(|b| b.generation == block.generation)
            {
                continue;
            }
            let positions = shelter_positions(block);
            #[allow(clippy::cast_possible_truncation)] // a block's vertex count
            let vertices = (positions.len() / 3) as u32;
            let buffer = (!positions.is_empty()).then(|| {
                let bytes: Vec<u8> = positions.iter().flat_map(|f| f.to_le_bytes()).collect();
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("weather shelter"),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&buffer, 0, &bytes);
                buffer
            });
            self.blocks.insert(
                block.block,
                Block {
                    generation: block.generation,
                    buffer,
                    vertices,
                    bytes: positions.len() as u64 * 4,
                },
            );
        }
        self.blocks
            .retain(|id, _| frame.terrain.iter().any(|t| t.block == *id));
    }

    /// Whether any block's geometry is held.
    #[must_use]
    pub fn holds_any(&self) -> bool {
        self.blocks.values().any(|b| b.buffer.is_some())
    }

    /// The video memory held: the geometry's copies, and the map once it has been drawn.
    #[must_use]
    pub fn held_bytes(&self) -> (u64, u64) {
        let geometry = self.blocks.values().map(|b| b.bytes).sum();
        let map = if self.gpu.is_some() {
            u64::from(SIDE) * u64::from(SIDE) * 4
        } else {
            0
        };
        (geometry, map)
    }

    /// Draw the map at `at` and hand back its view.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &HifiFrame,
        at: &Placement,
        timer: &mut crate::timing::GpuTimer,
    ) -> wgpu::TextureView {
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(device));
        let origins: HashMap<u16, glam::Vec3> =
            frame.terrain.iter().map(|t| (t.block, t.origin)).collect();
        let draws: Vec<(&Block, glam::Vec3)> = self
            .blocks
            .iter()
            .filter_map(|(id, b)| {
                let o = *origins.get(id)?;
                (b.buffer.is_some() && at.overlaps(o)).then_some((b, o))
            })
            .collect();
        let needed = draws.len().max(1) as u64;
        if needed > gpu.capacity {
            gpu.capacity = needed.next_power_of_two();
            gpu.uniform = uniform(device, gpu.capacity);
            gpu.bind = bind(device, &gpu.layout, &gpu.uniform);
        }
        #[allow(clippy::cast_possible_truncation)] // a few blocks of 256 bytes
        let mut bytes = vec![0u8; (needed * STRIDE) as usize];
        for (k, (_, origin)) in draws.iter().enumerate() {
            let v = [
                at.centre[0],
                at.centre[1],
                at.reach,
                0.0,
                at.top,
                at.span,
                0.0,
                0.0,
                origin.x,
                origin.y,
                origin.z,
                0.0,
            ];
            #[allow(clippy::cast_possible_truncation)] // 256
            let start = k * STRIDE as usize;
            for (i, f) in v.iter().enumerate() {
                bytes[start + i * 4..start + i * 4 + 4].copy_from_slice(&f.to_le_bytes());
            }
        }
        queue.write_buffer(&gpu.uniform, 0, &bytes);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("weather shelter"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &gpu.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: timer.writes("weather shelter"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.pipeline);
            for (k, (block, _)) in draws.iter().enumerate() {
                let Some(buffer) = block.buffer.as_ref() else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)] // a few blocks' offsets
                let offset = (k as u64 * STRIDE) as u32;
                pass.set_bind_group(0, &gpu.bind, &[offset]);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..block.vertices, 0..1);
            }
        }
        gpu.view.clone()
    }
}

fn uniform(device: &wgpu::Device, blocks: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weather shelter"),
        size: blocks * STRIDE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("weather shelter"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: wgpu::BufferSize::new(48),
            }),
        }],
    })
}

impl Gpu {
    fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather shelter"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(48),
                },
                count: None,
            }],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("weather shelter"),
            source: wgpu::ShaderSource::Wgsl(COVER_WGSL.into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather shelter"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("weather shelter"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weather shelter"),
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = uniform(device, 4);
        let bind = bind(device, &layout, &uniform);
        Self {
            layout,
            pipeline,
            uniform,
            bind,
            capacity: 4,
            _texture: texture,
            view,
        }
    }
}

/// The shader of the map, for validation without a device.
#[must_use]
pub fn shader_source() -> &'static str {
    COVER_WGSL
}
