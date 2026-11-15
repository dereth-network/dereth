//! The post passes: the scene drawn into a linear, unclamped target of its own, then temporal
//! anti-aliasing, the bloom, the exposure and a tone curve into the world's colors. Off, the
//! scene is drawn into the world's colors directly, in display values, and the frame is the one
//! drawn without them.
//!
//! The passes themselves, their targets and their settings are the shared passes'
//! (`dereth_gfx_passes::post`); this module draws the scene into their target and writes the
//! moving parts' motion the anti-aliasing reads, and grades the frame.
//!
//! - **The scene's target.** With the passes on, the resolve, the sky, the cut-outs' edges and the
//!   forward pass draw into the pass's own target through their linear pipelines
//!   ([`PostPlan::scene_pipelines`], [`PostPlan::scene_targets`]); with [`PostSettings::hdr`]
//!   the colors are linear light and unclamped, otherwise they are display values.
//! - **The moving parts' motion.** Where the frames are gathered, a pass draws only the parts
//!   that moved again, at their own last transforms, and the anti-aliasing finds them there; it
//!   finds the rest by the depth and the last frame's view.
//! - **With an upscaler on** ([`super::upscale`]) it stands in for the anti-aliasing.
//!
//! The shader of the motion is `shaders/motion.slang`; its pipeline is made with the pass
//! ([`PostPass::new`]) and none after.

use dereth_gfx::{
    Buffer, BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc,
    IndexType, Load, Pipeline, RasterPass, Recorder, Rect, Texture, TextureDesc, TextureUsage,
    Touch, Use,
};
use dereth_gfx_passes::post::Grade;
pub use dereth_gfx_passes::post::{
    halton, scene_format, target_bytes, Bound, Gathered, PostSettings, LINEAR, MOTION, NO_GRADE,
};
use glam::Mat4;

use super::scene::{Bucket, GpuScene, NONE};
use super::{push_words, shader, PerFrame, Pipelines, ScenePipelines, Targets};
use crate::scene::{Camera, FrameDelta};
use crate::shaders::Entry;

/// The saturation, the contrast and the vignette of the frame while an art pack's look is drawn.
pub const PACK_GRADE: [f32; 3] = [1.08, 1.04, 0.28];

/// The post passes: the shared passes, and what draws into them.
#[derive(Debug)]
pub struct PostPass {
    /// The shared passes: the targets, the anti-aliasing, the bloom and the tone map.
    passes: dereth_gfx_passes::post::PostPass,
    /// The resolves, the forward pass, the edges and the sky, drawing into the scene's target.
    scene: ScenePipelines,
    motion: Pipeline,
    /// Whether the frame takes an art pack's grade.
    pack_look: bool,
    /// Whether the frame is graded by the tables of its kind of day, and the tables' picture
    /// with its index in the table, made the first time it is.
    grade: bool,
    grade_tables: Option<(Texture, u32)>,
    /// The tables it holds, and a picture of older ones to let go of.
    grade_set: [super::grade::Grade; 7],
    grade_stale: Option<Texture>,
    /// The moving parts' items and last transforms, and their draw commands.
    moving: PerFrame,
    commands: PerFrame,
    /// The rows an instance takes in the scene's instance buffer.
    instance_rows: u32,
}

/// One frame of the passes, as [`PostPass::prepare`] made it.
#[derive(Debug, Clone, Copy)]
pub struct PostPlan {
    /// The shared passes' part of the frame.
    passes: dereth_gfx_passes::post::PostPlan,
    scene: ScenePipelines,
    motion: Pipeline,
    moving: u32,
    commands: Buffer,
    moving_count: u32,
    /// The bytes one of its commands takes.
    command_bytes: u32,
    instance_rows: u32,
}

