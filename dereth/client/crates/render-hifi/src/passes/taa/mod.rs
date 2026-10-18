//! Temporal anti-aliasing of the world drawn in the light per pixel.
//!
//! **Each frame is drawn a fraction of a pixel off the last.** Before the passes prepare, the
//! presentation takes the frame's place in a sequence of eight offsets spread evenly over a pixel
//! ([`jitter`]), and every draw of the world is replayed through a copy of its per-frame block
//! shifted by it ([`jittered_frames`]); the art pack's models are drawn shifted the same way. The
//! edges of cut-out surfaces are dithered across a fifth of their alpha's range, a new pattern
//! each frame (the re-shaded variant's alpha test, and the art pack's).
//!
//! **The frames are blended, following what moved.** Three passes share one state:
//! - [`Keep`], once the opaque world is lit: the motion of the things that moved since the last
//!   frame, drawn by the motion variant of their draws over the depth their re-shaded draws left,
//!   and a copy of the picture before the translucent draws.
//! - [`Translucent`], after the translucent draws: where they changed the picture. Smoke, sparks
//!   and glass have no depth of their own to follow, so there the blend takes the current frame.
//! - [`Resolve`], after the air: the blend into the history, and the sharpening of the history
//!   into the picture the rest of the frame works on.
//!
//! What moved is found on the processor ([`Tracker`]): each draw of the opaque world is matched
//! with a draw of the last frame that drew the same thing (the same pipeline, vertex count and
//! textures) where it stood; one that stood nowhere it stands now moved from where the nearest of
//! them stood. Every other pixel follows the camera, worked out from its depth.
//!
//! The history is dropped whenever a frame breaks the sequence: a frame of the world not
//! re-shaded, half a second of the game's clock between two frames, a new size or viewport.
//!
//! `DERETH_HIFI_TAA_BLEND` (the share of the current frame while nothing moves, default 0.1),
//! `DERETH_HIFI_TAA_MOVING` (its share at and past `DERETH_HIFI_TAA_MOVING_PIXELS` of motion,
//! defaults 0.2 and 12) and `DERETH_HIFI_TAA_SHARPEN` (default 0.35) tune it;
//! `DERETH_HIFI_TAA_OBJECTS=0` follows the camera alone, and `DERETH_HIFI_TAA_SHOW=1` shows the
//! blend in place of the picture, to look into it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Range;
use std::rc::Rc;

use dereth_render::device::PerFrameConstants;
use dereth_render::PipelineKey;
use glam::{DMat4, Mat4, Vec2, Vec3};

use crate::build::Build;
use crate::build::Prebuilt;
use crate::derive::cache::PipelineCache;
use crate::derive::motion::MOTION_FORMAT;
use crate::derive::pipeline::HDR_FORMAT;
use crate::derive::{DerivedKey, Variant};
use crate::graph::WarmCx;
use crate::graph::{EncodeCx, HifiPass, PrepareCx};
use crate::passes::lighting::env_f32;
use crate::reshade::{translucent, ReshadePlan};
use crate::resources::{ResourceName, TextureSpec};
use crate::shared::camera::HEIGHT_UP;
use crate::shared::fullscreen::COLOUR_WGSL;
use crate::{
    Caps, DebugView, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, ReplayFilter,
    SidecarContext, SkipRule,
};

const TAA_WGSL: &str = include_str!("taa.wgsl");

/// The passes' shader, for validation without a device.
#[must_use]
pub fn shader_source() -> String {
    format!("{TAA_WGSL}\n{COLOUR_WGSL}")
}

/// The history's format: the blended light, squeezed into 0..1.
const HISTORY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// How many offsets the jitter steps through before it repeats.
pub const SEQUENCE: u32 = 8;

/// How many frames the dither of the edges of cut-outs steps through before it repeats.
const DITHER_SEQUENCE: u32 = 64;

/// How far a draw may have moved from one frame to the next, metres, and still be matched with
/// the draw that drew the same thing in the last frame.
const FARTHEST_MOVE: f32 = 6.0;

/// How close a draw's transform must come to the last frame's for it to be standing still: a
/// tenth of a millimetre of place, and as little of turn and scale.
const STILL: f32 = 1e-4;

/// Whether `s` asks for the anti-aliasing: it is drawn in the light per pixel, and never under a
/// diagnostic view, which shows what the frame holds as it is.
#[must_use]
pub fn wanted(s: &HifiSettings) -> bool {
    s.anti_aliasing && s.lighting.is_on() && s.debug == DebugView::Off
}

