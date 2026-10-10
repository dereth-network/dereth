//! The optional high-fidelity presentation: draws the world a second time, from the frame the
//! renderer recorded, with modern lighting, sun shadows, bounced light, occlusion, lamps and a
//! physical sky and weather, changing pixels only.
//!
//! **Depends on** the device and its seam (`dereth-render`, feature `hifi`), the device-free half
//! of rendering (`dereth-render-cpu`), `dereth-primitives`, `wgpu` and `glam`. **Used by** the
//! scene (`dereth-scene`), behind its `hifi` feature, and nothing else.
//!
//! **Must never** reach the simulation, physics, world, network, runtime or interface crates, or
//! a platform: it is handed plain data and recorded draws, and it changes only pixels.
//!
//! The renderer records every frame as it always has. When the player turns this presentation
//! on, the device hands the frame's recording to a [`HifiRenderer`] at the end of the frame
//! instead of replaying it in one pass. The renderer replays the world into targets of its own,
//! runs its passes over them (light, shadow, bounced light, occlusion, lamps, sky, weather),
//! composites the result into the world viewport, and the interface is drawn over it unchanged.
//!
//! Everything the game reads -- what is drawn, what can be picked, where the camera may go, what
//! the server is told -- comes from the ordinary frame, which is recorded in full whether or not
//! this presentation is on. The scene hands it a plain owned [`HifiFrame`] after the frame has
//! been walked.
//!
//! **Off is the default, and off costs nothing.** With every option off no renderer is installed
//! and the device draws exactly the frame it draws without this crate. With the renderer
//! installed, a pass whose option is off is never prepared and allocates nothing.

use std::any::Any;
use std::fmt;

pub mod composite;
pub mod derive;
pub mod fail;
pub mod graph;
pub mod passes;
pub mod reshade;
pub mod resources;
pub mod shared;
pub mod snapshot;
pub mod timing;

pub use dereth_render::hifi_mark::Mark;
pub use dereth_render::wgpu::sidecar::{
    BindNote, Census, DrawAction, DrawNote, FrameSidecar, HifiRefused, LegacyReplay, ReplayFilter,
    SideTables, SidecarContext, SidecarError, SidecarFrame, SidecarReport, SkipRule,
};
pub use graph::{EncodeCx, HifiPass, PrepareCx};

use composite::{CompositeMode, Compositor};
use derive::cache::{catalogue_keys, BuildContext, PipelineCache};
use derive::{DerivedKey, Variant};
use graph::{Chain, Slot};
use reshade::{ReshadeFilter, ReshadePlan, ReshadeTargets, Reshader};
pub use snapshot::HifiFrame;
use timing::GpuTimer;

/// A quality level, as the options name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Level {
    /// The feature is off.
    #[default]
    Off,
    /// The cheapest form.
    Low,
    /// A middle form.
    Medium,
    /// The full form at ordinary cost.
    High,
    /// Everything, whatever it costs.
    Ultra,
}

impl Level {
    /// A level from its stored number: 0 Off up to 4 Ultra, anything higher Ultra.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Off,
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            _ => Self::Ultra,
        }
    }

    /// Whether the feature is on at all.
    #[must_use]
    pub const fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// The diagnostic views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DebugView {
    /// The ordinary picture.
    #[default]
    Off,
    /// The world replayed and composited with no pass at all: the same pixels as off.
    Passthrough,
    /// The world's depth.
    Depth,
    /// The world's normals.
    Normals,
    /// The ambient occlusion term.
    Ao,
    /// The sun's shadow term.
    Shadows,
    /// How each draw of the world replay was handled.
    Census,
    /// The world re-shaded and resolved with no pass of its own: within a level of the ordinary
    /// frame, the check that re-shading keeps the picture.
    Parity,
}

impl DebugView {
    /// A view from its stored number; an unknown number is off.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Passthrough,
            2 => Self::Depth,
            3 => Self::Normals,
            4 => Self::Ao,
            5 => Self::Shadows,
            6 => Self::Census,
            7 => Self::Parity,
            _ => Self::Off,
        }
    }
}

