//! The sun's shadows by rays traced toward it through the scene's acceleration structures, where
//! the device traces rays: they stand in for the cascades' look-up (pass 7) and write the same
//! target, so nothing downstream changes. Where the device does not trace rays, or the shadows are
//! off, the cascades draw them as ever.
//!
//! Its shader is `shaders/rt_shadow.slang`; its pipeline is made with it ([`RtShadowPass::new`]),
//! only where the device traces rays, and never after.

use dereth_gfx::{
    ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline, RasterPass,
    Recorder, Rect, Texture, Touch, Use,
};

use super::rt::RtPlan;
use super::{push_words, shader, Targets};
use crate::scene::FrameDelta;
use crate::shaders::Entry;

/// The format of the sun's share, as the cascades write it.
const VISIBILITY: Format = Format::R8Unorm;

/// How the ray-traced shadows are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtShadowSettings {
    /// Whether the shadows are traced, where the device traces rays and the shadows are drawn.
    pub on: bool,
    /// The rays each pixel casts toward the sun's disc.
    pub rays: u32,
}

impl Default for RtShadowSettings {
    fn default() -> Self {
        Self { on: false, rays: 4 }
    }
}

/// The ray-traced shadows' pass.
#[derive(Debug)]
pub struct RtShadowPass {
    settings: RtShadowSettings,
    /// Made only where the device traces rays.
    pipeline: Option<Pipeline>,
    /// Frames it drew the sun's share in.
    frames: u64,
}

/// One frame of the pass.
#[derive(Debug, Clone, Copy)]
pub struct RtShadowPlan {
    pipeline: Pipeline,
    rays: u32,
    strength: f32,
}

impl RtShadowPass {
    /// The pass on `gfx`, off: its pipeline made where the device traces rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let pipeline = if gfx.caps().ray_queries {
            Some(gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster traced sun visibility",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(Entry::RtShadowFsMain)),
                color: &[ColorTarget {
                    format: VISIBILITY,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })?)
        } else {
            None
        };
        Ok(Self {
            settings: RtShadowSettings::default(),
            pipeline,
            frames: 0,
        })
    }

    /// How the shadows are traced from the next frame on.
    pub fn set_settings(&mut self, settings: RtShadowSettings) {
        self.settings = settings;
    }

    /// How the shadows are traced.
    #[must_use]
    pub fn settings(&self) -> RtShadowSettings {
        self.settings
    }

    /// Whether it asks for the scene's acceleration structures: on, where the device traces rays.
    #[must_use]
    pub fn wants_rays(&self) -> bool {
        self.settings.on && self.pipeline.is_some()
    }

    /// Frames it drew the sun's share in.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The frame's plan: `None` when the cascades draw the sun's share, because the pass is off,
    /// the device does not trace rays, the shadows are not drawn (`shadows`) or the frame built
    /// no acceleration structures (`traced`).
    pub(crate) fn prepare(
        &mut self,
        frame: &FrameDelta,
        shadows: bool,
        traced: bool,
    ) -> Option<RtShadowPlan> {
        let pipeline = self
            .pipeline
            .filter(|_| self.settings.on && shadows && traced)?;
        let (_, strength) = super::shadow::sun_of(frame.light.sun)?;
        self.frames += 1;
        Some(RtShadowPlan {
            pipeline,
            rays: self.settings.rays.clamp(1, 64),
            strength,
        })
    }
}

impl RtShadowPlan {
    /// Trace the sun's share into `visibility`, the target the cascades would write. `frame` is
    /// the frame's constants' index, `depth` the depth the outdoors are placed by, `rt` the
    /// frame's acceleration structures and `pools` the scene's indices, vertices and materials
    /// and the sampler alpha tests read with, each by its index in the table.
    ///
    /// # Errors
    /// What the layer refuses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        depth: (Texture, u32),
        rt: &RtPlan,
        pools: [u32; 4],
        visibility: Texture,
    ) -> Result<(), dereth_gfx::Error> {
        let touches = [
            Touch::Texture(depth.0, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
        ];
        let [indices, vertices, materials, sampler] = pools;
        r.timestamp("sun visibility traced");
        r.raster(
            &RasterPass {
                name: "sun visibility traced",
                color: &[ColorAttachment {
                    texture: visibility,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: t.width,
                    height: t.height,
                }),
            },
            |d| {
                d.set_pipeline(self.pipeline)?;
                d.push(
                    0,
                    &push_words(&[
                        frame,
                        t.index[4],
                        depth.1,
                        t.index[1],
                        t.index[3],
                        rt.index,
                        rt.static_rows,
                        rt.object_rows,
                        indices,
                        vertices,
                        materials,
                        sampler,
                        self.rays,
                        self.strength.to_bits(),
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

impl super::Renderer {
    /// How the shadows are traced from the next frame on.
    pub fn set_rt_shadows(&mut self, settings: RtShadowSettings) {
        self.rt_shadow.set_settings(settings);
    }

    /// How the shadows are traced.
    #[must_use]
    pub fn rt_shadows(&self) -> RtShadowSettings {
        self.rt_shadow.settings()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Off by default, so the cascades draw the sun's share.
    #[test]
    fn traced_shadows_are_off_by_default() {
        assert!(!RtShadowSettings::default().on);
        assert!(RtShadowSettings::default().rays > 0);
    }
}
