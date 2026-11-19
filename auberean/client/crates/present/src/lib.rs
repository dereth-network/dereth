//! Putting a frame on a surface a host made, a window's or a browser canvas's.
//!
//! The world is drawn by the world's device into a picture; the interface's picture behind it, the world, the interface's elements deeper than its viewports,
//! the interface's textured batches that stand under its viewports (those before its first preview
//! space), the scenes of its viewports (each drawn into a clear picture of its viewport's size, its
//! alpha how much of it the scene covers, and laid over what is behind by that alpha), the
//! interface's picture in front, its other textured batches and the overlay's rectangles composed
//! into the window's
//! picture, which is
//! copied to the surface. The world's and portal space's pictures are laid as they stand, their
//! alpha the frame's own. The composed picture can be saved (a screenshot of exactly the window's
//! drawing). The composing alone ([`Composer`]) needs no surface: a host that draws the game into
//! a part of a picture of its own takes the composed picture from it.
//!
//! **Depends on** the world's device (`auberean-gpu`), a frame's pictures (`auberean-draw`) and
//! what a frame shows (`auberean-shell`). **Used by** the hosts that have a surface: the desktop
//! client and the browser client; and by the world builder, which draws the game in its view.
//!
//! **Must never** make its surface or name what it is on: the host brings it, a window's or a
//! canvas's, and asks for its device in the way its platform can wait for one.

// A browser's device holds JavaScript objects, which stay on the thread they were made on: this
// crate's shared views are shared within a thread there.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

mod canvas;
mod items;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use auberean_draw::picture::Image;
use auberean_draw::scene::Frame;
use auberean_gpu::{PictureFormat, Renderer};
use auberean_shell::interface::{
    Canvas, Overlay, OverlayMaterial, OverlaySampler, OverlayTexture, OverlayTextures, TextureData,
};
use auberean_shell::ViewFrame;
use bytemuck::{Pod, Zeroable};

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

@group(1) @binding(0) var ramp: texture_2d<f32>;

@fragment
fn fs_ramp(i: VOut) -> @location(0) vec4<f32> {
    let c = vec3<i32>(round(textureSample(tex, samp, i.uv).rgb * 255.0));
    return vec4(
        textureLoad(ramp, vec2(c.r, 0), 0).r,
        textureLoad(ramp, vec2(c.g, 0), 0).r,
        textureLoad(ramp, vec2(c.b, 0), 0).r,
        1.0,
    );
}

@fragment
fn fs_quad(i: VOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, i.uv) * i.color;
}

@fragment
fn fs_text(i: VOut) -> @location(0) vec4<f32> {
    return vec4(i.color.rgb, i.color.a * textureSample(tex, samp, i.uv).a);
}

@fragment
fn fs_laid(i: VOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    return vec4(c.rgb * i.color.rgb, i.color.a);
}
";

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// How pictures reach the screen: each waits for the display's refresh, in a window always; in full
/// screen unless told not to, then with none in the way, as near that as the surface offers.
// D2-09870
#[must_use]
pub fn present_mode(
    offered: &[wgpu::PresentMode],
    full_screen: bool,
    sync: bool,
) -> wgpu::PresentMode {
    if !full_screen || sync {
        return wgpu::PresentMode::Fifo;
    }
    [wgpu::PresentMode::Immediate, wgpu::PresentMode::Mailbox]
        .into_iter()
        .find(|m| offered.contains(m))
        .unwrap_or(wgpu::PresentMode::Fifo)
}

/// An interface picture on the device: the revision and size it holds, and a copy of its texels to
/// find what the next one changes.
struct KeptCanvas {
    revision: u64,
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    bgra: Vec<u8>,
}

