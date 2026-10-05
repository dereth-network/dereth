//! Landscape surfaces on the device: the composite and the splat.
//!
//! **The composite** is the shared compute shader the Vulkan device runs: the source images
//! (terrain tiles and alpha maps) live in one storage buffer, the pool, uploaded once each; a job
//! names which of them make one cell's surface; one dispatch writes the composite's texels, which
//! are copied into level 0 of a new texture and given their sublevels as an uploaded composite
//! is. Every step is integer, so the texels are bit-identical to the CPU compositor's. The one
//! device-specific value is the output row stride: a buffer-to-texture copy needs rows a multiple
//! of 256 bytes apart, so the stride is a job word rather than the composite's width.
//!
//! **The splat** draws a landscape cell by blending its layers in the pixel shader instead, with
//! the shared splat shader. Its layer constants live in a small uniform buffer in the layers'
//! binding rather than in push constants, which `wgpu` has only as a native extension, and each
//! sample takes the bias of the bank's wrap sampler, because a `wgpu` sampler carries none.

use crate::device::terrain::{pack_job, JOB_WORDS, MAX_OVERLAYS};
use std::collections::HashMap;

use super::{
    build_pipeline, extent, shader_source, DrawConstants, Gpu, MergeSource, PerDrawConstants,
    PerFrameConstants, PipelineKey, TerrainMergeJob, TerrainSplat, TextureData, TextureFormat,
    TextureKey, TextureSlot, TextureSpace, VertexFormat,
};
use crate::wgsl::{bias_splat_samples, TERRAIN_MERGE, TERRAIN_SPLAT_TAIL};
use crate::RenderError;

/// The pool's first size. It doubles when a source does not fit.
const INITIAL_POOL_BYTES: u64 = 16 << 20;

/// The most overlays one splat draw blends.
const MAX_LAYERS: usize = 5;
/// Texture bindings in group 4: the base, five alpha maps, five tiles.
const IMAGES: usize = 1 + 2 * MAX_LAYERS;
/// The layer constants: one `vec4<u32>` plus one per overlay.
const SPLAT_WORDS: usize = 4 * (1 + MAX_LAYERS);
/// Group 4's bindings after the textures: the two samplers, then the layer constants.
const WRAP_BINDING: u32 = 11;
const CONSTANTS_BINDING: u32 = 13;

/// The composite's pipeline and buffers.
pub(super) struct TerrainMerge {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    pool: wgpu::Buffer,
    /// Bytes of `pool` in use.
    pool_used: u64,
    /// Each source's `(word offset, width, height)`, indexed by [`MergeSource`].
    sources: Vec<(u32, u32, u32)>,
    params: wgpu::Buffer,
    output: wgpu::Buffer,
    /// The bind group over the current three buffers; `None` once one of them is replaced.
    bind: Option<wgpu::BindGroup>,
}

/// The splat's pipelines and its binding cache.
pub(super) struct TerrainSplatState {
    module_by_format: HashMap<VertexFormat, wgpu::ShaderModule>,
    group_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    /// One pipeline per terrain pipeline key, as an index into the device's pipelines.
    pipelines: HashMap<PipelineKey, usize>,
    /// One binding per distinct combination of bound textures (by slot, `u32::MAX` for the
    /// stand-in), of the two samplers, and of the layer constants.
    groups: HashMap<([u32; IMAGES], u32, u32, [u32; SPLAT_WORDS]), wgpu::BindGroup>,
    /// A 1 x 1 texture bound wherever a draw has nothing to bind.
    stand_in: TextureSlot,
}

impl Gpu {
    /// Whether this device composes landscape surfaces itself: it needs compute shaders with
    /// storage buffers, which WebGL 2 does not have.
    #[must_use]
    pub fn terrain_merge_supported(&self) -> bool {
        self.compute
    }

