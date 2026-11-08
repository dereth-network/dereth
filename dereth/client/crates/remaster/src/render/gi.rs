//! Bounced light (pass 7b): the light the world's surfaces send on to each other, added to each
//! pixel's own by the resolve (`compose`'s `Masks::gi`), so no pixel comes out darker than with
//! the direct light alone. Its shaders are `shaders/gi.slang` and `shaders/gi_trace.slang`, which
//! say how the light is found.
//!
//! **Two ways, as the device can.** With ray queries ([`dereth_gfx::Caps::ray_queries`]) and
//! [`GiSettings::rays`], a grid of probes round the eye outdoors and a grid of its own in each room
//! near the eye, traced against the scene's acceleration structures (pass 1b) and kept over the
//! frames; otherwise rays from each pixel through the G-buffer's depth, blurred.
//!
//! **Off, it is not there:** no target, no buffer, and the resolve's masks as before.

use std::collections::BTreeMap;

use dereth_gfx::{
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, ComputeDesc, Cull, Device,
    Format, GraphicsDesc, Load, Location, Pipeline, RasterPass, Recorder, Rect, Texture,
    TextureDesc, TextureUsage, Touch, Use,
};
use glam::{Mat3, Vec3};

use super::light::Rows;
use super::rt::RtPlan;
use super::scene::GpuScene;
use super::{push_words, shader, PerFrame, Targets};
use crate::scene::FrameDelta;
use crate::shaders::Entry;

/// How the bounced light is found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiSettings {
    /// Whether the pass runs. Off, no bounced light is added.
    pub on: bool,
    /// Find it with probes traced against the scene where the device traces rays; otherwise, and
    /// where it cannot, from the screen.
    pub rays: bool,
    /// How much of what is found is added: 1 as found.
    pub strength: f32,
    /// The share of the bounced light already at a surface that it sends on again.
    pub bounce: f32,
    /// How much of what a probe kept it keeps each frame, 0 to 1.
    pub hysteresis: f32,
    /// The rays each pixel casts from the screen, and how far they reach, meters.
    pub screen_rays: u32,
    pub screen_reach: f32,
}

impl Default for GiSettings {
    fn default() -> Self {
        Self {
            on: false,
            rays: true,
            strength: 1.0,
            bounce: 0.8,
            hysteresis: 0.95,
            screen_rays: 8,
            screen_reach: 4.0,
        }
    }
}

impl GiSettings {
    /// The settings that run the pass, everything else as it comes.
    #[must_use]
    pub fn on() -> Self {
        Self {
            on: true,
            ..Self::default()
        }
    }
}

/// The outdoor grid's size in probes, and its spacing across and up, meters.
pub const GRID: [i32; 3] = [32, 32, 8];
pub const SPACING: [f32; 2] = [4.0, 3.0];
/// The rays a probe casts each frame.
pub const RAYS: u32 = 64;
/// The rooms that get probes of their own at once, the most probes each, and their spacing.
pub const ROOMS: usize = 16;
const ROOM_SLOTS: u32 = 16;
pub const ROOM_PROBES: u32 = 256;
pub const ROOM_SPACING: f32 = 2.0;

/// The probes outdoors.
#[allow(clippy::cast_sign_loss)] // a positive constant
const OUTDOOR: u32 = (GRID[0] * GRID[1] * GRID[2]) as u32;
/// Every probe: the outdoor ones, then each room slot's.
const PROBES: u32 = OUTDOOR + ROOM_SLOTS * ROOM_PROBES;
/// The bytes of one probe (`gi.slang`'s twelve rows).
const PROBE_BYTES: u64 = 12 * 16;
/// The farthest a probe's ray runs, meters.
const FAR: f32 = 120.0;

/// The pass's target's format.
const GI: Format = Format::Rgba16Float;

/// The pass's targets, at the world's size: the light, and the screen's two before it is blurred.
#[derive(Debug, Clone, Copy)]
struct GiTargets {
    width: u32,
    height: u32,
    gi: Texture,
    raw: Texture,
    across: Texture,
    /// Each one's index in the table, in that order.
    index: [u32; 3],
}

/// The probes' memory: their light and distances, and their rays.
#[derive(Debug, Clone, Copy)]
struct ProbeBuffers {
    probes: Buffer,
    rays: Buffer,
    index: [u32; 2],
}

