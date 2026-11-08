//! The ambient occlusion pass: how much of the sky each pixel's surroundings leave it, from the
//! G-buffer's depth, at half the world's resolution, gathered over the frames while the
//! view holds still, and brought up to the full resolution. The resolve takes the ambient light by
//! it ([`AoPass::mask`]); with the pass off, every pixel's occlusion is 1 and the frame is the one drawn
//! without it.
//!
//! Its shader is `shaders/ao.slang`; its pipelines are made with it ([`AoPass::new`]) and never
//! after.

use dereth_gfx::{
    BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline,
    RasterPass, Recorder, Rect, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::Mat4;

use super::light::Rows;
use super::{push_words, shader, PerFrame, Targets};
use crate::scene::Camera;
use crate::shaders::Entry;

/// How the occlusion is found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AoSettings {
    /// Whether the pass runs. Off, every pixel's occlusion is 1.
    pub on: bool,
    /// How far round a pixel its surroundings are searched, metres.
    pub radius: f32,
    /// Where the search's weight begins to fall off, as a share of the radius.
    pub falloff: f32,
    /// A power on the occlusion: above 1 darker, below 1 lighter.
    pub strength: f32,
    /// Whether rooms and what is in them take the occlusion. Off, only the outdoors does.
    pub indoors: bool,
    /// How many frames of differently turned samples a still view gathers before it settles.
    pub frames: u32,
    /// The slices of the hemisphere searched each frame, and the steps each way along each.
    pub slices: u32,
    pub steps: u32,
}

impl Default for AoSettings {
    fn default() -> Self {
        Self {
            on: false,
            radius: 1.5,
            falloff: 0.4,
            strength: 1.0,
            indoors: false,
            frames: 8,
            slices: 2,
            steps: 4,
        }
    }
}

/// The pass's targets, at the world's size.
#[derive(Debug, Clone, Copy)]
struct AoTargets {
    width: u32,
    height: u32,
    /// This frame's occlusion, half resolution: occlusion, distance from the eye.
    raw: Texture,
    /// What was gathered, half resolution, this frame's and last frame's by turns: occlusion,
    /// distance, frames gathered.
    history: [Texture; 2],
    /// The occlusion at full resolution, which the resolve reads.
    ao: Texture,
    /// Each one's index in the table: raw, history 0 and 1, ao.
    index: [u32; 4],
}

/// The occlusion pass.
#[derive(Debug)]
pub struct AoPass {
    gtao: Pipeline,
    gather: Pipeline,
    upsample: Pipeline,
    settings: AoSettings,
    constants: PerFrame,
    targets: Option<AoTargets>,
    /// Frames drawn, and the last one's view and projection together.
    frame: u32,
    last_view_proj: Option<Mat4>,
}

/// One frame of the pass, as [`AoPass::prepare`] made it.
#[derive(Debug, Clone, Copy)]
pub struct AoPlan {
    gtao: Pipeline,
    gather: Pipeline,
    upsample: Pipeline,
    targets: AoTargets,
    /// The history this frame writes, and the one it reads.
    write: usize,
    constants: u32,
}

/// The half-resolution targets' format, and the full one's.
const HALF: Format = Format::Rgba16Float;
const FULL: Format = Format::R32Float;

