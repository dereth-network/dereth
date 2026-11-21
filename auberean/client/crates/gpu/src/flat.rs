//! The flat view of a frame, for a host that must show a great deal quickly (a world builder over a
//! big world): every surface one color, lit by a fixed light by the way its face turns. It draws the
//! land's bottom layer, the water's surfaces and the objects; no pictures, sky, reflection, shores,
//! shadows, flares or effects. What lies wholly outside the view is left out, and a mesh is drawn
//! once for every place it stands in the frame.
//!
//! An object's color is the one its host gives it, else the average of its material's first
//! picture; the land's is the average of its ground's picture, which the far landscape adds to the
//! color the day lit each of its points with, as the full drawing adds the picture itself.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use auberean_draw::constants::{mat, view_proj, MeshGpuVertex, TerrainGpuVertex};
use auberean_draw::cull::{Frustum, Relation};
use auberean_draw::material::d3d;
use auberean_draw::mesh::{MeshVertex, PosedVertex};
use auberean_draw::picture::Image;
use auberean_draw::scene::{Frame, ObjectBatch, TerrainVertex};
use auberean_world::math::Mat4;
use bytemuck::{Pod, Zeroable};
use dereth_primitives::num::math;

use crate::{begin_pass, cache, Renderer, COLOR_FORMAT, DEPTH_FORMAT};

/// How the flat view colors and lights what it draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flat {
    /// The color behind everything, where nothing is drawn.
    pub backdrop: [f32; 3],
    /// The water's surfaces.
    pub water: [f32; 3],
    /// What is drawn with no picture and no color of its own.
    pub plain: [f32; 3],
    /// Toward the light, in the frame's world.
    pub to_light: [f32; 3],
    /// How much of the color shows with no light on a face, and how much more the light adds to a
    /// face turned straight at it.
    pub ambient: f32,
    pub direct: f32,
    /// From and to how far from the camera, meters, colors fade into the backdrop; none when
    /// nothing fades.
    pub fade: Option<[f32; 2]>,
}

impl Default for Flat {
    /// A pale sky, blue water and gray, lit from high in the south-west.
    fn default() -> Self {
        Self {
            backdrop: [0.62, 0.72, 0.82],
            water: [0.25, 0.42, 0.62],
            plain: [0.6, 0.6, 0.6],
            to_light: [-0.35, -0.55, 0.76],
            ambient: 0.45,
            direct: 0.6,
            fade: None,
        }
    }
}

/// What the last flat frame drew.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatCounts {
    /// Draw calls: one for each mesh and set of triangles, however many places it stands in.
    pub draws: u32,
    /// The places drawn, over every draw.
    pub instances: u32,
    /// The places left out, wholly outside the view.
    pub culled: u32,
}

/// The frame's light and view as the flat shader reads them.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FlatUniforms {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    to_light: [f32; 4],
    light: [f32; 4],
    backdrop: [f32; 4],
}

/// One place a mesh stands: its transform's rows, and its color (its alpha 2 for the far
/// landscape, which keeps its points' lit colors).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Instance {
    model: [[f32; 4]; 4],
    color: [f32; 4],
}

/// Where a mesh's positions come from: a mesh's own vertices, a body's posed ones, or the land's.
#[derive(Clone)]
enum Positions {
    Mesh(Arc<Vec<MeshVertex>>),
    Posed(Arc<Vec<PosedVertex>>),
    Land(Arc<Vec<TerrainVertex>>),
}

impl Positions {
    fn key(&self) -> (u8, usize) {
        match self {
            Self::Mesh(l) => (0, Arc::as_ptr(l) as usize),
            Self::Posed(l) => (1, Arc::as_ptr(l) as usize),
            Self::Land(l) => (2, Arc::as_ptr(l) as usize),
        }
    }

    /// The box that holds every position, in the list's own frame.
    fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let mut take = |p: [f32; 3]| {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        };
        match self {
            Self::Mesh(l) => l.iter().for_each(|v| take(v.position)),
            Self::Posed(l) => l.iter().for_each(|v| take(v.position)),
            Self::Land(l) => l.iter().for_each(|v| take(v.position)),
        }
        (lo, hi)
    }
}

/// A mesh's triangles.
#[derive(Clone)]
enum Indices {
    Short(Arc<Vec<u16>>),
    Long(Arc<Vec<u32>>),
}

