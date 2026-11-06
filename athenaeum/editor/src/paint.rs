//! Putting a frame in the window: the world drawn by the world's device into a picture, laid into
//! the window's picture, and the panels' triangles drawn over it (egui's meshes, each cut to its
//! clip rectangle, over egui's textures). The composed picture is copied to the window, and can
//! be saved (a screenshot of exactly the window's drawing).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use auberean_draw::picture::Image;
use auberean_draw::scene::Frame;
use auberean_gpu::Renderer;
use bytemuck::{Pod, Zeroable};
use winit::window::Window;

/// The composed picture's format: colors as stored, as the world's device writes them.
const COMPOSED: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const SHADER: &str = r"
struct Screen { size: vec2<f32>, pad: vec2<f32> };
@group(0) @binding(0) var<uniform> screen: Screen;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var o: VOut;
    o.pos = vec4(p[i], 0.0, 1.0);
    o.uv = vec2((p[i].x + 1.0) * 0.5, 1.0 - (p[i].y + 1.0) * 0.5);
    o.color = vec4(1.0, 1.0, 1.0, 1.0);
    return o;
}

@fragment
fn fs_opaque(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    return vec4(c.rgb, 1.0);
}

@vertex
fn vs_mesh(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: u32) -> VOut {
    var o: VOut;
    o.pos = vec4(2.0 * pos.x / screen.size.x - 1.0, 1.0 - 2.0 * pos.y / screen.size.y, 0.0, 1.0);
    o.uv = uv;
    o.color = unpack4x8unorm(color);
    return o;
}

@fragment
fn fs_mesh(i: VOut) -> @location(0) vec4<f32> {
    return i.color * textureSample(tex, samp, i.uv);
}
";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: u32,
}

/// One of egui's textures on the device.
struct Texture {
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
    size: [u32; 2],
}

/// The window's device and what it keeps between frames.
pub struct Presenter {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
    texture_layout: wgpu::BindGroupLayout,
    screen_buffer: wgpu::Buffer,
    screen_group: wgpu::BindGroup,
    blit: wgpu::RenderPipeline,
    to_window: wgpu::RenderPipeline,
    mesh: wgpu::RenderPipeline,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    textures: HashMap<egui::TextureId, Texture>,
    composed: Option<(wgpu::Texture, u32, u32)>,
    /// The last frame found the window can't be seen.
    hidden: bool,
}

impl std::fmt::Debug for Presenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presenter")
            .field("adapter", &self.renderer.adapter)
            .finish_non_exhaustive()
    }
}

