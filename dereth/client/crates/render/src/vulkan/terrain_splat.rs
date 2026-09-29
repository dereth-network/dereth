//! Landscape cells drawn by blending their terrain layers in the pixel shader.
//!
//! The retail path composites each cell's layers into one texture beforehand and draws the cell
//! with that (`terrain_merge.rs` does the compositing on the device). This path skips the
//! composite: the cell's base tile, its overlay tiles and its alpha maps are bound directly and
//! the pixel shader blends them per pixel. That needs no composite textures at all, and none of
//! the work of making them, but it is not bit-identical to the composite: each layer is filtered
//! and mipmapped on its own before the blend, where the composite is blended first and filtered
//! after, and the composite's nearest-neighbour resampling becomes ordinary filtering.
//!
//! Everything else about the draw is the terrain's: the same vertex shader, the same
//! fixed-function state, the same constant blocks, fog, gamma and lighting.

use super::*;
use crate::device::TerrainSplat;
use crate::wgsl::TERRAIN_SPLAT_TAIL as SHADER_TAIL;

/// The most overlays one splat draw blends.
const MAX_LAYERS: usize = 5;
/// Image bindings in set 4: the base, five alpha maps, five tiles.
const IMAGES: usize = 1 + 2 * MAX_LAYERS;
/// `Splat` above: one `vec4<u32>` plus five.
const PUSH_BYTES: u32 = 16 * (1 + MAX_LAYERS as u32);
/// Sets per descriptor pool; another pool is added when one fills.
const SETS_PER_POOL: u32 = 1024;

/// The push-constant range both this pipeline's layout and the legacy one declare.
pub(super) fn push_range() -> vk::PushConstantRange {
    vk::PushConstantRange {
        stage_flags: vk::ShaderStageFlags::FRAGMENT,
        offset: 0,
        size: PUSH_BYTES,
    }
}

/// The splat pipeline's objects and its binding cache.
pub(super) struct TerrainSplatState {
    module: vk::ShaderModule,
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    pools: Vec<vk::DescriptorPool>,
    /// Sets allocated from the last pool.
    in_last_pool: u32,
    /// One set per distinct combination of bound textures, by texture slot (`u32::MAX` for the
    /// stand-in), and of the two samplers the filtering preference selected.
    sets: HashMap<([u32; IMAGES], vk::Sampler, vk::Sampler), vk::DescriptorSet>,
    /// One pipeline per terrain pipeline key.
    pipelines: HashMap<PipelineKey, vk::Pipeline>,
    /// A 1 x 1 texture bound wherever a draw has nothing to bind.
    stand_in: TextureSlot,
}

impl Gpu {
    /// Whether [`Self::draw_terrain_splat`] is available: the pipeline binds five descriptor sets
    /// (the legacy four and its own), one more than the Vulkan minimum. Every desktop device
    /// allows at least eight.
    #[must_use]
    pub fn terrain_splat_supported(&self) -> bool {
        self.max_bound_descriptor_sets >= 5
    }

    /// Draw landscape triangles with `key`'s fixed-function state, blending `splat`'s layers in
    /// the pixel shader. Otherwise exactly [`Self::draw_dynamic`].
    ///
    /// # Errors
    /// More overlays than the shader takes, a texture slot that is not live, or a device failure.
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
        let mut state = match self.terrain_splat.take() {
            Some(s) => s,
            None => self.create_terrain_splat()?,
        };
        let prepared = self.prepare_splat(&mut state, key, splat);
        let layout = state.layout;
        self.terrain_splat = Some(state);
        let (pipeline, set) = prepared?;