/// What the player asked this presentation for. Plain data: the scene maps the player's
/// preferences onto it, and nothing here reads a preference itself. Everything is off by default.
///
/// The light per pixel is the base the shadows, the bounced light and the lamps are drawn inside:
/// without it they draw nothing. The occlusion, the sky and the weather stand on their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HifiSettings {
    /// Per-pixel sun and sky light, high dynamic range, bloom and exposure. `Low` is the light
    /// alone; `Medium` and up also draw the sun's shadow cascades.
    pub lighting: Level,
    /// The sun's shadow cascades, drawn inside the light (with `lighting` on).
    pub shadows: Level,
    /// Light bounced from the surfaces around, inside the light (with `lighting` on).
    pub global_illumination: Level,
    /// Ambient and contact occlusion; left out of the outdoor frames the bounced light is drawn
    /// into, whose creases it already darkens, and drawn everywhere else.
    pub ambient_occlusion: Level,
    /// Outdoor lamps, lanterns, torches and braziers as light sources after dusk, with traced
    /// visibility (with `lighting` on, on a device with ray queries).
    pub lamps: Level,
    /// A physical sky and aerial perspective.
    pub sky: Level,
    /// Rain, snow and wet ground, when the day's own weather brings them.
    pub weather: Level,
    /// A diagnostic view.
    pub debug: DebugView,
}

impl HifiSettings {
    /// Whether anything at all would be drawn differently: at least one option or diagnostic
    /// view is on.
    #[must_use]
    pub fn any_effective(&self) -> bool {
        *self != Self::default()
    }

    /// Whether these settings turn off an effect, or leave a diagnostic view, that `before` had
    /// on.
    #[must_use]
    pub fn turns_off_any_of(&self, before: &Self) -> bool {
        let pairs = [
            (self.lighting, before.lighting),
            (self.shadows, before.shadows),
            (self.global_illumination, before.global_illumination),
            (self.ambient_occlusion, before.ambient_occlusion),
            (self.lamps, before.lamps),
            (self.sky, before.sky),
            (self.weather, before.weather),
        ];
        pairs.iter().any(|(now, was)| was.is_on() && !now.is_on())
            || (before.debug != DebugView::Off && self.debug != before.debug)
    }
}

/// What the device can do that a pass may need, read once when the renderer is made. Every pass
/// checks its own needs against it at run time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct Caps {
    /// Compute shaders with storage buffers and textures.
    pub compute: bool,
    /// GPU timestamps.
    pub timestamp_query: bool,
    /// Linear filtering of 32-bit float textures.
    pub float32_filterable: bool,
    /// Unclipped depth.
    pub depth_clip_control: bool,
    /// Ray queries in shaders.
    pub ray_query: bool,
}

impl Caps {
    /// The capabilities of a device with `features`, which runs compute shaders with storage
    /// buffers when `compute` says so.
    #[must_use]
    pub fn from_features(features: wgpu::Features, compute: bool) -> Self {
        Self {
            compute,
            timestamp_query: features.contains(wgpu::Features::TIMESTAMP_QUERY),
            float32_filterable: features.contains(wgpu::Features::FLOAT32_FILTERABLE),
            depth_clip_control: features.contains(wgpu::Features::DEPTH_CLIP_CONTROL),
            ray_query: features.contains(wgpu::Features::EXPERIMENTAL_RAY_QUERY),
        }
    }
}

/// A failure inside this presentation. The device logs it once, uninstalls the renderer and
/// draws the ordinary frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HifiError {
    /// The device refused something it was asked for.
    Device(String),
    /// An allocation would pass the video memory budget.
    Budget {
        /// What the allocation would take, in bytes.
        wanted: u64,
        /// What the budget has left, in bytes.
        left: u64,
    },
    /// A shader could not be derived from the shared source.
    Derive(String),
    /// The renderer cannot draw this frame yet.
    NotReady(&'static str),
}

