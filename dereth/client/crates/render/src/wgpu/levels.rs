//! A texture's generated sublevels, and reading a level back.
//!
//! **Sublevels.** An image texture that arrives with fewer levels than its chain has, and the
//! landscape composite, get the rest on the device: each destination texel samples the level above
//! it at its normalised centre with linear filtering, which lands exactly between the four source
//! texels it covers. That is the other devices' generation (a linear blit per level) done as a draw,
//! because `wgpu` has no blit.
//!
//! **Read-back.** One resident level, as the texture holds it: BGRA8 for everything drawn from
//! decoded texels, the block bytes for a block-compressed texture the device kept compressed.

use std::collections::HashMap;

use super::{Gpu, Texture, TextureData, TextureFormat, TextureSlot};
use crate::device::CapturedImage;
use crate::RenderError;

/// A triangle covering the target, and the level above sampled at each texel's centre.
const SHADER: &str = r"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct V {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: V;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment fn fs(v: V) -> @location(0) vec4<f32> {
    return textureSampleLevel(src, samp, v.uv, 0.0);
}
";

/// The sublevel pass's objects, made on first use.
#[derive(Default)]
pub(super) struct Levels {
    parts: Option<(
        wgpu::ShaderModule,
        wgpu::BindGroupLayout,
        wgpu::PipelineLayout,
        wgpu::Sampler,
    )>,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Gpu {
    /// Fill levels `from..to` of `texture`, each from the one above it.
    pub(super) fn generate_levels(
        &mut self,
        texture: &wgpu::Texture,
        format: wgpu::TextureFormat,
        from: u32,
        to: u32,
    ) {
        if from == 0 || from >= to {
            return;
        }
        let device = self.device.clone();
        let (module, layout, pipeline_layout, sampler) = self
            .levels
            .parts
            .get_or_insert_with(|| {
                let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("sublevels"),
                    source: wgpu::ShaderSource::Wgsl(SHADER.into()),
                });
                let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("sublevels"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
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
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("sublevels"),
                        bind_group_layouts: &[Some(&layout)],
                        immediate_size: 0,
                    });
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("sublevels"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                });
                (module, layout, pipeline_layout, sampler)
            })
            .clone();
        let pipeline = self
            .levels
            .pipelines
            .entry(format)
            .or_insert_with(|| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("sublevels"),
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
                        entry_point: Some("fs"),
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
            })
            .clone();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sublevels"),
        });
        for level in from..to {
            let view = |l: u32| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: l,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            let (above, target) = (view(level - 1), view(level));
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sublevel"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&above),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sublevel"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Read one resident BGRA8 level back.
    ///
    /// # Errors
    /// An absent slot, a texture held block-compressed, a level it does not have, or a device
    /// failure; in a browser, always, because a read-back cannot be waited for there.
    pub fn capture_texture_level(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<CapturedImage, RenderError> {
        let stored = self
            .textures
            .get(&slot.0)
            .map(|t| t.stored)
            .ok_or(RenderError::Unsupported("texture slot is not live"))?;
        if stored != TextureFormat::Bgra8 {
            return Err(RenderError::Unsupported(
                "readback requires an existing BGRA8 mip",
            ));
        }
        let mut data = self.capture_texture_level_data(slot, level)?;
        Ok(CapturedImage {
            width: data.width,
            height: data.height,
            bgra: data.levels.remove(0),
        })
    }

    /// Read one resident level back as it is held: BGRA8, or the block bytes of a texture kept
    /// block-compressed.
    ///
    /// # Errors
    /// As [`Self::capture_texture_level`], without the BGRA8 requirement.
    pub fn capture_texture_level_data(
        &mut self,
        slot: TextureSlot,
        level: u16,
    ) -> Result<TextureData, RenderError> {
        let (texture, stored, levels) = {
            let t: &Texture = self
                .textures
                .get(&slot.0)
                .ok_or(RenderError::Unsupported("texture slot is not live"))?;
            (t.texture.clone(), t.stored, t.levels)
        };
        if level >= levels {
            return Err(RenderError::Unsupported(
                "readback requires an existing mip",
            ));
        }
        let format = match stored {
            TextureFormat::Bgra8 => TextureFormat::Bgra8,
            TextureFormat::Bc1 => TextureFormat::Bc1,
            TextureFormat::Bc2 | TextureFormat::Bc2Premultiplied => TextureFormat::Bc2,
            TextureFormat::Bc3 | TextureFormat::Bc3Premultiplied => TextureFormat::Bc3,
            _ => {
                return Err(RenderError::Unsupported(
                    "unsupported texture readback format",
                ))
            }
        };
        let size = texture.size();
        let w = (size.width >> level).max(1);
        let h = (size.height >> level).max(1);
        let bytes = self.read_level(&texture, u32::from(level), w, h)?;
        Ok(TextureData {
            width: w,
            height: h,
            format,
            levels: vec![bytes],
        })
    }

    /// One level's bytes, tightly packed: whole blocks for a block format, BGRA8 texels otherwise
    /// (swizzled back from RGBA8 where the texture holds it that way).
    #[cfg(not(target_arch = "wasm32"))]
    fn read_level(
        &mut self,
        texture: &wgpu::Texture,
        level: u32,
        w: u32,
        h: u32,
    ) -> Result<Vec<u8>, RenderError> {
        let format = texture.format();
        let (block_w, block_h) = format.block_dimensions();
        let block_bytes = format
            .block_copy_size(None)
            .ok_or(RenderError::Unsupported(
                "unsupported texture readback format",
            ))?;
        let (cols, rows) = (w.div_ceil(block_w), h.div_ceil(block_h));
        let tight = cols * block_bytes;
        let padded = tight.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture read back"),
            size: u64::from(padded * rows),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(rows),
                },
            },
            super::extent(cols * block_w, rows * block_h),
        );
        self.queue.submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.wait_idle()?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| RenderError::Device(format!("read back: {e}")))?;
        let mut out = Vec::with_capacity((tight * rows) as usize);
        for r in 0..rows {
            let start = (r * padded) as usize;
            out.extend_from_slice(&mapped[start..start + tight as usize]);
        }
        drop(mapped);
        if format == wgpu::TextureFormat::Rgba8Unorm {
            super::swap_red_blue(&mut out);
        }
        Ok(out)
    }

    #[cfg(target_arch = "wasm32")]
    fn read_level(
        &mut self,
        _texture: &wgpu::Texture,
        _level: u32,
        _w: u32,
        _h: u32,
    ) -> Result<Vec<u8>, RenderError> {
        Err(RenderError::Unsupported(
            "a browser cannot wait for a read-back",
        ))
    }
}