impl PostPass {
    /// The passes on `gfx`, their pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let passes = dereth_gfx_passes::post::PostPass::new(gfx)?;
        let motion = gfx.create_graphics_pipeline(&GraphicsDesc {
            label: "remaster motion",
            vertex: shader(Entry::MotionVsMain),
            fragment: Some(shader(Entry::MotionFsMain)),
            color: &[ColorTarget {
                format: MOTION,
                blend: None,
                write_alpha: true,
            }],
            depth: None,
            cull: Cull::None,
        })?;
        let instance_rows = u32::try_from(
            GpuScene::instance_bytes(Mat4::IDENTITY, glam::Vec3::ONE, NONE, 0, 0, NONE).len() / 16,
        )
        .unwrap_or(0);
        Ok(Self {
            scene: ScenePipelines::new(gfx, passes.scene_format())?,
            passes,
            motion,
            pack_look: false,
            grade: false,
            grade_tables: None,
            grade_set: super::grade::TABLES,
            grade_stale: None,
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
            instance_rows,
        })
    }

    /// Whether the frame takes an art pack's grade.
    pub fn set_pack_look(&mut self, on: bool) {
        self.pack_look = on;
    }

    /// Whether the frame is graded by the tables of its kind of day.
    pub fn set_grade(&mut self, on: bool) {
        self.grade = on;
    }

    /// The grade's tables, from the next frame on: made again when they change.
    pub fn set_grade_tables(&mut self, tables: [super::grade::Grade; 7]) {
        if tables != self.grade_set {
            self.grade_set = tables;
            if let Some((texture, _)) = self.grade_tables.take() {
                self.grade_stale = Some(texture);
            }
        }
    }

    /// The grade's tables, made the first time they are asked for.
    fn grade_tables(&mut self, gfx: &mut Device) -> Result<u32, dereth_gfx::Error> {
        if let Some(old) = self.grade_stale.take() {
            gfx.destroy_texture(old);
        }
        if let Some((_, index)) = self.grade_tables {
            return Ok(index);
        }
        let tables = &self.grade_set;
        let side = super::grade::SIDE;
        let texture = gfx.create_texture(&TextureDesc {
            label: "remaster grade tables",
            width: side * side,
            height: side * u32::try_from(tables.len()).unwrap_or(1),
            mips: 1,
            format: Format::Rgba8Unorm,
            usage: TextureUsage::SAMPLED,
        })?;
        gfx.upload_texture(texture, 0, &super::grade::picture(tables))?;
        let index = gfx.bind_texture(texture)?;
        self.grade_tables = Some((texture, index));
        Ok(index)
    }

    /// How the passes draw from the next frame on.
    pub fn set_settings(&mut self, settings: PostSettings) {
        self.passes.set_settings(settings);
    }

    /// How the passes draw.
    #[must_use]
    pub fn settings(&self) -> PostSettings {
        self.passes.settings()
    }

    /// Whether the scene's colors are drawn linear and unclamped.
    #[must_use]
    pub fn linear(&self) -> bool {
        self.passes.linear()
    }

    /// While an upscaler brings the frame up to a display of `display`, or `None`, from the next
    /// frame on.
    pub fn set_upscaling(&mut self, display: Option<(u32, u32)>) {
        self.passes.set_upscaling(display);
    }

    /// Whether the passes run: asked for, or because an upscaler takes the scene from them.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.passes.runs()
    }

    /// What the upscaler's picture is left as: the target at the display's size, with its size,
    /// while one is on.
    pub(super) fn upscaled_output(&self) -> Option<(Texture, (u32, u32))> {
        self.passes.upscaled_output()
    }

    /// Move `camera`'s projection by `clip`, an upscaler's jitter in clip space; the camera as it
    /// was.
    pub fn jitter_by(&mut self, camera: &mut Camera, clip: [f32; 2]) -> Camera {
        self.passes.jitter_by(camera, clip)
    }

    /// How far this frame's projection is moved, in clip space, by the anti-aliasing or an
    /// upscaler: nothing while neither moves it.
    #[must_use]
    pub fn drawn_jitter(&self) -> [f32; 2] {
        self.passes.drawn_jitter()
    }

    /// This frame's place in the anti-aliasing's sequence and the sequence's length, while the
    /// anti-aliasing lays the frames of a sequence over each other; `None` while it does not.
    #[must_use]
    pub fn dither(&self) -> Option<(u32, u32)> {
        self.passes.dither()
    }

    /// Move `camera`'s projection by this frame's part of a pixel, for a world of `extent`
    /// pixels, when the anti-aliasing runs; the camera as it was.
    pub fn jitter(&mut self, camera: &mut Camera, extent: (u32, u32)) -> Camera {
        self.passes.jitter(camera, extent)
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
        let grade = if !self.passes.runs() {
            Grade::default()
        } else {
            let look = if self.pack_look { PACK_GRADE } else { NO_GRADE };
            let tables = if self.grade {
                let index = self.grade_tables(gfx)?;
                let b = super::grade::blend(frame);
                #[allow(clippy::cast_precision_loss)] // a handful of tables
                let count = super::grade::TABLES.len() as f32;
                Some((b.tables, index, b.weights, count))
            } else {
                None
            };
            Grade { look, tables }
        };
        let exposed =
            dereth_primitives::num::math::powf(2.0, exposure + self.passes.settings().exposure);
        let Some(passes) = self.passes.prepare(gfx, camera, t, exposed, grade)? else {
            return Ok(None);
        };
        let command_bytes = super::command_bytes(gfx);
        let (moving, commands) = if passes.gathers() {
            moving_parts(frame, scene, gfx.backend())
        } else {
            (Vec::new(), Vec::new())
        };
        let moving_count = u32::try_from(commands.len() / command_bytes as usize).unwrap_or(0);
        self.moving.write(gfx, &moving)?;
        self.commands.write(gfx, &commands)?;
        Ok(Some(PostPlan {
            passes,
            scene: self.scene,
            motion: self.motion,
            moving: self.moving.index,
            commands: self.commands.buffer,
            moving_count,
            command_bytes,
            instance_rows: self.instance_rows,
        }))
    }
}

