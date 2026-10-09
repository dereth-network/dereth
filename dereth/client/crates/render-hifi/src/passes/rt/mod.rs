//! The traced scene, where the device has ray queries: what the lamps' light is traced through.
//!
//! [`RtScene`] keeps one bottom-level acceleration structure per resident block (its landscape
//! and opaque static geometry) and one per block for its cut-out surfaces (leaves), each built
//! once per block generation, and a top-level structure over them rebuilt every frame. Cut-out
//! surfaces are tested against their texture's alpha, kept in a small atlas.

use std::collections::HashMap;

use glam::Vec3;

use crate::HifiFrame;

const ATLAS: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const ATLAS_SIZE: u32 = 256;
const ATLAS_LAYERS: u32 = 256;
const MAX_INSTANCES: u32 = 4096;

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// One geometry of a block's structure: its vertex buffer and count.
#[derive(Debug)]
struct Geometry {
    buffer: wgpu::Buffer,
    size: wgpu::BlasTriangleGeometrySizeDescriptor,
}

/// A cut-out geometry's alpha test: its texture slot, its cut and its texture coordinates.
#[derive(Debug)]
struct Cut {
    slot: u32,
    alpha_ref: u8,
    uvs: Vec<[f32; 2]>,
}

#[derive(Debug)]
struct Structure {
    blas: wgpu::Blas,
    geometries: Vec<Geometry>,
}

#[derive(Debug)]
struct RtBlock {
    generation: u32,
    opaque: Option<Structure>,
    cut: Option<(Structure, Vec<Cut>)>,
    built: bool,
}

fn structure(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    parts: &[Vec<f32>],
    opaque: bool,
) -> Option<Structure> {
    let flags = if opaque {
        wgpu::AccelerationStructureGeometryFlags::OPAQUE
    } else {
        wgpu::AccelerationStructureGeometryFlags::empty()
    };
    let mut geometries = Vec::new();
    for p in parts {
        #[allow(clippy::cast_possible_truncation)]
        let vertices = (p.len() / 3) as u32;
        if vertices < 3 {
            continue;
        }
        let bytes = floats(p);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("high-fidelity traced geometry"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::BLAS_INPUT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, &bytes);
        geometries.push(Geometry {
            buffer,
            size: wgpu::BlasTriangleGeometrySizeDescriptor {
                vertex_format: wgpu::VertexFormat::Float32x3,
                vertex_count: vertices,
                index_format: None,
                index_count: None,
                flags,
            },
        });
    }
    if geometries.is_empty() {
        return None;
    }
    let blas = device.create_blas(
        &wgpu::CreateBlasDescriptor {
            label: Some("high-fidelity block structure"),
            flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
            update_mode: wgpu::AccelerationStructureUpdateMode::Build,
        },
        wgpu::BlasGeometrySizeDescriptors::Triangles {
            descriptors: geometries.iter().map(|g| g.size.clone()).collect(),
        },
    );
    Some(Structure { blas, geometries })
}

fn batch_positions(batch: &crate::snapshot::HifiStaticBatch) -> (Vec<f32>, Vec<[f32; 2]>) {
    let stride = batch.format.stride() as usize;
    let count = batch.bytes.len() / stride;
    let usable = count - count % 3;
    let uv = if crate::derive::reshade::has_normal(batch.format) {
        28
    } else {
        16
    };
    let mut p = Vec::with_capacity(usable * 3);
    let mut t = Vec::with_capacity(usable);
    for v in 0..usable {
        let at = v * stride;
        p.extend_from_slice(&[
            read_f32(&batch.bytes, at),
            read_f32(&batch.bytes, at + 4),
            read_f32(&batch.bytes, at + 8),
        ]);
        if batch.cutout && at + uv + 8 <= batch.bytes.len() {
            t.push([
                read_f32(&batch.bytes, at + uv),
                read_f32(&batch.bytes, at + uv + 4),
            ]);
        }
    }
    (p, t)
}

#[derive(Debug)]
struct Gpu {
    blit_layout: wgpu::BindGroupLayout,
    blit: wgpu::RenderPipeline,
    atlas: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    blit_sampler: wgpu::Sampler,
}

const BLIT_WGSL: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: V;
    o.p = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}
@fragment fn fs(v: V) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(src, smp, v.uv, 0.0).a, 0.0, 0.0, 1.0);
}
";

