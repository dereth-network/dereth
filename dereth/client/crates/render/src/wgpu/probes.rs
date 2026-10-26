//! Depth probes: the depth the frame left at a few pixels, read back without waiting.
//!
//! A depth texture can only be copied whole, so the probed pixels are not copied out of it. A
//! small pass after the frame's own samples the depth at each probed pixel and writes it into
//! one texel of a row target, as the four bytes of its bits; that row is what is copied to a
//! buffer and read once the buffer maps. The same pass serves WebGPU, WebGL 2 and the native
//! APIs: the depth is sampled as an unfilterable texture through a nearest sampler, and the row
//! is ordinary RGBA8.

use super::compiler::BuildAlone as _;
use super::Gpu;
use crate::device::{DepthProbes, MAX_DEPTH_PROBES};

/// The row target's width, in texels: one a probe.
#[allow(clippy::cast_possible_truncation)] // a small constant
const ROW_TEXELS: u32 = MAX_DEPTH_PROBES as u32;
/// One row of the target in a buffer: four bytes a texel, whole copy rows.
const ROW_BYTES: u32 = ROW_TEXELS * 4;
const _: () = assert!(ROW_BYTES.is_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
/// The probes' texture coordinates, two probes to a 16-byte element.
const UNIFORM_LEN: usize = MAX_DEPTH_PROBES * 8;
const UNIFORM_BYTES: u64 = UNIFORM_LEN as u64;

/// A triangle over the row; each texel samples the depth at its probe's texel center and writes
/// the depth's bits, low byte first, as four unsigned normalised bytes, which read back exact.
const SHADER: &str = r"
@group(0) @binding(0) var depth: texture_2d<f32>;
@group(0) @binding(1) var nearest: sampler;
@group(0) @binding(2) var<uniform> probes: array<vec4<f32>, 128>;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment fn fs(@builtin(position) at: vec4<f32>) -> @location(0) vec4<f32> {
    let i = u32(at.x);
    let pair = probes[i / 2u];
    let uv = select(pair.xy, pair.zw, (i & 1u) == 1u);
    let bits = bitcast<u32>(textureSampleLevel(depth, nearest, uv, 0.0).r);
    let bytes = vec4<u32>(bits, bits >> 8u, bits >> 16u, bits >> 24u) & vec4<u32>(255u);
    return vec4<f32>(bytes) / 255.0;
}
";

/// The probe pass's objects, made on first use.
pub(super) struct ProbePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    row: wgpu::Texture,
    row_view: wgpu::TextureView,
}

impl ProbePass {
    fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("depth probes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("depth probes"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("depth probes"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.render_pipeline_alone(&wgpu::RenderPipelineDescriptor {
            label: Some("depth probes"),
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
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("depth probes"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth probes"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let row = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth probes"),
            size: super::extent(ROW_TEXELS, 1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row_view = row.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            pipeline,
            layout,
            sampler,
            uniforms,
            row,
            row_view,
        }
    }
}

