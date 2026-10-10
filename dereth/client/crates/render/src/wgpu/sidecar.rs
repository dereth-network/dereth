//! The high-fidelity seam of the `wgpu` device: what a frame sidecar sees of a recorded frame,
//! and how it may act on the replay of the world.
//!
//! A frame is recorded exactly as it is without a sidecar. When one is installed, the end of the
//! frame hands the recording to it instead of replaying it in one pass: the sidecar replays the
//! world between [`Mark::WorldBegin`] and [`Mark::WorldEnd`] into targets of its own, adds its
//! passes, composites the result into the frame's target, and the interface is replayed on top
//! unchanged. Nothing here writes any of the device's own state; a sidecar only reads the
//! recording and appends constant blocks after the recorded ones, so every recorded offset keeps
//! its meaning.
//!
//! **Failure is contained.** Everything a sidecar does in a frame runs inside a device error
//! scope. A failure, its own or the device's, is logged once, the sidecar is uninstalled, and the
//! frame is replayed in one pass exactly as it is without a sidecar, so the picture never misses
//! a frame and the frame is the ordinary one.
//!
//! With no sidecar installed none of this runs.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use super::{apply_viewport, Cmd, Gpu, Texture, DEPTH, FRAME_BLOCK};
use crate::device::{PerFrameConstants, TextureSlot};
use crate::{PipelineKey, RenderError, VertexFormat, Viewport};

pub use crate::hifi_mark::Mark;
/// The graphics API a sidecar draws with, the same version as the device.
pub use wgpu;

/// One recorded draw, noted beside its command so a sidecar can tell what it draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawNote {
    /// The index of the draw's command in the frame's recording.
    pub cmd: u32,
    /// The key the draw's pipeline was built from.
    pub key: PipelineKey,
    /// Whether the draw is a landscape splat, whose pipeline is not one of the keyed ones.
    pub splat: bool,
    /// The offset of the per-frame constant block the draw was recorded with.
    pub frame: u32,
}

/// One recorded texture bind, noted beside the first of its commands: a recorded bind group
/// cannot be told apart from another, so the slot it binds is kept here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindNote {
    /// The index of the bind's command in the frame's recording.
    pub cmd: u32,
    /// The group it binds: 1 the first stage's texture, 2 the second stage's.
    pub group: u32,
    /// The texture slot bound.
    pub slot: TextureSlot,
    /// The sampler descriptor bound with it, when the sampler pair follows the bind.
    pub sampler: u32,
}

/// The side tables of one frame: its marks, draw notes and bind notes, in recording order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SideTables {
    /// Each mark with the index of the command it precedes.
    pub marks: Vec<(u32, Mark)>,
    /// Each draw's note.
    pub draws: Vec<DrawNote>,
    /// Each texture bind's note.
    pub binds: Vec<BindNote>,
    /// The index of every recorded depth clear.
    pub clears: Vec<u32>,
    /// The index of every recorded draw that has no note: its pipeline is none the device keys.
    pub unnoted: Vec<u32>,
}

impl SideTables {
    /// Empty the tables for the next frame, keeping their storage.
    pub fn clear(&mut self) {
        self.marks.clear();
        self.draws.clear();
        self.binds.clear();
        self.clears.clear();
        self.unnoted.clear();
    }

    /// The command range between the first [`Mark::WorldBegin`] and the [`Mark::WorldEnd`] after
    /// it, or `None` when the frame drew no whole world.
    #[must_use]
    pub fn world_span(&self) -> Option<Range<u32>> {
        let begin = self
            .marks
            .iter()
            .position(|(_, m)| *m == Mark::WorldBegin)?;
        let start = self.marks[begin].0;
        let end = self.marks[begin..]
            .iter()
            .find(|(_, m)| *m == Mark::WorldEnd)?
            .0;
        Some(start..end)
    }

    /// The note of the draw at command `cmd`.
    #[must_use]
    pub fn draw_note(&self, cmd: u32) -> Option<&DrawNote> {
        self.draws
            .binary_search_by_key(&cmd, |d| d.cmd)
            .ok()
            .map(|i| &self.draws[i])
    }
}

/// Why a draw of the world replay was left out.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkipRule {
    /// A sky dome object, replaced by a sky the sidecar draws itself.
    SkyDome,
    /// A landscape cell whose geometry the sidecar already holds in a retained copy.
    CoveredByFeed,
    /// A draw a pass replaces with draws of its own at the same point.
    ReplacedByPass,
}

/// What the world replay does with one recorded draw.
#[derive(Debug)]
pub enum DrawAction<'p> {
    /// Replay it as recorded.
    Legacy,
    /// Leave it out, for the reason given.
    Skip(SkipRule),
    /// Replay it with another pipeline of the same layout.
    Pipeline(&'p wgpu::RenderPipeline),
    /// Replay it with another pipeline and another per-frame constant block offset.
    PipelineFrame(&'p wgpu::RenderPipeline, u32),
    /// Replay it with its recorded pipeline and another per-frame constant block offset.
    Frame(u32),
}

/// How each recorded draw of a replay was handled: the replay's census.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Census {
    /// Replayed as recorded.
    pub legacy: u32,
    /// Replayed with another pipeline.
    pub reshaded: u32,
    /// Replayed with the recorded pipeline and another per-frame block.
    pub reframed: u32,
    /// Left out as sky dome objects.
    pub skipped_sky_dome: u32,
    /// Left out as covered by a retained copy.
    pub skipped_covered_by_feed: u32,
    /// Left out as replaced by a pass's own draws.
    pub skipped_replaced_by_pass: u32,
}

impl Census {
    /// Count one action, `reframed` saying whether the draw's per-frame block was replaced: a
    /// draw replayed as recorded with another block is counted as re-framed.
    pub fn count_framed(&mut self, action: &DrawAction<'_>, reframed: bool) {
        if reframed && matches!(action, DrawAction::Legacy) {
            self.reframed += 1;
        } else {
            self.count(action);
        }
    }

