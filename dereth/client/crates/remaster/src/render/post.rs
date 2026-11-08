//! The post passes: the scene drawn into a linear, unclamped target of its own, then temporal
//! anti-aliasing, the bloom, the exposure and a tone curve into the world's colors. Off, the
//! scene is drawn into the world's colors directly, in display values, and the frame is the one
//! drawn without them.
//!
//! - **The scene's target.** With the passes on, the resolve, the sky, the cut-outs' edges and the
//!   forward pass draw into the pass's own target (RGBA16F) through their linear pipelines
//!   ([`PostPlan::scene_pipelines`], [`PostPlan::scene_targets`]); with [`PostSettings::hdr`]
//!   the colors are linear light and unclamped, otherwise they are display values.
//! - **Temporal anti-aliasing.** Each frame's projection is moved by a different part of a pixel
//!   ([`PostPass::jitter`]); `fs_taa` lays the frames over each other where each pixel's surface
//!   was the frame before: by the moving objects' own last transforms where they moved (a pass
//!   that draws only those parts again), by the depth and the last frame's view elsewhere.
//! - **The bloom, the exposure and the tone map**, when the colors are linear.
//! - **With an upscaler on** ([`super::upscale`]) it stands in for the anti-aliasing: the scene and
//!   the motion stay at the render size, and the bloom and the tone map run at the display's size
//!   into a target of their own, which the composite reads.
//!
//! The shaders are `shaders/post.slang` and `shaders/motion.slang`; every pipeline is made with the
//! pass ([`PostPass::new`]) and none after.

