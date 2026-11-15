//! The post passes' shared part: the scene's target of its own, temporal anti-aliasing, the
//! bloom, the exposure and a tone curve into the world's colors. What draws the scene into the
//! target, and what writes the moving objects' motion, is the game's.
//!
//! - **The scene's target** ([`PostPlan::scene_targets`]): with the passes on, the game draws its
//!   world into it rather than into the world's colors; with [`PostSettings::hdr`] its colors are
//!   linear light and unclamped, otherwise they are display values.
//! - **Temporal anti-aliasing.** Each frame's projection is moved by a different part of a pixel
//!   ([`PostPass::jitter`]); `fs_taa` lays the frames over each other where each pixel's surface
//!   was the frame before: by the motion the game wrote where something moved
//!   ([`PostPlan::record_taa`]), by the depth and the last frame's view elsewhere.
//! - **The bloom, the exposure and the tone map**, when the colors are linear
//!   ([`PostPlan::record_finish`]).
//! - **With an upscaler on** ([`PostPass::set_upscaling`]) it stands in for the anti-aliasing:
//!   the scene and the motion stay at the render size, and the bloom and the tone map run at the
//!   display's size into a target of their own.
//!
//! The shader is `shaders/post.slang`; every pipeline is made with the pass ([`PostPass::new`]) and
//! none after.

use dereth_gfx::{
    Blend, BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Factor, Format, GraphicsDesc,
    Load, Pipeline, RasterPass, Recorder, Rect, Sampler, SamplerDesc, Texture, TextureDesc,
    TextureUsage, Touch, Use,
};
use glam::Mat4;

use crate::frame::{push_words, reprojection, Camera, PerFrame, Rows};
use crate::gbuffer::{Targets, COLOR};
use crate::shaders::{shader, Entry};

/// A buffer or picture index meaning none.
const NONE: u32 = u32::MAX;

/// How the post passes draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostSettings {
    /// Whether the scene is drawn in linear light, unclamped, then exposed, bloomed and tone
    /// mapped to the display. Off, its colors are the game's display values.
    pub hdr: bool,
    /// Whether the frames are laid over each other to smooth the edges (temporal
    /// anti-aliasing).
    pub taa: bool,
    /// The bloom's share of the frame, 0 for none. Only with [`Self::hdr`].
    pub bloom: f32,
    /// Stops of exposure, added to the look's. The tone curve leaves the game's own colors as they
    /// are, so the default is none.
    pub exposure: f32,
    /// The frames of the anti-aliasing's sequence of moved projections.
    pub frames: u32,
    /// To measure the motion, with [`Self::taa`]: the frame shown is the last frame's, found
    /// where each pixel's surface was. Its target is made only while checking.
    pub check: bool,
}

impl Default for PostSettings {
    fn default() -> Self {
        Self {
            hdr: false,
            taa: false,
            bloom: 0.04,
            exposure: 0.0,
            frames: 8,
            check: false,
        }
    }
}

impl PostSettings {
    /// Whether the passes run: the scene is then drawn into their own target.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.hdr || self.taa
    }
}

/// The history's format, and the picture the check shows: linear light with an alpha (the frames
/// gathered, or where last frame's surface was found); the marks'; the motion's.
pub const LINEAR: Format = Format::Rgba16Float;
const MARKS: Format = Format::Rgba8Unorm;
pub const MOTION: Format = Format::Rg16Snorm;

/// The scene's and the bloom's format on `gfx`: packed floats where it draws into them, as they
/// need no alpha, else [`LINEAR`].
#[must_use]
pub fn scene_format(gfx: &Device) -> Format {
    if gfx.caps().packed_float_targets {
        Format::Rg11b10Ufloat
    } else {
        LINEAR
    }
}

/// The bytes a pixel of each format takes.
const LINEAR_BYTES: u64 = 8;
const MARKS_BYTES: u64 = 4;
const MOTION_BYTES: u64 = 4;

/// The bloom's levels: half the frame's size, a quarter, and so on.
pub const BLOOM_LEVELS: usize = 6;