impl Presenter {
    /// The device for `window`, on its hardware adapter.
    ///
    /// # Errors
    /// No surface for the window, no hardware adapter, or no configuration for the surface.
    #[allow(clippy::too_many_lines)] // the device's pipelines, set up once
    pub fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let size = window.inner_size();
        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("no surface for the window: {e}"))?;
        let renderer = Renderer::on(&instance, Some(&surface)).map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("no adapter for the window: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("the window's surface has no configuration for this adapter")?;
        // Colors are written as computed, as the world's device writes them.
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        let device = renderer.device();
        surface.configure(device, &config);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("panels"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let screen_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &screen_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&screen_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = |vs: &str,
                        fs: &str,
                        format: wgpu::TextureFormat,
                        blend: Option<wgpu::BlendState>,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        const ATTRS: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32];
        let mesh_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        let blit = pipeline("vs_full", "fs_opaque", COMPOSED, None, &[]);
        let to_window = pipeline("vs_full", "fs_opaque", config.format, None, &[]);
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let mesh = pipeline(
            "vs_mesh",
            "fs_mesh",
            COMPOSED,
            Some(premultiplied),
            &[Some(mesh_layout)],
        );
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Ok(Self {
            hidden: false,
            _instance: instance,
            surface,
            config,
            renderer,
            texture_layout,
            screen_buffer,
            screen_group,
            blit,
            to_window,
            mesh,
            linear,
            nearest,
            textures: HashMap::new(),
            composed: None,
        })
    }

    /// The window's drawing area changed.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(self.renderer.device(), &self.config);
    }

    /// The window's drawing area.
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn group(&self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
        self.renderer
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                ],
            })
    }

    /// Take egui's changes to its textures: whole images and patches of them.
    fn set_texture(&mut self, id: egui::TextureId, delta: &egui::epaint::ImageDelta) {
        let egui::ImageData::Color(image) = &delta.image;
        #[allow(clippy::cast_possible_truncation)]
        let [w, h] = [image.size[0] as u32, image.size[1] as u32];
        if w == 0 || h == 0 {
            return;
        }
        let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
        let device = self.renderer.device();
        let (origin, fresh) = match delta.pos {
            None => ([0, 0], true),
            #[allow(clippy::cast_possible_truncation)]
            Some([x, y]) => ([x as u32, y as u32], false),
        };
        if fresh {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("panels"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = if delta.options.magnification == egui::TextureFilter::Linear {
                &self.linear
            } else {
                &self.nearest
            };
            let group = self.group(&view, sampler);
            self.textures.insert(
                id,
                Texture {
                    texture,
                    group,
                    size: [w, h],
                },
            );
        }
        let Some(t) = self.textures.get(&id) else {
            return;
        };
        if origin[0] + w > t.size[0] || origin[1] + h > t.size[1] {
            return;
        }
        self.renderer.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &t.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: origin[0],
                    y: origin[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    fn composed(&mut self, w: u32, h: u32) -> wgpu::Texture {
        if let Some((t, cw, ch)) = &self.composed {
            if *cw == w && *ch == h {
                return t.clone();
            }
        }
        let t = self
            .renderer
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("composed"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COMPOSED,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
        self.composed = Some((t.clone(), w, h));
        t
    }

    /// Whether the last frame found the window can't be seen, so nothing waited for the display.
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// Draw a frame: the world (when there is one) and the panels over it, into the window; with
    /// `shot`, also save the window's picture there.
    #[allow(clippy::too_many_lines)] // one pass, in drawing order
    pub fn present(
        &mut self,
        world: Option<&Frame>,
        mut panels: egui::FullOutput,
        ctx: &egui::Context,
        load: &mut dyn FnMut(u32) -> Option<Image>,
        shot: Option<&Path>,
    ) {
        let (w, h) = (self.config.width, self.config.height);
        let mut textures = std::mem::take(&mut panels.textures_delta);
        for (id, deltas) in textures.set.drain() {
            for delta in &deltas {
                self.set_texture(id, delta);
            }
        }
        let ppp = panels.pixels_per_point;
        let primitives = ctx.tessellate(panels.shapes, ppp);
        self.renderer.backdrop = wgpu::Color::BLACK;
        let world_view = world.map(|f| {
            self.renderer
                .draw(f, w, h, load)
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        #[allow(clippy::cast_precision_loss)]
        let screen = [w as f32 / ppp, h as f32 / ppp, 0.0, 0.0];
        self.renderer
            .queue()
            .write_buffer(&self.screen_buffer, 0, bytemuck::cast_slice(&screen));
        // The panels' vertices and indices, one run of each mesh.
        let mut verts: Vec<MeshVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut runs: Vec<(egui::TextureId, [u32; 4], u32, u32, i32)> = Vec::new();
        for p in &primitives {
            let egui::epaint::Primitive::Mesh(m) = &p.primitive else {
                continue;
            };
            if m.indices.is_empty() {
                continue;
            }
            let r = p.clip_rect;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let clip = {
                let x0 = (r.min.x * ppp).round().clamp(0.0, w as f32) as u32;
                let y0 = (r.min.y * ppp).round().clamp(0.0, h as f32) as u32;
                let x1 = (r.max.x * ppp).round().clamp(0.0, w as f32) as u32;
                let y1 = (r.max.y * ppp).round().clamp(0.0, h as f32) as u32;
                [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
            };
            if clip[2] == 0 || clip[3] == 0 {
                continue;
            }
            let base = i32::try_from(verts.len()).unwrap_or(i32::MAX);
            let first = u32::try_from(indices.len()).unwrap_or(u32::MAX);
            verts.extend(m.vertices.iter().map(|v| MeshVertex {
                pos: [v.pos.x, v.pos.y],
                uv: [v.uv.x, v.uv.y],
                color: u32::from_le_bytes(v.color.to_array()),
            }));
            indices.extend(&m.indices);
            let count = u32::try_from(m.indices.len()).unwrap_or(0);
            runs.push((m.texture_id, clip, first, count, base));
        }
        let device = self.renderer.device();
        use wgpu::util::DeviceExt;
        let vbuf = (!verts.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("panels"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let ibuf = (!indices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("panels"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            })
        });
        let composed = self.composed(w, h);
        let composed_view = composed.create_view(&wgpu::TextureViewDescriptor::default());
        let world_group = world_view.as_ref().map(|v| self.group(v, &self.linear));
        let composed_group = self.group(&composed_view, &self.nearest);
        let device = self.renderer.device();
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &composed_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.06,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.screen_group, &[]);
            if let Some(g) = &world_group {
                pass.set_pipeline(&self.blit);
                pass.set_bind_group(1, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let (Some(vb), Some(ib)) = (&vbuf, &ibuf) {
                pass.set_pipeline(&self.mesh);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                for (tex, clip, first, count, base) in &runs {
                    let Some(t) = self.textures.get(tex) else {
                        continue;
                    };
                    pass.set_scissor_rect(clip[0], clip[1], clip[2], clip[3]);
                    pass.set_bind_group(1, &t.group, &[]);
                    pass.draw_indexed(*first..*first + *count, *base, 0..1);
                }
            }
        }
        let acquired = self.surface.get_current_texture();
        self.hidden = matches!(acquired, wgpu::CurrentSurfaceTexture::Occluded);
        let frame = match acquired {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            // A window that can't be seen, or a picture that came late: the surface is as it was,
            // and configuring it again would wait for the device.
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            _ => {
                self.surface.configure(device, &self.config);
                None
            }
        };
        if let Some(t) = &frame {
            let view = t
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("to the window"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.to_window);
            pass.set_bind_group(0, &self.screen_group, &[]);
            pass.set_bind_group(1, &composed_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.renderer.queue().submit([encoder.finish()]);
        if let Some(t) = frame {
            self.renderer.queue().present(t);
        }
        for id in textures.free.drain() {
            self.textures.remove(&id);
        }
        let _ = self.renderer.device().poll(wgpu::PollType::Poll);
        self.renderer.end_frame();
        if let Some(path) = shot {
            match self.save(&composed, w, h, path) {
                Ok(()) => tracing::info!("screenshot {}", path.display()),
                Err(e) => tracing::warn!("screenshot {}: {e}", path.display()),
            }
        }
    }

    /// Save the composed picture as a PNG.
    fn save(&self, t: &wgpu::Texture, w: u32, h: u32, path: &Path) -> Result<(), String> {
        let device = self.renderer.device();
        let row = (w * 4).div_ceil(256) * 256;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.renderer.queue().submit([encoder.finish()]);
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = slice.get_mapped_range().map_err(|e| format!("{e:?}"))?;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let s = (y * row) as usize;
            rgba.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        drop(data);
        buf.unmap();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&rgba).map_err(|e| e.to_string())
    }
}