impl AoPass {
    /// The pass on `gfx`, its pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let pipe = |gfx: &mut Device, e: Entry, format: Format| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster ambient occlusion",
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
        Ok(Self {
            gtao: pipe(gfx, Entry::AoFsGtao, HALF)?,
            gather: pipe(gfx, Entry::AoFsGather, HALF)?,
            upsample: pipe(gfx, Entry::AoFsUpsample, FULL)?,
            settings: AoSettings::default(),
            constants: PerFrame::new(
                gfx,
                "remaster occlusion constants",
                128,
                BufferUsage::default(),
            )?,
            targets: None,
            frame: 0,
            last_view_proj: None,
        })
    }

    /// How the occlusion is found from the next frame on.
    pub fn set_settings(&mut self, settings: AoSettings) {
        if settings != self.settings {
            // Gathered under other settings, the history is another picture's.
            self.last_view_proj = None;
        }
        self.settings = settings;
    }

    /// How the occlusion is found.
    #[must_use]
    pub fn settings(&self) -> AoSettings {
        self.settings
    }

    /// The full-resolution occlusion's index in the table, for the resolve's masks; `None` when
    /// the pass is off, and the occlusion is 1 everywhere.
    #[must_use]
    pub fn mask(&self) -> Option<u32> {
        self.settings
            .on
            .then_some(self.targets)
            .flatten()
            .map(|t| t.index[3])
    }

    fn targets_for(
        &mut self,
        gfx: &mut Device,
        t: &Targets,
    ) -> Result<AoTargets, dereth_gfx::Error> {
        if let Some(a) = self.targets {
            if (a.width, a.height) == (t.width, t.height) {
                return Ok(a);
            }
            for x in [a.raw, a.history[0], a.history[1], a.ao] {
                gfx.destroy_texture(x);
            }
        }
        let (hw, hh) = (t.width.div_ceil(2), t.height.div_ceil(2));
        let usage = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC;
        let mut make = |width, height, format| {
            gfx.create_texture(&TextureDesc {
                label: "remaster occlusion",
                width,
                height,
                mips: 1,
                format,
                usage,
            })
        };
        let raw = make(hw, hh, HALF)?;
        let history = [make(hw, hh, HALF)?, make(hw, hh, HALF)?];
        let ao = make(t.width, t.height, FULL)?;
        let index = [
            gfx.bind_texture(raw)?,
            gfx.bind_texture(history[0])?,
            gfx.bind_texture(history[1])?,
            gfx.bind_texture(ao)?,
        ];
        let a = AoTargets {
            width: t.width,
            height: t.height,
            raw,
            history,
            ao,
            index,
        };
        self.targets = Some(a);
        // New targets hold nothing gathered.
        self.last_view_proj = None;
        Ok(a)
    }

    /// The frame's constants, and the plan its passes are recorded from; `None` when the pass is
    /// off. `camera` is the frame's camera before the anti-aliasing moved its projection: the
    /// view holds still when it does.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        camera: &Camera,
        t: &Targets,
    ) -> Result<Option<AoPlan>, dereth_gfx::Error> {
        if !self.settings.on {
            self.last_view_proj = None;
            return Ok(None);
        }
        let targets = self.targets_for(gfx, t)?;
        let view_proj = camera.projection * camera.view;
        // Still: the view the same as last frame's, to the bit, and something gathered.
        let still = self.last_view_proj == Some(view_proj);
        // Nothing gathered yet: the history holds no picture.
        let fresh = self.last_view_proj.is_none();
        let previous = camera
            .previous_view_projection
            .filter(|_| self.last_view_proj.is_some())
            .unwrap_or(view_proj);
        let s = self.settings;
        let frames = s.frames.max(1);
        let mut rows = Rows::default();
        for c in previous.to_cols_array_2d() {
            rows.0.push(c.map(f32::to_bits));
        }
        #[allow(clippy::cast_precision_loss)] // small counts
        {
            rows.0.push(
                [
                    s.radius.max(0.01),
                    s.falloff.clamp(0.0, 0.99),
                    s.strength.max(0.0),
                    (self.frame % frames) as f32,
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    targets.width.div_ceil(2) as f32,
                    targets.height.div_ceil(2) as f32,
                    if still { 1.0 } else { 0.0 },
                    if s.indoors { 1.0 } else { 0.0 },
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    frames as f32,
                    s.slices.clamp(1, 8) as f32,
                    s.steps.clamp(1, 16) as f32,
                    if fresh { 1.0 } else { 0.0 },
                ]
                .map(f32::to_bits),
            );
        }
        self.constants.write(gfx, &rows.bytes())?;
        // A still view holds the sequence where it is once it is gathered: the shader keeps
        // what it has, so the frame's place in it no longer matters.
        self.frame = self.frame.wrapping_add(1);
        self.last_view_proj = Some(view_proj);
        let write = (self.frame % 2) as usize;
        Ok(Some(AoPlan {
            gtao: self.gtao,
            gather: self.gather,
            upsample: self.upsample,
            targets,
            write,
            constants: self.constants.index,
        }))
    }
}