impl fmt::Display for HifiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Device(e) => write!(f, "device: {e}"),
            Self::Budget { wanted, left } => {
                write!(f, "video memory budget: {wanted} bytes wanted, {left} left")
            }
            Self::Derive(e) => write!(f, "shader derivation: {e}"),
            Self::NotReady(what) => write!(f, "not ready: {what}"),
        }
    }
}

impl std::error::Error for HifiError {}

impl From<HifiError> for SidecarError {
    fn from(e: HifiError) -> Self {
        SidecarError(e.to_string())
    }
}

/// The high-fidelity renderer: the device's frame sidecar.
#[derive(Debug)]
pub struct HifiRenderer {
    settings: HifiSettings,
    caps: Caps,
    graph: graph::Graph,
    /// Which of the graph's passes this frame runs, in the graph's order.
    wanted: Vec<bool>,
    resources: resources::Resources,
    frame: Option<HifiFrame>,
    latch: fail::FailLatch,
    timer: Option<GpuTimer>,
    compositor: Compositor,
    world_depth: Option<wgpu::TextureView>,
    /// The derived pipelines, built off the frame's thread.
    pipelines: PipelineCache,
    /// Whether the device's whole catalogue has been asked for.
    catalogue_asked: bool,
    /// How this frame is re-shaded, when it is.
    plan: Option<ReshadePlan>,
    /// How the draws to re-shade stood this frame.
    readiness: reshade::Readiness,
    /// How many draws this frame's plan replays as recorded before the re-shade.
    recorded_draws: (u64, u64),
    reshader: Reshader,
}

impl HifiRenderer {
    /// A renderer with `settings`. It holds no GPU resource until a frame needs one.
    #[must_use]
    pub fn new(settings: HifiSettings) -> Self {
        Self {
            settings,
            caps: Caps::default(),
            graph: graph::Graph::standard(),
            wanted: Vec::new(),
            resources: resources::Resources::new(resources::DEFAULT_BUDGET),
            frame: None,
            latch: fail::FailLatch::default(),
            timer: None,
            compositor: Compositor::default(),
            world_depth: None,
            pipelines: PipelineCache::new(),
            catalogue_asked: false,
            plan: None,
            readiness: reshade::Readiness::default(),
            recorded_draws: (0, 0),
            reshader: Reshader::default(),
        }
    }

    /// The settings in force.
    #[must_use]
    pub fn settings(&self) -> &HifiSettings {
        &self.settings
    }

    /// Change the settings. Every shared resource is released, and a failure is forgotten, so
    /// the new settings get a fresh start. When an effect is turned off its passes are made
    /// anew, so what they held for it (a shadow map, a traced scene, their own targets) is
    /// given back at once rather than when the last effect goes.
    pub fn configure(&mut self, settings: HifiSettings) {
        if settings != self.settings {
            if settings.turns_off_any_of(&self.settings) {
                self.graph = graph::Graph::standard();
                self.wanted.clear();
            }
            self.settings = settings;
            self.resources.release_all();
            self.reshader.forget();
            self.world_depth = None;
            self.latch.reset();
        }
    }

    /// Hand over the snapshot of the frame the scene has just walked.
    pub fn receive_frame(&mut self, frame: HifiFrame) {
        self.frame = Some(frame);
    }

    /// The snapshot of the frame being ended, if the scene handed one over.
    #[must_use]
    pub fn frame(&self) -> Option<&HifiFrame> {
        self.frame.as_ref()
    }

    /// The last snapshot, handed back so its storage can be refilled for the next frame.
    pub fn recycle_frame(&mut self) -> Option<HifiFrame> {
        self.frame.take()
    }