    /// Count one action.
    pub fn count(&mut self, action: &DrawAction<'_>) {
        match action {
            DrawAction::Legacy => self.legacy += 1,
            DrawAction::Pipeline(_) | DrawAction::PipelineFrame(..) => self.reshaded += 1,
            DrawAction::Frame(_) => self.reframed += 1,
            DrawAction::Skip(SkipRule::SkyDome) => self.skipped_sky_dome += 1,
            DrawAction::Skip(SkipRule::CoveredByFeed) => self.skipped_covered_by_feed += 1,
            DrawAction::Skip(SkipRule::ReplacedByPass) => self.skipped_replaced_by_pass += 1,
        }
    }

    /// Every draw counted.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.legacy
            + self.reshaded
            + self.reframed
            + self.skipped_sky_dome
            + self.skipped_covered_by_feed
            + self.skipped_replaced_by_pass
    }

    /// Add `other`'s counts to these.
    pub fn add(&mut self, other: &Census) {
        self.legacy += other.legacy;
        self.reshaded += other.reshaded;
        self.reframed += other.reframed;
        self.skipped_sky_dome += other.skipped_sky_dome;
        self.skipped_covered_by_feed += other.skipped_covered_by_feed;
        self.skipped_replaced_by_pass += other.skipped_replaced_by_pass;
    }
}

/// A pass's say in the world replay: what to do with each draw, and draws of its own at a mark.
pub trait ReplayFilter {
    /// The action for the recorded draw at command `cmd`, with its note when it has one.
    fn draw(&mut self, cmd: u32, note: Option<&DrawNote>) -> DrawAction<'_>;

    /// The replay has reached `mark`, with the world pass open. A pass may record draws of its
    /// own here, for instance water before the held-back translucent draws. The pass's state
    /// afterwards is put back as the recording had it.
    fn mark(&mut self, mark: Mark, pass: &mut wgpu::RenderPass<'_>) {
        let _ = (mark, pass);
    }

    /// Another per-frame constant block for every draw recorded with the block at `recorded`,
    /// whatever pipeline draws it, or `None` to keep it. An action that names its own block keeps
    /// that one.
    fn frame(&mut self, recorded: u32) -> Option<u32> {
        let _ = recorded;
        None
    }
}

/// The filter that replays every draw as recorded.
#[derive(Debug, Clone, Copy, Default)]
pub struct LegacyReplay;

impl ReplayFilter for LegacyReplay {
    fn draw(&mut self, _cmd: u32, _note: Option<&DrawNote>) -> DrawAction<'_> {
        DrawAction::Legacy
    }
}

/// A sidecar's failure. The device logs it once, uninstalls the sidecar and draws the frame as it
/// would without one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("high-fidelity presentation failed: {0}")]
pub struct SidecarError(pub String);

/// Why a sidecar was not installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HifiRefused {
    /// The device is not the `wgpu` one.
    #[error("high-fidelity presentation needs the wgpu renderer")]
    NotWgpu,
    /// The device was made without the high-fidelity request.
    #[error("the device was made without high-fidelity support")]
    NotRequested,
    /// The device lacks what the presentation draws with: compute shaders with storage buffers,
    /// and four colour targets.
    #[error("this graphics backend cannot draw the high-fidelity presentation")]
    Unsupported,
    /// A sidecar is installed already.
    #[error("a high-fidelity sidecar is installed already")]
    AlreadyInstalled,
}

/// What a sidecar will do with the frame it has prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarFrame {
    /// Draw the world and composite it: the device calls [`FrameSidecar::encode`].
    Composite,
    /// Draw this frame the ordinary way, in one pass, and stay installed: the sidecar has no
    /// snapshot for it, or nothing ready to draw yet.
    Plain,
}

/// What the installed sidecar did with the last frame, as plain data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SidecarReport {
    /// Whether the frame was composited by the sidecar, rather than drawn the ordinary way.
    pub composited: bool,
    /// How the world replay handled each recorded draw.
    pub census: Census,
    /// The passes that ran, in order.
    pub passes: Vec<&'static str>,
    /// Each pass's GPU time, milliseconds, where the device has timestamps. These lag the frame
    /// by a few frames, the time the device takes to hand them back.
    pub gpu_ms: Vec<(&'static str, f32)>,
    /// The CPU time the sidecar took in the frame, milliseconds.
    pub cpu_ms: f32,
    /// How many pieces of background work (pipeline builds) the sidecar is still waiting for.
    pub waiting: u32,
    /// Whatever else the sidecar counted about the frame, by name.
    pub notes: Vec<(&'static str, u64)>,
}

/// The blend the ordinary pipeline of `key` draws with, `None` for a draw that does not blend.
#[must_use]
pub fn ordinary_blend(key: &PipelineKey) -> Option<wgpu::BlendState> {
    key.alpha_blend.then(|| wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: super::blend_factor(key.src_blend),
            dst_factor: super::blend_factor(key.dst_blend),
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: super::blend_factor(key.src_blend.alpha_factor()),
            dst_factor: super::blend_factor(key.dst_blend.alpha_factor()),
            operation: wgpu::BlendOperation::Add,
        },
    })
}

