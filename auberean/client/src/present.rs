//! Putting a frame in the window: the world drawn by the world's device into a picture; the
//! interface's picture behind it, the world, the interface's elements deeper than its viewports,
//! the scenes of its viewports (each drawn into a clear picture of its viewport's size and laid
//! over what is behind by its alpha), the interface's picture in front and the overlay's
//! rectangles composed into the window's picture, which is copied to the window. The composed picture can be saved (a screenshot of exactly the window's
//! drawing).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use auberean_draw::picture::Image;
use auberean_draw::scene::Frame;
use auberean_gpu::Renderer;
use auberean_shell::interface::{Canvas, Overlay};
use auberean_shell::ViewFrame;
use bytemuck::{Pod, Zeroable};
use winit::window::Window;

/// The composed picture's format: colors as stored, as the world's device writes them.
const COMPOSED: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// The color behind everything before the world is drawn.
const BACKDROP: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.03,
    a: 1.0,
};

const SHADER: &str = r"
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

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
fn vs_quad(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VOut {
    var o: VOut;
    o.pos = vec4(pos, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}

@fragment
fn fs_quad(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, i.uv) * i.color;
}
";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// The window's device and what it keeps between frames.
pub struct Presenter {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub renderer: Renderer,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    to_window: wgpu::RenderPipeline,
    world: wgpu::RenderPipeline,
    quads: wgpu::RenderPipeline,
    /// The interface's own pictures laid over what is beneath by their alpha.
    canvas: wgpu::RenderPipeline,
    /// Texel for texel: the interface's pictures are the window's size.
    nearest: wgpu::Sampler,
    /// The interface's pictures on the device: behind the world, in front, over the world and
    /// under the scenes; each with the revision and size it holds.
    canvases: [Option<(u64, u32, u32, wgpu::Texture)>; 3],
    white: wgpu::TextureView,
    pictures: HashMap<u32, Option<Arc<wgpu::TextureView>>>,
    composed: Option<(wgpu::Texture, u32, u32)>,
}

impl std::fmt::Debug for Presenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presenter")
            .field("adapter", &self.renderer.adapter)
            .finish_non_exhaustive()
    }
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    w: u32,
    h: u32,
    bgra: &[u8],
) -> wgpu::TextureView {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("overlay picture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &t,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bgra,
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
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

impl Presenter {
    /// The device for `window`, on its hardware adapter.
    ///
    /// # Errors
    /// No surface for the window, no hardware adapter, or no configuration for the surface.
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
        // Colors are written as computed, with no conversion, as the world's device writes them.
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let device = renderer.device();
        surface.configure(device, &config);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |vs: &str,
                        fs: &str,
                        format: wgpu::TextureFormat,
                        blend: Option<wgpu::BlendState>,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pipeline_layout),
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
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let quad_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        let to_window = pipeline("vs_full", "fs_opaque", config.format, None, &[]);
        let world = pipeline("vs_full", "fs_opaque", COMPOSED, None, &[]);
        let quads = pipeline(
            "vs_quad",
            "fs_quad",
            COMPOSED,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            &[Some(quad_layout)],
        );
        let canvas = pipeline(
            "vs_full",
            "fs_quad",
            COMPOSED,
            Some(wgpu::BlendState::ALPHA_BLENDING),
            &[],
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let white = upload(device, renderer.queue(), 1, 1, &[255; 4]);
        Ok(Self {
            _instance: instance,
            surface,
            config,
            renderer,
            layout,
            sampler,
            to_window,
            world,
            quads,
            canvas,
            nearest,
            canvases: [None, None, None],
            white,
            pictures: HashMap::new(),
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

    fn bind(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.bind_with(view, &self.sampler)
    }

    fn bind_with(&self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
        self.renderer
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.layout,
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

    fn picture(
        &mut self,
        id: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Option<Arc<wgpu::TextureView>> {
        if let Some(p) = self.pictures.get(&id) {
            return p.clone();
        }
        let v = load(id).map(|img| {
            Arc::new(upload(
                self.renderer.device(),
                self.renderer.queue(),
                img.width,
                img.height,
                &img.bgra,
            ))
        });
        self.pictures.insert(id, v.clone());
        v
    }

    /// One of the interface's pictures on the device, uploaded only when it changed.
    fn canvas_view(&mut self, slot: usize, c: &Canvas) -> Option<wgpu::TextureView> {
        if c.width == 0 || c.height == 0 || c.bgra.len() != (c.width * c.height * 4) as usize {
            return None;
        }
        let fresh = match &self.canvases[slot] {
            Some((rev, w, h, _)) => *rev != c.revision || *w != c.width || *h != c.height,
            None => true,
        };
        if fresh {
            let same_size = self.canvases[slot]
                .as_ref()
                .is_some_and(|(_, w, h, _)| *w == c.width && *h == c.height);
            let texture = if same_size {
                self.canvases[slot].take().map(|(_, _, _, t)| t)
            } else {
                None
            }
            .unwrap_or_else(|| {
                self.renderer
                    .device()
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("interface"),
                        size: wgpu::Extent3d {
                            width: c.width,
                            height: c.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    })
            });
            self.renderer.queue().write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &c.bgra,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(c.width * 4),
                    rows_per_image: Some(c.height),
                },
                wgpu::Extent3d {
                    width: c.width,
                    height: c.height,
                    depth_or_array_layers: 1,
                },
            );
            self.canvases[slot] = Some((c.revision, c.width, c.height, texture));
        }
        self.canvases[slot]
            .as_ref()
            .map(|(_, _, _, t)| t.create_view(&wgpu::TextureViewDescriptor::default()))
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

    /// Draw a frame: the world (when there is one), portal space laid over it (with how much of
    /// the way), a veil over both, and the overlay over all, into the window; with `shot`, also
    /// save the window's picture there.
    #[allow(clippy::too_many_arguments)]
    pub fn present(
        &mut self,
        world: Option<&Frame>,
        portal: Option<(&Frame, f32)>,
        veil: Option<[f32; 4]>,
        overlay: &Overlay,
        views: &[ViewFrame],
        load: &mut dyn FnMut(u32) -> Option<Image>,
        shot: Option<&Path>,
    ) {
        let (w, h) = (self.config.width, self.config.height);
        // A screen laid out smaller than the window fills it: its viewports stretch with it.
        #[allow(clippy::cast_precision_loss)]
        let (sx, sy) = overlay.screen.map_or((1.0, 1.0), |(sw, sh)| {
            (w as f32 / sw.max(1) as f32, h as f32 / sh.max(1) as f32)
        });
        // Each viewport's scene into a clear picture of the viewport's size.
        self.renderer.backdrop = wgpu::Color::TRANSPARENT;
        let view_pictures: Vec<([f32; 4], wgpu::TextureView)> = views
            .iter()
            .filter_map(|v| {
                let [l, t, r, b] = [
                    v.rect[0] * sx,
                    v.rect[1] * sy,
                    v.rect[2] * sx,
                    v.rect[3] * sy,
                ];
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (vw, vh) = ((r - l).round() as u32, (b - t).round() as u32);
                (vw > 0 && vh > 0).then(|| {
                    let tex = self.renderer.draw(&v.frame, vw, vh, load);
                    (
                        [l, t, r, b],
                        tex.create_view(&wgpu::TextureViewDescriptor::default()),
                    )
                })
            })
            .collect();
        self.renderer.backdrop = wgpu::Color::BLACK;
        let world_view = world.map(|f| {
            self.renderer
                .draw(f, w, h, load)
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let portal_view = portal.map(|(f, a)| {
            (
                self.renderer
                    .draw(f, w, h, load)
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                a,
            )
        });
        let behind = overlay
            .behind
            .as_deref()
            .and_then(|c| self.canvas_view(0, c));
        let middle = overlay
            .middle
            .as_deref()
            .and_then(|c| self.canvas_view(2, c));
        let front = overlay
            .front
            .as_deref()
            .and_then(|c| self.canvas_view(1, c));
        let composed = self.composed(w, h);
        let composed_view = composed.create_view(&wgpu::TextureViewDescriptor::default());
        // The overlay's vertices, in runs of one picture each.
        let mut verts: Vec<QuadVertex> = Vec::with_capacity(overlay.quads.len() * 6);
        let mut runs: Vec<(Option<Arc<wgpu::TextureView>>, u32, u32)> = Vec::new();
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (w as f32, h as f32);
        for q in &overlay.quads {
            let tex = q.picture.and_then(|id| self.picture(id, load));
            if q.picture.is_some() && tex.is_none() {
                continue;
            }
            let nx = |x: f32| x / fw * 2.0 - 1.0;
            let ny = |y: f32| 1.0 - y / fh * 2.0;
            let [l, t, r, b] = q.rect;
            let [u0, v0, u1, v1] = q.uv;
            let c = q.color;
            let corner = |x: f32, y: f32, u: f32, v: f32| QuadVertex {
                pos: [nx(x), ny(y)],
                uv: [u, v],
                color: c,
            };
            let start = u32::try_from(verts.len()).unwrap_or(u32::MAX);
            verts.extend([
                corner(l, t, u0, v0),
                corner(r, t, u1, v0),
                corner(l, b, u0, v1),
                corner(r, t, u1, v0),
                corner(r, b, u1, v1),
                corner(l, b, u0, v1),
            ]);
            let same = runs.last().is_some_and(|(p, _, _)| match (p, &tex) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            });
            if same {
                if let Some(last) = runs.last_mut() {
                    last.2 += 6;
                }
            } else {
                runs.push((tex, start, 6));
            }
        }
        // The viewports' pictures as rectangles after the overlay's.
        let view_start = u32::try_from(verts.len()).unwrap_or(u32::MAX);
        for (rect, _) in &view_pictures {
            let nx = |x: f32| x / fw * 2.0 - 1.0;
            let ny = |y: f32| 1.0 - y / fh * 2.0;
            let [l, t, r, b] = *rect;
            let corner = |x: f32, y: f32, u: f32, v: f32| QuadVertex {
                pos: [nx(x), ny(y)],
                uv: [u, v],
                color: [1.0; 4],
            };
            verts.extend([
                corner(l, t, 0.0, 0.0),
                corner(r, t, 1.0, 0.0),
                corner(l, b, 0.0, 1.0),
                corner(r, t, 1.0, 0.0),
                corner(r, b, 1.0, 1.0),
                corner(l, b, 0.0, 1.0),
            ]);
        }
        // Portal space's picture and the veil, as rectangles over the whole window after the
        // viewports'.
        let full = |verts: &mut Vec<QuadVertex>, color: [f32; 4]| {
            let corner = |x: f32, y: f32, u: f32, v: f32| QuadVertex {
                pos: [x, y],
                uv: [u, v],
                color,
            };
            verts.extend([
                corner(-1.0, 1.0, 0.0, 0.0),
                corner(1.0, 1.0, 1.0, 0.0),
                corner(-1.0, -1.0, 0.0, 1.0),
                corner(1.0, 1.0, 1.0, 0.0),
                corner(1.0, -1.0, 1.0, 1.0),
                corner(-1.0, -1.0, 0.0, 1.0),
            ]);
        };
        let portal_start = u32::try_from(verts.len()).unwrap_or(u32::MAX);
        if let Some((_, a)) = &portal_view {
            full(&mut verts, [1.0, 1.0, 1.0, *a]);
        }
        let veil_start = u32::try_from(verts.len()).unwrap_or(u32::MAX);
        if let Some(c) = veil {
            full(&mut verts, c);
        }
        let portal_group = portal_view.as_ref().map(|(v, _)| self.bind(v));
        let view_groups: Vec<wgpu::BindGroup> = view_pictures
            .iter()
            .map(|(_, v)| self.bind_with(v, &self.nearest))
            .collect();
        let device = self.renderer.device();
        let buffer = (!verts.is_empty()).then(|| {
            use wgpu::util::DeviceExt;
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let world_group = world_view.as_ref().map(|v| self.bind(v));
        // The interface's pictures texel for texel at the window's size; a smaller screen (the
        // screens before the world, laid out on 1024 by 768) is filtered as it fills the window.
        let sampler_for = |c: Option<&Canvas>| match c {
            Some(c) if (c.width, c.height) != (w, h) => &self.sampler,
            _ => &self.nearest,
        };
        let behind_sampler = sampler_for(overlay.behind.as_deref());
        let middle_sampler = sampler_for(overlay.middle.as_deref());
        let front_sampler = sampler_for(overlay.front.as_deref());
        let behind_group = behind.as_ref().map(|v| self.bind_with(v, behind_sampler));
        let middle_group = middle.as_ref().map(|v| self.bind_with(v, middle_sampler));
        let front_group = front.as_ref().map(|v| self.bind_with(v, front_sampler));
        let white_group = self.bind(&self.white);
        let run_groups: Vec<Option<wgpu::BindGroup>> = runs
            .iter()
            .map(|(t, _, _)| t.as_ref().map(|v| self.bind(v)))
            .collect();
        let composed_group = self.bind(&composed_view);
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
                        load: wgpu::LoadOp::Clear(BACKDROP),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(g) = &behind_group {
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some(g) = &world_group {
                pass.set_pipeline(&self.world);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some(b) = &buffer {
                pass.set_pipeline(&self.quads);
                pass.set_vertex_buffer(0, b.slice(..));
                if let Some(g) = &portal_group {
                    pass.set_bind_group(0, g, &[]);
                    pass.draw(portal_start..portal_start + 6, 0..1);
                }
                if veil.is_some() {
                    pass.set_bind_group(0, &white_group, &[]);
                    pass.draw(veil_start..veil_start + 6, 0..1);
                }
            }
            // The elements deeper than the viewports over the world; the viewports' scenes over
            // them (a panel's, such as the paper doll's) and over the picture behind them, under
            // the picture in front.
            if let Some(g) = &middle_group {
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let (Some(b), false) = (&buffer, view_groups.is_empty()) {
                pass.set_pipeline(&self.quads);
                pass.set_vertex_buffer(0, b.slice(..));
                for (k, g) in view_groups.iter().enumerate() {
                    let start = view_start + u32::try_from(k * 6).unwrap_or(0);
                    pass.set_bind_group(0, g, &[]);
                    pass.draw(start..start + 6, 0..1);
                }
            }
            if let Some(g) = &front_group {
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some(b) = &buffer {
                pass.set_pipeline(&self.quads);
                pass.set_vertex_buffer(0, b.slice(..));
                pass.set_vertex_buffer(0, b.slice(..));
                for ((_, start, count), g) in runs.iter().zip(&run_groups) {
                    pass.set_bind_group(0, g.as_ref().unwrap_or(&white_group), &[]);
                    pass.draw(*start..*start + *count, 0..1);
                }
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
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
                        load: wgpu::LoadOp::Clear(BACKDROP),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.to_window);
            pass.set_bind_group(0, &composed_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.renderer.queue().submit([encoder.finish()]);
        if let Some(t) = frame {
            self.renderer.queue().present(t);
        }
        // What the frames before this one used is given back once their work is done.
        let _ = self.renderer.device().poll(wgpu::PollType::Poll);
        if let Some(path) = shot {
            if let Err(e) = self.save(&composed, w, h, path) {
                tracing::warn!("screenshot {}: {e}", path.display());
            } else {
                tracing::info!("screenshot {}", path.display());
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