/// The `index`th number of the Halton sequence in `base`, 0..1.
#[must_use]
pub fn halton(mut index: u32, base: u32) -> f32 {
    let mut f = 1.0;
    let mut r = 0.0;
    #[allow(clippy::cast_precision_loss)] // small whole numbers
    while index > 0 {
        f /= base as f32;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

/// The jitter of the `n`th frame drawn with the anti-aliasing in a world viewport `viewport`
/// (x, y, width, height, pixels): the offset in normalised device coordinates, within half a pixel
/// each way, and the frame's place in the dither of the edges of cut-outs (1 and over).
#[must_use]
pub fn jitter(n: u32, viewport: [u32; 4]) -> (Vec2, f32) {
    let i = n % SEQUENCE + 1;
    let pixel = Vec2::new(halton(i, 2) - 0.5, halton(i, 3) - 0.5);
    #[allow(clippy::cast_precision_loss)] // a viewport's size and a small count
    let (size, place) = (
        Vec2::new(viewport[2].max(1) as f32, viewport[3].max(1) as f32),
        (n % DITHER_SEQUENCE) as f32 / DITHER_SEQUENCE as f32,
    );
    (
        Vec2::new(pixel.x * 2.0 / size.x, -pixel.y * 2.0 / size.y),
        1.0 + place,
    )
}

/// `block` drawn shifted by `jitter` (normalised device coordinates), with the frame's place
/// `dither` in the dither of the edges of cut-outs.
pub fn jitter_block(block: &mut PerFrameConstants, jitter: Vec2, dither: f32) {
    // The block holds the rows of the column-vector transform.
    let to_clip = Mat4::from_cols_array(&block.view_proj).transpose();
    let shifted = Mat4::from_translation(jitter.extend(0.0)) * to_clip;
    block.view_proj = shifted.transpose().to_cols_array();
    block.screen[2] = dither;
}

/// For each per-frame block a draw of the frame's world was recorded with, the block the replay
/// draws it with at the frame's `jitter` and `dither`: the block `chain` (the passes' say in the
/// replay) hands it, shifted.
pub fn jittered_frames(
    cx: &mut SidecarContext<'_>,
    chain: &mut dyn ReplayFilter,
    jitter: Vec2,
    dither: f32,
) -> HashMap<u32, u32> {
    let Some(span) = cx.tables().world_span() else {
        return HashMap::new();
    };
    let mut offsets: Vec<u32> = cx
        .tables()
        .draws
        .iter()
        .filter(|d| span.contains(&d.cmd))
        .map(|d| d.frame)
        .collect();
    offsets.sort_unstable();
    offsets.dedup();
    let mut map = HashMap::with_capacity(offsets.len());
    for recorded in offsets {
        let base = chain.frame(recorded).unwrap_or(recorded);
        if let Some(shifted) =
            cx.derived_frame_block(base, "jitter", |b| jitter_block(b, jitter, dither))
        {
            map.insert(recorded, shifted);
        }
    }
    map
}

/// The replay's per-frame blocks at the frame's jitter (see [`jittered_frames`]). First among the
/// passes' say in the replay, so its blocks are the ones drawn with.
#[derive(Debug)]
pub struct JitteredFrames<'m>(pub &'m HashMap<u32, u32>);

impl ReplayFilter for JitteredFrames<'_> {
    fn draw(&mut self, _cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        DrawAction::Legacy
    }

    fn frame(&mut self, recorded: u32) -> Option<u32> {
        self.0.get(&recorded).copied()
    }
}

/// What a draw draws, as far as one frame can be told from the next: the pipeline, how many
/// vertices, and the two textures bound for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity {
    /// The pipeline's key.
    pub key: PipelineKey,
    /// How many vertices it draws.
    pub vertices: u32,
    /// The texture slots of its two stages (`u32::MAX` for none).
    pub textures: [u32; 2],
}

/// A quick hash of a few words, for the thousands of draws a frame looks up: each word rotated
/// in and multiplied through.
#[derive(Debug, Default, Clone, Copy)]
pub struct QuickHash(u64);