/// A pipeline that draws the recorded vertices of `key` as the ordinary pipeline of `key` does
/// -- the same vertex layout, triangle winding and cull, depth test and depth write, against the
/// same depth format -- with `module`'s `vs_main` and `fragment` stages, into `targets`. With
/// `layout` one of the recorded layouts, it replays a recorded draw with the recorded binds.
#[must_use]
pub fn build_derived_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    fragment: &str,
    key: &PipelineKey,
    targets: &[Option<wgpu::ColorTargetState>],
) -> wgpu::RenderPipeline {
    use dereth_render_cpu::pso::{CullFace, FrontFace, FRONT_FACE};
    let (attributes, stride) = SidecarContext::legacy_vertex_layout(key);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("high-fidelity derived"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: match FRONT_FACE {
                FrontFace::Clockwise => wgpu::FrontFace::Cw,
                FrontFace::CounterClockwise => wgpu::FrontFace::Ccw,
            },
            cull_mode: match key.cull.face() {
                CullFace::None => None,
                CullFace::Front => Some(wgpu::Face::Front),
                CullFace::Back => Some(wgpu::Face::Back),
            },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(key.z_write),
            depth_compare: Some(super::compare(key.z_func)),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The features a high-fidelity device asks for where the adapter has them.
pub const HIFI_FEATURES: wgpu::Features = wgpu::Features::FLOAT32_FILTERABLE
    .union(wgpu::Features::TIMESTAMP_QUERY)
    .union(wgpu::Features::DEPTH_CLIP_CONTROL);

/// Whether `adapter` traces rays: a device asked for the high-fidelity presentation on it asks for
/// its ray queries.
pub(super) fn adapter_rays(adapter: &wgpu::Adapter) -> bool {
    adapter
        .features()
        .contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY)
}

/// The ordinary request widened for the high-fidelity presentation: the adapter's share of
/// [`HIFI_FEATURES`] and of ray queries, and the adapter's own limits for the bindings and targets
/// the presentation's passes use.
pub(super) fn widen_request(
    adapter: &wgpu::Adapter,
    features: wgpu::Features,
    mut limits: wgpu::Limits,
) -> (wgpu::Features, wgpu::Limits, wgpu::ExperimentalFeatures) {
    let have = adapter.features();
    let s = adapter.limits();
    let mut features = features | (have & HIFI_FEATURES);
    let mut experimental = wgpu::ExperimentalFeatures::default();
    if adapter_rays(adapter) {
        // SAFETY: the token only permits asking for the adapter's experimental ray queries; the
        // presentation uses them through `wgpu`'s own validated API, never on the ordinary path.
        experimental = unsafe { wgpu::ExperimentalFeatures::enabled() };
        features |= wgpu::Features::EXPERIMENTAL_RAY_QUERY;
        limits.max_blas_primitive_count = s.max_blas_primitive_count;
        limits.max_blas_geometry_count = s.max_blas_geometry_count;
        limits.max_tlas_instance_count = s.max_tlas_instance_count;
        limits.max_acceleration_structures_per_shader_stage =
            s.max_acceleration_structures_per_shader_stage;
        limits.max_buffers_and_acceleration_structures_per_shader_stage =
            s.max_buffers_and_acceleration_structures_per_shader_stage;
    }
    limits.max_storage_textures_per_shader_stage = s.max_storage_textures_per_shader_stage;
    limits.max_color_attachments = s.max_color_attachments;
    limits.max_color_attachment_bytes_per_sample = s.max_color_attachment_bytes_per_sample;
    limits.max_uniform_buffer_binding_size = s.max_uniform_buffer_binding_size;
    limits.max_samplers_per_shader_stage = s.max_samplers_per_shader_stage;
    limits.max_storage_buffer_binding_size = s.max_storage_buffer_binding_size;
    limits.max_non_sampler_bindings = s.max_non_sampler_bindings;
    (features, limits, experimental)
}

/// The installed sidecar and the frame's side tables.
pub(super) struct Installed {
    sidecar: Box<dyn FrameSidecar>,
    /// The marks of the frame being recorded, with the index of the command each precedes.
    marks: RefCell<Vec<(u32, Mark)>>,
    tables: SideTables,
    report: SidecarReport,
}

impl Installed {
    /// A new frame is being recorded.
    pub(super) fn begin_frame(&mut self) {
        self.marks.get_mut().clear();
    }
}

/// The recorded state at a point in the frame: the viewport in force and the last bind of each
/// group.
struct ReplayState<'c> {
    viewport: Viewport,
    binds: [Option<&'c wgpu::BindGroup>; 5],
}

/// What the replay draws with: the device's recorded frame and the resources it was recorded
/// against.
struct Replayer<'a> {
    commands: &'a [Cmd],
    pipelines: &'a [wgpu::RenderPipeline],
    clear_depth: &'a wgpu::RenderPipeline,
    white: &'a wgpu::BindGroup,
    default_samplers: &'a wgpu::BindGroup,
    full: Viewport,
    vertex_buffer: Option<wgpu::Buffer>,
    uniform_bind: Option<wgpu::BindGroup>,
}

