//! The water pass: the water drawn over the resolved world, where the ground is water, with its
//! ripples, its reflection and the sun's glint; and, on a rainy day, the puddles flat ground
//! gathers, as their own switch. Near the eye the water is simulated ([`super::wake`]), so the
//! bodies wading in it leave a wake.
//!
//! The reflection is traced through the scene's acceleration structures where the device traces
//! rays and the reflections are asked to be traced; otherwise, on the water nearest the eye, it is
//! the scene drawn mirrored in that water's level ([`super::planar`]), and elsewhere it is found on
//! screen. Either way,
//! what lies past what it finds is the land far off, from the world's heights, and the sky, from
//! the sky's own tables.
//!
//! It runs after the resolve. The resolved world is copied, and the pass draws over the scene's
//! colors where there is water, reading the copy. Off, it draws nothing and the frame is the one
//! drawn without it.
//!
//! Its shader is `shaders/water.slang`; its pipelines are made with it ([`WaterPass::new`]) and
//! never after.

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ComputeDesc, Cull, Device,
    Format, GraphicsDesc, Load, Location, Pipeline, RasterPass, Recorder, Rect, SamplerDesc,
    Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::Vec3;

use super::drawn::DrawnPlan;
use super::light::Rows;
use super::ripples;
use super::rt::RtPlan;
use super::scene::NONE;
use super::wake::{self, Wake};
use super::{push_words, shader, PerFrame, Targets};
use crate::scene::FrameDelta;
use crate::shaders::Entry;

/// How the water is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterSettings {
    /// Whether the water is drawn as water. Off, it is the ground retail draws.
    pub on: bool,
    /// Whether flat ground gathers puddles on a rainy day: a switch of its own, apart from the
    /// water's.
    pub puddles: bool,
    /// How strong the ripples are, 1 as they are made.
    pub waves: f32,
    /// Whether the water near the eye is simulated, so the bodies wading in it leave a wake of
    /// ripples there.
    pub wake: bool,
    /// To check the reflection: each water pixel shows what its reflection found, red a surface
    /// it met, green the land, blue the sky.
    pub check: bool,
    /// How strongly the water takes the light of what is seen through it where the bed is drawn
    /// under it, fading it toward the water's own deep color with the length of water the view
    /// runs through: 1 as made, 0 not at all.
    pub absorption: f32,
    /// Whether the reflection is traced through the scene, where the device traces rays; found
    /// on screen otherwise.
    pub traced: bool,
    /// Where it is not traced, whether the scene is drawn mirrored in the water nearest the eye
    /// for the water to reflect, where its surface lies at that water's level.
    pub planar: bool,
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            on: false,
            puddles: false,
            waves: 1.0,
            wake: true,
            check: false,
            absorption: 1.0,
            traced: false,
            planar: false,
        }
    }
}

impl WaterSettings {
    /// Whether the pass runs: the water or the puddles drawn.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.on || self.puddles
    }
}

/// The constants' row the traced reflection's words start at, after the ripples' layers'.
const TRACE_ROW: usize = 11;
/// How far from the eye a body stands for it to stir the water, meters.
const STIR_WITHIN: f32 = 60.0;
/// How long the clock runs before it starts again, seconds: the puddles' waves repeat over it,
/// and it keeps the clock's fraction precise.
const CLOCK_PERIOD: f64 = 3600.0;
/// A landscape block's side, meters.
const BLOCK: f64 = 192.0;
/// How near the water's edge the player's own body stands for the wake to follow it, meters.
const OWN_NEAR: f32 = 8.0;