impl QuickHash {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for QuickHash {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
}

type Quick = BuildHasherDefault<QuickHash>;

/// A transform's floats, bit for bit.
fn bits(m: &Mat4) -> [u32; 16] {
    m.to_cols_array().map(f32::to_bits)
}

/// Whether `a` and `b` place a model within [`STILL`] of each other.
fn same_place(a: &Mat4, b: &Mat4) -> bool {
    a.to_cols_array()
        .iter()
        .zip(b.to_cols_array())
        .all(|(x, y)| (x - y).abs() <= STILL * (1.0 + y.abs()))
}

/// The draws of the last frame, by what they draw, to tell this frame's draws that moved from
/// those that stand still.
#[derive(Debug, Default)]
pub struct Tracker {
    /// Each identity's transforms in the last frame, its drawing space; an identity the last
    /// frame did not draw holds none.
    last: HashMap<Identity, Vec<Mat4>, Quick>,
    /// Each draw of the last frame, by identity and transform bit for bit.
    still: HashSet<(Identity, [u32; 16]), Quick>,
    /// Frames since the identities nothing drew were let go.
    since_pruned: u32,
}

impl Tracker {
    /// Forget the last frame, keeping the storage.
    pub fn forget(&mut self) {
        for v in self.last.values_mut() {
            v.clear();
        }
        self.still.clear();
        self.since_pruned += 1;
        if self.since_pruned >= 120 {
            self.last.retain(|_, v| !v.is_empty());
            self.since_pruned = 0;
        }
    }

    /// Take this frame's draws, `draws` (each with its transform into the frame's drawing space),
    /// and the transform each that moved stood at in the last frame, by its index in `draws`.
    /// `shift` is how far the last frame's drawing space stands from this one's: a point that
    /// stands still is at `p + shift` there. The last frame is then this one.
    pub fn track(&mut self, draws: &[(Identity, Mat4)], shift: Vec3) -> Vec<(usize, Mat4)> {
        let shifted = shift != Vec3::ZERO;
        let mut moved = Vec::new();
        for (i, (id, world)) in draws.iter().enumerate() {
            if !shifted && self.still.contains(&(*id, bits(world))) {
                continue;
            }
            let Some(candidates) = self.last.get(id) else {
                continue;
            };
            // The last frame's draw of the same thing nearest to where this one stands now.
            let here = world.w_axis.truncate();
            let nearest = candidates
                .iter()
                .map(|m| (m, (m.w_axis.truncate() - shift).distance_squared(here)))
                .filter(|(_, d)| *d <= FARTHEST_MOVE * FARTHEST_MOVE)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((was, _)) = nearest else {
                continue;
            };
            if same_place(&(Mat4::from_translation(-shift) * *was), world) {
                continue;
            }
            moved.push((i, *was));
        }
        self.forget();
        for (id, world) in draws {
            self.last.entry(*id).or_default().push(*world);
            self.still.insert((*id, bits(world)));
        }
        moved
    }
}

/// One draw of a thing that moved: the per-frame block its motion is drawn with, and the
/// pipeline.
#[derive(Debug, Clone, Copy)]
struct Moving {
    block: u32,
    key: DerivedKey,
}

/// The last frame drawn with the anti-aliasing, as the next one needs it.
#[derive(Debug, Clone, Copy)]
struct LastFrame {
    /// Its drawing space to its clip space, without the jitter.
    view_projection: Mat4,
    /// What its render space was shifted by from world space.
    shift: Vec3,
    /// The game's clock when it was drawn, seconds.
    clock: f64,
    viewport: [u32; 4],
    size: (u32, u32),
}

/// How long the history is kept between two frames drawn with the anti-aliasing, seconds of the
/// game's clock: past it, what the history holds is too old to follow, and it starts again. By the
/// game's clock, so a frame drawn with the clock held keeps it however long the frame took.
const HISTORY_KEPT: f64 = 0.5;

/// The passes' device objects.
#[derive(Debug)]
struct TaaGpu {
    layout: wgpu::BindGroupLayout,
    resolve: wgpu::RenderPipeline,
    sharpen: wgpu::RenderPipeline,
    translucent: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

impl TaaGpu {
    fn new(device: &wgpu::Device) -> Self {
        let float = wgpu::TextureSampleType::Float { filterable: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("high-fidelity anti-aliasing"),
            entries: &[
                texture_entry(0, float),
                texture_entry(1, wgpu::TextureSampleType::Depth),
                texture_entry(2, float),
                texture_entry(3, float),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                texture_entry(6, float),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("high-fidelity anti-aliasing"),
            source: wgpu::ShaderSource::Wgsl(shader_source().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("high-fidelity anti-aliasing"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, format, write_mask| {
            device.build_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("high-fidelity anti-aliasing"),
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
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            resolve: pipeline("fs_resolve", HISTORY_FORMAT, wgpu::ColorWrites::ALL),
            sharpen: pipeline("fs_sharpen", HDR_FORMAT, wgpu::ColorWrites::COLOR),
            translucent: pipeline("fs_translucent", MOTION_FORMAT, wgpu::ColorWrites::BLUE),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("high-fidelity anti-aliasing"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..wgpu::SamplerDescriptor::default()
            }),
            params: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("high-fidelity anti-aliasing"),
                size: 256,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            layout,
        }
    }

    /// The bindings of one draw: the pictures in binding order.
    fn bind(
        &self,
        device: &wgpu::Device,
        current: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        motion: &wgpu::TextureView,
        history: &wgpu::TextureView,
        before_alpha: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let view = |binding, v| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(v),
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("high-fidelity anti-aliasing"),
            layout: &self.layout,
            entries: &[
                view(0, current),
                view(1, depth),
                view(2, motion),
                view(3, history),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.params.as_entire_binding(),
                },
                view(6, before_alpha),
            ],
        })
    }
}

/// What the three passes share.
#[derive(Debug, Default)]
struct State {
    gpu: Option<TaaGpu>,
    /// The same, built ahead by the warm-up.
    pre_gpu: Prebuilt<TaaGpu>,
    tracker: Tracker,
    /// This frame's draws of things that moved, by command.
    moving: HashMap<u32, Moving>,
    /// The opaque span whose motion is drawn, when the frame draws one.
    opaque: Option<Range<u32>>,
    /// Whether the frame has translucent draws.
    translucent: bool,
    /// Whether this frame's motion and copy were drawn.
    kept: bool,
    /// The last frame drawn with the anti-aliasing.
    last: Option<LastFrame>,
    /// Which of the two histories the last frame wrote.
    written: usize,
    /// Whether that history holds the frames before this one.
    valid: bool,
    /// How many draws moved this frame, and whether the history was used.
    moved_draws: u64,
    blended: bool,
}

impl State {
    /// Whether the device objects are made, or are not being built: the three passes sit out the
    /// frames before they are.
    fn ready(&self) -> bool {
        self.gpu.is_some() || !self.pre_gpu.waits()
    }

