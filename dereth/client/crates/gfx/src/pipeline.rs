//! Pipelines made from precompiled shader blobs, all on one layout: the bindless table at set 0
//! and [`PUSH_BYTES`] of push constants for every stage.

use std::ffi::CString;

use ash::vk;

use crate::device::{vkr, Device, Garbage};
use crate::{ComputeDesc, Cull, Error, GraphicsDesc, Pipeline};

/// The push constants every pipeline takes, bytes.
pub const PUSH_BYTES: u32 = 128;

/// A live pipeline and where it binds.
#[derive(Debug)]
pub(crate) struct PipelineEntry {
    pub pipeline: vk::Pipeline,
    pub bind_point: vk::PipelineBindPoint,
}

/// The layout every pipeline shares, the driver's cache, and the pipelines.
#[derive(Debug)]
pub(crate) struct Pipelines {
    pub layout: vk::PipelineLayout,
    cache: vk::PipelineCache,
    pub entries: Vec<Option<PipelineEntry>>,
}

impl Pipelines {
    pub(crate) fn new(device: &ash::Device, set: vk::DescriptorSetLayout) -> Result<Self, Error> {
        let sets = [set];
        let push = [vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::ALL,
            offset: 0,
            size: PUSH_BYTES,
        }];
        let info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&sets)
            .push_constant_ranges(&push);
        // SAFETY: live locals; destroyed with the table.
        let layout = vkr("vkCreatePipelineLayout", unsafe {
            device.create_pipeline_layout(&info, None)
        })?;
        // SAFETY: an empty cache.
        let cache = match unsafe {
            device.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default(), None)
        } {
            Ok(c) => c,
            Err(e) => {
                // SAFETY: unreferenced.
                unsafe { device.destroy_pipeline_layout(layout, None) };
                return Err(Error::Vulkan {
                    what: "vkCreatePipelineCache",
                    result: e,
                });
            }
        };
        Ok(Self {
            layout,
            cache,
            entries: Vec::new(),
        })
    }

    /// # Safety
    /// Nothing the device runs uses any of them any more.
    pub(crate) unsafe fn destroy(&mut self, device: &ash::Device) {
        for e in self.entries.drain(..).flatten() {
            device.destroy_pipeline(e.pipeline, None);
        }
        device.destroy_pipeline_cache(self.cache, None);
        device.destroy_pipeline_layout(self.layout, None);
    }
}

fn module(device: &ash::Device, words: &[u32]) -> Result<vk::ShaderModule, Error> {
    let info = vk::ShaderModuleCreateInfo::default().code(words);
    // SAFETY: `words` is live for the call; the module is destroyed by the caller.
    vkr("vkCreateShaderModule", unsafe {
        device.create_shader_module(&info, None)
    })
}

fn entry_name(name: &str) -> Result<CString, Error> {
    CString::new(name).map_err(|_| Error::Invalid(format!("an entry point named {name:?}")))
}

impl Device {
    fn insert_pipeline(&mut self, entry: PipelineEntry) -> Pipeline {
        self.stats.pipelines_built += 1;
        let entries = &mut self.pipelines.entries;
        #[allow(clippy::cast_possible_truncation)] // a few dozen pipelines
        let id = entries.len() as u32;
        entries.push(Some(entry));
        Pipeline(id)
    }

    /// Make a graphics pipeline. Viewport and scissor are set while drawing; vertices are pulled
    /// by the vertex shader.
    ///
    /// # Errors
    /// Any failure of the driver, which is how a shader the device rejects is reported.
    pub fn create_graphics_pipeline(&mut self, desc: &GraphicsDesc<'_>) -> Result<Pipeline, Error> {
        let vs = module(&self.device, desc.vertex.spirv)?;
        let fs = match desc.fragment {
            Some(f) => match module(&self.device, f.spirv) {
                Ok(m) => Some(m),
                Err(e) => {
                    // SAFETY: unreferenced.
                    unsafe { self.device.destroy_shader_module(vs, None) };
                    return Err(e);
                }
            },
            None => None,
        };
        let result = self.build_graphics(desc, vs, fs);
        // SAFETY: a pipeline keeps what it needs of its modules.
        unsafe {
            self.device.destroy_shader_module(vs, None);
            if let Some(fs) = fs {
                self.device.destroy_shader_module(fs, None);
            }
        }
        let pipeline = result?;
        Ok(self.insert_pipeline(PipelineEntry {
            pipeline,
            bind_point: vk::PipelineBindPoint::GRAPHICS,
        }))
    }