/// One room's probes: its slot and the serial they were laid out with.
#[derive(Debug, Clone, Copy)]
struct RoomSlot {
    slot: usize,
    serial: u32,
}

/// The bounced light pass.
#[derive(Debug)]
pub struct GiPass {
    settings: GiSettings,
    update: Pipeline,
    probes_pipeline: Pipeline,
    screen: Pipeline,
    blur: Pipeline,
    /// The probes' rays, where the device traces rays.
    trace: Option<Pipeline>,
    constants: Option<PerFrame>,
    buffers: Option<ProbeBuffers>,
    targets: Option<GiTargets>,
    rooms: BTreeMap<u32, RoomSlot>,
    serial: u32,
    frame: u32,
}

/// One frame of the pass, as [`GiPass::prepare`] made it.
#[derive(Debug, Clone, Copy)]
pub struct GiPlan {
    targets: GiTargets,
    constants: u32,
    /// The probes' way: its pipelines, buffers and the scene's structures; `None` for the
    /// screen's.
    probes: Option<(Pipeline, Pipeline, Pipeline, ProbeBuffers, RtPlan)>,
    screen: (Pipeline, Pipeline),
    settings: GiSettings,
}

impl GiPass {
    /// The pass on `gfx`, its pipelines made, off.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn new(gfx: &mut Device) -> Result<Self, dereth_gfx::Error> {
        let fragment = |gfx: &mut Device, e: Entry| {
            gfx.create_graphics_pipeline(&GraphicsDesc {
                label: "remaster bounced light",
                vertex: shader(Entry::FullscreenVsMain),
                fragment: Some(shader(e)),
                color: &[ColorTarget {
                    format: GI,
                    blend: None,
                    write_alpha: true,
                }],
                depth: None,
                cull: Cull::None,
            })
        };
        let trace = if gfx.caps().ray_queries {
            Some(gfx.create_compute_pipeline(&ComputeDesc {
                label: "remaster bounced light rays",
                shader: shader(Entry::GiTraceCsTrace),
            })?)
        } else {
            None
        };
        Ok(Self {
            settings: GiSettings::default(),
            update: gfx.create_compute_pipeline(&ComputeDesc {
                label: "remaster bounced light probes",
                shader: shader(Entry::GiCsUpdate),
            })?,
            probes_pipeline: fragment(gfx, Entry::GiFsProbes)?,
            screen: fragment(gfx, Entry::GiFsScreen)?,
            blur: fragment(gfx, Entry::GiFsBlur)?,
            trace,
            constants: None,
            buffers: None,
            targets: None,
            rooms: BTreeMap::new(),
            serial: 0,
            frame: 0,
        })
    }

    /// How the bounced light is found from the next frame on.
    pub fn set_settings(&mut self, settings: GiSettings) {
        self.settings = settings;
    }

    /// How the bounced light is found.
    #[must_use]
    pub fn settings(&self) -> GiSettings {
        self.settings
    }

    /// Whether the pass traces rays against the scene this frame: on, asked to, and the device
    /// can.
    #[must_use]
    pub fn wants_rays(&self) -> bool {
        self.settings.on && self.settings.rays && self.trace.is_some()
    }

    /// Let go of the targets and the probes, which nothing reads while the pass is off.
    fn release(&mut self, gfx: &mut Device) {
        if let Some(t) = self.targets.take() {
            for x in [t.gi, t.raw, t.across] {
                gfx.destroy_texture(x);
            }
        }
        if let Some(b) = self.buffers.take() {
            gfx.destroy_buffer(b.probes);
            gfx.destroy_buffer(b.rays);
        }
        if let Some(c) = self.constants.take() {
            gfx.destroy_buffer(c.buffer);
        }
        self.rooms.clear();
    }

    fn targets_for(
        &mut self,
        gfx: &mut Device,
        t: &Targets,
    ) -> Result<GiTargets, dereth_gfx::Error> {
        if let Some(a) = self.targets {
            if (a.width, a.height) == (t.width, t.height) {
                return Ok(a);
            }
            for x in [a.gi, a.raw, a.across] {
                gfx.destroy_texture(x);
            }
        }
        let usage = TextureUsage::COLOR_TARGET | TextureUsage::SAMPLED | TextureUsage::TRANSFER_SRC;
        let mut make = || {
            gfx.create_texture(&TextureDesc {
                label: "remaster bounced light",
                width: t.width,
                height: t.height,
                mips: 1,
                format: GI,
                usage,
            })
        };
        let (gi, raw, across) = (make()?, make()?, make()?);
        let index = [
            gfx.bind_texture(gi)?,
            gfx.bind_texture(raw)?,
            gfx.bind_texture(across)?,
        ];
        let a = GiTargets {
            width: t.width,
            height: t.height,
            gi,
            raw,
            across,
            index,
        };
        self.targets = Some(a);
        Ok(a)
    }

    fn buffers(&mut self, gfx: &mut Device) -> Result<ProbeBuffers, dereth_gfx::Error> {
        if let Some(b) = self.buffers {
            return Ok(b);
        }
        let probes = gfx.create_buffer(&BufferDesc {
            label: "remaster bounced light probes",
            size: u64::from(PROBES) * PROBE_BYTES,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_DST,
            location: Location::Gpu,
        })?;
        let rays = gfx.create_buffer(&BufferDesc {
            label: "remaster bounced light rays",
            size: u64::from(PROBES) * u64::from(RAYS) * 16,
            usage: BufferUsage::STORAGE,
            location: Location::Gpu,
        })?;
        // Every probe starts with nothing gathered.
        gfx.write_buffer(
            probes,
            0,
            &vec![0u8; usize::try_from(u64::from(PROBES) * PROBE_BYTES).unwrap_or(0)],
        )?;
        let b = ProbeBuffers {
            probes,
            rays,
            index: [gfx.bind_buffer(probes)?, gfx.bind_buffer(rays)?],
        };
        self.buffers = Some(b);
        Ok(b)
    }

    /// The frame's constants, and the plan its passes are recorded from; `None` when the pass is
    /// off. `rt` is the scene's acceleration structures, when the frame built them.
    ///
    /// # Errors
    /// What the device refuses.
    pub fn prepare(
        &mut self,
        gfx: &mut Device,
        frame: &FrameDelta,
        scene: &GpuScene,
        t: &Targets,
        rt: Option<RtPlan>,
    ) -> Result<Option<GiPlan>, dereth_gfx::Error> {
        if !self.settings.on {
            if self.targets.is_some() || self.buffers.is_some() {
                self.release(gfx);
            }
            return Ok(None);
        }
        let targets = self.targets_for(gfx, t)?;
        let probes = match (self.trace, rt, self.settings.rays) {
            (Some(trace), Some(rt), true) => Some((trace, rt, self.buffers(gfx)?)),
            _ => None,
        };
        self.frame = self.frame.wrapping_add(1);
        let rooms = if probes.is_some() {
            self.rooms(frame, scene)
        } else {
            Vec::new()
        };
        let rows = self.rows(frame, probes.is_some(), &rooms);
        let constants = match self.constants.as_mut() {
            Some(c) => c,
            None => self.constants.insert(PerFrame::new(
                gfx,
                "remaster bounced light constants",
                2048,
                BufferUsage::default(),
            )?),
        };
        constants.write(gfx, &rows.bytes())?;
        let constants = constants.index;
        Ok(Some(GiPlan {
            targets,
            constants,
            probes: probes.map(|(trace, rt, b)| (trace, self.update, self.probes_pipeline, b, rt)),
            screen: (self.screen, self.blur),
            settings: self.settings,
        }))
    }

    /// The rooms near the eye that get probes of their own, nearest first: each one's first
    /// probe's place (the frame's space), its size in probes, its spacing, its first probe and
    /// its serial. A room keeps its probes while it stays near; one new to them starts afresh.
    fn rooms(
        &mut self,
        frame: &FrameDelta,
        scene: &GpuScene,
    ) -> Vec<(Vec3, [i32; 3], f32, u32, u32)> {
        let eye = frame.camera.eye;
        let bounds: BTreeMap<u32, ([f32; 3], [f32; 3])> = scene
            .meshes
            .values()
            .map(|m| (m.vertices.start, m.bounds))
            .collect();
        let mut near: Vec<(f32, u32, Vec3, Vec3)> = Vec::new();
        for (id, block) in &scene.blocks {
            let (x, y) = (i32::from(id >> 8), i32::from(id & 0xFF));
            #[allow(clippy::cast_precision_loss)] // a few hundred blocks
            let offset = Vec3::new(
                (x - frame.origin[0]) as f32 * 192.0,
                (y - frame.origin[1]) as f32 * 192.0,
                0.0,
            );
            for room in &block.rooms {
                let Some(first) = room.walls.levels.first().and_then(|r| {
                    scene
                        .item_draws
                        .get(r.start as usize)
                        .filter(|_| !r.is_empty())
                }) else {
                    continue;
                };
                let Some((lo, hi)) = bounds.get(&first.vertex_base) else {
                    continue;
                };
                let (mut a, mut b) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for k in 0..8 {
                    let c = Vec3::new(
                        if k & 1 == 0 { lo[0] } else { hi[0] },
                        if k & 2 == 0 { lo[1] } else { hi[1] },
                        if k & 4 == 0 { lo[2] } else { hi[2] },
                    );
                    let w = room.walls.transform.transform_point3(c) + offset;
                    a = a.min(w);
                    b = b.max(w);
                }
                let d = eye.clamp(a, b).distance(eye);
                if d < 30.0 {
                    near.push((d, room.id, a, b));
                }
            }
        }
        near.sort_by(|x, y| x.0.total_cmp(&y.0));
        near.truncate(ROOMS);
        // Rooms no longer near give their slots back; new ones take free slots.
        let keep: std::collections::BTreeSet<u32> = near.iter().map(|r| r.1).collect();
        self.rooms.retain(|id, _| keep.contains(id));
        let mut out = Vec::new();
        for (_, id, a, b) in near {
            let slot = if let Some(s) = self.rooms.get(&id) {
                *s
            } else {
                let used: std::collections::BTreeSet<usize> =
                    self.rooms.values().map(|s| s.slot).collect();
                let Some(free) = (0..ROOMS).find(|s| !used.contains(s)) else {
                    continue;
                };
                self.serial = (self.serial + 1) & 0x7FFF;
                let s = RoomSlot {
                    slot: free,
                    serial: self.serial,
                };
                self.rooms.insert(id, s);
                s
            };
            let (size, spacing) = room_grid(b - a);
            #[allow(clippy::cast_possible_truncation)] // sixteen slots
            let first = OUTDOOR + slot.slot as u32 * ROOM_PROBES;
            out.push((a, size, spacing, first, slot.serial));
        }
        out
    }

    /// The constants' rows (`gi.slang` names them).
    fn rows(
        &self,
        frame: &FrameDelta,
        probes: bool,
        rooms: &[(Vec3, [i32; 3], f32, u32, u32)],
    ) -> Rows {
        let s = self.settings;
        let mut r = Rows::default();
        let cell = |v: f32, spacing: f32| dereth_primitives::num::floor_to_i32(v / spacing);
        let origin = [frame.origin[0] * 48, frame.origin[1] * 48];
        let eye = frame.camera.eye;
        let world = [
            cell(eye.x, SPACING[0]) + origin[0],
            cell(eye.y, SPACING[0]) + origin[1],
            cell(eye.z, SPACING[1]),
        ];
        let first = [
            world[0] - GRID[0] / 2,
            world[1] - GRID[1] / 2,
            world[2] - GRID[2] / 2 + 1,
        ];
        let int = |i: i32| u32::from_ne_bytes(i.to_ne_bytes());
        r.0.push([int(first[0]), int(first[1]), int(first[2]), OUTDOOR]);
        r.0.push([int(GRID[0]), int(GRID[1]), int(GRID[2]), RAYS]);
        r.0.push(
            [
                SPACING[0],
                SPACING[1],
                s.hysteresis.clamp(0.0, 0.999),
                s.strength.max(0.0),
            ]
            .map(f32::to_bits),
        );
        #[allow(clippy::cast_precision_loss)] // the frame's serial, for its turn
        r.0.push([
            int(origin[0]),
            int(origin[1]),
            (self.frame as f32).to_bits(),
            if probes { 1.0f32 } else { 0.0 }.to_bits(),
        ]);
        let turn = turn(self.frame);
        for c in [turn.row(0), turn.row(1), turn.row(2)] {
            r.0.push([c.x, c.y, c.z, 0.0].map(f32::to_bits));
        }
        #[allow(clippy::cast_possible_truncation)] // sixteen rooms
        r.0.push([
            s.bounce.clamp(0.0, 1.0).to_bits(),
            FAR.to_bits(),
            rooms.len() as u32,
            0,
        ]);
        for (a, size, spacing, first, serial) in rooms {
            r.0.push([a.x, a.y, a.z, *spacing].map(f32::to_bits));
            r.0.push([
                int(size[0]),
                int(size[1]),
                int(size[2]),
                first | (serial << 16),
            ]);
        }
        r
    }
}