    /// Make a decoded BGRA8 source image resident for [`Self::merge_terrain_texture`].
    ///
    /// # Errors
    /// A byte count that does not match the extent, a pool past the device's storage limit, or
    /// no compute on this device.
    pub fn upload_merge_source(
        &mut self,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> Result<MergeSource, RenderError> {
        let len = u64::from(width) * u64::from(height) * 4;
        if width == 0 || height == 0 || bgra.len() as u64 != len {
            return Err(RenderError::BadDimensions {
                width,
                height,
                reason: "a merge source is width x height BGRA8 texels",
            });
        }
        let mut tm = self.take_terrain_merge()?;
        let result = self.upload_merge_source_into(&mut tm, width, height, bgra);
        self.terrain_merge = Some(tm);
        result
    }

    /// Forget every resident source. Their pool space is reused by the next uploads; handles
    /// taken before this call must not be used after it.
    pub fn reset_merge_sources(&mut self) {
        if let Some(tm) = self.terrain_merge.as_mut() {
            tm.pool_used = 0;
            tm.sources.clear();
        }
    }

    /// Compose one landscape surface on the device and make it a texture, exactly as uploading
    /// the CPU composite through [`Self::upload_imgtex_keyed`] would: same level count, same
    /// sublevel generation, one link owned by the caller.
    ///
    /// # Errors
    /// A key that is not a world owner's, a job naming a source this device does not hold or
    /// more overlays than the shader takes, or no compute on this device.
    pub fn merge_terrain_texture(
        &mut self,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        if key.space() != TextureSpace::World {
            return Err(RenderError::Unsupported(
                "runtime image texture mips require a world-owner key",
            ));
        }
        if let Some(slot) = self.texture_book.get(key) {
            return Ok(TextureSlot(slot));
        }
        if job.size == 0 {
            return Err(RenderError::BadDimensions {
                width: 0,
                height: 0,
                reason: "an empty terrain composite",
            });
        }
        if job.overlays.len() > MAX_OVERLAYS {
            return Err(RenderError::Unsupported(
                "too many overlays in one terrain composite",
            ));
        }
        let mut tm = self.take_terrain_merge()?;
        let result = self.merge_into_texture(&mut tm, key, job);
        self.terrain_merge = Some(tm);
        result
    }

    fn take_terrain_merge(&mut self) -> Result<TerrainMerge, RenderError> {
        if !self.compute {
            return Err(RenderError::Unsupported(
                "this device has no compute shaders",
            ));
        }
        Ok(match self.terrain_merge.take() {
            Some(tm) => tm,
            None => self.create_terrain_merge(),
        })
    }

    fn create_terrain_merge(&mut self) -> TerrainMerge {
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain merge"),
                source: wgpu::ShaderSource::Wgsl(TERRAIN_MERGE.into()),
            });
        let storage = |binding: u32, read_only: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("terrain merge"),
                entries: &[storage(0, true), storage(1, true), storage(2, false)],
            });
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain merge"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("terrain merge"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("cs_merge"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        let pool = self.merge_buffer(INITIAL_POOL_BYTES, "terrain merge sources");
        let params = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain merge job"),
            size: (JOB_WORDS * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let output = self.merge_output(512 * 512 * 4);
        TerrainMerge {
            pipeline,
            layout,
            pool,
            pool_used: 0,
            sources: Vec::new(),
            params,
            output,
            bind: None,
        }
    }

    fn merge_buffer(&self, size: u64, label: &str) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    fn merge_output(&self, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain merge output"),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    fn upload_merge_source_into(
        &mut self,
        tm: &mut TerrainMerge,
        width: u32,
        height: u32,
        bgra: &[u8],
    ) -> Result<MergeSource, RenderError> {
        let len = bgra.len() as u64;
        let need = tm.pool_used + len;
        if need > tm.pool.size() {
            // Grow: a new pool at least twice the size, the used part copied across, but never past
            // what the device can bind as one storage buffer.
            let limit = self
                .device
                .limits()
                .max_storage_buffer_binding_size
                .min(self.device.limits().max_buffer_size);
            if need > limit {
                return Err(RenderError::Unsupported(
                    "terrain merge sources exceed this device's storage-buffer range",
                ));
            }
            let size = need.next_power_of_two().max(tm.pool.size() * 2).min(limit);
            let bigger = self.merge_buffer(size, "terrain merge sources");
            if tm.pool_used > 0 {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("terrain merge pool grow"),
                        });
                encoder.copy_buffer_to_buffer(&tm.pool, 0, &bigger, 0, tm.pool_used);
                self.queue.submit([encoder.finish()]);
            }
            tm.pool = bigger;
            tm.bind = None;
        }
        self.queue.write_buffer(&tm.pool, tm.pool_used, bgra);
        let index = u32::try_from(tm.sources.len())
            .map_err(|_| RenderError::Unsupported("too many merge sources"))?;
        let offset = u32::try_from(tm.pool_used / 4)
            .map_err(|_| RenderError::Unsupported("terrain merge pool too large"))?;
        tm.sources.push((offset, width, height));
        tm.pool_used += len;
        Ok(MergeSource(index))
    }

    fn merge_into_texture(
        &mut self,
        tm: &mut TerrainMerge,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        let source = |s: MergeSource| {
            tm.sources
                .get(s.0 as usize)
                .copied()
                .ok_or(RenderError::Unsupported(
                    "a merge source this device does not hold",
                ))
        };
        let size = job.size;
        // Rows a multiple of 256 bytes apart, for the copy into the texture.
        let stride = (size * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) / 4;
        let words = pack_job(job, stride, source)?;
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.queue.write_buffer(&tm.params, 0, &bytes);
        let out_len = u64::from(stride) * u64::from(size) * 4;
        if out_len > tm.output.size() {
            tm.output = self.merge_output(out_len);
            tm.bind = None;
        }
        let bind = tm
            .bind
            .get_or_insert_with(|| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("terrain merge"),
                    layout: &tm.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: tm.pool.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: tm.params.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: tm.output.as_entire_binding(),
                        },
                    ],
                })
            })
            .clone();
        // The same level count an uploaded one-level BGRA8 composite gets.
        let shape = TextureData {
            width: size,
            height: size,
            format: TextureFormat::Bgra8,
            levels: vec![Vec::new()],
        };
        let levels = u32::try_from(crate::mip::runtime_level_count(&shape, true)).unwrap_or(1);
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("terrain composite"),
            size: extent(size, size),
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain merge"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("terrain merge"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&tm.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            let groups = size.div_ceil(8);
            pass.dispatch_workgroups(groups, groups, 1);
        }
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &tm.output,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride * 4),
                    rows_per_image: Some(size),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            extent(size, size),
        );
        self.queue.submit([encoder.finish()]);
        self.generate_levels(&texture, format, 1, levels);
        self.register(key, texture, TextureFormat::Bgra8, levels)
    }

    /// Whether [`Self::draw_terrain_splat`] is available: the pipeline binds five groups (the
    /// legacy four and the layers), one more than `wgpu`'s default limit.
    #[must_use]
    pub fn terrain_splat_supported(&self) -> bool {
        self.max_bind_groups >= 5
    }

    /// Draw landscape triangles with `key`'s fixed-function state, blending `splat`'s layers in
    /// the pixel shader. Otherwise exactly [`Self::draw_dynamic`].
    ///
    /// # Errors
    /// More overlays than the shader takes, a texture slot that is not live, a device with too
    /// few bind groups, or whole vertices missing.
    pub fn draw_terrain_splat(
        &mut self,
        key: &PipelineKey,
        splat: &TerrainSplat,
        per_frame: &PerFrameConstants,
        per_draw: &PerDrawConstants,
        vertices: &[u8],
    ) -> Result<(), RenderError> {
        let stride = key.vertex_format.stride();
        if stride == 0 || !vertices.len().is_multiple_of(stride as usize) {
            return Err(RenderError::Device(
                "vertex data is not a whole number of vertices".into(),
            ));
        }
        if splat.overlays.len() > MAX_LAYERS {
            return Err(RenderError::Unsupported(
                "too many overlays in one splat draw",
            ));
        }
        if !self.terrain_splat_supported() {
            return Err(RenderError::Unsupported(
                "the landscape splat needs five bind groups",
            ));
        }
        if vertices.is_empty() || !self.frame_open {
            return Ok(());
        }
        let mut state = match self.terrain_splat.take() {
            Some(s) => s,
            None => self.create_terrain_splat()?,
        };
        let prepared = self.prepare_splat(&mut state, key, splat);
        self.terrain_splat = Some(state);
        let (pipeline, group) = prepared?;
        self.commands.get_mut().push(super::Cmd::Bind {
            group: 4,
            bind: group,
        });
        let bias = self.sampler_biases[self.sampler_descriptor_index(0) as usize];
        self.record_draw(
            pipeline,
            key,
            &DrawConstants::default(),
            per_frame,
            per_draw,
            vertices,
            bias,
        );
        Ok(())
    }

    /// Drop every cached splat binding. A binding holds its textures alive, so call it before
    /// releasing a texture a splat draw bound.
    pub fn reset_terrain_splat(&mut self) {
        if let Some(state) = self.terrain_splat.as_mut() {
            state.groups.clear();
        }
    }

    fn create_terrain_splat(&mut self) -> Result<TerrainSplatState, RenderError> {
        let texture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..WRAP_BINDING).map(texture).collect();
        for i in 0..2u32 {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: WRAP_BINDING + i,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: CONSTANTS_BINDING,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new((SPLAT_WORDS * 4) as u64),
            },
            count: None,
        });
        let group_layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("splat layers"),
                entries: &entries,
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("splat"),
                bind_group_layouts: &[
                    Some(&self.uniform_layout),
                    Some(&self.texture_layout),
                    Some(&self.texture_layout),
                    Some(&self.sampler_layout),
                    Some(&group_layout),
                ],
                immediate_size: 0,
            });
        let stand_in = self.upload_texture(&TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0, 0, 0, 0]],
        })?;
        Ok(TerrainSplatState {
            module_by_format: HashMap::new(),
            group_layout,
            layout,
            pipelines: HashMap::new(),
            groups: HashMap::new(),
            stand_in,
        })
    }

    /// The pipeline for `key` and the layers' binding for `splat`, made on first use.
    fn prepare_splat(
        &mut self,
        state: &mut TerrainSplatState,
        key: &PipelineKey,
        splat: &TerrainSplat,
    ) -> Result<(usize, wgpu::BindGroup), RenderError> {
        let pipeline = if let Some(&p) = state.pipelines.get(key) {
            p
        } else {
            let module = state
                .module_by_format
                .entry(key.vertex_format)
                .or_insert_with(|| {
                    self.device
                        .create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some("splat"),
                            source: wgpu::ShaderSource::Wgsl(
                                splat_source(key.vertex_format).into(),
                            ),
                        })
                })
                .clone();
            let p = build_pipeline(
                &self.device,
                &state.layout,
                &module,
                "ps_splat",
                key,
                self.format,
            );
            self.pipelines.push(p);
            let i = self.pipelines.len() - 1;
            state.pipelines.insert(*key, i);
            i
        };
        let mut slots = [u32::MAX; IMAGES];
        slots[0] = splat.base.map_or(u32::MAX, |s| s.0);
        for (k, o) in splat.overlays.iter().enumerate() {
            slots[1 + k] = o.alpha.0;
            slots[1 + MAX_LAYERS + k] = o.tex.map_or(u32::MAX, |s| s.0);
        }
        let mut words = [0u32; SPLAT_WORDS];
        words[0] = u32::from(splat.base.is_some());
        words[1] = splat.base_tiling;
        words[2] = u32::try_from(splat.overlays.len()).unwrap_or(0);
        for (k, o) in splat.overlays.iter().enumerate() {
            words[4 + k * 4] = o.rotation;
            words[4 + k * 4 + 1] = o.tiling;
            words[4 + k * 4 + 2] = u32::from(o.tex.is_some());
        }
        // The bank's wrap/wrap and clamp/clamp entries under the current filtering preference:
        // the terrain tiles repeat across the cell, the alpha maps cover it once.
        let wrap = self.sampler_descriptor_index(0);
        let clamp = self.sampler_descriptor_index(1);
        let cache_key = (slots, wrap, clamp, words);
        if let Some(group) = state.groups.get(&cache_key) {
            return Ok((pipeline, group.clone()));
        }
        let views = slots
            .iter()
            .map(|&slot| {
                let slot = if slot == u32::MAX {
                    state.stand_in.0
                } else {
                    slot
                };
                self.textures
                    .get(&slot)
                    .map(|t| {
                        t.texture
                            .create_view(&wgpu::TextureViewDescriptor::default())
                    })
                    .ok_or(RenderError::Unsupported(
                        "a splat texture slot that is not live",
                    ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let constants = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("splat layers"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&constants, 0, &bytes);
        let mut entries: Vec<wgpu::BindGroupEntry<'_>> = (0..)
            .zip(&views)
            .map(|(b, v)| wgpu::BindGroupEntry {
                binding: b,
                resource: wgpu::BindingResource::TextureView(v),
            })
            .collect();
        for (i, s) in (WRAP_BINDING..).zip([wrap, clamp]) {
            entries.push(wgpu::BindGroupEntry {
                binding: i,
                resource: wgpu::BindingResource::Sampler(&self.samplers[s as usize]),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: CONSTANTS_BINDING,
            resource: constants.as_entire_binding(),
        });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("splat layers"),
            layout: &state.group_layout,
            entries: &entries,
        });
        state.groups.insert(cache_key, group.clone());
        Ok((pipeline, group))
    }
}

/// The splat shader for `format`: this device's legacy source and the shared splat tail, its
/// constants read from the layers' binding and its samples biased as the legacy ones are.
fn splat_source(format: VertexFormat) -> String {
    let tail = bias_splat_samples(TERRAIN_SPLAT_TAIL).replace(
        "var<immediate> g_splat: Splat;",
        &format!("@group(4) @binding({CONSTANTS_BINDING}) var<uniform> g_splat: Splat;"),
    );
    shader_source(format) + &tail
}

#[cfg(test)]
mod tests {
    use super::super::tests::{device, pixel, quad, screen_key};
    use super::*;
    use crate::device::{TerrainMergeOverlay, TerrainSplatOverlay};
    use crate::ZFunc;

    fn solid(width: u32, height: u32, bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat((width * height) as usize)
    }

    /// A composite whose rows do not fill the copy alignment keeps its texels where they belong,
    /// and every generated sublevel of a solid composite is that colour.
    #[test]
    fn a_composite_off_the_copy_alignment_is_solid_at_every_level() {
        let Some(mut gpu) = device(16, 16) else {
            return;
        };
        if !gpu.terrain_merge_supported() {
            eprintln!("skipped: no compute on this adapter");
            return;
        }
        let colour = [10, 120, 230, 255];
        let base = gpu
            .upload_merge_source(8, 8, &solid(8, 8, colour))
            .expect("a source");
        let job = TerrainMergeJob {
            size: 24,
            base: Some(base),
            base_tiling: 1,
            overlays: Vec::new(),
        };
        let slot = gpu
            .merge_terrain_texture(TextureKey::world(0x7F00_0001), &job)
            .expect("a composite");
        let levels = gpu.texture_mip_levels(slot).expect("resident");
        assert!(levels > 1, "the composite gets sublevels");
        for level in 0..levels {
            let image = gpu.capture_texture_level(slot, level).expect("read back");
            assert!(
                image.bgra.chunks(4).all(|p| p == colour),
                "level {level} is the base colour"
            );
        }
    }

    /// An overlay whose alpha map is clear takes the tile everywhere; one whose alpha map is
    /// opaque keeps the base.
    #[test]
    fn an_overlay_takes_the_tile_where_its_alpha_is_clear() {
        let Some(mut gpu) = device(16, 16) else {
            return;
        };
        if !gpu.terrain_merge_supported() {
            eprintln!("skipped: no compute on this adapter");
            return;
        }
        let (under, over) = ([0, 0, 255, 255], [255, 0, 0, 255]);
        let base = gpu
            .upload_merge_source(4, 4, &solid(4, 4, under))
            .expect("base");
        let tile = gpu
            .upload_merge_source(4, 4, &solid(4, 4, over))
            .expect("tile");
        let clear = gpu
            .upload_merge_source(4, 4, &solid(4, 4, [0; 4]))
            .expect("alpha");
        let opaque = gpu
            .upload_merge_source(4, 4, &solid(4, 4, [0, 0, 0, 255]))
            .expect("alpha");
        for (alpha, (want, key)) in [clear, opaque].into_iter().zip([(over, 2), (under, 3)]) {
            let job = TerrainMergeJob {
                size: 8,
                base: Some(base),
                base_tiling: 1,
                overlays: vec![TerrainMergeOverlay {
                    alpha,
                    rotation: 0,
                    tex: Some(tile),
                    tiling: 1,
                }],
            };
            let slot = gpu
                .merge_terrain_texture(TextureKey::world(0x7F00_0000 + key), &job)
                .expect("a composite");
            let image = gpu.capture_texture_level(slot, 0).expect("read back");
            assert!(image.bgra.chunks(4).all(|p| p == want));
        }
    }

    /// A splat with a base alone draws the base, and the pipeline and binding are made once.
    #[test]
    fn a_splat_of_a_base_alone_draws_the_base() {
        let Some(mut gpu) = device(16, 16) else {
            return;
        };
        if !gpu.terrain_splat_supported() {
            eprintln!("skipped: the adapter binds fewer than five groups");
            return;
        }
        let base = gpu
            .upload_texture(&TextureData {
                width: 2,
                height: 2,
                format: TextureFormat::Bgra8,
                levels: vec![solid(2, 2, [0, 200, 0, 255])],
            })
            .expect("a texture");
        let alpha = gpu
            .upload_texture(&TextureData {
                width: 2,
                height: 2,
                format: TextureFormat::Bgra8,
                levels: vec![solid(2, 2, [0, 0, 0, 255])],
            })
            .expect("a texture");
        let splat = TerrainSplat {
            base: Some(base),
            base_tiling: 1,
            overlays: vec![TerrainSplatOverlay {
                alpha,
                rotation: 1,
                tex: None,
                tiling: 1,
            }],
        };
        let key = screen_key(ZFunc::Always);
        let frame = PerFrameConstants::default();
        let draw = PerDrawConstants::default();
        for _ in 0..2 {
            gpu.begin_frame().expect("frame");
            gpu.draw_terrain_splat(
                &key,
                &splat,
                &frame,
                &draw,
                &quad(0.0, 0.0, 16.0, 16.0, 0.5, 0xFFFF_FFFF),
            )
            .expect("a splat draw");
            gpu.end_frame().expect("present");
        }
        let image = gpu.capture().expect("capture");
        assert_eq!(&pixel(&image, 8, 8)[..3], &[0, 200, 0]);
        let state = gpu.terrain_splat.as_ref().expect("made");
        assert_eq!((state.pipelines.len(), state.groups.len()), (1, 1));
        gpu.reset_terrain_splat();
        assert!(gpu.terrain_splat.as_ref().expect("kept").groups.is_empty());
    }

    /// Every splat substitution lands.
    #[test]
    fn the_splat_shader_takes_every_substitution() {
        let source = splat_source(VertexFormat::XyzRhwDiffuseTex1);
        assert!(!source.contains("var<immediate>"));
        assert!(!source.contains("textureSample(alpha"));
        assert!(!source.contains("textureSample(tile"));
        assert!(!source.contains("textureSample(s_base"));
        assert!(source.contains("var<uniform> g_splat"));
    }
}