    fn build_graphics(
        &self,
        desc: &GraphicsDesc<'_>,
        vs: vk::ShaderModule,
        fs: Option<vk::ShaderModule>,
    ) -> Result<vk::Pipeline, Error> {
        let vs_name = entry_name(desc.vertex.entry)?;
        let fs_name = match desc.fragment {
            Some(f) => Some(entry_name(f.entry)?),
            None => None,
        };
        let mut stages = vec![vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vs)
            .name(&vs_name)];
        if let (Some(fs), Some(name)) = (fs, fs_name.as_ref()) {
            stages.push(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fs)
                    .name(name),
            );
        }
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let raster = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(match desc.cull {
                Cull::None => vk::CullModeFlags::NONE,
                Cull::Front => vk::CullModeFlags::FRONT,
                Cull::Back => vk::CullModeFlags::BACK,
            })
            // Front faces are wound clockwise in the framebuffer space a negative-height
            // viewport gives, as the ordinary renderer has them.
            .front_face(vk::FrontFace::CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let depth = desc
            .depth
            .map_or_else(vk::PipelineDepthStencilStateCreateInfo::default, |d| {
                vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(d.compare.is_some())
                    .depth_write_enable(d.write)
                    .depth_compare_op(d.compare.map_or(vk::CompareOp::ALWAYS, crate::Compare::vk))
            });
        let blends: Vec<vk::PipelineColorBlendAttachmentState> = desc
            .color
            .iter()
            .map(|c| {
                let mut mask = vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B;
                if c.write_alpha {
                    mask |= vk::ColorComponentFlags::A;
                }
                let s = vk::PipelineColorBlendAttachmentState::default().color_write_mask(mask);
                match c.blend {
                    None => s,
                    Some(b) => s
                        .blend_enable(true)
                        .src_color_blend_factor(b.src.vk())
                        .dst_color_blend_factor(b.dst.vk())
                        .color_blend_op(vk::BlendOp::ADD)
                        .src_alpha_blend_factor(b.src.vk())
                        .dst_alpha_blend_factor(b.dst.vk())
                        .alpha_blend_op(vk::BlendOp::ADD),
                }
            })
            .collect();
        let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
        let dynamic = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic);
        let formats: Vec<vk::Format> = desc.color.iter().map(|c| c.format.vk()).collect();
        let mut rendering = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(&formats)
            .depth_attachment_format(desc.depth.map_or(vk::Format::UNDEFINED, |d| d.format.vk()));
        let info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input)
            .viewport_state(&viewport)
            .rasterization_state(&raster)
            .multisample_state(&multisample)
            .depth_stencil_state(&depth)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(self.pipelines.layout)
            .push_next(&mut rendering);
        // SAFETY: `info` and everything it points at live for the call.
        match unsafe {
            self.device
                .create_graphics_pipelines(self.pipelines.cache, &[info], None)
        } {
            Ok(p) => Ok(p[0]),
            Err((_, result)) => Err(Error::Vulkan {
                what: "vkCreateGraphicsPipelines",
                result,
            }),
        }
    }

    /// Make a compute pipeline.
    ///
    /// # Errors
    /// Any failure of the driver.
    pub fn create_compute_pipeline(&mut self, desc: &ComputeDesc<'_>) -> Result<Pipeline, Error> {
        let m = module(&self.device, desc.shader.spirv)?;
        let name = entry_name(desc.shader.entry);
        let result = name.and_then(|name| {
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(m)
                .name(&name);
            let info = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(self.pipelines.layout);
            // SAFETY: `info` and everything it points at live for the call.
            match unsafe {
                self.device
                    .create_compute_pipelines(self.pipelines.cache, &[info], None)
            } {
                Ok(p) => Ok(p[0]),
                Err((_, result)) => Err(Error::Vulkan {
                    what: "vkCreateComputePipelines",
                    result,
                }),
            }
        });
        // SAFETY: a pipeline keeps what it needs of its module.
        unsafe { self.device.destroy_shader_module(m, None) };
        let pipeline = result?;
        Ok(self.insert_pipeline(PipelineEntry {
            pipeline,
            bind_point: vk::PipelineBindPoint::COMPUTE,
        }))
    }

    /// Destroy a pipeline once the work recorded so far is done with it.
    pub fn destroy_pipeline(&mut self, p: Pipeline) {
        if let Some(e) = self
            .pipelines
            .entries
            .get_mut(p.0 as usize)
            .and_then(Option::take)
        {
            self.defer(Garbage::Pipeline(e.pipeline));
        }
    }
}