    /// The device objects: those built ahead, or made here when there are none.
    fn device_objects(&mut self, device: &wgpu::Device) -> &mut TaaGpu {
        let pre = &mut self.pre_gpu;
        self.gpu
            .get_or_insert_with(|| pre.take().unwrap_or_else(|| TaaGpu::new(device)))
    }

    /// Drop the history: the next frame starts it anew.
    fn reset(&mut self) {
        self.valid = false;
        self.last = None;
        self.tracker.forget();
    }
}

/// Whether the motion of things that moved is followed: `DERETH_HIFI_TAA_OBJECTS=0` follows the
/// camera alone, to compare.
fn objects_followed() -> bool {
    std::env::var("DERETH_HIFI_TAA_OBJECTS").map_or(true, |v| v.trim() != "0")
}

/// The three passes of the anti-aliasing, sharing their state.
#[must_use]
pub fn passes() -> (Keep, Translucent, Resolve) {
    let state = Rc::new(RefCell::new(State::default()));
    (
        Keep(Rc::clone(&state)),
        Translucent(Rc::clone(&state)),
        Resolve(state),
    )
}

/// Once the opaque world is lit: the motion of what moved, and a copy of the picture.
#[derive(Debug)]
pub struct Keep(Rc<RefCell<State>>);

/// After the translucent draws: where they changed the picture.
#[derive(Debug)]
pub struct Translucent(Rc<RefCell<State>>);

/// After the air: the blend and the sharpening.
#[derive(Debug)]
pub struct Resolve(Rc<RefCell<State>>);

/// The texture slots bound for each draw of `tables`, by the draw's index: the last bind of each
/// stage's texture before it.
fn bound_textures(tables: &crate::SideTables) -> Vec<[u32; 2]> {
    let mut out = Vec::with_capacity(tables.draws.len());
    let mut now = [u32::MAX; 2];
    let mut binds = tables.binds.iter().peekable();
    for d in &tables.draws {
        while let Some(b) = binds.next_if(|b| b.cmd < d.cmd) {
            if let Some(slot) = now.get_mut(b.group.saturating_sub(1) as usize) {
                *slot = b.slot.0;
            }
        }
        out.push(now);
    }
    out
}

/// The point at `p` in render space (height third), in the frame's drawing space (height second).
fn drawn(p: Vec3) -> Vec3 {
    (HEIGHT_UP * p.extend(0.0)).truncate()
}

impl Keep {
    /// Find this frame's draws that moved, and make the block each one's motion is drawn with.
    fn track(s: &mut State, cx: &mut PrepareCx<'_, '_>, plan: &ReshadePlan) {
        let cam = &cx.frame.camera;
        let tables = cx.seam.tables();
        let textures = bound_textures(tables);
        let mut draws = Vec::new();
        let mut notes = Vec::new();
        for (d, t) in tables.draws.iter().zip(&textures) {
            if !plan.opaque.contains(&d.cmd)
                || d.splat
                || d.key.vertex_format.is_pre_transformed()
                || translucent(d)
            {
                continue;
            }
            let Some(rows) = cx.seam.draw_world(d.draw) else {
                continue;
            };
            draws.push((
                Identity {
                    key: d.key,
                    vertices: d.vertices,
                    textures: *t,
                },
                Mat4::from_cols_array(&rows).transpose(),
            ));
            notes.push(*d);
        }
        let shift = s
            .last
            .map_or(Vec3::ZERO, |l| drawn(l.shift - cam.block_shift));
        let moved = s.tracker.track(&draws, shift);
        s.moved_draws = moved.len() as u64;
        let Some(last) = s.last else {
            return;
        };
        let viewport = cam.viewport.map(|v| {
            #[allow(clippy::cast_precision_loss)] // pixels
            let f = v as f32;
            f
        });
        for (i, was) in moved {
            let d = notes[i];
            let now = draws[i].1;
            let Some(mut block) = cx.seam.frame_block(d.frame) else {
                continue;
            };
            jitter_block(&mut block, cam.jitter, cam.dither);
            // From where the draw's vertices stand now to where they stood in the last frame's
            // clip space.
            let back = last.view_projection * was * now.inverse();
            block.view = back.transpose().to_cols_array();
            block.fog_color = [cam.jitter.x, cam.jitter.y, 0.0, 0.0];
            block.ambient = viewport;
            let Some(offset) = cx.seam.push_frame_block(&block) else {
                continue;
            };
            let key = DerivedKey {
                key: d.key,
                splat: false,
                variant: Variant::Motion,
            };
            cx.pipelines.ask(key);
            s.moving.insert(d.cmd, Moving { block: offset, key });
        }
    }
}

impl HifiPass for Keep {
    fn name(&self) -> &'static str {
        "anti-aliasing motion"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s) && self.0.borrow().ready()
    }