/// The texture coordinates of each pixel's texel center in a `w` by `h` plane, two pixels to
/// a 16-byte element; a pixel outside the plane takes the nearest edge pixel.
fn probe_coordinates(pixels: &[(u32, u32)], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(UNIFORM_LEN);
    #[allow(clippy::cast_precision_loss)] // pixel counts well inside an f32's mantissa
    for &(x, y) in pixels {
        let (x, y) = (x.min(w.saturating_sub(1)), y.min(h.saturating_sub(1)));
        let u = (x as f32 + 0.5) / w.max(1) as f32;
        let v = (y as f32 + 0.5) / h.max(1) as f32;
        out.extend_from_slice(&u.to_le_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.resize(UNIFORM_LEN, 0);
    out
}

impl Gpu {
    /// Ask for the depth at `pixels` once this frame is drawn. See `Gpu::request_depth_probes`.
    pub fn request_depth_probes(&mut self, tag: u64, mut pixels: Vec<(u32, u32)>) {
        pixels.truncate(MAX_DEPTH_PROBES);
        self.probe_request = (!pixels.is_empty()).then_some((tag, pixels));
    }

    /// The newest depth probes read back. See `Gpu::take_depth_probes`.
    pub fn take_depth_probes(&mut self) -> Option<DepthProbes> {
        self.probe_result.take()
    }

    /// Whether the depth probes asked for are read back: always, but while the high-fidelity
    /// presentation draws the frames, which read none.
    #[must_use]
    pub fn depth_probes_answered(&self) -> bool {
        #[cfg(feature = "hifi")]
        if self.sidecar.is_some() {
            return false;
        }
        true
    }

    /// Behind the frame's pass: sample the depth at each pixel into the row target, and copy
    /// the probed part of the row into a buffer of its own.
    pub(super) fn record_depth_probes(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        pixels: &[(u32, u32)],
    ) -> Option<wgpu::Buffer> {
        let (_, depth, w, h) = self.depth.as_ref()?;
        let n = u32::try_from(pixels.len().min(MAX_DEPTH_PROBES)).ok()?;
        if n == 0 {
            return None;
        }
        let device = &self.device;
        let pass = self
            .probe_pass
            .get_or_insert_with(|| ProbePass::new(device));
        self.queue.write_buffer(
            &pass.uniforms,
            0,
            &probe_coordinates(&pixels[..n as usize], *w, *h),
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("depth probes"),
            layout: &pass.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&pass.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pass.uniforms.as_entire_binding(),
                },
            ],
        });
        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("depth probes"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &pass.row_view,
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
            #[allow(clippy::cast_precision_loss)] // at most a few hundred texels
            rp.set_viewport(0.0, 0.0, n as f32, 1.0, 0.0, 1.0);
            rp.set_pipeline(&pass.pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.draw(0..3, 0..1);
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth probes"),
            size: u64::from(ROW_BYTES),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &pass.row,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_BYTES),
                    rows_per_image: Some(1),
                },
            },
            super::extent(n, 1),
        );
        Some(buffer)
    }

    /// Read whichever depth probes have mapped, keeping the newest; nothing waits.
    pub(super) fn collect_depth_probes(&mut self) {
        if self.probes_pending.is_empty() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.device.poll(wgpu::PollType::Poll);
        let mut keep = Vec::new();
        for (buffer, done, tag, n) in std::mem::take(&mut self.probes_pending) {
            if !done.load(std::sync::atomic::Ordering::Acquire) {
                keep.push((buffer, done, tag, n));
                continue;
            }
            if let Ok(mapped) = buffer.slice(..).get_mapped_range() {
                let depths = mapped
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .take(n)
                    .map(|b| f32::from_bits(u32::from_le_bytes(*b)))
                    .collect();
                if self.probe_result.as_ref().is_none_or(|r| r.tag <= tag) {
                    self.probe_result = Some(DepthProbes { tag, depths });
                }
            }
            buffer.unmap();
        }
        // A request whose buffer never maps is not kept for ever.
        keep.truncate(8);
        self.probes_pending = keep;
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the device's own read-back of the depth it drew, which the plugins' labels
    //! in the world are shown or hidden by)

    use super::super::tests::{device, quad, screen_key};
    use crate::device::{PerDrawConstants, PerFrameConstants};
    use crate::{DrawConstants, ZFunc};
    use dereth_primitives::{TextureData, TextureFormat};

    /// Draw frames of a quad at depth 0.25 over the right half of a `w` by `h` frame, asking
    /// the first for the depth at `pixels`, and answer the probes once they are read back.
    fn probe(w: u32, h: u32, pixels: &[(u32, u32)]) -> Option<Vec<f32>> {
        let mut gpu = device(w, h)?;
        let texture = gpu
            .upload_texture(&TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![0xFF; 4]],
            })
            .expect("a texture");
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (w as f32, h as f32);
        for frame in 0..8u64 {
            gpu.begin_frame().expect("a frame");
            if let Some(answer) = gpu.take_depth_probes() {
                assert_eq!(answer.tag, 1, "the tag comes back with its depths");
                return Some(answer.depths);
            }
            gpu.bind_texture(texture, 1);
            gpu.draw_dynamic(
                &screen_key(ZFunc::LessEqual),
                &DrawConstants::default(),
                &PerFrameConstants::default(),
                &PerDrawConstants {
                    uv_offset: [0.0, 0.0, fw, fh],
                    ..PerDrawConstants::identity()
                },
                &quad(fw / 2.0, 0.0, fw / 2.0, fh, 0.25, 0xFFFF_FFFF),
            )
            .expect("a draw");
            if frame == 0 {
                gpu.request_depth_probes(1, pixels.to_vec());
            }
            gpu.end_frame().expect("the frame");
            gpu.wait_idle().expect("the device finishes");
        }
        panic!("no depth probes came back in eight frames");
    }

    /// The depth at any pixel of a wide frame reads back as the frame left it, in the order
    /// asked: the cleared far plane on the left half, the near quad's depth on the right, at the
    /// centre columns and at both edges; a pixel past the frame reads its nearest edge.
    #[test]
    fn depth_probes_read_the_depth_at_any_pixel_of_a_wide_frame() {
        let (w, h) = (5120, 16);
        let pixels = [
            (2560, 8),
            (0, 0),
            (2569, 3),
            (2559, 7),
            (w + 40, 2),
            (1, h - 1),
            (w - 1, h - 1),
        ];
        let Some(depths) = probe(w, h, &pixels) else {
            return;
        };
        assert_eq!(depths.len(), pixels.len(), "one depth a pixel");
        let near = [true, false, true, false, true, false, true];
        for ((&(x, y), &d), near) in pixels.iter().zip(&depths).zip(near) {
            let want = if near { 0.25 } else { 1.0 };
            assert!(
                (d - want).abs() < 1e-6,
                "({x}, {y}): depth {d}, want {want}"
            );
        }
    }

    /// While the high-fidelity presentation draws the frames the device reads no depth back and
    /// says so; with it taken away, the device answers again.
    #[cfg(feature = "hifi")]
    #[test]
    fn no_depth_probes_are_answered_while_the_high_fidelity_presentation_draws() {
        use super::super::sidecar::{FrameSidecar, SidecarContext, SidecarError, SidecarFrame};
        struct Plain;
        impl FrameSidecar for Plain {
            fn prepare(
                &mut self,
                _: &mut SidecarContext<'_>,
            ) -> Result<SidecarFrame, SidecarError> {
                Ok(SidecarFrame::Plain)
            }
            fn encode(
                &mut self,
                _: &mut SidecarContext<'_>,
                _: &mut wgpu::CommandEncoder,
                _: &wgpu::TextureView,
            ) -> Result<(), SidecarError> {
                Ok(())
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }
        let cfg = crate::device::DeviceConfig {
            width: 64,
            height: 64,
            hifi: true,
            ..crate::device::DeviceConfig::default()
        };
        let mut gpu = match super::super::Gpu::new(None, &cfg) {
            Ok(gpu) => gpu,
            Err(e) => {
                eprintln!("skipped: {e}");
                return;
            }
        };
        if !gpu.hifi_supported() {
            eprintln!("skipped: the adapter cannot draw the high-fidelity presentation");
            return;
        }
        assert!(gpu.depth_probes_answered());
        gpu.hifi_install(Box::new(Plain)).expect("installed");
        assert!(!gpu.depth_probes_answered());
        assert!(gpu.hifi_take().is_some());
        assert!(gpu.depth_probes_answered());
    }

    /// The probes' texture coordinates land on each pixel's texel center, two pixels to an
    /// element, and a pixel past the plane takes the edge.
    #[test]
    fn probe_coordinates_are_texel_centers_clamped_to_the_plane() {
        let bytes = super::probe_coordinates(&[(0, 0), (3, 1), (9, 9)], 4, 2);
        let f: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        assert_eq!(f.len(), super::UNIFORM_LEN / 4);
        assert_eq!(&f[..6], &[0.125, 0.25, 0.875, 0.75, 0.875, 0.75]);
        assert!(f[6..].iter().all(|v| *v == 0.0));
    }
}