/// Whether world point `(x, y)`, meters, lies in the water or within [`OWN_NEAR`] of its edge, by
/// `world`'s vertices: how far each lies inside the water, blended across the cell. The edge the
/// ground draws wanders a few meters from that blend, which the margin takes in.
fn near_water(world: &crate::scene::DrawnWorld, x: f64, y: f64) -> bool {
    let side = world.side as usize;
    if side < 2 || world.water.len() != side * side {
        return false;
    }
    let spacing = f64::from(world.grid.vertex_spacing());
    let (qx, qy) = (x / spacing, y / spacing);
    let (cx, cy) = (qx.floor(), qy.floor());
    // LINT-OK: the world's vertices along a side, a few thousand.
    #[allow(clippy::cast_precision_loss)]
    let last = (side - 2) as f64;
    if !(0.0..=last).contains(&cx) || !(0.0..=last).contains(&cy) {
        return false;
    }
    // LINT-OK: a cell within the world, checked above; where in it, 0..1.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let ((i, j), (fx, fy)) = (
        (cx as usize, cy as usize),
        ((qx - cx) as f32, (qy - cy) as f32),
    );
    let at = |dx: usize, dy: usize| {
        dereth_primitives::half::value(world.water[(j + dy) * side + i + dx][0])
    };
    let inside = (at(0, 0) * (1.0 - fx) + at(1, 0) * fx) * (1.0 - fy)
        + (at(0, 1) * (1.0 - fx) + at(1, 1) * fx) * fy;
    inside > -OWN_NEAR
}
/// The constants' row the wake's start at, after the mirrored scene's, and its steps' after its
/// kernel's.
const WAKE_ROW: usize = 24;
const WAKE_STEPS_ROW: usize = WAKE_ROW + 2 + wake::KERNEL_ROWS;
/// The bytes of the wake's cells: four floats each.
const WAKE_BYTES: u64 = (wake::SIDE as u64) * (wake::SIDE as u64) * 16;

/// The copy of the resolved world the pass reads, made again when the scene's colors change size
/// or format.
#[derive(Debug, Clone, Copy)]
struct Copy {
    texture: Texture,
    index: u32,
    format: Format,
    width: u32,
    height: u32,
}

/// The water pass.
#[derive(Debug)]
pub struct WaterPass {
    /// For colors in display values, and for the post passes' linear colors.
    pipelines: [(Format, Pipeline); 2],
    /// The same, with the reflection traced through the scene: made only where the device traces
    /// rays.
    traced: Option<[(Format, Pipeline); 2]>,
    settings: WaterSettings,
    constants: PerFrame,
    copy: Option<Copy>,
    /// The ripples' picture and its index, held while the water is drawn.
    ripples: Option<(Texture, u32)>,
    /// Whether the water is drawn with no ripples' picture, for a check.
    pub(crate) without_picture: bool,
    /// The index of the sampler the ripples' picture repeats under.
    wrap: u32,
    /// The wake near the eye: its state, its step's pipeline, its kernel's rows, its two sets of
    /// cells and their indices while it runs, and which of them holds the water as it stands.
    wake: Wake,
    wake_pipeline: Pipeline,
    wake_kernel: Option<[[u32; 4]; wake::KERNEL_ROWS]>,
    wake_cells: Option<[(Buffer, u32); 2]>,
    wake_current: usize,
}

/// The wake's steps of one frame: its step's pipeline, each step's cells read and written with
/// their indices, in order, and the cells the water is drawn from.
#[derive(Debug, Clone)]
struct WakeSteps {
    pipeline: Pipeline,
    steps: Vec<[(Buffer, u32); 2]>,
    drawn: Buffer,
}

/// One frame of the pass, as [`WaterPass::prepare`] made it.
#[derive(Debug, Clone)]
pub struct WaterPlan {
    pipeline: Pipeline,
    copy: Copy,
    constants: u32,
    ripples: Option<(Texture, u32)>,
    wrap: u32,
    /// Whether the frame's reflection is traced, and the textures and buffers it reads besides the
    /// frame's targets: the sky-view table and the world's heights.
    traced: bool,
    sky_view: Option<Texture>,
    heights: Option<dereth_gfx::Buffer>,
    shore: Option<dereth_gfx::Buffer>,
    probe: Option<[Texture; 6]>,
    planar: Option<Texture>,
    wake: Option<WakeSteps>,
}