impl Replayer<'_> {
    /// The state the recording is in just before command `at`.
    fn state_at(&self, at: usize) -> ReplayState<'_> {
        let mut state = ReplayState {
            viewport: self.full,
            binds: [None; 5],
        };
        for cmd in &self.commands[..at.min(self.commands.len())] {
            match cmd {
                Cmd::Viewport(v) => state.viewport = *v,
                Cmd::Bind { group, bind } => {
                    if let Some(b) = state.binds.get_mut(*group as usize) {
                        *b = Some(bind);
                    }
                }
                Cmd::ClearDepth(_) | Cmd::Draw { .. } => {}
            }
        }
        state
    }

    /// Replay `range` into `pass`, which was just begun, asking `filter` about every draw and
    /// handing it every mark of `marks` the range reaches. The pass starts in the state the
    /// recording is in at the range's start, exactly as the one-pass replay of the whole frame
    /// would have it there.
    fn replay(
        &self,
        range: Range<usize>,
        marks: &[(u32, Mark)],
        tables: &SideTables,
        pass: &mut wgpu::RenderPass<'_>,
        filter: &mut dyn ReplayFilter,
    ) -> Census {
        let mut census = Census::default();
        let Some(uniform_bind) = self.uniform_bind.as_ref() else {
            return census;
        };
        let end = range.end.min(self.commands.len());
        let start = range.start.min(end);
        let mut state = self.state_at(start);
        let restore = |pass: &mut wgpu::RenderPass<'_>, state: &ReplayState<'_>| {
            pass.set_bind_group(1, state.binds[1].unwrap_or(self.white), &[]);
            pass.set_bind_group(2, state.binds[2].unwrap_or(self.white), &[]);
            pass.set_bind_group(3, state.binds[3].unwrap_or(self.default_samplers), &[]);
            if let Some(b) = state.binds[4] {
                pass.set_bind_group(4, b, &[]);
            }
            pass.set_bind_group(0, uniform_bind, &[0, 0]);
        };
        restore(pass, &state);
        if state.viewport != self.full {
            apply_viewport(pass, state.viewport);
        }
        let mut current = None;
        let mut next_mark = marks.partition_point(|(at, _)| (*at as usize) < start);
        for (i, cmd) in self.commands[start..end].iter().enumerate() {
            let at = start + i;
            while next_mark < marks.len() && marks[next_mark].0 as usize == at {
                filter.mark(marks[next_mark].1, pass);
                next_mark += 1;
                // Whatever the filter drew, the recording continues in its own state.
                current = None;
                restore(pass, &state);
                apply_viewport(pass, state.viewport);
            }
            match cmd {
                Cmd::Viewport(v) => {
                    state.viewport = *v;
                    apply_viewport(pass, *v);
                }
                Cmd::ClearDepth(v) => {
                    apply_viewport(pass, *v);
                    pass.set_pipeline(self.clear_depth);
                    current = None;
                    pass.draw(0..3, 0..1);
                    apply_viewport(pass, state.viewport);
                }
                Cmd::Bind { group, bind } => {
                    if let Some(b) = state.binds.get_mut(*group as usize) {
                        *b = Some(bind);
                    }
                    pass.set_bind_group(*group, bind, &[]);
                }
                Cmd::Draw {
                    pipeline,
                    vertices,
                    count,
                    frame,
                    draw,
                } => {
                    let Some(vb) = self.vertex_buffer.as_ref() else {
                        continue;
                    };
                    let at32 = u32::try_from(at).unwrap_or(u32::MAX);
                    let remap = filter.frame(*frame);
                    let action = filter.draw(at32, tables.draw_note(at32));
                    census.count_framed(&action, remap.is_some());
                    match action {
                        DrawAction::Legacy => {
                            if current != Some(*pipeline) {
                                pass.set_pipeline(&self.pipelines[*pipeline]);
                                current = Some(*pipeline);
                            }
                            let f = remap.unwrap_or(*frame);
                            pass.set_bind_group(0, uniform_bind, &[f, *draw]);
                            pass.set_vertex_buffer(0, vb.slice(vertices.clone()));
                            pass.draw(0..*count, 0..1);
                        }
                        DrawAction::Skip(_) => {}
                        DrawAction::Frame(f) => {
                            if current != Some(*pipeline) {
                                pass.set_pipeline(&self.pipelines[*pipeline]);
                                current = Some(*pipeline);
                            }
                            pass.set_bind_group(0, uniform_bind, &[f, *draw]);
                            pass.set_vertex_buffer(0, vb.slice(vertices.clone()));
                            pass.draw(0..*count, 0..1);
                        }
                        DrawAction::Pipeline(p) => {
                            pass.set_pipeline(p);
                            current = None;
                            let f = remap.unwrap_or(*frame);
                            pass.set_bind_group(0, uniform_bind, &[f, *draw]);
                            pass.set_vertex_buffer(0, vb.slice(vertices.clone()));
                            pass.draw(0..*count, 0..1);
                        }
                        DrawAction::PipelineFrame(p, f) => {
                            pass.set_pipeline(p);
                            current = None;
                            pass.set_bind_group(0, uniform_bind, &[f, *draw]);
                            pass.set_vertex_buffer(0, vb.slice(vertices.clone()));
                            pass.draw(0..*count, 0..1);
                        }
                    }
                }
            }
        }
        // Marks at the range's end that close it, up to the next world bracket.
        while next_mark < marks.len() && marks[next_mark].0 as usize == end {
            let m = marks[next_mark].1;
            if matches!(m, Mark::WorldBegin | Mark::WorldEnd) {
                break;
            }
            filter.mark(m, pass);
            next_mark += 1;
        }
        census
    }
}

/// What a sidecar sees of one recorded frame, and the few things it may add to it.
pub struct SidecarContext<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    surface_size: (u32, u32),
    world_viewport: Option<Viewport>,
    frame_stamp: u64,
    compute: bool,
    tables: &'a SideTables,
    uniform_arena: &'a mut Vec<u8>,
    recorded_uniform_len: usize,
    /// The arenas are on the device: nothing more can be appended to them.
    sealed: bool,
    layout: &'a wgpu::PipelineLayout,
    uniform_layout: &'a wgpu::BindGroupLayout,
    texture_layout: &'a wgpu::BindGroupLayout,
    sampler_layout: &'a wgpu::BindGroupLayout,
    splat_layout: Option<&'a wgpu::PipelineLayout>,
    textures: &'a HashMap<u32, Texture>,
    /// Each derived block, by the recorded block it was derived from and the name of its edit.
    derived_frames: HashMap<(u32, &'static str), u32>,
    replayer: Replayer<'a>,
    census: Census,
    passes: u32,
}

impl std::fmt::Debug for SidecarContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SidecarContext")
            .field("surface_format", &self.surface_format)
            .field("surface_size", &self.surface_size)
            .field("world_viewport", &self.world_viewport)
            .field("frame_stamp", &self.frame_stamp)
            .field("commands", &self.replayer.commands.len())
            .field("sealed", &self.sealed)
            .finish_non_exhaustive()
    }
}

/// The constant arena's block alignment.
const BLOCK_ALIGN: usize = 256;

impl<'a> SidecarContext<'a> {
    /// The device.
    #[must_use]
    pub fn device(&self) -> &wgpu::Device {
        self.device
    }

    /// The device's queue.
    #[must_use]
    pub fn queue(&self) -> &wgpu::Queue {
        self.queue
    }

    /// The format of the frame's target.
    #[must_use]
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// The frame's target size in pixels.
    #[must_use]
    pub fn surface_size(&self) -> (u32, u32) {
        self.surface_size
    }

    /// The viewport in force where the world begins, or `None` when the frame drew no world.
    #[must_use]
    pub fn world_viewport(&self) -> Option<Viewport> {
        self.world_viewport
    }

    /// The stamp of the frame being ended: the device's presented-frame count before it.
    #[must_use]
    pub fn frame_stamp(&self) -> u64 {
        self.frame_stamp
    }

    /// Whether the device runs compute shaders with storage buffers.
    #[must_use]
    pub fn compute(&self) -> bool {
        self.compute
    }

    /// The frame's marks and notes.
    #[must_use]
    pub fn tables(&self) -> &SideTables {
        self.tables
    }

    /// How many commands the frame recorded.
    #[must_use]
    pub fn command_count(&self) -> u32 {
        u32::try_from(self.replayer.commands.len()).unwrap_or(u32::MAX)
    }