    /// The names of the passes this frame would run, in pass order.
    #[must_use]
    pub fn wanted_passes(&self) -> Vec<&'static str> {
        match &self.frame {
            Some(frame) => self.graph.wanted(&self.settings, frame, &self.caps),
            None => Vec::new(),
        }
    }

    /// The first failure, once one has happened.
    #[must_use]
    pub fn failed(&self) -> Option<&str> {
        self.latch.failed()
    }

    /// The latest pass timings the device has handed back.
    #[must_use]
    pub fn timing(&self) -> Option<&timing::Timing> {
        self.timer.as_ref().map(GpuTimer::last)
    }

    /// How many derived pipelines are built, and how many are still being built.
    #[must_use]
    pub fn pipelines(&self) -> (usize, usize) {
        (self.pipelines.ready_count(), self.pipelines.waiting())
    }

    /// Whether the last frame was re-shaded.
    #[must_use]
    pub fn reshaded(&self) -> bool {
        self.plan.is_some()
    }

    /// How the draws to re-shade stood in the last frame.
    #[must_use]
    pub fn readiness(&self) -> reshade::Readiness {
        self.readiness
    }

    /// The bytes of video memory the renderer holds.
    #[must_use]
    pub fn resident_bytes(&self) -> u64 {
        self.resources.used()
    }

    /// Ask for every derived pipeline this frame needs, take back those built, and plan the
    /// re-shade once every draw it covers is ready.
    fn prepare_reshade(&mut self, cx: &mut SidecarContext<'_>) -> Result<(), HifiError> {
        let Some(frame) = self.frame.as_ref() else {
            return Ok(());
        };
        let build = BuildContext {
            device: cx.device().clone(),
            layout: cx.pipeline_layout().clone(),
            splat_layout: cx.splat_pipeline_layout().cloned(),
        };
        let mut last = None;
        for note in &cx.tables().draws {
            // Draws come in runs of one pipeline: each run is asked for once.
            let key = reshade::derived_key(note);
            if last != Some(key) {
                self.pipelines.ask(key);
                last = Some(key);
            }
        }
        if !self.catalogue_asked {
            // After this frame's own: the keys the device can draw anything with.
            for key in catalogue_keys() {
                self.pipelines.ask(DerivedKey {
                    key,
                    splat: false,
                    variant: Variant::Reshade,
                });
            }
            self.catalogue_asked = true;
        }
        self.pipelines.pump(&build);
        let Some(plan) = ReshadePlan::new(cx.tables()) else {
            return Ok(());
        };
        let ready = reshade::readiness(&plan, cx.tables(), &self.pipelines);
        self.readiness = ready;
        if ready.failed > 0 {
            let why = self
                .pipelines
                .failures()
                .first()
                .map_or_else(String::new, |(k, e)| format!("{k:?}: {e}"));
            return Err(HifiError::Derive(why));
        }
        if ready.all_ready() && ready.ready > 0 {
            self.reshader
                .prepare_field(cx, frame, &mut self.resources)?;
            let count = |r: std::ops::Range<u32>| {
                cx.tables()
                    .draws
                    .iter()
                    .filter(|d| r.contains(&d.cmd))
                    .count() as u64
            };
            self.recorded_draws = (count(plan.recorded_before()), count(plan.rest.clone()));
            self.plan = Some(plan);
        }
        Ok(())
    }

    /// Latch `e` and hand it to the device as the frame's failure.
    fn fail(&mut self, e: &HifiError) -> SidecarError {
        self.latch.trip(e);
        SidecarError(e.to_string())
    }

    /// The GPU half of a composited frame.
    fn encode_frame(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) -> Result<(), HifiError> {
        let frame = self
            .frame
            .as_ref()
            .ok_or(HifiError::NotReady("no snapshot of the frame"))?;
        let timer = self
            .timer
            .as_mut()
            .ok_or(HifiError::NotReady("the frame was not prepared"))?;
        let targets = Compositor::targets(cx, &mut self.resources)?;
        let rt = match &self.plan {
            Some(_) => Some(ReshadeTargets::new(cx, &mut self.resources)?),
            None => None,
        };
        let mut colour = targets.colour.clone();
        // What the passes read: the depth each pixel sees, once a frame that stamps a building's
        // openings has had it worked out (see `composite`).
        let mut depth = targets.depth.clone();
        let mut world_depth = targets.depth.clone();
        let last_stamp = composite::last_building_stamp(cx.tables());
        let mut unstamped = None;
        let mut resolved = rt.is_none();
        for slot in Slot::ORDER {
            if matches!(slot, Slot::WorldReplay | Slot::AlphaReplay) {
                let mut filters: Vec<&mut dyn ReplayFilter> = self
                    .graph
                    .passes_mut()
                    .zip(&self.wanted)
                    .filter(|(_, wanted)| **wanted)
                    .filter_map(|((_, p), _)| p.replay_filter())
                    .collect();
                match (&self.plan, &rt) {
                    (Some(plan), Some(rt)) => {
                        if slot == Slot::WorldReplay {
                            // The sky's first pass with its own pipelines, but with the passes'
                            // say in it (the physical sky over the authored dome).
                            let mut pre = Chain(filters);
                            Compositor::replay_legacy(
                                cx,
                                encoder,
                                &targets,
                                plan.recorded_before(),
                                true,
                                timer.writes("sky replay"),
                                &mut pre,
                            );
                            filters = pre.0;
                        }
                        let mut reshade = ReshadeFilter::new(&self.pipelines);
                        filters.push(&mut reshade);
                        let mut chain = Chain(filters);
                        if slot == Slot::WorldReplay {
                            self.reshader.lift(
                                cx,
                                encoder,
                                &targets.colour,
                                rt,
                                timer.writes("sky lift"),
                            );
                            let mut pass = reshade::reshade_pass(
                                cx,
                                encoder,
                                rt,
                                &targets.depth,
                                "world re-shade",
                                timer.writes("world re-shade"),
                            );
                            cx.replay(plan.opaque.clone(), &mut pass, &mut chain);
                            drop(pass);
                            if let Some(last) = last_stamp {
                                let u = Compositor::unstamped_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    0..last + 1,
                                    timer.writes("unstamped depth"),
                                )?;
                                depth = self.compositor.seen_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    &targets.depth,
                                    &u,
                                    timer.writes("seen depth"),
                                )?;
                                if !plan.is_split() {
                                    world_depth = depth.clone();
                                }
                                unstamped = Some(u);
                            }
                            self.reshader.ground_normals(
                                cx,
                                encoder,
                                rt,
                                &depth,
                                frame,
                                &mut self.resources,
                                timer.writes("landscape normals"),
                            );
                        } else {
                            let mut pass = reshade::reshade_pass(
                                cx,
                                encoder,
                                rt,
                                &targets.depth,
                                "alpha re-shade",
                                timer.writes("alpha re-shade"),
                            );
                            cx.replay(plan.alpha.clone(), &mut pass, &mut chain);
                            drop(pass);
                            if plan.is_split() {
                                // The world steps indoors here: what follows is drawn with its
                                // own pipelines over the resolved picture, and the passes after
                                // it see the depth as it stood before the step.
                                self.reshader.resolve(
                                    cx,
                                    encoder,
                                    rt,
                                    &targets.colour,
                                    timer.writes("neutral resolve"),
                                );
                                resolved = true;
                                depth = match &unstamped {
                                    Some(u) => self.compositor.seen_depth(
                                        cx,
                                        &mut self.resources,
                                        encoder,
                                        &targets.depth,
                                        u,
                                        timer.writes("seen depth"),
                                    )?,
                                    None => Compositor::keep_depth(
                                        cx,
                                        &mut self.resources,
                                        encoder,
                                        &targets.depth,
                                    )?,
                                };
                                Compositor::replay_legacy(
                                    cx,
                                    encoder,
                                    &targets,
                                    plan.rest.clone(),
                                    false,
                                    timer.writes("indoor replay"),
                                    &mut LegacyReplay,
                                );
                                if let Some(step) = composite::indoor_step(cx.tables()) {
                                    world_depth = self.compositor.seen_after_step(
                                        cx,
                                        &mut self.resources,
                                        encoder,
                                        timer,
                                        &targets.depth,
                                        &depth,
                                        step,
                                    )?;
                                }
                            } else if let Some(u) = &unstamped {
                                // Again, with what the translucent draws wrote.
                                depth = self.compositor.seen_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    &targets.depth,
                                    u,
                                    timer.writes("seen depth"),
                                )?;
                                world_depth = depth.clone();
                            }
                        }
                    }
                    _ if slot == Slot::WorldReplay => {
                        let mut chain = Chain(filters);
                        if let Some(step) = composite::indoor_step(cx.tables()) {
                            // A frame that steps indoors is replayed in two, so the depth seen
                            // before the step is kept for what is seen through its rooms'
                            // openings.
                            Compositor::replay_legacy(
                                cx,
                                encoder,
                                &targets,
                                0..step.start,
                                true,
                                timer.writes("world replay"),
                                &mut chain,
                            );
                            let before = match last_stamp {
                                Some(last) => {
                                    let u = Compositor::unstamped_depth(
                                        cx,
                                        &mut self.resources,
                                        encoder,
                                        0..last + 1,
                                        timer.writes("unstamped depth"),
                                    )?;
                                    self.compositor.seen_depth(
                                        cx,
                                        &mut self.resources,
                                        encoder,
                                        &targets.depth,
                                        &u,
                                        timer.writes("seen depth"),
                                    )?
                                }
                                None => Compositor::keep_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    &targets.depth,
                                )?,
                            };
                            Compositor::replay_legacy(
                                cx,
                                encoder,
                                &targets,
                                step.clone(),
                                false,
                                timer.writes("indoor replay"),
                                &mut chain,
                            );
                            depth = self.compositor.seen_after_step(
                                cx,
                                &mut self.resources,
                                encoder,
                                timer,
                                &targets.depth,
                                &before,
                                step,
                            )?;
                            world_depth = depth.clone();
                        } else {
                            let writes = timer.writes("world replay");
                            Compositor::replay_world(cx, encoder, &targets, writes, &mut chain)?;
                            // A frame that steps indoors has cleared the outdoor stamps away
                            // with the depth, and its air is not drawn.
                            let split = ReshadePlan::new(cx.tables()).is_some_and(|p| p.is_split());
                            if let (Some(last), false) = (last_stamp, split) {
                                let u = Compositor::unstamped_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    0..last + 1,
                                    timer.writes("unstamped depth"),
                                )?;
                                depth = self.compositor.seen_depth(
                                    cx,
                                    &mut self.resources,
                                    encoder,
                                    &targets.depth,
                                    &u,
                                    timer.writes("seen depth"),
                                )?;
                                world_depth = depth.clone();
                            }
                        }
                    }
                    _ => {}
                }
            }
            for ((s, pass), wanted) in self.graph.passes_mut().zip(&self.wanted) {
                if s != slot || !*wanted {
                    continue;
                }
                let mut ecx = EncodeCx {
                    seam: cx,
                    settings: &self.settings,
                    frame,
                    caps: &self.caps,
                    resources: &mut self.resources,
                    encoder,
                    colour: &mut colour,
                    depth: &depth,
                    world_depth: &world_depth,
                    reshade: rt.as_ref(),
                    resolved: &mut resolved,
                    timer,
                };
                pass.encode(&mut ecx)?;
            }
            if slot == Slot::Hdr && !resolved {
                if let Some(rt) = &rt {
                    self.reshader.resolve(
                        cx,
                        encoder,
                        rt,
                        &colour,
                        timer.writes("neutral resolve"),
                    );
                }
                resolved = true;
            }
            if slot == Slot::Blit {
                let writes = timer.writes("composite");
                self.compositor.composite(
                    cx,
                    encoder,
                    &colour,
                    &world_depth,
                    target,
                    self.settings.debug,
                    (frame.camera.near, frame.camera.far),
                    writes,
                );
                let census = cx.census();
                self.reshader.debug_view(
                    cx,
                    encoder,
                    self.settings.debug,
                    &colour,
                    rt.as_ref(),
                    &census,
                    target,
                );
            }
        }
        timer.resolve(encoder);
        self.world_depth = Some(targets.depth);
        Ok(())
    }
}