/// The bloom's levels' sizes for a world of `width` by `height`.
fn bloom_sizes(width: u32, height: u32) -> [(u32, u32); BLOOM_LEVELS] {
    let mut sizes = [(1, 1); BLOOM_LEVELS];
    let (mut w, mut h) = (width, height);
    for size in &mut sizes {
        w = w.div_ceil(2).max(1);
        h = h.div_ceil(2).max(1);
        *size = (w, h);
    }
    sizes
}

/// What the targets are made for: the anti-aliasing, the check of the motion, the moving parts'
/// motion, the bloom, and the display's size where an upscaler brings the frame up to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Uses {
    taa: bool,
    check: bool,
    motion: bool,
    bloom: bool,
    display: Option<(u32, u32)>,
}

impl Uses {
    fn of(s: PostSettings) -> Self {
        Self {
            taa: s.taa,
            check: s.taa && s.check,
            motion: s.taa,
            bloom: s.hdr && s.bloom > 0.0,
            display: None,
        }
    }

    /// With an upscaler on, for a display of `display`: it takes the anti-aliasing's place.
    fn upscaled(s: PostSettings, display: (u32, u32)) -> Self {
        Self {
            taa: false,
            check: false,
            motion: true,
            bloom: s.hdr && s.bloom > 0.0,
            display: Some(display),
        }
    }
}

/// The bytes the passes' targets take for a world of `width` by `height` drawn as `settings`
/// say, the scene's colors and the bloom in `scene`: the scene's colors whenever they run; with
/// the anti-aliasing, the motion, two histories and two sets of marks (and, checking the motion,
/// the picture it shows); with the bloom, its levels.
#[must_use]
pub fn target_bytes(width: u32, height: u32, settings: PostSettings, scene: Format) -> u64 {
    if !settings.runs() {
        return 0;
    }
    let uses = Uses::of(settings);
    let px = u64::from(width) * u64::from(height);
    let scene_bytes = scene.level_bytes(1, 1);
    let mut bytes = px * scene_bytes;
    if uses.taa {
        bytes += px * (MOTION_BYTES + 2 * LINEAR_BYTES + 2 * MARKS_BYTES);
    }
    if uses.check {
        bytes += px * LINEAR_BYTES;
    }
    if uses.bloom {
        bytes += bloom_sizes(width, height)
            .iter()
            .map(|(w, h)| u64::from(*w) * u64::from(*h) * scene_bytes)
            .sum::<u64>();
    }
    bytes
}

/// A target and its index in the table.
#[derive(Debug, Clone, Copy)]
pub struct Bound {
    pub texture: Texture,
    pub index: u32,
}

/// What the anti-aliasing keeps, at the world's size.
#[derive(Debug, Clone, Copy)]
pub struct TaaTargets {
    /// The colors gathered (rgb) and how many frames (a): this frame's and the last's by turns.
    /// What a frame gathers is what it shows.
    pub history: [Bound; 2],
    /// Each pixel's marks of the frames at the sequence's eight places, by turns as the history.
    pub marks: [Bound; 2],
    /// Checking the motion: last frame's picture found where each pixel's surface was.
    pub shown: Option<Bound>,
}

/// The picture the frames were gathered into, for the passes after it.
#[derive(Debug, Clone, Copy)]
pub struct Gathered(pub Bound);

/// The passes' targets, at the world's size.
#[derive(Debug, Clone, Copy)]
pub struct PostTargets {
    pub width: u32,
    pub height: u32,
    uses: Uses,
    /// The scene's colors.
    pub scene: Bound,
    /// The moving objects' last places in clip space, halved; -1 where nothing moved.
    pub motion: Option<Bound>,
    pub taa: Option<TaaTargets>,
    /// The bloom's levels, each with its size.
    pub bloom: Option<[(Bound, u32, u32); BLOOM_LEVELS]>,
    /// The frame at the display's size, where an upscaler brings it up to that.
    pub output: Option<Bound>,
}

