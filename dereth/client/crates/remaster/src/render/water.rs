//! The water pass: the water drawn over the resolved world, where the ground is water, with its
//! ripples, the rings round the bodies standing in it, its reflection and the sun's glint; and, on
//! a rainy day, the puddles flat ground gathers, as their own switch.
//!
//! The reflection is traced through the scene's acceleration structures where the device traces
//! rays and the reflections are asked to be traced; otherwise it is found on screen. Either way,
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
    BufferUsage, ColorAttachment, ColorTarget, Cull, Device, Format, GraphicsDesc, Load, Pipeline,
    RasterPass, Recorder, Rect, SamplerDesc, Texture, TextureDesc, TextureUsage, Touch, Use,
};
use glam::Vec3;

use super::drawn::DrawnPlan;
use super::light::Rows;
use super::ripples;
use super::rt::RtPlan;
use super::scene::NONE;
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
    /// Whether the bodies standing in the water ring it.
    pub rings: bool,
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
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            on: false,
            puddles: false,
            waves: 1.0,
            rings: true,
            check: false,
            absorption: 1.0,
            traced: false,
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

/// The most bodies the water rings round.
pub const WADERS: usize = 8;
/// The most rings the water draws at once, and how long a ring lasts, seconds.
pub const RINGS: usize = 32;
const RING_AGE: f64 = 3.0;
/// How often a body sends out a ring, seconds: walking, and standing still.
const RING_EVERY_MOVING: f64 = 0.22;
const RING_EVERY_STILL: f64 = 1.4;
/// The speed under which a body stands still, metres a second.
const STILL: f32 = 0.3;

/// A ring a body sent out over the water: where it stood, when, and how strongly.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ring {
    at: Vec3,
    born: f64,
    strength: f32,
}

/// The rings the bodies within reach send out at clock `now`, each at most every so often as it
/// walks or stands, the rings older than [`RING_AGE`] let go and the oldest past [`RINGS`]; a
/// clock that runs back starts them again.
fn send_rings(
    rings: &mut Vec<Ring>,
    sent: &mut Vec<(crate::scene::ObjectKey, f64)>,
    bodies: &[(crate::scene::ObjectKey, Vec3, f32)],
    now: f64,
) {
    if rings.iter().any(|r| r.born > now) || sent.iter().any(|s| s.1 > now) {
        rings.clear();
        sent.clear();
    }
    for &(key, at, speed) in bodies {
        let every = if speed > STILL {
            RING_EVERY_MOVING
        } else {
            RING_EVERY_STILL
        };
        let last = sent.iter().position(|s| s.0 == key);
        if last.is_some_and(|k| now - sent[k].1 < every) {
            continue;
        }
        match last {
            Some(k) => sent[k].1 = now,
            None => sent.push((key, now)),
        }
        rings.push(Ring {
            at,
            born: now,
            strength: 0.25 + (speed / 2.0).min(1.0),
        });
    }
    rings.retain(|r| now - r.born < RING_AGE);
    sent.retain(|s| now - s.1 < RING_AGE);
    if rings.len() > RINGS {
        rings.drain(..rings.len() - RINGS);
    }
}

/// The constants' row the traced reflection's words start at, after the bodies'.
const TRACE_ROW: usize = 11 + WADERS;
/// How far from the eye a body stands for the water to ring round it, metres.
const WADERS_WITHIN: f32 = 60.0;
/// How long the clock runs before it starts again, seconds: the puddles' waves and the rings
/// repeat over it, and it keeps the clock's fraction precise.
const CLOCK_PERIOD: f64 = 3600.0;
/// A landscape block's side, meters.
const BLOCK: f64 = 192.0;

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
    /// Where each body stood last frame, by its place among the frame's objects' keys, and the
    /// clock then.
    last: Vec<(crate::scene::ObjectKey, Vec3)>,
    last_clock: f64,
    /// The rings the bodies have sent out, the oldest first, and when each body last sent one.
    rings: Vec<Ring>,
    sent: Vec<(crate::scene::ObjectKey, f64)>,
}