/// What draws the world's picture in place of the retail device, on the device the composer draws
/// on (a renderer beside the retail one, sharing its device): the frame at a size, as a picture
/// ready to sample once the work submitted after it is run; `None` leaves it to the retail device.
pub trait WorldPainter {
    fn paint(
        &mut self,
        frame: &Frame,
        size: (u32, u32),
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Option<wgpu::Texture>;
}

/// What lays a frame's pictures into one, on the world's device, and what it keeps between
/// frames: the interface's pictures, the overlay's, and the picture it composes. A [`Presenter`]
/// shows that picture on its surface; a host drawing the game into a picture of its own (an
/// editor's view) takes it from [`Composer::compose`].
pub struct Composer {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    world: wgpu::RenderPipeline,
    quads: wgpu::RenderPipeline,
    /// A picture of the world's laid over by its rectangle's alpha alone (portal space).
    laid: wgpu::RenderPipeline,
    /// The interface's own pictures laid over what is beneath by their alpha.
    canvas: wgpu::RenderPipeline,
    /// Texel for texel: the interface's pictures are the window's size.
    nearest: wgpu::Sampler,
    /// The interface's pictures on the device: behind the world, in front, over the world and
    /// under the scenes.
    canvases: [Option<KeptCanvas>; 3],
    white: wgpu::TextureView,
    pictures: HashMap<u32, Option<Arc<wgpu::TextureView>>>,
    composed: Option<(wgpu::Texture, u32, u32)>,
    /// The overlay's vertices.
    overlay_vertices: Option<wgpu::Buffer>,
    /// What draws the world's picture in place of the retail device, when something does.
    painter: Option<Box<dyn WorldPainter>>,
    /// The interface's textured batches that are text: the vertex's color, the texture's alpha.
    text: wgpu::RenderPipeline,
    /// The interface's textured batches that invert what is beneath them.
    invert: wgpu::RenderPipeline,
    /// The textures the interface's batches sample, on the device, with the picture each was
    /// uploaded from.
    item_textures: HashMap<OverlayTexture, (Arc<TextureData>, wgpu::TextureView)>,
    /// The device's samplers for the batches' samplings, made as first asked for.
    item_samplers: HashMap<OverlaySampler, wgpu::Sampler>,
}

impl std::fmt::Debug for Composer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Composer")
            .field("pictures", &self.pictures.len())
            .field("painted", &self.painter.is_some())
            .finish_non_exhaustive()
    }
}

/// The window's device and what it keeps between frames.
pub struct Presenter {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The ways the surface can take pictures to the screen.
    present_modes: Vec<wgpu::PresentMode>,
    pub renderer: Renderer,
    composer: Composer,
    to_window: wgpu::RenderPipeline,
    /// Into the window through the brightness's table of levels.
    to_window_ramped: wgpu::RenderPipeline,
    /// The brightness shown and its table of levels, while it changes the picture.
    ramp: Option<(f32, wgpu::BindGroup)>,
    /// The size each picture is drawn at, scaled to the window.
    drawn: (u32, u32),
    /// The last frame found the window can't be seen.
    hidden: bool,
    /// Pictures keep their shape in the window, black bars around them, instead of filling it.
    keep_shape: bool,
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
    pictures: PictureFormat,
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
        format: pictures.format(),
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
        &pictures.bytes(bgra),
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

/// The shader module every picture is laid with.
fn shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("present"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    })
}

/// A pipeline laying a picture into one of `format`, by the vertex and fragment stages of
/// `module` named in `stages`.
fn pipeline(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    (vs, fs): (&str, &str),
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fs),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module,
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
}

impl Composer {
    /// The world's picture drawn by `painter` from now on, or by the retail device with none.
    pub fn set_painter(&mut self, painter: Option<Box<dyn WorldPainter>>) {
        self.painter = painter;
    }