/// A room's grid: its size in probes, at most [`ROOM_PROBES`], a probe every
/// [`ROOM_SPACING`] or wider to fit, over `extent`.
#[must_use]
pub fn room_grid(extent: Vec3) -> ([i32; 3], f32) {
    let mut spacing = ROOM_SPACING;
    loop {
        let n = |e: f32| (dereth_primitives::num::to_i32((e / spacing).ceil()) + 1).max(2);
        let size = [n(extent.x), n(extent.y), n(extent.z)];
        #[allow(clippy::cast_sign_loss)] // positive sizes
        let count = (size[0] * size[1] * size[2]) as u32;
        if count <= ROOM_PROBES {
            return (size, spacing);
        }
        spacing *= 1.25;
    }
}

/// The rays' turn for frame `n`: a rotation that changes from frame to frame, the same for the
/// same frame.
#[must_use]
pub fn turn(n: u32) -> Mat3 {
    let h = |k: u32| {
        let mut x = n.wrapping_mul(0x9E37_79B9) ^ k.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 15;
        x = x.wrapping_mul(0x2C1B_3C6D);
        x ^= x >> 12;
        #[allow(clippy::cast_precision_loss)] // a fraction
        let f = (x & 0xFF_FFFF) as f32 / 16_777_216.0;
        f
    };
    let axis =
        Vec3::new(h(1) * 2.0 - 1.0, h(2) * 2.0 - 1.0, h(3) * 2.0 - 1.0).normalize_or(Vec3::Z);
    Mat3::from_axis_angle(axis, h(4) * std::f32::consts::TAU)
}