    fn warm(&mut self, cx: &mut WarmCx<'_>) {
        let mut s = self.0.borrow_mut();
        if wanted(cx.settings) && s.gpu.is_none() {
            let device = cx.device.clone();
            s.pre_gpu.ask(cx.warm, move || TaaGpu::new(&device));
        }
    }

    fn notes(&self, notes: &mut Vec<(&'static str, u64)>) {
        let s = self.0.borrow();
        notes.push(("anti-aliasing draws that moved", s.moved_draws));
        notes.push(("anti-aliasing blended a history", u64::from(s.blended)));
    }

    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        s.moving.clear();
        s.opaque = None;
        s.kept = false;
        s.moved_draws = 0;
        let Some(plan) = ReshadePlan::new(cx.seam.tables()) else {
            s.tracker.forget();
            return Ok(());
        };
        s.translucent = !plan.translucent.is_empty()
            || !plan.alpha.is_empty()
            || plan
                .step
                .as_ref()
                .is_some_and(|p| !p.translucent.is_empty() || !p.alpha.is_empty());
        // The motion of what moved is drawn over one depth: a frame that steps indoors, whose
        // rooms are drawn over a depth cleared after the outdoors, follows the camera alone.
        if plan.steps_indoors() || plan.is_split() || !objects_followed() {
            s.tracker.forget();
            return Ok(());
        }
        Self::track(s, cx, &plan);
        s.opaque = Some(plan.opaque);
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        let Some(rt) = cx.reshade else {
            s.reset();
            return Ok(());
        };
        let device = cx.seam.device().clone();
        let (width, height) = cx.seam.surface_size();
        let motion = cx.resources.texture(
            &device,
            ResourceName::TaaMotion,
            TextureSpec {
                width,
                height,
                format: MOTION_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        )?;
        let usable_depth = cx
            .depth
            .texture()
            .usage()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT);
        let drawn = s
            .opaque
            .clone()
            .filter(|_| usable_depth && !s.moving.is_empty());
        {
            let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("anti-aliasing motion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &motion,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: drawn.as_ref().map(|_| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view: cx.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }
                }),
                timestamp_writes: cx.timer.writes("anti-aliasing motion"),
                occlusion_query_set: None,
                multiview_mask: None,
            });
            cx.seam.count_pass();
            if let Some(range) = drawn {
                let mut filter = MotionReplay {
                    moving: &s.moving,
                    pipelines: cx.pipelines,
                };
                cx.seam.replay_uncounted(range, &mut pass, &mut filter);
            }
        }
        if s.translucent {
            let copy = cx.resources.texture(
                &device,
                ResourceName::TaaBeforeAlpha,
                TextureSpec {
                    width,
                    height,
                    format: HDR_FORMAT,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                },
            )?;
            cx.encoder.copy_texture_to_texture(
                rt.hdr.texture().as_image_copy(),
                copy.texture().as_image_copy(),
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        s.kept = true;
        Ok(())
    }
}