impl PostTargets {
    fn textures(&self) -> Vec<Texture> {
        let mut v = vec![self.scene.texture];
        v.extend(self.motion.map(|b| b.texture));
        v.extend(self.output.map(|b| b.texture));
        if let Some(t) = &self.taa {
            v.extend([
                t.history[0].texture,
                t.history[1].texture,
                t.marks[0].texture,
                t.marks[1].texture,
            ]);
            v.extend(t.shown.map(|b| b.texture));
        }
        if let Some(b) = &self.bloom {
            v.extend(b.iter().map(|l| l.0.texture));
        }
        v
    }
}

/// How the frame is graded: its saturation, contrast and vignette, and the grade's tables when it
/// takes them (the three blended, the tables' picture's index, their weights and how many the
/// picture holds).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grade {
    pub look: [f32; 3],
    pub tables: Option<([u32; 3], u32, [f32; 3], f32)>,
}

/// The saturation, the contrast and the vignette of the frame with no grade.
pub const NO_GRADE: [f32; 3] = [1.0, 1.0, 0.0];

impl Default for Grade {
    fn default() -> Self {
        Self {
            look: NO_GRADE,
            tables: None,
        }
    }
}

/// The post passes.
#[derive(Debug)]
pub struct PostPass {
    scene_format: Format,
    taa: Pipeline,
    check: Pipeline,
    down: Pipeline,
    up: Pipeline,
    tonemap: Pipeline,
    sampler: (Sampler, u32),
    settings: PostSettings,
    /// The display's size, while an upscaler brings the frame up to it.
    display: Option<(u32, u32)>,
    constants: PerFrame,
    targets: Option<PostTargets>,
    /// Frames drawn, the last one's view and projection together (unmoved), and how far this
    /// frame's projection is moved, in clip space.
    frame: u32,
    last_view_proj: Option<Mat4>,
    jitter: [f32; 2],
    /// The history the last frame wrote.
    last_write: usize,
}

/// One frame of the passes, as [`PostPass::prepare`] made it.
#[derive(Debug, Clone, Copy)]
pub struct PostPlan {
    /// The anti-aliasing's pipeline, or its check of the motion's.
    taa: Option<Pipeline>,
    down: Pipeline,
    up: Pipeline,
    tonemap: Pipeline,
    sampler: u32,
    targets: PostTargets,
    /// The history and marks this frame writes; it reads the others.
    write: usize,
    constants: u32,
    /// The world's colors the tone map writes, which the composite reads.
    output: Bound,
    /// Whether there is no last frame to gather from, and whether the colors are linear: what an
    /// upscaler takes.
    fresh: bool,
    hdr: bool,
}

/// The `i`th number of the base-`b` Halton sequence, in [0, 1).
#[must_use]
pub fn halton(mut i: u32, b: u32) -> f32 {
    let mut f = 1.0f32;
    let mut r = 0.0f32;
    #[allow(clippy::cast_precision_loss)] // a small base
    let bf = b as f32;
    while i > 0 {
        f /= bf;
        #[allow(clippy::cast_precision_loss)] // a digit
        let digit = (i % b) as f32;
        r += f * digit;
        i /= b;
    }
    r
}

