//! Ambient occlusion by rays traced over each pixel's hemisphere through the scene's acceleration
//! structures, where the device traces rays: it stands in for the screen-space occlusion (pass 5)
//! and writes the same target, at the radius, falloff and strength the occlusion's own settings
//! give, so nothing downstream changes. Where the device does not trace rays, or the occlusion
//! is off, the screen-space occlusion draws it as ever.
//!
//! Its shader is `shaders/rt_ao.slang`; its pipelines are made with it ([`RtAoPass::new`]), only
//! where the device traces rays, and never after.

use dereth_gfx::{
    ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline, RasterPass,
    Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};

use super::ao::AoSettings;
use super::rt::RtPlan;
use super::{push_words, shader, Targets};
use crate::shaders::Entry;

/// The rays' own occlusion, before it is evened out, and the occlusion the resolve reads.
const RAW: Format = Format::R16Float;
const FULL: Format = Format::R32Float;

/// How the ray-traced occlusion is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtAoSettings {
    /// Whether the occlusion is traced, where the device traces rays and the occlusion is drawn.
    pub on: bool,
    /// The rays each pixel casts.
    pub rays: u32,
}

impl Default for RtAoSettings {
    fn default() -> Self {
        Self { on: false, rays: 8 }
    }
}

#[derive(Debug, Clone, Copy)]
struct Raw {
    texture: Texture,
    index: u32,
    width: u32,
    height: u32,
}

/// The ray-traced occlusion's pass.
#[derive(Debug)]
pub struct RtAoPass {
    settings: RtAoSettings,
    /// The rays, then the evening out: made only where the device traces rays.
    pipelines: Option<[Pipeline; 2]>,
    raw: Option<Raw>,
    /// Frames it drew the occlusion in.
    frames: u64,
}

/// One frame of the pass.
#[derive(Debug, Clone, Copy)]
pub struct RtAoPlan {
    pipelines: [Pipeline; 2],
    raw: Raw,
    rays: u32,
    ao: AoSettings,
}

impl RtAoPass {
    /// The pass on `gfx`, off: its pipelines made where the device traces rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let pipelines = if gfx.caps().ray_queries {
            let mut make = |label, e, format| {
                gfx.create_graphics_pipeline(&GraphicsDesc {
                    label,
                    vertex: shader(Entry::FullscreenVsMain),
                    fragment: Some(shader(e)),
                    color: &[ColorTarget {
                        format,
                        blend: None,
                        write_alpha: true,
                    }],
                    depth: None,
                    cull: Cull::None,
                })
            };
            Some([
                make("remaster traced occlusion", Entry::RtAoFsRays, RAW)?,
                make(
                    "remaster traced occlusion evened",
                    Entry::RtAoFsFilter,
                    FULL,
                )?,
            ])
        } else {
            None
        };
        Ok(Self {
            settings: RtAoSettings::default(),
            pipelines,
            raw: None,
            frames: 0,
        })
    }

    /// How the occlusion is traced from the next frame on.
    pub fn set_settings(&mut self, settings: RtAoSettings) {
        self.settings = settings;
    }

    /// How the occlusion is traced.
    #[must_use]
    pub fn settings(&self) -> RtAoSettings {
        self.settings
    }

    /// Whether it asks for the scene's acceleration structures: on, where the device traces rays.
    #[must_use]
    pub fn wants_rays(&self) -> bool {
        self.settings.on && self.pipelines.is_some()
    }

    /// Frames it drew the occlusion in.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The frame's plan, its target made at the world's size: `None` when the screen-space
    /// occlusion draws it, because the pass is off, the device does not trace rays, the
    /// occlusion is not drawn (`ao`, its settings when it is) or the frame built no acceleration
    /// structures (`traced`).
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut Device,
        t: &Targets,
        ao: Option<AoSettings>,
        traced: bool,
    ) -> Result<Option<RtAoPlan>, dereth_gfx::Error> {
        let (Some(pipelines), Some(ao)) = (self.pipelines, ao) else {
            return Ok(None);
        };
        if !self.settings.on || !traced {
            return Ok(None);
        }
        let raw = match self.raw {
            Some(r) if (r.width, r.height) == (t.width, t.height) => r,
            old => {
                if let Some(r) = old {
                    gfx.destroy_texture(r.texture);
                }
                let texture = gfx.create_texture(&TextureDesc {
                    label: "remaster traced occlusion",
                    width: t.width,
                    height: t.height,
                    mips: 1,
                    format: RAW,
                    usage: TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED,
                })?;
                let r = Raw {
                    texture,
                    index: gfx.bind_texture(texture)?,
                    width: t.width,
                    height: t.height,
                };
                self.raw = Some(r);
                r
            }
        };
        self.frames += 1;
        Ok(Some(RtAoPlan {
            pipelines,
            raw,
            rays: self.settings.rays.clamp(1, 64),
            ao,
        }))
    }
}

impl RtAoPlan {
    /// Trace the occlusion, then even it out into `ao`, the target the screen-space occlusion
    /// would write. `frame` is the frame's constants' index, `outdoor_depth` the depth the
    /// outdoors are placed by, `rt` the frame's acceleration structures and `pools` the scene's
    /// indices, vertices and materials and the sampler alpha tests read with, each by its index in
    /// the table.
    ///
    /// # Errors
    /// What the layer refuses.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
        rt: &RtPlan,
        pools: [u32; 4],
        ao: Texture,
    ) -> Result<(), dereth_gfx::Error> {
        let [indices, vertices, materials, sampler] = pools;
        let push = |raw: u32| {
            push_words(&[
                frame,
                t.index[4],
                outdoor_depth.1,
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
                self.ao.radius.max(0.01).to_bits(),
                self.ao.falloff.clamp(0.0, 0.99).to_bits(),
                self.ao.strength.max(0.0).to_bits(),
                u32::from(self.ao.indoors),
                raw,
            ])
        };
        let full = Rect {
            x: 0,
            y: 0,
            width: t.width,
            height: t.height,
        };
        let gbuffer = [
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
        ];
        r.timestamp("occlusion traced");
        r.raster(
            &RasterPass {
                name: "occlusion traced",
                color: &[ColorAttachment {
                    texture: self.raw.texture,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &gbuffer,
                area: Some(full),
            },
            |d| {
                d.set_pipeline(self.pipelines[0])?;
                d.push(0, &push(0))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        let mut touches = gbuffer.to_vec();
        touches.push(Touch::Texture(self.raw.texture, Use::Sampled));
        r.raster(
            &RasterPass {
                name: "occlusion traced evened",
                color: &[ColorAttachment {
                    texture: ao,
                    load: Load::Discard,
                }],
                depth: None,
                touches: &touches,
                area: Some(full),
            },
            |d| {
                d.set_pipeline(self.pipelines[1])?;
                d.push(0, &push(self.raw.index))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

impl super::Renderer {
    /// How the occlusion is traced from the next frame on.
    pub fn set_rt_ao(&mut self, settings: RtAoSettings) {
        self.rt_ao.set_settings(settings);
    }

    /// How the occlusion is traced.
    #[must_use]
    pub fn rt_ao(&self) -> RtAoSettings {
        self.rt_ao.settings()
    }

    /// Frames the shadows and the occlusion were traced in, since the renderer was made.
    #[must_use]
    pub fn traced_frames(&self) -> (u64, u64) {
        (self.rt_shadow.frames(), self.rt_ao.frames())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Off by default, so the screen-space occlusion draws it.
    #[test]
    fn traced_occlusion_is_off_by_default() {
        assert!(!RtAoSettings::default().on);
        assert!(RtAoSettings::default().rays > 0);
    }
}