/// The replay of the draws that moved, each with its motion pipeline and block; every other draw
/// is left out.
struct MotionReplay<'a> {
    moving: &'a HashMap<u32, Moving>,
    pipelines: &'a PipelineCache,
}

impl ReplayFilter for MotionReplay<'_> {
    fn draw(&mut self, cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        match self
            .moving
            .get(&cmd)
            .and_then(|m| self.pipelines.get(&m.key).map(|p| (p, m.block)))
        {
            Some((p, block)) => DrawAction::PipelineFrame(p, block),
            None => DrawAction::Skip(SkipRule::OtherReplay),
        }
    }
}

/// The world viewport, x, y, width, height, as floats.
fn viewport_of(cx: &EncodeCx<'_, '_>) -> [f32; 4] {
    let (w, h) = cx.seam.surface_size();
    #[allow(clippy::cast_precision_loss)] // pixels
    cx.seam
        .world_viewport()
        .map_or([0.0, 0.0, w as f32, h as f32], |v| {
            [v.x as f32, v.y as f32, v.width as f32, v.height as f32]
        })
}

/// A full-target draw of `pipeline` into `target`, held to the world viewport.
fn full_draw(
    cx: &mut EncodeCx<'_, '_>,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
    name: &'static str,
) {
    let v = viewport_of(cx);
    let mut pass = cx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(name),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: cx.timer.writes(name),
        occlusion_query_set: None,
        multiview_mask: None,
    });
    cx.seam.count_pass();
    pass.set_viewport(v[0], v[1], v[2], v[3], 0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // whole pixels
    pass.set_scissor_rect(v[0] as u32, v[1] as u32, v[2] as u32, v[3] as u32);
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.draw(0..3, 0..1);
}

impl HifiPass for Translucent {
    fn name(&self) -> &'static str {
        "anti-aliasing translucent"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s) && self.0.borrow().ready()
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        let Some(rt) = cx.reshade else {
            return Ok(());
        };
        if !s.kept || !s.translucent {
            return Ok(());
        }
        let device = cx.seam.device().clone();
        let gpu = s.device_objects(&device);
        let (Some(motion), Some(copy)) = (
            texture_of(cx, ResourceName::TaaMotion),
            texture_of(cx, ResourceName::TaaBeforeAlpha),
        ) else {
            return Ok(());
        };
        let bind = gpu.bind(&device, &rt.hdr, cx.depth, &copy, &copy, &copy);
        full_draw(
            cx,
            &motion,
            &gpu.translucent,
            &bind,
            "anti-aliasing translucent",
        );
        Ok(())
    }
}

/// The shared texture `name`, when a pass has made it this frame.
fn texture_of(cx: &mut EncodeCx<'_, '_>, name: ResourceName) -> Option<wgpu::TextureView> {
    match cx.resources.touch(name) {
        Some(crate::resources::Resource::Texture(_, v)) => Some(v.clone()),
        _ => None,
    }
}