    /// What composes frames on `renderer`'s device.
    #[must_use]
    pub fn new(renderer: &Renderer) -> Self {
        let device = renderer.device();
        let module = shader(device);

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
        let pipeline = |stages, blend, buffers: &[Option<wgpu::VertexBufferLayout<'_>>]| {
            pipeline(
                device,
                &module,
                &pipeline_layout,
                stages,
                COMPOSED,
                blend,
                buffers,
            )
        };
        const ATTRS: [wgpu::VertexAttribute; 3] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let quad_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRS,
        };
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let world = pipeline(("vs_full", "fs_opaque"), None, &[]);
        let quads = pipeline(("vs_quad", "fs_quad"), blend, &[Some(quad_layout.clone())]);
        let laid = pipeline(("vs_quad", "fs_laid"), blend, &[Some(quad_layout.clone())]);
        let text = pipeline(("vs_quad", "fs_text"), blend, &[Some(quad_layout.clone())]);
        // The texture times the color, times one less what is beneath: white inverts it.
        let invert_blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::OneMinusDst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });
        let invert = pipeline(("vs_quad", "fs_quad"), invert_blend, &[Some(quad_layout)]);
        let canvas = pipeline(("vs_full", "fs_quad"), blend, &[]);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let white = upload(
            device,
            renderer.queue(),
            1,
            1,
            &[255; 4],
            renderer.pictures(),
        );
        Self {
            layout,
            sampler,
            world,
            quads,
            laid,
            canvas,
            nearest,
            canvases: [None, None, None],
            white,
            pictures: HashMap::new(),
            composed: None,
            overlay_vertices: None,
            painter: None,
            text,
            invert,
            item_textures: HashMap::new(),
            item_samplers: HashMap::new(),
        }
    }

    /// The interface's textures on the device as the overlay holds them: each one new or
    /// uploaded again sent up, each one gone let go of.
    fn sync_item_textures(&mut self, renderer: &Renderer, textures: &OverlayTextures) {
        self.item_textures.retain(|k, _| textures.get(k).is_some());
        for (key, picture) in textures.iter() {
            let held = self
                .item_textures
                .get(key)
                .is_some_and(|(p, _)| Arc::ptr_eq(p, picture));
            if held {
                continue;
            }
            let Some(texels) = picture.levels.first() else {
                continue;
            };
            let view = upload(
                renderer.device(),
                renderer.queue(),
                picture.width,
                picture.height,
                texels,
                renderer.pictures(),
            );
            self.item_textures.insert(*key, (Arc::clone(picture), view));
        }
    }

    /// The device's sampler for `sampling`.
    fn item_sampler(&mut self, device: &wgpu::Device, sampling: OverlaySampler) -> wgpu::Sampler {
        self.item_samplers
            .entry(sampling)
            .or_insert_with(|| device.create_sampler(&items::sampler_descriptor(sampling)))
            .clone()
    }

    fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.bind_with(device, view, &self.sampler)
    }

    fn bind_with(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
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
        renderer: &Renderer,
        id: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Option<Arc<wgpu::TextureView>> {
        if let Some(p) = self.pictures.get(&id) {
            return p.clone();
        }
        let v = load(id).map(|img| {
            Arc::new(upload(
                renderer.device(),
                renderer.queue(),
                img.width,
                img.height,
                &img.bgra,
                renderer.pictures(),
            ))
        });
        self.pictures.insert(id, v.clone());
        v
    }

    /// One of the interface's pictures on the device, uploaded only when it changed.
    fn canvas_view(
        &mut self,
        renderer: &Renderer,
        slot: usize,
        c: &Canvas,
    ) -> Option<wgpu::TextureView> {
        if c.width == 0 || c.height == 0 || c.bgra.len() != (c.width * c.height * 4) as usize {
            return None;
        }
        let same_size = self.canvases[slot]
            .as_ref()
            .is_some_and(|k| k.width == c.width && k.height == c.height);
        if !same_size {
            let texture = renderer.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("interface"),
                size: wgpu::Extent3d {
                    width: c.width,
                    height: c.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.pictures().format(),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.canvases[slot] = Some(KeptCanvas {
                revision: c.revision,
                width: c.width,
                height: c.height,
                texture,
                bgra: Vec::new(),
            });
        }
        let kept = self.canvases[slot].as_mut()?;
        if !same_size || kept.revision != c.revision {
            kept.revision = c.revision;
            // A new texture holds nothing yet: all of it goes up. Otherwise what changed.
            let spans = if kept.bgra.len() == c.bgra.len() {
                canvas::changed_spans(&kept.bgra, &c.bgra, c.width, c.height)
            } else {
                kept.bgra = vec![0; c.bgra.len()];
                vec![canvas::Span {
                    x: 0,
                    y: 0,
                    width: c.width,
                    height: c.height,
                }]
            };
            let pictures = renderer.pictures();
            for s in &spans {
                let start = ((s.y * c.width + s.x) * 4) as usize;
                let end = (((s.y + s.height - 1) * c.width + s.x + s.width) * 4) as usize;
                renderer.queue().write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &kept.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: s.x,
                            y: s.y,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pictures.bytes(&c.bgra[start..end]),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(c.width * 4),
                        rows_per_image: Some(s.height),
                    },
                    wgpu::Extent3d {
                        width: s.width,
                        height: s.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            canvas::copy_spans(&mut kept.bgra, &c.bgra, c.width, &spans);
        }
        Some(
            kept.texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    fn composed(&mut self, device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
        if let Some((t, cw, ch)) = &self.composed {
            if *cw == w && *ch == h {
                return t.clone();
            }
        }
        let t = device.create_texture(&wgpu::TextureDescriptor {
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

    /// The overlay's vertices on the device, in a buffer kept while they fit it.
    fn overlay_buffer(&mut self, renderer: &Renderer, verts: &[QuadVertex]) -> wgpu::Buffer {
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let fits = self
            .overlay_vertices
            .as_ref()
            .filter(|b| b.size() >= bytes.len() as u64)
            .cloned();
        let buffer = fits.unwrap_or_else(|| {
            let size = (bytes.len() as u64).next_power_of_two().max(4096);
            renderer.note_work(|w| {
                w.buffers += 1;
                w.buffer_bytes += size;
            });
            let made = renderer.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("overlay"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.overlay_vertices = Some(made.clone());
            made
        });
        renderer.queue().write_buffer(&buffer, 0, bytes);
        buffer
    }

    /// Compose a frame into a picture `size` pixels: the world (when there is one), portal space
    /// laid over it (with how much of the way), a veil over both, the interface's pictures and its
    /// viewports' scenes, and the overlay over all, drawn by `renderer`. The work is recorded in
    /// `encoder`: the picture holds the frame once that is submitted.
    #[allow(clippy::too_many_arguments)]
    pub fn compose(
        &mut self,
        renderer: &mut Renderer,
        (w, h): (u32, u32),
        world: Option<&Frame>,
        portal: Option<(&Frame, f32)>,
        veil: Option<[f32; 4]>,
        overlay: &Overlay,
        views: &[ViewFrame],
        load: &mut dyn FnMut(u32) -> Option<Image>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::Texture {
        let mut lap = auberean_gpu::Lap::start();
        // A screen laid out at another size shows at its own proportions in the window's middle:
        // its viewports scale and move with it.
        let area = overlay
            .screen
            .map(|s| auberean_shell::interface::fitted(s, (w, h)));
        #[allow(clippy::cast_precision_loss)]
        let (ox, oy, scale) = overlay.screen.zip(area).map_or((0.0, 0.0, 1.0), |(s, a)| {
            (a[0], a[1], a[2] / s.0.max(1) as f32)
        });
        // Each viewport's scene into a clear picture of the viewport's size.
        let view_pictures: Vec<([f32; 4], bool, wgpu::TextureView)> = views
            .iter()
            .filter_map(|v| {
                let [l, t, r, b] = [
                    ox + v.rect[0] * scale,
                    oy + v.rect[1] * scale,
                    ox + v.rect[2] * scale,
                    oy + v.rect[3] * scale,
                ];
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (vw, vh) = ((r - l).round() as u32, (b - t).round() as u32);
                (vw > 0 && vh > 0).then(|| {
                    let tex = renderer.draw_laid_over(&v.frame, vw, vh, load);
                    (
                        [l, t, r, b],
                        v.under,
                        tex.create_view(&wgpu::TextureViewDescriptor::default()),
                    )
                })
            })
            .collect();
        let world_view = world.map(|f| {
            self.painter
                .as_mut()
                .and_then(|p| p.paint(f, (w, h), load))
                .unwrap_or_else(|| renderer.draw(f, w, h, load))
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
        let portal_view = portal.map(|(f, a)| {
            (
                renderer
                    .draw(f, w, h, load)
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                a,
            )
        });
        lap.lap();
        let behind = overlay
            .behind
            .as_deref()
            .and_then(|c| self.canvas_view(renderer, 0, c));
        let middle = overlay
            .middle
            .as_deref()
            .and_then(|c| self.canvas_view(renderer, 2, c));
        let front = overlay
            .front
            .as_deref()
            .and_then(|c| self.canvas_view(renderer, 1, c));
        let composed = self.composed(renderer.device(), w, h);
        let composed_view = composed.create_view(&wgpu::TextureViewDescriptor::default());
        // The overlay's vertices, in runs of one picture each.
        let mut verts: Vec<QuadVertex> = Vec::with_capacity(overlay.quads.len() * 6);
        let mut runs: Vec<(Option<Arc<wgpu::TextureView>>, u32, u32)> = Vec::new();
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (w as f32, h as f32);
        for q in &overlay.quads {
            let tex = q.picture.and_then(|id| self.picture(renderer, id, load));
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
        for (rect, _, _) in &view_pictures {
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
        // The interface's textured batches, each run bound to its texture where that is on the
        // device.
        self.sync_item_textures(renderer, &overlay.textures);
        let (items_under, items_over) = items::split_at_scenes(&overlay.items);
        let under_runs = items::lower(items_under, &mut verts);
        let item_runs = items::lower(items_over, &mut verts);
        let mut groups_of = |runs: &[items::ItemRun]| -> Vec<Option<wgpu::BindGroup>> {
            runs.iter()
                .map(|r| {
                    let view = self.item_textures.get(&r.texture).map(|(_, v)| v.clone())?;
                    let sampler = self.item_sampler(renderer.device(), r.sampler);
                    Some(self.bind_with(renderer.device(), &view, &sampler))
                })
                .collect()
        };
        let under_groups = groups_of(&under_runs);
        let item_groups = groups_of(&item_runs);
        let portal_group = portal_view
            .as_ref()
            .map(|(v, _)| self.bind(renderer.device(), v));
        let view_groups: Vec<wgpu::BindGroup> = view_pictures
            .iter()
            .map(|(_, _, v)| self.bind_with(renderer.device(), v, &self.nearest))
            .collect();
        let buffer = (!verts.is_empty()).then(|| self.overlay_buffer(renderer, &verts));
        let world_group = world_view.as_ref().map(|v| self.bind(renderer.device(), v));
        // The interface's pictures texel for texel at the window's size; another screen (the
        // screens before the world, laid out on 1024 by 768) is filtered as it scales.
        let sampler_for = |c: Option<&Canvas>| match c {
            Some(c) if (c.width, c.height) != (w, h) => &self.sampler,
            _ => &self.nearest,
        };
        let behind_sampler = sampler_for(overlay.behind.as_deref());
        let middle_sampler = sampler_for(overlay.middle.as_deref());
        let front_sampler = sampler_for(overlay.front.as_deref());
        let behind_group = behind
            .as_ref()
            .map(|v| self.bind_with(renderer.device(), v, behind_sampler));
        let middle_group = middle
            .as_ref()
            .map(|v| self.bind_with(renderer.device(), v, middle_sampler));
        let front_group = front
            .as_ref()
            .map(|v| self.bind_with(renderer.device(), v, front_sampler));
        let white_group = self.bind(renderer.device(), &self.white);
        let run_groups: Vec<Option<wgpu::BindGroup>> = runs
            .iter()
            .map(|(t, _, _)| t.as_ref().map(|v| self.bind(renderer.device(), v)))
            .collect();
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
            // The interface's pictures show in the screen's part of the window.
            #[allow(clippy::cast_precision_loss)]
            let whole = [0.0, 0.0, w as f32, h as f32];
            let screen = area.unwrap_or(whole);
            let set_area = |pass: &mut wgpu::RenderPass<'_>, a: [f32; 4]| {
                pass.set_viewport(a[0], a[1], a[2], a[3], 0.0, 1.0);
            };
            let draw_views = |pass: &mut wgpu::RenderPass<'_>, under: bool| {
                let Some(b) = &buffer else { return };
                pass.set_pipeline(&self.quads);
                pass.set_vertex_buffer(0, b.slice(..));
                for (k, g) in view_groups.iter().enumerate() {
                    if view_pictures[k].1 == under {
                        let start = view_start + u32::try_from(k * 6).unwrap_or(0);
                        pass.set_bind_group(0, g, &[]);
                        pass.draw(start..start + 6, 0..1);
                    }
                }
            };
            let draw_runs = |pass: &mut wgpu::RenderPass<'_>,
                             runs: &[items::ItemRun],
                             groups: &[Option<wgpu::BindGroup>]| {
                for (run, g) in runs.iter().zip(groups) {
                    let Some(g) = g else { continue };
                    pass.set_pipeline(match run.material {
                        OverlayMaterial::Image => &self.quads,
                        OverlayMaterial::Text => &self.text,
                        OverlayMaterial::Invert => &self.invert,
                    });
                    pass.set_bind_group(0, g, &[]);
                    pass.draw(run.start..run.start + run.count, 0..1);
                }
            };
            draw_views(&mut pass, true);
            if let Some(g) = &behind_group {
                set_area(&mut pass, screen);
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
                set_area(&mut pass, whole);
            }
            if let Some(g) = &world_group {
                pass.set_pipeline(&self.world);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some(b) = &buffer {
                pass.set_vertex_buffer(0, b.slice(..));
                if let Some(g) = &portal_group {
                    pass.set_pipeline(&self.laid);
                    pass.set_bind_group(0, g, &[]);
                    pass.draw(portal_start..portal_start + 6, 0..1);
                }
                pass.set_pipeline(&self.quads);
                if veil.is_some() {
                    pass.set_bind_group(0, &white_group, &[]);
                    pass.draw(veil_start..veil_start + 6, 0..1);
                }
            }
            // The elements deeper than the viewports over the world; the viewports' scenes over
            // them (a panel's, such as the paper doll's) and over the picture behind them, under
            // the picture in front.
            if let Some(g) = &middle_group {
                set_area(&mut pass, screen);
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
                set_area(&mut pass, whole);
            }
            // The interface's batches that stand under its viewports (a window holding a
            // figure), then the viewports' scenes.
            if let Some(b) = &buffer {
                pass.set_vertex_buffer(0, b.slice(..));
                draw_runs(&mut pass, &under_runs, &under_groups);
            }
            draw_views(&mut pass, false);
            if let Some(g) = &front_group {
                set_area(&mut pass, screen);
                pass.set_pipeline(&self.canvas);
                pass.set_bind_group(0, g, &[]);
                pass.draw(0..3, 0..1);
                set_area(&mut pass, whole);
            }
            // The textured batches over the picture in front, under the rectangles.
            if let Some(b) = &buffer {
                pass.set_vertex_buffer(0, b.slice(..));
                draw_runs(&mut pass, &item_runs, &item_groups);
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
        let ms = lap.lap();
        renderer.note_work(|w| w.ms.compose += ms);
        composed
    }
}

impl Presenter {
    /// The device for `surface`, `size` pixels, on `instance`'s hardware adapter. A host whose
    /// platform can block for the device; one that cannot asks for it itself and calls
    /// [`Self::on`].
    ///
    /// # Errors
    /// No hardware adapter, or no configuration for the surface.
    pub fn new(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
        antialiasing: bool,
    ) -> Result<Self, String> {
        let renderer = Renderer::antialiased(&instance, Some(&surface), antialiasing)
            .map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|e| format!("no adapter for the window: {e}"))?;
        Self::on(instance, surface, &adapter, renderer, size)
    }

    /// The device for `surface`, `size` pixels, over `renderer`, which `adapter` made.
    ///
    /// # Errors
    /// No configuration for the surface.
    pub fn on(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        renderer: Renderer,
        size: (u32, u32),
    ) -> Result<Self, String> {
        let caps = surface.get_capabilities(adapter);
        let mut config = surface
            .get_default_config(adapter, size.0.max(1), size.1.max(1))
            .ok_or("the surface has no configuration for this adapter")?;
        // Colors are written as computed, with no conversion, as the world's device writes them.
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = present_mode(&caps.present_modes, false, true);
        let present_modes = caps.present_modes.clone();
        let device = renderer.device();
        let composer = Composer::new(&renderer);
        let module = shader(device);
        let layout = &composer.layout;
        let window_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(layout)],
            immediate_size: 0,
        });
        let to_window = pipeline(
            device,
            &module,
            &window_layout,
            ("vs_full", "fs_opaque"),
            config.format,
            None,
            &[],
        );
        let ramped_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(layout), Some(layout)],
            immediate_size: 0,
        });
        let to_window_ramped = pipeline(
            device,
            &module,
            &ramped_layout,
            ("vs_full", "fs_ramp"),
            config.format,
            None,
            &[],
        );
        Ok(Self {
            _instance: instance,
            surface,
            drawn: (config.width, config.height),
            hidden: false,
            keep_shape: false,
            config,
            present_modes,
            renderer,
            composer,
            to_window,
            to_window_ramped,
            ramp: None,
        })
    }

    /// The world's picture drawn by `painter` from now on (on this presenter's device), or by the
    /// retail device with none.
    pub fn set_painter(&mut self, painter: Option<Box<dyn WorldPainter>>) {
        self.composer.set_painter(painter);
    }

    /// How pictures reach the screen: a window waits for the display's refresh; full screen as
    /// asked, its back buffers one, or two with triple buffering.
    // D2-09870
    pub fn set_presentation(&mut self, full_screen: bool, triple_buffering: bool, sync: bool) {
        self.config.present_mode = present_mode(&self.present_modes, full_screen, sync);
        self.config.desired_maximum_frame_latency = if !full_screen || triple_buffering {
            2
        } else {
            1
        };
        self.surface.configure(self.renderer.device(), &self.config);
    }

    /// The window's drawing area changed: the pictures are drawn at its size until told otherwise.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.drawn = (width, height);
        self.surface.configure(self.renderer.device(), &self.config);
    }

    /// Draw each picture at this size, and scale it to fill the window.
    pub fn draw_at(&mut self, width: u32, height: u32) {
        if width != 0 && height != 0 {
            self.drawn = (width, height);
        }
    }

    /// Show each picture at its own shape in the window's middle, black bars around it, or filling
    /// the window whatever its shape.
    pub fn keep_shape(&mut self, keep: bool) {
        self.keep_shape = keep;
    }

    /// Ready on the device what a share of the world's readying names, for frames of the size
    /// drawn ([`Renderer::warm`]).
    pub fn warm(
        &mut self,
        share: &auberean_draw::warm::Share,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) {
        self.renderer.warm(share, self.drawn, load);
    }

    /// The screen's brightness, which shows only in a full screen and not in a screenshot: `None`
    /// in a window. No brightness leaves the picture as it stands.
    // D2-10524
    pub fn set_brightness(&mut self, brightness: Option<f32>) {
        let b = brightness.filter(|b| *b != 0.0);
        if b == self.ramp.as_ref().map(|(at, _)| *at) {
            return;
        }
        self.ramp = b.map(|b| {
            // 16 bits to 8, in every channel of the table's texel.
            let texels = auberean_draw::quality::brightness_ramp(b)
                .map(|level| [u8::try_from((u32::from(level) + 128) / 257).unwrap_or(u8::MAX); 4]);
            let view = upload(
                self.renderer.device(),
                self.renderer.queue(),
                256,
                1,
                texels.as_flattened(),
                self.renderer.pictures(),
            );
            (b, self.composer.bind(self.renderer.device(), &view))
        });
    }

    /// Whether the last frame found the window can't be seen, so nothing waited for the display.
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden
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
        let (w, h) = self.drawn;
        let mut encoder = self
            .renderer
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let composed = self.composer.compose(
            &mut self.renderer,
            (w, h),
            world,
            portal,
            veil,
            overlay,
            views,
            load,
            &mut encoder,
        );
        let composed_view = composed.create_view(&wgpu::TextureViewDescriptor::default());
        let composed_group = self.composer.bind(self.renderer.device(), &composed_view);
        let mut lap = auberean_gpu::Lap::start();
        let device = self.renderer.device();
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
        let ms = lap.lap();
        self.renderer.note_work(|w| w.ms.acquire += ms);
        if let Some(t) = &frame {
            let view = t
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("to the surface"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.keep_shape {
                            wgpu::Color::BLACK
                        } else {
                            BACKDROP
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if self.keep_shape {
                let window = (self.config.width, self.config.height);
                let a = auberean_shell::interface::fitted((w, h), window);
                pass.set_viewport(a[0], a[1], a[2], a[3], 0.0, 1.0);
            }
            if let Some((_, ramp)) = &self.ramp {
                pass.set_pipeline(&self.to_window_ramped);
                pass.set_bind_group(1, ramp, &[]);
            } else {
                pass.set_pipeline(&self.to_window);
            }
            pass.set_bind_group(0, &composed_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.renderer.queue().submit([encoder.finish()]);
        let ms = lap.lap();
        self.renderer.note_work(|w| w.ms.compose_submit += ms);
        if let Some(t) = frame {
            self.renderer.queue().present(t);
        }
        let ms = lap.lap();
        self.renderer.note_work(|w| w.ms.show += ms);
        // What the frames before this one used is given back once their work is done.
        let _ = self.renderer.device().poll(wgpu::PollType::Poll);
        self.renderer.end_frame();
        let ms = lap.lap();
        self.renderer.note_work(|w| w.ms.reclaim += ms);
        if let Some(path) = shot {
            if let Err(e) = self.save(&composed, w, h, path) {
                tracing::warn!("screenshot {}: {e}", path.display());
            } else {
                tracing::info!("screenshot {}", path.display());
            }
        }
    }

    /// What the frames since the last call asked of the device.
    pub fn work(&self) -> auberean_gpu::Work {
        self.renderer.take_work()
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (how the rendering preferences choose the way pictures reach the screen)
    use super::present_mode;
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};

    #[test]
    fn a_window_and_a_synchronized_full_screen_wait_for_the_refresh() {
        let all = [Fifo, Mailbox, Immediate];
        assert_eq!(present_mode(&all, false, true), Fifo);
        assert_eq!(present_mode(&all, false, false), Fifo);
        assert_eq!(present_mode(&all, true, true), Fifo);
    }

    #[test]
    fn full_screen_unsynchronized_takes_the_nearest_the_surface_offers() {
        assert_eq!(
            present_mode(&[Fifo, Mailbox, Immediate], true, false),
            Immediate
        );
        assert_eq!(present_mode(&[Fifo, Mailbox], true, false), Mailbox);
        assert_eq!(present_mode(&[Fifo], true, false), Fifo);
    }
}
