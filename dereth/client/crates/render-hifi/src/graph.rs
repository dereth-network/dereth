//! The fixed pass order, and what a pass sees when it runs.
//!
//! Every pass has one slot. The order of the slots never changes, and a pass never moves
//! another: a pass that is not wanted this frame is skipped where it stands.

use std::fmt;

use crate::resources::Resources;
use crate::timing::GpuTimer;
use crate::{
    Caps, DrawAction, DrawNote, HifiError, HifiFrame, HifiSettings, Mark, ReplayFilter,
    SidecarContext,
};

/// Where a pass runs in the frame, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// Compute work before anything is drawn: the sky tables.
    Compute,
    /// The world replay, with the sky drawn before it and the passes that act on single draws.
    WorldReplay,
    /// Screen-space work over the opaque world: the sun and sky light with its shadows,
    /// occlusion and bounced light.
    Opaque,
    /// The translucent draws, replayed after the opaque work when the world is re-shaded.
    AlphaReplay,
    /// The air: aerial perspective.
    Atmosphere,
    /// High dynamic range: bloom, exposure, tone mapping and grading.
    Hdr,
    /// The composite into the world viewport.
    Blit,
}

impl Slot {
    /// Every slot, in the order they run.
    pub const ORDER: [Slot; 7] = [
        Slot::Compute,
        Slot::WorldReplay,
        Slot::Opaque,
        Slot::AlphaReplay,
        Slot::Atmosphere,
        Slot::Hdr,
        Slot::Blit,
    ];
}

/// What a pass sees while it prepares a frame on the CPU.
#[derive(Debug)]
pub struct PrepareCx<'a, 's> {
    /// The frame's recording, and the constant blocks a pass may append to it.
    pub seam: &'a mut SidecarContext<'s>,
    /// The settings in force.
    pub settings: &'a HifiSettings,
    /// The frame's snapshot.
    pub frame: &'a HifiFrame,
    /// What the device can do.
    pub caps: &'a Caps,
    /// The shared resources.
    pub resources: &'a mut Resources,
}

/// What a pass sees while it records its GPU work.
#[derive(Debug)]
pub struct EncodeCx<'a, 's> {
    /// The frame's recording.
    pub seam: &'a mut SidecarContext<'s>,
    /// The settings in force.
    pub settings: &'a HifiSettings,
    /// The frame's snapshot.
    pub frame: &'a HifiFrame,
    /// What the device can do.
    pub caps: &'a Caps,
    /// The shared resources.
    pub resources: &'a mut Resources,
    /// The encoder the frame is recorded into.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The picture so far, in the frame target's format. A pass that writes a new picture
    /// points this at it.
    pub colour: &'a mut wgpu::TextureView,
    /// The world's depth, as the world replay left it; in a frame re-shaded only up to an indoor
    /// step, the depth as it stood before that step's clear.
    pub depth: &'a wgpu::TextureView,
    /// The world's depth as the whole frame left it: after an indoor step, what the step drew,
    /// over a cleared depth. The same as `depth` in a frame without one.
    pub world_depth: &'a wgpu::TextureView,
    /// The re-shaded world's linear light and normals, in a re-shaded frame.
    pub reshade: Option<&'a crate::reshade::ReshadeTargets>,
    /// Whether the re-shaded light is already written into the picture. A pass that writes its
    /// own picture from the linear light (a tone map) sets it, and the neutral resolve is then
    /// not drawn.
    pub resolved: &'a mut bool,
    /// The timestamp pool: a pass asks it for the writes of each render pass it begins.
    pub timer: &'a mut GpuTimer,
}

/// One pass of the presentation.
pub trait HifiPass {
    /// The pass's name, as the capture and timing tools spell it.
    fn name(&self) -> &'static str;

    /// Whether the pass runs this frame. False whenever its option is off, the device lacks
    /// what it needs, or its resources are not ready yet.
    fn wanted(&self, s: &HifiSettings, f: &HifiFrame, caps: &Caps) -> bool;

    /// The CPU half: uploads and constant blocks.
    ///
    /// # Errors
    /// Any failure ends the presentation and the frame is drawn the ordinary way.
    fn prepare(&mut self, cx: &mut PrepareCx<'_, '_>) -> Result<(), HifiError>;

    /// The pass's say in the world replay, if it has one.
    fn replay_filter(&mut self) -> Option<&mut dyn ReplayFilter> {
        None
    }

    /// How the frame is cut up for the re-shade, once that is planned: `None` when the frame is
    /// not re-shaded (its world is drawn as recorded).
    fn planned(&mut self, _plan: Option<&crate::reshade::ReshadePlan>) {}

    /// The GPU half, recorded at the pass's slot.
    ///
    /// # Errors
    /// As for [`HifiPass::prepare`].
    fn encode(&mut self, cx: &mut EncodeCx<'_, '_>) -> Result<(), HifiError>;
}

/// The passes, each in its slot.
pub struct Graph {
    passes: Vec<(Slot, Box<dyn HifiPass>)>,
}

impl fmt::Debug for Graph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.passes.iter().map(|(slot, p)| (slot, p.name())))
            .finish()
    }
}

impl Graph {
    /// Every pass this crate has, in slot order.
    #[must_use]
    pub fn standard() -> Self {
        let mut passes = crate::passes::all();
        // Stable: passes sharing a slot keep the order `all` lists them in.
        passes.sort_by_key(|(slot, _)| *slot);
        Self { passes }
    }

    /// Every pass's slot and name, in the order they run.
    #[must_use]
    pub fn order(&self) -> Vec<(Slot, &'static str)> {
        self.passes.iter().map(|(s, p)| (*s, p.name())).collect()
    }

    /// The names of the passes wanted this frame, in the order they run.
    #[must_use]
    pub fn wanted(&self, s: &HifiSettings, f: &HifiFrame, caps: &Caps) -> Vec<&'static str> {
        self.passes
            .iter()
            .filter(|(_, p)| p.wanted(s, f, caps))
            .map(|(_, p)| p.name())
            .collect()
    }

    /// Every pass with its slot, in the order they run.
    pub fn passes_mut(&mut self) -> impl Iterator<Item = (Slot, &mut (dyn HifiPass + 'static))> {
        self.passes.iter_mut().map(|(s, p)| (*s, p.as_mut()))
    }
}

/// Several passes' say in the world replay, as one: each draw goes to the first filter that
/// does something other than replay it as recorded, and every mark goes to every filter.
///
/// A filter may be asked about the same draw twice, and answers the same both times.
pub struct Chain<'f>(pub Vec<&'f mut dyn ReplayFilter>);

impl fmt::Debug for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Chain({} filters)", self.0.len())
    }
}

impl ReplayFilter for Chain<'_> {
    fn draw(&mut self, cmd: u32, note: Option<&DrawNote>) -> DrawAction<'_> {
        let chosen = self
            .0
            .iter_mut()
            .position(|f| !matches!(f.draw(cmd, note), DrawAction::Legacy));
        match chosen {
            Some(i) => self.0[i].draw(cmd, note),
            None => DrawAction::Legacy,
        }
    }

    fn mark(&mut self, mark: Mark, pass: &mut wgpu::RenderPass<'_>) {
        for f in &mut self.0 {
            f.mark(mark, pass);
        }
    }

    fn frame(&mut self, recorded: u32) -> Option<u32> {
        self.0.iter_mut().find_map(|f| f.frame(recorded))
    }
}