/// How a pixel is found where it was last frame.
pub use dereth_gfx_passes::frame::reprojection;

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
        let Some(items) = p.levels.get(usize::from(pp.level)) else {
            continue;
        };
        // A skinned body's vertices move however its anchor does: its items are drawn every
        // frame, each naming where its vertices' last places are kept (one more than their first,
        // so 0 says none).
        let last_of = |i: u32| {
            scene
                .item_draws
                .get(i as usize)
                .and_then(|d| scene.previous.get(&d.vertex_base))
                .map(|r| r.start + 1)
        };
        let skinned = items.clone().any(|i| last_of(i).is_some());
        if pp.hidden || (pp.previous == pp.transform && !skinned) {
            continue;
        }
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
            rows.extend(push_words(&[i, last_of(i).unwrap_or(0), 0, 0]));
            for c in pp.previous.to_cols_array() {
                rows.extend(c.to_le_bytes());
            }
            super::command_into(backend, d.index_count, d.first_index, k, &mut commands);
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
    pub fn scene_targets(&self, t: Targets) -> Targets {
        self.passes.scene_targets(t)
    }

    /// What an upscaler takes from the passes.
    #[must_use]
    pub(crate) fn upscale_inputs(&self) -> super::upscale::UpscaleInputs {
        let scene = self.passes.targets().scene;
        super::upscale::UpscaleInputs {
            color: scene.index,
            #[cfg(any(all(feature = "dlss", windows), target_os = "macos"))]
            color_texture: scene.texture,
            fresh: self.passes.fresh(),
            hdr: self.passes.hdr(),
        }
    }

    /// The world's colors the passes leave, by itself and its index in the table: what the
    /// composite reads.
    #[must_use]
    pub fn output(&self) -> (Texture, u32) {
        self.passes.output()
    }

    /// Record the passes. `t` are the frame's targets with the scene's colors this pass's own
    /// ([`Self::scene_targets`]); `frame` the frame's constants' index; `outdoor_depth` the depth
    /// the outdoors are placed by, as the resolve has it; `draw` the scene's items', instances'
    /// and vertices' indices in the table, and its index buffer; `upscale` the upscaler, while one
    /// stands in for the anti-aliasing, and the grass field, whose blades write their own motion.
    /// The passes up to the frames gathered: a scaler outside the layer runs between them and
    /// [`Self::record_finish`].
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
        (upscale, grass): (
            Option<&super::upscale::UpscalePlan>,
            Option<&super::grass::GrassPlan>,
        ),
    ) -> Result<Gathered, dereth_gfx::Error> {
        let p = self.passes.targets();
        let full = Rect {
            x: 0,
            y: 0,
            width: p.width,
            height: p.height,
        };
        let constants = self.passes.constants();

        // The moving parts' last places, and the grass's, as the wind bent it.
        let grass = grass.filter(|g| g.blades());
        let motion = match p.motion {
            Some(motion) if self.moving_count > 0 || grass.is_some() => {
                r.timestamp("motion");
                let mut touches = vec![
                    Touch::Buffer(self.commands, Use::Indirect),
                    Touch::Buffer(draw.1, Use::Index),
                    Touch::Texture(t.depth, Use::Sampled),
                    Touch::Texture(outdoor_depth.0, Use::Sampled),
                ];
                if let Some(g) = grass {
                    touches.extend(g.motion_touches());
                }
                r.raster(
                    &RasterPass {
                        name: "motion",
                        color: &[ColorAttachment {
                            texture: motion.texture,
                            load: Load::Clear([-1.0, -1.0, 0.0, 0.0]),
                        }],
                        depth: None,
                        touches: &touches,
                        area: Some(full),
                    },
                    |d| {
                        if let Some(g) = grass {
                            g.draw_motion(d, frame, constants, (t.index[4], outdoor_depth.1))?;
                        }
                        if self.moving_count == 0 {
                            return Ok(());
                        }
                        d.set_pipeline(self.motion)?;
                        d.bind_index_buffer(draw.1, 0, IndexType::U32)?;
                        d.push(
                            0,
                            &push_words(&[
                                frame,
                                constants,
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

        // The anti-aliasing, or the upscaler in its place.
        match upscale {
            Some(u) => {
                u.record(
                    r,
                    t,
                    frame,
                    constants,
                    outdoor_depth,
                    motion.map(|m| (m.texture, m.index)),
                )?;
                let (texture, index) = u.output();
                Ok(Gathered(Bound { texture, index }))
            }
            None => self.passes.record_taa(r, t, frame, outdoor_depth, motion),
        }
    }

    /// Record the passes after the frames are gathered: the bloom and the tone map.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record_finish(
        &self,
        r: &mut Recorder<'_>,
        frame: u32,
        gathered: Gathered,
    ) -> Result<(), dereth_gfx::Error> {
        self.passes.record_finish(r, frame, gathered)
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
        let (p, last_write) = self.post.passes.targets()?;
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
        let scene = if self.post.passes.scene_format() == LINEAR {
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
                let shown = a.shown.unwrap_or(a.history[last_write]);
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
            jitter: self.post.passes.drawn_jitter(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;
    use dereth_primitives::num::math;

    /// Where a pixel's surface was last frame, in last frame's clip space, worked out exactly in
    /// double precision from its place and its distance along its line of sight.
    fn exact_last(
        eye: glam::DVec3,
        (yaw, pitch): (f64, f64),
        last: glam::DMat4,
        ndc: glam::DVec2,
        projection: glam::DMat4,
    ) -> glam::DVec2 {
        let view = glam::DMat4::look_to_lh(
            eye,
            glam::DVec3::new(
                -math::sin(yaw) * math::cos(pitch),
                math::cos(yaw) * math::cos(pitch),
                math::sin(pitch),
            ),
            glam::DVec3::Z,
        );
        let inv = (projection * view).inverse();
        let far = inv * glam::DVec4::new(ndc.x, ndc.y, 0.5, 1.0);
        let c = last * (far / far.w);
        glam::DVec2::new(c.x / c.w, c.y / c.w)
    }

    /// **A turning view finds a far pixel's last place whatever its depth**: the camera of the
    /// screens before the world, two hundred meters from the frame's space's origin, turning a
    /// little, with the near plane a tenth of a meter off; a point three kilometers out, its depth
    /// as the depth buffer holds it and a few steps of that either way, is found where it was to
    /// within three hundredths of a pixel of a 1080-pixel-high picture, as it is by the exact sum
    /// (taken through the frame's space in single precision, it was 0.17 pixels off).
    #[test]
    fn a_turning_view_finds_a_far_pixels_last_place_whatever_its_depth() {
        let projection = Mat4::perspective_lh(1.0, 16.0 / 9.0, 0.1, 4000.0);
        let eye = glam::Vec3::new(96.0, 124.8, 141.0);
        let view_of = |yaw: f32, pitch: f32| {
            Mat4::look_to_lh(
                eye,
                glam::Vec3::new(
                    -math::sinf(yaw) * math::cosf(pitch),
                    math::cosf(yaw) * math::cosf(pitch),
                    math::sinf(pitch),
                ),
                glam::Vec3::Z,
            )
        };
        let (yaw, pitch) = (0.2f32, -0.42f32);
        let previous = projection * view_of(yaw - 0.0006, pitch - 0.0003);
        let view_proj = projection * view_of(yaw, pitch);
        let jitter = [0.3 / 1286.0, -0.2 / 724.0];
        let m = reprojection(previous, view_proj, jitter);
        // A point straight ahead and a little to the side, three kilometers out.
        let drawn = Mat4::from_translation(glam::Vec3::new(jitter[0], jitter[1], 0.0)) * view_proj;
        let forward = glam::Vec3::new(
            -math::sinf(yaw) * math::cosf(pitch),
            math::cosf(yaw) * math::cosf(pitch),
            math::sinf(pitch),
        );
        let point = eye + forward * 3000.0 + glam::Vec3::new(200.0, 0.0, 0.0);
        let c = drawn * point.extend(1.0);
        let (ndc, depth) = (glam::Vec2::new(c.x / c.w, c.y / c.w), c.z / c.w);
        // Unjittered, as the exact sum takes it.
        let exact = exact_last(
            eye.as_dvec3(),
            (f64::from(yaw), f64::from(pitch)),
            previous.as_dmat4(),
            (ndc - glam::Vec2::from(jitter)).as_dvec2(),
            projection.as_dmat4(),
        );
        for steps in -4i32..=4 {
            let d = f32::from_bits(depth.to_bits().wrapping_add_signed(steps));
            let l = m * glam::Vec4::new(ndc.x, ndc.y, d, 1.0);
            let found = glam::DVec2::new(f64::from(l.x / l.w), f64::from(l.y / l.w));
            // Clip space is two units high over 1080 pixels.
            let off = (found - exact).length() * 540.0;
            assert!(
                off < 0.03,
                "{steps} steps of depth off: {off} pixels from where it was"
            );
        }
    }

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
}