impl Indices {
    fn key(&self) -> usize {
        match self {
            Self::Short(l) => Arc::as_ptr(l) as usize,
            Self::Long(l) => Arc::as_ptr(l) as usize,
        }
    }

    fn count(&self) -> u32 {
        let n = match self {
            Self::Short(l) => l.len(),
            Self::Long(l) => l.len(),
        };
        u32::try_from(n).unwrap_or(u32::MAX)
    }
}

/// One draw: a mesh's positions and triangles, and every place it stands this frame.
struct Group {
    positions: Positions,
    indices: Indices,
    places: Vec<Instance>,
}

/// A list's box, kept with the list so its address stays its own while it keys the box.
struct Boxed {
    _list: Positions,
    lo: [f32; 3],
    hi: [f32; 3],
    used: u64,
}

/// How many frames a list's box is kept after the last that drew it.
const KEEP_BOXES: u64 = 240;

/// What the flat view keeps between frames.
pub(crate) struct FlatKept {
    /// By the stream the positions come in: a mesh's vertices, a body's posed ones, the land's.
    pipelines: [wgpu::RenderPipeline; 3],
    uniforms: wgpu::Buffer,
    group: wgpu::BindGroup,
    instances: Option<wgpu::Buffer>,
    /// The average color of each picture, by its id.
    colors: HashMap<u32, [f32; 3]>,
    boxes: HashMap<(u8, usize), Boxed>,
    frame: u64,
    counts: FlatCounts,
}

/// The average color of a picture, by its smallest level, each texel weighed by its alpha.
fn average_color(img: &Image) -> Option<[f32; 3]> {
    let level = img.smaller.last().unwrap_or(img);
    let (mut sum, mut weight) = ([0.0f64; 3], 0.0f64);
    for t in level.bgra.as_chunks::<4>().0 {
        let a = f64::from(t[3]) / 255.0;
        sum[0] += f64::from(t[2]) * a;
        sum[1] += f64::from(t[1]) * a;
        sum[2] += f64::from(t[0]) * a;
        weight += a;
    }
    #[allow(clippy::cast_possible_truncation)]
    (weight > 0.0).then(|| sum.map(|s| (s / weight / 255.0) as f32))
}

/// The box about the box from `lo` to `hi` carried by `m`.
fn placed(lo: [f32; 3], hi: [f32; 3], m: &Mat4) -> ([f32; 3], [f32; 3]) {
    let (mut a, mut b) = ([f32::MAX; 3], [f32::MIN; 3]);
    for k in 0..8 {
        let c = m.transform4([
            if k & 1 == 0 { lo[0] } else { hi[0] },
            if k & 2 == 0 { lo[1] } else { hi[1] },
            if k & 4 == 0 { lo[2] } else { hi[2] },
            1.0,
        ]);
        for i in 0..3 {
            a[i] = a[i].min(c[i]);
            b[i] = b[i].max(c[i]);
        }
    }
    (a, b)
}

fn v(p: [f32; 3]) -> dereth_primitives::Vec3 {
    dereth_primitives::Vec3::new(p[0], p[1], p[2])
}

/// The bits of a transform, which tell one place from another.
fn place_bits(m: &Mat4) -> [u32; 16] {
    let mut out = [0; 16];
    for (o, x) in out.iter_mut().zip(m.0.iter().flatten()) {
        *o = x.to_bits();
    }
    out
}