/// One frame of the pass, as [`WaterPass::prepare`] made it.
#[derive(Debug, Clone, Copy)]
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
    probe: Option<[Texture; 6]>,
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
    /// The sky all round the eye, its six pictures, and the sampler they are read with, where it
    /// is drawn.
    pub probe: Option<([(Texture, u32); 6], u32)>,
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
                vertex: shader(Entry::FullscreenVsMain),
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
            last: Vec::new(),
            last_clock: 0.0,
            rings: Vec::new(),
            sent: Vec::new(),
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

    /// The bodies the water rings round: each object's feet (its lowest part) and how fast it
    /// moves, the nearest the eye first, within [`WADERS_WITHIN`].
    fn waders(&mut self, frame: &FrameDelta) -> Vec<(crate::scene::ObjectKey, [f32; 4])> {
        let dt = (frame.clock - self.last_clock).max(1e-3);
        let eye = frame.camera.eye;
        let mut now = Vec::with_capacity(frame.objects.len());
        let mut out: Vec<(f32, (crate::scene::ObjectKey, [f32; 4]))> = Vec::new();
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
            now.push((o.key, at));
            let near = at.distance(eye);
            if near > WADERS_WITHIN {
                continue;
            }
            let was = self
                .last
                .iter()
                .find(|(k, _)| *k == o.key)
                .map_or(at, |(_, p)| *p);
            #[allow(clippy::cast_possible_truncation)] // a frame's seconds
            let speed = (at - was).truncate().length() / dt as f32;
            out.push((near, (o.key, [at.x, at.y, at.z, speed.min(20.0)])));
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.last = now;
        self.last_clock = frame.clock;
        out.into_iter().take(WADERS).map(|(_, w)| w).collect()
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
        let waders = self.waders(frame);
        let bodies: Vec<_> = waders
            .iter()
            .map(|(k, w)| (*k, Vec3::new(w[0], w[1], w[2]), w[3]))
            .collect();
        send_rings(&mut self.rings, &mut self.sent, &bodies, frame.clock);
        let waders: Vec<[f32; 4]> = waders.into_iter().map(|(_, w)| w).collect();
        if !s.on {
            if let Some((texture, _)) = self.ripples.take() {
                gfx.destroy_texture(texture);
            }
        }
        if !s.runs() {
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
        let global = [
            f64::from(frame.origin[0]) * BLOCK,
            f64::from(frame.origin[1]) * BLOCK,
        ];
        let mut rows = Rows::default();
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        // The clock within its period, and a few bodies.
        {
            rows.0.push(
                [
                    frame.clock.rem_euclid(CLOCK_PERIOD) as f32,
                    s.waves.max(0.0),
                    if s.rings { 1.0 } else { 0.0 },
                    if s.puddles && rainy { 1.0 } else { 0.0 },
                ]
                .map(f32::to_bits),
            );
            rows.0.push(
                [
                    waders.len() as f32,
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
        for k in 0..WADERS {
            rows.0
                .push(waders.get(k).map_or([0; 4], |w| w.map(f32::to_bits)));
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
        rows.0.push([inputs.heights.map_or(NONE, |h| h.0), 0, 0, 0]);
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
        #[allow(clippy::cast_possible_truncation)] // a ring's age, seconds
        {
            rows.0
                .push([u32::try_from(self.rings.len()).unwrap_or(0), 0, 0, 0]);
            for r in &self.rings {
                let age = (frame.clock - r.born) as f32;
                rows.0.push([r.at.x, r.at.y, r.at.z, age].map(f32::to_bits));
                rows.0.push([r.strength, 0.0, 0.0, 0.0].map(f32::to_bits));
            }
        }
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
            probe: inputs.probe.map(|(f, _)| f.map(|t| t.0)),
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
        for face in self.probe.iter().flatten() {
            touches.push(Touch::Texture(*face, Use::Sampled));
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

    /// A body walking sends out a ring every few frames, one standing now and then; the rings
    /// go as they age, the oldest first past the most the water draws; and a clock run back
    /// starts them again.
    #[test]
    fn a_walking_body_sends_out_rings_often_and_a_standing_one_seldom() {
        use crate::scene::ObjectKey;
        let (mut rings, mut sent) = (Vec::new(), Vec::new());
        let walker = (ObjectKey::Body, Vec3::ZERO, 1.5);
        let stander = (ObjectKey::Server(7), Vec3::new(5.0, 0.0, 0.0), 0.0);
        for k in 0..60 {
            let now = f64::from(k) / 30.0;
            send_rings(&mut rings, &mut sent, &[walker, stander], now);
        }
        let of = |x: f32| rings.iter().filter(|r: &&Ring| r.at.x == x).count();
        assert!(of(0.0) >= 8, "walking: {}", of(0.0));
        assert!((1..=2).contains(&of(5.0)), "standing: {}", of(5.0));
        assert!(rings.iter().all(|r| 59.0 / 30.0 - r.born < RING_AGE));
        for k in 0..400 {
            let now = 2.0 + f64::from(k) / 30.0;
            send_rings(&mut rings, &mut sent, &[walker, stander], now);
            assert!(rings.len() <= RINGS);
        }
        assert!(rings.iter().all(|r| r.born > 2.0), "the oldest go");
        send_rings(&mut rings, &mut sent, &[], 0.0);
        assert!(rings.is_empty(), "a clock run back starts them again");
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