impl PostPass {
    /// The passes on `gfx`, their pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let full = |gfx: &mut Device, e: Entry, color: &[ColorTarget]| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster post",
                vertex: crate::shaders::fullscreen(),
                fragment: Some(shader(e)),
                color,
                depth: None,
                cull: Cull::None,
            })
        };
        let target = |format| ColorTarget {
            format,
            blend: None,
            write_alpha: true,
        };
        let taa = full(gfx, Entry::PostFsTaa, &[target(LINEAR), target(MARKS)])?;
        let check = full(
            gfx,
            Entry::PostFsTaaCheck,
            &[target(LINEAR), target(LINEAR)],
        )?;
        let scene_format = scene_format(gfx);
        let down = full(gfx, Entry::PostFsBloomDown, &[target(scene_format)])?;
        let up = full(
            gfx,
            Entry::PostFsBloomUp,
            &[ColorTarget {
                format: scene_format,
                blend: Some(Blend {
                    src: Factor::One,
                    dst: Factor::One,
                }),
                write_alpha: false,
            }],
        )?;
        let tonemap = full(gfx, Entry::PostFsTonemap, &[target(COLOR)])?;
        let sampler = gfx.create_sampler(&SamplerDesc {
            filter: dereth_gfx::Filter::Linear,
            mip: dereth_gfx::Filter::Nearest,
            address: dereth_gfx::Address::Clamp,
            address_v: dereth_gfx::Address::Clamp,
            anisotropy: 1,
            lod_bias: 0.0,
        })?;
        let sampler_index = gfx.bind_sampler(sampler)?;
        Ok(Self {
            scene_format,
            taa,
            check,
            down,
            up,
            tonemap,
            sampler: (sampler, sampler_index),
            settings: PostSettings::default(),
            display: None,
            constants: PerFrame::new(gfx, "remaster post constants", 160, BufferUsage::default())?,
            targets: None,
            frame: 0,
            last_view_proj: None,
            jitter: [0.0; 2],
            last_write: 0,
        })
    }

    /// The scene's and the bloom's format the passes were made for.
    #[must_use]
    pub fn scene_format(&self) -> Format {
        self.scene_format
    }

    /// How the passes draw from the next frame on.
    pub fn set_settings(&mut self, settings: PostSettings) {
        if settings != self.settings {
            // Gathered under other settings, the history is another picture's.
            self.last_view_proj = None;
        }
        self.settings = settings;
    }

    /// How the passes draw.
    #[must_use]
    pub fn settings(&self) -> PostSettings {
        self.settings
    }

    /// Whether the scene's colors are drawn linear and unclamped.
    #[must_use]
    pub fn linear(&self) -> bool {
        self.settings.hdr
    }

    /// While an upscaler brings the frame up to a display of `display`, or `None`, from the next
    /// frame on.
    pub fn set_upscaling(&mut self, display: Option<(u32, u32)>) {
        if display != self.display {
            self.last_view_proj = None;
        }
        self.display = display;
    }

    /// Whether the passes run: asked for, or because an upscaler takes the scene from them.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.settings.runs() || self.display.is_some()
    }

    /// What the upscaler's picture is left as: the target at the display's size, with its size,
    /// while one is on.
    #[must_use]
    pub fn upscaled_output(&self) -> Option<(Texture, (u32, u32))> {
        let display = self.display?;
        Some((self.targets?.output?.texture, display))
    }

    /// The targets as last made, and the history the last frame wrote: what a reading of the
    /// pictures takes.
    #[must_use]
    pub fn targets(&self) -> Option<(PostTargets, usize)> {
        Some((self.targets?, self.last_write))
    }

    /// Move `camera`'s projection by `clip`, an upscaler's jitter in clip space; the camera as it
    /// was.
    pub fn jitter_by(&mut self, camera: &mut Camera, clip: [f32; 2]) -> Camera {
        let held = *camera;
        camera.projection =
            Mat4::from_translation(glam::Vec3::new(clip[0], clip[1], 0.0)) * camera.projection;
        self.jitter = clip;
        held
    }

    /// How far this frame's projection is moved, in clip space, by the anti-aliasing or an
    /// upscaler: nothing while neither moves it.
    #[must_use]
    pub fn drawn_jitter(&self) -> [f32; 2] {
        self.jitter
    }

    /// This frame's place in the anti-aliasing's sequence and the sequence's length, while the
    /// anti-aliasing lays the frames of a sequence over each other; `None` while it does not.
    #[must_use]
    pub fn dither(&self) -> Option<(u32, u32)> {
        let s = self.settings;
        if !s.runs() || !s.taa || s.check {
            return None;
        }
        let frames = s.frames.clamp(1, 8);
        Some((self.frame % frames, frames))
    }

    /// Move `camera`'s projection by this frame's part of a pixel, for a world of `extent`
    /// pixels, when the anti-aliasing runs; the camera as it was.
    pub fn jitter(&mut self, camera: &mut Camera, extent: (u32, u32)) -> Camera {
        let held = *camera;
        self.jitter = [0.0; 2];
        // Checking the motion, the projection holds still, so the comparison is of the motion
        // alone.
        let s = self.settings;
        if !s.runs() || !s.taa || s.check || extent.0 == 0 || extent.1 == 0 {
            return held;
        }
        let place = self.frame % s.frames.clamp(1, 8) + 1;
        #[allow(clippy::cast_precision_loss)] // a target's extent
        let j = [
            (halton(place, 2) - 0.5) * 2.0 / extent.0 as f32,
            (halton(place, 3) - 0.5) * 2.0 / extent.1 as f32,
        ];
        camera.projection =
            Mat4::from_translation(glam::Vec3::new(j[0], j[1], 0.0)) * camera.projection;
        self.jitter = j;
        held
    }

    fn targets_for(
        &mut self,
        gfx: &mut Device,
        t: &Targets,
    ) -> Result<PostTargets, dereth_gfx::Error> {
        let uses = self.display.map_or(Uses::of(self.settings), |d| {
            Uses::upscaled(self.settings, d)
        });
        if let Some(p) = self.targets {
            if (p.width, p.height, p.uses) == (t.width, t.height, uses) {
                return Ok(p);
            }
            for x in p.textures() {
                gfx.destroy_texture(x);
            }
            self.targets = None;
        }
        let usage = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC;
        let scene_format = self.scene_format;
        let mut make = |width: u32, height: u32, format| -> Result<Bound, dereth_gfx::Error> {
            let texture = gfx.create_texture(&TextureDesc {
                label: "remaster post",
                width,
                height,
                mips: 1,
                format,
                usage,
            })?;
            Ok(Bound {
                texture,
                index: gfx.bind_texture(texture)?,
            })
        };
        let (w, h) = (t.width, t.height);
        // The bloom's levels are of what it reads, which an upscaler leaves at the display's size.
        let (bloom_w, bloom_h) = uses.display.unwrap_or((w, h));
        let scene = make(w, h, scene_format)?;
        let motion = if uses.motion {
            Some(make(w, h, MOTION)?)
        } else {
            None
        };
        let taa = if uses.taa {
            Some(TaaTargets {
                history: [make(w, h, LINEAR)?, make(w, h, LINEAR)?],
                marks: [make(w, h, MARKS)?, make(w, h, MARKS)?],
                shown: if uses.check {
                    Some(make(w, h, LINEAR)?)
                } else {
                    None
                },
            })
        } else {
            None
        };
        let bloom = if uses.bloom {
            let mut levels = [(scene, 0, 0); BLOOM_LEVELS];
            for (level, (bw, bh)) in levels.iter_mut().zip(bloom_sizes(bloom_w, bloom_h)) {
                *level = (make(bw, bh, scene_format)?, bw, bh);
            }
            Some(levels)
        } else {
            None
        };
        let output = match uses.display {
            Some((dw, dh)) => Some(make(dw, dh, COLOR)?),
            None => None,
        };
        let p = PostTargets {
            width: w,
            height: h,
            uses,
            scene,
            motion,
            taa,
            bloom,
            output,
        };
        self.targets = Some(p);
        // New targets hold nothing gathered.
        self.last_view_proj = None;
        Ok(p)
    }

    /// The frame's constants, and the plan the passes are recorded from; `None` when the passes
    /// are off. `camera` is the frame's camera unmoved; `exposed` the exposure's scale (two to the
    /// look's stops and the settings' together, worked out by the game's own arithmetic); `grade`
    /// how the frame is graded.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        camera: &Camera,
        t: &Targets,
        exposed: f32,
        grade: Grade,
    ) -> Result<Option<PostPlan>, dereth_gfx::Error> {
        let s = self.settings;
        if !self.runs() {
            self.last_view_proj = None;
            return Ok(None);
        }
        let targets = self.targets_for(gfx, t)?;
        // The anti-aliasing, or an upscaler in its place, gathers the frames.
        let gathers = targets.uses.motion;
        let view_proj = camera.projection * camera.view;
        let still = self.last_view_proj == Some(view_proj);
        let fresh = !gathers || self.last_view_proj.is_none();
        let previous = camera
            .previous_view_projection
            .filter(|_| !fresh)
            .unwrap_or(view_proj);
        let frames = s.frames.clamp(1, 8);
        let mut rows = Rows::default();
        for c in previous.to_cols_array_2d() {
            rows.0.push(c.map(f32::to_bits));
        }
        let flag = |b: bool| if b { 1.0f32 } else { 0.0 };
        #[allow(clippy::cast_precision_loss)] // small counts
        {
            rows.0
                .push([self.jitter[0], self.jitter[1], flag(still), flag(fresh)].map(f32::to_bits));
            rows.0
                .push([frames as f32, (self.frame % frames) as f32, 0.0, 0.0].map(f32::to_bits));
            rows.0.push(
                [
                    exposed,
                    s.bloom.clamp(0.0, 1.0),
                    flag(s.hdr),
                    BLOOM_LEVELS as f32,
                ]
                .map(f32::to_bits),
            );
            let look = grade.look;
            rows.0
                .push([look[0], look[1], look[2], 0.0].map(f32::to_bits));
        }
        match grade.tables {
            Some((tables, index, weights, count)) => {
                rows.0.push([tables[0], tables[1], tables[2], index]);
                rows.0
                    .push([weights[0], weights[1], weights[2], count].map(f32::to_bits));
            }
            None => {
                rows.0.push([0, 0, 0, NONE]);
                rows.0.push([0; 4]);
            }
        }
        for c in reprojection(previous, view_proj, self.jitter).to_cols_array_2d() {
            rows.0.push(c.map(f32::to_bits));
        }
        self.constants.write(gfx, &rows.bytes())?;
        let write = (self.frame % 2) as usize;
        self.last_write = write;
        self.frame = self.frame.wrapping_add(1);
        self.last_view_proj = gathers.then_some(view_proj);
        Ok(Some(PostPlan {
            taa: targets.uses.taa.then_some(if targets.uses.check {
                self.check
            } else {
                self.taa
            }),
            down: self.down,
            up: self.up,
            tonemap: self.tonemap,
            sampler: self.sampler.1,
            targets,
            write,
            constants: self.constants.index,
            output: targets.output.unwrap_or(Bound {
                texture: t.color,
                index: t.index[5],
            }),
            fresh: fresh || camera.previous_view_projection.is_none(),
            hdr: s.hdr,
        }))
    }
}