    /// The recorded constant block at `offset`.
    #[must_use]
    fn uniform_bytes(&self, offset: u32) -> Option<&[u8; BLOCK_ALIGN]> {
        let start = usize::try_from(offset).ok()?;
        if start + BLOCK_ALIGN > self.recorded_uniform_len {
            return None;
        }
        self.uniform_arena[start..start + BLOCK_ALIGN]
            .try_into()
            .ok()
    }

    /// Append a constant block after every recorded one, and return its offset; recorded offsets
    /// keep their meaning. Only while the frame is being prepared: `None` once the arenas are on
    /// the device.
    pub fn push_uniform_block(&mut self, bytes: &[u8]) -> Option<u32> {
        if self.sealed {
            return None;
        }
        let start = self.uniform_arena.len().next_multiple_of(BLOCK_ALIGN);
        let end = start + bytes.len().max(1).next_multiple_of(BLOCK_ALIGN);
        self.uniform_arena.resize(end, 0);
        self.uniform_arena[start..start + bytes.len()].copy_from_slice(bytes);
        u32::try_from(start).ok()
    }

    /// The recorded per-frame block at `legacy_offset` with `edit` applied to it, appended once
    /// per offset and edit name per frame; `None` when no such block was recorded, or once the
    /// arenas are on the device. `name` says what the edit does: two callers asking for the same
    /// name get the same copy, so one name must always mean one edit, and different edits of one
    /// block must carry different names.
    pub fn derived_frame_block(
        &mut self,
        legacy_offset: u32,
        name: &'static str,
        edit: impl FnOnce(&mut PerFrameConstants),
    ) -> Option<u32> {
        if let Some(o) = self.derived_frames.get(&(legacy_offset, name)) {
            return Some(*o);
        }
        let mut block = decode_frame_block(self.uniform_bytes(legacy_offset)?);
        edit(&mut block);
        let offset = self.push_uniform_block(&encode_frame_block(&block))?;
        self.derived_frames.insert((legacy_offset, name), offset);
        Some(offset)
    }

    /// The recorded per-frame block at `legacy_offset`, decoded.
    #[must_use]
    pub fn frame_block(&self, legacy_offset: u32) -> Option<PerFrameConstants> {
        self.uniform_bytes(legacy_offset).map(decode_frame_block)
    }

    /// The length of the constant arena as the frame recorded it.
    #[must_use]
    pub fn recorded_uniform_len(&self) -> usize {
        self.recorded_uniform_len
    }

    /// The layout every recorded pipeline was built with: the constants, two textures and the
    /// sampler pair.
    #[must_use]
    pub fn pipeline_layout(&self) -> &wgpu::PipelineLayout {
        self.layout
    }

    /// The landscape splat's layout, once the device has made one.
    #[must_use]
    pub fn splat_pipeline_layout(&self) -> Option<&wgpu::PipelineLayout> {
        self.splat_layout
    }

    /// The layout of group 0, the two constant blocks.
    #[must_use]
    pub fn uniform_bind_layout(&self) -> &wgpu::BindGroupLayout {
        self.uniform_layout
    }

    /// The layout of groups 1 and 2, one texture each.
    #[must_use]
    pub fn texture_bind_layout(&self) -> &wgpu::BindGroupLayout {
        self.texture_layout
    }

    /// The layout of group 3, the sampler pair.
    #[must_use]
    pub fn sampler_bind_layout(&self) -> &wgpu::BindGroupLayout {
        self.sampler_layout
    }

    /// The texture resident in `slot` this frame. Slots are recycled, so look it up each frame.
    #[must_use]
    pub fn texture(&self, slot: TextureSlot) -> Option<&wgpu::Texture> {
        self.textures.get(&slot.0).map(|t| &t.texture)
    }

    /// The vertex attributes a recorded pipeline of `key` reads, and their stride: a pipeline
    /// drawing recorded vertices reads them the same way.
    #[must_use]
    pub fn legacy_vertex_layout(key: &PipelineKey) -> (Vec<wgpu::VertexAttribute>, u64) {
        use dereth_render_cpu::vertex::AttributeFormat;
        let attributes = key
            .vertex_format
            .elements()
            .iter()
            .map(|(e, offset)| wgpu::VertexAttribute {
                shader_location: e.location(),
                offset: u64::from(*offset),
                format: match e.attribute_format(false) {
                    AttributeFormat::Float2 => wgpu::VertexFormat::Float32x2,
                    AttributeFormat::Float3 => wgpu::VertexFormat::Float32x3,
                    AttributeFormat::Float4 => wgpu::VertexFormat::Float32x4,
                    AttributeFormat::Rgba8Unorm => wgpu::VertexFormat::Unorm8x4,
                    AttributeFormat::Bgra8Unorm => wgpu::VertexFormat::Unorm8x4Bgra,
                },
            })
            .collect();
        (attributes, u64::from(key.vertex_format.stride()))
    }

    /// The pipeline that clears depth over the viewport, as a recorded depth clear does.
    #[must_use]
    pub fn clear_depth_pipeline(&self) -> &wgpu::RenderPipeline {
        self.replayer.clear_depth
    }

    /// Replay the recorded commands in `range` into `pass`, a pass just begun on targets of the
    /// frame target's format and the depth format, asking `filter` about each draw. Only while
    /// the frame is being encoded; before that it replays nothing.
    pub fn replay(
        &mut self,
        range: Range<u32>,
        pass: &mut wgpu::RenderPass<'_>,
        filter: &mut dyn ReplayFilter,
    ) -> Census {
        let census = self.replayer.replay(
            range.start as usize..range.end as usize,
            &self.tables.marks,
            self.tables,
            pass,
            filter,
        );
        self.census.add(&census);
        census
    }

    /// Replay as [`SidecarContext::replay`] does, without adding the draws to the frame's
    /// census: for a replay that only works out something about the frame, such as a depth of its
    /// own, and draws none of the picture.
    pub fn replay_uncounted(
        &self,
        range: Range<u32>,
        pass: &mut wgpu::RenderPass<'_>,
        filter: &mut dyn ReplayFilter,
    ) -> Census {
        self.replayer.replay(
            range.start as usize..range.end as usize,
            &self.tables.marks,
            self.tables,
            pass,
            filter,
        )
    }

    /// How the replays so far this frame handled each recorded draw.
    #[must_use]
    pub fn census(&self) -> Census {
        self.census
    }