/// What the water reads besides the frame's targets, as the frame has it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WaterInputs<'a> {
    /// The scene's acceleration structures, where the frame built them.
    pub rt: Option<&'a RtPlan>,
    /// The scene's indices, vertices and materials, the sampler textures are read with, its
    /// instances, and its ground's vertices and cells, by their indices in the table.
    pub pools: [u32; 7],
    /// The sky's constants, its sky-view table and its sampler, and the table, where the sky's
    /// tables are drawn.
    pub sky: Option<([u32; 3], Texture)>,
    /// The world's vertex heights and their buffer, where the drawn ground is drawn.
    pub heights: Option<(u32, dereth_gfx::Buffer)>,
    /// Where the water lies about each of the world's vertices and their buffer, where the drawn
    /// ground is drawn: what the water's level is found from.
    pub shore: Option<(u32, dereth_gfx::Buffer)>,
    /// The sky all round the eye, its six pictures, and the sampler they are read with, where it
    /// is drawn.
    pub probe: Option<([(Texture, u32); 6], u32)>,
    /// The scene mirrored in the water, where it is drawn: its colors and their index, the
    /// water's level it is mirrored in, and its view and projection; and the sampler it is read
    /// with.
    pub planar: Option<(Texture, u32, f32, glam::Mat4)>,
    pub planar_sampler: u32,
}