impl GiPlan {
    /// The bounced light's index in the table and its target, which the resolve adds.
    #[must_use]
    pub fn mask(&self) -> (u32, Texture) {
        (self.targets.index[0], self.targets.gi)
    }

    /// Record the pass. `frame` is the frame's constants' index; `outdoor_depth` the depth the
    /// outdoors are placed by; `sun_visibility` the shadows' mask, when drawn; `pools` the
    /// scene's indices, vertices, materials, instances, ground vertices and ground cells, the
    /// lights and the light sets, and the sampler, by their index in the table.
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
        pools: [u32; 9],
    ) -> Result<(), dereth_gfx::Error> {
        let g = self.targets;
        let full = Rect {
            x: 0,
            y: 0,
            width: g.width,
            height: g.height,
        };
        let mut gbuffer = vec![
            Touch::Texture(t.albedo, Use::Sampled),
            Touch::Texture(t.normal, Use::Sampled),
            Touch::Texture(t.light, Use::Sampled),
            Touch::Texture(t.material, Use::Sampled),
            Touch::Texture(t.depth, Use::Sampled),
            Touch::Texture(outdoor_depth.0, Use::Sampled),
        ];
        if let Some(s) = sun_visibility.1 {
            gbuffer.push(Touch::Texture(s, Use::Sampled));
        }
        r.timestamp("bounced light");
        let fullscreen = |r: &mut Recorder<'_>,
                          target: Texture,
                          p: Pipeline,
                          words: Vec<u8>,
                          touches: &[Touch]|
         -> Result<(), dereth_gfx::Error> {
            r.raster(
                &RasterPass {
                    name: "bounced light",
                    color: &[ColorAttachment {
                        texture: target,
                        load: Load::Discard,
                    }],
                    depth: None,
                    touches,
                    area: Some(full),
                },
                |d| {
                    d.set_pipeline(p)?;
                    d.push(0, &words)?;
                    d.draw(3, 1, 0, 0);
                    Ok(())
                },
            )
        };
        if let Some((trace, update, apply, b, rt)) = self.probes {
            let [indices, vertices, materials, instances, ground_vertices, ground_cells, lights, sets, sampler] =
                pools;
            r.compute(
                &[
                    Touch::Buffer(b.rays, Use::StorageWrite),
                    Touch::Buffer(b.probes, Use::StorageRead),
                ],
                |c| {
                    c.set_pipeline(trace)?;
                    c.push(
                        0,
                        &push_words(&[
                            frame,
                            self.constants,
                            b.index[0],
                            b.index[1],
                            PROBES,
                            rt.index,
                            rt.static_rows,
                            rt.object_rows,
                            indices,
                            vertices,
                            materials,
                            sampler,
                            instances,
                            ground_vertices,
                            ground_cells,
                            lights,
                            sets,
                            rt.static_entries,
                            rt.object_entries,
                            rt.statics,
                        ]),
                    )?;
                    c.dispatch(PROBES, 1, 1);
                    Ok(())
                },
            )?;
            r.compute(
                &[
                    Touch::Buffer(b.rays, Use::StorageRead),
                    Touch::Buffer(b.probes, Use::StorageWrite),
                ],
                |c| {
                    c.set_pipeline(update)?;
                    c.push(
                        0,
                        &push_words(&[self.constants, b.index[0], b.index[1], PROBES]),
                    )?;
                    c.dispatch(PROBES, 1, 1);
                    Ok(())
                },
            )?;
            let mut touches = gbuffer.clone();
            touches.push(Touch::Buffer(b.probes, Use::StorageRead));
            fullscreen(
                r,
                g.gi,
                apply,
                push_words(&[
                    frame,
                    self.constants,
                    b.index[0],
                    t.index[1],
                    t.index[3],
                    t.index[4],
                    outdoor_depth.1,
                ]),
                &touches,
            )?;
            return Ok(());
        }
        let (screen, blur) = self.screen;
        let s = self.settings;
        let mut words = push_words(&[
            frame,
            self.constants,
            t.index[0],
            t.index[1],
            t.index[2],
            t.index[3],
            t.index[4],
            outdoor_depth.1,
            sun_visibility.0,
            s.screen_rays.clamp(1, 32),
        ]);
        words.extend(s.screen_reach.max(0.5).to_le_bytes());
        fullscreen(r, g.raw, screen, words, &gbuffer)?;
        let mut touches = gbuffer.clone();
        touches.push(Touch::Texture(g.raw, Use::Sampled));
        fullscreen(
            r,
            g.across,
            blur,
            push_words(&[frame, g.index[1], t.index[1], t.index[4], t.index[3], 1]),
            &touches,
        )?;
        let mut touches = gbuffer;
        touches.push(Touch::Texture(g.across, Use::Sampled));
        fullscreen(
            r,
            g.gi,
            blur,
            push_words(&[frame, g.index[2], t.index[1], t.index[4], t.index[3], 0]),
            &touches,
        )
    }
}