    /// Note one render pass begun, for the device's count of them.
    pub fn count_pass(&mut self) {
        self.passes += 1;
    }

    /// The depth format the recorded pipelines test against.
    #[must_use]
    pub const fn depth_format() -> wgpu::TextureFormat {
        DEPTH
    }
}

/// A recorded per-frame block, decoded.
fn decode_frame_block(bytes: &[u8; BLOCK_ALIGN]) -> PerFrameConstants {
    let mut floats = bytes[..FRAME_BLOCK]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b));
    let mut take = |out: &mut [f32]| {
        for f in out {
            *f = floats.next().unwrap_or(0.0);
        }
    };
    let mut block = PerFrameConstants::default();
    take(&mut block.view_proj);
    take(&mut block.view);
    take(&mut block.fog_params);
    take(&mut block.fog_color);
    take(&mut block.ambient);
    take(&mut block.screen);
    block
}

/// A per-frame block, encoded as the device records one.
fn encode_frame_block(block: &PerFrameConstants) -> Vec<u8> {
    [
        &block.view_proj[..],
        &block.view[..],
        &block.fog_params[..],
        &block.fog_color[..],
        &block.ambient[..],
        &block.screen[..],
    ]
    .concat()
    .iter()
    .flat_map(|f| f.to_le_bytes())
    .collect()
}