impl FrameSidecar for HifiRenderer {
    fn prepare(&mut self, cx: &mut SidecarContext<'_>) -> Result<SidecarFrame, SidecarError> {
        if let Some(e) = self.latch.failed() {
            return Err(SidecarError(e.to_owned()));
        }
        self.caps = Caps::from_features(cx.device().features(), cx.compute());
        self.timer
            .get_or_insert_with(|| GpuTimer::new(cx.device(), cx.queue()))
            .begin_frame(cx.device());
        self.resources.begin_frame();
        self.wanted.clear();
        let Some(frame) = self.frame.as_ref() else {
            return Ok(SidecarFrame::Plain);
        };
        // A snapshot of another frame, or settings that change nothing: the ordinary frame.
        if frame.stamp != cx.frame_stamp() || !self.settings.any_effective() {
            return Ok(SidecarFrame::Plain);
        }
        let mut failure = None;
        for (_, pass) in self.graph.passes_mut() {
            pass.observe(&mut PrepareCx {
                seam: cx,
                settings: &self.settings,
                frame,
                caps: &self.caps,
                resources: &mut self.resources,
            });
            let wanted = failure.is_none() && pass.wanted(&self.settings, frame, &self.caps);
            if wanted {
                let mut pcx = PrepareCx {
                    seam: cx,
                    settings: &self.settings,
                    frame,
                    caps: &self.caps,
                    resources: &mut self.resources,
                };
                if let Err(e) = pass.prepare(&mut pcx) {
                    failure = Some(e);
                }
            }
            self.wanted.push(wanted);
        }
        if let Some(e) = failure {
            return Err(self.fail(&e));
        }
        self.plan = None;
        // No pass changes this frame (the weather's dry day, the sky indoors): the ordinary frame,
        // without the replay into the presentation's own targets.
        if !self.wanted.iter().any(|w| *w)
            && self.settings.debug == DebugView::Off
            && CompositeMode::for_settings(&self.settings) == CompositeMode::Overlay
        {
            return Ok(SidecarFrame::Plain);
        }
        self.readiness = reshade::Readiness::default();
        self.recorded_draws = (0, 0);
        if CompositeMode::for_settings(&self.settings) == CompositeMode::Reshade {
            if let Err(e) = self.prepare_reshade(cx) {
                return Err(self.fail(&e));
            }
        }
        for ((_, pass), wanted) in self.graph.passes_mut().zip(&self.wanted) {
            if *wanted {
                pass.planned(self.plan.as_ref());
            }
        }
        Ok(SidecarFrame::Composite)
    }