impl super::Renderer {
    /// How the bounced light is found, from the next frame on.
    pub fn set_gi(&mut self, settings: GiSettings) {
        self.gi.set_settings(settings);
    }

    /// How the bounced light is found.
    #[must_use]
    pub fn gi(&self) -> GiSettings {
        self.gi.settings()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A room's grid holds at most its share of probes, a probe every two meters where they fit
    /// and wider where they do not, and at least two a side.
    #[test]
    fn a_rooms_grid_fits_its_share_of_probes() {
        let (small, s) = room_grid(Vec3::new(6.0, 4.0, 3.0));
        assert_eq!((small, s), ([4, 3, 3], 2.0));
        let (big, s) = room_grid(Vec3::new(40.0, 40.0, 10.0));
        assert!(big.iter().product::<i32>() <= 256 && s > 2.0, "{big:?} {s}");
        let (flat, _) = room_grid(Vec3::ZERO);
        assert_eq!(flat, [2, 2, 2]);
    }

    /// The rays' turn is a rotation, the same for the same frame and another for the next.
    #[test]
    fn the_rays_turn_is_a_rotation_of_its_frame() {
        let a = turn(7);
        assert!((a.determinant() - 1.0).abs() < 1e-4);
        assert!((a * a.transpose() - Mat3::IDENTITY).abs_diff_eq(Mat3::ZERO, 1e-4));
        assert_eq!(turn(7), a);
        assert_ne!(turn(8), a);
    }
}
