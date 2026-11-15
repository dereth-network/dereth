//! The sun's shadows and the ambient occlusion by rays traced through the scene's acceleration
//! structures, where the device traces rays: they stand in for the cascades' look-up
//! ([`crate::sun`]) and the screen-space occlusion ([`crate::ao`]) and write the same targets, so
//! nothing downstream changes. Where the device does not trace rays, or the frame has no
//! structures, the cascades and the screen-space occlusion draw them as ever.
//!
//! The structures are the game's: it builds them from its own scene and hands each frame's to the
//! passes as a [`TracedScene`]. The shaders (`shaders/rt_shadow.slang`, `shaders/rt_ao.slang`)
//! import a module named `rt` that the game writes beside its own shaders: the masks a ray asks
//! for and whether a query keeps a candidate hit on a surface it is handed (an alpha-tested or a
//! swaying one), read from the game's tables and pools. So each game compiles them in its own
//! shader folder, and hands its blobs to [`RtShadowPass::new`] and [`RtAoPass::new`]; their
//! pipelines are made only where the device traces rays, and never after.

use dereth_gfx::{
    ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline, RasterPass,
    Recorder, Rect, Shader, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::Vec3;

use crate::ao::AoSettings;
use crate::frame::push_words;
use crate::gbuffer::Targets;

/// The format of the sun's share, as the cascades write it.
const VISIBILITY: Format = Format::R8Unorm;

/// The rays' own occlusion, before it is evened out, and the occlusion the game's light reads.
const RAW: Format = Format::R16Float;
const FULL: Format = Format::R32Float;

/// A frame's acceleration structures, as the traced passes hand them to the game's `rt` module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TracedScene {
    /// The top-level structure's index in the table.
    pub accel: u32,
    /// The game's two tables of its instances' rows, each by its index in the table.
    pub tables: [u32; 2],
    /// The game's four pools its rows name (its indices, vertices and materials, and the sampler
    /// alpha tests read with, say), each by its index in the table.
    pub pools: [u32; 4],
}

impl TracedScene {
    /// The words every traced pass pushes after its G-buffer's: the structure, the tables and the
    /// pools.
    fn words(&self) -> [u32; 7] {
        let [a, b] = self.tables;
        let [c, d, e, f] = self.pools;
        [self.accel, a, b, c, d, e, f]
    }
}

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
    /// The pass on `gfx`, off: its pipeline made with the game's `fragment` (the game's own
    /// compilation of `rt_shadow.slang`'s `fs_main`) where the device traces rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device, fragment: Shader<'_>) -> Result<Self, dereth_gfx::Error> {
        let pipeline = if gfx.caps().ray_queries {
            Some(gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster traced sun visibility",
                vertex: crate::shaders::fullscreen(),
                fragment: Some(fragment),
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

    /// The frame's plan, toward the sun `sun` (the frame's direction toward it): `None` when the
    /// cascades draw the sun's share, because the pass is off, the device does not trace rays,
    /// the shadows are not drawn (`shadows`), the frame built no acceleration structures
    /// (`traced`) or there is no sun to cast them.
    pub fn prepare(&mut self, sun: Vec3, shadows: bool, traced: bool) -> Option<RtShadowPlan> {
        let pipeline = self
            .pipeline
            .filter(|_| self.settings.on && shadows && traced)?;
        let (_, strength) = crate::sun::sun_of(sun)?;
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
    /// the frame's constants' index, `depth` the depth the outdoors are placed by and `scene` the
    /// frame's acceleration structures.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        depth: (Texture, u32),
        scene: &TracedScene,
        visibility: Texture,
    ) -> Result<(), dereth_gfx::Error> {
        let touches = [
            Touch::Texture(depth.0, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
        ];
        let mut words = vec![frame, t.index[4], depth.1, t.index[1], t.index[3]];
        words.extend(scene.words());
        words.extend([
            self.rays,
            self.strength.to_bits(),
            crate::sun::SPLITS[0].to_bits(),
        ]);
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
                d.push(0, &push_words(&words))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

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
    /// The pass on `gfx`, off: its pipelines made with the game's `rays` and `filter` (its own
    /// compilation of `rt_ao.slang`'s `fs_rays` and `fs_filter`) where the device traces rays.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(
        gfx: &mut Device,
        rays: Shader<'_>,
        filter: Shader<'_>,
    ) -> Result<Self, dereth_gfx::Error> {
        let pipelines = if gfx.caps().ray_queries {
            let mut make = |label, fragment, format| {
                gfx.create_graphics_pipeline(&GraphicsDesc {
                    label,
                    vertex: crate::shaders::fullscreen(),
                    fragment: Some(fragment),
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
                make("remaster traced occlusion", rays, RAW)?,
                make("remaster traced occlusion evened", filter, FULL)?,
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
    pub fn prepare(
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
    /// outdoors are placed by and `scene` the frame's acceleration structures.
    ///
    /// # Errors
    /// What the layer refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
        scene: &TracedScene,
        ao: Texture,
    ) -> Result<(), dereth_gfx::Error> {
        let push = |raw: u32| {
            let mut words = vec![frame, t.index[4], outdoor_depth.1, t.index[1], t.index[3]];
            words.extend(scene.words());
            words.extend([
                self.rays,
                self.ao.radius.max(0.01).to_bits(),
                self.ao.falloff.clamp(0.0, 0.99).to_bits(),
                self.ao.strength.max(0.0).to_bits(),
                u32::from(self.ao.indoors),
                raw,
            ]);
            push_words(&words)
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

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// Both are off by default, so the cascades and the screen-space occlusion draw them.
    #[test]
    fn traced_shadows_and_occlusion_are_off_by_default() {
        assert!(!RtShadowSettings::default().on);
        assert!(RtShadowSettings::default().rays > 0);
        assert!(!RtAoSettings::default().on);
        assert!(RtAoSettings::default().rays > 0);
    }

    /// The scene's words follow the G-buffer's in the order the shaders read them: the structure,
    /// the two tables, then the four pools.
    #[test]
    fn a_traced_scene_pushes_its_structure_tables_and_pools_in_order() {
        let s = TracedScene {
            accel: 1,
            tables: [2, 3],
            pools: [4, 5, 6, 7],
        };
        assert_eq!(s.words(), [1, 2, 3, 4, 5, 6, 7]);
    }
}