    fn encode(
        &mut self,
        cx: &mut SidecarContext<'_>,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) -> Result<(), SidecarError> {
        self.encode_frame(cx, encoder, target)
            .map_err(|e| self.fail(&e))
    }

    fn world_depth(&self) -> Option<&wgpu::TextureView> {
        self.world_depth.as_ref()
    }

    fn submitted(&mut self) {
        if let Some(t) = self.timer.as_mut() {
            t.submitted();
        }
    }

    fn report(&self, report: &mut SidecarReport) {
        report.passes = std::iter::once("world replay")
            .chain(
                self.graph
                    .order()
                    .into_iter()
                    .zip(&self.wanted)
                    .filter(|(_, w)| **w)
                    .map(|((_, name), _)| name),
            )
            .chain(std::iter::once("composite"))
            .collect();
        if let Some(t) = &self.timer {
            report.gpu_ms = t.last().passes.iter().map(|p| (p.name, p.gpu_ms)).collect();
        }
        report.waiting = u32::try_from(self.pipelines.waiting()).unwrap_or(u32::MAX);
        report
            .notes
            .push(("re-shaded", u64::from(self.plan.is_some())));
        report.notes.push((
            "drawn as recorded before the re-shade",
            self.recorded_draws.0,
        ));
        report.notes.push((
            "drawn as recorded after an indoor step",
            self.recorded_draws.1,
        ));
        report
            .notes
            .push(("pipelines built", self.pipelines.ready_count() as u64));
        for (_, pass) in self.graph.passes() {
            pass.notes(&mut report.notes);
        }
    }

    fn settle(&mut self, timeout: std::time::Duration) -> bool {
        self.pipelines.settle(timeout)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