use dereth_gfx::{
    Blend, Buffer, BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Factor, Format,
    GraphicsDesc, IndexType, Load, Pipeline, RasterPass, Recorder, Rect, Sampler, SamplerDesc,
    Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::Mat4;

use super::light::Rows;
use super::scene::{Bucket, GpuScene, NONE};
use super::{push_words, shader, PerFrame, Pipelines, ScenePipelines, Targets};
use crate::scene::{Camera, FrameDelta};
use crate::shaders::Entry;

/// The saturation, the contrast and the vignette of the frame with no grade.
pub const NO_GRADE: [f32; 3] = [1.0, 1.0, 0.0];

/// The saturation, the contrast and the vignette of the frame while an art pack's look is drawn.
pub const PACK_GRADE: [f32; 3] = [1.08, 1.04, 0.28];

/// How the post passes draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostSettings {
    /// Whether the scene is drawn in linear light, unclamped, then exposed, bloomed and tone
    /// mapped to the display. Off, its colors are retail's display values.
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
    /// where each pixel's surface was ([`super::Renderer::read_post`] reads it beside this
    /// frame's). Its target is made only while checking.
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
pub(crate) const LINEAR: Format = Format::Rgba16Float;
const MARKS: Format = Format::Rgba8Unorm;
const MOTION: Format = Format::Rg16Snorm;

/// The scene's and the bloom's format on `gfx`: packed floats where it draws into them, as they
/// need no alpha, else [`LINEAR`].
pub(crate) fn scene_format(gfx: &Device) -> Format {
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
const BLOOM_LEVELS: usize = 6;

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
struct Bound {
    texture: Texture,
    index: u32,
}

/// What the anti-aliasing keeps, at the world's size.
#[derive(Debug, Clone, Copy)]
struct TaaTargets {
    /// The colors gathered (rgb) and how many frames (a): this frame's and the last's by turns.
    /// What a frame gathers is what it shows.
    history: [Bound; 2],
    /// Each pixel's marks of the frames at the sequence's eight places, by turns as the history.
    marks: [Bound; 2],
    /// Checking the motion: last frame's picture found where each pixel's surface was.
    shown: Option<Bound>,
}

/// The picture the frames were gathered into, for the passes after it.
#[derive(Debug, Clone, Copy)]
pub struct Gathered(Bound);

/// The pass's targets, at the world's size.
#[derive(Debug, Clone, Copy)]
struct PostTargets {
    width: u32,
    height: u32,
    uses: Uses,
    /// The scene's colors.
    scene: Bound,
    /// The moving objects' last places in clip space, halved; -1 where nothing moved.
    motion: Option<Bound>,
    taa: Option<TaaTargets>,
    /// The bloom's levels, each with its size.
    bloom: Option<[(Bound, u32, u32); BLOOM_LEVELS]>,
    /// The frame at the display's size, where an upscaler brings it up to that.
    output: Option<Bound>,
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

/// The post passes.
#[derive(Debug)]
pub struct PostPass {
    /// The resolves, the forward pass, the edges and the sky, drawing into the scene's target,
    /// of `scene_format`.
    scene: ScenePipelines,
    scene_format: Format,
    taa: Pipeline,
    check: Pipeline,
    down: Pipeline,
    up: Pipeline,
    tonemap: Pipeline,
    motion: Pipeline,
    sampler: (Sampler, u32),
    settings: PostSettings,
    /// The display's size, while an upscaler brings the frame up to it.
    display: Option<(u32, u32)>,
    constants: PerFrame,
    /// Whether the frame takes an art pack's grade.
    pack_look: bool,
    /// The moving parts' items and last transforms, and their draw commands.
    moving: PerFrame,
    commands: PerFrame,
    targets: Option<PostTargets>,
    /// The rows an instance takes in the scene's instance buffer.
    instance_rows: u32,
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
    scene: ScenePipelines,
    /// The anti-aliasing's pipeline, or its check of the motion's.
    taa: Option<Pipeline>,
    down: Pipeline,
    up: Pipeline,
    tonemap: Pipeline,
    motion: Pipeline,
    sampler: u32,
    targets: PostTargets,
    /// The history and marks this frame writes; it reads the others.
    write: usize,
    constants: u32,
    moving: u32,
    commands: Buffer,
    moving_count: u32,
    /// The bytes one of its commands takes.
    command_bytes: u32,
    instance_rows: u32,
    /// The world's colors the tone map writes, which the composite reads.
    output: Bound,
    /// Whether there is no last frame to gather from, and whether the colors are linear: what an
    /// upscaler takes.
    fresh: bool,
    hdr: bool,
}

/// The `i`th number of the base-`b` Halton sequence, in [0, 1).
pub(crate) fn halton(mut i: u32, b: u32) -> f32 {
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
                vertex: shader(Entry::FullscreenVsMain),
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
        let tonemap = full(gfx, Entry::PostFsTonemap, &[target(super::COLOR)])?;
        let motion = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster motion",
            vertex: shader(Entry::MotionVsMain),
            fragment: Some(shader(Entry::MotionFsMain)),
            color: &[target(MOTION)],
            depth: None,
            cull: Cull::None,
        })?;
        let sampler = gfx.create_sampler(&SamplerDesc {
            filter: dereth_gfx::Filter::Linear,
            mip: dereth_gfx::Filter::Nearest,
            address: dereth_gfx::Address::Clamp,
            address_v: dereth_gfx::Address::Clamp,
            anisotropy: 1,
            lod_bias: 0.0,
        })?;
        let sampler_index = gfx.bind_sampler(sampler)?;
        let instance_rows = u32::try_from(
            GpuScene::instance_bytes(Mat4::IDENTITY, glam::Vec3::ONE, NONE, 0, 0, NONE).len() / 16,
        )
        .unwrap_or(0);
        Ok(Self {
            scene: ScenePipelines::new(gfx, scene_format)?,
            scene_format,
            taa,
            check,
            down,
            up,
            tonemap,
            motion,
            sampler: (sampler, sampler_index),
            settings: PostSettings::default(),
            display: None,
            constants: PerFrame::new(gfx, "remaster post constants", 128, BufferUsage::default())?,
            pack_look: false,
            moving: PerFrame::new(
                gfx,
                "remaster moving parts",
                1 << 14,
                BufferUsage::default(),
            )?,
            commands: PerFrame::new(
                gfx,
                "remaster moving commands",
                1 << 12,
                BufferUsage::INDIRECT,
            )?,
            targets: None,
            instance_rows,
            frame: 0,
            last_view_proj: None,
            jitter: [0.0; 2],
            last_write: 0,
        })
    }

    /// Whether the frame takes an art pack's grade.
    pub fn set_pack_look(&mut self, on: bool) {
        self.pack_look = on;
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
    pub(super) fn upscaled_output(&self) -> Option<(Texture, (u32, u32))> {
        let display = self.display?;
        Some((self.targets?.output?.texture, display))
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
            Some((dw, dh)) => Some(make(dw, dh, super::COLOR)?),
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

    /// The frame's constants and the moving parts, and the plan the passes are recorded from;
    /// `None` when the passes are off. `camera` is the frame's camera unmoved; `exposure` the
    /// look's stops.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
        camera: &Camera,
        t: &Targets,
        exposure: f32,
        scene: &GpuScene,
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
        let exposed = dereth_primitives::num::math::powf(2.0, exposure + s.exposure);
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
            let grade = if self.pack_look { PACK_GRADE } else { NO_GRADE };
            rows.0
                .push([grade[0], grade[1], grade[2], 0.0].map(f32::to_bits));
        }
        self.constants.write(gfx, &rows.bytes())?;
        let command_bytes = super::command_bytes(gfx);
        let (moving, commands) = if gathers {
            moving_parts(frame, scene, gfx.backend())
        } else {
            (Vec::new(), Vec::new())
        };
        let moving_count = u32::try_from(commands.len() / command_bytes as usize).unwrap_or(0);
        self.moving.write(gfx, &moving)?;
        self.commands.write(gfx, &commands)?;
        let write = (self.frame % 2) as usize;
        self.last_write = write;
        self.frame = self.frame.wrapping_add(1);
        self.last_view_proj = gathers.then_some(view_proj);
        Ok(Some(PostPlan {
            scene: self.scene,
            taa: targets.uses.taa.then_some(if targets.uses.check {
                self.check
            } else {
                self.taa
            }),
            down: self.down,
            up: self.up,
            tonemap: self.tonemap,
            motion: self.motion,
            sampler: self.sampler.1,
            targets,
            write,
            constants: self.constants.index,
            moving: self.moving.index,
            commands: self.commands.buffer,
            moving_count,
            command_bytes,
            instance_rows: self.instance_rows,
            output: targets.output.unwrap_or(Bound {
                texture: t.color,
                index: t.index[5],
            }),
            fresh: fresh || camera.previous_view_projection.is_none(),
            hdr: s.hdr,
        }))
    }
}