impl Gpu {
    #[allow(clippy::too_many_lines)]
    fn new(device: &wgpu::Device) -> Self {
        let d2 = wgpu::TextureViewDimension::D2;
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity alpha atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: d2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bmodule = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity alpha atlas"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let bpl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity alpha atlas"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("high-fidelity alpha atlas"),
            layout: Some(&bpl),
            vertex: wgpu::VertexState {
                module: &bmodule,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &bmodule,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ATLAS,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("high-fidelity alpha atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: ATLAS_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ATLAS,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..wgpu::TextureViewDescriptor::default()
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity alpha atlas"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("high-fidelity alpha atlas source"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });
        Self {
            blit_layout,
            blit,
            atlas,
            atlas_view,
            sampler,
            blit_sampler,
        }
    }
}

/// The traced scene, kept across frames.
#[derive(Debug, Default)]
pub struct RtScene {
    gpu: Option<Gpu>,
    blocks: HashMap<u16, RtBlock>,
    tlas: Option<wgpu::Tlas>,
    layers: HashMap<u32, u32>,
    /// Atlas layers allocated but not yet drawn.
    pending_layers: Vec<(u32, u32)>,
    /// The cut table and texture coordinates, rebuilt when the resident set changes.
    cut_buffers: Option<(wgpu::Buffer, wgpu::Buffer)>,
    cut_bases: HashMap<u16, u32>,
    dirty: bool,
    /// Whether the structures are built for this frame.
    ready: bool,
}

/// What a traced pass binds to trace the scene.
#[derive(Debug)]
pub struct SceneBindings<'a> {
    /// The top-level structure.
    pub tlas: &'a wgpu::Tlas,
    /// The cut-out alpha atlas and its sampler.
    pub atlas: &'a wgpu::TextureView,
    pub sampler: &'a wgpu::Sampler,
    /// The cut table and texture coordinates.
    pub cuts: &'a wgpu::Buffer,
    pub uvs: &'a wgpu::Buffer,
}

impl RtScene {
    /// Takes in the frame's new or changed block geometry.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &HifiFrame) {
        for block in &frame.static_feed {
            if self
                .blocks
                .get(&block.block)
                .is_some_and(|b| b.generation == block.generation)
            {
                continue;
            }
            let mut terrain = Vec::with_capacity(block.terrain_triangles.len() * 9);
            for t in &block.terrain_triangles {
                for p in t {
                    terrain.extend_from_slice(&[p.x, p.y, p.z]);
                }
            }
            let mut opaque_parts = vec![terrain];
            let mut cut_parts = Vec::new();
            let mut cuts = Vec::new();
            for batch in &block.batches {
                if batch.format.is_pre_transformed() {
                    continue;
                }
                let (p, uvs) = batch_positions(batch);
                match (batch.cutout, batch.texture) {
                    (true, Some(slot)) if uvs.len() * 3 == p.len() && p.len() >= 9 => {
                        cut_parts.push(p);
                        cuts.push(Cut {
                            slot,
                            alpha_ref: batch.alpha_ref,
                            uvs,
                        });
                    }
                    (true, _) => {}
                    _ => opaque_parts.push(p),
                }
            }
            // One opaque geometry per block is quicker to trace than many small ones.
            let merged: Vec<f32> = opaque_parts.concat();
            let opaque = structure(device, queue, &[merged], true);
            let cut = structure(device, queue, &cut_parts, false).map(|s| (s, cuts));
            if let Some((_, cuts)) = &cut {
                for c in cuts {
                    if !self.layers.contains_key(&c.slot) {
                        #[allow(clippy::cast_possible_truncation)]
                        let layer = self.layers.len() as u32;
                        if layer < ATLAS_LAYERS {
                            self.layers.insert(c.slot, layer);
                            self.pending_layers.push((c.slot, layer));
                        }
                    }
                }
            }
            self.blocks.insert(
                block.block,
                RtBlock {
                    generation: block.generation,
                    opaque,
                    cut,
                    built: false,
                },
            );
            self.dirty = true;
        }
        let before = self.blocks.len();
        self.blocks
            .retain(|id, _| frame.terrain.iter().any(|t| t.block == *id));
        if self.blocks.len() != before {
            self.dirty = true;
        }
    }