impl AoPlan {
    /// Record the pass: this frame's occlusion, gathered, then brought up to full resolution.
    /// `frame` is the frame's constants' index; `outdoor_depth` the depth the outdoors are placed
    /// by, as the resolve has it.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
    ) -> Result<(), dereth_gfx::Error> {
        let a = self.targets;
        let read = 1 - self.write;
        let half = Rect {
            x: 0,
            y: 0,
            width: a.width.div_ceil(2),
            height: a.height.div_ceil(2),
        };
        let push = |raw: u32, history: u32| {
            push_words(&[
                frame,
                self.constants,
                t.index[4],
                t.index[3],
                outdoor_depth.1,
                raw,
                history,
            ])
        };
        let gbuffer = [
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
        ];
        r.timestamp("occlusion");
        r.raster(
            &RasterPass {
                name: "occlusion",
                color: &[ColorAttachment {
                    texture: a.raw,
                    load: Load::Clear([1.0, 0.0, 0.0, 0.0]),
                }],
                depth: None,
                touches: &gbuffer,
                area: Some(half),
            },
            |d| {
                d.set_pipeline(self.gtao)?;
                d.push(0, &push(0, 0))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        let mut touches = gbuffer.to_vec();
        touches.push(Touch::Texture(a.raw, Use::Sampled));
        touches.push(Touch::Texture(a.history[read], Use::Sampled));
        r.raster(
            &RasterPass {
                name: "occlusion gathered",
                color: &[ColorAttachment {
                    texture: a.history[self.write],
                    load: Load::Clear([1.0, 0.0, 0.0, 0.0]),
                }],
                depth: None,
                touches: &touches,
                area: Some(half),
            },
            |d| {
                d.set_pipeline(self.gather)?;
                d.push(0, &push(a.index[0], a.index[1 + read]))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        let mut touches = gbuffer.to_vec();
        touches.push(Touch::Texture(a.history[self.write], Use::Sampled));
        r.raster(
            &RasterPass {
                name: "occlusion at full size",
                color: &[ColorAttachment {
                    texture: a.ao,
                    load: Load::Clear([1.0, 0.0, 0.0, 0.0]),
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: a.width,
                    height: a.height,
                }),
            },
            |d| {
                d.set_pipeline(self.upsample)?;
                d.push(0, &push(0, a.index[1 + self.write]))?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        Ok(())
    }

    /// The full-resolution occlusion this plan writes, by its index in the table and itself: the
    /// resolve's mask of how much of the ambient reaches each pixel.
    #[must_use]
    pub fn mask(&self) -> (u32, Texture) {
        (self.targets.index[3], self.targets.ao)
    }
}

impl super::Renderer {
    /// How the ambient occlusion is found from the next frame on.
    pub fn set_ao(&mut self, settings: AoSettings) {
        self.ao.set_settings(settings);
    }

    /// How the ambient occlusion is found.
    #[must_use]
    pub fn ao_settings(&self) -> AoSettings {
        self.ao.settings()
    }

    /// The last frame's ambient occlusion at the world's size, row by row, 1 open and 0 shut in;
    /// `None` while the pass is off or before it has run. The device waits for its work first.
    pub fn read_ao(&mut self) -> Option<(u32, u32, Vec<f32>)> {
        let a = self.ao.settings.on.then_some(self.ao.targets).flatten()?;
        self.gfx.wait_idle().ok()?;
        let bytes = self.gfx.read_texture(a.ao, 0).ok()?;
        Some((
            a.width,
            a.height,
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|w| f32::from_le_bytes(*w))
                .collect(),
        ))
    }
}