/// The parts of the frame's objects that moved since the last frame and that the G-buffer
/// draws: a row of five for each of their opaque draw items (the item, then the part's last
/// transform's four columns), and a draw command for each, naming its row.
fn moving_parts(
    frame: &FrameDelta,
    scene: &GpuScene,
    backend: dereth_gfx::Backend,
) -> (Vec<u8>, Vec<u8>) {
    let poses: std::collections::BTreeMap<_, _> =
        frame.objects.iter().map(|o| (o.key, o)).collect();
    let mut rows = Vec::new();
    let mut commands = Vec::new();
    let mut k = 0u32;
    for dp in &frame.drawn_parts {
        let (Some(o), Some(pose)) = (scene.objects.get(&dp.key), poses.get(&dp.key)) else {
            continue;
        };
        let (Some(p), Some(pp)) = (
            o.parts.get(dp.part as usize),
            pose.parts.get(dp.part as usize),
        ) else {
            continue;
        };
        if pp.hidden || pp.previous == pp.transform {
            continue;
        }
        let Some(items) = p.levels.get(usize::from(pp.level)) else {
            continue;
        };
        for i in items.clone() {
            if matches!(
                scene.buckets.get(i as usize),
                Some(Bucket::Blended | Bucket::Additive)
            ) {
                continue;
            }
            let Some(d) = scene.item_draws.get(i as usize) else {
                continue;
            };
            rows.extend(push_words(&[i, 0, 0, 0]));
            for c in pp.previous.to_cols_array() {
                rows.extend(c.to_le_bytes());
            }
            commands.extend(super::command(backend, d.index_count, d.first_index, k));
            k += 1;
        }
    }
    (rows, commands)
}

impl PostPlan {
    /// `p` with the scene's colors drawn by the linear pipelines.
    #[must_use]
    pub(super) fn scene_pipelines(&self, p: Pipelines) -> Pipelines {
        p.with_scene(&self.scene)
    }

    /// `t` with the scene's colors drawn into the linear target.
    #[must_use]
    pub fn scene_targets(&self, mut t: Targets) -> Targets {
        t.color = self.targets.scene.texture;
        t.index[5] = self.targets.scene.index;
        t
    }

    /// What an upscaler takes from the passes.
    #[must_use]
    pub(crate) fn upscale_inputs(&self) -> super::upscale::UpscaleInputs {
        super::upscale::UpscaleInputs {
            color: self.targets.scene.index,
            #[cfg(any(feature = "dlss", target_os = "macos"))]
            color_texture: self.targets.scene.texture,
            fresh: self.fresh,
            hdr: self.hdr,
        }
    }