impl Resolve {
    /// The constants of this frame's blend.
    fn constants(
        frame: &HifiFrame,
        last: Option<&LastFrame>,
        viewport: [f32; 4],
        size: (u32, u32),
    ) -> Vec<f32> {
        let cam = &frame.camera;
        let now = (cam.projection * cam.view).as_dmat4();
        let reproject = last.map_or(DMat4::IDENTITY, |l| {
            let shift = drawn(l.shift - cam.block_shift).as_dvec3();
            l.view_projection.as_dmat4() * DMat4::from_translation(shift) * now.inverse()
        });
        #[allow(clippy::cast_precision_loss)] // pixels
        let (w, h) = (size.0 as f32, size.1 as f32);
        let mut c = Vec::with_capacity(32);
        c.extend(viewport);
        c.extend([
            cam.jitter.x,
            cam.jitter.y,
            f32::from(u8::from(last.is_some())),
            env_f32("DERETH_HIFI_TAA_BLEND", 0.1),
        ]);
        c.extend([
            env_f32("DERETH_HIFI_TAA_SHARPEN", 0.35),
            env_f32("DERETH_HIFI_TAA_MOVING", 0.2),
            env_f32("DERETH_HIFI_TAA_MOVING_PIXELS", 12.0),
            1.0,
        ]);
        c.extend([w, h, 1.0 / w.max(1.0), 1.0 / h.max(1.0)]);
        c.extend([env_f32("DERETH_HIFI_TAA_SHOW", 0.0), 0.0, 0.0, 0.0]);
        #[allow(clippy::cast_possible_truncation)]
        // the transform, in single precision for the shader
        c.extend(reproject.to_cols_array().map(|x| x as f32));
        c
    }
}