impl PostPlan {
    /// `t` with the scene's colors drawn into the passes' own target.
    #[must_use]
    pub fn scene_targets(&self, mut t: Targets) -> Targets {
        t.color = self.targets.scene.texture;
        t.index[5] = self.targets.scene.index;
        t
    }

    /// The passes' targets.
    #[must_use]
    pub fn targets(&self) -> &PostTargets {
        &self.targets
    }

    /// Whether the frames are gathered (by the anti-aliasing or an upscaler in its place): the
    /// game then writes the moving objects' motion.
    #[must_use]
    pub fn gathers(&self) -> bool {
        self.targets.uses.motion
    }

    /// The passes' constants' index in the table.
    #[must_use]
    pub fn constants(&self) -> u32 {
        self.constants
    }

    /// Whether there is no last frame to gather from, and whether the colors are linear: what an
    /// upscaler takes.
    #[must_use]
    pub fn fresh(&self) -> bool {
        self.fresh
    }

    #[must_use]
    pub fn hdr(&self) -> bool {
        self.hdr
    }

    /// The world's colors the passes leave, by itself and its index in the table: what the
    /// composite reads.
    #[must_use]
    pub fn output(&self) -> (Texture, u32) {
        (self.output.texture, self.output.index)
    }

    /// Record the anti-aliasing, when it runs: it writes the history and the marks, or, checking
    /// the motion, this frame's colors and last frame's found where each pixel was. `t` are the
    /// frame's targets with the scene's colors this pass's own ([`Self::scene_targets`]); `frame`
    /// the frame's constants' index; `outdoor_depth` the depth the outdoors are placed by;
    /// `motion` the moving objects' motion where the game wrote any. The picture the frames were
    /// gathered into: the scene's colors where the anti-aliasing doesn't run.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record_taa(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
        motion: Option<Bound>,
    ) -> Result<Gathered, dereth_gfx::Error> {
        let p = self.targets;
        let (Some(taa), Some(a)) = (self.taa, p.taa) else {
            return Ok(Gathered(Bound {
                texture: t.color,
                index: t.index[5],
            }));
        };
        let full = Rect {
            x: 0,
            y: 0,
            width: p.width,
            height: p.height,
        };
        let read = 1 - self.write;
        r.timestamp("anti-aliasing");
        let mut touches = vec![
            Touch::Texture(t.color, Use::Sampled),
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(a.history[read].texture, Use::Sampled),
            Touch::Texture(a.marks[read].texture, Use::Sampled),
        ];
        if let Some(m) = motion {
            touches.push(Touch::Texture(m.texture, Use::Sampled));
        }
        let second = a.shown.unwrap_or(a.marks[self.write]);
        let clear = Load::Clear([0.0; 4]);
        r.raster(
            &RasterPass {
                name: "anti-aliasing",
                color: &[
                    ColorAttachment {
                        texture: a.history[self.write].texture,
                        load: clear,
                    },
                    ColorAttachment {
                        texture: second.texture,
                        load: clear,
                    },
                ],
                depth: None,
                touches: &touches,
                area: Some(full),
            },
            |d| {
                d.set_pipeline(taa)?;
                d.push(
                    0,
                    &self.push(
                        frame,
                        [
                            t.index[5],
                            t.index[4],
                            outdoor_depth.1,
                            t.index[3],
                            motion.map_or(NONE, |m| m.index),
                            a.history[read].index,
                            a.marks[read].index,
                        ],
                        (0, (0, 0)),
                        NONE,
                        0,
                    ),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        Ok(Gathered(a.shown.unwrap_or(a.history[self.write])))
    }

    /// Record the passes after the frames are gathered: the bloom and the tone map.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record_finish(
        &self,
        r: &mut Recorder<'_>,
        frame: u32,
        Gathered(source): Gathered,
    ) -> Result<(), dereth_gfx::Error> {
        let p = self.targets;
        // What the bloom and the tone map run at.
        let (shown_w, shown_h) = p.uses.display.unwrap_or((p.width, p.height));

        // The bloom: down the chain, then back up it.
        if let Some(bloom) = p.bloom {
            r.timestamp("bloom");
            let mut from = (source, shown_w, shown_h);
            for (k, level) in bloom.iter().enumerate() {
                self.bloom_pass(
                    r,
                    "bloom down",
                    self.down,
                    *level,
                    from,
                    k == 0,
                    Load::Clear([0.0; 4]),
                )?;
                from = *level;
            }
            for k in (0..BLOOM_LEVELS - 1).rev() {
                self.bloom_pass(
                    r,
                    "bloom up",
                    self.up,
                    bloom[k],
                    bloom[k + 1],
                    false,
                    Load::Keep,
                )?;
            }
        }

        // The tone map into the world's colors.
        r.timestamp("tone map");
        let mut touches = vec![Touch::Texture(source.texture, Use::Sampled)];
        if let Some(bloom) = p.bloom {
            touches.push(Touch::Texture(bloom[0].0.texture, Use::Sampled));
        }
        r.raster(
            &RasterPass {
                name: "tone map",
                color: &[ColorAttachment {
                    texture: self.output.texture,
                    load: Load::Clear([0.0, 0.0, 0.0, 1.0]),
                }],
                depth: None,
                touches: &touches,
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: shown_w,
                    height: shown_h,
                }),
            },
            |d| {
                d.set_pipeline(self.tonemap)?;
                d.push(
                    0,
                    &self.push(
                        frame,
                        [NONE; 7],
                        (source.index, (shown_w, shown_h)),
                        p.bloom.map_or(NONE, |b| b[0].0.index),
                        0,
                    ),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )?;
        Ok(())
    }

    /// One step of the bloom: `to` drawn by `pipeline` from `from`; `first`, the first step
    /// down.
    #[allow(clippy::too_many_arguments)]
    fn bloom_pass(
        &self,
        r: &mut Recorder<'_>,
        name: &'static str,
        pipeline: Pipeline,
        to: (Bound, u32, u32),
        from: (Bound, u32, u32),
        first: bool,
        load: Load,
    ) -> Result<(), dereth_gfx::Error> {
        r.raster(
            &RasterPass {
                name,
                color: &[ColorAttachment {
                    texture: to.0.texture,
                    load,
                }],
                depth: None,
                touches: &[Touch::Texture(from.0.texture, Use::Sampled)],
                area: Some(Rect {
                    x: 0,
                    y: 0,
                    width: to.1,
                    height: to.2,
                }),
            },
            |d| {
                d.set_pipeline(pipeline)?;
                d.push(
                    0,
                    &self.push(
                        0,
                        [NONE; 7],
                        (from.0.index, (from.1, from.2)),
                        NONE,
                        u32::from(first),
                    ),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }

    /// The push constants `post.slang` reads: the frame's constants `frame`; the anti-aliasing's
    /// inputs `taa` (the scene, the depth, the outdoor depth, the material, the motion, the
    /// history and the marks); the picture read with its size; the bloom; the first step down.
    fn push(
        &self,
        frame: u32,
        taa: [u32; 7],
        source: (u32, (u32, u32)),
        bloom: u32,
        first: u32,
    ) -> Vec<u8> {
        #[allow(clippy::cast_precision_loss)] // a target's size
        let size = [
            (source.1 .0 as f32).to_bits(),
            (source.1 .1 as f32).to_bits(),
        ];
        let mut w = vec![frame, self.constants];
        w.extend(taa);
        w.extend([source.0, self.sampler, size[0], size[1], bloom, first]);
        push_words(&w)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The targets' bytes, linear light, the anti-aliasing and the bloom all on, the scene and the
    /// bloom in packed floats: 33.3 a pixel, 69.1 MB at 1920 by 1080 and 368.6 MB at 5120 by
    /// 2160 (38.7, 80.2 MB and 427.6 MB in 16-bit floats); checking the motion adds 8 a pixel; the
    /// anti-aliasing alone 32 a pixel; linear light and the bloom alone 5.3; off, none.
    #[test]
    fn the_targets_take_what_their_uses_need() {
        let packed = Format::Rg11b10Ufloat;
        let all = PostSettings {
            hdr: true,
            taa: true,
            ..PostSettings::default()
        };
        assert_eq!(target_bytes(1920, 1080, all, packed), 69_119_640);
        assert_eq!(target_bytes(5120, 2160, all, packed), 368_636_800);
        assert_eq!(target_bytes(1920, 1080, all, LINEAR), 80_178_480);
        assert_eq!(target_bytes(5120, 2160, all, LINEAR), 427_616_000);
        let check = PostSettings { check: true, ..all };
        assert_eq!(
            target_bytes(1920, 1080, check, packed) - target_bytes(1920, 1080, all, packed),
            1920 * 1080 * 8
        );
        let taa = PostSettings {
            taa: true,
            ..PostSettings::default()
        };
        assert_eq!(target_bytes(1920, 1080, taa, packed), 1920 * 1080 * 32);
        let hdr = PostSettings {
            hdr: true,
            ..PostSettings::default()
        };
        assert_eq!(
            target_bytes(1920, 1080, hdr, packed),
            1920 * 1080 * 4 + 2_764_440
        );
        assert_eq!(target_bytes(1920, 1080, PostSettings::default(), packed), 0);
    }

    /// The anti-aliasing's eight moves are inside the pixel, differ from each other, and average
    /// near its center.
    #[test]
    fn the_sequence_of_moves_covers_the_pixel() {
        let moves: Vec<[f32; 2]> = (1..=8)
            .map(|i| [halton(i, 2) - 0.5, halton(i, 3) - 0.5])
            .collect();
        for m in &moves {
            assert!(m.iter().all(|v| (-0.5..0.5).contains(v)), "{m:?}");
        }
        for (i, a) in moves.iter().enumerate() {
            assert!(moves[i + 1..].iter().all(|b| a != b));
        }
        let mean = moves
            .iter()
            .fold([0.0f32; 2], |s, m| [s[0] + m[0], s[1] + m[1]]);
        assert!(mean.iter().all(|v| (v / 8.0).abs() < 0.07), "{mean:?}");
    }
}
