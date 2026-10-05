//! Landscape surface composition on the device.
//!
//! The landscape draws each terrain cell with one composite texture: a base terrain image tiled
//! across it, then up to five overlays, each blended in through a rotated alpha map. The CPU
//! version of that composite is `dereth_world_render::land::merge::execute_merge_plan`; this is
//! the same arithmetic as a compute shader. Every step of it is integer -- nearest-neighbour
//! index arithmetic and an 8.8 fixed-point blend -- so the device's texels are **bit-identical**
//! to the CPU's, and the texture that results goes through exactly the same level-0 copy and
//! sublevel generation an uploaded composite does. The GPU test `terrain_gpu_merge` holds that.
//!
//! The source images (terrain tiles and alpha maps) are uploaded once each into one storage
//! buffer, the *pool*, and referenced from then on by [`MergeSource`]. They are few and shared
//! by every composite in the window, so after the first handful of cells a composite costs one
//! dispatch and no upload at all.

use super::*;
use crate::device::terrain::{pack_job, JOB_WORDS, MAX_OVERLAYS};
use crate::device::{MergeSource, TerrainMergeJob};
use crate::wgsl::TERRAIN_MERGE as SHADER;

/// The pool's first size. It doubles when a source does not fit.
const INITIAL_POOL_BYTES: u64 = 16 << 20;
/// The output buffer's first size: one 512 x 512 composite. It grows to the largest seen.
const INITIAL_OUTPUT_BYTES: u64 = 512 * 512 * 4;

/// The compute pipeline and its buffers. Built on the first use.
pub(super) struct TerrainMerge {
    module: vk::ShaderModule,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    desc_pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pool: Buffer,
    /// Bytes of `pool` in use.
    pool_used: u64,
    /// Each source's `(word offset, width, height)`, indexed by [`MergeSource`].
    sources: Vec<(u32, u32, u32)>,
    params: Buffer,
    output: Buffer,
    /// The descriptor set no longer names the current buffers.
    set_dirty: bool,
}

impl Gpu {
    /// Whether this device can compose landscape surfaces itself. It needs compute on the one
    /// queue this backend submits to.
    #[must_use]
    pub fn terrain_merge_supported(&self) -> bool {
        self.terrain_merge_supported
    }