        let mut push = [0u32; (PUSH_BYTES / 4) as usize];
        push[0] = u32::from(splat.base.is_some());
        push[1] = splat.base_tiling;
        push[2] = splat.overlays.len() as u32;
        for (k, o) in splat.overlays.iter().enumerate() {
            push[4 + k * 4] = o.rotation;
            push[4 + k * 4 + 1] = o.tiling;
            push[4 + k * 4 + 2] = u32::from(o.tex.is_some());
        }
        let bytes: Vec<u8> = push.iter().flat_map(|w| w.to_le_bytes()).collect();
        // The wrap and clamp samplers come from one bank, so they carry the same bias.
        let lod_bias =
            self.sampler_descriptions[self.sampler_descriptor_index(0) as usize].mip_lod_bias;
        self.draw_prepared(
            key,
            pipeline,
            layout,
            &crate::DrawConstants::default(),
            per_frame,
            per_draw,
            vertices,
            lod_bias,
            |device, cmd| {
                // SAFETY: the buffer is recording inside the pass; the set and the layout are
                // alive until the next `reset_terrain_splat`, which waits for the device first.
                unsafe {
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        layout,
                        4,
                        &[set],
                        &[],
                    );
                    device.cmd_push_constants(
                        cmd,
                        layout,
                        vk::ShaderStageFlags::FRAGMENT,
                        0,
                        &bytes,
                    );
                }
            },
        )
    }

    /// Drop every cached splat binding set. It waits for the device, because a set may still be
    /// named by a frame in flight; call it before releasing a texture a splat draw bound.
    pub fn reset_terrain_splat(&mut self) {
        let Some(state) = self.terrain_splat.as_mut() else {
            return;
        };
        if state.sets.is_empty() {
            return;
        }
        state.sets.clear();
        let _ = self.wait_idle();
        let Some(state) = self.terrain_splat.as_mut() else {
            return;
        };
        for &p in &state.pools {
            // SAFETY: the wait above retired every frame that could name a set from it.
            let _ = unsafe {
                self.device
                    .reset_descriptor_pool(p, vk::DescriptorPoolResetFlags::empty())
            };
        }
        state.in_last_pool = 0;
        // Keep only the first pool; the others are made again if needed.
        let extra: Vec<vk::DescriptorPool> = state.pools.drain(1..).collect();
        for p in extra {
            // SAFETY: as above.
            unsafe { self.device.destroy_descriptor_pool(p, None) };
        }
    }

    fn create_terrain_splat(&mut self) -> Result<TerrainSplatState, RenderError> {
        let words = shaders::compile_splat_fragment(SHADER_TAIL, "ps_splat", self.shader_lod_bias)?;
        let module = Self::shader_module(&self.device, &words)?;
        let mut bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..IMAGES as u32)
            .map(|b| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            })
            .collect();
        // The tiles' and the alpha maps' samplers: taken from the device's sampler bank at draw
        // time, so the splat follows `Render.TextureFiltering` exactly as the composites do.
        for i in 0..2u32 {
            bindings.push(
                vk::DescriptorSetLayoutBinding::default()
                    .binding(IMAGES as u32 + i)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            );
        }
        // SAFETY: live locals; every handle is owned by the returned state and destroyed in
        // `destroy_terrain_splat`.
        let set_layout = vkr("vkCreateDescriptorSetLayout(splat)", unsafe {
            self.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        })?;
        let sets = [
            self.uniform_set_layout,
            self.texture_set_layout,
            self.texture_set_layout,
            self.sampler_set_layout,
            set_layout,
        ];
        let push = [push_range()];
        // SAFETY: as above.
        let layout = vkr("vkCreatePipelineLayout(splat)", unsafe {
            self.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&sets)
                    .push_constant_ranges(&push),
                None,
            )
        })?;
        let pool = self.create_splat_pool()?;
        let stand_in = self.upload_texture(&TextureData {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0, 0, 0, 0]],
        })?;
        Ok(TerrainSplatState {
            module,
            set_layout,
            layout,
            pools: vec![pool],
            in_last_pool: 0,
            sets: HashMap::new(),
            pipelines: HashMap::new(),
            stand_in,
        })
    }

    fn create_splat_pool(&self) -> Result<vk::DescriptorPool, RenderError> {
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLED_IMAGE,
                descriptor_count: SETS_PER_POOL * IMAGES as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::SAMPLER,
                descriptor_count: SETS_PER_POOL * 2,
            },
        ];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(SETS_PER_POOL)
            .pool_sizes(&sizes);
        // SAFETY: a live local; the pool is owned by the splat state.
        vkr("vkCreateDescriptorPool(splat)", unsafe {
            self.device.create_descriptor_pool(&info, None)
        })
    }

    pub(super) fn destroy_terrain_splat(&mut self, state: TerrainSplatState) {
        // SAFETY: the caller has waited for the device; nothing below is in flight.
        unsafe {
            for (_, p) in state.pipelines {
                self.device.destroy_pipeline(p, None);
            }
            for p in state.pools {
                self.device.destroy_descriptor_pool(p, None);
            }
            self.device.destroy_pipeline_layout(state.layout, None);
            self.device
                .destroy_descriptor_set_layout(state.set_layout, None);
            self.device.destroy_shader_module(state.module, None);
        }
    }

    /// The pipeline for `key` and the binding set for `splat`'s textures, made on first use.
    fn prepare_splat(
        &mut self,
        state: &mut TerrainSplatState,
        key: &PipelineKey,
        splat: &TerrainSplat,
    ) -> Result<(vk::Pipeline, vk::DescriptorSet), RenderError> {
        let pipeline = match state.pipelines.get(key) {
            Some(&p) => p,
            None => {
                let p = self.build_pipeline_with(key, state.module, c"ps_splat", state.layout)?;
                state.pipelines.insert(*key, p);
                p
            }
        };
        let mut slots = [u32::MAX; IMAGES];
        slots[0] = splat.base.map_or(u32::MAX, |s| s.0);
        for (k, o) in splat.overlays.iter().enumerate() {
            slots[1 + k] = o.alpha.0;
            slots[1 + MAX_LAYERS + k] = o.tex.map_or(u32::MAX, |s| s.0);
        }
        // The bank's wrap/wrap and clamp/clamp entries under the current filtering preference:
        // the terrain tiles repeat across the cell, the alpha maps cover it once.
        let wrap = self.samplers[self.sampler_descriptor_index(0) as usize];
        let clamp = self.samplers[self.sampler_descriptor_index(1) as usize];
        let cache_key = (slots, wrap, clamp);
        if let Some(&set) = state.sets.get(&cache_key) {
            return Ok((pipeline, set));
        }
        let view = |slot: u32| -> Result<vk::ImageView, RenderError> {
            let slot = if slot == u32::MAX {
                state.stand_in.0
            } else {
                slot
            };
            self.textures
                .get(&slot)
                .map(|t| t.view)
                .ok_or(RenderError::Unsupported(
                    "a splat texture slot that is not live",
                ))
        };
        let views = slots
            .iter()
            .map(|&s| view(s))
            .collect::<Result<Vec<_>, _>>()?;
        if state.in_last_pool == SETS_PER_POOL {
            state.pools.push(self.create_splat_pool()?);
            state.in_last_pool = 0;
        }
        let pool = *state.pools.last().expect("the state is made with one pool");
        let layouts = [state.set_layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: the pool has room (counted above); the set lives until the pool is reset.
        let set = vkr("vkAllocateDescriptorSets(splat)", unsafe {
            self.device.allocate_descriptor_sets(&info)
        })?[0];
        state.in_last_pool += 1;
        let infos: Vec<[vk::DescriptorImageInfo; 1]> = views
            .iter()
            .map(|&v| {
                [vk::DescriptorImageInfo {
                    sampler: vk::Sampler::null(),
                    image_view: v,
                    image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                }]
            })
            .collect();
        let sampler_infos = [wrap, clamp].map(|sampler| {
            [vk::DescriptorImageInfo {
                sampler,
                image_view: vk::ImageView::null(),
                image_layout: vk::ImageLayout::UNDEFINED,
            }]
        });
        let mut writes: Vec<vk::WriteDescriptorSet> = infos
            .iter()
            .enumerate()
            .map(|(b, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(b as u32)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(info)
            })
            .collect();
        for (i, info) in sampler_infos.iter().enumerate() {
            writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(IMAGES as u32 + i as u32)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(info),
            );
        }
        // SAFETY: the set is fresh and not yet named by any command buffer.
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        state.sets.insert(cache_key, set);
        Ok((pipeline, set))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The splat pixel shader parses, validates and lowers to SPIR-V without a device, with its
    /// samples biased by the sampler or by the shader.
    #[test]
    fn the_splat_shader_compiles_to_spirv() {
        for shader_lod_bias in [false, true] {
            let words = shaders::compile_splat_fragment(SHADER_TAIL, "ps_splat", shader_lod_bias)
                .expect("compiles");
            assert_eq!(words[0], 0x0723_0203, "SPIR-V magic");
        }
    }
}