    /// The world's colors the passes leave, by itself and its index in the table: what the
    /// composite reads.
    #[must_use]
    pub fn output(&self) -> (Texture, u32) {
        (self.output.texture, self.output.index)
    }

    /// Record the passes. `t` are the frame's targets with the scene's colors this pass's own
    /// ([`Self::scene_targets`]); `frame` the frame's constants' index; `outdoor_depth` the depth
    /// the outdoors are placed by, as the resolve has it; `draw` the scene's items', instances'
    /// and vertices' indices in the table, and its index buffer; `upscale` the upscaler, while one
    /// stands in for the anti-aliasing. The passes up to the frames gathered: a scaler outside the
    /// layer runs between them and [`Self::record_finish`].
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record_scene(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
        draw: ([u32; 3], Buffer),
        upscale: Option<&super::upscale::UpscalePlan>,
    ) -> Result<Gathered, dereth_gfx::Error> {
        let p = self.targets;
        let full = Rect {
            x: 0,
            y: 0,
            width: p.width,
            height: p.height,
        };
        let read = 1 - self.write;

        // The moving parts' last places.
        let motion = match p.motion {
            Some(motion) if self.moving_count > 0 => {
                r.timestamp("motion");
                r.raster(
                    &RasterPass {
                        name: "motion",
                        color: &[ColorAttachment {
                            texture: motion.texture,
                            load: Load::Clear([-1.0, -1.0, 0.0, 0.0]),
                        }],
                        depth: None,
                        touches: &[
                            Touch::Buffer(self.commands, Use::Indirect),
                            Touch::Buffer(draw.1, Use::Index),
                            Touch::Texture(t.depth, Use::Sampled),
                            Touch::Texture(outdoor_depth.0, Use::Sampled),
                        ],
                        area: Some(full),
                    },
                    |d| {
                        d.set_pipeline(self.motion)?;
                        d.bind_index_buffer(draw.1, 0, IndexType::U32)?;
                        d.push(
                            0,
                            &push_words(&[
                                frame,
                                self.constants,
                                draw.0[0],
                                draw.0[1],
                                draw.0[2],
                                self.moving,
                                self.instance_rows,
                                t.index[4],
                                outdoor_depth.1,
                            ]),
                        )?;
                        d.draw_indexed_indirect(
                            self.commands,
                            0,
                            self.moving_count,
                            self.command_bytes,
                        )?;
                        Ok(())
                    },
                )?;
                Some(motion)
            }
            _ => None,
        };

        // The anti-aliasing: it writes the history and the marks, or, checking the motion, this
        // frame's colors and last frame's found where each pixel was.
        let source = match (upscale, self.taa, p.taa) {
            (Some(u), _, _) => {
                u.record(
                    r,
                    t,
                    frame,
                    self.constants,
                    outdoor_depth,
                    motion.map(|m| (m.texture, m.index)),
                )?;
                let (texture, index) = u.output();
                Bound { texture, index }
            }
            (None, Some(taa), Some(a)) => {
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
                a.shown.unwrap_or(a.history[self.write])
            }
            _ => Bound {
                texture: t.color,
                index: t.index[5],
            },
        };

        Ok(Gathered(source))
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

/// One picture of the post passes, read back: its size and each pixel's four values.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PostPicture {
    pub width: u32,
    pub height: u32,
    /// The scene's colors this frame.
    pub scene: Vec<[f32; 4]>,
    /// The colors shown: anti-aliased (alpha the frames gathered); or, checking the motion, last
    /// frame's found where each pixel was (alpha 1 where it was found).
    pub shown: Vec<[f32; 4]>,
    /// The moving objects' last places in clip space (xy), where they are drawn (w 1).
    pub motion: Vec<[f32; 4]>,
    /// How far the frame's projection was moved, in clip space.
    pub jitter: [f32; 2],
}

/// Two to the power `k`, for `k` a normal float's exponent (-126 to 127).
fn pow2(k: i32) -> f32 {
    f32::from_bits(u32::try_from(127 + k).unwrap_or(0) << 23)
}

/// The value of an unsigned float of 5 exponent bits and `m_bits` mantissa bits, `bits` its low
/// bits.
fn unsigned_float(bits: u32, m_bits: u32) -> f32 {
    let e = i32::try_from((bits >> m_bits) & 0x1f).unwrap_or(0);
    #[allow(clippy::cast_precision_loss)] // at most 6 bits
    let m = (bits & ((1 << m_bits) - 1)) as f32;
    #[allow(clippy::cast_precision_loss)] // 5 or 6
    let scale = (1u32 << m_bits) as f32;
    match e {
        0 => m / scale * pow2(-14),
        31 if m == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + m / scale) * pow2(e - 15),
    }
}