impl FlatKept {
    fn new(r: &Renderer) -> Self {
        let device = &r.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("flat"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/flat.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("flat frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("flat"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("flat frame"),
            size: std::mem::size_of::<FlatUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("flat frame"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        const PLACE: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4
        ];
        const POSITION: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
        const LAND: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let pipeline = |entry: &str, stride: usize, attributes: &[wgpu::VertexAttribute]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("flat"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: stride as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes,
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Instance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &PLACE,
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_flat"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: r.samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            pipelines: [
                pipeline("vs_flat", std::mem::size_of::<MeshGpuVertex>(), &POSITION),
                pipeline("vs_flat", std::mem::size_of::<PosedVertex>(), &POSITION),
                pipeline(
                    "vs_flat_land",
                    std::mem::size_of::<TerrainGpuVertex>(),
                    &LAND,
                ),
            ],
            uniforms,
            group,
            instances: None,
            colors: HashMap::new(),
            boxes: HashMap::new(),
            frame: 0,
            counts: FlatCounts::default(),
        }
    }

    /// The box of a list, worked out the first time it is asked for.
    fn bounds(&mut self, list: &Positions) -> ([f32; 3], [f32; 3]) {
        let frame = self.frame;
        let b = self.boxes.entry(list.key()).or_insert_with(|| {
            let (lo, hi) = list.bounds();
            Boxed {
                _list: list.clone(),
                lo,
                hi,
                used: frame,
            }
        });
        b.used = frame;
        (b.lo, b.hi)
    }

    /// The average color of picture `id`, read with `load` the first time it is asked for.
    fn color(
        &mut self,
        id: u32,
        plain: [f32; 3],
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> [f32; 3] {
        if id == 0 {
            return plain;
        }
        *self
            .colors
            .entry(id)
            .or_insert_with(|| load(id).as_ref().and_then(average_color).unwrap_or(plain))
    }
}

impl Renderer {
    /// What the last frame [`Self::draw_flat`] drew.
    #[must_use]
    pub fn flat_counts(&self) -> FlatCounts {
        self.flat.as_ref().map(|f| f.counts).unwrap_or_default()
    }

    /// Draw `frame` flat ([`Self::draw_flat`]) and read the picture back.
    ///
    /// # Errors
    /// When the picture cannot be read back from the device.
    pub fn render_flat(
        &mut self,
        frame: &Frame,
        colors: &[Option<[f32; 3]>],
        look: &Flat,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> Result<crate::Picture, crate::GpuError> {
        let color = self.draw_flat(frame, colors, look, width, height, load);
        let picture = self.read_back(&color, width, height, true);
        self.end_frame();
        picture
    }

    /// Draw `frame` flat ([`crate::flat`]) at `width` x `height` into a picture left on the device,
    /// as [`Self::draw`] leaves one, reading pictures with `load` only for their average colors.
    /// `colors` gives the objects their own colors, by their places in the frame's objects (none,
    /// or none past its end, for a picture's own).
    #[allow(clippy::too_many_lines)] // one pass: gather, cull, group, draw
    pub fn draw_flat(
        &mut self,
        frame: &Frame,
        colors: &[Option<[f32; 3]>],
        look: &Flat,
        width: u32,
        height: u32,
        load: &mut dyn FnMut(u32) -> Option<Image>,
    ) -> wgpu::Texture {
        let mut own = |id: u32| frame.own_picture(id).or_else(|| load(id));
        let load: &mut dyn FnMut(u32) -> Option<Image> = &mut own;
        if self.flat.is_none() {
            self.flat = Some(FlatKept::new(self));
        }
        let Some(mut kept) = self.flat.take() else {
            unreachable!("made above");
        };
        let cam = &frame.camera;
        let (h, p) = (cam.heading.to_radians(), cam.pitch.to_radians());
        let forward = [
            math::sinf(h) * math::cosf(p),
            math::cosf(h) * math::cosf(p),
            math::sinf(p),
        ];
        let eye = [cam.position.x, cam.position.y, cam.position.z];
        #[allow(clippy::cast_precision_loss)]
        let aspect = width as f32 / height.max(1) as f32;
        let vp = view_proj(
            eye,
            forward,
            cam.roll,
            cam.fov_y,
            cam.orthographic,
            aspect,
            cam.near,
            cam.far,
        );
        let view = Frustum::of_camera(cam, aspect);
        // Seen from inside, only what the doorways show, as the full frame draws.
        let sight = frame.sight(aspect);
        let outdoors = sight
            .as_ref()
            .is_none_or(auberean_draw::cull::Sight::outdoors);
        let shown = |b: &ObjectBatch| sight.as_ref().is_none_or(|s| s.shows(b));

        let mut groups: HashMap<((u8, usize), usize), Group> = HashMap::new();
        let mut order: Vec<((u8, usize), usize)> = Vec::new();
        let mut taken: HashSet<((u8, usize), usize, [u32; 16])> = HashSet::new();
        let mut counts = FlatCounts::default();
        let mut put = |kept: &mut FlatKept,
                       positions: Positions,
                       indices: Indices,
                       place: &Mat4,
                       color: [f32; 4]| {
            let key = (positions.key(), indices.key());
            // A fragment drawn in several layers is drawn once.
            if !taken.insert((key.0, key.1, place_bits(place))) {
                return;
            }
            let (lo, hi) = kept.bounds(&positions);
            if lo[0] > hi[0] {
                return;
            }
            let (lo, hi) = placed(lo, hi, place);
            if view.relation(v(lo), v(hi)) == Relation::Outside {
                counts.culled += 1;
                return;
            }
            let g = groups.entry(key).or_insert_with(|| {
                order.push(key);
                Group {
                    positions,
                    indices,
                    places: Vec::new(),
                }
            });
            g.places.push(Instance {
                model: mat(place),
                color,
            });
        };
        if outdoors {
            for t in frame.terrain.iter().filter(|t| t.layer == 0) {
                let [r, g, b] = kept.color(t.ground, look.plain, load);
                put(
                    &mut kept,
                    Positions::Land(Arc::clone(&t.vertices)),
                    Indices::Long(Arc::clone(&t.indices)),
                    &Mat4::IDENTITY,
                    [r, g, b, if t.far { 2.0 } else { 1.0 }],
                );
            }
            for b in frame.water.iter().filter(|b| b.drawn() && b.shown()) {
                put(
                    &mut kept,
                    Positions::Mesh(Arc::clone(&b.vertices)),
                    Indices::Short(Arc::clone(&b.indices)),
                    &b.transform,
                    [look.water[0], look.water[1], look.water[2], 1.0],
                );
            }
        }
        for (i, b) in frame.objects.iter().enumerate() {
            if b.indices.is_empty() || !b.drawn() || !b.shown() || !shown(b) {
                continue;
            }
            // What only adds light (a glow, a flame) has no surface of its own to show.
            if b.layer.dest_blend == d3d::BLEND_ONE {
                continue;
            }
            let color = match colors.get(i).copied().flatten() {
                Some(c) => c,
                None => {
                    let picture = auberean_draw::warm::texture_set(&b.layer)[0].0;
                    kept.color(picture, look.plain, load)
                }
            };
            let positions = match &b.posed {
                Some(list) => Positions::Posed(Arc::clone(list.shared())),
                None => Positions::Mesh(Arc::clone(&b.vertices)),
            };
            put(
                &mut kept,
                positions,
                Indices::Short(Arc::clone(&b.indices)),
                &b.transform,
                [color[0], color[1], color[2], 1.0],
            );
        }

        // Every draw's lists and places.
        self.meshes.begin_draw();
        let mut placed_all: Vec<Instance> = Vec::new();
        let mut draws: Vec<(
            usize,
            cache::Slice,
            cache::Slice,
            wgpu::IndexFormat,
            u32,
            u32,
            u32,
        )> = Vec::new();
        for key in &order {
            let Some(g) = groups.remove(key) else {
                continue;
            };
            let (stream, vertices, indices, format) = match (&g.positions, &g.indices) {
                (Positions::Mesh(l), Indices::Short(i)) => {
                    let (vb, ib) = self.meshes.buffers(&self.device, l, i);
                    (0, vb, ib, wgpu::IndexFormat::Uint16)
                }
                (Positions::Posed(l), Indices::Short(i)) => {
                    let vb = self.meshes.posed_buffer(&self.device, l);
                    let ib = self.meshes.index_buffer(&self.device, i);
                    (1, vb, ib, wgpu::IndexFormat::Uint16)
                }
                (Positions::Land(l), Indices::Long(i)) => {
                    let (vb, ib) = self.meshes.land_buffers(&self.device, l, i);
                    (2, vb, ib, wgpu::IndexFormat::Uint32)
                }
                _ => continue,
            };
            let first = u32::try_from(placed_all.len()).unwrap_or(u32::MAX);
            let n = u32::try_from(g.places.len()).unwrap_or(u32::MAX);
            counts.instances += n;
            placed_all.extend_from_slice(&g.places);
            draws.push((
                stream,
                vertices,
                indices,
                format,
                g.indices.count(),
                first,
                n,
            ));
        }
        counts.draws = u32::try_from(draws.len()).unwrap_or(u32::MAX);

        let bytes: &[u8] = bytemuck::cast_slice(&placed_all);
        let need = (bytes.len() as u64).max(std::mem::size_of::<Instance>() as u64);
        if kept.instances.as_ref().is_none_or(|b| b.size() < need) {
            let size = need.next_power_of_two();
            kept.instances = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("flat places"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.note_buffer(usize::try_from(size).unwrap_or(usize::MAX));
        }
        let Some(instances) = kept.instances.clone() else {
            unreachable!("made above");
        };
        if !bytes.is_empty() {
            self.queue.write_buffer(&instances, 0, bytes);
        }
        let len = |v: [f32; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        let to_light = look.to_light.map(|x| x / len(look.to_light));
        let [start, end] = look.fade.unwrap_or([0.0, 0.0]);
        let uniforms = FlatUniforms {
            view_proj: mat(&vp),
            eye: [eye[0], eye[1], eye[2], 1.0],
            to_light: [to_light[0], to_light[1], to_light[2], 0.0],
            light: [look.ambient, look.direct, start, end],
            backdrop: [look.backdrop[0], look.backdrop[1], look.backdrop[2], 1.0],
        };
        self.queue
            .write_buffer(&kept.uniforms, 0, bytemuck::bytes_of(&uniforms));

        let ((color, color_view), multi_view, depth_view) = self.frame_targets(width, height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("flat"),
            });
        {
            let (target, resolve) = match &multi_view {
                Some(m) => (m, Some(&color_view)),
                None => (&color_view, None),
            };
            let backdrop = wgpu::Color {
                r: f64::from(look.backdrop[0]),
                g: f64::from(look.backdrop[1]),
                b: f64::from(look.backdrop[2]),
                a: 1.0,
            };
            let mut pass = begin_pass(
                &mut encoder,
                "flat",
                target,
                resolve,
                &depth_view,
                backdrop,
                None,
            );
            pass.set_bind_group(0, &kept.group, &[]);
            pass.set_vertex_buffer(1, instances.slice(..));
            let mut bound = None;
            for (stream, vertices, indices, format, count, first, n) in &draws {
                if bound != Some(*stream) {
                    pass.set_pipeline(&kept.pipelines[*stream]);
                    bound = Some(*stream);
                }
                pass.set_vertex_buffer(0, vertices.get());
                pass.set_index_buffer(indices.get(), *format);
                pass.draw_indexed(0..*count, 0, *first..*first + *n);
            }
        }
        self.meshes.write(&self.queue);
        self.queue.submit([encoder.finish()]);
        let lists = self.meshes.take_counts();
        self.note_work(|w| {
            w.frames += 1;
            w.draws += counts.draws;
            w.mesh_hits += lists.hits;
            w.mesh_misses += lists.misses;
            w.buffers += lists.buffers;
            w.buffer_bytes += lists.buffer_bytes;
            w.written_bytes += lists.written + bytes.len() as u64;
        });
        // Boxes of lists no frame has drawn for a while are let go.
        let now = kept.frame;
        kept.boxes
            .retain(|_, b| now.saturating_sub(b.used) < KEEP_BOXES);
        kept.frame += 1;
        kept.counts = counts;
        self.flat = Some(kept);
        color
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the flat view's shader compiles, and its colors are averages: checks of the drawing tool, no behavior of the game)

    use super::*;

    /// The flat view's shader parses and passes the shader compiler's checks without a device.
    #[test]
    fn the_flat_shader_compiles() {
        let source = include_str!("shaders/flat.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
    }

    /// A picture's color is its smallest level's average, each texel weighed by how opaque it is,
    /// so what a leaf's clear border holds counts for nothing.
    #[test]
    fn a_pictures_color_is_its_smallest_levels_opaque_average() {
        let texel = |b: u8, g: u8, r: u8, a: u8| [b, g, r, a];
        let small = Image {
            width: 2,
            height: 1,
            bgra: [texel(0, 255, 0, 255), texel(255, 255, 255, 0)].concat(),
            smaller: Vec::new(),
        };
        let img = Image {
            width: 4,
            height: 2,
            bgra: vec![255; 32],
            smaller: vec![small],
        };
        let c = average_color(&img).expect("some texel is opaque");
        assert!(
            (c[0] - 0.0).abs() < 1e-6 && (c[1] - 1.0).abs() < 1e-6 && (c[2] - 0.0).abs() < 1e-6
        );
        let clear = Image {
            width: 1,
            height: 1,
            bgra: vec![0; 4],
            smaller: Vec::new(),
        };
        assert_eq!(average_color(&clear), None);
    }

    /// A box carried by a turn and a move holds the corners carried, and no more.
    #[test]
    fn a_placed_box_holds_its_corners_carried() {
        // A quarter turn about up (x to y), then a move of (10, 0, 5), as rows.
        let m = Mat4([
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 0.0, 5.0, 1.0],
        ]);
        let (lo, hi) = placed([0.0, 0.0, 0.0], [2.0, 1.0, 1.0], &m);
        assert_eq!(lo, [9.0, 0.0, 5.0]);
        assert_eq!(hi, [10.0, 2.0, 6.0]);
    }
}