impl HifiPass for Resolve {
    fn name(&self) -> &'static str {
        "anti-aliasing"
    }

    fn wanted(&self, s: &HifiSettings, _f: &HifiFrame, _caps: &Caps) -> bool {
        wanted(s) && self.0.borrow().ready()
    }

    fn prepare(&mut self, _cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError> {
        Ok(())
    }

    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError> {
        let mut guard = self.0.borrow_mut();
        let s = &mut *guard;
        s.blended = false;
        let Some(rt) = cx.reshade else {
            s.reset();
            return Ok(());
        };
        if !s.kept {
            s.reset();
            return Ok(());
        }
        let device = cx.seam.device().clone();
        let queue = cx.seam.queue().clone();
        let size = cx.seam.surface_size();
        let history: Vec<wgpu::TextureView> =
            [ResourceName::TaaHistory0, ResourceName::TaaHistory1]
                .into_iter()
                .map(|name| {
                    cx.resources.texture(
                        &device,
                        name,
                        TextureSpec {
                            width: size.0,
                            height: size.1,
                            format: HISTORY_FORMAT,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                                | wgpu::TextureUsages::TEXTURE_BINDING,
                        },
                    )
                })
                .collect::<Result<_, _>>()?;
        let Some(motion) = texture_of(cx, ResourceName::TaaMotion) else {
            s.reset();
            return Ok(());
        };
        let cam = cx.frame.camera;
        let now = cx.frame.time;
        // The history holds the frames before this one only when the last frame drawn with it
        // was drawn a moment ago by the game's clock, at this size and in this viewport. (Frames
        // drawn without the world in between, a preview of an item say, leave it as it is.)
        let last = s.last.filter(|l| {
            s.valid
                && (now - l.clock).abs() <= HISTORY_KEPT
                && l.size == size
                && l.viewport == cam.viewport
        });
        s.blended = last.is_some();
        let viewport = viewport_of(cx);
        queue.write_buffer(
            &s.device_objects(&device).params,
            0,
            &Self::constants(cx.frame, last.as_ref(), viewport, size)
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
        let Some(gpu) = s.gpu.as_ref() else {
            return Ok(());
        };
        let read = s.written;
        let write = 1 - read;
        let blend = gpu.bind(
            &device,
            &rt.hdr,
            cx.depth,
            &motion,
            &history[read],
            &history[read],
        );
        full_draw(cx, &history[write], &gpu.resolve, &blend, "anti-aliasing");
        let sharpen = gpu.bind(
            &device,
            &history[write],
            cx.depth,
            &motion,
            &history[write],
            &history[write],
        );
        full_draw(cx, &rt.hdr, &gpu.sharpen, &sharpen, "anti-aliasing sharpen");
        s.written = write;
        s.valid = true;
        s.last = Some(LastFrame {
            view_projection: cam.projection * cam.view,
            shift: cam.block_shift,
            clock: now,
            viewport: cam.viewport,
            size,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi anti-aliasing)
    use super::*;

    #[test]
    fn the_anti_aliasing_shader_parses_and_validates_with_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}"));
        let entries: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            entries,
            ["vs", "fs_resolve", "fs_sharpen", "fs_translucent"]
        );
    }

    #[test]
    fn the_jitter_spreads_eight_frames_over_a_pixel_and_repeats() {
        let viewport = [0, 0, 1920, 1080];
        let offsets: Vec<Vec2> = (0..SEQUENCE).map(|n| jitter(n, viewport).0).collect();
        // Within half a pixel each way.
        for o in &offsets {
            assert!(o.x.abs() * 1920.0 * 0.5 <= 0.5 + 1e-4, "{o}");
            assert!(o.y.abs() * 1080.0 * 0.5 <= 0.5 + 1e-4, "{o}");
        }
        // No two alike, and the mean near the pixel's middle.
        for (i, a) in offsets.iter().enumerate() {
            for b in &offsets[i + 1..] {
                assert!(a.distance(*b) > 1e-5);
            }
        }
        let mean = offsets.iter().copied().sum::<Vec2>() / 8.0;
        assert!(
            (mean.x * 960.0).abs() < 0.1 && (mean.y * 540.0).abs() < 0.1,
            "{mean}"
        );
        assert_eq!(jitter(SEQUENCE, viewport).0, offsets[0]);
        // The dither's place is 1 and over, and steps every frame.
        assert!((1.0..2.0).contains(&jitter(3, viewport).1));
        assert_ne!(jitter(3, viewport).1, jitter(4, viewport).1);
    }

    #[test]
    fn a_jittered_block_moves_every_point_by_the_jitter_on_screen() {
        let to_clip = Mat4::perspective_lh(1.0, 1.6, 0.5, 1000.0)
            * Mat4::look_at_lh(Vec3::new(1.0, 2.0, -5.0), Vec3::ZERO, Vec3::Y);
        let mut block = PerFrameConstants {
            view_proj: to_clip.transpose().to_cols_array(),
            ..PerFrameConstants::default()
        };
        let j = Vec2::new(0.002, -0.001);
        jitter_block(&mut block, j, 1.5);
        let shifted = Mat4::from_cols_array(&block.view_proj).transpose();
        for p in [Vec3::new(0.3, 0.2, 0.1), Vec3::new(-4.0, 1.0, 20.0)] {
            let a = to_clip * p.extend(1.0);
            let b = shifted * p.extend(1.0);
            let moved = b.truncate().truncate() / b.w - a.truncate().truncate() / a.w;
            assert!(moved.distance(j) < 1e-5, "{moved} against {j}");
            assert!((b.z / b.w - a.z / a.w).abs() < 1e-7, "the depth is kept");
        }
        assert_eq!(block.screen[2], 1.5);
    }

    fn id(vertices: u32) -> Identity {
        Identity {
            key: crate::derive::cache::catalogue_keys()[0],
            vertices,
            textures: [3, u32::MAX],
        }
    }

    #[test]
    fn a_draw_that_stands_still_is_not_taken_for_one_that_moved() {
        let mut t = Tracker::default();
        let tree = Mat4::from_translation(Vec3::new(10.0, 0.0, 5.0));
        let other = Mat4::from_translation(Vec3::new(12.0, 0.0, 5.0));
        assert!(t
            .track(&[(id(30), tree), (id(30), other)], Vec3::ZERO)
            .is_empty());
        // The same two, drawn the other way round: neither moved.
        assert!(t
            .track(&[(id(30), other), (id(30), tree)], Vec3::ZERO)
            .is_empty());
    }

    #[test]
    fn a_draw_that_moved_is_matched_with_where_it_stood() {
        let mut t = Tracker::default();
        let at = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 5.0));
        // Two of the same thing, and one of another, in the first frame.
        t.track(
            &[(id(30), at(10.0)), (id(30), at(40.0)), (id(12), at(10.0))],
            Vec3::ZERO,
        );
        // The first walked a little; the other two stood still.
        let moved = t.track(
            &[(id(30), at(10.2)), (id(30), at(40.0)), (id(12), at(10.0))],
            Vec3::ZERO,
        );
        assert_eq!(moved, [(0, at(10.0))]);
        // Something new with nothing like it before has no motion of its own.
        assert!(t.track(&[(id(99), at(1.0))], Vec3::ZERO).is_empty());
    }

    #[test]
    fn a_new_origin_for_the_drawing_space_is_not_taken_for_motion() {
        let mut t = Tracker::default();
        let at = |x: f32| Mat4::from_translation(Vec3::new(x, 0.0, 5.0));
        t.track(&[(id(30), at(10.0))], Vec3::ZERO);
        // The drawing space moved a block east: what stood still is now 192 metres further west
        // in it, and the last frame's place is 192 metres east of this one's.
        assert!(t
            .track(&[(id(30), at(10.0 - 192.0))], Vec3::new(192.0, 0.0, 0.0))
            .is_empty());
    }
}