/// A packed float pixel's red, green and blue, and an alpha of 1.
fn packed(px: u32) -> [f32; 4] {
    [
        unsigned_float(px & 0x7ff, 6),
        unsigned_float((px >> 11) & 0x7ff, 6),
        unsigned_float(px >> 22, 5),
        1.0,
    ]
}

/// A 16-bit float's value.
fn half(h: u16) -> f32 {
    let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
    let e = i32::from((h >> 10) & 0x1f);
    let m = f32::from(h & 0x3ff);
    match e {
        0 => sign * m * pow2(-24),
        31 if m == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + m / 1024.0) * pow2(e - 15),
    }
}

impl super::Renderer {
    /// How the post passes draw from the next frame on.
    pub fn set_post(&mut self, settings: PostSettings) {
        self.post.set_settings(settings);
    }

    /// How the post passes draw.
    #[must_use]
    pub fn post_settings(&self) -> PostSettings {
        self.post.settings()
    }

    /// The last frame's post pictures, at the render size, the colors shown being the scene's
    /// while an upscaler stands in for the anti-aliasing; `None` while the passes are off or before
    /// they have run. The device waits for its work first.
    pub fn read_post(&mut self) -> Option<PostPicture> {
        if !self.post.runs() {
            return None;
        }
        let p = self.post.targets?;
        self.gfx.wait_idle().ok()?;
        let linear = |bytes: Vec<u8>| -> Vec<[f32; 4]> {
            bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|px| {
                    let v = |k: usize| half(u16::from_le_bytes([px[k * 2], px[k * 2 + 1]]));
                    [v(0), v(1), v(2), v(3)]
                })
                .collect()
        };
        let scene_bytes = self.gfx.read_texture(p.scene.texture, 0).ok()?;
        let scene = if self.post.scene_format == LINEAR {
            linear(scene_bytes)
        } else {
            scene_bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|px| packed(u32::from_le_bytes(*px)))
                .collect()
        };
        let motion = match p.motion {
            Some(m) => self
                .gfx
                .read_texture(m.texture, 0)
                .ok()?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|px| {
                    let v = |k: usize| {
                        f32::from(i16::from_le_bytes([px[k * 2], px[k * 2 + 1]])) / 32767.0
                    };
                    let (x, y) = (v(0).max(-1.0), v(1).max(-1.0));
                    let moved = x > -0.99995 || y > -0.99995;
                    [x * 2.0, y * 2.0, 0.0, if moved { 1.0 } else { 0.0 }]
                })
                .collect(),
            None => vec![[0.0; 4]; scene.len()],
        };
        let shown = match p.taa {
            Some(a) => {
                let shown = a.shown.unwrap_or(a.history[self.post.last_write]);
                linear(self.gfx.read_texture(shown.texture, 0).ok()?)
            }
            None => scene.clone(),
        };
        Some(PostPicture {
            width: p.width,
            height: p.height,
            scene,
            shown,
            motion,
            jitter: self.post.jitter,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The 16-bit floats read back are their values: 1, -2, a half, the smallest normal, a
    /// denormal and infinity.
    #[test]
    fn a_half_reads_as_its_value() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0xC000), -2.0);
        assert_eq!(half(0x3800), 0.5);
        assert_eq!(half(0x0400), pow2(-14));
        assert_eq!(half(0x0001), pow2(-24));
        assert_eq!(half(0x7C00), f32::INFINITY);
    }

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

    /// Packed floats read back as the values they hold: 1, 0.5, the smallest normal, a
    /// denormal, and infinity, in each channel's own widths.
    #[test]
    fn packed_floats_read_back_as_their_values() {
        assert_eq!(packed(15 << 6), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(packed((14 << 6) << 11), [0.0, 0.5, 0.0, 1.0]);
        assert_eq!(packed((1 << 5) << 22), [0.0, 0.0, pow2(-14), 1.0]);
        assert_eq!(packed(1), [pow2(-20), 0.0, 0.0, 1.0]);
        assert_eq!(packed(31 << 6)[0], f32::INFINITY);
        assert_eq!(packed((15 << 5 | 16) << 22)[2], 1.5);
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