    /// Make a decoded BGRA8 source image resident for [`Self::merge_terrain_texture`].
    ///
    /// # Errors
    /// A byte count that does not match the extent, or a device failure.
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
    /// more overlays than the shader takes, or a device failure.
    pub fn merge_terrain_texture(
        &mut self,
        key: TextureKey,
        job: &TerrainMergeJob,
    ) -> Result<TextureSlot, RenderError> {
        if key.space() != crate::TextureSpace::World {
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
        if !self.terrain_merge_supported {
            return Err(RenderError::Unsupported(
                "this device's queue has no compute",
            ));
        }
        match self.terrain_merge.take() {
            Some(tm) => Ok(tm),
            None => self.create_terrain_merge(),
        }
    }

    fn create_terrain_merge(&mut self) -> Result<TerrainMerge, RenderError> {
        let words = shaders::compile_entry(SHADER, naga::ShaderStage::Compute, "cs_merge")?;
        let module = Self::shader_module(&self.device, &words)?;
        let binding = |b: u32| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(b)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        };
        let bindings = [binding(0), binding(1), binding(2)];
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);
        // SAFETY: live locals; every handle made here is owned by the returned `TerrainMerge`
        // and destroyed in `destroy_terrain_merge`.
        let set_layout = vkr("vkCreateDescriptorSetLayout(terrain merge)", unsafe {
            self.device.create_descriptor_set_layout(&info, None)
        })?;
        let set_layouts = [set_layout];
        let info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
        // SAFETY: as above.
        let layout = vkr("vkCreatePipelineLayout(terrain merge)", unsafe {
            self.device.create_pipeline_layout(&info, None)
        })?;
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(c"cs_merge");
        let info = [vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(layout)];
        // SAFETY: as above.
        let pipeline = unsafe {
            self.device
                .create_compute_pipelines(vk::PipelineCache::null(), &info, None)
        }
        .map_err(|(_, e)| {
            RenderError::Device(format!("vkCreateComputePipelines(terrain merge): {e}"))
        })?[0];
        let sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: 3,
        }];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&sizes);
        // SAFETY: as above.
        let desc_pool = vkr("vkCreateDescriptorPool(terrain merge)", unsafe {
            self.device.create_descriptor_pool(&info, None)
        })?;
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(desc_pool)
            .set_layouts(&set_layouts);
        // SAFETY: as above.
        let set = vkr("vkAllocateDescriptorSets(terrain merge)", unsafe {
            self.device.allocate_descriptor_sets(&info)
        })?[0];
        let storage = vk::BufferUsageFlags::STORAGE_BUFFER;
        let pool = self.create_buffer(
            INITIAL_POOL_BYTES,
            storage | vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::GpuOnly,
            "terrain merge sources",
        )?;
        let params = self.create_buffer(
            (JOB_WORDS * 4) as u64,
            storage,
            MemoryLocation::CpuToGpu,
            "terrain merge job",
        )?;
        let output = self.create_buffer(
            INITIAL_OUTPUT_BYTES,
            storage | vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::GpuOnly,
            "terrain merge output",
        )?;
        Ok(TerrainMerge {
            module,
            set_layout,
            layout,
            pipeline,
            desc_pool,
            set,
            pool,
            pool_used: 0,
            sources: Vec::new(),
            params,
            output,
            set_dirty: true,
        })
    }

    pub(super) fn destroy_terrain_merge(&mut self, tm: TerrainMerge) {
        // SAFETY: the caller has waited for the device; nothing below is in flight.
        unsafe {
            self.device.destroy_pipeline(tm.pipeline, None);
            self.device.destroy_pipeline_layout(tm.layout, None);
            self.device.destroy_descriptor_pool(tm.desc_pool, None);
            self.device
                .destroy_descriptor_set_layout(tm.set_layout, None);
            self.device.destroy_shader_module(tm.module, None);
        }
        self.destroy_buffer(tm.pool);
        self.destroy_buffer(tm.params);
        self.destroy_buffer(tm.output);
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
        if need > tm.pool.size {
            // Grow: a new pool at least twice the size, the used part copied across, but never past
            // what the device can bind as one storage buffer.
            if need > self.max_storage_buffer_range {
                return Err(RenderError::Unsupported(
                    "terrain merge sources exceed this device's storage-buffer range",
                ));
            }
            let size = need
                .next_power_of_two()
                .max(tm.pool.size * 2)
                .min(self.max_storage_buffer_range);
            let bigger = self.create_buffer(
                size,
                vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuOnly,
                "terrain merge sources",
            )?;
            let (from, to, used) = (tm.pool.buffer, bigger.buffer, tm.pool_used);
            let copied = if used == 0 {
                Ok(())
            } else {
                self.one_shot("terrain merge pool grow", |device, cmd| {
                    // SAFETY: both buffers are live until after the wait inside `one_shot`.
                    unsafe {
                        device.cmd_copy_buffer(
                            cmd,
                            from,
                            to,
                            &[vk::BufferCopy {
                                src_offset: 0,
                                dst_offset: 0,
                                size: used,
                            }],
                        );
                    }
                    Ok(())
                })
            };
            if let Err(e) = copied {
                self.destroy_buffer(bigger);
                return Err(e);
            }
            let old = std::mem::replace(&mut tm.pool, bigger);
            self.destroy_buffer(old);
            tm.set_dirty = true;
        }
        let mut staging = self.create_buffer(
            len,
            vk::BufferUsageFlags::TRANSFER_SRC,
            MemoryLocation::CpuToGpu,
            "staging",
        )?;
        if let Err(e) = staging.write(0, bgra) {
            self.destroy_buffer(staging);
            return Err(e);
        }
        let (from, to, offset) = (staging.buffer, tm.pool.buffer, tm.pool_used);
        let copied = self.one_shot("terrain merge source", |device, cmd| {
            // SAFETY: both buffers are live until after the wait inside `one_shot`.
            unsafe {
                device.cmd_copy_buffer(
                    cmd,
                    from,
                    to,
                    &[vk::BufferCopy {
                        src_offset: 0,
                        dst_offset: offset,
                        size: len,
                    }],
                );
            }
            Ok(())
        });
        self.destroy_buffer(staging);
        copied?;
        let index = u32::try_from(tm.sources.len())
            .map_err(|_| RenderError::Unsupported("too many merge sources"))?;
        tm.sources.push(((offset / 4) as u32, width, height));
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
        let words = pack_job(job, job.size, source)?;
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        tm.params.write(0, &bytes)?;

        let size = job.size;
        let out_len = u64::from(size) * u64::from(size) * 4;
        if out_len > tm.output.size {
            let bigger = self.create_buffer(
                out_len,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
                MemoryLocation::GpuOnly,
                "terrain merge output",
            )?;
            let old = std::mem::replace(&mut tm.output, bigger);
            self.destroy_buffer(old);
            tm.set_dirty = true;
        }
        if tm.set_dirty {
            let infos = [tm.pool.buffer, tm.params.buffer, tm.output.buffer].map(|buffer| {
                [vk::DescriptorBufferInfo {
                    buffer,
                    offset: 0,
                    range: vk::WHOLE_SIZE,
                }]
            });
            let writes: Vec<vk::WriteDescriptorSet> = infos
                .iter()
                .enumerate()
                .map(|(i, info)| {
                    vk::WriteDescriptorSet::default()
                        .dst_set(tm.set)
                        .dst_binding(i as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(info)
                })
                .collect();
            // SAFETY: the set is not in use -- every earlier dispatch was waited for.
            unsafe { self.device.update_descriptor_sets(&writes, &[]) };
            tm.set_dirty = false;
        }

        // The same level count an uploaded one-level BGRA8 composite gets.
        let shape = TextureData {
            width: size,
            height: size,
            format: TextureFormat::Bgra8,
            levels: vec![Vec::new()],
        };
        let levels = crate::mip::runtime_level_count(&shape, self.imgtex_autogen_supported);
        let levels = u16::try_from(levels)
            .map_err(|_| RenderError::Unsupported("too many texture levels"))?;
        let (format, _) = vk_format(TextureFormat::Bgra8);
        let texture = self.create_image(
            size,
            size,
            levels,
            format,
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageAspectFlags::COLOR,
            TextureFormat::Bgra8,
            "texture",
        )?;
        let (image, output, pipeline, layout, set) = (
            texture.image,
            tm.output.buffer,
            tm.pipeline,
            tm.layout,
            tm.set,
        );
        let groups = size.div_ceil(8);
        let result = self.one_shot("terrain merge", |device, cmd| {
            // SAFETY: the buffer is recording; every handle is live until after the wait.
            unsafe {
                // Earlier submissions wrote the pool (copies) and read the output (the previous
                // composite's copy into its image); both must be done before this dispatch.
                let before = vk::MemoryBarrier::default()
                    .src_access_mask(
                        vk::AccessFlags::TRANSFER_WRITE
                            | vk::AccessFlags::TRANSFER_READ
                            | vk::AccessFlags::SHADER_WRITE,
                    )
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER | vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[before],
                    &[],
                    &[],
                );
                device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::COMPUTE,
                    layout,
                    0,
                    &[set],
                    &[],
                );
                device.cmd_dispatch(cmd, groups, groups, 1);
                let written = vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(output)
                    .offset(0)
                    .size(vk::WHOLE_SIZE);
                let whole = vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: u32::from(levels),
                    base_array_layer: 0,
                    layer_count: 1,
                };
                let to_dst = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .old_layout(vk::ImageLayout::UNDEFINED)
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(image)
                    .subresource_range(whole);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[written],
                    &[to_dst],
                );
                let region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(mipgen::layers(0))
                    .image_extent(vk::Extent3D {
                        width: size,
                        height: size,
                        depth: 1,
                    });
                device.cmd_copy_buffer_to_image(
                    cmd,
                    output,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                );
                if levels > 1 {
                    mipgen::record(device, cmd, image, size, size, levels);
                } else {
                    let to_read = vk::ImageMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .image(image)
                        .subresource_range(whole);
                    device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[to_read],
                    );
                }
            }
            Ok(())
        });
        if let Err(e) = result {
            self.destroy_texture(texture);
            return Err(e);
        }
        self.register_texture(key, texture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The compositor parses, validates and lowers to SPIR-V without a device.
    #[test]
    fn the_compositor_compiles_to_spirv() {
        let words = shaders::compile_entry(SHADER, naga::ShaderStage::Compute, "cs_merge")
            .expect("compiles");
        assert_eq!(words[0], 0x0723_0203, "SPIR-V magic");
    }
}