impl WaterPass {
    /// The pass on `gfx`, its pipelines made, off: for the scene's colors in display values, and
    /// in the post passes' linear format.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let linear = super::post::scene_format(gfx);
        let make_traced = gfx.caps().ray_queries;
        let mut make = |format, entry| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster water",
                vertex: dereth_gfx_passes::shaders::fullscreen(),
                fragment: Some(shader(entry)),
                color: &[ColorTarget {
                    format,
                    blend: None,
                    write_alpha: false,
                }],
                depth: None,
                cull: Cull::None,
            })
            .map(|p| (format, p))
        };
        let pipelines = [
            make(super::COLOR, Entry::WaterFsMain)?,
            make(linear, Entry::WaterFsMain)?,
        ];
        let traced = if make_traced {
            Some([
                make(super::COLOR, Entry::WaterFsTraced)?,
                make(linear, Entry::WaterFsTraced)?,
            ])
        } else {
            None
        };
        let wrap = gfx.create_sampler(&SamplerDesc {
            filter: dereth_gfx::Filter::Linear,
            mip: dereth_gfx::Filter::Linear,
            address: dereth_gfx::Address::Repeat,
            address_v: dereth_gfx::Address::Repeat,
            anisotropy: 8,
            lod_bias: 0.0,
        })?;
        let wrap = gfx.bind_sampler(wrap)?;
        let wake_pipeline = gfx.create_compute_pipeline(&ComputeDesc {
            label: "remaster water wake",
            shader: shader(Entry::WaterCsWake),
        })?;
        Ok(Self {
            pipelines,
            traced,
            settings: WaterSettings::default(),
            constants: PerFrame::new(
                gfx,
                "remaster water constants",
                2048,
                BufferUsage::default(),
            )?,
            copy: None,
            ripples: None,
            without_picture: false,
            wrap,
            wake: Wake::default(),
            wake_pipeline,
            wake_kernel: None,
            wake_cells: None,
            wake_current: 0,
        })
    }

    /// How the water is drawn from the next frame on.
    pub fn set_settings(&mut self, settings: WaterSettings) {
        self.settings = settings;
    }

    /// How the water is drawn.
    #[must_use]
    pub fn settings(&self) -> WaterSettings {
        self.settings
    }

    /// Whether it asks for the scene's acceleration structures: drawn, its reflection asked to be
    /// traced, where the device traces rays.
    #[must_use]
    pub fn wants_rays(&self) -> bool {
        self.settings.runs() && self.settings.traced && self.traced.is_some()
    }

    /// Where the viewer's own body stands: its feet, the frame's space.
    fn viewer(frame: &FrameDelta) -> Option<Vec3> {
        let o = frame
            .objects
            .iter()
            .find(|o| o.key == crate::scene::ObjectKey::Body)?;
        let first = o.parts.first()?;
        let low = o
            .parts
            .iter()
            .map(|p| p.transform.w_axis.z)
            .fold(f32::INFINITY, f32::min);
        Some(Vec3::new(
            first.transform.w_axis.x,
            first.transform.w_axis.y,
            low,
        ))
    }

    /// The bodies that stir the water: each object's feet (its lowest part), the nearest the eye
    /// first, within [`STIR_WITHIN`], [`wake::BODIES`] at the most.
    fn bodies(frame: &FrameDelta) -> Vec<(crate::scene::ObjectKey, Vec3)> {
        let eye = frame.camera.eye;
        let mut out: Vec<(f32, (crate::scene::ObjectKey, Vec3))> = Vec::new();
        for o in &frame.objects {
            let Some(first) = o.parts.first() else {
                continue;
            };
            let low = o
                .parts
                .iter()
                .map(|p| p.transform.w_axis.z)
                .fold(f32::INFINITY, f32::min);
            let at = Vec3::new(first.transform.w_axis.x, first.transform.w_axis.y, low);
            let near = at.distance(eye);
            if near <= STIR_WITHIN {
                out.push((near, (o.key, at)));
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().take(wake::BODIES).map(|(_, b)| b).collect()
    }

    /// The frame's constants and the copy of the resolved world, and the plan the pass is
    /// recorded from; `None` when the pass is off. `linear` is whether the scene's colors are
    /// drawn by the post passes this frame; `rainy` whether it is a rainy day; `inputs` what it
    /// reads besides the frame's targets.
    ///
    /// # Errors
    /// What the device refuses.
    pub(crate) fn prepare(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
        t: &Targets,
        (linear, rainy): (bool, bool),
        inputs: &WaterInputs<'_>,
    ) -> Result<Option<WaterPlan>, dereth_gfx::Error> {
        let s = self.settings;
        let scene = if linear {
            super::post::scene_format(gfx)
        } else {
            super::COLOR
        };
        let bodies = Self::bodies(frame);
        let global = [
            f64::from(frame.origin[0]) * BLOCK,
            f64::from(frame.origin[1]) * BLOCK,
        ];
        if !s.on {
            if let Some((texture, _)) = self.ripples.take() {
                gfx.destroy_texture(texture);
            }
        }
        // The wake runs where the water is drawn and the drawn ground says where it lies.
        let wakes = s.on && s.wake && inputs.heights.is_some() && inputs.shore.is_some();
        let wake_plan = if wakes {
            let own_wet = frame.drawn.as_ref().is_some_and(|d| {
                bodies.iter().any(|(k, b)| {
                    *k == crate::scene::ObjectKey::Body
                        && near_water(
                            &d.world,
                            f64::from(b.x) + global[0],
                            f64::from(b.y) + global[1],
                        )
                })
            });
            Some(self.wake.plan(&wake::Stir {
                clock: frame.clock,
                global,
                eye: frame.camera.eye,
                bodies: &bodies,
                own_wet,
            }))
        } else {
            self.wake = Wake::default();
            if let Some(cells) = self.wake_cells.take() {
                for (b, _) in cells {
                    gfx.destroy_buffer(b);
                }
            }
            None
        };
        // Only where the frame draws the outdoors: past an opening that shows nothing, retail draws
        // nothing.
        if !s.runs() || !frame.landscape {
            return Ok(None);
        }
        let rt = inputs.rt.filter(|_| s.traced && self.traced.is_some());
        let pipelines = match (rt, self.traced.as_ref()) {
            (Some(_), Some(traced)) => traced,
            _ => &self.pipelines,
        };
        let Some(&(_, pipeline)) = pipelines.iter().find(|(f, _)| *f == scene) else {
            return Ok(None);
        };
        let copy = match self.copy {
            Some(c) if (c.format, c.width, c.height) == (scene, t.width, t.height) => c,
            old => {
                if let Some(c) = old {
                    gfx.destroy_texture(c.texture);
                }
                let texture = gfx.create_texture(&TextureDesc {
                    label: "remaster water copy",
                    width: t.width,
                    height: t.height,
                    mips: 1,
                    format: scene,
                    usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                })?;
                let c = Copy {
                    texture,
                    index: gfx.bind_texture(texture)?,
                    format: scene,
                    width: t.width,
                    height: t.height,
                };
                self.copy = Some(c);
                c
            }
        };
        if s.on && self.ripples.is_none() {
            let levels = ripples::picture();
            let texture = gfx.create_texture(&TextureDesc {
                label: "remaster water ripples",
                width: ripples::SIDE,
                height: ripples::SIDE,
                mips: u32::try_from(levels.len()).unwrap_or(1),
                format: Format::Rgba8Unorm,
                usage: TextureUsage::SAMPLED,
            })?;
            for (k, level) in levels.iter().enumerate() {
                gfx.upload_texture(texture, u32::try_from(k).unwrap_or(0), level)?;
            }
            self.ripples = Some((texture, gfx.bind_texture(texture)?));
        }
        let mut rows = Rows::default();
        #[allow(clippy::cast_possible_truncation)]
        // The clock within its period.
        {
            rows.0.push(
                [
                    frame.clock.rem_euclid(CLOCK_PERIOD) as f32,
                    s.waves.max(0.0),
                    0.0,
                    if s.puddles && rainy { 1.0 } else { 0.0 },
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    0.0,
                    if s.on { 1.0 } else { 0.0 },
                    if s.check { 1.0 } else { 0.0 },
                    s.absorption.max(0.0),
                ]
                .map(f32::to_bits),
            );
            rows.0
                .push([global[0] as f32, global[1] as f32, 0.0, 0.0].map(f32::to_bits));
        }
        for l in &ripples::LAYERS {
            let (sin, cos) = dereth_primitives::num::math::sin_cosf(l.turn);
            rows.0
                .push([l.repeat, l.strength, cos, sin].map(f32::to_bits));
            let [across, down] = ripples::moved(l, global, frame.clock);
            rows.0.push([across, down, 0.0, 0.0].map(f32::to_bits));
        }
        debug_assert_eq!(rows.0.len(), TRACE_ROW);
        let [indices, vertices, materials, sampler, instances, ground_vertices, ground_cells] =
            inputs.pools;
        rows.0.push(rt.map_or([NONE; 4], |r| {
            [r.index, r.static_rows, r.object_rows, indices]
        }));
        rows.0
            .push(rt.map_or([NONE; 4], |_| [vertices, materials, sampler, instances]));
        rows.0.push(rt.map_or([NONE; 4], |r| {
            [
                ground_vertices,
                ground_cells,
                r.static_entries,
                r.object_entries,
            ]
        }));
        let [a, view, view_sampler] = inputs.sky.map_or([NONE; 3], |s| s.0);
        rows.0
            .push([rt.map_or(0, |r| r.statics), a, view, view_sampler]);
        rows.0.push([
            inputs.heights.map_or(NONE, |h| h.0),
            inputs.shore.map_or(NONE, |h| h.0),
            0,
            0,
        ]);
        rows.0.push(
            Self::viewer(frame)
                .map_or([0.0; 4], |v| [v.x, v.y, v.z, 1.0])
                .map(f32::to_bits),
        );
        let faces = inputs.probe.map(|(f, _)| f.map(|t| t.1));
        rows.0
            .push(faces.map_or([NONE; 4], |f| [f[0], f[1], f[2], f[3]]));
        rows.0.push([
            faces.map_or(NONE, |f| f[4]),
            faces.map_or(NONE, |f| f[5]),
            inputs.probe.map_or(NONE, |p| p.1),
            0,
        ]);
        // The scene mirrored in the water: its colors, their sampler, and the level; then its
        // view and projection.
        match inputs.planar {
            Some((_, index, level, view_projection)) => {
                rows.0
                    .push([index, inputs.planar_sampler, level.to_bits(), 1]);
                for c in view_projection.to_cols_array_2d() {
                    rows.0.push(c.map(f32::to_bits));
                }
            }
            None => {
                rows.0.push([NONE, NONE, 0, 0]);
                rows.0.extend([[0; 4]; 4]);
            }
        }
        let wake = self.wake_rows(gfx, &mut rows, wake_plan)?;
        self.constants.write(gfx, &rows.bytes())?;
        Ok(Some(WaterPlan {
            pipeline,
            copy,
            constants: self.constants.index,
            ripples: self.ripples.filter(|_| !self.without_picture),
            wrap: self.wrap,
            traced: rt.is_some(),
            sky_view: inputs.sky.map(|s| s.1),
            heights: inputs.heights.map(|h| h.1),
            shore: inputs.shore.map(|h| h.1),
            probe: inputs.probe.map(|(f, _)| f.map(|t| t.0)),
            planar: inputs.planar.map(|p| p.0),
            wake,
        }))
    }

    /// The wake's rows, after the mirrored scene's: the cells the water is drawn from and the square they
    /// hold, the kernel, and each of the frame's steps; and the steps to record. Its cells are
    /// made the first time it runs.
    fn wake_rows(
        &mut self,
        gfx: &mut Device,
        rows: &mut Rows,
        plan: Option<wake::WakePlan>,
    ) -> Result<Option<WakeSteps>, dereth_gfx::Error> {
        debug_assert_eq!(rows.0.len(), WAKE_ROW);
        let Some(plan) = plan else {
            rows.0.push([NONE, 0, 0, 0]);
            rows.0.push([0; 4]);
            return Ok(None);
        };
        let cells = match self.wake_cells {
            Some(c) => c,
            None => {
                let mut make = |label| -> Result<(Buffer, u32), dereth_gfx::Error> {
                    let b = gfx.create_buffer(&BufferDesc {
                        label,
                        size: WAKE_BYTES,
                        usage: BufferUsage::STORAGE,
                        location: Location::Gpu,
                    })?;
                    Ok((b, gfx.bind_buffer(b)?))
                };
                let c = [
                    make("remaster water wake a")?,
                    make("remaster water wake b")?,
                ];
                self.wake_cells = Some(c);
                self.wake_current = 0;
                c
            }
        };
        let mut steps = Vec::with_capacity(plan.steps.len());
        for _ in &plan.steps {
            let from = self.wake_current;
            steps.push([cells[from], cells[1 - from]]);
            self.wake_current = 1 - from;
        }
        let drawn = cells[self.wake_current];
        match plan.square {
            Some((first, corner)) => {
                #[allow(clippy::cast_sign_loss)] // signed world cells, as the shader reads them
                rows.0.push([drawn.1, first[0] as u32, first[1] as u32, 0]);
                rows.0
                    .push([corner[0], corner[1], 0.0, 0.0].map(f32::to_bits));
            }
            None => {
                rows.0.push([NONE, 0, 0, 0]);
                rows.0.push([0; 4]);
            }
        }
        let kernel = *self.wake_kernel.get_or_insert_with(wake::kernel_rows);
        rows.0.extend(kernel);
        debug_assert_eq!(rows.0.len(), WAKE_STEPS_ROW);
        for step in &plan.steps {
            rows.0.extend(step);
        }
        Ok(Some(WakeSteps {
            pipeline: self.wake_pipeline,
            steps,
            drawn: drawn.0,
        }))
    }
}

impl WaterPlan {
    /// Record the pass: the resolved world copied, then the water drawn over the scene's colors.
    /// `t` are the frame's targets as the scene's colors are drawn into them; `frame` the frame's
    /// constants' index; `outdoor_depth` the depth the outdoors are placed by; `sun_visibility`
    /// how much of the sun reaches each pixel (`NONE` and `None` where nothing says); `drawn` the
    /// drawn ground, where it is drawn: its terrain's colors give the water over its bed its hue.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn record(
        &self,
        r: &mut Recorder<'_>,
        t: &Targets,
        frame: u32,
        outdoor_depth: (Texture, u32),
        sun_visibility: (u32, Option<Texture>),
        drawn: Option<&DrawnPlan>,
    ) -> Result<(), dereth_gfx::Error> {
        if let Some(w) = self.wake.as_ref().filter(|w| !w.steps.is_empty()) {
            r.timestamp("water wake");
            let reads: Vec<Touch> = self
                .heights
                .iter()
                .chain(&self.shore)
                .map(|b| Touch::Buffer(*b, Use::StorageRead))
                .collect();
            for (k, [from, to]) in w.steps.iter().enumerate() {
                let mut touches = reads.clone();
                touches.push(Touch::Buffer(from.0, Use::StorageRead));
                touches.push(Touch::Buffer(to.0, Use::StorageWrite));
                // The water pass's own words, the wake's last: the cells read and written, and
                // which step it is.
                let mut words = [NONE; 16];
                words[0] = frame;
                words[1] = self.constants;
                words[13] = from.1;
                words[14] = to.1;
                words[15] = u32::try_from(k).unwrap_or(0);
                r.compute(&touches, |c| {
                    c.set_pipeline(w.pipeline)?;
                    c.push(0, &push_words(&words))?;
                    c.dispatch(wake::SIDE / 16, wake::SIDE / 16, 1);
                    Ok(())
                })?;
            }
        }
        r.timestamp(if self.traced { "water traced" } else { "water" });
        r.copy_texture(t.color, self.copy.texture)?;
        let mut touches = vec![
            Touch::Texture(self.copy.texture, Use::Sampled),
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.albedo, Use::Sampled),
        ];
        if let Some(s) = sun_visibility.1 {
            touches.push(Touch::Texture(s, Use::Sampled));
        }
        if let Some((ripples, _)) = self.ripples {
            touches.push(Touch::Texture(ripples, Use::Sampled));
        }
        if let Some(view) = self.sky_view {
            touches.push(Touch::Texture(view, Use::Sampled));
        }
        if let Some(heights) = self.heights {
            touches.push(Touch::Buffer(heights, Use::StorageRead));
        }
        if let Some(shore) = self.shore {
            touches.push(Touch::Buffer(shore, Use::StorageRead));
        }
        for face in self.probe.iter().flatten() {
            touches.push(Touch::Texture(*face, Use::Sampled));
        }
        if let Some(p) = self.planar {
            touches.push(Touch::Texture(p, Use::Sampled));
        }
        if let Some(w) = &self.wake {
            touches.push(Touch::Buffer(w.drawn, Use::StorageRead));
        }
        let (drawn_constants, words) = drawn.map_or((NONE, NONE), |d| {
            let (constants, words, buffers) = d.terrain_inputs();
            touches.extend(buffers.map(|b| Touch::Buffer(b, Use::StorageRead)));
            (constants, words)
        });
        r.raster(
            &RasterPass {
                name: "water",
                color: &[ColorAttachment {
                    texture: t.color,
                    load: Load::Keep,
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
                        self.constants,
                        self.copy.index,
                        t.index[4],
                        outdoor_depth.1,
                        t.index[3],
                        t.index[1],
                        sun_visibility.0,
                        t.index[0],
                        self.ripples.map_or(NONE, |(_, index)| index),
                        self.wrap,
                        drawn_constants,
                        words,
                    ]),
                )?;
                d.draw(3, 1, 0, 0);
                Ok(())
            },
        )
    }
}