    /// The scene's bindings, once [`Self::build`] has built it this frame.
    #[must_use]
    pub fn bindings(&self) -> Option<SceneBindings<'_>> {
        if !self.ready {
            return None;
        }
        let gpu = self.gpu.as_ref()?;
        let (cuts, uvs) = self.cut_buffers.as_ref()?;
        Some(SceneBindings {
            tlas: self.tlas.as_ref()?,
            atlas: &gpu.atlas_view,
            sampler: &gpu.sampler,
            cuts,
            uvs,
        })
    }

    /// Builds the frame's structures: new blocks' bottom levels, the alpha atlas, the cut table
    /// and the top level. Returns whether there is a scene to trace.
    #[allow(clippy::too_many_lines)]
    pub fn build(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        seam: &crate::SidecarContext<'_>,
        frame: &HifiFrame,
    ) -> bool {
        self.ready = false;
        if self.blocks.is_empty() {
            return false;
        }
        let gpu = self.gpu.get_or_insert_with(|| Gpu::new(device));

        // --- the alpha atlas ---------------------------------------------------------------
        let mut still = Vec::new();
        for (slot, layer) in std::mem::take(&mut self.pending_layers) {
            let Some(t) = seam.texture(dereth_render::device::TextureSlot(slot)) else {
                still.push((slot, layer));
                continue;
            };
            let src = t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: 0,
                array_layer_count: Some(1),
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..wgpu::TextureViewDescriptor::default()
            });
            let dst = gpu.atlas.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..wgpu::TextureViewDescriptor::default()
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("high-fidelity alpha atlas"),
                layout: &gpu.blit_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&gpu.blit_sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("alpha atlas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&gpu.blit);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.pending_layers = still;

        // --- the cut table -------------------------------------------------------------------
        if self.dirty || self.cut_buffers.is_none() {
            let mut table: Vec<u32> = Vec::new();
            let mut uvs: Vec<f32> = Vec::new();
            self.cut_bases.clear();
            for (id, b) in &self.blocks {
                let Some((_, cuts)) = &b.cut else { continue };
                #[allow(clippy::cast_possible_truncation)]
                self.cut_bases.insert(*id, (table.len() / 4) as u32);
                for c in cuts {
                    #[allow(clippy::cast_possible_truncation)]
                    let first = (uvs.len() / 2) as u32;
                    for t in &c.uvs {
                        uvs.extend_from_slice(t);
                    }
                    let layer = self.layers.get(&c.slot).copied().unwrap_or(0);
                    table.extend_from_slice(&[first, layer, u32::from(c.alpha_ref), 0]);
                }
            }
            if table.is_empty() {
                table.extend_from_slice(&[0; 4]);
            }
            if uvs.is_empty() {
                uvs.extend_from_slice(&[0.0; 2]);
            }
            let mk = |label: &str, bytes: &[u8]| {
                let b = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: bytes.len() as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                queue.write_buffer(&b, 0, bytes);
                b
            };
            let tb: Vec<u8> = table.iter().flat_map(|x| x.to_le_bytes()).collect();
            self.cut_buffers = Some((
                mk("high-fidelity cut table", &tb),
                mk("high-fidelity cut coordinates", &floats(&uvs)),
            ));
            self.dirty = false;
        }

        // --- the structures ------------------------------------------------------------------
        let tlas = self.tlas.get_or_insert_with(|| {
            device.create_tlas(&wgpu::CreateTlasDescriptor {
                label: Some("high-fidelity scene"),
                max_instances: MAX_INSTANCES,
                flags: wgpu::AccelerationStructureFlags::PREFER_FAST_TRACE,
                update_mode: wgpu::AccelerationStructureUpdateMode::Build,
            })
        });
        let origins: HashMap<u16, Vec3> =
            frame.terrain.iter().map(|t| (t.block, t.origin)).collect();
        let mut n = 0usize;
        for (id, b) in &self.blocks {
            let Some(o) = origins.get(id) else { continue };
            let transform = [1.0, 0.0, 0.0, o.x, 0.0, 1.0, 0.0, o.y, 0.0, 0.0, 1.0, o.z];
            if let Some(s) = &b.opaque {
                if n < MAX_INSTANCES as usize {
                    tlas[n] = Some(wgpu::TlasInstance::new(&s.blas, transform, 0, 0x1));
                    n += 1;
                }
            }
            if let Some((s, _)) = &b.cut {
                let base = self.cut_bases.get(id).copied().unwrap_or(0);
                if n < MAX_INSTANCES as usize {
                    tlas[n] = Some(wgpu::TlasInstance::new(&s.blas, transform, base, 0x2));
                    n += 1;
                }
            }
        }
        for i in n..MAX_INSTANCES as usize {
            if tlas[i].is_none() {
                break;
            }
            tlas[i] = None;
        }
        if n == 0 {
            return false;
        }
        let mut entries = Vec::new();
        for b in self.blocks.values().filter(|b| !b.built) {
            for s in b.opaque.iter().chain(b.cut.as_ref().map(|(s, _)| s)) {
                entries.push(wgpu::BlasBuildEntry {
                    blas: &s.blas,
                    geometry: wgpu::BlasGeometries::TriangleGeometries(
                        s.geometries
                            .iter()
                            .map(|g| wgpu::BlasTriangleGeometry {
                                size: &g.size,
                                vertex_buffer: &g.buffer,
                                first_vertex: 0,
                                vertex_stride: 12,
                                index_buffer: None,
                                first_index: None,
                                transform_buffer: None,
                                transform_buffer_offset: None,
                            })
                            .collect(),
                    ),
                });
            }
        }
        encoder.build_acceleration_structures(entries.iter(), std::iter::once(&*tlas));
        drop(entries);
        for b in self.blocks.values_mut() {
            b.built = true;
        }
        self.ready = true;
        true
    }
}

/// The traced scene's shader sources, for validation.
#[must_use]
pub fn shaders() -> [(&'static str, String); 1] {
    [("alpha atlas", BLIT_WGSL.to_owned())]
}