/// A frame sidecar: draws the world a second way from the frame's recording.
pub trait FrameSidecar: Any {
    /// The CPU half of the frame: read the recording, append constant blocks, upload its own
    /// buffers. Runs before the frame's arenas are uploaded.
    ///
    /// # Errors
    /// A failure uninstalls the sidecar and the frame is drawn as it would be without one.
    fn prepare(&mut self, cx: &mut SidecarContext<'_>) -> Result<SidecarFrame, SidecarError>;

    /// The GPU half: record into `encoder` the frame up to the end of the world, composited into
    /// `target`, which covers the whole frame. The device then replays the interface over it, with
    /// [`FrameSidecar::world_depth`] as its depth.
    ///
    /// # Errors
    /// As for [`FrameSidecar::prepare`].
    fn encode(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) -> Result<(), SidecarError>;

    /// The depth the interface is replayed over: the world's, as it stood at the world's end.
    fn world_depth(&self) -> Option<&wgpu::TextureView> {
        None
    }

    /// The frame has been handed to the device's queue.
    fn submitted(&mut self) {}

    /// Add what the sidecar knows of the frame to `report`: its passes and their times.
    fn report(&self, report: &mut SidecarReport) {
        let _ = report;
    }

    /// Wait up to `timeout` for the sidecar's background work, such as pipelines it is building,
    /// and say whether none is left. Nothing in a frame calls this; it is for a caller that wants
    /// the sidecar's full picture before it looks, such as a capture.
    fn settle(&mut self, timeout: std::time::Duration) -> bool {
        let _ = timeout;
        true
    }

    /// The sidecar as [`Any`], so its owner can reach its own type.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// The legacy shader source for `format`, as this device compiles it.
#[must_use]
pub fn legacy_shader_source(format: VertexFormat) -> String {
    super::shader_source(format)
}

/// The landscape splat shader source for `format`, as this device compiles it.
#[must_use]
pub fn splat_shader_source(format: VertexFormat) -> String {
    super::terrain::splat_source(format)
}

/// What became of a frame the sidecar was asked to draw.
enum Outcome {
    /// Composited by the sidecar.
    Composited(wgpu::CommandBuffer),
    /// Drawn the ordinary way, the sidecar staying installed.
    Plain,
    /// The sidecar failed, and is uninstalled.
    Failed(String),
}

impl Gpu {
    /// Note that the recording has reached `mark`. Nothing is kept when no sidecar is installed.
    pub fn hifi_mark(&self, mark: Mark) {
        if let Some(s) = &self.sidecar {
            let at = u32::try_from(self.commands.borrow().len()).unwrap_or(u32::MAX);
            s.marks.borrow_mut().push((at, mark));
        }
    }

    /// Install `sidecar`, which draws the world from the next frame on. A past failure is
    /// forgotten.
    ///
    /// # Errors
    /// [`HifiRefused::NotRequested`] on a device made without the high-fidelity request;
    /// [`HifiRefused::Unsupported`] on one that lacks what it draws with;
    /// [`HifiRefused::AlreadyInstalled`] when one is installed already.
    pub fn hifi_install(&mut self, sidecar: Box<dyn FrameSidecar>) -> Result<(), HifiRefused> {
        if !self.hifi_requested {
            return Err(HifiRefused::NotRequested);
        }
        if !self.hifi_supported() {
            return Err(HifiRefused::Unsupported);
        }
        if self.sidecar.is_some() {
            return Err(HifiRefused::AlreadyInstalled);
        }
        self.hifi_failed = None;
        self.sidecar = Some(Installed {
            sidecar,
            marks: RefCell::new(Vec::new()),
            tables: SideTables::default(),
            report: SidecarReport::default(),
        });
        Ok(())
    }

    /// Uninstall the sidecar and hand it back; the device draws its ordinary frame again.
    pub fn hifi_take(&mut self) -> Option<Box<dyn FrameSidecar>> {
        self.sidecar.take().map(|i| i.sidecar)
    }

    /// The installed sidecar.
    pub fn hifi_sidecar_mut(&mut self) -> Option<&mut dyn FrameSidecar> {
        match self.sidecar.as_mut() {
            Some(i) => Some(i.sidecar.as_mut()),
            None => None,
        }
    }

    /// Why the last sidecar was uninstalled, if one failed.
    #[must_use]
    pub fn hifi_failed(&self) -> Option<String> {
        self.hifi_failed.clone()
    }

    /// What the installed sidecar did with the last frame.
    #[must_use]
    pub fn hifi_report(&self) -> Option<SidecarReport> {
        self.sidecar.as_ref().map(|i| i.report.clone())
    }

    /// Wait up to `timeout` for the installed sidecar's background work, and whether none is
    /// left; true with no sidecar.
    pub fn hifi_settle(&mut self, timeout: std::time::Duration) -> bool {
        self.sidecar
            .as_mut()
            .is_none_or(|i| i.sidecar.settle(timeout))
    }

    /// Whether the device was asked for what the high-fidelity presentation draws with.
    #[must_use]
    pub fn hifi_requested(&self) -> bool {
        self.hifi_requested
    }

    /// Whether the presentation traces rays here, which the lamps need: a device asked for what it
    /// draws with takes the adapter's ray queries, so a device made without that request answers
    /// for the one a start with it makes.
    #[must_use]
    pub fn hifi_rays(&self) -> bool {
        self.adapter_rays
    }

    /// Whether the device has what the presentation draws with: compute shaders with storage
    /// buffers, and four colour targets.
    #[must_use]
    pub fn hifi_supported(&self) -> bool {
        self.compute && self.device.limits().max_color_attachments >= 4
    }

    /// The features and limits the device was made with.
    #[must_use]
    pub fn device_features(&self) -> (wgpu::Features, wgpu::Limits) {
        (self.device.features(), self.device.limits())
    }

    /// The frame's side tables: its marks, and a note for every draw and texture bind, read from
    /// the device's own tables without changing them.
    // Bind groups are keyed by identity, which their interior state never changes.
    #[allow(clippy::mutable_key_type)]
    fn fill_tables(&self, installed: &mut Installed, commands: &[Cmd]) {
        let t = &mut installed.tables;
        t.clear();
        t.marks.append(installed.marks.get_mut());
        let mut keys: HashMap<usize, (PipelineKey, bool)> = self
            .pipeline_index
            .iter()
            .map(|(k, i)| (*i, (*k, false)))
            .collect();
        if let Some(s) = &self.terrain_splat {
            keys.extend(s.pipelines.iter().map(|(k, i)| (*i, (*k, true))));
        }
        let slots: HashMap<&wgpu::BindGroup, u32> = self
            .textures
            .iter()
            .map(|(s, tex)| (&tex.bind, *s))
            .collect();
        let pairs = self.sampler_pairs.borrow();
        let samplers: HashMap<&wgpu::BindGroup, u32> =
            pairs.iter().map(|(k, g)| (g, k.0)).collect();
        for (i, cmd) in commands.iter().enumerate() {
            let at = u32::try_from(i).unwrap_or(u32::MAX);
            match cmd {
                Cmd::Draw {
                    pipeline, frame, ..
                } => {
                    if let Some((key, splat)) = keys.get(pipeline) {
                        t.draws.push(DrawNote {
                            cmd: at,
                            key: *key,
                            splat: *splat,
                            frame: *frame,
                        });
                    } else {
                        t.unnoted.push(at);
                    }
                }
                Cmd::ClearDepth(_) => t.clears.push(at),
                Cmd::Bind { group, bind } if *group == 1 || *group == 2 => {
                    if let Some(slot) = slots.get(bind) {
                        let sampler = commands[i + 1..]
                            .iter()
                            .take(2)
                            .find_map(|c| match c {
                                Cmd::Bind { group: 3, bind } => samplers.get(bind).copied(),
                                _ => None,
                            })
                            .unwrap_or(u32::MAX);
                        t.binds.push(BindNote {
                            cmd: at,
                            group: *group,
                            slot: TextureSlot(*slot),
                            sampler,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// The end of a frame with a sidecar installed: the sidecar prepares and draws the world, and
    /// the interface is replayed over it; or, when the sidecar fails or has nothing to draw, the
    /// frame is replayed in one pass as it would be without one.
    #[allow(clippy::too_many_lines)]
    pub(super) fn end_frame_sidecar(
        &mut self,
        commands: &[Cmd],
        surface_texture: Option<wgpu::SurfaceTexture>,
        view: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        let Some(mut installed) = self.sidecar.take() else {
            return Ok(());
        };
        #[cfg(not(target_arch = "wasm32"))]
        let started = std::time::Instant::now();
        // A frame that drew no world is drawn the ordinary way, and needs no notes.
        if installed
            .marks
            .get_mut()
            .iter()
            .any(|(_, m)| *m == Mark::WorldBegin)
        {
            self.fill_tables(&mut installed, commands);
        } else {
            installed.tables.clear();
            installed.marks.get_mut().clear();
        }
        let (fw, fh) = self.size;
        let full = Viewport {
            x: 0,
            y: 0,
            width: fw,
            height: fh,
        };
        let world = installed.tables.world_span();
        let world_viewport = world.as_ref().map(|w| {
            commands[..(w.start as usize).min(commands.len())]
                .iter()
                .rev()
                .find_map(|c| match c {
                    Cmd::Viewport(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or(full)
        });
        let recorded_uniform_len = self.uniform_arena.len();
        let default_samplers = self.sampler_pair(0, 1);
        let frame_depth = self.depth_view();
        let mut census = Census::default();
        let mut sidecar_passes = 0u32;
        #[cfg(not(target_arch = "wasm32"))]
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);

        // The CPU half.
        let prepared = match &world {
            None => Ok(SidecarFrame::Plain),
            Some(_) => {
                let mut cx = SidecarContext {
                    device: &self.device,
                    queue: &self.queue,
                    surface_format: self.format,
                    surface_size: self.size,
                    world_viewport,
                    frame_stamp: self.frame_stamp,
                    compute: self.compute,
                    tables: &installed.tables,
                    uniform_arena: &mut self.uniform_arena,
                    recorded_uniform_len,
                    sealed: false,
                    layout: &self.layout,
                    uniform_layout: &self.uniform_layout,
                    texture_layout: &self.texture_layout,
                    sampler_layout: &self.sampler_layout,
                    splat_layout: self.terrain_splat.as_ref().map(|s| &s.layout),
                    textures: &self.textures,
                    derived_frames: HashMap::new(),
                    replayer: Replayer {
                        commands,
                        pipelines: &self.pipelines,
                        clear_depth: &self.clear_depth_pipeline,
                        white: &self.white,
                        default_samplers: &default_samplers,
                        full,
                        vertex_buffer: None,
                        uniform_bind: None,
                    },
                    census: Census::default(),
                    passes: 0,
                };
                installed.sidecar.prepare(&mut cx)
            }
        };

        // The arenas, with whatever the sidecar appended after the recorded constants.
        let vertex_buffer = self.vertex_buffer_for_frame();
        let (uniform_buffer, uniform_bind) = self.uniform_buffer_for_frame();
        // A browser cannot wait for the device's verdict, so there it is never revised.
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut outcome = match prepared {
            Err(e) => Outcome::Failed(e.0),
            Ok(SidecarFrame::Plain) => Outcome::Plain,
            Ok(SidecarFrame::Composite) => {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("high-fidelity frame"),
                        });
                let mut cx = SidecarContext {
                    device: &self.device,
                    queue: &self.queue,
                    surface_format: self.format,
                    surface_size: self.size,
                    world_viewport,
                    frame_stamp: self.frame_stamp,
                    compute: self.compute,
                    tables: &installed.tables,
                    uniform_arena: &mut self.uniform_arena,
                    recorded_uniform_len,
                    sealed: true,
                    layout: &self.layout,
                    uniform_layout: &self.uniform_layout,
                    texture_layout: &self.texture_layout,
                    sampler_layout: &self.sampler_layout,
                    splat_layout: self.terrain_splat.as_ref().map(|s| &s.layout),
                    textures: &self.textures,
                    derived_frames: HashMap::new(),
                    replayer: Replayer {
                        commands,
                        pipelines: &self.pipelines,
                        clear_depth: &self.clear_depth_pipeline,
                        white: &self.white,
                        default_samplers: &default_samplers,
                        full,
                        vertex_buffer: vertex_buffer.clone(),
                        uniform_bind: Some(uniform_bind.clone()),
                    },
                    census: Census::default(),
                    passes: 0,
                };
                match installed.sidecar.encode(&mut cx, &mut encoder, view) {
                    Err(e) => Outcome::Failed(e.0),
                    Ok(()) => {
                        census = cx.census;
                        sidecar_passes = cx.passes;
                        let world_end = world.as_ref().map_or(0, |w| w.end as usize);
                        let replayer = &cx.replayer;
                        let depth = installed
                            .sidecar
                            .world_depth()
                            .unwrap_or(&frame_depth)
                            .clone();
                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("interface"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view,
                                    depth_slice: None,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Load,
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: Some(
                                    wgpu::RenderPassDepthStencilAttachment {
                                        view: &depth,
                                        depth_ops: Some(wgpu::Operations {
                                            load: wgpu::LoadOp::Load,
                                            store: wgpu::StoreOp::Discard,
                                        }),
                                        stencil_ops: None,
                                    },
                                ),
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            });
                            replayer.replay(
                                world_end..commands.len(),
                                &[],
                                &installed.tables,
                                &mut pass,
                                &mut LegacyReplay,
                            );
                        }
                        Outcome::Composited(encoder.finish())
                    }
                }
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(e) = pollster::block_on(scope.pop()) {
            if !matches!(outcome, Outcome::Failed(_)) {
                outcome = Outcome::Failed(format!("the device refused a step: {e}"));
            }
        }

        let composited = matches!(outcome, Outcome::Composited(_));
        let mut failed = None;
        let buffer = match outcome {
            Outcome::Composited(buffer) => buffer,
            Outcome::Plain | Outcome::Failed(_) => {
                if let Outcome::Failed(why) = outcome {
                    failed = Some(why);
                }
                let depth = frame_depth;
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("frame"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("frame"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    let replayer = Replayer {
                        commands,
                        pipelines: &self.pipelines,
                        clear_depth: &self.clear_depth_pipeline,
                        white: &self.white,
                        default_samplers: &default_samplers,
                        full,
                        vertex_buffer,
                        uniform_bind: Some(uniform_bind),
                    };
                    replayer.replay(
                        0..commands.len(),
                        &[],
                        &installed.tables,
                        &mut pass,
                        &mut LegacyReplay,
                    );
                }
                encoder.finish()
            }
        };
        #[cfg(feature = "test-support")]
        {
            self.passes_encoded += 1 + u64::from(sidecar_passes) * u64::from(composited);
        }
        let _ = sidecar_passes;
        drop(uniform_buffer);
        self.queue.submit([buffer]);
        if let Some(t) = surface_texture {
            self.queue.present(t);
        }
        self.last_present_sync_interval = Some(self.present_sync_interval);
        self.frame_stamp += 1;
        self.uniform_arena.truncate(recorded_uniform_len);
        if let Some(why) = failed {
            // Logged once: the sidecar is gone, and nothing installs another until the
            // presentation's settings change.
            tracing::warn!(
                target: "dereth::render",
                "the high-fidelity presentation stopped and the ordinary picture is drawn: {why}"
            );
            self.hifi_failed = Some(why);
            return Ok(());
        }
        installed.sidecar.submitted();
        let mut report = SidecarReport {
            composited,
            census,
            ..SidecarReport::default()
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            report.cpu_ms = started.elapsed().as_secs_f32() * 1000.0;
        }
        installed.sidecar.report(&mut report);
        installed.report = report;
        self.sidecar = Some(installed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world span runs from the world's first mark to the end mark after it.
    #[test]
    fn the_world_span_runs_from_its_begin_mark_to_its_end_mark() {
        let mut t = SideTables::default();
        assert_eq!(t.world_span(), None);
        t.marks.push((1, Mark::WorldBegin));
        assert_eq!(t.world_span(), None);
        t.marks.push((5, Mark::AlphaFlush));
        t.marks.push((9, Mark::WorldEnd));
        assert_eq!(t.world_span(), Some(1..9));
    }

    /// A per-frame block survives decoding and encoding unchanged.
    #[test]
    fn a_frame_block_decodes_and_encodes_unchanged() {
        let mut block = PerFrameConstants::default();
        for (i, f) in block.view_proj.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *f = i as f32 * 0.5;
            }
        }
        block.fog_params = [1.0, 2.0, 1.0, 0.0];
        block.screen = [0.25, -1.0, 0.0, 0.0];
        let bytes = encode_frame_block(&block);
        assert_eq!(bytes.len(), FRAME_BLOCK);
        let mut slot = [0u8; BLOCK_ALIGN];
        slot[..FRAME_BLOCK].copy_from_slice(&bytes);
        let back = decode_frame_block(&slot);
        assert_eq!(back.view_proj, block.view_proj);
        assert_eq!(back.fog_params, block.fog_params);
        assert_eq!(back.screen, block.screen);
    }
}