impl super::Renderer {
    /// How the water is drawn from the next frame on.
    pub fn set_water(&mut self, settings: WaterSettings) {
        self.water.set_settings(settings);
    }

    /// How the water is drawn.
    #[must_use]
    pub fn water(&self) -> WaterSettings {
        self.water.settings()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// The player's own body counts as in or near the water inside it and a few meters out from
    /// its edge, and not farther out, nor off the world.
    #[test]
    fn the_players_own_body_is_near_the_water_inside_it_and_a_few_meters_out() {
        // Three vertices a side: the west column water 12 meters in, the rest land 12 meters
        // and 36 meters out.
        let world = crate::scene::DrawnWorld {
            side: 3,
            water: std::sync::Arc::new(
                [12.0, -12.0, -36.0]
                    .repeat(3)
                    .into_iter()
                    .map(|i| [dereth_primitives::half::bits(i), 0])
                    .collect(),
            ),
            ..Default::default()
        };
        assert!(near_water(&world, 2.0, 10.0), "in the water");
        assert!(near_water(&world, 15.0, 10.0), "a few meters out");
        assert!(!near_water(&world, 30.0, 10.0), "farther out");
        assert!(!near_water(&world, -1.0, 10.0) && !near_water(&world, 50.0, 10.0));
        assert!(!near_water(&crate::scene::DrawnWorld::default(), 2.0, 10.0));
    }

    /// Off, the pass draws nothing: neither the water nor the puddles; either alone runs it.
    #[test]
    fn the_water_and_the_puddles_are_switches_of_their_own() {
        let off = WaterSettings::default();
        assert!(!off.runs());
        let water = WaterSettings { on: true, ..off };
        let puddles = WaterSettings {
            puddles: true,
            ..off
        };
        assert!(water.runs() && !water.puddles);
        assert!(puddles.runs() && !puddles.on);
    }
}
